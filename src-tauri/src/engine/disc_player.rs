//! ディスクプレーヤー用: DVD-Video / Blu-ray / データディスクの再生項目の列挙と、再生用ファイルの用意。
//!
//! WebView(Chromium系)はVOB(MPEG-PS)やm2tsを直接再生できないため、ブラウザで再生できない形式は
//! ffmpegで一時フォルダへH.264+AACのMP4に変換してから再生する(変換が終わるまで待つ。長い本編は時間がかかる)。
//! コピーガード(CSS/AACS)で暗号化された市販ディスクは読めず、保護の回避は行わない。

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct DiscItem {
    pub label: String,
    /// 連結して1本として再生するファイル(DVDタイトルは複数のVOB)。
    pub paths: Vec<String>,
    pub bytes: u64,
    /// "dvd-title" / "bd-stream" / "file"
    pub kind: String,
}

const MEDIA_EXT: [&str; 18] = ["mp3", "wav", "flac", "aac", "m4a", "ogg", "dsf", "mp4", "mkv", "avi", "mov", "webm", "mpg", "mpeg", "vob", "m2ts", "ts", "wma"];
const BROWSER_OK: [&str; 8] = ["mp3", "wav", "flac", "aac", "m4a", "ogg", "mp4", "webm"];

/// 書き込み非対応のDVD-ROM等も含め、全ての光学ドライブを列挙する。
pub fn list_optical_drives() -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", "Get-CimInstance Win32_CDROMDrive | ForEach-Object { $_.Drive }"])
            .creation_flags(0x0800_0000)
            .output()
            .map_err(|e| format!("光学ドライブの列挙に失敗しました: {e}"))?;
        Ok(String::from_utf8_lossy(&out.stdout).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
    }
    #[cfg(not(windows))]
    {
        crate::engine::burn::list_devices()
    }
}

fn root_of(drive: &str) -> PathBuf {
    let d = drive.trim_end_matches(['\\', '/']);
    if d.len() == 2 && d.ends_with(':') {
        PathBuf::from(format!("{d}\\"))
    } else {
        PathBuf::from(d)
    }
}

fn size(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

pub fn list_items(drive: &str) -> Result<Vec<DiscItem>, String> {
    let root = root_of(drive);
    if !root.exists() {
        return Err("ディスクが入っていないか、読めません / No readable disc".into());
    }
    let mut items = Vec::new();
    // DVD-Video: VIDEO_TS/VTS_nn_m.VOB(m>=1)をタイトルnnごとに連結。
    if let Ok(rd) = std::fs::read_dir(root.join("VIDEO_TS")) {
        let mut by_title: std::collections::BTreeMap<String, Vec<PathBuf>> = Default::default();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_uppercase();
            if name.starts_with("VTS_") && name.ends_with(".VOB") && name.len() == 12 && !name.ends_with("_0.VOB") {
                by_title.entry(name[4..6].to_string()).or_default().push(e.path());
            }
        }
        let mut v: Vec<DiscItem> = by_title
            .into_iter()
            .map(|(t, mut files)| {
                files.sort();
                let bytes = files.iter().map(|p| size(p)).sum();
                DiscItem { label: format!("DVDタイトル {t}"), paths: files.iter().map(|p| p.to_string_lossy().to_string()).collect(), bytes, kind: "dvd-title".into() }
            })
            .collect();
        v.sort_by(|a, b| b.bytes.cmp(&a.bytes));
        items.extend(v);
    }
    // Blu-ray: BDMV/STREAM/*.m2ts(大きい順)。
    if let Ok(rd) = std::fs::read_dir(root.join("BDMV").join("STREAM")) {
        let mut v: Vec<DiscItem> = rd
            .flatten()
            .filter(|e| e.path().extension().map(|x| x.eq_ignore_ascii_case("m2ts")).unwrap_or(false))
            .map(|e| DiscItem { label: format!("Blu-ray {}", e.file_name().to_string_lossy()), paths: vec![e.path().to_string_lossy().to_string()], bytes: size(&e.path()), kind: "bd-stream".into() })
            .collect();
        v.sort_by(|a, b| b.bytes.cmp(&a.bytes));
        items.extend(v);
    }
    if items.is_empty() {
        walk(&root, 0, &mut items);
    }
    Ok(items)
}

fn walk(dir: &Path, depth: u32, out: &mut Vec<DiscItem>) {
    if depth > 3 || out.len() > 500 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            walk(&p, depth + 1, out);
        } else if p.extension().and_then(|x| x.to_str()).map(|x| MEDIA_EXT.contains(&x.to_lowercase().as_str())).unwrap_or(false) {
            out.push(DiscItem { label: p.to_string_lossy().to_string(), paths: vec![p.to_string_lossy().to_string()], bytes: size(&p), kind: "file".into() });
        }
    }
}

/// 再生できる形のファイルを返す。ブラウザで再生できる単一ファイルはそのまま、他はMP4/WAVへ変換する。
pub fn prepare(paths: &[String], video_hint: bool, target_height: Option<u32>) -> Result<String, String> {
    let first = paths.first().ok_or("再生する項目がありません")?;
    let ext = Path::new(first).extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
    // 映像は、解像度の引き上げ(アップコンバート)を頼まれたときは必ず変換経由にする。
    if paths.len() == 1 && BROWSER_OK.contains(&ext.as_str()) && (!video_hint || target_height.is_none()) {
        return Ok(first.clone());
    }
    let dir = std::env::temp_dir().join("make-disk-player");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&(paths, target_height), &mut h);
    let key = std::hash::Hasher::finish(&h);
    let e = if video_hint { "mp4" } else { "wav" };
    let out = dir.join(format!("disc-{key:x}.{e}"));
    if out.exists() && size(&out) > 0 {
        return Ok(out.to_string_lossy().to_string());
    }
    let input = if paths.len() == 1 { paths[0].clone() } else { format!("concat:{}", paths.join("|")) };
    let mut cmd = crate::engine::sidecar::resolve_tool("ffmpeg");
    cmd.args(["-y", "-hide_banner", "-v", "error", "-i", &input]);
    if video_hint {
        // 元の高さが目標より低いときだけlanczosで引き上げる(高い映像は縮めない)。補間であり、失われた細部の復元ではない。
        let vf = match target_height {
            Some(h) => format!("yadif=deint=interlaced,scale=w=-2:h='max(ih,{h})':flags=lanczos,format=yuv420p"),
            None => "yadif=deint=interlaced,format=yuv420p".to_string(),
        };
        cmd.args(["-vf", vf.as_str(), "-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart"]);
    } else {
        cmd.args(["-vn", "-c:a", "pcm_s24le"]);
    }
    let tmp = dir.join(format!("disc-{key:x}.part.{e}"));
    let st = cmd.arg(&tmp).output().map_err(|e| format!("ffmpegを起動できません: {e}"))?;
    if !st.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("再生用への変換に失敗しました(暗号化ディスクの可能性): {}", String::from_utf8_lossy(&st.stderr).chars().take(300).collect::<String>()));
    }
    std::fs::rename(&tmp, &out).map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn groups_dvd_vobs_by_title_and_skips_menu() {
        let d = std::env::temp_dir().join(format!("md_disc_{}", std::process::id()));
        let v = d.join("VIDEO_TS");
        std::fs::create_dir_all(&v).unwrap();
        for n in ["VTS_01_0.VOB", "VTS_01_1.VOB", "VTS_01_2.VOB", "VTS_02_1.VOB"] {
            std::fs::write(v.join(n), vec![0u8; 10]).unwrap();
        }
        let items = list_items(d.to_str().unwrap()).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items.iter().find(|i| i.label.ends_with("01")).unwrap().paths.len(), 2);
        let _ = std::fs::remove_dir_all(&d);
    }
}
