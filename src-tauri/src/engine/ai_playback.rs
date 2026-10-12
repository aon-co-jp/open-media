//! 再生時のアップサンプリング先(サンプルレート)を、`aruaru-llm`(Qwen)に判断させる。
//!
//! 材料は元音源のコーデック・サンプルレート・ビットレート。LLMに4つの候補から番号で答えさせ、
//! 接続できない・読めない場合は整数倍の規則(44.1k系→88.2k、48k系→96k)で決める。
//! 正直な開示: アップサンプリングは情報を増やさず、再生機器側の変換を高品質に置き換えるだけ。

use serde::Serialize;

pub const CHOICES: [u32; 4] = [88_200, 96_000, 176_400, 192_000];

#[derive(Debug, Clone, Serialize)]
pub struct RateDecision {
    pub sample_rate: u32,
    /// "llm" / "rule"
    pub method: String,
}

/// 元のサンプルレートの整数倍で、既定の2倍に最も近い候補(LLM不使用時の規則)。
pub fn rule_rate(src: u32) -> u32 {
    if src > 0 && src % 44_100 == 0 {
        if src >= 176_400 { src.min(192_000) } else { 88_200 }
    } else if src >= 96_000 {
        if src % 48_000 == 0 { src.min(192_000) } else { 192_000 }
    } else {
        96_000
    }
}

fn parse_choice(text: &str) -> Option<u32> {
    let d = text.chars().find(|c| c.is_ascii_digit())?.to_digit(10)? as usize;
    (1..=CHOICES.len()).contains(&d).then(|| CHOICES[d - 1])
}

pub fn decide(path: &str, llm_url: &str) -> Result<RateDecision, String> {
    let info = crate::engine::probe::probe(path)?;
    let src = info.audio_sample_rate.unwrap_or(44_100);
    let codec = info.audio_codec.clone().unwrap_or_default();
    let kbps = info.bit_rate.map(|b| b / 1000).unwrap_or(0);
    let fallback = RateDecision { sample_rate: rule_rate(src), method: "rule".into() };
    let llm = llm_url.trim().trim_end_matches('/');
    if llm.is_empty() {
        return Ok(fallback);
    }
    let prompt = format!(
        "<|im_start|>system\nYou choose the best playback upsampling rate. Prefer an integer multiple of the source rate. Lossy or low-bitrate sources gain nothing from very high rates. Answer with the number only.<|im_end|>\n<|im_start|>user\nSource: codec={codec}, sample rate={src} Hz, bitrate={kbps} kbps.\n1. 88200 Hz\n2. 96000 Hz\n3. 176400 Hz\n4. 192000 Hz\nWhich number is best?<|im_end|>\n<|im_start|>assistant\n"
    );
    let body = serde_json::json!({ "prompt": prompt, "max_new_tokens": 4 }).to_string();
    let resp = ureq::post(&format!("{llm}/v1/generate-qwen"))
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(20))
        .send_string(&body);
    let rate = resp
        .ok()
        .and_then(|r| r.into_string().ok())
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["completion"].as_str().or_else(|| v["text"].as_str()).and_then(parse_choice));
    // LLMの答えが元レートの整数倍でない場合は規則を優先する(非整数倍は品質が落ちるため)。
    Ok(match rate {
        Some(r) if src > 0 && r % src == 0 => RateDecision { sample_rate: r, method: "llm".into() },
        _ => fallback,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rule_prefers_integer_multiples() {
        assert_eq!(rule_rate(44_100), 88_200);
        assert_eq!(rule_rate(48_000), 96_000);
        assert_eq!(rule_rate(96_000), 96_000);
        assert_eq!(rule_rate(88_200), 88_200);
    }
    #[test]
    fn parses_choice() {
        assert_eq!(parse_choice(" 2"), Some(96_000));
        assert_eq!(parse_choice("9"), None);
    }
}
