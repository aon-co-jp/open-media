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

pub fn play(path: &str, mode: &str, treble: bool, treble_all: bool, bits: u16) -> Result<(), String> {
    let m = parse_mode(mode)?;
    if !std::path::Path::new(path).exists() {
        return Err(format!("ファイルがありません: {path}"));
    }
    with(|p| {
        p.stop();
        p.set_mode(m);
        p.set_treble_restore(treble);
        p.set_treble_all(treble_all);
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
pub fn set_treble_all(on: bool) { with(|p| p.set_treble_all(on)) }
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
        play(wav.to_str().unwrap(), "shared", false, false, 24).unwrap();
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

    /// 実機テスト: アプリと同じ手順(MQA判定→再生開始→音量→250msごとの状況確認)で再生し、固まらないか確かめる。
    /// `cargo test --lib app_sequence -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn app_sequence_cd_track_plays() {
        let wav = std::env::temp_dir().join("md_cd_test").join("Track01.wav");
        assert!(wav.exists(), "先に real_cd テストでTrack01.wavを作ってください");
        let path = wav.to_str().unwrap();
        println!("mqa={}", is_mqa(path));
        play(path, "exclusive_upsample", false, false, 32).unwrap();
        set_volume(1.0);
        let t0 = std::time::Instant::now();
        let mut last = String::new();
        while t0.elapsed().as_secs() < 45 {
            let st = status();
            let line = format!("state={:?} pos={:.2} dur={:.1} out={}Hz msg={} route={}", st.state, st.position_secs, st.duration_secs, st.out_rate_hz, st.message, st.route_en);
            if line != last {
                println!("[{:>5.1}s] {line}", t0.elapsed().as_secs_f64());
                last = line;
            }
            if st.position_secs > 3.0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        let st = status();
        stop();
        assert!(st.position_secs > 3.0, "アプリと同じ手順だと再生が進まない");
    }

    /// 実機テスト(音楽CDが入った光学ドライブ D: が必要): CDのTOCを読み、1曲を取り込み、
    /// 排他モード+高品質アップサンプル+32bitで再生して、位置が進み出力レートが上がることを確かめる。
    /// `cargo test --lib real_cd -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_cd_plays_via_exclusive_upsample_32bit() {
        let drive = std::env::var("OPEN_MEDIA_CD").unwrap_or_else(|_| "D:".to_string());
        let tracks = crate::engine::cdda::list_tracks(&drive).expect("TOCを読めない");
        println!("tracks: {}", tracks.iter().filter(|t| t.is_audio).count());
        let first = tracks.iter().find(|t| t.is_audio).expect("音声トラックが無い");
        let dir = std::env::temp_dir().join("md_cd_test");
        let outs = crate::engine::cdda::rip_tracks(&drive, &[first.number], &dir, false).expect("取り込めない");
        println!("ripped: {} ({} bytes)", outs[0], std::fs::metadata(&outs[0]).map(|m| m.len()).unwrap_or(0));
        play(&outs[0], "exclusive_upsample", false, false, 32).unwrap();
        set_volume(0.3);
        let t0 = std::time::Instant::now();
        while status().position_secs < 2.0 && t0.elapsed().as_secs() < 40 {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let st = status();
        println!("state={:?} pos={:.2} dur={:.1} src={}Hz out={}Hz mode={} route={} note={} msg={} underruns={}", st.state, st.position_secs, st.duration_secs, st.source_rate_hz, st.out_rate_hz, st.active_mode.id, st.route_en, st.note, st.message, st.underrun_frames);
        stop();
        assert!(st.position_secs >= 2.0, "再生が進まない");
        assert_eq!(st.source_rate_hz, 44_100);
        assert!(st.out_rate_hz > 44_100, "アップサンプルされていない");
        assert_eq!(st.underrun_frames, 0, "音切れ");
    }
}
