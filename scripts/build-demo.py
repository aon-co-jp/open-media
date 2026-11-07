#!/usr/bin/env python3
"""Webデモ(webpage/demo/)を、実際のアプリの画面(src/)から作る。
Builds the web demo (webpage/demo/) from the real app UI (src/).

  python scripts/build-demo.py

- src/ の index.html / main.js / ux.js / styles.css / fonts をそのまま使う(画面は常に実物と同じ)。
- Tauriの命令は webpage/demo-src/demo-stub.js が疑似的に返す(音は出ず、ファイルにも触れない)。
- index.html にデモの注意書きの帯を差し込み、stubをmain.jsより先に読み込ませる。
- 画面(src/)を変えたら、このスクリプトを再実行して、webpage/demo/ を更新してコミットする。
"""
import pathlib
import shutil

root = pathlib.Path(__file__).resolve().parent.parent
src = root / "src"
out = root / "webpage" / "demo"
stub = root / "webpage" / "demo-src" / "demo-stub.js"

if out.exists():
    # fonts / sample.mp4 は残してよいが、他は作り直す
    for p in out.iterdir():
        if p.name == "sample.mp4":
            continue
        if p.is_dir():
            shutil.rmtree(p)
        else:
            p.unlink()
out.mkdir(parents=True, exist_ok=True)

for name in ("main.js", "ux.js", "styles.css"):
    shutil.copy2(src / name, out / name)
shutil.copytree(src / "fonts", out / "fonts")
shutil.copy2(stub, out / "demo-stub.js")

html = (src / "index.html").read_text(encoding="utf-8")
banner = (
    '<div style="position:sticky;top:0;z-index:100;background:#fff3cd;color:#664d03;border-bottom:1px solid #ffda6a;'
    'padding:8px 12px;font:14px/1.5 system-ui,sans-serif;text-align:center">'
    "<b>デモ版 / Demo</b>: ブラウザ上の疑似動作です。音は出ず、ファイルにも触れません。変換・書き込みは動きません。"
    " / Simulated in your browser: no sound, no files touched; conversion and burning do not run. "
    '<a href="../" style="color:#0a58ca">ダウンロード / Download</a></div>\n'
)
assert "<body>" in html
html = html.replace("<body>", "<body>\n" + banner, 1)
assert '<script type="module" src="main.js"></script>' in html
html = html.replace('<script type="module" src="main.js"></script>', '<script src="demo-stub.js"></script>\n    <script type="module" src="main.js"></script>', 1)
html = html.replace("<title>open-media</title>", "<title>open-media デモ / Demo</title>", 1)
(out / "index.html").write_text(html, encoding="utf-8", newline="\n")
print("demo built:", out)
