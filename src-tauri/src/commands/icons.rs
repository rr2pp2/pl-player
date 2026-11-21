//! 手で入れるアイコン（v3.11.0）の窓口。判断は skins::icons と skins::runtime が持つ。
//! 画面からはパスを受け取らない（押して選ぶときはここでファイルを選ぶ画面を出す。落としたときは、OS から落としたパスを
//! Rust がウィンドウの知らせで控えておき、画面が知らせた落とした位置で取り出す）。枠は名前と状態で指す。
//! ファイルを選ぶ画面（blocking）を出す間、どの鍵も持たない。

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::error::{AppResult, ErrorKind};
use crate::skins::icons;
use crate::skins::manifest::{from_skin, skin_error};
use crate::skins::runtime::{self, AppIconPreviews, SkinState};

fn check_slot(name: &str, state: &str) -> AppResult<()> {
    match icons::slot(name, state) {
        Some(_) => Ok(()),
        None => Err(icons::for_screen(skin_error(ErrorKind::IconSlotInvalid, format!("{}/{}", from_skin(name), from_skin(state))))),
    }
}

/// 枠を押したとき：画像を選ばせて、その枠のアイコンにする（選ばなければ None）
#[tauri::command]
pub async fn icons_pick(app: AppHandle, name: String, state: String) -> AppResult<Option<SkinState>> {
    check_slot(&name, &state)?;
    runtime::refuse_icons_in_safe_mode(&app)?;
    let picked = app
        .dialog()
        .file()
        .set_title("アイコンの画像を選ぶ")
        .add_filter("画像（PNG・WebP・GIF）", &["png", "webp", "gif"])
        .blocking_pick_file();
    let Some(src) = picked.and_then(|f| f.into_path().ok()) else {
        return Ok(None);
    };
    runtime::set_icon_file(&app, &name, &state, &src).map(Some)
}

/// 枠に落としたとき：`x`・`y` は画面が受けたドロップの知らせの位置（そのまま返す）。同じ位置の、OS から落としたファイル1つだけを使う
#[tauri::command]
pub async fn icons_drop(app: AppHandle, name: String, state: String, x: f64, y: f64) -> AppResult<SkinState> {
    check_slot(&name, &state)?;
    let path = icons::take_drop(x, y).map_err(icons::for_screen)?;
    runtime::set_icon_file(&app, &name, &state, &path)
}

/// 枠の「戻す」：その枠の手のアイコンを外す
#[tauri::command]
pub async fn icons_reset(app: AppHandle, name: String, state: String) -> AppResult<SkinState> {
    check_slot(&name, &state)?;
    runtime::reset_icon(&app, &name, &state)
}

/// 「手で入れたアイコンを全部戻す」
#[tauri::command]
pub async fn icons_clear(app: AppHandle) -> AppResult<SkinState> {
    runtime::clear_icons(&app)
}

/// 今のウィンドウ・トレイのアイコンの見本
#[tauri::command]
pub async fn icons_app_preview(app: AppHandle) -> AppResult<AppIconPreviews> {
    Ok(runtime::app_icon_previews(&app))
}
