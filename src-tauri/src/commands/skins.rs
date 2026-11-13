//! 着せ替え（v3.2.0）の窓口。判断は skins::runtime と skins::package が持つ。
//! 画面からはパスを受け取らない（zip はここでファイルを選ぶ画面を出して選ばせる。スキンは id、取り込みは確認の印で指す）。

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::error::{AppResult, ErrorKind};
use crate::skins::package::{self, ExportReport, Inspection, SkinSummary};
use crate::skins::runtime::{self, SkinState};
use crate::skins::manifest::{from_skin, skin_error};
use crate::skins::names;

fn check_id(id: &str) -> AppResult<()> {
    if id == names::BUILTIN_ID || names::valid_id(id) {
        Ok(())
    } else {
        Err(skin_error(ErrorKind::SkinNotFound, from_skin(id)))
    }
}

#[tauri::command]
pub async fn skins_state(app: AppHandle) -> AppResult<SkinState> {
    Ok(runtime::state(&app))
}

#[tauri::command]
pub async fn skins_list(app: AppHandle) -> AppResult<Vec<SkinSummary>> {
    Ok(runtime::list(&app))
}

/// zip を選ばせて確かめる（選ばなければ None）
#[tauri::command]
pub async fn skins_pick_and_inspect(app: AppHandle) -> AppResult<Option<Inspection>> {
    let picked = app
        .dialog()
        .file()
        .set_title("スキンの zip を選ぶ")
        .add_filter("PL Player のスキン", &["zip"])
        .blocking_pick_file();
    let Some(src) = picked.and_then(|f| f.into_path().ok()) else {
        return Ok(None);
    };
    runtime::inspect(&app, &src).map(Some)
}

#[tauri::command]
pub async fn skins_import_commit(app: AppHandle, token: String) -> AppResult<SkinSummary> {
    if !package::valid_token(&token) {
        return Err(skin_error(ErrorKind::SkinImportExpired, from_skin(&token)));
    }
    runtime::commit(&app, &token)
}

#[tauri::command]
pub async fn skins_import_cancel(app: AppHandle, token: String) -> AppResult<()> {
    if package::valid_token(&token) {
        runtime::cancel(&app, &token);
    }
    Ok(())
}

#[tauri::command]
pub async fn skins_apply(app: AppHandle, id: String) -> AppResult<SkinState> {
    check_id(&id)?;
    runtime::apply(&app, &id)
}

#[tauri::command]
pub async fn skins_reset(app: AppHandle) -> AppResult<SkinState> {
    runtime::reset(&app)
}

#[tauri::command]
pub async fn skins_exit_safe_mode(app: AppHandle) -> AppResult<SkinState> {
    runtime::exit_safe_mode(&app)
}

#[tauri::command]
pub async fn skins_delete(app: AppHandle, id: String) -> AppResult<SkinState> {
    check_id(&id)?;
    runtime::delete(&app, &id)
}

/// スキンを zip に書き出す（保存先を選ばせる。選ばなければ None）
#[tauri::command]
pub async fn skins_export(app: AppHandle, id: String) -> AppResult<Option<ExportReport>> {
    check_id(&id)?;
    if id == names::BUILTIN_ID {
        return Err(skin_error(ErrorKind::SkinBuiltinReadonly, from_skin(&id)));
    }
    let picked = app
        .dialog()
        .file()
        .set_title("スキンを書き出す")
        .set_file_name(format!("{id}.zip"))
        .add_filter("PL Player のスキン", &["zip"])
        .blocking_save_file();
    let Some(dest) = picked.and_then(|f| f.into_path().ok()) else {
        return Ok(None);
    };
    let dest = if dest.extension().is_none() { dest.with_extension("zip") } else { dest };
    runtime::export(&app, &id, &dest).map(Some)
}
