//! 取り込み。ファイル選択・複数選択・フォルダ追加（サブフォルダまで）・ドラッグ＆ドロップの4経路を、全部ここに流す。
//!
//! 音声ファイルはコピーしない。パスを持つだけ（原文）。
//! 開けないものは「非対応形式」として数え、最後にまとめて知らせる。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::{scanner, tags_read};
use crate::platform::windows::localpath;
use crate::audio::decoder::Decoder;
use crate::db::{self, Db};
use crate::error::ErrorKind;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub done: usize,
    pub total: usize,
    pub current: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    /// 非対応形式（拡張子が違う・中身が開けない・HE-AAC など）
    pub unsupported: Vec<String>,
    /// タグ読み込み失敗（ファイル名で取り込んだ）
    pub tag_errors: Vec<String>,
    pub not_found: Vec<String>,
    pub access_denied: Vec<String>,
    pub other_errors: Vec<String>,
    /// v3.1.1：ネットワーク上の場所（UNC・ネットワークドライブ）なので、触らずに断ったもの
    pub network: Vec<String>,
    pub elapsed_ms: u64,
}

fn file_signature(p: &Path) -> Option<(i64, i64)> {
    let m = std::fs::metadata(p).ok()?;
    let mtime = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    Some((m.len() as i64, mtime))
}

pub fn run(
    db: &Db,
    paths: &[PathBuf],
    recursive: bool,
    remember_folders: bool,
    progress: &dyn Fn(&ImportProgress),
) -> ImportSummary {
    let start = Instant::now();
    let mut s = ImportSummary::default();
    // v3.1.1（ペネトレーションテスト v3.0.2 の5）：ネットワーク上の場所は、走査（is_dir など）の前に文字とドライブの種類で断る。
    // 触ると SMB の接続が出るため
    let (paths, network): (Vec<PathBuf>, Vec<PathBuf>) = paths.iter().cloned().partition(|p| localpath::is_local(p));
    for p in &network {
        s.network.push(p.to_string_lossy().into_owned());
    }
    let scan = scanner::scan(&paths, recursive);
    for p in &scan.not_audio {
        s.unsupported.push(p.to_string_lossy().into_owned());
    }
    for (p, kind) in &scan.errors {
        let ps = p.to_string_lossy().into_owned();
        match kind {
            std::io::ErrorKind::NotFound => s.not_found.push(ps),
            std::io::ErrorKind::PermissionDenied => s.access_denied.push(ps),
            _ => s.other_errors.push(ps),
        }
    }
    if remember_folders {
        let conn = db.writer();
        for f in &scan.folders {
            let _ = conn.execute(
                "INSERT OR IGNORE INTO library_folders (path, recursive, added_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![f.to_string_lossy(), recursive as i64, db::now_ms()],
            );
        }
    }
    let total = scan.files.len();
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let mut batch: Vec<crate::db::tracks::TrackInfo> = Vec::with_capacity(200);

    let flush = |batch: &mut Vec<crate::db::tracks::TrackInfo>, s: &mut ImportSummary| {
        if batch.is_empty() {
            return;
        }
        let mut conn = db.writer();
        match conn.transaction() {
            Ok(tx) => {
                for info in batch.iter() {
                    match db::tracks::upsert(&tx, info) {
                        Ok((_, true)) => s.added += 1,
                        Ok((_, false)) => s.updated += 1,
                        Err(e) => s.other_errors.push(format!("{}: {}", info.path, e.detail)),
                    }
                }
                if let Err(e) = tx.commit() {
                    s.other_errors.push(format!("database: {e}"));
                }
            }
            Err(e) => s.other_errors.push(format!("database: {e}")),
        }
        batch.clear();
    };

    for (i, path) in scan.files.iter().enumerate() {
        if last_emit.elapsed() > Duration::from_millis(100) {
            progress(&ImportProgress {
                done: i,
                total,
                current: path.to_string_lossy().into_owned(),
            });
            last_emit = Instant::now();
        }
        let ps = path.to_string_lossy().into_owned();
        // 変わっていなければ読み直さない
        if let Some((size, mtime)) = file_signature(path) {
            let conn = db.reader();
            if db::tracks::unchanged(&conn, &ps, size, mtime).unwrap_or(false) {
                s.unchanged += 1;
                continue;
            }
        }
        // 中身を Symphonia で確かめ（開けない・HE-AAC などは非対応形式）、タグを読む。
        // v3.1.1（ペネトレーションテスト v3.0.2 の2）：細工したファイルで部品が panic しても、その1曲を
        // 非対応形式として数えて次へ進む（取り込みのスレッドを止めない）
        let probed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Decoder::open(path).map(|d| (d.duration_ms, tags_read::read_track(path, &db.thumbs_dir)))
        }));
        let (decoder_duration, read) = match probed {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                match e.kind {
                    ErrorKind::FileNotFound => s.not_found.push(ps),
                    ErrorKind::AccessDenied => s.access_denied.push(ps),
                    _ => s.unsupported.push(ps),
                }
                continue;
            }
            Err(_) => {
                tracing::warn!("import: reader panicked: {ps}");
                s.unsupported.push(ps);
                continue;
            }
        };
        match read {
            Ok(mut info) => {
                if info.duration_ms <= 0 {
                    info.duration_ms = decoder_duration as i64;
                }
                if info.tag_error {
                    s.tag_errors.push(ps);
                }
                batch.push(info);
                if batch.len() >= 200 {
                    flush(&mut batch, &mut s);
                }
            }
            Err(e) => match e.kind {
                ErrorKind::FileNotFound => s.not_found.push(ps),
                ErrorKind::AccessDenied => s.access_denied.push(ps),
                _ => s.other_errors.push(ps),
            },
        }
    }
    flush(&mut batch, &mut s);
    progress(&ImportProgress {
        done: total,
        total,
        current: String::new(),
    });
    s.elapsed_ms = start.elapsed().as_millis() as u64;
    s
}

/// 取り込み済みの曲が、まだそこにあるかを確かめる（無ければ「見つからない印」）
pub fn check_missing(db: &Db) -> usize {
    let all = db::tracks::all_paths(db).unwrap_or_default();
    let mut missing = 0;
    for (id, p) in all {
        // v3.1.1：ネットワーク上の場所の曲（3.1.0 までに取り込んだもの）には触らない（SMB の接続を出さない）
        if !localpath::is_local(Path::new(&p)) {
            continue;
        }
        let gone = !Path::new(&p).exists();
        if gone {
            missing += 1;
        }
        let _ = db::tracks::set_missing(db, id, gone);
    }
    missing
}

pub fn library_folders(db: &Db) -> Vec<(String, bool)> {
    let conn = db.reader();
    let Ok(mut stmt) = conn.prepare("SELECT path, recursive FROM library_folders ORDER BY id") else {
        return vec![];
    };
    stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0)))
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

pub fn remove_library_folder(db: &Db, path: &str) {
    let _ = db
        .writer()
        .execute("DELETE FROM library_folders WHERE path = ?1", [path]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_fixtures_and_counts_problems() {
        let db = Db::open_memory("import_fixtures").unwrap();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures");
        let s = run(&db, &[dir.clone()], true, true, &|_| {});
        // 8つの正常なファイル（mono_22k.wav を含めて9）
        assert!(s.added >= 9, "{s:?}");
        assert!(s.unsupported.iter().any(|p| p.ends_with("broken.mp3")), "{s:?}");
        // 2回目は読み直さない
        let s2 = run(&db, &[dir], true, true, &|_| {});
        assert_eq!(s2.added, 0);
        assert!(s2.unchanged >= 9);
        assert_eq!(library_folders(&db).len(), 1);
        // 音声ファイルはコピーしていない：パスはフィクスチャそのもの
        let all = db::tracks::all_paths(&db).unwrap();
        assert!(all.iter().all(|(_, p)| p.contains("fixtures")));
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の2）：0 Hz の WAV は非対応形式として数え、取り込みは最後まで進む
    #[test]
    fn zero_rate_wav_does_not_stop_the_import() {
        let dir = crate::test_tmp().join(format!("plp-import-zero-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        crate::audio::decoder::test_zero_rate_wav(&dir);
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures");
        std::fs::copy(src.join("sine.flac"), dir.join("z_after.flac")).unwrap();
        let db = Db::open_memory("import_zero_rate").unwrap();
        let s = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&db, &[dir.clone()], true, false, &|_| {})))
            .expect("import panicked");
        assert!(s.unsupported.iter().any(|p| p.ends_with("zero_rate.wav")), "{s:?}");
        // 後ろの曲も取り込まれている（途中で止まっていない）
        assert_eq!(s.added, 1, "{s:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
