# open-media

[日本語](README.md) | **English**

**open-media = [make-disk](https://github.com/aon-co-jp/make-disk) + [open-bar](https://github.com/aon-co-jp/open-bar)**

One app that combines disc burning, audio/video conversion and ISO creation (make-disk) with a high-quality audio/video player (open-bar). It targets Windows, macOS, Linux and Android (phones and tablets) from a single codebase (Rust + Tauri).

> make-disk stays at its final release. Development continues here in open-media.

## Download

Latest release: <https://github.com/aon-co-jp/open-media/releases/latest>

| Platform | File |
|---|---|
| Windows | `open-media_0.1.6_x64-setup.exe` / `.msi` |
| macOS (Apple Silicon / Intel) | `open-media_0.1.6_aarch64.dmg` / `open-media_0.1.6_x64.dmg` |
| Linux | `.deb` / `.rpm` / `.AppImage` |
| Android (phones and tablets) | `open-media_0.1.6_android-universal.apk` (unsigned, sideload) |

## Features

### From make-disk (convert and burn)
- Burn CD / DVD / Blu-ray, write ISO images, rip audio CDs
- Convert audio and video, automatic bitrate fitting, range cutting
- AI noise reduction, AI bandwidth extension, AI super-resolution, frame interpolation, surround conversion, PDF spread conversion

### From open-bar (playback) — "0. Disc player"
- **Hybrid playback**: audio (CD, audio files, DSD) plays through the open-bar engine; video (DVD / Blu-ray / movies) plays in the on-screen video player.
- **Playback modes**: E exclusive mode with high-quality upsampling (default; CD to 352.8 kHz, 192 kHz to 384 kHz; falls back to shared mode if the device refuses) / B shared mode with upsampling / A exclusive mode, bit-perfect / D DoP (DSD as-is to the DAC; DoP-capable DACs only).
- **32-bit output (default)**; devices that refuse it fall back to 24-bit (internal processing is f32, so real resolution stops at 24-bit).
- **Video upconversion**: DVD / MP4 / MKV are enlarged to Full HD / 4K while playing (lanczos interpolation, not AI super-resolution). Audio is extracted and played with upsampling.
- **Treble restoration** (for MP3), pause, seek, volume, live mode switching.
- **Disc items are chosen with radio buttons.** Several tracks or titles can never play at the same time.
- **Video + separate audio combos** (for example an MP4 movie with DSD audio). The audio is the master clock and the video follows it; combos can be saved as `.obar.json`.
- **MQA**: detection only. It is never decoded (use mode A to pass it through to an MQA DAC).

## Status and known limitations (honest disclosure)

- **Audio-CD playback is verified on real hardware** (Windows: track read → exclusive mode + upsampling 44.1→352.8 kHz + 32-bit, zero underruns). **Real DVD / Blu-ray discs are not verified yet** (the conversion step was checked with synthetic data).
- Copy-protected commercial discs cannot be read (no protection is circumvented).
- DVD / Blu-ray video cannot be played directly in the browser engine, so it is converted to MP4 first. Long titles take a while.
- Exclusive mode and DoP are Windows-only. On macOS / Linux / Android playback uses shared mode or the video player. Audio-CD ripping and playback are Windows-only.
- Verification: Windows install / launch / uninstall, CD playback and the system-audio bridge (DAC: a Loxjie D40 Pro-class device at 384 kHz / 32-bit) checked on real hardware. Linux (Ubuntu on WSL) checked up to install and launch. Android checked on a real device (Android 14) up to MP4 / FLAC playback. macOS is unverified (built by GitHub Actions but never run on hardware).
- Auto-update: the desktop app checks for updates at startup and updates after you confirm (signed). Versions older than the one that introduced it use a different signing key, so replace those manually. An end-to-end update test on real hardware is pending until this release is out.
- License: MIT (dependencies keep their own licenses).

## Build

```
npm install
npm run tauri dev      # run in development
npm run tauri build    # build installers
```

ffmpeg / ffprobe / xorriso are bundled by CI or taken from PATH. See [CLAUDE.md](CLAUDE.md) and [PORTING.md](PORTING.md) (including the make-disk-era development log).
