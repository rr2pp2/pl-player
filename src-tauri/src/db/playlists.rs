//! プレイリスト（作成・名前変更・削除・曲の追加と削除・並び替え）。
//! 並び替えは位置の振り直しで行う。

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::tracks::{Track, SELECT_TRACK};
use super::{now_ms, Db};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntry {
    /// playlist_items.id（同じ曲が2回入っても区別できるように）
    pub item_id: i64,
    pub track: Track,
}

pub fn list(db: &Db) -> AppResult<Vec<Playlist>> {
    let conn = db.reader();
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, (SELECT COUNT(*) FROM playlist_items i WHERE i.playlist_id = p.id)
         FROM playlists p ORDER BY p.sort_order, p.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Playlist {
            id: r.get(0)?,
            name: r.get(1)?,
            count: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn create(db: &Db, name: &str) -> AppResult<i64> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::other("empty playlist name"));
    }
    let conn = db.writer();
    let next: i64 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM playlists",
        [],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO playlists (name, sort_order, created_at) VALUES (?1, ?2, ?3)",
        params![name, next, now_ms()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn rename(db: &Db, id: i64, name: &str) -> AppResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::other("empty playlist name"));
    }
    db.writer()
        .execute("UPDATE playlists SET name = ?2 WHERE id = ?1", params![id, name])?;
    Ok(())
}

pub fn delete(db: &Db, id: i64) -> AppResult<()> {
    db.writer().execute("DELETE FROM playlists WHERE id = ?1", [id])?;
    Ok(())
}

pub fn add_tracks(db: &Db, id: i64, track_ids: &[i64]) -> AppResult<()> {
    let mut conn = db.writer();
    let tx = conn.transaction()?;
    let mut pos: i64 = tx.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_items WHERE playlist_id = ?1",
        [id],
        |r| r.get(0),
    )?;
    for t in track_ids {
        tx.execute(
            "INSERT INTO playlist_items (playlist_id, position, track_id) VALUES (?1, ?2, ?3)",
            params![id, pos, t],
        )?;
        pos += 1;
    }
    tx.commit()?;
    Ok(())
}

fn renumber(tx: &rusqlite::Transaction<'_>, id: i64, item_ids: &[i64]) -> AppResult<()> {
    for (i, item) in item_ids.iter().enumerate() {
        tx.execute(
            "UPDATE playlist_items SET position = ?3 WHERE id = ?2 AND playlist_id = ?1",
            params![id, item, i as i64],
        )?;
    }
    Ok(())
}

fn item_ids(tx: &rusqlite::Transaction<'_>, id: i64) -> AppResult<Vec<i64>> {
    let mut stmt =
        tx.prepare("SELECT id FROM playlist_items WHERE playlist_id = ?1 ORDER BY position, id")?;
    let rows = stmt.query_map([id], |r| r.get(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn remove_items(db: &Db, id: i64, items: &[i64]) -> AppResult<()> {
    let mut conn = db.writer();
    let tx = conn.transaction()?;
    for it in items {
        tx.execute(
            "DELETE FROM playlist_items WHERE id = ?2 AND playlist_id = ?1",
            params![id, it],
        )?;
    }
    let rest = item_ids(&tx, id)?;
    renumber(&tx, id, &rest)?;
    tx.commit()?;
    Ok(())
}

/// 並び替え：item を to の位置へ動かす
pub fn move_item(db: &Db, id: i64, item: i64, to: usize) -> AppResult<()> {
    let mut conn = db.writer();
    let tx = conn.transaction()?;
    let mut ids = item_ids(&tx, id)?;
    if let Some(from) = ids.iter().position(|x| *x == item) {
        let v = ids.remove(from);
        let to = to.min(ids.len());
        ids.insert(to, v);
        renumber(&tx, id, &ids)?;
    }
    tx.commit()?;
    Ok(())
}

pub fn entries(db: &Db, id: i64) -> AppResult<Vec<PlaylistEntry>> {
    let conn = db.reader();
    let sql = format!(
        "SELECT i.id, x.* FROM playlist_items i JOIN ({SELECT_TRACK}) x ON x.id = i.track_id
         WHERE i.playlist_id = ?1 ORDER BY i.position, i.id"
    );
    // x.* の列順は SELECT_TRACK と同じ。先頭に item id が1つ足されるので、ずらして読む。
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([id], |r| {
        let item_id: i64 = r.get(0)?;
        let track = row_to_track_offset(db, r)?;
        Ok(PlaylistEntry { item_id, track })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// `SELECT i.id, x.*` の行から Track を読む（1列ずれている）
fn row_to_track_offset(db: &Db, r: &rusqlite::Row<'_>) -> rusqlite::Result<Track> {
    // SELECT_TRACK と同じ順で、番号を1つずらして読む
    let art_key: Option<String> = r.get(16)?;
    let override_path: Option<String> = r.get(17)?;
    let art_path = override_path.or_else(|| {
        art_key.map(|k| db.thumbs_dir.join(format!("{k}.jpg")).to_string_lossy().into_owned())
    });
    Ok(Track {
        id: r.get(1)?,
        path: r.get(2)?,
        format: r.get(3)?,
        duration_ms: r.get(4)?,
        title: r.get(5)?,
        artist: r.get(6)?,
        album: r.get(7)?,
        album_artist: r.get(8)?,
        genre: r.get(9)?,
        year: r.get(10)?,
        track_no: r.get(11)?,
        disc_no: r.get(12)?,
        composer: r.get(13)?,
        comment: r.get(14)?,
        has_art: r.get::<_, i64>(15)? != 0,
        art_path,
        album_key: r.get(18)?,
        added_at: r.get(19)?,
        last_played_at: r.get(20)?,
        play_count: r.get(21)?,
        favorite: r.get::<_, i64>(22)? != 0,
        missing: r.get::<_, i64>(23)? != 0,
        tag_error: r.get::<_, i64>(24)? != 0,
    })
}

pub fn track_ids(db: &Db, id: i64) -> AppResult<Vec<i64>> {
    let conn = db.reader();
    let mut stmt = conn.prepare(
        "SELECT track_id FROM playlist_items WHERE playlist_id = ?1 ORDER BY position, id",
    )?;
    let rows = stmt.query_map([id], |r| r.get(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn name_of(db: &Db, id: i64) -> AppResult<Option<String>> {
    let conn = db.reader();
    Ok(conn
        .query_row("SELECT name FROM playlists WHERE id = ?1", [id], |r| r.get(0))
        .optional()?)
}

/// 書き出し用：名前と曲のパス
pub fn export_all(db: &Db) -> AppResult<Vec<(String, Vec<String>)>> {
    let lists = list(db)?;
    let conn = db.reader();
    let mut out = vec![];
    for p in lists {
        let mut stmt = conn.prepare(
            "SELECT t.path FROM playlist_items i JOIN tracks t ON t.id = i.track_id
             WHERE i.playlist_id = ?1 ORDER BY i.position, i.id",
        )?;
        let rows = stmt.query_map([p.id], |r| r.get::<_, String>(0))?;
        out.push((p.name, rows.collect::<Result<Vec<_>, _>>()?));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tracks::{upsert, TrackInfo};

    fn setup(name: &str) -> (Db, Vec<i64>) {
        let db = Db::open_memory(name).unwrap();
        let mut ids = vec![];
        for i in 0..4 {
            let (id, _) = upsert(
                &db.writer(),
                &TrackInfo {
                    path: format!("C:/{i}.mp3"),
                    title: format!("T{i}"),
                    ..Default::default()
                },
            )
            .unwrap();
            ids.push(id);
        }
        (db, ids)
    }

    #[test]
    fn create_rename_delete() {
        let (db, _) = setup("pl_crud");
        let id = create(&db, "  通勤 ").unwrap();
        assert_eq!(list(&db).unwrap()[0].name, "通勤");
        rename(&db, id, "帰り道").unwrap();
        assert_eq!(name_of(&db, id).unwrap().unwrap(), "帰り道");
        assert!(create(&db, "   ").is_err());
        delete(&db, id).unwrap();
        assert!(list(&db).unwrap().is_empty());
    }

    #[test]
    fn add_remove_reorder() {
        let (db, ids) = setup("pl_items");
        let pl = create(&db, "P").unwrap();
        add_tracks(&db, pl, &ids).unwrap();
        add_tracks(&db, pl, &[ids[0]]).unwrap(); // 同じ曲を2回
        let e = entries(&db, pl).unwrap();
        assert_eq!(e.len(), 5);
        assert_eq!(e[0].track.title, "T0");
        // 最後の要素（2回目の T0）を先頭へ
        move_item(&db, pl, e[4].item_id, 0).unwrap();
        let e2 = entries(&db, pl).unwrap();
        assert_eq!(e2[0].item_id, e[4].item_id);
        assert_eq!(e2[1].item_id, e[0].item_id);
        // 削除すると位置が詰まる
        remove_items(&db, pl, &[e2[0].item_id]).unwrap();
        assert_eq!(track_ids(&db, pl).unwrap(), ids);
        // 曲をライブラリから外すと、プレイリストからも消える
        crate::db::tracks::remove(&db, &[ids[1]]).unwrap();
        assert_eq!(track_ids(&db, pl).unwrap().len(), 3);
    }
}
