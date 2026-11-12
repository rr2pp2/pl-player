//! 歌詞の2つの表（lyrics_sources・track_lyrics_settings）の読み書き（仕様 2.2）。

use rusqlite::{params, Connection, OptionalExtension};

use super::Db;
use crate::error::AppResult;

/// DB に持つ歌詞の種類（曲ファイルのタグは DB に持たない）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredKind {
    Manual,
    External,
}

impl StoredKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            StoredKind::Manual => "manual",
            StoredKind::External => "external",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredLyrics {
    pub raw_text: String,
    /// "timed" / "plain"
    pub format: String,
    pub updated_at: String,
}

/// 保存の時刻（UTC、ISO 8601）。SQLite の関数で作る（時刻の部品を足さないため）
const NOW_UTC: &str = "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";

pub fn get(db: &Db, track_id: i64, kind: StoredKind) -> AppResult<Option<StoredLyrics>> {
    let conn = db.reader();
    Ok(conn
        .query_row(
            "SELECT raw_text, format, updated_at FROM lyrics_sources WHERE track_id = ?1 AND source_kind = ?2",
            params![track_id, kind.as_str()],
            |r| {
                Ok(StoredLyrics {
                    raw_text: r.get(0)?,
                    format: r.get(1)?,
                    updated_at: r.get(2)?,
                })
            },
        )
        .optional()?)
}

pub fn exists(db: &Db, track_id: i64, kind: StoredKind) -> AppResult<bool> {
    let conn = db.reader();
    Ok(conn
        .query_row(
            "SELECT 1 FROM lyrics_sources WHERE track_id = ?1 AND source_kind = ?2",
            params![track_id, kind.as_str()],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// 上書き保存（同じ曲・同じ種類は1件だけ）。ほかの種類には触らない
pub fn upsert(conn: &Connection, track_id: i64, kind: StoredKind, raw_text: &str, format: &str) -> AppResult<()> {
    conn.execute(
        &format!(
            "INSERT INTO lyrics_sources (track_id, source_kind, raw_text, format, updated_at)
             VALUES (?1, ?2, ?3, ?4, {NOW_UTC})
             ON CONFLICT(track_id, source_kind) DO UPDATE SET
               raw_text = excluded.raw_text, format = excluded.format, updated_at = excluded.updated_at"
        ),
        params![track_id, kind.as_str(), raw_text, format],
    )?;
    Ok(())
}

/// 指定した種類だけ消す。消したら true
pub fn delete(db: &Db, track_id: i64, kind: StoredKind) -> AppResult<bool> {
    let n = db.writer().execute(
        "DELETE FROM lyrics_sources WHERE track_id = ?1 AND source_kind = ?2",
        params![track_id, kind.as_str()],
    )?;
    Ok(n > 0)
}

/// 曲ごとの時刻補正（ms）。設定が無ければ 0
pub fn delay(db: &Db, track_id: i64) -> AppResult<i64> {
    let conn = db.reader();
    Ok(conn
        .query_row(
            "SELECT user_delay_ms FROM track_lyrics_settings WHERE track_id = ?1",
            [track_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

pub fn set_delay(db: &Db, track_id: i64, delay_ms: i64) -> AppResult<()> {
    db.writer().execute(
        &format!(
            "INSERT INTO track_lyrics_settings (track_id, user_delay_ms, updated_at) VALUES (?1, ?2, {NOW_UTC})
             ON CONFLICT(track_id) DO UPDATE SET user_delay_ms = excluded.user_delay_ms, updated_at = excluded.updated_at"
        ),
        params![track_id, delay_ms],
    )?;
    Ok(())
}

/// 曲 ID が tracks にあるか
pub fn track_exists(db: &Db, track_id: i64) -> AppResult<bool> {
    let conn = db.reader();
    Ok(conn
        .query_row("SELECT 1 FROM tracks WHERE id = ?1", [track_id], |_| Ok(()))
        .optional()?
        .is_some())
}

/// 取り込み済みファイルの名前に使う、この PC の時刻（yyyyMMdd-HHmmss）。SQLite の localtime で作る
pub fn local_stamp(db: &Db) -> AppResult<String> {
    let conn = db.reader();
    Ok(conn.query_row("SELECT strftime('%Y%m%d-%H%M%S', 'now', 'localtime')", [], |r| r.get(0))?)
}

// ---- 歌詞をネットから取る（v3.1.0。移行 3 の lyrics_fetch_misses） ----

/// ネットから取る候補の1曲（手元の値だけ。パスは曲のタグを読むのに使い、外へは送らない）
#[derive(Debug, Clone, PartialEq)]
pub struct FetchCandidate {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
    pub has_manual: bool,
    pub has_external: bool,
    /// 覚えている「見つからない」などの、送ったときの印（無ければ None）
    pub miss_sig: Option<String>,
}

const CANDIDATE_SQL: &str = "SELECT t.id, t.path, t.title, t.artist, t.album, t.duration_ms,
        EXISTS(SELECT 1 FROM lyrics_sources s WHERE s.track_id = t.id AND s.source_kind = 'manual'),
        EXISTS(SELECT 1 FROM lyrics_sources s WHERE s.track_id = t.id AND s.source_kind = 'external'),
        m.query_sig
     FROM tracks t LEFT JOIN lyrics_fetch_misses m ON m.track_id = t.id";

fn candidate_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<FetchCandidate> {
    Ok(FetchCandidate {
        id: r.get(0)?,
        path: r.get(1)?,
        title: r.get(2)?,
        artist: r.get(3)?,
        album: r.get(4)?,
        duration_ms: r.get(5)?,
        has_manual: r.get(6)?,
        has_external: r.get(7)?,
        miss_sig: r.get(8)?,
    })
}

/// 手動の歌詞が無い曲を、曲 ID の順に（手動のある曲は、どの取り方でも送らない）
pub fn fetch_candidates(db: &Db) -> AppResult<Vec<FetchCandidate>> {
    let conn = db.reader();
    let mut stmt = conn.prepare(&format!(
        "{CANDIDATE_SQL} WHERE NOT EXISTS(SELECT 1 FROM lyrics_sources s WHERE s.track_id = t.id AND s.source_kind = 'manual')
         ORDER BY t.id"
    ))?;
    let rows = stmt.query_map([], candidate_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// 1曲ぶん（手動の歌詞があるかも返す）
pub fn fetch_candidate(db: &Db, track_id: i64) -> AppResult<Option<FetchCandidate>> {
    let conn = db.reader();
    Ok(conn
        .query_row(&format!("{CANDIDATE_SQL} WHERE t.id = ?1"), [track_id], candidate_row)
        .optional()?)
}

/// 見つからない・instrumental・大きすぎ を覚える（同じ曲は書き換える）
pub fn record_miss(db: &Db, track_id: i64, result: &str, query_sig: &str) -> AppResult<()> {
    db.writer().execute(
        &format!(
            "INSERT INTO lyrics_fetch_misses (track_id, result, query_sig, checked_at) VALUES (?1, ?2, ?3, {NOW_UTC})
             ON CONFLICT(track_id) DO UPDATE SET result = excluded.result, query_sig = excluded.query_sig, checked_at = excluded.checked_at"
        ),
        params![track_id, result, query_sig],
    )?;
    Ok(())
}

pub fn clear_miss(db: &Db, track_id: i64) -> AppResult<()> {
    db.writer().execute("DELETE FROM lyrics_fetch_misses WHERE track_id = ?1", [track_id])?;
    Ok(())
}

/// 覚えている結果（result, query_sig, checked_at）
pub fn miss(db: &Db, track_id: i64) -> AppResult<Option<(String, String, String)>> {
    let conn = db.reader();
    Ok(conn
        .query_row(
            "SELECT result, query_sig, checked_at FROM lyrics_fetch_misses WHERE track_id = ?1",
            [track_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tracks::{self, TrackInfo};

    fn add_track(db: &Db, path: &str) -> i64 {
        tracks::upsert(
            &db.writer(),
            &TrackInfo {
                path: path.into(),
                title: path.into(),
                ..Default::default()
            },
        )
        .unwrap()
        .0
    }

    #[test]
    fn migrations_2_and_3_are_applied() {
        let db = Db::open_memory("lyrics_mig").unwrap();
        let v: i64 = db.reader().query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        // v3.1.2 で移行 4（古い文字コードの値を直す。表は変えない）を足した
        assert_eq!(v, 4);
    }

    /// 移行 3：覚えた結果の読み書き。結果は3種だけ。曲を外すと消える
    #[test]
    fn fetch_misses_table() {
        let db = Db::open_memory("lyrics_misses").unwrap();
        let a = add_track(&db, "C:/x/m1.mp3");
        let b = add_track(&db, "C:/x/m2.mp3");
        upsert(&db.writer(), b, StoredKind::Manual, "手動", "plain").unwrap();
        record_miss(&db, a, "not_found", "sig1").unwrap();
        record_miss(&db, a, "instrumental", "sig2").unwrap();
        let (r, sig, at) = miss(&db, a).unwrap().unwrap();
        assert_eq!((r.as_str(), sig.as_str()), ("instrumental", "sig2"));
        assert!(at.ends_with('Z'));
        assert!(record_miss(&db, a, "synced", "x").is_err());
        let c = fetch_candidates(&db).unwrap();
        assert_eq!(c.len(), 1, "手動の歌詞がある曲は候補に入らない");
        assert_eq!(c[0].id, a);
        assert_eq!(c[0].miss_sig.as_deref(), Some("sig2"));
        assert!(fetch_candidate(&db, b).unwrap().unwrap().has_manual);
        clear_miss(&db, a).unwrap();
        assert!(miss(&db, a).unwrap().is_none());
        record_miss(&db, a, "not_found", "s").unwrap();
        tracks::remove(&db, &[a]).unwrap();
        assert!(miss(&db, a).unwrap().is_none());
    }

    #[test]
    fn upsert_get_delete_and_cascade() {
        let db = Db::open_memory("lyrics_rw").unwrap();
        let id = add_track(&db, "C:/x/a.mp3");
        assert!(get(&db, id, StoredKind::Manual).unwrap().is_none());
        upsert(&db.writer(), id, StoredKind::Manual, "一", "plain").unwrap();
        upsert(&db.writer(), id, StoredKind::External, "[00:01.00]二", "timed").unwrap();
        upsert(&db.writer(), id, StoredKind::External, "[00:01.00]三", "timed").unwrap();
        assert_eq!(get(&db, id, StoredKind::Manual).unwrap().unwrap().raw_text, "一");
        let ext = get(&db, id, StoredKind::External).unwrap().unwrap();
        assert_eq!(ext.raw_text, "[00:01.00]三");
        assert_eq!(ext.format, "timed");
        assert!(ext.updated_at.ends_with('Z'));
        set_delay(&db, id, -300).unwrap();
        assert_eq!(delay(&db, id).unwrap(), -300);
        assert!(delete(&db, id, StoredKind::Manual).unwrap());
        assert!(!delete(&db, id, StoredKind::Manual).unwrap());
        assert!(exists(&db, id, StoredKind::External).unwrap());
        // 曲をライブラリから外すと、歌詞と補正も消える
        tracks::remove(&db, &[id]).unwrap();
        assert!(!exists(&db, id, StoredKind::External).unwrap());
        assert_eq!(delay(&db, id).unwrap(), 0);
        let stamp = local_stamp(&db).unwrap();
        assert_eq!(stamp.len(), 15, "{stamp}");
    }
}
