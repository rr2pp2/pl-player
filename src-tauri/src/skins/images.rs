//! スキンの画像の確かめ（v3.2.0）。PNG（APNG を含む）・WebP（動くものを含む）・GIF だけ。
//!
//! - 中身の頭が `MZ`（Windows の実行ファイル）なら、拡張子にかかわらず断る
//! - 拡張子と中身の形式が同じでなければ断る（`.png` という名前の GIF なども）
//! - 縦横は、復号の前に見出しで確かめる（65535×65535 を名乗る PNG に大きなメモリを取らせない）
//! - 最後まで復号する。動く画像は全部のコマを1つずつ復号し、コマ数・画素の合計・長さを数える（全部のコマを同時には持たない）
//! - 動く画像は、最初の1コマを PNG にして返す（「動きを減らす」のときに画面が読む静止画）

use std::io::Cursor;

use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat};
use serde::{Deserialize, Serialize};

use super::manifest::skin_error;
use super::names::shown_name;
use crate::error::{AppError, AppResult, ErrorKind};

/// 1つの画像ファイルの大きさの上限
pub const MAX_IMAGE_BYTES: u64 = 15 * 1024 * 1024;
/// 縦・横の上限（px）
pub const MAX_SIDE: u32 = 4096;
/// 1枚（動く画像は1コマ）の画素の上限（4096×4096）
pub const MAX_FRAME_PIXELS: u64 = 4096 * 4096;
/// 動く画像のコマ数の上限
pub const MAX_FRAMES: u32 = 120;
/// 動く画像の、全部のコマを合わせた画素の上限。推奨の立ち絵（720×1280）で 120 コマ（約 1 億 1,059 万）が収まるように
/// 1億 2,800 万（128M）にした（docs\着せ替え_仕様.md 10章。6,400 万では約 69 コマで当たった）
pub const MAX_ANIMATION_PIXELS: u64 = 128_000_000;
/// 動く画像の長さの上限（ミリ秒）
pub const MAX_DURATION_MS: u64 = 90_000;
/// 1コマの表示時間がこれ以下のものは、ブラウザと同じく 100ms として数える（0 や極端に速いコマ）
pub const FAST_FRAME_MS: u64 = 10;
/// 復号で使ってよいメモリ（1枚ぶん。アルバムアートと同じ 128MiB）
const MAX_DECODE_ALLOC: u64 = 128 * 1024 * 1024;

/// 確かめた画像の情報（取り込みのとき derived\info.json に書き、起動のときに読む）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub duration_ms: u64,
    pub animated: bool,
    /// 確かめるときに復号した画素の数（動く画像は全部のコマの合計）
    pub decoded_pixels: u64,
}

pub struct Checked {
    pub info: ImageInfo,
    /// 動く画像の最初の1コマ（PNG）。静止画は None
    pub first_frame_png: Option<Vec<u8>>,
}

/// `why` は画像の部品（image）の文のこともある。名前も文も `skin_error` に通す
fn invalid(name: &str, why: impl std::fmt::Display) -> AppError {
    skin_error(ErrorKind::SkinImageInvalid, format!("{}: {why}", shown_name(name)))
}

fn limit(name: &str, why: impl std::fmt::Display) -> AppError {
    skin_error(ErrorKind::SkinImageLimit, format!("{}: {why}", shown_name(name)))
}

fn limits() -> image::Limits {
    let mut l = image::Limits::default();
    l.max_image_width = Some(MAX_SIDE);
    l.max_image_height = Some(MAX_SIDE);
    l.max_alloc = Some(MAX_DECODE_ALLOC);
    l
}

/// 拡張子から、期待する形式
pub fn expected_format(name: &str) -> Option<ImageFormat> {
    match name.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "webp" => Some(ImageFormat::WebP),
        "gif" => Some(ImageFormat::Gif),
        _ => None,
    }
}

/// 頭のバイトが実行ファイル（MZ）か
pub fn looks_executable(data: &[u8]) -> bool {
    data.starts_with(b"MZ")
}

/// 頭のバイトが、拡張子の形式と合うか（起動のときの軽い確かめ。復号はしない）
pub fn signature_matches(name: &str, head: &[u8]) -> bool {
    !looks_executable(head) && expected_format(name).is_some() && image::guess_format(head).ok() == expected_format(name)
}

fn check_dims(name: &str, w: u32, h: u32) -> AppResult<()> {
    if w == 0 || h == 0 {
        return Err(invalid(name, "empty image"));
    }
    if w > MAX_SIDE || h > MAX_SIDE || (w as u64) * (h as u64) > MAX_FRAME_PIXELS {
        return Err(limit(name, format!("{w}x{h} is larger than {MAX_SIDE}x{MAX_SIDE}")));
    }
    Ok(())
}

fn encode_png(img: &image::RgbaImage) -> AppResult<Vec<u8>> {
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|e| skin_error(ErrorKind::Other, format!("first frame: {e}")))?;
    Ok(out)
}

/// 全部のコマを1つずつ復号して数える。最初のコマは PNG にする
fn frames(name: &str, w: u32, h: u32, frames: image::Frames) -> AppResult<Checked> {
    let mut count: u32 = 0;
    let mut pixels: u64 = 0;
    let mut duration: u64 = 0;
    let mut first = None;
    for f in frames {
        if count >= MAX_FRAMES {
            // 先頭のコマだけで受け入れる、ということはしない
            return Err(limit(name, format!("more than {MAX_FRAMES} frames")));
        }
        let f = f.map_err(|e| invalid(name, e))?;
        count += 1;
        let (fw, fh) = f.buffer().dimensions();
        pixels += fw as u64 * fh as u64;
        if pixels > MAX_ANIMATION_PIXELS {
            return Err(limit(name, format!("frames total more than {MAX_ANIMATION_PIXELS} pixels")));
        }
        let (n, d) = f.delay().numer_denom_ms();
        let ms = if d == 0 { 0 } else { n as u64 / d as u64 };
        duration += if ms <= FAST_FRAME_MS { 100 } else { ms };
        if duration > MAX_DURATION_MS {
            return Err(limit(name, format!("longer than {} seconds", MAX_DURATION_MS / 1000)));
        }
        if first.is_none() {
            first = Some(encode_png(f.buffer())?);
        }
    }
    if count == 0 {
        return Err(invalid(name, "no frames"));
    }
    Ok(Checked {
        info: ImageInfo { width: w, height: h, frames: count, duration_ms: duration, animated: count > 1, decoded_pixels: pixels },
        // 1コマだけの GIF は静止画として扱う（そのファイルをそのまま読む）
        first_frame_png: if count > 1 { first } else { None },
    })
}

fn still<D: ImageDecoder>(name: &str, w: u32, h: u32, dec: D) -> AppResult<Checked> {
    DynamicImage::from_decoder(dec).map_err(|e| invalid(name, e))?;
    Ok(Checked {
        info: ImageInfo { width: w, height: h, frames: 1, duration_ms: 0, animated: false, decoded_pixels: w as u64 * h as u64 },
        first_frame_png: None,
    })
}

/// 画像を確かめる。`name` は zip の中の名前（拡張子で形式を決める）
pub fn check(data: &[u8], name: &str) -> AppResult<Checked> {
    if data.len() as u64 > MAX_IMAGE_BYTES {
        return Err(skin_error(ErrorKind::SkinTooLarge, format!("{}: {} bytes", shown_name(name), data.len())));
    }
    if looks_executable(data) {
        return Err(skin_error(ErrorKind::SkinExecutable, format!("{}: executable content", shown_name(name))));
    }
    let want = expected_format(name).ok_or_else(|| invalid(name, "unknown extension"))?;
    match image::guess_format(data) {
        Ok(f) if f == want => {}
        _ => return Err(invalid(name, "content does not match the extension")),
    }
    let cur = || Cursor::new(data);
    match want {
        ImageFormat::Png => {
            let dec = PngDecoder::with_limits(cur(), limits()).map_err(|e| map_err(name, e))?;
            let (w, h) = dec.dimensions();
            check_dims(name, w, h)?;
            if dec.is_apng().map_err(|e| invalid(name, e))? {
                let apng = dec.apng().map_err(|e| invalid(name, e))?;
                frames(name, w, h, apng.into_frames())
            } else {
                still(name, w, h, dec)
            }
        }
        ImageFormat::Gif => {
            let mut dec = GifDecoder::new(cur()).map_err(|e| map_err(name, e))?;
            let (w, h) = dec.dimensions();
            check_dims(name, w, h)?;
            dec.set_limits(limits()).map_err(|e| map_err(name, e))?;
            frames(name, w, h, dec.into_frames())
        }
        ImageFormat::WebP => {
            let mut dec = WebPDecoder::new(cur()).map_err(|e| map_err(name, e))?;
            let (w, h) = dec.dimensions();
            check_dims(name, w, h)?;
            dec.set_limits(limits()).map_err(|e| map_err(name, e))?;
            if dec.has_animation() {
                frames(name, w, h, dec.into_frames())
            } else {
                still(name, w, h, dec)
            }
        }
        _ => Err(invalid(name, "unsupported format")),
    }
}

fn map_err(name: &str, e: image::ImageError) -> AppError {
    match e {
        image::ImageError::Limits(l) => limit(name, l),
        e => invalid(name, e),
    }
}

/// アプリ・トレイのアイコンにする：静止画を読み、縦横 `size` 以内の正方形（透明で埋める）の RGBA にする
pub fn app_icon_rgba(data: &[u8], name: &str, size: u32) -> AppResult<(Vec<u8>, u32, u32)> {
    if data.len() as u64 > MAX_IMAGE_BYTES || looks_executable(data) {
        return Err(invalid(name, "not an icon image"));
    }
    let fmt = expected_format(name).ok_or_else(|| invalid(name, "unknown extension"))?;
    let mut r = image::ImageReader::with_format(Cursor::new(data), fmt);
    r.limits(limits());
    let img = r.decode().map_err(|e| map_err(name, e))?;
    let (w, h) = (img.width(), img.height());
    check_dims(name, w, h)?;
    let side = w.max(h).min(size).max(1);
    let fitted = img.resize(side, side, image::imageops::FilterType::Lanczos3).to_rgba8();
    let mut canvas = image::RgbaImage::new(side, side);
    let (fw, fh) = fitted.dimensions();
    image::imageops::overlay(&mut canvas, &fitted, ((side - fw) / 2) as i64, ((side - fh) / 2) as i64);
    Ok((canvas.into_raw(), side, side))
}

/// 取り込みの確認画面の見本（data: URL の PNG）。縦横 `max` 以内に縮める
pub fn preview_data_url(png_or_image: &[u8], name: &str, max: u32) -> Option<String> {
    let fmt = expected_format(name).unwrap_or(ImageFormat::Png);
    let mut r = image::ImageReader::with_format(Cursor::new(png_or_image), fmt);
    r.limits(limits());
    let img = r.decode().ok()?;
    let img = if img.width() > max || img.height() > max { img.thumbnail(max, max) } else { img };
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(img.to_rgba8()).write_to(&mut Cursor::new(&mut out), ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", base64(&out)))
}

/// RGBA の画素を PNG の data: URL にする。縦横 `max` を超えれば縮める
/// （v3.11.0：設定の「アイコン」の欄に出す、今のアプリ・トレイのアイコンの見本）
pub fn rgba_data_url(rgba: Vec<u8>, w: u32, h: u32, max: u32) -> Option<String> {
    let img = DynamicImage::ImageRgba8(image::RgbaImage::from_raw(w, h, rgba)?);
    let img = if w > max || h > max { img.resize(max, max, image::imageops::FilterType::Lanczos3) } else { img };
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(img.to_rgba8()).write_to(&mut Cursor::new(&mut out), ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", base64(&out)))
}

/// 標準の base64（依存を足さないための最小限）
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}
