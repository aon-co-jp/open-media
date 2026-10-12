//! AI超解像のワーカープール(CPU・GPUを同時に使う、2026-09-24新設)。
//!
//! ## 設計
//! - 1本の**待ち行列**(上限つき)に、超解像したいコマ(`Job`)を入れる。上限があるので、動画のデコードが先走ってメモリを食い尽くさない。
//! - **CPUワーカー**は1コマずつ取り出し、自前のRust実装(`cpu_sr`、AVX2+FMA)でメモリ上で処理する。
//! - **GPUワーカー**は最大`gpu_batch`コマをまとめて取り出し、`realesrgan-ncnn-vulkan`を1回起動して処理する
//!   (起動のたびにモデルの読み込みで数秒かかる〈このPCのGT 730で約3.6秒〉ため、まとめて処理して起動費を割り勘にする)。
//! - 速い方が**自然に多く取る**(早い者勝ちの作業取り合い)。ストリームの終わり際は、GPUが大きな束を抱えて
//!   CPUを待たせないよう、GPUの取り分を速度比で絞る。
//!
//! ## 正直な開示
//! CPU版とGPU版(ncnn)は別実装のため、同じコマでも出力が完全には一致しない(公式GPU実装との差はPSNR 42dB程度で、目視では区別できない)。

use crate::engine::cpu_sr::{self, SrModel};
use crate::engine::sidecar::background_command;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub struct SrSpec {
    /// `realesr-animevideov3`(アニメ・軽量)または`realesr-general-x4v3`(実写・汎用)。
    pub model: String,
    /// 出力倍率(2/3/4)。
    pub scale: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Cpu,
    /// GPUのみ。値はncnn(Vulkan)のデバイス番号。
    Gpu(u32),
    /// CPUとGPUの併用。
    Hybrid(u32),
}

pub struct Job {
    pub idx: u64,
    pub w: usize,
    pub h: usize,
    /// RGB8(HWC)。
    pub rgb: Vec<u8>,
}

pub struct Done {
    pub idx: u64,
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u8>,
    /// どの装置が処理したか("cpu" / "gpu")。
    pub by: &'static str,
}

#[derive(Clone)]
pub struct PoolConfig {
    pub spec: SrSpec,
    pub mode: Mode,
    /// `realesrgan-ncnn-vulkan`の実行ファイル(GPUを使うとき)。
    pub cli: Option<PathBuf>,
    pub models_dir: PathBuf,
    pub work_dir: PathBuf,
    /// GPUが1回に処理するコマ数の上限。
    pub gpu_batch: usize,
    /// 待ち行列の上限。
    pub queue_cap: usize,
    /// GPUの取り分(0〜1、GPUの処理速度 ÷ (GPU+CPU)。終わり際の配分に使う)。
    pub gpu_share: f64,
    /// CPUワーカーが使うスレッド数(`0`=全コア)。
    pub cpu_threads: usize,
}

struct Shared {
    queue: Mutex<VecDeque<Job>>,
    not_empty: Condvar,
    not_full: Condvar,
    closed: AtomicBool,
    stop: AtomicBool,
    cap: usize,
}

pub struct SrPool {
    shared: Arc<Shared>,
    rx: mpsc::Receiver<Result<Done, String>>,
    handles: Vec<JoinHandle<()>>,
}

/// コマを入れる側(スレッド間で共有できる)。結果を受け取る`SrPool`とは別のスレッドで使える。
#[derive(Clone)]
pub struct Submitter {
    shared: Arc<Shared>,
}

impl Submitter {
    /// コマを待ち行列へ入れる。行列がいっぱいの間は待つ。中止・停止されたらエラー。
    pub fn submit(&self, job: Job) -> Result<(), String> {
        let mut q = self.shared.queue.lock().unwrap();
        while q.len() >= self.shared.cap {
            if self.shared.stop.load(Ordering::SeqCst) || crate::progress::is_cancelled() {
                return Err(crate::progress::CANCELLED.to_string());
            }
            let (g, _) = self.shared.not_full.wait_timeout(q, Duration::from_millis(200)).unwrap();
            q = g;
        }
        if self.shared.stop.load(Ordering::SeqCst) {
            return Err("AI超解像のワーカーが停止しました / the upscaling workers have stopped".to_string());
        }
        q.push_back(job);
        self.shared.not_empty.notify_all();
        Ok(())
    }

    /// これ以上コマを入れないことを伝える(残りを処理し終えたらワーカーは終了する)。
    pub fn close(&self) {
        self.shared.closed.store(true, Ordering::SeqCst);
        self.shared.not_empty.notify_all();
    }
}

impl SrPool {
    pub fn start(cfg: PoolConfig) -> Result<SrPool, String> {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
            closed: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            cap: cfg.queue_cap.max(2),
        });
        let (tx, rx) = mpsc::channel::<Result<Done, String>>();
        let mut handles = Vec::new();

        let use_cpu = matches!(cfg.mode, Mode::Cpu | Mode::Hybrid(_));
        let gpu_idx = match cfg.mode {
            Mode::Gpu(i) | Mode::Hybrid(i) => Some(i),
            Mode::Cpu => None,
        };

        if use_cpu {
            // モデルはここで読み込み、失敗はすぐ呼び出し元へ返す。
            let model = Arc::new(cpu_sr::load_named(&cfg.models_dir, &cfg.spec.model, cfg.spec.scale)?);
            let (sh, txc, threads) = (shared.clone(), tx.clone(), cfg.cpu_threads);
            handles.push(std::thread::spawn(move || cpu_worker(sh, txc, model, threads)));
        }
        if let Some(idx) = gpu_idx {
            let cli = cfg.cli.clone().ok_or("GPUで処理するにはrealesrgan-ncnn-vulkanが必要です")?;
            let (sh, txg, c) = (shared.clone(), tx.clone(), cfg.clone());
            let mut c = c;
            if !use_cpu {
                c.gpu_share = 1.0; // 他に処理する担当がいないので、最後の1コマまでGPUが取る
            }
            handles.push(std::thread::spawn(move || gpu_worker(sh, txg, cli, idx, c)));
        }
        drop(tx);
        Ok(SrPool { shared, rx, handles })
    }

    /// コマを入れる側のハンドルを得る(別スレッドから使える)。
    pub fn submitter(&self) -> Submitter {
        Submitter { shared: self.shared.clone() }
    }

    pub fn submit(&self, job: Job) -> Result<(), String> {
        self.submitter().submit(job)
    }

    pub fn close(&self) {
        self.submitter().close()
    }

    /// 結果を1つ受け取る(最大`timeout`待つ)。`Ok(None)`は時間切れ、`Err(())`は全ワーカー終了。
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<Result<Done, String>>, ()> {
        match self.rx.recv_timeout(timeout) {
            Ok(r) => Ok(Some(r)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(()),
        }
    }

    /// ワーカーを止めて片付ける。
    pub fn shutdown(self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.closed.store(true, Ordering::SeqCst);
        self.shared.not_empty.notify_all();
        self.shared.not_full.notify_all();
        for h in self.handles {
            let _ = h.join();
        }
    }
}

fn mean_of(rgb: &[u8]) -> f64 {
    if rgb.is_empty() {
        return 0.0;
    }
    let step = (rgb.len() / 20000).max(1);
    let (mut sum, mut n) = (0u64, 0u64);
    for &b in rgb.iter().step_by(step) {
        sum += b as u64;
        n += 1;
    }
    sum as f64 / n as f64
}

/// 出力が明らかに壊れていないか(真っ黒・極端に暗い)を平均の明るさで確かめる。
pub fn output_looks_sane(input: &[u8], output: &[u8]) -> bool {
    let (mi, mo) = (mean_of(input), mean_of(output));
    !(mi > 12.0 && mo < mi * 0.4)
}

/// 出力が壊れていたら、双三次拡大で置き換える(`by`は"fallback"になる)。
fn checked(j: &Job, w: usize, h: usize, rgb: Vec<u8>, by: &'static str) -> Done {
    if output_looks_sane(&j.rgb, &rgb) {
        return Done { idx: j.idx, w, h, rgb, by };
    }
    let plain = image::RgbImage::from_raw(j.w as u32, j.h as u32, j.rgb.clone())
        .map(|src| image::imageops::resize(&src, w as u32, h as u32, image::imageops::FilterType::CatmullRom).into_raw())
        .unwrap_or(rgb);
    Done { idx: j.idx, w, h, rgb: plain, by: "fallback" }
}

fn should_stop(sh: &Shared) -> bool {
    sh.stop.load(Ordering::SeqCst) || crate::progress::is_cancelled()
}

fn cpu_worker(sh: Arc<Shared>, tx: mpsc::Sender<Result<Done, String>>, model: Arc<SrModel>, threads: usize) {
    loop {
        let job = {
            let mut q = sh.queue.lock().unwrap();
            loop {
                if should_stop(&sh) {
                    return;
                }
                if let Some(j) = q.pop_front() {
                    sh.not_full.notify_all();
                    break j;
                }
                if sh.closed.load(Ordering::SeqCst) {
                    return;
                }
                let (g, _) = sh.not_empty.wait_timeout(q, Duration::from_millis(200)).unwrap();
                q = g;
            }
        };
        let rgb32: Vec<f32> = job.rgb.iter().map(|&b| b as f32 / 255.0).collect();
        let (out, ow, oh) = cpu_sr::upscale_full_threads(&model, &rgb32, job.w, job.h, threads);
        let bytes: Vec<u8> = out.iter().map(|&v| (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8).collect();
        if tx.send(Ok(checked(&job, ow, oh, bytes, "cpu"))).is_err() {
            return;
        }
    }
}

/// GPUワーカーが次に取る束を決めて取り出す。行列が浅いうちは束が満ちるのを待ち、終わり際は速度比で取り分を絞る。
fn take_gpu_batch(sh: &Shared, max_b: usize, share: f64) -> Option<Vec<Job>> {
    let mut q = sh.queue.lock().unwrap();
    loop {
        if should_stop(sh) {
            return None;
        }
        let closed = sh.closed.load(Ordering::SeqCst);
        if q.len() >= max_b || (closed && !q.is_empty()) {
            let want = if closed { ((q.len() as f64 * share).floor() as usize).clamp(0, max_b) } else { max_b };
            if want == 0 {
                // 終わり際で、GPUに任せるほどの量が無い。CPUに任せて終了する。
                return None;
            }
            let n = want.min(q.len());
            let batch: Vec<Job> = q.drain(..n).collect();
            sh.not_full.notify_all();
            return Some(batch);
        }
        if closed && q.is_empty() {
            return None;
        }
        let (g, _) = sh.not_empty.wait_timeout(q, Duration::from_millis(200)).unwrap();
        q = g;
    }
}

fn gpu_worker(sh: Arc<Shared>, tx: mpsc::Sender<Result<Done, String>>, cli: PathBuf, gpu: u32, cfg: PoolConfig) {
    let mut batch_no = 0u64;
    while let Some(batch) = take_gpu_batch(&sh, cfg.gpu_batch.max(1), cfg.gpu_share.clamp(0.0, 1.0)) {
        batch_no += 1;
        match run_gpu_batch(&cli, gpu, &cfg, batch_no, batch) {
            Ok(done) => {
                for d in done {
                    if tx.send(Ok(d)).is_err() {
                        return;
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(Err(e));
                sh.stop.store(true, Ordering::SeqCst);
                sh.not_full.notify_all();
                return;
            }
        }
    }
}

/// 束のコマをPNGにして`realesrgan-ncnn-vulkan`へ渡し、結果を読み戻す。
pub fn run_gpu_batch(cli: &Path, gpu: u32, cfg: &PoolConfig, batch_no: u64, jobs: Vec<Job>) -> Result<Vec<Done>, String> {
    let dir = cfg.work_dir.join(format!("gpu-{}-{batch_no}", std::process::id()));
    let (din, dout) = (dir.join("in"), dir.join("out"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&din).and_then(|_| std::fs::create_dir_all(&dout)).map_err(|e| format!("作業フォルダを作れません: {e}"))?;
    let cleanup = || {
        let _ = std::fs::remove_dir_all(&dir);
    };
    for (i, j) in jobs.iter().enumerate() {
        if let Err(e) = write_png_fast(&din.join(format!("{i:08}.png")), j.w, j.h, &j.rgb) {
            cleanup();
            return Err(e);
        }
    }
    let scale_arg = if cfg.spec.model == "realesr-animevideov3" { cfg.spec.scale } else { 4 };
    let out = background_command(cli)
        .args(["-i", &din.to_string_lossy(), "-o", &dout.to_string_lossy(), "-m", &cfg.models_dir.to_string_lossy()])
        .args(["-n", &cfg.spec.model, "-s", &scale_arg.to_string(), "-f", "png", "-g", &gpu.to_string(), "-j", "1:2:2"])
        .output()
        .map_err(|e| {
            cleanup();
            format!("realesrgan-ncnn-vulkanの起動に失敗しました: {e}")
        })?;
    if !out.status.success() {
        cleanup();
        return Err(format!(
            "GPUでのAI超解像に失敗しました: {}",
            String::from_utf8_lossy(&out.stderr).lines().filter(|l| !l.contains('%')).collect::<Vec<_>>().join(" / ")
        ));
    }
    let mut done = Vec::with_capacity(jobs.len());
    for (i, j) in jobs.iter().enumerate() {
        let img = match image::open(dout.join(format!("{i:08}.png"))) {
            Ok(im) => im.to_rgb8(),
            Err(e) => {
                cleanup();
                return Err(format!("GPUの出力を読めません: {e}"));
            }
        };
        let (mut w, mut h) = (img.width() as usize, img.height() as usize);
        let mut rgb = img.into_raw();
        // x4専用モデルで2倍・3倍が必要な場合は、ここで双三次縮小する(CPU版の後段リサイズと同じ)。
        if scale_arg == 4 && cfg.spec.scale < 4 {
            let (tw, th) = (j.w * cfg.spec.scale as usize, j.h * cfg.spec.scale as usize);
            if let Some(src) = image::RgbImage::from_raw(w as u32, h as u32, rgb.clone()) {
                let r = image::imageops::resize(&src, tw as u32, th as u32, image::imageops::FilterType::CatmullRom);
                (w, h, rgb) = (tw, th, r.into_raw());
            }
        }
        done.push(checked(j, w, h, rgb, "gpu"));
    }
    cleanup();
    Ok(done)
}

/// 中間ファイル用なので圧縮を最小にして、書き出し時間を短くする。
pub fn write_png_fast(path: &Path, w: usize, h: usize, rgb: &[u8]) -> Result<(), String> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;
    let f = std::fs::File::create(path).map_err(|e| format!("{}を作れません: {e}", path.display()))?;
    PngEncoder::new_with_quality(std::io::BufWriter::new(f), CompressionType::Fast, FilterType::NoFilter)
        .write_image(rgb, w as u32, h as u32, image::ExtendedColorType::Rgb8)
        .map_err(|e| format!("PNGの書き出しに失敗しました: {e}"))
}

/// `realesrgan-ncnn-vulkan`の標準エラー出力から、Vulkanのデバイス一覧`[番号 名前]`を読み取る。
pub fn parse_gpu_list(stderr: &str) -> Vec<(u32, String)> {
    stderr
        .lines()
        .filter(|l| l.contains("queueC="))
        .filter_map(|l| {
            let inner = l.trim_start().strip_prefix('[')?.split(']').next()?;
            let (idx, name) = inner.split_once(' ')?;
            Some((idx.parse().ok()?, name.trim().to_string()))
        })
        .collect()
}

/// 動作確認・測定用の、決まった内容のテスト画像(グラデーション+輪郭+ノイズ)。畳み込みの計算量は内容によらない。
pub fn synthetic_frame(w: usize, h: usize, seed: u64) -> Vec<u8> {
    let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 40) as u8
    };
    let mut v = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let edge = if ((x / 40) + (y / 40)) % 2 == 0 { 30 } else { 0 };
            let base = [(x * 255 / w) as u8, (y * 255 / h) as u8, ((x + y) * 255 / (w + h)) as u8];
            for c in 0..3 {
                v[(y * w + x) * 3 + c] = base[c].saturating_add(edge).saturating_add(rnd() % 12);
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_real_ncnn_device_line() {
        let s = "[0 NVIDIA GeForce GT 730]  queueC=0[16]  queueG=0[16]  queueT=1[1]\n[0 NVIDIA GeForce GT 730]  bugsbn1=0  bugbilz=0  bugcopc=0  bugihfa=0\n";
        assert_eq!(parse_gpu_list(s), vec![(0, "NVIDIA GeForce GT 730".to_string())]);
        let two = "[0 Intel(R) UHD Graphics]  queueC=0[1]\n[1 AMD Radeon RX 6600]  queueC=0[8]\n";
        assert_eq!(parse_gpu_list(two), vec![(0, "Intel(R) UHD Graphics".to_string()), (1, "AMD Radeon RX 6600".to_string())]);
        assert!(parse_gpu_list("no gpu here").is_empty());
    }

    #[test]
    fn synthetic_frames_are_deterministic_and_differ_by_seed() {
        assert_eq!(synthetic_frame(64, 48, 1), synthetic_frame(64, 48, 1));
        assert_ne!(synthetic_frame(64, 48, 1), synthetic_frame(64, 48, 2));
    }

    #[test]
    fn sanity_check_catches_black_outputs() {
        let bright = vec![120u8; 3000];
        assert!(output_looks_sane(&bright, &vec![118u8; 12000]));
        assert!(!output_looks_sane(&bright, &vec![0u8; 12000]));
        // 元も暗いコマは、暗い出力でも正常とみなす
        assert!(output_looks_sane(&vec![2u8; 3000], &vec![0u8; 12000]));
    }

    #[test]
    fn a_black_output_is_replaced_by_a_plain_upscale() {
        let j = Job { idx: 7, w: 4, h: 4, rgb: vec![200u8; 48] };
        let d = checked(&j, 8, 8, vec![0u8; 192], "gpu");
        assert_eq!(d.by, "fallback");
        assert_eq!((d.w, d.h, d.rgb.len()), (8, 8, 192));
        assert!(d.rgb.iter().all(|&v| v > 190));
    }
}
