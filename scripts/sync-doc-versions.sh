#!/usr/bin/env bash
# README.md(全言語版)の「最新版」表記・ファイル名一覧のバージョン番号を、
# package.jsonの`version`へ自動で揃える(2026-09-26新設、ユーザー指示)。
#
# **対象外(意図的)**: CLAUDE.md・PORTING.mdの本文は「その時点でどのバージョン
# だったか」を記録する過去ログ(HANDOFF追記・再開用メッセージ)であり、ここを
# 最新版へ書き換えると履歴が失われる。README.mdだけが「今のバージョン」を
# 指す生きたポインタなので、同期対象はREADME系だけにしている。
#
# 使い方:
#   bash scripts/sync-doc-versions.sh          # README系をpackage.jsonのversionへ揃えて上書き
#   bash scripts/sync-doc-versions.sh --check   # 揃っているか確認するだけ(ズレがあれば非ゼロ終了、書き換えない)
# リポジトリルートから実行する想定。バージョンを上げてタグをpushする前に、通常モードで
# 実行してからコミットに含める(README.mdだけタグと食い違ったまま公開されるのを防ぐ)。
# `--check`は`.github/workflows/release.yml`がビルド前に走らせ、揃えるのを忘れたまま
# タグをpushしてもリリースが失敗して気づける(CI側で書き換え・自動コミットまではしない
# ——複数プラットフォームのジョブが同時にpushしようとして競合するのを避けるため)。
set -euo pipefail
cd "$(dirname "$0")/.."

CHECK_ONLY=0
if [ "${1:-}" = "--check" ]; then CHECK_ONLY=1; fi

VERSION=$(grep -m1 '"version"' package.json | sed -E 's/.*"version": *"([^"]+)".*/\1/')
if [ -z "$VERSION" ]; then
  echo "sync-doc-versions.sh: package.jsonからversionを読めませんでした" >&2
  exit 1
fi

# `\d+\.\d+\.\d+`形式の版番号だけを一括置換する(旧版が何であっても対応できるよう
# パターンで拾う。ファイル名中の版番号・「最新版: vX.X.X」見出しの両方にマッチする)。
PATTERN='[0-9]+\.[0-9]+\.[0-9]+'

changed=0
mismatched=0
for f in README.md README/README.*.md; do
  [ -f "$f" ] || continue
  if ! grep -qE "$PATTERN" "$f"; then continue; fi

  if [ "$CHECK_ONLY" -eq 1 ]; then
    # ファイル内の版番号パターンが1つでも$VERSION以外なら不一致とみなす
    if grep -oE "$PATTERN" "$f" | grep -qv "^${VERSION}$"; then
      echo "sync-doc-versions.sh: $f is out of sync with package.json ($VERSION)" >&2
      mismatched=1
    fi
    continue
  fi

  before=$(md5sum "$f")
  sed -i -E "s/${PATTERN}/${VERSION}/g" "$f"
  after=$(md5sum "$f")
  if [ "$before" != "$after" ]; then
    echo "sync-doc-versions.sh: updated $f to $VERSION"
    changed=1
  fi
done

if [ "$CHECK_ONLY" -eq 1 ]; then
  if [ "$mismatched" -eq 1 ]; then
    echo "sync-doc-versions.sh: run 'bash scripts/sync-doc-versions.sh' (no --check) and commit the result before tagging" >&2
    exit 1
  fi
  echo "sync-doc-versions.sh: README系は $VERSION と一致しています"
  exit 0
fi

if [ "$changed" -eq 0 ]; then
  echo "sync-doc-versions.sh: README系は既に $VERSION と一致していました(変更なし)"
fi
