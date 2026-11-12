//! タグの読み込み（lofty）。原文の10項目：Title・Artist・Album・Genre・Year・Track Number・Disc Number・Composer・Comment・Artwork。

use std::path::Path;

use lofty::picture::PictureType;
use lofty::prelude::*;
use lofty::tag::Tag;

use super::artwork;
use crate::db::tracks::TrackInfo;
use crate::error::{AppError, ErrorKind};

/// 画面で編集するときに読む値（ファイルの今のタグ）
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TagValues {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub genre: String,
    pub year: Option<i64>,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub composer: String,
    pub comment: String,
}

pub fn values_from_tag(tag: &Tag) -> TagValues {
    TagValues {
        title: tag.title().map(|s| s.trim().to_string()).unwrap_or_default(),
        artist: tag.artist().map(|s| s.trim().to_string()).unwrap_or_default(),
        album: tag.album().map(|s| s.trim().to_string()).unwrap_or_default(),
        album_artist: tag
            .get_string(ItemKey::AlbumArtist)
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
        genre: tag.genre().map(|s| s.trim().to_string()).unwrap_or_default(),
        year: tag.date().map(|d| d.year as i64).or_else(|| {
            tag.get_string(ItemKey::Year)
                .and_then(|s| s.get(..4))
                .and_then(|s| s.parse().ok())
        }),
        track_no: tag.track().map(|n| n as i64),
        disc_no: tag.disk().map(|n| n as i64),
        composer: tag
            .get_string(ItemKey::Composer)
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
        comment: tag.comment().map(|s| s.trim().to_string()).unwrap_or_default(),
    }
}

/// いちばん適切な埋め込み画像（表紙を優先）
pub fn best_picture(tag: &Tag) -> Option<&lofty::picture::Picture> {
    tag.get_picture_type(PictureType::CoverFront)
        .or_else(|| tag.pictures().first())
}

/// lofty は細工したファイル（チャンネル数 0 の Opus など）で panic することがある
/// （`docs\ペネトレーションテスト_v3.0.2.md` の3）。読み取りを包み、panic は None にする（呼び出し側で Err に倒す）。
/// v3.1.0 直し2回目で、読み取り側（open_tagged・read_values・read_lyrics）にだけ入れた。書き込み（tags_write）の後始末は v3.1.1
fn no_panic<T>(path: &Path, f: impl FnOnce() -> T) -> Option<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(v) => Some(v),
        Err(_) => {
            tracing::warn!("tag reader panicked: {}", path.display());
            None
        }
    }
}

fn panicked_error(path: &Path) -> AppError {
    AppError::new(ErrorKind::TagReadFailed, format!("{}: tag reader panicked", path.display()))
}

/// lofty でファイルを開く。
///
/// v1 の不具合：既定の読み方は、音声部分（MPEG のフレーム）の長さ計算まで行い、そこで失敗すると
/// タグが読めていてもファイル全体を「読めない」にしていた。タグを読み書きするだけなら長さは要らないので、
/// 失敗したら「長さを読まない・寛容に読む」でもう一度開く。
/// lofty が panic したら Err（v3.1.0 直し2回目）
pub fn open_tagged(path: &Path) -> Result<lofty::file::TaggedFile, lofty::error::FileParseError> {
    no_panic(path, || open_tagged_inner(path))
        .unwrap_or_else(|| Err(std::io::Error::other("tag reader panicked").into()))
}

fn open_tagged_inner(path: &Path) -> Result<lofty::file::TaggedFile, lofty::error::FileParseError> {
    match lofty::read_from_path(path) {
        Ok(t) => Ok(t),
        Err(first) => {
            let opts = lofty::config::ParseOptions::new()
                .read_properties(false)
                .parsing_mode(lofty::config::ParsingMode::Relaxed);
            lofty::probe::Probe::open(path)
                .and_then(|p| p.options(opts).read())
                .map_err(|_| first)
        }
    }
}

pub fn format_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// 1曲ぶんを読む。タグが読めなくてもファイル名で取り込めるように、`tag_error` を立てて返す。
pub fn read_track(path: &Path, thumbs_dir: &Path) -> Result<TrackInfo, AppError> {
    let meta = std::fs::metadata(path).map_err(|e| AppError::from_io(&e, path))?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut info = TrackInfo {
        path: path.to_string_lossy().into_owned(),
        file_size: meta.len() as i64,
        file_mtime: mtime,
        format: format_of(path),
        title: stem.clone(),
        ..Default::default()
    };
    match open_tagged(path) {
        Ok(tagged) => {
            info.duration_ms = tagged.properties().duration().as_millis() as i64;
            let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
            if let Some(tag) = tag {
                let v = values_from_tag(tag);
                if !v.title.is_empty() {
                    info.title = v.title;
                }
                info.artist = v.artist;
                info.album = v.album;
                info.album_artist = v.album_artist;
                info.genre = v.genre;
                info.year = v.year;
                info.track_no = v.track_no;
                info.disc_no = v.disc_no;
                info.composer = v.composer;
                info.comment = v.comment;
                if let Some(pic) = best_picture(tag) {
                    info.has_art = true;
                    info.art_key = artwork::store_thumbnail(thumbs_dir, pic.data());
                }
            }
        }
        Err(e) => {
            tracing::info!("tag read failed {}: {e}", path.display());
            info.tag_error = true;
        }
    }
    // 長さを読まずに開き直した曲（v2）は長さが 0 になる。音声を開いて長さを取る
    if info.duration_ms <= 0 {
        if let Ok(d) = crate::audio::decoder::Decoder::open(path) {
            info.duration_ms = d.duration_ms as i64;
        }
    }
    Ok(info)
}

/// 編集画面用：今のタグの値。lofty が panic したら Err（v3.1.0 直し2回目）
pub fn read_values(path: &Path) -> Result<TagValues, AppError> {
    no_panic(path, || read_values_inner(path)).unwrap_or_else(|| Err(panicked_error(path)))
}

fn read_values_inner(path: &Path) -> Result<TagValues, AppError> {
    let tagged = open_tagged(path).map_err(|e| {
        let kind = if !path.exists() {
            ErrorKind::FileNotFound
        } else {
            ErrorKind::TagReadFailed
        };
        AppError::new(kind, format!("{}: {e}", path.display()))
    })?;
    Ok(tagged
        .primary_tag()
        .or_else(|| tagged.first_tag())
        .map(values_from_tag)
        .unwrap_or_default())
}

/// 曲ファイルの歌詞タグを読む（v3、仕様 2.3）。読むだけで、書き込み（tags_write）にはつながない。
///
/// lofty 0.25.4 は MP3 の USLT を `ItemKey::UnsyncLyrics`、M4A の `©lyr` を `Lyrics`／`UnsyncLyrics`、
/// Vorbis コメントの LYRICS を `Lyrics` として返す。主のタグから順に見て、空でない最初の1件を使う
/// （言語での選び分けはしない）。歌詞が無ければ `Ok(None)`。
pub fn read_lyrics(path: &Path) -> Result<Option<String>, AppError> {
    // lofty が panic したら Err（v3.1.0 直し2回目。歌詞なし＋警告として扱われる）
    no_panic(path, || read_lyrics_inner(path)).unwrap_or_else(|| Err(panicked_error(path)))
}

/// 歌詞のタグを読むときの、タグの1項目あたりの割り当ての上限（v3.1.1。ペネトレーションテスト v3.0.2 の11）。
/// lofty の既定は 16MiB。歌詞は 1MiB を超えたら表示しないので、数MB に絞る。
/// （M4A は ilst 全体を、Vorbis コメントは表紙の項目も1つずつ割り当てるので、表紙が約 8MB を超える曲は歌詞のタグを読めず、
/// 歌詞画面に「歌詞タグを読み込めませんでした」が出る）
pub const LYRICS_TAG_ALLOC_LIMIT: usize = 8 * 1024 * 1024;

/// lofty の割り当ての上限を、このスレッドでだけ一時的に変える（抜けるときに既定へ戻す。panic で抜けても）
struct LoftyAllocLimit;

impl LoftyAllocLimit {
    fn set(limit: usize) -> Self {
        lofty::config::apply_global_options(lofty::config::GlobalOptions::new().allocation_limit(limit));
        LoftyAllocLimit
    }
}

impl Drop for LoftyAllocLimit {
    fn drop(&mut self) {
        lofty::config::apply_global_options(lofty::config::GlobalOptions::new());
    }
}

/// ファイルの頭の ID3v2 に、表紙（APIC・PIC）以外で `limit` を超える項目があるか（ファイルを読むだけで、項目の中身は割り当てない）。
///
/// lofty の割り当ての上限は MP3・AAC の頭の ID3v2 の項目（USLT など）には効かず、項目の大きさ（最大 256MB）まで読む。
/// そこで、lofty に渡す前に項目の見出しだけを見て、大きすぎる項目があれば読まない。
///
/// v3.1.1 直し1回目：見る範囲を lofty 0.25.4 の読み方（`mpeg\read.rs`・`aac\read.rs` の頭）にそろえた。
/// - 先頭に続く 0x00 を読み飛ばす
/// - `ID3` が続く限り、次のタグの見出しも確かめる（lofty は後ろのタグの項目も前のタグに足して読む）
/// - 頭に APE タグ（`APETAGEX`）があれば、そのあとの位置を lofty と同じには追えないので、大きすぎる扱いにする（読まない側に倒す。まれな形）
///
/// 表紙は読まない設定（read_cover_art(false)）で読み飛ばすので数えない。
/// 見出しが壊れていて歩けなくなったら、そのタグの残りは飛ばす（lofty もそこから先は項目として読まない）。
/// ID3v2.3 までのタグ全体の非同期化（まれ）は見出しが崩れうるので、タグ全体の大きさで判断する。
///
/// v3.1.1 直し2回目（検査2回目の指摘A）：細工した見出し（空のタグ 500万個・サイズ 0 の項目 200万個など）で、
/// 確かめ自体が何秒も読み続けていた。
/// - 見るタグは MAX_ID3_TAGS 個、項目は合計 MAX_ID3_FRAMES 個まで。超えたら「大きすぎる」扱い（読まない）
/// - 読み飛ばしは `BufReader::seek_relative`（読み込み用の中身を捨てない）。位置は自分で数える
fn id3v2_has_oversized_frame(path: &Path, limit: usize) -> std::io::Result<bool> {
    let mut f = std::io::BufReader::new(std::fs::File::open(path)?);
    leading_tags_have_oversized_frame(&mut f, limit)
}

/// 見出しの確かめで見るタグの数の上限（ふつうの曲は1つ、多くて2つ）
const MAX_ID3_TAGS: usize = 64;
/// 見出しの確かめで見る項目の数の上限（全部のタグの合計。ふつうの曲は数十）
const MAX_ID3_FRAMES: usize = 10_000;

/// 1つのタグを歩いた結果
enum TagWalk {
    /// 表紙以外で上限を超える項目があった、または項目の数が上限を超えた
    TooLarge,
    /// 歩き終えた。タグの見出しの直後から読み進めたバイト数
    Done(u64),
    /// ファイルの終わりに着いた
    Eof,
}

fn leading_tags_have_oversized_frame<R: std::io::Read + std::io::Seek>(f: &mut std::io::BufReader<R>, limit: usize) -> std::io::Result<bool> {
    use std::io::Read;
    // 先頭の 0x00 を読み飛ばす（lofty と同じ）
    let mut b = [0u8; 1];
    loop {
        if f.read(&mut b)? == 0 {
            return Ok(false);
        }
        if b[0] != 0 {
            break;
        }
    }
    f.seek_relative(-1)?;
    let synchsafe = |b: &[u8]| b.iter().fold(0u64, |a, &x| (a << 7) | (x & 0x7f) as u64);
    let mut frames = 0usize;
    let mut tags = 0usize;
    loop {
        let mut h = [0u8; 10];
        if f.read_exact(&mut h[..4]).is_err() {
            return Ok(false);
        }
        if &h[..4] == b"APET" {
            let mut rest = [0u8; 4];
            return Ok(f.read_exact(&mut rest).is_ok() && &rest == b"AGEX");
        }
        if &h[..3] != b"ID3" || f.read_exact(&mut h[4..]).is_err() {
            return Ok(false);
        }
        tags += 1;
        if tags > MAX_ID3_TAGS {
            return Ok(true); // タグが多すぎる
        }
        let (ver, flags) = (h[3], h[5]);
        let tag_size = synchsafe(&h[6..10]);
        let consumed = if flags & 0x80 != 0 && ver < 4 {
            if tag_size > limit as u64 {
                return Ok(true);
            }
            0
        } else {
            match tag_has_oversized_frame(f, ver, flags, tag_size, limit, &mut frames)? {
                TagWalk::TooLarge => return Ok(true),
                TagWalk::Eof => return Ok(false),
                TagWalk::Done(n) => n,
            }
        };
        // 次のタグへ（ID3v2.4 の後ろの見出し（footer）も飛ばす）。今の位置はタグの見出しの直後＋consumed
        let footer = if ver >= 4 && flags & 0x10 != 0 { 10 } else { 0 };
        f.seek_relative((tag_size + footer) as i64 - consumed as i64)?;
    }
}

/// 1つのタグの中の項目の見出しを歩く（読み位置はタグの見出しの直後）。`frames` は全部のタグで数えた項目の数
fn tag_has_oversized_frame<R: std::io::Read + std::io::Seek>(
    f: &mut std::io::BufReader<R>,
    ver: u8,
    flags: u8,
    tag_size: u64,
    limit: usize,
    frames: &mut usize,
) -> std::io::Result<TagWalk> {
    use std::io::Read;
    let synchsafe = |b: &[u8]| b.iter().fold(0u64, |a, &x| (a << 7) | (x & 0x7f) as u64);
    let mut pos = 0u64;
    if flags & 0x40 != 0 && ver >= 3 {
        let mut e = [0u8; 4];
        if f.read_exact(&mut e).is_err() {
            return Ok(TagWalk::Eof);
        }
        let skip = if ver == 3 { u32::from_be_bytes(e) as u64 } else { synchsafe(&e).saturating_sub(4) };
        f.seek_relative(skip as i64)?;
        pos = 4 + skip;
    }
    let head_len: u64 = if ver == 2 { 6 } else { 10 };
    let mut head = [0u8; 10];
    while pos + head_len <= tag_size {
        if f.read_exact(&mut head[..head_len as usize]).is_err() {
            return Ok(TagWalk::Eof);
        }
        pos += head_len;
        let id_len = if ver == 2 { 3 } else { 4 };
        let id = &head[..id_len];
        if !id.iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
            break; // 詰め物（0）か、壊れた見出し
        }
        *frames += 1;
        if *frames > MAX_ID3_FRAMES {
            return Ok(TagWalk::TooLarge);
        }
        let size = match ver {
            2 => (head[3] as u64) << 16 | (head[4] as u64) << 8 | head[5] as u64,
            3 => u32::from_be_bytes([head[4], head[5], head[6], head[7]]) as u64,
            _ => synchsafe(&head[4..8]),
        };
        let picture = id == b"APIC" || id == b"PIC";
        if !picture && size > limit as u64 {
            return Ok(TagWalk::TooLarge);
        }
        f.seek_relative(size as i64)?;
        pos += size;
    }
    Ok(TagWalk::Done(pos))
}

fn read_lyrics_inner(path: &Path) -> Result<Option<String>, AppError> {
    // v3.1.1（ペネトレーションテスト v3.0.2 の11）：大きすぎる項目を読まない（120MB の歌詞でピーク約 490MB を使っていた）
    let too_large = || AppError::new(ErrorKind::TagReadFailed, format!("{}: lyrics tag too large", path.display()));
    if id3v2_has_oversized_frame(path, LYRICS_TAG_ALLOC_LIMIT).unwrap_or(false) {
        return Err(too_large());
    }
    let _limit = LoftyAllocLimit::set(LYRICS_TAG_ALLOC_LIMIT);
    // 長さは要らないので読まない（表示のたびに読むため、軽くする）。表紙も要らないので読み飛ばす。だめなら寛容に読む
    let strict = lofty::config::ParseOptions::new().read_properties(false).read_cover_art(false);
    let relaxed = lofty::config::ParseOptions::new()
        .read_properties(false)
        .read_cover_art(false)
        .parsing_mode(lofty::config::ParsingMode::Relaxed);
    let tagged = lofty::probe::Probe::open(path)
        .and_then(|p| p.options(strict).read())
        .or_else(|_| lofty::probe::Probe::open(path).and_then(|p| p.options(relaxed).read()))
        .map_err(|e| {
            let kind = if !path.exists() {
                ErrorKind::FileNotFound
            } else {
                ErrorKind::TagReadFailed
            };
            AppError::new(kind, format!("{}: {e}", path.display()))
        })?;
    let primary = tagged.primary_tag_type();
    let mut tags: Vec<&Tag> = tagged.tags().iter().collect();
    // 主のタグを先に
    tags.sort_by_key(|t| if t.tag_type() == primary { 0 } else { 1 });
    for tag in tags {
        for key in [ItemKey::UnsyncLyrics, ItemKey::Lyrics] {
            if let Some(s) = tag.get_strings(key).find(|s| !s.trim().is_empty()) {
                // v3.1.1 直し1回目：二重の守り。見出しの確かめをすり抜けて読めてしまった歌詞も、上限を超えれば捨てる
                // （歌詞なし＋「歌詞タグを読み込めませんでした」の警告）
                if s.len() > LYRICS_TAG_ALLOC_LIMIT {
                    tracing::info!("lyrics tag too large after reading: {}", path.display());
                    return Err(too_large());
                }
                return Ok(Some(s.to_string()));
            }
        }
    }
    Ok(None)
}

/// テスト用：フィクスチャの sine.opus の OpusHead のチャンネル数を 0 にし、ページの CRC を付け直した Opus を
/// `target\test-tmp` に作る（ペネトレーションテスト v3.0.2 の3。lofty が "Channel count is valid" で panic する）
#[cfg(test)]
pub(crate) fn test_zero_channel_opus(tag: &str) -> std::path::PathBuf {
    let src = std::fs::read(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("sine.opus")).unwrap();
    // 最初のページの OpusHead：ページの見出し 27＋区切り表 1 バイトのあと、"OpusHead"(8)・版(1)・チャンネル数(1)
    let at = 27 + src[26] as usize;
    assert_eq!(&src[at..at + 8], b"OpusHead");
    let mut bad = src.clone();
    bad[at + 9] = 0;
    // ページの CRC を付け直す（Ogg の CRC-32：多項式 0x04C11DB7、反転なし、初期値 0。CRC の欄は 0 にして計算）
    let seg_total: usize = bad[27..27 + bad[26] as usize].iter().map(|&b| b as usize).sum();
    let end = 27 + bad[26] as usize + seg_total;
    bad[22..26].copy_from_slice(&[0, 0, 0, 0]);
    let mut crc: u32 = 0;
    for &byte in &bad[..end] {
        crc ^= (byte as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 { (crc << 1) ^ 0x04C1_1DB7 } else { crc << 1 };
        }
    }
    bad[22..26].copy_from_slice(&crc.to_le_bytes());
    let dir = crate::test_tmp().join(format!("plp-panic-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("zero_channels.opus");
    std::fs::write(&p, &bad).unwrap();
    p
}

/// テスト用：フィクスチャの複製に歌詞タグを書く（元のフィクスチャには書かない。置き場所は target\test-tmp）
#[cfg(test)]
pub(crate) fn test_copy_with_lyrics(name: &str, tag: &str, lyrics: &str) -> std::path::PathBuf {
    let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name);
    let dir = crate::test_tmp().join(format!("plp-lyrtag-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join(name);
    std::fs::copy(src, &dst).unwrap();
    let mut tagged = open_tagged(&dst).unwrap();
    let tt = tagged.primary_tag_type();
    if tagged.tag(tt).is_none() {
        tagged.insert_tag(Tag::new(tt));
    }
    let t = tagged.tag_mut(tt).unwrap();
    t.insert_text(ItemKey::UnsyncLyrics, lyrics.to_string());
    t.save_to_path(&dst, lofty::config::WriteOptions::default()).unwrap();
    dst
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// v3.1.0 直し2回目：lofty が panic するファイル（OpusHead のチャンネル数を 0 にした Opus。ペネトレーションテスト v3.0.2 の3）
    /// でも、読み取りは panic せず Err を返す
    #[test]
    fn panicking_files_become_errors() {
        let p = test_zero_channel_opus("read");
        // 前提：包まない lofty はこのファイルで panic する（しなければ、このテストは何も確かめていない）
        let raw = std::panic::catch_unwind(|| lofty::read_from_path(&p));
        assert!(raw.is_err(), "lofty did not panic on the crafted file");
        // 包んだ読み取りは Err を返す
        assert!(open_tagged(&p).is_err());
        assert_eq!(read_values(&p).unwrap_err().kind, ErrorKind::TagReadFailed);
        // 歌詞は長さ（properties）を読まない設定で開くので、このファイルでは lofty も panic しない。どちらでも歌詞なしになる
        assert_eq!(read_lyrics(&p).ok().flatten(), None);
        // 元のフィクスチャは今までどおり読める
        assert!(read_values(&fixture("sine.opus")).is_ok());
    }

    #[test]
    fn reads_lyrics_tag_from_mp3_and_m4a() {
        let mp3 = test_copy_with_lyrics("sine.mp3", "mp3", "[00:01.00]ダミーの一行目\n[00:02.00]ダミーの二行目");
        let got = read_lyrics(&mp3).unwrap().unwrap();
        assert!(got.contains("ダミーの一行目"), "{got}");
        let m4a = test_copy_with_lyrics("sine_aaclc.m4a", "m4a", "ダミーの全文");
        assert_eq!(read_lyrics(&m4a).unwrap().as_deref(), Some("ダミーの全文"));
        // 歌詞タグの無い曲
        assert_eq!(read_lyrics(&fixture("sine.flac")).unwrap(), None);
        // 無いファイル・壊れたファイルは歌詞なし扱いにできるようエラーで返す（画面は警告にとどめる）
        assert!(read_lyrics(&fixture("no-such-file.mp3")).is_err());
    }

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)
    }

    #[test]
    fn reads_tags_and_duration() {
        let dir = crate::test_tmp();
        let t = read_track(&fixture("sine.mp3"), &dir).unwrap();
        assert_eq!(t.title, "Sine MP3");
        assert_eq!(t.artist, "Test Artist");
        assert_eq!(t.album, "Test Album");
        assert_eq!(t.track_no, Some(1));
        assert!((2900..=3200).contains(&t.duration_ms), "{}", t.duration_ms);
        assert!(!t.tag_error);
    }

    #[test]
    fn untagged_uses_file_name() {
        let t = read_track(&fixture("sine.wav"), &crate::test_tmp()).unwrap();
        assert_eq!(t.title, "sine");
    }

    #[test]
    fn all_formats_have_duration() {
        for name in ["sine.mp3", "sine.wav", "sine.flac", "sine_aaclc.m4a", "sine_alac.m4a", "sine.ogg", "sine.opus"] {
            let t = read_track(&fixture(name), &crate::test_tmp()).unwrap();
            assert!(t.duration_ms > 2500, "{name}: {}", t.duration_ms);
        }
    }

    #[test]
    fn broken_file_sets_tag_error() {
        let t = read_track(&fixture("broken.mp3"), &crate::test_tmp()).unwrap();
        assert!(t.tag_error);
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の11）：歌詞のタグは数MB までしか読まない。
    /// MP3（USLT。lofty の割り当ての上限が効かない形）も、FLAC（Vorbis コメント）も、9MB の歌詞は読まずに Err
    /// （歌詞画面は歌詞なし＋警告）。上限は読み終えたら既定へ戻る（同じスレッドの、ほかの読み取りは今までどおり）
    #[test]
    fn huge_lyrics_tags_are_not_read() {
        let huge = "a".repeat(9 * 1024 * 1024);
        let mp3 = test_copy_with_lyrics("sine.mp3", "huge-mp3", &huge);
        assert!(id3v2_has_oversized_frame(&mp3, LYRICS_TAG_ALLOC_LIMIT).unwrap());
        assert_eq!(read_lyrics(&mp3).unwrap_err().kind, ErrorKind::TagReadFailed);
        let flac = test_copy_with_lyrics("sine.flac", "huge-flac", &huge);
        assert_eq!(read_lyrics(&flac).unwrap_err().kind, ErrorKind::TagReadFailed);
        // 上限が戻っている：既定（16MiB）で全部のタグを読む編集画面の読み取りは通る
        assert!(read_values(&flac).is_ok());
        // ふつうの大きさなら読める（上限のすぐ下）
        let ok = "b".repeat(1024 * 1024);
        let mp3_ok = test_copy_with_lyrics("sine.mp3", "ok-mp3", &ok);
        assert!(!id3v2_has_oversized_frame(&mp3_ok, LYRICS_TAG_ALLOC_LIMIT).unwrap());
        assert_eq!(read_lyrics(&mp3_ok).unwrap().map(|s| s.len()), Some(ok.len()));
        for p in [mp3, flac, mp3_ok] {
            let _ = std::fs::remove_dir_all(p.parent().unwrap());
        }
    }

    /// 大きな表紙（9MB）は歌詞の上限に数えない（MP3 は表紙を読み飛ばす）。歌詞は読める
    #[test]
    fn big_cover_does_not_hide_mp3_lyrics() {
        let p = test_copy_with_lyrics("sine.mp3", "bigcover", "[00:01.00]表紙が大きい曲");
        let mut tagged = open_tagged(&p).unwrap();
        let tt = tagged.primary_tag_type();
        let mut data = vec![0u8; 9 * 1024 * 1024];
        data[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        tagged.tag_mut(tt).unwrap().push_picture(
            lofty::picture::Picture::unchecked(data)
                .pic_type(PictureType::CoverFront)
                .mime_type(lofty::picture::MimeType::Png)
                .build(),
        );
        tagged.tag_mut(tt).unwrap().save_to_path(&p, lofty::config::WriteOptions::default()).unwrap();
        assert!(!id3v2_has_oversized_frame(&p, LYRICS_TAG_ALLOC_LIMIT).unwrap());
        assert!(read_lyrics(&p).unwrap().unwrap().contains("表紙が大きい曲"));
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    /// lofty 0.25.4 が 8MiB の上限で、9MB の歌詞をそのまま読めてしまう形か（前提の確かめ）
    fn lofty_reads_huge_lyrics(p: &std::path::Path) -> bool {
        let _limit = LoftyAllocLimit::set(LYRICS_TAG_ALLOC_LIMIT);
        let opts = lofty::config::ParseOptions::new().read_properties(false).read_cover_art(false);
        let Ok(t) = lofty::probe::Probe::open(p).and_then(|x| x.options(opts).read()) else {
            return false;
        };
        t.tags().iter().any(|tag| tag.get_strings(ItemKey::UnsyncLyrics).any(|s| s.len() > LYRICS_TAG_ALLOC_LIMIT))
    }

    /// v3.1.1 直し1回目（検査1回目の指摘1）：lofty は先頭の 0x00 を読み飛ばし、続く ID3v2 タグを全部読む。
    /// 「先頭に 0x00 を16個＋9MB の USLT」と「小さいタグの直後に 9MB のタグ」も、読む前に断る
    #[test]
    fn padded_and_chained_id3_tags_are_checked() {
        let huge = "c".repeat(9 * 1024 * 1024);
        let base = test_copy_with_lyrics("sine.mp3", "chain", &huge);
        let bytes = std::fs::read(&base).unwrap();
        assert_eq!(&bytes[..3], b"ID3");
        let dir = base.parent().unwrap().to_path_buf();
        // 1. 先頭に 0x00 を16個
        let padded = dir.join("padded.mp3");
        let mut v = vec![0u8; 16];
        v.extend_from_slice(&bytes);
        std::fs::write(&padded, &v).unwrap();
        // 2. 小さいタグ（ID3v2.4・TIT2 だけ）の直後に、9MB の歌詞のタグ
        let chained = dir.join("chained.mp3");
        let frame = [b"TIT2".as_slice(), &[0, 0, 0, 5, 0, 0], &[3, b'a', b'b', b'c', b'd']].concat();
        let mut v = [b"ID3".as_slice(), &[4, 0, 0], &[0, 0, 0, frame.len() as u8]].concat();
        v.extend_from_slice(&frame);
        v.extend_from_slice(&bytes);
        std::fs::write(&chained, &v).unwrap();
        for p in [&padded, &chained] {
            // 前提：lofty は上限 8MiB でも 9MB の歌詞を読めてしまう（見出しの確かめが無ければ通る形）
            assert!(lofty_reads_huge_lyrics(p), "{}: lofty did not read the huge lyrics", p.display());
            assert!(id3v2_has_oversized_frame(p, LYRICS_TAG_ALLOC_LIMIT).unwrap(), "{}", p.display());
            assert_eq!(read_lyrics(p).unwrap_err().kind, ErrorKind::TagReadFailed, "{}", p.display());
        }
        // 小さいタグだけが続く形・0x00 だけのファイルは「大きすぎない」
        let small = test_copy_with_lyrics("sine.mp3", "chain-small", "[00:01.00]小さい歌詞");
        let sb = std::fs::read(&small).unwrap();
        let mut v = vec![0u8; 16];
        v.extend_from_slice(&[b"ID3".as_slice(), &[4, 0, 0], &[0, 0, 0, frame.len() as u8]].concat());
        v.extend_from_slice(&frame);
        v.extend_from_slice(&sb);
        let ok = small.parent().unwrap().join("ok.mp3");
        std::fs::write(&ok, &v).unwrap();
        assert!(!id3v2_has_oversized_frame(&ok, LYRICS_TAG_ALLOC_LIMIT).unwrap());
        assert!(read_lyrics(&ok).unwrap().unwrap().contains("小さい歌詞"));
        let mut zeros = std::io::BufReader::new(std::io::Cursor::new(vec![0u8; 64]));
        assert!(!leading_tags_have_oversized_frame(&mut zeros, LYRICS_TAG_ALLOC_LIMIT).unwrap());
        // 頭の APE タグのあとは lofty と同じには追えないので、読まない側に倒す
        let mut ape = std::io::BufReader::new(std::io::Cursor::new([b"APETAGEX".as_slice(), &[0; 24]].concat()));
        assert!(leading_tags_have_oversized_frame(&mut ape, LYRICS_TAG_ALLOC_LIMIT).unwrap());
        for d in [dir, small.parent().unwrap().to_path_buf()] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    /// v3.1.1 直し2回目（検査2回目の指摘A）：細工した見出しでも、確かめはすぐ終わる（読まない側に倒す）。
    /// 空のタグ 500万個（約 50MB）・サイズ 0 の項目 200万個（約 20MB）
    #[test]
    fn crafted_headers_are_refused_quickly() {
        let dir = crate::test_tmp().join(format!("plp-id3walk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // 1. 空のタグ（`ID3` ＋ v2.4 ＋ サイズ 0）を 500万個
        let empty_tags = dir.join("empty_tags.mp3");
        std::fs::write(&empty_tags, b"ID3\x04\x00\x00\x00\x00\x00\x00".repeat(5_000_000)).unwrap();
        // 2. 1つのタグ（v2.3）の中に、サイズ 0 の項目を 200万個
        let zero_frames = dir.join("zero_frames.mp3");
        let n = 2_000_000usize;
        let size = n * 10;
        let ss = [(size >> 21) as u8 & 0x7f, (size >> 14) as u8 & 0x7f, (size >> 7) as u8 & 0x7f, size as u8 & 0x7f];
        let mut v = [b"ID3\x03\x00\x00".as_slice(), &ss].concat();
        v.extend_from_slice(&b"TXXX\x00\x00\x00\x00\x00\x00".repeat(n));
        std::fs::write(&zero_frames, &v).unwrap();
        for p in [&empty_tags, &zero_frames] {
            let t = std::time::Instant::now();
            assert!(id3v2_has_oversized_frame(p, LYRICS_TAG_ALLOC_LIMIT).unwrap(), "{}", p.display());
            let ms = t.elapsed().as_millis();
            println!("{}: {ms} ms", p.file_name().unwrap().to_string_lossy());
            assert!(ms < 500, "{}: {ms} ms", p.display());
        }
        // 上限の手前（タグ 64 個・項目 10,000 個）は、大きすぎない
        let mut ok = b"ID3\x04\x00\x00\x00\x00\x00\x00".repeat(MAX_ID3_TAGS);
        let size = MAX_ID3_FRAMES * 10;
        ok.extend_from_slice(&[b"ID3\x03\x00\x00".as_slice(), &[(size >> 21) as u8 & 0x7f, (size >> 14) as u8 & 0x7f, (size >> 7) as u8 & 0x7f, size as u8 & 0x7f]].concat());
        ok.extend_from_slice(&b"TXXX\x00\x00\x00\x00\x00\x00".repeat(MAX_ID3_FRAMES - 1));
        ok.extend_from_slice(b"\xff\xfb\x90\x00");
        // ここではタグが 65 個になるので、64 個に収まるよう空のタグを1つ減らす
        let ok = ok[10..].to_vec();
        let mut r = std::io::BufReader::new(std::io::Cursor::new(ok));
        assert!(!leading_tags_have_oversized_frame(&mut r, LYRICS_TAG_ALLOC_LIMIT).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
