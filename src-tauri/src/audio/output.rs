//! 出力（cpal / WASAPI）。
//!
//! デコード側とは受け渡しの輪（rtrb のリングバッファ）だけでつながる。
//! 再生位置は「出力のコールバックが実際に鳴らしたフレーム数」から出す（engine 側で計算）。

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use crossbeam_channel::Sender;
use rtrb::{Consumer, Producer, RingBuffer};

use super::devices;
use super::engine::Internal;
use crate::error::{AppError, ErrorKind};

/// 出力先が変わっても引き継ぐ操作（音量・ミュート・一時停止）
pub struct Controls {
    pub volume_bits: AtomicU32,
    pub muted: AtomicBool,
    pub paused: AtomicBool,
}

impl Controls {
    pub fn new(volume: f32, muted: bool) -> Self {
        Self {
            volume_bits: AtomicU32::new(volume.to_bits()),
            muted: AtomicBool::new(muted),
            paused: AtomicBool::new(true),
        }
    }
    pub fn set_volume(&self, v: f32) {
        self.volume_bits.store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }
    /// 実際に掛ける倍率（聞こえ方に合わせて2乗にする）
    fn gain(&self) -> f32 {
        if self.muted.load(Ordering::Relaxed) {
            0.0
        } else {
            let v = self.volume();
            v * v
        }
    }
}

struct Shared {
    consumer: Mutex<Consumer<f32>>,
    played_frames: AtomicU64,
}

pub struct Output {
    _stream: cpal::Stream,
    producer: Producer<f32>,
    shared: Arc<Shared>,
    pub rate: u32,
    pub channels: usize,
    /// None なら「既定のデバイス」で開いている（Windows の既定の変更に自動で追従する）
    pub device_id: Option<String>,
    pub device_name: String,
    capacity: usize,
}

impl Output {
    pub fn open(
        device_id: Option<&str>,
        controls: Arc<Controls>,
        internal: Sender<Internal>,
    ) -> Result<Self, AppError> {
        let device = devices::find_output(device_id).ok_or_else(|| {
            AppError::new(
                ErrorKind::DeviceDisconnected,
                format!("output device not found: {}", device_id.unwrap_or("(default)")),
            )
        })?;
        let device_name = devices::device_name(&device);
        let supported = device
            .default_output_config()
            .map_err(|e| AppError::new(ErrorKind::PlaybackFailed, format!("default_output_config: {e}")))?;
        let rate = supported.sample_rate();
        let channels = supported.channels() as usize;
        let format = supported.sample_format();
        let config = cpal::StreamConfig {
            channels: supported.channels(),
            sample_rate: rate,
            buffer_size: cpal::BufferSize::Default,
        };
        // 0.5 秒ぶんの輪
        let capacity = (rate as usize / 2).max(4096) * channels;
        let (producer, consumer) = RingBuffer::<f32>::new(capacity);
        let shared = Arc::new(Shared {
            consumer: Mutex::new(consumer),
            played_frames: AtomicU64::new(0),
        });
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, &config, channels, shared.clone(), controls, internal),
            SampleFormat::I16 => build::<i16>(&device, &config, channels, shared.clone(), controls, internal),
            SampleFormat::I32 => build::<i32>(&device, &config, channels, shared.clone(), controls, internal),
            SampleFormat::U16 => build::<u16>(&device, &config, channels, shared.clone(), controls, internal),
            SampleFormat::F64 => build::<f64>(&device, &config, channels, shared.clone(), controls, internal),
            other => Err(AppError::new(
                ErrorKind::PlaybackFailed,
                format!("unsupported device sample format: {other:?}"),
            )),
        }?;
        stream
            .play()
            .map_err(|e| AppError::new(map_kind(e.kind()), format!("stream play: {e}")))?;
        Ok(Self {
            _stream: stream,
            producer,
            shared,
            rate,
            channels,
            device_id: device_id.map(|s| s.to_string()),
            device_name,
            capacity,
        })
    }

    /// 輪の空き（サンプル数）
    pub fn free(&self) -> usize {
        self.producer.slots()
    }

    /// 輪に残っているサンプル数
    pub fn buffered(&self) -> usize {
        self.capacity - self.producer.slots()
    }

    /// 入るだけ入れて、入れた数を返す
    pub fn push(&mut self, samples: &[f32]) -> usize {
        let (pushed, _rest) = self.producer.push_partial_slice(samples);
        pushed.len()
    }

    /// シーク・曲の切り替え：輪を空にして、鳴らしたフレーム数を 0 に戻す
    pub fn flush(&self) {
        let mut c = self.shared.consumer.lock().unwrap_or_else(|p| p.into_inner());
        let n = c.slots();
        if let Ok(chunk) = c.read_chunk(n) {
            chunk.commit_all();
        }
        self.shared.played_frames.store(0, Ordering::SeqCst);
    }

    pub fn played_frames(&self) -> u64 {
        self.shared.played_frames.load(Ordering::SeqCst)
    }
}

fn map_kind(kind: cpal::ErrorKind) -> ErrorKind {
    match kind {
        cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::StreamInvalidated => {
            ErrorKind::DeviceDisconnected
        }
        _ => ErrorKind::PlaybackFailed,
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    shared: Arc<Shared>,
    controls: Arc<Controls>,
    internal: Sender<Internal>,
) -> Result<cpal::Stream, AppError>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let silence = T::from_sample(0.0f32);
    let data_cb = move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
        if controls.paused.load(Ordering::Relaxed) {
            data.fill(silence);
            return;
        }
        let gain = controls.gain();
        let mut written = 0usize;
        if let Ok(mut c) = shared.consumer.try_lock() {
            let mut n = data.len().min(c.slots());
            n -= n % channels.max(1);
            if n > 0 {
                if let Ok(chunk) = c.read_chunk(n) {
                    let (a, b) = chunk.as_slices();
                    for (dst, src) in data.iter_mut().zip(a.iter().chain(b.iter())) {
                        *dst = T::from_sample((*src * gain).clamp(-1.0, 1.0));
                    }
                    chunk.commit_all();
                    written = n;
                    shared
                        .played_frames
                        .fetch_add((n / channels.max(1)) as u64, Ordering::SeqCst);
                }
            }
        }
        if written < data.len() {
            data[written..].fill(silence);
        }
    };
    let err_cb = move |err: cpal::Error| {
        // コールバックの中では待たない。種類だけを音声スレッドへ流す。
        let _ = internal.try_send(Internal::StreamError(err.kind(), err.to_string()));
    };
    device
        .build_output_stream::<T, _, _>(config.clone(), data_cb, err_cb, None)
        .map_err(|e| AppError::new(map_kind(e.kind()), format!("build_output_stream: {e}")))
}
