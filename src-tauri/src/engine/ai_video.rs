//! AI超解像つきのアップコンバート本体(2026-09-24新設)。ストリーミング・分割・再開可能。
//!
//! ## 旧実装との違い(なぜ作り直したか)
//! 旧実装は「全コマをPNGに展開 → 100コマずつ超解像」で、映画1本(約17万コマ)では展開だけで約100GBになるため、
//! `MAX_FRAMES=2400`(約100秒)の上限を設けていた。DVD→ブルーレイの変換で映画全体を扱うには、この上限を外す必要がある。
//!
//! ## 流れ
//! 1. **解析**: インターレース/テレシネの判定(`idet`)、黒帯の検出(`cropdetect`)、コマ数・尺の見積もり。
//! 2. **デコード**: ffmpegが前処理(インターレース解除・黒帯切り取り・色行列変換)をして、生のRGBのコマを標準出力へ流す。
//!    Rust側が1コマずつ読む(全コマをディスクへ展開しない。上限つきの待ち行列で、デコードが先走らない)。
//! 3. **静止コマの省略**: 単色(真っ黒・真っ白)のコマと、直前に処理したコマとほぼ同じコマは、AIを通さず再利用する。
//! 4. **超解像**: `sr_pool`(CPU・GPUを同時に使う)で処理。
//! 5. **エンコード**: 240コマごとに中間ファイル(x264、CRF14)へ確定。**途中で止まっても、確定済みの分から再開できる**。
//! 6. 中間ファイルを結合して元の音声を付け、通常の変換(容量に合わせた最終エンコード)へ渡す。
//!
//! ## 正直な開示
//! - 静止コマの省略が減らすのは主に**計算時間**。エンコーダは静止画面にほとんどビットを使わないので、容量への効果は小さい。
//! - 黒帯の自動検出は暗い場面で誤ることがある(画面上で切り替えられる)。テレシネ判定も推定なので、手動指定を用意している。

use crate::engine::ai_upscale::{self, AiUpscale};
use crate::engine::hw_bench;
use crate::engine::sidecar::resolve_tool;
use crate::engine::sr_pool::{self, Job, Mode, PoolConfig, SrPool, SrSpec};
use crate::progress;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// 中間ファイル1個あたりのコマ数(24fpsで10秒)。再開の粒度になる。
pub const CHUNK_FRAMES: u64 = 240;
/// 中間ファイルの画質(x264のCRF。最終エンコードで容量に合わせて再圧縮するので、高画質にしておく)。
const MEZZANINE_CRF: &str = "14";

// ── 入力の解析 ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub w: u32,
    pub h: u32,
    /// 平均フレームレート(分子, 分母)。
    pub fps: (u64, u64),
    pub duration: f64,
    /// ピクセルのアスペクト比(SAR)。
    pub sar: (u32, u32),
}

fn parse_ratio(s: &str) -> Option<(u64, u64)> {
    let (a, b) = s.split_once(|c| c == '/' || c == ':')?;
    let (a, b): (u64, u64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    (a > 0 && b > 0).then_some((a, b))
}

pub fn probe_video(input: &str) -> Result<VideoInfo, String> {
    let out = resolve_tool("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,avg_frame_rate,r_frame_rate,sample_aspect_ratio,duration:format=duration", "-of", "json", input])
        .output()
        .map_err(|e| format!("ffprobeの起動に失敗しました: {e}"))?;
    if !out.status.success() {
        return Err(format!("ffprobeが失敗しました: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("ffprobeの出力を読めません: {e}"))?;
    let st = v["streams"].get(0).ok_or("映像ストリームが見つかりません / no video stream")?;
    let (w, h) = (st["width"].as_u64().unwrap_or(0) as u32, st["height"].as_u64().unwrap_or(0) as u32);
    if w == 0 || h == 0 {
        return Err("映像の大きさを取得できません".to_string());
    }
    let fps = st["avg_frame_rate"].as_str().and_then(parse_ratio).or_else(|| st["r_frame_rate"].as_str().and_then(parse_ratio)).ok_or("フレームレートを取得できません")?;
    let sar = st["sample_aspect_ratio"].as_str().and_then(parse_ratio).map(|(a, b)| (a as u32, b as u32)).unwrap_or((1, 1));
    let duration = v["format"]["duration"].as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| st["duration"].as_str().and_then(|s| s.parse().ok())).unwrap_or(0.0);
    Ok(VideoInfo { w, h, fps, duration, sar })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Interlace {
    Progressive,
    Interlaced,
    /// 24pの映画を60iに変換(3:2プルダウン)したもの。逆テレシネ(IVTC)が必要。
    Telecine,
}

fn num_after(line: &str, key: &str) -> Option<u64> {
    let rest = &line[line.find(key)? + key.len()..];
    let digits: String = rest.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// `idet`の要約(標準エラー出力)から (インターレース, プログレッシブ) のコマ数を読む。複数の窓があれば合計する。
pub fn parse_idet(stderr: &str) -> Option<(u64, u64)> {
    let (mut il, mut pr, mut found) = (0u64, 0u64, false);
    for line in stderr.lines().filter(|l| l.contains("Multi frame detection")) {
        let (tff, bff, prog) = (num_after(line, "TFF:")?, num_after(line, "BFF:")?, num_after(line, "Progressive:")?);
        il += tff + bff;
        pr += prog;
        found = true;
    }
    found.then_some((il, pr))
}

/// インターレースのコマの割合から種類を決める。テレシネは5コマ中2コマが縞になるので、割合は約0.4になる。
pub fn classify_interlace(interlaced: u64, progressive: u64) -> (Interlace, f64) {
    let total = interlaced + progressive;
    if total < 20 {
        return (Interlace::Progressive, 0.0);
    }
    let ratio = interlaced as f64 / total as f64;
    let kind = if ratio >= 0.70 {
        Interlace::Interlaced
    } else if ratio >= 0.10 {
        Interlace::Telecine
    } else {
        Interlace::Progressive
    };
    (kind, ratio)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Crop {
    pub w: u32,
    pub h: u32,
    pub x: u32,
    pub y: u32,
}

/// `cropdetect`の出力の最後の`crop=W:H:X:Y`(そこまでの全コマの和集合)。
pub fn parse_cropdetect_last(stderr: &str) -> Option<Crop> {
    stderr.lines().rev().find_map(|l| {
        let rest = &l[l.find("crop=")? + 5..];
        let mut it = rest.split_whitespace().next()?.split(':').map(|n| n.trim().parse::<u32>());
        Some(Crop { w: it.next()?.ok()?, h: it.next()?.ok()?, x: it.next()?.ok()?, y: it.next()?.ok()? })
    })
}

/// 複数の窓の検出結果の和集合(一番広い範囲)。どこかの場面で映像がある部分は切らない。
pub fn union_crops(crops: &[Crop]) -> Option<Crop> {
    let first = crops.first()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x + first.w, first.y + first.h);
    for c in &crops[1..] {
        x0 = x0.min(c.x);
        y0 = y0.min(c.y);
        x1 = x1.max(c.x + c.w);
        y1 = y1.max(c.y + c.h);
    }
    Some(Crop { w: x1 - x0, h: y1 - y0, x: x0, y: y0 })
}

/// 切り取りを採用してよいか(明らかな黒帯だけ)。採用するなら偶数にそろえて返す。
pub fn accept_crop(c: Crop, w: u32, h: u32) -> Option<Crop> {
    let (x, y) = (c.x & !1, c.y & !1);
    let (cw, ch) = (c.w & !1, c.h & !1);
    if cw < w / 2 || ch < h / 2 || x + cw > w || y + ch > h {
        return None;
    }
    // 画素数が4%以上減る場合だけ(小さな差は誤検出の可能性のほうが高い)。
    if (cw as f64 * ch as f64) > 0.96 * (w as f64 * h as f64) {
        return None;
    }
    // 黒帯は上下(または左右)でほぼ同じ太さのはず。片側だけなら字幕帯などの可能性があるので採用しない。
    let (top, bottom) = (y as i64, (h - y - ch) as i64);
    let (left, right) = (x as i64, (w - x - cw) as i64);
    let balanced = |a: i64, b: i64| (a - b).abs() <= (a.max(b) / 3).max(8);
    (balanced(top, bottom) && balanced(left, right)).then_some(Crop { w: cw, h: ch, x, y })
}

#[derive(Debug, Clone, Serialize)]
pub struct Analysis {
    pub width: u32,
    pub height: u32,
    pub sar_num: u32,
    pub sar_den: u32,
    pub fps: f64,
    pub duration_secs: f64,
    pub interlace: Interlace,
    pub interlace_ratio: f64,
    pub crop: Option<Crop>,
    /// 前処理後のコマの間隔(分子, 分母)。逆テレシネで4/5になる。
    pub out_fps_num: u64,
    pub out_fps_den: u64,
    /// 処理するコマ数の見積もり。
    pub frames: u64,
    pub notes_ja: Vec<String>,
    pub notes_en: Vec<String>,
}

fn ffmpeg_stderr(args: &[String]) -> Result<String, String> {
    let out = resolve_tool("ffmpeg").args(args).output().map_err(|e| format!("ffmpegの起動に失敗しました: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stderr).to_string())
}

/// 範囲`[s0, s0+len]`の中の`fracs`の位置に、長さ`win`秒の窓を作る。
fn windows(s0: f64, len: f64, fracs: &[f64], win: f64) -> Vec<(f64, f64)> {
    let win = win.min(len.max(0.5));
    fracs.iter().map(|f| ((s0 + (len - win).max(0.0) * f).max(0.0), win)).collect()
}

fn detect_interlace(input: &str, s0: f64, len: f64) -> Result<(Interlace, f64), String> {
    let (mut il, mut pr) = (0u64, 0u64);
    for (t, d) in windows(s0, len, &[0.3, 0.7], 15.0) {
        let args: Vec<String> = ["-hide_banner", "-v", "info", "-ss", &t.to_string(), "-t", &d.to_string(), "-i", input, "-an", "-vf", "idet", "-f", "null", "-"].iter().map(|s| s.to_string()).collect();
        if let Some((a, b)) = parse_idet(&ffmpeg_stderr(&args)?) {
            il += a;
            pr += b;
        }
    }
    Ok(classify_interlace(il, pr))
}

fn detect_crop(input: &str, s0: f64, len: f64, v: &VideoInfo) -> Result<Option<Crop>, String> {
    let mut found = Vec::new();
    for (t, d) in windows(s0, len, &[0.15, 0.5, 0.85], 6.0) {
        let args: Vec<String> = ["-hide_banner", "-v", "info", "-ss", &t.to_string(), "-t", &d.to_string(), "-i", input, "-an", "-vf", "cropdetect=limit=24:round=2:reset=0", "-f", "null", "-"].iter().map(|s| s.to_string()).collect();
        if let Some(c) = parse_cropdetect_last(&ffmpeg_stderr(&args)?) {
            found.push(c);
        }
    }
    Ok(union_crops(&found).and_then(|c| accept_crop(c, v.w, v.h)))
}

pub fn analyze(input: &str, start: Option<f64>, dur: Option<f64>, up: &AiUpscale) -> Result<Analysis, String> {
    let v = probe_video(input)?;
    let s0 = start.unwrap_or(0.0);
    let len = dur.unwrap_or((v.duration - s0).max(0.0));
    let (mut ja, mut en) = (Vec::new(), Vec::new());

    let mode = up.deinterlace.as_deref().unwrap_or("auto");
    let (interlace, ratio) = match mode {
        "off" => (Interlace::Progressive, 0.0),
        "interlaced" => (Interlace::Interlaced, 1.0),
        "telecine" => (Interlace::Telecine, 0.4),
        _ => detect_interlace(input, s0, len)?,
    };
    match interlace {
        Interlace::Interlaced => {
            ja.push("インターレース映像と判定したので、拡大の前に縞(くし形ノイズ)を解除します。".to_string());
            en.push("Detected interlaced video; deinterlacing before upscaling so the combing is not locked in as detail.".to_string());
        }
        Interlace::Telecine => {
            ja.push("テレシネ(24p映画を60iに変換したもの)と判定したので、逆テレシネで元の24コマに戻してから拡大します。".to_string());
            en.push("Detected telecine (24p film converted to 60i); applying inverse telecine to recover the original 24 fps before upscaling.".to_string());
        }
        Interlace::Progressive => {}
    }

    let crop = if up.crop_bars.unwrap_or(true) { detect_crop(input, s0, len, &v)? } else { None };
    if let Some(c) = crop {
        ja.push(format!("黒帯を検出したので切り取ってAI処理します({}×{} → {}×{})。処理する画素数が減り、速くなります。", v.w, v.h, c.w, c.h));
        en.push(format!("Black bars detected and cropped before AI processing ({}x{} -> {}x{}); fewer pixels to process, so it is faster.", v.w, v.h, c.w, c.h));
    }

    let (n, d) = v.fps;
    let (out_n, out_d) = if interlace == Interlace::Telecine { (n * 4, d * 5) } else { (n, d) };
    let fps = out_n as f64 / out_d as f64;
    Ok(Analysis {
        width: v.w,
        height: v.h,
        sar_num: v.sar.0,
        sar_den: v.sar.1,
        fps,
        duration_secs: len,
        interlace,
        interlace_ratio: ratio,
        crop,
        out_fps_num: out_n,
        out_fps_den: out_d,
        frames: (len * fps).round() as u64,
        notes_ja: ja,
        notes_en: en,
    })
}

// ── フィルタと出力の形 ─────────────────────────────────────

/// デコード側のフィルタ: インターレース解除/逆テレシネ → 黒帯の切り取り → YUVからRGBへ(SDはBT.601、HDはBT.709)。
pub fn decode_filter(interlace: Interlace, crop: Option<Crop>, src_h: u32) -> String {
    let mut parts: Vec<String> = Vec::new();
    match interlace {
        Interlace::Interlaced => parts.push("bwdif=mode=send_frame:parity=auto:deint=all".into()),
        Interlace::Telecine => parts.push("fieldmatch=order=auto:combmatch=full,yadif=mode=send_frame:parity=auto:deint=interlaced,decimate".into()),
        Interlace::Progressive => {}
    }
    if let Some(c) = crop {
        parts.push(format!("crop={}:{}:{}:{}", c.w, c.h, c.x, c.y));
    }
    let matrix = if src_h <= 576 { "bt601" } else { "bt709" };
    parts.push(format!("scale=in_color_matrix={matrix}:flags=accurate_rnd+full_chroma_int"));
    parts.push("format=rgb24".into());
    parts.join(",")
}

/// 表示の縦横比を保って`target`(幅, 高さ)に収める。ほぼ同じ比なら余白なしで埋め、違えば黒帯を付ける。
/// 戻り値: (拡縮後の幅, 高さ, 余白のx, y)。
pub fn fit_geometry(canvas_w: u32, canvas_h: u32, sar: (u32, u32), target: (u32, u32)) -> (u32, u32, u32, u32) {
    let (tw, th) = target;
    let dar = canvas_w as f64 * sar.0.max(1) as f64 / (canvas_h as f64 * sar.1.max(1) as f64);
    let tdar = tw as f64 / th as f64;
    if (dar / tdar - 1.0).abs() <= 0.03 {
        return (tw, th, 0, 0);
    }
    let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
    if dar > tdar {
        let sh = even(tw as f64 / dar).min(th);
        (tw, sh, 0, ((th - sh) / 2) & !1)
    } else {
        let sw = even(th as f64 * dar).min(tw);
        (sw, th, ((tw - sw) / 2) & !1, 0)
    }
}

#[derive(Debug, Clone)]
pub struct Geometry {
    /// 超解像後の1コマの大きさ(エンコーダへ流す生のRGBの大きさ)。
    pub ow: u32,
    pub oh: u32,
    /// 黒帯を戻した後の全体の大きさ。
    pub canvas_w: u32,
    pub canvas_h: u32,
    /// 黒帯を切り取った場合の、全体の中での位置(拡大後)。
    pub pad: Option<(u32, u32)>,
    pub sar: (u32, u32),
    /// 最終的な出力の大きさ(未指定なら拡大後の大きさのまま)。
    pub target: Option<(u32, u32)>,
}

/// 中間ファイルを作るエンコーダ側のフィルタ: 黒帯を戻す → 大きさをそろえる(縦横比を保つ)→ RGBからYUV(BT.709)へ。
pub fn encoder_vf(g: &Geometry) -> String {
    let mut f: Vec<String> = Vec::new();
    if let Some((px, py)) = g.pad {
        f.push(format!("pad={}:{}:{}:{}:black", g.canvas_w, g.canvas_h, px, py));
    }
    let conv = "flags=lanczos+accurate_rnd+full_chroma_int:out_color_matrix=bt709:out_range=tv";
    match g.target {
        Some(t) => {
            let (sw, sh, px, py) = fit_geometry(g.canvas_w, g.canvas_h, g.sar, t);
            f.push(format!("setsar={}/{}", g.sar.0.max(1), g.sar.1.max(1)));
            f.push(format!("scale={sw}:{sh}:{conv}"));
            f.push("format=yuv420p".into());
            if (sw, sh) != t {
                f.push(format!("pad={}:{}:{px}:{py}:black", t.0, t.1));
            }
            f.push("setsar=1".into());
        }
        None => {
            f.push(format!("scale=iw:ih:{conv}"));
            f.push("format=yuv420p".into());
            f.push(format!("setsar={}/{}", g.sar.0.max(1), g.sar.1.max(1)));
        }
    }
    f.join(",")
}

// ── 静止コマの判定 ────────────────────────────────────────

/// 全画素がほぼ同じ色(真っ黒・真っ白など)なら、その色を返す。全画素を調べるので、黒地の小さな文字は見逃さない。
pub fn flat_color(rgb: &[u8]) -> Option<[u8; 3]> {
    if rgb.len() < 3 {
        return None;
    }
    let (mut lo, mut hi) = ([255u8; 3], [0u8; 3]);
    for px in rgb.chunks_exact(3) {
        for c in 0..3 {
            lo[c] = lo[c].min(px[c]);
            hi[c] = hi[c].max(px[c]);
        }
    }
    (0..3).all(|c| hi[c] - lo[c] <= 4).then(|| [((lo[0] as u16 + hi[0] as u16) / 2) as u8, ((lo[1] as u16 + hi[1] as u16) / 2) as u8, ((lo[2] as u16 + hi[2] as u16) / 2) as u8])
}

/// `reference`(直前に処理したコマ)と`cur`がほぼ同じか。平均の差が小さく、大きく違う画素がほとんど無いときだけ真。
pub fn similar_to(reference: &[u8], cur: &[u8]) -> bool {
    if reference.len() != cur.len() || cur.is_empty() {
        return false;
    }
    let (mut sum, mut big) = (0u64, 0u64);
    for (a, b) in reference.iter().zip(cur) {
        let d = (*a as i32 - *b as i32).unsigned_abs() as u64;
        sum += d;
        big += (d > 12) as u64;
    }
    let n = cur.len() as f64;
    sum as f64 / n <= 0.5 && (big as f64 / n) <= 0.0002
}

#[derive(Debug, PartialEq)]
pub enum FrameKind {
    Flat([u8; 3]),
    Same,
    New,
}

pub fn classify(cur: &[u8], reference: Option<&[u8]>, skip_static: bool) -> FrameKind {
    if !skip_static {
        return FrameKind::New;
    }
    if let Some(c) = flat_color(cur) {
        return FrameKind::Flat(c);
    }
    match reference {
        Some(r) if similar_to(r, cur) => FrameKind::Same,
        _ => FrameKind::New,
    }
}

// ── 設定の解決 ────────────────────────────────────────────

/// `scale`が0(自動)のとき、出力の高さが目標以上になる最小の倍率(2〜4)。倍率が大きくても計算量はほぼ変わらない。
pub fn auto_scale(src_h: u32, target_h: Option<u32>) -> u32 {
    match target_h {
        Some(t) => (t as f64 / src_h.max(1) as f64).ceil().clamp(2.0, 4.0) as u32,
        None => 2,
    }
}

/// 使うモデル。`auto`は内容が実写なら汎用モデル(入っていれば)、アニメなら動画向けモデル。
pub fn resolve_model(up: &AiUpscale, models_dir: &Path) -> String {
    let general_ok = models_dir.join("realesr-general-x4v3.param").is_file() && models_dir.join("realesr-general-x4v3.bin").is_file();
    match up.model.as_str() {
        "" | "auto" => {
            if up.content.as_deref() == Some("anime") || !general_ok {
                "realesr-animevideov3".to_string()
            } else {
                "realesr-general-x4v3".to_string()
            }
        }
        m => m.to_string(),
    }
}

fn fnv1a(s: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn file_signature(path: &str) -> String {
    match std::fs::metadata(path) {
        Ok(m) => format!("{}:{}", m.len(), m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())),
        Err(_) => "?".to_string(),
    }
}

// ── 中間ファイルのエンコーダ ───────────────────────────────

struct ChunkEnc {
    child: Child,
    stdin: Option<ChildStdin>,
    part: PathBuf,
    fin: PathBuf,
}

fn chunk_paths(work: &Path, chunk: u64) -> (PathBuf, PathBuf) {
    (work.join(format!("chunk_{chunk:06}.part.mkv")), work.join(format!("chunk_{chunk:06}.mkv")))
}

fn spawn_chunk(work: &Path, chunk: u64, g: &Geometry, fps: (u64, u64)) -> Result<ChunkEnc, String> {
    let (part, fin) = chunk_paths(work, chunk);
    let _ = std::fs::remove_file(&part);
    let mut child = resolve_tool("ffmpeg")
        .args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{}x{}", g.ow, g.oh), "-framerate", &format!("{}/{}", fps.0, fps.1), "-i", "-"])
        .args(["-vf", &encoder_vf(g), "-c:v", "libx264", "-crf", MEZZANINE_CRF, "-preset", "veryfast", "-pix_fmt", "yuv420p", "-profile:v", "high"])
        .args(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv", "-an"])
        .arg(&part)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpegの起動に失敗しました: {e}"))?;
    let stdin = child.stdin.take();
    Ok(ChunkEnc { child, stdin, part, fin })
}

impl ChunkEnc {
    fn write(&mut self, frame: &[u8]) -> Result<(), String> {
        self.stdin.as_mut().ok_or("エンコーダの入力が閉じています")?.write_all(frame).map_err(|e| format!("エンコーダへの書き込みに失敗しました: {e}"))
    }

    /// 入力を閉じて、エンコードの完了を待ち、確定した名前へ改名する。
    fn finish(mut self) -> Result<(), String> {
        drop(self.stdin.take());
        let out = self.child.wait_with_output().map_err(|e| format!("ffmpegの終了待ちに失敗しました: {e}"))?;
        if !out.status.success() {
            let _ = std::fs::remove_file(&self.part);
            return Err(format!("中間ファイルのエンコードに失敗しました: {}", String::from_utf8_lossy(&out.stderr)));
        }
        std::fs::rename(&self.part, &self.fin).map_err(|e| format!("中間ファイルを確定できません: {e}"))
    }

    /// 中止のとき: 書きかけを捨てる。
    fn abort(mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.part);
    }
}

// ── 再開用の記録 ──────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize)]
struct Manifest {
    key: String,
    chunk_frames: u64,
    chunks_done: u64,
}

fn read_manifest(work: &Path) -> Option<Manifest> {
    serde_json::from_slice(&std::fs::read(work.join("manifest.json")).ok()?).ok()
}

fn write_manifest(work: &Path, m: &Manifest) {
    if let Ok(j) = serde_json::to_vec(m) {
        let _ = std::fs::write(work.join("manifest.json"), j);
    }
}

/// 続きから再開できる確定済みの中間ファイルの数。連番が途切れていたら、途切れる手前まで。
fn resumable_chunks(work: &Path, key: &str) -> u64 {
    match read_manifest(work) {
        Some(m) if m.key == key && m.chunk_frames == CHUNK_FRAMES => (0..m.chunks_done).take_while(|c| chunk_paths(work, *c).1.is_file()).count() as u64,
        _ => 0,
    }
}

// ── 本体 ─────────────────────────────────────────────────

enum Slot {
    Frame(Arc<Vec<u8>>),
    Flat([u8; 3]),
    Repeat,
}

#[derive(Default, Clone, Copy)]
struct Stats {
    sr_cpu: u64,
    sr_gpu: u64,
    flat: u64,
    repeat: u64,
    fallback: u64,
}

fn say(msg: impl Into<String>) {
    progress::emit("ai-progress", serde_json::json!({ "stage": "info", "message": msg.into() }));
}

/// 動画を、AI超解像した映像に元の音声を付けた中間ファイル(`mezzanine`)へ変換する。
/// `trim`は(開始秒, 長さ秒)、`target`は最終的な出力の大きさ。
pub fn make_upscaled_mezzanine(input: &str, trim: Option<(Option<f64>, Option<f64>)>, up: &AiUpscale, target: Option<(u32, u32)>, mezzanine: &Path) -> Result<(), String> {
    ai_upscale::validate(up)?;
    let (start, dur) = trim.unwrap_or((None, None));
    let t_all = Instant::now();

    say("映像を解析しています… / Analyzing the video…");
    let an = analyze(input, start, dur, up)?;
    for n in an.notes_ja.iter().zip(&an.notes_en) {
        say(format!("{} / {}", n.0, n.1));
    }
    if an.frames == 0 {
        return Err("処理するコマがありません / no frames to process".to_string());
    }

    let exe = ai_upscale::ensure_plugin()?;
    let models = exe.parent().ok_or("プラグインの場所が不正です")?.join("models");
    let model = resolve_model(up, &models);
    let (cw, ch) = an.crop.map_or((an.width, an.height), |c| (c.w, c.h));
    let scale = if up.scale == 0 { auto_scale(ch, target.map(|t| t.1)) } else { up.scale };
    let spec = SrSpec { model: model.clone(), scale };

    // このPCで一番速い方式を選ぶ(初回だけ実測する)。CPUを明示したときは実測を省く(保存済みの結果があれば見積もりに使う)。
    let bench = if up.backend == "cpu" { hw_bench::cached() } else { Some(hw_bench::benchmark(false, &|m| say(m))?) };
    if let Some(b) = &bench {
        say(format!("{} / {}", b.message_ja, b.message_en));
    }
    let gpu_index = bench.as_ref().and_then(|b| b.gpu_index);
    let mut mode = match up.backend.as_str() {
        "cpu" => Mode::Cpu,
        "gpu" => Mode::Gpu(gpu_index.unwrap_or(0)),
        "hybrid" => Mode::Hybrid(gpu_index.unwrap_or(0)),
        _ => bench.as_ref().map(hw_bench::mode_of).unwrap_or(Mode::Cpu),
    };
    if model == "realesrgan-x4plus" {
        // 高品質モデルはCPU版に無い。GPUだけで処理する。
        if up.backend == "cpu" {
            return Err("realesrgan-x4plusはGPUが必要です(CPU指定では使えません) / realesrgan-x4plus needs a GPU".to_string());
        }
        mode = Mode::Gpu(gpu_index.ok_or("realesrgan-x4plusにはGPUが必要ですが、使えるGPUが見つかりません")?);
    }
    if matches!(mode, Mode::Gpu(_) | Mode::Hybrid(_)) && gpu_index.is_none() && up.backend != "auto" && !up.backend.is_empty() {
        return Err("GPUが指定されましたが、使えるGPUが見つかりません / a GPU was requested but none is usable".to_string());
    }
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let (ow, oh) = ((cw as f64 * scale as f64).round() as u32, (ch as f64 * scale as f64).round() as u32);
    let (canvas_w, canvas_h, pad) = match an.crop {
        Some(c) => ((an.width as f64 * scale as f64).round() as u32, (an.height as f64 * scale as f64).round() as u32, Some(((c.x as f64 * scale as f64).round() as u32, (c.y as f64 * scale as f64).round() as u32))),
        None => (ow, oh, None),
    };
    let geo = Geometry { ow, oh, canvas_w, canvas_h, pad, sar: (an.sar_num, an.sar_den), target };
    let fps = (an.out_fps_num, an.out_fps_den);

    let eta = bench.as_ref().map_or(f64::NAN, |b| hw_bench::secs_per_frame(b, cw as usize, ch as usize) * an.frames as f64);
    say(format!(
        "AI超解像: {}コマ({}×{} → {}×{}、モデル {}、{})。最悪の場合の所要時間の目安 {}(単色・静止コマは省略されるので、実際はこれより短くなります)。 / AI upscaling: {} frames ({}x{} -> {}x{}, model {}). Worst-case estimate {}.",
        an.frames, cw, ch, ow, oh, model,
        match mode { Mode::Cpu => "CPU", Mode::Gpu(_) => "GPU", Mode::Hybrid(_) => "CPU+GPU" },
        if eta.is_finite() { progress::fmt_duration(eta) } else { "不明(未測定)".to_string() },
        an.frames, cw, ch, ow, oh, model, if eta.is_finite() { progress::fmt_duration(eta) } else { "unknown (not measured)".to_string() }
    ));

    // 作業フォルダ(入力・設定が同じなら、前回の途中経過から再開する)。
    let key = format!(
        "{:016x}",
        fnv1a(&format!("{}|{}|{:?}|{:?}|{}|{}|{:?}|{:?}|{:?}|{}|{:?}", input, file_signature(input), start, dur, model, scale, an.interlace, an.crop, target, up.skip_static.unwrap_or(true), (an.out_fps_num, an.out_fps_den)))
    );
    let work = mezzanine.parent().unwrap_or(Path::new(".")).join(format!(".make-disk-ai-{key}"));
    let resume = resumable_chunks(&work, &key);
    if resume == 0 {
        let _ = std::fs::remove_dir_all(&work);
    }
    std::fs::create_dir_all(&work).map_err(|e| format!("作業フォルダを作成できません: {e}"))?;
    if resume > 0 {
        say(format!("前回の途中経過から再開します({}コマ済み)。 / Resuming from the previous progress ({} frames done).", resume * CHUNK_FRAMES, resume * CHUNK_FRAMES));
    }
    write_manifest(&work, &Manifest { key: key.clone(), chunk_frames: CHUNK_FRAMES, chunks_done: resume });

    let cfg = PoolConfig {
        spec,
        mode,
        cli: matches!(mode, Mode::Gpu(_) | Mode::Hybrid(_)).then(|| exe.clone()),
        models_dir: models,
        work_dir: work.clone(),
        gpu_batch: bench.as_ref().map_or(4, |b| b.gpu_batch),
        queue_cap: (bench.as_ref().map_or(4, |b| b.gpu_batch) * 2).max(8),
        gpu_share: bench.as_ref().map_or(0.5, |b| b.gpu_share),
        cpu_threads: if matches!(mode, Mode::Hybrid(_)) { cores.saturating_sub(2).max(1) } else { 0 },
    };
    let pool = SrPool::start(cfg)?;
    let submitter = pool.submitter();

    let total = Arc::new(AtomicU64::new(u64::MAX));
    let (direct_tx, direct_rx) = mpsc::channel::<(u64, Slot)>();

    // 回収スレッド: 結果を順番に並べ直し、中間ファイルへ流す。
    let collector = {
        let (total, work, geo, key) = (total.clone(), work.clone(), geo.clone(), key.clone());
        let frames_est = an.frames;
        std::thread::spawn(move || collect(pool, direct_rx, total, work, geo, fps, key, resume, frames_est))
    };

    // デコード(このスレッド)。
    let decode_result = decode_and_submit(input, start, dur, &an, &decode_filter(an.interlace, an.crop, an.height), (cw, ch), resume * CHUNK_FRAMES, up.skip_static.unwrap_or(true), &submitter, &direct_tx, &total);
    submitter.close();
    drop(direct_tx);
    let collected = collector.join().map_err(|_| "回収スレッドが異常終了しました".to_string())?;
    decode_result?;
    let stats = collected?;

    say(format!(
        "超解像が完了しました({}、AI処理 CPU {} + GPU {}コマ、単色 {}、再利用 {}コマ)。 / Upscaling done (AI: CPU {} + GPU {} frames, flat {}, reused {}).",
        progress::fmt_duration(t_all.elapsed().as_secs_f64()), stats.sr_cpu, stats.sr_gpu, stats.flat, stats.repeat, stats.sr_cpu, stats.sr_gpu, stats.flat, stats.repeat
    ));

    // 中間ファイルを結合し、元の音声(同じ範囲)を付ける。
    let total_frames = total.load(Ordering::SeqCst);
    let n_chunks = total_frames.div_ceil(CHUNK_FRAMES);
    let list: String = (0..n_chunks).map(|c| format!("file '{}'\n", chunk_paths(&work, c).1.to_string_lossy().replace('\\', "/").replace('\'', "'\\''"))).collect();
    let list_path = work.join("list.txt");
    std::fs::write(&list_path, list).map_err(|e| e.to_string())?;
    let mut mux = resolve_tool("ffmpeg");
    mux.args(["-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", &list_path.to_string_lossy()]);
    if let Some(s) = start {
        mux.args(["-ss", &s.to_string()]);
    }
    if let Some(d) = dur {
        mux.args(["-t", &d.to_string()]);
    }
    mux.args(["-i", input, "-map", "0:v", "-map", "1:a?", "-c", "copy", "-shortest"]).arg(mezzanine);
    let res = mux.output().map_err(|e| format!("ffmpegの起動に失敗しました: {e}"))?;
    if !res.status.success() {
        return Err(format!("中間ファイルの結合に失敗しました: {}", String::from_utf8_lossy(&res.stderr)));
    }
    let _ = std::fs::remove_dir_all(&work);

    // フレーム補間(RIFE): 目標fpsが元より十分高いときだけ。
    let out_fps_f = fps.0 as f64 / fps.1 as f64;
    if let Some(f) = crate::engine::rife::factor_for(out_fps_f, up.target_fps) {
        say(format!(
            "フレーム補間(RIFE)を行います: {:.3}fps → {:.3}fps({}倍)。 / Interpolating frames with RIFE: {:.3} fps -> {:.3} fps (x{}).",
            out_fps_f, out_fps_f * f as f64, f, out_fps_f, out_fps_f * f as f64, f
        ));
        crate::engine::rife::interpolate_in_place(mezzanine, f, gpu_index)?;
    }
    Ok(())
}

/// 超解像をせず、フレーム補間(RIFE)だけで目標のfpsにした中間ファイルを作る。
/// 元の大きさのまま、色行列だけ(SDならBT.601→709)を整えてx264 CRF14へ書き出し、RIFEで補間する。
pub fn make_interpolated_mezzanine(input: &str, trim: Option<(Option<f64>, Option<f64>)>, up: &AiUpscale, mezzanine: &Path) -> Result<(), String> {
    ai_upscale::validate(up)?;
    let (start, dur) = trim.unwrap_or((None, None));
    let info = probe_video(input)?;
    let src_fps = info.fps.0 as f64 / info.fps.1 as f64;
    let factor = crate::engine::rife::factor_for(src_fps, up.target_fps).ok_or_else(|| format!("元のfps({src_fps:.2})に対して目標のfpsが低すぎるため、補間は行いません / target fps is too low relative to the source ({src_fps:.2})"))?;
    say(format!(
        "フレーム補間(RIFE)だけを行います: {:.3}fps → {:.3}fps({}倍、超解像なし)。 / Interpolation only (no upscaling): {:.3} fps -> {:.3} fps (x{}).",
        src_fps, src_fps * factor as f64, factor, src_fps, src_fps * factor as f64, factor
    ));
    let vf = if info.h <= 576 {
        "scale=in_color_matrix=bt601:out_color_matrix=bt709:out_range=tv,format=yuv420p"
    } else {
        "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p"
    };
    let mut cmd = resolve_tool("ffmpeg");
    cmd.args(["-v", "error", "-y"]);
    if let Some(s) = start {
        cmd.args(["-ss", &s.to_string()]);
    }
    if let Some(d) = dur {
        cmd.args(["-t", &d.to_string()]);
    }
    cmd.args(["-i", input, "-map", "0:v:0", "-map", "0:a?", "-vf", vf, "-c:v", "libx264", "-crf", MEZZANINE_CRF, "-preset", "veryfast", "-profile:v", "high"]);
    cmd.args(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv", "-c:a", "copy"]).arg(mezzanine);
    let out = cmd.output().map_err(|e| format!("ffmpegの起動に失敗しました: {e}"))?;
    if !out.status.success() {
        return Err(format!("中間ファイルの作成に失敗しました: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let gpu = hw_bench::cached().and_then(|b| b.gpu_index);
    crate::engine::rife::interpolate_in_place(mezzanine, factor, gpu)
}

#[allow(clippy::too_many_arguments)]
fn decode_and_submit(
    input: &str,
    start: Option<f64>,
    dur: Option<f64>,
    an: &Analysis,
    vf: &str,
    (cw, ch): (u32, u32),
    skip: u64,
    skip_static: bool,
    submitter: &sr_pool::Submitter,
    direct: &mpsc::Sender<(u64, Slot)>,
    total: &AtomicU64,
) -> Result<(), String> {
    let _ = an;
    let mut cmd = resolve_tool("ffmpeg");
    cmd.args(["-v", "error", "-nostdin"]);
    if let Some(s) = start {
        cmd.args(["-ss", &s.to_string()]);
    }
    cmd.args(["-i", input]);
    if let Some(d) = dur {
        cmd.args(["-t", &d.to_string()]);
    }
    cmd.args(["-an", "-sn", "-dn", "-vf", vf, "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]);
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("ffmpegの起動に失敗しました: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("ffmpegの出力を取得できません")?;
    let mut stderr = child.stderr.take().ok_or("ffmpegのエラー出力を取得できません")?;
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });

    let frame_len = cw as usize * ch as usize * 3;
    let mut idx = 0u64;
    let mut reference: Option<Vec<u8>> = None;
    let outcome: Result<(), String> = loop {
        let mut buf = vec![0u8; frame_len];
        match read_full(&mut stdout, &mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) if n < frame_len => break Err("映像の最後のコマが途中で切れています".to_string()),
            Ok(_) => {}
            Err(e) => break Err(format!("デコード結果を読めません: {e}")),
        }
        if progress::is_cancelled() {
            break Err(progress::CANCELLED.to_string());
        }
        if idx < skip {
            idx += 1; // 再開: 確定済みの部分は読み捨てる。
            continue;
        }
        let sent = match classify(&buf, reference.as_deref(), skip_static) {
            FrameKind::Flat(c) => {
                reference = Some(buf);
                direct.send((idx, Slot::Flat(c))).map_err(|_| "回収スレッドが停止しました".to_string())
            }
            FrameKind::Same => direct.send((idx, Slot::Repeat)).map_err(|_| "回収スレッドが停止しました".to_string()),
            FrameKind::New => {
                reference = Some(buf.clone());
                submitter.submit(Job { idx, w: cw as usize, h: ch as usize, rgb: buf })
            }
        };
        if let Err(e) = sent {
            break Err(e);
        }
        idx += 1;
    };
    let _ = child.kill();
    let status = child.wait();
    let err_text = err_thread.join().unwrap_or_default();
    total.store(idx, Ordering::SeqCst);
    outcome?;
    if let Ok(st) = status {
        // 正常終了で全コマを読んだあとにkillしているので、終了コードは見ない。デコード失敗はコマが0のときに現れる。
        if idx == 0 && !st.success() {
            return Err(format!("映像のデコードに失敗しました: {err_text}"));
        }
    }
    if idx == 0 {
        return Err(format!("映像のコマを取り出せませんでした: {err_text}"));
    }
    Ok(())
}

/// `buf`がいっぱいになるかEOFまで読む。読めたバイト数を返す(0=最初からEOF)。
fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        let n = r.read(&mut buf[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    Ok(got)
}

#[allow(clippy::too_many_arguments)]
fn collect(pool: SrPool, direct_rx: mpsc::Receiver<(u64, Slot)>, total: Arc<AtomicU64>, work: PathBuf, geo: Geometry, fps: (u64, u64), key: String, resume: u64, frames_est: u64) -> Result<Stats, String> {
    let frame_bytes = geo.ow as usize * geo.oh as usize * 3;
    let mut pending: BTreeMap<u64, Slot> = BTreeMap::new();
    let mut next = resume * CHUNK_FRAMES;
    let mut last_out: Option<Arc<Vec<u8>>> = None;
    let mut flat_cache: Option<([u8; 3], Arc<Vec<u8>>)> = None;
    let mut enc: Option<ChunkEnc> = None;
    let mut stats = Stats::default();
    let (t0, mut last_emit) = (Instant::now(), Instant::now());
    let start_frame = next;
    let mut worker_error: Option<String> = None;
    let mut workers_gone = false;

    let result: Result<(), String> = 'outer: loop {
        while let Ok((i, s)) = direct_rx.try_recv() {
            pending.insert(i, s);
        }
        match pool.recv_timeout(Duration::from_millis(30)) {
            Ok(Some(Ok(d))) => {
                if (d.w * d.h * 3) != frame_bytes || d.w as u32 != geo.ow {
                    break 'outer Err(format!("超解像の出力の大きさが想定と違います({}×{}、想定 {}×{})", d.w, d.h, geo.ow, geo.oh));
                }
                match d.by {
                    "gpu" => stats.sr_gpu += 1,
                    "fallback" => {
                        if stats.fallback == 0 {
                            say("一部のコマでAI出力が壊れていたため、通常の拡大に置き換えました / Some AI outputs were invalid and were replaced by a plain upscale");
                        }
                        stats.fallback += 1;
                    }
                    _ => stats.sr_cpu += 1,
                }
                pending.insert(d.idx, Slot::Frame(Arc::new(d.rgb)));
            }
            Ok(Some(Err(e))) => {
                worker_error = Some(e);
            }
            Ok(None) => {}
            Err(()) => workers_gone = true,
        }
        if let Some(e) = worker_error.take() {
            break 'outer Err(e);
        }
        while let Some(slot) = pending.remove(&next) {
            if enc.is_none() {
                match spawn_chunk(&work, next / CHUNK_FRAMES, &geo, fps) {
                    Ok(e) => enc = Some(e),
                    Err(e) => break 'outer Err(e),
                }
            }
            let frame: Arc<Vec<u8>> = match slot {
                Slot::Frame(a) => a,
                Slot::Flat(c) => {
                    stats.flat += 1;
                    match &flat_cache {
                        Some((col, a)) if *col == c => a.clone(),
                        _ => {
                            let mut v = vec![0u8; frame_bytes];
                            for px in v.chunks_exact_mut(3) {
                                px.copy_from_slice(&c);
                            }
                            let a = Arc::new(v);
                            flat_cache = Some((c, a.clone()));
                            a
                        }
                    }
                }
                Slot::Repeat => {
                    stats.repeat += 1;
                    match &last_out {
                        Some(a) => a.clone(),
                        None => break 'outer Err("再利用するコマがありません(内部エラー)".to_string()),
                    }
                }
            };
            if let Err(e) = enc.as_mut().unwrap().write(&frame) {
                break 'outer Err(e);
            }
            last_out = Some(frame);
            next += 1;
            if next % CHUNK_FRAMES == 0 {
                if let Err(e) = enc.take().unwrap().finish() {
                    break 'outer Err(e);
                }
                write_manifest(&work, &Manifest { key: key.clone(), chunk_frames: CHUNK_FRAMES, chunks_done: next / CHUNK_FRAMES });
            }
        }
        if last_emit.elapsed() >= Duration::from_secs(2) {
            last_emit = Instant::now();
            let done_now = next - start_frame;
            let elapsed = t0.elapsed().as_secs_f64();
            let total_now = total.load(Ordering::SeqCst);
            let total_frames = if total_now == u64::MAX { frames_est } else { total_now };
            let rate = if elapsed > 0.0 { done_now as f64 / elapsed } else { 0.0 };
            let eta = if rate > 0.0 { (total_frames.saturating_sub(next)) as f64 / rate } else { f64::NAN };
            progress::emit(
                "ai-progress",
                serde_json::json!({
                    "stage": "upscale", "done": next, "total": total_frames, "elapsed_secs": elapsed,
                    "eta_secs": if eta.is_finite() { serde_json::json!(eta) } else { serde_json::Value::Null },
                    "fps": rate, "cpu": stats.sr_cpu, "gpu": stats.sr_gpu, "flat": stats.flat, "repeat": stats.repeat
                }),
            );
        }
        let total_now = total.load(Ordering::SeqCst);
        if total_now != u64::MAX && next >= total_now {
            break 'outer Ok(());
        }
        if workers_gone {
            while let Ok((i, sl)) = direct_rx.try_recv() {
                pending.insert(i, sl);
            }
            if !pending.contains_key(&next) {
                break 'outer Err("AI超解像のワーカーが予期せず終了しました / the upscaling workers ended unexpectedly".to_string());
            }
        }
        if progress::is_cancelled() {
            break 'outer Err(progress::CANCELLED.to_string());
        }
    };

    match result {
        Ok(()) => {
            if let Some(e) = enc.take() {
                e.finish()?;
                write_manifest(&work, &Manifest { key, chunk_frames: CHUNK_FRAMES, chunks_done: next.div_ceil(CHUNK_FRAMES) });
            }
            pool.shutdown();
            Ok(stats)
        }
        Err(e) => {
            if let Some(en) = enc.take() {
                en.abort();
            }
            pool.shutdown();
            Err(e)
        }
    }
}

// ── 見積もり ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Estimate {
    pub analysis: Analysis,
    pub model: String,
    pub scale: u32,
    pub out_w: u32,
    pub out_h: u32,
    /// このPCの実測がまだ無いとき`None`(先に測定してもらう)。
    pub secs_per_frame: Option<f64>,
    pub eta_secs: Option<f64>,
    /// フレーム補間(RIFE)の倍率・補間コマ1枚あたりの秒・方式・所要時間(補間を行わないときは`None`)。
    pub rife_factor: Option<u32>,
    pub rife_secs_per_frame: Option<f64>,
    pub rife_method: Option<String>,
    pub rife_eta_secs: Option<f64>,
    /// 超解像とフレーム補間を合わせた所要時間(どちらかが不明なら`None`)。
    pub total_eta_secs: Option<f64>,
    pub hw_message_ja: Option<String>,
    pub hw_message_en: Option<String>,
    pub temp_bytes: u64,
    pub free_bytes: Option<u64>,
    pub warnings_ja: Vec<String>,
    pub warnings_en: Vec<String>,
}

/// 中間ファイルの大きさの見積もり(1画素・1コマあたりのビット数)。x264 CRF14・veryfastで、AIで鮮明にした映像を想定して多めに見る。
const MEZZANINE_BPP: f64 = 0.30;

pub fn estimate(input: &str, start: Option<f64>, dur: Option<f64>, up: &AiUpscale, target: Option<(u32, u32)>, work_dir: &Path) -> Result<Estimate, String> {
    let an = analyze(input, start, dur, up)?;
    let exe = ai_upscale::ensure_plugin()?;
    let models = exe.parent().ok_or("プラグインの場所が不正です")?.join("models");
    let model = resolve_model(up, &models);
    let (cw, ch) = an.crop.map_or((an.width, an.height), |c| (c.w, c.h));
    let scale = if up.scale == 0 { auto_scale(ch, target.map(|t| t.1)) } else { up.scale };
    let (out_w, out_h) = target.unwrap_or(((an.width as f64 * scale as f64) as u32, (an.height as f64 * scale as f64) as u32));

    let bench = hw_bench::cached();
    // 通常のモデル(animevideov3)は測定済みの速さ、汎用モデルは層が2倍なので約2倍かかるとみなす。
    let model_factor = if model == "realesr-general-x4v3" { 2.0 } else if model == "realesrgan-x4plus" { 30.0 } else { 1.0 };
    let spf = bench.as_ref().map(|b| hw_bench::secs_per_frame(b, cw as usize, ch as usize) * model_factor);
    let eta = spf.map(|s| s * an.frames as f64);

    // フレーム補間(RIFE)の見積もり。GPUが壊れた出力を返す大きさ(GT 730の4Kなど)では、CPUの速さで見積もる。
    let out_fps = an.out_fps_num as f64 / an.out_fps_den.max(1) as f64;
    let rife_factor = crate::engine::rife::factor_for(out_fps, up.target_fps);
    let (rife_spf, rife_method, rife_eta, mut rife_warn) = match rife_factor {
        Some(f) => {
            let (rw, rh) = if up.interpolate_only { (an.width, an.height) } else { (out_w, out_h) };
            match crate::engine::rife::speed(rw, rh, bench.as_ref().and_then(|b| b.gpu_index)) {
                Ok(sp) => {
                    let frames = an.duration_secs * out_fps;
                    let eta = sp.secs_per_frame * frames * (f as f64 - 1.0);
                    let mut w = Vec::new();
                    if let Some(note) = &sp.gpu_note {
                        w.push((
                            format!("この画面の大きさ({rw}×{rh})では、このPCのGPUがフレーム補間で壊れた出力を返します({note})。CPUで補間するため非常に遅くなります(補間コマ1枚あたり約{:.0}秒)。", sp.secs_per_frame),
                            format!("At {rw}x{rh} this PC's GPU returns broken interpolation output ({note}); the CPU is used and is very slow (~{:.0} s per interpolated frame).", sp.secs_per_frame),
                        ));
                    }
                    (Some(sp.secs_per_frame), Some(sp.method.clone()), Some(eta), w)
                }
                Err(_) => (None, None, None, Vec::new()),
            }
        }
        None => (None, None, None, Vec::new()),
    };
    let total_eta = match (up.interpolate_only, eta, rife_eta) {
        (true, _, r) => r,
        (false, Some(e), Some(r)) => Some(e + r),
        (false, e, None) if rife_factor.is_none() => e,
        _ => None,
    };
    let mezz_bytes = (out_w as f64 * out_h as f64 * an.fps * MEZZANINE_BPP / 8.0 * an.duration_secs) as u64;
    let temp_bytes = mezz_bytes * 2; // 中間ファイルの分割分 + 結合後の1本
    let free = free_space_bytes(work_dir);
    let (mut wj, mut we) = (Vec::new(), Vec::new());
    if let Some(f) = free {
        if f < temp_bytes {
            wj.push(format!("作業用のディスク空き容量が足りない見込みです(必要 約{:.1}GB、空き {:.1}GB)。出力先の空きを増やしてください。", temp_bytes as f64 / 1e9, f as f64 / 1e9));
            we.push(format!("Not enough free disk space for temporary files (need ~{:.1} GB, {:.1} GB free).", temp_bytes as f64 / 1e9, f as f64 / 1e9));
        }
    }
    if let Some(e) = eta {
        if e > 86400.0 {
            wj.push(format!("所要時間が{}と非常に長くなります。必要な部分だけ切り出す(8.)か、より速いPC(GPU)での実行をご検討ください。", progress::fmt_duration(e)));
            we.push(format!("This will take {} — consider extracting only the part you need (section 8) or a faster GPU.", progress::fmt_duration(e)));
        }
    }
    for (ja, en) in rife_warn.drain(..) {
        wj.push(ja);
        we.push(en);
    }
    if let Some(e) = rife_eta {
        if e > 86400.0 {
            wj.push(format!("フレーム補間だけで{}かかる見込みです。fpsを下げるか、解像度を下げるか、必要な部分だけ切り出してください。", progress::fmt_duration(e)));
            we.push(format!("Frame interpolation alone will take about {} — lower the fps or resolution, or extract only the part you need.", progress::fmt_duration(e)));
        }
    }
    Ok(Estimate {
        model,
        scale,
        out_w,
        out_h,
        secs_per_frame: spf,
        eta_secs: if up.interpolate_only { None } else { eta },
        rife_factor,
        rife_secs_per_frame: rife_spf,
        rife_method,
        rife_eta_secs: rife_eta,
        total_eta_secs: total_eta,
        hw_message_ja: bench.as_ref().map(|b| b.message_ja.clone()),
        hw_message_en: bench.as_ref().map(|b| b.message_en.clone()),
        temp_bytes,
        free_bytes: free,
        warnings_ja: wj,
        warnings_en: we,
        analysis: an,
    })
}

/// `path`のあるドライブの空き容量(バイト)。取得できない環境では`None`。
pub fn free_space_bytes(path: &Path) -> Option<u64> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
        }
        let probe = if path.exists() { path.to_path_buf() } else { path.ancestors().find(|p| p.exists())?.to_path_buf() };
        let wide: Vec<u16> = probe.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        // SAFETY: 終端付きのUTF-16パスと、有効なu64へのポインタを渡している。
        let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
        (ok != 0).then_some(avail)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_idet_summary_and_classifies() {
        let s = "[Parsed_idet_0 @ 0x1] Repeated Fields: Neither: 700 Top: 0 Bottom: 0\n\
                 [Parsed_idet_0 @ 0x1] Single frame detection: TFF: 10 BFF: 0 Progressive: 690 Undetermined: 0\n\
                 [Parsed_idet_0 @ 0x1] Multi frame detection: TFF: 280 BFF: 0 Progressive: 420 Undetermined: 0\n";
        assert_eq!(parse_idet(s), Some((280, 420)));
        assert_eq!(classify_interlace(280, 420).0, Interlace::Telecine); // 40%
        assert_eq!(classify_interlace(650, 50).0, Interlace::Interlaced);
        assert_eq!(classify_interlace(5, 695).0, Interlace::Progressive);
        assert_eq!(classify_interlace(5, 5).0, Interlace::Progressive, "サンプルが少なすぎるときは何もしない");
        assert_eq!(parse_idet("nothing here"), None);
    }

    #[test]
    fn parses_and_accepts_letterbox_crops() {
        let s = "[Parsed_cropdetect_0 @ 0x1] x1:0 x2:719 y1:60 y2:419 w:720 h:352 x:0 y:64 pts:1 t:0.04 limit:0.094 crop=720:352:0:64\n\
                 [Parsed_cropdetect_0 @ 0x1] x1:0 x2:719 y1:60 y2:419 w:720 h:360 x:0 y:60 pts:2 t:0.08 limit:0.094 crop=720:360:0:60\n";
        let c = parse_cropdetect_last(s).unwrap();
        assert_eq!(c, Crop { w: 720, h: 360, x: 0, y: 60 });
        assert_eq!(accept_crop(c, 720, 480), Some(c));
        let two = union_crops(&[Crop { w: 720, h: 352, x: 0, y: 64 }, Crop { w: 720, h: 400, x: 0, y: 40 }]).unwrap();
        assert_eq!(two, Crop { w: 720, h: 400, x: 0, y: 40 });
        // 黒帯が無い/小さい/片側だけ/小さすぎる場合は採用しない。
        assert_eq!(accept_crop(Crop { w: 720, h: 478, x: 0, y: 1 }, 720, 480), None);
        assert_eq!(accept_crop(Crop { w: 720, h: 400, x: 0, y: 0 }, 720, 480), None, "片側だけの帯は字幕などの可能性がある");
        assert_eq!(accept_crop(Crop { w: 720, h: 200, x: 0, y: 140 }, 720, 480), None, "半分未満は暗い場面の誤検出とみなす");
    }

    #[test]
    fn decode_filter_orders_deinterlace_crop_and_color_matrix() {
        let f = decode_filter(Interlace::Interlaced, Some(Crop { w: 720, h: 360, x: 0, y: 60 }), 480);
        assert!(f.starts_with("bwdif=") && f.contains("crop=720:360:0:60") && f.contains("in_color_matrix=bt601") && f.ends_with("format=rgb24"), "{f}");
        assert!(f.find("bwdif").unwrap() < f.find("crop").unwrap(), "解除してから切り取る");
        assert!(decode_filter(Interlace::Telecine, None, 480).contains("decimate"));
        assert!(decode_filter(Interlace::Progressive, None, 1080).contains("bt709"));
        assert!(!decode_filter(Interlace::Progressive, None, 480).contains("bwdif"));
    }

    #[test]
    fn fit_keeps_the_display_aspect_ratio() {
        // 16:9のDVD(720x480、SAR 32:27)→フルHD: そのまま埋める。
        assert_eq!(fit_geometry(1440, 960, (32, 27), (1920, 1080)), (1920, 1080, 0, 0));
        // 4:3のDVD(SAR 8:9)→フルHD: 1440x1080に収め、左右に黒帯。
        assert_eq!(fit_geometry(1440, 960, (8, 9), (1920, 1080)), (1440, 1080, 240, 0));
        // 2.35:1の映画を黒帯付き16:9のまま処理(黒帯を戻した後は16:9なので埋める)。
        assert_eq!(fit_geometry(2880, 1920, (32, 27), (3840, 2160)), (3840, 2160, 0, 0));
        // 横長すぎる映像は上下に黒帯。
        let (w, h, x, y) = fit_geometry(1920, 800, (1, 1), (1920, 1080));
        assert_eq!((w, h, x), (1920, 800, 0));
        assert_eq!(y, 140);
    }

    #[test]
    fn encoder_filter_restores_bars_and_tags_bt709() {
        let g = Geometry { ow: 1440, oh: 720, canvas_w: 1440, canvas_h: 960, pad: Some((0, 120)), sar: (32, 27), target: Some((1920, 1080)) };
        let f = encoder_vf(&g);
        assert!(f.starts_with("pad=1440:960:0:120:black"), "{f}");
        assert!(f.contains("scale=1920:1080:") && f.contains("out_color_matrix=bt709") && f.contains("format=yuv420p") && f.ends_with("setsar=1"), "{f}");
        let keep = encoder_vf(&Geometry { ow: 1440, oh: 960, canvas_w: 1440, canvas_h: 960, pad: None, sar: (32, 27), target: None });
        assert!(keep.ends_with("setsar=32/27") && !keep.contains("pad="), "{keep}");
        let pillar = encoder_vf(&Geometry { ow: 1440, oh: 960, canvas_w: 1440, canvas_h: 960, pad: None, sar: (8, 9), target: Some((1920, 1080)) });
        assert!(pillar.contains("scale=1440:1080:") && pillar.contains("pad=1920:1080:240:0:black"), "{pillar}");
    }

    #[test]
    fn flat_frames_are_detected_but_not_frames_with_small_details() {
        let black = vec![0u8; 720 * 480 * 3];
        assert_eq!(flat_color(&black), Some([0, 0, 0]));
        let white = vec![255u8; 720 * 480 * 3];
        assert_eq!(flat_color(&white), Some([255, 255, 255]));
        let mut text = black.clone();
        for i in 0..12 {
            text[(100 * 720 + 300 + i) * 3] = 255; // 黒地の小さな文字の一画
        }
        assert_eq!(flat_color(&text), None, "黒地の小さな文字を単色と誤認してはいけない");
        assert_eq!(classify(&black, None, true), FrameKind::Flat([0, 0, 0]));
        assert_eq!(classify(&black, None, false), FrameKind::New, "省略を切ると必ず処理する");
    }

    #[test]
    fn similar_frames_are_reused_only_when_really_close() {
        let a = sr_pool::synthetic_frame(160, 120, 1);
        assert!(similar_to(&a, &a));
        let mut noisy = a.clone();
        for (i, v) in noisy.iter_mut().enumerate() {
            if i % 97 == 0 {
                *v = v.saturating_add(3);
            }
        }
        assert!(similar_to(&a, &noisy), "小さなノイズ差は同じコマとみなす");
        let other = sr_pool::synthetic_frame(160, 120, 2);
        assert!(!similar_to(&a, &other));
        let mut moved = a.clone();
        for v in moved.iter_mut().take(2000) {
            *v = v.wrapping_add(80); // 一部が大きく変わった(動き)
        }
        assert!(!similar_to(&a, &moved));
        assert_eq!(classify(&a, Some(&a), true), FrameKind::Same);
        assert_eq!(classify(&other, Some(&a), true), FrameKind::New);
    }

    #[test]
    fn scale_and_model_resolution() {
        assert_eq!(auto_scale(480, Some(1080)), 3);
        assert_eq!(auto_scale(480, Some(2160)), 4);
        assert_eq!(auto_scale(360, Some(1080)), 3);
        assert_eq!(auto_scale(1080, Some(2160)), 2);
        assert_eq!(auto_scale(480, None), 2);
        let dir = std::env::temp_dir().join("md_model_test");
        let _ = std::fs::create_dir_all(&dir);
        let up = AiUpscale { model: "auto".into(), ..Default::default() };
        assert_eq!(resolve_model(&up, &dir), "realesr-animevideov3", "汎用モデルが無ければ動画向けモデル");
    }

    #[test]
    fn key_is_stable_and_sensitive() {
        assert_eq!(fnv1a("abc"), fnv1a("abc"));
        assert_ne!(fnv1a("abc"), fnv1a("abd"));
    }

    #[test]
    fn ratios_parse() {
        assert_eq!(parse_ratio("24000/1001"), Some((24000, 1001)));
        assert_eq!(parse_ratio("32:27"), Some((32, 27)));
        assert_eq!(parse_ratio("0/0"), None);
        assert_eq!(parse_ratio("N/A"), None);
    }
    fn have_ffmpeg() -> bool {
        std::process::Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
    }

    fn make_src(dir: &Path, name: &str, vf: Option<&str>) -> String {
        let p = dir.join(name);
        let mut args: Vec<String> = ["-y", "-f", "lavfi", "-i", "testsrc=duration=1:size=160x120:rate=3", "-f", "lavfi", "-i", "sine=frequency=440:duration=1"].iter().map(|s| s.to_string()).collect();
        if let Some(f) = vf {
            args.extend(["-vf".to_string(), f.to_string()]);
        }
        args.extend(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"].iter().map(|s| s.to_string()));
        args.push(p.to_string_lossy().to_string());
        let st = std::process::Command::new("ffmpeg").args(&args).output().unwrap();
        assert!(st.status.success(), "{}", String::from_utf8_lossy(&st.stderr));
        p.to_string_lossy().to_string()
    }

    fn probe_streams(path: &Path) -> String {
        let o = std::process::Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream=codec_type,width,height", "-of", "csv=p=0", path.to_str().unwrap()]).output().unwrap();
        String::from_utf8_lossy(&o.stdout).to_string()
    }

    /// GPUを使わずCPU版だけで、実クリップを本当にAI超解像して4倍になり音声も保持されることを検証する
    /// (Vulkan非対応のPC相当)。プラグイン(初回のみネットワーク)とffmpegが無い環境ではスキップする。
    #[test]
    fn real_ai_upscale_quadruples_a_short_clip_on_the_cpu_backend() {
        if !have_ffmpeg() {
            return;
        }
        if let Err(e) = ai_upscale::ensure_plugin() {
            eprintln!("Real-ESRGANのモデルを用意できないためスキップ: {e}");
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_ai_cpu_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let src = make_src(&tmp, "src.mp4", None);
        let mezz = tmp.join("mezz.mkv");
        let up = AiUpscale { model: "realesr-animevideov3".into(), scale: 4, backend: "cpu".into(), ..Default::default() };
        make_upscaled_mezzanine(&src, None, &up, None, &mezz).expect("CPU AI upscaling should succeed");
        let text = probe_streams(&mezz);
        let leftover = std::fs::read_dir(&tmp).unwrap().filter_map(|e| e.ok()).any(|e| e.file_name().to_string_lossy().starts_with(".make-disk-ai-"));
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(text.contains("640,480"), "160x120が4倍の640x480になるはず(実際: {text})");
        assert!(text.contains("audio"), "元の音声が保持されるはず(実際: {text})");
        assert!(!leftover, "作業フォルダは片付けられるはず");
    }

    /// 実GPU・実プラグインで、短いクリップを本当にAI超解像して解像度が4倍になることを検証する。
    /// Vulkan対応GPU・プラグイン・ffmpegが無い環境ではスキップする。
    #[test]
    fn real_ai_upscale_quadruples_a_short_clip_on_the_gpu() {
        if !have_ffmpeg() {
            return;
        }
        if let Err(e) = ai_upscale::ensure_plugin() {
            eprintln!("Real-ESRGANプラグインを用意できないためスキップ: {e}");
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_ai_gpu_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let src = make_src(&tmp, "src.mp4", None);
        let mezz = tmp.join("mezz.mkv");
        let up = AiUpscale { model: "realesr-animevideov3".into(), scale: 4, backend: "gpu".into(), ..Default::default() };
        let result = make_upscaled_mezzanine(&src, None, &up, None, &mezz);
        if let Err(e) = &result {
            if e.contains("Vulkan") || e.contains("GPU") {
                eprintln!("Vulkan対応GPUが無いためスキップ: {e}");
                let _ = std::fs::remove_dir_all(&tmp);
                return;
            }
        }
        result.expect("AI upscaling should succeed on a Vulkan GPU");
        let text = probe_streams(&mezz);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(text.contains("640,480"), "160x120が4倍の640x480になるはず(実際: {text})");
        assert!(text.contains("audio"), "元の音声が保持されるはず(実際: {text})");
    }

    /// 長時間実行の検証: 2分(約3600コマ)のDVD相当クリップを、途中で中止→再開しながらフルHDへ超解像する。
    /// 1時間以上かかるので通常は実行しない。コマ数の一致・再開・処理速度・一時ディスクの使用量を確かめる。
    #[test]
    #[ignore]
    fn real_long_run_cancel_and_resume() {
        let tmp = std::env::temp_dir().join(format!("make-disk-longrun-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let src = tmp.join("dvd.mkv");
        let ok = resolve_tool("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=720x480:rate=30000/1001:duration=120", "-f", "lavfi", "-i", "sine=frequency=440:duration=120"])
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "ac3", "-shortest"])
            .arg(&src)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let src_frames = crate::engine::rife::count_frames_pub(&src.to_string_lossy());
        eprintln!("source frames: {src_frames}");
        let out = tmp.join("out.mkv");
        let up = AiUpscale { backend: "auto".into(), ..Default::default() };
        let t = Instant::now();
        // 1回目: 約15分で中止を依頼する。
        let killer = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(15 * 60));
            progress::request_cancel();
        });
        progress::clear_cancel();
        let r = make_upscaled_mezzanine(&src.to_string_lossy(), None, &up, Some((1920, 1080)), &out);
        let _ = killer.join();
        eprintln!("first run ended after {:.0}s: {:?}", t.elapsed().as_secs_f64(), r.as_ref().err().map(|e| e.chars().take(40).collect::<String>()));
        assert!(r.is_err() && r.unwrap_err().contains("中止"), "中止されるはず");
        let work_bytes: u64 = std::fs::read_dir(&tmp).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().starts_with(".make-disk-ai-")).map(|e| dir_size(&e.path())).sum();
        eprintln!("work dir after cancel: {:.1} MB", work_bytes as f64 / 1e6);
        assert!(work_bytes > 0, "再開用の途中経過が残っているはず");
        // 2回目: 続きから再開して最後まで。
        progress::clear_cancel();
        let t2 = Instant::now();
        make_upscaled_mezzanine(&src.to_string_lossy(), None, &up, Some((1920, 1080)), &out).expect("resume to completion");
        eprintln!("second run took {:.0}s (total {:.0}s)", t2.elapsed().as_secs_f64(), t.elapsed().as_secs_f64());
        let n = crate::engine::rife::count_frames_pub(&out.to_string_lossy());
        let info = probe_video(&out.to_string_lossy()).unwrap();
        eprintln!("output frames: {n}, {}x{}", info.w, info.h);
        assert_eq!((info.w, info.h), (1920, 1080));
        assert_eq!(n, src_frames, "コマ数が元と一致するはず");
        assert!(std::fs::read_dir(&tmp).unwrap().filter_map(|e| e.ok()).all(|e| !e.file_name().to_string_lossy().starts_with(".make-disk-ai-")), "完了後は作業フォルダが消えるはず");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn dir_size(p: &Path) -> u64 {
        std::fs::read_dir(p).map(|r| r.filter_map(|e| e.ok()).map(|e| e.metadata().map(|m| if m.is_dir() { dir_size(&e.path()) } else { m.len() }).unwrap_or(0)).sum()).unwrap_or(0)
    }
}
