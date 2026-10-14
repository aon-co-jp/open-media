# 開発方針＆開発環境ルール(open-media)

**言語 / Languages**: 日本語(このページ・正本) | [English](CLAUDE.en.md)
関連: [`PORTING.md`](PORTING.md)(引っ越し・OS別の状況) / [`README.md`](README.md)
make-disk 時代の詳細な開発記録(エンジンの設計・AI機能・過去の不具合)は [`docs/make-disk-legacy/`](docs/make-disk-legacy/) に残してある。

## このリポジトリの役割

**open-media = [make-disk](https://github.com/aon-co-jp/make-disk) + [open-bar](https://github.com/aon-co-jp/open-bar)**。
CD/DVD/Blu-ray の書き込み・変換(make-disk)と、高音質プレーヤー(open-bar)を1つのアプリにまとめた、
Windows / macOS / Linux / Android 共通コードの GUI アプリ(Rust + Tauri 2)。make-disk は最終版で止め、開発は本リポジトリで続ける。
**基本はWindows版。Ubuntu(WSL)版はテスト用**。

## 構成

- `src/` … 画面(素のJS)。`index.html` / `main.js`(機能) / `ux.js`(タブ・折りたたみ・設定の記憶・ショートカット・ドロップ) / `styles.css` / `fonts/`(同梱の日本語フォント)
- `src-tauri/src/lib.rs` … Tauri の命令(コマンド)の入口。**命令の登録一覧(`generate_handler!`)は2か所**(Android用とデスクトップ用)。新しい命令は**両方**へ足す。
- `src-tauri/src/engine/` … 実装
  - `bar_player.rs`(open-barの再生エンジン) / `bridge.rs`(システム音声の橋渡し、Windows) / `disc_player.rs`(ディスクの項目・再生用の変換)
  - `autoplay.rs`(Windowsの自動再生の登録) / `assoc.rs`(拡張子の関連付け) / `cdda.rs`(音楽CD) / `burn.rs`・`iso.rs`(書き込み)
  - `convert.rs` ほか(変換・AI機能。詳細は legacy 文書)
- `src-tauri/capabilities/` … `default.json`(共通) / `desktop.json`(**更新の権限。デスクトップだけ**)
- `src-tauri/gen/android/` … Androidプロジェクト(パッケージ名は互換のため `jp.co.aon.make_disk` のまま)
- `installer/installer.nsi` … Windowsのインストーラーの雛形(既定の置き場所は `%LOCALAPPDATA%\open-easy-web\open-media`)
- `linux/pipewire/` … Linuxで全アプリの音を高品質に変換する設定(**未検証**)
- `scripts/` … `sync-doc-versions.sh`(版番号の同期検査) / `android-local-build.ps1` / `doctor.ps1`(環境診断) ほか
- `.github/workflows/release.yml` … `v*` タグの push で全OS+APKをビルドしてリリースへ添付

## 運用ルール

1. **正直に書く**。確認していないことを「完了」と書かない。READMEの「現状と既知の制限」を、確認の都度、実態に合わせる。
2. **完成を言う前に実機で確かめる**(CIの成功は動作確認ではない)。画面は撮って目で見る(下の「試験の作法」)。
3. **DRMで保護された音声・映像は加工しない**(保護を外す処理は作らない)。システム音声の橋渡しは、受け取れたものを高品質に変換して別のDACへ出すだけで、受け取れない(無音になる)ものはそのまま。
4. **利用者の設定を勝手に変えない**。自動再生の既定・関連付けの登録は、画面のボタンを押したときだけ。変えるときは以前の値を控え、「解除」で戻せるようにする。管理者権限が要る操作(MSIの削除など)は、許可を得てから。
5. **Windows版が本命、Ubuntu(WSL)はテスト用**。Ubuntuのテストは Windows 版を閉じてから行い、終わったらすぐ止める(`pkill -x open-media`)。同時に起動しない。Windows版は `single-instance` で1つだけ起動する。
6. 秘密(更新の署名鍵・トークン・SSH鍵)は**リポジトリに入れない**。値を画面・コミット・ログに出さない。置き場所は下の「秘密情報」。
7. 版番号は `package.json` / `src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml` を揃え、`README.md` / `README.en.md` の版表記も `bash scripts/sync-doc-versions.sh` で揃える(READMEに別の `x.y.z` 形式の数字を書くと検査が落ちる)。
8. 日本語と英語の両方を併記する(画面・README・CLAUDE・PORTING)。文書の正本は日本語。
9. コミットの末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` を付ける。

## リリース手順

1. 版を上げる(上の7)→ コミット → `git push origin main`
2. `git tag vX.Y.Z && git push origin vX.Y.Z` → GitHub Actions が Windows / macOS(2種) / Linux / Android を作り、`latest.json` と署名(`.sig`)も添付する。
3. 完了は `gh run view <id> -R aon-co-jp/open-media --json status,conclusion,jobs` で**自分で確認する**(通知や `gh run watch` は当てにならないことがあった)。
4. 実機で、インストーラーを入れて起動・再生を確かめる。
- 自動更新: 起動時に `latest.json` を見て、新しい版があれば確認のうえ更新する。署名の秘密鍵はリポジトリのシークレット `TAURI_SIGNING_PRIVATE_KEY`(パスワード無し)。公開鍵は `tauri.conf.json`。

## 試験の作法

- **ビルド**: `cd src-tauri && cargo check`(約2〜4分)。実行ファイルは `npm run tauri build -- --no-bundle`(約17分)。インストーラーは `--bundles nsis`(署名のところで「秘密鍵が無い」と出るが、インストーラー本体はできる)。
- **単体テスト**: `cargo test --lib <名前>`。実機が要るものは `#[ignore]` で、`-- --ignored --nocapture` を付けて走らせる。
  - `real_cd`(音楽CD→排他+アップサンプルで再生、D:に音楽CDが必要) / `app_sequence`(アプリと同じ手順) / `loopback_real`(open-bar側、橋渡し) / `assoc_real`(関連付けの登録→解除) / `autoplay_real`(自動再生の有効化→元に戻す)
- **画面の確認**: ブラウザで見る場合は、`window.__TAURI__` の代役(スタブ)を付けた試験用ページを静的サーバーで配る。実アプリの画面は、PowerShellで撮る。**4K・拡大表示の画面では、撮影前に `SetProcessDPIAware()` を呼ぶ**(呼ばないと左上の一部しか写らない)。
- **更新の試験**: **`--no-bundle` で作った実行ファイルは更新に `.msi` を選んでしまう**(インストーラー版は `nsis` を選ぶ)。更新の試験は `--bundles nsis` のインストーラーを入れて行う。更新を割り込ませたくない試験は、環境変数 `HTTPS_PROXY=http://127.0.0.1:9` を付けて起動する。
- **Android**: 実機(USB)は `scripts/android-local-build.ps1 -Install`。CIが作るAPKとローカルのAPKは署名が違うので、入れ替えには先にアンインストールが要る。
- **Ubuntu(WSL)**: `.deb` を入れて起動。WSLgのウィンドウは `msrdc` が持つ(タイトル `open-media (Ubuntu)`)。

## 見つけた落とし穴(再発させない)

- 更新の権限(`updater:default` と `process:allow-restart`)が無いと、更新確認は黙って拒否される → `capabilities/desktop.json`。
- 命令を足したのに「Command … not found」→ `generate_handler!` の**もう一方の一覧**へ足し忘れ。
- 画面(`src/`)だけを変えても、Android・デスクトップとも**Rustの再ビルド**が要る(画面は埋め込み)。`lib.rs` の更新日時を進める。
- 再生開始直後、エンジンは一瞬「停止」と報告する。そこで終了と判断しない(`barSeenPlaying`)。再生が始まるとメッセージが空になるので、「再生中」を明示的に書き直す。
- **排他モードの間は、DACが使用中**で、別の処理は `0x8889000A` で開けない。テストの前に、アプリを止める。
- Windowsの自動再生は `EventHandlersDefaultSelection` が優先される(PowerDVDなどが自分を書く)。`UserChosenExecuteHandlers` だけでは負ける。控えは `HKCU\Software\open-media\AutoplayBackup*`。
- 日本語のWindowsの `reg query` は、既定値を `(既定)` と出す(英語の `(Default)` ではない)。両方に対応する。
- Ubuntuで日本語が四角(□)になる → 日本語フォントが無い。同梱フォント(`src/fonts/`、Noto Sans JP、SIL OFL)で解消。
- CIでWindowsのpopplerの取得が失敗 → `RUNNER_TEMP` のバックスラッシュと、zipの配置の変更(`cygpath` と `share/poppler` の探索で解消)。
- PowerShellで `gh ... --jq '...'` を引用符つきで渡すと崩れる。bash から呼ぶ。
- Pythonでファイルを書き換えるときは改行(CRLF/LF)を保つ(二重に変換すると `\r\r\n` になる)。

## 秘密情報(値は書かない)

- 更新の署名鍵: `F:\open-media-updater.key` と `.pub`(**別の場所にも保管**。無くすと更新を配れない)。GitHubのシークレット `TAURI_SIGNING_PRIVATE_KEY` に登録済み。
- GitHub: `gh auth` のログイン(組織 `aon-co-jp`)。
- VPS: `ssh conoha`(`~/.ssh/config`)。VPSでGitHubのprivate repoを取るトークンは設定済み。

## 現状と次にやること(2026-10-14)

- 公開: `v0.1.10`(自動更新・1つだけ起動・日本語フォント同梱・関連付け・自動再生)。
- 確認済み: Windows実機(インストール・更新・CD再生・橋渡し)、Android実機(MP4/FLAC)、Ubuntu(WSL、日本語の表示)。
- 未了:
  1. PowerDVDが先に開く件の、CDを入れ直した結果(本体の修正は `v0.1.8` 以降に入っている)
  2. DVD / Blu-rayの実ディスク再生、Blu-rayの自動再生のイベント名
  3. macOS実機、Android TV実機
  4. YouTube等へのループバック(音楽向けのみ)の実際の聴き比べ

## 文書の更新ルール

READMEの「現状」・本書・`PORTING.md` は、機能や確認が変わるたびに、日本語と英語の両方を更新し、コミットして push する(VPSの `/root/repository/open-media` も `git pull` で追従)。
