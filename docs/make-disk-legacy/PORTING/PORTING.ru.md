# Заметки о передаче между сессиями (make-disk) — кратко

**Языки**: [日本語 (полный текст, основной)](../PORTING.md) | [English](PORTING.en.md) | [简体中文](PORTING.zh-CN.md) | [繁體中文(台灣)](PORTING.zh-TW.md) | [한국어](PORTING.ko.md) | [Deutsch](PORTING.de.md) | [Français](PORTING.fr.md) | Русский | [Українська](PORTING.uk.md) | [فارسی](PORTING.iran%28Perusha%29.md) | [العربية](PORTING.ar.md)

> Краткое описание текущего состояния и следующих шагов. Полная история заметок для возобновления — японский [`PORTING.md`](../PORTING.md); технические решения — в [`CLAUDE.md`](../CLAUDE.md).

## Текущее состояние (2026-09-23)

- Уже сделано: запись через IMAPI2 (проверено на реальном CD-R), AV1/Opus, сохранение Dolby/объёмного звука, ИИ-шумоподавление (RNNoise), DSD64–1024 (DSF) и DoP WAV,
  PCM высокого разрешения, ИИ-сверхразрешение видео (GPU/CPU), экспериментальное расширение полосы аудио, извлечение аудио-CD (проверено на реальном диске), несколько аудиодорожек/субтитров в MKV.
- 2026-09-23: исправлено неработающее из раздела 8 вырезание участков; при создании DSD больше не создаётся PCM; взаимоисключающие ДА/НЕТ «обрезать по размеру / по времени» (по умолчанию размер);
  флажки постобработки «заполнить диск» и «ИИ-обрезка тишины»; пределы стандартов воспроизведения (CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / только ПК); многоязычная документация.

## Следующие шаги

1. E2E на реальных файлах для функций от 2026-09-23: позиции обрезки по размеру/времени, ограничения `-ar` / разрядности / битрейта по стандартам, битрейт «заполнить диск».
2. Запись аудио-CD (CD-DA) (IMAPI2 TrackAtOnce; сейчас только диски с данными).
3. Убрать инструменты `rs-*` из установщика и загружать их по требованию из выпусков родственных репозиториев.
4. Экспорт «видео + DSD-аудио» набором (видеофайл + `.dsf` + `.obar.json` для `open-bar`) и вынос дельта-сигма-модулятора в `open-mqa-dsd`.
5. Извлечение незащищённых BD/DVD (обход защиты от копирования реализовываться не будет).

## Связанные репозитории

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — это приложение
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) / [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — версии FFmpeg / xorriso на Rust
- [aon-co-jp/open-cpu](https://github.com/aon-co-jp/open-cpu) — определение наборов инструкций CPU
- [aon-co-jp/open-cuda](https://github.com/aon-co-jp/open-cuda) — абстракция вычислений на GPU
- [aon-co-jp/open-mqa](https://github.com/aon-co-jp/open-mqa) / [aon-co-jp/open-mqa-dsd](https://github.com/aon-co-jp/open-mqa-dsd) / [aon-co-jp/open-bar](https://github.com/aon-co-jp/open-bar) — конвейер Hi-Res-аудио, инструменты DSD, плеер
