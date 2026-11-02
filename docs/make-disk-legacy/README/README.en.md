# make-disk

**Languages**: [日本語](../README.md) | English | [简体中文](README.zh-CN.md) | [繁體中文(台灣)](README.zh-TW.md) | [한국어](README.ko.md) | [Deutsch](README.de.md) | [Français](README.fr.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | [فارسی](README.iran%28Perusha%29.md) | [العربية](README.ar.md)

A cross-platform (Rust + Tauri) GUI for burning CD/DVD/Blu-ray and converting audio/video.
One codebase; only the installers differ per OS.

## Latest version: v0.1.32

Get it from the [latest release](https://github.com/aon-co-jp/make-disk/releases/latest). Every file name carries the same version.

| Platform | File |
|---|---|
| Windows | `make-disk_0.1.32_x64-setup.exe` (recommended), `make-disk_0.1.32_x64_en-US.msi` |
| macOS | `make-disk_0.1.32_aarch64.dmg` (Apple Silicon), `make-disk_0.1.32_x64.dmg` (Intel) |
| Linux | `make-disk_0.1.32_amd64.deb`, `make-disk_0.1.32_amd64.AppImage`, `make-disk-0.1.32-1.x86_64.rpm` |
| Android | `make-disk_0.1.32_android-universal.apk` (unsigned, sideload) |

On Windows the default install folder is `%LOCALAPPDATA%\open-easy-web\make-disk` (updates from the old layout are moved automatically).

## Features

- **I/O**: pick multiple sources (audio/video/PDF) and an output folder; convert, build ISO images and burn
  discs at once (multiple formats, files and disc types run asynchronously in parallel).
- **Audio**: MP3 / WAV / FLAC / AAC / OGG / **Opus** / **AC-3 / E-AC-3 (5.1–7.1)** /
  **DSD64·128·256·512·1024 (DSF)**. ffmpeg cannot write DSD, so PCM→1-bit delta-sigma modulation
  (5th order, per-channel threads) and DSF writing are implemented in Rust and verified by round-tripping through ffmpeg's DSF decoder.
- **Video**: MP4 / MKV / AVI / MOV / WebM / **AV1** (libsvtav1 preferred, libaom fallback) /
  **HEVC 10-bit (HDR10)** / **lossless stream copy** (keeps Dolby Vision, Atmos, 5.1/7.1 surround).
  Resolution (720x480 up to 8K, PAL, custom, AI-optimized) and FPS (24/30/60/120, custom) are selectable;
  choices narrow by disc direction (BD→DVD: standard DVD resolution or Full HD; DVD→BD: Full HD or 4K).
- **High-resolution PCM (for R-2R / multi-bit DACs)**: 352.8 kHz / 384 kHz (24- and 32-bit) and 705.6 kHz / 768 kHz (32-bit) WAV, using the best
  available resampler (soxr, else a high-precision swresample setup) with TPDF dither. Measured SNR against an exact reference sine:
  352.8 kHz/24-bit = 141.2 dB, 384 kHz/32-bit = 150.2 dB. When DSD is created, no PCM is written alongside it
  (players convert DSD to PCM automatically on hardware without DSD, so PCM would only waste space). Measured DSD round-trip SNR: DSD64 = 99.6 dB, DSD128 = 132.4 dB.
- **AI super-resolution (video) and frame interpolation**: Real-ESRGAN (MIT) as an on-demand plugin (upconvert DVD 1-2 layer -> Blu-ray 1-4 layer, Full HD / 4K).
  **It benchmarks this PC's CPU and GPU and automatically picks the fastest of CPU-only / GPU-only / both** (our own Rust CPU build uses AVX2+FMA; output PSNR 42-57 dB against the official GPU build).
  Interlace/telecine auto-detection, black-bar crop and restore, skipping the AI on black/still frames, cancel and resume, and remaining-time display.
  **4K / 120 FPS** uses RIFE frame interpolation (rife-ncnn-vulkan, MIT; only ~12 MB fetched by range requests), and "Predict fit" tells you beforehand whether it fits the disc.
- **AI bandwidth extension (experimental)**: fills in the missing highs of band-limited audio with a trained model (LavaSR, Apache-2.0, run by pure-Rust tract, ~56 MB fetched on first use).
  **The existing band is never modified**, and the generated highs are capped by an extrapolation of the input envelope (the raw model output worsened music, hence this design).
  Measured (music band-limited at 8/12 kHz): LSD in the band with ground truth 3.4→1.5 and 2.7→1.6, low band unchanged. It is synthesis, not restoration, and does nothing for sources that are not band-limited.
- **AI noise reduction**: bundles a real trained neural network (RNNoise), verified with the real model. It is trained mostly on speech, so the effect on music is modest.
- **Bitrate / capacity**: fixed, auto from disc capacity, and a "maximum quality" mode (lossless WAV + ISO when no format is chosen).
  Auto bitrate is capped at the source bitrate, and audio-only outputs correctly use `-b:a`.
- **Cutting the source**: answer YES/NO to "Cut by size?" and "Cut by time?" (mutually exclusive, exactly one is YES; size is the default).
  Post-processing checkboxes: "Fill the disc" (CD / DVD 1–2 layer / Blu-ray 1–4 layer) and "AI auto-cut" (silence detection).
- **Playback-standard limits**: shows the maximum kHz, bit depth and bitrate of CD / DVD-Video / DVD-Audio / Blu-ray / Ultra HD Blu-ray / PC only,
  and converts to fit the chosen one (sources already below the limit are left as is; no upsampling).
- **Upconvert and fill the disc**: pick one of DVD 1–2 layer / Blu-ray 1–4 layer and Full HD or 4K; the bitrate that fills the disc is computed from the total length (audio subtracted) and hit precisely with 2-pass encoding (measured ~96–99% of the target, never over). It stops at the ceiling beyond which quality no longer improves (Full HD 40 Mbps, 4K 100 Mbps).
- **Fast extract of only the part you need**: even from a 5-hour source, set the start and end of the part you want (e.g. 60 or 10 minutes) and only that part is read and processed (measured: 10 minutes from the 2-hour mark of a 3h33m MP4 to WAV in ~1 s, 60 minutes in ~5 s).
- **Find with AI (aruaru-llm)**: describe what you want and it searches the subtitles (or a transcription) for a matching range and fills in the extract start/end, offering 3 options. It uses what is said, not what is shown. Re-encoding after an extract uses the GPU hardware encoder (or a speed-first setting chosen from open-cpu).
- **Editing**: multi-range cutting (choose the file in section 8 and set ranges while previewing the video/audio), concatenating files,
  equal-interval / size-based splitting (the remainder is auto-fitted to fill the disc).
- **PDF**: spread images (right/left binding, up to 4K) and bulk binding-direction conversion (page order reversal).
- **Burning (Windows)**: auto-detects optical drives, builds ISOs with IMAPI2 (keeps Japanese filenames) and burns.
  Verified on real hardware (BD-RE drive + CD-R): a 3.5-hour video → disc-filling audio → ISO → burn succeeded.
- **Other**: automatic update check on launch (bilingual dialog); versioned rs-ffmpeg / rs-xorriso plugins (an identical version is not overwritten).

## Not supported / limitations (honest disclosure)

- **Circumventing copy protection (CSS/AACS, etc.) is not implemented** (it can be illegal). Unprotected discs only.
- **Newly creating Dolby Vision / Atmos / Dolby Cinema / IMAX / 4DX is not possible** (licensed formats). Only preservation (stream copy) and compatible lower formats.
- **AI video super-resolution is very slow** (per 720x480 frame, measured on this PC: ~1.3 s on a 32-thread AVX2 CPU, ~2.8 s on a GT 730 GPU, both together ~1.3x faster).
  End-to-end check: a 2-minute (3596-frame) DVD-like clip -> Full HD ran to completion with a cancel and resume in the middle (~1.8 s/frame, frame count matched). **A feature film (~170,000 frames) would take about 3.5 days**; a full-length run has not been done ("Estimate" shows the expected time).
  Skipping still frames mainly saves compute time and has little effect on disc capacity. open-cuda has no GPU convolution, so the AI math runs on our own CPU kernel and Vulkan (NCNN). aruaru-llm is a language model and cannot create video frames: RIFE makes the motion smoother, and aruaru-llm only helps with GPU detection and range suggestions.
  **RIFE interpolation is an approximation**: fast motion and scene cuts can show ghosting or warping, and it is not real 120 fps footage. **Measured on this PC (per interpolated frame): Full HD ~4.2 s on the GPU; 4K ~31 s on the CPU because the GPU returns broken output.**
  A 4K / 120 fps end-to-end conversion (DVD-like -> 3840x2160, 120 fps, with audio) succeeded on a tiny 4-frame clip but is impractical for a feature film (4 interpolated frames per source frame at 24 -> 120 fps). The GT 730 loses the device at 4K and returns black frames, so every output frame is checked and a broken segment is redone on the CPU.
  With AI upscaling off, raising the fps runs interpolation-only (RIFE, no upscaling). The live-action model (realesr-general-x4v3, about 2x slower) is bundled; its converted weights match the official GPU implementation at 46.5 dB PSNR.
  **Audio AI bandwidth extension is experimental** (synthesis, no guarantee of perceptual quality). The "AI-optimized" resolution/FPS options are simple heuristics, not AI super-resolution.
- DSD files are huge (per stereo minute: DSD64 ≈ 42 MB … DSD1024 ≈ 678 MB) and are DSF files, not a Super Audio CD disc.
- Full HD on a DVD is outside the DVD-Video standard (max 720x480/576); set-top DVD players do not downscale it automatically, so it may not play.
- Upconversion restores detail with AI when "Use AI super-resolution" is on, and is plain interpolation when off. Video bitrate stops at 40 Mbps (Full HD) / 100 Mbps (4K), beyond which quality does not improve, and some disc space is then left free.
- "Find with AI" only uses what is said (subtitles/speech), not what is shown. Music-only videos (auto-captions like `[Music]`) cannot be searched by content.
- Burning on Linux/macOS relies on real xorriso (not bundled). Discs are written as data discs (audio CD / CD-DA burning is unsupported).
- iOS is unsupported (no test device).

## External dependencies

The Windows/Linux installers bundle ffmpeg/ffprobe and rs-ffmpeg/rs-xorriso (see `src-tauri/src/engine/sidecar.rs` and `plugins.rs`);
if absent, the tools on `PATH` are used. macOS ffmpeg and real xorriso are not bundled.

## Development

```bash
npm install
npm run tauri dev
cd src-tauri && cargo test --lib -- --test-threads=1
```

## Downloads / installers

Windows (.msi/.exe), macOS (.dmg, Intel/Apple Silicon), Linux (.deb/.rpm/.AppImage) and Android (universal APK) are on
[GitHub Releases](https://github.com/aon-co-jp/make-disk/releases/latest). Pushing a `v*` tag builds all platforms via
`.github/workflows/release.yml`. Landing page: <https://easy-web.tokyo/make-disk/>

## License

MIT (the bundled RNNoise model is stated by its author to be outside copyright; see `CLAUDE.md`).
