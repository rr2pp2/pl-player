//! デコード（Symphonia 0.6 ＋ libopus。v3.1.2 から、Symphonia が読めない AAC だけ Windows の Media Foundation）。
//!
//! 1曲を開いて、f32 のインターリーブで取り出す。シークは「指定位置より前のフレームを捨てる」ところまで行う。
//!
//! v3.1.2：HE-AAC（SBR・PS）の m4a は Symphonia が「aac too complex」で断る。AAC の曲で Symphonia のデコーダが作れないときだけ、
//! `platform::windows::mf_audio`（Media Foundation）で開く。外から見た形（`open`・`decode_next`・`seek`・`sample_rate` など）は同じ。
//! Media Foundation の読み手はスレッドをまたげないので、`Decoder` は作ったスレッドの中だけで使う
//! （音声のスレッド・取り込みのスレッド・タグの書き込みの確かめ。どれも今もそうなっている。`Send` でないのでコンパイラが確かめる）

use std::fs::File;
use std::path::Path;
use std::sync::OnceLock;

use symphonia::core::codecs::audio::well_known::CODEC_ID_AAC;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase, Timestamp};

use crate::error::{AppError, ErrorKind};
use crate::platform::windows::mf_audio::MfReader;

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
    backend: Backend,
    pub sample_rate: u32,
    pub channels: usize,
    pub duration_ms: u64,
}

/// 実際に読む部品
enum Backend {
    Symphonia(SymphoniaTrack),
    /// v3.1.2：Symphonia が読めない AAC（HE-AAC など）
    MediaFoundation(MfReader),
}

struct SymphoniaTrack {
    format: Box<dyn FormatReader + 'static>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
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
        let decoder = match codecs().make_audio_decoder(&params, &AudioDecoderOptions::default()) {
            Ok(d) => d,
            // v3.1.2：AAC なのに Symphonia がデコーダを作れない（HE-AAC など）ときだけ、Media Foundation で開く
            Err(e) if params.codec == CODEC_ID_AAC => {
                drop(format);
                return Self::open_media_foundation(path, &e.to_string());
            }
            Err(e) => {
                return Err(match e {
                    SymError::Unsupported(_) => AppError::new(
                        ErrorKind::UnsupportedFormat,
                        format!("{}: {e}", path.display()),
                    ),
                    other => map_err(other, path),
                })
            }
        };
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
            backend: Backend::Symphonia(SymphoniaTrack {
                format,
                decoder,
                track_id,
                time_base,
                skip_until: None,
                scratch: Vec::new(),
            }),
            sample_rate,
            channels,
            duration_ms,
        })
    }

    /// Media Foundation で開く（Symphonia が読めない AAC のときだけ）。開けなければ非対応形式
    fn open_media_foundation(path: &Path, symphonia_error: &str) -> Result<Self, AppError> {
        match MfReader::open(path) {
            Ok(r) => {
                tracing::info!("opened with Media Foundation ({symphonia_error}): {}", path.display());
                Ok(Self {
                    sample_rate: r.sample_rate,
                    channels: r.channels,
                    duration_ms: r.duration_ms,
                    backend: Backend::MediaFoundation(r),
                })
            }
            Err(mf) => Err(AppError::new(
                ErrorKind::UnsupportedFormat,
                format!("{}: {symphonia_error}; Media Foundation: {mf}", path.display()),
            )),
        }
    }

    /// Media Foundation で読んでいるか（テスト用）
    #[cfg(test)]
    pub fn uses_media_foundation(&self) -> bool {
        matches!(self.backend, Backend::MediaFoundation(_))
    }

    /// 次の塊をデコードして `out` に足す（インターリーブ、f32）。
    /// 戻り値：Ok(true)＝続きがある／Ok(false)＝曲の終わり。
    pub fn decode_next(&mut self, out: &mut Vec<f32>) -> Result<bool, AppError> {
        match &mut self.backend {
            Backend::Symphonia(t) => t.decode_next(out, &mut self.channels),
            Backend::MediaFoundation(r) => {
                let res = r.decode_next(out).map_err(|e| AppError::new(ErrorKind::PlaybackFailed, e));
                // 途中で形が変わったとき（まれ）は合わせる。音声のスレッドはこれを見て変換器を作り直す
                self.sample_rate = r.sample_rate;
                self.channels = r.channels;
                res
            }
        }
    }

    /// 指定の位置（ミリ秒）へ動かす。実際に動いた位置（ミリ秒）を返す。
    pub fn seek(&mut self, ms: u64) -> Result<u64, AppError> {
        match &mut self.backend {
            Backend::Symphonia(t) => t.seek(ms, self.sample_rate),
            Backend::MediaFoundation(r) => r
                .seek(ms)
                .map_err(|e| AppError::new(ErrorKind::PlaybackFailed, format!("seek: {e}"))),
        }
    }
}

impl SymphoniaTrack {
    fn decode_next(&mut self, out: &mut Vec<f32>, channels: &mut usize) -> Result<bool, AppError> {
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
                    if ch != *channels {
                        *channels = ch;
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

    fn seek(&mut self, ms: u64, sample_rate: u32) -> Result<u64, AppError> {
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
                    .map(|tb| tb.calc_time_saturating(Timestamp::new(sample_rate as i64)))
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

    /// v3.1.2：HE-AAC の m4a は Symphonia が読めないので Media Foundation で開く。長さ・最後までのデコード・シークが今までどおり。
    /// AAC-LC は今までどおり Symphonia（Media Foundation を使うのは「Symphonia が読めない AAC」のときだけ）
    #[test]
    fn he_aac_falls_back_to_media_foundation() {
        let _mf = crate::platform::windows::mf_audio::test_lock();
        // 前提：Symphonia だけでは HE-AAC のデコーダを作れない（作れるなら、この道は要らない）
        let p = fixture("sine_heaac.m4a");
        let probe = symphonia::default::get_probe()
            .probe(
                Hint::new().with_extension("m4a"),
                MediaSourceStream::new(Box::new(File::open(&p).unwrap()), Default::default()),
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .unwrap();
        let params = probe.default_track(TrackType::Audio).unwrap().codec_params.as_ref().unwrap().audio().unwrap().clone();
        assert_eq!(params.codec, CODEC_ID_AAC);
        assert!(codecs().make_audio_decoder(&params, &AudioDecoderOptions::default()).is_err());

        let d = Decoder::open(&p).unwrap();
        assert!(d.uses_media_foundation());
        assert_eq!((d.sample_rate, d.channels), (44_100, 2));
        assert!((2900..=3200).contains(&d.duration_ms), "{}", d.duration_ms);
        drop(d);
        let (frames, rate, ch) = decode_all_frames(&p).unwrap();
        let secs = frames as f64 / rate as f64;
        assert!((2.9..=3.2).contains(&secs), "{secs}");
        assert_eq!(ch, 2);
        // シーク：2秒目から残りは約1秒。途中で戻して頭からも読める
        let mut d = Decoder::open(&p).unwrap();
        for (to, left) in [(2000u64, 0.8..=1.25), (500, 2.3..=2.75), (0, 2.9..=3.2)] {
            assert_eq!(d.seek(to).unwrap(), to);
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
            println!("heaac: after seek({to}) {secs:.3}s left");
            assert!(left.contains(&secs), "seek({to}): {secs}s left");
        }
        // 曲の終わりより先へのシークでも落ちない
        let _ = d.seek(60_000);
        let mut buf = vec![];
        let _ = d.decode_next(&mut buf);
        // AAC-LC は Symphonia のまま
        assert!(!Decoder::open(&fixture("sine_aaclc.m4a")).unwrap().uses_media_foundation());
        assert!(!Decoder::open(&fixture("sine.aac")).unwrap().uses_media_foundation());
    }

    /// v3.1.2：Media Foundation の初期化（MFStartup）と後始末（MFShutdown）が、スレッドをまたいでも釣り合う。
    /// - 4つのスレッドが同時に開いて読んで閉じる（各スレッドで COM を MTA で初期化）
    /// - 先に STA で COM を初期化したスレッド（cpal が先に使った音声のスレッドと同じ形）でも読める
    /// - 全部閉じたら数は 0（MFShutdown 済み）。そのあと開き直しても読める
    #[test]
    fn media_foundation_startup_and_shutdown_across_threads() {
        use crate::platform::windows::mf_audio;
        let _mf = mf_audio::test_lock();
        assert_eq!(mf_audio::open_readers(), 0);
        let p = fixture("sine_heaac.m4a");
        let mut handles = vec![];
        for i in 0..4 {
            let p = p.clone();
            handles.push(std::thread::spawn(move || {
                if i == 0 {
                    // cpal と同じく STA で先に初期化しておく（このスレッドの終わりに釣り合わせる）
                    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
                    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
                    assert!(hr.is_ok());
                    for _ in 0..3 {
                        let (frames, ..) = decode_all_frames(&p).unwrap();
                        assert!(frames > 100_000);
                    }
                    unsafe { CoUninitialize() };
                } else {
                    for _ in 0..3 {
                        let mut d = Decoder::open(&p).unwrap();
                        let mut buf = vec![];
                        assert!(d.decode_next(&mut buf).unwrap());
                        d.seek(1000).unwrap();
                        assert!(d.decode_next(&mut buf).unwrap());
                    }
                }
            }));
        }
        for h in handles {
            h.join().expect("thread panicked");
        }
        assert_eq!(mf_audio::open_readers(), 0);
        // 開いている間は数が残り、閉じると戻る（別のスレッドで開いて、そのスレッドで閉じる）
        let d = Decoder::open(&p).unwrap();
        assert_eq!(mf_audio::open_readers(), 1);
        let t = std::thread::spawn(move || {
            let d2 = Decoder::open(&fixture("sine_heaac.m4a")).unwrap();
            let n = mf_audio::open_readers();
            drop(d2);
            n
        });
        assert_eq!(t.join().unwrap(), 2);
        assert_eq!(mf_audio::open_readers(), 1);
        drop(d);
        assert_eq!(mf_audio::open_readers(), 0);
        // 全部閉じた（MFShutdown した）あとでも、開き直せる
        let (frames, ..) = decode_all_frames(&p).unwrap();
        assert!(frames > 100_000);
        assert_eq!(mf_audio::open_readers(), 0);
    }

    /// v3.1.2（検査1回目）：Media Foundation の DLL が無い Windows（N エディション）の代わりに、無いものとして振る舞わせる。
    /// HE-AAC だけ非対応形式になり、Media Foundation の関数は呼ばれない（数が増えない）。AAC-LC など Symphonia の道はそのまま
    #[test]
    fn he_aac_is_unsupported_without_media_foundation() {
        use crate::platform::windows::mf_audio;
        let _mf = mf_audio::test_lock();
        mf_audio::force_unavailable_for_test(true);
        let e = Decoder::open(&fixture("sine_heaac.m4a")).err().expect("must be unsupported");
        assert_eq!(e.kind, ErrorKind::UnsupportedFormat);
        assert!(e.detail.contains("not installed"), "{}", e.detail);
        assert_eq!(mf_audio::open_readers(), 0);
        assert!(!Decoder::open(&fixture("sine_aaclc.m4a")).unwrap().uses_media_foundation());
        mf_audio::force_unavailable_for_test(false);
        // 戻せば開ける
        assert!(Decoder::open(&fixture("sine_heaac.m4a")).unwrap().uses_media_foundation());
        // DLL の有無の確かめ自体：実在しない名前は読み込めない、実在する名前は読み込める
        assert!(mf_audio::dlls_loadable_for_test(&mf_audio::DELAY_LOADED_DLLS));
        assert!(!mf_audio::dlls_loadable_for_test(&["plp-no-such-module.dll"]));
    }

    /// v3.1.2：Media Foundation で開けない AAC（中身を壊した HE-AAC）は、落ちずに非対応形式か再生エラー
    #[test]
    fn broken_he_aac_is_an_error_not_a_panic() {
        let _mf = crate::platform::windows::mf_audio::test_lock();
        let dir = crate::test_tmp().join(format!("plp-heaac-broken-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = std::fs::read(fixture("sine_heaac.m4a")).unwrap();
        // 1. 後ろ半分を切ったもの 2. 先頭の 1/3 だけ 3・4. 箱の見出し（moov）は残し、音声のデータ（mdat の中身）だけを壊したもの
        //    （3 は 0xFF で埋める、4 は決まった乱れたバイトで埋める。どちらも Symphonia は開けず、Media Foundation で開いて読む道を通る）
        let mut cases = vec![("half.m4a", src[..src.len() / 2].to_vec()), ("head.m4a", src[..src.len() / 3].to_vec())];
        // 一番上の箱を順にたどって mdat の範囲を探す
        let mut at = 0usize;
        let mut mdat = None;
        while at + 8 <= src.len() {
            let size = u32::from_be_bytes(src[at..at + 4].try_into().unwrap()) as usize;
            if &src[at + 4..at + 8] == b"mdat" {
                mdat = Some((at + 8, (at + size).min(src.len())));
            }
            if size < 8 {
                break;
            }
            at += size;
        }
        let (from, to) = mdat.expect("mdat box");
        let mut ff = src.clone();
        ff[from..to].fill(0xFF);
        cases.push(("mdat_ff.m4a", ff));
        let mut noise = src.clone();
        let mut x = 0x1234_5678u32;
        for b in &mut noise[from..to] {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *b = x as u8;
        }
        cases.push(("mdat_noise.m4a", noise));
        for (name, bytes) in cases {
            let p = dir.join(name);
            std::fs::write(&p, &bytes).unwrap();
            let r = std::panic::catch_unwind(|| match Decoder::open(&p) {
                Ok(mut d) => {
                    let mut buf = vec![];
                    for _ in 0..10_000 {
                        match d.decode_next(&mut buf) {
                            Ok(true) => continue,
                            _ => break,
                        }
                    }
                    "opened".to_string()
                }
                Err(e) => format!("{:?}", e.kind),
            });
            println!("{name}: {r:?}");
            assert!(r.is_ok(), "{name} panicked");
        }
        assert_eq!(crate::platform::windows::mf_audio::open_readers(), 0);
        let _ = std::fs::remove_dir_all(&dir);
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
