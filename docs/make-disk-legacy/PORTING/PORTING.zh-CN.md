# 会话交接备忘(make-disk)— 摘要

**语言**: [日本語(全文・正本)](../PORTING.md) | [English](PORTING.en.md) | 简体中文 | [繁體中文(台灣)](PORTING.zh-TW.md) | [한국어](PORTING.ko.md) | [Deutsch](PORTING.de.md) | [Français](PORTING.fr.md) | [Русский](PORTING.ru.md) | [Українська](PORTING.uk.md) | [فارسی](PORTING.iran%28Perusha%29.md) | [العربية](PORTING.ar.md)

> 本文概述当前进度与下一步。历次恢复备忘的全文以日文版 [`PORTING.md`](../PORTING.md) 为准;技术决策见 [`CLAUDE.md`](../CLAUDE.md)。

## 当前进度(2026-09-23)

- 已完成: IMAPI2 刻录(已用真实 CD-R 验证)、AV1/Opus、Dolby/环绕声保留、AI 降噪(RNNoise)、DSD64–1024(DSF)与 DoP WAV、
  高分辨率 PCM、AI 视频超分辨率(GPU/CPU)、实验性音频频带扩展、音乐 CD 抓取(已用真实光盘验证)、MKV 多音轨/多字幕。
- 2026-09-23: 修复了无法在第 8 节操作区间剪切的问题;创建 DSD 时不再同时生成 PCM;互斥的 YES/NO"按大小剪切/按时间剪切"(默认按大小);
  后处理复选框"填满光盘""AI 静音剪切";播放规格上限(CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / 仅 PC);文档多语言化。

## 下一步

1. 对 2026-09-23 新功能进行真实文件 E2E: 按大小/时间剪切的位置、各播放规格下 `-ar`・位深・码率上限、填满光盘的码率计算。
2. 音乐 CD(CD-DA)刻录(IMAPI2 TrackAtOnce;目前仅支持数据光盘)。
3. 将 `rs-*` 工具移出安装程序,改为按需从姊妹仓库的发布资源下载。
4. 将"视频 + DSD 音频"作为一套导出(视频文件 + `.dsf` + 供 `open-bar` 使用的 `.obar.json`),并把 ΔΣ 调制器拆分到 `open-mqa-dsd`。
5. 抓取无保护的 BD/DVD(不实现绕过版权保护)。

## 相关仓库

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — 本应用
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) / [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — FFmpeg / xorriso 的 Rust 版
- [aon-co-jp/open-cpu](https://github.com/aon-co-jp/open-cpu) — CPU 指令集检测
- [aon-co-jp/open-cuda](https://github.com/aon-co-jp/open-cuda) — GPU 计算抽象层
- [aon-co-jp/open-mqa](https://github.com/aon-co-jp/open-mqa) / [aon-co-jp/open-mqa-dsd](https://github.com/aon-co-jp/open-mqa-dsd) / [aon-co-jp/open-bar](https://github.com/aon-co-jp/open-bar) — 高分辨率音频流程、DSD 工具、播放器
