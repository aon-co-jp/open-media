# セッション引き継ぎメモ

**言語 / Languages**: 日本語(このページ・正本) | [English](PORTING/PORTING.en.md) | [简体中文](PORTING/PORTING.zh-CN.md) | [繁體中文(台灣)](PORTING/PORTING.zh-TW.md) | [한국어](PORTING/PORTING.ko.md) | [Deutsch](PORTING/PORTING.de.md) | [Français](PORTING/PORTING.fr.md) | [Русский](PORTING/PORTING.ru.md) | [Українська](PORTING/PORTING.uk.md) | [فارسی](PORTING/PORTING.iran%28Perusha%29.md) | [العربية](PORTING/PORTING.ar.md)
(多言語版は要約です。全文・履歴は日本語版が正本 / The translations are summaries; this Japanese file is the full, authoritative version.)

このファイルは、複数セッションにまたがる作業の到達点・次回再開ポイントを
記録する(`open-raid-z`等の他リポジトリのPORTING.md運用に準じる)。
詳細な技術的発見・方針決定は[`CLAUDE.md`](CLAUDE.md)にあるので、
ここでは「今どこまで進んでいて、次に何をするか」だけを簡潔に記す。

## 🔁 再開用メッセージ / Resume note (2026-09-26、最新 / latest、v0.1.31)

**日本語**:
- **リリース済み**: v0.1.29(open-cuda/open-directx/aruaru-llmの32GB VRAM級3ベンダー前提+F16/F32/F64/F128精度対応)、v0.1.30(open-easy-webのインストール配置`%LOCALAPPDATA%\open-easy-web\make-disk`への移行、旧配置からの自動移動)、v0.1.31(4KでのRIFE実測・GPU喪失時の全コマ検査+区間単位のCPUフォールバック・4K/120fps通し変換・実写向けrealesr-general-x4v3同梱・AI超解像オフでの補間専用再生成・Android APKファイル名統一)。全プラットフォームのCI成功(Windows NSIS/WiX・macOS Intel/Apple Silicon・Linux deb/rpm/AppImage・Android universal apk)、`gh release view v0.1.31`で全資産を確認済み。
- **make-disk-installer.exe新設(未リリース版、installer-exe/)**: 単一自己完結インストーラー(Rust製、native-windows-gui)。make-disk本体・本家ffmpeg/ffprobe・実験的rs-ffmpeg/rs-xorrisoを`include_bytes!`で埋め込み、WebView2は未導入時のみブートストラッパー取得、open-barは「同時にインストール」チェック時だけGitHub Releasesから最新版を取得してサイレントインストール。実機で展開・レジストリ登録(プログラムと機能)・アンインストール・open-bar取得経路まで自動検証済み(`--test-install`/`--uninstall`)。GUIの実クリック操作(参照ダイアログ等)は自動操作ツールが無く未検証。**既知の欠落**: 本家xorriso未同梱(`KNOWN_GAPS.txt`で告知)。現状v0.1.31のGitHub Releaseへ追加資産として手動アップロード済み(タグは切り直していない)、CI(`release.yml`)への統合はまだ。
- **バージョン自動同期の仕組みを新設**: `scripts/sync-doc-versions.sh`が`package.json`のversionを読み、README.md(日本語・英語)の「最新版」表記とファイル名一覧を自動で揃える(CLAUDE.md/PORTING.mdの過去ログ本文は履歴保存のため対象外——このエントリのような新規追記でのみ最新化する運用)。
- **open-bar側の実バグ修正(姉妹リポジトリ、ユーザー報告により対応)**: 共有モード出力コールバックが`try_lock()`失敗時にフェード無しの無音を差し込んでおり、標準/シャープ/ソフトいずれのフィルターでも再生開始少し後に一瞬ノイズが入る原因になっていた。ブロッキング`lock()`へ変更+リング事前確保で修正、`503611f`としてpush済み。

**次にやること(ユーザー依頼済み・未着手)**:
1. make-disk-installer.exeのGUI実クリック検証(実機で手動、または自動操作ツールの導入)。
2. make-disk-installer.exeへ本家xorrisoの同梱(簡易な静的Windows版の入手手段を探す)。
3. make-disk-installer.exeのCI統合(`release.yml`への組み込み、または別ワークフロー)。
4. LLMマネージャ(推奨/一つ大きい/一つ小さい、NPUは後回し)→ローカルopen-web-server(aruaru-llmのゲートウェイ)→easy-web.tokyo連携と`make-disk://`起動(v0.1.28時点から継続、未着手のまま)。
5. アップコンバートの本番サイズでの実変換E2E(v0.1.28時点から継続、未着手のまま)。

**English**:
- **Released**: v0.1.29 (open-cuda/open-directx/aruaru-llm now assume a 32GB-VRAM-class, 3-vendor baseline with F16/F32/F64/F128 precision), v0.1.30 (moved the install layout to `%LOCALAPPDATA%\open-easy-web\make-disk`, auto-migrating from the old location), v0.1.31 (measured real 4K RIFE speed, per-frame inspection with section-level CPU fallback on GPU loss, a full 4K/120fps end-to-end conversion, bundled the live-action `realesr-general-x4v3` model, interpolation-only re-generation with AI upscaling off, unified the Android APK filename). CI green on all platforms; all release assets confirmed via `gh release view v0.1.31`.
- **New: make-disk-installer.exe (not yet released, installer-exe/)**: a single self-contained installer (Rust, native-windows-gui) embedding make-disk itself plus genuine ffmpeg/ffprobe and the experimental rs-ffmpeg/rs-xorriso tributes via `include_bytes!`; fetches the WebView2 bootstrapper only if missing, and open-bar's latest Windows installer from GitHub Releases only when its checkbox is checked, running it silently. Verified on this machine: extraction, "Programs and Features" registration, uninstall, and the open-bar fetch path (via `--test-install`/`--uninstall`); the GUI click-through itself is not yet verified (no GUI automation tool available). **Known gap**: genuine xorriso is not bundled (disclosed via `KNOWN_GAPS.txt`). Currently uploaded as an extra asset on the existing v0.1.31 GitHub Release (no new tag), not yet wired into CI.
- **New version-sync tooling**: `scripts/sync-doc-versions.sh` reads `package.json`'s version and keeps README.md's (Japanese + English) "latest version" line and filename table in sync automatically. CLAUDE.md/PORTING.md's past log entries are intentionally excluded (they're historical records); only new entries like this one get the current version.
- **open-bar bugfix (sibling repo, from a user report)**: the shared-mode output callback injected unfaded hard silence whenever `try_lock()` failed, causing an audible click/noise a moment after playback started in every filter mode (Standard/Sharp/Soft). Fixed with a blocking `lock()` and ring pre-allocation, pushed as `503611f`.

**Next**: click-through GUI verification for make-disk-installer.exe; bundle genuine xorriso; wire the installer into CI; LLM manager / local open-web-server / easy-web.tokyo integration (carried over, still not started); full-size upconvert E2E (carried over, still not started).

## 🔁 再開用メッセージ / Resume note (2026-09-24、最新 / latest、v0.1.28)

**日本語**:
- **リリース済み**: v0.1.26(MP4→CDでWAVが0.6秒になる実バグ修正・変換失敗時はISO化/書き込み中止・必要な部分だけ切り出す(高速)・切り出し後の再エンコードをGPU/open-cpuで高速化・AIで探す(aruaru-llm)・出力先フォルダ検証)、v0.1.27(アップコンバートしてディスクいっぱいに収める: DVD 1〜2層/Blu-ray 1〜4層×フルHD/4K、容量から逆算したビットレートは2パス)、v0.1.28(「拡大の方法」の注意書きを日英で独立表示)。全プラットフォームのCI成功。
- **インストール版で確認済み(v0.1.28、このPC)**: 本体・アプリ一覧・ショートカットが0.1.28、起動・応答OK。WebView2のデバッグ接続(`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`でアプリを起動し、CDPの`Runtime.evaluate`で画面を読み取り・操作)で、アップコンバート欄(6種のディスク・日英注意書き・「この設定にする」でディスク/MKV/4K/ディスクいっぱい/ISOが設定される)と、**アプリ自身の`convert_media`で3時間33分の実動画から60分切り出し→WAV 3600秒・635MB(8.6秒)**を確認。
- **未確認**: アップコンバートの本番サイズ(25GB〜)いっぱいまでの実変換(数時間かかるため未実施。容量合わせの計算と2パスの精度は短い素材で96〜99%・超過なしを確認済み)。

**次にやること(ユーザー依頼済み・未着手)**:
1. インストール構成: `%LOCALAPPDATA%\open-easy-web\`を一番上にし、その下に`make-disk`・`aruaru-llm`・`open-web-server`(open-cpu/open-directx/open-cudaはライブラリなのでaruaru-llm・make-diskに組み込み)。make-disk本体のNSISインストール先変更も含む。
2. make-diskに「AIエンジン(LLM)」設定画面: aruaru-llm(Releases v0.2.4)とopen-web-server(v0.1.0)を取得・起動/停止、CPU(open-cpu)・メモリ・GPU/VRAM(aruaru-llm`/v1/recommend`)を表示、「推奨/一つ大きい/一つ小さい」LLMのインストール。**NPUは後回し**(ユーザー指示)。
3. ローカルのopen-web-serverはaruaru-llmへの窓口(ユーザー決定)。「AIで探す」の接続先もそこへ。
4. easy-web.tokyoに紹介+リンク、easy-web.tokyo/make-diskのブラウザからも操作可能に。**ローカルにインストール済みならアイコンからもブラウザからもローカル版を優先起動**(`make-disk://`のURLプロトコルをインストーラーで登録)。ブラウザ操作は許可元をeasy-web.tokyoに限定+初回ペアリングコード。
5. アップコンバートの本番サイズでの実変換E2E。

**English**:
- **Released**: v0.1.26 (fixed the 0.6-second WAV bug, abort ISO/burn after a failed conversion, fast extract of only the needed range, GPU/open-cpu-accelerated re-encode, AI range search via aruaru-llm, output-folder checks), v0.1.27 (upconvert and fill the disc: DVD 1–2 layer / Blu-ray 1–4 layer × Full HD / 4K, 2-pass for capacity-derived bitrates), v0.1.28 (separate bilingual note on interpolation upscaling). CI green on all platforms.
- **Verified on the installed app (v0.1.28, this PC)**: version 0.1.28 everywhere, launches and responds. Through WebView2 remote debugging (start the app with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`, then read/drive the page with CDP `Runtime.evaluate`): the upconvert panel works, and **the app's own `convert_media` extracted 60 minutes from a real 3h33m video → WAV 3600 s, 635 MB, in 8.6 s**.
- **Not yet verified**: a full-size upconvert filling a real disc (hours long); the fill math and 2-pass accuracy were verified on short clips (96–99%, never over).

**Next (requested, not started)**: open-easy-web install layout; LLM manager screen (recommended / one size up / one size down, NPU later); local open-web-server as the aruaru-llm gateway; easy-web.tokyo intro + browser control with local-first launch via `make-disk://`; full-size upconvert E2E.

## 🔁 再開用メッセージ / Resume note (2026-09-23)

**日本語**: 区間カットが「8.」から操作できなかった実バグを修正、DSD作成時はPCMを作らない仕様へ変更、
「サイズ/時間でカット(YES/NO排他)」「ディスクいっぱいに収める」「再生規格(CD/DVD/BD/PC)の上限kHz・ビットレート」を実装し、
README/CLAUDE/PORTINGを多言語化した(詳細は`CLAUDE.md`の「続き21」)。

**追記(同日、v0.1.26)**: MP4→CDでWAVが0.6秒になる実バグ(カット時の中間ファイルを.wavで無変換抽出しAACが壊れていた)を実ファイルで再現・修正し、変換失敗時はISO化・書き込みを中止するようにしてv0.1.26としてリリース。

**次にやること**:
1. 実ファイルでのE2E(カット変換は実ファイルで確認済み): 再生規格選択時の`-ar`・ビット深度・ビットレート上限、ディスクいっぱいの算出、ブルーレイ再生と同時変換時の体感。
2. 下の2026-09-19版「次にやること」(音楽CD書き込み、`rs-*`のオンデマンド取得、動画+DSDのセット書き出し等)は継続。

**v0.1.26リリース後の未着手タスク(2026-09-23夜、ユーザー依頼・次回ここから)**:
1. インストール構成: `%LOCALAPPDATA%\open-easy-web\`を一番上にし、その下に`make-disk`・`aruaru-llm`・`open-web-server`を置く(open-cpu/open-directx/open-cudaはライブラリなのでaruaru-llm・make-diskに組み込みの形。フォルダ単独配置は不要と説明済み、ユーザーの最終判断待ち)。make-disk本体のNSISインストール先変更も含む。
2. make-diskに「AIエンジン(LLM)」設定画面: aruaru-llm(GitHub Releases v0.2.4のwindows zip)とopen-web-server(v0.1.0 zip)を取得・起動/停止、CPU(open-cpu)・メモリ・GPU/VRAM(aruaru-llmの`/v1/recommend`=open-cuda/open-directx)を表示、「推奨/一つ大きい/一つ小さい」LLMのインストール(`/v1/recommend-and-download`・`/v1/models/*`)。**NPUは後回し**(ユーザー指示)。
3. ローカルのopen-web-serverはaruaru-llmへの窓口(ユーザー決定)。「AIで探す」の接続先もそこへ。
4. easy-web.tokyoに紹介+リンク。easy-web.tokyo/make-diskのブラウザからも操作可能に。**ローカルにインストール済みならアイコンからもブラウザからもローカル版を優先起動**(`make-disk://`のURLプロトコルをインストーラーで登録)。ブラウザ操作は許可元をeasy-web.tokyoに限定+初回ペアリングコード。
5. v0.1.26のインストール版で「必要な部分だけ切り出す」をアプリ画面で実機確認(このセッションはデスクトップ操作不可のため、ユーザーに手順を依頼済み。Rust側は同じコードで実ファイル確認済み)。

**English**: Fixed the cut editor that could not be operated from section 8, stopped creating PCM alongside DSD, implemented
exclusive YES/NO cut-by-size/time, fill-the-disc post-processing and playback-standard (CD/DVD/BD/PC) kHz/bitrate limits, and added multilingual docs.
**Next**: real-file E2E of those features; the 2026-09-19 list below still applies.

## 🔁 再開用メッセージ / Resume note (2026-09-19、最新 / latest、v0.1.20+)

**日本語**: v0.1.20までにIMAPI2書き込み(実機CD書き込み成功)・AV1/Opus/Dolby保持・AIノイズ除去(RNNoise)・
DSD64〜1024(DSF)を実装した。その後の作業として、DSD(チャンネル並列)・rs-ffmpeg/rs-xorrisoの
バージョン管理付きプラグイン(`engine/plugins.rs`、同じ版は上書きしない)・ディスク変換方向による
解像度の絞り込みを追加した。DSDの実速度(最適化ビルド、2秒素材、実時間比): DSD64=0.3倍・DSD128=0.4倍・
DSD256=0.8倍・DSD512=1.6倍・DSD1024=3.1倍(3.5時間素材のDSD1024は約11時間)。

**次にやること**:
1. 本物のAI映像超解像(まずCPU=tract+open-cpu、次にopen-directx/open-cuda/aruaru-llm)。最大の壁はモデル調達:
   この環境にはPythonが無く.pth→ONNX変換ができないため、ncnn形式のReal-ESRGAN general-x4v3を自前で推論する案を調査済み。
2. 音声超解像(帯域拡張)AIモデルの選定。DSD化の前段が唯一意味のあるAI導入点(ΔΣ変調自体は数学的処理でAI不要、
   逐次処理のためGPU/NPU/SIMDでは高速化できない)。
3. BD/DVD(保護なし)の取り込み。著作権保護の回避は実装しない。
4. 音楽CD(CD-DA)書き込み(IMAPI2 TrackAtOnce。現状はデータCD)。
5. `rs-*`をインストーラーから外し、姉妹リポジトリのリリース資産からオンデマンド取得(上書きの無駄を完全に無くす)。

**English**: Through v0.1.20 we shipped IMAPI2 burning (verified on a real CD), AV1/Opus/Dolby preservation, AI noise reduction (RNNoise)
and DSD64–1024 (DSF). Afterwards we added per-channel parallel DSD, versioned rs-ffmpeg/rs-xorriso plugins (`engine/plugins.rs`, an identical
version is not overwritten) and resolution presets narrowed by disc direction. Measured DSD speed (optimized build, 2 s clip, real-time ratio):
DSD64 0.3x, DSD128 0.4x, DSD256 0.8x, DSD512 1.6x, DSD1024 3.1x (DSD1024 of a 3.5 h source takes ~11 h).

**Next**:
1. Real AI video super-resolution (CPU via tract + open-cpu first, then open-directx / open-cuda / aruaru-llm). Model sourcing is the main obstacle:
   there is no Python here to convert .pth→ONNX, so running Real-ESRGAN general-x4v3 from its ncnn files is the investigated route.
2. An audio super-resolution (bandwidth extension) model. This is the only meaningful AI hook before DSD (delta-sigma modulation is deterministic math,
   and being sequential it cannot be sped up with GPU/NPU/SIMD).
3. Unprotected BD/DVD ripping. Circumventing copy protection is not implemented.
4. Audio-CD (CD-DA) burning (IMAPI2 TrackAtOnce; currently data CDs).
5. Move `rs-*` out of the installer and fetch them on demand from the sister repos' release assets (removes the redundant overwrite entirely).

## 🔁 再開用メッセージ(2026-09-16続き、v0.1.7実装完了)

PC再起動による中断(下記2026-09-16エントリ)から復帰し、以下すべてを
実機ビルド検証まで完了させ、v0.1.7としてタグpush準備完了。詳細は
`CLAUDE.md`の2026-09-16 HANDOFF「自動アップデート確認・rs-FFmpeg/
rs-xorriso追加同梱・最高音質モード・変換の並列化」参照。

- 自動アップデート確認(起動時、日英併記ダイアログ、署名付き)。
- rs-FFmpeg/rs-xorrisoの実際の同梱(ビルド確認済み)。
- 「最高音質・最高画質で記録する」モード(未選択時は自動ロスレスWAV+
  ISO化、ディスク容量からの収録可否・必要時間の自動算出)。
- 変換処理の並列化(非同期・マルチスレッド)。

**次回への引き継ぎ**: (1) タグpush後のCI実際の成否確認(`prerelease:
false`への変更・署名付きビルドがCI環境でも成功するか)、(2) 実機での
自動アップデート機能そのもののエンドツーエンド確認(次のリリースで
実際に更新ダイアログが出て更新できるか)、(3) ユーザーから追加指示の
あった「open-directx/open-cuda/aruaru-llmのマルチCPU・マルチコア・
非同期対応」は別課題として保留(このリポジトリの対象外)。

## 🔁 再開用メッセージ(2026-09-16、PC再起動により中断)

- `rs-FFmpeg`/`rs-xorriso`(既存の姉妹Rustリスペクト版、新規作成は不要
  だった)をソースからビルドして`src-tauri/binaries/`へ配置する
  `scripts/build-rs-tribute-sidecars.sh`を新設し、
  `tauri.windows.conf.json`/`tauri.linux.conf.json`の`externalBin`へ
  `binaries/rs-ffmpeg`・`binaries/rs-xorriso`を追加した。ローカルで
  スクリプト単体の動作(ビルド成功・正しい命名での配置)は確認済み。
  **`npm run tauri build`によるインストーラー全体のビルド検証はPCの
  再起動により中断**——次回、まず`npm run tauri build`を最後まで実行し、
  `target/release/`に`rs-ffmpeg.exe`/`rs-xorriso.exe`(bareな名前、
  `sidecar.rs`のモジュールdoc「実機検証で発見・修正した実装ミス」と
  同じ命名規則のはず)が実際に配置されるか確認してから、
  ドキュメント更新・コミット・タグpushへ進むこと。
- ユーザーから「open-directx/open-cuda/aruaru-llmをmake-diskへ
  AI付きGPU支援機能として同梱してほしい」との追加指示があった。
  調査の結果、**本自前H.264/HEVCエンコードのGPUシェーダー実装は
  既にこのファイル内で非推奨と結論済み**(GT730にVulkan Video拡張が
  無い、CABACの逐次エントロピー符号化がGPU並列化に不向き——上記
  「H.264/H.265/HEVCの自前シェーダー実装は...非推奨」節参照)。
  実際に動いているGPU支援は`convert.rs`の`detect_hw_video_encoder`
  (ffmpeg自身のNVENC/QuickSync/AMF)経由のものであり、これは既に
  実装済み。aruaru-llmの統合(AIアシスタント機能)は、サーバー
  プロセスの起動・IPC・チャットUI・モデル選択という相応の設計が
  必要な別機能として、次回セッションで正式に設計してから着手する
  方針とし、拙速な実装は避けた(ユーザーもこの方針に同意済み:
  「その様に進めて」)。aruaru-llmは`ARUARU_LLM_BIND`環境変数で
  bindアドレスを指定可能(既定`0.0.0.0:4600`、ローカル単体運用時は
  `127.0.0.1`限定にできる設計が既にコード内にコメントで存在)——
  sidecarとして起動する場合の設計に使える情報として記録。

## 🔁 再開用メッセージ(2026-09-14続き)

- **ffmpeg/ffprobeのsidecar同梱化(Windows/Linux)を実装・実機検証・
  リリース完了**(v0.1.6)。詳細設計・実機検証で発見したバグ(当初
  `<name>-<target-triple>.exe`という誤った命名で探していたが、実際の
  インストール後は`<name>.exe`〈bare〉だった)は`CLAUDE.md`の
  2026-09-14 HANDOFF参照。**次回はCI(`gh run list`)の実際の成否を
  必ず確認すること**(このメッセージ記載時点ではタグpush前/直後)。
- **次の増分候補**: (1) macOS向けffmpeg/ffprobe同梱(信頼できる
  Intel/Apple Silicon両対応の静的ビルド入手元の確定が先決)、
  (2) xorrisoの同梱(静的クロスプラットフォームビルドの入手元確定+
  GPLライセンス表示整備が先決)、(3) モバイル(Android/iOS)向け
  ffmpegクロスコンパイル。

## 🔁 再開用メッセージ(2026-09-14時点、最新)

- **v0.1.5を全プラットフォーム(Windows/macOS×2/Linux×3形式/Android)
  で公開完了・実際にCIグリーンを確認済み**(v0.1.4は`--target x86`の
  誤り〈正しくは`i686`〉で`release-android`が即失敗、v0.1.5で修正し
  全5ジョブ成功。GitHub Releaseのアセット一覧を`gh release view
  v0.1.5`で実際に確認済み)。VPS(`easy-web.tokyo/make-disk/`)の
  紹介ページも`git pull`で更新反映済み(実際に`curl`でライブ確認済み)。
- Android APKのアセット名は当初`app-universal-debug.apk`という
  分かりにくい名前で公開されてしまった(`gh release upload
  local#label`の`#label`は表示名のみでファイル名は変わらないという
  仕様を実際に確認)——v0.1.5では手動でリネームして再アップロード済み、
  `release.yml`側も次回リリースから自動的に分かりやすい名前になるよう
  修正済み(`cp`でリネームしてからアップロード)。
- ユーザーから「必要なリポジトリを同梱してインストーラー付きアプリを
  完成させて」との指示があり、ffmpeg/xorrisoをTauriのsidecar機構で
  同梱する設計を`CLAUDE.md`の2026-09-14(続き)HANDOFFに記録した
  (実装は次回)。**最優先で次回着手すべき項目**。

## 🔁 再開用メッセージ(2026-09-12時点)

### 完了したこと

1. Tauri + Rustでのデスクトップ版スケルトン一式(UI・ffmpeg/xorriso
   ラッパー・容量ベース自動ビットレート・4段階品質警告)。
2. `https://easy-web.tokyo/make-disk/`に紹介・ダウンロードページを公開
   (VPSの`open-web-server`にルート追加、`easy-web.tokyo`トップページにも
   紹介カードを追加してWASM再ビルド込みで反映済み)。
3. 姉妹リポジトリ`rs-FFmpeg`・`rs-xorriso`(Rustでのリスペクト版)を新規
   作成。実際のmake-disk呼び出し引数で検証し、見つかった重大な非互換
   (無音で壊れた出力を書く等)を修正済み。両リポジトリとも現時点では
   本家の一部機能のみ対応。
4. Android実機(OnePlus A401OP, Android 15)での動作確認に成功。
   その過程で**全プラットフォームに影響する重大バグ**
   (`main.js`のベアESモジュールインポートが原因で全ボタンが無反応に
   なっていた)を発見・修正した。

### 未完了・次回やること

1. **最優先**: Android向けSAFフォルダ選択の独自Tauriプラグイン実装
   (`CLAUDE.md`の「発見2」に具体的な設計を記載済み・未着手)。
   Kotlin側の`ACTION_OPEN_DOCUMENT_TREE`ハンドリング、Rust側の
   `pick_output_tree`コマンド、`main.js`でのAndroid分岐、の3点セット。
2. `rs-FFmpeg`/`rs-xorriso`のAndroidクロスコンパイル・Rustライブラリ
   直接リンク化の検討(外部プロセスシェルアウトをやめる方向)。
3. このPC(Windows)でのAndroid Developer Mode有効化が反映されない問題
   ([`CLAUDE.md`](CLAUDE.md)のプラットフォーム範囲節に詳細記録)は
   未解決のまま。手動`.so`コピー+`gradlew`直叩きの回避策で当面は
   進められるが、`tauri android dev`によるホットリロード開発はできない
   状態が続いている。
4. 実機でのディスク書き込み検証(CD/DVD/Blu-ray)は光学ドライブが
   無いため未実施。

## 🔁 再開用メッセージ(2026-09-12 続き、v0.1.1)

### 完了したこと(追加分)

5. v0.1.0をGitHub Actions(全プラットフォームビルド)+GitHub Releaseで
   公開。https://github.com/aon-co-jp/make-disk/releases/tag/v0.1.0
6. 複数区間の動画カット機能(`CutRange`)を実装。最初・途中・最後、
   いくつでも指定可能。抽出は`-c copy`(無劣化・高速)、フォーマット
   変換が必要な場合のみ結合時に1回だけエンコード。単体テスト4件で
   区間計算ロジックを検証済み。
7. フレーム精度カットモード(`frame_accurate`)を追加。GPUハードウェア
   エンコーダ(NVENC/QuickSync/AMF)を自動検出して使用し、無ければ
   CPU(libx264、AVX2/AVX512はlibx264自身が自動活用)にフォールバック。
8. `open-cpu`をCargo依存として統合、CPUフォールバック時の速度目安を
   `estimate_cpu_encode_speed`コマンドでUIに提供。
9. Blu-ray 4層/BDXL(128GB, `DiscType::Bd128`)対応を追加。
10. フロントエンドに動画プレビュー付き区間カットエディタを実装
    (`<video>`要素+`convertFileSrc`、マウスでシーク→「現在位置」ボタン、
    または時:分:秒を直接数字入力、の両対応)。これに伴い
    `tauri.conf.json`の`assetProtocol`を有効化し、`tauri`クレートに
    `protocol-asset`フィーチャを追加した(無いとビルド時エラーになる
    ことを実際に確認済み)。
11. `open-cuda`に`yuv_to_rgb_cpu`(YUV420p→RGB24変換カーネル、CPU/rayon)
    を追加、スカラー参照実装と全画素一致を検証(2ユニットテスト+
    3072画素のend-to-end検証)。動画コーデック自体(H.264等)は
    非現実的なスコープのため対象外と明記。`open-directx`での実GPU
    ディスパッチ(既存の狭いDXBC→SPIR-Vデコーダの拡張が必要)は
    次段階として保留。

### 既知の未検証事項(v0.1.1時点、v0.1.2で一部解消)

- ~~このPCにffmpegが入っていないため~~ → **v0.1.2で解消**。gyan.devの
  公式Windowsビルドをこの開発機にインストールし、実際のffmpegで
  複数区間カット機能を検証した(下記参照)。
- 動画プレビューエディタ(`<video>` + `convertFileSrc`)の実機での
  表示・シーク動作は未検証(ビルド成功とロジックレビューのみ)。

## 🔁 再開用メッセージ(2026-09-12 続き、v0.1.2)

### 実施したこと

12. この開発機にffmpeg(gyan.dev公式Windowsビルド)をインストールし、
    複数区間カット機能・フレーム精度カットモードの**実際のffmpeg実行を
    伴う統合テストを2件追加**(合成テスト動画を`testsrc`で生成→
    カット適用→`ffprobe`で結果の尺を検証)。
13. **その統合テストで実際のバグを発見・修正した**:
    `detect_hw_video_encoder`は`ffmpeg -encoders`の一覧に載っているか
    (=ffmpegのビルドにそのコーデックがコンパイルされているか)だけを
    見ており、実際にこのマシンのGPUドライバが対応しているかは
    見ていなかった。実機のNVIDIAドライバが古く、`h264_nvenc`は
    リストには出るが実行すると
    `"Driver does not support the required nvenc API version. \
    Required: 13.1 Found: 11.1"`で失敗することを統合テストで実際に
    再現した。修正: 候補ごとに実際に1フレームだけ試しエンコードし、
    本当に成功するものだけを採用するよう変更(`hw_encoder_actually_works`)。
    修正後、10テスト全green。

このv0.1.2の教訓: 「実際に統合テストを書いて実行する」ことでしか
見つからないバグ(実機のドライバ依存の挙動)があった。単体テストと
コードレビューだけでは検出できなかった。

## 🔁 再開用メッセージ(2026-09-12 続き、v0.1.3予定分)

### 実施したこと

14. `open-cpu`統合を「参考表示のみ」から「実際の分岐・制御」へ強化。
    - `estimate_cpu_encode_speed`をFMA・AVX-512BW/VL等も見た4段階
      (fast/moderate/slow/very_slow)に拡張し、検出できた命令セット
      一覧(`detected_features`)も返すようにした。
    - `recommended_x264_preset()`を新設し、CPUフォールバック時の
      実際のffmpeg`-preset`引数を検出結果に応じて自動選択する
      (非力なCPUには`ultrafast`/`veryfast`、強力なCPUには`slow`)。
      表示だけでなく実際にffmpegのコマンドライン引数へ反映される
      制御になった。
    - `main.js`側: フレーム精度モードのチェックボックスをONにした際、
      速度目安がslow/very_slowなら`window.confirm()`で続行確認を
      挟むようにした。
    - 実際のffmpeg統合テスト(11件)全green(`-preset`引数追加後も
      複数区間カット・フレーム精度カットの結果尺は正しいことを確認)。

### 未完了・次回やること(継続)

- `open-directx`での実GPUディスパッチ(既存の狭いDXBC→SPIR-Vデコーダの
  拡張が必要、次段階として保留中)。
- macOS/Linux版の実機動作確認(GitHub Actionsでのビルド自体は
  v0.1.0〜成功しているが、実機起動確認はWindows版のみ)。
- 動画プレビューエディタの実機での表示・シーク動作の実機確認。
- rusty_h264(ピュアRust H.264実装、調査で発見)は現時点でスター6・
  作成2.5ヶ月と実績が浅く、「ffmpegとビット完全一致」という主張も
  独立検証できていないため、採用は見送り。将来の選択肢として記録のみ。

## 🔁 再開用メッセージ(2026-09-12 続き、v0.1.3、AndroidのSAFプラグイン完了)

### 実施したこと

15. **Android向けSAFフォルダ選択プラグインを実装・実機検証まで完了**。
    `src-tauri/plugins/tauri-plugin-android-folder/`に新規プラグイン作成
    (Rust: `pick_output_tree`コマンド。Kotlin:
    `ACTION_OPEN_DOCUMENT_TREE`+`takePersistableUriPermission`、
    `tauri-plugin-dialog`自身のAndroidソースを正確なAPIリファレンスとして
    参照)。`Cargo.toml`では`target.'cfg(target_os = "android")'`で
    Android限定の依存にし、デスクトップ版のビルド・11テストへの影響
    無しを確認。
    実機(OPPO Reno11 A)で「フォルダを選択」→ネイティブのSAFフォルダ
    ツリーピッカーが開く→フォルダを選んで「このフォルダを使用」→
    アクセス許可ダイアログ→`content://com.android.externalstorage.
    documents/tree/primary%3ADocuments`というURIが実際に「出力先
    フォルダ」欄へ反映される、という一連の流れをエンドツーエンドで
    確認した。
    **残課題**: 返るのはcontent:// URIでありffmpeg/xorrisoにそのまま
    渡せる実ファイルシステムパスではない。実際の変換機能との連携には
    URI→実処理の橋渡しが別途必要(かつffmpeg/xorriso自体がAndroidに
    存在しない問題は解決していない、上記「発見3」参照)。

## 🔁 再開用メッセージ(2026-09-12 続き、open-directx実GPU実行の事前検証)

### 実施したこと(調査のみ、コード変更なし)

16. `open-directx`でyuv_to_rgbカーネルを実GPU(Vulkan)で動かす前段階として、
    リスクを減らすため実装前に3点を検証した。
    - **ツール確認**: `fxc.exe`(HLSLコンパイラ)はこのマシンのWindows SDK
      (`C:\Program Files (x86)\Windows Kits\10\bin\...\x64\fxc.exe`)に
      同梱されており利用可能。
    - **GPU確認**: `open-cuda/examples/vulkan_info`を実行し、このマシンの
      GT 730で実際にVulkan 1.2のcompute queueが使えることを確認済み
      (`api_version: 1.2.175`、`OK: ... compute queue are available`)。
    - **既存デコーダの対応範囲を精査**: `directx-shader-translate`には
      想定より進んだ`translate_chain_shader`(N入力バッファ+逐次2項演算
      〈add/mul/div/sub〉のチェーンに対応、`vector_add_mul_div_sub_...`
      系のテストが多数存在)があった。
    - **重大な制約を発見**: `RegExpr`は`Load(uav)`と`BinOp`のみを表現でき、
      **即値(リテラル定数)オペランドに対応していない**
      (`decode_chain_shape`のソースを直接確認)。YUV→RGB変換の係数
      (1.402, 0.344136, 0.714136, 1.772, オフセット128)は全て定数のため、
      現状のデコーダでは原理的に表現不可能。

### 次にやること(具体的にスコープ確定済み、次回セッション)

- `RegExpr`に`Immediate(f32)`相当のバリアントを追加。
- `decode_chain_shape`で`RegisterType::Immediate32`オペランドを実際の
  fxc.exe出力で確認しながら認識できるよう拡張。
- `emit_chain_spirv`側で対応する`OpConstant`を生成するよう拡張。
- 簡単な定数付きシェーダー(例: `Output[i] = A[i] * 1.402 + 128.0`)を
  実際にfxc.exeでコンパイルし、実GPU(GT730)で動かしてCPU参照実装と
  数値一致することを検証してから、yuv_to_rgb本体の実装に進む。

## 🔁 再開用メッセージ(2026-09-12 続き、open-directx実GPU実行の第一段階完了)

### 実施したこと

17. **open-directxの`directx-shader-translate`に即値定数対応を実装し、
    実GPU上で検証まで完了した**。
    - `RegExpr::Immediate(f32)`を追加。基盤の`dxbc`クレートは
      `RegisterType::Immediate32`を既に汎用的にパース済みだったため、
      手書きのパターンマッチャー(`decode_chain_shape`)に「即値も
      有効な葉ノード」という認識を追加するだけで済んだ
      (`resolve_chain_source`ヘルパー新設)。
    - **実際にfxc.exeでコンパイルして初めて分かった発見**:
      `Output[i] = A[i] * 1.402 + B[i]`をコンパイルすると、fxc.exeは
      `mul`+`add`の2命令ではなく、単一の積和融合命令`mad`
      (`dest = src0*src1+src2`)に最適化していた。これは事前の
      静的解析だけでは分からず、実際にコンパイルして初めて発覚した。
      `Opcode::Mad`を`Add(Mul(a,b),c)`として式木に展開する対応を追加。
    - 新規テスト`vector_mul_const_add_real_vulkan.rs`で、実GPU
      (GT730, Vulkan 1.2)上で256要素すべてCPU参照実装と数値一致する
      ことを確認。ワークスペース全体の既存テスト(実GPU依存のものも
      含む)に回帰無し。
    - `tools/compile-dxbc-shaders.ps1`に新シェーダーのビルド手順を追加
      (再現性確保)。
18. `open-cuda`/`open-directx`双方への貢献はpush済み。

### 未完了・次にやること(v2、下記v3で一部完了)

- ~~yuv_to_rgb本体を、この即値対応済みチェーンデコーダの上に実装~~
  → **4:4:4(サブサンプリング無し)版のR/G/Bチャンネルまで完了(下記参照)**。

## 🔁 再開用メッセージ(2026-09-12 続き、yuv444_to_rgbプロトタイプ完成)

### 実施したこと

19. **BT.601のYUV(4:4:4)→RGB変換を、R/G/B各チャンネルごとの
    コンピュートシェーダーとして実装し、R/Bは実GPU(GT730)で
    数値検証まで完了した**。
    - `yuv444_to_r.hlsl`: `R = Y + 1.402*(V-128)` — 実GPUで256要素
      すべてCPU参照実装と一致。初回コンパイルでそのまま成功。
    - `yuv444_to_b.hlsl`: `B = Y + 1.772*(U-128)` — 同様に実GPU検証済み。
    - `yuv444_to_g.hlsl`: `G = Y - 0.344136*(U-128) - 0.714136*(V-128)`
      — 構造検証(正しいSPIR-Vへ変換されること)のみ完了、実GPU
      ディスパッチは下記の理由で未実施。
    - **実コンパイルで新たに発見した実パターン**: fxc.exeは
      `Y - k*(x-128)`を、negate単独命令ではなく**madの乗算オペランド
      自体にnegateフラグを立てる**形(`-(x-128)*k+Y`)へ最適化していた。
      これに対応するmad+negate処理を追加(新しいRegExpr種別は不要、
      `Sub(0, x)`への展開で対応)。
    - ワークスペース全体の既存テスト(実GPU依存分含む)に回帰無し。
20. **Gチャンネル(4バッファ: Y,U,V,Output)の実GPU検証を試みる過程で、
    opencuda-vulkan側の制約を発見**: 公開`launch_kernel`はカーネル名
    ("vector_add"等)でディスパッチ先を決める方式で、各ハンドラが
    バッファ本数を固定(`ensure_vector_add_args`は常に3バッファ+
    push constant 1個を要求)している。内部の`dispatch_spirv`自体は
    バッファ本数に汎用対応済み(記述子バインディングを
    `buffers.len()`から動的生成)なのに、それを外部から汎用に呼べる
    公開エントリポイントが無い。

### 未完了・次にやること(v3)

- **最優先**: `opencuda-vulkan`に汎用N バッファディスパッチの公開API
  (例: `run_generic_spirv`のような、バッファ本数を`args`から動的に
  取る経路)を追加する。これができればGチャンネル、および将来の
  「R/G/B/クランプを1カーネルにまとめた本実装」の実GPU検証が可能になる。
- クロマサブサンプリング(U/VがY解像度の半分)対応: 現状のチェーン
  デコーダは「全バッファが同じスレッドID添字」前提のため、
  `col/2`相当のインデックス計算(シフト/除算命令)を認識できるよう
  デコーダをさらに拡張する必要がある。4:4:4版が先に動いた今、
  この拡張が次の段階。
- `Opcode::Clamp`/`saturate`相当の命令のデコーダ対応(RGB出力を
  0-255にクランプする処理に必要、現状は未調査)。
- H.264/H.265/HEVCの自前シェーダー実装は、調査の結果
  **非推奨と判断・記録**: GT730はVulkan Video拡張(`VK_KHR_video_*`)を
  一切持たず(実機の`vulkaninfo`で0件確認)、GPU専用ハードウェアビデオ
  エンジン経由の道が無い。汎用コンピュートシェーダーでの自前実装も、
  CABACのような逐次エントロピー符号化がGPU並列化に極めて不向きという
  学術的知見が多数([HEVC decoder最適化](https://arxiv.org/pdf/1601.05313)等)。
  ffmpeg自身のVulkanコンピュートシェーダー実装もFFv1/ProRes/APV等の
  CABACを持たない形式限定([Khronos公式ブログ](https://www.khronos.org/blog/video-encoding-and-decoding-with-vulkan-compute-shaders-in-ffmpeg))。
  今後「シェーダーで動く自前コーデック」を目指す場合はFFv1のような
  よりシンプルな形式が現実的な候補。

## 関連リポジトリ

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — 本体
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) — FFmpegのRustリスペクト版
- [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — xorrisoのRustリスペクト版
- [aon-co-jp/open-cpu](https://github.com/aon-co-jp/open-cpu) — CPU命令セット検出(依存として使用)
- [aon-co-jp/open-cuda](https://github.com/aon-co-jp/open-cuda) — GPU計算抽象化層(`yuv_to_rgb_cpu`を追加)

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
