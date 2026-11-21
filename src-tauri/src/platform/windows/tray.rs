//! タスクトレイ。メニューは「表示」「再生/一時停止」「次」「前」「既定の見た目に戻す」（v3.2.0）「立ち絵を出さない」（v3.9.0）「終了」。

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::commands::AppState;
use crate::playback::{PlaybackListener, Snapshot};

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// 完全に終了する（状態を保存してから）
pub fn quit(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.controller.save_all();
    }
    app.exit(0);
}

/// v3.10.0（直し2回目）：「既定の見た目に戻す」も「立ち絵を出さない」と同じく別のスレッドへ渡す。中で設定を書く鍵（settings_write）を取り、
/// 設定の zip の読み込みはその鍵を読み込みの間ずっと持つので、メインのスレッドで呼ぶと読み込みが終わるまで固まるため
fn reset_look_off_main(app: &AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("tray-reset-look".into()).spawn(move || {
        if let Err(e) = crate::skins::runtime::reset_look(&app) {
            crate::commands::notice_error(&app, &e);
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("tray: could not start reset-look thread: {e}");
    }
}

/// v3.9.0：トレイのメニューの「立ち絵を出さない」
pub(crate) const HIDE_AVATAR_ID: &str = "hide-avatar";

/// v3.9.0：「立ち絵を出さない」。メニューの処理はメインのスレッドで走るので、設定の保存・ファイルの確かめ・通知は別のスレッドへ渡し、
/// メインのスレッドでは待たない（鍵を持ったままメインのスレッドを待つ形を作らない。v3.8.1 のホットキーの直しと同じ考え）。
/// 画面（WebView）は通らない
fn hide_avatar_off_main(app: &AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("tray-hide-avatar".into()).spawn(move || {
        if let Err(e) = crate::skins::runtime::hide_avatar(&app) {
            crate::commands::notice_error(&app, &e);
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("tray: could not start hide-avatar thread: {e}");
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let show = MenuItem::with_id(app, "show", "表示", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "再生 / 一時停止", true, None::<&str>)?;
    let next = MenuItem::with_id(app, "next", "次の曲", true, None::<&str>)?;
    let prev = MenuItem::with_id(app, "previous", "前の曲", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    // v3.2.0：着せ替えの逃げ道（画面が読めない配色にしてしまったときも、ここから戻せる）
    // 「既定の見た目に戻す」はスキンと手で選んだ色（v3.10.0）を戻し、立ち絵は残す（v3.9.0）
    let reset_skin = MenuItem::with_id(app, "reset-skin", "既定の見た目に戻す", true, None::<&str>)?;
    // v3.9.0：立ち絵だけを外す逃げ道（立ち絵の画像で画面が崩れたときも、画面を通らずに外せる。作った立ち絵は消さない）
    let hide_avatar = MenuItem::with_id(app, HIDE_AVATAR_ID, "立ち絵を出さない", true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &toggle, &next, &prev, &sep, &reset_skin, &hide_avatar, &sep2, &quit_item])?;
    let mut b = TrayIconBuilder::with_id("main")
        .tooltip("PL Player 3")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main(app),
            "toggle" => super::hotkeys::dispatch(app, "playPause"),
            "next" => super::hotkeys::dispatch(app, "next"),
            "previous" => super::hotkeys::dispatch(app, "previous"),
            "reset-skin" => {
                reset_look_off_main(app);
                show_main(app);
            }
            HIDE_AVATAR_ID => hide_avatar_off_main(app),
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.build(app)
}

/// トレイのヒントに今の曲を出す
pub struct TrayListener(pub AppHandle);

impl PlaybackListener for TrayListener {
    fn on_snapshot(&self, s: &Snapshot) {
        let tip = match &s.track {
            Some(t) if !t.artist.is_empty() => format!("PL Player 3\n{} - {}", t.title, t.artist),
            Some(t) => format!("PL Player 3\n{}", t.title),
            None => "PL Player 3".to_string(),
        };
        // ヒントは最大 127 文字
        let tip: String = tip.chars().take(120).collect();
        if let Some(tray) = self.0.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(tip));
        }
    }
}

pub fn listener(app: &AppHandle) -> Arc<dyn PlaybackListener> {
    Arc::new(TrayListener(app.clone()))
}
