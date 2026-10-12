# 開発方針＆開発環境ルール(make-disk)

**言語 / Languages**: 日本語(このページ・正本) | [English](CLAUDE/CLAUDE.en.md) | [简体中文](CLAUDE/CLAUDE.zh-CN.md) | [繁體中文(台灣)](CLAUDE/CLAUDE.zh-TW.md) | [한국어](CLAUDE/CLAUDE.ko.md) | [Deutsch](CLAUDE/CLAUDE.de.md) | [Français](CLAUDE/CLAUDE.fr.md) | [Русский](CLAUDE/CLAUDE.ru.md) | [Українська](CLAUDE/CLAUDE.uk.md) | [فارسی](CLAUDE/CLAUDE.iran%28Perusha%29.md) | [العربية](CLAUDE/CLAUDE.ar.md)
(多言語版は要約です。全文・履歴は日本語版が正本 / The translations are summaries; this Japanese file is the full, authoritative version.)

全リポジトリ共通の開発ルール(自動継続・検証徹底等)は
[`open-raid-z`](https://github.com/aon-co-jp/open-raid-z)の`CLAUDE.md`を
正本として参照すること。この節では本リポジトリ固有の事項のみ記す。

## リポジトリの役割

CD/DVD/Blu-ray書き込み・音声/動画フォーマット変換・ISO出力を行う
Windows/macOS/Linux共通コードのGUIアプリ(Rust + Tauri)。
プラットフォームごとの差分はインストーラー(bundle)のみに閉じ込め、
アプリ本体(`src-tauri/src`・`src`)は単一コードベースとする。

## アーキテクチャ

- フロントエンド: `src/`(バニラJS、Tauri IPC経由でRustコマンドを呼ぶ)
- バックエンド: `src-tauri/src/engine/`
  - `probe.rs` — ffprobeでメディア尺・コーデック取得
  - `convert.rs` — ffmpegでフォーマット変換・ビットレート制御・トリミング・
    複数区間カット(後述)
  - `capacity.rs` — CD/DVD/Blu-ray容量からの自動最大ビットレート算出、
    および基準ビットレートに対する低下度合いを4段階(下がります→
    少し下がります→かなり下がります→画質/音質が落ちます)で警告する
    `quality_warning`(下限は設けず、常に容量に収まる値を返す仕様)
  - `cpu.rs` — `open-cpu`によるCPU命令セット検出。CPUフォールバック時の
    速度目安表示と、`-preset`自動選択の両方に使う(後述)
  - `iso.rs` — xorriso(`-as mkisofs`)でISOイメージ生成
  - `burn.rs` — xorriso(`-as cdrecord`)でディスク書き込み・速度指定・デバイス列挙

xorrisoはlibburn/libisofs/cdrtools(cdrecord)相当の機能をOS非依存の
単一コマンド体系で提供するため、CD/DVD/Blu-rayの書き込み経路を
xorrisoに統一している(OSごとに別ライブラリを直接バインディングしない)。

## 外部依存(実行時に必要、同梱はしない)

- `ffmpeg` / `ffprobe` — フォーマット変換・ビットレート制御
- `xorriso` — ISO生成・ディスク書き込み(cdrtools/cdrdao/libburn/libisofs相当)

インストーラー側の課題として、これらのバイナリをOSごとにどう同梱/
案内するか(Windows: 同梱バイナリ配布、macOS: Homebrew案内、
Linux: パッケージマネージャー案内、等)は未確定・要検討。

## 複数区間カット・フレーム精度モード・open-cpu統合(2026-09-12)

- `CutRange`(`convert.rs`): 1本の動画に対して「最初のA〜Bをカット」
  「途中のC〜Dをカット」「最後のE〜末尾をカット」を同時にいくつでも
  指定できる。残す区間は`-c copy`(無劣化・高速)で抽出し、フォーマット
  変換が必要な場合のみ結合(concat demuxer)時に1回だけエンコードする。
- `frame_accurate`フラグ: カット境界をフレーム単位で正確に切りたい場合、
  GPUハードウェアエンコーダ(NVENC→QuickSync→AMFの順)を実際に1フレーム
  試しエンコードして本当に動くか検証してから採用する(`ffmpeg -encoders`
  の一覧に載っているかだけでは不十分——古いNVIDIAドライバでは
  `h264_nvenc`がリストには出るのに実行すると失敗する実バグを統合テストで
  発見・修正済み)。GPUが無ければCPU(`libx264`)にフォールバックする。
- `open-cpu`(エコシステム共通のCPU命令セット検出ライブラリ)をCargo
  依存として統合。CPUフォールバック時に:
  1. UIのログ欄に速度目安(fast/moderate/slow/very_slow、AVX2/AVX-512
     BW/VL/FMA等の検出状況に基づく4段階)を表示するだけでなく、
  2. `recommended_x264_preset()`で実際のffmpeg`-preset`引数を自動選択する
     (非力なCPUには`ultrafast`/`veryfast`、強力なCPUには`slow`など、
     速度目安を実際のコマンドライン引数へ反映する——表示のみでなく
     実際の分岐・制御に使っている)。
  3. `main.js`側では、フレーム精度モードのチェックボックスをONにした際、
     速度目安がslow/very_slowなら`window.confirm()`で続行確認を挟む。
- ffmpeg自身(libx264)がAVX2/AVX-512を使うかどうかは実行時にx264が
  自動判定するものであり、open-cpuの検出結果でその判定自体を上書き
  することはできない(そうする必要も無い)。open-cpuの役割は
  あくまで「-presetの選択」と「事前警告」という、実際にffmpegの
  コマンドライン引数・UIの分岐に反映される制御である。

## プラットフォーム範囲(2026-09-12更新)

- **現在**: Windows/macOS/Linuxのデスクトップ3プラットフォームが対象。
- **Android**: `npm run tauri android init`でプロジェクト生成済み
  (`src-tauri/gen/android`)、Rustのandroidターゲット(aarch64/armv7/
  i686/x86_64)へのクロスコンパイルも成功を確認済み。この開発機
  (Windows)ではWindows開発者モードが有効化できず(2026-09-12に複数回
  試行・再起動後も`HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\
  AppModelUnlock\AllowDevelopmentWithoutDevLicense`が作成されないことを
  確認)、`tauri android build`のjniLibsシンボリックリンク作成が失敗する。
  回避策として、Rustクロスコンパイル成功後の`.so`を手動で
  `gen/android/app/src/main/jniLibs/arm64-v8a/`へコピーし、
  `gradlew assembleUniversalDebug -PabiList=arm64-v8a
  -PtargetList=aarch64 -x rustBuildArm64Debug`で直接パッケージングする
  ことでAPK生成・実機インストール・起動まで到達した(実機:
  OnePlus A401OP, Android 15)。
  UIは`styles.css`にタッチ操作向けの調整(44px以上のタップ領域、
  スマホ幅でのレイアウト崩し、iOS自動ズーム防止のinput最小フォント
  16px)を追加済み。
- **iPhone/iOS**: ユーザーは現時点でテスト実機を保有しておらず、
  対応は将来課題として保留。Tauriはmobile_entry_pointを通じてiOSにも
  対応するため、コード側の障壁は大きくない見込みだが未検証。
- **ディスク書き込み機能のモバイル版での扱い**: Android/iOSはOSレベルで
  内蔵光学ドライブへの直接アクセス手段を持たないため、モバイル版では
  当面「音声/動画変換・ISOイメージ生成」までを提供し、実ディスクへの
  書き込みはPC版限定の機能として明記する。将来的にはUSB接続の外付け
  光学ドライブ(Android USB OTG経由等)への対応を検討課題とする
  (現時点では未着手・要実機検証)。

## Android実機検証で発見した問題と対応方針(2026-09-12)

### 発見1: main.jsのベアインポートが全プラットフォームで動かないバグ(修正済み)
`import { invoke } from "@tauri-apps/api/core"`のようなベア指定子は
バンドラー無しのWebViewでは解決できず、モジュール読み込みが例外で
止まり**main.js内の全イベントリスナーが登録されない**(=全ボタンが
無反応になる)という重大バグがあった。`window.__TAURI__.core.invoke`
経由に変更して修正済み(実機で「ファイルを追加」ボタンがネイティブ
ピッカーを開くことまで確認)。デスクトップ版でもこのバグは(検証は
していなかったが)理論上同じ影響を受けていたはずで、静的ファイル
プレビューだけでは検出できなかった教訓が大きい。

### 発見2: Tauri dialogプラグインはモバイルでフォルダ選択が未実装
実機で`open({directory:true})`を呼ぶと
`"Folder picker is not implemented on mobile"`で例外になることを
確認。ユーザー指示により「まとめて1フォルダに出力」の仕様を
モバイルでも維持する方針とし、Android向けにはSAF
(`ACTION_OPEN_DOCUMENT_TREE`)を直接扱う**独自Tauriプラグインの
新規実装が必要**という結論に至った(標準dialogプラグインの範囲では
実現不可)。

**実装計画(次回セッション向け、未着手)**:
1. `src-tauri/`配下に小さなカスタムTauriプラグインを追加
   (例: `tauri-plugin-android-folder`、Kotlin側で
   `ACTION_OPEN_DOCUMENT_TREE`のIntentを発行しActivity Resultを
   受け取り、`takePersistableUriPermission`で永続化)。
2. Rust側に`pick_output_tree() -> Result<String, String>`のような
   コマンドを追加し、返ったtree URI文字列をJS側で保持。
3. JS側(`main.js`)でAndroid判定時にこのコマンドを呼ぶよう分岐。

### 発見3(重要・優先度確定): ffmpeg/xorrisoはAndroidに存在しないため、
### SAF実装だけでは変換機能は動かない
make-diskは`std::process::Command::new("ffmpeg"/"xorriso")`で外部
バイナリをシェルアウトする設計。Android端末にはこれらのバイナリが
存在せず、同梱もしていないため、**SAFフォルダ選択を実装しても
「実行」ボタンを押した時点で確実に失敗する**(コマンドが見つからない
エラー)。

**方針(ユーザー承認済み、2026-09-12)**: Android対応のffmpeg/xorriso戦略は
以下の優先順で検討する。
1. **完了(2026-09-12)**: SAFフォルダ選択のUI・権限取得部分を実装した。
   `src-tauri/plugins/tauri-plugin-android-folder/`に自前Tauriプラグインを
   新規作成(Rust側`pick_output_tree`コマンド、Kotlin側
   `ACTION_OPEN_DOCUMENT_TREE`+`takePersistableUriPermission`)。
   実機(OPPO Reno11 A, Android)で「フォルダを選択」→ネイティブの
   フォルダツリーピッカー→アクセス許可ダイアログ→
   `content://com.android.externalstorage.documents/tree/primary%3ADocuments`
   というURIが実際に「出力先フォルダ」欄に表示されるところまで
   エンドツーエンドで確認済み。実装は`tauri-plugin-dialog`
   (v2.7.3)自身のAndroidソース(`DialogPlugin.kt`の
   `saveFileDialog`/`ACTION_CREATE_DOCUMENT`パターン)を正確なAPI
   リファレンスとして参照した。`Cargo.toml`では
   `[target.'cfg(target_os = "android")'.dependencies]`でAndroid限定の
   依存とし、デスクトップ版のビルド・テスト(11件)には影響しないことを
   確認済み。
   なお、返るのはcontent:// URIであり実ファイルシステムパスではないため、
   実際のffmpeg/xorriso連携(後述の2)は別途URI→実処理の橋渡しが必要。
2. [`rs-FFmpeg`](https://github.com/aon-co-jp/rs-FFmpeg)・
   [`rs-xorriso`](https://github.com/aon-co-jp/rs-xorriso)を
   Android向けにクロスコンパイルし、`jniLibs`同梱の実行可能ファイル
   またはRustライブラリとして直接リンクする方向を検討する
   (外部プロセスのシェルアウトではなくRust関数呼び出しに変更できれば
   Android/iOS双方で動く可能性が高い——ただし両リポジトリとも
   現時点でmake-diskの実引数と非互換な部分が残っている点に注意)。
3. 本家ffmpeg/xorrisoをAndroidバイナリとして同梱する道は、
   ライセンス・バイナリサイズ・クロスコンパイルの複雑さの観点から
   優先度を下げる。

## 既知の未実装・要検証事項(2026-09-12時点)

- 実機での書き込み検証(CD/DVD/Blu-rayいずれも)は未実施。
  この開発機に光学ドライブが無いため、`xorriso -devices`の実際の
  出力形式やドライブ列挙の挙動は未検証。
- Windows/macOS/Linux各インストーラー(bundle target)はGitHub Actions
  (`v*`タグpushで発火)で全プラットフォームのビルドに成功済み
  (v0.1.0〜v0.1.2のリリースで確認)。ただし実機での動作確認は
  Windows版のみ実施済みで、macOS/Linux版は未実施。
- 5分/10分等の固定時間トリミングは、複数区間カット機能(`CutRange`)の
  「終了」欄に秒数指定で対応可能になったが、プリセットボタン
  (5分/10分ワンクリック)や波形プレビューは未実装。
- 動画プレビューエディタ(`<video>` + `convertFileSrc`)の実機での
  表示・シーク動作は未検証(ビルド成功とロジックレビューのみ)。

## HANDOFF追記(2026-09-14) Android CI追加・installerフォルダ新設・紹介ページ更新・v0.1.4リリース / Follow-up: added Android CI, new installer/ folder, updated the landing page, released v0.1.4

ユーザー指示「installerフォルダーを作ってWindows/Mac/Linux/Android
スマホ・タブレット用などのインストーラー付きアプリをリリース公開して」
への対応。

**確認した既存状態**: Windows/macOS/Linuxのデスクトップ3プラットフォーム
は既にv0.1.0〜v0.1.3で`.github/workflows/release.yml`(`v*`タグpushで
発火、`tauri-apps/tauri-action`)経由でGitHub Releaseへ公開済みだった
(`gh release view v0.1.3`で`.msi`/`.exe`/`.dmg`×2/`.deb`/`.rpm`/
`.AppImage`の全アセットを実際に確認)。Androidのみリリースパイプライン
未配線だった(この開発機ではWindows Developer Mode有効化の壁で
`tauri android build`が直接動かせず、手動`.so`コピー+`gradlew`直叩きの
回避策でローカル実機確認〈OnePlus A401OP〉のみ済んでいた)。

**今回追加した3点**:
1. `.github/workflows/release.yml`へ`release-android`ジョブを追加
   (`ubuntu-latest`、`android-actions/setup-android@v3`でSDK、
   `sdkmanager`でNDK 27.0.12077973を明示インストール、Rust側は
   aarch64/armv7/i686/x86_64のAndroidターゲットをクロスコンパイル、
   `npm run tauri android build -- --apk ... --debug`でuniversal APK
   〈スマホ・タブレット共通、署名鍵未設定のためサイドロード専用〉を
   ビルドし`gh release upload`で同じReleaseへ添付)。この開発機の
   Developer Mode制約はGitHub Actionsのubuntu-latestランナーには
   無いため、公式コマンドをそのまま使える設計にした。
   **正直な開示**: このCIジョブ自体はこのセッションでは実際に
   グリーンになるまで検証していない(NDKバージョン27.0.12077973は
   Tauri 2の一般的な要求バージョンからの推測であり、実際にCIを
   走らせてみないと確定できない——次回セッションで`gh run watch`
   により実際の成否を確認し、失敗する場合はログを見てNDK
   バージョン等を調整すること)。
2. `installer/`フォルダを新設(`installer/README.md`)。バイナリ自体は
   コミットせず(リポジトリ肥大化を避ける)、プラットフォームごとの
   ファイル形式・ビルド元・正直な制限(未署名APK・実機書き込み未検証・
   iOS保留)をまとめた対応表を置いた。
3. `webpage/index.html`(VPS `easy-web.tokyo/make-disk/`で公開中の
   紹介ページ、`make-disk-web.service`が`/root/repository/make-disk/
   webpage/`を`python3 -m http.server 8108`で配信、nginx経由で公開)の
   「現在は初期開発段階のため近日公開予定」という**既に古くなっていた
   文言**を、実際に公開済みの全プラットフォーム一覧+プレリリース扱い
   である旨の正直な開示へ更新。Androidバッジも追加。VPS側は
   `/root/repository/make-disk`で`git pull`するだけで反映される
   (ビルド不要、静的ファイルをそのまま配信する設計のため)。

`package.json`/`src-tauri/Cargo.toml`/`src-tauri/tauri.conf.json`の
バージョンを0.1.3→0.1.4へ統一し、`v0.1.4`タグをpushしてリリースを
発火させた。

## HANDOFF追記(2026-09-14続き) 次の開発増分: ffmpeg/xorrisoのsidecarバイナリ同梱化(ユーザー指示、設計を記録) / Next increment: bundle ffmpeg/xorriso as Tauri sidecar binaries (user request, design recorded)

ユーザーから「必要性のある全てのリポジトリを同梱してのインストーラー
付きアプリとして完成させて」との指示を受けた。現状、実行には`ffmpeg`/
`xorriso`が別途インストール済みでPATHが通っている必要があり(README/
webpage双方で明記)、これをインストーラーへ同梱する(ユーザーが別途
インストールしなくて済む)ことが求められている。**拙速な実装で壊すより、
今回スコープ済みの増分〈Android CI・installerフォルダ・紹介ページ更新・
リリース〉を確実に完成させることを優先し、この機能は正式な次の増分
として設計だけ記録した**(実装は次回セッション)。

**設計方針**:
- Tauriの[`bundle.externalBin`](https://tauri.app/develop/sidecar/)
  機構(通称sidecar)を使い、`ffmpeg`/`ffprobe`/`xorriso`の実行可能
  ファイルをプラットフォームごとにビルド成果物へ同梱する。
  `src-tauri/tauri.conf.json`の`bundle.externalBin`にバイナリの
  ベースパスを列挙し、実行時は`Command::sidecar("ffmpeg")`のように
  呼び出す(現状は`std::process::Command`で`PATH`上の`ffmpeg`を
  直接呼んでいる`convert.rs`/`iso.rs`/`burn.rs`の呼び出し方を変更する
  必要がある)。
- **バイナリの入手元**: 本家ffmpeg/xorrisoの静的ビルド済みバイナリ
  (Windows: 公式ffmpeg.orgの静的ビルド配布・xorriso公式は無いため
  MSYS2/Chocolatey等の配布物を調査要。macOS/Linux: Homebrew/apt等の
  パッケージではなく、CI内で静的リンクビルドするか、事前ビルド済み
  バイナリをダウンロードして`externalBin`用ディレクトリへ配置する
  ビルドステップをrelease.ymlへ追加する形になる見込み)。
  `rs-FFmpeg`/`rs-xorriso`(このリポジトリの姉妹Rustリスペクト版)は
  README/webpageの既存の正直な開示の通り**現時点で本家の一部機能しか
  対応しておらず**(rs-xorrisoはISO生成のみ・長いファイル名切り詰め・
  書き込み機能無し、rs-ffmpegは無圧縮WAVのprobe/変換のみ)、これらを
  そのままsidecarとして同梱してもmake-disk本体の実要件(任意フォーマット
  変換・実ディスク書き込み)を満たせない——**本家バイナリの同梱が
  本命、rs-FFmpeg/rs-xorrisoは将来両者が本家相当の機能を持った時点での
  代替候補**という位置づけを維持する。
- **ライセンス上の注意**: FFmpeg・xorrisoともGPL系ライセンス
  (xorrisoはGPLv2/v3、FFmpegはビルド構成によりLGPL/GPL)。バイナリを
  同梱配布する場合、ライセンス全文の同梱・ソース入手先の明記が
  必要(webpage/README/インストーラー内のライセンス表示への追記が
  要る)。
- **Android/iOSでの扱い**: モバイル版は光学ドライブアクセス自体が
  無いため書き込み機能はそもそも対象外(既存方針通り)。フォーマット
  変換機能のみをモバイルでも提供するなら、ffmpegのAndroid/iOS向け
  クロスコンパイル(NDK/Xcodeツールチェーン)が別途必要——これも
  本家バイナリの静的クロスコンパイルが前提になるため、まずデスクトップ
  3プラットフォームでの同梱を先に完成させ、モバイルは次々回以降の
  増分とするのが妥当と判断する。

**次回セッションでの着手順序案**: (1) デスクトップ3プラットフォーム
向けの本家ffmpeg/xorriso静的バイナリの入手元を確定させる調査、
(2) `tauri.conf.json`の`externalBin`配線+`convert.rs`/`iso.rs`/
`burn.rs`の呼び出し変更、(3) release.ymlへバイナリダウンロード
ステップを追加、(4) ライセンス表示の追加、(5) 実機での動作確認
(sidecar経由でも既存の統合テストが通ることを確認)。

## HANDOFF追記(2026-09-14) ffmpeg/ffprobeのsidecar同梱化(Windows/Linux)実装完了、v0.1.6リリース / Follow-up: implemented ffmpeg/ffprobe sidecar bundling for Windows/Linux, released v0.1.6

前回のHANDOFFで設計だけ記録していた同梱化(ユーザー指示への対応)を、
このセッションで実装・実機検証・リリースまで完了させた。

**実装したもの**:
- `engine/sidecar.rs`新設。`resolve_tool(name)`が、実行ファイルと
  同じディレクトリにある同梱バイナリを探し、見つかればそれを、
  無ければ従来通りPATH上の`name`を使う`Command`を返す。
  `Command::new("ffmpeg")`だった8箇所(`probe.rs`・`convert.rs`
  〈非テスト部分の2箇所〉・`iso.rs`・`burn.rs`)を全て置き換えた。
- **Tauri公式の`tauri_plugin_shell::ShellExt::sidecar`(AppHandle経由の
  非同期API)は採用しなかった**——採用すると`engine/*.rs`の同期関数
  全てを非同期化し`AppHandle`を全呼び出し経路(`lib.rs`の
  `#[tauri::command]`群含む)へ配線する必要があり、影響範囲が広い
  大規模な変更になる。かわりに、Tauriのsidecar機構が実際にバイナリを
  配置する場所を`std::env::current_exe()`から自前で解決する軽量な
  実装にした——既存の同期設計・既存テストを一切壊さずに済む
  トレードオフとして採用。
- `tauri.windows.conf.json`・`tauri.linux.conf.json`(新設、Tauriの
  プラットフォーム別config上書きの仕組み)に`bundle.externalBin`を
  設定。macOSは対象外(理由は後述)。
- `scripts/fetch-ffmpeg-sidecars.sh`新設。
  [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds)の
  静的ビルドをダウンロードし、`src-tauri/binaries/`へ`externalBin`の
  命名規則(`<name>-<target-triple>[.exe]`)で配置する。
  `release.yml`のWindows/Linuxビルドジョブがビルド前に自動実行する。
- `src-tauri/binaries/`はgit管理外(`.gitignore`、バイナリでリポジトリを
  肥大化させないため)、`README.md`だけをコミットして対応表・入手方法・
  正直な制限を記載。

**実機検証で発見・修正した実装ミス(重要な教訓)**: 当初、sidecar
バイナリは`externalBin`のソース側と同じ`<name>-<target-triple>[.exe]`
という名前のままインストール後も配置されると想定して`resolve_tool`を
実装していた。実際に`npm run tauri build`でMSI/NSISインストーラーを
ビルドし(このセッションで実際にビルド成功、`.msi`が約2.8MB→約130MB、
`.exe`が約1.9MB→約95MBへ増加=バイナリが実際に同梱されたことを確認)、
NSISインストーラーを`/S /D=<dir>`でサイレントインストールして
インストール先の中身を実際に確認したところ、**Tauriのバンドラーは
ターゲットトリプル部分を落として`ffmpeg.exe`/`ffprobe.exe`という
bareな名前で配置する**ことが判明した(ビルド対象は常に単一ターゲット
なので、サフィックスを付ける理由がそもそも無い)。当初の実装のままだと
同梱バイナリを一切見つけられず、常にPATHへフォールバックするだけの
無意味な変更になっていた——`resolve_tool`をbareな名前で探す実装へ
修正し、修正後に改めて全テスト(15本、うち新規4本)が通ることを確認。
**この発見は「ビルドが通る」「テストが通る(自分で作ったモックが
自分の想定と一致するだけ)」だけでは不十分で、実際にインストーラーを
作りインストールして中身を見るという実機検証まで行って初めて防げた
バグだった**——このプロジェクト・このエコシステム全体の検証方針
(フェイクな成功にしない)が実際に効いた具体例として記録する。

**新規テスト4本**(`engine::sidecar`3本、`engine::probe`1本)、
うち`probe_actually_executes_a_real_sidecar_binary_when_one_is_bundled`
は実際にダウンロードしたffmpeg/ffprobeバイナリをテスト実行ファイルの
隣へ実際に配置し、`probe()`がそれを検出・実行して正しい動画長を返す
ことを検証する実機E2Eテスト(`src-tauri/binaries/`が無い環境では
スキップ)。クレート全体15本成功、clippy警告は既存の無関係な1件のみ。

**正直な開示(誇張しない、今回のスコープ外)**:
- **macOSは未対応**——BtbN/FFmpeg-BuildsはmacOSビルドを配布しておらず、
  Intel/Apple Silicon両対応の信頼できる単一の入手元を今回確定できな
  かった。
- **xorrisoは未対応**——本家に信頼できる静的クロスプラットフォーム
  ビルド配布が見当たらなかった。GPLライセンス表示整備も別途必要。
- **モバイル(Android/iOS)は未対応**——ffmpegのNDK/iOS向けクロス
  コンパイルが別途必要、かつモバイル版はそもそもディスク書き込み
  機能の対象外(既存方針通り)。
- いずれも`resolve_tool`の仕組み自体は汎用的なので、静的バイナリの
  入手元さえ確定すればコード変更なしで追加できる設計にしてある。

v0.1.6としてタグpush、CI(`release.yml`)で全プラットフォーム
ビルド・GitHub Release公開・VPS紹介ページへの反映まで実施予定
(このHANDOFF記載時点でCI実行前——次回このメッセージを読む人は
実際の成否を`gh run list`で確認すること)。

## HANDOFF追記(2026-09-16) 自動アップデート確認・rs-FFmpeg/rs-xorriso追加同梱・最高音質モード・変換の並列化、v0.1.7リリース / Follow-up: startup update-check, rs-FFmpeg/rs-xorriso bundling, max-quality mode, parallel conversion, v0.1.7

ユーザーからの複数の指示に対応した、今回のセッションでの主な変更点。

### 1. 自動アップデート確認機能

アプリ起動時に一度だけGitHub Releasesの最新版を確認し、新しいバージョンが
あれば「新しいバージョン`<version>`があります。バージョンアップします
か？」(日英併記)のダイアログを出し、選択実行できる機能を追加した
(ユーザー指示通りの文言)。

- `tauri-plugin-updater`・`tauri-plugin-process`をデスクトップのみ
  (`#[cfg(not(any(target_os = "android", target_os = "ios")))]`)で
  追加。モバイルは対象外(ストア/APKサイドロードでの更新が前提)。
- 署名鍵ペアを`npx tauri signer generate`で生成
  (`src-tauri/updater.key`はgit管理外、`src-tauri/updater.key.pub`は
  `tauri.conf.json`の`plugins.updater.pubkey`へ埋め込み済み)。
  秘密鍵はGitHub Secrets(`TAURI_SIGNING_PRIVATE_KEY`)へ登録済み。
- `tauri.conf.json`に`plugins.updater.endpoints`
  (`https://github.com/aon-co-jp/make-disk/releases/latest/download/latest.json`)
  と`bundle.createUpdaterArtifacts: true`を追加。
- **正直な開示・重要な設計変更**: GitHubの`/releases/latest/download/...`
  エイリアスは**prerelease扱いのリリースを除外する**仕様のため、
  `release.yml`の`prerelease`を`true`→**`false`**へ変更した——これに
  より`v0.1.7`以降のリリースはGitHub上で通常のリリース(pre-release
  ラベル無し)として表示されるようになる。アプリ自体の完成度が
  変わったわけではなく、あくまでアップデーター機構を正しく動かす
  ための技術的な必要変更である点に注意。
- `main.js`に`checkForUpdatesOnStartup()`を追加、起動時に自動実行。
- **実機検証で発見した2つの罠**: (1) `plugins.updater`を設定しただけ
  では`.sig`署名アーティファクトは生成されない——`bundle.
  createUpdaterArtifacts: true`を明示的に設定する必要があった
  (最初のローカルビルドでは`.sig`が生成されずハマった)。(2) ローカル
  ビルドでは`TAURI_SIGNING_PRIVATE_KEY`環境変数が無いと
  `failed to build app`という手掛かりの薄いエラーで失敗する
  (署名ステップの失敗が汎用エラーメッセージに丸められる)——
  実際に`export TAURI_SIGNING_PRIVATE_KEY=$(cat src-tauri/updater.key)`
  してから再ビルドし、`.sig`ファイル生成まで実機確認して解決した。

### 2. rs-FFmpeg/rs-xorrisoの追加同梱

ユーザー指示「作成したRust版(rs-FFmpeg・rs-xorriso、共にMITライセンス・
新規作成は不要で既存)も同梱して」への対応。`scripts/
build-rs-tribute-sidecars.sh`新設(隣にcloneされた`rs-FFmpeg`/
`rs-xorriso`をソースからビルドし`src-tauri/binaries/`へ配置)、
`tauri.windows.conf.json`/`tauri.linux.conf.json`の`externalBin`へ
`binaries/rs-ffmpeg`・`binaries/rs-xorriso`を追加。実際に
`npm run tauri build`でインストーラーへ同梱され、`target/release/`に
`rs-ffmpeg.exe`/`rs-xorriso.exe`(bareな名前、既存の`sidecar.rs`の
命名規則通り)として配置されることを実機確認済み。`release.yml`にも
rs-FFmpeg/rs-xorrisoをcheckout+ビルドするステップを追加。

**正直な開示**: これらのバイナリは同梱されるが、アプリ本体のロジックは
これらを自動選択・呼び出さない(既存のCLAUDE.md「早期WIPで本家の
完全な代替にはならない」という開示は変わらず——あくまで「試したい人が
使える実験的な追加バイナリ」の位置づけ)。

### 3. 「最高音質・最高画質で記録する」モード

ユーザー指示「音声や画像フォーマットを選択する代わりに最高音質＆最高
画質で記録するを選択可能として」「未選択時はMP4などの動画をCDのISOに
変換する場合は、自動でロスレスWAVでかつISO同時変換化」への対応。

- `bitrate-mode`ラジオボタンに`max_quality`を追加。
- 音声/動画フォーマットが1つも選択されていない場合、自動的にWAV
  (ロスレスPCM)を選択し、常にISO化する。
- `capacity.rs`に`estimate_lossless_audio_fit`(CD品質ロスレスWAVで
  指定ディスクに収まるかどうか、収まらない場合は代わりに何秒までなら
  収まるかを算出)を新設——ユーザー指示「必要な時間やデータサイズを
  自動で割り出す」に対応。新規テスト3本(短い尺は収まる/長すぎる尺は
  収まらず代替秒数を返す/より大きいディスクなら収まる、の3パターン)。
- 実行時に「[cd700] 収録時間(...)はロスレスWAVで収まります(必要:
  ...MB / 容量: ...MB)」のような見積もりを日英併記でログ表示する。

### 4. 変換処理の並列化(非同期・マルチスレッド)

ユーザー指示「MP4をWAVでかつISOに同時変換などいくつかの代表的な
組み合わせも非同期でマルチスレッドで同時に行える様に」への対応。
`main.js`の`convertAll`を、逐次`await`のfor文から
`runWithConcurrencyLimit`(単純なワーカープール、外部ライブラリ不要、
`navigator.hardwareConcurrency`の半分を目安の同時実行数にする)へ
変更。音声変換・動画変換自体も`Promise.all`で互いを待たず並行に開始
するようにした。**設計メモ**: `convert_media`は非asyncな
`#[tauri::command]`のため、Tauri v2は内部で`spawn_blocking`相当の
スレッドプール実行にしており、JS側で複数の`invoke`を逐次`await`せず
同時に発行すれば、複数のffmpegプロセスが実際に並列実行される
(Rust側の追加変更は不要だった)。

### バージョン

`package.json`/`src-tauri/Cargo.toml`/`src-tauri/tauri.conf.json`を
0.1.6→0.1.7へ統一。クレート全体テスト18本成功(新規4本:
`estimate_lossless_audio_fit`系3本+既存の再検証)、clippy警告は
既存の無関係な1件のみ。実機で`npm run tauri build`成功・`.msi`/
`.exe`インストーラー生成・`.sig`署名アーティファクト生成・全4種の
sidecarバイナリ(ffmpeg/ffprobe/rs-ffmpeg/rs-xorriso)の同梱を確認済み。

**次回への引き継ぎ**: (1) `v0.1.7`タグpush後のCI実際の成否確認
(特に`prerelease: false`への変更・`TAURI_SIGNING_PRIVATE_KEY`
シークレットを使った署名付きビルドがCI環境でも成功するか)。
(2) 実機での自動アップデート機能そのもののエンドツーエンド確認
(v0.1.7インストール後、v0.1.8のような次のリリースを出して実際に
「アップデートがあります」ダイアログが出て更新できるか)。
(3) ユーザーから追加指示のあった「open-directx/open-cuda/aruaru-llmの
マルチCPU・マルチコア・非同期対応」はこのリポジトリではなく各リポジトリ
側での横断的な調査・改修が必要な別課題として保留。

## HANDOFF追記(2026-09-16続き) v0.1.7 CI実際の結果確認・Android CI修正・複数ドライブ並列書き込み、v0.1.8 / Follow-up: verified v0.1.7 CI results, fixed Android CI, parallel multi-drive burning, v0.1.8

**v0.1.7の実際のCI結果(前回HANDOFFで「次回確認」としていた項目)**:
デスクトップ4ジョブ(Windows/macOS×2/Linux)は全て成功
(`gh release view v0.1.7`で`.sig`署名ファイル・`latest.json`
アップデーターマニフェスト・`prerelease: false`を実際に確認済み)。
Linuxジョブは`tauri-action`ステップだけで約24分かかった(v0.1.6の
約27分と同様の傾向——ffmpeg/ffprobe同梱+LTOビルドのため。ハングでは
なく正常な所要時間と判断)。VPS(`easy-web.tokyo/make-disk/`)への
反映も`git pull`+`curl`でのライブ確認まで完了。

**release-androidジョブは失敗**——今回のセッションの変更とは無関係な
外部要因: `android-actions/setup-android@v3`が
`Warning: Failed to find package 'tools'`で失敗するようになっていた
(Googleが非推奨の`tools`パッケージをSDKリポジトリから削除したためと
見られる)。**修正**: GitHub Actionsの`ubuntu-latest`ランナーには
元々Android SDK(`cmdline-tools`込み、`ANDROID_HOME=/usr/local/lib/
android/sdk`)がプリインストールされていることを確認済みだったため、
`android-actions/setup-android@v3`のステップ自体を削除し、
プリインストール済みの`sdkmanager`でNDKだけを追加インストールする
形に簡略化した(次回のタグpushで実際の成否を確認すること)。

**複数ドライブ並列書き込み**: ユーザー指示「ディスクへの書き込みを
選んで実行すると...書き込みも同時に行なって」への対応。`main.js`の
書き込みループを、物理ドライブが複数検出された場合はドライブ単位で
並行実行するよう変更(`Promise.all`)——1台のドライブへ同時に2つの
書き込みストリームは送れないため、同じドライブへ割り当てられた
ディスク種別同士は順番に、異なるドライブへの書き込みは互いを待たずに
並行実行する設計(ドライブ数より種別数が多い場合はラウンドロビンで
割り当て)。Rust側(`burn.rs`)は無変更——並列度の制御はJS側の
呼び出しタイミングだけで実現できる(`convertAll`の並列化と同じ設計
パターン)。

バージョンを0.1.7→0.1.8へ統一。

**次回への引き継ぎ**: (1) Android CI修正(setup-android削除)が
実際にタグpushで成功するか確認。(2) 複数物理ドライブでの並列書き込みは
実機検証未実施(この開発機には光学ドライブが1台も無い——既存の
「実機での書き込み検証は未実施」という制約がここでも該当する)。

## HANDOFF追記(2026-09-16続き2) v0.1.9 CI障害3件の切り分け・修正、v0.1.10 / Follow-up: diagnosed and fixed 3 CI failures for v0.1.9, v0.1.10

v0.1.9のタグpushで、デスクトップ4ジョブ全てが
`Resource not accessible by integration`(リリース作成権限エラー)で
一斉に失敗するという新しい問題が発生した。原因を`gh api`で調査した
ところ、**このリポジトリのActions設定`default_workflow_permissions`が
`"read"`になっていた**ことが判明(`gh api repos/aon-co-jp/make-disk/
actions/permissions/workflow`で確認)——ワークフローYAML側の
`permissions: contents: write`はリポジトリ設定の上限を超えて昇格
できないため、リポジトリ側の既定値が"read"のままだと個々のワークフローの
宣言は無視される。`gh api -X PUT .../actions/permissions/workflow
-f default_workflow_permissions=write`で修正した。

**教訓その1**: `gh run rerun --failed`は、実行中のrunに紐づいた
トークン設定をそのまま引き継ぐらしく、権限設定を直してから
再実行しても直らなかった(実際に確認)。真に新しい設定を反映させるには
新しいタグをpush(=完全に新規のワークフロー実行)する必要がある。

Android修正(前回HANDOFF記載のsdkmanagerパス修正)も同じv0.1.9で
検証したところ、さらに2つの実バグが連続して見つかった:
- **教訓その2**: `install NDK`ステップの`env:`ブロックで設定した
  `ANDROID_HOME`は、そのステップの中でしか有効ではない——後続の
  `build Android universal APK`ステップで`${{ env.ANDROID_HOME }}`
  (ワークフロー式の`env`コンテキスト)を参照しても空文字列にしか
  ならなかった(`env`コンテキストは`$GITHUB_ENV`へ明示的に書き出した
  変数だけを見る)。結果`NDK_HOME: /ndk/27.0.12077973`という壊れた値に
  なり、`tauri android build`が「Android NDK invalid」で失敗した。
  修正: `NDK_HOME`は`run:`スクリプト内で`export NDK_HOME="$ANDROID_HOME/
  ndk/..."`のようにシェル変数展開で組み立てる(`$ANDROID_HOME`自体は
  ubuntu-latestランナーにOS標準で設定済みのjob全体で有効な環境変数)。

これら3件は全て、CI環境特有の設定・スコープの落とし穴であり、
ローカルの`cargo build`/`npm run tauri build`では検出できない種類の
問題だった(実際にCIで実行して初めて発見できた)——このプロジェクトの
実機検証重視の方針が、ローカルビルドだけでは終わらない理由の実例として
記録する。

バージョンを0.1.9→0.1.10へ統一(Android修正の検証用)。

**ユーザーから新たに2件の大きな機能追加指示があった**(まだ未着手、
CI安定化を優先したため):
1. サイズ指定/時間指定/ディスクいっぱい/「AI判断で自動カット」の
   いずれかを選択可能にする機能(日英表示)。**正直な開示の方針**:
   「AI判断」は実際のML/LLM分析モデルを統合するものではなく、
   ffmpegの無音検出(`silencedetect`)等のヒューリスティックによる
   自動カットとして実装する予定(誇張しない)。
2. 音声・静止画・動画に加え、最大4Kの見開きPDF対応(右綴じ/左綴じ選択、
   選択に応じた左右ページ入れ替え)。Windows/macOS/Linux/Android/
   iPhone/iPad対応。**正直な開示**: iOS/iPadOSは既存の制約(テスト
   実機を保有していないため対応保留、本ファイル「プラットフォーム
   範囲」節参照)がそのまま当てはまる。PDF処理は音声/動画変換とは
   別の技術領域(PDFライブラリ選定・見開きページレイアウト処理)で、
   別途まとまった設計が必要。
3. (さらに)将来的なゲーム機対応の要望も受けた——現時点では具体的な
   対象機種・実現方式とも未確定のため、方向性のみ記録し設計は次回以降。

**次回への引き継ぎ(優先順位)**: (1) v0.1.10でAndroid CI修正の最終
検証(NDK_HOMEのシェル変数展開修正)。(2) 上記機能1(サイズ/時間/
ディスクいっぱい/自動カット選択)の設計・実装。(3) 上記機能2(PDF
見開き対応)は独立した大きな増分として別途設計セッションが必要。
(4) ゲーム機対応は要件が未確定のため、まずユーザーへの追加ヒアリングが
必要(対象機種・用途を明確にしてから着手すべき)。

## HANDOFF追記(2026-09-16続き3) v0.1.10でNDK_HOME修正は成功・Androidは4件目の実バグ(レースコンディション)、v0.1.11 / Follow-up: NDK_HOME fix worked in v0.1.10, Android hit a 4th real bug (race condition), v0.1.11

v0.1.10で前回のNDK_HOME修正(シェル変数展開)は**実際に効いた**——
`install NDK`・`build Android universal APK`ステップの両方が成功し、
universal APKのビルド自体は完走した。デスクトップ4ジョブ(Windows/
macOS×2/Linux)も全て成功(`.sig`・`latest.json`込みで実際に確認、
VPSへも反映済み)。

**Androidは4件目の実バグ(今度はレースコンディション)で失敗**:
`upload APK to release`ステップが`release not found`で失敗した。
原因: `release-android`ジョブと`release`(デスクトップ)ジョブは
ワークフロー内で並行実行される独立ジョブで、実際のGitHub Release
本体を作成するのは`release`ジョブ(`tauri-action`経由)側——
Androidのビルドはデスクトップより速く終わるため(6分程度)、
デスクトップ側がまだリリースを作っていない時点でAPKのアップロードを
試みてしまっていた。**修正**: `release-android`に`needs: release`を
追加し、デスクトップ側(全マトリクス)の完了を待ってから実行する
ようにした。

**このセッションでAndroid CIだけで合計4件の異なる実バグを発見・修正した
(いずれもローカルビルドでは検出不可能、CI環境特有の問題)**:
(1) `--target x86`という誤ったCLI引数値(正しくは`i686`)、
(2) `android-actions/setup-android@v3`の非推奨パッケージ取得失敗、
(3) `sdkmanager`のPATH未設定+`NDK_HOME`の`env`コンテキストスコープの
誤解、(4) 今回のジョブ間レースコンディション。加えてデスクトップ側でも
リポジトリの`default_workflow_permissions`設定という、コードとは
無関係な環境要因の障害が1件あった。**教訓**: CI/CDパイプラインの
複雑さ(並行ジョブ・環境変数スコープ・外部アクションの非推奨化・
リポジトリ設定)は、アプリケーションコード自体の複雑さとは独立した、
別の種類の「実機」であり、同じ「実際に動かして確認する」という
検証哲学がそのまま当てはまる。

バージョンを0.1.10→0.1.11へ統一(Android修正の最終検証用)。

**次回への引き継ぎ**: v0.1.11でAndroidジョブが最終的に成功するか確認
すること。成功すれば、Android CIに関する既知の実バグは今回のセッションで
全て解消されたことになる。その後、ユーザーから積み上がっている機能
要望(サイズ/時間/自動カット選択、PDF見開き対応、複数動画合成編集・
等間隔分割、ゲーム機対応)へ順番に着手する。

## HANDOFF追記(2026-09-16続き4) 「収まらない分の自動調整方法」選択機能を実装、v0.1.12 / Follow-up: implemented the "how to fit content that's too long" selector, v0.1.12

v0.1.11でAndroidジョブが最終的に成功したことを確認済み(過去の
HANDOFFで記録済みの4件のAndroid実バグは全て解消)。これを受けて、
CI安定化を優先して積み上げていた機能要望の1番目に着手した:

> 「DISKいっぱいのビットレートに変換して書き込みも良いですが、画質や
> 音質に、影響が少ない範囲で英語と日本語で表示して　サイズか、
> 時分秒か、DISKいっぱいか、AI判断で自動カット　のいずれかを選択可能
> として」

**実装内容**:
- バックエンド(`src-tauri/src/engine/convert.rs`): `detect_silence_ranges()`
  (ffmpegの`silencedetect`フィルタで無音区間を検出)と
  `bitrate_for_target_size_kbps()`(目標サイズ・総尺からビットレートを
  逆算)を新設。**正直な開示**: 「AI判断」は実際には音量ベースの無音
  検出であり、意味的なシーン解析(真のAI/ML判断)ではない。この点は
  コードのdocコメント・UI双方に日英併記で明記した。ffmpeg実バイナリを
  使った統合テスト(無音区間を含むテスト用音声を実際に生成し検出させる)
  を含め、全21テストがパス。
- `src-tauri/src/lib.rs`: 上記2関数を`#[tauri::command]`として
  Android/非Android両方のinvoke_handlerに登録。
- `src/index.html`: 「7.5. 収まらない分の自動調整方法」セクションを
  新設。`fit-strategy`ラジオボタン(disk_full[既定]/target_size/
  target_duration/ai_auto_cut)、サイズ入力・時分秒入力を追加。
- `src/main.js`: run-btnハンドラ内に分岐ロジックを追加。
  - `target_size`: 総尺から`calc_bitrate_for_target_size_kbps`で
    ビットレートを算出し上書き。
  - `target_duration`: 手動カット未設定のファイルに対し、先頭から
    指定秒数までの`CutRange`を自動設定。
  - `ai_auto_cut`: 手動カット未設定のファイルごとに
    `detect_silence_ranges`を呼び、検出した無音区間をそのまま
    `CutRange`として設定(結果は既存の「編集...」UIで確認・調整可能)。
  - `disk_full`(既定): 既存のディスク容量ベースの自動ビットレート
    算出をそのまま維持(無変更)。
  - いずれの自動編集も、ユーザーが既に手動でカット区間を設定済みの
    ファイルは上書きしない(既存の編集作業を尊重)。

バージョンを0.1.11→0.1.12へ更新。

**次回への引き継ぎ**: 残る積み上げ要望は (2) 音声・静止画・動画に
加えPDF見開き対応(右綴じ/左綴じ、最大4K、Windows/Mac/Linux/Android/
iPhone/iPad対応——iOS/iPadOSは実機無しの制約が既存)、(3) 複数の
音声・静止画・動画・PDFの合成編集・等間隔/サイズ指定分割+余りの
自動ディスクフィット、(4) 将来的なゲーム機対応(対象プラットフォーム
の要確認)。この順で着手予定。また、自動アップデート機能自体は
ビルド・署名基盤の検証は済んでいるが、実際にインストール済みの
旧バージョンが新バージョンを検知・適用するE2E動作確認はまだ未実施。

## HANDOFF追記(2026-09-16続き5) PDF見開き対応(右綴じ/左綴じ、最大4K)を実装、v0.1.13 / Follow-up: implemented PDF spread support (right/left binding, up to 4K), v0.1.13

積み上げ要望の2番目に着手した:

> 「音声、静止画、動画の他に、最大4Kの見開きPDF対応で、右綴じ、
> 左綴じ選択可能で、選択によって左右も入れ替える機能も搭載させて
> Windows、MAC、LINUC、Android、iPhone、iPAD対応して」

**実装内容**:
- 新規モジュール`src-tauri/src/engine/pdf.rs`: `BindingDirection`
  (RightToLeft/LeftToRight)、`compose_spread()`(2ページを横に並べ、
  綴じ方向に応じて左右を入れ替え、4K上限〈3840px〉を超える場合は
  縦横比を保って縮小)、`render_pdf_as_spreads()`(PDF全体を見開き画像
  群として書き出す)。
- **正直な開示・設計判断**: 見開き合成そのもの(`compose_spread`)は
  純Rustの`image` crateのみで実装し、外部ネイティブ依存が無いため
  Android/iOSを含む全ターゲットでコンパイル・実行できる。実際に
  合成ロジックを5件の単体テストで検証済み(左綴じ/右綴じでの左右配置、
  高さの異なるページの拡縮、4K超過時の縮小、4K以内では無変更)——
  全26テストがパス、clippyも(既存の無関係な1件を除き)クリーン。
  一方、PDFのラスタライズ(ページ→画像)自体はpoppler-utils
  (`pdftoppm`/`pdfinfo`)をffmpeg/xorrisoと同じsidecarパターンで
  呼ぶ設計にしたが、**poppler-utilsはまだこのリポジトリのCIで
  sidecarバイナリとして同梱・検証されていない**(現時点では実行環境の
  PATHに別途インストールされている前提。ffmpeg同様の取得スクリプト
  〈`scripts/fetch-ffmpeg-sidecars.sh`に相当するもの〉は次の増分で
  追加予定)。iOS/iPadOSは既存の「実機無し」の制約により、そもそも
  Tauriモバイルビルド自体がこのセッションでは未検証のまま。
- `src-tauri/src/lib.rs`: `convert_pdf_to_spreads`を`#[tauri::command]`
  として追加・両invoke_handlerへ登録。4K超過指定は常に4Kへ丸める
  (ユーザー指示「最大4K」の厳密な適用)。
- `src/index.html`: 「4.5. PDF見開き変換」セクションを新設
  (右綴じ/左綴じのラジオボタン、poppler-utils未同梱である旨の
  日英併記の注記)。
- `src/main.js`: run-btnハンドラの先頭でソース内のPDFファイルを検出し、
  見開き変換を実行。ソースが全てPDFの場合は音声/動画フォーマット
  必須のバリデーションをスキップしてPDF変換のみで完了とする。
  音声/動画変換(`convertAll`)からはPDFファイルを除外(ffmpegへ渡さない)。

バージョンを0.1.12→0.1.13へ更新。

**次回への引き継ぎ**: (1) poppler-utils(pdftoppm/pdfinfo)の
Windows/Linux向け静的ビルド取得スクリプトを追加し、真に「同梱・
単体で動く」状態にする、(2) 実際のPDFファイルを使ったE2E動作確認
(このセッションでは合成ロジックの単体テストのみ、実PDFでの
pdftoppm呼び出し経路は未検証)、(3) Android/iOS向けのUI導線確認
(PDF選択→見開き変換→出力先確認)、(4) 残る積み上げ要望
(複数の音声・静止画・動画・PDFの合成編集・等間隔/サイズ指定分割+
余りの自動ディスクフィット、将来的なゲーム機対応)への着手。

## HANDOFF追記(2026-09-16続き6) PDF綴じ方向の一括変換・保存機能を追加、v0.1.14 / Follow-up: added bulk PDF binding-direction conversion & save, v0.1.14

ユーザー指示への追加対応:

> 「make-disk の機能として pdf が左綴じを右綴じや右綴じを左綴じなどに
> 一括編集して保存も可能にして」

これは前段(v0.1.13)の「見開き画像として書き出す」機能とは別の要望——
**PDF自体(画像化せず)のページ順序を編集して、PDFのまま保存する**機能。

**実装内容**:
- `src-tauri/src/engine/pdf.rs`に`reverse_pdf_page_order()`を追加。
  綴じ方向の変換はページの並び順を反転することと等価(反転操作は
  どちら向きにも効く自己逆元)なので、変換先の方向を毎回指定する
  必要が無い設計にした。純Rustの`lopdf`(ネイティブ依存無し、
  Android/iOSクロスコンパイルも問題無い)でPDFの`/Pages`の`/Kids`配列を
  直接反転して保存する。
- **既知の制限(正直な開示)**: ページツリーがフラット(直接ページ
  オブジェクトのみ)であることが前提。スキャナ出力の単純なPDFは
  ほぼこの構造だが、章ごとに入れ子のページツリーを持つ複雑なPDFには
  未対応で、その場合は黙って壊れたPDFを作らず明確なエラーを返す。
- テスト: lopdfでメモリ上に最小限のテスト用PDF(ページごとに異なる
  MediaBox幅を持たせて順序を判別可能にした)を実際に構築し、
  (1) フラットなページツリーが正しく反転されること、(2) 入れ子の
  ページツリーが検出されて明確なエラーになること、の2つを実際に
  検証(モック無し、本物のPDF読み書きを経由)。全28テストがパス、
  clippyも既存の無関係な1件を除きクリーン。
- `src-tauri/src/lib.rs`: `rebind_pdfs`を`#[tauri::command]`として
  追加(複数PDFの一括処理に対応、`Vec<Result<String, String>>`を
  返し1件の失敗で全体を止めない)、両invoke_handlerへ登録。
- `src/index.html`: PDF見開き変換セクション内に「選択したPDFの
  綴じ方向を一括変換して保存」ボタンと日英併記の注記を追加。
- `src/main.js`: 専用のクリックハンドラを追加、ソース内のPDF全てを
  対象に`rebind_pdfs`を呼び、結果(成功/エラー)をファイルごとに
  ログ表示する。

バージョンを0.1.13→0.1.14へ更新。

**次回への引き継ぎ**: (1) 実際の複雑なPDF(入れ子ページツリー・
暗号化・フォーム等)での動作確認(このセッションでは合成テスト用の
最小PDFのみで検証)、(2) 入れ子ページツリー対応の拡張(現状は
明確なエラーで止まるのみ)、(3) 前回同様に残る積み上げ要望
(複数の音声・静止画・動画・PDFの合成編集・等間隔/サイズ指定分割+
余りの自動ディスクフィット、将来的なゲーム機対応)への着手。

## HANDOFF追記(2026-09-16続き7) 複数ファイルの合成・分割編集(等間隔/サイズ指定、あまりの自動ディスクフィット)を実装、v0.1.15 / Follow-up: implemented multi-file composite/split editing (equal-interval/size-based, remainder auto-fit), v0.1.15

積み上げ要望の3番目に着手した:

> 「複数の動画も同時に編集して合成可能にして一つか複数の長い動画も、
> 短く編集や時間指定なども等間隔でも指定可能にやサイズ指定でも
> 編集可能にしてあまりは、DISKいっぱいにビットレートを自動変更して
> 自動編集して」/「複数の音声・静止画・動画・PDFの合成編集・
> 等間隔/サイズ指定分割+余りの自動ディスクフィット」

**実装内容(静止画・PDFの合成は既存のPDF見開き機能〈4.5〉で対応済みの
ため、ここでは音声・動画の合成〈結合〉と分割を担う)**:
- `src-tauri/src/engine/convert.rs`に3つの関数を追加:
  - `equal_interval_segments(total_secs, segment_count)`: 等間隔分割。
    既存の`TrimRange`(開始+長さ)をそのまま返すため、`convert_media`を
    区間ごとに呼ぶだけで分割出力でき、新しい抽出処理の実装は不要。
  - `fixed_length_segments(total_secs, segment_secs)`: サイズ指定分割。
    割り切れない最後の区間はそのまま短い「あまり」として返す
    (呼び出し側でその区間だけディスク容量いっぱいのビットレートに
    自動調整する設計)。
  - `concat_media(input_paths, output_path, has_video)`: ffmpegの
    `concat`フィルタで複数ファイルを結合(合成)する。再エンコード
    方式のため入力同士のコーデック・解像度が揃っていなくても結合
    できる。動画/音声の混在結合は現時点で未対応(呼び出し側が
    拡張子から判定する`has_video`で動画のみ/音声のみを前提)。
- テスト: 等間隔/サイズ指定分割の純粋なロジックを4件のテストで検証
  (境界値含む)。結合(`concat_media`)は実ffmpegを使ったE2Eテストで
  検証(音声トラック付きテスト動画を2本生成し、結合後の尺が合計と
  一致することを確認)。全34テストがパス、clippyも既存の無関係な
  1件を除きクリーン。
- `src-tauri/src/lib.rs`: `concat_media_files`・`calc_equal_interval_segments`・
  `calc_fixed_length_segments`を`#[tauri::command]`として追加・
  両invoke_handlerへ登録。
- `src/index.html`: 「8.5. 複数ファイルの合成・分割編集」セクションを
  新設(結合ボタン、等間隔/サイズ指定の分割設定、あまりの自動
  ディスクフィットのチェックボックス)。
- `src/main.js`: 結合(`concat-btn`)・分割(`split-btn`)の専用ハンドラを
  追加。分割はソース先頭の音声/動画ファイル1本を対象とし、サイズ
  指定モードで最後の区間が「あまり」(ちょうど割り切れない)場合、
  チェックボックスがオンかつディスク種別が選択されていれば、既存の
  `calc_auto_bitrate_kbps`(ディスク容量からの自動ビットレート算出)を
  その区間だけに適用する。

バージョンを0.1.14→0.1.15へ更新。

**次回への引き継ぎ**: (1) 音声/動画の混在結合への対応、(2) 3ファイル
以上の複雑な合成編集UI(現状は「ソース順に結合」のみ、部分区間の
指定結合等は未対応)、(3) 分割対象を「ソース先頭の1本」に限定して
いる制約の緩和(複数ファイルを選んで一括分割)、(4) 積み上げ要望の
最後の項目「将来的なゲーム機対応」への着手(対象プラットフォーム
の要確認)。

## HANDOFF追記(2026-09-16続き8) 積み上げ要望1〜3が完了、項目4「将来的なゲーム機対応」はロードマップ化 / Follow-up: backlog items 1-3 complete, item 4 "future game console support" recorded as a roadmap entry (not started)

CI安定化を優先して積み上げていた4件の機能要望のうち、1〜3は実装・
リリース済み(v0.1.12〜v0.1.15、詳細は上記の各HANDOFFを参照):

1. サイズ/時間/ディスクいっぱい/AI判断自動カットの選択機能(日英表示) — v0.1.12
2. 音声・動画に加えPDF見開き対応(右綴じ/左綴じ、最大4K)+ 綴じ方向の
   一括変換保存 — v0.1.13/v0.1.14
3. 複数の音声・動画の合成(結合)編集・等間隔/サイズ指定分割+余りの
   自動ディスクフィット — v0.1.15

**項目4「将来的なゲーム機対応」について**: ユーザーへ対象プラット
フォーム(PS/Xbox/Switch等どれを想定するか)の確認を提案したところ、
「将来的にはGAMEマシンにも対応したい、とするのはロードマップに
入れておいていただければよい」との回答。**今回は実装に着手せず、
ロードマップ項目として記録するのみ**(具体的な対象機種・設計は
未確定・未着手)。

**次回このセッションを再開する人へ**: 積み上げ要望は全て「着手済み
または記録済み」の状態になった。次に何か新しい要望が来るまでは、
特に緊急の未完了タスクは無い(自動アップデートのE2E動作確認、
poppler-utilsのsidecar同梱化等、各HANDOFFに記載した細かい
フォローアップ項目はあるが、いずれも致命的ではない改善点)。

## HANDOFF追記(2026-09-17) 実バグ修正: ISO作成が本家xorriso未同梱により必ず失敗する問題+rs-ffmpeg/rs-xorrisoフォールバック実装、v0.1.16 / Follow-up: fixed a real bug where ISO creation always failed (real xorriso was never bundled) and wired rs-ffmpeg/rs-xorriso as fallbacks, v0.1.16

ユーザーが実機(v0.1.15)で実際にISO作成を試したところ、下記のエラーで
必ず失敗することが判明した:

```
エラー: xorrisoの起動に失敗しました(未インストールの可能性): program not found
```

**根本原因(実機ログから特定)**: `scripts/fetch-ffmpeg-sidecars.sh`は
コメントに明記の通りffmpeg/ffprobeのみを取得し、**本家xorrisoは
Windows/Linux/macOSいずれもsidecarとして一切同梱していなかった**。
一方、`scripts/build-rs-tribute-sidecars.sh`で`rs-xorriso`(このプロジェクト
用の純Rust実装、ISO生成のみ対応)はWindows/Linux向けに実際に同梱
されていたにもかかわらず、`engine/iso.rs`は常に`xorriso`(本家)だけを
呼んでおり`rs-xorriso`を一度も試していなかった——同梱していても
呼び出し側が使っていない、という単純だが致命的な配線ミスだった。

**ユーザーからの追加指示**: 「もう一つのオープンソースのRust版
(rs-ffmpeg)も同梱して呼び出すように修正して」「rs-ffmpeg.exe/
rs-xorriso.exeの二つは同梱して呼び出して利用して」

**修正内容**:
- `engine/iso.rs`: `create_iso`を、本家`xorriso`を優先して試し、
  起動自体に失敗した場合(PATH上に無い場合)のみ同梱の`rs-xorriso`へ
  フォールバックするように修正。実際にビルドした`rs-xorriso.exe`を
  テスト実行ファイルの隣へ配置し、本当にISOファイルを書き出せることを
  検証する実機E2Eテストを追加(モックに頼らない、このプロジェクトの
  既存方針通り)。
- `engine/burn.rs`: 同じフォールバックパターンを`burn_image`・
  `list_devices`にも適用。**正直な開示**: rs-xorrisoは実際のディスク
  書き込み(`-as cdrecord`)・ドライブ列挙(`-devices`)を明示的に
  「未実装、本家xorrisoを使ってください」と拒否する設計のため、
  ここでのフォールバックは実際の書き込みを可能にはしないが、OS
  レベルの生の「program not found」より分かりやすいエラーになる。
- `engine/probe.rs`: `probe()`を、本家`ffprobe`が起動できない場合に
  同梱の`rs-ffmpeg probe`へフォールバックするように修正。**正直な
  開示**: rs-ffmpegは非圧縮WAV専用のprobeのみ対応のため、WAV以外の
  ファイルでは分かりやすいエラーになる(黙って嘘の結果を返さない、
  rs-ffmpeg自身の設計方針)。実際にビルドした`rs-ffmpeg.exe`と実ffmpegで
  生成した本物のWAVファイルを使い、フォールバック経路(`probe_with_rs_ffmpeg`)
  が正しく解析することを検証する実機E2Eテストを追加。
- `engine/convert.rs`の`run_ffmpeg`(通常変換・カット処理の共通実行点)にも
  同じフォールバックを追加。**正直な開示**: rs-ffmpegはコーデック指定・
  ビットレート指定・トリミング等のフラグを自身で明確に拒否する設計
  (未対応フラグを黙って無視して壊れたファイルを作らない)なので、
  この経路は「本家ffmpegが無く、かつ単純なWAVのサンプルレート/
  チャンネル変換のみ」の場合にのみ実際に成功し、それ以外は分かりやすい
  エラーで終わる。
- 全37テストがパス(新規追加した実バイナリE2Eテスト2件含む)、
  clippyも既存の無関係な1件を除きクリーン。

バージョンを0.1.15→0.1.16へ更新。

**次回への引き継ぎ**: (1) macOSにも本家xorriso/ffmpegの何らかの取得
手段(Homebrew前提の案内、または静的ビルドの取得)を検討する、
(2) 「自動算出ビットレート: 10003421 kbps」のような明らかに異常な値が
ユーザーのログで観測された件は未調査(probe結果が極端に短い尺を
返した可能性が高いが、該当ファイルでの再現待ち)——次回セッションで
優先して調査すること。

## HANDOFF追記(2026-09-17続き) 動画の解像度・FPS指定機能を追加(DVD/BDプリセット、カスタム、AI最適化)、v0.1.17 / Follow-up: added video resolution & FPS controls (DVD/BD presets, custom, AI-optimized), v0.1.17

ユーザー指示:

> 「DVDは通常解像度720×480とフルHDを選択可能に、ブルーレイは
> フルHDと4Kを選べるようにして、ビデオ出力は、720X480から4Kや5Kや
> 8Kも指定可能にして、FPSは、最低不明、24FPS、30FPS 60FPS 120FPなども
> 選択や指定可能にして」「無指定やAIが最適化も選択可能にして」
> 「無指定やAI最適化はミス AI最適化も選択可能にして」(表記統一)

**実装内容**:
- `src-tauri/src/engine/convert.rs`: `ConvertJob`に`resolution: Option<Resolution>`
  (`Resolution{width,height}`)・`fps: Option<u32>`を追加。
  `push_resolution_args`(`-vf scale=W:H`)・`push_fps_args`(`-r fps`)を
  新設し、単純変換パス(`run_convert_simple`)とカット区間結合後の
  最終エンコードパスの両方に適用。カット区間の結合を`-c copy`(無劣化)
  にするかどうかの判定にも解像度/FPS指定の有無を追加(指定があれば
  再エンコードが必要なため)。実ffmpegで実際に1920x1080・30fpsへ
  変換し、ffprobeで実際の出力を検証する実機E2Eテストを追加。
- `src-tauri/src/engine/probe.rs`: `MediaInfo`に`width`・`height`・`fps`
  (動画ストリームが無ければ`None`)を追加し、「AI最適化」判定の
  材料にする。`r_frame_rate`(分数文字列)をf64へ変換する
  `parse_frame_rate_fraction`を追加(単体テスト込み)。
- `src/index.html`: 「4.2. 解像度・フレームレート」セクションを新設。
  解像度は無指定/720×480(DVD標準)/1920×1080(フルHD)/4K/5K/8K/
  カスタム/AI最適化から選択、FPSは無指定/24/30/60/120/カスタム/
  AI最適化から選択(日英ハイブリッド表記)。
- `src/main.js`: `resolveResolutionSetting`・`resolveFpsSetting`を追加し
  `convertAll`(動画フォーマットのみに適用、音声には適用しない)へ配線。
  **「AI最適化」の正直な開示**(意味的な画質・動き解析ではない
  ヒューリスティック、index.htmlの注記にも明記):
  - 解像度: ソースの解像度を検出し、4Kの画素数を超える場合のみ
    アスペクト比を保って4Kへ縮小する(4K以下ならそのまま、不要な
    再エンコードを避ける)。
  - FPS: ソースのフレームレートを検出し、24/30/60/120のうち最も
    近い値へスナップする(既に十分近ければ無変換)。

**「フルHD→4Kコンバート」に関するユーザーからの追加要望と方針決定**:
上記の解像度指定機能により、ffmpegのscaleフィルタ(Lanczos等)による
基本的な拡大変換は既に可能(4Kプリセットを選ぶだけ)。一方
ユーザーは「open-cuda/open-directxをフル動員した本格的なAI超解像
(ディテールを補う高画質化)」を新規設計から進めることを選択した
(選択肢: 基本版〈補間スケール、即日〉 vs 本格AI超解像〈ML モデル+
GPU推論ランタイムの新規同梱、設計・検証に複数セッション規模の
作業が必要〉のうち後者を選択)。**これはまだ着手前**——次回セッションは
まずRust向けONNXランタイムクレート(`ort`等)の調査、このマシンでの
実際のGPU有無確認、ライセンス的に利用可能な超解像モデル
(Real-ESRGAN等)の選定から始めること。

全39テストがパス、clippyも既存の無関係な1件を除きクリーン。
バージョンを0.1.16→0.1.17へ更新。

## HANDOFF追記(2026-09-19) 実バグ修正: Windowsで光学ドライブを検出できず書き込み不可、v0.1.18 / Fix: Windows could not detect optical drives or burn, v0.1.18

v0.1.16でISO作成は成功するようになったが、ユーザー実機(D:にBD-REドライブ)で
「エラー: 書き込み可能な光学ドライブが見つかりません。」が出て、ISOへ変換後の
ディスク書き込みができなかった。**原因**: `list_devices`/`burn_image`が本家
`xorriso -devices`/`-as cdrecord`頼みで、Windowsには本家xorrisoが同梱されて
おらず(rs-xorrisoも列挙・書き込みを明示的に未実装として拒否)、列挙も書き込みも
原理的に不可能だった。
**修正(`engine/burn.rs`)**: Windowsでは(1)`Get-CimInstance Win32_CDROMDrive`
(Capabilitiesに4=書き込み対応を含むもの)でドライブレターを列挙、(2)`D:`形式の
デバイスにはWindows標準の`isoburn.exe /Q D: <iso>`で書き込む。実機で
`list_devices()`が`["D:"]`を返すことを実テストで確認。
**正直な開示(未検証)**: 実際にディスクを焼くE2Eは未実施(空きメディアが必要で
このセッションでは焼いていない)。isoburn.exeは速度・ディスク種別の指定を
受け付けず(メディア自動判定)、終了コードでしか成否が分からない。Linux/macOSは
従来通り本家xorriso前提のまま(macOS/Linuxの本家xorriso同梱は未対応)。
**未調査の残課題**: ログにあった「自動算出ビットレート: 10003421 kbps」の異常値
(尺が極端に短く誤検出された疑い、要再現)。

## HANDOFF追記(2026-09-19続き) IMAPI2化(日本語ファイル名保持・成否判定)・ビットレート上限、v0.1.19 / IMAPI2 rewrite (Unicode filenames, reliable result) and bitrate cap, v0.1.19

実機(BD-REドライブ+空きCD-R)テストで判明したこと:
- v0.1.18のisoburn.exe経路は、**実際にはディスクへ書き込めていた**(D:に
  データ確認)が、終了コードは1で成否をコードから判定できなかった。
- rs-xorrisoが日本語ファイル名を`________.WAV`に潰していた(8.3のみ対応)。
- `create_iso`の元フォルダが出力フォルダ自身のため、前回の`output.iso`が次回の
  ISOに混入する。
- 「自動算出ビットレート 10003421 kbps」の原因: **元MP4が0.533秒しかない**
  (ffprobeで確認)。変換の不具合ではなく、極端に短い尺で容量逆算した結果。

対応:
- 新規`engine/windows_imapi.rs`+`scripts/imapi_*.ps1`(ASCIIのみ厳守:
  PowerShell 5.1はBOM無しUTF-8をANSI読みし、日本語コメントが次行を巻き込んで
  自己除外処理を無効化する実バグを実際に踏んだ。回帰テストあり)。Windowsでは
  IMAPI2FSでISO9660+JolietのISO作成(日本語名保持を実テストで確認)、IMAPI2で
  書き込み(例外メッセージ取得、成功後に排出)。失敗時のみ従来のxorriso系へ。
- ISO作成時に出力ISO自身を除外(一時ファイル経由で差し替え)。
- `main.js`: 自動/最高品質モードのビットレートを元ファイルのビットレートで頭打ち。
**正直な開示(未検証)**: IMAPI2書き込み(`imapi_burn.ps1`)は空きメディアでの
E2E未実施(前回のCDは書き込み済みのため)。手動テスト
`MAKE_DISK_TEST_ISO=<iso> cargo test --lib real_disc_tests -- --ignored`で実施できる。

## HANDOFF追記(2026-09-19続き2) CD書き込みE2E成功・AV1/Opus・Dolby/サラウンド保持・音声ビットレート修正 / Real CD burn E2E OK, AV1/Opus, Dolby/surround preservation, audio bitrate fix

- **実機E2E**(空きCD-R): 元MP4(AV1 1080p・3.56時間・2GB)→AAC(CD容量いっぱい
  約397kbps、655MB)→IMAPI2FSでISO(666MB、日本語名保持)→IMAPI2で書き込み、を
  `windows_imapi::full_flow`(`--ignored`の手動テスト)で実行し**書き込み成功**
  (221秒)。途中で発見・修正した実バグ: (1) PowerShellの`[ref]`でIStreamを受け取れず
  書き込みが必ず失敗→C#側`NativeStream.Open`で開いて返す形に修正、(2) ビットレート指定が
  常に`-b:v`で**音声のみ出力では無視されていた**→`is_audio_only_output`で`-b:a`に、
  音声専用出力には`-vn`。書き込み後のディスク内容確認は未実施(トレイ排出のため)。
- **AV1/Opus**(`convert.rs`): 疑似指定`-c:v av1`を、ffmpegの`-encoders`から選んだ
  libsvtav1(優先)/libaom-av1へ置換(`detect_av1_encoder`)。UIに Opus・AV1+Opus(MKV/WebM)・
  AV1+AAC(MP4)を追加。実ffmpegでAV1映像+Opus音声を出力して検証。
- **Dolby/サラウンド**: Dolby Vision/Atmos/Dolby Cinema/IMAX Enhanced/4DXは各社
  ライセンス制で**新規生成(エンコード)は不可**と明示。対応するのは(1)`-map 0 -c copy`の
  無変換コピー(DV RPU・Atmos・5.1/7.1保持、ビットレート/解像度/fps指定は付けない)、
  (2)互換下位形式(HEVC 10bit HDR10〔DVの動的メタデータは落ちる〕、AC-3/E-AC-3)、
  (3)`probe_media`が`dolby_vision`/`audio_codec`/`audio_channels`/`audio_profile`を返し
  変換時に検出結果をログ表示。実ffmpegで5.1がE-AC-3変換・無変換コピーで保持されることを検証。
- **著作権保護(CSS/AACS等)の回避は実装しない**と回答済み(違法となり得るため)。
  保護なしディスクの取り込みと、保護検出時の日英案内で設計予定(未着手)。
- 未着手の要望: DVD/BDディスクの取り込み(リッピング、保護なしのみ)→アップコンバート、
  AI超解像(open-cpu→open-directx→open-cuda→aruaru-llm、tract/ort調査済み)。

## HANDOFF追記(2026-09-19続き3) AIノイズ除去(本物のRNNoise)・CI取得スクリプト修正、v0.1.20 / Real-model AI denoise + CI fetch fix, v0.1.20

- **v0.1.19はWindowsジョブがCIで失敗**(ubuntu/mac aarch64のみ成功、リリースは不完全)。
  原因: `fetch-ffmpeg-sidecars.sh`が未認証でGitHub APIを呼び、共有IPのレート制限で
  応答が空→`grep`失敗→`set -e`でメッセージ無しのexit 1。修正: `GITHUB_TOKEN`で認証
  (`release.yml`のステップにenv追加)、5回リトライ、失敗時はAPI応答を表示。**v0.1.20で出し直す**。
- **AIノイズ除去(ユーザー指示「ヒューリスティックではなく本物のAIモデルで」)**:
  `ConvertJob.ai_denoise{mix}`→ffmpegの`arnndn`(RNNoise、リカレントNN、CPU動作)。
  モデルは`src-tauri/models/rnnoise-general.rnnn`(GregorR/rnnoise-models
  marathon-prescription、作者が「著作権対象外」と明記)を`include_bytes!`で埋め込み、
  使用時に一時ファイルへ展開(Tauriのリソース配置に依存しない)。Windowsのパスは
  フィルタ記述で`\:`の二重エスケープが必要(実際に踏んだ)。実ffmpeg+実モデルで
  ホワイトノイズが6dB以上低下することを検証。UIは「3.5 AIノイズ除去」+強さ。
  **正直な開示**: RNNoiseは主に音声で学習されており、音楽では効果が控えめで高域が
  鈍ることがある。**音声の超解像(高音質化)モデルは未実装**(要モデル選定:
  重い拡散系が多くCPUで数時間素材は非現実的)。
- 未着手: BD/DVD(保護なし)の取り込み→CD化、本物のAI映像超解像、音楽CD(CD-DA)書き込み
  (現状はデータCD。CD-DAはIMAPI2のTrackAtOnceが別途必要)。

## HANDOFF追記(2026-09-19続き4) ディスク変換方向による解像度プリセットの絞り込み / Resolution presets by disc-conversion direction

ユーザー指示: ブルーレイ→DVDは「フルHDとそれ以下の普通のDVD解像度」、DVD→ブルーレイは
「フルHDと4K」を選べるように。`index.html`に「ディスク変換の方向」(指定なし/BD→DVD/DVD→BD)
を追加、`main.js`の`applyDiscDirection`で解像度プリセットを絞り込む(BD→DVD: 720×480・
720×576(PAL、新規追加)・1920×1080、DVD→BD: 1920×1080・3840×2160。無指定/カスタム/AI最適化は常に可)。
選択中の値が範囲外になったら先頭の許可値へ自動切替。ローカルHTTPサーバー+スタブで実際のDOM
操作を検証済み。**正直な開示**: フルHDのDVDはDVD-Video規格外で家庭用プレーヤーで再生できない
場合がある(UIに明記)。DVD→4Kは補間拡大で、本格AI超解像は未実装。

## HANDOFF追記 / Handoff (2026-09-19続き5) DSD64〜1024・プラグイン機構・解像度の方向絞り込み・実測 / DSD, plugin manager, direction-based presets, measurements

**日本語**
- **DSD出力(64/128/256/512/1024、DSF)**: ffmpegはDSDを**デコードのみ**で書き出せない(実機確認)ため`engine/dsd.rs`で自前実装。
  ffmpegでDSDレート(44.1kHz×倍率)のf32へリサンプル→5次ΔΣ変調器(Butterworth型NTF、高域ゲイン1.5=Lee基準、入力-6dB)→
  DSF(LSBファースト、4096Bブロック)。2チャンネルは別スレッドで並列変調。**実測(実ffmpegのDSFデコーダで往復)**: DSD64=99.6dB/
  DSD128=132.4dB(**訂正(2026-09-19)**: 当初「両方約78.9dB=デコーダ側の限界」と書いたのは誤りで、ffmpegの`sine`ソースの位相誤差による測定の床だった。
  Rustで厳密な正弦波を生成し直して測り直した実測値がこれ)。**速度(最適化ビルド、
  2秒素材、実時間比)**: DSD64=0.3倍/128=0.4/256=0.8/512=1.6/1024=3.1(デバッグビルドは約10倍遅い)。実用的なのはDSD256程度まで。
  実バグ: DSFヘッダの「サンプル数」欄のオフセット誤り(ffmpegが不正データとして拒否)を往復テストで検出・修正。
- **「AI/ハードウェア加速」の正直な結論**: ΔΣ変調は直前の出力に依存する逐次処理で、時間方向のSIMD(AVX2/AVX-512)やGPU/NPUでは
  高速化できない(open-cpuはISA検出のみでコア数情報は無い)。効くのはチャンネル並列のみ(実装済み)。ffmpeg側のx264/x265/SVT-AV1/
  リサンプラはAVX2/AVX-512を実行時に自動使用。AIが意味を持つのはDSD化の**前段**の音声超解像(帯域拡張)で、未実装(モデル選定が必要)。
  ΔΣ変調そのものを「AI変換」と称することはしない(数学的処理のため)。
- **プラグイン機構(`engine/plugins.rs`)**: 同梱のrs-ffmpeg/rs-xorrisoを`<データフォルダ>/make-disk/plugins`へ同期。版=サイズ+FNV-1aハッシュ、
  `<名前>.version`が同じなら**コピーをスキップ**(実テストで更新日時が不変なことを確認)、違えば上書き、無ければ新規。`resolve_tool`は
  プラグインフォルダを最優先。**限界**: インストーラー自体は上書き時に同梱`rs-*`(各約200KB)を書き直す。完全に省くには姉妹リポジトリの
  リリース資産からのオンデマンド取得が必要(未実装)。Windowsのファイル名保持のためISO作成はIMAPI2が優先で、rs-xorrisoはフォールバック。
- **解像度の方向絞り込み**: BD→DVD(720×480/720×576/フルHD)、DVD→BD(フルHD/4K)。DOM操作で実検証。
- **著作権保護の回避は実装しない**旨をユーザーへ回答済み。**本格AI超解像(DVD→4K)は未実装**——ユーザー指示で最優先課題だが、モデル調達
  (Python無し、.pth→ONNX不可)が壁。CPU(tract+open-cpu)→GPU(open-directx/open-cuda)の順で、次回から着手。

**English**
- **DSD output (64/128/256/512/1024, DSF)**: ffmpeg can only *decode* DSD (verified), so `engine/dsd.rs` implements it: resample to the DSD rate as f32 via
  ffmpeg → 5th-order delta-sigma modulator (Butterworth NTF, max gain 1.5, input −6 dB) → DSF. The two channels are modulated on separate threads.
  Round-tripped through ffmpeg's DSF decoder: SNR = 99.6 dB (DSD64) and 132.4 dB (DSD128). **Correction (2026-09-19)**: the earlier "78.9 dB for both, decoder-limited" was wrong — it was a
  measurement floor caused by the frequency error of ffmpeg's `sine` source; these are the values against an exact Rust-generated sine. Speed (optimized build, 2 s clip, real-time ratio): 0.3x / 0.4 / 0.8 / 1.6 / 3.1 for DSD64…1024
  (debug builds are ~10x slower); DSD256 is about the practical limit. A real bug (wrong DSF sample-count offset, rejected by ffmpeg) was caught by the round-trip test.
- **Honest conclusion on "AI / hardware acceleration"**: delta-sigma modulation is sequential, so time-axis SIMD (AVX2/AVX-512) and GPU/NPU cannot speed it up
  (open-cpu only detects ISA features, no core counts); only channel parallelism helps (done). ffmpeg's encoders/resampler already use AVX2/AVX-512 at runtime.
  AI is meaningful *before* DSD conversion as audio super-resolution (bandwidth extension) — not implemented (needs a model). We do not label the modulator "AI".
- **Plugin manager (`engine/plugins.rs`)**: syncs the bundled rs-ffmpeg/rs-xorriso into `<data dir>/make-disk/plugins`. Version = size + FNV-1a hash; an identical
  `<name>.version` means the copy is skipped (test confirms the mtime is unchanged), a different one overwrites, none installs. `resolve_tool` prefers the plugin
  folder. **Limit**: the installer itself still rewrites the bundled `rs-*` (~200 KB each) on overwrite; removing that needs on-demand download from the sister
  repos' release assets (not implemented). On Windows IMAPI2 takes precedence for ISO creation (keeps filenames); rs-xorriso is the fallback.
- **Direction-based resolution presets**: BD→DVD (720×480/720×576/Full HD), DVD→BD (Full HD/4K), verified via real DOM interaction.
- Answered that **copy-protection circumvention is not implemented**. **Real AI super-resolution (DVD→4K) is not implemented** — the user's top priority, blocked by
  model sourcing (no Python, no .pth→ONNX); next session starts with CPU (tract + open-cpu), then GPU (open-directx / open-cuda).

## HANDOFF追記 / Handoff (2026-09-19続き6) AI超解像(GPU)・高解像度PCM・DSD同梱PCM・測定の訂正 / GPU AI upscaling, hi-res PCM, DSD PCM companion, measurement correction

**日本語**
- **本格AI超解像(映像、GPU)**: `engine/ai_upscale.rs`。公式Real-ESRGAN-ncnn-vulkan(MIT、v0.2.5.0、約45MB)を**オンデマンドDLのプラグイン**
  (`<プラグインフォルダ>/realesrgan/<版>/`、あれば再取得しない)として使用。ffmpegでPNG展開→100枚ずつ超解像→x264(crf12)の中間動画→元音声を付けて
  中間ファイル→通常の変換(解像度/コーデック/ビットレート)へ。実GPU(GT 730)で160×120→640×480+音声保持を実テストで確認。**実測速度**(720×480の1フレーム):
  `realesr-animevideov3`=4.6秒、`realesrgan-x4plus`=110秒 → 短いクリップ向け(上限2400フレーム)。**GPU非搭載/Vulkan非対応では動かない**
  (`-g -1`は「invalid gpu device」を実機確認)。CPU専用版はロードマップ(モデルはconv18層+PReLU+PixelShuffleの小型で、720×480で約0.4TMAC/フレーム→
  AVX2/AVX-512のRust推論でGT 730並みの見込み)。**「Pythonが無い」は誤りだった**: `C:\Users\noruk\AppData\Local\Programs\Python\Python313`が実在
  (`python3`名で見つからなかっただけ)。torch未導入だが`pip`可なので、.pth→ONNX変換も可能。Rust実装/資産は`reve`(Real-ESRGAN動画)等がGitHubにある。
- **高解像度PCM(マルチビット/R-2R向け)**: 352.8k/384kHz×24・32bit、705.6k/768kHz×32bit。疑似フィルタ`-af hq-resample@<Hz>`をRust側で
  `hq_resample_filter()`(soxrが使えればsoxr precision=33、この開発機のffmpegはsoxr無し=実機確認→swresampleの高精度設定)+`triangular_hp`ディザへ展開し、
  レートはフィルタ内`out_sample_rate`で指定(`-ar`だと後段に標準リサンプラが入り無駄になる)。複数の`-af`は1チェーンへ統合(ffmpegは最後の`-af`しか使わない)し、
  48kHz専用のarnndn(AIノイズ除去)はリサンプルより**前**に並べる(後だと48kHzへ戻る、実テストで発見)。**実測SNR(厳密な基準正弦波、1kHz)**: 352.8k/24bit=141.2dB、
  352.8k/32bit=149.8dB、384k/32bit=150.2dB、705.6k/32bit=149.9dB。
- **DSD同梱PCM**: DSD選択時、既定でFLAC 24bit/352.8kHzも並べて出力(DSD非対応機器向け)。**ファイル内のDSD→PCM自動フォールバックは不可能**(再生機器の機能)なので並置で対応。
  DSDは仕様上1bitのためマルチビット変調器の出力は格納できず、「マルチビットの良さ」は高解像度PCMで提供する(ユーザー合意済みの仕様)。
- **測定の訂正(重要)**: 以前のSNR約78.9dB(DSD64/128同値)を「デコーダ側の限界」としたのは**誤り**。ffmpegの`sine`ソースの位相誤差による測定の床だった。厳密な正弦波
  (`dsd::write_exact_sine_wav`)で測り直した実測は **DSD64=99.6dB、DSD128=132.4dB**(レートが上がるほど改善)。ステレオ→モノの`-ac 1`は√2倍に混合されるため
  `pan=mono|c0=c0`で左chを取り出す。DSD変換の速度(最適化ビルド): DSD64=0.3倍/128=0.5/256=1.0/512=2.0/1024=3.2(高品質リサンプラ適用後)。
- **開発上の反省**: perl/sed/nodeによる文字列置換で`\d`のバックスラッシュ消失・二重適用・stdin待ちの`cat`によるハングが複数回発生。以後のソース編集はEditツールを使う。

**English**
- **Real AI video super-resolution (GPU)**: `engine/ai_upscale.rs` uses the official Real-ESRGAN-ncnn-vulkan (MIT, v0.2.5.0, ~45 MB) as an on-demand plugin
  (`<plugins>/realesrgan/<version>/`, not re-downloaded if present): ffmpeg → PNG frames → upscale 100 at a time → x264 (crf 12) chunks → mezzanine with the original
  audio → the normal resolution/codec/bitrate pipeline. Verified on the real GPU (GT 730): 160×120 → 640×480 with audio kept. **Measured speed** (720×480 frame):
  `realesr-animevideov3` 4.6 s, `realesrgan-x4plus` 110 s → short clips only (limit 2400 frames). **It does not run without a Vulkan GPU** (`-g -1` → "invalid gpu device",
  verified). A CPU-only build is on the roadmap (the model is small: 18 conv layers + PReLU + PixelShuffle, ~0.4 TMAC per 720×480 frame, so AVX2/AVX-512 Rust inference should
  match a GT 730). **"No Python here" was wrong**: Python 3.13 exists at `...\Programs\Python\Python313` (it just wasn't found as `python3`); torch isn't installed but `pip` works,
  so .pth→ONNX conversion is possible. Rust projects such as `reve` exist on GitHub.
- **High-resolution PCM (multi-bit / R-2R)**: 352.8/384 kHz × 24/32-bit and 705.6/768 kHz × 32-bit. The pseudo filter `-af hq-resample@<Hz>` is expanded in Rust to
  `hq_resample_filter()` (soxr precision 33 if available — this dev ffmpeg has no soxr, verified — else a high-precision swresample setup) plus `triangular_hp` dither; the
  rate is set inside the filter (`out_sample_rate`) because `-ar` would add a second default resampler. Multiple `-af` are merged into one chain (ffmpeg keeps only the last),
  and the 48 kHz-only arnndn (AI denoise) is placed **before** the resampler (after it the output falls back to 48 kHz — found by a test).
  **Measured SNR (exact reference sine, 1 kHz)**: 352.8k/24-bit 141.2 dB, 352.8k/32-bit 149.8 dB, 384k/32-bit 150.2 dB, 705.6k/32-bit 149.9 dB.
- **DSD PCM companion**: selecting DSD also writes FLAC 24-bit/352.8 kHz by default (for devices without DSD). An in-file DSD→PCM fallback is impossible (a player/DAC feature), so
  the files sit side by side. DSD is 1-bit by definition, so a multi-bit modulator's output cannot be stored; the "multi-bit goodness" is delivered as high-res PCM (agreed spec).
- **Measurement correction (important)**: the earlier "SNR ≈ 78.9 dB for both DSD64/128, decoder-limited" was **wrong** — it was a measurement floor from the frequency error of ffmpeg's `sine` source.
  Against an exact sine (`dsd::write_exact_sine_wav`) the real values are **DSD64 = 99.6 dB, DSD128 = 132.4 dB** (better at higher rates). `-ac 1` mixes stereo to √2×, so tests use `pan=mono|c0=c0`.
  DSD speed (optimized build, after the high-quality resampler): DSD64 0.3x / 128 0.5 / 256 1.0 / 512 2.0 / 1024 3.2 real time.
- **Process note**: string edits via perl/sed/node repeatedly lost `\d` backslashes, double-applied, or hung on a stdin-waiting `cat`. Source edits now use the Edit tool.

## HANDOFF追記 / Handoff (2026-09-19続き7) CPU版AI超解像・音声AI超解像の評価 / CPU AI upscaling and audio SR evaluation

**日本語**
- **CPU版AI超解像(`engine/cpu_sr.rs`)**: ncnn形式のReal-ESRGAN `realesr-animevideov3`(conv18層+PReLU+PixelShuffle+最近傍拡大の加算、x2/x3は後段に双三次縮小)を自前で読み込み・推論。
  重みはfp16タグ+アライン+float32バイアスで、**全バイトが過不足なく消費される**ことを実測で検証。128×128タイル+受容野の余白18で処理(タイル処理と一括処理が一致)、
  マルチスレッド。AVX2+FMAカーネルはopen-cpu(`avx2`/`fma`検出)で選択、スカラー実装が参照(AVX2との出力差<1e-3)。**実測(最適化ビルド、720×480→2880×1920、32スレッド)**:
  **1.16秒/フレーム**(GT 730 GPUの4.6秒より速い)。**公式ncnn-vulkan(GPU)との出力PSNR 42.0dB**(実画像、GPUはfp16計算)。`backend=auto`はGPUが実際に動くか小画像で確認し、
  動かなければCPUへ切替。`realesrgan-x4plus`(RRDBNet)はCPU版の対象外(GPU必須)。`image`クレートにjpegを追加(JPEG入力対応)。UIに実行環境の選択。
  **AVX-512カーネルは未実装**(open-cpuは`avx512f`を検出できるが、AVX2で十分速かったため後回し)。
- **音声AI超解像の調達と評価(結論: 採用しない)**: 調達先は`TigreGotico/audiosronnx`(Apache-2.0、ONNX集、HuggingFaceに重み)。LavaSR(Apache-2.0、52MB、CPUで実時間の約24倍速)、
  HiFi-GAN-BWE(MIT、4MB)、AP-BWE(MIT)、ノイズ除去のdpdfnet(Apache-2.0、48kHz)など。**Python 3.13は実在**(venvで`pip install audiosronnx`成功)。ただし音楽素材(12秒)を
  対数スペクトル距離(LSD)で評価すると、**LavaSRは元信号からの距離が悪化**(8kHzカット: 帯域制限のみ0.93→LavaSR 3.56、高域のみ1.13→4.36。16kHzカット: 0.008→3.56)。
  これらは音声で学習した「高域を生成する」モデルで、元に無い高域を作るため忠実度の指標では悪化し、しかも**ご提供の素材自体がロッシー(16kHz付近で帯域が切れる)で全帯域の正解が存在しない**ため
  「復元の正しさ」を検証できない。以上より、音質最優先の機能としては**採用しない**(ハルシネーションされた高域を「高音質」と称するのは不誠実)。将来、全帯域の正解データで
  改善が示せる音楽用モデルが見つかれば再検討。
- 全75テスト成功(約12分、デバッグビルドのAVX2/スカラーが遅いため)。clippyは既存の無関係な1件のみ。
- **未着手**: DSD書き出しのrs-ffmpeg化と比較検討(ユーザー要望)、音楽CD(CD-DA)、BD/DVD取り込み。

**English**
- **CPU AI upscaling (`engine/cpu_sr.rs`)**: loads the ncnn-format Real-ESRGAN `realesr-animevideov3` itself (18 conv + PReLU + PixelShuffle + nearest-upsample add; x2/x3 add a bicubic downscale) and runs it.
  The weights (fp16 flag + alignment + float32 bias) are verified to consume **every byte**. 128×128 tiles with an 18-px halo (tiled == whole-image result), multi-threaded. The AVX2+FMA kernel is
  chosen via open-cpu; the scalar path is the reference (max diff < 1e-3). **Measured (optimized build, 720×480 → 2880×1920, 32 threads): 1.16 s/frame** (faster than the GT 730 GPU's 4.6 s).
  **Output PSNR 42.0 dB against the official ncnn-vulkan (GPU)** on a real image (the GPU computes in fp16). `backend=auto` checks the GPU with a tiny image and falls back to the CPU.
  `realesrgan-x4plus` (RRDBNet) is GPU-only. JPEG input was added to the `image` crate; the UI has a device selector. **No AVX-512 kernel yet** (open-cpu detects `avx512f`, but AVX2 was fast enough).
- **Audio AI super-resolution: sourced and evaluated — decision: not adopted**. Source: `TigreGotico/audiosronnx` (Apache-2.0, ONNX collection with weights on HuggingFace): LavaSR (Apache-2.0, 52 MB, ~24× real time on CPU),
  HiFi-GAN-BWE (MIT, 4 MB), AP-BWE (MIT), the dpdfnet denoiser (Apache-2.0, 48 kHz) and more. **Python 3.13 exists** (`pip install audiosronnx` worked in a venv). On a 12 s music excerpt scored by log-spectral distance,
  **LavaSR moved *away* from the original** (8 kHz cutoff: 0.93 lowpassed-only → 3.56 with LavaSR; HF-only 1.13 → 4.36; 16 kHz cutoff: 0.008 → 3.56). These are speech-trained models that *generate* highs; on top of that
  **the supplied material is itself lossy (band-limited near 16 kHz), so there is no full-band ground truth** to verify a "restoration". Presenting hallucinated highs as "higher quality" would be dishonest, so it is **not offered**;
  reconsider only if a music model shows gains against full-band ground truth.
- All 75 tests pass (~12 min; debug-build AVX2/scalar is slow). Clippy: only the pre-existing unrelated warning.
- **Not started**: moving DSD writing into rs-ffmpeg with the requested comparison, audio CD (CD-DA), BD/DVD ripping.

## HANDOFF追記 / Handoff (2026-09-19続き8) 音声AI高域生成(改良型)のRust実装 / Rust implementation of the improved AI bandwidth extension

**日本語**(前項「音声AI超解像は採用しない」を**改良型で覆した**。根拠は下記の実測)
- **なぜ前回は悪化したか**: LavaSRの生出力は(a)低域まで再合成する、(b)生成する高域が実際の音楽より約12dB大きい。前回のLSDは元信号が存在しない16〜24kHzも含めて比較していて公平でなかった。
- **改良設計**(`engine/audio_sr.rs`): ①入力の帯域は一切変えない(出力=入力+生成した高域のみ)、②入力のカットオフを崖検出(平均パワースペクトルで、低域側800Hz平均と600Hz先の高域側800Hz平均の差が最大の位置、
  25dB以上の落ち込みが無ければ帯域制限なしとして素通し。単純な「基準から55dB下」のしきい値はHann窓のサイドローブ漏れ(-95dB付近)で不安定だったので廃止)、③生成した高域はフレームごとに
  「カットオフ直下[0.6fc,0.95fc)のlog10パワーを周波数に直線当てはめ→上向きには外挿しない→その外挿値」を各binの上限に頭打ち(STFT 2048/512)。
- **客観評価**(Python、正解の存在する帯域のLSD、4素材×カットオフ8k/12k): 無処理 3.3〜3.5 / 2.5〜2.8 → 改良型 1.25〜1.55 / 1.4〜1.6(**8/8ケースで改善**、生の出力は3/8のみ)、低域LSD 0.08。
  **Rust移植後の実モデル・実音源テスト**でも再現: fc=8k 3.46→1.59、fc=12k 2.82→1.60、低域の変化0.000。**変換パイプライン全体**(ffmpeg展開→帯域拡張→FLAC)の実音源テスト: 高域(10-16kHz)パワー約140倍、低域パワー不変。
- **tract移植の検証**: LavaSRのONNX(backbone 51.7MB+spec_head 4.2MB)がtract 0.21で読み込め、onnxruntimeとの出力差は相対3e-5/3e-6、3秒分を184ms。DSP(scipy互換の`resample_poly`(Kaiser β=5)、STFT/ISTFT(hann・boundary=zeros・spectrumスケーリング)、
  メルフィルタ80)をRustで実装(往復再構成・トーン精度・メルの単調性をテスト)。モデルはHuggingFaceの固定リビジョン(`b3df8a26…`)から初回のみ取得、プラグインフォルダ`audio-sr/`。処理は30秒区間(前後1秒の余白)でメモリを抑え、チャンネルごと(カットオフは共通)。
- **順序**: AIノイズ除去(RNNoise)→帯域拡張(逆順だと拡張器がノイズから高域を作る、audiosronnx作者の指摘)。`run_convert`で、ノイズ除去を展開時に適用し、後段の`ai_denoise`は無効化。音声専用出力/DSDのみ、カット区間は未対応。
- **正直な限界**: LSDはスペクトル包絡の近さの指標で聴感品質ではない。帯域拡張は復元ではなく合成で、ロッシー音源で捨てられた高域を本物として取り戻せない。カットオフ自動検出は緩やかな減衰(ffmpegの2次ローパス6段=72dB/oct)では落ち込み25.8dBとぎりぎり(手動指定欄あり)。
  16kHz付近で帯域が切れる一般的なロッシー音源では、生成できるのは16k以上のごく小さな成分で、効果は小さい。**推奨は「低ビットレート/電話品質/古い録音」など明確に帯域が欠けた素材向け**。
- **開発上の反省(再掲)**: node/perl経由の文字列置換でバックスラッシュが消える事故が再発(`C:\AUDIO`→`C:AUDIO`)。テスト用「雑音」を乗算ハッシュで作ると周期的な鋸歯波になり白色雑音にならない(xorshiftを使う)。

**English**(this **reverses** the previous "audio SR not adopted" with an improved design, backed by the measurements below)
- **Why it worsened before**: the raw LavaSR output (a) resynthesizes the low band and (b) generates highs ~12 dB louder than real music. The earlier LSD also included 16–24 kHz where the source has no ground truth.
- **Improved design** (`engine/audio_sr.rs`): (1) the input band is never modified (output = input + generated highs only); (2) the input cutoff is found by cliff detection (largest difference between the 800 Hz mean below and the 800 Hz mean 600 Hz above in the average power spectrum;
  no drop of ≥25 dB means "not band-limited" → pass-through; a plain "55 dB below reference" threshold was dropped because Hann sidelobe leakage (~ −95 dB) made it unstable); (3) generated highs are capped per frame and bin at a log-linear extrapolation
  of the input's envelope just below the cutoff (linear fit of log10 power over [0.6 fc, 0.95 fc), never extrapolated upward; STFT 2048/512).
- **Objective evaluation** (Python, LSD in the band with ground truth, 4 clips × cutoffs 8k/12k): unprocessed 3.3–3.5 / 2.5–2.8 → improved 1.25–1.55 / 1.4–1.6 (**better in 8/8 cases**, raw output 3/8), low-band LSD 0.08. **Reproduced by the Rust port with the real model and real audio**: fc=8k 3.46→1.59,
  fc=12k 2.82→1.60, low-band change 0.000; the **whole conversion pipeline** (ffmpeg decode → extension → FLAC) raised 10–16 kHz power ~140× with the low band unchanged.
- **tract port verified**: LavaSR's ONNX graphs (backbone 51.7 MB + spec_head 4.2 MB) load in tract 0.21 and match onnxruntime to 3e-5 / 3e-6 relative (3 s in 184 ms). The DSP (scipy-compatible `resample_poly` (Kaiser β=5), STFT/ISTFT, 80-band mel filterbank) is implemented in Rust. The model is fetched once from a pinned Hugging Face revision into the plugin folder `audio-sr/`.
- **Order**: AI denoise (RNNoise) first, then extension (the reverse makes the extender build highs out of noise). Audio-only/DSD outputs only; cut ranges unsupported.
- **Honest limits**: LSD measures spectral-envelope similarity, not perceived quality; this is synthesis, not restoration, and cannot bring back highs a lossy encoder discarded. Auto cutoff detection is marginal on gentle roll-offs (a 72 dB/oct filter gave a 25.8 dB drop; a manual field exists).
  For typical lossy sources cut near 16 kHz the effect is small. **Recommended for clearly band-limited material (low bitrate / telephone / old recordings).**
- **Not started / queued**: DSD writing in rs-ffmpeg with the requested comparison; DSD quality checklist (out-of-band noise, headroom, segment-parallel SIMD); audio CD (CD-DA) ripping and burning; SACD/Blu-ray-audio images.

## HANDOFF追記 / Handoff (2026-09-19続き9) 容量チェック・3層BD・SACD風プリセット・MQAの扱い / Capacity check, 3-layer BD, hi-res disc preset, MQA decision

**日本語**
- **ディスク**: `DiscType::Bd100`(BDXL 3層100GB)を追加(BD 1層25/2層50/3層100/4層128GB、DVD 1層4.7/2層8.5GB)。`folder_size_bytes`/`disc_usable_bytes`コマンドを追加し、ISO作成後・書き込み前に
  出力サイズが各ディスクの実用容量(公称の約98%)に収まるかを確認、**収まらない種別は書き込みを飛ばして日英で理由を表示**(以前は容量超過でも書き込みを始めて途中で失敗していた)。
- **SACD/Blu-rayオーディオ風プリセット**(ボタン1つでDSD256+384kHz/32bit PCM+ISO): 標準のSACD/Pure Audio BD規格ではない(SACDは独自暗号化・認定オーサリング、Pure Audio BDは192kHz/24bitまで)。
  DSF/WAV/FLACを収めたデータディスクで、PC・Android・ネットワークプレーヤーでの再生が目的(ユーザー合意済み)。映像+音声は、音声部分をDSD/PCMにし映像は別ファイルとして同じディスクへ。
- **MQA**: **実装しない**。MQAのエンコード/デコード(折り紙)は特許・営業秘密で、互換実装は特許侵害のリスクがあり、可逆でもない。**以前ユーザーとAIで作った`aon-co-jp/open-mqa`が実在**し、同じ結論
  (「MQA互換ではなく、FLAC・DSDなど既存オープン規格を土台にした独自パイプライン」、FLAC往復+DoPパッキング実装済み、11テスト)。make-diskの高解像度出力(384kHz/32bit FLAC/WAV、DSD)はこの方針に沿う。
  **今後の連携候補**: open-mqaのDoP(DSD over PCM)をDSDのエクスポート形式(DoP-FLAC/WAV)として取り込む(DoP対応DACのみ有効で、DSD非対応DACへのフォールバックではない点に注意)。
- 全82テスト成功(約13.6分)、clippyは既存の無関係な1件のみ。

**English**
- **Discs**: added `DiscType::Bd100` (BDXL 3-layer, 100 GB) (BD 25/50/100/128 GB for 1–4 layers, DVD 4.7/8.5 GB). New `folder_size_bytes` / `disc_usable_bytes` commands let the app check, after the ISO is built and before burning, whether the output fits each
  disc's usable capacity (~98% of nominal); **types that do not fit are skipped with a bilingual explanation** (previously it started burning and failed midway).
- **SACD / Blu-ray-Audio-style preset** (one button: DSD256 + 384 kHz/32-bit PCM + ISO): not the standard SACD or Pure Audio BD spec (SACD needs proprietary encryption and licensed authoring; Pure Audio BD stops at 192 kHz/24-bit). It is a data disc of DSF/WAV/FLAC files for PC, Android and
  network players (agreed with the user). For video+audio, the audio part becomes DSD/PCM and the video goes on the same disc as separate files.
- **MQA**: **not implemented**. MQA's encode/decode ("origami") is patented/trade-secret, a compatible implementation risks patent infringement, and it is not lossless. **`aon-co-jp/open-mqa`, an earlier user+AI project, exists** and reached the same conclusion ("not MQA-compatible; a separate pipeline on
  open formats such as FLAC and DSD", with FLAC round-trip and DoP packing implemented, 11 tests). make-disk's hi-res outputs (384 kHz/32-bit FLAC/WAV, DSD) follow that line. **Possible follow-up**: take open-mqa's DoP (DSD over PCM) as a DSD export (DoP-FLAC/WAV) — it only works with DoP-capable DACs and is not a fallback for DACs without DSD.
- All 82 tests pass (~13.6 min); clippy shows only the pre-existing unrelated warning.

- **2026-09-19続き10 / Continued 10**: 音楽CD(CD-DA)取り込みを追加(`engine/cdda.rs`、Windows IOCTL_CDROM_READ_TOC/RAW_READ、簡易セキュアリード=2回読み一致確認、WAV出力、UIは「1. ソースファイル」内)。TOC解析・セキュアリード・WAV書き出しはモックで単体テスト済み、実機ドライブのTOC取得も確認(データCDを「音声0本」と正しく判定)。**実際の音楽CDでの取り込みE2Eは未実施**(手元に音楽CDが無い)。保護回避・曲名取得・Linux/macOSは非対応。 / Added audio-CD ripping (Windows, simple secure read, WAV). Unit-tested with mocks; real-drive TOC verified on a data CD; **no real audio-CD rip E2E yet**. No protection circumvention, no title lookup, no Linux/macOS.

- **2026-09-19続き11 / Continued 11**: (1) **open-mqa融合**: `open-mqa`(git依存、`default-features=false`)のWAV/DoPを利用し、`dsd::dsf_to_dop_wav`でDSF→DoP WAV(24bit標準/32bit左詰め、`<出力>.dop.wav`)を書き出せるようにした(UI: 「3. 音声」のDoPチェック+ビット深度)。テスト: DoPのビット列が完全保存、32bit=24bit<<8。384kHzのDoPは48kHz系DSDの入れ物で、本ツールのDSD(44.1kHz系)とは一致しない(DSD128=352.8kHz)ことをUIに明記。(2) **DSD区間並列化は不採用(実測で有害)**: 時間方向に区間分割し先頭をウォームアップする方式を実装したところ、実機E2EでDSD64のSNRが99.6→51.3dB、DSD128が132.4→53.9dBへ劣化した。原因はNTFが低域に高次の零点を持つため、区間境界で雑音の低域積分状態が食い違うこと(ウォームアップでは直せない)。**逐次版とビット完全一致**を保つ方針に戻し、チャンネル並列のままにした(最適化カーネルと参照実装のビット完全一致テストを追加)。速度は既に全レート実時間以上(DSD512=1.9倍速、DSD1024=3.7倍速、2秒クリップ実測)なので区間並列は不要。区間横断のSIMDも同じ理由で不可(複数ファイル同時変換のSIMD化は将来課題)。 / open-mqa fused for DoP WAV (24/32-bit). Segment-parallel DSD was tried and rejected: measured SNR fell 99.6→51.3 dB (DSD64) because splicing independent modulator trajectories breaks the shaped noise's low-frequency integral state; reverted to bit-exact sequential per-channel threads (all rates already ≥ real time).

- **2026-09-19続き12 / Continued 12**: (1) 音楽CD(CD-DA)取り込みを**実機の音楽CD(11トラック)で検証**(TOC・セキュアリード・最終トラック123秒を8.4秒で取り込み、ffprobeで44.1kHz/2ch/16bitの正常WAV確認)。コピーコントロール付きCDは未検証。(2) 取り込んだ実音源をDSD256+DoP WAVへ変換: DSF 349.6MB(89秒、約1.4倍速)、DoP WAV(24bit)524.5MB。ffmpegでPCMへ戻すと元と相関0.99999、ゲインは約2.0(=DSDは入力を0.5倍に抑えて変調するため、DSD再生は原音より約6dB小さい。SACD慣行のヘッドルームで、再生側の音量で補う)。(3) Opusの仕様上限に合わせた: ビットレートはステレオ最大510kbps(3ch以上は1chあたり256kbps)へ丸め、入力は48kHzへ(`convert::apply_opus_limits`)。ビットレートはVBRで目標平均として必要に応じて変動。 / Real-disc CD rip verified; real track converted to DSD256 + DoP WAV (DSD plays ~6 dB below source by design); Opus now capped at 510 kbps stereo / 48 kHz.

- **2026-09-19続き13 / Continued 13 — 音楽向けAIノイズ除去の調査結果(不採用・記録)**: `eloimoliner/denoising-historical-recordings`(MIT、クラシック音楽+レコードノイズで学習、44.1kHz)を評価した。**TensorFlow 2.3のKeras**モデルで、チェックポイントは約286MB(数千万パラメータ、複素STFT領域の2段U-Net+密結合ブロック+SAM注意機構+周波数位置埋め込み、対称パディング)。ONNX変換にはPython 3.8+TF2.3環境が要るが開発機はPython 3.13/3.14のみ(TF2.3不可)。手動でRustへ移植するにしても、この規模のモデルをCPUで回すと実時間より大幅に遅くなる見込み(未実測の見積り)で、実用性に疑問が残るため**採用を見送った**。音声用(RNNoise、任意で他)のみ現状維持。再検討する場合は、より小さい音楽向けモデルの登場を待つか、Python 3.8環境を用意してONNX化し実測してから。 / Evaluated the historical-recordings U-Net (MIT, music-trained): TF2.3 Keras, ~286 MB checkpoint, no ONNX path on this machine's Python and likely far slower than real time on CPU (estimate, not measured) → not adopted.

- **2026-09-19続き14 / Continued 14 — 実機E2E(CD→DSD256→CD-R→再生)**: コピーコントロールCDの全11トラックをセキュアリードで取り込み(2回の取り込みで全トラック一致)→1曲(Track04、DSD256で682.9MB、CD-R実用容量に収まる唯一の曲)をDSD256のDSFへ変換→ISO(682,921,984バイト)→IMAPI2でCD-Rへ書き込み成功(226秒)。**再生**: foobar2000は標準ではDSFを再生できず無音になる(標準コンポーネントのみ)。SourceForge公式の`foo_input_sacd` 2.0.25(x64)を入れると**音が出た**(ユーザー確認)。ffmpeg系プレーヤー(VLC等)はDSD256を1.4112MHzのPCMとして復号するため無音になり得る。DSD対応プレーヤー+(DSD対応DACまたはPCM変換出力)が必要。 / End-to-end verified: protected CD rip → DSD256 → CD-R burn → playback OK in foobar2000 with foo_input_sacd (plain foobar2000 is silent on DSF).

- **2026-09-19続き15 / Continued 15 — v0.1.21の実バグ修正**: 実機報告「`ReferenceError: dsdMatch is not defined`」。ジョブ生成ループ内の`const dsdMatch/hiresMatch`を、別スコープの変換タスク内で参照していた(DSD/ハイレゾ選択時に必ず失敗。Rust側のテストでは検出できないフロントエンド側の不具合)。変換タスク内でformatから再判定するよう修正。再発防止として(1)`eslint.config.mjs`+`npm run lint`(no-undef)を追加し、main.js全体を検査(他に未定義変数なし)、(2)Tauri APIをスタブしたページをブラウザで実際に操作し、DSD256+384kHz/32bit PCM+DoP指定で`dsd_rate:256, dop_wav_bits:24`のジョブが生成されエラーが出ないこと、MP3のみ選択時はMP3ジョブだけになること(初期状態で選択済みの形式は無し)を確認。実行前に「選択中の出力形式」をログへ表示するようにした(意図せず選択されたプリセットに気づけるように)。ユーザーのログにDSDが含まれていたのは、「DSD256+384kHz/32bit PCM+ISOを一括設定」プリセットボタン(6.ディスク書き込み節)が押された状態だった可能性が高い(コード上、他に自動でDSD形式を選択する経路は無い)。 / Fixed a frontend ReferenceError (dsdMatch out of scope) in convertAll tasks; added eslint no-undef lint and a stubbed-UI browser check; selected formats are now logged before running.

- **2026-09-19続き16 / Continued 16 — v0.1.23**: (1) 「DSDと同時にPCM版も作る」を既定オフに(DSD対応の再生ソフトは自動でPCM変換でき、容量も倍近く使うため。ハイレゾプリセットでも付けない。必要なときだけチェック)。(2) Android APKの肥大(v0.1.20 728MB→v0.1.21 1.58GB)への対策として`[profile.dev] debug = "line-tables-only"`を追加(CIのAndroidは`--debug`ビルドで全ABI分のデバッグ情報が入っていたため。効果は次回CIのAPKサイズで確認する)。v0.1.22のWindows版(MSI展開)は起動とバージョン0.1.22を確認。 / v0.1.23: PCM companion off by default; slimmer debug info to shrink the Android APK.

- **2026-09-19続き17 / Continued 17 — v0.1.24**: 分割UIの日本語を分かりやすく改善(「同じ長さに分ける: N個のファイルに」「大きさで分ける: 1ファイルあたり約NMBずつ」等)。分割数は0=分割しない、1は「分けない」と同じなので選べず、▲▼で0↔2と1を飛ばす(手入力の12等はそのまま)。ファイルが指定サイズ以下で分割不要な場合は理由を表示。音楽CD取り込みUIはAPIスタブの画面で操作確認(トラック選択・取り込み引数・ソース追加)。 / Clearer split wording; count skips 1 (0 = no split, arrows 0 ↔ 2).

- **2026-09-19続き18 / Continued 18 — v0.1.25**: 「DSD256 + ISOを一括設定」プリセットがPCM(384kHz/32bit)まで自動チェックしていたのを修正(DSD変換時はPCMを同時に作らない。PCM版companionも既定オフ)。v0.1.24をユーザーPCへインストール(`%LOCALAPPDATA%\make-disk`、NSISは`/S /D=`指定が必要——指定しないと前回のインストール先=テスト用Tempに入る)。 / DSD256 preset no longer ticks PCM.

- **2026-09-19続き19 / Continued 19 — MKVの複数音声・字幕トラック対応**: `engine/mkv_tracks.rs`新設。(1) 出力が`.mkv`のとき既定で元ファイルの全映像/音声/字幕/添付を保持(`-map 0:v? 0:a? 0:s? 0:t?` + `-c:s copy`。ffmpegは`-map`省略だと各1本しか選ばないため二重音声・複数字幕が失われていた)。(2) 別ファイルの音声(wav/flac/mka/ac3等)・字幕(srt/ass/vtt/sup等)を、言語コード(jpn/eng…)・タイトル付きの追加トラックとして多重化(`ConvertJob.extra_tracks`、追加入力の`-i`は出力側`-t`より前に置く、トリミング時は各入力に同じ`-ss`)。UI: 「4.4. MKVの複数音声・字幕トラック」(全保持チェック+音声/字幕追加+言語・タイトル入力)。追加トラックはソース先頭ファイルのMKV出力のみに付く。実ffmpegのテスト2本(既定で音声2+字幕1が残る/オフで既定に戻る、追加音声(fra,Commentary)+字幕(eng)で音声3+字幕2・言語タグ確認)+単体5本が通過。スタブUIの操作でMKVジョブにだけ追加トラックが付きMP4には付かないことも確認。限界: AI超解像の中間ファイル経由やカット区間の結合経路では字幕・追加トラックは引き継がれない。 / MKV: keep all source audio/subtitle/attachment tracks by default; mux extra audio/subtitle files with language/title.

- **2026-09-20続き20 / Continued 20 — open-bar / open-mqa-dsd 新設と連携方針**: ユーザー依頼で`aon-co-jp/open-bar`(foobar2000をリスペクトした高音質・高画質プレーヤー、公開)と`aon-co-jp/open-mqa-dsd`(DSF/DSDIFF読み込み・DSD→PCM・DoP、公開)を新設(ローカル`F:\open-bar`・`F:\open-mqa-dsd`)。「MP4動画+DSD音声」などの自由な組み合わせは、MP4/MKVにDSDの入れ場所が無いため**別ファイルのまま組み合わせ、音声を時間の基準に**する(`open-bar`の`Combo`=`.obar.json`)。make-disk側の残作業: 「動画+DSD音声」を書き出す際に映像(MP4等)+`.dsf`+`.obar.json`をセットで出力する機能、ΔΣ変調器(`engine/dsd.rs`)を`open-mqa-dsd`へ切り出し。 / Created open-bar (player) and open-mqa-dsd; video+DSD combos are kept as separate files linked by .obar.json; make-disk should export such sets next.

- **2026-09-23続き21 / Continued 21 — 区間カットの操作不能バグ修正・DSD時PCM廃止・カット/規格上限の再設計**:
  (1) 「8. 時間指定・トリミング」節に説明文しか無く、編集UIは「1.」のファイル一覧内にしか開かなかったため8.から操作できなかった実バグを修正(8.に対象ファイル選択+編集欄を常設、音声も`<audio>`でプレビュー・「現在位置」取り込み可)。
  (2) DSD作成時はPCMを一切同時に作らない仕様に変更(PCM companionのチェックと処理を削除、DSDと同時に選ばれたWAV/FLAC/高解像度PCMは自動除外。DoP WAVはDSDデータなので対象外)。
  (3) 7.5を再設計: 「サイズでカットする？」「時間でカットする？」をYES/NOの排他(必ず一方がYES、既定はサイズ、元データのサイズ基準で先頭から残す)。後処理として「ディスクいっぱいに収める」「AI無音カット」をチェックボックス化。旧「サイズ指定でビットレートを下げる」方式は削除。
  (4) 7.6「再生規格の上限」を新設(CD 44.1kHz/16bit・DVD-Video 96kHz/24bit 映像9.8Mbps・DVD-Audio 192kHz/24bit・Blu-ray 192kHz/24bit 映像40Mbps・UHD BD 映像100Mbps・PC専用 768kHz/32bit上限なし)。元が上限以下ならアップサンプリングしない判定のため`MediaInfo.audio_sample_rate`を追加。
  (5) DVDでのフルHDは、DVD-Video規格(最大720×480/576)外で家庭用DVDプレイヤーは自動で解像度を落として再生しないため、現状の注意書きを維持(DVD-Video+フルHDファイル同時収録はユーザー判断で不採用)。
  (6) README/CLAUDE/PORTINGを多言語化(`README/`・`CLAUDE/`・`PORTING/`フォルダに英・簡体中文・繁體中文・韓・独・仏・露・ウクライナ語・ペルシャ語(イラン、ファイル名は`.iran(Perusha).md`)・アラビア語。CLAUDE/PORTINGは要約版、日本語が正本)。
  検証: スタブUIのブラウザ操作(8.のカット追加・YES/NO排他・規格一覧表示)、`cargo test --lib probe`成功。**実ファイルでの変換E2E(カット位置・-ar・ビットレート上限)は未実施**。 / Fixed the inoperable cut editor, dropped PCM alongside DSD, redesigned cut-by-size/time + fill-disc + playback-spec limits, added multilingual docs.

- **2026-09-23続き22 / Continued 22 — 出力先フォルダの検証**: ソース元と出力先は必ず別フォルダ(Windowsは大文字小文字・区切り文字の違いを無視して比較)。出力先選択時に同じフォルダなら選ばせない、出力先と同じフォルダのファイルはソースに追加しない、実行系ボタン(実行・結合・分割・PDF綴じ変換・CD取り込み)は出力先未選択/同一フォルダなら実行しない。警告は日英併記でダイアログ(`window.alert`)とログの両方に出す(`requireValidOutputFolder`)。スタブUIのブラウザ操作で4ケース確認。 / Source and output folders must differ; missing output folder blocks running; bilingual alerts.

- **2026-09-23続き23 / Continued 23 — 再生と同時の変換でも他アプリを妨げない**: ユーザー要望「ブルーレイを再生しながらMP4→CD ISO変換してもBUGにならないように」。外部プロセス(ffmpeg/ffprobe/rs-*/Real-ESRGAN)は`sidecar::background_command`でWindowsの優先度「通常以下」(BELOW_NORMAL_PRIORITY_CLASS)+CREATE_NO_WINDOWで起動、アプリ内の重いワーカースレッド(DSD変調・CPU版AI超解像)は`lower_current_thread_priority`(SetThreadPriority BELOW_NORMAL)。マルチスレッドは維持し、他アプリがCPUを要する瞬間はそちらを優先する。関連テスト31件成功(DSD全レート速度テストは並列実行時のみ時間超過で失敗、単独では成功)。
  同時に報告された「MP4→CDでWAV 108KB・ISO 160KB」は、現行コードの既定設定では再現せず(スタブUIのジョブは正常、実ffmpegで60秒MP4→WAV 10.6MB)。ユーザーのインストール版はv0.1.25で、本日の修正は未リリース。原因特定にはユーザーのログ欄の内容と元MP4の情報が必要。 / Background priority for all heavy work; the 108 KB WAV report could not be reproduced yet.

- **2026-09-23続き24 / Continued 24 — v0.1.26: MP4→CDでWAVが0.6秒になる実バグ修正**: ユーザー報告「MP4(1.89GB・3時間33分)→CDでoutput.iso 160KB・WAV 108KB」。ユーザーPCの実ファイル(AV1+AAC 44.1kHz、読み取りのみ)で原因を特定し、**アプリと同じ手順で0.626939秒・110,670バイトを完全再現**。原因: カット区間ありの変換(`run_convert_with_cut_ranges`)が、中間の区間ファイルを**出力と同じ拡張子(.wav)**で`-c copy`抽出していたため、AACを無変換でWAVの入れ物へ詰めてフレーム境界が壊れ、結合時にほぼ復号できず(ffmpeg exit 69)先頭0.6秒で止まっていた。さらに画面側が変換失敗後もISO作成・書き込みへ進んでいた。修正: (1) 中間区間は常にMatroska(.mkv)、(2) 音声だけの出力では区間抽出・結合とも`-vn`、(3) 変換が1件でも失敗したらISO作成・書き込みを中止し日英で警告。検証: 実ファイルを先頭4000秒でカット→WAV 4000.0秒・705.6MB(6.9秒)、回帰テスト`real_ffmpeg_cut_from_mp4_to_wav_keeps_the_expected_duration`追加。本日の続き21〜23(区間カットUI修正・DSD時PCM廃止・サイズ/時間カットYES/NO・再生規格上限・出力先フォルダ検証・低優先度実行・多言語ドキュメント)もv0.1.26に含めてリリース。 / v0.1.26: fixed the 0.6-second WAV bug (stream-copying AAC into .wav intermediates), abort ISO/burn after any failed conversion, plus today's changes.

- **2026-09-23続き25 / Continued 25 — 「必要な部分だけ切り出す(高速)」**: ユーザー要望「元が5時間でも欲しいのは60分・10分だけ、編集に5時間掛けたくない」。8.の編集欄に開始・終了を指定する切り出しを追加(`SourceFile.extract`)。変換ジョブでは`trim`(入力側シーク`-ss`+`-t`)として渡し、開始位置へ直接シークして必要な長さだけを読むため、処理時間は切り出す長さにほぼ比例する。切り出し設定時はカット区間を使わない(実行時にログで通知)。7.5のサイズ/時間カットも同じ高速な切り出しに変更(以前はカット区間=`-c copy`分割+結合で、DSD出力では未対応だった)。実測(ユーザーの3時間33分のMP4→WAV): 2時間地点から10分=1.0秒、1時間地点から60分=4.9秒、長さは正確。 / Fast extract of only the needed range (input seek), also used by 7.5 size/time cutting.

- **2026-09-23続き26 / Continued 26 — 切り出しの再エンコード高速化(GPU・open-cpu)とAIによる範囲提案(aruaru-llm)**:
  (1) ユーザー要望「open-directx・open-cuda・aruaru-llm・open-cpuを必要なだけフル動員」。**正直な評価**: 切り出し自体はシーク+読み込み(ディスクI/O)で既に10分=約1秒。時間が掛かるのは切り出し後の再エンコードなので、`trim`指定時は`-c:v libx264`をGPUのハードウェアエンコーダ(NVENC→QuickSync→AMF、試しエンコードで動くものだけ、結果はプロセス内キャッシュ)へ置換し、GPUが無ければopen-cpuの判定から速度優先の`-preset`(`cpu::fast_x264_preset`)を付ける(`convert::accelerate_h264_args`)。open-cuda/open-directxは動画コーデック実装を持たないため、この処理の高速化には使えない(形だけの組み込みはしない)。
  (2) **AIで探す**(`engine/ai_range.rs`、コマンド`ai_suggest_range`、非同期で画面を固めない): 欲しい内容の文章と長さから切り出し範囲を提案し、8.の切り出し欄へ反映(候補3つから選び直し可)。手掛かりは①同名の字幕ファイル(.srt/.vtt、同じフォルダか1つ上)②埋め込み字幕③無ければaruaru-llm`/v1/transcribe`で1分ずつ書き起こし。候補は文字2-gramの一致で上位3件(重ならない)、最終選択はaruaru-llm`/v1/generate-qwen`(Qwen2.5)。aruaru-llmに接続できなければ文字の近さだけで選び、その旨を表示。URLは既定`http://127.0.0.1:4600`(画面で変更可、localStorageに保存)。**映像の中身は見ていない**(話している内容のみ)。音楽中心の動画は自動字幕が`[Music]`等だけのため内容で探せない。実測(ユーザーの58分のトーク動画+字幕、aruaru-llm未起動): 「エリア51の秘密」→冒頭10分(点数91、2位15)、約2.3秒。
  (3) 帯域拡張の実音源テストは、評価用素材(C:\AUDIOの…(1).mp4)が差し替わり、新しい話し声の動画や10分未満の動画が選ばれて失敗していた。最も古い素材を使い、10分未満ならスキップするよう修正(今日のコード変更とは無関係)。

- **2026-09-24 / Continued 27 — アップコンバートしてディスクいっぱいに収める**: ユーザー要望「DVD 1〜2層からBlu-ray 1〜4層まで選択可能で、フルHDか4Kにアップコンバートし、ディスク容量いっぱいに自動で収める」。6.に専用の設定欄(ディスク1種+フルHD/4K→「この設定にする」でディスク種別・MKV・解像度・ディスクいっぱい・ISOをまとめて設定)を追加。既存の実装では容量いっぱいにならない問題が3つあったので修正: (1) 「ディスクいっぱい」の動画は元ファイルのビットレートで頭打ちにしない(DVD由来の6Mbps等で止まり、ディスクがスカスカになっていた)、(2) 同時に作る動画の本数で割り、音声(AAC 320kbps)と2%の余裕を差し引く、(3) 画質がそれ以上上がらない規格上限(フルHD 40Mbps、4K 100Mbps)で止め、空き容量が残る旨を表示。さらに**1パスのlibx264は指定より6〜7%大きくなりディスクに収まらない**ことを実測(DVD画質60秒→フルHD/4K、目標100MBで105.8/107.2MB)したため、容量から逆算したビットレートのソフトウェアエンコード(libx264/libx265)は**2パス**にした(`needs_two_pass`/`run_two_pass`、統計ファイルは一時フォルダで後片付け)。実測: 2パスで98.6%(所要約1.3倍)、回帰テスト`real_ffmpeg_two_pass_fills_the_target_size_without_overflow`で95.8%(超過しないこと・85%以上を確認)。スタブUIで DVD2層+フルHD=8,749kbps、BD1層+フルHD=26,357kbps、BD3層+4K=100Mbpsで上限、を確認。

- **2026-09-24続き28 / Continued 28 — v0.1.28とインストール版での確認方法**: 「拡大の方法: 補間による拡大です。細部を補いたい場合は、「4.3 AI超解像」も一緒にチェックしてください。」を日英の独立した注意書きにし、「この設定にする」でもログに出すようにしてv0.1.28をリリース。
  **インストール版の画面を確認する方法(このセッションはデスクトップ操作不可のため)**: 環境変数`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`を付けてインストール済みの`make-disk.exe`を起動すると、`http://127.0.0.1:9333/json/list`にページ(`http://tauri.localhost/`)が出る。Node.jsの組み込み`WebSocket`でCDPの`Runtime.evaluate`を送れば、実際の画面のDOMを読み取り・操作でき、`window.__TAURI__.core.invoke`でアプリ自身のRustコマンドも呼べる。これで v0.1.28 のアップコンバート欄(6種のディスク・日英注意書き・設定の一括反映)と、アプリの`convert_media`で3時間33分の実動画から60分切り出し→WAV 3600秒・635MB(8.6秒)を確認した。デバッグポートはこのPCのローカルのみ・確認後にアプリを終了。
  なおTauriは画面ファイルを圧縮して実行ファイルへ埋め込むため、exeを文字列検索しても画面の文言は見つからない(問題ではない)。 / v0.1.28; how to verify the installed app's UI via WebView2 remote debugging + CDP.

- **2026-09-24続き29 / Continued 29 — AI超解像の作り直し・CPU/GPU自動選択・RIFE補間・収まる予測**:
  - **`engine/ai_video.rs`(新)**: ストリーミング(生RGBをパイプ)・240コマごとに確定して再開可能・コマ数の上限なし。インターレース/テレシネ判定(idet)、黒帯検出(cropdetect)、BT.601→709、DAR維持、単色/静止コマの再利用、残り時間、中止(`progress.rs`、イベント`make-disk-progress`)。
  - **`engine/hw_bench.rs`(新)**: このPCでCPU/GPU/併用を実測し(結果は保存、CPU構成・ビルド種別が変わると再測定)、1.08倍以上速い方式を選ぶ。**このPC(GT 730)の実測: CPU 1.27秒/コマ、GPU 2.76秒+起動1.6秒、併用で約1.3倍**。`engine/sr_pool.rs`がCPU・GPUの作業を分担(GPUは複数コマをまとめて起動の固定費を割る)。
    open-cudaには畳み込み演算が無いので、AI計算そのものは自前CPUカーネル(open-cpuで命令セット検出)とNCNN-Vulkanで行い、open-cuda/open-directx/aruaru-llmは速度向上には寄与しない(GPU検出・範囲検索の補助のみ)と正直に開示した。
  - **`engine/rife.rs`(新)**: RIFE(20221029、rife-v4.6)。配布ZIP約411MBのうち約12MBだけHTTP Rangeで取得。区間ごとに確定して再開可能。**GT 730では既定の`-j 1:2:2`で黒画面(エラー無し)になる**ため`-j 1:1:1`で動かし、出力の明るさを検査して壊れた区間はコマの繰り返しに置換。完全静止の区間はRIFEを省略。実機テスト(10fps→20fps、60コマ)合格。
  - **`engine/fit_predict.rs`(新)**: ディスク容量とbpp(fps^0.6で増加)から◎○△▲×を判定、規格上限(フルHD 40/4K 100Mbps)で頭打ち、blackdetect/freezedetectで静止割合を抜き取り推定。UI(6.の「収まるか予測」)から`ai_fit_predict`で呼ぶ。
  - **バグ修正(実機テストで発見)**: GPUのみのモードで最後の1コマが処理されずハング/回収側が切断を無視、GPU出力が黒くなる問題(出力検査+双三次へ置換)、デバッグビルドの測定結果が保存されて誤選択(署名にビルド種別を追加)。
  - **未実施・制限**: 汎用モデルrealesr-general-x4v3は未同梱、映画1本規模の通し実行は未検証(数日規模)、4KでのRIFE速度は未測定、複数本の合計での収まる予測は未対応(1本目のみ)、`realesrgan-x4plus`はGPU必須。
  - **次回**: open-easy-webのインストール配置(`%LOCALAPPDATA%\open-easy-web\`)、LLMマネージャ(NPUは後回し)、ローカルopen-web-server、easy-web.tokyo連携と`make-disk://`起動。

- **2026-09-25続き30 / Continued 30 — open-easy-webのインストール配置(v0.1.30)**:
  - 一番上を`%LOCALAPPDATA%\open-easy-web\`にし、`make-disk\`(本体とプラグイン)・`aruaru-llm\`・`open-web-server\`・`open-cpu\`・`open-directx\`・`open-cuda\`を置く(ユーザー選択: make-diskだけ移動、ライブラリも独立フォルダを作る)。macOS/Linuxはデータフォルダ直下の`open-easy-web`。
  - **中身の無いフォルダを実体があるように見せない**: 各フォルダの`component.json`に状態を書く(`planned`=未実装で予約: aruaru-llm・open-web-server、`embedded`=make-disk/aruaru-llmに組み込み済みで単独の実行物なし: open-cpu・open-directx・open-cuda)。
  - `engine/layout.rs`(新): 起動時に配置を整え、旧配置`%LOCALAPPDATA%\make-disk\plugins`を**移動**(取得済みのAI/RIFE/測定結果を再取得させない)。`plugins::plugin_dir()`は新配置を指す(`MAKE_DISK_PLUGIN_DIR`での上書きは従来どおり)。
  - NSISの既定のインストール先を`open-easy-web\make-disk`へ: 公式テンプレートを`installer/installer.nsi`にコピーして2か所だけ変更し(`tauri.conf.json`の`bundle.windows.nsis.template`)、**旧配置からの更新も新配置へ移す**。Tauri CLIを上げたら、テンプレートも同じ版から取り直して差分を当て直すこと。
  - 実機確認: ローカルでNSISインストーラーを作り、旧配置の環境へ上書きインストール→新配置に本体が入り、初回起動でプラグイン(realesrgan・rife・audio-sr・測定結果)が移動し、5つのフォルダと`component.json`ができ、旧プラグインフォルダは消えた。旧配置の本体(手動でインストールした古い分)は残るため、不要ならユーザーが削除する。
  - **次回**: LLMマネージャ(推奨/一段大きい/一段小さい、NPUは後回し)→ローカルopen-web-server(aruaru-llmのゲートウェイ)→easy-web.tokyo連携と`make-disk://`起動。

- **2026-09-25続き31 / Continued 31 — 未検証だった点の検証と補強(v0.1.31)**:
  - **4KのRIFE実測(GT 730)**: 補間コマ1枚あたり、フルHDはGPUで約4.2秒。**4KはGPUが`vkQueueSubmit/vkWaitForFences failed -4`(デバイス喪失)で一部のコマが黒くなる**(23枚中4枚)。CPUなら正常で約31〜46秒/枚。従来の出力検査は2コマだけだったため見逃す恐れがあり、**全コマを前後の元コマの明るさと比較する検査**に変更。壊れたらその区間をCPUでやり直し、以降の区間は最初からCPU(`rife.rs`)。`rife::speed()`で大きさごとの速さを測って保存(`rife-bench.json`)、「下調べ」に補間の所要時間と、GPUが使えない場合の警告を出す。
  - **4K・120fps通し変換**: DVD相当(720×480、24fps)→3840×2160・120fps・音声つきが成功(4コマ、258秒、CPUへ自動切替)。`convert.rs`の`real_dvd_to_4k_120fps_end_to_end`と`real_dvd_to_full_hd_60fps_with_ai_and_rife`(`--ignored`)。
  - **長時間検証**: `ai_video.rs`の`real_long_run_cancel_and_resume`(`--ignored`): 2分(3596コマ)のDVD相当→フルHD。約15分で中止→再開し、出力3596コマ(一致)、作業フォルダは完了後に消える、中止時の途中経過17MB、全体6408秒(約1.8秒/コマ)。映画1本(約17万コマ)は約3.5日の見込みだが、**通しの実行はしていない**。
  - **実写モデルrealesr-general-x4v3を同梱**(`src-tauri/models/`、BSD-3-Clause、Real-ESRGAN): 公式のncnn配布物に無いため、公式の`.pth`2つ(general/wdn)をノイズ除去0.5(公式既定)で混ぜてncnn形式へ変換(`scripts/convert_general_model.py`、torch不要)。`realesr-general-x4v3`のCPU出力は公式GPU実装(NCNN-Vulkan)と**PSNR 46.5dB**で一致(`hw_bench.rs`の`real_general_model_cpu_matches_the_official_gpu`)。「自動」は実写→実写向け、アニメ→高速モデル。ノイズ除去の強さの変更UIは無い(0.5固定)。
  - **AI超解像オフ+fps指定**: 超解像なしのRIFE補間だけを行う(`AiUpscale.interpolate_only`、`ai_video::make_interpolated_mezzanine`)。以前のコマ複製はやめた。**aruaru-llmは言語モデルで映像のコマを作れないため、動きを滑らかにするのはRIFE**であり、aruaru-llmは関与しない旨をUI・READMEに正直に書いた。
  - **リリースの整理**: v0.1.30のCIは4ジョブともビルドに成功したが、リリース作成だけ`Resource not accessible by integration`で失敗したため、手元からリリースを作成して失敗ジョブを再実行して公開した。APKだけ旧名だったので`make-disk_<版>_android-universal.apk`へ統一(ワークフローも修正、v0.1.31から自動)。
  - **未実施・制限**: 映画1本の実際の通し実行、4Kでのフレーム補間を映画規模で行うこと(このPCでは現実的でない)、補間の品質(動きの大きい場面でのゴースト)の定量評価、ノイズ除去の強さの調整。

- **2026-09-28 — GUI改善3件・AIアップミックス(HT-Demucs)新設**:
  - **色分けUI/時間指定フォーム統合/見積もり音声対応/panフィルタ疑似サラウンド**:
    前セッションで実装済み(このファイルの前の版を参照。8.1/8.2への統合、3.7の新設等)。
  - **AIアップミックス(3.7、実在の音源分離モデルHT-Demucs使用)**: ユーザーから
    「単純なpanフィルタではなく、実際のAIで」との要望を受け、実在するMeta製の音源分離
    モデル**HT-Demucs**(MIT)の[demucs-onnx](https://github.com/StemSplit/demucs-onnx)
    ONNX変換版を、make-diskの他のAI機能(RNNoise/LavaSR)と同じく**純Rustのtractで推論**
    する形で実装した(Pythonは実行時に一切使わない設計を維持)。
    - **技術的な壁と突破**: 配布されているhtdemucs.onnxをそのままtractで読み込むと、
      tract側のONNX`Pad`(省略可能な第3入力を必須とみなす)・`Range`(697個のノードで
      symbolic dimension型推論に失敗)の2つの演算子実装の狭さで失敗した
      (onnxruntimeでは問題なく動く、モデル側の欠陥ではない)。onnxライブラリで
      グラフを直接パッチ(Pad: 明示的な定数入力を追加、Range: onnxruntimeで実際に
      1回推論して値を捕捉しConstantノードへ置換)することで解決し、
      **2026-09-28にtractでの読み込み・最適化・推論実行(出力shape `[1,4,2,343980]`)
      まで実機確認済み**。パッチ手順は`tools/demucs-onnx-patch/`に再現可能な形で記録。
    - **設計**: ボーカル/ドラム/ベース/その他の4パートへ分離し、センター=ボーカル、
      フロント=ドラム+ベース(元のステレオ配置)、リア(サイド)=その他(残響/伴奏)、
      LFEは常に無音、へ配置する(`src-tauri/src/engine/ai_upmix.rs`)。固定長
      (343,980サンプル=約7.8秒)チャンクに分けて処理し端数はゼロ埋め(オーバーラップ
      加算によるクロスフェードは未実装、境目にごく僅かな不連続が生じ得る近似)。
    - **未実施(次回)**: モデル配布(パッチ後約2.2GBとGitHub Releasesの標準上限2GBを
      超えており、LavaSR等と同じ「初回自動ダウンロード」方式をまだ用意できていない。
      現状は環境変数`MAKE_DISK_DEMUCS_MODEL_DIR`での手動配置のみ対応、未設定/失敗時は
      既存のpanフィルタ疑似サラウンドへ自動フォールバックする)。オーバーラップ加算。
      モデルファイルの肥大化(316MB→2.06GB、693個のRangeベイクがノード属性のため
      外部データ化されないことが原因)の解消。
  - `cargo test`: 153 passed, 0 failed(ai_upmix関連5件含む。probe側の1件は
    マルチスレッド実行時のみ再現する既存の不安定なテストで、今回の変更とは無関係)。

## 音声・音質の研究の成果(maid-cafe-se由来、2026-09-30)

ユーザー指示(2026-09-30)「この音質研究はaruaru-llmにもopen-englishにもmaidcafe-programming-schoolにもmake-diskにもmaid-cafe-seにも影響させて」に基づく。
正本は[`aon-co-jp/maid-cafe-se`](https://github.com/aon-co-jp/maid-cafe-se)の`PORTING.md`「音質向上の研究」節。ここには、このリポジトリに関係する要点だけを書く。

| 項目 | 結果(すべて実測。聴感ではなく数値・テストでの検証) |
|---|---|
| 音程と声の太さ(フォルマント)を独立に制御 | リサンプリング方式は音程を動かすと声の太さも同じ比率で動く(音程を下げると「怪物っぽい声」)。FFT+ケプストラム包絡の周波数伸縮補正で独立に制御できた。直接合成した正解の母音との包絡距離: 新方式1.6dB、旧方式10.3dB(音程0.72倍・声の太さ据え置き)。補正ゲイン上限は±12dBだと鋭いフォルマントを動かせず、±24dBで解決 |
| AI帯域拡張(LavaSR、Apache-2.0、学習データVCTK) | make-diskの設計(入力の帯域は変えず、高域だけを頭打ちつきで足す)が声にも有効。ただし**声は高域が「崖」でなくなだらかに減衰する**ため、音楽向けの崖検出は12.4kHzを返し可聴域に何も足さなかった。声向けのロールオフ検出を新設。Windows音声Harukaで7.5kHzを検出、自己教師あり評価(6kHzで帯域制限→拡張→元の音声とのLSD、6〜9.5kHz)46.8dB→12.9dB、入力の帯域は変化なし。**LSDはスペクトル包絡の近さで聴感品質ではない。拡張前は帯域が無音のため差の大部分は「何か入れれば縮む」分** |
| 日本語ニューラルTTS | 安全に配布アプリへ同梱できるモデルは未発見。sherpa-onnx公式に日本語TTSモデルは無い/piper-plus系は日本語の学習データがMOE-Speech(ゲーム音声、機械学習解析目的のみ・再配布禁止)由来で配布不可/Kokoro日本語は作者評価がC+〜C-でG2Pの移植が重い |
| Rust化と音質 | Rust化そのものは音質を変えない。Kotlin版とRust版の出力は数値的に同一(最大誤差0.00000)、速度もウォーム時はほぼ同じ(4秒の音声を、Kotlin 36ms/126ms、Rust 34ms/68ms、単独/ハモり) |

実装: **RPoemの共有クレート`open-runo-voice`**(`RPoem/crates/open-runo-voice`、`docs/voice.md`。依存クレート無しの純Rust。wasm32-unknown-unknown向けのコンパイルは確認済み、ブラウザでの実行は未検証)。AI帯域拡張は`maid-cafe-se`の`crates/maid-cafe-enhance`(tract+ONNX、モデル約56MBは固定リビジョン+SHA-256で取得/同梱。重い依存をRPoem本体へ持ち込まないため別置き)。

### このリポジトリへの影響(**反映済み**: 声向けの帯域拡張モード)

- make-diskの`audio_sr.rs`(LavaSRによる帯域拡張)が、この研究の**出発点**。maid-cafe-seへ移植して声(TTS)に使ったところ、**声は高域がなだらかに減衰するだけで、音楽向けの崖検出が機能しない**ことが分かった(Windows音声Harukaで実測: 崖検出は12.4kHz=可聴域に何も足さない)。
- そこで逆方向に、声向けの検出をmake-diskへ取り込んだ: `audio_sr::detect_speech_rolloff_hz`(声の主要帯域3〜6kHzの平均より18dB下がった最初の周波数の1.5kHz手前をカットオフにする)、`AudioBwe.speech`(`extend_wav_file`の引数、UIのチェックボックス`audio-bwe-speech`)。**音楽には使わない**(自然な高域の減衰だけで条件を満たし、帯域が欠けていない音源にも拡張が掛かる)。既定は従来どおり音楽向け(`speech: false`)で、既存の挙動は変えない。
- 音程と声の太さの独立制御・ニューラルTTS・ハモり等は、ディスクオーサリングの用途外なので取り込まない。

## HANDOFF (2026-10-06 v0.1.32: poppler同梱・入れ子PDFの右綴じ変換)

- Windows版インストーラーへpoppler-utils(pdftoppm/pdfinfo+DLL+poppler-data)を同梱
  (`scripts/fetch-poppler-sidecars.sh`→`bundle.resources`で`poppler/bin`・`poppler/share/poppler`、
  `engine::sidecar`が探索)。LinuxはdebのDependsにpoppler-utils。macOSは未対応。
- `reverse_pdf_page_order`を入れ子ページツリー(Acrobat Distiller出力)対応へ拡張(葉へフラット化、継承属性を複製)。
  実PDF(44ページ)で変換し、先頭ページ=元の最終ページを画像一致で検証済み。
- 注意: `Missing language pack for 'Adobe-Japan1'`の警告は、非埋め込みCJKフォントの代替フォントが無いことが原因で、
  poppler-data(cMap)だけでは消えない。スキャンPDFの不可視OCR文字層にのみ影響し、描画結果には影響しない。
- 未コミットの他セッション作業(ai_upmix.rs・ort依存のCargo.toml/Cargo.lock差分)は今回含めていない。
