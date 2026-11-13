//! 着せ替えの取り込み・読み込み・書き出し・起動のときの決め方のテスト（v3.2.0）。
//! 壊れた zip は仕様の表の例を全部（docs\着せ替え_仕様.md 10章）。画像は testkit がその場で作る図形だけ。

use std::fs;
use std::path::PathBuf;

use super::images;
use super::package::{self, Inspection, Staged};
use super::runtime::plan_startup;
use super::testkit::*;
use super::{limits, Dirs};
use crate::error::{AppResult, ErrorKind};
use crate::settings::store::SkinSettings;

const VER: &str = "3.2.0";

fn dirs(tag: &str) -> Dirs {
    let base: PathBuf = crate::test_tmp().join(format!("plp-skin-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    Dirs::new(&base)
}

fn manifest(id: &str, parts: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec_pretty(&serde_json::json!({
        "format": "pl-player-skin",
        "schemaVersion": 1,
        "id": id,
        "name": "Shape Sample",
        "version": "1.0.0",
        "author": "Test",
        "preview": "preview.png",
        "parts": parts,
        "extensions": {}
    }))
    .unwrap()
}

fn full_parts() -> serde_json::Value {
    serde_json::json!({
        "appearance": {"themes": {"dark": {"tokens": {"--c-accent": "#d08a55", "--c-accent-contrast": "#17130f"}}}},
        "icons": {"IconPlay": "icons/play.png", "IconStar": {"filled": "icons/IconStar/filled.webp", "outline": "icons/IconStar/outline.png"}},
        "application": {"window": "app/window.png", "tray": "app/tray.png"},
        "character": {"states": {"playing": "character/playing.gif", "stopped": "character/stopped.png"}, "placement": {"mode": "right-rail"}}
    })
}

/// 図形だけの見本のスキン（エクスプローラーで作った zip のように、項目は deflate）
fn good_entries(id: &str) -> Vec<Z> {
    vec![
        Z::file("skin.json", &manifest(id, full_parts())).deflated(),
        Z::file("preview.png", &png(&shape(64, 40, [40, 60, 120], false))).deflated(),
        Z::dir("icons/"),
        Z::file("icons/play.png", &png(&shape(96, 96, [230, 230, 240], true))).deflated(),
        Z::file("icons/IconStar/filled.webp", &webp(&shape(96, 96, [240, 200, 60], true))),
        Z::file("icons/IconStar/outline.png", &png(&shape(96, 96, [200, 200, 200], false))),
        Z::file("app/window.png", &png(&shape(256, 256, [124, 108, 255], true))).deflated(),
        Z::file("app/tray.png", &png(&shape(32, 32, [124, 108, 255], true))),
        Z::file("character/playing.gif", &gif_anim(&moving(72, 128, 6), 10)),
        Z::file("character/stopped.png", &png(&figure(72, 128, [120, 110, 200], 0))).deflated(),
    ]
}

fn inspect(d: &Dirs, entries: &[Z]) -> AppResult<(Inspection, Staged)> {
    package::inspect_bytes(&zip(entries), d, VER)
}

fn kind(d: &Dirs, entries: &[Z]) -> ErrorKind {
    match inspect(d, entries) {
        Ok(_) => panic!("accepted"),
        Err(e) => {
            // 断ったときは一時の場所に何も残さない・正式の場所にも何も作らない
            let left = fs::read_dir(&d.staging).map(|r| r.count()).unwrap_or(0);
            assert_eq!(left, 0, "staging not cleaned: {:?}", e);
            assert!(fs::read_dir(&d.skins).map(|r| r.count()).unwrap_or(0) == 0);
            e.kind
        }
    }
}

/// good_entries の1つを差し替える
fn replace(id: &str, name: &str, z: Z) -> Vec<Z> {
    let mut v: Vec<Z> = good_entries(id).into_iter().filter(|e| e.name != name.as_bytes()).collect();
    v.push(z);
    v
}

/// 項目を全部 `prefix` の下に入れる（フォルダごと zip にした形。フォルダの項目も先頭に足す）
fn wrap(prefix: &[u8], entries: Vec<Z>) -> Vec<Z> {
    let mut out = vec![Z::dir("x/")];
    out[0].name = prefix.to_vec();
    for mut z in entries {
        let mut n = prefix.to_vec();
        n.extend_from_slice(&z.name);
        z.name = n;
        out.push(z);
    }
    out
}

/// エクスプローラーの「送る → 圧縮（zip 形式）フォルダー」の形（フォルダごと・日本語のフォルダ名・Thumbs.db など）と、
/// macOS の「圧縮」の形（__MACOSX/ と .DS_Store）を取り込める。OS のファイルは戻さず、種類と数だけ知らせる
#[test]
fn folder_zips_from_explorer_and_macos() {
    let id = "shape-sample";
    // エクスプローラー：フォルダの名前は Shift_JIS（UTF-8 の印なし）。中の名前は ASCII
    let d = dirs("wrap-win");
    let sjis: &[u8] = b"\x83\x58\x83\x4c\x83\x93/"; // 「スキン/」
    let mut win = wrap(sjis, good_entries(id));
    win.push(Z::raw(&[sjis, b"Thumbs.db"].concat(), &[7u8; 300]));
    win.push(Z::raw(&[sjis, b"icons/desktop.ini"].concat(), b"[.ShellClassInfo]"));
    for z in win.iter_mut() {
        z.flags = 0;
        z.made_by = 0x0A3F; // NTFS（エクスプローラーの zip）
    }
    let (insp, staged) = inspect(&d, &win).unwrap();
    assert!(insp.unwrapped);
    assert_eq!(insp.file_count, 9);
    let ig: Vec<(String, usize)> = insp.ignored.iter().map(|g| (g.name.clone(), g.count)).collect();
    assert_eq!(ig, vec![("Thumbs.db".to_string(), 1), ("desktop.ini".to_string(), 1)]);
    // 飛ばしたファイルは一時の場所にも書かない
    assert!(!staged.dir.join("Thumbs.db").exists() && !staged.dir.join("icons").join("desktop.ini").exists());
    package::commit(&staged, &d, VER).unwrap();
    assert!(package::load(&d, id, VER).is_ok());

    // macOS：フォルダごと＋__MACOSX/ に項目ごとの写し＋.DS_Store。写しを足すと 80 項目を超えても取り込める
    let d = dirs("wrap-mac");
    let mut mac = wrap(b"MySkin/", good_entries(id));
    mac.push(Z::file("MySkin/.DS_Store", &[0u8; 64]));
    mac.push(Z::dir("__MACOSX/"));
    mac.push(Z::dir("__MACOSX/MySkin/"));
    let copies: Vec<Z> = mac.iter().filter(|z| z.name.starts_with(b"MySkin/") && !z.name.ends_with(b"/")).map(|z| {
        let mut n = b"__MACOSX/MySkin/._".to_vec();
        n.extend_from_slice(&z.name[7..]);
        Z::raw(&n, &[0u8; 82])
    }).collect();
    mac.extend(copies);
    for i in 0..70 {
        mac.push(Z::raw(format!("__MACOSX/MySkin/._extra{i}").as_bytes(), &[0u8; 82]));
    }
    assert!(mac.len() > limits::MAX_ENTRIES);
    let (insp, staged) = inspect(&d, &mac).unwrap();
    assert!(insp.unwrapped);
    let ig: Vec<(String, usize)> = insp.ignored.iter().map(|g| (g.name.clone(), g.count)).collect();
    assert_eq!(ig, vec![(".DS_Store".to_string(), 1), ("__MACOSX/".to_string(), 2 + 10 + 70)]);
    package::discard(&staged);

    // 一番上に置いた形（今までどおり）は、外さない
    let d = dirs("wrap-none");
    let (insp, staged) = inspect(&d, &good_entries(id)).unwrap();
    assert!(!insp.unwrapped && insp.ignored.is_empty());
    package::discard(&staged);
    // 全部で 256 項目を超える zip は、中央ディレクトリを読む前に断る
    let mut huge = good_entries(id);
    for i in 0..limits::MAX_SCANNED_ENTRIES {
        huge.push(Z::raw(format!("__MACOSX/._{i}").as_bytes(), &[]));
    }
    assert_eq!(kind(&d, &huge), ErrorKind::SkinTooLarge);
}

/// 区切りを `\` に、印を 0 にする（Windows 標準の PowerShell 5.1 の Compress-Archive・.NET Framework の ZipFile の形。
/// ASCII の名前には UTF-8 の印を付けない。フォルダは空のときだけ項目になる。2026-11-13 に実物で確かめた形）
fn powershell51(entries: Vec<Z>) -> Vec<Z> {
    entries
        .into_iter()
        .map(|mut z| {
            z.name = z.name.iter().map(|c| if *c == b'/' { b'\\' } else { *c }).collect();
            if z.name.is_ascii() {
                z.flags = 0;
            }
            z.made_by = 20;
            z
        })
        .collect()
}

/// Windows 標準の PowerShell 5.1 で作った zip（区切りが `\`）を取り込める。読み替えたあとも危ない名前は断る（壊す担当の検査2回目の1）
#[test]
fn powershell51_backslash_zips() {
    let id = "shape-sample";
    // 中身を選んで圧縮（Compress-Archive -Path .\ex1\*）。空のフォルダは `icons\empty\` の項目になる
    let d = dirs("ps51-contents");
    let mut v: Vec<Z> = good_entries(id).into_iter().filter(|z| !z.name.ends_with(b"/")).collect();
    v.push(Z::dir("icons/empty/"));
    let v = powershell51(v);
    assert!(v.iter().any(|z| z.name == b"icons\\IconStar\\filled.webp"));
    let (insp, staged) = inspect(&d, &v).unwrap();
    assert!(!insp.unwrapped);
    assert!(staged.dir.join("icons").join("IconStar").join("filled.webp").is_file());
    package::commit(&staged, &d, VER).unwrap();
    assert!(package::load(&d, id, VER).is_ok());

    // フォルダごと圧縮（Compress-Archive -Path .\ex1）・日本語のフォルダ名（UTF-8 の印あり）
    for (tag, prefix) in [("ps51-folder", "ex1/"), ("ps51-jp", "スキン/")] {
        let d = dirs(tag);
        let v = powershell51(wrap(prefix.as_bytes(), good_entries(id)).into_iter().skip(1).collect());
        let (insp, staged) = inspect(&d, &v).unwrap();
        assert!(insp.unwrapped, "{tag}");
        assert_eq!(insp.file_count, 9, "{tag}");
        package::discard(&staged);
    }

    // Shift_JIS のフォルダ名（印なし）の 2 バイト目の 0x5C は読み替えない（「ソフト」= 83 5C 83 74 83 67）
    let d = dirs("sjis-5c");
    let mut v = wrap(b"\x83\x5c\x83\x74\x83\x67/", good_entries(id));
    for z in v.iter_mut() {
        z.flags = 0;
    }
    let (insp, staged) = inspect(&d, &v).unwrap();
    assert!(insp.unwrapped);
    package::discard(&staged);

    // 読み替えたあとで、親フォルダへ出る・絶対パス・ドライブ・UNC・重複・段の数・知らないフォルダを断る
    let d = dirs("ps51-bad");
    let img = png(&shape(16, 16, [1, 2, 3], true));
    let with = |extra: Z| {
        let mut v = powershell51(good_entries(id));
        v.push(extra);
        v
    };
    let ascii = |n: &str, data: &[u8]| {
        let mut z = Z::file(n, data);
        z.flags = 0;
        z
    };
    for n in [
        "icons\\..\\..\\x.png",
        "icons\\..\\..\\settings.json",
        "..\\..\\startup\\evil.lnk",
        "\\abs.png",
        "\\\\server\\share\\x.png",
        "\\\\?\\C:\\x.png",
        "C:\\Windows\\System32\\x.png",
        "icons\\\\x.png",
        "icons\\.\\x.png",
        "icons\\a\\b\\c.png",
        // 大文字小文字・区切りだけ違う重複
        "icons\\play.png",
        "icons\\PLAY.png",
    ] {
        assert_eq!(kind(&d, &with(ascii(n, &img))), ErrorKind::SkinPathUnsafe, "{n}");
    }
    // `\` と `/` が混ざった重複（片方は PowerShell の形、片方は ふつうの形）
    let mut mixed = good_entries(id);
    mixed.push(ascii("icons\\play.png", &img));
    assert_eq!(kind(&d, &mixed), ErrorKind::SkinPathUnsafe);
    // UTF-8 の印があっても同じ
    assert_eq!(kind(&d, &with(Z::file("icons\\..\\..\\x.png", &img))), ErrorKind::SkinPathUnsafe);
    // フォルダごとの形でも、外したあとの名前で断る
    let mut wrapped = powershell51(wrap(b"ex1/", good_entries(id)).into_iter().skip(1).collect());
    wrapped.push(ascii("ex1\\..\\..\\x.png", &img));
    assert_eq!(kind(&d, &wrapped), ErrorKind::SkinPathUnsafe);
    assert_eq!(kind(&d, &with(ascii("fonts\\a.ttf", b"\0\x01\0\0"))), ErrorKind::SkinFontNotAllowed);
    assert_eq!(kind(&d, &with(ascii("derived\\a.png", &img))), ErrorKind::SkinFileNotAllowed);
    assert_eq!(kind(&d, &with(ascii("icons\\setup.exe", b"MZ"))), ErrorKind::SkinExecutable);
}

/// 外すフォルダの項目 `X/` に中身があれば断る（外さないときと同じ。壊す担当の検査2回目の2）。
/// フォルダの外にも項目があるときは「全部が1つのフォルダの下にない」と知らせる（同じく3）
#[test]
fn wrapper_folder_entry_and_outside_items() {
    let id = "shape-sample";
    let d = dirs("wrap-data");
    let mut v = wrap(b"X/", good_entries(id));
    v[0].data = b"hidden".to_vec();
    assert_eq!(kind(&d, &v), ErrorKind::SkinZipInvalid);
    let mut v = wrap(b"X/", good_entries(id));
    v[0].data = b"hidden".to_vec();
    v[0].deflate = true;
    assert_eq!(kind(&d, &v), ErrorKind::SkinZipInvalid);
    // 外すフォルダの項目がリンク
    let mut v = wrap(b"X/", good_entries(id));
    v[0].made_by = 3 << 8;
    v[0].external_attr = 0o120777 << 16;
    assert_eq!(kind(&d, &v), ErrorKind::SkinPathUnsafe);
    // 中身の無いフォルダの項目は今までどおり外す
    let (insp, staged) = inspect(&d, &wrap(b"X/", good_entries(id))).unwrap();
    assert!(insp.unwrapped);
    package::discard(&staged);

    // フォルダの外に項目がある
    for extra in [Z::file("other/readme.txt", b"x"), Z::file("readme.txt", b"x"), Z::file("Y/skin.json", b"{}")] {
        let mut v = wrap(b"X/", good_entries(id));
        let out = String::from_utf8_lossy(&extra.name).into_owned();
        v.push(extra);
        let e = inspect(&d, &v).err().expect("accepted");
        assert_eq!(e.kind, ErrorKind::SkinFileNotAllowed, "{out}");
        assert!(e.detail.contains("not all files are inside one folder") && e.detail.contains(&out), "{}", e.detail);
    }
    // フォルダの名前が日本語でも同じ知らせ（ASCII でないフォルダの名前は出さない）
    let mut v = wrap("スキン\u{202E}/".as_bytes(), good_entries(id));
    v.push(Z::file("other/readme.txt", b"x"));
    let e = inspect(&d, &v).err().expect("accepted");
    assert_eq!(e.kind, ErrorKind::SkinFileNotAllowed);
    assert!(e.detail.contains("not all files are inside one folder") && !e.detail.contains('\u{202E}'), "{}", e.detail);
    // 危ない名前は、そちらで断る（知らせを差し替えない）
    let mut v = wrap(b"X/", good_entries(id));
    v.push(Z::file("../../evil.png", b"x"));
    assert_eq!(kind(&d, &v), ErrorKind::SkinPathUnsafe);
}

/// deflate のデータの後ろにごみが付いた項目（圧縮後の大きさの申告が実際より大きい）を断る（壊す担当の検査1回目の5）
#[test]
fn deflate_with_trailing_bytes_is_refused() {
    let d = dirs("trail");
    let id = "shape-sample";
    let mut z = Z::file("icons/play.png", &png(&shape(96, 96, [230, 230, 240], true))).deflated();
    z.trailing = b"hidden payload".to_vec();
    assert_eq!(kind(&d, &replace(id, "icons/play.png", z)), ErrorKind::SkinZipInvalid);
    let mut z = Z::file("skin.json", &manifest(id, full_parts())).deflated();
    z.trailing = vec![0];
    assert_eq!(kind(&d, &replace(id, "skin.json", z)), ErrorKind::SkinZipInvalid);
    // ごみの無いものは通る
    let (_, staged) = inspect(&d, &good_entries(id)).unwrap();
    package::discard(&staged);
}

/// 正式の場所へ移したあとで読めなかったら、正式の場所に残さない（壊す担当の検査1回目の3）
#[test]
fn commit_that_cannot_be_loaded_leaves_nothing() {
    let d = dirs("commit-fail");
    let id = "shape-sample";
    let (_, staged) = inspect(&d, &good_entries(id)).unwrap();
    // 確認のあいだに一時の場所が欠けた（読み込みで断られる形）
    fs::remove_file(staged.dir.join("derived").join("info.json")).unwrap();
    let e = package::commit(&staged, &d, VER).unwrap_err();
    assert_eq!(e.kind, ErrorKind::SkinApplyFailed);
    assert!(!d.skins.join(id).exists(), "broken skin left in the skins folder");
    assert_eq!(package::list(&d, VER).len(), 1);
    // 一時の場所に戻っているので、片付けは今までどおり discard
    package::discard(&staged);
    assert_eq!(fs::read_dir(&d.staging).map(|r| r.count()).unwrap_or(0), 0);
}

/// 取り込みの確かめが2本同時に来ても、確認待ちは1つで、一時の場所も1つだけ（壊す担当の検査1回目の2）
#[test]
fn two_inspections_at_once_leave_one_staging_folder() {
    use super::runtime::{inspect_one_at_a_time, Runtime};
    use std::sync::{Arc, Barrier, Mutex};
    let d = dirs("two-at-once");
    let rt = Arc::new(Mutex::new(Runtime::default()));
    let bytes = Arc::new(zip(&good_entries("shape-sample")));
    let go = Arc::new(Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let (rt, d, bytes, go) = (rt.clone(), d.clone(), bytes.clone(), go.clone());
            std::thread::spawn(move || {
                go.wait();
                inspect_one_at_a_time(&rt, || package::inspect_bytes(&bytes, &d, VER)).unwrap().token
            })
        })
        .collect();
    let tokens: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let pending = rt.lock().unwrap().pending.clone().unwrap();
    assert!(tokens.contains(&pending.token));
    let left: Vec<String> = fs::read_dir(&d.staging).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(left, vec![pending.token.clone()], "staging folders left behind");
    package::discard(&pending);
}

#[test]
fn import_commit_load_resolve_and_list() {
    let d = dirs("ok");
    let (insp, staged) = inspect(&d, &good_entries("shape-sample")).unwrap();
    assert_eq!(insp.id, "shape-sample");
    assert!(!insp.conflict);
    assert_eq!(insp.file_count, 9);
    assert_eq!(insp.parts.icons, 2);
    assert!(insp.parts.application && insp.parts.character && insp.parts.appearance && insp.parts.animated);
    assert!(insp.preview_data_url.as_deref().unwrap().starts_with("data:image/png;base64,"));
    let gif = insp.files.iter().find(|f| f.path == "character/playing.gif").unwrap();
    assert!(gif.animated && gif.frames == 6);
    assert_eq!(gif.duration_ms, 600);
    // 一時の場所に置いただけ（正式の場所には無い）
    assert!(staged.dir.join("skin.json").is_file());
    assert!(!d.skins.join("shape-sample").exists());
    let sum = package::commit(&staged, &d, VER).unwrap();
    assert_eq!(sum.id, "shape-sample");
    assert!(sum.preview.is_some());
    assert!(!staged.dir.exists());
    let inst = package::load(&d, "shape-sample", VER).unwrap();
    let r = inst.resolve();
    let ch = r.character.as_ref().unwrap();
    // 動く立ち絵には最初の1コマ（静止画）があり、静止画には無い
    let still = ch.playing.still.as_deref().unwrap();
    assert!(still.contains("derived") && PathBuf::from(still).is_file());
    assert!(ch.stopped.still.is_none());
    assert_eq!((ch.width_px, ch.max_height_px, ch.min_window_width_px), (180, 420, 920));
    // 画面へ渡すパスは skins\<id> の下だけ
    for p in [&ch.playing.path, &ch.stopped.path, still] {
        assert!(PathBuf::from(p).starts_with(d.skins.join("shape-sample")), "{p}");
    }
    assert!(r.has_application);
    assert_eq!(r.icons["IconStar"]["filled"].dark.as_ref().unwrap().path, r.icons["IconStar"]["filled"].light.as_ref().unwrap().path);
    let list = package::list(&d, VER);
    assert_eq!(list.len(), 2);
    assert!(list[0].builtin && list[0].id == "builtin:default");
    assert_eq!(list[1].id, "shape-sample");
    // 同じ id は上書きしない：確認画面で知らせ、入れようとしても断る
    let (insp2, staged2) = inspect(&d, &good_entries("shape-sample")).unwrap();
    assert!(insp2.conflict);
    assert_eq!(package::commit(&staged2, &d, VER).unwrap_err().kind, ErrorKind::SkinIdExists);
    package::discard(&staged2);
    // アプリ・トレイのアイコンの画像
    let (path, rel) = inst.app_icon_file(true).unwrap();
    assert_eq!(rel, "app/tray.png");
    let (rgba, w, h) = images::app_icon_rgba(&fs::read(path).unwrap(), &rel, 64).unwrap();
    assert_eq!((w, h, rgba.len()), (32, 32, 32 * 32 * 4));
    let (_, w, _) = images::app_icon_rgba(&fs::read(inst.app_icon_file(false).unwrap().0).unwrap(), "app/window.png", 256).unwrap();
    assert_eq!(w, 256);
    // 消す
    package::delete(&d, "shape-sample").unwrap();
    assert!(!d.skins.join("shape-sample").exists());
    assert_eq!(package::delete(&d, "builtin:default").unwrap_err().kind, ErrorKind::SkinBuiltinReadonly);
    assert_eq!(package::delete(&d, "..").unwrap_err().kind, ErrorKind::SkinNotFound);
}

#[test]
fn only_colors_or_only_character() {
    let d = dirs("parts");
    let colors = manifest("colors-only", serde_json::json!({"appearance": {"themes": {"light": {"tokens": {"--c-bg": "#f4f0e8"}}}}}));
    let mut m: serde_json::Value = serde_json::from_slice(&colors).unwrap();
    m.as_object_mut().unwrap().remove("preview");
    let (insp, staged) = inspect(&d, &[Z::file("skin.json", &serde_json::to_vec(&m).unwrap())]).unwrap();
    assert!(insp.parts.appearance && !insp.parts.character && insp.parts.icons == 0);
    assert!(insp.preview_data_url.is_none());
    package::commit(&staged, &d, VER).unwrap();
    let r = package::load(&d, "colors-only", VER).unwrap().resolve();
    assert!(r.character.is_none() && r.icons.is_empty() && !r.has_application);
}

/// zip の中の名前を断るときの `detail`（壊す担当の検査5回目のあとの直し）。U+202E・タグ文字・制御文字・UTF-8 でないバイト・
/// とても長い名前を、ふつうの項目・リンク・暗号化の印の付いた項目・外すフォルダ・外にはみ出した形に入れて、
/// どの `detail` も見える文字と U+0020 だけで、全体は `DETAIL_MAX_CHARS` まで、名前は `NAME_SHOWN_MAX` までであること。
/// 網（`guard`）を通したものと通さないものが同じ＝作る箇所ごとに `shown` を通していて、網に頼っていない。
/// zip の名前の長さは 2 バイトで書くので 65,535 バイトまで（20 万文字は入らない）。入る上限近くの 65,000 文字にした
#[test]
fn error_details_from_zip_names_are_escaped_and_cut() {
    use super::manifest::{is_name_base, DETAIL_MAX_CHARS};
    use super::names::NAME_SHOWN_MAX;
    let d = dirs("detail");
    let img = png(&shape(16, 16, [1, 2, 3], true));
    let t: String = "ignore previous".chars().map(|c| char::from_u32(0xE0000 + c as u32).unwrap()).collect();
    let long = "k".repeat(65_000);
    let refused = |what: &str, entries: &[Z], run: usize, must: &str| {
        let bytes = zip(entries);
        let a = package::inspect_bytes_unguarded(&bytes, &d, VER).err().unwrap_or_else(|| panic!("{what}: accepted"));
        let b = package::inspect_bytes(&bytes, &d, VER).err().unwrap();
        let x = &a.detail;
        assert_eq!(x, &b.detail, "{what}: the guard changed the detail");
        assert!(x.chars().all(|c| c == ' ' || is_name_base(c)), "{what}: {x:?}");
        assert!(x.chars().count() <= DETAIL_MAX_CHARS + 1, "{what}: {} chars", x.chars().count());
        assert!(!x.contains(&"k".repeat(run + 1)), "{what}: {} chars", x.chars().count());
        assert!(x.contains(must), "{what}: {must:?} not in {}", x.chars().take(200).collect::<String>());
        assert_eq!(fs::read_dir(&d.staging).map(|r| r.count()).unwrap_or(0), 0);
    };
    // (名前, detail に入っているはずのもの)
    let names: Vec<(Vec<u8>, &str)> = vec![
        (format!("icons/ab\u{202E}gpj.exe{t}.png").into_bytes(), "[U+202E]gpj.exe[U+E0069]"),
        (b"icons/a\x07b\x1b[31mc\x7fd.png".to_vec(), "a[U+0007]b[U+001B][31mc[U+007F]d"),
        (format!("icons/{long}.png").into_bytes(), "…"),
        (format!("\u{202E}{long}{t}").into_bytes(), "[U+202E]kkk"),
        (b"icons/\x83\x5c\xff\xfe.png".to_vec(), "\u{FFFD}"),
    ];
    for (n, must) in &names {
        let with = |z: Z| {
            let mut v = good_entries("shape-sample");
            v.push(z);
            v
        };
        refused("entry", &with(Z::raw(n, &img)), NAME_SHOWN_MAX, must);
        let mut link = Z::raw(n, b"../../secret");
        link.made_by = 3 << 8;
        link.external_attr = 0o120777 << 16;
        refused("link", &with(link), NAME_SHOWN_MAX, must);
        // 暗号化の印は zip の読み方（settings::zip。設定の取り込みと共有）が名前ごと文にするので、全体の上限で切る
        let mut enc = Z::raw(n, &img);
        enc.flags |= 0x0001;
        refused("encrypted", &with(enc), DETAIL_MAX_CHARS, if must == &"…" { "kkk" } else { must });
    }
    // 外すフォルダの名前（`/` を含まないもの）
    let folders: Vec<(Vec<u8>, &str)> = vec![
        (format!("\u{202E}{long}{t}/").into_bytes(), "[U+202E]kkk"),
        (b"a\x07b\x1b[31m\x00/".to_vec(), "a[U+0007]b[U+001B][31m[U+0000]/"),
        (format!("ab\u{202E}gpj.exe{t}/").into_bytes(), "ab[U+202E]gpj.exe[U+E0069]"),
    ];
    for (f, must) in &folders {
        let mut v = wrap(f, good_entries("shape-sample"));
        v[0].data = b"x".to_vec();
        refused("wrapper folder with data", &v, NAME_SHOWN_MAX, must);
        let mut v = wrap(f, good_entries("shape-sample"));
        v.push(Z::file("readme.txt", b"x"));
        refused("outside the folder", &v, NAME_SHOWN_MAX, must);
    }
}

/// 確認画面に渡す警告は、種類ごとに3件まで。超えた分の数も渡す（壊す担当の検査5回目のあとの直し）
#[test]
fn inspection_caps_warnings_per_code() {
    let d = dirs("warncap");
    let mut m: serde_json::Value = serde_json::from_slice(&manifest("shape-sample", full_parts())).unwrap();
    for i in 0..10 {
        m[format!("unknown{i:02}")] = serde_json::json!(i);
    }
    m["minAppVersion"] = serde_json::json!("9.0.0");
    let entries = replace("shape-sample", "skin.json", Z::file("skin.json", &serde_json::to_vec(&m).unwrap()));
    let (insp, staged) = inspect(&d, &entries).unwrap();
    package::discard(&staged);
    let codes: Vec<&str> = insp.warnings.iter().map(|w| w.code.as_str()).collect();
    assert_eq!(codes, ["unknown_field", "unknown_field", "unknown_field", "min_app_version_newer"]);
    assert_eq!(insp.warnings_omitted.len(), 1);
    assert_eq!((insp.warnings_omitted[0].code.as_str(), insp.warnings_omitted[0].count), ("unknown_field", 7));
    let json = serde_json::to_value(&insp).unwrap();
    assert_eq!(json["warningsOmitted"][0]["count"], 7);
}

/// 壊れた zip・攻撃的な入力（docs\着せ替え_仕様.md 10章の表）
#[test]
fn hostile_zips_are_refused() {
    let d = dirs("bad");
    let id = "shape-sample";
    let img = png(&shape(16, 16, [1, 2, 3], true));
    let with = |extra: Z| {
        let mut v = good_entries(id);
        v.push(extra);
        v
    };
    // パスの外し
    assert_eq!(kind(&d, &with(Z::file("../../startup/evil.lnk", b"x"))), ErrorKind::SkinPathUnsafe);
    assert_eq!(kind(&d, &with(Z::file("icons/../../x.png", &img))), ErrorKind::SkinPathUnsafe);
    // 絶対パス・ドライブ・UNC・\ 区切り
    assert_eq!(kind(&d, &with(Z::file("C:\\Windows\\System32\\x.dll", b"MZ"))), ErrorKind::SkinPathUnsafe);
    assert_eq!(kind(&d, &with(Z::file("C:/Windows/x.png", &img))), ErrorKind::SkinPathUnsafe);
    assert_eq!(kind(&d, &with(Z::file("\\\\server\\share\\x", b"x"))), ErrorKind::SkinPathUnsafe);
    assert_eq!(kind(&d, &with(Z::file("//server/share/x.png", &img))), ErrorKind::SkinPathUnsafe);
    assert_eq!(kind(&d, &with(Z::file("/abs.png", &img))), ErrorKind::SkinPathUnsafe);
    assert_eq!(kind(&d, &with(Z::file("icons\\..\\..\\settings.json", b"{}"))), ErrorKind::SkinPathUnsafe);
    // URL の形で隠した名前・NUL・ASCII 以外
    assert_eq!(kind(&d, &with(Z::file("icons/%2e%2e%2fx.png", &img))), ErrorKind::SkinPathUnsafe);
    assert_eq!(kind(&d, &with(Z::raw(b"icons/x\0.png", &img))), ErrorKind::SkinPathUnsafe);
    assert_eq!(kind(&d, &with(Z::file("icons/\u{3042}.png", &img))), ErrorKind::SkinPathUnsafe);
    // シンボリックリンク（Unix のモード）・特殊なファイル・Windows の再解析点
    let mut link = Z::file("character/link.png", b"../../../../secret");
    link.made_by = 3 << 8;
    link.external_attr = 0o120777 << 16;
    assert_eq!(kind(&d, &with(link)), ErrorKind::SkinPathUnsafe);
    let mut fifo = Z::file("icons/fifo.png", &img);
    fifo.made_by = 3 << 8;
    fifo.external_attr = 0o010644 << 16;
    assert_eq!(kind(&d, &with(fifo)), ErrorKind::SkinPathUnsafe);
    let mut reparse = Z::file("icons/j.png", &img);
    reparse.external_attr = 0x400;
    assert_eq!(kind(&d, &with(reparse)), ErrorKind::SkinPathUnsafe);
    // 大文字小文字だけ違う重複・同じ名前の重複
    assert_eq!(kind(&d, &with(Z::file("icons/PLAY.png", &img))), ErrorKind::SkinPathUnsafe);
    assert_eq!(kind(&d, &with(Z::file("icons/play.png", &img))), ErrorKind::SkinPathUnsafe);
    // CRC 違い・途中で切れた PNG
    let mut crc = Z::file("icons/play.png", &png(&shape(96, 96, [230, 230, 240], true)));
    crc.crc_override = Some(0x1234_5678);
    assert_eq!(kind(&d, &replace(id, "icons/play.png", crc)), ErrorKind::SkinZipInvalid);
    let full = png(&shape(96, 96, [230, 230, 240], true));
    assert_eq!(kind(&d, &replace(id, "icons/play.png", Z::file("icons/play.png", &full[..full.len() / 2]))), ErrorKind::SkinImageInvalid);
    // 65535×65535 を名乗る PNG（巨大なメモリを取る前に断る）
    let mut huge = png(&shape(1, 1, [0, 0, 0], false));
    huge[16..20].copy_from_slice(&65_535u32.to_be_bytes());
    huge[20..24].copy_from_slice(&65_535u32.to_be_bytes());
    let c = crate::settings::test_crc32(&huge[12..29]);
    huge[29..33].copy_from_slice(&c.to_be_bytes());
    let t = std::time::Instant::now();
    assert_eq!(kind(&d, &replace(id, "icons/play.png", Z::file("icons/play.png", &huge))), ErrorKind::SkinImageLimit);
    assert!(t.elapsed().as_secs() < 5);
    // GIF 10000 コマ・極小の遅延（先頭のコマだけで受け入れることはしない）
    let many: Vec<_> = (0..10_000).map(|i| shape(4, 4, [200, 0, 0], i % 2 == 0)).collect();
    let g = gif_anim(&many, 0);
    assert_eq!(kind(&d, &replace(id, "character/playing.gif", Z::file("character/playing.gif", &g))), ErrorKind::SkinImageLimit);
    // APNG の制御の塊が不正（コマが画像の外へ出る）・WebP のコマが壊れている
    let mut a = apng(&moving(32, 48, 3), 100);
    let pos = a.windows(4).position(|w| w == b"fcTL").unwrap();
    // 最初の fcTL の幅を 4096 にして CRC を付け直す
    a[pos + 8..pos + 12].copy_from_slice(&4096u32.to_be_bytes());
    let len = u32::from_be_bytes(a[pos - 4..pos].try_into().unwrap()) as usize;
    let c = crate::settings::test_crc32(&a[pos..pos + 4 + len]);
    a[pos + 4 + len..pos + 8 + len].copy_from_slice(&c.to_be_bytes());
    let apng_manifest = |still: &str| {
        Z::file(
            "skin.json",
            &manifest(id, serde_json::json!({"character": {"states": {"playing": still, "stopped": "character/stopped.png"}, "placement": {"mode": "right-rail"}}})),
        )
    };
    let only_char = |name: &str, data: &[u8]| {
        vec![
            apng_manifest(name),
            Z::file("preview.png", &png(&shape(64, 40, [40, 60, 120], false))),
            Z::file(name, data),
            Z::file("character/stopped.png", &png(&figure(72, 128, [120, 110, 200], 0))),
        ]
    };
    assert_eq!(kind(&d, &only_char("character/playing.png", &a)), ErrorKind::SkinImageInvalid);
    let mut w = webp_anim(&moving(32, 48, 3), 100);
    let last = w.windows(4).rposition(|x| x == b"VP8L").unwrap();
    for b in &mut w[last + 9..last + 30] {
        *b = 0xFF;
    }
    assert_eq!(kind(&d, &only_char("character/playing.webp", &w)), ErrorKind::SkinImageInvalid);
    // skin.json：壊れた JSON・schemaVersion 99・必須キー欠落・重複したキー
    let sj = |b: &[u8]| replace(id, "skin.json", Z::file("skin.json", b));
    assert_eq!(kind(&d, &sj(b"{ not json")), ErrorKind::SkinManifestInvalid);
    let mut v99: serde_json::Value = serde_json::from_slice(&manifest(id, full_parts())).unwrap();
    v99["schemaVersion"] = serde_json::json!(99);
    assert_eq!(kind(&d, &sj(&serde_json::to_vec(&v99).unwrap())), ErrorKind::SkinSchemaUnsupported);
    let mut noname = v99.clone();
    noname["schemaVersion"] = serde_json::json!(1);
    noname.as_object_mut().unwrap().remove("name");
    assert_eq!(kind(&d, &sj(&serde_json::to_vec(&noname).unwrap())), ErrorKind::SkinManifestInvalid);
    assert_eq!(kind(&d, &sj(br#"{"format":"pl-player-skin","schemaVersion":1,"id":"a","id":"b","name":"x","version":"1"}"#)), ErrorKind::SkinManifestInvalid);
    // フォルダが2つ（どちらを外すか決まらない）・フォルダの中に skin.json が無いものは、今までどおり断る
    let mut two = wrap(b"A/", good_entries(id));
    two.push(Z::file("B/icons/x.png", &png(&shape(8, 8, [1, 2, 3], false))));
    assert_eq!(kind(&d, &two), ErrorKind::SkinFileNotAllowed);
    assert_eq!(kind(&d, &wrap(b"A/", good_entries(id)[1..].to_vec())), ErrorKind::SkinFileNotAllowed);
    // 外したあとの名前は今までと同じ決まりで確かめる
    let mut esc = wrap(b"A/", good_entries(id));
    esc.push(Z::file("A/../evil.png", &[0u8; 4]));
    assert_eq!(kind(&d, &esc), ErrorKind::SkinPathUnsafe);
    let mut deep = wrap(b"A/", good_entries(id));
    deep.push(Z::file("A/icons/a/b/c.png", &[0u8; 4]));
    assert_eq!(kind(&d, &deep), ErrorKind::SkinPathUnsafe);
    assert_eq!(kind(&d, &good_entries(id)[1..]), ErrorKind::SkinManifestInvalid);
    // 立ち絵が無いファイルを指す
    let mut missing = good_entries(id);
    missing.retain(|z| z.name != b"character/playing.gif");
    assert_eq!(kind(&d, &missing), ErrorKind::SkinAssetMissing);
    // 名前は .png だが中身は実行ファイル（MZ/PE）
    let mut pe = b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xff\xff".to_vec();
    pe.extend_from_slice(&[0u8; 64]);
    pe.extend_from_slice(b"PE\0\0");
    assert_eq!(kind(&d, &replace(id, "icons/play.png", Z::file("icons/play.png", &pe))), ErrorKind::SkinExecutable);
    // 拡張子と中身が違う（GIF を .png と名乗る）
    assert_eq!(kind(&d, &replace(id, "icons/play.png", Z::file("icons/play.png", &gif_anim(&moving(8, 8, 2), 10)))), ErrorKind::SkinImageInvalid);
    // CSS（外部の読み込みを含む）・SVG（script を含む）・フォント・実行ファイル
    assert_eq!(kind(&d, &with(Z::file("custom.css", b"@import url(https://example.invalid/x.css);"))), ErrorKind::SkinCssNotAllowed);
    assert_eq!(kind(&d, &with(Z::file("icons/icon.svg", b"<svg onload=\"alert(1)\"><script>x</script></svg>"))), ErrorKind::SkinSvgNotAllowed);
    assert_eq!(kind(&d, &with(Z::file("fonts/a.ttf", b"\0\x01\0\0"))), ErrorKind::SkinFontNotAllowed);
    assert_eq!(kind(&d, &with(Z::file("setup.exe", b"MZ"))), ErrorKind::SkinExecutable);
    // 知らないファイル・使われていない画像・アプリが作る場所
    assert_eq!(kind(&d, &with(Z::file("readme.txt", b"hi"))), ErrorKind::SkinFileNotAllowed);
    assert_eq!(kind(&d, &with(Z::file("icons/unused.png", &img))), ErrorKind::SkinFileNotAllowed);
    assert_eq!(kind(&d, &with(Z::file("derived/info.json", b"{}"))), ErrorKind::SkinFileNotAllowed);
    // 暗号化
    let mut enc = Z::file("icons/x.png", &img);
    enc.flags |= 1;
    assert_eq!(kind(&d, &with(enc)), ErrorKind::SkinZipEncrypted);
    // ローカルヘッダの名前が中央ディレクトリと違う
    let mut lie = Z::file("icons/play.png", &full);
    lie.local_name = Some(b"icons/evil.png".to_vec());
    assert_eq!(kind(&d, &replace(id, "icons/play.png", lie)), ErrorKind::SkinZipInvalid);
    // 動くアプリのアイコン・置き方が right-rail 以外
    let anim_icon = replace(id, "app/window.png", Z::file("app/window.png", &apng(&moving(32, 32, 2), 100)));
    assert_eq!(kind(&d, &anim_icon), ErrorKind::SkinImageInvalid);
    let mut overlay: serde_json::Value = serde_json::from_slice(&manifest(id, full_parts())).unwrap();
    overlay["parts"]["character"]["placement"]["mode"] = serde_json::json!("overlay-bottom-right");
    assert_eq!(kind(&d, &sj(&serde_json::to_vec(&overlay).unwrap())), ErrorKind::SkinPlacementUnsupported);
    // 壊れた zip
    assert_eq!(kind(&d, &[]), ErrorKind::SkinManifestInvalid);
    assert_eq!(package::inspect_bytes(b"not a zip at all, just text", &d, VER).unwrap_err().kind, ErrorKind::SkinZipInvalid);
    let mut multi = zip(&good_entries(id));
    let n = multi.len();
    multi[n - 22 + 4] = 1; // ディスク番号
    assert_eq!(package::inspect_bytes(&multi, &d, VER).unwrap_err().kind, ErrorKind::SkinZipInvalid);
    let mut trailing = zip(&good_entries(id));
    trailing.extend_from_slice(b"junk");
    assert_eq!(package::inspect_bytes(&trailing, &d, VER).unwrap_err().kind, ErrorKind::SkinZipInvalid);
}

/// zip 爆弾・数と大きさの上限
#[test]
fn bombs_and_limits() {
    let d = dirs("bomb");
    let id = "shape-sample";
    // 1つの項目が 14MB の 0（数十 KB に縮む）：圧縮率で断る
    let zeros = vec![0u8; 14 * 1024 * 1024];
    let z = Z::file("icons/zero.png", &zeros).deflated();
    assert!(zip(std::slice::from_ref(&z)).len() < 200 * 1024);
    let mut v = good_entries(id);
    v.push(z);
    assert_eq!(kind(&d, &v), ErrorKind::SkinTooLarge);
    // 申告は 1KB なのに、戻すと 14MB になる（申告を信じない）
    let mut lie = Z::file("icons/play.png", &zeros).deflated();
    lie.size_override = Some(1024);
    lie.crc_override = Some(0);
    assert_eq!(kind(&d, &replace(id, "icons/play.png", lie)), ErrorKind::SkinTooLarge);
    // 1ファイルの上限（15MB）
    let mut big = Z::file("icons/play.png", &[1u8; 16]).deflated();
    big.size_override = Some(16 * 1024 * 1024);
    assert_eq!(kind(&d, &replace(id, "icons/play.png", big)), ErrorKind::SkinTooLarge);
    // 展開後の合計（80MB）。1つは 15MB 以下・圧縮率 100 以下でも、合計で断る
    let mut v = good_entries(id);
    // 縮まないバイト列（圧縮率がほぼ 1）を、申告だけ 14MB にする
    let mut x: u32 = 1;
    let noise: Vec<u8> = (0..200 * 1024).map(|_| {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x as u8
    }).collect();
    for i in 0..6 {
        let mut e = Z::file(&format!("icons/fill{i}.png"), &noise).deflated();
        e.size_override = Some(14 * 1024 * 1024);
        v.push(e);
    }
    assert_eq!(kind(&d, &v), ErrorKind::SkinTooLarge);
    // 項目の数（81）・ファイルの数（65）
    let mut v = good_entries(id);
    for i in 0..72 {
        v.push(Z::dir(&format!("icons/d{i}/")));
    }
    assert_eq!(kind(&d, &v), ErrorKind::SkinTooLarge);
    let mut v = good_entries(id);
    for i in 0..56 {
        v.push(Z::file(&format!("icons/f{i}.png"), &[0u8; 4]));
    }
    assert_eq!(kind(&d, &v), ErrorKind::SkinTooLarge);
    // zip のファイルの大きさ（25MB）
    let big = vec![0u8; (limits::MAX_ZIP_BYTES + 1) as usize];
    assert_eq!(package::inspect_bytes(&big, &d, VER).unwrap_err().kind, ErrorKind::SkinTooLarge);
}

fn assert_first_frame(data: &[u8], name: &str, frames: u32) {
    let c = images::check(data, name).unwrap();
    assert!(c.info.animated, "{name}");
    assert_eq!(c.info.frames, frames, "{name}");
    let first = image::load_from_memory_with_format(c.first_frame_png.as_ref().unwrap(), image::ImageFormat::Png).unwrap().to_rgba8();
    assert_eq!(first.dimensions(), (c.info.width, c.info.height));
    // 最初の1コマ（頭の位置が phase 0 の図形）と同じ
    let want = figure(c.info.width, c.info.height, [120, 110, 200], 0);
    let same = first.pixels().zip(want.pixels()).filter(|(a, b)| a == b).count();
    assert!(same as f64 > 0.99 * (want.width() * want.height()) as f64, "{name}: first frame differs");
}

/// 動く画像の形式ごと：GIF・APNG・動く WebP は全部のコマを読み、最初の1コマを静止画にする
#[test]
fn animated_formats_first_frame() {
    let fr = moving(48, 80, 5);
    assert_first_frame(&gif_anim(&fr, 8), "character/a.gif", 5);
    assert_first_frame(&apng(&fr, 80), "character/a.png", 5);
    assert_first_frame(&webp_anim(&fr, 80), "character/a.webp", 5);
    // 静止画の PNG・WebP・1コマの GIF は動かない（最初の1コマを作らない）
    for (data, name) in [(png(&fr[0]), "a.png"), (webp(&fr[0]), "a.webp"), (gif_anim(&fr[..1], 0), "a.gif")] {
        let c = images::check(&data, name).unwrap();
        assert!(!c.info.animated && c.first_frame_png.is_none(), "{name}");
    }
    // 遅延 0 のコマは 100ms として数える（長さの上限の計算）
    let c = images::check(&gif_anim(&fr, 0), "a.gif").unwrap();
    assert_eq!(c.info.duration_ms, 500);
    // 長すぎる（90 秒を超える）
    let long = moving(8, 8, 100);
    assert_eq!(images::check(&gif_anim(&long, 100), "a.gif").err().unwrap().kind, ErrorKind::SkinImageLimit);
}

/// 推奨の立ち絵（720×1280）で 120 コマの動く画像が、上限に当たらずに取り込める（形式ごと）。かかった時間も出す
#[test]
fn recommended_character_size_fits_the_limits() {
    let fr = moving(720, 1280, images::MAX_FRAMES);
    for (data, name) in [(gif_anim(&fr, 5), "character/a.gif"), (apng(&fr, 50), "character/a.png"), (webp_anim(&fr, 50), "character/a.webp")] {
        assert!((data.len() as u64) < images::MAX_IMAGE_BYTES, "{name}: {} bytes", data.len());
        let t = std::time::Instant::now();
        let c = images::check(&data, name).unwrap();
        println!("{name}: {} bytes, {} frames, decode {:?}", data.len(), c.info.frames, t.elapsed());
        assert_eq!(c.info.frames, 120);
        assert!(c.info.decoded_pixels <= images::MAX_ANIMATION_PIXELS);
    }
    // 再生中と停止中の両方が 120 コマでも、合計の上限に収まる
    const { assert!(2 * 720 * 1280 * 120 + 512 * 320 < limits::MAX_TOTAL_PIXELS) };
}

/// 書き出し：skin.json と使う画像だけ（derived は入れない）。書き出した zip はそのまま取り込める
#[test]
fn export_roundtrip() {
    let d = dirs("export");
    let (_, staged) = inspect(&d, &good_entries("shape-sample")).unwrap();
    package::commit(&staged, &d, VER).unwrap();
    let out = d.skins.parent().unwrap().join("out.zip");
    let r = package::export(&d, "shape-sample", &out, VER).unwrap();
    assert_eq!(r.files, 9);
    let bytes = fs::read(&out).unwrap();
    let names: Vec<String> = crate::settings::zip::scan(&bytes, 100).unwrap().iter().map(|e| String::from_utf8(e.raw_name.clone()).unwrap()).collect();
    assert!(names.iter().all(|n| !n.starts_with("derived")));
    assert!(names.contains(&"skin.json".to_string()));
    package::delete(&d, "shape-sample").unwrap();
    let (insp, staged) = package::inspect_bytes(&bytes, &d, VER).unwrap();
    assert_eq!(insp.id, "shape-sample");
    package::commit(&staged, &d, VER).unwrap();
    assert_eq!(package::export(&d, "builtin:default", &out, VER).unwrap_err().kind, ErrorKind::SkinBuiltinReadonly);
}

/// 起動のとき：前に選んでいたスキンが壊れていたら既定に戻す。--safe-mode・次の起動の印なら既定の見た目
#[test]
fn startup_falls_back_and_safe_mode() {
    let d = dirs("startup");
    let (_, staged) = inspect(&d, &good_entries("shape-sample")).unwrap();
    package::commit(&staged, &d, VER).unwrap();
    let sk = SkinSettings { active_skin_id: "shape-sample".into(), ..Default::default() };
    let load = |id: &str| package::load(&d, id, VER);
    let p = plan_startup(&sk, false, load);
    assert!(!p.safe_mode && p.fallback.is_none() && p.installed.is_some());
    // --safe-mode：読み込まずに既定の見た目（保存している選択は変えない）
    let p = plan_startup(&sk, true, |_| panic!("must not load"));
    assert!(p.safe_mode && p.installed.is_none() && p.fallback.is_none() && !p.clear_flag);
    // 次の起動を既定の見た目にする：一度だけ（印を消す）
    let flagged = SkinSettings { safe_mode_next_start: true, ..sk.clone() };
    let p = plan_startup(&flagged, false, |_| panic!("must not load"));
    assert!(p.safe_mode && p.clear_flag);
    // 既定のスキン
    let p = plan_startup(&SkinSettings::default(), false, |_| panic!("must not load"));
    assert!(!p.safe_mode && p.installed.is_none() && p.fallback.is_none());
    // 壊し方ごとに、既定に戻す
    let dir = d.skins.join("shape-sample");
    let broken = |f: &dyn Fn()| {
        f();
        let p = plan_startup(&sk, false, |id| package::load(&d, id, VER));
        p.fallback.map(|(id, k)| {
            assert_eq!(id, "shape-sample");
            k
        })
    };
    let orig_png = fs::read(dir.join("character").join("stopped.png")).unwrap();
    assert_eq!(broken(&|| fs::write(dir.join("character").join("stopped.png"), b"MZ\0\0").unwrap()), Some(ErrorKind::SkinImageInvalid));
    fs::write(dir.join("character").join("stopped.png"), &orig_png).unwrap();
    assert!(broken(&|| {}).is_none());
    let info = fs::read(dir.join("derived").join("info.json")).unwrap();
    assert_eq!(broken(&|| fs::remove_file(dir.join("derived").join("info.json")).unwrap()), Some(ErrorKind::SkinApplyFailed));
    fs::write(dir.join("derived").join("info.json"), &info).unwrap();
    assert_eq!(broken(&|| fs::remove_file(dir.join("icons").join("play.png")).unwrap()), Some(ErrorKind::SkinAssetMissing));
    assert_eq!(broken(&|| fs::write(dir.join("skin.json"), b"{broken").unwrap()), Some(ErrorKind::SkinManifestInvalid));
    assert_eq!(broken(&|| fs::remove_dir_all(&dir).unwrap()), Some(ErrorKind::SkinNotFound));
}

/// 起動の指定（2つ目の起動から届く指定も同じ形。先頭は exe の場所）
#[test]
fn safe_mode_argument() {
    let a = |v: &[&str]| crate::has_safe_mode_arg(v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert!(a(&["PL Player 3.exe", "--safe-mode"]));
    assert!(a(&["x.exe", "--SAFE-MODE"]));
    assert!(!a(&["x.exe"]));
    assert!(!a(&["--safe-mode"])); // exe の場所の欄は見ない
    assert!(!a(&["x.exe", "--safe-mode=no", "safe-mode"]));
}

/// 動作確認用の見本のスキン（図形だけ）：名前と zip のバイト列
fn sample_zips() -> Vec<(&'static str, Vec<u8>)> {
    let mut v = good_entries("shape-sample");
    // 立ち絵は推奨の大きさの半分で、再生中は動く WebP・停止中は静止画
    v.retain(|z| !z.name.starts_with(b"character/") && z.name != b"skin.json");
    let mut m: serde_json::Value = serde_json::from_slice(&manifest("shape-sample", full_parts())).unwrap();
    m["parts"]["character"]["states"]["playing"] = serde_json::json!("character/playing.webp");
    m["description"] = serde_json::json!("図形だけの見本。色・アイコン2つ・アプリのアイコン・立ち絵（再生中は動く）");
    v.push(Z::file("skin.json", &serde_json::to_vec_pretty(&m).unwrap()).deflated());
    v.push(Z::file("character/playing.webp", &webp_anim(&moving(360, 640, 8), 120)));
    v.push(Z::file("character/stopped.png", &png(&figure(360, 640, [120, 110, 200], 0))).deflated());
    let colors = serde_json::json!({
        "format": "pl-player-skin", "schemaVersion": 1, "id": "shape-colors", "name": "Shape Colors", "version": "1.0.0",
        "parts": {"appearance": {
            "themes": {
                "dark": {"tokens": {"--c-bg": "#111319", "--c-panel-rgb": "28, 30, 38", "--c-accent": "#d08a55", "--c-accent-contrast": "#17130f"}, "shadow": "soft"},
                "light": {"tokens": {"--c-bg": "#f4f0e8", "--c-accent": "#87502b", "--c-accent-contrast": "#ffffff"}}
            },
            "tokens": {"--radius": 6}
        }}
    });
    // 読みにくい色（確認画面に警告が出る）
    let low = serde_json::json!({
        "format": "pl-player-skin", "schemaVersion": 1, "id": "shape-low-contrast", "name": "Shape Low Contrast", "version": "1.0.0",
        "parts": {"appearance": {"themes": {"dark": {"tokens": {"--c-secondary": "#3a3a40", "--c-accent": "#303060"}}}}}
    });
    // 壊れたもの（取り込みで断られる）
    let mut bad = good_entries("shape-broken");
    bad.push(Z::file("../evil.png", &png(&shape(8, 8, [1, 1, 1], true))));
    // フォルダごと圧縮した形（エクスプローラー）＋ Thumbs.db と macOS の __MACOSX（確認画面に「無視したファイル」が出る）
    let colors_folder = serde_json::json!({
        "format": "pl-player-skin", "schemaVersion": 1, "id": "shape-folder", "name": "Shape Folder", "version": "1.0.0",
        "parts": {"appearance": {"themes": {"dark": {"tokens": {"--c-accent": "#5aa0d0", "--c-accent-contrast": "#0d1620"}}}}}
    });
    let mut folder = wrap(b"ShapeFolder/", vec![Z::file("skin.json", &serde_json::to_vec_pretty(&colors_folder).unwrap()).deflated()]);
    folder.push(Z::raw(b"ShapeFolder/Thumbs.db", &[0u8; 64]));
    folder.push(Z::raw(b"__MACOSX/ShapeFolder/._skin.json", &[0u8; 82]));
    vec![
        ("shape-sample", zip(&v)),
        ("shape-colors", zip(&[Z::file("skin.json", &serde_json::to_vec_pretty(&colors).unwrap()).deflated()])),
        ("shape-low-contrast", zip(&[Z::file("skin.json", &serde_json::to_vec_pretty(&low).unwrap())])),
        ("shape-broken", zip(&bad)),
        ("shape-folder", zip(&folder)),
    ]
}

/// 手元の zip を、取り込みと同じ確かめに通して結果を出す（一時の場所はすぐ消す。正式の場所には入れない）。
/// エクスプローラーや macOS で実際に作った zip を確かめるため。
/// 走らせ方：`PL_SKIN_ZIP=<zip> PL_SKIN_CHECK_DIR=<プロジェクト内のフォルダ> cargo test --lib skins::tests::check_zip_file -- --ignored --nocapture`
#[test]
#[ignore]
fn check_zip_file() {
    let zip_path = PathBuf::from(std::env::var("PL_SKIN_ZIP").expect("PL_SKIN_ZIP"));
    let d = Dirs::new(&PathBuf::from(std::env::var("PL_SKIN_CHECK_DIR").expect("PL_SKIN_CHECK_DIR")));
    match package::inspect_zip(&zip_path, &d, VER) {
        Ok((insp, staged)) => {
            println!("ok id={} files={} unwrapped={} ignored={:?}", insp.id, insp.file_count, insp.unwrapped, insp.ignored);
            package::discard(&staged);
        }
        Err(e) => println!("refused {:?} {}", e.kind, e.detail),
    }
}

/// 動作確認用の見本のスキン（図形だけ）を zip に書き出す（画面の「＋ zip を取り込む…」で選ぶ）。
/// 走らせ方：`PL_SKIN_SAMPLE_OUT=<プロジェクト内のフォルダ> cargo test --lib skins::tests::make_sample_skins -- --ignored`
#[test]
#[ignore]
fn make_sample_skins() {
    let out = PathBuf::from(std::env::var("PL_SKIN_SAMPLE_OUT").expect("PL_SKIN_SAMPLE_OUT"));
    fs::create_dir_all(&out).unwrap();
    for (name, bytes) in sample_zips() {
        fs::write(out.join(format!("{name}.zip")), bytes).unwrap();
    }
    println!("written to {}", out.display());
}

/// 動作確認用のデータフォルダに、見本のスキンを取り込んで（アプリの取り込みと同じ処理）使うスキンにしておく。
/// 走らせ方：`PL_PLAYER_DATA_DIR=<プロジェクト内のまだ無いフォルダ> [PL_SKIN_ACTIVE=<id>] cargo test --lib skins::tests::seed_skins -- --ignored`
/// そのあと同じ PL_PLAYER_DATA_DIR でアプリを起動すると、そのスキンで開く（壊れていれば既定に戻して知らせる）
#[test]
#[ignore]
fn seed_skins() {
    let data = PathBuf::from(std::env::var("PL_PLAYER_DATA_DIR").expect("PL_PLAYER_DATA_DIR"));
    fs::create_dir_all(&data).unwrap();
    let db = crate::db::Db::open(&data.join("library.db"), data.join("artwork").join("embedded")).unwrap();
    let d = Dirs::new(&data);
    for (name, bytes) in sample_zips() {
        match package::inspect_bytes(&bytes, &d, VER) {
            Ok((insp, staged)) if !insp.conflict => {
                package::commit(&staged, &d, VER).unwrap();
                println!("installed {name}");
            }
            Ok((_, staged)) => {
                package::discard(&staged);
                println!("already installed {name}");
            }
            Err(e) => println!("refused {name}: {:?} {}", e.kind, e.detail),
        }
    }
    let mut s = crate::settings::store::load(&db);
    s.skin.active_skin_id = std::env::var("PL_SKIN_ACTIVE").unwrap_or_else(|_| "shape-sample".into());
    crate::settings::store::save(&db, &s).unwrap();
    println!("active skin: {}", s.skin.active_skin_id);
}
