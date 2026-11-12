//! プレイリスト：作成・名前変更・削除・曲追加・曲削除・並び替え・再生（シャッフル・リピートは再生側）。

use tauri::{AppHandle, Emitter, State};

use super::{ev, AppState};
use crate::db::playlists::{self, Playlist, PlaylistEntry};
use crate::error::AppResult;

fn changed(app: &AppHandle) {
    let _ = app.emit(ev::PLAYLISTS, ());
}

#[tauri::command]
pub async fn playlists_list(state: State<'_, AppState>) -> AppResult<Vec<Playlist>> {
    playlists::list(&state.db)
}

#[tauri::command]
pub async fn playlist_create(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    track_ids: Option<Vec<i64>>,
) -> AppResult<i64> {
    let id = playlists::create(&state.db, &name)?;
    if let Some(ids) = track_ids {
        playlists::add_tracks(&state.db, id, &ids)?;
    }
    changed(&app);
    Ok(id)
}

#[tauri::command]
pub async fn playlist_rename(app: AppHandle, state: State<'_, AppState>, id: i64, name: String) -> AppResult<()> {
    playlists::rename(&state.db, id, &name)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn playlist_delete(app: AppHandle, state: State<'_, AppState>, id: i64) -> AppResult<()> {
    playlists::delete(&state.db, id)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn playlist_add_tracks(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    track_ids: Vec<i64>,
) -> AppResult<()> {
    playlists::add_tracks(&state.db, id, &track_ids)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn playlist_remove_items(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    item_ids: Vec<i64>,
) -> AppResult<()> {
    playlists::remove_items(&state.db, id, &item_ids)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn playlist_move_item(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    item_id: i64,
    to: usize,
) -> AppResult<()> {
    playlists::move_item(&state.db, id, item_id, to)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn playlist_entries(state: State<'_, AppState>, id: i64) -> AppResult<Vec<PlaylistEntry>> {
    playlists::entries(&state.db, id)
}

/// プレイリストを再生。`shuffle` を渡すと、その設定にしてから再生する
#[tauri::command]
pub async fn playlist_play(
    state: State<'_, AppState>,
    id: i64,
    index: Option<usize>,
    shuffle: Option<bool>,
) -> AppResult<()> {
    let ids = playlists::track_ids(&state.db, id)?;
    if ids.is_empty() {
        return Ok(());
    }
    if let Some(s) = shuffle {
        state.controller.set_shuffle(s);
    }
    let start = match (index, shuffle) {
        (Some(i), _) => i,
        (None, Some(true)) => {
            let mut rng = crate::playback::order::Rng::from_time();
            rng.below(ids.len())
        }
        _ => 0,
    };
    state.controller.play_context(ids, start);
    Ok(())
}
