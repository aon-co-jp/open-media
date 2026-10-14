# open-media: 引っ越しガイドとOS別の状況

**言語 / Languages**: 日本語(このページ・正本) | [English](PORTING.en.md)
関連: [`CLAUDE.md`](CLAUDE.md)(開発ルール) / [`README.md`](README.md)

別のPC(新しい開発機)へ、開発環境ごと「お引越し」するための手順と、OSごとの対応状況をまとめる。
**リポジトリ(コード・文書・スクリプト)は GitHub と VPS にある。リポジトリに入らないもの(鍵・認証・実行ファイルの部品)だけを、別に運ぶ。**

## 1. 引っ越しの全体像(チェックリスト)

- [ ] ① 新しいPCに道具を入れる(第2章)。`powershell -File scripts\doctor.ps1` で不足を確かめる
- [ ] ② リポジトリを取得する(第3章)
- [ ] ③ **鍵・認証を安全な方法で運ぶ**(第4章。リポジトリ・チャット・メールに貼らない)
- [ ] ④ 動作確認(第5章)
- [ ] ⑤ 旧PCで、必要なら後片付け(第6章)

## 2. 新しいPCに入れる道具(Windows)

| 道具 | 用途 | 入れ方の例 |
|---|---|---|
| Git / GitHub CLI(`gh`) | 取得・push・リリース確認 | `winget install Git.Git GitHub.cli`、`gh auth login`(組織 `aon-co-jp`) |
| Rust(stable、MSVC) | ビルド | `rustup`(Visual Studio Build Tools の「C++によるデスクトップ開発」が必要) |
| Node.js 20 以上 | Tauri CLI・画面のビルド | `winget install OpenJS.NodeJS.LTS` |
| WebView2 | 実行(Windows 11 は標準) | 通常は不要 |
| ffmpeg / ffprobe / poppler / rs-FFmpeg / rs-xorriso | 同梱の部品(CIが用意) | **ローカルで実行ファイルを作るときだけ必要**。`scripts/fetch-ffmpeg-sidecars.sh` / `fetch-poppler-sidecars.sh` / `build-rs-tribute-sidecars.sh`(隣のフォルダに `rs-FFmpeg`・`rs-xorriso` をcloneしておく)。または旧PCの `src-tauri/binaries/` を丸ごとコピー |
| Android(任意) | APKのローカルビルド・実機試験 | Android SDK + **NDK 27.1.12297006** + Java 17 + `rustup target add aarch64-linux-android` + `adb` |
| WSL の Ubuntu(任意、**テスト用**) | Linux版の確認 | `wsl --install -d Ubuntu`、`sudo apt install libwebkit2gtk-4.1-dev libasound2-dev libappindicator3-dev librsvg2-dev patchelf` ほか、`.deb` を入れて試す |
| 音楽CDが読める光学ドライブ・USB DAC(任意) | 実機の再生試験 | CDの試験は `D:` を想定(`OPEN_MEDIA_CD` 環境変数で変更) |

Mac・Linuxの開発機へ移す場合: Rust・Node・Tauriの前提(Linuxは `libwebkit2gtk-4.1-dev` など、macOSは Xcode Command Line Tools)を入れれば、同じコードベースでビルドできる(**macOSは未確認**)。排他モード・システム音声の橋渡し・自動再生・関連付け・音楽CDの取り込みは Windows 専用。

## 3. リポジトリの取得

```
git clone https://github.com/aon-co-jp/open-media.git
cd open-media
npm install
cd src-tauri && cargo check
```

- **open-bar など依存は、Cargoが GitHub から自動で取得する**(`Cargo.toml` に版を固定してある)。別途cloneは不要。
- メタ索引リポジトリ `RUNO`(`F:\runo`)は、全プロジェクトの入口と引き継ぎ(`CLAUDE.md` の HANDOFF)を持つ。必要なら一緒にcloneする。
- **VPS(conoha)にも同じリポジトリがある**: `/root/repository/open-media`。新PCから `ssh conoha` でき、`git pull` で追従する。VPSを経由した引っ越しも可能: 旧PCで push 済みなら、新PCは GitHub から clone するだけでよい。

## 4. 運ぶもの(リポジトリに無い。**安全な方法で**)

| もの | 旧PCでの場所(例) | 新PCでの扱い |
|---|---|---|
| **更新の署名鍵**(秘密鍵) | `F:\open-media-updater.key`(`.pub`は公開鍵で、`tauri.conf.json` にも入っている) | 旧PCから**暗号化したUSB等**で運ぶ。**無くすと、既存の利用者へ自動更新を配れなくなる**(鍵を作り直すと、旧版は手動の入れ替えが必要)。GitHubのシークレット `TAURI_SIGNING_PRIVATE_KEY` には登録済みなので、CIのリリース自体は新PCに鍵が無くても動く |
| GitHub の認証 | `gh auth` | 新PCで `gh auth login`(トークンは運ばず、ログインし直す) |
| VPS への SSH 鍵 | `~/.ssh/`、`~/.ssh/config`(`Host conoha`) | 鍵と設定を安全に運ぶ。VPSの `authorized_keys` に新PCの公開鍵を足してもよい |
| Claude Code の記憶 | `%USERPROFILE%\.claude\projects\F--runo\memory\` | 新PCの同じ場所へコピー(プロジェクトのパス名が違うときは、フォルダ名を新しいパスに合わせる) |
| テスト用の素材 | `F:\open-media-test\`(ビルドのログ・撮った画面・インストーラー・試験の動画) | **不要**。必要なら作り直せる |
| 同梱の部品の実行ファイル | `src-tauri/binaries/` | 第2章のとおり。リポジトリには入れない(大きいため) |

**運んではいけないもの**: 鍵や認証の値を、リポジトリ・チャット・メールの本文・ログに貼ること。

## 5. 新PCでの動作確認

```
powershell -File scripts\doctor.ps1                  # 道具の確認
cd src-tauri; cargo check                            # ビルドが通るか(約2〜4分)
cargo test --lib assoc                               # 単体テスト
cargo test --lib real_cd -- --ignored --nocapture    # 実機: 音楽CDをD:に入れて(排他モードのDACが要る)
npm run tauri build -- --bundles nsis                # インストーラーを作る(約20分。署名の所は「秘密鍵が無い」と出てよい)
```

- インストーラーを入れて、起動・再生・アンインストールまで確かめる。
- 自動再生・関連付けのボタンは、押すと**そのPCの設定が変わる**。試したあと「解除」で元に戻す。
- リリースを出す試験は、`CLAUDE.md` の「リリース手順」のとおり。

## 6. 旧PCの後片付け(任意)

- このPCで自動再生の既定を open-media にしてあれば、アプリの「解除」を押す(以前の値に戻る)。
- 更新の署名鍵のコピーを、安全に消すか保管する。
- `F:\open-media-test\` と `%TEMP%\make-disk-player\`(再生用の一時ファイル)は削除してよい。

## 7. OS別の対応状況(2026-10-14)

| OS | 提供 | 確認したこと | 未確認・制限 |
|---|---|---|---|
| Windows | `.exe`(NSIS) / `.msi` | 実機: インストール・起動・アンインストール・自動更新・CD再生(352.8kHz/32bit)・橋渡し(384kHz/32bit)・1つだけ起動 | DVD/Blu-rayの実ディスク、自動再生の実際の動作(CDの入れ直し) |
| macOS | `.dmg`(Apple Silicon / Intel) | CIのビルドのみ | **実機は未確認**。排他モード・DoP・CD取り込みは非対応 |
| Linux(Ubuntu) | `.deb` / `.rpm` / `.AppImage` | WSLでインストール・起動・日本語の表示 | 音声デバイスでの再生は未確認。PipeWire設定は未検証。排他モード等は非対応 |
| Android(スマホ・タブレット) | APK(未署名、サイドロード) | 実機(Android 14): MP4・FLACの再生、画面の整理、十字キーの操作 | open-barのエンジンは無し(WebViewで再生)。映像の変換は無し |
| Android TV | 同じAPK | 起動口(leanback)と十字キーの見やすさを設定 | **TV実機は未確認** |
| iPhone / iOS | なし | — | 未対応(試験機が無い) |

## 8. つまずいたとき

- ビルドが `resource path ... binaries/... doesn't exist` で止まる → 同梱の部品が無い。第2章の部品を `src-tauri/binaries/` に置く(中身が空のファイルでも `cargo check` は通る)。
- `tauri android build` が「シンボリックリンクを作れない」で失敗する(Windowsの開発者モードが無効) → `scripts/android-local-build.ps1`。
- 更新が動かない → `CLAUDE.md` の「見つけた落とし穴」(権限・`--no-bundle` は `.msi` を選ぶ)。
- 排他モードで音が出ない・`0x8889000A` → 別のアプリがDACを使用中(open-media自身を含む)。止める。
- Ubuntuで日本語が四角 → 同梱フォントで解消済み(`v0.1.9` 以降)。
- CIの結果の見方 → `gh run view <id> -R aon-co-jp/open-media --json status,conclusion,jobs`(bashから)。
