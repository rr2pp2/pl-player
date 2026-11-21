//! 録った音からクリックの立ち上がりを探す・録った流れを時刻で切り出す（v3.8.0。音を鳴らさずに確かめられる部分だけ）。
//!
//! 中身は計測の道具 `tests\sync_probe.rs` の `loopback_click_probe` で作者の実機に当てて確かめた作り（仕様 13.3）を、
//! 本体で使うために写したもの。計測の道具は本体を使わない決まりなので、道具の側はそのまま残している（直すときは両方を見る）。
//! 本体で変えたのは、要約・CSV のための数を持たないことだけ。決め方（窓・断る基準）は同じ。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// クリックの前後を弱める長さ（ms）
pub const CLICK_FADE_MS: f64 = 0.5;

/// 高さを見分ける窓の長さ（ms。ハン窓）。2ms のハン窓は、狙った高さから 1kHz 以上離れた「ずっと鳴っている音」を消す
/// （クリックの 1kHz・3kHz・5kHz どうしも消し合う）。決め方は sync_probe.rs の同じ名前の値
const TONE_WINDOW_MS: f64 = 2.0;
/// 見つけたとみなす大きさの下限（振幅。-80 dBFS）
const ONSET_MIN_PEAK: f32 = 1e-4;
/// 窓の中の中央値（雑音の目安）に対して、これだけ大きくないと「小さい」で断る（dB）
pub const ONSET_MIN_SNR_DB: f64 = 20.0;
/// 同じ高さで、見つけたものの半分以上の山がほかにもあれば「紛らわしい」で断る
const ONSET_MAX_RIVAL: f32 = 0.5;
/// クリックの所で「隣の高さ（＋1kHz）」がふだんより上がった分が、きれいなクリックより山のこの割合以上大きければ断る
const ONSET_MAX_OFF_EXTRA: f32 = 0.3;

/// クリック1回の波形（モノラル）。正弦波を `dur_ms` だけ、前後 `fade_ms` を半分の余弦で弱める
pub fn click_burst(freq_hz: f64, rate: u32, dur_ms: f64, fade_ms: f64, amp: f32) -> Vec<f32> {
    let n = ((dur_ms / 1000.0) * rate as f64).round().max(1.0) as usize;
    let fade = (((fade_ms / 1000.0) * rate as f64).round() as usize).min(n / 2);
    (0..n)
        .map(|i| {
            let t = i as f64 / rate as f64;
            let edge = i.min(n - 1 - i);
            let g = if fade > 0 && edge < fade {
                0.5 - 0.5 * (std::f64::consts::PI * (edge as f64 + 0.5) / fade as f64).cos()
            } else {
                1.0
            };
            (amp as f64 * g * (2.0 * std::f64::consts::PI * freq_hz * t).sin()) as f32
        })
        .collect()
}

fn tone_window_len(rate: u32) -> usize {
    ((rate as f64 * TONE_WINDOW_MS / 1000.0).round() as usize).max(4)
}

/// 高さ `freq_hz` の成分の大きさ（振幅）を、`m` 標本のハン窓をずらしながら出す。戻り値の i 番目は標本 i〜i+m−1 の窓。
/// ハン窓を掛けた和を、3つの高さ（ω と ω±2π/m）の素の和（累積和）の組み合わせで出す（長さに比例する手間・誤差がたまらない）
fn tone_envelope(x: &[f32], rate: u32, freq_hz: f64, m: usize) -> Vec<f32> {
    if m == 0 || x.len() < m {
        return vec![];
    }
    let w = 2.0 * std::f64::consts::PI * freq_hz / rate as f64;
    let big = 2.0 * std::f64::consts::PI / m as f64;
    let ws = [w, w - big, w + big];
    let pre: Vec<Vec<(f64, f64)>> = ws
        .iter()
        .map(|&wj| {
            let mut v = Vec::with_capacity(x.len() + 1);
            let (mut re, mut im) = (0f64, 0f64);
            v.push((re, im));
            for (k, &xv) in x.iter().enumerate() {
                let ph = wj * k as f64;
                re += xv as f64 * ph.cos();
                im -= xv as f64 * ph.sin();
                v.push((re, im));
            }
            v
        })
        .collect();
    let norm = m as f64 / 2.0;
    (0..=x.len() - m)
        .map(|i| {
            let sum = |j: usize| (pre[j][i + m].0 - pre[j][i].0, pre[j][i + m].1 - pre[j][i].1);
            let (a, b, c) = (sum(0), sum(1), sum(2));
            let th = big * (i as f64 - 0.5);
            let (ct, st) = (th.cos(), th.sin());
            let eb = (ct * b.0 + st * b.1, ct * b.1 - st * b.0);
            let ec = (ct * c.0 - st * c.1, ct * c.1 + st * c.0);
            let re = 0.5 * a.0 - 0.25 * (eb.0 + ec.0);
            let im = 0.5 * a.1 - 0.25 * (eb.1 + ec.1);
            (2.0 * (re * re + im * im).sqrt() / norm) as f32
        })
        .collect()
}

fn median_f32(v: &[f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut w = v.to_vec();
    w.sort_unstable_by(|a, b| a.total_cmp(b));
    w[w.len() / 2]
}

/// 山 `p` を含み、`half` 以上が続く範囲 [a, b]
fn above_run(env: &[f32], p: usize, half: f32) -> (usize, usize) {
    let (mut a, mut b) = (p, p);
    while a > 0 && env[a - 1] >= half {
        a -= 1;
    }
    while b + 1 < env.len() && env[b + 1] >= half {
        b += 1;
    }
    (a, b)
}

/// 半分の大きさを横切る位置（小数）。左は a−1 → a、右は b → b+1 の間を直線で補う
fn half_crossings(env: &[f32], a: usize, b: usize, half: f32) -> (f64, f64) {
    let xl = {
        let (lo, hi) = (env[a - 1] as f64, env[a] as f64);
        (a - 1) as f64 + if hi > lo { (half as f64 - lo) / (hi - lo) } else { 0.5 }
    };
    let xr = {
        let (hi, lo) = (env[b] as f64, env[b + 1] as f64);
        b as f64 + if hi > lo { (hi - half as f64) / (hi - lo) } else { 0.5 }
    };
    (xl, xr)
}

/// きれいなクリック（雑音の無いもの）の形。立ち上がりの決め方と「形が違う」の検査の物差し
#[derive(Clone, Copy, Debug, PartialEq)]
struct ClickShape {
    width: f64,
    mid_offset: f64,
    off_ratio: f32,
}

fn click_shape(rate: u32, freq_hz: f64, burst_ms: f64, m: usize) -> Option<ClickShape> {
    let pad = m + 8;
    let burst = click_burst(freq_hz, rate, burst_ms, CLICK_FADE_MS, 1.0);
    let mut x = vec![0f32; pad];
    x.extend_from_slice(&burst);
    x.resize(x.len() + pad, 0.0);
    let env = tone_envelope(&x, rate, freq_hz, m);
    let (p, peak) = env.iter().copied().enumerate().max_by(|a, b| a.1.total_cmp(&b.1))?;
    let half = peak / 2.0;
    let (a, b) = above_run(&env, p, half);
    if a == 0 || b + 1 >= env.len() {
        return None;
    }
    let (xl, xr) = half_crossings(&env, a, b, half);
    let mid_env = (xl + xr) / 2.0;
    let off = tone_envelope(&x, rate, freq_hz + 1000.0, m).get(mid_env.round() as usize).copied().unwrap_or(0.0);
    Some(ClickShape { width: xr - xl, mid_offset: mid_env + (m as f64 - 1.0) / 2.0 - pad as f64, off_ratio: off / peak })
}

/// 見つけたクリック
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Onset {
    /// クリックの最初の標本の位置（渡した並びの中の標本番号。小数）
    pub pos: f64,
    /// 大きさ（振幅。1.0 がフルスケール）
    pub peak: f32,
    /// 窓の中のその高さの大きさの中央値（雑音の目安）との比（dB）
    pub snr_db: f64,
}

/// クリックが取れなかった理由
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HitMiss {
    /// 狙った時刻の前後が録れていない
    NotCaptured,
    /// 前後の途中で録音が途切れた（取りこぼし・時刻の飛び）
    Gap,
    /// その高さの音が無い
    NoSignal,
    /// 雑音に対して小さい
    Weak,
    /// 窓の端で切れている
    Truncated,
    /// 長さがクリックと違う
    WrongShape,
    /// ほかの高さも同じくらい鳴っている（物音）
    NotTone,
    /// 同じ高さの山が2つ以上ある
    Ambiguous,
}

/// 録った音 `x`（モノラル。狙った時刻の前後だけ）から、高さ `freq_hz`・長さ `burst_ms` のクリックの立ち上がりを探す。
/// 1. 2ms のハン窓でその高さの大きさを出す 2. いちばん大きい山の半分を横切る左右の中心から、クリックの最初の標本を逆算する
/// 3. 断る：音が無い・小さい・窓の端で切れている・長さが違う・隣の高さも一緒に上がる（物音）・同じ高さの山がほかにもある
pub fn find_click_onset(x: &[f32], rate: u32, freq_hz: f64, burst_ms: f64) -> Result<Onset, HitMiss> {
    let m = tone_window_len(rate);
    let shape = click_shape(rate, freq_hz, burst_ms, m).ok_or(HitMiss::NoSignal)?;
    let env = tone_envelope(x, rate, freq_hz, m);
    if env.len() < 3 {
        return Err(HitMiss::NotCaptured);
    }
    let (p, peak) = env.iter().copied().enumerate().max_by(|a, b| a.1.total_cmp(&b.1)).ok_or(HitMiss::NoSignal)?;
    if !(peak >= ONSET_MIN_PEAK) {
        return Err(HitMiss::NoSignal);
    }
    let noise = median_f32(&env);
    let snr_db = 20.0 * (peak as f64 / (noise as f64).max(1e-12)).log10();
    if snr_db < ONSET_MIN_SNR_DB {
        return Err(HitMiss::Weak);
    }
    let half = peak / 2.0;
    let (a, b) = above_run(&env, p, half);
    if a == 0 || b + 1 >= env.len() {
        return Err(HitMiss::Truncated);
    }
    let (xl, xr) = half_crossings(&env, a, b, half);
    let width = xr - xl;
    let tol = (0.5e-3 * rate as f64).max(0.25 * shape.width);
    if (width - shape.width).abs() > tol {
        return Err(HitMiss::WrongShape);
    }
    let mid_env = (xl + xr) / 2.0;
    let off_env = tone_envelope(x, rate, freq_hz + 1000.0, m);
    let off_rise = off_env.get(mid_env.round() as usize).copied().unwrap_or(0.0) - median_f32(&off_env);
    if off_rise / peak > shape.off_ratio + ONSET_MAX_OFF_EXTRA {
        return Err(HitMiss::NotTone);
    }
    let burst_len = (burst_ms / 1000.0 * rate as f64).round() as usize;
    let guard = m + burst_len;
    let (lo, hi) = (a.saturating_sub(guard), (b + guard).min(env.len() - 1));
    let rival = env.iter().enumerate().filter(|(i, _)| *i < lo || *i > hi).map(|(_, v)| *v).fold(0f32, f32::max);
    if rival >= ONSET_MAX_RIVAL * peak {
        return Err(HitMiss::Ambiguous);
    }
    Ok(Onset { pos: mid_env + (m as f64 - 1.0) / 2.0 - shape.mid_offset, peak, snr_db })
}

// ----------------------------------------------------------------------
// 録った流れ（パケットと標本）
// ----------------------------------------------------------------------

/// 録った流れの1パケット（コールバック1回）
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CapPacket {
    /// この流れのフレームの通し番号。取りこぼしたパケットのぶんも進める（つながりの検査に使う）
    pub pos: u64,
    /// このパケットより前に、実際に手元へ渡したフレームの数（取りこぼしたぶんは数えない）
    pub stored_before: u64,
    pub frames: u32,
    /// cpal の入力の `capture`（WASAPI の GetBuffer が返す QPC。パケットの最初のフレームの時刻。ns）
    pub capture_ns: u64,
}

/// パケットの時刻のつながりを許す幅。これより飛んだら、そこで途切れたとみなす
pub const CAPTURE_GAP_TOL_NS: u64 = 1_000_000;

fn packet_end_ns(p: &CapPacket, rate: u32) -> f64 {
    p.capture_ns as f64 + p.frames as f64 * 1e9 / rate as f64
}

fn packets_connect(prev: &CapPacket, next: &CapPacket, rate: u32, tol_ns: u64) -> bool {
    next.pos == prev.pos + prev.frames as u64 && (next.capture_ns as f64 - packet_end_ns(prev, rate)).abs() <= tol_ns as f64
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowMiss {
    /// まだ録れていない（あとで来る）
    NotYet,
    NotCaptured,
    Gap,
}

/// 時刻 from〜to を覆うフレームの範囲。戻り値は（最初の通し番号, 数, 最初のフレームの stored_before での番号）。packets は時刻順
fn window_positions(packets: &[CapPacket], rate: u32, from_ns: u64, to_ns: u64, tol_ns: u64) -> Result<(u64, usize, u64), WindowMiss> {
    let last = packets.last().ok_or(WindowMiss::NotYet)?;
    if packet_end_ns(last, rate) < to_ns as f64 {
        return Err(WindowMiss::NotYet);
    }
    let i0 = packets.iter().position(|p| packet_end_ns(p, rate) > from_ns as f64).ok_or(WindowMiss::NotYet)?;
    let first = &packets[i0];
    if first.capture_ns > from_ns + tol_ns {
        return Err(WindowMiss::NotCaptured);
    }
    let mut i = i0;
    while packet_end_ns(&packets[i], rate) < to_ns as f64 {
        let next = packets.get(i + 1).ok_or(WindowMiss::NotYet)?;
        if !packets_connect(&packets[i], next, rate, tol_ns) {
            return Err(WindowMiss::Gap);
        }
        i += 1;
    }
    let frames_floor = |dt_ns: u64| (dt_ns as u128 * rate as u128 / 1_000_000_000) as u64;
    let frames_ceil = |dt_ns: u64| (dt_ns as u128 * rate as u128).div_ceil(1_000_000_000) as u64;
    let skip = frames_floor(from_ns.saturating_sub(first.capture_ns));
    let start = first.pos + skip.min(first.frames as u64);
    let lastp = &packets[i];
    let upto = frames_ceil(to_ns.saturating_sub(lastp.capture_ns)).min(lastp.frames as u64);
    let end = lastp.pos + upto;
    Ok((start, end.saturating_sub(start) as usize, first.stored_before + (start - first.pos)))
}

/// フレームの通し番号（小数）→ QPC の ns。そのフレームを含むパケットの時刻から数える
fn pos_to_ns(packets: &[CapPacket], pos: f64, rate: u32) -> Option<f64> {
    let p = packets.iter().rev().find(|p| p.pos as f64 <= pos)?;
    if pos >= (p.pos + p.frames as u64) as f64 {
        return None;
    }
    Some(p.capture_ns as f64 + (pos - p.pos as f64) * 1e9 / rate as f64)
}

/// 録った流れの、手元に残している所。古い所は捨てて、増え続けない
pub struct CaptureTrack {
    rate: u32,
    /// モノラルにした標本（stored_before の番号 `trimmed` から）
    samples: VecDeque<f32>,
    trimmed: u64,
    packets: VecDeque<CapPacket>,
    /// 記録用（実機の不具合の診断）：前のパケットから見込んだ時刻とのずれの最大（つながった所）と、途切れた回数
    last: Option<CapPacket>,
    jump_max_ns: f64,
    breaks: u64,
}

impl CaptureTrack {
    pub fn new(rate: u32) -> Self {
        Self { rate: rate.max(1), samples: VecDeque::new(), trimmed: 0, packets: VecDeque::new(), last: None, jump_max_ns: 0.0, breaks: 0 }
    }
    pub fn rate(&self) -> u32 {
        self.rate
    }
    pub fn add_packet(&mut self, p: CapPacket) {
        if let Some(prev) = self.last {
            if packets_connect(&prev, &p, self.rate, CAPTURE_GAP_TOL_NS) {
                self.jump_max_ns = self.jump_max_ns.max((p.capture_ns as f64 - packet_end_ns(&prev, self.rate)).abs());
            } else {
                self.breaks += 1;
            }
        }
        self.last = Some(p);
        self.packets.push_back(p);
    }
    /// 時刻の飛びの最大（ms。つながった所）と、途切れた回数
    pub fn stats(&self) -> (f64, u64) {
        (self.jump_max_ns / 1e6, self.breaks)
    }
    pub fn add_samples(&mut self, s: &[f32]) {
        self.samples.extend(s.iter().copied());
    }
    /// 何か録れているか（最初のパケットが来たか）
    pub fn started(&self) -> bool {
        !self.packets.is_empty()
    }
    /// 時刻 from〜to の標本と、最初の標本の通し番号
    pub fn window(&mut self, from_ns: u64, to_ns: u64) -> Result<(Vec<f32>, u64), WindowMiss> {
        let (start, n, stored) = window_positions(self.packets.make_contiguous(), self.rate, from_ns, to_ns, CAPTURE_GAP_TOL_NS)?;
        let i0 = stored.checked_sub(self.trimmed).ok_or(WindowMiss::NotCaptured)? as usize;
        if i0 + n > self.samples.len() {
            return Err(WindowMiss::NotYet);
        }
        Ok((self.samples.range(i0..i0 + n).copied().collect(), start))
    }
    pub fn pos_to_ns(&mut self, pos: f64) -> Option<f64> {
        pos_to_ns(self.packets.make_contiguous(), pos, self.rate)
    }
    /// `keep_from_ns` より前に終わるパケットと、その標本を捨てる（最後のパケットは残す）
    pub fn trim_before(&mut self, keep_from_ns: u64) {
        while self.packets.len() > 1 && packet_end_ns(&self.packets[0], self.rate) < keep_from_ns as f64 {
            self.packets.pop_front();
        }
        if let Some(front) = self.packets.front() {
            let drop_n = (front.stored_before.saturating_sub(self.trimmed) as usize).min(self.samples.len());
            self.samples.drain(..drop_n);
            self.trimmed += drop_n as u64;
        }
    }
}

/// コールバックの中で、録った音をモノラルにして輪へ渡す役。輪に空きが無ければ、そのパケットを丸ごと捨てて数える
/// （ロック・メモリの確保をしない）
pub struct CaptureFeed {
    pub samples: rtrb::Producer<f32>,
    pub packets: rtrb::Producer<CapPacket>,
    pub pos: u64,
    pub stored: u64,
    pub dropped: Arc<AtomicU64>,
}

impl CaptureFeed {
    pub fn push(&mut self, frames: usize, mono: impl Iterator<Item = f32>, capture_ns: u64) {
        if frames == 0 {
            return;
        }
        if self.samples.slots() >= frames && self.packets.slots() >= 1 {
            let rec = CapPacket { pos: self.pos, stored_before: self.stored, frames: frames as u32, capture_ns };
            for v in mono.take(frames) {
                let _ = self.samples.push(if v.is_finite() { v } else { 0.0 });
            }
            // 標本を先に書き、パケットの記録はあと（取り出す側はパケット → 標本の順に読むので、読んだパケットの標本は必ずそろう）
            let _ = self.packets.push(rec);
            self.stored += frames as u64;
        } else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        self.pos += frames as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// -30 dBFS（計測の道具と同じ大きさ）
    const AMP_M30: f32 = 0.031_622_776;

    fn lcg_noise(n: usize, amp: f32, seed: u64) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                ((s >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0) * amp
            })
            .collect()
    }

    fn add_click(x: &mut [f32], at: usize, freq: f64, rate: u32, amp: f32) {
        for (i, v) in click_burst(freq, rate, 5.0, CLICK_FADE_MS, amp).iter().enumerate() {
            if let Some(d) = x.get_mut(at + i) {
                *d += v;
            }
        }
    }

    fn add_sine(x: &mut [f32], freq: f64, rate: u32, amp: f32) {
        for (k, v) in x.iter_mut().enumerate() {
            *v += amp * (2.0 * std::f64::consts::PI * freq * k as f64 / rate as f64).sin() as f32;
        }
    }

    /// クリックの最初の標本を、48kHz・44.1kHz と3つの高さで指す（計測の道具と同じ確かめ）
    #[test]
    fn onset_is_found_where_the_click_starts() {
        for &rate in &[48_000u32, 44_100] {
            for f in [1000.0, 3000.0, 5000.0] {
                let mut x = vec![0f32; rate as usize * 2 / 5];
                let at = 7_123;
                add_click(&mut x, at, f, rate, AMP_M30);
                let o = find_click_onset(&x, rate, f, 5.0).unwrap_or_else(|e| panic!("{rate} {f}: {e:?}"));
                assert!((o.pos - at as f64).abs() < 0.2, "{rate} {f}: pos {}", o.pos);
                assert!((o.peak - AMP_M30).abs() < AMP_M30 * 0.05, "{rate} {f}: peak {}", o.peak);
            }
        }
    }

    /// 雑音と、どのクリックの高さでもない音（曲のつもり）が鳴っていても、3つの高さのクリックを見分ける
    #[test]
    fn onset_survives_noise_and_other_tones() {
        let rate = 48_000;
        let mut x = lcg_noise(19_200, 0.003, 7);
        add_sine(&mut x, 2000.0, rate, 0.01);
        add_sine(&mut x, 4000.0, rate, 0.01);
        add_click(&mut x, 9_000, 1000.0, rate, AMP_M30);
        add_click(&mut x, 9_010, 3000.0, rate, AMP_M30);
        add_click(&mut x, 8_990, 5000.0, rate, AMP_M30);
        for (f, at) in [(1000.0, 9_000.0), (3000.0, 9_010.0), (5000.0, 8_990.0)] {
            let o = find_click_onset(&x, rate, f, 5.0).unwrap_or_else(|e| panic!("{f}: {e:?}"));
            assert!((o.pos - at).abs() < 3.0, "{f}: pos {} want {at}", o.pos);
            assert!(o.snr_db > ONSET_MIN_SNR_DB);
        }
    }

    /// クリックでないものは断る：無音・ほかの高さだけ・同じ高さがずっと鳴る・端で切れる・長い音・2つある・物音
    #[test]
    fn onset_refuses_what_is_not_the_click() {
        let rate = 48_000;
        let n = 19_200;
        assert_eq!(find_click_onset(&vec![0f32; n], rate, 1000.0, 5.0), Err(HitMiss::NoSignal));
        let mut other = vec![0f32; n];
        add_click(&mut other, 9_000, 3000.0, rate, AMP_M30);
        assert!(find_click_onset(&other, rate, 1000.0, 5.0).is_err());
        let mut steady = vec![0f32; n];
        add_sine(&mut steady, 1000.0, rate, 0.01);
        add_click(&mut steady, 9_000, 1000.0, rate, AMP_M30);
        assert_eq!(find_click_onset(&steady, rate, 1000.0, 5.0), Err(HitMiss::Weak));
        let b = click_burst(1000.0, rate, 5.0, CLICK_FADE_MS, AMP_M30);
        let mut head_cut = vec![0f32; n];
        head_cut[..120].copy_from_slice(&b[120..]);
        assert_eq!(find_click_onset(&head_cut, rate, 1000.0, 5.0), Err(HitMiss::Truncated));
        let mut long = vec![0f32; n];
        let lb = click_burst(1000.0, rate, 50.0, CLICK_FADE_MS, AMP_M30);
        long[5_000..5_000 + lb.len()].copy_from_slice(&lb);
        assert_eq!(find_click_onset(&long, rate, 1000.0, 5.0), Err(HitMiss::WrongShape));
        let mut two = vec![0f32; n];
        add_click(&mut two, 5_000, 1000.0, rate, AMP_M30);
        add_click(&mut two, 9_800, 1000.0, rate, AMP_M30 * 0.8);
        assert_eq!(find_click_onset(&two, rate, 1000.0, 5.0), Err(HitMiss::Ambiguous));
        let mut thump = lcg_noise(n, 0.0003, 3);
        for (i, v) in lcg_noise(240, 0.3, 4).into_iter().enumerate() {
            thump[9_000 + i] += v;
        }
        assert!(find_click_onset(&thump, rate, 1000.0, 5.0).is_err());
    }

    /// コールバックの役 → 輪 → 手元 → 切り出し → 立ち上がりの QPC 時刻、を通しで。取りこぼしは途切れとして断る
    #[test]
    fn feed_and_track_give_click_time_and_see_gaps() {
        let rate = 48_000u32;
        let frames = 480usize;
        let (sp, mut sc) = rtrb::RingBuffer::<f32>::new(rate as usize * 4);
        let (pp, mut pc) = rtrb::RingBuffer::<CapPacket>::new(4096);
        let dropped = Arc::new(AtomicU64::new(0));
        let mut feed = CaptureFeed { samples: sp, packets: pp, pos: 0, stored: 0, dropped: dropped.clone() };
        let t0 = 5_000_000_000u64;
        let total = rate as usize * 2;
        let k0 = (1.2345 * rate as f64).round() as usize;
        let mut mono = vec![0f32; total];
        add_click(&mut mono, k0, 3000.0, rate, AMP_M30);
        let mut track = CaptureTrack::new(rate);
        for (i, chunk) in mono.chunks(frames).enumerate() {
            let pos = (i * frames) as u64;
            let cap = (t0 + (pos as f64 * 1e9 / rate as f64) as u64) / 100 * 100;
            feed.push(chunk.len(), chunk.iter().copied(), cap);
            while let Ok(p) = pc.pop() {
                track.add_packet(p);
            }
            let n = sc.slots();
            let c = sc.read_chunk(n).unwrap();
            let (a, b) = c.as_slices();
            track.add_samples(a);
            track.add_samples(b);
            c.commit_all();
            track.trim_before(t0 + 500_000_000);
        }
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
        let truth = t0 as f64 + k0 as f64 * 1e9 / rate as f64;
        let aimed = truth as u64 - 33_000_000; // 狙いより 33ms 遅れて届いた
        let (win, start) = track.window(aimed - 10_000_000, aimed + 400_000_000).unwrap();
        let o = find_click_onset(&win, rate, 3000.0, 5.0).unwrap();
        let ns = track.pos_to_ns(start as f64 + o.pos).unwrap();
        assert!((ns - truth).abs() < 5_000.0, "{} µs", (ns - truth) / 1e3);
        // まだ録れていない所・捨てた所
        assert_eq!(track.window(t0 + 1_900_000_000, t0 + 2_100_000_000).unwrap_err(), WindowMiss::NotYet);
        assert_eq!(track.window(t0, t0 + 10_000_000).unwrap_err(), WindowMiss::NotCaptured);
        // パケットを1つ取りこぼした流れ → 途切れ
        let mut gap = CaptureTrack::new(rate);
        for i in 0..20u64 {
            if i == 10 {
                continue;
            }
            gap.add_packet(CapPacket { pos: i * 480, stored_before: i * 480, frames: 480, capture_ns: t0 + i * 10_000_000 });
        }
        gap.add_samples(&vec![0.0; 20 * 480]);
        assert_eq!(gap.window(t0 + 50_000_000, t0 + 150_000_000).unwrap_err(), WindowMiss::Gap);
        assert_eq!(gap.stats(), (0.0, 1));
        assert_eq!(track.stats().1, 0);
        assert!(track.stats().0 < 0.001, "{:?}", track.stats());
    }

    /// 輪がいっぱいならパケットを丸ごと捨てて数える（コールバックは待たない）。数でない値は 0 にして渡す
    #[test]
    fn feed_drops_whole_packets_when_full() {
        let (sp, mut sc) = rtrb::RingBuffer::<f32>::new(1_000);
        let (pp, _pc) = rtrb::RingBuffer::<CapPacket>::new(16);
        let dropped = Arc::new(AtomicU64::new(0));
        let mut feed = CaptureFeed { samples: sp, packets: pp, pos: 0, stored: 0, dropped: dropped.clone() };
        feed.push(480, std::iter::repeat(f32::NAN), 1);
        feed.push(480, std::iter::repeat(0.5), 2);
        feed.push(480, std::iter::repeat(0.5), 3);
        assert_eq!(dropped.load(Ordering::Relaxed), 1);
        assert_eq!(feed.pos, 1_440);
        assert_eq!(feed.stored, 960);
        assert_eq!(sc.pop().unwrap(), 0.0);
    }
}
