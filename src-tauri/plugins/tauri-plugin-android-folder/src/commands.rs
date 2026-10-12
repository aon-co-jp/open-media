use crate::AndroidFolderExt;
use tauri::{command, AppHandle, Runtime};

#[command]
pub(crate) async fn pick_output_tree<R: Runtime>(app: AppHandle<R>) -> crate::Result<Option<String>> {
    app.android_folder().pick_output_tree()
}
