//! 音声エンジン（専用スレッドで動く）。
//! デコード（Symphonia＋libopus）→ 曲の側の輪（2ch）→ 出力ごとの変換（rubato）→ 受け渡しの輪（rtrb）→ 出力（cpal / WASAPI）。
//! v3.8.0：出力は最大3つ。仮想の再生時計に合わせて、どの出力も同じ時刻に同じ所を鳴らす（仕様「同時再生と配信モード」13章）

pub mod decoder;
pub mod devices;
pub mod engine;
mod lane;
pub mod latency;
pub mod output;
pub mod resample;
pub mod song_ring;
pub mod sync;
mod thread;

pub use engine::{AudioEngine, Command, Event, OutputSpec, OutputSpecs, OutputStatus, UnavailableReason};
pub use output::{OutputBank, SLOTS};
