//! SQLite（rusqlite bundled）。WAL モードで、読み込み中でも書き込みで画面が止まらないようにする。
//!
//! 接続は2本：書き込み用と読み取り用。どちらも Mutex で守る。

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
];

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
            conn.execute_batch(&format!("BEGIN;\n{sql}\nPRAGMA user_version = {version};\nCOMMIT;"))?;
        }
    }
    Ok(())
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
