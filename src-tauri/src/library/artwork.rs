//! アルバムアート：埋め込み画像の縮小版を作って保管する。外部画像・背景はアプリのデータフォルダへ複製する。
//! 画像は DB に入れない（アーキテクチャ案 5章）。

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use image::ImageReader;

use crate::error::{AppError, AppResult, ErrorKind};

/// 縮小版の一辺（px）。曲一覧・再生バー・SMTC で使う
pub const THUMB_SIZE: u32 = 512;

/// FNV-1a 64bit（同じ画像を1つにまとめるための鍵。暗号用途ではない）
pub fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// 埋め込み画像のバイト列から縮小版を作り、鍵を返す（既にあれば作らない）
pub fn store_thumbnail(thumbs_dir: &Path, data: &[u8]) -> Option<String> {
    let key = format!("{:016x}", fnv1a64(data));
    let out = thumbs_dir.join(format!("{key}.jpg"));
    if out.exists() {
        return Some(key);
    }
    // v3.1.1：復号の割り当て上限を下げる（ペネトレーションテスト v3.0.2 の9）
    let (_, img) = decode_limited(data).ok()?;
    let thumb = if img.width() > THUMB_SIZE || img.height() > THUMB_SIZE {
        img.resize(THUMB_SIZE, THUMB_SIZE, FilterType::Triangle)
    } else {
        img
    };
    let rgb = thumb.to_rgb8();
    let tmp = thumbs_dir.join(format!("{key}.jpg.tmp"));
    rgb.save_with_format(&tmp, image::ImageFormat::Jpeg).ok()?;
    fs::rename(&tmp, &out).ok()?;
    Some(key)
}

pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];

pub fn is_image_path(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// 画像として受け取るファイルの大きさの上限（v3.1.1。ペネトレーションテスト v3.0.2 の9）。
/// アルバムアート・背景・設定 zip の背景・曲に埋め込む表紙のどれにも当てる
pub const MAX_IMAGE_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// 画像の復号で使ってよいメモリの上限（image の既定 512MiB から 128MiB へ下げた。v3.1.1）
pub const MAX_DECODE_ALLOC: u64 = 128 * 1024 * 1024;

fn decode_limits() -> image::Limits {
    let mut l = image::Limits::default();
    l.max_alloc = Some(MAX_DECODE_ALLOC);
    l
}

fn unsupported(what: &str, e: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorKind::UnsupportedFormat, format!("{what}: {e}"))
}

/// 中身から形式を決め、上限つきで最後まで復号する
fn decode_limited(data: &[u8]) -> image::ImageResult<(image::ImageFormat, image::DynamicImage)> {
    let fmt = image::guess_format(data)?;
    let mut r = ImageReader::with_format(std::io::Cursor::new(data), fmt);
    r.limits(decode_limits());
    Ok((fmt, r.decode()?))
}

/// 画像ファイルを上限つきで読む。大きさを確かめてから読むのではなく、上限＋1 バイトまでしか読まない
/// （確かめたあとにファイルが伸びても、上限より先は読まない）
pub fn read_image_file(src: &Path) -> AppResult<Vec<u8>> {
    let f = fs::File::open(src).map_err(|e| AppError::from_io(&e, src))?;
    read_image_from(f, &src.display().to_string())
}

fn read_image_from(r: impl Read, what: &str) -> AppResult<Vec<u8>> {
    let mut data = Vec::new();
    r.take(MAX_IMAGE_FILE_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|e| AppError::new(ErrorKind::Other, format!("{what}: {e}")))?;
    if data.len() as u64 > MAX_IMAGE_FILE_BYTES {
        return Err(unsupported(what, format!("image too large (over {MAX_IMAGE_FILE_BYTES} bytes)")));
    }
    Ok(data)
}

/// 画像として最後まで復号できるか確かめ、形式を返す（先頭だけ本物の画像は、ここで断る。v3.1.1 の8）
pub fn check_image(data: &[u8], what: &str) -> AppResult<image::ImageFormat> {
    if data.len() as u64 > MAX_IMAGE_FILE_BYTES {
        return Err(unsupported(what, "image too large"));
    }
    decode_limited(data).map(|(fmt, _)| fmt).map_err(|e| unsupported(what, e))
}

/// 確かめた画像を、データフォルダへ自前の名前（`{prefix}-{内容の鍵}.{形式の拡張子}`）で書く。
/// 元の名前は使わない（設定 zip の `evil.exe` なども、画像なら `bg-….png` になる。v3.1.1 の6）
pub fn store_image_bytes(data: &[u8], dest_dir: &Path, prefix: &str, what: &str) -> AppResult<PathBuf> {
    let fmt = check_image(data, what)?;
    let ext = fmt.extensions_str().first().copied().unwrap_or("img");
    fs::create_dir_all(dest_dir).map_err(|e| AppError::from_io(&e, dest_dir))?;
    let name = format!("{prefix}-{:016x}.{ext}", fnv1a64(data));
    let dest = dest_dir.join(name);
    if !dest.exists() {
        fs::write(&dest, data).map_err(|e| AppError::from_io(&e, &dest))?;
    }
    Ok(dest)
}

/// ユーザーが選んだ画像を確かめて、データフォルダへ複製する（設定の書き出しで持ち運べるように）。
/// 戻り値は複製先のパス。50MB を超えるもの・最後まで復号できないものは断る（v3.1.1）
pub fn import_image(src: &Path, dest_dir: &Path, prefix: &str) -> AppResult<PathBuf> {
    let data = read_image_file(src)?;
    store_image_bytes(&data, dest_dir, prefix, &src.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_is_cached_by_content() {
        let dir = crate::test_tmp().join(format!("plp-thumb-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let img = image::RgbImage::from_pixel(1000, 800, image::Rgb([200, 30, 30]));
        let mut png = vec![];
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let k1 = store_thumbnail(&dir, &png).unwrap();
        let k2 = store_thumbnail(&dir, &png).unwrap();
        assert_eq!(k1, k2);
        let t = image::open(dir.join(format!("{k1}.jpg"))).unwrap();
        assert!(t.width() <= THUMB_SIZE && t.height() <= THUMB_SIZE);
        let _ = fs::remove_dir_all(&dir);
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の9）：50MB を超える画像は、上限＋1 バイトまでしか読まずに断る
    /// （終わりの無い入力でも止まる）。復号の割り当ての上限は 128MiB
    #[test]
    fn image_size_and_decode_limits() {
        let e = read_image_from(std::io::repeat(0), "endless").unwrap_err();
        assert_eq!(e.kind, ErrorKind::UnsupportedFormat);
        assert_eq!(decode_limits().max_alloc, Some(128 * 1024 * 1024));
        // 頭だけ PNG のものは画像として扱わない
        let mut fake = b"\x89PNG\r\n\x1a\n".to_vec();
        fake.extend_from_slice(&[0; 64]);
        assert!(check_image(&fake, "fake").is_err());
        // 縦横の数だけ大きい PNG（10万×10万。復号すると 30GB）は、上限で断る
        use image::ImageEncoder;
        let mut huge = Vec::new();
        image::codecs::png::PngEncoder::new(&mut huge)
            .write_image(&[0u8; 3], 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
        // IHDR の幅と高さを書き換え、CRC を付け直す
        huge[16..20].copy_from_slice(&100_000u32.to_be_bytes());
        huge[20..24].copy_from_slice(&100_000u32.to_be_bytes());
        let crc = crate::settings::test_crc32(&huge[12..29]);
        huge[29..33].copy_from_slice(&crc.to_be_bytes());
        assert!(check_image(&huge, "huge").is_err());
        // ふつうの画像は通る
        let dir = crate::test_tmp().join(format!("plp-img-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut png = vec![];
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3])))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let src = dir.join("ok.png");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&src, &png).unwrap();
        let out = import_image(&src, &dir.join("out"), "bg").unwrap();
        assert!(out.file_name().unwrap().to_string_lossy().starts_with("bg-"));
        let _ = fs::remove_dir_all(&dir);
    }
}
