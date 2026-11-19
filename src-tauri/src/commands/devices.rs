//! 再生デバイスの一覧。出力先のデバイスの切り替えは、出力先ごとに commands::outputs::output_set_device（v3.8.0）。

use crate::audio::devices::{list_output_devices, DeviceInfo};
use crate::error::AppResult;

#[tauri::command]
pub async fn devices_list() -> AppResult<Vec<DeviceInfo>> {
    // WASAPI の列挙は COM を使うので、専用のスレッドで行う
    let list = std::thread::spawn(list_output_devices).join().unwrap_or_default();
    Ok(list)
}
