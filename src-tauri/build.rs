fn main() {
    delay_load_media_foundation();
    tauri_build::build()
}

/// v3.1.2：Media Foundation の DLL を遅延読み込みにする（MSVC のリンカーのとき）。
/// N エディションで Media Feature Pack を入れていない Windows には `mfplat.dll`・`mfreadwrite.dll` が無い。
/// ふつうに結び付けると、起動のときに見つからずにアプリが起動しない。遅延読み込みなら、使うときまで読み込まない。
/// 使う前に有無を確かめるのは `platform::windows::mf_audio`（無ければ HE-AAC だけ「非対応形式」）。
/// 名前は `mf_audio::DELAY_LOADED_DLLS` と同じにする。
/// 本体・テストのどちらにも効く（`rustc-link-arg` は実行ファイル・cdylib・テストに渡る）
fn delay_load_media_foundation() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os != "windows" || env != "msvc" {
        return;
    }
    for dll in ["mfplat.dll", "mfreadwrite.dll"] {
        println!("cargo:rustc-link-arg=/DELAYLOAD:{dll}");
    }
    // 遅延読み込みの仕組み（__delayLoadHelper2）は MSVC の delayimp.lib にある
    println!("cargo:rustc-link-arg=delayimp.lib");
}
