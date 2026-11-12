//! グローバルホットキー（アプリがバックグラウンドでも効く）。Tauri 公式プラグインで登録する。
//!
//! 既定では1つも登録しない（Space などをグローバルに取ると他のアプリが壊れるため）。
//! 他のアプリが使っているキーは登録に失敗するので、その結果を画面へ返して「競合」として知らせる。

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::commands::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    pub action: String,
    pub accelerator: String,
    pub registered: bool,
    pub error: Option<String>,
}

static LAST: Mutex<Vec<HotkeyStatus>> = Mutex::new(Vec::new());

/// 動作の名前 → 再生の司令塔の操作
pub fn dispatch(app: &AppHandle, action: &str) {
    let state = app.state::<AppState>();
    let c = &state.controller;
    match action {
        "playPause" => c.toggle(),
        "next" => c.next(),
        "previous" => c.previous(),
        "seekForward" => c.seek_relative(5_000),
        "seekBackward" => c.seek_relative(-5_000),
        "volumeUp" => c.volume_step(0.05),
        "volumeDown" => c.volume_step(-0.05),
        "mute" => c.toggle_mute(),
        "shuffle" => c.toggle_shuffle(),
        "repeat" => c.cycle_repeat(),
        _ => {}
    }
}

/// 登録し直す（全部外してから、設定にあるものを1つずつ）
pub fn apply(app: &AppHandle, map: &BTreeMap<String, String>) -> Vec<HotkeyStatus> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut out = vec![];
    // 同じキーを2つの動作に割り当てていないか
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for (action, accel) in map {
        let accel = accel.trim().to_string();
        if accel.is_empty() {
            continue;
        }
        let norm = accel.to_ascii_uppercase();
        if let Some(other) = seen.get(&norm) {
            out.push(HotkeyStatus {
                action: action.clone(),
                accelerator: accel,
                registered: false,
                error: Some(format!("duplicate:{other}")),
            });
            continue;
        }
        let act = action.clone();
        let r = gs.on_shortcut(accel.as_str(), move |app, _sc, event| {
            if event.state() == ShortcutState::Pressed {
                dispatch(app, &act);
            }
        });
        match r {
            Ok(()) => {
                seen.insert(norm, action.clone());
                out.push(HotkeyStatus {
                    action: action.clone(),
                    accelerator: accel,
                    registered: true,
                    error: None,
                });
            }
            Err(e) => {
                tracing::info!("hotkey {accel} for {action} failed: {e}");
                out.push(HotkeyStatus {
                    action: action.clone(),
                    accelerator: accel,
                    registered: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }
    *LAST.lock().unwrap_or_else(|p| p.into_inner()) = out.clone();
    out
}

pub fn status(app: &AppHandle, map: &BTreeMap<String, String>) -> Vec<HotkeyStatus> {
    let last = LAST.lock().unwrap_or_else(|p| p.into_inner()).clone();
    if last.is_empty() && !map.is_empty() {
        return apply(app, map);
    }
    last
}
