# binaries/

Tauriの[sidecar機構](https://tauri.app/develop/sidecar/)
(`tauri.windows.conf.json`/`tauri.linux.conf.json`の`bundle.externalBin`)が
参照する実行ファイルの配置場所。**このディレクトリの中身自体は
コミットしない**(`.gitignore`参照、バイナリでリポジトリを肥大化させない
ため)。

## 命名規則

`<ツール名>-<ターゲットトリプル>[.exe]`(Windowsのみ`.exe`拡張子)。
例(Windows x64): `ffmpeg-x86_64-pc-windows-msvc.exe`・
`ffprobe-x86_64-pc-windows-msvc.exe`

## 取得方法

`scripts/fetch-ffmpeg-sidecars.sh`(Windows/Linux対応、macOSは
`CLAUDE.md`「sidecar同梱化」節参照——現時点では未対応)を実行すると、
[BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds)の静的ビルドを
ダウンロードし、このディレクトリへ正しい名前で配置する。
`.github/workflows/release.yml`のWindows/Linuxビルドジョブがビルド前に
自動実行する(手動ビルド時は`npm run tauri build`の前に自分で
実行すること)。

```bash
bash scripts/fetch-ffmpeg-sidecars.sh
```

## 正直な開示(誇張しない)

- **xorrisoは同梱していない**——本家に信頼できる静的クロスプラット
  フォームビルド配布(BtbN/FFmpeg-Buildsのffmpegに相当するもの)が
  見当たらなかったため。CI内でのソースビルド(apt/Homebrew経由)は
  検討したが、GPLライセンスされた成果物を配布物へ含める場合の
  ライセンス表示整備を先に済ませるべきと判断し、今回は見送った。
  `ffmpeg`/`ffprobe`同様の仕組み(`sidecar::resolve_tool`)は既に
  汎用的に実装済みなので、静的バイナリの入手元さえ確定すれば
  コード変更なしで追加できる。
- **macOSは同梱していない**——BtbNはmacOSビルドを配布しておらず、
  Intel/Apple Silicon両対応の信頼できる単一の入手元を今回のセッションで
  確定できなかった(evermeet.cx等はIntel版のみ)。
- 上記いずれも、同梱バイナリが無い場合は既存通りPATH上の実行ファイルを
  使うフォールバックが効くため、**同梱していないプラットフォーム/
  ツールでもアプリ自体は壊れない**(従来通り別途インストールが必要
  なだけ)。
