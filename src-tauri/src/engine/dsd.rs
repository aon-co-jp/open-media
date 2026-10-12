//! DSD(Direct Stream Digital)出力(2026-09-19新設)。DSD64/128/256/512/1024。
//!
//! ## 設計
//!
//! ffmpegはDSDの**デコードのみ**でエンコード(書き出し)機能が無い(実機確認済み:
//! `-encoders`/`-muxers`にDSD関連なし)ため、PCM→1bit変換(ΔΣ変調)とDSFファイルの
//! 書き出しを自前で実装する:
//! 1. ffmpegで元音声をDSDレート(44.1kHz×64〜1024)の32bit floatへリサンプルし
//!    パイプで受け取る(ストリーミング、全体をメモリに載せない)。
//! 2. 5次のΔΣ変調器(Butterworth型NTF、最大ゲイン1.5=Lee基準)で1bitへ量子化。
//! 3. DSF形式(LSBファースト、4096バイトのブロック/チャンネルのインターリーブ)で保存。
//!
//! **正直な開示**: (a) DSD1024は45MHz×2chで1時間あたり約41GB、DSD64でも約2.5GB/時間と
//! ファイルが非常に大きい(CD 700MBではDSD64でも約16分、詳細はUIに表示)。(b) 出力は
//! DSFファイルであり、Super Audio CD(SACD)のディスク規格(ISO/著作権保護付き)の
//! オーサリングではない。(c) ΔΣ変調のため音量は-6dB(0.5倍)に抑えて安定性を確保している。
//! (d) 高レートほど変換が遅い(DSD1024は実時間よりかなり遅くなり得る)。

use crate::engine::sidecar::resolve_tool;
use std::io::{Read, Seek, SeekFrom, Write};
use std::process::Stdio;

/// DSD64のサンプリング周波数(44.1kHz × 64)。
pub const DSD64_HZ: u32 = 2_822_400;

/// `64`/`128`/`256`/`512`/`1024`から実際のサンプリング周波数(Hz)を返す。
pub fn dsd_sample_rate(multiplier: u32) -> Result<u32, String> {
    match multiplier {
        64 | 128 | 256 | 512 | 1024 => Ok(DSD64_HZ / 64 * multiplier),
        other => Err(format!("未対応のDSDレートです: DSD{other}(64/128/256/512/1024のみ) / unsupported DSD rate")),
    }
}

/// DSD出力のおおよそのファイルサイズ(バイト、ステレオ2ch想定)。
pub fn estimate_dsd_size_bytes(multiplier: u32, channels: u32, duration_secs: f64) -> Result<u64, String> {
    let rate = dsd_sample_rate(multiplier)? as f64;
    Ok((rate * channels as f64 / 8.0 * duration_secs.max(0.0)) as u64)
}

const NTF_ORDER: usize = 5;
/// NTFの高域ゲイン上限(Lee基準の目安。大きいほど帯域内ノイズは減るが不安定になる)。
const NTF_MAX_GAIN: f64 = 1.5;
/// 入力を0.5倍(-6dB)にして変調器を安定させる。
const INPUT_SCALE: f64 = 0.5;

#[derive(Clone, Copy)]
struct Complex {
    re: f64,
    im: f64,
}

impl Complex {
    fn mul(self, o: Complex) -> Complex {
        Complex { re: self.re * o.re - self.im * o.im, im: self.re * o.im + self.im * o.re }
    }
    fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }
}

/// Butterworthハイパス型のNTF(分子`(1-z^-1)^N`、モニック)を設計し、`(b, a)`を返す
/// (どちらも`z^-k`の係数、`b[0]=a[0]=1`)。`fc`は正規化カットオフ(fs=1)。
fn design_ntf(fc: f64) -> (Vec<f64>, Vec<f64>, f64) {
    let n = NTF_ORDER;
    // 正規化アナログButterworthローパスの極(左半面)→ハイパス化 → 双一次変換。
    let wc = 2.0 * (std::f64::consts::PI * fc).tan();
    let mut z_poles = Vec::with_capacity(n);
    for k in 0..n {
        let theta = std::f64::consts::PI * (2 * k + n + 1) as f64 / (2 * n) as f64;
        let s = Complex { re: theta.cos(), im: theta.sin() };
        // ハイパス: p = wc / s
        let denom = s.re * s.re + s.im * s.im;
        let p = Complex { re: wc * s.re / denom, im: -wc * s.im / denom };
        // 双一次変換(T=1): z = (2 + p) / (2 - p)
        let num = Complex { re: 2.0 + p.re, im: p.im };
        let den = Complex { re: 2.0 - p.re, im: -p.im };
        let d2 = den.re * den.re + den.im * den.im;
        z_poles.push(Complex { re: (num.re * den.re + num.im * den.im) / d2, im: (num.im * den.re - num.re * den.im) / d2 });
    }
    // A(z^-1) = Π (1 - z_p z^-1)
    let mut poly = vec![Complex { re: 1.0, im: 0.0 }];
    for zp in &z_poles {
        let mut next = vec![Complex { re: 0.0, im: 0.0 }; poly.len() + 1];
        for (i, c) in poly.iter().enumerate() {
            next[i].re += c.re;
            next[i].im += c.im;
            let t = c.mul(Complex { re: -zp.re, im: -zp.im });
            next[i + 1].re += t.re;
            next[i + 1].im += t.im;
        }
        poly = next;
    }
    let a: Vec<f64> = poly.iter().map(|c| c.re).collect();
    // B(z^-1) = (1 - z^-1)^N (二項係数)
    let mut b = vec![1.0];
    for _ in 0..n {
        let mut next = vec![0.0; b.len() + 1];
        for (i, c) in b.iter().enumerate() {
            next[i] += c;
            next[i + 1] -= c;
        }
        b = next;
    }
    // ナイキストでのゲイン |B(-1)| / |A(-1)| = 2^N / Π|1 + z_p|
    let gain = 2f64.powi(n as i32) / z_poles.iter().map(|zp| Complex { re: 1.0 + zp.re, im: zp.im }.abs()).product::<f64>();
    (b, a, gain)
}

/// 高域ゲインが`NTF_MAX_GAIN`になるようカットオフを二分探索してNTFを設計する。
fn design_ntf_for_max_gain() -> (Vec<f64>, Vec<f64>) {
    let (mut lo, mut hi) = (1e-4_f64, 0.45_f64);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if design_ntf(mid).2 < NTF_MAX_GAIN {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (b, a, _) = design_ntf((lo + hi) / 2.0);
    (b, a)
}

/// 1チャンネル分のΔΣ変調器(状態を保持し、サンプルを順次1bitへ量子化する)。
/// 係数・履歴はすべて固定長配列で、ループは`NTF_ORDER`の定数で完全展開される(ヒープ/境界チェック無し)。
struct DeltaSigma {
    b: [f64; NTF_ORDER],
    a: [f64; NTF_ORDER],
    /// 過去の量子化誤差 e[n-1..n-N]
    e_hist: [f64; NTF_ORDER],
    /// 過去の (y-u)[n-1..n-N]
    yu_hist: [f64; NTF_ORDER],
}

impl DeltaSigma {
    fn new(b: &[f64], a: &[f64]) -> Self {
        let mut bb = [0.0; NTF_ORDER];
        let mut aa = [0.0; NTF_ORDER];
        bb.copy_from_slice(&b[1..=NTF_ORDER]);
        aa.copy_from_slice(&a[1..=NTF_ORDER]);
        Self { b: bb, a: aa, e_hist: [0.0; NTF_ORDER], yu_hist: [0.0; NTF_ORDER] }
    }

    /// 入力`u`(-1〜1)を1bit(true=+1)へ量子化する。
    /// (y-u)[n] = Σ b_k e[n-k] - Σ a_k (y-u)[n-k] を満たすようvを決める。
    #[inline(always)]
    fn step(&mut self, u: f64) -> bool {
        let mut shaped = 0.0;
        for k in 0..NTF_ORDER {
            shaped += self.b[k] * self.e_hist[k] - self.a[k] * self.yu_hist[k];
        }
        let v = u + shaped;
        let y = if v >= 0.0 { 1.0 } else { -1.0 };
        for k in (1..NTF_ORDER).rev() {
            self.e_hist[k] = self.e_hist[k - 1];
            self.yu_hist[k] = self.yu_hist[k - 1];
        }
        self.e_hist[0] = y - v;
        self.yu_hist[0] = y - u;
        y > 0.0
    }
}

const DSF_BLOCK_BYTES: usize = 4096;

/// DSFのヘッダを書く(サイズ欄は後で`patch_dsf_sizes`で確定する)。
fn write_dsf_header<W: Write>(w: &mut W, sample_rate: u32, channels: u32) -> std::io::Result<()> {
    // 'DSD ' チャンク(28バイト)
    w.write_all(b"DSD ")?;
    w.write_all(&28u64.to_le_bytes())?;
    w.write_all(&0u64.to_le_bytes())?; // 総ファイルサイズ(後で更新)
    w.write_all(&0u64.to_le_bytes())?; // メタデータ位置(なし)
    // 'fmt ' チャンク(52バイト)
    w.write_all(b"fmt ")?;
    w.write_all(&52u64.to_le_bytes())?;
    w.write_all(&1u32.to_le_bytes())?; // フォーマットバージョン
    w.write_all(&0u32.to_le_bytes())?; // フォーマットID(DSD raw)
    w.write_all(&(if channels == 1 { 1u32 } else { 2u32 }).to_le_bytes())?; // チャンネルタイプ(1=mono,2=stereo)
    w.write_all(&channels.to_le_bytes())?;
    w.write_all(&sample_rate.to_le_bytes())?;
    w.write_all(&1u32.to_le_bytes())?; // 1サンプル1bit、LSBファースト
    w.write_all(&0u64.to_le_bytes())?; // チャンネルあたりのサンプル数(後で更新)
    w.write_all(&(DSF_BLOCK_BYTES as u32).to_le_bytes())?;
    w.write_all(&0u32.to_le_bytes())?; // 予約
    // 'data' チャンク(サイズは後で更新)
    w.write_all(b"data")?;
    w.write_all(&0u64.to_le_bytes())?;
    Ok(())
}

fn patch_dsf_sizes<F: Write + Seek>(f: &mut F, total_samples_per_channel: u64, data_bytes: u64) -> std::io::Result<()> {
    let file_size = 28 + 52 + 12 + data_bytes;
    f.seek(SeekFrom::Start(12))?;
    f.write_all(&file_size.to_le_bytes())?;
    f.seek(SeekFrom::Start(28 + 36))?;
    f.write_all(&total_samples_per_channel.to_le_bytes())?;
    f.seek(SeekFrom::Start(28 + 52 + 4))?;
    f.write_all(&(data_bytes + 12).to_le_bytes())?;
    Ok(())
}

/// `input_path`をDSD(`multiplier`=64〜1024)のDSFファイルとして`output_path`へ書き出す。
/// `trim`(開始秒, 長さ秒)を指定するとその区間だけを変換する。
pub fn convert_to_dsf(input_path: &str, output_path: &str, multiplier: u32, trim: Option<(Option<f64>, Option<f64>)>) -> Result<(), String> {
    let sample_rate = dsd_sample_rate(multiplier)?;
    let channels: u32 = 2;

    let mut cmd = resolve_tool("ffmpeg");
    cmd.args(["-v", "error"]);
    if let Some((Some(s), _)) = trim {
        cmd.args(["-ss", &s.to_string()]);
    }
    cmd.args(["-i", input_path]);
    if let Some((_, Some(d))) = trim {
        cmd.args(["-t", &d.to_string()]);
    }
    // 音質最優先: 使える最高品質のリサンプラ(soxr、無ければswresampleの高精度設定)で
    // DSDレートまで一気に補間する(標準設定より鏡像成分の除去が良い)。
    let resample = format!("{}:out_sample_rate={sample_rate}", crate::engine::convert::hq_resample_filter());
    cmd.args(["-vn", "-ac", &channels.to_string(), "-af", &resample, "-f", "f32le", "-"]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("ffmpegの起動に失敗しました: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("ffmpegの出力を取得できませんでした")?;

    let (b, a) = design_ntf_for_max_gain();
    let mut modulators: Vec<DeltaSigma> = (0..channels).map(|_| DeltaSigma::new(&b, &a)).collect();

    let file = std::fs::File::create(output_path).map_err(|e| format!("出力ファイルの作成に失敗しました: {e}"))?;
    let mut out = std::io::BufWriter::with_capacity(1 << 20, file);
    write_dsf_header(&mut out, sample_rate, channels).map_err(|e| format!("DSFヘッダの書き込みに失敗しました: {e}"))?;

    // チャンネルごとのブロックバッファ(ビットをLSBファーストで詰める)
    let mut blocks: Vec<Vec<u8>> = (0..channels).map(|_| Vec::with_capacity(DSF_BLOCK_BYTES)).collect();
    let mut cur_byte = [0u8; 2];
    let mut bit_pos: u32 = 0;
    let mut total_samples: u64 = 0;
    let mut data_bytes: u64 = 0;

    let frame_bytes = 4 * channels as usize;
    let mut buf = vec![0u8; frame_bytes * 262_144];
    let mut carry = 0usize;
    loop {
        // 並列化の効率のため、バッファが満杯かEOFになるまで読み溜める(パイプの1回の読み取りは小さい)。
        let mut n = 0;
        while carry + n < buf.len() {
            let r = stdout.read(&mut buf[carry + n..]).map_err(|e| format!("ffmpegの出力の読み取りに失敗しました: {e}"))?;
            if r == 0 {
                break;
            }
            n += r;
        }
        if n == 0 {
            break;
        }
        let avail = carry + n;
        let frames = avail / frame_bytes;
        // ΔΣ変調は過去の出力に依存する逐次処理でGPU並列化が効かない。ビット列を逐次版と完全に一致させるため
        // (区間に分割して並列化すると、区間境界で雑音の低域の積分状態が食い違い、実測でSNRが99→51dBに劣化した)、
        // 時間方向には分割せず、チャンネルごとに別スレッドで並列に変調する。
        let chunk = &buf[..frames * frame_bytes];
        let bits: Vec<Vec<bool>> = std::thread::scope(|s| {
            let handles: Vec<_> = modulators
                .iter_mut()
                .enumerate()
                .map(|(ch, m)| {
                    s.spawn(move || {
                        crate::engine::sidecar::lower_current_thread_priority();
                        (0..frames)
                            .map(|f| {
                                let o = f * frame_bytes + ch * 4;
                                let sample = f32::from_le_bytes([chunk[o], chunk[o + 1], chunk[o + 2], chunk[o + 3]]) as f64;
                                m.step((sample * INPUT_SCALE).clamp(-1.0, 1.0))
                            })
                            .collect::<Vec<bool>>()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("modulator thread panicked")).collect()
        });
        #[allow(clippy::needless_range_loop)]
        for f in 0..frames {
            for ch in 0..channels as usize {
                if bits[ch][f] {
                    cur_byte[ch] |= 1 << bit_pos;
                }
            }
            bit_pos += 1;
            total_samples += 1;
            if bit_pos == 8 {
                for ch in 0..channels as usize {
                    blocks[ch].push(cur_byte[ch]);
                    cur_byte[ch] = 0;
                }
                bit_pos = 0;
                if blocks[0].len() == DSF_BLOCK_BYTES {
                    for blk in blocks.iter_mut() {
                        out.write_all(blk).map_err(|e| format!("書き込みに失敗しました: {e}"))?;
                        data_bytes += blk.len() as u64;
                        blk.clear();
                    }
                }
            }
        }
        let used = frames * frame_bytes;
        buf.copy_within(used..avail, 0);
        carry = avail - used;
    }

    // 端数のバイト・ブロックを0パディングして書き出す。
    if bit_pos != 0 {
        for ch in 0..channels as usize {
            blocks[ch].push(cur_byte[ch]);
        }
    }
    if !blocks[0].is_empty() {
        for blk in blocks.iter_mut() {
            blk.resize(DSF_BLOCK_BYTES, 0);
            out.write_all(blk).map_err(|e| format!("書き込みに失敗しました: {e}"))?;
            data_bytes += blk.len() as u64;
        }
    }

    let status = child.wait().map_err(|e| format!("ffmpegの終了待ちに失敗しました: {e}"))?;
    if !status.success() {
        let mut err = String::new();
        if let Some(mut se) = child.stderr.take() {
            let _ = se.read_to_string(&mut err);
        }
        return Err(format!("ffmpegによるDSD用リサンプルが失敗しました: {err}"));
    }
    if total_samples == 0 {
        return Err("変換する音声データがありませんでした / no audio data to convert".to_string());
    }

    let mut file = out.into_inner().map_err(|e| format!("ファイルのフラッシュに失敗しました: {e}"))?;
    patch_dsf_sizes(&mut file, total_samples, data_bytes).map_err(|e| format!("DSFヘッダの更新に失敗しました: {e}"))?;
    Ok(())
}

/// DSFを**DoP(DSD over PCM)**の24bit WAVへ変換する(2026-09-19、open-mqaと融合)。
/// DoPはDSDをPCMの入れ物(DSDレート/16のPCM、上位8bitがマーカー)に詰める方式で、DSF非対応でも
/// DoP対応のDACとプレーヤー(ビットパーフェクト再生)なら本物のDSD再生ができる。
/// **正直な開示**: 音量調整・SRC・ミキサーを通る再生ではDSDが壊れノイズになる。DoP非対応DACでは使えない。
///
/// `container_bits`は24(DoP標準)または32。32bitは24bitのDoPデータを上位に左詰めした非標準寄りの入れ物で、
/// 32bit出力のDAC/ドライバ経路(WASAPI排他32bit等)を使う環境向け(下位8bitは0)。対応DACはマーカーで判別する。
/// DoPのPCMレートはDSDレート/16で、DSD64=176.4k・DSD128=352.8k・DSD256=705.6k・DSD512=1411.2kHz
/// (44.1kHz系のみ。384kHzは48kHz系DSDの入れ物で、本ツールのDSDレートとは一致しない)。
pub fn dsf_to_dop_wav(dsf_path: &str, wav_path: &str, container_bits: u8) -> Result<(), String> {
    if container_bits != 24 && container_bits != 32 {
        return Err("DoPのコンテナは24bitまたは32bitのみです".to_string());
    }
    use open_mqa::dop::{pack_dop_frames, DopConfig, DsdFormat};
    let mut f = std::io::BufReader::new(std::fs::File::open(dsf_path).map_err(|e| format!("DSFを開けません: {e}"))?);
    let mut head = [0u8; 92];
    f.read_exact(&mut head).map_err(|e| format!("DSFヘッダを読めません: {e}"))?;
    if &head[0..4] != b"DSD " {
        return Err("DSFファイルではありません".to_string());
    }
    let channels = u32::from_le_bytes(head[52..56].try_into().unwrap()) as usize;
    let rate = u32::from_le_bytes(head[56..60].try_into().unwrap());
    let total_samples = u64::from_le_bytes(head[64..72].try_into().unwrap());
    let cfg = DopConfig { format: DsdFormat { dsd_bitrate_hz: rate }, container_bits: 24 };
    let pcm_rate = cfg.format.dop_pcm_sample_rate_hz();
    let mut valid_bytes = total_samples.div_ceil(8) as usize;
    valid_bytes += valid_bytes % 2; // DoPは1フレーム=DSD 2バイト
    let mut out = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(wav_path).map_err(|e| format!("出力を作成できません: {e}"))?);
    let header = open_mqa::wav::encode_wav(&vec![Vec::new(); channels], pcm_rate, container_bits).map_err(|e| e.to_string())?;
    out.write_all(&header).map_err(|e| e.to_string())?;
    let mut data_bytes: u64 = 0;
    let mut remaining = valid_bytes;
    let mut block = vec![0u8; DSF_BLOCK_BYTES * channels];
    while remaining > 0 {
        f.read_exact(&mut block).map_err(|e| format!("DSFデータを読めません: {e}"))?;
        let take = remaining.min(DSF_BLOCK_BYTES);
        let per_ch: Vec<Vec<[u8; 3]>> = (0..channels)
            .map(|ch| {
                // DSFはLSBファースト、DoPは時間順のMSBファーストなのでビットを反転する。
                let bytes: Vec<u8> = block[ch * DSF_BLOCK_BYTES..][..take].iter().map(|b| b.reverse_bits()).collect();
                pack_dop_frames(&bytes, &cfg).map_err(|e| e.to_string())
            })
            .collect::<Result<_, _>>()?;
        for i in 0..per_ch[0].len() {
            for ch in &per_ch {
                let fr = ch[i];
                if container_bits == 32 {
                    out.write_all(&[0, fr[2], fr[1], fr[0]]).map_err(|e| e.to_string())?;
                    data_bytes += 4;
                } else {
                    out.write_all(&[fr[2], fr[1], fr[0]]).map_err(|e| e.to_string())?;
                    data_bytes += 3;
                }
            }
        }
        remaining -= take;
    }
    let mut file = out.into_inner().map_err(|e| e.to_string())?;
    let len = header.len() as u64;
    file.seek(SeekFrom::Start(4)).map_err(|e| e.to_string())?;
    file.write_all(&((len - 8 + data_bytes + (data_bytes & 1)) as u32).to_le_bytes()).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(len - 4)).map_err(|e| e.to_string())?;
    file.write_all(&(data_bytes as u32).to_le_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn dsd_rates_are_multiples_of_44_1khz() {
        assert_eq!(dsd_sample_rate(64).unwrap(), 2_822_400);
        assert_eq!(dsd_sample_rate(128).unwrap(), 5_644_800);
        assert_eq!(dsd_sample_rate(256).unwrap(), 11_289_600);
        assert_eq!(dsd_sample_rate(512).unwrap(), 22_579_200);
        assert_eq!(dsd_sample_rate(1024).unwrap(), 45_158_400);
        assert!(dsd_sample_rate(32).is_err());
    }

    #[test]
    fn size_estimate_matches_known_dsd64_stereo_rate() {
        // DSD64ステレオは約705.6kB/秒。
        let bytes = estimate_dsd_size_bytes(64, 2, 1.0).unwrap();
        assert_eq!(bytes, 705_600);
    }

    #[test]
    fn ntf_is_monic_and_stable() {
        let (b, a) = design_ntf_for_max_gain();
        assert_eq!((b[0], a[0]), (1.0, 1.0));
        assert_eq!(b.len(), NTF_ORDER + 1);
        // 極が単位円内(安定)かを、インパルス応答が減衰することで確認する。
        let mut y = vec![0.0f64; 4096];
        for n in 0..y.len() {
            let mut acc = if n == 0 { 1.0 } else { 0.0 };
            for k in 1..a.len() {
                if n >= k {
                    acc -= a[k] * y[n - k];
                }
            }
            y[n] = acc;
        }
        assert!(y[4000..].iter().all(|v| v.abs() < 1e-6), "NTFの極は単位円内のはず");
    }

    /// 最適化したΔΣカーネルが、素直な参照実装とビット完全に一致すること(高速化で結果が変わっていない証明)。
    #[test]
    fn optimized_kernel_is_bit_identical_to_the_reference_implementation() {
        let (b, a) = design_ntf_for_max_gain();
        let n = 200_000;
        let mut rng = 0x1234_5678_9abc_def0u64;
        let x: Vec<f64> = (0..n)
            .map(|i| {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                0.4 * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 2_822_400.0).sin() + 0.01 * ((rng >> 11) as f64 / (1u64 << 53) as f64 - 0.5)
            })
            .collect();
        // 参照: 配列シフト方式の素直な実装(最適化前のコードそのまま)
        let mut e_hist = [0.0f64; NTF_ORDER];
        let mut yu_hist = [0.0f64; NTF_ORDER];
        let mut reference = Vec::with_capacity(n);
        for &u in &x {
            let mut shaped = 0.0;
            for k in 1..=NTF_ORDER {
                shaped += b[k] * e_hist[k - 1] - a[k] * yu_hist[k - 1];
            }
            let v = u + shaped;
            let y = if v >= 0.0 { 1.0 } else { -1.0 };
            for k in (1..NTF_ORDER).rev() {
                e_hist[k] = e_hist[k - 1];
                yu_hist[k] = yu_hist[k - 1];
            }
            e_hist[0] = y - v;
            yu_hist[0] = y - u;
            reference.push(y > 0.0);
        }
        let mut m = DeltaSigma::new(&b, &a);
        let fast: Vec<bool> = x.iter().map(|&u| m.step(u)).collect();
        assert_eq!(reference, fast);
    }

    /// DSF→DoP WAVで、DSDのビット列がマーカー込みで完全に保存されること。
    #[test]
    fn dop_wav_preserves_the_dsd_bitstream_exactly() {
        if !ffmpeg_ok() {
            return;
        }
        let dir = std::env::temp_dir();
        let wav = dir.join(format!("make_disk_dop_src_{}.wav", std::process::id()));
        let dsf = dir.join(format!("make_disk_dop_{}.dsf", std::process::id()));
        let dop = dir.join(format!("make_disk_dop_{}.dop.wav", std::process::id()));
        write_exact_sine_wav(&wav, 1000.0, 0.5, 0.5, 44_100, 2);
        convert_to_dsf(wav.to_str().unwrap(), dsf.to_str().unwrap(), 64, None).unwrap();
        dsf_to_dop_wav(dsf.to_str().unwrap(), dop.to_str().unwrap(), 24).unwrap();
        // 32bitコンテナ: 24bit版と同じDoPデータが上位24bitに左詰めされ、下位8bitは0であること。
        let dop32 = dir.join(format!("make_disk_dop_{}.dop32.wav", std::process::id()));
        dsf_to_dop_wav(dsf.to_str().unwrap(), dop32.to_str().unwrap(), 32).unwrap();
        let a24 = open_mqa::wav::decode_wav(&std::fs::read(&dop).unwrap()).unwrap();
        let a32 = open_mqa::wav::decode_wav(&std::fs::read(&dop32).unwrap()).unwrap();
        assert_eq!(a32.bits_per_sample, 32);
        assert_eq!(a24.sample_rate, a32.sample_rate);
        for ch in 0..2 {
            assert_eq!(a24.samples_per_channel[ch].len(), a32.samples_per_channel[ch].len());
            assert!(a24.samples_per_channel[ch].iter().zip(&a32.samples_per_channel[ch]).all(|(x, y)| *y == *x << 8), "ch{ch}: 32bit=24bit<<8");
        }
        let _ = std::fs::remove_file(&dop32);
        let dsf_bytes = std::fs::read(&dsf).unwrap();
        let samples = u64::from_le_bytes(dsf_bytes[64..72].try_into().unwrap());
        let valid = samples.div_ceil(8) as usize;
        let frames = open_mqa::wav::decode_dop_wav(&std::fs::read(&dop).unwrap()).unwrap();
        assert_eq!(frames.len(), 2);
        for ch in 0..2 {
            let back = open_mqa::dop::unpack_dop_frames(&frames[ch]).unwrap();
            let expected: Vec<u8> = dsf_bytes[92 + ch * DSF_BLOCK_BYTES..].chunks(DSF_BLOCK_BYTES * 2).flat_map(|c| c[..DSF_BLOCK_BYTES.min(c.len())].iter().copied()).take(valid).map(|b| b.reverse_bits()).collect();
            assert_eq!(&back[..valid], &expected[..], "ch{ch}");
        }
        assert_eq!(open_mqa::wav::decode_wav(&std::fs::read(&dop).unwrap()).unwrap().sample_rate, 176_400, "DSD64のDoPは176.4kHz");
        for p in [wav, dsf, dop] {
            let _ = std::fs::remove_file(p);
        }
    }

    fn ffmpeg_ok() -> bool {
        Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
    }

    /// 1kHzサイン波→DSD64/128→(ffmpegのDSFデコーダで)PCMへ戻し、元の正弦波と
    /// 比べてSNRを測る実機E2E検証。
    fn roundtrip_snr_db(multiplier: u32) -> f64 {
        let tmp = std::env::temp_dir().join(format!("make_disk_dsd_{}_{}", multiplier, std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let src = tmp.join("sine.wav");
        write_exact_sine_wav(&src, 1000.0, 0.875, 1.0, 44100, 2);
        let dsf = tmp.join("out.dsf");
        convert_to_dsf(src.to_str().unwrap(), dsf.to_str().unwrap(), multiplier, None).expect("convert_to_dsf");

        let header = std::fs::read(&dsf).unwrap();
        assert_eq!(&header[0..4], b"DSD ");
        assert_eq!(&header[28..32], b"fmt ");
        assert_eq!(&header[80..84], b"data");

        // ffmpegのDSFデコーダで44.1kHzのfloatへ戻す。
        let dec = Command::new("ffmpeg")
            .args(["-v", "error", "-i", dsf.to_str().unwrap(), "-af", "pan=mono|c0=c0", "-ar", "44100", "-f", "f32le", "-"])
            .output()
            .unwrap();
        assert!(dec.status.success(), "{}", String::from_utf8_lossy(&dec.stderr));
        let samples: Vec<f64> = dec.stdout.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64).collect();
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(samples.len() > 40000, "約1秒分のPCMに戻るはず(実際: {})", samples.len());

        // 端の過渡応答を除いた中央部で、1kHzの正弦波を最小二乗フィットして残差からSNRを求める。
        let mid = &samples[8000..samples.len() - 8000];
        let w = 2.0 * std::f64::consts::PI * 1000.0 / 44100.0;
        let (mut sc, mut ss, mut cc, mut ss2, mut cs) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (i, &v) in mid.iter().enumerate() {
            let (s, c) = ((w * (i + 8000) as f64).sin(), (w * (i + 8000) as f64).cos());
            sc += v * c;
            ss += v * s;
            cc += c * c;
            ss2 += s * s;
            cs += c * s;
        }
        let det = cc * ss2 - cs * cs;
        let (ac, as_) = ((sc * ss2 - ss * cs) / det, (ss * cc - sc * cs) / det);
        let (mut sig, mut noise) = (0.0, 0.0);
        for (i, &v) in mid.iter().enumerate() {
            let fit = ac * (w * (i + 8000) as f64).cos() + as_ * (w * (i + 8000) as f64).sin();
            sig += fit * fit;
            noise += (v - fit) * (v - fit);
        }
        let amplitude = (ac * ac + as_ * as_).sqrt();
        assert!((amplitude - INPUT_SCALE * 0.875).abs() < 0.03, "振幅0.875(sine既定0.125×volume=7)の0.5倍=約0.4375のはず(実際: {amplitude})");
        10.0 * (sig / noise).log10()
    }

    #[test]
    fn real_dsd64_roundtrip_reproduces_the_sine_with_good_snr() {
        if !ffmpeg_ok() {
            return;
        }
        let snr = roundtrip_snr_db(64);
        eprintln!("DSD64 round-trip SNR: {snr:.1} dB");
        assert!(snr > 90.0, "DSD64でSNR 90dB超のはず(実測99.6dB、実際: {snr} dB)");
    }

    /// 全レート(64〜1024)で、出力サイズが計算式どおりで、所要時間を実測して報告する。
    #[test]
    fn real_all_dsd_rates_produce_correctly_sized_files_and_report_speed() {
        if !ffmpeg_ok() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_dsd_all_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let src = tmp.join("tone.wav");
        let st = Command::new("ffmpeg").args(["-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=2:sample_rate=44100", "-ac", "2", src.to_str().unwrap()]).output().unwrap();
        assert!(st.status.success());
        for mult in [64u32, 128, 256, 512, 1024] {
            let dsf = tmp.join(format!("out{mult}.dsf"));
            let t = std::time::Instant::now();
            convert_to_dsf(src.to_str().unwrap(), dsf.to_str().unwrap(), mult, None).expect("convert");
            let elapsed = t.elapsed().as_secs_f64();
            let size = std::fs::metadata(&dsf).unwrap().len();
            let samples = dsd_sample_rate(mult).unwrap() as u64 * 2; // 2秒分
            let blocks = samples.div_ceil(8).div_ceil(DSF_BLOCK_BYTES as u64);
            let expected = 92 + blocks * DSF_BLOCK_BYTES as u64 * 2;
            eprintln!("DSD{mult}: 2秒の音声を {elapsed:.2}秒で変換(実時間の{:.1}倍)、{:.1} MB", elapsed / 2.0, size as f64 / 1e6);
            assert_eq!(size, expected, "DSD{mult}のファイルサイズが計算式どおりのはず");
            let _ = std::fs::remove_file(&dsf);
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn real_dsd128_roundtrip_reproduces_the_sine_with_good_snr() {
        if !ffmpeg_ok() {
            return;
        }
        let snr = roundtrip_snr_db(128);
        eprintln!("DSD128 round-trip SNR: {snr:.1} dB");
        assert!(snr > 120.0, "DSD128でSNR 120dB超のはず(実測132.4dB、実際: {snr} dB)");
    }

    /// 実音源(環境変数`MAKE_DISK_DSD_IN`)をDSD256+DoP WAVへ変換し、DSFをffmpegでPCMに戻して元と比べる手動確認用。
    #[test]
    #[ignore]
    fn real_music_to_dsd256_and_dop() {
        let input = std::env::var("MAKE_DISK_DSD_IN").expect("set MAKE_DISK_DSD_IN");
        let dir = std::path::PathBuf::from(std::env::var("MAKE_DISK_DSD_OUT").expect("set MAKE_DISK_DSD_OUT"));
        std::fs::create_dir_all(&dir).unwrap();
        let dsf = dir.join("track.dsf");
        let started = std::time::Instant::now();
        let mult: u32 = std::env::var("MAKE_DISK_DSD_MULT").ok().and_then(|v| v.parse().ok()).unwrap_or(256);
        convert_to_dsf(&input, dsf.to_str().unwrap(), mult, None).unwrap();
        eprintln!("DSD256変換: {:.1}秒、{:.1} MB", started.elapsed().as_secs_f64(), std::fs::metadata(&dsf).unwrap().len() as f64 / 1e6);
        let dop = dir.join("track.dop.wav");
        dsf_to_dop_wav(dsf.to_str().unwrap(), dop.to_str().unwrap(), 24).unwrap();
        eprintln!("DoP WAV: {:.1} MB", std::fs::metadata(&dop).unwrap().len() as f64 / 1e6);
    }
}

/// 正弦波(`freq` Hz)を最小二乗フィットし、(SNR dB, 振幅)を返す。端の過渡応答は`skip`サンプル除外する。
/// DSD/高解像度PCMの往復・変換品質を実測するためのテスト用ユーティリティ。
#[cfg(test)]
pub(crate) fn sine_fit_snr_db(samples: &[f64], freq: f64, sample_rate: f64, skip: usize) -> (f64, f64) {
    let mid = &samples[skip..samples.len() - skip];
    let w = 2.0 * std::f64::consts::PI * freq / sample_rate;
    let (mut sc, mut ss, mut cc, mut s2, mut cs) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (i, &v) in mid.iter().enumerate() {
        let t = w * (i + skip) as f64;
        let (s, c) = (t.sin(), t.cos());
        sc += v * c;
        ss += v * s;
        cc += c * c;
        s2 += s * s;
        cs += c * s;
    }
    let det = cc * s2 - cs * cs;
    let (ac, a_s) = ((sc * s2 - ss * cs) / det, (ss * cc - sc * cs) / det);
    let (mut sig, mut noise) = (0.0, 0.0);
    for (i, &v) in mid.iter().enumerate() {
        let t = w * (i + skip) as f64;
        let fit = ac * t.cos() + a_s * t.sin();
        sig += fit * fit;
        noise += (v - fit) * (v - fit);
    }
    (10.0 * (sig / noise).log10(), (ac * ac + a_s * a_s).sqrt())
}

/// 周波数が厳密な正弦波(32bit float、`channels`ch)のWAVを書き出す。ffmpegの`sine`ソースは
/// 位相の固定小数点誤差で周波数が僅かにずれ、SNR測定の床(約78.8dB)になるため、
/// 変換品質の測定には使えない(実機で判明)。テスト用。
#[cfg(test)]
pub(crate) fn write_exact_sine_wav(path: &std::path::Path, freq: f64, amplitude: f64, seconds: f64, sample_rate: u32, channels: u16) {
    let frames = (seconds * sample_rate as f64) as usize;
    let data_bytes = (frames * channels as usize * 4) as u32;
    let mut out: Vec<u8> = Vec::with_capacity(44 + data_bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * channels as u32 * 4).to_le_bytes());
    out.extend_from_slice(&(channels * 4).to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());
    for n in 0..frames {
        let v = (amplitude * (2.0 * std::f64::consts::PI * freq * n as f64 / sample_rate as f64).sin()) as f32;
        for _ in 0..channels {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    std::fs::write(path, out).unwrap();
}
