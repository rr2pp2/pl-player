//! 歌詞をネットから取るときの通信（v3.1.0）。Windows の WinHTTP を使う（依存を足さないため。windows クレートの機能だけ足した）。
//!
//! - 宛先は `lyrics::lrclib::ALLOWED_URLS` の2つだけ。**送る直前（`get` の頭）でも照合する**
//! - HTTPS だけ（443・`WINHTTP_FLAG_SECURE`・TLS 1.2／1.3）
//! - プロキシは使わない（`WINHTTP_ACCESS_TYPE_NO_PROXY`：システムや環境変数のプロキシ設定を読まない）
//! - リダイレクトはたどらない（`WINHTTP_OPTION_REDIRECT_POLICY_NEVER` ＋ `WINHTTP_DISABLE_REDIRECTS`）。3xx はそのまま状態コードで返す
//! - クッキー・自動の認証・資格情報の自動送信はしない
//! - タイムアウト：名前解決・接続・送信・受信それぞれ 20 秒、1応答を読み切るまで 60 秒。応答は上限を超えたら読むのをやめる
//! - 200 以外は本文を読まない
//!
//! このファイルは歌詞の本文も URL も（曲名が入るので）ログに出さない。

use std::ffi::c_void;
use std::time::{Duration, Instant};

use windows::core::PCWSTR;
use windows::Win32::Networking::WinHttp::*;

use crate::lyrics::lrclib::{self, HttpResponse, Transport, TransportError};

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// WinHTTP のエラー番号を短い記号に（中身は出さない）
fn code_of(e: &windows::core::Error) -> &'static str {
    match (e.code().0 as u32) & 0xFFFF {
        12002 => "timeout",
        // 証明書・TLS まわり（SECURE_FAILURE・SECURE_CHANNEL_ERROR・証明書の名前／日付・クライアント証明書を求められた など）
        12037 | 12038 | 12044 | 12045 | 12157 | 12169 | 12175 | 12179 => "tls_error",
        _ => "network_error",
    }
}

fn set_u32(h: *mut c_void, option: u32, value: u32) -> windows::core::Result<()> {
    unsafe { WinHttpSetOption(Some(h as *const c_void), option, Some(&value.to_ne_bytes())) }
}

pub struct WinHttpTransport {
    // 閉じる順：connect → session（宣言の順に drop される）
    connect: Handle,
    #[allow(dead_code)]
    session: Handle,
}

impl WinHttpTransport {
    /// セッションを用意する（ここでは通信しない。接続は `get` で送るときに初めて行われる）
    pub fn new(user_agent: &str) -> Result<Self, TransportError> {
        let ua = wide(user_agent);
        let session = Handle(unsafe { WinHttpOpen(PCWSTR(ua.as_ptr()), WINHTTP_ACCESS_TYPE_NO_PROXY, PCWSTR::null(), PCWSTR::null(), 0) });
        if session.0.is_null() {
            return Err(TransportError::Failed("network_error"));
        }
        let fail = |_| TransportError::Failed("network_error");
        set_u32(session.0, WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_NEVER).map_err(fail)?;
        // TLS 1.2／1.3 だけ。1.3 を知らない古い Windows では 1.2 だけにする
        if set_u32(session.0, WINHTTP_OPTION_SECURE_PROTOCOLS, WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2 | WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3).is_err() {
            set_u32(session.0, WINHTTP_OPTION_SECURE_PROTOCOLS, WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2).map_err(fail)?;
        }
        let ms = (lrclib::SOCKET_TIMEOUT_SEC * 1000) as i32;
        unsafe { WinHttpSetTimeouts(session.0, ms, ms, ms, ms) }.map_err(fail)?;
        let host = wide(lrclib::HOST);
        let connect = Handle(unsafe { WinHttpConnect(session.0, PCWSTR(host.as_ptr()), INTERNET_DEFAULT_HTTPS_PORT, 0) });
        if connect.0.is_null() {
            return Err(TransportError::Failed("network_error"));
        }
        Ok(Self { connect, session })
    }

    fn query_status(req: *mut c_void) -> windows::core::Result<u32> {
        let mut status: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        unsafe {
            WinHttpQueryHeaders(
                req,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                PCWSTR::null(),
                Some(&mut status as *mut u32 as *mut c_void),
                &mut size,
                std::ptr::null_mut(),
            )?;
        }
        Ok(status)
    }

    /// 見出しを文字列で。無い・2KiB を超えるなら None
    fn query_header(req: *mut c_void, level: u32) -> Option<String> {
        let mut size: u32 = 0;
        let first = unsafe { WinHttpQueryHeaders(req, level, PCWSTR::null(), None, &mut size, std::ptr::null_mut()) };
        if first.is_ok() || size == 0 || size > 2048 {
            return None;
        }
        let mut buf = vec![0u16; (size as usize) / 2 + 1];
        let mut size2 = (buf.len() * 2) as u32;
        unsafe {
            WinHttpQueryHeaders(req, level, PCWSTR::null(), Some(buf.as_mut_ptr() as *mut c_void), &mut size2, std::ptr::null_mut()).ok()?;
        }
        let n = ((size2 as usize) / 2).min(buf.len());
        Some(String::from_utf16_lossy(&buf[..n]))
    }
}

impl Transport for WinHttpTransport {
    fn get(&mut self, url: &str, _user_agent: &str, max_bytes: usize, cancelled: &dyn Fn() -> bool) -> Result<HttpResponse, TransportError> {
        // 送る直前にもう一度、許可リストと照合する
        let target = lrclib::path_and_query(url).map_err(|_| TransportError::NotAllowed)?;
        if cancelled() {
            return Err(TransportError::Cancelled);
        }
        let verb = wide("GET");
        let obj = wide(target);
        let req = Handle(unsafe {
            WinHttpOpenRequest(
                self.connect.0,
                PCWSTR(verb.as_ptr()),
                PCWSTR(obj.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                // HTTPS。URL は自分で全部エンコード済みなので、WinHTTP には手を加えさせない
                WINHTTP_FLAG_SECURE | WINHTTP_FLAG_ESCAPE_DISABLE,
            )
        });
        if req.0.is_null() {
            return Err(TransportError::Failed("network_error"));
        }
        let fail = |e: windows::core::Error| TransportError::Failed(code_of(&e));
        set_u32(req.0, WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_DISABLE_REDIRECTS | WINHTTP_DISABLE_COOKIES | WINHTTP_DISABLE_AUTHENTICATION)
            .map_err(fail)?;
        set_u32(req.0, WINHTTP_OPTION_AUTOLOGON_POLICY, WINHTTP_AUTOLOGON_SECURITY_LEVEL_HIGH).map_err(fail)?;
        let headers: Vec<u16> = "Accept: application/json\r\n".encode_utf16().collect();
        unsafe { WinHttpSendRequest(req.0, Some(&headers), None, 0, 0, 0) }.map_err(fail)?;
        unsafe { WinHttpReceiveResponse(req.0, std::ptr::null_mut()) }.map_err(fail)?;
        let status = Self::query_status(req.0).map_err(fail)?;
        if status != 200 {
            let retry_after = Self::query_header(req.0, WINHTTP_QUERY_RETRY_AFTER);
            return Ok(HttpResponse { status, retry_after, body: vec![] });
        }
        if let Some(len) = Self::query_header(req.0, WINHTTP_QUERY_CONTENT_LENGTH) {
            let len = len.trim();
            if lrclib::is_ascii_digits(len) && (len.len() > 15 || len.parse::<u64>().unwrap_or(u64::MAX) > max_bytes as u64) {
                return Err(TransportError::TooLarge);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(lrclib::TOTAL_TIMEOUT_SEC);
        let mut body: Vec<u8> = Vec::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            if cancelled() {
                return Err(TransportError::Cancelled);
            }
            if Instant::now() > deadline {
                return Err(TransportError::Failed("timeout"));
            }
            let mut n: u32 = 0;
            unsafe { WinHttpReadData(req.0, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut n) }.map_err(fail)?;
            if n == 0 {
                break;
            }
            if body.len() + n as usize > max_bytes {
                return Err(TransportError::TooLarge);
            }
            body.extend_from_slice(&buf[..n as usize]);
        }
        Ok(HttpResponse { status, retry_after: None, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 許可リストの外は、要求を作る前に断る（送らない）。セッションを作るだけでは通信しない
    #[test]
    fn rejects_outside_allowlist_without_sending() {
        let mut t = WinHttpTransport::new(&lrclib::user_agent()).expect("session");
        for url in ["https://example.com/api/get", "http://lrclib.net/api/get", "https://lrclib.net/api/publish?x=1", "https://lrclib.net/api/get?a=b#c"] {
            assert_eq!(t.get(url, "", 1024, &|| false), Err(TransportError::NotAllowed), "{url}");
        }
        // やめるボタンが押されていれば、許可リストの中でも送らない
        assert_eq!(t.get(lrclib::API_GET, "", 1024, &|| true), Err(TransportError::Cancelled));
    }
}
