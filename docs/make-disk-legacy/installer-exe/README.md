# installer-exe/

`make-disk-installer.exe` の実装(単一自己完結インストーラー、Rust製)。
配布物としてのダウンロード先・使い方は [`../installer/README.md`](../installer/README.md)
を参照。ここは実装側のメモ。

## 何をするか

ダブルクリック → インストール先を選ぶ(既定は
`%LOCALAPPDATA%\Programs\make-disk`) → 「open-barも同時にインストールする」の
チェック → 「インストール」ボタン、の数クリックだけで完了する。
`pip install`/`npm install`のようなコマンド操作は一切要求しない。

同梱(すべて`build.rs`が`payload.zip`としてまとめ、`include_bytes!`で
埋め込む。実行時の追加ダウンロードは無い):

- make-disk本体(`src-tauri/target/release/make-disk.exe`をそのまま)
- 本家ffmpeg/ffprobe(`scripts/fetch-ffmpeg-sidecars.sh`が用意したもの)
- 実験的な純Rust版 rs-ffmpeg/rs-xorriso(`scripts/build-rs-tribute-sidecars.sh`)

実行時に取得するもの(埋め込むと肥大化するため):

- WebView2ランタイム: 既にインストール済みならレジストリ確認だけで済ませ、
  無ければ既存のNSISインストーラー(`../installer/installer.nsi`)と同じ
  固定URLから小さなブートストラッパーだけ取得して`/silent /install`する。
- open-bar: 「同時にインストールする」にチェックが入っている時だけ、
  `aon-co-jp/open-bar`のGitHub Releasesから最新のWindows用インストーラー
  (`*_x64-setup.exe`)を取得し`/S`でサイレントインストールする。

アンインストールは自分自身をインストール先へコピーしておき(元の配布exeが
消されても動くように)、「プログラムと機能」からそのコピーを
`--uninstall --dir <インストール先>`付きで呼ぶ形でレジストリ登録する。

## 既知の欠落(正直な開示)

**本家xorriso(GPL)は未同梱。** Windows向けの単純な静的exeが無く
(Cygwin/MSYS依存が強い)、今回のビルドでは調達できていない。展開先へ
`KNOWN_GAPS.txt`として明記され、完了画面でも案内される。ISO書き込み機能を
使うには利用者が別途xorrisoを導入しPATHへ通す必要がある(`engine/sidecar.rs`
が既存の仕組み通りPATH上のxorrisoへフォールバックする)。

## ビルドする

前提: リポジトリルートで`npm run tauri build`(または最低限
`cargo build --release`をsrc-tauri/で)済みで`src-tauri/target/release/make-disk.exe`が
存在すること。`scripts/fetch-ffmpeg-sidecars.sh`と
`scripts/build-rs-tribute-sidecars.sh`も実行し`src-tauri/binaries/`が
揃っていること(rs-ffmpeg/rs-xorrisoは無くてもビルドは通る、警告のみ)。

```bash
cd installer-exe
cargo build --release
# 出力: target/release/make-disk-installer.exe
```

## 動作検証

GUIを経由しない動作検証用に、非公開のフラグを用意している(通常の配布物には
影響しない、開発時の自動テスト用):

```bash
# 展開・WebView2確認・レジストリ登録までを自動実行(open-barは取得しない)
make-disk-installer.exe --test-install <検証用フォルダ>
# --with-open-bar を付けるとopen-barの取得・サイレントインストールも試す

# アンインストール(インストール先へコピーされた方を呼ぶのが正しい使い方)
<インストール先>\make-disk-installer.exe --uninstall --dir <インストール先>
```

2026-09-26に`--test-install`で実機検証済み: 展開(make-disk.exe/ffmpeg.exe/
ffprobe.exe/rs-ffmpeg.exe/rs-xorriso.exe/KNOWN_GAPS.txt)・レジストリ登録・
アンインストール(レジストリ削除+フォルダ削除)まで確認。

**2026-09-27にGUI実クリックも実機検証済み**(Windows UI Automation
(`System.Windows.Automation`)+`SetCursorPos`/`mouse_event`によるPowerShell
スクリプトで、実際のマウス座標をGUIの各ボタン中心へ動かしクリックする方式。
`InvokePattern`等のUIAパターンはこのGUI(native-windows-gui)のコントロールが
一切公開していない〈`GetSupportedPatterns()`が全コントロールで空〉ため、
座標クリックのみが有効だった)。実際に確認できたこと:
- 「参照...」ボタン・チェックボックス(open-bar同梱)・「インストール」
  ボタンの実クリックが、実際にGUIの状態(コントロールの無効化・ステータス
  文言の変化)を変えることを画面キャプチャで確認。
- 「インストール」を実クリックすると実際に`perform_install`が走り、
  既定のインストール先(`%LOCALAPPDATA%\Programs\make-disk`)へ
  make-disk.exe/ffmpeg.exe/ffprobe.exe/rs-ffmpeg.exe/rs-xorriso.exe/
  KNOWN_GAPS.txtが実際に展開されることをファイルシステムで確認。
  完了後は「完了」モーダルダイアログ(`nwg::modal_info_message`)が実際に
  表示され、実クリックでOKを押して閉じられることを確認。ボタンの表示が
  「インストール」→「再インストール」に変わる仕様通りの挙動も確認。
- レジストリ(`HKCU\...\Uninstall\make-disk-installer`)の
  DisplayName/DisplayVersion/Publisher/InstallLocation/UninstallString/
  NoModify/NoRepairが全て正しい値で登録されることを確認。
- 登録された`UninstallString`をそのまま実行(`--uninstall --dir <先>`)し、
  実際にインストール先フォルダとレジストリキーの両方が削除される
  (アンインストール)ことも実機で確認。
- **見つかった環境上の注意点(次回のため)**: (1) 自動操作するプロセス側で
  `SetProcessDPIAware()`を呼ばないと、UI Automationが返す座標(物理ピクセル)
  と`SetCursorPos`が期待する座標系(非DPI対応プロセスからは仮想化される)が
  ずれ、クリックが全く違う場所に飛んで何も起きない(この不整合の解消に
  最も時間がかかった)。(2) 完了モーダル(`nwg::modal_info_message`が出す
  `MessageBox`)は`AutomationElement.RootElement`の`TreeScope.Children`
  列挙には出てこないことがあり(所有ウィンドウ扱いのためか)、代わりに
  Win32の`EnumWindows`で確実にhwndを見つけてから`AutomationElement.FromHandle`
  で扱うと確実だった。(3) UTF-8(BOM無し)で保存したPowerShellスクリプトは、
  Windows PowerShell 5.1がシステムのコードページで誤って読み込み、
  日本語の文字列リテラルが壊れて構文エラーになることがある(UTF-8のBOMを
  先頭に付けると解決)。
- **open-bar同時インストールの経路も2026-09-27に実機検証済み**(ユーザーの明示的な指示により、
  実機へ本当にインストールする副作用を承知の上で実行)。
  `make-disk-installer.exe --test-install <検証用フォルダ> --with-open-bar`で
  `fetch_and_install_open_bar()`を実行し、`aon-co-jp/open-bar`のGitHub Releasesから
  最新のWindows用インストーラー(`*_x64-setup.exe`)を実際にダウンロードし、`/S`で
  サイレントインストールが最後まで完了する(標準出力の5段階全てがエラー無く完了)ことを
  確認した。`%LOCALAPPDATA%\open-bar\uninstall.exe`が新しいタイムスタンプで再生成されて
  いることから、ダウンロードしたNSISインストーラーが実際に実行されたことも裏付けられた
  (この開発機には既にopen-barが入っていたため「新規インストール」そのものではなく
  「再インストール/上書き」の確認になった点には留意。まっさらな環境での初回インストールは
  未検証)。検証用の`--test-install`出力フォルダとダウンロードした一時exeは後片付け済み。
