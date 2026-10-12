//! 「欲しい部分をAIが探して、切り出しの開始・終了を提案する」(2026-09-23新設)。
//!
//! 5時間の元データから「60分だけ」「10分だけ」欲しいとき、どこを切り出すかを探す作業自体を
//! 助ける機能。手順は次の3段:
//!
//! 1. **何を話しているか(時刻付きのテキスト)を集める**:
//!    (a) 元ファイルと同じ名前の字幕ファイル(.srt/.vtt)、(b) ファイルに埋め込まれた字幕、
//!    (c) どちらも無ければ`aruaru-llm`の`POST /v1/transcribe`(whisper.cpp)で1分ずつ書き起こす。
//! 2. **候補を絞る**: 指定した長さの窓を少しずつずらし、依頼文との文字の近さ(文字2-gramの一致、
//!    日本語でも英語でも使える)で点数を付け、重ならない上位3件を候補にする。
//! 3. **LLMで最終選択**: `aruaru-llm`の`POST /v1/generate-qwen`(Qwen2.5)に依頼文と3候補の内容を渡し、
//!    最も合うものの番号を答えさせる。
//!
//! ## 正直な開示
//! - 映像そのもの(画面に何が映っているか)は見ていない。**話している内容(字幕・音声)だけ**が手掛かり。
//! - 字幕が無い長い音声の書き起こしは時間がかかる(whisper.cppの速度次第)。字幕があれば一瞬で終わる。
//! - `aruaru-llm`に接続できない場合は、手順2の点数だけで選び、その旨を結果の`method`/`note`に明記する。
//! - Qwen2.5-0.5B等の小型モデルは長文を扱えないため、LLMに渡すのは3候補の要約(先頭の一部)だけにしている。

use serde::Serialize;
use crate::engine::sidecar::resolve_tool;

#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub start_secs: f64,
    pub end_secs: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub start_secs: f64,
    pub end_secs: f64,
    pub score: f64,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    pub start_secs: f64,
    pub end_secs: f64,
    /// 手掛かりの出どころ: "subtitle_file" / "embedded_subtitle" / "transcribe"
    pub source: String,
    /// 選び方: "llm"(aruaru-llmが選んだ) / "keyword"(文字の近さの点数のみ)
    pub method: String,
    pub candidates: Vec<Candidate>,
    pub note: String,
}

/// 元ファイル`path`から、`request`(欲しい内容の説明)に合う`length_secs`秒の範囲を提案する。
/// `llm_url`は`aruaru-llm`のURL(例: `http://127.0.0.1:4600`)。
pub fn suggest_range(path: &str, request: &str, length_secs: f64, llm_url: &str) -> Result<Suggestion, String> {
    if request.trim().is_empty() {
        return Err("欲しい内容の説明を入力してください / Please describe what you want".into());
    }
    if length_secs <= 0.0 {
        return Err("切り出す長さを指定してください / Please specify the length".into());
    }
    let duration = crate::engine::probe::probe(path)?.duration_secs;
    let llm = llm_url.trim_end_matches('/');

    let (segments, source) = if let Some(s) = read_sidecar_subtitles(path) {
        (s, "subtitle_file")
    } else if let Some(s) = read_embedded_subtitles(path) {
        (s, "embedded_subtitle")
    } else {
        (transcribe_with_aruaru(path, duration, llm)?, "transcribe")
    };
    if segments.is_empty() {
        return Err("話している内容(字幕・音声)を取り出せませんでした / Could not extract any spoken content".into());
    }

    let candidates = top_candidates(&segments, request, length_secs.min(duration.max(1.0)), duration, 3);
    if candidates.is_empty() {
        return Err("候補を作れませんでした / No candidate ranges".into());
    }

    let (pick, method, note) = match ask_llm_to_pick(llm, request, &candidates) {
        Ok(i) => (i, "llm", "aruaru-llm(Qwen)が候補から選びました。 / Chosen by aruaru-llm (Qwen).".to_string()),
        Err(e) => (
            0,
            "keyword",
            format!("aruaru-llmに接続できなかったため、文字の近さの点数だけで選びました({e})。 / Chose by keyword score only because aruaru-llm was unreachable."),
        ),
    };
    let c = &candidates[pick];
    Ok(Suggestion { start_secs: c.start_secs, end_secs: c.end_secs, source: source.into(), method: method.into(), candidates: candidates.clone(), note })
}

// ── 1. 時刻付きテキストを集める ────────────────────────────

fn read_sidecar_subtitles(path: &str) -> Option<Vec<Segment>> {
    let p = std::path::Path::new(path);
    let stem = p.file_stem()?.to_string_lossy().to_string();
    let dir = p.parent()?;
    // 同じフォルダ、および1つ上のフォルダ(元動画をサブフォルダへ移した場合)で、同じ名前で始まる字幕を探す。
    for d in [Some(dir), dir.parent()].into_iter().flatten() {
        let Ok(rd) = std::fs::read_dir(d) else { continue };
        let mut files: Vec<_> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|f| {
                let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let ext = f.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
                (ext == "srt" || ext == "vtt") && name.starts_with(&stem)
            })
            .collect();
        files.sort();
        for f in files {
            if let Ok(text) = std::fs::read_to_string(&f) {
                let segs = parse_srt_or_vtt(&text);
                if !segs.is_empty() {
                    return Some(segs);
                }
            }
        }
    }
    None
}

fn read_embedded_subtitles(path: &str) -> Option<Vec<Segment>> {
    let out = resolve_tool("ffmpeg").args(["-v", "error", "-i", path, "-map", "0:s:0", "-f", "srt", "-"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let segs = parse_srt_or_vtt(&String::from_utf8_lossy(&out.stdout));
    (!segs.is_empty()).then_some(segs)
}

/// SRT/WebVTTの「00:01:02,345 --> 00:01:05,000」行と、その後に続く本文を読む。
pub fn parse_srt_or_vtt(text: &str) -> Vec<Segment> {
    let mut segs = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let Some((a, b)) = line.split_once("-->") else { continue };
        let (Some(start), Some(end)) = (parse_ts(a.trim()), parse_ts(b.split_whitespace().next().unwrap_or(""))) else { continue };
        let mut body = String::new();
        while let Some(l) = lines.peek() {
            if l.trim().is_empty() {
                break;
            }
            if !body.is_empty() {
                body.push(' ');
            }
            body.push_str(strip_tags(l).trim());
            lines.next();
        }
        if !body.is_empty() {
            segs.push(Segment { start_secs: start, end_secs: end, text: body });
        }
    }
    segs
}

fn parse_ts(s: &str) -> Option<f64> {
    let s = s.replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (h.parse::<f64>().ok()?, m.parse::<f64>().ok()?, s.parse::<f64>().ok()?),
        [m, s] => (0.0, m.parse::<f64>().ok()?, s.parse::<f64>().ok()?),
        _ => return None,
    };
    Some(h * 3600.0 + m * 60.0 + sec)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in s.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out
}

/// 字幕が無い場合: 1分ずつ16kHzモノラルのf32 PCMにして、aruaru-llmの`/v1/transcribe`で書き起こす。
fn transcribe_with_aruaru(path: &str, duration: f64, llm: &str) -> Result<Vec<Segment>, String> {
    const CHUNK: f64 = 60.0;
    let mut segs = Vec::new();
    let mut t = 0.0;
    while t < duration {
        let len = CHUNK.min(duration - t);
        let pcm = resolve_tool("ffmpeg")
            .args(["-v", "error", "-ss", &t.to_string(), "-i", path, "-t", &len.to_string(), "-vn", "-ac", "1", "-ar", "16000", "-f", "f32le", "-"])
            .output()
            .map_err(|e| format!("ffmpegの起動に失敗しました: {e}"))?;
        if !pcm.status.success() {
            return Err(format!("音声の取り出しに失敗しました: {}", String::from_utf8_lossy(&pcm.stderr)));
        }
        let body = serde_json::json!({ "pcm_f32_base64": base64_encode(&pcm.stdout), "sample_rate": 16000, "language": "auto" }).to_string();
        let resp = ureq::post(&format!("{llm}/v1/transcribe"))
            .set("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(600))
            .send_string(&body)
            .map_err(|e| {
                format!(
                    "字幕が無く、aruaru-llmの書き起こし(/v1/transcribe)にも接続できませんでした({e})。字幕ファイル(.srt)を元ファイルと同じ名前で置くか、aruaru-llmを起動してください。 / \
                     No subtitles and aruaru-llm transcription is unreachable. Put a .srt with the same name next to the file, or start aruaru-llm."
                )
            })?;
        let v: serde_json::Value = serde_json::from_str(&resp.into_string().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let text = v["transcript"].as_str().unwrap_or("").trim().to_string();
        if !text.is_empty() {
            segs.push(Segment { start_secs: t, end_secs: t + len, text });
        }
        t += CHUNK;
    }
    Ok(segs)
}

fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

// ── 2. 候補を絞る ──────────────────────────────────────

fn bigrams(s: &str) -> std::collections::HashSet<(char, char)> {
    let chars: Vec<char> = s.to_lowercase().chars().filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation()).collect();
    chars.windows(2).map(|w| (w[0], w[1])).collect()
}

/// 各字幕区間の開始位置から`length`秒の窓を作り、依頼文との文字2-gramの一致数で点数を付け、
/// 重ならない上位`k`件を返す。
pub fn top_candidates(segments: &[Segment], request: &str, length: f64, duration: f64, k: usize) -> Vec<Candidate> {
    let q = bigrams(request);
    let seg_scores: Vec<f64> = segments.iter().map(|s| bigrams(&s.text).intersection(&q).count() as f64).collect();
    let mut windows: Vec<Candidate> = Vec::new();
    for (i, s) in segments.iter().enumerate() {
        let start = s.start_secs.min((duration - length).max(0.0));
        let end = (start + length).min(duration.max(length));
        let mut score = 0.0;
        let mut preview = String::new();
        for (j, t) in segments.iter().enumerate().skip(i) {
            if t.start_secs >= end {
                break;
            }
            if t.end_secs > start {
                score += seg_scores[j];
                if preview.chars().count() < 300 {
                    preview.push_str(&t.text);
                    preview.push(' ');
                }
            }
        }
        windows.push(Candidate { start_secs: start, end_secs: end, score, preview: preview.chars().take(300).collect() });
    }
    windows.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal).then(a.start_secs.partial_cmp(&b.start_secs).unwrap_or(std::cmp::Ordering::Equal)));
    let mut picked: Vec<Candidate> = Vec::new();
    for w in windows {
        if picked.iter().all(|p| w.end_secs <= p.start_secs || w.start_secs >= p.end_secs) {
            picked.push(w);
            if picked.len() == k {
                break;
            }
        }
    }
    picked
}

// ── 3. LLMで最終選択 ────────────────────────────────────

fn ask_llm_to_pick(llm: &str, request: &str, candidates: &[Candidate]) -> Result<usize, String> {
    let mut prompt = String::from(
        "<|im_start|>system\nYou pick the passage that best matches the user's request. Answer with the number only.<|im_end|>\n<|im_start|>user\n",
    );
    prompt.push_str(&format!("Request: {request}\n\n"));
    for (i, c) in candidates.iter().enumerate() {
        prompt.push_str(&format!("{}. {}\n\n", i + 1, c.preview.chars().take(200).collect::<String>()));
    }
    prompt.push_str("Which number best matches the request?<|im_end|>\n<|im_start|>assistant\n");
    let body = serde_json::json!({ "prompt": prompt, "max_new_tokens": 4 }).to_string();
    let resp = ureq::post(&format!("{llm}/v1/generate-qwen"))
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(120))
        .send_string(&body)
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&resp.into_string().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let text = v["completion"].as_str().or_else(|| v["text"].as_str()).unwrap_or("");
    parse_choice(text, candidates.len()).ok_or_else(|| format!("LLMの答えを読めませんでした: {text:?}"))
}

fn parse_choice(text: &str, n: usize) -> Option<usize> {
    let d = text.chars().find(|c| c.is_ascii_digit())?.to_digit(10)? as usize;
    (1..=n).contains(&d).then(|| d - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_srt_and_vtt() {
        let srt = "1\n00:00:01,000 --> 00:00:03,500\nこんにちは\n世界\n\n2\n00:01:00,000 --> 00:01:02,000\n<i>龍神の話</i>\n";
        let s = parse_srt_or_vtt(srt);
        assert_eq!(s.len(), 2);
        assert!((s[0].start_secs - 1.0).abs() < 1e-9 && (s[0].end_secs - 3.5).abs() < 1e-9);
        assert_eq!(s[0].text, "こんにちは 世界");
        assert_eq!(s[1].text, "龍神の話");
        let vtt = "WEBVTT\n\n01:02.000 --> 01:04.000 align:start\nhello\n";
        let v = parse_srt_or_vtt(vtt);
        assert_eq!(v.len(), 1);
        assert!((v[0].start_secs - 62.0).abs() < 1e-9);
    }

    #[test]
    fn picks_the_window_that_talks_about_the_request() {
        let mut segs = Vec::new();
        for m in 0..300 {
            let text = if (120..130).contains(&m) { "龍神の祈りと開運について説明します" } else { "今日はいい天気です" };
            segs.push(Segment { start_secs: m as f64 * 60.0, end_secs: m as f64 * 60.0 + 60.0, text: text.into() });
        }
        let c = top_candidates(&segs, "龍神の開運の説明", 600.0, 18000.0, 3);
        assert_eq!(c.len(), 3);
        assert!((c[0].start_secs - 7200.0).abs() < 1e-9, "2時間地点からの10分が最上位のはず: {:?}", c[0]);
        // 候補同士は重ならない
        for a in &c {
            for b in &c {
                if a.start_secs != b.start_secs {
                    assert!(a.end_secs <= b.start_secs || a.start_secs >= b.end_secs);
                }
            }
        }
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn parses_llm_choice() {
        assert_eq!(parse_choice(" 2", 3), Some(1));
        assert_eq!(parse_choice("Answer: 3.", 3), Some(2));
        assert_eq!(parse_choice("5", 3), None);
        assert_eq!(parse_choice("none", 3), None);
    }
}
