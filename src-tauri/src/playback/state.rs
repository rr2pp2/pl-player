//! 状態の保存と復元（アーキテクチャ案 8章）。
//!
//! 保存の時機：終了するとき・曲が変わったとき・10秒ごと（位置。controller の通知処理から）。
//! 復元：起動時に controller が `db::state::load` で読み、一時停止で位置だけ戻す（自動では再生しない）。

use super::controller::Controller;
use crate::db;

impl Controller {
    pub fn save_state(&self) {
        let s = {
            let i = self.lock();
            db::state::SavedPlayback {
                track_id: i.current.as_ref().map(|t| t.id),
                position_ms: i.position_ms as i64,
                volume: i.volume as f64,
                muted: i.muted,
                shuffle: i.shuffle,
                repeat_mode: i.repeat.as_str().into(),
                // v3.8.0：出力先1のデバイス（3.7.0 に戻したときに、その版が使う値。出力先の設定の本体は `outputs`）
                device_id: i.outputs.slot(0).and_then(|s| s.device.id.clone()),
                context: i.context.clone(),
                context_index: i.order.get(i.order_pos).copied().unwrap_or(0) as i64,
            }
        };
        if let Err(e) = db::state::save(&self.db, &s) {
            tracing::warn!("save state: {e}");
        }
    }

    /// 終了時：状態とキューを全部書く（v3.8.0：出力先の設定も）
    pub fn save_all(&self) {
        self.save_state();
        self.save_outputs();
        let ids = self.queue_ids();
        let _ = db::queue::save(&self.db, &ids);
    }

}
