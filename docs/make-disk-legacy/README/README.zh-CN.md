# make-disk

**语言**: [日本語](../README.md) | [English](README.en.md) | 简体中文 | [繁體中文(台灣)](README.zh-TW.md) | [한국어](README.ko.md) | [Deutsch](README.de.md) | [Français](README.fr.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | [فارسی](README.iran%28Perusha%29.md) | [العربية](README.ar.md)

基于跨平台通用代码(Rust + Tauri)的 CD/DVD/Blu-ray 刻录与音频/视频格式转换 GUI 应用。
应用本体为单一代码库,仅安装程序按操作系统区分。

## 功能

- **输入/输出**: 选择多个源文件(音频/视频/PDF)和输出文件夹。可通过复选框同时选择转换、ISO 镜像输出和光盘刻录
  (多种格式、多个文件、多种光盘类型以异步并行方式执行)。
- **音频**: MP3 / WAV / FLAC / AAC / OGG / **Opus** / **AC-3 / E-AC-3(5.1–7.1)** /
  **DSD64·128·256·512·1024(DSF)**。由于 ffmpeg 无法写出 DSD,PCM→1bit 的 ΔΣ 调制(5 阶,按声道并行)和 DSF 写出均为自研实现
  (已通过真实 ffmpeg 的 DSF 解码器进行往返验证)。
- **视频**: MP4 / MKV / AVI / MOV / WebM / **AV1**(优先 libsvtav1,否则 libaom) /
  **HEVC 10bit(HDR10)** / **无损直接复制**(完整保留 Dolby Vision、Atmos、5.1/7.1 环绕声)。
  可指定分辨率(720×480–8K、PAL、自定义、AI 优化)和帧率(24/30/60/120、自定义)。
  按光盘转换方向缩小选项(BD→DVD: 标准 DVD 分辨率或全高清;DVD→BD: 全高清或 4K)。
- **高分辨率 PCM(面向 R-2R 等多比特 DAC)**: 352.8kHz/384kHz(24bit・32bit)、705.6kHz/768kHz(32bit)的 WAV。
  使用可用的最高品质重采样器(soxr,否则为 swresample 高精度设置)+ TPDF 抖动。实测 SNR(与精确基准正弦波比较):
  352.8kHz/24bit = 141.2dB,384kHz/32bit = 150.2dB。
  创建 DSD 时不会同时生成 PCM(不支持 DSD 的硬件上,播放端会自动转换为 PCM,PCM 版本只会浪费容量)。
  DSD 实测往返 SNR: DSD64 = 99.6dB,DSD128 = 132.4dB。
- **AI 超分辨率(视频)**: 以按需插件形式使用 Real-ESRGAN(MIT)(如 DVD→4K)。支持 Vulkan 的 GPU 可用时使用 GPU(NCNN-Vulkan),
  否则自动切换为**自研的 Rust CPU 版**(自动使用 AVX2+FMA,与官方 GPU 实现的输出 PSNR 为 42.0dB)。
- **AI 高频生成(频带扩展,实验性)**: 用训练好的模型(LavaSR,Apache-2.0,由纯 Rust 的 tract 推理,首次使用时下载约 56MB)为缺失高频的音频补充高频。
  **完全不改变输入原有频带**,生成部分以输入包络的外推为上限(原始模型输出会使音乐变差,因此采用此设计)。
  实测(将音乐限带至 8kHz/12kHz): 有正确答案频带的 LSD 3.4→1.5、2.7→1.6,低频不变。这是合成而非复原,对未限带的音源不做任何处理。
- **AI 降噪**: 内置真正的已训练神经网络(RNNoise),已用真实模型验证。主要以人声训练,对音乐效果有限。
- **码率/容量**: 固定码率、根据光盘容量自动计算、"最高音质・最高画质"模式(未选格式时自动使用无损 WAV + ISO)。
  自动码率以源文件码率为上限。仅音频输出正确使用 `-b:a`。
- **源数据剪切**: 以 YES/NO 选择"按大小剪切?"和"按时间剪切?"(互斥,必须且只能一项为 YES,默认按大小)。
  后处理可勾选"填满光盘"(CD / DVD 1–2 层 / Blu-ray 1–4 层)和"AI 判断(静音检测)自动剪切"。
- **播放规格上限**: 显示 CD / DVD-Video / DVD-Audio / Blu-ray / Ultra HD Blu-ray / 仅 PC 的最大 kHz、位深和码率,
  并按所选规格转换(源已低于上限则保持不变,不进行升采样)。
- **编辑**: 多区间剪切(在第 8 节选择文件,边预览视频/音频边指定)、多文件合并、等间隔/按大小分割(剩余部分自动适配以填满光盘)。
- **PDF**: 跨页图像化(右装订/左装订,最高 4K),批量转换装订方向并保存(反转页序)。
- **刻录(Windows)**: 自动检测光驱,用 IMAPI2 创建 ISO(保留日文文件名)并刻录。
  已在实机(BD-RE 光驱 + CD-R)上确认: 3.5 小时视频 → 填满 CD 容量的音频 → ISO → 刻录成功。
- **其他**: 启动时自动检查更新(日英对话框),带版本管理的 rs-ffmpeg / rs-xorriso 插件(相同版本不覆盖)。

## 不支持的功能与限制(如实说明)

- **不实现绕过版权保护(CSS/AACS 等)**(可能违法)。仅针对无保护的光盘。
- **无法新生成 Dolby Vision / Atmos / Dolby Cinema / IMAX / 4DX**(授权格式)。仅支持保留(直接复制)和兼容的下位格式。
- **AI 视频超分辨率(Real-ESRGAN)非常慢,仅适用于短片段**(720×480 单帧: 高速模型在 32 线程 AVX2 CPU 上约 1.2 秒,GT 730 GPU 上约 4.6 秒;
  高品质模型在 GT 730 上约 110 秒)。CPU 版仅支持高速模型。**音频 AI 高频生成为实验性功能**(属于合成,不保证听感品质)。
  "AI 优化"的分辨率/帧率设置是简单的启发式处理,与 AI 超分辨率无关。
- DSD 文件非常大(立体声每分钟: DSD64 ≈ 42MB … DSD1024 ≈ 678MB),为 DSF 文件,并非 SACD 规格光盘。
- DVD 上的全高清不在 DVD-Video 规格(最大 720×480/576)内;家用 DVD 播放机不会自动降低分辨率播放,因此可能无法播放。
- Linux/macOS 的刻录依赖原版 xorriso(未内置)。以数据光盘形式刻录(不支持音乐 CD / CD-DA 刻录)。
- 由于没有测试设备,暂不支持 iOS。

## 运行时外部依赖

Windows/Linux 安装程序已内置 ffmpeg/ffprobe 和 rs-ffmpeg/rs-xorriso(参见 `src-tauri/src/engine/sidecar.rs`・`plugins.rs`)。
若未内置则回退到 PATH 上的工具。macOS 的 ffmpeg 和原版 xorriso 未内置。

## 开发

```bash
npm install
npm run tauri dev
cd src-tauri && cargo test --lib -- --test-threads=1
```

## 下载 / 安装程序

Windows(.msi/.exe)、macOS(.dmg,Intel/Apple Silicon)、Linux(.deb/.rpm/.AppImage)、Android(universal APK)发布于
[GitHub Releases](https://github.com/aon-co-jp/make-disk/releases/latest)。推送 `v*` 标签后,
`.github/workflows/release.yml` 会自动构建全部平台。介绍页: <https://easy-web.tokyo/make-disk/>

## 许可证

MIT(内置的 RNNoise 模型经作者声明不受著作权保护,详见 `CLAUDE.md`)。
