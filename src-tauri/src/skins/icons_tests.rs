//! 手で入れるアイコン（v3.11.0）のテスト。画像は testkit がその場で作る図形だけ。
//! 決まりは docs\着せ替え_仕様.md の 5.1：枠・確かめ（スキンのアイコンと同じ）・置き場所・戻すの単位・ドロップ・古い設定からの引き継ぎ。

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::*;
use crate::settings::store::AppSettings;
use crate::skins::testkit::*;

fn dirs(tag: &str) -> Dirs {
    let base: PathBuf = crate::test_tmp().join(format!("plp-icons-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    Dirs::new(&base)
}

fn icon_png() -> Vec<u8> {
    png(&shape(20, 20, [200, 80, 40], true))
}

fn prep(name: &str, state: &str, data: Vec<u8>, file: &str) -> Prepared {
    prepare(name, state, data, file).unwrap_or_else(|e| panic!("{name}/{state} {file}: {:?} {}", e.kind, e.detail))
}

fn prep_err(name: &str, state: &str, data: Vec<u8>, file: &str) -> AppError {
    match prepare(name, state, data, file) {
        Ok(p) => panic!("accepted {p:?}"),
        Err(e) => e,
    }
}

fn icons_of(pairs: &[(&str, &str, &str)]) -> Icons {
    let mut i = Icons::new();
    for (n, s, e) in pairs {
        set(&mut i, n, s, Some(e));
    }
    i
}

/// 枠：ボタンのアイコン 35 個（状態のある4つは default を除いた状態ごと）＋ウィンドウ・トレイ。知らない名前・状態は無い
#[test]
fn slots_follow_the_skin_icon_table() {
    let all = all_slots();
    // 状態の無い 31 個（v3.12.0 で IconShufflePlay を足した）＋ 星2・音量4・リピート3・シャッフル2 ＋ ウィンドウ・トレイ
    assert_eq!(all.len(), 31 + 2 + 4 + 3 + 2 + 2);
    assert_eq!(ICONS.len(), 35);
    for (n, states) in ICONS.iter() {
        let mine = slot_states(n).unwrap();
        if states.len() == 1 {
            assert_eq!(mine, vec!["default"], "{n}");
        } else {
            assert!(!mine.contains(&"default"), "{n}");
            assert_eq!(mine.len(), states.len() - 1, "{n}");
            for s in &mine {
                assert!(states.contains(s), "{n} {s}");
            }
        }
    }
    assert_eq!(slot_states("IconStar").unwrap(), vec!["outline", "filled"]);
    assert_eq!(slot_states("IconVolume").unwrap(), vec!["muted", "zero", "low", "high"]);
    assert_eq!(slot_states("IconRepeat").unwrap(), vec!["off", "all", "one"]);
    assert_eq!(slot_states("IconShuffle").unwrap(), vec!["off", "on"]);
    // v3.12.0：「シャッフル再生」のボタンの枠は状態なし（default だけ）。シャッフルの枠に default は無いまま
    assert_eq!(slot_states("IconShufflePlay").unwrap(), vec!["default"]);
    assert_eq!(slot("IconShufflePlay", "default"), Some(("IconShufflePlay", "default")));
    assert_eq!(slot("IconShuffle", "default"), None);
    assert_eq!(slot("AppWindow", "default"), Some((APP_WINDOW, "default")));
    assert_eq!(slot("AppTray", "default"), Some((APP_TRAY, "default")));
    for (n, s) in [
        ("IconStar", "default"),
        ("IconPlay", "on"),
        ("IconShufflePlay", "on"),
        ("iconshuffleplay", "default"),
        ("iconplay", "default"),
        ("IconHeadphones", "default"),
        ("AppWindow", "playing"),
        ("../IconPlay", "default"),
        ("IconPlay", "../default"),
        ("IconPlay", ""),
        ("", "default"),
    ] {
        assert_eq!(slot(n, s), None, "{n} {s}");
    }
    assert!(is_app("AppWindow") && is_app("AppTray") && !is_app("IconPlay"));
}

/// 設定の値：オブジェクトでなければ空・形の違う値は捨てる。入れられない枠と、png・webp・gif でない形式も捨てる（形式は小文字にそろえる）
#[test]
fn settings_values_are_lenient_and_normalized() {
    use serde_json::json;
    for v in [json!(null), json!("x"), json!([1, 2]), json!(5)] {
        assert!(from_json(&v).is_empty(), "{v}");
    }
    let raw = from_json(&json!({
        "IconPlay": {"default": "PNG"},
        "IconStar": {"filled": "gif", "default": "png", "outline": 5},
        "IconVolume": "png",
        "AppTray": {"default": "webp"},
        "AppWindow": {"default": "jpg"},
        "IconNope": {"default": "png"},
        "IconNext": {"default": "png/../../x"},
    }));
    let n = normalize(&raw);
    assert_eq!(n, icons_of(&[("IconPlay", "default", "png"), ("IconStar", "filled", "gif"), ("AppTray", "default", "webp")]));
    assert_eq!(normalize(&n), n);
    assert_eq!(count(&n), 3);
    // 外すと、空になった名前も消える
    let mut m = n.clone();
    set(&mut m, "IconStar", "filled", None);
    assert!(!m.contains_key("IconStar"));
    set(&mut m, "IconStar", "filled", None);
    assert_eq!(count(&m), 2);
}

/// 確かめはスキンのアイコンと同じ images::check（形式・中身と拡張子・途中で切れた画像・実行ファイル・大きさ・コマ数・長さ）。
/// アプリ・トレイのアイコンは静止画だけ。断る文に、選んだファイルの名前は入らない
#[test]
fn images_are_checked_like_skin_icons() {
    // 通るもの：PNG・WebP・GIF・APNG・動く WebP。動く画像は最初の1コマを作る
    let p = prep("IconPlay", "default", icon_png(), "Play.PNG");
    assert_eq!((p.name, p.state, p.ext), ("IconPlay", "default", "png"));
    assert!(p.still.is_none());
    let g = prep("IconStar", "filled", gif_anim(&moving(20, 20, 4), 10), "star.gif");
    assert!(g.still.is_some());
    prep("IconVolume", "muted", webp(&shape(16, 16, [1, 2, 3], true)), "m.webp");
    assert!(prep("IconRepeat", "one", apng(&moving(16, 16, 3), 100), "r.png").still.is_some());
    assert!(prep("IconShuffle", "on", webp_anim(&moving(16, 16, 3), 100), "s.webp").still.is_some());
    prep("AppWindow", "default", png(&shape(300, 120, [9, 9, 9], false)), "w.png");
    // 1コマだけの GIF は静止画（アプリのアイコンにも使える）
    prep("AppTray", "default", gif_anim(&moving(16, 16, 1), 10), "t.gif");

    let secret = "private-folder-name";
    let named = |ext: &str| format!("{secret}.{ext}");
    for ext in ["jpg", "jpeg", "svg", "bmp", "ico", "exe", ""] {
        assert_eq!(prep_err("IconPlay", "default", icon_png(), &named(ext)).kind, ErrorKind::IconImageFormat, "{ext}");
    }
    let e = prep_err("IconPlay", "default", gif_anim(&moving(8, 8, 2), 10), &named("png"));
    assert_eq!(e.kind, ErrorKind::SkinImageInvalid);
    assert!(!e.detail.contains(secret), "{}", e.detail);
    assert!(e.detail.contains("default.png"), "{}", e.detail);
    let full = icon_png();
    assert_eq!(prep_err("IconPlay", "default", full[..full.len() / 2].to_vec(), "a.png").kind, ErrorKind::SkinImageInvalid);
    let mut pe = b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xff\xff".to_vec();
    pe.resize(256, 0);
    assert_eq!(prep_err("IconPlay", "default", pe, "a.png").kind, ErrorKind::SkinImageInvalid);
    let mut huge = png(&shape(1, 1, [0, 0, 0], false));
    huge[16..20].copy_from_slice(&65_535u32.to_be_bytes());
    huge[20..24].copy_from_slice(&65_535u32.to_be_bytes());
    let c = crate::settings::test_crc32(&huge[12..29]);
    huge[29..33].copy_from_slice(&c.to_be_bytes());
    assert_eq!(prep_err("IconPlay", "default", huge, "a.png").kind, ErrorKind::SkinImageLimit);
    let many: Vec<_> = (0..(images::MAX_FRAMES + 1)).map(|i| shape(4, 4, [200, 0, 0], i % 2 == 0)).collect();
    assert_eq!(prep_err("IconPlay", "default", gif_anim(&many, 0), "a.gif").kind, ErrorKind::SkinImageLimit);
    assert_eq!(prep_err("IconPlay", "default", gif_anim(&moving(8, 8, 2), 5000), "a.gif").kind, ErrorKind::SkinImageLimit);
    let mut big = icon_png();
    big.resize(images::MAX_IMAGE_BYTES as usize + 1, 0);
    assert_eq!(prep_err("IconPlay", "default", big, "a.png").kind, ErrorKind::AvatarTooLarge);
    // アプリ・トレイのアイコンは動く画像を断る（スキンの application と同じ）
    for n in ["AppWindow", "AppTray"] {
        assert_eq!(prep_err(n, "default", gif_anim(&moving(16, 16, 3), 10), "a.gif").kind, ErrorKind::SkinImageInvalid, "{n}");
        assert_eq!(prep_err(n, "default", apng(&moving(16, 16, 3), 100), "a.png").kind, ErrorKind::SkinImageInvalid, "{n}");
    }
    // 枠が違う
    assert_eq!(prep_err("IconStar", "default", icon_png(), "a.png").kind, ErrorKind::IconSlotInvalid);
    let long = "x".repeat(10_000);
    let e = prep_err(&long, "default", icon_png(), "a.png");
    assert!(e.detail.chars().count() < 200, "{}", e.detail.len());
}

/// 読むのはファイル1つだけ（フォルダは断る）。PNG・WebP・GIF でなければ読まない。15MB を超えれば読まない。文にファイルの名前と場所は入れない
#[test]
fn read_file_reads_only_image_files() {
    let d = dirs("read");
    let base = d.icons.parent().unwrap().to_path_buf();
    let secret = base.join("private-name.png");
    fs::write(&secret, icon_png()).unwrap();
    let (data, name) = read_file(&secret).unwrap();
    assert_eq!(data, icon_png());
    assert_eq!(name, "private-name.png");
    let folder = base.join("a-folder.png");
    fs::create_dir_all(&folder).unwrap();
    let e = read_file(&folder).unwrap_err();
    assert_eq!(e.kind, ErrorKind::IconDropInvalid);
    assert!(!e.detail.contains("a-folder"), "{}", e.detail);
    let jpg = base.join("private-name.jpg");
    fs::write(&jpg, b"\xff\xd8\xff").unwrap();
    let e = read_file(&jpg).unwrap_err();
    assert_eq!(e.kind, ErrorKind::IconImageFormat);
    assert!(!e.detail.contains("private-name"), "{}", e.detail);
    let big = base.join("big.png");
    fs::write(&big, vec![0u8; images::MAX_IMAGE_BYTES as usize + 1]).unwrap();
    assert_eq!(read_file(&big).unwrap_err().kind, ErrorKind::AvatarTooLarge);
    let gone = base.join("gone.png");
    let e = read_file(&gone).unwrap_err();
    assert_eq!(e.kind, ErrorKind::FileNotFound);
    assert!(!e.detail.contains("gone.png"), "{}", e.detail);
}

/// 置き場所：icons\<名前>\<状態>.<形式>（動く画像の最初の1コマは icons\<名前>\derived\<状態>.png）。入れ直すと前の画像は置き換わり、
/// ほかの形式と使わなくなった最初の1コマは消える。一時の場所に何も残らない。元のファイルは使い続けない（写して置く）
#[test]
fn files_are_copied_into_the_icons_folder() {
    let d = dirs("write");
    let src = d.icons.parent().unwrap().join("from-user.png");
    fs::write(&src, icon_png()).unwrap();
    let (data, name) = read_file(&src).unwrap();
    write(&d, &prep("IconPlay", "default", data, &name)).unwrap();
    let f = d.icons.join("IconPlay").join("default.png");
    assert_eq!(fs::read(&f).unwrap(), icon_png());
    // 元のファイルを消しても、写したものは残る
    fs::remove_file(&src).unwrap();
    let chosen = icons_of(&[("IconPlay", "default", "png")]);
    let (ok, missing) = resolve(&d, &chosen);
    assert!(missing.is_empty());
    let m = &ok["IconPlay"]["default"];
    assert_eq!(PathBuf::from(&m.path), f);
    assert!(m.still.is_none());
    let rev1 = m.rev.clone();
    // 動く画像で入れ直す：形式が変わり、前の png は消え、最初の1コマができる。印も変わる
    std::thread::sleep(Duration::from_millis(20));
    write(&d, &prep("IconPlay", "default", gif_anim(&moving(20, 20, 3), 10), "x.gif")).unwrap();
    assert!(!f.exists());
    assert!(d.icons.join("IconPlay").join("default.gif").is_file());
    assert!(d.icons.join("IconPlay").join("derived").join("default.png").is_file());
    let (ok, _) = resolve(&d, &icons_of(&[("IconPlay", "default", "gif")]));
    let m = &ok["IconPlay"]["default"];
    assert!(m.still.as_deref().unwrap().ends_with("default.png"));
    assert_ne!(m.rev, rev1);
    // 静止画で入れ直すと、使わなくなった最初の1コマも消える
    write(&d, &prep("IconPlay", "default", webp(&shape(16, 16, [5, 5, 5], true)), "y.webp")).unwrap();
    assert!(!d.icons.join("IconPlay").join("default.gif").exists());
    assert!(!d.icons.join("IconPlay").join("derived").join("default.png").exists());
    // 状態ごとに別のファイル
    write(&d, &prep("IconStar", "filled", icon_png(), "a.png")).unwrap();
    write(&d, &prep("IconStar", "outline", icon_png(), "b.png")).unwrap();
    assert!(d.icons.join("IconStar").join("filled.png").is_file() && d.icons.join("IconStar").join("outline.png").is_file());
    // 一時の場所には何も残らない
    assert_eq!(fs::read_dir(&d.icon_staging).map(|r| r.count()).unwrap_or(0), 0);
    // 書いたファイルの名前は、アプリが決めたものだけ（元のファイル名は使わない）
    let mut names: Vec<String> = vec![];
    for e in fs::read_dir(&d.icons).unwrap().flatten() {
        for f in fs::read_dir(e.path()).unwrap().flatten() {
            names.push(format!("{}/{}", e.file_name().to_string_lossy(), f.file_name().to_string_lossy()));
        }
    }
    names.sort();
    assert_eq!(names, ["IconPlay/default.webp", "IconStar/filled.png", "IconStar/outline.png"]);
}

/// 読めない枠：消えた・壊れた（中身と拡張子が違う・実行ファイル）・入れたのと違う形式・リンク。読めた枠と分けて返す
#[test]
fn resolve_reports_missing_and_broken_files() {
    let d = dirs("resolve");
    write(&d, &prep("IconPlay", "default", icon_png(), "a.png")).unwrap();
    write(&d, &prep("IconNext", "default", icon_png(), "a.png")).unwrap();
    write(&d, &prep("IconPrev", "default", icon_png(), "a.png")).unwrap();
    fs::write(d.icons.join("IconNext").join("default.png"), b"GIF89a....").unwrap();
    fs::remove_file(d.icons.join("IconPrev").join("default.png")).unwrap();
    let chosen = icons_of(&[
        ("IconPlay", "default", "png"),
        ("IconNext", "default", "png"),
        ("IconPrev", "default", "png"),
        ("IconPause", "default", "webp"),
    ]);
    let (ok, mut missing) = resolve(&d, &chosen);
    assert_eq!(ok.keys().collect::<Vec<_>>(), vec!["IconPlay"]);
    missing.sort();
    assert_eq!(
        missing,
        vec![
            ("IconNext".to_string(), "default".to_string(), "png".to_string()),
            ("IconPause".to_string(), "default".to_string(), "webp".to_string()),
            ("IconPrev".to_string(), "default".to_string(), "png".to_string()),
        ]
    );
    // 形式が違う（png を入れたのに設定は gif）
    let (ok, missing) = resolve(&d, &icons_of(&[("IconPlay", "default", "gif")]));
    assert!(ok.is_empty() && missing.len() == 1);
    // アプリのアイコンのファイル
    write(&d, &prep("AppTray", "default", icon_png(), "t.png")).unwrap();
    let both = icons_of(&[("AppTray", "default", "png"), ("AppWindow", "default", "png")]);
    let (p, rel) = app_file(&d, &both, APP_TRAY).unwrap();
    assert_eq!(p, d.icons.join("AppTray").join("default.png"));
    assert_eq!(rel, "default.png");
    assert!(app_file(&d, &both, APP_WINDOW).is_none());

    // ジャンクション（管理者の権限は要らない）。作れなければ、その確かめだけ飛ばす
    let outside = d.icons.parent().unwrap().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("default.png"), icon_png()).unwrap();
    let junction = d.icons.join("IconPause");
    let made = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&junction).arg(&outside).output().map(|o| o.status.success()).unwrap_or(false);
    println!("junction made: {made}");
    if made {
        let (ok, missing) = resolve(&d, &icons_of(&[("IconPause", "default", "png")]));
        assert!(ok.is_empty() && missing.len() == 1, "a link inside icons must not be followed");
        // 外すと、リンクだけを外す（先の中身は残る）
        remove(&d, "IconPause", "default");
        assert!(!junction.exists());
        assert!(outside.join("default.png").is_file());
        // icons\ そのものがリンク：何も読まない・全部消すときもリンクだけを外す
        let d2 = dirs("resolve-link");
        let real = d2.icons.parent().unwrap().join("real-icons");
        fs::create_dir_all(real.join("IconPlay")).unwrap();
        fs::write(real.join("IconPlay").join("default.png"), icon_png()).unwrap();
        let ok2 = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&d2.icons).arg(&real).output().map(|o| o.status.success()).unwrap_or(false);
        if ok2 {
            let (ok, missing) = resolve(&d2, &icons_of(&[("IconPlay", "default", "png")]));
            assert!(ok.is_empty() && missing.len() == 1);
            assert_eq!(write(&d2, &prep("IconNext", "default", icon_png(), "a.png")).unwrap_err().kind, ErrorKind::SkinPathUnsafe);
            remove_all(&d2);
            assert!(!d2.icons.exists());
            assert!(real.join("IconPlay").join("default.png").is_file(), "removing must not follow the link");
        }
    }
}

/// 「戻す」の単位（ファイル）：枠の「戻す」はその枠のどの形式も・最初の1コマも消す（ほかの状態は残す）。全部戻すは icons\ ごと
#[test]
fn remove_one_and_all() {
    let d = dirs("remove");
    write(&d, &prep("IconStar", "filled", gif_anim(&moving(16, 16, 2), 10), "a.gif")).unwrap();
    write(&d, &prep("IconStar", "outline", icon_png(), "b.png")).unwrap();
    write(&d, &prep("IconPlay", "default", icon_png(), "c.png")).unwrap();
    remove(&d, "IconStar", "filled");
    assert!(!d.icons.join("IconStar").join("filled.gif").exists());
    assert!(!d.icons.join("IconStar").join("derived").exists());
    assert!(d.icons.join("IconStar").join("outline.png").is_file());
    remove(&d, "IconStar", "outline");
    assert!(!d.icons.join("IconStar").exists(), "empty folders are removed");
    // 無い枠・知らない枠を消しても何もしない
    remove(&d, "IconStar", "outline");
    remove(&d, "../IconPlay", "default");
    assert!(d.icons.join("IconPlay").join("default.png").is_file());
    fs::create_dir_all(&d.icon_staging).unwrap();
    fs::write(d.icon_staging.join("left.png"), b"x").unwrap();
    remove_all(&d);
    assert!(!d.icons.exists() && !d.icon_staging.exists());
    remove_all(&d);
}

/// ドロップ：画面が知らせた位置と同じ、新しい記録だけを使う。落としたものがファイル1つでなければ断る（複数・ファイルでないもの）
#[test]
fn dropped_files_are_taken_by_position() {
    let now = Instant::now();
    let one = |paths: Vec<&str>, x: f64, y: f64, age: u64| {
        Some(Dropped { paths: paths.into_iter().map(PathBuf::from).collect(), x, y, at: now - Duration::from_secs(age) })
    };
    assert_eq!(pick_dropped(one(vec!["C:\\a\\b.png"], 120.0, 340.0, 0), 120.0, 340.0, now).unwrap(), PathBuf::from("C:\\a\\b.png"));
    for (d, x, y, why) in [
        (one(vec!["a.png", "b.png"], 1.0, 2.0, 0), 1.0, 2.0, "two files"),
        (one(vec![], 1.0, 2.0, 0), 1.0, 2.0, "nothing that is a file"),
        (one(vec!["a.png"], 1.0, 2.0, 0), 1.0, 3.0, "another position"),
        (one(vec!["a.png"], 1.0, 2.0, 31), 1.0, 2.0, "too old"),
        (None, 1.0, 2.0, "no drop"),
    ] {
        assert_eq!(pick_dropped(d, x, y, now).unwrap_err().kind, ErrorKind::IconDropInvalid, "{why}");
    }
    // 記録して取り出すと消える（2回目は使えない）
    record_drop(&[PathBuf::from("x.png")], 5.0, 6.0);
    assert_eq!(take_drop(5.0, 6.0).unwrap(), PathBuf::from("x.png"));
    assert_eq!(take_drop(5.0, 6.0).unwrap_err().kind, ErrorKind::IconDropInvalid);
}

/// 位置の合わない問い合わせでは控えを消さない。窓口が届く前に続けて2回落としても、どちらも取り出せる。
/// 控えは新しいものから 8 つまでで、30 秒より古いものは落とす
#[test]
fn drops_are_kept_until_their_own_slot_takes_them() {
    let now = Instant::now();
    let at = |p: &str, x: f64, age: u64| Dropped { paths: vec![PathBuf::from(p)], x, y: 10.0, at: now - Duration::from_secs(age) };
    let mut list = vec![];
    push_drop(&mut list, at("a.png", 1.0, 0));
    push_drop(&mut list, at("b.png", 2.0, 0));
    // 位置の違う問い合わせ：何も取り出さず、控えも減らない
    assert!(take_matching(&mut list, 3.0, 10.0, now).is_none());
    assert_eq!(list.len(), 2);
    // 1回目のドロップの窓口が、2回目のあとに届いても取り出せる（順番は問わない）
    assert_eq!(take_matching(&mut list, 1.0, 10.0, now).unwrap().paths[0], PathBuf::from("a.png"));
    assert_eq!(take_matching(&mut list, 2.0, 10.0, now).unwrap().paths[0], PathBuf::from("b.png"));
    assert!(list.is_empty());
    // 同じ位置に2回落としたら、新しいほうから
    push_drop(&mut list, at("old.png", 1.0, 0));
    push_drop(&mut list, at("new.png", 1.0, 0));
    assert_eq!(take_matching(&mut list, 1.0, 10.0, now).unwrap().paths[0], PathBuf::from("new.png"));
    // 古いもの・数を超えたものは落とす
    let mut list = vec![];
    push_drop(&mut list, Dropped { paths: vec![], x: 0.0, y: 0.0, at: now - Duration::from_secs(40) });
    for i in 0..10 {
        push_drop(&mut list, at(&format!("{i}.png"), i as f64, 0));
    }
    assert_eq!(list.len(), DROPS_KEPT);
    assert_eq!(list[0].paths[0], PathBuf::from("2.png"));
    assert!(take_matching(&mut list, 0.0, 0.0, now).is_none());
    assert!(take_matching(&mut list, 1.0, 10.0, now).is_none());
    // 本物の控え：位置違いの問い合わせのあとでも、合う位置で取り出せる
    record_drop(&[PathBuf::from("p.png")], 50.0, 60.0);
    record_drop(&[PathBuf::from("q.png")], 70.0, 80.0);
    assert_eq!(take_drop(1.0, 1.0).unwrap_err().kind, ErrorKind::IconDropInvalid);
    assert_eq!(take_drop(50.0, 60.0).unwrap(), PathBuf::from("p.png"));
    assert_eq!(take_drop(70.0, 80.0).unwrap(), PathBuf::from("q.png"));
}

/// 画面へ返す断りの文は、種別だけ（英語の詳しいところ・アプリが決めた置き場所の名前・一時の場所のパスを出さない）
#[test]
fn refusals_for_the_screen_have_no_internal_detail() {
    let d = dirs("screen");
    let base = d.icons.parent().unwrap().to_path_buf();
    let jpg = base.join("photo.jpg");
    fs::write(&jpg, b"\xff\xd8\xff").unwrap();
    let fake = base.join("fake.png");
    fs::write(&fake, gif_anim(&moving(8, 8, 2), 10)).unwrap();
    let errs = [
        read_file(&jpg).unwrap_err(),
        prepare("IconPlay", "default", fs::read(&fake).unwrap(), "fake.png").unwrap_err(),
        prepare("AppTray", "default", gif_anim(&moving(8, 8, 3), 10), "a.gif").unwrap_err(),
        read_file(&base).unwrap_err(),
        pick_dropped(None, 1.0, 1.0, Instant::now()).unwrap_err(),
    ];
    let kinds: Vec<_> = errs.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [ErrorKind::IconImageFormat, ErrorKind::SkinImageInvalid, ErrorKind::SkinImageInvalid, ErrorKind::IconDropInvalid, ErrorKind::IconDropInvalid]);
    for e in errs {
        assert!(!e.detail.is_empty(), "the log keeps the detail");
        let shown = for_screen(e.clone());
        assert_eq!(shown.kind, e.kind);
        assert_eq!(shown.detail, "");
    }
    // 窓口とその先が通す（ソースを読む）
    let rt = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/skins/runtime.rs")).unwrap();
    for needle in ["icons::read_file(path).map_err(icons::for_screen)?", "icons::prepare(n, s, data, &file_name).map_err(icons::for_screen)?", "icons::write(&dirs(&state), &prepared).map_err(icons::for_screen)?"] {
        assert!(rt.contains(needle), "{needle}");
    }
    let cmd = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands/icons.rs")).unwrap();
    assert!(cmd.contains("icons::take_drop(x, y).map_err(icons::for_screen)?"));
}

/// 読めなかった枠は、少し待ってもう1回読む（その間に読めるようになれば外さない。読めないままなら外す）
#[test]
fn unreadable_icons_are_read_again_before_removal() {
    let d = dirs("retry");
    write(&d, &prep("IconPlay", "default", icon_png(), "a.png")).unwrap();
    write(&d, &prep("IconNext", "default", icon_png(), "a.png")).unwrap();
    let late = d.icons.join("IconNext").join("default.png");
    let saved = fs::read(&late).unwrap();
    fs::remove_file(&late).unwrap();
    let chosen = icons_of(&[("IconPlay", "default", "png"), ("IconNext", "default", "png"), ("IconPrev", "default", "png")]);
    let writer = {
        let late = late.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            fs::write(&late, saved).unwrap();
        })
    };
    let (ok, missing) = resolve_retrying(&d, &chosen, Duration::from_millis(400));
    writer.join().unwrap();
    assert!(ok.contains_key("IconPlay") && ok.contains_key("IconNext"), "{:?}", ok.keys().collect::<Vec<_>>());
    assert_eq!(missing, vec![("IconPrev".to_string(), "default".to_string(), "png".to_string())]);
    // 全部読めれば待たない
    let t = Instant::now();
    let (_, none) = resolve_retrying(&d, &icons_of(&[("IconPlay", "default", "png")]), Duration::from_secs(5));
    assert!(none.is_empty() && t.elapsed() < Duration::from_secs(2));
}

/// 設定の zip で枠が置き換わったあと、設定に無い枠の画像（形式違い・最初の1コマも）を消す。設定にある枠と、
/// アプリが書かない名前のファイルは残す。リンクは辿らない
#[test]
fn unused_images_are_removed_after_the_choice_changes() {
    let d = dirs("unused");
    write(&d, &prep("IconPause", "default", gif_anim(&moving(16, 16, 2), 10), "a.gif")).unwrap();
    write(&d, &prep("IconPlay", "default", icon_png(), "a.png")).unwrap();
    write(&d, &prep("IconStar", "filled", icon_png(), "a.png")).unwrap();
    write(&d, &prep("IconStar", "outline", gif_anim(&moving(16, 16, 2), 10), "b.gif")).unwrap();
    fs::write(d.icons.join("IconPlay").join("notes.txt"), b"x").unwrap();
    // zip の値：IconPlay は形式違い（webp）、IconStar は filled だけ
    let chosen = icons_of(&[("IconPlay", "default", "webp"), ("IconStar", "filled", "png")]);
    remove_unused(&d, &chosen);
    assert!(!d.icons.join("IconPause").exists(), "a slot not in the settings is removed with its still frame");
    assert!(!d.icons.join("IconPlay").join("default.png").exists(), "another format is removed");
    assert!(d.icons.join("IconPlay").join("notes.txt").is_file(), "files the app does not write are left");
    assert!(d.icons.join("IconStar").join("filled.png").is_file());
    assert!(!d.icons.join("IconStar").join("outline.gif").exists());
    assert!(!d.icons.join("IconStar").join("derived").exists());
    // 空の設定：全部消える（アプリが書かないファイルの入ったフォルダは残る）
    remove_unused(&d, &Icons::new());
    assert!(!d.icons.join("IconStar").exists());
    // icons\ が無くても何もしない
    let empty = dirs("unused-none");
    remove_unused(&empty, &Icons::new());
    // ジャンクションの枠のフォルダ：設定に無ければリンクだけを外す（先は残る）
    let outside = d.icons.parent().unwrap().join("outside-unused");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("default.png"), icon_png()).unwrap();
    let j = d.icons.join("IconNext");
    if std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&j).arg(&outside).output().map(|o| o.status.success()).unwrap_or(false) {
        remove_unused(&d, &Icons::new());
        assert!(!j.exists());
        assert!(outside.join("default.png").is_file());
    }
}

/// アプリのアイコンの重ね方：ウィンドウは 手 → スキン → 組み込み。トレイは 手のトレイ → スキンのトレイ → 決まったウィンドウの画像
#[test]
fn app_icons_are_layered() {
    use crate::skins::runtime::layer_app_icons;
    let l = |mw: Option<&'static str>, mt: Option<&'static str>, sw: Option<&'static str>, st: Option<&'static str>| layer_app_icons(mw, mt, sw, st);
    assert_eq!(l(None, None, None, None), (None, None));
    assert_eq!(l(None, None, Some("sw"), None), (Some("sw"), Some("sw")));
    assert_eq!(l(None, None, Some("sw"), Some("st")), (Some("sw"), Some("st")));
    assert_eq!(l(Some("mw"), None, Some("sw"), Some("st")), (Some("mw"), Some("st")));
    assert_eq!(l(Some("mw"), None, Some("sw"), None), (Some("mw"), Some("mw")));
    assert_eq!(l(None, Some("mt"), Some("sw"), Some("st")), (Some("sw"), Some("mt")));
    assert_eq!(l(None, Some("mt"), None, None), (None, Some("mt")));
    assert_eq!(l(Some("mw"), Some("mt"), Some("sw"), Some("st")), (Some("mw"), Some("mt")));
}

/// 「戻す」の単位（設定）：スキンを替えても手のアイコンは残る・「既定の見た目に戻す」は手のアイコンも手の色も外し、立ち絵は残す・
/// 画面から来た設定の保存では変わらない
#[test]
fn what_each_operation_keeps() {
    use crate::skins::runtime::{reset_look_settings, with_active};
    let mut s = AppSettings::default();
    s.skin.active_skin_id = "quiet-paper".into();
    s.skin.set_avatar("avatar:0123456789abcdef");
    s.skin.colors.insert("--c-accent".into(), "#123456".into());
    s.skin.icons = icons_of(&[("IconPlay", "default", "png"), ("AppTray", "default", "webp")]);
    let t = with_active(s.clone(), "other-skin");
    assert_eq!(t.skin.icons, s.skin.icons);
    let t = with_active(s.clone(), crate::skins::names::BUILTIN_ID);
    assert_eq!(t.skin.icons, s.skin.icons);
    let r = reset_look_settings(s.clone());
    assert!(r.skin.icons.is_empty() && r.skin.colors.is_empty());
    assert_eq!(r.skin.active_skin_id, crate::skins::names::BUILTIN_ID);
    assert_eq!(r.skin.avatar, "avatar:0123456789abcdef");
    // 画面から来た設定（古い写し・細工した値）では変わらない
    let bg = std::path::Path::new(r"C:\data\backgrounds");
    for icons in [
        serde_json::json!({}),
        serde_json::json!({"IconNext": {"default": "png"}}),
        serde_json::json!({"IconPlay": {"default": "gif"}}),
        serde_json::json!({"IconPlay": {"default": "../../x"}}),
    ] {
        let mut from = s.clone();
        from.skin.icons = from_json(&icons);
        from.theme = "light".into();
        let out = crate::commands::settings::from_screen(from, &s, bg).sanitize();
        assert_eq!(out.skin.icons, s.skin.icons, "{icons}");
        assert_eq!(out.theme, "light");
    }
}

/// 古い設定からの引き継ぎ：3.10.0 の設定（icons が無い）は手のアイコンなし。形の違う値はその枠だけ捨てる・
/// オブジェクトでない値は手のアイコンなし（設定全体は既定に戻さない）。DB に保存して読み直す
#[test]
fn upgrade_from_3_10_0_and_saved_shape() {
    use crate::db::Db;
    use crate::settings::store;
    let read = |j: &str| serde_json::from_str::<AppSettings>(j).unwrap_or_else(|e| panic!("{j}: {e}")).sanitize();
    assert!(AppSettings::default().skin.icons.is_empty());
    let old = read(r##"{"theme":"light","skin":{"activeSkinId":"shape-test","avatar":"none","characterVisible":false,"characterScale":1.5,"safeModeNextStart":false,"colors":{"--c-bg":"#000000"}}}"##);
    assert!(old.skin.icons.is_empty());
    assert_eq!((old.theme.as_str(), old.skin.avatar.as_str(), old.skin.colors.len()), ("light", "none", 1));
    for bad in ["null", "5", "\"IconPlay\"", "true", r#"["IconPlay"]"#] {
        let s = read(&format!(r#"{{"theme":"light","skin":{{"icons":{bad},"characterScale":0.8}}}}"#));
        assert!(s.skin.icons.is_empty(), "{bad}");
        assert_eq!(s.theme, "light", "{bad}");
        assert!((s.skin.character_scale - 0.8).abs() < 1e-6, "{bad}");
    }
    let s = read(r#"{"skin":{"icons":{"IconPlay":{"default":"png"},"IconStar":{"default":"png","filled":"WEBP"},"IconX":{"default":7}}}}"#);
    assert_eq!(s.skin.icons, icons_of(&[("IconPlay", "default", "png"), ("IconStar", "filled", "webp")]));
    let db = Db::open_memory("icons_store").unwrap();
    store::save(&db, &s).unwrap();
    assert_eq!(store::load(&db).skin.icons, s.skin.icons);
    assert!(serde_json::to_string(&AppSettings::default()).unwrap().contains(r#""icons":{}"#));
}
