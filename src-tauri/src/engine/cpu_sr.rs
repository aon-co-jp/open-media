//! CPU専用のAI超解像(Real-ESRGAN `realesr-animevideov3`、2026-09-19新設)。
//!
//! Vulkan対応GPUが無いPC向け。公式ncnn形式のモデル(`.param`/`.bin`、MITライセンス)を自前で読み込み、
//! 小型の畳み込みネット(SRVGGNetCompact: 3x3畳み込み×18層 + PReLU + PixelShuffle + 最近傍拡大の加算)を
//! CPUで推論する。AVX2+FMAが使えれば(open-cpuの検出結果に従い)専用カーネルを、無ければスカラー実装を使う。
//!
//! ## 設計
//! - 画像を128×128のタイルに分け、各タイルに受容野の半径(=畳み込み層数18)だけ余白を付けて処理するので、
//!   タイル境界でも画像全体を一度に処理した結果と(浮動小数点の誤差を除いて)一致する。タイルは複数スレッドで並列処理。
//! - 活性化はHWC(チャンネルが連続)。重みは`[tap][入力ch][出力ch]`に並べ替え、AVX2では出力16ch×4画素を
//!   レジスタに保持して重みの読み込みを再利用する。
//!
//! **正直な開示**: 対応モデルはこの小型の`realesr-animevideov3`(2/3/4倍)のみ。高品質な`realesrgan-x4plus`(RRDBNet、約64MB)は
//! 構造が大きく異なり、CPUでは非現実的なため対象外(GPU版のみ)。速度はコア数・命令セット次第(実測値はCLAUDE.md参照)。

use std::path::Path;

const FP16_FLAG: u32 = 0x0130_6B47;
/// 3x3畳み込みを18層重ねた受容野の半径。タイルの余白にする。
const HALO: usize = 18;
const TILE: usize = 128;

struct ConvLayer {
    in_c: usize,
    out_c: usize,
    /// `[tap(0..9)][in_c][out_c]`
    w: Vec<f32>,
    bias: Vec<f32>,
    prelu: Option<Vec<f32>>,
}

pub struct SrModel {
    layers: Vec<ConvLayer>,
    /// ネットワーク自体の拡大倍率(PixelShuffleの倍率、常に4)。
    scale: usize,
    /// 後段の双三次リサイズ倍率(x2は0.5、x3は0.75。x4はNone)。
    post_resize: Option<f32>,
}

impl SrModel {
    /// x4専用のネットワーク(例: `realesr-general-x4v3`)から、2倍・3倍の出力を作るための後段リサイズを設定する
    /// (ncnnの公式配布のx2/x3モデルが、x4の出力を双三次で縮小しているのと同じ方法)。`target`が4以上なら何もしない。
    pub fn with_output_scale(mut self, target: u32) -> Self {
        if (2..self.scale as u32).contains(&target) {
            self.post_resize = Some(target as f32 / self.scale as f32);
        }
        self
    }

    /// ネットワークの拡大倍率(後段のリサイズ前)。テスト用。
    #[cfg(test)]
    pub fn scale(&self) -> usize {
        self.scale
    }

    /// 入力`w`×`h`に対する最終的な出力サイズ。
    pub fn output_size(&self, w: usize, h: usize) -> (usize, usize) {
        let f = self.post_resize.unwrap_or(1.0) * self.scale as f32;
        (((w as f32) * f).round() as usize, ((h as f32) * f).round() as usize)
    }
}

/// 双三次補間(ncnnのInterp mode 3に合わせ係数a=-0.75、`align_corners=false`、端は複製)。
fn bicubic_resize(src: &[f32], w: usize, h: usize, ow: usize, oh: usize) -> Vec<f32> {
    fn cubic(t: f32) -> [f32; 4] {
        let a = -0.75f32;
        let f = |x: f32| {
            let x = x.abs();
            if x <= 1.0 { ((a + 2.0) * x - (a + 3.0)) * x * x + 1.0 } else if x < 2.0 { (((x - 5.0) * x + 8.0) * x - 4.0) * a } else { 0.0 }
        };
        [f(1.0 + t), f(t), f(1.0 - t), f(2.0 - t)]
    }
    let (sx, sy) = (w as f32 / ow as f32, h as f32 / oh as f32);
    let mut out = vec![0f32; ow * oh * 3];
    for oy in 0..oh {
        let fy = (oy as f32 + 0.5) * sy - 0.5;
        let y0 = fy.floor();
        let wy = cubic(fy - y0);
        for ox in 0..ow {
            let fx = (ox as f32 + 0.5) * sx - 0.5;
            let x0 = fx.floor();
            let wx = cubic(fx - x0);
            for c in 0..3 {
                let mut acc = 0.0;
                for (j, wyj) in wy.iter().enumerate() {
                    let yy = (y0 as i64 + j as i64 - 1).clamp(0, h as i64 - 1) as usize;
                    for (i, wxi) in wx.iter().enumerate() {
                        let xx = (x0 as i64 + i as i64 - 1).clamp(0, w as i64 - 1) as usize;
                        acc += wyj * wxi * src[(yy * w + xx) * 3 + c];
                    }
                }
                out[(oy * ow + ox) * 3 + c] = acc.clamp(0.0, 1.0);
            }
        }
    }
    out
}

fn half_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1f) as u32;
    let frac = (h & 0x3ff) as u32;
    let bits = if exp == 0 {
        if frac == 0 {
            sign << 31
        } else {
            // 非正規化数
            let mut e = 127 - 15 + 1;
            let mut f = frac;
            while f & 0x400 == 0 {
                f <<= 1;
                e -= 1;
            }
            (sign << 31) | ((e as u32) << 23) | ((f & 0x3ff) << 13)
        }
    } else if exp == 31 {
        (sign << 31) | 0x7f80_0000 | (frac << 13)
    } else {
        (sign << 31) | ((exp + 127 - 15) << 23) | (frac << 13)
    };
    f32::from_bits(bits)
}

struct BinReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl BinReader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.pos.checked_add(n).filter(|e| *e <= self.data.len()).ok_or("モデルの.binが途中で終わっています")?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    /// タグ付きの重み(fp16またはfloat32)を読む。fp16は4バイト境界まで読み飛ばす。
    fn tagged_weights(&mut self, n: usize) -> Result<Vec<f32>, String> {
        let flag = self.u32()?;
        match flag {
            FP16_FLAG => {
                let raw = self.take(n * 2)?.to_vec();
                let out = raw.as_chunks::<2>().0.iter().map(|c| half_to_f32(u16::from_le_bytes(*c))).collect();
                self.pos = (self.pos + 3) & !3;
                Ok(out)
            }
            0 => self.floats(n),
            other => Err(format!("未対応の重み形式です(flag=0x{other:08x})")),
        }
    }
    fn floats(&mut self, n: usize) -> Result<Vec<f32>, String> {
        Ok(self.take(n * 4)?.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect())
    }
}

/// ncnnの`.param`(テキスト)と`.bin`から、対応する構造のモデルを読み込む。
pub fn parse_model(param: &str, bin: &[u8]) -> Result<SrModel, String> {
    let mut lines = param.lines();
    if lines.next().map(str::trim) != Some("7767517") {
        return Err("ncnnの.paramではありません(マジック不一致)".to_string());
    }
    lines.next(); // レイヤー数/blob数
    let mut reader = BinReader { data: bin, pos: 0 };
    let mut layers: Vec<ConvLayer> = Vec::new();
    let mut shuffle: Option<usize> = None;
    let mut interp: Option<(i32, f32)> = None;
    let mut added = false;
    let mut post_resize: Option<f32> = None;
    let mut prev_in_c = 3usize;

    for line in lines.filter(|l| !l.trim().is_empty()) {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() < 4 {
            return Err(format!("解析できない行です: {line}"));
        }
        let n_in: usize = t[2].parse().map_err(|_| "入力数が不正です")?;
        let n_out: usize = t[3].parse().map_err(|_| "出力数が不正です")?;
        let kv: std::collections::HashMap<&str, &str> = t[4 + n_in + n_out..].iter().filter_map(|s| s.split_once('=')).collect();
        let geti = |k: &str| -> Option<i64> { kv.get(k).and_then(|v| v.parse::<f64>().ok()).map(|v| v as i64) };
        match t[0] {
            "Input" | "Split" => {}
            "Convolution" => {
                let out_c = geti("0").ok_or("出力チャンネル数がありません")? as usize;
                let (k, pad, bias_term, wsize) = (geti("1").unwrap_or(0), geti("4").unwrap_or(0), geti("5").unwrap_or(0), geti("6").ok_or("重みサイズがありません")? as usize);
                if k != 3 || pad != 1 || bias_term != 1 {
                    return Err("未対応の畳み込みです(3x3・パディング1・バイアス有りのみ対応)".to_string());
                }
                if out_c == 0 || wsize % (out_c * 9) != 0 {
                    return Err("重みサイズが不正です".to_string());
                }
                let in_c = wsize / (out_c * 9);
                if in_c != prev_in_c {
                    return Err(format!("層のチャンネル数がつながりません(期待{prev_in_c}、実際{in_c})"));
                }
                let raw = reader.tagged_weights(wsize)?; // [oc][ic][ky][kx]
                let bias = reader.floats(out_c)?;
                let mut w = vec![0f32; wsize];
                for oc in 0..out_c {
                    for ic in 0..in_c {
                        for tap in 0..9 {
                            w[(tap * in_c + ic) * out_c + oc] = raw[(oc * in_c + ic) * 9 + tap];
                        }
                    }
                }
                layers.push(ConvLayer { in_c, out_c, w, bias, prelu: None });
                prev_in_c = out_c;
            }
            "PReLU" => {
                let n = geti("0").ok_or("PReLUのチャンネル数がありません")? as usize;
                let slopes = reader.floats(n)?;
                let last = layers.last_mut().ok_or("PReLUの前に畳み込みがありません")?;
                if last.out_c != n {
                    return Err("PReLUのチャンネル数が畳み込みと一致しません".to_string());
                }
                last.prelu = Some(slopes);
            }
            "PixelShuffle" => shuffle = Some(geti("0").ok_or("PixelShuffleの倍率がありません")? as usize),
            "Interp" => {
                let mode = geti("0").unwrap_or(0) as i32;
                let factor: f32 = kv.get("1").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                if added {
                    // 加算の後のInterpは、x2/x3モデルの最終リサイズ(双三次、mode 3)。
                    if mode != 3 || !(0.1..=1.0).contains(&factor) {
                        return Err("未対応の後段リサイズです(双三次の縮小のみ対応)".to_string());
                    }
                    post_resize = Some(factor);
                } else {
                    interp = Some((mode, factor));
                }
            }
            "BinaryOp" => {
                if geti("0").unwrap_or(0) != 0 {
                    return Err("未対応のBinaryOpです(加算のみ対応)".to_string());
                }
                added = true;
            }
            other => return Err(format!("未対応の層です: {other}(realesr-animevideov3のみ対応)")),
        }
    }
    let scale = shuffle.ok_or("PixelShuffleがありません")?;
    let (mode, factor) = interp.ok_or("Interpがありません")?;
    if mode != 1 || (factor - scale as f32).abs() > 1e-3 || !added {
        return Err("未対応の構造です(最近傍拡大+加算の残差接続のみ対応)".to_string());
    }
    if layers.last().map(|l| l.out_c) != Some(3 * scale * scale) {
        return Err("最終層の出力チャンネル数がPixelShuffleと合いません".to_string());
    }
    if reader.pos != bin.len() {
        return Err(format!("重みファイルに未使用のデータが残っています(読了{} / {}バイト)", reader.pos, bin.len()));
    }
    Ok(SrModel { layers, scale, post_resize })
}

/// モデルフォルダから`realesr-animevideov3-x{scale}`(2/3/4)を読み込む。
pub fn load_model(models_dir: &Path, scale: u32) -> Result<SrModel, String> {
    load_named(models_dir, "realesr-animevideov3", scale)
}

/// モデル名`name`を読み込む。`realesr-animevideov3`は倍率ごとの配布ファイル(`-x2/-x3/-x4`)を使い、
/// それ以外(`realesr-general-x4v3`など、x4専用)は`<name>.param/.bin`を読んで、必要なら後段リサイズで2倍・3倍にする。
pub fn load_named(models_dir: &Path, name: &str, scale: u32) -> Result<SrModel, String> {
    let per_scale = name == "realesr-animevideov3";
    let base = if per_scale { format!("{name}-x{scale}") } else { name.to_string() };
    let param = std::fs::read_to_string(models_dir.join(format!("{base}.param"))).map_err(|e| format!("{base}.paramを読めません: {e}"))?;
    let bin = std::fs::read(models_dir.join(format!("{base}.bin"))).map_err(|e| format!("{base}.binを読めません: {e}"))?;
    let model = parse_model(&param, &bin)?;
    Ok(if per_scale { model } else { model.with_output_scale(scale) })
}

/// 使う計算カーネルの名前(ログ表示用)。open-cpuの検出結果に従う。
pub fn kernel_name() -> &'static str {
    if avx2_available() { "AVX2+FMA" } else { "scalar" }
}

fn avx2_available() -> bool {
    let caps = open_cpu::detect();
    cfg!(target_arch = "x86_64") && caps.avx2 && caps.fma
}

/// ゼロ余白付きのHWCバッファ`(h+2)*(w+2)*c`。
fn padded(h: usize, w: usize, c: usize) -> Vec<f32> {
    vec![0f32; (h + 2) * (w + 2) * c]
}

fn activate(v: f32, slope: Option<f32>) -> f32 {
    match slope {
        Some(s) => v.max(0.0) + s * v.min(0.0),
        None => v,
    }
}

/// スカラー(可搬)実装。AVX2カーネルの正解参照にもなる。
fn conv_generic(inp: &[f32], out: &mut [f32], h: usize, w: usize, l: &ConvLayer) {
    let (ic_n, oc_n) = (l.in_c, l.out_c);
    let (sin, sout) = ((w + 2) * ic_n, (w + 2) * oc_n);
    let mut acc = vec![0f32; oc_n];
    for y in 0..h {
        for x in 0..w {
            acc.copy_from_slice(&l.bias);
            for tap in 0..9 {
                let (ky, kx) = (tap / 3, tap % 3);
                let px = &inp[(y + ky) * sin + (x + kx) * ic_n..][..ic_n];
                for (ic, &v) in px.iter().enumerate() {
                    let wrow = &l.w[(tap * ic_n + ic) * oc_n..][..oc_n];
                    for (a, &wv) in acc.iter_mut().zip(wrow) {
                        *a += v * wv;
                    }
                }
            }
            let dst = &mut out[(y + 1) * sout + (x + 1) * oc_n..][..oc_n];
            for (o, d) in dst.iter_mut().enumerate() {
                *d = activate(acc[o], l.prelu.as_ref().map(|p| p[o]));
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn conv_avx2(inp: &[f32], out: &mut [f32], h: usize, w: usize, l: &ConvLayer) {
    use std::arch::x86_64::*;
    let (ic_n, oc_n) = (l.in_c, l.out_c);
    let (sin, sout) = ((w + 2) * ic_n, (w + 2) * oc_n);
    let zero = _mm256_setzero_ps();
    for y in 0..h {
        let mut ob = 0;
        while ob < oc_n {
            let b0 = _mm256_loadu_ps(l.bias.as_ptr().add(ob));
            let b1 = _mm256_loadu_ps(l.bias.as_ptr().add(ob + 8));
            let (s0, s1) = match &l.prelu {
                Some(p) => (_mm256_loadu_ps(p.as_ptr().add(ob)), _mm256_loadu_ps(p.as_ptr().add(ob + 8))),
                None => (zero, zero),
            };
            let mut x = 0;
            // 4画素ずつ: 出力16ch×4画素=8個のアキュムレータをレジスタに保持する。
            while x + 4 <= w {
                let mut a = [[b0, b1]; 4];
                for tap in 0..9 {
                    let (ky, kx) = (tap / 3, tap % 3);
                    let base = inp.as_ptr().add((y + ky) * sin + (x + kx) * ic_n);
                    let wp = l.w.as_ptr().add(tap * ic_n * oc_n + ob);
                    for ic in 0..ic_n {
                        let w0 = _mm256_loadu_ps(wp.add(ic * oc_n));
                        let w1 = _mm256_loadu_ps(wp.add(ic * oc_n + 8));
                        for (p, ap) in a.iter_mut().enumerate() {
                            let v = _mm256_broadcast_ss(&*base.add(p * ic_n + ic));
                            ap[0] = _mm256_fmadd_ps(v, w0, ap[0]);
                            ap[1] = _mm256_fmadd_ps(v, w1, ap[1]);
                        }
                    }
                }
                for (p, ap) in a.iter().enumerate() {
                    let dst = out.as_mut_ptr().add((y + 1) * sout + (x + p + 1) * oc_n + ob);
                    for (j, (acc, s)) in [(ap[0], s0), (ap[1], s1)].into_iter().enumerate() {
                        let r = if l.prelu.is_some() { _mm256_fmadd_ps(s, _mm256_min_ps(acc, zero), _mm256_max_ps(acc, zero)) } else { acc };
                        _mm256_storeu_ps(dst.add(j * 8), r);
                    }
                }
                x += 4;
            }
            // 余りの画素(1画素ずつ)
            while x < w {
                let (mut a0, mut a1) = (b0, b1);
                for tap in 0..9 {
                    let (ky, kx) = (tap / 3, tap % 3);
                    let base = inp.as_ptr().add((y + ky) * sin + (x + kx) * ic_n);
                    let wp = l.w.as_ptr().add(tap * ic_n * oc_n + ob);
                    for ic in 0..ic_n {
                        let v = _mm256_broadcast_ss(&*base.add(ic));
                        a0 = _mm256_fmadd_ps(v, _mm256_loadu_ps(wp.add(ic * oc_n)), a0);
                        a1 = _mm256_fmadd_ps(v, _mm256_loadu_ps(wp.add(ic * oc_n + 8)), a1);
                    }
                }
                let dst = out.as_mut_ptr().add((y + 1) * sout + (x + 1) * oc_n + ob);
                for (j, (acc, s)) in [(a0, s0), (a1, s1)].into_iter().enumerate() {
                    let r = if l.prelu.is_some() { _mm256_fmadd_ps(s, _mm256_min_ps(acc, zero), _mm256_max_ps(acc, zero)) } else { acc };
                    _mm256_storeu_ps(dst.add(j * 8), r);
                }
                x += 1;
            }
            ob += 16;
        }
    }
}

fn conv_layer(inp: &[f32], out: &mut [f32], h: usize, w: usize, l: &ConvLayer, use_avx2: bool) {
    #[cfg(target_arch = "x86_64")]
    if use_avx2 && l.out_c.is_multiple_of(16) {
        // 安全性: avx2+fmaはavx2_available()(open-cpu)で確認済み。範囲はバッファ寸法から決まる。
        unsafe { conv_avx2(inp, out, h, w, l) };
        return;
    }
    let _ = use_avx2;
    conv_generic(inp, out, h, w, l);
}

/// 1タイル(HWC、`h`×`w`×3、0〜1)を処理して`(h*r)×(w*r)×3`(0〜1、クリップ済み)を返す。
fn run_tile(model: &SrModel, rgb: &[f32], h: usize, w: usize, use_avx2: bool) -> Vec<f32> {
    let mut cur = padded(h, w, 3);
    for y in 0..h {
        for x in 0..w {
            cur[(y + 1) * (w + 2) * 3 + (x + 1) * 3..][..3].copy_from_slice(&rgb[(y * w + x) * 3..][..3]);
        }
    }
    for l in &model.layers {
        let mut next = padded(h, w, l.out_c);
        conv_layer(&cur, &mut next, h, w, l, use_avx2);
        cur = next;
    }
    let r = model.scale;
    let sc = (w + 2) * model.layers.last().map_or(0, |l| l.out_c);
    let oc = model.layers.last().map_or(0, |l| l.out_c);
    let mut out = vec![0f32; h * r * w * r * 3];
    for y in 0..h {
        for x in 0..w {
            let feat = &cur[(y + 1) * sc + (x + 1) * oc..][..oc];
            for c in 0..3 {
                let base_in = rgb[(y * w + x) * 3 + c];
                for i in 0..r {
                    for j in 0..r {
                        // PixelShuffle: out[c][y*r+i][x*r+j] = in[c*r*r + i*r + j][y][x]
                        let v = feat[c * r * r + i * r + j] + base_in;
                        out[((y * r + i) * (w * r) + (x * r + j)) * 3 + c] = v.clamp(0.0, 1.0);
                    }
                }
            }
        }
    }
    out
}

/// 画像全体(HWC、0〜1)を超解像する。タイルを複数スレッドで並列処理する。
pub fn upscale_rgb(model: &SrModel, rgb: &[f32], w: usize, h: usize) -> Vec<f32> {
    upscale_rgb_threads(model, rgb, w, h, 0)
}

/// `upscale_rgb`のスレッド数指定版(`0`=全論理コア)。GPUと同時に動かすとき、GPU側の読み書きに数コアを残すために使う。
pub fn upscale_rgb_threads(model: &SrModel, rgb: &[f32], w: usize, h: usize, threads: usize) -> Vec<f32> {
    let r = model.scale;
    let use_avx2 = avx2_available();
    let mut tiles = Vec::new();
    for ty in (0..h).step_by(TILE) {
        for tx in (0..w).step_by(TILE) {
            tiles.push((tx, ty));
        }
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let threads = (if threads == 0 { cores } else { threads.min(cores) }).max(1).min(tiles.len().max(1));
    let results = std::sync::Mutex::new(Vec::<(usize, usize, usize, usize, Vec<f32>)>::new());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                crate::engine::sidecar::lower_current_thread_priority();
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(&(tx, ty)) = tiles.get(i) else { break };
                let (x1, y1) = ((tx + TILE).min(w), (ty + TILE).min(h));
                let (rx0, ry0) = (tx.saturating_sub(HALO), ty.saturating_sub(HALO));
                let (rx1, ry1) = ((x1 + HALO).min(w), (y1 + HALO).min(h));
                let (rw, rh) = (rx1 - rx0, ry1 - ry0);
                let mut region = vec![0f32; rw * rh * 3];
                for yy in 0..rh {
                    region[yy * rw * 3..(yy + 1) * rw * 3].copy_from_slice(&rgb[((ry0 + yy) * w + rx0) * 3..((ry0 + yy) * w + rx0 + rw) * 3]);
                }
                let out = run_tile(model, &region, rh, rw, use_avx2);
                // 余白を除いた中心部だけを切り出す。
                let (cw, ch) = (x1 - tx, y1 - ty);
                let mut core = vec![0f32; cw * r * ch * r * 3];
                for yy in 0..ch * r {
                    let src_y = (ty - ry0) * r + yy;
                    let src = &out[(src_y * rw * r + (tx - rx0) * r) * 3..][..cw * r * 3];
                    core[yy * cw * r * 3..(yy + 1) * cw * r * 3].copy_from_slice(src);
                }
                results.lock().unwrap().push((tx, ty, cw, ch, core));
            });
        }
    });
    let mut full = vec![0f32; w * r * h * r * 3];
    for (tx, ty, cw, ch, core) in results.into_inner().unwrap() {
        for yy in 0..ch * r {
            let dst = ((ty * r + yy) * w * r + tx * r) * 3;
            full[dst..dst + cw * r * 3].copy_from_slice(&core[yy * cw * r * 3..(yy + 1) * cw * r * 3]);
        }
    }
    full
}

/// タイル処理のAI拡大(ネットワークの倍率)に、x2/x3用の後段リサイズを加えた最終結果`(データ, 幅, 高さ)`を返す。
pub fn upscale_full(model: &SrModel, rgb: &[f32], w: usize, h: usize) -> (Vec<f32>, usize, usize) {
    upscale_full_threads(model, rgb, w, h, 0)
}

/// `upscale_full`のスレッド数指定版(`0`=全論理コア)。
pub fn upscale_full_threads(model: &SrModel, rgb: &[f32], w: usize, h: usize, threads: usize) -> (Vec<f32>, usize, usize) {
    let up = upscale_rgb_threads(model, rgb, w, h, threads);
    let (nw, nh) = (w * model.scale, h * model.scale);
    match model.post_resize {
        Some(_) => {
            let (ow, oh) = model.output_size(w, h);
            (bicubic_resize(&up, nw, nh, ow, oh), ow, oh)
        }
        None => (up, nw, nh),
    }
}

/// PNG等の画像ファイルをCPUで超解像して`output`(PNG)へ保存する。
pub fn upscale_image_file(model: &SrModel, input: &Path, output: &Path) -> Result<(), String> {
    let img = image::open(input).map_err(|e| format!("画像を開けません({}): {e}", input.display()))?.to_rgb8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    let rgb: Vec<f32> = img.as_raw().iter().map(|&b| b as f32 / 255.0).collect();
    let (up, uw, uh) = upscale_full(model, &rgb, w, h);
    let bytes: Vec<u8> = up.iter().map(|&v| (v * 255.0 + 0.5) as u8).collect();
    let (ow, oh) = (uw as u32, uh as u32);
    image::RgbImage::from_raw(ow, oh, bytes).ok_or("出力画像の組み立てに失敗しました")?.save(output).map_err(|e| format!("画像を保存できません: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn models_dir() -> Option<std::path::PathBuf> {
        let dir = crate::engine::plugins::plugin_dir()?.join("realesrgan").join(crate::engine::ai_upscale::REALESRGAN_VERSION).join("models");
        dir.join("realesr-animevideov3-x4.param").is_file().then_some(dir)
    }

    #[test]
    fn half_precision_conversion_is_exact_for_known_values() {
        assert_eq!(half_to_f32(0x3c00), 1.0);
        assert_eq!(half_to_f32(0xc000), -2.0);
        assert_eq!(half_to_f32(0x0000), 0.0);
        assert_eq!(half_to_f32(0x3555), 0.333_251_95);
        assert!(half_to_f32(0x7c00).is_infinite());
    }

    #[test]
    fn rejects_non_ncnn_or_unsupported_models() {
        assert!(parse_model("hello", &[]).is_err());
        assert!(parse_model("7767517\n1 1\nLSTM x 1 1 a b\n", &[]).is_err());
    }

    /// 実モデルを読み込み、構造・重みの全バイトが過不足なく使われることを検証する。
    #[test]
    fn real_model_loads_with_every_byte_consumed() {
        let Some(dir) = models_dir() else {
            eprintln!("Real-ESRGANのモデルが未導入のためスキップ(ai_upscaleのテストで導入される)");
            return;
        };
        for scale in [2u32, 3, 4] {
            let m = load_model(&dir, scale).unwrap_or_else(|e| panic!("x{scale}: {e}"));
            assert_eq!(m.scale(), 4, "ネットワーク自体は常に4倍");
            assert_eq!(m.output_size(100, 60), (100 * scale as usize, 60 * scale as usize), "x{scale}の最終出力サイズ");
            assert_eq!(m.layers.len(), 18);
            assert_eq!(m.layers[0].in_c, 3);
            assert_eq!(m.layers[17].out_c, 48);
        }
    }

    /// AVX2カーネルとスカラー参照実装が同じ結果を出すこと(実モデルの重みで)。
    #[test]
    fn avx2_kernel_matches_the_scalar_reference() {
        let Some(dir) = models_dir() else { return };
        if !avx2_available() {
            eprintln!("AVX2+FMAが無いためスキップ");
            return;
        }
        let m = load_model(&dir, 4).unwrap();
        let (h, w) = (13usize, 19usize); // 4の倍数でない幅で余りの画素経路も通す
        let rgb: Vec<f32> = (0..h * w * 3).map(|i| ((i * 37 % 255) as f32) / 255.0).collect();
        let fast = run_tile(&m, &rgb, h, w, true);
        let slow = run_tile(&m, &rgb, h, w, false);
        let max_diff = fast.iter().zip(&slow).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
        assert!(max_diff < 1e-3, "AVX2とスカラーの出力差が小さいはず(最大差 {max_diff})");
    }

    /// 自前のCPU推論が、公式実装(ncnn-vulkan、GPU)と同じ結果を出すこと。
    /// 同じ実画像(公式リリース同梱のinput.jpg)を両方で4倍にして、出力同士のPSNRを測る。
    /// (GPUはfp16で計算するため完全一致はしないが、高いPSNRになるはず。GPUが無い環境ではスキップ)
    #[test]
    fn cpu_output_matches_the_official_gpu_implementation() {
        let Some(dir) = models_dir() else { return };
        let root = dir.parent().unwrap().to_path_buf();
        let exe = root.join(format!("realesrgan-ncnn-vulkan{}", std::env::consts::EXE_SUFFIX));
        let input = root.join("input.jpg");
        if !exe.is_file() || !input.is_file() || !crate::engine::ai_upscale::gpu_usable(&exe) {
            eprintln!("公式実装(GPU)を実行できないためスキップ");
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_cpu_vs_gpu_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let gpu_out = tmp.join("gpu.png");
        let ok = std::process::Command::new(&exe)
            .args(["-i", input.to_str().unwrap(), "-o", gpu_out.to_str().unwrap(), "-m", dir.to_str().unwrap(), "-n", "realesr-animevideov3", "-s", "4", "-f", "png"])
            .output()
            .unwrap();
        assert!(ok.status.success());
        let m = load_model(&dir, 4).unwrap();
        let cpu_out = tmp.join("cpu.png");
        upscale_image_file(&m, &input, &cpu_out).unwrap();
        let (g, c) = (image::open(&gpu_out).unwrap().to_rgb8(), image::open(&cpu_out).unwrap().to_rgb8());
        assert_eq!((g.width(), g.height()), (c.width(), c.height()), "出力サイズが一致するはず");
        let mse: f64 = g.as_raw().iter().zip(c.as_raw()).map(|(a, b)| { let d = *a as f64 - *b as f64; d * d }).sum::<f64>() / g.as_raw().len() as f64;
        let psnr = if mse == 0.0 { 99.0 } else { 10.0 * (255.0f64 * 255.0 / mse).log10() };
        let _ = std::fs::remove_dir_all(&tmp);
        eprintln!("CPU版とGPU版(公式)の出力PSNR: {psnr:.1} dB");
        assert!(psnr > 38.0, "自前のCPU推論が公式実装とほぼ同じ結果になるはず(PSNR {psnr:.1} dB)");
    }

    /// 720×480の1フレームを実際にCPUで4倍超解像し、所要時間を報告する(判定はしない)。
    #[test]
    fn cpu_frame_speed_report() {
        let Some(dir) = models_dir() else { return };
        let m = load_model(&dir, 4).unwrap();
        let (h, w) = (480usize, 720usize);
        let rgb: Vec<f32> = (0..h * w * 3).map(|i| (((i * 31) ^ (i / 7)) % 255) as f32 / 255.0).collect();
        let t = std::time::Instant::now();
        let (out, ow, oh) = upscale_full(&m, &rgb, w, h);
        let secs = t.elapsed().as_secs_f64();
        eprintln!("CPU超解像 720x480 -> {ow}x{oh}: {secs:.2}秒 (カーネル: {}, スレッド: {})", kernel_name(), std::thread::available_parallelism().map_or(1, |n| n.get()));
        assert_eq!(out.len(), ow * oh * 3);
    }

    /// タイル分割の結果が、画像全体を1タイルで処理した結果と一致すること(余白が十分なことの検証)。
    #[test]
    fn tiled_result_matches_single_tile_processing() {
        let Some(dir) = models_dir() else { return };
        let m = load_model(&dir, 4).unwrap();
        let (h, w) = (135usize, 131usize); // TILE(128)を超えるので複数タイルになる
        let rgb: Vec<f32> = (0..h * w * 3).map(|i| (((i * 31) ^ (i / 7)) % 255) as f32 / 255.0).collect();
        let tiled = upscale_rgb(&m, &rgb, w, h);
        let whole = run_tile(&m, &rgb, h, w, avx2_available());
        let max_diff = tiled.iter().zip(&whole).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
        assert!(max_diff < 1e-3, "タイル処理と一括処理が一致するはず(最大差 {max_diff})");
    }
}
