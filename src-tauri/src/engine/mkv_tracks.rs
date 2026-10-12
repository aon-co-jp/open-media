//! MKV(Matroska)の複数音声・字幕トラックの管理(2026-09-19新設)。
//!
//! MKVは1本のファイルに複数の音声(言語違い・解説音声など)・字幕・添付(フォント)を持てる。
//! ffmpegは`-map`を省略すると映像・音声・字幕を**各1本だけ**選ぶため、そのままでは二重音声や複数字幕が
//! 失われる。ここでは次の2つを提供する:
//! 1. **全トラック保持**: 出力が`.mkv`のとき、元ファイルの全映像/音声/字幕/添付をマップする。
//! 2. **トラック追加**: 別ファイルの音声(`.mka`/`.wav`/`.flac`/`.ac3`等)や字幕(`.srt`/`.ass`等)を、
//!    言語コード・タイトル付きの追加トラックとして同じMKVへ多重化する。
//!
//! 字幕は再エンコードせず`-c:s copy`(テキスト字幕もPGS等の画像字幕もそのまま)。**正直な開示**: MP4等の
//! MKV以外への出力では複数字幕を持てない形式があるため、この処理はMKV出力のときだけ働く。

use crate::engine::sidecar::resolve_tool;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtraTrack {
    pub path: String,
    /// `"audio"`または`"subtitle"`。
    pub kind: String,
    /// ISO 639-2の言語コード(例: `jpn`、`eng`)。省略可。
    #[serde(default)]
    pub language: Option<String>,
    /// トラック名(例: `監督解説`)。省略可。
    #[serde(default)]
    pub title: Option<String>,
}

pub fn is_mkv_output(output_path: &str) -> bool {
    std::path::Path::new(output_path).extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mkv"))
}

/// 言語コード・タイトルに使える文字か(ffmpegの`-metadata`値としてそのまま渡すので、制御文字だけ弾く)。
fn clean(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect::<String>().trim().to_string()
}

/// 指定ファイルの音声(`a`)または字幕(`s`)ストリーム数を実ffprobeで数える。
pub fn count_streams(path: &str, kind: char) -> usize {
    let out = resolve_tool("ffprobe").args(["-v", "error", "-select_streams", &kind.to_string(), "-show_entries", "stream=index", "-of", "csv=p=0", path]).output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.trim().is_empty()).count(),
        _ => 0,
    }
}

/// 追加トラック用の入力引数(`-i <path>`。トリミング指定があれば主入力と同じ`-ss`を各入力の前に付けて同期を保つ)。
pub fn extra_input_args(extras: &[ExtraTrack], trim_start: Option<f64>) -> Vec<String> {
    let mut a = Vec::new();
    for t in extras {
        if let Some(s) = trim_start {
            a.push("-ss".to_string());
            a.push(s.to_string());
        }
        a.push("-i".to_string());
        a.push(t.path.clone());
    }
    a
}

/// `-map`・`-c:s`・トラックのメタデータ引数を作る。
/// - `keep_source_all`: 主入力(入力0)の映像/音声/字幕/添付をすべてマップする(`-map`を既に含む場合は不要)。
/// - `existing_audio` / `existing_subs`: 主入力に元々ある音声・字幕の本数(追加トラックのメタデータ添字の計算に使う)。
pub fn map_args(extras: &[ExtraTrack], keep_source_all: bool, existing_audio: usize, existing_subs: usize) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    if keep_source_all {
        for m in ["0:v?", "0:a?", "0:s?", "0:t?"] {
            a.push("-map".into());
            a.push(m.into());
        }
        a.push("-c:s".into());
        a.push("copy".into());
    }
    let (mut audio_idx, mut sub_idx) = (existing_audio, existing_subs);
    let mut has_sub_extra = false;
    for (i, t) in extras.iter().enumerate() {
        let input = i + 1;
        let (sel, spec_kind, idx) = if t.kind == "subtitle" {
            has_sub_extra = true;
            let n = sub_idx;
            sub_idx += 1;
            ("s", "s", n)
        } else {
            let n = audio_idx;
            audio_idx += 1;
            ("a", "a", n)
        };
        a.push("-map".into());
        a.push(format!("{input}:{sel}:0"));
        if let Some(l) = t.language.as_deref().map(clean).filter(|l| !l.is_empty()) {
            a.push(format!("-metadata:s:{spec_kind}:{idx}"));
            a.push(format!("language={l}"));
        }
        if let Some(title) = t.title.as_deref().map(clean).filter(|l| !l.is_empty()) {
            a.push(format!("-metadata:s:{spec_kind}:{idx}"));
            a.push(format!("title={title}"));
        }
    }
    if has_sub_extra && !keep_source_all {
        a.push("-c:s".into());
        a.push("copy".into());
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extra(kind: &str, lang: Option<&str>, title: Option<&str>) -> ExtraTrack {
        ExtraTrack { path: "x".into(), kind: kind.into(), language: lang.map(String::from), title: title.map(String::from) }
    }

    #[test]
    fn detects_mkv_output_case_insensitively() {
        assert!(is_mkv_output("a/b.MKV"));
        assert!(!is_mkv_output("a/b.mp4"));
    }

    #[test]
    fn map_args_keep_all_sources_and_number_extra_tracks_after_the_existing_ones() {
        let a = map_args(&[extra("audio", Some("eng"), Some("Commentary")), extra("subtitle", Some("jpn"), None)], true, 2, 1);
        let joined = a.join(" ");
        assert!(joined.starts_with("-map 0:v? -map 0:a? -map 0:s? -map 0:t? -c:s copy"), "{joined}");
        assert!(joined.contains("-map 1:a:0 -metadata:s:a:2 language=eng -metadata:s:a:2 title=Commentary"), "既存音声2本の次(添字2): {joined}");
        assert!(joined.contains("-map 2:s:0 -metadata:s:s:1 language=jpn"), "既存字幕1本の次(添字1): {joined}");
    }

    #[test]
    fn extra_only_mode_does_not_add_source_maps_and_copies_subs() {
        let a = map_args(&[extra("subtitle", None, None)], false, 0, 0);
        assert_eq!(a, vec!["-map", "1:s:0", "-c:s", "copy"]);
    }

    #[test]
    fn control_characters_are_stripped_from_metadata() {
        let a = map_args(&[extra("audio", Some("jp\nn"), Some("a\u{0}b"))], false, 0, 0);
        assert!(a.contains(&"language=jpn".to_string()) && a.contains(&"title=ab".to_string()), "{a:?}");
    }

    #[test]
    fn extra_inputs_repeat_the_trim_start_so_tracks_stay_in_sync() {
        let a = extra_input_args(&[extra("audio", None, None)], Some(12.5));
        assert_eq!(a, vec!["-ss", "12.5", "-i", "x"]);
    }
}
