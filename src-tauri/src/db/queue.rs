//! キュー（queue_items）の保存と読み出し。
//! 並びの規則そのものは `playback::queue` が持つ。ここは「今の並びを丸ごと書く・読む」だけ。

use super::Db;
use crate::error::AppResult;

pub fn load(db: &Db) -> AppResult<Vec<i64>> {
    let conn = db.reader();
    let mut stmt = conn.prepare("SELECT track_id FROM queue_items ORDER BY position, id")?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn save(db: &Db, ids: &[i64]) -> AppResult<()> {
    let mut conn = db.writer();
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM queue_items", [])?;
    for (i, id) in ids.iter().enumerate() {
        tx.execute(
            "INSERT INTO queue_items (position, track_id) VALUES (?1, ?2)",
            rusqlite::params![i as i64, id],
        )?;
    }
    tx.commit()?;
    Ok(())
}
