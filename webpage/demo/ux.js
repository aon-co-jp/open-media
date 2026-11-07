// 使いやすさの補助(2026-10-13): タブ切替・長い説明の折りたたみ・設定の記憶・ショートカットキー・ドラッグ&ドロップ。
// main.jsの機能は変えず、画面の見せ方と操作の入口だけを足す(失敗しても本体の機能は動く)。
const $ = (id) => document.getElementById(id);
const store = {
  get(k) { try { return localStorage.getItem("open-media." + k); } catch (_) { return null; } },
  set(k, v) { try { localStorage.setItem("open-media." + k, v); } catch (_) { /* 保存できなくても動く */ } },
};

// ── 1. タブ(再生 / 変換 / 書き込み) ─────────────────────────────
const TABS = [
  { id: "play", ja: "再生", en: "Play" },
  { id: "convert", ja: "変換", en: "Convert" },
  { id: "burn", ja: "書き込み", en: "Burn" },
];
function tabsOf(section) {
  if (section.querySelector("#run-btn")) return ["convert", "burn"];
  const h = section.querySelector("h2");
  const n = h ? parseFloat(h.textContent) : NaN;
  if (Number.isNaN(n)) return ["convert"];
  if (n < 1) return ["play"];
  if (n === 1 || n === 2) return ["convert", "burn"]; // ソースと出力先は両方で使う
  if (n === 5 || n === 6) return ["burn"];
  return ["convert"];
}
function setupTabs() {
  const app = document.querySelector(".app");
  const sections = [...app.querySelectorAll(":scope > section")];
  sections.forEach((s) => (s.dataset.tabs = tabsOf(s).join(" ")));
  const nav = document.createElement("nav");
  nav.className = "tabbar";
  nav.setAttribute("role", "tablist");
  const show = (id) => {
    store.set("tab", id);
    for (const s of sections) s.hidden = !s.dataset.tabs.split(" ").includes(id);
    for (const b of nav.querySelectorAll("button")) b.setAttribute("aria-selected", String(b.dataset.tab === id));
    window.scrollTo({ top: 0 });
  };
  for (const t of TABS) {
    const b = document.createElement("button");
    b.type = "button";
    b.dataset.tab = t.id;
    b.setAttribute("role", "tab");
    b.textContent = `${t.ja} / ${t.en}`;
    b.addEventListener("click", () => show(t.id));
    nav.appendChild(b);
  }
  const sub = app.querySelector(".subtitle");
  (sub || app.querySelector("h1")).after(nav);
  const saved = store.get("tab");
  show(TABS.some((t) => t.id === saved) ? saved : "play");
  window.__showTab = show;
}

// ── 2. 長い説明の折りたたみ ───────────────────────────────────
function setupHints() {
  document.querySelectorAll("p.hint:not([id])").forEach((p) => {
    if (p.textContent.length < 150 || p.dataset.folded) return;
    p.dataset.folded = "1";
    p.classList.add("folded");
    const b = document.createElement("button");
    b.type = "button";
    // 隠される説明文(engine-only/disc-only)には、そのボタンも一緒に隠す。
    b.className = ("more " + (p.className.match(/engine-only|disc-only/g) || []).join(" ")).trim();
    b.textContent = "▼ 詳細 / More";
    b.addEventListener("click", () => {
      const open = p.classList.toggle("folded");
      b.textContent = open ? "▼ 詳細 / More" : "▲ 閉じる / Less";
    });
    p.after(b);
  });
}

// ── 3. 設定の記憶 ────────────────────────────────────────────
const REMEMBER = ["player-mode", "player-bits", "player-treble", "player-treble-all", "player-vres", "player-vol", "bridge-bits", "bridge-maxrate", "bridge-capture", "bridge-output"];
function setupMemory() {
  for (const id of REMEMBER) {
    const el = $(id);
    if (!el) continue;
    const apply = () => {
      const v = store.get(id);
      if (v === null) return;
      if (el.type === "checkbox") el.checked = v === "1";
      else if ([...(el.options || [])].length === 0 || [...el.options].some((o) => o.value === v) || el.tagName !== "SELECT") el.value = v;
      el.dispatchEvent(new Event("input"));
    };
    apply();
    if (el.tagName === "SELECT") new MutationObserver(apply).observe(el, { childList: true }); // 後から選択肢が入る欄(デバイス一覧)
    const save = () => store.set(id, el.type === "checkbox" ? (el.checked ? "1" : "0") : el.value);
    el.addEventListener("change", save);
    el.addEventListener("input", save);
  }
}

// ── 4. ショートカットキー(再生) ──────────────────────────────
function setupShortcuts() {
  document.addEventListener("keydown", (e) => {
    const t = e.target;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (t && /^(INPUT|SELECT|TEXTAREA|BUTTON)$/.test(t.tagName)) return;
    const seek = $("player-seek"), vol = $("player-vol");
    if (e.key === " " && !$("player-pause-btn").disabled) { e.preventDefault(); $("player-pause-btn").click(); }
    else if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && seek && !seek.disabled) {
      e.preventDefault();
      seek.value = String(Math.min(Number(seek.max), Math.max(0, Number(seek.value) + (e.key === "ArrowRight" ? 10 : -10))));
      seek.dispatchEvent(new Event("change"));
    } else if ((e.key === "ArrowUp" || e.key === "ArrowDown") && vol) {
      e.preventDefault();
      vol.value = String(Math.min(1, Math.max(0, Number(vol.value) + (e.key === "ArrowUp" ? 0.05 : -0.05))));
      vol.dispatchEvent(new Event("input"));
    } else if ((e.key === "s" || e.key === "S") && !$("player-stop-btn").disabled) { $("player-stop-btn").click(); }
  });
}

// ── 5. ドラッグ&ドロップ ─────────────────────────────────────
const MEDIA_RE = /\.(mp3|wav|flac|aac|m4a|ogg|opus|dsf|dff|mka|mp4|mkv|avi|mov|webm|mpg|mpeg|vob|m2ts|ts)$/i;
function setupDrop() {
  const ev = window.__TAURI__ && window.__TAURI__.event;
  if (!ev || !window.openMedia) return; // モバイル等では無効
  ev.listen("tauri://drag-drop", (e) => {
    const paths = ((e.payload && e.payload.paths) || []).filter((p) => MEDIA_RE.test(p) || /\.iso$/i.test(p));
    if (!paths.length) return;
    const tab = store.get("tab") || "play";
    if (tab === "play") {
      window.openMedia.play(paths[0]); // 再生タブ: 最初の1つを再生(同時再生はしない)
    } else {
      window.openMedia.addSources(paths); // 変換・書き込みタブ: ソースへ追加
    }
  }).catch(() => {});
}

for (const f of [setupTabs, setupHints, setupMemory, setupShortcuts, setupDrop]) {
  try { f(); } catch (e) { console.warn("ux:", f.name, e); }
}
