# 工作階段交接備忘(make-disk)— 摘要

**語言**: [日本語(全文・正本)](../PORTING.md) | [English](PORTING.en.md) | [简体中文](PORTING.zh-CN.md) | 繁體中文(台灣) | [한국어](PORTING.ko.md) | [Deutsch](PORTING.de.md) | [Français](PORTING.fr.md) | [Русский](PORTING.ru.md) | [Українська](PORTING.uk.md) | [فارسی](PORTING.iran%28Perusha%29.md) | [العربية](PORTING.ar.md)

> 本文概述目前進度與下一步。歷次恢復備忘的全文以日文版 [`PORTING.md`](../PORTING.md) 為準;技術決策請見 [`CLAUDE.md`](../CLAUDE.md)。

## 目前進度(2026-09-23)

- 已完成: IMAPI2 燒錄(已以實際 CD-R 驗證)、AV1/Opus、Dolby/環繞音效保留、AI 降噪(RNNoise)、DSD64–1024(DSF)與 DoP WAV、
  高解析度 PCM、AI 影片超解析度(GPU/CPU)、實驗性音訊頻寬擴展、音樂 CD 擷取(已以實際光碟驗證)、MKV 多音軌/多字幕。
- 2026-09-23: 修正了無法在第 8 節操作區段剪輯的問題;建立 DSD 時不再同時產生 PCM;互斥的 YES/NO「依大小剪輯/依時間剪輯」(預設依大小);
  後處理核取方塊「填滿光碟」「AI 靜音剪輯」;播放規格上限(CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / 僅限 PC);文件多語化。

## 下一步

1. 對 2026-09-23 新功能進行實際檔案 E2E: 依大小/時間剪輯的位置、各播放規格下 `-ar`・位元深度・位元率上限、填滿光碟的位元率計算。
2. 音樂 CD(CD-DA)燒錄(IMAPI2 TrackAtOnce;目前僅支援資料光碟)。
3. 將 `rs-*` 工具移出安裝程式,改為隨選從姊妹儲存庫的發布資源下載。
4. 將「影片 + DSD 音訊」作為一組匯出(影片檔 + `.dsf` + 供 `open-bar` 使用的 `.obar.json`),並把 ΔΣ 調變器拆分到 `open-mqa-dsd`。
5. 擷取無保護的 BD/DVD(不實作繞過著作權保護)。

## 相關儲存庫

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — 本應用程式
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) / [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — FFmpeg / xorriso 的 Rust 版
- [aon-co-jp/open-cpu](https://github.com/aon-co-jp/open-cpu) — CPU 指令集偵測
- [aon-co-jp/open-cuda](https://github.com/aon-co-jp/open-cuda) — GPU 運算抽象層
- [aon-co-jp/open-mqa](https://github.com/aon-co-jp/open-mqa) / [aon-co-jp/open-mqa-dsd](https://github.com/aon-co-jp/open-mqa-dsd) / [aon-co-jp/open-bar](https://github.com/aon-co-jp/open-bar) — 高解析度音訊流程、DSD 工具、播放器
