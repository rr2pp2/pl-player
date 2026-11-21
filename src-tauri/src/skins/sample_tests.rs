//! 見本の立ち絵「くじら」（v3.9.1）のテスト。新しく入れた人は選ぶ・上げた人は選ばない・消したら戻らない・30 体の上限・
//! 入れられなかったら次の起動でもう一度・印を書けなければ作らない・作業中の印のまま落ちたら入れ直さない・設定の zip の読み込みで印が変わらない・ほかの立ち絵と同じに扱えること。
//! あわせて、配布の zip に入れるスキンの見本（くもくじら・ペンギン。v3.11.0 から全部入り）が今のアプリの取り込みを通り、アイコンが全部使われること。

use std::fs;
use std::path::PathBuf;

use super::*;
use crate::settings::backup;
use crate::skins::avatars::{StoppedChange, CHOICE_SKIN};
use crate::skins::package;
use crate::skins::testkit::*;

const VER: &str = "3.9.1";

fn base(tag: &str) -> PathBuf {
    let b = crate::test_tmp().join(format!("plp-sample-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&b);
    fs::create_dir_all(&b).unwrap();
    b
}

fn dirs(tag: &str) -> Dirs {
    Dirs::new(&base(tag))
}

fn mem_db(tag: &str) -> Db {
    Db::open_memory(&format!("sample_{tag}")).unwrap()
}

fn mark_value(db: &Db) -> Option<serde_json::Value> {
    db::settings::get(db, MARK_KEY).unwrap()
}

fn names(d: &Dirs) -> Vec<String> {
    avatars::list(d).into_iter().map(|a| a.name).collect()
}

fn small_avatar(d: &Dirs, name: &str) -> String {
    let (p, _) = avatars::pick_bytes(png(&shape(16, 24, [90, 90, 200], true)), "p.png", Slot::Playing).unwrap();
    avatars::create(d, name, &p, None).unwrap().id
}

/// 新しく入れた人：くじらを入れ、使う立ち絵にも選んで保存する（初めての起動から右に出る）。二度目からは何もしない
#[test]
fn fresh_install_adds_and_chooses_kujira() {
    let db = mem_db("fresh");
    let d = dirs("fresh");
    let mut s = store::load(&db);
    let out = install_at_start(&db, &d, true, &mut s);
    let Outcome::Added(id) = out else { panic!("{out:?}") };
    assert_eq!(names(&d), [KUJIRA_NAME]);
    assert_eq!(s.skin.avatar, format!("avatar:{id}"));
    assert!(s.skin.character_visible);
    // 保存してある（次の起動でも同じ）
    assert_eq!(store::load(&db).skin.avatar, format!("avatar:{id}"));
    assert_eq!(mark_value(&db), Some(serde_json::json!({ "kujira": "added" })));
    // 使う立ち絵として解ける（スキンは既定のまま）
    let r = avatars::resolve(&s.skin.avatar, None, false, |i| avatars::load(&d, i).map(|a| a.character()), |_| Ok(None));
    assert!(r.fallback.is_none());
    let c = r.character.unwrap();
    assert_eq!((c.playing.width, c.playing.height, c.stopped.width, c.stopped.height), (450, 950, 450, 950));
    // 次の起動（DB がもうある）・もう一度「新しく入れた人」と言われても、入れ直さない
    for fresh in [false, true] {
        let mut again = store::load(&db);
        assert_eq!(install_at_start(&db, &d, fresh, &mut again), Outcome::AlreadyHandled);
        assert_eq!(again, store::load(&db));
    }
    assert_eq!(avatars::count(&d), 1);
}

/// 前の版から上げた人：一覧に足すだけ。使う立ち絵・見た目は変えない（設定を一度も変えていない人も、DB があれば上げた人）
#[test]
fn upgrade_adds_kujira_but_keeps_the_choice() {
    for (i, before) in [None, Some("skin"), Some("none"), Some("skin:quiet-paper"), Some("mine")].into_iter().enumerate() {
        let db = mem_db(&format!("upgrade_{i}"));
        let d = dirs(&format!("upgrade-{i}"));
        let mine = small_avatar(&d, "Mine");
        if let Some(v) = before {
            let mut s = store::load(&db);
            s.skin.set_avatar(&if v == "mine" { format!("avatar:{mine}") } else { v.to_string() });
            store::save(&db, &s).unwrap();
        }
        let raw_before = db::settings::get(&db, "app").unwrap();
        let mut s = store::load(&db);
        let loaded = s.clone();
        assert!(matches!(install_at_start(&db, &d, false, &mut s), Outcome::Added(_)), "{before:?}");
        assert_eq!(s, loaded, "{before:?}");
        // 設定は書かない（値が無かった人は無いまま）
        assert_eq!(db::settings::get(&db, "app").unwrap(), raw_before, "{before:?}");
        assert_eq!(names(&d), ["Mine", KUJIRA_NAME], "{before:?}");
    }
}

/// 使う人が消したら、次の起動でも戻らない（新しく入れた人として起動し直しても）
#[test]
fn deleted_kujira_does_not_come_back() {
    let db = mem_db("deleted");
    let d = dirs("deleted");
    let mut s = store::load(&db);
    let Outcome::Added(id) = install_at_start(&db, &d, true, &mut s) else { panic!() };
    avatars::delete(&d, &id).unwrap();
    for fresh in [false, true] {
        let mut s = store::load(&db);
        assert_eq!(install_at_start(&db, &d, fresh, &mut s), Outcome::AlreadyHandled);
        assert_eq!(avatars::count(&d), 0);
    }
}

/// 30 体が埋まっていたら足さず、入れたことにする（あとで枠を空けても入らない）。新しく入れた人でも選ばない
#[test]
fn full_avatars_skip_kujira_for_good() {
    let db = mem_db("full");
    let d = dirs("full");
    let ids: Vec<String> = (0..MAX_AVATARS).map(|i| small_avatar(&d, &format!("A{i}"))).collect();
    let mut s = store::load(&db);
    assert_eq!(install_at_start(&db, &d, true, &mut s), Outcome::Full);
    assert_eq!(s.skin.avatar, CHOICE_SKIN);
    assert_eq!(avatars::count(&d), MAX_AVATARS);
    assert!(!names(&d).iter().any(|n| n == KUJIRA_NAME));
    assert_eq!(mark_value(&db), Some(serde_json::json!({ "kujira": "full" })));
    avatars::delete(&d, &ids[0]).unwrap();
    assert_eq!(install_once(&db, &d), Outcome::AlreadyHandled);
    assert_eq!(avatars::count(&d), MAX_AVATARS - 1);
}

/// 入れられなかった（立ち絵のフォルダに書けない）ときは印を付けず、次の起動でもう一度試す。そのときは上げた人と同じ（選ばない）
#[test]
fn failed_install_is_retried_without_choosing() {
    let db = mem_db("failed");
    let b = base("failed");
    let d = Dirs::new(&b);
    fs::write(&d.avatars, b"not a folder").unwrap();
    let mut s = store::load(&db);
    assert!(matches!(install_at_start(&db, &d, true, &mut s), Outcome::Failed(_)));
    assert_eq!(s.skin.avatar, CHOICE_SKIN);
    assert_eq!(mark_value(&db), None);
    assert!(db::settings::get(&db, "app").unwrap().is_none());
    fs::remove_file(&d.avatars).unwrap();
    let mut s = store::load(&db);
    assert!(matches!(install_at_start(&db, &d, false, &mut s), Outcome::Added(_)));
    assert_eq!(s.skin.avatar, CHOICE_SKIN);
    assert_eq!(names(&d), [KUJIRA_NAME]);
}

/// settings 表の印への書き込みを断る（ディスクが一杯・DB が壊れたなどの代わり）。`when` は SQLite の条件（NEW・OLD が使える）
fn block_mark(db: &Db, name: &str, ops: &[&str], when: &str) {
    for op in ops {
        let row = if *op == "DELETE" { "OLD" } else { "NEW" };
        db.writer()
            .execute_batch(&format!(
                "CREATE TRIGGER {name}_{op} BEFORE {op} ON settings WHEN {row}.key = '{MARK_KEY}' AND ({when}) \
                 BEGIN SELECT RAISE(ABORT, 'blocked'); END;"
            ))
            .unwrap();
    }
}

fn unblock_mark(db: &Db, name: &str, ops: &[&str]) {
    for op in ops {
        db.writer().execute_batch(&format!("DROP TRIGGER {name}_{op};")).unwrap();
    }
}

/// 印を書けないときは作らない。書けない間は何度起動しても増えない。書けるようになったら上げた人として足す
#[test]
fn unwritable_mark_adds_nothing() {
    let db = mem_db("nomark");
    let d = dirs("nomark");
    block_mark(&db, "nomark", &["INSERT", "UPDATE"], "1");
    for fresh in [true, false, false] {
        let mut s = store::load(&db);
        assert!(matches!(install_at_start(&db, &d, fresh, &mut s), Outcome::Failed(_)));
        assert_eq!(s.skin.avatar, CHOICE_SKIN);
        assert_eq!(avatars::count(&d), 0);
        assert_eq!(mark_value(&db), None);
    }
    assert!(db::settings::get(&db, "app").unwrap().is_none());
    unblock_mark(&db, "nomark", &["INSERT", "UPDATE"]);
    let mut s = store::load(&db);
    assert!(matches!(install_at_start(&db, &d, false, &mut s), Outcome::Added(_)));
    assert_eq!(s.skin.avatar, CHOICE_SKIN);
    assert_eq!(names(&d), [KUJIRA_NAME]);
    assert_eq!(mark_value(&db), Some(serde_json::json!({ "kujira": "added" })));
}

/// 作業中の印が残っていたら（作った直後に落ちたなど）、次の起動では入れ直さない（新しく入れた人として起動しても）
#[test]
fn pending_mark_is_not_retried() {
    let db = mem_db("pending");
    let d = dirs("pending");
    db::settings::set(&db, MARK_KEY, &serde_json::json!({ "kujira": "pending" })).unwrap();
    for fresh in [true, false] {
        let mut s = store::load(&db);
        assert_eq!(install_at_start(&db, &d, fresh, &mut s), Outcome::AlreadyHandled);
        assert_eq!(s.skin.avatar, CHOICE_SKIN);
        assert_eq!(avatars::count(&d), 0);
    }
    assert_eq!(mark_value(&db), Some(serde_json::json!({ "kujira": "pending" })));
}

/// 作れたあと「入れた」の印を書けなかった：作業中の印が残り、次の起動で増えない。この起動の「選ぶ」は今までどおり
#[test]
fn added_mark_unwritable_keeps_pending() {
    let db = mem_db("noadded");
    let d = dirs("noadded");
    block_mark(&db, "noadded", &["INSERT", "UPDATE"], "NEW.value LIKE '%\"added\"%'");
    let mut s = store::load(&db);
    let Outcome::Added(id) = install_at_start(&db, &d, true, &mut s) else { panic!() };
    assert_eq!(s.skin.avatar, format!("avatar:{id}"));
    assert_eq!(mark_value(&db), Some(serde_json::json!({ "kujira": "pending" })));
    for fresh in [true, false] {
        let mut again = store::load(&db);
        assert_eq!(install_at_start(&db, &d, fresh, &mut again), Outcome::AlreadyHandled);
    }
    assert_eq!(avatars::count(&d), 1);
}

/// 作れず、作業中の印も外せなかった：作業中のまま残り、もう試さない（入らない形が残るのは許す）。
/// 外せたときは、ほかの名前の印を残して外す
#[test]
fn failed_install_with_stuck_pending_is_not_retried() {
    let db = mem_db("stuck");
    let d = dirs("stuck");
    fs::write(&d.avatars, b"not a folder").unwrap();
    // 外せたとき：ほかの名前は残る
    db::settings::set(&db, MARK_KEY, &serde_json::json!({ "other": "added" })).unwrap();
    assert!(matches!(install_once(&db, &d), Outcome::Failed(_)));
    assert_eq!(mark_value(&db), Some(serde_json::json!({ "other": "added" })));
    // 外せないとき（キーを消すのを断る）
    db::settings::remove(&db, MARK_KEY).unwrap();
    block_mark(&db, "stuck", &["DELETE"], "1");
    let mut s = store::load(&db);
    assert!(matches!(install_at_start(&db, &d, true, &mut s), Outcome::Failed(_)));
    assert_eq!(mark_value(&db), Some(serde_json::json!({ "kujira": "pending" })));
    unblock_mark(&db, "stuck", &["DELETE"]);
    fs::remove_file(&d.avatars).unwrap();
    let mut s = store::load(&db);
    assert_eq!(install_at_start(&db, &d, false, &mut s), Outcome::AlreadyHandled);
    assert_eq!(avatars::count(&d), 0);
}

/// 印が読めない形（手で書き換えたなど）なら、入れたことにする（二重に入れない側）。ほかの見本の名前は印を消さずに足せる
#[test]
fn unreadable_mark_counts_as_handled() {
    let db = mem_db("mark");
    let d = dirs("mark");
    db::settings::set(&db, MARK_KEY, &serde_json::json!("yes")).unwrap();
    assert_eq!(install_once(&db, &d), Outcome::AlreadyHandled);
    db::settings::set(&db, MARK_KEY, &serde_json::json!({ "other": "added" })).unwrap();
    assert!(matches!(install_once(&db, &d), Outcome::Added(_)));
    assert_eq!(mark_value(&db), Some(serde_json::json!({ "other": "added", "kujira": "added" })));
}

/// 入れたくじらは、使う人が追加した立ち絵と同じに扱える（使う・変更・停止中を外す・出さない・削除）
#[test]
fn kujira_is_an_ordinary_avatar() {
    let db = mem_db("ordinary");
    let d = dirs("ordinary");
    let Outcome::Added(id) = install_once(&db, &d) else { panic!() };
    let a = avatars::load(&d, &id).unwrap();
    assert_eq!(a.name(), KUJIRA_NAME);
    let sum = a.summary();
    assert!(sum.has_stopped && !sum.animated && sum.broken.is_none());
    // 絵はアプリに埋め込んだものをそのまま写す
    let dir = d.avatars.join(&id);
    assert_eq!(fs::read(dir.join("playing.png")).unwrap(), KUJIRA_PLAYING);
    assert_eq!(fs::read(dir.join("stopped.png")).unwrap(), KUJIRA_STOPPED);
    // 使う
    assert_eq!(avatars::check_choice(&d, &format!("avatar:{id}"), VER).unwrap(), format!("avatar:{id}"));
    // 変更：名前・停止中を外す
    let u = avatars::update(&d, &id, "くじら（青）", None, StoppedChange::Remove).unwrap();
    assert_eq!(u.name, "くじら（青）");
    assert!(!u.has_stopped);
    // 出さない（選んでいても出ない）
    let none = avatars::resolve("none", None, false, |i| avatars::load(&d, i).map(|a| a.character()), |_| Ok(None));
    assert!(none.character.is_none());
    // 削除
    avatars::delete(&d, &id).unwrap();
    assert_eq!(avatars::count(&d), 0);
    assert_eq!(install_once(&db, &d), Outcome::AlreadyHandled);
}

/// 設定のバックアップ（zip）：印は zip に入らず、読み込んでも増えない・消えない。読み込んだあとの起動でも入れ直さない
#[test]
fn settings_zip_does_not_touch_the_mark() {
    let bg = base("zip-bg");
    // A：新しく入れた人（くじらを選んでいる）
    let a = mem_db("zip_a");
    let da = dirs("zip-a");
    let mut sa = store::load(&a);
    let Outcome::Added(_) = install_at_start(&a, &da, true, &mut sa) else { panic!() };
    let (entries, _) = backup::export_entries(&a, &sa, &bg).unwrap();
    for e in &entries {
        let text = String::from_utf8_lossy(&e.data);
        assert!(!text.contains(MARK_KEY) && !text.contains("sampleAvatars"), "{}", e.name);
    }
    // B：上げた人でくじらを消した（印はある・くじらは無い）→ A の zip を読み込んでも印は同じ・くじらは戻らない
    let b = mem_db("zip_b");
    let db_ = dirs("zip-b");
    let Outcome::Added(kb) = install_once(&b, &db_) else { panic!() };
    avatars::delete(&db_, &kb).unwrap();
    let mark_b = mark_value(&b);
    backup::import_entries(&b, &entries, &bg).unwrap();
    assert_eq!(mark_value(&b), mark_b);
    assert_eq!(install_once(&b, &db_), Outcome::AlreadyHandled);
    assert_eq!(avatars::count(&db_), 0);
    // C：まだ入れていない DB へ読み込んでも、印は付かない（読み込みは印を作らない。次の起動で上げた人として足す）
    let c = mem_db("zip_c");
    backup::import_entries(&c, &entries, &bg).unwrap();
    assert_eq!(mark_value(&c), None);
    // 印に似た名前を書き足した zip（settings.json の項目・別のファイル）でも、印は変わらない
    let mut forged = entries.clone();
    for e in forged.iter_mut().filter(|e| e.name == "settings.json") {
        let mut v: serde_json::Value = serde_json::from_slice(&e.data).unwrap();
        v["sampleAvatars"] = serde_json::json!({});
        v[MARK_KEY] = serde_json::json!({});
        e.data = serde_json::to_vec(&v).unwrap();
    }
    forged.push(crate::settings::zip::Entry { name: format!("{MARK_KEY}.json"), data: b"{}".to_vec() });
    backup::import_entries(&b, &forged, &bg).unwrap();
    assert_eq!(mark_value(&b), mark_b);
    backup::import_entries(&c, &forged, &bg).unwrap();
    assert_eq!(mark_value(&c), None);
}

/// 新しく入れた人の見分け方：データのフォルダにライブラリの DB があるか
#[test]
fn fresh_install_is_told_by_the_library_db() {
    let b = base("fresh-check");
    assert!(is_fresh_install(&b));
    assert!(is_fresh_install(&b.join("missing-folder")));
    fs::write(b.join("library.db"), b"").unwrap();
    assert!(!is_fresh_install(&b));
}

/// 見本のスキン（v3.11.0 から2つ）：samples\ の中のフォルダ・id・名前・フォルダの外から足すファイル（zip の中の名前, samples\ からの場所）。
/// scripts\release-samples.mts の `SAMPLE_SKINS` と同じ
const SAMPLE_SKINS: [(&str, &str, &str, &[(&str, &str)]); 2] = [
    ("kumokujira", "kumokujira-sample", "くもくじら（見本）", &[("character/playing.png", "kujira/playing.png"), ("character/stopped.png", "kujira/stopped.png")]),
    ("penguin", "penguin-sample", "ペンギン（見本）", &[]),
];

fn samples_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("samples")
}

fn walk(dir: &std::path::Path, prefix: &str, out: &mut Vec<String>) {
    for e in fs::read_dir(dir).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        if e.path().is_dir() {
            walk(&e.path(), &rel, out);
        } else {
            out.push(rel);
        }
    }
}

/// 配布の zip の「見本」に入れるスキンの中身（zip の中の名前, samples\ からの場所）。scripts\release-samples.mts の `skinFileList` と同じ決まり：
/// フォルダの中のファイル全部と足すファイル。skin.json を先頭に、残りは zip の中の名前の順
fn sample_skin_files(dir: &str, extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut names = vec![];
    walk(&samples_root().join(dir), "", &mut names);
    let mut files: Vec<(String, String)> = names.into_iter().map(|n| (n.clone(), format!("{dir}/{n}"))).collect();
    files.extend(extra.iter().map(|(n, r)| (n.to_string(), r.to_string())));
    files.sort_by(|a, b| (a.0 != "skin.json", &a.0).cmp(&(b.0 != "skin.json", &b.0)));
    files
}

fn sample_skin_zip(dir: &str, extra: &[(&str, &str)]) -> Vec<u8> {
    let items: Vec<Z> = sample_skin_files(dir, extra)
        .into_iter()
        .map(|(name, rel)| Z::file(&name, &fs::read(samples_root().join(rel.replace('/', std::path::MAIN_SEPARATOR_STR))).unwrap()).deflated())
        .collect();
    zip(&items)
}

/// 見本のスキンを今のアプリの取り込みに通す。警告なし・34 個のアイコン（状態ごと）とウィンドウ・トレイのアイコンが全部使われる・
/// 両テーマの色が確かめに通る（読みやすさの警告は画面が出すもので、取り込みを止めない。色の値の形が違えばここで断られる）
fn check_sample_skin(bytes: &[u8], tag: &str, id: &str, name: &str) {
    let d = dirs(tag);
    let (insp, staged) = package::inspect_bytes(bytes, &d, VER).unwrap_or_else(|e| panic!("{id}: {:?} {}", e.kind, e.detail));
    assert!(!insp.conflict);
    assert!(insp.warnings.is_empty(), "{id}: {:?}", insp.warnings);
    assert!(insp.ignored.is_empty() && !insp.unwrapped);
    assert_eq!(insp.id, id);
    assert_eq!(insp.name, name);
    assert!(insp.description.starts_with("スキン作りの見本。"), "{}", insp.description);
    assert!(insp.parts.appearance && insp.parts.application && insp.parts.character && !insp.parts.animated);
    // v3.11.0：全部入り。skin.json・ボタンのアイコン 41 枚（34 個の状態ごと）・ウィンドウとトレイ・立ち絵2枚
    assert_eq!(insp.parts.icons, 34, "{id}");
    assert_eq!(insp.file_count, 1 + 41 + 2 + 2, "{id}");
    // 両テーマの色が入っている（形の違う値なら inspect_bytes が断る）
    let a = insp.appearance.as_ref().unwrap();
    assert!(a.dark.as_ref().map(|t| !t.tokens.is_empty()).unwrap_or(false), "{id}: dark");
    assert!(a.light.as_ref().map(|t| !t.tokens.is_empty()).unwrap_or(false), "{id}: light");
    package::commit(&staged, &d, VER).unwrap();
    package::discard(&staged);
    let inst = package::load(&d, id, VER).unwrap();
    let r = inst.resolve();
    // 34 個のアイコンの、状態ごとの絵が全部使われる（拒まれたり落とされたりしない）。手で入れるアイコンの枠と同じ数
    for (n, s) in crate::skins::icons::all_slots() {
        if crate::skins::icons::is_app(n) {
            continue;
        }
        let t = r.icons.get(n).and_then(|m| m.get(s)).unwrap_or_else(|| panic!("{id}: icon {n}/{s} is missing"));
        assert!(t.dark.is_some() && t.light.is_some(), "{id}: {n}/{s}");
    }
    assert_eq!(r.icons.values().map(|m| m.len()).sum::<usize>(), 41, "{id}");
    // ウィンドウとトレイのアイコンは、それぞれの絵（静止画で、正方形に整えられる）
    let (w, wrel) = inst.app_icon_exact(false).unwrap();
    let (t, trel) = inst.app_icon_exact(true).unwrap();
    assert_eq!((wrel.as_str(), trel.as_str()), ("app/window.png", "app/tray.png"));
    assert_eq!(crate::skins::images::app_icon_rgba(&fs::read(&w).unwrap(), &wrel, 256).unwrap().1, 256);
    assert_eq!(crate::skins::images::app_icon_rgba(&fs::read(&t).unwrap(), &trel, 64).unwrap().1, 64);
    let c = r.character.unwrap();
    assert_eq!((c.playing.width, c.playing.height, c.width_px, c.max_height_px), (450, 950, 220, 420));
    // 取り込んだスキンの立ち絵として選べる
    assert_eq!(avatars::check_choice(&d, &format!("skin:{id}"), VER).unwrap(), format!("skin:{id}"));
}

/// スキンの見本（くもくじら・ペンギン）は、今のアプリの確かめを警告なしで通る
#[test]
fn sample_skin_imports() {
    for (dir, id, name, extra) in SAMPLE_SKINS {
        check_sample_skin(&sample_skin_zip(dir, extra), &format!("skin-{dir}"), id, name);
    }
    // くじらの立ち絵は samples\kujira\ の1か所だけ（くもくじらのフォルダには置かない）。ペンギンの立ち絵はアプリに埋め込まない
    assert!(!samples_root().join("kumokujira").join("character").exists());
    let src = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src").join("skins").join("sample.rs")).unwrap();
    assert!(!src.contains("penguin"));
    // 埋め込んだくじらの絵が、くもくじらの zip の絵と同じ
    let files = sample_skin_files("kumokujira", SAMPLE_SKINS[0].3);
    assert!(files.iter().any(|(n, r)| n == "character/playing.png" && r == "kujira/playing.png"));
    assert_eq!(fs::read(samples_root().join("kujira").join("playing.png")).unwrap(), KUJIRA_PLAYING);
    assert_eq!(fs::read(samples_root().join("kujira").join("stopped.png")).unwrap(), KUJIRA_STOPPED);
}

/// 配布の zip に入れたスキンの見本（npm run release が作ったもの）を、今のアプリの確かめに通す。どちらの zip でもよい（id で見分ける）。
/// 走らせ方：`PL_SAMPLE_SKIN_ZIP=<release\PL-Player-<版>\見本\くもくじら.zip か ペンギン.zip> cargo test --lib skins::sample::tests::release_sample_skin_imports -- --ignored`
#[test]
#[ignore]
fn release_sample_skin_imports() {
    let p = PathBuf::from(std::env::var("PL_SAMPLE_SKIN_ZIP").expect("PL_SAMPLE_SKIN_ZIP"));
    let bytes = fs::read(&p).unwrap();
    let d = dirs("release-skin-peek");
    let (insp, staged) = package::inspect_bytes(&bytes, &d, VER).unwrap();
    package::discard(&staged);
    let (_, id, name, _) = SAMPLE_SKINS.iter().find(|s| s.1 == insp.id).unwrap_or_else(|| panic!("not a sample skin: {}", insp.id));
    check_sample_skin(&bytes, "release-skin", id, name);
    println!("ok: {} ({id})", p.file_name().unwrap().to_string_lossy());
}
