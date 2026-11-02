//! ビルド時に、実行時アプリ本体+同梱ツールを1本のzip(`payload.zip`)へまとめ、
//! `main.rs`から`include_bytes!`で埋め込む(単一自己完結exeにするため)。
//!
//! **正直な開示**: 本家xorriso(GPL、GNU/Linux・Cygwin依存が強くWindows向けの
//! シンプルな静的exeが無い)は現時点でここに同梱できていない
//! (`CLAUDE.md`/`installer-exe/README.md`に既知の欠落として記録)。
//! 同梱するのは: make-disk本体・本家ffmpeg/ffprobe・実験的な純Rust版
//! rs-ffmpeg/rs-xorriso(いずれも`fetch-ffmpeg-sidecars.sh`/
//! `build-rs-tribute-sidecars.sh`が`src-tauri/binaries/`へ用意した実バイナリ)。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent().expect("installer-exe should live directly under the make-disk repo root");
    let bin_dir = repo_root.join("src-tauri/binaries");
    let make_disk_exe = repo_root.join("src-tauri/target/release/make-disk.exe");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let payload_path = out_dir.join("payload.zip");
    let file = fs::File::create(&payload_path).expect("create payload.zip");
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // (埋め込み先ファイル名, ソースパス, 無いとビルド自体を失敗させるか)
    let entries: [(&str, PathBuf, bool); 5] = [
        ("make-disk.exe", make_disk_exe, true),
        ("ffmpeg.exe", bin_dir.join("ffmpeg-x86_64-pc-windows-msvc.exe"), true),
        ("ffprobe.exe", bin_dir.join("ffprobe-x86_64-pc-windows-msvc.exe"), true),
        ("rs-ffmpeg.exe", bin_dir.join("rs-ffmpeg-x86_64-pc-windows-msvc.exe"), false),
        ("rs-xorriso.exe", bin_dir.join("rs-xorriso-x86_64-pc-windows-msvc.exe"), false),
    ];

    for (name, src, required) in &entries {
        add_file(&mut zip, &opts, name, src, *required);
    }

    // 本家xorrisoは未同梱(上記の理由)。空き枠として明示し、`engine/sidecar.rs`と同じく
    // 実行時にPATH上のxorrisoへフォールバックさせる(インストーラーが黙って壊れた状態を
    // 作らないよう、README/completion画面で必ず告知する)。
    zip.start_file("KNOWN_GAPS.txt", opts).unwrap();
    zip.write_all(
        "本家xorrisoはこのインストーラーに同梱されていません。\
         ISO書き込み機能を使うにはxorrisoを別途インストールしPATHへ通してください。\n\
         Genuine xorriso is NOT bundled with this installer. To use ISO burning, \
         install xorriso separately and add it to PATH.\n"
            .as_bytes(),
    )
    .unwrap();

    zip.finish().expect("finish payload.zip");

    // インストーラー自身のCargo.tomlバージョン(0.1.0固定)ではなく、埋め込んだmake-disk本体の
    // バージョン(../package.json)をレジストリ表示等に使う。ここが食い違うとユーザーが
    // 「インストーラーのバージョンとmake-disk本体のバージョンが合っていない」と混乱するため
    // (2026-09-26 ユーザー指摘、以後は手動同期せず自動で追従させる)。
    let package_json = fs::read_to_string(repo_root.join("package.json")).expect("read ../package.json for the bundled make-disk version");
    let app_version = extract_json_string_field(&package_json, "version").expect("find \"version\" in ../package.json");
    println!("cargo:rustc-env=MAKE_DISK_APP_VERSION={app_version}");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../package.json");
    println!("cargo:rustc-env=MAKE_DISK_INSTALLER_PAYLOAD={}", payload_path.display());
}

/// `serde_json`を持ち込むまでもない単純な`"version": "0.1.31"`抽出(build.rsはbuild-dependenciesを
/// 増やしたくないので手書き。package.jsonの整形が変わったら分かるようメッセージ付きでpanicする)。
fn extract_json_string_field(json: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let after_key = json.split(&needle).nth(1)?;
    let after_colon = after_key.split_once(':')?.1;
    let start = after_colon.find('"')? + 1;
    let rest = &after_colon[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn add_file<W: std::io::Write + std::io::Seek>(zip: &mut zip::ZipWriter<W>, opts: &zip::write::FileOptions, name: &str, src: &Path, required: bool) {
    match fs::read(src) {
        Ok(bytes) => {
            zip.start_file(name, *opts).unwrap();
            zip.write_all(&bytes).unwrap();
        }
        Err(e) => {
            let msg = format!("make-disk-installer: payload source '{}' not found ({e}) — run `npm run tauri build` and scripts/fetch-ffmpeg-sidecars.sh / scripts/build-rs-tribute-sidecars.sh in the repo root first", src.display());
            if required {
                panic!("{msg}");
            } else {
                println!("cargo:warning={msg} (optional, skipping)");
            }
        }
    }
}
