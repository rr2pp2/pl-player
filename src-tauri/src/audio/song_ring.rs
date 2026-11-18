//! 曲の側の共有の輪（v3.8.0。仕様 13.1 の5）。
//!
//! デコードした曲を 2ch・曲のレートのまま溜め、出力ごとに「時計 − 遅らせ量」の位置から読む。
//! 大きさは「遅らせ量の上限 1000ms ＋ 先読み 500ms ＋ 余裕」。位置は曲の頭からのフレーム番号（絶対）で指す。
//! 出力のコールバックは触らない（音声スレッドだけが書いて読む）。だから普通の VecDeque でよい。

use std::collections::VecDeque;

use super::sync::tuning;

/// 曲の側の輪のチャンネル数（13.5 の7：曲の側は 2ch にそろえる）
pub const RING_CHANNELS: usize = 2;

#[derive(Debug)]
pub struct SongRing {
    rate: u32,
    buf: VecDeque<f32>,
    /// buf の先頭のフレーム番号
    start: i64,
    /// 曲を読み切ったら、その終わりのフレーム番号
    end: Option<i64>,
    cap_frames: usize,
}

impl SongRing {
    pub fn new(rate: u32, start_frame: i64) -> Self {
        let ms = (tuning::RING_DELAY_MS + tuning::RING_LOOKAHEAD_MS + tuning::RING_MARGIN_MS) as usize;
        let cap_frames = (rate.max(1) as usize * ms / 1000).max(4096);
        Self {
            rate,
            buf: VecDeque::with_capacity(cap_frames * RING_CHANNELS),
            start: start_frame,
            end: None,
            cap_frames,
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn capacity_frames(&self) -> usize {
        self.cap_frames
    }

    /// いちばん古く残っているフレーム
    pub fn start_frame(&self) -> i64 {
        self.start
    }

    /// 次に書くフレーム（書き終えた所）
    pub fn written_end(&self) -> i64 {
        self.start + (self.buf.len() / RING_CHANNELS) as i64
    }

    pub fn len_frames(&self) -> usize {
        self.buf.len() / RING_CHANNELS
    }

    pub fn free_frames(&self) -> usize {
        self.cap_frames.saturating_sub(self.len_frames())
    }

    pub fn eof(&self) -> bool {
        self.end.is_some()
    }

    pub fn end_frame(&self) -> Option<i64> {
        self.end
    }

    /// 曲を読み切った（ここで終わり）
    pub fn set_eof(&mut self) {
        self.end = Some(self.written_end());
    }

    /// シーク・曲を開く：空にして、`start_frame` から書き直す
    pub fn reset(&mut self, start_frame: i64) {
        self.buf.clear();
        self.start = start_frame;
        self.end = None;
    }

    /// 入るだけ書いて、書いたフレーム数を返す（`samples` は 2ch のインターリーブ）
    pub fn push(&mut self, samples: &[f32]) -> usize {
        if self.end.is_some() {
            return 0;
        }
        let frames = (samples.len() / RING_CHANNELS).min(self.free_frames());
        self.buf.extend(samples[..frames * RING_CHANNELS].iter().copied());
        frames
    }

    /// `frame` より前を捨てる（もう誰も読まない所）
    pub fn drop_before(&mut self, frame: i64) {
        if frame <= self.start {
            return;
        }
        let n = ((frame - self.start) as usize).min(self.len_frames());
        self.buf.drain(..n * RING_CHANNELS);
        self.start += n as i64;
    }

    /// `[from, from + n)` を読めるか。輪の外（曲の頭より前・捨てた所・終わりより後）は無音として読めるので、
    /// 読めないのは「まだデコードしていない」所だけ
    pub fn available(&self, from: i64, n: usize) -> bool {
        let last = from + n as i64;
        last <= self.written_end() || self.end.is_some()
    }

    /// `[from, from + n)` を `out` へ（2ch のインターリーブ）。輪の外は無音
    pub fn read(&self, from: i64, n: usize, out: &mut Vec<f32>) {
        out.clear();
        out.resize(n * RING_CHANNELS, 0.0);
        let we = self.written_end();
        let lo = from.max(self.start);
        let hi = (from + n as i64).min(we);
        if lo >= hi {
            return;
        }
        let src = ((lo - self.start) as usize) * RING_CHANNELS;
        let dst = ((lo - from) as usize) * RING_CHANNELS;
        let len = ((hi - lo) as usize) * RING_CHANNELS;
        let (a, b) = self.buf.as_slices();
        for i in 0..len {
            let s = src + i;
            out[dst + i] = if s < a.len() { a[s] } else { b[s - a.len()] };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(from: i64, n: usize) -> Vec<f32> {
        (0..n).flat_map(|i| {
            let v = (from + i as i64) as f32;
            [v, -v]
        }).collect()
    }

    /// 輪の大きさ：遅らせ量の上限 1000ms ＋ 先読み 500ms ＋ 余裕 500ms ＝ 2 秒ぶん
    #[test]
    fn capacity_holds_a_second_of_delay_plus_lookahead() {
        assert_eq!(tuning::RING_DELAY_MS, 1000);
        let r = SongRing::new(48_000, 0);
        assert_eq!(r.capacity_frames(), 96_000);
        assert!(r.capacity_frames() as u32 >= 48_000 * (tuning::RING_DELAY_MS + tuning::RING_LOOKAHEAD_MS) / 1000);
    }

    /// 書いた所を絶対のフレーム番号で読める。輪の外（頭より前・捨てた所・終わりより後）は無音。
    /// まだデコードしていない所は読めない
    #[test]
    fn read_by_absolute_frame_with_silence_outside() {
        let mut r = SongRing::new(1000, 100);
        assert_eq!(r.push(&frames(100, 50)), 50);
        assert_eq!(r.written_end(), 150);
        let mut out = vec![];
        r.read(120, 3, &mut out);
        assert_eq!(out, vec![120.0, -120.0, 121.0, -121.0, 122.0, -122.0]);
        // 頭より前（遅らせた出力が曲の頭より前を読む）は無音
        r.read(98, 4, &mut out);
        assert_eq!(out, vec![0.0, 0.0, 0.0, 0.0, 100.0, -100.0, 101.0, -101.0]);
        // まだデコードしていない
        assert!(r.available(140, 10));
        assert!(!r.available(140, 11));
        // 読み切れば、終わりより後は無音として読める
        r.set_eof();
        assert!(r.available(140, 100));
        r.read(149, 2, &mut out);
        assert_eq!(out, vec![149.0, -149.0, 0.0, 0.0]);
        assert_eq!(r.push(&frames(150, 1)), 0);
    }

    /// いっぱいなら入る分だけ。古い所を捨てると、そのぶん書ける。捨てた所は無音
    #[test]
    fn push_until_full_and_drop_old() {
        let mut r = SongRing::new(1000, 0);
        let cap = r.capacity_frames();
        assert_eq!(r.push(&frames(0, cap + 10)), cap);
        assert_eq!(r.free_frames(), 0);
        r.drop_before(100);
        assert_eq!(r.start_frame(), 100);
        assert_eq!(r.free_frames(), 100);
        assert_eq!(r.push(&frames(cap as i64, 200)), 100);
        let mut out = vec![];
        r.read(99, 2, &mut out);
        assert_eq!(out, vec![0.0, 0.0, 100.0, -100.0]);
        // 前へは戻らない・書いた所より先までは捨てない
        r.drop_before(50);
        assert_eq!(r.start_frame(), 100);
        r.drop_before(1_000_000);
        assert_eq!(r.len_frames(), 0);
        assert_eq!(r.start_frame(), r.written_end());
    }

    /// シーク：空にして、新しい位置から書き直す
    #[test]
    fn reset_moves_the_start() {
        let mut r = SongRing::new(1000, 0);
        r.push(&frames(0, 10));
        r.set_eof();
        r.reset(5000);
        assert!(!r.eof());
        assert_eq!(r.written_end(), 5000);
        assert_eq!(r.push(&frames(5000, 2)), 2);
        let mut out = vec![];
        r.read(5001, 1, &mut out);
        assert_eq!(out, vec![5001.0, -5001.0]);
    }
}
