# open-media: moving to another PC, and per-OS status

<!-- status-2026-11-19 -->
## Where things stand (2026-11-19, in plain words)

- **v0.1.11 is out.** It has installers for Windows, Mac and Linux.
- **AI surround got faster.** The order it tries is "GPU, then onnxruntime (CPU), then tract". onnxruntime is the stepping stone when the GPU fails. It is about 5.6 times faster than tract and gives almost the same answers. The Windows installer includes onnxruntime, and we opened it to check.
- **Not fixed yet**: (1) the Windows ARM64 build (the step that fetches ffmpeg is missing for it); (2) the Android APK (we are still looking for the cause); (3) why this model will not run on the GPU (we still do not know).

<!-- plain-language-2026-11-19 -->
## In plain words (how to move and resume)

**The shortest steps for continuing on another computer or account.**

1. Get the repository (`git clone https://github.com/aon-co-jp/open-media`).
2. Install Rust (`cargo`) and Node.js. On Windows you also need WebView2 (usually already there).
3. To try AI surround, prepare the model yourself (it is big, so nothing is downloaded behind your back). Prepare it with `tract-onnx-patch` and put it in `MAKE_DISK_DEMUCS_MODEL_DIR`.
4. Continue from the "Resume message" further down this file.

**What is still open (simply)**: (1) find out again why the model does not run on the GPU (DirectML); (2) check that onnxruntime is inside the installer (after the v0.1.11 release); (3) real DVD/Blu-ray discs; (4) real macOS and Android TV devices.

**Languages**: [日本語](PORTING.md) (authoritative) | English (this page)
See also: [`CLAUDE.en.md`](CLAUDE.en.md) (development rules) / [`README.en.md`](README.en.md)

How to move the whole development setup to a new machine, plus the support status per OS.
**The repository (code, docs, scripts) lives on GitHub and on the VPS. Only what cannot be in the repository (keys, credentials, sidecar binaries) is carried separately.**

## 1. Overview (checklist)

- [ ] ① Install the tools on the new PC (chapter 2); run `powershell -File scripts\doctor.ps1` to see what is missing
- [ ] ② Get the repository (chapter 3)
- [ ] ③ **Carry keys and credentials safely** (chapter 4; never paste them into the repository, chat or e-mail)
- [ ] ④ Verify (chapter 5)
- [ ] ⑤ Clean up the old PC if needed (chapter 6)

## 2. Tools for the new PC (Windows)

| Tool | Purpose | Example |
|---|---|---|
| Git / GitHub CLI (`gh`) | clone, push, release checks | `winget install Git.Git GitHub.cli`, `gh auth login` (org `aon-co-jp`) |
| Rust (stable, MSVC) | build | `rustup` (needs Visual Studio Build Tools "Desktop development with C++") |
| Node.js 20+ | Tauri CLI, UI build | `winget install OpenJS.NodeJS.LTS` |
| WebView2 | runtime (built into Windows 11) | usually nothing to do |
| ffmpeg / ffprobe / poppler / rs-FFmpeg / rs-xorriso | bundled parts (CI prepares them) | **only needed for local executable builds**: `scripts/fetch-ffmpeg-sidecars.sh`, `fetch-poppler-sidecars.sh`, `build-rs-tribute-sidecars.sh` (clone `rs-FFmpeg` and `rs-xorriso` next to this repo), or copy the old PC's `src-tauri/binaries/` |
| Android (optional) | local APK build, device tests | Android SDK + **NDK 27.1.12297006** + Java 17 + `rustup target add aarch64-linux-android` + `adb` |
| Ubuntu on WSL (optional, **test only**) | check the Linux build | `wsl --install -d Ubuntu`, `sudo apt install libwebkit2gtk-4.1-dev libasound2-dev libappindicator3-dev librsvg2-dev patchelf`, then install the `.deb` |
| Optical drive that reads audio CDs, a USB DAC (optional) | hardware playback tests | the CD tests assume `D:` (override with the `OPEN_MEDIA_CD` environment variable) |

To move to a Mac or Linux development machine: install the Rust / Node / Tauri prerequisites (Linux: `libwebkit2gtk-4.1-dev` etc.; macOS: Xcode Command Line Tools) and the same codebase builds (**macOS is unverified**). Exclusive mode, the system-audio bridge, AutoPlay, file associations and audio-CD ripping are Windows-only.

## 3. Getting the repository

```
git clone https://github.com/aon-co-jp/open-media.git
cd open-media
npm install
cd src-tauri && cargo check
```

- **Dependencies such as open-bar are fetched by Cargo from GitHub** (versions are pinned in `Cargo.toml`); no separate clone is needed.
- The meta-index repository `RUNO` (`F:\runo`) holds the entry points for all projects and the hand-off notes (HANDOFF in its `CLAUDE.md`). Clone it too if you need it.
- **The VPS (conoha) has the same repository** at `/root/repository/open-media`. Once the new PC can `ssh conoha`, `git pull` keeps it current. Moving via the VPS also works: after pushing from the old PC, the new PC only needs to clone from GitHub.

## 4. What to carry (not in the repository — **carry it safely**)

| Item | Where on the old PC (example) | On the new PC |
|---|---|---|
| **Updater signing key** (private) | `F:\open-media-updater.key` (the `.pub` is the public key, also in `tauri.conf.json`) | Carry on **encrypted removable media**. **If lost, updates can no longer be shipped to existing users** (a new key forces a manual reinstall of older versions). It is already registered as the GitHub secret `TAURI_SIGNING_PRIVATE_KEY`, so CI releases work even without the key on the new PC |
| GitHub credentials | `gh auth` | `gh auth login` again on the new PC (do not carry tokens) |
| SSH key for the VPS | `~/.ssh/`, `~/.ssh/config` (`Host conoha`) | Carry key and config safely; alternatively add the new PC's public key to the VPS `authorized_keys` |
| Claude Code memory | `%USERPROFILE%\.claude\projects\F--runo\memory\` | Copy to the same place on the new PC (rename the folder if the project path differs) |
| Test material | `F:\open-media-test\` (build logs, screenshots, installers, test videos) | **Not needed**; can be recreated |
| Bundled executables | `src-tauri/binaries/` | See chapter 2. Not in the repository (too large) |

**Never**: paste key or credential values into the repository, chat, e-mail bodies or logs.

## 5. Verifying on the new PC

```
powershell -File scripts\doctor.ps1                  # tool check
cd src-tauri; cargo check                            # does it build (2-4 min)
cargo test --lib assoc                               # unit tests
cargo test --lib real_cd -- --ignored --nocapture    # hardware: put an audio CD in D: (needs an exclusive-mode DAC)
npm run tauri build -- --bundles nsis                # build the installer (~20 min; a "missing private key" message at the signing step is fine)
```

- Install it and check launch, playback and uninstall.
- The AutoPlay / file-association buttons **change that PC's settings** when pressed. Press "Undo" after trying them.
- For release tests follow "Release procedure" in `CLAUDE.en.md`.

## 6. Cleaning up the old PC (optional)

- If AutoPlay was set to open-media on that PC, press "Undo" in the app (the previous value is restored).
- Securely delete or archive the copy of the updater signing key.
- `F:\open-media-test\` and `%TEMP%\make-disk-player\` (temporary playback files) can be deleted.

## 7. Per-OS status (2026-10-14)

| OS | Shipped as | Verified | Unverified / limits |
|---|---|---|---|
| Windows | `.exe` (NSIS) / `.msi` | Real hardware: install, launch, uninstall, auto-update, CD playback (352.8 kHz / 32-bit), bridge (384 kHz / 32-bit), single instance | Real DVD / Blu-ray discs; AutoPlay actually firing on disc insertion |
| macOS | `.dmg` (Apple Silicon / Intel) | CI build only | **Hardware unverified.** No exclusive mode, DoP or CD ripping |
| Linux (Ubuntu) | `.deb` / `.rpm` / `.AppImage` | Install, launch and Japanese rendering on WSL | Playback on a sound device unverified; PipeWire config untested; no exclusive mode etc. |
| Android (phones, tablets) | APK (unsigned, sideload) | Real device (Android 14): MP4 / FLAC playback, tidied UI, D-pad operation | No open-bar engine (plays in the WebView); no video conversion |
| Android TV | same APK | Launcher entry (leanback) and D-pad-friendly focus set up | **TV hardware unverified** |
| iPhone / iOS | none | — | Not supported (no test device) |

## 8. Troubleshooting

- Build stops with `resource path ... binaries/... doesn't exist` → the bundled parts are missing. Place them in `src-tauri/binaries/` (empty placeholder files are enough for `cargo check`).
- `tauri android build` fails with "cannot create symbolic link" (Windows Developer Mode off) → use `scripts/android-local-build.ps1`.
- Updates do not work → "Pitfalls found" in `CLAUDE.en.md` (permissions; `--no-bundle` picks the `.msi`).
- No sound in exclusive mode / `0x8889000A` → another app (including open-media itself) is using the DAC. Stop it.
- Japanese shows as boxes on Ubuntu → fixed by the bundled font (`v0.1.9` and later).
- Reading CI results → `gh run view <id> -R aon-co-jp/open-media --json status,conclusion,jobs` (from bash).

## 9. Moving the public site (easy-web.tokyo)

- **Introduction and downloads** `https://easy-web.tokyo/open-media/` = the repository's `webpage/index.html` (shows the latest release from the GitHub API). **Demo** `/open-media/demo/` = `webpage/demo/` (generated from `src/` with `python scripts/build-demo.py`).
- **Location on the VPS**: `/root/easy-web.tokyo/open-media` (a clone of the repository). Updating is just `git pull --ff-only` (no restart).
- **Serving**: `/etc/systemd/system/open-media-web.service` (`WorkingDirectory=/root/easy-web.tokyo/open-media/webpage`, `ExecStart=/usr/bin/python3 -m http.server 8117 --bind 127.0.0.1`, `Restart=always`) plus an `open-web-server` `domains.toml` entry with `host="easy-web.tokyo"`, `backend="open_runo"`, `backend_addr="127.0.0.1:8117"`, `path_prefix="/open-media"`, `strip_prefix=true`. The front end is `open-web-server`, not nginx.
- **Top-page intro card**: `media_banner_html()` in `open-easy-web-app/server/src/main.rs`. Building needs `vps-tidy`, `RS-JSON`, `RPoem` and `open-raid-z` as siblings under `/root/repository/`.
- When moving to a new VPS, recreate these four things (clone, unit, domains.toml, top-page card) and check that `curl -I https://easy-web.tokyo/open-media/` and `/open-media/demo/` return 200.

## 10. 2026-11-17 Resume message (open-media)

To resume on another account, paste this as is.

> Repositories to resume: `F:\open-media` (aon-co-jp/open-media) and `F:\runo` (meta index). VPS: `ssh conoha`, `/root/easy-web.tokyo/open-media`, `/root/repository/open-media`, `/root/repository/open-easy-web-app`. First `git pull --ff-only`, then read open-media's `CLAUDE.en.md` ("Public site (easy-web.tokyo) hand-off") and `PORTING.en.md` chapter 9. Verified (2026-11-07 to 17): `/open-media/` and `/open-media/demo/` return 200, the top-page intro card (`media_banner_html()`, open-easy-web-app `6b01170`) is deployed, docs `1eeec5c`. Open / unverified: (1) whether open-media (not PowerDVD) opens on CD re-insert (the owner checks on the real PC), (2) real DVD / Blu-ray discs, (3) macOS and Android TV hardware, (4) intro pages for other projects not yet on easy-web.tokyo (survey first), (5) only if an admin page is added, put it behind open-easy-web's existing e-mail OTP (not needed for static public pages). Notes: after editing the UI (`src/`) re-run `python scripts/build-demo.py` and commit; never print secret values (`F:\open-media-updater.key` etc.); Windows is primary, Ubuntu (WSL) is test-only and must not run at the same time.
