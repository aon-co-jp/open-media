//! 「選んだディスクに、この解像度・fpsで収まるか」の予測(2026-09-24新設)。
//!
//! ## 何を予測するか
//! ディスクの容量から、映像に使えるビットレートを求め、それが目標の解像度・fpsに**足りるか**を判定する。
//! 「容量に入る」だけなら、ビットレートをいくらでも下げれば必ず入る。問題は**画質が保てるか**なので、
//! 1画素・1コマあたりのビット数(bpp)を目安にして、5段階(余裕/標準/ぎりぎり/画質低下/収まらない)で答える。
//!
//! ## 正直な開示
//! - これは**目安**であり、実際に必要なビットレートは映像の内容(動きの多さ・細かさ)で大きく変わる。x264(H.264)を想定。
//! - fpsが上がると必要なビットレートは比例では増えない(隣のコマがよく似ているため)ので、`fps^0.6`に比例するとみなした。
//! - 単色(真っ黒・真っ白)や静止したコマは、エンコーダがほとんどビットを使わない。その割合が多いほど、他のコマに
//!   ビットを回せるので有利になる。割合はffmpegの`blackdetect`・`freezedetect`で数か所を抜き取って推定する(全編は調べない)。
//! - フルHDは40Mbps、4Kは100Mbps(ブルーレイ/Ultra HD ブルーレイの規格上限)を超えても画質は上がらないので、そこで頭打ちにする。

use crate::engine::capacity::DiscType;
use crate::engine::sidecar::resolve_tool;
use serde::{Deserialize, Serialize};

/// 判定の基準(24fpsのH.264で、1画素・1コマあたりのビット数)。
const BPP_COMFORTABLE: f64 = 0.20;
const BPP_OK: f64 = 0.12;
const BPP_TIGHT: f64 = 0.07;
const BPP_POOR: f64 = 0.04;
/// fpsが上がったときに必要なビットレートが増える指数(1.0なら比例)。
const FPS_EXPONENT: f64 = 0.6;
/// 静止・単色コマが使うビットの割合(ほぼ0だが、GOPの先頭などで少し使う)。
const STATIC_COST: f64 = 0.1;
/// ISO9660やコンテナのオーバーヘッド。
const RESERVED_BYTES: u64 = 50 * 1024 * 1024;
const CONTAINER_OVERHEAD: f64 = 0.02;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Comfortable,
    Ok,
    Tight,
    Poor,
    Impossible,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FitOption {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FitRow {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// 映像に使えるビットレート(kbps)。規格の上限で頭打ちにした値。
    pub video_kbps: f64,
    /// 規格の上限に達し、これ以上ビットを使っても画質が上がらないか。
    pub at_ceiling: bool,
    /// 頭打ちの結果、ディスクに残る空き容量(バイト)。
    pub free_bytes: u64,
    /// 実質のbpp(静止コマを除いた1コマあたり)。
    pub bpp: f64,
    pub verdict: Verdict,
    pub message_ja: String,
    pub message_en: String,
}

/// 規格上の意味のあるビットレート上限(kbps)。
pub fn ceiling_kbps(width: u32, height: u32) -> f64 {
    if width as u64 * height as u64 > 1920 * 1088 {
        100_000.0
    } else {
        40_000.0
    }
}

/// 24fps・H.264で`bpp`を保つために必要なビットレート(bps)。fpsの影響は`fps^0.6`。
fn need_bps(bpp: f64, w: u32, h: u32, fps: f64) -> f64 {
    bpp * w as f64 * h as f64 * 24.0 * (fps / 24.0).powf(FPS_EXPONENT)
}

pub fn verdict_for(available_bps: f64, w: u32, h: u32, fps: f64) -> Verdict {
    let r = available_bps / need_bps(BPP_COMFORTABLE, w, h, fps);
    let (ok, tight, poor) = (BPP_OK / BPP_COMFORTABLE, BPP_TIGHT / BPP_COMFORTABLE, BPP_POOR / BPP_COMFORTABLE);
    if r >= 1.0 {
        Verdict::Comfortable
    } else if r >= ok {
        Verdict::Ok
    } else if r >= tight {
        Verdict::Tight
    } else if r >= poor {
        Verdict::Poor
    } else {
        Verdict::Impossible
    }
}

fn messages(v: Verdict, at_ceiling: bool, free_gb: f64) -> (String, String) {
    let (ja, en) = match v {
        Verdict::Comfortable => ("余裕をもって収まります(画質はほぼ問題ありません)", "Fits with room to spare (quality should be fine)"),
        Verdict::Ok => ("収まります(標準的な画質)", "Fits (standard quality)"),
        Verdict::Tight => ("ぎりぎりです(動きの激しい場面で画質が下がることがあります)", "Tight (quality may drop in fast-moving scenes)"),
        Verdict::Poor => ("収まりますが、画質はかなり低下します", "Fits, but quality drops noticeably"),
        Verdict::Impossible => ("収まりません(画質が保てません)。長さを短くするか、解像度・fpsを下げてください", "Does not fit at usable quality — shorten it or lower the resolution / fps"),
    };
    let (mut ja, mut en) = (ja.to_string(), en.to_string());
    if at_ceiling {
        ja.push_str(&format!(" ※規格の上限に達するため、ディスクに約{free_gb:.1}GBの空きが残ります"));
        en.push_str(&format!(" (the standard's bitrate limit is reached, so ~{free_gb:.1} GB stays free)"));
    }
    (ja, en)
}

/// `disc`に、長さ`duration_secs`の映像を`opt`で収められるかを予測する。
/// `static_fraction`は、単色・静止のコマの割合(0〜1)。`audio_kbps`は同時に入れる音声のビットレート。
pub fn predict(disc: DiscType, duration_secs: f64, audio_kbps: f64, static_fraction: f64, opt: &FitOption) -> FitRow {
    let usable = disc.usable_bytes().saturating_sub(RESERVED_BYTES) as f64;
    let audio_bytes = audio_kbps * 1000.0 / 8.0 * duration_secs;
    let video_bytes = ((usable - audio_bytes) * (1.0 - CONTAINER_OVERHEAD)).max(0.0);
    let secs = duration_secs.max(1e-6);
    let avail_bps = video_bytes * 8.0 / secs;

    let ceil_bps = ceiling_kbps(opt.width, opt.height) * 1000.0;
    let at_ceiling = avail_bps > ceil_bps;
    let used_bps = avail_bps.min(ceil_bps);
    let free_bytes = if at_ceiling { ((avail_bps - ceil_bps) * secs / 8.0) as u64 } else { 0 };

    // 静止コマがほぼビットを使わない分、動きのあるコマに回せる。
    let sf = static_fraction.clamp(0.0, 0.98);
    let active = 1.0 - sf * (1.0 - STATIC_COST);
    let eff_bps = used_bps / active;
    let verdict = verdict_for(eff_bps, opt.width, opt.height, opt.fps);
    let bpp = eff_bps / (opt.width as f64 * opt.height as f64 * opt.fps.max(1.0));
    let (message_ja, message_en) = messages(verdict, at_ceiling, free_bytes as f64 / 1e9);
    FitRow { width: opt.width, height: opt.height, fps: opt.fps, video_kbps: used_bps / 1000.0, at_ceiling, free_bytes, bpp, verdict, message_ja, message_en }
}

/// 数か所を抜き取って、単色(黒)・静止しているコマの時間の割合を推定する。取得できなければ`None`。
pub fn estimate_static_fraction(input: &str, start: Option<f64>, dur: Option<f64>) -> Option<f64> {
    let probe = crate::engine::probe::probe(input).ok()?;
    let s0 = start.unwrap_or(0.0);
    let len = dur.unwrap_or((probe.duration_secs - s0).max(0.0));
    if len < 1.0 {
        return None;
    }
    let win = 20.0f64.min(len);
    let fracs = [0.05, 0.2, 0.35, 0.5, 0.65, 0.8, 0.95];
    let (mut stat, mut total) = (0.0f64, 0.0f64);
    for f in fracs {
        let t = s0 + (len - win).max(0.0) * f;
        let out = resolve_tool("ffmpeg")
            .args(["-hide_banner", "-v", "info", "-ss", &t.to_string(), "-t", &win.to_string(), "-i", input, "-an", "-vf", "blackdetect=d=0.1:pix_th=0.10,freezedetect=n=0.001:d=0.3", "-f", "null", "-"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stderr);
        let (black, freeze) = static_seconds(&text);
        stat += (black + freeze).min(win);
        total += win;
    }
    (total > 0.0).then(|| (stat / total).clamp(0.0, 1.0))
}

/// ffmpegのログから、黒の時間と静止の時間(秒)の合計を読む。
pub fn static_seconds(log: &str) -> (f64, f64) {
    let (mut black, mut freeze) = (0.0, 0.0);
    for line in log.lines() {
        if let Some(i) = line.find("black_duration:") {
            black += line[i + 15..].split_whitespace().next().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        }
        if let Some(i) = line.find("lavfi.freezedetect.freeze_duration:") {
            freeze += line[i + 35..].split_whitespace().next().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        }
    }
    (black, freeze)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(disc: DiscType, secs: f64, w: u32, h: u32, fps: f64, sf: f64) -> FitRow {
        predict(disc, secs, 320.0, sf, &FitOption { width: w, height: h, fps })
    }

    #[test]
    fn a_two_hour_film_fits_full_hd_on_a_bd25_with_room_to_spare() {
        let r = row(DiscType::Bd25, 7200.0, 1920, 1080, 24.0, 0.0);
        assert_eq!(r.verdict, Verdict::Comfortable, "{r:?}");
        assert!(!r.at_ceiling, "2時間なら26Mbps台で、フルHDの上限40Mbpsには届かない: {r:?}");
        assert!(r.video_kbps > 20_000.0 && r.video_kbps < 40_001.0, "{}", r.video_kbps);
    }

    #[test]
    fn a_two_hour_film_does_not_fit_4k_on_a_dvd() {
        let r = row(DiscType::Dvd47, 7200.0, 3840, 2160, 24.0, 0.0);
        assert!(matches!(r.verdict, Verdict::Poor | Verdict::Impossible), "{r:?}");
    }

    #[test]
    fn a_short_clip_hits_the_standards_ceiling_and_leaves_space() {
        let r = row(DiscType::Bd25, 600.0, 1920, 1080, 24.0, 0.0);
        assert!(r.at_ceiling && r.free_bytes > 1_000_000_000, "{r:?}");
        assert_eq!(r.video_kbps, 40_000.0);
        assert!(r.message_ja.contains("空きが残ります"));
        let k = row(DiscType::Bd25, 600.0, 3840, 2160, 24.0, 0.0);
        assert!(k.video_kbps <= 100_000.0);
    }

    #[test]
    fn higher_fps_needs_more_but_not_proportionally() {
        let need24 = need_bps(0.2, 3840, 2160, 24.0);
        let need120 = need_bps(0.2, 3840, 2160, 120.0);
        let ratio = need120 / need24;
        assert!(ratio > 2.0 && ratio < 3.0, "5倍のfpsでも必要量は5倍にならない: {ratio}");
        // 同じディスクなら、fpsが高いほど判定は厳しくなる。
        let v24 = row(DiscType::Bd50, 7200.0, 3840, 2160, 24.0, 0.0).verdict;
        let v120 = row(DiscType::Bd50, 7200.0, 3840, 2160, 120.0, 0.0).verdict;
        assert!((v120 as u8) >= (v24 as u8), "{v24:?} -> {v120:?}");
    }

    #[test]
    fn static_frames_make_room_for_the_rest() {
        let without = row(DiscType::Bd25, 7200.0, 3840, 2160, 60.0, 0.0);
        let with = row(DiscType::Bd25, 7200.0, 3840, 2160, 60.0, 0.5);
        assert!(with.bpp > without.bpp * 1.5, "{} vs {}", with.bpp, without.bpp);
        assert!((with.verdict as u8) <= (without.verdict as u8));
    }

    #[test]
    fn verdict_thresholds_are_ordered() {
        let (w, h, fps) = (1920, 1080, 24.0);
        let bps = |bpp: f64| bpp * w as f64 * h as f64 * fps;
        assert_eq!(verdict_for(bps(0.25), w, h, fps), Verdict::Comfortable);
        assert_eq!(verdict_for(bps(0.15), w, h, fps), Verdict::Ok);
        assert_eq!(verdict_for(bps(0.09), w, h, fps), Verdict::Tight);
        assert_eq!(verdict_for(bps(0.05), w, h, fps), Verdict::Poor);
        assert_eq!(verdict_for(bps(0.02), w, h, fps), Verdict::Impossible);
    }

    #[test]
    fn parses_black_and_freeze_durations() {
        let log = "[blackdetect @ 0x1] black_start:2 black_end:5 black_duration:3\n\
                   [freezedetect @ 0x2] lavfi.freezedetect.freeze_start: 8\n\
                   [freezedetect @ 0x2] lavfi.freezedetect.freeze_duration: 2.5\n\
                   [freezedetect @ 0x2] lavfi.freezedetect.freeze_end: 10.5\n";
        assert_eq!(static_seconds(log), (3.0, 2.5));
    }
}
