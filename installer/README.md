# installer/

各プラットフォーム向けインストーラーの配布状況・入手方法をまとめる
(バイナリ自体はこのフォルダにコミットしない——GitHub Releasesが
正本の配布場所。理由: バイナリをgitへコミットするとリポジトリが
肥大化し続け、`git clone`が遅くなる一方だから)。

Where to get each platform's installer, and how it's built. Binaries
themselves are **not** committed into this folder — GitHub Releases is
the canonical distribution point (committing binaries into git would
make the repository grow unboundedly and slow down every clone).

## ダウンロード / Download

最新版: <https://github.com/aon-co-jp/make-disk/releases/latest>

| プラットフォーム / Platform | ファイル形式 / File(s) | ビルド元 / Built by |
|---|---|---|
| Windows | `make-disk_<version>_x64-setup.exe`(NSIS)・`make-disk_<version>_x64_en-US.msi`(WiX) | `.github/workflows/release.yml` の `release` ジョブ(`windows-latest`) |
| Windows(単一自己完結インストーラー) | `make-disk-installer.exe` — ffmpeg/xorriso(実験的Rust版)を同梱、open-barの同時インストールを任意で選べる。詳細は[`../installer-exe/README.md`](../installer-exe/README.md) | `installer-exe/`(ローカルビルド、まだCIには未統合。本家xorrisoは未同梱の既知の欠落あり) |
| macOS (Apple Silicon) | `make-disk_<version>_aarch64.dmg` | 同上(`macos-latest`、`--target aarch64-apple-darwin`) |
| macOS (Intel) | `make-disk_<version>_x64.dmg` | 同上(`macos-latest`、`--target x86_64-apple-darwin`) |
| Linux (Debian/Ubuntu系) | `make-disk_<version>_amd64.deb` | 同上(`ubuntu-22.04`) |
| Linux (Fedora/RHEL系) | `make-disk-<version>-1.x86_64.rpm` | 同上 |
| Linux (配布形式非依存) | `make-disk_<version>_amd64.AppImage` | 同上 |
| Android(スマホ・タブレット共通) | `make-disk_<version>_android-universal.apk`(universal、未署名、サイドロード配布) | `.github/workflows/release.yml` の `release-android` ジョブ(`ubuntu-latest`、`npm run tauri android build`) |
| iOS | 未対応(テスト実機が無いため保留、`CLAUDE.md`参照) | — |

## リリースの仕組み / How releases work

`v*`形式のgitタグをpushすると、GitHub Actions
(`.github/workflows/release.yml`)が自動的に全プラットフォーム分を
ビルドし、同じGitHub Releaseへ添付する。ローカルでの手動ビルドは
開発時の動作確認用(`npm run tauri build` / `npm run tauri android
build`)で、正式な配布物はCI経由のもののみを使う(実行環境の差異による
非再現性を避けるため)。

Pushing a `v*`-shaped git tag triggers GitHub Actions
(`.github/workflows/release.yml`), which builds all platforms and
attaches them to the same GitHub Release. Local builds (`npm run
tauri build` / `npm run tauri android build`) are for development
verification only — the canonical distributed artifacts always come
from CI, to avoid non-reproducibility from local environment
differences.

## 正直な開示 / Honest disclosure

- 全プラットフォームとも**プレリリース(pre-release)** 扱い。実機での
  ディスク書き込み検証(CD/DVD/Blu-ray)はまだ完了していない
  (`PORTING.md`参照)。
- Androidの`.apk`は**未署名**(Play Store配布用の署名鍵はまだ用意して
  いない)——サイドロード(設定から「提供元不明のアプリ」を許可して
  インストール)専用。
- iOS/iPadOSは対応保留(テスト実機を保有していないため、`CLAUDE.md`
  「プラットフォーム範囲」節参照)。
- 実行には`ffmpeg`/`xorriso`が別途インストール済みでPATHが通っている
  必要がある(インストーラーには同梱していない)——ユーザー指示により
  同梱化(sidecarバイナリ化)を次の開発増分として計画中、詳細は
  `CLAUDE.md`/`PORTING.md`の該当HANDOFF参照。

- All platforms are currently tagged as **pre-release**. Real-hardware
  disc-burning verification (CD/DVD/Blu-ray) is not yet complete (see
  `PORTING.md`).
- The Android `.apk` is **unsigned** (no Play Store signing key set up
  yet) — sideload only (enable "install from unknown sources").
- iOS/iPadOS support is on hold (no test device available, see
  `CLAUDE.md` "プラットフォーム範囲").
- `ffmpeg`/`xorriso` must be separately installed and on `PATH` — not
  bundled into the installer yet. Bundling them as Tauri sidecar
  binaries is now planned as the next development increment per user
  request; see the relevant `CLAUDE.md`/`PORTING.md` HANDOFF entry for
  the design.
