# Development policy & environment rules (make-disk) — summary

**Languages**: [日本語 (full, authoritative)](../CLAUDE.md) | English | [简体中文](CLAUDE.zh-CN.md) | [繁體中文(台灣)](CLAUDE.zh-TW.md) | [한국어](CLAUDE.ko.md) | [Deutsch](CLAUDE.de.md) | [Français](CLAUDE.fr.md) | [Русский](CLAUDE.ru.md) | [Українська](CLAUDE.uk.md) | [فارسی](CLAUDE.iran%28Perusha%29.md) | [العربية](CLAUDE.ar.md)

> This is a summary. The full text, including the complete development history (HANDOFF entries), is the Japanese [`CLAUDE.md`](../CLAUDE.md).
> Rules shared by all repositories (autonomous continuation, thorough verification, etc.) follow `CLAUDE.md` in [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z).

## Role of the repository

A GUI app (Rust + Tauri) for burning CD/DVD/Blu-ray, converting audio/video and writing ISO images, with one codebase for Windows/macOS/Linux (and Android).
Platform differences are confined to the installers (bundles); the app itself (`src-tauri/src`, `src`) is a single codebase.

## Architecture

- Frontend: `src/` (vanilla JS; calls Rust commands through Tauri IPC via `window.__TAURI__`, because bare imports do not work without a bundler).
- Backend: `src-tauri/src/engine/`
  - `probe.rs` — duration, codecs, sample rate, Dolby traits via ffprobe (rs-ffmpeg fallback)
  - `convert.rs` — ffmpeg conversion, bitrate control, trimming, multi-range cuts (stream copy by default, GPU encoder for frame-accurate cuts)
  - `capacity.rs` — max bitrate from disc capacity and a 4-level quality warning
  - `dsd.rs` — own PCM→1-bit delta-sigma modulator and DSF writer, DoP WAV
  - `cdda.rs` — audio-CD ripping (Windows, simple secure read)
  - `iso.rs` / `burn.rs` / `windows_imapi.rs` — ISO building and burning (IMAPI2 on Windows, xorriso elsewhere)
  - `ai_upscale.rs` / `cpu_sr.rs` / `audio_sr.rs` — Real-ESRGAN (GPU/CPU), audio bandwidth extension
  - `mkv_tracks.rs` — keeping/adding multiple audio and subtitle tracks in MKV
  - `plugins.rs` / `sidecar.rs` — bundled/downloaded tools with version management
  - `cpu.rs` — CPU feature detection via `open-cpu` (speed hints and x264 `-preset` choice)

## Fixed policies

- **No circumvention of copy protection** (CSS/AACS etc.). Unprotected discs only.
- **No new generation of Dolby Vision / Atmos / IMAX / 4DX** (licensed); preservation by stream copy and compatible lower formats only.
- **No MQA** (patents/trade secrets). Hi-res output follows the open-format route of `aon-co-jp/open-mqa`.
- **When creating DSD, no PCM is created alongside it** (players convert DSD to PCM automatically on hardware without DSD). DoP WAV is DSD data, so it is allowed.
- DSD modulation stays **sequential per channel (bit-exact)**; segment-parallel modulation was measured to destroy SNR and was rejected.
- "AI" features are disclosed honestly: silence-based auto-cut and "AI-optimized" resolution/FPS are heuristics; audio bandwidth extension is synthesis.
- Full HD on DVD is outside DVD-Video; set-top players do not downscale automatically. The UI keeps this warning (a DVD-Video + Full HD file combo was declined by the user).
- Documentation languages: Japanese is authoritative; `README/`, `CLAUDE/`, `PORTING/` hold English, Simplified Chinese, Traditional Chinese (Taiwan), Korean, German, French, Russian, Ukrainian, Persian (Iran, file suffix `.iran(Perusha)`) and Arabic (CLAUDE/PORTING as summaries).

## Verification rules

- CI success is not proof of working. Before reporting completion, verify for real: `npm run lint` (eslint `no-undef`), a browser check of the UI with stubbed Tauri APIs, Rust tests (`cargo test --lib -- --test-threads=1`) and, where possible, real-file / real-hardware E2E.
- Honestly state what has not been verified.

## Releases

Pushing a `v*` tag makes `.github/workflows/release.yml` build Windows/macOS/Linux/Android installers and publish them on GitHub Releases.

## Latest state (2026-09-24, v0.1.28)

- v0.1.26: fixed the 0.6-second WAV bug (cut intermediates were stream-copied into `.wav`, breaking AAC framing; they are now MKV), ISO/burn aborted after any failed conversion,
  fast extract of only the needed range (input seek), GPU hardware encoder / open-cpu speed preset for the extract's re-encode, AI range search via aruaru-llm (speech only, not the picture), source and output folders must differ.
  Heavy work runs at below-normal priority so playback (e.g. a Blu-ray) stays smooth.
- v0.1.27: upconvert and fill the disc (DVD 1–2 layer / Blu-ray 1–4 layer × Full HD / 4K). Capacity-derived bitrates for libx264/libx265 use **2-pass** (1-pass overshot by 6–7% and would not fit);
  audio is subtracted, the source-bitrate cap is skipped when filling, and 40 Mbps (Full HD) / 100 Mbps (4K) ceilings apply.
- v0.1.28: separate bilingual note that scaling is interpolation.
- **Verifying the installed app without desktop control**: start `make-disk.exe` with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`, read `http://127.0.0.1:9333/json/list`,
  and drive the page with CDP `Runtime.evaluate` (Node's built-in `WebSocket`); `window.__TAURI__.core.invoke` calls the app's own Rust commands. Local only; close the app afterwards.
- Not yet verified: a full-size upconvert filling a real disc (hours long).

- **2026-09-24 Continued 29 — AI upscaling rebuilt, CPU/GPU auto-selection, RIFE interpolation, fit prediction**:
  - **`engine/ai_video.rs` (new)**: streaming (raw RGB over a pipe), resumable (committed every 240 frames), no frame limit. Interlace/telecine detection (idet), black-bar detection (cropdetect), BT.601->709, DAR preserved, flat/still-frame reuse, ETA, cancel (`progress.rs`, event `make-disk-progress`).
  - **`engine/hw_bench.rs` (new)**: measures CPU / GPU / both on this PC (cached; re-measured if the CPU or build type changes) and picks a method only if it is at least 1.08x faster. **Measured here (GT 730): CPU 1.27 s/frame, GPU 2.76 s + 1.6 s launch, both ~1.3x faster.** `engine/sr_pool.rs` shares the work (the GPU takes several frames per launch to amortize the fixed cost).
    open-cuda has no convolution op, so the AI math runs on our own CPU kernel (open-cpu detects the ISA) and NCNN-Vulkan; open-cuda / open-directx / aruaru-llm do not speed it up (only GPU detection and search help) — disclosed honestly.
  - **`engine/rife.rs` (new)**: RIFE (20221029, rife-v4.6). Only ~12 MB of the ~411 MB zip is fetched by HTTP range requests. Segments are committed for resume. **On the GT 730 the default `-j 1:2:2` silently yields black frames**, so it runs with `-j 1:1:1`, output brightness is checked and broken segments are replaced by frame repetition. Fully static segments skip RIFE. Real test (10 fps -> 20 fps, 60 frames) passes.
  - **`engine/fit_predict.rs` (new)**: rates fit (verdict marks) from disc capacity and bits-per-pixel (scaling with fps^0.6), capped by the standards' bitrate limits (Full HD 40 / 4K 100 Mbps), with static share sampled via blackdetect/freezedetect. Exposed as `ai_fit_predict` behind the "Predict fit" button in section 6.
  - **Bugs found by real tests and fixed**: GPU-only mode leaving the last frame unprocessed (hang; collector ignoring disconnects), black GPU outputs (checked and replaced by bicubic), a debug-build benchmark being cached and mis-selected (build type added to the signature).
  - **Not done / limits**: realesr-general-x4v3 is not bundled; a feature-length run (days) is unverified; RIFE speed at 4K is unmeasured; fit prediction covers only the first file, not the sum of several; `realesrgan-x4plus` needs a GPU.
  - **Next**: open-easy-web install layout (`%LOCALAPPDATA%\open-easy-web\`), LLM manager (NPU postponed), local open-web-server, easy-web.tokyo integration and `make-disk://` launch.

- **2026-09-25 Continued 30 — open-easy-web install layout (v0.1.30)**:
  - Top folder `%LOCALAPPDATA%\open-easy-web\` holding `make-disk\` (app and plugins), `aruaru-llm\`, `open-web-server\`, `open-cpu\`, `open-directx\`, `open-cuda\` (user's choices: move only make-disk; also create standalone library folders). On macOS/Linux it is `open-easy-web` under the data directory.
  - **Empty folders are not dressed up as real components**: each has a `component.json` stating its status (`planned` = reserved, not implemented: aruaru-llm, open-web-server; `embedded` = built into make-disk/aruaru-llm, no standalone executable: open-cpu, open-directx, open-cuda).
  - `engine/layout.rs` (new): sets up the layout at startup and **moves** the legacy `%LOCALAPPDATA%\make-disk\plugins` (so downloaded AI/RIFE plugins and the benchmark are not fetched again). `plugins::plugin_dir()` points to the new layout (`MAKE_DISK_PLUGIN_DIR` still overrides).
  - NSIS default install dir is now `open-easy-web\make-disk`: the official template is copied to `installer/installer.nsi` with two changes (`bundle.windows.nsis.template` in `tauri.conf.json`), and **updates from the legacy location are moved too**. When bumping the Tauri CLI, re-fetch the template from the same version and reapply the diff.
  - Verified on this PC: built the NSIS installer locally and installed over the legacy layout -> the app landed in the new folder, first launch moved the plugins (realesrgan, rife, audio-sr, benchmark), created the five folders with `component.json`, and the old plugins folder was gone. The old app files from the earlier manual install remain and can be deleted by the user.
  - **Next**: LLM manager (recommended / one size up / one size down; NPU later) -> local open-web-server (aruaru-llm gateway) -> easy-web.tokyo integration and `make-disk://` launch.

- **2026-09-25 Continued 31 — verifying and hardening the previously unverified parts (v0.1.31)**:
  - **RIFE at 4K (GT 730), measured**: Full HD ~4.2 s per interpolated frame on the GPU. **At 4K the GPU fails with `vkQueueSubmit/vkWaitForFences failed -4` (device lost) and some frames come out black** (4 of 23). The CPU is correct at ~31-46 s per frame. The old check looked at only 2 frames and could miss this, so **every output frame is now compared with its neighbouring source frames**; a broken segment is redone on the CPU and later segments start on the CPU (`rife.rs`). `rife::speed()` measures and caches the speed per size (`rife-bench.json`); "Estimate" now shows the interpolation time and a warning when the GPU cannot be used.
  - **4K / 120 fps end to end**: DVD-like (720x480, 24 fps) -> 3840x2160, 120 fps, with audio succeeded (4 frames, 258 s, automatic CPU fallback). Tests `real_dvd_to_4k_120fps_end_to_end` and `real_dvd_to_full_hd_60fps_with_ai_and_rife` in `convert.rs` (`--ignored`).
  - **Long-run check**: `real_long_run_cancel_and_resume` in `ai_video.rs` (`--ignored`): a 2-minute (3596-frame) DVD-like clip -> Full HD; cancelled after ~15 min and resumed, 3596 output frames (matches), the work folder is removed on completion, 17 MB of progress kept at cancel, 6408 s total (~1.8 s/frame). A feature film (~170,000 frames) is estimated at ~3.5 days, but **a full-length run has not been done**.
  - **Live-action model realesr-general-x4v3 bundled** (`src-tauri/models/`, BSD-3-Clause, Real-ESRGAN): the official ncnn release lacks it, so the two official `.pth` files (general / wdn) are blended at denoise 0.5 (the official default) and converted to ncnn (`scripts/convert_general_model.py`, no torch needed). Its CPU output matches the official GPU implementation (NCNN-Vulkan) at **46.5 dB PSNR** (`real_general_model_cpu_matches_the_official_gpu` in `hw_bench.rs`). "Auto" picks the general model for live action and the fast model for anime. There is no UI for the denoise strength (fixed at 0.5).
  - **AI off + a target fps**: runs interpolation-only RIFE (`AiUpscale.interpolate_only`, `ai_video::make_interpolated_mezzanine`); the earlier frame duplication is gone. **aruaru-llm is a language model and cannot create video frames, so RIFE makes the motion smoother** and aruaru-llm is not involved — stated plainly in the UI and README.
  - **Release housekeeping**: for v0.1.30 all four CI build jobs succeeded but only the release creation failed with `Resource not accessible by integration`; the release was created from the command line and the failed jobs re-run. The APK was the only asset with an old name and was unified to `make-disk_<version>_android-universal.apk` (workflow fixed too, automatic from v0.1.31).
  - **Not done / limits**: an actual full-length film run; feature-length interpolation at 4K (impractical on this PC); a quantitative quality evaluation of interpolation (ghosting in fast motion); adjustable denoise strength.
