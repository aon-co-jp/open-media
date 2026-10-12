use serde::de::DeserializeOwned;
use serde::Deserialize;
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

const PLUGIN_IDENTIFIER: &str = "jp.co.aon.makedisk.androidfolder";

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<AndroidFolder<R>> {
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "AndroidFolderPlugin")?;
    Ok(AndroidFolder(handle))
}

pub struct AndroidFolder<R: Runtime>(PluginHandle<R>);

#[derive(Debug, Deserialize)]
struct PickTreeResponse {
    uri: Option<String>,
}

impl<R: Runtime> AndroidFolder<R> {
    /// `ACTION_OPEN_DOCUMENT_TREE`でフォルダを選ばせ、選ばれたツリーの
    /// content:// URI文字列を返す(キャンセル時はNone)。
    /// Kotlin側で`takePersistableUriPermission`により永続化しているため、
    /// このURIはアプリ再起動後も有効。
    pub fn pick_output_tree(&self) -> crate::Result<Option<String>> {
        let res: PickTreeResponse = self.0.run_mobile_plugin("pickOutputTree", ())?;
        Ok(res.uri)
    }
}
