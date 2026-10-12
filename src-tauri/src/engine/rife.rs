//! RIFEによるフレーム補間(30→60、24→120など)(2026-09-24新設)。
//!
//! [rife-ncnn-vulkan](https://github.com/nihui/rife-ncnn-vulkan)(MIT、リリース`20221029`、モデル`rife-v4.6`)を
//! **オンデマンドのプラグイン**として使う。配布ZIPは約411MBあるが、必要なのは約12MBだけなので、
//! HTTPのRangeリクエストでZIPの目次と必要なファイルだけを取得する(全体はダウンロードしない)。
//!
//! ## 流れ(超解像済みの中間ファイルに対して行う)
//! 1. 元動画を`SEG`コマずつ(+つなぎの1コマ)PNGへ書き出す。
//! 2. `rife-ncnn-vulkan -n (m-1)*倍率+1`で、ちょうど倍率ぶんのコマ数に補間する。
//! 3. 出力をx264で区間ファイルにする。**区間ごとに確定するので、止めても続きから再開できる。**
//! 4. 全区間を結合し、元の音声を付けて中間ファイルを置き換える。
//!
//! ## 実機で確認した事実・制限(正直な開示)
//! - このPC(GT 730・2GB)では、**既定のスレッド設定(`-j 1:2:2`)だと補間結果が黒画面になる**(エラーは出ない)。
//!   `-j 1:1:1`にすると正常。出力の明るさを検査し、壊れた区間はコマの繰り返し(補間なし)へ置き換えて警告する。
//! - 補間は**動きの大きい場面や場面転換で破綻(ゴースト・ゆがみ)することがある**。RIFEは近似であり、
//!   実際に120fpsで撮影した映像と同じにはならない。
//! - 完全に静止した区間はRIFEを通さず、コマの繰り返しで済ませる(計算時間の節約)。
//! - GPUが無い環境では`-g -1`のCPUモードになるが、GPUの約10倍遅い。

use crate::engine::plugins;
use crate::engine::sidecar::{background_command, resolve_tool};
use crate::progress;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub const RIFE_VERSION: &str = "20221029";
pub const RIFE_MODEL: &str = "rife-v4.6";

fn asset_name() -> Result<String, String> {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "ubuntu"
    } else {
        return Err("フレーム補間はデスクトップのみ対応です / frame interpolation is desktop-only".to_string());
    };
    Ok(format!("rife-ncnn-vulkan-{RIFE_VERSION}-{os}.zip"))
}

fn exe_name() -> String {
    format!("rife-ncnn-vulkan{}", std::env::consts::EXE_SUFFIX)
}

fn plugin_root() -> Result<PathBuf, String> {
    Ok(plugins::plugin_dir().ok_or("プラグインフォルダを特定できません")?.join("rife").join(RIFE_VERSION))
}

pub fn installed_exe() -> Option<PathBuf> {
    let root = plugin_root().ok()?;
    (root.join(exe_name()).is_file() && root.join(RIFE_MODEL).join("flownet.bin").is_file()).then(|| root.join(exe_name()))
}

/// HTTPのRangeリクエストで読む、`Read + Seek`なリモートファイル(ZIPの一部だけを取るため)。
struct RangeReader {
    url: String,
    len: u64,
    pos: u64,
    cache_start: u64,
    cache: Vec<u8>,
}

impl RangeReader {
    fn open(url: &str) -> Result<Self, String> {
        let resp = ureq::get(url).set("Range", "bytes=0-0").call().map_err(|e| format!("RIFEの取得に失敗しました({url}): {e}"))?;
        let len = resp
            .header("Content-Range")
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .ok_or("配布ファイルの大きさを取得できません(Range非対応?)")?;
        Ok(Self { url: url.to_string(), len, pos: 0, cache_start: 0, cache: Vec::new() })
    }

    fn fetch(&mut self, start: u64, want: u64) -> std::io::Result<()> {
        let end = (start + want.max(64 * 1024)).min(self.len) - 1;
        let resp = ureq::get(&self.url).set("Range", &format!("bytes={start}-{end}")).call().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let mut buf = Vec::with_capacity((end - start + 1) as usize);
        resp.into_reader().read_to_end(&mut buf)?;
        self.cache_start = start;
        self.cache = buf;
        Ok(())
    }
}

impl Read for RangeReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.len || out.is_empty() {
            return Ok(0);
        }
        let in_cache = self.pos >= self.cache_start && self.pos < self.cache_start + self.cache.len() as u64;
        if !in_cache {
            self.fetch(self.pos, out.len() as u64)?;
        }
        let off = (self.pos - self.cache_start) as usize;
        let n = out.len().min(self.cache.len() - off);
        out[..n].copy_from_slice(&self.cache[off..off + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for RangeReader {
    fn seek(&mut self, p: SeekFrom) -> std::io::Result<u64> {
        let np = match p {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::End(d) => self.len as i64 + d,
            SeekFrom::Current(d) => self.pos as i64 + d,
        };
        if np < 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek before start"));
        }
        self.pos = np as u64;
        Ok(self.pos)
    }
}

/// 展開する価値のあるファイルか(実行ファイル・ランタイムDLL・使うモデルだけ)。
fn wanted(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let in_model = rel.contains(&format!("/{RIFE_MODEL}/"));
    in_model || name == exe_name() || name.ends_with(".dll") || name.eq_ignore_ascii_case("LICENSE")
}

/// プラグインが無ければ、必要な部分だけ取得して展開する。
pub fn ensure_plugin() -> Result<PathBuf, String> {
    if let Some(e) = installed_exe() {
        return Ok(e);
    }
    let url = format!("https://github.com/nihui/rife-ncnn-vulkan/releases/download/{RIFE_VERSION}/{}", asset_name()?);
    let root = plugin_root()?;
    std::fs::create_dir_all(&root).map_err(|e| format!("プラグインフォルダを作成できません: {e}"))?;
    progress::emit("ai-progress", serde_json::json!({ "stage": "info", "message": "フレーム補間(RIFE)を取得しています(約12MB)… / Fetching RIFE (about 12 MB)…" }));
    let reader = RangeReader::open(&url)?;
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| format!("ZIPを開けません: {e}"))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("ZIPの読み取りに失敗しました: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let Some(rel) = entry.enclosed_name() else { continue };
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        if !wanted(&rel_s) {
            continue;
        }
        // 先頭の`rife-ncnn-vulkan-…-windows/`を外して、プラグインの直下へ置く。
        let stripped: PathBuf = rel.components().skip(1).collect();
        let dest = root.join(stripped);
        if let Some(p) = dest.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&dest).map_err(|e| format!("{}を作成できません: {e}", dest.display()))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let exe = root.join(exe_name());
        if let Ok(meta) = std::fs::metadata(&exe) {
            let mut perm = meta.permissions();
            perm.set_mode(0o755);
            let _ = std::fs::set_permissions(&exe, perm);
        }
    }
    installed_exe().ok_or_else(|| "展開後にrife-ncnn-vulkanまたはモデルが見つかりません".to_string())
}

/// 目標fpsから補間の倍率を決める。`None`は補間しない。
pub fn factor_for(src_fps: f64, target_fps: Option<u32>) -> Option<u32> {
    let t = target_fps? as f64;
    if src_fps <= 0.0 || t < src_fps * 1.5 {
        return None;
    }
    Some(((t / src_fps).round() as u32).clamp(2, 8))
}

/// 1区間あたりの元コマ数(大きい画面ほど一時PNGが大きいので少なくする)。
pub fn segment_frames(w: u32) -> u64 {
    if w >= 2560 {
        12
    } else {
        24
    }
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

fn load_rgb(p: &Path) -> Option<Vec<u8>> {
    image::open(p).ok().map(|i| i.to_rgb8().into_raw())
}

fn count_pngs(dir: &Path) -> usize {
    std::fs::read_dir(dir).map(|r| r.filter_map(|e| e.ok()).filter(|e| e.path().extension().is_some_and(|x| x == "png")).count()).unwrap_or(0)
}

fn png_name(dir: &Path, n: usize) -> PathBuf {
    dir.join(format!("{n:08}.png"))
}

/// `video`(映像+音声)を`factor`倍のフレームレートへ補間し、同じファイル名へ置き換える。
/// `gpu`は使うVulkanデバイス(`None`ならCPUモード=非常に遅い)。
pub fn interpolate_in_place(video: &Path, factor: u32, gpu: Option<u32>) -> Result<(), String> {
    let mut gpu = gpu;
    let exe = ensure_plugin()?;
    let models = exe.parent().ok_or("プラグインの場所が不正です")?.to_path_buf();
    let path_s = video.to_string_lossy().to_string();
    let info = crate::engine::ai_video::probe_video(&path_s)?;
    let total = count_frames(&path_s, info.duration, info.fps)?;
    let seg = segment_frames(info.w);
    let out_fps = (info.fps.0 * factor as u64, info.fps.1);
    let n_seg = total.div_ceil(seg);
    let ext = video.extension().and_then(|e| e.to_str()).unwrap_or("mp4").to_string();

    let key = format!("{:016x}", fnv(&format!("{}|{}|{}|{}", path_s, std::fs::metadata(video).map(|m| m.len()).unwrap_or(0), factor, RIFE_MODEL)));
    let work = video.parent().unwrap_or(Path::new(".")).join(format!(".make-disk-rife-{key}"));
    std::fs::create_dir_all(&work).map_err(|e| format!("作業フォルダを作成できません: {e}"))?;

    if gpu.is_none() {
        progress::emit("ai-progress", serde_json::json!({ "stage": "info", "message": "GPUが無いため、フレーム補間はCPUで行います(GPUの約10倍遅くなります)。 / No GPU: interpolation runs on the CPU (about 10x slower)." }));
    }
    let t0 = Instant::now();
    let (mut done_new, mut replaced) = (0u64, 0u64);
    for k in 0..n_seg {
        if progress::is_cancelled() {
            return Err(progress::CANCELLED.to_string());
        }
        let seg_file = work.join(format!("seg-{k:06}.mp4"));
        if !seg_file.is_file() {
            let s = k * seg;
            let n = seg.min(total - s);
            let has_next = s + n < total;
            if let SegOutcome::Replaced = make_segment(&exe, &models, &mut gpu, &path_s, &info, factor, out_fps, s, n, has_next, &work, &seg_file)? {
                replaced += 1;
            }
            done_new += 1;
        }
        let elapsed = t0.elapsed().as_secs_f64();
        let rate = if elapsed > 0.0 { done_new as f64 / elapsed } else { 0.0 };
        let eta = if rate > 0.0 { Some((n_seg - k - 1) as f64 / rate) } else { None };
        progress::emit(
            "ai-progress",
            serde_json::json!({ "stage": "interpolate", "done": ((k + 1) * seg).min(total), "total": total, "elapsed_secs": elapsed, "eta_secs": eta, "factor": factor }),
        );
    }
    if replaced > 0 {
        progress::emit(
            "ai-progress",
            serde_json::json!({ "stage": "info", "message": format!("{replaced}区間で補間の出力が壊れていたため、コマの繰り返しに置き換えました。 / {replaced} segments had invalid interpolation output and were replaced by frame repetition.") }),
        );
    }

    // 結合して元の音声を付ける。
    let list: String = (0..n_seg).map(|c| format!("file '{}'\n", work.join(format!("seg-{c:06}.mp4")).to_string_lossy().replace('\\', "/").replace('\'', "'\\''"))).collect();
    let list_path = work.join("list.txt");
    std::fs::write(&list_path, list).map_err(|e| e.to_string())?;
    let tmp = video.with_file_name(format!("{}.rife.{ext}", video.file_stem().and_then(|s| s.to_str()).unwrap_or("out")));
    let res = resolve_tool("ffmpeg")
        .args(["-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", &list_path.to_string_lossy(), "-i", &path_s])
        .args(["-map", "0:v", "-map", "1:a?", "-c", "copy", "-shortest"])
        .arg(&tmp)
        .output()
        .map_err(|e| format!("ffmpegの起動に失敗しました: {e}"))?;
    if !res.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("補間した映像の結合に失敗しました: {}", String::from_utf8_lossy(&res.stderr)));
    }
    std::fs::rename(&tmp, video).map_err(|e| format!("中間ファイルを置き換えられません: {e}"))?;
    let _ = std::fs::remove_dir_all(&work);
    Ok(())
}

/// テスト用: コマ数を数える。
pub fn count_frames_pub(input: &str) -> u64 {
    let info = crate::engine::ai_video::probe_video(input).expect("probe");
    count_frames(input, info.duration, info.fps).expect("count")
}

fn count_frames(input: &str, duration: f64, fps: (u64, u64)) -> Result<u64, String> {
    let out = resolve_tool("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-count_packets", "-show_entries", "stream=nb_read_packets", "-of", "csv=p=0", input])
        .output()
        .map_err(|e| format!("ffprobeの起動に失敗しました: {e}"))?;
    let n = String::from_utf8_lossy(&out.stdout).trim().trim_end_matches(',').parse::<u64>().ok();
    let est = (duration * fps.0 as f64 / fps.1 as f64).round() as u64;
    n.filter(|&n| n > 0).or((est > 0).then_some(est)).ok_or_else(|| "コマ数を数えられません".to_string())
}

enum SegOutcome {
    Interpolated,
    Static,
    Replaced,
}

#[allow(clippy::too_many_arguments)]
fn make_segment(
    exe: &Path,
    models: &Path,
    gpu: &mut Option<u32>,
    input: &str,
    info: &crate::engine::ai_video::VideoInfo,
    factor: u32,
    out_fps: (u64, u64),
    s: u64,
    n: u64,
    has_next: bool,
    work: &Path,
    seg_file: &Path,
) -> Result<SegOutcome, String> {
    let (din, dout) = (work.join("in"), work.join("out"));
    for d in [&din, &dout] {
        let _ = std::fs::remove_dir_all(d);
        std::fs::create_dir_all(d).map_err(|e| format!("作業フォルダを作れません: {e}"))?;
    }
    let cleanup = || {
        let _ = std::fs::remove_dir_all(&din);
        let _ = std::fs::remove_dir_all(&dout);
    };
    let m = n + has_next as u64; // 取り出すコマ数(つなぎの1コマを含む)
    let fps_f = info.fps.0 as f64 / info.fps.1 as f64;
    let t = (s as f64 / fps_f - 0.25 / fps_f).max(0.0);
    let ex = resolve_tool("ffmpeg")
        .args(["-v", "error", "-y", "-ss", &format!("{t:.6}"), "-i", input, "-map", "0:v:0", "-frames:v", &m.to_string(), "-start_number", "1"])
        .arg(din.join("%08d.png"))
        .output()
        .map_err(|e| format!("ffmpegの起動に失敗しました: {e}"))?;
    let got = count_pngs(&din) as u64;
    if !ex.status.success() || got == 0 {
        cleanup();
        return Err(format!("補間用のコマを取り出せません: {}", String::from_utf8_lossy(&ex.stderr)));
    }
    // 端で取れたコマ数が足りなければ、取れた数に合わせる。
    let m = got;
    let has_next = has_next && m > n;
    let n = if has_next { n.min(m - 1) } else { m.min(n) };
    let want_out = n * factor as u64; // この区間が出力するコマ数

    let first = load_rgb(&png_name(&din, 1));
    let mid = load_rgb(&png_name(&din, (m as usize).div_ceil(2)));
    let last = load_rgb(&png_name(&din, m as usize));
    let is_static = match (&first, &mid, &last) {
        (Some(a), Some(b), Some(c)) => crate::engine::ai_video::similar_to(a, b) && crate::engine::ai_video::similar_to(a, c),
        _ => false,
    };

    let mut outcome = SegOutcome::Interpolated;
    let mut frames_ok = false;
    if is_static || m < 2 {
        outcome = SegOutcome::Static;
    } else {
        // GPUで試し、出力が壊れていたら(実機のGT 730では4Kで「デバイス喪失」になり、一部のコマが黒くなる)
        // 同じ区間をCPUでやり直す。GPUが壊れたら、以降の区間は最初からCPUで処理する。
        let mut attempts: Vec<Option<u32>> = Vec::new();
        if gpu.is_some() {
            attempts.push(*gpu);
        }
        attempts.push(None);
        for dev in attempts {
            let _ = std::fs::remove_dir_all(&dout);
            let _ = std::fs::create_dir_all(&dout);
            let out = run_rife(exe, models, dev, &din, &dout, m, factor, info.w)?;
            if all_interpolated_frames_ok(&din, &dout, m, factor) {
                frames_ok = true;
                break;
            }
            if dev.is_some() {
                *gpu = None;
                progress::emit(
                    "ai-progress",
                    serde_json::json!({ "stage": "info", "message": format!("GPUでの補間結果が壊れていました(この画面の大きさではGPUが対応できない場合があります)。CPUでやり直します(遅くなります)。 / GPU interpolation output was invalid; redoing on the CPU (slower). {}", out.lines().last().unwrap_or("")) }),
                );
            }
        }
        if !frames_ok {
            outcome = SegOutcome::Replaced;
        }
    }

    // 区間ファイルへエンコード。補間できたときは補間結果、それ以外は元コマの繰り返し。
    let part = seg_file.with_extension("part.mp4");
    let vf_common = "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p";
    let mut enc = resolve_tool("ffmpeg");
    enc.args(["-v", "error", "-y"]);
    if frames_ok {
        enc.args(["-framerate", &format!("{}/{}", out_fps.0, out_fps.1), "-start_number", "1", "-i"]).arg(dout.join("%08d.png"));
        if has_next {
            enc.args(["-vf", vf_common]);
        } else {
            // 最後の区間は、末尾の元コマを補間の倍率ぶんだけ保持して長さを合わせる。
            enc.args(["-vf", &format!("tpad=stop_mode=clone:stop={},{vf_common}", factor - 1)]);
        }
        enc.args(["-frames:v", &want_out.to_string()]);
    } else {
        enc.args(["-framerate", &format!("{}/{}", out_fps.0 / factor as u64, out_fps.1), "-start_number", "1", "-i"]).arg(din.join("%08d.png"));
        enc.args(["-vf", &format!("{vf_common},fps={}/{}", out_fps.0, out_fps.1), "-frames:v", &want_out.to_string()]);
    }
    enc.args(["-c:v", "libx264", "-crf", "14", "-preset", "veryfast", "-pix_fmt", "yuv420p", "-profile:v", "high"]);
    enc.args(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv", "-an"]).arg(&part);
    let r = enc.output().map_err(|e| format!("ffmpegの起動に失敗しました: {e}"))?;
    cleanup();
    if !r.status.success() {
        let _ = std::fs::remove_file(&part);
        return Err(format!("補間区間のエンコードに失敗しました: {}", String::from_utf8_lossy(&r.stderr)));
    }
    std::fs::rename(&part, seg_file).map_err(|e| format!("区間ファイルを確定できません: {e}"))?;
    Ok(outcome)
}

/// rife-ncnn-vulkanを1回実行する。標準エラー出力の末尾(失敗の手掛かり)を返す。
#[allow(clippy::too_many_arguments)]
fn run_rife(exe: &Path, models: &Path, dev: Option<u32>, din: &Path, dout: &Path, m: u64, factor: u32, width: u32) -> Result<String, String> {
    let mut cmd = background_command(exe);
    cmd.args(["-i", &din.to_string_lossy(), "-o", &dout.to_string_lossy(), "-m", RIFE_MODEL]);
    cmd.args(["-n", &((m - 1) * factor as u64 + 1).to_string(), "-f", "%08d.png", "-j", "1:1:1"]);
    cmd.args(["-g", &dev.map_or("-1".to_string(), |g| g.to_string())]);
    if width >= 2560 {
        cmd.arg("-u");
    }
    cmd.current_dir(models);
    let out = cmd.output().map_err(|e| format!("rife-ncnn-vulkanの起動に失敗しました: {e}"))?;
    let err = String::from_utf8_lossy(&out.stderr);
    Ok(err.lines().filter(|l| l.contains("failed") || l.contains("error")).last().unwrap_or("").to_string())
}

fn mean_luma(rgb: &[u8]) -> f64 {
    let step = (rgb.len() / 30000).max(1);
    let (mut sum, mut n) = (0u64, 0u64);
    for &b in rgb.iter().step_by(step) {
        sum += b as u64;
        n += 1;
    }
    if n == 0 {
        0.0
    } else {
        sum as f64 / n as f64
    }
}

/// 全ての出力コマが存在し、補間コマが前後の元コマに比べて極端に暗くないか(黒画面の検出)を確かめる。
fn all_interpolated_frames_ok(din: &Path, dout: &Path, m: u64, factor: u32) -> bool {
    let f = factor as usize;
    let total = (m as usize - 1) * f + 1;
    if count_pngs(dout) < total {
        return false;
    }
    let src: Vec<Option<f64>> = (1..=m as usize).map(|i| load_rgb(&png_name(din, i)).map(|v| mean_luma(&v))).collect();
    for k in 1..=total {
        let (q, r) = ((k - 1) / f, (k - 1) % f);
        let Some(v) = load_rgb(&png_name(dout, k)) else { return false };
        let mo = mean_luma(&v);
        let (a, b) = (src.get(q).copied().flatten(), src.get(q + 1).copied().flatten());
        let lo = match (a, b) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) | (None, Some(a)) => a,
            _ => continue,
        };
        // 元コマ自体(r==0)も、暗くなっていないか確かめる。
        let _ = r;
        if lo > 12.0 && mo < lo * 0.4 {
            return false;
        }
    }
    true
}

/// この画面の大きさでのRIFEの速さ(補間コマ1枚あたりの秒)の実測結果。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RifeSpeed {
    pub width: u32,
    pub height: u32,
    /// `"gpu"`(GPUで正しく動いた)/ `"cpu"`(GPUが壊れた出力を返したのでCPU)。
    pub method: String,
    pub secs_per_frame: f64,
    /// GPUが使えなかった理由(あれば)。
    pub gpu_note: Option<String>,
    pub signature: String,
}

fn speed_cache_path() -> Option<PathBuf> {
    Some(plugins::plugin_dir()?.join("rife-bench.json"))
}

fn speed_signature() -> String {
    format!("v1|{RIFE_VERSION}|{}", if cfg!(debug_assertions) { "debug" } else { "release" })
}

/// 指定の大きさで、補間コマ1枚あたりの秒を測る(GPUで試し、出力が壊れていればCPUで測る)。結果は保存して次回以降に使う。
pub fn speed(width: u32, height: u32, gpu: Option<u32>) -> Result<RifeSpeed, String> {
    let sig = speed_signature();
    let mut cache: Vec<RifeSpeed> = speed_cache_path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if let Some(c) = cache.iter().find(|c| c.width == width && c.height == height && c.signature == sig) {
        return Ok(c.clone());
    }
    let exe = ensure_plugin()?;
    let models = exe.parent().ok_or("プラグインの場所が不正です")?.to_path_buf();
    let dir = std::env::temp_dir().join(format!("make-disk-rife-speed-{}", std::process::id()));
    let (din, dout) = (dir.join("in"), dir.join("out"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&din).and_then(|_| std::fs::create_dir_all(&dout)).map_err(|e| e.to_string())?;
    for i in 1..=2u64 {
        let f = crate::engine::sr_pool::synthetic_frame(width as usize, height as usize, 900 + i);
        crate::engine::sr_pool::write_png_fast(&png_name(&din, i as usize), width as usize, height as usize, &f)?;
    }
    let mut result: Option<RifeSpeed> = None;
    let mut gpu_note = None;
    let mut attempts: Vec<Option<u32>> = Vec::new();
    if gpu.is_some() {
        attempts.push(gpu);
    }
    attempts.push(None);
    for dev in attempts {
        let _ = std::fs::remove_dir_all(&dout);
        let _ = std::fs::create_dir_all(&dout);
        let t = Instant::now();
        let note = run_rife(&exe, &models, dev, &din, &dout, 2, 2, width)?;
        let secs = t.elapsed().as_secs_f64();
        if all_interpolated_frames_ok(&din, &dout, 2, 2) {
            result = Some(RifeSpeed { width, height, method: if dev.is_some() { "gpu" } else { "cpu" }.to_string(), secs_per_frame: secs, gpu_note: gpu_note.clone(), signature: sig.clone() });
            break;
        }
        if dev.is_some() {
            gpu_note = Some(if note.is_empty() { "GPUの出力が壊れていました".to_string() } else { note });
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    let r = result.ok_or("RIFEの速さを測れませんでした(CPUでも正しい出力が得られません)")?;
    cache.retain(|c| !(c.width == width && c.height == height));
    cache.push(r.clone());
    if let Some(p) = speed_cache_path() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, serde_json::to_vec_pretty(&cache).unwrap_or_default());
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factor_follows_the_target_and_ignores_small_gains() {
        assert_eq!(factor_for(30.0, Some(60)), Some(2));
        assert_eq!(factor_for(24.0, Some(120)), Some(5));
        assert_eq!(factor_for(29.97, Some(120)), Some(4));
        assert_eq!(factor_for(60.0, Some(60)), None);
        assert_eq!(factor_for(50.0, Some(60)), None);
        assert_eq!(factor_for(30.0, None), None);
    }

    #[test]
    fn big_frames_use_smaller_segments() {
        assert!(segment_frames(3840) < segment_frames(1920));
    }

    #[test]
    fn only_needed_files_are_extracted() {
        let root = "rife-ncnn-vulkan-20221029-windows";
        assert!(wanted(&format!("{root}/{}", exe_name())));
        assert!(wanted(&format!("{root}/vcomp140.dll")));
        assert!(wanted(&format!("{root}/{RIFE_MODEL}/flownet.bin")));
        assert!(!wanted(&format!("{root}/rife-v2/flownet.bin")));
    }

    /// 実物: 範囲取得でRIFEを導入し、短い動画を2倍に補間できる(ネットワークとGPUが必要)。
    #[test]
    #[ignore]
    fn real_rife_doubles_the_frame_rate() {
        let exe = ensure_plugin().expect("plugin");
        assert!(exe.is_file());
        let dir = std::env::temp_dir().join("make-disk-rife-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.mp4");
        let ok = resolve_tool("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=10:duration=3", "-f", "lavfi", "-i", "sine=frequency=440:duration=3", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
            .arg(&src)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        interpolate_in_place(&src, 2, Some(0)).expect("interpolate");
        let info = crate::engine::ai_video::probe_video(&src.to_string_lossy()).unwrap();
        assert_eq!(info.fps, (20, 1));
        let n = count_frames(&src.to_string_lossy(), info.duration, info.fps).unwrap();
        assert!((59..=61).contains(&n), "frames={n}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 実測: このPCでのRIFEの速さ(補間コマ1枚あたりの秒)を、フルHDと4Kで測る。
    #[test]
    #[ignore]
    fn real_rife_speed_at_full_hd_and_4k() {
        for (w, h) in [(1920u32, 1080u32), (3840, 2160)] {
            let sp = speed(w, h, Some(0)).expect("speed");
            eprintln!("RIFE {w}x{h}: {} {:.1} s per interpolated frame, gpu_note={:?}", sp.method, sp.secs_per_frame, sp.gpu_note);
        }
    }
}
