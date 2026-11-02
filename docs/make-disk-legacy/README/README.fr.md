# make-disk

**Langues** : [日本語](../README.md) | [English](README.en.md) | [简体中文](README.zh-CN.md) | [繁體中文(台灣)](README.zh-TW.md) | [한국어](README.ko.md) | [Deutsch](README.de.md) | Français | [Русский](README.ru.md) | [Українська](README.uk.md) | [فارسی](README.iran%28Perusha%29.md) | [العربية](README.ar.md)

Application graphique multiplateforme (Rust + Tauri) pour graver des CD/DVD/Blu-ray et convertir de l'audio/vidéo.
Une seule base de code ; seuls les installateurs diffèrent selon le système d'exploitation.

## Fonctionnalités

- **Entrées/sorties** : choix de plusieurs fichiers sources (audio/vidéo/PDF) et d'un dossier de sortie. Conversion, image ISO et gravure
  peuvent être cochées simultanément (plusieurs formats, fichiers et types de disques s'exécutent en parallèle et de façon asynchrone).
- **Audio** : MP3 / WAV / FLAC / AAC / OGG / **Opus** / **AC-3 / E-AC-3 (5.1–7.1)** /
  **DSD64·128·256·512·1024 (DSF)**. ffmpeg ne sait pas écrire le DSD : la modulation delta-sigma PCM→1 bit
  (5e ordre, un thread par canal) et l'écriture DSF sont implémentées en Rust et vérifiées par aller-retour via le décodeur DSF de ffmpeg.
- **Vidéo** : MP4 / MKV / AVI / MOV / WebM / **AV1** (libsvtav1 de préférence, sinon libaom) /
  **HEVC 10 bits (HDR10)** / **copie de flux sans perte** (conserve Dolby Vision, Atmos, surround 5.1/7.1).
  Résolution (720×480 à 8K, PAL, personnalisée, optimisée par IA) et images par seconde (24/30/60/120, personnalisé) au choix ;
  les choix sont restreints selon le sens de conversion (BD→DVD : résolution DVD standard ou Full HD ; DVD→BD : Full HD ou 4K).
- **PCM haute résolution (pour DAC R-2R / multibit)** : WAV en 352,8 kHz / 384 kHz (24 et 32 bits) et 705,6 kHz / 768 kHz (32 bits),
  avec le meilleur rééchantillonneur disponible (soxr, sinon réglage swresample haute précision) et un dither TPDF. RSB mesuré face à une sinusoïde de référence exacte :
  352,8 kHz/24 bits = 141,2 dB, 384 kHz/32 bits = 150,2 dB.
  Lors de la création de DSD, aucun PCM n'est produit en parallèle (sur un matériel sans DSD, le lecteur convertit automatiquement le DSD en PCM ; une version PCM ne ferait que gaspiller de la place).
  RSB mesuré en aller-retour DSD : DSD64 = 99,6 dB, DSD128 = 132,4 dB.
- **Super-résolution IA (vidéo)** : Real-ESRGAN (MIT) sous forme de plugin chargé à la demande (p. ex. DVD→4K). Utilise le GPU (NCNN-Vulkan) si un périphérique Vulkan fonctionne,
  sinon bascule automatiquement sur notre **propre implémentation CPU en Rust** (AVX2+FMA automatiques ; PSNR de sortie 42,0 dB par rapport à l'implémentation GPU officielle).
- **Extension de bande par IA (expérimental)** : ajoute les aigus manquants d'un audio à bande limitée avec un modèle entraîné (LavaSR, Apache-2.0, exécuté par tract en Rust pur, ~56 Mo au premier usage).
  **La bande existante n'est jamais modifiée** ; les aigus générés sont plafonnés par une extrapolation de l'enveloppe d'entrée (la sortie brute du modèle dégradait la musique, d'où cette conception).
  Mesures (musique limitée à 8/12 kHz) : LSD dans la bande avec référence 3,4→1,5 et 2,7→1,6, graves inchangés. C'est de la synthèse, pas une restauration ; rien n'est fait sur les sources non limitées en bande.
- **Réduction de bruit par IA** : intègre un vrai réseau neuronal entraîné (RNNoise), vérifié avec le vrai modèle. Entraîné surtout sur la voix, son effet sur la musique est modeste.
- **Débit / capacité** : fixe, automatique d'après la capacité du disque, et un mode « qualité maximale » (WAV sans perte + ISO si aucun format n'est choisi).
  Le débit automatique est plafonné au débit de la source ; les sorties audio seules utilisent correctement `-b:a`.
- **Découpe de la source** : « Couper par taille ? » et « Couper par durée ? » se répondent par OUI/NON (mutuellement exclusifs, exactement un OUI ; la taille par défaut).
  Post-traitement par cases à cocher : « Remplir le disque » (CD / DVD 1–2 couches / Blu-ray 1–4 couches) et « Découpe auto par IA » (détection de silence).
- **Limites des normes de lecture** : affiche les kHz, la profondeur de bits et le débit maximaux de CD / DVD-Video / DVD-Audio / Blu-ray / Ultra HD Blu-ray / PC uniquement,
  et convertit pour respecter la norme choisie (les sources déjà en dessous restent inchangées ; pas de suréchantillonnage).
- **Montage** : découpe de plusieurs plages (choisir le fichier dans la section 8 et définir les plages en prévisualisant la vidéo/l'audio), concaténation de fichiers,
  découpage à intervalles égaux ou par taille (le reste est ajusté automatiquement pour remplir le disque).
- **PDF** : images en double page (reliure à droite/à gauche, jusqu'à 4K) et conversion en lot du sens de reliure (inversion de l'ordre des pages).
- **Gravure (Windows)** : détecte automatiquement les lecteurs optiques, crée les ISO avec IMAPI2 (conserve les noms de fichiers japonais) et grave.
  Vérifié sur matériel réel (graveur BD-RE + CD-R) : vidéo de 3,5 h → audio remplissant le CD → ISO → gravure réussie.
- **Autres** : vérification automatique des mises à jour au démarrage (dialogue bilingue) ; plugins rs-ffmpeg / rs-xorriso versionnés (une version identique n'est pas écrasée).

## Non pris en charge / limites (en toute transparence)

- **Le contournement des protections anticopie (CSS/AACS, etc.) n'est pas implémenté** (potentiellement illégal). Disques non protégés uniquement.
- **Impossible de créer du Dolby Vision / Atmos / Dolby Cinema / IMAX / 4DX** (formats sous licence). Seulement la conservation (copie de flux) et des formats inférieurs compatibles.
- **La super-résolution vidéo IA (Real-ESRGAN) est très lente — clips courts uniquement** (par image 720×480 : modèle rapide ~1,2 s sur un CPU AVX2 32 threads, ~4,6 s sur un GPU GT 730 ;
  modèle haute qualité ~110 s sur la GT 730). La version CPU ne prend en charge que le modèle rapide. **L'extension de bande audio par IA est expérimentale** (synthèse, sans garantie de qualité perçue).
  Les options de résolution/FPS « optimisées par IA » sont de simples heuristiques, pas de la super-résolution.
- Les fichiers DSD sont énormes (par minute stéréo : DSD64 ≈ 42 Mo … DSD1024 ≈ 678 Mo) et sont des fichiers DSF, pas un disque Super Audio CD.
- Le Full HD sur DVD sort de la norme DVD-Video (max. 720×480/576) ; les lecteurs DVD de salon ne réduisent pas automatiquement la résolution, la lecture peut donc échouer.
- La gravure sous Linux/macOS nécessite le xorriso d'origine (non inclus). Les disques sont gravés comme disques de données (CD audio / CD-DA non pris en charge).
- iOS n'est pas pris en charge (pas d'appareil de test).

## Dépendances externes à l'exécution

Les installateurs Windows/Linux incluent ffmpeg/ffprobe et rs-ffmpeg/rs-xorriso (voir `src-tauri/src/engine/sidecar.rs` et `plugins.rs`) ;
à défaut, les outils du `PATH` sont utilisés. ffmpeg et le xorriso d'origine ne sont pas inclus sous macOS.

## Développement

```bash
npm install
npm run tauri dev
cd src-tauri && cargo test --lib -- --test-threads=1
```

## Téléchargements / installateurs

Windows (.msi/.exe), macOS (.dmg, Intel/Apple Silicon), Linux (.deb/.rpm/.AppImage) et Android (APK universel) sont sur
[GitHub Releases](https://github.com/aon-co-jp/make-disk/releases/latest). Pousser une étiquette `v*` construit toutes les plateformes via
`.github/workflows/release.yml`. Page de présentation : <https://easy-web.tokyo/make-disk/>

## Licence

MIT (le modèle RNNoise inclus est déclaré par son auteur comme non soumis au droit d'auteur ; voir `CLAUDE.md`).
