//! アプリ全体のエラー型。
//!
//! 原文「エラー処理」の8種をそのまま種別として置く。
//! 画面は `code` から `src/messages/ja.ts` の日本語の文を引く（Rust 側で日本語の文を作らない）。

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// ファイル不存在
    FileNotFound,
    /// ファイルアクセス拒否
    AccessDenied,
    /// 非対応形式
    UnsupportedFormat,
    /// タグ読み込み失敗
    TagReadFailed,
    /// タグ書き込み失敗
    TagWriteFailed,
    /// 再生デバイス切断
    DeviceDisconnected,
    /// 音声再生エラー
    PlaybackFailed,
    /// データベースエラー
    Database,
    /// 歌詞の内容を読み取れない（手動保存で中身が空など。v3）
    LyricsFormatInvalid,
    /// 歌詞ファイルを UTF-8 として読めない（v3）
    LyricsEncodingUnsupported,
    /// 歌詞をネットから取る設定がオフ（v3.1.0。通信せずに断った）
    LyricsOnlineDisabled,
    /// 歌詞をネットから取る処理が、すでに走っている（v3.1.0）
    LyricsOnlineBusy,
    /// ネットワーク上の場所（UNC・ネットワークドライブ）なので触らずに断った（v3.1.1）
    NetworkPathUnsupported,
    /// 上のどれにも当たらないもの（入力の誤りなど）
    Other,
}

#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[error("{kind:?}: {detail}")]
pub struct AppError {
    #[serde(rename = "code")]
    pub kind: ErrorKind,
    /// 技術的な詳細（パスや下位のエラー文）。画面では補足として小さく出す。
    pub detail: String,
}

impl AppError {
    pub fn new(kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    pub fn other(detail: impl Into<String>) -> Self {
        Self::new(ErrorKind::Other, detail)
    }

    /// 入出力のエラーを、原文の種別へ振り分ける。
    pub fn from_io(err: &std::io::Error, path: &std::path::Path) -> Self {
        let kind = match err.kind() {
            std::io::ErrorKind::NotFound => ErrorKind::FileNotFound,
            std::io::ErrorKind::PermissionDenied => ErrorKind::AccessDenied,
            _ => ErrorKind::Other,
        };
        Self::new(kind, format!("{}: {}", path.display(), err))
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::new(ErrorKind::Database, e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::other(e.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::other(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// 原文のエラー8種（と「その他」）が、画面の日本語の文（src/messages/ja.ts）にすべてある
    #[test]
    fn every_kind_has_japanese_text() {
        let ja = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/messages/ja.ts"),
        )
        .unwrap();
        for k in [
            ErrorKind::FileNotFound,
            ErrorKind::AccessDenied,
            ErrorKind::UnsupportedFormat,
            ErrorKind::TagReadFailed,
            ErrorKind::TagWriteFailed,
            ErrorKind::DeviceDisconnected,
            ErrorKind::PlaybackFailed,
            ErrorKind::Database,
            ErrorKind::LyricsFormatInvalid,
            ErrorKind::LyricsEncodingUnsupported,
            ErrorKind::LyricsOnlineDisabled,
            ErrorKind::LyricsOnlineBusy,
            ErrorKind::NetworkPathUnsupported,
            ErrorKind::Other,
        ] {
            let code = serde_json::to_value(k).unwrap();
            let code = code.as_str().unwrap();
            assert!(ja.contains(&format!("  {code}: \"")), "missing text for {code}");
        }
    }

    #[test]
    fn io_errors_map_to_kinds() {
        let p = std::path::Path::new("x");
        let nf = std::io::Error::from(std::io::ErrorKind::NotFound);
        let pd = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert_eq!(AppError::from_io(&nf, p).kind, ErrorKind::FileNotFound);
        assert_eq!(AppError::from_io(&pd, p).kind, ErrorKind::AccessDenied);
        let db: AppError = rusqlite::Error::InvalidQuery.into();
        assert_eq!(db.kind, ErrorKind::Database);
    }
}
