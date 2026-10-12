#!/usr/bin/env bash
# 姉妹リポジトリ rs-FFmpeg・rs-xorriso(このプロジェクトのために作成した
# 純Rust実装、いずれもMITライセンス)をソースからビルドし、
# src-tauri/binaries/へTauriのsidecar命名規則(<name>-<target-triple>
# [.exe])で配置する(2026-09-16新設、ユーザー指示「作成したRust版も
# 同梱して」への対応)。
#
# ## 正直な開示(誇張しない、CLAUDE.mdにも記載)
# rs-FFmpeg/rs-xorrisoは早期WIPで、本家ffmpeg/xorriso(既に
# fetch-ffmpeg-sidecars.shで同梱済み)の完全な代替にはならない
# (rs-xorriso: ISO生成のみ・長いファイル名切り詰め・ディスク書き込み
# 機能無し。rs-ffmpeg: 無圧縮WAVのprobe/変換のみ・コーデック指定や
# ビットレート制御は非対応)。このスクリプトが同梱するのは、あくまで
# 「試してみたい人のための実験的な純Rustツール」であり、make-diskの
# アプリ本体はこれらを自動選択・呼び出さない(常に本家ffmpeg/xorriso、
# または無ければPATH上のものを使う——`engine/sidecar.rs`参照)。
#
# 使い方: bash scripts/build-rs-tribute-sidecars.sh
# (リポジトリルートから実行する想定。デフォルトでは
# `../rs-FFmpeg`・`../rs-xorriso`〈make-diskと同じ階層にcloneされている
# 前提〉を参照するが、`RS_FFMPEG_DIR`/`RS_XORRISO_DIR`環境変数で
# 上書きできる〈CI側はこれらをgit cloneして渡す〉)。
set -euo pipefail

BIN_DIR="src-tauri/binaries"
mkdir -p "$BIN_DIR"

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) EXE_SUFFIX=".exe" ;;
  Linux) EXE_SUFFIX="" ;;
  *)
    echo "build-rs-tribute-sidecars.sh: unsupported OS '$(uname -s)' (only Windows/Linux are supported)" >&2
    exit 1
    ;;
esac

TARGET_TRIPLE=$(rustc -vV | sed -n 's/^host: //p')

RS_FFMPEG_DIR="${RS_FFMPEG_DIR:-../rs-FFmpeg}"
RS_XORRISO_DIR="${RS_XORRISO_DIR:-../rs-xorriso}"

build_one() {
  local src_dir="$1" bin_name="$2"
  if [ ! -d "$src_dir" ]; then
    echo "build-rs-tribute-sidecars.sh: '$src_dir' not found, skipping $bin_name" >&2
    return 1
  fi
  (cd "$src_dir" && cargo build --release)
  cp "$src_dir/target/release/${bin_name}${EXE_SUFFIX}" "$BIN_DIR/${bin_name}-${TARGET_TRIPLE}${EXE_SUFFIX}"
}

build_one "$RS_FFMPEG_DIR" "rs-ffmpeg" || true
build_one "$RS_XORRISO_DIR" "rs-xorriso" || true

echo "build-rs-tribute-sidecars.sh: done, placed into $BIN_DIR:"
ls -la "$BIN_DIR"
