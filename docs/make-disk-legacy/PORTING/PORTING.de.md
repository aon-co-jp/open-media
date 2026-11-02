# Übergabenotizen zwischen Sitzungen (make-disk) — Zusammenfassung

**Sprachen**: [日本語 (vollständig, maßgeblich)](../PORTING.md) | [English](PORTING.en.md) | [简体中文](PORTING.zh-CN.md) | [繁體中文(台灣)](PORTING.zh-TW.md) | [한국어](PORTING.ko.md) | Deutsch | [Français](PORTING.fr.md) | [Русский](PORTING.ru.md) | [Українська](PORTING.uk.md) | [فارسی](PORTING.iran%28Perusha%29.md) | [العربية](PORTING.ar.md)

> Zusammenfassung des aktuellen Stands und der nächsten Schritte. Maßgeblich für die vollständige Historie der Wiederaufnahme-Notizen ist die japanische [`PORTING.md`](../PORTING.md); technische Entscheidungen stehen in [`CLAUDE.md`](../CLAUDE.md).

## Aktueller Stand (2026-09-23)

- Bisher umgesetzt: Brennen mit IMAPI2 (mit echter CD-R verifiziert), AV1/Opus, Erhalt von Dolby/Surround, KI-Rauschunterdrückung (RNNoise), DSD64–1024 (DSF) und DoP-WAV,
  hochauflösendes PCM, KI-Video-Superauflösung (GPU/CPU), experimentelle Audio-Bandbreitenerweiterung, Audio-CD-Rippen (mit echter Disc verifiziert), mehrere Audio-/Untertitelspuren in MKV.
- 2026-09-23: in Abschnitt 8 nicht bedienbaren Bereichsschnitt behoben; beim Erzeugen von DSD wird kein PCM mehr mitgeschrieben; gegenseitig ausschließendes JA/NEIN „nach Größe / nach Zeit schneiden“ (Standard: Größe);
  Nachbearbeitungs-Kontrollkästchen „Disc voll ausnutzen“ und „KI-Stille-Schnitt“; Grenzen von Wiedergabestandards (CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / nur PC); mehrsprachige Dokumentation.

## Nächste Schritte

1. E2E mit echten Dateien für die Funktionen vom 2026-09-23: Schnittpositionen nach Größe/Zeit, `-ar`-/Bittiefe-/Bitraten-Grenzen je Wiedergabestandard, Bitrate für „Disc voll ausnutzen“.
2. Brennen von Audio-CDs (CD-DA) (IMAPI2 TrackAtOnce; derzeit nur Daten-Discs).
3. `rs-*`-Werkzeuge aus dem Installer entfernen und bei Bedarf aus den Releases der Schwester-Repositories laden.
4. „Video + DSD-Audio“ als Set exportieren (Videodatei + `.dsf` + `.obar.json` für `open-bar`) und den Delta-Sigma-Modulator nach `open-mqa-dsd` auslagern.
5. Rippen ungeschützter BD/DVD (eine Umgehung von Kopierschutz wird nicht implementiert).

## Verwandte Repositories

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — diese Anwendung
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) / [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — Rust-Umsetzungen von FFmpeg / xorriso
- [aon-co-jp/open-cpu](https://github.com/aon-co-jp/open-cpu) — CPU-Befehlssatzerkennung
- [aon-co-jp/open-cuda](https://github.com/aon-co-jp/open-cuda) — GPU-Rechenabstraktion
- [aon-co-jp/open-mqa](https://github.com/aon-co-jp/open-mqa) / [aon-co-jp/open-mqa-dsd](https://github.com/aon-co-jp/open-mqa-dsd) / [aon-co-jp/open-bar](https://github.com/aon-co-jp/open-bar) — Hi-Res-Audio-Pipeline, DSD-Werkzeuge, Player
