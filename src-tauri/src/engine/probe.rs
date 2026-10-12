//! ffprobeでメディア情報(尺・コーデック等)を取得する。
//!
//! ## rs-ffmpegへのフォールバック(2026-09-17新設)
//!
//! ユーザー指示「もう一つのオープンソースのRust版も同梱して呼び出す
//! ように修正して」への対応(`engine::iso`でxorriso→rs-xorrisoの
//! フォールバックを実装したのに合わせて、ffmpeg/ffprobe→rs-ffmpegにも
//! 同じパターンを適用する)。**正直な開示**: `rs-ffmpeg`は非圧縮WAVの
//! probe専用(`scripts/build-rs-tribute-sidecars.sh`のdocコメント参照)
//! なので、本家ffprobeが見つからずWAV以外のファイルをprobeしようと
//! した場合はrs-ffmpeg側が明確なエラーを返す(黙って嘘の結果を返さない、
//! rs-ffmpeg自身の設計方針)。`format_name`/`bit_rate`はrs-ffmpegの出力
//! (サンプルレート・チャンネル数・ビット深度)から非圧縮WAVとして
//! 妥当な値を合成する。
use crate::engine::sidecar::resolve_tool;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct MediaInfo {
    pub duration_secs: f64,
    pub format_name: String,
    pub bit_rate: Option<u64>,
    /// 動画ストリームの幅・高さ・フレームレート(2026-09-17新設、
    /// 「AIが最適化」する解像度/FPS指定〈main.js側のヒューリスティック〉の
    /// 判断材料。音声ファイル等、動画ストリームが無い場合は`None`)。
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    /// 音声ストリームの情報(2026-09-19新設、Dolby Atmos/サラウンド素材の検出用)。
    pub audio_codec: Option<String>,
    pub audio_channels: Option<u32>,
    /// ffprobeの`profile`(例: "Dolby Digital Plus + Dolby Atmos")。
    pub audio_profile: Option<String>,
    /// 映像にDolby Vision設定(DOVI configuration record)があるか。
    pub dolby_vision: bool,
    /// 音声ストリームのサンプリング周波数(Hz、2026-09-23新設)。再生規格の上限kHzへ
    /// 収める際に、元が上限以下ならアップサンプリングしないための判定に使う。
    #[serde(default)]
    pub audio_sample_rate: Option<u32>,
}

pub fn probe(path: &str) -> Result<MediaInfo, String> {
    match resolve_tool("ffprobe")
        .args([
            "-v", "error",
            "-show_format", "-show_streams",
            "-of", "json",
            path,
        ])
        .output()
    {
        Ok(output) if output.status.success() => parse_ffprobe_json(&output.stdout),
        Ok(output) => Err(String::from_utf8_lossy(&output.stderr).to_string()),
        Err(_) => probe_with_rs_ffmpeg(path),
    }
}

fn parse_ffprobe_json(stdout: &[u8]) -> Result<MediaInfo, String> {
    let json: serde_json::Value = serde_json::from_slice(stdout).map_err(|e| format!("ffprobe出力の解析に失敗しました: {e}"))?;

    let format = &json["format"];
    let duration_secs: f64 = format["duration"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let format_name = format["format_name"].as_str().unwrap_or("unknown").to_string();
    let bit_rate = format["bit_rate"].as_str().and_then(|s| s.parse().ok());

    let streams = json["streams"].as_array().cloned().unwrap_or_default();
    let stream = streams.iter().find(|s| s["codec_type"] == "video");
    let audio = streams.iter().find(|s| s["codec_type"] == "audio");
    let dolby_vision = stream
        .and_then(|s| s["side_data_list"].as_array())
        .is_some_and(|l| l.iter().any(|d| d["side_data_type"].as_str().is_some_and(|t| t.contains("DOVI"))));
    let audio_codec = audio.and_then(|a| a["codec_name"].as_str()).map(String::from);
    let audio_channels = audio.and_then(|a| a["channels"].as_u64()).map(|c| c as u32);
    let audio_profile = audio.and_then(|a| a["profile"].as_str()).map(String::from);
    let audio_sample_rate = audio.and_then(|a| a["sample_rate"].as_str()).and_then(|s| s.parse().ok());
    let width = stream.and_then(|s| s["width"].as_u64()).map(|v| v as u32);
    let height = stream.and_then(|s| s["height"].as_u64()).map(|v| v as u32);
    let fps = stream.and_then(|s| s["r_frame_rate"].as_str()).and_then(parse_frame_rate_fraction);

    Ok(MediaInfo { duration_secs, format_name, bit_rate, width, height, fps, audio_codec, audio_channels, audio_profile, dolby_vision, audio_sample_rate })
}

/// ffprobeの`r_frame_rate`(例: `"30000/1001"`や`"25/1"`)を`f64`に変換する。
fn parse_frame_rate_fraction(s: &str) -> Option<f64> {
    let (num, den) = s.split_once('/')?;
    let (num, den): (f64, f64) = (num.parse().ok()?, den.parse().ok()?);
    if den == 0.0 {
        return None;
    }
    Some(num / den)
}

/// `rs-ffmpeg probe <path>`の出力
/// (`sample_rate=44100 channels=2 bits_per_sample=16 duration_secs=1.234`)
/// を解析し、非圧縮WAVとして妥当な`MediaInfo`を合成する。
fn probe_with_rs_ffmpeg(path: &str) -> Result<MediaInfo, String> {
    let output = resolve_tool("rs-ffmpeg")
        .args(["probe", path])
        .output()
        .map_err(|e| format!("ffprobe・rs-ffmpegともに起動に失敗しました(いずれも未インストール/未同梱の可能性): {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let field = |key: &str| -> Option<&str> { stdout.split_whitespace().find_map(|tok| tok.strip_prefix(&format!("{key}="))) };

    let sample_rate: u64 = field("sample_rate").and_then(|s| s.parse().ok()).ok_or("rs-ffmpeg probe出力の解析に失敗しました(sample_rate)")?;
    let channels: u64 = field("channels").and_then(|s| s.parse().ok()).ok_or("rs-ffmpeg probe出力の解析に失敗しました(channels)")?;
    let bits_per_sample: u64 = field("bits_per_sample").and_then(|s| s.parse().ok()).ok_or("rs-ffmpeg probe出力の解析に失敗しました(bits_per_sample)")?;
    let duration_secs: f64 = field("duration_secs").and_then(|s| s.parse().ok()).unwrap_or(0.0);

    Ok(MediaInfo {
        duration_secs,
        format_name: "wav".to_string(),
        bit_rate: Some(sample_rate * channels * bits_per_sample),
        width: None,
        height: None,
        fps: None,
        audio_codec: Some("pcm".to_string()),
        audio_channels: Some(channels as u32),
        audio_profile: None,
        dolby_vision: false,
        audio_sample_rate: Some(sample_rate as u32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn parse_frame_rate_fraction_handles_common_ffprobe_values() {
        assert_eq!(parse_frame_rate_fraction("25/1"), Some(25.0));
        assert!((parse_frame_rate_fraction("30000/1001").unwrap() - 29.97).abs() < 0.01);
        assert_eq!(parse_frame_rate_fraction("0/0"), None);
        assert_eq!(parse_frame_rate_fraction("not-a-fraction"), None);
    }

    /// `scripts/fetch-ffmpeg-sidecars.sh`で取得した実バイナリ(ソース側は
    /// `<name>-<target-triple>[.exe]`という`externalBin`規約の名前)を、
    /// 実際にインストール後のアプリで確認した命名規則(bareな`<name>
    /// [.exe]`、`sidecar.rs`のモジュールdoc「実機検証で発見・修正した
    /// 実装ミス」参照)でテスト実行ファイルの隣へ配置し、`probe()`が
    /// 本当にそのバイナリを見つけて実行・正しい結果を返すことを検証する
    /// (2026-09-14追加、sidecar同梱化の実機E2E検証)。`src-tauri/binaries/`
    /// はgit管理外(README.md参照)なので、未取得の環境ではスキップする
    /// (このプロジェクトの既存方針: フェイクな成功にしない、
    /// `convert.rs`の`ffmpeg_available()`と同じ考え方)。
    #[test]
    fn probe_actually_executes_a_real_sidecar_binary_when_one_is_bundled() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let binaries_dir = manifest_dir.join("binaries");
        // `scripts/fetch-ffmpeg-sidecars.sh`が置くファイル名は
        // `ffmpeg-<target-triple>[.exe]`(externalBinのソース側規約)だが、
        // このテストはターゲットトリプルの正確な文字列を知る必要が無いよう
        // 単純にプレフィックス一致で探す。
        let find_by_prefix = |prefix: &str| -> Option<std::path::PathBuf> {
            std::fs::read_dir(&binaries_dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(prefix) && n.ends_with(std::env::consts::EXE_SUFFIX))
            })
        };
        let (Some(ffmpeg_src), Some(ffprobe_src)) = (find_by_prefix("ffmpeg-"), find_by_prefix("ffprobe-")) else {
            eprintln!("src-tauri/binaries/にffmpeg/ffprobeが無いためスキップ / skipping: run scripts/fetch-ffmpeg-sidecars.sh first");
            return;
        };

        let exe = std::env::current_exe().unwrap();
        let dir = exe.parent().unwrap().to_path_buf();
        // 実行時に探すのはbareな名前(sidecar::find_sidecarと同じ規約)。
        let ffprobe_sidecar = dir.join(format!("ffprobe{}", std::env::consts::EXE_SUFFIX));
        let ffmpeg_sidecar = dir.join(format!("ffmpeg{}", std::env::consts::EXE_SUFFIX));
        std::fs::copy(&ffprobe_src, &ffprobe_sidecar).expect("failed to place ffprobe sidecar next to the test binary");
        std::fs::copy(&ffmpeg_src, &ffmpeg_sidecar).expect("failed to place ffmpeg sidecar next to the test binary");

        let cleanup = || {
            let _ = std::fs::remove_file(&ffprobe_sidecar);
            let _ = std::fs::remove_file(&ffmpeg_sidecar);
        };

        // 同梱ffmpeg(=ffprobe_sidecarの隣に置いたffmpeg_sidecar自体では
        // なく、上でコピーしたffmpeg_sidecarパス)でごく短いテスト動画を
        // 生成し、それを同梱ffprobeでprobeする。
        let tmp = std::env::temp_dir().join(format!("make_disk_sidecar_e2e_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let test_file = tmp.join("sidecar_test.mp4");
        let gen_status = Command::new(&ffmpeg_sidecar)
            .args(["-y", "-f", "lavfi", "-i", "testsrc=duration=2:size=64x64:rate=5", "-c:v", "libx264", "-pix_fmt", "yuv420p", test_file.to_str().unwrap()])
            .output();
        if gen_status.is_err() || !gen_status.as_ref().unwrap().status.success() {
            cleanup();
            let _ = std::fs::remove_dir_all(&tmp);
            panic!("failed to generate test fixture with the bundled ffmpeg sidecar: {gen_status:?}");
        }

        let result = probe(test_file.to_str().unwrap());
        cleanup();
        let _ = std::fs::remove_dir_all(&tmp);

        let info = result.expect("probe() should succeed using the bundled ffprobe sidecar");
        assert!((info.duration_secs - 2.0).abs() < 0.5, "expected ~2s duration from the bundled sidecar, got {}", info.duration_secs);
    }

    /// 実際にビルドした`rs-ffmpeg`バイナリを実行ファイルの隣へ配置し、
    /// `probe_with_rs_ffmpeg`(本家ffprobeが無い場合のフォールバック経路の
    /// 中身)が本物のWAVファイルを正しく解析することを検証する
    /// (2026-09-17新設、ユーザー指示「もう一つのオープンソースのRust版
    /// も同梱して呼び出すように」への対応の直接検証——モックに頼らない
    /// 実機E2E)。実ffmpegでWAVを生成し、実rs-ffmpegでprobeする。
    /// `F:\rs-FFmpeg`をcloneしてリリースビルド済みでない環境ではスキップする。
    #[test]
    fn probe_with_rs_ffmpeg_actually_parses_a_real_wav_via_the_bundled_binary() {
        let rs_ffmpeg_release = std::path::PathBuf::from("F:\\rs-FFmpeg\\target\\release\\rs-ffmpeg.exe");
        if !rs_ffmpeg_release.is_file() {
            eprintln!("F:\\rs-FFmpeg のリリースビルドが無いためスキップ / skipping: build rs-FFmpeg first");
            return;
        }
        if !Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false) {
            eprintln!("ffmpegが見つからないためスキップ(テスト用WAV生成に必要) / skipping: ffmpeg not found (needed to generate the test WAV)");
            return;
        }

        let exe = std::env::current_exe().unwrap();
        let dir = exe.parent().unwrap().to_path_buf();
        let sidecar_path = dir.join(format!("rs-ffmpeg{}", std::env::consts::EXE_SUFFIX));
        std::fs::copy(&rs_ffmpeg_release, &sidecar_path).expect("failed to place rs-ffmpeg sidecar next to the test binary");

        let tmp = std::env::temp_dir().join(format!("make_disk_test_rsffmpeg_probe_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let wav_path = tmp.join("test.wav");
        let gen_status = Command::new("ffmpeg")
            .args(["-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=3", "-ar", "44100", "-ac", "2", "-c:a", "pcm_s16le", wav_path.to_str().unwrap()])
            .output();

        let result = if gen_status.is_ok() && gen_status.as_ref().unwrap().status.success() {
            Some(probe_with_rs_ffmpeg(wav_path.to_str().unwrap()))
        } else {
            None
        };

        let _ = std::fs::remove_file(&sidecar_path);
        let _ = std::fs::remove_dir_all(&tmp);

        let result = result.expect("failed to generate the test WAV fixture with ffmpeg");
        let info = result.expect("probe_with_rs_ffmpeg should succeed using the bundled rs-ffmpeg binary");
        assert_eq!(info.format_name, "wav");
        assert!((info.duration_secs - 3.0).abs() < 0.2, "expected ~3s duration from rs-ffmpeg probe, got {}", info.duration_secs);
        assert_eq!(info.bit_rate, Some(44100 * 2 * 16), "非圧縮WAVのビットレートはsample_rate*channels*bits_per_sampleのはず");
    }
}
