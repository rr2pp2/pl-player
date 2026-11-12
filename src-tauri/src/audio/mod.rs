//! 音声エンジン（専用スレッドで動く）。
//! デコード（Symphonia＋libopus）→ レート変換（rubato）→ 受け渡しの輪（rtrb）→ 出力（cpal / WASAPI）。

pub mod decoder;
pub mod devices;
pub mod engine;
pub mod output;
pub mod resample;
mod thread;

pub use engine::{AudioEngine, Command, Event};
