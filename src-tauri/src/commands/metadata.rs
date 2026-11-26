//! メタデータ編集とアルバムアート（埋め込み画像の差し替え・外部画像の設定・画像削除）。

use std::path::Path;

use rusqlite::OptionalExtension;
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

/// 実ファイルのタグへ保存する（一時ファイル → 検証 → 置換）。
/// v3.6.0：返すのは保存したあとのライブラリの曲。曲ファイルには書いたが、その間に曲がライブラリから外された（ライブラリには戻していない）ときは None。
/// 保存の時点で曲がもうライブラリに無ければ、曲ファイルにも書かずに `TrackRemovedBeforeSave` を返す（`write_and_reload`）
#[tauri::command]
pub async fn metadata_write(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    values: TagValues,
    art: Option<ArtChange>,
) -> AppResult<Option<Track>> {
    let path = tracks::path_of(&state.db, id)?.ok_or_else(|| removed_before_save(id))?;
    let path = std::path::PathBuf::from(path);
    let art = art.unwrap_or_default();
    let controller = &state.controller;
    // 再生中の曲なら、差し替えの前に手放して位置を覚え、終わったら（panic で抜けても）必ず再開する（v3.1.1）
    let result = with_resume(
        || controller.release_for_write(id),
        |r| controller.resume_after_write(r),
        || write_and_reload(&state.db, id, &path, &values, &art),
    );
    state.controller.refresh_track(id);
    let _ = app.emit(ev::LIBRARY, ());
    match result? {
        // 保存のあとに外された（外す操作が、保存のトランザクションのあとに走った）ときも None になる
        Saved::InLibrary => tracks::get(&state.db, id),
        Saved::RemovedMeanwhile => Ok(None),
    }
}

fn removed_before_save(id: i64) -> AppError {
    AppError::new(ErrorKind::TrackRemovedBeforeSave, format!("track {id}"))
}

/// 編集画面の保存の結果（v3.6.0）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Saved {
    /// 曲はライブラリにある（読み直した値を書いた。トランザクションを始められず読み直せなかったときも、こちら）
    InLibrary,
    /// 曲ファイルに書いている間に、曲がライブラリから外された。ライブラリには戻していない（読み直しも印も書いていない）
    RemovedMeanwhile,
}

/// 曲がライブラリにあるか：同じ id の行が、同じパスのまま残っているか（v3.6.0）。
/// パスも見るのは、外した曲の id が、あとから取り込んだ別の曲に振り直されることがあるため
/// （`tracks.id` は AUTOINCREMENT でないので、いちばん大きい id の行を消すと、次の行が同じ id になる）
fn in_library(conn: &rusqlite::Connection, id: i64, path: &Path) -> AppResult<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM tracks WHERE id = ?1 AND path = ?2",
            rusqlite::params![id, path.to_string_lossy()],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// 曲ファイルのタグに書き、成否に関わらずライブラリの値を実ファイルから読み直す。
/// v3.5.0：書けたら、手で変えた項目（保存する前のライブラリの値と違う項目）に印を付ける（古い文字コードの読み直しで書き換えないため）。
///
/// v3.6.0：保存の時点で曲がもうライブラリに無ければ、ライブラリに戻さない。
/// 3.5.0 までは、編集画面を開いたまま曲を「ライブラリから外す」「ごみ箱へ移す」と、保存のあとの読み直し（`tracks::upsert`）が
/// 同じパスで行を作り直し、外した曲がライブラリに戻っていた。
/// - 書く前：書き込みの鍵（`db.writer()`）の中で、曲がまだあるか（`in_library`）を確かめる。無ければ曲ファイルにも書かず、
///   `TrackRemovedBeforeSave` を返す（外した曲は、使う人がもう要らないとした曲なので触らない）。確かめられない（DB のエラー）ときは、
///   3.5.0 までと同じく曲ファイルには書く（DB が壊れていてもタグは保存できるように。ライブラリに戻さないのは下で守る）
/// - 書いたあと：読み直しと印を書くトランザクションの中で、もう一度確かめる。曲ファイルに書いている間（鍵の外）に外されていたら、
///   読み直しも印も書かず `Saved::RemovedMeanwhile` を返す（曲ファイルには書いてある。外す操作より保存が先だったのと同じ形。
///   曲ファイルに書けなかったとき（ごみ箱へ移されて、その場所に無い）は `TrackRemovedBeforeSave`）。
///   確かめるのと書くのが同じトランザクションの中なので、その間に外す操作は割り込めない（外す操作は書き込みの鍵を待つ）
///
/// 比べる「保存する前のライブラリの値」は、書き込みの鍵（`db.writer()`）を取ってから、同じ接続で読む。
/// 編集画面を開いたときの値（呼び出し側が先に取った `Track`）とは比べない。その間に古い文字コードの読み直し・取り込みが
/// その曲を書き換えていると、古い値と比べることになるため（3.5.0 の検査の指摘。決まりが同じ 3.5.0 では起きないが、決まりを変える版で効く）。
/// - 鍵の順：書き込みの鍵の中では、読み取りの鍵（`db.reader()`）を取らない（同じ接続で読む）。読み直し・取り込みも
///   読み取りの鍵を手放してから書き込みの鍵を取るので、鍵を持ったまま相手の鍵を待つ組み合わせが無い
/// - 曲ファイルから読み直した値と印を、1つのトランザクションで書く。古い文字コードの読み直しが「新しい値なのに印が無い」途中の形を読んで、手で入れた値を直してしまわないため
/// - 曲ファイルを読む・書くのは鍵の外（重い処理の間、ほかの書き込みを止めない）
pub(crate) fn write_and_reload(db: &Db, id: i64, path: &Path, values: &TagValues, art: &ArtChange) -> AppResult<Saved> {
    write_and_reload_with(db, id, path, values, art, &|| {})
}

/// `write_and_reload` の中身。`before_write` は、書く前の確かめが済んでから曲ファイルに書くまでの間に呼ぶ
/// （テストで、その間に曲を外すため。ふだんは何もしない）
pub(crate) fn write_and_reload_with(
    db: &Db,
    id: i64,
    path: &Path,
    values: &TagValues,
    art: &ArtChange,
    before_write: &dyn Fn(),
) -> AppResult<Saved> {
    // 書き込みの鍵の中で確かめる（確かめたら鍵を手放す。曲ファイルに書いている間、ほかの書き込みを止めない）
    match in_library(&db.writer(), id, path) {
        Ok(true) => {}
        Ok(false) => return Err(removed_before_save(id)),
        Err(e) => tracing::warn!("could not check the track before saving: {:?}", e.kind),
    }
    before_write();
    let result = tags_write::write_tags(path, values, art);
    let read = tags_read::read_track(path, &db.thumbs_dir);
    let mut conn = db.writer();
    let tx = match conn.transaction() {
        Ok(tx) => tx,
        Err(e) => {
            tracing::warn!("could not reload the saved tags: {e}");
            return result.map(|_| Saved::InLibrary);
        }
    };
    // 曲ファイルに書いている間に外されていたら、読み直し（upsert）で行を作り直さない。確かめられないときも書かない（戻さないほうに倒す）
    // 書けなかったときは、外されたほうを知らせる（ごみ箱へ移すと曲ファイルがその場所から無くなり、書けない。
    // 書き込みは失敗したら元のファイルを変えないので、「保存しませんでした（曲ファイルは変えていません）」のとおり）
    match in_library(&tx, id, path) {
        Ok(true) => {}
        Ok(false) => return result.map(|_| Saved::RemovedMeanwhile).map_err(|_| removed_before_save(id)),
        Err(e) => {
            tracing::warn!("could not check the track after saving: {:?}", e.kind);
            return result.map(|_| Saved::InLibrary);
        }
    }
    // 書き込みの鍵の中で取り直す（上で確かめたので、ふつうは Some）
    let before = if result.is_ok() { tracks::legacy_fields(&tx, id) } else { Ok(None) };
    if let Ok(info) = read {
        let _ = tracks::upsert(&tx, &info);
    }
    match before {
        Ok(Some(before)) => {
            let edited = crate::library::legacy_reread::edited_fields(&before, values);
            if let Err(e) = tracks::mark_manual_edits(&tx, id, edited) {
                tracing::warn!("could not mark manual tag edits: {:?}", e.kind);
            }
        }
        Ok(None) => {}
        Err(e) => tracing::warn!("could not read the values before saving: {:?}", e.kind),
    }
    if let Err(e) = tx.commit() {
        tracing::warn!("could not reload the saved tags: {e}");
    }
    result.map(|_| Saved::InLibrary)
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
    use super::{with_resume, write_and_reload, write_and_reload_with, Saved};
    use crate::db::{tracks, Db};
    use crate::error::ErrorKind;
    use crate::library::tags_read::{self, TagValues};
    use crate::library::tags_write::ArtChange;
    use std::cell::Cell;
    use std::sync::atomic::AtomicBool;

    /// 手で直した印の数（全部の曲）
    fn marks(db: &Db) -> i64 {
        db.reader().query_row("SELECT COUNT(*) FROM tag_manual_edits", [], |r| r.get(0)).unwrap()
    }

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
        write_and_reload(&db, track.id, &mp3, &v, &ArtChange::Keep).unwrap();
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
        write_and_reload(&db, track.id, &mp3, &v, &ArtChange::Keep).unwrap();
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
        assert!(write_and_reload(&db2, t2.id, &mp3, &bad, &ArtChange::Keep).is_err());
        assert_eq!(marks(&db2), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// v3.5.0（検査の指摘）：編集画面を開いてから保存するまでに DB の値が変わっていたら、印は変わったあとの値と比べて付ける
    /// （開いたときの古い値とは比べない）。曲名・名前は架空のもの
    #[test]
    fn marks_compare_with_the_values_just_before_saving() {
        let dir = crate::test_tmp().join(format!("plp-edit-fresh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mp3 = tags_read::test_legacy_mp3(
            &dir,
            "a.mp3",
            &[(b"TIT2", b"\x82\xb3\x82\xad\x82\xe7\x90F\x82\xcc\x8d\xe2\x93\xb9"), (b"TPE1", b"\xd1and\xfa Br\xfbl\xe9")],
            None,
        );
        let db = Db::open_memory("edit_mark_fresh").unwrap();
        crate::library::import::run(&db, &[mp3.clone()], false, false, &|_| {});
        let id = tracks::id_by_path(&db, &mp3.to_string_lossy()).unwrap().unwrap();
        // 編集画面を開いた（このときのライブラリの値は 曲名「さくら色の坂道」・アーティスト「Ñandú Brûlé」）
        let opened = tracks::get(&db, id).unwrap().unwrap();
        let mut v = tags_read::read_values(&mp3).unwrap();
        assert_eq!((v.title.as_str(), v.artist.as_str()), (opened.title.as_str(), opened.artist.as_str()));
        // 保存する前に、決まりを変えた版の読み直しがアーティストを直し、ほかの書き込みが曲名を変えたと見立てる
        let future = |s: &str, _: Option<&str>| -> Option<String> { s.contains('Ñ').then(|| "変わった名前".to_string()) };
        let (_, fixed) = crate::library::legacy_reread::run_with(&db, &AtomicBool::new(false), &future, 200, &mut |_| {});
        assert_eq!(fixed, vec![id]);
        db.writer().execute("UPDATE tracks SET title = '別の曲名' WHERE id = ?1", [id]).unwrap();
        // 保存する値：アーティストは今の DB と同じ（手で変えていない）、曲名は開いたときのまま（今の DB とは違う）、アルバムは手で入れた
        v.artist = "変わった名前".into();
        v.album = "手で入れたアルバム".into();
        write_and_reload(&db, opened.id, &mp3, &v, &ArtChange::Keep).unwrap();
        let marks: i64 = db
            .reader()
            .query_row("SELECT fields FROM tag_manual_edits WHERE track_id = ?1", [id], |r| r.get(0))
            .unwrap();
        // 開いたときの値と比べると「アーティスト・アルバム」（2 | 4）になる。保存する直前の値と比べて「曲名・アルバム」
        assert_eq!(marks, 1 | 4);
        let t = tracks::get(&db, id).unwrap().unwrap();
        assert_eq!((t.title.as_str(), t.artist.as_str(), t.album.as_str()), ("さくら色の坂道", "変わった名前", "手で入れたアルバム"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// v3.5.0：トランザクションを始められないときは、ファイルへの書き込みの結果だけを返し、
    /// 読み直しも印も書かない。DB は壊れず、次の保存はふつうに通る。
    /// 書き込みの接続を別のトランザクションの中にしておき（`BEGIN` のまま鍵を手放す）、`BEGIN DEFERRED` を失敗させる。曲名・名前は架空のもの
    #[test]
    fn saving_when_the_transaction_cannot_start() {
        let dir = crate::test_tmp().join(format!("plp-edit-nobegin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mp3 = tags_read::test_legacy_mp3(&dir, "a.mp3", &[(b"TIT2", b"Paper Lantern"), (b"TPE1", b"Test Artist")], None);
        let db = Db::open_memory("edit_mark_nobegin").unwrap();
        crate::library::import::run(&db, &[mp3.clone()], false, false, &|_| {});
        let id = tracks::id_by_path(&db, &mp3.to_string_lossy()).unwrap().unwrap();
        let mut v = tags_read::read_values(&mp3).unwrap();
        v.album = "手で入れたアルバム".into();
        // 書き込みの接続を別のトランザクションの中にする
        db.writer().execute_batch("BEGIN").unwrap();
        assert!(!db.writer().is_autocommit());
        // ファイルには書けた：その結果（Ok）だけを返す
        write_and_reload(&db, id, &mp3, &v, &ArtChange::Keep).unwrap();
        assert_eq!(tags_read::read_values(&mp3).unwrap().album, "手で入れたアルバム");
        // 同じ接続から見ても、読み直し（upsert）も印も書いていない（外のトランザクションに混ざっていない）
        let peek = |db: &Db| -> (String, i64) {
            let w = db.writer();
            let album = w.query_row("SELECT album FROM tracks WHERE id = ?1", [id], |r| r.get(0)).unwrap();
            let marks = w.query_row("SELECT COUNT(*) FROM tag_manual_edits", [], |r| r.get(0)).unwrap();
            (album, marks)
        };
        assert_eq!(peek(&db), (String::new(), 0));
        // ファイルに書けなかった：書き込みのエラーをそのまま返す（種別もファイルへの書き込みと同じ）。
        // v3.6.0：曲はライブラリにあるまま（同じ id・同じパス）、曲ファイルだけを一時的に別の名前へ動かして書けなくする
        let aside = dir.join("aside.mp3");
        std::fs::rename(&mp3, &aside).unwrap();
        let direct = crate::library::tags_write::write_tags(&mp3, &v, &ArtChange::Keep).unwrap_err().kind;
        let e = write_and_reload(&db, id, &mp3, &v, &ArtChange::Keep).unwrap_err();
        std::fs::rename(&aside, &mp3).unwrap();
        assert_eq!(e.kind, direct);
        assert_eq!(peek(&db), (String::new(), 0));
        // 外のトランザクションを戻すと、DB は壊れていない
        db.writer().execute_batch("ROLLBACK").unwrap();
        assert!(db.writer().is_autocommit());
        let ok: String = db.writer().query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
        assert_eq!(ok, "ok");
        let t = tracks::get(&db, id).unwrap().unwrap();
        assert_eq!((t.title.as_str(), t.artist.as_str(), t.album.as_str()), ("Paper Lantern", "Test Artist", ""));
        // 次の保存はふつうに通る（読み直しが入り、手で入れたアルバムに印が付く）
        write_and_reload(&db, id, &mp3, &v, &ArtChange::Keep).unwrap();
        assert_eq!(tracks::get(&db, id).unwrap().unwrap().album, "手で入れたアルバム");
        let marks: i64 = db
            .reader()
            .query_row("SELECT fields FROM tag_manual_edits WHERE track_id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(marks, 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// v3.6.0：編集画面を開いたあと、保存の前に曲がライブラリから外されていたら、
    /// 曲ファイルにも書かず、ライブラリにも戻さない（3.5.0 までは、保存のあとの読み直しが同じパスで行を作り直していた）。
    /// 印も付かない。曲名・名前は架空のもの
    #[test]
    fn saving_a_track_removed_before_saving_writes_nothing() {
        let dir = crate::test_tmp().join(format!("plp-edit-removed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mp3 = tags_read::test_legacy_mp3(&dir, "a.mp3", &[(b"TIT2", b"Paper Lantern"), (b"TPE1", b"Test Artist")], None);
        let db = Db::open_memory("edit_removed_before").unwrap();
        crate::library::import::run(&db, &[mp3.clone()], false, false, &|_| {});
        let id = tracks::id_by_path(&db, &mp3.to_string_lossy()).unwrap().unwrap();
        let mut v = tags_read::read_values(&mp3).unwrap();
        v.album = "手で入れたアルバム".into();
        // 編集画面を開いたあと、保存の前に、曲がライブラリから外された
        tracks::remove(&db, &[id]).unwrap();
        let e = write_and_reload(&db, id, &mp3, &v, &ArtChange::Keep).unwrap_err();
        assert_eq!(e.kind, ErrorKind::TrackRemovedBeforeSave);
        // 曲ファイルは変えていない・ライブラリに戻っていない・印も無い
        assert_eq!(tags_read::read_values(&mp3).unwrap().album, "");
        assert_eq!(tracks::id_by_path(&db, &mp3.to_string_lossy()).unwrap(), None);
        assert_eq!(tracks::count(&db).unwrap(), 0);
        assert_eq!(marks(&db), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// v3.6.0：書く前に確かめたあと、曲ファイルに書いている間に外されたときも、ライブラリに戻さない（読み直しも印も書かない）。
    /// 曲ファイルには書いてある（外す操作より保存が先だったのと同じ形）ので、`RemovedMeanwhile` を返す。
    /// ごみ箱へ移された（曲ファイルがその場所から無くなり、書けない）ときは `TrackRemovedBeforeSave`。曲名・名前は架空のもの
    #[test]
    fn saving_a_track_removed_while_writing_does_not_bring_it_back() {
        let dir = crate::test_tmp().join(format!("plp-edit-removed-mid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mp3 = tags_read::test_legacy_mp3(&dir, "a.mp3", &[(b"TIT2", b"Paper Lantern"), (b"TPE1", b"Test Artist")], None);
        let db = Db::open_memory("edit_removed_mid").unwrap();
        crate::library::import::run(&db, &[mp3.clone()], false, false, &|_| {});
        let id = tracks::id_by_path(&db, &mp3.to_string_lossy()).unwrap().unwrap();
        let mut v = tags_read::read_values(&mp3).unwrap();
        v.album = "手で入れたアルバム".into();
        // 「ライブラリから外す」：確かめたあと、書く前に外す
        let removed = Cell::new(false);
        let hook = || {
            tracks::remove(&db, &[id]).unwrap();
            removed.set(true);
        };
        let saved = write_and_reload_with(&db, id, &mp3, &v, &ArtChange::Keep, &hook).unwrap();
        assert!(removed.get());
        assert_eq!(saved, Saved::RemovedMeanwhile);
        assert_eq!(tags_read::read_values(&mp3).unwrap().album, "手で入れたアルバム");
        assert_eq!(tracks::id_by_path(&db, &mp3.to_string_lossy()).unwrap(), None, "ライブラリに戻さない");
        assert_eq!(tracks::count(&db).unwrap(), 0);
        assert_eq!(marks(&db), 0);

        // 「ごみ箱へ移す」：確かめたあと、曲ファイルがその場所から無くなり（ごみ箱の代わりに別のフォルダへ動かす）、ライブラリから外される
        crate::library::import::run(&db, &[mp3.clone()], false, false, &|_| {});
        let id = tracks::id_by_path(&db, &mp3.to_string_lossy()).unwrap().unwrap();
        let moved_to = dir.join("moved.mp3");
        let hook = || {
            std::fs::rename(&mp3, &moved_to).unwrap();
            tracks::remove(&db, &[id]).unwrap();
        };
        v.album = "もう一度手で入れたアルバム".into();
        let e = write_and_reload_with(&db, id, &mp3, &v, &ArtChange::Keep, &hook).unwrap_err();
        assert_eq!(e.kind, ErrorKind::TrackRemovedBeforeSave);
        assert!(!mp3.exists());
        assert_eq!(tags_read::read_values(&moved_to).unwrap().album, "手で入れたアルバム", "動かした曲ファイルは変えていない");
        assert_eq!(tracks::count(&db).unwrap(), 0);
        assert_eq!(marks(&db), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// v3.6.0：曲の id だけでなくパスも見る。外した曲の id が、あとから取り込んだ別の曲に振り直されていても、
    /// 外した曲の保存は書かず、別の曲の値も印も変えない。曲名・名前は架空のもの
    #[test]
    fn saving_does_not_touch_another_track_that_got_the_same_id() {
        let dir = crate::test_tmp().join(format!("plp-edit-reused-id-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = tags_read::test_legacy_mp3(&dir, "a.mp3", &[(b"TIT2", b"Paper Lantern"), (b"TPE1", b"Test Artist")], None);
        let b = tags_read::test_legacy_mp3(&dir, "b.mp3", &[(b"TIT2", b"Glass Harbor"), (b"TPE1", b"Other Artist")], None);
        let db = Db::open_memory("edit_reused_id").unwrap();
        crate::library::import::run(&db, &[a.clone()], false, false, &|_| {});
        let id = tracks::id_by_path(&db, &a.to_string_lossy()).unwrap().unwrap();
        let mut v = tags_read::read_values(&a).unwrap();
        v.album = "手で入れたアルバム".into();
        // a を外し、b を取り込む（いちばん大きい id を消したので、b に同じ id が振られる）
        tracks::remove(&db, &[id]).unwrap();
        crate::library::import::run(&db, &[b.clone()], false, false, &|_| {});
        assert_eq!(tracks::id_by_path(&db, &b.to_string_lossy()).unwrap(), Some(id), "同じ id が振られる前提");
        let e = write_and_reload(&db, id, &a, &v, &ArtChange::Keep).unwrap_err();
        assert_eq!(e.kind, ErrorKind::TrackRemovedBeforeSave);
        assert_eq!(tags_read::read_values(&a).unwrap().album, "");
        assert_eq!(tracks::id_by_path(&db, &a.to_string_lossy()).unwrap(), None);
        let t = tracks::get(&db, id).unwrap().unwrap();
        assert_eq!((t.title.as_str(), t.album.as_str()), ("Glass Harbor", ""));
        assert_eq!(marks(&db), 0);
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
