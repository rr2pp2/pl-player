//! キュー：キュー追加・次に再生・削除・並び替え・全削除。

use tauri::State;

use super::AppState;
use crate::db::tracks::{self, Track};
use crate::error::AppResult;

#[tauri::command]
pub async fn queue_get(state: State<'_, AppState>) -> AppResult<Vec<Track>> {
    let ids = state.controller.queue_ids();
    // 同じ曲が何度入っていても、その数だけ返す
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(t) = tracks::get(&state.db, id)? {
            out.push(t);
        }
    }
    Ok(out)
}

#[tauri::command]
pub async fn queue_add(state: State<'_, AppState>, ids: Vec<i64>) -> AppResult<()> {
    state.controller.queue_add(&ids);
    Ok(())
}

#[tauri::command]
pub async fn queue_play_next(state: State<'_, AppState>, ids: Vec<i64>) -> AppResult<()> {
    state.controller.queue_play_next(&ids);
    Ok(())
}

#[tauri::command]
pub async fn queue_remove(state: State<'_, AppState>, positions: Vec<usize>) -> AppResult<()> {
    state.controller.queue_remove(&positions);
    Ok(())
}

#[tauri::command]
pub async fn queue_move(state: State<'_, AppState>, from: usize, to: usize) -> AppResult<()> {
    state.controller.queue_move(from, to);
    Ok(())
}

#[tauri::command]
pub async fn queue_clear(state: State<'_, AppState>) -> AppResult<()> {
    state.controller.queue_clear();
    Ok(())
}

#[tauri::command]
pub async fn queue_play_at(state: State<'_, AppState>, position: usize) -> AppResult<()> {
    state.controller.queue_play_at(position);
    Ok(())
}
