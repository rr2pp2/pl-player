//! 遅れを測る：デバイスを開いてクリックを鳴らし、録って、届いた時刻を出す（音が出る部分）。
//!
//! 呼ぶのは専用のスレッド（`commands\latency.rs` が `spawn_blocking` で走らせる）。画面のスレッドも音声スレッドも止めない。
//! 本体の出力（音声スレッドが持つ流れ）には触らず、測る専用の流れを共有モードで別に開く（同じデバイスでも開ける）。
//!
//! デバイスを開く処理の決まり（段2-1 の検査で出た穴を最初から塞ぐ）：
//! - 開くのは出力ごとの「開く役」のスレッド。待つのは `OPEN_TIMEOUT_MS` まで。過ぎたら「時間がかかりすぎました」で、その出力だけ外す
//! - 開く役の中で panic しても、結果の通り道が切れるので「開けませんでした」になる（無音のまま待ち続けない）
//! - 待つのをやめたあとに開けた流れは、受け取り手がいないので開く役の中で落とす（デバイスを握ったままにしない）
//! - コールバックの中ではロック・メモリの確保・ログ出力をしない（原子変数と rtrb の輪だけ）
//! - 流れのエラー（抜けた・形式が変わった）は原子変数で知らせ、その出力だけ「止まりました」
//! - 途中でやめる（やめる・曲を流し始めた）と、開いた流れは全部この関数を抜けるときに落ちる

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, I24};
use crossbeam_channel::{bounded, Receiver, TryRecvError};
use rtrb::{Consumer, Producer, RingBuffer};

use super::detect::{click_burst, find_click_onset, CapPacket, CaptureFeed, CaptureTrack, HitMiss, Onset, WindowMiss, CLICK_FADE_MS};
use super::{summarize, tuning, Abort, CaptureSource, FailCode, Measured, Miss, Options, Target};
use crate::audio::devices;
use crate::audio::output::Timing;
use crate::audio::sync::{ms_to_ns, qpc_now_ns, tuning as sync_tuning, StreamTiming};

const MAX_CLICKS: usize = tuning::CLICK_COUNT;

/// 狙ったクリックのフレーム（測る役が書き、出力のコールバックが読む）
struct ClickPlan {
    starts: [AtomicU64; MAX_CLICKS],
    /// 書き終えた数（Release で出す。コールバックは Acquire で読んでから starts を読む）
    count: AtomicUsize,
    /// 鳴らすのが間に合わなかった回（ビット）
    late: AtomicU32,
}

impl ClickPlan {
    fn new() -> Self {
        Self { starts: std::array::from_fn(|_| AtomicU64::new(u64::MAX)), count: AtomicUsize::new(0), late: AtomicU32::new(0) }
    }
}

struct OutSide {
    stream: cpal::Stream,
    timing: Consumer<Timing>,
    plan: Arc<ClickPlan>,
    rate: u32,
    fault: Arc<AtomicBool>,
}

struct CapSide {
    stream: cpal::Stream,
    samples: Consumer<f32>,
    packets: Consumer<CapPacket>,
    rate: u32,
    fault: Arc<AtomicBool>,
    /// 輪がいっぱいで捨てたパケットの数（記録用）
    dropped: Arc<AtomicU64>,
}

/// 出力1つぶんの、開いた流れ（鳴らす側と録る側）
struct Pair {
    out: OutSide,
    cap: CapSide,
}

fn instant_ns(i: cpal::StreamInstant) -> u64 {
    i.as_nanos().min(u64::MAX as u128) as u64
}

fn out_fail(kind: cpal::ErrorKind) -> FailCode {
    match kind {
        cpal::ErrorKind::DeviceNotAvailable => FailCode::OutputMissing,
        cpal::ErrorKind::DeviceBusy => FailCode::OutputBusy,
        cpal::ErrorKind::UnsupportedConfig => FailCode::OutputFormat,
        _ => FailCode::OutputOpen,
    }
}

fn cap_fail(kind: cpal::ErrorKind) -> FailCode {
    match kind {
        cpal::ErrorKind::DeviceNotAvailable => FailCode::CaptureMissing,
        cpal::ErrorKind::DeviceBusy => FailCode::CaptureBusy,
        cpal::ErrorKind::UnsupportedConfig => FailCode::CaptureFormat,
        cpal::ErrorKind::PermissionDenied => FailCode::CaptureDenied,
        _ => FailCode::CaptureOpen,
    }
}

/// 流れが止まったとみなすエラー（抜けた・作り直しが要る）。Xrun などは測るのを続ける
fn is_fatal(kind: cpal::ErrorKind) -> bool {
    matches!(kind, cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::StreamInvalidated)
}

/// 自前の QPC と流れの時刻がそろっているか（13.1 の2。そろわなければ時刻を比べられない）
fn clock_ok(stream: &cpal::Stream) -> bool {
    let q1 = qpc_now_ns();
    let now = instant_ns(stream.now());
    let q2 = qpc_now_ns();
    let diff = (q1 / 2 + q2 / 2) as i64 - now as i64;
    (diff.unsigned_abs() as f64) < sync_tuning::QPC_STREAM_TOLERANCE_MS * 1e6
}

fn open_output(t: &Target) -> Result<OutSide, FailCode> {
    let device = devices::find_output(t.output_id.as_deref()).ok_or(FailCode::OutputMissing)?;
    let sup = device.default_output_config().map_err(|e| out_fail(e.kind()))?;
    let rate = sup.sample_rate();
    let channels = sup.channels() as usize;
    if rate == 0 || channels == 0 {
        return Err(FailCode::OutputFormat);
    }
    // 本体の出力と同じく、既定の形式のまま開く（Windows の中で変換を挟ませない。13.5 の7）
    let config = cpal::StreamConfig { channels: sup.channels(), sample_rate: rate, buffer_size: cpal::BufferSize::Default };
    let amp = 10f64.powf(tuning::CLICK_DB / 20.0) as f32;
    let burst = click_burst(t.freq_hz, rate, tuning::CLICK_MS, CLICK_FADE_MS, amp);
    let plan = Arc::new(ClickPlan::new());
    let (tp, tc) = RingBuffer::<Timing>::new(4096);
    let fault = Arc::new(AtomicBool::new(false));
    let parts = OutParts { channels, burst, plan: plan.clone(), timing: tp, fault: fault.clone() };
    let stream = match sup.sample_format() {
        SampleFormat::F32 => build_out::<f32>(&device, &config, parts),
        SampleFormat::F64 => build_out::<f64>(&device, &config, parts),
        SampleFormat::I16 => build_out::<i16>(&device, &config, parts),
        SampleFormat::I24 => build_out::<I24>(&device, &config, parts),
        SampleFormat::I32 => build_out::<i32>(&device, &config, parts),
        SampleFormat::I64 => build_out::<i64>(&device, &config, parts),
        SampleFormat::U8 => build_out::<u8>(&device, &config, parts),
        _ => Err(FailCode::OutputFormat),
    }?;
    stream.play().map_err(|e| out_fail(e.kind()))?;
    if !clock_ok(&stream) {
        return Err(FailCode::Clock);
    }
    Ok(OutSide { stream, timing: tc, plan, rate, fault })
}

struct OutParts {
    channels: usize,
    burst: Vec<f32>,
    plan: Arc<ClickPlan>,
    timing: Producer<Timing>,
    fault: Arc<AtomicBool>,
}

/// 1回のコールバック（フレーム p から n フレーム）で、狙ったクリックを書く。`put(バッファの中のフレーム, 値)`。
/// コールバックの中で呼ぶので、ロック・メモリの確保をしない（原子変数を読むだけ）。
/// 頭がもう過ぎていた回（狙いを決めるのが間に合わなかった）は、途中から鳴らさずに「間に合わなかった」の印だけ付ける。
/// 検査3回目の指摘：この判定を「もう全部過ぎた回は飛ばす」より先に置く。前は、狙いを決めたときにクリックの終わりまで過ぎていた回が
/// 印の付かないまま飛ばされ、録音で探されて「音が無い」になっていた（本当の理由の「間に合わなかった」が出ない）
fn render_clicks(p: u64, n: u64, plan: &ClickPlan, burst: &[f32], begun: &mut [bool; MAX_CLICKS], put: &mut dyn FnMut(usize, f32)) {
    let len = burst.len() as u64;
    let count = plan.count.load(Ordering::Acquire).min(MAX_CLICKS);
    for k in 0..count {
        let s = plan.starts[k].load(Ordering::Relaxed);
        if !begun[k] {
            if s < p {
                plan.late.fetch_or(1 << k, Ordering::Relaxed);
                continue;
            }
            if s >= p + n {
                continue;
            }
            begun[k] = true;
        }
        let end = s.saturating_add(len);
        for f in s.max(p)..end.min(p + n) {
            put((f - p) as usize, burst[(f - s) as usize]);
        }
    }
}

fn build_out<T>(device: &cpal::Device, config: &cpal::StreamConfig, parts: OutParts) -> Result<cpal::Stream, FailCode>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let OutParts { channels, burst, plan, mut timing, fault } = parts;
    let ch = channels.max(1);
    let mut written = 0u64;
    let mut begun = [false; MAX_CLICKS];
    let data_cb = move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
        let n = (data.len() / ch) as u64;
        let ts = info.timestamp();
        let _ = timing.push(Timing { frames_before: written, callback_ns: instant_ns(ts.callback), playback_ns: instant_ns(ts.playback) });
        data.fill(T::from_sample(0.0f32));
        let p = written;
        written += n;
        render_clicks(p, n, &plan, &burst, &mut begun, &mut |j, x| {
            let v = T::from_sample(x.clamp(-1.0, 1.0));
            let i = j * ch;
            if let Some(frame) = data.get_mut(i..i + ch) {
                frame.fill(v);
            }
        });
    };
    let err_cb = move |e: cpal::Error| {
        if is_fatal(e.kind()) {
            fault.store(true, Ordering::Relaxed);
        }
    };
    device.build_output_stream::<T, _, _>(*config, data_cb, err_cb, None).map_err(|e| out_fail(e.kind()))
}

fn open_capture(t: &Target) -> Result<CapSide, FailCode> {
    let (device, sup) = match &t.capture {
        // ループバック：出力デバイスに入力の流れを開く（cpal が AUDCLNT_STREAMFLAGS_LOOPBACK を付ける）。形式は出力の既定の形式
        CaptureSource::Loopback => {
            let d = devices::find_output(t.output_id.as_deref()).ok_or(FailCode::OutputMissing)?;
            let c = d.default_output_config().map_err(|e| cap_fail(e.kind()))?;
            (d, c)
        }
        CaptureSource::Input { id } => {
            let d = devices::find_input(id).ok_or(FailCode::CaptureMissing)?;
            let c = d.default_input_config().map_err(|e| cap_fail(e.kind()))?;
            (d, c)
        }
    };
    let rate = sup.sample_rate();
    let channels = sup.channels() as usize;
    if rate == 0 || channels == 0 {
        return Err(FailCode::CaptureFormat);
    }
    let config = cpal::StreamConfig { channels: sup.channels(), sample_rate: rate, buffer_size: cpal::BufferSize::Default };
    // 2秒ぶん（測る役は 15ms ごとに取り出す）
    let (sp, sc) = RingBuffer::<f32>::new((rate as usize * 2).max(1 << 16));
    let (pp, pc) = RingBuffer::<CapPacket>::new(4096);
    let fault = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicU64::new(0));
    let feed = CaptureFeed { samples: sp, packets: pp, pos: 0, stored: 0, dropped: dropped.clone() };
    let stream = match sup.sample_format() {
        SampleFormat::F32 => build_cap::<f32>(&device, &config, channels, feed, fault.clone()),
        SampleFormat::F64 => build_cap::<f64>(&device, &config, channels, feed, fault.clone()),
        SampleFormat::I16 => build_cap::<i16>(&device, &config, channels, feed, fault.clone()),
        SampleFormat::I24 => build_cap::<I24>(&device, &config, channels, feed, fault.clone()),
        SampleFormat::I32 => build_cap::<i32>(&device, &config, channels, feed, fault.clone()),
        SampleFormat::I64 => build_cap::<i64>(&device, &config, channels, feed, fault.clone()),
        SampleFormat::U8 => build_cap::<u8>(&device, &config, channels, feed, fault.clone()),
        _ => Err(FailCode::CaptureFormat),
    }?;
    stream.play().map_err(|e| cap_fail(e.kind()))?;
    if !clock_ok(&stream) {
        return Err(FailCode::Clock);
    }
    Ok(CapSide { stream, samples: sc, packets: pc, rate, fault, dropped })
}

fn build_cap<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    mut feed: CaptureFeed,
    fault: Arc<AtomicBool>,
) -> Result<cpal::Stream, FailCode>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let ch = channels.max(1);
    let data_cb = move |data: &[T], info: &cpal::InputCallbackInfo| {
        let frames = data.len() / ch;
        let ts = info.timestamp();
        let mono = data.chunks_exact(ch).map(|f| f.iter().map(|&v| <f32 as FromSample<T>>::from_sample_(v)).sum::<f32>() / ch as f32);
        feed.push(frames, mono, instant_ns(ts.capture));
    };
    let err_cb = move |e: cpal::Error| {
        if is_fatal(e.kind()) {
            fault.store(true, Ordering::Relaxed);
        }
    };
    device.build_input_stream::<T, _, _>(*config, data_cb, err_cb, None).map_err(|e| cap_fail(e.kind()))
}

fn open_pair(t: &Target) -> Result<Pair, FailCode> {
    let out = open_output(t)?;
    let cap = open_capture(t)?;
    Ok(Pair { out, cap })
}

/// クリック1回の記録（診断用。機器の名前は持たない）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClickDiag {
    /// 狙いを決めたとき、その回の時刻まで何 ms 先だったか（大きいほど先の見積もり）
    pub horizon_ms: f64,
    /// 取れた：届くまでの遅れ（ms）・大きさ（dBFS）・雑音との比（dB）。取れなかった：理由
    pub result: Result<(f64, f64, f64), Miss>,
}

/// 出力1つぶんの記録（診断用。`measure_with` が返す。機器の名前は持たない）
#[derive(Debug, Clone)]
pub struct TargetDiag {
    pub outcome: Result<Measured, FailCode>,
    /// 回ごとの記録（狙いを決めた回だけ）
    pub clicks: Vec<ClickDiag>,
    /// 最初の回の狙いを決めたときの、時刻の当てはめ：名目の速さとの差（ppm）・残りの揺れ（ms）・記録の長さ（ms）
    pub fit_ppm: Option<f64>,
    pub fit_jitter_ms: Option<f64>,
    pub fit_span_ms: u64,
    /// 録る流れ：時刻の飛びの最大（ms。つながった所）・途切れた回数・輪があふれて捨てたパケット
    pub cap_jump_max_ms: f64,
    pub cap_breaks: u64,
    pub cap_dropped: u64,
    pub out_rate: u32,
    pub cap_rate: u32,
}

/// 記録の1行（ログと診断のテストで同じ形。機器の名前は入れない）
pub fn diag_line(d: &TargetDiag) -> String {
    let outcome = match &d.outcome {
        Ok(m) => format!("ok {:.2}ms（ばらつき {:.2}・{} 回）", m.arrival_ms, m.spread_ms, m.hits),
        Err(f) => format!("測れない {f:?}"),
    };
    let clicks: Vec<String> = d
        .clicks
        .iter()
        .enumerate()
        .map(|(k, c)| match c.result {
            Ok((ms, db, snr)) => format!("#{k} {ms:+.2}ms {db:.0}dBFS snr{snr:.0} 先{:.0}ms", c.horizon_ms),
            Err(m) => format!("#{k} {m:?} 先{:.0}ms", c.horizon_ms),
        })
        .collect();
    let opt = |v: Option<f64>, digits: usize| v.map(|x| format!("{x:.digits$}")).unwrap_or_else(|| "-".into());
    format!(
        "{outcome}｜当てはめ {}ppm 揺れ {}ms 長さ {}ms｜録る流れ 飛び最大 {:.3}ms 途切れ {} 捨て {}｜{}Hz→{}Hz｜{}",
        opt(d.fit_ppm, 1),
        opt(d.fit_jitter_ms, 3),
        d.fit_span_ms,
        d.cap_jump_max_ms,
        d.cap_breaks,
        d.cap_dropped,
        d.out_rate,
        d.cap_rate,
        clicks.join("・"),
    )
}

/// 測っている出力1つぶん（測る役のスレッドだけが触る）
struct Live {
    pair: Pair,
    freq_hz: f64,
    timing: StreamTiming,
    track: CaptureTrack,
    /// 回ごとの、狙ったフレームが鳴ると見込んだ時刻（QPC ns）。狙いを決めた回だけ
    aimed: Vec<f64>,
    horizons_ms: Vec<f64>,
    results: Vec<Option<Result<(f64, Onset), Miss>>>,
    failed: Option<FailCode>,
    /// 最初の回の狙いを決めたときの当てはめ（ppm・揺れ・長さ）
    fit_at_plan: Option<(Option<f64>, Option<f64>, u64)>,
}

impl Live {
    fn new(pair: Pair, freq_hz: f64) -> Self {
        let (out_rate, cap_rate) = (pair.out.rate, pair.cap.rate);
        Self {
            pair,
            freq_hz,
            timing: StreamTiming::new(out_rate),
            track: CaptureTrack::new(cap_rate),
            aimed: vec![],
            horizons_ms: vec![],
            results: vec![None; MAX_CLICKS],
            failed: None,
            fit_at_plan: None,
        }
    }

    /// 輪から取り出す。パケット → 標本の順（コールバックは標本を先に書くので、取り出したパケットの標本は必ずそろっている）
    fn pump(&mut self) {
        while let Ok(t) = self.pair.out.timing.pop() {
            self.timing.add(t.frames_before, t.callback_ns, t.playback_ns);
        }
        while let Ok(p) = self.pair.cap.packets.pop() {
            self.track.add_packet(p);
        }
        let n = self.pair.cap.samples.slots();
        if let Ok(c) = self.pair.cap.samples.read_chunk(n) {
            let (a, b) = c.as_slices();
            self.track.add_samples(a);
            self.track.add_samples(b);
            c.commit_all();
        }
        if self.failed.is_none() && (self.pair.out.fault.load(Ordering::Relaxed) || self.pair.cap.fault.load(Ordering::Relaxed)) {
            self.failed = Some(FailCode::StreamError);
        }
    }

    fn warm(&self, warmup_ms: u64) -> bool {
        self.timing.ready() && self.timing.observed_ms() >= warmup_ms && self.track.started()
    }

    /// 時刻が `plan_ahead_ns` 以内に来た回の狙いを決めて、コールバックへ渡す（その時点のいちばん新しい当てはめで。
    /// 前の作りは全部の回を始めに決めていて、最後の回は約 2.8 秒先の見積もりだった）
    fn plan_due(&mut self, targets_ns: &[u64], now_ns: u64, plan_ahead_ns: u64) {
        let plan = self.pair.out.plan.clone();
        while self.failed.is_none() && self.aimed.len() < targets_ns.len().min(MAX_CLICKS) {
            let k = self.aimed.len();
            let t = targets_ns[k];
            if t > now_ns.saturating_add(plan_ahead_ns) {
                break;
            }
            if self.fit_at_plan.is_none() {
                let nominal = 1e9 / self.pair.out.rate.max(1) as f64;
                let ppm = self.timing.fit_slope().map(|s| (nominal / s - 1.0) * 1e6);
                self.fit_at_plan = Some((ppm, self.timing.jitter_ms(), self.timing.observed_ms()));
            }
            let Some(frame) = self.timing.frame_at(t as f64).filter(|f| f.is_finite() && *f >= 0.0) else {
                self.failed = Some(FailCode::NoTiming);
                return;
            };
            let kf = frame.round() as u64;
            let Some(aimed) = self.timing.time_of_frame(kf as f64).filter(|v| v.is_finite()) else {
                self.failed = Some(FailCode::NoTiming);
                return;
            };
            plan.starts[k].store(kf, Ordering::Relaxed);
            self.aimed.push(aimed);
            self.horizons_ms.push((t as f64 - now_ns as f64) / 1e6);
            // 書き終えた数を出す（コールバックは count を Acquire で読んでから starts を読む）
            plan.count.store(self.aimed.len(), Ordering::Release);
        }
    }

    /// 録れた回から調べる。まだ録れていない回・まだ狙っていない回があれば true
    fn examine(&mut self, now_ns: u64, give_up_ns: u64, total: usize) -> bool {
        let late = self.pair.out.plan.late.load(Ordering::Relaxed);
        let before = ms_to_ns(tuning::LOOK_BEFORE_MS) as f64;
        let mut pending = self.aimed.len() < total;
        for k in 0..self.aimed.len() {
            if self.results[k].is_some() {
                continue;
            }
            if late & (1 << k) != 0 {
                self.results[k] = Some(Err(Miss::Late));
                continue;
            }
            match judge_click(&mut self.track, self.aimed[k], self.freq_hz, now_ns >= give_up_ns) {
                Some(r) => self.results[k] = Some(r),
                None => pending = true,
            }
        }
        // まだ調べていない回の窓より前は捨てる（長く持たない）。まだ狙っていない回があれば、今より前の 1 秒を残す
        let keep = (0..self.aimed.len())
            .filter(|k| self.results[*k].is_none())
            .map(|k| (self.aimed[k] - before).max(0.0) as u64)
            .min()
            .unwrap_or(if self.aimed.len() < total { now_ns.saturating_sub(ms_to_ns(1000)) } else { u64::MAX });
        self.track.trim_before(keep.saturating_sub(ms_to_ns(50)));
        pending
    }

    fn diag(&self) -> TargetDiag {
        let outcome = match self.failed {
            Some(f) => Err(f),
            None => {
                let rs: Vec<Result<f64, Miss>> = (0..self.aimed.len())
                    .map(|k| match self.results[k] {
                        Some(Ok((ms, _))) => Ok(ms),
                        Some(Err(m)) => Err(m),
                        None => Err(Miss::Detect(HitMiss::NotCaptured)),
                    })
                    .collect();
                summarize(&rs)
            }
        };
        let clicks = (0..self.aimed.len())
            .map(|k| ClickDiag {
                horizon_ms: self.horizons_ms[k],
                result: match self.results[k] {
                    Some(Ok((ms, o))) => Ok((ms, 20.0 * (o.peak.max(1e-12) as f64).log10(), o.snr_db)),
                    Some(Err(m)) => Err(m),
                    None => Err(Miss::Detect(HitMiss::NotCaptured)),
                },
            })
            .collect();
        let (jump, breaks) = self.track.stats();
        let (ppm, jit, span) = self.fit_at_plan.unwrap_or((None, None, self.timing.observed_ms()));
        TargetDiag {
            outcome,
            clicks,
            fit_ppm: ppm,
            fit_jitter_ms: jit,
            fit_span_ms: span,
            cap_jump_max_ms: jump,
            cap_breaks: breaks,
            cap_dropped: self.pair.cap.dropped.load(Ordering::Relaxed),
            out_rate: self.pair.out.rate,
            cap_rate: self.pair.cap.rate,
        }
    }
}

/// 開けなかった・始めなかった出力の記録
fn failed_diag(f: FailCode) -> TargetDiag {
    TargetDiag {
        outcome: Err(f),
        clicks: vec![],
        fit_ppm: None,
        fit_jitter_ms: None,
        fit_span_ms: 0,
        cap_jump_max_ms: 0.0,
        cap_breaks: 0,
        cap_dropped: 0,
        out_rate: 0,
        cap_rate: 0,
    }
}

/// クリック1回を調べる：狙った時刻 `aimed_ns` の前 `LOOK_BEFORE_MS`〜後ろ `LOOK_AFTER_MS`（＋クリックの長さ）を切り出し、
/// 高さ `freq_hz` のクリックの立ち上がりを探して「届くまでの遅れ（ms）」と見つけたクリックを返す。まだ録れていなければ None
/// （`give_up` なら「録れていない」で閉じる）
fn judge_click(track: &mut CaptureTrack, aimed_ns: f64, freq_hz: f64, give_up: bool) -> Option<Result<(f64, Onset), Miss>> {
    let before = ms_to_ns(tuning::LOOK_BEFORE_MS) as f64;
    let after = ms_to_ns(tuning::LOOK_AFTER_MS) as f64 + tuning::CLICK_MS * 1e6;
    let from = (aimed_ns - before).max(0.0) as u64;
    let to = (aimed_ns + after).max(0.0) as u64;
    let rate = track.rate();
    Some(match track.window(from, to) {
        Err(WindowMiss::NotYet) if !give_up => return None,
        Err(WindowMiss::NotYet | WindowMiss::NotCaptured) => Err(Miss::Detect(HitMiss::NotCaptured)),
        Err(WindowMiss::Gap) => Err(Miss::Detect(HitMiss::Gap)),
        Ok((win, start)) => match find_click_onset(&win, rate, freq_hz, tuning::CLICK_MS) {
            Ok(o) => match track.pos_to_ns(start as f64 + o.pos) {
                Some(ns) => Ok(((ns - aimed_ns) / 1e6, o)),
                None => Err(Miss::Detect(HitMiss::NotCaptured)),
            },
            Err(m) => Err(Miss::Detect(m)),
        },
    })
}

/// 出力ごとに遅れを測る。戻り値は `targets` と同じ並びで、回ごとの記録（診断用）も返す（結果は `outcome`）。
/// `stop` が Some を返したら、全体をやめる（開いた流れは全部落とす）。
/// 時間は、開く（ふつう数十 ms）＋落ち着くまで 3 秒＋クリック 6 回で約 2.8 秒＋最後の窓 0.4 秒（通常は約 6 秒）。
/// いちばん長いと、開く待ち 5 秒＋落ち着く待ち 5 秒＋約 2.8 秒＋録り切れない待ち約 1.9 秒で 14.7 秒ほど（画面の文は「長くて 15 秒ほど」）
pub fn measure_with(targets: &[Target], stop: &dyn Fn() -> Option<Abort>, opts: Options) -> Result<Vec<TargetDiag>, Abort> {
    let n = targets.len();
    let poll = Duration::from_millis(tuning::POLL_MS);
    let mut failed: Vec<Option<FailCode>> = vec![None; n];
    let mut lives: Vec<Option<Live>> = (0..n).map(|_| None).collect();

    // 1. 開く（出力ごとの開く役。待つのは OPEN_TIMEOUT_MS まで）
    let mut waiting: Vec<Option<Receiver<Result<Pair, FailCode>>>> = Vec::with_capacity(n);
    for t in targets {
        let (tx, rx) = bounded(1);
        let t = t.clone();
        let spawned = std::thread::Builder::new().name("pl-latency-open".into()).spawn(move || {
            // 受け取り手がもういなければ、開いた流れはここで落ちる
            let _ = tx.send(open_pair(&t));
        });
        waiting.push(spawned.ok().map(|_| rx));
    }
    for (i, w) in waiting.iter().enumerate() {
        if w.is_none() {
            failed[i] = Some(FailCode::OutputOpen);
        }
    }
    let deadline = Instant::now() + Duration::from_millis(tuning::OPEN_TIMEOUT_MS);
    loop {
        if let Some(a) = stop() {
            return Err(a);
        }
        let mut any = false;
        for i in 0..n {
            let Some(rx) = &waiting[i] else { continue };
            match rx.try_recv() {
                Ok(Ok(p)) => {
                    lives[i] = Some(Live::new(p, targets[i].freq_hz));
                    waiting[i] = None;
                }
                Ok(Err(c)) => {
                    failed[i] = Some(c);
                    waiting[i] = None;
                }
                Err(TryRecvError::Empty) => any = true,
                // 開く役が結果を返さずに終わった（panic）
                Err(TryRecvError::Disconnected) => {
                    failed[i] = Some(FailCode::OutputOpen);
                    waiting[i] = None;
                }
            }
        }
        if !any {
            break;
        }
        if Instant::now() >= deadline {
            for i in 0..n {
                if waiting[i].take().is_some() {
                    failed[i] = Some(FailCode::OpenTimeout);
                }
            }
            break;
        }
        std::thread::sleep(poll);
    }
    drop(waiting);

    // 2. 時刻の見積もりが落ち着き、録る側に音が来るまで待つ
    let warm_deadline = Instant::now() + Duration::from_millis((opts.warmup_ms + 2000).max(tuning::WARMUP_MAX_MS));
    loop {
        if let Some(a) = stop() {
            return Err(a);
        }
        let now = qpc_now_ns();
        for l in lives.iter_mut().flatten() {
            l.pump();
            l.track.trim_before(now.saturating_sub(ms_to_ns(1000)));
        }
        if lives.iter().flatten().all(|l| l.failed.is_some() || l.warm(opts.warmup_ms)) {
            break;
        }
        if Instant::now() >= warm_deadline {
            for l in lives.iter_mut().flatten() {
                if l.failed.is_none() && !l.warm(opts.warmup_ms) {
                    l.failed = Some(if l.timing.ready() { FailCode::CaptureSilent } else { FailCode::NoTiming });
                }
            }
            break;
        }
        std::thread::sleep(poll);
    }

    // 3・4. 同じ時刻を狙って、出力ごとに違う高さのクリックを鳴らす。回ごとの狙いは、その回の PLAN_AHEAD_MS 前に決める。
    //       録れた回から調べる
    let t0 = qpc_now_ns() + ms_to_ns(tuning::FIRST_CLICK_LEAD_MS);
    let targets_ns: Vec<u64> = (0..tuning::CLICK_COUNT as u64).map(|k| t0 + ms_to_ns(k * tuning::CLICK_INTERVAL_MS)).collect();
    let total = targets_ns.len().min(MAX_CLICKS);
    let last = *targets_ns.last().unwrap_or(&t0);
    let give_up = last + ms_to_ns(tuning::LOOK_AFTER_MS + tuning::CAPTURE_TAIL_MS) + 10_000_000;
    let plan_ahead = ms_to_ns(opts.plan_ahead_ms);
    loop {
        if let Some(a) = stop() {
            return Err(a);
        }
        let now = qpc_now_ns();
        let mut pending = false;
        for l in lives.iter_mut().flatten() {
            l.pump();
            if l.failed.is_none() {
                l.plan_due(&targets_ns, now, plan_ahead);
            }
            if l.failed.is_none() && l.examine(now, give_up, total) {
                pending = true;
            }
        }
        if !pending {
            break;
        }
        std::thread::sleep(poll);
    }

    let out = (0..n)
        .map(|i| match (&failed[i], &lives[i]) {
            (Some(f), _) => failed_diag(*f),
            (None, Some(l)) => l.diag(),
            (None, None) => failed_diag(FailCode::OutputOpen),
        })
        .collect();
    // 流れを止めてから落とす（落とす順は気にしない）
    for l in lives.iter().flatten() {
        let _ = l.pair.out.stream.pause();
        let _ = l.pair.cap.stream.pause();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 10ms ごとのパケットで、`click_at_ns` に高さ `freq` のクリックが入った録音を作る（音は出さない）
    fn recorded(rate: u32, t0: u64, secs: f64, clicks: &[(u64, f64)]) -> CaptureTrack {
        let total = (secs * rate as f64) as usize;
        let mut x = vec![0f32; total];
        for &(at_ns, freq) in clicks {
            let k = ((at_ns - t0) as f64 * rate as f64 / 1e9).round() as usize;
            for (i, v) in click_burst(freq, rate, tuning::CLICK_MS, CLICK_FADE_MS, 0.03).iter().enumerate() {
                if let Some(d) = x.get_mut(k + i) {
                    *d += v;
                }
            }
        }
        let mut track = CaptureTrack::new(rate);
        let frames = (rate / 100) as usize;
        for (i, chunk) in x.chunks(frames).enumerate() {
            let pos = (i * frames) as u64;
            let cap = (t0 + (pos as f64 * 1e9 / rate as f64) as u64) / 100 * 100;
            track.add_packet(CapPacket { pos, stored_before: pos, frames: chunk.len() as u32, capture_ns: cap });
            track.add_samples(chunk);
        }
        track
    }

    /// 仮想ケーブルの出口のように 33ms 遅れて届いたクリックも、ループバックのように狙いどおりのクリックも、遅れを ms で出す。
    /// 同じ録音にほかの出力のクリック（別の高さ）が入っていても見分ける
    #[test]
    fn judge_click_gives_arrival_in_ms() {
        for &rate in &[48_000u32, 44_100] {
            let t0 = 10_000_000_000u64;
            let aimed = t0 + 500_000_000;
            let late = aimed + 33_400_000;
            let mut track = recorded(rate, t0, 1.5, &[(late, 3000.0), (aimed, 1000.0)]);
            let d = judge_click(&mut track, aimed as f64, 3000.0, false).unwrap().unwrap().0;
            assert!((d - 33.4).abs() < 0.05, "{rate}: {d}");
            let d1 = judge_click(&mut track, aimed as f64, 1000.0, false).unwrap().unwrap().0;
            assert!(d1.abs() < 0.05, "{rate}: {d1}");
        }
    }

    /// まだ録れていない回は待つ（give_up なら「録れていない」）。測れる上限（LOOK_AFTER_MS）より遅いクリックは「届かない」
    #[test]
    fn judge_click_waits_and_gives_up() {
        let rate = 48_000;
        let t0 = 10_000_000_000u64;
        let mut track = recorded(rate, t0, 0.6, &[]);
        let aimed = (t0 + 500_000_000) as f64;
        assert_eq!(judge_click(&mut track, aimed, 1000.0, false), None);
        assert_eq!(judge_click(&mut track, aimed, 1000.0, true), Some(Err(Miss::Detect(HitMiss::NotCaptured))));
        let too_late = t0 + 500_000_000 + ms_to_ns(tuning::LOOK_AFTER_MS + 20);
        let mut track = recorded(rate, t0, 1.5, &[(too_late, 1000.0)]);
        assert_eq!(judge_click(&mut track, aimed, 1000.0, false), Some(Err(Miss::Detect(HitMiss::NoSignal))));
    }

    /// コールバック1回ぶんを書く（ステレオを見立てず、フレームごとの値だけを集める）
    fn render(p: u64, n: u64, plan: &ClickPlan, burst: &[f32], begun: &mut [bool; MAX_CLICKS]) -> Vec<f32> {
        let mut out = vec![0.0f32; n as usize];
        render_clicks(p, n, plan, burst, begun, &mut |j, v| out[j] = v);
        out
    }

    fn plan_of(starts: &[u64]) -> ClickPlan {
        let plan = ClickPlan::new();
        for (k, s) in starts.iter().enumerate() {
            plan.starts[k].store(*s, Ordering::Relaxed);
        }
        plan.count.store(starts.len(), Ordering::Release);
        plan
    }

    /// 狙った回は、その頭のフレームから書き、コールバックをまたいでも続きを書く
    #[test]
    fn clicks_are_written_from_their_first_frame() {
        let burst = [1.0, 2.0, 3.0, 4.0];
        let plan = plan_of(&[12]);
        let mut begun = [false; MAX_CLICKS];
        assert_eq!(render(0, 10, &plan, &burst, &mut begun), vec![0.0; 10]);
        assert_eq!(render(10, 4, &plan, &burst, &mut begun), vec![0.0, 0.0, 1.0, 2.0]);
        assert_eq!(render(14, 4, &plan, &burst, &mut begun), vec![3.0, 4.0, 0.0, 0.0]);
        assert_eq!(plan.late.load(Ordering::Relaxed), 0);
    }

    /// 検査3回目の指摘：狙いを決めたときにもう過ぎていた回は「間に合わなかった」になる。
    /// クリックの終わりまで過ぎていた回（前は印が付かず「音が無い」になっていた）も、頭だけ過ぎていた回も。途中からは鳴らさない
    #[test]
    fn clicks_already_past_are_marked_late() {
        let burst = [1.0, 2.0, 3.0, 4.0];
        // 回0：終わりまで過ぎている（2..6 で、今は 100 から）。回1：頭だけ過ぎている（98..102）。回2：これから（105）
        let plan = plan_of(&[2, 98, 105]);
        let mut begun = [false; MAX_CLICKS];
        let out = render(100, 8, &plan, &burst, &mut begun);
        assert_eq!(out, vec![0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0]);
        assert_eq!(plan.late.load(Ordering::Relaxed), 0b011);
        // 測る役は、印の付いた回を録音で探さずに「間に合わなかった」とする
        let late = plan.late.load(Ordering::Relaxed);
        let miss = if late & 1 != 0 { Some(Miss::Late) } else { None };
        assert_eq!(miss.map(|m| m.code()), Some(FailCode::Late));
        // 続きのコールバック：回2 の残りを書き、過ぎていた回は書かない
        assert_eq!(render(108, 4, &plan, &burst, &mut begun), vec![4.0, 0.0, 0.0, 0.0]);
        assert_eq!(plan.late.load(Ordering::Relaxed), 0b011);
    }

    /// まだ狙っていない回（count より後ろ・starts が既定の値）は書かない
    #[test]
    fn unplanned_clicks_are_not_written() {
        let plan = ClickPlan::new();
        let mut begun = [false; MAX_CLICKS];
        assert_eq!(render(0, 8, &plan, &[1.0], &mut begun), vec![0.0; 8]);
        plan.count.store(1, Ordering::Release);
        assert_eq!(render(8, 8, &plan, &[1.0], &mut begun), vec![0.0; 8]);
        assert_eq!(plan.late.load(Ordering::Relaxed), 0);
    }

    /// 記録の1行：機器の名前は入らず、回ごとの遅れ・大きさ・先の見積もり・取れなかった理由が入る
    #[test]
    fn diag_line_has_numbers_and_reasons() {
        let d = TargetDiag {
            outcome: Err(FailCode::Unstable),
            clicks: vec![
                ClickDiag { horizon_ms: 195.0, result: Ok((33.41, -30.2, 41.0)) },
                ClickDiag { horizon_ms: 190.0, result: Err(Miss::Detect(HitMiss::Weak)) },
            ],
            fit_ppm: Some(-12.34),
            fit_jitter_ms: Some(0.271),
            fit_span_ms: 1800,
            cap_jump_max_ms: 0.989,
            cap_breaks: 1,
            cap_dropped: 0,
            out_rate: 48_000,
            cap_rate: 44_100,
        };
        let s = diag_line(&d);
        for want in ["測れない Unstable", "-12.3ppm", "揺れ 0.271ms", "長さ 1800ms", "飛び最大 0.989ms", "途切れ 1", "48000Hz→44100Hz", "#0 +33.41ms -30dBFS snr41 先195ms", "#1 Detect(Weak) 先190ms"] {
            assert!(s.contains(want), "{want} が無い：{s}");
        }
        assert!(diag_line(&failed_diag(FailCode::OpenTimeout)).starts_with("測れない OpenTimeout"));
    }

    /// 実機の診断（クリックが鳴る。2026-11-18 の「仮想ケーブルが毎回そろわない」を絞るため）。手で走らせる：
    ///
    /// ```text
    /// cd src-tauri
    /// $env:PL_LATENCY_DEVICES = "default|CABLE Input"   # 測る出力。「default」か名前の一部（1つに決まるもの）を | で
    /// cargo test --lib audio::latency::run::tests::diagnose -- --ignored --exact --nocapture
    /// ```
    ///
    /// ほかの環境変数（無ければアプリと同じ値）：
    /// - `PL_LATENCY_CAPTURE`：出力ごとの録る所を | で。`auto`（名前から。既定）・`loopback`・録音デバイスの名前の一部
    /// - `PL_LATENCY_WARMUP_MS`：落ち着くまで待つ長さ（アプリは 3000。最初の作りは 500）
    /// - `PL_LATENCY_PLAN_AHEAD_MS`：回ごとの狙いを何 ms 前に決めるか（アプリは 200。10000 にすると前の作り＝全部の回を始めに決める）
    /// - `PL_LATENCY_RUNS`：繰り返す回数（既定 3）
    ///
    /// 出すもの：枠の番号・録る所の種類（ループバック／録音デバイス）・クリックの高さと、`diag_line` の1行（回ごとの遅れ ms・大きさ・
    /// 雑音との比・狙いを決めたときの先の長さ・取れなかった理由・時刻の当てはめの ppm と揺れ・録る流れの時刻の飛びと途切れ）。
    /// 機器の名前は出さない。指定が1つに決まらないときも、名前の一覧は出さずに止まる
    #[test]
    #[ignore]
    fn diagnose() {
        use crate::audio::latency::{fill_values, pick_capture};
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let num = |k: &str, d: u64| env(k).map(|v| v.trim().parse::<u64>().unwrap_or_else(|_| panic!("{k} が数でない"))).unwrap_or(d);
        let specs = env("PL_LATENCY_DEVICES").expect("PL_LATENCY_DEVICES に測る出力を | で（「default」か名前の一部）");
        let caps: Vec<String> = env("PL_LATENCY_CAPTURE").map(|v| v.split('|').map(|x| x.trim().to_string()).collect()).unwrap_or_default();
        let outs = devices::list_output_devices();
        let ins = devices::list_input_devices();
        let pick_one = |list: &[devices::DeviceInfo], spec: &str, what: &str, i: usize| -> devices::DeviceInfo {
            let low = spec.to_lowercase();
            let hit: Vec<&devices::DeviceInfo> = list.iter().filter(|d| d.name.to_lowercase().contains(&low)).collect();
            assert!(hit.len() == 1, "{what} の {} 番目の指定が {} 個に当たった（1つに決まる名前の一部にする）", i + 1, hit.len());
            hit[0].clone()
        };
        let mut targets = vec![];
        let mut labels = vec![];
        for (i, spec) in specs.split('|').map(str::trim).enumerate().take(tuning::FREQS_HZ.len()) {
            let (output_id, name) = if spec.eq_ignore_ascii_case("default") {
                let d = devices::find_output(None).expect("既定の出力が無い");
                (None, devices::device_name(&d))
            } else {
                let d = pick_one(&outs, spec, "PL_LATENCY_DEVICES", i);
                (Some(d.id), d.name)
            };
            let capture = match caps.get(i).map(String::as_str) {
                None | Some("auto") | Some("") => pick_capture(&name, &ins),
                Some("loopback") => CaptureSource::Loopback,
                Some(c) => CaptureSource::Input { id: pick_one(&ins, c, "PL_LATENCY_CAPTURE", i).id },
            };
            let kind = if capture == CaptureSource::Loopback { "ループバック" } else { "録音デバイス" };
            let freq = tuning::FREQS_HZ[i];
            labels.push(format!("枠{i}（{kind}・{freq:.0}Hz）"));
            targets.push(Target { output_id, capture, freq_hz: freq });
        }
        let opts = Options {
            warmup_ms: num("PL_LATENCY_WARMUP_MS", tuning::WARMUP_MS),
            plan_ahead_ms: num("PL_LATENCY_PLAN_AHEAD_MS", tuning::PLAN_AHEAD_MS),
        };
        let runs = num("PL_LATENCY_RUNS", 3).clamp(1, 20);
        println!("遅れを測る診断：{opts:?}・{runs} 回・クリック {} 回・{}ms おき・窓 -{}〜+{}ms", tuning::CLICK_COUNT, tuning::CLICK_INTERVAL_MS, tuning::LOOK_BEFORE_MS, tuning::LOOK_AFTER_MS);
        for r in 0..runs {
            let started = Instant::now();
            let diags = measure_with(&targets, &|| None, opts).expect("やめていない");
            let fills = fill_values(&diags.iter().map(|d| d.outcome.as_ref().ok().map(|m| m.arrival_ms)).collect::<Vec<_>>());
            println!("== {} 回目（{:.1} 秒）", r + 1, started.elapsed().as_secs_f64());
            for (i, d) in diags.iter().enumerate() {
                println!("{}：入れる値 {:?}｜{}", labels[i], fills[i], diag_line(d));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }
}
