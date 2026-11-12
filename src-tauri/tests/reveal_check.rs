//! エクスプローラーで「ファイルの場所を開く」の確認。実際に explorer.exe を起動する。
//! 走らせ方：PL_REVEAL=<ファイル> cargo test --test reveal_check -- --ignored
#[test]
#[ignore]
fn reveal_opens_parent_folder() {
    use std::os::windows::process::CommandExt;
    let Some(p) = std::env::var_os("PL_REVEAL") else { return };
    let arg = pl_player_lib::commands::library::explorer_select_arg(std::path::Path::new(&p));
    println!("arg: {arg}");
    std::process::Command::new("explorer.exe").raw_arg(arg).spawn().unwrap();
}
