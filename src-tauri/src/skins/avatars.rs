//! 登録した立ち絵（v3.9.0）。画面・README の呼び名は「立ち絵」、コードの中では avatar。
//!
//! - 画像を選ぶだけで作る（zip も skin.json も要らない）。再生中の絵は必須、停止中の絵は無くてよい（無ければ再生中の絵）
//! - 画像の確かめはスキンの立ち絵と同じ `images::check`（形式・大きさ・コマ数・画素・長さの上限も同じ。新しく緩めない）。
//!   2枚の画素の合計も、スキンの取り込みと同じ `limits::MAX_TOTAL_PIXELS` まで
//! - 置き場所はデータのフォルダの `avatars\<id>\`：`avatar.json`（名前と確かめた画像の情報）・`playing.<形式>`・
//!   `stopped.<形式>`（無くてよい）・`derived\<状態>.png`（動く画像の最初の1コマ）。作るときに画像を写して置き、元のファイルは使い続けない。
//!   「変更」で差し替えた絵は `playing-<印>.<形式>`（最初の1コマは `derived\playing-<印>.png`）の新しい名前で書く（画面の URL が変わるので、
//!   動く画像も読み直される。書き終えてから avatar.json を置き換えるので、途中で失敗しても前の絵が残る）
//! - id はアプリが作る 16 桁の英小文字と数字（`package::valid_token` と同じ形）。画面から来た id はこの形を確かめてからフォルダの名前にする
//!   （`..`・区切り・予約名・ドライブは、この形に入らない）。フォルダとファイルは途中の段も含めてリンクを辿らない（`package::safe_file`）
//! - 作る途中は `avatars-staging\<id>\` に書き、全部書けたら `avatars\` へ移して読み込んで確かめる（スキンの取り込みと同じ）
//!
//! 「使う立ち絵」の値（設定の `skin.avatar`）と、その解き方（`resolve`）もここに置く。

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::images::{self, ImageInfo};
use super::manifest::{self, guard, skin_error};
use super::names;
use super::package::{self, AssetRef, CharacterAssets, ResolvedSkin};
use super::{limits, Dirs};
use crate::error::{AppError, AppResult, ErrorKind};

/// 登録できる数（暫定）
pub const MAX_AVATARS: usize = 30;
/// 名前の長さ（文字数）
pub const NAME_MAX: usize = 40;
/// v3.9.0：名前が空で、ファイル名からも名前が作れないときの名前
pub const DEFAULT_NAME: &str = "立ち絵";
/// 置き方（スキンの立ち絵の既定の値と同じ。docs\着せ替え_仕様.md 6章）
pub const WIDTH_PX: u32 = 180;
pub const MAX_HEIGHT_PX: u32 = 420;
pub const MIN_WINDOW_WIDTH_PX: u32 = 920;

const META_FILE: &str = "avatar.json";
const META_VERSION: u32 = 1;
/// avatar.json の大きさの上限（名前と画像2枚の情報で 1KB ほど）
const MAX_META_BYTES: u64 = 64 * 1024;
/// 選んだ画像の見本（data: URL）の縦横
const PICK_PREVIEW_PX: u32 = 200;

/// 使う立ち絵の値
pub const CHOICE_SKIN: &str = "skin";
pub const CHOICE_NONE: &str = "none";
const AVATAR_PREFIX: &str = "avatar:";
const SKIN_PREFIX: &str = "skin:";

/// 使う立ち絵（設定の `skin.avatar` の値）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// `skin`：当てているスキンの立ち絵（既定。3.8.1 から上げたときも）
    Skin,
    /// `none`：出さない
    None,
    /// `avatar:<id>`：登録した立ち絵
    Avatar(String),
    /// `skin:<id>`：取り込んだスキンの立ち絵（スキンの中の絵を指すだけ。写さない）
    SkinCharacter(String),
}

impl Choice {
    /// 形の合わない値は None（呼び出し側は「スキンの立ち絵」にする）
    pub fn parse(s: &str) -> Option<Choice> {
        match s {
            CHOICE_SKIN => Some(Choice::Skin),
            CHOICE_NONE => Some(Choice::None),
            _ => {
                if let Some(id) = s.strip_prefix(AVATAR_PREFIX) {
                    valid_id(id).then(|| Choice::Avatar(id.to_string()))
                } else if let Some(id) = s.strip_prefix(SKIN_PREFIX) {
                    names::valid_id(id).then(|| Choice::SkinCharacter(id.to_string()))
                } else {
                    None
                }
            }
        }
    }

    pub fn as_setting(&self) -> String {
        match self {
            Choice::Skin => CHOICE_SKIN.into(),
            Choice::None => CHOICE_NONE.into(),
            Choice::Avatar(id) => format!("{AVATAR_PREFIX}{id}"),
            Choice::SkinCharacter(id) => format!("{SKIN_PREFIX}{id}"),
        }
    }
}

/// 設定に置ける形にそろえる（形の合わない値は「スキンの立ち絵」）
pub fn normalize_choice(s: &str) -> String {
    Choice::parse(s).map(|c| c.as_setting()).unwrap_or_else(|| CHOICE_SKIN.into())
}

/// 立ち絵の id の形（アプリが作る 16 桁の英小文字と数字）
pub fn valid_id(id: &str) -> bool {
    package::valid_token(id)
}

/// 再生中・停止中
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Playing,
    Stopped,
}

impl Slot {
    pub fn parse(s: &str) -> Option<Slot> {
        match s {
            "playing" => Some(Slot::Playing),
            "stopped" => Some(Slot::Stopped),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Slot::Playing => "playing",
            Slot::Stopped => "stopped",
        }
    }
    pub fn index(self) -> usize {
        match self {
            Slot::Playing => 0,
            Slot::Stopped => 1,
        }
    }
}

/// 画像のファイル名：`<状態>.<形式>`（作ったとき）か `<状態>-<印>.<形式>`（「変更」で差し替えたとき。印は 16 桁の英小文字と数字）
fn image_file(slot: Slot, rev: Option<&str>, ext: &str) -> String {
    match rev {
        Some(r) => format!("{}-{r}.{ext}", slot.name()),
        None => format!("{}.{ext}", slot.name()),
    }
}

/// 読むときに受けるファイル名か（アプリが書く2つの形だけ）
fn image_file_ok(file: &str, slot: Slot) -> bool {
    let Some((stem, ext)) = file.rsplit_once('.') else { return false };
    if !names::IMAGE_EXTS.contains(&ext) {
        return false;
    }
    stem == slot.name() || stem.strip_prefix(slot.name()).and_then(|r| r.strip_prefix('-')).map(package::valid_token).unwrap_or(false)
}

/// 動く画像の最初の1コマの置き場所（画像のファイル名から決まる：`derived/<拡張子を外した名前>.png`）
fn still_for(file: &str) -> String {
    format!("derived/{}.png", file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct MetaImage {
    /// `playing.<形式>`・`stopped.<形式>`（形式は png・webp・gif）。読むときにこの形だけを受ける
    file: String,
    /// 動く画像の最初の1コマ（`derived/<状態>.png`）。静止画は None
    still: Option<String>,
    #[serde(flatten)]
    image: ImageInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    version: u32,
    name: String,
    created_at: i64,
    playing: MetaImage,
    stopped: Option<MetaImage>,
}

/// 一覧の1行
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AvatarSummary {
    pub id: String,
    pub name: String,
    /// 一覧の見本（静止画のパス。停止中の絵、無ければ再生中の絵。動く画像なら最初の1コマ）
    pub preview: Option<String>,
    /// 停止中の絵を持つか
    pub has_stopped: bool,
    /// v3.9.0：「変更」の小窓の見本（静止画のパス。動く画像なら最初の1コマ）。停止中が無ければ None
    pub playing_preview: Option<String>,
    pub stopped_preview: Option<String>,
    pub animated: bool,
    /// 読めないときの種別（消すことだけできる）
    pub broken: Option<ErrorKind>,
}

/// 取り込んだスキンのうち、立ち絵を持つもの（一覧で選べる。スキンの中の絵を指すだけ）
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkinCharacterRow {
    pub skin_id: String,
    pub name: String,
    pub preview: Option<String>,
    pub animated: bool,
}

/// 設定の「立ち絵」の欄に出す一覧
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AvatarList {
    pub skins: Vec<SkinCharacterRow>,
    pub avatars: Vec<AvatarSummary>,
    pub max: usize,
}

/// 選んで確かめた画像（作るまでメモリに置く。画面へはパスを返さず、印 token で指させる）
pub struct Picked {
    pub token: String,
    pub slot: Slot,
    ext: &'static str,
    data: Vec<u8>,
    checked: images::Checked,
    /// ファイル名から作った名前（名前を空のまま作ったときに使う。`name_from_file`）
    name_hint: String,
}

/// 選んだ画像のうち、画面へ返すもの（パスは返さない）
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PickedImage {
    pub token: String,
    pub slot: String,
    /// 見本（data: URL の PNG。縦横 200px 以内）
    pub preview_data_url: Option<String>,
    /// 名前の既定（ファイル名から作った名前。`name_from_file`。空にはならない）。名前を空のまま作ると、再生中の絵のこの名前になる
    pub suggested_name: String,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub duration_ms: u64,
    pub animated: bool,
    pub bytes: u64,
}

/// 入っている立ち絵（読み込んで確かめたもの）
pub struct Avatar {
    pub id: String,
    pub dir: PathBuf,
    meta: Meta,
}

fn not_found(id: &str) -> AppError {
    skin_error(ErrorKind::AvatarNotFound, manifest::from_skin(id))
}

fn broken(id: &str, why: impl std::fmt::Display) -> AppError {
    skin_error(ErrorKind::AvatarBroken, format!("{}: {why}", manifest::from_skin(id)))
}

/// 画像の確かめ（スキンと共有）の種別のうち、文が zip のことを言うものを立ち絵の種別に読み替える
fn as_avatar_error(e: AppError) -> AppError {
    match e.kind {
        ErrorKind::SkinTooLarge => AppError::new(ErrorKind::AvatarTooLarge, e.detail),
        ErrorKind::SkinExecutable => AppError::new(ErrorKind::SkinImageInvalid, e.detail),
        _ => e,
    }
}

impl std::fmt::Debug for Picked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picked").field("token", &self.token).field("slot", &self.slot).field("bytes", &self.data.len()).finish()
    }
}

impl std::fmt::Debug for Avatar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Avatar").field("id", &self.id).field("name", &self.meta.name).finish()
    }
}

fn name_error(e: AppError) -> AppError {
    AppError::new(ErrorKind::AvatarNameInvalid, e.detail)
}

/// 名前を確かめる（前後の空白は外す）
pub fn check_name(s: &str) -> AppResult<String> {
    manifest::check_display_name(s.trim(), NAME_MAX).map_err(name_error)
}

/// 拡張子から形式（png・webp・gif だけ。大文字小文字は区別しない）
fn ext_of(file_name: &str) -> Option<&'static str> {
    let ext = Path::new(file_name).extension()?.to_str()?.to_ascii_lowercase();
    names::IMAGE_EXTS.iter().copied().find(|e| *e == ext)
}

/// 名前の既定（v3.9.0。作者の決定）：ファイル名から拡張子を外したもの。名前の決まりに合わなければ、合うように詰める
/// （40 文字で切る・使えない文字を外す＝名前の見える文字 `is_name_base` と空白だけを残す）。それでも空なら「立ち絵」
pub fn name_from_file(file_name: &str) -> String {
    let stem = Path::new(file_name).file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let cut: String = stem.trim().chars().take(NAME_MAX).collect();
    if let Ok(n) = check_name(&cut) {
        return n;
    }
    let kept: String = stem.chars().filter(|c| *c == ' ' || manifest::is_name_base(*c)).collect();
    let cut: String = kept.trim().chars().take(NAME_MAX).collect();
    check_name(&cut).unwrap_or_else(|_| DEFAULT_NAME.to_string())
}

/// 選んだファイルを読んで確かめる（ファイルを選ぶ画面で選んだもの）。ファイルでないもの（フォルダ・デバイス）は読まない
pub fn pick_file(path: &Path, slot: Slot) -> AppResult<(Picked, PickedImage)> {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    // 拡張子を先に見る（読む前に断る）
    if ext_of(&file_name).is_none() {
        return Err(skin_error(ErrorKind::AvatarImageFormat, slot.name()));
    }
    let meta = fs::metadata(path).map_err(|e| AppError::from_io(&e, Path::new(slot.name())))?;
    if !meta.is_file() {
        return Err(skin_error(ErrorKind::SkinImageInvalid, format!("{}: not a file", slot.name())));
    }
    if meta.len() > images::MAX_IMAGE_BYTES {
        return Err(skin_error(ErrorKind::AvatarTooLarge, format!("{}: {} bytes (max {})", slot.name(), meta.len(), images::MAX_IMAGE_BYTES)));
    }
    let f = fs::File::open(path).map_err(|e| AppError::from_io(&e, Path::new(slot.name())))?;
    let mut data = Vec::new();
    f.take(images::MAX_IMAGE_BYTES + 1).read_to_end(&mut data).map_err(|e| AppError::from_io(&e, Path::new(slot.name())))?;
    pick_bytes(data, &file_name, slot)
}

/// 画像のバイト列を確かめる（`pick_file` の本体。テストからも呼ぶ）。`file_name` は拡張子と名前の既定にだけ使い、
/// 断るときの文には入れない（文に入るのは `playing.png` のようなアプリが決めた名前）
pub fn pick_bytes(data: Vec<u8>, file_name: &str, slot: Slot) -> AppResult<(Picked, PickedImage)> {
    guard(pick_unguarded(data, file_name, slot))
}

fn pick_unguarded(data: Vec<u8>, file_name: &str, slot: Slot) -> AppResult<(Picked, PickedImage)> {
    let ext = ext_of(file_name).ok_or_else(|| skin_error(ErrorKind::AvatarImageFormat, slot.name()))?;
    let inner = format!("{}.{ext}", slot.name());
    let checked = images::check(&data, &inner).map_err(as_avatar_error)?;
    let preview_data_url = match &checked.first_frame_png {
        Some(png) => images::preview_data_url(png, "first.png", PICK_PREVIEW_PX),
        None => images::preview_data_url(&data, &inner, PICK_PREVIEW_PX),
    };
    let token = package::new_token();
    let i = &checked.info;
    let shown = PickedImage {
        token: token.clone(),
        slot: slot.name().into(),
        preview_data_url,
        suggested_name: name_from_file(file_name),
        width: i.width,
        height: i.height,
        frames: i.frames,
        duration_ms: i.duration_ms,
        animated: i.animated,
        bytes: data.len() as u64,
    };
    let name_hint = shown.suggested_name.clone();
    Ok((Picked { token, slot, ext, data, checked, name_hint }, shown))
}

fn reparse_at(p: &Path) -> bool {
    fs::symlink_metadata(p).map(|m| package::is_reparse(&m)).unwrap_or(false)
}

/// `avatars\<id>`。id の形を確かめ、置き場所とフォルダがリンクなら断る
fn avatar_dir(dirs: &Dirs, id: &str) -> AppResult<PathBuf> {
    if !valid_id(id) {
        return Err(not_found(id));
    }
    if reparse_at(&dirs.avatars) {
        return Err(broken(id, "avatars folder is a link"));
    }
    let dir = dirs.avatars.join(id);
    let meta = fs::symlink_metadata(&dir).map_err(|_| not_found(id))?;
    if package::is_reparse(&meta) {
        return Err(broken(id, "avatar folder is a link"));
    }
    if !meta.is_dir() {
        return Err(not_found(id));
    }
    Ok(dir)
}

/// 入っている立ち絵を読み、確かめる（使うとき・一覧）。画像は復号せず、在ること・大きさ・頭のバイトを見る
/// （中身は作るときに全部確かめてある。スキンの読み込みと同じ）。avatar.json の中のファイル名は、アプリが書く形だけを受ける
pub fn load(dirs: &Dirs, id: &str) -> AppResult<Avatar> {
    guard(load_unguarded(dirs, id))
}

fn load_unguarded(dirs: &Dirs, id: &str) -> AppResult<Avatar> {
    let dir = avatar_dir(dirs, id)?;
    let mpath = package::safe_file(&dir, META_FILE).map_err(|_| broken(id, "avatar.json is missing"))?;
    let bytes = package::read_capped(&mpath, MAX_META_BYTES).map_err(|e| broken(id, e.detail))?;
    let meta: Meta = serde_json::from_slice(&bytes).map_err(|e| broken(id, format!("avatar.json: {e}")))?;
    if meta.version != META_VERSION {
        return Err(broken(id, "avatar.json version"));
    }
    manifest::check_display_name(&meta.name, NAME_MAX).map_err(|e| broken(id, e.detail))?;
    check_stored(&dir, id, &meta.playing, Slot::Playing)?;
    if let Some(s) = &meta.stopped {
        check_stored(&dir, id, s, Slot::Stopped)?;
    }
    Ok(Avatar { id: id.to_string(), dir, meta })
}

fn check_stored(dir: &Path, id: &str, m: &MetaImage, slot: Slot) -> AppResult<()> {
    if !image_file_ok(&m.file, slot) {
        return Err(broken(id, format!("unexpected file name for {}", slot.name())));
    }
    let f = package::safe_file(dir, &m.file).map_err(|e| broken(id, e.detail))?;
    let len = fs::metadata(&f).map(|x| x.len()).unwrap_or(u64::MAX);
    if len > images::MAX_IMAGE_BYTES {
        return Err(broken(id, format!("{} is too large", m.file)));
    }
    let mut head = [0u8; 32];
    let n = fs::File::open(&f).and_then(|mut h| h.read(&mut head)).map_err(|e| broken(id, e))?;
    if !images::signature_matches(&m.file, &head[..n]) {
        return Err(broken(id, format!("{}: content does not match the extension", m.file)));
    }
    if m.image.width == 0 || m.image.height == 0 || m.image.width > images::MAX_SIDE || m.image.height > images::MAX_SIDE {
        return Err(broken(id, format!("{}: size", m.file)));
    }
    match (&m.still, m.image.animated) {
        (Some(s), true) if *s == still_for(&m.file) => {
            package::safe_file(dir, s).map_err(|e| broken(id, e.detail))?;
        }
        (None, false) => {}
        _ => return Err(broken(id, format!("{}: still frame", m.file))),
    }
    Ok(())
}

impl Avatar {
    pub fn name(&self) -> &str {
        &self.meta.name
    }

    fn asset(&self, m: &MetaImage) -> AssetRef {
        AssetRef {
            path: package::abs(&self.dir, &m.file),
            still: m.still.as_deref().map(|s| package::abs(&self.dir, s)),
            width: m.image.width,
            height: m.image.height,
            animated: m.image.animated,
        }
    }

    /// 画面へ渡す立ち絵（置き方はスキンの立ち絵の既定の値）。停止中の絵が無ければ、再生中の絵を両方に使う
    pub fn character(&self) -> CharacterAssets {
        let playing = self.asset(&self.meta.playing);
        let stopped = self.meta.stopped.as_ref().map(|s| self.asset(s)).unwrap_or_else(|| playing.clone());
        CharacterAssets { playing, stopped, width_px: WIDTH_PX, max_height_px: MAX_HEIGHT_PX, min_window_width_px: MIN_WINDOW_WIDTH_PX }
    }

    pub fn summary(&self) -> AvatarSummary {
        let still = |m: &MetaImage| {
            let a = self.asset(m);
            a.still.unwrap_or(a.path)
        };
        let playing_preview = still(&self.meta.playing);
        let stopped_preview = self.meta.stopped.as_ref().map(still);
        AvatarSummary {
            id: self.id.clone(),
            name: self.meta.name.clone(),
            preview: Some(stopped_preview.clone().unwrap_or_else(|| playing_preview.clone())),
            has_stopped: self.meta.stopped.is_some(),
            playing_preview: Some(playing_preview),
            stopped_preview,
            animated: self.meta.playing.image.animated || self.meta.stopped.as_ref().map(|s| s.image.animated).unwrap_or(false),
            broken: None,
        }
    }
}

/// `avatars\` の下の、立ち絵の形の名前のもの（読めないものも数える）
fn ids(dirs: &Dirs) -> Vec<String> {
    if reparse_at(&dirs.avatars) {
        return vec![];
    }
    let Ok(rd) = fs::read_dir(&dirs.avatars) else { return vec![] };
    rd.flatten().filter_map(|e| e.file_name().to_str().map(|s| s.to_string())).filter(|n| valid_id(n)).collect()
}

/// 入っている数（読めないものも数える。上限 `MAX_AVATARS` に使う）
pub fn count(dirs: &Dirs) -> usize {
    ids(dirs).len()
}

/// 一覧（作った順。読めないものも、消せるように最後に出す）
pub fn list(dirs: &Dirs) -> Vec<AvatarSummary> {
    let mut ok: Vec<(i64, AvatarSummary)> = vec![];
    let mut bad: Vec<AvatarSummary> = vec![];
    for id in ids(dirs) {
        match load(dirs, &id) {
            Ok(a) => ok.push((a.meta.created_at, a.summary())),
            Err(e) => {
                tracing::warn!("avatar {id} is broken: {:?}", e.kind);
                bad.push(AvatarSummary {
                    id: id.clone(),
                    name: String::new(),
                    preview: None,
                    has_stopped: false,
                    playing_preview: None,
                    stopped_preview: None,
                    animated: false,
                    broken: Some(e.kind),
                });
            }
        }
    }
    ok.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
    bad.sort_by(|a, b| a.id.cmp(&b.id));
    ok.into_iter().map(|(_, s)| s).chain(bad).collect()
}

/// 取り込んだスキンのうち、立ち絵を持つもの（読めないスキンは出さない）。名前の順
pub fn skin_characters(dirs: &Dirs, app_version: &str) -> Vec<SkinCharacterRow> {
    let mut out = vec![];
    for s in package::list(dirs, app_version) {
        if s.builtin || s.broken.is_some() || !s.parts.character {
            continue;
        }
        let Ok(inst) = package::load(dirs, &s.id, app_version) else { continue };
        let Some(c) = inst.manifest.character.as_ref() else { continue };
        let animated = [&c.playing, &c.stopped].iter().any(|p| inst.assets.get(p.as_str()).map(|a| a.image.animated).unwrap_or(false));
        out.push(SkinCharacterRow { skin_id: s.id.clone(), name: s.name.clone(), preview: inst.still_path(&c.stopped), animated });
    }
    out
}

pub fn list_all(dirs: &Dirs, app_version: &str) -> AvatarList {
    AvatarList { skins: skin_characters(dirs, app_version), avatars: list(dirs), max: MAX_AVATARS }
}

/// 一時の場所（`avatars-staging`）を作って返す。リンクなら断る（外へ書かない）
fn staging_root(dirs: &Dirs) -> AppResult<&Path> {
    fs::create_dir_all(&dirs.avatar_staging).map_err(|e| AppError::from_io(&e, &dirs.avatar_staging))?;
    if reparse_at(&dirs.avatar_staging) {
        return Err(skin_error(ErrorKind::SkinPathUnsafe, "avatars-staging is a link"));
    }
    Ok(&dirs.avatar_staging)
}

fn avatars_root(dirs: &Dirs) -> AppResult<&Path> {
    fs::create_dir_all(&dirs.avatars).map_err(|e| AppError::from_io(&e, &dirs.avatars))?;
    if reparse_at(&dirs.avatars) {
        return Err(skin_error(ErrorKind::SkinPathUnsafe, "avatars is a link"));
    }
    Ok(&dirs.avatars)
}

/// 立ち絵を作る。名前・数の上限・画素の合計を確かめてから、一時の場所に書き、`avatars\<id>\` へ移して読み込んで確かめる。
/// どこで断っても、一時の場所は消し、正式の場所には何も残さない
pub fn create(dirs: &Dirs, name: &str, playing: &Picked, stopped: Option<&Picked>) -> AppResult<AvatarSummary> {
    guard(create_unguarded(dirs, name, playing, stopped))
}

fn create_unguarded(dirs: &Dirs, name: &str, playing: &Picked, stopped: Option<&Picked>) -> AppResult<AvatarSummary> {
    if playing.slot != Slot::Playing || stopped.map(|s| s.slot != Slot::Stopped).unwrap_or(false) {
        return Err(skin_error(ErrorKind::AvatarPickExpired, "slot"));
    }
    // 名前が空なら、再生中の絵のファイル名から作った名前（v3.9.0。断らない）
    let name = if name.trim().is_empty() { playing.name_hint.clone() } else { check_name(name)? };
    let name = check_name(&name).unwrap_or_else(|_| DEFAULT_NAME.to_string());
    let n = count(dirs);
    if n >= MAX_AVATARS {
        return Err(skin_error(ErrorKind::AvatarLimit, format!("{n} >= {MAX_AVATARS}")));
    }
    let total = playing.checked.info.decoded_pixels + stopped.map(|s| s.checked.info.decoded_pixels).unwrap_or(0);
    if total > limits::MAX_TOTAL_PIXELS {
        return Err(skin_error(ErrorKind::SkinImageLimit, format!("images total more than {} pixels", limits::MAX_TOTAL_PIXELS)));
    }
    let root = avatars_root(dirs)?.to_path_buf();
    let staging = staging_root(dirs)?.to_path_buf();
    let id = loop {
        let t = package::new_token();
        if fs::symlink_metadata(root.join(&t)).is_err() && fs::symlink_metadata(staging.join(&t)).is_err() {
            break t;
        }
    };
    let tmp = staging.join(&id);
    fs::create_dir(&tmp).map_err(|e| AppError::from_io(&e, &tmp))?;
    let write_all = || -> AppResult<()> {
        let mut written = vec![];
        let pm = write_image(&tmp, playing, None, &mut written)?;
        let sm = match stopped {
            Some(s) => Some(write_image(&tmp, s, None, &mut written)?),
            None => None,
        };
        let meta = Meta { version: META_VERSION, name: name.clone(), created_at: crate::db::now_ms(), playing: pm, stopped: sm };
        package::write_new(&tmp, META_FILE, &serde_json::to_vec_pretty(&meta)?)
    };
    if let Err(e) = write_all() {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let target = root.join(&id);
    if let Err(e) = fs::rename(&tmp, &target) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(AppError::from_io(&e, &target));
    }
    match load(dirs, &id) {
        Ok(a) => Ok(a.summary()),
        Err(e) => {
            // 読めないものを正式の場所に残さない（target は上で「無い」ことを確かめた名前で、ここで作ったもの）
            tracing::warn!("avatar {id} could not be loaded after create: {:?}", e.kind);
            let _ = fs::remove_dir_all(&target);
            Err(e)
        }
    }
}

/// 画像（と動く画像の最初の1コマ）を新しいファイルとして書く。書いたものは `written` に足す（失敗したときに消すため）
fn write_image(dir: &Path, p: &Picked, rev: Option<&str>, written: &mut Vec<String>) -> AppResult<MetaImage> {
    let file = image_file(p.slot, rev, p.ext);
    package::write_new(dir, &file, &p.data)?;
    written.push(file.clone());
    let still = match &p.checked.first_frame_png {
        Some(png) => {
            let s = still_for(&file);
            package::write_new(dir, &s, png)?;
            written.push(s.clone());
            Some(s)
        }
        None => None,
    };
    Ok(MetaImage { file, still, image: p.checked.info.clone() })
}

/// 立ち絵の中のファイルを消す（リンクは辿らない。無ければ何もしない）。消せなくても続ける（画面がまだ開いているときなど）
fn remove_inside(dir: &Path, rel: &str) {
    if let Ok(p) = package::safe_file(dir, rel) {
        if let Err(e) = fs::remove_file(&p) {
            tracing::warn!("avatar: could not remove an old file: {e}");
        }
    }
}

/// 「変更」のときの停止中の絵
pub enum StoppedChange<'a> {
    /// 今のまま
    Keep,
    /// 差し替える
    Replace(&'a Picked),
    /// 外す（停止中も再生中の絵）
    Remove,
}

/// 作った立ち絵を変える（v3.9.0。作者の決定）：再生中だけ／停止中だけを差し替える・停止中を外す・名前を変える。
/// 名前が空なら今の名前のまま。差し替える絵は新しい名前のファイル（`<状態>-<印>.<形式>`）に書き、全部書けてから avatar.json を
/// 一時のファイル経由で置き換える。途中で失敗したら書いたものを消し、前の絵と avatar.json はそのまま。置き換えたあとで、使わなくなった絵を消す
pub fn update(dirs: &Dirs, id: &str, name: &str, playing: Option<&Picked>, stopped: StoppedChange) -> AppResult<AvatarSummary> {
    update_then(dirs, id, name, playing, stopped, || {})
}

/// `update` と同じ。avatar.json を置き換えたあと、古い絵を消す前に `after_commit` を呼ぶ（使っている立ち絵なら、
/// 先に画面の絵を新しい絵へ当て直してから古い絵を消す。削除の `runtime::switch_then_delete` と同じ考え）。失敗したときは呼ばない
pub fn update_then(
    dirs: &Dirs,
    id: &str,
    name: &str,
    playing: Option<&Picked>,
    stopped: StoppedChange,
    after_commit: impl FnOnce(),
) -> AppResult<AvatarSummary> {
    guard(update_unguarded(dirs, id, name, playing, stopped, after_commit))
}

fn update_unguarded(
    dirs: &Dirs,
    id: &str,
    name: &str,
    playing: Option<&Picked>,
    stopped: StoppedChange,
    after_commit: impl FnOnce(),
) -> AppResult<AvatarSummary> {
    let a = load(dirs, id)?;
    if playing.map(|p| p.slot != Slot::Playing).unwrap_or(false) || matches!(stopped, StoppedChange::Replace(s) if s.slot != Slot::Stopped) {
        return Err(skin_error(ErrorKind::AvatarPickExpired, "slot"));
    }
    let name = if name.trim().is_empty() { a.meta.name.clone() } else { check_name(name)? };
    let new_pixels = playing.map(|p| p.checked.info.decoded_pixels).unwrap_or(a.meta.playing.image.decoded_pixels)
        + match &stopped {
            StoppedChange::Keep => a.meta.stopped.as_ref().map(|s| s.image.decoded_pixels).unwrap_or(0),
            StoppedChange::Replace(s) => s.checked.info.decoded_pixels,
            StoppedChange::Remove => 0,
        };
    if new_pixels > limits::MAX_TOTAL_PIXELS {
        return Err(skin_error(ErrorKind::SkinImageLimit, format!("images total more than {} pixels", limits::MAX_TOTAL_PIXELS)));
    }
    let rev = package::new_token();
    let mut written: Vec<String> = vec![];
    let tmp_name = "avatar.json.tmp";
    let mut old: Vec<String> = vec![];
    let result = (|| -> AppResult<()> {
        let mut meta = a.meta.clone();
        meta.name = name.clone();
        if let Some(p) = playing {
            meta.playing = write_image(&a.dir, p, Some(&rev), &mut written)?;
            old.push(a.meta.playing.file.clone());
            old.extend(a.meta.playing.still.clone());
        }
        match &stopped {
            StoppedChange::Keep => {}
            StoppedChange::Replace(s) => {
                meta.stopped = Some(write_image(&a.dir, s, Some(&rev), &mut written)?);
            }
            StoppedChange::Remove => meta.stopped = None,
        }
        if !matches!(stopped, StoppedChange::Keep) {
            if let Some(s) = &a.meta.stopped {
                old.push(s.file.clone());
                old.extend(s.still.clone());
            }
        }
        // 前に残った一時のファイル（リンクならリンクそのもの）を消してから、新しいファイルとして書いて置き換える
        let _ = fs::remove_file(a.dir.join(tmp_name));
        package::write_new(&a.dir, tmp_name, &serde_json::to_vec_pretty(&meta)?)?;
        written.push(tmp_name.to_string());
        let dest = a.dir.join(META_FILE);
        fs::rename(a.dir.join(tmp_name), &dest).map_err(|e| AppError::from_io(&e, &dest))?;
        written.pop();
        Ok(())
    })();
    if let Err(e) = result {
        for w in &written {
            remove_inside(&a.dir, w);
        }
        return Err(e);
    }
    // 置き換えた。古い絵を消す前に、呼び出し側が画面の絵を当て直す（画面が古い絵を手放してから消す）
    after_commit();
    // 使わなくなった絵を消す（新しく書いた名前とぶつからないものだけ）
    for o in old.iter().filter(|o| !written.contains(o)) {
        remove_inside(&a.dir, o);
    }
    Ok(load(dirs, id)?.summary())
}

/// 消す。読めない立ち絵も消せる。フォルダがリンク（ジャンクション）なら、リンクだけを外す（先は辿らない・消さない）
pub fn delete(dirs: &Dirs, id: &str) -> AppResult<()> {
    if !valid_id(id) {
        return Err(not_found(id));
    }
    if reparse_at(&dirs.avatars) {
        return Err(broken(id, "avatars folder is a link"));
    }
    let dir = dirs.avatars.join(id);
    let meta = fs::symlink_metadata(&dir).map_err(|_| not_found(id))?;
    if package::is_reparse(&meta) {
        return fs::remove_dir(&dir).or_else(|_| fs::remove_file(&dir)).map_err(|e| AppError::from_io(&e, &dir));
    }
    if !meta.is_dir() {
        return Err(not_found(id));
    }
    fs::remove_dir_all(&dir).map_err(|e| AppError::from_io(&e, &dir))
}

/// 一時の場所を全部消す（起動のとき。作る途中で終わったものの残り）
pub fn clean_staging(dirs: &Dirs) {
    if fs::symlink_metadata(&dirs.avatar_staging).map(|m| !package::is_reparse(&m)).unwrap_or(false) {
        let _ = fs::remove_dir_all(&dirs.avatar_staging);
    }
}

/// 使う立ち絵を解いた結果
#[derive(Debug, Clone, PartialEq)]
pub struct Resolution {
    /// 画面に出す立ち絵（出さないときは None）
    pub character: Option<CharacterAssets>,
    /// 選んでいた立ち絵・スキンが無い・読めないので、「スキンの立ち絵」として解いた（呼び出し側は設定を `skin` に直す）
    pub fallback: Option<ErrorKind>,
}

/// 使う立ち絵を解く（ファイルには触らない。読み込みは渡された関数で。テストで確かめる）。
/// - セーフモード（既定の見た目）の間は出さない
/// - `skin`：当てているスキンの立ち絵（無ければ出さない）／`none`：出さない
/// - `avatar:<id>`：登録した立ち絵。読めなければ「スキンの立ち絵」に戻す
/// - `skin:<id>`：取り込んだスキンの立ち絵（当てているスキンでもよい）。無い・読めない・立ち絵が無ければ「スキンの立ち絵」に戻す
/// - 形の合わない値は「スキンの立ち絵」
pub fn resolve(
    choice: &str,
    current: Option<&ResolvedSkin>,
    safe_mode: bool,
    load_avatar: impl FnOnce(&str) -> AppResult<CharacterAssets>,
    load_skin: impl FnOnce(&str) -> AppResult<Option<CharacterAssets>>,
) -> Resolution {
    let skin_character = || current.and_then(|s| s.character.clone());
    if safe_mode {
        return Resolution { character: None, fallback: None };
    }
    let back = |kind: ErrorKind| Resolution { character: skin_character(), fallback: Some(kind) };
    match Choice::parse(choice) {
        None | Some(Choice::Skin) => Resolution { character: skin_character(), fallback: None },
        Some(Choice::None) => Resolution { character: None, fallback: None },
        Some(Choice::Avatar(id)) => match load_avatar(&id) {
            Ok(c) => Resolution { character: Some(c), fallback: None },
            Err(e) => back(e.kind),
        },
        Some(Choice::SkinCharacter(id)) => {
            if let Some(s) = current.filter(|s| s.id == id) {
                return match &s.character {
                    Some(c) => Resolution { character: Some(c.clone()), fallback: None },
                    None => back(ErrorKind::AvatarNotFound),
                };
            }
            match load_skin(&id) {
                Ok(Some(c)) => Resolution { character: Some(c), fallback: None },
                Ok(None) => back(ErrorKind::AvatarNotFound),
                Err(e) => back(e.kind),
            }
        }
    }
}

/// 「使う」で選べるか（選ぶ前に確かめる）。選べるなら設定に置く値を返す
pub fn check_choice(dirs: &Dirs, choice: &str, app_version: &str) -> AppResult<String> {
    let c = Choice::parse(choice).ok_or_else(|| skin_error(ErrorKind::AvatarNotFound, manifest::from_skin(choice)))?;
    match &c {
        Choice::Skin | Choice::None => {}
        Choice::Avatar(id) => {
            load(dirs, id)?;
        }
        Choice::SkinCharacter(id) => {
            let inst = package::load(dirs, id, app_version)?;
            if inst.manifest.character.is_none() {
                return Err(skin_error(ErrorKind::AvatarNotFound, format!("skin {} has no character", manifest::from_skin(id))));
            }
        }
    }
    Ok(c.as_setting())
}

#[cfg(test)]
#[path = "avatars_tests.rs"]
mod tests;
