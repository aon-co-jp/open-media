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

## 9. 公開サイト(easy-web.tokyo)の引っ越し

- **紹介・ダウンロード** `https://easy-web.tokyo/open-media/` = リポジトリの `webpage/index.html`(最新リリースをGitHub APIから自動表示)。**デモ** `/open-media/demo/` = `webpage/demo/`(`python scripts/build-demo.py` で `src/` から作る)。
- **VPSの置き場所**: `/root/easy-web.tokyo/open-media`(リポジトリのclone)。更新は `git pull --ff-only` だけ(再起動不要)。
- **配信**: `/etc/systemd/system/open-media-web.service`(`WorkingDirectory=/root/easy-web.tokyo/open-media/webpage`、`ExecStart=/usr/bin/python3 -m http.server 8117 --bind 127.0.0.1`、`Restart=always`)+ `open-web-server` の `domains.toml` に `host="easy-web.tokyo"`、`backend="open_runo"`、`backend_addr="127.0.0.1:8117"`、`path_prefix="/open-media"`、`strip_prefix=true`。前面は nginx ではなく `open-web-server`。
- **トップページの紹介カード**: `open-easy-web-app/server/src/main.rs` の `media_banner_html()`。ビルドには `vps-tidy`・`RS-JSON`・`RPoem`・`open-raid-z` が横に必要(`/root/repository/` 以下)。
- 新しいVPSへ移すときは、上の4点(clone・unit・domains.toml・トップのカード)を作り直し、`curl -I https://easy-web.tokyo/open-media/` と `/open-media/demo/` が200か確かめる。
