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

### Deutsch (KI-Übersetzung, redaktionell überarbeitet; noch nicht von Muttersprachlern geprüft)
- **REST wird vollständig abgeschafft; neue APIs entstehen nicht mehr als REST.** APIs, die aus dem Browser oder per `curl` aufgerufen werden, verwenden **unäre Connect-Aufrufe** (`POST /<Paket>.<Service>/<Methode>`, `content-type: application/json`, JSON-Body). Zwischen Diensten kommen **gRPC bzw. gRPC-web** zum Einsatz, im selben Stil wie die Admin-API von RPoem (`open_runo.admin.v1.Admin`). Streaming wird erst ergänzt, wenn es tatsächlich gebraucht wird.
- **Bestehende REST-Endpunkte werden schrittweise umgestellt, und zwar immer dann, wenn man ohnehin an ihnen arbeitet.** Sobald ein Endpunkt umgestellt ist, wird die alte REST-Route entfernt. Bis zum Abschluss der Migration bleiben die übrigen bestehen, es werden aber **keine neuen mehr hinzugefügt**.
- **JSON wird einheitlich über RS-JSON verarbeitet**: Gelesen wird mit `from_str_secure` bzw. `parse_secure` (mit Obergrenzen für Größe, Verschachtelungstiefe und doppelte Schlüssel), geschrieben mit `rust_json::fast`. `prost` wird nicht in das Browser-WASM aufgenommen, da es die Datei vergrößern würde.
- **Ausnahmen (Schnittstellen, deren Form ein fremder Standard vorgibt und die nicht zu unserer eigenen API gehören)**: ACME, OAuth/OIDC-Callbacks, Webhooks von Drittanbietern (z. B. GitHub), Health-Checks, die Auslieferung statischer Dateien, OTLP und Ähnliches. Diese nehmen, wie vom jeweiligen Standard verlangt, gewöhnliches HTTP entgegen.
- Referenzimplementierungen: `RPoem` (`open_runo.admin.v1.Admin`) und `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Hinweis: Ob der Code eines einzelnen Repositorys diese Richtlinie bereits erfüllt, hängt vom jeweiligen Repository ab. Dieser Abschnitt dokumentiert lediglich die Richtlinie; er besagt nicht, dass der Code schon angepasst wurde.

### Русский (перевод выполнен с помощью ИИ и отредактирован; носителями языка пока не проверен)
- **От REST мы полностью отказываемся; новые API больше не создаются в стиле REST.** API, к которым обращаются из браузера или через `curl`, используют **унарные вызовы Connect** (`POST /<пакет>.<Service>/<Method>`, `content-type: application/json`, тело запроса — JSON). Для взаимодействия между сервисами применяются **gRPC и gRPC-web** — в том же стиле, что и административный API RPoem (`open_runo.admin.v1.Admin`). Потоковая передача данных добавляется лишь тогда, когда в ней действительно возникает необходимость.
- **Существующие REST-эндпоинты переводятся поэтапно — по мере того, как с ними приходится работать.** После перевода старый REST-маршрут удаляется. Пока миграция не завершена, оставшиеся эндпоинты сохраняются, но **новые не добавляются**.
- **Работа с JSON унифицирована на основе RS-JSON**: чтение — через `from_str_secure` или `parse_secure` (с ограничениями на размер, глубину вложенности и повторяющиеся ключи), запись — через `rust_json::fast`. `prost` в браузерный WASM не включается, поскольку это увеличило бы размер файла.
- **Исключения (интерфейсы, форму которых определяет чужой стандарт, а не наш собственный API)**: ACME, обратные вызовы OAuth/OIDC, вебхуки сторонних сервисов (например, GitHub), проверки работоспособности, раздача статических файлов, OTLP и тому подобное. Такие интерфейсы принимают обычный HTTP, как того требует соответствующий стандарт.
- Эталонные реализации: `RPoem` (`open_runo.admin.v1.Admin`) и `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Примечание: соответствует ли код конкретного репозитория этой политике, зависит от самого репозитория. Данный раздел лишь фиксирует политику и не означает, что код уже изменён.

## API 方針の補足: 用語の整理と使い分け (2026-11-04)

### 日本語
**1つに統一するものではなく、別の層の話です。** 「REST の代わり」になるのは Connect・gRPC・GraphQL だけで、WASM は画面を作る技術です。

| 用語 | 何か | REST の代わりか |
|---|---|---|
| REST | URL と HTTP メソッドで操作を表す API の作り方 | 全廃する側 |
| **Connect** | `POST /サービス名/メソッド名` に JSON を送る形式。`curl` でもブラウザの `fetch` でも呼べる | はい(**採用**) |
| **gRPC** | 同じ「サービス/メソッド」の考え方を、バイナリ(protobuf)と HTTP/2 で行う形式。速く、ストリーミングも得意 | はい(**サービス同士に採用**) |
| gRPC-web | gRPC のブラウザ向けの変形版 | はい(必要なら) |
| GraphQL | 欲しい項目を問い合わせる形式 | 一部だけ(**問い合わせ(読み取り)専用**) |
| WASM | Rust をブラウザの中で動かす仕組み | いいえ(画面を作る技術) |

- **ブラウザの画面や `curl` から呼ぶ API は Connect**(POST、JSON を RS-JSON で読み書き)。ブラウザの WASM は素の gRPC を話せず、Connect なら `fetch` のままで、`prost` を足さずに済む(画面のサイズも増えない)。
- **サービス同士の通信(サーバー間、重い処理、ストリーミング)は gRPC。** 多くは Connect で足りるので、ストリーミングが必要になった所だけ gRPC に上げる。
- **GraphQL は、まとめて問い合わせたいときの読み取り専用。** 書き込み(操作)には使わない。
- **WASM は画面を作る技術として、そのまま使う。** 画面から呼ぶ API が Connect になっただけ。
- 外部の規格が形を決めるもの(ACME、OAuth のコールバック、第三者の Webhook、ヘルスチェック、静的ファイル、OTLP)は、規格どおりの HTTP のまま。
- **迷ったら Connect。**

### English
**These are not alternatives to unify; they sit at different layers.** Only Connect, gRPC and GraphQL can replace REST; WASM is a way to build the UI.

| Term | What it is | Replaces REST? |
|---|---|---|
| REST | API style that expresses operations through URLs and HTTP methods | The thing being abolished |
| **Connect** | `POST /Service/Method` with a JSON body; callable from `curl` and from a browser `fetch` | Yes (**adopted**) |
| **gRPC** | The same service/method idea over binary (protobuf) and HTTP/2; fast, good at streaming | Yes (**for service-to-service**) |
| gRPC-web | A browser-friendly variant of gRPC | Yes (if needed) |
| GraphQL | A query language: ask for exactly the fields you want | Partly (**read-only queries**) |
| WASM | Runs Rust inside the browser | No (a UI technology) |

- **APIs called from browser screens or `curl` use Connect** (POST, JSON read and written with RS-JSON). Browser WASM cannot speak plain gRPC; with Connect it stays a `fetch`, and `prost` is not needed (the binary does not grow).
- **Service-to-service calls (server to server, heavy work, streaming) use gRPC.** Connect is enough in most places; move a call to gRPC only where streaming is actually needed.
- **GraphQL is read-only**, for queries that gather several pieces of data. It is not used for writes (operations).
- **WASM stays as the way to build the UI.** The API it calls simply became Connect.
- Interfaces whose shape an outside standard dictates (ACME, OAuth callbacks, third-party webhooks, health checks, static files, OTLP) stay plain HTTP.
- **When in doubt, use Connect.**

### Deutsch (KI-Übersetzung, sprachlich überarbeitet; noch nicht von Muttersprachlern geprüft)
**Es geht nicht darum, eine einzige Technik auszuwählen – die Begriffe liegen auf unterschiedlichen Ebenen.** Als Ersatz für REST kommen nur Connect, gRPC und GraphQL in Frage; WASM dagegen ist eine Technik, mit der die Benutzeroberfläche gebaut wird.

| Begriff | Was ist das? | Ersatz für REST? |
|---|---|---|
| REST | Ein API-Stil, bei dem Operationen über URLs und HTTP-Methoden ausgedrückt werden | wird abgeschafft |
| **Connect** | `POST /Dienst/Methode` mit JSON im Body; aufrufbar mit `curl` ebenso wie mit `fetch` im Browser | ja (**gewählt**) |
| **gRPC** | Dasselbe Prinzip aus Dienst und Methode, aber über binäres protobuf und HTTP/2; schnell und gut für Streaming geeignet | ja (**zwischen Diensten**) |
| gRPC-web | Eine für den Browser angepasste Variante von gRPC | ja (bei Bedarf) |
| GraphQL | Abfragesprache, mit der man genau die benötigten Felder anfordert | teilweise (**nur für lesende Abfragen**) |
| WASM | Führt Rust-Code direkt im Browser aus | nein (Technik für die Oberfläche) |

- **APIs, die von Browseroberflächen oder per `curl` aufgerufen werden, verwenden Connect** (POST; JSON wird mit RS-JSON gelesen und geschrieben). WASM im Browser kann kein reines gRPC sprechen. Mit Connect bleibt es bei einem gewöhnlichen `fetch`, und `prost` ist nicht nötig – die WASM-Datei wird dadurch nicht größer.
- **Für die Kommunikation zwischen Diensten (Server zu Server, rechenintensive Aufgaben, Streaming) wird gRPC eingesetzt.** In den meisten Fällen reicht Connect aus; auf gRPC wird nur dort umgestellt, wo Streaming tatsächlich gebraucht wird.
- **GraphQL dient ausschließlich lesenden Abfragen**, wenn mehrere Informationen gebündelt abgefragt werden sollen. Für Schreibzugriffe (Operationen) wird es nicht verwendet.
- **WASM bleibt die Technik für die Oberfläche.** Geändert hat sich lediglich, dass die aufgerufene API nun auf Connect beruht.
- Schnittstellen, deren Form durch einen externen Standard festgelegt ist (ACME, OAuth-Callbacks, Webhooks von Drittanbietern, Health-Checks, statische Dateien, OTLP), bleiben gewöhnliches HTTP.
- **Im Zweifel: Connect.**

### Русский (перевод выполнен с помощью ИИ и литературно отредактирован; носителями языка пока не проверен)
**Это не варианты, из которых нужно выбрать один, а технологии разных уровней.** Заменить REST могут только Connect, gRPC и GraphQL; WASM же — это способ создания пользовательского интерфейса.

| Термин | Что это | Заменяет REST? |
|---|---|---|
| REST | Стиль построения API, при котором операции выражаются через URL и HTTP-методы | отменяется |
| **Connect** | `POST /Сервис/Метод` с телом в формате JSON; вызывается как из `curl`, так и из браузерного `fetch` | да (**выбран**) |
| **gRPC** | Тот же принцип «сервис — метод», но поверх двоичного формата protobuf и HTTP/2; работает быстро и хорошо подходит для потоковой передачи | да (**для связи между сервисами**) |
| gRPC-web | Адаптированный для браузера вариант gRPC | да (при необходимости) |
| GraphQL | Язык запросов: клиент запрашивает ровно те поля, которые ему нужны | частично (**только запросы на чтение**) |
| WASM | Позволяет выполнять код на Rust прямо в браузере | нет (технология для интерфейса) |

- **API, которые вызываются из браузерных интерфейсов или через `curl`, строятся на Connect** (POST; JSON читается и записывается через RS-JSON). Браузерный WASM не умеет работать с «чистым» gRPC. С Connect достаточно обычного `fetch`, а `prost` не требуется — размер WASM-файла не увеличивается.
- **Для обмена между сервисами (сервер — сервер, ресурсоёмкие задачи, потоковая передача) применяется gRPC.** В большинстве случаев достаточно Connect; на gRPC переходят только там, где потоковая передача действительно необходима.
- **GraphQL используется только для чтения**, когда нужно получить сразу несколько связанных данных. Для записи (операций) он не применяется.
- **WASM по-прежнему остаётся технологией для интерфейса.** Изменилось лишь то, что вызываемый им API теперь построен на Connect.
- Интерфейсы, форма которых задана внешним стандартом (ACME, обратные вызовы OAuth, вебхуки сторонних сервисов, проверки работоспособности, статические файлы, OTLP), остаются обычным HTTP.
- **Если есть сомнения — выбирайте Connect.**
