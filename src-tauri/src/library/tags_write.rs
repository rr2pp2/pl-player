//! タグの安全な書き込み（技術調査 7章の5段階）。
//!
//! 1. 元ファイルと同じフォルダに一時ファイルとして複製する（置換は同じドライブの中でしかできないため）
//! 2. 一時ファイルにだけ lofty で書く
//! 3. 一時ファイルを読み直して値を照合し、Symphonia で頭の数フレームを実際に復号できるか確かめる
//! 4. Windows の ReplaceFileW で差し替える
//! 5. 途中で失敗したら一時ファイルを消して、元は触らない
//!
//! v3.1.1（`docs\ペネトレーションテスト_v3.0.2.md` の3）：書き込み全体を `catch_unwind` で包み、lofty などが panic しても
//! エラーとして返す。一時ファイルは Drop ガードで必ず消す（panic で抜けたときも）

use std::fs;
use std::path::{Path, PathBuf};

use lofty::config::WriteOptions;
use lofty::picture::{Picture, PictureType};
use lofty::prelude::*;
use lofty::tag::items::Timestamp;
use lofty::tag::Tag;
use serde::Deserialize;

use super::artwork;
use super::tags_read::{values_from_tag, TagValues};
use crate::audio::decoder::Decoder;
use crate::error::{AppError, AppResult, ErrorKind};

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "kind", content = "path")]
pub enum ArtChange {
    #[default]
    Keep,
    /// 指定した画像ファイルを表紙として埋め込む（既存の表紙は差し替え）
    Set(String),
    /// 埋め込み画像をすべて消す
    Remove,
}

/// テストで「わざと失敗させる」ための場所
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailPoint {
    None,
    AfterTempWrite,
    BeforeReplace,
    /// 一時ファイルに書いたあとで panic する（v3.1.1。後始末と Err になることを確かめる）
    PanicAfterTempWrite,
}

/// 一時ファイルを、抜け方（成功・エラー・panic）にかかわらず消す。
/// 成功したときは ReplaceFileW が一時ファイルを使い切っているので、消すものは無い
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        if self.0.exists() {
            if let Err(e) = fs::remove_file(&self.0) {
                tracing::warn!("temp file not removed {}: {e}", self.0.display());
            }
        }
    }
}

fn tag_write_err(path: &Path, what: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorKind::TagWriteFailed, format!("{}: {what}", path.display()))
}

fn temp_path_for(path: &Path) -> PathBuf {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // 拡張子を保つ（lofty が形式を拡張子と中身で判定するため）
    dir.join(format!(".{stem}.plp-tmp-{nonce:x}.{ext}"))
}

/// 書き込みの設定（v2 で見直し）
///
/// - 書く前の形式の判定を、読むときと同じく寛容にする。ID3v2 の後ろに詰め物が長く続く MP3 は、
///   既定（1KB まで）では MP3 と判定できず「書けない」になっていた（作者のライブラリで多数）
/// - ID3v2 は v2.3 で書く。元の曲の多くが v2.3 で、Windows のエクスプローラーなどほかのアプリが確実に読める形
fn write_options(deep: bool) -> WriteOptions {
    let mut parse = lofty::config::ParseOptions::new()
        .read_properties(false)
        .parsing_mode(lofty::config::ParsingMode::Relaxed);
    if deep {
        // 詰め物の長い MP3 用。1バイトずつ探すので遅い（実測 1曲 数秒）。速いほうで失敗したときだけ使う
        parse = parse.max_junk_bytes(8 * 1024 * 1024);
    }
    WriteOptions::default().parse_options(parse).use_id3v23(true)
}

/// まず速い設定で行い、失敗したら判定を深くしてもう一度だけ行う
fn with_write_options<T>(
    mut f: impl FnMut(WriteOptions) -> Result<T, lofty::error::FileEncodingError>,
) -> Result<T, lofty::error::FileEncodingError> {
    match f(write_options(false)) {
        Ok(v) => Ok(v),
        Err(_) => f(write_options(true)),
    }
}

/// 下位のエラーまでつないだ文（「書けない」だけでは原因が分からないため）
fn error_chain(e: &dyn std::error::Error) -> String {
    let mut s = e.to_string();
    let mut cur = e.source();
    while let Some(x) = cur {
        s.push_str(" <- ");
        s.push_str(&x.to_string());
        cur = x.source();
    }
    s
}

/// 歌詞・コメントの「言語」が3文字の英字でないと、ID3v2 に書き戻せない（lofty が書き込みを断る）。
/// 読めた値はそのまま保ち、言語だけを「不明（XXX）」に直す
fn sanitize_langs(tag: &mut Tag) {
    for key in [ItemKey::Lyrics, ItemKey::UnsyncLyrics, ItemKey::Comment] {
        let bad: Vec<lofty::tag::TagItem> = tag
            .take_filter(key, |i| !i.lang().iter().all(|b| b.is_ascii_alphabetic()))
            .collect();
        for mut it in bad {
            it.set_lang(lofty::tag::items::UNKNOWN_LANGUAGE);
            tag.push_unchecked(it);
        }
    }
}

fn apply_values(tag: &mut Tag, v: &TagValues) {
    fn set_or_remove(tag: &mut Tag, key: ItemKey, s: &str) {
        if s.trim().is_empty() {
            tag.remove_key(key);
        } else {
            tag.insert_text(key, s.trim().to_string());
        }
    }
    if v.title.trim().is_empty() {
        tag.remove_title();
    } else {
        tag.set_title(v.title.trim().to_string());
    }
    if v.artist.trim().is_empty() {
        tag.remove_artist();
    } else {
        tag.set_artist(v.artist.trim().to_string());
    }
    if v.album.trim().is_empty() {
        tag.remove_album();
    } else {
        tag.set_album(v.album.trim().to_string());
    }
    set_or_remove(tag, ItemKey::AlbumArtist, &v.album_artist);
    if v.genre.trim().is_empty() {
        tag.remove_genre();
    } else {
        tag.set_genre(v.genre.trim().to_string());
    }
    match v.year {
        Some(y) if (1..=9999).contains(&y) => tag.set_date(Timestamp {
            year: y as u16,
            ..Default::default()
        }),
        _ => tag.remove_date(),
    }
    match v.track_no {
        Some(n) if n > 0 => tag.set_track(n as u32),
        _ => tag.remove_track(),
    }
    match v.disc_no {
        Some(n) if n > 0 => tag.set_disk(n as u32),
        _ => tag.remove_disk(),
    }
    set_or_remove(tag, ItemKey::Composer, &v.composer);
    if v.comment.trim().is_empty() {
        tag.remove_comment();
    } else {
        tag.set_comment(v.comment.trim().to_string());
    }
}

fn apply_art(tag: &mut Tag, art: &ArtChange, original: &Path) -> AppResult<()> {
    match art {
        ArtChange::Keep => {}
        ArtChange::Remove => {
            while tag.picture_count() > 0 {
                tag.remove_picture(0);
            }
        }
        ArtChange::Set(p) => {
            // v3.1.1（ペネトレーションテスト v3.0.2 の8・9）：先頭だけ本物の画像を埋め込まないよう、
            // アルバムアートの取り込みと同じく、上限つき（50MB）で読み、最後まで復号できるかを確かめる
            let p = Path::new(p);
            let what = p.display().to_string();
            let data = artwork::read_image_file(p)?;
            artwork::check_image(&data, &what)?;
            let mut pic = Picture::from_reader(&mut std::io::Cursor::new(&data))
                .map_err(|e| AppError::new(ErrorKind::UnsupportedFormat, format!("{what}: {e}")))?;
            pic.set_pic_type(PictureType::CoverFront);
            tag.remove_picture_type(PictureType::CoverFront);
            tag.push_picture(pic);
            let _ = original;
        }
    }
    Ok(())
}

/// 書いた値がそのまま読めるか（照合する項目は、形式が持てるものだけ）
fn verify(temp: &Path, expect: &TagValues, art: &ArtChange) -> AppResult<()> {
    let tagged = super::tags_read::open_tagged(temp).map_err(|e| tag_write_err(temp, format!("verify read: {e}")))?;
    let tag = tagged
        .primary_tag()
        .ok_or_else(|| tag_write_err(temp, "verify: tag missing"))?;
    let got = values_from_tag(tag);
    let norm = |s: &str| s.trim().to_string();
    let checks = [
        ("title", norm(&expect.title), got.title.clone()),
        ("artist", norm(&expect.artist), got.artist.clone()),
        ("album", norm(&expect.album), got.album.clone()),
        ("genre", norm(&expect.genre), got.genre.clone()),
    ];
    for (name, want, have) in checks {
        if want != have {
            return Err(tag_write_err(temp, format!("verify {name}: want {want:?}, got {have:?}")));
        }
    }
    if let Some(n) = expect.track_no.filter(|n| *n > 0) {
        if got.track_no != Some(n) {
            return Err(tag_write_err(temp, "verify track number"));
        }
    }
    match art {
        ArtChange::Remove if tag.picture_count() > 0 => {
            return Err(tag_write_err(temp, "verify: picture not removed"))
        }
        ArtChange::Set(_) if tag.picture_count() == 0 => {
            return Err(tag_write_err(temp, "verify: picture not written"))
        }
        _ => {}
    }
    // 音声部分が壊れていないか：頭の数パケットを実際に復号する
    let mut d = Decoder::open(temp).map_err(|e| tag_write_err(temp, format!("verify decode: {}", e.detail)))?;
    let mut buf = Vec::new();
    for _ in 0..4 {
        match d.decode_next(&mut buf) {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => return Err(tag_write_err(temp, format!("verify decode: {}", e.detail))),
        }
    }
    if buf.is_empty() {
        return Err(tag_write_err(temp, "verify decode: no audio"));
    }
    Ok(())
}

#[cfg(windows)]
fn replace_file(original: &Path, replacement: &Path) -> AppResult<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{ReplaceFileW, REPLACEFILE_IGNORE_MERGE_ERRORS};

    let wide = |p: &Path| -> Vec<u16> { p.as_os_str().encode_wide().chain(std::iter::once(0)).collect() };
    let a = wide(original);
    let b = wide(replacement);
    // ウイルス対策・検索の索引づけが、書いた直後のファイルを一瞬つかむことがある。
    // 共有違反・アクセス拒否のときだけ、少し待って数回やり直す（元のファイルは置換が成功するまで変わらない）
    let mut last = None;
    for attempt in 0..6 {
        let r = unsafe {
            ReplaceFileW(
                PCWSTR(a.as_ptr()),
                PCWSTR(b.as_ptr()),
                PCWSTR::null(),
                REPLACEFILE_IGNORE_MERGE_ERRORS,
                None,
                None,
            )
        };
        match r {
            Ok(()) => return Ok(()),
            Err(e) => {
                let code = e.code().0 as u32;
                let transient = code == 0x8007_0020 || code == 0x8007_0005 || code == 0x8007_0497;
                last = Some(e);
                if !transient {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(80 * (attempt + 1)));
            }
        }
    }
    let e = last.expect("error");
    let kind = if e.code().0 as u32 == 0x8007_0005 {
        ErrorKind::AccessDenied
    } else {
        ErrorKind::TagWriteFailed
    };
    Err(AppError::new(kind, format!("{}: ReplaceFileW: {e}", original.display())))
}

#[cfg(not(windows))]
fn replace_file(original: &Path, replacement: &Path) -> AppResult<()> {
    fs::rename(replacement, original).map_err(|e| AppError::from_io(&e, original))
}

/// 外から呼ぶ口
pub fn write_tags(path: &Path, values: &TagValues, art: &ArtChange) -> AppResult<()> {
    write_tags_with(path, values, art, FailPoint::None)
}

pub fn write_tags_with(path: &Path, values: &TagValues, art: &ArtChange, fail: FailPoint) -> AppResult<()> {
    if !path.exists() {
        return Err(AppError::new(ErrorKind::FileNotFound, path.display().to_string()));
    }
    let meta = fs::metadata(path).map_err(|e| AppError::from_io(&e, path))?;
    if meta.permissions().readonly() {
        return Err(AppError::new(ErrorKind::AccessDenied, format!("{}: read-only", path.display())));
    }
    let temp = TempFile(temp_path_for(path));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| write_into_temp(path, &temp.0, values, art, fail)))
        .unwrap_or_else(|_| {
            tracing::warn!("tag writer panicked: {}", path.display());
            Err(tag_write_err(path, "tag writer panicked"))
        });
    // 5. 一時ファイルを消す（成功時は ReplaceFileW が一時ファイルを消費している）
    drop(temp);
    result
}

/// 1〜4（一時ファイルに書いて確かめ、差し替える）
fn write_into_temp(path: &Path, temp: &Path, values: &TagValues, art: &ArtChange, fail: FailPoint) -> AppResult<()> {
    let temp = temp.to_path_buf();
    (|| -> AppResult<()> {
        // 1. 同じフォルダに複製
        fs::copy(path, &temp).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => AppError::from_io(&e, path),
            _ => tag_write_err(path, format!("copy: {e}")),
        })?;
        // 2. 一時ファイルにだけ書く
        let mut tagged = super::tags_read::open_tagged(&temp).map_err(|e| tag_write_err(path, format!("read: {e}")))?;
        let tag_type = tagged.primary_tag_type();
        if tagged.tag(tag_type).is_none() {
            tagged.insert_tag(Tag::new(tag_type));
        }
        // ファイルにある全部のタグ（ID3v2 と ID3v1 など）を同じ値にそろえる。アートは主のタグだけ
        let mut tags: Vec<Tag> = tagged.tags().to_vec();
        for t in tags.iter_mut() {
            sanitize_langs(t);
            apply_values(t, values);
            if t.tag_type() == tag_type {
                apply_art(t, art, path)?;
            }
        }
        // v1 の不具合：古い形式の主タグ（ID3v2.2 など）が残っていると、新しいタグを足しても
        // 読むときは古いほうが先に見つかり、書き換えが反映されなかった。
        // いったんその種類のタグを消してから書く（一時ファイルの上でだけ行う）
        // （lofty の remove_from_path はファイルを読み取り専用で開くので書けない。自分で読み書きで開いて渡す）
        for t in &tags {
            with_write_options(|o| {
                let mut f = fs::OpenOptions::new().read(true).write(true).open(&temp)?;
                t.tag_type().remove_from(&mut f, o)
            })
            .map_err(|e| tag_write_err(path, format!("remove old tag: {}", error_chain(&e))))?;
        }
        for t in &tags {
            with_write_options(|o| t.save_to_path(&temp, o))
                .map_err(|e| tag_write_err(path, format!("write: {}", error_chain(&e))))?;
        }
        if fail == FailPoint::AfterTempWrite {
            return Err(tag_write_err(path, "simulated failure after temp write"));
        }
        if fail == FailPoint::PanicAfterTempWrite {
            panic!("simulated panic after temp write");
        }
        // 3. 読み直して照合し、復号できるか確かめる
        verify(&temp, values, art)?;
        if fail == FailPoint::BeforeReplace {
            return Err(tag_write_err(path, "simulated failure before replace"));
        }
        // 4. 差し替え
        replace_file(path, &temp)
    })()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_copy(name: &str, tag: &str) -> PathBuf {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name);
        let dir = crate::test_tmp().join(format!("plp-tagw-{}-{tag}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let dst = dir.join(name);
        fs::copy(&src, &dst).unwrap();
        dst
    }

    fn values() -> TagValues {
        TagValues {
            title: "新しい曲名".into(),
            artist: "アーティスト".into(),
            album: "アルバム".into(),
            album_artist: "AA".into(),
            genre: "J-Pop".into(),
            year: Some(2024),
            track_no: Some(3),
            disc_no: Some(1),
            composer: "作曲者".into(),
            comment: "コメント".into(),
        }
    }

    fn png_file(dir: &Path) -> PathBuf {
        let p = dir.join("cover.png");
        image::RgbImage::from_pixel(64, 64, image::Rgb([10, 200, 10])).save(&p).unwrap();
        p
    }

    /// v3.1.2：HE-AAC の m4a にも書ける（書いたあとの確かめで、音声を Media Foundation で復号する）。
    /// 古い文字コードで化けたタグの曲で、編集画面が直した値をそのまま保存すると、読み直しの照合も通る
    #[test]
    fn writes_he_aac_and_repaired_values() {
        let _mf = crate::platform::windows::mf_audio::test_lock();
        let p = work_copy("sine_heaac.m4a", "heaac");
        write_tags(&p, &values(), &ArtChange::Keep).unwrap();
        let got = super::super::tags_read::read_values(&p).unwrap();
        assert_eq!(got.title, "新しい曲名");
        assert!(crate::audio::decoder::decode_all_frames(&p).is_ok());
        // 「さくら色の坂道」（Shift_JIS）を ID3v2 の Latin-1 の枠に入れた MP3。編集画面は直した値を出し、それを保存できる
        let dir = p.parent().unwrap().to_path_buf();
        let mp3 = super::super::tags_read::test_legacy_mp3(&dir, "legacy.mp3", &[(b"TIT2", b"\x82\xb3\x82\xad\x82\xe7\x90\x46\x82\xcc\x8d\xe2\x93\xb9")], None);
        let shown = super::super::tags_read::read_values(&mp3).unwrap();
        assert_eq!(shown.title, "さくら色の坂道");
        write_tags(&mp3, &shown, &ArtChange::Keep).unwrap();
        assert_eq!(super::super::tags_read::read_values(&mp3).unwrap().title, "さくら色の坂道");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_all_formats_safely() {
        for name in ["sine.mp3", "sine.flac", "sine_aaclc.m4a", "sine.ogg", "sine.opus", "sine.wav"] {
            let p = work_copy(name, "ok");
            let cover = png_file(p.parent().unwrap());
            let art = if name == "sine.wav" {
                ArtChange::Keep
            } else {
                ArtChange::Set(cover.to_string_lossy().into_owned())
            };
            write_tags(&p, &values(), &art).unwrap_or_else(|e| panic!("{name}: {e}"));
            let got = super::super::tags_read::read_values(&p).unwrap();
            assert_eq!(got.title, "新しい曲名", "{name}");
            assert_eq!(got.artist, "アーティスト", "{name}");
            assert_eq!(got.track_no, Some(3), "{name}");
            assert_eq!(got.composer, "作曲者", "{name}");
            // 一時ファイルが残っていない
            let leftovers: Vec<_> = fs::read_dir(p.parent().unwrap())
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().contains("plp-tmp"))
                .collect();
            assert!(leftovers.is_empty(), "{name}: temp left");
            // まだ鳴らせる
            assert!(crate::audio::decoder::decode_all_frames(&p).is_ok(), "{name}");
        }
    }

    #[test]
    fn simulated_failures_leave_original_untouched() {
        for fp in [FailPoint::AfterTempWrite, FailPoint::BeforeReplace] {
            let p = work_copy("sine.mp3", &format!("{fp:?}"));
            let before = fs::read(&p).unwrap();
            let r = write_tags_with(&p, &values(), &ArtChange::Keep, fp);
            assert!(matches!(r, Err(ref e) if e.kind == ErrorKind::TagWriteFailed));
            assert_eq!(fs::read(&p).unwrap(), before, "{fp:?}: original changed");
            let leftovers = fs::read_dir(p.parent().unwrap())
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().contains("plp-tmp"))
                .count();
            assert_eq!(leftovers, 0, "{fp:?}: temp left");
        }
    }

    #[test]
    fn remove_picture() {
        let p = work_copy("sine.flac", "rm");
        let cover = png_file(p.parent().unwrap());
        write_tags(&p, &values(), &ArtChange::Set(cover.to_string_lossy().into_owned())).unwrap();
        write_tags(&p, &values(), &ArtChange::Remove).unwrap();
        let tagged = lofty::read_from_path(&p).unwrap();
        assert_eq!(tagged.primary_tag().unwrap().picture_count(), 0);
    }

    #[test]
    fn missing_and_readonly() {
        let e = write_tags(Path::new("C:/no/such/file.mp3"), &values(), &ArtChange::Keep).unwrap_err();
        assert_eq!(e.kind, ErrorKind::FileNotFound);
        let p = work_copy("sine.mp3", "ro");
        let mut perm = fs::metadata(&p).unwrap().permissions();
        perm.set_readonly(true);
        fs::set_permissions(&p, perm.clone()).unwrap();
        let e = write_tags(&p, &values(), &ArtChange::Keep).unwrap_err();
        assert_eq!(e.kind, ErrorKind::AccessDenied);
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        fs::set_permissions(&p, perm).unwrap();
    }

    fn leftovers(dir: &Path) -> usize {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("plp-tmp"))
            .count()
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の3）：書き込みの途中で panic しても Err で返り、一時ファイルは残らず、元は変わらない
    #[test]
    fn panic_while_writing_is_an_error_and_cleans_up() {
        for name in ["sine.mp3", "sine.opus"] {
            let p = work_copy(name, "panic");
            let before = fs::read(&p).unwrap();
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                write_tags_with(&p, &values(), &ArtChange::Keep, FailPoint::PanicAfterTempWrite)
            }))
            .expect("write_tags panicked out");
            assert_eq!(r.unwrap_err().kind, ErrorKind::TagWriteFailed, "{name}");
            assert_eq!(fs::read(&p).unwrap(), before, "{name}: original changed");
            assert_eq!(leftovers(p.parent().unwrap()), 0, "{name}: temp left");
        }
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の3）：チャンネル数 0 の Opus へ書いても、panic せず Err。一時ファイルは残らない
    #[test]
    fn zero_channel_opus_write_fails_cleanly() {
        let p = super::super::tags_read::test_zero_channel_opus("write");
        let before = fs::read(&p).unwrap();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| write_tags(&p, &values(), &ArtChange::Keep)))
            .expect("write_tags panicked out");
        assert!(r.is_err());
        assert_eq!(fs::read(&p).unwrap(), before);
        assert_eq!(leftovers(p.parent().unwrap()), 0);
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の8）：先頭だけ本物の画像（PNG の頭＋ゴミ）は、表紙として埋め込まない
    #[test]
    fn fake_image_is_not_embedded() {
        let p = work_copy("sine.flac", "fakeart");
        let fake = p.parent().unwrap().join("fake.png");
        let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
        data.extend_from_slice(&[0x41; 4096]);
        fs::write(&fake, &data).unwrap();
        let before = fs::read(&p).unwrap();
        let e = write_tags(&p, &values(), &ArtChange::Set(fake.to_string_lossy().into_owned())).unwrap_err();
        assert_eq!(e.kind, ErrorKind::UnsupportedFormat);
        assert_eq!(fs::read(&p).unwrap(), before);
        assert_eq!(leftovers(p.parent().unwrap()), 0);
    }
}
