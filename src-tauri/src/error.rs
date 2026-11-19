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
    /// 着せ替え（v3.2.0）：zip が壊れている・読めない形（分割・zip64・知らない圧縮方式・CRC 違い・重なり）
    SkinZipInvalid,
    /// 着せ替え：暗号化された zip
    SkinZipEncrypted,
    /// 着せ替え：大きさ・数の上限を超えた（zip・展開後・項目数・ファイル数・1ファイル・圧縮率）
    SkinTooLarge,
    /// 着せ替え：危ないパス（親フォルダへ出る・絶対パス・UNC・\ 区切り・リンク・大文字小文字だけ違う重複 など）
    SkinPathUnsafe,
    /// 着せ替え：知らないファイル・使われていないファイル・置いてはいけない場所
    SkinFileNotAllowed,
    /// 着せ替え：実行ファイル（拡張子か中身の MZ）
    SkinExecutable,
    /// 着せ替え：SVG（3.2.0 では受け付けない）
    SkinSvgNotAllowed,
    /// 着せ替え：CSS（3.2.0 では受け付けない）
    SkinCssNotAllowed,
    /// 着せ替え：フォントのファイル（3.2.0 では受け付けない）
    SkinFontNotAllowed,
    /// 着せ替え：skin.json が読めない・必須の項目が無い・型が違う・重複したキー
    SkinManifestInvalid,
    /// 着せ替え：format・schemaVersion がこの版で読めない
    SkinSchemaUnsupported,
    /// 着せ替え：立ち絵の置き方がこの版で使えない（right-rail だけ）
    SkinPlacementUnsupported,
    /// 着せ替え：skin.json が指す画像が zip・フォルダに無い
    SkinAssetMissing,
    /// 着せ替え：画像として最後まで読めない・拡張子と中身が違う
    SkinImageInvalid,
    /// 着せ替え：画像の縦横・画素・コマ数・長さの上限を超えた
    SkinImageLimit,
    /// 着せ替え：同じ id のスキンが既に入っている（上書きしない）
    SkinIdExists,
    /// 着せ替え：そのスキンが入っていない
    SkinNotFound,
    /// 着せ替え：既定のスキンは消せない・書き出せない
    SkinBuiltinReadonly,
    /// 着せ替え：入っているスキンが壊れていて使えない（既定の見た目に戻した）
    SkinApplyFailed,
    /// 着せ替え：取り込みの確認が古い（別の zip を選んだ・取り消した）
    SkinImportExpired,
    /// 立ち絵（v3.9.0。画像から追加する立ち絵）：PNG・WebP・GIF でない（拡張子で見る。中身は画像の確かめで見る）
    AvatarImageFormat,
    /// 立ち絵：画像のファイルが大きすぎる（1つ 15MB まで。スキンの画像と同じ上限）
    AvatarTooLarge,
    /// 立ち絵：名前が決まりに合わない（1〜40 文字。スキンの名前と同じ文字の決まり）
    AvatarNameInvalid,
    /// 立ち絵：登録できる数（30）を超える。何も作っていない
    AvatarLimit,
    /// 立ち絵：その立ち絵が無い（消した・フォルダが無い）・その取り込んだスキンが無いか立ち絵を持たない
    AvatarNotFound,
    /// 立ち絵：フォルダはあるが読めない（avatar.json が壊れた・画像が消えた・リンク）
    AvatarBroken,
    /// 立ち絵：選んだ画像の確認が古い（選び直した・取り消した）
    AvatarPickExpired,
    /// 曲をごみ箱へ移す（v3.3.0）：その曲はライブラリに無い（もう外した・一覧が古い）。何も動かしていない
    TrackNotInLibrary,
    /// 曲をごみ箱へ移す：ライブラリの曲ファイルとして確かめられない（音声ファイルの拡張子でない・フォルダ・リンク）。何も動かしていない
    TrashNotAllowed,
    /// 曲をごみ箱へ移す：ごみ箱の無い場所（USB メモリ・CD・RAM ディスク、ごみ箱を切ったドライブ、ごみ箱に入らない大きさ）。
    /// 完全に消えてしまうので、移さずに断った
    TrashUnavailable,
    /// 曲をごみ箱へ移す：ほかのアプリがファイルを使っている
    FileInUse,
    /// 曲をごみ箱へ移す：1回に送れる曲の数（library::trash::MAX_TRACKS）を超えた。何も動かしていない
    TrashTooMany,
    /// 古い文字コードのタグの読み直し（v3.5.0）：もう走っている。何も変えていない
    LegacyRereadBusy,
    /// 古い文字コードのタグの読み直し：曲の取り込み・再走査が走っているので始めなかった。何も変えていない
    LibraryImportBusy,
    /// タグの編集画面の保存（v3.6.0）：保存の時点で、その曲はもうライブラリから外されていた。曲ファイルに書かず、ライブラリにも戻していない
    TrackRemovedBeforeSave,
    /// 出力先の操作を断った（v3.8.0：出力先は3つまで・ほかの出力先で使っているデバイス・無い枠・範囲の外の値）。何も変えていない
    OutputInvalid,
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
            ErrorKind::SkinZipInvalid,
            ErrorKind::SkinZipEncrypted,
            ErrorKind::SkinTooLarge,
            ErrorKind::SkinPathUnsafe,
            ErrorKind::SkinFileNotAllowed,
            ErrorKind::SkinExecutable,
            ErrorKind::SkinSvgNotAllowed,
            ErrorKind::SkinCssNotAllowed,
            ErrorKind::SkinFontNotAllowed,
            ErrorKind::SkinManifestInvalid,
            ErrorKind::SkinSchemaUnsupported,
            ErrorKind::SkinPlacementUnsupported,
            ErrorKind::SkinAssetMissing,
            ErrorKind::SkinImageInvalid,
            ErrorKind::SkinImageLimit,
            ErrorKind::SkinIdExists,
            ErrorKind::SkinNotFound,
            ErrorKind::SkinBuiltinReadonly,
            ErrorKind::SkinApplyFailed,
            ErrorKind::SkinImportExpired,
            ErrorKind::AvatarImageFormat,
            ErrorKind::AvatarTooLarge,
            ErrorKind::AvatarNameInvalid,
            ErrorKind::AvatarLimit,
            ErrorKind::AvatarNotFound,
            ErrorKind::AvatarBroken,
            ErrorKind::AvatarPickExpired,
            ErrorKind::TrackNotInLibrary,
            ErrorKind::TrashNotAllowed,
            ErrorKind::TrashUnavailable,
            ErrorKind::FileInUse,
            ErrorKind::TrashTooMany,
            ErrorKind::LegacyRereadBusy,
            ErrorKind::LibraryImportBusy,
            ErrorKind::TrackRemovedBeforeSave,
            ErrorKind::OutputInvalid,
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
