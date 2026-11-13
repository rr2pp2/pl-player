//! 曲ファイルをごみ箱へ移し、ライブラリから外す（v3.3.0。データの整理用。ライブラリの曲一覧の右クリックと、設定でオンにした行のボタンから。確認は画面が出す）。
//!
//! 決まり：
//! - 画面からは曲の id だけを受ける。パスは DB から引く（ライブラリの曲であることを、ここで確かめる）
//! - 先に全部の曲を確かめ、1曲でも断る理由があれば何も動かさずにエラー（`plan`）：
//!   ライブラリに無い id／ネットワーク上の場所（文字だけで決める。触らない）／音声ファイルの拡張子でない・フォルダ・リンク／
//!   ごみ箱の無いドライブ（固定ディスク以外。USB メモリ・CD・RAM ディスク。書かれたパスの先頭の文字ではなく、
//!   ジャンクション・リンクをたどった本当の場所のボリュームで見る。本当の場所がネットワーク上なら断る）／ファイルが無い
//! - 再生中・読み込み中の曲なら、先に手放す（止める）。送られていない別の綴りの行でも、同じファイルなら手放す。
//!   移せなかったら、元の位置から再開する
//! - 1曲ずつごみ箱へ移す（`platform::windows::recycle`。ごみ箱へ入らず完全に消える項目は、消す前に取りやめる）。
//!   **移せた曲だけ**をライブラリから外す（「ライブラリから外す」と同じ `tracks::remove`。プレイリスト・キュー・
//!   手動と外部の歌詞・歌詞の補正・ネットで見つからなかった記録は DB の外部キー（ON DELETE CASCADE）で一緒に消え、
//!   お気に入りは曲の行の印なので一緒に消える）。移せなかった曲は、ファイルもライブラリもそのまま

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::db::{tracks, Db};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::library::scanner;
use crate::platform::windows::{localpath, recycle};
use crate::playback::Controller;

/// 結果。移せた曲の id と、移せなかった曲（種別つき）。
/// `left_in_library`：ファイルはごみ箱へ移せたのに、ライブラリから外せなかった曲（DB の書き込みに失敗。種別つき）。
/// この曲は `moved` にも入る（ファイルはもう元の場所に無い）。「ライブラリから外す」で外せる
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashReport {
    pub moved: Vec<i64>,
    pub failed: Vec<TrashFailure>,
    pub left_in_library: Vec<TrashFailure>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashFailure {
    pub track_id: i64,
    /// エラーの種別（画面の messages\ja.ts で文を引く）
    pub code: ErrorKind,
    pub detail: String,
}

/// 確かめ終わった、移すファイル。`ids` は、そのファイルを指すライブラリの曲（送られた id のうち）。
/// 同じファイルを大文字小文字違い・短い名前などの別の綴りで指す行が2つ以上あれば、1つにまとめる（ファイルは1回だけ移し、
/// 移せたら送られたその行を全部外す）。送られていない行はそのまま（ほかの消えたファイルと同じく「見つからない」曲として残る）
/// `real` は OS が返す本当の場所（`canonicalize`。同じファイルかを比べる・ドライブの種類を見るのに使う）。移すのは `path`（DB の綴り）
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    pub ids: Vec<i64>,
    pub path: PathBuf,
    pub real: PathBuf,
}

/// 1回に移せる曲の数の上限（画面の取り違えで、ライブラリ全部を一度に送らないための歯止め）。
/// 画面（src\layout\visibility.ts の TRASH_MAX_TRACKS）も同じ数で、確認の前に断る
pub const MAX_TRACKS: usize = 1000;

/// 移す前の確かめ。1曲でも断るなら、何も動かさずにエラー
pub fn plan(db: &Db, ids: &[i64]) -> AppResult<Vec<Planned>> {
    plan_with(db, ids, &localpath::volume_drive_type_of_real)
}

/// 本当の場所（`canonicalize` の結果）が、ごみ箱へ移してよい場所か。
/// ネットワーク上（`\\?\UNC\…`。ボリュームの種類が DRIVE_REMOTE も）→ `NetworkPathUnsupported`、
/// ドライブの文字の無いボリュームなど形の分からない場所・固定ディスク以外・種類が分からない → `TrashUnavailable`。
/// `drive_type` は本当の場所を受け取る（ドライブの文字で始まる場所のときだけ呼ぶ）
pub(crate) fn check_real_location(real: &Path, drive_type: &dyn Fn(&Path) -> Option<u32>) -> AppResult<()> {
    match localpath::real_place(real) {
        localpath::RealPlace::Network => {
            return Err(AppError::new(ErrorKind::NetworkPathUnsupported, format!("{} is on the network", real.display())))
        }
        localpath::RealPlace::Unknown => {
            return Err(AppError::new(ErrorKind::TrashUnavailable, format!("{}: unknown kind of location", real.display())))
        }
        localpath::RealPlace::Disk => {}
    }
    match drive_type(real) {
        Some(t) if localpath::recycle_drive_allowed(t) => Ok(()),
        Some(localpath::drive::REMOTE) => {
            Err(AppError::new(ErrorKind::NetworkPathUnsupported, format!("{} is on a network drive", real.display())))
        }
        t => Err(AppError::new(ErrorKind::TrashUnavailable, format!("{}: drive type {t:?}", real.display()))),
    }
}

/// 再生中の曲が、移すファイルのどれか（`plan` の何番目か）。送られた id か、送られていなくても同じファイル
/// （大文字小文字違い・短い名前など別の綴りの行。`canonicalize` した本当の場所が同じ）なら、その番号。
/// 再生中の曲の場所がネットワーク上・形の分からないものなら、触らずに None（`check_local` で先に断る）
pub(crate) fn current_in_plan(plan: &[Planned], current_id: i64, current_path: &Path) -> Option<usize> {
    if let Some(i) = plan.iter().position(|p| p.ids.contains(&current_id)) {
        return Some(i);
    }
    localpath::check_local(current_path).ok()?;
    let real = std::fs::canonicalize(current_path).ok()?;
    plan.iter().position(|p| p.real == real)
}

pub(crate) fn plan_with(db: &Db, ids: &[i64], drive_type: &dyn Fn(&Path) -> Option<u32>) -> AppResult<Vec<Planned>> {
    if ids.is_empty() {
        return Err(AppError::other("no tracks"));
    }
    let mut seen = std::collections::HashSet::new();
    let ids: Vec<i64> = ids.iter().copied().filter(|id| seen.insert(*id)).collect();
    if ids.len() > MAX_TRACKS {
        return Err(AppError::new(ErrorKind::TrashTooMany, format!("too many tracks ({} > {MAX_TRACKS})", ids.len())));
    }
    let mut out: Vec<Planned> = Vec::with_capacity(ids.len());
    // 同じファイルか：OS が返す本当の場所（canonicalize。大文字小文字・短い名前・途中のジャンクションをそろえる）で比べる。
    // 小文字にはそろえない（大文字小文字を区別するフォルダでは、別のファイルのため）
    let mut by_file: std::collections::HashMap<PathBuf, usize> = std::collections::HashMap::new();
    for id in ids {
        let path = tracks::path_of(db, id)?.ok_or_else(|| AppError::new(ErrorKind::TrackNotInLibrary, format!("track {id}")))?;
        let p = PathBuf::from(&path);
        // ネットワーク上の場所・形の分からないパスは、触る前に断る
        localpath::check_local(&p)?;
        if !scanner::is_audio_path(&p) {
            return Err(AppError::new(ErrorKind::TrashNotAllowed, format!("track {id}: not an audio file name")));
        }
        // 通常のファイルだけ（フォルダ・リンク・ジャンクションは断る。無ければ「見つからない」）。移す直前にも同じ確かめをする
        recycle::check_regular_file(&p).map_err(|e| AppError::new(e.kind, format!("track {id}: {}", e.detail)))?;
        let real = std::fs::canonicalize(&p).map_err(|e| AppError::from_io(&e, &p))?;
        // ドライブの種類は、本当の場所で見る（途中のフォルダがジャンクション・リンクで USB メモリやネットワーク上を指していても、
        // 書かれたパスの先頭の文字ではなく、行き着いた先のボリュームで決める）
        check_real_location(&real, drive_type).map_err(|e| AppError::new(e.kind, format!("track {id}: {}", e.detail)))?;
        match by_file.get(&real) {
            Some(&i) => out[i].ids.push(id),
            None => {
                by_file.insert(real.clone(), out.len());
                out.push(Planned { ids: vec![id], path: p, real });
            }
        }
    }
    Ok(out)
}

/// ごみ箱へ移して、移せた曲をライブラリから外す（`controller` があれば、再生中の曲を手放してから）
pub fn trash_tracks(db: &Db, controller: Option<&Controller>, ids: &[i64]) -> AppResult<TrashReport> {
    trash_tracks_with(db, controller, ids, &localpath::volume_drive_type_of_real, &recycle::move_to_recycle_bin)
}

/// `trash_tracks` の中身。ドライブの種類（本当の場所を受け取る）と、ファイルを移す処理を差し替えられる（テスト用）。
/// 移す処理（`recycle::move_to_recycle_bin`）は、移す直前にもう一度「通常のファイルか」を確かめる
/// （`plan` のあとで同じ名前のフォルダ・リンクに差し替えられても、それは移さない）
#[doc(hidden)]
pub fn trash_tracks_with(
    db: &Db,
    controller: Option<&Controller>,
    ids: &[i64],
    drive_type: &dyn Fn(&Path) -> Option<u32>,
    mover: &dyn Fn(&Path) -> Result<(), AppError>,
) -> AppResult<TrashReport> {
    let plan = plan_with(db, ids, drive_type)?;
    // 再生中・読み込み中の曲が移すファイルなら、先に止めてファイルを手放す。
    // 送られた行でなくても、同じファイルを別の綴りで指す行が鳴っていれば手放す（`current_in_plan`）
    let current: Option<(i64, usize)> = controller
        .and_then(|c| c.snapshot().track)
        .and_then(|t| current_in_plan(&plan, t.id, Path::new(&t.path)).map(|i| (t.id, i)));
    let resume = match (controller, current) {
        (Some(c), Some((id, _))) => c.release_for_write(id),
        _ => None,
    };
    let mut report = TrashReport::default();
    let mut moved_files = vec![false; plan.len()];
    for (i, p) in plan.iter().enumerate() {
        match mover(&p.path) {
            Ok(()) => {
                moved_files[i] = true;
                report.moved.extend(&p.ids);
            }
            Err(e) => {
                tracing::warn!("trash: tracks {:?} were not moved: {:?}", p.ids, e.kind);
                report.failed.extend(p.ids.iter().map(|&id| TrashFailure {
                    track_id: id,
                    code: e.kind,
                    detail: e.detail.clone(),
                }));
            }
        }
    }
    // 移せた曲をライブラリから外す（1つのトランザクション。失敗したら1曲も外れない）。
    // 失敗しても、移せた曲の報告は返す（ファイルはもう元の場所に無い。外せなかった曲を種別つきで知らせる）
    if !report.moved.is_empty() {
        if let Err(e) = tracks::remove(db, &report.moved) {
            tracing::warn!("trash: moved files but could not remove them from the library: {:?}", e.kind);
            report.left_in_library = report
                .moved
                .iter()
                .map(|&id| TrashFailure {
                    track_id: id,
                    code: e.kind,
                    detail: e.detail.clone(),
                })
                .collect();
        }
    }
    if let Some(c) = controller {
        // ファイルが無くなった曲は、ライブラリから外せなかったときもキューと今の並びから外す（鳴らせないため）。
        // 鳴っていたのが送られていない別の綴りの行で、そのファイルを移せたときは、その行もキューと今の並びから外して止める
        // （ライブラリには残る。ほかの消えたファイルと同じく「見つからない」曲）
        let mut gone = report.moved.clone();
        if let Some((id, i)) = current {
            if moved_files[i] && !gone.contains(&id) {
                gone.push(id);
            }
        }
        if !gone.is_empty() {
            c.on_tracks_removed(&gone);
        }
        // 再生中の曲のファイルを移せなかった：元の位置から戻す（移せたら止めたまま）
        if let (Some(r), Some((_, i))) = (resume, current) {
            if !moved_files[i] {
                c.resume_after_write(r);
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::lyrics::{self, StoredKind};
    use crate::db::tracks::{upsert, TrackInfo};
    use crate::db::{playlists, queue};
    use crate::platform::windows::localpath::drive;

    fn tmp(tag: &str) -> PathBuf {
        let d = crate::test_tmp().join(format!("plp-trash-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn add(db: &Db, path: &Path) -> i64 {
        upsert(&db.writer(), &TrackInfo { path: path.to_string_lossy().into_owned(), title: "架空の曲".into(), ..Default::default() })
            .unwrap()
            .0
    }

    fn write_song(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, b"pl-player test file (not real audio)").unwrap();
        p
    }

    fn count(db: &Db, sql: &str, id: i64) -> i64 {
        db.reader().query_row(sql, [id], |r| r.get(0)).unwrap()
    }

    /// 移す処理が呼ばれたら失敗にする（断るときに何も動かしていない証拠）
    fn never(p: &Path) -> Result<(), AppError> {
        panic!("mover must not be called: {}", p.display())
    }

    /// 実際のごみ箱へ移し、テストの項目だけをごみ箱から消す。ごみ箱の中の場所を覚える
    fn recycle_and_purge(bin: &std::sync::Mutex<Vec<PathBuf>>) -> impl Fn(&Path) -> Result<(), AppError> + '_ {
        move |p| {
            let in_bin = recycle::move_to_recycle_bin_then_purge(p)?;
            if let Some(b) = in_bin {
                bin.lock().unwrap().push(b);
            }
            Ok(())
        }
    }

    /// 一時フォルダの曲を実際のごみ箱へ移す：ファイルが元の場所から消え、ライブラリ・プレイリスト・キュー・お気に入り・
    /// 歌詞（手動・外部）・歌詞の補正・ネットで見つからなかった記録からも外れる。ほかの曲はそのまま。
    /// 終わったら、ごみ箱からもその項目だけを消す（消えたことも確かめる）
    #[test]
    fn moves_to_recycle_bin_and_removes_from_library() {
        let dir = tmp("ok");
        let a = write_song(&dir, "01 架空の曲.mp3");
        let b = write_song(&dir, "02 keep.flac");
        let db = Db::open_memory("trash_ok").unwrap();
        let (ia, ib) = (add(&db, &a), add(&db, &b));
        tracks::set_favorite(&db, &[ia, ib], true).unwrap();
        let pl = playlists::create(&db, "テスト").unwrap();
        playlists::add_tracks(&db, pl, &[ia, ib, ia]).unwrap();
        queue::save(&db, &[ia, ib]).unwrap();
        lyrics::upsert(&db.writer(), ia, StoredKind::Manual, "[00:01.00]a", "timed").unwrap();
        lyrics::upsert(&db.writer(), ia, StoredKind::External, "a", "plain").unwrap();
        lyrics::set_delay(&db, ia, 300).unwrap();
        lyrics::record_miss(&db, ia, "not_found", "sig").unwrap();

        let bin = std::sync::Mutex::new(vec![]);
        let r = trash_tracks_with(&db, None, &[ia], &localpath::volume_drive_type_of_real, &recycle_and_purge(&bin)).unwrap();
        assert_eq!(r.moved, vec![ia]);
        assert!(r.failed.is_empty(), "{r:?}");
        assert!(!a.exists());
        assert!(b.exists());
        assert!(tracks::get(&db, ia).unwrap().is_none());
        assert!(tracks::get(&db, ib).unwrap().unwrap().favorite);
        assert_eq!(playlists::track_ids(&db, pl).unwrap(), vec![ib]);
        assert_eq!(queue::load(&db).unwrap(), vec![ib]);
        assert_eq!(count(&db, "SELECT count(*) FROM lyrics_sources WHERE track_id = ?1", ia), 0);
        assert_eq!(count(&db, "SELECT count(*) FROM track_lyrics_settings WHERE track_id = ?1", ia), 0);
        assert_eq!(count(&db, "SELECT count(*) FROM lyrics_fetch_misses WHERE track_id = ?1", ia), 0);
        // ごみ箱に入ったことと、テストの項目をごみ箱から消せたこと
        let bin = bin.into_inner().unwrap();
        assert_eq!(bin.len(), 1, "the shell returned the recycled item");
        assert!(bin[0].to_string_lossy().to_ascii_lowercase().contains("$recycle.bin"), "{}", bin[0].display());
        assert!(!bin[0].exists(), "purged from the recycle bin: {}", bin[0].display());
        let info = recycle::info_file_of(&bin[0]).unwrap();
        assert!(!info.exists(), "info file purged too: {}", info.display());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 断るとき：1曲でも断る理由があれば、何も動かさず、何も外さない
    #[test]
    fn refuses_without_touching_anything() {
        let dir = tmp("refuse");
        let good = write_song(&dir, "good.mp3");
        let note = write_song(&dir, "note.txt");
        let folder = dir.join("folder.mp3");
        std::fs::create_dir_all(&folder).unwrap();
        let gone = dir.join("gone.mp3");
        let db = Db::open_memory("trash_refuse").unwrap();
        let ig = add(&db, &good);
        let it = add(&db, &note);
        let id_dir = add(&db, &folder);
        let id_gone = add(&db, &gone);
        let id_unc = add(&db, Path::new(r"\\pl-player-test.invalid\share\a.mp3"));
        let id_rel = add(&db, Path::new(r"music\a.mp3"));
        let fixed = |_: &Path| Some(drive::FIXED);
        let kind = |ids: &[i64], dt: &dyn Fn(&Path) -> Option<u32>| trash_tracks_with(&db, None, ids, dt, &never).unwrap_err().kind;
        // ライブラリに無い id（もう外した・作った id）。正しい曲と一緒でも全部断る
        assert_eq!(kind(&[987_654], &fixed), ErrorKind::TrackNotInLibrary);
        assert_eq!(kind(&[ig, 987_654], &fixed), ErrorKind::TrackNotInLibrary);
        // ライブラリの曲ファイルとして確かめられない（DB が指す先が音声ファイルの名前でない・フォルダ）
        assert_eq!(kind(&[it], &fixed), ErrorKind::TrashNotAllowed);
        assert_eq!(kind(&[ig, id_dir], &fixed), ErrorKind::TrashNotAllowed);
        // ネットワーク上の場所・ドライブの無いパスは、触らずに断る
        assert_eq!(kind(&[id_unc], &fixed), ErrorKind::NetworkPathUnsupported);
        assert_eq!(kind(&[id_rel], &fixed), ErrorKind::NetworkPathUnsupported);
        // ごみ箱の無いドライブ（USB メモリ・CD・RAM ディスク・種類が分からない）
        for t in [drive::REMOVABLE, drive::CDROM, drive::RAMDISK, drive::UNKNOWN] {
            assert_eq!(kind(&[ig], &move |_: &Path| Some(t)), ErrorKind::TrashUnavailable, "{t}");
        }
        // ファイルが無い（ライブラリから外すのは「ライブラリから外す」で）
        assert_eq!(kind(&[id_gone], &fixed), ErrorKind::FileNotFound);
        // 空・多すぎ
        assert_eq!(kind(&[], &fixed), ErrorKind::Other);
        let many: Vec<i64> = (1..=(MAX_TRACKS as i64 + 1)).collect();
        assert_eq!(kind(&many, &fixed), ErrorKind::TrashTooMany);
        // ちょうど上限なら、数では断らない（ここではライブラリに無い id で断られる）
        let edge: Vec<i64> = (1_000_000..1_000_000 + MAX_TRACKS as i64).collect();
        assert_eq!(kind(&edge, &fixed), ErrorKind::TrackNotInLibrary);
        // どれも何も変えていない
        assert!(good.exists() && note.exists() && folder.is_dir());
        for id in [ig, it, id_dir, id_gone, id_unc, id_rel] {
            assert!(tracks::get(&db, id).unwrap().is_some(), "{id}");
        }
        // 同じ id が2つ来ても1回だけ（移す処理の回数で確かめる）
        let calls = std::sync::Mutex::new(0);
        let r = trash_tracks_with(&db, None, &[ig, ig], &fixed, &|_| {
            *calls.lock().unwrap() += 1;
            Err(AppError::new(ErrorKind::FileInUse, "test"))
        })
        .unwrap();
        assert_eq!(*calls.lock().unwrap(), 1);
        assert!(r.moved.is_empty());
        assert_eq!(r.failed.len(), 1);
        assert_eq!(r.failed[0].code, ErrorKind::FileInUse);
        // 移せなかった曲はライブラリに残る
        assert!(tracks::get(&db, ig).unwrap().is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ほかのアプリが使っている（共有なしで開いている）曲は移せない：種別は「使用中」、ファイルもライブラリもそのまま。
    /// 一緒に送った別の曲は移る（移せた曲だけを外す）
    #[test]
    fn file_in_use_is_reported_and_kept() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tmp("inuse");
        let busy = write_song(&dir, "busy.mp3");
        let free = write_song(&dir, "free.ogg");
        let db = Db::open_memory("trash_inuse").unwrap();
        let (ib, ifr) = (add(&db, &busy), add(&db, &free));
        let lock = std::fs::OpenOptions::new().read(true).share_mode(0).open(&busy).unwrap();
        let bin = std::sync::Mutex::new(vec![]);
        let r = trash_tracks_with(&db, None, &[ib, ifr], &localpath::volume_drive_type_of_real, &recycle_and_purge(&bin)).unwrap();
        drop(lock);
        assert_eq!(r.failed.len(), 1, "{r:?}");
        assert_eq!(r.failed[0].track_id, ib);
        assert_eq!(r.failed[0].code, ErrorKind::FileInUse, "{r:?}");
        assert_eq!(r.moved, vec![ifr]);
        assert!(busy.exists());
        assert!(!free.exists());
        assert!(tracks::get(&db, ib).unwrap().is_some());
        assert!(tracks::get(&db, ifr).unwrap().is_none());
        let bin = bin.into_inner().unwrap();
        assert_eq!(bin.len(), 1);
        for b in bin {
            assert!(!b.exists(), "purged: {}", b.display());
            assert!(!recycle::info_file_of(&b).unwrap().exists());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    /// 確かめたあと・移す前に、同じ名前のフォルダへ差し替えられた：移す直前の確かめで断り、フォルダもその中身もそのまま。
    /// ライブラリからも外さない（移す処理は本物。万一入ってしまったときのために、ごみ箱から消す形で呼ぶ）
    #[test]
    fn swapped_for_a_folder_after_planning_is_not_moved() {
        let dir = tmp("swap");
        let song = write_song(&dir, "swap.mp3");
        let db = Db::open_memory("trash_swap").unwrap();
        let id = add(&db, &song);
        let bin = std::sync::Mutex::new(vec![]);
        let real = recycle_and_purge(&bin);
        let r = trash_tracks_with(&db, None, &[id], &localpath::volume_drive_type_of_real, &|p: &Path| {
            std::fs::remove_file(p).unwrap();
            std::fs::create_dir_all(p).unwrap();
            std::fs::write(p.join("inner.mp3"), b"x").unwrap();
            real(p)
        })
        .unwrap();
        assert!(r.moved.is_empty(), "{r:?}");
        assert_eq!(r.failed.len(), 1);
        assert_eq!(r.failed[0].code, ErrorKind::TrashNotAllowed, "{r:?}");
        assert!(song.join("inner.mp3").exists());
        assert!(tracks::get(&db, id).unwrap().is_some());
        drop(real);
        assert!(bin.into_inner().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ファイルは移せたのに、ライブラリから外す書き込みが失敗した：移せた曲の報告は返し、外せなかった曲を種別つきで知らせる。
    /// （DB に「曲の行を消すと失敗する」仕掛けを置いて起こす。移す処理は、ファイルを消すだけの代わり）
    #[test]
    fn reports_moved_tracks_even_if_library_update_fails() {
        let dir = tmp("dbfail");
        let a = write_song(&dir, "a.mp3");
        let b = write_song(&dir, "b.flac");
        let db = Db::open_memory("trash_dbfail").unwrap();
        let (ia, ib) = (add(&db, &a), add(&db, &b));
        db.writer()
            .execute_batch("CREATE TEMP TRIGGER plp_test_no_delete BEFORE DELETE ON tracks BEGIN SELECT RAISE(ABORT, 'test'); END;")
            .unwrap();
        let fixed = |_: &Path| Some(drive::FIXED);
        let r = trash_tracks_with(&db, None, &[ia, ib], &fixed, &|p: &Path| {
            if p.ends_with("b.flac") {
                return Err(AppError::new(ErrorKind::FileInUse, "test"));
            }
            std::fs::remove_file(p).map_err(|e| AppError::from_io(&e, p))
        })
        .unwrap();
        assert_eq!(r.moved, vec![ia]);
        assert_eq!(r.failed.len(), 1);
        assert_eq!(r.failed[0].track_id, ib);
        assert_eq!(r.left_in_library.len(), 1, "{r:?}");
        assert_eq!(r.left_in_library[0].track_id, ia);
        assert_eq!(r.left_in_library[0].code, ErrorKind::Database);
        assert!(!a.exists() && b.exists());
        assert!(tracks::get(&db, ia).unwrap().is_some());
        // 画面へ渡す形（camelCase）
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["leftInLibrary"][0]["trackId"], ia);
        assert_eq!(v["leftInLibrary"][0]["code"], "database");
        // 書き込みが通るときは空
        db.writer().execute_batch("DROP TRIGGER plp_test_no_delete;").unwrap();
        std::fs::write(&a, b"again").unwrap();
        let r = trash_tracks_with(&db, None, &[ia], &fixed, &|p: &Path| std::fs::remove_file(p).map_err(|e| AppError::from_io(&e, p))).unwrap();
        assert_eq!(r.moved, vec![ia]);
        assert!(r.left_in_library.is_empty());
        assert!(tracks::get(&db, ia).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 同じファイルを大文字小文字違いの2行で持つライブラリ：ファイルは1回だけ移し、送られた行を全部外す。
    /// 送られていない行はそのまま（ほかの消えたファイルと同じく残る）。綴りの違う別のファイルはまとめない
    #[test]
    fn same_file_under_different_spellings_is_moved_once() {
        let dir = tmp("case");
        let song = write_song(&dir, "Case Song.mp3");
        let other = write_song(&dir, "other.mp3");
        let db = Db::open_memory("trash_case").unwrap();
        let i1 = add(&db, &song);
        let i2 = add(&db, &dir.join("CASE SONG.MP3"));
        let i3 = add(&db, &dir.join("case song.Mp3"));
        let io = add(&db, &other);
        assert!(i1 != i2 && i2 != i3, "the library keeps the spellings as separate rows");
        let fixed = |_: &Path| Some(drive::FIXED);
        let p = plan_with(&db, &[i1, io, i2], &fixed).unwrap();
        assert_eq!(p.len(), 2, "{p:?}");
        assert_eq!(p[0].ids, vec![i1, i2]);
        assert_eq!(p[1].ids, vec![io]);
        let calls = std::sync::Mutex::new(vec![]);
        let r = trash_tracks_with(&db, None, &[i1, io, i2], &fixed, &|p: &Path| {
            calls.lock().unwrap().push(p.to_path_buf());
            std::fs::remove_file(p).map_err(|e| AppError::from_io(&e, p))
        })
        .unwrap();
        assert_eq!(calls.into_inner().unwrap(), vec![song.clone(), other.clone()]);
        assert_eq!(r.moved, vec![i1, i2, io]);
        assert!(r.failed.is_empty() && r.left_in_library.is_empty(), "{r:?}");
        for id in [i1, i2, io] {
            assert!(tracks::get(&db, id).unwrap().is_none(), "{id}");
        }
        // 送っていない3つ目の綴りは残る（ファイルはもう無い）
        assert!(tracks::get(&db, i3).unwrap().is_some());
        assert!(!song.exists());
        // 移せなかったときは、まとめた行が全部「移せなかった」に入る
        let again = write_song(&dir, "Again.ogg");
        let (j1, j2) = (add(&db, &again), add(&db, &dir.join("AGAIN.OGG")));
        let r = trash_tracks_with(&db, None, &[j1, j2], &fixed, &|_| Err(AppError::new(ErrorKind::FileInUse, "test"))).unwrap();
        assert_eq!(r.failed.iter().map(|f| f.track_id).collect::<Vec<_>>(), vec![j1, j2]);
        assert!(r.moved.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 再生中の曲が移すファイルか：送った行か、送っていない別の綴りの行でも同じファイルなら、その番号。
    /// 別のファイル・無いファイル・ネットワーク上の場所（触らない）は None
    #[test]
    fn current_track_under_another_spelling_is_found() {
        let dir = tmp("current");
        let song = write_song(&dir, "Now Playing.mp3");
        let other = write_song(&dir, "other.mp3");
        let db = Db::open_memory("trash_current").unwrap();
        let (i1, io) = (add(&db, &song), add(&db, &other));
        let alias = dir.join("NOW PLAYING.MP3");
        let i2 = add(&db, &alias);
        let fixed = |_: &Path| Some(drive::FIXED);
        let plan = plan_with(&db, &[io, i1], &fixed).unwrap();
        assert_eq!(plan.len(), 2);
        // 送った行
        assert_eq!(current_in_plan(&plan, i1, &song), Some(1));
        assert_eq!(current_in_plan(&plan, io, &other), Some(0));
        // 送っていない別の綴りの行（大文字小文字違い）
        assert_eq!(current_in_plan(&plan, i2, &alias), Some(1));
        // 別のファイル・無いファイル・ネットワーク上の場所
        let third = write_song(&dir, "third.mp3");
        assert_eq!(current_in_plan(&plan, 999, &third), None);
        assert_eq!(current_in_plan(&plan, 999, &dir.join("gone.mp3")), None);
        assert_eq!(current_in_plan(&plan, 999, Path::new(r"\\pl-player-test.invalid\share\Now Playing.mp3")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 本当の場所の形で断る：ネットワーク上（ボリュームの種類が DRIVE_REMOTE も）・ドライブの文字の無いボリュームなど。
    /// ネットワーク上・形の分からない場所は、ドライブの種類を聞かない
    #[test]
    fn real_location_decides() {
        let never_asked = |p: &Path| -> Option<u32> { panic!("drive type must not be asked: {}", p.display()) };
        for net in [r"\\?\UNC\pl-player-test.invalid\share\a.mp3", r"\\pl-player-test.invalid\share\a.mp3"] {
            assert_eq!(check_real_location(Path::new(net), &never_asked).unwrap_err().kind, ErrorKind::NetworkPathUnsupported, "{net}");
        }
        for odd in [r"\\?\Volume{00000000-0000-0000-0000-000000000000}\a.mp3", r"\\.\C:\a.mp3", r"\\?\GLOBALROOT\Device\HarddiskVolume1\a.mp3"] {
            assert_eq!(check_real_location(Path::new(odd), &never_asked).unwrap_err().kind, ErrorKind::TrashUnavailable, "{odd}");
        }
        let disk = Path::new(r"\\?\C:\Music\a.mp3");
        assert!(check_real_location(disk, &|_| Some(drive::FIXED)).is_ok());
        assert!(check_real_location(Path::new(r"C:\Music\a.mp3"), &|_| Some(drive::FIXED)).is_ok());
        assert_eq!(check_real_location(disk, &|_| Some(drive::REMOTE)).unwrap_err().kind, ErrorKind::NetworkPathUnsupported);
        for t in [drive::REMOVABLE, drive::CDROM, drive::RAMDISK, drive::UNKNOWN, drive::NO_ROOT_DIR] {
            assert_eq!(check_real_location(disk, &move |_| Some(t)).unwrap_err().kind, ErrorKind::TrashUnavailable, "{t}");
        }
        assert_eq!(check_real_location(disk, &|_| None).unwrap_err().kind, ErrorKind::TrashUnavailable);
    }

    /// ライブラリのフォルダがジャンクションの中：ドライブの種類は、ジャンクションの先（本当の場所）で聞く。
    /// 先が USB メモリに見立てた場所なら断り、何も動かさない。書かれたパス（ジャンクションの側）では聞かない。
    /// ジャンクションを作れない環境では、その確かめだけ飛ばす
    #[test]
    fn drive_type_is_asked_for_the_real_location_behind_a_junction() {
        let dir = tmp("junction");
        let usb = dir.join("usb-like");
        std::fs::create_dir_all(&usb).unwrap();
        let song = write_song(&usb, "far.mp3");
        let link = dir.join("music-link");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&usb)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        println!("junction made: {made}");
        if made {
            let db = Db::open_memory("trash_junction").unwrap();
            let id = add(&db, &link.join("far.mp3"));
            let real_usb = std::fs::canonicalize(&usb).unwrap();
            let asked = std::sync::Mutex::new(vec![]);
            let usb_like = |p: &Path| {
                asked.lock().unwrap().push(p.to_path_buf());
                if p.starts_with(&real_usb) {
                    Some(drive::REMOVABLE)
                } else {
                    Some(drive::FIXED)
                }
            };
            let e = trash_tracks_with(&db, None, &[id], &usb_like, &never).unwrap_err();
            assert_eq!(e.kind, ErrorKind::TrashUnavailable, "{e:?}");
            let asked = asked.into_inner().unwrap();
            assert_eq!(asked, vec![real_usb.join("far.mp3")], "asked with the real location, not the junction");
            assert!(song.exists());
            assert!(tracks::get(&db, id).unwrap().is_some());
            // 本物の聞き方でも、ジャンクションの先のボリューム（ここでは同じ固定ディスク）で決まる
            assert_eq!(localpath::volume_drive_type_of_real(&real_usb.join("far.mp3")), Some(drive::FIXED));
            let p = plan_with(&db, &[id], &localpath::volume_drive_type_of_real).unwrap();
            assert_eq!(p[0].path, link.join("far.mp3"), "moves the path the library holds");
            assert_eq!(p[0].real, real_usb.join("far.mp3"));
            let _ = std::fs::remove_dir(&link);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
