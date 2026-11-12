//! 再生状態（playback_state、1行だけ）。

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::Db;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SavedPlayback {
    pub track_id: Option<i64>,
    pub position_ms: i64,
    pub volume: f64,
    pub muted: bool,
    pub shuffle: bool,
    pub repeat_mode: String,
    pub device_id: Option<String>,
    /// 今流している並び（曲一覧・プレイリストから再生したときの曲 ID の列）
    pub context: Vec<i64>,
    pub context_index: i64,
}

pub fn load(db: &Db) -> AppResult<SavedPlayback> {
    let conn = db.reader();
    let s = conn.query_row(
        "SELECT track_id, position_ms, volume, muted, shuffle, repeat_mode, device_id, context_json, context_index
         FROM playback_state WHERE id = 1",
        [],
        |r| {
            let ctx: String = r.get(7)?;
            Ok(SavedPlayback {
                track_id: r.get(0)?,
                position_ms: r.get(1)?,
                volume: r.get(2)?,
                muted: r.get::<_, i64>(3)? != 0,
                shuffle: r.get::<_, i64>(4)? != 0,
                repeat_mode: r.get(5)?,
                device_id: r.get(6)?,
                context: serde_json::from_str(&ctx).unwrap_or_default(),
                context_index: r.get(8)?,
            })
        },
    )?;
    Ok(s)
}

pub fn save(db: &Db, s: &SavedPlayback) -> AppResult<()> {
    db.writer().execute(
        "UPDATE playback_state SET track_id=?1, position_ms=?2, volume=?3, muted=?4, shuffle=?5,
         repeat_mode=?6, device_id=?7, context_json=?8, context_index=?9 WHERE id = 1",
        params![
            s.track_id,
            s.position_ms,
            s.volume,
            s.muted as i64,
            s.shuffle as i64,
            s.repeat_mode,
            s.device_id,
            serde_json::to_string(&s.context)?,
            s.context_index
        ],
    )?;
    Ok(())
}

/// 位置だけ（10秒ごとの保存用）
pub fn save_position(db: &Db, track_id: Option<i64>, position_ms: i64) -> AppResult<()> {
    db.writer().execute(
        "UPDATE playback_state SET track_id=?1, position_ms=?2 WHERE id = 1",
        params![track_id, position_ms],
    )?;
    Ok(())
}
