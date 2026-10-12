//! システム音声のアップサンプリング(ループバック橋渡し、Windowsのみ)。
//!
//! OSが出力している音(YouTube・ブラウザ・音楽アプリなど)を受け取り、open-barの高品質sincアップサンプル+32bitで
//! 別のDACへ排他モードで送り直す。DRMで保護された音声は受け取れないことがあり、保護を外す処理は行わない。
//! 遅延が約0.3秒入るため、音楽向け(映像と音がずれる動画には不向き)。

use open_bar::loopback::{self, Bridge, BridgeStatus};
use std::sync::{Mutex, OnceLock};

fn slot() -> &'static Mutex<Option<Bridge>> {
    static S: OnceLock<Mutex<Option<Bridge>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

pub fn devices() -> Result<Vec<(String, bool)>, String> {
    loopback::devices().map_err(|e| e.to_string())
}

pub fn start(capture: &str, output: &str, max_rate: u32, bits: u16) -> Result<BridgeStatus, String> {
    let mut g = slot().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(old) = g.take() {
        old.stop();
    }
    let b = Bridge::start(capture, output, max_rate, bits).map_err(|e| e.to_string())?;
    let st = b.status();
    *g = Some(b);
    Ok(st)
}

pub fn stop() {
    let mut g = slot().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(b) = g.take() {
        b.stop();
    }
}

pub fn status() -> Option<BridgeStatus> {
    slot().lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|b| b.status())
}
