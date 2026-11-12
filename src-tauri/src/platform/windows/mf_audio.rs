//! Windows の Media Foundation で AAC を PCM（f32）に戻す（v3.1.2）。
//!
//! Symphonia 0.6 の AAC デコーダは AAC-LC だけで、HE-AAC（SBR・PS。明示的な合図のもの）は「aac too complex」で断る。
//! `audio::decoder` は、Symphonia が AAC の曲を読めなかったときだけ、ここを使う。ほかの形式・ほかのコーデックには使わない。
//!
//! 作り：
//! - ファイルは `SHCreateStreamOnFileEx` で開いた流れを `MFCreateMFByteStreamOnStream` で包み、`MFCreateSourceReaderFromByteStream` で読む
//!   （URL として解釈させない。共有は Symphonia の道と同じく、ほかの読み書き・名前の変更を妨げない）
//! - 最初の音声の流れだけを選び、出力を `MFAudioFormat_Float` にする（AAC のデコーダは Windows の部品）
//! - シークは `SetCurrentPosition`。行き過ぎない位置へ戻るので、目標より前のサンプルは読みながら捨てる（Symphonia の側と同じ）
//!
//! 初期化と後始末：
//! - COM：使うスレッドごとに、初めて使うときに `CoInitializeEx`（MTA）。そのスレッドが終わるときに `CoUninitialize`
//!   （成功したときだけ。既に STA で初期化済み（cpal が先に使った音声のスレッドなど）なら、そのまま使い、後始末もしない）
//! - Media Foundation：`MFStartup`・`MFShutdown` は、開いている読み手の数で数える（プロセスで1つの数。鍵で守る）。
//!   最初の1つで `MFStartup`、最後の1つが閉じたら `MFShutdown`。`MFSTARTUP_LITE`（ソケットを初期化しない）
//! - 読み手（`MfReader`）は作ったスレッドの外へ出せない（`Send` でない）。閉じるのも同じスレッド
//!
//! Media Foundation の無い Windows（N エディションで Media Feature Pack を入れていないもの）：
//! - `mfplat.dll`・`mfreadwrite.dll` は遅延読み込み（`build.rs` の `/DELAYLOAD`）。起動のときには読み込まないので、無くても起動できる
//! - 遅延読み込みの DLL が無いまま関数を呼ぶと、例外でプロセスが落ちる。だから最初に使う前に、System32 から読み込めるかを確かめる
//!   （`mf_available`）。読み込めなければ Media Foundation の関数を1つも呼ばずに断り、曲は「非対応形式」になる

use std::marker::PhantomData;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use windows::core::{Interface, GUID, HSTRING};
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFMediaBuffer, IMFSample, IMFSourceReader, MFAudioFormat_AAC, MFAudioFormat_Float, MFCreateMFByteStreamOnStream,
    MFCreateMediaType, MFCreateSourceReaderFromByteStream, MFMediaType_Audio, MFShutdown, MFStartup, MFSTARTUP_LITE,
    MF_BYTESTREAM_ORIGIN_NAME, MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_MAJOR_TYPE,
    MF_MT_SUBTYPE, MF_PD_DURATION, MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED,
    MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR, MF_SOURCE_READER_ALL_STREAMS, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
    MF_SOURCE_READER_MEDIASOURCE, MF_VERSION,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED, STGM_READ, STGM_SHARE_DENY_NONE};
use windows::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
use windows::Win32::UI::Shell::SHCreateStreamOnFileEx;
use windows::Win32::System::Variant::{VT_I8, VT_UI8};

/// 100ns 単位（Media Foundation の時刻）
const HNS_PER_SEC: i64 = 10_000_000;
/// サンプルを返さない読み取り（流れの合図だけ）がこれだけ続いたら、壊れているとみなす
const MAX_EMPTY_READS: usize = 1000;
/// 出力として受け付けるサンプルレートとチャンネル数（AAC のデコーダの範囲より広め。これを外れたら読まない）
const MAX_RATE: u32 = 384_000;
const MAX_CHANNELS: u32 = 8;

fn first_audio() -> u32 {
    MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32
}

// ---------------------------------------------------------------- COM（スレッドごと）

struct ComInit {
    /// CoInitializeEx が成功した（S_OK・S_FALSE）。スレッドの終わりに CoUninitialize で釣り合わせる
    balanced: bool,
    /// このスレッドで COM が使える（自分で初期化した、または既に別の形で初期化されていた）
    usable: bool,
}

impl ComInit {
    fn new() -> Self {
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr.is_ok() {
            ComInit { balanced: true, usable: true }
        } else if hr == RPC_E_CHANGED_MODE {
            // 既に STA（cpal が音声のスレッドで先に初期化した、など）。そのまま使い、後始末はしない
            ComInit { balanced: false, usable: true }
        } else {
            tracing::warn!("CoInitializeEx failed: {hr:?}");
            ComInit { balanced: false, usable: false }
        }
    }
}

impl Drop for ComInit {
    fn drop(&mut self) {
        if self.balanced {
            unsafe { CoUninitialize() };
        }
    }
}

thread_local! {
    static COM: ComInit = ComInit::new();
}

fn com_usable() -> bool {
    COM.try_with(|c| c.usable).unwrap_or(false)
}

// ---------------------------------------------------------------- DLL があるか（遅延読み込みの前に）

/// 遅延読み込みにしている DLL（`build.rs` の `/DELAYLOAD` と同じもの）
pub(crate) const DELAY_LOADED_DLLS: [&str; 2] = ["mfplat.dll", "mfreadwrite.dll"];

/// 名前の DLL を System32 から全部読み込めるか。読み込めたものは閉じない（あとで遅延読み込みの仕組みが同じものを使う。
/// 読み込み済みのものは名前で見つかるので、System32 以外の同じ名前の DLL を拾わない）
fn dlls_loadable(names: &[&str]) -> bool {
    names.iter().all(|name| {
        let r = unsafe { LoadLibraryExW(&HSTRING::from(*name), None, LOAD_LIBRARY_SEARCH_SYSTEM32) };
        if let Err(e) = &r {
            tracing::warn!("{name} is not available, HE-AAC is unsupported: {e}");
        }
        r.is_ok()
    })
}

/// Media Foundation の DLL があるか（プロセスで1回だけ確かめる）
fn mf_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    #[cfg(test)]
    if FORCE_UNAVAILABLE.with(|f| f.get()) {
        return false;
    }
    *AVAILABLE.get_or_init(|| dlls_loadable(&DELAY_LOADED_DLLS))
}

#[cfg(test)]
thread_local! {
    /// テスト用：このスレッドでは DLL が無いものとして振る舞う（N エディションの代わり）
    static FORCE_UNAVAILABLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// テスト用：このスレッドで、Media Foundation の DLL が無いものとして振る舞うかを決める
#[cfg(test)]
pub(crate) fn force_unavailable_for_test(on: bool) {
    FORCE_UNAVAILABLE.with(|f| f.set(on));
}

/// テスト用：`dlls_loadable` をそのまま呼ぶ
#[cfg(test)]
pub(crate) fn dlls_loadable_for_test(names: &[&str]) -> bool {
    dlls_loadable(names)
}

// ---------------------------------------------------------------- MFStartup・MFShutdown（プロセスで1つの数）

/// 開いている読み手の数。0 → 1 で MFStartup、1 → 0 で MFShutdown
static OPEN_READERS: Mutex<usize> = Mutex::new(0);

struct Platform {
    _not_send: PhantomData<*const ()>,
}

impl Platform {
    fn acquire() -> Result<Self, String> {
        // DLL が無ければ、Media Foundation の関数を1つも呼ばない（遅延読み込みの例外で落ちないため）
        if !mf_available() {
            return Err("Media Foundation is not installed (mfplat.dll / mfreadwrite.dll)".into());
        }
        let mut n = OPEN_READERS.lock().unwrap_or_else(|p| p.into_inner());
        if *n == 0 {
            unsafe { MFStartup(MF_VERSION, MFSTARTUP_LITE) }.map_err(|e| format!("MFStartup: {e}"))?;
        }
        *n += 1;
        Ok(Platform { _not_send: PhantomData })
    }
}

impl Drop for Platform {
    fn drop(&mut self) {
        let mut n = OPEN_READERS.lock().unwrap_or_else(|p| p.into_inner());
        *n = n.saturating_sub(1);
        if *n == 0 {
            if let Err(e) = unsafe { MFShutdown() } {
                tracing::warn!("MFShutdown: {e}");
            }
        }
    }
}

/// テスト用：今開いている読み手の数（MFStartup の数）
#[cfg(test)]
pub(crate) fn open_readers() -> usize {
    *OPEN_READERS.lock().unwrap_or_else(|p| p.into_inner())
}

/// テスト用：Media Foundation を使うテストを1つずつ走らせる（読み手の数を確かめるテストが、ほかのテストの読み手を数えないように）
#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

// ---------------------------------------------------------------- 読み手

/// 1曲ぶんの読み手。フィールドは宣言の順に閉じる（読み手 → MFShutdown の数）
pub struct MfReader {
    reader: IMFSourceReader,
    pub sample_rate: u32,
    pub channels: usize,
    pub duration_ms: u64,
    /// シーク直後：この時刻（100ns）より前のサンプルを捨てる
    skip_until: Option<i64>,
    _platform: Platform,
}

fn mf_err(what: &str, e: windows::core::Error) -> String {
    format!("{what}: {e}")
}

impl MfReader {
    /// 開く。最初の音声の流れが AAC でなければ断る（この道は AAC のためだけ）
    pub fn open(path: &Path) -> Result<Self, String> {
        if !com_usable() {
            return Err("COM is not available on this thread".into());
        }
        let platform = Platform::acquire()?;
        let url = HSTRING::from(path.as_os_str());
        // ファイルは自分で開いたバイトの流れとして渡す（URL として解釈させない）。共有はほかの読み書き・名前の変更を妨げない形
        // （STGM_SHARE_DENY_NONE。Symphonia の道（Rust の File::open）と同じく、鳴らしている間もほかのアプリがタグを書ける）。
        // MFCreateFile は書き込みの共有を断る（MF_FILEFLAGS_ALLOW_WRITE_SHARING でも断られた。テスト file_is_shared_while_open）
        let file = unsafe { SHCreateStreamOnFileEx(&url, (STGM_READ | STGM_SHARE_DENY_NONE).0, 0, false, None) }
            .map_err(|e| mf_err("open file", e))?;
        let stream = unsafe { MFCreateMFByteStreamOnStream(&file) }.map_err(|e| mf_err("byte stream", e))?;
        // 形式の見分けにファイル名（拡張子）を使わせる（MFCreateFile と同じ手がかり）
        if let Ok(attrs) = stream.cast::<IMFAttributes>() {
            let _ = unsafe { attrs.SetString(&MF_BYTESTREAM_ORIGIN_NAME, &url) };
        }
        let reader = unsafe { MFCreateSourceReaderFromByteStream(&stream, None) }.map_err(|e| mf_err("source reader", e))?;
        unsafe {
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false).map_err(|e| mf_err("select", e))?;
            reader.SetStreamSelection(first_audio(), true).map_err(|e| mf_err("select audio", e))?;
            let native = reader.GetNativeMediaType(first_audio(), 0).map_err(|e| mf_err("native type", e))?;
            let sub = native.GetGUID(&MF_MT_SUBTYPE).map_err(|e| mf_err("native subtype", e))?;
            if sub != MFAudioFormat_AAC {
                return Err(format!("not AAC: {sub:?}"));
            }
            let want = MFCreateMediaType().map_err(|e| mf_err("media type", e))?;
            want.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(|e| mf_err("major type", e))?;
            want.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float).map_err(|e| mf_err("subtype", e))?;
            reader.SetCurrentMediaType(first_audio(), None, &want).map_err(|e| mf_err("set float output", e))?;
        }
        let mut r = MfReader {
            reader,
            sample_rate: 0,
            channels: 0,
            duration_ms: 0,
            skip_until: None,
            _platform: platform,
        };
        r.refresh_format()?;
        r.duration_ms = r.read_duration_ms();
        Ok(r)
    }

    /// 今の出力の形（サンプルレート・チャンネル数）を読む。f32 でない・範囲の外なら Err
    fn refresh_format(&mut self) -> Result<(), String> {
        unsafe {
            let cur = self.reader.GetCurrentMediaType(first_audio()).map_err(|e| mf_err("current type", e))?;
            let sub = cur.GetGUID(&MF_MT_SUBTYPE).map_err(|e| mf_err("current subtype", e))?;
            let bits = cur.GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE).unwrap_or(0);
            let rate = cur.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).map_err(|e| mf_err("rate", e))?;
            let ch = cur.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).map_err(|e| mf_err("channels", e))?;
            if sub != MFAudioFormat_Float || bits != 32 {
                return Err(format!("output is not 32-bit float: {sub:?} {bits}"));
            }
            if rate == 0 || rate > MAX_RATE || ch == 0 || ch > MAX_CHANNELS {
                return Err(format!("unsupported output: {rate} Hz x{ch}"));
            }
            self.sample_rate = rate;
            self.channels = ch as usize;
        }
        Ok(())
    }

    /// 曲の長さ（ミリ秒）。読めなければ 0
    fn read_duration_ms(&self) -> u64 {
        let mut pv = match unsafe { self.reader.GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION) } {
            Ok(v) => v,
            Err(_) => return 0,
        };
        let hns = unsafe {
            let inner = &pv.Anonymous.Anonymous;
            if inner.vt == VT_UI8 {
                inner.Anonymous.uhVal
            } else {
                0
            }
        };
        unsafe {
            let _ = PropVariantClear(&mut pv);
        }
        hns / 10_000
    }

    /// 次の塊を `out` に足す（インターリーブ、f32）。Ok(true)＝続きがある／Ok(false)＝曲の終わり
    pub fn decode_next(&mut self, out: &mut Vec<f32>) -> Result<bool, String> {
        for _ in 0..MAX_EMPTY_READS {
            let mut flags = 0u32;
            let mut ts = 0i64;
            let mut sample: Option<IMFSample> = None;
            unsafe {
                self.reader
                    .ReadSample(first_audio(), 0, None, Some(&mut flags), Some(&mut ts), Some(&mut sample))
                    .map_err(|e| mf_err("ReadSample", e))?;
            }
            if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                return Err("source reader error".into());
            }
            if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                // まれ（途中で形が変わる流れ）。呼び出し側はサンプルレート・チャンネル数を見て変換器を作り直す
                self.refresh_format()?;
            }
            if let Some(s) = sample {
                let before = out.len();
                self.copy_sample(&s, ts, out)?;
                if out.len() > before {
                    return Ok(true);
                }
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Ok(false);
            }
        }
        Err("too many empty reads".into())
    }

    /// サンプルの中身（f32）を足す。シーク直後は目標より前を捨てる
    fn copy_sample(&mut self, s: &IMFSample, ts: i64, out: &mut Vec<f32>) -> Result<(), String> {
        let buf: IMFMediaBuffer = unsafe { s.ConvertToContiguousBuffer() }.map_err(|e| mf_err("buffer", e))?;
        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut len = 0u32;
        unsafe { buf.Lock(&mut ptr, None, Some(&mut len)) }.map_err(|e| mf_err("lock", e))?;
        // Lock したら、どの抜け方でも Unlock する
        struct Unlock<'a>(&'a IMFMediaBuffer);
        impl Drop for Unlock<'_> {
            fn drop(&mut self) {
                let _ = unsafe { self.0.Unlock() };
            }
        }
        let _unlock = Unlock(&buf);
        if ptr.is_null() || len == 0 {
            return Ok(());
        }
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
        let ch = self.channels.max(1);
        let frame_bytes = 4 * ch;
        let frames = bytes.len() / frame_bytes;
        let mut skip_frames = 0usize;
        if let Some(until) = self.skip_until {
            let end = ts.saturating_add(frames as i64 * HNS_PER_SEC / self.sample_rate.max(1) as i64);
            if end <= until {
                return Ok(()); // まるごと目標より前
            }
            if ts < until {
                skip_frames = ((until - ts) as i128 * self.sample_rate as i128 / HNS_PER_SEC as i128) as usize;
            }
            self.skip_until = None;
        }
        let start = skip_frames.min(frames) * frame_bytes;
        out.reserve((frames * frame_bytes - start) / 4);
        out.extend(
            bytes[start..frames * frame_bytes]
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        );
        Ok(())
    }

    /// 指定の位置（ミリ秒）へ動かす。戻り値は目標の位置（目標より前は decode_next で捨てる）
    pub fn seek(&mut self, ms: u64) -> Result<u64, String> {
        let hns = (ms.min(i64::MAX as u64 / 10_000) as i64) * 10_000;
        let mut pv = PROPVARIANT::default();
        unsafe {
            let inner = &mut pv.Anonymous.Anonymous;
            inner.vt = VT_I8;
            inner.Anonymous.hVal = hns;
            self.reader.SetCurrentPosition(&GUID::zeroed(), &pv).map_err(|e| mf_err("seek", e))?;
        }
        self.skip_until = Some(hns);
        Ok(ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)
    }

    fn decode_all(r: &mut MfReader) -> u64 {
        let mut buf = Vec::new();
        let mut frames = 0u64;
        loop {
            buf.clear();
            let more = r.decode_next(&mut buf).unwrap();
            frames += (buf.len() / r.channels) as u64;
            if !more {
                break;
            }
        }
        frames
    }

    /// AAC-LC の m4a・HE-AAC の m4a を、Media Foundation で最後まで読める。HE-AAC（芯は 22.05kHz）は SBR を当てた 44.1kHz で出る
    #[test]
    fn reads_aac_lc_and_he_aac() {
        let _g = test_lock();
        for (name, rate) in [("sine_aaclc.m4a", 44_100), ("sine_heaac.m4a", 44_100)] {
            let mut r = MfReader::open(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(r.sample_rate, rate, "{name}");
            assert_eq!(r.channels, 2, "{name}");
            assert!((2900..=3200).contains(&r.duration_ms), "{name}: {}", r.duration_ms);
            let secs = decode_all(&mut r) as f64 / r.sample_rate as f64;
            println!("{name}: {secs:.3}s @ {} Hz x{}", r.sample_rate, r.channels);
            assert!((2.9..=3.2).contains(&secs), "{name}: {secs}s");
        }
    }

    /// 鳴らしている間も、ほかの読み手（歌詞のタグ・タグの編集画面）は読め、ほかのアプリはファイルに書けて名前も変えられる
    /// （Symphonia の道と同じ。MF_FILEFLAGS_ALLOW_WRITE_SHARING。無いと書き込みの共有が断られる）
    #[test]
    fn file_is_shared_while_open() {
        let _g = test_lock();
        let dir = crate::test_tmp().join(format!("plp-mf-share-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.m4a");
        std::fs::copy(fixture("sine_heaac.m4a"), &p).unwrap();
        let r = MfReader::open(&p).unwrap();
        assert!(std::fs::read(&p).is_ok());
        assert_eq!(crate::library::tags_read::read_values(&p).unwrap().title, "Sine HE-AAC");
        assert!(std::fs::OpenOptions::new().write(true).open(&p).is_ok());
        std::fs::rename(&p, dir.join("b.m4a")).unwrap();
        drop(r);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// AAC でないもの（MP3・FLAC・壊れたファイル・無いファイル）は開かない。開けなくても MFStartup の数は戻る
    #[test]
    fn refuses_non_aac_and_broken_files() {
        let _g = test_lock();
        for name in ["sine.mp3", "sine.flac", "broken.mp3", "no_such_file.m4a"] {
            assert!(MfReader::open(&fixture(name)).is_err(), "{name}");
        }
        assert_eq!(open_readers(), 0);
    }
}
