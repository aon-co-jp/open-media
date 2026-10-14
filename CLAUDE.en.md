# Development policy & environment rules (open-media)

**Languages**: [日本語](CLAUDE.md) (authoritative) | English (this page)
See also: [`PORTING.en.md`](PORTING.en.md) (moving to another PC, per-OS status) / [`README.en.md`](README.en.md)
Detailed make-disk-era records (engine design, AI features, past bugs) are kept in [`docs/make-disk-legacy/`](docs/make-disk-legacy/).

## What this repository is

**open-media = [make-disk](https://github.com/aon-co-jp/make-disk) + [open-bar](https://github.com/aon-co-jp/open-bar)**: one GUI app
(Rust + Tauri 2, one codebase for Windows / macOS / Linux / Android) that combines disc burning/conversion (make-disk) with a hi-fi
player (open-bar). make-disk is frozen at its final release; development continues here. **The Windows build is primary; the Ubuntu (WSL) build is for testing.**

## Layout

- `src/` — UI (plain JS): `index.html`, `main.js` (features), `ux.js` (tabs, folded hints, remembered settings, shortcuts, drag & drop), `styles.css`, `fonts/` (bundled Japanese font)
- `src-tauri/src/lib.rs` — Tauri command entry points. **There are two `generate_handler!` lists** (Android and desktop). Add every new command to **both**.
- `src-tauri/src/engine/` — implementation
  - `bar_player.rs` (open-bar playback engine) / `bridge.rs` (system-audio bridge, Windows) / `disc_player.rs` (disc items, conversion for playback)
  - `autoplay.rs` (Windows AutoPlay registration) / `assoc.rs` (file associations) / `cdda.rs` (audio CD) / `burn.rs`, `iso.rs` (burning)
  - `convert.rs` etc. (conversion and AI features; see the legacy docs)
- `src-tauri/capabilities/` — `default.json` (common) / `desktop.json` (**updater permissions, desktop only**)
- `src-tauri/gen/android/` — Android project (the package name stays `jp.co.aon.make_disk` for compatibility)
- `installer/installer.nsi` — Windows installer template (default install dir `%LOCALAPPDATA%\open-easy-web\open-media`)
- `linux/pipewire/` — config that upsamples all apps' audio on Linux (**untested**)
- `scripts/` — `sync-doc-versions.sh` (version consistency check), `android-local-build.ps1`, `doctor.ps1` (environment check), etc.
- `.github/workflows/release.yml` — a pushed `v*` tag builds all OSes + APK and attaches them to the release

## Working rules

1. **Be honest.** Never write "done" for something unverified. Keep the README's "Status and known limitations" in line with reality after every check.
2. **Verify on real hardware before claiming completion** (a green CI is not a test). Take screenshots and look at them (see "How to test").
3. **Never process DRM-protected audio/video** (no protection-stripping code). The system-audio bridge only re-renders what it can capture, at higher quality, to another DAC; what cannot be captured (silent) stays as is.
4. **Never change user settings silently.** AutoPlay defaults and file associations change only when the user presses the button; record the previous value so "Undo" restores it. Ask before anything that needs admin rights (e.g. removing an MSI).
5. **Windows is primary, Ubuntu (WSL) is for tests.** Run Ubuntu tests with the Windows app closed and stop the Ubuntu app right after (`pkill -x open-media`). The two must not run at the same time. The Windows app is single-instance.
6. Secrets (updater signing key, tokens, SSH keys) **never go into the repository**; never print their values to the screen, commits or logs. Locations: see "Secrets".
7. Keep versions equal in `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and sync the README version text with `bash scripts/sync-doc-versions.sh` (any other `x.y.z`-looking number in the README fails the check).
8. Write Japanese and English side by side (UI, README, CLAUDE, PORTING). The Japanese text is authoritative.
9. End commit messages with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.

## Release procedure

1. Bump the version (rule 7) → commit → `git push origin main`
2. `git tag vX.Y.Z && git push origin vX.Y.Z` → GitHub Actions builds Windows / macOS (2) / Linux / Android and attaches `latest.json` and signatures (`.sig`).
3. **Check completion yourself** with `gh run view <id> -R aon-co-jp/open-media --json status,conclusion,jobs` (notifications and `gh run watch` were unreliable).
4. Install on real hardware, launch and play.
- Auto-update: at startup the app reads `latest.json` and, if a newer version exists, updates after confirmation. The private key is the repository secret `TAURI_SIGNING_PRIVATE_KEY` (no password); the public key is in `tauri.conf.json`.

## How to test

- **Build**: `cd src-tauri && cargo check` (2–4 min). Executable: `npm run tauri build -- --no-bundle` (~17 min). Installer: `--bundles nsis` (it ends with a "missing private key" error at the signing step, but the installer itself is produced).
- **Unit tests**: `cargo test --lib <name>`. Hardware tests are `#[ignore]`; run them with `-- --ignored --nocapture`.
  - `real_cd` (audio CD → exclusive + upsampling; needs an audio CD in D:) / `app_sequence` (same steps as the app) / `loopback_real` (in open-bar; the bridge) / `assoc_real` (register → unregister) / `autoplay_real` (enable → restore)
- **UI checks**: in a browser, serve a test page with a stub for `window.__TAURI__` from a static server. For the real app, take screenshots with PowerShell. **On 4K / scaled displays call `SetProcessDPIAware()` before capturing** (otherwise only the top-left part is captured).
- **Update tests**: **a `--no-bundle` executable picks the `.msi` for updates** (installer builds pick `nsis`). Test updates with an `--bundles nsis` installer. To keep an update check from interfering, start the app with `HTTPS_PROXY=http://127.0.0.1:9`.
- **Android**: for a USB device use `scripts/android-local-build.ps1 -Install`. CI APKs and local APKs have different signatures, so uninstall first when switching.
- **Ubuntu (WSL)**: install the `.deb` and launch it. WSLg windows belong to `msrdc` (title `open-media (Ubuntu)`).

## Pitfalls found (do not repeat)

- Without the updater permissions (`updater:default`, `process:allow-restart`) the update check is silently denied → `capabilities/desktop.json`.
- "Command … not found" after adding a command → it was missing from the **other** `generate_handler!` list.
- Changing only `src/` still needs a **Rust rebuild** (the UI is embedded) on Android and desktop; bump the mtime of `lib.rs`.
- Right after playback starts the engine reports "stopped" for a moment; do not treat that as the end (`barSeenPlaying`). When playback starts the message becomes empty, so write "Playing" explicitly.
- **While exclusive mode is active the DAC is in use**; other code fails with `0x8889000A`. Stop the app before tests.
- Windows AutoPlay gives priority to `EventHandlersDefaultSelection` (PowerDVD etc. write themselves there); `UserChosenExecuteHandlers` alone loses. Backups are under `HKCU\Software\open-media\AutoplayBackup*`.
- On Japanese Windows `reg query` prints the default value as `(既定)`, not `(Default)`; handle both.
- Japanese text shows as boxes on Ubuntu → no Japanese font installed. Fixed with a bundled font (`src/fonts/`, Noto Sans JP, SIL OFL).
- CI Windows poppler fetch failed → backslashes in `RUNNER_TEMP` and a changed zip layout (fixed with `cygpath` and a `share/poppler` search).
- In PowerShell, quoted `gh ... --jq '...'` gets mangled. Call it from bash.
- When rewriting files from Python keep their line endings (double conversion produces `\r\r\n`).

## Secrets (values are never written here)

- Updater signing key: `F:\open-media-updater.key` and `.pub` (**keep another copy elsewhere**; if lost, updates can no longer be shipped). Registered as the GitHub secret `TAURI_SIGNING_PRIVATE_KEY`.
- GitHub: `gh auth` login (organization `aon-co-jp`).
- VPS: `ssh conoha` (`~/.ssh/config`). A token for private GitHub repos is already set up on the VPS.

## Status and next steps (2026-10-14)

- Released: `v0.1.10` (auto-update, single instance, bundled Japanese font, file associations, AutoPlay).
- Verified: Windows hardware (install, update, CD playback, bridge), Android device (MP4/FLAC), Ubuntu on WSL (Japanese rendering).
- Open:
  1. Result of re-inserting the CD for the PowerDVD-takes-priority issue (the fix is in `v0.1.8` and later)
  2. Real DVD / Blu-ray playback; the AutoPlay event name for Blu-ray
  3. macOS hardware, Android TV hardware
  4. Real listening comparison of the loopback bridge for YouTube etc. (music only)

## Keeping docs current

Update the README "Status", this file and `PORTING.en.md` in **both languages** whenever features or verification change, commit and push (and `git pull` in `/root/repository/open-media` on the VPS).
