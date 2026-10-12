//! ディスク書き込み(xorriso -as cdrecord/growisofs 経由で統一)。
//! xorrisoはcdrtools(cdrecord)・cdrdao・libburn/growisofs相当の書き込み経路を
//! 内包しているため、CD/DVD/Blu-rayを単一コマンド体系で扱える。
//!
//! ## rs-xorrisoについて(2026-09-17)
//!
//! `engine::iso`のISO生成とは異なり、実際のディスクへの書き込み
//! (`-as cdrecord`)・ドライブ列挙(`-devices`)はrs-xorriso側が
//! 明示的に「未実装、本家xorrisoを使ってください」という分かりやすい
//! エラーを返す設計になっている(ISO生成のみ対応、正直な開示)。
//! そのため本家xorrisoが無い環境でここへフォールバックしても実際の
//! 書き込みは行えないが、OSレベルの生の「program not found」より
//! ずっと分かりやすいエラーメッセージになる。

use crate::engine::capacity::DiscType;
use crate::engine::sidecar::resolve_tool;

fn run_xorriso(args: &[String]) -> Result<std::process::Output, String> {
    match resolve_tool("xorriso").args(args).output() {
        Ok(output) => Ok(output),
        Err(_) => resolve_tool("rs-xorriso")
            .args(args)
            .output()
            .map_err(|e| format!("xorriso・rs-xorrisoともに起動に失敗しました(いずれも未インストール/未同梱の可能性): {e}")),
    }
}

/// 書き込み速度。`Auto`は指定を省略し、ドライブ・メディアの自動判定に委ねる。
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteSpeed {
    Auto,
    Max,
    Fixed(u32),
}

/// `"D:"`のようなWindowsドライブレター形式か。
fn is_windows_drive_letter(device: &str) -> bool {
    let b = device.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// PowerShell(`Get-CimInstance Win32_CDROMDrive`)の出力からドライブレターを抽出する。
fn parse_windows_drive_list(stdout: &str) -> Vec<String> {
    stdout.lines().map(|l| l.trim()).filter(|l| is_windows_drive_letter(l)).map(|l| l.to_ascii_uppercase()).collect()
}

/// WindowsではIMAPI2でISOを書き込む(2026-09-19新設)。本家xorrisoを同梱
/// していないWindowsでも実際に書き込める経路。**正直な開示**: 書き込み速度・
/// ディスク種別の指定は現状無視される(メディアに応じて自動判定)。
#[cfg(windows)]
fn burn_with_imapi(image_path: &str, drive: &str) -> Result<(), String> {
    crate::engine::windows_imapi::burn_iso(image_path, drive)
}

#[cfg(not(windows))]
fn burn_with_imapi(_image_path: &str, _drive: &str) -> Result<(), String> {
    Err("IMAPI2はWindows専用です".to_string())
}

pub fn burn_image(image_path: &str, device: &str, disc: DiscType, speed: WriteSpeed) -> Result<(), String> {
    if is_windows_drive_letter(device) {
        return burn_with_imapi(image_path, device);
    }
    let mut args: Vec<String> = vec!["-as".into(), "cdrecord".into()];

    match speed {
        WriteSpeed::Auto => {}
        WriteSpeed::Max => args.push("speed=0".into()),
        WriteSpeed::Fixed(v) => args.push(format!("speed={v}")),
    }

    args.push(format!("dev={device}"));

    match disc {
        DiscType::Cd700 => {
            args.push("-v".into());
            args.push("-data".into());
        }
        DiscType::Dvd47 | DiscType::DvdDl85 | DiscType::Bd25 | DiscType::Bd50 | DiscType::Bd100 | DiscType::Bd128 => {
            args.push("-dao".into());
        }
    }

    args.push(image_path.to_string());

    let output = run_xorriso(&args)?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

/// 利用可能な光学ドライブの一覧(device文字列)。
pub fn list_devices() -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // 書き込み対応(Capabilitiesに4=Supports Writingを含む)の光学ドライブのみ列挙する。
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_CDROMDrive | Where-Object { $_.Capabilities -contains 4 } | ForEach-Object { $_.Drive }",
            ])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output()
            .map_err(|e| format!("光学ドライブの列挙に失敗しました: {e}"))?;
        Ok(parse_windows_drive_list(&String::from_utf8_lossy(&output.stdout)))
    }
    #[cfg(not(windows))]
    {
        list_devices_xorriso()
    }
}

#[cfg(not(windows))]
fn list_devices_xorriso() -> Result<Vec<String>, String> {
    let output = run_xorriso(&["-devices".to_string()])?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();
    for line in text.lines() {
        if let Some(idx) = line.find("dev='") {
            if let Some(end) = line[idx + 5..].find('\'') {
                devices.push(line[idx + 5..idx + 5 + end].to_string());
            }
        }
    }
    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_windows_drive_list_extracts_drive_letters_only() {
        assert_eq!(parse_windows_drive_list("D:\r\ne:\r\n\r\ngarbage\r\n"), vec!["D:", "E:"]);
        assert!(parse_windows_drive_list("").is_empty());
    }

    #[test]
    fn windows_drive_letter_detection() {
        assert!(is_windows_drive_letter("D:"));
        assert!(!is_windows_drive_letter("/dev/sr0"));
        assert!(!is_windows_drive_letter("D:\\"));
    }

    /// 実機のWindowsで本当にPowerShell経由で光学ドライブを列挙できることを検証する
    /// (ドライブが無い環境では空でも成功とする)。
    #[cfg(windows)]
    #[test]
    fn list_devices_actually_queries_windows_optical_drives() {
        let devices = list_devices().expect("list_devices should not fail on Windows");
        eprintln!("detected writable optical drives: {devices:?}");
        assert!(devices.iter().all(|d| is_windows_drive_letter(d)));
    }
}

#[cfg(all(test, windows))]
mod real_disc_tests {
    use super::*;

    /// 実際に空きメディアへ書き込む手動テスト(`cargo test -- --ignored`)。
    /// 環境変数`MAKE_DISK_TEST_ISO`にISOのパスを指定する。
    #[test]
    #[ignore]
    fn burn_image_actually_burns_a_real_disc_on_windows() {
        let iso = std::env::var("MAKE_DISK_TEST_ISO").expect("set MAKE_DISK_TEST_ISO");
        let devices = list_devices().unwrap();
        assert!(!devices.is_empty(), "no writable optical drive");
        burn_image(&iso, &devices[0], DiscType::Cd700, WriteSpeed::Auto).expect("burn should succeed");
    }
}
