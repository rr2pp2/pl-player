//! 再生の操作。全部 playback（司令塔）へ渡すだけ。

use tauri::State;

use super::AppState;
use crate::error::AppResult;
use crate::playback::order::Repeat;
use crate::playback::Snapshot;

#[tauri::command]
pub async fn playback_snapshot(state: State<'_, AppState>) -> AppResult<Snapshot> {
    Ok(state.controller.snapshot())
}

#[tauri::command]
pub async fn play_context(state: State<'_, AppState>, ids: Vec<i64>, index: usize) -> AppResult<()> {
    state.controller.play_context(ids, index);
    Ok(())
}

#[tauri::command]
pub async fn playback_action(state: State<'_, AppState>, action: String) -> AppResult<()> {
    let c = &state.controller;
    match action.as_str() {
        "toggle" => c.toggle(),
        "play" => c.play(),
        "pause" => c.pause(),
        "next" => c.next(),
        "previous" => c.previous(),
        "toggleMute" => c.toggle_mute(),
        "toggleShuffle" => c.toggle_shuffle(),
        "cycleRepeat" => c.cycle_repeat(),
        "volumeUp" => c.volume_step(0.05),
        "volumeDown" => c.volume_step(-0.05),
        "seekForward" => c.seek_relative(5_000),
        "seekBackward" => c.seek_relative(-5_000),
        // v3.8.0（13.5 の11）：詳細表示の切替・出力 1〜3 のオン/オフ（hotkeys の dispatch と同じ関数）
        "toggleDetail" => c.toggle_detail(),
        "toggleOutput1" => c.toggle_output(0),
        "toggleOutput2" => c.toggle_output(1),
        "toggleOutput3" => c.toggle_output(2),
        _ => {}
    }
    Ok(())
}

#[tauri::command]
pub async fn playback_seek(state: State<'_, AppState>, ms: u64) -> AppResult<()> {
    state.controller.seek(ms);
    Ok(())
}

#[tauri::command]
pub async fn playback_set_volume(state: State<'_, AppState>, volume: f32) -> AppResult<()> {
    state.controller.set_volume(volume);
    Ok(())
}

#[tauri::command]
pub async fn playback_commit_volume(state: State<'_, AppState>) -> AppResult<()> {
    // v3.8.0：全体の音量は出力先の設定（通常用・詳細用の2組）にも入る
    state.controller.commit_volume();
    Ok(())
}

#[tauri::command]
pub async fn playback_set_muted(state: State<'_, AppState>, muted: bool) -> AppResult<()> {
    state.controller.set_muted(muted);
    Ok(())
}

#[tauri::command]
pub async fn playback_set_shuffle(state: State<'_, AppState>, shuffle: bool) -> AppResult<()> {
    state.controller.set_shuffle(shuffle);
    Ok(())
}

#[tauri::command]
pub async fn playback_set_repeat(state: State<'_, AppState>, repeat: String) -> AppResult<()> {
    state.controller.set_repeat(Repeat::parse(&repeat));
    Ok(())
}
