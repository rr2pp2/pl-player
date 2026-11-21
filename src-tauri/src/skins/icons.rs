//! 手で入れるアイコン（v3.11.0）。設定の「着せ替え」の「アイコン」の欄で、枠に画像を落とす（または押して選ぶ）と、そのアイコンが替わる。
//!
//! - 枠は、スキンで差し替えられるボタンのアイコン（`manifest::ICONS` の 34 個）と、ウィンドウ（`AppWindow`）・トレイ（`AppTray`）のアイコン。
//!   状態のある4つ（星・音量・リピート・シャッフル）は `default` を除いた状態ごと、ほかは `default` だけ（`slot_states`）
//! - 重なり順は 今の SVG（既定）< スキンのアイコン < 手で入れたアイコン（当てるのは画面。src\skins\icons.ts）。Dark・Light 共通の1組。
//!   アプリ・トレイのアイコンは Rust が当てる（skins::runtime::apply_app_icons）
//! - 画像の確かめはスキンのアイコンと同じ `images::check`（形式・大きさ・コマ数・画素・長さの上限も同じ。新しく緩めない）。
//!   アプリ・トレイのアイコンは静止画だけ（スキンの `application` と同じ）
//! - 置き場所はデータのフォルダの `icons\<名前>\<状態>.<形式>`（動く画像の最初の1コマは `icons\<名前>\derived\<状態>.png`）。
//!   入れるときに画像を写して置き、元のファイルは使い続けない。書く途中は `icons-staging\` に書き、全部書けてから名前を変えて置く
//! - どの枠に何を入れたか（形式）は設定の `skin.icons`（名前 → 状態 → 形式）。画面から来た設定の保存（`settings_set`）では変わらない
//! - ファイルを置く・消すのと、設定の `skin.icons` を書くのは、`files_lock` の中で一続きにする（同じ枠を2つ同時に入れても、
//!   ファイルと設定が食い違わない）。鍵の順番は いつも files_lock → 設定を書く鍵（settings_write）で、逆に取る所は無い
//! - ドロップ：画面は落とした位置だけを知らせ、パスは渡さない。OS から落としたパスは Rust がウィンドウの知らせ（`record_drop`）で
//!   控えておき、画面が知らせた位置と同じ落とし方のものだけを使う（`take_drop`。画面から Rust にパスを渡す窓口は作らない）

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::images;
use super::manifest::{guard, skin_error, ICONS};
use super::names::IMAGE_EXTS;
use super::package;
use super::Dirs;
use crate::error::{AppError, AppResult, ErrorKind};

/// ウィンドウ・タスクバーのアイコンの枠の名前
pub const APP_WINDOW: &str = "AppWindow";
/// タスクトレイのアイコンの枠の名前
pub const APP_TRAY: &str = "AppTray";
pub const DEFAULT_STATE: &str = "default";
/// アプリのアイコンにする大きさ（スキンの `application` と同じ。docs\着せ替え_仕様.md 7章）
pub const WINDOW_ICON_PX: u32 = 256;
pub const TRAY_ICON_PX: u32 = 64;
/// 落とした記録を使える時間（落とした直後に画面が知らせてくる。古い記録は使わない）
pub const DROP_MAX_AGE: Duration = Duration::from_secs(30);
/// 動く画像の最初の1コマを置くフォルダ（`icons\<名前>\derived\<状態>.png`）
const STILL_DIR: &str = "derived";

/// 手で入れたアイコン（名前 → 状態 → 形式。形式は png・webp・gif）
pub type Icons = BTreeMap<String, BTreeMap<String, String>>;

/// 画面へ渡す手のアイコン（Rust が `icons\` の中で確かめたファイルだけ）
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManualIcon {
    pub path: String,
    /// 動く画像の最初の1コマ（「動きを減らす」のときに使う）。静止画は None
    pub still: Option<String>,
    /// 入れ直すたびに変わる印（ファイルの名前は同じなので、画面は URL にこれを付けて読み直す）
    pub rev: String,
}

/// 名前 → 状態 → 画面へ渡す手のアイコン
pub type Resolved = BTreeMap<String, BTreeMap<String, ManualIcon>>;

/// アプリ・トレイのアイコンの枠か
pub fn is_app(name: &str) -> bool {
    name == APP_WINDOW || name == APP_TRAY
}

/// その名前の枠の状態（状態のあるアイコンは `default` を除いた状態ごと、ほかは `default`）。知らない名前は None
pub fn slot_states(name: &str) -> Option<Vec<&'static str>> {
    if is_app(name) {
        return Some(vec![DEFAULT_STATE]);
    }
    let (_, states) = ICONS.iter().find(|(n, _)| *n == name)?;
    let named: Vec<&'static str> = states.iter().copied().filter(|s| *s != DEFAULT_STATE).collect();
    Some(if named.is_empty() { vec![DEFAULT_STATE] } else { named })
}

/// 枠を確かめ、アプリが持つ名前に置き換える（パスを作るときは、画面から来た文字ではなくこちらを使う）
pub fn slot(name: &str, state: &str) -> Option<(&'static str, &'static str)> {
    let n: &'static str = match name {
        APP_WINDOW => APP_WINDOW,
        APP_TRAY => APP_TRAY,
        _ => ICONS.iter().find(|(n, _)| *n == name)?.0,
    };
    let s = slot_states(n)?.into_iter().find(|s| *s == state)?;
    Some((n, s))
}

/// 全部の枠（ボタンのアイコンは ICONS の並び、そのあとウィンドウ・トレイ）
pub fn all_slots() -> Vec<(&'static str, &'static str)> {
    let mut out = vec![];
    for (n, _) in ICONS.iter() {
        for s in slot_states(n).unwrap_or_default() {
            out.push((*n, s));
        }
    }
    out.push((APP_WINDOW, DEFAULT_STATE));
    out.push((APP_TRAY, DEFAULT_STATE));
    out
}

fn ext_ok(ext: &str) -> Option<&'static str> {
    let e = ext.to_ascii_lowercase();
    IMAGE_EXTS.iter().copied().find(|x| *x == e)
}

/// ファイル名の拡張子から形式（png・webp・gif だけ。大文字小文字は区別しない）
pub fn ext_of(file_name: &str) -> Option<&'static str> {
    ext_ok(Path::new(file_name).extension()?.to_str()?)
}

/// 設定の値を整える（読み込み・設定の保存・zip の読み込みのとき）：知らない枠・知らない形式を捨てる
pub fn normalize(i: &Icons) -> Icons {
    let mut out = Icons::new();
    for (n, states) in i {
        for (s, ext) in states {
            if let (Some((n2, s2)), Some(e)) = (slot(n, s), ext_ok(ext)) {
                out.entry(n2.to_string()).or_default().insert(s2.to_string(), e.to_string());
            }
        }
    }
    out
}

/// JSON の値から読む。オブジェクトでなければ空、形の違う値は捨てる（形の違う1つの値で、設定全体を既定に戻さないため）。
/// 枠と形式の確かめは `normalize`（`AppSettings::sanitize` が呼ぶ）
pub fn from_json(v: &serde_json::Value) -> Icons {
    let mut out = Icons::new();
    if let Some(o) = v.as_object() {
        for (n, sv) in o {
            if let Some(so) = sv.as_object() {
                let m: BTreeMap<String, String> = so.iter().filter_map(|(s, e)| e.as_str().map(|e| (s.clone(), e.to_string()))).collect();
                if !m.is_empty() {
                    out.insert(n.clone(), m);
                }
            }
        }
    }
    out
}

/// 入れた数
pub fn count(i: &Icons) -> usize {
    i.values().map(|m| m.len()).sum()
}

/// その枠に入れた形式
pub fn get<'a>(i: &'a Icons, name: &str, state: &str) -> Option<&'a str> {
    i.get(name).and_then(|m| m.get(state)).map(|s| s.as_str())
}

/// 1つの枠を変える（`None` なら外す）。空になった名前は消す
pub fn set(i: &mut Icons, name: &str, state: &str, ext: Option<&str>) {
    match ext {
        Some(e) => {
            i.entry(name.to_string()).or_default().insert(state.to_string(), e.to_string());
        }
        None => {
            if let Some(m) = i.get_mut(name) {
                m.remove(state);
                if m.is_empty() {
                    i.remove(name);
                }
            }
        }
    }
}

fn rel(name: &str, state: &str, ext: &str) -> String {
    format!("{name}/{state}.{ext}")
}

fn still_rel(name: &str, state: &str) -> String {
    format!("{name}/{STILL_DIR}/{state}.png")
}

fn broken(why: impl std::fmt::Display) -> AppError {
    skin_error(ErrorKind::IconBroken, why.to_string())
}

fn reparse_at(p: &Path) -> bool {
    fs::symlink_metadata(p).map(|m| package::is_reparse(&m)).unwrap_or(false)
}

/// 頭のバイトが拡張子の形式と合うか（`MZ` でないか）。大きさの上限も見る（復号は入れるときに済ませてある）
fn file_looks_right(p: &Path, rel_name: &str) -> AppResult<fs::Metadata> {
    let meta = fs::metadata(p).map_err(|e| broken(format!("{rel_name}: {e}")))?;
    if meta.len() > images::MAX_IMAGE_BYTES {
        return Err(broken(format!("{rel_name} is too large")));
    }
    let mut head = [0u8; 32];
    let n = fs::File::open(p).and_then(|mut h| h.read(&mut head)).map_err(|e| broken(format!("{rel_name}: {e}")))?;
    if !images::signature_matches(rel_name, &head[..n]) {
        return Err(broken(format!("{rel_name}: content does not match the extension")));
    }
    Ok(meta)
}

/// 入れたアイコンを1つ確かめて、画面へ渡す形にする。フォルダとファイルは途中の段も含めてリンクを辿らない
fn stored(dirs: &Dirs, name: &str, state: &str, ext: &str) -> AppResult<ManualIcon> {
    if reparse_at(&dirs.icons) {
        return Err(broken("icons folder is a link"));
    }
    let r = rel(name, state, ext);
    let f = package::safe_file(&dirs.icons, &r).map_err(|e| broken(e.detail))?;
    let meta = file_looks_right(&f, &r)?;
    let sr = still_rel(name, state);
    let still = package::safe_file(&dirs.icons, &sr).ok().filter(|p| file_looks_right(p, &sr).is_ok());
    let nanos = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
    Ok(ManualIcon {
        path: f.to_string_lossy().into_owned(),
        still: still.map(|p| p.to_string_lossy().into_owned()),
        rev: format!("{:x}-{:x}", nanos, meta.len()),
    })
}

/// 設定の手のアイコンを全部確かめる。読めたもの（画面へ渡す形）と、読めなかった枠（名前・状態・形式）を返す。
/// 読めなかった枠は、呼び出し側が設定から外す（消えた・壊れた・リンク）
pub fn resolve(dirs: &Dirs, icons: &Icons) -> (Resolved, Vec<(String, String, String)>) {
    let mut ok = Resolved::new();
    let mut missing = vec![];
    for (n, states) in normalize(icons) {
        for (s, ext) in states {
            match stored(dirs, &n, &s, &ext) {
                Ok(m) => {
                    ok.entry(n.clone()).or_default().insert(s.clone(), m);
                }
                Err(e) => {
                    tracing::warn!("manual icon {n}/{s} could not be used: {:?}", e.kind);
                    missing.push((n.clone(), s.clone(), ext.clone()));
                }
            }
        }
    }
    (ok, missing)
}

/// `resolve` と同じ。読めなかった枠があれば `wait` だけ待ってから、その枠だけもう1回読む（起動の瞬間に
/// ほかのアプリ（ウイルス対策ソフトなど）がファイルを掴んでいるなど、一時的に読めないだけの枠を、設定から外さないため）
pub fn resolve_retrying(dirs: &Dirs, icons: &Icons, wait: Duration) -> (Resolved, Vec<(String, String, String)>) {
    let (mut ok, missing) = resolve(dirs, icons);
    if missing.is_empty() {
        return (ok, missing);
    }
    std::thread::sleep(wait);
    let mut again = Icons::new();
    for (n, s, e) in &missing {
        set(&mut again, n, s, Some(e));
    }
    let (ok2, still) = resolve(dirs, &again);
    for (n, m) in ok2 {
        ok.entry(n).or_default().extend(m);
    }
    (ok, still)
}

/// 設定に無い枠の画像を消す（設定の zip を読み込んで枠が置き換わったとき、使われなくなった画像を残さない）。
/// 消すのはアプリが書く名前（`<枠の名前>\<状態>.<形式>`・`<枠の名前>\derived\<状態>.png`）のうち、設定の枠と形式に合わないものだけ。
/// リンクは辿らない（`icons\` がリンクなら何もしない。`<枠の名前>` がリンクならリンクだけを外す）。空になったフォルダも消す
pub fn remove_unused(dirs: &Dirs, icons: &Icons) {
    if reparse_at(&dirs.icons) || !dirs.icons.is_dir() {
        return;
    }
    let keep = normalize(icons);
    let mut names: Vec<&'static str> = all_slots().into_iter().map(|(n, _)| n).collect();
    names.dedup();
    for n in names {
        let dir = dirs.icons.join(n);
        let Ok(m) = fs::symlink_metadata(&dir) else { continue };
        if package::is_reparse(&m) {
            if !keep.contains_key(n) {
                let _ = fs::remove_dir(&dir).or_else(|_| fs::remove_file(&dir));
            }
            continue;
        }
        if !m.is_dir() {
            continue;
        }
        let still_dir = dir.join(STILL_DIR);
        for s in slot_states(n).unwrap_or_default() {
            let chosen = get(&keep, n, s);
            for e in IMAGE_EXTS {
                if chosen != Some(e) {
                    remove_file_no_follow(&dir.join(format!("{s}.{e}")));
                }
            }
            if chosen.is_none() && !reparse_at(&still_dir) {
                remove_file_no_follow(&still_dir.join(format!("{s}.png")));
            }
        }
        let _ = fs::remove_dir(&still_dir);
        let _ = fs::remove_dir(&dir);
    }
}

/// 窓口から画面へ返す前に、断る文の詳しいところ（`detail`）を外す。詳しいところは英語で、アプリが決めた置き場所の名前
/// （`default.png` など）や一時の場所のパスが入り、使う人の役に立たないため。種別（画面の日本語の文）はそのまま。詳しいところはログに残す
pub fn for_screen(e: AppError) -> AppError {
    if !e.detail.is_empty() {
        tracing::warn!("manual icon refused: {:?} {}", e.kind, e.detail);
    }
    AppError::new(e.kind, String::new())
}

/// アプリ・トレイのアイコンにする手の画像のファイル（無い・読めなければ None）。返す名前は形式を決めるためだけのもの（`default.png` など）
pub fn app_file(dirs: &Dirs, icons: &Icons, name: &str) -> Option<(PathBuf, String)> {
    let ext = get(icons, name, DEFAULT_STATE)?;
    let ext = ext_ok(ext)?;
    let m = stored(dirs, name, DEFAULT_STATE, ext).ok()?;
    Some((PathBuf::from(m.path), format!("{DEFAULT_STATE}.{ext}")))
}

/// 入れる前に確かめた画像（ファイルへ書くまでメモリに置く）
pub struct Prepared {
    pub name: &'static str,
    pub state: &'static str,
    pub ext: &'static str,
    data: Vec<u8>,
    /// 動く画像の最初の1コマ（PNG）。静止画とアプリのアイコンは None
    still: Option<Vec<u8>>,
}

impl std::fmt::Debug for Prepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prepared").field("name", &self.name).field("state", &self.state).field("ext", &self.ext).field("bytes", &self.data.len()).finish()
    }
}

/// 画像の確かめ（スキンと共有）の種別のうち、文が zip のことを言うものを読み替える（立ち絵と同じ）
fn as_icon_error(e: AppError) -> AppError {
    match e.kind {
        ErrorKind::SkinTooLarge => AppError::new(ErrorKind::AvatarTooLarge, e.detail),
        ErrorKind::SkinExecutable => AppError::new(ErrorKind::SkinImageInvalid, e.detail),
        _ => e,
    }
}

fn slot_error(name: &str, state: &str) -> AppError {
    skin_error(ErrorKind::IconSlotInvalid, format!("{}/{}", super::manifest::from_skin(name), super::manifest::from_skin(state)))
}

/// 画像のバイト列を、その枠に入れられるか確かめる。`file_name` は拡張子にだけ使い、断るときの文には入れない
/// （文に入るのは `default.png` のようなアプリが決めた名前）
pub fn prepare(name: &str, state: &str, data: Vec<u8>, file_name: &str) -> AppResult<Prepared> {
    guard(prepare_unguarded(name, state, data, file_name))
}

fn prepare_unguarded(name: &str, state: &str, data: Vec<u8>, file_name: &str) -> AppResult<Prepared> {
    let (n, s) = slot(name, state).ok_or_else(|| slot_error(name, state))?;
    let ext = ext_of(file_name).ok_or_else(|| skin_error(ErrorKind::IconImageFormat, s))?;
    let inner = format!("{s}.{ext}");
    let checked = images::check(&data, &inner).map_err(as_icon_error)?;
    if is_app(n) {
        // アプリ・トレイのアイコンは静止画だけ（スキンの application と同じ）。正方形に整えられることも確かめる
        if checked.info.animated {
            return Err(skin_error(ErrorKind::SkinImageInvalid, format!("{inner}: an application icon must be a still image")));
        }
        images::app_icon_rgba(&data, &inner, WINDOW_ICON_PX)?;
        return Ok(Prepared { name: n, state: s, ext, data, still: None });
    }
    Ok(Prepared { name: n, state: s, ext, data, still: checked.first_frame_png })
}

/// 落としたファイル・選んだファイルを読む（ファイルでないもの・15MB を超えるもの・PNG・WebP・GIF でないものは読まない）。
/// 断るときの文に、ファイルの名前と場所は入れない
pub fn read_file(path: &Path) -> AppResult<(Vec<u8>, String)> {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    let shown = Path::new("icon");
    let meta = fs::metadata(path).map_err(|e| AppError::from_io(&e, shown))?;
    if !meta.is_file() {
        return Err(skin_error(ErrorKind::IconDropInvalid, "not a file"));
    }
    if ext_of(&file_name).is_none() {
        return Err(skin_error(ErrorKind::IconImageFormat, "icon"));
    }
    if meta.len() > images::MAX_IMAGE_BYTES {
        return Err(skin_error(ErrorKind::AvatarTooLarge, format!("icon: {} bytes (max {})", meta.len(), images::MAX_IMAGE_BYTES)));
    }
    let f = fs::File::open(path).map_err(|e| AppError::from_io(&e, shown))?;
    let mut data = Vec::new();
    f.take(images::MAX_IMAGE_BYTES + 1).read_to_end(&mut data).map_err(|e| AppError::from_io(&e, shown))?;
    if data.len() as u64 > images::MAX_IMAGE_BYTES {
        return Err(skin_error(ErrorKind::AvatarTooLarge, "icon"));
    }
    Ok((data, file_name))
}

/// ファイルを置く・消すのと、設定の `skin.icons` を書くのを一続きにする鍵（中でメインのスレッドを待たない）
static FILES: Mutex<()> = Mutex::new(());

pub fn files_lock() -> MutexGuard<'static, ()> {
    FILES.lock().unwrap_or_else(|p| p.into_inner())
}

/// フォルダを作って返す。リンクなら断る（外へ書かない）
fn ensure_dir(p: &Path) -> AppResult<()> {
    match fs::symlink_metadata(p) {
        Ok(m) if package::is_reparse(&m) => Err(skin_error(ErrorKind::SkinPathUnsafe, "icons folder is a link")),
        Ok(m) if m.is_dir() => Ok(()),
        Ok(_) => Err(skin_error(ErrorKind::SkinPathUnsafe, "icons folder is not a folder")),
        Err(_) => fs::create_dir_all(p).map_err(|e| AppError::from_io(&e, p)),
    }
}

/// 新しいファイルとして書く（既にあれば断る）
fn write_new_file(p: &Path, data: &[u8]) -> AppResult<()> {
    let mut f = fs::OpenOptions::new().write(true).create_new(true).open(p).map_err(|e| AppError::from_io(&e, p))?;
    f.write_all(data).map_err(|e| AppError::from_io(&e, p))
}

/// ファイル（かリンクそのもの）を消す。フォルダは消さない。無ければ何もしない
fn remove_file_no_follow(p: &Path) {
    if let Ok(m) = fs::symlink_metadata(p) {
        let r = if package::is_reparse(&m) && m.is_dir() {
            fs::remove_dir(p)
        } else if m.is_dir() {
            return;
        } else {
            fs::remove_file(p)
        };
        if let Err(e) = r {
            tracing::warn!("manual icon: could not remove an old file: {e}");
        }
    }
}

/// 確かめた画像をその枠のファイルとして置く（前の画像は置き換え、ほかの形式のファイルと使わなくなった最初の1コマは消す）。
/// 一時の場所に全部書いてから名前を変えて置く。途中で失敗したら一時のファイルを消す（前の画像は、置き換える直前までそのまま）
pub fn write(dirs: &Dirs, p: &Prepared) -> AppResult<()> {
    ensure_dir(&dirs.icons)?;
    ensure_dir(&dirs.icon_staging)?;
    let dir = dirs.icons.join(p.name);
    ensure_dir(&dir)?;
    let token = package::new_token();
    let tmp_main = dirs.icon_staging.join(format!("{token}.{}", p.ext));
    let tmp_still = dirs.icon_staging.join(format!("{token}-still.png"));
    let r = (|| -> AppResult<()> {
        write_new_file(&tmp_main, &p.data)?;
        let still_dir = dir.join(STILL_DIR);
        let still_dest = still_dir.join(format!("{}.png", p.state));
        match &p.still {
            Some(png) => {
                write_new_file(&tmp_still, png)?;
                ensure_dir(&still_dir)?;
                fs::rename(&tmp_still, &still_dest).map_err(|e| AppError::from_io(&e, &still_dest))?;
            }
            None => {
                remove_file_no_follow(&still_dest);
                // 空のときだけ消える
                let _ = fs::remove_dir(&still_dir);
            }
        }
        let dest = dir.join(format!("{}.{}", p.state, p.ext));
        fs::rename(&tmp_main, &dest).map_err(|e| AppError::from_io(&e, &dest))?;
        for e in IMAGE_EXTS.iter().filter(|e| **e != p.ext) {
            remove_file_no_follow(&dir.join(format!("{}.{e}", p.state)));
        }
        Ok(())
    })();
    if r.is_err() {
        let _ = fs::remove_file(&tmp_main);
        let _ = fs::remove_file(&tmp_still);
    }
    r
}

/// 1つの枠の画像を消す（どの形式も・最初の1コマも）。空になったフォルダも消す。リンクは辿らない
/// （`icons\` がリンクなら何もしない。`icons\<名前>` がリンクなら、リンクだけを外す）
pub fn remove(dirs: &Dirs, name: &str, state: &str) {
    let Some((n, s)) = slot(name, state) else { return };
    if reparse_at(&dirs.icons) {
        return;
    }
    let dir = dirs.icons.join(n);
    let Ok(m) = fs::symlink_metadata(&dir) else { return };
    if package::is_reparse(&m) {
        let _ = fs::remove_dir(&dir).or_else(|_| fs::remove_file(&dir));
        return;
    }
    if !m.is_dir() {
        return;
    }
    for e in IMAGE_EXTS {
        remove_file_no_follow(&dir.join(format!("{s}.{e}")));
    }
    let still_dir = dir.join(STILL_DIR);
    if !reparse_at(&still_dir) {
        remove_file_no_follow(&still_dir.join(format!("{s}.png")));
    }
    // 空のときだけ消える（ほかの状態の画像が残っていれば残る）
    let _ = fs::remove_dir(&still_dir);
    let _ = fs::remove_dir(&dir);
}

/// 手で入れたアイコンの画像を全部消す（「手で入れたアイコンを全部戻す」・「既定の見た目に戻す」）。
/// `icons\` がリンクなら、リンクだけを外す（先は辿らない・消さない）
pub fn remove_all(dirs: &Dirs) {
    match fs::symlink_metadata(&dirs.icons) {
        Ok(m) if package::is_reparse(&m) => {
            let _ = fs::remove_dir(&dirs.icons).or_else(|_| fs::remove_file(&dirs.icons));
        }
        Ok(m) if m.is_dir() => {
            if let Err(e) = fs::remove_dir_all(&dirs.icons) {
                tracing::warn!("manual icons: could not remove the folder: {e}");
            }
        }
        _ => {}
    }
    clean_staging(dirs);
}

/// 一時の場所を消す（起動のとき。書く途中で終わったものの残り）
pub fn clean_staging(dirs: &Dirs) {
    if fs::symlink_metadata(&dirs.icon_staging).map(|m| !package::is_reparse(&m)).unwrap_or(false) {
        let _ = fs::remove_dir_all(&dirs.icon_staging);
    }
}

// ---------------- ドロップ ----------------

/// OS からウィンドウに落とされたもの（Rust がウィンドウの知らせで控える）
#[derive(Debug, Clone)]
pub struct Dropped {
    pub paths: Vec<PathBuf>,
    /// 落とした位置（ウィンドウの中の物理ピクセル。画面の知らせと同じ値）
    pub x: f64,
    pub y: f64,
    pub at: Instant,
}

/// 控えておくドロップの数（1つだけだと、窓口が届く前に続けて落としたとき、前の記録が置き換わって見つからなかった）
pub const DROPS_KEPT: usize = 8;

/// 新しいドロップ（新しいものが後ろ。古いもの・数を超えたものは落とす。アイコンの枠が使ったものだけ消える）
static DROPS: Mutex<Vec<Dropped>> = Mutex::new(Vec::new());

/// ウィンドウにファイルが落とされた（lib.rs の on_window_event。メインのスレッドで呼ばれる。鍵は値を置く間だけ持つ）
pub fn record_drop(paths: &[PathBuf], x: f64, y: f64) {
    let d = Dropped { paths: paths.to_vec(), x, y, at: Instant::now() };
    let mut list = DROPS.lock().unwrap_or_else(|p| p.into_inner());
    push_drop(&mut list, d);
}

/// 控えに足す（古いもの・数を超えたものは落とす。テストで確かめる）
pub(crate) fn push_drop(list: &mut Vec<Dropped>, d: Dropped) {
    let now = d.at;
    list.retain(|x| now.checked_duration_since(x.at).map(|a| a <= DROP_MAX_AGE).unwrap_or(true));
    list.push(d);
    let over = list.len().saturating_sub(DROPS_KEPT);
    list.drain(..over);
}

/// 位置が同じで、古すぎないか
fn drop_matches(d: &Dropped, x: f64, y: f64, now: Instant) -> bool {
    (d.x - x).abs() < 0.5 && (d.y - y).abs() < 0.5 && now.checked_duration_since(d.at).map(|a| a <= DROP_MAX_AGE).unwrap_or(true)
}

/// 控えから、位置の合ういちばん新しいものだけを取り出す（合わなければ何も取り出さない。前は位置を比べる前に取り出していた）
pub(crate) fn take_matching(list: &mut Vec<Dropped>, x: f64, y: f64, now: Instant) -> Option<Dropped> {
    let i = list.iter().rposition(|d| drop_matches(d, x, y, now))?;
    Some(list.remove(i))
}

/// アイコンの枠に落としたファイルを取り出す（画面が知らせた位置と同じドロップだけ。使ったものだけ消える）
pub fn take_drop(x: f64, y: f64) -> AppResult<PathBuf> {
    let now = Instant::now();
    let d = {
        let mut list = DROPS.lock().unwrap_or_else(|p| p.into_inner());
        take_matching(&mut list, x, y, now)
    };
    pick_dropped(d, x, y, now)
}

/// 控えたドロップから、枠に入れるファイルを決める（ファイルには触らない。テストで確かめる）。
/// 位置が違う・古い・記録が無いものは使わない。落としたものが1つでなければ断る（複数・ファイルでないもの）。
/// フォルダかどうかは、読むとき（`read_file`）に断る
pub(crate) fn pick_dropped(d: Option<Dropped>, x: f64, y: f64, now: Instant) -> AppResult<PathBuf> {
    let d = d
        .filter(|d| drop_matches(d, x, y, now))
        .ok_or_else(|| skin_error(ErrorKind::IconDropInvalid, "no matching drop"))?;
    match d.paths.len() {
        0 => Err(skin_error(ErrorKind::IconDropInvalid, "nothing that is a file was dropped")),
        1 => Ok(d.paths.into_iter().next().expect("one path")),
        n => Err(skin_error(ErrorKind::IconDropInvalid, format!("{n} items were dropped"))),
    }
}

#[cfg(test)]
#[path = "icons_tests.rs"]
mod tests;
