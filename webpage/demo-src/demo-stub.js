// open-media のWebデモ用「エンジンの代役」。実際のアプリの画面(src/)をそのままブラウザで動かし、
// Tauriの命令(Rustの処理)は、ここで疑似的に返す。音は出ず、ファイルにも触れない。
// A stand-in for the engine so the real UI (src/) runs in the browser. Everything below is simulated:
// no sound is played and no file is touched.
(() => {
  const now = () => Date.now() / 1000;
  const TRACK_SECS = 241;
  const player = { state: "stopped", startedAt: 0, base: 0, pausedAt: 0, mode: "exclusive_upsample", bits: 32, name: "" };
  const modes = [
    { id: "shared", letter: "B", title_ja: "共有モード・高品質アップサンプル", title_en: "Shared mode · high-quality upsampling", desc_ja: "Windowsのミキサーを経由し、デバイスの設定レートへ高品質のsinc補間で変換して再生します。", desc_en: "Plays through the Windows mixer, converting to the device's configured rate with a high-quality sinc resampler.", bit_perfect: false },
    { id: "exclusive", letter: "A", title_ja: "排他モード・ビットパーフェクト", title_en: "Exclusive mode · bit-perfect", desc_ja: "元のサンプルレートのままDACへ直接送ります(変換・音量処理なし)。", desc_en: "Sends the original sample rate straight to the DAC (no conversion, no volume processing).", bit_perfect: true },
    { id: "exclusive_upsample", letter: "E", title_ja: "排他モード・高品質アップサンプル", title_en: "Exclusive mode · high-quality upsampling", desc_ja: "高品質のsinc補間で高いレート(例: 352.8kHz/384kHz)へ変換してDACへ直接送ります。", desc_en: "Converts to a higher rate (e.g. 352.8/384 kHz) with a high-quality sinc resampler and sends it straight to the DAC.", bit_perfect: false },
    { id: "dop", letter: "D", title_ja: "DoP・DSDのままDACへ", title_en: "DoP · DSD as-is to the DAC", desc_ja: "DSDをDoPに載せ、排他モードでDACへ直接送ります(対応DACのみ)。", desc_en: "Packs DSD as DoP and sends it in exclusive mode (DoP-capable DACs only).", bit_perfect: true },
  ];
  const modeOf = (id) => modes.find((m) => m.id === id) || modes[2];
  const position = () => {
    if (player.state === "playing") return Math.min(TRACK_SECS, player.base + (now() - player.startedAt));
    if (player.state === "paused") return player.base;
    return 0;
  };
  const rateText = () => {
    if (player.mode === "exclusive_upsample") return ["PCM 44.1 kHz → 352.8 kHz(sincで変換) / " + player.bits + "bit", "PCM 44.1 kHz → converted to 352.8 kHz (sinc resampler) / " + player.bits + "-bit"];
    if (player.mode === "exclusive") return ["PCM 44.1 kHz をそのまま出力(ビットパーフェクト)", "PCM 44.1 kHz sent as is (bit-perfect)"];
    if (player.mode === "dop") return ["DSDをDoPで送出(対応DAC向け)", "DSD sent as DoP (DoP-capable DACs)"];
    return ["PCM 44.1 kHz → デバイスのレートへ変換", "PCM 44.1 kHz → converted to the device rate"];
  };
  const tracks = Array.from({ length: 8 }, (_, i) => ({ number: i + 1, is_audio: true, sectors: (180 + ((i * 37) % 120)) * 75 }));
  const assocApps = ["メディア プレーヤー / Media Player (Microsoft Store)", "未設定 / not set"];
  let autoplayOn = false;
  const registered = new Set();
  const ASSOC_ORDER = ["cda", "mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "dsf", "dff", "mka", "mp4", "mkv", "avi", "mov", "webm", "mpg", "vob", "ifo", "m2ts", "iso"];
  let bridge = null;

  const invoke = async (cmd, args = {}) => {
    switch (cmd) {
      // ── 再生エンジン(open-bar)の代役 ──
      case "bar_modes": return modes;
      case "bar_is_mqa": return false;
      case "bar_play": player.state = "playing"; player.base = 0; player.startedAt = now(); player.mode = args.mode || "exclusive_upsample"; player.bits = args.bits || 32; return null;
      case "bar_pause": if (player.state === "playing") { player.base = position(); player.state = "paused"; } return null;
      case "bar_resume": if (player.state === "paused") { player.startedAt = now(); player.state = "playing"; } return null;
      case "bar_stop": player.state = "stopped"; player.base = 0; return null;
      case "bar_seek": player.base = Math.max(0, Math.min(TRACK_SECS, args.secs || 0)); player.startedAt = now(); return null;
      case "bar_set_volume": case "bar_set_treble": return null;
      case "bar_set_mode": player.mode = args.mode; return null;
      case "bar_status": {
        const pos = position();
        if (player.state === "playing" && pos >= TRACK_SECS) { player.state = "stopped"; player.base = 0; }
        const [ja, en] = rateText();
        return { state: player.state, index: 0, path: "demo", position_secs: pos, duration_secs: TRACK_SECS, device: "デモ用の仮想DAC / Demo virtual DAC", out_rate_hz: 352800, source_rate_hz: 44100, source_kind: "PCM", volume: 1, selected_mode: player.mode, active_mode: modeOf(player.mode), route_ja: ja, route_en: en, note: "", message: "", underrun_frames: 0, filter: "standard", auto_version: true, treble_restore: false };
      }
      case "bar_auto_pair": return [];
      case "bar_save_combo": return null;
      // ── ディスク ──
      case "list_optical_drives": return ["D:"];
      case "list_cd_tracks": return tracks;
      case "prepare_cd_track_for_playback": return "demo-cd.wav";
      case "list_disc_items": return [
        { label: "DVDタイトル 01(デモ)", paths: ["demo.mp4"], bytes: 4.2e9, kind: "dvd-title" },
        { label: "DVDタイトル 02(デモ)", paths: ["demo.mp4"], bytes: 3.1e8, kind: "dvd-title" },
      ];
      case "prepare_disc_item_for_playback": return args.paths && args.paths[0] ? args.paths[0] : "demo.mp4";
      case "upsample_for_playback": return args.input_path || "demo-cd.wav";
      case "ai_playback_rate": return { sample_rate: 352800, method: "rule" };
      // ── システム音声の橋渡し(Windows) ──
      case "bridge_devices": return [["スピーカー (デモDAC)", true], ["HDMIモニター (デモ)", false]];
      case "bridge_start":
        bridge = { running: true, capture_device: args.capture, output_device: args.output, in_rate_hz: 48000, out_rate_hz: 384000, out_bits: args.bits || 32, container_bits: 32, buffer_secs: 0.23, underrun_frames: 0, in_peak: 0.3, message: "" };
        return bridge;
      case "bridge_status": if (!bridge) return null; bridge.in_peak = 0.2 + 0.15 * Math.abs(Math.sin(now() * 2)); bridge.buffer_secs = 0.22 + 0.02 * Math.sin(now()); return bridge;
      case "bridge_stop": bridge = null; return null;
      // ── 関連付け・自動再生(Windows) ──
      case "assoc_list": return (args.exts || ASSOC_ORDER).map((ext, i) => ({ ext, progid: "demo", app: ext === "dsf" || ext === "dff" || ext === "iso" || ext === "ifo" || ext === "cda" ? assocApps[1] : assocApps[0], registered: registered.has(ext), is_default: false }));
      case "assoc_register": (args.exts || []).forEach((e) => registered.add(e)); return (args.exts || []).length;
      case "assoc_unregister": (args.exts || []).forEach((e) => registered.delete(e)); return null;
      case "open_default_apps_settings": alert("デモ: 実際のアプリではWindowsの「既定のアプリ」設定が開きます。\nDemo: the real app opens Windows' default-apps settings."); return null;
      case "autoplay_status": return autoplayOn;
      case "autoplay_enable": autoplayOn = true; return "demo";
      case "autoplay_disable": autoplayOn = false; return null;
      case "startup_args": return [];
      case "probe_media": return { duration_secs: 3, format_name: "mp4", bit_rate: 300000, width: 640, height: 360, fps: 24, audio_codec: "aac", audio_channels: 2, audio_profile: null, dolby_vision: false, audio_sample_rate: 44100 };
      // ── そのほか(変換・書き込みなど)は、デモでは動かさない ──
      case "list_plugins": case "open_easy_web_layout": case "ai_hw_status": return null;
      default:
        if (/^(convert|burn|create_iso|rip|ai_|copy_source|concat|rebind|convert_pdf|ai_upmix)/.test(cmd)) throw "デモでは動きません。実際のアプリをダウンロードしてお試しください。 / Not available in the demo — please download the real app.";
        return null;
    }
  };

  // デモ用の動画(生成したテストパターン、3秒)。映像の項目(DVD等)を選んだときだけ使う。
  const convertFileSrc = (p) => (/\.(mp4|mkv|vob)$/i.test(String(p)) ? "sample.mp4" : "");
  window.__TAURI__ = {
    core: { invoke, convertFileSrc },
    // 「ファイルを選ぶ」では、デモ用の動画(sample.mp4)が選ばれる。フォルダ選択・保存は何もしない。
    dialog: { open: async (opts) => (opts && opts.directory ? null : "demo.mp4"), save: async () => null },
    event: { listen: async () => () => {} },
  };
  // 画面のUA判定(Windows専用の欄を出す)。デモはどのOSのブラウザでも、Windows版と同じ画面を見せる。
  try { Object.defineProperty(navigator, "userAgent", { get: () => "Mozilla/5.0 (Windows NT 10.0; Win64; x64) open-media-demo" }); } catch (_) { /* 変更できなくても動く */ }
})();
