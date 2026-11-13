//! 取り込み・タグ・アート。

pub mod artwork;
pub mod import;
/// v3.5.0：古い文字コードのタグを読み直す（設定のボタンから。DB の値に今の決まりをかけ直す）
pub mod legacy_reread;
pub mod legacy_text;
pub mod scanner;
pub mod tags_read;
pub mod tags_write;
/// v3.3.0：曲ファイルをごみ箱へ移し、ライブラリから外す
pub mod trash;
