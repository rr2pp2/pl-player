//! 画面の動作確認用のデータを作る（取り込み済みのライブラリ・プレイリスト・背景）。
//! 走らせ方：PL_PLAYER_DATA_DIR=<置き場所> cargo test --test smoke_seed -- --ignored
//! そのあと同じ PL_PLAYER_DATA_DIR でアプリを起動すると、そのデータで開く。
//!
//! v3：歌詞の確認用データも作る（歌詞はダミーの文。曲はフィクスチャの複製で、実際の音楽フォルダには触れない）
//! - 1曲目：曲ファイルの歌詞タグ（時刻つき）／2曲目：手動（時刻つき）／3曲目：外部（取り込み済み・時刻つき）
//! - 4曲目：受け渡しフォルダに置いたまま（時刻なし .txt）。「外部歌詞を取り込む」で取り込まれる
//! - 受け渡しフォルダに、ライブラリに無い曲 ID のファイルも1つ置く（rejected へ移ることを確かめる）

use std::path::PathBuf;

use pl_player_lib::db::{self, Db};
use pl_player_lib::library::{artwork, import, tags_read::TagValues, tags_write};
use pl_player_lib::lyrics::service as lyrics;
use pl_player_lib::settings::store;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)
}

#[test]
#[ignore]
fn seed() {
    let Some(dir) = std::env::var_os("PL_PLAYER_DATA_DIR").map(PathBuf::from) else {
        println!("PL_PLAYER_DATA_DIR is not set; skip");
        return;
    };
    let music = dir.join("music");
    std::fs::create_dir_all(&music).unwrap();
    let thumbs = dir.join("artwork").join("embedded");
    std::fs::create_dir_all(&thumbs).unwrap();
    // 曲：フィクスチャを複製して、タグとジャケットを付ける
    let list = [
        ("sine.mp3", "夜明けのテーマ", "Aoi Sora", "Morning Light", "J-Pop", Some("cover_a.png")),
        ("sine.flac", "群青の街", "Aoi Sora", "Morning Light", "J-Pop", Some("cover_a.png")),
        ("sine_aaclc.m4a", "Starfall", "Hikari", "Night Parade", "Anime", Some("cover_b.jpg")),
        ("sine.ogg", "風の通り道", "Hikari", "Night Parade", "Anime", Some("cover_b.jpg")),
        ("sine.opus", "とても長い曲名のテスト：狭いウィンドウで省略されるかを確かめるための曲名", "Long Name Artist Collective", "Very Long Album Title For Layout Check", "Soundtrack", None),
        ("sine.wav", "Plain Wave", "", "", "", None),
        ("sine.aac", "ADTS Track", "Test", "Formats", "Test", None),
        ("sine_alac.m4a", "Lossless", "Test", "Formats", "Test", None),
    ];
    for (i, (src, title, artist, album, genre, cover)) in list.iter().enumerate() {
        let dst = music.join(format!("{:02}_{}", i + 1, src));
        std::fs::copy(fixture(src), &dst).unwrap();
        let v = TagValues {
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            genre: genre.to_string(),
            track_no: Some(i as i64 + 1),
            year: Some(2026),
            ..Default::default()
        };
        let art = match cover {
            Some(c) if !src.ends_with(".wav") => tags_write::ArtChange::Set(fixture(c).to_string_lossy().into_owned()),
            _ => tags_write::ArtChange::Keep,
        };
        tags_write::write_tags(&dst, &v, &art).unwrap_or_else(|e| panic!("{src}: {e}"));
    }
    let db = Db::open(&dir.join("library.db"), thumbs).unwrap();
    let s = import::run(&db, &[music.clone()], true, true, &|_| {});
    println!("import: {s:?}");
    let ids: Vec<i64> = db::tracks::all_paths(&db).unwrap().into_iter().map(|(id, _)| id).collect();
    let pl = db::playlists::create(&db, "お気に入りの朝").unwrap();
    db::playlists::add_tracks(&db, pl, &ids[..4.min(ids.len())]).unwrap();
    db::playlists::create(&db, "作業用").unwrap();
    db::tracks::set_favorite(&db, &ids[..2.min(ids.len())], true).unwrap();
    // 背景（明るい画像で、文字の読みやすさを確かめる）
    let bg = artwork::import_image(&fixture("bg_bright.png"), &dir.join("backgrounds"), "bg").unwrap();
    let mut st = store::load(&db);
    st.background.path = Some(bg.to_string_lossy().into_owned());
    st.close_to_tray = false;
    store::save(&db, &st).unwrap();
    db::state::save(&db, &db::state::SavedPlayback { volume: 0.0, repeat_mode: "off".into(), ..Default::default() }).unwrap();
    seed_lyrics(&db, &dir, &ids);
    println!("seeded {} tracks into {}", ids.len(), dir.display());
}

/// 歌詞の確認用データ（フィクスチャの曲は3秒なので、0.5秒ごとの短い歌詞にする）
fn seed_lyrics(db: &Db, dir: &std::path::Path, ids: &[i64]) {
    use lofty::prelude::*;
    let timed = |label: &str| -> String {
        (0..6)
            .map(|i| format!("[00:0{}.{}0]{label}の{}行目", i / 2, (i % 2) * 5, i + 1))
            .collect::<Vec<_>>()
            .join("\n")
    };
    // 1曲目：曲ファイル（データフォルダの中の複製）に歌詞タグを書く
    let path1 = db::tracks::path_of(db, ids[0]).unwrap().unwrap();
    let mut tagged = lofty::read_from_path(&path1).unwrap();
    let tt = tagged.primary_tag_type();
    let tag = tagged.tag_mut(tt).expect("tag");
    tag.insert_text(lofty::tag::ItemKey::UnsyncLyrics, format!("[ti:ダミー]\n{}", timed("曲のタグ")));
    tag.save_to_path(&path1, lofty::config::WriteOptions::default()).unwrap();
    // 2曲目：手動
    lyrics::save_manual(db, ids[1], &format!("[offset:+100]\n{}\n[00:02.90]", timed("手動"))).unwrap();
    // 3曲目：外部（取り込み済み）
    let inbox = lyrics::inbox_dir(dir).unwrap();
    std::fs::write(inbox.join(format!("{}.lrc", ids[2])), timed("外部")).unwrap();
    let r = lyrics::import_inbox(db, dir).unwrap();
    println!("lyrics inbox: {r:?}");
    // 4曲目：受け渡しフォルダに置いたまま（画面のボタンで取り込む）。ライブラリに無い曲 ID も1つ
    std::fs::write(inbox.join(format!("{}.txt", ids[3])), "時刻の無い歌詞の一行目\n二行目\n\n二番の一行目").unwrap();
    std::fs::write(inbox.join("999999.lrc"), "[00:00.50]ライブラリに無い曲").unwrap();
    for id in &ids[..4] {
        let p = lyrics::get_lyrics(db, *id).unwrap();
        println!(
            "track {id}: source={:?} format={:?} lines={} warnings={:?}",
            p.source,
            p.format,
            p.lines.len(),
            p.warnings
        );
    }
}
