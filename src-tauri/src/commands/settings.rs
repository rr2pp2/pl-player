//! 設定・背景・グローバルホットキー・設定の書き出しと読み込み。

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use super::{ev, AppState};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::library::artwork;
use crate::platform::windows::hotkeys::{self, HotkeyStatus};
use crate::settings::backup::{self, BackupReport};
use crate::settings::store::{self, AppSettings};

pub(crate) fn store_and_emit(app: &AppHandle, state: &AppState, s: AppSettings) -> AppResult<AppSettings> {
    let s = s.sanitize();
    store::save(&state.db, &s)?;
    *state.settings.lock().unwrap_or_else(|p| p.into_inner()) = s.clone();
    let _ = app.emit(ev::SETTINGS, &s);
    Ok(s)
}

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> AppResult<AppSettings> {
    Ok(state.settings())
}

/// 設定を保存する。ホットキーが変わっていれば登録し直し、結果（競合）を返す
#[tauri::command]
pub async fn settings_set(app: AppHandle, state: State<'_, AppState>, settings: AppSettings) -> AppResult<AppSettings> {
    let before = state.settings();
    let settings = from_screen(settings, &before, &state.paths.bg_dir);
    let s = store_and_emit(&app, &state, settings)?;
    if before.hotkeys != s.hotkeys {
        let statuses = hotkeys::apply(&app, &s.hotkeys);
        let _ = app.emit(ev::HOTKEYS, &statuses);
    }
    Ok(s)
}

/// 画面から来た設定のうち、画面に変えさせない値を今の値に戻す
pub(crate) fn from_screen(s: AppSettings, before: &AppSettings, bg_dir: &Path) -> AppSettings {
    let mut s = keep_background_inside(s, before, bg_dir);
    // v3.2.0：使うスキンは skins_apply（確かめてから当てる）だけが変える。画面から来た値は使わない
    s.skin.active_skin_id = before.skin.active_skin_id.clone();
    s
}

/// 画面から来た設定の背景のパスは、データフォルダの backgrounds の中だけを受ける（v3.1.1。ペネトレーションテスト v3.0.2 の4）。
/// 外を指していたら、今の値（それも中でなければ無し）に戻す。背景を変えるのは background_set（画像を確かめて複製する）だけ
pub(crate) fn keep_background_inside(mut s: AppSettings, before: &AppSettings, bg_dir: &Path) -> AppSettings {
    if let Some(p) = s.background.path.as_deref() {
        if !store::background_in_dir(p, bg_dir) {
            tracing::warn!("settings_set: background path outside the data folder was ignored");
            s.background.path = before
                .background
                .path
                .clone()
                .filter(|b| store::background_in_dir(b, bg_dir));
        }
    }
    s
}

#[tauri::command]
pub async fn hotkeys_status(app: AppHandle, state: State<'_, AppState>) -> AppResult<Vec<HotkeyStatus>> {
    Ok(hotkeys::status(&app, &state.settings().hotkeys))
}

/// 背景画像を設定する（PNG・JPG・JPEG・WebP）。データフォルダへ複製する
#[tauri::command]
pub async fn background_set(app: AppHandle, state: State<'_, AppState>, path: String) -> AppResult<AppSettings> {
    let src = Path::new(&path);
    let ok_ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp"))
        .unwrap_or(false);
    if !ok_ext {
        return Err(AppError::new(ErrorKind::UnsupportedFormat, path));
    }
    let dest = artwork::import_image(src, &state.paths.bg_dir, "bg")?;
    let mut s = state.settings();
    s.background.path = Some(dest.to_string_lossy().into_owned());
    store_and_emit(&app, &state, s)
}

#[tauri::command]
pub async fn background_clear(app: AppHandle, state: State<'_, AppState>) -> AppResult<AppSettings> {
    let mut s = state.settings();
    s.background.path = None;
    store_and_emit(&app, &state, s)
}

/// 設定の書き出し
#[tauri::command]
pub async fn settings_export(app: AppHandle, state: State<'_, AppState>) -> AppResult<Option<BackupReport>> {
    let picked = app
        .dialog()
        .file()
        .set_title("設定を書き出す")
        .set_file_name("pl-player-settings.zip")
        .add_filter("PL Player の設定", &["zip"])
        .blocking_save_file();
    let Some(dest) = picked.and_then(|f| f.into_path().ok()) else {
        return Ok(None);
    };
    let dest = if dest.extension().is_none() {
        dest.with_extension("zip")
    } else {
        dest
    };
    Ok(Some(backup::export(&state.db, &state.settings(), &state.paths.bg_dir, &dest)?))
}

/// 設定の読み込み
#[tauri::command]
pub async fn settings_import(app: AppHandle, state: State<'_, AppState>) -> AppResult<Option<BackupReport>> {
    let picked = app
        .dialog()
        .file()
        .set_title("設定を読み込む")
        .add_filter("PL Player の設定", &["zip"])
        .blocking_pick_file();
    let Some(src) = picked.and_then(|f| f.into_path().ok()) else {
        return Ok(None);
    };
    let (settings, report, folders) = backup::import(&state.db, &src, &state.paths.bg_dir)?;
    *state.settings.lock().unwrap_or_else(|p| p.into_inner()) = settings.clone();
    let statuses = hotkeys::apply(&app, &settings.hotkeys);
    let _ = app.emit(ev::HOTKEYS, &statuses);
    let _ = app.emit(ev::SETTINGS, &settings);
    let _ = app.emit(ev::PLAYLISTS, ());
    let _ = app.emit(ev::LIBRARY, ());
    // 読み込んだライブラリのフォルダを取り込む（この PC にあるものだけ）。
    // v3.1.1：ネットワーク上の場所は backup::import が入れない。在るかの確かめは取り込みのスレッドの中で行う
    let folders: Vec<PathBuf> = folders.into_iter().map(|(p, _)| PathBuf::from(p)).collect();
    if !folders.is_empty() {
        super::library::start_rescan(&app, folders);
    }
    Ok(Some(report))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v3.1.1（ペネトレーションテスト v3.0.2 の4）：画面から backgrounds の外のパスが来ても受けない。今の値に戻す
    #[test]
    fn settings_set_keeps_background_inside() {
        let bg = Path::new(r"C:\data\backgrounds");
        let mut before = AppSettings::default();
        before.background.path = Some(r"C:\data\backgrounds\bg-1.png".into());
        let mut evil = before.clone();
        evil.background.path = Some(r"C:\Users\x\.ssh\id_ed25519".into());
        evil.theme = "light".into();
        let s = keep_background_inside(evil, &before, bg);
        assert_eq!(s.background.path.as_deref(), Some(r"C:\data\backgrounds\bg-1.png"));
        // ほかの変更はそのまま
        assert_eq!(s.theme, "light");
        // 今の値も外を指していたら（3.1.0 までに入った値）、無しにする
        let mut old = AppSettings::default();
        old.background.path = Some(r"D:\elsewhere\x.png".into());
        let mut evil2 = old.clone();
        evil2.background.path = Some(r"\\pl-player-test.invalid\share\x.png".into());
        assert_eq!(keep_background_inside(evil2, &old, bg).background.path, None);
        // 中を指す値・無しは、そのまま受ける
        let mut ok = AppSettings::default();
        ok.background.path = Some(r"C:\data\backgrounds\bg-2.png".into());
        assert_eq!(keep_background_inside(ok.clone(), &before, bg).background.path, ok.background.path);
        assert_eq!(keep_background_inside(AppSettings::default(), &before, bg).background.path, None);
    }

    /// v3.2.0：画面から来た設定では、使うスキンは変わらない（変えるのは skins_apply だけ）。立ち絵の設定などは画面の値を受ける
    #[test]
    fn settings_set_keeps_active_skin() {
        let bg = Path::new(r"C:\data\backgrounds");
        let mut before = AppSettings::default();
        before.skin.active_skin_id = "quiet-paper".into();
        let mut from = before.clone();
        from.skin.active_skin_id = "never-checked".into();
        from.skin.character_visible = false;
        from.skin.character_scale = 0.8;
        from.skin.safe_mode_next_start = true;
        from.theme = "light".into();
        let s = from_screen(from, &before, bg);
        assert_eq!(s.skin.active_skin_id, "quiet-paper");
        assert!(!s.skin.character_visible && s.skin.safe_mode_next_start);
        assert!((s.skin.character_scale - 0.8).abs() < 1e-6);
        assert_eq!(s.theme, "light");
        // 既定のスキンのときに、画面から別の id が来ても既定のまま
        let s = from_screen(AppSettings { skin: crate::settings::store::SkinSettings { active_skin_id: "x".into(), ..Default::default() }, ..AppSettings::default() }, &AppSettings::default(), bg);
        assert_eq!(s.skin.active_skin_id, crate::skins::names::BUILTIN_ID);
    }
}
