//! 受け渡しフォルダ（外部ツール → アプリ）の取り込み（仕様 3章）。
//!
//! ```text
//! <データフォルダ>\lyrics\
//! ├─ inbox\       外部ツールが書く
//! ├─ processed\   取り込み済みを移す
//! └─ rejected\    取り込めなかったものを移す
//! ```
//!
//! - 名前は `{曲ID}.lrc`（時刻つき）か `{曲ID}.txt`（時刻なし）だけ。`^[0-9]+\.(lrc|txt)$` に合わないものは読まずに残す
//! - UTF-8（BOM は外す）。改行は LF にそろえる。1ファイル 1MiB まで
//! - 取り込みはボタンを押したときだけ（フォルダの監視はしない）
//! - 保存するのは external だけ。manual には触らない

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::lrc;
use crate::db::lyrics::{self as store, StoredKind};
use crate::db::Db;
use crate::error::{AppError, AppResult, ErrorKind};

/// 1ファイルの上限（1MiB）。解析に渡す原文の上限と同じ値（lrc::MAX_TEXT_BYTES）
pub const MAX_BYTES: u64 = lrc::MAX_TEXT_BYTES as u64;

/// 取り込みの結果（件数はファイルの数）
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxReport {
    /// 取り込んで、表示に使われるもの
    pub imported: usize,
    /// 取り込んだが、その曲に手動の歌詞があるので表示は変わらないもの
    pub manual_kept: usize,
    /// 曲 ID がライブラリに無い（rejected へ）
    pub unknown_track: usize,
    /// 同じ曲 ID に .lrc と .txt の両方がある（両方 rejected へ）
    pub duplicate: usize,
    /// UTF-8 として読めない（rejected へ）
    pub encoding: usize,
    /// 空・空白だけ・1MiB 超（rejected へ）
    pub empty_or_too_large: usize,
    /// DB への保存やファイルの読み込みに失敗し、inbox に残したもの（もう一度押せば取り込み直す）
    pub kept_in_inbox: usize,
    /// 名前が取り決めに合わず、読まずに残したもの（書きかけの一時ファイルなど）。
    /// v3.1.1：ファイルでないもの（`N.lrc` という名前のフォルダ・ジャンクション・シンボリックリンク）も数える
    pub ignored: usize,
    /// v3.1.1：取り込んだが、行が多すぎて先頭の lrc::MAX_LINES 行だけを表示するもの
    pub lines_truncated: usize,
    /// 外部の歌詞が変わった曲（表示中の曲なら画面が読み直す）
    pub track_ids: Vec<i64>,
}

pub struct Dirs {
    pub inbox: PathBuf,
    pub processed: PathBuf,
    pub rejected: PathBuf,
}

/// 3つのフォルダを用意する（無ければ作る）
pub fn ensure_dirs(lyrics_dir: &Path) -> AppResult<Dirs> {
    let d = Dirs {
        inbox: lyrics_dir.join("inbox"),
        processed: lyrics_dir.join("processed"),
        rejected: lyrics_dir.join("rejected"),
    };
    for p in [&d.inbox, &d.processed, &d.rejected] {
        fs::create_dir_all(p).map_err(|e| AppError::from_io(&e, p))?;
    }
    Ok(d)
}

/// `^[0-9]+\.(lrc|txt)$` に合えば (曲IDの数字, 拡張子)
pub fn match_name(name: &str) -> Option<(&str, &str)> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || !stem.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if ext != "lrc" && ext != "txt" {
        return None;
    }
    Some((stem, ext))
}

/// 読めなかった理由
pub enum ReadFail {
    /// 開いてみたら、ふつうのファイルではなかった（フォルダ・リンクなど。判定のあとに差し替えられたものを含む）
    NotRegular,
    App(AppError),
}

impl From<AppError> for ReadFail {
    fn from(e: AppError) -> Self {
        ReadFail::App(e)
    }
}

/// 開いたハンドルで、ふつうのファイルかを確かめる（v3.1.1。ペネトレーションテスト v3.0.2 の12）。
/// Windows では、リンクをたどらずにリンクそのものを開く（FILE_FLAG_OPEN_REPARSE_POINT）。
/// 名前で種類を確かめたあとにリンクへ差し替えられても、リンクの先（データフォルダの外）は読まない
fn open_regular(path: &Path) -> Result<fs::File, ReadFail> {
    let mut opts = fs::OpenOptions::new();
    opts.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        opts.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let f = opts.open(path).map_err(|e| AppError::from_io(&e, path))?;
    let meta = f.metadata().map_err(|e| AppError::from_io(&e, path))?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(ReadFail::NotRegular);
        }
    }
    if !meta.is_file() {
        return Err(ReadFail::NotRegular);
    }
    Ok(f)
}

/// 上限＋1 バイトまでしか読まない（大きさを確かめたあとに伸びたファイルも、上限より先は読まない）
pub fn read_limited(r: impl std::io::Read, what: &Path) -> AppResult<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    r.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| AppError::from_io(&e, what))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(AppError::new(ErrorKind::LyricsFormatInvalid, format!("{}: too large", what.display())));
    }
    Ok(bytes)
}

/// 1ファイルを読んで、保存する原文（BOM を外し、改行を LF にそろえたもの）を返す
pub fn read_lyrics_file(path: &Path) -> AppResult<String> {
    read_inbox_file(path).map_err(|e| match e {
        ReadFail::App(e) => e,
        ReadFail::NotRegular => AppError::other(format!("{}: not a regular file", path.display())),
    })
}

fn read_inbox_file(path: &Path) -> Result<String, ReadFail> {
    let f = open_regular(path)?;
    let bytes = read_limited(f, path)?;
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&bytes);
    let text = std::str::from_utf8(body)
        .map_err(|e| AppError::new(ErrorKind::LyricsEncodingUnsupported, format!("{}: {e}", path.display())))?;
    let text = lrc::normalize(text);
    if !lrc::parse(&text).has_content() {
        return Err(AppError::new(ErrorKind::LyricsFormatInvalid, format!("{}: empty", path.display())).into());
    }
    Ok(text)
}

/// 同じ名前があれば `-2` `-3` … を付けて、空いている名前を返す
fn free_name(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem}-{n}.{ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

fn move_file(from: &Path, to: PathBuf) {
    if let Err(e) = fs::rename(from, &to) {
        tracing::warn!("lyrics inbox: move {} -> {} failed: {e}", from.display(), to.display());
    }
}

fn reject(from: &Path, rejected: &Path, stamp: &str) {
    let name = from.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut to = rejected.join(&name);
    if to.exists() {
        let (stem, ext) = name.rsplit_once('.').unwrap_or((&name, ""));
        to = free_name(rejected, &format!("{stem}-{stamp}"), ext);
    }
    move_file(from, to);
}

/// 受け渡しフォルダを取り込む
pub fn import(db: &Db, lyrics_dir: &Path) -> AppResult<InboxReport> {
    let dirs = ensure_dirs(lyrics_dir)?;
    let mut report = InboxReport::default();
    let stamp = store::local_stamp(db).unwrap_or_else(|_| "00000000-000000".into());

    // 曲 ID ごとにまとめる（.lrc と .txt の両方があるかを見るため）。
    // 数字の文字列のままでまとめていたので、`02.lrc` と `2.txt` が別の曲として両方取り込まれていた。
    // 数に直してからまとめる。i64 に収まらない数字（どの曲 ID にも当たらない）は別に持つ
    let mut by_id: BTreeMap<i64, Vec<(PathBuf, String)>> = BTreeMap::new();
    let mut out_of_range: Vec<PathBuf> = Vec::new();
    let entries = fs::read_dir(&dirs.inbox).map_err(|e| AppError::from_io(&e, &dirs.inbox))?;
    for entry in entries.flatten() {
        let path = entry.path();
        // v3.1.1：ファイルでないもの（フォルダ・ジャンクション・リンク）は読まずに残し、数えて知らせる
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            report.ignored += 1;
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        match match_name(&name) {
            Some((digits, ext)) => match digits.parse::<i64>() {
                Ok(id) => by_id.entry(id).or_default().push((path, ext.to_string())),
                Err(_) => out_of_range.push(path),
            },
            None => report.ignored += 1,
        }
    }

    for path in &out_of_range {
        reject(path, &dirs.rejected, &stamp);
        report.unknown_track += 1;
    }

    for (track_id, files) in by_id {
        if files.len() > 1 {
            for (p, _) in &files {
                reject(p, &dirs.rejected, &stamp);
            }
            report.duplicate += files.len();
            continue;
        }
        let (path, ext) = &files[0];
        match store::track_exists(db, track_id) {
            Ok(true) => {}
            Ok(false) => {
                reject(path, &dirs.rejected, &stamp);
                report.unknown_track += 1;
                continue;
            }
            Err(e) => {
                tracing::warn!("lyrics inbox: {}: {}", path.display(), e.detail);
                report.kept_in_inbox += 1;
                continue;
            }
        }
        let text = match read_inbox_file(path) {
            Ok(t) => t,
            // 名前を見たあとにフォルダやリンクへ差し替えられたもの：読まずに残す
            Err(ReadFail::NotRegular) => {
                tracing::warn!("lyrics inbox: not a regular file: {}", path.display());
                report.ignored += 1;
                continue;
            }
            Err(ReadFail::App(e)) => {
                match e.kind {
                    ErrorKind::LyricsEncodingUnsupported => {
                        reject(path, &dirs.rejected, &stamp);
                        report.encoding += 1;
                    }
                    ErrorKind::LyricsFormatInvalid => {
                        reject(path, &dirs.rejected, &stamp);
                        report.empty_or_too_large += 1;
                    }
                    // 開けない（外部ツールが書いている途中など）：残して、次に押したときに取り込む
                    _ => {
                        tracing::warn!("lyrics inbox: {}", e.detail);
                        report.kept_in_inbox += 1;
                    }
                }
                continue;
            }
        };
        // 拡張子は目安。中身に有効な時刻タグがあれば時刻つき
        let parsed = lrc::parse(&text);
        let format = parsed.format;
        let truncated = parsed.warnings.iter().any(|w| w == lrc::warn::LINES_TRUNCATED);
        let saved = store::exists(db, track_id, StoredKind::Manual).and_then(|has_manual| {
            store::upsert(&db.writer(), track_id, StoredKind::External, &text, format.as_str())?;
            Ok(has_manual)
        });
        match saved {
            Ok(has_manual) => {
                move_file(path, free_name(&dirs.processed, &format!("{track_id}-{stamp}"), ext));
                if has_manual {
                    report.manual_kept += 1;
                } else {
                    report.imported += 1;
                }
                if truncated {
                    report.lines_truncated += 1;
                }
                report.track_ids.push(track_id);
            }
            Err(e) => {
                // DB への保存に失敗：ファイルは inbox に残す
                tracing::warn!("lyrics inbox: save failed {}: {}", path.display(), e.detail);
                report.kept_in_inbox += 1;
            }
        }
    }
    tracing::info!("lyrics inbox: {report:?}");
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tracks::{self, TrackInfo};

    fn setup(name: &str, n: usize) -> (Db, PathBuf, Vec<i64>) {
        let db = Db::open_memory(&format!("lyrics_inbox_{name}")).unwrap();
        let ids = (0..n)
            .map(|i| {
                tracks::upsert(&db.writer(), &TrackInfo { path: format!("C:/m/{name}/{i}.mp3"), ..Default::default() })
                    .unwrap()
                    .0
            })
            .collect();
        let dir = crate::test_tmp().join(format!("plp-inbox-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        ensure_dirs(&dir).unwrap();
        (db, dir, ids)
    }

    fn put(dir: &Path, name: &str, bytes: &[u8]) {
        fs::write(dir.join("inbox").join(name), bytes).unwrap();
    }

    fn names(dir: &Path, sub: &str) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir.join(sub))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn name_rule() {
        assert_eq!(match_name("12.lrc"), Some(("12", "lrc")));
        assert_eq!(match_name("007.txt"), Some(("007", "txt")));
        for bad in ["12.LRC", "12.lrc.tmp", "a12.lrc", ".lrc", "12.srt", "12", "1 2.lrc", "-1.lrc"] {
            assert_eq!(match_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn imports_and_moves_to_processed() {
        let (db, dir, ids) = setup("ok", 2);
        // BOM・CRLF つきの .lrc
        put(&dir, &format!("{}.lrc", ids[0]), "\u{feff}[00:01.00]一\r\n[00:02.00]二\r\n".as_bytes());
        // 拡張子は .txt でも、中身に時刻タグがあれば時刻つき
        put(&dir, &format!("{}.txt", ids[1]), b"[00:03.00]three");
        let r = import(&db, &dir).unwrap();
        assert_eq!(r.imported, 2);
        assert_eq!(r.track_ids, ids);
        assert!(names(&dir, "inbox").is_empty());
        let done = names(&dir, "processed");
        assert_eq!(done.len(), 2);
        for n in &done {
            // {曲ID}-{yyyyMMdd-HHmmss}.{拡張子}
            let (stem, ext) = n.rsplit_once('.').unwrap();
            let (id, ts) = stem.split_once('-').unwrap();
            assert!(ids.contains(&id.parse::<i64>().unwrap()), "{n}");
            assert!(ext == "lrc" || ext == "txt");
            assert_eq!(ts.len(), 15, "{n}");
            assert_eq!(&ts[8..9], "-");
        }
        let a = store::get(&db, ids[0], StoredKind::External).unwrap().unwrap();
        assert_eq!(a.raw_text, "[00:01.00]一\n[00:02.00]二\n");
        assert_eq!(a.format, "timed");
        assert_eq!(store::get(&db, ids[1], StoredKind::External).unwrap().unwrap().format, "timed");
        // 2回目：inbox は空なので何もしない
        assert_eq!(import(&db, &dir).unwrap(), InboxReport::default());
    }

    #[test]
    fn rejects_each_case() {
        let (db, dir, ids) = setup("reject", 6);
        let big = vec![b'a'; MAX_BYTES as usize + 1];
        put(&dir, "999999.lrc", b"[00:01.00]no track"); // 曲 ID なし
        put(&dir, &format!("{}.lrc", ids[0]), b"[00:01.00]a"); // 重複
        put(&dir, &format!("{}.txt", ids[0]), b"a");
        put(&dir, &format!("{}.lrc", ids[1]), &[0xFF, 0xFE, b'a', 0x00, 0xC3]); // UTF-8 でない
        put(&dir, &format!("{}.txt", ids[2]), b""); // 空
        put(&dir, &format!("{}.txt", ids[3]), b"  \r\n\t \n"); // 空白だけ
        put(&dir, &format!("{}.txt", ids[4]), &big); // 1MiB 超
        put(&dir, &format!("{}.lrc", ids[5]), b"[00:01.00]ok");
        // 名前が合わない：読まずに残す（Windows は大文字小文字を区別しないので、同じ曲 ID の .LRC は置けない。別の番号で）
        put(&dir, &format!("{}.lrc.tmp", ids[5]), b"partial");
        put(&dir, "424242.LRC", b"upper");
        let r = import(&db, &dir).unwrap();
        assert_eq!(r.unknown_track, 1);
        assert_eq!(r.duplicate, 2);
        assert_eq!(r.encoding, 1);
        assert_eq!(r.empty_or_too_large, 3);
        assert_eq!(r.imported, 1);
        assert_eq!(r.ignored, 2);
        assert_eq!(r.kept_in_inbox, 0);
        assert_eq!(r.track_ids, vec![ids[5]]);
        assert_eq!(names(&dir, "rejected").len(), 7);
        assert_eq!(names(&dir, "inbox"), {
            let mut v = vec!["424242.LRC".to_string(), format!("{}.lrc.tmp", ids[5])];
            v.sort();
            v
        });
        for id in &ids[..5] {
            assert!(store::get(&db, *id, StoredKind::External).unwrap().is_none());
        }
        // rejected に同じ名前があっても上書きしない
        put(&dir, "999999.lrc", b"again");
        import(&db, &dir).unwrap();
        assert_eq!(names(&dir, "rejected").len(), 8);
    }

    #[test]
    fn manual_is_not_touched_and_counted_separately() {
        let (db, dir, ids) = setup("manual", 1);
        let id = ids[0];
        store::upsert(&db.writer(), id, StoredKind::Manual, "手動", "plain").unwrap();
        put(&dir, &format!("{id}.txt"), "外部".as_bytes());
        let r = import(&db, &dir).unwrap();
        assert_eq!(r.manual_kept, 1);
        assert_eq!(r.imported, 0);
        assert_eq!(r.track_ids, vec![id]);
        assert_eq!(store::get(&db, id, StoredKind::Manual).unwrap().unwrap().raw_text, "手動");
        assert_eq!(store::get(&db, id, StoredKind::External).unwrap().unwrap().raw_text, "外部");
        assert_eq!(names(&dir, "processed").len(), 1);
    }

    /// 先頭の0が違うだけの名前（`02.lrc` と `2.txt`）は同じ曲 ID。両方 rejected へ
    #[test]
    fn leading_zeros_are_the_same_track() {
        let (db, dir, ids) = setup("zeros", 2);
        let id = ids[1];
        put(&dir, &format!("0{id}.lrc"), b"[00:01.00]a");
        put(&dir, &format!("{id}.txt"), b"b");
        // 先頭の0だけの1件は、その曲として取り込む（processed の名前は数に直した ID）
        put(&dir, &format!("000{}.txt", ids[0]), b"c");
        // i64 に収まらない数字は曲 ID なし
        put(&dir, "99999999999999999999999.lrc", b"d");
        let r = import(&db, &dir).unwrap();
        assert_eq!(r.duplicate, 2);
        assert_eq!(r.imported, 1);
        assert_eq!(r.unknown_track, 1);
        assert_eq!(r.track_ids, vec![ids[0]]);
        assert!(store::get(&db, id, StoredKind::External).unwrap().is_none());
        assert_eq!(store::get(&db, ids[0], StoredKind::External).unwrap().unwrap().raw_text, "c");
        assert!(names(&dir, "inbox").is_empty());
        assert_eq!(names(&dir, "rejected").len(), 3);
        let done = names(&dir, "processed");
        assert_eq!(done.len(), 1);
        assert!(done[0].starts_with(&format!("{}-", ids[0])), "{done:?}");
    }

    #[test]
    fn db_failure_keeps_file_in_inbox() {
        let (db, dir, ids) = setup("dbfail", 1);
        db.writer()
            .execute_batch(
                "CREATE TRIGGER lyrics_fail BEFORE INSERT ON lyrics_sources BEGIN SELECT RAISE(ABORT, 'test'); END;",
            )
            .unwrap();
        let name = format!("{}.lrc", ids[0]);
        put(&dir, &name, b"[00:01.00]a");
        let r = import(&db, &dir).unwrap();
        assert_eq!(r.kept_in_inbox, 1);
        assert_eq!(r.imported, 0);
        assert!(r.track_ids.is_empty());
        assert_eq!(names(&dir, "inbox"), vec![name]);
        assert!(names(&dir, "processed").is_empty());
        assert!(names(&dir, "rejected").is_empty());
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の10）：取り込んだ歌詞の行が多すぎれば、件数で知らせる
    #[test]
    fn too_many_lines_are_counted() {
        let (db, dir, ids) = setup("cap", 1);
        let many: String = (0..6000).map(|i| format!("行{i}\n")).collect();
        put(&dir, &format!("{}.txt", ids[0]), many.as_bytes());
        let r = import(&db, &dir).unwrap();
        assert_eq!((r.imported, r.lines_truncated), (1, 1));
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の12）：
    /// - 名前が `N.lrc` のフォルダとジャンクションは、読まずに残して「読まなかったもの」に数える
    /// - シンボリックリンク（作れる環境のとき）は、開いたハンドルで種類を見て、リンクの先を読まない
    /// - 読むのは上限＋1 バイトまで（大きさを確かめたあとに伸びても、上限より先は読まない）
    #[test]
    fn folders_links_and_growing_files() {
        let (db, dir, ids) = setup("links", 3);
        let inbox = dir.join("inbox");
        fs::create_dir_all(inbox.join(format!("{}.lrc", ids[0]))).unwrap();
        // データフォルダの外に見立てた場所
        let outside = dir.join("outside");
        fs::create_dir_all(&outside).unwrap();
        let secret = outside.join("secret.lrc");
        fs::write(&secret, b"[00:01.00]outside").unwrap();
        // ジャンクション（管理者の権限は要らない）。作れなければ、その確かめだけ飛ばす
        let junction = inbox.join(format!("{}.lrc", ids[1]));
        let made_junction = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&outside)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        // ファイルのシンボリックリンク（開発者モードか権限が要る）。作れなければ飛ばす
        let link = inbox.join(format!("{}.txt", ids[2]));
        #[cfg(windows)]
        let made_link = std::os::windows::fs::symlink_file(&secret, &link).is_ok();
        #[cfg(not(windows))]
        let made_link = false;
        println!("junction made: {made_junction}, file symlink made: {made_link}");
        let r = import(&db, &dir).unwrap();
        let expected_ignored = 1 + made_junction as usize + made_link as usize;
        assert_eq!(r.ignored, expected_ignored, "{r:?}");
        assert_eq!(r.imported, 0, "{r:?}");
        for id in &ids {
            assert!(store::get(&db, *id, StoredKind::External).unwrap().is_none());
        }
        if made_link {
            // 名前を見たあとにリンクへ差し替えられた場合と同じ：開いたハンドルで断る
            assert!(read_lyrics_file(&link).unwrap_err().detail.contains("not a regular file"));
            let _ = fs::remove_file(&link);
        }
        if made_junction {
            // 名前で種類を見たあとに差し替えられた場合と同じ：ジャンクションそのものを開いても、ふつうのファイルとしては読まない
            let e = read_lyrics_file(&junction).unwrap_err();
            println!("junction opened directly: {:?} {}", e.kind, e.detail);
            assert!(e.kind != ErrorKind::LyricsFormatInvalid && e.kind != ErrorKind::LyricsEncodingUnsupported);
            // ジャンクションだけを外す（中の外部のファイルは消さない）
            let _ = fs::remove_dir(&junction);
        }
        assert!(secret.exists());
        // 終わりの無い入力でも、上限＋1 バイトで止まる
        let e = read_limited(std::io::repeat(b'a'), Path::new("endless")).unwrap_err();
        assert_eq!(e.kind, ErrorKind::LyricsFormatInvalid);
        let _ = fs::remove_dir_all(&dir);
    }
}
