//! 歌詞（v3）の窓口。薄く保ち、判断は lyrics::service が持つ。
//! 曲ファイルには書き込まない（タグの書き込み処理にはつながない）。

use tauri::State;

use super::AppState;
use crate::error::AppResult;
use crate::lyrics::inbox::InboxReport;
use crate::lyrics::service::{self, LyricsPayload, LyricsPreview};

/// 優先順（manual ＞ external ＞ embedded_tag）で選んだ歌詞
#[tauri::command]
pub async fn get_lyrics(state: State<'_, AppState>, track_id: i64) -> AppResult<LyricsPayload> {
    service::get_lyrics(&state.db, track_id)
}

/// 編集画面の「保存前の見本」（保存しない）
#[tauri::command]
pub async fn preview_lyrics(raw_text: String) -> AppResult<LyricsPreview> {
    Ok(service::preview(&raw_text))
}

#[tauri::command]
pub async fn save_manual_lyrics(state: State<'_, AppState>, track_id: i64, raw_text: String) -> AppResult<LyricsPayload> {
    service::save_manual(&state.db, track_id, &raw_text)
}

#[tauri::command]
pub async fn delete_manual_lyrics(state: State<'_, AppState>, track_id: i64) -> AppResult<LyricsPayload> {
    service::delete_manual(&state.db, track_id)
}

#[tauri::command]
pub async fn set_lyrics_delay(state: State<'_, AppState>, track_id: i64, delay_ms: i64) -> AppResult<i64> {
    service::set_delay(&state.db, track_id, delay_ms)
}

/// 受け渡しフォルダを取り込む（ボタンを押したときだけ。フォルダの監視はしない）
#[tauri::command]
pub async fn import_lyrics_inbox(state: State<'_, AppState>) -> AppResult<InboxReport> {
    service::import_inbox(&state.db, &state.paths.data_dir)
}

/// 受け渡しフォルダをエクスプローラーで開く（無ければ作る）
#[tauri::command]
pub async fn open_lyrics_inbox(state: State<'_, AppState>) -> AppResult<String> {
    let dir = service::inbox_dir(&state.paths.data_dir)?;
    super::library::open_folder(&dir).map_err(|e| crate::error::AppError::from_io(&e, &dir))?;
    Ok(dir.to_string_lossy().into_owned())
}

// ---- 歌詞をネットから取る（v3.1.0） ----
//
// どの窓口も、最初に online::begin で設定を確かめる。**オフなら、ここで断って何もしない**
// （通信のセッションも作らない）。走っている途中でオフにされたら、次の要求を送る前に gate が止める。

use tauri::{AppHandle, Emitter, Manager};

use super::ev;
use crate::lyrics::lrclib::{self, Client};
use crate::lyrics::online::{self, FetchDone, FetchProgress, Mode, OneResult, RealEnv};
use crate::platform::windows::winhttp::WinHttpTransport;

/// 送る直前に見る：やめるボタン → 今の設定
fn fetch_gate(app: AppHandle, cancel: std::sync::Arc<std::sync::atomic::AtomicBool>) -> impl Fn() -> Option<lrclib::Stop> {
    online::gate(cancel, move || app.try_state::<AppState>().map(|s| s.settings().lyrics_online).unwrap_or(false))
}

/// 一括で取る（別スレッド。進み具合は pl://lyrics-fetch-progress、終わりは pl://lyrics-fetch-done）
#[tauri::command]
pub async fn lyrics_fetch_start(app: AppHandle, state: State<'_, AppState>, mode: Mode) -> AppResult<FetchProgress> {
    online::begin(&state.settings(), &state.lyrics_fetch)?; // ★ オフならここで断る
    let initial = FetchProgress { running: true, mode: Some(mode), ..Default::default() };
    state.lyrics_fetch.set_progress(initial.clone());
    let app2 = app.clone();
    std::thread::spawn(move || {
        let state = app2.state::<AppState>();
        let ctl = &state.lyrics_fetch;
        // 途中で落ちても、終わりの知らせを必ず出し、処理中の印を外す
        let app3 = app2.clone();
        let guard = online::DoneGuard::new(ctl, move |d: &FetchDone| {
            let _ = app3.emit(ev::LYRICS_FETCH_DONE, d);
        });
        let done = match WinHttpTransport::new(&lrclib::user_agent()) {
            Ok(transport) => {
                let cancel = ctl.cancel_flag();
                let mut client = Client::new(transport, RealEnv::new(cancel.clone()), fetch_gate(app2.clone(), cancel));
                client.last = ctl.last_request();
                let (progress, track_ids) = online::run_batch(&state.db, mode, &mut client, &online::tag_has_lyrics, &online::tag_has_title, &mut |p| {
                    ctl.set_progress(p.clone());
                    let _ = app2.emit(ev::LYRICS_FETCH_PROGRESS, p);
                });
                ctl.set_last_request(client.last);
                FetchDone { progress, track_ids }
            }
            Err(_) => {
                tracing::warn!("lyrics fetch: could not open WinHTTP session");
                FetchDone {
                    progress: FetchProgress { finished: true, mode: Some(mode), aborted: Some("network_init_failed".into()), ..Default::default() },
                    track_ids: vec![],
                }
            }
        };
        guard.finish(done);
    });
    Ok(initial)
}

/// やめるボタン（次の待ち・次の要求の前で止まる）
#[tauri::command]
pub async fn lyrics_fetch_cancel(state: State<'_, AppState>) -> AppResult<()> {
    state.lyrics_fetch.cancel();
    Ok(())
}

/// 今の進み具合（設定画面を開き直したとき）
#[tauri::command]
pub async fn lyrics_fetch_status(state: State<'_, AppState>) -> AppResult<FetchProgress> {
    Ok(state.lyrics_fetch.progress())
}

/// 1曲だけ取る（歌詞画面の「ネットから探す」）
#[tauri::command]
pub async fn lyrics_fetch_track(app: AppHandle, state: State<'_, AppState>, track_id: i64) -> AppResult<OneResult> {
    online::begin(&state.settings(), &state.lyrics_fetch)?; // ★ オフならここで断る
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app2.state::<AppState>();
        let ctl = &state.lyrics_fetch;
        let _end = ctl.end_guard();
        let transport = match WinHttpTransport::new(&lrclib::user_agent()) {
            Ok(t) => t,
            Err(_) => {
                return Ok(OneResult { track_id, status: "error".into(), error: Some("network_init_failed".into()) });
            }
        };
        let cancel = ctl.cancel_flag();
        let mut client = Client::new(transport, RealEnv::new(cancel.clone()), fetch_gate(app2.clone(), cancel));
        client.last = ctl.last_request();
        let r = online::fetch_one(&state.db, track_id, &mut client);
        ctl.set_last_request(client.last);
        r
    })
    .await
    .map_err(|e| crate::error::AppError::other(e.to_string()))?
}
