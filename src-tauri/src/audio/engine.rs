//! 音声エンジンの公開する口。
//!
//! audio は「次の曲」を知らない。渡された1曲を鳴らす・止める・位置を変える・出力先を変える、だけ。
//! やりとりは命令の通り道と通知の通り道の2本だけ（アーキテクチャ案 1章）。

use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::{unbounded, Receiver, Sender};

use super::output::Controls;
use super::thread;
use crate::error::AppError;

#[derive(Debug)]
pub enum Command {
    /// 曲を開く。`token` は playback が付ける通し番号（どの曲の通知かを区別する）
    Load {
        path: PathBuf,
        start_ms: u64,
        play: bool,
        token: u64,
    },
    Play,
    Pause,
    Stop,
    Seek(u64),
    /// None なら既定のデバイス
    SetDevice(Option<String>),
    Shutdown,
    /// テストだけ：音声スレッドの中でわざと panic させる（v3.1.1。スレッドが止まらないことを確かめる）
    #[cfg(test)]
    TestPanic,
}

/// 音声スレッドの中だけで使う知らせ（cpal のエラーコールバック・デバイス通知から）
#[derive(Debug)]
pub enum Internal {
    StreamError(cpal::ErrorKind, String),
    DevicesChanged,
}

#[derive(Debug, Clone)]
pub enum Event {
    Loaded {
        token: u64,
        duration_ms: u64,
    },
    Position {
        token: u64,
        position_ms: u64,
        duration_ms: u64,
    },
    Playing(bool),
    Ended {
        token: u64,
    },
    Error {
        token: Option<u64>,
        error: AppError,
    },
    /// 出力先が決まった・変わった。`fallback_from` があれば切断による切り替え
    DeviceChanged {
        device_id: Option<String>,
        name: String,
        fallback_from: Option<String>,
    },
    DevicesChanged,
}

pub struct AudioEngine {
    cmd: Sender<Command>,
    internal: Sender<Internal>,
    events: Receiver<Event>,
    controls: Arc<Controls>,
}

impl AudioEngine {
    pub fn start(volume: f32, muted: bool, device: Option<String>) -> Self {
        let (cmd_tx, cmd_rx) = unbounded();
        let (int_tx, int_rx) = unbounded();
        let (ev_tx, ev_rx) = unbounded();
        let controls = Arc::new(Controls::new(volume, muted));
        thread::spawn(cmd_rx, int_rx, int_tx.clone(), ev_tx, controls.clone(), device);
        Self {
            cmd: cmd_tx,
            internal: int_tx,
            events: ev_rx,
            controls,
        }
    }

    pub fn send(&self, c: Command) {
        let _ = self.cmd.send(c);
    }

    pub fn events(&self) -> Receiver<Event> {
        self.events.clone()
    }

    /// デバイス一覧が変わった（Windows の通知から）
    pub fn notify_devices_changed(&self) {
        let _ = self.internal.send(Internal::DevicesChanged);
    }

    pub fn internal_sender(&self) -> Sender<Internal> {
        self.internal.clone()
    }

    /// 音量とミュートは audio の中で掛ける（OS の音量ミキサーは触らない）
    pub fn set_volume(&self, v: f32) {
        self.controls.set_volume(v);
    }

    pub fn set_muted(&self, m: bool) {
        self.controls
            .muted
            .store(m, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        let _ = self.cmd.send(Command::Shutdown);
    }
}
