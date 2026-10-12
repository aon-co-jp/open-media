const COMMANDS: &[&str] = &["pick_output_tree"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .try_build()
        .expect("android-folder plugin build failed");
}
