//! 再生の司令塔。キュー・シャッフル・リピート・前後の曲・状態の保存を持つ。
//!
//! 画面・SMTC・ホットキー・タスクバー・トレイの操作は、全部ここを通る。
//! audio へは「音を出す指示」だけを送る（audio は次の曲を知らない）。

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::order::{build_order, next_pos, prev_pos, Repeat, Rng};
use super::output_control::{OutRuntime, OutputsView};
use super::outputs::{self, Mode, OutputsDoc};
use super::queue::Queue;
use crate::audio::{AudioEngine, Command, Event, SLOTS};
use crate::db::{self, tracks::Track, Db};
use crate::error::{AppError, ErrorKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub status: Status,
    pub track: Option<Track>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: f32,
    pub muted: bool,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub device_id: Option<String>,
    pub device_name: String,
    pub queue_len: usize,
    pub context_len: usize,
}

/// 画面へ出す知らせ。`code` は ErrorKind の名前か、情報の種類（output_returned など）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub level: &'static str,
    pub code: String,
    pub detail: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

impl Notice {
    pub fn error(e: &AppError) -> Self {
        Notice {
            level: "error",
            code: serde_json::to_value(e.kind)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "other".into()),
            detail: e.detail.clone(),
            from: None,
            to: None,
        }
    }
    pub fn info(code: &str, from: Option<String>, to: Option<String>) -> Self {
        Notice {
            level: "info",
            code: code.into(),
            detail: String::new(),
            from,
            to,
        }
    }
}

pub trait PlaybackListener: Send + Sync {
    fn on_snapshot(&self, _s: &Snapshot) {}
    fn on_position(&self, _position_ms: u64, _duration_ms: u64) {}
    fn on_queue_changed(&self) {}
    fn on_notice(&self, _n: &Notice) {}
    fn on_devices_changed(&self) {}
    /// v3.8.0：出力先の設定・状態・表示のモードが変わった（変わるたびに全部を送る。13.5 の9）
    fn on_outputs(&self, _v: &OutputsView) {}
}

pub(super) struct Inner {
    pub(super) context: Vec<i64>,
    pub(super) order: Vec<usize>,
    pub(super) order_pos: usize,
    pub(super) queue: Queue,
    pub(super) current: Option<Track>,
    pub(super) token: u64,
    pub(super) status: Status,
    pub(super) position_ms: u64,
    pub(super) duration_ms: u64,
    /// 今の表示のモードの、全体の音量（`outputs.master` の今のモードの値と同じ）
    pub(super) volume: f32,
    pub(super) muted: bool,
    pub(super) shuffle: bool,
    pub(super) repeat: Repeat,
    /// v3.8.0：出力先の設定（DB のキー `outputs`。書くのは Rust だけ）と、表示のモード（キー `mode`）
    pub(super) outputs: OutputsDoc,
    pub(super) mode: Mode,
    /// 枠ごとの、音声スレッドが知らせた今の状態
    pub(super) out_rt: [OutRuntime; SLOTS],
    pub(super) rng: Rng,
    /// 再生できない曲が続いたときに止めるための数
    pub(super) consecutive_failures: usize,
    pub(super) last_saved: Instant,
}

pub struct Controller {
    pub(super) db: Arc<Db>,
    pub(super) engine: AudioEngine,
    inner: Mutex<Inner>,
    listeners: Listeners,
}

/// 通知の受け口の一覧。配る間は鍵を持たない：写しを取ってすぐ離し、離してから配る。
/// 受け手には、メインのスレッドを待つもの（トレイのヒントの書き換え）がある。鍵を持ったまま配ると、
/// 起動の途中でメインのスレッドが `add` の鍵を待っている間に、再生のスレッドがメインのスレッドを待って、互いに止まる
/// （v3.8.0 段2 の画面の確認で、起動直後に「応答なし」。3.7.0 からあった形で、段2-1 で起動直後の通知の順番が変わって毎回当たった）
#[derive(Default)]
struct Listeners(RwLock<Vec<Arc<dyn PlaybackListener>>>);

impl Listeners {
    fn add(&self, l: Arc<dyn PlaybackListener>) {
        self.0.write().unwrap_or_else(|p| p.into_inner()).push(l);
    }

    fn each(&self, f: impl Fn(&dyn PlaybackListener)) {
        let list: Vec<Arc<dyn PlaybackListener>> = self.0.read().unwrap_or_else(|p| p.into_inner()).clone();
        for l in &list {
            f(l.as_ref());
        }
    }
}

/// タグを書き換える間、再生中の曲を手放して、あとで同じ位置から戻すための控え
pub struct Resume {
    track_id: i64,
    position_ms: u64,
    was_playing: bool,
}

impl Controller {
    pub fn new(db: Arc<Db>) -> Arc<Self> {
        let saved = db::state::load(&db).unwrap_or_default();
        // v3.8.0：出力先の設定。3.7.0 から上げたときは、今のデバイスを出力先1・オンで入れる（13.5 の9）
        let out_doc = outputs::load_or_migrate(&db, &saved);
        let mode = outputs::load_mode(&db);
        let volume = out_doc.master.get(mode);
        let engine = AudioEngine::start(out_doc.specs(), out_doc.gains(mode, saved.muted));
        let queue_ids = db::queue::load(&db).unwrap_or_default();
        let mut rng = Rng::from_time();
        let ctx = saved.context.clone();
        let ctx_index = (saved.context_index.max(0) as usize).min(ctx.len().saturating_sub(1));
        let order = build_order(ctx.len(), saved.shuffle, Some(ctx_index), &mut rng);
        let order_pos = order.iter().position(|x| *x == ctx_index).unwrap_or(0);
        let c = Arc::new(Self {
            db,
            engine,
            inner: Mutex::new(Inner {
                context: ctx,
                order,
                order_pos,
                queue: Queue::from_ids(queue_ids),
                current: None,
                token: 0,
                status: Status::Stopped,
                position_ms: 0,
                duration_ms: 0,
                volume,
                muted: saved.muted,
                shuffle: saved.shuffle,
                repeat: Repeat::parse(&saved.repeat_mode),
                outputs: out_doc,
                mode,
                out_rt: Default::default(),
                rng,
                consecutive_failures: 0,
                last_saved: Instant::now(),
            }),
            listeners: Listeners::default(),
        });
        // 通知を受けるスレッド
        let weak = Arc::downgrade(&c);
        let events = c.engine.events();
        std::thread::Builder::new()
            .name("pl-playback".into())
            .spawn(move || {
                while let Ok(ev) = events.recv() {
                    let Some(c) = weak.upgrade() else { break };
                    c.on_event(ev);
                }
            })
            .expect("spawn playback thread");
        // 前回の曲を、一時停止で位置だけ戻す（自動では再生しない）
        if let Some(id) = saved.track_id {
            if let Ok(Some(t)) = db::tracks::get(&c.db, id) {
                c.load_track(t, saved.position_ms.max(0) as u64, false);
            }
        }
        c
    }

    pub fn add_listener(&self, l: Arc<dyn PlaybackListener>) {
        self.listeners.add(l);
    }

    pub(super) fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub(super) fn each(&self, f: impl Fn(&dyn PlaybackListener)) {
        self.listeners.each(f);
    }

    fn snapshot_of(inner: &Inner) -> Snapshot {
        Snapshot {
            status: inner.status,
            track: inner.current.clone(),
            position_ms: inner.position_ms,
            duration_ms: inner.duration_ms,
            volume: inner.volume,
            muted: inner.muted,
            shuffle: inner.shuffle,
            repeat: inner.repeat,
            // v3.8.0：出力先1（3.7.0 の設定画面の「再生デバイス」が見る値）
            device_id: inner.outputs.slot(0).and_then(|s| s.device.id.clone()),
            device_name: inner.out_rt[0]
                .name
                .clone()
                .or_else(|| inner.outputs.slot(0).map(|s| s.device.name.clone()))
                .unwrap_or_default(),
            queue_len: inner.queue.len(),
            context_len: inner.context.len(),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Self::snapshot_of(&self.lock())
    }

    pub(super) fn publish(&self) {
        let s = self.snapshot();
        self.each(|l| l.on_snapshot(&s));
    }

    fn publish_queue(&self) {
        let ids = self.lock().queue.ids().to_vec();
        let _ = db::queue::save(&self.db, &ids);
        self.each(|l| l.on_queue_changed());
        self.publish();
    }

    pub fn notice(&self, n: Notice) {
        self.each(|l| l.on_notice(&n));
    }

    // ---------------- 曲を鳴らす ----------------

    fn load_track(&self, t: Track, start_ms: u64, play: bool) {
        let path = PathBuf::from(&t.path);
        {
            let mut i = self.lock();
            i.token += 1;
            let token = i.token;
            i.position_ms = start_ms;
            i.duration_ms = t.duration_ms.max(0) as u64;
            i.status = if play { Status::Playing } else { Status::Paused };
            i.current = Some(t);
            self.engine.send(Command::Load {
                path,
                start_ms,
                play,
                token,
            });
        }
        self.publish();
    }

    fn play_id(&self, id: i64) -> bool {
        match db::tracks::get(&self.db, id) {
            Ok(Some(t)) => {
                let tid = t.id;
                self.load_track(t, 0, true);
                let _ = db::tracks::mark_played(&self.db, tid);
                true
            }
            _ => false,
        }
    }

    /// 曲一覧・プレイリストから再生：その並びを「今流している並び」にする
    pub fn play_context(&self, ids: Vec<i64>, index: usize) {
        if ids.is_empty() {
            return;
        }
        let index = index.min(ids.len() - 1);
        let id = ids[index];
        {
            let mut i = self.lock();
            let shuffle = i.shuffle;
            let len = ids.len();
            let order = build_order(len, shuffle, Some(index), &mut i.rng);
            i.order_pos = order.iter().position(|x| *x == index).unwrap_or(0);
            i.order = order;
            i.context = ids;
            i.consecutive_failures = 0;
        }
        self.play_id(id);
        self.save_state();
    }

    pub fn play(&self) {
        let has = self.lock().current.is_some();
        if !has {
            // 何も選んでいなければ、キューか並びの先頭から
            self.next_inner(false);
            return;
        }
        self.engine.send(Command::Play);
        self.lock().status = Status::Playing;
        self.publish();
    }

    pub fn pause(&self) {
        self.engine.send(Command::Pause);
        {
            let mut i = self.lock();
            if i.status == Status::Playing {
                i.status = Status::Paused;
            }
        }
        self.publish();
        self.save_state();
    }

    pub fn toggle(&self) {
        let st = self.lock().status;
        if st == Status::Playing {
            self.pause()
        } else {
            self.play()
        }
    }

    pub fn next(&self) {
        self.next_inner(false);
    }

    /// 次へ進む。`auto` は曲の終わりで自動で進むとき（1曲リピートが効く）
    fn next_inner(&self, auto: bool) {
        let target: Option<i64> = {
            let mut i = self.lock();
            if auto && i.repeat == Repeat::One {
                i.current.as_ref().map(|t| t.id)
            } else if let Some(q) = i.queue.pop_front() {
                Some(q)
            } else {
                let len = i.context.len();
                if i.current.is_none() && len > 0 {
                    i.order_pos = 0;
                    Some(i.context[i.order[0]])
                } else {
                    match next_pos(i.order_pos, len, i.repeat) {
                        Some(p) => {
                            // 全曲リピートで一周したら、シャッフルなら混ぜ直す
                            if p == 0 && i.shuffle && len > 1 {
                                let new_order = build_order(len, true, None, &mut i.rng);
                                i.order = new_order;
                            }
                            i.order_pos = p;
                            Some(i.context[i.order[p]])
                        }
                        None => None,
                    }
                }
            }
        };
        match target {
            Some(id) => {
                if !self.play_id(id) {
                    self.on_unplayable();
                }
            }
            None => {
                // 最後まで来た：止めて先頭に戻す（自動再生はしない）
                self.engine.send(Command::Stop);
                let first = {
                    let mut i = self.lock();
                    i.status = Status::Stopped;
                    i.position_ms = 0;
                    i.order_pos = 0;
                    i.order.first().and_then(|o| i.context.get(*o).copied())
                };
                if let Some(id) = first {
                    if let Ok(Some(t)) = db::tracks::get(&self.db, id) {
                        self.load_track(t, 0, false);
                        self.lock().status = Status::Stopped;
                    }
                }
                self.publish();
            }
        }
        self.publish_queue();
        self.save_state();
    }

    pub fn previous(&self) {
        let (pos, current) = {
            let i = self.lock();
            (i.position_ms, i.current.as_ref().map(|t| t.id))
        };
        // 3秒より先なら曲の頭へ
        if pos > 3000 || current.is_none() {
            self.seek(0);
            return;
        }
        let target = {
            let mut i = self.lock();
            let len = i.context.len();
            match prev_pos(i.order_pos, len, i.repeat) {
                Some(p) => {
                    i.order_pos = p;
                    Some(i.context[i.order[p]])
                }
                None => None,
            }
        };
        match target {
            Some(id) => {
                if !self.play_id(id) {
                    self.on_unplayable();
                }
            }
            None => self.seek(0),
        }
        self.save_state();
    }

    pub fn seek(&self, ms: u64) {
        self.engine.send(Command::Seek(ms));
        self.lock().position_ms = ms;
        let (p, d) = {
            let i = self.lock();
            (i.position_ms, i.duration_ms)
        };
        self.each(|l| l.on_position(p, d));
    }

    pub fn seek_relative(&self, delta_ms: i64) {
        let (p, d) = {
            let i = self.lock();
            (i.position_ms as i64, i.duration_ms as i64)
        };
        let mut t = (p + delta_ms).max(0);
        if d > 0 {
            t = t.min(d - 500).max(0);
        }
        self.seek(t as u64);
    }

    pub fn set_volume(&self, v: f32) {
        let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { return };
        {
            let mut i = self.lock();
            i.volume = v;
            let mode = i.mode;
            i.outputs.master.set(mode, v);
            // 全体の音量を 0 より上にするとミュートが外れる（3.7.0 と同じ）。出力ごとの音量・モードの切り替えはこの道を通さない
            if v > 0.0 && i.muted {
                i.muted = false;
            }
            self.apply_gains(&i);
        }
        self.publish();
        self.publish_outputs();
    }

    pub fn volume_step(&self, delta: f32) {
        let v = self.lock().volume;
        self.set_volume(v + delta);
        self.commit_volume();
    }

    pub fn set_muted(&self, m: bool) {
        {
            let mut i = self.lock();
            i.muted = m;
            self.apply_gains(&i);
        }
        self.publish();
        self.publish_outputs();
        self.save_state();
    }

    pub fn toggle_mute(&self) {
        let m = self.lock().muted;
        self.set_muted(!m);
    }

    pub fn set_shuffle(&self, on: bool) {
        {
            let mut i = self.lock();
            if i.shuffle == on {
                return;
            }
            i.shuffle = on;
            let len = i.context.len();
            let cur_index = i.order.get(i.order_pos).copied();
            let order = build_order(len, on, cur_index, &mut i.rng);
            i.order_pos = cur_index
                .and_then(|c| order.iter().position(|x| *x == c))
                .unwrap_or(0);
            i.order = order;
        }
        self.publish();
        self.save_state();
    }

    pub fn toggle_shuffle(&self) {
        let s = self.lock().shuffle;
        self.set_shuffle(!s);
    }

    pub fn set_repeat(&self, r: Repeat) {
        self.lock().repeat = r;
        self.publish();
        self.save_state();
    }

    pub fn cycle_repeat(&self) {
        let r = self.lock().repeat;
        self.set_repeat(r.cycle());
    }

    /// 3.7.0 の「再生デバイス」の選び方（設定画面）：出力先1のデバイスを差し替える。出力先1が空なら足してオンにする
    /// `name` は一覧の名前（切断中の行に出すためだけ）
    pub fn set_device(&self, id: Option<String>, name: String) {
        let r = self.update_outputs(|d| {
            let dev = outputs::DeviceRef { id: id.clone(), name };
            if d.slot(0).is_some() {
                d.replace_device(0, dev, db::now_ms())
            } else {
                // 空の出力先1：ほかの枠で使っていれば断る（同じデバイスは1つまで）
                let key = dev.key();
                if d.slots.iter().flatten().any(|s| s.device.key() == key) {
                    return Err(outputs::OpError::Duplicate);
                }
                let mut s = outputs::Slot::fresh(dev);
                s.on = true;
                d.slots[0] = Some(s);
                Ok(())
            }
        });
        if let Err(e) = r {
            self.notice(Notice::error(&e));
        }
        self.save_state();
    }

    pub fn notify_devices_changed(&self) {
        self.engine.notify_devices_changed();
    }

    // ---------------- キュー ----------------

    pub fn queue_ids(&self) -> Vec<i64> {
        self.lock().queue.ids().to_vec()
    }
    pub fn queue_add(&self, ids: &[i64]) {
        self.lock().queue.add(ids);
        self.publish_queue();
    }
    pub fn queue_play_next(&self, ids: &[i64]) {
        self.lock().queue.play_next(ids);
        self.publish_queue();
    }
    pub fn queue_remove(&self, positions: &[usize]) {
        self.lock().queue.remove_at(positions);
        self.publish_queue();
    }
    pub fn queue_move(&self, from: usize, to: usize) {
        self.lock().queue.move_item(from, to);
        self.publish_queue();
    }
    pub fn queue_clear(&self) {
        self.lock().queue.clear();
        self.publish_queue();
    }
    /// キューの n 番目を今すぐ再生（そこまでのキューは飛ばす）
    pub fn queue_play_at(&self, pos: usize) {
        let id = {
            let mut i = self.lock();
            let ids = i.queue.ids().to_vec();
            if pos >= ids.len() {
                return;
            }
            i.queue.remove_at(&(0..=pos).collect::<Vec<_>>());
            ids[pos]
        };
        self.play_id(id);
        self.publish_queue();
    }

    // ---------------- ライブラリの変化 ----------------

    /// ライブラリから外した曲を、並び・キューからも外す
    pub fn on_tracks_removed(&self, removed: &[i64]) {
        let stop = {
            let mut i = self.lock();
            i.queue.retain_existing(|id| !removed.contains(&id));
            let cur = i.current.as_ref().map(|t| t.id);
            let ctx: Vec<i64> = i.context.iter().copied().filter(|id| !removed.contains(id)).collect();
            let cur_idx = cur.and_then(|c| ctx.iter().position(|x| *x == c));
            let shuffle = i.shuffle;
            let len = ctx.len();
            let order = build_order(len, shuffle, cur_idx, &mut i.rng);
            i.order_pos = cur_idx.and_then(|c| order.iter().position(|x| *x == c)).unwrap_or(0);
            i.order = order;
            i.context = ctx;
            cur.map(|c| removed.contains(&c)).unwrap_or(false)
        };
        if stop {
            self.engine.send(Command::Stop);
            let mut i = self.lock();
            i.current = None;
            i.status = Status::Stopped;
            i.position_ms = 0;
        }
        self.publish_queue();
    }

    /// タグを書き換えた曲の表示を更新する
    pub fn refresh_track(&self, id: i64) {
        let is_current = self.lock().current.as_ref().map(|t| t.id) == Some(id);
        if is_current {
            if let Ok(Some(t)) = db::tracks::get(&self.db, id) {
                self.lock().current = Some(t);
                self.publish();
            }
        }
    }

    /// タグを書き換える前に、その曲を鳴らしていれば手放す
    pub fn release_for_write(&self, track_id: i64) -> Option<Resume> {
        let r = {
            let i = self.lock();
            match &i.current {
                Some(t) if t.id == track_id => Some(Resume {
                    track_id,
                    position_ms: i.position_ms,
                    was_playing: i.status == Status::Playing,
                }),
                _ => None,
            }
        };
        if r.is_some() {
            self.engine.send(Command::Stop);
            // 音声スレッドがファイルを閉じるのを待つ（命令は順に処理される）
            std::thread::sleep(Duration::from_millis(60));
        }
        r
    }

    pub fn resume_after_write(&self, r: Resume) {
        if let Ok(Some(t)) = db::tracks::get(&self.db, r.track_id) {
            self.load_track(t, r.position_ms, r.was_playing);
        }
    }

    // ---------------- 音声エンジンからの通知 ----------------

    fn on_unplayable(&self) {
        let (fails, len) = {
            let mut i = self.lock();
            i.consecutive_failures += 1;
            (i.consecutive_failures, i.context.len() + i.queue.len())
        };
        if fails <= len.max(1) && fails < 50 {
            self.next_inner(false);
        } else {
            self.engine.send(Command::Stop);
            self.lock().status = Status::Stopped;
            self.publish();
        }
    }

    fn on_event(&self, ev: Event) {
        match ev {
            Event::Loaded { token, duration_ms } => {
                let mut i = self.lock();
                if token == i.token && duration_ms > 0 {
                    i.duration_ms = duration_ms;
                }
            }
            Event::Position {
                token,
                position_ms,
                duration_ms,
            } => {
                let (save, cur) = {
                    let mut i = self.lock();
                    if token != i.token {
                        return;
                    }
                    i.position_ms = position_ms;
                    if duration_ms > 0 {
                        i.duration_ms = duration_ms;
                    }
                    if i.status == Status::Playing {
                        i.consecutive_failures = 0;
                    }
                    let save = i.last_saved.elapsed() > Duration::from_secs(10);
                    if save {
                        i.last_saved = Instant::now();
                    }
                    (save, i.current.as_ref().map(|t| t.id))
                };
                let d = self.lock().duration_ms;
                self.each(|l| l.on_position(position_ms, d));
                if save {
                    let _ = db::state::save_position(&self.db, cur, position_ms as i64);
                }
            }
            Event::Playing(p) => {
                let changed = {
                    let mut i = self.lock();
                    let new = if p {
                        Status::Playing
                    } else if i.status == Status::Stopped {
                        Status::Stopped
                    } else {
                        Status::Paused
                    };
                    let c = new != i.status && i.current.is_some();
                    if i.current.is_some() {
                        i.status = new;
                    }
                    c
                };
                if changed {
                    self.publish();
                }
            }
            Event::Ended { token } => {
                if token == self.lock().token {
                    self.next_inner(true);
                }
            }
            Event::Error { token, error } => {
                let current_token = self.lock().token;
                let cur_id = self.lock().current.as_ref().map(|t| t.id);
                if error.kind == ErrorKind::FileNotFound {
                    if let Some(id) = cur_id {
                        let _ = db::tracks::set_missing(&self.db, id, true);
                    }
                }
                self.notice(Notice::error(&error));
                if token.is_some() && token == Some(current_token) {
                    match error.kind {
                        ErrorKind::FileNotFound
                        | ErrorKind::AccessDenied
                        | ErrorKind::UnsupportedFormat
                        | ErrorKind::PlaybackFailed => {
                            let playing = self.lock().status == Status::Playing;
                            if playing {
                                self.on_unplayable();
                            } else {
                                self.lock().status = Status::Paused;
                                self.publish();
                            }
                        }
                        ErrorKind::DeviceDisconnected => {
                            self.lock().status = Status::Paused;
                            self.publish();
                        }
                        _ => {}
                    }
                } else if error.kind == ErrorKind::DeviceDisconnected {
                    self.lock().status = Status::Paused;
                    self.publish();
                }
            }
            Event::Output { slot, status, reason, name, resolved_id, kind } => {
                self.on_output_status(slot, status, reason, name, resolved_id, kind);
            }
            Event::OutputReturned { name, .. } => {
                self.notice(Notice::info("output_returned", Some(name), None));
            }
            Event::OutputsAllLost => {
                {
                    let mut i = self.lock();
                    if i.status == Status::Playing {
                        i.status = Status::Paused;
                    }
                }
                self.notice(Notice::info("outputs_all_lost", None, None));
                self.publish();
                self.save_state();
            }
            Event::SyncUnavailable => self.notice(Notice::info("sync_unavailable", None, None)),
            Event::DevicesChanged => self.each(|l| l.on_devices_changed()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Listeners, PlaybackListener};
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::time::Duration;

    /// 受け手が「別のスレッドの add が終わるのを待つ」形でも止まらない（起動直後の「応答なし」の再現。
    /// 実物では、トレイのヒントの書き換えがメインのスレッドを待ち、メインのスレッドは add_listener の鍵を待っていた）
    #[test]
    fn listener_can_wait_for_another_add() {
        struct WaitsForAdd(Arc<Listeners>);
        impl PlaybackListener for WaitsForAdd {
            fn on_devices_changed(&self) {
                let ls = self.0.clone();
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    struct Nop;
                    impl PlaybackListener for Nop {}
                    ls.add(Arc::new(Nop));
                    let _ = tx.send(());
                });
                rx.recv_timeout(Duration::from_secs(5)).expect("add was blocked while listeners were being called");
            }
        }
        let ls = Arc::new(Listeners::default());
        ls.add(Arc::new(WaitsForAdd(ls.clone())));
        let (tx, rx) = mpsc::channel();
        let ls2 = ls.clone();
        std::thread::spawn(move || {
            ls2.each(|l| l.on_devices_changed());
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(10)).expect("each did not finish");
        assert_eq!(ls.0.read().unwrap().len(), 2);
    }
}
