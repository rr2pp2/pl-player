//! 実際の出力デバイスを使う確認（音量 0 で鳴らす）。出力デバイスが無い PC では飛ばす。
//! 走らせ方：cargo test --features test-support --test device_playback -- --ignored --test-threads=1 --nocapture
//! （v3.3.0 から機能 `test-support` が要る。ごみ箱へ入れたテストのファイルを、ごみ箱からも消す処理を使うため。配布物には入らない）

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pl_player_lib::audio::devices::list_output_devices;
use pl_player_lib::audio::engine::single_output;
use pl_player_lib::audio::{AudioEngine, Command, Event};
use pl_player_lib::db::{self, Db};
use pl_player_lib::library::import;
use pl_player_lib::playback::order::Repeat;
use pl_player_lib::playback::{Controller, PlaybackListener, Snapshot, Status};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)
}

fn has_device() -> bool {
    let d = list_output_devices();
    println!("output devices: {}", d.iter().map(|x| x.name.clone()).collect::<Vec<_>>().join(" / "));
    !d.is_empty()
}

fn wait_for<F: FnMut(&Event) -> bool>(rx: &crossbeam_channel::Receiver<Event>, secs: u64, mut f: F) -> Vec<Event> {
    let end = Instant::now() + Duration::from_secs(secs);
    let mut seen = vec![];
    while Instant::now() < end {
        if let Ok(e) = rx.recv_timeout(Duration::from_millis(50)) {
            let hit = f(&e);
            seen.push(e);
            if hit {
                return seen;
            }
        }
    }
    panic!("timeout; seen: {seen:?}");
}

/// 7形式すべて（v3.1.2 から HE-AAC の m4a も。Media Foundation で読む）を実際のデバイスで最後まで鳴らす（位置が進み、終わりの通知が来る）
#[test]
#[ignore]
fn plays_all_formats_to_the_end() {
    if !has_device() {
        return;
    }
    let engine = AudioEngine::start_single(0.0, None);
    let rx = engine.events();
    for (i, name) in ["sine.mp3", "sine.wav", "sine.flac", "sine_aaclc.m4a", "sine_alac.m4a", "sine.aac", "sine.ogg", "sine.opus", "mono_22k.wav", "sine_heaac.m4a"]
        .iter()
        .enumerate()
    {
        let token = i as u64 + 1;
        let t0 = Instant::now();
        engine.send(Command::Load { path: fixture(name), start_ms: 2000, play: true, token });
        let mut max_pos = 0;
        wait_for(&rx, 10, |e| match e {
            Event::Position { token: t, position_ms, .. } if *t == token => {
                max_pos = max_pos.max(*position_ms);
                false
            }
            Event::Ended { token: t } => *t == token,
            Event::Error { error, .. } => panic!("{name}: {error}"),
            _ => false,
        });
        let el = t0.elapsed().as_millis();
        println!("{name}: ended after {el} ms (max position {max_pos} ms)");
        // 2秒目から始めたので、約1秒で終わる
        assert!((600..=3000).contains(&el), "{name}: {el} ms");
        assert!(max_pos >= 2000, "{name}: position {max_pos}");
    }
}

/// 再生中に出力先を変えても、曲と位置が保たれる（v3.1.2：Media Foundation で読む HE-AAC も）
#[test]
#[ignore]
fn switching_device_keeps_position() {
    for name in ["sine.flac", "sine_heaac.m4a"] {
        println!("== {name}");
        switching_device_keeps_position_for(name);
    }
}

fn switching_device_keeps_position_for(name: &str) {
    let devs = list_output_devices();
    if devs.is_empty() {
        return;
    }
    // 長めの曲（3秒の曲を、0.5秒から）
    let engine = AudioEngine::start_single(0.0, None);
    let rx = engine.events();
    engine.send(Command::Load { path: fixture(name), start_ms: 500, play: true, token: 7 });
    let mut before_switch = 0;
    wait_for(&rx, 5, |e| match e {
        Event::Position { position_ms, .. } => {
            before_switch = *position_ms;
            *position_ms >= 1000
        }
        _ => false,
    });
    // 別のデバイスがあればそれへ、無ければ同じデバイスを ID で選び直す
    let target = devs.iter().find(|d| !d.is_default).unwrap_or(&devs[0]).clone();
    println!("switch to {}", target.name);
    // v3.8.0：出力先の枠を入れ替える（3.7.0 の SetDevice の代わり）。新しい出力は途中から入る
    engine.send(Command::SetOutputs(single_output(Some(target.id.clone()))));
    let seen = wait_for(&rx, 5, |e| match e {
        Event::Position { position_ms, .. } => {
            before_switch = before_switch.max(*position_ms);
            false
        }
        Event::Output { status: pl_player_lib::audio::OutputStatus::Ok, name: Some(_), .. } => true,
        Event::Error { error, .. } => panic!("{error}"),
        _ => false,
    });
    println!("events until switch: {}", seen.len());
    // 切り替えのあとも、位置は前へ進み続ける（0 に戻らない）
    let mut after: Vec<u64> = vec![];
    wait_for(&rx, 5, |e| match e {
        Event::Position { position_ms, .. } => {
            after.push(*position_ms);
            after.len() >= 2
        }
        Event::Ended { .. } => true,
        _ => false,
    });
    println!("before {before_switch} ms, after {after:?}");
    if let Some(first) = after.first() {
        assert!(*first + 300 >= before_switch, "position went back: {before_switch} -> {first}");
    }
    // 最後まで鳴り切る
    wait_for(&rx, 5, |e| matches!(e, Event::Ended { token: 7 }));
}

/// 一時停止・シーク・再開（v3.1.2：Media Foundation で読む HE-AAC も）
#[test]
#[ignore]
fn pause_seek_resume() {
    for name in ["sine.ogg", "sine_heaac.m4a"] {
        println!("== {name}");
        pause_seek_resume_for(name);
    }
}

fn pause_seek_resume_for(name: &str) {
    if !has_device() {
        return;
    }
    let engine = AudioEngine::start_single(0.0, None);
    let rx = engine.events();
    engine.send(Command::Load { path: fixture(name), start_ms: 0, play: true, token: 1 });
    wait_for(&rx, 5, |e| matches!(e, Event::Position { position_ms, .. } if *position_ms >= 500));
    engine.send(Command::Pause);
    std::thread::sleep(Duration::from_millis(400));
    // 一時停止までの通知は読み捨てる
    while rx.try_recv().is_ok() {}
    engine.send(Command::Seek(2500));
    let mut p = 0;
    wait_for(&rx, 3, |e| match e {
        Event::Position { position_ms, .. } => {
            p = *position_ms;
            true
        }
        _ => false,
    });
    assert!((2400..=2600).contains(&p), "after seek: {p}");
    engine.send(Command::Play);
    wait_for(&rx, 5, |e| matches!(e, Event::Ended { token: 1 }));
}

struct Collect(std::sync::Mutex<Vec<Snapshot>>);
impl PlaybackListener for Collect {
    fn on_snapshot(&self, s: &Snapshot) {
        self.0.lock().unwrap().push(s.clone());
    }
}

/// 司令塔：取り込み → 並びで再生 → 自動で次へ → キューが先 → 終わりで止まる → 状態の保存と復元
#[test]
#[ignore]
fn controller_end_to_end() {
    if !has_device() {
        return;
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("test-tmp").join(format!("plp-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("thumbs")).unwrap();
    let db = Arc::new(Db::open(&dir.join("library.db"), dir.join("thumbs")).unwrap());
    // 音量 0 で始める
    db::state::save(
        &db,
        &db::state::SavedPlayback { volume: 0.0, repeat_mode: "off".into(), ..Default::default() },
    )
    .unwrap();
    let s = import::run(&db, &[fixture("sine.mp3"), fixture("sine.ogg"), fixture("sine.opus")], false, false, &|_| {});
    assert_eq!(s.added, 3);
    let ids: Vec<i64> = db::tracks::all_paths(&db).unwrap().into_iter().map(|(id, _)| id).collect();

    let c = Controller::new(db.clone());
    let col = Arc::new(Collect(Default::default()));
    c.add_listener(col.clone());
    c.set_repeat(Repeat::Off);
    // 1曲目を2.4秒目から → 自動で2曲目へ。キューに3曲目を入れておくと、2曲目より先に流れる
    c.play_context(ids.clone(), 0);
    c.seek(2400);
    c.queue_add(&[ids[2]]);
    let t0 = Instant::now();
    let mut order = vec![];
    while t0.elapsed() < Duration::from_secs(15) {
        let snap = c.snapshot();
        if let Some(t) = &snap.track {
            if order.last() != Some(&t.id) {
                order.push(t.id);
                if order.len() >= 2 {
                    c.seek(2500); // 早く終わらせる
                }
            }
        }
        if snap.status == Status::Stopped && order.len() >= 4 {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("played order: {order:?} (ids {ids:?})");
    // 1曲目 → キューの曲 → 並びの2曲目 → 並びの3曲目 → 終わり（止めて、先頭の曲を一時停止で読み込む）
    assert_eq!(order, vec![ids[0], ids[2], ids[1], ids[2], ids[0]]);
    assert!(c.queue_ids().is_empty());
    // 終わりまで来たら止まって、先頭に戻る
    let end = c.snapshot();
    assert_eq!(end.status, Status::Stopped, "{end:?}");
    // 状態の保存と復元
    c.set_shuffle(true);
    c.queue_add(&[ids[1], ids[0]]);
    c.save_all();
    drop(c);
    std::thread::sleep(Duration::from_millis(200));
    let saved = db::state::load(&db).unwrap();
    assert!(saved.shuffle);
    assert_eq!(saved.context.len(), 3);
    assert_eq!(db::queue::load(&db).unwrap(), vec![ids[1], ids[0]]);
    let c2 = Controller::new(db.clone());
    let s2 = c2.snapshot();
    assert!(s2.shuffle);
    assert_eq!(c2.queue_ids(), vec![ids[1], ids[0]]);
    assert_ne!(s2.status, Status::Playing, "must not autoplay on restore");
    drop(c2);
    println!("snapshots seen: {}", col.0.lock().unwrap().len());
    let _ = std::fs::remove_dir_all(&dir);
}

/// ミュートの切り替えと、ミュート中に音量を変えるとミュートが解けること（v2 の確認）
#[test]
#[ignore]
fn mute_and_volume() {
    if !has_device() {
        return;
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("test-tmp").join(format!("mute-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("thumbs")).unwrap();
    let db = Arc::new(Db::open(&dir.join("library.db"), dir.join("thumbs")).unwrap());
    let c = Controller::new(db.clone());
    c.set_muted(true);
    assert!(c.snapshot().muted);
    assert!(db::state::load(&db).unwrap().muted);
    c.toggle_mute();
    assert!(!c.snapshot().muted);
    c.set_muted(true);
    c.set_volume(0.3);
    c.save_state();
    assert!(!c.snapshot().muted, "volume change must unmute");
    assert!(!db::state::load(&db).unwrap().muted);
}

/// v3.3.0：再生中の曲をごみ箱へ移す。先に止めてファイルを手放し、移せたら止まったまま（曲なし）になる。
/// 移せなかったときは、同じ曲を続けて鳴らす。曲は一時フォルダに写したもの。ごみ箱へ入れた項目は、テストの終わりにごみ箱からも消す
#[test]
#[ignore]
fn trash_playing_track() {
    if !has_device() {
        return;
    }
    for name in ["sine.flac", "sine_heaac.m4a"] {
        println!("== {name}");
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-tmp")
            .join(format!("plp-trash-play-{}-{}", std::process::id(), name.replace('.', "-")));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("thumbs")).unwrap();
        let song = dir.join(name);
        std::fs::copy(fixture(name), &song).unwrap();
        let db = Arc::new(Db::open(&dir.join("library.db"), dir.join("thumbs")).unwrap());
        db::state::save(&db, &db::state::SavedPlayback { volume: 0.0, repeat_mode: "one".into(), ..Default::default() }).unwrap();
        let s = import::run(&db, &[song.clone()], false, false, &|_| {});
        assert_eq!(s.added, 1, "{s:?}");
        let id = db::tracks::all_paths(&db).unwrap()[0].0;
        let c = Controller::new(db.clone());
        c.set_repeat(Repeat::One);
        c.play_context(vec![id], 0);
        let t0 = Instant::now();
        while c.snapshot().status != Status::Playing && t0.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(c.snapshot().status, Status::Playing);
        std::thread::sleep(Duration::from_millis(400));
        // 移せなかった（使用中など）：ライブラリに残り、同じ曲を続けて鳴らす
        let r = pl_player_lib::library::trash::trash_tracks_with(&db, Some(&c), &[id], &|_| Some(3), &|_| {
            Err(pl_player_lib::error::AppError::new(pl_player_lib::error::ErrorKind::FileInUse, "test"))
        })
        .unwrap();
        assert!(r.moved.is_empty() && r.failed.len() == 1);
        let t1 = Instant::now();
        while c.snapshot().status != Status::Playing && t1.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let snap = c.snapshot();
        assert_eq!(snap.status, Status::Playing, "{snap:?}");
        assert_eq!(snap.track.as_ref().map(|t| t.id), Some(id));
        assert!(song.exists());
        // 移せた：止まって曲なしになり、ファイルとライブラリから消える
        let r = pl_player_lib::library::trash::trash_tracks_with(&db, Some(&c), &[id], &|_| Some(3), &|p| {
            pl_player_lib::platform::windows::recycle::move_to_recycle_bin_then_purge(p).map(|_| ())
        })
        .unwrap();
        assert_eq!(r.moved, vec![id], "{r:?}");
        assert!(r.left_in_library.is_empty(), "{r:?}");
        let snap = c.snapshot();
        assert_eq!(snap.status, Status::Stopped, "{snap:?}");
        assert!(snap.track.is_none());
        assert!(!song.exists());
        assert!(db::tracks::get(&db, id).unwrap().is_none());
        drop(c);
        std::thread::sleep(Duration::from_millis(200));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// v3.3.0：鳴っているのが、送った行とは別の綴り（フォルダ名の大文字小文字違い）で同じファイルを指す行のとき。
/// 送った行が鳴っていなくても、同じファイルなので先に手放す。移せなかったら別の綴りの行を続けて鳴らし、
/// 移せたら止まって曲なしになる（別の綴りの行はライブラリに残る。ファイルの無い曲として）
#[test]
#[ignore]
fn trash_track_playing_under_another_spelling() {
    if !has_device() {
        return;
    }
    for name in ["sine.flac", "sine_heaac.m4a"] {
        println!("== {name}");
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-tmp")
            .join(format!("plp-trash-alias-{}-{}", std::process::id(), name.replace('.', "-")));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("thumbs")).unwrap();
        let song = dir.join(name);
        std::fs::copy(fixture(name), &song).unwrap();
        let db = Arc::new(Db::open(&dir.join("library.db"), dir.join("thumbs")).unwrap());
        db::state::save(&db, &db::state::SavedPlayback { volume: 0.0, repeat_mode: "one".into(), ..Default::default() }).unwrap();
        let s = import::run(&db, &[song.clone()], false, false, &|_| {});
        assert_eq!(s.added, 1, "{s:?}");
        let id = db::tracks::all_paths(&db).unwrap()[0].0;
        // 同じファイルを、フォルダ名の大文字小文字だけ違う綴りで指す行（ファイル名と拡張子はそのまま）
        let t = db::tracks::get(&db, id).unwrap().unwrap();
        let alias = song.to_string_lossy().replace(r"\test-tmp\", r"\TEST-TMP\");
        assert_ne!(alias, t.path);
        let (alias_id, _) = db::tracks::upsert(
            &db.writer(),
            &db::tracks::TrackInfo { path: alias.clone(), format: t.format.clone(), duration_ms: t.duration_ms, title: t.title.clone(), ..Default::default() },
        )
        .unwrap();
        assert_ne!(alias_id, id);
        let c = Controller::new(db.clone());
        c.set_repeat(Repeat::One);
        c.play_context(vec![alias_id], 0);
        let wait_playing = |what: &str| {
            let t0 = Instant::now();
            while c.snapshot().status != Status::Playing && t0.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(50));
            }
            let snap = c.snapshot();
            assert_eq!(snap.status, Status::Playing, "{what}: {snap:?}");
            assert_eq!(snap.track.as_ref().map(|t| t.id), Some(alias_id), "{what}");
        };
        wait_playing("start");
        std::thread::sleep(Duration::from_millis(400));
        // 移せなかった：手放したあと、別の綴りの行を続けて鳴らす
        let r = pl_player_lib::library::trash::trash_tracks_with(&db, Some(&c), &[id], &|_| Some(3), &|_| {
            Err(pl_player_lib::error::AppError::new(pl_player_lib::error::ErrorKind::FileInUse, "test"))
        })
        .unwrap();
        assert!(r.moved.is_empty() && r.failed.len() == 1, "{r:?}");
        wait_playing("resumed");
        assert!(song.exists());
        // 移せた：止まって曲なしになる。送った行はライブラリから外れ、別の綴りの行は残る
        let r = pl_player_lib::library::trash::trash_tracks_with(&db, Some(&c), &[id], &|_| Some(3), &|p| {
            pl_player_lib::platform::windows::recycle::move_to_recycle_bin_then_purge(p).map(|_| ())
        })
        .unwrap();
        assert_eq!(r.moved, vec![id], "{r:?}");
        assert!(r.failed.is_empty() && r.left_in_library.is_empty(), "{r:?}");
        let snap = c.snapshot();
        assert_eq!(snap.status, Status::Stopped, "{snap:?}");
        assert!(snap.track.is_none(), "{snap:?}");
        assert_eq!(snap.context_len, 0, "{snap:?}");
        assert!(!song.exists());
        assert!(db::tracks::get(&db, id).unwrap().is_none());
        assert!(db::tracks::get(&db, alias_id).unwrap().is_some());
        drop(c);
        std::thread::sleep(Duration::from_millis(200));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
