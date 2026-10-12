//! ディスクプレーヤーの音声エンジン(open-barの取り込み、2026-10-12)。
//!
//! 音声(CD・音声ファイル・DSD)はopen-barの`Player`で再生する: 共有モードの高品質アップサンプル、
//! 排他モードのビットパーフェクト、排他+アップサンプル、DoP(DSDのまま)、ギャップレス、高音補正(MP3向け)。
//! 映像(DVD/Blu-ray/動画)はWebViewで再生し、映像+別音声の組み合わせでは音声を時間の基準にして映像を追従させる。
//! MQAは復号せず、判別だけを行う(open-barの方針どおり)。

use open_bar::combo::{self, Combo};
use open_bar::player::{mode_text, ModeText, PlayMode, Player, Status};
use std::sync::{Mutex, OnceLock};

fn player() -> &'static Mutex<Player> {
    static P: OnceLock<Mutex<Player>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(Player::new()))
}

fn with<R>(f: impl FnOnce(&Player) -> R) -> R {
    let g = player().lock().unwrap_or_else(|e| e.into_inner());
    f(&g)
}

pub fn parse_mode(s: &str) -> Result<PlayMode, String> {
    match s {
        "shared" => Ok(PlayMode::Shared),
        "exclusive" => Ok(PlayMode::Exclusive),
        "exclusive_upsample" => Ok(PlayMode::ExclusiveUpsample),
        "dop" => Ok(PlayMode::Dop),
        _ => Err(format!("不明な再生モード: {s}")),
    }
}

pub fn modes() -> Vec<ModeText> {
    [PlayMode::Shared, PlayMode::Exclusive, PlayMode::ExclusiveUpsample, PlayMode::Dop].into_iter().map(mode_text).collect()
}

pub fn play(path: &str, mode: &str, treble: bool, bits: u16) -> Result<(), String> {
    let m = parse_mode(mode)?;
    if !std::path::Path::new(path).exists() {
        return Err(format!("ファイルがありません: {path}"));
    }
    with(|p| {
        p.stop();
        p.set_mode(m);
        p.set_treble_restore(treble);
        p.set_bits(bits);
        p.set_playlist(vec![path.to_string()]);
        p.play(0);
    });
    Ok(())
}

pub fn pause() { with(|p| p.pause()) }
pub fn resume() { with(|p| p.resume()) }
pub fn stop() { with(|p| p.stop()) }
pub fn seek(secs: f64) { with(|p| p.seek(secs)) }
pub fn set_volume(v: f32) { with(|p| p.set_volume(v)) }
pub fn set_mode(mode: &str) -> Result<(), String> {
    let m = parse_mode(mode)?;
    with(|p| p.set_mode(m));
    Ok(())
}
pub fn set_treble(on: bool) { with(|p| p.set_treble_restore(on)) }
pub fn status() -> Status { with(|p| p.status()) }
pub fn is_mqa(path: &str) -> bool { open_bar::mqa::is_mqa_file(path) }
pub fn auto_pair(dir: &str) -> Vec<Combo> { combo::auto_pair(dir) }

/// 映像+音声の組み合わせを検証して`.obar.json`へ保存する(make-diskの書き出しとopen-barで共通の形式)。
pub fn save_combo(video: &str, audio: &str, audio_offset_ms: i64, out_path: &str) -> Result<(), String> {
    let c = Combo { video: video.to_string(), audio: audio.to_string(), audio_offset_ms };
    c.validate().map_err(|e| e.to_string())?;
    std::fs::write(out_path, c.to_json()).map_err(|e| format!("保存できません: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実機の既定オーディオデバイスで、生成した正弦波WAVを再生して位置が進むことを確かめる
    /// (`cargo test bar_player -- --ignored --nocapture`。デバイスが無いCIでは走らせない)。
    #[test]
    #[ignore]
    fn plays_a_real_file_and_position_advances() {
        let dir = std::env::temp_dir().join("md_bar_test");
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("sine.wav");
        let ok = crate::engine::sidecar::resolve_tool("ffmpeg")
            .args(["-y", "-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=3", "-ar", "44100"])
            .arg(&wav)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "ffmpegでテスト用WAVを作れません");
        play(wav.to_str().unwrap(), "shared", false, 24).unwrap();
        // デバッグビルドは遅いので、再生が始まる(位置が進む)まで最大30秒待つ。
        let t0 = std::time::Instant::now();
        while status().position_secs < 0.3 && t0.elapsed().as_secs() < 30 {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let st = status();
        println!("state={:?} pos={:.2} dur={:.2} out={}Hz route={} msg={}", st.state, st.position_secs, st.duration_secs, st.out_rate_hz, st.route_en, st.message);
        assert!(st.position_secs > 0.3, "位置が進んでいない: {}", st.position_secs);
        assert!((st.duration_secs - 3.0).abs() < 0.2);
        pause();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let p1 = status().position_secs;
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert!((status().position_secs - p1).abs() < 0.05, "一時停止中に進んでいる");
        resume();
        seek(2.0);
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert!(status().position_secs >= 1.9);
        stop();
        assert!(!is_mqa(wav.to_str().unwrap()));
    }
}
