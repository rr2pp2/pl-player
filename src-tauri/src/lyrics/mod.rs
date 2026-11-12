//! 歌詞（v3）。曲ファイルには書き込まない。歌詞のタグは読むだけ。
//!
//! - `lrc`：読み方（純粋な関数）
//! - `resolver`：manual ＞ external ＞ embedded_tag の優先順
//! - `inbox`：受け渡しフォルダ（外部ツール → アプリ）の取り込み
//! - `service`：上の組み合わせ（commands から呼ぶ）
//! - `lrclib`：歌詞をネットから取る決まり（v3.1.0。通信しない部分。元は非公開の取得スクリプト）
//! - `online`：歌詞をネットから取る一括と1曲（設定の確認・送る曲の選び方・保存）

pub mod inbox;
pub mod lrc;
pub mod lrclib;
pub mod online;
pub mod resolver;
pub mod service;
