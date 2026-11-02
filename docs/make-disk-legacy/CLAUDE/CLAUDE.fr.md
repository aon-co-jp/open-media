# Règles de développement et d'environnement (make-disk) — résumé

**Langues** : [日本語 (texte complet, faisant foi)](../CLAUDE.md) | [English](CLAUDE.en.md) | [简体中文](CLAUDE.zh-CN.md) | [繁體中文(台灣)](CLAUDE.zh-TW.md) | [한국어](CLAUDE.ko.md) | [Deutsch](CLAUDE.de.md) | Français | [Русский](CLAUDE.ru.md) | [Українська](CLAUDE.uk.md) | [فارسی](CLAUDE.iran%28Perusha%29.md) | [العربية](CLAUDE.ar.md)

> Ceci est un résumé. Le texte complet, avec tout l'historique de développement (entrées HANDOFF), est le [`CLAUDE.md`](../CLAUDE.md) japonais.
> Les règles communes à tous les dépôts (poursuite autonome, vérification rigoureuse, etc.) suivent le `CLAUDE.md` de [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z).

## Rôle du dépôt

Application graphique (Rust + Tauri) de gravure CD/DVD/Blu-ray, de conversion audio/vidéo et de création d'images ISO, avec une seule base de code pour Windows/macOS/Linux (et Android).
Les différences entre plateformes sont limitées aux installateurs ; l'application (`src-tauri/src`, `src`) est une base de code unique.

## Architecture

- Frontend : `src/` (JS natif ; appelle les commandes Rust via l'IPC Tauri par `window.__TAURI__`, car les imports nus ne fonctionnent pas sans bundler).
- Backend : `src-tauri/src/engine/`
  - `probe.rs` — durée, codecs, fréquence d'échantillonnage, caractéristiques Dolby via ffprobe (repli rs-ffmpeg)
  - `convert.rs` — conversion ffmpeg, contrôle du débit, découpe, découpe multi-plages (copie de flux par défaut, encodeur GPU pour les coupes à l'image près)
  - `capacity.rs` — débit maximal d'après la capacité du disque et avertissement de qualité à 4 niveaux
  - `dsd.rs` — modulateur delta-sigma PCM→1 bit et écriture DSF maison, WAV DoP
  - `cdda.rs` — extraction de CD audio (Windows, lecture sécurisée simple)
  - `iso.rs` / `burn.rs` / `windows_imapi.rs` — création d'ISO et gravure (IMAPI2 sous Windows, xorriso ailleurs)
  - `ai_upscale.rs` / `cpu_sr.rs` / `audio_sr.rs` — Real-ESRGAN (GPU/CPU), extension de bande audio
  - `mkv_tracks.rs` — conservation/ajout de plusieurs pistes audio et sous-titres en MKV
  - `plugins.rs` / `sidecar.rs` — outils inclus/téléchargés avec gestion des versions
  - `cpu.rs` — détection des jeux d'instructions CPU via `open-cpu` (indications de vitesse et choix du `-preset` x264)

## Règles établies

- **Aucun contournement de protection anticopie** (CSS/AACS, etc.). Disques non protégés uniquement.
- **Aucune création de Dolby Vision / Atmos / IMAX / 4DX** (sous licence) ; seulement conservation par copie de flux et formats inférieurs compatibles.
- **Pas de MQA** (brevets/secrets commerciaux). La haute résolution suit la voie des formats ouverts de `aon-co-jp/open-mqa`.
- **Lors de la création de DSD, aucun PCM n'est créé en parallèle** (le lecteur convertit automatiquement en PCM sur un matériel sans DSD). Le WAV DoP est une donnée DSD, donc autorisé.
- La modulation DSD reste **séquentielle par canal (identique au bit près)** ; la modulation par segments parallèles détruisait le RSB (mesuré) et a été rejetée.
- Les fonctions « IA » sont décrites honnêtement : la découpe auto par silence et la résolution/FPS « optimisées par IA » sont des heuristiques ; l'extension de bande audio est de la synthèse.
- Le Full HD sur DVD sort de la norme DVD-Video ; les lecteurs de salon ne réduisent pas la résolution automatiquement. L'interface garde cet avertissement (l'utilisateur a refusé la combinaison DVD-Video + fichier Full HD).
- Langues de la documentation : le japonais fait foi ; `README/`, `CLAUDE/`, `PORTING/` contiennent anglais, chinois simplifié, chinois traditionnel (Taïwan), coréen, allemand, français, russe, ukrainien, persan (Iran, suffixe de fichier `.iran(Perusha)`) et arabe (CLAUDE/PORTING en résumé).

## Règles de vérification

- Une CI réussie ne prouve pas que ça fonctionne. Avant d'annoncer la fin, vérifier réellement : `npm run lint` (eslint `no-undef`), test de l'interface dans un navigateur avec les API Tauri simulées, tests Rust (`cargo test --lib -- --test-threads=1`) et, si possible, E2E avec de vrais fichiers / du vrai matériel.
- Indiquer honnêtement ce qui n'a pas été vérifié.

## Publications

Pousser une étiquette `v*` déclenche `.github/workflows/release.yml`, qui construit les installateurs Windows/macOS/Linux/Android et les publie sur GitHub Releases.

## État actuel (2026-09-23)

Correction de la découpe par plages inutilisable depuis la section 8 ; plus de PCM créé avec le DSD ; découpe repensée en OUI/NON exclusifs « couper par taille / par durée » (taille par défaut), avec « remplir le disque » et « découpe IA des silences » en cases de post-traitement ;
ajout des limites des normes de lecture (CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / PC uniquement) pour les kHz, la profondeur de bits et le débit maximaux. L'E2E avec de vrais fichiers reste à faire.
