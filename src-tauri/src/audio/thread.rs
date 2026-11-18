//! 音声スレッド本体。デコード → 曲の側の輪 → 出力ごとの流し込み。命令と通知を受けて状態を変える。
//!
//! v3.8.0（同時再生。仕様 13.1・13.5）：
//! - 仮想の再生時計（`Clock`）が位置と曲の終わりを決める。出力が無くても（全部オフでも）進む。
//!   速さは基準の出力1つに寄せる（基準は伸び縮みさせない＝出力が1つだけなら 3.7.0 と同じ音）
//! - 出力は枠3つ。どれも同じ「開始時刻の門」で入る（再生開始・再開・シーク・曲の切り替え・途中でオン・戻ってきた）
//! - 抜けた出力はその枠だけ切断中にして手放し、つなぎ直す（知らせの直後、0.5・1・2 秒、そのあと一覧にある間は 5 秒おき）
//! - オンの出力が全部鳴らせなくなったら一時停止（戻っても自動では再生しない）
//! - デバイスを開くのは別のスレッド（`pl-audio-open`）。開くのに時間がかかる機器（Bluetooth など）でも、
//!   ほかの出力への書き込みを止めない（検査1回目の指摘4）。開けたストリームは知らせの通り道で受け取る

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{select, unbounded, Receiver, Sender};

use super::decoder::Decoder;
use super::devices;
use super::engine::{Command, Event, Internal, OutputSpec, OutputSpecs, OutputStatus, UnavailableReason};
use super::lane::{Control, Lane, Phase};
use super::output::{OpenFail, OutputBank, OutputStream, SLOTS};
use super::resample::{Converter, STRETCH_CHUNK};
use super::song_ring::{SongRing, RING_CHANNELS};
use super::sync::{
    delay_frames, ms_to_ns, pick_reference, plan_gate, qpc_now_ns, retry_delay_ms, track_ended, tuning, Clock,
};
use crate::error::{AppError, ErrorKind};

const POSITION_INTERVAL: Duration = Duration::from_millis(250);
/// 音声スレッドの部品が panic したときの知らせの間隔（v3.1.1 直し1回目）
const PANIC_NOTICE_INTERVAL: Duration = Duration::from_secs(5);
/// 再生を始めるとき、温まっている途中の出力をこれだけ待つ（それより遅いものは途中から入る）
const START_WAIT: Duration = Duration::from_millis(600);

pub(super) fn spawn(
    cmd_rx: Receiver<Command>,
    int_rx: Receiver<Internal>,
    int_tx: Sender<Internal>,
    ev_tx: Sender<Event>,
    bank: Arc<OutputBank>,
    outputs: OutputSpecs,
) {
    let (open_tx, open_rx) = unbounded();
    let opener = Arc::new(OpenerShared::default());
    spawn_opener(open_rx, opener.clone(), bank.clone(), int_tx.clone());
    std::thread::Builder::new()
        .name("pl-audio".into())
        .spawn(move || {
            let mut t = AudioThread::new(ev_tx, bank, open_tx, opener);
            t.set_outputs(outputs);
            t.run(cmd_rx, int_rx);
        })
        .expect("spawn audio thread");
}

/// デバイスを開いてほしい（音声スレッド → 開く役）
#[derive(Debug)]
pub(super) struct OpenRequest {
    slot: usize,
    serial: u64,
    device_id: Option<String>,
}

/// 音声スレッドと開く役が共有する、枠ごとの番号（検査3回目の新指摘1・2）
#[derive(Default)]
pub(super) struct OpenerShared {
    /// 今その枠が欲しい頼みの番号（0 は何も欲しくない）。音声スレッドが頼む前に書き、オフ・差し替えで 0 にする
    wanted: [AtomicU64; SLOTS],
    /// 開く役が実際に開き始めた頼みの番号（待ちの上限は、頼んだ時刻ではなくここから数える）
    started: [AtomicU64; SLOTS],
}

/// デバイスを開く役。頼まれた順に1つずつ開き、結果を音声スレッドへ返す。
/// 開けなかったときは、そのデバイスが一覧にあるかも引き直す（13.5 の12：一覧にある＝開けません／無い＝切断中）。
/// 音声スレッドが終わる（頼みの通り道が閉じる）と終わる
fn spawn_opener(rx: Receiver<OpenRequest>, shared: Arc<OpenerShared>, bank: Arc<OutputBank>, int_tx: Sender<Internal>) {
    let _ = std::thread::Builder::new().name("pl-audio-open".into()).spawn(move || {
        let tx = int_tx.clone();
        run_opener(
            &rx,
            &shared,
            |r| {
                let result = guard_open(|| {
                    OutputStream::open(r.device_id.as_deref(), r.slot, r.serial, bank.clone(), int_tx.clone()).map(Box::new)
                });
                let present =
                    result.is_ok() || guard_open(|| Ok(devices::find_output(r.device_id.as_deref()).is_some())).unwrap_or(true);
                (result, present)
            },
            |m| tx.send(m).is_ok(),
        );
    });
}

/// 開く役の中身。**その枠がもう欲しがっていない頼み（古い番号）は開かずに捨てる**（開くと、捨てる結果のために
/// デバイスを一瞬掴み、無音を鳴らす。遅い機器では、溜まった古い頼みを開いている間に今の頼みが待たされ続ける）。
/// 開き始めたら、その番号を `started` に書く
fn run_opener(
    rx: &Receiver<OpenRequest>,
    shared: &OpenerShared,
    mut open: impl FnMut(&OpenRequest) -> (Result<Box<OutputStream>, OpenFail>, bool),
    mut send: impl FnMut(Internal) -> bool,
) {
    while let Ok(r) = rx.recv() {
        let Some(want) = shared.wanted.get(r.slot) else { continue };
        if want.load(Ordering::Acquire) != r.serial {
            continue;
        }
        shared.started[r.slot].store(r.serial, Ordering::Release);
        let (result, present) = open(&r);
        if !send(Internal::Opened { slot: r.slot, serial: r.serial, result, present }) {
            break;
        }
    }
}

/// 開く役の中で部品（cpal・ドライバ）が panic しても、開く役のスレッドを止めない。「開けません（その他）」として返す
/// （検査2回目の指摘A：止まると、開いている途中の印が残り、音も出ず一時停止もしないまま曲が進む）
fn guard_open<T>(f: impl FnOnce() -> Result<T, OpenFail>) -> Result<T, OpenFail> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err(OpenFail::Other("panicked while opening the device".into())))
}

/// 一覧の引き方（テストでは実際の一覧を引かずに差し替える）
#[derive(Clone, Copy)]
struct DeviceLookup {
    /// ID（None は既定のデバイス）で探して、名前・実際の ID・自動のアイコン
    find: fn(Option<&str>) -> Option<(String, Option<String>, &'static str)>,
    default_id: fn() -> Option<String>,
}

fn real_find(id: Option<&str>) -> Option<(String, Option<String>, &'static str)> {
    devices::find_output(id).map(|d| (devices::device_name(&d), devices::device_id(&d), devices::device_kind(&d)))
}

const REAL_LOOKUP: DeviceLookup = DeviceLookup { find: real_find, default_id: devices::default_output_id };

struct Track {
    path: PathBuf,
    token: u64,
    decoder: Decoder,
    /// デコードしたものを曲の側の輪（2ch・開いたときのレート）へそろえる
    converter: Option<Converter>,
    decoded: Vec<f32>,
    /// 変換済みで、まだ輪に入っていないサンプル
    staged: Vec<f32>,
    eof: bool,
    ring: SongRing,
}

/// つなぎ直しの予定
#[derive(Debug, Clone, Copy)]
struct Retry {
    attempt: u32,
    at: Instant,
}

#[derive(Default)]
struct Slot {
    spec: Option<OutputSpec>,
    lane: Option<Lane>,
    status: Option<(OutputStatus, Option<UnavailableReason>)>,
    name: Option<String>,
    resolved_id: Option<String>,
    kind: Option<&'static str>,
    /// 最後に知らせた状態
    reported: Option<(OutputStatus, Option<UnavailableReason>, Option<String>)>,
    retry: Option<Retry>,
    /// 一覧に無いので、デバイスの知らせを待っている
    waiting_notice: bool,
    /// オンのまま鳴らせなくなった（戻ったら「戻りました」と知らせる）
    lost: bool,
    /// 遅らせ量を変えた：この時刻に入り直す
    delay_apply_at: Option<Instant>,
    /// 開く役に頼んで、結果を待っている（その頼みの番号）
    opening: Option<u64>,
    /// 開く役が実際に開き始めたのを見た時刻（待ちの上限はここから数える。検査3回目の新指摘1）
    opening_since: Option<Instant>,
    /// 開き始めてから上限を過ぎた：「開けません」と出し、鳴らせる見込みとしては数えない。
    /// 頼みは取り消さず、結果が届けば（その枠がまだ同じデバイスを欲しがっていれば）採る＝遅い機器でもいずれつながる
    open_overdue: bool,
}

impl Slot {
    fn on(&self) -> bool {
        self.spec.as_ref().is_some_and(|s| s.on)
    }
    fn delay_ms(&self) -> u32 {
        self.spec.as_ref().map(|s| s.delay_ms).unwrap_or(0)
    }
}

struct AudioThread {
    ev: Sender<Event>,
    bank: Arc<OutputBank>,
    open_tx: Sender<OpenRequest>,
    opener: Arc<OpenerShared>,
    lookup: DeviceLookup,
    slots: [Slot; SLOTS],
    track: Option<Track>,
    clock: Clock,
    playing: bool,
    /// 再生を始める（出力が温まるのを待っている）
    start_pending: Option<Instant>,
    /// 一度でも曲を開いた（3.7.0 と同じく、それまではデバイスを開かない）
    ever_loaded: bool,
    reference: Option<usize>,
    ref_offset: f64,
    ref_needs_offset: bool,
    ref_bad_since: Option<u64>,
    last_follow_ns: u64,
    sync_ok: bool,
    serial: u64,
    last_tick_ns: u64,
    devices_dirty_at: Option<Instant>,
    last_pos_emit: Instant,
    /// 部品が panic したことを最後に知らせた token と時刻（v3.1.1 直し1回目：同じ token で続けて起きるときは知らせを間引く）
    last_panic_notice: Option<(Option<u64>, Instant)>,
}

impl AudioThread {
    fn new(ev: Sender<Event>, bank: Arc<OutputBank>, open_tx: Sender<OpenRequest>, opener: Arc<OpenerShared>) -> Self {
        Self {
            ev,
            bank,
            open_tx,
            opener,
            lookup: REAL_LOOKUP,
            slots: Default::default(),
            track: None,
            clock: Clock::new(48_000, 0.0),
            playing: false,
            start_pending: None,
            ever_loaded: false,
            reference: None,
            ref_offset: 0.0,
            ref_needs_offset: true,
            ref_bad_since: None,
            last_follow_ns: 0,
            sync_ok: true,
            serial: 0,
            last_tick_ns: 0,
            devices_dirty_at: None,
            last_pos_emit: Instant::now(),
            last_panic_notice: None,
        }
    }

    fn run(&mut self, cmd_rx: Receiver<Command>, int_rx: Receiver<Internal>) {
        loop {
            let timeout = if self.needs_decode() { Duration::from_millis(1) } else { Duration::from_millis(5) };
            select! {
                recv(cmd_rx) -> c => match c {
                    Ok(Command::Shutdown) | Err(_) => break,
                    Ok(c) => self.handle_guarded(c),
                },
                recv(int_rx) -> i => if let Ok(i) = i {
                    let token = self.current_token();
                    self.guarded(token, |t| t.handle_internal(i));
                },
                default(timeout) => {}
            }
            let token = self.current_token();
            self.guarded(token, |t| t.tick());
        }
        for s in self.slots.iter_mut() {
            s.lane = None;
        }
    }

    fn current_token(&self) -> Option<u64> {
        self.track.as_ref().map(|t| t.token)
    }

    /// 命令を1つ処理する。曲を開く命令なら、その曲の通し番号で知らせる
    fn handle_guarded(&mut self, c: Command) {
        let token = match &c {
            Command::Load { token, .. } => Some(*token),
            _ => self.current_token(),
        };
        self.guarded(token, |t| t.handle(c));
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の2）：細工したファイルなどで部品が panic しても、音声スレッドを止めない。
    /// その曲を手放して一時停止し、「再生できない」として知らせる（司令塔は次の曲へ進むか、一時停止にする）
    fn guarded(&mut self, token: Option<u64>, f: impl FnOnce(&mut Self)) {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(self))).is_ok() {
            return;
        }
        self.track = None;
        self.halt();
        // 曲と関係ない箇所で毎回 panic しても、知らせ（とログ）は PANIC_NOTICE_INTERVAL に1回まで。
        // v3.1.1 直し2回目（検査2回目の指摘C）：間引くのは「前と同じ token（token なし同士を含む）」のときだけ。
        // 別の曲（token が違う）の panic は、間隔の中でも必ず知らせる（司令塔がその曲の結果を待ち続けないように）
        if self
            .last_panic_notice
            .is_some_and(|(last, at)| last == token && at.elapsed() < PANIC_NOTICE_INTERVAL)
        {
            return;
        }
        self.last_panic_notice = Some((token, Instant::now()));
        tracing::error!("audio thread: a component panicked; the track was dropped");
        let _ = self.ev.send(Event::Error {
            token,
            error: AppError::new(ErrorKind::PlaybackFailed, "internal error in the audio thread"),
        });
        let _ = self.ev.send(Event::Playing(false));
    }

    fn emit(&self, e: Event) {
        let _ = self.ev.send(e);
    }

    fn emit_error(&self, token: Option<u64>, error: AppError) {
        tracing::warn!("audio error: {error}");
        self.emit(Event::Error { token, error });
    }

    fn rate(&self) -> u32 {
        self.track.as_ref().map(|t| t.ring.rate()).unwrap_or(48_000)
    }

    fn position_ms(&self, now_ns: u64) -> u64 {
        let pos = self.clock.pos_at(now_ns as f64).max(0.0);
        (pos * 1000.0 / self.rate().max(1) as f64) as u64
    }

    fn emit_position(&mut self, force: bool) {
        if !force && (!self.playing || self.last_pos_emit.elapsed() < POSITION_INTERVAL) {
            return;
        }
        if let Some(t) = &self.track {
            let pos = self.position_ms(qpc_now_ns());
            let dur = t.decoder.duration_ms;
            self.emit(Event::Position {
                token: t.token,
                position_ms: if dur > 0 { pos.min(dur) } else { pos },
                duration_ms: dur,
            });
        }
        self.last_pos_emit = Instant::now();
    }

    /// 鳴らすのをやめる（一時停止・曲を手放す・エラー）。時計は止め、出力の輪は空にする
    fn halt(&mut self) {
        self.playing = false;
        self.start_pending = None;
        self.bank.paused.store(true, Ordering::Relaxed);
        self.clock.stop(qpc_now_ns());
        for s in self.slots.iter_mut() {
            if let Some(l) = s.lane.as_mut() {
                l.flush();
            }
        }
    }

    fn set_playing(&mut self, p: bool) {
        if p {
            self.start_playing();
        } else {
            self.halt();
            self.emit(Event::Playing(false));
        }
    }

    /// 再生を始める。オンの出力があるのに1つも鳴らせないなら、始めずに知らせる（2.5）
    fn start_playing(&mut self) {
        if self.track.is_none() {
            return;
        }
        self.ensure_lanes();
        if self.none_can_sound() {
            self.halt();
            self.emit(Event::Playing(false));
            self.emit(Event::OutputsAllLost);
            return;
        }
        self.playing = true;
        self.start_pending = Some(Instant::now());
        self.emit(Event::Playing(true));
    }

    /// オンの出力先があり、そのどれも鳴らせない（全部オフなら false：無音のまま進む）
    fn none_can_sound(&self) -> bool {
        let mut any_on = false;
        for s in &self.slots {
            if s.on() {
                any_on = true;
                // 開いている途中のものは、鳴らせる見込みとして数える（結果が来たら数え直す）。上限を過ぎたものは数えない
                if s.lane.is_some() || (s.opening.is_some() && !s.open_overdue) {
                    return false;
                }
            }
        }
        any_on
    }

    fn check_all_lost(&mut self) {
        if (self.playing || self.start_pending.is_some()) && self.none_can_sound() {
            self.halt();
            self.emit(Event::Playing(false));
            self.emit(Event::OutputsAllLost);
            self.emit_position(true);
        }
    }

    fn handle(&mut self, c: Command) {
        match c {
            Command::Load { path, start_ms, play, token } => self.load(path, start_ms, play, token),
            Command::Play => {
                if self.track.is_some() && !self.playing {
                    self.set_playing(true);
                }
            }
            Command::Pause => {
                self.set_playing(false);
                self.emit_position(true);
            }
            Command::Stop => {
                self.halt();
                self.emit(Event::Playing(false));
                self.track = None;
            }
            Command::Seek(ms) => self.seek(ms),
            Command::SetOutputs(o) => {
                self.set_outputs(o);
                self.check_all_lost();
            }
            Command::Shutdown => {}
            #[cfg(test)]
            Command::TestPanic => panic!("test panic in the audio thread"),
        }
    }

    fn handle_internal(&mut self, i: Internal) {
        match i {
            Internal::StreamError { slot, serial, kind, msg } => {
                let Some(s) = self.slots.get_mut(slot) else { return };
                if s.lane.as_ref().map(|l| l.stream.serial) != Some(serial) {
                    return; // 閉じたストリームの遅れた知らせ
                }
                match kind {
                    cpal::ErrorKind::DeviceNotAvailable
                    | cpal::ErrorKind::StreamInvalidated
                    | cpal::ErrorKind::DeviceChanged => {
                        tracing::info!("output {slot}: stream stopped ({kind:?}): {msg}");
                        // その枠だけ手放し、すぐ開き直してみる（既定のデバイスが替わった・形式が変わった・抜けた）
                        self.close_lane(slot);
                        self.slots[slot].retry = None;
                        self.slots[slot].waiting_notice = false;
                        if self.ever_loaded {
                            self.open_slot(slot);
                        }
                        self.report(slot);
                        self.check_all_lost();
                        self.emit(Event::DevicesChanged);
                    }
                    cpal::ErrorKind::Xrun => {}
                    _ => tracing::warn!("stream error: {kind:?} {msg}"),
                }
            }
            Internal::DevicesChanged => {
                self.devices_dirty_at = Some(Instant::now() + Duration::from_millis(tuning::DEVICE_NOTICE_DEBOUNCE_MS));
                self.emit(Event::DevicesChanged);
            }
            Internal::Opened { slot, serial, result, present } => self.finish_open(slot, serial, result, present),
        }
    }

    // ---------------- 出力の枠 ----------------

    fn set_outputs(&mut self, new: OutputSpecs) {
        for (i, spec) in new.into_iter().enumerate() {
            let old = self.slots[i].spec.clone();
            if old == spec {
                continue;
            }
            let device_changed = old.as_ref().map(|s| &s.device_id) != spec.as_ref().map(|s| &s.device_id);
            let on_changed = old.as_ref().is_some_and(|s| s.on) != spec.as_ref().is_some_and(|s| s.on);
            let delay_changed = old.as_ref().map(|s| s.delay_ms) != spec.as_ref().map(|s| s.delay_ms);
            let now_on = spec.as_ref().is_some_and(|s| s.on);
            self.slots[i].spec = spec;
            if !now_on || device_changed || on_changed {
                // オフにした・デバイスを差し替えた：手放す（オンなら開き直す）
                self.close_lane(i);
                let s = &mut self.slots[i];
                if device_changed {
                    // 前のデバイスの名前を、差し替え先の名前として出さない（検査1回目の指摘2）
                    s.name = None;
                    s.resolved_id = None;
                    s.kind = None;
                    // 司令塔はこの枠の状態を消している。同じ名前の別デバイス（同じ機種の2台目など）でも知らせ直す（検査2回目の指摘B）
                    s.reported = None;
                }
                s.retry = None;
                s.waiting_notice = false;
                s.lost = false;
                s.delay_apply_at = None;
                if !now_on {
                    s.status = Some((OutputStatus::Off, None));
                } else {
                    s.status = None;
                }
            } else if delay_changed && self.slots[i].lane.is_some() {
                // 遅らせ量だけ：つまみを止めて約150ms 後に、その出力だけ入り直す
                self.slots[i].delay_apply_at = Some(Instant::now() + Duration::from_millis(tuning::DELAY_APPLY_AFTER_MS));
            }
        }
        self.check_duplicates();
        if self.ever_loaded {
            self.ensure_lanes();
        } else {
            self.refresh_idle_status();
        }
        for i in 0..SLOTS {
            self.report(i);
        }
    }

    fn close_lane(&mut self, i: usize) {
        if let Some(l) = self.slots[i].lane.take() {
            l.stream.pause_stream();
        }
        // 開いている途中なら、その結果は受け取らない（届いたら捨てる＝デバイスを手放す）。
        // まだ開き始めていない頼みは、開く役が開かずに捨てる
        self.slots[i].opening = None;
        self.slots[i].opening_since = None;
        self.slots[i].open_overdue = false;
        self.opener.wanted[i].store(0, Ordering::Release);
        if self.reference == Some(i) {
            self.reference = None;
            self.ref_bad_since = None;
        }
    }

    /// 既定のデバイスを選んでいる枠が、ほかの枠のデバイスと同じになったら「重複」にして鳴らさない（2.1）
    fn check_duplicates(&mut self) {
        let has_default = self.slots.iter().any(|s| s.spec.as_ref().is_some_and(|x| x.device_id.is_none()));
        if !has_default {
            return;
        }
        let def = (self.lookup.default_id)();
        let explicit: Vec<String> =
            self.slots.iter().filter_map(|s| s.spec.as_ref().and_then(|x| x.device_id.clone())).collect();
        for i in 0..SLOTS {
            let is_default = self.slots[i].spec.as_ref().is_some_and(|x| x.device_id.is_none());
            if !is_default {
                continue;
            }
            let dup = def.as_ref().is_some_and(|d| explicit.contains(d));
            let was_dup = matches!(self.slots[i].status, Some((OutputStatus::Duplicate, _)));
            if dup {
                self.close_lane(i);
                let s = &mut self.slots[i];
                s.retry = None;
                if s.on() {
                    s.status = Some((OutputStatus::Duplicate, None));
                }
            } else if was_dup {
                self.slots[i].status = None;
            }
        }
    }

    fn is_duplicate(&self, i: usize) -> bool {
        matches!(self.slots[i].status, Some((OutputStatus::Duplicate, _)))
    }

    /// オンの枠を開く（開いていない・つなぎ直しの予定が無い・知らせ待ちでない・重複でないもの）
    fn ensure_lanes(&mut self) {
        for i in 0..SLOTS {
            let s = &self.slots[i];
            if s.on()
                && s.lane.is_none()
                && s.opening.is_none()
                && s.retry.is_none()
                && !s.waiting_notice
                && !self.is_duplicate(i)
            {
                self.open_slot(i);
                self.report(i);
            }
        }
    }

    /// 曲を開く前（デバイスをまだ掴まない間）：一覧にあるかだけで状態を出す
    fn refresh_idle_status(&mut self) {
        let def = (self.lookup.default_id)();
        for i in 0..SLOTS {
            if self.slots[i].lane.is_some() || self.is_duplicate(i) {
                continue;
            }
            let Some(spec) = self.slots[i].spec.clone() else {
                self.slots[i].status = None;
                continue;
            };
            if !spec.on {
                self.slots[i].status = Some((OutputStatus::Off, None));
                continue;
            }
            let find = self.lookup.find;
            let dev = match &spec.device_id {
                None => def.as_ref().and_then(|_| find(None)),
                Some(id) => find(Some(id)),
            };
            let s = &mut self.slots[i];
            match dev {
                Some((name, id, kind)) => {
                    s.status = Some((OutputStatus::Ok, None));
                    s.name = Some(name);
                    s.resolved_id = id;
                    s.kind = Some(kind);
                }
                None => s.status = Some((OutputStatus::Disconnected, None)),
            }
        }
    }

    /// 開く役に頼む（結果は `finish_open`）。重複なら頼まない
    fn open_slot(&mut self, i: usize) -> bool {
        let Some(spec) = self.slots[i].spec.clone().filter(|s| s.on) else { return false };
        self.check_duplicates();
        if self.is_duplicate(i) {
            return false;
        }
        self.serial += 1;
        let serial = self.serial;
        self.slots[i].opening = Some(serial);
        self.slots[i].opening_since = None;
        self.slots[i].open_overdue = false;
        // 頼む前に「今欲しいのはこの番号」と書く（前の頼みが開く役に溜まっていても、それは開かれない）
        self.opener.wanted[i].store(serial, Ordering::Release);
        if self.open_tx.send(OpenRequest { slot: i, serial, device_id: spec.device_id }).is_ok() {
            return true;
        }
        // 開く役がいない（検査2回目の指摘A）：印を残さず、開けなかったものとして試し直しに回す
        tracing::warn!("output {i}: the opener thread is gone");
        self.finish_open(i, serial, Err(OpenFail::Other("the opener thread is gone".into())), true);
        false
    }

    /// 開くのに時間がかかっている枠（検査2回目の指摘C・3回目の新指摘1）。
    /// 上限は、開く役が**実際に開き始めてから**数える（ほかの枠の遅い機器の後ろで待っている間は数えない）。
    /// 過ぎたら「開けません（その他）」と出し、鳴らせる見込みとしては数えない（ほかに鳴らせる出力が無ければ一時停止）。
    /// ただし頼みは取り消さず、新しい頼みも積まない。結果が届けば採るので、10 秒を超えて開く機器もいずれつながる
    fn check_open_timeouts(&mut self) {
        let limit = Duration::from_millis(tuning::OPEN_TIMEOUT_MS);
        for i in 0..SLOTS {
            let s = &mut self.slots[i];
            let Some(serial) = s.opening else { continue };
            let Some(since) = s.opening_since else {
                if self.opener.started[i].load(Ordering::Acquire) == serial {
                    s.opening_since = Some(Instant::now());
                }
                continue;
            };
            if !s.open_overdue && since.elapsed() >= limit {
                tracing::warn!("output {i}: opening is taking long; still waiting for it");
                s.open_overdue = true;
                s.lost = true;
                s.status = Some((OutputStatus::Unavailable, Some(UnavailableReason::Other)));
                self.report(i);
                self.check_all_lost();
            }
        }
    }

    /// 開く役からの結果。頼んだあとで枠が変わっていたら（オフにした・差し替えた・別の頼みを出した）捨てる
    fn finish_open(&mut self, i: usize, serial: u64, result: Result<Box<OutputStream>, OpenFail>, present: bool) {
        let Some(s) = self.slots.get_mut(i) else { return };
        if s.opening != Some(serial) || !s.on() {
            return; // 古い頼みの結果。ストリームはここで drop されてデバイスを手放す
        }
        s.opening = None;
        s.opening_since = None;
        s.open_overdue = false;
        match result {
            Ok(stream) => {
                s.name = Some(stream.name.clone());
                s.resolved_id = stream.resolved_id.clone();
                s.kind = Some(stream.kind);
                s.status = Some((OutputStatus::Ok, None));
                s.retry = None;
                s.waiting_notice = false;
                s.delay_apply_at = None;
                let returned = std::mem::take(&mut s.lost);
                let name = stream.name.clone();
                s.lane = Some(Lane::new(*stream));
                if returned {
                    self.emit(Event::OutputReturned { slot: i, name });
                }
                self.update_sync();
            }
            Err(f) => {
                if !matches!(f, OpenFail::NotFound) {
                    tracing::info!("output {i}: open failed: {}", f.detail());
                }
                s.lost = true;
                // 検査1回目の指摘1：切断中か開けませんかは、cpal のエラーの種類ではなく、一覧にあるかで決める
                s.status = Some(if !present {
                    (OutputStatus::Disconnected, None)
                } else {
                    match f {
                        OpenFail::Format(_) => (OutputStatus::Unavailable, Some(UnavailableReason::Format)),
                        OpenFail::Busy(_) => (OutputStatus::Unavailable, Some(UnavailableReason::Busy)),
                        OpenFail::NotFound | OpenFail::Other(_) => (OutputStatus::Unavailable, Some(UnavailableReason::Other)),
                    }
                });
                let attempt = s.retry.map(|r| r.attempt + 1).unwrap_or(0);
                match retry_delay_ms(attempt, present) {
                    Some(ms) => {
                        s.retry = Some(Retry { attempt, at: Instant::now() + Duration::from_millis(ms) });
                        s.waiting_notice = false;
                    }
                    None => {
                        s.retry = None;
                        s.waiting_notice = true;
                    }
                }
            }
        }
        self.report(i);
        self.check_all_lost();
    }

    /// 開いている出力の時計が全部 QPC とそろっているか（13.1 の2）。そろわない出力を閉じたら、ずれの直しを戻す
    /// （検査1回目の指摘8：一度切ったら戻らなかった）
    fn update_sync(&mut self) {
        let ok = self.slots.iter().filter_map(|s| s.lane.as_ref()).all(|l| l.stream.clock_matches());
        if ok == self.sync_ok {
            return;
        }
        self.sync_ok = ok;
        if !ok {
            tracing::warn!("an output's clock differs from QPC; sync disabled");
            self.reference = None;
            self.emit(Event::SyncUnavailable);
        } else {
            tracing::info!("all output clocks match QPC again; sync enabled");
        }
    }

    /// 状態が変わっていれば知らせる
    fn report(&mut self, i: usize) {
        let s = &self.slots[i];
        let (status, reason) = match (&s.spec, s.status) {
            (None, _) => (OutputStatus::Off, None),
            (Some(sp), _) if !sp.on => (OutputStatus::Off, None),
            (_, Some(st)) => st,
            (_, None) => (OutputStatus::Ok, None),
        };
        let key = (status, reason, s.name.clone());
        if s.reported.as_ref() == Some(&key) {
            return;
        }
        let ev = Event::Output {
            slot: i,
            status,
            reason,
            name: s.name.clone(),
            resolved_id: s.resolved_id.clone(),
            kind: s.kind,
        };
        self.slots[i].reported = Some(key);
        self.emit(ev);
    }

    // ---------------- 曲 ----------------

    fn load(&mut self, path: PathBuf, start_ms: u64, play: bool, token: u64) {
        self.halt();
        self.emit(Event::Playing(false));
        self.track = None;
        let opened = Decoder::open(&path);
        self.last_tick_ns = qpc_now_ns();
        let mut decoder = match opened {
            Ok(d) => d,
            Err(e) => {
                self.emit_error(Some(token), e);
                return;
            }
        };
        let mut base = 0;
        if start_ms > 0 {
            if let Ok(p) = decoder.seek(start_ms) {
                base = p;
            }
        }
        let rate = decoder.sample_rate.max(1);
        let start_frame = (base as u128 * rate as u128 / 1000) as i64;
        let duration_ms = decoder.duration_ms;
        self.track = Some(Track {
            path,
            token,
            decoder,
            converter: None,
            decoded: Vec::new(),
            staged: Vec::new(),
            eof: false,
            ring: SongRing::new(rate, start_frame),
        });
        let speed = if self.clock.speed() > 0.0 { self.clock.speed() } else { Clock::nominal_speed(rate) };
        self.clock = Clock::new(rate, start_frame as f64);
        // 前の曲と同じレートなら、基準の速さを引き継ぐ（違えば名目から。基準に合わせ直す）
        if (speed / Clock::nominal_speed(rate) - 1.0).abs() < 0.01 {
            self.clock.set_speed(0, speed);
        }
        self.ref_needs_offset = true;
        self.ever_loaded = true;
        self.emit(Event::Loaded { token, duration_ms });
        self.ensure_lanes();
        if play {
            self.start_playing();
        }
        self.emit_position(true);
    }

    fn seek(&mut self, ms: u64) {
        let Some(t) = self.track.as_mut() else { return };
        let ms = if t.decoder.duration_ms > 0 { ms.min(t.decoder.duration_ms.saturating_sub(1)) } else { ms };
        match t.decoder.seek(ms) {
            Ok(p) => {
                let rate = t.ring.rate();
                let frame = (p as u128 * rate as u128 / 1000) as i64;
                t.ring.reset(frame);
                t.staged.clear();
                t.eof = false;
                if let Some(c) = t.converter.as_mut() {
                    c.reset();
                }
                self.clock.set_stopped(frame as f64);
                for s in self.slots.iter_mut() {
                    if let Some(l) = s.lane.as_mut() {
                        l.flush();
                    }
                }
                if self.playing {
                    self.start_pending = Some(Instant::now());
                }
            }
            Err(e) => {
                let token = t.token;
                self.emit_error(Some(token), e);
            }
        }
        self.emit_position(true);
    }

    fn needs_decode(&self) -> bool {
        match &self.track {
            Some(t) => !t.eof && t.ring.free_frames() > t.ring.capacity_frames() / 4,
            None => false,
        }
    }

    /// 遅らせ量の最大（オンの枠。曲のフレーム）
    fn max_delay_frames(&self, open_only: bool) -> f64 {
        let rate = self.rate();
        self.slots
            .iter()
            .filter(|s| s.on() && (!open_only || s.lane.is_some()))
            .map(|s| delay_frames(s.delay_ms(), rate))
            .fold(0.0, f64::max)
    }

    fn decode(&mut self, now_ns: u64) {
        // もう誰も読まない所を捨てる：鳴らしている出力が読む所と、「時計 − 遅らせ量の最大」のうち前のほう
        let keep_back = self.max_delay_frames(false) + 2.0 * STRETCH_CHUNK as f64;
        let clock_pos = self.clock.pos_at(now_ns as f64);
        let lanes_min = self.slots.iter().filter_map(|s| s.lane.as_ref().and_then(|l| l.feed_pos())).min();
        let Some(t) = self.track.as_mut() else { return };
        let mut keep_from = (clock_pos - keep_back).floor() as i64;
        if let Some(m) = lanes_min {
            keep_from = keep_from.min(m);
        }
        t.ring.drop_before(keep_from);
        if let Err((token, path, e)) = fill_ring(t) {
            tracing::warn!("decode error in {}: {e}", path.display());
            self.track = None;
            self.halt();
            let _ = self.ev.send(Event::Error { token: Some(token), error: e });
            let _ = self.ev.send(Event::Playing(false));
        }
    }

    fn tick(&mut self) {
        let now = qpc_now_ns();
        // スリープ明けなど音声スレッドが長く空いた：時計を止めて付け直す（13.1 の1）
        if self.last_tick_ns != 0 && now.saturating_sub(self.last_tick_ns) > ms_to_ns(tuning::STALL_RESET_MS) {
            let pos = self.clock.pos_at(self.last_tick_ns as f64);
            let rate = self.rate();
            self.clock = Clock::new(rate, pos);
            self.reference = None;
            for s in self.slots.iter_mut() {
                if let Some(l) = s.lane.as_mut() {
                    l.rewarm();
                }
            }
            if self.playing {
                self.start_pending = Some(Instant::now());
            }
        }
        self.last_tick_ns = now;

        self.update_sync();
        for s in self.slots.iter_mut() {
            if let Some(l) = s.lane.as_mut() {
                l.poll();
            }
        }
        self.resolve_start(now);
        self.update_reference(now);
        self.decode(now);
        self.drive_lanes(now);
        self.check_end(now);
        self.check_open_timeouts();
        self.run_retries();
        self.run_device_notice();
        self.emit_position(false);
    }

    /// 再生を始める：温まっている出力を少し待ってから、時計を「今＋80ms」で動かす。同じ時刻で全部の門が開く
    fn resolve_start(&mut self, now: u64) {
        let Some(since) = self.start_pending else { return };
        if self.track.is_none() {
            self.start_pending = None;
            return;
        }
        let waiting = self.slots.iter().any(|s| s.on() && s.opening.is_some())
            || self
                .slots
                .iter()
                .filter_map(|s| s.lane.as_ref())
                .any(|l| matches!(l.phase, Phase::Warming | Phase::Flushing));
        if waiting && since.elapsed() < START_WAIT {
            return;
        }
        let pos = self.clock.pos_at(now as f64);
        self.clock.start_at(now + ms_to_ns(tuning::GATE_LEAD_MS), pos);
        self.bank.paused.store(false, Ordering::Relaxed);
        self.start_pending = None;
        self.ref_needs_offset = true;
    }

    /// 基準の出力を決め、時計の速さをそれに寄せる（13.1 の1）
    fn update_reference(&mut self, now: u64) {
        if !self.sync_ok {
            self.reference = None;
            return;
        }
        // 基準を失った（閉じた）ら選び直す。鳴っていないだけ（入り直し・一時停止）の間は、時計は直前の速さのまま
        if let Some(r) = self.reference {
            if self.slots[r].lane.is_none() {
                self.reference = None;
                self.ref_bad_since = None;
            }
        }
        let cands: Vec<(usize, f64)> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let l = s.lane.as_ref()?;
                (l.phase == Phase::Running && l.timing.observed_ms() >= tuning::REFERENCE_PICK_AFTER_MS)
                    .then(|| (i, l.timing.jitter_ms().unwrap_or(f64::INFINITY)))
            })
            .collect();
        match self.reference {
            None => {
                if let Some(r) = pick_reference(&cands) {
                    self.set_reference(r);
                }
            }
            Some(r) => {
                let j = self.slots[r].lane.as_ref().and_then(|l| l.timing.jitter_ms()).unwrap_or(0.0);
                if j > tuning::REFERENCE_JITTER_MAX_MS {
                    let since = *self.ref_bad_since.get_or_insert(now);
                    if now.saturating_sub(since) > ms_to_ns(tuning::REFERENCE_JITTER_HOLD_MS) {
                        let better: Vec<(usize, f64)> = cands.iter().copied().filter(|c| c.0 != r && c.1 < j).collect();
                        if let Some(n) = pick_reference(&better) {
                            tracing::info!("reference output {r} -> {n} (jitter {j:.2} ms)");
                            self.set_reference(n);
                        }
                    }
                } else {
                    self.ref_bad_since = None;
                }
            }
        }
        // 基準の位置と速さに時計を寄せる（約100ms ごと）
        let Some(r) = self.reference else { return };
        if now.saturating_sub(self.last_follow_ns) < ms_to_ns(tuning::CONTROL_INTERVAL_MS) {
            return;
        }
        let Some(lane) = self.slots[r].lane.as_ref() else { return };
        // 時計も基準も、開始時刻を過ぎてから（門の前の位置で差を覚えると、時計がずれる）
        if !lane.started(now) || !self.clock.started(now) {
            return;
        }
        let delay = delay_frames(self.slots[r].delay_ms(), self.rate());
        let Some(p) = lane.song_pos_at(now as f64) else { return };
        let pos = p + delay;
        if self.ref_needs_offset {
            // 基準が決まった・入り直した：今の時計の位置との差を覚えて、時計の位置は跳ばさない
            self.ref_offset = self.clock.pos_at(now as f64) - pos;
            self.ref_needs_offset = false;
        }
        let speed = lane.song_speed();
        self.clock.follow(now, pos + self.ref_offset, speed);
        self.last_follow_ns = now;
    }

    fn set_reference(&mut self, r: usize) {
        self.reference = Some(r);
        self.ref_needs_offset = true;
        self.ref_bad_since = None;
        if let Some(l) = self.slots[r].lane.as_mut() {
            l.become_reference();
        }
    }

    /// 出力ごと：門を開く・書く・倍率を決め直す・入り直す
    fn drive_lanes(&mut self, now: u64) {
        let rate = self.rate();
        let can_play = self.playing && self.start_pending.is_none() && self.clock.running() && self.track.is_some();
        for i in 0..SLOTS {
            // 遅らせ量を変えた枠は入り直す
            if self.slots[i].delay_apply_at.is_some_and(|at| Instant::now() >= at) {
                self.slots[i].delay_apply_at = None;
                if let Some(l) = self.slots[i].lane.as_mut() {
                    l.flush();
                }
            }
            let delay = delay_frames(self.slots[i].delay_ms(), rate);
            let is_ref = self.reference == Some(i);
            let follow = self.sync_ok && self.reference.is_some() && !is_ref;
            let Some(lane) = self.slots[i].lane.as_mut() else { continue };
            if can_play && lane.phase == Phase::Idle {
                if let Some(g) = plan_gate(&lane.timing, &self.clock, now, delay) {
                    if lane.arm(g, rate, delay, now) && is_ref {
                        // 基準が入り直した：門を過ぎてから、時計との差を覚え直す
                        lane.become_reference();
                        self.ref_needs_offset = true;
                    }
                }
            }
            if let Some(t) = self.track.as_ref() {
                lane.fill(&t.ring);
            }
            if follow {
                if let Control::Reenter(e) = lane.control(now, &self.clock) {
                    tracing::info!("output {i}: off by {e:.1} ms; re-entering");
                    lane.flush();
                }
            }
        }
    }

    fn check_end(&mut self, now: u64) {
        if !self.playing || self.start_pending.is_some() {
            return;
        }
        let max_delay = self.max_delay_frames(true);
        let ended = match &self.track {
            Some(t) => {
                let end = t.ring.end_frame().unwrap_or(i64::MAX);
                t.staged.is_empty() && track_ended(t.ring.eof(), end, self.clock.pos_at(now as f64), max_delay)
            }
            None => false,
        };
        if ended {
            let token = self.current_token().unwrap_or(0);
            self.emit_position(true);
            self.track = None;
            self.halt();
            self.emit(Event::Ended { token });
        }
    }

    /// つなぎ直しの予定が来た枠を試す。失敗の数え方（何回目か）は予定に持ち、結果を受けた `finish_open` が次の予定を決める。
    /// 一覧に無い（知らせ待ち）の枠は予定を持たないので、ここでは試さない
    fn run_retries(&mut self) {
        let now = Instant::now();
        for i in 0..SLOTS {
            let s = &self.slots[i];
            let due = s.retry.is_some_and(|r| now >= r.at);
            if !due || !s.on() || s.lane.is_some() || s.opening.is_some() {
                continue;
            }
            if self.ever_loaded {
                self.open_slot(i);
            } else {
                self.slots[i].retry = None;
                self.refresh_idle_status();
                self.report(i);
            }
        }
    }

    /// デバイスの変化の知らせ（約250ms まとめたもの）：重複を見直し、切断中・開けませんのオンの枠を、すぐ試す
    fn run_device_notice(&mut self) {
        let Some(at) = self.devices_dirty_at else { return };
        if Instant::now() < at {
            return;
        }
        self.devices_dirty_at = None;
        self.check_duplicates();
        for s in self.slots.iter_mut() {
            if s.lane.is_none() {
                s.retry = None;
                s.waiting_notice = false;
            }
        }
        if self.ever_loaded {
            self.ensure_lanes();
        } else {
            self.refresh_idle_status();
        }
        for i in 0..SLOTS {
            self.report(i);
        }
        self.check_all_lost();
    }
}

/// 曲の側の輪に詰める。デコードに失敗したら (token, パス, エラー) を返す
fn fill_ring(t: &mut Track) -> Result<(), (u64, PathBuf, AppError)> {
    let rate = t.ring.rate();
    // 1回の呼び出しで詰めすぎない（命令への反応を遅らせないため）
    for _ in 0..16 {
        if !t.staged.is_empty() {
            let n = t.ring.push(&t.staged);
            t.staged.drain(..n * RING_CHANNELS);
            if !t.staged.is_empty() {
                return Ok(()); // 輪がいっぱい
            }
        }
        if t.eof {
            if !t.ring.eof() {
                t.ring.set_eof();
            }
            return Ok(());
        }
        if t.ring.free_frames() < 4096 {
            return Ok(());
        }
        let need_new = t
            .converter
            .as_ref()
            .map(|c| !c.matches(t.decoder.sample_rate, t.decoder.channels, rate, RING_CHANNELS))
            .unwrap_or(true);
        if need_new {
            t.converter = Some(Converter::new(t.decoder.sample_rate, t.decoder.channels, rate, RING_CHANNELS));
        }
        t.decoded.clear();
        match t.decoder.decode_next(&mut t.decoded) {
            Ok(more) => {
                if let Some(conv) = t.converter.as_mut() {
                    conv.process(&t.decoded, &mut t.staged, !more);
                }
                if !more {
                    t.eof = true;
                }
            }
            Err(e) => {
                t.eof = true;
                t.staged.clear();
                return Err((t.token, t.path.clone(), e));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::engine::{AudioEngine, Command, Event};
    use crate::error::ErrorKind;
    use std::time::Duration;

    fn next_error(ev: &crossbeam_channel::Receiver<Event>) -> (Option<u64>, ErrorKind) {
        loop {
            match ev.recv_timeout(Duration::from_secs(5)).expect("no event from the audio thread") {
                Event::Error { token, error } => return (token, error.kind),
                _ => continue,
            }
        }
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の2）：0 Hz の WAV を開いても、音声スレッドの中で panic が起きても、
    /// スレッドは止まらず、次の命令を受ける。どの場合も出力デバイスは開かない（開く前に失敗する）。
    /// v3.8.0：出力の枠を渡しても、曲を開けるまではデバイスを開かない（3.7.0 と同じ）
    #[test]
    fn audio_thread_survives_zero_rate_and_panics() {
        let dir = crate::test_tmp().join(format!("plp-audio-thread-{}", std::process::id()));
        let wav = super::super::decoder::test_zero_rate_wav(&dir);
        let engine = AudioEngine::start_single(0.0, None);
        let ev = engine.events();
        engine.send(Command::Load { path: wav.clone(), start_ms: 0, play: true, token: 1 });
        assert_eq!(next_error(&ev), (Some(1), ErrorKind::UnsupportedFormat));
        engine.send(Command::TestPanic);
        assert_eq!(next_error(&ev).1, ErrorKind::PlaybackFailed);
        // まだ生きている：次の命令に答える
        engine.send(Command::Load { path: wav, start_ms: 0, play: true, token: 3 });
        assert_eq!(next_error(&ev), (Some(3), ErrorKind::UnsupportedFormat));
        drop(engine);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// v3.1.1 直し1回目：同じ panic が毎ループ起きても、知らせは間引く（5 秒に1回まで）。曲は毎回手放す
    #[test]
    fn repeated_panics_are_reported_sparingly() {
        let (ev_tx, ev_rx) = crossbeam_channel::unbounded();
        let (open_tx, _open_rx) = crossbeam_channel::unbounded();
        let mut t = super::AudioThread::new(ev_tx, std::sync::Arc::new(super::OutputBank::new()), open_tx, Default::default());
        for _ in 0..100 {
            t.guarded(Some(1), |_| panic!("test panic every loop"));
        }
        let errors = ev_rx.try_iter().filter(|e| matches!(e, Event::Error { .. })).count();
        assert_eq!(errors, 1);
        // 間隔を過ぎたら、同じ token でもまた知らせる
        t.last_panic_notice = Some((Some(1), std::time::Instant::now() - super::PANIC_NOTICE_INTERVAL));
        t.guarded(Some(1), |_| panic!("test panic after the interval"));
        assert_eq!(ev_rx.try_iter().filter(|e| matches!(e, Event::Error { .. })).count(), 1);
        // v3.1.1 直し2回目（検査2回目の指摘C）：5 秒の中でも、token が違う panic（曲 A → 曲 B）は必ず知らせる。
        // Error（その token）と Playing(false) の両方が届く
        for tok in [Some(10), Some(11), None, Some(12)] {
            t.guarded(tok, |_| panic!("test panic on another track"));
            let got: Vec<Event> = ev_rx.try_iter().collect();
            assert!(got.iter().any(|e| matches!(e, Event::Error { token, .. } if *token == tok)), "{tok:?}: {got:?}");
            assert!(got.iter().any(|e| matches!(e, Event::Playing(false))), "{tok:?}: {got:?}");
        }
        // 同じ token がすぐ続けば、また間引く
        t.guarded(Some(12), |_| panic!("same token again"));
        assert_eq!(ev_rx.try_iter().count(), 0);
        // ふつうに終わる処理は何も送らない
        t.guarded(Some(3), |_| {});
        assert_eq!(ev_rx.try_iter().count(), 0);
    }

    /// v3.8.0：全部オフ（出力が無い）でも、時計は進み、位置の知らせが進み、曲の終わりで Ended が来る（仕様 2.1）。
    /// 出力の枠が全部空なので、デバイスは開かない。曲はテスト用の音源（約3秒）の 2.5 秒目から
    #[test]
    fn all_off_advances_and_ends_without_devices() {
        let wav = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("sine.wav");
        let engine = AudioEngine::start([None, None, None], [0.0; 3]);
        let ev = engine.events();
        engine.send(Command::Load { path: wav, start_ms: 2500, play: true, token: 9 });
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let (mut ended, mut playing, mut last_pos, mut positions) = (false, false, 0u64, 0usize);
        while std::time::Instant::now() < deadline {
            match ev.recv_timeout(Duration::from_millis(100)) {
                Ok(Event::Playing(true)) => playing = true,
                Ok(Event::Position { token: 9, position_ms, .. }) => {
                    assert!(position_ms + 5 >= last_pos, "position went back: {last_pos} -> {position_ms}");
                    last_pos = position_ms;
                    positions += 1;
                }
                Ok(Event::Ended { token: 9 }) => {
                    ended = true;
                    break;
                }
                Ok(Event::Error { error, .. }) => panic!("{error}"),
                Ok(Event::OutputsAllLost) => panic!("all-off must not pause"),
                Ok(Event::Output { status, .. }) => assert_eq!(status, super::OutputStatus::Off),
                _ => {}
            }
        }
        assert!(playing && ended, "playing {playing} ended {ended}");
        assert!(positions >= 2 && last_pos >= 2500, "positions {positions} last {last_pos}");
    }

    // ---- つなぎ直しの組み立て（検査1回目の指摘10）。デバイスは開かない：開く役への頼みをテストが受け取り、
    // 開けなかった結果だけを渡す。デバイスの ID は実在しない形（`test:`）で、既定のデバイスの枠も作らない（一覧を引かない）

    use super::{AudioThread, OpenRequest, OutputSpec, OutputStatus, UnavailableReason};
    use crate::audio::output::OpenFail;
    use std::time::Instant;

    fn quiet_thread() -> (AudioThread, crossbeam_channel::Receiver<Event>, crossbeam_channel::Receiver<OpenRequest>) {
        let (ev_tx, ev_rx) = crossbeam_channel::unbounded();
        let (open_tx, open_rx) = crossbeam_channel::unbounded();
        let mut t = AudioThread::new(ev_tx, std::sync::Arc::new(super::OutputBank::new()), open_tx, Default::default());
        t.ever_loaded = true;
        (t, ev_rx, open_rx)
    }

    fn one(id: &str) -> [Option<OutputSpec>; 3] {
        [Some(OutputSpec { device_id: Some(id.into()), on: true, delay_ms: 0 }), None, None]
    }

    /// 次の予定までの ms（今から）
    fn wait_ms(t: &AudioThread) -> u128 {
        t.slots[0].retry.expect("retry").at.saturating_duration_since(Instant::now()).as_millis()
    }

    fn make_due(t: &mut AudioThread) {
        if let Some(r) = t.slots[0].retry.as_mut() {
            r.at = Instant::now() - Duration::from_millis(1);
        }
    }

    /// 一覧にあるのに開けない：開けません。0.5・1・2 秒あけて試し、そのあと 5 秒おき。
    /// 予定の前には試さない・開いている途中は二重に頼まない・古い頼みの結果は捨てる
    #[test]
    fn retry_counts_failures_and_spaces_attempts() {
        let (mut t, _ev, open_rx) = quiet_thread();
        t.set_outputs(one("test:out-a"));
        let r = open_rx.try_recv().expect("open request");
        assert_eq!((r.slot, r.device_id.as_deref()), (0, Some("test:out-a")));
        assert!(open_rx.try_recv().is_err());
        // 開いている途中は鳴らせる見込みとして数える
        assert!(!t.none_can_sound());
        t.run_retries();
        t.ensure_lanes();
        assert!(open_rx.try_recv().is_err(), "asked twice while opening");
        let mut serial = r.serial;
        // 古い番号の結果は捨てる
        t.finish_open(0, serial + 100, Err(OpenFail::Busy("busy".into())), true);
        assert_eq!(t.slots[0].opening, Some(serial));
        let expect = [500u128, 1000, 2000, 5000, 5000];
        for (n, ms) in expect.iter().enumerate() {
            t.finish_open(0, serial, Err(OpenFail::Busy("busy".into())), true);
            assert_eq!(t.slots[0].status, Some((OutputStatus::Unavailable, Some(UnavailableReason::Busy))));
            assert_eq!(t.slots[0].retry.map(|r| r.attempt), Some(n as u32));
            let w = wait_ms(&t);
            assert!(w <= *ms && w + 200 >= *ms, "attempt {n}: wait {w} ms, expected {ms}");
            assert!(!t.slots[0].waiting_notice);
            // 予定の前には試さない
            t.run_retries();
            assert!(open_rx.try_recv().is_err());
            make_due(&mut t);
            t.run_retries();
            serial = open_rx.try_recv().expect("retry request").serial;
            t.run_retries();
            assert!(open_rx.try_recv().is_err(), "asked twice while opening");
        }
    }

    /// cpal が「デバイスが無い」と返しても、一覧にあれば開けません（5 秒おきに試し続ける。検査1回目の指摘1）。
    /// 一覧に無ければ切断中で、試さずに知らせを待つ。知らせが来たら、数え直してすぐ試す
    #[test]
    fn missing_devices_wait_for_the_notice_and_restart_the_count() {
        let (mut t, _ev, open_rx) = quiet_thread();
        t.set_outputs(one("test:out-b"));
        let s1 = open_rx.try_recv().unwrap().serial;
        t.finish_open(0, s1, Err(OpenFail::NotFound), true);
        assert_eq!(t.slots[0].status, Some((OutputStatus::Unavailable, Some(UnavailableReason::Other))));
        assert!(t.slots[0].retry.is_some() && !t.slots[0].waiting_notice);
        make_due(&mut t);
        t.run_retries();
        let s2 = open_rx.try_recv().unwrap().serial;
        // 一覧に無い：切断中・予定なし・知らせ待ち。予定も、ほかの入口（ensure_lanes）も試さない
        t.finish_open(0, s2, Err(OpenFail::NotFound), false);
        assert_eq!(t.slots[0].status, Some((OutputStatus::Disconnected, None)));
        assert!(t.slots[0].retry.is_none() && t.slots[0].waiting_notice);
        t.run_retries();
        t.ensure_lanes();
        assert!(open_rx.try_recv().is_err());
        // デバイスの知らせ（まとめる間が過ぎた）：すぐ試し、失敗の数は 0 から
        t.devices_dirty_at = Some(Instant::now() - Duration::from_millis(1));
        t.run_device_notice();
        let s3 = open_rx.try_recv().expect("request after the notice").serial;
        t.finish_open(0, s3, Err(OpenFail::Other("x".into())), true);
        assert_eq!(t.slots[0].retry.map(|r| r.attempt), Some(0));
        let w = wait_ms(&t);
        assert!(w <= 500 && w + 200 >= 500, "{w}");
    }

    /// 再生中にオンの出力が全部鳴らせなくなったら一時停止して知らせる。開いている途中は待つ
    #[test]
    fn all_lost_after_the_last_open_fails() {
        let (mut t, ev, open_rx) = quiet_thread();
        t.set_outputs(one("test:out-c"));
        let s1 = open_rx.try_recv().unwrap().serial;
        t.playing = true;
        t.check_all_lost();
        assert!(t.playing, "paused while still opening");
        t.finish_open(0, s1, Err(OpenFail::NotFound), false);
        assert!(!t.playing);
        let got: Vec<Event> = ev.try_iter().collect();
        assert!(got.iter().any(|e| matches!(e, Event::OutputsAllLost)), "{got:?}");
        assert!(got.iter().any(|e| matches!(e, Event::Playing(false))));
    }

    /// デバイスを差し替えたら、前のデバイスの名前を出さない（検査1回目の指摘2）。オフにしたら、届いた結果は捨てる
    #[test]
    fn replacing_the_device_forgets_the_old_name_and_off_drops_results() {
        let (mut t, ev, open_rx) = quiet_thread();
        t.set_outputs(one("test:out-d"));
        let s1 = open_rx.try_recv().unwrap().serial;
        t.slots[0].name = Some("前のテスト用デバイス".into());
        t.slots[0].kind = Some("headphones");
        let _: Vec<Event> = ev.try_iter().collect();
        t.set_outputs(one("test:out-e"));
        assert_eq!(t.slots[0].name, None);
        assert_eq!(t.slots[0].kind, None);
        let s2 = open_rx.try_recv().unwrap().serial;
        assert_ne!(s1, s2);
        // 前の頼みの結果は捨てる
        t.finish_open(0, s1, Err(OpenFail::Busy("x".into())), true);
        assert_eq!(t.slots[0].opening, Some(s2));
        // オフにしたら、今の頼みの結果も捨てる
        t.set_outputs([Some(OutputSpec { device_id: Some("test:out-e".into()), on: false, delay_ms: 0 }), None, None]);
        assert_eq!(t.slots[0].opening, None);
        t.finish_open(0, s2, Err(OpenFail::Busy("x".into())), true);
        assert_eq!(t.slots[0].status, Some((OutputStatus::Off, None)));
        assert!(t.slots[0].retry.is_none());
    }

    // ---- 検査2回目の指摘 A・B・C ----

    /// A：開く役の中で panic しても、開く役は止まらず「開けません（その他）」として返る
    #[test]
    fn opener_panic_becomes_an_open_failure() {
        let r: Result<u8, OpenFail> = super::guard_open(|| panic!("test panic inside the opener"));
        assert!(matches!(r, Err(OpenFail::Other(_))), "{r:?}");
        assert!(matches!(super::guard_open(|| Ok(7u8)), Ok(7)));
    }

    /// A：開く役がいない（頼めない）なら、開いている途中の印を残さず、開けなかったものとして試し直しに回す。
    /// 再生中で、ほかに鳴らせる出力が無ければ一時停止して知らせる
    #[test]
    fn no_opener_leaves_no_opening_mark() {
        let (mut t, ev, open_rx) = quiet_thread();
        drop(open_rx);
        t.playing = true;
        t.set_outputs(one("test:out-f"));
        assert_eq!(t.slots[0].opening, None);
        assert_eq!(t.slots[0].status, Some((OutputStatus::Unavailable, Some(UnavailableReason::Other))));
        assert_eq!(t.slots[0].retry.map(|r| r.attempt), Some(0));
        assert!(!t.playing);
        assert!(ev.try_iter().any(|e| matches!(e, Event::OutputsAllLost)));
    }

    /// 開く役の中身（検査3回目の新指摘1・2）：その枠が今欲しい番号の頼みだけを開く。古い頼みは開かずに捨てる
    /// （デバイスを一瞬掴まない）。開き始めたら started に番号を書く。開く処理はテストの関数（デバイスは開かない）
    #[test]
    fn opener_skips_stale_requests_without_opening() {
        let shared = super::OpenerShared::default();
        let (tx, rx) = crossbeam_channel::unbounded();
        for serial in [1u64, 2, 3] {
            tx.send(OpenRequest { slot: 0, serial, device_id: Some("test:slow".into()) }).unwrap();
        }
        tx.send(OpenRequest { slot: 1, serial: 4, device_id: Some("test:other".into()) }).unwrap();
        tx.send(OpenRequest { slot: 1, serial: 5, device_id: Some("test:other".into()) }).unwrap();
        drop(tx);
        shared.wanted[0].store(3, std::sync::atomic::Ordering::Release);
        shared.wanted[1].store(0, std::sync::atomic::Ordering::Release); // 枠1は取り消し済み
        let mut opened = vec![];
        let mut sent = vec![];
        super::run_opener(
            &rx,
            &shared,
            |r| {
                opened.push((r.slot, r.serial));
                (Err(OpenFail::Busy("test".into())), true)
            },
            |m| {
                if let super::Internal::Opened { slot, serial, .. } = m {
                    sent.push((slot, serial));
                }
                true
            },
        );
        assert_eq!(opened, vec![(0, 3)]);
        assert_eq!(sent, vec![(0, 3)]);
        assert_eq!(shared.started[0].load(std::sync::atomic::Ordering::Acquire), 3);
        assert_eq!(shared.started[1].load(std::sync::atomic::Ordering::Acquire), 0);
    }

    /// 10 秒を超えて開く機器でも、いずれつながる（検査3回目の新指摘1）：
    /// - 待ちの上限は、開く役が実際に開き始めてから数える（ほかの枠の後ろで待っている間は数えない）
    /// - 上限を過ぎたら「開けません」と出し、鳴らせる見込みとしては数えない（再生中でほかに無ければ一時停止）
    /// - その間、新しい頼みは積まない（古い頼みが溜まらない）。頼みも取り消さない
    /// - 遅れて届いた結果は、その枠がまだ同じデバイスを欲しがっていれば採る（ここでは失敗の結果を渡し、採られて
    ///   試し直しの予定が立つことで確かめる。成功の結果は実際のデバイスが要るので作れないが、採るかどうかの判定は同じ）
    #[test]
    fn slow_device_is_waited_for_and_its_late_result_is_taken() {
        let (mut t, ev, open_rx) = quiet_thread();
        t.set_outputs(one("test:slow"));
        let s1 = open_rx.try_recv().unwrap().serial;
        assert_eq!(t.opener.wanted[0].load(std::sync::atomic::Ordering::Acquire), s1);
        let long_ago = Instant::now() - Duration::from_millis(crate::audio::sync::tuning::OPEN_TIMEOUT_MS + 1);
        // まだ開き始めていない（ほかの枠の後ろで待っている）：時間がたっても数えない
        t.check_open_timeouts();
        assert_eq!(t.slots[0].opening_since, None);
        // 開き始めた
        t.opener.started[0].store(s1, std::sync::atomic::Ordering::Release);
        t.check_open_timeouts();
        assert!(t.slots[0].opening_since.is_some());
        t.check_open_timeouts();
        assert!(!t.slots[0].open_overdue, "overdue too early");
        // 上限を過ぎた：開けませんと出す。再生中でほかに無いので一時停止
        t.playing = true;
        t.slots[0].opening_since = Some(long_ago);
        t.check_open_timeouts();
        assert!(t.slots[0].open_overdue);
        assert_eq!(t.slots[0].opening, Some(s1), "the request must not be dropped");
        assert_eq!(t.slots[0].status, Some((OutputStatus::Unavailable, Some(UnavailableReason::Other))));
        assert!(!t.playing);
        assert!(ev.try_iter().any(|e| matches!(e, Event::OutputsAllLost)));
        // 待っている間は、試し直し・知らせ・確かめのどの入口からも、新しい頼みを積まない
        t.run_retries();
        t.ensure_lanes();
        t.devices_dirty_at = Some(Instant::now() - Duration::from_millis(1));
        t.run_device_notice();
        t.check_open_timeouts();
        assert!(open_rx.try_recv().is_err(), "a new request was queued behind the slow one");
        assert_eq!(t.opener.wanted[0].load(std::sync::atomic::Ordering::Acquire), s1);
        // 遅れて届いた結果は採る（捨てない）
        t.finish_open(0, s1, Err(OpenFail::Busy("finally answered".into())), true);
        assert_eq!(t.slots[0].opening, None);
        assert!(!t.slots[0].open_overdue);
        assert_eq!(t.slots[0].status, Some((OutputStatus::Unavailable, Some(UnavailableReason::Busy))));
        assert_eq!(t.slots[0].retry.map(|r| r.attempt), Some(0));
    }

    /// 待っている間にオフにした・差し替えた枠の頼みは、開く役が開かない（欲しい番号を 0 にする）。届いた結果も捨てる
    #[test]
    fn turning_off_cancels_the_queued_request() {
        let (mut t, _ev, open_rx) = quiet_thread();
        t.set_outputs(one("test:slow"));
        let s1 = open_rx.try_recv().unwrap().serial;
        t.set_outputs([Some(OutputSpec { device_id: Some("test:slow".into()), on: false, delay_ms: 0 }), None, None]);
        assert_eq!(t.opener.wanted[0].load(std::sync::atomic::Ordering::Acquire), 0);
        assert_ne!(s1, 0);
        t.finish_open(0, s1, Err(OpenFail::Busy("late".into())), true);
        assert_eq!(t.slots[0].status, Some((OutputStatus::Off, None)));
        assert!(t.slots[0].retry.is_none());
    }

    fn same_name_find(id: Option<&str>) -> Option<(String, Option<String>, &'static str)> {
        id.filter(|x| x.starts_with("test:")).map(|x| ("同じ機種のテスト機".to_string(), Some(x.to_string()), "headphones"))
    }

    fn no_default() -> Option<String> {
        None
    }

    /// B：同じ名前の別デバイスへ差し替えても、名前と状態を知らせ直す（司令塔は差し替えでその枠の状態を消すため）。
    /// 曲を開く前（一覧を引くだけ）の場合。一覧は差し替えたもので、実際の一覧は引かない
    #[test]
    fn replacing_with_a_same_named_device_reports_again() {
        let (mut t, ev, _open_rx) = quiet_thread();
        t.ever_loaded = false;
        t.lookup = super::DeviceLookup { find: same_name_find, default_id: no_default };
        t.set_outputs(one("test:twin-1"));
        let first: Vec<Event> = ev.try_iter().collect();
        assert!(
            first.iter().any(|e| matches!(e, Event::Output { slot: 0, name: Some(n), .. } if n == "同じ機種のテスト機")),
            "{first:?}"
        );
        t.set_outputs(one("test:twin-2"));
        let again: Vec<Event> = ev.try_iter().collect();
        assert!(
            again.iter().any(|e| matches!(
                e,
                Event::Output { slot: 0, status: OutputStatus::Ok, name: Some(n), resolved_id: Some(id), kind: Some("headphones"), .. }
                    if n == "同じ機種のテスト機" && id == "test:twin-2"
            )),
            "{again:?}"
        );
        // 同じデバイスのまま（設定が変わらない）なら、何も知らせない（司令塔も状態を消さない）
        t.set_outputs(one("test:twin-2"));
        assert_eq!(ev.try_iter().count(), 0);
    }
}
