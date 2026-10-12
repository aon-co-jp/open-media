//! AI超解像(Real-ESRGAN、GPU=NCNN-Vulkan)(2026-09-19新設)。
//!
//! ユーザー指示「DVD→4K等の拡大は、補間ではなく失われたディテールを補う本格的なAI超解像を」への対応。
//! 公式の[Real-ESRGAN-ncnn-vulkan](https://github.com/xinntao/Real-ESRGAN)(MITライセンス)を
//! **オンデマンドでダウンロードするプラグイン**として使う(約45MB、インストーラーを肥大化させない。
//! バージョン`v0.2.5.0`ごとに`<プラグインフォルダ>/realesrgan/<版>/`へ展開し、あれば再取得しない)。
//!
//! ## 実機で確認した事実・制限(正直な開示)
//! - **GPU版(NCNN-Vulkan)はVulkan対応GPUが必須**(NVIDIA/AMD/Intel、内蔵GPUも可)。公式ビルドにCPUモードは無い
//!   (`-g -1`は「invalid gpu device」で失敗することを実機確認)。そこで**GPUが使えない環境向けに自前のCPU版**
//!   (`engine::cpu_sr`、`realesr-animevideov3`のみ、AVX2+FMA対応)を用意し、`backend="auto"`ではGPUが動かなければ自動でCPU版に切り替える。
//!   `realesrgan-x4plus`(高品質)はGPU必須。
//! - (2026-09-24) 動画の変換は`engine::ai_video`(ストリーミング・分割・再開可能、フレーム数の上限なし)へ移した。
//!   ここに残るのは、プラグインの取得・実行環境の選択・1枚の画像の超解像。
//! - **速度は非常に遅い**: このPCのGT 730(2GB)で720×480の1フレームあたり、軽量モデル
//!   `realesr-animevideov3`が約4.6秒、高品質モデル`realesrgan-x4plus`が約110秒。映画1本(約13万フレーム)は
//!   現実的ではなく、短いクリップ向け(2026-09-24以降は上限を撤廃し、`ai_video`で分割・再開しながら処理する)。
//! - フレームをPNGへ書き出して処理するため一時ディスク容量を使う(100フレームずつ処理して都度削除)。

use crate::engine::plugins;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 配布物の版(GitHub: xinntao/Real-ESRGAN のリリースタグ)。
pub const REALESRGAN_VERSION: &str = "v0.2.5.0";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiUpscale {
    /// `"auto"`(内容から選ぶ)/ `"realesr-animevideov3"`(軽量・アニメ向け)/ `"realesr-general-x4v3"`(実写・汎用)/
    /// `"realesrgan-x4plus"`(高品質だが非常に遅い、GPU必須)。
    #[serde(default)]
    pub model: String,
    /// 拡大倍率(2/3/4)。`0`は自動(出力の高さが目標以上になる最小の倍率)。`realesrgan-x4plus`は4のみ。
    #[serde(default)]
    pub scale: u32,
    /// 実行環境: `"auto"`(既定。Vulkan対応GPUが使えればGPU、無ければCPU)/`"gpu"`/`"cpu"`。
    /// CPU版は`realesr-animevideov3`のみ対応(`engine::cpu_sr`)。
    #[serde(default = "default_backend")]
    pub backend: String,
    /// 映像の内容: `"live"`(実写)/ `"anime"`。`model`が`auto`のときのモデル選びに使う。
    #[serde(default)]
    pub content: Option<String>,
    /// ノイズ除去の強さ(0〜1、汎用モデルのみ)。
    #[serde(default)]
    pub denoise: Option<f32>,
    /// インターレース解除: `"auto"`(既定)/ `"off"` / `"interlaced"` / `"telecine"`。
    #[serde(default)]
    pub deinterlace: Option<String>,
    /// 黒帯を切り取ってからAI処理するか(既定: する)。
    #[serde(default)]
    pub crop_bars: Option<bool>,
    /// 単色・静止コマのAI処理を省略して再利用するか(既定: する)。
    #[serde(default)]
    pub skip_static: Option<bool>,
    /// フレーム補間の目標fps(60/120など)。未指定なら補間しない。
    #[serde(default)]
    pub target_fps: Option<u32>,
    /// `true`なら超解像はせず、フレーム補間(RIFE)だけを行う(動きを滑らかにするだけ)。`target_fps`が必須。
    #[serde(default)]
    pub interpolate_only: bool,
}

fn default_backend() -> String {
    "auto".to_string()
}

/// 実際に使う実行環境。
enum Backend {
    Gpu(PathBuf),
    Cpu(crate::engine::cpu_sr::SrModel),
}

/// Vulkan対応GPUで実際に動くか(小さな画像で試す。結果はプロセス内でキャッシュ)。
pub(crate) fn gpu_usable(exe: &Path) -> bool {
    static CACHE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("make-disk-gpucheck-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let (input, output) = (dir.join("in.png"), dir.join("out.png"));
        let ok = image::RgbImage::new(16, 16).save(&input).is_ok()
            && crate::engine::sidecar::background_command(exe)
                .args(["-i", &input.to_string_lossy(), "-o", &output.to_string_lossy(), "-m", &exe.parent().map(|p| p.join("models")).unwrap_or_default().to_string_lossy(), "-n", "realesr-animevideov3", "-s", "2", "-f", "png"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            && output.is_file();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    })
}

/// 設定と実機の状況から実行環境を決める。CPUのみのPC(GPU非搭載/Vulkan非対応)ではCPU版へ自動で切り替える。
fn choose_backend(exe: &Path, up: &AiUpscale) -> Result<Backend, String> {
    let models = exe.parent().ok_or("プラグインの場所が不正です")?.join("models");
    let cpu_capable = up.model == "realesr-animevideov3";
    match up.backend.as_str() {
        "gpu" => Ok(Backend::Gpu(exe.to_path_buf())),
        "cpu" if cpu_capable => Ok(Backend::Cpu(crate::engine::cpu_sr::load_model(&models, up.scale)?)),
        "cpu" => Err("CPU版はrealesr-animevideov3のみ対応です(realesrgan-x4plusはGPUが必要) / the CPU build only supports realesr-animevideov3".to_string()),
        _ => {
            // 実測でCPUのほうが速いと分かっているPCでは、GPUが動いてもCPUを使う(このPCではGPUのほうが約3倍遅い)。
            if cpu_capable && crate::engine::hw_bench::cached().is_some_and(|b| b.choice == "cpu") {
                Ok(Backend::Cpu(crate::engine::cpu_sr::load_model(&models, up.scale)?))
            } else if gpu_usable(exe) {
                Ok(Backend::Gpu(exe.to_path_buf()))
            } else if cpu_capable {
                Ok(Backend::Cpu(crate::engine::cpu_sr::load_model(&models, up.scale)?))
            } else {
                Err("Vulkan対応GPUが見つかりません。realesrgan-x4plusはGPUが必要です(CPUでは高速モデルrealesr-animevideov3を選んでください) / no Vulkan GPU found; realesrgan-x4plus needs one".to_string())
            }
        }
    }
}

/// 画像ファイルまたはフォルダ`input`を`output`へ超解像する(選ばれた実行環境で)。
fn run_backend(backend: &Backend, input: &Path, output: &Path, up: &AiUpscale) -> Result<(), String> {
    match backend {
        Backend::Gpu(exe) => run_realesrgan(exe, input, output, up),
        Backend::Cpu(model) => {
            if input.is_dir() {
                std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
                let mut files: Vec<_> = std::fs::read_dir(input).map_err(|e| e.to_string())?.filter_map(|e| e.ok()).map(|e| e.path()).collect();
                files.sort();
                for f in files {
                    let name = f.file_name().ok_or("ファイル名が不正です")?;
                    crate::engine::cpu_sr::upscale_image_file(model, &f, &output.join(name))?;
                }
                Ok(())
            } else {
                crate::engine::cpu_sr::upscale_image_file(model, input, output)
            }
        }
    }
}

fn asset_name() -> Result<&'static str, String> {
    if cfg!(target_os = "windows") {
        Ok("realesrgan-ncnn-vulkan-20220424-windows.zip")
    } else if cfg!(target_os = "macos") {
        Ok("realesrgan-ncnn-vulkan-20220424-macos.zip")
    } else if cfg!(target_os = "linux") {
        Ok("realesrgan-ncnn-vulkan-20220424-ubuntu.zip")
    } else {
        Err("AI超解像はデスクトップ(Windows/macOS/Linux)のみ対応です / AI upscaling is desktop-only".to_string())
    }
}

fn exe_file_name() -> String {
    format!("realesrgan-ncnn-vulkan{}", std::env::consts::EXE_SUFFIX)
}

/// プラグインの展開先(版ごと)。
fn plugin_root() -> Result<PathBuf, String> {
    Ok(plugins::plugin_dir().ok_or("プラグインフォルダを特定できません")?.join("realesrgan").join(REALESRGAN_VERSION))
}

/// 実行ファイルがあれば(=導入済みなら)そのパスを返す。ネットワークは使わない。
pub fn installed_exe() -> Option<PathBuf> {
    let exe = plugin_root().ok()?.join(exe_file_name());
    exe.is_file().then_some(exe)
}

/// プラグインが無ければダウンロード・展開して、実行ファイルのパスを返す(導入済みならスキップ)。
pub fn ensure_plugin() -> Result<PathBuf, String> {
    if let Some(exe) = installed_exe() {
        install_general_model(&exe);
        return Ok(exe);
    }
    let asset = asset_name()?;
    let url = format!("https://github.com/xinntao/Real-ESRGAN/releases/download/{REALESRGAN_VERSION}/{asset}");
    let root = plugin_root()?;
    std::fs::create_dir_all(&root).map_err(|e| format!("プラグインフォルダを作成できません: {e}"))?;

    let mut bytes: Vec<u8> = Vec::new();
    let resp = ureq::get(&url).call().map_err(|e| format!("Real-ESRGANのダウンロードに失敗しました({url}): {e}"))?;
    std::io::copy(&mut resp.into_reader(), &mut bytes).map_err(|e| format!("ダウンロードの読み取りに失敗しました: {e}"))?;

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("ZIPを開けません: {e}"))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("ZIPの読み取りに失敗しました: {e}"))?;
        let Some(rel) = entry.enclosed_name() else { continue };
        let dest = root.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = std::fs::File::create(&dest).map_err(|e| format!("{}を作成できません: {e}", dest.display()))?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let exe = root.join(exe_file_name());
        if let Ok(meta) = std::fs::metadata(&exe) {
            let mut perm = meta.permissions();
            perm.set_mode(0o755);
            let _ = std::fs::set_permissions(&exe, perm);
        }
    }
    let exe = installed_exe().ok_or_else(|| "展開後にrealesrgan-ncnn-vulkanが見つかりません".to_string())?;
    install_general_model(&exe);
    Ok(exe)
}

/// 実写・汎用モデル`realesr-general-x4v3`(Real-ESRGAN、BSD-3-Clause)。公式のncnn配布物には含まれないため、
/// 公式の重み(`realesr-general-x4v3.pth`と`realesr-general-wdn-x4v3.pth`)を公式の既定どおりノイズ除去の強さ0.5で混ぜ、
/// ncnn形式へ変換したものを本体に同梱している(変換手順は`scripts/convert_general_model.py`)。
static GENERAL_PARAM: &str = include_str!("../../models/realesr-general-x4v3.param");
static GENERAL_BIN: &[u8] = include_bytes!("../../models/realesr-general-x4v3.bin");

fn install_general_model(exe: &Path) {
    let Some(models) = exe.parent().map(|p| p.join("models")) else { return };
    let (param, bin) = (models.join("realesr-general-x4v3.param"), models.join("realesr-general-x4v3.bin"));
    let up_to_date = std::fs::metadata(&bin).is_ok_and(|m| m.len() == GENERAL_BIN.len() as u64) && std::fs::read_to_string(&param).is_ok_and(|t| t == GENERAL_PARAM);
    if !up_to_date {
        let _ = std::fs::create_dir_all(&models);
        let _ = std::fs::write(&param, GENERAL_PARAM);
        let _ = std::fs::write(&bin, GENERAL_BIN);
    }
}

pub(crate) fn validate(up: &AiUpscale) -> Result<(), String> {
    if up.interpolate_only && up.target_fps.is_none() {
        return Err("フレーム補間だけを行うには、目標のfpsを指定してください / choose a target fps for interpolation-only".to_string());
    }
    let scale_ok = up.scale == 0 || (2..=4).contains(&up.scale);
    match up.model.as_str() {
        "" | "auto" | "realesr-animevideov3" | "realesr-general-x4v3" if scale_ok => Ok(()),
        "realesrgan-x4plus" if up.scale == 0 || up.scale == 4 => Ok(()),
        "" | "auto" | "realesr-animevideov3" | "realesr-general-x4v3" => Err("倍率は2/3/4のみです(0で自動)".to_string()),
        "realesrgan-x4plus" => Err("realesrgan-x4plusの倍率は4のみです".to_string()),
        other => Err(format!("未対応のAIモデルです: {other}")),
    }
}

/// `input`(画像ファイルまたはフォルダ)を`output`へAI超解像する。
fn run_realesrgan(exe: &Path, input: &Path, output: &Path, up: &AiUpscale) -> Result<(), String> {
    let models = exe.parent().ok_or("プラグインの場所が不正です")?.join("models");
    let out = crate::engine::sidecar::background_command(exe)
        .args(["-i", &input.to_string_lossy(), "-o", &output.to_string_lossy(), "-m", &models.to_string_lossy(), "-n", &up.model, "-s", &up.scale.to_string(), "-f", "png"])
        .output()
        .map_err(|e| format!("realesrgan-ncnn-vulkanの起動に失敗しました: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "AI超解像に失敗しました(Vulkan対応GPUが必要です): {}",
            String::from_utf8_lossy(&out.stderr).lines().filter(|l| !l.contains('%')).collect::<Vec<_>>().join(" / ")
        ));
    }
    Ok(())
}

/// 1枚の画像をAI超解像する。
pub fn upscale_image(input: &str, output: &str, up: &AiUpscale) -> Result<(), String> {
    validate(up)?;
    let mut up = up.clone();
    if up.scale == 0 {
        up.scale = 2;
    }
    if up.model.is_empty() || up.model == "auto" {
        up.model = "realesr-animevideov3".to_string();
    }
    let exe = ensure_plugin()?;
    let backend = choose_backend(&exe, &up)?;
    run_backend(&backend, Path::new(input), Path::new(output), &up)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_models_and_scales() {
        let ok = |m: &str, s: u32| validate(&AiUpscale { model: m.into(), scale: s, backend: "auto".into(), ..Default::default() }).is_ok();
        assert!(ok("realesr-animevideov3", 2) && ok("realesr-animevideov3", 4) && ok("realesrgan-x4plus", 4));
        assert!(!ok("realesr-animevideov3", 5) && !ok("realesrgan-x4plus", 2) && !ok("unknown", 4));
    }

    #[test]
    fn asset_name_matches_this_os() {
        let name = asset_name().unwrap();
        assert!(name.starts_with("realesrgan-ncnn-vulkan-20220424-") && name.ends_with(".zip"));
    }

    #[test]
    fn x4plus_requires_the_gpu_backend() {
        let exe = std::path::PathBuf::from("dummy");
        let up = AiUpscale { model: "realesrgan-x4plus".into(), scale: 4, backend: "cpu".into(), ..Default::default() };
        assert!(choose_backend(&exe, &up).is_err(), "x4plusはCPU版の対象外");
    }

}
