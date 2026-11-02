//! 「ダブルクリック→数回の選択→完了」のウィザードGUI(native-windows-gui)。
//! コマンド操作(pip/npm等)は一切要求しない。

use std::cell::RefCell;
use std::path::PathBuf;

use native_windows_derive as nwd;
use native_windows_gui as nwg;
use nwd::NwgUi;
use nwg::NativeUi;

use crate::default_install_dir;

#[derive(Default, NwgUi)]
pub struct InstallerApp {
    #[nwg_control(size: (520, 300), position: (300, 300), title: "make-disk installer", flags: "WINDOW|VISIBLE|MINIMIZE_BOX")]
    // ウィンドウタイトルは`build_ui`直後に`format!("make-disk installer (v{})", env!("MAKE_DISK_APP_VERSION"))`へ
    // `run()`側で上書きする(nwg_control属性はリテラルしか書けないため)。
    #[nwg_events( OnWindowClose: [InstallerApp::exit] )]
    window: nwg::Window,

    #[nwg_control(text: "make-disk を1クリックでインストールします。", size: (480, 25), position: (20, 15))]
    title_label: nwg::Label,

    #[nwg_control(text: "インストール先:", size: (100, 25), position: (20, 55))]
    dir_label: nwg::Label,

    #[nwg_control(size: (300, 25), position: (120, 52))]
    dir_edit: nwg::TextInput,

    #[nwg_control(text: "参照...", size: (80, 25), position: (430, 51))]
    #[nwg_events( OnButtonClick: [InstallerApp::browse] )]
    browse_btn: nwg::Button,

    #[nwg_control(text: "open-bar(高音質プレーヤー)の最新版も同時にインストールする", size: (480, 25), position: (20, 95))]
    bundle_check: nwg::CheckBox,

    #[nwg_control(text: "本家xorriso(ISO書き込み)は未同梱です。必要な場合は別途導入してください。", size: (480, 20), position: (20, 125))]
    gap_label: nwg::Label,

    #[nwg_control(text: "", size: (480, 40), position: (20, 160))]
    status_label: nwg::Label,

    #[nwg_control(text: "インストール", size: (110, 32), position: (280, 230))]
    #[nwg_events( OnButtonClick: [InstallerApp::install] )]
    install_btn: nwg::Button,

    #[nwg_control(text: "閉じる", size: (110, 32), position: (400, 230))]
    #[nwg_events( OnButtonClick: [InstallerApp::exit] )]
    close_btn: nwg::Button,

    installed_ok: RefCell<bool>,
}

impl InstallerApp {
    fn browse(&self) {
        let mut dialog = Default::default();
        nwg::FileDialog::builder().title("インストール先を選択").action(nwg::FileDialogAction::OpenDirectory).build(&mut dialog).ok();
        if dialog.run(Some(&self.window)) {
            if let Ok(dir) = dialog.get_selected_item() {
                self.dir_edit.set_text(&dir.to_string_lossy());
            }
        }
    }

    fn install(&self) {
        self.install_btn.set_enabled(false);
        self.browse_btn.set_enabled(false);
        self.bundle_check.set_enabled(false);

        let dir_text = self.dir_edit.text();
        let dir = if dir_text.trim().is_empty() { default_install_dir() } else { PathBuf::from(dir_text) };
        let bundle_open_bar = self.bundle_check.check_state() == nwg::CheckBoxState::Checked;

        self.status_label.set_text("インストール中です。数秒〜数十秒かかることがあります…");

        let result = crate::perform_install(&dir, bundle_open_bar, |msg| {
            self.status_label.set_text(msg);
        });

        match result {
            Ok(()) => {
                *self.installed_ok.borrow_mut() = true;
                self.status_label.set_text(&format!("完了しました: {}", dir.display()));
                nwg::modal_info_message(&self.window, "完了", &format!("make-disk のインストールが完了しました。\n\nインストール先: {}", dir.display()));
                self.install_btn.set_text("再インストール");
            }
            Err(e) => {
                self.status_label.set_text(&format!("エラー: {e}"));
                nwg::modal_error_message(&self.window, "インストールに失敗しました", &format!("{e}"));
            }
        }

        self.install_btn.set_enabled(true);
        self.browse_btn.set_enabled(true);
        self.bundle_check.set_enabled(true);
    }

    fn exit(&self) {
        nwg::stop_thread_dispatch();
    }
}

pub fn run() -> anyhow::Result<()> {
    nwg::init().map_err(|e| anyhow::anyhow!("GUIの初期化に失敗しました: {e}"))?;
    let app = InstallerApp::build_ui(Default::default()).map_err(|e| anyhow::anyhow!("画面の構築に失敗しました: {e}"))?;
    app.window.set_text(&format!("make-disk installer (v{})", env!("MAKE_DISK_APP_VERSION")));
    app.dir_edit.set_text(&default_install_dir().to_string_lossy());
    nwg::dispatch_thread_events();
    Ok(())
}
