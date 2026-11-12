//! 再生デバイス：一覧と切り替え（再生中でも、曲と位置を保つ）。

use tauri::State;

use super::AppState;
use crate::audio::devices::{list_output_devices, DeviceInfo};
use crate::error::AppResult;

#[tauri::command]
pub async fn devices_list() -> AppResult<Vec<DeviceInfo>> {
    // WASAPI の列挙は COM を使うので、専用のスレッドで行う
    let list = std::thread::spawn(list_output_devices).join().unwrap_or_default();
    Ok(list)
}

/// None なら「既定のデバイス（Windows の設定に従う）」
#[tauri::command]
pub async fn device_set(state: State<'_, AppState>, id: Option<String>) -> AppResult<()> {
    state.controller.set_device(id);
    Ok(())
}
