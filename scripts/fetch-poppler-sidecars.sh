#!/usr/bin/env bash
# Windows向けにpoppler-utils(pdftoppm/pdfinfo+依存DLL)をダウンロードし、
# src-tauri/binaries/poppler/へ配置する(PDF右綴じ/左綴じ見開き変換用)。
# DLL同梱が必要なためexternalBinではなくbundle.resourcesで同梱し、
# 実行時は`engine::sidecar`が<exeのフォルダ>/poppler/bin/を探す。
# Linuxはdebの依存関係(poppler-utils)で導入されるため何もしない。
# 配布元: https://github.com/oschwartz10612/poppler-windows (GPL/MIT系、ソースは同リポジトリ参照)
set -euo pipefail
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) ;;
  *) echo "fetch-poppler-sidecars.sh: not Windows, skipping (Linux uses the poppler-utils package)"; exit 0 ;;
esac
VER="v26.09.0-0"
ZIP_VER="26.09.0-0"
DEST="src-tauri/binaries/poppler"
TMP="${RUNNER_TEMP:-/tmp}/poppler-fetch"
rm -rf "$TMP" "$DEST"; mkdir -p "$TMP" "$DEST"
for attempt in 1 2 3; do
  curl -sfL -o "$TMP/p.zip" "https://github.com/oschwartz10612/poppler-windows/releases/download/${VER}/Release-${ZIP_VER}.zip" && break
  echo "fetch-poppler-sidecars.sh: download attempt $attempt failed" >&2; sleep $((attempt * 5))
done
unzip -q "$TMP/p.zip" -d "$TMP"
BIN=$(find "$TMP" -iname pdftoppm.exe | head -1 | xargs dirname)
# 配置は poppler/bin(exe+DLL) と poppler/share/poppler(poppler-data: CJK用cMap等)。
# popplerはDLLの隣の ../share/poppler を参照するため、この相対配置を保つ。
mkdir -p "$DEST/bin" "$DEST/share"
cp "$BIN"/*.dll "$DEST/bin/"
cp "$BIN/pdftoppm.exe" "$BIN/pdfinfo.exe" "$DEST/bin/"
cp -r "$(dirname "$BIN")/share/poppler" "$DEST/share/poppler"
echo "fetch-poppler-sidecars.sh: done"; find "$DEST" -type f | wc -l
