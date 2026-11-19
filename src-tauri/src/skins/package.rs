//! スキンの取り込み（zip → 一時の場所 → 確認 → 正式の場所）・読み込み・一覧・削除・書き出し（v3.2.0）。
//! ファイルに触るのはここだけ。画面とのやり取りと設定の保存は runtime.rs。

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::images::{self, ImageInfo};
use super::manifest::{self, from_skin, guard, skin_error, Appearance, Manifest, OmittedWarnings, Role, Warning};
use super::names::{self, shown_bytes, shown_name, PathKind};
use super::{limits, Dirs};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::settings::zip::{self, ZipFault};

/// derived\info.json の1件：確かめた画像の情報と、動く画像の最初の1コマ（`derived/…png`）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssetInfo {
    #[serde(flatten)]
    pub image: ImageInfo,
    pub still: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InfoFile {
    version: u32,
    assets: BTreeMap<String, AssetInfo>,
}

const INFO_VERSION: u32 = 1;
const INFO_FILE: &str = "derived/info.json";

/// 画面へ渡す画像。パスは skins の下の、アプリが確かめた場所だけ（画面はこれを asset の URL にするだけ）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssetRef {
    pub path: String,
    /// 動く画像の最初の1コマ（「動きを減らす」のときに使う）。静止画は None
    pub still: Option<String>,
    pub width: u32,
    pub height: u32,
    pub animated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThemedAsset {
    pub dark: Option<AssetRef>,
    pub light: Option<AssetRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CharacterAssets {
    pub playing: AssetRef,
    pub stopped: AssetRef,
    pub width_px: u32,
    pub max_height_px: u32,
    pub min_window_width_px: u32,
}

/// 画面が使う形にしたスキン
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSkin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub appearance: Option<Appearance>,
    pub icons: BTreeMap<String, BTreeMap<String, ThemedAsset>>,
    pub has_application: bool,
    pub character: Option<CharacterAssets>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Parts {
    pub appearance: bool,
    /// 差し替えるアイコンの数（34 のうち。v3.5.0 までは 33）
    pub icons: usize,
    pub application: bool,
    pub character: bool,
    /// 動く画像があるか
    pub animated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkinSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub builtin: bool,
    pub parts: Parts,
    /// 一覧の見本（静止画のパス）
    pub preview: Option<String>,
    /// 読めないときのエラーの種別（消すことだけできる）
    pub broken: Option<ErrorKind>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    pub path: String,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub duration_ms: u64,
    pub animated: bool,
}

/// 取り込みの確認画面に出すもの
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub token: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub min_app_version: Option<String>,
    pub parts: Parts,
    pub files: Vec<FileRow>,
    pub zip_bytes: u64,
    pub unpacked_bytes: u64,
    pub file_count: usize,
    pub max_files: usize,
    /// 種類ごとに `manifest::WARNINGS_PER_CODE` 件まで
    pub warnings: Vec<Warning>,
    /// 上の上限を超えて出さなかった数（種類ごと。画面は「ほか N 件」と出す）
    pub warnings_omitted: Vec<OmittedWarnings>,
    /// 飛ばした OS のファイル（種類ごとの数。名前そのものは出さない）
    pub ignored: Vec<IgnoredFiles>,
    /// フォルダごと zip にした形だったので、そのフォルダを外して読んだ
    pub unwrapped: bool,
    /// 同じ id のスキンが既に入っている（取り込めない）
    pub conflict: bool,
    /// 見本（data: URL の PNG。縦横 320px 以内）
    pub preview_data_url: Option<String>,
    /// 色の読みやすさの警告を画面で計算するため
    pub appearance: Option<Appearance>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IgnoredFiles {
    /// `__MACOSX/`・`.DS_Store`・`Thumbs.db`・`desktop.ini`
    pub name: String,
    pub count: usize,
}

/// 一時の場所に置いた、確認待ちのスキン
#[derive(Debug, Clone, PartialEq)]
pub struct Staged {
    pub token: String,
    pub id: String,
    pub dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub files: usize,
    pub bytes: u64,
}

/// 入っているスキン（読み込んで確かめたもの）
pub struct Installed {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub manifest_bytes: Vec<u8>,
    pub roles: BTreeMap<String, Role>,
    pub assets: BTreeMap<String, AssetInfo>,
}

/// zip の読み方（settings::zip。設定の書き出しと共有）の文には、zip の中の名前がそのまま入る。ここで `skin_error` に通す
fn fault(f: ZipFault) -> AppError {
    match f {
        ZipFault::Invalid(s) => skin_error(ErrorKind::SkinZipInvalid, s),
        ZipFault::Encrypted(s) => skin_error(ErrorKind::SkinZipEncrypted, s),
        ZipFault::TooLarge(s) => skin_error(ErrorKind::SkinTooLarge, s),
    }
}

fn too_large(s: impl AsRef<str>) -> AppError {
    skin_error(ErrorKind::SkinTooLarge, s)
}

/// ファイルを上限＋1 バイトまでだけ読む
pub(crate) fn read_capped(path: &Path, max: u64) -> AppResult<Vec<u8>> {
    let f = fs::File::open(path).map_err(|e| AppError::from_io(&e, path))?;
    let mut v = Vec::new();
    f.take(max + 1).read_to_end(&mut v).map_err(|e| AppError::from_io(&e, path))?;
    if v.len() as u64 > max {
        return Err(too_large(format!("{}: over {max} bytes", path.display())));
    }
    Ok(v)
}

/// 再解析点（シンボリックリンク・ジャンクション）か
pub(crate) fn is_reparse(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0 || meta.file_type().is_symlink()
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

/// スキンのフォルダの中のファイル。途中の段も含めて、リンクを辿らない（再解析点があれば断る）
pub(crate) fn safe_file(dir: &Path, rel: &str) -> AppResult<PathBuf> {
    let mut p = dir.to_path_buf();
    let segs: Vec<&str> = rel.split('/').collect();
    for (i, s) in segs.iter().enumerate() {
        p.push(s);
        let meta = fs::symlink_metadata(&p).map_err(|_| skin_error(ErrorKind::SkinAssetMissing, shown_name(rel)))?;
        if is_reparse(&meta) {
            return Err(skin_error(ErrorKind::SkinPathUnsafe, format!("link inside the skin: {}", shown_name(rel))));
        }
        let last = i + 1 == segs.len();
        if (last && !meta.is_file()) || (!last && !meta.is_dir()) {
            return Err(skin_error(ErrorKind::SkinAssetMissing, shown_name(rel)));
        }
    }
    Ok(p)
}

/// スキンのフォルダ（skins\<id>）。リンクなら断る
fn skin_dir(dirs: &Dirs, id: &str) -> AppResult<PathBuf> {
    if !names::valid_id(id) {
        return Err(skin_error(ErrorKind::SkinNotFound, from_skin(id)));
    }
    let dir = dirs.skins.join(id);
    let meta = fs::symlink_metadata(&dir).map_err(|_| skin_error(ErrorKind::SkinNotFound, from_skin(id)))?;
    if is_reparse(&meta) {
        return Err(skin_error(ErrorKind::SkinPathUnsafe, format!("skin folder is a link: {}", from_skin(id))));
    }
    if !meta.is_dir() {
        return Err(skin_error(ErrorKind::SkinNotFound, from_skin(id)));
    }
    Ok(dir)
}

fn derived_name(rel: &str) -> String {
    format!("derived/{:016x}.png", crate::library::artwork::fnv1a64(rel.as_bytes()))
}

fn parts_of(m: &Manifest, assets: &BTreeMap<String, AssetInfo>) -> Parts {
    Parts {
        appearance: m.appearance.is_some(),
        icons: m.icons.len(),
        application: m.application.as_ref().map(|a| a.window.is_some() || a.tray.is_some()).unwrap_or(false),
        character: m.character.is_some(),
        animated: assets.values().any(|a| a.image.animated),
    }
}

pub(crate) fn new_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let seed = format!("{nanos}-{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed));
    format!("{:016x}", crate::library::artwork::fnv1a64(seed.as_bytes()))
}

/// 確認の印（token）の形：16 桁の英小文字と数字
pub fn valid_token(t: &str) -> bool {
    t.len() == 16 && t.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// zip のファイルを読んで確かめ、一時の場所に置く（正式の場所には触らない）。
/// 読めないときの文には zip のファイルのパスが入る（ダウンロードしたファイルの名前に U+202E などが入りうる）ので、これも `guard` に通す
pub fn inspect_zip(zip_path: &Path, dirs: &Dirs, app_version: &str) -> AppResult<(Inspection, Staged)> {
    let bytes = guard(read_capped(zip_path, limits::MAX_ZIP_BYTES)).map_err(|e| {
        if e.kind == ErrorKind::SkinTooLarge {
            too_large(format!("zip is larger than {} bytes", limits::MAX_ZIP_BYTES))
        } else {
            e
        }
    })?;
    inspect_bytes(&bytes, dirs, app_version)
}

struct FileEntry {
    name: String,
    kind: PathKind,
    data: Vec<u8>,
}

/// zip のバイト列を確かめる（取り込みの本体。テストからも呼ぶ）。どこで作られたエラーでも、`detail` は `manifest::guard` に通して返す
pub fn inspect_bytes(bytes: &[u8], dirs: &Dirs, app_version: &str) -> AppResult<(Inspection, Staged)> {
    guard(inspect_bytes_unguarded(bytes, dirs, app_version))
}

/// 網（`guard`）を通さない本体。テストは、作る箇所ごとに `shown` を通していること（網に頼っていないこと）をこちらで確かめる
pub(crate) fn inspect_bytes_unguarded(bytes: &[u8], dirs: &Dirs, app_version: &str) -> AppResult<(Inspection, Staged)> {
    if bytes.len() as u64 > limits::MAX_ZIP_BYTES {
        return Err(too_large(format!("zip is larger than {} bytes", limits::MAX_ZIP_BYTES)));
    }
    let all = zip::scan(bytes, limits::MAX_SCANNED_ENTRIES).map_err(fault)?;
    // 0. 区切りの `\` を `/` に読み替える（PowerShell 5.1 の zip）。ここから先は全部、読み替えた名前で見る
    let all: Vec<(Vec<u8>, &zip::ScanEntry)> = all.iter().map(|e| (names::zip_name(&e.raw_name, e.utf8_name).into_owned(), e)).collect();
    // OS が足すファイルは飛ばす（戻さない・書かない。種類と数だけ確認の画面に出す）
    let mut ignored: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut entries = Vec::with_capacity(all.len());
    for (n, e) in &all {
        match names::os_junk(n) {
            Some(j) => *ignored.entry(j).or_default() += 1,
            None => entries.push((n.as_slice(), *e)),
        }
    }
    let link_check = |n: &[u8], e: &zip::ScanEntry| -> AppResult<()> {
        if names::is_link_or_special(e.made_by, e.external_attr) {
            return Err(skin_error(ErrorKind::SkinPathUnsafe, format!("link or special file: {}", shown_bytes(n))));
        }
        Ok(())
    };
    // フォルダごと zip にした形なら、そのフォルダを外す（フォルダの項目そのものも数えない。ただし中身があれば・リンクなら断る）
    let raw: Vec<&[u8]> = entries.iter().map(|(n, _)| *n).collect();
    let strip = names::wrapper_len(&raw).unwrap_or(0);
    if strip > 0 {
        for (n, e) in entries.iter().filter(|(n, _)| n.len() <= strip) {
            link_check(n, e)?;
            if e.uncompressed != 0 {
                return Err(skin_error(ErrorKind::SkinZipInvalid, format!("folder entry with data: {}", shown_bytes(n))));
            }
        }
        entries.retain(|(n, _)| n.len() > strip);
    }
    if entries.len() > limits::MAX_ENTRIES {
        return Err(too_large(format!("too many entries ({} > {})", entries.len(), limits::MAX_ENTRIES)));
    }
    // フォルダごと zip にしたつもりで、フォルダの外にも項目がある形なら、「知らないフォルダ」ではなく
    // 「全部が1つのフォルダの下にない」と知らせて断る（直し方が分かるように）。
    // ただし危ない名前・リンク・実行ファイルなどがあれば、先にそちらで断る（フォルダの中の名前は、外したとしての名前で見る）
    if strip == 0 {
        if let Some((folder, out)) = names::outside_wrapper(&raw) {
            for (n, e) in &entries {
                link_check(n, e)?;
                if *n == folder {
                    continue; // フォルダの項目そのもの（名前は ASCII でなくてもよい）
                }
                let as_unwrapped = n.strip_prefix(folder).unwrap_or(n);
                if let Err(err) = names::classify(as_unwrapped) {
                    if err.kind != ErrorKind::SkinFileNotAllowed {
                        return Err(err);
                    }
                }
            }
            // フォルダの名前は ASCII でなくてもよい（向きを変える文字なども入りうる）。どちらの名前も `shown_bytes` に通す
            return Err(skin_error(
                ErrorKind::SkinFileNotAllowed,
                format!("not all files are inside one folder: skin.json is in {}, but {} is outside it", shown_bytes(folder), shown_bytes(out)),
            ));
        }
    }
    // 1. 名前と大きさ（中身を戻す前）
    let mut seen = HashSet::new();
    let mut picked = Vec::new();
    let mut declared: u64 = 0;
    let mut file_count = 0usize;
    for (n, e) in entries {
        link_check(n, e)?;
        let (name, kind) = names::classify(&n[strip..])?;
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(skin_error(ErrorKind::SkinPathUnsafe, format!("duplicate name (ignoring case): {}", shown_name(&name))));
        }
        if kind == PathKind::Dir {
            if e.uncompressed != 0 {
                return Err(skin_error(ErrorKind::SkinZipInvalid, format!("folder entry with data: {}", shown_name(&name))));
            }
            continue;
        }
        file_count += 1;
        if file_count > limits::MAX_FILES {
            return Err(too_large(format!("more than {} files", limits::MAX_FILES)));
        }
        let cap = if kind == PathKind::Manifest { manifest::MAX_MANIFEST_BYTES } else { images::MAX_IMAGE_BYTES };
        if e.uncompressed > cap {
            return Err(too_large(format!("{}: {} bytes (max {cap})", shown_name(&name), e.uncompressed)));
        }
        if e.uncompressed > limits::RATIO_CHECK_FROM && e.uncompressed > e.compressed.saturating_mul(limits::MAX_RATIO) {
            return Err(too_large(format!("{}: compression ratio over {}:1", shown_name(&name), limits::MAX_RATIO)));
        }
        declared += e.uncompressed;
        if declared > limits::MAX_UNPACKED_BYTES {
            return Err(too_large(format!("unpacked total over {} bytes", limits::MAX_UNPACKED_BYTES)));
        }
        picked.push((name, kind, e));
    }
    // 2. 中身を戻す（実際に戻したバイト数でも数える）
    let mut files: Vec<FileEntry> = Vec::new();
    let mut unpacked: u64 = 0;
    for (name, kind, e) in picked {
        let data = zip::extract(bytes, e, limits::MAX_UNPACKED_BYTES - unpacked).map_err(fault)?;
        unpacked += data.len() as u64;
        files.push(FileEntry { name, kind, data });
    }
    // 3. skin.json
    let mf = files
        .iter()
        .find(|f| f.kind == PathKind::Manifest)
        .ok_or_else(|| skin_error(ErrorKind::SkinManifestInvalid, "skin.json is missing (put it at the top of the zip)"))?;
    let parsed = manifest::parse(&mf.data, app_version)?;
    let manifest_bytes = mf.data.clone();
    // 4. 使う画像が揃っているか・使われていないファイルが無いか
    let by_name: BTreeMap<&str, &FileEntry> = files.iter().filter(|f| f.kind == PathKind::Image).map(|f| (f.name.as_str(), f)).collect();
    for p in parsed.assets.keys() {
        if !by_name.contains_key(p.as_str()) {
            let hint = if by_name.keys().any(|k| k.eq_ignore_ascii_case(p)) { " (differs only in case)" } else { "" };
            return Err(skin_error(ErrorKind::SkinAssetMissing, format!("{}{hint}", shown_name(p))));
        }
    }
    for n in by_name.keys() {
        if !parsed.assets.contains_key(*n) {
            return Err(skin_error(ErrorKind::SkinFileNotAllowed, format!("not used by skin.json: {}", shown_name(n))));
        }
    }
    // 5. 画像を全部確かめる（1枚ずつ。復号した画素の合計にも上限）
    let mut checked: BTreeMap<String, (images::Checked, u64)> = BTreeMap::new();
    let mut total_pixels: u64 = 0;
    for (p, role) in &parsed.assets {
        let f = by_name[p.as_str()];
        let c = images::check(&f.data, p)?;
        if *role == Role::AppIcon && c.info.animated {
            return Err(skin_error(ErrorKind::SkinImageInvalid, format!("{}: application and tray icons must be still images", shown_name(p))));
        }
        total_pixels += c.info.decoded_pixels;
        if total_pixels > limits::MAX_TOTAL_PIXELS {
            return Err(skin_error(ErrorKind::SkinImageLimit, format!("images total more than {} pixels", limits::MAX_TOTAL_PIXELS)));
        }
        checked.insert(p.clone(), (c, f.data.len() as u64));
    }
    // 6. 一時の場所に書く
    let token = new_token();
    fs::create_dir_all(&dirs.staging).map_err(|e| AppError::from_io(&e, &dirs.staging))?;
    let dir = dirs.staging.join(&token);
    fs::create_dir(&dir).map_err(|e| AppError::from_io(&e, &dir))?;
    let staged = Staged { token: token.clone(), id: parsed.manifest.id.clone(), dir: dir.clone() };
    let write_all = || -> AppResult<BTreeMap<String, AssetInfo>> {
        write_new(&dir, "skin.json", &manifest_bytes)?;
        let mut infos = BTreeMap::new();
        for (p, (c, _)) in &checked {
            write_new(&dir, p, &by_name[p.as_str()].data)?;
            let still = match &c.first_frame_png {
                Some(png) => {
                    let n = derived_name(p);
                    write_new(&dir, &n, png)?;
                    Some(n)
                }
                None => None,
            };
            infos.insert(p.clone(), AssetInfo { image: c.info.clone(), still });
        }
        let info = InfoFile { version: INFO_VERSION, assets: infos.clone() };
        write_new(&dir, INFO_FILE, &serde_json::to_vec_pretty(&info)?)?;
        Ok(infos)
    };
    let infos = match write_all() {
        Ok(i) => i,
        Err(e) => {
            let _ = fs::remove_dir_all(&dir);
            return Err(e);
        }
    };
    let m = &parsed.manifest;
    let conflict = fs::symlink_metadata(dirs.skins.join(&m.id)).is_ok();
    // 見本：preview → 立ち絵（停止中・再生中）→ ウィンドウのアイコン → 最初のアイコン
    let mut sources: Vec<&str> = vec![];
    if let Some(p) = &m.preview {
        sources.push(p);
    }
    if let Some(c) = &m.character {
        sources.push(&c.stopped);
        sources.push(&c.playing);
    }
    if let Some(w) = m.application.as_ref().and_then(|a| a.window.as_deref()) {
        sources.push(w);
    }
    if let Some(first) = m.icons.values().flat_map(|s| s.values()).find_map(|t| t.dark.as_deref().or(t.light.as_deref())) {
        sources.push(first);
    }
    let preview_data_url = sources.iter().find_map(|p| {
        let (c, _) = checked.get(*p)?;
        match &c.first_frame_png {
            Some(png) => images::preview_data_url(png, "first.png", 320),
            None => images::preview_data_url(&by_name[*p].data, p, 320),
        }
    });
    let mut rows: Vec<FileRow> = vec![FileRow {
        path: "skin.json".into(),
        bytes: manifest_bytes.len() as u64,
        width: 0,
        height: 0,
        frames: 0,
        duration_ms: 0,
        animated: false,
    }];
    for (p, (c, size)) in &checked {
        rows.push(FileRow {
            path: p.clone(),
            bytes: *size,
            width: c.info.width,
            height: c.info.height,
            frames: c.info.frames,
            duration_ms: c.info.duration_ms,
            animated: c.info.animated,
        });
    }
    let insp = Inspection {
        token,
        id: m.id.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        author: m.author.clone(),
        description: m.description.clone(),
        min_app_version: m.min_app_version.clone(),
        parts: parts_of(m, &infos),
        files: rows,
        zip_bytes: bytes.len() as u64,
        unpacked_bytes: unpacked,
        file_count,
        max_files: limits::MAX_FILES,
        warnings: parsed.warnings.clone(),
        warnings_omitted: parsed.warnings_omitted.clone(),
        ignored: ignored.into_iter().map(|(name, count)| IgnoredFiles { name: name.to_string(), count }).collect(),
        unwrapped: strip > 0,
        conflict,
        preview_data_url,
        appearance: m.appearance.clone(),
    };
    Ok((insp, staged))
}

/// 一時の場所の中に、新しいファイルとして書く（既にあれば断る。途中のフォルダは作る）
pub(crate) fn write_new(dir: &Path, rel: &str, data: &[u8]) -> AppResult<()> {
    let mut p = dir.to_path_buf();
    for s in rel.split('/') {
        p.push(s);
    }
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::from_io(&e, parent))?;
    }
    let mut f = fs::OpenOptions::new().write(true).create_new(true).open(&p).map_err(|e| AppError::from_io(&e, &p))?;
    f.write_all(data).map_err(|e| AppError::from_io(&e, &p))?;
    Ok(())
}

/// 確認が済んだスキンを正式の場所（skins\<id>）へ移す。同じ id があれば上書きしない
pub fn commit(staged: &Staged, dirs: &Dirs, app_version: &str) -> AppResult<SkinSummary> {
    if !staged.dir.is_dir() {
        return Err(skin_error(ErrorKind::SkinImportExpired, from_skin(&staged.token)));
    }
    let target = dirs.skins.join(&staged.id);
    if fs::symlink_metadata(&target).is_ok() {
        return Err(skin_error(ErrorKind::SkinIdExists, from_skin(&staged.id)));
    }
    fs::create_dir_all(&dirs.skins).map_err(|e| AppError::from_io(&e, &dirs.skins))?;
    fs::rename(&staged.dir, &target).map_err(|e| AppError::from_io(&e, &target))?;
    match load(dirs, &staged.id, app_version) {
        Ok(inst) => Ok(summary(&inst)),
        Err(e) => {
            // 読めないものを正式の場所に残さない（target は上で「無い」ことを確かめてから、ここで作ったもの）
            tracing::warn!("skin {} could not be loaded after import: {:?}", staged.id, e.kind);
            if fs::rename(&target, &staged.dir).is_err() {
                let _ = fs::remove_dir_all(&target);
            }
            Err(e)
        }
    }
}

/// 一時の場所を消す
pub fn discard(staged: &Staged) {
    let _ = fs::remove_dir_all(&staged.dir);
}

/// 一時の場所を全部消す（起動のとき。前に取り込みの途中で終わったものの残り）
pub fn clean_staging(dirs: &Dirs) {
    if fs::symlink_metadata(&dirs.staging).map(|m| !is_reparse(&m)).unwrap_or(false) {
        let _ = fs::remove_dir_all(&dirs.staging);
    }
}

/// 入っているスキンを読み、確かめる（起動のとき・適用のとき）。画像は復号せず、在ること・大きさ・頭のバイトを見る
/// （中身は取り込みのときに全部確かめてある）
/// どこで作られたエラーでも、`detail` は `manifest::guard` に通して返す
pub fn load(dirs: &Dirs, id: &str, app_version: &str) -> AppResult<Installed> {
    guard(load_unguarded(dirs, id, app_version))
}

fn load_unguarded(dirs: &Dirs, id: &str, app_version: &str) -> AppResult<Installed> {
    let dir = skin_dir(dirs, id)?;
    // ここから先の id は valid_id を通ったもの（英小文字・数字・-）。それでも detail に入れるときは from_skin に通す
    let sid = from_skin(id);
    let mpath = safe_file(&dir, "skin.json").map_err(|e| skin_error(ErrorKind::SkinApplyFailed, format!("{sid}: {}", e.detail)))?;
    let manifest_bytes = read_capped(&mpath, manifest::MAX_MANIFEST_BYTES)?;
    let parsed = manifest::parse(&manifest_bytes, app_version)?;
    if parsed.manifest.id != id {
        return Err(skin_error(ErrorKind::SkinManifestInvalid, format!("id {} does not match the folder {sid}", from_skin(&parsed.manifest.id))));
    }
    let broken = |why: String| skin_error(ErrorKind::SkinApplyFailed, format!("{sid}: {why}"));
    let ipath = safe_file(&dir, INFO_FILE).map_err(|_| broken("derived/info.json is missing".into()))?;
    let info: InfoFile = serde_json::from_slice(&read_capped(&ipath, 1024 * 1024)?).map_err(|e| broken(format!("derived/info.json: {e}")))?;
    if info.version != INFO_VERSION {
        return Err(broken("derived/info.json version".into()));
    }
    let mut assets = BTreeMap::new();
    for (p, role) in &parsed.assets {
        let a = info.assets.get(p).ok_or_else(|| broken(format!("no info for {}", shown_name(p))))?;
        let f = safe_file(&dir, p)?;
        let len = fs::metadata(&f).map(|m| m.len()).unwrap_or(u64::MAX);
        if len > images::MAX_IMAGE_BYTES {
            return Err(too_large(shown_name(p)));
        }
        let mut head = [0u8; 32];
        let n = fs::File::open(&f).and_then(|mut h| h.read(&mut head)).map_err(|e| AppError::from_io(&e, &f))?;
        if !images::signature_matches(p, &head[..n]) {
            return Err(skin_error(ErrorKind::SkinImageInvalid, format!("{}: content does not match the extension", shown_name(p))));
        }
        if *role == Role::AppIcon && a.image.animated {
            return Err(skin_error(ErrorKind::SkinImageInvalid, format!("{}: application icons must be still images", shown_name(p))));
        }
        if a.image.animated {
            let s = a.still.as_deref().filter(|s| *s == derived_name(p)).ok_or_else(|| broken(format!("no still frame for {}", shown_name(p))))?;
            safe_file(&dir, s)?;
        }
        assets.insert(p.clone(), a.clone());
    }
    Ok(Installed { dir, manifest: parsed.manifest, manifest_bytes, roles: parsed.assets, assets })
}

pub(crate) fn abs(dir: &Path, rel: &str) -> String {
    let mut p = dir.to_path_buf();
    for s in rel.split('/') {
        p.push(s);
    }
    p.to_string_lossy().into_owned()
}

impl Installed {
    /// zip の中の名前 → 画面へ渡す画像
    pub fn asset_ref(&self, rel: &str) -> Option<AssetRef> {
        let a = self.assets.get(rel)?;
        Some(AssetRef {
            path: abs(&self.dir, rel),
            still: a.still.as_deref().map(|s| abs(&self.dir, s)),
            width: a.image.width,
            height: a.image.height,
            animated: a.image.animated,
        })
    }

    /// 静止画のパス（動く画像なら最初の1コマ）
    pub fn still_path(&self, rel: &str) -> Option<String> {
        self.asset_ref(rel).map(|r| r.still.unwrap_or(r.path))
    }

    /// アプリ・トレイのアイコンにする画像のファイル（トレイが無ければウィンドウの画像）
    pub fn app_icon_file(&self, tray: bool) -> Option<(PathBuf, String)> {
        let a = self.manifest.application.as_ref()?;
        let rel = if tray { a.tray.as_ref().or(a.window.as_ref()) } else { a.window.as_ref() }?;
        Some((PathBuf::from(abs(&self.dir, rel)), rel.clone()))
    }

    pub fn resolve(&self) -> ResolvedSkin {
        let m = &self.manifest;
        let icons = m
            .icons
            .iter()
            .map(|(name, states)| {
                let s = states
                    .iter()
                    .map(|(st, t)| {
                        let ta = ThemedAsset {
                            dark: t.dark.as_deref().and_then(|p| self.asset_ref(p)),
                            light: t.light.as_deref().and_then(|p| self.asset_ref(p)),
                        };
                        (st.clone(), ta)
                    })
                    .collect();
                (name.clone(), s)
            })
            .collect();
        let character = m.character.as_ref().and_then(|c| {
            Some(CharacterAssets {
                playing: self.asset_ref(&c.playing)?,
                stopped: self.asset_ref(&c.stopped)?,
                width_px: c.width_px,
                max_height_px: c.max_height_px,
                min_window_width_px: c.min_window_width_px,
            })
        });
        ResolvedSkin {
            id: m.id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            author: m.author.clone(),
            appearance: m.appearance.clone(),
            icons,
            has_application: parts_of(m, &self.assets).application,
            character,
        }
    }
}

pub fn summary(inst: &Installed) -> SkinSummary {
    let m = &inst.manifest;
    let preview = m
        .preview
        .as_deref()
        .or(m.character.as_ref().map(|c| c.stopped.as_str()))
        .and_then(|p| inst.still_path(p));
    SkinSummary {
        id: m.id.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        author: m.author.clone(),
        description: m.description.clone(),
        builtin: false,
        parts: parts_of(m, &inst.assets),
        preview,
        broken: None,
    }
}

/// 組み込みの既定のスキン
pub fn builtin_summary() -> SkinSummary {
    SkinSummary {
        id: names::BUILTIN_ID.into(),
        name: String::new(),
        version: String::new(),
        author: String::new(),
        description: String::new(),
        builtin: true,
        parts: Parts::default(),
        preview: None,
        broken: None,
    }
}

/// 一覧（既定のスキンが先頭。読めないスキンも、消せるように出す）
pub fn list(dirs: &Dirs, app_version: &str) -> Vec<SkinSummary> {
    let mut out = vec![];
    if let Ok(rd) = fs::read_dir(&dirs.skins) {
        for e in rd.flatten() {
            let Some(id) = e.file_name().to_str().map(|s| s.to_string()) else { continue };
            if !names::valid_id(&id) {
                continue;
            }
            match load(dirs, &id, app_version) {
                Ok(inst) => out.push(summary(&inst)),
                Err(err) => {
                    tracing::warn!("skin {id} is broken: {:?}", err.kind);
                    out.push(SkinSummary { id: id.clone(), name: id, broken: Some(err.kind), ..builtin_summary() }.not_builtin());
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
    out.insert(0, builtin_summary());
    out
}

impl SkinSummary {
    fn not_builtin(mut self) -> Self {
        self.builtin = false;
        self
    }
}

/// 入っているスキンを消す（既定のスキンは消せない）
pub fn delete(dirs: &Dirs, id: &str) -> AppResult<()> {
    if id == names::BUILTIN_ID {
        return Err(skin_error(ErrorKind::SkinBuiltinReadonly, from_skin(id)));
    }
    let dir = skin_dir(dirs, id)?;
    fs::remove_dir_all(&dir).map_err(|e| AppError::from_io(&e, &dir))
}

/// 入っているスキンを zip にする（skin.json と、それが使う画像だけ。アプリが作った derived\ は入れない）
pub fn export(dirs: &Dirs, id: &str, dest: &Path, app_version: &str) -> AppResult<ExportReport> {
    if id == names::BUILTIN_ID {
        return Err(skin_error(ErrorKind::SkinBuiltinReadonly, from_skin(id)));
    }
    let inst = load(dirs, id, app_version)?;
    let mut entries = vec![zip::Entry { name: "skin.json".into(), data: inst.manifest_bytes.clone() }];
    for rel in inst.roles.keys() {
        let f = safe_file(&inst.dir, rel)?;
        entries.push(zip::Entry { name: rel.clone(), data: read_capped(&f, images::MAX_IMAGE_BYTES)? });
    }
    let bytes: u64 = entries.iter().map(|e| e.data.len() as u64).sum();
    let mut f = fs::File::create(dest).map_err(|e| AppError::from_io(&e, dest))?;
    zip::write(&mut f, &entries).map_err(|e| AppError::from_io(&e, dest))?;
    Ok(ExportReport { files: entries.len(), bytes })
}
