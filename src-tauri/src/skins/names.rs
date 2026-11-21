//! スキンの id と、zip の中・skin.json の中のパスの決まり（v3.2.0）。
//! 文字だけで決める（ファイルには触らない）。決まりの全文は docs\着せ替え_仕様.md 2章と 10章。

use std::borrow::Cow;

use super::manifest::{shown, skin_error};
use crate::error::{AppError, ErrorKind};
use crate::settings::store::is_reserved_device_name;

/// 組み込みの既定のスキン（今の見た目そのもの。消せない・書き出せない）
pub const BUILTIN_ID: &str = "builtin:default";

/// 画像を置ける上の段のフォルダ
pub const TOP_DIRS: [&str; 3] = ["icons", "app", "character"];

/// 受け付ける画像の拡張子（APNG は .png、動く WebP は .webp）
pub const IMAGE_EXTS: [&str; 3] = ["png", "webp", "gif"];

/// パスの段の数の上限（`icons/IconStar/filled.png` が 3）
pub const MAX_DEPTH: usize = 3;

/// 1つの段の長さの上限
const MAX_SEGMENT: usize = 64;

/// エラーの `detail` に入れる zip の中の名前の長さの上限（置き換えたあとの文字数）。
/// 正しい名前のいちばん長いもの（64 文字の段が3つと区切り、末尾の `/`）が切れない長さ
pub const NAME_SHOWN_MAX: usize = MAX_DEPTH * (MAX_SEGMENT + 1);

/// zip の中の名前・skin.json の中のパスを、エラーの `detail` に入れる形にする（`manifest::shown`。見えない文字・制御文字は `[U+XXXX]`）
pub fn shown_name(name: &str) -> String {
    shown(name, NAME_SHOWN_MAX)
}

/// zip の中の名前（バイト列）を `shown_name` の形にする。UTF-8 として読めないバイトは U+FFFD にする
pub fn shown_bytes(raw: &[u8]) -> String {
    shown_name(&String::from_utf8_lossy(raw))
}

/// id：英小文字・数字・`-`、1〜48 文字、先頭は英小文字か数字。Windows の予約デバイス名（`con` など）は使えない
pub fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    !b.is_empty()
        && b.len() <= 48
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        && !is_reserved_device_name(id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// `skin.json`
    Manifest,
    /// 画像（`preview.*` か、icons・app・character の下）
    Image,
    /// フォルダの項目（名前が `/` で終わるもの）
    Dir,
}

fn unsafe_path(name: &str, why: &str) -> AppError {
    skin_error(ErrorKind::SkinPathUnsafe, format!("{why}: {}", shown_name(name)))
}

fn not_allowed(name: &str, why: &str) -> AppError {
    skin_error(ErrorKind::SkinFileNotAllowed, format!("{why}: {}", shown_name(name)))
}

/// 拡張子だけで断るもの（場所より先に見る。何が引っかかったかを分けて知らせるため）
fn forbidden_ext(name: &str, ext: &str) -> Option<AppError> {
    let kind = match ext {
        "svg" | "svgz" => ErrorKind::SkinSvgNotAllowed,
        "css" | "scss" | "sass" | "less" => ErrorKind::SkinCssNotAllowed,
        "ttf" | "otf" | "ttc" | "woff" | "woff2" | "eot" | "fon" | "fnt" => ErrorKind::SkinFontNotAllowed,
        "exe" | "dll" | "msi" | "msp" | "bat" | "cmd" | "com" | "scr" | "pif" | "cpl" | "sys" | "drv" | "ocx" | "ps1" | "psm1"
        | "psd1" | "vbs" | "vbe" | "js" | "jse" | "mjs" | "cjs" | "ts" | "wsf" | "wsh" | "hta" | "wasm" | "html" | "htm"
        | "xhtml" | "jar" | "lnk" | "url" | "reg" | "inf" | "msc" | "appx" | "msix" | "sh" | "py" | "pyw" | "app" => {
            ErrorKind::SkinExecutable
        }
        _ => return None,
    };
    Some(skin_error(kind, shown_name(name)))
}

fn ext_of(seg: &str) -> String {
    seg.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

fn segment_ok(seg: &str) -> bool {
    let b = seg.as_bytes();
    !b.is_empty()
        && b.len() <= MAX_SEGMENT
        && b[0].is_ascii_alphanumeric()
        && b.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
        && !seg.ends_with('.')
        && !is_reserved_device_name(seg)
}

/// zip の中の名前の区切りをそろえる：`\` を `/` に読み替える。Windows 標準の PowerShell 5.1 の `Compress-Archive` と
/// .NET Framework の `ZipFile.CreateFromDirectory` は、区切りを `\` で書くため（zip の決まりでは `/`）。
/// 読み替えたあとの名前を、ほかと同じ決まり（`classify`・`os_junk`・`wrapper_len`・重複）に全部通す。
/// `icons\..\..\x` は `icons/../../x` になって親フォルダへ出る名前として断られ、`\\server\x` は `//server/x`（UNC）、
/// `C:\x` は `C:/x`（ドライブ）として断られる。
///
/// 読み替えるのは、名前が ASCII だけのときと、UTF-8 の印があるときだけ（UTF-8 では 0x5C は必ず `\` の1文字）。
/// 印の無い ASCII 以外の名前は Shift_JIS などのことがあり、0x5C が2バイトの文字の後ろ半分（「ソ」= 0x83 0x5C）のことがあるので、
/// 読み替えない（`\` が残れば `classify` で断る）。PowerShell 5.1 は ASCII 以外の名前に UTF-8 の印を付ける（2026-11-13 に実物で確かめた）
pub fn zip_name(raw: &[u8], utf8: bool) -> Cow<'_, [u8]> {
    if !raw.contains(&b'\\') || !(utf8 || raw.is_ascii()) {
        return Cow::Borrowed(raw);
    }
    Cow::Owned(raw.iter().map(|c| if *c == b'\\' { b'/' } else { *c }).collect())
}

/// zip の中の名前（区切りは `zip_name` でそろえたもの）を確かめて分類する。戻り値の名前は、フォルダなら末尾の `/` を外したもの。
/// skin.json の中に書かれたパスにも使う（こちらは読み替えない。`\` があれば断る）。
///
/// 断る順：NUL・ASCII 以外・制御文字 → `\` → 絶対パス（`/` で始まる。UNC の `//` も）→ ドライブ（`:`）→ `%`（URL の形で隠したもの）→
/// 空の段・`.`・`..` → 段の数 → 段の文字 → 拡張子（SVG・CSS・フォント・実行ファイル）→ 置き場所
pub fn classify(raw: &[u8]) -> Result<(String, PathKind), AppError> {
    let lossy = String::from_utf8_lossy(raw).into_owned();
    if raw.is_empty() {
        return Err(unsafe_path(&lossy, "empty name"));
    }
    if raw.contains(&0) {
        return Err(unsafe_path(&lossy, "NUL in name"));
    }
    if raw.iter().any(|c| !(0x20..0x7f).contains(c)) {
        return Err(unsafe_path(&lossy, "only ASCII letters, digits and . _ - are allowed"));
    }
    let name = lossy.as_str();
    if name.contains('\\') {
        return Err(unsafe_path(name, "backslash in name"));
    }
    if name.starts_with('/') {
        return Err(unsafe_path(name, "absolute path"));
    }
    if name.contains(':') {
        return Err(unsafe_path(name, "drive or stream in name"));
    }
    if name.contains('%') {
        return Err(unsafe_path(name, "encoded name"));
    }
    let (body, is_dir) = match name.strip_suffix('/') {
        Some(b) => (b, true),
        None => (name, false),
    };
    let segs: Vec<&str> = body.split('/').collect();
    if segs.iter().any(|s| *s == ".." || *s == ".") {
        return Err(unsafe_path(name, "parent or current folder in name"));
    }
    if segs.iter().any(|s| s.is_empty()) {
        return Err(unsafe_path(name, "empty segment"));
    }
    if segs.len() > MAX_DEPTH {
        return Err(unsafe_path(name, "too deep"));
    }
    if let Some(s) = segs.iter().find(|s| !segment_ok(s)) {
        return Err(unsafe_path(name, &format!("segment not allowed ({})", shown(s, MAX_SEGMENT))));
    }
    if !is_dir {
        if let Some(e) = forbidden_ext(name, &ext_of(segs[segs.len() - 1])) {
            return Err(e);
        }
    }
    let top = segs[0];
    if top == "derived" {
        return Err(not_allowed(name, "derived/ is made by the app"));
    }
    if segs.len() == 1 {
        if is_dir {
            return if TOP_DIRS.contains(&top) {
                Ok((body.to_string(), PathKind::Dir))
            } else {
                Err(not_allowed(name, "unknown folder"))
            };
        }
        if top == "skin.json" {
            return Ok((body.to_string(), PathKind::Manifest));
        }
        if let Some((stem, ext)) = top.rsplit_once('.') {
            if stem == "preview" && IMAGE_EXTS.contains(&ext.to_ascii_lowercase().as_str()) {
                return Ok((body.to_string(), PathKind::Image));
            }
        }
        return Err(not_allowed(name, "unknown file"));
    }
    if !TOP_DIRS.contains(&top) {
        return Err(not_allowed(name, "unknown folder"));
    }
    if is_dir {
        return Ok((body.to_string(), PathKind::Dir));
    }
    if !IMAGE_EXTS.contains(&ext_of(segs[segs.len() - 1]).as_str()) {
        return Err(not_allowed(name, "unknown file type"));
    }
    Ok((body.to_string(), PathKind::Image))
}

/// skin.json の中に書かれた画像のパスを確かめる（zip の中の名前と同じ決まりで、画像であること）
pub fn asset_path(s: &str) -> Result<String, AppError> {
    match classify(s.as_bytes())? {
        (n, PathKind::Image) => Ok(n),
        _ => Err(skin_error(ErrorKind::SkinManifestInvalid, format!("not an image path: {}", shown_name(s)))),
    }
}

/// OS が勝手に足すファイル（取り込みでは戻さずに飛ばし、確認の画面に種類と数だけ出す。理由は docs\着せ替え_仕様.md 10章）。
/// - `__MACOSX/` の下（macOS の「圧縮」が足す。中の名前は見ない）
/// - `.DS_Store`（macOS）・`Thumbs.db`・`desktop.ini`（Windows のエクスプローラー）。どの段にあっても。大文字小文字は区別しない
///
/// 戻り値は種類の名前（固定の文字列。zip の中の名前そのものは画面に出さない）。名前は `zip_name` でそろえたものを渡す
pub fn os_junk(raw: &[u8]) -> Option<&'static str> {
    if raw.starts_with(b"__MACOSX/") {
        return Some("__MACOSX/");
    }
    if raw.ends_with(b"/") {
        return None;
    }
    let last = raw.rsplit(|c| *c == b'/').next().unwrap_or(raw);
    [".DS_Store", "Thumbs.db", "desktop.ini"].into_iter().find(|j| last.eq_ignore_ascii_case(j.as_bytes()))
}

/// フォルダごと zip にした形（エクスプローラーの「送る → 圧縮（zip 形式）フォルダー」）なら、外すフォルダの名前の長さ（末尾の `/` 込み）。
///
/// 当てはまるのは、項目（OS が足すファイルは除いたもの）の全部が同じ1つのフォルダ `X/` の下にあり、`X/skin.json` があり、
/// 一番上に `skin.json` が無いときだけ。`X` の名前は使わない（捨てる）ので、ASCII でなくてもよい。外した後の名前は今までと同じ決まりで確かめる
pub fn wrapper_len(raw_names: &[&[u8]]) -> Option<usize> {
    let first = raw_names.first()?;
    let cut = first.iter().position(|c| *c == b'/')?;
    let prefix = &first[..=cut];
    if cut == 0 || prefix == b"./" || prefix == b"../" {
        return None;
    }
    if raw_names.iter().any(|n| *n == b"skin.json") {
        return None;
    }
    if !raw_names.iter().all(|n| n.starts_with(prefix)) {
        return None;
    }
    let mut manifest = prefix.to_vec();
    manifest.extend_from_slice(b"skin.json");
    raw_names.iter().any(|n| *n == manifest.as_slice()).then_some(prefix.len())
}

/// フォルダごと zip にしたつもりで、そのフォルダの外にも項目がある形か。当てはまれば（フォルダ `X/`, 外にある項目の1つ）を返す。
/// `wrapper_len` が外さなかったときに、「知らないフォルダ」ではなく「全部が1つのフォルダの下にない」と知らせるため。
///
/// 当てはまるのは、一番上に `skin.json` が無く、`X/skin.json`（1段だけ。`X` は空・`.`・`..` 以外）があり、`X/` の下に無い項目があるとき。
/// 名前は `zip_name` でそろえ、OS が足すファイルを除いたものを渡す
pub fn outside_wrapper<'a>(raw_names: &[&'a [u8]]) -> Option<(&'a [u8], &'a [u8])> {
    if raw_names.iter().any(|n| *n == b"skin.json") {
        return None;
    }
    let manifest = raw_names.iter().find(|n| {
        n.ends_with(b"/skin.json")
            && n.iter().filter(|c| **c == b'/').count() == 1
            && !matches!(&n[..n.len() - 10], b"" | b"." | b"..")
    })?;
    let prefix = &manifest[..manifest.len() - 9];
    let outside = raw_names.iter().find(|n| !n.starts_with(prefix))?;
    Some((prefix, outside))
}

/// zip の項目がリンク・特殊なファイルか（Unix のモードがふつうのファイルとフォルダ以外、または Windows の再解析点）
pub fn is_link_or_special(made_by: u16, external_attr: u32) -> bool {
    let host = made_by >> 8;
    if external_attr & 0x0400 != 0 {
        return true; // FILE_ATTRIBUTE_REPARSE_POINT（シンボリックリンク・ジャンクション）
    }
    if host == 3 || host == 19 {
        let mode = external_attr >> 16;
        let ftype = mode & 0o170000;
        if mode != 0 && ftype != 0o100000 && ftype != 0o040000 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        for ok in ["a", "quiet-paper", "skin-01", "0abc"] {
            assert!(valid_id(ok), "{ok}");
        }
        for ng in ["", "-a", "A", "a_b", "a.b", "a/b", "con", "nul", "com1", "builtin:default", &"a".repeat(49)] {
            assert!(!valid_id(ng), "{ng}");
        }
    }

    fn kind(n: &str) -> Result<PathKind, ErrorKind> {
        classify(n.as_bytes()).map(|(_, k)| k).map_err(|e| e.kind)
    }

    #[test]
    fn paths() {
        assert_eq!(kind("skin.json"), Ok(PathKind::Manifest));
        assert_eq!(kind("preview.png"), Ok(PathKind::Image));
        assert_eq!(kind("icons/"), Ok(PathKind::Dir));
        assert_eq!(kind("icons/IconStar/"), Ok(PathKind::Dir));
        assert_eq!(kind("icons/IconPlay.png"), Ok(PathKind::Image));
        assert_eq!(kind("icons/IconStar/filled.webp"), Ok(PathKind::Image));
        assert_eq!(kind("character/playing.GIF"), Ok(PathKind::Image));
        // 危ないパス
        for n in [
            "../../startup/evil.lnk",
            "icons/../../settings.json",
            "icons\\..\\..\\settings.json",
            "C:\\Windows\\System32\\x.dll",
            "C:/Windows/x.png",
            "/etc/passwd",
            "//server/share/x.png",
            "\\\\server\\share\\x",
            "icons/%2e%2e/x.png",
            "icons//x.png",
            "icons/./x.png",
            "icons/a b.png",
            "icons/a/b/c.png",
            "icons/con.png",
            "icons/x.png.",
            "icons/\u{3042}.png",
            "icons/x\u{0}.png",
        ] {
            assert_eq!(kind(n), Err(ErrorKind::SkinPathUnsafe), "{n}");
        }
        assert_eq!(kind("icons/x.svg"), Err(ErrorKind::SkinSvgNotAllowed));
        assert_eq!(kind("custom.css"), Err(ErrorKind::SkinCssNotAllowed));
        assert_eq!(kind("fonts/a.ttf"), Err(ErrorKind::SkinFontNotAllowed));
        assert_eq!(kind("evil.exe"), Err(ErrorKind::SkinExecutable));
        assert_eq!(kind("icons/run.js"), Err(ErrorKind::SkinExecutable));
        assert_eq!(kind("readme.txt"), Err(ErrorKind::SkinFileNotAllowed));
        assert_eq!(kind("fonts/"), Err(ErrorKind::SkinFileNotAllowed));
        assert_eq!(kind("derived/a.png"), Err(ErrorKind::SkinFileNotAllowed));
        assert_eq!(kind("icons/notes.txt"), Err(ErrorKind::SkinFileNotAllowed));
        assert_eq!(kind("Skin.json"), Err(ErrorKind::SkinFileNotAllowed));
        assert_eq!(kind("__MACOSX/"), Err(ErrorKind::SkinPathUnsafe));
        assert_eq!(asset_path("icons/a.png").unwrap(), "icons/a.png");
        assert!(asset_path("skin.json").is_err());
        assert!(asset_path("icons/").is_err());
    }

    /// Windows 標準の PowerShell 5.1 の zip は区切りが `\`。読み替えたあとも危ない名前は断る
    #[test]
    fn backslash_separators() {
        let z = |n: &[u8], utf8: bool| zip_name(n, utf8).into_owned();
        assert_eq!(z(b"app\\tray.png", false), b"app/tray.png");
        assert_eq!(z(b"icons\\IconStar\\filled.webp", false), b"icons/IconStar/filled.webp");
        assert_eq!(z(b"icons\\empty\\", false), b"icons/empty/");
        assert_eq!(z("スキン\\skin.json".as_bytes(), true), "スキン/skin.json".as_bytes());
        // 印の無い ASCII 以外（Shift_JIS の「ソフ」= 83 5C 83 74）は読み替えない
        assert_eq!(z(b"\x83\x5c\x83\x74\\skin.json", false), b"\x83\x5c\x83\x74\\skin.json");
        assert_eq!(z(b"icons/a.png", false), b"icons/a.png");
        let k = |n: &str| classify(&z(n.as_bytes(), false)).map_err(|e| e.kind);
        assert_eq!(k("app\\tray.png"), Ok(("app/tray.png".to_string(), PathKind::Image)));
        assert_eq!(k("icons\\"), Ok(("icons".to_string(), PathKind::Dir)));
        for n in [
            "icons\\..\\..\\x.png",
            "icons\\..\\..\\settings.json",
            "..\\..\\startup\\evil.lnk",
            "icons\\.\\x.png",
            "\\abs.png",
            "\\\\server\\share\\x.png",
            "\\\\?\\C:\\x.png",
            "C:\\Windows\\x.png",
            "icons\\\\x.png",
            "icons\\a\\b\\c.png",
            "icons/%2e%2e\\x.png",
            "icons\\x.png.",
            "icons\\con.png",
        ] {
            assert_eq!(k(n), Err(ErrorKind::SkinPathUnsafe), "{n}");
        }
        assert_eq!(k("fonts\\a.ttf"), Err(ErrorKind::SkinFontNotAllowed));
        assert_eq!(k("icons\\run.js"), Err(ErrorKind::SkinExecutable));
        assert_eq!(k("derived\\a.png"), Err(ErrorKind::SkinFileNotAllowed));
        // 読み替えなかった `\`（印の無い ASCII 以外）は今までどおり断る
        assert_eq!(classify(&z(b"\x83\x74\\skin.json", false)).map_err(|e| e.kind), Err(ErrorKind::SkinPathUnsafe));
        // skin.json の中のパスは読み替えない
        assert!(asset_path("icons\\a.png").is_err());
        // OS のファイル・フォルダごとの形も、読み替えたあとで見る
        assert_eq!(os_junk(&z(b"__MACOSX\\._skin.json", false)), Some("__MACOSX/"));
        assert_eq!(os_junk(&z(b"ex1\\Thumbs.db", false)), Some("Thumbs.db"));
        let names: Vec<Vec<u8>> = [&b"ex1\\skin.json"[..], b"ex1\\preview.png"].iter().map(|n| z(n, false)).collect();
        let refs: Vec<&[u8]> = names.iter().map(|n| n.as_slice()).collect();
        assert_eq!(wrapper_len(&refs), Some(4));
    }

    #[test]
    fn outside_the_wrapper_folder() {
        let o = |v: &[&[u8]]| outside_wrapper(v).map(|(p, n)| (p.to_vec(), n.to_vec()));
        assert_eq!(o(&[b"X/", b"X/skin.json", b"other/readme.txt"]), Some((b"X/".to_vec(), b"other/readme.txt".to_vec())));
        assert_eq!(o(&[b"X/skin.json", b"Y/skin.json"]), Some((b"X/".to_vec(), b"Y/skin.json".to_vec())));
        assert_eq!(o(&[b"X/skin.json", b"readme.txt"]), Some((b"X/".to_vec(), b"readme.txt".to_vec())));
        // 当てはまらない形（一番上に skin.json・全部が X/ の下・2段・親フォルダ・絶対パス）
        assert_eq!(o(&[b"skin.json", b"X/skin.json", b"a"]), None);
        assert_eq!(o(&[b"X/skin.json", b"X/a.png"]), None);
        assert_eq!(o(&[b"X/Y/skin.json", b"a"]), None);
        assert_eq!(o(&[b"../skin.json", b"a"]), None);
        assert_eq!(o(&[b"/skin.json", b"a"]), None);
    }

    #[test]
    fn os_junk_names() {
        assert_eq!(os_junk(b"__MACOSX/"), Some("__MACOSX/"));
        assert_eq!(os_junk(b"__MACOSX/MySkin/._skin.json"), Some("__MACOSX/"));
        assert_eq!(os_junk(b".DS_Store"), Some(".DS_Store"));
        assert_eq!(os_junk(b"icons/.DS_Store"), Some(".DS_Store"));
        assert_eq!(os_junk(b"MySkin/Thumbs.db"), Some("Thumbs.db"));
        assert_eq!(os_junk(b"thumbs.DB"), Some("Thumbs.db"));
        assert_eq!(os_junk(b"Desktop.ini"), Some("desktop.ini"));
        // 似た名前・フォルダ・中身のある名前は飛ばさない（今までどおり確かめて断る）
        for n in [&b"Thumbs.db.png"[..], b"xThumbs.db", b"Thumbs.db/", b"_MACOSX/x", b"skin.json", b"icons/a.png", b"a/__MACOSX/x"] {
            assert_eq!(os_junk(n), None, "{}", String::from_utf8_lossy(n));
        }
    }

    #[test]
    fn wrapper_folder() {
        let w = |v: &[&[u8]]| wrapper_len(v);
        // エクスプローラーでフォルダごと圧縮した形（フォルダの項目あり・なし）
        assert_eq!(w(&[b"MySkin/", b"MySkin/skin.json", b"MySkin/icons/a.png"]), Some(7));
        assert_eq!(w(&[b"MySkin/skin.json", b"MySkin/preview.png"]), Some(7));
        // フォルダの名前は使わないので、ASCII でなくてもよい（UTF-8・Shift_JIS のバイト列）
        assert_eq!(w(&["スキン/skin.json".as_bytes()]), Some("スキン/".len()));
        assert_eq!(w(&[b"\x83\x5c\x83\x74/skin.json"]), Some(5));
        // 当てはまらない形
        assert_eq!(w(&[b"skin.json", b"icons/a.png"]), None);
        assert_eq!(w(&[b"MySkin/skin.json", b"skin.json"]), None);
        assert_eq!(w(&[b"A/skin.json", b"B/icons/a.png"]), None);
        assert_eq!(w(&[b"A/icons/a.png", b"A/B/skin.json"]), None);
        assert_eq!(w(&[b"A/B/skin.json"]), None);
        assert_eq!(w(&[b"/skin.json"]), None);
        assert_eq!(w(&[b"../skin.json"]), None);
        assert_eq!(w(&[b"./skin.json"]), None);
        assert_eq!(w(&[]), None);
    }

    #[test]
    fn links() {
        // Unix のシンボリックリンク（made by = 3、モード 0o120777）
        assert!(is_link_or_special(3 << 8, 0o120777 << 16));
        // Unix のふつうのファイルとフォルダ
        assert!(!is_link_or_special(3 << 8, 0o100644 << 16));
        assert!(!is_link_or_special(3 << 8, (0o040755 << 16) | 0x10));
        // FIFO・デバイス
        assert!(is_link_or_special(3 << 8, 0o010644 << 16));
        assert!(is_link_or_special(3 << 8, 0o060644 << 16));
        // Windows の再解析点
        assert!(is_link_or_special(0, 0x0400));
        assert!(!is_link_or_special(0, 0x20));
    }
}
