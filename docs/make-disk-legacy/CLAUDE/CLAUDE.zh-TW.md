# 開發方針與開發環境規則(make-disk)— 摘要

**語言**: [日本語(全文・正本)](../CLAUDE.md) | [English](CLAUDE.en.md) | [简体中文](CLAUDE.zh-CN.md) | 繁體中文(台灣) | [한국어](CLAUDE.ko.md) | [Deutsch](CLAUDE.de.md) | [Français](CLAUDE.fr.md) | [Русский](CLAUDE.ru.md) | [Українська](CLAUDE.uk.md) | [فارسی](CLAUDE.iran%28Perusha%29.md) | [العربية](CLAUDE.ar.md)

> 本文為摘要。包含完整開發歷史(HANDOFF 紀錄)在內的全文以日文版 [`CLAUDE.md`](../CLAUDE.md) 為準。
> 所有儲存庫共通的開發規則(自動繼續、徹底驗證等)以 [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z) 的 `CLAUDE.md` 為準。

## 儲存庫的角色

以 Windows/macOS/Linux(及 Android)共用程式碼撰寫的 GUI 應用程式(Rust + Tauri),用於 CD/DVD/Blu-ray 燒錄、音訊/影片格式轉換與 ISO 輸出。
平台差異僅限於安裝程式(bundle),應用程式本體(`src-tauri/src`・`src`)為單一程式碼庫。

## 架構

- 前端: `src/`(原生 JS。由於沒有打包工具時無法解析裸匯入,因此透過 `window.__TAURI__` 經 Tauri IPC 呼叫 Rust 指令)。
- 後端: `src-tauri/src/engine/`
  - `probe.rs` — 以 ffprobe 取得長度、編解碼器、取樣率、Dolby 特徵(改用 rs-ffmpeg 作為備援)
  - `convert.rs` — ffmpeg 轉換、位元率控制、裁切、多區段剪輯(預設直接複製串流,影格精確剪輯時使用 GPU 編碼器)
  - `capacity.rs` — 依光碟容量計算最大位元率,並以 4 個等級提示畫質下降程度
  - `dsd.rs` — 自行實作的 PCM→1bit ΔΣ 調變器與 DSF 寫出、DoP WAV
  - `cdda.rs` — 音樂 CD 擷取(Windows,簡易安全讀取)
  - `iso.rs` / `burn.rs` / `windows_imapi.rs` — ISO 建立與燒錄(Windows 用 IMAPI2,其他用 xorriso)
  - `ai_upscale.rs` / `cpu_sr.rs` / `audio_sr.rs` — Real-ESRGAN(GPU/CPU)、音訊頻寬擴展
  - `mkv_tracks.rs` — MKV 多音軌・多字幕的保留與追加
  - `plugins.rs` / `sidecar.rs` — 內建/下載工具及版本管理
  - `cpu.rs` — 透過 `open-cpu` 偵測 CPU 指令集(速度提示與 x264 `-preset` 選擇)

## 既定方針

- **不繞過著作權保護**(CSS/AACS 等),僅支援無保護的光碟。
- **不新產生 Dolby Vision / Atmos / IMAX / 4DX**(授權格式),僅支援直接複製保留與相容的下位格式。
- **不支援 MQA**(專利・營業秘密)。高解析度輸出遵循 `aon-co-jp/open-mqa` 的開放格式路線。
- **建立 DSD 時不同時建立 PCM**(不支援 DSD 的硬體上播放端會自動轉換為 PCM)。DoP WAV 屬於 DSD 資料,不受此限。
- DSD 調變維持**依聲道循序處理(逐位元一致)**;分段平行處理經實測會嚴重降低 SNR,已不採用。
- 誠實說明「AI」功能: 靜音自動剪輯與「AI 最佳化」的解析度/影格率屬於啟發式處理;音訊頻寬擴展屬於合成。
- DVD 上的 Full HD 不在 DVD-Video 規格內,家用播放器不會自動降低解析度。介面保留此提示(使用者決定不採用 DVD-Video + Full HD 檔案同時收錄)。
- 文件語言: 以日文為正本;`README/`、`CLAUDE/`、`PORTING/` 存放英文、簡體中文、繁體中文(台灣)、韓文、德文、法文、俄文、烏克蘭文、波斯文(伊朗,檔名後綴 `.iran(Perusha)`)、阿拉伯文(CLAUDE/PORTING 為摘要)。

## 驗證規則

- CI 成功不等於能正常運作。回報完成前必須實際驗證: `npm run lint`(eslint `no-undef`)、在瀏覽器中以替身 Tauri API 操作介面、Rust 測試(`cargo test --lib -- --test-threads=1`),並盡可能進行實際檔案/實機 E2E。
- 誠實寫明尚未驗證的內容。

## 發布

推送 `v*` 標籤後,`.github/workflows/release.yml` 會建置 Windows/macOS/Linux/Android 安裝程式並發布到 GitHub Releases。

## 最新狀態(2026-09-23)

修正了無法在第 8 節操作區段剪輯的問題;建立 DSD 時不再同時產生 PCM;將剪輯重新設計為互斥的 YES/NO「依大小剪輯/依時間剪輯」(預設依大小),並將「填滿光碟」「AI 靜音剪輯」作為後處理核取方塊;
新增播放規格上限(CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / 僅限 PC)的最大 kHz、位元深度與位元率。這些功能的實際檔案 E2E 尚未進行。
