//! 着せ替えの適用・既定に戻す・セーフモード・アプリとトレイのアイコン（v3.2.0）。
//! 画面・トレイ・二重起動の知らせから呼ばれる。ファイルの扱いは package.rs、設定の保存は settings::store。
//! v3.9.0：使う立ち絵（登録した立ち絵・ほかのスキンの立ち絵・出さない）を解いて、画面へ渡す（`refresh_character`）。
//! 立ち絵のファイルの扱いは avatars.rs。
//! v3.11.0：手で入れたアイコン（設定の `skin.icons` と `icons\` の中の画像）を確かめて画面へ渡す（`refresh_icons`）。
//! ウィンドウ・トレイのアイコンは 手で入れたもの → スキン → 組み込み の順（`app_icon_sources`）。ファイルの扱いは icons.rs。
//!
//! 鍵（`Runtime` の Mutex）を持ったまま、Tauri のメインスレッドを待つ呼び出し（ウィンドウ・トレイのアイコン・ダイアログ・
//! 設定の保存と通知）をしない。どの関数も、鍵は値を出し入れする間だけ持つ（v3.8.0〜3.8.1 で2回、この形で固まったため）

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::manifest::{from_skin, skin_error};
use super::names::BUILTIN_ID;
use super::avatars::{self, AvatarList, AvatarSummary, PickedImage, Slot};
use super::icons;
use super::package::{self, CharacterAssets, Installed, Inspection, ResolvedSkin, SkinSummary, Staged};
use super::{images, Dirs};
use crate::commands::{ev, AppState};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::playback::Notice as PlaybackNotice;
use crate::settings::store::AppSettings;

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
    /// v3.9.0：いま画面に出す立ち絵（使う立ち絵を解いたもの。出さないときは None）
    pub character: Option<CharacterAssets>,
    /// v3.9.0：立ち絵を作るために選んで確かめた画像（再生中・停止中の2枠。作る・取り消す・選び直すまで）
    pub picks: [Option<avatars::Picked>; 2],
    /// v3.9.0：選んだ画像の世代。取り消す（`clear_picks`）たびに1つ進める。ファイルを選ぶ前に控えた世代と
    /// 違えば、あとから届いた確かめの結果は置かずに捨てる（取り消したあとに画像がメモリに残らないように）
    pub pick_gen: u64,
    /// v3.11.0：いま画面に当てる手のアイコン（設定の `skin.icons` を確かめたもの。セーフモードの間は空）
    pub manual_icons: icons::Resolved,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkinNotice {
    /// `broken_fallback`（前に選んでいたスキンが壊れていたので既定に戻した）・`safe_mode`（既定の見た目で起動した）・
    /// `safe_mode_second`（2つ目の起動の --safe-mode で既定の見た目にした）・`app_icon_failed`・
    /// `avatar_fallback`（v3.9.0：使う立ち絵に選んでいた立ち絵・スキンが無い・読めないので「スキンの立ち絵」に戻した）・
    /// `icon_fallback`（v3.11.0：手で入れたアイコンの画像が無い・読めないので、その枠を外した。外した数は `count`）
    pub code: String,
    pub id: Option<String>,
    pub error: Option<ErrorKind>,
    /// v3.11.0：`icon_fallback` で外した枠の数（ほかの知らせでは None）
    pub count: Option<u32>,
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
    /// v3.9.0：使う立ち絵（設定の `skin.avatar`）
    pub avatar: String,
    /// v3.9.0：いま画面に出す立ち絵（使う立ち絵を解いたもの。スキンの立ち絵を選んでいれば `skin.character` と同じ）。
    /// 画面はスキンの `character` ではなく、こちらを出す
    pub character: Option<CharacterAssets>,
    /// v3.11.0：手で入れたアイコン（名前 → 状態 → 画像。Rust が `icons\` の中で確かめたものだけ。セーフモードの間は空）。
    /// 画面は 手のアイコン → スキンのアイコン → 今の SVG の順に選ぶ（src\skins\icons.ts の `pickLayeredIcon`）
    pub manual_icons: icons::Resolved,
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
    let sk = state.settings().skin;
    let mut rt = lock(state);
    let notice = if take_notice { rt.notice.take() } else { None };
    SkinState {
        active_id: sk.active_skin_id,
        effective_id: rt.current.as_ref().map(|c| c.id.clone()).unwrap_or_else(|| BUILTIN_ID.into()),
        safe_mode: rt.safe_mode,
        skin: rt.current.clone(),
        notice,
        avatar: sk.avatar,
        character: rt.character.clone(),
        manual_icons: rt.manual_icons.clone(),
    }
}

fn emit(app: &AppHandle, state: &AppState) {
    let _ = app.emit(ev::SKIN, snapshot(state, true));
}

/// 保存しているスキンの id を変える（画面から来た設定ではここは変わらない。commands::settings::settings_set）
fn save_active(app: &AppHandle, state: &AppState, id: &str) -> AppResult<()> {
    update_settings(app, state, |s| (s.skin.active_skin_id != id).then(|| with_active(s.clone(), id)))
}

/// 設定を「読む → 直す → 書く」の一続きで変える（v3.10.0。commands::settings::update_and_emit）。`change` が None なら書かない。
/// `change` の中では Runtime の鍵を取らない（鍵の順番を一方向に保つ）
fn update_settings(app: &AppHandle, state: &AppState, change: impl FnOnce(&AppSettings) -> Option<AppSettings>) -> AppResult<()> {
    crate::commands::settings::update_and_emit(app, state, |s| Ok(change(s))).map(|_| ())
}

/// 使うスキンだけを変えた設定（v3.10.0：手で選んだ色と立ち絵はそのまま。スキンを替えても手の色は残る）
pub(crate) fn with_active(mut s: AppSettings, id: &str) -> AppSettings {
    s.skin.active_skin_id = id.to_string();
    s
}

/// 「既定の見た目に戻す」で変わる設定（v3.10.0）：既定のスキン・手で選んだ色なし。立ち絵の選択と大きさはそのまま。
/// v3.11.0：手で入れたアイコンも外す（画像のファイルは reset_look が消す）
pub(crate) fn reset_look_settings(s: AppSettings) -> AppSettings {
    let mut s = with_active(s, BUILTIN_ID);
    s.skin.colors.clear();
    s.skin.icons.clear();
    s
}

/// v3.9.0：使う立ち絵を変えて保存する（画面から来た設定ではここは変わらない。commands::settings::from_screen）
fn save_avatar(app: &AppHandle, state: &AppState, choice: &str) -> AppResult<()> {
    update_settings(app, state, |s| {
        (s.skin.avatar != choice).then(|| {
            let mut s = s.clone();
            s.skin.set_avatar(choice);
            s
        })
    })
}

/// v3.9.0：使う立ち絵を解いて、画面に出す立ち絵を決め直す（スキンを替えた・セーフモード・使う立ち絵を変えた・消した・設定の読み込み）。
/// 選んでいた立ち絵・スキンが無い・読めなければ「スキンの立ち絵」に戻して保存する。`notify` なら知らせる（起動・設定の読み込み。
/// 使う人が自分で消したときは知らせない）。鍵は値を出し入れする間だけ持つ（保存と通知は鍵の外）
fn refresh_character(app: &AppHandle, state: &AppState, notify: bool) {
    let choice = state.settings().skin.avatar;
    let (current, safe) = {
        let rt = lock(state);
        (rt.current.clone(), rt.safe_mode)
    };
    let d = dirs(state);
    let ver = app_version(app);
    let r = avatars::resolve(
        &choice,
        current.as_ref(),
        safe,
        |id| avatars::load(&d, id).map(|a| a.character()),
        |id| package::load(&d, id, &ver).map(|i| i.resolve().character),
    );
    if let Some(kind) = r.fallback {
        tracing::warn!("avatar choice could not be used, back to the skin's character: {kind:?}");
        let _ = save_avatar(app, state, avatars::CHOICE_SKIN);
        if notify {
            let mut rt = lock(state);
            if rt.notice.is_none() {
                rt.notice = Some(SkinNotice { code: "avatar_fallback".into(), id: None, error: Some(kind), count: None });
            }
        }
    }
    lock(state).character = r.character;
}

/// ウィンドウ・トレイのアイコンにする画像のファイル（パスと、形式を決める名前）
type IconFile = Option<(std::path::PathBuf, String)>;

/// v3.11.0：ウィンドウ・トレイのアイコンにする画像のファイル。ウィンドウは 手で入れたもの → スキン、
/// トレイは 手で入れたもの → スキンのトレイ → （トレイが無ければ）ウィンドウに決まった画像。どれも無ければ None（組み込みのアイコン）。
/// セーフモードの間は手のアイコンを使わない（スキンは呼び出し側が None を渡す）
fn app_icon_sources(state: &AppState, inst: Option<&Installed>) -> (IconFile, IconFile) {
    let safe = lock(state).safe_mode;
    let (manual_window, manual_tray) = if safe {
        (None, None)
    } else {
        let chosen = state.settings().skin.icons;
        let d = dirs(state);
        (icons::app_file(&d, &chosen, icons::APP_WINDOW), icons::app_file(&d, &chosen, icons::APP_TRAY))
    };
    layer_app_icons(manual_window, manual_tray, inst.and_then(|i| i.app_icon_exact(false)), inst.and_then(|i| i.app_icon_exact(true)))
}

/// アプリのアイコンの重ね方（ファイルには触らない。テストで確かめる）：ウィンドウは 手 → スキン、
/// トレイは 手のトレイ → スキンのトレイ → 決まったウィンドウの画像。None は組み込みのアイコン
pub(crate) fn layer_app_icons<T: Clone>(manual_window: Option<T>, manual_tray: Option<T>, skin_window: Option<T>, skin_tray: Option<T>) -> (Option<T>, Option<T>) {
    let window = manual_window.or(skin_window);
    let tray = manual_tray.or(skin_tray).or_else(|| window.clone());
    (window, tray)
}

/// ウィンドウ・タスクバー・トレイのアイコンを、手で入れたもの（v3.11.0）→ スキンのもの → 組み込みのもの の順で決めて当てる。
/// exe・ショートカット・スタートメニューのアイコンは変わらない（ビルドのときに exe に入れたもの）
pub fn apply_app_icons(app: &AppHandle, inst: Option<&Installed>) {
    let state = app.state::<AppState>();
    let (window_src, tray_src) = app_icon_sources(&state, inst);
    let default = app.default_window_icon().cloned();
    let mut failed = false;
    let mut pick = |src: IconFile, size: u32| -> Option<tauri::image::Image<'static>> {
        let (path, rel) = src?;
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
    let window_icon = pick(window_src, icons::WINDOW_ICON_PX).or_else(|| default.clone());
    let tray_icon = pick(tray_src, icons::TRAY_ICON_PX).or_else(|| default.clone());
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
    // v3.9.0：使う立ち絵（無い・読めなければ「スキンの立ち絵」に戻して知らせる）。作る途中で終わった一時の場所を消す
    avatars::clean_staging(&dirs(&state));
    refresh_character(app, &state, true);
    // v3.11.0：手で入れたアイコン（画像が無い・読めない枠は外して知らせる）。書く途中で終わった一時の場所を消す。
    // ウィンドウ・トレイのアイコンを手で入れていれば当て直す（スキンが無いときは startup_inner が当てていないため）
    icons::clean_staging(&dirs(&state));
    refresh_icons(app, &state, true);
    let safe = lock(&state).safe_mode;
    if !safe && has_manual_app_icon(&state) {
        reapply_app_icons(app, &state);
    }
    lock(&state).started = true;
}

fn startup_inner(app: &AppHandle, state: &AppState, safe_arg: bool) {
    let d = dirs(state);
    package::clean_staging(&d);
    let s = state.settings();
    let ver = app_version(app);
    let plan = plan_startup(&s.skin, safe_arg, |id| package::load(&d, id, &ver));
    if plan.clear_flag {
        let _ = update_settings(app, state, |cur| {
            let mut c = cur.clone();
            c.skin.safe_mode_next_start = false;
            Some(c)
        });
    }
    if plan.safe_mode {
        tracing::info!("skin: safe mode ({})", if safe_arg { "--safe-mode" } else { "next start" });
        let mut rt = lock(state);
        rt.safe_mode = true;
        rt.current = None;
        rt.notice = Some(SkinNotice { code: "safe_mode".into(), id: None, error: None, count: None });
        drop(rt);
        apply_app_icons(app, None);
        return;
    }
    if let Some((id, kind)) = plan.fallback {
        let _ = save_active(app, state, BUILTIN_ID);
        let mut rt = lock(state);
        rt.current = None;
        rt.notice = Some(SkinNotice { code: "broken_fallback".into(), id: Some(id), error: Some(kind), count: None });
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
    refresh_character(app, &state, false);
    // v3.11.0：手のアイコンはスキンを替えても残る（セーフモードを抜けたときは、ここで当て直す）
    refresh_icons(app, &state, false);
    emit(app, &state);
    Ok(snapshot(&state, false))
}

/// 既定のスキンに切り替えて保存する（使っているスキンを消すとき・セーフモードを抜けられなかったとき）。
/// 手で選んだ色と立ち絵は残す。使う人の「既定の見た目に戻す」は `reset_look`
pub fn reset(app: &AppHandle) -> AppResult<SkinState> {
    apply(app, BUILTIN_ID)
}

/// 「既定の見た目に戻す」（設定画面・トレイのメニュー）。スキンを既定にし、手で選んだ色も全部外す（v3.10.0）。立ち絵は残す（v3.9.0）。
/// 色を先に外してからスキンを戻す（スキンを戻せなくても、読めなくなった色はもう当たらない）。
/// v3.11.0：手で入れたアイコンも色と一緒に外し、画面とアプリのアイコンが手放してから（スキンを戻したあとで）画像のファイルを消す
pub fn reset_look(app: &AppHandle) -> AppResult<SkinState> {
    let state = app.state::<AppState>();
    {
        let _f = icons::files_lock();
        update_settings(app, &state, |s| {
            let reset = reset_look_settings(s.clone());
            let mut plain = s.clone();
            plain.skin.colors = reset.skin.colors;
            plain.skin.icons = reset.skin.icons;
            (plain != *s).then(|| {
                tracing::info!("manual colors and icons cleared (reset to the default look)");
                plain
            })
        })?;
    }
    let r = reset(app);
    remove_icon_files_if_cleared(&state);
    r
}

/// v3.10.0：手で選んだ色を1つ変える（`None` なら外す）。決まりに合わなければ何も変えずに断る。保存して設定を画面へ知らせる
pub fn set_color(app: &AppHandle, name: &str, value: Option<&str>) -> AppResult<AppSettings> {
    let state = app.state::<AppState>();
    crate::commands::settings::update_and_emit(app, &state, |cur| {
        let mut s = cur.clone();
        super::colors::set(&mut s.skin.colors, name, value)?;
        Ok(Some(s))
    })
    .map(|(_, now)| now)
}

/// v3.10.0：手で選んだ色を全部外す（「手で選んだ色を全部戻す」）。スキンと立ち絵は変えない
pub fn clear_colors(app: &AppHandle) -> AppResult<AppSettings> {
    let state = app.state::<AppState>();
    update_settings(app, &state, |cur| {
        (!cur.skin.colors.is_empty()).then(|| {
            tracing::info!("manual colors cleared");
            let mut s = cur.clone();
            s.skin.colors.clear();
            s
        })
    })?;
    Ok(state.settings())
}

/// 2つ目の起動に `--safe-mode` が付いていたとき：前からあるアプリを、この起動の間だけ既定の見た目にする
pub fn enter_safe_mode(app: &AppHandle) {
    tracing::info!("skin: safe mode (second instance)");
    let state = app.state::<AppState>();
    {
        let mut rt = lock(&state);
        rt.safe_mode = true;
        rt.current = None;
        rt.notice = Some(SkinNotice { code: "safe_mode_second".into(), id: None, error: None, count: None });
    }
    apply_app_icons(app, None);
    refresh_character(app, &state, false);
    refresh_icons(app, &state, false);
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
    // v3.9.0：消すスキンの立ち絵を使う立ち絵に選んでいたら、消す前に「スキンの立ち絵」へ切り替える（画面がその絵を手放してから消す。
    // 使う人が自分で消したので知らせない）。消せなかったら選択を元に戻す
    let before = state.settings().skin.avatar;
    let chosen = avatars::Choice::parse(&before) == Some(avatars::Choice::SkinCharacter(id.to_string()));
    switch_then_delete(
        chosen,
        || switch_avatar(app, &state, avatars::CHOICE_SKIN),
        || package::delete(&dirs(&state), id),
        || switch_avatar(app, &state, &before),
    )?;
    refresh_character(app, &state, false);
    emit(app, &state);
    Ok(snapshot(&state, false))
}

/// v3.9.0：使う立ち絵を変えて、画面の立ち絵を当て直す（消す前の切り替え・消せなかったときの戻し）
fn switch_avatar(app: &AppHandle, state: &AppState, choice: &str) {
    let _ = save_avatar(app, state, choice);
    refresh_character(app, state, false);
    emit(app, state);
}

/// v3.9.0：使っている立ち絵（スキンの立ち絵を含む）を消すときの順番。使っていれば先に切り替え（画面が絵を手放す）、
/// それから消す。消せなかったら切り替えを戻す。AppHandle を使わない部分（テストで順番を確かめる）
pub(crate) fn switch_then_delete(
    in_use: bool,
    switch_away: impl FnOnce(),
    delete: impl FnOnce() -> AppResult<()>,
    restore: impl FnOnce(),
) -> AppResult<()> {
    if in_use {
        switch_away();
    }
    match delete() {
        Ok(()) => Ok(()),
        Err(e) => {
            if in_use {
                restore();
            }
            Err(e)
        }
    }
}

pub fn export(app: &AppHandle, id: &str, dest: &std::path::Path) -> AppResult<package::ExportReport> {
    let state = app.state::<AppState>();
    package::export(&dirs(&state), id, dest, &app_version(app))
}

// ---------------- v3.9.0：登録した立ち絵 ----------------

/// 設定の「立ち絵」の欄の一覧（取り込んだスキンの立ち絵・登録した立ち絵）
pub fn avatars_list(app: &AppHandle) -> AvatarList {
    let state = app.state::<AppState>();
    avatars::list_all(&dirs(&state), &app_version(app))
}

/// 選んだ画像を確かめて、その枠（再生中・停止中）に置く。前にその枠に置いていたものは捨てる。
/// 確かめ（復号）は鍵の外で行い、置くときだけ鍵を持つ
/// `generation` はファイルを選ぶ画面を出す前に `pick_generation` で控えた値。その間に取り消されていたら（小窓の「キャンセル」）、
/// 確かめた画像は置かずに捨て、None を返す
pub fn pick(app: &AppHandle, path: &std::path::Path, slot: Slot, generation: u64) -> AppResult<Option<PickedImage>> {
    let state = app.state::<AppState>();
    let (picked, shown) = avatars::pick_file(path, slot)?;
    Ok(store_pick(&state.skin, generation, picked).then_some(shown))
}

/// 今の選んだ画像の世代（ファイルを選ぶ画面を出す前に控える）
pub fn pick_generation(app: &AppHandle) -> u64 {
    let state = app.state::<AppState>();
    let g = lock(&state).pick_gen;
    g
}

/// 選んだ画像を捨てる（作るのをやめた）。世代を進め、確かめている途中のものも、届いたときに捨てる
pub fn cancel_picks(app: &AppHandle) {
    let state = app.state::<AppState>();
    clear_picks(&state.skin);
}

/// 確かめた画像を枠に置く。控えた世代が今と違えば（その間に取り消された）置かずに false（画像はここで手放す）
pub(crate) fn store_pick(rt: &std::sync::Mutex<Runtime>, generation: u64, picked: avatars::Picked) -> bool {
    let mut r = lock_rt(rt);
    if r.pick_gen != generation {
        tracing::info!("avatar pick arrived after cancel; dropped");
        return false;
    }
    let i = picked.slot.index();
    r.picks[i] = Some(picked);
    true
}

pub(crate) fn clear_picks(rt: &std::sync::Mutex<Runtime>) {
    let mut r = lock_rt(rt);
    r.picks = [None, None];
    r.pick_gen = r.pick_gen.wrapping_add(1);
}

/// 選んだ画像の枠から、印の合うものを取り出す（合わなければ何も取り出さない）
pub(crate) fn take_picks(
    picks: &mut [Option<avatars::Picked>; 2],
    playing: &str,
    stopped: Option<&str>,
) -> AppResult<(avatars::Picked, Option<avatars::Picked>)> {
    let (p, s) = take_some(picks, Some(playing), stopped)?;
    Ok((p.expect("checked in take_some"), s))
}

/// 選んだ画像の枠から、印で指したものだけを取り出す（指さない枠は残す）。どれかの印が合わなければ何も取り出さない
pub(crate) fn take_some(
    picks: &mut [Option<avatars::Picked>; 2],
    playing: Option<&str>,
    stopped: Option<&str>,
) -> AppResult<(Option<avatars::Picked>, Option<avatars::Picked>)> {
    let ok = |i: usize, t: Option<&str>| match t {
        None => true,
        Some(t) => picks[i].as_ref().map(|p| p.token == t).unwrap_or(false),
    };
    if !ok(0, playing) || !ok(1, stopped) {
        return Err(skin_error(ErrorKind::AvatarPickExpired, "picked image"));
    }
    let p = if playing.is_some() { picks[0].take() } else { None };
    let s = if stopped.is_some() { picks[1].take() } else { None };
    Ok((p, s))
}

/// 枠に戻す（作る・変えるのを断ったとき。名前を直して、選び直さずにやり直せる）。その間に選び直された枠は上書きしない
fn put_back(state: &AppState, p: Option<avatars::Picked>, s: Option<avatars::Picked>) {
    let mut rt = lock(state);
    for (i, x) in [(0, p), (1, s)] {
        if let Some(x) = x {
            if rt.picks[i].is_none() {
                rt.picks[i] = Some(x);
            }
        }
    }
}

/// 立ち絵を作る（使う立ち絵は変えない）。画像は選んだときの印で指す。断ったときは、選んだ画像を枠に戻す（名前を直して作り直せる）
pub fn create(app: &AppHandle, name: &str, playing: &str, stopped: Option<&str>) -> AppResult<AvatarSummary> {
    let state = app.state::<AppState>();
    let taken = take_picks(&mut lock(&state).picks, playing, stopped);
    let (p, s) = taken?;
    match avatars::create(&dirs(&state), name, &p, s.as_ref()) {
        Ok(sum) => {
            tracing::info!("avatar created: {}", sum.id);
            // 使わなかった停止中の枠（「なし」で作った）も捨てる
            lock(&state).picks = [None, None];
            Ok(sum)
        }
        Err(e) => {
            put_back(&state, Some(p), s);
            Err(e)
        }
    }
}

/// 作った立ち絵を変える（v3.9.0）。`playing`・`stopped` は差し替える画像の印（avatars_pick_image が返したもの。無ければそのまま）。
/// `remove_stopped` なら停止中を外す（`stopped` と同時には指せない）。名前が空なら今の名前のまま。
/// 使っている立ち絵なら、画面の絵を当て直す（差し替えた絵はファイル名が変わるので、動く画像も読み直される）
pub fn update(app: &AppHandle, id: &str, name: &str, playing: Option<&str>, stopped: Option<&str>, remove_stopped: bool) -> AppResult<AvatarSummary> {
    let state = app.state::<AppState>();
    if remove_stopped && stopped.is_some() {
        return Err(skin_error(ErrorKind::Other, "stopped image: replace or remove, not both"));
    }
    let taken = take_some(&mut lock(&state).picks, playing, stopped);
    let (p, s) = taken?;
    let change = match (&s, remove_stopped) {
        (Some(x), _) => avatars::StoppedChange::Replace(x),
        (None, true) => avatars::StoppedChange::Remove,
        (None, false) => avatars::StoppedChange::Keep,
    };
    // 使っている立ち絵なら、avatar.json を置き換えた直後（古い絵を消す前）に解き直して通知する（画面が新しい絵へ移ってから古い絵を消す）
    let in_use = avatars::Choice::parse(&state.settings().skin.avatar) == Some(avatars::Choice::Avatar(id.to_string()));
    let after_commit = || {
        if in_use {
            refresh_character(app, &state, false);
            emit(app, &state);
        }
    };
    match avatars::update_then(&dirs(&state), id, name, p.as_ref(), change, after_commit) {
        Ok(sum) => {
            tracing::info!("avatar updated: {id}");
            lock(&state).picks = [None, None];
            Ok(sum)
        }
        Err(e) => {
            put_back(&state, p, s);
            Err(e)
        }
    }
}

/// 立ち絵を消す。使う立ち絵に選んでいたら、「スキンの立ち絵」に戻す
pub fn delete_avatar(app: &AppHandle, id: &str) -> AppResult<SkinState> {
    let state = app.state::<AppState>();
    // 使っている立ち絵なら、消す前に「スキンの立ち絵」へ切り替える（画面がその絵を手放してから消す）。消せなかったら選択を元に戻す
    let before = state.settings().skin.avatar;
    let in_use = avatars::Choice::parse(&before) == Some(avatars::Choice::Avatar(id.to_string()));
    switch_then_delete(
        in_use,
        || switch_avatar(app, &state, avatars::CHOICE_SKIN),
        || avatars::delete(&dirs(&state), id),
        || switch_avatar(app, &state, &before),
    )?;
    tracing::info!("avatar deleted: {id}");
    refresh_character(app, &state, false);
    emit(app, &state);
    Ok(snapshot(&state, false))
}

/// v3.9.0：立ち絵を出さない（設定の「立ち絵を出さない」・タスクトレイの「立ち絵を出さない」）。「使う」で「出さない」を選んだのと同じ。
/// 作った立ち絵は消さない。画面（WebView）を通らず Rust だけで済む（立ち絵の画像で画面が崩れたときの逃げ道）。
/// タスクトレイからはメインのスレッドで呼ばれるので、呼ぶ側（platform::windows::tray）が別のスレッドへ渡す
pub fn hide_avatar(app: &AppHandle) -> AppResult<SkinState> {
    use_avatar(app, avatars::CHOICE_NONE)
}

/// 使う立ち絵を選ぶ（`skin`・`none`・`avatar:<id>`・`skin:<id>`）。選べないもの（無い・読めない・立ち絵の無いスキン）なら何も変えずにエラー
pub fn use_avatar(app: &AppHandle, choice: &str) -> AppResult<SkinState> {
    let state = app.state::<AppState>();
    let value = avatars::check_choice(&dirs(&state), choice, &app_version(app))?;
    save_avatar(app, &state, &value)?;
    tracing::info!("avatar choice: {value}");
    refresh_character(app, &state, false);
    emit(app, &state);
    Ok(snapshot(&state, false))
}

/// 設定の読み込み（zip）のあと：zip の「使う立ち絵」がこの PC に無ければ「スキンの立ち絵」に戻して知らせ、画面の立ち絵を当て直す
pub fn after_settings_import(app: &AppHandle) {
    let state = app.state::<AppState>();
    refresh_character(app, &state, true);
    // v3.11.0：zip の値で使われなくなった枠の画像を先に消す（残しておくと、あとで同じ枠を指す zip を読んだとき、
    // 使った覚えのない古い画像が出るため）。消すのは読み込んだ設定に無い枠だけ（読み直しで外れた枠は、下の refresh_icons が外す）
    {
        let _f = icons::files_lock();
        icons::remove_unused(&dirs(&state), &state.settings().skin.icons);
    }
    // zip の手のアイコンのうち、この PC に画像の無い枠を外して知らせる。ウィンドウ・トレイのアイコンも当て直す
    refresh_icons(app, &state, true);
    reapply_app_icons(app, &state);
    emit(app, &state);
}

// ---------------- v3.11.0：手で入れるアイコン ----------------

/// 設定の手のアイコンを確かめて、画面に当てるものを決め直す（入れた・戻した・スキンを替えた・セーフモード・起動・設定の読み込み）。
/// 画像が無い・読めない枠は設定から外して保存する。`notify` なら知らせる（起動・設定の読み込み。使う人が自分で戻したときは知らせない）。
/// セーフモードの間は当てない（設定は変えない）。ファイルの確かめと設定の書き込みは files_lock の中で一続きにする
fn refresh_icons(app: &AppHandle, state: &AppState, notify: bool) {
    let safe = lock(state).safe_mode;
    if safe {
        lock(state).manual_icons = icons::Resolved::new();
        return;
    }
    let _f = icons::files_lock();
    let chosen = state.settings().skin.icons;
    // 読めなかった枠は、少し待ってもう1回読んでから外す（一時的に掴まれているだけの画像で外さない）
    let (resolved, missing) = icons::resolve_retrying(&dirs(state), &chosen, ICON_RETRY_WAIT);
    if !missing.is_empty() {
        let n = missing.len();
        tracing::warn!("{n} manual icon(s) could not be used and were removed from the settings");
        let _ = update_settings(app, state, |cur| {
            let mut t = cur.clone();
            // 読んだあとで入れ直された枠（形式が変わった）は外さない
            for (name, st, ext) in &missing {
                if icons::get(&t.skin.icons, name, st) == Some(ext.as_str()) {
                    icons::set(&mut t.skin.icons, name, st, None);
                }
            }
            (t != *cur).then_some(t)
        });
        if notify {
            let mut rt = lock(state);
            if rt.notice.is_none() {
                rt.notice = Some(SkinNotice { code: "icon_fallback".into(), id: None, error: Some(ErrorKind::IconBroken), count: Some(n as u32) });
            }
        }
    }
    lock(state).manual_icons = resolved;
}

/// 読めなかった手のアイコンを読み直すまで待つ時間（読めない枠があるときだけ待つ）
const ICON_RETRY_WAIT: std::time::Duration = std::time::Duration::from_millis(300);

/// 手でウィンドウかトレイのアイコンを入れているか
fn has_manual_app_icon(state: &AppState) -> bool {
    let chosen = state.settings().skin.icons;
    chosen.contains_key(icons::APP_WINDOW) || chosen.contains_key(icons::APP_TRAY)
}

/// いま当てているスキン（既定・セーフモードなら None。読めなければ None）
fn current_installed(app: &AppHandle, state: &AppState) -> Option<Installed> {
    let id = lock(state).current.as_ref().map(|c| c.id.clone())?;
    package::load(&dirs(state), &id, &app_version(app)).ok()
}

/// ウィンドウ・トレイのアイコンを、今のスキンと手のアイコンで当て直す（鍵は持たずに呼ぶ）
fn reapply_app_icons(app: &AppHandle, state: &AppState) {
    let inst = current_installed(app, state);
    apply_app_icons(app, inst.as_ref());
}

/// 手のアイコンが1つも残っていなければ、`icons\` の画像を全部消す（全部戻す・既定の見た目に戻すのあと。その間に入れ直されていれば消さない）
fn remove_icon_files_if_cleared(state: &AppState) {
    let _f = icons::files_lock();
    if state.settings().skin.icons.is_empty() {
        icons::remove_all(&dirs(state));
    }
}

fn icon_slot(name: &str, st: &str) -> AppResult<(&'static str, &'static str)> {
    icons::slot(name, st).ok_or_else(|| icons::for_screen(skin_error(ErrorKind::IconSlotInvalid, format!("{}/{}", from_skin(name), from_skin(st)))))
}

/// セーフモードの間は手のアイコンを入れない（枠を押したとき、ファイルを選ぶ画面を出す前にも確かめる）
pub fn refuse_icons_in_safe_mode(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let safe = lock(&state).safe_mode;
    if safe {
        tracing::warn!("manual icon refused: safe mode");
        return Err(AppError::new(ErrorKind::IconSafeMode, String::new()));
    }
    Ok(())
}

/// 画像のファイル（枠に落とした・選んだもの）を、その枠の手のアイコンにする。確かめ（復号）は鍵の外で行い、
/// ファイルを置くのと設定に書くのは files_lock の中で一続きにする。セーフモードの間は断る
pub fn set_icon_file(app: &AppHandle, name: &str, st: &str, path: &std::path::Path) -> AppResult<SkinState> {
    let state = app.state::<AppState>();
    let (n, s) = icon_slot(name, st)?;
    refuse_icons_in_safe_mode(app)?;
    // 断る文の詳しいところ（英語・アプリが決めた置き場所の名前）は画面へ返さない（icons::for_screen）
    let (data, file_name) = icons::read_file(path).map_err(icons::for_screen)?;
    let prepared = icons::prepare(n, s, data, &file_name).map_err(icons::for_screen)?;
    {
        let _f = icons::files_lock();
        icons::write(&dirs(&state), &prepared).map_err(icons::for_screen)?;
        update_settings(app, &state, |cur| {
            (icons::get(&cur.skin.icons, n, s) != Some(prepared.ext)).then(|| {
                let mut t = cur.clone();
                icons::set(&mut t.skin.icons, n, s, Some(prepared.ext));
                t
            })
        })?;
    }
    tracing::info!("manual icon set: {n}/{s}");
    refresh_icons(app, &state, false);
    if icons::is_app(n) {
        reapply_app_icons(app, &state);
    }
    emit(app, &state);
    Ok(snapshot(&state, false))
}

/// 1つの枠の手のアイコンを外す（スキンのアイコンか今のアイコンに戻る）。画面が手放してから画像のファイルを消す
pub fn reset_icon(app: &AppHandle, name: &str, st: &str) -> AppResult<SkinState> {
    let state = app.state::<AppState>();
    let (n, s) = icon_slot(name, st)?;
    {
        let _f = icons::files_lock();
        update_settings(app, &state, |cur| {
            icons::get(&cur.skin.icons, n, s).is_some().then(|| {
                let mut t = cur.clone();
                icons::set(&mut t.skin.icons, n, s, None);
                t
            })
        })?;
    }
    tracing::info!("manual icon reset: {n}/{s}");
    refresh_icons(app, &state, false);
    if icons::is_app(n) {
        reapply_app_icons(app, &state);
    }
    emit(app, &state);
    {
        let _f = icons::files_lock();
        if icons::get(&state.settings().skin.icons, n, s).is_none() {
            icons::remove(&dirs(&state), n, s);
        }
    }
    Ok(snapshot(&state, false))
}

/// 手で入れたアイコンを全部外す（「手で入れたアイコンを全部戻す」）。スキン・手の色・立ち絵は変えない。画像のファイルも消す
pub fn clear_icons(app: &AppHandle) -> AppResult<SkinState> {
    let state = app.state::<AppState>();
    {
        let _f = icons::files_lock();
        update_settings(app, &state, |cur| {
            (!cur.skin.icons.is_empty()).then(|| {
                let mut t = cur.clone();
                t.skin.icons.clear();
                t
            })
        })?;
    }
    tracing::info!("manual icons cleared");
    refresh_icons(app, &state, false);
    reapply_app_icons(app, &state);
    emit(app, &state);
    remove_icon_files_if_cleared(&state);
    Ok(snapshot(&state, false))
}

/// 設定の「アイコン」の欄に出す、今のウィンドウ・トレイのアイコンの見本（data: URL の PNG）
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppIconPreviews {
    pub window: Option<String>,
    pub tray: Option<String>,
}

/// 今のウィンドウ・トレイのアイコン（手で入れたもの → スキン → 組み込み。当てるときと同じ決め方）を見本にする
pub fn app_icon_previews(app: &AppHandle) -> AppIconPreviews {
    let state = app.state::<AppState>();
    let inst = current_installed(app, &state);
    let (window_src, tray_src) = app_icon_sources(&state, inst.as_ref());
    let default = app.default_window_icon().cloned();
    let one = |src: IconFile, size: u32| -> Option<String> {
        let from_file = src.and_then(|(path, rel)| {
            let data = std::fs::read(&path).ok()?;
            let (rgba, w, h) = images::app_icon_rgba(&data, &rel, size).ok()?;
            images::rgba_data_url(rgba, w, h, size)
        });
        from_file.or_else(|| {
            let d = default.as_ref()?;
            images::rgba_data_url(d.rgba().to_vec(), d.width(), d.height(), size)
        })
    };
    // 画面では縦横 32px・16px で出す。高い倍率の画面でもにじまないように2倍で作る
    AppIconPreviews { window: one(window_src, 64), tray: one(tray_src, 32) }
}
