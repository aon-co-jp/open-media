//! このPCのCPUとGPUの速さを実測して、AI処理に使う装置を自動で選ぶ(2026-09-24新設)。
//!
//! ## なぜ実測するのか(実機で確認した事実)
//! これまでの「自動」は「Vulkan対応GPUが動けばGPU、無ければCPU」だった。ところがこのPC(Ryzen 32スレッド+GT 730)では、
//! 720×480の1コマあたり **CPUが約1.2秒、GPUが約3.9秒**(しかもGPUは1回起動するたびに約3.6秒の起動費)で、
//! 「GPUが動くから使う」は約3倍遅くなっていた。GPUが速いPCではその逆になる。**どちらが速いかは実測しないと分からない**。
//!
//! ## 測るもの
//! 1. CPU単独の1コマの時間(自前のAVX2+FMA実装、全スレッド)。
//! 2. 各GPUの起動費と1コマの時間(1コマの実行と3コマの実行の差から分離)。
//! 3. **CPUとGPUを同時に動かしたときの両者の速度**(CPUの数コアをGPUの読み書きが使うため、単独時より遅くなる)。
//! 4. 上の実測から、CPUのみ/GPUのみ/併用のうち一番速いものを選ぶ(併用の得が8%未満なら、簡単な単独を選ぶ)。
//!
//! 結果は`plugins/hw-bench.json`に保存し、CPUの種類・スレッド数などが変わらない限り再測定しない。

use crate::engine::sr_pool::{self, Job, Mode, PoolConfig, SrSpec};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

const BENCH_VERSION: u32 = 1;
/// 測定に使うコマの大きさ(DVD相当)。実際の処理時間は画素数に比例するとみなして換算する。
pub const BENCH_W: usize = 720;
pub const BENCH_H: usize = 480;
/// GPUの起動費が全体の何割以内に収まる束の大きさを選ぶか。
const GPU_OVERHEAD_TARGET: f64 = 0.15;
/// 併用が単独の何倍以上速いときだけ併用を選ぶ(スケジューリングの無駄を見込んだ余裕)。
const HYBRID_MIN_GAIN: f64 = 1.08;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuBench {
    pub name: String,
    pub threads: usize,
    pub kernel: String,
    pub secs_per_frame: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuBench {
    pub index: u32,
    pub name: String,
    /// 1回の起動で必ずかかる時間(モデルの読み込みなど)。
    pub startup_secs: f64,
    pub secs_per_frame: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwBench {
    pub version: u32,
    pub signature: String,
    pub measured_at: u64,
    pub cpu: Option<CpuBench>,
    pub gpus: Vec<GpuBench>,
    /// 併用時(GPUが動いている間)のCPUの1コマの時間。
    pub cpu_under_load_secs: Option<f64>,
    /// 併用時(CPUが動いている間)のGPUの1コマの時間。
    pub gpu_under_load_secs: Option<f64>,
    /// `"cpu"` / `"gpu"` / `"hybrid"`。
    pub choice: String,
    pub gpu_index: Option<u32>,
    pub gpu_batch: usize,
    pub gpu_share: f64,
    /// 選ばれた方式での処理速度(720×480換算、コマ/秒)。
    pub cpu_fps: f64,
    pub gpu_fps: f64,
    pub hybrid_fps: f64,
    pub message_ja: String,
    pub message_en: String,
}

/// 判断の入力(1コマ=720×480あたりの秒)。
#[derive(Debug, Clone, Copy)]
pub struct GpuTimes {
    pub startup: f64,
    pub per_frame: f64,
    pub per_frame_under_load: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub choice: &'static str,
    pub gpu_batch: usize,
    pub gpu_share: f64,
    pub cpu_fps: f64,
    pub gpu_fps: f64,
    pub hybrid_fps: f64,
}

/// GPUの1束(`b`コマ)あたりの処理速度(コマ/秒)。起動費を束の中で割り勘にする。
fn gpu_batch_fps(startup: f64, per_frame: f64, b: usize) -> f64 {
    b as f64 / (startup + b as f64 * per_frame)
}

/// 起動費が全体の`GPU_OVERHEAD_TARGET`以内になる最小の束の大きさ(2〜48)。
pub fn pick_gpu_batch(startup: f64, per_frame: f64) -> usize {
    if per_frame <= 0.0 {
        return 48;
    }
    ((startup / (GPU_OVERHEAD_TARGET * per_frame)).ceil() as usize).clamp(2, 48)
}

/// 実測値から使う方式を決める(純粋な計算)。
pub fn decide(cpu_secs: Option<f64>, cpu_under_load: Option<f64>, gpu: Option<GpuTimes>) -> Decision {
    let cpu_fps = cpu_secs.filter(|s| *s > 0.0).map_or(0.0, |s| 1.0 / s);
    let (gpu_batch, gpu_fps, gpu_fps_load) = match gpu {
        Some(g) if g.per_frame > 0.0 => {
            let b = pick_gpu_batch(g.startup, g.per_frame);
            (b, gpu_batch_fps(g.startup, g.per_frame, b), gpu_batch_fps(g.startup, g.per_frame_under_load.max(g.per_frame), b))
        }
        _ => (2, 0.0, 0.0),
    };
    let cpu_fps_load = cpu_under_load.filter(|s| *s > 0.0).map_or(cpu_fps, |s| 1.0 / s);
    let hybrid_fps = (cpu_fps_load + gpu_fps_load) * 0.97;
    let gpu_share = if cpu_fps_load + gpu_fps_load > 0.0 { gpu_fps_load / (cpu_fps_load + gpu_fps_load) } else { 0.0 };
    let best_single = cpu_fps.max(gpu_fps);
    let choice = if cpu_fps == 0.0 && gpu_fps == 0.0 {
        "cpu"
    } else if cpu_fps > 0.0 && gpu_fps > 0.0 && hybrid_fps >= HYBRID_MIN_GAIN * best_single {
        "hybrid"
    } else if gpu_fps > cpu_fps {
        "gpu"
    } else {
        "cpu"
    };
    Decision { choice, gpu_batch, gpu_share, cpu_fps, gpu_fps, hybrid_fps }
}

fn cache_path() -> Option<PathBuf> {
    Some(crate::engine::plugins::plugin_dir()?.join("hw-bench.json"))
}

fn cpu_name() -> String {
    if let Ok(v) = std::env::var("PROCESSOR_IDENTIFIER") {
        return v;
    }
    if let Ok(s) = std::fs::read_to_string("/proc/cpuinfo") {
        if let Some(l) = s.lines().find(|l| l.starts_with("model name")) {
            return l.split(':').nth(1).unwrap_or("").trim().to_string();
        }
    }
    "unknown".to_string()
}

fn cpu_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

fn signature() -> String {
    format!("v{BENCH_VERSION}|{}|{}|{}|{}", cpu_name(), cpu_threads(), crate::engine::cpu_sr::kernel_name(), if cfg!(debug_assertions) { "debug" } else { "release" })
}

/// 保存済みの測定結果(CPUの構成が同じときだけ有効)。
pub fn cached() -> Option<HwBench> {
    let b: HwBench = serde_json::from_slice(&std::fs::read(cache_path()?).ok()?).ok()?;
    (b.version == BENCH_VERSION && b.signature == signature()).then_some(b)
}

fn save(b: &HwBench) {
    if let Some(p) = cache_path() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(j) = serde_json::to_vec_pretty(b) {
            let _ = std::fs::write(p, j);
        }
    }
}

/// 選ばれた方式を`sr_pool`の実行方式へ。
pub fn mode_of(b: &HwBench) -> Mode {
    match (b.choice.as_str(), b.gpu_index) {
        ("gpu", Some(i)) => Mode::Gpu(i),
        ("hybrid", Some(i)) => Mode::Hybrid(i),
        _ => Mode::Cpu,
    }
}

/// 選ばれた方式での、`w`×`h`のコマ1枚あたりの秒数(画素数に比例させて換算)。
pub fn secs_per_frame(b: &HwBench, w: usize, h: usize) -> f64 {
    let scale = (w * h) as f64 / (BENCH_W * BENCH_H) as f64;
    let fps = match b.choice.as_str() {
        "hybrid" => b.hybrid_fps,
        "gpu" => b.gpu_fps,
        _ => b.cpu_fps,
    };
    if fps > 0.0 { scale / fps } else { f64::INFINITY }
}

fn models_dir_and_cli() -> Result<(PathBuf, PathBuf), String> {
    let exe = crate::engine::ai_upscale::ensure_plugin()?;
    let models = exe.parent().ok_or("プラグインの場所が不正です")?.join("models");
    Ok((models, exe))
}

fn gpu_cfg(models: &std::path::Path, work: &std::path::Path, mode: Mode, cli: Option<PathBuf>) -> PoolConfig {
    PoolConfig {
        spec: SrSpec { model: "realesr-animevideov3".into(), scale: 2 },
        mode,
        cli,
        models_dir: models.to_path_buf(),
        work_dir: work.to_path_buf(),
        gpu_batch: 1,
        queue_cap: 4,
        gpu_share: 0.5,
        cpu_threads: 0,
    }
}

fn frames(n: usize, seed0: u64) -> Vec<Job> {
    (0..n).map(|i| Job { idx: i as u64, w: BENCH_W, h: BENCH_H, rgb: sr_pool::synthetic_frame(BENCH_W, BENCH_H, seed0 + i as u64) }).collect()
}

/// 測定する。`force`が偽で有効な保存結果があればそれを返す。`say`には進行状況の文を渡す。
pub fn benchmark(force: bool, say: &dyn Fn(String)) -> Result<HwBench, String> {
    if !force {
        if let Some(b) = cached() {
            return Ok(b);
        }
    }
    say("AI処理用のプラグインを確認しています… / Checking the AI plugin…".into());
    let (models, cli) = models_dir_and_cli()?;
    let work = std::env::temp_dir().join(format!("make-disk-bench-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&work);
    let result = measure(&models, &cli, &work, say);
    let _ = std::fs::remove_dir_all(&work);
    let b = result?;
    save(&b);
    Ok(b)
}

fn measure(models: &std::path::Path, cli: &std::path::Path, work: &std::path::Path, say: &dyn Fn(String)) -> Result<HwBench, String> {
    let threads = cpu_threads();

    // 1) CPU単独。
    say(format!("CPUの速さを測っています({threads}スレッド)… / Measuring the CPU ({threads} threads)…"));
    let cpu_model = crate::engine::cpu_sr::load_named(models, "realesr-animevideov3", 2)?;
    let f32frame: Vec<f32> = sr_pool::synthetic_frame(BENCH_W, BENCH_H, 7).iter().map(|&b| b as f32 / 255.0).collect();
    let mut best = f64::INFINITY;
    for _ in 0..2 {
        let t = Instant::now();
        let _ = crate::engine::cpu_sr::upscale_full_threads(&cpu_model, &f32frame, BENCH_W, BENCH_H, 0);
        best = best.min(t.elapsed().as_secs_f64());
    }
    let cpu = CpuBench { name: cpu_name(), threads, kernel: crate::engine::cpu_sr::kernel_name().to_string(), secs_per_frame: best };

    // 2) GPUの一覧と、それぞれの起動費・1コマの時間。
    say("GPUを探しています… / Looking for GPUs…".into());
    let tiny = work.join("tiny");
    let _ = std::fs::create_dir_all(&tiny);
    let _ = sr_pool::write_png_fast(&tiny.join("a.png"), 16, 16, &vec![128u8; 16 * 16 * 3]);
    let listing = crate::engine::sidecar::background_command(cli)
        .args(["-i", &tiny.join("a.png").to_string_lossy(), "-o", &tiny.join("b.png").to_string_lossy(), "-m", &models.to_string_lossy(), "-n", "realesr-animevideov3", "-s", "2", "-g", "0"])
        .output();
    let devices = listing.map(|o| sr_pool::parse_gpu_list(&String::from_utf8_lossy(&o.stderr))).unwrap_or_default();

    let mut gpus: Vec<GpuBench> = Vec::new();
    for (idx, name) in devices.iter().take(4) {
        say(format!("GPU「{name}」の速さを測っています… / Measuring GPU \"{name}\"…"));
        let cfg = gpu_cfg(models, work, Mode::Gpu(*idx), Some(cli.to_path_buf()));
        let t = Instant::now();
        if sr_pool::run_gpu_batch(cli, *idx, &cfg, 1, frames(1, 100)).is_err() {
            continue; // このGPUでは動かない
        }
        let t1 = t.elapsed().as_secs_f64();
        let per_frame;
        let startup;
        if t1 > 90.0 {
            // 非常に遅いGPU。3コマの追加測定は省き、起動費を3.6秒と仮定して見積もる。
            per_frame = (t1 - 3.6).max(0.1);
            startup = t1 - per_frame;
        } else {
            let t = Instant::now();
            if sr_pool::run_gpu_batch(cli, *idx, &cfg, 2, frames(3, 200)).is_err() {
                continue;
            }
            let t3 = t.elapsed().as_secs_f64();
            per_frame = ((t3 - t1) / 2.0).max(0.001);
            startup = (t1 - per_frame).max(0.0);
        }
        gpus.push(GpuBench { index: *idx, name: name.clone(), startup_secs: startup, secs_per_frame: per_frame });
    }
    let best_gpu = gpus.iter().min_by(|a, b| a.secs_per_frame.partial_cmp(&b.secs_per_frame).unwrap()).cloned();

    // 3) CPUとGPUを同時に動かしたときの両者の速度。
    let (mut cpu_load, mut gpu_load) = (None, None);
    if let Some(g) = &best_gpu {
        say("CPUとGPUを同時に動かしたときの速さを測っています… / Measuring CPU and GPU together…".into());
        let reserve = 2.min(threads.saturating_sub(1));
        let cfg = gpu_cfg(models, work, Mode::Gpu(g.index), Some(cli.to_path_buf()));
        let (cli2, idx) = (cli.to_path_buf(), g.index);
        let handle = std::thread::spawn(move || {
            let t = Instant::now();
            let r = sr_pool::run_gpu_batch(&cli2, idx, &cfg, 3, frames(3, 300));
            (r.is_ok(), t.elapsed().as_secs_f64())
        });
        let (mut n, t_start) = (0u32, Instant::now());
        while !handle.is_finished() {
            let _ = crate::engine::cpu_sr::upscale_full_threads(&cpu_model, &f32frame, BENCH_W, BENCH_H, threads - reserve);
            n += 1;
        }
        let cpu_elapsed = t_start.elapsed().as_secs_f64();
        let (ok, gpu_t3) = handle.join().unwrap_or((false, 0.0));
        if n > 0 && ok {
            // 最後の1コマはGPUの終了と重なるので、完了したコマ数で割る(やや控えめな見積もりになる)。
            cpu_load = Some((cpu_elapsed / n as f64).max(cpu.secs_per_frame));
            gpu_load = Some(((gpu_t3 - g.startup_secs) / 3.0).max(g.secs_per_frame));
        }
    }

    // 4) 決定。
    let d = decide(
        Some(cpu.secs_per_frame),
        cpu_load,
        best_gpu.as_ref().map(|g| GpuTimes { startup: g.startup_secs, per_frame: g.secs_per_frame, per_frame_under_load: gpu_load.unwrap_or(g.secs_per_frame) }),
    );
    let gpu_desc_ja = best_gpu.as_ref().map(|g| format!("GPU「{}」は1コマ{:.2}秒(起動ごとに{:.1}秒)", g.name, g.secs_per_frame, g.startup_secs)).unwrap_or_else(|| "使えるGPUは見つかりませんでした".into());
    let gpu_desc_en = best_gpu.as_ref().map(|g| format!("GPU \"{}\" takes {:.2} s per frame (+{:.1} s per launch)", g.name, g.secs_per_frame, g.startup_secs)).unwrap_or_else(|| "no usable GPU was found".into());
    let (choice_ja, choice_en) = match d.choice {
        "hybrid" => (format!("CPUとGPUの併用(CPUのみより約{:.0}%速い)", (d.hybrid_fps / d.cpu_fps.max(1e-9) - 1.0) * 100.0), "CPU and GPU together".to_string()),
        "gpu" => ("GPUのみ".to_string(), "GPU only".to_string()),
        _ => (if best_gpu.is_some() { "CPUのみ(このPCではGPUより速い、または併用しても得が小さいため)" } else { "CPUのみ" }.to_string(), "CPU only".to_string()),
    };
    let message_ja = format!(
        "測定結果(720×480の1コマ): CPU({}、{}スレッド、{})は{:.2}秒 / {}。自動選択: {}。",
        cpu.name, cpu.threads, cpu.kernel, cpu.secs_per_frame, gpu_desc_ja, choice_ja
    );
    let message_en = format!(
        "Measured (one 720x480 frame): CPU ({} threads, {}) takes {:.2} s; {}. Auto-selected: {}.",
        cpu.threads, cpu.kernel, cpu.secs_per_frame, gpu_desc_en, choice_en
    );
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    Ok(HwBench {
        version: BENCH_VERSION,
        signature: signature(),
        measured_at: now,
        cpu: Some(cpu),
        gpu_index: best_gpu.as_ref().map(|g| g.index),
        gpus,
        cpu_under_load_secs: cpu_load,
        gpu_under_load_secs: gpu_load,
        choice: d.choice.to_string(),
        gpu_batch: d.gpu_batch,
        gpu_share: d.gpu_share,
        cpu_fps: d.cpu_fps,
        gpu_fps: d.gpu_fps,
        hybrid_fps: d.hybrid_fps,
        message_ja,
        message_en,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_pcs_real_numbers_choose_cpu_only() {
        // 実測: CPU 1.2秒、GT 730は起動3.6秒+1コマ3.86秒(併用時は両方やや遅くなる)。
        let d = decide(Some(1.2), Some(1.5), Some(GpuTimes { startup: 3.6, per_frame: 3.86, per_frame_under_load: 4.2 }));
        assert_eq!(d.choice, "cpu", "{d:?}");
        assert_eq!(d.gpu_batch, 7);
    }

    #[test]
    fn a_fast_gpu_is_chosen_alone() {
        let d = decide(Some(1.2), Some(1.4), Some(GpuTimes { startup: 1.0, per_frame: 0.05, per_frame_under_load: 0.06 }));
        assert_eq!(d.choice, "gpu", "{d:?}");
        assert_eq!(d.gpu_batch, 48);
    }

    #[test]
    fn similar_speeds_are_used_together() {
        // CPUとGPUがほぼ同じ速さで、併用の干渉も小さいなら併用が最速。
        let d = decide(Some(1.0), Some(1.05), Some(GpuTimes { startup: 0.5, per_frame: 0.9, per_frame_under_load: 0.95 }));
        assert_eq!(d.choice, "hybrid", "{d:?}");
        assert!(d.gpu_share > 0.3 && d.gpu_share < 0.7);
    }

    #[test]
    fn without_a_gpu_it_is_cpu_only() {
        assert_eq!(decide(Some(2.0), None, None).choice, "cpu");
    }

    #[test]
    fn gpu_batch_amortizes_the_startup_cost() {
        assert_eq!(pick_gpu_batch(3.6, 3.86), 7);
        assert_eq!(pick_gpu_batch(0.0, 1.0), 2);
        assert_eq!(pick_gpu_batch(100.0, 0.001), 48);
    }

    #[test]
    fn time_scales_with_pixels() {
        let b = HwBench {
            version: BENCH_VERSION, signature: String::new(), measured_at: 0, cpu: None, gpus: vec![], cpu_under_load_secs: None, gpu_under_load_secs: None,
            choice: "cpu".into(), gpu_index: None, gpu_batch: 2, gpu_share: 0.0, cpu_fps: 1.0, gpu_fps: 0.0, hybrid_fps: 0.0, message_ja: String::new(), message_en: String::new(),
        };
        assert!((secs_per_frame(&b, 720, 480) - 1.0).abs() < 1e-9);
        assert!((secs_per_frame(&b, 1440, 960) - 4.0).abs() < 1e-9);
    }
    /// 実機(プラグイン導入済みのPC)での測定。`cargo test -- --ignored real_benchmark --nocapture`で手動実行する。
    #[test]
    #[ignore]
    fn real_benchmark_measures_this_pc() {
        let t = Instant::now();
        let b = benchmark(true, &|m| eprintln!("  … {m}")).expect("benchmark");
        eprintln!("BENCH took {:.1}s
{}
{}", t.elapsed().as_secs_f64(), b.message_ja, b.message_en);
        eprintln!("{}", serde_json::to_string_pretty(&b).unwrap());
        assert!(b.cpu_fps > 0.0);
        assert!(cached().is_some(), "結果が保存され、次回は再測定されない");
    }

    /// CPU・GPU・併用の3方式で同じコマを処理し、出力の大きさと、CPU版とGPU版のPSNRを確認する。
    #[test]
    #[ignore]
    fn real_pool_runs_cpu_gpu_and_hybrid() {
        let (models, cli) = models_dir_and_cli().expect("plugin");
        let work = std::env::temp_dir().join(format!("make-disk-pooltest-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let run = |mode: Mode, n: usize| -> Vec<sr_pool::Done> {
            let mut cfg = gpu_cfg(&models, &work, mode, Some(cli.clone()));
            cfg.gpu_batch = 2;
            cfg.queue_cap = 6;
            cfg.gpu_share = 0.4;
            let pool = sr_pool::SrPool::start(cfg).expect("start");
            for j in frames(n, 500) {
                pool.submit(j).unwrap();
            }
            pool.close();
            let mut out = Vec::new();
            while let Ok(r) = pool.recv_timeout(std::time::Duration::from_secs(120)) {
                if let Some(r) = r {
                    out.push(r.expect("frame"));
                }
            }
            pool.shutdown();
            out.sort_by_key(|d| d.idx);
            out
        };
        let t = Instant::now();
        let cpu = run(Mode::Cpu, 2);
        eprintln!("cpu x2: {:.1}s", t.elapsed().as_secs_f64());
        let t = Instant::now();
        let gpu = run(Mode::Gpu(0), 2);
        eprintln!("gpu x2: {:.1}s", t.elapsed().as_secs_f64());
        let t = Instant::now();
        let hyb = run(Mode::Hybrid(0), 6);
        let by: Vec<_> = hyb.iter().map(|d| d.by).collect();
        eprintln!("hybrid x6: {:.1}s  by={by:?}", t.elapsed().as_secs_f64());
        for d in cpu.iter().chain(gpu.iter()).chain(hyb.iter()) {
            assert_eq!((d.w, d.h), (BENCH_W * 2, BENCH_H * 2));
            assert_eq!(d.rgb.len(), d.w * d.h * 3);
        }
        assert_eq!(hyb.len(), 6);
        // 同じコマ(seed=500)のCPU出力とGPU出力のPSNR。
        let mse: f64 = cpu[0].rgb.iter().zip(&gpu[0].rgb).map(|(a, b)| (*a as f64 - *b as f64).powi(2)).sum::<f64>() / cpu[0].rgb.len() as f64;
        let psnr = 10.0 * (255.0f64 * 255.0 / mse.max(1e-9)).log10();
        eprintln!("PSNR(cpu vs gpu) = {psnr:.1} dB");
        assert!(psnr > 35.0, "CPU版とGPU版は近い出力のはず: {psnr}");
        let _ = std::fs::remove_dir_all(&work);
    }

    /// 同梱した実写モデル(realesr-general-x4v3、自前でncnn形式へ変換)が、公式のGPU実装(NCNN-Vulkan)と同じ出力になるか。
    /// 変換の誤り(重みの並び・層の対応)があれば、ここで大きくずれる。
    #[test]
    #[ignore]
    fn real_general_model_cpu_matches_the_official_gpu() {
        let cli = crate::engine::ai_upscale::ensure_plugin().expect("plugin");
        let models = cli.parent().unwrap().join("models");
        assert!(models.join("realesr-general-x4v3.bin").is_file());
        let work = std::env::temp_dir().join(format!("make-disk-generaltest-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let run = |mode: Mode| -> sr_pool::Done {
            let mut cfg = gpu_cfg(&models, &work, mode, Some(cli.clone()));
            cfg.spec = sr_pool::SrSpec { model: "realesr-general-x4v3".to_string(), scale: 2 };
            cfg.gpu_batch = 1;
            cfg.queue_cap = 4;
            let pool = sr_pool::SrPool::start(cfg).expect("start");
            pool.submit(frames(1, 700).remove(0)).unwrap();
            pool.close();
            let mut out = None;
            while let Ok(r) = pool.recv_timeout(std::time::Duration::from_secs(300)) {
                if let Some(r) = r {
                    out = Some(r.expect("frame"));
                }
            }
            pool.shutdown();
            out.expect("no output")
        };
        let cpu = run(Mode::Cpu);
        let gpu = run(Mode::Gpu(0));
        assert_eq!((cpu.w, cpu.h), (BENCH_W * 2, BENCH_H * 2));
        assert_eq!(cpu.by, "cpu");
        assert_eq!(gpu.by, "gpu");
        let mse: f64 = cpu.rgb.iter().zip(&gpu.rgb).map(|(a, b)| (*a as f64 - *b as f64).powi(2)).sum::<f64>() / cpu.rgb.len() as f64;
        let psnr = 10.0 * (255.0f64 * 255.0 / mse.max(1e-9)).log10();
        eprintln!("general model: PSNR(cpu vs official gpu) = {psnr:.1} dB");
        assert!(psnr > 35.0, "自前変換したモデルが公式GPU実装とずれています: {psnr}");
        let _ = std::fs::remove_dir_all(&work);
    }

}
