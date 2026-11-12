//! 曲の表（tracks）の読み書き。

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::{now_ms, search, Db};
use crate::error::AppResult;

/// 画面へ渡す1曲ぶん。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: i64,
    pub path: String,
    pub format: String,
    pub duration_ms: i64,
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
    pub has_art: bool,
    /// 表示に使うアートのファイルパス（外部画像の設定があればそちらが優先）
    pub art_path: Option<String>,
    pub album_key: String,
    pub added_at: i64,
    pub last_played_at: Option<i64>,
    pub play_count: i64,
    pub favorite: bool,
    pub missing: bool,
    pub tag_error: bool,
}

/// 取り込み・再読み込みで tracks に書く値。
#[derive(Debug, Clone, Default)]
pub struct TrackInfo {
    pub path: String,
    pub file_size: i64,
    pub file_mtime: i64,
    pub format: String,
    pub duration_ms: i64,
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
    pub has_art: bool,
    pub art_key: Option<String>,
    pub tag_error: bool,
}

pub fn album_key(album_artist: &str, artist: &str, album: &str) -> String {
    let who = if album_artist.trim().is_empty() {
        artist
    } else {
        album_artist
    };
    format!("{}\u{1}{}", who.trim().to_lowercase(), album.trim().to_lowercase())
}

pub(crate) const SELECT_TRACK: &str = "SELECT t.id, t.path, t.format, t.duration_ms, t.title, t.artist, t.album, t.album_artist,
    t.genre, t.year, t.track_no, t.disc_no, t.composer, t.comment, t.has_art, t.art_key, o.image_path,
    t.album_key, t.added_at, t.last_played_at, t.play_count, t.favorite, t.missing, t.tag_error
    FROM tracks t LEFT JOIN album_art_overrides o ON o.album_key = t.album_key";

pub(crate) fn row_to_track(db: &Db, r: &Row<'_>) -> rusqlite::Result<Track> {
    let art_key: Option<String> = r.get(15)?;
    let override_path: Option<String> = r.get(16)?;
    let art_path = override_path.or_else(|| {
        art_key.map(|k| db.thumbs_dir.join(format!("{k}.jpg")).to_string_lossy().into_owned())
    });
    Ok(Track {
        id: r.get(0)?,
        path: r.get(1)?,
        format: r.get(2)?,
        duration_ms: r.get(3)?,
        title: r.get(4)?,
        artist: r.get(5)?,
        album: r.get(6)?,
        album_artist: r.get(7)?,
        genre: r.get(8)?,
        year: r.get(9)?,
        track_no: r.get(10)?,
        disc_no: r.get(11)?,
        composer: r.get(12)?,
        comment: r.get(13)?,
        has_art: r.get::<_, i64>(14)? != 0,
        art_path,
        album_key: r.get(17)?,
        added_at: r.get(18)?,
        last_played_at: r.get(19)?,
        play_count: r.get(20)?,
        favorite: r.get::<_, i64>(21)? != 0,
        missing: r.get::<_, i64>(22)? != 0,
        tag_error: r.get::<_, i64>(23)? != 0,
    })
}

/// 取り込み。同じパスがあれば更新（追加日時・再生回数・お気に入りは保つ）。戻り値は (id, 新規か)
pub fn upsert(conn: &Connection, info: &TrackInfo) -> AppResult<(i64, bool)> {
    let key = album_key(&info.album_artist, &info.artist, &info.album);
    let existing: Option<i64> = conn
        .query_row("SELECT id FROM tracks WHERE path = ?1", [&info.path], |r| r.get(0))
        .optional()?;
    if let Some(id) = existing {
        conn.execute(
            "UPDATE tracks SET file_size=?2, file_mtime=?3, format=?4, duration_ms=?5, title=?6, artist=?7,
             album=?8, album_artist=?9, genre=?10, year=?11, track_no=?12, disc_no=?13, composer=?14,
             comment=?15, has_art=?16, art_key=?17, album_key=?18, missing=0, tag_error=?19 WHERE id=?1",
            params![
                id,
                info.file_size,
                info.file_mtime,
                info.format,
                info.duration_ms,
                info.title,
                info.artist,
                info.album,
                info.album_artist,
                info.genre,
                info.year,
                info.track_no,
                info.disc_no,
                info.composer,
                info.comment,
                info.has_art as i64,
                info.art_key,
                key,
                info.tag_error as i64
            ],
        )?;
        Ok((id, false))
    } else {
        conn.execute(
            "INSERT INTO tracks (path, file_size, file_mtime, format, duration_ms, title, artist, album,
             album_artist, genre, year, track_no, disc_no, composer, comment, has_art, art_key, album_key,
             added_at, tag_error)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
            params![
                info.path,
                info.file_size,
                info.file_mtime,
                info.format,
                info.duration_ms,
                info.title,
                info.artist,
                info.album,
                info.album_artist,
                info.genre,
                info.year,
                info.track_no,
                info.disc_no,
                info.composer,
                info.comment,
                info.has_art as i64,
                info.art_key,
                key,
                now_ms(),
                info.tag_error as i64
            ],
        )?;
        Ok((conn.last_insert_rowid(), true))
    }
}

/// 既に取り込み済みで、大きさと更新日時が変わっていないか（再走査を速くする）
pub fn unchanged(conn: &Connection, path: &str, size: i64, mtime: i64) -> AppResult<bool> {
    let row: Option<(i64, i64, i64)> = conn
        .query_row(
            // タグを読めなかった曲は、変わっていなくても読み直す（v2：読み方を直したので、再走査で正しい値になる）
            "SELECT file_size, file_mtime, missing + tag_error FROM tracks WHERE path = ?1",
            [path],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    Ok(matches!(row, Some((s, m, 0)) if s == size && m == mtime))
}

pub fn get(db: &Db, id: i64) -> AppResult<Option<Track>> {
    let conn = db.reader();
    let sql = format!("{SELECT_TRACK} WHERE t.id = ?1");
    Ok(conn
        .query_row(&sql, [id], |r| row_to_track(db, r))
        .optional()?)
}

pub fn get_many(db: &Db, ids: &[i64]) -> AppResult<Vec<Track>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let conn = db.reader();
    let mut out = Vec::with_capacity(ids.len());
    let sql = format!("{SELECT_TRACK} WHERE t.id = ?1");
    let mut stmt = conn.prepare_cached(&sql)?;
    for id in ids {
        if let Some(t) = stmt.query_row([id], |r| row_to_track(db, r)).optional()? {
            out.push(t);
        }
    }
    Ok(out)
}

pub fn path_of(db: &Db, id: i64) -> AppResult<Option<String>> {
    let conn = db.reader();
    Ok(conn
        .query_row("SELECT path FROM tracks WHERE id = ?1", [id], |r| r.get(0))
        .optional()?)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackQuery {
    /// all / artist / album / genre / recent_added / recent_played / favorites / playlist
    pub view: String,
    pub key: Option<String>,
    pub search: Option<String>,
    pub sort: Option<String>,
    pub desc: Option<bool>,
}

fn order_clause(view: &str, sort: Option<&str>, desc: bool) -> String {
    let dir = if desc { "DESC" } else { "ASC" };
    match sort {
        Some("title") => format!("t.title COLLATE NOCASE {dir}"),
        Some("artist") => format!("t.artist COLLATE NOCASE {dir}, t.album COLLATE NOCASE, t.disc_no, t.track_no"),
        Some("album") => format!("t.album COLLATE NOCASE {dir}, t.disc_no, t.track_no"),
        Some("genre") => format!("t.genre COLLATE NOCASE {dir}, t.artist COLLATE NOCASE"),
        Some("duration") => format!("t.duration_ms {dir}"),
        Some("added") => format!("t.added_at {dir}"),
        _ => match view {
            "recent_added" => "t.added_at DESC".into(),
            "recent_played" => "t.last_played_at DESC".into(),
            "album" => "t.disc_no, t.track_no, t.title COLLATE NOCASE".into(),
            _ => "t.artist COLLATE NOCASE, t.album COLLATE NOCASE, t.disc_no, t.track_no, t.title COLLATE NOCASE"
                .into(),
        },
    }
}

/// 曲一覧（プレイリスト以外の表示）。検索語があれば同時に絞る。
pub fn query(db: &Db, q: &TrackQuery) -> AppResult<Vec<Track>> {
    let mut wheres: Vec<String> = vec![];
    let mut args: Vec<rusqlite::types::Value> = vec![];
    let mut limit = String::new();
    match q.view.as_str() {
        "artist" => {
            wheres.push(format!("t.artist = ?{}", args.len() + 1));
            args.push(q.key.clone().unwrap_or_default().into());
        }
        "album" => {
            wheres.push(format!("t.album_key = ?{}", args.len() + 1));
            args.push(q.key.clone().unwrap_or_default().into());
        }
        "genre" => {
            wheres.push(format!("t.genre = ?{}", args.len() + 1));
            args.push(q.key.clone().unwrap_or_default().into());
        }
        "recent_added" => {
            limit = " LIMIT 500".into();
        }
        "recent_played" => {
            wheres.push("t.last_played_at IS NOT NULL".into());
            limit = " LIMIT 500".into();
        }
        "favorites" => wheres.push("t.favorite = 1".into()),
        _ => {}
    }
    if let Some(s) = q.search.as_deref() {
        search::push_search_filter(s, &mut wheres, &mut args);
    }
    let where_sql = if wheres.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", wheres.join(" AND "))
    };
    let order = order_clause(&q.view, q.sort.as_deref(), q.desc.unwrap_or(false));
    let sql = format!("{SELECT_TRACK}{where_sql} ORDER BY {order}{limit}");
    let conn = db.reader();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| row_to_track(db, r))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// 一覧を引くときの鍵（アーティスト名・アルバムの鍵・ジャンル名）
    pub key: String,
    pub name: String,
    pub sub: String,
    pub count: i64,
    pub art_path: Option<String>,
}

/// アーティスト・アルバム・ジャンルの一覧
pub fn groups(db: &Db, kind: &str, search: Option<&str>) -> AppResult<Vec<Group>> {
    let mut wheres: Vec<String> = vec![];
    let mut args: Vec<rusqlite::types::Value> = vec![];
    if let Some(s) = search {
        search::push_search_filter(s, &mut wheres, &mut args);
    }
    let where_sql = if wheres.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", wheres.join(" AND "))
    };
    let sql = match kind {
        "artist" => format!(
            "SELECT t.artist, t.artist, '', COUNT(*), NULL, NULL FROM tracks t{where_sql}
             GROUP BY t.artist ORDER BY t.artist COLLATE NOCASE"
        ),
        "genre" => format!(
            "SELECT t.genre, t.genre, '', COUNT(*), NULL, NULL FROM tracks t{where_sql}
             GROUP BY t.genre ORDER BY t.genre COLLATE NOCASE"
        ),
        _ => format!(
            "SELECT t.album_key, MAX(t.album),
                    COALESCE(NULLIF(MAX(t.album_artist), ''), MAX(t.artist)),
                    COUNT(*), MAX(t.art_key), MAX(o.image_path)
             FROM tracks t LEFT JOIN album_art_overrides o ON o.album_key = t.album_key{where_sql}
             GROUP BY t.album_key ORDER BY MAX(t.album) COLLATE NOCASE"
        ),
    };
    let conn = db.reader();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
        let art_key: Option<String> = r.get(4)?;
        let over: Option<String> = r.get(5)?;
        Ok(Group {
            key: r.get(0)?,
            name: r.get(1)?,
            sub: r.get(2)?,
            count: r.get(3)?,
            art_path: over.or_else(|| {
                art_key.map(|k| db.thumbs_dir.join(format!("{k}.jpg")).to_string_lossy().into_owned())
            }),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn set_favorite(db: &Db, ids: &[i64], favorite: bool) -> AppResult<()> {
    let mut conn = db.writer();
    let tx = conn.transaction()?;
    for id in ids {
        tx.execute(
            "UPDATE tracks SET favorite = ?2 WHERE id = ?1",
            params![id, favorite as i64],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn mark_played(db: &Db, id: i64) -> AppResult<()> {
    db.writer().execute(
        "UPDATE tracks SET last_played_at = ?2, play_count = play_count + 1 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    Ok(())
}

pub fn set_missing(db: &Db, id: i64, missing: bool) -> AppResult<()> {
    db.writer().execute(
        "UPDATE tracks SET missing = ?2 WHERE id = ?1",
        params![id, missing as i64],
    )?;
    Ok(())
}

/// ライブラリから外す（ファイルは消さない）
pub fn remove(db: &Db, ids: &[i64]) -> AppResult<()> {
    let mut conn = db.writer();
    let tx = conn.transaction()?;
    for id in ids {
        tx.execute("DELETE FROM tracks WHERE id = ?1", [id])?;
    }
    tx.commit()?;
    Ok(())
}

pub fn all_paths(db: &Db) -> AppResult<Vec<(i64, String)>> {
    let conn = db.reader();
    let mut stmt = conn.prepare("SELECT id, path FROM tracks")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn favorite_paths(db: &Db) -> AppResult<Vec<String>> {
    let conn = db.reader();
    let mut stmt = conn.prepare("SELECT path FROM tracks WHERE favorite = 1")?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn set_favorite_by_paths(db: &Db, paths: &[String]) -> AppResult<usize> {
    let mut conn = db.writer();
    let tx = conn.transaction()?;
    let mut n = 0;
    for p in paths {
        n += tx.execute("UPDATE tracks SET favorite = 1 WHERE path = ?1", [p])?;
    }
    tx.commit()?;
    Ok(n)
}

pub fn id_by_path(db: &Db, path: &str) -> AppResult<Option<i64>> {
    let conn = db.reader();
    Ok(conn
        .query_row("SELECT id FROM tracks WHERE path = ?1", [path], |r| r.get(0))
        .optional()?)
}

pub fn count(db: &Db) -> AppResult<i64> {
    let conn = db.reader();
    Ok(conn.query_row("SELECT COUNT(*) FROM tracks", [], |r| r.get(0))?)
}

// ---- アルバムアートの外部画像 ----

pub fn set_album_override(db: &Db, album_key: &str, image_path: Option<&str>) -> AppResult<()> {
    let conn = db.writer();
    match image_path {
        Some(p) => {
            conn.execute(
                "INSERT INTO album_art_overrides (album_key, image_path, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(album_key) DO UPDATE SET image_path = excluded.image_path, updated_at = excluded.updated_at",
                params![album_key, p, now_ms()],
            )?;
        }
        None => {
            conn.execute("DELETE FROM album_art_overrides WHERE album_key = ?1", [album_key])?;
        }
    }
    Ok(())
}

pub fn album_override(db: &Db, album_key: &str) -> AppResult<Option<String>> {
    let conn = db.reader();
    Ok(conn
        .query_row(
            "SELECT image_path FROM album_art_overrides WHERE album_key = ?1",
            [album_key],
            |r| r.get(0),
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(path: &str, title: &str, artist: &str, album: &str) -> TrackInfo {
        TrackInfo {
            path: path.into(),
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            format: "mp3".into(),
            ..Default::default()
        }
    }

    #[test]
    fn upsert_keeps_favorite_and_added_at() {
        let db = Db::open_memory("tracks_upsert").unwrap();
        let (id, new) = upsert(&db.writer(), &info("C:/a.mp3", "A", "X", "Al")).unwrap();
        assert!(new);
        set_favorite(&db, &[id], true).unwrap();
        let before = get(&db, id).unwrap().unwrap();
        let (id2, new2) = upsert(&db.writer(), &info("C:/a.mp3", "A2", "X", "Al")).unwrap();
        assert_eq!(id, id2);
        assert!(!new2);
        let after = get(&db, id).unwrap().unwrap();
        assert!(after.favorite);
        assert_eq!(after.title, "A2");
        assert_eq!(before.added_at, after.added_at);
    }

    #[test]
    fn groups_and_views() {
        let db = Db::open_memory("tracks_groups").unwrap();
        {
            let w = db.writer();
            upsert(&w, &info("C:/1.mp3", "One", "Alice", "First")).unwrap();
            upsert(&w, &info("C:/2.mp3", "Two", "Alice", "First")).unwrap();
            upsert(&w, &info("C:/3.mp3", "Three", "Bob", "Second")).unwrap();
        }
        let artists = groups(&db, "artist", None).unwrap();
        assert_eq!(artists.len(), 2);
        assert_eq!(artists[0].count, 2);
        let albums = groups(&db, "album", None).unwrap();
        assert_eq!(albums.len(), 2);
        let q = TrackQuery {
            view: "album".into(),
            key: Some(albums[0].key.clone()),
            search: None,
            sort: None,
            desc: None,
        };
        assert_eq!(query(&db, &q).unwrap().len(), 2);
    }
}
