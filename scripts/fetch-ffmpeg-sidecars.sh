#!/usr/bin/env bash
# Windows/Linux向けにffmpeg/ffprobeの静的バイナリをダウンロードし、
# src-tauri/binaries/へTauriのsidecar命名規則
# (<name>-<target-triple>[.exe])で配置する(2026-09-14新設)。
# macOS/xorrisoは対応していない(理由: src-tauri/binaries/README.md参照)。
#
# 使い方: bash scripts/fetch-ffmpeg-sidecars.sh
# (リポジトリルートから実行する想定、`.github/workflows/release.yml`の
# Windows/Linuxビルドジョブがビルド前に自動実行する)
set -euo pipefail

BIN_DIR="src-tauri/binaries"
mkdir -p "$BIN_DIR"

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    OS=windows
    ;;
  Linux)
    OS=linux
    ;;
  *)
    echo "fetch-ffmpeg-sidecars.sh: unsupported OS '$(uname -s)' (only Windows/Linux are supported, see src-tauri/binaries/README.md)" >&2
    exit 1
    ;;
esac

# `/releases/latest/download/ffmpeg-master-latest-*`という固定名エイリアスは
# 実際には404を返すことがある(2026-09-14に実機確認済み、GitHubの
# アセット命名がバージョン付きの実名`ffmpeg-N-<build>-<hash>-*`のみに
# なっているらしくエイリアスが機能しないケースがある)。そのため
# GitHub APIで最新リリースの実際のアセットURLを解決してから
# ダウンロードする。
# 未認証のGitHub APIはCIランナーの共有IPでレート制限されることがある(v0.1.19で
# 実際に発生: 応答が空になりgrepが失敗、メッセージ無しでexit 1)。GITHUB_TOKENがあれば
# 認証し、失敗時は数回リトライして、原因が分かるメッセージを出す。
AUTH_ARGS=()
if [ -n "${GITHUB_TOKEN:-}" ]; then AUTH_ARGS=(-H "Authorization: Bearer $GITHUB_TOKEN"); fi
LATEST_RELEASE_JSON=""
for attempt in 1 2 3 4 5; do
  LATEST_RELEASE_JSON=$(curl -sL "${AUTH_ARGS[@]}" "https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/latest" || true)
  if echo "$LATEST_RELEASE_JSON" | grep -q browser_download_url; then break; fi
  echo "fetch-ffmpeg-sidecars.sh: GitHub API attempt $attempt failed: $(echo "$LATEST_RELEASE_JSON" | head -c 300)" >&2
  sleep $((attempt * 5))
done

if [ "$OS" = windows ]; then
  TARGET_TRIPLE="x86_64-pc-windows-msvc"
  ARCHIVE_URL=$(echo "$LATEST_RELEASE_JSON" | grep -o '"browser_download_url": *"[^"]*win64-gpl\.zip"' | head -1 | sed -E 's/.*"(https[^"]+)"/\1/' || true)
  if [ -z "$ARCHIVE_URL" ]; then
    echo "fetch-ffmpeg-sidecars.sh: failed to resolve the win64-gpl.zip asset URL from BtbN/FFmpeg-Builds latest release" >&2
    exit 1
  fi
  ARCHIVE="/tmp/ffmpeg-sidecar.zip"
  curl -sL "$ARCHIVE_URL" -o "$ARCHIVE"
  EXTRACT_DIR="/tmp/ffmpeg-sidecar-extract"
  rm -rf "$EXTRACT_DIR"
  mkdir -p "$EXTRACT_DIR"
  unzip -q "$ARCHIVE" -d "$EXTRACT_DIR"
  FFMPEG_SRC=$(find "$EXTRACT_DIR" -iname "ffmpeg.exe" | head -1)
  FFPROBE_SRC=$(find "$EXTRACT_DIR" -iname "ffprobe.exe" | head -1)
  cp "$FFMPEG_SRC" "$BIN_DIR/ffmpeg-${TARGET_TRIPLE}.exe"
  cp "$FFPROBE_SRC" "$BIN_DIR/ffprobe-${TARGET_TRIPLE}.exe"
else
  TARGET_TRIPLE="x86_64-unknown-linux-gnu"
  ARCHIVE_URL=$(echo "$LATEST_RELEASE_JSON" | grep -o '"browser_download_url": *"[^"]*linux64-gpl\.tar\.xz"' | head -1 | sed -E 's/.*"(https[^"]+)"/\1/' || true)
  if [ -z "$ARCHIVE_URL" ]; then
    echo "fetch-ffmpeg-sidecars.sh: failed to resolve the linux64-gpl.tar.xz asset URL from BtbN/FFmpeg-Builds latest release" >&2
    exit 1
  fi
  ARCHIVE="/tmp/ffmpeg-sidecar.tar.xz"
  curl -sL "$ARCHIVE_URL" -o "$ARCHIVE"
  EXTRACT_DIR="/tmp/ffmpeg-sidecar-extract"
  rm -rf "$EXTRACT_DIR"
  mkdir -p "$EXTRACT_DIR"
  tar -xf "$ARCHIVE" -C "$EXTRACT_DIR"
  FFMPEG_SRC=$(find "$EXTRACT_DIR" -iname "ffmpeg" -type f | head -1)
  FFPROBE_SRC=$(find "$EXTRACT_DIR" -iname "ffprobe" -type f | head -1)
  cp "$FFMPEG_SRC" "$BIN_DIR/ffmpeg-${TARGET_TRIPLE}"
  cp "$FFPROBE_SRC" "$BIN_DIR/ffprobe-${TARGET_TRIPLE}"
  chmod +x "$BIN_DIR/ffmpeg-${TARGET_TRIPLE}" "$BIN_DIR/ffprobe-${TARGET_TRIPLE}"
fi

echo "fetch-ffmpeg-sidecars.sh: done, placed into $BIN_DIR:"
ls -la "$BIN_DIR"
