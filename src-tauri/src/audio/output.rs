//! 出力（cpal / WASAPI）。v3.8.0 で、出力を最大3つ同時に開けるように作り直した（仕様 13.1・13.5）。
//!
//! 音声スレッドとは、出力ごとの受け渡しの輪（rtrb のリングバッファ・0.5 秒）と原子変数だけでつながる。
//! **コールバックの中ではロック・メモリの確保・ログ出力をしない**（3.7.0 と同じ決まり）。
//!
//! - 開始時刻の門（13.1 の6）：音声スレッドが「輪の最初のサンプルを鳴らすフレーム番号」（`base`）を決めてから書く。
//!   コールバックは自分が書くフレームの番号を数え、`base` より前は無音、遅れたら遅れた分を読み捨てる。
//!   輪が空になった（デコードが間に合わない）ときも、後で読み捨てて合わせ直すので、ずれない
//! - 世代番号（13.1 の6）：シーク・曲の切り替え・一時停止では、音声スレッドが世代を上げ、コールバックが輪を空にして
//!   世代を返す。3.7.0 の `flush` は Mutex を握り、コールバックは `try_lock` に失敗すると数えずに無音を出していた（通し番号が狂う）
//! - メーター（13.5 の13）：音量を掛けたあと・±1 に切り詰める前の左右の最大を、原子変数に fetch_max で溜める
//! - 時刻の記録：コールバックごとに（そのコールバックの最初のフレームの番号, callback, playback）を音声スレッドへ渡す

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, I24};
use crossbeam_channel::Sender;
use rtrb::{Consumer, Producer, RingBuffer};

use super::devices;
use super::engine::Internal;
use super::sync::{qpc_now_ns, tuning};

/// 出力先の数（仕様 2.1）
pub const SLOTS: usize = 3;

/// 枠ごとの、出力が変わっても引き継ぐ値（音量とメーター）。画面のメーターはここから読む
pub struct SlotShared {
    gain_bits: AtomicU32,
    peak: [AtomicU32; 2],
}

impl SlotShared {
    fn new(gain: f32) -> Self {
        Self { gain_bits: AtomicU32::new(gain.to_bits()), peak: [AtomicU32::new(0), AtomicU32::new(0)] }
    }
    pub fn gain(&self) -> f32 {
        f32::from_bits(self.gain_bits.load(Ordering::Relaxed))
    }
}

/// 全部の出力が共有する値（一時停止と、枠ごとの音量・メーター）
pub struct OutputBank {
    pub paused: AtomicBool,
    pub slots: [SlotShared; SLOTS],
}

impl OutputBank {
    pub fn new() -> Self {
        Self { paused: AtomicBool::new(true), slots: [SlotShared::new(0.0), SlotShared::new(0.0), SlotShared::new(0.0)] }
    }

    /// 実際に掛ける倍率（司令塔が「全体 × 出力ごと」の2乗やミュートを済ませた値）
    pub fn set_gain(&self, slot: usize, gain: f32) {
        if let Some(s) = self.slots.get(slot) {
            let g = if gain.is_finite() { gain.clamp(0.0, 4.0) } else { 0.0 };
            s.gain_bits.store(g.to_bits(), Ordering::Relaxed);
        }
    }

    /// メーターの値（左右の最大。1.0 がフルスケール）を読んで 0 に戻す（13.5 の13）
    pub fn take_peaks(&self) -> [[f32; 2]; SLOTS] {
        let mut out = [[0.0f32; 2]; SLOTS];
        for (o, s) in out.iter_mut().zip(self.slots.iter()) {
            for (v, cell) in o.iter_mut().zip(s.peak.iter()) {
                *v = f32::from_bits(cell.swap(0, Ordering::Relaxed));
            }
        }
        out
    }
}

impl Default for OutputBank {
    fn default() -> Self {
        Self::new()
    }
}

/// メーターに溜める値。数でない値（NaN・無限）は 0（13.5 の13）
pub fn meter_value(v: f32) -> f32 {
    let a = v.abs();
    if a.is_finite() {
        a
    } else {
        0.0
    }
}

/// 原子変数に「大きいほう」を残す。0 以上の有限の f32 はビットの並びの大小と値の大小が同じなので、u32 の fetch_max でよい
pub fn store_peak(cell: &AtomicU32, v: f32) {
    let v = meter_value(v);
    if v > 0.0 {
        cell.fetch_max(v.to_bits(), Ordering::Relaxed);
    }
}

/// コールバック1回ぶんの時刻の記録
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub frames_before: u64,
    pub callback_ns: u64,
    pub playback_ns: u64,
}

/// コールバックと音声スレッドが共有する、ストリーム1本ぶんの値
struct StreamShared {
    /// 音声スレッドが上げる世代（輪を空にしてほしい）
    gen: AtomicU64,
    /// コールバックが空にし終えた世代
    acked: AtomicU64,
    /// この世代の輪の最初のサンプルを鳴らすフレーム番号。`NO_BASE` なら何も鳴らさない
    base: AtomicU64,
}

const NO_BASE: u64 = u64::MAX;

/// 開けなかった理由（6.4 の「切断中」と「開けません」の見分け。13.5 の7）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenFail {
    /// 一覧に無い（切断中）
    NotFound,
    /// このデバイスの形式は使えません
    Format(String),
    /// ほかのアプリが独り占めしています
    Busy(String),
    /// このデバイスを開けませんでした
    Other(String),
}

impl OpenFail {
    fn from_cpal(kind: cpal::ErrorKind, msg: String) -> Self {
        match kind {
            cpal::ErrorKind::DeviceNotAvailable => OpenFail::NotFound,
            cpal::ErrorKind::UnsupportedConfig => OpenFail::Format(msg),
            cpal::ErrorKind::DeviceBusy => OpenFail::Busy(msg),
            _ => OpenFail::Other(msg),
        }
    }
    pub fn detail(&self) -> &str {
        match self {
            OpenFail::NotFound => "not found",
            OpenFail::Format(s) | OpenFail::Busy(s) | OpenFail::Other(s) => s,
        }
    }
}

/// 開いている出力1本（音声スレッドが持つ。開くのは別のスレッドで、開けたものを音声スレッドへ渡す）
pub struct OutputStream {
    stream: cpal::Stream,
    producer: Producer<f32>,
    timing: Consumer<Timing>,
    shared: Arc<StreamShared>,
    pub rate: u32,
    pub channels: usize,
    capacity: usize,
    /// エラーの知らせがどのストリームのものかを見分ける番号
    pub serial: u64,
    /// 開いたデバイスの ID（既定のデバイスで開いたときは、そのときの既定の ID）
    pub resolved_id: Option<String>,
    pub name: String,
    /// 自動のアイコン（headphones / speaker / stream）
    pub kind: &'static str,
    /// 自前の QPC と `stream.now()` の差（ns。13.1 の2）
    pub now_diff_ns: i64,
    gen: u64,
}

impl std::fmt::Debug for OutputStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutputStream")
            .field("serial", &self.serial)
            .field("rate", &self.rate)
            .field("channels", &self.channels)
            .finish_non_exhaustive()
    }
}

impl OutputStream {
    /// デバイスを、その既定の形式（ミックスの形式）のまま開く（13.5 の7）。`device_id` が None なら既定のデバイス
    pub fn open(
        device_id: Option<&str>,
        slot: usize,
        serial: u64,
        bank: Arc<OutputBank>,
        internal: Sender<Internal>,
    ) -> Result<Self, OpenFail> {
        let device = devices::find_output(device_id).ok_or(OpenFail::NotFound)?;
        let name = devices::device_name(&device);
        let kind = devices::device_kind(&device);
        let resolved_id = devices::device_id(&device);
        let supported = device
            .default_output_config()
            .map_err(|e| OpenFail::from_cpal(e.kind(), format!("default_output_config: {e}")))?;
        let rate = supported.sample_rate();
        let channels = supported.channels() as usize;
        if channels == 0 || rate == 0 {
            return Err(OpenFail::Format(format!("{channels} ch / {rate} Hz")));
        }
        let config = cpal::StreamConfig {
            channels: supported.channels(),
            sample_rate: rate,
            buffer_size: cpal::BufferSize::Default,
        };
        // 0.5 秒ぶんの輪（13.1 の5：出力の輪は太らせない）
        let capacity = (rate as usize / 2).max(4096) * channels;
        let (producer, consumer) = RingBuffer::<f32>::new(capacity);
        let (timing_tx, timing_rx) = RingBuffer::<Timing>::new(1024);
        let shared = Arc::new(StreamShared { gen: AtomicU64::new(0), acked: AtomicU64::new(0), base: AtomicU64::new(NO_BASE) });
        let p = CallbackParts {
            channels,
            fade_frames: (rate as u64 * tuning::FADE_IN_MS as u64 / 1000).max(1),
            slot,
            serial,
            shared: shared.clone(),
            bank,
            consumer,
            timing: timing_tx,
        };
        let format = supported.sample_format();
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, &config, p, internal),
            SampleFormat::F64 => build::<f64>(&device, &config, p, internal),
            SampleFormat::I16 => build::<i16>(&device, &config, p, internal),
            SampleFormat::I24 => build::<I24>(&device, &config, p, internal),
            SampleFormat::I32 => build::<i32>(&device, &config, p, internal),
            SampleFormat::I64 => build::<i64>(&device, &config, p, internal),
            SampleFormat::U8 => build::<u8>(&device, &config, p, internal),
            // U16 などは WASAPI の既定の形式からは出てこない（cpal device.rs 217-252行）
            other => Err(OpenFail::Format(format!("unsupported device sample format: {other:?}"))),
        }?;
        stream.play().map_err(|e| OpenFail::from_cpal(e.kind(), format!("stream play: {e}")))?;
        let q1 = qpc_now_ns();
        let now = stream.now().as_nanos().min(u64::MAX as u128) as u64;
        let q2 = qpc_now_ns();
        let now_diff_ns = (q1 / 2 + q2 / 2) as i64 - now as i64;
        Ok(Self {
            stream,
            producer,
            timing: timing_rx,
            shared,
            rate,
            channels,
            capacity,
            serial,
            resolved_id,
            name,
            kind,
            now_diff_ns,
            gen: 0,
        })
    }

    /// 自前の QPC と cpal の時刻がそろっているか（13.1 の2）
    pub fn clock_matches(&self) -> bool {
        (self.now_diff_ns.unsigned_abs() as f64) < tuning::QPC_STREAM_TOLERANCE_MS * 1e6
    }

    /// 輪の空き（フレーム）
    pub fn free_frames(&self) -> usize {
        self.producer.slots() / self.channels.max(1)
    }

    pub fn capacity_frames(&self) -> usize {
        self.capacity / self.channels.max(1)
    }

    /// 入るだけ入れて、入れたサンプル数を返す
    pub fn push(&mut self, samples: &[f32]) -> usize {
        let (pushed, _rest) = self.producer.push_partial_slice(samples);
        pushed.len()
    }

    /// 輪を空にしてもらう（世代を上げる）。鳴らす予定も消す
    pub fn request_flush(&mut self) {
        self.shared.base.store(NO_BASE, Ordering::Release);
        self.gen += 1;
        self.shared.gen.store(self.gen, Ordering::Release);
    }

    /// コールバックが空にし終えたか
    pub fn flush_done(&self) -> bool {
        self.shared.acked.load(Ordering::Acquire) == self.gen
    }

    /// この世代の輪の最初のサンプルを、フレーム `k` から鳴らす。書く前に呼ぶ
    pub fn set_base(&self, k: u64) {
        self.shared.base.store(k, Ordering::Release);
    }

    /// コールバックの時刻の記録を取り出す
    pub fn drain_timing(&mut self, mut f: impl FnMut(Timing)) {
        while let Ok(t) = self.timing.pop() {
            f(t);
        }
    }

    /// 止める（デバイスを手放すのは drop のとき）
    pub fn pause_stream(&self) {
        let _ = self.stream.pause();
    }
}

/// コールバックに渡す部品
struct CallbackParts {
    channels: usize,
    fade_frames: u64,
    slot: usize,
    serial: u64,
    shared: Arc<StreamShared>,
    bank: Arc<OutputBank>,
    consumer: Consumer<f32>,
    timing: Producer<Timing>,
}

/// 1回ぶんのコールバックの中身（型に依らない部分）。`out` に鳴らす値（音量を掛けたあと・切り詰める前）を書く。
/// テストできるように、cpal の型から切り離している
struct Renderer {
    channels: usize,
    fade_frames: u64,
    frames_total: u64,
    acked: u64,
    popped: u64,
    /// 音を上げ始めるフレーム（門の base。遅れて入って読み捨てたときは、読み捨てたあとの最初のフレーム）
    fade_origin: u64,
    /// 最後に見た base（変わったら新しい門）
    last_base: u64,
}

impl Renderer {
    /// `n` フレームぶんを `put(フレームの中の位置, サンプルの値)` で書く。書かない所は呼び出し側が無音で埋めておく
    fn render(
        &mut self,
        n: usize,
        shared: &StreamShared,
        paused: bool,
        gain: f32,
        consumer: &mut Consumer<f32>,
        put: &mut dyn FnMut(usize, f32),
    ) {
        let ch = self.channels.max(1);
        let p = self.frames_total;
        self.frames_total += n as u64;
        let gen = shared.gen.load(Ordering::Acquire);
        if gen != self.acked {
            let all = consumer.slots();
            if let Ok(chunk) = consumer.read_chunk(all) {
                chunk.commit_all();
            }
            self.popped = 0;
            self.acked = gen;
            self.last_base = NO_BASE;
            shared.acked.store(gen, Ordering::Release);
        }
        if paused {
            return;
        }
        // 輪の中身を先に見てから base を読む（書く側は base を決めてから書く）
        let mut avail = (consumer.slots() / ch) as u64;
        let base = shared.base.load(Ordering::Acquire);
        if base == NO_BASE {
            return;
        }
        if base != self.last_base {
            self.last_base = base;
            self.fade_origin = base;
        }
        let mut read_abs = base + self.popped;
        // 遅れた分は読み捨てる（開くのが遅れた・輪が空になった）
        if read_abs < p {
            let d = (p - read_abs).min(avail);
            if d > 0 {
                if let Ok(chunk) = consumer.read_chunk(d as usize * ch) {
                    chunk.commit_all();
                }
                self.popped += d;
                read_abs += d;
                avail -= d;
                // 途中から入った・音が切れて合わせ直した：波形の途中から始まるので、そこからも音を上げる
                self.fade_origin = read_abs;
            }
            if read_abs < p {
                return;
            }
        }
        // 門より前は無音
        let first = (read_abs - p).min(n as u64) as usize;
        let m = ((n - first) as u64).min(avail) as usize;
        if m == 0 {
            return;
        }
        if let Ok(chunk) = consumer.read_chunk(m * ch) {
            let (a, b) = chunk.as_slices();
            for (i, v) in a.iter().chain(b.iter()).enumerate() {
                let frame = first + i / ch;
                let since = (read_abs + (i / ch) as u64).saturating_sub(self.fade_origin);
                let w = if since < self.fade_frames { (since + 1) as f32 / self.fade_frames as f32 } else { 1.0 };
                let x = *v * gain * w;
                // 数でない値（NaN・無限）はデバイスへ渡さない（f32::clamp は NaN をそのまま返す）
                put(frame * ch + i % ch, if x.is_finite() { x } else { 0.0 });
            }
            chunk.commit_all();
            self.popped += m as u64;
        }
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    parts: CallbackParts,
    internal: Sender<Internal>,
) -> Result<cpal::Stream, OpenFail>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let CallbackParts { channels, fade_frames, slot, serial, shared, bank, mut consumer, mut timing } = parts;
    let ch = channels.max(1);
    let silence = T::from_sample(0.0f32);
    let mut r = Renderer { channels: ch, fade_frames, frames_total: 0, acked: 0, popped: 0, fade_origin: 0, last_base: NO_BASE };
    let data_cb = move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
        let n = data.len() / ch;
        let ts = info.timestamp();
        let _ = timing.push(Timing {
            frames_before: r.frames_total,
            callback_ns: ts.callback.as_nanos().min(u64::MAX as u128) as u64,
            playback_ns: ts.playback.as_nanos().min(u64::MAX as u128) as u64,
        });
        data.fill(silence);
        let sh = &bank.slots[slot];
        let gain = sh.gain();
        let paused = bank.paused.load(Ordering::Relaxed);
        let (mut pl, mut pr) = (0.0f32, 0.0f32);
        r.render(n, &shared, paused, gain, &mut consumer, &mut |idx, v| {
            let c = idx % ch;
            let m = meter_value(v);
            if c == 0 {
                pl = pl.max(m);
                if ch == 1 {
                    pr = pr.max(m);
                }
            } else if c == 1 {
                pr = pr.max(m);
            }
            if let Some(d) = data.get_mut(idx) {
                *d = T::from_sample(v.clamp(-1.0, 1.0));
            }
        });
        store_peak(&sh.peak[0], pl);
        store_peak(&sh.peak[1], pr);
    };
    let err_cb = move |err: cpal::Error| {
        // コールバックの中では待たない。種類だけを音声スレッドへ流す
        let _ = internal.try_send(Internal::StreamError { slot, serial, kind: err.kind(), msg: err.to_string() });
    };
    device
        .build_output_stream::<T, _, _>(*config, data_cb, err_cb, None)
        .map_err(|e| OpenFail::from_cpal(e.kind(), format!("build_output_stream: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(ch: usize, cap: usize) -> (Renderer, Arc<StreamShared>, Producer<f32>, Consumer<f32>) {
        let (p, c) = RingBuffer::<f32>::new(cap * ch);
        let shared = Arc::new(StreamShared { gen: AtomicU64::new(0), acked: AtomicU64::new(0), base: AtomicU64::new(NO_BASE) });
        (Renderer { channels: ch, fade_frames: 1, frames_total: 0, acked: 0, popped: 0, fade_origin: 0, last_base: NO_BASE }, shared, p, c)
    }

    fn run(r: &mut Renderer, s: &StreamShared, c: &mut Consumer<f32>, n: usize, ch: usize, paused: bool, gain: f32) -> Vec<f32> {
        let mut out = vec![0.0f32; n * ch];
        r.render(n, s, paused, gain, c, &mut |i, v| out[i] = v);
        out
    }

    /// 門：base（輪の最初のサンプルを鳴らすフレーム）より前は無音、そこから輪の中身。音量を掛ける
    #[test]
    fn gate_waits_for_base_then_plays() {
        let (mut r, s, mut p, mut c) = parts(1, 64);
        // base が無ければ何も鳴らさない（輪に何かあっても）
        assert_eq!(run(&mut r, &s, &mut c, 4, 1, false, 1.0), vec![0.0; 4]);
        s.base.store(6, Ordering::Release);
        let _ = p.push_partial_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        // フレーム 4..8：6 から鳴る
        assert_eq!(run(&mut r, &s, &mut c, 4, 1, false, 0.5), vec![0.0, 0.0, 0.5, 1.0]);
        assert_eq!(run(&mut r, &s, &mut c, 4, 1, false, 1.0), vec![3.0, 4.0, 5.0, 6.0]);
        // 輪が空：無音（数は進む）
        assert_eq!(run(&mut r, &s, &mut c, 2, 1, false, 1.0), vec![0.0, 0.0]);
        // 遅れて来たデータ（フレーム 12,13 の分）は読み捨て、今のフレーム 14 から合わせる
        let _ = p.push_partial_slice(&[7.0, 8.0, 9.0, 10.0, 11.0, 12.0]);
        assert_eq!(run(&mut r, &s, &mut c, 2, 1, false, 1.0), vec![9.0, 10.0]);
    }

    /// 遅れて開いた（base が過去）なら、遅れた分を読み捨てて今のフレームに合わせる
    #[test]
    fn late_join_drops_the_late_part() {
        let (mut r, s, mut p, mut c) = parts(2, 64);
        r.frames_total = 10;
        s.base.store(7, Ordering::Release);
        let _ = p.push_partial_slice(&[7.0, -7.0, 8.0, -8.0, 9.0, -9.0, 10.0, -10.0, 11.0, -11.0]);
        assert_eq!(run(&mut r, &s, &mut c, 2, 2, false, 1.0), vec![10.0, -10.0, 11.0, -11.0]);
    }

    /// 世代を上げたら、コールバックは輪を空にして世代を返す。一時停止中は無音で、数だけ進む
    #[test]
    fn flush_by_generation_and_pause() {
        let (mut r, s, mut p, mut c) = parts(1, 64);
        s.base.store(0, Ordering::Release);
        let _ = p.push_partial_slice(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(run(&mut r, &s, &mut c, 1, 1, false, 1.0), vec![1.0]);
        s.base.store(NO_BASE, Ordering::Release);
        s.gen.store(1, Ordering::Release);
        assert_eq!(run(&mut r, &s, &mut c, 2, 1, true, 1.0), vec![0.0, 0.0]);
        assert_eq!(s.acked.load(Ordering::Acquire), 1);
        assert_eq!(c.slots(), 0);
        assert_eq!(r.frames_total, 3);
        // 新しい世代の門：フレーム 5 から
        s.base.store(5, Ordering::Release);
        let _ = p.push_partial_slice(&[9.0, 9.5]);
        assert_eq!(run(&mut r, &s, &mut c, 3, 1, false, 1.0), vec![0.0, 0.0, 9.0]);
    }

    /// 門を開いたあと約 10ms で音を上げる
    #[test]
    fn fade_in_after_the_gate() {
        let (mut r, s, mut p, mut c) = parts(1, 64);
        r.fade_frames = 4;
        s.base.store(0, Ordering::Release);
        let _ = p.push_partial_slice(&[1.0; 6]);
        assert_eq!(run(&mut r, &s, &mut c, 6, 1, false, 1.0), vec![0.25, 0.5, 0.75, 1.0, 1.0, 1.0]);
    }

    /// 遅れて入った出力も、読み捨てたあとの最初のフレームから音を上げる
    #[test]
    fn late_join_fades_in_from_the_first_played_frame() {
        let (mut r, s, mut p, mut c) = parts(1, 64);
        r.fade_frames = 4;
        r.frames_total = 10;
        s.base.store(7, Ordering::Release);
        let _ = p.push_partial_slice(&[7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0]);
        assert_eq!(run(&mut r, &s, &mut c, 4, 1, false, 1.0), vec![2.5, 5.5, 9.0, 13.0]);
        // 輪が空になって合わせ直したときも、そこから上げる
        assert_eq!(run(&mut r, &s, &mut c, 2, 1, false, 1.0), vec![0.0, 0.0]);
        let _ = p.push_partial_slice(&[14.0, 15.0, 16.0, 17.0, 18.0, 19.0, 20.0, 21.0]);
        assert_eq!(run(&mut r, &s, &mut c, 4, 1, false, 1.0), vec![4.0, 8.5, 13.5, 19.0]);
    }

    /// 数でない値は無音にして出す（メーターも 0）
    #[test]
    fn nan_and_inf_are_sent_as_silence() {
        let (mut r, s, mut p, mut c) = parts(1, 64);
        s.base.store(0, Ordering::Release);
        let _ = p.push_partial_slice(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.5]);
        assert_eq!(run(&mut r, &s, &mut c, 4, 1, false, 1.0), vec![0.0, 0.0, 0.0, 0.5]);
    }

    /// メーター：数でない値は 0。原子変数には大きいほうが残り、読むと 0 に戻る
    #[test]
    fn meter_keeps_the_max_and_ignores_nan() {
        assert_eq!(meter_value(f32::NAN), 0.0);
        assert_eq!(meter_value(f32::INFINITY), 0.0);
        assert_eq!(meter_value(-0.5), 0.5);
        let bank = OutputBank::new();
        store_peak(&bank.slots[1].peak[0], 0.25);
        store_peak(&bank.slots[1].peak[0], 0.125);
        store_peak(&bank.slots[1].peak[0], f32::NAN);
        store_peak(&bank.slots[1].peak[1], -1.5);
        let got = bank.take_peaks();
        assert_eq!(got[1], [0.25, 1.5]);
        assert_eq!(got[0], [0.0, 0.0]);
        assert_eq!(bank.take_peaks()[1], [0.0, 0.0]);
        // 0 以上の f32 は、ビットの並びの大小と値の大小が同じ
        let vals = [0.0f32, 1e-30, 1e-3, 0.5, 0.99, 1.0, 2.0, 1e30];
        for w in vals.windows(2) {
            assert!(w[0].to_bits() < w[1].to_bits());
        }
        bank.set_gain(2, f32::NAN);
        assert_eq!(bank.slots[2].gain(), 0.0);
    }
}
