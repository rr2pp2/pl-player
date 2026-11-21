//! 歌詞をネットから取る（v3.1.0）：一括と1曲。設定の確認・送る曲の選び方・保存・覚えておく結果。
//!
//! - **設定がオフなら、`begin` で断る**（commands の入口で最初に呼ぶ）。走っている途中でオフにされたら、
//!   次の要求を送る前に `Client` の gate が止める
//! - 送る決まり・応答の読み方は `lrclib`（元は非公開の取得スクリプト）
//! - 取れた歌詞は external として保存する（受け渡しフォルダの取り込みと同じ：改行を LF にそろえ、1MiB まで、同じ読み方）。
//!   **manual には書かない。手動の歌詞がある曲は送りもしない**
//! - 取り直しで取れなかったときは、今の外部の歌詞を消さずに残す
//! - 見つからない・instrumental・大きすぎ は `lyrics_fetch_misses` に覚え、「歌詞の無い曲だけ」では聞き直さない
//! - ログ・画面へ返すものは件数・曲 ID・状態だけ（歌詞の本文と LRCLIB 側の曲名などは出さない）

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::lrc;
use super::lrclib::{self, Client, Env, GetError, Outcome, Status, Stop, Transport};
use crate::db::lyrics::{self as store, FetchCandidate, StoredKind};
use crate::db::Db;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::library::tags_read;
use crate::settings::store::AppSettings;

/// 一括で取るときの対象
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// 歌詞の無い曲だけ（手動・外部・曲のタグのどれも無い曲。前に見つからなかった曲は聞き直さない）
    Missing,
    /// 外部の歌詞を取り直す（外部の歌詞がある曲と、前に見つからなかった曲も聞き直す）
    Refetch,
}

/// 進み具合（画面へ送る。件数だけ）
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchProgress {
    pub running: bool,
    pub finished: bool,
    /// 送る曲を選んでいる途中。この間は prepare_done / prepare_total で進み具合を出す
    pub preparing: bool,
    /// 選ぶ対象の曲数（手動の歌詞が無い曲）
    pub prepare_total: usize,
    /// そのうち見終わった曲数
    pub prepare_done: usize,
    pub mode: Option<Mode>,
    /// 今回送る予定の曲数（何曲中の「何曲」）
    pub total: usize,
    /// 処理した曲数
    pub done: usize,
    pub synced: usize,
    pub plain: usize,
    pub instrumental: usize,
    pub not_found: usize,
    pub too_large: usize,
    pub errors: usize,
    /// 取り直しで取れなかったので、今の外部の歌詞を残した曲（上の件数には入れない）
    pub kept: usize,
    /// 曲のタグに歌詞があるので送らなかった曲
    pub has_tag: usize,
    /// 曲名かアーティストが空・長すぎて送らなかった曲（予定の曲数に入れない）
    pub no_metadata: usize,
    /// 前に見つからなかったので聞き直さなかった曲（予定の曲数に入れない）
    pub skipped_known: usize,
    /// 一括の途中でライブラリから外された・手動の歌詞が付いたので送らなかった曲（エラーに数えない）
    pub changed_during_run: usize,
    /// 最後にエラーになった曲の記号（timeout・http_404・bad_response など。打ち切りの文言を原因ごとに分けるため）
    pub last_error: Option<String>,
    /// 送った要求の数
    pub requests: u64,
    /// 打ち切りの理由（cancelled / disabled / rate_limited_long_wait / rate_limited_repeatedly / too_many_errors / database /
    /// network_init_failed / internal_error）
    pub aborted: Option<String>,
}

/// 終わったときに画面へ送るもの
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchDone {
    #[serde(flatten)]
    pub progress: FetchProgress,
    /// 外部の歌詞が変わった曲（表示中の曲なら画面が読み直す）
    pub track_ids: Vec<i64>,
}

/// 1曲だけ取ったときの結果
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OneResult {
    pub track_id: i64,
    /// synced / plain / instrumental / not_found / too_large / kept / no_metadata / manual / error / stopped
    pub status: String,
    /// error・stopped のときの短い記号（応答の中身は入れない）
    pub error: Option<String>,
}

// ---------------------------------------------------------------- 入口の確認と、同時に1つだけ

/// 一括と1曲で共有する。同時に走らせない（要求は常に1つずつ）
#[derive(Default)]
pub struct Control {
    busy: Mutex<bool>,
    cancel: Arc<AtomicBool>,
    /// 最後に応答を受けた時刻（RealEnv の時計）。操作をまたいでも要求の間を空ける
    last_request: Mutex<Option<f64>>,
    progress: Mutex<FetchProgress>,
}

pub fn disabled_error() -> AppError {
    AppError::new(ErrorKind::LyricsOnlineDisabled, "lyrics online fetch is turned off")
}

/// **入口の確認。**設定がオフなら断る（通信の処理にたどり着かない）。オンでも、走っているものがあれば断る
pub fn begin(settings: &AppSettings, control: &Control) -> AppResult<()> {
    if !settings.lyrics_online {
        return Err(disabled_error());
    }
    let mut busy = control.busy.lock().unwrap_or_else(|p| p.into_inner());
    if *busy {
        return Err(AppError::new(ErrorKind::LyricsOnlineBusy, "lyrics online fetch is already running"));
    }
    *busy = true;
    control.cancel.store(false, Ordering::SeqCst);
    Ok(())
}

/// begin と対で使う。落ちても（drop で）必ず空ける
pub struct EndGuard<'a>(&'a Control);

impl Drop for EndGuard<'_> {
    fn drop(&mut self) {
        *self.0.busy.lock().unwrap_or_else(|p| p.into_inner()) = false;
    }
}

impl Control {
    pub fn end_guard(&self) -> EndGuard<'_> {
        EndGuard(self)
    }
    pub fn cancel(&self) {
        if *self.busy.lock().unwrap_or_else(|p| p.into_inner()) {
            self.cancel.store(true, Ordering::SeqCst);
        }
    }
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }
    pub fn last_request(&self) -> Option<f64> {
        *self.last_request.lock().unwrap_or_else(|p| p.into_inner())
    }
    pub fn set_last_request(&self, t: Option<f64>) {
        *self.last_request.lock().unwrap_or_else(|p| p.into_inner()) = t;
    }
    pub fn progress(&self) -> FetchProgress {
        self.progress.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    pub fn set_progress(&self, p: FetchProgress) {
        *self.progress.lock().unwrap_or_else(|p| p.into_inner()) = p;
    }
}

/// 一括のスレッドに持たせる。`finish` で終わりを知らせる。
/// **途中で落ちても（panic）drop で必ず**：処理中の印を外し、進み具合を `running:false`・`aborted:internal_error` にして、
/// 終わりの知らせを出す（設定画面のボタンが押せないまま残らないように）
pub struct DoneGuard<'a, F: FnMut(&FetchDone)> {
    ctl: &'a Control,
    emit: F,
    sent: bool,
}

impl<'a, F: FnMut(&FetchDone)> DoneGuard<'a, F> {
    /// begin が通ったあとに作る
    pub fn new(ctl: &'a Control, emit: F) -> Self {
        Self { ctl, emit, sent: false }
    }

    pub fn finish(mut self, done: FetchDone) {
        self.send(done);
    }

    fn send(&mut self, done: FetchDone) {
        self.sent = true;
        self.ctl.set_progress(done.progress.clone());
        // 先に空けてから知らせる（知らせを受けた画面がすぐ次を始められるように）
        *self.ctl.busy.lock().unwrap_or_else(|p| p.into_inner()) = false;
        (self.emit)(&done);
    }
}

impl<F: FnMut(&FetchDone)> Drop for DoneGuard<'_, F> {
    fn drop(&mut self) {
        if self.sent {
            return;
        }
        let mut p = self.ctl.progress();
        p.running = false;
        p.finished = true;
        if p.aborted.is_none() {
            p.aborted = Some("internal_error".into());
        }
        tracing::error!("lyrics fetch: stopped unexpectedly (done={} total={})", p.done, p.total);
        self.send(FetchDone { progress: p, track_ids: vec![] });
    }
}

/// 送る直前の確認（Client の gate）。やめるボタン → 設定 の順に見る
pub fn gate(cancel: Arc<AtomicBool>, enabled: impl Fn() -> bool) -> impl Fn() -> Option<Stop> {
    move || {
        if cancel.load(Ordering::SeqCst) {
            Some(Stop::Cancelled)
        } else if !enabled() {
            Some(Stop::Disabled)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------- 本物の待ち・時計

fn clock_base() -> Instant {
    static BASE: OnceLock<Instant> = OnceLock::new();
    *BASE.get_or_init(Instant::now)
}

/// 本物の待ち（50ms ごとにやめるボタンを見る）・乱数・時計
pub struct RealEnv {
    cancel: Arc<AtomicBool>,
    rng: u64,
}

impl RealEnv {
    pub fn new(cancel: Arc<AtomicBool>) -> Self {
        use std::hash::{BuildHasher, Hasher};
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(clock_base().elapsed().as_nanos());
        Self { cancel, rng: h.finish() | 1 }
    }
}

impl Env for RealEnv {
    fn sleep(&mut self, secs: f64) -> Result<(), Stop> {
        let secs = if secs.is_finite() { secs.clamp(0.0, 3600.0) } else { 0.0 };
        let end = Instant::now() + Duration::from_secs_f64(secs);
        loop {
            if self.cancel.load(Ordering::SeqCst) {
                return Err(Stop::Cancelled);
            }
            let now = Instant::now();
            if now >= end {
                return Ok(());
            }
            std::thread::sleep((end - now).min(Duration::from_millis(50)));
        }
    }
    fn rand(&mut self) -> f64 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        let x = self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
    fn now(&mut self) -> f64 {
        clock_base().elapsed().as_secs_f64()
    }
    fn epoch(&mut self) -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }
}

/// 曲名がタグに無く、ファイル名（拡張子なし）から作った値か。
///
/// 取り込みは、タグに曲名が無いとファイル名を曲名にする（`library::tags_read::read_track`）。それを LRCLIB へ送ると、
/// 「送らないもの：ファイルの場所」に反してファイル名が出ていく。だから曲名がファイル名と同じ曲は、
/// 曲のタグを読み直し、**タグに曲名があるときだけ**送る。読めないときは「タグに無い」とみなす（送らない側に倒す）。
/// ファイル名と同じ曲名をタグに書いてある曲は、タグの値なので送る
///
/// `has_tag_title` は曲のタグに曲名があるか（本物は `tag_has_title`。テストでは差し替える）。
/// **読み取りが panic しても止まらない**：panic は「タグに無い」＝送らない
pub fn title_is_file_name(path: &str, title: &str, has_tag_title: &dyn Fn(&str) -> bool) -> bool {
    let Some(stem) = Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()) else {
        return false;
    };
    if stem.trim() != title.trim() {
        return false;
    }
    !no_panic(|| has_tag_title(path)).unwrap_or(false)
}

/// 曲のタグに曲名があるか（`tags_read::read_values` は lofty の panic を Err にする）
pub fn tag_has_title(path: &str) -> bool {
    matches!(tags_read::read_values(Path::new(path)), Ok(v) if !v.title.trim().is_empty())
}

/// タグを読む関数の panic を None にする（`tags_read` の中でも包んでいるが、差し替えた関数も含めて一括を止めないため）
fn no_panic<T>(f: impl FnOnce() -> T) -> Option<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).ok()
}

/// 送る問い合わせを組む。曲名かアーティストが空・長すぎ・曲名がファイル名から作った値なら None（送らない）
fn query_for(c: &FetchCandidate, has_tag_title: &dyn Fn(&str) -> bool) -> Option<lrclib::Query> {
    let q = lrclib::build_query(&c.title, &c.artist, &c.album, c.duration_ms)?;
    if title_is_file_name(&c.path, &c.title, has_tag_title) {
        return None;
    }
    Some(q)
}

/// 曲ファイルのタグに、表示される歌詞があるか（歌詞画面と同じ：空でなく 1MiB 以下）
pub fn tag_has_lyrics(path: &str) -> bool {
    match tags_read::read_lyrics(Path::new(path)) {
        Ok(Some(t)) => lrc::normalize(&t).len() <= lrc::MAX_TEXT_BYTES,
        _ => false,
    }
}

// ---------------------------------------------------------------- 保存

enum Applied {
    /// 外部の歌詞として保存した
    Saved,
    /// 取れなかったが、今の外部の歌詞を残した
    Kept,
    /// 見つからない・instrumental・大きすぎ を覚えた
    Remembered,
}

/// 結果を DB に反映する。manual には触らない
fn apply(db: &Db, c: &FetchCandidate, sig: &str, oc: &Outcome) -> AppResult<Applied> {
    if oc.status.is_ok() {
        if let Some(text) = &oc.text {
            // 取り込みと同じ扱い：改行を LF にそろえ、1MiB まで、中身に有効な時刻タグがあれば時刻つき
            let text = lrc::normalize(text);
            let parsed = (text.len() <= lrc::MAX_TEXT_BYTES).then(|| lrc::parse(&text));
            if let Some(p) = parsed.filter(|p| p.has_content()) {
                store::upsert(&db.writer(), c.id, StoredKind::External, &text, p.format.as_str())?;
                store::clear_miss(db, c.id)?;
                return Ok(Applied::Saved);
            }
        }
    }
    // 取れなかった。取れていた歌詞は消さない
    if store::exists(db, c.id, StoredKind::External)? {
        return Ok(Applied::Kept);
    }
    let result = if oc.status.is_ok() { Status::NotFound } else { oc.status };
    store::record_miss(db, c.id, result.code(), sig)?;
    Ok(Applied::Remembered)
}

fn count(p: &mut FetchProgress, st: Status) {
    match st {
        Status::Synced => p.synced += 1,
        Status::Plain => p.plain += 1,
        Status::Instrumental => p.instrumental += 1,
        Status::NotFound => p.not_found += 1,
        Status::TooLarge => p.too_large += 1,
    }
}

fn local_sec(c: &FetchCandidate) -> Option<f64> {
    (c.duration_ms > 0).then(|| c.duration_ms as f64 / 1000.0)
}

// ---------------------------------------------------------------- 一括

/// 一括で取る。`has_tag_lyrics` は曲のタグに歌詞があるか、`has_tag_title` は曲のタグに曲名があるか（テストでは差し替える）。
/// 戻り値は (最後の進み具合, 外部の歌詞が変わった曲)
pub fn run_batch<T: Transport, E: Env>(
    db: &Db,
    mode: Mode,
    client: &mut Client<'_, T, E>,
    has_tag_lyrics: &dyn Fn(&str) -> bool,
    has_tag_title: &dyn Fn(&str) -> bool,
    on_progress: &mut dyn FnMut(&FetchProgress),
) -> (FetchProgress, Vec<i64>) {
    let mut p = FetchProgress { running: true, mode: Some(mode), ..Default::default() };
    let mut changed: Vec<i64> = Vec::new();
    let finish = |mut p: FetchProgress, changed: Vec<i64>, requests: u64| {
        p.running = false;
        p.finished = true;
        p.requests = requests;
        tracing::info!(
            "lyrics fetch: mode={:?} total={} done={} synced={} plain={} instrumental={} not_found={} too_large={} errors={} kept={} has_tag={} no_metadata={} skipped_known={} requests={} aborted={:?}",
            p.mode, p.total, p.done, p.synced, p.plain, p.instrumental, p.not_found, p.too_large, p.errors, p.kept, p.has_tag, p.no_metadata, p.skipped_known, p.requests, p.aborted
        );
        (p, changed)
    };
    if let Some(stop) = client.check_gate() {
        p.aborted = Some(stop.code().into());
        return finish(p, changed, client.requests_sent);
    }
    let candidates = match store::fetch_candidates(db) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("lyrics fetch: candidates failed: {}", e.detail);
            p.aborted = Some("database".into());
            return finish(p, changed, client.requests_sent);
        }
    };
    // 送る曲を決める（手動の歌詞がある曲は候補に入っていない）。
    // 曲ファイルを読む（曲名がファイル名と同じ曲のタグを確かめる）のは最後にする。覚えた結果で聞き直さない曲は読まない。
    // 選ぶ間も、やめるボタン・設定のオフを見て、進み具合（準備中 ◯曲中◯曲）を出す
    p.preparing = true;
    p.prepare_total = candidates.len();
    on_progress(&p);
    let mut last_emit = Instant::now();
    let mut work: Vec<(FetchCandidate, lrclib::Query, String)> = Vec::new();
    for (i, c) in candidates.into_iter().enumerate() {
        p.prepare_done = i;
        if i % 64 == 0 {
            if let Some(stop) = client.check_gate() {
                p.aborted = Some(stop.code().into());
                p.preparing = false;
                return finish(p, changed, client.requests_sent);
            }
        }
        if last_emit.elapsed() >= Duration::from_millis(250) {
            on_progress(&p);
            last_emit = Instant::now();
        }
        if mode == Mode::Missing && c.has_external {
            continue;
        }
        let Some(q) = lrclib::build_query(&c.title, &c.artist, &c.album, c.duration_ms) else {
            p.no_metadata += 1;
            continue;
        };
        let sig = lrclib::query_sig(&q);
        if mode == Mode::Missing && c.miss_sig.as_deref() == Some(sig.as_str()) {
            p.skipped_known += 1;
            continue;
        }
        // ここで初めて曲ファイルを読むことがある。読む前にも止める理由を見る
        if Path::new(&c.path).file_stem().is_some_and(|s| s.to_string_lossy().trim() == c.title.trim()) {
            if let Some(stop) = client.check_gate() {
                p.aborted = Some(stop.code().into());
                p.preparing = false;
                return finish(p, changed, client.requests_sent);
            }
        }
        if title_is_file_name(&c.path, &c.title, has_tag_title) {
            p.no_metadata += 1;
            continue;
        }
        work.push((c, q, sig));
    }
    p.preparing = false;
    p.prepare_done = p.prepare_total;
    p.total = work.len();
    on_progress(&p);

    let mut consecutive_errors: u32 = 0;
    for (c, q, sig) in work {
        if let Some(stop) = client.check_gate() {
            p.aborted = Some(stop.code().into());
            break;
        }
        // 送る直前に確かめ直す：一括の途中で、ライブラリから外された曲・手動の歌詞が付いた曲は送らない。
        // 外部の歌詞の有無も今の値を使う（取れていた歌詞を残す判断に使う）
        let c = match store::fetch_candidate(db, c.id) {
            Ok(Some(now)) if !now.has_manual => FetchCandidate { has_external: now.has_external, ..c },
            Ok(_) => {
                p.changed_during_run += 1;
                p.done += 1;
                p.requests = client.requests_sent;
                on_progress(&p);
                continue;
            }
            Err(e) => {
                tracing::warn!("lyrics fetch: recheck failed (track {}): {}", c.id, e.detail);
                p.aborted = Some("database".into());
                break;
            }
        };
        // 曲のタグに歌詞がある曲は送らない（外部の歌詞を保存すると、タグの歌詞より優先されてしまうため）
        // 読み取りが panic したら「タグに歌詞は無い」（歌詞画面も同じく歌詞なしと出す）
        if !c.has_external && no_panic(|| has_tag_lyrics(&c.path)).unwrap_or(false) {
            p.has_tag += 1;
            p.done += 1;
            p.requests = client.requests_sent;
            on_progress(&p);
            continue;
        }
        let status: Result<Status, String> = match lrclib::lookup(client, &q, local_sec(&c)) {
            Ok(oc) => match apply(db, &c, &sig, &oc) {
                Ok(Applied::Saved) if [oc.title_match, oc.artist_match].contains(&Some(lrclib::NameMatch::Partial)) => {
                    // スクリプトと同じく、名前が部分一致の候補から選んだ曲は曲 ID だけを残す（確かめたいときに歌詞画面で見る）
                    tracing::info!("lyrics fetch: track {} chosen from a partial name match", c.id);
                    changed.push(c.id);
                    Ok(oc.status)
                }
                Ok(Applied::Saved) => {
                    changed.push(c.id);
                    Ok(oc.status)
                }
                Ok(Applied::Kept) => {
                    p.kept += 1;
                    Err(String::new())
                }
                Ok(Applied::Remembered) => Ok(if oc.status.is_ok() { Status::NotFound } else { oc.status }),
                Err(e) => {
                    tracing::warn!("lyrics fetch: save failed (track {}): {}", c.id, e.detail);
                    Err("write_failed".into())
                }
            },
            Err(GetError::Fetch(code)) => Err(code),
            Err(GetError::Abort(code)) => {
                p.aborted = Some(code.into());
                break;
            }
            Err(GetError::Stop(stop)) => {
                p.aborted = Some(stop.code().into());
                break;
            }
        };
        match status {
            Ok(st) => {
                count(&mut p, st);
                consecutive_errors = 0;
                tracing::debug!("lyrics fetch: track {} {}", c.id, st.code());
            }
            Err(code) if code.is_empty() => {
                consecutive_errors = 0;
                tracing::debug!("lyrics fetch: track {} kept", c.id);
            }
            Err(code) => {
                p.errors += 1;
                consecutive_errors += 1;
                tracing::info!("lyrics fetch: track {} error {code}", c.id);
                p.last_error = Some(code);
            }
        }
        p.done += 1;
        p.requests = client.requests_sent;
        on_progress(&p);
        if consecutive_errors >= lrclib::MAX_CONSECUTIVE_ERRORS {
            p.aborted = Some("too_many_errors".into());
            break;
        }
    }
    finish(p, changed, client.requests_sent)
}

// ---------------------------------------------------------------- 1曲

/// 1曲だけ取る（歌詞画面の「ネットから探す」）。押したときは、覚えた「見つからない」があっても聞き直す。
/// 手動の歌詞がある曲は送らない
pub fn fetch_one<T: Transport, E: Env>(db: &Db, track_id: i64, client: &mut Client<'_, T, E>) -> AppResult<OneResult> {
    let c = store::fetch_candidate(db, track_id)?.ok_or_else(|| AppError::new(ErrorKind::FileNotFound, format!("track {track_id}")))?;
    let res = |status: &str, error: Option<String>| OneResult { track_id, status: status.into(), error };
    if c.has_manual {
        return Ok(res("manual", None));
    }
    let Some(q) = query_for(&c, &tag_has_title) else {
        return Ok(res("no_metadata", None));
    };
    let sig = lrclib::query_sig(&q);
    let out = match lrclib::lookup(client, &q, local_sec(&c)) {
        Ok(oc) => match apply(db, &c, &sig, &oc)? {
            Applied::Saved => res(oc.status.code(), None),
            Applied::Kept => res("kept", None),
            Applied::Remembered => res(if oc.status.is_ok() { "not_found" } else { oc.status.code() }, None),
        },
        Err(GetError::Fetch(code)) => res("error", Some(code)),
        Err(GetError::Abort(code)) => res("error", Some(code.into())),
        Err(GetError::Stop(stop)) => res("stopped", Some(stop.code().into())),
    };
    tracing::info!("lyrics fetch one: track {track_id} {} {:?} requests={}", out.status, out.error, client.requests_sent);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tracks::{self, TrackInfo};
    use crate::lyrics::lrclib::fake::*;
    use std::cell::Cell;

    fn add(db: &Db, path: &str, title: &str, artist: &str) -> i64 {
        tracks::upsert(
            &db.writer(),
            &TrackInfo {
                path: path.into(),
                title: title.into(),
                artist: artist.into(),
                album: "Album".into(),
                duration_ms: 200_000,
                ..Default::default()
            },
        )
        .unwrap()
        .0
    }

    fn ext(db: &Db, id: i64) -> Option<String> {
        store::get(db, id, StoredKind::External).unwrap().map(|s| s.raw_text)
    }

    const SYNCED: &str = r#"{"duration":200,"syncedLyrics":"[00:01.00]one\n[00:02.00]two"}"#;
    const PLAIN: &str = r#"{"duration":200,"plainLyrics":"plain words"}"#;
    const INSTR: &str = r#"{"duration":200,"instrumental":true}"#;

    fn no_tag(_: &str) -> bool {
        false
    }

    /// 歌詞の無い曲だけ → 覚えて聞き直さない → 取り直し。manual は送らず書かず、取れていた歌詞は消さない
    #[test]
    fn batch_missing_then_refetch() {
        let db = Db::open_memory("online_batch").unwrap();
        let t1 = add(&db, "C:/o/1.mp3", "One", "Artist");
        let t2 = add(&db, "C:/o/2.mp3", "Two", "Artist");
        let t3 = add(&db, "C:/o/3.mp3", "Three", "Artist"); // 手動の歌詞あり
        let t4 = add(&db, "C:/o/4.mp3", "Four", ""); // アーティストが空
        let t5 = add(&db, "C:/o/5.mp3", "Five", "Artist"); // 外部の歌詞あり
        let t6 = add(&db, "C:/o/6.mp3", "Six", "Artist");
        store::upsert(&db.writer(), t3, StoredKind::Manual, "手動の歌詞", "plain").unwrap();
        store::upsert(&db.writer(), t5, StoredKind::External, "前の外部", "plain").unwrap();

        // 1回目（歌詞の無い曲だけ）：t1・t2・t6 を送る
        let mut t = FakeTransport::default();
        t.push(200, SYNCED); // t1 get
        t.push(404, "").push(200, "[]"); // t2 get → search
        t.push(200, INSTR); // t6 get
        let mut c = Client::new(t, FakeEnv::default(), || None);
        let mut seen = vec![];
        let (p, changed) = run_batch(&db, Mode::Missing, &mut c, &no_tag, &tag_has_title, &mut |p| {
            if !p.preparing {
                seen.push((p.done, p.total))
            }
        });
        assert_eq!(p.total, 3);
        assert_eq!(p.done, 3);
        assert_eq!((p.synced, p.plain, p.not_found, p.instrumental, p.no_metadata, p.errors), (1, 0, 1, 1, 1, 0));
        assert!(p.finished && !p.running && p.aborted.is_none());
        assert_eq!(p.requests, 4);
        assert_eq!(changed, vec![t1]);
        assert_eq!(seen, vec![(0, 3), (1, 3), (2, 3), (3, 3)]);
        assert_eq!(c.transport.sent.len(), 4);
        assert!(c.transport.sent.iter().all(|u| !u.contains("Three") && !u.contains("Four") && !u.contains("Five")));
        assert_eq!(ext(&db, t1).as_deref(), Some("[00:01.00]one\n[00:02.00]two\n"));
        assert_eq!(store::get(&db, t1, StoredKind::External).unwrap().unwrap().format, "timed");
        assert_eq!(store::miss(&db, t2).unwrap().unwrap().0, "not_found");
        assert_eq!(store::miss(&db, t6).unwrap().unwrap().0, "instrumental");
        assert_eq!(store::get(&db, t3, StoredKind::Manual).unwrap().unwrap().raw_text, "手動の歌詞");
        assert!(ext(&db, t3).is_none());
        assert!(ext(&db, t4).is_none());

        // 2回目（歌詞の無い曲だけ）：覚えた曲は聞き直さない。送る曲が無い
        let mut c = Client::new(PanicTransport, FakeEnv::default(), || None);
        let (p, _) = run_batch(&db, Mode::Missing, &mut c, &no_tag, &tag_has_title, &mut |_| {});
        assert_eq!((p.total, p.skipped_known, p.no_metadata, p.requests), (0, 2, 1, 0));

        // 曲名を直すと、覚えた結果は使わず聞き直す
        db.writer().execute("UPDATE tracks SET title = 'Two (fixed)' WHERE id = ?1", [t2]).unwrap();
        let mut t = FakeTransport::default();
        t.push(200, PLAIN);
        let mut c = Client::new(t, FakeEnv::default(), || None);
        let (p, changed) = run_batch(&db, Mode::Missing, &mut c, &no_tag, &tag_has_title, &mut |_| {});
        assert_eq!((p.total, p.plain, p.skipped_known), (1, 1, 1));
        assert_eq!(changed, vec![t2]);
        assert!(store::miss(&db, t2).unwrap().is_none(), "取れたら覚えた結果を消す");

        // 取り直し：外部の歌詞がある曲（t1・t2・t5）と、覚えた曲（t6）も聞き直す。手動の曲（t3）は送らない
        let mut t = FakeTransport::default();
        t.push(404, "").push(404, ""); // t1：見つからない → 今の歌詞を残す
        t.push(200, INSTR); // t2：instrumental → 今の歌詞を残す
        t.push(200, PLAIN); // t5：取れた → 置き換え
        t.push(200, SYNCED); // t6：取れた
        let mut c = Client::new(t, FakeEnv::default(), || None);
        let (p, changed) = run_batch(&db, Mode::Refetch, &mut c, &no_tag, &tag_has_title, &mut |_| {});
        assert_eq!(p.total, 4);
        assert_eq!((p.kept, p.plain, p.synced, p.not_found, p.instrumental), (2, 1, 1, 0, 0));
        assert_eq!(changed, vec![t5, t6]);
        assert_eq!(ext(&db, t1).as_deref(), Some("[00:01.00]one\n[00:02.00]two\n"), "取れていた歌詞を取り直しで消さない");
        assert_eq!(ext(&db, t2).as_deref(), Some("plain words\n"));
        assert!(store::miss(&db, t1).unwrap().is_none());
        assert_eq!(ext(&db, t5).as_deref(), Some("plain words\n"));
        assert!(store::miss(&db, t6).unwrap().is_none());
        assert_eq!(store::get(&db, t3, StoredKind::Manual).unwrap().unwrap().raw_text, "手動の歌詞");
    }

    #[test]
    fn tag_lyrics_are_not_overridden() {
        let db = Db::open_memory("online_tag").unwrap();
        let a = add(&db, "C:/o/tagged.mp3", "A Song", "Artist");
        let mut c = Client::new(PanicTransport, FakeEnv::default(), || None);
        let (p, _) = run_batch(&db, Mode::Refetch, &mut c, &|path| path.ends_with("tagged.mp3"), &tag_has_title, &mut |_| {});
        assert_eq!((p.total, p.done, p.has_tag, p.requests), (1, 1, 1, 0));
        assert!(ext(&db, a).is_none());
        assert!(store::miss(&db, a).unwrap().is_none());
    }

    /// オフ：通信の処理にたどり着かない（PanicTransport は呼ばれたら落ちる）。入口の begin でも断る
    #[test]
    fn disabled_never_reaches_transport() {
        let db = Db::open_memory("online_off").unwrap();
        let a = add(&db, "C:/o/off.mp3", "Song", "Artist");
        let mut c = Client::new(PanicTransport, FakeEnv::default(), || Some(Stop::Disabled));
        let (p, _) = run_batch(&db, Mode::Missing, &mut c, &no_tag, &tag_has_title, &mut |_| {});
        assert_eq!(p.aborted.as_deref(), Some("disabled"));
        assert_eq!((p.requests, p.done), (0, 0));
        let r = fetch_one(&db, a, &mut c).unwrap();
        assert_eq!((r.status.as_str(), r.error.as_deref()), ("stopped", Some("disabled")));

        let ctl = Control::default();
        let off = AppSettings::default();
        assert!(!off.lyrics_online, "既定はオフ");
        assert_eq!(begin(&off, &ctl).unwrap_err().kind, ErrorKind::LyricsOnlineDisabled);
        assert!(!*ctl.busy.lock().unwrap(), "断ったときは何も始めない");
        let on = AppSettings { lyrics_online: true, ..Default::default() };
        begin(&on, &ctl).unwrap();
        assert_eq!(begin(&on, &ctl).unwrap_err().kind, ErrorKind::LyricsOnlineBusy);
        drop(ctl.end_guard());
        begin(&on, &ctl).unwrap();
        // 走っている途中でオフにされたら、次の要求の前に止まる
        let enabled = Cell::new(true);
        let flag = Arc::new(AtomicBool::new(false));
        let g = gate(flag.clone(), || enabled.get());
        assert_eq!(g(), None);
        enabled.set(false);
        assert_eq!(g(), Some(Stop::Disabled));
        flag.store(true, Ordering::SeqCst);
        assert_eq!(g(), Some(Stop::Cancelled));
    }

    /// やめるボタン：待ちの途中でも、曲の間でも止まる。それまでの結果は残る
    #[test]
    fn cancel_stops_batch() {
        let db = Db::open_memory("online_cancel").unwrap();
        let a = add(&db, "C:/o/c1.mp3", "One", "Artist");
        let b = add(&db, "C:/o/c2.mp3", "Two", "Artist");
        add(&db, "C:/o/c3.mp3", "Three", "Artist");
        // 2曲目の前の間隔の待ちで止まる
        let mut t = FakeTransport::default();
        t.push(200, SYNCED).push(200, SYNCED).push(200, SYNCED);
        let env = FakeEnv { stop_after_sleeps: Some(1), ..Default::default() };
        let mut c = Client::new(t, env, || None);
        let (p, changed) = run_batch(&db, Mode::Missing, &mut c, &no_tag, &tag_has_title, &mut |_| {});
        assert_eq!(p.aborted.as_deref(), Some("cancelled"));
        assert_eq!((p.done, p.synced, p.requests), (1, 1, 1));
        assert_eq!(changed, vec![a]);
        assert!(ext(&db, b).is_none());
        // 曲の間で（gate が止める）
        let calls = Cell::new(0);
        let mut t = FakeTransport::default();
        t.push(200, SYNCED).push(200, SYNCED);
        let mut c = Client::new(t, FakeEnv::default(), || {
            calls.set(calls.get() + 1);
            // 1回目：一括の頭、2回目：選ぶ途中（足した）、3回目：1曲目の前、4回目：1曲目の送る直前、5回目：2曲目の前
            (calls.get() >= 5).then_some(Stop::Cancelled)
        });
        let (p, _) = run_batch(&db, Mode::Missing, &mut c, &no_tag, &tag_has_title, &mut |_| {});
        assert_eq!(p.aborted.as_deref(), Some("cancelled"));
        assert_eq!(p.done, 1);
    }

    #[test]
    fn errors_and_rate_limits_abort() {
        let db = Db::open_memory("online_err").unwrap();
        for i in 0..7 {
            add(&db, &format!("C:/o/e{i}.mp3"), &format!("Song {i}"), "Artist");
        }
        // 5曲続けてエラー → 打ち切り
        let mut t = FakeTransport::default();
        for _ in 0..7 {
            t.push(301, "");
        }
        let mut c = Client::new(t, FakeEnv::default(), || None);
        let (p, _) = run_batch(&db, Mode::Missing, &mut c, &no_tag, &tag_has_title, &mut |_| {});
        assert_eq!(p.aborted.as_deref(), Some("too_many_errors"));
        assert_eq!((p.errors, p.done, p.requests), (5, 5, 5));
        // 429 の Retry-After が長すぎる → 全体を打ち切る
        let mut t = FakeTransport::default();
        t.push_ra(429, "100000");
        let mut c = Client::new(t, FakeEnv::default(), || None);
        let (p, _) = run_batch(&db, Mode::Missing, &mut c, &no_tag, &tag_has_title, &mut |_| {});
        assert_eq!(p.aborted.as_deref(), Some("rate_limited_long_wait"));
        assert_eq!(p.done, 0);
        // 壊れた応答はその曲のエラー。次の曲へ進む
        let mut t = FakeTransport::default();
        t.push(200, "{broken").push(200, PLAIN);
        let mut c = Client::new(t, FakeEnv::default(), || None);
        let (p, _) = run_batch(&db, Mode::Missing, &mut c, &no_tag, &tag_has_title, &mut |_| {});
        assert_eq!((p.errors, p.plain), (1, 1));
        assert!(store::miss(&db, 1).unwrap().is_none(), "エラーは覚えない（次も聞く）");
    }

    #[test]
    fn single_track() {
        let db = Db::open_memory("online_one").unwrap();
        let a = add(&db, "C:/o/s1.mp3", "One", "Artist");
        let m = add(&db, "C:/o/s2.mp3", "Two", "Artist");
        let e = add(&db, "C:/o/s3.mp3", "", "Artist");
        store::upsert(&db.writer(), m, StoredKind::Manual, "手動", "plain").unwrap();
        // 手動の歌詞がある曲・名前が空の曲は送らない
        let mut c = Client::new(PanicTransport, FakeEnv::default(), || None);
        assert_eq!(fetch_one(&db, m, &mut c).unwrap().status, "manual");
        assert_eq!(fetch_one(&db, e, &mut c).unwrap().status, "no_metadata");
        assert_eq!(fetch_one(&db, 9999, &mut c).unwrap_err().kind, ErrorKind::FileNotFound);
        // 覚えた「見つからない」があっても、押したときは聞き直す
        store::record_miss(&db, a, "not_found", "old").unwrap();
        let mut t = FakeTransport::default();
        t.push(200, SYNCED);
        let mut c = Client::new(t, FakeEnv::default(), || None);
        let r = fetch_one(&db, a, &mut c).unwrap();
        assert_eq!(r, OneResult { track_id: a, status: "synced".into(), error: None });
        assert!(store::miss(&db, a).unwrap().is_none());
        // もう一度押して見つからなければ、取れていた歌詞は残す
        let mut t = FakeTransport::default();
        t.push(404, "").push(404, "");
        let mut c = Client::new(t, FakeEnv::default(), || None);
        assert_eq!(fetch_one(&db, a, &mut c).unwrap().status, "kept");
        assert!(ext(&db, a).is_some());
        // 通信のエラーは記号だけ
        let mut t = FakeTransport::default();
        t.push(200, "[1,2");
        let mut c = Client::new(t, FakeEnv::default(), || None);
        let r = fetch_one(&db, a, &mut c).unwrap();
        assert_eq!((r.status.as_str(), r.error.as_deref()), ("error", Some("bad_response")));
        // 手動の歌詞は上書きしない
        assert_eq!(store::get(&db, m, StoredKind::Manual).unwrap().unwrap().raw_text, "手動");
    }

    /// 曲名がタグに無く、ファイル名から作った値なら送らない。タグにファイル名と同じ曲名があれば送る
    #[test]
    fn title_from_file_name_is_not_sent() {
        let db = Db::open_memory("online_stem").unwrap();
        // 曲ファイルが無い（タグを読めない）：送らない側に倒す
        let a = add(&db, "C:/o/My Private Recording 2024.mp3", "My Private Recording 2024", "Artist");
        // タグの無い曲ファイル（wav）：曲名はファイル名から作った値
        let dir = crate::test_tmp().join(format!("plp-stem-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures");
        let wav = dir.join("mono_22k.wav");
        std::fs::copy(fixtures.join("mono_22k.wav"), &wav).unwrap();
        let b = add(&db, &wav.to_string_lossy(), "mono_22k", "Artist");
        // タグに曲名「Sine MP3」がある曲を、同じ名前のファイルにした：タグの値なので送る
        let tagged = dir.join("Sine MP3.mp3");
        std::fs::copy(fixtures.join("sine.mp3"), &tagged).unwrap();
        assert_eq!(tags_read::read_values(&tagged).unwrap().title, "Sine MP3");
        let c = add(&db, &tagged.to_string_lossy(), "Sine MP3", "Artist");
        assert!(title_is_file_name("C:/o/My Private Recording 2024.mp3", " My Private Recording 2024 ", &tag_has_title));
        assert!(title_is_file_name(&wav.to_string_lossy(), "mono_22k", &tag_has_title));
        assert!(!title_is_file_name(&tagged.to_string_lossy(), "Sine MP3", &tag_has_title));
        assert!(!title_is_file_name("C:/o/x.mp3", "Other", &tag_has_title));

        let mut t = FakeTransport::default();
        t.push(200, PLAIN);
        let mut c_ = Client::new(t, FakeEnv::default(), || None);
        let (p, changed) = run_batch(&db, Mode::Missing, &mut c_, &no_tag, &tag_has_title, &mut |_| {});
        assert_eq!((p.total, p.no_metadata, p.plain), (1, 2, 1));
        assert_eq!(changed, vec![c]);
        assert_eq!(c_.transport.sent.len(), 1);
        assert!(c_.transport.sent.iter().all(|u| !u.contains("Private") && !u.contains("mono_22k")));
        assert!(c_.transport.sent[0].contains("track_name=Sine+MP3"));
        // 1曲でも同じ
        let mut c_ = Client::new(PanicTransport, FakeEnv::default(), || None);
        assert_eq!(fetch_one(&db, a, &mut c_).unwrap().status, "no_metadata");
        assert_eq!(fetch_one(&db, b, &mut c_).unwrap().status, "no_metadata");
    }

    /// 一括の途中で外された曲・手動の歌詞が付いた曲は、送る直前に確かめ直して送らない（エラーに数えない）
    #[test]
    fn recheck_before_each_send() {
        let db = Db::open_memory("online_recheck").unwrap();
        let a = add(&db, "C:/o/r1.mp3", "One", "Artist");
        let b = add(&db, "C:/o/r2.mp3", "Two", "Artist");
        let c = add(&db, "C:/o/r3.mp3", "Three", "Artist");
        let d = add(&db, "C:/o/r4.mp3", "Four", "Artist");
        let mut t = FakeTransport::default();
        t.push(200, PLAIN).push(200, PLAIN);
        let mut cl = Client::new(t, FakeEnv::default(), || None);
        // 1曲目を処理したところで、2曲目を外し、3曲目に手動の歌詞を付ける
        let mut fired = false;
        let (p, changed) = run_batch(&db, Mode::Missing, &mut cl, &no_tag, &tag_has_title, &mut |p| {
            if !p.preparing && p.done == 1 && !fired {
                fired = true;
                tracks::remove(&db, &[b]).unwrap();
                store::upsert(&db.writer(), c, StoredKind::Manual, "手動", "plain").unwrap();
            }
        });
        assert_eq!(p.total, 4);
        assert_eq!((p.done, p.plain, p.changed_during_run, p.errors), (4, 2, 2, 0));
        assert!(p.aborted.is_none());
        assert_eq!(changed, vec![a, d]);
        assert_eq!(cl.transport.sent.len(), 2);
        assert!(cl.transport.sent.iter().all(|u| !u.contains("Two") && !u.contains("Three")));
        assert!(ext(&db, c).is_none());
        assert_eq!(store::get(&db, c, StoredKind::Manual).unwrap().unwrap().raw_text, "手動");
    }

    /// 打ち切りの文言を分けるため、最後のエラーの記号を返す
    #[test]
    fn last_error_is_reported() {
        let db = Db::open_memory("online_lasterr").unwrap();
        for i in 0..5 {
            add(&db, &format!("C:/o/l{i}.mp3"), &format!("Song {i}"), "Artist");
        }
        let mut t = FakeTransport::default();
        for _ in 0..5 {
            t.push(403, "");
        }
        let mut cl = Client::new(t, FakeEnv::default(), || None);
        let (p, _) = run_batch(&db, Mode::Missing, &mut cl, &no_tag, &tag_has_title, &mut |_| {});
        assert_eq!(p.aborted.as_deref(), Some("too_many_errors"));
        assert_eq!(p.last_error.as_deref(), Some("http_403"));
        let s = serde_json::to_string(&p).unwrap();
        assert!(s.contains("\"lastError\":\"http_403\"") && s.contains("\"changedDuringRun\":0"), "{s}");
    }

    /// 一括のスレッドが落ちても、終わりの知らせが出て running:false に戻り、処理中の印も外れる
    #[test]
    fn done_guard_reports_on_panic() {
        let ctl = Control::default();
        let on = AppSettings { lyrics_online: true, ..Default::default() };
        begin(&on, &ctl).unwrap();
        ctl.set_progress(FetchProgress { running: true, total: 10, done: 3, mode: Some(Mode::Missing), ..Default::default() });
        let sent = std::cell::RefCell::new(Vec::<FetchDone>::new());
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = DoneGuard::new(&ctl, |d: &FetchDone| sent.borrow_mut().push(d.clone()));
            panic!("boom");
        }));
        assert!(r.is_err());
        let sent = sent.into_inner();
        assert_eq!(sent.len(), 1);
        let p = &sent[0].progress;
        assert!(!p.running && p.finished);
        assert_eq!((p.done, p.total, p.aborted.as_deref()), (3, 10, Some("internal_error")));
        assert!(!ctl.progress().running);
        begin(&on, &ctl).expect("処理中の印が外れている");
        // ふつうに終われば、知らせは1回だけ
        let count = Cell::new(0);
        let g = DoneGuard::new(&ctl, |_: &FetchDone| count.set(count.get() + 1));
        g.finish(FetchDone::default());
        assert_eq!(count.get(), 1);
        begin(&on, &ctl).unwrap();
    }

    /// タグの読み取りが panic しても、一括は止まらず、その曲だけ no_metadata（送らない）
    #[test]
    fn panicking_tag_reader_skips_only_that_track() {
        let db = Db::open_memory("online_panic").unwrap();
        let a = add(&db, "C:/o/p/broken.opus", "broken", "Artist"); // 曲名がファイル名と同じ → タグを読む → panic
        let b = add(&db, "C:/o/p/ok.mp3", "Fine Song", "Artist");
        let c = add(&db, "C:/o/p/lyr.mp3", "Other Song", "Artist"); // 歌詞のタグを読むと panic → 歌詞なしとして送る
        let boom_title = |path: &str| -> bool {
            if path.ends_with("broken.opus") {
                panic!("lofty panicked (test)");
            }
            true
        };
        let boom_lyrics = |path: &str| -> bool {
            if path.ends_with("lyr.mp3") {
                panic!("lofty panicked (test)");
            }
            false
        };
        let mut t = FakeTransport::default();
        t.push(200, PLAIN).push(200, PLAIN);
        let mut cl = Client::new(t, FakeEnv::default(), || None);
        let (p, changed) = run_batch(&db, Mode::Missing, &mut cl, &boom_lyrics, &boom_title, &mut |_| {});
        assert!(p.aborted.is_none(), "{:?}", p.aborted);
        assert_eq!((p.total, p.no_metadata, p.plain, p.errors), (2, 1, 2, 0));
        assert_eq!(changed, vec![b, c]);
        assert!(cl.transport.sent.iter().all(|u| !u.contains("broken")));
        assert!(ext(&db, a).is_none());
        // 差し替えない本物の関数も、読めないファイルで落ちない
        assert!(!tag_has_title("C:/o/p/no-such.opus"));
        assert!(!tag_has_lyrics("C:/o/p/no-such.opus"));
    }

    /// 選ぶ途中でも、やめるボタンが効く。曲ファイルを読む前に止まる
    #[test]
    fn cancel_during_preparation() {
        let db = Db::open_memory("online_prep_cancel").unwrap();
        for i in 0..200 {
            add(&db, &format!("C:/o/q/song{i}.mp3"), &format!("song{i}"), "Artist"); // 全曲、曲名がファイル名と同じ
        }
        let reads = Cell::new(0);
        let probe = |_: &str| -> bool {
            reads.set(reads.get() + 1);
            true
        };
        let gate_calls = Cell::new(0);
        let mut cl = Client::new(PanicTransport, FakeEnv::default(), || {
            gate_calls.set(gate_calls.get() + 1);
            // 1回目：一括の頭。2回目以降：選ぶ途中。5回目で「やめる」
            (gate_calls.get() >= 5).then_some(Stop::Cancelled)
        });
        let mut events = vec![];
        let (p, _) = run_batch(&db, Mode::Missing, &mut cl, &no_tag, &probe, &mut |p| events.push(p.clone()));
        assert_eq!(p.aborted.as_deref(), Some("cancelled"));
        assert!(!p.preparing && p.finished && !p.running);
        assert_eq!((p.total, p.requests), (0, 0));
        assert!(reads.get() < 10, "読んだ曲 {}", reads.get());
        // 準備中の進み具合を出している（total が 0 のままではない）
        assert!(events[0].preparing);
        assert_eq!(events[0].prepare_total, 200);
    }

    /// 覚えた結果で聞き直さない曲は、曲ファイルを読まない。準備が終わると準備中を外す
    #[test]
    fn remembered_tracks_are_not_read() {
        let db = Db::open_memory("online_prep_known").unwrap();
        let a = add(&db, "C:/o/k/known.mp3", "known", "Artist");
        let b = add(&db, "C:/o/k/fresh.mp3", "fresh", "Artist");
        let q = lrclib::build_query("known", "Artist", "Album", 200_000).unwrap();
        store::record_miss(&db, a, "not_found", &lrclib::query_sig(&q)).unwrap();
        let read: std::cell::RefCell<Vec<String>> = Default::default();
        let probe = |path: &str| -> bool {
            read.borrow_mut().push(path.to_string());
            true
        };
        let mut t = FakeTransport::default();
        t.push(200, PLAIN);
        let mut cl = Client::new(t, FakeEnv::default(), || None);
        let mut events = vec![];
        let (p, changed) = run_batch(&db, Mode::Missing, &mut cl, &no_tag, &probe, &mut |p| events.push(p.clone()));
        assert_eq!((p.skipped_known, p.total, p.plain), (1, 1, 1));
        assert_eq!(changed, vec![b]);
        assert_eq!(read.borrow().as_slice(), ["C:/o/k/fresh.mp3"], "覚えた曲のタグは読まない");
        assert!(events.first().unwrap().preparing);
        let after: Vec<&FetchProgress> = events.iter().filter(|e| !e.preparing).collect();
        assert_eq!((after[0].total, after[0].prepare_done, after[0].prepare_total), (1, 2, 2));
    }

    #[test]
    fn real_env_sleep_is_cancellable() {
        let flag = Arc::new(AtomicBool::new(true));
        let mut env = RealEnv::new(flag.clone());
        let t = Instant::now();
        assert_eq!(env.sleep(30.0), Err(Stop::Cancelled));
        assert!(t.elapsed() < Duration::from_secs(1));
        flag.store(false, Ordering::SeqCst);
        assert_eq!(env.sleep(f64::NAN), Ok(()));
        assert_eq!(env.sleep(-5.0), Ok(()));
        for _ in 0..1000 {
            let r = env.rand();
            assert!((0.0..1.0).contains(&r));
        }
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の10）：ネットから取った歌詞（偽の応答）も、表示は 5,000 行と警告
    #[test]
    fn fetched_lyrics_are_capped_on_display() {
        let db = Db::open_memory("online_cap").unwrap();
        let a = add(&db, "C:/o/cap.mp3", "Song", "Artist");
        let lines: String = (0..6000).map(|i| format!("[{:02}:{:02}.00]line{i}\\n", i / 60, i % 60)).collect();
        let body = format!(r#"{{"duration":200,"syncedLyrics":"{lines}"}}"#);
        let mut t = FakeTransport::default();
        t.push(200, &body);
        let mut c = Client::new(t, FakeEnv::default(), || None);
        assert_eq!(fetch_one(&db, a, &mut c).unwrap().status, "synced");
        let p = crate::lyrics::service::get_lyrics(&db, a).unwrap();
        assert_eq!(p.lines.len(), crate::lyrics::lrc::MAX_LINES);
        assert!(p.warnings.iter().any(|w| w == crate::lyrics::lrc::warn::LINES_TRUNCATED));
    }
}
