//! 古い文字コードのタグを読み直す（v3.5.0）。設定の「ライブラリ」の「古い文字コードのタグを読み直す…」から、確認のあとで走る。
//!
//! **やり方：DB に残っている値に、今の決まり（`legacy_text::repair`）をかけ直す。曲ファイルは読まない・書かない。**
//! 曲ファイルのタグを読み直す形にしなかった理由（v3.5.0 で決めた）：
//! - 決まりで直らなかった値は、DB に読んだときのまま（タグの値の前後の空白を落としただけ）残っている。取り込み（`tags_read::read_track`）も
//!   同じ値・同じファイル名で `legacy_text::repair` を呼ぶので、DB の値にかけ直した結果は、ファイルを読み直して直した結果と同じになる
//! - 移行 4（v3.1.2）と同じ処理（`db::tracks::repair_fields`・`write_repaired`）を使えるので、ボタンと移行で決まりがずれない
//! - ファイルを開かないので速く、見つからない曲・外したドライブの曲・ネットワーク上の場所の曲にも触らずに済む（通信も起きない）
//! - できないこと：決まりが誤って漢字に変えた値は、DB にはもう元の文字が無いので戻せない（README の既知の制限。編集画面で直す）
//!
//! 守ること：
//! - **編集画面で手で変えた項目は書き換えない。**v3.5.0 から、編集画面で保存したときに、変えた項目に印を付ける（`tag_manual_edits`）。
//!   印の付いた項目には決まりを当てない。3.4.0 までに編集画面で保存した値には印が無く、DB からは見分けられないが、
//!   保存のあとの読み直しで今と同じ決まりを通っている（3.1.2 より前に保存した値は移行 4 で通った）ので、今の決まりのままなら変わらない
//!   （テスト `values_saved_in_the_editor_before_3_5_0_are_not_changed`）。決まりを変える版では、ここを考え直す
//! - 直してはいけない名前（ロシア語・西欧の引用符など）は、`legacy_text::repair` が直さない（同じ決まりを使う。テストで確かめる）
//! - 曲名がファイル名と同じなら当てない（移行 4 と同じ）
//! - 読んでから書くまでの間に値が変わった行・消えた行は書かない（`write_repaired`。編集画面の保存・ごみ箱へ移すと同時に走っても、
//!   手で入れた値を上書きせず、消えた曲を戻さない）
//! - 取り込み・再走査とは同時に走らせない（`begin_exclusive`・`begin_import`）。ごみ箱へ移す・ライブラリから外すは止めない
//!   （どちらも曲の行を消すだけで、消えた行は上の決まりで書かないため。ごみ箱へ移すは曲ファイルを動かすが、ここはファイルに触らない）
//! - 1つずつしか走らせない。「やめる」で、次のまとまり（`CHUNK` 曲）の前に止まる（それまでに直した曲はそのまま）

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::legacy_text;
use super::tags_read::TagValues;
use crate::db::tracks::{self, LegacyFields, Track};
use crate::db::Db;
use crate::error::{AppError, AppResult, ErrorKind};

/// 1回に読む曲の数（読み取りの鍵を長く持たない・「やめる」がすぐ効く大きさ）
pub const CHUNK: usize = 200;

/// 進み具合と結果（画面の `LegacyRereadProgress`）
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RereadProgress {
    pub running: bool,
    pub finished: bool,
    /// 「やめる」で止めた
    pub cancelled: bool,
    /// 始めたときのライブラリの曲数
    pub total: usize,
    /// 見た曲の数
    pub done: usize,
    /// 値を直した曲の数
    pub fixed: usize,
    /// 手で直した印のある項目なので、直さなかった曲の数（決まりを当てれば変わった曲だけを数える）
    pub skipped_manual: usize,
    /// 読んでから書くまでの間に、値が変わった・ライブラリから外されたので書かなかった曲の数
    pub changed_during_run: usize,
    /// 途中で止まったときのエラーの種別（DB の読み書きの失敗など）。それまでに直した曲はそのまま
    pub error: Option<ErrorKind>,
}

/// 走っているかの印・「やめる」の印・今の進み具合（設定画面を開き直したときに出す）
#[derive(Default)]
pub struct Control {
    running: Mutex<bool>,
    cancel: Arc<AtomicBool>,
    progress: Mutex<RereadProgress>,
}

/// `Control::begin` と対で使う。落ちても（drop で）必ず空ける
pub struct EndGuard<'a>(&'a Control);

impl Drop for EndGuard<'_> {
    fn drop(&mut self) {
        *self.0.running.lock().unwrap_or_else(|p| p.into_inner()) = false;
    }
}

impl Control {
    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// 走らせる印を立てる。走っていれば断る
    fn begin(&self) -> AppResult<()> {
        let mut running = self.running.lock().unwrap_or_else(|p| p.into_inner());
        if *running {
            return Err(AppError::new(ErrorKind::LegacyRereadBusy, "legacy tag reread is already running"));
        }
        *running = true;
        self.cancel.store(false, Ordering::SeqCst);
        Ok(())
    }
    pub fn end_guard(&self) -> EndGuard<'_> {
        EndGuard(self)
    }
    /// 「やめる」（走っていなければ何もしない。次に走らせたときに効かないように）
    pub fn cancel(&self) {
        if self.is_running() {
            self.cancel.store(true, Ordering::SeqCst);
        }
    }
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }
    pub fn progress(&self) -> RereadProgress {
        self.progress.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    pub fn set_progress(&self, p: RereadProgress) {
        *self.progress.lock().unwrap_or_else(|p| p.into_inner()) = p;
    }
}

/// 読み直しを始める前の確かめ。取り込み・再走査が走っていれば断る。読み直しが走っていても断る。
/// 取り込みの印（`AppState::importing`）の鍵を持ったまま読み直しの印を立てるので、取り込みの入口（`begin_import`）と行き違わない
pub fn begin_exclusive(importing: &Mutex<bool>, ctl: &Control) -> AppResult<()> {
    let importing = importing.lock().unwrap_or_else(|p| p.into_inner());
    if *importing {
        return Err(AppError::new(ErrorKind::LibraryImportBusy, "import or rescan is running"));
    }
    ctl.begin()
}

/// 取り込み・再走査の入口の確かめ（`commands::library::start_import_with`）。走らせてよければ取り込みの印を立てて Ok。
/// 断るときは、画面へ出す知らせの記号を返す（取り込みが走っている：`import_busy`、読み直しが走っている：`import_refused_legacy_reread`）
pub fn begin_import(importing: &Mutex<bool>, ctl: &Control) -> Result<(), &'static str> {
    let mut busy = importing.lock().unwrap_or_else(|p| p.into_inner());
    if *busy {
        return Err("import_busy");
    }
    if ctl.is_running() {
        return Err("import_refused_legacy_reread");
    }
    *busy = true;
    Ok(())
}

/// 編集画面で保存した値のうち、手で変えた項目（保存する前のライブラリの値と違う項目）のビット（`LegacyFields` の並び）。
/// 比べるのは前後の空白を落とした値（書き込みは前後の空白を落として書く）。
/// 曲名のタグが空でファイル名を曲名にしている曲で、曲名を空のまま保存したときも「変えた」に数える（印が多い側。読み直しは曲名に当てない）
pub fn edited_fields(before: &Track, after: &TagValues) -> i64 {
    let pairs = [
        (&before.title, &after.title),
        (&before.artist, &after.artist),
        (&before.album, &after.album),
        (&before.album_artist, &after.album_artist),
        (&before.genre, &after.genre),
        (&before.composer, &after.composer),
        (&before.comment, &after.comment),
    ];
    let mut bits = 0;
    for (i, (b, a)) in pairs.iter().enumerate() {
        if b.trim() != a.trim() {
            bits |= 1 << i;
        }
    }
    bits
}

/// 読み直す（今の決まり `legacy_text::repair` で）。結果と、直した曲の id を返す（エラーで止まっても、それまでの結果を返す）
pub fn run(db: &Db, cancel: &AtomicBool, progress: &mut dyn FnMut(&RereadProgress)) -> (RereadProgress, Vec<i64>) {
    run_with(db, cancel, &legacy_text::repair, CHUNK, progress)
}

type Row = (i64, String, String, LegacyFields, i64);

/// `run` の中身。決まり（`repair`）とまとまりの大きさを差し替えられる（テスト用：決まりを変えた版を見立てる・途中でやめる）
pub(crate) fn run_with(
    db: &Db,
    cancel: &AtomicBool,
    repair: &dyn Fn(&str, Option<&str>) -> Option<String>,
    chunk: usize,
    progress: &mut dyn FnMut(&RereadProgress),
) -> (RereadProgress, Vec<i64>) {
    let mut p = RereadProgress { running: true, ..Default::default() };
    let mut fixed_ids = vec![];
    if let Err(e) = run_inner(db, cancel, repair, chunk.max(1), progress, &mut p, &mut fixed_ids) {
        tracing::warn!("legacy tag reread stopped: {:?}", e.kind);
        p.error = Some(e.kind);
    }
    p.running = false;
    p.finished = true;
    if p.fixed > 0 {
        tracing::info!("legacy tag reread: repaired {} tracks", p.fixed);
    }
    progress(&p);
    (p, fixed_ids)
}

fn run_inner(
    db: &Db,
    cancel: &AtomicBool,
    repair: &dyn Fn(&str, Option<&str>) -> Option<String>,
    chunk: usize,
    progress: &mut dyn FnMut(&RereadProgress),
    p: &mut RereadProgress,
    fixed_ids: &mut Vec<i64>,
) -> AppResult<()> {
    p.total = tracks::count(db)? as usize;
    progress(p);
    let mut last_emit = Instant::now();
    let mut after: i64 = i64::MIN;
    loop {
        if cancel.load(Ordering::SeqCst) {
            p.cancelled = true;
            return Ok(());
        }
        // 読み取りの鍵は、このまとまりを読む間だけ持つ
        let rows: Vec<Row> = {
            let conn = db.reader();
            let mut stmt = conn.prepare_cached(
                "SELECT t.id, t.path, t.album_key, t.title, t.artist, t.album, t.album_artist, t.genre, t.composer, t.comment,
                        COALESCE(m.fields, 0)
                 FROM tracks t LEFT JOIN tag_manual_edits m ON m.track_id = t.id
                 WHERE t.id > ?1 ORDER BY t.id LIMIT ?2",
            )?;
            let it = stmt.query_map(rusqlite::params![after, chunk as i64], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    [r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?],
                    r.get(10)?,
                ))
            })?;
            it.collect::<rusqlite::Result<_>>()?
        };
        let Some(last) = rows.last() else {
            return Ok(());
        };
        after = last.0;
        let mut pending: Vec<(i64, LegacyFields, String, LegacyFields)> = vec![];
        for (id, path, old_key, v, manual) in rows.iter() {
            p.done += 1;
            if !tracks::has_latin1_high(v) {
                continue;
            }
            let r = tracks::repair_fields(path, v, *manual, repair);
            if r.skipped_manual {
                p.skipped_manual += 1;
            }
            if let Some(new) = r.fixed {
                pending.push((*id, v.clone(), old_key.clone(), new));
            }
        }
        if !pending.is_empty() {
            let mut conn = db.writer();
            let tx = conn.transaction()?;
            let mut written = vec![];
            for (id, old, old_key, new) in &pending {
                if tracks::write_repaired(&tx, *id, old, old_key, new)? {
                    written.push(*id);
                } else {
                    p.changed_during_run += 1;
                }
            }
            tx.commit()?;
            p.fixed += written.len();
            fixed_ids.extend(written);
        }
        if last_emit.elapsed() >= Duration::from_millis(100) {
            progress(p);
            last_emit = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tracks::TrackInfo;
    use crate::library::legacy_text::test_mojibake;

    // 名前はどれも架空のもの・一般語だけ
    /// 「さくら色の坂道」（Shift_JIS）
    const SJIS_SAKAMICHI: &[u8] = b"\x82\xb3\x82\xad\x82\xe7\x90\x46\x82\xcc\x8d\xe2\x93\xb9";
    /// 「アオイ」（Shift_JIS）
    const SJIS_AOI: &[u8] = b"\x83\x41\x83\x49\x83\x43";

    fn add(db: &Db, path: &str, title: &str, artist: &str, album: &str) -> i64 {
        let info = TrackInfo {
            path: path.into(),
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            format: "mp3".into(),
            ..Default::default()
        };
        tracks::upsert(&db.writer(), &info).unwrap().0
    }

    fn get(db: &Db, id: i64) -> Track {
        tracks::get(db, id).unwrap().unwrap()
    }

    fn quiet() -> impl FnMut(&RereadProgress) {
        |_| {}
    }

    /// 今の決まりで直る値（3.1.2 より前の決まりで取り込んだまま、DB に化けたまま残っている形を見立てる）は直る。
    /// アルバムの鍵・外部画像の設定・全文検索もついてくる。曲ファイルには触らない（無いパスの曲でも直る）
    #[test]
    fn repairs_values_left_in_the_db() {
        let db = Db::open_memory("reread_repairs").unwrap();
        let title = test_mojibake(SJIS_SAKAMICHI);
        let artist = test_mojibake(SJIS_AOI);
        let a = add(&db, r"C:\plp-no-such-dir\01 a.mp3", &title, &artist, &artist);
        let b = add(&db, r"C:\plp-no-such-dir\02 b.mp3", "Plain", "Someone", "Album");
        let old_key = get(&db, a).album_key;
        tracks::set_album_override(&db, &old_key, Some("x.png")).unwrap();
        let (p, ids) = run(&db, &AtomicBool::new(false), &mut quiet());
        assert_eq!((p.total, p.done, p.fixed, p.changed_during_run, p.skipped_manual), (2, 2, 1, 0, 0), "{p:?}");
        assert!(p.finished && !p.running && !p.cancelled && p.error.is_none());
        assert_eq!(ids, vec![a]);
        let t = get(&db, a);
        assert_eq!((t.title.as_str(), t.artist.as_str(), t.album.as_str()), ("さくら色の坂道", "アオイ", "アオイ"));
        assert_eq!(t.album_key, tracks::album_key("", "アオイ", "アオイ"));
        assert_eq!(tracks::album_override(&db, &t.album_key).unwrap().as_deref(), Some("x.png"));
        assert_eq!(get(&db, b).title, "Plain");
        let hits: i64 = db
            .reader()
            .query_row("SELECT count(*) FROM tracks_fts WHERE tracks_fts MATCH '\"色の坂\"'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hits, 1);
        // 2回目は直すものが無い
        let (p2, ids2) = run(&db, &AtomicBool::new(false), &mut quiet());
        assert_eq!((p2.fixed, ids2.len()), (0, 0));
    }

    /// 直してはいけない名前（ロシア語・ギリシャ語・東欧・韓国語・西欧の引用符とアポストロフィ。legacy_text のテストと同じもの）と、
    /// ファイル名と同じ曲名は、化けに変えない。EUC-JP などはファイル名と同じときだけ直る（取り込みと同じ）
    #[test]
    fn names_that_must_not_change_are_kept() {
        let db = Db::open_memory("reread_keeps").unwrap();
        let keep: Vec<(&[u8], &str)> = vec![
            (b"\xcd\xe5\xe1\xee", "Небо"),
            (b"\x93\xcd\xe5\xe1\xee\x94", "“Небо”"),
            (b"\xc1\xe8\xde\xed\xe1", "Αθήνα"),
            (b"\x8akoda \xc8aj", "Škoda Čaj"),
            (b"pie\x9c\xf1", "pieśń"),
            (b"\xbb\xe7\xb6\xfb", "사랑"),
            (b"\x93Hello\x94", "“Hello”"),
            (b"Don\x92t Stop", "Don’t Stop"),
            (b"L\x92\xe9t\xe9", "L’été"),
            (b"\x97\xc9lan", "—Élan"),
            (b"80\x92s", "80’s"),
            (b"\x93U\x8e\x94", "“UŽ”"),
            (b"D\xe9mon \xc9p\xe9e", "Démon Épée"),
        ];
        let mut ids = vec![];
        for (i, (bytes, _)) in keep.iter().enumerate() {
            let m = test_mojibake(bytes);
            ids.push(add(&db, &format!(r"C:\plp-no-such-dir\{i:02} x.mp3"), &m, &m, &m));
        }
        add(&db, r"C:\plp-no-such-dir\Raison.mp3", "Raison d'être", "A × B", "Grüne Möwe");
        // 曲名がファイル名と同じ（タグに曲名が無かった曲）は、曲名には当てない
        let stem = test_mojibake(SJIS_AOI);
        let s = add(&db, &format!(r"C:\plp-no-such-dir\{stem}.mp3"), &stem, "", "");
        // EUC-JP の「さくら」：ファイル名が同じ曲だけ直る
        let euc = test_mojibake(b"\xa4\xb5\xa4\xaf\xa4\xe9");
        let e1 = add(&db, r"C:\plp-no-such-dir\01 さくら.mp3", &euc, "", "");
        let e2 = add(&db, r"C:\plp-no-such-dir\other.mp3", &euc, "", "");
        let (p, fixed) = run(&db, &AtomicBool::new(false), &mut quiet());
        assert_eq!(fixed, vec![e1], "{p:?}");
        for (id, (bytes, word)) in ids.iter().zip(&keep) {
            let t = get(&db, *id);
            let m = test_mojibake(bytes);
            assert_eq!((&t.title, &t.artist, &t.album), (&m, &m, &m), "{word}");
        }
        assert_eq!(get(&db, s).title, stem);
        assert_eq!(get(&db, e1).title, "さくら");
        assert_eq!(get(&db, e2).title, euc);
    }

    /// 編集画面で手で変えた項目（印の付いた項目）は、決まりを変えた版（ここでは何でも直す決まりを見立てる）でも書き換えない。
    /// 印の無い項目は直る。直さなかった曲は数える
    #[test]
    fn manually_edited_fields_are_not_overwritten() {
        let db = Db::open_memory("reread_manual").unwrap();
        // 「×」を含む名前は、今の決まりでは直らない。決まりを変えた版で「直る」ことになったと見立てる
        let id = add(&db, r"C:\plp-no-such-dir\a.mp3", "Ñandú Brûlé Ça", "Ñandú Brûlé Ça", "Ñandú Brûlé Ça");
        let other = add(&db, r"C:\plp-no-such-dir\b.mp3", "Ñandú Brûlé Ça", "Plain", "Plain");
        // 編集画面で曲名とアルバムを変えて保存した（印は edited_fields で作る）
        let before = get(&db, id);
        let after = TagValues {
            title: "Ñandú Brûlé Ça".into(),
            artist: "Ñandú Brûlé Ça".into(),
            album: "  Ñandú Brûlé Ça  ".into(),
            ..Default::default()
        };
        // 同じ値（前後の空白だけ違う）は変えていない
        assert_eq!(edited_fields(&before, &after), 0);
        let after = TagValues { title: "Ñandú Brûlé Ça!".into(), album: "Ñandú".into(), artist: before.artist.clone(), ..Default::default() };
        let bits = edited_fields(&before, &after);
        assert_eq!(bits, 1 | 4);
        // DB の値はそのままにして、印だけを付ける（編集画面の保存が付ける印と同じ）
        tracks::mark_manual_edits(&db.writer(), id, bits).unwrap();
        let future = |v: &str, _: Option<&str>| -> Option<String> { v.contains('Ñ').then(|| "直した".to_string()) };
        let (p, fixed) = run_with(&db, &AtomicBool::new(false), &future, CHUNK, &mut quiet());
        let t = get(&db, id);
        assert_eq!(t.title, "Ñandú Brûlé Ça", "印の付いた曲名は書き換えない");
        assert_eq!(t.album, "Ñandú Brûlé Ça", "印の付いたアルバムは書き換えない");
        assert_eq!(t.artist, "直した", "印の無い項目は直る");
        assert_eq!(get(&db, other).title, "直した");
        assert_eq!(fixed, vec![id, other]);
        assert_eq!(p.skipped_manual, 1, "{p:?}");
        // 全部の項目に印があれば、その曲は1つも変えない
        let all = add(&db, r"C:\plp-no-such-dir\c.mp3", "Ñ1 Ñ2 Ñ3", "Ñ1 Ñ2 Ñ3", "Ñ1 Ñ2 Ñ3");
        tracks::mark_manual_edits(&db.writer(), all, 0x7f).unwrap();
        let (p, fixed) = run_with(&db, &AtomicBool::new(false), &future, CHUNK, &mut quiet());
        assert!(!fixed.contains(&all), "{p:?}");
        assert_eq!(get(&db, all).title, "Ñ1 Ñ2 Ñ3");
    }

    /// 3.4.0 までに編集画面で保存した値（印が無い）は、DB からは見分けられない。
    /// 保存のあとの読み直し（`tags_read::read_track`）で今の決まりを通っているので、今の決まりで読み直しても変わらない
    #[test]
    fn values_saved_in_the_editor_before_3_5_0_are_not_changed() {
        let db = Db::open_memory("reread_saved_before").unwrap();
        // 手で入れた値（日本語・西欧の名前・ロシア語）。書き込みのあとの読み直しと同じく、legacy_text::repair を通した値が DB に入る
        let typed = ["さくら色の坂道", "Raison d'être", "Ñandú Brûlé Ça", "Небо", "“Hello”", "Démon Épée", "Schöne Müde Grüße"];
        let mut ids = vec![];
        for (i, v) in typed.iter().enumerate() {
            let mut tv = TagValues { title: v.to_string(), artist: v.to_string(), album: v.to_string(), ..Default::default() };
            legacy_text::repair_values(&mut tv, std::path::Path::new(&format!("{i}.mp3")));
            ids.push(add(&db, &format!(r"C:\plp-no-such-dir\{i}.mp3"), &tv.title, &tv.artist, &tv.album));
        }
        let before: Vec<Track> = ids.iter().map(|&id| get(&db, id)).collect();
        let (p, fixed) = run(&db, &AtomicBool::new(false), &mut quiet());
        assert!(fixed.is_empty(), "{p:?}");
        for (b, id) in before.iter().zip(&ids) {
            let a = get(&db, *id);
            assert_eq!((&a.title, &a.artist, &a.album), (&b.title, &b.artist, &b.album));
        }
    }

    /// 「やめる」：次のまとまりの前で止まる。それまでに直した曲はそのまま、残りは触らない
    #[test]
    fn cancel_stops_before_the_next_chunk() {
        let db = Db::open_memory("reread_cancel").unwrap();
        let m = test_mojibake(SJIS_AOI);
        let ids: Vec<i64> = (0..6).map(|i| add(&db, &format!(r"C:\plp-no-such-dir\{i}.mp3"), &m, "", "")).collect();
        let cancel = AtomicBool::new(false);
        // まとまり 2 曲ずつ。最初のまとまりに決まりを当てている途中で「やめる」を押したと見立てる（決まりを当てた回数で数える）
        let calls = std::cell::Cell::new(0);
        let repair = |v: &str, s: Option<&str>| {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                cancel.store(true, Ordering::SeqCst);
            }
            legacy_text::repair(v, s)
        };
        let (p, fixed) = run_with(&db, &cancel, &repair, 2, &mut quiet());
        assert!(p.cancelled && p.finished && !p.running, "{p:?}");
        assert_eq!((p.total, p.done, p.fixed), (6, 2, 2), "{p:?}");
        assert_eq!(fixed, ids[..2].to_vec());
        for id in &ids[2..] {
            assert_eq!(get(&db, *id).title, m, "やめたあとの曲は触らない");
        }
        // やめる印を戻して走らせ直すと、残りが直る
        cancel.store(false, Ordering::SeqCst);
        let (p2, fixed2) = run(&db, &cancel, &mut quiet());
        assert_eq!((p2.fixed, fixed2), (4, ids[2..].to_vec()));
    }

    /// 読んでから書くまでの間に、編集画面の保存で値が変わった曲は書かない（手で入れた値を上書きしない）。
    /// ごみ箱へ移す・ライブラリから外すで消えた曲は、戻さない
    #[test]
    fn rows_changed_or_removed_during_the_run_are_left_alone() {
        let db = Db::open_memory("reread_race").unwrap();
        let m = test_mojibake(SJIS_SAKAMICHI);
        let edited = add(&db, r"C:\plp-no-such-dir\1.mp3", &m, "", "");
        let trashed = add(&db, r"C:\plp-no-such-dir\2.mp3", &m, "", "");
        let plain = add(&db, r"C:\plp-no-such-dir\3.mp3", &m, "", "");
        // 決まりを当てている間（読んだあと・書く前）に、ほかの操作が入ったと見立てる
        let once = std::cell::Cell::new(false);
        let repair = |v: &str, s: Option<&str>| {
            if !once.replace(true) {
                db.writer().execute("UPDATE tracks SET title = '手で入れた名前' WHERE id = ?1", [edited]).unwrap();
                tracks::remove(&db, &[trashed]).unwrap();
            }
            legacy_text::repair(v, s)
        };
        let (p, fixed) = run_with(&db, &AtomicBool::new(false), &repair, CHUNK, &mut quiet());
        assert_eq!(fixed, vec![plain], "{p:?}");
        assert_eq!(p.changed_during_run, 2, "{p:?}");
        assert_eq!(get(&db, edited).title, "手で入れた名前");
        assert!(tracks::get(&db, trashed).unwrap().is_none(), "消えた曲は戻さない");
        assert_eq!(get(&db, plain).title, "さくら色の坂道");
    }

    /// 同時に走らせない：読み直しが走っていれば、2つ目の読み直しと取り込み・再走査を断る。取り込みが走っていれば、読み直しを断る。
    /// 終われば（落ちても）また走らせられる。「やめる」は走っていないときは印を立てない
    #[test]
    fn refuses_to_run_together_with_imports_or_itself() {
        let importing = Mutex::new(false);
        let ctl = Control::default();
        ctl.cancel();
        assert!(!ctl.cancel_flag().load(Ordering::SeqCst), "走っていないときの「やめる」は効かない");
        begin_exclusive(&importing, &ctl).unwrap();
        assert!(ctl.is_running());
        assert_eq!(begin_exclusive(&importing, &ctl).unwrap_err().kind, ErrorKind::LegacyRereadBusy);
        assert_eq!(begin_import(&importing, &ctl), Err("import_refused_legacy_reread"));
        assert!(!*importing.lock().unwrap(), "断った取り込みは印を立てない");
        {
            let _g = ctl.end_guard();
        }
        assert!(!ctl.is_running());
        // 取り込みが走っている間は、読み直しを断る（取り込みの印はそのまま）
        assert_eq!(begin_import(&importing, &ctl), Ok(()));
        assert_eq!(begin_exclusive(&importing, &ctl).unwrap_err().kind, ErrorKind::LibraryImportBusy);
        assert!(!ctl.is_running());
        assert_eq!(begin_import(&importing, &ctl), Err("import_busy"));
        *importing.lock().unwrap() = false;
        // 落ちても印は戻る
        begin_exclusive(&importing, &ctl).unwrap();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = ctl.end_guard();
            panic!("test panic in the reread thread");
        }));
        assert!(r.is_err());
        assert!(!ctl.is_running());
        begin_exclusive(&importing, &ctl).unwrap();
        ctl.cancel();
        assert!(ctl.cancel_flag().load(Ordering::SeqCst));
        drop(ctl.end_guard());
        // 次に始めるときは「やめる」の印を戻す
        begin_exclusive(&importing, &ctl).unwrap();
        assert!(!ctl.cancel_flag().load(Ordering::SeqCst));
    }

    /// DB の読み書きに失敗したら、そこで止めて種別を返す（それまでに直した曲はそのまま）
    #[test]
    fn database_errors_stop_with_a_kind() {
        let db = Db::open_memory("reread_db_error").unwrap();
        let m = test_mojibake(SJIS_AOI);
        let a = add(&db, r"C:\plp-no-such-dir\1.mp3", &m, "", "");
        add(&db, r"C:\plp-no-such-dir\2.mp3", &m, "", "");
        // 2曲目の書き込みで失敗する仕掛け
        db.writer()
            .execute_batch(&format!(
                "CREATE TRIGGER plp_test_fail BEFORE UPDATE OF title ON tracks WHEN old.id > {a}
                 BEGIN SELECT RAISE(ABORT, 'test failure'); END;"
            ))
            .unwrap();
        let (p, fixed) = run_with(&db, &AtomicBool::new(false), &legacy_text::repair, 1, &mut quiet());
        assert_eq!(p.error, Some(ErrorKind::Database), "{p:?}");
        assert!(p.finished && !p.running);
        assert_eq!(fixed, vec![a]);
        assert_eq!(get(&db, a).title, "アオイ");
    }
}
