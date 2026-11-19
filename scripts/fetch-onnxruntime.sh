#!/usr/bin/env bash
# Windows x64向けにonnxruntime(DirectML対応ビルド)とDirectML.dllを取得し、
# src-tauri/binaries/onnxruntime/へ配置する(AIアップミックスの高速化用)。
# 実行時は`engine::ai_upmix`が<exeのフォルダ>/onnxruntime/onnxruntime.dllを探す。
# 見つからなくてもAIアップミックスは純Rustのtractで動く(遅いだけ)ので、失敗してもビルドは壊さない設計。
# 配布元: PyPIのonnxruntime-directml(Microsoft、MITライセンス)のwheelからDLLだけを取り出す。
# ARM64などx64以外は、DLLの種類が違うので何もしない。
set -euo pipefail
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) ;;
  *) echo "fetch-onnxruntime.sh: not Windows, skipping"; exit 0 ;;
esac
if [ "${RUNNER_ARCH:-X64}" != "X64" ]; then echo "fetch-onnxruntime.sh: ${RUNNER_ARCH} is not X64, skipping"; exit 0; fi
VER="1.24.4"
DEST="src-tauri/binaries/onnxruntime"
BASE="${RUNNER_TEMP:-/tmp}"
if command -v cygpath >/dev/null 2>&1; then BASE="$(cygpath -u "$BASE")"; fi
TMP="$BASE/ort-fetch"
rm -rf "$TMP" "$DEST"; mkdir -p "$TMP" "$DEST"
python -m pip download "onnxruntime-directml==${VER}" --no-deps --only-binary=:all: --platform win_amd64 --python-version 3.13 -d "$TMP" -q
unzip -q -o "$TMP"/*.whl -d "$TMP/w"
CAPI=$(dirname "$(find "$TMP/w" -iname onnxruntime.dll | head -1)")
cp "$CAPI/onnxruntime.dll" "$CAPI/onnxruntime_providers_shared.dll" "$CAPI/DirectML.dll" "$DEST/"
echo "fetch-onnxruntime.sh: done"; ls -la "$DEST"
