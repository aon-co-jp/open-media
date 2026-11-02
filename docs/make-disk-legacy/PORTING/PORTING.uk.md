# Нотатки передачі між сесіями (make-disk) — стисло

**Мови**: [日本語 (повний текст, основний)](../PORTING.md) | [English](PORTING.en.md) | [简体中文](PORTING.zh-CN.md) | [繁體中文(台灣)](PORTING.zh-TW.md) | [한국어](PORTING.ko.md) | [Deutsch](PORTING.de.md) | [Français](PORTING.fr.md) | [Русский](PORTING.ru.md) | Українська | [فارسی](PORTING.iran%28Perusha%29.md) | [العربية](PORTING.ar.md)

> Стислий опис поточного стану та наступних кроків. Повна історія нотаток для відновлення — японський [`PORTING.md`](../PORTING.md); технічні рішення — у [`CLAUDE.md`](../CLAUDE.md).

## Поточний стан (2026-09-23)

- Уже зроблено: запис через IMAPI2 (перевірено на реальному CD-R), AV1/Opus, збереження Dolby/об'ємного звуку, ШІ-шумозаглушення (RNNoise), DSD64–1024 (DSF) і DoP WAV,
  PCM високої роздільності, ШІ-надроздільність відео (GPU/CPU), експериментальне розширення смуги аудіо, видобування аудіо-CD (перевірено на реальному диску), кілька аудіодоріжок/субтитрів у MKV.
- 2026-09-23: виправлено вирізання ділянок, яке не працювало з розділу 8; під час створення DSD більше не створюється PCM; взаємовиключні ТАК/НІ «обрізати за розміром / за часом» (типово розмір);
  прапорці постобробки «заповнити диск» і «ШІ-обрізання тиші»; межі стандартів відтворення (CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / лише ПК); багатомовна документація.

## Наступні кроки

1. E2E на реальних файлах для функцій від 2026-09-23: позиції обрізання за розміром/часом, обмеження `-ar` / розрядності / бітрейту за стандартами, бітрейт «заповнити диск».
2. Запис аудіо-CD (CD-DA) (IMAPI2 TrackAtOnce; зараз лише диски з даними).
3. Прибрати інструменти `rs-*` з інсталятора й завантажувати їх на вимогу з випусків споріднених репозиторіїв.
4. Експорт «відео + DSD-аудіо» набором (відеофайл + `.dsf` + `.obar.json` для `open-bar`) і винесення дельта-сигма-модулятора в `open-mqa-dsd`.
5. Видобування незахищених BD/DVD (обхід захисту від копіювання не реалізовуватиметься).

## Пов'язані репозиторії

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — цей застосунок
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) / [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — версії FFmpeg / xorriso на Rust
- [aon-co-jp/open-cpu](https://github.com/aon-co-jp/open-cpu) — визначення наборів інструкцій CPU
- [aon-co-jp/open-cuda](https://github.com/aon-co-jp/open-cuda) — абстракція обчислень на GPU
- [aon-co-jp/open-mqa](https://github.com/aon-co-jp/open-mqa) / [aon-co-jp/open-mqa-dsd](https://github.com/aon-co-jp/open-mqa-dsd) / [aon-co-jp/open-bar](https://github.com/aon-co-jp/open-bar) — конвеєр Hi-Res-аудіо, інструменти DSD, програвач
