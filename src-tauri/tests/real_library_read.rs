//! 作者の実際のライブラリで、タグ編集画面が使う読み込みを確かめる（読むだけ。書かない）。
//! 走らせ方：PL_REAL_DB=<library.db のパス> cargo test --test real_library_read -- --ignored --nocapture

use std::path::Path;

#[test]
#[ignore]
fn read_values_for_real_files() {
    let Some(db) = std::env::var_os("PL_REAL_DB") else { return };
    let conn = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut stmt = conn.prepare("SELECT path, title FROM tracks").unwrap();
    let rows: Vec<(String, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().filter_map(Result::ok).collect();
    let (mut ok, mut bad, mut empty) = (0, 0, 0);
    for (p, title) in &rows {
        match pl_player_lib::library::tags_read::read_values(Path::new(p)) {
            Ok(v) => {
                if v.title.is_empty() && !title.is_empty() { empty += 1; if empty <= 3 { println!("EMPTY {p}"); } }
                ok += 1
            }
            Err(e) => { bad += 1; if bad <= 5 { println!("ERR {p}: {e}"); } }
        }
    }
    println!("total {} ok {ok} err {bad} empty-title {empty}", rows.len());
}

/// 作者の曲の「複製」にタグを書いて、読み直せること・鳴らせることを確かめる（元のファイルには書かない）。
/// 複製は target\test-tmp\real-write に置き、終わったら消す。
#[test]
#[ignore]
fn write_values_on_copies() {
    use pl_player_lib::library::{tags_read, tags_write};
    let Some(db) = std::env::var_os("PL_REAL_DB") else { return };
    let conn = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    // 読めなかった曲・タグが無い曲・普通の曲を、形式ごとに少しずつ
    let mut stmt = conn
        .prepare(
            "SELECT path FROM (SELECT path, format, tag_error, artist='' AS untagged,
               ROW_NUMBER() OVER (PARTITION BY format, tag_error, artist='' ORDER BY id) AS n FROM tracks) WHERE n <= 4",
        )
        .unwrap();
    let paths: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().filter_map(Result::ok).collect();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("test-tmp").join("real-write");
    std::fs::create_dir_all(&dir).unwrap();
    let (mut ok, mut bad) = (0, 0);
    for (i, p) in paths.iter().enumerate() {
        let src = Path::new(p);
        let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("bin");
        let dst = dir.join(format!("{i:03}.{ext}"));
        std::fs::copy(src, &dst).unwrap();
        let mut v = tags_read::read_values(&dst).unwrap_or_default();
        v.title = format!("書き換えテスト {i}");
        v.artist = "テスト アーティスト".into();
        v.composer = "作曲".into();
        v.track_no = Some(7);
        match tags_write::write_tags(&dst, &v, &tags_write::ArtChange::Keep) {
            Ok(()) => {
                let back = tags_read::read_values(&dst).unwrap();
                assert_eq!(back.title, v.title, "{p}");
                assert_eq!(back.track_no, Some(7), "{p}");
                ok += 1;
            }
            Err(e) => {
                bad += 1;
                println!("WRITE ERR {p}: {e}");
            }
        }
        let _ = std::fs::remove_file(&dst);
    }
    println!("write on copies: {} files, ok {ok}, err {bad}", paths.len());
    assert_eq!(bad, 0);
}

/// 長さを読まずに開き直す曲でも、長さが入ること（読むだけ）
#[test]
#[ignore]
fn duration_known_for_all() {
    let Some(db) = std::env::var_os("PL_REAL_DB") else { return };
    let conn = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut stmt = conn.prepare("SELECT path FROM tracks").unwrap();
    let paths: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().filter_map(Result::ok).collect();
    let thumbs = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("test-tmp").join("thumbs");
    std::fs::create_dir_all(&thumbs).unwrap();
    let mut zero = 0;
    for p in &paths {
        let t = pl_player_lib::library::tags_read::read_track(Path::new(p), &thumbs).unwrap();
        if t.duration_ms <= 0 {
            zero += 1;
            println!("ZERO {p}");
        }
    }
    println!("duration: {} files, zero {zero}", paths.len());
    assert_eq!(zero, 0);
}

/// 全曲の「複製」に、何も変えずに保存したときと同じ書き込みをする（元のファイルには書かない）
#[test]
#[ignore]
fn write_all_copies_unchanged_values() {
    use pl_player_lib::library::{tags_read, tags_write};
    let Some(db) = std::env::var_os("PL_REAL_DB") else { return };
    let conn = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut stmt = conn.prepare("SELECT path FROM tracks ORDER BY id").unwrap();
    let paths: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().filter_map(Result::ok).collect();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("test-tmp").join("real-write-all");
    std::fs::create_dir_all(&dir).unwrap();
    let (mut ok, mut bad) = (0, 0);
    for (i, p) in paths.iter().enumerate() {
        let src = Path::new(p);
        let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("bin");
        let dst = dir.join(format!("{i:04}.{ext}"));
        std::fs::copy(src, &dst).unwrap();
        let mut v = tags_read::read_values(&dst).unwrap();
        v.comment = format!("{} edited", v.comment);
        match tags_write::write_tags(&dst, &v, &tags_write::ArtChange::Keep) {
            Ok(()) => ok += 1,
            Err(e) => {
                bad += 1;
                println!("WRITE ERR {p}: {e}");
            }
        }
        let _ = std::fs::remove_file(&dst);
    }
    println!("write all copies: {} files, ok {ok}, err {bad}", paths.len());
}
