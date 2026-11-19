//! 設定の書き出し・読み込み（原文「設定バックアップ」）。
//!
//! 対象：テーマ・背景・UI 設定・ショートカット・プレイリスト・お気に入り・ライブラリ設定。
//! 1つの zip（中身は JSON と背景画像）にまとめる。曲はパスで持つので、別の PC では見つからない曲が出る。
//! 読み込みのときに、その件数を知らせる。
//!
//! v3.8.0：出力先の設定（DB の `outputs`）を `outputs.json` として入れる（登録したデバイス・外した出力先の覚えも全部。
//! デバイスの名前が入るので、人に渡すときは注意。README）。FORMAT_VERSION は上げない（古い版は知らない名前を読まない）。
//! 読み込むときは全部を確かめてから書き、出力先は全部オフで入れる（一時停止は呼び出し側）。`outputs.json` の無い zip
//! （3.7.0 まで）では出力に触らない。表示のモード（DB の `mode`）は入れない
//!
//! v3.9.0：着せ替えの設定のうち、使う立ち絵の選択（`skin.avatar`）だけは読み込む（立ち絵の画像は入らない）

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::store::{self, AppSettings};
use super::zip;
use crate::db::{self, Db};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::library::{artwork, import};
use crate::platform::windows::localpath;
use crate::playback::outputs::{OutputsDoc, OUTPUTS_KEY};

const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    app: String,
    format_version: u32,
    exported_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaylistOut {
    name: String,
    paths: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FolderOut {
    path: String,
    recursive: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupReport {
    pub playlists: usize,
    pub playlist_tracks: usize,
    /// プレイリストの曲のうち、この PC のライブラリに無かったもの
    pub playlist_tracks_missing: usize,
    pub favorites: usize,
    pub favorites_missing: usize,
    pub folders: usize,
    pub background: bool,
    /// v3.1.1：ネットワーク上の場所（`\\` で始まる・ネットワークドライブ）なので入れなかったライブラリのフォルダ
    pub folders_rejected: usize,
    /// v3.1.1：zip の背景が画像として読めなかったので、入れなかった
    pub background_rejected: bool,
    /// v3.8.0：出力先の設定（outputs.json）を書き出した・読み込んだ（読み込んだときは全部オフ）
    pub outputs: bool,
}

/// 読み込みの結果（v3.8.0：出力先の設定を足した。`import` は 3.7.0 までと同じ3つを返す）
pub struct Imported {
    pub settings: AppSettings,
    pub report: BackupReport,
    pub folders: Vec<(String, bool)>,
    /// zip に outputs.json があれば、確かめて全部オフにしたもの。司令塔が一時停止して入れる
    pub outputs: Option<OutputsDoc>,
}

/// 書き出し。`bg_dir` はデータフォルダの背景の置き場（v3.1.1：そこにある背景だけを zip に入れる）
pub fn export(db: &Db, settings: &AppSettings, bg_dir: &Path, dest: &Path) -> AppResult<BackupReport> {
    let (entries, report) = export_entries(db, settings, bg_dir)?;
    let mut f = fs::File::create(dest).map_err(|e| AppError::from_io(&e, dest))?;
    zip::write(&mut f, &entries).map_err(|e| AppError::from_io(&e, dest))?;
    Ok(report)
}

/// zip に入れる中身を作る（テストは zip のファイルを作らずにここを呼べる）
pub(crate) fn export_entries(db: &Db, settings: &AppSettings, bg_dir: &Path) -> AppResult<(Vec<zip::Entry>, BackupReport)> {
    let mut entries = vec![];
    let mut report = BackupReport::default();
    entries.push(zip::Entry {
        name: "manifest.json".into(),
        data: serde_json::to_vec_pretty(&Manifest {
            app: "PL Player".into(),
            format_version: FORMAT_VERSION,
            exported_at: db::now_ms(),
        })?,
    });
    let mut s = settings.clone();
    // 背景画像は zip に入れ、設定には zip の中の名前を書く
    // v3.1.1（ペネトレーションテスト v3.0.2 の4）：背景のパスがデータフォルダの backgrounds の外を指していたら入れない。
    // 読むのも上限つき（50MB）
    if let Some(bg) = settings.background.path.as_deref() {
        let p = Path::new(bg);
        let inside = store::background_in_dir(bg, bg_dir);
        if !inside {
            tracing::warn!("settings export: background outside the data folder was not exported");
        }
        if let Some(data) = inside.then(|| artwork::read_image_file(p).ok()).flatten() {
            let name = format!(
                "background/{}",
                p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "bg".into())
            );
            entries.push(zip::Entry { name: name.clone(), data });
            s.background.path = Some(name);
            report.background = true;
        } else {
            s.background.path = None;
        }
    }
    entries.push(zip::Entry {
        name: "settings.json".into(),
        data: serde_json::to_vec_pretty(&s)?,
    });
    let pls: Vec<PlaylistOut> = db::playlists::export_all(db)?
        .into_iter()
        .map(|(name, paths)| PlaylistOut { name, paths })
        .collect();
    report.playlists = pls.len();
    report.playlist_tracks = pls.iter().map(|p| p.paths.len()).sum();
    entries.push(zip::Entry {
        name: "playlists.json".into(),
        data: serde_json::to_vec_pretty(&pls)?,
    });
    let favs = db::tracks::favorite_paths(db)?;
    report.favorites = favs.len();
    entries.push(zip::Entry {
        name: "favorites.json".into(),
        data: serde_json::to_vec_pretty(&favs)?,
    });
    let folders: Vec<FolderOut> = import::library_folders(db)
        .into_iter()
        .map(|(path, recursive)| FolderOut { path, recursive })
        .collect();
    report.folders = folders.len();
    entries.push(zip::Entry {
        name: "library.json".into(),
        data: serde_json::to_vec_pretty(&folders)?,
    });
    // v3.8.0：出力先の設定（Rust が保存している DB の値のまま。書き出す前に司令塔が保存し直す）
    if let Some(v) = db::settings::get(db, OUTPUTS_KEY)? {
        entries.push(zip::Entry { name: "outputs.json".into(), data: serde_json::to_vec_pretty(&v)? });
        report.outputs = true;
    }
    Ok((entries, report))
}

/// 設定 zip から取り出す名前（ほかの名前は中身を写さない。v3.1.1）
fn wanted(name: &str) -> bool {
    matches!(
        name,
        "manifest.json" | "settings.json" | "playlists.json" | "favorites.json" | "library.json" | "outputs.json"
    )
        || name.starts_with("background/")
}

fn broken(name: &str, e: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorKind::UnsupportedFormat, format!("{name} is broken: {e}"))
}

/// zip の中の JSON を読む。無ければ None、あるのに読めなければエラー（v3.1.1：黙って既定値に戻さない）
fn json_entry<T: serde::de::DeserializeOwned>(entries: &[zip::Entry], name: &str) -> AppResult<Option<T>> {
    match entries.iter().find(|e| e.name == name) {
        None => Ok(None),
        Some(e) => serde_json::from_slice(&e.data).map(Some).map_err(|err| broken(name, err)),
    }
}

/// v3.9.0：zip の settings.json に、使う立ち絵の項目（`skin.avatar`）があるか（値が壊れていても、項目があれば true）
fn avatar_in_zip(entries: &[zip::Entry]) -> bool {
    entries
        .iter()
        .find(|e| e.name == "settings.json")
        .and_then(|e| serde_json::from_slice::<serde_json::Value>(&e.data).ok())
        .map(|v| v.get("skin").and_then(|s| s.get("avatar")).is_some())
        .unwrap_or(false)
}

/// 読み込み。設定は戻り値で返し、呼び出し側が保存・反映する。
/// ライブラリのフォルダは記録だけ足す（取り込み直しは呼び出し側が行う）。
///
/// v3.1.1：中身を全部確かめてから、背景・DB に書く。`settings.json` が無い・壊れている、
/// ほかの JSON が壊れているときは、何も変えずにエラーを返す（ペネトレーションテスト v3.0.2 の13）
pub fn import(db: &Db, src: &Path, bg_dir: &Path) -> AppResult<(AppSettings, BackupReport, Vec<(String, bool)>)> {
    let r = import_full(db, src, bg_dir)?;
    Ok((r.settings, r.report, r.folders))
}

/// 読み込み（v3.8.0：出力先の設定も返す）
pub fn import_full(db: &Db, src: &Path, bg_dir: &Path) -> AppResult<Imported> {
    let mut f = fs::File::open(src).map_err(|e| AppError::from_io(&e, src))?;
    let entries = zip::read(&mut f, &wanted)
        .map_err(|e| AppError::new(ErrorKind::UnsupportedFormat, format!("{}: {e}", src.display())))?;
    import_entries(db, &entries, bg_dir)
}

/// zip の中身から読み込む（テストは zip のファイルを作らずにここを呼べる）
pub(crate) fn import_entries(db: &Db, entries: &[zip::Entry], bg_dir: &Path) -> AppResult<Imported> {
    let manifest: Manifest = json_entry(entries, "manifest.json")?
        .ok_or_else(|| AppError::new(ErrorKind::UnsupportedFormat, "manifest.json missing"))?;
    if manifest.format_version > FORMAT_VERSION {
        return Err(AppError::new(
            ErrorKind::UnsupportedFormat,
            format!("format version {}", manifest.format_version),
        ));
    }
    let mut settings: AppSettings = json_entry(entries, "settings.json")?
        .ok_or_else(|| AppError::new(ErrorKind::UnsupportedFormat, "settings.json missing"))?;
    // v3.9.0：使う立ち絵の選択は zip の値を使う（整える前の値。値があって壊れていれば `skin`。store::de_avatar）。
    // 値が無い（3.8.1 までの zip。`skin` の項目ごと無い zip も）は None。有る無しは JSON の項目で見る（読んだ形では既定の値と見分けられないため）
    let zip_avatar = avatar_in_zip(entries).then(|| settings.skin.avatar.clone());
    let folders: Vec<FolderOut> = json_entry(entries, "library.json")?.unwrap_or_default();
    let favs: Vec<String> = json_entry(entries, "favorites.json")?.unwrap_or_default();
    let pls: Vec<PlaylistOut> = json_entry(entries, "playlists.json")?.unwrap_or_default();
    // v3.8.0：出力先の設定も、何かを書く前に全部確かめる（外れた値があれば zip 全体を断る）
    let outputs = match entries.iter().find(|e| e.name == "outputs.json") {
        None => None,
        Some(e) => Some(OutputsDoc::from_zip(&e.data).map_err(|err| broken("outputs.json", err))?),
    };

    let mut report = BackupReport::default();
    // 背景画像：zip から取り出し、画像として最後まで読めるものだけを、自前の名前でデータフォルダへ（v3.1.1 の6）
    if let Some(name) = settings.background.path.take() {
        if let Some(e) = entries.iter().find(|e| e.name == name && name.starts_with("background/")) {
            match artwork::store_image_bytes(&e.data, bg_dir, "bg", &name) {
                Ok(dest) => {
                    settings.background.path = Some(dest.to_string_lossy().into_owned());
                    report.background = true;
                }
                Err(err) => {
                    tracing::warn!("settings import: background rejected: {}", err.detail);
                    report.background_rejected = true;
                }
            }
        }
    }
    let mut settings = settings.sanitize();
    // v3.1.0（直し1回目）：「歌詞をネットから取る」は、読み込んだ値を使わず常にオフにする。
    // 他人から受け取った zip で、気づかないうちに通信がオンにならないように（オンにするのは設定画面でだけ）
    settings.lyrics_online = false;
    // v3.3.0：行の「ごみ箱へ移す」ボタンも同じ。曲ファイルを動かす操作のボタンなので、受け取った zip で気づかないうちに行に出ないように
    settings.ui.trash_button = false;
    // ライブラリのフォルダ。v3.1.1（ペネトレーションテスト v3.0.2 の5）：ネットワーク上の場所は入れない
    // （記録すると、起動時の再走査などで SMB の接続が出る）。判定は文字とドライブの種類だけで、そこへはつながない
    let (folders, rejected): (Vec<FolderOut>, Vec<FolderOut>) =
        folders.into_iter().partition(|fo| localpath::is_local(Path::new(&fo.path)));
    report.folders_rejected = rejected.len();
    if !rejected.is_empty() {
        tracing::warn!("settings import: {} network folder(s) were not added", rejected.len());
    }
    {
        let conn = db.writer();
        for fo in &folders {
            let _ = conn.execute(
                "INSERT OR IGNORE INTO library_folders (path, recursive, added_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![fo.path, fo.recursive as i64, db::now_ms()],
            );
        }
    }
    report.folders = folders.len();
    // お気に入り
    let applied = db::tracks::set_favorite_by_paths(db, &favs)?;
    report.favorites = applied;
    report.favorites_missing = favs.len().saturating_sub(applied);
    // プレイリスト（同じ名前があれば「（読み込み）」を付けて別に作る）
    let existing: Vec<String> = db::playlists::list(db)?.into_iter().map(|p| p.name).collect();
    for p in pls {
        let name = if existing.contains(&p.name) {
            format!("{}（読み込み）", p.name)
        } else {
            p.name.clone()
        };
        let id = db::playlists::create(db, &name)?;
        let mut ids = vec![];
        for path in &p.paths {
            match db::tracks::id_by_path(db, path)? {
                Some(t) => ids.push(t),
                None => report.playlist_tracks_missing += 1,
            }
        }
        report.playlist_tracks += ids.len();
        db::playlists::add_tracks(db, id, &ids)?;
        report.playlists += 1;
    }
    // v3.2.0：着せ替えの設定は読み込まない（スキンそのものは zip に入らず、別の PC には無いことがあるため）。
    // v3.9.0：ただし使う立ち絵の選択は zip の値にする（立ち絵の画像は zip に入らない）。
    // - 値が無い（3.8.1 までの zip）：この PC の今の選択を変えない（作者の決定。3.8.1 で「立ち絵を出す」を切っていた人に出てしまわないように）
    // - 値があって壊れている（形の違う文字・文字でない値）：「スキンの立ち絵」（store::de_avatar と normalize_choice）
    // - この PC に無い立ち絵・スキンを指していたら、呼び出し側（skins::runtime::after_settings_import）が「スキンの立ち絵」に戻す（ここではファイルを見ない）
    let mut skin = store::load(db).skin;
    if let Some(v) = zip_avatar {
        skin.set_avatar(&v);
    }
    settings.skin = skin;
    store::save(db, &settings)?;
    let folder_list = folders.into_iter().map(|f| (f.path, f.recursive)).collect();
    report.outputs = outputs.is_some();
    Ok(Imported { settings, report, folders: folder_list, outputs })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tracks::{upsert, TrackInfo};

    #[test]
    fn export_then_import_into_fresh_db() {
        let tmp = crate::test_tmp().join(format!("plp-backup-{}", std::process::id()));
        let bg_dir = tmp.join("a-backgrounds");
        fs::create_dir_all(&bg_dir).unwrap();
        let bg = bg_dir.join("bg-1.png");
        image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3])).save(&bg).unwrap();

        let a = Db::open_memory("backup_a").unwrap();
        let (t1, _) = upsert(&a.writer(), &TrackInfo { path: "C:/m/1.mp3".into(), ..Default::default() }).unwrap();
        let (t2, _) = upsert(&a.writer(), &TrackInfo { path: "C:/m/2.mp3".into(), ..Default::default() }).unwrap();
        db::tracks::set_favorite(&a, &[t2], true).unwrap();
        let pl = db::playlists::create(&a, "朝").unwrap();
        db::playlists::add_tracks(&a, pl, &[t1, t2]).unwrap();
        let mut s = AppSettings::default();
        s.theme = "light".into();
        s.ui.lyrics_row = false;
        s.lyrics_online = true;
        // v3.3.0：隠した再生バーのボタン・サイドバーの項目は、書き出し・読み込みに入る。ごみ箱のボタンは読み込むとオフ
        s.ui.player_buttons.insert("shuffle".into(), false);
        s.ui.player_buttons.insert("lyricsRow".into(), false);
        s.ui.sidebar_items.insert("genre".into(), false);
        s.ui.sidebar_items.insert("playlists".into(), false);
        s.ui.trash_button = true;
        s.ui.settings_tabs = vec!["app".into(), "display".into()];
        // v3.4.0：コンパクト表示とサイドバーのアイコンの大きさも、書き出し・読み込みに入る
        s.ui.compact_list = "always".into();
        s.ui.sidebar_icon_px = 26;
        // v3.5.0：曲一覧の列のアーティストも、書き出し・読み込みに入る
        s.ui.columns.insert("artist".into(), false);
        s.background.path = Some(bg.to_string_lossy().into_owned());
        let zip_path = tmp.join("backup.zip");
        let r = export(&a, &s, &bg_dir, &zip_path).unwrap();
        assert_eq!(r.playlists, 1);
        assert!(r.background);

        // 別の PC：1.mp3 だけがある
        let b = Db::open_memory("backup_b").unwrap();
        upsert(&b.writer(), &TrackInfo { path: "C:/m/1.mp3".into(), ..Default::default() }).unwrap();
        let (s2, r2, _) = import(&b, &zip_path, &tmp.join("bgs")).unwrap();
        assert_eq!(s2.theme, "light");
        // v3.0.2：流れる歌詞の列のオン・オフも書き出し・読み込みに入る
        assert!(!s2.ui.lyrics_row);
        // v3.1.0（直し1回目）：書き出しには入るが、読み込みでは常にオフ（オンの zip でも）
        assert!(std::str::from_utf8(&zip::read(&mut fs::File::open(&zip_path).unwrap(), &|_| true).unwrap().into_iter().find(|e| e.name == "settings.json").unwrap().data).unwrap().contains("\"lyricsOnline\": true"));
        assert!(!s2.lyrics_online);
        assert!(!store::load(&b).lyrics_online);
        assert_eq!(s2.ui.player_buttons.get("shuffle"), Some(&false));
        assert_eq!(s2.ui.player_buttons.get("lyricsRow"), Some(&false));
        assert_eq!(s2.ui.player_buttons.get("repeat"), Some(&true));
        assert_eq!(s2.ui.sidebar_items.get("genre"), Some(&false));
        assert_eq!(s2.ui.sidebar_items.get("playlists"), Some(&false));
        assert_eq!(s2.ui.sidebar_items.get("all"), Some(&true));
        assert!(!s2.ui.trash_button);
        assert_eq!(s2.ui.settings_tabs, vec!["app", "display"]);
        assert_eq!(s2.ui.compact_list, "always");
        assert_eq!(s2.ui.sidebar_icon_px, 26);
        assert_eq!(store::load(&b).ui.compact_list, "always");
        assert_eq!(store::load(&b).ui.sidebar_icon_px, 26);
        assert_eq!(s2.ui.columns.get("artist"), Some(&false));
        assert_eq!(store::load(&b).ui.columns.get("artist"), Some(&false));
        assert_eq!(s2.ui.columns.get("album"), Some(&true));
        let bg2 = Path::new(s2.background.path.as_ref().unwrap());
        assert!(bg2.exists());
        // v3.1.1：取り出した背景は自前の名前（bg-<鍵>.png）
        assert!(bg2.file_name().unwrap().to_string_lossy().starts_with("bg-"));
        assert!(bg2.starts_with(tmp.join("bgs")));
        assert_eq!(r2.playlists, 1);
        assert_eq!(r2.playlist_tracks, 1);
        assert_eq!(r2.playlist_tracks_missing, 1);
        assert_eq!(r2.favorites_missing, 1);
        let _ = fs::remove_dir_all(&tmp);
    }

    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        let d = crate::test_tmp().join(format!("plp-backup-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn manifest() -> zip::Entry {
        zip::Entry { name: "manifest.json".into(), data: br#"{"app":"PL Player","formatVersion":1,"exportedAt":0}"#.to_vec() }
    }

    fn make_zip(dir: &Path, name: &str, entries: &[zip::Entry]) -> std::path::PathBuf {
        let p = dir.join(name);
        zip::write(&mut fs::File::create(&p).unwrap(), entries).unwrap();
        p
    }

    fn png_bytes() -> Vec<u8> {
        let mut v = vec![];
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4, 4, image::Rgb([9, 9, 9])))
            .write_to(&mut std::io::Cursor::new(&mut v), image::ImageFormat::Png)
            .unwrap();
        v
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の4）：背景のパスが backgrounds の外（ここではこのプロジェクトの Cargo.toml）を
    /// 指していても、その中身を zip に入れない
    #[test]
    fn export_ignores_background_outside_the_data_folder() {
        let tmp = tmp_dir("outside");
        let db = Db::open_memory("backup_outside").unwrap();
        let mut s = AppSettings::default();
        let secret = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        s.background.path = Some(secret.to_string_lossy().into_owned());
        let z = tmp.join("out.zip");
        let r = export(&db, &s, &tmp.join("backgrounds"), &z).unwrap();
        assert!(!r.background);
        let entries = zip::read(&mut fs::File::open(&z).unwrap(), &|_| true).unwrap();
        assert!(entries.iter().all(|e| !e.name.starts_with("background/")));
        let settings = entries.iter().find(|e| e.name == "settings.json").unwrap();
        assert!(!String::from_utf8_lossy(&settings.data).contains("Cargo.toml"));
        let _ = fs::remove_dir_all(&tmp);
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の13）：settings.json が壊れている・無い、ほかの JSON が壊れている zip は、
    /// 何も変えずにエラー（3.1.0 までは黙って既定値に戻していた）
    #[test]
    fn broken_json_stops_import_without_changes() {
        let tmp = tmp_dir("broken");
        let good_lib = zip::Entry { name: "library.json".into(), data: br#"[{"path":"C:\\Music","recursive":true}]"#.to_vec() };
        let good_pl = zip::Entry { name: "playlists.json".into(), data: br#"[{"name":"p","paths":[]}]"#.to_vec() };
        let cases = [
            ("settings-broken.zip", vec![manifest(), zip::Entry { name: "settings.json".into(), data: b"{\"theme\": ".to_vec() }, good_lib.clone(), good_pl.clone()]),
            ("settings-type.zip", vec![manifest(), zip::Entry { name: "settings.json".into(), data: br#"{"theme": 5}"#.to_vec() }, good_lib.clone(), good_pl.clone()]),
            ("settings-missing.zip", vec![manifest(), good_lib.clone(), good_pl.clone()]),
            ("playlists-broken.zip", vec![manifest(), zip::Entry { name: "settings.json".into(), data: b"{}".to_vec() }, good_lib.clone(), zip::Entry { name: "playlists.json".into(), data: b"[{".to_vec() }]),
        ];
        for (name, entries) in cases {
            let db = Db::open_memory(&format!("backup_broken_{name}")).unwrap();
            let z = make_zip(&tmp, name, &entries);
            let e = import(&db, &z, &tmp.join("bgs")).err().unwrap_or_else(|| panic!("{name}: imported"));
            assert_eq!(e.kind, ErrorKind::UnsupportedFormat, "{name}");
            assert!(import::library_folders(&db).is_empty(), "{name}: folders added");
            assert!(db::playlists::list(&db).unwrap().is_empty(), "{name}: playlists added");
            assert!(db::settings::get(&db, "app").unwrap().is_none(), "{name}: settings saved");
        }
        let _ = fs::remove_dir_all(&tmp);
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の6）：zip の背景は画像として最後まで読めるものだけを、自前の名前で書く
    #[test]
    fn background_from_zip_must_be_an_image() {
        let tmp = tmp_dir("bgcheck");
        let bgs = tmp.join("bgs");
        let settings = |n: &str| zip::Entry {
            name: "settings.json".into(),
            data: format!(r#"{{"background":{{"path":"{n}"}}}}"#).into_bytes(),
        };
        // 画像でない（実行ファイルに見えるもの）・先頭だけ PNG のもの：書かない
        let mut half = png_bytes();
        half.truncate(20);
        for (i, data) in [b"MZ\x90\x00 this is not an image".to_vec(), half].into_iter().enumerate() {
            let db = Db::open_memory(&format!("backup_bg_bad_{i}")).unwrap();
            let z = make_zip(&tmp, &format!("bad{i}.zip"), &[manifest(), settings("background/evil.exe"), zip::Entry { name: "background/evil.exe".into(), data }]);
            let (s, r, _) = import(&db, &z, &bgs).unwrap();
            assert!(!r.background && r.background_rejected, "{i}");
            assert!(s.background.path.is_none());
            assert!(!bgs.exists() || fs::read_dir(&bgs).unwrap().next().is_none(), "{i}: something was written");
        }
        // 本物の画像なら、名前が evil.exe でも bg-<鍵>.png で書く
        let db = Db::open_memory("backup_bg_good").unwrap();
        let z = make_zip(&tmp, "good.zip", &[manifest(), settings("background/evil.exe"), zip::Entry { name: "background/evil.exe".into(), data: png_bytes() }]);
        let (s, r, _) = import(&db, &z, &bgs).unwrap();
        assert!(r.background && !r.background_rejected);
        let name = Path::new(s.background.path.as_ref().unwrap()).file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("bg-") && name.ends_with(".png"), "{name}");
        assert!(!bgs.join("evil.exe").exists());
        let _ = fs::remove_dir_all(&tmp);
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の5）：ライブラリのフォルダのうち、ネットワーク上の場所は入れない。
    /// 判定は文字だけで、そのパスには触らない（実在しない名前 .invalid とループバックを使うが、つながない）
    #[test]
    fn network_library_folders_are_not_added() {
        let tmp = tmp_dir("unc");
        let db = Db::open_memory("backup_unc").unwrap();
        let lib = r#"[{"path":"\\\\127.0.0.1\\share","recursive":true},{"path":"//pl-player-test.invalid/share","recursive":true},{"path":"\\\\?\\UNC\\pl-player-test.invalid\\s","recursive":true},{"path":"C:\\Music-plp-test","recursive":true}]"#;
        let z = make_zip(&tmp, "unc.zip", &[manifest(), zip::Entry { name: "settings.json".into(), data: b"{}".to_vec() }, zip::Entry { name: "library.json".into(), data: lib.as_bytes().to_vec() }]);
        let (_, r, folders) = import(&db, &z, &tmp.join("bgs")).unwrap();
        assert_eq!(r.folders_rejected, 3);
        assert_eq!(r.folders, 1);
        assert_eq!(folders, vec![(r"C:\Music-plp-test".to_string(), true)]);
        assert_eq!(import::library_folders(&db), vec![(r"C:\Music-plp-test".to_string(), true)]);
        let _ = fs::remove_dir_all(&tmp);
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の1）：重なりエントリの zip は、設定の読み込みでも断る
    #[test]
    fn overlapping_zip_is_refused_by_import() {
        let tmp = tmp_dir("bomb");
        let db = Db::open_memory("backup_bomb").unwrap();
        // settings.json を 500 回、同じデータで指す
        let mut one = Vec::new();
        zip::write(&mut one, &[zip::Entry { name: "settings.json".into(), data: vec![b' '; 64 * 1024] }]).unwrap();
        let local_len = 30 + 13 + 64 * 1024;
        let central = one[local_len..one.len() - 22].to_vec();
        let mut z = one[..local_len].to_vec();
        for _ in 0..500 {
            z.extend_from_slice(&central);
        }
        z.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        z.extend_from_slice(&[0, 0, 0, 0]);
        z.extend_from_slice(&500u16.to_le_bytes());
        z.extend_from_slice(&500u16.to_le_bytes());
        z.extend_from_slice(&((central.len() * 500) as u32).to_le_bytes());
        z.extend_from_slice(&(local_len as u32).to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        let p = tmp.join("bomb.zip");
        fs::write(&p, &z).unwrap();
        assert_eq!(import(&db, &p, &tmp.join("bgs")).unwrap_err().kind, ErrorKind::UnsupportedFormat);
        let _ = fs::remove_dir_all(&tmp);
    }

    /// v3.4.0：設定の zip の settings.json に、コンパクト表示・サイドバーのアイコンの大きさが無い（3.3.0 までの zip）なら既定。
    /// 知らない値・範囲の外の値・形の違う値は、その値だけを整えて読み込む（zip を断らない。ほかの設定は入る）。
    /// v3.8.0：3.7.0 までの zip の "narrow"（狭いときだけ）は、使わない＋「窓が狭いときは自動でコンパクトにする」オンで入る
    #[test]
    fn import_compact_list_and_sidebar_icon() {
        let tmp = tmp_dir("density");
        let cases: [(&str, &str, &str, bool, u32); 9] = [
            ("old", r#"{"theme":"light","ui":{"sidebarCollapsed":true}}"#, "off", false, 20),
            ("narrow", r#"{"theme":"light","ui":{"compactList":"narrow","sidebarIconPx":16}}"#, "off", true, 16),
            ("off", r#"{"theme":"light","ui":{"compactList":"off","sidebarIconPx":16}}"#, "off", false, 16),
            ("new", r#"{"theme":"light","ui":{"compactList":"off","compactAuto":true}}"#, "off", true, 20),
            ("unknown", r#"{"theme":"light","ui":{"compactList":"micro","sidebarIconPx":64}}"#, "off", false, 32),
            ("small", r#"{"theme":"light","ui":{"compactList":"always","sidebarIconPx":2}}"#, "always", false, 14),
            ("types", r#"{"theme":"light","ui":{"compactList":1,"sidebarIconPx":"28"}}"#, "off", false, 20),
            ("nulls", r#"{"theme":"light","ui":{"compactList":null,"sidebarIconPx":null,"compactAuto":null}}"#, "off", false, 20),
            ("auto-type", r#"{"theme":"light","ui":{"compactList":"always","compactAuto":"yes"}}"#, "always", false, 20),
        ];
        for (name, json, mode, auto, px) in cases {
            let db = Db::open_memory(&format!("backup_density_{name}")).unwrap();
            let z = make_zip(&tmp, &format!("{name}.zip"), &[manifest(), zip::Entry { name: "settings.json".into(), data: json.as_bytes().to_vec() }]);
            let (s, _, _) = import(&db, &z, &tmp.join("bgs")).unwrap_or_else(|e| panic!("{name}: {}", e.detail));
            assert_eq!(s.theme, "light", "{name}");
            assert_eq!(s.ui.compact_list, mode, "{name}");
            assert_eq!(s.ui.compact_auto, auto, "{name}");
            assert_eq!(s.ui.sidebar_icon_px, px, "{name}");
            let saved = store::load(&db);
            assert_eq!(saved.ui.compact_list, mode, "{name}: saved");
            assert_eq!(saved.ui.compact_auto, auto, "{name}: saved");
            assert_eq!(saved.ui.sidebar_icon_px, px, "{name}: saved");
        }
        // 書き出した zip の settings.json に、画面と同じ名前で入る
        let a = Db::open_memory("backup_density_export").unwrap();
        let mut s = AppSettings::default();
        s.ui.compact_list = "off".into();
        s.ui.compact_auto = true;
        s.ui.sidebar_icon_px = 30;
        let z = tmp.join("export.zip");
        export(&a, &s, &tmp.join("no-bg"), &z).unwrap();
        let entries = zip::read(&mut fs::File::open(&z).unwrap(), &|_| true).unwrap();
        let text = String::from_utf8(entries.into_iter().find(|e| e.name == "settings.json").unwrap().data).unwrap();
        assert!(text.contains("\"compactList\": \"off\"") && text.contains("\"sidebarIconPx\": 30"), "{text}");
        assert!(text.contains("\"compactAuto\": true"), "{text}");
        let _ = fs::remove_dir_all(&tmp);
    }

    /// v3.5.0：曲一覧の列のアーティスト。3.4.0 までの zip（columns に artist が無い・columns が無い）はオンで読み込み、
    /// 切った値はそのまま入る。書き出した zip の settings.json に、画面と同じ名前で入る
    #[test]
    fn import_track_columns_artist() {
        let tmp = tmp_dir("columns");
        let cases: [(&str, &str, bool, bool); 4] = [
            ("old", r#"{"theme":"light","ui":{"columns":{"album":false,"genre":true,"duration":true,"favorite":true}}}"#, true, false),
            ("nocols", r#"{"theme":"light","ui":{"sidebarCollapsed":true}}"#, true, true),
            ("off", r#"{"theme":"light","ui":{"columns":{"artist":false,"album":true}}}"#, false, true),
            ("on", r#"{"theme":"light","ui":{"columns":{"artist":true}}}"#, true, true),
        ];
        for (name, json, artist, album) in cases {
            let db = Db::open_memory(&format!("backup_columns_{name}")).unwrap();
            let z = make_zip(&tmp, &format!("{name}.zip"), &[manifest(), zip::Entry { name: "settings.json".into(), data: json.as_bytes().to_vec() }]);
            let (s, _, _) = import(&db, &z, &tmp.join("bgs")).unwrap_or_else(|e| panic!("{name}: {}", e.detail));
            assert_eq!(s.theme, "light", "{name}");
            assert_eq!(s.ui.columns.get("artist"), Some(&artist), "{name}");
            assert_eq!(s.ui.columns.get("album"), Some(&album), "{name}");
            let saved = store::load(&db);
            assert_eq!(saved.ui.columns.get("artist"), Some(&artist), "{name}: saved");
        }
        let a = Db::open_memory("backup_columns_export").unwrap();
        let mut s = AppSettings::default();
        s.ui.columns.insert("artist".into(), false);
        let z = tmp.join("export.zip");
        export(&a, &s, &tmp.join("no-bg"), &z).unwrap();
        let entries = zip::read(&mut fs::File::open(&z).unwrap(), &|_| true).unwrap();
        let text = String::from_utf8(entries.into_iter().find(|e| e.name == "settings.json").unwrap().data).unwrap();
        assert!(text.contains("\"artist\": false"), "{text}");
        let _ = fs::remove_dir_all(&tmp);
    }

    /// v3.2.0：設定の読み込みは着せ替えの設定を変えない（スキンそのものは zip に入らず、読み込む側の PC には無いことがある）
    #[test]
    fn import_keeps_skin_settings_of_this_pc() {
        let tmp = tmp_dir("skin");
        let a = Db::open_memory("backup_skin_a").unwrap();
        let mut s = AppSettings::default();
        s.theme = "light".into();
        s.skin.active_skin_id = "from-other-pc".into();
        s.skin.character_visible = false;
        s.skin.character_scale = 1.25;
        s.skin.safe_mode_next_start = true;
        let zip_path = tmp.join("backup.zip");
        export(&a, &s, &tmp.join("no-bg"), &zip_path).unwrap();

        let b = Db::open_memory("backup_skin_b").unwrap();
        let mut mine = AppSettings::default();
        mine.skin.active_skin_id = "quiet-paper".into();
        mine.skin.character_scale = 0.9;
        store::save(&b, &mine).unwrap();
        let (s2, _, _) = import(&b, &zip_path, &tmp.join("bgs")).unwrap();
        // ほかの設定は読み込む
        assert_eq!(s2.theme, "light");
        // 着せ替えは、読み込む前のこの PC の値のまま（返す値も、保存した値も）
        assert_eq!(s2.skin, mine.skin);
        assert_eq!(store::load(&b).skin, mine.skin);
        let _ = fs::remove_dir_all(&tmp);
    }

    /// v3.9.0：使う立ち絵の選択は zip の値を使う（ほかの着せ替えの設定はこの PC の値のまま）。値が無い zip（3.8.1 まで）は
    /// この PC の今の選択を変えない（直し1回目の作者の決定）。値があって壊れていれば「スキンの立ち絵」。立ち絵の画像は zip に入らない
    /// （この PC に無い立ち絵を指していたときに戻すのは skins::runtime::after_settings_import。決まりは skins::avatars::resolve のテスト）
    #[test]
    fn import_takes_avatar_choice_from_zip() {
        let tmp = tmp_dir("avatar");
        let mine_db = |tag: &str| {
            let b = Db::open_memory(tag).unwrap();
            let mut mine = AppSettings::default();
            mine.skin.active_skin_id = "quiet-paper".into();
            // v3.9.0：大きさの上限は 2.0。125% を超える値も、この PC の値のまま残る
            mine.skin.character_scale = 1.8;
            mine.skin.set_avatar("none");
            store::save(&b, &mine).unwrap();
            (b, mine)
        };
        // 3.9.0 の zip：選択が入る
        for v in ["avatar:0123456789abcdef", "skin:from-other-pc", "none", "skin"] {
            let a = Db::open_memory(&format!("backup_avatar_a_{v}")).unwrap();
            let mut s = AppSettings::default();
            s.skin.active_skin_id = "from-other-pc".into();
            s.skin.character_scale = 1.25;
            s.skin.set_avatar(v);
            let (entries, _) = export_entries(&a, &s, &tmp.join("no-bg")).unwrap();
            let settings_json: serde_json::Value = serde_json::from_slice(&entries.iter().find(|e| e.name == "settings.json").unwrap().data).unwrap();
            assert_eq!(settings_json["skin"]["avatar"], v);
            // 画像は入らない（設定の zip に入るのは決まった名前だけ）
            assert!(entries.iter().all(|e| !e.name.starts_with("avatars")));
            let (b, mine) = mine_db(&format!("backup_avatar_b_{v}"));
            let r = import_entries(&b, &entries, &tmp.join("bgs")).unwrap();
            assert_eq!(r.settings.skin.avatar, v);
            assert_eq!(r.settings.skin.character_visible, v != "none");
            assert_eq!(r.settings.skin.active_skin_id, mine.skin.active_skin_id);
            assert!((r.settings.skin.character_scale - 1.8).abs() < 1e-6);
            assert!((store::load(&b).skin.character_scale - 1.8).abs() < 1e-6);
            assert_eq!(store::load(&b).skin.avatar, v);
        }
        // 値が無い zip（3.8.1 まで）：この PC の今の選択を変えない（作者の決定）。この PC が「出さない」でも「作った立ち絵」でも、そのまま
        let old = br#"{"theme":"light","skin":{"activeSkinId":"from-other-pc","characterVisible":true,"characterScale":1.2,"safeModeNextStart":false}}"#;
        let no_skin_at_all = br#"{"theme":"light"}"#;
        for (i, json) in [&old[..], &no_skin_at_all[..]].into_iter().enumerate() {
            for mine_choice in ["none", "avatar:0123456789abcdef", "skin:quiet-paper", "skin"] {
                let (b, mine) = mine_db(&format!("backup_avatar_old_{i}_{mine_choice}"));
                let mut m = mine.clone();
                m.skin.set_avatar(mine_choice);
                store::save(&b, &m).unwrap();
                let entries = vec![manifest(), zip::Entry { name: "settings.json".into(), data: json.to_vec() }];
                let r = import_entries(&b, &entries, &tmp.join("bgs")).unwrap();
                assert_eq!(r.settings.theme, "light");
                assert_eq!(r.settings.skin.avatar, mine_choice, "{i}");
                assert_eq!(r.settings.skin.character_visible, mine_choice != "none");
                assert_eq!(r.settings.skin.active_skin_id, mine.skin.active_skin_id);
                assert_eq!(store::load(&b).skin, m.skin);
            }
        }
        // 値があって壊れている（形の違う文字・空の文字・文字でない値）→ スキンの立ち絵（この PC が「出さない」でも）
        let bad = br#"{"theme":"light","skin":{"avatar":"avatar:../../x"}}"#;
        let empty = br#"{"theme":"light","skin":{"avatar":""}}"#;
        let not_text = br#"{"theme":"light","skin":{"avatar":42}}"#;
        let null = br#"{"theme":"light","skin":{"avatar":null}}"#;
        for (i, json) in [&bad[..], &empty[..], &not_text[..], &null[..]].into_iter().enumerate() {
            let (b, mine) = mine_db(&format!("backup_avatar_bad_{i}"));
            let entries = vec![manifest(), zip::Entry { name: "settings.json".into(), data: json.to_vec() }];
            let r = import_entries(&b, &entries, &tmp.join("bgs")).unwrap();
            assert_eq!(r.settings.theme, "light");
            assert_eq!(r.settings.skin.avatar, "skin", "{i}");
            assert!(r.settings.skin.character_visible);
            assert_eq!(r.settings.skin.active_skin_id, mine.skin.active_skin_id);
            assert_eq!(store::load(&b).skin.avatar, "skin");
        }
        let _ = fs::remove_dir_all(&tmp);
    }

    fn outputs_json(on: bool, delay: u32) -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "slots": [
                {"device": {"id": null, "name": ""}, "on": on, "delayMs": 0, "icon": null, "volume": {"normal": 1.0, "detail": 1.0}, "followMaster": true},
                {"device": {"id": "wasapi:{test-out-b}", "name": "テスト用スピーカー"}, "on": on, "delayMs": delay, "icon": "speaker", "volume": {"normal": 0.5, "detail": 0.25}, "followMaster": false},
                null
            ],
            "master": {"normal": 0.7, "detail": 0.4},
            "remembered": [{"key": "wasapi:{test-out-z}", "name": "外したテスト用デバイス", "delayMs": 12, "icon": null, "volume": {"normal": 1.0, "detail": 1.0}, "followMaster": true, "removedAt": 5}]
        })
    }

    /// v3.8.0：出力先の設定は outputs.json として書き出す（デバイス・覚えも全部）。表示のモードは入れない。
    /// 読み込むと全部オフで返り（一時停止は司令塔）、デバイス・音量・遅らせ量・アイコン・覚えは zip のとおり
    #[test]
    fn outputs_round_trip_and_import_all_off() {
        let a = Db::open_memory("backup_outputs_a").unwrap();
        db::settings::set(&a, OUTPUTS_KEY, &outputs_json(true, 300)).unwrap();
        crate::playback::outputs::save_mode(&a, crate::playback::outputs::Mode::Detail).unwrap();
        let (entries, r) = export_entries(&a, &AppSettings::default(), Path::new("no-bg")).unwrap();
        assert!(r.outputs);
        assert!(entries.iter().any(|e| e.name == "outputs.json"));
        assert!(entries.iter().all(|e| !e.name.contains("mode")));
        let b = Db::open_memory("backup_outputs_b").unwrap();
        let before = serde_json::json!({"version": 1, "slots": [null, null, null], "master": {"normal": 1.0, "detail": 1.0}, "remembered": []});
        db::settings::set(&b, OUTPUTS_KEY, &before).unwrap();
        let imp = import_entries(&b, &entries, Path::new("no-bg")).unwrap();
        assert!(imp.report.outputs);
        let o = imp.outputs.unwrap();
        assert!(o.slots.iter().flatten().all(|s| !s.on));
        let s1 = o.slot(1).unwrap();
        assert_eq!(s1.device.id.as_deref(), Some("wasapi:{test-out-b}"));
        assert_eq!((s1.delay_ms, s1.icon.as_deref(), s1.follow_master), (300, Some("speaker"), false));
        assert_eq!(o.master.detail, 0.4);
        assert_eq!(o.remembered.len(), 1);
        // DB へ書くのは司令塔（一時停止してから）。backup は書かない
        assert_eq!(db::settings::get(&b, OUTPUTS_KEY).unwrap().unwrap(), before);
        // 表示のモードは読み込まない
        assert!(db::settings::get(&b, crate::playback::outputs::MODE_KEY).unwrap().is_none());
    }

    /// v3.8.0：outputs.json の無い zip（3.7.0 まで）は出力に触らない
    #[test]
    fn old_zip_without_outputs_leaves_outputs_alone() {
        let b = Db::open_memory("backup_outputs_old").unwrap();
        let entries = vec![manifest(), zip::Entry { name: "settings.json".into(), data: br#"{"theme":"light"}"#.to_vec() }];
        let imp = import_entries(&b, &entries, Path::new("no-bg")).unwrap();
        assert!(imp.outputs.is_none());
        assert!(!imp.report.outputs);
        assert_eq!(imp.settings.theme, "light");
        // 書き出しも、DB に出力先の設定が無ければ入れない
        let (entries, r) = export_entries(&b, &AppSettings::default(), Path::new("no-bg")).unwrap();
        assert!(!r.outputs && entries.iter().all(|e| e.name != "outputs.json"));
    }

    /// v3.8.0：outputs.json の値が1つでも外れていたら、zip 全体を断る。何も書かない（ライブラリのフォルダ・プレイリスト・設定）
    #[test]
    fn broken_outputs_rejects_the_whole_zip() {
        let good_lib = zip::Entry { name: "library.json".into(), data: br#"[{"path":"C:\\Music","recursive":true}]"#.to_vec() };
        let good_pl = zip::Entry { name: "playlists.json".into(), data: br#"[{"name":"p","paths":[]}]"#.to_vec() };
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("delay", serde_json::to_vec(&outputs_json(true, 5000)).unwrap()),
            ("json", b"{\"slots\": [".to_vec()),
            ("slots", {
                let mut v = outputs_json(false, 0);
                v["slots"] = serde_json::json!([null, null, null, null]);
                serde_json::to_vec(&v).unwrap()
            }),
            ("dup", {
                let mut v = outputs_json(false, 0);
                v["slots"][2] = v["slots"][1].clone();
                serde_json::to_vec(&v).unwrap()
            }),
        ];
        for (name, data) in cases {
            let db = Db::open_memory(&format!("backup_outputs_bad_{name}")).unwrap();
            let entries = vec![
                manifest(),
                zip::Entry { name: "settings.json".into(), data: br#"{"theme":"light"}"#.to_vec() },
                good_lib.clone(),
                good_pl.clone(),
                zip::Entry { name: "outputs.json".into(), data },
            ];
            let e = import_entries(&db, &entries, Path::new("no-bg")).err().unwrap_or_else(|| panic!("{name}: imported"));
            assert_eq!(e.kind, ErrorKind::UnsupportedFormat, "{name}");
            assert!(e.detail.contains("outputs.json"), "{name}: {}", e.detail);
            assert!(import::library_folders(&db).is_empty(), "{name}: folders added");
            assert!(db::playlists::list(&db).unwrap().is_empty(), "{name}: playlists added");
            assert!(db::settings::get(&db, "app").unwrap().is_none(), "{name}: settings saved");
        }
        // zip から取り出す名前に outputs.json が入っている
        assert!(wanted("outputs.json"));
    }
}
