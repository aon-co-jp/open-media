# Notes de passation entre sessions (make-disk) — résumé

**Langues** : [日本語 (texte complet, faisant foi)](../PORTING.md) | [English](PORTING.en.md) | [简体中文](PORTING.zh-CN.md) | [繁體中文(台灣)](PORTING.zh-TW.md) | [한국어](PORTING.ko.md) | [Deutsch](PORTING.de.md) | Français | [Русский](PORTING.ru.md) | [Українська](PORTING.uk.md) | [فارسی](PORTING.iran%28Perusha%29.md) | [العربية](PORTING.ar.md)

> Résumé de l'état d'avancement et des prochaines étapes. L'historique complet des notes de reprise est le [`PORTING.md`](../PORTING.md) japonais ; les décisions techniques sont dans [`CLAUDE.md`](../CLAUDE.md).

## Où en sommes-nous (2026-09-23)

- Déjà livré : gravure IMAPI2 (vérifiée avec un vrai CD-R), AV1/Opus, conservation Dolby/surround, réduction de bruit IA (RNNoise), DSD64–1024 (DSF) et WAV DoP,
  PCM haute résolution, super-résolution vidéo IA (GPU/CPU), extension de bande audio expérimentale, extraction de CD audio (vérifiée avec un vrai disque), pistes audio/sous-titres multiples en MKV.
- 2026-09-23 : correction de la découpe par plages inutilisable depuis la section 8 ; plus de PCM créé avec le DSD ; OUI/NON exclusifs « couper par taille / par durée » (taille par défaut) ;
  cases de post-traitement « remplir le disque » et « découpe IA des silences » ; limites des normes de lecture (CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / PC uniquement) ; documentation multilingue.

## Prochaines étapes

1. E2E avec de vrais fichiers pour les fonctions du 2026-09-23 : positions de coupe par taille/durée, plafonds `-ar` / profondeur de bits / débit par norme, débit « remplir le disque ».
2. Gravure de CD audio (CD-DA) (IMAPI2 TrackAtOnce ; actuellement disques de données uniquement).
3. Retirer les outils `rs-*` de l'installateur et les télécharger à la demande depuis les publications des dépôts associés.
4. Exporter « vidéo + audio DSD » en lot (fichier vidéo + `.dsf` + `.obar.json` pour `open-bar`) et extraire le modulateur delta-sigma vers `open-mqa-dsd`.
5. Extraction de BD/DVD non protégés (aucun contournement de protection ne sera implémenté).

## Dépôts associés

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — cette application
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) / [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — versions Rust de FFmpeg / xorriso
- [aon-co-jp/open-cpu](https://github.com/aon-co-jp/open-cpu) — détection des jeux d'instructions CPU
- [aon-co-jp/open-cuda](https://github.com/aon-co-jp/open-cuda) — abstraction du calcul GPU
- [aon-co-jp/open-mqa](https://github.com/aon-co-jp/open-mqa) / [aon-co-jp/open-mqa-dsd](https://github.com/aon-co-jp/open-mqa-dsd) / [aon-co-jp/open-bar](https://github.com/aon-co-jp/open-bar) — chaîne audio haute résolution, outils DSD, lecteur
