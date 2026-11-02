# make-disk

**언어**: [日本語](../README.md) | [English](README.en.md) | [简体中文](README.zh-CN.md) | [繁體中文(台灣)](README.zh-TW.md) | 한국어 | [Deutsch](README.de.md) | [Français](README.fr.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | [فارسی](README.iran%28Perusha%29.md) | [العربية](README.ar.md)

플랫폼 공통 코드(Rust + Tauri)로 만든 CD/DVD/Blu-ray 굽기 및 오디오/비디오 포맷 변환 GUI 앱입니다.
앱 본체는 단일 코드베이스이며, 설치 프로그램만 OS별로 나뉩니다.

## 기능

- **입력/출력**: 여러 소스 파일(오디오/비디오/PDF)과 출력 폴더를 선택합니다. 변환·ISO 이미지 출력·디스크 굽기를 체크박스로 동시에 여러 개 선택할 수 있습니다
  (여러 포맷·여러 파일·여러 디스크 종류는 비동기로 병렬 실행).
- **오디오**: MP3 / WAV / FLAC / AAC / OGG / **Opus** / **AC-3 / E-AC-3(5.1–7.1)** /
  **DSD64·128·256·512·1024(DSF)**. ffmpeg는 DSD를 쓸 수 없으므로 PCM→1bit ΔΣ 변조(5차, 채널별 병렬)와 DSF 쓰기를 자체 구현했습니다
  (실제 ffmpeg의 DSF 디코더로 왕복 검증 완료).
- **비디오**: MP4 / MKV / AVI / MOV / WebM / **AV1**(libsvtav1 우선, 없으면 libaom) /
  **HEVC 10bit(HDR10)** / **무변환 복사**(Dolby Vision·Atmos·5.1/7.1 서라운드를 그대로 유지).
  해상도(720×480–8K·PAL·사용자 지정·AI 최적화)와 FPS(24/30/60/120·사용자 지정)를 지정할 수 있습니다.
  디스크 변환 방향에 따라 선택지를 좁힙니다(BD→DVD: 일반 DVD 해상도 또는 풀HD, DVD→BD: 풀HD 또는 4K).
- **고해상도 PCM(R-2R 등 멀티비트 DAC용)**: 352.8kHz/384kHz(24bit·32bit), 705.6kHz/768kHz(32bit) WAV.
  사용 가능한 최고 품질의 리샘플러(soxr, 없으면 swresample 고정밀 설정)+TPDF 디더. 실측 SNR(정확한 기준 사인파와 비교):
  352.8kHz/24bit = 141.2dB, 384kHz/32bit = 150.2dB.
  DSD를 만들 때는 PCM을 함께 만들지 않습니다(DSD를 지원하지 않는 하드웨어에서는 재생 측이 자동으로 PCM으로 변환하므로 PCM 버전은 용량 낭비입니다).
  DSD 실측 왕복 SNR: DSD64 = 99.6dB, DSD128 = 132.4dB.
- **AI 초해상도(영상)**: Real-ESRGAN(MIT)을 온디맨드 플러그인으로 사용합니다(DVD→4K 등). Vulkan 지원 GPU가 동작하면 GPU(NCNN-Vulkan),
  없으면 **자체 개발한 Rust CPU 버전**(AVX2+FMA 자동 사용, 공식 GPU 구현과의 출력 PSNR 42.0dB)으로 자동 전환합니다.
- **AI 고역 생성(대역 확장, 실험적)**: 대역이 잘린 오디오의 고역을 학습된 모델(LavaSR, Apache-2.0, 순수 Rust tract로 추론, 첫 사용 시 약 56MB 다운로드)로 생성해 더합니다.
  **입력의 기존 대역은 전혀 바꾸지 않으며**, 생성분은 입력 포락선의 외삽을 상한으로 제한합니다(원래 모델 출력은 음악에서 오히려 나빠졌기 때문에 이렇게 설계).
  실측(음악을 8kHz/12kHz로 대역 제한): 정답이 있는 대역의 LSD 3.4→1.5, 2.7→1.6, 저역은 변화 없음. 복원이 아닌 합성이며, 대역이 잘리지 않은 음원에는 아무것도 하지 않습니다.
- **AI 노이즈 제거**: 실제 학습된 신경망(RNNoise)을 내장했으며 실제 모델로 검증했습니다. 주로 사람 목소리로 학습되어 음악에서는 효과가 제한적입니다.
- **비트레이트/용량**: 고정, 디스크 용량으로 자동 계산, "최고 음질·최고 화질" 모드(포맷 미선택 시 자동으로 무손실 WAV+ISO).
  자동 비트레이트는 원본 파일의 비트레이트를 상한으로 합니다. 오디오 전용 출력에는 `-b:a`를 올바르게 적용합니다.
- **원본 데이터 자르기**: "크기로 자를까요?"와 "시간으로 자를까요?"를 YES/NO로 선택합니다(배타적이며 반드시 한쪽만 YES, 기본값은 크기).
  후처리로 "디스크를 가득 채우기"(CD / DVD 1–2층 / Blu-ray 1–4층)와 "AI 판단(무음 검출) 자동 자르기"를 체크로 선택합니다.
- **재생 규격 상한**: CD / DVD-Video / DVD-Audio / Blu-ray / Ultra HD Blu-ray / PC 전용의 최대 kHz·비트 심도·비트레이트를 표시하고,
  선택한 규격에 맞게 변환합니다(원본이 상한 이하이면 그대로이며 업샘플링하지 않음).
- **편집**: 여러 구간 자르기(8번 항목에서 파일을 고르고 영상/음성을 미리 보며 지정), 여러 파일 합치기, 등간격/크기 지정 분할(나머지는 디스크를 가득 채우도록 자동 조정).
- **PDF**: 펼침면 이미지화(오른쪽 제본/왼쪽 제본, 최대 4K), 제본 방향 일괄 변환 저장(페이지 순서 반전).
- **굽기(Windows)**: 광학 드라이브를 자동 감지하고 IMAPI2로 ISO 생성(일본어 파일명 유지)·굽기를 수행합니다.
  실기(BD-RE 드라이브+CD-R)에서 3.5시간 영상 → CD 용량을 가득 채운 음성 → ISO → 굽기 성공을 확인했습니다.
- **기타**: 시작 시 자동 업데이트 확인(일·영 대화상자), 버전 관리가 되는 rs-ffmpeg / rs-xorriso 플러그인(같은 버전은 덮어쓰지 않음).

## 지원하지 않는 기능·제한(솔직한 공개)

- **저작권 보호(CSS/AACS 등) 우회는 구현하지 않습니다**(불법이 될 수 있음). 보호가 없는 디스크만 대상입니다.
- **Dolby Vision / Atmos / Dolby Cinema / IMAX / 4DX를 새로 생성할 수 없습니다**(라이선스 포맷). 유지(무변환 복사)와 호환 하위 포맷만 지원합니다.
- **AI 영상 초해상도(Real-ESRGAN)는 매우 느려 짧은 클립용입니다**(720×480 1프레임: 고속 모델은 32스레드 AVX2 CPU에서 약 1.2초, GT 730 GPU에서 약 4.6초,
  고품질 모델은 GT 730에서 약 110초). CPU 버전은 고속 모델만 지원합니다. **오디오 AI 고역 생성은 실험적입니다**(합성이며 청감 품질을 보장하지 않음).
  "AI 최적화" 해상도/FPS 설정은 간단한 휴리스틱이며 AI 초해상도와는 다릅니다.
- DSD 파일은 매우 큽니다(스테레오 1분당 DSD64 ≈ 42MB … DSD1024 ≈ 678MB). SACD 규격 디스크가 아닌 DSF 파일입니다.
- DVD의 풀HD는 DVD-Video 규격(최대 720×480/576) 밖입니다. 가정용 DVD 플레이어는 자동으로 해상도를 낮춰 재생하지 않으므로 재생되지 않을 수 있습니다.
- Linux/macOS의 굽기는 원본 xorriso가 필요합니다(미포함). 데이터 디스크로 굽습니다(음악 CD / CD-DA 굽기는 미지원).
- 테스트 기기가 없어 iOS는 미지원입니다.

## 실행 시 외부 의존성

Windows/Linux 설치 프로그램에는 ffmpeg/ffprobe와 rs-ffmpeg/rs-xorriso가 포함되어 있습니다(`src-tauri/src/engine/sidecar.rs`·`plugins.rs` 참조).
포함되어 있지 않으면 PATH의 도구를 사용합니다. macOS의 ffmpeg와 원본 xorriso는 포함되어 있지 않습니다.

## 개발

```bash
npm install
npm run tauri dev
cd src-tauri && cargo test --lib -- --test-threads=1
```

## 다운로드 / 설치 프로그램

Windows(.msi/.exe)·macOS(.dmg, Intel/Apple Silicon)·Linux(.deb/.rpm/.AppImage)·Android(universal APK)를
[GitHub Releases](https://github.com/aon-co-jp/make-disk/releases/latest)에서 공개합니다. `v*` 태그를 push하면
`.github/workflows/release.yml`이 모든 플랫폼을 자동 빌드합니다. 소개 페이지: <https://easy-web.tokyo/make-disk/>

## 라이선스

MIT(포함된 RNNoise 모델은 저작자가 저작권 대상이 아니라고 명시, 자세한 내용은 `CLAUDE.md`).
