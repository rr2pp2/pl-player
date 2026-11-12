//! メタデータ編集とアルバムアート（埋め込み画像の差し替え・外部画像の設定・画像削除）。

use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use super::{ev, AppState};
use crate::db::tracks::{self, Track};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::library::tags_read::{self, TagValues};
use crate::library::tags_write::{self, ArtChange};
use crate::library::artwork;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataView {
    pub track: Track,
    pub values: TagValues,
    /// アルバム単位の外部画像（設定していれば）
    pub album_override: Option<String>,
}

fn track_or_err(state: &AppState, id: i64) -> AppResult<Track> {
    tracks::get(&state.db, id)?.ok_or_else(|| AppError::new(ErrorKind::FileNotFound, format!("track {id}")))
}

#[tauri::command]
pub async fn metadata_read(state: State<'_, AppState>, id: i64) -> AppResult<MetadataView> {
    let track = track_or_err(&state, id)?;
    let values = tags_read::read_values(Path::new(&track.path))?;
    let album_override = tracks::album_override(&state.db, &track.album_key)?;
    Ok(MetadataView {
        track,
        values,
        album_override,
    })
}

/// 実ファイルのタグへ保存する（一時ファイル → 検証 → 置換）
#[tauri::command]
pub async fn metadata_write(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    values: TagValues,
    art: Option<ArtChange>,
) -> AppResult<Track> {
    let track = track_or_err(&state, id)?;
    let path = std::path::PathBuf::from(&track.path);
    let art = art.unwrap_or_default();
    let controller = &state.controller;
    // 再生中の曲なら、差し替えの前に手放して位置を覚え、終わったら（panic で抜けても）必ず再開する（v3.1.1）
    let result = with_resume(
        || controller.release_for_write(id),
        |r| controller.resume_after_write(r),
        || {
            let result = tags_write::write_tags(&path, &values, &art);
            // 成否に関わらず、ライブラリの値は実ファイルから読み直す
            if let Ok(info) = tags_read::read_track(&path, &state.db.thumbs_dir) {
                let _ = tracks::upsert(&state.db.writer(), &info);
            }
            result
        },
    );
    state.controller.refresh_track(id);
    let _ = app.emit(ev::LIBRARY, ());
    result?;
    track_or_err(&state, id)
}

/// `release` で手放したものを、`work` の抜け方（panic を含む）にかかわらず `resume` で戻す
/// （v3.1.1。ペネトレーションテスト v3.0.2 の3：タグの書き込みが panic すると、再生の再開が走らなかった）
pub(crate) fn with_resume<R, T>(release: impl FnOnce() -> Option<R>, resume: impl FnOnce(R), work: impl FnOnce() -> T) -> T {
    struct Guard<R, F: FnOnce(R)>(Option<(R, F)>);
    impl<R, F: FnOnce(R)> Drop for Guard<R, F> {
        fn drop(&mut self) {
            if let Some((r, f)) = self.0.take() {
                f(r);
            }
        }
    }
    let _guard = Guard(release().map(|r| (r, resume)));
    work()
}

/// 画像を選ぶ（アルバムアート・背景で共用）
#[tauri::command]
pub async fn pick_image(app: AppHandle, title: Option<String>) -> AppResult<Option<String>> {
    let picked = app
        .dialog()
        .file()
        .set_title(title.unwrap_or_else(|| "画像を選ぶ".into()))
        .add_filter("画像", &["png", "jpg", "jpeg", "webp"])
        .blocking_pick_file();
    Ok(picked
        .and_then(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned()))
}

/// 選んだ・ドロップした画像を確かめて、データフォルダへ複製する（画面で見本を出すため。
/// 画面が読めるのはデータフォルダだけ）。複製先のパスを返す
#[tauri::command]
pub async fn stage_image(state: State<'_, AppState>, path: String) -> AppResult<String> {
    let src = Path::new(&path);
    if !artwork::is_image_path(src) {
        return Err(AppError::new(ErrorKind::UnsupportedFormat, path));
    }
    let dest = artwork::import_image(src, &state.paths.art_dir, "staged")?;
    Ok(dest.to_string_lossy().into_owned())
}

/// 外部画像をアルバムに設定する（ファイルは変えない）。画像はデータフォルダへ複製する
#[tauri::command]
pub async fn album_art_set_external(
    app: AppHandle,
    state: State<'_, AppState>,
    album_key: String,
    image_path: String,
) -> AppResult<String> {
    let src = Path::new(&image_path);
    if !artwork::is_image_path(src) {
        return Err(AppError::new(ErrorKind::UnsupportedFormat, image_path));
    }
    let dest = artwork::import_image(src, &state.paths.art_dir, "album")?;
    let dest_s = dest.to_string_lossy().into_owned();
    tracks::set_album_override(&state.db, &album_key, Some(&dest_s))?;
    let _ = app.emit(ev::LIBRARY, ());
    refresh_current(&state);
    Ok(dest_s)
}

/// 外部画像の設定を外す（埋め込み画像があればそちらに戻る）
#[tauri::command]
pub async fn album_art_clear_external(app: AppHandle, state: State<'_, AppState>, album_key: String) -> AppResult<()> {
    tracks::set_album_override(&state.db, &album_key, None)?;
    let _ = app.emit(ev::LIBRARY, ());
    refresh_current(&state);
    Ok(())
}

fn refresh_current(state: &AppState) {
    if let Some(t) = state.controller.snapshot().track {
        state.controller.refresh_track(t.id);
    }
}

#[cfg(test)]
mod tests {
    use super::with_resume;
    use std::cell::Cell;

    /// v3.1.1（ペネトレーションテスト v3.0.2 の3）：タグの書き込みが panic で抜けても、再生の再開は必ず走る
    #[test]
    fn resume_runs_even_if_writing_panics() {
        let resumed = Cell::new(0);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_resume(|| Some(42), |v: i32| resumed.set(resumed.get() + v), || -> () { panic!("test panic while writing") })
        }));
        assert!(r.is_err());
        assert_eq!(resumed.get(), 42);
        // ふつうに終わったときも1回だけ
        let v = with_resume(|| Some(1), |v: i32| resumed.set(resumed.get() + v), || "done");
        assert_eq!((v, resumed.get()), ("done", 43));
        // 手放していなければ（再生中の曲でなければ）呼ばない
        with_resume(|| None::<i32>, |_| panic!("must not resume"), || ());
    }
}
