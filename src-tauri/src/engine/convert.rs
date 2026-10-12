//! FFmpegによるフォーマット変換・ビットレート制御・時間トリミング。

use crate::engine::cpu;
use crate::engine::sidecar::resolve_tool;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
#[cfg(test)]
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BitrateMode {
    /// 固定ビットレート(kbps)
    Fixed(u64),
    /// ディスク容量から自動算出した最大平均ビットレート(kbps)
    AutoMaxForCapacity(u64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrimRange {
    /// 開始位置(秒)。省略時は先頭から。
    pub start_secs: Option<f64>,
    /// 長さ(秒)。省略時は末尾まで。
    pub duration_secs: Option<f64>,
}

/// 「カットしたい範囲」(1本の動画に対して複数指定可能)。
/// 例: 「最初の0:00〜2:00をカット」「途中の1:00:00〜1:05:00をカット」
/// 「最後の4:50:00〜末尾をカット」の3つを同時に指定できる。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CutRange {
    pub start_secs: f64,
    /// 省略時はファイル末尾までカットする(「最後の◯◯から末尾まで」用)。
    pub end_secs: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertJob {
    pub input_path: String,
    pub output_path: String,
    /// 出力コンテナ/コーデックのffmpeg引数(例: "-c:v libx264 -c:a aac")は
    /// フォーマットプリセット側で組み立てて渡す。
    pub codec_args: Vec<String>,
    pub bitrate: Option<BitrateMode>,
    /// 単一区間の簡易トリミング(開始+長さ)。`cut_ranges`指定時は無視される。
    pub trim: Option<TrimRange>,
    /// 複数区間のカット指定。指定時は`trim`より優先する。
    ///
    /// 実装方針(数時間規模の動画でも実用的な速度で処理するため):
    /// 1. 既定(`frame_accurate: false`)では、残す各区間を`-c copy`
    ///    (ストリームコピー、エンコード無し)で高速に抜き出す
    ///    (キーフレーム単位の精度になる点はffmpegの一般的な制約)。
    /// 2. `frame_accurate: true`の場合は、区間の境界をフレーム単位で
    ///    正確に切るため抽出時にエンコードが必要になるが、CPUソフト
    ///    エンコード(libx264等)ではなく、GPU(NVENC/QuickSync/AMF)の
    ///    ハードウェアエンコーダが検出できればそちらを優先して使う
    ///    ことで、5時間級の動画でも実用的な速度で処理する
    ///    (`hw_encoder`参照)。
    /// 3. 抜き出した区間をconcat demuxerで結合する際に、フォーマット
    ///    変換・ビットレート指定が必要な場合のみ1回だけエンコードする
    ///    (区間ごとに毎回エンコードし直さない)。
    pub cut_ranges: Option<Vec<CutRange>>,
    /// trueならカット境界をフレーム単位で正確に切る(要エンコード、
    /// ただしGPUハードウェアエンコーダを自動検出して使用)。
    /// falseなら`-c copy`でキーフレーム単位の高速カットを行う。
    #[serde(default)]
    pub frame_accurate: bool,
    /// 出力解像度(2026-09-17新設)。ユーザー指示「DVDは通常解像度
    /// 720×480とフルHDを選択可能に、ブルーレイはフルHDと4Kを選べる
    /// ようにして、ビデオ出力は、720X480から4Kや5Kや8Kも指定可能に」
    /// への対応。`None`なら元の解像度のまま(無変換)。
    #[serde(default)]
    pub resolution: Option<Resolution>,
    /// 出力フレームレート(fps、2026-09-17新設)。ユーザー指示
    /// 「FPSは、最低不明、24FPS、30FPS 60FPS 120FPなども選択や指定可能に」
    /// への対応。`None`(「不明」=元のフレームレートのまま)なら無変換。
    #[serde(default)]
    pub fps: Option<u32>,
    /// AIノイズ除去(2026-09-19新設)。本物のニューラルネット(RNNoise、ffmpegの
    /// `arnndn`フィルタ)で音声のノイズを低減する。`None`なら無効。
    #[serde(default)]
    pub ai_denoise: Option<AiDenoise>,
    /// DSD出力(2026-09-19新設)。`Some(64|128|256|512|1024)`ならDSF(`.dsf`)で書き出す。
    /// ffmpegはDSDをエンコードできないため`engine::dsd`の自前変換器を使う。
    #[serde(default)]
    pub dsd_rate: Option<u32>,
    /// DSD出力と同時にDoP(DSD over PCM)のWAV(`<出力名>.dop.wav`)も書き出す。`Some(24|32)`でコンテナのビット深度
    /// (open-mqa連携、2026-09-19)。DoP対応DAC+ビットパーフェクト再生専用(`dsd::dsf_to_dop_wav`参照)。
    #[serde(default)]
    pub dop_wav_bits: Option<u8>,
    /// MKV出力のとき、元ファイルの全ての音声・字幕・添付トラックを保持する(2026-09-19新設、`mkv_tracks`参照)。
    /// 省略時は保持する(MKV以外の出力では無視)。`Some(false)`でffmpegの既定(各1本のみ)に戻せる。
    #[serde(default)]
    pub mkv_keep_all_tracks: Option<bool>,
    /// MKV出力へ多重化する追加の音声・字幕トラック(別ファイル)。
    #[serde(default)]
    pub extra_tracks: Vec<crate::engine::mkv_tracks::ExtraTrack>,
    /// AI超解像(2026-09-19新設)。Real-ESRGAN(GPU/NCNN-Vulkan)で映像を拡大してから、
    /// 通常のコーデック/解像度/ビットレート指定を適用する。短いクリップ向け(`ai_upscale`モジュール参照)。
    #[serde(default)]
    pub ai_upscale: Option<crate::engine::ai_upscale::AiUpscale>,
    /// 音声の帯域拡張(AI、2026-09-19新設)。低域を保持し、生成した高域を入力の包絡で頭打ちにする
    /// (`audio_sr`モジュール参照)。音声専用出力(またはDSD)向け。
    #[serde(default)]
    pub audio_bwe: Option<AudioBwe>,
}

/// 音声の帯域拡張の設定。`cutoff_hz`が`None`なら入力の帯域のカットオフを自動検出し、
/// 帯域が欠けていない音源には何もしない。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioBwe {
    #[serde(default)]
    pub cutoff_hz: Option<f32>,
    /// 声(TTS・ポッドキャスト・電話など)向け。カットオフの自動検出を、音楽向けの「崖」検出ではなく、
    /// 声のなだらかな高域の減衰に合わせた検出にする(`audio_sr::detect_speech_rolloff_hz`)。音楽には使わない。
    #[serde(default)]
    pub speech: bool,
}

/// AIノイズ除去の設定。`mix`は原音とのブレンド(-1.0〜1.0、1.0で完全適用、
/// 負値は除去した「ノイズ成分」側)。RNNoiseは主に音声で学習されたモデルで、
/// 音楽では効果が控えめ・高域が鈍る場合があるため、既定は弱め。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiDenoise {
    pub mix: f32,
}

/// 同梱のRNNoise学習済みモデル(GregorR/rnnoise-models の marathon-prescription、
/// 作者が「著作権の対象外」と明記)。バイナリへ埋め込み、使用時に一時ファイルへ書き出す
/// (ffmpegの`arnndn`はファイルパスを要求するため。Tauriのリソース配置に依存せず
/// 全OSで同じ動作にできる)。
static RNNOISE_MODEL: &[u8] = include_bytes!("../../models/rnnoise-general.rnnn");

fn rnnoise_model_path() -> Result<std::path::PathBuf, String> {
    let path = std::env::temp_dir().join(format!("make-disk-rnnoise-general-{}.rnnn", RNNOISE_MODEL.len()));
    if fs::metadata(&path).map(|m| m.len() as usize).ok() != Some(RNNOISE_MODEL.len()) {
        fs::write(&path, RNNOISE_MODEL).map_err(|e| format!("AIモデルの展開に失敗しました: {e}"))?;
    }
    Ok(path)
}

/// `-af arnndn=...`を追加する。フィルタ文字列内のパスでは`:`をバックスラッシュで
/// エスケープする必要があり、`\`はパス区切りと紛らわしいため`/`へ置き換える。
fn push_ai_denoise_args(args: &mut Vec<String>, denoise: &Option<AiDenoise>) -> Result<(), String> {
    if let Some(d) = denoise {
        let p = rnnoise_model_path()?.to_string_lossy().replace('\\', "/").replace(':', "\\\\:");
        args.push("-af".into());
        args.push(format!("arnndn=m={p}:mix={}", d.mix.clamp(-1.0, 1.0)));
    }
    Ok(())
}

/// 出力動画の解像度(幅×高さ、ピクセル)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

/// カットしたい範囲の一覧から、残す(keepする)区間の一覧を求める。
/// 戻り値の各要素は(開始秒, 終了秒。Noneならファイル末尾まで)。
fn keep_segments_from_cuts(cuts: &[CutRange]) -> Vec<(f64, Option<f64>)> {
    let mut sorted: Vec<&CutRange> = cuts.iter().collect();
    sorted.sort_by(|a, b| a.start_secs.partial_cmp(&b.start_secs).unwrap_or(std::cmp::Ordering::Equal));

    let mut keep = Vec::new();
    let mut prev_end: f64 = 0.0;
    let mut reached_eof_cut = false;

    for cut in sorted {
        if reached_eof_cut {
            break; // 既に末尾までカット済みなので、それ以降のカット指定は無視
        }
        if cut.start_secs > prev_end {
            keep.push((prev_end, Some(cut.start_secs)));
        }
        match cut.end_secs {
            Some(e) => prev_end = prev_end.max(e),
            None => reached_eof_cut = true,
        }
    }
    if !reached_eof_cut {
        keep.push((prev_end, None));
    }
    keep
}

pub fn run_convert(job: &ConvertJob) -> Result<(), String> {
    if let Some(bwe) = &job.audio_bwe {
        if !(is_audio_only_output(&job.output_path) || job.dsd_rate.is_some()) {
            return Err("音声の帯域拡張は音声専用の出力(またはDSD)でのみ使えます / audio bandwidth extension needs an audio-only output".to_string());
        }
        if job.cut_ranges.as_ref().is_some_and(|c| !c.is_empty()) {
            return Err("音声の帯域拡張ではカット区間の指定は未対応です(開始位置+長さのトリミングは可) / cut ranges are not supported with bandwidth extension".to_string());
        }
        // 1) 元音声を48kHz/32bit floatのWAVへ展開(トリミング適用。AIノイズ除去は帯域拡張の**前**に行う:
        //    逆順だと拡張器がノイズから高域を作ってしまう)。2) 帯域拡張。3) 結果を入力に通常の変換を行う。
        let tag = format!("{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        let (decoded, extended) = (std::env::temp_dir().join(format!("make-disk-bwe-in-{tag}.wav")), std::env::temp_dir().join(format!("make-disk-bwe-out-{tag}.wav")));
        let mut args: Vec<String> = Vec::new();
        if let Some(t) = &job.trim {
            if let Some(start) = t.start_secs {
                args.extend(["-ss".into(), start.to_string()]);
            }
        }
        args.extend(["-i".into(), job.input_path.clone()]);
        if let Some(t) = &job.trim {
            if let Some(d) = t.duration_secs {
                args.extend(["-t".into(), d.to_string()]);
            }
        }
        args.extend(["-vn".into(), "-ac".into(), "2".into()]);
        push_ai_denoise_args(&mut args, &job.ai_denoise)?;
        args.extend(["-ar".into(), "48000".into(), "-c:a".into(), "pcm_f32le".into(), "-y".into(), decoded.to_string_lossy().to_string()]);
        let prep = run_ffmpeg(&merge_audio_filters(args));
        let result = prep.and_then(|_| crate::engine::audio_sr::extend_wav_file(&decoded, &extended, bwe.cutoff_hz, bwe.speech)).and_then(|_| {
            let mut next = job.clone();
            next.input_path = extended.to_string_lossy().to_string();
            next.trim = None;
            next.audio_bwe = None;
            next.ai_denoise = None; // 既に前段で適用済み
            run_convert(&next)
        });
        let _ = fs::remove_file(&decoded);
        let _ = fs::remove_file(&extended);
        return result;
    }
    if let Some(up) = &job.ai_upscale {
        if job.cut_ranges.as_ref().is_some_and(|c| !c.is_empty()) {
            return Err("AI超解像ではカット区間の指定は未対応です(開始位置+長さのトリミングは可) / cut ranges are not supported with AI upscaling".to_string());
        }
        // 1) AI超解像した映像+元の音声の中間ファイルを作り、2) それを入力に通常の変換を行う。
        let mezzanine = std::path::Path::new(&job.output_path).with_extension("ai-upscaled.mkv");
        let trim = job.trim.as_ref().map(|t| (t.start_secs, t.duration_secs));
        let target = job.resolution.as_ref().map(|r| (r.width, r.height));
        if up.interpolate_only {
            crate::engine::ai_video::make_interpolated_mezzanine(&job.input_path, trim, up, &mezzanine)?;
        } else {
            crate::engine::ai_video::make_upscaled_mezzanine(&job.input_path, trim, up, target, &mezzanine)?;
        }
        let mut next = job.clone();
        next.input_path = mezzanine.to_string_lossy().to_string();
        next.trim = None;
        next.ai_upscale = None;
        let result = run_convert(&next);
        let _ = fs::remove_file(&mezzanine);
        return result;
    }
    if let Some(mult) = job.dsd_rate {
        if job.cut_ranges.as_ref().is_some_and(|c| !c.is_empty()) {
            return Err("DSD出力ではカット区間の指定は未対応です(開始位置+長さのトリミングは可) / cut ranges are not supported with DSD output".to_string());
        }
        let trim = job.trim.as_ref().map(|t| (t.start_secs, t.duration_secs));
        crate::engine::dsd::convert_to_dsf(&job.input_path, &job.output_path, mult, trim)?;
        if let Some(bits) = job.dop_wav_bits {
            let dop_path = std::path::Path::new(&job.output_path).with_extension("dop.wav");
            crate::engine::dsd::dsf_to_dop_wav(&job.output_path, &dop_path.to_string_lossy(), bits)?;
        }
        return Ok(());
    }
    if let Some(cuts) = &job.cut_ranges {
        if !cuts.is_empty() {
            return run_convert_with_cut_ranges(job, cuts);
        }
    }
    run_convert_simple(job)
}

/// 使えるAV1エンコーダを選ぶ(2026-09-19新設)。高速なlibsvtav1が使えればそれを、
/// 無ければlibaom-av1を、どちらも無ければNone。ffmpegのビルドごとに含まれる
/// エンコーダが違うため、実際に`-encoders`の一覧を見て判断する。
fn detect_av1_encoder() -> Option<(&'static str, Vec<&'static str>)> {
    static CACHE: std::sync::OnceLock<Option<&'static str>> = std::sync::OnceLock::new();
    let name = CACHE.get_or_init(|| {
        let out = resolve_tool("ffmpeg").args(["-hide_banner", "-encoders"]).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        ["libsvtav1", "libaom-av1"].into_iter().find(|e| text.contains(e))
    });
    name.map(|n| (n, if n == "libaom-av1" { vec!["-cpu-used", "6", "-row-mt", "1"] } else { vec!["-preset", "8"] }))
}

/// チャンネル構成の変換先(2026-09-28新設、ユーザー指示:
/// 「モノラルや2CHステレオから擬似的に5.1CHや7.1CHへ」「サラウンドから2CHやモノラルへ」)。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelLayoutTarget {
    /// 変更しない(既定)。
    Keep,
    Mono,
    Stereo,
    /// 5.1ch(FL/FR/FC/LFE/BL/BR)。
    Surround51,
    /// 7.1ch(5.1ch+SL/SR)。
    Surround71,
}

/// 元のチャンネル数と変換先から、ffmpegへ渡す引数(`-ac`または`-af pan=...`)を作る。
///
/// **正直な開示**: モノラル・ステレオから5.1ch/7.1chへの変換は、元に無い音を新たに
/// 「生成」するわけではない、単純な複製・按分による疑似(バーチャル)サラウンドである
/// (実際のディスクリート5.1ch/7.1ch音源が持つ情報量には及ばない)。センターは左右の
/// 単純ミックス、リア/サイドは前方の減衰コピー、LFEは常に無音にする。
/// 元から3ch以上ある場合、5.1ch/7.1chへの変換は対象外(何もしない)——想定外の
/// チャンネルマッピングを避けるため。サラウンド→ステレオ/モノラルの各`-ac`降ミックスは、
/// ffmpeg(libswresample)の標準機能によるもので、疑似ではない。
pub fn channel_layout_args(source_channels: u32, target: ChannelLayoutTarget) -> Vec<String> {
    match target {
        ChannelLayoutTarget::Keep => vec![],
        ChannelLayoutTarget::Mono => vec!["-ac".to_string(), "1".to_string()],
        ChannelLayoutTarget::Stereo => vec!["-ac".to_string(), "2".to_string()],
        ChannelLayoutTarget::Surround51 if source_channels <= 2 => {
            let pan = if source_channels <= 1 {
                "pan=5.1|FL=c0|FR=c0|FC=c0|LFE=0|BL=c0|BR=c0"
            } else {
                "pan=5.1|FL=FL|FR=FR|FC=0.5*FL+0.5*FR|LFE=0|BL=0.6*FL|BR=0.6*FR"
            };
            vec!["-af".to_string(), pan.to_string()]
        }
        ChannelLayoutTarget::Surround71 if source_channels <= 2 => {
            let pan = if source_channels <= 1 {
                "pan=7.1|FL=c0|FR=c0|FC=c0|LFE=0|BL=c0|BR=c0|SL=c0|SR=c0"
            } else {
                "pan=7.1|FL=FL|FR=FR|FC=0.5*FL+0.5*FR|LFE=0|BL=0.6*FL|BR=0.6*FR|SL=0.4*FL|SR=0.4*FR"
            };
            vec!["-af".to_string(), pan.to_string()]
        }
        // 元から3ch以上(既にサラウンド等)の場合、5.1ch/7.1chへの疑似変換は対象外。
        ChannelLayoutTarget::Surround51 | ChannelLayoutTarget::Surround71 => vec![],
    }
}

/// codec_args内の疑似コーデック`-c:v av1`を、実際に使えるAV1エンコーダへ置き換える。
fn resolve_av1_codec_args(codec_args: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < codec_args.len() {
        if codec_args[i] == "-c:v" && codec_args.get(i + 1).map(String::as_str) == Some("av1") {
            let (enc, extra) = detect_av1_encoder().ok_or("このffmpegにはAV1エンコーダ(libsvtav1/libaom-av1)がありません / this ffmpeg build has no AV1 encoder")?;
            out.push("-c:v".to_string());
            out.push(enc.to_string());
            out.extend(extra.into_iter().map(String::from));
            i += 2;
        } else if codec_args[i] == "-af" && codec_args.get(i + 1).is_some_and(|v| v == "hq-resample" || v.starts_with("hq-resample@")) {
            // 疑似フィルタ`hq-resample[@出力レートHz]`を、使える最高品質のリサンプラ+ディザへ置き換える。
            // レートはフィルタ内(out_sample_rate)で指定する——`-ar`だと後段に標準設定のリサンプラが
            // 追加されて高品質設定が無駄になるため。
            let rate = codec_args[i + 1].strip_prefix("hq-resample@");
            out.push("-af".to_string());
            out.push(match rate {
                Some(r) => format!("{}:out_sample_rate={r}", hq_resample_filter()),
                None => hq_resample_filter().to_string(),
            });
            i += 2;
        } else {
            out.push(codec_args[i].clone());
            i += 1;
        }
    }
    Ok(merge_audio_filters(out))
}

/// 使える最高品質のリサンプラ(+TPDF系ディザ)のフィルタ文字列(2026-09-19新設、音質最優先)。
/// soxr(`precision=33`、最高精度)が使えればそれを、無ければswresampleの高精度設定
/// (長いフィルタ・高いカットオフ)を使う。ffmpegのビルドごとにsoxrの有無が違い、
/// このセッションの開発機のffmpegには無かった(実機確認)ため、実際に試して判断する。
pub fn hq_resample_filter() -> &'static str {
    static CACHE: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let soxr_ok = resolve_tool("ffmpeg")
            .args(["-hide_banner", "-v", "error", "-f", "lavfi", "-i", "sine=d=0.05", "-af", "aresample=resampler=soxr", "-f", "null", "-"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if soxr_ok {
            "aresample=resampler=soxr:precision=33:dither_method=triangular_hp"
        } else {
            "aresample=filter_size=256:phase_shift=13:cutoff=0.995:dither_method=triangular_hp"
        }
    })
}

/// 複数の`-af`(例: hq-resampleとAIノイズ除去)を、1つのフィルタチェーン(`,`連結)にまとめる。
/// ffmpegは同じ出力に`-af`を複数指定すると最後のものしか使わないため、そのままだと片方が無視される。
fn merge_audio_filters(args: Vec<String>) -> Vec<String> {
    let mut chain: Vec<String> = Vec::new();
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-af" && i + 1 < args.len() {
            chain.push(args[i + 1].clone());
            i += 2;
        } else {
            rest.push(args[i].clone());
            i += 1;
        }
    }
    if !chain.is_empty() {
        // arnndn(AIノイズ除去)は48kHz専用なので、高レート化するリサンプルより**前**に置く
        // (後ろだと48kHzへ戻されてしまう、実機テストで確認)。
        chain.sort_by_key(|f| if f.starts_with("arnndn") { 0 } else { 1 });
        rest.push("-af".to_string());
        rest.push(chain.join(","));
    }
    rest
}

fn run_convert_simple(job: &ConvertJob) -> Result<(), String> {
    let mut args: Vec<String> = Vec::new();

    // MKV出力のときだけ複数トラック(元の全音声・字幕+追加トラック)を扱う。追加トラックの`-i`は、出力側の`-t`より前に置く
    // (`-t`は直後の入力にも掛かってしまうため)。
    let mkv = crate::engine::mkv_tracks::is_mkv_output(&job.output_path) && !is_audio_only_output(&job.output_path);
    let extras: &[crate::engine::mkv_tracks::ExtraTrack] = if mkv { &job.extra_tracks } else { &[] };
    if let Some(trim) = &job.trim {
        if let Some(start) = trim.start_secs {
            args.push("-ss".into());
            args.push(start.to_string());
        }
        args.push("-i".into());
        args.push(job.input_path.clone());
        args.extend(crate::engine::mkv_tracks::extra_input_args(extras, trim.start_secs));
        if let Some(dur) = trim.duration_secs {
            args.push("-t".into());
            args.push(dur.to_string());
        }
    } else {
        args.push("-i".into());
        args.push(job.input_path.clone());
        args.extend(crate::engine::mkv_tracks::extra_input_args(extras, None));
    }

    if is_audio_only_output(&job.output_path) {
        args.push("-vn".into()); // 動画入力から音声だけを取り出す場合に映像ストリームを含めない
    }
    let codec_args = resolve_av1_codec_args(&job.codec_args)?;
    // 「必要な部分だけ切り出す(高速)」(trim指定)のときは、切り出した部分の再エンコードも速くする:
    // H.264はGPUのハードウェアエンコーダ(NVENC→QuickSync→AMF、実際に試しエンコードして動くものだけ)を使い、
    // GPUが無ければopen-cpuの判定から速度優先の`-preset`を選ぶ。
    let codec_args = if job.trim.is_some() { accelerate_h264_args(codec_args) } else { codec_args };
    args.extend(codec_args);
    if mkv {
        // `-map`を既に含むコーデック指定(passthrough等)では元の全マップ追加は不要。
        let already_mapped = job.codec_args.iter().any(|a| a == "-map");
        let keep_all = job.mkv_keep_all_tracks.unwrap_or(true) && !already_mapped;
        if keep_all || !extras.is_empty() {
            let (a, s) = (crate::engine::mkv_tracks::count_streams(&job.input_path, 'a'), crate::engine::mkv_tracks::count_streams(&job.input_path, 's'));
            // 追加トラックだけ指定して元の全保持を切った場合は、映像+音声1本など既定の選択が消えないよう元の映像・音声もマップする。
            let mut m = crate::engine::mkv_tracks::map_args(extras, keep_all, a, s);
            if !keep_all && !already_mapped && !extras.is_empty() {
                let mut base: Vec<String> = ["-map", "0:v?", "-map", "0:a:0?"].iter().map(|x| x.to_string()).collect();
                base.append(&mut m);
                m = base;
            }
            args.extend(m);
        }
    }
    if codec_args_are_stream_copy(&job.codec_args) {
        // 無変換コピー(Dolby Vision/Atmos等の保持)では、再エンコード系の指定
        // (ビットレート・拡縮・fps)は矛盾するので付けない。
    } else {
        push_bitrate_args(&mut args, &job.bitrate, &job.output_path);
        push_resolution_args(&mut args, &job.resolution);
        push_fps_args(&mut args, &job.fps);
        push_ai_denoise_args(&mut args, &job.ai_denoise)?;
    }
    // hq-resampleとAIノイズ除去などが両方`-af`を持つ場合に1つのチェーンへ統合する。
    let mut args = merge_audio_filters(args);
    apply_opus_limits(&mut args);

    // ディスク容量から逆算したビットレート(ディスクいっぱいに収める)の動画は、2パスで正確に合わせる(2026-09-24)。
    // 1パスのlibx264は指定より6〜7%大きくなることを実測で確認(DVD画質→フルHD/4K、60秒、目標100MBに対し105.8/107.2MB)
    // し、ディスクに収まらなくなるため。2パスでは98.6%(はみ出さず、ほぼいっぱい)、所要時間は約1.3倍だった。
    if needs_two_pass(job, &args) {
        return run_two_pass(args, &job.output_path);
    }

    args.push("-y".into());
    args.push(job.output_path.clone());

    run_ffmpeg(&args)
}

/// 容量から逆算したビットレートの動画を、ソフトウェアエンコーダ(libx264/libx265)で作るときだけ2パスにする。
/// GPUのハードウェアエンコーダ・音声だけの出力・無変換コピーは対象外(従来どおり1パス)。
fn needs_two_pass(job: &ConvertJob, args: &[String]) -> bool {
    matches!(job.bitrate, Some(BitrateMode::AutoMaxForCapacity(_)))
        && !is_audio_only_output(&job.output_path)
        && args.windows(2).any(|w| w[0] == "-c:v" && (w[1] == "libx264" || w[1] == "libx265"))
}

fn run_two_pass(args: Vec<String>, output_path: &str) -> Result<(), String> {
    let log = std::env::temp_dir().join(format!(
        "make-disk-2pass-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
    ));
    let log_s = log.to_string_lossy().to_string();
    let null_dev = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let x265 = args.windows(2).any(|w| w[0] == "-c:v" && w[1] == "libx265");
    // libx265は`-pass`ではなく`-x265-params pass=N:stats=...`で指定する。
    let pass_args = |n: u8| -> Vec<String> {
        if x265 {
            vec!["-x265-params".into(), format!("pass={n}:stats={}", log_s.replace('\\', "/"))]
        } else {
            vec!["-pass".into(), n.to_string(), "-passlogfile".into(), log_s.clone()]
        }
    };
    let mut pass1 = args.clone();
    pass1.extend(pass_args(1));
    pass1.extend(["-an".into(), "-f".into(), "null".into(), "-y".into(), null_dev.into()]);
    let mut pass2 = args;
    pass2.extend(pass_args(2));
    pass2.extend(["-y".into(), output_path.to_string()]);
    let result = run_ffmpeg(&pass1).and_then(|_| run_ffmpeg(&pass2));
    // 2パスの統計ファイル(<log>-0.log、.mbtree、x265の.cutree等)を片付ける。
    if let (Some(dir), Some(name)) = (log.parent(), log.file_name().map(|n| n.to_string_lossy().to_string())) {
        if let Ok(rd) = fs::read_dir(dir) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with(&name) {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
    }
    result
}

fn run_convert_with_cut_ranges(job: &ConvertJob, cuts: &[CutRange]) -> Result<(), String> {
    let keep = keep_segments_from_cuts(cuts);
    if keep.is_empty() {
        return Err(
            "指定したカット範囲により、残る区間がありません / no footage remains after applying the given cut ranges".to_string(),
        );
    }

    let output_path = Path::new(&job.output_path);
    // 中間の区間ファイルは、出力の拡張子ではなく常にMatroska(.mkv)にする(2026-09-23の実バグ修正)。
    // 以前は出力の拡張子をそのまま使っていたため、MP4→WAVでは`-c copy`でAACをWAVの入れ物へ
    // 無変換で詰めることになり、フレーム境界が失われて結合時にほぼ復号できず、3時間33分の元から
    // 0.627秒のWAVしかできなかった(実ファイルで再現・確認)。MKVはAV1/H.264/AAC/Opus等どの
    // コーデックもそのまま入れられる。
    let ext = "mkv";
    let audio_only = is_audio_only_output(&job.output_path);
    let tmp_dir = output_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".make-disk-tmp-{}", std::process::id()));
    fs::create_dir_all(&tmp_dir).map_err(|e| format!("一時フォルダの作成に失敗しました: {e}"))?;

    let cleanup = |tmp_dir: &Path| {
        let _ = fs::remove_dir_all(tmp_dir);
    };

    let hw_encoder = if job.frame_accurate { detect_hw_video_encoder() } else { None };

    let mut segment_paths: Vec<std::path::PathBuf> = Vec::new();
    for (i, (start, end)) in keep.iter().enumerate() {
        let seg_path = tmp_dir.join(format!("segment_{i}.{ext}"));
        let mut args: Vec<String> = vec!["-ss".into(), start.to_string()];
        if let Some(e) = end {
            args.push("-to".into());
            args.push(e.to_string());
        }
        args.push("-i".into());
        args.push(job.input_path.clone());
        if audio_only {
            args.push("-vn".into()); // 音声だけの出力では映像を取り出さない(無駄な容量・時間を省く)
        }

        if job.frame_accurate && !audio_only {
            // カット境界をフレーム単位で正確に切るにはエンコードが要る
            // (`-c copy`はキーフレーム単位の精度しか出せない)。
            // CPUソフトエンコード(libx264等)は5時間級の動画では非現実的な
            // 時間がかかるため、GPUハードウェアエンコーダが検出できれば
            // それを優先して使う(NVENC/QuickSync/AMFはCPUソフトエンコード
            // に比べて大幅に高速)。
            args.push("-c:v".into());
            args.push(hw_encoder.unwrap_or("libx264").to_string());
            if hw_encoder.is_none() {
                // GPUが無くCPU(libx264)にフォールバックする場合のみ、
                // open-cpuの検出結果(AVX2/AVX-512の有無)で-presetを
                // 自動選択する。x264自身はAVX2/AVX-512の使用可否を実行時に
                // 自動判定するが、「どれだけ探索を頑張るか」を決める
                // presetはこちらで明示的に選ぶ必要がある——非力なCPUには
                // 軽いpreset、強力なCPUにはより圧縮効率の良いpresetを
                // 割り当てることで、open-cpuの検出結果を実際にffmpegの
                // 挙動制御へ反映する。
                args.push("-preset".into());
                args.push(cpu::recommended_x264_preset().to_string());
            }
            args.push("-c:a".into());
            args.push("aac".into());
        } else {
            args.push("-c".into());
            args.push("copy".into());
        }
        args.push("-y".into());
        args.push(seg_path.to_string_lossy().to_string());

        if let Err(e) = run_ffmpeg(&args) {
            cleanup(&tmp_dir);
            return Err(format!("区間抽出に失敗しました({start}s〜{end:?}s): {e}"));
        }
        segment_paths.push(seg_path);
    }

    let list_path = tmp_dir.join("concat_list.txt");
    let list_content: String = segment_paths
        .iter()
        .map(|p| format!("file '{}'\n", p.to_string_lossy().replace('\'', "'\\''")))
        .collect();
    if let Err(e) = fs::write(&list_path, list_content) {
        cleanup(&tmp_dir);
        return Err(format!("結合リストの作成に失敗しました: {e}"));
    }

    let mut concat_args: Vec<String> = vec![
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        list_path.to_string_lossy().to_string(),
    ];
    if audio_only {
        concat_args.push("-vn".into());
    }

    // フォーマット変換・ビットレート・解像度・フレームレートいずれの
    // 指定も無ければ、結合も-c copyで完全に再エンコード無しにする
    // (最速・無劣化)。
    if job.codec_args.is_empty() && job.bitrate.is_none() && job.resolution.is_none() && job.fps.is_none() && job.ai_denoise.is_none() {
        concat_args.push("-c".into());
        concat_args.push("copy".into());
    } else {
        match resolve_av1_codec_args(&job.codec_args) {
            Ok(a) => concat_args.extend(a),
            Err(e) => {
                cleanup(&tmp_dir);
                return Err(e);
            }
        }
        push_bitrate_args(&mut concat_args, &job.bitrate, &job.output_path);
        push_resolution_args(&mut concat_args, &job.resolution);
        push_fps_args(&mut concat_args, &job.fps);
        if let Err(e) = push_ai_denoise_args(&mut concat_args, &job.ai_denoise) {
            cleanup(&tmp_dir);
            return Err(e);
        }
    }

    concat_args.push("-y".into());
    concat_args.push(job.output_path.clone());

    let result = run_ffmpeg(&concat_args);
    cleanup(&tmp_dir);
    result
}

/// このマシンで実際に使えるGPUハードウェアエンコーダを検出する。
/// NVIDIA(NVENC)→Intel(QuickSync)→AMD(AMF)の順で優先し、
/// どれも無ければNoneを返す(呼び出し側はlibx264にフォールバックする)。
///
/// 「open-cuda」「open-directx」等の自前GPU抽象化層は動画コーデックの
/// 実装を持たないため、ここでは既存のffmpegビルドが持つハードウェア
/// エンコーダをそのまま活用する方針にしている(実際に動作し、CPUソフト
/// エンコードより大幅に高速な、現実的な解決策のため)。
///
/// 重要: `ffmpeg -encoders`の一覧に載っているかどうかだけでは不十分
/// (実機検証で発見した実バグ)。それはそのエンコーダが「ffmpegの
/// ビルドにコンパイルされているか」を示すだけで、実際にこのマシンの
/// GPUドライバが対応しているかは別問題——古いNVIDIAドライバでは
/// 「h264_nvencはリストに出るが実行すると
/// "Driver does not support the required nvenc API version" で失敗する」
/// ことを実際に確認した。そのため、候補ごとに実際に1フレームだけ
/// 試しエンコードしてみて、本当に成功するものだけを採用する。
fn detect_hw_video_encoder() -> Option<&'static str> {
    // 試しエンコードは1回数百ミリ秒〜かかるため、結果はプロセス内でキャッシュする。
    static CACHE: std::sync::OnceLock<Option<&'static str>> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| ["h264_nvenc", "h264_qsv", "h264_amf"].into_iter().find(|c| hw_encoder_actually_works(c)))
}

/// `-c:v libx264`をGPUのハードウェアエンコーダへ置き換える(無ければ速度優先の`-preset`を付ける)。
/// `-preset`が既に指定されていればCPUの場合は触らない。H.264以外の指定はそのまま返す。
fn accelerate_h264_args(mut args: Vec<String>) -> Vec<String> {
    let Some(i) = args.iter().position(|a| a == "-c:v") else { return args };
    if args.get(i + 1).map(String::as_str) != Some("libx264") {
        return args;
    }
    match detect_hw_video_encoder() {
        Some(hw) => args[i + 1] = hw.to_string(),
        None => {
            if !args.iter().any(|a| a == "-preset") {
                args.extend(["-preset".to_string(), cpu::fast_x264_preset().to_string()]);
            }
        }
    }
    args
}

fn hw_encoder_actually_works(encoder: &str) -> bool {
    resolve_tool("ffmpeg")
        .args([
            "-f", "lavfi", "-i", "color=black:size=64x64:rate=1",
            "-frames:v", "1", "-c:v", encoder, "-f", "null", "-",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// `-c copy`(全ストリーム無変換コピー)指定か。Dolby Vision/Atmos等を壊さず保持するモード。
fn codec_args_are_stream_copy(codec_args: &[String]) -> bool {
    codec_args.windows(2).any(|w| w[0] == "-c" && w[1] == "copy")
}

/// 出力が音声専用のコンテナ/拡張子か。
fn is_audio_only_output(output_path: &str) -> bool {
    let ext = std::path::Path::new(output_path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" | "opus" | "ac3" | "eac3" | "mka")
}

/// ビットレート指定を追加する。音声専用出力には`-b:a`、動画には`-b:v`を使う
/// (以前は常に`-b:v`で、音声のみの出力ではビットレート指定が無視されていた実バグ)。
fn push_bitrate_args(args: &mut Vec<String>, bitrate: &Option<BitrateMode>, output_path: &str) {
    match bitrate {
        Some(BitrateMode::Fixed(kbps)) | Some(BitrateMode::AutoMaxForCapacity(kbps)) => {
            args.push(if is_audio_only_output(output_path) { "-b:a" } else { "-b:v" }.into());
            args.push(format!("{kbps}k"));
        }
        None => {}
    }
}

/// Opusの仕様上限を守る(2026-09-19): 入力は最大48kHz、ビットレートはステレオで最大510kbps
/// (3ch以上は1chあたり256kbps)。指定が上限を超える場合は上限へ丸め、`-ar`が無ければ48kHzを指定する。
/// ビットレートはVBR(ffmpegのlibopus既定)で、指定値は目標平均として必要に応じて変動する。
pub(crate) fn apply_opus_limits(args: &mut Vec<String>) {
    if !args.windows(2).any(|w| w[0] == "-c:a" && w[1] == "libopus") {
        return;
    }
    let channels = args.windows(2).find(|w| w[0] == "-ac").and_then(|w| w[1].parse::<u32>().ok()).unwrap_or(2);
    let cap_kbps = if channels <= 2 { 510 } else { 256 * channels as u64 };
    if let Some(i) = args.iter().position(|a| a == "-b:a") {
        if let Some(v) = args.get_mut(i + 1) {
            if let Some(kbps) = v.strip_suffix('k').and_then(|n| n.parse::<u64>().ok()) {
                *v = format!("{}k", kbps.clamp(6, cap_kbps));
            }
        }
    }
    if !args.iter().any(|a| a == "-ar") {
        let at = args.iter().position(|a| a == "-c:a").unwrap_or(0);
        args.splice(at..at, ["-ar".to_string(), "48000".to_string()]);
    }
}

/// 出力解像度の指定があれば`-vf scale=W:H`を追加する(2026-09-17新設)。
fn push_resolution_args(args: &mut Vec<String>, resolution: &Option<Resolution>) {
    if let Some(r) = resolution {
        args.push("-vf".into());
        args.push(format!("scale={}:{}", r.width, r.height));
    }
}

/// フレームレート指定があれば`-r <fps>`を追加する(2026-09-17新設)。
/// 「不明」(未指定)の場合は元のフレームレートのまま変換しない。
fn push_fps_args(args: &mut Vec<String>, fps: &Option<u32>) {
    if let Some(f) = fps {
        args.push("-r".into());
        args.push(f.to_string());
    }
}

/// 「AI判断で自動カット」モード(2026-09-16新設)で使う、無音区間の
/// 自動検出結果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SilenceRange {
    pub start_secs: f64,
    pub end_secs: f64,
}

/// ffmpegの`silencedetect`フィルタで無音区間を検出し、[`CutRange`]として
/// 使える形で返す(2026-09-16新設)。
///
/// **正直な開示(誇張しない)**: これはユーザー指示にある「AI判断で
/// 自動カット」の実装だが、実際にはLLM/画像認識モデルによる意味的な
/// 判断ではなく、ffmpeg内蔵の**音量ベースの無音検出**という
/// ヒューリスティックである。無音部分(既定: -30dB未満が0.5秒以上
/// 続く区間)を「重要度が低く、削っても画質・音質への影響が少ない
/// 部分」とみなして自動的にカット候補にする、という単純だが実用的な
/// 近似——本当の意味でのシーン重要度判定(退屈な場面の検出等)は
/// 行っていない。
pub fn detect_silence_ranges(path: &str, silence_threshold_db: f64, min_silence_secs: f64) -> Result<Vec<SilenceRange>, String> {
    let output = resolve_tool("ffmpeg")
        .args([
            "-i",
            path,
            "-af",
            &format!("silencedetect=noise={silence_threshold_db}dB:d={min_silence_secs}"),
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|e| format!("ffmpegの起動に失敗しました(未インストールの可能性): {e}"))?;

    // silencedetectはstderrへログを出す(ffmpegの一般的な挙動、
    // exit codeは正常終了する)。
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut ranges = Vec::new();
    let mut pending_start: Option<f64> = None;
    for line in stderr.lines() {
        if let Some(idx) = line.find("silence_start: ") {
            let rest = &line[idx + "silence_start: ".len()..];
            if let Some(v) = rest.split_whitespace().next().and_then(|s| s.parse::<f64>().ok()) {
                pending_start = Some(v);
            }
        } else if let Some(idx) = line.find("silence_end: ") {
            let rest = &line[idx + "silence_end: ".len()..];
            if let (Some(start), Some(end)) = (pending_start.take(), rest.split_whitespace().next().and_then(|s| s.parse::<f64>().ok())) {
                ranges.push(SilenceRange { start_secs: start, end_secs: end });
            }
        }
    }
    Ok(ranges)
}

/// 「サイズ指定」モード(2026-09-16新設)向け: 目標ファイルサイズ
/// (バイト)と総尺(秒)から、そのサイズに収まる平均ビットレート(kbps)を
/// 算出する。ディスク容量ではなく任意の目標サイズを指定できる点が
/// `capacity::max_bitrate_for_capacity`(ディスク種別限定)との違い。
pub fn bitrate_for_target_size_kbps(target_bytes: u64, total_duration_secs: f64) -> u64 {
    if total_duration_secs <= 0.0 {
        return 0;
    }
    ((target_bytes as f64 * 8.0) / total_duration_secs / 1000.0) as u64
}

/// 「等間隔分割」(2026-09-16新設): `total_secs`を`segment_count`個の
/// 等しい長さの区間に分割する。各区間は既存の`TrimRange`(開始+長さ)
/// として返すため、呼び出し側は`convert_media`を区間ごとに呼ぶだけで
/// 分割出力できる(新しい抽出処理を実装する必要が無い)。
pub fn equal_interval_segments(total_secs: f64, segment_count: u32) -> Vec<TrimRange> {
    if segment_count == 0 || total_secs <= 0.0 {
        return Vec::new();
    }
    let segment_len = total_secs / segment_count as f64;
    (0..segment_count)
        .map(|i| TrimRange {
            start_secs: Some(segment_len * i as f64),
            duration_secs: Some(segment_len),
        })
        .collect()
}

/// 「サイズ指定分割」(2026-09-16新設): `segment_secs`ごとに区切る。
/// 割り切れない最後の区間は、その分だけ短い「あまり」として返す
/// (呼び出し側で、この最後の区間だけディスク容量いっぱいに
/// ビットレートを自動調整することを想定——ユーザー指示「あまりは、
/// DISKいっぱいにビットレートを自動変更して自動編集して」への対応)。
pub fn fixed_length_segments(total_secs: f64, segment_secs: f64) -> Vec<TrimRange> {
    if segment_secs <= 0.0 || total_secs <= 0.0 {
        return Vec::new();
    }
    let mut segments = Vec::new();
    let mut start = 0.0;
    while start < total_secs {
        let remaining = total_secs - start;
        let len = remaining.min(segment_secs);
        segments.push(TrimRange { start_secs: Some(start), duration_secs: Some(len) });
        start += segment_secs;
    }
    segments
}

/// 複数の音声/動画ファイルを結合(合成)する(2026-09-16新設)。
/// ユーザー指示「複数の音声・静止画・動画・PDFの合成編集」のうち、
/// 静止画・PDFの合成は別機能(PDF見開き対応)で扱うため、ここでは
/// 音声/動画同士の結合を担う。ffmpegの`concat`フィルタを使うため、
/// 入力同士のコーデック・解像度が揃っていなくても(再エンコードで)
/// 結合できる。`has_video`は呼び出し側(フロントエンド)が入力の
/// 拡張子から判定して渡す(全入力が動画か、全て音声かのどちらかを
/// 前提とする——動画と音声の混在結合は現時点で未対応)。
pub fn concat_media(input_paths: &[String], output_path: &str, has_video: bool) -> Result<(), String> {
    if input_paths.len() < 2 {
        return Err("結合には2つ以上のファイルが必要です / concatenation needs at least 2 files".to_string());
    }

    let mut cmd = resolve_tool("ffmpeg");
    for p in input_paths {
        cmd.args(["-i", p]);
    }

    let n = input_paths.len();
    let mut filter = String::new();
    for i in 0..n {
        if has_video {
            filter.push_str(&format!("[{i}:v][{i}:a]"));
        } else {
            filter.push_str(&format!("[{i}:a]"));
        }
    }
    if has_video {
        filter.push_str(&format!("concat=n={n}:v=1:a=1[outv][outa]"));
    } else {
        filter.push_str(&format!("concat=n={n}:v=0:a=1[outa]"));
    }
    cmd.args(["-filter_complex", &filter]);
    if has_video {
        cmd.args(["-map", "[outv]", "-map", "[outa]"]);
    } else {
        cmd.args(["-map", "[outa]"]);
    }
    cmd.args(["-y", output_path]);

    let output = cmd.output().map_err(|e| format!("ffmpegの起動に失敗しました(未インストールの可能性): {e}"))?;
    if !output.status.success() {
        return Err(format!("ffmpegによる結合が失敗しました: {}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(())
}

/// 本家ffmpegを優先して実行し、起動自体に失敗した場合(PATH上に無い場合)
/// のみ同梱の`rs-ffmpeg`へフォールバックする(2026-09-17新設、
/// `engine::iso`のxorriso→rs-xorrisoと同じパターン)。**正直な開示**:
/// `rs-ffmpeg`は非圧縮WAVの単純なサンプルレート/チャンネル変換専用
/// (`-i in.wav [-ar rate] [-ac channels] out.wav`のみ)で、コーデック
/// 指定・トリミング・ビットレート指定等の引数は自身で明確に拒否する
/// 設計になっている(黙って無視して壊れたファイルを作らない)ため、
/// このフォールバックは「本家ffmpegが無く、かつ単純なWAV処理」の
/// 場合のみ実際に成功し、それ以外は分かりやすいエラーで終わる。
fn run_ffmpeg(args: &[String]) -> Result<(), String> {
    match resolve_tool("ffmpeg").args(args).output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(String::from_utf8_lossy(&output.stderr).to_string()),
        Err(_) => {
            let output = resolve_tool("rs-ffmpeg")
                .args(args)
                .output()
                .map_err(|e| format!("ffmpeg・rs-ffmpegともに起動に失敗しました(いずれも未インストール/未同梱の可能性): {e}"))?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).to_string());
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrate_for_target_size_kbps_computes_expected_value() {
        // 10MB(=80,000,000ビット)を100秒に収めるなら800kbps。
        let kbps = bitrate_for_target_size_kbps(10_000_000, 100.0);
        assert_eq!(kbps, 800);
    }

    #[test]
    fn bitrate_for_target_size_kbps_returns_zero_for_zero_duration() {
        assert_eq!(bitrate_for_target_size_kbps(10_000_000, 0.0), 0);
    }

    #[test]
    fn channel_layout_keep_is_a_no_op() {
        assert!(channel_layout_args(2, ChannelLayoutTarget::Keep).is_empty());
    }

    #[test]
    fn channel_layout_downmix_uses_ac_flag() {
        assert_eq!(channel_layout_args(6, ChannelLayoutTarget::Stereo), vec!["-ac".to_string(), "2".to_string()]);
        assert_eq!(channel_layout_args(8, ChannelLayoutTarget::Mono), vec!["-ac".to_string(), "1".to_string()]);
    }

    #[test]
    fn channel_layout_upmix_from_mono_duplicates_to_all_channels() {
        let args = channel_layout_args(1, ChannelLayoutTarget::Surround51);
        assert_eq!(args[0], "-af");
        assert!(args[1].contains("FL=c0") && args[1].contains("LFE=0"), "{}", args[1]);
    }

    #[test]
    fn channel_layout_upmix_from_stereo_mixes_center_and_attenuates_rear() {
        let args = channel_layout_args(2, ChannelLayoutTarget::Surround71);
        assert_eq!(args[0], "-af");
        assert!(args[1].contains("FC=0.5*FL+0.5*FR") && args[1].contains("SL=0.4*FL"), "{}", args[1]);
    }

    #[test]
    fn channel_layout_upmix_is_skipped_for_already_multichannel_sources() {
        assert!(channel_layout_args(6, ChannelLayoutTarget::Surround71).is_empty());
        assert!(channel_layout_args(3, ChannelLayoutTarget::Surround51).is_empty());
    }

    #[test]
    fn no_cuts_keeps_whole_file() {
        let keep = keep_segments_from_cuts(&[]);
        assert_eq!(keep, vec![(0.0, None)]);
    }

    #[test]
    fn single_middle_cut_produces_two_keep_segments() {
        let cuts = vec![CutRange { start_secs: 100.0, end_secs: Some(200.0) }];
        let keep = keep_segments_from_cuts(&cuts);
        assert_eq!(keep, vec![(0.0, Some(100.0)), (200.0, None)]);
    }

    #[test]
    fn start_middle_and_end_cuts_leave_two_middle_segments() {
        // 「最初の0〜120をカット」「途中の600〜660をカット」
        // 「最後の1000〜末尾をカット」の3つを同時指定するケース。
        let cuts = vec![
            CutRange { start_secs: 0.0, end_secs: Some(120.0) },
            CutRange { start_secs: 600.0, end_secs: Some(660.0) },
            CutRange { start_secs: 1000.0, end_secs: None },
        ];
        let keep = keep_segments_from_cuts(&cuts);
        assert_eq!(keep, vec![(120.0, Some(600.0)), (660.0, Some(1000.0))]);
    }

    #[test]
    fn overlapping_cuts_are_merged() {
        let cuts = vec![
            CutRange { start_secs: 0.0, end_secs: Some(300.0) },
            CutRange { start_secs: 200.0, end_secs: Some(400.0) },
        ];
        let keep = keep_segments_from_cuts(&cuts);
        assert_eq!(keep, vec![(400.0, None)]);
    }

    #[test]
    fn cuts_after_an_eof_cut_are_ignored() {
        let cuts = vec![
            CutRange { start_secs: 500.0, end_secs: None },
            CutRange { start_secs: 100.0, end_secs: Some(200.0) },
        ];
        let keep = keep_segments_from_cuts(&cuts);
        assert_eq!(keep, vec![(0.0, Some(100.0)), (200.0, Some(500.0))]);
    }

    // --- ここから実ffmpegを使う統合テスト ---
    // ffmpegが無い環境ではeprintln!してスキップする(open-cuda/open-directx
    // の実機テストと同じ方針: fakeな成功にしない)。

    fn ffmpeg_available() -> bool {
        Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
    }

    /// (幅, 高さ, フレームレート)を実ffprobeで取得する
    /// (解像度/FPS指定〈2026-09-17新設〉の実機E2E検証用)。
    fn probe_video_dimensions_and_fps(path: &Path) -> (u32, u32, f64) {
        let output = Command::new("ffprobe")
            .args([
                "-v", "error",
                "-select_streams", "v:0",
                "-show_entries", "stream=width,height,r_frame_rate",
                "-of", "csv=p=0",
                path.to_str().unwrap(),
            ])
            .output()
            .expect("ffprobe should run");
        let text = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = text.trim().split(',').collect();
        let width: u32 = parts[0].parse().expect("width should parse");
        let height: u32 = parts[1].parse().expect("height should parse");
        let fps = parts[2]
            .split('/')
            .map(|s| s.parse::<f64>().unwrap())
            .reduce(|num, den| num / den)
            .expect("r_frame_rate should parse as a fraction");
        (width, height, fps)
    }

    fn probe_duration_secs(path: &Path) -> f64 {
        let output = Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", path.to_str().unwrap()])
            .output()
            .expect("ffprobe should run");
        String::from_utf8_lossy(&output.stdout).trim().parse().expect("ffprobe should print a duration")
    }

    fn make_test_video(dir: &Path, name: &str, duration_secs: u32) -> std::path::PathBuf {
        let path = dir.join(name);
        let status = Command::new("ffmpeg")
            .args([
                "-y", "-f", "lavfi", "-i", &format!("testsrc=duration={duration_secs}:size=320x240:rate=10"),
                "-c:v", "libx264", "-g", "10", "-pix_fmt", "yuv420p",
                path.to_str().unwrap(),
            ])
            .output()
            .expect("ffmpeg should run");
        assert!(status.status.success(), "test fixture generation failed: {}", String::from_utf8_lossy(&status.stderr));
        path
    }

    /// 音声の途中(2〜4秒)だけ無音にした6秒のテスト音声ファイルを作る
    /// (`detect_silence_ranges`の実機E2Eテスト用)。
    fn make_test_audio_with_silence_gap(dir: &Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        let status = Command::new("ffmpeg")
            .args([
                "-y", "-f", "lavfi", "-i", "sine=frequency=1000:duration=6",
                "-af", "volume=enable='between(t,2,4)':volume=0",
                path.to_str().unwrap(),
            ])
            .output()
            .expect("ffmpeg should run");
        assert!(status.status.success(), "test fixture generation failed: {}", String::from_utf8_lossy(&status.stderr));
        path
    }

    #[test]
    fn real_ffmpeg_silence_detection_finds_the_expected_gap() {
        if !ffmpeg_available() {
            eprintln!("ffmpegが見つからないためスキップ / skipping: ffmpeg not found on PATH");
            return;
        }

        let tmp = std::env::temp_dir().join(format!("make_disk_test_silence_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_test_audio_with_silence_gap(&tmp, "source_with_gap.wav");

        let ranges = detect_silence_ranges(source.to_str().unwrap(), -30.0, 0.5).expect("detect_silence_ranges should succeed");
        let _ = fs::remove_dir_all(&tmp);

        assert_eq!(ranges.len(), 1, "expected exactly one silence range, got {ranges:?}");
        let r = &ranges[0];
        assert!((r.start_secs - 2.0).abs() < 0.2, "silence should start around 2.0s, got {}", r.start_secs);
        assert!((r.end_secs - 4.0).abs() < 0.2, "silence should end around 4.0s, got {}", r.end_secs);
    }

    #[test]
    fn real_ffmpeg_multi_range_cut_produces_expected_duration() {
        if !ffmpeg_available() {
            eprintln!("ffmpegが見つからないためスキップ / skipping: ffmpeg not found on PATH");
            return;
        }

        let tmp = std::env::temp_dir().join(format!("make_disk_test_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_test_video(&tmp, "source.mp4", 20);
        let output = tmp.join("output.mp4");

        // カット: 最初の0-3秒、途中の8-10秒、最後の16-20秒(末尾まで)。
        // 残る区間: 3-8秒(5秒) + 10-16秒(6秒) = 合計11秒のはず。
        let job = ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: vec![],
            bitrate: None,
            trim: None,
            cut_ranges: Some(vec![
                CutRange { start_secs: 0.0, end_secs: Some(3.0) },
                CutRange { start_secs: 8.0, end_secs: Some(10.0) },
                CutRange { start_secs: 16.0, end_secs: None },
            ]),
            frame_accurate: false,
            resolution: None,
            fps: None,
            ai_denoise: None,
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: None,
            extra_tracks: vec![],
            ai_upscale: None,
            audio_bwe: None,
        };

        run_convert(&job).expect("run_convert with cut_ranges should succeed");

        let result_duration = probe_duration_secs(&output);
        assert!(
            (result_duration - 11.0).abs() < 2.0,
            "expected ~11s after cuts (stream-copy is keyframe-aligned so some slack is expected), got {result_duration}s"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn real_ffmpeg_frame_accurate_cut_is_close_to_exact() {
        if !ffmpeg_available() {
            eprintln!("ffmpegが見つからないためスキップ / skipping: ffmpeg not found on PATH");
            return;
        }

        let tmp = std::env::temp_dir().join(format!("make_disk_test_fa_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_test_video(&tmp, "source.mp4", 10);
        let output = tmp.join("output.mp4");

        // 単一区間(2秒〜7秒、5秒分)のフレーム精度カット。
        // GPUエンコーダはこのCI/開発機には無い想定なのでlibx264
        // フォールバック経路を検証する。
        let job = ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: vec![],
            bitrate: None,
            trim: None,
            cut_ranges: Some(vec![
                CutRange { start_secs: 0.0, end_secs: Some(2.0) },
                CutRange { start_secs: 7.0, end_secs: None },
            ]),
            frame_accurate: true,
            resolution: None,
            fps: None,
            ai_denoise: None,
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: None,
            extra_tracks: vec![],
            ai_upscale: None,
            audio_bwe: None,
        };

        run_convert(&job).expect("run_convert with frame_accurate should succeed");

        let result_duration = probe_duration_secs(&output);
        assert!(
            (result_duration - 5.0).abs() < 0.5,
            "frame-accurate cut should be close to exact (expected ~5s), got {result_duration}s"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn real_ffmpeg_applies_resolution_and_fps() {
        if !ffmpeg_available() {
            eprintln!("ffmpegが見つからないためスキップ / skipping: ffmpeg not found on PATH");
            return;
        }

        let tmp = std::env::temp_dir().join(format!("make_disk_test_res_fps_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_test_video(&tmp, "source.mp4", 2);
        let output = tmp.join("output.mp4");

        // DVDフルHD相当(1920x1080)・30fpsへの変換を指定(2026-09-17新設の
        // 解像度/FPS指定機能)。
        let job = ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: vec!["-c:v".to_string(), "libx264".to_string(), "-pix_fmt".to_string(), "yuv420p".to_string()],
            bitrate: None,
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: Some(Resolution { width: 1920, height: 1080 }),
            fps: Some(30),
            ai_denoise: None,
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: None,
            extra_tracks: vec![],
            ai_upscale: None,
            audio_bwe: None,
        };

        run_convert(&job).expect("run_convert with resolution/fps should succeed");

        let (width, height, fps) = probe_video_dimensions_and_fps(&output);
        let _ = fs::remove_dir_all(&tmp);

        assert_eq!((width, height), (1920, 1080), "指定した解像度(1920x1080)に変換されているはず");
        assert!((fps - 30.0).abs() < 0.1, "指定したフレームレート(30fps)に変換されているはず、実際: {fps}");
    }

    /// 実物の通しテスト: DVD相当(720×480、29.97fps、音声つき)→フルHD・約60fps(AI超解像+RIFE補間)。
    /// 遅い(CPU/GPU実測・RIFE取得が入る)ので通常は実行しない。`--ignored`で実行する。
    #[test]
    #[ignore]
    fn real_dvd_to_full_hd_60fps_with_ai_and_rife() {
        if !ffmpeg_available() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_dvd_hd_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let source = tmp.join("dvd.mkv");
        let ok = crate::engine::sidecar::resolve_tool("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=720x480:rate=30000/1001:duration=2", "-f", "lavfi", "-i", "sine=frequency=440:duration=2"])
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "ac3", "-shortest"])
            .arg(&source)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let output = tmp.join("out.mkv");
        let job = ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: vec!["-c:v".to_string(), "libx264".to_string(), "-pix_fmt".to_string(), "yuv420p".to_string(), "-c:a".to_string(), "aac".to_string()],
            bitrate: None,
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: Some(Resolution { width: 1920, height: 1080 }),
            fps: None,
            ai_denoise: None,
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: Some(false),
            extra_tracks: vec![],
            ai_upscale: Some(crate::engine::ai_upscale::AiUpscale { backend: "auto".to_string(), target_fps: Some(60), ..Default::default() }),
            audio_bwe: None,
        };
        run_convert(&job).expect("DVD -> Full HD 60fps conversion");
        let (w, h, fps) = probe_video_dimensions_and_fps(&output);
        let has_audio = crate::engine::mkv_tracks::count_streams(&output.to_string_lossy(), 'a') > 0;
        let _ = fs::remove_dir_all(&tmp);
        assert_eq!((w, h), (1920, 1080));
        assert!((fps - 59.94).abs() < 0.1, "fps={fps}");
        assert!(has_audio, "音声が残っているはず");
    }

    /// 実物の通しテスト: DVD相当(720×480、24fps)→4K・120fps(AI超解像+RIFE 5倍補間)。
    /// このPC(GT 730)ではGPUが4Kで壊れた補間結果を返すため、CPUへ自動で切り替わる(非常に遅い)。極小の4コマで確かめる。
    #[test]
    #[ignore]
    fn real_dvd_to_4k_120fps_end_to_end() {
        if !ffmpeg_available() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_dvd_4k120_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let source = tmp.join("dvd.mkv");
        let ok = crate::engine::sidecar::resolve_tool("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=720x480:rate=24:duration=0.1667", "-f", "lavfi", "-i", "sine=frequency=440:duration=0.1667"])
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "ac3", "-shortest"])
            .arg(&source)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let output = tmp.join("out.mkv");
        let job = ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: vec!["-c:v".to_string(), "libx264".to_string(), "-pix_fmt".to_string(), "yuv420p".to_string(), "-c:a".to_string(), "aac".to_string()],
            bitrate: None,
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: Some(Resolution { width: 3840, height: 2160 }),
            fps: None,
            ai_denoise: None,
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: Some(false),
            extra_tracks: vec![],
            ai_upscale: Some(crate::engine::ai_upscale::AiUpscale { backend: "auto".to_string(), target_fps: Some(120), ..Default::default() }),
            audio_bwe: None,
        };
        let t = std::time::Instant::now();
        run_convert(&job).expect("DVD -> 4K 120fps conversion");
        eprintln!("4K120 e2e took {:.0}s", t.elapsed().as_secs_f64());
        let (w, h, fps) = probe_video_dimensions_and_fps(&output);
        let frames = crate::engine::probe::probe(&output.to_string_lossy()).map(|i| i.duration_secs * fps).unwrap_or(0.0);
        let has_audio = crate::engine::mkv_tracks::count_streams(&output.to_string_lossy(), 'a') > 0;
        let _ = fs::remove_dir_all(&tmp);
        assert_eq!((w, h), (3840, 2160));
        assert!((fps - 120.0).abs() < 0.1, "fps={fps}");
        assert!(frames > 15.0, "frames~{frames}");
        assert!(has_audio);
    }

    #[test]
    fn real_ffmpeg_applies_audio_bitrate_to_audio_only_output() {
        if !ffmpeg_available() {
            eprintln!("ffmpegが見つからないためスキップ");
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_abr_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_test_video_with_audio(&tmp, "src.mp4", 6);
        let output = tmp.join("out.mp3");
        let job = ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: vec!["-c:a".into(), "libmp3lame".into()],
            bitrate: Some(BitrateMode::Fixed(64)),
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: None,
            fps: None,
            ai_denoise: None,
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: None,
            extra_tracks: vec![],
            ai_upscale: None,
            audio_bwe: None,
        };
        run_convert(&job).expect("audio-only conversion from a video input should succeed");
        let out = Command::new("ffprobe").args(["-v", "error", "-show_entries", "format=bit_rate", "-of", "default=nw=1:nk=1", output.to_str().unwrap()]).output().unwrap();
        let kbps: f64 = String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().unwrap() / 1000.0;
        let _ = fs::remove_dir_all(&tmp);
        assert!((kbps - 64.0).abs() < 8.0, "指定した64kbpsが音声出力に効いているはず(実際: {kbps} kbps)");
    }

    #[test]
    fn real_ffmpeg_encodes_av1_video_with_opus_audio() {
        if !ffmpeg_available() || detect_av1_encoder().is_none() {
            eprintln!("ffmpegまたはAV1エンコーダが無いためスキップ");
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_av1_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_test_video_with_audio(&tmp, "src.mp4", 2);
        let output = tmp.join("out.webm");
        let job = ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: vec!["-c:v".into(), "av1".into(), "-c:a".into(), "libopus".into()],
            bitrate: Some(BitrateMode::Fixed(200)),
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: None,
            fps: None,
            ai_denoise: None,
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: None,
            extra_tracks: vec![],
            ai_upscale: None,
            audio_bwe: None,
        };
        run_convert(&job).expect("AV1+Opus conversion should succeed");
        let out = Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream=codec_name", "-of", "csv=p=0", output.to_str().unwrap()]).output().unwrap();
        let codecs = String::from_utf8_lossy(&out.stdout).to_string();
        let _ = fs::remove_dir_all(&tmp);
        assert!(codecs.contains("av1") && codecs.contains("opus"), "出力はAV1+Opusのはず(実際: {codecs})");
    }

    #[test]
    fn real_ffmpeg_encodes_opus_audio_only() {
        if !ffmpeg_available() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_opus_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_test_video_with_audio(&tmp, "src.mp4", 3);
        let output = tmp.join("out.opus");
        let job = ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: vec!["-c:a".into(), "libopus".into()],
            bitrate: Some(BitrateMode::Fixed(96)),
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: None,
            fps: None,
            ai_denoise: None,
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: None,
            extra_tracks: vec![],
            ai_upscale: None,
            audio_bwe: None,
        };
        run_convert(&job).expect("Opus conversion should succeed");
        let out = Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream=codec_name", "-of", "csv=p=0", output.to_str().unwrap()]).output().unwrap();
        let codecs = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let _ = fs::remove_dir_all(&tmp);
        assert_eq!(codecs, "opus");
    }

    fn make_surround_video(dir: &Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        let st = Command::new("ffmpeg")
            .args(["-y", "-f", "lavfi", "-i", "testsrc=duration=2:size=320x240:rate=10", "-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-filter_complex", "[1:a]pan=5.1|c0=c0|c1=c0|c2=c0|c3=c0|c4=c0|c5=c0[a]", "-map", "0:v", "-map", "[a]", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "ac3", path.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(st.status.success(), "{}", String::from_utf8_lossy(&st.stderr));
        path
    }

    #[test]
    fn real_ffmpeg_preserves_surround_with_eac3_and_stream_copy() {
        if !ffmpeg_available() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_surround_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_surround_video(&tmp, "src.mkv");
        let mk = |out: &Path, codec: Vec<&str>| ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: out.to_string_lossy().to_string(),
            codec_args: codec.into_iter().map(String::from).collect(),
            bitrate: Some(BitrateMode::Fixed(500)),
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: Some(Resolution { width: 640, height: 480 }),
            fps: Some(30),
            ai_denoise: None,
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: None,
            extra_tracks: vec![],
            ai_upscale: None,
            audio_bwe: None,
        };
        let channels = |p: &Path| -> String {
            let o = Command::new("ffprobe").args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=channels,codec_name", "-of", "csv=p=0", p.to_str().unwrap()]).output().unwrap();
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        };
        let eac3 = tmp.join("out.eac3");
        run_convert(&mk(&eac3, vec!["-c:a", "eac3"])).expect("E-AC-3");
        let copy = tmp.join("out.mkv");
        run_convert(&mk(&copy, vec!["-map", "0", "-c", "copy"])).expect("stream copy must ignore bitrate/resolution/fps");
        let (e, c) = (channels(&eac3), channels(&copy));
        let dims = probe_video_dimensions_and_fps(&copy);
        let _ = fs::remove_dir_all(&tmp);
        assert_eq!(e, "eac3,6", "E-AC-3は5.1を保持するはず");
        assert_eq!(c, "ac3,6", "無変換コピーは音声コーデックとチャンネル数をそのまま保持するはず");
        assert_eq!((dims.0, dims.1), (320, 240), "無変換コピーでは解像度指定は適用されず元のまま");
    }

    fn mean_volume_db(path: &Path) -> f64 {
        let o = Command::new("ffmpeg").args(["-hide_banner", "-i", path.to_str().unwrap(), "-af", "volumedetect", "-f", "null", "-"]).output().unwrap();
        let text = String::from_utf8_lossy(&o.stderr).to_string();
        let line = text.lines().find(|l| l.contains("mean_volume")).expect("mean_volume line");
        line.split("mean_volume:").nth(1).unwrap().trim().trim_end_matches(" dB").trim().parse().unwrap()
    }

    #[test]
    fn real_ffmpeg_ai_denoise_reduces_noise_with_the_rnnoise_model() {
        if !ffmpeg_available() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_denoise_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        // ホワイトノイズのみの音声(4秒)。ノイズ除去なら大きく下がるはず。
        let noisy = tmp.join("noise.wav");
        let st = Command::new("ffmpeg").args(["-y", "-f", "lavfi", "-i", "anoisesrc=d=4:c=white:a=0.3:r=48000", noisy.to_str().unwrap()]).output().unwrap();
        assert!(st.status.success());
        let out = tmp.join("clean.wav");
        let job = ConvertJob {
            input_path: noisy.to_string_lossy().to_string(),
            output_path: out.to_string_lossy().to_string(),
            codec_args: vec!["-c:a".into(), "pcm_s16le".into()],
            bitrate: None,
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: None,
            fps: None,
            ai_denoise: Some(AiDenoise { mix: 1.0 }),
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: None,
            extra_tracks: vec![],
            ai_upscale: None,
            audio_bwe: None,
        };
        run_convert(&job).expect("AI denoise conversion should succeed");
        let (before, after) = (mean_volume_db(&noisy), mean_volume_db(&out));
        let _ = fs::remove_dir_all(&tmp);
        assert!(after < before - 6.0, "RNNoiseでノイズが6dB以上下がるはず(前: {before} dB, 後: {after} dB)");
    }

    fn hires_job(input: &Path, output: &Path, args: &[&str], denoise: bool) -> ConvertJob {
        ConvertJob {
            input_path: input.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: args.iter().map(|s| s.to_string()).collect(),
            bitrate: None,
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: None,
            fps: None,
            ai_denoise: denoise.then_some(AiDenoise { mix: 0.5 }),
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: None,
            extra_tracks: vec![],
            ai_upscale: None,
            audio_bwe: None,
        }
    }

    fn decode_f32_mono(path: &Path) -> Vec<f64> {
        let o = Command::new("ffmpeg").args(["-v", "error", "-i", path.to_str().unwrap(), "-af", "pan=mono|c0=c0", "-f", "f32le", "-"]).output().unwrap();
        o.stdout.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64).collect()
    }

    /// 高解像度PCM(R-2R等マルチビットDAC向け): 352.8kHz/24bitで出力され、1kHz正弦波の再現SNRが高いこと。
    #[test]
    fn real_ffmpeg_writes_high_resolution_pcm_with_high_snr() {
        if !ffmpeg_available() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_dxd_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let src = tmp.join("tone.wav");
        crate::engine::dsd::write_exact_sine_wav(&src, 1000.0, 0.875, 2.0, 44100, 2);

        for (name, args, rate, bits) in [
            ("dxd352.wav", vec!["-af", "hq-resample@352800", "-c:a", "pcm_s24le"], 352_800u32, "24"),
            ("pcm705.wav", vec!["-af", "hq-resample@705600", "-c:a", "pcm_s32le"], 705_600u32, "32"),
            ("pcm384_32.wav", vec!["-af", "hq-resample@384000", "-c:a", "pcm_s32le"], 384_000u32, "32"),
            ("pcm352_32.wav", vec!["-af", "hq-resample@352800", "-c:a", "pcm_s32le"], 352_800u32, "32"),
        ] {
            let out = tmp.join(name);
            run_convert(&hires_job(&src, &out, &args, false)).expect("high-resolution PCM conversion");
            let probe = Command::new("ffprobe").args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=sample_rate,bits_per_sample,bits_per_raw_sample", "-of", "csv=p=0", out.to_str().unwrap()]).output().unwrap();
            let text = String::from_utf8_lossy(&probe.stdout).to_string();
            assert!(text.contains(&rate.to_string()), "{name}のサンプルレートは{rate}Hzのはず(実際: {text})");
            let samples = decode_f32_mono(&out);
            let (snr, amp) = crate::engine::dsd::sine_fit_snr_db(&samples, 1000.0, rate as f64, rate as usize / 10);
            eprintln!("{name}: {rate} Hz / {bits} bit — 1kHz SNR {snr:.1} dB, 振幅 {amp:.4}");
            assert!(snr > 90.0, "{name}: リサンプル品質SNRは90dB超のはず(実際: {snr} dB)");
            assert!((amp - 0.875).abs() < 0.01, "{name}: 振幅が保たれるはず(実際: {amp})");
        }
        let _ = fs::remove_dir_all(&tmp);
    }

    /// DSDと同時に作るPCM版(FLAC 24bit/352.8kHz)と、AIノイズ除去との`-af`統合(片方が無視されない)。
    #[test]
    fn real_ffmpeg_flac_companion_and_merged_audio_filters() {
        if !ffmpeg_available() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_comp_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let src = tmp.join("noise.wav");
        let st = Command::new("ffmpeg").args(["-y", "-f", "lavfi", "-i", "anoisesrc=d=3:c=white:a=0.3:r=48000", src.to_str().unwrap()]).output().unwrap();
        assert!(st.status.success());
        let out = tmp.join("companion.flac");
        let args = ["-af", "hq-resample@352800", "-c:a", "flac", "-sample_fmt", "s32", "-bits_per_raw_sample", "24"];
        // ノイズ除去を併用: 高品質リサンプル(352.8kHz)とRNNoiseの両方が適用されるはず。
        run_convert(&hires_job(&src, &out, &args, true)).expect("companion FLAC with denoise");
        let probe = Command::new("ffprobe").args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=codec_name,sample_rate,bits_per_raw_sample", "-of", "csv=p=0", out.to_str().unwrap()]).output().unwrap();
        let text = String::from_utf8_lossy(&probe.stdout).trim().to_string();
        let (before, after) = (mean_volume_db(&src), mean_volume_db(&out));
        let _ = fs::remove_dir_all(&tmp);
        assert!(text.contains("flac") && text.contains("352800"), "FLAC 352.8kHzのはず(実際: {text})");
        assert!(after < before - 3.0, "ノイズ除去も適用されているはず(前: {before} dB, 後: {after} dB)");
    }

    /// 変換パイプライン全体(ノイズ除去なし→帯域拡張→FLAC出力)を実音源・実モデルで検証する。
    /// 8kHzで帯域制限した音楽を入力し、(1)高域(10〜16kHz)のエネルギーが増え、(2)低域(0〜7kHz)は変わらないことを確認する。
    /// 音源またはモデルを用意できない環境ではスキップする。
    #[test]
    fn real_pipeline_extends_the_band_of_a_lowpassed_music_clip_and_keeps_the_low_band() {
        // 評価用の音楽素材。後から同じ名前の形(…(1).mp4)の別ファイル(話し声の動画など)がフォルダに増えても
        // 変わらないよう、最も古いものを使う(2026-09-23、新しい動画が選ばれて失敗したため)。
        let Some(src) = std::fs::read_dir("C:\\AUDIO").ok().and_then(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.to_string_lossy().ends_with("(1).mp4"))
                .min_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
        }) else {
            eprintln!("評価用の音源が無いためスキップ");
            return;
        };
        if crate::engine::audio_sr::ensure_models().is_err() || !ffmpeg_available() {
            eprintln!("モデルまたはffmpegを用意できないためスキップ");
            return;
        }
        if crate::engine::probe::probe(&src.to_string_lossy()).map(|i| i.duration_secs < 610.0).unwrap_or(true) {
            eprintln!("評価用の音源が短すぎる(10分未満)ためスキップ: {}", src.display());
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_bwe_pipe_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let lowpassed = tmp.join("lp.wav");
        // 8kHzでかなり急峻に帯域制限する(2次を6段)。
        let st = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-ss", "600", "-t", "8", "-i", src.to_str().unwrap(), "-vn", "-ac", "2", "-ar", "48000", "-af", "lowpass=f=8000:poles=2,lowpass=f=8000:poles=2,lowpass=f=8000:poles=2,lowpass=f=8000:poles=2,lowpass=f=8000:poles=2,lowpass=f=8000:poles=2", "-c:a", "pcm_s16le", lowpassed.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(st.status.success(), "{}", String::from_utf8_lossy(&st.stderr));
        let out = tmp.join("out.flac");
        let mut job = hires_job(&lowpassed, &out, &["-c:a", "flac"], false);
        job.audio_bwe = Some(AudioBwe { cutoff_hz: None, speech: false }); // 自動検出
        run_convert(&job).expect("bandwidth extension pipeline should succeed");

        let band_power = |path: &Path, lo: f32, hi: f32| -> f64 {
            let o = Command::new("ffmpeg").args(["-v", "error", "-i", path.to_str().unwrap(), "-af", "pan=mono|c0=c0,ashowinfo", "-f", "null", "-"]).output().unwrap();
            let _ = o;
            let raw = Command::new("ffmpeg").args(["-v", "error", "-i", path.to_str().unwrap(), "-af", "pan=mono|c0=c0", "-ar", "48000", "-f", "f32le", "-"]).output().unwrap();
            let x: Vec<f32> = raw.stdout.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect();
            let n = x.len().min(48_000 * 6);
            let mut planner = rustfft::FftPlanner::<f32>::new();
            let fft = planner.plan_fft_forward(n);
            let mut buf: Vec<rustfft::num_complex::Complex<f32>> = x[..n].iter().map(|v| rustfft::num_complex::Complex::new(*v, 0.0)).collect();
            fft.process(&mut buf);
            (0..n / 2).filter(|&k| { let f = k as f32 * 48_000.0 / n as f32; f >= lo && f <= hi }).map(|k| buf[k].norm_sqr() as f64).sum::<f64>() / n as f64
        };
        let (hf_in, hf_out) = (band_power(&lowpassed, 10_000.0, 16_000.0), band_power(&out, 10_000.0, 16_000.0));
        let (lf_in, lf_out) = (band_power(&lowpassed, 100.0, 7_000.0), band_power(&out, 100.0, 7_000.0));
        let _ = fs::remove_dir_all(&tmp);
        eprintln!("高域(10-16kHz)のパワー: {hf_in:.3e} → {hf_out:.3e}、低域(0.1-7kHz): {lf_in:.3e} → {lf_out:.3e}");
        assert!(hf_out > hf_in * 10.0, "高域が生成されて増えるはず");
        assert!((lf_out / lf_in - 1.0).abs() < 0.02, "低域のパワーは保たれるはず(比 {})", lf_out / lf_in);
    }

    #[test]
    fn merge_audio_filters_joins_multiple_af_into_one_chain() {
        let merged = merge_audio_filters(vec!["-c:a".into(), "flac".into(), "-af".into(), "a".into(), "-af".into(), "b=1".into()]);
        assert_eq!(merged, vec!["-c:a", "flac", "-af", "a,b=1"]);
    }

    #[test]
    fn equal_interval_segments_splits_into_n_equal_parts() {
        let segments = equal_interval_segments(100.0, 4);
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[0], TrimRange { start_secs: Some(0.0), duration_secs: Some(25.0) });
        assert_eq!(segments[1], TrimRange { start_secs: Some(25.0), duration_secs: Some(25.0) });
        assert_eq!(segments[3], TrimRange { start_secs: Some(75.0), duration_secs: Some(25.0) });
    }

    #[test]
    fn equal_interval_segments_returns_empty_for_zero_count_or_duration() {
        assert!(equal_interval_segments(100.0, 0).is_empty());
        assert!(equal_interval_segments(0.0, 4).is_empty());
    }

    #[test]
    fn fixed_length_segments_splits_with_a_shorter_remainder_at_the_end() {
        let segments = fixed_length_segments(250.0, 100.0);
        assert_eq!(segments, vec![
            TrimRange { start_secs: Some(0.0), duration_secs: Some(100.0) },
            TrimRange { start_secs: Some(100.0), duration_secs: Some(100.0) },
            TrimRange { start_secs: Some(200.0), duration_secs: Some(50.0) },
        ]);
    }

    #[test]
    fn fixed_length_segments_exact_division_has_no_remainder() {
        let segments = fixed_length_segments(200.0, 100.0);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[1], TrimRange { start_secs: Some(100.0), duration_secs: Some(100.0) });
    }

    #[test]
    fn concat_media_requires_at_least_two_files() {
        let result = concat_media(&["only-one.mp4".to_string()], "out.mp4", true);
        assert!(result.is_err());
    }

    /// `concat_media`は音声トラックも結合対象にするため、`make_test_video`
    /// (映像のみ)ではなく、無音の音声トラックも持つテスト動画を作る。
    fn make_test_video_with_audio(dir: &Path, name: &str, duration_secs: u32) -> std::path::PathBuf {
        let path = dir.join(name);
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-f", "lavfi", "-i", &format!("testsrc=duration={duration_secs}:size=320x240:rate=10"),
                "-f", "lavfi", "-i", &format!("anullsrc=r=44100:cl=stereo:d={duration_secs}"),
                "-c:v", "libx264", "-g", "10", "-pix_fmt", "yuv420p",
                "-c:a", "aac",
                path.to_str().unwrap(),
            ])
            .output()
            .expect("ffmpeg should run");
        assert!(status.status.success(), "test fixture generation failed: {}", String::from_utf8_lossy(&status.stderr));
        path
    }

    #[test]
    fn real_ffmpeg_concat_produces_the_expected_total_duration() {
        if !ffmpeg_available() {
            eprintln!("ffmpegが見つからないためスキップ / skipping: ffmpeg not found on PATH");
            return;
        }

        let tmp = std::env::temp_dir().join(format!("make_disk_test_concat_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let a = make_test_video_with_audio(&tmp, "a.mp4", 3);
        let b = make_test_video_with_audio(&tmp, "b.mp4", 4);
        let output = tmp.join("concat_output.mp4");

        concat_media(&[a.to_string_lossy().to_string(), b.to_string_lossy().to_string()], output.to_str().unwrap(), true).expect("concat_media should succeed");

        let result_duration = probe_duration_secs(&output);
        assert!((result_duration - 7.0).abs() < 0.5, "結合後の尺は3秒+4秒=7秒に近いはず、実際: {result_duration}s");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn opus_limits_cap_bitrate_and_force_48khz() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let mut a = v(&["-c:a", "libopus", "-b:a", "900k"]);
        apply_opus_limits(&mut a);
        assert!(a.windows(2).any(|w| w[0] == "-b:a" && w[1] == "510k"), "{a:?}");
        assert!(a.windows(2).any(|w| w[0] == "-ar" && w[1] == "48000"));
        let mut a = v(&["-c:a", "libopus", "-ac", "6", "-b:a", "900k"]);
        apply_opus_limits(&mut a);
        assert!(a.contains(&"900k".to_string()), "6chは1536kbpsまで許容: {a:?}");
        let mut a = v(&["-c:a", "libopus", "-b:a", "128k", "-ar", "24000"]);
        apply_opus_limits(&mut a);
        assert_eq!(a, v(&["-c:a", "libopus", "-b:a", "128k", "-ar", "24000"]), "上限内・レート指定済みは変更しない");
        let mut a = v(&["-c:a", "aac", "-b:a", "900k"]);
        apply_opus_limits(&mut a);
        assert_eq!(a, v(&["-c:a", "aac", "-b:a", "900k"]));
    }

    fn stream_summary(path: &Path) -> Vec<String> {
        let out = Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_type:stream_tags=language,title", "-of", "csv=p=0", path.to_str().unwrap()])
            .output()
            .expect("ffprobe should run");
        String::from_utf8_lossy(&out.stdout).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
    }

    /// 映像1+音声2(jpn/eng)+字幕1(jpn)のMKVを作る。
    fn make_multitrack_mkv(dir: &Path) -> std::path::PathBuf {
        let srt = dir.join("in.srt");
        fs::write(&srt, "1
00:00:00,000 --> 00:00:01,500
こんにちは

").unwrap();
        let path = dir.join("multi.mkv");
        let status = Command::new("ffmpeg")
            .args([
                "-y", "-f", "lavfi", "-i", "testsrc=duration=3:size=320x240:rate=10",
                "-f", "lavfi", "-i", "sine=frequency=440:duration=3", "-f", "lavfi", "-i", "sine=frequency=880:duration=3",
                "-i", srt.to_str().unwrap(),
                "-map", "0:v", "-map", "1:a", "-map", "2:a", "-map", "3:s",
                "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-c:s", "srt",
                "-metadata:s:a:0", "language=jpn", "-metadata:s:a:1", "language=eng", "-metadata:s:s:0", "language=jpn",
                path.to_str().unwrap(),
            ])
            .output()
            .expect("ffmpeg should run");
        assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
        path
    }

    fn mkv_job(input: &Path, output: &Path) -> ConvertJob {
        ConvertJob {
            input_path: input.to_str().unwrap().to_string(),
            output_path: output.to_str().unwrap().to_string(),
            codec_args: vec!["-c:v".into(), "libx264".into(), "-pix_fmt".into(), "yuv420p".into(), "-c:a".into(), "aac".into()],
            bitrate: None,
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: None,
            fps: None,
            ai_denoise: None,
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: None,
            extra_tracks: vec![],
            ai_upscale: None,
            audio_bwe: None,
        }
    }

    #[test]
    fn real_mkv_keeps_all_audio_and_subtitle_tracks_by_default() {
        if !ffmpeg_available() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_mkv_keep_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let src = make_multitrack_mkv(&tmp);
        let out = tmp.join("out.mkv");
        run_convert(&mkv_job(&src, &out)).expect("MKV conversion should succeed");
        let s = stream_summary(&out);
        eprintln!("{s:?}");
        assert_eq!(s.iter().filter(|l| l.starts_with("audio")).count(), 2, "音声は2本のまま: {s:?}");
        assert_eq!(s.iter().filter(|l| l.starts_with("subtitle")).count(), 1, "字幕は1本のまま: {s:?}");
        assert!(s.iter().any(|l| l.contains("eng")) && s.iter().any(|l| l.contains("jpn")), "言語タグが残る: {s:?}");
        // 保持をオフにするとffmpegの既定(音声1本・字幕1本)に戻る
        let out2 = tmp.join("out_default.mkv");
        let mut job = mkv_job(&src, &out2);
        job.mkv_keep_all_tracks = Some(false);
        run_convert(&job).unwrap();
        assert_eq!(stream_summary(&out2).iter().filter(|l| l.starts_with("audio")).count(), 1);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn real_mkv_muxes_extra_audio_and_subtitle_tracks_with_language_and_title() {
        if !ffmpeg_available() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_mkv_extra_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let src = make_multitrack_mkv(&tmp);
        let extra_audio = tmp.join("commentary.wav");
        assert!(Command::new("ffmpeg").args(["-y", "-f", "lavfi", "-i", "sine=frequency=220:duration=3", extra_audio.to_str().unwrap()]).output().unwrap().status.success());
        let extra_sub = tmp.join("eng.srt");
        fs::write(&extra_sub, "1
00:00:00,000 --> 00:00:01,500
Hello

").unwrap();
        let out = tmp.join("out.mkv");
        let mut job = mkv_job(&src, &out);
        job.extra_tracks = vec![
            crate::engine::mkv_tracks::ExtraTrack { path: extra_audio.to_str().unwrap().into(), kind: "audio".into(), language: Some("fra".into()), title: Some("Commentary".into()) },
            crate::engine::mkv_tracks::ExtraTrack { path: extra_sub.to_str().unwrap().into(), kind: "subtitle".into(), language: Some("eng".into()), title: None },
        ];
        run_convert(&job).expect("MKV with extra tracks should succeed");
        let s = stream_summary(&out);
        eprintln!("{s:?}");
        assert_eq!(s.iter().filter(|l| l.starts_with("audio")).count(), 3, "元2本+追加1本: {s:?}");
        assert_eq!(s.iter().filter(|l| l.starts_with("subtitle")).count(), 2, "元1本+追加1本: {s:?}");
        assert!(s.iter().any(|l| l.starts_with("audio") && l.contains("fra") && l.contains("Commentary")), "追加音声の言語・タイトル: {s:?}");
        assert!(s.iter().any(|l| l.starts_with("subtitle") && l.contains("eng")), "追加字幕の言語: {s:?}");
        let _ = fs::remove_dir_all(&tmp);
    }
    /// 2026-09-23の実バグの回帰テスト: 動画(MP4)から音声だけのWAVをカット付きで作ると、
    /// 以前は中間区間ファイルを出力と同じ.wavで`-c copy`していたためAACが壊れ、極端に短いWAVになった。
    #[test]
    fn real_ffmpeg_cut_from_mp4_to_wav_keeps_the_expected_duration() {
        if !ffmpeg_available() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_cut_wav_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let src = make_test_video_with_audio(&tmp, "src.mp4", 20);
        let out = tmp.join("out.wav");
        let job = ConvertJob {
            input_path: src.to_string_lossy().to_string(),
            output_path: out.to_string_lossy().to_string(),
            codec_args: vec!["-c:a".into(), "pcm_s16le".into()],
            bitrate: None,
            trim: None,
            cut_ranges: Some(vec![CutRange { start_secs: 12.0, end_secs: None }]),
            frame_accurate: false,
            resolution: None,
            fps: None,
            ai_denoise: None,
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: None,
            extra_tracks: vec![],
            ai_upscale: None,
            audio_bwe: None,
        };
        run_convert(&job).expect("MP4→WAVのカット変換は成功するはず");
        let dur = crate::engine::probe::probe(&out.to_string_lossy()).unwrap().duration_secs;
        let _ = fs::remove_dir_all(&tmp);
        assert!((dur - 12.0).abs() < 0.5, "先頭12秒が残るはず、実際: {dur}秒");
    }

    #[test]
    fn accelerate_h264_args_only_touches_libx264() {
        let v = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        // H.264以外(音声のみ・VP9)はそのまま
        assert_eq!(accelerate_h264_args(v(&["-c:a", "pcm_s16le"])), v(&["-c:a", "pcm_s16le"]));
        assert_eq!(accelerate_h264_args(v(&["-c:v", "libvpx-vp9", "-c:a", "libopus"])), v(&["-c:v", "libvpx-vp9", "-c:a", "libopus"]));
        // libx264はGPUエンコーダへ置き換わるか、速度優先の-presetが付く
        let out = accelerate_h264_args(v(&["-c:v", "libx264", "-c:a", "aac"]));
        let enc = &out[1];
        assert!(enc.starts_with("h264_") || (enc == "libx264" && out.iter().any(|a| a == "-preset")), "{out:?}");
    }

    /// 2026-09-24: 「ディスクいっぱいに収める」動画は2パスで目標サイズに合わせる。
    /// DVD画質(720x480)の素材をフルHDへ拡大し、10MBに収まるビットレートを指定して、はみ出さずほぼいっぱいになることを確認する。
    #[test]
    fn real_ffmpeg_two_pass_fills_the_target_size_without_overflow() {
        if !ffmpeg_available() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_2pass_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let src = tmp.join("dvd.mkv");
        let st = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=720x480:rate=30:duration=20", "-f", "lavfi", "-i", "sine=frequency=440:duration=20",
                   "-c:v", "libx264", "-b:v", "4000k", "-c:a", "aac", src.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(st.status.success(), "{}", String::from_utf8_lossy(&st.stderr));
        let target_bytes: u64 = 10_000_000;
        let total_kbps = target_bytes * 8 / 20 / 1000;
        let video_kbps = total_kbps * 98 / 100 - 320;
        let out = tmp.join("out.mkv");
        let job = ConvertJob {
            input_path: src.to_string_lossy().to_string(),
            output_path: out.to_string_lossy().to_string(),
            codec_args: vec!["-c:v".into(), "libx264".into(), "-c:a".into(), "aac".into(), "-b:a".into(), "320k".into()],
            bitrate: Some(BitrateMode::AutoMaxForCapacity(video_kbps)),
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: Some(Resolution { width: 1920, height: 1080 }),
            fps: None,
            ai_denoise: None,
            dsd_rate: None,
            dop_wav_bits: None,
            mkv_keep_all_tracks: None,
            extra_tracks: vec![],
            ai_upscale: None,
            audio_bwe: None,
        };
        run_convert(&job).expect("2-pass encode should succeed");
        let size = fs::metadata(&out).unwrap().len();
        let _ = fs::remove_dir_all(&tmp);
        let ratio = size as f64 / target_bytes as f64;
        eprintln!("2パス: 目標{target_bytes}バイトに対し{size}バイト({:.1}%)", ratio * 100.0);
        assert!(ratio <= 1.0, "目標サイズを超えてはいけない: {:.1}%", ratio * 100.0);
        assert!(ratio >= 0.85, "ほぼいっぱいになるはず: {:.1}%", ratio * 100.0);
    }

}
