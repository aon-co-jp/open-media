# open-media

**日本語** | [English](README.en.md)

**open-media = [make-disk](https://github.com/aon-co-jp/make-disk) + [open-bar](https://github.com/aon-co-jp/open-bar)**

CD / DVD / Blu-ray の書き込み・音声/動画の変換・ISO作成(make-disk)と、高音質・高画質プレーヤー(open-bar)を **1つのアプリ** にまとめたものです。Windows / macOS / Linux / Android(スマホ・タブレット)向けに、同じコードベースから作ります(Rust + Tauri)。

> make-disk は最終版のまま残し、以降の開発はこのリポジトリ(open-media)で続けます。

## ダウンロード

最新版: <https://github.com/aon-co-jp/open-media/releases/latest>

| 環境 | ファイル |
|---|---|
| Windows | `open-media_0.1.0_x64-setup.exe` / `.msi` |
| macOS (Apple Silicon / Intel) | `open-media_0.1.0_aarch64.dmg` / `open-media_0.1.0_x64.dmg` |
| Linux | `.deb` / `.rpm` / `.AppImage` |
| Android(スマホ・タブレット共通) | `open-media_0.1.0_android-universal.apk`(未署名、サイドロード) |

## できること

### make-disk 由来(変換・書き込み)
- CD / DVD / Blu-ray への書き込み、ISO出力、音楽CDの取り込み
- 音声・動画のフォーマット変換、ビットレート自動調整、区間カット
- AIノイズ除去・AI高域生成・AI超解像・フレーム補間・サラウンド変換、PDF見開き変換

### open-bar 由来(再生)— 「0. ディスクプレーヤー」
- **ハイブリッド再生**: 音声(CD・音声ファイル・DSD)は open-bar エンジン、映像(DVD / Blu-ray / 動画)はこの画面の映像プレーヤーで再生します。
- **再生モード**: B 共有モード・高品質アップサンプル(既定)/ A 排他モード・ビットパーフェクト / E 排他+アップサンプル / D DoP(DSDのままDACへ。対応DACのみ)。
- **高音補正**(MP3向け)、一時停止・シーク・音量、モードの切り替え。
- **ディスクの選択はラジオボタン**。複数の曲やタイトルを同時に再生することはできません。
- **映像+別音声の組み合わせ**(例: MP4動画 + DSD音声)。音声を時間の基準に映像を追従させ、`.obar.json` で保存できます。
- **MQA**: 判別のみ。復号は行いません(MQA対応DACへはモードAで素通し)。

## 現状と既知の制限(正直な開示)

- **実際のディスクを入れた再生は、まだ通しで確認していません**(CD / DVD / Blu-ray)。ビルドとエンジン単体の確認までです。
- コピーガード付きの市販ディスクは読めません(保護の回避は行いません)。
- DVD / Blu-ray の映像はブラウザで直接再生できないため、MP4へ変換してから再生します。長い本編は待ち時間があります。
- 排他モード・DoP は Windows のみです。macOS / Linux / Android では共有モードまたは映像プレーヤー経由の再生になります。音楽CDの取り込み・再生も Windows のみです。
- 検証状況: Windows は実機でビルド。Linux(Ubuntu)と Android(arm64)はコンパイルまで確認。macOS は未確認(GitHub Actions でビルドされますが、実機では動かしていません)。
- 自動アップデートは未設定です(次の版で対応予定)。
- ライセンス: MIT(各依存クレートのライセンスは個別に従います)。

## ビルド

```
npm install
npm run tauri dev      # 開発起動
npm run tauri build    # インストーラー作成
```

ffmpeg / ffprobe / xorriso は同梱(CI)またはPATH上のものを使います。詳細は [CLAUDE.md](CLAUDE.md) / [PORTING.md](PORTING.md)(make-disk 時代の開発記録を含みます)。
