//! SQLite（rusqlite bundled）。WAL モードで、読み込み中でも書き込みで画面が止まらないようにする。
//!
//! 接続は2本：書き込み用と読み取り用。どちらも Mutex で守る。

pub mod history;
pub mod lyrics;
pub mod playlists;
pub mod queue;
pub mod search;
pub mod settings;
pub mod state;
pub mod tracks;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

use crate::error::{AppError, AppResult, ErrorKind};

const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("migrations/001_init.sql")),
    (2, include_str!("migrations/002_lyrics.sql")),
    (3, include_str!("migrations/003_lyrics_fetch.sql")),
    (4, include_str!("migrations/004_legacy_text.sql")),
    (5, include_str!("migrations/005_tag_manual_edits.sql")),
    (6, include_str!("migrations/006_play_history.sql")),
];

/// 移行 4（v3.1.2）：SQL のあとに、同じトランザクションの中で走らせる Rust の処理（古い文字コードの値を直す）
const LEGACY_TEXT_VERSION: i64 = 4;

pub struct Db {
    write: Mutex<Connection>,
    read: Mutex<Connection>,
    /// 埋め込みアートの縮小版を置くフォルダ（`tracks.art_key` からパスを作るのに使う）
    pub thumbs_dir: PathBuf,
}

fn open_conn(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(path)
        .map_err(|e| AppError::new(ErrorKind::Database, format!("{}: {e}", path.display())))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;
         PRAGMA temp_store=MEMORY;",
    )?;
    Ok(conn)
}

impl Db {
    pub fn open(path: &Path, thumbs_dir: PathBuf) -> AppResult<Self> {
        let write = open_conn(path)?;
        migrate(&write)?;
        let read = open_conn(path)?;
        Ok(Self {
            write: Mutex::new(write),
            read: Mutex::new(read),
            thumbs_dir,
        })
    }

    /// テスト用：メモリ上の DB（読み書き同じ接続を使えないので、共有キャッシュの名前付きメモリ DB にする）
    #[cfg(test)]
    pub fn open_memory(name: &str) -> AppResult<Self> {
        let uri = format!("file:{name}?mode=memory&cache=shared");
        let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
            | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
            | rusqlite::OpenFlags::SQLITE_OPEN_URI;
        let write = Connection::open_with_flags(&uri, flags)?;
        write.execute_batch("PRAGMA foreign_keys=ON;")?;
        migrate(&write)?;
        let read = Connection::open_with_flags(&uri, flags)?;
        read.execute_batch("PRAGMA foreign_keys=ON;")?;
        Ok(Self {
            write: Mutex::new(write),
            read: Mutex::new(read),
            thumbs_dir: crate::test_tmp(),
        })
    }

    pub fn writer(&self) -> MutexGuard<'_, Connection> {
        self.write.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn reader(&self) -> MutexGuard<'_, Connection> {
        self.read.lock().unwrap_or_else(|p| p.into_inner())
    }
}

fn migrate(conn: &Connection) -> AppResult<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (version, sql) in MIGRATIONS {
        if *version > current {
            if *version == LEGACY_TEXT_VERSION {
                // SQL と Rust の処理と版の書き換えを1つのトランザクションにする（途中で失敗したら戻し、次の起動でやり直す）
                conn.execute_batch(&format!("BEGIN;\n{sql}"))?;
                let done = tracks::repair_legacy_text(conn)
                    .and_then(|_| Ok(conn.execute_batch(&format!("PRAGMA user_version = {version};\nCOMMIT;"))?));
                if let Err(e) = done {
                    let _ = conn.execute_batch("ROLLBACK;");
                    return Err(e);
                }
                continue;
            }
            conn.execute_batch(&format!("BEGIN;\n{sql}\nPRAGMA user_version = {version};\nCOMMIT;"))?;
        }
    }
    Ok(())
}

/// テスト用：移行を `upto` の版まで当てた接続を返す（移行 4 の前の DB を作るため）
#[cfg(test)]
pub(crate) fn test_conn_at_version(name: &str, upto: i64) -> Connection {
    let conn = Connection::open_with_flags(
        format!("file:{name}?mode=memory&cache=shared"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_CREATE | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= upto) {
        conn.execute_batch(&format!("BEGIN;\n{sql}\nPRAGMA user_version = {version};\nCOMMIT;")).unwrap();
    }
    conn
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v3.1.2：3.1.1 までの DB（版 3）にある文字化けした値は、移行 4 で直る。直してはいけない値・ファイル名から作った曲名は変えない。
    /// アルバムの鍵と外部画像の設定もついてくる。2回目の起動では走らない
    #[test]
    fn migration_4_repairs_legacy_text_in_existing_rows() {
        let mk = |b: &[u8]| -> String { b.iter().map(|&x| x as char).collect() };
        // 「さくら色の坂道」「アオイ」（Shift_JIS）を Latin-1 で読んだ形。架空の名前
        let title = mk(b"\x82\xb3\x82\xad\x82\xe7\x90\x46\x82\xcc\x8d\xe2\x93\xb9");
        let artist = mk(b"\x83\x41\x83\x49\x83\x43");
        let conn = test_conn_at_version("migration4", 3);
        let ins = |path: &str, title: &str, artist: &str, album: &str| {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, album, album_key, added_at) VALUES (?1, ?2, ?3, ?4, ?5, 0)",
                rusqlite::params![path, title, artist, album, tracks::album_key("", artist, album)],
            )
            .unwrap();
        };
        ins(r"C:\m\01 a.mp3", &title, &artist, &artist);
        ins(r"C:\m\02 b.mp3", "Raison d'être", "A × B", "Plain");
        // 曲名がファイル名と同じ（タグに曲名が無かった曲）は、曲名には当てない
        let stem_title = mk(b"\x82\xa0\x82\xa2\x82\xa4");
        ins(&format!(r"C:\m\{stem_title}.mp3"), &stem_title, "", "");
        let old_key = tracks::album_key("", &artist, &artist);
        conn.execute("INSERT INTO album_art_overrides (album_key, image_path, updated_at) VALUES (?1, 'x.png', 0)", [&old_key]).unwrap();
        migrate(&conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        // v3.5.0 から移行 5、v3.6.0 から移行 6 まで続けて走る
        assert_eq!(v, MIGRATIONS.last().unwrap().0);
        assert!(v >= LEGACY_TEXT_VERSION);
        let get = |id: i64| -> (String, String, String, String) {
            conn.query_row("SELECT title, artist, album, album_key FROM tracks WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .unwrap()
        };
        let (t, a, al, key) = get(1);
        assert_eq!((t.as_str(), a.as_str(), al.as_str()), ("さくら色の坂道", "アオイ", "アオイ"));
        assert_eq!(key, tracks::album_key("", "アオイ", "アオイ"));
        let moved: String = conn.query_row("SELECT album_key FROM album_art_overrides", [], |r| r.get(0)).unwrap();
        assert_eq!(moved, key);
        assert_eq!(get(2).0, "Raison d'être");
        assert_eq!(get(2).1, "A × B");
        assert_eq!(get(3).0, stem_title);
        // 全文検索も新しい値で引ける（更新のトリガ）
        let n: i64 = conn
            .query_row("SELECT count(*) FROM tracks_fts WHERE tracks_fts MATCH '\"色の坂\"'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        // 2回目は走らない（版が 4 なので、戻した値もそのまま）
        conn.execute("UPDATE tracks SET artist = ?1 WHERE id = 1", [&artist]).unwrap();
        migrate(&conn).unwrap();
        assert_eq!(get(1).1, artist);
    }

    /// v3.5.0：移行 5 は、手で直した項目の印の表を作るだけ（曲の値は変えない。3.4.0 までの曲に印は付かない）。
    /// 曲を外すと印も消える（ON DELETE CASCADE）
    #[test]
    fn migration_5_adds_manual_edit_marks_without_touching_tracks() {
        let conn = test_conn_at_version("migration5", 4);
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album_key, added_at) VALUES ('C:\\m\\a.mp3', 'Plain', '\u{93}A\u{94}', '', 0)",
            [],
        )
        .unwrap();
        migrate(&conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        // v3.6.0 から移行 6 まで続けて走る
        assert_eq!(v, MIGRATIONS.last().unwrap().0);
        let (t, a): (String, String) = conn.query_row("SELECT title, artist FROM tracks", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((t.as_str(), a.as_str()), ("Plain", "\u{93}A\u{94}"));
        let marks: i64 = conn.query_row("SELECT count(*) FROM tag_manual_edits", [], |r| r.get(0)).unwrap();
        assert_eq!(marks, 0);
        tracks::mark_manual_edits(&conn, 1, 1).unwrap();
        tracks::mark_manual_edits(&conn, 1, 4).unwrap();
        let f: i64 = conn.query_row("SELECT fields FROM tag_manual_edits WHERE track_id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(f, 5, "印は前の印に足す");
        conn.execute("DELETE FROM tracks WHERE id = 1", []).unwrap();
        let marks: i64 = conn.query_row("SELECT count(*) FROM tag_manual_edits", [], |r| r.get(0)).unwrap();
        assert_eq!(marks, 0);
    }

    /// v3.6.0：移行 6 は、再生の記録の表と数え始めた時刻を作るだけ。3.5.0 までの再生回数・最後に再生した時刻は変えず、
    /// そこから記録を作り出さない（時刻が無いので嘘になる）。2回目の起動では走らず、数え始めた時刻も変わらない
    #[test]
    fn migration_6_adds_play_history_without_backfill() {
        let conn = test_conn_at_version("migration6", 5);
        conn.execute(
            "INSERT INTO tracks (path, title, album_key, added_at, last_played_at, play_count) VALUES ('C:\\m\\a.mp3', 'Plain', '', 0, 1700000000000, 7)",
            [],
        )
        .unwrap();
        let before = now_ms();
        migrate(&conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 6);
        let (last, count): (i64, i64) =
            conn.query_row("SELECT last_played_at, play_count FROM tracks WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((last, count), (1_700_000_000_000, 7));
        let rows: i64 = conn.query_row("SELECT count(*) FROM play_history", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0, "3.5.0 までの再生から記録を作らない");
        let started = history::started_at(&conn).unwrap().unwrap();
        assert!(started >= before - 1000 && started <= now_ms() + 1000);
        // 索引：期間で数える（played_at, track_id）と、曲を外すときに記録を探す（track_id）
        let idx: Vec<String> = {
            let mut s = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'play_history' ORDER BY name").unwrap();
            let v = s.query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
            v
        };
        assert_eq!(idx, vec!["idx_play_history_played_at".to_string(), "idx_play_history_track".to_string()]);
        let plan: String = conn
            .query_row(
                "EXPLAIN QUERY PLAN SELECT track_id, COUNT(*), MAX(played_at) FROM play_history WHERE played_at >= 1 AND played_at <= 2 GROUP BY track_id",
                [],
                |r| r.get(3),
            )
            .unwrap();
        assert!(plan.contains("COVERING INDEX idx_play_history_played_at"), "{plan}");
        // 2回目は走らない
        conn.execute("UPDATE play_history_meta SET started_at = 5 WHERE id = 1", []).unwrap();
        migrate(&conn).unwrap();
        assert_eq!(history::started_at(&conn).unwrap(), Some(5));
    }

    /// v3.6.0：3.5.0 の版で移行 6 の DB を開いても壊れない。
    /// 3.5.0 の `migrate` は「今の版より大きい移行だけを当てる」形で、知らない版（6）を断らず、何も当てない。
    /// 3.5.0 の書き込み（再生回数を +1・曲を外す・取り込み）は、知らない表があっても通り、曲を外せば記録も消える（表の決まり）。
    /// 3.5.0 で再生した分は記録されない（README の既知の制限）。実物の 3.5.0 のコードでも同じことを確かめた（作業の報告）
    #[test]
    fn a_v6_db_opened_by_3_5_0_stays_intact() {
        let conn = test_conn_at_version("migration6_old", 6);
        conn.execute("INSERT INTO tracks (path, title, album_key, added_at) VALUES ('C:\\m\\a.mp3', 'Plain', '', 0)", []).unwrap();
        conn.execute("INSERT INTO tracks (path, title, album_key, added_at) VALUES ('C:\\m\\b.mp3', 'Other', '', 0)", []).unwrap();
        conn.execute("INSERT INTO play_history (track_id, played_at) VALUES (1, 10), (1, 20), (2, 30)", []).unwrap();
        // 3.5.0 の移行の一覧（1〜5）と、3.5.0 の migrate と同じ当て方
        let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        let old: Vec<i64> = MIGRATIONS.iter().map(|(v, _)| *v).filter(|v| *v <= 5).filter(|v| *v > current).collect();
        assert!(old.is_empty(), "3.5.0 は版 6 の DB に何も当てない");
        // 3.5.0 の書き込み
        conn.execute("UPDATE tracks SET last_played_at = 40, play_count = play_count + 1 WHERE id = 2", []).unwrap();
        conn.execute("DELETE FROM tracks WHERE id = 1", []).unwrap();
        conn.execute("INSERT INTO tracks (path, title, album_key, added_at) VALUES ('C:\\m\\c.mp3', 'New', '', 0)", []).unwrap();
        let rows: Vec<(i64, i64)> = {
            let mut s = conn.prepare("SELECT track_id, played_at FROM play_history ORDER BY id").unwrap();
            let v = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
            v
        };
        assert_eq!(rows, vec![(2, 30)]);
        let ok: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
        assert_eq!(ok, "ok");
        let fk: i64 = conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0)).unwrap();
        assert_eq!(fk, 0);
        // 3.6.0 に戻すと、移行は走らず、記録はそのまま使える
        migrate(&conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 6);
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
