//! 着せ替えの適用・既定に戻す・セーフモード・アプリとトレイのアイコン（v3.2.0）。
//! 画面・トレイ・二重起動の知らせから呼ばれる。ファイルの扱いは package.rs、設定の保存は settings::store。

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::manifest::{from_skin, skin_error};
use super::names::BUILTIN_ID;
use super::package::{self, Installed, Inspection, ResolvedSkin, SkinSummary, Staged};
use super::{images, Dirs};
use crate::commands::{ev, AppState};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::playback::Notice as PlaybackNotice;

/// 起動中だけの状態（設定には残さない）
#[derive(Default)]
pub struct Runtime {
    /// セーフモード（この起動の間は既定の見た目。保存しているスキンの選択は変えない）
    pub safe_mode: bool,
    /// 画面へ一度だけ知らせること
    pub notice: Option<SkinNotice>,
    /// 確認待ちの取り込み（1つだけ）
    pub pending: Option<Staged>,
    /// いま画面に当てているスキン（既定のスキンなら None）
    pub current: Option<ResolvedSkin>,
    /// 起動のときの確かめが済んだ（画面が先に状態を聞いてきたら、済むまで待たせる）
    pub started: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkinNotice {
    /// `broken_fallback`（前に選んでいたスキンが壊れていたので既定に戻した）・`safe_mode`（既定の見た目で起動した）・
    /// `safe_mode_second`（2つ目の起動の --safe-mode で既定の見た目にした）・`app_icon_failed`
    pub code: String,
    pub id: Option<String>,
    pub error: Option<ErrorKind>,
}

/// 画面へ渡す着せ替えの状態
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkinState {
    /// 保存しているスキン
    pub active_id: String,
    /// いま当てているスキン（セーフモードなら既定）
    pub effective_id: String,
    pub safe_mode: bool,
    pub skin: Option<ResolvedSkin>,
    pub notice: Option<SkinNotice>,
}

pub fn dirs(state: &AppState) -> Dirs {
    Dirs::new(&state.paths.data_dir)
}

fn app_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

fn lock(state: &AppState) -> std::sync::MutexGuard<'_, Runtime> {
    lock_rt(&state.skin)
}

fn lock_rt(rt: &std::sync::Mutex<Runtime>) -> std::sync::MutexGuard<'_, Runtime> {
    rt.lock().unwrap_or_else(|p| p.into_inner())
}

fn snapshot(state: &AppState, take_notice: bool) -> SkinState {
    let active_id = state.settings().skin.active_skin_id;
    let mut rt = lock(state);
    let notice = if take_notice { rt.notice.take() } else { None };
    SkinState {
        active_id,
        effective_id: rt.current.as_ref().map(|c| c.id.clone()).unwrap_or_else(|| BUILTIN_ID.into()),
        safe_mode: rt.safe_mode,
        skin: rt.current.clone(),
        notice,
    }
}

fn emit(app: &AppHandle, state: &AppState) {
    let _ = app.emit(ev::SKIN, snapshot(state, true));
}

/// 保存しているスキンの id を変える（画面から来た設定ではここは変わらない。commands::settings::settings_set）
fn save_active(app: &AppHandle, state: &AppState, id: &str) -> AppResult<()> {
    let mut s = state.settings();
    if s.skin.active_skin_id == id {
        return Ok(());
    }
    s.skin.active_skin_id = id.to_string();
    crate::commands::settings::store_and_emit(app, state, s).map(|_| ())
}

/// ウィンドウ・タスクバー・トレイのアイコンを、スキンのもの（無ければ組み込みのもの）にする。
/// exe・ショートカット・スタートメニューのアイコンは変わらない（ビルドのときに exe に入れたもの）
pub fn apply_app_icons(app: &AppHandle, inst: Option<&Installed>) {
    let default = app.default_window_icon().cloned();
    let mut failed = false;
    let mut pick = |tray: bool, size: u32| -> Option<tauri::image::Image<'static>> {
        let (path, rel) = inst?.app_icon_file(tray)?;
        let r = std::fs::read(&path)
            .map_err(|e| AppError::from_io(&e, &path))
            .and_then(|data| images::app_icon_rgba(&data, &rel, size));
        match r {
            Ok((rgba, w, h)) => Some(tauri::image::Image::new_owned(rgba, w, h)),
            Err(e) => {
                tracing::warn!("skin app icon failed: {:?} {}", e.kind, rel);
                failed = true;
                None
            }
        }
    };
    let window_icon = pick(false, 256).or_else(|| default.clone());
    let tray_icon = pick(true, 64).or_else(|| default.clone());
    if let Some(w) = app.get_webview_window("main") {
        if let Some(i) = window_icon {
            if w.set_icon(i).is_err() {
                failed = true;
            }
        }
    }
    if let Some(t) = app.tray_by_id("main") {
        if t.set_icon(tray_icon).is_err() {
            failed = true;
        }
    }
    if failed {
        // 組み込みのアイコンに戻して知らせる
        if let Some(d) = default {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_icon(d.clone());
            }
            if let Some(t) = app.tray_by_id("main") {
                let _ = t.set_icon(Some(d));
            }
        }
        let e = skin_error(ErrorKind::SkinApplyFailed, "application icon");
        let _ = app.emit(ev::NOTICE, PlaybackNotice::error(&e));
    }
}

/// 起動のときに決めること（AppHandle を使わない部分。テストで確かめる）
pub struct StartupPlan {
    pub safe_mode: bool,
    /// 「次の起動を既定の見た目にする」を消して保存する
    pub clear_flag: bool,
    /// 当てるスキン
    pub installed: Option<Installed>,
    /// 前に選んでいたスキンが読めなかった（id, 種別）。既定に戻して知らせる
    pub fallback: Option<(String, ErrorKind)>,
}

pub fn plan_startup(sk: &crate::settings::store::SkinSettings, safe_arg: bool, load: impl FnOnce(&str) -> AppResult<Installed>) -> StartupPlan {
    let clear_flag = sk.safe_mode_next_start;
    if safe_arg || clear_flag {
        return StartupPlan { safe_mode: true, clear_flag, installed: None, fallback: None };
    }
    if sk.active_skin_id == BUILTIN_ID {
        return StartupPlan { safe_mode: false, clear_flag, installed: None, fallback: None };
    }
    match load(&sk.active_skin_id) {
        Ok(inst) => StartupPlan { safe_mode: false, clear_flag, installed: Some(inst), fallback: None },
        Err(e) => {
            tracing::warn!("skin {} could not be loaded at start: {:?} {}", sk.active_skin_id, e.kind, e.detail);
            StartupPlan { safe_mode: false, clear_flag, installed: None, fallback: Some((sk.active_skin_id.clone(), e.kind)) }
        }
    }
}

/// 起動のとき（AppState を置いたあと、最初の画面の描画より前）。
/// `safe_arg` は起動の指定に `--safe-mode` があったか。前に選んでいたスキンが壊れていたら、既定に戻して知らせる
pub fn startup(app: &AppHandle, safe_arg: bool) {
    let state = app.state::<AppState>();
    startup_inner(app, &state, safe_arg);
    lock(&state).started = true;
}

fn startup_inner(app: &AppHandle, state: &AppState, safe_arg: bool) {
    let d = dirs(state);
    package::clean_staging(&d);
    let mut s = state.settings();
    let ver = app_version(app);
    let plan = plan_startup(&s.skin, safe_arg, |id| package::load(&d, id, &ver));
    if plan.clear_flag {
        s.skin.safe_mode_next_start = false;
        let _ = crate::commands::settings::store_and_emit(app, state, s.clone());
    }
    if plan.safe_mode {
        tracing::info!("skin: safe mode ({})", if safe_arg { "--safe-mode" } else { "next start" });
        let mut rt = lock(state);
        rt.safe_mode = true;
        rt.current = None;
        rt.notice = Some(SkinNotice { code: "safe_mode".into(), id: None, error: None });
        drop(rt);
        apply_app_icons(app, None);
        return;
    }
    if let Some((id, kind)) = plan.fallback {
        let _ = save_active(app, state, BUILTIN_ID);
        let mut rt = lock(state);
        rt.current = None;
        rt.notice = Some(SkinNotice { code: "broken_fallback".into(), id: Some(id), error: Some(kind) });
        return;
    }
    if let Some(inst) = plan.installed {
        tracing::info!("skin at start: {}", inst.manifest.id);
        lock(state).current = Some(inst.resolve());
        apply_app_icons(app, Some(&inst));
    }
}

/// 今の状態（画面の起動のとき。知らせは一度だけ渡す）。
/// 画面はウィンドウを作った直後から読み込みを始めるので、起動のときの確かめ（startup）より先に聞いてくることがある。
/// そのときは済むまで待つ（最長 5 秒。待っても済まなければ、その時点の状態を返す）
pub fn state(app: &AppHandle) -> SkinState {
    let state = app.state::<AppState>();
    for _ in 0..250 {
        if lock(&state).started {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    snapshot(&state, true)
}

/// スキンを当てて保存する（セーフモードは抜ける）。読めないスキンなら何も変えずにエラー
pub fn apply(app: &AppHandle, id: &str) -> AppResult<SkinState> {
    let state = app.state::<AppState>();
    if id == BUILTIN_ID {
        save_active(app, &state, BUILTIN_ID)?;
        {
            let mut rt = lock(&state);
            rt.safe_mode = false;
            rt.current = None;
        }
        apply_app_icons(app, None);
    } else {
        let inst = package::load(&dirs(&state), id, &app_version(app))?;
        save_active(app, &state, id)?;
        {
            let mut rt = lock(&state);
            rt.safe_mode = false;
            rt.current = Some(inst.resolve());
        }
        apply_app_icons(app, Some(&inst));
    }
    tracing::info!("skin applied: {id}");
    emit(app, &state);
    Ok(snapshot(&state, false))
}

/// 既定の見た目に戻す（設定画面・トレイのメニュー）。保存しているスキンも既定にする
pub fn reset(app: &AppHandle) -> AppResult<SkinState> {
    apply(app, BUILTIN_ID)
}

/// 2つ目の起動に `--safe-mode` が付いていたとき：前からあるアプリを、この起動の間だけ既定の見た目にする
pub fn enter_safe_mode(app: &AppHandle) {
    tracing::info!("skin: safe mode (second instance)");
    let state = app.state::<AppState>();
    {
        let mut rt = lock(&state);
        rt.safe_mode = true;
        rt.current = None;
        rt.notice = Some(SkinNotice { code: "safe_mode_second".into(), id: None, error: None });
    }
    apply_app_icons(app, None);
    emit(app, &state);
}

/// セーフモードを抜けて、保存しているスキンに戻す（読めなければ既定に戻して知らせる）
pub fn exit_safe_mode(app: &AppHandle) -> AppResult<SkinState> {
    let state = app.state::<AppState>();
    let id = state.settings().skin.active_skin_id;
    match apply(app, &id) {
        Ok(s) => Ok(s),
        Err(e) => {
            reset(app)?;
            Err(e)
        }
    }
}

pub fn list(app: &AppHandle) -> Vec<SkinSummary> {
    let state = app.state::<AppState>();
    package::list(&dirs(&state), &app_version(app))
}

/// zip を確かめて一時の場所に置く。前の確認待ちは捨てる
pub fn inspect(app: &AppHandle, zip_path: &std::path::Path) -> AppResult<Inspection> {
    let state = app.state::<AppState>();
    inspect_one_at_a_time(&state.skin, || package::inspect_zip(zip_path, &dirs(&state), &app_version(app)))
}

/// 取り込みの確かめは1本ずつ（画面の「zip を取り込む…」が続けて押されても、2つ目は1つ目が済むまで待つ）
static INSPECTING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 前の確認待ちを捨て、確かめ、新しい確認待ちを置く。確認待ちは常に1つで、捨てたものの一時の場所は消す
/// （AppHandle を使わない部分。テストで2本同時に走らせて確かめる）
pub(crate) fn inspect_one_at_a_time(
    rt: &std::sync::Mutex<Runtime>,
    run: impl FnOnce() -> AppResult<(Inspection, Staged)>,
) -> AppResult<Inspection> {
    let _one = INSPECTING.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(old) = lock_rt(rt).pending.take() {
        package::discard(&old);
    }
    let (insp, staged) = run()?;
    if let Some(old) = lock_rt(rt).pending.replace(staged) {
        // 1本ずつなのでここには来ないが、来ても一時の場所を残さない
        package::discard(&old);
    }
    Ok(insp)
}

/// 確認が済んだスキンを入れる（適用はしない）
pub fn commit(app: &AppHandle, token: &str) -> AppResult<SkinSummary> {
    let state = app.state::<AppState>();
    let staged = {
        let mut rt = lock(&state);
        match rt.pending.take() {
            Some(s) if s.token == token => s,
            other => {
                rt.pending = other;
                return Err(skin_error(ErrorKind::SkinImportExpired, from_skin(token)));
            }
        }
    };
    let r = package::commit(&staged, &dirs(&state), &app_version(app));
    package::discard(&staged);
    r
}

pub fn cancel(app: &AppHandle, token: &str) {
    let state = app.state::<AppState>();
    let mut rt = lock(&state);
    if rt.pending.as_ref().map(|s| s.token == token).unwrap_or(false) {
        if let Some(s) = rt.pending.take() {
            package::discard(&s);
        }
    }
}

/// 入っているスキンを消す。使っているスキンなら、先に既定の見た目に戻す
pub fn delete(app: &AppHandle, id: &str) -> AppResult<SkinState> {
    let state = app.state::<AppState>();
    if id == BUILTIN_ID {
        return Err(skin_error(ErrorKind::SkinBuiltinReadonly, from_skin(id)));
    }
    let in_use = state.settings().skin.active_skin_id == id || lock(&state).current.as_ref().map(|c| c.id == id).unwrap_or(false);
    if in_use {
        reset(app)?;
    }
    package::delete(&dirs(&state), id)?;
    emit(app, &state);
    Ok(snapshot(&state, false))
}

pub fn export(app: &AppHandle, id: &str, dest: &std::path::Path) -> AppResult<package::ExportReport> {
    let state = app.state::<AppState>();
    package::export(&dirs(&state), id, dest, &app_version(app))
}
