//! 登録した立ち絵（v3.9.0）のテスト。画像は testkit がその場で作る図形だけ。
//! 断る画像はスキンの立ち絵と同じ決まり（docs\着せ替え_仕様.md 10章の画像の行）を、画像だけの取り込みでも通ることを確かめる。

use std::fs;
use std::path::PathBuf;

use super::*;
use crate::skins::package::CharacterAssets;
use crate::skins::testkit::*;

const VER: &str = "3.9.0";

fn dirs(tag: &str) -> Dirs {
    let base: PathBuf = crate::test_tmp().join(format!("plp-avatar-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    Dirs::new(&base)
}

fn still_png() -> Vec<u8> {
    png(&figure(72, 128, [120, 110, 200], 0))
}

fn pick(data: Vec<u8>, name: &str, slot: Slot) -> Picked {
    pick_bytes(data, name, slot).unwrap().0
}

fn pick_err(data: Vec<u8>, name: &str) -> AppError {
    match pick_bytes(data, name, Slot::Playing) {
        Ok(_) => panic!("accepted {name}"),
        Err(e) => e,
    }
}

fn left_in(dir: &std::path::Path) -> usize {
    fs::read_dir(dir).map(|r| r.count()).unwrap_or(0)
}

#[test]
fn choice_values() {
    assert_eq!(Choice::parse("skin"), Some(Choice::Skin));
    assert_eq!(Choice::parse("none"), Some(Choice::None));
    assert_eq!(Choice::parse("avatar:0123456789abcdef"), Some(Choice::Avatar("0123456789abcdef".into())));
    assert_eq!(Choice::parse("skin:quiet-paper"), Some(Choice::SkinCharacter("quiet-paper".into())));
    for bad in [
        "",
        "Skin",
        "avatar:",
        "avatar:../x",
        "avatar:0123456789ABCDEF",
        "avatar:0123456789abcde",
        "avatar:0123456789abcdef0",
        "skin:",
        "skin:builtin:default",
        "skin:../x",
        "skin:CON",
        "skin:a/b",
        "skin:a\\b",
        "x:y",
    ] {
        assert_eq!(Choice::parse(bad), None, "{bad}");
        assert_eq!(normalize_choice(bad), CHOICE_SKIN, "{bad}");
    }
    for ok in ["skin", "none", "avatar:0123456789abcdef", "skin:quiet-paper"] {
        assert_eq!(normalize_choice(ok), ok);
    }
}

/// 画像だけの取り込みでも、スキンの立ち絵と同じ確かめを通す（形式・中身と拡張子・壊れた画像・大きさ・コマ数・長さ・実行ファイル）。
/// 断る文に、選んだファイルの名前は入らない（アプリが決めた `playing.png` などだけ）
#[test]
fn picked_images_are_checked_like_skin_characters() {
    // 通るもの：PNG・WebP・GIF（動く画像）・APNG・動く WebP
    let p = pick_bytes(still_png(), "My Pose.PNG", Slot::Playing).unwrap().1;
    assert_eq!((p.width, p.height, p.frames, p.animated), (72, 128, 1, false));
    assert_eq!(p.suggested_name, "My Pose");
    assert!(p.preview_data_url.as_deref().unwrap().starts_with("data:image/png;base64,"));
    assert_eq!(p.slot, "playing");
    assert!(valid_id(&p.token));
    let g = pick_bytes(gif_anim(&moving(72, 128, 6), 10), "wave.gif", Slot::Stopped).unwrap();
    assert!(g.1.animated && g.1.frames == 6);
    assert!(g.0.checked.first_frame_png.is_some());
    assert!(pick_bytes(webp(&shape(64, 64, [10, 200, 10], true)), "a.webp", Slot::Playing).is_ok());
    assert!(pick_bytes(apng(&moving(32, 48, 3), 100), "a.png", Slot::Playing).unwrap().1.animated);
    assert!(pick_bytes(webp_anim(&moving(32, 48, 3), 100), "a.webp", Slot::Playing).unwrap().1.animated);

    let secret_name = "secret-folder-name";
    let named = |ext: &str| format!("{secret_name}.{ext}");
    // 形式（拡張子）：PNG・WebP・GIF のほかは断る
    for ext in ["jpg", "svg", "bmp", "exe", "apng", ""] {
        assert_eq!(pick_err(still_png(), &named(ext)).kind, ErrorKind::AvatarImageFormat, "{ext}");
    }
    assert_eq!(pick_err(still_png(), "noext").kind, ErrorKind::AvatarImageFormat);
    // 中身と拡張子が違う（GIF を .png と名乗る）・途中で切れた PNG・中身が実行ファイル
    let e = pick_err(gif_anim(&moving(8, 8, 2), 10), &named("png"));
    assert_eq!(e.kind, ErrorKind::SkinImageInvalid);
    assert!(!e.detail.contains(secret_name), "{}", e.detail);
    assert!(e.detail.contains("playing.png"), "{}", e.detail);
    let full = still_png();
    assert_eq!(pick_err(full[..full.len() / 2].to_vec(), "a.png").kind, ErrorKind::SkinImageInvalid);
    let mut pe = b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xff\xff".to_vec();
    pe.resize(256, 0);
    assert_eq!(pick_err(pe, "a.png").kind, ErrorKind::SkinImageInvalid);
    // 65535×65535 を名乗る PNG（大きなメモリを取る前に断る）
    let mut huge = png(&shape(1, 1, [0, 0, 0], false));
    huge[16..20].copy_from_slice(&65_535u32.to_be_bytes());
    huge[20..24].copy_from_slice(&65_535u32.to_be_bytes());
    let c = crate::settings::test_crc32(&huge[12..29]);
    huge[29..33].copy_from_slice(&c.to_be_bytes());
    let t = std::time::Instant::now();
    assert_eq!(pick_err(huge, "a.png").kind, ErrorKind::SkinImageLimit);
    assert!(t.elapsed().as_secs() < 5);
    // GIF 121 コマ・極小の遅延（先頭のコマだけで受け入れない）
    let many: Vec<_> = (0..(images::MAX_FRAMES + 1)).map(|i| shape(4, 4, [200, 0, 0], i % 2 == 0)).collect();
    assert_eq!(pick_err(gif_anim(&many, 0), "a.gif").kind, ErrorKind::SkinImageLimit);
    // 90 秒を超える（1コマ 50 秒 × 2）
    let long = gif_anim(&moving(8, 8, 2), 5000);
    assert_eq!(pick_err(long, "a.gif").kind, ErrorKind::SkinImageLimit);
    // 1つ 15MB を超える（スキンの画像と同じ上限。文が zip のことを言わない種別に読み替える）
    let mut big = still_png();
    big.resize(images::MAX_IMAGE_BYTES as usize + 1, 0);
    assert_eq!(pick_err(big, "a.png").kind, ErrorKind::AvatarTooLarge);
}

#[test]
fn pick_file_reads_only_files() {
    let d = dirs("pickfile");
    let base = d.avatars.parent().unwrap().to_path_buf();
    let f = base.join("pose.webp");
    fs::write(&f, webp(&shape(40, 40, [1, 2, 3], true))).unwrap();
    let (p, shown) = pick_file(&f, Slot::Stopped).unwrap();
    assert_eq!(p.slot, Slot::Stopped);
    assert_eq!(shown.suggested_name, "pose");
    // 元のファイルを使い続けない（作るときは読んだバイト列を写す）
    fs::remove_file(&f).unwrap();
    // フォルダ・無いファイル・形式の違う名前
    let folder = base.join("folder.png");
    fs::create_dir_all(&folder).unwrap();
    assert_eq!(pick_file(&folder, Slot::Playing).unwrap_err().kind, ErrorKind::SkinImageInvalid);
    assert_eq!(pick_file(&base.join("missing.png"), Slot::Playing).unwrap_err().kind, ErrorKind::FileNotFound);
    assert_eq!(pick_file(&base.join("x.jpg"), Slot::Playing).unwrap_err().kind, ErrorKind::AvatarImageFormat);
    // 断る文に選んだ場所が入らない
    let e = pick_file(&base.join("missing.png"), Slot::Playing).unwrap_err();
    assert!(!e.detail.contains("missing") && !e.detail.contains("plp-avatar"), "{}", e.detail);
}

/// 名前の既定はファイル名。決まりに合わなければ詰める（40 文字で切る・使えない文字を外す）。それでも空なら「立ち絵」
#[test]
fn names_from_file_names() {
    assert_eq!(name_from_file("Pose 1.png"), "Pose 1");
    assert_eq!(name_from_file("  spaced  .png"), "spaced");
    assert_eq!(name_from_file("日本語の名前.gif"), "日本語の名前");
    // 40 文字で切る
    let long = format!("{}.png", "あ".repeat(60));
    assert_eq!(name_from_file(&long), "あ".repeat(40));
    // 使えない文字を外す（向きを変える文字・幅の無い空白・制御文字・続けた異体字セレクタ）
    assert_eq!(name_from_file("gpj\u{202E}exe.png"), "gpjexe");
    assert_eq!(name_from_file("a\u{200B}b.png"), "ab");
    assert_eq!(name_from_file("a\u{0007}b.png"), "ab");
    assert_eq!(name_from_file("A\u{FE0F}\u{FE0F}.png"), "A");
    // 外してから 40 文字で切る
    let mixed = format!("{}.png", "x\u{200B}".repeat(50));
    assert_eq!(name_from_file(&mixed), "x".repeat(40));
    // 決まりに合う名前（絵文字の ZWJ・異体字セレクタ1つ）はそのまま
    assert_eq!(name_from_file("👨\u{200D}👩\u{200D}👧.png"), "👨\u{200D}👩\u{200D}👧");
    // それでも空なら「立ち絵」
    for empty in ["\u{3164}.png", "\u{200B}.png", "\u{202E}.png", ".png", "", "   .gif"] {
        let n = name_from_file(empty);
        if empty == ".png" {
            // 拡張子だけに見える名前は、名前そのもの（Path の決まり。拡張子が無いファイル）
            assert_eq!(n, ".png");
        } else {
            assert_eq!(n, DEFAULT_NAME, "{empty:?}");
        }
    }
    assert_eq!(DEFAULT_NAME, "立ち絵");
    // どの名前も、名前の決まりに通る
    for f in ["gpj\u{202E}exe.png", &long, &mixed, "\u{3164}.png"] {
        assert!(check_name(&name_from_file(f)).is_ok(), "{f:?}");
    }
}

/// 名前を空のまま作ったら、再生中の絵のファイル名を名前にする（断らない）。空白だけも同じ
#[test]
fn empty_name_takes_the_file_name() {
    let d = dirs("noname");
    for (input, file, want) in [("", "Wave Pose.gif", "Wave Pose"), ("   ", "z\u{200B}y.png", "zy"), ("", "\u{3164}.png", "立ち絵")] {
        let p = pick(still_png(), &file.replace(".gif", ".png"), Slot::Playing);
        let s = pick(still_png(), "other-name.png", Slot::Stopped);
        let a = create(&d, input, &p, Some(&s)).unwrap();
        assert_eq!(a.name, want, "{file:?}");
        assert_eq!(load(&d, &a.id).unwrap().name(), want);
    }
    // 名前を入れたときの決まりは今までどおり（決まりに合わなければ断る）
    let p = pick(still_png(), "ok.png", Slot::Playing);
    assert_eq!(create(&d, "a\u{202E}b", &p, None).unwrap_err().kind, ErrorKind::AvatarNameInvalid);
}

#[test]
fn names_follow_the_skin_name_rules() {
    assert_eq!(check_name("  Pose  ").unwrap(), "Pose");
    assert_eq!(check_name(&"a".repeat(40)).unwrap().len(), 40);
    for ok in ["日本語の名前", "Cafe\u{301}", "👨\u{200D}👩\u{200D}👧", "❤\u{FE0F}"] {
        assert!(check_name(ok).is_ok(), "{ok:?}");
    }
    for bad in [
        String::new(),
        "   ".into(),
        "a".repeat(41),
        "gpj\u{202E}exe".into(),
        "a\u{200B}b".into(),
        "a\u{0007}b".into(),
        "a\nb".into(),
        "\u{3164}".into(),
        "\u{301}".into(),
        "A\u{FE0F}\u{FE0F}".into(),
        "a\u{2028}b".into(),
    ] {
        let e = check_name(&bad).unwrap_err();
        assert_eq!(e.kind, ErrorKind::AvatarNameInvalid, "{bad:?}");
        // 断る文に名前そのものは入らない
        if bad.chars().count() > 3 {
            assert!(!e.detail.contains(&bad), "{bad:?}: {}", e.detail);
        }
    }
}

/// 作る：avatars\<id>\ に写して置く（avatar.json・playing.<形式>・stopped.<形式>・動く画像の最初の1コマ）。一時の場所は残さない。
/// 停止中を選ばなければ、再生中の絵を両方に使う
#[test]
fn create_load_character_rename_and_delete() {
    let d = dirs("create");
    let playing = pick(gif_anim(&moving(72, 128, 6), 10), "wave.gif", Slot::Playing);
    let stopped = pick(still_png(), "rest.png", Slot::Stopped);
    let sum = create(&d, "Shape Pose", &playing, Some(&stopped)).unwrap();
    assert!(valid_id(&sum.id));
    assert_eq!(sum.name, "Shape Pose");
    assert!(sum.has_stopped && sum.animated && sum.broken.is_none());
    let dir = d.avatars.join(&sum.id);
    let mut files: Vec<String> = fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    files.sort();
    assert_eq!(files, ["avatar.json", "derived", "playing.gif", "stopped.png"]);
    assert!(dir.join("derived").join("playing.png").is_file());
    assert_eq!(fs::read(dir.join("stopped.png")).unwrap(), still_png());
    assert_eq!(left_in(&d.avatar_staging), 0);
    // 見本は停止中の絵（静止画）
    assert_eq!(sum.preview.as_deref(), Some(dir.join("stopped.png").to_string_lossy().as_ref()));
    // 「変更」の小窓の見本：再生中は動く画像の最初の1コマ・停止中はその絵
    assert_eq!(sum.playing_preview.as_deref(), Some(dir.join("derived").join("playing.png").to_string_lossy().as_ref()));
    assert_eq!(sum.stopped_preview, sum.preview);

    let a = load(&d, &sum.id).unwrap();
    let c = a.character();
    assert_eq!((c.width_px, c.max_height_px, c.min_window_width_px), (WIDTH_PX, MAX_HEIGHT_PX, MIN_WINDOW_WIDTH_PX));
    assert_eq!((WIDTH_PX, MAX_HEIGHT_PX, MIN_WINDOW_WIDTH_PX), (180, 420, 920));
    assert!(c.playing.path.ends_with("playing.gif") && c.playing.animated);
    assert!(c.playing.still.as_deref().unwrap().ends_with("playing.png"));
    assert!(c.stopped.path.ends_with("stopped.png") && c.stopped.still.is_none());
    // 画面へ渡すパスは avatars\<id>\ の中だけ
    for p in [&c.playing.path, c.playing.still.as_ref().unwrap(), &c.stopped.path] {
        assert!(std::path::Path::new(p).starts_with(&dir), "{p}");
    }

    // 停止中なし → 再生中の絵を両方に
    let only = pick(still_png(), "one.png", Slot::Playing);
    let s2 = create(&d, "One", &only, None).unwrap();
    assert!(!s2.has_stopped && !s2.animated);
    let c2 = load(&d, &s2.id).unwrap().character();
    assert_eq!(c2.playing, c2.stopped);
    let mut f2: Vec<String> = fs::read_dir(d.avatars.join(&s2.id)).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    f2.sort();
    assert_eq!(f2, ["avatar.json", "playing.png"]);

    // 一覧は作った順
    let l = list(&d);
    assert_eq!(l.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), [sum.id.as_str(), s2.id.as_str()]);

    // 名前を変える（「変更」の小窓。絵はそのまま）
    assert_eq!(update(&d, &s2.id, " New Name ", None, StoppedChange::Keep).unwrap().name, "New Name");
    assert_eq!(load(&d, &s2.id).unwrap().name(), "New Name");
    assert_eq!(update(&d, &s2.id, "x\u{202E}y", None, StoppedChange::Keep).unwrap_err().kind, ErrorKind::AvatarNameInvalid);
    assert_eq!(load(&d, &s2.id).unwrap().name(), "New Name");
    // 名前が空なら今の名前のまま
    assert_eq!(update(&d, &s2.id, "", None, StoppedChange::Keep).unwrap().name, "New Name");
    assert!(!d.avatars.join(&s2.id).join("avatar.json.tmp").exists());
    assert_eq!(update(&d, "ffffffffffffffff", "x", None, StoppedChange::Keep).unwrap_err().kind, ErrorKind::AvatarNotFound);

    // 消す
    delete(&d, &s2.id).unwrap();
    assert!(!d.avatars.join(&s2.id).exists());
    assert_eq!(delete(&d, &s2.id).unwrap_err().kind, ErrorKind::AvatarNotFound);
    assert_eq!(load(&d, &s2.id).unwrap_err().kind, ErrorKind::AvatarNotFound);
    assert_eq!(list(&d).len(), 1);
}

/// 作るときに断る：名前・数の上限（30）・枠の取り違え。何も残さない
#[test]
fn create_refusals_leave_nothing() {
    let d = dirs("refuse");
    let p = pick(still_png(), "p.png", Slot::Playing);
    let long = "a".repeat(41);
    // 空・空白だけの名前は断らない（ファイル名を名前にする。empty_name_takes_the_file_name）
    for bad in ["a\u{202E}b", long.as_str(), "a\u{200B}b"] {
        assert_eq!(create(&d, bad, &p, None).unwrap_err().kind, ErrorKind::AvatarNameInvalid);
    }
    // 停止中の枠に再生中の画像を渡した（取り違え）
    let p2 = pick(still_png(), "p.png", Slot::Playing);
    assert_eq!(create(&d, "x", &p, Some(&p2)).unwrap_err().kind, ErrorKind::AvatarPickExpired);
    assert_eq!(left_in(&d.avatars), 0);
    assert_eq!(left_in(&d.avatar_staging), 0);

    // 30 体まで（読めないものも数える）
    for i in 0..MAX_AVATARS {
        create(&d, &format!("n{i}"), &p, None).unwrap();
    }
    assert_eq!(count(&d), MAX_AVATARS);
    let e = create(&d, "one more", &p, None).unwrap_err();
    assert_eq!(e.kind, ErrorKind::AvatarLimit);
    assert_eq!(count(&d), MAX_AVATARS);
    assert_eq!(left_in(&d.avatar_staging), 0);
    // 1つ消せば作れる
    let first = list(&d)[0].id.clone();
    delete(&d, &first).unwrap();
    assert!(create(&d, "again", &p, None).is_ok());
    // 立ち絵の形の名前でないフォルダは数えない（一覧にも出さない）
    fs::create_dir_all(d.avatars.join("not-an-avatar")).unwrap();
    assert_eq!(count(&d), MAX_AVATARS);
}

/// 読み込み：avatar.json の書き換え（外を指すファイル名・形の違う名前）・画像が消えた・中身が違う・リンクを断る。
/// 読めない立ち絵も一覧に出し（消せるように）、消せる
#[test]
fn tampered_avatars_are_refused() {
    let d = dirs("tamper");
    let p = pick(gif_anim(&moving(16, 16, 3), 10), "p.gif", Slot::Playing);
    let make = |name: &str| create(&d, name, &p, None).unwrap().id;
    let meta_of = |id: &str| -> serde_json::Value { serde_json::from_slice(&fs::read(d.avatars.join(id).join("avatar.json")).unwrap()).unwrap() };
    let write_meta = |id: &str, v: &serde_json::Value| fs::write(d.avatars.join(id).join("avatar.json"), serde_json::to_vec(v).unwrap()).unwrap();

    for (field, value) in [
        ("file", serde_json::json!("../../settings.json")),
        ("file", serde_json::json!("playing.exe")),
        ("file", serde_json::json!("stopped.gif")),
        ("file", serde_json::json!("C:\\x.gif")),
        ("file", serde_json::json!("playing.gif/../../x")),
        ("still", serde_json::json!("../derived/playing.png")),
        ("still", serde_json::json!(null)),
        ("width", serde_json::json!(0)),
    ] {
        let id = make("t");
        let mut m = meta_of(&id);
        m["playing"][field] = value.clone();
        write_meta(&id, &m);
        assert_eq!(load(&d, &id).unwrap_err().kind, ErrorKind::AvatarBroken, "{field} = {value}");
        delete(&d, &id).unwrap();
    }
    // 名前を書き換え（向きを変える文字）・版が違う・壊れた JSON・大きすぎる JSON
    let id = make("t");
    let mut m = meta_of(&id);
    m["name"] = serde_json::json!("a\u{202E}b");
    write_meta(&id, &m);
    assert_eq!(load(&d, &id).unwrap_err().kind, ErrorKind::AvatarBroken);
    m["name"] = serde_json::json!("ok");
    m["version"] = serde_json::json!(99);
    write_meta(&id, &m);
    assert_eq!(load(&d, &id).unwrap_err().kind, ErrorKind::AvatarBroken);
    fs::write(d.avatars.join(&id).join("avatar.json"), b"{not json").unwrap();
    assert_eq!(load(&d, &id).unwrap_err().kind, ErrorKind::AvatarBroken);
    fs::write(d.avatars.join(&id).join("avatar.json"), vec![b' '; 70 * 1024]).unwrap();
    assert_eq!(load(&d, &id).unwrap_err().kind, ErrorKind::AvatarBroken);
    // 一覧に「読めない」として出る（消せる）
    let l = list(&d);
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].broken, Some(ErrorKind::AvatarBroken));
    assert!(l[0].preview.is_none());
    delete(&d, &id).unwrap();

    // 画像が消えた・中身が実行ファイル・最初の1コマが消えた
    let id = make("t");
    fs::write(d.avatars.join(&id).join("playing.gif"), b"MZ\0\0").unwrap();
    assert_eq!(load(&d, &id).unwrap_err().kind, ErrorKind::AvatarBroken);
    fs::remove_file(d.avatars.join(&id).join("playing.gif")).unwrap();
    assert_eq!(load(&d, &id).unwrap_err().kind, ErrorKind::AvatarBroken);
    delete(&d, &id).unwrap();
    let id = make("t");
    fs::remove_file(d.avatars.join(&id).join("derived").join("playing.png")).unwrap();
    assert_eq!(load(&d, &id).unwrap_err().kind, ErrorKind::AvatarBroken);
    delete(&d, &id).unwrap();

    // 画面から来た形の違う id（外し・区切り・予約名）はフォルダの名前にしない
    for bad in ["..", "../skins", "..\\x", "CON", "nul", "0123456789abcde", "0123456789ABCDEF", "", "a/b"] {
        assert_eq!(load(&d, bad).unwrap_err().kind, ErrorKind::AvatarNotFound, "{bad}");
        assert_eq!(delete(&d, bad).unwrap_err().kind, ErrorKind::AvatarNotFound, "{bad}");
        assert_eq!(update(&d, bad, "x", None, StoppedChange::Keep).unwrap_err().kind, ErrorKind::AvatarNotFound, "{bad}");
    }

    // ジャンクション（管理者の権限は要らない）。作れなければ、その確かめだけ飛ばす
    let outside = d.avatars.parent().unwrap().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let real = make("real");
    // 外の場所に、正しく見える立ち絵のフォルダを写しておく
    for e in fs::read_dir(d.avatars.join(&real)).unwrap().flatten() {
        if e.path().is_file() {
            fs::copy(e.path(), outside.join(e.file_name())).unwrap();
        }
    }
    fs::create_dir_all(outside.join("derived")).unwrap();
    fs::copy(d.avatars.join(&real).join("derived").join("playing.png"), outside.join("derived").join("playing.png")).unwrap();
    let link_id = "abcdefabcdef0123";
    let junction = d.avatars.join(link_id);
    let made = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&junction).arg(&outside).output().map(|o| o.status.success()).unwrap_or(false);
    println!("junction made: {made}");
    if made {
        assert_eq!(load(&d, link_id).unwrap_err().kind, ErrorKind::AvatarBroken);
        // 消すと、リンクだけを外す（先の中身は残る）
        delete(&d, link_id).unwrap();
        assert!(!junction.exists());
        assert!(outside.join("avatar.json").is_file());
        // 立ち絵の中の画像がリンク
        let inner = d.avatars.join(&real).join("derived");
        fs::remove_dir_all(&inner).unwrap();
        let ok = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&inner).arg(outside.join("derived")).output().map(|o| o.status.success()).unwrap_or(false);
        if ok {
            assert_eq!(load(&d, &real).unwrap_err().kind, ErrorKind::AvatarBroken);
            delete(&d, &real).unwrap();
            assert!(outside.join("derived").join("playing.png").is_file(), "deleting must not follow the link");
        }
    }
}

fn ch(tag: &str) -> CharacterAssets {
    let a = |p: &str| AssetRef { path: format!("{tag}/{p}"), still: None, width: 10, height: 20, animated: false };
    CharacterAssets { playing: a("playing.png"), stopped: a("stopped.png"), width_px: 200, max_height_px: 400, min_window_width_px: 900 }
}

fn skin(id: &str, character: Option<CharacterAssets>) -> ResolvedSkin {
    ResolvedSkin {
        id: id.into(),
        name: id.into(),
        version: "1".into(),
        author: String::new(),
        appearance: None,
        icons: Default::default(),
        has_application: false,
        character,
    }
}

/// 使う立ち絵の解き方：スキンの立ち絵・出さない・登録した立ち絵・ほかのスキンの立ち絵。無い・読めなければ「スキンの立ち絵」に戻す。
/// スキンを替えても、登録した立ち絵・ほかのスキンの立ち絵を選んでいればそのまま
#[test]
fn resolve_choices_and_fallbacks() {
    let a_skin = skin("a-skin", Some(ch("a")));
    let plain = skin("plain", None);
    let no_avatar = |_: &str| -> AppResult<CharacterAssets> { panic!("avatar must not be loaded") };
    let no_skin = |_: &str| -> AppResult<Option<CharacterAssets>> { panic!("skin must not be loaded") };
    let gone = |_: &str| -> AppResult<CharacterAssets> { Err(AppError::new(ErrorKind::AvatarNotFound, "x")) };
    let ok_avatar = |id: &str| -> AppResult<CharacterAssets> {
        assert_eq!(id, "0123456789abcdef");
        Ok(ch("avatar"))
    };
    let other_skin = |id: &str| -> AppResult<Option<CharacterAssets>> {
        assert_eq!(id, "b-skin");
        Ok(Some(ch("b")))
    };

    // スキンの立ち絵（既定）
    let r = resolve("skin", Some(&a_skin), false, no_avatar, no_skin);
    assert_eq!(r, Resolution { character: Some(ch("a")), fallback: None });
    assert_eq!(resolve("skin", Some(&plain), false, no_avatar, no_skin).character, None);
    assert_eq!(resolve("skin", None, false, no_avatar, no_skin).character, None);
    // 形の違う値もスキンの立ち絵（戻したとは言わない。設定を読むときに整えてある）
    assert_eq!(resolve("bogus", Some(&a_skin), false, no_avatar, no_skin), Resolution { character: Some(ch("a")), fallback: None });
    // 出さない
    assert_eq!(resolve("none", Some(&a_skin), false, no_avatar, no_skin).character, None);
    // 登録した立ち絵：スキンに関係なく出す（スキンを替えてもそのまま）
    for cur in [Some(&a_skin), Some(&plain), None] {
        assert_eq!(resolve("avatar:0123456789abcdef", cur, false, ok_avatar, no_skin), Resolution { character: Some(ch("avatar")), fallback: None });
    }
    // 消えた → スキンの立ち絵に戻す
    assert_eq!(
        resolve("avatar:0123456789abcdef", Some(&a_skin), false, gone, no_skin),
        Resolution { character: Some(ch("a")), fallback: Some(ErrorKind::AvatarNotFound) }
    );
    // ほかのスキンの立ち絵（当てているスキンは替えない）
    assert_eq!(resolve("skin:b-skin", Some(&a_skin), false, no_avatar, other_skin), Resolution { character: Some(ch("b")), fallback: None });
    assert_eq!(resolve("skin:b-skin", None, false, no_avatar, other_skin).character, Some(ch("b")));
    // 当てているスキンを指していれば、読み直さない
    assert_eq!(resolve("skin:a-skin", Some(&a_skin), false, no_avatar, no_skin).character, Some(ch("a")));
    // 指しているスキンが消えた・立ち絵が無い → スキンの立ち絵に戻す
    let skin_gone = |_: &str| -> AppResult<Option<CharacterAssets>> { Err(AppError::new(ErrorKind::SkinNotFound, "x")) };
    assert_eq!(resolve("skin:b-skin", Some(&a_skin), false, no_avatar, skin_gone), Resolution { character: Some(ch("a")), fallback: Some(ErrorKind::SkinNotFound) });
    let no_char = |_: &str| -> AppResult<Option<CharacterAssets>> { Ok(None) };
    assert_eq!(resolve("skin:b-skin", Some(&plain), false, no_avatar, no_char), Resolution { character: None, fallback: Some(ErrorKind::AvatarNotFound) });
    assert_eq!(resolve("skin:plain", Some(&plain), false, no_avatar, no_skin).fallback, Some(ErrorKind::AvatarNotFound));
    // セーフモード（既定の見た目）の間は、どれを選んでいても出さない。戻しもしない
    for c in ["skin", "none", "avatar:0123456789abcdef", "skin:b-skin"] {
        assert_eq!(resolve(c, Some(&a_skin), true, no_avatar, no_skin), Resolution { character: None, fallback: None }, "{c}");
    }
}

/// 本物のフォルダで：作った立ち絵を選べる・消すと選べない（戻す）。取り込んだスキンの立ち絵は、立ち絵を持つスキンだけ一覧に出て選べる
#[test]
fn choices_against_real_folders() {
    let d = dirs("choices");
    let p = pick(still_png(), "p.png", Slot::Playing);
    let a = create(&d, "Mine", &p, None).unwrap();
    let choice = format!("avatar:{}", a.id);
    assert_eq!(check_choice(&d, &choice, VER).unwrap(), choice);
    assert_eq!(check_choice(&d, "skin", VER).unwrap(), "skin");
    assert_eq!(check_choice(&d, "none", VER).unwrap(), "none");
    assert_eq!(check_choice(&d, "avatar:ffffffffffffffff", VER).unwrap_err().kind, ErrorKind::AvatarNotFound);
    assert_eq!(check_choice(&d, "avatar:../x", VER).unwrap_err().kind, ErrorKind::AvatarNotFound);
    assert_eq!(check_choice(&d, "skin:nothing-here", VER).unwrap_err().kind, ErrorKind::SkinNotFound);
    let r = resolve(&choice, None, false, |id| load(&d, id).map(|x| x.character()), |_| Ok(None));
    assert!(r.character.is_some() && r.fallback.is_none());
    delete(&d, &a.id).unwrap();
    let r = resolve(&choice, None, false, |id| load(&d, id).map(|x| x.character()), |_| Ok(None));
    assert_eq!(r, Resolution { character: None, fallback: Some(ErrorKind::AvatarNotFound) });

    // 取り込んだスキン：立ち絵だけのスキンと、色だけのスキン
    let fig = png(&figure(72, 128, [120, 110, 200], 0));
    let with_char = zip(&[
        Z::file(
            "skin.json",
            &serde_json::to_vec(&serde_json::json!({
                "format": "pl-player-skin", "schemaVersion": 1, "id": "char-only", "name": "Char Only", "version": "1.0.0",
                "parts": {"character": {"states": {"playing": "character/playing.gif", "stopped": "character/stopped.png"}, "placement": {"mode": "right-rail", "widthPx": 200}}}
            }))
            .unwrap(),
        ),
        Z::file("character/playing.gif", &gif_anim(&moving(72, 128, 4), 10)),
        Z::file("character/stopped.png", &fig),
    ]);
    let colors = zip(&[Z::file(
        "skin.json",
        &serde_json::to_vec(&serde_json::json!({
            "format": "pl-player-skin", "schemaVersion": 1, "id": "colors-only", "name": "Colors", "version": "1.0.0",
            "parts": {"appearance": {"themes": {"dark": {"tokens": {"--c-accent": "#d08a55"}}}}}
        }))
        .unwrap(),
    )]);
    for z in [with_char, colors] {
        let (_, staged) = package::inspect_bytes(&z, &d, VER).unwrap();
        package::commit(&staged, &d, VER).unwrap();
    }
    let rows = skin_characters(&d, VER);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].skin_id, "char-only");
    assert_eq!(rows[0].name, "Char Only");
    assert!(rows[0].animated);
    assert!(rows[0].preview.as_deref().unwrap().ends_with("stopped.png"));
    assert_eq!(check_choice(&d, "skin:char-only", VER).unwrap(), "skin:char-only");
    assert_eq!(check_choice(&d, "skin:colors-only", VER).unwrap_err().kind, ErrorKind::AvatarNotFound);
    // スキンの絵を指すだけで写さない（avatars\ に何も増えない）・置き方はそのスキンの値
    assert_eq!(count(&d), 0);
    let c = package::load(&d, "char-only", VER).unwrap().resolve().character.unwrap();
    assert_eq!(c.width_px, 200);
    assert!(std::path::Path::new(&c.stopped.path).starts_with(d.skins.join("char-only")));
    let all = list_all(&d, VER);
    assert_eq!((all.skins.len(), all.avatars.len(), all.max), (1, 0, 30));
}

/// 作る途中で終わった一時の場所は、起動のときに消す
#[test]
fn staging_is_cleaned() {
    let d = dirs("staging");
    fs::create_dir_all(d.avatar_staging.join("0123456789abcdef")).unwrap();
    fs::write(d.avatar_staging.join("0123456789abcdef").join("playing.png"), still_png()).unwrap();
    clean_staging(&d);
    assert!(!d.avatar_staging.exists());
}

/// 選んだ画像の印：合うものだけ取り出す。合わなければ何も取り出さない（枠に残る）
#[test]
fn picks_are_taken_by_token() {
    use crate::skins::runtime::take_picks;
    let p = pick(still_png(), "p.png", Slot::Playing);
    let s = pick(still_png(), "s.png", Slot::Stopped);
    let (pt, st) = (p.token.clone(), s.token.clone());
    let mut slots = [Some(p), Some(s)];
    assert_eq!(take_picks(&mut slots, "ffffffffffffffff", None).unwrap_err().kind, ErrorKind::AvatarPickExpired);
    assert_eq!(take_picks(&mut slots, &pt, Some("ffffffffffffffff")).unwrap_err().kind, ErrorKind::AvatarPickExpired);
    // 再生中と停止中の印を入れ替えても取り出せない
    assert_eq!(take_picks(&mut slots, &st, Some(&pt)).unwrap_err().kind, ErrorKind::AvatarPickExpired);
    assert!(slots[0].is_some() && slots[1].is_some());
    // 停止中なし：再生中だけを取り出し、停止中の枠は残す
    let (a, b) = take_picks(&mut slots, &pt, None).unwrap();
    assert_eq!(a.token, pt);
    assert!(b.is_none());
    assert!(slots[0].is_none() && slots[1].is_some());
    slots[0] = Some(a);
    let (a, b) = take_picks(&mut slots, &pt, Some(&st)).unwrap();
    assert_eq!((a.token, b.unwrap().token), (pt, st));
    assert!(slots.iter().all(|x| x.is_none()));
}

/// 使っている立ち絵（スキンの立ち絵を含む）を消すときは、先に切り替えて（画面が絵を手放して）から消す。
/// 消せなかったら切り替えを戻す。使っていなければ切り替えない
#[test]
fn delete_switches_away_first() {
    use crate::skins::runtime::switch_then_delete;
    use std::cell::RefCell;
    let log = RefCell::new(Vec::<&str>::new());
    let ok = switch_then_delete(true, || log.borrow_mut().push("switch"), || { log.borrow_mut().push("delete"); Ok(()) }, || log.borrow_mut().push("restore"));
    assert!(ok.is_ok());
    assert_eq!(*log.borrow(), ["switch", "delete"]);
    log.borrow_mut().clear();
    let e = switch_then_delete(
        true,
        || log.borrow_mut().push("switch"),
        || {
            log.borrow_mut().push("delete");
            Err(AppError::new(ErrorKind::AccessDenied, "in use"))
        },
        || log.borrow_mut().push("restore"),
    );
    assert_eq!(e.unwrap_err().kind, ErrorKind::AccessDenied);
    assert_eq!(*log.borrow(), ["switch", "delete", "restore"]);
    log.borrow_mut().clear();
    let _ = switch_then_delete(false, || log.borrow_mut().push("switch"), || { log.borrow_mut().push("delete"); Err(AppError::other("x")) }, || log.borrow_mut().push("restore"));
    assert_eq!(*log.borrow(), ["delete"]);
    // runtime の2か所（立ち絵の削除・スキンの削除）が、この順番の関数を通っていること（ソースを読む）
    let src = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/skins/runtime.rs")).unwrap();
    for f in ["pub fn delete_avatar(", "pub fn delete(app: &AppHandle"] {
        let body = &src[src.find(f).unwrap()..];
        let body = &body[..body.find("\n}\n").unwrap()];
        let sw = body.find("switch_then_delete(").unwrap_or_else(|| panic!("{f}"));
        let del = body.find("::delete(&dirs(&state), id)").unwrap_or_else(|| panic!("{f}"));
        assert!(sw < del, "{f}: delete must be inside switch_then_delete");
        assert_eq!(body.matches("::delete(&dirs(&state), id)").count(), 1, "{f}");
    }
}

/// 画像を選んで確かめている間に取り消されたら（小窓の「キャンセル」）、あとから届いた画像は置かずに捨てる
#[test]
fn picks_after_cancel_are_dropped() {
    use crate::skins::runtime::{clear_picks, store_pick, Runtime};
    let rt = std::sync::Mutex::new(Runtime::default());
    let gen0 = rt.lock().unwrap().pick_gen;
    // ふつう：控えた世代のままなら置く
    assert!(store_pick(&rt, gen0, pick(still_png(), "p.png", Slot::Playing)));
    assert!(rt.lock().unwrap().picks[0].is_some());
    // 選ぶ画面を出す前に世代を控える → その間に取り消す → 届いた画像は捨てる
    let gen1 = rt.lock().unwrap().pick_gen;
    clear_picks(&rt);
    assert!(rt.lock().unwrap().picks.iter().all(|p| p.is_none()));
    assert!(!store_pick(&rt, gen1, pick(still_png(), "s.png", Slot::Stopped)));
    assert!(rt.lock().unwrap().picks.iter().all(|p| p.is_none()));
    // 取り消したあとに新しく選び始めたもの（新しい世代）は置く
    let gen2 = rt.lock().unwrap().pick_gen;
    assert_ne!(gen1, gen2);
    assert!(store_pick(&rt, gen2, pick(still_png(), "s.png", Slot::Stopped)));
    assert!(rt.lock().unwrap().picks[1].is_some());
    // 窓口は、選ぶ画面を出す前に世代を控えている（ソースを読む）
    let src = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/commands/avatars.rs")).unwrap();
    let body = &src[src.find("pub async fn avatars_pick_image(").unwrap()..];
    let g = body.find("runtime::pick_generation(&app)").unwrap();
    let d = body.find("blocking_pick_file()").unwrap();
    assert!(g < d);
}

/// 「立ち絵を出さない」（設定・タスクトレイ）。「使う」で「出さない」を選んだのと同じ道で、作った立ち絵は消さない。
/// タスクトレイのメニューはメインのスレッドで走るので、別のスレッドへ渡して画面（WebView）を通らずに済ませる
#[test]
fn hide_avatar_keeps_avatars_and_tray_hands_off_the_main_thread() {
    let d = dirs("hide");
    let p = pick(still_png(), "p.png", Slot::Playing);
    let a = create(&d, "Keep", &p, None).unwrap();
    assert_eq!(check_choice(&d, CHOICE_NONE, VER).unwrap(), "none");
    let mut sk = crate::settings::store::SkinSettings::default();
    sk.set_avatar(&format!("avatar:{}", a.id));
    sk.set_avatar(CHOICE_NONE);
    assert_eq!((sk.avatar.as_str(), sk.character_visible), ("none", false));
    let r = resolve(&sk.avatar, None, false, |id| load(&d, id).map(|x| x.character()), |_| Ok(None));
    assert_eq!(r, Resolution { character: None, fallback: None });
    // 作った立ち絵は残る（あとで「使う」で戻せる）
    assert!(load(&d, &a.id).is_ok());
    assert_eq!(count(&d), 1);

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let runtime = fs::read_to_string(root.join("src/skins/runtime.rs")).unwrap();
    let body = &runtime[runtime.find("pub fn hide_avatar(").unwrap()..];
    let body = &body[..body.find("\n}\n").unwrap()];
    assert!(body.contains("use_avatar(app, avatars::CHOICE_NONE)"), "{body}");
    let tray = fs::read_to_string(root.join("src/platform/windows/tray.rs")).unwrap();
    // メニュー：「既定の見た目に戻す」の次に「立ち絵を出さない」
    assert!(tray.contains(r#"MenuItem::with_id(app, HIDE_AVATAR_ID, "立ち絵を出さない""#));
    assert!(tray.contains("&reset_skin, &hide_avatar,"));
    assert!(tray.contains(r#"pub(crate) const HIDE_AVATAR_ID: &str = "hide-avatar";"#));
    // メニューの処理は、別のスレッドへ渡すだけ（メインのスレッドで設定の保存・通知をしない）
    assert!(tray.contains("HIDE_AVATAR_ID => hide_avatar_off_main(app),"));
    let off = &tray[tray.find("fn hide_avatar_off_main(").unwrap()..];
    let off = &off[..off.find("\n}\n").unwrap()];
    let spawn = off.find("std::thread::Builder::new()").unwrap();
    let call = off.find("crate::skins::runtime::hide_avatar(&app)").unwrap();
    assert!(spawn < call && off[spawn..call].contains(".spawn(move ||"), "{off}");
    assert_eq!(tray.matches("runtime::hide_avatar(").count(), 1);
    // 画面からの窓口も同じ道
    let cmd = fs::read_to_string(root.join("src/commands/avatars.rs")).unwrap();
    assert!(cmd.contains("pub async fn avatars_hide(app: AppHandle) -> AppResult<SkinState> {\n    runtime::hide_avatar(&app)\n}"));
}

fn files_in(dir: &std::path::Path) -> Vec<String> {
    let mut out = vec![];
    for e in fs::read_dir(dir).unwrap().flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if e.path().is_dir() {
            for f in fs::read_dir(e.path()).unwrap().flatten() {
                out.push(format!("{n}/{}", f.file_name().to_string_lossy()));
            }
        } else {
            out.push(n);
        }
    }
    out.sort();
    out
}

/// 「変更」。再生中だけ・停止中だけを差し替える・停止中を外す・名前を変える。差し替えた絵は新しい名前で書き
/// （画面の URL が変わる＝動く画像も読み直される）、使わなくなった絵は消す。確かめは追加と同じ（同じ pick）
#[test]
fn update_replaces_one_image_at_a_time() {
    let d = dirs("update");
    let a = create(&d, "Base", &pick(still_png(), "p.png", Slot::Playing), Some(&pick(webp(&shape(40, 60, [1, 2, 3], true)), "s.webp", Slot::Stopped))).unwrap();
    let dir = d.avatars.join(&a.id);
    let before = load(&d, &a.id).unwrap().character();

    // 再生中だけを差し替える（動く GIF）：停止中はそのまま
    let g = pick(gif_anim(&moving(72, 128, 4), 10), "wave.gif", Slot::Playing);
    let s1 = update(&d, &a.id, "", Some(&g), StoppedChange::Keep).unwrap();
    assert_eq!(s1.name, "Base");
    assert!(s1.animated && s1.has_stopped);
    let c1 = load(&d, &a.id).unwrap().character();
    assert_ne!(c1.playing.path, before.playing.path, "replaced image must get a new path (the screen reloads it)");
    assert_eq!(c1.stopped, before.stopped);
    assert!(c1.playing.animated && c1.playing.still.is_some());
    let f1 = files_in(&dir);
    assert!(!f1.contains(&"playing.png".to_string()), "old playing image is removed: {f1:?}");
    assert_eq!(f1.iter().filter(|f| f.starts_with("playing-") && f.ends_with(".gif")).count(), 1, "{f1:?}");
    assert_eq!(f1.iter().filter(|f| f.starts_with("derived/playing-")).count(), 1, "{f1:?}");
    assert!(f1.contains(&"stopped.webp".to_string()));

    // 停止中だけを差し替える：再生中はそのまま
    let st = pick(still_png(), "rest.png", Slot::Stopped);
    update(&d, &a.id, "", None, StoppedChange::Replace(&st)).unwrap();
    let c2 = load(&d, &a.id).unwrap().character();
    assert_eq!(c2.playing, c1.playing);
    assert_ne!(c2.stopped.path, c1.stopped.path);
    assert!(!files_in(&dir).contains(&"stopped.webp".to_string()));

    // 再生中をもう一度差し替える：前に差し替えた絵（と最初の1コマ）も消える
    update(&d, &a.id, "", Some(&pick(still_png(), "p2.png", Slot::Playing)), StoppedChange::Keep).unwrap();
    let f3 = files_in(&dir);
    assert!(f3.iter().all(|f| !f.ends_with(".gif")), "{f3:?}");
    assert!(f3.iter().all(|f| !f.starts_with("derived/")), "{f3:?}");

    // 停止中を外す（停止中も再生中の絵）＋名前を変える
    let s4 = update(&d, &a.id, "Renamed", None, StoppedChange::Remove).unwrap();
    assert_eq!(s4.name, "Renamed");
    assert!(!s4.has_stopped);
    let c4 = load(&d, &a.id).unwrap().character();
    assert_eq!(c4.playing, c4.stopped);
    let f4 = files_in(&dir);
    assert_eq!(f4.len(), 2, "{f4:?}"); // avatar.json と再生中の絵だけ
    assert!(f4.contains(&"avatar.json".to_string()));

    // 停止中が無いときに外しても何も起きない・差し替えれば足せる
    update(&d, &a.id, "", None, StoppedChange::Remove).unwrap();
    update(&d, &a.id, "", None, StoppedChange::Replace(&pick(still_png(), "x.png", Slot::Stopped))).unwrap();
    assert!(load(&d, &a.id).unwrap().summary().has_stopped);

    // 枠の取り違え（停止中に再生中の画像）を断る
    let wrong = pick(still_png(), "w.png", Slot::Playing);
    assert_eq!(update(&d, &a.id, "", None, StoppedChange::Replace(&wrong)).unwrap_err().kind, ErrorKind::AvatarPickExpired);
    assert_eq!(update(&d, &a.id, "", Some(&pick(still_png(), "w.png", Slot::Stopped)), StoppedChange::Keep).unwrap_err().kind, ErrorKind::AvatarPickExpired);
    assert_eq!(left_in(&d.avatar_staging), 0);
}

/// 「変更」で途中で失敗しても、前の絵と avatar.json が残る（書いたものは消す）
#[test]
fn failed_update_keeps_the_old_images() {
    let d = dirs("update-fail");
    let a = create(&d, "Keep", &pick(still_png(), "p.png", Slot::Playing), None).unwrap();
    let dir = d.avatars.join(&a.id);
    let meta_before = fs::read(dir.join("avatar.json")).unwrap();
    let files_before = files_in(&dir);
    // avatar.json を置き換えられないようにする（同じ名前の一時のファイルの場所をフォルダで塞ぐ）
    fs::create_dir_all(dir.join("avatar.json.tmp").join("block")).unwrap();
    let g = pick(gif_anim(&moving(16, 16, 3), 10), "g.gif", Slot::Playing);
    assert!(update(&d, &a.id, "New", Some(&g), StoppedChange::Keep).is_err());
    fs::remove_dir_all(dir.join("avatar.json.tmp")).unwrap();
    assert_eq!(fs::read(dir.join("avatar.json")).unwrap(), meta_before);
    assert_eq!(files_in(&dir), files_before, "written images are removed when the update fails");
    let l = load(&d, &a.id).unwrap();
    assert_eq!(l.name(), "Keep");
    assert!(l.character().playing.path.ends_with("playing.png"));
    // 塞いだものを外せば、同じ画像で変えられる
    assert!(update(&d, &a.id, "New", Some(&g), StoppedChange::Keep).is_ok());
}

/// 差し替えた名前の形（`<状態>-<16 桁>.<形式>`）だけを受け、ほかの形は読むときに断る
#[test]
fn replaced_file_names_are_checked() {
    assert!(image_file_ok("playing.png", Slot::Playing));
    assert!(image_file_ok("playing-0123456789abcdef.gif", Slot::Playing));
    assert!(image_file_ok("stopped-0123456789abcdef.webp", Slot::Stopped));
    for bad in [
        "playing-0123456789abcdef.gif/../x",
        "playing-../x.png",
        "playing-0123456789ABCDEF.png",
        "playing-0123.png",
        "playing-0123456789abcdef.exe",
        "stopped-0123456789abcdef.png",
        "playing_0123456789abcdef.png",
        "../playing.png",
        "playing",
    ] {
        assert!(!image_file_ok(bad, Slot::Playing), "{bad}");
    }
    assert_eq!(still_for("playing-0123456789abcdef.gif"), "derived/playing-0123456789abcdef.png");
    // 書き換えた avatar.json：差し替えの形の名前でも、最初の1コマが合わなければ断る
    let d = dirs("revnames");
    let a = create(&d, "R", &pick(still_png(), "p.png", Slot::Playing), None).unwrap();
    update(&d, &a.id, "", Some(&pick(gif_anim(&moving(16, 16, 3), 10), "g.gif", Slot::Playing)), StoppedChange::Keep).unwrap();
    let mpath = d.avatars.join(&a.id).join("avatar.json");
    let mut m: serde_json::Value = serde_json::from_slice(&fs::read(&mpath).unwrap()).unwrap();
    m["playing"]["still"] = serde_json::json!("derived/playing.png");
    fs::write(&mpath, serde_json::to_vec(&m).unwrap()).unwrap();
    assert_eq!(load(&d, &a.id).unwrap_err().kind, ErrorKind::AvatarBroken);
}

/// 「変更」の印。指した枠だけを取り出し、指さない枠は残す。どれかが合わなければ何も取り出さない
#[test]
fn update_picks_take_only_named_slots() {
    use crate::skins::runtime::take_some;
    let p = pick(still_png(), "p.png", Slot::Playing);
    let s = pick(still_png(), "s.png", Slot::Stopped);
    let (pt, st) = (p.token.clone(), s.token.clone());
    let mut slots = [Some(p), Some(s)];
    // 停止中だけ
    assert_eq!(take_some(&mut slots, Some("ffffffffffffffff"), Some(&st)).unwrap_err().kind, ErrorKind::AvatarPickExpired);
    assert!(slots.iter().all(|x| x.is_some()));
    let (a, b) = take_some(&mut slots, None, Some(&st)).unwrap();
    assert!(a.is_none() && b.unwrap().token == st);
    assert!(slots[0].is_some() && slots[1].is_none());
    // 何も指さない（名前だけ）：何も取り出さない
    let (a, b) = take_some(&mut slots, None, None).unwrap();
    assert!(a.is_none() && b.is_none() && slots[0].is_some());
    let (a, _) = take_some(&mut slots, Some(&pt), None).unwrap();
    assert_eq!(a.unwrap().token, pt);
    // 窓口：画面からの差し替えも、追加と同じ pick と印（avatars_pick_image）で、取り消しの世代の決まりも同じ
    let src = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/skins/runtime.rs")).unwrap();
    let body = &src[src.find("pub fn update(app: &AppHandle").unwrap()..];
    let body = &body[..body.find("\n}\n").unwrap()];
    assert!(body.contains("take_some(&mut lock(&state).picks, playing, stopped)"));
    assert!(body.contains("refresh_character(app, &state, false)"));
    // 解き直しと通知は、avatar.json を置き換えたあと・古い絵を消す前に呼ぶ関数（after_commit）の中だけ
    let hook = body.find("let after_commit = ||").unwrap();
    let call = body.find("avatars::update_then(").unwrap();
    assert!(hook < call && body[hook..call].contains("refresh_character(app, &state, false)") && body[hook..call].contains("emit(app, &state)"));
    assert_eq!(body.matches("refresh_character(").count(), 1);
}

/// 「変更」で差し替えるとき、avatar.json を置き換えた直後（古い絵を消す前）に呼び出し側の処理を走らせる。
/// その時点では avatar.json は新しい絵を指し、古い絵はまだ残っている。失敗したときは呼ばない
#[test]
fn update_hook_runs_before_old_images_are_removed() {
    use std::cell::Cell;
    let d = dirs("update-order");
    let a = create(&d, "Order", &pick(still_png(), "p.png", Slot::Playing), None).unwrap();
    let dir = d.avatars.join(&a.id);
    let called = Cell::new(false);
    let g = pick(gif_anim(&moving(16, 16, 3), 10), "g.gif", Slot::Playing);
    update_then(&d, &a.id, "", Some(&g), StoppedChange::Keep, || {
        called.set(true);
        // 新しい絵で読める（画面へ当て直す値が作れる）
        let c = load(&d, &a.id).unwrap().character();
        assert!(c.playing.path.contains("playing-") && c.playing.animated);
        // 古い絵はまだ残っている
        assert!(dir.join("playing.png").is_file(), "old image must still exist when the screen is switched");
    })
    .unwrap();
    assert!(called.get());
    assert!(!dir.join("playing.png").exists(), "old image is removed after the hook");
    // 失敗したとき（avatar.json を置き換えられない）は呼ばない
    fs::create_dir_all(dir.join("avatar.json.tmp").join("block")).unwrap();
    let called2 = Cell::new(false);
    let g2 = pick(still_png(), "q.png", Slot::Playing);
    assert!(update_then(&d, &a.id, "", Some(&g2), StoppedChange::Keep, || called2.set(true)).is_err());
    assert!(!called2.get());
}
