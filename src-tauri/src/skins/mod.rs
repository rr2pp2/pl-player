//! 着せ替え（スキン。v3.2.0）。
//!
//! 1スキン＝1フォルダ（`skin.json` と画像）。置き場所はデータフォルダの `skins\<id>\`。使うスキンは1つ。
//! 組み込みの既定のスキン `builtin:default`（今の見た目そのもの）は消せない。
//!
//! - names：id とパスの決まり ／ json：重複キーと深さを断る JSON の読み方 ／ tokens：色と文字の値
//! - manifest：skin.json の読み方 ／ grapheme_base：名前の見える文字の判定に使う Unicode の表 ／ images：画像の確かめと最初の1コマ
//! - package：取り込み（一時の場所 → 確認 → 正式の場所）・読み込み・一覧・削除・書き出し
//! - runtime：画面・トレイ・ウィンドウとのやり取り（適用・既定に戻す・セーフモード・アイコンの差し替え・使う立ち絵）
//! - avatars：登録した立ち絵（v3.9.0。画像を選ぶだけで作る。画面の呼び名は「立ち絵」）と、使う立ち絵の選び方
//! - sample：見本の立ち絵「くじら」を一度だけ入れる（v3.9.1）
//! - colors：手で選ぶ色（v3.10.0。設定の `skin.colors`。スキンの色の上に重ねる。値の確かめは tokens と同じ）
//!
//! 仕様の全文は docs\着せ替え_仕様.md。

pub mod avatars;
pub mod colors;
pub mod grapheme_base;
pub mod images;
pub mod json;
pub mod manifest;
pub mod names;
pub mod package;
pub mod runtime;
pub mod sample;
pub mod tokens;

#[cfg(test)]
pub(crate) mod testkit;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

/// 取り込みの上限（推奨の立ち絵の大きさで動く画像が当たらないよう合わせた。理由は docs\着せ替え_仕様.md 10章）
pub mod limits {
    /// zip のファイルの大きさ
    pub const MAX_ZIP_BYTES: u64 = 25 * 1024 * 1024;
    /// 戻した中身の合計（申告ではなく、実際に戻したバイト数で数える）
    pub const MAX_UNPACKED_BYTES: u64 = 80 * 1024 * 1024;
    /// zip の項目の数（フォルダの項目も数える。飛ばした OS のファイルと、外したフォルダの項目は数えない）
    pub const MAX_ENTRIES: usize = 80;
    /// 中央ディレクトリを読む項目の数の上限（OS のファイルも含めた全部。macOS の zip は項目ごとに `__MACOSX/` の写しが付き、倍を超えるため）
    pub const MAX_SCANNED_ENTRIES: usize = 256;
    /// ファイルの数
    pub const MAX_FILES: usize = 64;
    /// 圧縮率の上限（戻すと 1MiB を超える項目だけに当てる。小さい JSON は圧縮率が高くても害が無いため）
    pub const MAX_RATIO: u64 = 100;
    pub const RATIO_CHECK_FROM: u64 = 1024 * 1024;
    /// 取り込みで確かめるときに復号する画素の合計（全部の画像の、全部のコマ）。
    /// 推奨の立ち絵 720×1280 の 120 コマ（約 1.1 億）を、再生中と停止中の2つ持てるように
    pub const MAX_TOTAL_PIXELS: u64 = 320_000_000;
}

/// スキンの置き場所
#[derive(Debug, Clone)]
pub struct Dirs {
    /// 正式の場所（画面が asset で読める）
    pub skins: PathBuf,
    /// 確認待ちの一時の場所（画面からは読めない）
    pub staging: PathBuf,
    /// v3.9.0：登録した立ち絵（画面が asset で読める）
    pub avatars: PathBuf,
    /// v3.9.0：立ち絵を作っている途中の一時の場所（画面からは読めない。起動のときに消す）
    pub avatar_staging: PathBuf,
}

impl Dirs {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            skins: data_dir.join("skins"),
            staging: data_dir.join("skins-staging"),
            avatars: data_dir.join("avatars"),
            avatar_staging: data_dir.join("avatars-staging"),
        }
    }
}
