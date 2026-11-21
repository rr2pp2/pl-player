//! サンプルレート変換（rubato）と、チャンネル数の合わせ込み。
//!
//! - `Converter`：デコードした曲を、曲の側の輪（2ch・曲を開いたときのレート）へそろえる。レートが同じなら通さない
//! - `StretchConverter`（v3.8.0。仕様 13.1 の4）：曲の側の輪から出力へ。出力ごとに1つ持ち、常に通す。
//!   同じレートは `rubato::Slip`（倍率 1.0 なら入力そのまま）、違うレートは `Async`（倍率の上限 ±0.1%）。
//!   倍率を少しずつ動かして、出力ごとの時計のずれを消す

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Adjustable as _, Async, FixedAsync, Resampler as _, SincInterpolationParameters, SincInterpolationType, Slip,
    WindowFunction,
};

use super::sync::{clamp_ratio, tuning};

const CHUNK: usize = 1024;

pub struct Converter {
    in_rate: u32,
    out_rate: u32,
    in_ch: usize,
    out_ch: usize,
    resampler: Option<Async<f32>>,
    /// 変換器に渡す前の、チャンネルを合わせ済みの入力（インターリーブ）
    pending: Vec<f32>,
    outbuf: Vec<f32>,
}

impl Converter {
    pub fn new(in_rate: u32, in_ch: usize, out_rate: u32, out_ch: usize) -> Self {
        let resampler = if in_rate != out_rate {
            let params = SincInterpolationParameters::new(128, WindowFunction::Blackman2)
                .oversampling_factor(128)
                .interpolation(SincInterpolationType::Linear);
            Async::<f32>::new_sinc(
                out_rate as f64 / in_rate as f64,
                1.0,
                &params,
                CHUNK,
                out_ch,
                FixedAsync::Input,
            )
            .ok()
        } else {
            None
        };
        Self {
            in_rate,
            out_rate,
            in_ch,
            out_ch,
            resampler,
            pending: Vec::new(),
            outbuf: Vec::new(),
        }
    }

    pub fn matches(&self, in_rate: u32, in_ch: usize, out_rate: u32, out_ch: usize) -> bool {
        self.in_rate == in_rate && self.in_ch == in_ch && self.out_rate == out_rate && self.out_ch == out_ch
    }

    /// シークのとき：内部に溜まったものを捨てる
    pub fn reset(&mut self) {
        self.pending.clear();
        if let Some(r) = self.resampler.as_mut() {
            r.reset();
        }
    }

    /// 入力（in_ch のインターリーブ）を受け取り、変換できた分を `out`（out_ch のインターリーブ）へ足す。
    /// `flush` が真なら、端数も無音で埋めて出し切る（曲の終わり）。
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>, flush: bool) {
        remap_channels(input, self.in_ch, self.out_ch, &mut self.pending);
        let Some(rs) = self.resampler.as_mut() else {
            out.extend_from_slice(&self.pending);
            self.pending.clear();
            return;
        };
        let ch = self.out_ch;
        loop {
            let need = rs.input_frames_next();
            let have = self.pending.len() / ch;
            if have < need && !(flush && have > 0) {
                break;
            }
            let frames_in = have.min(need);
            let out_frames = rs.output_frames_max();
            self.outbuf.resize(out_frames * ch, 0.0);
            let input_adapter = match InterleavedSlice::new(&self.pending[..], ch, have) {
                Ok(a) => a,
                Err(_) => break,
            };
            let mut output_adapter = match InterleavedSlice::new_mut(&mut self.outbuf[..], ch, out_frames) {
                Ok(a) => a,
                Err(_) => break,
            };
            let indexing = rubato::Indexing {
                input_offset: 0,
                output_offset: 0,
                partial_len: if frames_in < need { Some(frames_in) } else { None },
                active_channels_mask: None,
            };
            match rs.process_into_buffer(&input_adapter, &mut output_adapter, Some(&indexing)) {
                Ok((used, produced)) => {
                    out.extend_from_slice(&self.outbuf[..produced * ch]);
                    let used = used.min(have);
                    self.pending.drain(..used * ch);
                    if used == 0 {
                        break;
                    }
                }
                Err(_) => {
                    // 変換に失敗したら、その塊はそのまま流す（音が途切れるよりよい）
                    out.extend_from_slice(&self.pending);
                    self.pending.clear();
                    break;
                }
            }
        }
    }
}

/// チャンネル数を合わせる。モノラル→複数は複製、2ch より多い→2ch は前の2つ、足りないぶんは無音。
/// v3.8.0（仕様 2.6）：複数→モノラルは前の2つを足して半分（3.7.0 までは左だけで、右の音が消えていた）
pub fn remap_channels(input: &[f32], in_ch: usize, out_ch: usize, out: &mut Vec<f32>) {
    if in_ch == out_ch {
        out.extend_from_slice(input);
        return;
    }
    let frames = input.len() / in_ch.max(1);
    out.reserve(frames * out_ch);
    for f in 0..frames {
        let frame = &input[f * in_ch..(f + 1) * in_ch];
        if out_ch == 1 {
            out.push((frame[0] + frame[1]) * 0.5);
            continue;
        }
        for c in 0..out_ch {
            let v = if in_ch == 1 {
                frame[0]
            } else if c < in_ch {
                frame[c]
            } else {
                0.0
            };
            out.push(v);
        }
    }
}

/// 曲の側の輪（2ch）から出力へ流すときの、倍率を動かせる変換器（v3.8.0）
pub struct StretchConverter {
    in_rate: u32,
    out_rate: u32,
    kind: Stretch,
    /// 名目の倍率（出力フレーム / 入力フレーム）
    nominal: f64,
    ratio: f64,
    outbuf: Vec<f32>,
}

enum Stretch {
    Slip(Slip<f32>),
    Async(Box<Async<f32>>),
}

/// `StretchConverter` が1回に読む入力のフレーム数
pub const STRETCH_CHUNK: usize = 1024;
const STRETCH_CH: usize = 2;

impl StretchConverter {
    pub fn new(in_rate: u32, out_rate: u32) -> Option<Self> {
        let nominal = out_rate as f64 / in_rate.max(1) as f64;
        let kind = if in_rate == out_rate {
            Stretch::Slip(Slip::<f32>::new(STRETCH_CHUNK, STRETCH_CH, FixedAsync::Input).ok()?)
        } else {
            let params = SincInterpolationParameters::new(128, WindowFunction::Blackman2)
                .oversampling_factor(128)
                .interpolation(SincInterpolationType::Linear);
            Stretch::Async(Box::new(
                Async::<f32>::new_sinc(nominal, tuning::MAX_RATIO_REL, &params, STRETCH_CHUNK, STRETCH_CH, FixedAsync::Input)
                    .ok()?,
            ))
        };
        let mut c = Self { in_rate, out_rate, kind, nominal, ratio: nominal, outbuf: Vec::new() };
        let max = c.output_frames_max();
        c.outbuf.resize(max * STRETCH_CH, 0.0);
        Some(c)
    }

    pub fn matches(&self, in_rate: u32, out_rate: u32) -> bool {
        self.in_rate == in_rate && self.out_rate == out_rate
    }

    pub fn nominal(&self) -> f64 {
        self.nominal
    }

    pub fn ratio(&self) -> f64 {
        self.ratio
    }

    pub fn input_frames_next(&self) -> usize {
        match &self.kind {
            Stretch::Slip(r) => r.input_frames_next(),
            Stretch::Async(r) => r.input_frames_next(),
        }
    }

    pub fn output_frames_max(&self) -> usize {
        match &self.kind {
            Stretch::Slip(r) => r.output_frames_max(),
            Stretch::Async(r) => r.output_frames_max(),
        }
    }

    /// 入力の出来事が出力に出るまでの遅れ（出力フレーム）。門を開いた直後はこの数だけ捨てる
    pub fn output_delay(&self) -> usize {
        match &self.kind {
            Stretch::Slip(r) => r.output_delay(),
            Stretch::Async(r) => r.output_delay(),
        }
    }

    /// 倍率を変える（上限の中へ収める）。実際に入れた倍率を返す
    pub fn set_ratio(&mut self, ratio: f64) -> f64 {
        let r = clamp_ratio(ratio, self.nominal);
        let ok = match &mut self.kind {
            Stretch::Slip(s) => s.set_resample_ratio(r, false).is_ok(),
            Stretch::Async(a) => a.set_resample_ratio(r, false).is_ok(),
        };
        if ok {
            self.ratio = r;
        }
        self.ratio
    }

    /// 名目の倍率に戻し、中に溜まったものを捨てる（門を開き直すとき）
    pub fn reset(&mut self) {
        match &mut self.kind {
            Stretch::Slip(s) => s.reset(),
            Stretch::Async(a) => a.reset(),
        }
        self.ratio = self.nominal;
        let n = self.nominal;
        let _ = self.set_ratio(n);
    }

    /// `input`（2ch・`input_frames_next` フレーム以上）の頭から変換し、`out` の末尾へ足す。（使った入力, 出した出力）
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) -> (usize, usize) {
        let need = self.input_frames_next();
        let have = input.len() / STRETCH_CH;
        if have < need {
            return (0, 0);
        }
        let out_frames = self.output_frames_max();
        if self.outbuf.len() < out_frames * STRETCH_CH {
            self.outbuf.resize(out_frames * STRETCH_CH, 0.0);
        }
        let Ok(input_adapter) = InterleavedSlice::new(input, STRETCH_CH, have) else {
            return (0, 0);
        };
        let Ok(mut output_adapter) = InterleavedSlice::new_mut(&mut self.outbuf[..], STRETCH_CH, out_frames) else {
            return (0, 0);
        };
        let r = match &mut self.kind {
            Stretch::Slip(s) => s.process_into_buffer(&input_adapter, &mut output_adapter, None),
            Stretch::Async(a) => a.process_into_buffer(&input_adapter, &mut output_adapter, None),
        };
        match r {
            Ok((used, produced)) => {
                out.extend_from_slice(&self.outbuf[..produced * STRETCH_CH]);
                (used, produced)
            }
            Err(_) => {
                // 変換に失敗したら、その塊はそのまま流す（音が途切れるよりよい）
                out.extend_from_slice(&input[..need * STRETCH_CH]);
                (need, need)
            }
        }
    }
}

/// 2ch を出力のチャンネル数へ広げる（13.5 の7）：1ch は (L+R)/2・2ch はそのまま・3ch 以上は 0 番・1 番に左右、残りは無音
pub fn spread_stereo(input: &[f32], out_ch: usize, out: &mut Vec<f32>) {
    remap_channels(input, 2, out_ch.max(1), out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remap_mono_to_stereo() {
        let mut out = vec![];
        remap_channels(&[0.1, 0.2], 1, 2, &mut out);
        assert_eq!(out, vec![0.1, 0.1, 0.2, 0.2]);
    }

    #[test]
    fn remap_six_to_two() {
        let mut out = vec![];
        remap_channels(&[1., 2., 3., 4., 5., 6.], 6, 2, &mut out);
        assert_eq!(out, vec![1., 2.]);
    }

    #[test]
    fn resample_length_is_about_ratio() {
        let mut c = Converter::new(44_100, 2, 48_000, 2);
        let input = vec![0.0f32; 44_100 * 2];
        let mut out = vec![];
        c.process(&input, &mut out, true);
        let frames = out.len() / 2;
        assert!((47_000..=49_500).contains(&frames), "frames = {frames}");
    }

    #[test]
    fn same_rate_passes_through() {
        let mut c = Converter::new(48_000, 2, 48_000, 2);
        let mut out = vec![];
        c.process(&[0.5; 10], &mut out, false);
        assert_eq!(out.len(), 10);
    }

    /// v3.8.0（仕様 2.6）：モノラルの出力には左右を足して半分。2ch → 3ch 以上は 0・1 番に左右、残りは無音
    #[test]
    fn stereo_to_mono_and_surround() {
        let mut out = vec![];
        spread_stereo(&[1.0, 0.0, 0.5, -0.5], 1, &mut out);
        assert_eq!(out, vec![0.5, 0.0]);
        out.clear();
        spread_stereo(&[0.25, -0.75], 6, &mut out);
        assert_eq!(out, vec![0.25, -0.75, 0.0, 0.0, 0.0, 0.0]);
        out.clear();
        spread_stereo(&[0.25, -0.75], 2, &mut out);
        assert_eq!(out, vec![0.25, -0.75]);
    }

    /// 同じレートは Slip：倍率 1.0 なら入力そのまま（基準の出力は伸び縮みさせない＝3.7.0 と同じ音）
    #[test]
    fn stretch_same_rate_is_identity_at_ratio_one() {
        let mut c = StretchConverter::new(48_000, 48_000).unwrap();
        assert_eq!(c.output_delay(), 0);
        let n = c.input_frames_next();
        let input: Vec<f32> = (0..n * 2).map(|i| (i as f32 * 0.001).sin()).collect();
        let mut out = vec![];
        let (used, produced) = c.process(&input, &mut out);
        assert_eq!((used, produced), (n, n));
        assert_eq!(out, input);
    }

    /// 倍率を上げると出力が増える。上限（±0.1%）の外は切る。長く流すと、出力 / 入力 が倍率に近づく。
    /// 約 200 万フレーム流して 3ppm（1時間で 11ms）以内（：200ppm から締めた。
    /// 残りの差は変換器の中に溜まる数フレームで、流す長さに比例しない）
    #[test]
    fn stretch_ratio_is_clamped_and_realized() {
        for (in_rate, out_rate) in [(48_000u32, 48_000u32), (44_100, 48_000)] {
            let mut c = StretchConverter::new(in_rate, out_rate).unwrap();
            let nominal = out_rate as f64 / in_rate as f64;
            let got = c.set_ratio(nominal * 1.01);
            assert!((got - nominal * tuning::MAX_RATIO_REL).abs() < 1e-12, "{got}");
            let target = nominal * 1.0008;
            c.set_ratio(target);
            let input = vec![0.1f32; STRETCH_CHUNK * 2];
            let (mut ins, mut outs) = (0usize, 0usize);
            let mut out = vec![];
            for _ in 0..2000 {
                out.clear();
                let (u, p) = c.process(&input, &mut out);
                ins += u;
                outs += p;
            }
            let realized = outs as f64 / ins as f64;
            assert!((realized / target - 1.0).abs() < 3e-6, "{in_rate}->{out_rate}: {realized} vs {target}");
            c.reset();
            assert_eq!(c.ratio(), nominal);
        }
    }
}
