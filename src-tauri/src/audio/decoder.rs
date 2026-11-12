//! デコード（Symphonia 0.6 ＋ libopus）。
//!
//! 1曲を開いて、f32 のインターリーブで取り出す。シークは「指定位置より前のフレームを捨てる」ところまで行う。

use std::fs::File;
use std::path::Path;
use std::sync::OnceLock;

use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase, Timestamp};

use crate::error::{AppError, ErrorKind};

/// Symphonia の標準のデコーダに、libopus のデコーダを足した一覧
fn codecs() -> &'static CodecRegistry {
    static REG: OnceLock<CodecRegistry> = OnceLock::new();
    REG.get_or_init(|| {
        let mut reg = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut reg);
        reg.register_audio_decoder::<symphonia_adapter_libopus::OpusDecoder>();
        reg
    })
}

pub struct Decoder {
    format: Box<dyn FormatReader + 'static>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    pub sample_rate: u32,
    pub channels: usize,
    pub duration_ms: u64,
    /// シーク直後：この時刻（トラックの時間単位）より前のサンプルを捨てる
    skip_until: Option<Timestamp>,
    scratch: Vec<f32>,
}

fn map_err(e: SymError, path: &Path) -> AppError {
    match e {
        SymError::IoError(io) => {
            let kind = match io.kind() {
                std::io::ErrorKind::NotFound => ErrorKind::FileNotFound,
                std::io::ErrorKind::PermissionDenied => ErrorKind::AccessDenied,
                _ => ErrorKind::PlaybackFailed,
            };
            AppError::new(kind, format!("{}: {io}", path.display()))
        }
        SymError::Unsupported(what) => {
            AppError::new(ErrorKind::UnsupportedFormat, format!("{}: {what}", path.display()))
        }
        other => AppError::new(ErrorKind::PlaybackFailed, format!("{}: {other}", path.display())),
    }
}

impl Decoder {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        let file = File::open(path).map_err(|e| AppError::from_io(&e, path))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| match e {
                SymError::IoError(_) => map_err(e, path),
                _ => AppError::new(
                    ErrorKind::UnsupportedFormat,
                    format!("{}: {e}", path.display()),
                ),
            })?;
        let track = format.default_track(TrackType::Audio).ok_or_else(|| {
            AppError::new(
                ErrorKind::UnsupportedFormat,
                format!("{}: no audio track", path.display()),
            )
        })?;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .ok_or_else(|| {
                AppError::new(
                    ErrorKind::UnsupportedFormat,
                    format!("{}: no audio codec parameters", path.display()),
                )
            })?
            .clone();
        let track_id = track.id;
        let time_base = track.time_base;
        let num_frames = track.num_frames;
        let decoder = codecs()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
            .map_err(|e| match e {
                SymError::Unsupported(_) => AppError::new(
                    ErrorKind::UnsupportedFormat,
                    format!("{}: {e}", path.display()),
                ),
                other => map_err(other, path),
            })?;
        // v3.1.1（ペネトレーションテスト v3.0.2 の2）：サンプルレート 0 のファイル（細工した WAV など）は、
        // 下の長さの計算や変換で 0 で割って panic していた。0 は使える値として扱わず、鳴らせないので非対応形式にする
        let sample_rate = match params.sample_rate {
            Some(0) => {
                return Err(AppError::new(
                    ErrorKind::UnsupportedFormat,
                    format!("{}: sample rate is 0", path.display()),
                ))
            }
            Some(r) => r,
            None => 44_100,
        };
        let channels = params.channels.as_ref().map(|c| c.count()).unwrap_or(2).max(1);
        let duration_ms = match (num_frames, time_base) {
            (Some(n), Some(tb)) => tb
                .calc_time(Timestamp::new(n as i64))
                .map(|t| (t.as_secs_f64() * 1000.0) as u64)
                .unwrap_or(0),
            (Some(n), None) => n * 1000 / sample_rate as u64,
            _ => 0,
        };
        Ok(Self {
            format,
            decoder,
            track_id,
            time_base,
            sample_rate,
            channels,
            duration_ms,
            skip_until: None,
            scratch: Vec::new(),
        })
    }

    /// 次の塊をデコードして `out` に足す（インターリーブ、f32）。
    /// 戻り値：Ok(true)＝続きがある／Ok(false)＝曲の終わり。
    pub fn decode_next(&mut self, out: &mut Vec<f32>) -> Result<bool, AppError> {
        // 壊れたパケットは数回までは飛ばす
        let mut bad = 0;
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(false),
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(false)
                }
                Err(SymError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(e) => {
                    return Err(AppError::new(ErrorKind::PlaybackFailed, e.to_string()));
                }
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let pts = packet.pts;
            match self.decoder.decode(&packet) {
                Ok(buf) => {
                    let ch = buf.spec().channels().count().max(1);
                    let n = buf.samples_interleaved();
                    if n == 0 {
                        continue;
                    }
                    self.scratch.resize(n, 0.0);
                    buf.copy_to_slice_interleaved(&mut self.scratch[..]);
                    // デコーダが途中でチャンネル数を変えたとき（まれ）は合わせる
                    if ch != self.channels {
                        self.channels = ch;
                    }
                    let mut start = 0usize;
                    if let Some(until) = self.skip_until {
                        if pts.get() + (n / ch) as i64 <= until.get() {
                            // まるごと目標より前
                            continue;
                        }
                        if pts.get() < until.get() {
                            start = ((until.get() - pts.get()) as usize) * ch;
                        }
                        self.skip_until = None;
                    }
                    out.extend_from_slice(&self.scratch[start.min(n)..n]);
                    return Ok(true);
                }
                Err(SymError::DecodeError(_)) => {
                    bad += 1;
                    if bad > 20 {
                        return Err(AppError::new(
                            ErrorKind::PlaybackFailed,
                            "too many decode errors",
                        ));
                    }
                    continue;
                }
                Err(SymError::Unsupported(what)) => {
                    return Err(AppError::new(ErrorKind::UnsupportedFormat, what));
                }
                Err(e) => return Err(AppError::new(ErrorKind::PlaybackFailed, e.to_string())),
            }
        }
    }

    /// 指定の位置（ミリ秒）へ動かす。実際に動いた位置（ミリ秒）を返す。
    pub fn seek(&mut self, ms: u64) -> Result<u64, AppError> {
        let time = Time::try_from_secs_f64(ms as f64 / 1000.0).unwrap_or_default();
        let res = self.format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time,
                track_id: Some(self.track_id),
            },
        );
        self.decoder.reset();
        match res {
            Ok(seeked) => {
                // 目標の時刻までのサンプルは decode_next で捨てる。
                // トラックの時間単位がサンプル単位（1/rate）でないときは、ずれを許して捨てない。
                let per_sample = self
                    .time_base
                    .map(|tb| tb.calc_time_saturating(Timestamp::new(self.sample_rate as i64)))
                    .map(|t| (t.as_secs_f64() - 1.0).abs() < 1e-6)
                    .unwrap_or(false);
                self.skip_until = if per_sample {
                    Some(seeked.required_ts)
                } else {
                    None
                };
                Ok(ms)
            }
            Err(e) => Err(AppError::new(ErrorKind::PlaybackFailed, format!("seek: {e}"))),
        }
    }
}

/// テスト用：フィクスチャの sine.wav の fmt のサンプルレート（とバイトレート）を 0 にした WAV を `dir` に作る
/// （ペネトレーションテスト v3.0.2 の2）
#[cfg(test)]
pub(crate) fn test_zero_rate_wav(dir: &Path) -> std::path::PathBuf {
    let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("sine.wav");
    let mut b = std::fs::read(src).unwrap();
    let at = b.windows(4).position(|w| w == b"fmt ").expect("fmt chunk") + 8;
    assert_eq!(u32::from_le_bytes(b[at + 4..at + 8].try_into().unwrap()), 44_100);
    b[at + 4..at + 8].copy_from_slice(&0u32.to_le_bytes()); // sample rate
    b[at + 8..at + 12].copy_from_slice(&0u32.to_le_bytes()); // byte rate
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join("zero_rate.wav");
    std::fs::write(&p, &b).unwrap();
    p
}

#[cfg(test)]
pub(crate) fn decode_all_frames(path: &Path) -> Result<(u64, u32, usize), AppError> {
    let mut d = Decoder::open(path)?;
    let mut buf = Vec::new();
    let mut frames = 0u64;
    loop {
        buf.clear();
        let more = d.decode_next(&mut buf)?;
        frames += (buf.len() / d.channels) as u64;
        if !more {
            break;
        }
    }
    Ok((frames, d.sample_rate, d.channels))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)
    }

    /// 原文の7形式（M4A は AAC-LC と ALAC の2種）が最後までデコードでき、長さが約3秒になる
    #[test]
    fn seven_formats_decode() {
        let cases = [
            "sine.mp3",
            "sine.wav",
            "sine.flac",
            "sine_aaclc.m4a",
            "sine_alac.m4a",
            "sine.aac",
            "sine.ogg",
            "sine.opus",
        ];
        for name in cases {
            let p = fixture(name);
            if !p.exists() {
                panic!("fixture missing: {} (run scripts/make-test-audio.sh)", p.display());
            }
            let (frames, rate, ch) = decode_all_frames(&p).unwrap_or_else(|e| panic!("{name}: {e}"));
            let secs = frames as f64 / rate as f64;
            println!("{name}: {frames} frames @ {rate} Hz x{ch} = {secs:.3}s");
            assert!((2.9..=3.2).contains(&secs), "{name}: {secs}s");
            assert_eq!(ch, 2, "{name}");
        }
    }

    #[test]
    fn duration_is_known_for_seekable_formats() {
        for name in ["sine.mp3", "sine.wav", "sine.flac", "sine_aaclc.m4a", "sine.ogg", "sine.opus"] {
            let d = Decoder::open(&fixture(name)).unwrap();
            assert!((2900..=3200).contains(&d.duration_ms), "{name}: {}", d.duration_ms);
        }
    }

    #[test]
    fn seek_then_decode_rest() {
        for name in ["sine.mp3", "sine.flac", "sine_aaclc.m4a", "sine_alac.m4a", "sine.aac", "sine.ogg", "sine.opus", "sine.wav"] {
            let mut d = Decoder::open(&fixture(name)).unwrap();
            d.seek(2000).unwrap();
            let mut buf = vec![];
            let mut frames = 0u64;
            loop {
                buf.clear();
                let more = d.decode_next(&mut buf).unwrap();
                frames += (buf.len() / d.channels) as u64;
                if !more {
                    break;
                }
            }
            let secs = frames as f64 / d.sample_rate as f64;
            println!("{name}: after seek(2000) {secs:.3}s left");
            assert!((0.8..=1.25).contains(&secs), "{name}: {secs}s left after seek");
        }
    }

    #[test]
    fn broken_file_is_unsupported() {
        let e = Decoder::open(&fixture("broken.mp3")).err().expect("must fail");
        assert_eq!(e.kind, ErrorKind::UnsupportedFormat);
    }

    #[test]
    fn missing_file_is_not_found() {
        let e = Decoder::open(&fixture("no_such_file.mp3")).err().expect("must fail");
        assert_eq!(e.kind, ErrorKind::FileNotFound);
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の2）：サンプルレート 0 の WAV は panic せず非対応形式
    #[test]
    fn zero_sample_rate_is_unsupported_not_a_panic() {
        let dir = crate::test_tmp().join(format!("plp-zero-rate-{}", std::process::id()));
        let p = test_zero_rate_wav(&dir);
        let r = std::panic::catch_unwind(|| Decoder::open(&p).err().map(|e| e.kind));
        assert_eq!(r.expect("Decoder::open panicked"), Some(ErrorKind::UnsupportedFormat));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
