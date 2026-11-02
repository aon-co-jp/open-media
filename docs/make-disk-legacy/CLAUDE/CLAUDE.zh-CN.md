# 开发方针与开发环境规则(make-disk)— 摘要

**语言**: [日本語(全文・正本)](../CLAUDE.md) | [English](CLAUDE.en.md) | 简体中文 | [繁體中文(台灣)](CLAUDE.zh-TW.md) | [한국어](CLAUDE.ko.md) | [Deutsch](CLAUDE.de.md) | [Français](CLAUDE.fr.md) | [Русский](CLAUDE.ru.md) | [Українська](CLAUDE.uk.md) | [فارسی](CLAUDE.iran%28Perusha%29.md) | [العربية](CLAUDE.ar.md)

> 本文为摘要。包括完整开发历史(HANDOFF 记录)在内的全文以日文版 [`CLAUDE.md`](../CLAUDE.md) 为准。
> 所有仓库通用的开发规则(自动继续、彻底验证等)以 [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z) 的 `CLAUDE.md` 为准。

## 仓库的作用

以 Windows/macOS/Linux(及 Android)通用代码编写的 GUI 应用(Rust + Tauri),用于 CD/DVD/Blu-ray 刻录、音频/视频格式转换和 ISO 输出。
平台差异仅限于安装程序(bundle),应用本体(`src-tauri/src`・`src`)为单一代码库。

## 架构

- 前端: `src/`(原生 JS。由于没有打包器时无法解析裸导入,因此通过 `window.__TAURI__` 经 Tauri IPC 调用 Rust 命令)。
- 后端: `src-tauri/src/engine/`
  - `probe.rs` — 用 ffprobe 获取时长、编解码器、采样率、Dolby 特征(回退到 rs-ffmpeg)
  - `convert.rs` — ffmpeg 转换、码率控制、裁剪、多区间剪切(默认直接复制流,帧精确剪切时使用 GPU 编码器)
  - `capacity.rs` — 根据光盘容量计算最大码率,以 4 级提示画质下降程度
  - `dsd.rs` — 自研 PCM→1bit ΔΣ 调制器与 DSF 写出、DoP WAV
  - `cdda.rs` — 音乐 CD 抓取(Windows,简易安全读取)
  - `iso.rs` / `burn.rs` / `windows_imapi.rs` — ISO 创建与刻录(Windows 用 IMAPI2,其他用 xorriso)
  - `ai_upscale.rs` / `cpu_sr.rs` / `audio_sr.rs` — Real-ESRGAN(GPU/CPU)、音频频带扩展
  - `mkv_tracks.rs` — MKV 多音轨・多字幕的保留与追加
  - `plugins.rs` / `sidecar.rs` — 内置/下载工具及版本管理
  - `cpu.rs` — 通过 `open-cpu` 检测 CPU 指令集(速度提示与 x264 `-preset` 选择)

## 既定方针

- **不绕过版权保护**(CSS/AACS 等),仅支持无保护光盘。
- **不新生成 Dolby Vision / Atmos / IMAX / 4DX**(授权格式),仅支持直接复制保留和兼容的下位格式。
- **不支持 MQA**(专利・商业秘密)。高分辨率输出遵循 `aon-co-jp/open-mqa` 的开放格式路线。
- **创建 DSD 时不同时创建 PCM**(不支持 DSD 的硬件上播放端会自动转换为 PCM)。DoP WAV 属于 DSD 数据,不受此限。
- DSD 调制保持**按声道顺序处理(逐位一致)**;分段并行实测会严重降低 SNR,已放弃。
- 如实说明"AI"功能: 静音自动剪切和"AI 优化"的分辨率/帧率属于启发式处理;音频频带扩展属于合成。
- DVD 上的全高清不在 DVD-Video 规格内,家用播放机不会自动降低分辨率。界面保留此提示(用户决定不采用 DVD-Video + 全高清文件同时收录)。
- 文档语言: 以日文为正本;`README/`、`CLAUDE/`、`PORTING/` 存放英文、简体中文、繁体中文(台湾)、韩文、德文、法文、俄文、乌克兰文、波斯文(伊朗,文件后缀 `.iran(Perusha)`)、阿拉伯文(CLAUDE/PORTING 为摘要)。

## 验证规则

- CI 成功不等于能正常运行。报告完成前必须实际验证: `npm run lint`(eslint `no-undef`)、在浏览器中以桩化 Tauri API 操作界面、Rust 测试(`cargo test --lib -- --test-threads=1`),尽可能进行真实文件/真实设备 E2E。
- 如实写明尚未验证的内容。

## 发布

推送 `v*` 标签后,`.github/workflows/release.yml` 会构建 Windows/macOS/Linux/Android 安装程序并发布到 GitHub Releases。

## 最新状态(2026-09-23)

修复了无法在第 8 节操作区间剪切的问题;创建 DSD 时不再同时生成 PCM;将剪切重新设计为互斥的 YES/NO"按大小剪切/按时间剪切"(默认按大小),并将"填满光盘""AI 静音剪切"作为后处理复选框;
新增播放规格上限(CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / 仅 PC)的最大 kHz、位深和码率。这些功能的真实文件 E2E 尚未进行。
