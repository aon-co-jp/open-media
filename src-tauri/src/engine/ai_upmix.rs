//! AIによるアップミックス(モノラル/ステレオ→5.1ch/7.1ch、2026-09-28新設)。
//!
//! `convert::channel_layout_args`の単純な複製・按分によるpanフィルタ疑似サラウンドとは異なり、
//! 実在する音源分離モデル**HT-Demucs**(Meta、MITライセンス、[StemSplitio/htdemucs-onnx]
//! (https://huggingface.co/StemSplitio/htdemucs-onnx)配布のONNX版)で、音声を実際に
//! ボーカル/ドラム/ベース/その他(残響・伴奏)の4パートへ分離し、それぞれを異なるスピーカーへ
//! 配置する(センター=ボーカル、フロント=ドラム+ベース、リア=その他)。他の音声AI機能
//! (RNNoise/LavaSR)と同じく推論は純Rustの`tract`で行い、Pythonは使わない。
//!
//! ## 正直な開示(重要)
//! - モデル自体は実在するボーカル/ドラム/ベース/その他の**4パート分離**であり、5.1ch/7.1ch
//!   そのものを直接出力するわけではない。分離結果をどのスピーカーへ配置するかは
//!   本モジュールの設計(下記)であり、実際のディスクリート5.1ch/7.1ch制作物とは異なる。
//! - tractの標準のONNX `Pad`/`Range`演算子実装がこのモデルの一部の使い方(省略可能な
//!   `Pad`の第3入力、動的形状に依存する`Range`)に対応していなかったため、
//!   `tools/demucs-onnx-patch/patch_htdemucs_onnx.py`で事前にグラフを固定長
//!   (343,980サンプル=44.1kHzで約7.8秒)化・定数化したパッチ済みモデルを使う
//!   (onnxruntimeでは無印のモデルのまま問題なく動く。これはtract側の制約)。
//! - パッチ済みモデルは約2.2GB(.onnx本体+外部データ)と大きく、make-disk本体には同梱せず、
//!   利用者が別途用意したモデルフォルダを指す必要がある(自動ダウンロードは未実装、
//!   配布インフラ〈2GB超のためGitHub Releasesの標準上限を超える〉は次回課題)。
//!   モデルが無い/読み込めない環境では[`is_available`]が`false`を返し、
//!   呼び出し側(`convert.rs`)は既存のpanフィルタ疑似サラウンドへフォールバックする。
//! - 固定長(343,980サンプル)単位のチャンクに分けて処理し、最後の端数はゼロ埋めする
//!   (Demucs本来のオーバーラップ加算によるクロスフェードは行わないため、チャンクの
//!   境目でごく僅かな不連続が生じ得る近似)。
//!
//! ## GPU高速化(2026-09-28新設、DirectML)
//! Windowsでは、`ort`(Microsoft ONNXRuntimeのRustバインディング。C++実装のネイティブ
//! ライブラリを実行時に読み込むだけで、Pythonは使わない)経由でDirectML(AMD/NVIDIA/Intelを
//! 問わないWindows標準のGPUアクセラレーションAPI)を試す。GPUが無い/対応しない/
//! DirectMLの初期化に失敗する環境では、既存の`tract`(純Rust、CPU)推論へ自動的に
//! フォールバックする(`rife-ncnn-vulkan`のGPU失敗時CPUフォールバックと同じ設計)。
//! **正直な開示**: このGPU経路は、この開発機のGPU(GT 730、2026年時点で非常に旧式)では
//! 実際の高速化を測定できていない(ビルドは通るが、実機での速度・安定性の検証は
//! 将来もっと新しいGPUを用意してから行う必要がある、次回課題)。

use std::path::{Path, PathBuf};
use tract_onnx::prelude::*;

use crate::engine::convert::ChannelLayoutTarget;

/// 1チャンク分の推論を行うバックエンド(CPU=tract、GPU=ort/DirectML)の共通インタフェース。
trait UpmixBackend {
    fn run_chunk(&self, left: &[f32], right: &[f32]) -> Result<[[Vec<f32>; 2]; 4], String>;
    fn label(&self) -> &'static str;
}

/// パッチ済みモデルが1回に処理する固定入力長(サンプル数、44.1kHz換算で約7.8秒)。
/// `tools/demucs-onnx-patch/patch_htdemucs_onnx.py`の`FIXED_INPUT_SAMPLES`と一致させること。
const CHUNK_SAMPLES: usize = 343_980;
const MODEL_SR: u32 = 44_100;
/// モデル出力の4ステムの順序(demucs-onnxの`list-models`が報告する順序どおり)。
const STEM_ORDER: [&str; 4] = ["drums", "bass", "other", "vocals"];

fn model_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("MAKE_DISK_DEMUCS_MODEL_DIR") {
        return Some(PathBuf::from(dir));
    }
    crate::engine::plugins::plugin_dir().map(|d| d.join("ai-upmix").join("htdemucs"))
}

fn model_path() -> Option<PathBuf> {
    let dir = model_dir()?;
    let path = dir.join("htdemucs_patched.onnx");
    path.is_file().then_some(path)
}

/// このマシンでAIアップミックスが実際に使えるか(モデルファイルが用意されているか)の軽い確認。
/// 実際にモデルを読み込むわけではないので高速。
pub fn is_available() -> bool {
    model_path().is_some()
}

type Plan = std::sync::Arc<TypedRunnableModel>;

struct TractBackend {
    plan: Plan,
}

impl TractBackend {
    fn load() -> Result<Self, String> {
        let path = model_path().ok_or("AIアップミックスのモデルが見つかりません(MAKE_DISK_DEMUCS_MODEL_DIRを確認してください)")?;
        let plan = tract_onnx::onnx()
            .model_for_path(&path)
            .and_then(|m| m.into_optimized())
            .and_then(|m| m.into_runnable())
            .map_err(|e| format!("AIアップミックスのモデルを読み込めません: {e}"))?;
        Ok(TractBackend { plan })
    }
}

impl UpmixBackend for TractBackend {
    fn label(&self) -> &'static str {
        "CPU(tract)"
    }

    /// 1チャンク分(ちょうど[`CHUNK_SAMPLES`]サンプル、ステレオ)を推論し、
    /// `[drums, bass, other, vocals]`の4ステム(各ステレオ)を返す。
    fn run_chunk(&self, left: &[f32], right: &[f32]) -> Result<[[Vec<f32>; 2]; 4], String> {
        debug_assert_eq!(left.len(), CHUNK_SAMPLES);
        debug_assert_eq!(right.len(), CHUNK_SAMPLES);
        let mut data = vec![0f32; 2 * CHUNK_SAMPLES];
        data[..CHUNK_SAMPLES].copy_from_slice(left);
        data[CHUNK_SAMPLES..].copy_from_slice(right);
        let input = tract_ndarray::Array3::from_shape_vec((1, 2, CHUNK_SAMPLES), data).map_err(|e| e.to_string())?.into_tensor();
        let result = self.plan.run(tvec!(input.into())).map_err(|e| format!("推論に失敗しました: {e}"))?;
        let out = result[0].to_plain_array_view::<f32>().map_err(|e| e.to_string())?;
        // 出力shape: [1, 4, 2, CHUNK_SAMPLES]
        let mut stems: [[Vec<f32>; 2]; 4] = std::array::from_fn(|_| [Vec::new(), Vec::new()]);
        for (s, stem) in stems.iter_mut().enumerate() {
            for (c, ch) in stem.iter_mut().enumerate() {
                ch.reserve(CHUNK_SAMPLES);
                for t in 0..CHUNK_SAMPLES {
                    ch.push(out[[0, s, c, t]]);
                }
            }
        }
        Ok(stems)
    }
}

/// 利用可能なバックエンドを選ぶ: Windowsではまずort/DirectML(GPU)を試し、失敗すれば
/// tract(CPU)へフォールバックする。Windows以外は常にtract(CPU)。
fn load_backend() -> Result<Box<dyn UpmixBackend>, String> {
    // 1) GPU(DirectML、Windowsのみ) 2) onnxruntimeのCPU(tractより約5倍速い) 3) 純Rustのtract(常に使える最後の砦)
    #[cfg(windows)]
    match gpu::OrtBackend::load(true) {
        Ok(b) => {
            eprintln!("make-disk: AIアップミックスをGPU(DirectML)で実行します");
            return Ok(Box::new(b));
        }
        Err(e) => eprintln!("make-disk: AIアップミックスのGPU(DirectML)を使えないため、次の方法を試します: {e}"),
    }
    #[cfg(not(target_os = "android"))]
    match gpu::OrtBackend::load(false) {
        Ok(b) => return Ok(Box::new(b)),
        Err(e) => eprintln!("make-disk: onnxruntime(CPU)を使えないため、純Rustのtractで実行します(遅くなります): {e}"),
    }
    Ok(Box::new(TractBackend::load()?))
}

/// 入力(任意長・任意チャンネル数)を、[`CHUNK_SAMPLES`]単位のステレオチャンクに分けて
/// 全区間分離し、`[drums, bass, other, vocals]`(各ステレオ、元の長さぶん)を返す。
fn separate_all(sr: u32, channels: &[Vec<f32>]) -> Result<[[Vec<f32>; 2]; 4], String> {
    if sr != MODEL_SR {
        return Err(format!("AIアップミックスは{MODEL_SR}Hz固定です(入力: {sr}Hz)。呼び出し側でリサンプルしてください。"));
    }
    let total = channels.first().map_or(0, |c| c.len());
    let (left, right) = match channels.len() {
        1 => (channels[0].clone(), channels[0].clone()),
        _ => (channels[0].clone(), channels[1].clone()),
    };
    let backend = load_backend()?;
    eprintln!("make-disk: AIアップミックス バックエンド: {}", backend.label());
    let mut stems: [Vec<Vec<f32>>; 4] = std::array::from_fn(|_| vec![Vec::with_capacity(total), Vec::with_capacity(total)]);
    let mut pos = 0usize;
    while pos < total.max(1) {
        let end = (pos + CHUNK_SAMPLES).min(total);
        let mut chunk_l = vec![0f32; CHUNK_SAMPLES];
        let mut chunk_r = vec![0f32; CHUNK_SAMPLES];
        chunk_l[..end - pos].copy_from_slice(&left[pos..end]);
        chunk_r[..end - pos].copy_from_slice(&right[pos..end]);
        let separated = backend.run_chunk(&chunk_l, &chunk_r)?;
        let keep = end - pos;
        for (s, stem) in separated.into_iter().enumerate() {
            stems[s][0].extend_from_slice(&stem[0][..keep]);
            stems[s][1].extend_from_slice(&stem[1][..keep]);
        }
        pos += CHUNK_SAMPLES;
        if total == 0 {
            break;
        }
    }
    Ok(stems.map(|s| [s[0].clone(), s[1].clone()]))
}

/// onnxruntime(ort)バックエンド。GPU(DirectML、Windowsのみ)と、CPU(tractより約5倍速い)の2通り。
/// どちらも`onnxruntime.dll`(`.so`/`.dylib`)を実行時に読み込む(同梱または`ORT_DYLIB_PATH`)。
/// 見つからなければ読み込みに失敗し、呼び出し側が純Rustのtractへ落とす。
#[cfg(not(target_os = "android"))]
mod gpu {
    use super::{model_path, CHUNK_SAMPLES};
    #[cfg(windows)]
    use ort::ep::DirectML;
    use ort::session::{builder::GraphOptimizationLevel, Session};
    use ort::value::Tensor;
    use std::sync::Mutex;

    pub struct OrtBackend {
        gpu: bool,
        // Session::run()は&mut selfを要求するが、UpmixBackendトレイトは&selfで
        // 呼ばれる(チャンクを順に処理するだけで並行実行はしないため、Mutexで足りる)。
        session: Mutex<Session>,
    }

    /// onnxruntimeのライブラリを探し、`ORT_DYLIB_PATH`が未設定なら見つかった場所を設定する。
    /// 探す順: 環境変数 → 実行ファイルの隣 → プラグインフォルダの`ai-upmix/onnxruntime/`。
    fn locate_runtime() -> Result<(), String> {
        if std::env::var_os("ORT_DYLIB_PATH").is_some() {
            return Ok(());
        }
        let name = if cfg!(windows) { "onnxruntime.dll" } else if cfg!(target_os = "macos") { "libonnxruntime.dylib" } else { "libonnxruntime.so" };
        let mut dirs: Vec<std::path::PathBuf> = Vec::new();
        if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf())) {
            dirs.push(d.join("onnxruntime")); // インストーラーが同梱する場所(Windows)
            dirs.push(d);
        }
        if let Some(d) = crate::engine::plugins::plugin_dir() {
            dirs.push(d.join("ai-upmix").join("onnxruntime"));
        }
        for d in dirs {
            let f = d.join(name);
            if f.is_file() {
                std::env::set_var("ORT_DYLIB_PATH", &f);
                return Ok(());
            }
        }
        Err(format!("{name}が見つかりません(実行ファイルの隣か、ai-upmix/onnxruntime/に置くか、ORT_DYLIB_PATHで指定してください)"))
    }

    impl OrtBackend {
        /// `gpu`=trueならDirectML(Windowsのみ)、falseならonnxruntimeのCPU実行。
        pub fn load(gpu: bool) -> Result<Self, String> {
            let path = model_path().ok_or("AIアップミックスのモデルが見つかりません")?;
            locate_runtime()?;
            let mut builder = Session::builder().map_err(|e| e.to_string())?.with_optimization_level(GraphOptimizationLevel::Level3).map_err(|e| e.to_string())?;
            if gpu {
                #[cfg(windows)]
                {
                    builder = builder.with_execution_providers([DirectML::default().build()]).map_err(|e| e.to_string())?;
                }
                #[cfg(not(windows))]
                return Err("DirectMLはWindowsのみです".into());
            }
            let session = builder.commit_from_file(&path).map_err(|e| format!("{}セッションを作れません: {e}", if gpu { "DirectML" } else { "onnxruntime(CPU)" }))?;
            Ok(OrtBackend { gpu, session: Mutex::new(session) })
        }
    }

    impl super::UpmixBackend for OrtBackend {
        fn label(&self) -> &'static str {
            if self.gpu { "GPU(DirectML)" } else { "CPU(onnxruntime)" }
        }

        fn run_chunk(&self, left: &[f32], right: &[f32]) -> Result<[[Vec<f32>; 2]; 4], String> {
            let mut data = vec![0f32; 2 * CHUNK_SAMPLES];
            data[..CHUNK_SAMPLES].copy_from_slice(left);
            data[CHUNK_SAMPLES..].copy_from_slice(right);
            let input = Tensor::from_array(([1usize, 2, CHUNK_SAMPLES], data)).map_err(|e| e.to_string())?;
            let mut session = self.session.lock().map_err(|e| e.to_string())?;
            let outputs = session.run(ort::inputs!["mix" => input]).map_err(|e| format!("推論に失敗しました: {e}"))?;
            let (shape, out) = outputs[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
            let stride_s = dims[2] * dims[3];
            let stride_c = dims[3];
            let mut stems: [[Vec<f32>; 2]; 4] = std::array::from_fn(|_| [Vec::new(), Vec::new()]);
            for (s, stem) in stems.iter_mut().enumerate() {
                for (c, ch) in stem.iter_mut().enumerate() {
                    let start = s * stride_s + c * stride_c;
                    ch.extend_from_slice(&out[start..start + CHUNK_SAMPLES]);
                }
            }
            Ok(stems)
        }
    }
}

fn stem_index(name: &str) -> usize {
    STEM_ORDER.iter().position(|s| *s == name).expect("known stem name")
}

/// 分離した4ステムから、5.1ch/7.1chのチャンネル配列(FL,FR,FC,LFE,BL,BR[,SL,SR])を組み立てる。
/// センター=ボーカル(左右の単純和)、フロント=ドラム+ベース(元のステレオ配置)、
/// リア(サイド)=その他(残響・伴奏成分)。LFEは常に無音。
fn mix_to_surround(stems: &[[Vec<f32>; 2]; 4], target: ChannelLayoutTarget) -> Result<Vec<Vec<f32>>, String> {
    let drums = &stems[stem_index("drums")];
    let bass = &stems[stem_index("bass")];
    let other = &stems[stem_index("other")];
    let vocals = &stems[stem_index("vocals")];
    let n = drums[0].len();

    let mix = |a: &[f32], b: &[f32]| -> Vec<f32> { a.iter().zip(b).map(|(x, y)| x + y).collect() };
    let fl = mix(&drums[0], &bass[0]);
    let fr = mix(&drums[1], &bass[1]);
    let fc: Vec<f32> = (0..n).map(|i| 0.5 * (vocals[0][i] + vocals[1][i])).collect();
    let lfe = vec![0f32; n];
    let rear_gain = 0.9f32; // 残響/伴奏成分をそのままリアへ(前方と重複しないため減衰は最小限)
    let bl: Vec<f32> = other[0].iter().map(|v| v * rear_gain).collect();
    let br: Vec<f32> = other[1].iter().map(|v| v * rear_gain).collect();

    let mut channels = vec![fl, fr, fc, lfe, bl, br];
    if matches!(target, ChannelLayoutTarget::Surround71) {
        let side_gain = 0.6f32;
        channels.push(other[0].iter().map(|v| v * side_gain).collect());
        channels.push(other[1].iter().map(|v| v * side_gain).collect());
    }
    Ok(channels)
}

/// 44.1kHzのWAV(`input`、モノラルまたはステレオ)を読み、AIで4パートに分離してから
/// 5.1ch/7.1chへミックスし、`output`へ書く。`target`は`Surround51`/`Surround71`のみ有効。
pub fn upmix_wav_file(input: &Path, output: &Path, target: ChannelLayoutTarget) -> Result<(), String> {
    if !matches!(target, ChannelLayoutTarget::Surround51 | ChannelLayoutTarget::Surround71) {
        return Err("AIアップミックスは5.1ch/7.1ch指定のときだけ使えます".to_string());
    }
    let (sr, channels) = crate::engine::audio_sr::read_f32_wav(input)?;
    let stems = separate_all(sr, &channels)?;
    let surround = mix_to_surround(&stems, target)?;
    crate::engine::audio_sr::write_f32_wav(output, sr, &surround)
}

/// 任意の音声/動画ソースファイルから、AIアップミックス済み(5.1ch/7.1ch)のWAVを作る。
/// 内部でffmpegを使い、いったん44.1kHzステレオWAVへデコードしてから[`upmix_wav_file`]を呼ぶ
/// (`convert.rs`の`AudioBwe`〈帯域拡張〉が行っているのと同じ「一旦WAVへ落としてRustのAI処理へ渡す」構成)。
pub fn upmix_source_file(input_path: &str, output: &Path, target: ChannelLayoutTarget) -> Result<(), String> {
    let tag = format!("{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let decoded = std::env::temp_dir().join(format!("make-disk-upmix-in-{tag}.wav"));
    let status = crate::engine::sidecar::resolve_tool("ffmpeg")
        .args(["-y", "-v", "error", "-i", input_path, "-vn", "-ac", "2", "-ar", &MODEL_SR.to_string(), "-c:a", "pcm_f32le", decoded.to_str().unwrap_or_default()])
        .status()
        .map_err(|e| format!("ffmpegを実行できません: {e}"))?;
    if !status.success() {
        return Err(format!("ffmpegによるデコードに失敗しました(終了コード{:?})", status.code()));
    }
    let result = upmix_wav_file(&decoded, output, target);
    let _ = std::fs::remove_file(&decoded);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実モデルでGPU(DirectML)とCPU(tract)の出力一致と速度を測る手動試験。
    /// `MAKE_DISK_DEMUCS_MODEL_DIR`にパッチ済みモデルを置き、`ORT_DYLIB_PATH`にDirectML対応の
    /// onnxruntime.dllを指して `cargo test --lib gpu_matches_cpu -- --ignored --nocapture` で実行する。
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn gpu_matches_cpu_on_real_model() {
        let n = CHUNK_SAMPLES;
        let mut seed = 12345u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / 16_777_216.0 - 0.5
        };
        let left: Vec<f32> = (0..n).map(|i| 0.3 * (i as f32 * 0.02).sin() + 0.1 * (i as f32 * 0.31).sin() + 0.05 * noise()).collect();
        let right: Vec<f32> = (0..n).map(|i| 0.3 * (i as f32 * 0.02).sin() - 0.1 * (i as f32 * 0.17).sin() + 0.05 * noise()).collect();

        let t = std::time::Instant::now();
        let cpu = TractBackend::load().expect("tract load");
        let cpu_load = t.elapsed();
        let t = std::time::Instant::now();
        let c = cpu.run_chunk(&left, &right).expect("cpu run");
        let cpu_run = t.elapsed();
        eprintln!("CPU(tract): load {cpu_load:?} run {cpu_run:?}");

        let t = std::time::Instant::now();
        let gpu = match gpu::OrtBackend::load(true) {
            Ok(g) => g,
            Err(e) => {
                // 非力なGPU(VRAM不足等)ではDirectMLの初期化に失敗する。本番ではCPUへフォールバックする設計どおり。
                eprintln!("GPU(DirectML)を使えません。onnxruntime(CPU)で測定します: {e}");
                let t = std::time::Instant::now();
                let oc = gpu::OrtBackend::load(false).expect("onnxruntime cpu load");
                let ort_load = t.elapsed();
                let t = std::time::Instant::now();
                let o = oc.run_chunk(&left, &right).expect("ort cpu run");
                let ort_run = t.elapsed();
                let (mut md, mut ma, mut de, mut se) = (0f32, 0f32, 0f64, 0f64);
                for s in 0..4 {
                    for ch in 0..2 {
                        for (a, b) in c[s][ch].iter().zip(&o[s][ch]) {
                            md = md.max((a - b).abs());
                            ma = ma.max(a.abs());
                            de += ((a - b) as f64).powi(2);
                            se += (*a as f64).powi(2);
                        }
                    }
                }
                let rel_rms = (de / se.max(1e-12)).sqrt();
                eprintln!("ORT CPU: load {ort_load:?} run {ort_run:?} / tractとの最大差 {md:.6} (最大振幅 {ma:.4})、相対RMS誤差 {rel_rms:.5}");
                assert!(rel_rms < 0.05, "onnxruntime(CPU)とtractの出力が大きく違う(相対RMS誤差 {rel_rms})");
                return;
            }
        };
        let gpu_load = t.elapsed();
        let t = std::time::Instant::now();
        let g = gpu.run_chunk(&left, &right).expect("gpu run");
        let gpu_run = t.elapsed();
        let t = std::time::Instant::now();
        let _ = gpu.run_chunk(&left, &right).expect("gpu run 2");
        let gpu_run2 = t.elapsed();

        let (mut max_diff, mut max_abs, mut energy) = (0f32, 0f32, 0f64);
        for s in 0..4 {
            for ch in 0..2 {
                for (a, b) in c[s][ch].iter().zip(&g[s][ch]) {
                    max_diff = max_diff.max((a - b).abs());
                    max_abs = max_abs.max(a.abs());
                    energy += (*a as f64) * (*a as f64);
                }
            }
        }
        eprintln!("CPU: load {cpu_load:?} run {cpu_run:?} / GPU: load {gpu_load:?} run {gpu_run:?} 2nd {gpu_run2:?}");
        eprintln!("max|cpu-gpu| = {max_diff:.6} (max|cpu| = {max_abs:.4}, energy {energy:.1})");
        assert!(energy > 0.0, "出力が無音");
        assert!(max_diff < 0.05 * max_abs.max(1e-3), "GPUとCPUの出力が大きく違う");
    }

    fn silent_stems(n: usize) -> [[Vec<f32>; 2]; 4] {
        std::array::from_fn(|_| [vec![0f32; n], vec![0f32; n]])
    }

    #[test]
    fn stem_index_matches_known_order() {
        assert_eq!(stem_index("drums"), 0);
        assert_eq!(stem_index("bass"), 1);
        assert_eq!(stem_index("other"), 2);
        assert_eq!(stem_index("vocals"), 3);
    }

    #[test]
    fn mix_to_surround51_has_six_channels_with_silent_lfe() {
        let mut stems = silent_stems(2);
        stems[stem_index("vocals")] = [vec![1.0, 1.0], vec![1.0, 1.0]];
        let out = mix_to_surround(&stems, ChannelLayoutTarget::Surround51).unwrap();
        assert_eq!(out.len(), 6);
        assert_eq!(out[3], vec![0.0, 0.0], "LFEは常に無音");
        assert_eq!(out[2], vec![1.0, 1.0], "センターはボーカルの単純和(0.5*(1+1)=1)");
    }

    #[test]
    fn mix_to_surround71_adds_two_side_channels_from_other_stem() {
        let mut stems = silent_stems(3);
        stems[stem_index("other")] = [vec![1.0, 1.0, 1.0], vec![1.0, 1.0, 1.0]];
        let out = mix_to_surround(&stems, ChannelLayoutTarget::Surround71).unwrap();
        assert_eq!(out.len(), 8);
        assert!((out[6][0] - 0.6).abs() < 1e-6, "サイドLはotherの0.6倍");
        assert!((out[4][0] - 0.9).abs() < 1e-6, "リアLもotherの0.9倍");
    }

    #[test]
    fn front_channels_sum_drums_and_bass_keeping_stereo_placement() {
        let mut stems = silent_stems(2);
        stems[stem_index("drums")] = [vec![0.3, 0.3], vec![0.1, 0.1]];
        stems[stem_index("bass")] = [vec![0.2, 0.2], vec![0.05, 0.05]];
        let out = mix_to_surround(&stems, ChannelLayoutTarget::Surround51).unwrap();
        assert!((out[0][0] - 0.5).abs() < 1e-6, "FL = drums.L + bass.L");
        assert!((out[1][0] - 0.15).abs() < 1e-6, "FR = drums.R + bass.R");
    }

    #[test]
    fn upmix_requires_a_surround_target() {
        let dir = std::env::temp_dir().join(format!("make_disk_ai_upmix_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("in.wav");
        crate::engine::audio_sr::write_f32_wav(&input, 44_100, &[vec![0.0; 100], vec![0.0; 100]]).unwrap();
        let output = dir.join("out.wav");
        let err = upmix_wav_file(&input, &output, ChannelLayoutTarget::Stereo).unwrap_err();
        assert!(err.contains("5.1ch/7.1ch"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_available_is_false_without_a_configured_model_dir() {
        // このテスト環境にMAKE_DISK_DEMUCS_MODEL_DIRを設定していない前提(CI/開発機の既定)。
        if std::env::var("MAKE_DISK_DEMUCS_MODEL_DIR").is_ok() {
            return;
        }
        assert!(!is_available());
    }
}
