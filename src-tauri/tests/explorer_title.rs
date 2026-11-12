//! 書いたタグが Windows（エクスプローラーの詳細表示）からも読めるかを確かめるための、複製への書き込み。
//! 走らせ方：PL_SRC=<元の曲> cargo test --test explorer_title -- --ignored --nocapture
//! 複製は target\test-tmp\explorer に残す（PowerShell で読んだあと消す）
#[test]
#[ignore]
fn write_copy_for_explorer() {
    use pl_player_lib::library::{tags_read, tags_write};
    let Some(src) = std::env::var_os("PL_SRC") else { return };
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("test-tmp").join("explorer");
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join("copy.mp3");
    std::fs::copy(&src, &dst).unwrap();
    let mut v = tags_read::read_values(&dst).unwrap();
    v.title = "反映テスト タイトル".into();
    v.artist = "反映テスト アーティスト".into();
    tags_write::write_tags(&dst, &v, &tags_write::ArtChange::Keep).unwrap();
    println!("written {}", dst.display());
}
