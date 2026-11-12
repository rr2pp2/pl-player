//! 組み立てだけ（プラグイン登録・状態の注入・起動時の復元）。判断はそれぞれの部品が持つ。

pub mod audio;
pub mod commands;
pub mod db;
pub mod error;
pub mod library;
mod logging;
pub mod lyrics;
pub mod platform;
pub mod playback;
pub mod settings;

use std::sync::{Arc, Mutex};

/// テストが書く一時ファイルの置き場所（プロジェクトの中の target\test-tmp。PC の一時フォルダは使わない）
#[cfg(test)]
pub(crate) fn test_tmp() -> std::path::PathBuf {
    let d = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("test-tmp");
    let _ = std::fs::create_dir_all(&d);
    d
}

use tauri::{Emitter, Manager, RunEvent, WindowEvent};

use commands::{ev, AppState, Paths, UiEmitter};
use platform::windows::{device_watch, hotkeys, smtc, thumbbar, tray};

/// データの置き場所。既定は `%APPDATA%\<識別子>`。
/// 環境変数 `PL_PLAYER_DATA_DIR` があればそちら（動作テストで、PC の既定の場所を汚さないため）。
fn data_dir_override() -> Option<std::path::PathBuf> {
    std::env::var_os("PL_PLAYER_DATA_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let data_dir = match data_dir_override() {
        Some(d) => d,
        None => app.path().app_data_dir()?,
    };
    // メインのウィンドウは設定（tauri.conf.json）から作る。置き場所を変えているときは WebView のデータもそこへ
    if let Some(cfg) = app.config().app.windows.iter().find(|w| w.label == "main").cloned() {
        let mut b = tauri::WebviewWindowBuilder::from_config(app, &cfg)?;
        if data_dir_override().is_some() {
            b = b.data_directory(data_dir.join("webview"));
        }
        b.build()?;
    }
    let paths = Paths {
        thumbs_dir: data_dir.join("artwork").join("embedded"),
        art_dir: data_dir.join("artwork").join("external"),
        bg_dir: data_dir.join("backgrounds"),
        data_dir: data_dir.clone(),
    };
    for d in [&paths.data_dir, &paths.thumbs_dir, &paths.art_dir, &paths.bg_dir] {
        std::fs::create_dir_all(d)?;
    }
    logging::init(data_dir.join("logs"));
    // 画面が読める画像は、データフォルダの artwork と backgrounds の中だけ（v3.1.1。ペネトレーションテスト v3.0.2 の14）。
    // 設定（tauri.conf.json）の $APPDATA/artwork・$APPDATA/backgrounds に加え、置き場所を変えたときもそこだけを許す。
    // データフォルダ全体（library.db・logs・lyrics・webview）は許さない
    for d in asset_dirs(&data_dir) {
        let _ = app.asset_protocol_scope().allow_directory(&d, true);
    }
    tracing::info!("start {}", app.package_info().version);

    let db = match db::Db::open(&data_dir.join("library.db"), paths.thumbs_dir.clone()) {
        Ok(db) => Arc::new(db),
        Err(e) => {
            // データベースが開けないと何もできない：知らせて終わる
            tracing::error!("database open failed: {e}");
            use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
            handle
                .dialog()
                .message(format!(
                    "ライブラリのデータベースを開けませんでした。\n\n{}\n\n場所：{}",
                    e.detail,
                    data_dir.display()
                ))
                .title("PL Player")
                .kind(MessageDialogKind::Error)
                .blocking_show();
            return Err(Box::new(e));
        }
    };
    let settings = settings::store::load(&db);
    let controller = playback::Controller::new(db.clone());
    controller.add_listener(Arc::new(UiEmitter(handle.clone())));

    app.manage(AppState {
        db,
        controller: controller.clone(),
        paths,
        settings: Mutex::new(settings.clone()),
        importing: Mutex::new(false),
        lyrics_fetch: Default::default(),
    });

    // タスクトレイ
    if tray::build(&handle).is_ok() {
        controller.add_listener(tray::listener(&handle));
    }

    // SMTC（メディアキー）とサムネイルボタン：ウィンドウに結びつける
    if let Some(w) = app.get_webview_window("main") {
        if let Ok(hwnd) = w.hwnd() {
            if let Some(l) = smtc::attach(&handle, hwnd) {
                controller.add_listener(l);
            }
            if let Some(l) = thumbbar::attach(&handle, hwnd) {
                controller.add_listener(l);
            }
        }
    }

    // デバイスの抜き差し
    let c2 = controller.clone();
    device_watch::start(move || c2.notify_devices_changed());

    // グローバルホットキー（設定にあるものだけ）
    let statuses = hotkeys::apply(&handle, &settings.hotkeys);
    let _ = handle.emit(ev::HOTKEYS, &statuses);

    if settings.rescan_on_start {
        // v3.1.1（ペネトレーションテスト v3.0.2 の5）：ここでは exists() を呼ばない（ネットワーク上の場所だと起動が止まり、
        // SMB の接続も出る）。在るかの確かめとネットワーク上の場所の断りは、取り込みのスレッドの中で行う
        let folders: Vec<std::path::PathBuf> = library::import::library_folders(&controller_db(&handle))
            .into_iter()
            .map(|(p, _)| std::path::PathBuf::from(p))
            .collect();
        if !folders.is_empty() {
            commands::library::start_rescan(&handle, folders);
        }
    }
    Ok(())
}

/// 画面（asset プロトコル）が読んでよいフォルダ（v3.1.1）。曲の縮小版（artwork\embedded）・外部画像と見本（artwork\external）・背景
pub(crate) fn asset_dirs(data_dir: &std::path::Path) -> [std::path::PathBuf; 2] {
    [data_dir.join("artwork"), data_dir.join("backgrounds")]
}

fn controller_db(app: &tauri::AppHandle) -> Arc<db::Db> {
    app.state::<AppState>().db.clone()
}

pub fn run() {
    let app = tauri::Builder::default()
        // 二重起動の防止：2つ目を起動したら1つ目を前に出す（最初に登録する）
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin({
            let b = tauri_plugin_window_state::Builder::default();
            match data_dir_override() {
                // 絶対パスを渡すと、既定の設定フォルダではなくそこへ書く
                Some(d) => b.with_filename(d.join("window-state.json").to_string_lossy()),
                None => b,
            }
            .build()
        })
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(setup)
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() != "main" {
                    return;
                }
                let app = window.app_handle();
                let to_tray = app
                    .try_state::<AppState>()
                    .map(|s| s.settings().close_to_tray)
                    .unwrap_or(false);
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                    if let Some(s) = app.try_state::<AppState>() {
                        s.controller.save_all();
                    }
                } else {
                    tray::quit(app);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::library::app_info,
            commands::library::library_query,
            commands::library::library_groups,
            commands::library::track_get,
            commands::library::tracks_get,
            commands::library::set_favorite,
            commands::library::library_import,
            commands::library::library_pick_files,
            commands::library::library_pick_folder,
            commands::library::library_folders,
            commands::library::library_remove_folder,
            commands::library::library_rescan,
            commands::library::library_remove_tracks,
            commands::library::reveal_in_explorer,
            commands::playback::playback_snapshot,
            commands::playback::play_context,
            commands::playback::playback_action,
            commands::playback::playback_seek,
            commands::playback::playback_set_volume,
            commands::playback::playback_commit_volume,
            commands::playback::playback_set_muted,
            commands::playback::playback_set_shuffle,
            commands::playback::playback_set_repeat,
            commands::queue::queue_get,
            commands::queue::queue_add,
            commands::queue::queue_play_next,
            commands::queue::queue_remove,
            commands::queue::queue_move,
            commands::queue::queue_clear,
            commands::queue::queue_play_at,
            commands::playlist::playlists_list,
            commands::playlist::playlist_create,
            commands::playlist::playlist_rename,
            commands::playlist::playlist_delete,
            commands::playlist::playlist_add_tracks,
            commands::playlist::playlist_remove_items,
            commands::playlist::playlist_move_item,
            commands::playlist::playlist_entries,
            commands::playlist::playlist_play,
            commands::metadata::metadata_read,
            commands::metadata::metadata_write,
            commands::metadata::pick_image,
            commands::metadata::stage_image,
            commands::metadata::album_art_set_external,
            commands::metadata::album_art_clear_external,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::settings::hotkeys_status,
            commands::settings::background_set,
            commands::settings::background_clear,
            commands::settings::settings_export,
            commands::settings::settings_import,
            commands::devices::devices_list,
            commands::devices::device_set,
            commands::lyrics::get_lyrics,
            commands::lyrics::preview_lyrics,
            commands::lyrics::save_manual_lyrics,
            commands::lyrics::delete_manual_lyrics,
            commands::lyrics::set_lyrics_delay,
            commands::lyrics::import_lyrics_inbox,
            commands::lyrics::open_lyrics_inbox,
            commands::lyrics::lyrics_fetch_start,
            commands::lyrics::lyrics_fetch_cancel,
            commands::lyrics::lyrics_fetch_status,
            commands::lyrics::lyrics_fetch_track,
        ])
        .build(tauri::generate_context!())
        .expect("error while building PL Player");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            if let Some(s) = app.try_state::<AppState>() {
                s.controller.save_all();
            }
            tracing::info!("exit");
        }
    });
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    fn manifest_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn json(rel: &str) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(manifest_dir().join(rel)).unwrap()).unwrap()
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if matches!(p.extension().and_then(|x| x.to_str()), Some("ts" | "tsx")) {
                out.push(p);
            }
        }
    }

    /// 画面のソース（src\**\*.ts・*.tsx）が `@tauri-apps/...` から取り込んでいる名前（モジュール, 名前）
    fn tauri_imports() -> (BTreeSet<(String, String)>, Vec<(PathBuf, String)>) {
        let mut files = vec![];
        walk(&manifest_dir().join("..").join("src"), &mut files);
        assert!(files.len() > 20, "frontend sources not found");
        let mut names = BTreeSet::new();
        let mut texts = vec![];
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            for line in text.lines() {
                let l = line.trim();
                if !(l.starts_with("import ") && l.contains("\"@tauri-apps/")) {
                    continue;
                }
                let module = l.split('"').nth(1).unwrap().to_string();
                let inner = l.split('{').nth(1).and_then(|x| x.split('}').next()).unwrap_or("*");
                for n in inner.split(',') {
                    let n = n.trim().trim_start_matches("type ").trim();
                    if !n.is_empty() {
                        names.insert((module.clone(), n.to_string()));
                    }
                }
            }
            texts.push((f, text));
        }
        (names, texts)
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の7）：画面に渡す権限を core:default から、event の listen と unlisten だけに絞った。
    ///
    /// 画面が使う Tauri の API を洗い出し、絞った権限で足りることを確かめる：
    /// - `invoke`（アプリのコマンド）：アプリが定義したコマンドは権限の外（build.rs に AppManifest を書いていない）
    /// - `listen`（pl://… の通知）：`plugin:event|listen`、やめるときは `plugin:event|unlisten` → allow-listen・allow-unlisten
    /// - `getCurrentWebview().onDragDropEvent`（ドラッグ＆ドロップ）：中で同じ `listen` を4つ（drag-enter・over・drop・leave）
    /// - `convertFileSrc`（画像の表示）：URL を作るだけで IPC を呼ばない。読めるかは assetProtocol.scope が決める
    /// - 通知（トースト）は pl://notice の listen。トレイ・ウィンドウ・ファイル選択・ホットキーは Rust 側だけ
    ///
    /// 画面に新しい API を足したら、このテストが落ちる（権限を見直す合図）
    #[test]
    fn capability_is_narrow_and_covers_the_frontend() {
        let cap = json("capabilities/default.json");
        let perms: Vec<&str> = cap["permissions"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(perms, ["core:event:allow-listen", "core:event:allow-unlisten"]);
        assert_eq!(cap["windows"], serde_json::json!(["main"]));
        // ほかの capability のファイルを置いていない（置くと権限が足される）
        let files: Vec<_> = std::fs::read_dir(manifest_dir().join("capabilities")).unwrap().flatten().collect();
        assert_eq!(files.len(), 1);
        // build.rs はアプリのコマンドに権限を求めない形のまま
        assert!(std::fs::read_to_string(manifest_dir().join("build.rs")).unwrap().contains("tauri_build::build()"));

        let (names, texts) = tauri_imports();
        let allowed: BTreeSet<(String, String)> = [
            ("@tauri-apps/api/core", "invoke"),
            ("@tauri-apps/api/core", "convertFileSrc"),
            ("@tauri-apps/api/event", "listen"),
            ("@tauri-apps/api/event", "UnlistenFn"),
            ("@tauri-apps/api/webview", "getCurrentWebview"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let extra: Vec<_> = names.difference(&allowed).collect();
        assert!(extra.is_empty(), "frontend uses Tauri APIs not covered by the capability: {extra:?}");
        // 使っている API は、実際に使われている（洗い出しが空振りしていない）
        assert!(names.contains(&("@tauri-apps/api/event".into(), "listen".into())));
        assert!(names.contains(&("@tauri-apps/api/webview".into(), "getCurrentWebview".into())));
        for (f, text) in &texts {
            // 画面から直接 plugin:… を呼んでいない・emit していない
            assert!(!text.contains("plugin:"), "{}", f.display());
            // getCurrentWebview() から使うのはドラッグ＆ドロップの受け取りだけ
            for (i, _) in text.match_indices("getCurrentWebview()") {
                let after = text[i + "getCurrentWebview()".len()..].trim_start();
                assert!(after.starts_with(".onDragDropEvent"), "{}: getCurrentWebview() used for something else", f.display());
            }
        }
        // onDragDropEvent が中で使うのは listen だけ（unlisten は返り値の関数から）。取り込んだ版の実物で確かめる
        let api = std::fs::read_to_string(manifest_dir().join("..").join("node_modules/@tauri-apps/api/webview.js")).unwrap();
        let body = &api[api.find("async onDragDropEvent(").unwrap()..];
        let body = &body[..body.find("\n    }\n").unwrap()];
        assert_eq!(body.matches("await this.listen(").count(), 4);
        assert!(!body.contains("invoke("));
        let ev = std::fs::read_to_string(manifest_dir().join("..").join("node_modules/@tauri-apps/api/event.js")).unwrap();
        assert!(ev.contains("invoke('plugin:event|listen'") && ev.contains("invoke('plugin:event|unlisten'"));
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の14）：asset プロトコルで画面が読めるのは artwork と backgrounds だけ。
    /// 画面が表示に使う画像（曲の縮小版・外部画像と見本・背景）はその中にあり、library.db・ログ・歌詞・WebView のデータは外。
    /// 音声は Rust が読んで鳴らすので、asset プロトコルは通らない
    #[test]
    fn asset_scope_is_artwork_and_backgrounds_only() {
        let conf = json("tauri.conf.json");
        assert_eq!(
            conf["app"]["security"]["assetProtocol"]["scope"],
            serde_json::json!(["$APPDATA/artwork/**", "$APPDATA/backgrounds/**"])
        );
        let data = Path::new(r"C:\Users\x\AppData\Roaming\jp.local.plplayer3");
        let dirs = super::asset_dirs(data);
        let allowed = |p: &Path| dirs.iter().any(|d| p.starts_with(d));
        // lib.rs の setup と同じ置き場所
        for ok in [
            data.join("artwork").join("embedded").join("0123456789abcdef.jpg"),
            data.join("artwork").join("external").join("album-0123456789abcdef.png"),
            data.join("artwork").join("external").join("staged-0123456789abcdef.png"),
            data.join("backgrounds").join("bg-0123456789abcdef.webp"),
        ] {
            assert!(allowed(&ok), "{}", ok.display());
        }
        for ng in [
            data.join("library.db"),
            data.join("library.db-wal"),
            data.join("logs").join("pl-player.log"),
            data.join("lyrics").join("inbox").join("1.lrc"),
            data.join("webview").join("EBWebView"),
            data.join("window-state.json"),
            data.parent().unwrap().join("jp.local.plplayer2").join("library.db"),
        ] {
            assert!(!allowed(&ng), "{}", ng.display());
        }
        // 画面が asset で開く URL は、events.ts の fileUrl（convertFileSrc）だけ
        let (_, texts) = tauri_imports();
        let users: Vec<_> = texts.iter().filter(|(_, t)| t.contains("convertFileSrc(")).map(|(f, _)| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(users, ["events.ts"]);
    }
}
