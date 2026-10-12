//! 音楽CD(CD-DA)からのトラック取り込み(リッピング、2026-09-19新設)。
//!
//! Windowsでは`\\.\D:`を開き、`IOCTL_CDROM_READ_TOC`でトラック一覧を、`IOCTL_CDROM_RAW_READ`(CDDAモード、
//! 1セクタ2352バイト)で音声データを読む。読み取り層を`SectorReader`に抽象化しているので、TOC解析・セキュアリード・
//! WAV書き出しはCD無しで単体テストできる。
//!
//! ## セキュアリード(簡易)
//! 各チャンクを**2回読んで一致を確認**し、不一致なら最大`MAX_RETRIES`回まで読み直す(cdparanoiaのような
//! ジッタ補正・キャッシュ対策・C2エラー訂正までは行わない、正直な開示)。読み取り速度は約半分になる。
//!
//! ## 正直な開示
//! - コピーコントロール付きCDや傷のあるディスクでは失敗することがある。**保護の回避は行わない**
//!   (読めない場合はエラーとして報告する)。
//! - CDDB/MusicBrainzの曲名取得はしない(`Track01.wav`のような名前で保存)。
//! - Linux/macOSは未対応(Windowsのみ)。cdparanoia等の利用を案内する。

use std::io::Write;
use std::path::Path;

pub const SECTOR_BYTES: usize = 2352;
const MAX_RETRIES: u32 = 8;
/// 1回の読み取りで扱うセクタ数(RAW_READの上限を超えない安全な値)。
const CHUNK_SECTORS: u32 = 16;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TrackInfo {
    pub number: u8,
    pub start_lba: i64,
    pub sectors: i64,
    pub is_audio: bool,
}

impl TrackInfo {
    pub fn duration_secs(&self) -> f64 {
        self.sectors as f64 / 75.0
    }
}

/// MSF(分・秒・フレーム)からLBA(論理ブロックアドレス)へ。CDは先頭に150フレームのポーズがある。
pub fn msf_to_lba(m: u8, s: u8, f: u8) -> i64 {
    (m as i64 * 60 + s as i64) * 75 + f as i64 - 150
}

/// `IOCTL_CDROM_READ_TOC`の出力(`CDROM_TOC`構造体)を解析する。
/// 先頭4バイト: 長さ(2、ビッグエンディアン)・最初のトラック・最後のトラック。以降8バイトずつのエントリ:
/// `[予約, Control<<0|Adr<<4の1バイト, トラック番号, 予約, 予約, M, S, F]`。最後はリードアウト(トラック番号0xAA)。
pub fn parse_toc(bytes: &[u8]) -> Result<Vec<TrackInfo>, String> {
    if bytes.len() < 4 {
        return Err("TOCが短すぎます".to_string());
    }
    let length = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
    let entries = (length.saturating_sub(2)) / 8;
    if entries == 0 || bytes.len() < 4 + entries * 8 {
        return Err("TOCにトラックがありません(ディスクが入っていない、または音楽CDではありません)".to_string());
    }
    let mut raw: Vec<(u8, u8, i64)> = Vec::new(); // (番号, control, lba)
    for i in 0..entries {
        let e = &bytes[4 + i * 8..4 + (i + 1) * 8];
        raw.push((e[2], e[1] & 0x0F, msf_to_lba(e[5], e[6], e[7])));
    }
    let mut tracks = Vec::new();
    for w in raw.windows(2) {
        let (n, control, lba) = w[0];
        if n == 0xAA {
            break;
        }
        let next_lba = w[1].2;
        tracks.push(TrackInfo { number: n, start_lba: lba, sectors: next_lba - lba, is_audio: control & 0x04 == 0 });
    }
    if tracks.is_empty() {
        return Err("有効なトラックが見つかりません".to_string());
    }
    Ok(tracks)
}

/// セクタ読み取りの抽象(実ドライブまたはテスト用のモック)。
pub trait SectorReader {
    fn read_sectors(&mut self, lba: i64, count: u32) -> Result<Vec<u8>, String>;
}

/// 同じ範囲を2回読み、一致するまで最大`MAX_RETRIES`回読み直す簡易セキュアリード。
pub fn secure_read(reader: &mut dyn SectorReader, lba: i64, count: u32) -> Result<Vec<u8>, String> {
    let mut previous = reader.read_sectors(lba, count)?;
    for _ in 0..MAX_RETRIES {
        let again = reader.read_sectors(lba, count)?;
        if again == previous {
            return Ok(again);
        }
        previous = again;
    }
    Err(format!("LBA {lba}付近で読み取り結果が安定しません(傷・汚れ、またはコピーコントロールの可能性) / read results at LBA {lba} never stabilised"))
}

fn write_wav_header<W: Write>(w: &mut W, data_bytes: u32) -> std::io::Result<()> {
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_bytes).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?; // PCM
    w.write_all(&2u16.to_le_bytes())?; // ステレオ
    w.write_all(&44_100u32.to_le_bytes())?;
    w.write_all(&(44_100u32 * 4).to_le_bytes())?;
    w.write_all(&4u16.to_le_bytes())?;
    w.write_all(&16u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_bytes.to_le_bytes())
}

/// 1トラックを16bit/44.1kHz/ステレオのWAVとして`output`へ書き出す。
pub fn rip_track(reader: &mut dyn SectorReader, track: &TrackInfo, output: &Path, secure: bool) -> Result<(), String> {
    if !track.is_audio {
        return Err(format!("トラック{}はデータトラックで、音声として取り込めません", track.number));
    }
    let file = std::fs::File::create(output).map_err(|e| format!("出力ファイルを作成できません: {e}"))?;
    let mut out = std::io::BufWriter::new(file);
    let data_bytes = (track.sectors as usize * SECTOR_BYTES) as u32;
    write_wav_header(&mut out, data_bytes).map_err(|e| e.to_string())?;
    let mut done = 0i64;
    while done < track.sectors {
        let count = (track.sectors - done).min(CHUNK_SECTORS as i64) as u32;
        let lba = track.start_lba + done;
        let data = if secure { secure_read(reader, lba, count)? } else { reader.read_sectors(lba, count)? };
        if data.len() != count as usize * SECTOR_BYTES {
            return Err(format!("LBA {lba}で想定外のサイズのデータが返りました({}バイト)", data.len()));
        }
        out.write_all(&data).map_err(|e| format!("書き込みに失敗しました: {e}"))?;
        done += count as i64;
    }
    out.flush().map_err(|e| e.to_string())
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;

    const IOCTL_CDROM_READ_TOC: u32 = 0x0002_4000;
    const IOCTL_CDROM_RAW_READ: u32 = 0x0002_403E;

    #[repr(C)]
    struct RawReadInfo {
        disk_offset: i64,
        sector_count: u32,
        track_mode: u32, // 2 = CDDA
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn DeviceIoControl(h: *mut core::ffi::c_void, code: u32, inbuf: *const core::ffi::c_void, insz: u32, outbuf: *mut core::ffi::c_void, outsz: u32, returned: *mut u32, overlapped: *mut core::ffi::c_void) -> i32;
    }

    pub struct Drive {
        file: std::fs::File,
    }

    impl Drive {
        /// `"D:"`のようなドライブレターの光学ドライブを開く。
        pub fn open(letter: &str) -> Result<Self, String> {
            let path = format!("\\\\.\\{}", letter.trim_end_matches('\\'));
            let file = std::fs::OpenOptions::new().read(true).share_mode(3).open(&path).map_err(|e| format!("ドライブ{letter}を開けません(ディスクが入っていない、または権限不足): {e}"))?;
            Ok(Drive { file })
        }

        pub fn read_toc(&self) -> Result<Vec<u8>, String> {
            let mut buf = vec![0u8; 804];
            let mut returned = 0u32;
            let ok = unsafe { DeviceIoControl(self.file.as_raw_handle() as *mut _, IOCTL_CDROM_READ_TOC, std::ptr::null(), 0, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut returned, std::ptr::null_mut()) };
            if ok == 0 {
                return Err(format!("TOCを読めません(ディスクが入っていない可能性): {}", std::io::Error::last_os_error()));
            }
            buf.truncate(returned as usize);
            Ok(buf)
        }
    }

    impl SectorReader for Drive {
        fn read_sectors(&mut self, lba: i64, count: u32) -> Result<Vec<u8>, String> {
            let info = RawReadInfo { disk_offset: lba * 2048, sector_count: count, track_mode: 2 };
            let mut buf = vec![0u8; count as usize * SECTOR_BYTES];
            let mut returned = 0u32;
            let ok = unsafe { DeviceIoControl(self.file.as_raw_handle() as *mut _, IOCTL_CDROM_RAW_READ, &info as *const _ as *const _, std::mem::size_of::<RawReadInfo>() as u32, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut returned, std::ptr::null_mut()) };
            if ok == 0 {
                return Err(format!("LBA {lba}の読み取りに失敗しました: {}", std::io::Error::last_os_error()));
            }
            buf.truncate(returned as usize);
            Ok(buf)
        }
    }
}

/// 光学ドライブのトラック一覧を読む(Windows)。
#[cfg(windows)]
pub fn list_tracks(drive: &str) -> Result<Vec<TrackInfo>, String> {
    parse_toc(&win::Drive::open(drive)?.read_toc()?)
}

#[cfg(not(windows))]
pub fn list_tracks(_drive: &str) -> Result<Vec<TrackInfo>, String> {
    Err("音楽CDの取り込みは現在Windowsのみ対応です(Linux/macOSはcdparanoia等をお使いください) / CD-DA ripping currently supports Windows only".to_string())
}

/// 指定トラックを`out_dir`へ`Track01.wav`のように書き出し、生成したパスの一覧を返す。
#[cfg(windows)]
pub fn rip_tracks(drive: &str, track_numbers: &[u8], out_dir: &Path, secure: bool) -> Result<Vec<String>, String> {
    let mut d = win::Drive::open(drive)?;
    let tracks = parse_toc(&d.read_toc()?)?;
    std::fs::create_dir_all(out_dir).map_err(|e| format!("出力フォルダを作成できません: {e}"))?;
    let mut outputs = Vec::new();
    for n in track_numbers {
        let t = tracks.iter().find(|t| t.number == *n).ok_or_else(|| format!("トラック{n}は存在しません"))?;
        let path = out_dir.join(format!("Track{n:02}.wav"));
        rip_track(&mut d, t, &path, secure)?;
        outputs.push(path.to_string_lossy().to_string());
    }
    Ok(outputs)
}

#[cfg(not(windows))]
pub fn rip_tracks(_drive: &str, _track_numbers: &[u8], _out_dir: &Path, _secure: bool) -> Result<Vec<String>, String> {
    Err("音楽CDの取り込みは現在Windowsのみ対応です / CD-DA ripping currently supports Windows only".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 3トラック(うち1つはデータ)+リードアウトの、実際のTOCバイナリ形式のデータを作る。
    fn sample_toc() -> Vec<u8> {
        let entry = |control: u8, track: u8, m: u8, s: u8, f: u8| [0u8, control, track, 0, 0, m, s, f];
        let entries = [
            entry(0x10, 1, 0, 2, 0),   // 音声、LBA 0
            entry(0x10, 2, 3, 2, 0),   // 音声、LBA 3*60*75=13500
            entry(0x14, 3, 6, 2, 0),   // データ(Control bit2)、LBA 27000
            entry(0x10, 0xAA, 8, 2, 0), // リードアウト、LBA 36000
        ];
        let mut v = vec![0u8; 4];
        let length = (2 + entries.len() * 8) as u16;
        v[0..2].copy_from_slice(&length.to_be_bytes());
        v[2] = 1;
        v[3] = 3;
        for e in entries {
            v.extend_from_slice(&e);
        }
        v
    }

    #[test]
    fn msf_conversion_accounts_for_the_two_second_pregap() {
        assert_eq!(msf_to_lba(0, 2, 0), 0);
        assert_eq!(msf_to_lba(74, 0, 0), 74 * 60 * 75 - 150);
    }

    #[test]
    fn toc_parsing_computes_lengths_and_detects_data_tracks() {
        let tracks = parse_toc(&sample_toc()).unwrap();
        assert_eq!(tracks.len(), 3);
        assert_eq!(tracks[0], TrackInfo { number: 1, start_lba: 0, sectors: 13_500, is_audio: true });
        assert_eq!(tracks[1].sectors, 13_500);
        assert!(!tracks[2].is_audio, "Controlビット2が立つトラックはデータトラック");
        assert!((tracks[0].duration_secs() - 180.0).abs() < 1e-9);
        assert!(parse_toc(&[0, 2, 1, 1]).is_err(), "空のTOCはエラー");
    }

    /// 同じLBAに対して決まったデータを返すモック。指定回数だけ最初の読み取りを壊すこともできる。
    struct MockReader {
        corrupt_reads_left: u32,
        calls: u32,
    }

    impl MockReader {
        fn sector_data(lba: i64, count: u32) -> Vec<u8> {
            (0..count as usize * SECTOR_BYTES).map(|i| ((lba as usize * 31 + i) % 251) as u8).collect()
        }
    }

    impl SectorReader for MockReader {
        fn read_sectors(&mut self, lba: i64, count: u32) -> Result<Vec<u8>, String> {
            self.calls += 1;
            let mut d = Self::sector_data(lba, count);
            if self.corrupt_reads_left > 0 {
                self.corrupt_reads_left -= 1;
                d[100] ^= 0xFF; // 読み取りエラーを模擬(1バイト化け)
            }
            Ok(d)
        }
    }

    #[test]
    fn secure_read_retries_until_two_reads_match() {
        // 最初の1回だけ壊れる: 1回目(壊れ)と2回目(正常)は不一致、3回目(正常)で2回目と一致して確定する。
        let mut r = MockReader { corrupt_reads_left: 1, calls: 0 };
        let data = secure_read(&mut r, 100, 4).unwrap();
        assert_eq!(data, MockReader::sector_data(100, 4), "壊れた読み取りは採用されず、正しいデータが返るはず");
        assert_eq!(r.calls, 3);
    }

    #[test]
    fn secure_read_gives_up_on_a_persistently_unstable_disc() {
        struct Flaky(u8);
        impl SectorReader for Flaky {
            fn read_sectors(&mut self, _lba: i64, count: u32) -> Result<Vec<u8>, String> {
                self.0 = self.0.wrapping_add(1);
                Ok(vec![self.0; count as usize * SECTOR_BYTES])
            }
        }
        assert!(secure_read(&mut Flaky(0), 0, 1).is_err(), "毎回結果が違うディスクはエラーになるはず");
    }

    #[test]
    fn ripping_writes_a_valid_wav_with_the_exact_audio_bytes() {
        let track = TrackInfo { number: 1, start_lba: 500, sectors: 40, is_audio: true }; // チャンク16の倍数でない長さ
        let path = std::env::temp_dir().join(format!("make_disk_cdda_test_{}.wav", std::process::id()));
        let mut r = MockReader { corrupt_reads_left: 0, calls: 0 };
        rip_track(&mut r, &track, &path, true).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(bytes.len(), 44 + 40 * SECTOR_BYTES);
        let expected: Vec<u8> = (0..40).step_by(16).flat_map(|off| MockReader::sector_data(500 + off as i64, (40 - off).min(16) as u32)).collect();
        assert_eq!(&bytes[44..], &expected[..], "音声データはセクタの内容と一致するはず");
        assert!(rip_track(&mut r, &TrackInfo { number: 2, start_lba: 0, sectors: 1, is_audio: false }, &path, false).is_err(), "データトラックは取り込めない");
    }

    /// 実機の光学ドライブでTOCを読む(ディスクが無い/データCDの場合は、その旨のエラーまたは音声トラック無しになる)。
    #[cfg(windows)]
    #[test]
    fn real_drive_toc_query_behaves_sensibly() {
        let drives = crate::engine::burn::list_devices().unwrap_or_default();
        let Some(drive) = drives.first() else {
            eprintln!("光学ドライブが無いためスキップ");
            return;
        };
        match list_tracks(drive) {
            Ok(tracks) => eprintln!("{drive}: {}トラック、音声{}本 → {:?}", tracks.len(), tracks.iter().filter(|t| t.is_audio).count(), tracks),
            Err(e) => eprintln!("{drive}: {e}"),
        }
    }

    /// 実際の音楽CDの先頭音声トラックを読み(最大20秒)、WAV化・無音でないことを確かめる(ディスクが無い場合はスキップ)。
    #[cfg(windows)]
    #[test]
    fn real_disc_rips_the_first_audio_track() {
        let drives = crate::engine::burn::list_devices().unwrap_or_default();
        let Some(drive) = drives.first() else { return };
        let Ok(tracks) = list_tracks(drive) else { return };
        let Some(t) = tracks.iter().find(|t| t.is_audio) else { return };
        eprintln!("{drive}: {}トラック、Track{} {}秒", tracks.len(), t.number, t.duration_secs() as u32);
        let part = TrackInfo { sectors: t.sectors.min(1500), ..t.clone() };
        let out = std::env::temp_dir().join("make_disk_real_rip.wav");
        let mut d = win::Drive::open(drive).unwrap();
        let started = std::time::Instant::now();
        rip_track(&mut d, &part, &out, true).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        eprintln!("{}秒分を{:.1}秒で取り込み(セキュアリード)", part.sectors / 75, started.elapsed().as_secs_f64());
        assert_eq!(bytes.len(), 44 + part.sectors as usize * SECTOR_BYTES);
        let peak = bytes[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs()).max().unwrap();
        eprintln!("ピーク振幅: {peak}");
        let _ = std::fs::remove_file(&out);
        assert!(peak > 100, "無音のはずがない");
    }

    /// 実ディスクの最終トラックを丸ごとセキュアリードで取り込み、環境変数`MAKE_DISK_RIP_OUT`のパスへ保存する(手動確認用)。
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn real_disc_full_rip_of_last_track() {
        let out = std::env::var("MAKE_DISK_RIP_OUT").expect("set MAKE_DISK_RIP_OUT");
        let drives = crate::engine::burn::list_devices().unwrap();
        let tracks = list_tracks(&drives[0]).unwrap();
        let t = tracks.iter().rev().find(|t| t.is_audio).unwrap();
        let mut d = win::Drive::open(&drives[0]).unwrap();
        let started = std::time::Instant::now();
        rip_track(&mut d, t, std::path::Path::new(&out), true).unwrap();
        eprintln!("Track{} {}秒を{:.1}秒で取り込み", t.number, t.duration_secs() as u32, started.elapsed().as_secs_f64());
    }

    /// 実ディスクの全音声トラックを`MAKE_DISK_RIP_DIR`へ丸ごと取り込む(手動確認用、コピーコントロールCDの検証に使用)。
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn real_disc_full_rip_all_tracks() {
        let dir = std::env::var("MAKE_DISK_RIP_DIR").expect("set MAKE_DISK_RIP_DIR");
        let drives = crate::engine::burn::list_devices().unwrap();
        let tracks = list_tracks(&drives[0]).unwrap();
        let nums: Vec<u8> = tracks.iter().filter(|t| t.is_audio).map(|t| t.number).collect();
        let started = std::time::Instant::now();
        let outs = rip_tracks(&drives[0], &nums, std::path::Path::new(&dir), true).unwrap();
        eprintln!("{}トラックを{:.1}秒で取り込み", outs.len(), started.elapsed().as_secs_f64());
    }
}
