# make-disk

**Sprachen**: [日本語](../README.md) | [English](README.en.md) | [简体中文](README.zh-CN.md) | [繁體中文(台灣)](README.zh-TW.md) | [한국어](README.ko.md) | Deutsch | [Français](README.fr.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | [فارسی](README.iran%28Perusha%29.md) | [العربية](README.ar.md)

Plattformübergreifende GUI-Anwendung (Rust + Tauri) zum Brennen von CD/DVD/Blu-ray und zum Konvertieren von Audio/Video.
Eine einzige Codebasis; nur die Installer unterscheiden sich je nach Betriebssystem.

## Funktionen

- **Ein-/Ausgabe**: mehrere Quelldateien (Audio/Video/PDF) und einen Ausgabeordner wählen. Konvertierung, ISO-Abbild und Brennen
  lassen sich per Kontrollkästchen gleichzeitig wählen (mehrere Formate, Dateien und Disc-Typen laufen asynchron parallel).
- **Audio**: MP3 / WAV / FLAC / AAC / OGG / **Opus** / **AC-3 / E-AC-3 (5.1–7.1)** /
  **DSD64·128·256·512·1024 (DSF)**. Da ffmpeg kein DSD schreiben kann, sind die PCM→1-Bit-Delta-Sigma-Modulation
  (5. Ordnung, ein Thread pro Kanal) und das Schreiben von DSF selbst implementiert (per Rundlauf durch den DSF-Decoder von ffmpeg verifiziert).
- **Video**: MP4 / MKV / AVI / MOV / WebM / **AV1** (bevorzugt libsvtav1, sonst libaom) /
  **HEVC 10 Bit (HDR10)** / **verlustfreie Stream-Kopie** (behält Dolby Vision, Atmos, 5.1/7.1-Surround).
  Auflösung (720×480 bis 8K, PAL, benutzerdefiniert, KI-optimiert) und Bildrate (24/30/60/120, benutzerdefiniert) sind wählbar;
  die Auswahl wird nach Konvertierungsrichtung eingeschränkt (BD→DVD: Standard-DVD-Auflösung oder Full HD; DVD→BD: Full HD oder 4K).
- **Hochauflösendes PCM (für R-2R-/Multibit-DACs)**: WAV mit 352,8 kHz / 384 kHz (24 und 32 Bit) sowie 705,6 kHz / 768 kHz (32 Bit),
  mit dem besten verfügbaren Resampler (soxr, sonst hochpräzise swresample-Einstellung) und TPDF-Dither. Gemessenes SNR gegen einen exakten Referenzsinus:
  352,8 kHz/24 Bit = 141,2 dB, 384 kHz/32 Bit = 150,2 dB.
  Beim Erzeugen von DSD wird kein PCM mitgeschrieben (auf Hardware ohne DSD wandelt der Player DSD automatisch in PCM um, eine PCM-Version wäre nur Platzverschwendung).
  Gemessenes DSD-Rundlauf-SNR: DSD64 = 99,6 dB, DSD128 = 132,4 dB.
- **KI-Superauflösung (Video)**: Real-ESRGAN (MIT) als bei Bedarf geladenes Plugin (z. B. DVD→4K). Nutzt die GPU (NCNN-Vulkan), wenn ein Vulkan-Gerät funktioniert,
  sonst automatisch unsere **eigene Rust-CPU-Implementierung** (AVX2+FMA automatisch; Ausgabe-PSNR 42,0 dB gegenüber der offiziellen GPU-Implementierung).
- **KI-Bandbreitenerweiterung (experimentell)**: ergänzt fehlende Höhen bandbegrenzter Audiodaten mit einem trainierten Modell (LavaSR, Apache-2.0, ausgeführt mit reinem Rust-tract, ca. 56 MB beim ersten Mal).
  **Das vorhandene Band wird nie verändert**; die erzeugten Höhen werden durch eine Extrapolation der Eingangshüllkurve begrenzt (die rohe Modellausgabe verschlechterte Musik, daher dieses Design).
  Gemessen (Musik bei 8/12 kHz bandbegrenzt): LSD im Band mit Referenz 3,4→1,5 und 2,7→1,6, tiefes Band unverändert. Es ist Synthese, keine Wiederherstellung, und wirkt nicht auf Quellen ohne Bandbegrenzung.
- **KI-Rauschunterdrückung**: enthält ein echtes trainiertes neuronales Netz (RNNoise), mit dem echten Modell verifiziert. Es ist überwiegend auf Sprache trainiert, bei Musik ist die Wirkung gering.
- **Bitrate / Kapazität**: fest, automatisch aus der Disc-Kapazität, sowie ein Modus „maximale Qualität“ (verlustfreies WAV + ISO, wenn kein Format gewählt ist).
  Die automatische Bitrate ist auf die Bitrate der Quelle begrenzt; reine Audioausgaben verwenden korrekt `-b:a`.
- **Schneiden der Quelle**: „Nach Größe schneiden?“ und „Nach Zeit schneiden?“ werden mit JA/NEIN beantwortet (gegenseitig ausschließend, genau eines ist JA; Standard ist Größe).
  Nachbearbeitung per Kontrollkästchen: „Disc voll ausnutzen“ (CD / DVD 1–2 Schichten / Blu-ray 1–4 Schichten) und „KI-Auto-Schnitt“ (Stilleerkennung).
- **Grenzen von Wiedergabestandards**: zeigt maximale kHz, Bittiefe und Bitrate von CD / DVD-Video / DVD-Audio / Blu-ray / Ultra HD Blu-ray / nur PC
  und konvertiert passend zum gewählten Standard (Quellen unterhalb der Grenze bleiben unverändert; kein Upsampling).
- **Bearbeitung**: Schneiden mehrerer Bereiche (Datei in Abschnitt 8 wählen und Bereiche mit Video-/Audio-Vorschau festlegen), Zusammenfügen von Dateien,
  Aufteilen in gleiche Abschnitte oder nach Größe (der Rest wird automatisch so angepasst, dass er die Disc füllt).
- **PDF**: Doppelseiten als Bilder (Rechts-/Linksbindung, bis 4K) und Stapelumwandlung der Bindungsrichtung (Umkehr der Seitenreihenfolge).
- **Brennen (Windows)**: erkennt optische Laufwerke automatisch, erstellt ISOs mit IMAPI2 (japanische Dateinamen bleiben erhalten) und brennt.
  Auf echter Hardware (BD-RE-Laufwerk + CD-R) verifiziert: 3,5-Stunden-Video → CD-füllendes Audio → ISO → Brennen erfolgreich.
- **Sonstiges**: automatische Update-Prüfung beim Start (zweisprachiger Dialog); versionierte rs-ffmpeg-/rs-xorriso-Plugins (eine identische Version wird nicht überschrieben).

## Nicht unterstützt / Einschränkungen (ehrliche Offenlegung)

- **Das Umgehen von Kopierschutz (CSS/AACS usw.) ist nicht implementiert** (kann illegal sein). Nur ungeschützte Discs.
- **Dolby Vision / Atmos / Dolby Cinema / IMAX / 4DX können nicht neu erzeugt werden** (lizenzierte Formate). Nur Erhalt (Stream-Kopie) und kompatible niedrigere Formate.
- **KI-Video-Superauflösung (Real-ESRGAN) ist sehr langsam — nur für kurze Clips** (pro 720×480-Bild: schnelles Modell ca. 1,2 s auf einer 32-Thread-AVX2-CPU, ca. 4,6 s auf einer GT 730;
  hochwertiges Modell ca. 110 s auf der GT 730). Die CPU-Version unterstützt nur das schnelle Modell. **Die KI-Bandbreitenerweiterung für Audio ist experimentell** (Synthese, keine Garantie für Hörqualität).
  Die „KI-optimierten“ Auflösungs-/FPS-Optionen sind einfache Heuristiken, keine KI-Superauflösung.
- DSD-Dateien sind sehr groß (pro Stereominute: DSD64 ≈ 42 MB … DSD1024 ≈ 678 MB) und sind DSF-Dateien, keine Super-Audio-CD.
- Full HD auf DVD liegt außerhalb des DVD-Video-Standards (max. 720×480/576); normale DVD-Player skalieren nicht automatisch herunter, daher ist eine Wiedergabe eventuell nicht möglich.
- Brennen unter Linux/macOS setzt das originale xorriso voraus (nicht enthalten). Discs werden als Daten-Discs geschrieben (Audio-CD / CD-DA wird nicht unterstützt).
- iOS wird nicht unterstützt (kein Testgerät).

## Externe Abhängigkeiten zur Laufzeit

Die Windows-/Linux-Installer enthalten ffmpeg/ffprobe und rs-ffmpeg/rs-xorriso (siehe `src-tauri/src/engine/sidecar.rs` und `plugins.rs`);
fehlen sie, werden die Werkzeuge im `PATH` verwendet. ffmpeg und das originale xorriso sind unter macOS nicht enthalten.

## Entwicklung

```bash
npm install
npm run tauri dev
cd src-tauri && cargo test --lib -- --test-threads=1
```

## Downloads / Installer

Windows (.msi/.exe), macOS (.dmg, Intel/Apple Silicon), Linux (.deb/.rpm/.AppImage) und Android (Universal-APK) gibt es unter
[GitHub Releases](https://github.com/aon-co-jp/make-disk/releases/latest). Ein gepushtes `v*`-Tag baut alle Plattformen über
`.github/workflows/release.yml`. Projektseite: <https://easy-web.tokyo/make-disk/>

## Lizenz

MIT (das enthaltene RNNoise-Modell ist laut seinem Autor nicht urheberrechtlich geschützt; siehe `CLAUDE.md`).
