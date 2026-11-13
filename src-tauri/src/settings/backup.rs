//! 設定の書き出し・読み込み（原文「設定バックアップ」）。
//!
//! 対象：テーマ・背景・UI 設定・ショートカット・プレイリスト・お気に入り・ライブラリ設定。
//! 1つの zip（中身は JSON と背景画像）にまとめる。曲はパスで持つので、別の PC では見つからない曲が出る。
//! 読み込みのときに、その件数を知らせる。

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::store::{self, AppSettings};
use super::zip;
use crate::db::{self, Db};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::library::{artwork, import};
use crate::platform::windows::localpath;

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
}

/// 書き出し。`bg_dir` はデータフォルダの背景の置き場（v3.1.1：そこにある背景だけを zip に入れる）
pub fn export(db: &Db, settings: &AppSettings, bg_dir: &Path, dest: &Path) -> AppResult<BackupReport> {
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
    let mut f = fs::File::create(dest).map_err(|e| AppError::from_io(&e, dest))?;
    zip::write(&mut f, &entries).map_err(|e| AppError::from_io(&e, dest))?;
    Ok(report)
}

/// 設定 zip から取り出す名前（ほかの名前は中身を写さない。v3.1.1）
fn wanted(name: &str) -> bool {
    matches!(name, "manifest.json" | "settings.json" | "playlists.json" | "favorites.json" | "library.json")
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

/// 読み込み。設定は戻り値で返し、呼び出し側が保存・反映する。
/// ライブラリのフォルダは記録だけ足す（取り込み直しは呼び出し側が行う）。
///
/// v3.1.1：中身を全部確かめてから、背景・DB に書く。`settings.json` が無い・壊れている、
/// ほかの JSON が壊れているときは、何も変えずにエラーを返す（ペネトレーションテスト v3.0.2 の13）
pub fn import(db: &Db, src: &Path, bg_dir: &Path) -> AppResult<(AppSettings, BackupReport, Vec<(String, bool)>)> {
    let mut f = fs::File::open(src).map_err(|e| AppError::from_io(&e, src))?;
    let entries = zip::read(&mut f, &wanted)
        .map_err(|e| AppError::new(ErrorKind::UnsupportedFormat, format!("{}: {e}", src.display())))?;
    let manifest: Manifest = json_entry(&entries, "manifest.json")?
        .ok_or_else(|| AppError::new(ErrorKind::UnsupportedFormat, "manifest.json missing"))?;
    if manifest.format_version > FORMAT_VERSION {
        return Err(AppError::new(
            ErrorKind::UnsupportedFormat,
            format!("format version {}", manifest.format_version),
        ));
    }
    let mut settings: AppSettings = json_entry(&entries, "settings.json")?
        .ok_or_else(|| AppError::new(ErrorKind::UnsupportedFormat, "settings.json missing"))?;
    let folders: Vec<FolderOut> = json_entry(&entries, "library.json")?.unwrap_or_default();
    let favs: Vec<String> = json_entry(&entries, "favorites.json")?.unwrap_or_default();
    let pls: Vec<PlaylistOut> = json_entry(&entries, "playlists.json")?.unwrap_or_default();

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
    // v3.2.0：着せ替えの設定は読み込まない（スキンそのものは zip に入らず、別の PC には無いことがあるため）
    settings.skin = store::load(db).skin;
    store::save(db, &settings)?;
    let folder_list = folders.into_iter().map(|f| (f.path, f.recursive)).collect();
    Ok((settings, report, folder_list))
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
}
