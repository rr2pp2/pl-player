//! ライブラリ：取り込み（4経路）・一覧・お気に入り・フォルダの管理。

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::{ev, AppState};
use crate::db::tracks::{self, Group, Track, TrackQuery};
use crate::error::AppResult;
use crate::library::{import, legacy_reread};
use crate::platform::windows::localpath;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub track_count: i64,
}

#[tauri::command]
pub async fn app_info(app: AppHandle, state: State<'_, AppState>) -> AppResult<AppInfo> {
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        data_dir: state.paths.data_dir.to_string_lossy().into_owned(),
        track_count: tracks::count(&state.db)?,
    })
}

#[tauri::command]
pub async fn library_query(state: State<'_, AppState>, query: TrackQuery) -> AppResult<Vec<Track>> {
    tracks::query(&state.db, &query)
}

/// v3.6.0：年間まとめ（画面を開いた時点の今から 365 日さかのぼって、再生した回数の上位 100 曲。決まりは db::history）
#[tauri::command]
pub async fn library_yearly_top(state: State<'_, AppState>, search: Option<String>) -> AppResult<crate::db::history::YearlyTop> {
    crate::db::history::yearly_top(&state.db, crate::db::now_ms(), search.as_deref())
}

#[tauri::command]
pub async fn library_groups(
    state: State<'_, AppState>,
    kind: String,
    search: Option<String>,
) -> AppResult<Vec<Group>> {
    tracks::groups(&state.db, &kind, search.as_deref())
}

#[tauri::command]
pub async fn track_get(state: State<'_, AppState>, id: i64) -> AppResult<Option<Track>> {
    tracks::get(&state.db, id)
}

#[tauri::command]
pub async fn tracks_get(state: State<'_, AppState>, ids: Vec<i64>) -> AppResult<Vec<Track>> {
    tracks::get_many(&state.db, &ids)
}

#[tauri::command]
pub async fn set_favorite(app: AppHandle, state: State<'_, AppState>, ids: Vec<i64>, favorite: bool) -> AppResult<()> {
    tracks::set_favorite(&state.db, &ids, favorite)?;
    for id in &ids {
        state.controller.refresh_track(*id);
    }
    let _ = app.emit(ev::LIBRARY, ());
    Ok(())
}

/// 取り込み中の印を、スレッドの終わり方（panic を含む）にかかわらず必ず戻す（v3.1.1。ペネトレーションテスト v3.0.2 の2）
pub(crate) struct ImportingGuard<'a>(pub(crate) &'a std::sync::Mutex<bool>);

impl Drop for ImportingGuard<'_> {
    fn drop(&mut self) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = false;
    }
}

/// 取り込みの本体。panic しても結果を返す（取り込みの印は呼ぶ側のガードが戻す）。
/// `skip_missing` のときは、この PC のドライブにあるものだけ `exists()` で確かめ、無いフォルダを黙って外す
/// （起動時・再走査・設定の読み込み。v3.1.1：起動処理を止めないよう、確かめるのはこの別スレッドの中だけ。
/// ネットワーク上の場所には触らず、import::run が断って件数に入れる）
pub(crate) fn run_import(
    db: &crate::db::Db,
    paths: Vec<PathBuf>,
    recursive: bool,
    remember_folders: bool,
    skip_missing: bool,
    progress: &dyn Fn(&import::ImportProgress),
) -> import::ImportSummary {
    let paths: Vec<PathBuf> = if skip_missing {
        paths
            .into_iter()
            .filter(|p| !localpath::is_local(p) || p.exists())
            .collect()
    } else {
        paths
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| import::run(db, &paths, recursive, remember_folders, progress))) {
        Ok(s) => s,
        Err(_) => {
            tracing::error!("import thread panicked");
            import::ImportSummary {
                other_errors: vec!["internal error".into()],
                ..Default::default()
            }
        }
    }
}

/// 取り込みを別スレッドで走らせる。進み具合と結果は通知で返す
pub fn start_import(app: &AppHandle, paths: Vec<PathBuf>, recursive: bool, remember_folders: bool) {
    start_import_with(app, paths, recursive, remember_folders, false)
}

/// 記録したフォルダの取り込み直し（起動時・再走査・設定の読み込み）。無いフォルダは黙って外す
pub fn start_rescan(app: &AppHandle, folders: Vec<PathBuf>) {
    start_import_with(app, folders, true, false, true)
}

fn start_import_with(app: &AppHandle, paths: Vec<PathBuf>, recursive: bool, remember_folders: bool, skip_missing: bool) {
    let state = app.state::<AppState>();
    // 走っている取り込み・古い文字コードのタグの読み直し（v3.5.0）が終わったら、もう一度呼んでもらう
    if let Err(code) = crate::library::legacy_reread::begin_import(&state.importing, &state.legacy_reread) {
        let _ = app.emit(ev::NOTICE, crate::playback::Notice::info(code, None, None));
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let guard = ImportingGuard(&state.importing);
        let app2 = app.clone();
        let summary = run_import(&state.db, paths, recursive, remember_folders, skip_missing, &move |p| {
            let _ = app2.emit(ev::IMPORT_PROGRESS, p);
        });
        drop(guard);
        let _ = app.emit(ev::IMPORT_DONE, &summary);
        let _ = app.emit(ev::LIBRARY, ());
    });
}

/// ドラッグ＆ドロップ・設定の読み込みなど、パスを直接渡す経路
#[tauri::command]
pub async fn library_import(app: AppHandle, paths: Vec<String>) -> AppResult<()> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    // ドロップされたフォルダは「フォルダ追加」と同じ扱い（サブフォルダまで・記録する）
    start_import(&app, paths, true, true);
    Ok(())
}

/// ファイル選択・複数選択
#[tauri::command]
pub async fn library_pick_files(app: AppHandle) -> AppResult<bool> {
    let picked = app
        .dialog()
        .file()
        .set_title("取り込む曲を選ぶ")
        .add_filter("音声ファイル", crate::library::scanner::AUDIO_EXTS)
        .blocking_pick_files();
    let Some(files) = picked else { return Ok(false) };
    let paths: Vec<PathBuf> = files.into_iter().filter_map(|f| f.into_path().ok()).collect();
    start_import(&app, paths, false, false);
    Ok(true)
}

/// フォルダ追加（サブフォルダまで）
#[tauri::command]
pub async fn library_pick_folder(app: AppHandle) -> AppResult<bool> {
    let picked = app
        .dialog()
        .file()
        .set_title("取り込むフォルダを選ぶ")
        .blocking_pick_folder();
    let Some(folder) = picked.and_then(|f| f.into_path().ok()) else {
        return Ok(false);
    };
    start_import(&app, vec![folder], true, true);
    Ok(true)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderEntry {
    pub path: String,
    pub recursive: bool,
}

#[tauri::command]
pub async fn library_folders(state: State<'_, AppState>) -> AppResult<Vec<FolderEntry>> {
    Ok(import::library_folders(&state.db)
        .into_iter()
        .map(|(path, recursive)| FolderEntry { path, recursive })
        .collect())
}

#[tauri::command]
pub async fn library_remove_folder(state: State<'_, AppState>, path: String) -> AppResult<()> {
    import::remove_library_folder(&state.db, &path);
    Ok(())
}

/// 再走査：記録したフォルダを取り込み直し、無くなった曲に印を付ける
#[tauri::command]
pub async fn library_rescan(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let folders: Vec<PathBuf> = import::library_folders(&state.db)
        .into_iter()
        .map(|(p, _)| PathBuf::from(p))
        .collect();
    let db = state.db.clone();
    let app2 = app.clone();
    std::thread::spawn(move || {
        import::check_missing(&db);
        let _ = app2.emit(ev::LIBRARY, ());
    });
    if !folders.is_empty() {
        // 再走査の無いフォルダは今までどおり「見つからない」として知らせる（ネットワーク上の場所は import::run が断る）
        start_import(&app, folders, true, false);
    }
    Ok(())
}

/// ライブラリから外す（ファイルは消さない）
#[tauri::command]
pub async fn library_remove_tracks(app: AppHandle, state: State<'_, AppState>, ids: Vec<i64>) -> AppResult<()> {
    tracks::remove(&state.db, &ids)?;
    state.controller.on_tracks_removed(&ids);
    let _ = app.emit(ev::LIBRARY, ());
    let _ = app.emit(ev::PLAYLISTS, ());
    Ok(())
}

/// 曲ファイルをごみ箱へ移し、移せた曲をライブラリから外す（v3.3.0）。画面からは曲の id だけを受ける（パスは DB から引く）。
/// 設定（`ui.trash_button`）は見ない（v3.3.0 の直し）。右クリックのメニューには設定にかかわらず「ごみ箱へ移す…」を出すので、
/// 設定は「行の右端にボタンを出すか」だけの意味になった。ここで設定を見ると、メニューからの操作が断られる。
/// 「どこから押したか」を画面から受けて分ける形は取らない（呼べるのは同じ画面だけで、名乗りを信じることになり、守りにならない）。
/// 守りは今までどおり：画面は必ず確認を出す（trashTracksFlow）。ここでは曲の id だけを受け、パスは DB から引き、
/// 音声ファイルか・ごみ箱の有る場所か・数の上限を library::trash が確かめる（完全に消える操作はしない）
#[tauri::command]
pub async fn library_trash_tracks(app: AppHandle, state: State<'_, AppState>, ids: Vec<i64>) -> AppResult<crate::library::trash::TrashReport> {
    let db = state.db.clone();
    let controller = state.controller.clone();
    let report = tauri::async_runtime::spawn_blocking(move || crate::library::trash::trash_tracks(&db, Some(&controller), &ids))
        .await
        .map_err(|e| crate::error::AppError::other(format!("trash task: {e}")))??;
    if !report.moved.is_empty() {
        let _ = app.emit(ev::LIBRARY, ());
        let _ = app.emit(ev::PLAYLISTS, ());
    }
    Ok(report)
}

/// 古い文字コードのタグを読み直す（v3.5.0。設定の「ライブラリ」のボタン。確認は画面が出す）。
/// DB の値に今の決まりをかけ直すだけで、曲ファイルは読まない・書かない（判断は library::legacy_reread）。
/// 取り込み・再走査が走っていれば断る。別スレッドで走らせ、進み具合は pl://legacy-reread-progress、終わりは pl://legacy-reread-done
#[tauri::command]
pub async fn legacy_reread_start(app: AppHandle, state: State<'_, AppState>) -> AppResult<legacy_reread::RereadProgress> {
    legacy_reread::begin_exclusive(&state.importing, &state.legacy_reread)?;
    let initial = legacy_reread::RereadProgress { running: true, ..Default::default() };
    state.legacy_reread.set_progress(initial.clone());
    let app2 = app.clone();
    std::thread::spawn(move || {
        let state = app2.state::<AppState>();
        let ctl = &state.legacy_reread;
        let end = ctl.end_guard();
        let cancel = ctl.cancel_flag();
        let emit_app = app2.clone();
        let mut on_progress = |p: &legacy_reread::RereadProgress| {
            ctl.set_progress(p.clone());
            let _ = emit_app.emit(ev::LEGACY_REREAD_PROGRESS, p);
        };
        // 途中で落ちても、終わりの知らせを必ず出し、走っている印を外す
        let (done, fixed) = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            legacy_reread::run(&state.db, &cancel, &mut on_progress)
        })) {
            Ok(r) => r,
            Err(_) => {
                tracing::error!("legacy tag reread thread panicked");
                let mut p = ctl.progress();
                p.running = false;
                p.finished = true;
                p.error = Some(crate::error::ErrorKind::Other);
                (p, vec![])
            }
        };
        ctl.set_progress(done.clone());
        // 終わりの知らせより先に印を外す（知らせを受けた画面が、すぐにもう一度押せるように）
        drop(end);
        if !fixed.is_empty() {
            if let Some(t) = state.controller.snapshot().track {
                if fixed.contains(&t.id) {
                    state.controller.refresh_track(t.id);
                }
            }
            let _ = app2.emit(ev::LIBRARY, ());
        }
        let _ = app2.emit(ev::LEGACY_REREAD_DONE, &done);
    });
    Ok(initial)
}

/// 古い文字コードのタグの読み直しを、やめる（次のまとまりの前で止まる。それまでに直した曲はそのまま）
#[tauri::command]
pub async fn legacy_reread_cancel(state: State<'_, AppState>) -> AppResult<()> {
    state.legacy_reread.cancel();
    Ok(())
}

/// 今の進み具合（設定画面を開き直したとき）
#[tauri::command]
pub async fn legacy_reread_status(state: State<'_, AppState>) -> AppResult<legacy_reread::RereadProgress> {
    Ok(state.legacy_reread.progress())
}

/// エクスプローラーでファイルの場所を開く（そのファイルを選んだ状態で）
///
/// v1 の不具合：`Command::arg` は空白を含む引数を丸ごと引用符で囲む（`"/select,D:\Music\01 - a b.mp3"`）。
/// explorer.exe はこの形を読めず、既定のフォルダ（ドキュメント）を開いていた。
/// explorer が読める形（`/select,"D:\Music\01 - a b.mp3"`）を、そのまま渡す。
#[tauri::command]
pub async fn reveal_in_explorer(path: String) -> AppResult<()> {
    let p = std::path::PathBuf::from(&path);
    // v3.1.1：ネットワーク上の場所は exists() の前に断る（触ると SMB の接続が出る）
    localpath::check_local(&p)?;
    if !p.exists() {
        return Err(crate::error::AppError::new(crate::error::ErrorKind::FileNotFound, path));
    }
    reveal(&p).map_err(|e| crate::error::AppError::from_io(&e, &p))
}

/// explorer.exe へ渡す引数（テストで形を確かめる）
pub fn explorer_select_arg(p: &std::path::Path) -> String {
    // 区切りを Windows の形にそろえる（取り込み経路によっては / が混ざる）
    let s = p.to_string_lossy().replace('/', "\\");
    format!("/select,\"{s}\"")
}

#[cfg(windows)]
fn reveal(p: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("explorer.exe")
        .raw_arg(explorer_select_arg(p))
        .spawn()
        .map(|_| ())
}

#[cfg(not(windows))]
fn reveal(_p: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

/// フォルダを開くときに explorer.exe へ渡す引数（`reveal` と同じく、パスだけを引用符で囲む）
pub fn explorer_open_arg(p: &std::path::Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\");
    format!("\"{s}\"")
}

/// フォルダをエクスプローラーで開く（v3：歌詞の受け渡しフォルダ）。開き方は `reveal` と同じ
#[cfg(windows)]
pub fn open_folder(p: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    // v3.1.1：ネットワーク上の場所は開かない（受け渡しフォルダは PL_PLAYER_DATA_DIR で動かせるため）
    if !localpath::is_local(p) {
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "network location is not opened"));
    }
    std::process::Command::new("explorer.exe")
        .raw_arg(explorer_open_arg(p))
        .spawn()
        .map(|_| ())
}

#[cfg(not(windows))]
pub fn open_folder(_p: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_arg_quotes_only_the_path() {
        let a = explorer_select_arg(std::path::Path::new(r"D:\Music\01 - a b, live.mp3"));
        assert_eq!(a, r#"/select,"D:\Music\01 - a b, live.mp3""#);
        let b = explorer_select_arg(std::path::Path::new("D:/Music/a.mp3"));
        assert_eq!(b, r#"/select,"D:\Music\a.mp3""#);
    }

    #[test]
    fn open_arg_quotes_the_folder() {
        let a = explorer_open_arg(std::path::Path::new(r"C:\Users\x y\AppData\Roaming\jp.local.plplayer3\lyrics\inbox"));
        assert_eq!(a, r#""C:\Users\x y\AppData\Roaming\jp.local.plplayer3\lyrics\inbox""#);
        assert_eq!(explorer_open_arg(std::path::Path::new("C:/a/b")), r#""C:\a\b""#);
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の2）：取り込みのスレッドが panic で抜けても、取り込み中の印は戻る
    #[test]
    fn importing_flag_is_reset_even_on_panic() {
        let m = std::sync::Mutex::new(true);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = ImportingGuard(&m);
            panic!("test panic in the import thread");
        }));
        assert!(r.is_err());
        assert!(!*m.lock().unwrap_or_else(|p| p.into_inner()));
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の5）：エクスプローラーで開く前に、ネットワーク上の場所を断る（文字だけで。触らない）
    #[test]
    fn reveal_refuses_network_paths_before_touching() {
        for bad in [r"\\127.0.0.1\share\a.mp3", r"\\pl-player-test.invalid\share\a.mp3", "//pl-player-test.invalid/s/a.mp3"] {
            let e = tauri::async_runtime::block_on(reveal_in_explorer(bad.into())).unwrap_err();
            assert_eq!(e.kind, crate::error::ErrorKind::NetworkPathUnsupported, "{bad}");
            assert!(open_folder(std::path::Path::new(bad)).is_err(), "{bad}");
        }
    }

    /// 記録したフォルダの取り込み直しは、この PC に無いフォルダを黙って外す（起動時と同じ）。
    /// `\\?\` で始まるパスは、ローカルのフォルダを指していても、走査の前に断る（文字だけで決めている証拠）
    #[test]
    fn rescan_skips_missing_and_refuses_odd_paths_before_scanning() {
        let db = crate::db::Db::open_memory("cmd_rescan").unwrap();
        let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures");
        let verbatim = PathBuf::from(format!(r"\\?\{}", fixtures.display()));
        let s = run_import(
            &db,
            vec![fixtures.join("no-such-folder-plp"), verbatim.clone()],
            true,
            false,
            true,
            &|_| {},
        );
        assert!(s.not_found.is_empty(), "{s:?}");
        assert_eq!(s.network, vec![verbatim.to_string_lossy().into_owned()]);
        assert_eq!(s.added, 0, "{s:?}");
    }
}
