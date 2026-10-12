//! `open-cpu`(エコシステム共通のCPU命令セット検出ライブラリ)を使い、
//! GPUハードウェアエンコーダが見つからない場合のCPUフォールバック
//! (libx264)がどの程度実用的な速度になりそうかをUIへ伝える。
//!
//! 実際のffmpeg/libx264の内部SIMDディスパッチには関与しない
//! (libx264自身が実行時に最適な命令セットを自動選択するため、
//! その挙動を変えることはできないし変える必要も無い)。ここでの役割は
//! あくまで「フレーム精度カットでGPUが無い場合、この見積もりで進めて
//! よいか」をユーザーに判断してもらうための参考情報の提示。
//!
//! `open-cpu`が検出する命令セットのうち、動画エンコードの実速度に
//! 関係し得るものを網羅的に反映する(AVX2/AVX512Fだけでなく、
//! AVX512BW/AVX512VL〈x264のAVX-512最適化パスが実際に要求する組み合わせ〉・
//! FMA〈積和演算、動き探索等で有利〉も見て、4段階で見積もる)。
//! BMI1/BMI2/AES/SHA/GFNI等は動画エンコードの主要ホットパスには
//! 直接関与しないため見積もりには含めないが、`detected_features`として
//! 参考表示する。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CpuEncodeEstimate {
    pub avx2: bool,
    pub avx512: bool,
    /// "fast" | "moderate" | "slow" | "very_slow"(参考値、実測ではない)。
    pub speed_hint: &'static str,
    pub message_ja: String,
    pub message_en: String,
    /// 参考表示用に検出できた命令セット名の一覧(動画エンコードの
    /// 判断には使っていないものも含む)。
    pub detected_features: Vec<&'static str>,
}

pub fn estimate_cpu_encode_speed() -> CpuEncodeEstimate {
    let caps = open_cpu::detect();

    // x264のAVX-512最適化パスは実際にはAVX512F単体ではなく
    // AVX512F+AVX512BW+AVX512VLの組み合わせを要求することが多いため、
    // 3つ揃って初めて「full」とみなす。
    let avx512_full = caps.has_all(&[caps.avx512f, caps.avx512bw, caps.avx512vl]);
    let avx512 = caps.avx512f;
    let avx2 = caps.avx2;
    let fma = caps.fma;

    let (speed_hint, ja, en): (&'static str, &str, &str) = if avx512_full {
        (
            "fast",
            "AVX-512(BW/VL込み)対応CPUです。libx264のAVX-512最適化パスを自動的に使うため、GPU無しでも比較的高速です。",
            "AVX-512 (with BW/VL) detected. libx264's AVX-512-optimized path will be used automatically, so CPU-only encoding should be reasonably fast.",
        )
    } else if avx512 {
        (
            "moderate",
            "AVX-512F対応ですが、libx264のAVX-512最適化パスが要求するBW/VLの一部が欠けています。AVX2相当の速度になる見込みです。",
            "AVX-512F is present, but some of BW/VL that libx264's AVX-512 path needs are missing — expect roughly AVX2-level speed.",
        )
    } else if avx2 && fma {
        (
            "moderate",
            "AVX2+FMA対応CPUです。libx264が自動的に活用するため、GPUエンコーダには及ばないものの実用的な速度が見込めます。",
            "AVX2+FMA detected. libx264 will use both automatically — not as fast as a GPU encoder, but practically usable.",
        )
    } else if avx2 {
        (
            "moderate",
            "AVX2対応CPUです。libx264が自動的にAVX2を使いますが、GPUエンコーダに比べると時間はかかります。",
            "AVX2-capable CPU detected. libx264 will automatically use it, but it will still be slower than a GPU encoder.",
        )
    } else if caps.ssse3 {
        (
            "slow",
            "AVX2非対応(SSSE3止まり)のCPUです。フレーム精度カットのCPUエンコードは数時間規模の動画ではかなり時間がかかります。",
            "This CPU lacks AVX2 (SSSE3 at best). CPU-only frame-accurate encoding will take considerable time for multi-hour footage.",
        )
    } else {
        (
            "very_slow",
            "SSSE3にも非対応の非常に古いCPUです。CPUエンコードは数時間規模の動画では非現実的な時間がかかる可能性が高いです。GPUエンコーダの利用を強く推奨します。",
            "This CPU lacks even SSSE3. CPU-only encoding is very likely to take an unrealistic amount of time for multi-hour footage — a GPU encoder is strongly recommended.",
        )
    };

    let mut detected_features = Vec::new();
    let flag_table: &[(bool, &'static str)] = &[
        (caps.sse2, "SSE2"),
        (caps.ssse3, "SSSE3"),
        (caps.avx2, "AVX2"),
        (caps.fma, "FMA"),
        (caps.avx512f, "AVX-512F"),
        (caps.avx512bw, "AVX-512BW"),
        (caps.avx512vl, "AVX-512VL"),
        (caps.avx_vnni, "AVX-VNNI"),
        (caps.avx512vnni, "AVX-512VNNI"),
        (caps.bmi1, "BMI1"),
        (caps.bmi2, "BMI2"),
        (caps.aes, "AES-NI"),
        (caps.sha, "SHA"),
        (caps.popcnt, "POPCNT"),
        (caps.pclmulqdq, "PCLMULQDQ"),
        (caps.gfni, "GFNI"),
        (caps.vpclmulqdq, "VPCLMULQDQ"),
    ];
    for (present, name) in flag_table {
        if *present {
            detected_features.push(*name);
        }
    }

    CpuEncodeEstimate {
        avx2,
        avx512,
        speed_hint,
        message_ja: ja.to_string(),
        message_en: en.to_string(),
        detected_features,
    }
}

/// open-cpuの検出結果に基づき、CPU(libx264)フォールバック時のffmpeg
/// `-preset`を自動選択する。x264自身はAVX2/AVX-512の使用可否を実行時に
/// 自動判定するが、「1フレームあたりどれだけ探索を頑張るか」を決める
/// presetは外部から明示的に指定する必要があるため、ここでopen-cpuの
/// 検出結果を実際のffmpegコマンドライン引数へ反映させる
/// (=表示するだけでなく実際に分岐・制御に使う)。
///
/// 非力なCPUには軽いpreset(ultrafast/veryfast)を、強力なCPUには
/// より圧縮効率の良い(=遅い)presetを割り当てる。
/// 「必要な部分だけ切り出す(高速)」用の、速度優先のx264 `-preset`(2026-09-23新設)。
/// `recommended_x264_preset`は画質寄り(強いCPUほど`slow`)だが、切り出しでは「待たずに済む」ことが
/// 目的なので、同じopen-cpuの判定から速度寄りの値を選ぶ。
pub fn fast_x264_preset() -> &'static str {
    match estimate_cpu_encode_speed().speed_hint {
        "fast" => "faster",
        "moderate" => "veryfast",
        "slow" => "superfast",
        _ => "ultrafast",
    }
}

pub fn recommended_x264_preset() -> &'static str {
    match estimate_cpu_encode_speed().speed_hint {
        "fast" => "slow",
        "moderate" => "medium",
        "slow" => "veryfast",
        _ => "ultrafast",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_a_valid_speed_hint_on_this_machine() {
        let estimate = estimate_cpu_encode_speed();
        assert!(["fast", "moderate", "slow", "very_slow"].contains(&estimate.speed_hint));
        // avx512はavx2の上位互換なので、avx512ならavx2も真であるはず。
        if estimate.avx512 {
            assert!(estimate.avx2);
        }
        // 検出できた機能が1つも無いのにavx2がtrueになることは無いはず。
        if estimate.avx2 {
            assert!(estimate.detected_features.contains(&"AVX2"));
        }
    }

    #[test]
    fn detected_features_never_contains_duplicates() {
        let estimate = estimate_cpu_encode_speed();
        let mut seen = std::collections::HashSet::new();
        for f in &estimate.detected_features {
            assert!(seen.insert(f), "duplicate feature reported: {f}");
        }
    }
}
