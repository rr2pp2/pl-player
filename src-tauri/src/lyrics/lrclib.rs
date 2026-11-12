//! LRCLIB から歌詞を引く決まり（v3.1.0）。
//!
//! 先に作った非公開の取得スクリプト（検査を3回通したもの。以下「スクリプト」）が元。定数・照合・再試行・応答の読み方は
//! そこから同じ値・同じ順序で移した。違えたところは関数ごとに「スクリプトとの違い」として書く。
//!
//! - ここは通信しない。通信は `Transport`（本物は `platform::windows::winhttp`、テストは偽物）
//! - 宛先は `ALLOWED_URLS` の2つだけ。URL を組むときと、送る直前（Transport の中）の両方で照合する
//! - 歌詞の本文と、LRCLIB が返した曲名・アーティスト名は、ログにもエラーにも出さない（件数・曲 ID・状態だけ）

use std::collections::BTreeMap;

use serde_json::value::RawValue;
use serde_json::Value;

use super::lrc;

// ---------------------------------------------------------------- 定数（スクリプトと同じ値）

pub const API_GET: &str = "https://lrclib.net/api/get";
pub const API_SEARCH: &str = "https://lrclib.net/api/search";
/// 送ってよい宛先（クエリを除いた部分が完全一致すること）
pub const ALLOWED_URLS: [&str; 2] = [API_GET, API_SEARCH];
/// 接続先のホスト（WinHTTP に渡す）。ALLOWED_URLS と同じホスト
pub const HOST: &str = "lrclib.net";
const ORIGIN: &str = "https://lrclib.net";

/// 名札（User-Agent）に入れるアプリ名。版は Cargo.toml の version
pub const APP_NAME: &str = "PL-Player";
/// 連絡先の URL（LRCLIB の求め。後で足せるよう定数にしておく）。**個人を指すもの（メール・個人の URL）は入れない**
pub const CONTACT_URL: Option<&str> = None;

pub const MIN_GAP_SEC: f64 = 0.3; // 要求の間を 300〜500ms 空ける
pub const MAX_GAP_SEC: f64 = 0.5;
pub const SOCKET_TIMEOUT_SEC: u32 = 20; // 1回の送受信の待ち
pub const TOTAL_TIMEOUT_SEC: u64 = 60; // 1応答を読み切るまでの上限
pub const MAX_ATTEMPTS: u32 = 5; // 1要求あたりの試行回数（429・503・通信エラー）
pub const MAX_RETRY_AFTER_SEC: f64 = 300.0; // これより長い Retry-After は待たずに打ち切る
pub const JITTER: f64 = 0.3; // Retry-After に足すゆらぎ（最大 30%）
pub const MAX_CONSECUTIVE_ERRORS: u32 = 5; // 曲のエラーがこれだけ続いたら打ち切る

pub const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024; // 応答の上限（超えたら読まずにエラー）
pub const MAX_LYRICS_BYTES: usize = lrc::MAX_TEXT_BYTES; // 保存する歌詞の上限（取り込みと同じ 1MiB）
pub const MAX_SEARCH_ITEMS: usize = 50; // search の結果は先頭だけ見る（公式は最大20件）
pub const MAX_NAME_CHARS: usize = 512; // これより長い曲名などは送らない
pub const DURATION_TOLERANCE_SEC: f64 = 2.0;
pub const MIN_PARTIAL_CHARS: usize = 3; // 名前の部分一致は、正規化して短い方が3文字以上のときだけ
pub const PARTIAL_RATIO_NUM: usize = 1; // かつ短い方が長い方の 1/2 以上
pub const PARTIAL_RATIO_DEN: usize = 2;

/// 応答の JSON の中の、UTF-8 で表せない文字（単独サロゲート `\ud800` など）の置き換え先。
/// 歌詞にこれがあれば `bad_lyrics_text`（スクリプトの encodable と同じ扱い）。名前では記号として消える
const LONE_SURROGATE_MARK: char = '\u{FDD0}';

/// 名札。アプリ名と版だけ（CONTACT_URL があれば足す）
pub fn user_agent() -> String {
    match CONTACT_URL {
        Some(u) => format!("{APP_NAME}/{} (+{u})", env!("CARGO_PKG_VERSION")),
        None => format!("{APP_NAME}/{}", env!("CARGO_PKG_VERSION")),
    }
}

// ---------------------------------------------------------------- 宛先の照合

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotAllowed;

/// 許可リストの2つ以外へは出さない。クエリを除いた部分が完全一致し、クエリは URL エンコード済みの文字だけ
/// （スキーム・ホスト・ポート・ユーザー名・パス・`#` をすべてここで縛る）
pub fn check_allowed(url: &str) -> Result<(), NotAllowed> {
    let (base, query) = match url.split_once('?') {
        Some((b, q)) => (b, Some(q)),
        None => (url, None),
    };
    if !ALLOWED_URLS.contains(&base) {
        return Err(NotAllowed);
    }
    if let Some(q) = query {
        if !q.bytes().all(|b| b.is_ascii_alphanumeric() || b"%+=&._~-".contains(&b)) {
            return Err(NotAllowed);
        }
    }
    Ok(())
}

/// 照合を通った URL から、WinHTTP に渡す「パス＋クエリ」を取り出す
pub fn path_and_query(url: &str) -> Result<&str, NotAllowed> {
    check_allowed(url)?;
    url.strip_prefix(ORIGIN).filter(|p| p.starts_with("/api/")).ok_or(NotAllowed)
}

/// Python の urllib.parse.quote_plus と同じ（英数字と `_.-~` 以外は %XX、空白は +）
fn quote_plus(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn build_url(base: &str, params: &[(&str, String)]) -> Result<String, NotAllowed> {
    check_allowed(base)?;
    let q: Vec<String> = params.iter().map(|(k, v)| format!("{}={}", quote_plus(k), quote_plus(v))).collect();
    let url = format!("{base}?{}", q.join("&"));
    check_allowed(&url)?;
    Ok(url)
}

// ---------------------------------------------------------------- Retry-After

/// 0-9 だけか（上付きの数字 `²` などは通さない）
pub fn is_ascii_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// 秒数か HTTP の日付。読めなければ None（スクリプトの parse_retry_after）
pub fn parse_retry_after(value: Option<&str>, now_epoch: f64) -> Option<f64> {
    let v = value?.trim();
    if v.is_empty() || v.chars().count() > 64 {
        return None;
    }
    if is_ascii_digits(v) {
        // 64桁までなので f64 で表せる（巨大な値は MAX_RETRY_AFTER_SEC を超えて打ち切りになる）
        return v.parse::<f64>().ok().filter(|x| x.is_finite());
    }
    let t = parse_http_date(v)?;
    Some((t - now_epoch).max(0.0))
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

const MONTHS_FULL: [&str; 12] = [
    "january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december",
];

/// 月の名前（3文字か、つづり全部）。Python の _monthnames と同じ
fn month_of(s: &str) -> Option<u32> {
    let s = s.to_ascii_lowercase();
    MONTHS
        .iter()
        .position(|m| *m == s)
        .or_else(|| MONTHS_FULL.iter().position(|m| *m == s))
        .map(|i| i as u32 + 1)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        _ => 28,
    }
}

fn small_num(s: &str) -> Option<i64> {
    if !is_ascii_digits(s) || s.len() > 4 {
        return None;
    }
    s.parse().ok()
}

/// HTTP の日付（IMF-fixdate・RFC 850・asctime・RFC 2822 の数字の時差）を UNIX 時刻（秒）に。
/// Python の email.utils.parsedate_to_datetime が読む形のうち、HTTP で使われるものを読む。時差が無ければ UTC
pub fn parse_http_date(s: &str) -> Option<f64> {
    let mut data: Vec<String> = s.split_whitespace().map(str::to_string).collect();
    if data.is_empty() {
        return None;
    }
    let first = data[0].to_ascii_lowercase();
    if first.ends_with(',') || DAYS.iter().any(|d| first.starts_with(d) && first.len() <= 10 && first.chars().all(|c| c.is_ascii_alphabetic() || c == ',')) {
        data.remove(0);
    }
    // RFC 850：06-Nov-94
    if data.len() == 3 {
        let parts: Vec<String> = data[0].split('-').map(str::to_string).collect();
        if parts.len() == 3 {
            let rest = data[1..].to_vec();
            data = parts;
            data.extend(rest);
        }
    }
    if data.len() < 4 {
        return None;
    }
    let (mut dd, mut mm, mut yy, mut tm) = (data[0].clone(), data[1].clone(), data[2].clone(), data[3].clone());
    let tz = data.get(4).cloned();
    if month_of(&mm).is_none() {
        // asctime：Nov 6 08:49:37 1994
        std::mem::swap(&mut dd, &mut mm);
    }
    let month = month_of(&mm)?;
    if yy.contains(':') {
        std::mem::swap(&mut yy, &mut tm);
    }
    let dd = dd.trim_end_matches(',');
    let day = small_num(dd)?;
    let mut year = small_num(yy.trim_end_matches(','))?;
    if year < 100 {
        year += if year > 68 { 1900 } else { 2000 };
    }
    if !(1..=9999).contains(&year) {
        return None;
    }
    let t: Vec<&str> = tm.split(':').collect();
    let (h, mi, sec) = match t.as_slice() {
        [h, m] => (small_num(h)?, small_num(m)?, 0),
        [h, m, s] => (small_num(h)?, small_num(m)?, small_num(s)?),
        _ => return None,
    };
    if day < 1 || day > days_in_month(year, month) as i64 || h > 23 || mi > 59 || sec > 59 {
        return None;
    }
    let offset_sec: i64 = match tz.as_deref() {
        None => 0,
        Some(z) => {
            let zu = z.to_ascii_uppercase();
            match zu.as_str() {
                "UT" | "UTC" | "GMT" | "Z" => 0,
                "AST" => -4 * 3600,
                "ADT" => -3 * 3600,
                "EST" => -5 * 3600,
                "EDT" => -4 * 3600,
                "CST" => -6 * 3600,
                "CDT" => -5 * 3600,
                "MST" => -7 * 3600,
                "MDT" => -6 * 3600,
                "PST" => -8 * 3600,
                "PDT" => -7 * 3600,
                _ if (zu.starts_with('+') || zu.starts_with('-')) && zu.len() == 5 && is_ascii_digits(&zu[1..]) => {
                    let v: i64 = zu[1..].parse().ok()?;
                    let secs = (v / 100) * 3600 + (v % 100) * 60;
                    if zu.starts_with('-') {
                        -secs
                    } else {
                        secs
                    }
                }
                _ => 0, // 知らない名前は UTC（スクリプトと同じ）
            }
        }
    };
    let days = days_from_civil(year, month as u32, day as u32);
    Some((days * 86400 + h * 3600 + mi * 60 + sec - offset_sec) as f64)
}

// ---------------------------------------------------------------- 通信の口

/// 応答。200 以外は本文を読まない（空）
#[derive(Debug, Clone, PartialEq)]
pub struct HttpResponse {
    pub status: u32,
    pub retry_after: Option<String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransportError {
    /// 通信そのものの失敗（再試行してよい）。timeout / tls_error / network_error
    Failed(&'static str),
    /// 応答が MAX_RESPONSE_BYTES を超えた（読むのをやめた）
    TooLarge,
    /// 許可リストの外（送っていない）
    NotAllowed,
    /// やめるボタン
    Cancelled,
}

/// GET を1回送る。リダイレクトはたどらず、3xx はそのまま状態コードで返すこと。
/// 実装は送る直前に `check_allowed` で照合すること
pub trait Transport {
    fn get(&mut self, url: &str, user_agent: &str, max_bytes: usize, cancelled: &dyn Fn() -> bool) -> Result<HttpResponse, TransportError>;
}

/// 止める理由（やめるボタン・設定がオフになった）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Cancelled,
    Disabled,
}

impl Stop {
    pub fn code(&self) -> &'static str {
        match self {
            Stop::Cancelled => "cancelled",
            Stop::Disabled => "disabled",
        }
    }
}

/// 待ち・乱数・時計（テストでは差し替えて実時間を使わない）
pub trait Env {
    /// 待つ。途中でやめるボタンが押されたら Err
    fn sleep(&mut self, secs: f64) -> Result<(), Stop>;
    /// 0 以上 1 未満
    fn rand(&mut self) -> f64;
    /// 単調に増える時計（秒）
    fn now(&mut self) -> f64;
    /// UNIX 時刻（秒）。Retry-After の日付に使う
    fn epoch(&mut self) -> f64;
}

/// 1要求の失敗
#[derive(Debug, Clone, PartialEq)]
pub enum GetError {
    /// その曲をエラーにして次へ（短い記号だけ。応答の中身は持たない）
    Fetch(String),
    /// 全体を打ち切る（rate_limited_long_wait / rate_limited_repeatedly）
    Abort(&'static str),
    /// やめるボタン・設定がオフ
    Stop(Stop),
}

/// 逐次・間隔あり・再試行ありの GET（スクリプトの Client）。
/// `gate` は送る直前に毎回呼ぶ：設定がオフ・やめるボタンなら送らない
pub struct Client<'a, T: Transport, E: Env> {
    pub transport: T,
    pub env: E,
    gate: Box<dyn Fn() -> Option<Stop> + 'a>,
    /// 最後に応答を受けた時刻（env.now の値）。前の操作から引き継ぐ
    pub last: Option<f64>,
    pub requests_sent: u64,
    user_agent: String,
}

impl<'a, T: Transport, E: Env> Client<'a, T, E> {
    pub fn new(transport: T, env: E, gate: impl Fn() -> Option<Stop> + 'a) -> Self {
        Self {
            transport,
            env,
            gate: Box::new(gate),
            last: None,
            requests_sent: 0,
            user_agent: user_agent(),
        }
    }

    /// 止める理由があるか（曲の間で見る。送る直前は get の中で毎回見る）
    pub fn check_gate(&self) -> Option<Stop> {
        (self.gate)()
    }

    fn pace(&mut self) -> Result<(), Stop> {
        let Some(last) = self.last else { return Ok(()) };
        let gap = MIN_GAP_SEC + (MAX_GAP_SEC - MIN_GAP_SEC) * self.env.rand();
        let wait = gap - (self.env.now() - last);
        if wait > 0.0 {
            self.env.sleep(wait)?;
        }
        Ok(())
    }

    fn jittered(&mut self, seconds: f64) -> f64 {
        seconds * (1.0 + JITTER * self.env.rand())
    }

    fn sleep(&mut self, secs: f64) -> Result<(), GetError> {
        self.env.sleep(secs).map_err(GetError::Stop)
    }

    /// (状態コード, 本文) を返す。返すのは 200 と 404 だけ。ほかはここで扱いを決める
    pub fn get(&mut self, base: &str, params: &[(&str, String)]) -> Result<(u32, Vec<u8>), GetError> {
        let url = build_url(base, params).map_err(|_| GetError::Fetch("not_allowed".into()))?;
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            self.pace().map_err(GetError::Stop)?;
            // 送る直前に毎回：設定がオフ・やめるボタンなら、通信の処理へ進まない
            if let Some(stop) = (self.gate)() {
                return Err(GetError::Stop(stop));
            }
            self.requests_sent += 1;
            let gate = &self.gate;
            let cancelled = || gate().is_some();
            let res = self.transport.get(&url, &self.user_agent, MAX_RESPONSE_BYTES, &cancelled);
            self.last = Some(self.env.now());
            let resp = match res {
                Ok(r) => r,
                Err(TransportError::TooLarge) => return Err(GetError::Fetch("response_too_large".into())),
                Err(TransportError::NotAllowed) => return Err(GetError::Fetch("not_allowed".into())),
                Err(TransportError::Cancelled) => return Err(GetError::Stop((self.gate)().unwrap_or(Stop::Cancelled))),
                Err(TransportError::Failed(code)) => {
                    if attempt >= MAX_ATTEMPTS {
                        return Err(GetError::Fetch(code.into()));
                    }
                    let w = self.jittered(30f64.min(2f64.powi(attempt as i32 - 1)));
                    self.sleep(w)?;
                    continue;
                }
            };
            let status = resp.status;
            if status == 429 {
                let now = self.env.epoch();
                let ra = parse_retry_after(resp.retry_after.as_deref(), now)
                    .unwrap_or_else(|| 60f64.min(5.0 * 2f64.powi(attempt as i32 - 1)));
                if ra > MAX_RETRY_AFTER_SEC {
                    return Err(GetError::Abort("rate_limited_long_wait"));
                }
                if attempt >= MAX_ATTEMPTS {
                    return Err(GetError::Abort("rate_limited_repeatedly"));
                }
                let w = self.jittered(ra);
                self.sleep(w)?;
                continue;
            }
            if status == 503 {
                let now = self.env.epoch();
                let ra = parse_retry_after(resp.retry_after.as_deref(), now).unwrap_or(1.0);
                if ra > MAX_RETRY_AFTER_SEC || attempt >= MAX_ATTEMPTS {
                    return Err(GetError::Fetch("http_503".into()));
                }
                let w = self.jittered(ra);
                self.sleep(w)?;
                continue;
            }
            if (500..600).contains(&status) {
                if attempt >= MAX_ATTEMPTS {
                    return Err(GetError::Fetch(format!("http_{status}")));
                }
                let w = self.jittered(30f64.min(2f64.powi(attempt as i32 - 1)));
                self.sleep(w)?;
                continue;
            }
            if status == 200 || status == 404 {
                return Ok((status, resp.body));
            }
            return Err(GetError::Fetch(if (100..1000).contains(&status) {
                format!("http_{status}")
            } else {
                "http_bad_status".into()
            }));
        }
    }
}

// ---------------------------------------------------------------- 応答の読み方（中身は外へ出さない）

type Obj = BTreeMap<String, Box<RawValue>>;

const BAD_RESPONSE: &str = "bad_response";
const BAD_LYRICS_TEXT: &str = "bad_lyrics_text";

/// UTF-8 として読み、JSON の中の単独サロゲート（`\ud800` など。serde_json は読めない）を印に置き換える。
/// 正しい組（`\ud83c\udfb5`）はそのまま
fn prepare_json(body: &[u8]) -> Result<String, String> {
    let text = std::str::from_utf8(body).map_err(|_| BAD_RESPONSE.to_string())?;
    if !text.contains("\\u") {
        return Ok(text.to_string());
    }
    let b = text.as_bytes();
    let hex4 = |i: usize| -> Option<u32> {
        let s = text.get(i..i + 4)?;
        if s.bytes().all(|c| c.is_ascii_hexdigit()) {
            u32::from_str_radix(s, 16).ok()
        } else {
            None
        }
    };
    let mark = format!("\\u{:04X}", LONE_SURROGATE_MARK as u32);
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut copied = 0;
    while i < b.len() {
        if b[i] != b'\\' {
            i += 1;
            continue;
        }
        // バックスラッシュの次の1文字までが1つのエスケープ
        if i + 1 < b.len() && b[i + 1] == b'u' {
            if let Some(cu) = hex4(i + 2) {
                if (0xD800..0xDC00).contains(&cu) {
                    let pair = i + 6 + 1 < b.len() && b[i + 6] == b'\\' && b[i + 7] == b'u' && hex4(i + 8).is_some_and(|lo| (0xDC00..0xE000).contains(&lo));
                    if pair {
                        i += 12;
                        continue;
                    }
                    out.push_str(&text[copied..i]);
                    out.push_str(&mark);
                    i += 6;
                    copied = i;
                    continue;
                }
                if (0xDC00..0xE000).contains(&cu) {
                    out.push_str(&text[copied..i]);
                    out.push_str(&mark);
                    i += 6;
                    copied = i;
                    continue;
                }
                i += 6;
                continue;
            }
        }
        i += 2;
    }
    out.push_str(&text[copied.min(text.len())..]);
    Ok(out)
}

fn parse_value(raw: Option<&RawValue>) -> Option<Value> {
    // 巨大な数・深すぎる入れ子などで読めない値は None（値が無いのとは区別する呼び出し側もある）
    raw.and_then(|r| serde_json::from_str::<Value>(r.get()).ok())
}

enum Dur {
    Absent,
    Num(f64),
    /// 値はあるが数として読めない（文字列・真偽値・巨大な数・NaN など）
    Bad,
}

fn duration_of(o: &Obj) -> Dur {
    let Some(raw) = o.get("duration") else { return Dur::Absent };
    if raw.get().trim() == "null" {
        return Dur::Absent;
    }
    match parse_value(Some(raw)) {
        Some(Value::Number(n)) => match n.as_f64() {
            Some(f) if f.is_finite() => Dur::Num(f),
            _ => Dur::Bad,
        },
        _ => Dur::Bad,
    }
}

fn str_field(o: &Obj, k: &str) -> Option<String> {
    match parse_value(o.get(k).map(|b| b.as_ref())) {
        Some(Value::String(s)) => Some(s),
        _ => None,
    }
}

fn is_instrumental(o: &Obj) -> bool {
    matches!(parse_value(o.get("instrumental").map(|b| b.as_ref())), Some(Value::Bool(true)))
}

/// 文字列なら BOM を外し改行を LF にそろえる。空・空白だけなら None。UTF-8 で表せない文字があれば Err
pub fn clean_lyrics(v: Option<String>) -> Result<Option<String>, &'static str> {
    let Some(v) = v else { return Ok(None) };
    let v = v.strip_prefix('\u{feff}').unwrap_or(&v);
    let mut v = v.replace("\r\n", "\n").replace('\r', "\n");
    if v.trim().is_empty() {
        return Ok(None);
    }
    if v.contains(LONE_SURROGATE_MARK) {
        return Err(BAD_LYRICS_TEXT);
    }
    if !v.ends_with('\n') {
        v.push('\n');
    }
    Ok(Some(v))
}

pub fn within_limit(text: &str) -> bool {
    text.len() <= MAX_LYRICS_BYTES
}

/// 取り込みと同じ読み方で、表示できる中身があるか（情報行だけ・空のものは使わない）
fn has_content(text: &str) -> bool {
    lrc::parse(&lrc::normalize(text)).has_content()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Synced,
    Plain,
    Instrumental,
    NotFound,
    TooLarge,
}

impl Status {
    pub fn code(&self) -> &'static str {
        match self {
            Status::Synced => "synced",
            Status::Plain => "plain",
            Status::Instrumental => "instrumental",
            Status::NotFound => "not_found",
            Status::TooLarge => "too_large",
        }
    }
    pub fn is_ok(&self) -> bool {
        matches!(self, Status::Synced | Status::Plain)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameMatch {
    None,
    Partial,
    Exact,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub status: Status,
    /// 保存する本文（外へは出さない）
    pub text: Option<String>,
    /// LRCLIB の長さ − 手元の長さ（秒）
    pub diff: Option<f64>,
    pub via: Option<&'static str>,
    pub title_match: Option<NameMatch>,
    pub artist_match: Option<NameMatch>,
}

impl Outcome {
    fn simple(status: Status) -> Self {
        Self { status, text: None, diff: None, via: None, title_match: None, artist_match: None }
    }
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// 時刻つきを優先。1MiB を超えるものは保存しない。
/// スクリプトとの違い：取り込みで「空」として弾かれる中身（情報行だけ）は、無いものとして扱う（全文へ進む）
fn choose_text(synced: Option<String>, plain: Option<String>) -> (Option<Status>, Option<String>) {
    let mut over = false;
    for (st, t) in [(Status::Synced, synced), (Status::Plain, plain)] {
        if let Some(t) = t {
            if !within_limit(&t) {
                over = true;
            } else if has_content(&t) {
                return (Some(st), Some(t));
            }
        }
    }
    if over {
        (Some(Status::TooLarge), None)
    } else {
        (None, None)
    }
}

/// get の応答。None は「歌詞が空 → search へ」
pub fn outcome_from_get(body: &[u8], local_sec: Option<f64>) -> Result<Option<Outcome>, String> {
    let text = prepare_json(body)?;
    let obj: Obj = serde_json::from_str(&text).map_err(|_| BAD_RESPONSE.to_string())?;
    let dur = match duration_of(&obj) {
        Dur::Bad => return Err(BAD_RESPONSE.into()), // 巨大な整数・文字列など
        Dur::Num(f) => Some(f),
        Dur::Absent => None,
    };
    let diff = match (dur, local_sec) {
        (Some(d), Some(l)) => Some(round2(d - l)),
        _ => None,
    };
    if is_instrumental(&obj) {
        return Ok(Some(Outcome { diff, via: Some("get"), ..Outcome::simple(Status::Instrumental) }));
    }
    let synced = clean_lyrics(str_field(&obj, "syncedLyrics")).map_err(String::from)?;
    let plain = clean_lyrics(str_field(&obj, "plainLyrics")).map_err(String::from)?;
    let (status, text) = choose_text(synced, plain);
    let Some(status) = status else { return Ok(None) };
    Ok(Some(Outcome { status, text, diff, via: Some("get"), title_match: None, artist_match: None }))
}

/// search の応答から選ぶ（スクリプトの outcome_from_search）。
/// スクリプトとの違い：1件の中に読めない値（巨大な数など）があれば、その1件だけ外す（スクリプトは値ごとに判断する）
pub fn outcome_from_search(body: &[u8], q_track: &str, q_artist: &str, local_sec: Option<f64>) -> Result<Outcome, String> {
    let text = prepare_json(body)?;
    let items: Vec<Box<RawValue>> = serde_json::from_str(&text).map_err(|_| BAD_RESPONSE.to_string())?;
    struct Cand {
        key: (u8, u8, u8, f64),
        idx: usize,
        synced: Option<String>,
        plain: Option<String>,
        d: Option<f64>,
        tm: NameMatch,
        am: NameMatch,
    }
    let mut cands: Vec<Cand> = Vec::new();
    let mut saw_instrumental = false;
    let mut saw_bad = false;
    let nq_track = norm_name(Some(q_track));
    let nq_artist = norm_name(Some(q_artist));
    for raw in items.iter().take(MAX_SEARCH_ITEMS) {
        let Ok(it) = serde_json::from_str::<Obj>(raw.get()) else { continue };
        let dur = match duration_of(&it) {
            Dur::Num(f) => Some(f),
            _ => None,
        };
        let tm = name_match_norm(&norm_name(str_field(&it, "trackName").as_deref()), &nq_track);
        let am = name_match_norm(&norm_name(str_field(&it, "artistName").as_deref()), &nq_artist);
        if let Some(l) = local_sec {
            match dur {
                Some(d) if (d - l).abs() <= DURATION_TOLERANCE_SEC => {}
                _ => continue,
            }
            // 曲名もアーティストも、一致か部分一致が要る（同名の別の曲の歌詞を置かないため）
            if tm == NameMatch::None || am == NameMatch::None {
                continue;
            }
        } else if tm != NameMatch::Exact || am != NameMatch::Exact {
            // 手元に長さが無いときは、名前が両方とも一致するものだけ
            continue;
        }
        if is_instrumental(&it) {
            saw_instrumental = true;
            continue;
        }
        let (synced, plain) = match (clean_lyrics(str_field(&it, "syncedLyrics")), clean_lyrics(str_field(&it, "plainLyrics"))) {
            (Ok(s), Ok(p)) => (s, p),
            _ => {
                saw_bad = true;
                continue;
            }
        };
        if synced.is_none() && plain.is_none() {
            continue;
        }
        let d = match (dur, local_sec) {
            (Some(d), Some(l)) => Some(d - l),
            _ => None,
        };
        let key = (
            if synced.is_some() { 0 } else { 1 },
            if tm == NameMatch::Exact { 0 } else { 1 },
            if am == NameMatch::Exact { 0 } else { 1 },
            d.map(f64::abs).unwrap_or(0.0),
        );
        let idx = cands.len();
        cands.push(Cand { key, idx, synced, plain, d, tm, am });
    }
    if cands.is_empty() {
        if saw_bad {
            return Err(BAD_LYRICS_TEXT.into());
        }
        if saw_instrumental {
            return Ok(Outcome { via: Some("search"), ..Outcome::simple(Status::Instrumental) });
        }
        return Ok(Outcome::simple(Status::NotFound));
    }
    cands.sort_by(|a, b| {
        a.key
            .0
            .cmp(&b.key.0)
            .then(a.key.1.cmp(&b.key.1))
            .then(a.key.2.cmp(&b.key.2))
            .then(a.key.3.partial_cmp(&b.key.3).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.idx.cmp(&b.idx))
    });
    let best = cands.swap_remove(0);
    let (status, text) = choose_text(best.synced, best.plain);
    Ok(Outcome {
        status: status.unwrap_or(Status::NotFound),
        text,
        diff: best.d.map(round2),
        via: Some("search"),
        title_match: Some(best.tm),
        artist_match: Some(best.am),
    })
}

// ---------------------------------------------------------------- 名前の照合

/// Python の str.casefold に近づける（to_lowercase に、違いの出る主なもの ß・ẞ・ς を足す）
fn casefold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'ß' | 'ẞ' => out.push_str("ss"),
            'ς' => out.push('σ'),
            _ => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// NFKC・casefold・文字と数字だけ残す（何も残らなければ前後の空白を外したもの）。スクリプトの norm_name。
/// スクリプトとの違い：「文字」は Rust の is_alphanumeric（Unicode の Alphabetic＋数字）。
/// インド系文字の母音記号など、Alphabetic に含まれる結合記号も残る（Python は L と N の分類だけ）
pub fn norm_name(s: Option<&str>) -> String {
    let Some(s) = s else { return String::new() };
    let cut: String = s.chars().take(MAX_NAME_CHARS * 2).collect();
    let n = casefold(&crate::platform::windows::unicode::nfkc(&cut));
    let kept: String = n.chars().filter(|c| c.is_alphanumeric()).collect();
    if kept.is_empty() {
        n.trim().to_string()
    } else {
        kept
    }
}

fn name_match_norm(a: &str, b: &str) -> NameMatch {
    if a.is_empty() || b.is_empty() {
        return NameMatch::None;
    }
    if a == b {
        return NameMatch::Exact;
    }
    // 部分一致は縛る。短い名前（SID・ZAQ・ナノ・ゆず など）は別の長い名前にいくらでも含まれるため
    let (la, lb) = (a.chars().count(), b.chars().count());
    let (short, long, ls, ll) = if la <= lb { (a, b, la, lb) } else { (b, a, lb, la) };
    if ls < MIN_PARTIAL_CHARS {
        return NameMatch::None; // 正規化して3文字未満なら完全一致だけ
    }
    if long.contains(short) && ls * PARTIAL_RATIO_DEN >= ll * PARTIAL_RATIO_NUM {
        return NameMatch::Partial; // 短い方が長い方の半分以上
    }
    NameMatch::None
}

pub fn name_match(remote: Option<&str>, query: &str) -> NameMatch {
    name_match_norm(&norm_name(remote), &norm_name(Some(query)))
}

// ---------------------------------------------------------------- 送るもの

/// 1曲ぶんの問い合わせ。送るのはこの4つだけ（パス・曲 ID は送らない）
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub track_name: String,
    pub artist_name: String,
    pub album_name: Option<String>,
    /// 秒（1〜3600 のときだけ）
    pub duration: Option<i64>,
}

pub fn duration_param(duration_ms: i64) -> Option<i64> {
    if duration_ms <= 0 {
        return None;
    }
    let sec = duration_ms.saturating_add(500) / 1000;
    (1..=3600).contains(&sec).then_some(sec)
}

/// 手元の曲名・アーティスト名・アルバム名・長さから組む。曲名かアーティストが空・長すぎるなら None（送らない）
pub fn build_query(title: &str, artist: &str, album: &str, duration_ms: i64) -> Option<Query> {
    let (t, a, al) = (title.trim(), artist.trim(), album.trim());
    if t.is_empty() || a.is_empty() {
        return None;
    }
    if t.chars().count() > MAX_NAME_CHARS || a.chars().count() > MAX_NAME_CHARS {
        return None;
    }
    let album_name = (!al.is_empty() && al.chars().count() <= MAX_NAME_CHARS).then(|| al.to_string());
    Some(Query {
        track_name: t.to_string(),
        artist_name: a.to_string(),
        album_name,
        duration: duration_param(duration_ms),
    })
}

/// 問い合わせの印（曲名などを直したら、覚えた「見つからない」を使わずに聞き直すため）。FNV-1a 64bit の16進
pub fn query_sig(q: &Query) -> String {
    let raw = serde_json::to_string(&(&q.track_name, &q.artist_name, &q.album_name, &q.duration)).unwrap_or_default();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in raw.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// 1曲を引く（get → 見つからなければ search）
pub fn lookup<T: Transport, E: Env>(client: &mut Client<'_, T, E>, q: &Query, local_sec: Option<f64>) -> Result<Outcome, GetError> {
    let mut params: Vec<(&str, String)> = vec![("track_name", q.track_name.clone()), ("artist_name", q.artist_name.clone())];
    if let Some(al) = &q.album_name {
        params.push(("album_name", al.clone()));
    }
    if let Some(d) = q.duration {
        params.push(("duration", d.to_string()));
    }
    let (status, body) = client.get(API_GET, &params)?;
    if status == 200 {
        if let Some(oc) = outcome_from_get(&body, local_sec).map_err(GetError::Fetch)? {
            return Ok(oc);
        }
    }
    let (status, body) = client.get(API_SEARCH, &[("track_name", q.track_name.clone()), ("artist_name", q.artist_name.clone())])?;
    if status == 404 {
        return Ok(Outcome::simple(Status::NotFound));
    }
    outcome_from_search(&body, &q.track_name, &q.artist_name, local_sec).map_err(GetError::Fetch)
}

// ---------------------------------------------------------------- テスト用の偽物

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::collections::VecDeque;

    /// 用意した応答を順に返す。送られた URL を全部覚える。応答が尽きたら 404
    #[derive(Default)]
    pub struct FakeTransport {
        pub replies: VecDeque<Result<HttpResponse, TransportError>>,
        pub sent: Vec<String>,
        pub user_agents: Vec<String>,
    }

    impl FakeTransport {
        pub fn push(&mut self, status: u32, body: &str) -> &mut Self {
            self.replies.push_back(Ok(HttpResponse { status, retry_after: None, body: body.as_bytes().to_vec() }));
            self
        }
        pub fn push_ra(&mut self, status: u32, retry_after: &str) -> &mut Self {
            self.replies.push_back(Ok(HttpResponse { status, retry_after: Some(retry_after.into()), body: vec![] }));
            self
        }
        pub fn push_err(&mut self, e: TransportError) -> &mut Self {
            self.replies.push_back(Err(e));
            self
        }
    }

    impl Transport for FakeTransport {
        fn get(&mut self, url: &str, ua: &str, max_bytes: usize, _c: &dyn Fn() -> bool) -> Result<HttpResponse, TransportError> {
            check_allowed(url).map_err(|_| TransportError::NotAllowed)?;
            self.sent.push(url.to_string());
            self.user_agents.push(ua.to_string());
            let r = self.replies.pop_front().unwrap_or(Ok(HttpResponse { status: 404, retry_after: None, body: vec![] }));
            if let Ok(resp) = &r {
                if resp.body.len() > max_bytes {
                    return Err(TransportError::TooLarge);
                }
            }
            r
        }
    }

    /// 呼ばれたら落ちる（オフのとき・許可リスト外で、通信の処理にたどり着かないことを確かめる）
    pub struct PanicTransport;
    impl Transport for PanicTransport {
        fn get(&mut self, _: &str, _: &str, _: usize, _: &dyn Fn() -> bool) -> Result<HttpResponse, TransportError> {
            panic!("transport must not be reached");
        }
    }

    /// 実時間を使わない時計。sleep は時計を進めて記録するだけ。stop_after_sleeps 回目の sleep で Cancelled
    #[derive(Default)]
    pub struct FakeEnv {
        pub t: f64,
        pub sleeps: Vec<f64>,
        pub rand_value: f64,
        pub epoch_now: f64,
        pub stop_after_sleeps: Option<usize>,
    }

    impl Env for FakeEnv {
        fn sleep(&mut self, secs: f64) -> Result<(), Stop> {
            self.sleeps.push(secs);
            self.t += secs;
            if self.stop_after_sleeps.is_some_and(|n| self.sleeps.len() >= n) {
                return Err(Stop::Cancelled);
            }
            Ok(())
        }
        fn rand(&mut self) -> f64 {
            self.rand_value
        }
        fn now(&mut self) -> f64 {
            self.t
        }
        fn epoch(&mut self) -> f64 {
            self.epoch_now
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::*;
    use super::*;

    fn client(t: FakeTransport) -> Client<'static, FakeTransport, FakeEnv> {
        Client::new(t, FakeEnv::default(), || None)
    }

    fn q() -> Query {
        build_query("Song Title", "The Artist", "Album", 200_000).unwrap()
    }

    #[test]
    fn allowlist_and_url() {
        assert!(check_allowed(API_GET).is_ok());
        assert!(check_allowed(&format!("{API_SEARCH}?track_name=a+b&artist_name=%E3%81%82")).is_ok());
        for bad in [
            "http://lrclib.net/api/get",
            "https://lrclib.net:443/api/get",
            "https://user@lrclib.net/api/get",
            "https://lrclib.net/api/publish",
            "https://lrclib.net/api/get/",
            "https://lrclib.net.evil.example/api/get",
            "https://evil.example/api/get",
            "https://lrclib.net/api/get?a=b#frag",
            "https://lrclib.net/api/get?a=b c",
            "https://lrclib.net/api/get?a=b/../x",
            "HTTPS://LRCLIB.NET/api/get",
        ] {
            assert_eq!(check_allowed(bad), Err(NotAllowed), "{bad}");
        }
        assert!(build_url("https://evil.example/api/get", &[]).is_err());
        let u = build_url(API_GET, &[("track_name", "A B&C=D/é?#".into()), ("duration", "200".into())]).unwrap();
        assert_eq!(u, "https://lrclib.net/api/get?track_name=A+B%26C%3DD%2F%C3%A9%3F%23&duration=200");
        assert_eq!(path_and_query(&u).unwrap(), "/api/get?track_name=A+B%26C%3DD%2F%C3%A9%3F%23&duration=200");
        assert!(path_and_query("https://evil.example/api/get").is_err());
    }

    #[test]
    fn user_agent_names_app_and_version_only() {
        let ua = user_agent();
        assert_eq!(ua, format!("PL-Player/{}", env!("CARGO_PKG_VERSION")));
        assert!(!ua.contains('@'));
    }

    #[test]
    fn retry_after_values() {
        assert_eq!(parse_retry_after(Some("120"), 0.0), Some(120.0));
        assert_eq!(parse_retry_after(Some(" 7 "), 0.0), Some(7.0));
        // 上付きの数字・全角の数字・負の数・小数は秒として読まない（日付としても読めない）
        for v in ["²", "１２", "-5", "1.5", "", "   ", "abc"] {
            assert_eq!(parse_retry_after(Some(v), 0.0), None, "{v:?}");
        }
        assert_eq!(parse_retry_after(None, 0.0), None);
        // 64文字を超えるものは読まない
        assert_eq!(parse_retry_after(Some(&"9".repeat(65)), 0.0), None);
        // 巨大な数（64桁）は読めるが、上限を超えるので呼び出し側が打ち切る
        assert!(parse_retry_after(Some(&"9".repeat(64)), 0.0).unwrap() > MAX_RETRY_AFTER_SEC);
        // HTTP の日付
        let t = parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
        assert_eq!(t, 784111777.0);
        assert_eq!(parse_http_date("Sunday, 06-Nov-94 08:49:37 GMT"), Some(784111777.0));
        assert_eq!(parse_http_date("Sun Nov  6 08:49:37 1994"), Some(784111777.0));
        assert_eq!(parse_http_date("Sun, 06 Nov 1994 17:49:37 +0900"), Some(784111777.0));
        assert_eq!(parse_retry_after(Some("Sun, 06 Nov 1994 08:49:47 GMT"), 784111777.0), Some(10.0));
        // 過去の日付は 0
        assert_eq!(parse_retry_after(Some("Sun, 06 Nov 1994 08:49:37 GMT"), 784111800.0), Some(0.0));
        for bad in ["Sun, 31 Feb 1994 08:49:37 GMT", "Sun, 06 Foo 1994 08:49:37 GMT", "Sun, 06 Nov 1994 25:00:00 GMT", "06 Nov 99999 08:49:37 GMT", "Sun, 06 Nov 1994 08:49:60 GMT"] {
            assert_eq!(parse_http_date(bad), None, "{bad}");
        }
    }

    #[test]
    fn get_synced_plain_instrumental() {
        let mut t = FakeTransport::default();
        t.push(200, r#"{"id":1,"trackName":"x","duration":201.4,"instrumental":false,"syncedLyrics":"\ufeff[00:01.00]a\r\n[00:02.00]b","plainLyrics":"a\nb"}"#);
        let mut c = client(t);
        let oc = lookup(&mut c, &q(), Some(200.0)).unwrap();
        assert_eq!(oc.status, Status::Synced);
        assert_eq!(oc.text.as_deref(), Some("[00:01.00]a\n[00:02.00]b\n"));
        assert_eq!(oc.diff, Some(1.4));
        assert_eq!(oc.via, Some("get"));
        assert_eq!(c.transport.sent.len(), 1);
        assert_eq!(c.transport.sent[0], "https://lrclib.net/api/get?track_name=Song+Title&artist_name=The+Artist&album_name=Album&duration=200");

        let mut t = FakeTransport::default();
        t.push(200, r#"{"duration":200,"syncedLyrics":null,"plainLyrics":"line one\nline two"}"#);
        let oc = lookup(&mut client(t), &q(), Some(200.0)).unwrap();
        assert_eq!(oc.status, Status::Plain);
        assert_eq!(oc.text.as_deref(), Some("line one\nline two\n"));

        let mut t = FakeTransport::default();
        t.push(200, r#"{"duration":200,"instrumental":true,"syncedLyrics":"[00:01.00]x"}"#);
        let oc = lookup(&mut client(t), &q(), Some(200.0)).unwrap();
        assert_eq!(oc.status, Status::Instrumental);
        assert!(oc.text.is_none());
    }

    #[test]
    fn get_404_then_search_rules() {
        let mut t = FakeTransport::default();
        t.push(404, "");
        // 候補：長さが外れる・アーティストが違う・instrumental・全文だけ・時刻つき（選ばれる）
        t.push(
            200,
            r#"[
              {"trackName":"Song Title","artistName":"The Artist","duration":230,"syncedLyrics":"[00:01.00]far"},
              {"trackName":"Song Title","artistName":"Somebody Else","duration":200,"syncedLyrics":"[00:01.00]other"},
              {"trackName":"Song Title","artistName":"The Artist","duration":200,"instrumental":true},
              {"trackName":"Song Title","artistName":"The Artist","duration":200.5,"plainLyrics":"plain only"},
              {"trackName":"SONG TITLE","artistName":"the artist","duration":201.9,"syncedLyrics":"[00:01.00]chosen"},
              "not an object", 5, null
            ]"#,
        );
        let mut c = client(t);
        let oc = lookup(&mut c, &q(), Some(200.0)).unwrap();
        assert_eq!(oc.status, Status::Synced);
        assert_eq!(oc.text.as_deref(), Some("[00:01.00]chosen\n"));
        assert_eq!(oc.via, Some("search"));
        assert_eq!(oc.title_match, Some(NameMatch::Exact));
        assert_eq!(c.transport.sent.len(), 2);
        assert_eq!(c.transport.sent[1], "https://lrclib.net/api/search?track_name=Song+Title&artist_name=The+Artist");

        // 候補が instrumental だけ → instrumental。search も 404 → 見つからない
        let mut t = FakeTransport::default();
        t.push(404, "").push(200, r#"[{"trackName":"Song Title","artistName":"The Artist","duration":200,"instrumental":true}]"#);
        assert_eq!(lookup(&mut client(t), &q(), Some(200.0)).unwrap().status, Status::Instrumental);
        let mut t = FakeTransport::default();
        t.push(404, "").push(404, "");
        assert_eq!(lookup(&mut client(t), &q(), Some(200.0)).unwrap().status, Status::NotFound);
        // get の歌詞が空 → search へ
        let mut t = FakeTransport::default();
        t.push(200, r#"{"duration":200,"syncedLyrics":"  ","plainLyrics":""}"#).push(200, "[]");
        let mut c = client(t);
        assert_eq!(lookup(&mut c, &q(), Some(200.0)).unwrap().status, Status::NotFound);
        assert_eq!(c.transport.sent.len(), 2);
        // 手元に長さが無いときは、名前が両方とも完全一致のものだけ
        let mut t = FakeTransport::default();
        t.push(404, "").push(200, r#"[{"trackName":"Song Title (Live)","artistName":"The Artist","duration":10,"plainLyrics":"x"},{"trackName":"song title","artistName":"THE ARTIST","duration":999,"plainLyrics":"y"}]"#);
        let q0 = build_query("Song Title", "The Artist", "", 0).unwrap();
        assert_eq!(q0.duration, None);
        let oc = lookup(&mut client(t), &q0, None).unwrap();
        assert_eq!(oc.text.as_deref(), Some("y\n"));
    }

    #[test]
    fn short_names_match_narrowly() {
        // 正規化して3文字未満は完全一致だけ
        assert_eq!(name_match(Some("ナノ"), "ナノ"), NameMatch::Exact);
        assert_eq!(name_match(Some("ナノウ"), "ナノ"), NameMatch::None);
        assert_eq!(name_match(Some("Totally Other"), "A"), NameMatch::None);
        assert_eq!(name_match(Some("ゆず"), "ゆず (Yuzu)"), NameMatch::None);
        // 3文字以上でも、短い方が長い方の半分以上のときだけ部分一致
        assert_eq!(name_match(Some("SID"), "SIDE"), NameMatch::Partial);
        assert_eq!(name_match(Some("ZAQ"), "ZAQ and Friends Orchestra"), NameMatch::None);
        assert_eq!(name_match(Some("Song (Remastered)"), "Song"), NameMatch::None);
        assert_eq!(name_match(Some("Song Title (Live)"), "Song Title"), NameMatch::Partial);
        // 大文字小文字・全角半角・記号は無視
        assert_eq!(name_match(Some("ＳＯＮＧ－ＴＩＴＬＥ！"), "song title"), NameMatch::Exact);
        assert_eq!(name_match(Some("ｱｲｳｴｵ"), "アイウエオ"), NameMatch::Exact);
        assert_eq!(name_match(Some("Straße"), "STRASSE"), NameMatch::Exact);
        // 空・文字列でない
        assert_eq!(name_match(None, "x"), NameMatch::None);
        assert_eq!(name_match(Some(""), "x"), NameMatch::None);
        // 記号だけの名前は、記号のまま比べる
        assert_eq!(name_match(Some(" !!! "), "!!!"), NameMatch::Exact);

        // search で：短いアーティスト名「A」は、長い別の名前の候補に当たらない
        let mut t = FakeTransport::default();
        t.push(404, "").push(200, r#"[{"trackName":"Song Title","artistName":"Totally Other A Band","duration":200,"syncedLyrics":"[00:01.00]x"}]"#);
        let qa = build_query("Song Title", "A", "", 200_000).unwrap();
        assert_eq!(lookup(&mut client(t), &qa, Some(200.0)).unwrap().status, Status::NotFound);
    }

    #[test]
    fn rate_limit_and_unavailable() {
        // 429 → Retry-After＋ゆらぎで待って再試行
        let mut t = FakeTransport::default();
        t.push_ra(429, "3").push(200, r#"{"duration":200,"plainLyrics":"x"}"#);
        let mut c = client(t);
        c.env.rand_value = 0.5;
        assert_eq!(lookup(&mut c, &q(), Some(200.0)).unwrap().status, Status::Plain);
        assert_eq!(c.env.sleeps.len(), 1);
        assert!((c.env.sleeps[0] - 3.45).abs() < 1e-9, "{:?}", c.env.sleeps);
        // 上付きの数字の Retry-After → 見出しが無いときと同じ（5秒から倍）
        let mut t = FakeTransport::default();
        t.push_ra(429, "²").push(200, r#"{"duration":200,"plainLyrics":"x"}"#);
        let mut c = client(t);
        lookup(&mut c, &q(), Some(200.0)).unwrap();
        assert_eq!(c.env.sleeps, vec![5.0]);
        // 300 秒を超える → 待たずに全体を打ち切る
        let mut t = FakeTransport::default();
        t.push_ra(429, "301");
        let mut c = client(t);
        assert_eq!(lookup(&mut c, &q(), Some(200.0)), Err(GetError::Abort("rate_limited_long_wait")));
        assert!(c.env.sleeps.is_empty());
        // 5回続く → 打ち切り
        let mut t = FakeTransport::default();
        for _ in 0..5 {
            t.push_ra(429, "1");
        }
        let mut c = client(t);
        assert_eq!(lookup(&mut c, &q(), Some(200.0)), Err(GetError::Abort("rate_limited_repeatedly")));
        assert_eq!(c.transport.sent.len(), 5);
        // 503 → 待って再試行。5回でその曲をエラー
        let mut t = FakeTransport::default();
        t.push_ra(503, "2").push(200, r#"{"duration":200,"plainLyrics":"x"}"#);
        let mut c = client(t);
        assert_eq!(lookup(&mut c, &q(), Some(200.0)).unwrap().status, Status::Plain);
        assert_eq!(c.env.sleeps, vec![2.0]);
        let mut t = FakeTransport::default();
        for _ in 0..5 {
            t.push(503, "");
        }
        assert_eq!(lookup(&mut client(t), &q(), Some(200.0)), Err(GetError::Fetch("http_503".into())));
        // 通信エラーは 1・2・4・8 秒で再試行
        let mut t = FakeTransport::default();
        for _ in 0..5 {
            t.push_err(TransportError::Failed("timeout"));
        }
        let mut c = client(t);
        assert_eq!(lookup(&mut c, &q(), Some(200.0)), Err(GetError::Fetch("timeout".into())));
        assert_eq!(c.env.sleeps, vec![1.0, 2.0, 4.0, 8.0]);
        // リダイレクト（3xx）はたどらずにエラー
        let mut t = FakeTransport::default();
        t.push(301, "");
        assert_eq!(lookup(&mut client(t), &q(), Some(200.0)), Err(GetError::Fetch("http_301".into())));
        let mut t = FakeTransport::default();
        t.push(4_000_000_000, "");
        assert_eq!(lookup(&mut client(t), &q(), Some(200.0)), Err(GetError::Fetch("http_bad_status".into())));
    }

    #[test]
    fn requests_are_spaced() {
        let mut t = FakeTransport::default();
        t.push(404, "").push(404, "");
        let mut c = client(t);
        c.env.rand_value = 0.0;
        lookup(&mut c, &q(), Some(200.0)).unwrap();
        // 2つ目の前に 300ms（時計は進んでいないので、まるごと待つ）
        assert_eq!(c.env.sleeps, vec![0.3]);
        // 前の操作から引き継いだ時刻でも間を空ける
        let mut t = FakeTransport::default();
        t.push(404, "").push(404, "");
        let mut c = client(t);
        c.env.rand_value = 1.0;
        c.last = Some(0.0);
        c.env.t = 0.1;
        lookup(&mut c, &q(), Some(200.0)).unwrap();
        assert!((c.env.sleeps[0] - 0.4).abs() < 1e-9, "{:?}", c.env.sleeps);
    }

    #[test]
    fn gate_stops_before_transport() {
        let mut c = Client::new(PanicTransport, FakeEnv::default(), || Some(Stop::Disabled));
        assert_eq!(lookup(&mut c, &q(), Some(200.0)), Err(GetError::Stop(Stop::Disabled)));
        assert_eq!(c.requests_sent, 0);
        let mut c = Client::new(PanicTransport, FakeEnv::default(), || Some(Stop::Cancelled));
        assert_eq!(lookup(&mut c, &q(), Some(200.0)), Err(GetError::Stop(Stop::Cancelled)));
        // 許可リスト外の宛先は組む段階で止まり、通信の処理へ進まない
        let mut c = Client::new(PanicTransport, FakeEnv::default(), || None);
        assert_eq!(c.get("https://evil.example/api/get", &[]), Err(GetError::Fetch("not_allowed".into())));
        assert_eq!(c.get("https://lrclib.net/api/publish", &[]), Err(GetError::Fetch("not_allowed".into())));
        assert_eq!(c.requests_sent, 0);
    }

    #[test]
    fn extreme_responses_do_not_crash() {
        let run = |body: Vec<u8>| -> Result<Outcome, GetError> {
            let mut t = FakeTransport::default();
            t.replies.push_back(Ok(HttpResponse { status: 200, retry_after: None, body }));
            t.push(200, "[]");
            lookup(&mut client(t), &q(), Some(200.0))
        };
        // 巨大な応答（16MiB 超）は読まずにエラー
        let big = vec![b' '; MAX_RESPONSE_BYTES + 1];
        assert_eq!(run(big), Err(GetError::Fetch("response_too_large".into())));
        // 壊れた JSON・UTF-8 でない・BOM つき・形が違う
        for b in [&b"{not json"[..], b"\xff\xfe{}", b"\xef\xbb\xbf{}", b"[]", b"\"str\"", b""] {
            assert_eq!(run(b.to_vec()), Err(GetError::Fetch(BAD_RESPONSE.into())), "{b:?}");
        }
        // 深い入れ子
        let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        assert!(run(deep.clone().into_bytes()).is_err());
        let deep_field = format!(r#"{{"duration":200,"plainLyrics":"ok","x":{}{}}}"#, "[".repeat(100_000), "]".repeat(100_000));
        assert!(run(deep_field.into_bytes()).is_ok());
        // 巨大な数・数でない duration（get）→ bad_response
        for d in ["1e999", &"9".repeat(400), "\"200\"", "true", "-1e400"] {
            let b = format!(r#"{{"duration":{d},"plainLyrics":"x"}}"#);
            assert_eq!(run(b.into_bytes()), Err(GetError::Fetch(BAD_RESPONSE.into())), "{d}");
        }
        // ほかの欄の巨大な数は関係ない
        assert_eq!(run(br#"{"id":1e999,"duration":200,"plainLyrics":"x"}"#.to_vec()).unwrap().status, Status::Plain);
        // 単独サロゲート：歌詞にあれば bad_lyrics_text、名前にあっても落ちない
        assert_eq!(run(br#"{"duration":200,"plainLyrics":"a\ud800b"}"#.to_vec()), Err(GetError::Fetch(BAD_LYRICS_TEXT.into())));
        assert_eq!(run(br#"{"duration":200,"plainLyrics":"a\udc00"}"#.to_vec()), Err(GetError::Fetch(BAD_LYRICS_TEXT.into())));
        assert_eq!(run(br#"{"trackName":"\ud800","duration":200,"plainLyrics":"ok \ud83c\udfb5 \\ud800"}"#.to_vec()).unwrap().text.as_deref(), Some("ok \u{1F3B5} \\ud800\n"));
        // 1MiB を超える歌詞は保存しない。時刻つきが大きすぎて全文が収まれば全文
        let huge = "a".repeat(MAX_LYRICS_BYTES + 1);
        let b = format!(r#"{{"duration":200,"syncedLyrics":"[00:01.00]{huge}"}}"#);
        assert_eq!(run(b.into_bytes()).unwrap().status, Status::TooLarge);
        let b = format!(r#"{{"duration":200,"syncedLyrics":"[00:01.00]{huge}","plainLyrics":"small"}}"#);
        assert_eq!(run(b.into_bytes()).unwrap().status, Status::Plain);
        // 情報行だけの時刻つき（取り込みで空になるもの）は使わず全文へ
        assert_eq!(run(br#"{"duration":200,"syncedLyrics":"[ti:x]","plainLyrics":"p"}"#.to_vec()).unwrap().status, Status::Plain);

        // search：壊れた候補は飛ばす。巨大な数の候補は外す
        let mut t = FakeTransport::default();
        t.push(404, "").push(200, r#"[{"trackName":"Song Title","artistName":"The Artist","duration":1e999,"plainLyrics":"bad"},{"trackName":"Song Title","artistName":"The Artist","duration":200,"plainLyrics":"bad\udfff"},{"trackName":"Song Title","artistName":"The Artist","duration":200,"plainLyrics":"good"}]"#);
        assert_eq!(lookup(&mut client(t), &q(), Some(200.0)).unwrap().text.as_deref(), Some("good\n"));
        // 単独サロゲートの候補しか無い → bad_lyrics_text
        let mut t = FakeTransport::default();
        t.push(404, "").push(200, r#"[{"trackName":"Song Title","artistName":"The Artist","duration":200,"plainLyrics":"bad\udfff"}]"#);
        assert_eq!(lookup(&mut client(t), &q(), Some(200.0)), Err(GetError::Fetch(BAD_LYRICS_TEXT.into())));
        // search の候補が多くても、先頭 50 件だけ見る
        let mut items: Vec<String> = (0..60).map(|_| r#"{"trackName":"x","artistName":"y","duration":200}"#.to_string()).collect();
        items.push(r#"{"trackName":"Song Title","artistName":"The Artist","duration":200,"plainLyrics":"late"}"#.into());
        let mut t = FakeTransport::default();
        t.push(404, "").push(200, &format!("[{}]", items.join(",")));
        assert_eq!(lookup(&mut client(t), &q(), Some(200.0)).unwrap().status, Status::NotFound);
    }

    #[test]
    fn query_rules() {
        assert!(build_query("", "a", "", 1000).is_none());
        assert!(build_query("t", "  ", "", 1000).is_none());
        assert!(build_query(&"x".repeat(MAX_NAME_CHARS + 1), "a", "", 1000).is_none());
        let q = build_query(" t ", " a ", &"y".repeat(MAX_NAME_CHARS + 1), 0).unwrap();
        assert_eq!((q.track_name.as_str(), q.artist_name.as_str(), q.album_name.clone(), q.duration), ("t", "a", None, None));
        assert_eq!(duration_param(499), None);
        assert_eq!(duration_param(500), Some(1));
        assert_eq!(duration_param(3_600_499), Some(3600));
        assert_eq!(duration_param(3_600_500), None);
        assert_eq!(duration_param(i64::MAX), None);
        let a = build_query("t", "a", "al", 1000).unwrap();
        let b = build_query("t", "a", "al2", 1000).unwrap();
        assert_ne!(query_sig(&a), query_sig(&b));
        assert_eq!(query_sig(&a), query_sig(&a.clone()));
        assert_eq!(query_sig(&a).len(), 16);
    }
}
