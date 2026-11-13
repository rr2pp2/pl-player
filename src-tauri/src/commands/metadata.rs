//! メタデータ編集とアルバムアート（埋め込み画像の差し替え・外部画像の設定・画像削除）。

use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use super::{ev, AppState};
use crate::db::Db;
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
        || write_and_reload(&state.db, &track, &path, &values, &art),
    );
    state.controller.refresh_track(id);
    let _ = app.emit(ev::LIBRARY, ());
    result?;
    track_or_err(&state, id)
}

/// 曲ファイルのタグに書き、成否に関わらずライブラリの値を実ファイルから読み直す。
/// v3.5.0：書けたら、手で変えた項目（保存する前のライブラリの値と違う項目）に印を付ける（古い文字コードの読み直しで書き換えないため）
pub(crate) fn write_and_reload(db: &Db, track: &Track, path: &Path, values: &TagValues, art: &ArtChange) -> AppResult<()> {
    let edited = crate::library::legacy_reread::edited_fields(track, values);
    let result = tags_write::write_tags(path, values, art);
    let read = tags_read::read_track(path, &db.thumbs_dir);
    let conn = db.writer();
    if let Ok(info) = read {
        let _ = tracks::upsert(&conn, &info);
    }
    if result.is_ok() {
        if let Err(e) = tracks::mark_manual_edits(&conn, track.id, edited) {
            tracing::warn!("could not mark manual tag edits: {:?}", e.kind);
        }
    }
    result
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
    use super::{with_resume, write_and_reload};
    use crate::db::{tracks, Db};
    use crate::library::tags_read::{self, TagValues};
    use crate::library::tags_write::ArtChange;
    use std::cell::Cell;
    use std::sync::atomic::AtomicBool;

    /// v3.5.0：編集画面で保存すると、手で変えた項目にだけ印が付く（保存に失敗したら付けない）。
    /// 印の付いた項目は、古い文字コードの読み直しで（決まりを変えた版を見立てても）書き換えない。曲名・名前は架空のもの
    #[test]
    fn saving_in_the_editor_marks_only_changed_fields() {
        let dir = crate::test_tmp().join(format!("plp-edit-mark-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // 曲名は「さくら色の坂道」（Shift_JIS を Latin-1 の枠に）、アーティストは西欧の名前（今の決まりでは直らない）
        let mp3 = tags_read::test_legacy_mp3(
            &dir,
            "a.mp3",
            &[(b"TIT2", b"\x82\xb3\x82\xad\x82\xe7\x90F\x82\xcc\x8d\xe2\x93\xb9"), (b"TPE1", b"\xd1and\xfa Br\xfbl\xe9")],
            None,
        );
        let db = Db::open_memory("edit_mark").unwrap();
        crate::library::import::run(&db, &[mp3.clone()], false, false, &|_| {});
        let id = tracks::id_by_path(&db, &mp3.to_string_lossy()).unwrap().unwrap();
        let track = tracks::get(&db, id).unwrap().unwrap();
        assert_eq!(track.title, "さくら色の坂道");
        assert_eq!(track.artist, "Ñandú Brûlé");
        // 編集画面に出た値のまま、アルバムだけを手で入れて保存する
        let mut v = tags_read::read_values(&mp3).unwrap();
        v.album = "手で入れたアルバム".into();
        write_and_reload(&db, &track, &mp3, &v, &ArtChange::Keep).unwrap();
        let marks = |db: &Db| -> Option<i64> {
            db.reader()
                .query_row("SELECT fields FROM tag_manual_edits WHERE track_id = ?1", [id], |r| r.get(0))
                .ok()
        };
        assert_eq!(marks(&db), Some(4), "アルバムだけ");
        assert_eq!(tracks::get(&db, id).unwrap().unwrap().album, "手で入れたアルバム");
        // 次にアーティストを手で直す（印は足される）
        let track = tracks::get(&db, id).unwrap().unwrap();
        v.artist = "Ñandú Brûlé Ça".into();
        write_and_reload(&db, &track, &mp3, &v, &ArtChange::Keep).unwrap();
        assert_eq!(marks(&db), Some(4 | 2));
        // 決まりを変えた版で、西欧の名前も「直る」ことになったと見立てても、手で直したアーティストは書き換えない
        let future = |s: &str, _: Option<&str>| -> Option<String> { s.contains('Ñ').then(|| "変わった".to_string()) };
        let (_, fixed) = crate::library::legacy_reread::run_with(&db, &AtomicBool::new(false), &future, 200, &mut |_| {});
        assert!(fixed.is_empty());
        assert_eq!(tracks::get(&db, id).unwrap().unwrap().artist, "Ñandú Brûlé Ça");
        // 書けなかったとき（ファイルが無い）は印を付けない
        let db2 = Db::open_memory("edit_mark_fail").unwrap();
        crate::library::import::run(&db2, &[mp3.clone()], false, false, &|_| {});
        let id2 = tracks::id_by_path(&db2, &mp3.to_string_lossy()).unwrap().unwrap();
        let t2 = tracks::get(&db2, id2).unwrap().unwrap();
        std::fs::remove_file(&mp3).unwrap();
        let bad = TagValues { title: "別の名前".into(), ..v.clone() };
        assert!(write_and_reload(&db2, &t2, &mp3, &bad, &ArtChange::Keep).is_err());
        assert_eq!(marks(&db2), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

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
