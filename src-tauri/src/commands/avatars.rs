//! 立ち絵（v3.9.0。画像から追加する立ち絵と、使う立ち絵の選択）の窓口。判断は skins::avatars と skins::runtime が持つ。
//! 画面からはパスを受け取らない・画面へパスを返さない（画像はここでファイルを選ぶ画面を出して選ばせ、確かめた結果を印 token で返す。
//! 立ち絵は id で指す）。画面へ返す画像のパスは、Rust が `avatars\<id>\`・`skins\<id>\` の中で確かめたものだけ。
//! ファイルを選ぶ画面（blocking）を出す間、どの鍵も持たない。

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::error::{AppResult, ErrorKind};
use crate::skins::avatars::{self, AvatarList, AvatarSummary, PickedImage, Slot};
use crate::skins::manifest::{from_skin, skin_error};
use crate::skins::package;
use crate::skins::runtime::{self, SkinState};

fn check_id(id: &str) -> AppResult<()> {
    if avatars::valid_id(id) {
        Ok(())
    } else {
        Err(skin_error(ErrorKind::AvatarNotFound, from_skin(id)))
    }
}

/// 設定の「立ち絵」の欄の一覧（取り込んだスキンの立ち絵・登録した立ち絵・数の上限）
#[tauri::command]
pub async fn avatars_list(app: AppHandle) -> AppResult<AvatarList> {
    Ok(runtime::avatars_list(&app))
}

/// 画像を選ばせて確かめる（`slot` は "playing" か "stopped"。選ばなければ None）。確かめた画像は作るまで Rust が持つ
#[tauri::command]
pub async fn avatars_pick_image(app: AppHandle, slot: String) -> AppResult<Option<PickedImage>> {
    let slot = Slot::parse(&slot).ok_or_else(|| skin_error(ErrorKind::Other, from_skin(&slot)))?;
    let title = match slot {
        Slot::Playing => "立ち絵：再生中の絵を選ぶ",
        Slot::Stopped => "立ち絵：停止中の絵を選ぶ",
    };
    // 選ぶ画面を出す前に世代を控える（選んでいる間・確かめている間に小窓が取り消されたら、届いた画像は捨てる）
    let generation = runtime::pick_generation(&app);
    let picked = app.dialog().file().set_title(title).add_filter("画像（PNG・WebP・GIF）", &["png", "webp", "gif"]).blocking_pick_file();
    let Some(src) = picked.and_then(|f| f.into_path().ok()) else {
        return Ok(None);
    };
    runtime::pick(&app, &src, slot, generation)
}

/// 選んだ画像を捨てる（作るのをやめた）
#[tauri::command]
pub async fn avatars_cancel_pick(app: AppHandle) -> AppResult<()> {
    runtime::cancel_picks(&app);
    Ok(())
}

/// 立ち絵を作る。`playing`・`stopped` は avatars_pick_image が返した印（停止中は無くてよい）。名前が空なら、再生中の絵のファイル名から作った名前
#[tauri::command]
pub async fn avatars_create(app: AppHandle, name: String, playing: String, stopped: Option<String>) -> AppResult<AvatarSummary> {
    for t in std::iter::once(&playing).chain(stopped.as_ref()) {
        if !package::valid_token(t) {
            return Err(skin_error(ErrorKind::AvatarPickExpired, from_skin(t)));
        }
    }
    runtime::create(&app, &name, &playing, stopped.as_deref())
}

/// 作った立ち絵を変える（v3.9.0 直し2回目）。`playing`・`stopped` は差し替える画像の印（無ければそのまま）、
/// `remove_stopped`（画面からは removeStopped）なら停止中を外す。名前が空なら今の名前のまま
#[tauri::command]
pub async fn avatars_update(
    app: AppHandle,
    id: String,
    name: String,
    playing: Option<String>,
    stopped: Option<String>,
    remove_stopped: bool,
) -> AppResult<AvatarSummary> {
    check_id(&id)?;
    for t in playing.iter().chain(stopped.iter()) {
        if !package::valid_token(t) {
            return Err(skin_error(ErrorKind::AvatarPickExpired, from_skin(t)));
        }
    }
    runtime::update(&app, &id, &name, playing.as_deref(), stopped.as_deref(), remove_stopped)
}

/// 立ち絵を消す（使う立ち絵に選んでいたら「スキンの立ち絵」に戻す）
#[tauri::command]
pub async fn avatars_delete(app: AppHandle, id: String) -> AppResult<SkinState> {
    check_id(&id)?;
    runtime::delete_avatar(&app, &id)
}

/// 立ち絵を出さない（設定の「立ち絵を出さない」。avatars_use の "none" と同じ道。作った立ち絵は消さない）
#[tauri::command]
pub async fn avatars_hide(app: AppHandle) -> AppResult<SkinState> {
    runtime::hide_avatar(&app)
}

/// 使う立ち絵を選ぶ（"skin"・"none"・"avatar:<id>"・"skin:<id>"）
#[tauri::command]
pub async fn avatars_use(app: AppHandle, choice: String) -> AppResult<SkinState> {
    runtime::use_avatar(&app, &choice)
}
