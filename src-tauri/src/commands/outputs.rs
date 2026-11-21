//! 出力先（同時再生）・表示のモード・メーター（v3.8.0。仕様「同時再生と配信モード」）。
//! 全部 playback（司令塔）へ渡すだけ。出力先の設定を書くのは Rust だけで、変わるたびに `pl://outputs` で全部を送る。
//!
//! メーター（13.5 の13）：画面が `meter_start` に Tauri の Channel を渡すと、約 33ms ごとに枠3つの左右の最大
//! （音量を掛けたあと・切り詰める前。1.0 がフルスケール）を送る。`meter_stop` か、次の `meter_start` で止まる。
//! 窓が最小化されている・隠れている間は送らない（値は読み捨てる）。dB への換算・最大の線・音割れの印は画面側。
//! 送る中身は小さい JSON なので、Tauri は WebView へ直接書き込む（`plugin:__TAURI_CHANNEL__|fetch` を通らない。
//! tauri 2.12.1 の `ipc/channel.rs`：8192 バイト未満は eval）。権限の設定は増やさない

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use super::AppState;
use crate::audio::devices::list_output_devices;
use crate::audio::sync::tuning;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::playback::outputs::{DeviceRef, Mode, NAME_MAX_CHARS};
use crate::playback::OutputsView;

/// メーターの送り役（同時に1つだけ。番号が変わったら前のものは止まる）。
/// v3.8.0：開始と停止は画面が付けた通し番号（seq。開始・停止のたびに増える）で順番を決める。
/// コマンドは非同期なので「開始→停止→開始」の実行が入れ替わりうる。いちばん大きい番号だけが効き、遅れて着いた古い開始・停止は何もしない
#[derive(Default)]
pub struct MeterControl {
    gen: AtomicU64,
}

impl MeterControl {
    /// 開始：これまでの開始・停止のどれより新しい番号なら true（その番号で送り始める）。古ければ false（何もしない）
    pub fn begin(&self, seq: u64) -> bool {
        self.gen.fetch_max(seq, Ordering::SeqCst) < seq
    }

    /// 停止：これより古い番号の送り役を止める。新しい開始のあとに遅れて着いた停止は何もしない
    pub fn end(&self, seq: u64) {
        self.gen.fetch_max(seq, Ordering::SeqCst);
    }

    /// その番号の送り役が、まだ送ってよいか
    pub fn alive(&self, seq: u64) -> bool {
        self.gen.load(Ordering::SeqCst) == seq
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterFrame {
    /// 枠3つの [左, 右] の最大（オフ・切断中は 0）
    pub peaks: [[f32; 2]; 3],
}

/// 一覧から名前を引く（WASAPI の列挙は COM を使うので、専用のスレッドで行う。devices_list と同じ）
fn device_ref(id: Option<String>) -> DeviceRef {
    let name = match id.clone() {
        None => String::new(),
        Some(x) => std::thread::spawn(move || list_output_devices().into_iter().find(|d| d.id == x).map(|d| d.name))
            .join()
            .ok()
            .flatten()
            .unwrap_or_default(),
    };
    DeviceRef { id, name: name.chars().take(NAME_MAX_CHARS).collect() }
}

fn check_id(id: &Option<String>) -> AppResult<()> {
    match id {
        Some(s) if s.trim().is_empty() || s.chars().count() > crate::playback::outputs::ID_MAX_CHARS => {
            Err(AppError::new(ErrorKind::OutputInvalid, "invalid device id"))
        }
        _ => Ok(()),
    }
}

#[tauri::command]
pub async fn outputs_get(state: State<'_, AppState>) -> AppResult<OutputsView> {
    Ok(state.controller.outputs_view())
}

/// 出力先を足す（`deviceId` が null なら既定のデバイス）。足した枠の番号を返す
#[tauri::command]
pub async fn output_add(state: State<'_, AppState>, device_id: Option<String>) -> AppResult<usize> {
    check_id(&device_id)?;
    state.controller.output_add(device_ref(device_id))
}

#[tauri::command]
pub async fn output_remove(state: State<'_, AppState>, slot: usize) -> AppResult<()> {
    state.controller.output_remove(slot)
}

#[tauri::command]
pub async fn output_set_on(state: State<'_, AppState>, slot: usize, on: bool) -> AppResult<()> {
    state.controller.output_set_on(slot, on)
}

/// 出力ごとの音量（動かしている間）。保存は `output_commit_volume`
#[tauri::command]
pub async fn output_set_volume(state: State<'_, AppState>, slot: usize, volume: f32) -> AppResult<()> {
    state.controller.output_set_volume(slot, volume)
}

#[tauri::command]
pub async fn output_commit_volume(state: State<'_, AppState>) -> AppResult<()> {
    state.controller.output_commit_volume();
    Ok(())
}

#[tauri::command]
pub async fn output_set_delay(state: State<'_, AppState>, slot: usize, delay_ms: u32) -> AppResult<()> {
    state.controller.output_set_delay(slot, delay_ms)
}

#[tauri::command]
pub async fn output_set_follow(state: State<'_, AppState>, slot: usize, follow: bool) -> AppResult<()> {
    state.controller.output_set_follow(slot, follow)
}

/// アイコンの選び直し（headphones / speaker / stream / monitor。null で自動）
#[tauri::command]
pub async fn output_set_icon(state: State<'_, AppState>, slot: usize, icon: Option<String>) -> AppResult<()> {
    state.controller.output_set_icon(slot, icon)
}

/// デバイスの差し替え（`deviceId` が null なら既定のデバイス）
#[tauri::command]
pub async fn output_set_device(state: State<'_, AppState>, slot: usize, device_id: Option<String>) -> AppResult<()> {
    check_id(&device_id)?;
    state.controller.output_set_device(slot, device_ref(device_id))
}

/// 表示のモード（"normal" / "detail"）
#[tauri::command]
pub async fn view_mode_set(state: State<'_, AppState>, mode: Mode) -> AppResult<()> {
    state.controller.set_mode(mode);
    Ok(())
}

/// メーターを送り始める（画面にメーターが出ている間だけ呼ぶ）。`seq` は画面が付けた通し番号（MeterControl）
#[tauri::command]
pub async fn meter_start(app: AppHandle, state: State<'_, AppState>, seq: u64, on_frame: Channel<MeterFrame>) -> AppResult<()> {
    let my = seq;
    if !state.meter.begin(my) {
        return Ok(());
    }
    let bank = state.controller.meter_bank();
    std::thread::Builder::new()
        .name("pl-meter".into())
        .spawn(move || {
            let mut tick: u64 = 0;
            let mut hidden = false;
            loop {
                std::thread::sleep(Duration::from_millis(tuning::METER_INTERVAL_MS));
                let Some(st) = app.try_state::<AppState>() else { break };
                if !st.meter.alive(my) {
                    break;
                }
                // 最小化・隠れているかは、約 0.3 秒ごとに確かめる（窓へ聞くのは軽くないため）
                if tick % 10 == 0 {
                    hidden = app
                        .get_webview_window("main")
                        .map(|w| w.is_minimized().unwrap_or(false) || !w.is_visible().unwrap_or(true))
                        .unwrap_or(true);
                }
                tick += 1;
                let peaks = bank.take_peaks();
                if hidden {
                    continue;
                }
                if on_frame.send(MeterFrame { peaks }).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| AppError::other(e.to_string()))?;
    Ok(())
}

/// メーターを止める。`seq` は画面が付けた通し番号（これより古い開始の送り役を止める）
#[tauri::command]
pub async fn meter_stop(state: State<'_, AppState>, seq: u64) -> AppResult<()> {
    state.meter.end(seq);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::MeterControl;

    /// 開始・停止の実行が入れ替わっても、いちばん新しい番号だけが効く
    #[test]
    fn meter_start_stop_order() {
        // 順番どおり：開始1 → 停止2 → 開始3
        let m = MeterControl::default();
        assert!(m.begin(1));
        assert!(m.alive(1));
        m.end(2);
        assert!(!m.alive(1));
        assert!(m.begin(3));
        assert!(m.alive(3));
        // 入れ替わり：開始3 が先に着き、そのあと古い開始1・停止2 が着いても、開始3 は止まらない
        let m = MeterControl::default();
        assert!(m.begin(3));
        assert!(!m.begin(1));
        m.end(2);
        assert!(m.alive(3));
        assert!(!m.alive(1));
        // 停止のあとに遅れて着いた古い開始は送り始めない
        let m = MeterControl::default();
        m.end(5);
        assert!(!m.begin(4));
        assert!(!m.alive(4));
        // 新しい開始は前の送り役を止める
        let m = MeterControl::default();
        assert!(m.begin(10));
        assert!(m.begin(11));
        assert!(!m.alive(10) && m.alive(11));
    }
}
