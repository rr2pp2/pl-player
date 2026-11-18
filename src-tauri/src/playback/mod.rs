//! 再生の進め方（キュー・シャッフル・リピート・前後の曲・状態の保存）。

pub mod controller;
pub mod order;
pub mod output_control;
pub mod outputs;
pub mod queue;
mod state;

pub use controller::{Controller, Notice, PlaybackListener, Snapshot, Status};
pub use output_control::{MeasureSlot, OutputsView, SlotView};
