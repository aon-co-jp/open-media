# Entwicklungsrichtlinien & Umgebungsregeln (make-disk) — Zusammenfassung

**Sprachen**: [日本語 (vollständig, maßgeblich)](../CLAUDE.md) | [English](CLAUDE.en.md) | [简体中文](CLAUDE.zh-CN.md) | [繁體中文(台灣)](CLAUDE.zh-TW.md) | [한국어](CLAUDE.ko.md) | Deutsch | [Français](CLAUDE.fr.md) | [Русский](CLAUDE.ru.md) | [Українська](CLAUDE.uk.md) | [فارسی](CLAUDE.iran%28Perusha%29.md) | [العربية](CLAUDE.ar.md)

> Dies ist eine Zusammenfassung. Maßgeblich ist der vollständige japanische Text [`CLAUDE.md`](../CLAUDE.md) mit der gesamten Entwicklungshistorie (HANDOFF-Einträge).
> Für alle Repositories geltende Regeln (selbstständiges Weiterarbeiten, gründliche Verifikation usw.) richten sich nach `CLAUDE.md` in [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z).

## Rolle des Repositorys

Eine GUI-Anwendung (Rust + Tauri) zum Brennen von CD/DVD/Blu-ray, Konvertieren von Audio/Video und Erstellen von ISO-Abbildern — eine Codebasis für Windows/macOS/Linux (und Android).
Plattformunterschiede sind auf die Installer (Bundles) beschränkt; die Anwendung selbst (`src-tauri/src`, `src`) ist eine einzige Codebasis.

## Architektur

- Frontend: `src/` (Vanilla-JS; ruft Rust-Befehle über Tauri-IPC via `window.__TAURI__` auf, da Bare-Imports ohne Bundler nicht funktionieren).
- Backend: `src-tauri/src/engine/`
  - `probe.rs` — Dauer, Codecs, Abtastrate, Dolby-Merkmale per ffprobe (Fallback rs-ffmpeg)
  - `convert.rs` — ffmpeg-Konvertierung, Bitratensteuerung, Trimmen, Schneiden mehrerer Bereiche (standardmäßig Stream-Kopie, für bildgenaue Schnitte GPU-Encoder)
  - `capacity.rs` — maximale Bitrate aus der Disc-Kapazität und eine vierstufige Qualitätswarnung
  - `dsd.rs` — eigener PCM→1-Bit-Delta-Sigma-Modulator und DSF-Writer, DoP-WAV
  - `cdda.rs` — Audio-CD-Rippen (Windows, einfaches Secure Read)
  - `iso.rs` / `burn.rs` / `windows_imapi.rs` — ISO-Erstellung und Brennen (IMAPI2 unter Windows, sonst xorriso)
  - `ai_upscale.rs` / `cpu_sr.rs` / `audio_sr.rs` — Real-ESRGAN (GPU/CPU), Audio-Bandbreitenerweiterung
  - `mkv_tracks.rs` — Erhalten/Hinzufügen mehrerer Audio- und Untertitelspuren in MKV
  - `plugins.rs` / `sidecar.rs` — mitgelieferte/heruntergeladene Werkzeuge mit Versionsverwaltung
  - `cpu.rs` — CPU-Befehlssatzerkennung über `open-cpu` (Geschwindigkeitshinweise und Wahl des x264-`-preset`)

## Feste Richtlinien

- **Keine Umgehung von Kopierschutz** (CSS/AACS usw.). Nur ungeschützte Discs.
- **Keine Neuerzeugung von Dolby Vision / Atmos / IMAX / 4DX** (lizenziert); nur Erhalt per Stream-Kopie und kompatible niedrigere Formate.
- **Kein MQA** (Patente/Geschäftsgeheimnisse). Hi-Res-Ausgabe folgt dem Open-Format-Weg von `aon-co-jp/open-mqa`.
- **Beim Erzeugen von DSD wird kein PCM mitgeschrieben** (auf Hardware ohne DSD wandelt der Player automatisch in PCM). DoP-WAV sind DSD-Daten und daher erlaubt.
- Die DSD-Modulation bleibt **sequenziell pro Kanal (bitgenau)**; segmentparallele Modulation zerstörte messbar das SNR und wurde verworfen.
- „KI“-Funktionen werden ehrlich offengelegt: Auto-Schnitt per Stilleerkennung und „KI-optimierte“ Auflösung/FPS sind Heuristiken; die Audio-Bandbreitenerweiterung ist Synthese.
- Full HD auf DVD liegt außerhalb von DVD-Video; normale Player skalieren nicht automatisch herunter. Die UI behält diesen Hinweis (eine Kombination aus DVD-Video + Full-HD-Datei hat der Nutzer abgelehnt).
- Dokumentationssprachen: Japanisch ist maßgeblich; `README/`, `CLAUDE/`, `PORTING/` enthalten Englisch, vereinfachtes Chinesisch, traditionelles Chinesisch (Taiwan), Koreanisch, Deutsch, Französisch, Russisch, Ukrainisch, Persisch (Iran, Dateisuffix `.iran(Perusha)`) und Arabisch (CLAUDE/PORTING als Zusammenfassung).

## Verifikationsregeln

- Ein erfolgreicher CI-Lauf ist kein Funktionsnachweis. Vor der Fertigmeldung real prüfen: `npm run lint` (eslint `no-undef`), Browser-Test der UI mit gestubbten Tauri-APIs, Rust-Tests (`cargo test --lib -- --test-threads=1`) und nach Möglichkeit E2E mit echten Dateien / echter Hardware.
- Nicht Verifiziertes ehrlich benennen.

## Releases

Ein gepushtes `v*`-Tag lässt `.github/workflows/release.yml` die Installer für Windows/macOS/Linux/Android bauen und auf GitHub Releases veröffentlichen.

## Aktueller Stand (2026-09-23)

Der in Abschnitt 8 nicht bedienbare Bereichsschnitt wurde behoben; beim Erzeugen von DSD wird kein PCM mehr mitgeschrieben; das Schneiden wurde als gegenseitig ausschließendes JA/NEIN „nach Größe / nach Zeit schneiden“ (Standard: Größe) neu gestaltet, mit „Disc voll ausnutzen“ und „KI-Stille-Schnitt“ als Nachbearbeitungs-Kontrollkästchen;
Grenzen von Wiedergabestandards (CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / nur PC) für maximale kHz, Bittiefe und Bitrate wurden ergänzt. E2E mit echten Dateien steht für diese Funktionen noch aus.
