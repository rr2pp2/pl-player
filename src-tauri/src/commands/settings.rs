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

/// 書き込みの結果：書く前の設定と、書いた設定（`change` が None を返したときは書いていない）
pub(crate) struct Written {
    pub before: AppSettings,
    pub after: Option<AppSettings>,
}

/// 設定の「読む → 直す → 書く」を、書く人どうしの鍵（`AppState::settings_write`）の中で一続きにする（v3.10.0）。
/// `change` は今の設定を受け取り、書く設定を返す（None なら書かない）。`change` の中では、メインのスレッド・ほかの鍵
/// （Runtime の鍵・再生の司令塔）を待つ呼び出しをしない（v3.8.1 で、鍵を持ったままメインのスレッドを待って固まったため）。
/// 鍵の順番は いつも settings_write → settings（写しの鍵。値を出し入れする間だけ）で、逆に取る所は無い
pub(crate) fn write_locked(
    db: &crate::db::Db,
    write: &std::sync::Mutex<()>,
    current: &std::sync::Mutex<AppSettings>,
    change: impl FnOnce(&AppSettings) -> AppResult<Option<AppSettings>>,
) -> AppResult<Written> {
    let _w = write.lock().unwrap_or_else(|p| p.into_inner());
    let before = current.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let Some(next) = change(&before)? else {
        return Ok(Written { before, after: None });
    };
    let next = next.sanitize();
    store::save(db, &next)?;
    *current.lock().unwrap_or_else(|p| p.into_inner()) = next.clone();
    Ok(Written { before, after: Some(next) })
}

/// `write_locked` で書き、書いたら画面へ知らせる（書く鍵を外してから `emit_latest`）。返すのは知らせた最新の設定
pub(crate) fn update_and_emit(
    app: &AppHandle,
    state: &AppState,
    change: impl FnOnce(&AppSettings) -> AppResult<Option<AppSettings>>,
) -> AppResult<(Written, AppSettings)> {
    let w = write_locked(&state.db, &state.settings_write, &state.settings, change)?;
    let now = if w.after.is_some() { emit_settings(app, state) } else { state.settings() };
    Ok((w, now))
}

/// 設定の知らせを、その時点の最新の設定で送る（v3.10.0 直し2回目）
pub(crate) fn emit_settings(app: &AppHandle, state: &AppState) -> AppSettings {
    emit_latest(&state.settings_emit, &state.settings, |s| {
        let _ = app.emit(ev::SETTINGS, s);
    })
}

/// 知らせる人どうしの鍵の中で、最新の設定を読み直してから `send` に渡す。読んでから送るまでの間に別の知らせが割り込まないので、
/// 知らせは読んだ順に並ぶ。書く人はみな書いたあとでここを通るので、最後に送られる知らせは、最後に書いた値を含む
/// （前は鍵の外で読んで送っていたので、先に読んだ古い値が、あとの値より後に届くことがあった）。
/// 鍵の中は写しの鍵を一瞬取って送るだけ。書く鍵（settings_write）の中からは呼ばない
pub(crate) fn emit_latest(
    emit_lock: &std::sync::Mutex<()>,
    current: &std::sync::Mutex<AppSettings>,
    send: impl FnOnce(&AppSettings),
) -> AppSettings {
    let _e = emit_lock.lock().unwrap_or_else(|p| p.into_inner());
    let now = current.lock().unwrap_or_else(|p| p.into_inner()).clone();
    send(&now);
    now
}

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> AppResult<AppSettings> {
    Ok(state.settings())
}

/// 設定を保存する。ホットキーが変わっていれば登録し直し、結果（競合）を返す
#[tauri::command]
pub async fn settings_set(app: AppHandle, state: State<'_, AppState>, settings: AppSettings) -> AppResult<AppSettings> {
    // v3.10.0：画面に変えさせない値（使うスキン・使う立ち絵・手で選んだ色・背景のパス）を戻す元の「今の設定」は、鍵の中で読む
    let bg_dir = state.paths.bg_dir.clone();
    let (w, now) = update_and_emit(&app, &state, |before| Ok(Some(from_screen(settings, before, &bg_dir))))?;
    // ホットキーの登録し直しはメインのスレッドへ渡すので、鍵を外してから
    if let Some(s) = w.after.as_ref().filter(|s| s.hotkeys != w.before.hotkeys) {
        let statuses = hotkeys::apply(&app, &s.hotkeys);
        let _ = app.emit(ev::HOTKEYS, &statuses);
    }
    Ok(now)
}

/// 画面から来た設定のうち、画面に変えさせない値を今の値に戻す
pub(crate) fn from_screen(s: AppSettings, before: &AppSettings, bg_dir: &Path) -> AppSettings {
    let mut s = keep_background_inside(s, before, bg_dir);
    // v3.2.0：使うスキンは skins_apply（確かめてから当てる）だけが変える。画面から来た値は使わない
    s.skin.active_skin_id = before.skin.active_skin_id.clone();
    // v3.9.0：使う立ち絵も同じ（変えるのは avatars_use・削除だけ）。「立ち絵を出す」の値は sanitize がこれに合わせる
    s.skin.avatar = before.skin.avatar.clone();
    // v3.10.0：手で選んだ色も同じ（変えるのは skins_color_set・skins_colors_clear・「既定の見た目に戻す」だけ。
    // 画面が持っている古い写しで、選んだばかりの色を上書きしないため）
    s.skin.colors = before.skin.colors.clone();
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
    let path = dest.to_string_lossy().into_owned();
    update_and_emit(&app, &state, |before| {
        let mut s = before.clone();
        s.background.path = Some(path);
        Ok(Some(s))
    })
    .map(|(_, now)| now)
}

#[tauri::command]
pub async fn background_clear(app: AppHandle, state: State<'_, AppState>) -> AppResult<AppSettings> {
    update_and_emit(&app, &state, |before| {
        let mut s = before.clone();
        s.background.path = None;
        Ok(Some(s))
    })
    .map(|(_, now)| now)
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
    // v3.8.0：出力先の設定は Rust が DB に持つ。つまみを動かしている途中の値も入るよう、書き出す前に保存し直す
    state.controller.save_outputs();
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
    // v3.10.0：読み込み（この PC の着せ替えの設定を読んで zip の値と合わせ、DB に書く）と写しの置き換えを、書く人どうしの鍵の中で一続きにする。
    // 鍵の中は zip と DB を読み書きするだけ（出力先の取り込み・ホットキー・知らせは鍵の外）
    let backup::Imported { settings, report, folders, outputs } = {
        let _w = state.settings_write.lock().unwrap_or_else(|p| p.into_inner());
        let imported = backup::import_full(&state.db, &src, &state.paths.bg_dir)?;
        *state.settings.lock().unwrap_or_else(|p| p.into_inner()) = imported.settings.clone();
        imported
    };
    // v3.8.0：zip に出力先の設定があれば、一時停止して全部オフで入れる（自動では再開しない）。無ければ出力に触らない
    if let Some(doc) = outputs {
        state.controller.import_outputs(doc);
    }
    let statuses = hotkeys::apply(&app, &settings.hotkeys);
    let _ = app.emit(ev::HOTKEYS, &statuses);
    emit_settings(&app, &state);
    let _ = app.emit(ev::PLAYLISTS, ());
    let _ = app.emit(ev::LIBRARY, ());
    // v3.9.0：zip の「使う立ち絵」がこの PC に無い立ち絵・スキンなら「スキンの立ち絵」に戻し、画面の立ち絵を当て直す
    crate::skins::runtime::after_settings_import(&app);
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
        let s = from_screen(from.clone(), &before, bg);
        assert_eq!(s.skin.active_skin_id, "quiet-paper");
        // v3.9.0：使う立ち絵も、画面から来た値では変わらない（変えるのは avatars_use・削除だけ）
        let mut b2 = before.clone();
        b2.skin.set_avatar("avatar:0123456789abcdef");
        let mut f2 = from.clone();
        f2.skin.avatar = "skin:never-checked".into();
        let s2 = from_screen(f2, &b2, bg).sanitize();
        assert_eq!(s2.skin.avatar, "avatar:0123456789abcdef");
        // 「立ち絵を出す」は使う立ち絵に合わせる（画面から false が来ても、出さないにはならない）
        assert!(s2.skin.character_visible);
        let mut b3 = before.clone();
        b3.skin.set_avatar("none");
        let s3 = from_screen(from.clone(), &b3, bg).sanitize();
        assert_eq!((s3.skin.avatar.as_str(), s3.skin.character_visible), ("none", false));
        assert!(!s.skin.character_visible && s.skin.safe_mode_next_start);
        assert!((s.skin.character_scale - 0.8).abs() < 1e-6);
        assert_eq!(s.theme, "light");
        // 既定のスキンのときに、画面から別の id が来ても既定のまま
        let s = from_screen(AppSettings { skin: crate::settings::store::SkinSettings { active_skin_id: "x".into(), ..Default::default() }, ..AppSettings::default() }, &AppSettings::default(), bg);
        assert_eq!(s.skin.active_skin_id, crate::skins::names::BUILTIN_ID);
    }

    /// v3.10.0（直し1回目）：画面の保存と手の色の保存が同時に走っても、読んでから書くまでが一続きなので、どちらの変更も落ちない。
    /// 画面の保存は手の色を知らない古い写し（手の色なし）を何度も送り、その間に手の色を1つずつ 15 個足す。2つずつ同時に足す形も
    #[test]
    fn concurrent_writes_keep_every_change() {
        use crate::skins::tokens::COLOR_VARS;
        use std::sync::Mutex;
        let bg = Path::new(r"C:\data\backgrounds");
        let db = crate::db::Db::open_memory("settings_write_race").unwrap();
        let write = Mutex::new(());
        let cur = Mutex::new(AppSettings::default());
        let sample = |n: &str| if n.ends_with("-rgb") { "1, 2, 3".to_string() } else { "#123456".to_string() };
        std::thread::scope(|sc| {
            sc.spawn(|| {
                for i in 0..300 {
                    let mut screen = AppSettings::default();
                    screen.theme = if i % 2 == 0 { "light".into() } else { "dark".into() };
                    write_locked(&db, &write, &cur, |before| Ok(Some(from_screen(screen, before, bg)))).unwrap();
                }
            });
            for half in [&COLOR_VARS[..8], &COLOR_VARS[8..]] {
                let (db, write, cur) = (&db, &write, &cur);
                sc.spawn(move || {
                    for n in half {
                        write_locked(db, write, cur, |before| {
                            let mut s = before.clone();
                            crate::skins::colors::set(&mut s.skin.colors, n, Some(&sample(n)))?;
                            Ok(Some(s))
                        })
                        .unwrap();
                        std::thread::yield_now();
                    }
                });
            }
        });
        let now = cur.lock().unwrap().clone();
        assert_eq!(now.skin.colors.len(), 15, "{:?}", now.skin.colors.keys().collect::<Vec<_>>());
        assert_eq!(store::load(&db).skin.colors, now.skin.colors);
        assert_eq!(now.theme, "dark");
        // 書かないとき（None）は DB にも写しにも触らない。断ったときも同じ
        let w = write_locked(&db, &write, &cur, |_| Ok(None)).unwrap();
        assert!(w.after.is_none() && w.before == now);
        let e = write_locked(&db, &write, &cur, |b| {
            let mut s = b.clone();
            crate::skins::colors::set(&mut s.skin.colors, "--c-bg", Some("url(x)"))?;
            Ok(Some(s))
        });
        assert!(e.is_err());
        assert_eq!(*cur.lock().unwrap(), now);
        assert_eq!(store::load(&db), now);
    }

    /// v3.10.0（直し2回目）：書く人が何人いても、知らせは読んだ順に並び、最後に届く知らせは最後に書いた値になる
    /// （書いたあとで emit_latest を通る。送り先はここでは Vec に積むだけ）
    #[test]
    fn last_notice_carries_the_last_write() {
        use std::sync::Mutex;
        let db = crate::db::Db::open_memory("settings_emit_order").unwrap();
        let write = Mutex::new(());
        let emit = Mutex::new(());
        let cur = Mutex::new(AppSettings::default());
        let sent: Mutex<Vec<AppSettings>> = Mutex::new(Vec::new());
        std::thread::scope(|sc| {
            for t in 0..4u32 {
                let (db, write, emit, cur, sent) = (&db, &write, &emit, &cur, &sent);
                sc.spawn(move || {
                    for i in 0..100u32 {
                        write_locked(db, write, cur, |b| {
                            let mut s = b.clone();
                            s.ui.sidebar_icon_px = 14 + (t * 100 + i) % 19;
                            s.background.dim = ((t * 100 + i) % 90) as f32 / 100.0;
                            Ok(Some(s))
                        })
                        .unwrap();
                        emit_latest(emit, cur, |s| sent.lock().unwrap().push(s.clone()));
                    }
                });
            }
        });
        let sent = sent.into_inner().unwrap();
        assert_eq!(sent.len(), 400);
        assert_eq!(sent.last().unwrap(), &*cur.lock().unwrap());
        assert_eq!(store::load(&db), *cur.lock().unwrap());
    }

    /// v3.10.0：画面から来た設定では、手で選んだ色は変わらない（足す・変える・消す・細工した値のどれも）。ほかの設定は画面の値を受ける
    #[test]
    fn settings_set_keeps_manual_colors() {
        let bg = Path::new(r"C:\data\backgrounds");
        let mut before = AppSettings::default();
        before.skin.colors.insert("--c-accent".into(), "#112233".into());
        for colors in [
            serde_json::json!({}),
            serde_json::json!({"--c-accent": "#ffffff"}),
            serde_json::json!({"--c-accent": "#112233", "--c-bg": "#000000"}),
            serde_json::json!({"--c-bg": "url(x)"}),
        ] {
            let mut from = before.clone();
            from.skin.colors = crate::skins::colors::from_json(&colors);
            from.theme = "light".into();
            let s = from_screen(from, &before, bg).sanitize();
            assert_eq!(s.skin.colors, before.skin.colors, "{colors}");
            assert_eq!(s.theme, "light");
        }
    }
}
