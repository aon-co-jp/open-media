//! 長時間処理の進捗通知と中止(2026-09-24新設)。
//!
//! AI超解像・フレーム補間のように数時間〜数日かかり得る処理では、(1)画面に進捗と残り時間を出す、
//! (2)途中で中止できる、の2つが必須になる。エンジン(`engine/*`)はTauriのAppHandleを知らなくてよいよう、
//! ここに「通知の出口(sink)」を1つだけ登録し、エンジンは`emit`を呼ぶだけにしている。
//! 出口が未登録(単体テスト・CLI)のときは何もしない。

use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

type Sink = Box<dyn Fn(&str, Value) + Send + Sync>;

static SINK: OnceLock<Sink> = OnceLock::new();
static CANCEL: AtomicBool = AtomicBool::new(false);

/// 通知の出口を登録する(アプリ起動時に1回だけ)。
pub fn set_sink(f: Sink) {
    let _ = SINK.set(f);
}

/// 進捗などの通知を送る。`kind`は`"ai-progress"`など、`payload`はJSON。
pub fn emit(kind: &str, payload: Value) {
    if let Some(s) = SINK.get() {
        s(kind, payload);
    }
}

/// 実行中の処理へ中止を依頼する。処理は次の区切りで止まり、途中経過は再開用に残す。
pub fn request_cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

/// 中止の依頼を取り下げる(新しい処理を始めるとき)。
pub fn clear_cancel() {
    CANCEL.store(false, Ordering::SeqCst);
}

pub fn is_cancelled() -> bool {
    CANCEL.load(Ordering::SeqCst)
}

/// 中止されたことを示すエラー文字列(呼び出し側が「失敗」ではなく「中断」と区別するため)。
pub const CANCELLED: &str = "中止しました(途中経過は残してあります。同じ設定でもう一度実行すると続きから再開します) / Cancelled (progress is kept; run again with the same settings to resume)";

/// 秒を「1時間23分」「45秒」のような日英どちらでも読める短い表記にする。
pub fn fmt_duration(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    let (d, h, m, sec) = (s / 86400, (s % 86400) / 3600, (s % 3600) / 60, s % 60);
    if d > 0 {
        format!("{d}日{h}時間 / {d}d {h}h")
    } else if h > 0 {
        format!("{h}時間{m}分 / {h}h {m}m")
    } else if m > 0 {
        format!("{m}分{sec}秒 / {m}m {sec}s")
    } else {
        format!("{sec}秒 / {sec}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_flag_round_trips() {
        clear_cancel();
        assert!(!is_cancelled());
        request_cancel();
        assert!(is_cancelled());
        clear_cancel();
        assert!(!is_cancelled());
    }

    #[test]
    fn duration_is_human_readable() {
        assert_eq!(fmt_duration(45.0), "45秒 / 45s");
        assert_eq!(fmt_duration(125.0), "2分5秒 / 2m 5s");
        assert_eq!(fmt_duration(3900.0), "1時間5分 / 1h 5m");
        assert_eq!(fmt_duration(90000.0), "1日1時間 / 1d 1h");
    }
}
