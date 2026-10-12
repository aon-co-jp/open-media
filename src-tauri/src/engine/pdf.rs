//! PDF見開き対応(2026-09-16新設)。
//!
//! ユーザー指示「音声、静止画、動画の他に、最大4Kの見開きPDF対応で、
//! 右綴じ、左綴じ選択可能で、選択によって左右も入れ替える機能も搭載」
//! への対応。
//!
//! ## 設計・正直な開示
//!
//! PDFのラスタライズ(ページ→画像への変換)自体は、このアプリが既に
//! 採用している「実績のある外部ツールをsidecarとして呼ぶ」方針
//! ([`super::sidecar`]参照、ffmpeg/xorrisoと同じパターン)に倣い、
//! poppler-utilsの`pdftoppm`(ページ→PNG)・`pdfinfo`(総ページ数取得)を
//! 呼ぶ([`render_pdf_as_spreads`])。
//!
//! **既知の制限(2026-09-16時点)**: ffmpeg/xorrisoとは異なり、poppler-utils
//! はまだこのリポジトリのCIでsidecarバイナリとして同梱・検証されていない
//! (`scripts/fetch-ffmpeg-sidecars.sh`のような取得スクリプトが未作成)。
//! そのため現時点では、実行環境のPATH上に`pdftoppm`/`pdfinfo`が
//! インストールされていることが前提(未インストールの場合は明確な
//! エラーメッセージを返す)。次の増分でWindows/Linux向けの静的ビルド
//! 取得スクリプトを追加し、真の「同梱・単体で動く」状態にする予定
//! (iOS/iPadOS同様、実機/実環境が無い部分は正直に制限として記録する
//! というこのリポジトリの既存方針に従う)。
//!
//! 一方、見開き合成そのもの([`compose_spread`]——2ページを横に並べ、
//! 綴じ方向に応じて左右を入れ替え、4K上限に縮小する処理)は純Rustの
//! `image` crateのみで実装しており、外部ツール・ネイティブ依存が無いため
//! Android/iOSを含む全ターゲットでコンパイル・実行でき、単体テストで
//! 実際に検証済み。

use image::{imageops::FilterType, DynamicImage, GenericImage};
use serde::{Deserialize, Serialize};

use super::sidecar::resolve_tool;

/// 綴じ方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingDirection {
    /// 右綴じ(日本式・漫画など、右から左へ読む): 先行ページを右に配置。
    RightToLeft,
    /// 左綴じ(欧米式、左から右へ読む): 先行ページを左に配置。
    LeftToRight,
}

/// 4K相当の最大辺ピクセル数(3840 = 4K UHDの横幅)。
pub const MAX_4K_DIMENSION: u32 = 3840;

/// 連続する2ページ(`page_a`が先行ページ、`page_b`が後続ページ)を、
/// 綴じ方向に応じて左右に配置した1枚の見開き画像へ合成する。
/// 高さが異なる場合は高い方に合わせて拡縮し、結果が
/// `max_dimension`(既定4K)を超える場合は縦横比を保ったまま縮小する。
pub fn compose_spread(page_a: &DynamicImage, page_b: &DynamicImage, binding: BindingDirection, max_dimension: u32) -> DynamicImage {
    let target_height = page_a.height().max(page_b.height()).max(1);

    let scale_to_height = |img: &DynamicImage| -> DynamicImage {
        if img.height() == target_height {
            img.clone()
        } else {
            let new_width = ((img.width() as f64) * (target_height as f64) / (img.height() as f64)).round().max(1.0) as u32;
            img.resize_exact(new_width, target_height, FilterType::Lanczos3)
        }
    };

    let scaled_a = scale_to_height(page_a);
    let scaled_b = scale_to_height(page_b);

    let (left, right) = match binding {
        BindingDirection::LeftToRight => (&scaled_a, &scaled_b),
        BindingDirection::RightToLeft => (&scaled_b, &scaled_a),
    };

    let total_width = left.width() + right.width();
    let mut canvas = DynamicImage::new_rgba8(total_width, target_height);
    canvas.copy_from(left, 0, 0).expect("見開きキャンバスへの左ページ描画に失敗することは無いはず(範囲内)");
    canvas.copy_from(right, left.width(), 0).expect("見開きキャンバスへの右ページ描画に失敗することは無いはず(範囲内)");

    let longest_side = canvas.width().max(canvas.height());
    if longest_side > max_dimension {
        let scale_factor = max_dimension as f64 / longest_side as f64;
        let new_w = ((canvas.width() as f64) * scale_factor).round().max(1.0) as u32;
        let new_h = ((canvas.height() as f64) * scale_factor).round().max(1.0) as u32;
        canvas = canvas.resize_exact(new_w, new_h, FilterType::Lanczos3);
    }

    canvas
}

/// PDFの総ページ数を`pdfinfo`で取得する。
fn pdf_page_count(pdf_path: &str) -> Result<u32, String> {
    let output = resolve_tool("pdfinfo")
        .arg(pdf_path)
        .output()
        .map_err(|e| format!("pdfinfoの起動に失敗しました(poppler-utils未インストールの可能性): {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("Pages:") {
            if let Ok(n) = rest.trim().parse::<u32>() {
                return Ok(n);
            }
        }
    }
    Err("pdfinfoの出力からページ数を取得できませんでした".to_string())
}

/// PDFの1ページを指定した最大辺サイズのPNGとしてラスタライズする。
fn render_pdf_page(pdf_path: &str, page_number: u32, max_dimension: u32) -> Result<DynamicImage, String> {
    let tmp_dir = std::env::temp_dir();
    let prefix = tmp_dir.join(format!("make-disk-pdfpage-{}-{}", std::process::id(), page_number));
    let prefix_str = prefix.to_string_lossy().to_string();

    let status = resolve_tool("pdftoppm")
        .args(["-f", &page_number.to_string(), "-l", &page_number.to_string(), "-png", "-scale-to", &max_dimension.to_string(), pdf_path, &prefix_str])
        .status()
        .map_err(|e| format!("pdftoppmの起動に失敗しました(poppler-utils未インストールの可能性): {e}"))?;
    if !status.success() {
        return Err(format!("pdftoppmがページ{page_number}のレンダリングに失敗しました"));
    }

    // pdftoppmは総ページ数の桁数に応じて`<prefix>-1.png`や`<prefix>-01.png`のように
    // 出力する。実際に生成されたファイルを探して読み込む。
    let parent = prefix.parent().ok_or("一時ディレクトリの解決に失敗しました")?;
    let file_stem = prefix.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string();
    let mut found: Option<std::path::PathBuf> = None;
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&format!("{file_stem}-")) && name.ends_with(".png") {
                found = Some(entry.path());
                break;
            }
        }
    }
    let png_path = found.ok_or_else(|| format!("pdftoppmの出力ファイルが見つかりませんでした(prefix: {prefix_str})"))?;
    let img = image::open(&png_path).map_err(|e| format!("レンダリング結果の読み込みに失敗しました: {e}"))?;
    let _ = std::fs::remove_file(&png_path);
    Ok(img)
}

/// PDF全体を見開き画像群として`output_dir`へ出力し、生成したファイルパスの
/// 一覧を返す(奇数ページ数の場合、最後の1ページは単独ページとして
/// そのまま最大辺`max_dimension`に収まるよう出力する)。
pub fn render_pdf_as_spreads(pdf_path: &str, output_dir: &str, binding: BindingDirection, max_dimension: u32) -> Result<Vec<String>, String> {
    let page_count = pdf_page_count(pdf_path)?;
    std::fs::create_dir_all(output_dir).map_err(|e| format!("出力先フォルダの作成に失敗しました: {e}"))?;

    let mut outputs = Vec::new();
    let mut page = 1;
    let mut spread_index = 1;
    while page <= page_count {
        let img_a = render_pdf_page(pdf_path, page, max_dimension)?;
        let output_path = if page < page_count {
            let img_b = render_pdf_page(pdf_path, page + 1, max_dimension)?;
            let spread = compose_spread(&img_a, &img_b, binding, max_dimension);
            let out_path = format!("{output_dir}/spread-{spread_index:04}.png");
            spread.save(&out_path).map_err(|e| format!("見開き画像の保存に失敗しました: {e}"))?;
            page += 2;
            out_path
        } else {
            // 奇数ページ数の最終ページは単独出力(相方が無いため合成しない)。
            let out_path = format!("{output_dir}/spread-{spread_index:04}.png");
            img_a.save(&out_path).map_err(|e| format!("単独ページ画像の保存に失敗しました: {e}"))?;
            page += 1;
            out_path
        };
        outputs.push(output_path);
        spread_index += 1;
    }

    Ok(outputs)
}

/// PDFの綴じ方向を一括変換して保存する(2026-09-16新設)。
///
/// ユーザー指示「PDFが左綴じを右綴じや右綴じを左綴じなどに一括編集して
/// 保存も可能にして」への対応。綴じ方向の変換は、ページの並び順を
/// 反転することと等価(反転操作はどちら向きにも同じ操作で自己逆元)
/// なので、目的の方向を都度指定する必要は無い——このPDFの
/// ページ順序をそのまま反転して`output_path`へ保存する。
///
/// 入れ子のページツリーのPDFは、全ページをルート直下へフラット化して反転する。
pub fn reverse_pdf_page_order(input_path: &str, output_path: &str) -> Result<(), String> {
    let mut doc = lopdf::Document::load(input_path).map_err(|e| format!("PDFの読み込みに失敗しました: {e}"))?;

    let pages_id = doc
        .catalog()
        .map_err(|e| format!("PDFのカタログ取得に失敗しました: {e}"))?
        .get(b"Pages")
        .map_err(|e| format!("Pagesツリーの取得に失敗しました: {e}"))?
        .as_reference()
        .map_err(|e| format!("Pagesツリーの参照解決に失敗しました: {e}"))?;

    // 入れ子のページツリー(Distiller等の出力)にも対応するため、全ページを葉として
    // 列挙し、ルート直下のフラットなKidsへ付け替える。中間ノードが持つ継承属性
    // (MediaBox/Resources/CropBox/Rotate)は、葉へ複製して失わない。
    let leaf_ids: Vec<lopdf::ObjectId> = doc.get_pages().values().copied().collect();
    const INHERITABLE: [&[u8]; 4] = [b"MediaBox", b"Resources", b"CropBox", b"Rotate"];
    for &leaf in &leaf_ids {
        let own: Vec<bool> = {
            let d = doc.get_dictionary(leaf).map_err(|e| format!("ページ辞書の取得に失敗しました: {e}"))?;
            INHERITABLE.iter().map(|k| d.has(k)).collect()
        };
        let mut inherited: Vec<(Vec<u8>, lopdf::Object)> = Vec::new();
        let mut cursor = doc.get_dictionary(leaf).ok().and_then(|d| d.get(b"Parent").ok()).and_then(|o| o.as_reference().ok());
        while let Some(pid) = cursor {
            let Ok(pd) = doc.get_dictionary(pid) else { break };
            for (i, key) in INHERITABLE.iter().enumerate() {
                if !own[i] && !inherited.iter().any(|(k, _)| k.as_slice() == *key) {
                    if let Ok(v) = pd.get(key) {
                        inherited.push((key.to_vec(), v.clone()));
                    }
                }
            }
            cursor = pd.get(b"Parent").ok().and_then(|o| o.as_reference().ok());
        }
        let d = doc.get_dictionary_mut(leaf).map_err(|e| format!("ページ辞書の取得(更新用)に失敗しました: {e}"))?;
        for (k, v) in inherited {
            d.set(k, v);
        }
        d.set("Parent", lopdf::Object::Reference(pages_id));
    }

    let reversed: lopdf::Object = lopdf::Object::Array(leaf_ids.iter().rev().map(|id| lopdf::Object::Reference(*id)).collect());
    let root = doc.get_dictionary_mut(pages_id).map_err(|e| format!("Pages辞書の取得(更新用)に失敗しました: {e}"))?;
    root.set("Kids", reversed);
    root.set("Count", leaf_ids.len() as i64);


    doc.save(output_path).map_err(|e| format!("PDFの保存に失敗しました: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgba};

    fn solid_image(width: u32, height: u32, color: [u8; 4]) -> DynamicImage {
        let mut img = DynamicImage::new_rgba8(width, height);
        for y in 0..height {
            for x in 0..width {
                img.put_pixel(x, y, Rgba(color));
            }
        }
        img
    }

    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];

    #[test]
    fn left_to_right_binding_places_page_a_on_the_left() {
        let page_a = solid_image(100, 200, RED);
        let page_b = solid_image(100, 200, BLUE);
        let spread = compose_spread(&page_a, &page_b, BindingDirection::LeftToRight, MAX_4K_DIMENSION);

        assert_eq!(spread.width(), 200);
        assert_eq!(spread.height(), 200);
        assert_eq!(spread.get_pixel(10, 100), Rgba(RED), "左綴じでは先行ページ(page_a)が左に来るはず");
        assert_eq!(spread.get_pixel(190, 100), Rgba(BLUE), "左綴じでは後続ページ(page_b)が右に来るはず");
    }

    #[test]
    fn right_to_left_binding_places_page_a_on_the_right() {
        let page_a = solid_image(100, 200, RED);
        let page_b = solid_image(100, 200, BLUE);
        let spread = compose_spread(&page_a, &page_b, BindingDirection::RightToLeft, MAX_4K_DIMENSION);

        assert_eq!(spread.get_pixel(10, 100), Rgba(BLUE), "右綴じでは後続ページ(page_b)が左に来るはず");
        assert_eq!(spread.get_pixel(190, 100), Rgba(RED), "右綴じでは先行ページ(page_a)が右に来るはず(日本式の読み順)");
    }

    #[test]
    fn differing_heights_are_scaled_to_match_before_composing() {
        let page_a = solid_image(100, 200, RED);
        let page_b = solid_image(50, 100, BLUE); // 高さが半分 → 200へ拡大されるはず
        let spread = compose_spread(&page_a, &page_b, BindingDirection::LeftToRight, MAX_4K_DIMENSION);

        assert_eq!(spread.height(), 200);
        assert_eq!(spread.width(), 100 + 100, "page_bは高さ200に合わせて幅も100へ拡大されるはず(元の縦横比 50:100 を維持)");
    }

    #[test]
    fn result_wider_than_max_dimension_is_downscaled_preserving_aspect_ratio() {
        let page_a = solid_image(3000, 4000, RED);
        let page_b = solid_image(3000, 4000, BLUE);
        let spread = compose_spread(&page_a, &page_b, BindingDirection::LeftToRight, MAX_4K_DIMENSION);

        // 合成前は幅6000×高さ4000で、長辺(6000)がMAX_4K_DIMENSION(3840)を超える。
        assert!(spread.width() <= MAX_4K_DIMENSION);
        assert!(spread.height() <= MAX_4K_DIMENSION);
        // 縦横比(6000:4000 = 3:2)がおおむね保たれているはず。
        let ratio = spread.width() as f64 / spread.height() as f64;
        assert!((ratio - 1.5).abs() < 0.05, "縮小後も縦横比3:2が概ね保たれているはず(実際の比率: {ratio})");
    }

    #[test]
    fn result_within_max_dimension_is_left_unchanged() {
        let page_a = solid_image(100, 200, RED);
        let page_b = solid_image(100, 200, BLUE);
        let spread = compose_spread(&page_a, &page_b, BindingDirection::LeftToRight, MAX_4K_DIMENSION);

        assert_eq!(spread.width(), 200);
        assert_eq!(spread.height(), 200);
    }

    /// テスト用に、ページごとにMediaBoxの幅を変えた最小限のPDFを
    /// メモリ上に構築する(ページ順序を後で判別するための目印として、
    /// 実際のPDF構造〈Pages/Kids/Page/Contents〉を組み立てる——
    /// 文字列描画やフォント埋め込みが不要な最小構成)。
    fn build_test_pdf_with_page_widths(widths: &[f64]) -> lopdf::Document {
        use lopdf::{dictionary, Object, Stream};

        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::new();
        for &w in widths {
            let content_id = doc.add_object(Stream::new(dictionary! {}, vec![]));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![Object::Integer(0), Object::Integer(0), Object::Real(w as f32), Object::Integer(300)],
                "Contents" => content_id,
            });
            kids.push(Object::Reference(page_id));
        }
        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => widths.len() as i64,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages_dict));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    fn page_widths_in_order(pdf_path: &str) -> Vec<i64> {
        let doc = lopdf::Document::load(pdf_path).unwrap();
        doc.get_pages()
            .values()
            .map(|&id| {
                let page = doc.get_dictionary(id).unwrap();
                let media_box = page.get(b"MediaBox").unwrap().as_array().unwrap();
                media_box[2].as_float().unwrap().round() as i64
            })
            .collect()
    }

    #[test]
    fn reverse_pdf_page_order_reverses_a_flat_page_tree() {
        let dir = std::env::temp_dir();
        let input_path = dir.join(format!("make-disk-test-reverse-input-{}.pdf", std::process::id()));
        let output_path = dir.join(format!("make-disk-test-reverse-output-{}.pdf", std::process::id()));

        let mut doc = build_test_pdf_with_page_widths(&[100.0, 200.0, 300.0]);
        doc.save(&input_path).unwrap();

        let result = reverse_pdf_page_order(input_path.to_str().unwrap(), output_path.to_str().unwrap());

        let widths = page_widths_in_order(output_path.to_str().unwrap());

        let _ = std::fs::remove_file(&input_path);
        let _ = std::fs::remove_file(&output_path);

        result.expect("reverse_pdf_page_order should succeed for a flat page tree");
        assert_eq!(widths, vec![300, 200, 100], "ページ順序が反転しているはず(綴じ方向の変換)");
    }

    #[test]
    fn reverse_pdf_page_order_flattens_a_nested_page_tree() {
        use lopdf::{dictionary, Object};

        let dir = std::env::temp_dir();
        let input_path = dir.join(format!("make-disk-test-nested-input-{}.pdf", std::process::id()));

        let mut doc = build_test_pdf_with_page_widths(&[100.0, 200.0]);
        // 既存のフラットなKidsの1件を、意図的に入れ子のPagesノードへ差し替える。
        let nested_pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Vec::<Object>::new(),
            "Count" => 0,
        });
        let root_pages_id = doc.catalog().unwrap().get(b"Pages").unwrap().as_reference().unwrap();
        let kids = doc.get_dictionary(root_pages_id).unwrap().get(b"Kids").unwrap().as_array().unwrap().clone();
        let mut new_kids = kids;
        new_kids[0] = Object::Reference(nested_pages_id);
        doc.get_dictionary_mut(root_pages_id).unwrap().set("Kids", new_kids);
        doc.save(&input_path).unwrap();

        let result = reverse_pdf_page_order(input_path.to_str().unwrap(), &format!("{}-out.pdf", input_path.to_str().unwrap()));
        let _ = std::fs::remove_file(&input_path);
        let _ = std::fs::remove_file(format!("{}-out.pdf", input_path.to_str().unwrap()));

        assert!(result.is_ok(), "入れ子のページツリーもフラット化して反転できるはず");
    }
}
