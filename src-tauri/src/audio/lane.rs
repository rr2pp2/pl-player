//! 出力1本ぶんの流し込み（v3.8.0。仕様 13.1 の4〜6）。音声スレッドの中だけで使う。
//!
//! 曲の側の輪から「時計 − 遅らせ量」の位置を読み、倍率を動かせる変換器（`StretchConverter`）を通して、
//! 出力の輪へ書く。書くフレームが鳴る時刻（`StreamTiming` の当てはめ）と、そこで鳴るべき曲の位置（時計）を比べ、
//! 約100ms ごとに倍率を決め直す（速さの項＋位置の項の PI 制御。不感帯 ±1ms）。ずれが 20ms を超えたら入り直す。

use super::output::OutputStream;
use super::resample::{spread_stereo, StretchConverter};
use super::song_ring::{SongRing, RING_CHANNELS};
use super::sync::{follower_ratio, ms_to_ns, tuning, Clock, Gate, Pi, StreamTiming};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// 開いて無音を流しながら、時刻を測っている（約200ms）
    Warming,
    /// 開いていて、門を開ける（鳴らすものが無い・一時停止中）
    Idle,
    /// 輪を空にしてもらっている
    Flushing,
    /// 門を開いて鳴らしている
    Running,
}

/// 倍率の決め直しの結果
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Control {
    /// まだ（間隔の前・記録が足りない）
    Skip,
    /// 倍率を決めた（ずれ ms）
    Adjusted(f64),
    /// ずれが大きいので入り直す（ずれ ms）
    Reenter(f64),
}

pub struct Lane {
    pub stream: OutputStream,
    pub timing: StreamTiming,
    pub phase: Phase,
    conv: Option<StretchConverter>,
    song_rate: u32,
    /// 次に変換器へ入れる曲のフレーム
    feed_pos: i64,
    /// 次に出力の輪へ書くフレーム（出力のフレーム番号）
    w_abs: u64,
    /// `w_abs` のフレームで鳴る曲の位置
    next_out_pos: f64,
    skip_out: usize,
    pi: Pi,
    last_control_ns: u64,
    /// 門を開いたときの遅らせ量（曲のフレーム）
    delay_frames: f64,
    /// 門を開いた時刻（QPC ns）。これより前は、まだ鳴っていない
    gate_t_ns: f64,
    inbuf: Vec<f32>,
    outbuf: Vec<f32>,
    pushbuf: Vec<f32>,
}

impl Lane {
    pub fn new(stream: OutputStream) -> Self {
        let rate = stream.rate;
        Self {
            stream,
            timing: StreamTiming::new(rate),
            phase: Phase::Warming,
            conv: None,
            song_rate: 0,
            feed_pos: 0,
            w_abs: 0,
            next_out_pos: 0.0,
            skip_out: 0,
            pi: Pi::default(),
            last_control_ns: 0,
            delay_frames: 0.0,
            gate_t_ns: 0.0,
            inbuf: Vec::new(),
            outbuf: Vec::new(),
            pushbuf: Vec::new(),
        }
    }

    /// コールバックの時刻の記録を取り込み、温まったら Idle へ。輪を空にし終えたら Idle へ
    pub fn poll(&mut self) {
        let t = &mut self.timing;
        self.stream.drain_timing(|r| t.add(r.frames_before, r.callback_ns, r.playback_ns));
        match self.phase {
            // 温まる途中で空にする要求を出していたら、それが済むまで待つ（済む前に書くと、書いた分まで空にされる）
            Phase::Warming if self.timing.ready() && self.stream.flush_done() => self.phase = Phase::Idle,
            Phase::Flushing if self.stream.flush_done() => {
                self.phase = if self.timing.ready() { Phase::Idle } else { Phase::Warming }
            }
            _ => {}
        }
    }

    /// 鳴らすのをやめ、輪を空にしてもらう（一時停止・シーク・曲の切り替え・入り直し）
    pub fn flush(&mut self) {
        if matches!(self.phase, Phase::Running | Phase::Idle) {
            self.stream.request_flush();
            self.phase = Phase::Flushing;
        } else if self.phase == Phase::Warming {
            // 温まる途中でも、前の世代の予定は消す
            self.stream.request_flush();
        }
    }

    /// 時刻が飛んだ（スリープ明けなど）：測り直す
    pub fn rewarm(&mut self) {
        self.flush();
        self.timing.reset();
        if self.phase != Phase::Flushing {
            self.phase = Phase::Warming;
        }
    }

    /// 門を開く（13.1 の6）。`gate.k_start` のフレームから、曲の `gate.song_start` を鳴らす
    pub fn arm(&mut self, gate: Gate, song_rate: u32, delay_frames: f64, now_ns: u64) -> bool {
        if self.phase != Phase::Idle {
            return false;
        }
        let out_rate = self.stream.rate;
        let reuse = self.conv.as_ref().is_some_and(|c| c.matches(song_rate, out_rate));
        if reuse {
            if let Some(c) = self.conv.as_mut() {
                c.reset();
            }
        } else {
            self.conv = StretchConverter::new(song_rate, out_rate);
        }
        let Some(conv) = self.conv.as_ref() else {
            return false;
        };
        self.song_rate = song_rate;
        self.skip_out = conv.output_delay();
        self.feed_pos = gate.song_start.round() as i64;
        self.next_out_pos = self.feed_pos as f64;
        self.w_abs = gate.k_start;
        self.delay_frames = delay_frames;
        self.gate_t_ns = gate.t_start_ns;
        self.pi.reset();
        self.last_control_ns = now_ns;
        self.stream.set_base(gate.k_start);
        self.phase = Phase::Running;
        true
    }

    pub fn ratio(&self) -> f64 {
        self.conv.as_ref().map(|c| c.ratio()).unwrap_or(1.0)
    }

    /// 次に読む曲のフレーム（これより前は、この出力はもう読まない）
    pub fn feed_pos(&self) -> Option<i64> {
        (self.phase == Phase::Running).then_some(self.feed_pos)
    }

    /// 曲の側の輪から読んで、出力の輪へ書けるだけ書く
    pub fn fill(&mut self, ring: &SongRing) {
        if self.phase != Phase::Running || ring.rate() != self.song_rate {
            return;
        }
        let ch = self.stream.channels.max(1);
        let Some(conv) = self.conv.as_mut() else { return };
        // 1回の呼び出しで書きすぎない（命令への反応を遅らせないため）
        for _ in 0..16 {
            if self.stream.free_frames() < conv.output_frames_max() + 1 {
                return;
            }
            let need = conv.input_frames_next();
            if !ring.available(self.feed_pos, need) {
                return;
            }
            ring.read(self.feed_pos, need, &mut self.inbuf);
            self.outbuf.clear();
            let ratio = conv.ratio();
            let (used, produced) = conv.process(&self.inbuf, &mut self.outbuf);
            if used == 0 {
                return;
            }
            self.feed_pos += used as i64;
            let skip = self.skip_out.min(produced);
            self.skip_out -= skip;
            let frames = produced - skip;
            if frames == 0 {
                continue;
            }
            self.pushbuf.clear();
            spread_stereo(&self.outbuf[skip * RING_CHANNELS..produced * RING_CHANNELS], ch, &mut self.pushbuf);
            let pushed = self.stream.push(&self.pushbuf) / ch;
            self.w_abs += pushed as u64;
            self.next_out_pos += pushed as f64 / ratio;
        }
    }

    /// 門を開いた時刻を過ぎて、鳴っている
    pub fn started(&self, now_ns: u64) -> bool {
        self.phase == Phase::Running && now_ns as f64 >= self.gate_t_ns
    }

    /// 時刻 `t_ns` にこの出力で鳴っている曲の位置（基準の出力が時計を決めるときに使う）。門より前は None
    pub fn song_pos_at(&self, t_ns: f64) -> Option<f64> {
        if self.phase != Phase::Running || t_ns < self.gate_t_ns {
            return None;
        }
        let k = self.timing.frame_at(t_ns)?;
        Some(self.next_out_pos - (self.w_abs as f64 - k) / self.ratio())
    }

    /// 基準の出力の時計の速さ（曲のフレーム / ns）
    pub fn song_speed(&self) -> f64 {
        1.0 / (self.ratio() * self.timing.ns_per_frame())
    }

    /// 基準になった：伸び縮みをやめる（名目の倍率）
    pub fn become_reference(&mut self) {
        if let Some(c) = self.conv.as_mut() {
            let n = c.nominal();
            c.set_ratio(n);
        }
        self.pi.reset();
    }

    /// 追う側：約100ms ごとに倍率を決め直す（13.1 の4）
    pub fn control(&mut self, now_ns: u64, clock: &Clock) -> Control {
        if self.phase != Phase::Running || !clock.running() {
            return Control::Skip;
        }
        let dt_ns = now_ns.saturating_sub(self.last_control_ns);
        if dt_ns < ms_to_ns(tuning::CONTROL_INTERVAL_MS) {
            return Control::Skip;
        }
        self.last_control_ns = now_ns;
        let Some(t_w) = self.timing.time_of_frame(self.w_abs as f64) else {
            return Control::Skip;
        };
        let target = clock.pos_at(t_w) - self.delay_frames;
        let err_ms = (self.next_out_pos - target) * 1000.0 / self.song_rate.max(1) as f64;
        if err_ms.abs() > tuning::REENTRY_MS {
            return Control::Reenter(err_ms);
        }
        let adj = self.pi.update(err_ms, dt_ns as f64 / 1e9);
        let ns_per_frame = self.timing.ns_per_frame();
        if let Some(c) = self.conv.as_mut() {
            let r = follower_ratio(clock.speed(), ns_per_frame, adj, c.nominal());
            c.set_ratio(r);
        }
        Control::Adjusted(err_ms)
    }
}
