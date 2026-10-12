//! open-easy-web のインストール配置(2026-09-25新設)。
//!
//! ```text
//! %LOCALAPPDATA%\open-easy-web\          ← 一番上(macOS: ~/Library/Application Support/open-easy-web、Linux: $XDG_DATA_HOME/open-easy-web)
//!   make-disk\                           ← make-disk本体(NSISの既定のインストール先)とその`plugins\`
//!   aruaru-llm\                          ← 予約(LLMマネージャ実装時に使う)
//!   open-web-server\                     ← 予約(ローカルゲートウェイ実装時に使う)
//!   open-cpu\  open-directx\  open-cuda\ ← ライブラリ。今はmake-disk/aruaru-llmに組み込み済みで、単独の実行物は無い
//! ```
//! 各フォルダには`component.json`(状態の説明)を置く。**中身の無いフォルダを実体があるように見せない**ため、
//! 状態(`embedded`=組み込み済み / `planned`=未実装で予約)を必ず書く。
//!
//! 旧配置(`%LOCALAPPDATA%\make-disk\plugins`)があれば、初回起動時に新しい配置へ**移動**する(取得済みのAI/RIFEを再ダウンロードさせない)。

use std::path::{Path, PathBuf};

const COMPONENTS: &[(&str, &str, &str, &str)] = &[
    ("aruaru-llm", "planned", "ローカルLLM(未実装。LLMマネージャの実装時にここへ置く)", "Local LLM (not implemented yet; will live here once the LLM manager exists)"),
    ("open-web-server", "planned", "ローカルのゲートウェイ(未実装)", "Local gateway (not implemented yet)"),
    ("open-cpu", "embedded", "ライブラリ。make-diskとaruaru-llmに組み込み済みで、単独の実行物は無い", "Library, embedded in make-disk and aruaru-llm; no standalone executable"),
    ("open-directx", "embedded", "ライブラリ。make-diskとaruaru-llmに組み込み済みで、単独の実行物は無い", "Library, embedded in make-disk and aruaru-llm; no standalone executable"),
    ("open-cuda", "embedded", "ライブラリ。make-diskとaruaru-llmに組み込み済みで、単独の実行物は無い", "Library, embedded in make-disk and aruaru-llm; no standalone executable"),
];

fn data_base() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }
}

/// 一番上のフォルダ(`OPEN_EASY_WEB_DIR`で上書きできる。テスト用)。
pub fn root_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("OPEN_EASY_WEB_DIR") {
        return Some(PathBuf::from(d));
    }
    Some(data_base()?.join("open-easy-web"))
}

/// make-diskのフォルダ(`plugins\`などはこの下)。
pub fn app_dir() -> Option<PathBuf> {
    Some(root_dir()?.join("make-disk"))
}

/// 旧配置(open-easy-web導入前)のmake-diskのフォルダ。
fn legacy_app_dir() -> Option<PathBuf> {
    if std::env::var_os("OPEN_EASY_WEB_DIR").is_some() {
        return None; // 上書き時(テスト)は旧配置を触らない
    }
    Some(data_base()?.join("make-disk"))
}

/// フォルダを`from`から`to`へ移す。同じドライブなら改名、だめなら複製して元を消す。
fn move_dir(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(p) = to.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_dir(from, to)?;
    std::fs::remove_dir_all(from).map_err(|e| format!("旧フォルダを消せません: {e}"))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if e.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| format!("{}を複製できません: {e}", src.display()))?;
        }
    }
    Ok(())
}

/// 配置を整える(何度呼んでも安全)。戻り値は、実行した作業の説明(ログ用)。
pub fn ensure_layout() -> Vec<String> {
    let mut notes = Vec::new();
    let (Some(root), Some(app)) = (root_dir(), app_dir()) else { return notes };
    if let Err(e) = std::fs::create_dir_all(&app) {
        notes.push(format!("open-easy-webのフォルダを作れません / cannot create the open-easy-web folder: {e}"));
        return notes;
    }
    // 旧配置のプラグインを移す。
    if let Some(legacy) = legacy_app_dir() {
        let (old_plugins, new_plugins) = (legacy.join("plugins"), app.join("plugins"));
        if legacy != app && old_plugins.is_dir() {
            if new_plugins.exists() {
                notes.push(format!("旧配置のプラグインフォルダが残っています(新しい配置に既にあるため移していません): {} / legacy plugins folder left in place: {}", old_plugins.display(), old_plugins.display()));
            } else {
                match move_dir(&old_plugins, &new_plugins) {
                    Ok(()) => notes.push(format!("プラグインを新しい配置へ移しました: {} / plugins moved to the new layout: {}", new_plugins.display(), new_plugins.display())),
                    Err(e) => notes.push(format!("プラグインの移動に失敗しました(旧配置のまま再取得します) / moving plugins failed (they will be fetched again): {e}")),
                }
            }
        }
    }
    for (name, status, ja, en) in COMPONENTS {
        let dir = root.join(name);
        let info = dir.join("component.json");
        if info.is_file() {
            continue;
        }
        let body = serde_json::json!({ "name": name, "status": status, "note_ja": ja, "note_en": en });
        if std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&info, serde_json::to_vec_pretty(&body).unwrap_or_default())).is_err() {
            notes.push(format!("{name}のフォルダを作れませんでした / could not create the {name} folder"));
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("make-disk-layout-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn creates_the_tree_with_honest_status_files() {
        let d = temp("tree");
        std::env::set_var("OPEN_EASY_WEB_DIR", &d);
        ensure_layout();
        assert!(d.join("make-disk").is_dir());
        for (name, status, _, _) in COMPONENTS {
            let v: serde_json::Value = serde_json::from_slice(&std::fs::read(d.join(name).join("component.json")).unwrap()).unwrap();
            assert_eq!(v["status"], *status, "{name}");
        }
        // 2回目は何も壊さない
        std::fs::write(d.join("open-cpu").join("component.json"), b"{\"name\":\"open-cpu\",\"status\":\"custom\"}").unwrap();
        ensure_layout();
        assert!(std::fs::read_to_string(d.join("open-cpu").join("component.json")).unwrap().contains("custom"));
        std::env::remove_var("OPEN_EASY_WEB_DIR");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn move_dir_moves_nested_files() {
        let d = temp("move");
        let (a, b) = (d.join("a"), d.join("x").join("b"));
        std::fs::create_dir_all(a.join("sub")).unwrap();
        std::fs::write(a.join("sub").join("f.bin"), b"hello").unwrap();
        move_dir(&a, &b).unwrap();
        assert!(!a.exists());
        assert_eq!(std::fs::read(b.join("sub").join("f.bin")).unwrap(), b"hello");
        // 複製経路も確かめる
        let c = d.join("c");
        copy_dir(&b, &c).unwrap();
        assert_eq!(std::fs::read(c.join("sub").join("f.bin")).unwrap(), b"hello");
        let _ = std::fs::remove_dir_all(&d);
    }
}
