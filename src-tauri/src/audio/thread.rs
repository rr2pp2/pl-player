//! 音声スレッド本体。デコード → 変換 → 輪へ詰める。命令と通知を受けて状態を変える。

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{select, Receiver, Sender};

use super::decoder::Decoder;
use super::engine::{Command, Event, Internal};
use super::output::{Controls, Output};
use super::resample::Converter;
use crate::error::{AppError, ErrorKind};

const POSITION_INTERVAL: Duration = Duration::from_millis(250);
/// 音声スレッドの部品が panic したときの知らせの間隔（v3.1.1 直し1回目）
const PANIC_NOTICE_INTERVAL: Duration = Duration::from_secs(5);

pub(super) fn spawn(
    cmd_rx: Receiver<Command>,
    int_rx: Receiver<Internal>,
    int_tx: Sender<Internal>,
    ev_tx: Sender<Event>,
    controls: Arc<Controls>,
    device: Option<String>,
) {
    std::thread::Builder::new()
        .name("pl-audio".into())
        .spawn(move || {
            let mut t = AudioThread {
                ev: ev_tx,
                int_tx,
                controls,
                desired_device: device,
                output: None,
                track: None,
                last_pos_emit: Instant::now(),
                last_panic_notice: None,
            };
            t.run(cmd_rx, int_rx);
        })
        .expect("spawn audio thread");
}

struct Track {
    path: PathBuf,
    token: u64,
    decoder: Decoder,
    converter: Option<Converter>,
    /// 変換済みで、まだ輪に入っていないサンプル
    pending: Vec<f32>,
    decoded: Vec<f32>,
    eof: bool,
    /// 輪を空にした時点の曲の位置（ミリ秒）
    base_ms: u64,
}

struct AudioThread {
    ev: Sender<Event>,
    int_tx: Sender<Internal>,
    controls: Arc<Controls>,
    desired_device: Option<String>,
    output: Option<Output>,
    track: Option<Track>,
    last_pos_emit: Instant,
    /// 部品が panic したことを最後に知らせた token と時刻（v3.1.1 直し1回目：同じ token で続けて起きるときは知らせを間引く）
    last_panic_notice: Option<(Option<u64>, Instant)>,
}

impl AudioThread {
    fn run(&mut self, cmd_rx: Receiver<Command>, int_rx: Receiver<Internal>) {
        loop {
            let timeout = if self.needs_fill() {
                Duration::from_millis(0)
            } else {
                Duration::from_millis(10)
            };
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
            self.guarded(token, |t| {
                t.fill();
                t.check_end();
                t.emit_position(false);
            });
        }
        self.output = None;
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
        self.controls.paused.store(true, Ordering::Relaxed);
        if let Some(o) = &self.output {
            o.flush();
        }
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

    fn playing(&self) -> bool {
        !self.controls.paused.load(Ordering::Relaxed)
    }

    fn set_playing(&mut self, p: bool) {
        self.controls.paused.store(!p, Ordering::Relaxed);
        let _ = self.ev.send(Event::Playing(p));
    }

    fn position_ms(&self) -> u64 {
        match (&self.track, &self.output) {
            (Some(t), Some(o)) => t.base_ms + o.played_frames() * 1000 / o.rate.max(1) as u64,
            (Some(t), None) => t.base_ms,
            _ => 0,
        }
    }

    fn emit(&self, e: Event) {
        let _ = self.ev.send(e);
    }

    fn emit_error(&self, token: Option<u64>, error: AppError) {
        tracing::warn!("audio error: {error}");
        self.emit(Event::Error { token, error });
    }

    fn emit_position(&mut self, force: bool) {
        if !force && (!self.playing() || self.last_pos_emit.elapsed() < POSITION_INTERVAL) {
            return;
        }
        if let Some(t) = &self.track {
            let pos = self.position_ms();
            let dur = t.decoder.duration_ms;
            self.emit(Event::Position {
                token: t.token,
                position_ms: if dur > 0 { pos.min(dur) } else { pos },
                duration_ms: dur,
            });
        }
        self.last_pos_emit = Instant::now();
    }

    fn emit_device(&self, fallback_from: Option<String>) {
        if let Some(o) = &self.output {
            self.emit(Event::DeviceChanged {
                device_id: o.device_id.clone(),
                name: o.device_name.clone(),
                fallback_from,
            });
        }
    }

    /// 出力を開く（無ければ）。開けなければエラーを知らせて false
    fn ensure_output(&mut self, token: Option<u64>) -> bool {
        if self.output.is_some() {
            return true;
        }
        match Output::open(
            self.desired_device.as_deref(),
            self.controls.clone(),
            self.int_tx.clone(),
        ) {
            Ok(o) => {
                self.output = Some(o);
                self.emit_device(None);
                true
            }
            Err(e) if self.desired_device.is_some() => {
                // 選んでいたデバイスが無い → 既定のデバイスで続ける
                let from = self.desired_device.clone();
                self.desired_device = None;
                match Output::open(None, self.controls.clone(), self.int_tx.clone()) {
                    Ok(o) => {
                        self.output = Some(o);
                        self.emit_device(from.or(Some(e.detail)));
                        true
                    }
                    Err(e2) => {
                        self.emit_error(token, e2);
                        false
                    }
                }
            }
            Err(e) => {
                self.emit_error(token, e);
                false
            }
        }
    }

    fn handle(&mut self, c: Command) {
        match c {
            Command::Load {
                path,
                start_ms,
                play,
                token,
            } => self.load(path, start_ms, play, token),
            Command::Play => {
                if self.track.is_some() {
                    if self.ensure_output(self.track.as_ref().map(|t| t.token)) {
                        self.set_playing(true);
                    }
                }
            }
            Command::Pause => {
                self.set_playing(false);
                self.emit_position(true);
            }
            Command::Stop => {
                self.set_playing(false);
                self.track = None;
                if let Some(o) = &self.output {
                    o.flush();
                }
            }
            Command::Seek(ms) => self.seek(ms),
            Command::SetDevice(id) => {
                self.desired_device = id.clone();
                if self.output.is_some() {
                    if let Err(e) = self.reopen(id) {
                        self.emit_error(None, e);
                        // 選んだ先が開けない → 既定へ
                        if let Err(e2) = self.reopen(None) {
                            self.emit_error(None, e2);
                            self.set_playing(false);
                        } else {
                            self.emit_device(None);
                        }
                    } else {
                        self.emit_device(None);
                    }
                } else if self.track.is_some() {
                    self.ensure_output(None);
                } else {
                    // 何も鳴らしていない：開いて名前だけ確かめる
                    if self.ensure_output(None) {
                        // 鳴らすものが無いのでデバイスは掴んだままにしない
                        self.output = None;
                    }
                }
            }
            Command::Shutdown => {}
            #[cfg(test)]
            Command::TestPanic => panic!("test panic in the audio thread"),
        }
    }

    fn handle_internal(&mut self, i: Internal) {
        match i {
            Internal::StreamError(kind, msg) => match kind {
                cpal::ErrorKind::DeviceNotAvailable
                | cpal::ErrorKind::StreamInvalidated
                | cpal::ErrorKind::DeviceChanged => {
                    let from = self.output.as_ref().map(|o| o.device_name.clone());
                    let was_specific = self.output.as_ref().and_then(|o| o.device_id.clone()).is_some();
                    let changed_only = kind == cpal::ErrorKind::DeviceChanged;
                    if was_specific && !changed_only {
                        self.desired_device = None;
                    }
                    let target = self.desired_device.clone();
                    match self.reopen(target) {
                        Ok(()) => {
                            if changed_only {
                                self.emit_device(None);
                            } else {
                                self.emit_device(from);
                            }
                        }
                        Err(e) => {
                            // 既定のデバイスも無い → 一時停止して知らせる
                            self.output = None;
                            self.set_playing(false);
                            self.emit_error(
                                self.track.as_ref().map(|t| t.token),
                                AppError::new(ErrorKind::DeviceDisconnected, format!("{msg} / {}", e.detail)),
                            );
                        }
                    }
                    self.emit(Event::DevicesChanged);
                }
                cpal::ErrorKind::Xrun => {}
                _ => tracing::warn!("stream error: {kind:?} {msg}"),
            },
            Internal::DevicesChanged => self.emit(Event::DevicesChanged),
        }
    }

    /// 出力だけを作り直し、曲と位置を保つ
    fn reopen(&mut self, device: Option<String>) -> Result<(), AppError> {
        let pos = self.position_ms();
        self.output = None; // 先に古いストリームを止める
        let out = Output::open(device.as_deref(), self.controls.clone(), self.int_tx.clone())?;
        let (rate, ch) = (out.rate, out.channels);
        self.output = Some(out);
        if let Some(t) = self.track.as_mut() {
            match t.decoder.seek(pos) {
                Ok(p) => t.base_ms = p,
                Err(_) => t.base_ms = pos,
            }
            t.converter = Some(Converter::new(t.decoder.sample_rate, t.decoder.channels, rate, ch));
            t.pending.clear();
            t.eof = false;
        }
        Ok(())
    }

    fn load(&mut self, path: PathBuf, start_ms: u64, play: bool, token: u64) {
        self.set_playing(false);
        if let Some(o) = &self.output {
            o.flush();
        }
        self.track = None;
        let mut decoder = match Decoder::open(&path) {
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
        let duration_ms = decoder.duration_ms;
        self.track = Some(Track {
            path,
            token,
            decoder,
            converter: None,
            pending: Vec::new(),
            decoded: Vec::new(),
            eof: false,
            base_ms: base,
        });
        self.emit(Event::Loaded { token, duration_ms });
        if !self.ensure_output(Some(token)) {
            return;
        }
        if let Some(o) = &self.output {
            o.flush();
        }
        if play {
            self.set_playing(true);
        }
        self.emit_position(true);
    }

    fn seek(&mut self, ms: u64) {
        let Some(t) = self.track.as_mut() else { return };
        let ms = if t.decoder.duration_ms > 0 {
            ms.min(t.decoder.duration_ms.saturating_sub(1))
        } else {
            ms
        };
        match t.decoder.seek(ms) {
            Ok(p) => {
                t.base_ms = p;
                t.pending.clear();
                t.eof = false;
                if let Some(c) = t.converter.as_mut() {
                    c.reset();
                }
                if let Some(o) = &self.output {
                    o.flush();
                }
            }
            Err(e) => {
                let token = t.token;
                self.emit_error(Some(token), e);
            }
        }
        self.emit_position(true);
    }

    fn needs_fill(&self) -> bool {
        match (&self.track, &self.output) {
            (Some(t), Some(o)) => {
                (!t.eof || !t.pending.is_empty()) && o.free() > o.channels * 1024
            }
            _ => false,
        }
    }

    fn fill(&mut self) {
        let (Some(t), Some(o)) = (self.track.as_mut(), self.output.as_mut()) else {
            return;
        };
        if let Err((token, path, e)) = fill_track(t, o) {
            tracing::warn!("decode error in {}: {e}", path.display());
            self.track = None;
            self.controls.paused.store(true, Ordering::Relaxed);
            let _ = self.ev.send(Event::Error {
                token: Some(token),
                error: e,
            });
            let _ = self.ev.send(Event::Playing(false));
        }
    }

    fn check_end(&mut self) {
        let ended = match (&self.track, &self.output) {
            (Some(t), Some(o)) => t.eof && t.pending.is_empty() && o.buffered() == 0 && self.playing(),
            _ => false,
        };
        if ended {
            let token = self.track.as_ref().map(|t| t.token).unwrap_or(0);
            self.emit_position(true);
            self.track = None;
            self.controls.paused.store(true, Ordering::Relaxed);
            self.emit(Event::Ended { token });
        }
    }
}

/// 輪に詰める。デコードに失敗したら (token, パス, エラー) を返す
fn fill_track(t: &mut Track, o: &mut Output) -> Result<(), (u64, PathBuf, AppError)> {
    let (rate, ch) = (o.rate, o.channels);
    // 1回の呼び出しで詰めすぎない（命令への反応を遅らせないため）
    for _ in 0..16 {
        if !t.pending.is_empty() {
            let n = o.push(&t.pending);
            t.pending.drain(..n);
            if !t.pending.is_empty() {
                return Ok(()); // 輪がいっぱい
            }
        }
        if t.eof || o.free() < ch * 256 {
            return Ok(());
        }
        let need_new = t
            .converter
            .as_ref()
            .map(|c| !c.matches(t.decoder.sample_rate, t.decoder.channels, rate, ch))
            .unwrap_or(true);
        if need_new {
            t.converter = Some(Converter::new(t.decoder.sample_rate, t.decoder.channels, rate, ch));
        }
        t.decoded.clear();
        match t.decoder.decode_next(&mut t.decoded) {
            Ok(more) => {
                if let Some(conv) = t.converter.as_mut() {
                    conv.process(&t.decoded, &mut t.pending, !more);
                }
                if !more {
                    t.eof = true;
                }
            }
            Err(e) => {
                t.eof = true;
                t.pending.clear();
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
    /// スレッドは止まらず、次の命令を受ける。どの場合も出力デバイスは開かない（開く前に失敗する）
    #[test]
    fn audio_thread_survives_zero_rate_and_panics() {
        let dir = crate::test_tmp().join(format!("plp-audio-thread-{}", std::process::id()));
        let wav = super::super::decoder::test_zero_rate_wav(&dir);
        let engine = AudioEngine::start(0.0, true, None);
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
        let (int_tx, _int_rx) = crossbeam_channel::unbounded();
        let mut t = super::AudioThread {
            ev: ev_tx,
            int_tx,
            controls: std::sync::Arc::new(super::super::output::Controls::new(0.0, true)),
            desired_device: None,
            output: None,
            track: None,
            last_pos_emit: std::time::Instant::now(),
            last_panic_notice: None,
        };
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
}
