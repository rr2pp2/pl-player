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

/// 3.1.1 までに取り込んだ曲の、古い文字コードで化けた値を直す（v3.1.2。移行 4 の中で1回だけ走る。`db::migrate`）。
///
/// 取り込み（`tags_read::read_track`）は v3.1.2 から読むときに直すが、変わっていない曲は再走査で読み直さない。
/// そこで DB の値に同じ決まり（`library::legacy_text::repair`）を当てる。曲ファイルは読まない・書かない。
/// - 先に SQL で、U+0080〜U+00FF の文字を含む行だけに絞る（それ以外の行は決まり 1・2 に当たらない）
/// - 曲名がファイル名（拡張子なし）と同じ行は、曲名には当てない（タグに曲名が無く、ファイル名から作った値のため）
/// - アルバムの鍵（album_key）も作り直し、外部画像の設定（album_art_overrides）は新しい鍵へ移す
///   （古い鍵の曲がもう無く、新しい鍵に設定が無いときだけ）
///
/// 直した行の数を返す
pub fn repair_legacy_text(conn: &Connection) -> AppResult<usize> {
    use crate::library::legacy_text::repair;
    type Row7 = (i64, String, String, LegacyFields);
    let rows: Vec<Row7> = {
        let mut stmt = conn.prepare(
            "SELECT id, path, album_key, title, artist, album, album_artist, genre, composer, comment FROM tracks
             WHERE (title || char(1) || artist || char(1) || album || char(1) || album_artist || char(1) || genre
                    || char(1) || composer || char(1) || comment) GLOB ('*[' || char(128) || '-' || char(255) || ']*')",
        )?;
        let it = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                [r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?],
            ))
        })?;
        it.collect::<rusqlite::Result<_>>()?
    };
    let mut fixed = 0;
    for (id, path, old_key, v) in rows {
        let Some(new) = repair_fields(&path, &v, 0, &repair).fixed else {
            continue;
        };
        if write_repaired(conn, id, &v, &old_key, &new)? {
            fixed += 1;
        }
    }
    if fixed > 0 {
        tracing::info!("legacy text repaired in {fixed} tracks");
    }
    Ok(fixed)
}

/// 古い文字コードの決まりを当てる7項目（曲名・アーティスト・アルバム・アルバムアーティスト・ジャンル・作曲者・コメント）。
/// この並びは、手で直した項目の印（`tag_manual_edits.fields`。v3.5.0）のビットの並びと同じ（曲名が 1、アーティストが 2 …）
pub type LegacyFields = [String; 7];

/// 7項目のどれかに U+0080〜U+00FF の文字があるか（無ければ決まり 1・2 に当たらないので、決まりを当てるまでもない）
pub(crate) fn has_latin1_high(v: &LegacyFields) -> bool {
    v.iter().any(|f| f.chars().any(|c| ('\u{80}'..='\u{FF}').contains(&c)))
}

/// `repair_fields` の結果
#[derive(Debug, Default, PartialEq)]
pub(crate) struct FieldsRepair {
    /// 直した値（変わる項目が無ければ None）
    pub fixed: Option<LegacyFields>,
    /// 手で直した印（`skip`）があるので当てなかったが、当てれば変わった項目があった
    pub skipped_manual: bool,
}

/// 1曲の7項目に、古い文字コードの決まり（`repair`）を当てる（移行 4 と、v3.5.0 の読み直しで共用）。
/// - `skip`：当てない項目のビット（v3.5.0：編集画面で手で変えた項目。移行 4 では 0）
/// - 曲名がファイル名（拡張子なし）と同じなら、曲名には当てない（タグに曲名が無く、ファイル名から作った値のため）
/// - `repair` は `library::legacy_text::repair` と同じ形（値・ファイル名）。テストで決まりを差し替えるために受け取る
pub(crate) fn repair_fields(
    path: &str,
    v: &LegacyFields,
    skip: i64,
    repair: &dyn Fn(&str, Option<&str>) -> Option<String>,
) -> FieldsRepair {
    let stem = std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned());
    let mut out = v.clone();
    let mut changed = false;
    let mut skipped_manual = false;
    for (i, field) in out.iter_mut().enumerate() {
        if i == 0 && stem.as_deref().is_some_and(|s| s == field.as_str()) {
            continue;
        }
        if let Some(f) = repair(field, stem.as_deref()) {
            if f == *field {
                continue;
            }
            if skip & (1 << i) != 0 {
                skipped_manual = true;
                continue;
            }
            *field = f;
            changed = true;
        }
    }
    FieldsRepair { fixed: changed.then_some(out), skipped_manual }
}

/// 直した値を書く。**読んだときの値（`old` と `old_key`）のままの行だけ**を書き換える
/// （v3.5.0：読み直しの途中で、編集画面の保存・取り込みが値を変えた行や、ごみ箱へ移す・ライブラリから外すで消えた行は書かない）。
/// アルバムの鍵も作り直し、外部画像の設定（album_art_overrides）は新しい鍵へ移す（古い鍵の曲がもう無く、新しい鍵に設定が無いときだけ）。
/// 書いたら true
pub(crate) fn write_repaired(conn: &Connection, id: i64, old: &LegacyFields, old_key: &str, new: &LegacyFields) -> AppResult<bool> {
    let key = album_key(&new[3], &new[1], &new[2]);
    let n = conn.execute(
        "UPDATE tracks SET title=?2, artist=?3, album=?4, album_artist=?5, genre=?6, composer=?7, comment=?8, album_key=?9
         WHERE id=?1 AND title=?10 AND artist=?11 AND album=?12 AND album_artist=?13 AND genre=?14 AND composer=?15
               AND comment=?16 AND album_key=?17",
        params![
            id, new[0], new[1], new[2], new[3], new[4], new[5], new[6], key, old[0], old[1], old[2], old[3], old[4], old[5],
            old[6], old_key
        ],
    )?;
    if n == 0 {
        return Ok(false);
    }
    if key != old_key {
        conn.execute(
            "UPDATE OR IGNORE album_art_overrides SET album_key = ?2 WHERE album_key = ?1
             AND NOT EXISTS (SELECT 1 FROM tracks WHERE album_key = ?1)",
            params![old_key, key],
        )?;
    }
    Ok(true)
}

/// 編集画面で手で変えた項目に印を付ける（v3.5.0。`fields` は `LegacyFields` の並びのビット。前の印に足す）。
/// 古い文字コードの読み直し（`library::legacy_reread`）は、印の付いた項目を書き換えない
pub fn mark_manual_edits(conn: &Connection, id: i64, fields: i64) -> AppResult<()> {
    if fields == 0 {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO tag_manual_edits (track_id, fields, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(track_id) DO UPDATE SET fields = fields | excluded.fields, updated_at = excluded.updated_at",
        params![id, fields, now_ms()],
    )?;
    Ok(())
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
