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

## 文書と引っ越し

- [`CLAUDE.md`](CLAUDE.md) … 開発ルール・構成・試験の作法・見つけた落とし穴(英語: [`CLAUDE.en.md`](CLAUDE.en.md))
- [`PORTING.md`](PORTING.md) … **別のPCへの引っ越し手順**(道具・鍵の運び方・確認)と、OS別の対応状況(英語: [`PORTING.en.md`](PORTING.en.md))
- `scripts/doctor.ps1` … 新しいPCで、必要な道具がそろっているかの診断
- make-disk 時代の詳しい開発記録: [`docs/make-disk-legacy/`](docs/make-disk-legacy/)

---

作者 / Author: 石 塚 正 浩（MASAHIRO ISHIZUKA） (https://github.com/aon-co-jp)

## API 方針: REST 全廃・gRPC / Connect へ (2026-11-04)

### 日本語
- **REST は全廃する。新しい API は REST で作らない。** ブラウザや `curl` から呼ぶ API は **Connect の単純呼び出し**(`POST /<パッケージ>.<Service>/<Method>`、`content-type: application/json`、本文は JSON)、サービス間は **gRPC / gRPC-web**(RPoem の管理API `open_runo.admin.v1.Admin` と同じ流儀)。ストリーミングは、必要になってから。
- **既存の REST は、触るときに順に移す。** 移したら旧 REST の経路は消す。移行が終わるまで残るが、**新規には追加しない**。
- **JSON は RS-JSON に統一**する(読み込みは `from_str_secure` / `parse_secure` でサイズ・深さ・重複キーの上限つき、書き出しは `rust_json::fast`)。ブラウザの WASM に `prost` は入れない(サイズが増えるため)。
- **例外(自分たちの API ではなく、相手の規格が決めているもの)**: ACME、OAuth/OIDC のコールバック、第三者(GitHub など)からの Webhook、ヘルスチェック、静的ファイル配信、OTLP など。これらは規格どおりの HTTP を受ける。
- 参考実装: `RPoem`(`open_runo.admin.v1.Admin`)、`open-easy-web`(`server/src/connect.rs`、`proto/open_easy_web/admin/v1/admin.proto`)。
- ※ このリポジトリのコードが既に方針どおりかどうかは、各リポジトリの現状による。この節は方針の記録であり、コードの変更を意味しない。

### English
- **REST is being abolished. New APIs are not built as REST.** APIs called from browsers or `curl` use **Connect unary calls** (`POST /<package>.<Service>/<Method>`, `content-type: application/json`, JSON body); service-to-service calls use **gRPC / gRPC-web** (the same style as RPoem's admin API `open_runo.admin.v1.Admin`). Streaming only when it is actually needed.
- **Existing REST endpoints are migrated one by one, when they are touched.** The old REST route is removed once migrated. Until migration is finished they remain, but **no new ones are added**.
- **JSON is unified on RS-JSON**: reading with `from_str_secure` / `parse_secure` (size, depth and duplicate-key limits), writing with `rust_json::fast`. `prost` is not put into browser WASM (it would grow the binary).
- **Exceptions (shapes dictated by someone else's standard, not our own API)**: ACME, OAuth/OIDC callbacks, webhooks from third parties (e.g. GitHub), health checks, static file serving, OTLP, and so on. These accept plain HTTP as the standard requires.
- Reference implementations: `RPoem` (`open_runo.admin.v1.Admin`) and `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Note: whether the code in a given repository already follows this policy depends on that repository. This section records the policy; it does not mean the code has been changed.

### Deutsch (KI-Übersetzung, noch nicht von Muttersprachlern geprüft)
- **REST wird abgeschafft. Neue APIs werden nicht als REST gebaut.** APIs, die aus dem Browser oder mit `curl` aufgerufen werden, nutzen **Connect-Unary-Aufrufe** (`POST /<Paket>.<Service>/<Methode>`, `content-type: application/json`, JSON-Body); zwischen Diensten gilt **gRPC / gRPC-web** (wie bei der Admin-API von RPoem, `open_runo.admin.v1.Admin`). Streaming erst, wenn es wirklich nötig ist.
- **Bestehende REST-Endpunkte werden einzeln migriert, sobald man sie anfasst.** Nach der Migration wird die alte REST-Route entfernt. Bis dahin bleiben sie bestehen, es werden aber **keine neuen hinzugefügt**.
- **JSON wird auf RS-JSON vereinheitlicht**: Lesen mit `from_str_secure` / `parse_secure` (Grenzen für Größe, Tiefe und doppelte Schlüssel), Schreiben mit `rust_json::fast`. `prost` kommt nicht in Browser-WASM (die Datei würde größer).
- **Ausnahmen (Formate, die ein fremder Standard vorgibt, nicht unsere eigene API)**: ACME, OAuth/OIDC-Callbacks, Webhooks von Dritten (z. B. GitHub), Health-Checks, statische Dateien, OTLP usw. Diese nehmen schlichtes HTTP entgegen, wie es der Standard verlangt.
- Referenzimplementierungen: `RPoem` (`open_runo.admin.v1.Admin`) und `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Hinweis: Ob der Code eines Repositorys diese Richtlinie schon befolgt, hängt vom jeweiligen Repository ab. Dieser Abschnitt hält die Richtlinie fest; er bedeutet nicht, dass der Code bereits geändert wurde.

### Русский (машинный перевод, носителями языка ещё не проверен)
- **REST полностью отменяется. Новые API не создаются как REST.** API, вызываемые из браузера или через `curl`, используют **унарные вызовы Connect** (`POST /<пакет>.<Service>/<Method>`, `content-type: application/json`, тело — JSON); между сервисами — **gRPC / gRPC-web** (так же, как административный API RPoem, `open_runo.admin.v1.Admin`). Потоковая передача — только когда она действительно нужна.
- **Существующие REST-эндпоинты переносятся по одному, когда до них доходят руки.** После переноса старый REST-маршрут удаляется. До завершения переноса они остаются, но **новые не добавляются**.
- **JSON унифицируется на RS-JSON**: чтение через `from_str_secure` / `parse_secure` (ограничения на размер, глубину и повторяющиеся ключи), запись через `rust_json::fast`. `prost` не добавляется в браузерный WASM (размер вырос бы).
- **Исключения (форматы, заданные чужим стандартом, а не наш собственный API)**: ACME, обратные вызовы OAuth/OIDC, вебхуки сторонних сервисов (например, GitHub), проверки работоспособности, раздача статических файлов, OTLP и т. п. Они принимают обычный HTTP, как требует стандарт.
- Эталонные реализации: `RPoem` (`open_runo.admin.v1.Admin`) и `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Примечание: соответствует ли код конкретного репозитория этой политике, зависит от самого репозитория. Этот раздел фиксирует политику и не означает, что код уже изменён.
