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

/// None なら「既定のデバイス（Windows の設定に従う）」。
/// v3.8.0：出力先1のデバイスを差し替える（3.7.0 の設定画面の「再生デバイス」。段2-2 で出力先3つの選び方に作り直す）
#[tauri::command]
pub async fn device_set(state: State<'_, AppState>, id: Option<String>) -> AppResult<()> {
    let name = match id.clone() {
        None => String::new(),
        Some(x) => std::thread::spawn(move || list_output_devices().into_iter().find(|d| d.id == x).map(|d| d.name))
            .join()
            .ok()
            .flatten()
            .unwrap_or_default(),
    };
    state.controller.set_device(id, name);
    Ok(())
}
