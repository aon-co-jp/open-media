//! rs-ffmpeg / rs-xorriso のバージョン管理付きプラグイン機構(2026-09-19新設)。
//!
//! ユーザー指示「rs-ffmpeg.exe/rs-xorriso.exeの二つは同梱しますがバージョン管理して、
//! プラグインとして、既にインストールしてあれば、上書きインストールの時に、その部分だけは、
//! 上書きする無駄を省いて」への対応。
//!
//! ## 仕組み
//! - インストーラーに同梱された`rs-*`(アプリ本体の隣)を「配布元(シード)」とみなし、
//!   起動時にユーザーのプラグインフォルダ(`<データフォルダ>/make-disk/plugins`)へ同期する。
//! - バージョンは内容ハッシュ(FNV-1a)+サイズ。プラグインフォルダの`<名前>.version`と同じなら
//!   **コピーせずスキップ**(UpToDate)、違えば上書き(Updated)、無ければ新規(Installed)。
//! - `resolve_tool`は`rs-*`についてプラグインフォルダを最優先で探す。
//!
//! **正直な開示**: インストーラー(NSIS/MSI)自体は、上書きインストール時に同梱の`rs-*`
//! (各約200KB)を書き直す。これを完全に省くには、`rs-*`をインストーラーから外して姉妹リポジトリの
//! リリース資産からオンデマンドで取得する必要があり、姉妹リポジトリ側のバイナリ公開が前提となる
//! ため未実装(ロードマップ)。現状は「アプリ側のプラグイン領域への不要な再コピーを省く」までを実現している。

use std::path::{Path, PathBuf};

pub const PLUGIN_NAMES: [&str; 2] = ["rs-ffmpeg", "rs-xorriso"];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginAction {
    /// 新規にプラグインフォルダへ配置した。
    Installed,
    /// 版が異なるため上書きした。
    Updated,
    /// 同じ版が既にあるため何もしなかった(上書きの無駄を省いた)。
    UpToDate,
    /// このアプリには同梱されていない。
    NotBundled,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginStatus {
    pub name: String,
    pub version: Option<String>,
    pub action: PluginAction,
    pub path: Option<String>,
}

fn exe_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// 内容から版文字列を作る(`<サイズ>-<FNV-1aハッシュ>`)。
pub fn version_of(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{}-{hash:016x}", bytes.len())
}

/// プラグインフォルダ。`MAKE_DISK_PLUGIN_DIR`で上書きできる(テスト用)。
pub fn plugin_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("MAKE_DISK_PLUGIN_DIR") {
        return Some(PathBuf::from(dir));
    }
    // open-easy-web配置(2026-09-25): <open-easy-web>/make-disk/plugins。旧配置からの移動は`layout::ensure_layout`が行う。
    Some(crate::engine::layout::app_dir()?.join("plugins"))
}

/// 配布元フォルダ`bundled_dir`のプラグインを`plugin_dir`へ同期する(版が同じならスキップ)。
pub fn sync_plugins(bundled_dir: &Path, plugin_dir: &Path) -> Vec<PluginStatus> {
    PLUGIN_NAMES
        .iter()
        .map(|name| {
            let bundled = bundled_dir.join(exe_name(name));
            let installed = plugin_dir.join(exe_name(name));
            let version_file = plugin_dir.join(format!("{name}.version"));

            let Ok(bytes) = std::fs::read(&bundled) else {
                let version = std::fs::read_to_string(&version_file).ok().map(|s| s.trim().to_string());
                return PluginStatus { name: name.to_string(), version, action: PluginAction::NotBundled, path: installed.is_file().then(|| installed.to_string_lossy().to_string()) };
            };
            let version = version_of(&bytes);
            let existing_version = std::fs::read_to_string(&version_file).ok().map(|s| s.trim().to_string());

            let action = if installed.is_file() && existing_version.as_deref() == Some(version.as_str()) {
                PluginAction::UpToDate
            } else {
                let existed = installed.is_file();
                let result = std::fs::create_dir_all(plugin_dir).and_then(|_| std::fs::write(&installed, &bytes)).and_then(|_| std::fs::write(&version_file, &version));
                if result.is_err() {
                    return PluginStatus { name: name.to_string(), version: existing_version, action: PluginAction::NotBundled, path: None };
                }
                if existed { PluginAction::Updated } else { PluginAction::Installed }
            };
            PluginStatus { name: name.to_string(), version: Some(version), action, path: Some(installed.to_string_lossy().to_string()) }
        })
        .collect()
}

/// アプリ本体の隣にある同梱プラグインを、ユーザーのプラグインフォルダへ同期する。
pub fn sync_bundled_plugins() -> Vec<PluginStatus> {
    let (Ok(exe), Some(dir)) = (std::env::current_exe(), plugin_dir()) else {
        return Vec::new();
    };
    match exe.parent() {
        Some(bundled_dir) => sync_plugins(bundled_dir, &dir),
        None => Vec::new(),
    }
}

/// インストール済みプラグイン(`rs-*`)の実行ファイルパス。
pub fn installed_plugin_path(name: &str) -> Option<PathBuf> {
    if !PLUGIN_NAMES.contains(&name) {
        return None;
    }
    let path = plugin_dir()?.join(exe_name(name));
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("make-disk-plugins-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn version_changes_with_content() {
        assert_eq!(version_of(b"abc"), version_of(b"abc"));
        assert_ne!(version_of(b"abc"), version_of(b"abd"));
        assert!(version_of(b"abc").starts_with("3-"));
    }

    #[test]
    fn install_then_skip_then_update() {
        let bundled = tmp("bundled");
        let plugins = tmp("plugins").join("nested");
        std::fs::write(bundled.join(exe_name("rs-ffmpeg")), b"v1-binary").unwrap();

        // 1回目: 新規インストール(rs-xorrisoは同梱なし)。
        let first = sync_plugins(&bundled, &plugins);
        assert_eq!(first[0].action, PluginAction::Installed);
        assert_eq!(first[1].action, PluginAction::NotBundled);

        // 2回目: 同じ版なので書き込まない(更新日時が変わらない=上書きの無駄が無い)。
        let installed = plugins.join(exe_name("rs-ffmpeg"));
        let before = std::fs::metadata(&installed).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let second = sync_plugins(&bundled, &plugins);
        assert_eq!(second[0].action, PluginAction::UpToDate);
        assert_eq!(before, std::fs::metadata(&installed).unwrap().modified().unwrap(), "同じ版のプラグインは再コピーされないはず");

        // 3回目: 同梱側が新しい版になったら上書き。
        std::fs::write(bundled.join(exe_name("rs-ffmpeg")), b"v2-binary-newer").unwrap();
        let third = sync_plugins(&bundled, &plugins);
        assert_eq!(third[0].action, PluginAction::Updated);
        assert_eq!(std::fs::read(&installed).unwrap(), b"v2-binary-newer");

        let _ = std::fs::remove_dir_all(&bundled);
        let _ = std::fs::remove_dir_all(plugins.parent().unwrap());
    }

    #[test]
    fn only_known_plugin_names_resolve() {
        assert!(installed_plugin_path("ffmpeg").is_none());
        assert!(installed_plugin_path("not-a-plugin").is_none());
    }
}
