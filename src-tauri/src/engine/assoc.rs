//! ファイルの関連付け(Windows): 拡張子ごとに「今開くアプリ」を調べ、open-mediaを「開くアプリの候補」に登録する。
//!
//! Windowsは既定のアプリの選択(UserChoice)をハッシュで保護していて、アプリから強制的に書き換えることはできない
//! (回避もしない)。そこで、open-mediaを候補(OpenWithProgids)に登録し、既定の選択はWindowsの設定画面でユーザーが行う。
//! 登録先はHKCU(管理者権限は不要、このユーザーだけ)。

use serde::Serialize;
use std::process::Command;

const PROGID: &str = "open-media.Media";

#[derive(Debug, Clone, Serialize)]
pub struct AssocInfo {
    pub ext: String,
    /// 今の既定(UserChoiceまたはHKCRの既定)のProgID。無ければ空。
    pub progid: String,
    /// 見やすい名前(実行ファイル名または種類の説明)。無ければ「未設定」。
    pub app: String,
    /// open-mediaが候補に登録済みか。
    pub registered: bool,
    /// 今の既定がopen-mediaか。
    pub is_default: bool,
}

#[cfg(windows)]
fn reg_out(args: &[&str]) -> Option<String> {
    use std::os::windows::process::CommandExt;
    let o = Command::new("reg").args(args).creation_flags(0x0800_0000).output().ok()?;
    if o.status.success() {
        Some(String::from_utf8_lossy(&o.stdout).to_string())
    } else {
        None
    }
}
#[cfg(not(windows))]
fn reg_out(_args: &[&str]) -> Option<String> {
    let _ = Command::new("true");
    None
}

/// `reg query`の出力から、指定した値名のデータを取り出す(値名`(既定)`は`/ve`の結果)。
fn parse_value(out: &str, name: &str) -> Option<String> {
    for line in out.lines() {
        let t = line.trim();
        let parts: Vec<&str> = t.splitn(3, "    ").map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if parts.len() >= 3 && parts[0].eq_ignore_ascii_case(name) {
            return Some(parts[2].to_string());
        }
    }
    None
}

fn exe_name_from_command(cmd: &str) -> Option<String> {
    let c = cmd.trim();
    let path = if let Some(rest) = c.strip_prefix('"') { rest.split('"').next()? } else { c.split(' ').next()? };
    std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string())
}

fn default_value(out: &str) -> Option<String> {
    parse_value(out, "(既定)").or_else(|| parse_value(out, "(Default)"))
}

fn friendly(progid: &str) -> String {
    if progid.is_empty() {
        return "未設定 / not set".into();
    }
    if let Some(app) = progid.strip_prefix("Applications\\") {
        return app.to_string();
    }
    // Microsoft Storeのアプリ(ProgIDが`AppX…`の長い名前)は、登録情報のパッケージ名から見やすい名前を作る。
    if progid.starts_with("AppX") {
        if let Some(out) = reg_out(&["query", &format!(r"HKCR\{progid}\Application"), "/v", "ApplicationName"]) {
            if let Some(v) = parse_value(&out, "ApplicationName") {
                if let Some(pkg) = v.strip_prefix("@{").and_then(|r| r.split('_').next()) {
                    return match pkg {
                        "Microsoft.ZuneMusic" => "メディア プレーヤー / Media Player (Microsoft Store)".to_string(),
                        "Microsoft.ZuneVideo" => "映画 & テレビ / Movies & TV (Microsoft Store)".to_string(),
                        _ => format!("{pkg} (Microsoft Store)"),
                    };
                }
            }
        }
    }
    if let Some(out) = reg_out(&["query", &format!(r"HKCR\{progid}\shell\open\command"), "/ve"]) {
        if let Some(n) = default_value(&out).and_then(|c| exe_name_from_command(&c)) {
            return n;
        }
    }
    if let Some(d) = reg_out(&["query", &format!(r"HKCR\{progid}"), "/ve"]).and_then(|o| default_value(&o)) {
        return d;
    }
    progid.to_string()
}

pub fn list(exts: &[String]) -> Vec<AssocInfo> {
    exts.iter()
        .map(|e| {
            let ext = e.trim_start_matches('.').to_lowercase();
            let key = format!(".{ext}");
            let progid = reg_out(&["query", &format!(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{key}\UserChoice"), "/v", "ProgId"])
                .and_then(|o| parse_value(&o, "ProgId"))
                .or_else(|| reg_out(&["query", &format!(r"HKCR\{key}"), "/ve"]).and_then(|o| default_value(&o)))
                .unwrap_or_default();
            let registered = reg_out(&["query", &format!(r"HKCU\Software\Classes\{key}\OpenWithProgids"), "/v", PROGID]).is_some();
            let is_default = progid == PROGID;
            AssocInfo { app: friendly(&progid), ext, progid, registered, is_default }
        })
        .collect()
}

fn reg_set(args: &[&str]) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let o = Command::new("reg").args(args).creation_flags(0x0800_0000).output().map_err(|e| e.to_string())?;
        if o.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (Command::new("true"), args);
        Err("Windowsのみ対応です".into())
    }
}

/// open-mediaを選んだ拡張子の「開くアプリの候補」に登録する。
pub fn register(exts: &[String]) -> Result<usize, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().to_string();
    let cmd = format!("\"{exe}\" \"%1\"");
    reg_set(&["add", &format!(r"HKCU\Software\Classes\{PROGID}"), "/ve", "/d", "open-media", "/f"])?;
    reg_set(&["add", &format!(r"HKCU\Software\Classes\{PROGID}\DefaultIcon"), "/ve", "/d", &format!("{exe},0"), "/f"])?;
    reg_set(&["add", &format!(r"HKCU\Software\Classes\{PROGID}\shell\open\command"), "/ve", "/d", &cmd, "/f"])?;
    let mut n = 0;
    for e in exts {
        let ext = e.trim_start_matches('.').to_lowercase();
        reg_set(&["add", &format!(r"HKCU\Software\Classes\.{ext}\OpenWithProgids"), "/v", PROGID, "/t", "REG_SZ", "/d", "", "/f"])?;
        n += 1;
    }
    Ok(n)
}

/// 候補から外す。
pub fn unregister(exts: &[String]) {
    for e in exts {
        let ext = e.trim_start_matches('.').to_lowercase();
        let _ = reg_set(&["delete", &format!(r"HKCU\Software\Classes\.{ext}\OpenWithProgids"), "/v", PROGID, "/f"]);
    }
}

/// Windowsの「既定のアプリ」設定画面を開く(既定の選択はここでユーザーが行う)。
pub fn open_default_apps_settings() -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("cmd").args(["/C", "start", "", "ms-settings:defaultapps"]).creation_flags(0x0800_0000).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        Err("Windowsのみ対応です".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reg_query_output() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\X\r\n    ProgId    REG_SZ    Applications\\vlc.exe\r\n";
        assert_eq!(parse_value(out, "ProgId").as_deref(), Some("Applications\\vlc.exe"));
        let d = "HKEY_CLASSES_ROOT\\mp3file\r\n    (既定)    REG_SZ    MP3 ファイル\r\n";
        assert_eq!(default_value(d).as_deref(), Some("MP3 ファイル"));
    }

    /// 実機テスト(Windows): 実際のレジストリで、現在のアプリの取得→登録→確認→解除を行う。
    /// `cargo test --lib assoc_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn assoc_real_register_and_unregister() {
        let exts: Vec<String> = ["mp3", "flac", "mp4"].iter().map(|s| s.to_string()).collect();
        let before = list(&exts);
        for r in &before {
            println!("before: .{} app={} progid={} registered={} default={}", r.ext, r.app, r.progid, r.registered, r.is_default);
        }
        assert!(before.iter().all(|r| !r.registered), "テスト前にすでに登録済み");
        let n = register(&exts).expect("登録できない");
        assert_eq!(n, 3);
        let mid = list(&exts);
        for r in &mid {
            println!("after register: .{} registered={}", r.ext, r.registered);
        }
        assert!(mid.iter().all(|r| r.registered), "登録が反映されていない");
        unregister(&exts);
        let after = list(&exts);
        assert!(after.iter().all(|r| !r.registered), "解除できていない");
        // 解除後に、open-media.Media自体の登録も消す
        let _ = reg_set(&["delete", &format!(r"HKCU\Software\Classes\{PROGID}"), "/f"]);
        println!("restored");
    }

    #[test]
    fn exe_name_handles_quotes() {
        assert_eq!(exe_name_from_command("\"C:\\Program Files\\VLC\\vlc.exe\" --started-from-file \"%1\"").as_deref(), Some("vlc.exe"));
    }
}
