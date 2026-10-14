# open-media

**日本語** | [English](README.en.md)

**open-media = [make-disk](https://github.com/aon-co-jp/make-disk) + [open-bar](https://github.com/aon-co-jp/open-bar)**

CD / DVD / Blu-ray の書き込み・音声/動画の変換・ISO作成(make-disk)と、高音質・高画質プレーヤー(open-bar)を **1つのアプリ** にまとめたものです。Windows / macOS / Linux / Android(スマホ・タブレット)向けに、同じコードベースから作ります(Rust + Tauri)。

> make-disk は最終版のまま残し、以降の開発はこのリポジトリ(open-media)で続けます。

## ダウンロード

最新版: <https://github.com/aon-co-jp/open-media/releases/latest>

| 環境 | ファイル |
|---|---|
| Windows | `open-media_0.1.10_x64-setup.exe` / `.msi` |
| macOS (Apple Silicon / Intel) | `open-media_0.1.10_aarch64.dmg` / `open-media_0.1.10_x64.dmg` |
| Linux | `.deb` / `.rpm` / `.AppImage` |
| Android(スマホ・タブレット共通) | `open-media_0.1.10_android-universal.apk`(未署名、サイドロード) |

## できること

### make-disk 由来(変換・書き込み)
- CD / DVD / Blu-ray への書き込み、ISO出力、音楽CDの取り込み
- 音声・動画のフォーマット変換、ビットレート自動調整、区間カット
- AIノイズ除去・AI高域生成・AI超解像・フレーム補間・サラウンド変換、PDF見開き変換

### open-bar 由来(再生)— 「0. ディスクプレーヤー」
- **ハイブリッド再生**: 音声(CD・音声ファイル・DSD)は open-bar エンジン、映像(DVD / Blu-ray / 動画)はこの画面の映像プレーヤーで再生します。
- **再生モード**: E 排他モード・高品質アップサンプル(既定。CDは352.8kHz、192kHzは384kHzまで。非対応機器は自動で共有モードへ)/ B 共有モード・高品質アップサンプル / A 排他モード・ビットパーフェクト / D DoP(DSDのままDACへ。対応DACのみ)。
- **出力は32bit(既定)**。受け付けない機器は24bitへ自動で落とします(内部はf32のため実分解能は24bitまで)。
- **映像のアップコンバート**: DVD / MP4 / MKV などを再生しながらFull HD / 4Kへ引き上げ(lanczos補間。AI超解像ではありません)。音声は別に取り出してアップサンプリング再生。
- **高音補正**(MP3向け)、一時停止・シーク・音量、モードの切り替え。
- **ディスクの選択はラジオボタン**。複数の曲やタイトルを同時に再生することはできません。
- **映像+別音声の組み合わせ**(例: MP4動画 + DSD音声)。音声を時間の基準に映像を追従させ、`.obar.json` で保存できます。
- **MQA**: 判別のみ。復号は行いません(MQA対応DACへはモードAで素通し)。

## 現状と既知の制限(正直な開示)

- **音楽CDの再生は実機で確認済み**(Windows。トラックの読み取り→排他モード+アップサンプル 44.1→352.8 kHz+32bit、音切れ0)。**DVD / Blu-rayの実ディスクでの再生は未確認**です(変換処理は合成データで確認)。
- コピーガード付きの市販ディスクは読めません(保護の回避は行いません)。
- DVD / Blu-ray の映像はブラウザで直接再生できないため、MP4へ変換してから再生します。長い本編は待ち時間があります。
- 排他モード・DoP は Windows のみです。macOS / Linux / Android では共有モードまたは映像プレーヤー経由の再生になります。音楽CDの取り込み・再生も Windows のみです。
- 検証状況: Windows は実機でインストール・起動・アンインストール、CD再生、システム音声の橋渡し(DAC: Loxjie D40 Pro 相当で384 kHz/32bit)を確認。Linux(Ubuntu、WSL)はインストールと起動まで。Android は実機(Android 14)でMP4/FLACの再生まで。macOS は未確認(GitHub Actions でビルドされますが、実機では動かしていません)。
- 自動アップデート: アプリ起動時に更新を確認し、確認のうえで更新します(デスクトップ版。署名つき)。この仕組みを入れた版より前の版は署名鍵が違うため、手動で入れ替えてください。更新の実機での通し確認: インストーラー版から公開済みの次の版への更新を実機で確認しました。ただし、この機能を入れる前に公開した旧版は、更新の権限が無いため手動で入れ替えてください。
- ライセンス: MIT(各依存クレートのライセンスは個別に従います)。

## ビルド

```
npm install
npm run tauri dev      # 開発起動
npm run tauri build    # インストーラー作成
```

ffmpeg / ffprobe / xorriso は同梱(CI)またはPATH上のものを使います。詳細は [CLAUDE.md](CLAUDE.md) / [PORTING.md](PORTING.md)(make-disk 時代の開発記録を含みます)。
