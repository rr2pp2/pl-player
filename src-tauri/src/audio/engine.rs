//! 音声エンジンの公開する口。
//!
//! audio は「次の曲」を知らない。渡された1曲を鳴らす・止める・位置を変える・出力先を変える、だけ。
//! やりとりは命令の通り道と通知の通り道の2本だけ（アーキテクチャ案 1章）。
//!
//! v3.8.0（同時再生）：出力先は枠3つ（`OutputSpec`）。枠ごとの音量とメーターは原子変数（`OutputBank`）で渡す。
//! どの出力を開くか・つなぎ直し・時計とずれの直しは、音声スレッド（thread.rs）が決める。

use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::{unbounded, Receiver, Sender};
use serde::Serialize;

use super::output::{OpenFail, OutputBank, OutputStream, SLOTS};
use super::thread;
use crate::error::AppError;

/// 枠1つぶんの、音声スレッドが知るべき設定（音量は `OutputBank` で別に渡す）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputSpec {
    /// None なら既定のデバイス（Windows の Console の役割に従う）
    pub device_id: Option<String>,
    pub on: bool,
    /// 遅らせ量（0〜1000ms）
    pub delay_ms: u32,
}

pub type OutputSpecs = [Option<OutputSpec>; SLOTS];

/// 1つだけの出力（3.7.0 と同じ使い方。テストと、出力の設定が無いとき）
pub fn single_output(device_id: Option<String>) -> OutputSpecs {
    [Some(OutputSpec { device_id, on: true, delay_ms: 0 }), None, None]
}

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
    /// 出力先の枠3つを入れ替える（v3.8.0。3.7.0 の SetDevice の代わり）
    SetOutputs(OutputSpecs),
    Shutdown,
    /// テストだけ：音声スレッドの中でわざと panic させる（v3.1.1。スレッドが止まらないことを確かめる）
    #[cfg(test)]
    TestPanic,
}

/// 音声スレッドの中だけで使う知らせ（cpal のエラーコールバック・デバイス通知から）
#[derive(Debug)]
pub enum Internal {
    /// `serial` はストリームの番号（閉じたストリームの遅れた知らせを見分ける）
    StreamError { slot: usize, serial: u64, kind: cpal::ErrorKind, msg: String },
    DevicesChanged,
    /// デバイスを開く役（別のスレッド）からの結果（検査1回目の指摘4：開くのに時間がかかっても、ほかの出力を止めない）。
    /// `present` は、開けなかったときにそのデバイスが一覧にあったか（一覧にある＝開けません／無い＝切断中。13.5 の12）
    Opened { slot: usize, serial: u64, result: Result<Box<OutputStream>, OpenFail>, present: bool },
}

/// 出力先の状態（仕様 6.4）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStatus {
    /// オフ（デバイスを手放している）
    Off,
    /// 鳴らせる（開いている、または一覧にあって開ける見込み）
    Ok,
    /// 切断中（一覧に無い。その PC に無いデバイスも同じ）
    Disconnected,
    /// 開けません（つながっているのに開けない）
    Unavailable,
    /// 重複（既定のデバイスが、ほかの出力先と同じデバイスになった）
    Duplicate,
}

/// 「開けません」の理由（13.5 の7）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    /// このデバイスの形式は使えません
    Format,
    /// ほかのアプリが独り占めしています
    Busy,
    /// このデバイスを開けませんでした
    Other,
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
    /// 出力先の状態が変わった（v3.8.0）
    Output {
        slot: usize,
        status: OutputStatus,
        reason: Option<UnavailableReason>,
        /// 実際のデバイスの名前（既定のデバイスなら、今の既定のデバイスの名前）
        name: Option<String>,
        /// 実際のデバイスの ID
        resolved_id: Option<String>,
        /// 自動のアイコン
        kind: Option<&'static str>,
    },
    /// 抜けていた出力先が戻った（「○○が戻りました」。2.5）
    OutputReturned {
        slot: usize,
        name: String,
    },
    /// オンの出力先が全部鳴らせなくなったので一時停止した（2.5）
    OutputsAllLost,
    /// 自前の時計と出力の時計がそろわないので、出力どうしのずれの直しを切った（13.1 の2）
    SyncUnavailable,
    DevicesChanged,
}

pub struct AudioEngine {
    cmd: Sender<Command>,
    internal: Sender<Internal>,
    events: Receiver<Event>,
    bank: Arc<OutputBank>,
}

impl AudioEngine {
    /// `gains` は枠ごとの実際に掛ける倍率（司令塔が「全体 × 出力ごと」の2乗とミュートを済ませた値）
    pub fn start(outputs: OutputSpecs, gains: [f32; SLOTS]) -> Self {
        let (cmd_tx, cmd_rx) = unbounded();
        let (int_tx, int_rx) = unbounded();
        let (ev_tx, ev_rx) = unbounded();
        let bank = Arc::new(OutputBank::new());
        for (i, g) in gains.iter().enumerate() {
            bank.set_gain(i, *g);
        }
        thread::spawn(cmd_rx, int_rx, int_tx.clone(), ev_tx, bank.clone(), outputs);
        Self { cmd: cmd_tx, internal: int_tx, events: ev_rx, bank }
    }

    /// 出力1つ（3.7.0 と同じ形）で始める。`gain` は実際に掛ける倍率
    pub fn start_single(gain: f32, device: Option<String>) -> Self {
        Self::start(single_output(device), [gain, 0.0, 0.0])
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

    /// 枠ごとの音量（実際に掛ける倍率）。audio の中で掛ける（OS の音量ミキサーは触らない）
    pub fn set_output_gain(&self, slot: usize, gain: f32) {
        self.bank.set_gain(slot, gain);
    }

    /// メーターを読むため
    pub fn bank(&self) -> Arc<OutputBank> {
        self.bank.clone()
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        let _ = self.cmd.send(Command::Shutdown);
    }
}
