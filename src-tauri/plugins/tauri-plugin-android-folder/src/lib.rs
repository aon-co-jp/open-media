//! Android SAF(`ACTION_OPEN_DOCUMENT_TREE`)によるフォルダ選択プラグイン。
//!
//! Tauri標準の`tauri-plugin-dialog`はモバイルでのフォルダ選択を
//! 意図的に未実装としている(`Cargo.toml`のplatform supportノートに
//! "Does not support folder picker"と明記されている)。make-diskは
//! 「変換結果をまとめて1フォルダに出力する」UXをモバイルでも維持したい
//! ため、Android向けにこの小さな専用プラグインを追加した。

use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

mod commands;
mod error;
mod mobile;

pub use error::{Error, Result};
pub use mobile::AndroidFolder;

pub trait AndroidFolderExt<R: Runtime> {
    fn android_folder(&self) -> &AndroidFolder<R>;
}

impl<R: Runtime, T: Manager<R>> AndroidFolderExt<R> for T {
    fn android_folder(&self) -> &AndroidFolder<R> {
        self.state::<AndroidFolder<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("android-folder")
        .setup(|app, api| {
            let handle = mobile::init(app, api)?;
            app.manage(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::pick_output_tree])
        .build()
}
