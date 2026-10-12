//! ディスク種別ごとの公称容量とビットレート自動計算。

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscType {
    Cd700,
    Dvd47,
    DvdDl85,
    Bd25,
    Bd50,
    /// BDXL 3層(triple layer)、100GB。
    Bd100,
    /// BDXL 4層(quad layer)、128GB。
    Bd128,
}

impl DiscType {
    /// 公称容量(バイト)。書き込み時のファイルシステムオーバーヘッド分を
    /// 差し引いた実用値(公称値の約98%)を返す。
    pub fn usable_bytes(self) -> u64 {
        let nominal: u64 = match self {
            DiscType::Cd700 => 700 * 1024 * 1024,
            DiscType::Dvd47 => 4_700_000_000,
            DiscType::DvdDl85 => 8_500_000_000,
            DiscType::Bd25 => 25_000_000_000,
            DiscType::Bd50 => 50_000_000_000,
            DiscType::Bd100 => 100_000_000_000,
            DiscType::Bd128 => 128_000_000_000,
        };
        nominal * 98 / 100
    }
}

/// CD品質(44.1kHz・16bit・ステレオ)の非圧縮PCM WAVのビット/秒。
/// 「最高音質」モード(2026-09-16新設)でのロスレス音声変換に使う——
/// ロスレスPCMは可変ビットレートではなく、サンプルレート・ビット深度・
/// チャンネル数だけで一意に決まる物理量なので、[`max_bitrate_for_capacity`]
/// のような「容量から逆算」はできない(逆に「この設定なら何秒収まるか」を
/// 計算する側になる、[`max_lossless_audio_duration_secs`]参照)。
pub const LOSSLESS_CD_QUALITY_WAV_BPS: u64 = 44_100 * 16 * 2;

/// 「最高音質」モードで、指定ディスクにCD品質ロスレスWAVとして収まる
/// 最大収録時間(秒)を返す。
pub fn max_lossless_audio_duration_secs(disc: DiscType, reserved_bytes: u64) -> f64 {
    let usable = disc.usable_bytes().saturating_sub(reserved_bytes);
    (usable as f64 * 8.0) / LOSSLESS_CD_QUALITY_WAV_BPS as f64
}

/// 「最高音質」モードで、実際にディスクへ収まるかどうかの判定結果。
/// 収まらない場合は、代わりにどれだけの時間なら収まるか
/// (`max_fitting_duration_secs`)も返す——ユーザー指示「必要な時間や
/// データサイズを自動で割り出す」への対応。
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct LosslessFitEstimate {
    pub fits: bool,
    pub required_bytes: u64,
    pub usable_bytes: u64,
    /// `fits`が`false`の場合、代わりにこの秒数までなら収まる。
    pub max_fitting_duration_secs: f64,
}

/// [`LosslessFitEstimate`]を計算する。
pub fn estimate_lossless_audio_fit(disc: DiscType, total_duration_secs: f64, reserved_bytes: u64) -> LosslessFitEstimate {
    let usable_bytes = disc.usable_bytes().saturating_sub(reserved_bytes);
    let required_bytes = ((total_duration_secs.max(0.0) * LOSSLESS_CD_QUALITY_WAV_BPS as f64) / 8.0) as u64;
    LosslessFitEstimate {
        fits: required_bytes <= usable_bytes,
        required_bytes,
        usable_bytes,
        max_fitting_duration_secs: max_lossless_audio_duration_secs(disc, reserved_bytes),
    }
}

/// 収録したいメディアの合計尺(秒)から、指定ディスクに収まる
/// 最大平均ビットレート(bit/s)を算出する。
/// `reserved_bytes` はISO9660オーバーヘッド等の予約分。
///
/// 下限は設けていない。収録時間がディスク容量に対して極端に長い場合
/// (例: 10時間分を1枚のCD/DVDに収める等)でも、常に「収まる値」まで
/// ビットレートを下げて返す。品質面の目安は[`quality_warning`]で別途
/// 判定する。
pub fn max_bitrate_for_capacity(disc: DiscType, total_duration_secs: f64, reserved_bytes: u64) -> u64 {
    if total_duration_secs <= 0.0 {
        return 0;
    }
    let usable = disc.usable_bytes().saturating_sub(reserved_bytes);
    ((usable as f64 * 8.0) / total_duration_secs) as u64
}

/// 品質の目安として基準とするビットレート(bit/s)。
/// 音声はMP3の高品質帯(256kbps)、動画はSD〜HD相当の実用帯(6Mbps)を基準にする。
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Audio,
    Video,
}

impl MediaKind {
    fn reference_bps(self) -> f64 {
        match self {
            MediaKind::Audio => 256_000.0,
            MediaKind::Video => 6_000_000.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct QualityWarning {
    pub level: u8,
    pub message_ja: String,
    pub message_en: String,
}

use serde::Serialize;

/// 自動算出ビットレートが基準よりどれだけ低いかに応じて、
/// 4段階(下がります → 少し下がります → かなり下がります → 画質/音質が
/// 落ちます)で警告メッセージを返す。基準の85%以上ならNoneで問題なし。
pub fn quality_warning(bitrate_bps: u64, kind: MediaKind) -> Option<QualityWarning> {
    let reference = kind.reference_bps();
    let ratio = bitrate_bps as f64 / reference;

    let (level, ja, en): (u8, &str, &str) = if ratio >= 0.85 {
        return None;
    } else if ratio >= 0.7 {
        (1, "ビットレートが下がります。", "The bitrate will be reduced.")
    } else if ratio >= 0.4 {
        (2, "ビットレートが少し下がります。", "The bitrate will be reduced a bit further.")
    } else if ratio >= 0.15 {
        (3, "ビットレートがかなり下がります。", "The bitrate will be reduced considerably.")
    } else {
        match kind {
            MediaKind::Audio => (4, "音質が落ちます。", "Audio quality will noticeably drop."),
            MediaKind::Video => (4, "画質が落ちます。", "Picture quality will noticeably drop."),
        }
    };

    Some(QualityWarning {
        level,
        message_ja: ja.to_string(),
        message_en: en.to_string(),
    })
}

/// 任意のビットレート(kbps)での容量見積もり。[`LosslessFitEstimate`]と同じ形だが、
/// ロスレスPCM固定ではなく指定ビットレートで計算する——「収まるか予測」「下調べ」を
/// 動画のアップコンバートだけでなく、CD取り込みや通常の音声変換にも使えるようにする
/// (2026-09-28、ユーザー指摘: 音声/CDにも対応してほしい)。
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct BitrateFitEstimate {
    pub fits: bool,
    pub required_bytes: u64,
    pub usable_bytes: u64,
    /// `fits`が`false`の場合、代わりにこの秒数までなら収まる。
    pub max_fitting_duration_secs: f64,
}

pub fn estimate_bitrate_fit(disc: DiscType, total_duration_secs: f64, bitrate_kbps: u64, reserved_bytes: u64) -> BitrateFitEstimate {
    let usable_bytes = disc.usable_bytes().saturating_sub(reserved_bytes);
    let bps = bitrate_kbps as f64 * 1000.0;
    let required_bytes = ((total_duration_secs.max(0.0) * bps) / 8.0) as u64;
    let max_fitting_duration_secs = if bps > 0.0 { (usable_bytes as f64 * 8.0) / bps } else { 0.0 };
    BitrateFitEstimate {
        fits: required_bytes <= usable_bytes,
        required_bytes,
        usable_bytes,
        max_fitting_duration_secs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bd128_usable_bytes_is_98_percent_of_128gb() {
        let usable = DiscType::Bd128.usable_bytes();
        assert_eq!(usable, 128_000_000_000 * 98 / 100);
        assert!(usable > DiscType::Bd50.usable_bytes());
    }

    /// 「最高音質」モード(2026-09-16新設)の基本動作:
    /// CD(700MB)に十分収まる短い尺なら`fits=true`になること。
    #[test]
    fn lossless_audio_fit_reports_true_for_a_short_clip_on_cd() {
        let estimate = estimate_lossless_audio_fit(DiscType::Cd700, 60.0, 0);
        assert!(estimate.fits);
        assert!(estimate.required_bytes < estimate.usable_bytes);
    }

    /// CD1枚に収まらない長さ(例: 10時間)を指定した場合は`fits=false`に
    /// なり、代わりに収まる秒数(`max_fitting_duration_secs`)が
    /// `usable_bytes`から逆算した一貫性のある値になること。
    #[test]
    fn lossless_audio_fit_reports_false_and_max_fitting_duration_for_a_too_long_clip() {
        let ten_hours = 10.0 * 3600.0;
        let estimate = estimate_lossless_audio_fit(DiscType::Cd700, ten_hours, 0);
        assert!(!estimate.fits);
        assert!(estimate.max_fitting_duration_secs < ten_hours);
        assert!(estimate.max_fitting_duration_secs > 0.0);

        // 逆算した秒数ちょうどなら収まるはず(整合性チェック)。
        let recheck = estimate_lossless_audio_fit(DiscType::Cd700, estimate.max_fitting_duration_secs, 0);
        assert!(recheck.fits);
    }

    /// より大きなディスク(Blu-ray 25GB)なら、CD1枚に収まらない長さでも
    /// 収まること(ディスク種別による違いが正しく反映されているか)。
    #[test]
    fn lossless_audio_fit_on_a_larger_disc_fits_a_longer_clip() {
        let ten_hours = 10.0 * 3600.0;
        let estimate = estimate_lossless_audio_fit(DiscType::Bd25, ten_hours, 0);
        assert!(estimate.fits);
    }

    /// 一般ビットレートでの見積もり(音声/CD向け): 短い尺・低ビットレートなら
    /// CD1枚に十分収まること。
    #[test]
    fn bitrate_fit_reports_true_for_a_short_low_bitrate_clip_on_cd() {
        let estimate = estimate_bitrate_fit(DiscType::Cd700, 180.0, 192, 0);
        assert!(estimate.fits);
        assert!(estimate.required_bytes < estimate.usable_bytes);
    }

    /// 収まらない場合は、代わりに収まる秒数を返し、その秒数ちょうどなら
    /// 実際に収まる(ロスレス版と同じ整合性)。
    #[test]
    fn bitrate_fit_reports_false_and_max_fitting_duration_when_too_long() {
        let ten_hours = 10.0 * 3600.0;
        let estimate = estimate_bitrate_fit(DiscType::Cd700, ten_hours, 320, 0);
        assert!(!estimate.fits);
        assert!(estimate.max_fitting_duration_secs < ten_hours);
        assert!(estimate.max_fitting_duration_secs > 0.0);
        let recheck = estimate_bitrate_fit(DiscType::Cd700, estimate.max_fitting_duration_secs, 320, 0);
        assert!(recheck.fits);
    }

    #[test]
    fn serde_round_trips_bd128_as_snake_case() {
        let json = serde_json::to_string(&DiscType::Bd128).unwrap();
        assert_eq!(json, "\"bd128\"");
        let back: DiscType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, DiscType::Bd128);
    }
}
