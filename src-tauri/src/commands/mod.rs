//! 画面との窓口。薄く保ち、判断を持たない（判断は playback / library / settings が持つ）。

pub mod devices;
pub mod library;
pub mod lyrics;
pub mod metadata;
pub mod playback;
pub mod playlist;
pub mod queue;
pub mod settings;
pub mod skins;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::db::Db;
use crate::playback::{Controller, Notice, PlaybackListener, Snapshot};
use crate::settings::store::AppSettings;

/// 画面へ送る通知の名前（src/ipc/events.ts と同じ）
pub mod ev {
    pub const PLAYBACK: &str = "pl://playback";
    pub const POSITION: &str = "pl://position";
    pub const QUEUE: &str = "pl://queue";
    pub const NOTICE: &str = "pl://notice";
    pub const DEVICES: &str = "pl://devices";
    pub const LIBRARY: &str = "pl://library";
    pub const PLAYLISTS: &str = "pl://playlists";
    pub const IMPORT_PROGRESS: &str = "pl://import-progress";
    pub const IMPORT_DONE: &str = "pl://import-done";
    pub const HOTKEYS: &str = "pl://hotkeys";
    pub const SETTINGS: &str = "pl://settings";
    pub const UI_COMMAND: &str = "pl://ui-command";
    /// v3.1.0：歌詞をネットから取る（一括）の進み具合と終わり
    pub const LYRICS_FETCH_PROGRESS: &str = "pl://lyrics-fetch-progress";
    pub const LYRICS_FETCH_DONE: &str = "pl://lyrics-fetch-done";
    /// v3.2.0：着せ替えの状態（適用・既定に戻す・セーフモード）
    pub const SKIN: &str = "pl://skin";
    /// v3.5.0：古い文字コードのタグの読み直しの進み具合と終わり
    pub const LEGACY_REREAD_PROGRESS: &str = "pl://legacy-reread-progress";
    pub const LEGACY_REREAD_DONE: &str = "pl://legacy-reread-done";
}

pub struct Paths {
    pub data_dir: PathBuf,
    pub thumbs_dir: PathBuf,
    pub art_dir: PathBuf,
    pub bg_dir: PathBuf,
}

pub struct AppState {
    pub db: Arc<Db>,
    pub controller: Arc<Controller>,
    pub paths: Paths,
    pub settings: Mutex<AppSettings>,
    /// 取り込みが走っている間は true（二重に走らせない）
    pub importing: Mutex<bool>,
    /// v3.1.0：歌詞をネットから取る処理（同時に1つだけ・やめるボタン・要求の間隔・進み具合）
    pub lyrics_fetch: crate::lyrics::online::Control,
    /// v3.2.0：着せ替えの、起動中だけの状態（セーフモード・確認待ちの取り込み・いま当てているスキン）
    pub skin: Mutex<crate::skins::runtime::Runtime>,
    /// v3.5.0：古い文字コードのタグの読み直し（同時に1つだけ・取り込みとは同時に走らせない・やめるボタン・進み具合）
    pub legacy_reread: crate::library::legacy_reread::Control,
}

impl AppState {
    pub fn settings(&self) -> AppSettings {
        self.settings.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

/// 再生の司令塔からの知らせを、画面へ流す
pub struct UiEmitter(pub AppHandle);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PositionPayload {
    position_ms: u64,
    duration_ms: u64,
}

impl PlaybackListener for UiEmitter {
    fn on_snapshot(&self, s: &Snapshot) {
        let _ = self.0.emit(ev::PLAYBACK, s);
    }
    fn on_position(&self, position_ms: u64, duration_ms: u64) {
        let _ = self.0.emit(ev::POSITION, PositionPayload { position_ms, duration_ms });
    }
    fn on_queue_changed(&self) {
        let _ = self.0.emit(ev::QUEUE, ());
    }
    fn on_notice(&self, n: &Notice) {
        let _ = self.0.emit(ev::NOTICE, n);
    }
    fn on_devices_changed(&self) {
        let _ = self.0.emit(ev::DEVICES, ());
    }
}

pub fn notice_error(app: &AppHandle, e: &crate::error::AppError) {
    let _ = app.emit(ev::NOTICE, Notice::error(e));
}
