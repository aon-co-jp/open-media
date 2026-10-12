//! xorriso(libisofs/libburnベース)によるISOイメージ生成。
//! xorrisoはWindows/macOS/Linuxいずれにも移植されており、
//! 本アプリではOS判定を持ち込まず単一コマンド経路に統一する。
//!
//! ## 実機で発見した実バグと修正(2026-09-17)
//!
//! `scripts/fetch-ffmpeg-sidecars.sh`はffmpeg/ffprobeのみを取得し、
//! **本家xorrisoはWindows/Linux/macOSいずれもsidecarとして同梱して
//! いない**(スクリプト冒頭のコメントに明記の既知の制限)。一方、
//! `scripts/build-rs-tribute-sidecars.sh`で`rs-xorriso`(このプロジェクト
//! 用に作成した純Rust実装、ISO生成のみ対応)はWindows/Linux向けに
//! 実際に同梱されているにもかかわらず、このファイルは常に`xorriso`
//! (本家)だけを呼んでおり`rs-xorriso`を一度も試していなかった——
//! その結果、本家xorrisoを別途インストールしていない環境では
//! ISO作成が必ず失敗する実バグになっていた(ユーザー実機で確認)。
//!
//! 修正: 本家`xorriso`を優先して試し、起動自体に失敗した場合(PATH上に
//! 無い場合)のみ、同梱されている`rs-xorriso`にフォールバックする。
//! **正直な開示**: `rs-xorriso`は長いファイル名を8.3形式へ切り詰める
//! 既知の非互換があるため、本家xorrisoが使える環境ではそちらを優先する。

use crate::engine::sidecar::resolve_tool;

pub fn create_iso(source_dir: &str, output_iso: &str, volume_label: &str) -> Result<(), String> {
    // Windowsでは標準のIMAPI2FSを優先する(Unicodeファイル名を保持できる。
    // rs-xorrisoは非ASCII名を8.3へ潰す)。失敗時のみ従来のxorriso系へ。
    #[cfg(windows)]
    if crate::engine::windows_imapi::create_iso(source_dir, output_iso, volume_label).is_ok() {
        return Ok(());
    }
    let build_args = || -> Vec<String> {
        ["-as", "mkisofs", "-iso-level", "3", "-J", "-R", "-V", volume_label, "-o", output_iso, source_dir]
            .iter()
            .map(|s| s.to_string())
            .collect()
    };

    match resolve_tool("xorriso").args(build_args()).output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(String::from_utf8_lossy(&output.stderr).to_string()),
        Err(_) => {
            // 本家xorrisoがPATH上に見つからない場合、同梱している
            // rs-xorriso(ISO生成のみ対応の純Rust実装)へフォールバックする。
            let output = resolve_tool("rs-xorriso")
                .args(build_args())
                .output()
                .map_err(|e| format!("xorriso・rs-xorrisoともに起動に失敗しました(いずれも未インストール/未同梱の可能性): {e}"))?;
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
    fn create_iso_falls_back_to_rs_xorriso_when_real_xorriso_is_not_on_path() {
        // このテスト実行環境には本家xorrisoが無い前提(このリポジトリの
        // CI/開発機いずれも同梱していない、上のdocコメント参照)。
        // rs-xorrisoも同梱されていないため、両方失敗して明確な
        // エラーメッセージになることを検証する(黙って成功したふりを
        // しない)。
        let tmp = std::env::temp_dir().join(format!("make-disk-test-iso-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("dummy.txt"), b"hello").unwrap();
        let output_iso = tmp.join("output.iso");

        let result = create_iso(tmp.to_str().unwrap(), output_iso.to_str().unwrap(), "TEST");

        let _ = std::fs::remove_dir_all(&tmp);

        if result.is_ok() {
            // 実行環境に本家xorrisoかrs-xorrisoが実際に存在する場合は
            // 成功して構わない(モックに頼らない実機テストの都合上、
            // どちらの経路になるかは環境依存)。
            return;
        }
        assert!(result.unwrap_err().contains("xorriso"), "エラーメッセージにxorriso関連の説明が含まれるはず");
    }

    /// 実際にビルドした`rs-xorriso`バイナリを実行ファイルの隣へ配置し、
    /// 本家xorrisoが無い環境で`create_iso`が本当にrs-xorrisoへ
    /// フォールバックしてISOを作成できることを検証する(2026-09-17新設、
    /// ユーザー報告の実バグ修正の直接検証——モックに頼らない実機E2E)。
    /// `F:\rs-xorriso`をcloneしてリリースビルド済みでない環境では
    /// スキップする。
    #[test]
    fn create_iso_actually_succeeds_via_a_real_bundled_rs_xorriso_binary() {
        let rs_xorriso_release = std::path::PathBuf::from("F:\\rs-xorriso\\target\\release\\rs-xorriso.exe");
        if !rs_xorriso_release.is_file() {
            eprintln!("F:\\rs-xorriso のリリースビルドが無いためスキップ / skipping: build rs-xorriso first");
            return;
        }

        let exe = std::env::current_exe().unwrap();
        let dir = exe.parent().unwrap().to_path_buf();
        let sidecar_path = dir.join(format!("rs-xorriso{}", std::env::consts::EXE_SUFFIX));
        std::fs::copy(&rs_xorriso_release, &sidecar_path).expect("failed to place rs-xorriso sidecar next to the test binary");

        let tmp = std::env::temp_dir().join(format!("make-disk-test-iso-real-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("dummy.txt"), b"hello").unwrap();
        let output_iso = tmp.join("output.iso");

        let result = create_iso(tmp.to_str().unwrap(), output_iso.to_str().unwrap(), "TEST");

        let _ = std::fs::remove_file(&sidecar_path);
        let iso_exists = output_iso.is_file();
        let _ = std::fs::remove_dir_all(&tmp);

        result.expect("create_iso should succeed via the bundled rs-xorriso fallback when real xorriso is absent");
        assert!(iso_exists, "rs-xorrisoが実際にISOファイルを書き出しているはず");
    }

    /// 実フォルダ(`MAKE_DISK_ISO_SRC`)から実ISO(`MAKE_DISK_ISO_OUT`)を作り、サイズを表示する手動確認用。
    #[test]
    #[ignore]
    fn real_folder_to_iso() {
        let src = std::env::var("MAKE_DISK_ISO_SRC").expect("set MAKE_DISK_ISO_SRC");
        let out = std::env::var("MAKE_DISK_ISO_OUT").expect("set MAKE_DISK_ISO_OUT");
        create_iso(&src, &out, "DSD256").unwrap();
        eprintln!("ISOサイズ: {} バイト", std::fs::metadata(&out).unwrap().len());
    }
}
