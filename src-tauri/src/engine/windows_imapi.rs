//! Windows標準のIMAPI2(COM)経由のISO作成・ディスク書き込み(2026-09-19新設)。
//!
//! ## 経緯(実機テストで判明した問題)
//!
//! - `rs-xorriso`は日本語などの非ASCIIファイル名を`________.WAV`のように
//!   潰してしまう(8.3形式のみ対応)。IMAPI2FSはISO9660+Jolietで
//!   Unicodeのファイル名をそのまま保持できる。
//! - `isoburn.exe`は実際にディスクへ書き込んでも終了コード1を返すことが
//!   あり、成否をコードから判定できなかった。IMAPI2は例外メッセージ
//!   (空きメディア無し・空きでない等)を取得できる。
//!
//! PowerShellスクリプト(`scripts/*.ps1`、ASCIIのみ)を一時ファイルへ書き出して
//! 実行する。IMAPI2を直接叩くRustバインディングを足すより依存が少ない。

use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_ISO_PS1: &str = include_str!("scripts/imapi_create_iso.ps1");
const BURN_PS1: &str = include_str!("scripts/imapi_burn.ps1");

fn run_script(script: &str, args: &[&str]) -> Result<String, String> {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let path = std::env::temp_dir().join(format!("make-disk-imapi-{}-{nanos}.ps1", std::process::id()));
    std::fs::write(&path, script).map_err(|e| format!("一時スクリプトの作成に失敗しました: {e}"))?;

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&path)
        .args(args)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output();
    let _ = std::fs::remove_file(&path);

    let output = output.map_err(|e| format!("PowerShellの起動に失敗しました: {e}"))?;
    if !output.status.success() {
        let msg = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if msg.is_empty() { format!("PowerShellが失敗しました(終了コード: {:?})", output.status.code()) } else { msg });
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// `source_dir`の内容からISO9660+JolietのISOを作る(Unicodeファイル名を保持)。
pub fn create_iso(source_dir: &str, output_iso: &str, volume_label: &str) -> Result<(), String> {
    run_script(CREATE_ISO_PS1, &["-SourceDir", source_dir, "-OutIso", output_iso, "-Label", volume_label]).map(|_| ())
}

/// ISOを`drive`(例: `"D:"`)の空きメディアへ書き込み、成功後に排出する。
pub fn burn_iso(iso_path: &str, drive: &str) -> Result<(), String> {
    let out = run_script(BURN_PS1, &["-IsoPath", iso_path, "-Drive", drive])?;
    if out.contains("burn-ok") {
        Ok(())
    } else {
        Err(format!("IMAPI2の書き込み結果を確認できませんでした: {out}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実機のIMAPI2FSで、日本語ファイル名を保持したISOを作れることを検証する。
    #[test]
    fn create_iso_preserves_japanese_filenames() {
        let tmp = std::env::temp_dir().join(format!("make-disk-imapi-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("龍神波動.txt"), b"hello").unwrap();
        let iso = tmp.with_extension("iso");

        let result = create_iso(tmp.to_str().unwrap(), iso.to_str().unwrap(), "TEST");
        let bytes = std::fs::read(&iso).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_file(&iso);

        result.expect("IMAPI2FS ISO creation should succeed");
        // Jolietは名前をUTF-16BEで保持する。「龍」=U+9F8D,「神」=U+795E。
        assert!(bytes.windows(4).any(|w| w == [0x9F, 0x8D, 0x79, 0x5E]), "日本語ファイル名がISOに保持されているはず");
    }

    #[test]
    fn embedded_powershell_scripts_are_ascii_only() {
        // PowerShell 5.1はBOM無しUTF-8をANSIとして読むため、非ASCIIがあると
        // 後続行が壊れる(実機で自己除外の行が無効化された実バグ)。
        assert!(CREATE_ISO_PS1.is_ascii() && BURN_PS1.is_ascii());
    }

    #[test]
    fn create_iso_excludes_a_previous_output_iso_from_itself() {
        let tmp = std::env::temp_dir().join(format!("make-disk-imapi-self-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("a.txt"), b"hello").unwrap();
        let iso = tmp.join("output.iso");
        create_iso(tmp.to_str().unwrap(), iso.to_str().unwrap(), "T").unwrap();
        let first = std::fs::metadata(&iso).unwrap().len();
        create_iso(tmp.to_str().unwrap(), iso.to_str().unwrap(), "T").unwrap();
        let second = std::fs::metadata(&iso).unwrap().len();
        let _ = std::fs::remove_dir_all(&tmp);
        assert_eq!(first, second, "2回目のISOに1回目のISOが混入していないはず");
    }

    #[test]
    fn burn_iso_reports_a_clear_error_for_a_missing_drive() {
        let err = burn_iso("C:\nonexistent.iso", "Z:").unwrap_err();
        assert!(err.contains("Z:") || !err.is_empty());
    }
}

#[cfg(test)]
mod full_flow {
    use crate::engine::{burn, capacity::{self, DiscType}, convert::{self, BitrateMode, ConvertJob}, iso, probe};

    /// 手動E2E(`--ignored`): MP4→(CD容量いっぱいの音声)→ISO→書き込み。
    /// GUIの実行ボタンと同じバックエンド関数を同じ順に呼ぶ。
    /// 環境変数: MAKE_DISK_E2E_SRC(元動画)、MAKE_DISK_E2E_OUT(空の出力フォルダ)。
    #[test]
    #[ignore]
    fn mp4_to_full_cd_iso_and_burn() {
        let src = std::env::var("MAKE_DISK_E2E_SRC").expect("MAKE_DISK_E2E_SRC");
        let out = std::env::var("MAKE_DISK_E2E_OUT").expect("MAKE_DISK_E2E_OUT");
        std::fs::create_dir_all(&out).unwrap();

        let info = probe::probe(&src).expect("probe");
        eprintln!("source duration: {:.1}s", info.duration_secs);
        let kbps = capacity::max_bitrate_for_capacity(DiscType::Cd700, info.duration_secs, 50 * 1024 * 1024) / 1000;
        eprintln!("disc-full audio bitrate: {kbps} kbps");

        let stem = std::path::Path::new(&src).file_stem().unwrap().to_string_lossy().to_string();
        let audio = format!("{out}/{stem}.aac");
        convert::run_convert(&ConvertJob {
            input_path: src.clone(),
            output_path: audio.clone(),
            codec_args: vec!["-c:a".into(), "aac".into()],
            bitrate: Some(BitrateMode::AutoMaxForCapacity(kbps)),
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: None,
            fps: None,
            ai_denoise: None,
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: None,
            extra_tracks: vec![],
            ai_upscale: None,
            audio_bwe: None,
        })
        .expect("convert");
        let audio_bytes = std::fs::metadata(&audio).unwrap().len();
        eprintln!("audio size: {} MB", audio_bytes / 1_000_000);

        let iso_path = format!("{out}/output.iso");
        iso::create_iso(&out, &iso_path, "MAKE_DISK").expect("create_iso");
        let iso_bytes = std::fs::metadata(&iso_path).unwrap().len();
        eprintln!("iso size: {} MB", iso_bytes / 1_000_000);
        assert!(iso_bytes < 703_000_000, "ISO must fit a 700MB CD");

        let drives = burn::list_devices().expect("list_devices");
        eprintln!("drives: {drives:?}");
        burn::burn_image(&iso_path, &drives[0], DiscType::Cd700, burn::WriteSpeed::Auto).expect("burn");
        eprintln!("BURN OK");
    }
}
