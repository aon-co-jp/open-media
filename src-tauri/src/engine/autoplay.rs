//! Windowsの「自動再生」にopen-mediaを登録する(ユーザーが画面のボタンを押したときだけ)。
//!
//! 音楽CD(PlayCDAudioOnArrival)とDVDビデオ(PlayDVDMovieOnArrival)を入れたとき、open-mediaを既定の動作にする。
//! 登録先はHKCU(管理者権限は不要、このユーザーだけ)。解除すると、open-mediaの登録と既定の選択を消し、
//! 次からWindowsが「どうするか」を尋ねる状態に戻る(以前の既定は記録していないため復元はしない)。
//! Blu-rayには自動再生のイベント名が確認できていないため、ここでは扱わない。

use std::process::Command;

const HANDLER: &str = "OpenMediaDisc";
const PROGID: &str = "open-media.AutoPlay";
const BASE: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\AutoplayHandlers";
const EVENTS: [&str; 2] = ["PlayCDAudioOnArrival", "PlayDVDMovieOnArrival"];
/// 有効にする前の既定の選択を控える場所(解除で元に戻す)。
const BACKUP: &str = r"HKCU\Software\open-media\AutoplayBackup";

fn reg(args: &[&str]) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let out = Command::new("reg").args(args).creation_flags(0x0800_0000).output().map_err(|e| format!("reg.exeを起動できません: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (Command::new("true"), args);
        Err("Windowsのみ対応です".into())
    }
}

/// open-mediaを自動再生の候補に登録し、音楽CD・DVDの既定にする。
pub fn enable() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_string_lossy().to_string();
    let cmd = format!("\"{exe}\" --play-disc \"%1\"");
    reg(&["add", &format!(r"HKCU\Software\Classes\{PROGID}\shell\play\command"), "/ve", "/d", &cmd, "/f"])?;
    let h = format!(r"{BASE}\Handlers\{HANDLER}");
    reg(&["add", &h, "/v", "Action", "/d", "open-mediaで再生 / Play with open-media", "/f"])?;
    reg(&["add", &h, "/v", "Provider", "/d", "open-media", "/f"])?;
    reg(&["add", &h, "/v", "InvokeProgID", "/d", PROGID, "/f"])?;
    reg(&["add", &h, "/v", "InvokeVerb", "/d", "play", "/f"])?;
    reg(&["add", &h, "/v", "DefaultIcon", "/d", &format!("{exe},0"), "/f"])?;
    for ev in EVENTS {
        // 有効にする前の既定の選択(例: 別の再生アプリ)を控えておき、解除で元に戻せるようにする。
        // すでにopen-mediaが既定のときは、控えを上書きしない。
        if let Some(prev) = current_choice(ev) {
            if prev != HANDLER {
                reg(&["add", &format!(r"{BACKUP}"), "/v", ev, "/d", &prev, "/f"])?;
            }
        }
        reg(&["add", &format!(r"{BASE}\EventHandlers\{ev}"), "/v", HANDLER, "/d", "", "/f"])?;
        reg(&["add", &format!(r"{BASE}\UserChosenExecuteHandlers"), "/v", ev, "/d", HANDLER, "/f"])?;
    }
    Ok(exe)
}

/// 今の既定の選択(UserChosenExecuteHandlersの値)を読む。無ければNone。
fn current_choice(ev: &str) -> Option<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let o = Command::new("reg").args(["query", &format!(r"{BASE}\UserChosenExecuteHandlers"), "/v", ev]).creation_flags(0x0800_0000).output().ok()?;
        if !o.status.success() {
            return None;
        }
        let out = String::from_utf8_lossy(&o.stdout).to_string();
        out.lines().find_map(|l| {
            let parts: Vec<&str> = l.trim().splitn(3, "    ").map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            (parts.len() >= 3 && parts[0].eq_ignore_ascii_case(ev)).then(|| parts[2].to_string())
        })
    }
    #[cfg(not(windows))]
    {
        let _ = ev;
        None
    }
}

/// 控えておいた以前の既定の選択を返す。
fn backed_up_choice(ev: &str) -> Option<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let o = Command::new("reg").args(["query", BACKUP, "/v", ev]).creation_flags(0x0800_0000).output().ok()?;
        if !o.status.success() {
            return None;
        }
        let out = String::from_utf8_lossy(&o.stdout).to_string();
        out.lines().find_map(|l| {
            let parts: Vec<&str> = l.trim().splitn(3, "    ").map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            (parts.len() >= 3 && parts[0].eq_ignore_ascii_case(ev)).then(|| parts[2].to_string())
        })
    }
    #[cfg(not(windows))]
    {
        let _ = ev;
        None
    }
}

/// 登録を消し、有効にする前の既定の選択があれば元に戻す(無ければ、次からWindowsが「どうするか」を尋ねる)。
pub fn disable() -> Result<(), String> {
    for ev in EVENTS {
        let _ = reg(&["delete", &format!(r"{BASE}\EventHandlers\{ev}"), "/v", HANDLER, "/f"]);
        match backed_up_choice(ev) {
            Some(prev) => {
                let _ = reg(&["add", &format!(r"{BASE}\UserChosenExecuteHandlers"), "/v", ev, "/d", &prev, "/f"]);
                let _ = reg(&["delete", BACKUP, "/v", ev, "/f"]);
            }
            None => {
                let _ = reg(&["delete", &format!(r"{BASE}\UserChosenExecuteHandlers"), "/v", ev, "/f"]);
            }
        }
    }
    let _ = reg(&["delete", &format!(r"{BASE}\Handlers\{HANDLER}"), "/f"]);
    let _ = reg(&["delete", &format!(r"HKCU\Software\Classes\{PROGID}"), "/f"]);
    Ok(())
}

/// 今、既定になっているか(音楽CDの既定がopen-mediaか)。
pub fn is_enabled() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("reg")
            .args(["query", &format!(r"{BASE}\UserChosenExecuteHandlers"), "/v", "PlayCDAudioOnArrival"])
            .creation_flags(0x0800_0000)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(HANDLER))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実機テスト(Windows): 今の既定の選択を控え、有効化→確認→解除して、元の値に戻ることを確かめる。
    /// `cargo test --lib autoplay_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn autoplay_real_enable_and_restore() {
        let before: Vec<Option<String>> = EVENTS.iter().map(|e| current_choice(e)).collect();
        println!("before: {before:?}");
        assert!(!is_enabled(), "テスト前からopen-mediaが既定になっている");
        enable().expect("有効化できない");
        assert!(is_enabled(), "有効化が反映されていない");
        for e in EVENTS {
            assert_eq!(current_choice(e).as_deref(), Some(HANDLER));
        }
        println!("enabled ok");
        disable().unwrap();
        let after: Vec<Option<String>> = EVENTS.iter().map(|e| current_choice(e)).collect();
        println!("after: {after:?}");
        assert!(!is_enabled(), "解除できていない");
        assert_eq!(before, after, "元の既定の選択に戻っていない");
        // 控えの場所も片付ける
        let _ = reg(&["delete", r"HKCU\Software\open-media", "/f"]);
        println!("restored");
    }
}
