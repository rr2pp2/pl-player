//! サンプルレート変換（rubato）と、チャンネル数の合わせ込み。
//!
//! デコードしたレートと出力先のレートが違うときだけ変換器を挟む。
//! 出力先が変わってレートが変わったら、作り直す（engine 側の責務）。

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async, FixedAsync, Resampler as _, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};

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
pub fn remap_channels(input: &[f32], in_ch: usize, out_ch: usize, out: &mut Vec<f32>) {
    if in_ch == out_ch {
        out.extend_from_slice(input);
        return;
    }
    let frames = input.len() / in_ch.max(1);
    out.reserve(frames * out_ch);
    for f in 0..frames {
        let frame = &input[f * in_ch..(f + 1) * in_ch];
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
}
