# make-disk

**語言**: [日本語](../README.md) | [English](README.en.md) | [简体中文](README.zh-CN.md) | 繁體中文(台灣) | [한국어](README.ko.md) | [Deutsch](README.de.md) | [Français](README.fr.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | [فارسی](README.iran%28Perusha%29.md) | [العربية](README.ar.md)

以跨平台共用程式碼(Rust + Tauri)打造的 CD/DVD/Blu-ray 燒錄與音訊/影片格式轉換 GUI 應用程式。
應用程式本體為單一程式碼庫,僅安裝程式依作業系統區分。

## 功能

- **輸入/輸出**: 選擇多個來源檔案(音訊/影片/PDF)與輸出資料夾。可透過核取方塊同時選擇轉換、ISO 映像檔輸出與光碟燒錄
  (多種格式、多個檔案、多種光碟類型皆以非同步平行方式執行)。
- **音訊**: MP3 / WAV / FLAC / AAC / OGG / **Opus** / **AC-3 / E-AC-3(5.1–7.1)** /
  **DSD64·128·256·512·1024(DSF)**。由於 ffmpeg 無法寫出 DSD,PCM→1bit 的 ΔΣ 調變(5 階,依聲道平行處理)與 DSF 寫出皆為自行實作
  (已透過實際 ffmpeg 的 DSF 解碼器進行往返驗證)。
- **影片**: MP4 / MKV / AVI / MOV / WebM / **AV1**(優先 libsvtav1,否則 libaom) /
  **HEVC 10bit(HDR10)** / **無損直接複製**(完整保留 Dolby Vision、Atmos、5.1/7.1 環繞音效)。
  可指定解析度(720×480–8K、PAL、自訂、AI 最佳化)與影格率(24/30/60/120、自訂)。
  依光碟轉換方向縮小選項(BD→DVD: 標準 DVD 解析度或 Full HD;DVD→BD: Full HD 或 4K)。
- **高解析度 PCM(適用 R-2R 等多位元 DAC)**: 352.8kHz/384kHz(24bit・32bit)、705.6kHz/768kHz(32bit)的 WAV。
  使用可用的最高品質重新取樣器(soxr,否則為 swresample 高精度設定)+ TPDF 抖動。實測 SNR(與精確基準正弦波比較):
  352.8kHz/24bit = 141.2dB,384kHz/32bit = 150.2dB。
  建立 DSD 時不會同時產生 PCM(不支援 DSD 的硬體上,播放端會自動轉換為 PCM,PCM 版本只會浪費容量)。
  DSD 實測往返 SNR: DSD64 = 99.6dB,DSD128 = 132.4dB。
- **AI 超解析度(影片)**: 以隨選外掛形式使用 Real-ESRGAN(MIT)(如 DVD→4K)。支援 Vulkan 的 GPU 可用時使用 GPU(NCNN-Vulkan),
  否則自動切換為**自行開發的 Rust CPU 版**(自動使用 AVX2+FMA,與官方 GPU 實作的輸出 PSNR 為 42.0dB)。
- **AI 高頻生成(頻寬擴展,實驗性)**: 以訓練好的模型(LavaSR,Apache-2.0,由純 Rust 的 tract 推論,首次使用時下載約 56MB)為缺少高頻的音訊補上高頻。
  **完全不改變輸入原有頻帶**,生成部分以輸入包絡的外推為上限(原始模型輸出會使音樂變差,因此採用此設計)。
  實測(將音樂限頻至 8kHz/12kHz): 有正確答案頻帶的 LSD 3.4→1.5、2.7→1.6,低頻不變。這是合成而非還原,對未限頻的音源不做任何處理。
- **AI 降噪**: 內建真正的已訓練神經網路(RNNoise),已以實際模型驗證。主要以人聲訓練,對音樂效果有限。
- **位元率/容量**: 固定位元率、依光碟容量自動計算、「最高音質・最高畫質」模式(未選格式時自動使用無損 WAV + ISO)。
  自動位元率以來源檔案的位元率為上限。僅音訊輸出會正確使用 `-b:a`。
- **來源資料剪輯**: 以 YES/NO 選擇「依大小剪輯?」與「依時間剪輯?」(互斥,必定且只有一項為 YES,預設依大小)。
  後處理可勾選「填滿光碟」(CD / DVD 1–2 層 / Blu-ray 1–4 層)與「AI 判斷(靜音偵測)自動剪輯」。
- **播放規格上限**: 顯示 CD / DVD-Video / DVD-Audio / Blu-ray / Ultra HD Blu-ray / 僅限 PC 的最大 kHz、位元深度與位元率,
  並依所選規格轉換(來源已低於上限則維持原狀,不進行升頻取樣)。
- **編輯**: 多區段剪輯(在第 8 節選擇檔案,邊預覽影片/音訊邊指定)、多檔案合併、等間隔/依大小分割(剩餘部分自動調整以填滿光碟)。
- **PDF**: 跨頁圖片化(右翻/左翻,最高 4K),批次轉換裝訂方向並儲存(反轉頁序)。
- **燒錄(Windows)**: 自動偵測光碟機,以 IMAPI2 建立 ISO(保留日文檔名)並燒錄。
  已在實機(BD-RE 光碟機 + CD-R)上確認: 3.5 小時影片 → 填滿 CD 容量的音訊 → ISO → 燒錄成功。
- **其他**: 啟動時自動檢查更新(日英對話框),具版本管理的 rs-ffmpeg / rs-xorriso 外掛(相同版本不覆寫)。

## 不支援的功能與限制(誠實揭露)

- **不實作繞過著作權保護(CSS/AACS 等)**(可能違法)。僅適用於無保護的光碟。
- **無法新產生 Dolby Vision / Atmos / Dolby Cinema / IMAX / 4DX**(授權格式)。僅支援保留(直接複製)與相容的下位格式。
- **AI 影片超解析度(Real-ESRGAN)非常慢,僅適用於短片段**(720×480 單一影格: 高速模型在 32 執行緒 AVX2 CPU 上約 1.2 秒,GT 730 GPU 上約 4.6 秒;
  高品質模型在 GT 730 上約 110 秒)。CPU 版僅支援高速模型。**音訊 AI 高頻生成為實驗性功能**(屬於合成,不保證聽感品質)。
  「AI 最佳化」的解析度/影格率設定為簡易的啟發式處理,與 AI 超解析度無關。
- DSD 檔案非常大(立體聲每分鐘: DSD64 ≈ 42MB … DSD1024 ≈ 678MB),為 DSF 檔案,並非 SACD 規格光碟。
- DVD 上的 Full HD 不在 DVD-Video 規格(最大 720×480/576)之內;家用 DVD 播放器不會自動降低解析度播放,因此可能無法播放。
- Linux/macOS 的燒錄需仰賴原版 xorriso(未內建)。以資料光碟形式燒錄(不支援音樂 CD / CD-DA 燒錄)。
- 由於沒有測試裝置,暫不支援 iOS。

## 執行時的外部相依

Windows/Linux 安裝程式已內建 ffmpeg/ffprobe 與 rs-ffmpeg/rs-xorriso(參見 `src-tauri/src/engine/sidecar.rs`・`plugins.rs`)。
若未內建則改用 PATH 上的工具。macOS 的 ffmpeg 與原版 xorriso 未內建。

## 開發

```bash
npm install
npm run tauri dev
cd src-tauri && cargo test --lib -- --test-threads=1
```

## 下載 / 安裝程式

Windows(.msi/.exe)、macOS(.dmg,Intel/Apple Silicon)、Linux(.deb/.rpm/.AppImage)、Android(universal APK)發布於
[GitHub Releases](https://github.com/aon-co-jp/make-disk/releases/latest)。推送 `v*` 標籤後,
`.github/workflows/release.yml` 會自動建置所有平台。介紹頁面: <https://easy-web.tokyo/make-disk/>

## 授權

MIT(內建的 RNNoise 模型經作者聲明不受著作權保護,詳見 `CLAUDE.md`)。
