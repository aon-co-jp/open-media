// バンドラー(Vite等)を使わない素のWebView(特にAndroidのSystem WebView)は
// "@tauri-apps/api/core"のようなベア指定子を解決できず、モジュール読み込みが
// 例外で止まり、main.js内の全イベントリスナーが登録されないまま失敗する
// (実機検証で発見: ボタンが一切反応しない不具合の原因だった)。
// tauri.conf.jsonのwithGlobalTauri:trueで公開されるグローバルを使うことで、
// バンドラー無しでも全プラットフォームで確実に動く。
const invoke = window.__TAURI__.core.invoke;
const open = window.__TAURI__.dialog.open;
const save = window.__TAURI__.dialog.save;
const convertFileSrc = window.__TAURI__.core.convertFileSrc;

/**
 * @typedef {{ startSecs: number, endSecs: number | null }} CutRange
 * @typedef {{ startSecs: number, endSecs: number }} ExtractRange
 * @typedef {{ path: string, cutRanges: CutRange[], frameAccurate: boolean, extract?: ExtractRange | null, includeAsIs: boolean }} SourceFile
 */

// make-diskが変換できる(音声・動画・PDF)以外の拡張子は、既定で「そのまま含める」に
// する(2026-09-27新設)。以前はここに当てはまらないファイルを追加しても、変換されず
// ISOにも入らず黙って無視されていた(混乱の原因)。
const CONVERTIBLE_EXT_RE = /\.(mp3|wav|flac|aac|m4a|ogg|opus|mp4|mkv|avi|mov|webm|pdf)$/i;
function defaultIncludeAsIs(path) {
  return !CONVERTIBLE_EXT_RE.test(path);
}
/** @type {SourceFile[]} */
let sourceFiles = [];
let outputFolder = "";

const fileListEl = document.getElementById("file-list");
const outputFolderEl = document.getElementById("output-folder");
const logEl = document.getElementById("log");

const VIDEO_CODEC_ARGS = {
  mp4: ["-c:v", "libx264", "-c:a", "aac"],
  mkv: ["-c:v", "libx264", "-c:a", "aac"],
  avi: ["-c:v", "mpeg4", "-c:a", "libmp3lame"],
  mov: ["-c:v", "libx264", "-c:a", "aac"],
  webm: ["-c:v", "libvpx-vp9", "-c:a", "libopus"],
  // AV1(2026-09-19新設)。"-c:v av1"は疑似指定で、Rust側が使えるエンコーダ
  // (libsvtav1優先、無ければlibaom-av1)へ置き換える。出力名は`<名前>.av1.<拡張子>`。
  "av1-mkv": ["-c:v", "av1", "-c:a", "libopus"],
  "av1-webm": ["-c:v", "av1", "-c:a", "libopus"],
  "av1-mp4": ["-c:v", "av1", "-c:a", "aac"],
  // HEVC 10bit(HDR10互換のベースレイヤー)。音声は無変換でサラウンド/Atmosを保持。
  "hevc-hdr10-mkv": ["-c:v", "libx265", "-pix_fmt", "yuv420p10le", "-c:a", "copy"],
  // 全ストリーム無変換コピー(Dolby Vision RPU・Atmos・字幕等を保持)。
  "passthrough-mkv": ["-map", "0", "-c", "copy"],
};

const AUDIO_CODEC_ARGS = {
  mp3: ["-c:a", "libmp3lame"],
  wav: ["-c:a", "pcm_s16le"],
  flac: ["-c:a", "flac"],
  aac: ["-c:a", "aac"],
  ogg: ["-c:a", "libvorbis"],
  opus: ["-c:a", "libopus"],
  // DSD(2026-09-19新設): ffmpegでは書き出せないためRust側の自前変換器(engine::dsd)を使う。
  // 高解像度PCM(R-2R等マルチビットDAC向け、2026-09-19新設): 使える最高品質のリサンプラ+ディザ。
  dxd352: ["-af", "hq-resample@352800", "-c:a", "pcm_s24le"],
  pcm352_32: ["-af", "hq-resample@352800", "-c:a", "pcm_s32le"],
  pcm384_24: ["-af", "hq-resample@384000", "-c:a", "pcm_s24le"],
  pcm384_32: ["-af", "hq-resample@384000", "-c:a", "pcm_s32le"],
  pcm705_32: ["-af", "hq-resample@705600", "-c:a", "pcm_s32le"],
  pcm768_32: ["-af", "hq-resample@768000", "-c:a", "pcm_s32le"],
  dsd64: [],
  dsd128: [],
  dsd256: [],
  dsd512: [],
  dsd1024: [],
  ac3: ["-c:a", "ac3"],
  eac3: ["-c:a", "eac3"],
};

function secsToHms(totalSecs) {
  const s = Math.max(0, Math.floor(totalSecs));
  return { h: Math.floor(s / 3600), m: Math.floor((s % 3600) / 60), sec: s % 60 };
}

function hmsToSecs(h, m, sec) {
  return (parseInt(h, 10) || 0) * 3600 + (parseInt(m, 10) || 0) * 60 + (parseFloat(sec) || 0);
}

function formatHms(totalSecs) {
  const { h, m, sec } = secsToHms(totalSecs);
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
}

/** 開始/終了を時・分・秒の数値入力3つ(+任意の"末尾まで"チェック)として
 * 描画するミニフォーム。マウス操作(動画プレビューの現在位置をセット)と
 * 数字直接入力のどちらでも同じ値を編集できる。 */
function buildTimeInputs(labelText, initialSecs, videoEl, onSetFromPlayhead) {
  const wrap = document.createElement("span");
  wrap.className = "hms-input";

  const { h, m, sec } = secsToHms(initialSecs ?? 0);
  const hEl = document.createElement("input");
  hEl.type = "number";
  hEl.min = "0";
  hEl.value = h;
  hEl.className = "hms-h";
  const mEl = document.createElement("input");
  mEl.type = "number";
  mEl.min = "0";
  mEl.max = "59";
  mEl.value = m;
  mEl.className = "hms-m";
  const sEl = document.createElement("input");
  sEl.type = "number";
  sEl.min = "0";
  sEl.max = "59";
  sEl.value = sec;
  sEl.className = "hms-s";

  const setBtn = document.createElement("button");
  setBtn.type = "button";
  setBtn.textContent = "現在位置";
  setBtn.title = "動画プレビューの再生位置をこの欄にセットします";
  setBtn.addEventListener("click", () => {
    const { h: ch, m: cm, sec: cs } = secsToHms(videoEl.currentTime);
    hEl.value = ch;
    mEl.value = cm;
    sEl.value = cs;
    if (onSetFromPlayhead) onSetFromPlayhead();
  });

  const label = document.createElement("span");
  label.textContent = labelText;

  wrap.append(label, hEl, document.createTextNode(":"), mEl, document.createTextNode(":"), sEl, setBtn);
  const setSecs = (secs) => {
    const t = secsToHms(secs);
    hEl.value = t.h;
    mEl.value = t.m;
    sEl.value = t.sec;
  };
  return { el: wrap, getSecs: () => hmsToSecs(hEl.value, mEl.value, sEl.value), setSecs };
}

function renderCutEditor(container, file, index) {
  container.innerHTML = "";
  container.className = "cut-editor";

  const isVideo = /\.(mp4|mkv|avi|mov|webm)$/i.test(file.path);
  const isAudio = /\.(mp3|wav|flac|aac|m4a|ogg|opus)$/i.test(file.path);
  let video = null;
  if (isVideo || isAudio) {
    // 音声も<audio>で再生・シークでき、「現在位置」ボタンで時刻を取り込める。
    video = document.createElement(isVideo ? "video" : "audio");
    video.controls = true;
    video.preload = "metadata";
    video.style.maxWidth = "100%";
    video.style.width = "100%";
    video.src = convertFileSrc(file.path);
    container.appendChild(video);
  } else {
    const note = document.createElement("p");
    note.className = "hint";
    note.textContent = "このファイル形式はプレビューできません。数字で時:分:秒を直接指定してください。 / Preview isn't available for this format — enter times directly.";
    container.appendChild(note);
  }

  const rangeList = document.createElement("ul");
  rangeList.className = "cut-range-list";
  function renderRangeList() {
    rangeList.innerHTML = "";
    file.cutRanges.forEach((r, ri) => {
      const li = document.createElement("li");
      li.textContent = `カット ${ri + 1}: ${formatHms(r.startSecs)} 〜 ${r.endSecs === null ? "末尾まで" : formatHms(r.endSecs)}`;
      const removeBtn = document.createElement("button");
      removeBtn.textContent = "削除";
      removeBtn.addEventListener("click", () => {
        file.cutRanges.splice(ri, 1);
        renderRangeList();
        renderFileNames();
      });
      li.appendChild(removeBtn);
      rangeList.appendChild(li);
    });
  }
  renderRangeList();

  const cutBox = document.createElement("div");
  cutBox.className = "range-box range-box-remove";
  const cutTitle = document.createElement("h4");
  cutTitle.innerHTML = '<span class="range-badge range-badge-remove">🟠 ここを消す(カットする区間)</span> カットする区間(いくつでも追加可) / Ranges to cut out (add as many as you like)';
  const cutHint = document.createElement("p");
  cutHint.className = "hint";
  cutHint.textContent =
    "上の再生プレビューで消したい部分の頭で止めて「開始」の現在位置、消したい部分の終わりで止めて" +
    "「終了」の現在位置を押すと、マウス操作だけで区間を指定できます(時:分:秒を直接入力してもOK)。 / " +
    "Pause the preview at the start of what you want to remove and click Start's “現在位置”, then pause at the end " +
    "and click End's “現在位置” — this sets the range with only mouse clicks (typing h:m:s directly also works).";

  const addForm = document.createElement("div");
  addForm.className = "cut-range-form";

  const dummyVideo = video ?? { currentTime: 0 };
  const startInput = buildTimeInputs("開始", 0, dummyVideo);
  const endInput = buildTimeInputs("終了", 0, dummyVideo);

  const toEofCheckbox = document.createElement("input");
  toEofCheckbox.type = "checkbox";
  const toEofLabel = document.createElement("label");
  toEofLabel.append(toEofCheckbox, document.createTextNode(" 末尾までカット"));

  const addBtn = document.createElement("button");
  addBtn.textContent = "この区間をカットに追加";
  addBtn.addEventListener("click", () => {
    const startSecs = startInput.getSecs();
    const endSecs = toEofCheckbox.checked ? null : endInput.getSecs();
    if (endSecs !== null && endSecs <= startSecs) {
      log("エラー: 終了位置は開始位置より後にしてください。 / End must be after start.");
      return;
    }
    file.cutRanges.push({ startSecs, endSecs });
    renderRangeList();
    renderFileNames();
  });

  addForm.append(startInput.el, endInput.el, toEofLabel, addBtn);

  const frameAccurateLabel = document.createElement("label");
  const frameAccurateCheckbox = document.createElement("input");
  frameAccurateCheckbox.type = "checkbox";
  frameAccurateCheckbox.checked = file.frameAccurate;
  frameAccurateCheckbox.addEventListener("change", async () => {
    if (frameAccurateCheckbox.checked) {
      // GPUエンコーダの有無はRust側(実際に1フレーム試しエンコード)でしか
      // 判定できないため、ここではCPUの命令セット対応状況(open-cpu検出)
      // だけを見て、GPUが無かった場合にどれくらい遅くなりそうかを事前警告する。
      try {
        const estimate = await invoke("estimate_cpu_encode_speed");
        if (estimate.speed_hint === "slow" || estimate.speed_hint === "very_slow") {
          const proceed = window.confirm(
            `${estimate.message_ja}\n${estimate.message_en}\n\n` +
            "(GPUエンコーダが実際に見つかればこの警告どおりにはなりません / this warning won't apply if a GPU encoder is actually found)\n\n" +
            "このままフレーム精度カットを続けますか？ / Proceed with frame-accurate cutting anyway?"
          );
          if (!proceed) {
            frameAccurateCheckbox.checked = false;
            return;
          }
        }
      } catch (e) {
        // 参考情報の取得失敗は無視してよい(必須機能ではない)。
      }
    }
    file.frameAccurate = frameAccurateCheckbox.checked;
  });
  frameAccurateLabel.append(
    frameAccurateCheckbox,
    document.createTextNode(" フレーム精度で正確にカットする(GPUエンコーダがあれば自動使用、無ければCPU) / Frame-accurate cut (uses GPU encoder if available)")
  );

  // ── 必要な部分だけ切り出す(高速、2026-09-23新設) ──
  // 5時間の元データから60分・10分だけ欲しい場合に、全体を処理せず、開始位置へ直接シークして
  // 必要な長さだけを読み込む(ffmpegの入力側シーク`-ss`+`-t`)。処理時間は切り出す長さにほぼ比例する。
  // 色分け(2026-09-27新設、ユーザー指摘): 「使う範囲(青)」と「消す範囲(オレンジ)」が
  // 同じ見た目の開始・終了欄で並んでいて混乱の原因になっていたため、枠の色・見出しの
  // バッジで一目で区別できるようにした。指定するのは常にどちらか片方だけでよい(両方
  // 使うことは無い。切り出しを設定すると下のカット区間は使われない、既存の案内どおり)。
  const extractBox = document.createElement("div");
  extractBox.className = "extract-box range-box range-box-keep";
  const extractTitle = document.createElement("h4");
  extractTitle.innerHTML = '<span class="range-badge range-badge-keep">🔵 ここだけ残す(使う範囲)</span> 必要な部分だけ切り出す(高速・長い元データ向け) / Extract only the part you want (fast, for long sources)';
  const extractHint = document.createElement("p");
  extractHint.className = "hint";
  extractHint.textContent =
    "5時間の元データでも、欲しい部分(例: 60分・10分)だけを直接読み込むので、処理時間は切り出す長さぶんだけで済みます。切り出しを設定すると、下の「カットする区間」は使われません。 / " +
    "Even from a 5-hour source, only the part you want (e.g. 60 or 10 minutes) is read, so processing takes about as long as that part. When set, the cut ranges below are not used.";
  const exStart = buildTimeInputs("開始 / Start", file.extract?.startSecs ?? 0, dummyVideo);
  const exEnd = buildTimeInputs("終了 / End", file.extract?.endSecs ?? 0, dummyVideo);
  const exStatus = document.createElement("span");
  const showExtract = () => {
    exStatus.textContent = file.extract
      ? `設定中 / Set: ${formatHms(file.extract.startSecs)} 〜 ${formatHms(file.extract.endSecs)}(${formatHms(file.extract.endSecs - file.extract.startSecs)})`
      : "未設定(全体を使う) / Not set (whole file)";
  };
  const exSetBtn = document.createElement("button");
  exSetBtn.type = "button";
  exSetBtn.textContent = "この範囲だけ切り出す / Extract this range";
  exSetBtn.addEventListener("click", () => {
    const startSecs = exStart.getSecs();
    const endSecs = exEnd.getSecs();
    if (endSecs <= startSecs) {
      warn("切り出しの終了位置は開始位置より後にしてください。", "The end of the extract range must be after its start.");
      return;
    }
    file.extract = { startSecs, endSecs };
    showExtract();
    renderFileNames();
  });
  const exClearBtn = document.createElement("button");
  exClearBtn.type = "button";
  exClearBtn.textContent = "切り出しを解除 / Clear";
  exClearBtn.addEventListener("click", () => {
    file.extract = null;
    showExtract();
    renderFileNames();
  });
  showExtract();
  const exForm = document.createElement("div");
  exForm.className = "cut-range-form";
  exForm.append(exStart.el, exEnd.el, exSetBtn, exClearBtn);
  // ── AIで探す(aruaru-llm、2026-09-23新設) ──
  // 欲しい内容を文章で書くと、字幕(または音声の書き起こし)から合う範囲を探し、上の開始・終了へ入れる。
  const aiBox = document.createElement("div");
  aiBox.className = "cut-range-form";
  const aiReq = document.createElement("input");
  aiReq.type = "text";
  aiReq.placeholder = "欲しい内容(例: 龍神の開運の説明の部分) / What you want (e.g. the part explaining ...)";
  aiReq.style.minWidth = "18em";
  aiReq.style.flex = "1";
  const aiLen = document.createElement("input");
  aiLen.type = "number";
  aiLen.min = "1";
  aiLen.value = "10";
  aiLen.style.width = "5em";
  const aiLenLabel = document.createElement("label");
  aiLenLabel.append("長さ / Length ", aiLen, " 分 / min");
  const aiUrl = document.createElement("input");
  aiUrl.type = "text";
  aiUrl.style.width = "14em";
  aiUrl.title = "aruaru-llmのURL / aruaru-llm URL";
  try {
    aiUrl.value = localStorage.getItem("make-disk.aruaruLlmUrl") || "http://127.0.0.1:4600";
  } catch (e) {
    aiUrl.value = "http://127.0.0.1:4600";
  }
  const aiBtn = document.createElement("button");
  aiBtn.type = "button";
  aiBtn.textContent = "AIで探す / Find with AI";
  const aiResult = document.createElement("div");
  aiResult.className = "hint";
  const applyRange = (st, en) => {
    exStart.setSecs(st);
    exEnd.setSecs(en);
    file.extract = { startSecs: st, endSecs: en };
    showExtract();
    renderFileNames();
  };
  aiBtn.addEventListener("click", async () => {
    const request = aiReq.value.trim();
    const minutes = parseFloat(aiLen.value);
    if (!request || !(minutes > 0)) {
      warn("欲しい内容と長さ(分)を入力してください。", "Enter what you want and the length in minutes.");
      return;
    }
    try {
      localStorage.setItem("make-disk.aruaruLlmUrl", aiUrl.value.trim());
    } catch (e) {
      // 保存できなくても続行
    }
    aiBtn.disabled = true;
    aiResult.textContent = "探しています…(字幕が無い長い動画は、音声の書き起こしに時間がかかります) / Searching… (long videos without subtitles take time to transcribe)";
    try {
      const r = await invoke("ai_suggest_range", { path: file.path, request, lengthSecs: minutes * 60, llmUrl: aiUrl.value.trim() });
      applyRange(r.start_secs, r.end_secs);
      const src = { subtitle_file: "字幕ファイル / subtitle file", embedded_subtitle: "埋め込み字幕 / embedded subtitles", transcribe: "音声の書き起こし / transcription" }[r.source] || r.source;
      aiResult.textContent = `提案 / Suggested: ${formatHms(r.start_secs)} 〜 ${formatHms(r.end_secs)}(手掛かり / from: ${src})。${r.note}`;
      // 他の候補も選べるようにする
      const list = document.createElement("ul");
      r.candidates.forEach((c, i) => {
        const li = document.createElement("li");
        const b = document.createElement("button");
        b.type = "button";
        b.textContent = `候補${i + 1} / Option ${i + 1}: ${formatHms(c.start_secs)}〜${formatHms(c.end_secs)}`;
        b.addEventListener("click", () => applyRange(c.start_secs, c.end_secs));
        const pv = document.createElement("span");
        pv.textContent = ` ${c.preview.slice(0, 80)}…`;
        li.append(b, pv);
        list.appendChild(li);
      });
      aiResult.appendChild(list);
    } catch (e) {
      aiResult.textContent = `見つけられませんでした / Could not find: ${e}`;
    } finally {
      aiBtn.disabled = false;
    }
  });
  aiBox.append(aiReq, aiLenLabel, aiUrl, aiBtn);

  extractBox.append(extractTitle, extractHint, exForm, exStatus, aiBox, aiResult);

  const rangeLegend = document.createElement("p");
  rangeLegend.className = "range-legend hint";
  rangeLegend.innerHTML =
    '<span class="range-badge range-badge-keep">🔵 使う範囲</span>と<span class="range-badge range-badge-remove">🟠 消す範囲</span>は' +
    "同時には使いません。欲しい部分が1か所だけなら🔵、要らない部分を後で取り除きたいなら🟠を使ってください。" +
    " / Use only one of <span class=\"range-badge range-badge-keep\">🔵 keep</span> or " +
    '<span class="range-badge range-badge-remove">🟠 remove</span> at a time: pick 🔵 for a single wanted portion, 🟠 to cut out unwanted parts.';

  cutBox.append(cutTitle, cutHint, rangeList, addForm, frameAccurateLabel);

  container.append(rangeLegend, extractBox, cutBox);
}

// 8.「時間指定・トリミング」節の編集対象(sourceFilesのインデックス)。
// 以前は編集UIが1.のファイル一覧の中にしか無く、8.の節には説明文しか無かったため
// 8.から実際には操作できなかった(2026-09-23修正)。
let cutTargetIndex = -1;
const cutTargetSel = document.getElementById("cut-target");
const cutEditorHost = document.getElementById("cut-editor-host");
const cutEmptyNote = document.getElementById("cut-empty");

function fileLabel(f) {
  if (f.extract) return f.path + ` (切り出し ${formatHms(f.extract.startSecs)}〜${formatHms(f.extract.endSecs)})`;
  return f.path + (f.cutRanges.length > 0 ? ` (カット${f.cutRanges.length}件)` : "");
}

/** エディタを作り直さずにファイル名表示(カット件数)だけ更新する。 */
function renderFileNames() {
  fileListEl.querySelectorAll(".file-name").forEach((el, i) => {
    if (sourceFiles[i]) el.textContent = fileLabel(sourceFiles[i]);
  });
  Array.from(cutTargetSel.options).forEach((o) => {
    const f = sourceFiles[parseInt(o.value, 10)];
    if (f) o.textContent = fileLabel(f);
  });
}

function renderCutSection() {
  cutTargetSel.innerHTML = "";
  if (cutTargetIndex >= sourceFiles.length) cutTargetIndex = sourceFiles.length - 1;
  if (cutTargetIndex < 0 && sourceFiles.length > 0) cutTargetIndex = 0;
  sourceFiles.forEach((f, i) => cutTargetSel.add(new Option(fileLabel(f), String(i))));
  cutTargetSel.disabled = sourceFiles.length === 0;
  cutEmptyNote.hidden = sourceFiles.length > 0;
  cutEditorHost.innerHTML = "";
  if (cutTargetIndex >= 0) {
    cutTargetSel.value = String(cutTargetIndex);
    renderCutEditor(cutEditorHost, sourceFiles[cutTargetIndex], cutTargetIndex);
  }
}

cutTargetSel.addEventListener("change", () => {
  cutTargetIndex = parseInt(cutTargetSel.value, 10);
  renderCutSection();
});
renderCutSection();

function renderFileList() {
  fileListEl.innerHTML = "";
  sourceFiles.forEach((f, i) => {
    const li = document.createElement("li");
    li.className = "file-row";

    const name = document.createElement("span");
    name.className = "file-name";
    name.textContent = fileLabel(f);

    // 「そのまま含める」(2026-09-27新設): make-diskが変換できない形式
    // (文書・画像・アーカイブ等)や、あえて再変換したくないファイルを、無変換の
    // まま出力フォルダへコピーし、ISO/ディスクへ一緒に含める。音声/動画/PDFに
    // 一致しない形式は既定でON(付けないと黙って無視されていた挙動を解消)。
    const asIsLabel = document.createElement("label");
    asIsLabel.className = "include-as-is-label";
    const asIsCheckbox = document.createElement("input");
    asIsCheckbox.type = "checkbox";
    asIsCheckbox.checked = f.includeAsIs;
    asIsCheckbox.title =
      "無変換のままディスク(ISO)へ含めます。音声/動画への変換とは独立していて、両方チェックすれば変換版とオリジナル両方が入ります。 / " +
      "Include the file as-is on the disc (ISO), unconverted. Independent of audio/video conversion — check both to include the converted version and the original.";
    asIsCheckbox.addEventListener("change", () => {
      f.includeAsIs = asIsCheckbox.checked;
    });
    asIsLabel.append(asIsCheckbox, document.createTextNode(" そのまま含める / include as-is"));

    const editBtn = document.createElement("button");
    editBtn.textContent = "編集...";
    editBtn.addEventListener("click", () => {
      cutTargetIndex = i;
      renderCutSection();
      cutTargetSel.scrollIntoView({ behavior: "smooth", block: "start" });
    });

    const removeBtn = document.createElement("button");
    removeBtn.textContent = "削除";
    removeBtn.addEventListener("click", () => {
      sourceFiles.splice(i, 1);
      if (cutTargetIndex > i) cutTargetIndex--;
      renderFileList();
    });

    li.append(name, asIsLabel, editBtn, removeBtn);
    fileListEl.appendChild(li);
  });
  renderCutSection();
}

document.getElementById("add-files-btn").addEventListener("click", async () => {
  const selected = await open({
    multiple: true,
    filters: [
      { name: "音声/動画", extensions: ["mp3", "wav", "flac", "aac", "m4a", "ogg", "mp4", "mkv", "avi", "mov", "webm", "iso"] },
      { name: "すべてのファイル", extensions: ["*"] },
    ],
  });
  if (!selected) return;
  let paths = Array.isArray(selected) ? selected : [selected];
  if (outputFolder) {
    const clash = sourcesInFolder(outputFolder, paths);
    if (clash.length > 0) {
      warn(
        `出力先フォルダと同じフォルダにあるファイルは追加できません(${clash.length}件)。ソース元と出力先は別のフォルダにしてください。`,
        `Files in the same folder as the output folder cannot be added (${clash.length}). The source and output folders must be different.`
      );
      paths = paths.filter((x) => !clash.includes(x));
    }
  }
  for (const path of paths) {
    sourceFiles.push({ path, cutRanges: [], frameAccurate: false, includeAsIs: defaultIncludeAsIs(path) });
  }
  renderFileList();
});

// ディスクプレーヤー: ハイブリッド構成。
//  音声(CD・音声ファイル・DSD) → open-barエンジン(Rust側。共有/排他/DoP、高品質アップサンプル、高音補正)
//  映像(DVD/Blu-ray/動画)       → この画面の<video>(ブラウザ非対応形式はffmpegでMP4へ変換)
//  映像+別音声の組み合わせ       → 映像は消音で<video>、音声はopen-bar。音声を時間の基準に映像を追従させる
// どの経路でも同時に鳴らせるのは1つだけ(新しい再生の前に必ず全てを止め、準備中は全ボタンを無効化)。
const playerMedia = document.getElementById("player-media");
const playerStatus = document.getElementById("player-status");
const playBtn = document.getElementById("player-play-btn");
const playerScanBtn = document.getElementById("player-scan-btn");
const playerFileBtn = document.getElementById("player-file-btn");
const playerComboBtn = document.getElementById("player-combo-btn");
const playerComboSaveBtn = document.getElementById("player-combo-save-btn");
const playerPauseBtn = document.getElementById("player-pause-btn");
const playerStopBtn = document.getElementById("player-stop-btn");
const playerSeek = document.getElementById("player-seek");
const playerTime = document.getElementById("player-time");
const playerRoute = document.getElementById("player-route");
const VIDEO_RE = /\.(mp4|mkv|avi|mov|webm|mpg|mpeg|vob|m2ts|ts)$/i;
const fmtTime = (s) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
const PAUSE_LABEL = "⏸ 一時停止 / Pause";
const RESUME_LABEL = "▶ 再開 / Resume";

let playerBusy = false;
let barActive = false; // open-barで再生中(一時停止中を含む)
let barSeenPlaying = false; // 再生中の状態を一度でも確認したか(開始直後の「停止」を終了と誤解しないため)
let barStartedAt = 0;
let playerNowName = ""; // いま再生している曲・項目の名前(状況の行に出す)
let comboOffsetMs = 0; // 組み合わせ再生中の音声オフセット
let comboActive = false;
let seeking = false;
let barModes = [];
let playerItems = new Map();

function selectedPlayerItem() {
  const sel = document.querySelector('#player-tracks input[name="player-track"]:checked');
  return sel ? playerItems.get(sel.value) : undefined;
}

function updateButtons() {
  const item = selectedPlayerItem();
  playBtn.disabled = playerBusy || !item;
  playerScanBtn.disabled = playerBusy;
  playerFileBtn.disabled = playerBusy;
  playerComboBtn.disabled = playerBusy;
  playerComboSaveBtn.disabled = playerBusy || item?.kind !== "combo";
  playerPauseBtn.disabled = !(barActive || (playerMedia.currentSrc && !playerMedia.ended));
  playerStopBtn.disabled = !(barActive || playerMedia.currentSrc);
  playerSeek.disabled = !barActive;
}
function setPlayerBusy(b) {
  playerBusy = b;
  updateButtons();
}

async function stopEverything() {
  comboActive = false;
  playerMedia.pause();
  playerMedia.muted = false;
  playerMedia.removeAttribute("src");
  playerMedia.load();
  playerMedia.style.display = "none";
  try {
    await invoke("bar_stop");
  } catch (_) {
    /* エンジン未起動なら何もしない */
  }
  barActive = false;
  playerPauseBtn.textContent = PAUSE_LABEL;
  playerTime.textContent = "0:00 / 0:00";
  playerRoute.textContent = "";
  document.getElementById("player-mqa").textContent = "";
}

// 状態表示と、組み合わせ再生の音声→映像の同期(250msごと)
async function pollBar() {
  if (!barActive) return;
  try {
    const st = await invoke("bar_status");
    if (!seeking) {
      playerSeek.max = st.duration_secs || 0;
      playerSeek.value = st.position_secs;
    }
    playerTime.textContent = `${fmtTime(st.position_secs)} / ${fmtTime(st.duration_secs)}`;
    playerRoute.textContent =
      `${st.active_mode.letter}: ${st.active_mode.title_ja} / ${st.active_mode.title_en} — ${st.route_ja} / ${st.route_en}` +
      (st.note ? ` — ${st.note}` : "") +
      (st.underrun_frames ? ` — 音切れ/underruns: ${st.underrun_frames}` : "");
    // エンジンのメッセージ(読み込み中・エラーなど)があればそれを出す。無ければ、再生中・一時停止中の表示にする
    // (再生が始まるとメッセージは空になるため、書き換えないと「読み込み中…」が残ってしまう)。
    if (st.message) playerStatus.textContent = st.message;
    else if (st.state === "playing") playerStatus.textContent = `再生中 / Playing: ${playerNowName}`;
    else if (st.state === "paused") playerStatus.textContent = `一時停止中 / Paused: ${playerNowName}`;
    if (comboActive) {
      const target = st.position_secs + comboOffsetMs / 1000;
      if (Math.abs(playerMedia.currentTime - target) > 0.15) playerMedia.currentTime = Math.max(0, target);
      if (st.state === "paused" && !playerMedia.paused) playerMedia.pause();
      if (st.state === "playing" && playerMedia.paused) playerMedia.play().catch(() => {});
    }
    if (st.state === "playing" || st.state === "paused") barSeenPlaying = true;
    if (st.state === "stopped") {
      // 再生を始めた直後は、エンジンが読み込み中で一瞬「停止」と報告する。再生を一度でも確認するまで(最大60秒)は
      // 終了とみなさない。ただしエラー(排他モードを開けない等)の報告があれば、すぐ終わらせて理由を見せる。
      const failed = /開けません|失敗|エラー|cannot|failed|error/i.test(st.message || "");
      const loading = !barSeenPlaying && !failed && Date.now() - barStartedAt < 60000;
      if (!loading) {
        barActive = false;
        if (comboActive) playerMedia.pause();
        playerStatus.textContent = failed ? `再生できません / Cannot play: ${st.message}` : "再生が終わりました / Finished";
        updateButtons();
        return;
      }
    }
  } catch (_) {
    /* 次の周期で再試行 */
  }
  setTimeout(pollBar, 250);
}

async function startBar(path, allowFallback = true) {
  const mode = document.getElementById("player-mode").value;
  const treble = document.getElementById("player-treble").checked;
  const mqa = await invoke("bar_is_mqa", { path }).catch(() => false);
  document.getElementById("player-mqa").textContent = mqa
    ? "MQAを検出しました。復号は行いません。MQA対応DACへ渡すにはモードA(排他・ビットパーフェクト)で音量を100%にして再生してください。それ以外では通常のPCMとして鳴ります。 / MQA detected. It is never decoded. For an MQA DAC use mode A (exclusive, bit-perfect) at 100% volume; otherwise it plays as ordinary PCM."
    : "";
  try {
    const bits = Number(document.getElementById("player-bits").value);
    const trebleAll = document.getElementById("player-treble-all").checked;
    await invoke("bar_play", { path, mode, treble, trebleAll, bits });
  } catch (e) {
    if (!allowFallback) throw e;
    // open-barが使えない環境(Android/iOS、音声デバイスが開けない等)では、WebViewの再生へ縮退する。
    const ready = await invoke("prepare_disc_item_for_playback", { paths: [path], video: false, targetHeight: null });
    playerMedia.style.display = "";
    playerMedia.muted = false;
    playerMedia.src = convertFileSrc(ready);
    await playerMedia.play();
    playerRoute.textContent = `WebView再生に切替 / Fell back to WebView playback (${String(e).slice(0, 120)})`;
    return;
  }
  await invoke("bar_set_volume", { volume: Number(document.getElementById("player-vol").value) });
  barActive = true;
  barSeenPlaying = false;
  barStartedAt = Date.now();
  pollBar();
}

async function playOne(statusName, run) {
  if (playerBusy) return;
  setPlayerBusy(true);
  await stopEverything();
  playerNowName = statusName;
  playerStatus.textContent = `${statusName} を準備中... / Preparing...`;
  try {
    await run();
    playerStatus.textContent = `再生中 / Playing: ${statusName}`;
  } catch (e) {
    await stopEverything();
    playerStatus.textContent = `再生できません / Cannot play: ${e}`;
  } finally {
    setPlayerBusy(false);
  }
}

// 項目の種類ごとの再生。cd=音楽CDのトラック / combo=映像+音声 / dvd-title・bd-stream・file=ディスク内の項目
async function playItem(item, drive) {
  if (item.kind === "cd") {
    const wav = await invoke("prepare_cd_track_for_playback", { drive, track: item.track });
    await startBar(wav);
  } else if (item.kind === "combo") {
    comboOffsetMs = item.audio_offset_ms || 0;
    comboActive = true;
    const ready = await invoke("prepare_disc_item_for_playback", { paths: [item.video], video: true, targetHeight: videoTargetHeight() });
    playerMedia.style.display = "";
    playerMedia.muted = true; // 映像内蔵の音声は使わない(別音声が主)
    playerMedia.src = convertFileSrc(ready);
    await startBar(item.audio);
    await playerMedia.play().catch(() => {});
  } else if (item.kind === "file" && !VIDEO_RE.test(item.paths[0])) {
    await startBar(item.paths[0]); // 音声ファイル(DSDを含む)はopen-bar
  } else {
    // 映像(DVD/Blu-ray/MP4/MKV等): 映像は引き上げて消音で表示し、音声は別に取り出してopen-barでアップサンプリング再生する。
    // 音声を時間の基準(組み合わせ再生と同じ仕組み)に映像を追従させる。
    // 変換できない環境(ffmpegが無いスマホ等)では、元のファイルをそのまま再生する。
    const ready = await invoke("prepare_disc_item_for_playback", { paths: item.paths, video: true, targetHeight: videoTargetHeight() }).catch(() => item.paths[0]);
    playerMedia.style.display = "";
    playerMedia.src = convertFileSrc(ready);
    let audioPath = null;
    try {
      audioPath = await invoke("prepare_disc_item_for_playback", { paths: item.paths, video: false, targetHeight: null });
      comboOffsetMs = 0;
      comboActive = true;
      playerMedia.muted = true;
      await startBar(audioPath, false);
    } catch (e) {
      // 音声エンジンを使えない/音声が無い場合は、映像プレーヤーの音で再生する。
      comboActive = false;
      playerMedia.muted = false;
      playerRoute.textContent = `映像プレーヤーの音で再生 / Using the video player's own audio (${String(e).slice(0, 120)})`;
    }
    await playerMedia.play().catch(() => {});
  }
}

function videoTargetHeight() {
  if (!ffmpegLikely) return null;
  const h = Number(document.getElementById("player-vres").value);
  return h > 0 ? h : null;
}

playBtn.addEventListener("click", () => {
  const item = selectedPlayerItem();
  if (!item) return;
  playOne(item.label, () => playItem(item, document.getElementById("player-drive").value));
});

function addPlayerItem(listEl, key, label, item, disabled) {
  playerItems.set(key, item);
  const li = document.createElement("li");
  const lab = document.createElement("label");
  const radio = document.createElement("input");
  radio.type = "radio";
  radio.name = "player-track";
  radio.value = key;
  radio.disabled = !!disabled;
  radio.addEventListener("change", updateButtons);
  lab.append(radio, ` ${label}`);
  li.appendChild(lab);
  listEl.appendChild(li);
}

playerScanBtn.addEventListener("click", async () => {
  const driveSel = document.getElementById("player-drive");
  const listEl = document.getElementById("player-tracks");
  setPlayerBusy(true);
  try {
    if (!driveSel.options.length) {
      const devices = await invoke("list_optical_drives");
      for (const d of devices) driveSel.add(new Option(d, d));
      if (!devices.length) throw "光学ドライブが見つかりません / No optical drive found";
    }
    listEl.innerHTML = "";
    playerItems = new Map();
    let tracks = [];
    try {
      tracks = await invoke("list_cd_tracks", { drive: driveSel.value });
    } catch (_) {
      /* 音楽CDではない */
    }
    const audio = tracks.filter((t) => t.is_audio);
    if (audio.length) {
      for (const t of audio) {
        const secs = Math.round(t.sectors / 75);
        addPlayerItem(listEl, `cd-${t.number}`, `Track ${String(t.number).padStart(2, "0")} — ${fmtTime(secs)}`, { kind: "cd", track: t.number, label: `Track ${t.number}` });
      }
      playerStatus.textContent = "音楽CD: 曲を1つ選んで「再生」 / Audio CD: pick one track and press Play";
    } else {
      const items = await invoke("list_disc_items", { drive: driveSel.value });
      items.forEach((it, i) => {
        const size = it.bytes >= 1e9 ? `${(it.bytes / 1e9).toFixed(1)} GB` : `${(it.bytes / 1e6).toFixed(0)} MB`;
        addPlayerItem(listEl, `item-${i}`, `${it.label} (${size})`, it);
      });
      playerStatus.textContent = items.length
        ? "DVD/Blu-ray/データ: タイトルまたはファイルを1つ選んで「再生」(ブラウザ非対応の映像は変換してから再生するため、長い本編は待ち時間があります) / Pick one title or file and press Play (unsupported video is converted first, so long titles take a while)"
        : "再生できる項目が見つかりません(暗号化ディスクは読めません) / No playable items (encrypted discs cannot be read)";
    }
  } catch (e) {
    playerStatus.textContent = `ディスクを読めません / Cannot read disc: ${e}`;
  } finally {
    setPlayerBusy(false);
  }
});

// スマホ/タブレット(Android/iOS): ネイティブのファイル選択は content:// を返し、映像プレーヤーが読めない。
// ブラウザ標準のファイル入力でFileを受け取り、メモリ上のURL(Blob)で再生する。
const MOBILE_UA = /Android|iPhone|iPad/i.test(navigator.userAgent);
const playerFileInput = document.getElementById("player-file-input");
let playerBlobUrl = null;
playerFileInput.addEventListener("change", async () => {
  const f = playerFileInput.files && playerFileInput.files[0];
  if (!f) return;
  if (playerBusy) return;
  setPlayerBusy(true);
  await stopEverything();
  try {
    if (playerBlobUrl) URL.revokeObjectURL(playerBlobUrl);
    playerBlobUrl = URL.createObjectURL(f);
    playerMedia.style.display = "";
    playerMedia.muted = false;
    playerMedia.src = playerBlobUrl;
    await playerMedia.play();
    playerStatus.textContent = `再生中 / Playing: ${f.name}`;
  } catch (e) {
    playerStatus.textContent = `再生できません(この端末が対応していない形式の可能性) / Cannot play (format may be unsupported on this device): ${e}`;
  } finally {
    setPlayerBusy(false);
    playerFileInput.value = "";
  }
});
playerFileBtn.addEventListener("click", async () => {
  if (MOBILE_UA) { playerFileInput.click(); return; }
  const sel = await open({
    multiple: false,
    filters: [
      { name: "音声/動画", extensions: ["mp3", "wav", "flac", "aac", "m4a", "ogg", "opus", "dsf", "dff", "mka", "mp4", "mkv", "avi", "mov", "webm", "mpg", "mpeg", "vob", "m2ts", "ts"] },
      { name: "すべてのファイル", extensions: ["*"] },
    ],
  });
  if (!sel) return;
  const chosen = Array.isArray(sel) ? sel[0] : sel;
  playOne(chosen, () => playItem({ kind: "file", paths: [chosen], label: chosen }, ""));
});

// 映像+音声の組み合わせ(例: MP4動画 + DSD音声)。同名の映像と音声を自動で組み合わせる。
playerComboBtn.addEventListener("click", async () => {
  const dir = await open({ directory: true, multiple: false });
  if (!dir) return;
  const folder = Array.isArray(dir) ? dir[0] : dir;
  const listEl = document.getElementById("player-tracks");
  const combos = await invoke("bar_auto_pair", { folder });
  listEl.innerHTML = "";
  playerItems = new Map();
  combos.forEach((c, i) => addPlayerItem(listEl, `combo-${i}`, `${c.video}  +  ${c.audio}`, { ...c, kind: "combo", label: c.video.split(/[\\/]/).pop() }));
  playerStatus.textContent = combos.length
    ? "組み合わせを1つ選んで「再生」。映像の音は使わず、別音声を鳴らします / Pick a combo and press Play (the video's own audio is muted; the separate audio plays)"
    : "同名の映像+音声の組み合わせが見つかりません / No same-named video + audio pairs found";
  updateButtons();
});
playerComboSaveBtn.addEventListener("click", async () => {
  const item = selectedPlayerItem();
  if (!item || item.kind !== "combo") return;
  const out = await save({ defaultPath: item.video.replace(/\.[^.\\/]+$/, "") + ".obar.json", filters: [{ name: "open-bar combo", extensions: ["json"] }] });
  if (!out) return;
  try {
    await invoke("bar_save_combo", { video: item.video, audio: item.audio, audioOffsetMs: item.audio_offset_ms || 0, outPath: out });
    playerStatus.textContent = `保存しました / Saved: ${out}`;
  } catch (e) {
    playerStatus.textContent = `保存できません / Cannot save: ${e}`;
  }
});

// 操作: 一時停止・停止・シーク・音量・モード切替(再生中は同じ位置から新しいモードで再開)
playerPauseBtn.addEventListener("click", async () => {
  if (barActive) {
    const st = await invoke("bar_status");
    if (st.state === "paused") {
      await invoke("bar_resume");
      playerPauseBtn.textContent = PAUSE_LABEL;
    } else {
      await invoke("bar_pause");
      playerPauseBtn.textContent = RESUME_LABEL;
    }
  } else if (playerMedia.paused) {
    playerMedia.play();
    playerPauseBtn.textContent = PAUSE_LABEL;
  } else {
    playerMedia.pause();
    playerPauseBtn.textContent = RESUME_LABEL;
  }
});
playerStopBtn.addEventListener("click", async () => {
  await stopEverything();
  playerStatus.textContent = "停止しました / Stopped";
  updateButtons();
});
playerSeek.addEventListener("input", () => {
  seeking = true;
  playerTime.textContent = `${fmtTime(Number(playerSeek.value))} / ${fmtTime(Number(playerSeek.max))}`;
});
playerSeek.addEventListener("change", async () => {
  await invoke("bar_seek", { secs: Number(playerSeek.value) });
  seeking = false;
});
document.getElementById("player-vol").addEventListener("input", (e) => invoke("bar_set_volume", { volume: Number(e.target.value) }));
document.getElementById("player-treble").addEventListener("change", (e) => {
  if (barActive) invoke("bar_set_treble", { on: e.target.checked });
});
document.getElementById("player-treble-all").addEventListener("change", (e) => {
  if (e.target.checked) document.getElementById("player-treble").checked = true; // 全音源に掛けるには高音補正そのものも有効にする
});
async function showModeDesc() {
  if (!barModes.length) barModes = await invoke("bar_modes").catch(() => []);
  const id = document.getElementById("player-mode").value;
  const m = barModes.find((x) => x.id === id);
  document.getElementById("player-mode-desc").textContent = m ? `${m.desc_ja} / ${m.desc_en}` : "";
}
document.getElementById("player-mode").addEventListener("change", async () => {
  await showModeDesc();
  if (barActive) await invoke("bar_set_mode", { mode: document.getElementById("player-mode").value });
});
showModeDesc();
// このOSで使える機能を調べ、使えないUI(光学ドライブ・排他モード・システム音声の橋渡し・映像の変換)を隠す。
let engineAvailable = true;
let ffmpegLikely = true;
(async () => {
  const hide = (sel) => document.querySelectorAll(sel).forEach((e) => (e.style.display = "none"));
  const modes = await invoke("bar_modes").catch(() => []);
  if (!modes.length) { engineAvailable = false; hide(".engine-only"); }
  await invoke("list_optical_drives").then((d) => { if (!d.length) hide(".disc-only"); }).catch(() => hide(".disc-only"));
  await invoke("bridge_devices").catch(() => hide("#bridge-section"));
  // スマホ/タブレット(Android)にはffmpegが無い: 映像の変換(アップコンバート)は行わず、対応形式をそのまま再生する。
  if (/Android|iPhone|iPad/i.test(navigator.userAgent)) { ffmpegLikely = false; hide("#vres-row"); hide("#vres-hint"); hide(".engine-only"); }
})();
// システム音声のアップサンプリング(ループバック橋渡し、Windows)
const bridgeCapture = document.getElementById("bridge-capture");
const bridgeOutput = document.getElementById("bridge-output");
const bridgeStartBtn = document.getElementById("bridge-start-btn");
const bridgeStopBtn = document.getElementById("bridge-stop-btn");
const bridgeStatusEl = document.getElementById("bridge-status");
let bridgeTimer = null;
async function loadBridgeDevices() {
  try {
    const devs = await invoke("bridge_devices"); // [[名前, 既定か], ...]
    bridgeCapture.innerHTML = "";
    bridgeOutput.innerHTML = "";
    for (const [name, isDef] of devs) {
      bridgeCapture.add(new Option(name + (isDef ? " (既定/default)" : ""), name));
      bridgeOutput.add(new Option(name + (isDef ? " (既定/default)" : ""), name));
    }
    // 既定の候補: 送り先=既定の出力以外を避けて最初のものではなく「既定」、受け取り=既定ではない最初のもの
    const def = devs.find((d) => d[1]);
    const other = devs.find((d) => !d[1]);
    if (def) bridgeOutput.value = def[0];
    if (other) bridgeCapture.value = other[0];
  } catch (e) {
    bridgeStatusEl.textContent = String(e);
    bridgeStartBtn.disabled = true;
  }
}
function showBridge(st) {
  if (!st) return;
  bridgeStatusEl.textContent = `${st.running ? "動作中 / Running" : "停止 / Stopped"}: ${st.capture_device} → ${st.output_device} — ` +
    `${st.in_rate_hz / 1000} kHz → ${st.out_rate_hz / 1000} kHz / ${st.out_bits}bit(コンテナ${st.container_bits}bit) — ` +
    `バッファ ${st.buffer_secs.toFixed(2)} 秒 / 音切れ ${st.underrun_frames} / 入力レベル ${(st.in_peak * 100).toFixed(0)}%` +
    (st.in_peak === 0 ? " — 音が届いていません(再生していない、またはDRMで無音) / no audio arriving (nothing playing, or DRM-silenced)" : "") +
    (st.message ? ` — ${st.message}` : "");
}
function pollBridge() {
  clearInterval(bridgeTimer);
  bridgeTimer = setInterval(async () => {
    const st = await invoke("bridge_status").catch(() => null);
    if (!st) { clearInterval(bridgeTimer); bridgeStartBtn.disabled = false; bridgeStopBtn.disabled = true; return; }
    showBridge(st);
    if (!st.running) { clearInterval(bridgeTimer); bridgeStartBtn.disabled = false; bridgeStopBtn.disabled = true; }
  }, 500);
}
bridgeStartBtn.addEventListener("click", async () => {
  bridgeStartBtn.disabled = true;
  try {
    const st = await invoke("bridge_start", { capture: bridgeCapture.value, output: bridgeOutput.value, maxRate: Number(document.getElementById("bridge-maxrate").value), bits: Number(document.getElementById("bridge-bits").value) });
    showBridge(st);
    bridgeStopBtn.disabled = false;
    pollBridge();
  } catch (e) {
    bridgeStatusEl.textContent = `開始できません / Cannot start: ${e}`;
    bridgeStartBtn.disabled = false;
  }
});
bridgeStopBtn.addEventListener("click", async () => {
  await invoke("bridge_stop").catch(() => {});
  clearInterval(bridgeTimer);
  bridgeStatusEl.textContent = "停止しました / Stopped";
  bridgeStartBtn.disabled = false;
  bridgeStopBtn.disabled = true;
});
document.getElementById("bridge-refresh-btn").addEventListener("click", loadBridgeDevices);
loadBridgeDevices();
playerMedia.style.display = "none";
playerMedia.addEventListener("pause", updateButtons);
playerMedia.addEventListener("play", updateButtons);

// 音楽CD(CD-DA)取り込み
let cddaTracks = [];
document.getElementById("cdda-scan-btn").addEventListener("click", async () => {
  const driveSel = document.getElementById("cdda-drive");
  const listEl = document.getElementById("cdda-tracks");
  try {
    if (!driveSel.options.length) {
      const devices = await invoke("list_burn_devices");
      for (const d of devices) driveSel.add(new Option(d, d));
      if (!devices.length) throw "光学ドライブが見つかりません / No optical drive found";
    }
    cddaTracks = await invoke("list_cd_tracks", { drive: driveSel.value });
  } catch (e) {
    log(`CDを読めません / Cannot read disc: ${e}`);
    return;
  }
  listEl.innerHTML = "";
  for (const t of cddaTracks) {
    const li = document.createElement("li");
    const secs = Math.round(t.sectors / 75);
    const label = `Track ${String(t.number).padStart(2, "0")} — ${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}` + (t.is_audio ? "" : " (データ/data)");
    li.innerHTML = `<label><input type="checkbox" data-track="${t.number}" ${t.is_audio ? "checked" : "disabled"} /> ${label}</label>`;
    listEl.appendChild(li);
  }
  document.getElementById("cdda-rip-btn").disabled = !cddaTracks.some((t) => t.is_audio);
  if (!cddaTracks.some((t) => t.is_audio)) log("音声トラックがありません(データディスクの可能性) / No audio tracks (probably a data disc)");
});

document.getElementById("cdda-rip-btn").addEventListener("click", async () => {
  if (!requireValidOutputFolder()) return;
  const tracks = [...document.querySelectorAll("#cdda-tracks input[data-track]:checked")].map((c) => Number(c.dataset.track));
  if (!tracks.length) return;
  const btn = document.getElementById("cdda-rip-btn");
  btn.disabled = true;
  log(`取り込み中(${tracks.length}トラック)... / Ripping ${tracks.length} track(s)...`);
  try {
    const outs = await invoke("rip_cd_tracks", { drive: document.getElementById("cdda-drive").value, tracks, outputDir: outputFolder + "/CD-rip", secure: document.getElementById("cdda-secure").checked });
    for (const path of outs) sourceFiles.push({ path, cutRanges: [], frameAccurate: false, includeAsIs: false });
    renderFileList();
    log(`取り込み完了 / Ripped ${outs.length} track(s) → ${outputFolder}/CD-rip`);
  } catch (e) {
    log(`取り込み失敗 / Rip failed: ${e}`);
  } finally {
    btn.disabled = false;
  }
});

// MKVの追加トラック(別ファイルの音声・字幕)。ソース一覧の先頭ファイルのMKV出力に多重化する。
const mkvExtraTracks = [];
function renderMkvExtraTracks() {
  const ul = document.getElementById("mkv-extra-list");
  ul.innerHTML = "";
  mkvExtraTracks.forEach((t, i) => {
    const li = document.createElement("li");
    const kindLabel = t.kind === "subtitle" ? "字幕 / subtitle" : "音声 / audio";
    li.append(`${kindLabel}: ${t.path} `);
    const lang = document.createElement("input");
    lang.placeholder = "言語 jpn / eng ...";
    lang.size = 10;
    lang.value = t.language || "";
    lang.addEventListener("input", () => (t.language = lang.value));
    const title = document.createElement("input");
    title.placeholder = "タイトル / title";
    title.size = 16;
    title.value = t.title || "";
    title.addEventListener("input", () => (t.title = title.value));
    const del = document.createElement("button");
    del.type = "button";
    del.textContent = "削除 / Remove";
    del.addEventListener("click", () => {
      mkvExtraTracks.splice(i, 1);
      renderMkvExtraTracks();
    });
    li.append(lang, title, del);
    ul.appendChild(li);
  });
}
async function addMkvTracks(kind) {
  const exts = kind === "subtitle" ? ["srt", "ass", "ssa", "vtt", "sub", "sup"] : ["mka", "wav", "flac", "aac", "m4a", "mp3", "ogg", "opus", "ac3", "eac3", "dts"];
  const selected = await open({ multiple: true, filters: [{ name: kind === "subtitle" ? "字幕 / subtitles" : "音声 / audio", extensions: exts }] });
  if (!selected) return;
  for (const path of Array.isArray(selected) ? selected : [selected]) mkvExtraTracks.push({ path, kind, language: "", title: "" });
  renderMkvExtraTracks();
}
document.getElementById("mkv-add-audio-btn").addEventListener("click", () => addMkvTracks("audio"));
document.getElementById("mkv-add-sub-btn").addEventListener("click", () => addMkvTracks("subtitle"));

document.getElementById("pick-output-btn").addEventListener("click", async () => {
  try {
    const dir = await open({ directory: true });
    if (!dir) return;
    if (sourcesInFolder(dir, sourceFiles.map((f) => f.path)).length > 0) {
      warn(SAME_FOLDER_JA, SAME_FOLDER_EN);
      return;
    }
    outputFolder = dir;
    outputFolderEl.value = dir;
  } catch (e) {
    if (String(e).includes("not implemented on mobile")) {
      // Tauri標準のdialogプラグインはモバイルでのフォルダ選択を未実装
      // なので、Android向けに追加した自前プラグイン(tauri-plugin-android-folder、
      // SAF ACTION_OPEN_DOCUMENT_TREE)にフォールバックする。
      try {
        const uri = await invoke("pick_output_tree");
        if (!uri) return; // ユーザーがキャンセル
        outputFolder = uri;
        outputFolderEl.value = uri;
        log(
          "注意: Androidでは選択したフォルダがcontent:// URIになります。ffmpeg/xorrisoはAndroidに" +
          "同梱されていないため、変換の実行自体は現時点でエラーになります(フォルダ選択UIの動作確認まで)。 / " +
          "Note: on Android the selected folder is a content:// URI. Since ffmpeg/xorriso aren't bundled for " +
          "Android, actually running a conversion will still fail for now — this only verifies the folder-picker UI."
        );
      } catch (e2) {
        log(`エラー: ${e2}`);
      }
    } else {
      log(`エラー: ${e}`);
    }
  }
});

document.getElementById("write-speed-mode").addEventListener("change", (e) => {
  const fixedInput = document.getElementById("write-speed-fixed");
  fixedInput.style.display = e.target.value === "fixed" ? "inline-block" : "none";
});

function log(msg) {
  logEl.textContent += msg + "\n";
}

// ── 出力先フォルダの検証(2026-09-23) ──────────────────
// ソース元と出力先は必ず違うフォルダにする(同じフォルダだと出力が元ファイルと混ざり、
// 同名出力で元データを上書きする危険もあるため)。警告は日英併記でダイアログとログの両方に出す。
function normDir(p) {
  let d = String(p).replace(/\\/g, "/").replace(/\/+$/, "");
  if (/^[a-z]:/i.test(d) || d.startsWith("//")) d = d.toLowerCase(); // Windowsは大文字小文字を区別しない
  return d;
}
function dirOf(filePath) {
  const p = String(filePath).replace(/\\/g, "/");
  return p.slice(0, p.lastIndexOf("/"));
}
function warn(ja, en) {
  log(`⚠ ${ja} / ${en}`);
  try {
    window.alert(`${ja}\n${en}`);
  } catch (e) {
    // ダイアログが使えない環境ではログのみ
  }
}
const SAME_FOLDER_JA = "ソース元と出力先に同じフォルダは選べません。出力先には別のフォルダを選んでください。";
const SAME_FOLDER_EN = "The source and output folders must be different. Please choose a different output folder.";
/** pathsのうち、folderと同じフォルダにあるものを返す。 */
function sourcesInFolder(folder, paths) {
  const d = normDir(folder);
  return paths.filter((x) => normDir(dirOf(x)) === d);
}
/** 実行系ボタン共通: 出力先が未選択、またはソースと同じフォルダなら警告してfalse。 */
function requireValidOutputFolder() {
  if (!outputFolder) {
    warn("出力先フォルダが選ばれていません。「2. 出力先フォルダ」でフォルダを選んでから実行してください。", "No output folder is selected. Choose one in section 2 before running.");
    return false;
  }
  if (sourcesInFolder(outputFolder, sourceFiles.map((f) => f.path)).length > 0) {
    warn(SAME_FOLDER_JA, SAME_FOLDER_EN);
    return false;
  }
  return true;
}

function checkedValues(name) {
  return Array.from(document.querySelectorAll(`input[name="${name}"]:checked`)).map((el) => el.value);
}

function bitrateMode() {
  return document.querySelector('input[name="bitrate-mode"]:checked').value;
}

function baseName(path) {
  return path.replace(/\\/g, "/").split("/").pop().replace(/\.[^.]+$/, "");
}

/** カット区間を考慮した実効尺(自動ビットレート算出用)。
 * カット区間の合計を元の尺から差し引く。 */
async function effectiveDurationSecs(f) {
  if (f.extract) return Math.max(0, f.extract.endSecs - f.extract.startSecs);
  let total = 0;
  try {
    const info = await invoke("probe_media", { path: f.path });
    total = info.duration_secs;
  } catch (e) {
    log(`警告: ${f.path} の尺取得に失敗: ${e}`);
    return 0;
  }
  for (const r of f.cutRanges) {
    const end = r.endSecs ?? total;
    total -= Math.max(0, end - r.startSecs);
  }
  return Math.max(0, total);
}

/** 選択された複数ディスク種別のうち、最も容量が小さいものを基準に
 * 自動ビットレートを算出する(=どのディスクにも収まる保守的な値)。 */
async function computeAutoBitrateKbps(discTypes) {
  let totalDuration = 0;
  for (const f of sourceFiles) {
    totalDuration += await effectiveDurationSecs(f);
  }

  let minKbps = null;
  for (const discType of discTypes) {
    const kbps = await invoke("calc_auto_bitrate_kbps", {
      disc: discType,
      totalDurationSecs: totalDuration,
      reservedBytes: 50 * 1024 * 1024,
    });
    if (minKbps === null || kbps < minKbps) minKbps = kbps;
  }
  return minKbps ?? 0;
}

/** 同時実行数の目安。WebView(素のブラウザAPI)から分かる範囲の
 * `navigator.hardwareConcurrency`を使い、無ければ4を既定にする
 * (2026-09-16新設、ユーザー指示「非同期でマルチスレッドで同時に
 * 行える様に」への対応)。ffmpegプロセス自体もマルチスレッドで動くため
 * 「論理コア数と同じだけ同時起動」は詰め込みすぎになりやすく、半分
 * 程度を上限にする(最低1)。 */
function conversionConcurrency() {
  const cores = typeof navigator !== "undefined" && navigator.hardwareConcurrency ? navigator.hardwareConcurrency : 4;
  return Math.max(1, Math.floor(cores / 2));
}

/** `tasks`(引数無しの非同期関数の配列)を、同時実行数`limit`件までの
 * 並列度で全て実行する(単純なワーカープール、外部ライブラリ不要)。 */
async function runWithConcurrencyLimit(tasks, limit) {
  const results = new Array(tasks.length);
  let next = 0;
  async function worker() {
    while (true) {
      const i = next++;
      if (i >= tasks.length) return;
      results[i] = await tasks[i]();
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, tasks.length) }, worker));
  return results;
}

/** 音声/動画フォーマット×ソースファイルの全組み合わせを変換する。
 * 2026-09-16変更: 逐次(1件ずつawait)ではなく、`runWithConcurrencyLimit`で
 * 複数のffmpegプロセスを同時に起動する非同期・マルチスレッド実行に
 * した(1トラック変換完了を待ってから次へ、という無駄な直列待ちを
 * 無くす——例: 「MP4をWAVへ」+「ISO化」のような組み合わせでも、
 * 複数ファイル・複数フォーマットの変換自体は並列に進む)。 */
/** 解像度指定(2026-09-17新設)。「無指定」はnull(元のまま)、「カスタム」は
 * 入力欄の値、「AI最適化」はソースの解像度を検出し4Kを超える場合のみ
 * 4Kへ抑える簡易ヒューリスティック(意味的な画質判断は行わない、
 * 正直な開示——index.htmlの注記参照)。動画ファイルにのみ適用する。 */
async function resolveResolutionSetting(f) {
  const preset = document.getElementById("resolution-preset").value;
  if (preset === "unspecified") return null;
  if (preset === "custom") {
    const width = parseInt(document.getElementById("resolution-custom-width").value, 10);
    const height = parseInt(document.getElementById("resolution-custom-height").value, 10);
    return width > 0 && height > 0 ? { width, height } : null;
  }
  if (preset === "ai") {
    try {
      const info = await invoke("probe_media", { path: f.path });
      const max4k = 3840 * 2160;
      if (info.width && info.height && info.width * info.height > max4k) {
        const scale = Math.sqrt(max4k / (info.width * info.height));
        return { width: Math.round((info.width * scale) / 2) * 2, height: Math.round((info.height * scale) / 2) * 2 };
      }
      return null; // 4K以下ならそのまま(不要な再エンコードを避ける)
    } catch (e) {
      log(`警告: AI最適化の解像度判定に失敗しました(${f.path}): ${e}`);
      return null;
    }
  }
  const [w, h] = preset.split("x").map((s) => parseInt(s, 10));
  return { width: w, height: h };
}

/** フレームレート指定(2026-09-17新設)。「AI最適化」はソースのfpsを検出し
 * 24/30/60/120のうち最も近い値へ合わせる簡易ヒューリスティック
 * (意味的な動き解析は行わない、正直な開示)。動画ファイルにのみ適用する。 */
async function resolveFpsSetting(f) {
  const preset = document.getElementById("fps-preset").value;
  if (preset === "unspecified") return null;
  if (preset === "custom") {
    const value = parseInt(document.getElementById("fps-custom-value").value, 10);
    return value > 0 ? value : null;
  }
  if (preset === "ai") {
    try {
      const info = await invoke("probe_media", { path: f.path });
      if (!info.fps) return null;
      const standardFps = [24, 30, 60, 120];
      const nearest = standardFps.reduce((a, b) => (Math.abs(b - info.fps) < Math.abs(a - info.fps) ? b : a));
      return Math.abs(nearest - info.fps) < 0.5 ? null : nearest; // 既に標準値に近ければ無変換
    } catch (e) {
      log(`警告: AI最適化のFPS判定に失敗しました(${f.path}): ${e}`);
      return null;
    }
  }
  return parseInt(preset, 10);
}

/** 元素材のDolby Vision/Atmos/サラウンドの検出結果をログに出す(1ファイル1回)。 */
const traitsLogged = new Set();
async function logSourceTraits(f) {
  if (traitsLogged.has(f.path)) return;
  traitsLogged.add(f.path);
  try {
    const info = await invoke("probe_media", { path: f.path });
    const traits = [];
    if (info.dolby_vision) traits.push("Dolby Vision");
    if (info.audio_profile && /atmos/i.test(info.audio_profile)) traits.push("Dolby Atmos");
    else if (info.audio_codec === "truehd") traits.push("TrueHD(Atmosの可能性 / may carry Atmos)");
    if (info.audio_channels && info.audio_channels > 2) traits.push(`ch サラウンド / surround`);
    if (traits.length > 0) {
      log(`検出:  — 。保持するには「無変換コピー」を選択してください。 / Detected: . Choose "Lossless copy" to keep them.`);
    }
  } catch (e) {
    // 検出は参考情報なので失敗しても続行する
  }
}

// 再生規格の上限(2026-09-23新設)。hz=最大サンプリング周波数、bits=最大ビット深度、
// audioKbps=音声の最大ビットレート、videoKbps=映像の最大ビットレート、
// videoAudioHz=動画内の音声に使う上限。null=上限なし(PC専用)。
const PLAYBACK_SPECS = {
  none: null,
  cd: { label: "CD(CD-DA / Video CD)", hz: 44100, bits: 16, audioKbps: 1411, videoKbps: 1150, videoAudioHz: 44100 },
  dvd: { label: "DVD-Video", hz: 96000, bits: 24, audioKbps: 6144, videoKbps: 9800, videoAudioHz: 48000 },
  dvda: { label: "DVD-Audio", hz: 192000, bits: 24, audioKbps: 9600, videoKbps: 9800, videoAudioHz: 48000 },
  bd: { label: "Blu-ray", hz: 192000, bits: 24, audioKbps: 27648, videoKbps: 40000, videoAudioHz: 48000 },
  uhdbd: { label: "Ultra HD Blu-ray", hz: 192000, bits: 24, audioKbps: 27648, videoKbps: 100000, videoAudioHz: 48000 },
  pc: { label: "PC専用 / PC only", hz: 768000, bits: 32, audioKbps: null, videoKbps: null, videoAudioHz: 192000 },
};

// 非可逆コーデック自身の上限(これを超える-ar/-bは指定できない、または無意味)。
const CODEC_LIMITS = {
  mp3: { hz: 48000, kbps: 320 },
  aac: { hz: 96000, kbps: 512 },
  ogg: { hz: 192000, kbps: 500 },
  opus: { hz: 48000, kbps: 510 },
  ac3: { hz: 48000, kbps: 640 },
  eac3: { hz: 48000, kbps: 6144 },
};

function selectedPlaybackSpec() {
  return PLAYBACK_SPECS[document.getElementById("playback-spec").value] ?? null;
}

(() => {
  const sel = document.getElementById("playback-spec");
  const detail = document.getElementById("playback-spec-detail");
  const kbps = (v) => (v === null ? "上限なし / unlimited" : v >= 1000 ? `${+(v / 1000).toFixed(3)} Mbps` : `${v} kbps`);
  sel.add(new Option("指定しない(元のまま) / No limit (keep source)", "none"));
  for (const [key, sp] of Object.entries(PLAYBACK_SPECS)) {
    if (!sp) continue;
    sel.add(new Option(`${sp.label} — 最大 / max ${sp.hz / 1000}kHz・${sp.bits}bit・音声 / audio ${kbps(sp.audioKbps)}・映像 / video ${kbps(sp.videoKbps)}`, key));
  }
  const show = () => {
    const sp = selectedPlaybackSpec();
    detail.textContent = sp ? `${sp.label}: 最大 ${sp.hz / 1000}kHz / ${sp.bits}bit、音声 ${kbps(sp.audioKbps)}、映像 ${kbps(sp.videoKbps)} / max ${sp.hz / 1000} kHz, ${sp.bits}-bit, audio ${kbps(sp.audioKbps)}, video ${kbps(sp.videoKbps)}` : "";
  };
  sel.addEventListener("change", show);
  show();

  // サイズ/時間カットのYES/NOは排他で、必ずどちらか一方がYES(「選択しない」は無い、既定はサイズ)。
  for (const [me, other] of [["cut-by-size", "cut-by-time"], ["cut-by-time", "cut-by-size"]]) {
    document.querySelector(`input[name="${me}"][value="yes"]`).addEventListener("change", () => {
      document.querySelector(`input[name="${other}"][value="no"]`).checked = true;
    });
    document.querySelector(`input[name="${me}"][value="no"]`).addEventListener("change", () => {
      document.querySelector(`input[name="${other}"][value="yes"]`).checked = true;
    });
  }
})();

// 変換に失敗した件数(実行ごとにリセット)。1件でも失敗したら、壊れた/不完全な出力を
// ISO化・書き込みしないために使う(2026-09-23、0.6秒のWAVがISOに入った実バグの再発防止)。
let conversionFailures = 0;

async function convertAll(formats, codecMap, mode, bitrateKbps) {
  // DSD作成時はPCMを同時に作らない仕様(2026-09-23)。DSD非対応のハードウェアでは
  // 再生側がDSD→PCMへ自動変換するため、PCM版は容量の無駄になる。
  // (DoP WAVはDSDデータそのものをPCMの入れ物に詰めたものなので対象外)
  if (codecMap === AUDIO_CODEC_ARGS && formats.some((x) => /^dsd\d+$/.test(x))) {
    const pcm = formats.filter((x) => /^(wav|flac|dxd352|pcm\d+_\d+)$/.test(x));
    if (pcm.length > 0) {
      formats = formats.filter((x) => !pcm.includes(x));
      log(`DSD作成時はPCM(${pcm.join(", ")})を作りません(DSD非対応機器では再生側が自動でPCMへ変換します)。 / PCM (${pcm.join(", ")}) is skipped when creating DSD (players convert DSD to PCM automatically on hardware without DSD).`);
    }
  }
  const jobs = [];
  for (const format of formats) {
    const codecArgs = codecMap[format];
    for (const f of sourceFiles) {
      if (f.path.toLowerCase().endsWith(".pdf")) {
        continue; // PDFは上のPDF見開き変換で個別に処理済み、ffmpeg変換の対象外
      }
      const dsdMatch = /^dsd(\d+)$/.exec(format);
      const av1Match = /^(av1|hevc-hdr10|passthrough)-(.+)$/.exec(format);
      const hiresMatch = /^(dxd352|pcm\d+_\d+)$/.exec(format);
      const outputPath = dsdMatch ? `${outputFolder}/${baseName(f.path)}.dsd${dsdMatch[1]}.dsf` : hiresMatch ? `${outputFolder}/${baseName(f.path)}.${format}.wav` : av1Match ? `${outputFolder}/${baseName(f.path)}.${av1Match[1]}.${av1Match[2]}` : `${outputFolder}/${baseName(f.path)}.${format}`;
      jobs.push({ outputPath, f, format, codecArgs });
    }
  }

  const tasks = jobs.map(({ outputPath, f, format, codecArgs: baseCodecArgs }) => async () => {
    let codecArgs = baseCodecArgs;
    // (ジョブ生成ループ内の定数はここでは見えないため、formatから再度判定する)
    const dsdMatch = /^dsd(\d+)$/.exec(format);
    const hiresMatch = /^(dxd352|pcm\d+_\d+)$/.exec(format);
    log(`変換中: ${f.path} -> ${outputPath}`);
    await logSourceTraits(f);
    try {
      // 自動/最高品質モードのビットレートは、元ファイル自身のビットレートを
      // 超えても画質は上がらない(尺が極端に短いと容量逆算が数Gbpsになる、
      // 実機で「10003421 kbps」を確認)ため、元のビットレートで頭打ちにする。
      let effectiveBitrateKbps = bitrateKbps;
      const isVideoJob = format in VIDEO_CODEC_ARGS;
      const fillingDisc = document.getElementById("fill-disc").checked;
      if (fillingDisc && isVideoJob && format !== "passthrough-mkv" && !/^hevc-hdr10/.test(format)) {
        // 「ディスクいっぱいに収める」動画(2026-09-24): 拡大すると画素が増えるので、元ファイルのビットレートでは
        // 頭打ちにしない。代わりに、同時に作る動画の本数で分け、音声(AAC 320kbps)とコンテナの余裕を差し引き、
        // 画質がそれ以上上がらない規格上限(フルHD 40Mbps、4K 100Mbps)で止める。
        const res = await resolveResolutionSetting(f);
        const is4k = res ? res.width * res.height > 1920 * 1080 : false;
        const videoFormatsCount = Math.max(1, document.querySelectorAll('input[name="video-format"]:checked').length);
        const AUDIO_KBPS = 320;
        let videoKbps = Math.floor((bitrateKbps / videoFormatsCount) * 0.98) - AUDIO_KBPS;
        const ceiling = is4k ? 100000 : 40000;
        if (videoKbps > ceiling) {
          log(`映像ビットレートを${ceiling / 1000}Mbpsで止めました(${is4k ? "Ultra HD Blu-ray" : "Blu-ray"}規格の上限で、これ以上は画質が上がりません)。ディスクには空き容量が残ります。 / Video bitrate capped at ${ceiling / 1000} Mbps (the ${is4k ? "Ultra HD Blu-ray" : "Blu-ray"} limit; quality does not improve beyond it), so some space is left on the disc.`);
          videoKbps = ceiling;
        }
        if (videoKbps < 500) {
          log(`⚠ 映像ビットレートが${videoKbps}kbpsと非常に低くなります(長さに対してディスクが小さすぎます)。 / Very low video bitrate for this length.`);
        }
        effectiveBitrateKbps = Math.max(100, videoKbps);
        codecArgs = [...codecArgs, "-b:a", `${AUDIO_KBPS}k`];
        log(`ディスクいっぱい: 映像 ${effectiveBitrateKbps} kbps + 音声 ${AUDIO_KBPS} kbps(${res ? `${res.width}×${res.height}` : "元の解像度 / original"}) / Fill disc: video ${effectiveBitrateKbps} kbps + audio ${AUDIO_KBPS} kbps`);
      } else if ((mode === "auto" || mode === "max_quality") && format !== "wav" && format !== "flac") {
        try {
          const srcInfo = await invoke("probe_media", { path: f.path });
          if (srcInfo.bit_rate) {
            const srcKbps = Math.floor(srcInfo.bit_rate / 1000);
            if (srcKbps > 0 && srcKbps < effectiveBitrateKbps) {
              log(`ビットレートを元ファイルの${srcKbps} kbpsに制限しました(${effectiveBitrateKbps} kbpsは元より高く無意味なため)。 / Capped bitrate at the source's ${srcKbps} kbps.`);
              effectiveBitrateKbps = srcKbps;
            }
          }
        } catch (e) {
          log(`警告: 元ファイルのビットレート取得に失敗(${f.path}): ${e}`);
        }
      }
      // DSDは非常に大きいため、出力サイズの見積もりを事前に表示し、選択中のディスクに収まらなければ警告する。
      if (dsdMatch) {
        try {
          const info = await invoke("probe_media", { path: f.path });
          const bytes = await invoke("estimate_dsd_size", { multiplier: parseInt(dsdMatch[1], 10), channels: 2, durationSecs: info.duration_secs });
          const gb = (bytes / 1e9).toFixed(2);
          const caps = { cd700: 0.686, dvd47: 4.6, dvd_dl85: 8.3, bd25: 24.5, bd50: 49, bd100: 98, bd128: 125 };
          const smallest = checkedValues("disc-type").map((d) => caps[d]).filter((c) => c).sort((a, b) => a - b)[0];
          const over = smallest && bytes / 1e9 > smallest;
          log(`DSDの推定サイズ:  GB / Estimated DSD size:  GB` + (over ? ` — ⚠ 選択したディスク(約GB)に収まりません / exceeds the selected disc (~ GB)` : ""));
        } catch (e) {
          // 見積もりは参考情報のため失敗しても続行する
        }
      }
      const isVideo = format in VIDEO_CODEC_ARGS;
      // 再生規格の上限(7.5)に収める。
      const spec = selectedPlaybackSpec();
      if (spec) {
        if (dsdMatch && spec !== PLAYBACK_SPECS.pc) {
          log(`⚠ DSD(DSF)は${spec.label}の再生規格外です(PC等のDSD対応プレーヤー向け)。 / DSD (DSF) is outside the ${spec.label} standard.`);
        }
        if (hiresMatch) {
          const m = /^(?:dxd(\d+)|pcm(\d+)_(\d+))$/.exec(format);
          const khz = parseInt(m[1] ?? m[2], 10);
          const bits = format === "dxd352" ? 24 : parseInt(m[3], 10);
          if (khz * 1000 > spec.hz + 1000 || bits > spec.bits) {
            log(`スキップ: ${format}は${spec.label}の上限(${spec.hz / 1000}kHz/${spec.bits}bit)を超えます。 / Skipped: ${format} exceeds the ${spec.label} limit.`);
            return null;
          }
        }
        const lim = CODEC_LIMITS[format];
        const caps = [isVideo ? spec.videoKbps : spec.audioKbps, isVideo ? null : lim?.kbps].filter((v) => v !== null && v !== undefined);
        if (caps.length > 0 && effectiveBitrateKbps > Math.min(...caps)) {
          effectiveBitrateKbps = Math.min(...caps);
          log(`${spec.label}の上限に合わせてビットレートを${effectiveBitrateKbps} kbpsにしました。 / Bitrate capped at ${effectiveBitrateKbps} kbps for ${spec.label}.`);
        }
        if (!dsdMatch && !hiresMatch && format !== "passthrough-mkv" && !/^hevc-hdr10/.test(format)) {
          let srcHz = null;
          try {
            srcHz = (await invoke("probe_media", { path: f.path })).audio_sample_rate ?? null;
          } catch (e) {
            // 取得できなければ上限値をそのまま使う
          }
          const hzCap = Math.min(isVideo ? spec.videoAudioHz : spec.hz, lim?.hz ?? Infinity);
          if (srcHz === null || srcHz > hzCap) {
            codecArgs = [...codecArgs, "-ar", String(hzCap)];
          }
          if (spec.bits <= 16 && format === "flac") {
            codecArgs = [...codecArgs, "-sample_fmt", "s16"];
          } else if (spec.bits >= 24 && format === "wav") {
            codecArgs = ["-c:a", "pcm_s24le", ...codecArgs.slice(2)];
          }
        }
      }
      // チャンネル構成の変換(3.7、2026-09-28新設): 音声(DSD以外)にのみ適用。
      let effectiveInputPath = f.path;
      if (!isVideo && !dsdMatch) {
        const channelTarget = document.getElementById("channel-layout").value;
        if (channelTarget !== "keep") {
          try {
            const srcInfo = await invoke("probe_media", { path: f.path });
            const sourceChannels = srcInfo.audio_channels ?? 2;
            const isSurroundTarget = /^surround/.test(channelTarget);
            // AIアップミックス(HT-Demucsによる実音源分離、2026-09-28新設): モノ/ステレオ→
            // 5.1ch/7.1chのときだけ、モデルが用意されていれば単純panフィルタより優先して使う
            // (未設定/失敗時は既存のpanフィルタへフォールバックする)。
            let usedAiUpmix = false;
            if (isSurroundTarget && sourceChannels <= 2 && (await invoke("ai_upmix_available"))) {
              const aiOutPath = `${outputFolder}/${baseName(f.path)}.ai-upmix.wav`;
              try {
                log(`AIアップミックス中(HT-Demucsで実音源分離): ${f.path} ... / AI upmixing (real source separation via HT-Demucs): ${f.path} ...`);
                await invoke("ai_upmix_convert", { inputPath: f.path, outputPath: aiOutPath, target: channelTarget });
                effectiveInputPath = aiOutPath;
                usedAiUpmix = true;
              } catch (e) {
                log(`警告: AIアップミックスに失敗したため、簡易疑似サラウンドにフォールバックします(${f.path}): ${e}`);
              }
            }
            if (!usedAiUpmix) {
              const extraArgs = await invoke("channel_layout_args", { sourceChannels, target: channelTarget });
              if (extraArgs.length > 0) {
                codecArgs = [...codecArgs, ...extraArgs];
              } else if (isSurroundTarget && sourceChannels > 2) {
                log(`${f.path}: 元から${sourceChannels}chあるため、5.1ch/7.1chへの疑似変換は行いません。 / Already ${sourceChannels}ch — skipping the pseudo-surround conversion.`);
              }
            }
          } catch (e) {
            log(`警告: チャンネル構成の変換に失敗(${f.path}): ${e}`);
          }
        }
      }
      const resolution = isVideo ? await resolveResolutionSetting(f) : null;
      const fps = isVideo ? await resolveFpsSetting(f) : null;
      await invoke("convert_media", {
        job: {
          input_path: effectiveInputPath,
          output_path: outputPath,
          codec_args: codecArgs,
          bitrate: format === "wav" || format === "flac" || hiresMatch || dsdMatch ? null : { [mode === "auto" || mode === "max_quality" ? "auto_max_for_capacity" : "fixed"]: effectiveBitrateKbps },
          // 切り出し(高速)があればそれを優先し、開始位置へ直接シークして必要な長さだけを処理する。
          trim: f.extract ? { start_secs: f.extract.startSecs, duration_secs: f.extract.endSecs - f.extract.startSecs } : null,
          cut_ranges: !f.extract && f.cutRanges.length > 0 ? f.cutRanges.map((r) => ({ start_secs: r.startSecs, end_secs: r.endSecs })) : null,
          frame_accurate: f.frameAccurate,
          resolution,
          fps,
          dsd_rate: dsdMatch ? parseInt(dsdMatch[1], 10) : null,
          mkv_keep_all_tracks: isVideo && /mkv$/.test(format) ? document.getElementById("mkv-keep-all").checked : null,
          extra_tracks:
            isVideo && /mkv$/.test(format) && f === sourceFiles[0]
              ? mkvExtraTracks.map((t) => ({ path: t.path, kind: t.kind, language: (t.language || "").trim() || null, title: (t.title || "").trim() || null }))
              : [],
          dop_wav_bits: dsdMatch && document.getElementById("dsd-dop-wav").checked ? parseInt(document.getElementById("dsd-dop-bits").value, 10) : null,
          audio_bwe:
            !isVideo && document.getElementById("audio-bwe").checked
              ? {
                  cutoff_hz: parseFloat(document.getElementById("audio-bwe-cutoff").value) || null,
                  speech: document.getElementById("audio-bwe-speech").checked,
                }
              : null,
          ai_upscale: isVideo && format !== "passthrough-mkv" ? aiJobOptions() : null,
          ai_denoise: document.getElementById("ai-denoise").checked && format !== "passthrough-mkv" ? { mix: parseFloat(document.getElementById("ai-denoise-mix").value) } : null,
        },
      });
      log(`完了: ${outputPath}`);
      return outputPath;
    } catch (e) {
      log(`エラー: ${e}`);
      return null;
    }
  });

  const results = await runWithConcurrencyLimit(tasks, conversionConcurrency());
  conversionFailures += results.filter((p) => p === null).length;
  return results.filter((p) => p !== null);
}

// ── AI超解像・フレーム補間(4.3)の設定と進捗 ─────────────────
function aiUpscaleOptions() {
  const fps = parseInt(document.getElementById("ai-target-fps").value, 10);
  return {
    model: document.getElementById("ai-upscale-model").value,
    scale: parseInt(document.getElementById("ai-upscale-scale").value, 10),
    backend: document.getElementById("ai-upscale-backend").value,
    content: document.getElementById("ai-upscale-content").value,
    deinterlace: document.getElementById("ai-deinterlace").value,
    crop_bars: document.getElementById("ai-crop-bars").checked,
    skip_static: document.getElementById("ai-skip-static").checked,
    target_fps: fps > 0 ? fps : null,
  };
}

// 超解像がオンなら超解像(+目標fps)、オフでも目標fpsが選ばれていればフレーム補間(RIFE)だけを行う。
function aiJobOptions() {
  const o = aiUpscaleOptions();
  if (document.getElementById("ai-upscale").checked) return o;
  return o.target_fps ? { ...o, interpolate_only: true } : null;
}

function fmtDur(secs) {
  if (secs == null || !isFinite(secs)) return "?";
  const s = Math.max(0, Math.round(secs));
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (d > 0) return `${d}日${h}時間 / ${d}d ${h}h`;
  if (h > 0) return `${h}時間${m}分 / ${h}h ${m}m`;
  if (m > 0) return `${m}分${s % 60}秒 / ${m}m ${s % 60}s`;
  return `${s}秒 / ${s}s`;
}

function setAiRunning(running) {
  document.getElementById("ai-cancel-btn").disabled = !running;
  const bar = document.getElementById("ai-progress");
  bar.hidden = !running;
  if (!running) bar.value = 0;
}

window.__TAURI__.event.listen("make-disk-progress", (ev) => {
  const { kind, payload: p } = ev.payload || {};
  if (kind !== "ai-progress" || !p) return;
  if (p.stage === "info") {
    log(p.message);
    return;
  }
  if (p.stage === "upscale" || p.stage === "interpolate") {
    setAiRunning(true);
    const bar = document.getElementById("ai-progress");
    bar.max = p.total || 100;
    bar.value = p.done || 0;
    const label = p.stage === "upscale" ? "AI超解像 / Upscaling" : `フレーム補間(${p.factor}倍) / Interpolating (x${p.factor})`;
    const detail = p.stage === "upscale" ? `(CPU ${p.cpu} / GPU ${p.gpu} / 単色 ${p.flat} / 再利用 ${p.repeat})` : "";
    document.getElementById("ai-status").textContent = `${label}: ${p.done}/${p.total} コマ / frames、経過 ${fmtDur(p.elapsed_secs)}、残り約 ${fmtDur(p.eta_secs)}${detail}`;
  }
});

document.getElementById("ai-cancel-btn").addEventListener("click", async () => {
  await invoke("cancel_conversion");
  log("中止を依頼しました。次の区切りで止まり、途中経過は残ります。 / Cancel requested; it stops at the next checkpoint and progress is kept.");
});

document.getElementById("ai-bench-btn").addEventListener("click", async () => {
  const btn = document.getElementById("ai-bench-btn");
  btn.disabled = true;
  document.getElementById("ai-status").textContent = "測定中(1分ほどかかります)… / Measuring (about a minute)…";
  try {
    const b = await invoke("ai_hw_benchmark", { force: true });
    document.getElementById("ai-status").textContent = `${b.message_ja}\n${b.message_en}`;
  } catch (e) {
    document.getElementById("ai-status").textContent = `測定に失敗しました / Benchmark failed: ${e}`;
  } finally {
    btn.disabled = false;
  }
});

function selectedTargetSize() {
  const r = document.querySelector('input[name="upconv-res"]:checked');
  const sel = document.getElementById("resolution-preset").value;
  const v = /^\d+x\d+$/.test(sel) ? sel : r ? r.value : null;
  if (!v) return {};
  const [w, h] = v.split("x").map((n) => parseInt(n, 10));
  return { target_w: w, target_h: h };
}

document.getElementById("ai-estimate-btn").addEventListener("click", async () => {
  const f = sourceFiles[0];
  const status = document.getElementById("ai-status");
  if (!f) {
    status.textContent = "先に動画ファイルを追加してください。 / Add a video file first.";
    return;
  }
  status.textContent = "解析中… / Analyzing…";
  try {
    const t = selectedTargetSize();
    const e = await invoke("ai_estimate", {
      path: f.path,
      startSecs: f.extract ? f.extract.startSecs : null,
      durationSecs: f.extract ? f.extract.endSecs - f.extract.startSecs : null,
      options: aiUpscaleOptions(),
      targetW: t.target_w ?? null,
      targetH: t.target_h ?? null,
      workDir: outputFolder || ".",
    });
    const a = e.analysis;
    const free = e.free_bytes != null ? `${(e.free_bytes / 1e9).toFixed(1)}GB` : "不明 / unknown";
    const lines = [
      `${a.width}×${a.height}、${a.frames}コマ → ${e.out_w}×${e.out_h}(${e.scale}倍、モデル ${e.model})`,
      `所要時間の目安(最悪): ${fmtDur(e.eta_secs)}${e.eta_secs == null ? "(先に「CPU/GPUを測定」を実行) / (run the benchmark first)" : ""}`,
      ...(e.rife_factor ? [`フレーム補間(${e.rife_factor}倍、${e.rife_method === "gpu" ? "GPU" : "CPU"}): 補間コマ1枚あたり約${(e.rife_secs_per_frame ?? 0).toFixed(1)}秒 → ${fmtDur(e.rife_eta_secs)}`] : []),
      ...(e.rife_factor ? [`合計の目安: ${fmtDur(e.total_eta_secs)}`] : []),
      `作業用の一時容量: 約${(e.temp_bytes / 1e9).toFixed(1)}GB(空き ${free})`,
      ...(a.notes_ja || []),
      ...(e.warnings_ja || []).map((w) => `⚠ ${w}`),
      ...(e.warnings_en || []).map((w) => `⚠ ${w}`),
    ];
    status.textContent = lines.join("\n");
  } catch (err) {
    status.textContent = `下調べに失敗しました / Estimate failed: ${err}`;
  }
});

// 見積もり(所要時間・空き容量・収まるか、2026-09-28新設): 動画のAI超解像(4.3)専用だった
// 「下調べ」「収まるか予測」を、CD取り込み・通常の音声変換にも使えるようにする
// (ユーザー指摘: 「下調べ」「収まるか予測」がビデオのみで音声/CDに対応していなかった)。
document.getElementById("estimate-audio-fit-btn").addEventListener("click", async () => {
  const box = document.getElementById("estimate-audio-fit-result");
  if (sourceFiles.length === 0) {
    box.textContent = "先にソースファイルを追加してください。 / Add source files first.";
    return;
  }
  const discTypes = checkedValues("disc-type");
  if (discTypes.length === 0) {
    box.textContent = "6.でディスク種別を1つ以上選択してください。 / Pick at least one disc type in 6.";
    return;
  }
  box.textContent = "見積もり中… / Estimating…";
  try {
    let totalDuration = 0;
    for (const f of sourceFiles) totalDuration += await effectiveDurationSecs(f);

    const mode = bitrateMode();
    const useLossless = mode === "max_quality" && document.querySelectorAll('input[name="audio-format"]:checked').length === 0;
    let bitrateKbps = parseInt(document.getElementById("bitrate-fixed").value, 10) || 0;
    if (mode === "auto" || mode === "max_quality") {
      bitrateKbps = await computeAutoBitrateKbps(discTypes);
    }

    const lines = [`合計時間 / Total duration: ${fmtDur(totalDuration)}`];
    for (const discType of discTypes) {
      const est = useLossless
        ? await invoke("estimate_lossless_audio_fit", { disc: discType, totalDurationSecs: totalDuration, reservedBytes: 50 * 1024 * 1024 })
        : await invoke("estimate_bitrate_fit", { disc: discType, totalDurationSecs: totalDuration, bitrateKbps, reservedBytes: 50 * 1024 * 1024 });
      const mark = est.fits ? "◎" : "×";
      const need = (est.required_bytes / 1e9).toFixed(2);
      const usable = (est.usable_bytes / 1e9).toFixed(2);
      lines.push(
        `${mark} [${discType}]${useLossless ? " ロスレスWAV" : ` ${bitrateKbps}kbps`}: 必要 約${need}GB / 実用容量 約${usable}GB` +
          (est.fits ? "" : ` — 収まりません(このディスクなら最大${fmtDur(est.max_fitting_duration_secs)}まで) / doesn't fit (max ${fmtDur(est.max_fitting_duration_secs)} on this disc)`)
      );
    }

    if (outputFolder) {
      const free = await invoke("free_space_bytes", { path: outputFolder });
      if (free != null) {
        lines.push(`出力先の空き容量 / Free space at output folder: 約${(free / 1e9).toFixed(1)}GB`);
      }
    }
    box.textContent = lines.join("\n");
  } catch (e) {
    box.textContent = `見積もりに失敗しました / Estimate failed: ${e}`;
  }
});

document.getElementById("ai-denoise-mix").addEventListener("input", (e) => {
  document.getElementById("ai-denoise-mix-label").textContent = e.target.value;
});

/** ディスク変換の方向に応じて、選べる解像度プリセットを絞り込む(2026-09-19新設)。
 * BD→DVD: 720x480 / 1920x1080、DVD→BD: 1920x1080 / 3840x2160。
 * 「無指定」「カスタム」「AI最適化」は常に選べる。 */
const DIRECTION_RESOLUTIONS = {
  any: null,
  bd_to_dvd: ["720x480", "720x576", "1920x1080"],
  dvd_to_bd: ["1920x1080", "3840x2160"],
};
function applyDiscDirection(direction) {
  const allowed = DIRECTION_RESOLUTIONS[direction];
  const select = document.getElementById("resolution-preset");
  const fixedValues = ["unspecified", "custom", "ai"];
  for (const opt of select.options) {
    const enabled = allowed === null || fixedValues.includes(opt.value) || allowed.includes(opt.value);
    opt.hidden = !enabled;
    opt.disabled = !enabled;
  }
  if (allowed !== null && select.selectedOptions[0]?.disabled) {
    select.value = allowed[0];
    select.dispatchEvent(new Event("change"));
  }
}
document.getElementById("disc-direction").addEventListener("change", (e) => applyDiscDirection(e.target.value));

document.getElementById("resolution-preset").addEventListener("change", (e) => {
  document.getElementById("resolution-custom-inputs").hidden = e.target.value !== "custom";
  document.getElementById("resolution-ai-hint").hidden = e.target.value !== "ai";
});
document.getElementById("fps-preset").addEventListener("change", (e) => {
  document.getElementById("fps-custom-value").hidden = e.target.value !== "custom";
  document.getElementById("fps-ai-hint").hidden = e.target.value !== "ai";
});

document.getElementById("rebind-pdfs-btn").addEventListener("click", async () => {
  logEl.textContent = "";
  const pdfFiles = sourceFiles.filter((f) => f.path.toLowerCase().endsWith(".pdf"));
  if (pdfFiles.length === 0) {
    log("エラー: PDFファイルをソースに追加してください。 / Error: add at least one PDF to the source list.");
    return;
  }
  if (!requireValidOutputFolder()) return;
  log(`${pdfFiles.length}件のPDFの綴じ方向を変換中... / Converting binding direction for ${pdfFiles.length} PDF(s)...`);
  const results = await invoke("rebind_pdfs", { pdfPaths: pdfFiles.map((f) => f.path), outputDir: outputFolder });
  results.forEach((result, i) => {
    if (result && typeof result === "object" && "Ok" in result) {
      log(`完了: ${pdfFiles[i].path} -> ${result.Ok}`);
    } else {
      log(`エラー(${pdfFiles[i].path}): ${result && result.Err ? result.Err : result}`);
    }
  });
  log("すべての処理が完了しました。");
});

document.getElementById("concat-btn").addEventListener("click", async () => {
  logEl.textContent = "";
  const targets = sourceFiles.filter((f) => !f.path.toLowerCase().endsWith(".pdf"));
  if (targets.length < 2) {
    log("エラー: 結合には2つ以上の音声/動画ファイルが必要です。 / Error: concatenation needs at least 2 audio/video files.");
    return;
  }
  if (!requireValidOutputFolder()) return;
  const videoExts = Object.keys(VIDEO_CODEC_ARGS);
  const hasVideo = targets.some((f) => videoExts.some((ext) => f.path.toLowerCase().endsWith(`.${ext}`)));
  const outputPath = `${outputFolder}/composite-output.${hasVideo ? "mp4" : "mp3"}`;
  log(`${targets.length}件のファイルをソース順に結合中... / Concatenating ${targets.length} file(s) in list order...`);
  try {
    await invoke("concat_media_files", { inputPaths: targets.map((f) => f.path), outputPath, hasVideo });
    log(`完了: ${outputPath}`);
  } catch (e) {
    log(`エラー: ${e}`);
  }
});

/** 動画/音声の分割(2026-09-16新設)。等間隔かサイズ指定で複数ファイルに
 * 分割する——既存の`convert_media`の`trim`(開始+長さ)をそのまま使うため、
 * 新しい抽出処理は不要。「あまり」(サイズ指定分割で割り切れない最後の
 * 区間)だけディスク容量いっぱいのビットレートへ自動調整することで、
 * ユーザー指示「あまりは、DISKいっぱいにビットレートを自動変更して
 * 自動編集して」に対応する。 */
// 分割数の入力欄: 1は「分けない」(0)と同じなので、矢印で0の次は2、2の前は0へ飛ばす。
{
  const countEl = document.getElementById("split-equal-count");
  let prev = parseInt(countEl.value, 10);
  countEl.addEventListener("change", () => {
    const v = parseInt(countEl.value, 10);
    if (v === 1) countEl.value = String(prev === 0 ? 2 : 0);
    prev = parseInt(countEl.value, 10);
  });
}

document.getElementById("split-btn").addEventListener("click", async () => {
  logEl.textContent = "";
  const target = sourceFiles.find((f) => !f.path.toLowerCase().endsWith(".pdf"));
  if (!target) {
    log("エラー: 分割対象の音声/動画ファイルが見つかりません。 / Error: no audio/video file found to split.");
    return;
  }
  if (!requireValidOutputFolder()) return;

  const totalSecs = await effectiveDurationSecs(target);
  if (totalSecs <= 0) {
    log("エラー: 対象ファイルの尺を取得できませんでした。 / Error: could not determine the file's duration.");
    return;
  }

  const splitMode = document.querySelector('input[name="split-mode"]:checked').value;
  const fixedBitrateKbps = parseInt(document.getElementById("bitrate-fixed").value, 10) || 192;
  let segments;
  let nominalSegmentSecs = null;
  if (splitMode === "equal") {
    const count = parseInt(document.getElementById("split-equal-count").value, 10);
    if (count === 0 || count === 1) {
      log("分割しません(0個・1個は「分けない」と同じです)。2以上を指定すると分割します。 / Not splitting (0 or 1 part means no split). Enter 2 or more to split.");
      return;
    }
    if (!Number.isInteger(count) || count < 2) {
      log("エラー: 分割数は0(分割しない)または2以上の整数にしてください。 / Error: enter 0 (no split) or an integer of 2 or more.");
      return;
    }
    segments = await invoke("calc_equal_interval_segments", { totalSecs, segmentCount: count });
  } else {
    const targetMb = parseFloat(document.getElementById("split-size-mb").value);
    if (!targetMb || targetMb <= 0) {
      log("エラー: 分割サイズ(MB)を入力してください。 / Error: enter a split size in MB.");
      return;
    }
    // 現在のビットレート設定(section 7の固定値)から、目標サイズに相当する区間長(秒)を逆算する。
    nominalSegmentSecs = (targetMb * 1024 * 1024 * 8) / (fixedBitrateKbps * 1000);
    segments = await invoke("calc_fixed_length_segments", { totalSecs, segmentSecs: nominalSegmentSecs });
  }

  if (segments.length === 1) {
    log("分割する必要がありません: ファイル全体が指定した大きさ(または1区間)に収まるため、分割されません。 / Nothing to split: the whole file fits in a single part.");
    return;
  }
  if (segments.length === 0) {
    log("エラー: 分割区間を計算できませんでした。 / Error: could not compute split segments.");
    return;
  }

  const discTypes = checkedValues("disc-type");
  const autoFitRemainder = document.getElementById("split-remainder-disk-fit").checked;
  const ext = target.path.split(".").pop();
  const base = baseName(target.path);

  for (let i = 0; i < segments.length; i++) {
    const seg = segments[i];
    const isRemainder = nominalSegmentSecs !== null && i === segments.length - 1 && seg.duration_secs < nominalSegmentSecs - 0.01;
    let bitrateKbpsForSegment = fixedBitrateKbps;
    if (isRemainder && autoFitRemainder && discTypes.length > 0) {
      bitrateKbpsForSegment = await invoke("calc_auto_bitrate_kbps", { disc: discTypes[0], totalDurationSecs: seg.duration_secs, reservedBytes: 50 * 1024 * 1024 });
      log(`あまり区間(${i + 1}/${segments.length}、${discTypes[0]})をディスクいっぱいのビットレート(${bitrateKbpsForSegment} kbps)に自動調整します。 / Auto-fitting the remainder segment (${i + 1}/${segments.length}, ${discTypes[0]}) to ${bitrateKbpsForSegment} kbps to fill the disc.`);
    }
    const outputPath = `${outputFolder}/${base}-part${String(i + 1).padStart(3, "0")}.${ext}`;
    log(`分割中(${i + 1}/${segments.length}): ${outputPath} ...`);
    try {
      await invoke("convert_media", {
        job: {
          input_path: target.path,
          output_path: outputPath,
          codec_args: [],
          bitrate: { fixed: bitrateKbpsForSegment },
          trim: { start_secs: seg.start_secs, duration_secs: seg.duration_secs },
          cut_ranges: null,
          frame_accurate: false,
        },
      });
      log(`完了: ${outputPath}`);
    } catch (e) {
      log(`エラー: ${e}`);
    }
  }
  log("すべての処理が完了しました。");
});

document.getElementById("run-btn").addEventListener("click", async () => {
  logEl.textContent = "";
  if (sourceFiles.length === 0) {
    log("エラー: ソースファイルを追加してください。");
    return;
  }
  if (!requireValidOutputFolder()) return;
  for (const f of sourceFiles) {
    if (f.extract && f.cutRanges.length > 0) {
      log(`注意: ${f.path} は切り出しが設定されているため、カット区間(${f.cutRanges.length}件)は使いません。 / Note: an extract range is set for this file, so its cut ranges are not used.`);
    }
  }

  // PDF見開き変換(2026-09-16新設)。音声/動画とは独立して、ソースに
  // 含まれるPDFがあれば見開き画像として先に書き出す。
  const pdfFiles = sourceFiles.filter((f) => f.path.toLowerCase().endsWith(".pdf"));
  if (pdfFiles.length > 0) {
    const binding = document.querySelector('input[name="pdf-binding"]:checked').value;
    for (const f of pdfFiles) {
      const baseName = f.path.replace(/\\/g, "/").split("/").pop().replace(/\.pdf$/i, "");
      const pdfOutDir = `${outputFolder}/pdf-spreads-${baseName}`;
      log(`PDF見開き変換中: ${f.path} ... / Converting PDF spreads: ${f.path} ...`);
      try {
        const outputs = await invoke("convert_pdf_to_spreads", { pdfPath: f.path, outputDir: pdfOutDir, binding, maxDimension: 3840 });
        log(`  ${outputs.length}枚の見開き画像を書き出しました: ${pdfOutDir} / wrote ${outputs.length} spread image(s) to: ${pdfOutDir}`);
      } catch (e) {
        log(`  エラー: ${e}`);
      }
    }
  }

  // 「そのまま含める」ファイル(2026-09-27新設): 音声/動画/PDFのように
  // make-diskが変換できる形式とは限らない、任意のファイルを無変換のまま
  // 出力フォルダへコピーする。これにより、色々な形式のファイルを1つに
  // まとめて、まとめて1枚のISO/ディスクに書き込める(音声/動画への変換とは
  // 独立で併用可能)。
  const asIsFiles = sourceFiles.filter((f) => f.includeAsIs);
  if (asIsFiles.length > 0) {
    for (const f of asIsFiles) {
      log(`そのまま含める(無変換でコピー): ${f.path} ... / Including as-is (copying, no conversion): ${f.path} ...`);
      try {
        const dest = await invoke("copy_source_as_is", { sourcePath: f.path, outputFolder });
        log(`  コピーしました: ${dest} / copied to: ${dest}`);
      } catch (e) {
        log(`  エラー: ${e}`);
      }
    }
  }

  // ソースが全てPDF(+そのまま含めるファイル)で、音声/動画の変換対象が
  // 無い場合は、PDF見開き変換・そのままコピーだけで完了とする——音声/動画
  // フォーマット必須のバリデーションを誤って適用しないようにする。
  if (pdfFiles.length + asIsFiles.filter((f) => !pdfFiles.includes(f)).length === sourceFiles.length) {
    if (asIsFiles.length > 0 && (document.getElementById("output-iso").checked || checkedValues("disc-type").length > 0)) {
      await createIsoAndMaybeBurn();
    } else {
      log("すべての処理が完了しました。");
    }
    return;
  }

  let audioFormats = checkedValues("audio-format");
  let videoFormats = checkedValues("video-format");
  const discTypes = checkedValues("disc-type");
  let wantIso = document.getElementById("output-iso").checked;

  let mode = bitrateMode();

  // 「最高音質・最高画質」モード(2026-09-16新設): フォーマット未選択でも
  // 実行できる——音声フォーマットが1つも選ばれていなければロスレスWAVを
  // 自動選択し、常にISO化する(ユーザー指示「音声や画像フォーマットを
  // 選択しない場合...ロスレスのWAVに変換後ISO変換を同時に行なう」)。
  if (mode === "max_quality") {
    if (discTypes.length === 0) {
      log("エラー: 最高音質・最高画質モードにはディスク種別を1つ以上選択してください。 / Error: select at least one disc type for maximum-quality mode.");
      return;
    }
    if (audioFormats.length === 0 && videoFormats.length === 0) {
      audioFormats = ["wav"];
      log("音声/動画フォーマットが未選択のため、ロスレスWAVを自動選択しました。 / No audio/video format selected — automatically using lossless WAV.");
    }
    wantIso = true;
  }

  if (audioFormats.length === 0 && videoFormats.length === 0) {
    log("エラー: 音声または動画フォーマットを1つ以上選択してください。");
    return;
  }

  let bitrateKbps = parseInt(document.getElementById("bitrate-fixed").value, 10);

  if (mode === "auto" || mode === "max_quality") {
    if (discTypes.length === 0) {
      log("エラー: 自動ビットレートにはディスク種別を1つ以上選択してください。");
      return;
    }
    log("選択したディスクのうち最小容量に合わせて最大ビットレートを算出中...");
    bitrateKbps = await computeAutoBitrateKbps(discTypes);
    log(`自動算出ビットレート: ${bitrateKbps} kbps`);

    const mediaKind = videoFormats.length > 0 ? "video" : "audio";
    const warning = await invoke("check_bitrate_quality", { bitrateKbps, kind: mediaKind });
    if (warning) {
      log(`⚠️ ${warning.message_ja} / ${warning.message_en}`);
    }
  }

  // 最高音質モードでWAV(ロスレス)を使う場合、容量から逆算した
  // 「収まる/収まらない、収まるなら何秒まで」を先に表示する
  // (ユーザー指示「必要な時間やデータサイズを自動で割り出す」)。
  if (mode === "max_quality" && audioFormats.includes("wav")) {
    let totalDuration = 0;
    for (const f of sourceFiles) {
      totalDuration += await effectiveDurationSecs(f);
    }
    for (const discType of discTypes) {
      const est = await invoke("estimate_lossless_audio_fit", {
        disc: discType,
        totalDurationSecs: totalDuration,
        reservedBytes: 50 * 1024 * 1024,
      });
      const { h, m, sec } = secsToHms(totalDuration);
      const need = `${h}時間${m}分${Math.floor(sec)}秒 / ${h}h${m}m${Math.floor(sec)}s`;
      if (est.fits) {
        log(`[${discType}] 収録時間(${need})はロスレスWAVで収まります(必要: ${(est.required_bytes / 1e6).toFixed(1)}MB / 容量: ${(est.usable_bytes / 1e6).toFixed(1)}MB)。`);
      } else {
        const fit = secsToHms(est.max_fitting_duration_secs);
        log(
          `⚠️ [${discType}] 収録時間(${need})はロスレスWAVでは収まりません(必要: ${(est.required_bytes / 1e9).toFixed(2)}GB / 容量: ${(est.usable_bytes / 1e9).toFixed(2)}GB)。` +
            `このディスクにロスレスで収まるのは最大${fit.h}時間${fit.m}分${Math.floor(fit.sec)}秒までです。 / ` +
            `Won't fit losslessly — this disc can hold at most ${fit.h}h${fit.m}m${Math.floor(fit.sec)}s of lossless audio.`
        );
      }
    }
  }

  // ── 元データのカット(2026-09-23再設計) ──────────────────
  // 「サイズでカット」「時間でカット」はYES/NOの排他で必ずどちらか一方がYES(サイズ優先)。
  // 後処理として「ディスクいっぱいに収める」「AI無音カット」をチェックで選べる。
  const cutBySize = document.querySelector('input[name="cut-by-size"]:checked').value === "yes";

  if (cutBySize) {
    const targetMb = parseFloat(document.getElementById("cut-size-mb").value);
    if (!targetMb || targetMb <= 0) {
      log("サイズ(MB)が未入力のため、サイズでのカットは行いません。 / No size (MB) entered — not cutting by size.");
    } else {
      const targetBytes = targetMb * 1024 * 1024;
      for (const f of sourceFiles) {
        if (f.cutRanges.length > 0 || f.extract) continue; // 手動編集済みは上書きしない
        try {
          const info = await invoke("probe_media", { path: f.path });
          const srcBytes = ((info.bit_rate ?? 0) / 8) * info.duration_secs;
          if (!srcBytes || !info.duration_secs) {
            log(`警告: ${f.path} のサイズを判定できないためカットしません。 / Can't determine size; not cut.`);
            continue;
          }
          if (srcBytes <= targetBytes) {
            log(`${f.path}: 元データ(${(srcBytes / 1048576).toFixed(1)}MB)は${targetMb}MB以下のためカットしません。 / Already within ${targetMb} MB.`);
            continue;
          }
          const keepSecs = (info.duration_secs * targetBytes) / srcBytes;
          f.extract = { startSecs: 0, endSecs: keepSecs }; // 先頭から必要な長さだけを高速に読み込む
          log(`サイズでカット: ${f.path} を先頭から${formatHms(keepSecs)}(約${targetMb}MB)まで残します。 / Cut by size: keeping the first ${formatHms(keepSecs)} (~${targetMb} MB).`);
        } catch (e) {
          log(`警告: ${f.path} の情報取得に失敗: ${e}`);
        }
      }
    }
  } else {
    const targetSecs = hmsToSecs(document.getElementById("cut-time-h").value, document.getElementById("cut-time-m").value, document.getElementById("cut-time-s").value);
    if (!targetSecs || targetSecs <= 0) {
      log("時間(時分秒)が未入力のため、時間でのカットは行いません。 / No time entered — not cutting by time.");
    } else {
      for (const f of sourceFiles) {
        if (f.cutRanges.length === 0 && !f.extract) {
          f.extract = { startSecs: 0, endSecs: targetSecs }; // 先頭から必要な長さだけを高速に読み込む
        }
      }
      log(`時間でカット: 先頭から${formatHms(targetSecs)}までを残します(手動編集済みのファイルは変更しません)。 / Cut by time: keeping the first ${formatHms(targetSecs)} (manually edited files untouched).`);
    }
  }

  if (document.getElementById("ai-auto-cut").checked) {
    // 「AI判断」の正直な開示: 実際にはffmpegの音量ベースの無音検出
    // (silencedetect)による近似であり、意味的なシーン解析ではない。
    for (const f of sourceFiles) {
      log(`無音区間を検出中: ${f.path} ... / Detecting silence in: ${f.path} ...`);
      try {
        const silences = await invoke("detect_silence_ranges", { path: f.path, silenceThresholdDb: -30.0, minSilenceSecs: 0.5 });
        if (silences.length > 0) {
          f.cutRanges.push(...silences.map((x) => ({ startSecs: x.start_secs, endSecs: x.end_secs })));
          log(`  ${silences.length}箇所の無音区間を自動カット対象にしました。 / marked ${silences.length} silent range(s) for auto-cut.`);
        } else {
          log(`  無音区間は見つかりませんでした(カット無し)。 / no silence found (nothing to cut).`);
        }
      } catch (e) {
        log(`  エラー: ${e}`);
      }
    }
  }

  // 後処理: カット後の実効尺でディスクいっぱいのビットレートを算出する。
  if (document.getElementById("fill-disc").checked) {
    if (discTypes.length === 0) {
      log("エラー: 「ディスクいっぱいに収める」には6.でディスク種別を1つ以上選択してください。 / Error: pick a disc type in section 6 to fill the disc.");
      return;
    }
    bitrateKbps = await computeAutoBitrateKbps(discTypes);
    if (mode === "fixed") mode = "auto";
    log(`ディスクいっぱいに収めるビットレート: ${bitrateKbps} kbps(選択中で最小容量のディスク基準) / Bitrate to fill the disc: ${bitrateKbps} kbps (smallest selected disc).`);
  }
  renderFileList(); // 自動トリム/自動カットで設定した区間を8.の編集欄にも反映する

  // どの形式が選択されているかを実行前に必ず表示する(プリセットボタン等で意図せず選択されていた場合に気づけるように)。
  log(`選択中の出力形式 / Selected formats: 音声=${audioFormats.join(", ") || "なし"} / 動画=${videoFormats.join(", ") || "なし"}`);

  // 音声変換・動画変換もお互いを待たず並行して進める(2026-09-16変更、
  // 「MP4をWAVに変換しつつISO化」のような組み合わせも含め、全体として
  // 非同期・マルチスレッドに実行する)。
  conversionFailures = 0;
  const [audioOutputs, videoOutputs] = await Promise.all([
    audioFormats.length > 0 ? convertAll(audioFormats, AUDIO_CODEC_ARGS, mode, bitrateKbps) : Promise.resolve([]),
    videoFormats.length > 0 ? convertAll(videoFormats, VIDEO_CODEC_ARGS, mode, bitrateKbps) : Promise.resolve([]),
  ]);
  const convertedPaths = [...audioOutputs, ...videoOutputs];
  if (conversionFailures > 0 && (wantIso || discTypes.length > 0)) {
    warn(
      `変換に失敗したファイルが${conversionFailures}件あるため、ISO作成とディスクへの書き込みを中止しました。ログのエラー内容を確認してください。`,
      `${conversionFailures} conversion(s) failed, so ISO creation and disc burning were cancelled. Check the errors in the log.`
    );
    return;
  }

  if (wantIso || discTypes.length > 0) {
    await createIsoAndMaybeBurn();
  }

  log("すべての処理が完了しました。");
});

// ISO作成+(ディスク種別が選ばれていれば)書き込みまでを行う。出力フォルダに
// 溜まったファイル(変換済み音声/動画・PDF見開き画像・「そのまま含める」で
// コピーしたファイル)を、形式を問わずまとめて1枚のISO/ディスクにする
// (2026-09-27: 音声/動画変換が無い、そのまま含めるファイルのみの実行からも
// 呼べるよう、実行ボタンの本処理から切り出した)。
async function createIsoAndMaybeBurn() {
  const discTypes = checkedValues("disc-type");
  const isoPath = `${outputFolder}/output.iso`;
  log(`ISO作成中: ${isoPath}`);
  try {
    await invoke("create_iso", {
      sourceDir: outputFolder,
      outputIso: isoPath,
      volumeLabel: "MAKE_DISK",
    });
    log(`完了: ${isoPath}`);

    if (discTypes.length > 0) {
      const devices = await invoke("list_burn_devices");
      if (devices.length === 0) {
        log("エラー: 書き込み可能な光学ドライブが見つかりません。");
      } else {
        const speedMode = document.getElementById("write-speed-mode").value;
        const speed =
          speedMode === "fixed"
            ? { fixed: parseInt(document.getElementById("write-speed-fixed").value, 10) }
            : speedMode === "max"
              ? "max"
              : "auto";

        // 2026-09-16変更: 複数のディスク種別を選択した場合、物理
        // ドライブが複数あればドライブごとに並行して書き込む
        // (1台のドライブへ同時に2つの書き込みストリームは送れない
        // ため、同じドライブへ割り当てられた種別同士は順番に、
        // 異なるドライブへの書き込みは互いを待たずに並行実行する
        // ——ユーザー指示「書き込みも同時に行なって」への対応)。
        // ドライブより種別数が多い場合はラウンドロビンで割り当てる。
        // ISOがディスクの実用容量に収まらない種別は書き込みを試みず、日英で理由を示して飛ばす
        // (以前は容量超過でも書き込みを始めて途中で失敗していた)。
        let isoBytes = 0;
        try {
          isoBytes = await invoke("folder_size_bytes", { path: outputFolder });
        } catch (e) {
          log(`警告: 出力フォルダのサイズを確認できませんでした: ${e}`);
        }
        const burnable = [];
        for (const discType of discTypes) {
          const usable = await invoke("disc_usable_bytes", { disc: discType });
          if (isoBytes > usable) {
            log(`⚠ [${discType}] 出力が大きすぎてこのディスクには収まりません(${(isoBytes / 1e9).toFixed(2)}GB > 実用容量${(usable / 1e9).toFixed(2)}GB)。書き込みをスキップします。 / Output (${(isoBytes / 1e9).toFixed(2)} GB) exceeds this disc's usable capacity (${(usable / 1e9).toFixed(2)} GB); skipping the burn.`);
          } else {
            burnable.push(discType);
          }
        }
        const byDevice = new Map();
        burnable.forEach((discType, i) => {
          const device = devices[i % devices.length];
          if (!byDevice.has(device)) byDevice.set(device, []);
          byDevice.get(device).push(discType);
        });
        if (devices.length < burnable.length) {
          log(`ドライブが${devices.length}台のため、一部のディスク種別は同じドライブへ順番に書き込みます。`);
        }

        await Promise.all(
          Array.from(byDevice.entries()).map(async ([device, types]) => {
            for (const discType of types) {
              log(`書き込み中(${discType}): ${isoPath} -> ${device}`);
              try {
                await invoke("burn_image", { imagePath: isoPath, device, disc: discType, speed });
                log(`書き込み完了(${discType})。`);
              } catch (e) {
                log(`エラー(${discType}): ${e}`);
              }
            }
          })
        );
      }
    }
  } catch (e) {
    log(`エラー: ${e}`);
  }
}

// ── 自動アップデート確認(2026-09-16新設) ──────────────────────────
// アプリ起動時に一度だけGitHub Releasesの最新版を確認し、新しいバージョンが
// あれば日本語・英語併記のダイアログで確認してから更新する。デスクトップ
// のみ対応(`window.__TAURI__.updater`はAndroid/iOSでは登録していない
// ため未定義——モバイルはストア/APKサイドロードでの更新が前提、
// `lib.rs`のコメント参照)。
async function checkForUpdatesOnStartup() {
  const updater = window.__TAURI__.updater;
  const process = window.__TAURI__.process;
  if (!updater || !process) {
    return; // モバイル等、アップデータープラグインが登録されていない環境
  }

  let update;
  try {
    update = await updater.check();
  } catch (e) {
    console.warn("update check failed / アップデート確認に失敗しました:", e);
    return;
  }
  if (!update) {
    return; // 最新版を使用中
  }

  const message =
    `新しいバージョン ${update.version} があります。バージョンアップしますか？\n\n` +
    `A new version (${update.version}) is available. Would you like to update now?`;
  const shouldUpdate = window.confirm(message);
  if (!shouldUpdate) {
    return;
  }

  try {
    log(`アップデートをダウンロード中... / Downloading update... (v${update.version})`);
    await update.downloadAndInstall();
    log("アップデートが完了しました。アプリを再起動します。 / Update installed. Restarting the app.");
    await process.relaunch();
  } catch (e) {
    log(`アップデートに失敗しました / Update failed: ${e}`);
  }
}

checkForUpdatesOnStartup();

// 起動時にrs-ffmpeg/rs-xorrisoプラグインの状態(版・同期結果)を表示する(2026-09-19新設)。
// 同じ版が既に入っていれば再コピーしない(上書きの無駄を省く)。
(async () => {
  try {
    const plugins = await invoke("list_plugins");
    const labels = { installed: "新規インストール", updated: "更新", up_to_date: "最新(上書きスキップ)", not_bundled: "未同梱" };
    const shown = plugins.filter((p) => p.action !== "not_bundled");
    if (shown.length > 0) {
      log("プラグイン / Plugins: " + shown.map((p) => `${p.name} [${labels[p.action] ?? p.action}]`).join(", "));
    }
  } catch (e) {
    // 参考情報のため失敗しても続行する
  }
})();

// open-easy-webの配置(旧配置からの移動などがあれば知らせる)。
(async () => {
  try {
    const l = await invoke("open_easy_web_layout");
    if (l.root) log(`open-easy-web: ${l.root}`);
    for (const n of l.notes || []) log(n);
  } catch (e) {
    // 参考情報のため失敗しても続行する
  }
})();

// 起動時に、AI超解像(CPU版)が使う計算カーネル(open-cpuの検出結果)を表示する。
(async () => {
  try {
    const kernel = await invoke("ai_upscale_cpu_kernel");
    log(`AI超解像(CPU版)の計算カーネル / CPU upscaling kernel: ${kernel}`);
  } catch (e) {
    // 参考情報のため失敗しても続行する
  }
})();

// アップコンバートしてディスクいっぱいに収める(2026-09-24新設)。
// DVD 1〜2層 / Blu-ray 1〜4層のどれか1つと、フルHD/4Kを選ぶと、関係する設定をまとめて行う。
document.getElementById("upconv-apply-btn").addEventListener("click", () => {
  const disc = document.getElementById("upconv-disc").value;
  const res = document.querySelector('input[name="upconv-res"]:checked').value;
  for (const el of document.querySelectorAll('input[name="disc-type"]')) el.checked = el.value === disc;
  for (const el of document.querySelectorAll('input[name="video-format"]')) el.checked = el.value === "mkv";
  for (const el of document.querySelectorAll('input[name="audio-format"]')) el.checked = false; // 同じディスクの容量を奪わないよう音声だけの出力は外す
  document.getElementById("disc-direction").value = "any";
  applyDiscDirection("any");
  const resSel = document.getElementById("resolution-preset");
  resSel.value = res;
  resSel.dispatchEvent(new Event("change"));
  document.getElementById("fill-disc").checked = true;
  document.getElementById("output-iso").checked = true;
  const useAi = document.getElementById("upconv-ai").checked;
  document.getElementById("ai-upscale").checked = useAi;
  const wantFps = document.getElementById("upconv-fps").value;
  document.getElementById("ai-target-fps").value = wantFps;
  const discLabel = document.getElementById("upconv-disc").selectedOptions[0].textContent;
  const resLabel = res === "3840x2160" ? "4K" : "フルHD / Full HD";
  log(`設定しました: ${discLabel} に ${resLabel} で容量いっぱいに収めます(MKV・ISO作成)。出力先を選んで「実行」を押してください。 / Set: fill ${discLabel} at ${resLabel} (MKV + ISO). Choose the output folder and press Run.`);
  log(useAi ? "拡大の方法: AI超解像(4.3)で細部を補います。非常に時間がかかるので、4.3の「下調べ」で所要時間を確認してください。 / How it scales: AI super-resolution (4.3) restores detail. It is very slow — check Estimate in 4.3 first." : "拡大の方法: 補間による拡大です(AI超解像はオフ)。 / How it scales: interpolation only (AI off).");
  if (wantFps !== "0") {
    log(useAi ? "フレーム補間(RIFE)で目標のfpsにします。近似のため動きの大きい場面で破綻することがあります。 / Frame rate is raised by RIFE interpolation, which is approximate and can break on fast motion." : "AI超解像はオフですが、フレーム補間(RIFE)で新しいコマを作って目標のfpsにします(動きは滑らかになります。近似のため動きの大きい場面で破綻することがあります)。 / AI upscaling is off, but RIFE still creates new frames to reach the target fps (smoother motion; approximate, may break on fast motion).");
  }
  if (disc.startsWith("dvd")) {
    log("※DVDへのフルHD・4Kは家庭用DVDプレイヤーでは再生できない場合があります(PC・対応機器向けのデータディスク)。 / Full HD/4K on DVD may not play on set-top DVD players.");
  }
});

// 「選んだディスクに収まるか」の予測(フルHD/4K × 元のfps/60/120)。
const VERDICT_MARK = { comfortable: "◎", ok: "○", tight: "△", poor: "▲", impossible: "×" };
document.getElementById("upconv-fit-btn").addEventListener("click", async () => {
  const box = document.getElementById("upconv-fit-result");
  const f = sourceFiles[0];
  if (!f) {
    box.textContent = "先に動画ファイルを追加してください。 / Add a video file first.";
    return;
  }
  box.textContent = "予測中(数か所を抜き取って調べます)… / Predicting (sampling a few places)…";
  try {
    const info = await invoke("probe_media", { path: f.path });
    const r = await invoke("ai_fit_predict", {
      path: f.path,
      startSecs: f.extract ? f.extract.startSecs : null,
      durationSecs: f.extract ? f.extract.endSecs - f.extract.startSecs : null,
      disc: document.getElementById("upconv-disc").value,
      audioKbps: 320,
      srcFps: info.fps || 24,
    });
    const rows = r.rows.map((x) => `${VERDICT_MARK[x.verdict] || "?"} ${x.width}×${x.height} ${Number(x.fps.toFixed(2))}fps: 映像 約${(x.video_kbps / 1000).toFixed(1)}Mbps — ${x.message_ja}`);
    const sf = r.static_fraction == null ? "不明" : `${(r.static_fraction * 100).toFixed(0)}%`;
    const multi = sourceFiles.length > 1 ? "のみ(複数本の合計では計算していません)" : "";
    box.style.whiteSpace = "pre-wrap";
    box.textContent = `${rows.join("\n")}\n(対象: 1本目のファイル${multi}、長さ ${fmtDur(r.duration_secs)}、単色・静止コマの推定割合 ${sf}。目安であり、内容によって変わります。 / Estimate for the first file only; static-frame share ${sf}; a guide, content-dependent.)`;
  } catch (e) {
    box.textContent = `予測に失敗しました / Prediction failed: ${e}`;
  }
});

// SACD/Blu-rayオーディオ風プリセット(2026-09-19新設): DSD256 + 384kHz/32bit PCM + ISO出力を一括で選ぶ。
// 標準のSACD/BD-Audio規格ディスクではなく、DSF/WAVを収めたデータディスクを作る(index.htmlの注記参照)。
document.getElementById("preset-hires-disc-btn").addEventListener("click", () => {
  for (const name of ["audio-format"]) {
    for (const el of document.querySelectorAll(`input[name="${name}"]`)) {
      el.checked = el.value === "dsd256"; // DSD変換時はPCMを同時に作らない仕様
    }
  }
  document.getElementById("output-iso").checked = true;
  log("プリセットを設定しました: DSD256 + ISO(PCMは付けません)。書き込むディスク種別(6)を選び、実行してください。 / Preset applied: DSD256 + ISO (no PCM). Pick the disc types (6) and run.");
});

// ux.js(ドラッグ&ドロップ)から呼ぶ入口。
window.openMedia = {
  addSources(paths) {
    for (const path of paths) sourceFiles.push({ path, cutRanges: [], frameAccurate: false, includeAsIs: defaultIncludeAsIs(path) });
    renderFileList();
  },
  play(path) {
    playOne(path, () => playItem({ kind: "file", paths: [path], label: path }, ""));
  },
};

// ── 関連付けと自動再生(Windows) ─────────────────────────────
const ASSOC_EXTS = ["mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "dsf", "dff", "mka", "mp4", "mkv", "avi", "mov", "webm", "mpg", "vob", "m2ts"];
const assocTable = document.getElementById("assoc-table");
const assocStatus = document.getElementById("assoc-status");
const autoplayStatus = document.getElementById("autoplay-status");
async function refreshAutoplay() {
  try {
    const on = await invoke("autoplay_status");
    autoplayStatus.textContent = on ? "自動再生: 音楽CD・DVDはopen-mediaで開く設定になっています / AutoPlay: audio CDs and DVDs open with open-media" : "自動再生: 未設定(Windowsの既定の動作) / AutoPlay: not set (Windows default)";
  } catch (e) { autoplayStatus.textContent = String(e); }
}
async function checkAssoc() {
  assocStatus.textContent = "確認中... / Checking...";
  try {
    const rows = await invoke("assoc_list", { exts: ASSOC_EXTS });
    assocTable.innerHTML = "<tr><th></th><th align=left>拡張子 / Ext</th><th align=left>いま開くアプリ / Current app</th><th align=left>open-media</th></tr>";
    for (const r of rows) {
      const tr = document.createElement("tr");
      const state = r.is_default ? "既定 / default" : r.registered ? "候補に登録済み / registered" : "未登録 / not registered";
      tr.innerHTML = `<td><input type="checkbox" data-ext="${r.ext}" ${r.registered ? "" : "checked"} /></td><td>.${r.ext}</td><td>${r.app}</td><td>${state}</td>`;
      assocTable.appendChild(tr);
    }
    assocStatus.textContent = "登録したい拡張子にチェックを入れて「登録」を押してください。既定にする操作は「Windowsの既定アプリ設定を開く」から。 / Tick the extensions to register, then press Register. To make it the default, use the Windows settings button.";
  } catch (e) { assocStatus.textContent = String(e); }
}
document.getElementById("assoc-check-btn").addEventListener("click", checkAssoc);
document.getElementById("assoc-register-btn").addEventListener("click", async () => {
  const exts = [...assocTable.querySelectorAll("input[data-ext]:checked")].map((c) => c.dataset.ext);
  if (!exts.length) { assocStatus.textContent = "拡張子を選んでください / Select extensions first"; return; }
  try {
    const n = await invoke("assoc_register", { exts });
    assocStatus.textContent = `${n}個の拡張子にopen-mediaを候補として登録しました。既定にするには「Windowsの既定アプリ設定を開く」で選んでください。 / Registered open-media as a candidate for ${n} extension(s). Pick it as default in the Windows settings.`;
    await checkAssoc();
  } catch (e) { assocStatus.textContent = `登録できません / Cannot register: ${e}`; }
});
document.getElementById("assoc-settings-btn").addEventListener("click", () => invoke("open_default_apps_settings").catch((e) => { assocStatus.textContent = String(e); }));
document.getElementById("autoplay-on-btn").addEventListener("click", async () => {
  try { await invoke("autoplay_enable"); } catch (e) { autoplayStatus.textContent = `設定できません / Cannot set: ${e}`; return; }
  refreshAutoplay();
});
document.getElementById("autoplay-off-btn").addEventListener("click", async () => {
  await invoke("autoplay_disable").catch(() => {});
  refreshAutoplay();
});
(async () => {
  // Windows以外(スマホ・Mac・Linux)では関連付けの欄を隠す
  if (!/Windows/i.test(navigator.userAgent)) { document.getElementById("assoc-section").style.display = "none"; return; }
  refreshAutoplay();
  checkAssoc();
})();

// ── 起動時の引数: 自動再生(--play-disc D:\)やファイルを開く(open-media.exe "file") ──
(async () => {
  let args = [];
  try { args = await invoke("startup_args"); } catch (_) { return; }
  if (!args.length) return;
  const show = window.__showTab; if (show) show("play");
  const i = args.indexOf("--play-disc");
  if (i >= 0 && args[i + 1]) {
    const drive = args[i + 1].replace(/[\\\/]+$/, ""); // D:\ → D:
    // ドライブを選んで読み込み、音楽CDなら1曲目を再生する
    const sel = document.getElementById("player-drive");
    await new Promise((r) => setTimeout(r, 600)); // 起動直後のデバイス一覧の読み込みを待つ
    if (![...sel.options].some((o) => o.value === drive)) sel.add(new Option(drive, drive));
    sel.value = drive;
    document.getElementById("player-scan-btn").click();
    for (let n = 0; n < 40; n++) { // 最大約12秒、曲の一覧が出るのを待つ
      await new Promise((r) => setTimeout(r, 300));
      const first = document.querySelector('#player-tracks input[name="player-track"]:not(:disabled)');
      if (first) { first.checked = true; first.dispatchEvent(new Event("change")); document.getElementById("player-play-btn").click(); return; }
    }
    return;
  }
  const file = args.find((a) => !a.startsWith("--"));
  if (file && window.openMedia && /\.(mp3|wav|flac|aac|m4a|ogg|opus|dsf|dff|mka|mp4|mkv|avi|mov|webm|mpg|mpeg|vob|m2ts|ts)$/i.test(file)) {
    window.openMedia.play(file); // ファイルをopen-mediaで開いたとき: 再生
  }
})();
