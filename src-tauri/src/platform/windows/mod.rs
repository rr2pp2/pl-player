//! Windows 連携：SMTC・サムネイルボタン・デバイス通知・グローバルホットキー・トレイ。
//! v3.1.0：歌詞をネットから取るときの通信（winhttp）と、名前の照合に使う文字の正規化（unicode）。
//! v3.1.1：この PC のドライブにある場所かの確かめ（localpath）。

pub mod device_watch;
pub mod hotkeys;
pub mod localpath;
pub mod smtc;
pub mod thumbbar;
pub mod tray;
pub mod unicode;
pub mod winhttp;
