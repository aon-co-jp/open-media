//! make-disk を「ダブルクリック→数回の選択→完了」だけでインストールする自己完結インストーラー。
//!
//! 埋め込み(`build.rs`が`payload.zip`としてまとめ、ここで`include_bytes!`する):
//! make-disk本体・本家ffmpeg/ffprobe・実験的な純Rust版rs-ffmpeg/rs-xorriso。
//! 本家xorrisoは未同梱(`KNOWN_GAPS.txt`として払い出し、完了画面でも告知する)。
//!
//! 実行時に別途取得するもの:
//! - WebView2ランタイム(未インストールなら、既存のNSISインストーラーと同じ
//!   固定URLからブートストラッパーをダウンロードして`/silent /install`)
//! - open-bar(チェックボックスONの時だけ、GitHub Releasesから最新のWindows
//!   インストーラーを取得して`/S`でサイレント実行。肥大化を避けるため埋め込まない)
//!
//! アンインストールは「プログラムと機能」から`make-disk-installer.exe --uninstall`
//! (インストール先へコピーした自分自身)を呼ぶ形で登録する。

// 注意: `windows_subsystem = "windows"`にすると--test-install/--uninstallの標準出力・
// 終了コードの扱いが不安定になったため、当面はコンソール付き(既定)のままにする
// (GUI起動時に一瞬コンソールが出る程度で実害は無い。将来ここを見直す余地はある)。

use std::env;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use winreg::enums::*;
use winreg::RegKey;

const PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.zip"));
const APP_NAME: &str = "make-disk";
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\make-disk-installer";
const WEBVIEW2_APP_GUID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
const WEBVIEW2_BOOTSTRAPPER_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

mod gui;

fn default_install_dir() -> PathBuf {
    let base = env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("Programs").join(APP_NAME)
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--uninstall") {
        let dir = args
            .iter()
            .position(|a| a == "--dir")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from)
            .unwrap_or_else(default_install_dir);
        return uninstall(&dir);
    }
    // GUIを経由しない動作検証用(オープンな公開UIには出さない): 展開・WebView2確認・
    // レジストリ登録までを自動実行する。open-barの取得は`--with-open-bar`を付けたときだけ。
    if let Some(i) = args.iter().position(|a| a == "--test-install") {
        let dir = args.get(i + 1).map(PathBuf::from).context("--test-install には対象フォルダを指定してください")?;
        let bundle = args.iter().any(|a| a == "--with-open-bar");
        return perform_install(&dir, bundle, |msg| println!("[test-install] {msg}"));
    }
    gui::run()
}

/// インストール本体(GUIの「インストール」ボタンから呼ばれる)。
/// 途中経過は`on_progress`(日本語メッセージ)へ都度渡す。
pub fn perform_install(install_dir: &Path, bundle_open_bar: bool, on_progress: impl Fn(&str)) -> Result<()> {
    on_progress("インストール先を準備しています…");
    std::fs::create_dir_all(install_dir).with_context(|| format!("フォルダを作成できません: {}", install_dir.display()))?;

    on_progress("同梱ツールを展開しています(make-disk本体・ffmpeg・実験的Rust版)…");
    extract_payload(install_dir)?;

    on_progress("WebView2ランタイムを確認しています…");
    if let Err(e) = ensure_webview2() {
        on_progress(&format!("WebView2の確認/導入でエラー(続行します): {e}"));
    }

    if bundle_open_bar {
        on_progress("open-barの最新版を取得しています…");
        if let Err(e) = fetch_and_install_open_bar() {
            on_progress(&format!("open-barの取得/インストールに失敗しました(make-disk本体は続けます): {e}"));
        }
    }

    on_progress("アンインストール情報を登録しています…");
    register_uninstaller(install_dir)?;

    on_progress("完了しました。");
    Ok(())
}

fn extract_payload(install_dir: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(PAYLOAD)).context("同梱データの読み込みに失敗しました")?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let out_path = install_dir.join(entry.name());
        let mut out_file = std::fs::File::create(&out_path).with_context(|| format!("書き込めません: {}", out_path.display()))?;
        std::io::copy(&mut entry, &mut out_file)?;
    }
    Ok(())
}

/// 既にWebView2が入っていれば何もしない(NSIS版の`Section WebView2`と同じレジストリチェック)。
/// 無ければ小さなブートストラッパーだけダウンロードしてサイレントインストールする
/// (ランタイム本体〈数十〜百MB超〉はこの埋め込みexeへ入れない)。
fn ensure_webview2() -> Result<()> {
    let paths = [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\EdgeUpdate\Clients"),
        (HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\EdgeUpdate\Clients"),
    ];
    for (hive, base) in paths {
        let root = RegKey::predef(hive);
        if let Ok(key) = root.open_subkey(format!(r"{base}\{WEBVIEW2_APP_GUID}")) {
            let pv: Result<String, _> = key.get_value("pv");
            if pv.map(|v| !v.is_empty()).unwrap_or(false) {
                return Ok(()); // 既にインストール済み
            }
        }
    }

    let tmp = env::temp_dir().join("MicrosoftEdgeWebview2Setup.exe");
    let bytes = ureq::get(WEBVIEW2_BOOTSTRAPPER_URL)
        .call()
        .context("WebView2ブートストラッパーのダウンロードに失敗")?
        .into_reader();
    let mut bytes = bytes;
    let mut file = std::fs::File::create(&tmp)?;
    std::io::copy(&mut bytes, &mut file)?;
    drop(file);
    let status = Command::new(&tmp).arg("/silent").arg("/install").status().context("WebView2インストーラーの起動に失敗")?;
    if !status.success() {
        anyhow::bail!("WebView2インストーラーが終了コード{:?}で終了しました", status.code());
    }
    Ok(())
}

#[derive(serde::Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}
#[derive(serde::Deserialize)]
struct GhRelease {
    assets: Vec<GhAsset>,
}

/// open-barのGitHub Releasesから最新のWindowsインストーラー(`*_x64-setup.exe`)を取得し、
/// サイレントインストールする(肥大化を避けるため、埋め込まず実行時に取得する設計)。
fn fetch_and_install_open_bar() -> Result<()> {
    let json: GhRelease = ureq::get("https://api.github.com/repos/aon-co-jp/open-bar/releases/latest")
        .set("User-Agent", "make-disk-installer")
        .call()
        .context("open-barの最新リリース情報の取得に失敗")?
        .into_json()
        .context("open-barのリリース情報のJSON解析に失敗")?;
    let asset = json
        .assets
        .iter()
        .find(|a| a.name.ends_with("_x64-setup.exe"))
        .context("open-barのWindows用インストーラー資産が見つかりません")?;

    let tmp = env::temp_dir().join(&asset.name);
    let mut reader = ureq::get(&asset.browser_download_url).call().context("open-barのダウンロードに失敗")?.into_reader();
    let mut file = std::fs::File::create(&tmp)?;
    std::io::copy(&mut reader, &mut file)?;
    drop(file);
    // Tauri/NSISベースのインストーラーは`/S`でサイレット実行できる。
    let status = Command::new(&tmp).arg("/S").status().context("open-barインストーラーの起動に失敗")?;
    if !status.success() {
        anyhow::bail!("open-barインストーラーが終了コード{:?}で終了しました", status.code());
    }
    Ok(())
}

fn register_uninstaller(install_dir: &Path) -> Result<()> {
    // アンインストーラーとして自分自身を持ち込む(ダウンロード元/一時フォルダを消されても動くように)。
    let self_path = env::current_exe().context("自分自身のパスを取得できません")?;
    let installed_self = install_dir.join("make-disk-installer.exe");
    if self_path != installed_self {
        std::fs::copy(&self_path, &installed_self).context("アンインストーラーの配置に失敗しました")?;
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(UNINSTALL_KEY).context("アンインストール情報の登録に失敗しました")?;
    key.set_value("DisplayName", &"make-disk".to_string())?;
    // インストーラー自体のCargo.tomlバージョンではなく、埋め込んだmake-disk本体の
    // バージョン(build.rsが../package.jsonから読んだもの)を表示する。
    key.set_value("DisplayVersion", &env!("MAKE_DISK_APP_VERSION").to_string())?;
    key.set_value("Publisher", &"aon-co-jp".to_string())?;
    key.set_value("DisplayIcon", &installed_self.to_string_lossy().to_string())?;
    key.set_value("UninstallString", &format!("\"{}\" --uninstall --dir \"{}\"", installed_self.display(), install_dir.display()))?;
    key.set_value("InstallLocation", &install_dir.to_string_lossy().to_string())?;
    key.set_value("NoModify", &1u32)?;
    key.set_value("NoRepair", &1u32)?;
    Ok(())
}

fn uninstall(install_dir: &Path) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(UNINSTALL_KEY);

    // 実行中の自分自身が入っているフォルダは自分では消せないので、少し待ってから
    // 削除する使い捨てのcmdを切り離して起動し、このプロセスはすぐ終了する。
    let dir = install_dir.to_string_lossy().to_string();
    let _ = Command::new("cmd")
        .args(["/C", "timeout", "/T", "2", "/NOBREAK", ">nul", "&", "rd", "/S", "/Q", &dir])
        .spawn();
    Ok(())
}
