# 개발 방침 및 개발 환경 규칙(make-disk) — 요약

**언어**: [日本語(전문·정본)](../CLAUDE.md) | [English](CLAUDE.en.md) | [简体中文](CLAUDE.zh-CN.md) | [繁體中文(台灣)](CLAUDE.zh-TW.md) | 한국어 | [Deutsch](CLAUDE.de.md) | [Français](CLAUDE.fr.md) | [Русский](CLAUDE.ru.md) | [Українська](CLAUDE.uk.md) | [فارسی](CLAUDE.iran%28Perusha%29.md) | [العربية](CLAUDE.ar.md)

> 이 문서는 요약입니다. 전체 개발 이력(HANDOFF 기록)을 포함한 전문은 일본어판 [`CLAUDE.md`](../CLAUDE.md)가 정본입니다.
> 모든 저장소 공통 개발 규칙(자동 계속, 철저한 검증 등)은 [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z)의 `CLAUDE.md`를 따릅니다.

## 저장소의 역할

Windows/macOS/Linux(및 Android) 공통 코드로 만든 GUI 앱(Rust + Tauri)으로, CD/DVD/Blu-ray 굽기·오디오/비디오 포맷 변환·ISO 출력을 수행합니다.
플랫폼별 차이는 설치 프로그램(bundle)에만 두고, 앱 본체(`src-tauri/src`·`src`)는 단일 코드베이스입니다.

## 아키텍처

- 프런트엔드: `src/`(바닐라 JS. 번들러 없이는 bare import를 해석할 수 없으므로 `window.__TAURI__`를 통해 Tauri IPC로 Rust 명령을 호출).
- 백엔드: `src-tauri/src/engine/`
  - `probe.rs` — ffprobe로 길이·코덱·샘플레이트·Dolby 특성 획득(rs-ffmpeg 폴백)
  - `convert.rs` — ffmpeg 변환, 비트레이트 제어, 트리밍, 여러 구간 자르기(기본은 스트림 복사, 프레임 정확 자르기는 GPU 인코더 사용)
  - `capacity.rs` — 디스크 용량으로 최대 비트레이트 계산, 화질 저하 정도를 4단계로 경고
  - `dsd.rs` — 자체 PCM→1bit ΔΣ 변조기와 DSF 쓰기, DoP WAV
  - `cdda.rs` — 음악 CD 추출(Windows, 간이 보안 읽기)
  - `iso.rs` / `burn.rs` / `windows_imapi.rs` — ISO 생성과 굽기(Windows는 IMAPI2, 그 외는 xorriso)
  - `ai_upscale.rs` / `cpu_sr.rs` / `audio_sr.rs` — Real-ESRGAN(GPU/CPU), 오디오 대역 확장
  - `mkv_tracks.rs` — MKV의 여러 음성·자막 트랙 유지 및 추가
  - `plugins.rs` / `sidecar.rs` — 포함/다운로드 도구와 버전 관리
  - `cpu.rs` — `open-cpu`로 CPU 명령어 세트 감지(속도 안내와 x264 `-preset` 선택)

## 확정된 방침

- **저작권 보호(CSS/AACS 등) 우회는 하지 않습니다.** 보호가 없는 디스크만 대상입니다.
- **Dolby Vision / Atmos / IMAX / 4DX를 새로 생성하지 않습니다**(라이선스 포맷). 스트림 복사로 유지하거나 호환 하위 포맷만 지원합니다.
- **MQA는 지원하지 않습니다**(특허·영업비밀). 고해상도 출력은 `aon-co-jp/open-mqa`의 오픈 포맷 노선을 따릅니다.
- **DSD를 만들 때는 PCM을 함께 만들지 않습니다**(DSD 미지원 하드웨어에서는 재생 측이 자동으로 PCM 변환). DoP WAV는 DSD 데이터이므로 예외입니다.
- DSD 변조는 **채널별 순차 처리(비트 단위 일치)**를 유지합니다. 구간 병렬화는 실측에서 SNR을 크게 떨어뜨려 채택하지 않았습니다.
- "AI" 기능은 솔직하게 공개합니다: 무음 자동 자르기와 "AI 최적화" 해상도/FPS는 휴리스틱이며, 오디오 대역 확장은 합성입니다.
- DVD의 풀HD는 DVD-Video 규격 밖이며 가정용 플레이어는 자동으로 해상도를 낮추지 않습니다. UI에 이 주의 문구를 유지합니다(DVD-Video + 풀HD 파일 동시 수록은 사용자가 채택하지 않음).
- 문서 언어: 일본어가 정본이며, `README/`·`CLAUDE/`·`PORTING/`에 영어·중국어 간체·중국어 번체(대만)·한국어·독일어·프랑스어·러시아어·우크라이나어·페르시아어(이란, 파일 접미사 `.iran(Perusha)`)·아랍어를 둡니다(CLAUDE/PORTING은 요약).

## 검증 규칙

- CI 성공은 동작 확인이 아닙니다. 완료 보고 전에 실제로 검증합니다: `npm run lint`(eslint `no-undef`), Tauri API를 스텁한 UI의 브라우저 조작, Rust 테스트(`cargo test --lib -- --test-threads=1`), 가능하면 실제 파일/실기 E2E.
- 검증하지 못한 부분은 솔직하게 명시합니다.

## 릴리스

`v*` 태그를 push하면 `.github/workflows/release.yml`이 Windows/macOS/Linux/Android 설치 프로그램을 빌드해 GitHub Releases에 공개합니다.

## 최신 상태(2026-09-23)

8번 항목에서 구간 자르기를 조작할 수 없던 버그를 수정했습니다. DSD 생성 시 PCM을 함께 만들지 않도록 했습니다. 자르기를 배타적인 YES/NO "크기로 자르기/시간으로 자르기"(기본값은 크기)로 재설계하고, "디스크 가득 채우기"와 "AI 무음 자르기"를 후처리 체크박스로 만들었습니다.
재생 규격 상한(CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / PC 전용)의 최대 kHz·비트 심도·비트레이트를 추가했습니다. 이 기능들의 실제 파일 E2E는 아직 하지 않았습니다.
