//! 見本の立ち絵「くじら」（v3.9.1）。アプリに最初から入れておく立ち絵。
//!
//! - 絵はリポジトリの `samples\kujira\`（再生中・停止中。450×950 の透明な背景の PNG。右の欄が細いとき（窓 1200px で約 216px）でも顔が切れないよう、顔を真ん中寄りに描いた。図形だけで描いたもの）を exe に埋め込む。
//!   配布の zip の「見本」フォルダに入れる絵と同じファイル（scripts\release-samples.mts）
//! - 入れ方は、使う人が「＋ 立ち絵を追加…」で足すのと同じ道（`avatars::pick_bytes` で確かめ、`avatars::create` で作る）。
//!   入ったあとは、ほかの立ち絵と区別しない（変更・削除・使う・出さない がそのまま効く）
//! - **入れるのは一度だけ。**入れたかの印は DB の settings 表のキー `sample_avatars`（設定の `app` とは別のキー。出力の `outputs`・
//!   `mode` と同じ置き方）に置く。設定のバックアップ（zip）が settings 表から書き出すのは `app` と `outputs` だけで、読み込みが書くのも
//!   その2つだけなので、印は zip に入らず、zip を読み込んでも変わらない（読み込んで増える・消える、が起きない）。
//!   設定 `app` に置かなかったのは、画面からの保存（`settings_set`）と zip の読み込みの両方で、値を守る決まりを足さずに済むため
//! - 使う人がくじらを消しても、印は残るので戻らない
//! - **新しく入れた人**（起動の前にライブラリの DB が無かった）だけ、使う立ち絵にも選ぶ（初めての起動から右にくじらが出る）。
//!   前の版から上げた人は一覧に足すだけで、使う立ち絵・見た目は変えない
//! - **30 体（上限）が埋まっていたら足さず、入れたことにする。**あとで枠が空いても入れない（自分で消して空けた枠に、
//!   頼んでいない立ち絵が入るのは驚かせるため）
//! - **作る前に印を `pending`（作業中）で書き、書けなければ作らない。**作れたら `added` に書き換える。作業中の印が残っていたら
//!   （作った直後に落ちた・`added` を書けなかった）、次の起動では入れ直さない（二重に入れない側に倒す。入らなかった形が残るのは許す）。
//!   印を書けないまま作ると、書けない間は起動のたびに1体ずつ増えるため
//! - ほかの理由で入れられなかった（フォルダに書けないなど）ときは作業中の印を消し、次の起動でもう一度試す（そのときは新しく入れた人
//!   でも選ばない。上げた人と同じ扱い）。印を消せなければ作業中のまま残り、もう試さない

use std::path::Path;

use super::avatars::{self, Choice, Slot, MAX_AVATARS};
use super::Dirs;
use crate::db::{self, Db};
use crate::error::{AppResult, ErrorKind};
use crate::settings::store::{self, AppSettings};

/// 入れたかの印のキー（DB の settings 表。値は `{"kujira": "pending" | "added" | "full"}`。どの値でも、あれば入れない）
pub const MARK_KEY: &str = "sample_avatars";
/// 印の中の名前
pub const KUJIRA: &str = "kujira";
/// 立ち絵の名前（一覧に出る名前。使う人が「変更」で変えてよい）
pub const KUJIRA_NAME: &str = "くじら";
pub(crate) const KUJIRA_PLAYING: &[u8] = include_bytes!("../../../samples/kujira/playing.png");
pub(crate) const KUJIRA_STOPPED: &[u8] = include_bytes!("../../../samples/kujira/stopped.png");

/// 起動のときに入れた結果
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// 前に入れた（か、上限で入れないと決めた）。何もしない
    AlreadyHandled,
    /// 入れた（立ち絵の id）
    Added(String),
    /// 30 体が埋まっていたので入れず、入れたことにした
    Full,
    /// 入れられなかった（印を書けなかった、か、作れず作業中の印を外した。次の起動でもう一度。外せなかったときはもう試さない）
    Failed(ErrorKind),
}

/// 新しく入れた人か（データのフォルダにライブラリの DB がまだ無い）。DB を開く前に呼ぶ。
/// 3.9.0 までの版を使っていた人は、設定を一度も変えていなくても DB がある（設定の `app` は変えたときにしか書かないので、
/// 「設定が無い」では上げた人と見分けられない）
pub fn is_fresh_install(data_dir: &Path) -> bool {
    std::fs::symlink_metadata(data_dir.join("library.db")).is_err()
}

/// 印を読む。キーが無ければ false。読めない値（形の違う JSON）は入れたことにする（二重に入れない側に倒す）
fn handled(db: &Db, name: &str) -> bool {
    match db::settings::get(db, MARK_KEY) {
        Ok(None) => false,
        Ok(Some(v)) => v.as_object().map(|o| o.contains_key(name)).unwrap_or(true),
        Err(e) => {
            tracing::warn!("sample avatar mark could not be read: {:?}", e.kind);
            true
        }
    }
}

fn mark(db: &Db, name: &str, how: &str) -> AppResult<()> {
    let mut o = db::settings::get(db, MARK_KEY)?.and_then(|v| v.as_object().cloned()).unwrap_or_default();
    o.insert(name.to_string(), serde_json::Value::String(how.to_string()));
    db::settings::set(db, MARK_KEY, &serde_json::Value::Object(o))
}

fn mark_or_warn(db: &Db, how: &str) {
    if let Err(e) = mark(db, KUJIRA, how) {
        tracing::warn!("sample avatar mark could not be saved: {:?}", e.kind);
    }
}

/// 印から名前を外す（ほかの名前が無くなればキーごと消す）
fn unmark(db: &Db, name: &str) -> AppResult<()> {
    let mut o = db::settings::get(db, MARK_KEY)?.and_then(|v| v.as_object().cloned()).unwrap_or_default();
    o.remove(name);
    if o.is_empty() {
        db::settings::remove(db, MARK_KEY)
    } else {
        db::settings::set(db, MARK_KEY, &serde_json::Value::Object(o))
    }
}

/// くじらを一度だけ入れる（作る前に作業中の印を書き、書けなければ作らない。作れたら入れた印にする）。使う立ち絵は変えない
pub fn install_once(db: &Db, dirs: &Dirs) -> Outcome {
    if handled(db, KUJIRA) {
        return Outcome::AlreadyHandled;
    }
    if avatars::count(dirs) >= MAX_AVATARS {
        tracing::info!("sample avatar not added: {MAX_AVATARS} avatars already");
        mark_or_warn(db, "full");
        return Outcome::Full;
    }
    // 作る前に作業中の印。書けないまま作ると、書けない間は起動のたびに増える
    if let Err(e) = mark(db, KUJIRA, "pending") {
        tracing::warn!("sample avatar not added: mark could not be saved: {:?}", e.kind);
        return Outcome::Failed(e.kind);
    }
    let made = (|| {
        let (p, _) = avatars::pick_bytes(KUJIRA_PLAYING.to_vec(), "playing.png", Slot::Playing)?;
        let (s, _) = avatars::pick_bytes(KUJIRA_STOPPED.to_vec(), "stopped.png", Slot::Stopped)?;
        avatars::create(dirs, KUJIRA_NAME, &p, Some(&s))
    })();
    match made {
        Ok(sum) => {
            tracing::info!("sample avatar added: {}", sum.id);
            // 書けなくても作業中の印が残るので、次の起動で入れ直さない
            mark_or_warn(db, "added");
            Outcome::Added(sum.id)
        }
        Err(e) if e.kind == ErrorKind::AvatarLimit => {
            mark_or_warn(db, "full");
            Outcome::Full
        }
        Err(e) => {
            // 作れなかった：作業中の印を外して次の起動でもう一度。外せなければ作業中のまま（もう試さない）
            match unmark(db, KUJIRA) {
                Ok(()) => tracing::warn!("sample avatar could not be added (will retry at next start): {:?}", e.kind),
                Err(u) => tracing::warn!(
                    "sample avatar could not be added: {:?}; pending mark could not be cleared ({:?}), will not retry",
                    e.kind,
                    u.kind
                ),
            }
            Outcome::Failed(e.kind)
        }
    }
}

/// 起動のとき（設定を読んだあと、AppState に置く前。lib.rs の setup）。くじらを一度だけ入れ、新しく入れた人で、
/// この起動で入れられたときだけ、使う立ち絵にも選んで保存する
pub fn install_at_start(db: &Db, dirs: &Dirs, fresh_install: bool, settings: &mut AppSettings) -> Outcome {
    let out = install_once(db, dirs);
    if let (true, Outcome::Added(id)) = (fresh_install, &out) {
        settings.skin.set_avatar(&Choice::Avatar(id.clone()).as_setting());
        if let Err(e) = store::save(db, settings) {
            tracing::warn!("sample avatar choice could not be saved: {:?}", e.kind);
        }
    }
    out
}

#[cfg(test)]
#[path = "sample_tests.rs"]
mod tests;
