mod engine;
mod progress;

use engine::burn::{self, WriteSpeed};
use engine::capacity::{self, DiscType, MediaKind, QualityWarning};
use engine::convert::{self, ChannelLayoutTarget, ConvertJob, TrimRange as ConvertTrimRange};
use engine::cpu::{self, CpuEncodeEstimate};
use engine::iso;
use engine::pdf::{self, BindingDirection};
use engine::probe;
use tauri::Emitter;

/// 欲しい部分をAIが探して切り出し範囲を提案する(時間がかかり得るため別スレッドで実行し、画面を固めない)。
#[tauri::command]
async fn ai_suggest_range(path: String, request: String, length_secs: f64, llm_url: String) -> Result<engine::ai_range::Suggestion, String> {
    tauri::async_runtime::spawn_blocking(move || engine::ai_range::suggest_range(&path, &request, length_secs, &llm_url))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn probe_media(path: String) -> Result<probe::MediaInfo, String> {
    probe::probe(&path)
}

/// 変換を実行する。数時間かかり得る(AI超解像など)ので、別スレッドで動かして画面を固めない。
#[tauri::command]
async fn convert_media(job: ConvertJob) -> Result<(), String> {
    if job.ai_upscale.is_some() {
        progress::clear_cancel();
    }
    tauri::async_runtime::spawn_blocking(move || convert::run_convert(&job)).await.map_err(|e| e.to_string())?
}

/// 実行中の長い処理(AI超解像など)を中止する。途中経過は残り、同じ設定でもう一度実行すると続きから再開する。
#[tauri::command]
fn cancel_conversion() {
    progress::request_cancel();
}

/// このPCのCPU・GPUの速さを測って、AI処理に使う装置を自動で選ぶ(初回は1〜数分かかる)。`force`で再測定。
#[tauri::command]
async fn ai_hw_benchmark(force: bool) -> Result<engine::hw_bench::HwBench, String> {
    tauri::async_runtime::spawn_blocking(move || {
        engine::hw_bench::benchmark(force, &|m| progress::emit("ai-progress", serde_json::json!({ "stage": "info", "message": m })))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 保存済みの測定結果(無ければnull)。
#[tauri::command]
fn ai_hw_status() -> Option<engine::hw_bench::HwBench> {
    engine::hw_bench::cached()
}

/// open-easy-webの配置(一番上のフォルダと、起動時に行った整備の説明)。
static LAYOUT_NOTES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

#[tauri::command]
fn open_easy_web_layout() -> serde_json::Value {
    serde_json::json!({
        "root": engine::layout::root_dir().map(|p| p.to_string_lossy().to_string()),
        "notes": LAYOUT_NOTES.lock().map(|n| n.clone()).unwrap_or_default(),
    })
}

/// 「選んだディスクに、この解像度・fpsで収まるか」の予測(フルHD/4K × 元のfps/60/120)。静止・単色コマの割合も抜き取りで推定する。
#[tauri::command]
async fn ai_fit_predict(path: String, start_secs: Option<f64>, duration_secs: Option<f64>, disc: DiscType, audio_kbps: f64, src_fps: f64) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let probe = engine::probe::probe(&path)?;
        let dur = duration_secs.unwrap_or((probe.duration_secs - start_secs.unwrap_or(0.0)).max(0.0));
        let static_fraction = engine::fit_predict::estimate_static_fraction(&path, start_secs, duration_secs);
        let mut fps_list = vec![src_fps];
        for f in [60.0, 120.0] {
            if f >= src_fps * 1.5 {
                fps_list.push(f);
            }
        }
        let mut rows = Vec::new();
        for (w, h) in [(1920u32, 1080u32), (3840, 2160)] {
            for &fps in &fps_list {
                let r = engine::fit_predict::predict(disc, dur, audio_kbps, static_fraction.unwrap_or(0.0), &engine::fit_predict::FitOption { width: w, height: h, fps });
                rows.push(r);
            }
        }
        Ok(serde_json::json!({ "duration_secs": dur, "static_fraction": static_fraction, "rows": rows }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// AI超解像の下調べ: 映像の種類(インターレース・黒帯)、コマ数、このPCでの所要時間、作業用の空き容量。
#[tauri::command]
async fn ai_estimate(path: String, start_secs: Option<f64>, duration_secs: Option<f64>, options: engine::ai_upscale::AiUpscale, target_w: Option<u32>, target_h: Option<u32>, work_dir: String) -> Result<engine::ai_video::Estimate, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = target_w.zip(target_h);
        engine::ai_video::estimate(&path, start_secs, duration_secs, &options, target, std::path::Path::new(&work_dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 「AI判断で自動カット」モード(2026-09-16新設)向け: 無音区間を検出する。
/// **正直な開示**: 実際にはffmpegの音量ベースの無音検出であり、LLM/
/// 画像認識等の意味的なAI判断ではない(`convert::detect_silence_ranges`
/// のdocコメント参照)。
#[tauri::command]
fn detect_silence_ranges(path: String, silence_threshold_db: f64, min_silence_secs: f64) -> Result<Vec<convert::SilenceRange>, String> {
    convert::detect_silence_ranges(&path, silence_threshold_db, min_silence_secs)
}

/// 「サイズ指定」モード(2026-09-16新設)向け: 目標ファイルサイズと
/// 総尺から平均ビットレート(kbps)を算出する。
#[tauri::command]
fn calc_bitrate_for_target_size_kbps(target_bytes: u64, total_duration_secs: f64) -> u64 {
    convert::bitrate_for_target_size_kbps(target_bytes, total_duration_secs)
}

#[tauri::command]
fn calc_auto_bitrate_kbps(disc: DiscType, total_duration_secs: f64, reserved_bytes: u64) -> u64 {
    capacity::max_bitrate_for_capacity(disc, total_duration_secs, reserved_bytes) / 1000
}

#[tauri::command]
fn check_bitrate_quality(bitrate_kbps: u64, kind: MediaKind) -> Option<QualityWarning> {
    capacity::quality_warning(bitrate_kbps * 1000, kind)
}

/// 「最高音質」モード(2026-09-16新設)向け:CD品質ロスレスWAVで
/// 指定ディスクに収まるかどうか、収まらない場合は代わりに何秒までなら
/// 収まるかを算出する。
#[tauri::command]
fn estimate_lossless_audio_fit(disc: DiscType, total_duration_secs: f64, reserved_bytes: u64) -> capacity::LosslessFitEstimate {
    capacity::estimate_lossless_audio_fit(disc, total_duration_secs, reserved_bytes)
}

/// 一般ビットレートでの容量見積もり(音声/CD・動画のいずれにも使える、2026-09-28新設)。
/// 「収まるか予測」「下調べ」を動画のアップコンバートだけに限定せず、通常の音声変換や
/// CD取り込みでも使えるようにする。
#[tauri::command]
fn estimate_bitrate_fit(disc: DiscType, total_duration_secs: f64, bitrate_kbps: u64, reserved_bytes: u64) -> capacity::BitrateFitEstimate {
    capacity::estimate_bitrate_fit(disc, total_duration_secs, bitrate_kbps, reserved_bytes)
}

/// チャンネル構成の変換(モノ/ステレオ⇔5.1ch/7.1ch、2026-09-28新設)に必要な
/// ffmpeg引数を、元のチャンネル数と変換先から組み立てる。
#[tauri::command]
fn channel_layout_args(source_channels: u32, target: ChannelLayoutTarget) -> Vec<String> {
    convert::channel_layout_args(source_channels, target)
}

/// AIアップミックス(HT-Demucsによる実音源分離、2026-09-28新設)が、このマシンで
/// 実際に使えるか(モデルファイルが用意されているか)の軽い確認。
#[tauri::command]
fn ai_upmix_available() -> bool {
    engine::ai_upmix::is_available()
}

/// 音声/動画ソースファイルから、AIアップミックス済み(5.1ch/7.1ch)のWAVを作る。
#[tauri::command]
fn ai_upmix_convert(input_path: String, output_path: String, target: ChannelLayoutTarget) -> Result<(), String> {
    engine::ai_upmix::upmix_source_file(&input_path, std::path::Path::new(&output_path), target)
}

/// 出力先フォルダのあるドライブの空き容量(バイト)。取得できなければNone。
/// 「下調べ」を動画のAI超解像だけでなく、音声/CD処理でも使えるようにするための
/// 汎用コマンド(2026-09-28新設、既存のengine::ai_video::free_space_bytesを再利用)。
#[tauri::command]
fn free_space_bytes(path: String) -> Option<u64> {
    engine::ai_video::free_space_bytes(std::path::Path::new(&path))
}

/// PDF見開き対応(2026-09-16新設): PDFを見開き画像群に変換して
/// `output_dir`へ出力する。`pdf::render_pdf_as_spreads`のdocコメントに
/// 記載の通り、現時点ではpoppler-utils(`pdftoppm`/`pdfinfo`)が
/// 実行環境のPATHに存在する必要がある(まだsidecar同梱は未対応)。
#[tauri::command]
fn convert_pdf_to_spreads(pdf_path: String, output_dir: String, binding: BindingDirection, max_dimension: u32) -> Result<Vec<String>, String> {
    // ユーザー指示「最大4Kの見開きPDF対応」通り、4Kを超える指定は常に4Kへ丸める。
    let clamped = max_dimension.min(pdf::MAX_4K_DIMENSION);
    pdf::render_pdf_as_spreads(&pdf_path, &output_dir, binding, clamped)
}

/// DSD出力のおおよそのファイルサイズ(バイト)。UIで容量超過を事前に警告するために使う。
#[tauri::command]
fn estimate_dsd_size(multiplier: u32, channels: u32, duration_secs: f64) -> Result<u64, String> {
    engine::dsd::estimate_dsd_size_bytes(multiplier, channels, duration_secs)
}

/// AI超解像のCPU版が使う計算カーネル(`AVX2+FMA`または`scalar`、open-cpuの検出結果)。UIの実行環境表示用。
#[tauri::command]
fn ai_upscale_cpu_kernel() -> String {
    engine::cpu_sr::kernel_name().to_string()
}

/// 画像1枚をAI超解像する(GPUがあればGPU、無ければCPU版)。初回のみプラグイン(約45MB)をダウンロードする。
#[tauri::command]
fn ai_upscale_image(input: String, output: String, model: String, scale: u32, backend: Option<String>) -> Result<(), String> {
    engine::ai_upscale::upscale_image(&input, &output, &engine::ai_upscale::AiUpscale { model, scale, backend: backend.unwrap_or_else(|| "auto".to_string()), ..Default::default() })
}

/// フォルダ内の全ファイルの合計サイズ(バイト)。ISO化・書き込みの前にディスク容量へ収まるか確かめるために使う。
#[tauri::command]
fn folder_size_bytes(path: String) -> Result<u64, String> {
    fn walk(dir: &std::path::Path) -> std::io::Result<u64> {
        let mut total = 0;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            total += if meta.is_dir() { walk(&entry.path())? } else { meta.len() };
        }
        Ok(total)
    }
    walk(std::path::Path::new(&path)).map_err(|e| format!("フォルダのサイズを調べられません: {e}"))
}

/// ディスク種別の実用容量(バイト、公称の約98%)。
#[tauri::command]
fn disc_usable_bytes(disc: DiscType) -> u64 {
    disc.usable_bytes()
}

/// 複数の音声/動画ファイルを結合(合成)する(2026-09-16新設)。
#[tauri::command]
fn concat_media_files(input_paths: Vec<String>, output_path: String, has_video: bool) -> Result<(), String> {
    convert::concat_media(&input_paths, &output_path, has_video)
}

/// 等間隔分割(2026-09-16新設): 総尺を指定個数の等しい区間に分割する。
#[tauri::command]
fn calc_equal_interval_segments(total_secs: f64, segment_count: u32) -> Vec<ConvertTrimRange> {
    convert::equal_interval_segments(total_secs, segment_count)
}

/// サイズ指定分割(2026-09-16新設): 指定した長さごとに区切り、
/// 割り切れない最後の「あまり」は短い区間としてそのまま返す
/// (呼び出し側〈フロントエンド〉で、あまりの区間だけディスク容量
/// いっぱいにビットレートを自動調整する想定)。
#[tauri::command]
fn calc_fixed_length_segments(total_secs: f64, segment_secs: f64) -> Vec<ConvertTrimRange> {
    convert::fixed_length_segments(total_secs, segment_secs)
}

/// PDFの綴じ方向一括変換(2026-09-16新設): 複数のPDFのページ順序を
/// まとめて反転し、`output_dir`へ`<元のファイル名>-rebind.pdf`として保存する。
/// 失敗したファイルはエラーメッセージ付きで結果に含め、他のファイルの
/// 処理は継続する(1件の失敗で全体を止めない)。
#[tauri::command]
fn rebind_pdfs(pdf_paths: Vec<String>, output_dir: String) -> Vec<Result<String, String>> {
    pdf_paths
        .into_iter()
        .map(|input_path| {
            let stem = std::path::Path::new(&input_path).file_stem().and_then(|s| s.to_str()).unwrap_or("output").to_string();
            let output_path = format!("{output_dir}/{stem}-rebind.pdf");
            pdf::reverse_pdf_page_order(&input_path, &output_path).map(|_| output_path)
        })
        .collect()
}

#[tauri::command]
fn create_iso(source_dir: String, output_iso: String, volume_label: String) -> Result<(), String> {
    iso::create_iso(&source_dir, &output_iso, &volume_label)
}

/// 音声/動画/PDFのような変換対象ではない任意のファイル(文書・画像・アーカイブ・
/// 既にエンコード済みで再変換したくないファイル等)を、無変換のまま出力フォルダへ
/// コピーする。出力フォルダはISO化(`create_iso`)でそのまま丸ごと1枚のディスクへ
/// まとめられるため、これにより「色々な形式のファイルを1枚のディスクへ」まとめられる。
/// 同名ファイルがあれば連番を付けて衝突を避ける。コピー後の実際のパスを返す。
#[tauri::command]
fn copy_source_as_is(source_path: String, output_folder: String) -> Result<String, String> {
    let src = std::path::Path::new(&source_path);
    let file_name = src.file_name().ok_or_else(|| "ファイル名を取得できません".to_string())?;
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_string();
    let ext = src.extension().and_then(|s| s.to_str()).map(|s| format!(".{s}")).unwrap_or_default();
    let mut dest = std::path::Path::new(&output_folder).join(file_name);
    let mut n = 1;
    while dest.exists() {
        dest = std::path::Path::new(&output_folder).join(format!("{stem}-{n}{ext}"));
        n += 1;
    }
    std::fs::copy(&src, &dest).map_err(|e| format!("コピーできません({source_path}): {e}"))?;
    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
fn burn_image(image_path: String, device: String, disc: DiscType, speed: WriteSpeed) -> Result<(), String> {
    burn::burn_image(&image_path, &device, disc, speed)
}

#[tauri::command]
fn list_burn_devices() -> Result<Vec<String>, String> {
    burn::list_devices()
}

/// rs-ffmpeg/rs-xorrisoプラグインの状態(版・同期結果)を返す。同じ版なら再コピーしない。
#[tauri::command]
fn list_plugins() -> Vec<engine::plugins::PluginStatus> {
    engine::plugins::sync_bundled_plugins()
}

/// 音楽CD(CD-DA)のトラック一覧(Windowsのみ)。
#[tauri::command]
fn list_cd_tracks(drive: String) -> Result<Vec<engine::cdda::TrackInfo>, String> {
    engine::cdda::list_tracks(&drive)
}

/// 音楽CDの1トラックを一時フォルダへ展開して再生用のWAVパスを返す(プレーヤー用、通常読み取り)。
#[tauri::command]
async fn prepare_cd_track_for_playback(drive: String, track: u8) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = std::env::temp_dir().join("make-disk-player");
        engine::cdda::rip_tracks(&drive, &[track], &dir, false).map(|v| v.into_iter().next().unwrap_or_default())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn bar_modes() -> Vec<open_bar::player::ModeText> { engine::bar_player::modes() }
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn bar_play(path: String, mode: String, treble: bool, treble_all: bool, bits: u16) -> Result<(), String> { engine::bar_player::play(&path, &mode, treble, treble_all, bits) }
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn bar_pause() { engine::bar_player::pause() }
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn bar_resume() { engine::bar_player::resume() }
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn bar_stop() { engine::bar_player::stop() }
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn bar_seek(secs: f64) { engine::bar_player::seek(secs) }
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn bar_set_volume(volume: f32) { engine::bar_player::set_volume(volume) }
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn bar_set_mode(mode: String) -> Result<(), String> { engine::bar_player::set_mode(&mode) }
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn bar_set_treble(on: bool) { engine::bar_player::set_treble(on) }
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn bar_status() -> open_bar::player::Status { engine::bar_player::status() }
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn bar_is_mqa(path: String) -> bool { engine::bar_player::is_mqa(&path) }
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn bar_auto_pair(folder: String) -> Vec<open_bar::combo::Combo> { engine::bar_player::auto_pair(&folder) }
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn bar_save_combo(video: String, audio: String, audio_offset_ms: i64, out_path: String) -> Result<(), String> {
    engine::bar_player::save_combo(&video, &audio, audio_offset_ms, &out_path)
}


// モバイルではopen-barエンジンが無い: 画面側はこの応答を見て、音声も<video>で再生する。
#[cfg(any(target_os = "android", target_os = "ios"))]
const BAR_NA: &str = "このOSではopen-barエンジンは使えません(WebViewで再生します) / open-bar engine is unavailable on this OS (using the WebView player)";
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
fn bar_modes() -> Vec<serde_json::Value> { vec![] }
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
fn bar_play(_path: String, _mode: String, _treble: bool, _treble_all: bool, _bits: u16) -> Result<(), String> { Err(BAR_NA.into()) }
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
fn bar_pause() {}
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
fn bar_resume() {}
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
fn bar_stop() {}
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
fn bar_seek(_secs: f64) {}
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
fn bar_set_volume(_volume: f32) {}
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
fn bar_set_mode(_mode: String) -> Result<(), String> { Err(BAR_NA.into()) }
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
fn bar_set_treble(_on: bool) {}
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
fn bar_status() -> serde_json::Value { serde_json::Value::Null }
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
fn bar_is_mqa(_path: String) -> bool { false }
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
fn bar_auto_pair(_folder: String) -> Vec<serde_json::Value> { vec![] }
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
fn bar_save_combo(_video: String, _audio: String, _audio_offset_ms: i64, _out_path: String) -> Result<(), String> { Err(BAR_NA.into()) }

/// 起動時の引数(自動再生から `--play-disc D:\` で起動されたときにJS側が読む)。
#[tauri::command]
fn startup_args() -> Vec<String> { std::env::args().skip(1).collect() }

/// Windowsの自動再生にopen-mediaを登録して、音楽CD・DVDの既定にする(Windowsのみ、ユーザーの設定)。
#[tauri::command]
fn autoplay_enable() -> Result<String, String> { engine::autoplay::enable() }
#[tauri::command]
fn autoplay_disable() -> Result<(), String> { engine::autoplay::disable() }
#[tauri::command]
fn autoplay_status() -> bool { engine::autoplay::is_enabled() }

/// 起動時の引数(自動再生から `--play-disc D:\` や、ファイルを開くときの `"file"` でJS側が読む)。

/// Windowsの自動再生にopen-mediaを登録して、音楽CD・DVDの既定にする(Windowsのみ、ユーザーの設定)。

/// ファイルの関連付け(Windows): 拡張子ごとに今のアプリを調べ、open-mediaを開くアプリの候補に登録する。
#[tauri::command]
fn assoc_list(exts: Vec<String>) -> Vec<engine::assoc::AssocInfo> { engine::assoc::list(&exts) }
#[tauri::command]
fn assoc_register(exts: Vec<String>) -> Result<usize, String> { engine::assoc::register(&exts) }
#[tauri::command]
fn assoc_unregister(exts: Vec<String>) { engine::assoc::unregister(&exts) }
#[tauri::command]
fn open_default_apps_settings() -> Result<(), String> { engine::assoc::open_default_apps_settings() }

/// システム音声のアップサンプリング(ループバック橋渡し、Windowsのみ)。
#[cfg(windows)]
#[tauri::command]
fn bridge_devices() -> Result<Vec<(String, bool)>, String> { engine::bridge::devices() }
#[cfg(windows)]
#[tauri::command]
async fn bridge_start(capture: String, output: String, max_rate: u32, bits: u16) -> Result<open_bar::loopback::BridgeStatus, String> {
    tauri::async_runtime::spawn_blocking(move || engine::bridge::start(&capture, &output, max_rate, bits)).await.map_err(|e| e.to_string())?
}
#[cfg(windows)]
#[tauri::command]
fn bridge_stop() { engine::bridge::stop() }
#[cfg(windows)]
#[tauri::command]
fn bridge_status() -> Option<open_bar::loopback::BridgeStatus> { engine::bridge::status() }

#[cfg(not(windows))]
const BRIDGE_NA: &str = "システム音声の橋渡しは現在Windowsのみ対応です(Linuxは同梱のPipeWire設定を使います) / System-audio bridging is Windows-only for now (Linux: use the bundled PipeWire config)";
#[cfg(not(windows))]
#[tauri::command]
fn bridge_devices() -> Result<Vec<(String, bool)>, String> { Err(BRIDGE_NA.into()) }
#[cfg(not(windows))]
#[tauri::command]
fn bridge_start(_capture: String, _output: String, _max_rate: u32, _bits: u16) -> Result<serde_json::Value, String> { Err(BRIDGE_NA.into()) }
#[cfg(not(windows))]
#[tauri::command]
fn bridge_stop() {}
#[cfg(not(windows))]
#[tauri::command]
fn bridge_status() -> Option<serde_json::Value> { None }

/// 書き込み非対応のDVD-ROM等も含む全光学ドライブ。
#[tauri::command]
fn list_optical_drives() -> Result<Vec<String>, String> {
    engine::disc_player::list_optical_drives()
}

/// DVD-Video/Blu-ray/データディスクの再生項目(タイトル・ストリーム・ファイル)。
#[tauri::command]
async fn list_disc_items(drive: String) -> Result<Vec<engine::disc_player::DiscItem>, String> {
    tauri::async_runtime::spawn_blocking(move || engine::disc_player::list_items(&drive)).await.map_err(|e| e.to_string())?
}

/// 再生用ファイルを用意する(ブラウザ非対応形式はMP4/WAVへ変換)。
#[tauri::command]
async fn prepare_disc_item_for_playback(paths: Vec<String>, video: bool, target_height: Option<u32>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || engine::disc_player::prepare(&paths, video, target_height)).await.map_err(|e| e.to_string())?
}

/// 再生のアップサンプリング先レートをaruaru-llmに判断させる(失敗時は整数倍の規則)。
#[tauri::command]
async fn ai_playback_rate(path: String, llm_url: String) -> Result<engine::ai_playback::RateDecision, String> {
    tauri::async_runtime::spawn_blocking(move || engine::ai_playback::decide(&path, &llm_url)).await.map_err(|e| e.to_string())?
}

/// 再生用に音声を高精度リサンプラ(soxr等)で`sample_rate`へアップサンプリングした24bit WAVを一時フォルダへ作る。
/// 失敗時は呼び出し側が元のファイルをそのまま再生する。
#[tauri::command]
async fn upsample_for_playback(input_path: String, sample_rate: u32) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = std::env::temp_dir().join("make-disk-player");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let stem = std::path::Path::new(&input_path).file_stem().and_then(|s| s.to_str()).unwrap_or("track");
        let out = dir.join(format!("{stem}-{sample_rate}.wav"));
        let af = format!("{}:out_sample_rate={sample_rate}", convert::hq_resample_filter());
        let st = engine::sidecar::resolve_tool("ffmpeg")
            .args(["-y", "-hide_banner", "-v", "error", "-i", &input_path, "-vn", "-af", &af, "-ar", &sample_rate.to_string(), "-c:a", "pcm_s24le"])
            .arg(&out)
            .output()
            .map_err(|e| format!("ffmpegを起動できません: {e}"))?;
        if !st.status.success() {
            return Err(String::from_utf8_lossy(&st.stderr).to_string());
        }
        Ok(out.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 音楽CDのトラックをWAV(16bit/44.1kHz)として取り込む。`secure`なら各区間を2回読んで一致確認する。
#[tauri::command]
fn rip_cd_tracks(drive: String, tracks: Vec<u8>, output_dir: String, secure: bool) -> Result<Vec<String>, String> {
    engine::cdda::rip_tracks(&drive, &tracks, std::path::Path::new(&output_dir), secure)
}

#[tauri::command]
fn estimate_cpu_encode_speed() -> CpuEncodeEstimate {
    cpu::estimate_cpu_encode_speed()
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn pick_output_tree(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_android_folder::AndroidFolderExt;
    app.android_folder().pick_output_tree().map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 同梱のrs-*プラグインをプラグインフォルダへ同期する(同じ版ならスキップ)。
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let notes = engine::layout::ensure_layout();
        *LAYOUT_NOTES.lock().unwrap() = notes;
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let _ = engine::plugins::sync_bundled_plugins();

    let builder = tauri::Builder::default();

    // 同時に2つ起動しない(Windowsの自動再生で何度も起動されても、開いているアプリへ引数を渡して前面に出す)。
    // 公式の手順どおり、他のプラグインより先に登録する。モバイルには無い。
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
        use tauri::Manager;
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
        // 2つ目の起動の引数(`--play-disc D:\`、開いたファイル等)を、画面側へ渡す。
        let _ = app.emit("second-instance", argv.into_iter().skip(1).collect::<Vec<String>>());
    }));

    let builder = builder
        .setup(|app| {
            // 長い処理の進捗を画面へ届ける出口(エンジンはAppHandleを知らなくてよい)。
            let handle = app.handle().clone();
            progress::set_sink(Box::new(move |kind, payload| {
                let _ = handle.emit("make-disk-progress", serde_json::json!({ "kind": kind, "payload": payload }));
            }));
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init());

    // 自動アップデート確認(2026-09-16新設、デスクトップのみ——
    // `tauri-plugin-updater`はモバイル未対応、Android/iOSはストア/APK
    // サイドロードでの更新が前提のため元々対象外)。
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build()).plugin(tauri_plugin_process::init());

    #[cfg(target_os = "android")]
    let builder = builder
        .plugin(tauri_plugin_android_folder::init())
        .invoke_handler(tauri::generate_handler![
            probe_media,
            prepare_cd_track_for_playback,
            upsample_for_playback,
            ai_playback_rate,
            list_optical_drives,
            startup_args,
            autoplay_enable,
            autoplay_disable,
            autoplay_status,
            assoc_list,
            assoc_register,
            assoc_unregister,
            open_default_apps_settings,
            bridge_devices,
            bridge_start,
            bridge_stop,
            bridge_status,
            bar_modes,
            bar_play,
            bar_pause,
            bar_resume,
            bar_stop,
            bar_seek,
            bar_set_volume,
            bar_set_mode,
            bar_set_treble,
            bar_status,
            bar_is_mqa,
            bar_auto_pair,
            bar_save_combo,
            list_disc_items,
            prepare_disc_item_for_playback,
            convert_media,
            cancel_conversion,
            ai_hw_benchmark,
            ai_hw_status,
            ai_estimate,
            ai_fit_predict,
            open_easy_web_layout,
            calc_auto_bitrate_kbps,
            check_bitrate_quality,
            create_iso,
            burn_image,
            list_burn_devices,
            estimate_cpu_encode_speed,
            estimate_lossless_audio_fit,
            detect_silence_ranges,
            calc_bitrate_for_target_size_kbps,
            convert_pdf_to_spreads,
            rebind_pdfs,
            estimate_dsd_size,
            list_plugins,
            ai_upscale_image,
            ai_upscale_cpu_kernel,
            folder_size_bytes,
            disc_usable_bytes,
            concat_media_files,
            calc_equal_interval_segments,
            calc_fixed_length_segments,
            list_cd_tracks,
            rip_cd_tracks,
            ai_suggest_range,
            pick_output_tree,
            copy_source_as_is,
            estimate_bitrate_fit,
            free_space_bytes,
            channel_layout_args,
            ai_upmix_available,
            ai_upmix_convert,
        ]);

    #[cfg(not(target_os = "android"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        probe_media,
        prepare_cd_track_for_playback,
        upsample_for_playback,
        ai_playback_rate,
        list_optical_drives,
        bar_modes,
        bar_play,
        bar_pause,
        bar_resume,
        bar_stop,
        bar_seek,
        bar_set_volume,
        bar_set_mode,
        bar_set_treble,
        bar_status,
        bar_is_mqa,
        bar_auto_pair,
        bar_save_combo,
        list_disc_items,
        prepare_disc_item_for_playback,
        convert_media,
        cancel_conversion,
        ai_hw_benchmark,
        ai_hw_status,
        ai_estimate,
        ai_fit_predict,
        open_easy_web_layout,
        calc_auto_bitrate_kbps,
        check_bitrate_quality,
        create_iso,
        burn_image,
        list_burn_devices,
        estimate_cpu_encode_speed,
        estimate_lossless_audio_fit,
        detect_silence_ranges,
        calc_bitrate_for_target_size_kbps,
        convert_pdf_to_spreads,
        rebind_pdfs,
        estimate_dsd_size,
        list_plugins,
        ai_upscale_image,
        ai_upscale_cpu_kernel,
        folder_size_bytes,
        disc_usable_bytes,
        concat_media_files,
        calc_equal_interval_segments,
        calc_fixed_length_segments,
        list_cd_tracks,
        rip_cd_tracks,
        ai_suggest_range,
        copy_source_as_is,
        estimate_bitrate_fit,
        free_space_bytes,
        channel_layout_args,
        ai_upmix_available,
        ai_upmix_convert,
        startup_args,
        autoplay_enable,
        autoplay_disable,
        autoplay_status,
        assoc_list,
        assoc_register,
        assoc_unregister,
        open_default_apps_settings,
        bridge_devices,
        bridge_start,
        bridge_stop,
        bridge_status,
    ]);

    builder.run(tauri::generate_context!()).expect("error while running tauri application");
}
