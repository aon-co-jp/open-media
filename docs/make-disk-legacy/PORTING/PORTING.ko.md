# 세션 인수인계 메모(make-disk) — 요약

**언어**: [日本語(전문·정본)](../PORTING.md) | [English](PORTING.en.md) | [简体中文](PORTING.zh-CN.md) | [繁體中文(台灣)](PORTING.zh-TW.md) | 한국어 | [Deutsch](PORTING.de.md) | [Français](PORTING.fr.md) | [Русский](PORTING.ru.md) | [Українська](PORTING.uk.md) | [فارسی](PORTING.iran%28Perusha%29.md) | [العربية](PORTING.ar.md)

> 현재 진행 상황과 다음 할 일을 요약한 문서입니다. 재개 메모의 전체 이력은 일본어판 [`PORTING.md`](../PORTING.md)가 정본이며, 기술적 결정은 [`CLAUDE.md`](../CLAUDE.md)에 있습니다.

## 현재 상황(2026-09-23)

- 완료: IMAPI2 굽기(실제 CD-R로 검증), AV1/Opus, Dolby/서라운드 유지, AI 노이즈 제거(RNNoise), DSD64–1024(DSF)와 DoP WAV,
  고해상도 PCM, AI 영상 초해상도(GPU/CPU), 실험적 오디오 대역 확장, 음악 CD 추출(실제 디스크로 검증), MKV 여러 음성/자막 트랙.
- 2026-09-23: 8번 항목에서 구간 자르기를 조작할 수 없던 버그 수정, DSD 생성 시 PCM을 함께 만들지 않음, 배타적인 YES/NO "크기로 자르기/시간으로 자르기"(기본값은 크기),
  후처리 체크박스 "디스크 가득 채우기"와 "AI 무음 자르기", 재생 규격 상한(CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / PC 전용), 문서 다국어화.

## 다음 할 일

1. 2026-09-23 기능의 실제 파일 E2E: 크기/시간 자르기 위치, 재생 규격별 `-ar`·비트 심도·비트레이트 상한, 디스크 가득 채우기 비트레이트 계산.
2. 음악 CD(CD-DA) 굽기(IMAPI2 TrackAtOnce, 현재는 데이터 디스크만).
3. `rs-*` 도구를 설치 프로그램에서 빼고 자매 저장소의 릴리스에서 필요할 때 받아오기.
4. "영상 + DSD 음성"을 세트로 내보내기(영상 파일 + `.dsf` + `open-bar`용 `.obar.json`), ΔΣ 변조기를 `open-mqa-dsd`로 분리.
5. 보호가 없는 BD/DVD 추출(저작권 보호 우회는 구현하지 않음).

## 관련 저장소

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — 이 앱
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) / [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — FFmpeg / xorriso의 Rust 버전
- [aon-co-jp/open-cpu](https://github.com/aon-co-jp/open-cpu) — CPU 명령어 세트 감지
- [aon-co-jp/open-cuda](https://github.com/aon-co-jp/open-cuda) — GPU 연산 추상화 계층
- [aon-co-jp/open-mqa](https://github.com/aon-co-jp/open-mqa) / [aon-co-jp/open-mqa-dsd](https://github.com/aon-co-jp/open-mqa-dsd) / [aon-co-jp/open-bar](https://github.com/aon-co-jp/open-bar) — 고해상도 오디오 파이프라인, DSD 도구, 플레이어
