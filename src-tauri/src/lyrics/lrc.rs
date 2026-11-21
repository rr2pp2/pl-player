//! LRC の読み方（仕様 4章）。ファイルも DB も触らない純粋な関数。
//!
//! - 時刻タグ `[m:ss]`〜`[mm:ss.fff]`。小数は右を0で埋めて読む（`.2` → 200ms）
//! - 情報行（`[ti:]` など `[英字:値]`）は表示しない。`[offset:]` は最初の有効な値を使い、正の値は歌詞を早める
//! - 時刻つきで本文が空の行は「今の行を消す」合図
//! - 語ごとの時刻 `<mm:ss.xx>` は表示から外す（原文には残す）
//! - 有効な時刻が1つも無ければ時刻なし（全文）
//!
//! 警告はコード（文字列）で返す。文は画面側の `src/messages/ja.ts`。

use serde::Serialize;

/// 警告のコード（`src/messages/ja.ts` の lyricsWarningText と同じ）
pub mod warn {
    /// 秒が60以上・小数が4桁以上など、時刻として使えないタグがあった（原文は残す）
    pub const TIMESTAMP_INVALID: &str = "timestamp_invalid";
    /// 行の時刻が前の行より早く、並べ替えた
    pub const REORDERED: &str = "timestamps_reordered";
    /// offset を引くと 0 未満になる時刻があった（表示では 0 にそろえる。式は画面側の max(0, 元 − offset + 補正)）
    pub const OFFSET_CLAMPED: &str = "offset_clamped";
    /// 時刻つきの中に時刻なしの本文行があった（末尾にまとめて出す）
    pub const UNTIMED_LINES: &str = "untimed_lines";
    /// 1行の時刻が多すぎる、または展開すると大きすぎるため、一部の時刻を使わなかった（原文は残す）
    pub const TIMESTAMPS_TRUNCATED: &str = "timestamps_truncated";
    /// 行が多すぎるため、先頭の MAX_LINES 行だけを使った（v3.1.1。原文は残す）
    pub const LINES_TRUNCATED: &str = "lines_truncated";
}

/// 解析に渡す原文の上限（1MiB）。受け渡しフォルダ・編集画面の見本・手動保存・曲のタグの歌詞のどれにも、
/// 解析する前に当てる（超えたら解析しない）
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;

/// 1行に書ける時刻の数の上限。超えた分は使わない（サビの繰り返しなら数十で足りる）
pub const MAX_TIMES_PER_LINE: usize = 100;

/// 1行に複数の時刻があるとき、同じ本文を時刻の数だけ複製する。その複製の合計（バイト）の上限。
/// 各行の最初の時刻は必ず使い、2つ目からの複製だけをこの上限で止める（極端な入力でメモリを食い尽くさないため）
pub const MAX_DUPLICATED_BYTES: usize = 4 * 1024 * 1024;

/// 読み解いた歌詞の行の数の上限（v3.1.1。`docs\ペネトレーションテスト_v3.0.2.md` の10、作者指示）。
/// 普通の歌詞は数十〜数百行で届かない。1MiB の原文で約13万行になり、歌詞画面が全部の行を描いて固まるおそれがあった。
/// 超えた分は捨てて警告する（時刻つきは時刻の順に並べたあとの先頭から。時刻なしの行は末尾にあるので先に落ちる）。
/// 取り込み・手動保存・曲のタグ・ネットから取った歌詞のどれも、表示はこの関数を通るので、同じく当たる
pub const MAX_LINES: usize = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LyricsFormat {
    Timed,
    Plain,
}

impl LyricsFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            LyricsFormat::Timed => "timed",
            LyricsFormat::Plain => "plain",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    /// 時刻つきの歌詞の行
    Lyric,
    /// 時刻つきで本文が空：「今の行を消す」合図（行としては表示しない）
    Clear,
    /// 時刻なしの行（全文の各行、または時刻つきの中に混ざった時刻なしの行）
    Untimed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsLine {
    pub key: String,
    /// 元の時刻 − offset（0 未満でも切らない）。表示時刻は画面で max(0, time_ms + 曲ごとの補正)
    pub time_ms: Option<i64>,
    pub text: String,
    pub kind: LineKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLyrics {
    pub format: LyricsFormat,
    pub lines: Vec<LyricsLine>,
    /// 表示用の全文（時刻・情報行・語ごとの時刻を外したもの）
    pub plain_text: String,
    /// 情報行（キーは小文字）。表示はしないが保持する
    pub meta: Vec<(String, String)>,
    /// 使った `[offset:]` の値（無ければ 0）
    pub offset_ms: i64,
    pub warnings: Vec<String>,
}

impl ParsedLyrics {
    /// 表示できる本文が1行でもあるか（空・空白だけ・情報行だけのときは false）
    pub fn has_content(&self) -> bool {
        self.lines
            .iter()
            .any(|l| l.kind != LineKind::Clear && !l.text.trim().is_empty())
    }
}

/// BOM を外し、改行を LF にそろえる（CRLF・CR・LF のどれでも）
pub fn normalize(raw: &str) -> String {
    let s = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    s.replace("\r\n", "\n").replace('\r', "\n")
}

enum Bracket<'a> {
    /// 時刻に見えるもの。None は「時刻の形だが使えない」
    Time(Option<i64>),
    Info(&'a str, &'a str),
    Other,
}

fn looks_like_time(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_digit()) && s.chars().all(|c| c.is_ascii_digit() || c == ':' || c == '.')
}

/// `m:ss` `mm:ss.f` `mm:ss.ff` `mm:ss.fff`。分は1桁以上（60以上も可）、秒は 00〜59
fn parse_time(s: &str) -> Option<i64> {
    let (mm, rest) = s.split_once(':')?;
    if mm.is_empty() || mm.len() > 9 || !mm.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (ss, frac) = match rest.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    if ss.len() != 2 || !ss.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let min: i64 = mm.parse().ok()?;
    let sec: i64 = ss.parse().ok()?;
    if sec >= 60 {
        return None;
    }
    let frac_ms = match frac {
        None => 0,
        Some(f) => {
            if f.is_empty() || f.len() > 3 || !f.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            // 右を0で埋めて3桁にする（.2 → 200、.02 → 20、.250 → 250）
            format!("{f:0<3}").parse::<i64>().ok()?
        }
    };
    Some(min * 60_000 + sec * 1_000 + frac_ms)
}

fn classify(inner: &str) -> Bracket<'_> {
    if looks_like_time(inner) {
        return Bracket::Time(parse_time(inner));
    }
    if let Some((k, v)) = inner.split_once(':') {
        let mut chars = k.chars();
        if chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Bracket::Info(k, v);
        }
    }
    Bracket::Other
}

/// 行の中の `<mm:ss.xx>`（語ごとの時刻）を外す。
///
/// `<` のたびに残り全部から `>` を探していたため、閉じない `<` が並ぶ行で
/// 二次時間になっていた（1MiB で 21秒）。`<` の直後を時刻に使う文字（数字・`:`・`.`）の間だけ読み、
/// その次が `>` なら外す、という形にして、各文字を高々2回しか見ないようにした。
fn strip_word_times(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut copied = 0; // ここまでは out へ写した
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < b.len() && (b[j].is_ascii_digit() || b[j] == b':' || b[j] == b'.') {
            j += 1;
        }
        if j < b.len() && b[j] == b'>' && looks_like_time(&s[i + 1..j]) {
            // `<` と `>` はどちらも1バイトの ASCII なので、ここで切っても文字の途中にならない
            out.push_str(&s[copied..i]);
            copied = j + 1;
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out.push_str(&s[copied..]);
    out
}

enum LineParse {
    Info { key: String, value: String },
    Body { times: Vec<i64>, invalid_time: bool, text: String },
}

fn parse_line(line: &str) -> LineParse {
    let mut rest = line.trim_start();
    let mut times = Vec::new();
    let mut invalid_time = false;
    while rest.starts_with('[') {
        let Some(end) = rest.find(']') else { break };
        match classify(&rest[1..end]) {
            Bracket::Time(Some(ms)) => times.push(ms),
            Bracket::Time(None) => invalid_time = true,
            Bracket::Info(k, v) if times.is_empty() && !invalid_time => {
                return LineParse::Info {
                    key: k.to_ascii_lowercase(),
                    value: v.trim().to_string(),
                };
            }
            _ => break,
        }
        rest = &rest[end + 1..];
    }
    LineParse::Body {
        times,
        invalid_time,
        text: strip_word_times(rest).trim().to_string(),
    }
}

fn push_warning(w: &mut Vec<String>, code: &str) {
    if !w.iter().any(|x| x == code) {
        w.push(code.to_string());
    }
}

/// 歌詞の原文を読み解く。原文は変えない（返すのは表示用の形）
pub fn parse(raw: &str) -> ParsedLyrics {
    let text = normalize(raw);
    let mut warnings = Vec::new();
    let mut meta = Vec::new();
    let mut offset: Option<i64> = None;
    let mut bodies: Vec<(Vec<i64>, String)> = Vec::new();
    for line in text.split('\n') {
        match parse_line(line) {
            LineParse::Info { key, value } => {
                if key == "offset" && offset.is_none() {
                    offset = value.parse::<i64>().ok();
                }
                meta.push((key, value));
            }
            LineParse::Body { times, invalid_time, text } => {
                if invalid_time {
                    push_warning(&mut warnings, warn::TIMESTAMP_INVALID);
                }
                bodies.push((times, text));
            }
        }
    }
    let offset_ms = offset.unwrap_or(0);
    let timed = bodies.iter().any(|(t, _)| !t.is_empty());

    let mut lines = Vec::new();
    if timed {
        // (表示時刻, 本文)。並べ替えは安定（同じ時刻は元の順）
        let mut entries: Vec<(i64, String)> = Vec::new();
        let mut untimed: Vec<String> = Vec::new();
        let mut running_max: Option<i64> = None;
        let mut reordered = false;
        let mut clamped = false;
        let mut truncated = false;
        let mut duplicated_bytes: usize = 0;
        for (mut times, text) in bodies {
            if times.is_empty() {
                if !text.is_empty() {
                    untimed.push(text);
                }
                continue;
            }
            // 並べ替えの判定は「行ごとの最初の時刻」が前の行より早いか。
            // 1行に複数の時刻を書く形（サビの繰り返し）は、並べ替えても警告しない
            let line_min = *times.iter().min().unwrap_or(&0);
            if running_max.is_some_and(|m| line_min < m) {
                reordered = true;
            }
            running_max = Some(running_max.map_or(line_min, |m| m.max(line_min)));
            // 時刻の数だけ本文を複製していたため、時刻7万個＋長い本文で数十GBを要求し得た。
            // 1行の時刻は 100 個まで、2つ目からの複製は合計 4MiB まで（どちらも超えたら警告）
            if times.len() > MAX_TIMES_PER_LINE {
                times.truncate(MAX_TIMES_PER_LINE);
                truncated = true;
            }
            for (k, t) in times.into_iter().enumerate() {
                if k > 0 {
                    if duplicated_bytes.saturating_add(text.len()) > MAX_DUPLICATED_BYTES {
                        truncated = true;
                        break;
                    }
                    duplicated_bytes += text.len();
                }
                // 元 − offset。0 未満でも切らない（表示の 0 そろえは画面で、曲ごとの補正を足したあと）。
                // 極端な offset で桁あふれしないよう飽和演算にする
                let shifted = t.saturating_sub(offset_ms);
                if shifted < 0 {
                    clamped = true;
                }
                entries.push((shifted, text.clone()));
            }
        }
        entries.sort_by_key(|(t, _)| *t);
        if reordered {
            push_warning(&mut warnings, warn::REORDERED);
        }
        if clamped {
            push_warning(&mut warnings, warn::OFFSET_CLAMPED);
        }
        if truncated {
            push_warning(&mut warnings, warn::TIMESTAMPS_TRUNCATED);
        }
        for (t, text) in entries {
            let kind = if text.is_empty() { LineKind::Clear } else { LineKind::Lyric };
            lines.push(LyricsLine {
                key: String::new(),
                time_ms: Some(t),
                text,
                kind,
            });
        }
        if !untimed.is_empty() {
            push_warning(&mut warnings, warn::UNTIMED_LINES);
        }
        for text in untimed {
            lines.push(LyricsLine {
                key: String::new(),
                time_ms: None,
                text,
                kind: LineKind::Untimed,
            });
        }
    } else {
        // 全文：空行はそのまま残す（前後の空行だけ落とす）
        let mut texts: Vec<String> = bodies.into_iter().map(|(_, t)| t).collect();
        while texts.last().is_some_and(|t| t.is_empty()) {
            texts.pop();
        }
        let first = texts.iter().position(|t| !t.is_empty()).unwrap_or(texts.len());
        for text in texts.into_iter().skip(first) {
            lines.push(LyricsLine {
                key: String::new(),
                time_ms: None,
                text,
                kind: LineKind::Untimed,
            });
        }
    }
    if lines.len() > MAX_LINES {
        lines.truncate(MAX_LINES);
        push_warning(&mut warnings, warn::LINES_TRUNCATED);
    }
    for (i, l) in lines.iter_mut().enumerate() {
        l.key = format!("l{i}");
    }
    let plain_text = lines
        .iter()
        .filter(|l| l.kind != LineKind::Clear)
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    ParsedLyrics {
        format: if timed { LyricsFormat::Timed } else { LyricsFormat::Plain },
        lines,
        plain_text,
        meta,
        offset_ms,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn times(p: &ParsedLyrics) -> Vec<i64> {
        p.lines.iter().filter_map(|l| l.time_ms).collect()
    }
    fn texts(p: &ParsedLyrics) -> Vec<&str> {
        p.lines.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn several_times_on_one_line() {
        let p = parse("[00:12.00][01:30.00]同じ場所へ戻る");
        assert_eq!(p.format, LyricsFormat::Timed);
        assert_eq!(times(&p), vec![12_000, 90_000]);
        assert_eq!(texts(&p), vec!["同じ場所へ戻る", "同じ場所へ戻る"]);
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    }

    #[test]
    fn positive_offset_makes_lyrics_earlier() {
        let p = parse("[offset:+200]\n[00:10.00]遠くで音がする");
        assert_eq!(p.offset_ms, 200);
        assert_eq!(times(&p), vec![9_800]);
        assert_eq!(texts(&p), vec!["遠くで音がする"]);
    }

    #[test]
    fn fractions_are_padded_on_the_right() {
        let p = parse("[01:02]一\n[01:02.2]二\n[01:02.02]三\n[01:02.250]四");
        assert_eq!(times(&p), vec![62_000, 62_020, 62_200, 62_250]);
        assert_eq!(texts(&p), vec!["一", "三", "二", "四"]);
    }

    #[test]
    fn info_lines_are_hidden_but_kept() {
        let p = parse("[ti:題名]\n[ar:歌う人]\n[al:アルバム]\n[by:作った人]\n[00:01.00]夜が明ける");
        assert_eq!(texts(&p), vec!["夜が明ける"]);
        assert_eq!(p.lines.len(), 1);
        let keys: Vec<&str> = p.meta.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["ti", "ar", "al", "by"]);
        assert_eq!(p.meta[0].1, "題名");
        // キーの大文字小文字は区別しない・ほかの [英字:値] も情報行
        let q = parse("[TI:x]\n[Length:03:20]\n[custom:1]\n[00:01.00]本文");
        assert_eq!(texts(&q), vec!["本文"]);
        assert_eq!(q.meta[0].0, "ti");
    }

    #[test]
    fn out_of_order_is_sorted_with_warning() {
        let p = parse("[00:10.00]後\n[00:05.00]先");
        assert_eq!(texts(&p), vec!["先", "後"]);
        assert_eq!(times(&p), vec![5_000, 10_000]);
        assert!(p.warnings.contains(&warn::REORDERED.to_string()));
    }

    #[test]
    fn empty_timed_line_clears() {
        let p = parse("[00:01.00]一行目\n\n[00:02.00]\n[00:03.00]三行目");
        assert_eq!(times(&p), vec![1_000, 2_000, 3_000]);
        let kinds: Vec<LineKind> = p.lines.iter().map(|l| l.kind).collect();
        assert_eq!(kinds, vec![LineKind::Lyric, LineKind::Clear, LineKind::Lyric]);
        assert_eq!(texts(&p), vec!["一行目", "", "三行目"]);
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        assert_eq!(p.plain_text, "一行目\n三行目");
    }

    #[test]
    fn word_times_are_removed_from_display_only() {
        let raw = "[00:07.25]<00:07.25>小さな <00:07.70>光が <00:08.10>揺れる";
        let p = parse(raw);
        assert_eq!(times(&p), vec![7_250]);
        assert_eq!(texts(&p), vec!["小さな 光が 揺れる"]);
        // 原文はそのまま（parse は原文を変えない。正規化しても語ごとの時刻は残る）
        assert_eq!(normalize(raw), raw);
    }

    #[test]
    fn crlf_and_cr_give_the_same_result() {
        let lf = parse("[00:01.00]一\n[00:02.00]二\n\n三");
        let crlf = parse("[00:01.00]一\r\n[00:02.00]二\r\n\r\n三");
        let cr = parse("[00:01.00]一\r[00:02.00]二\r\r三");
        assert_eq!(lf, crlf);
        assert_eq!(lf, cr);
        assert_eq!(normalize("a\r\nb\rc"), "a\nb\nc");
    }

    #[test]
    fn utf8_bom_is_removed() {
        let p = parse("\u{feff}[ti:題]\n[00:01.00]始まり");
        assert_eq!(p.meta[0].0, "ti");
        assert_eq!(texts(&p), vec!["始まり"]);
        assert!(!normalize("\u{feff}abc").starts_with('\u{feff}'));
    }

    #[test]
    fn invalid_timestamps_are_not_used() {
        let p = parse("[00:61.00]秒が範囲外\n[00:10.1234]小数が長い");
        assert_eq!(p.format, LyricsFormat::Plain);
        assert!(p.lines.iter().all(|l| l.time_ms.is_none()));
        assert_eq!(texts(&p), vec!["秒が範囲外", "小数が長い"]);
        assert!(p.warnings.contains(&warn::TIMESTAMP_INVALID.to_string()));
        // ほかに有効な時刻があれば、使えない時刻の行は「時刻なしの行」として末尾へ
        let q = parse("[00:61.00]秒が範囲外\n[00:01.00]正しい");
        assert_eq!(q.format, LyricsFormat::Timed);
        assert_eq!(texts(&q), vec!["正しい", "秒が範囲外"]);
        assert_eq!(q.lines[1].kind, LineKind::Untimed);
        assert!(q.warnings.contains(&warn::TIMESTAMP_INVALID.to_string()));
    }

    #[test]
    fn long_minutes() {
        let p = parse("[123:45.678]長い曲");
        assert_eq!(times(&p), vec![7_425_678]);
    }

    #[test]
    fn offset_below_zero_is_clamped() {
        // 仕様 4.2 の式 max(0, 元 − offset + 補正) の「元 − offset」までを返す。0 未満でも切らない
        // （0 にそろえるのは画面で補正を足したあと。src/stores/lyricsTime.ts と scripts/lyrics-time.test.mts）
        let p = parse("[offset:+500]\n[00:00.20]先頭");
        assert_eq!(times(&p), vec![-300]);
        assert!(p.warnings.contains(&warn::OFFSET_CLAMPED.to_string()));
    }

    /// 閉じない `<` が 1MiB 並んでも、すぐ終わる（以前は 21秒）
    #[test]
    fn unclosed_angle_brackets_are_linear() {
        let line = "<".repeat(MAX_TEXT_BYTES);
        let t = std::time::Instant::now();
        let p = parse(&line);
        assert!(t.elapsed() < std::time::Duration::from_secs(2), "{:?}", t.elapsed());
        assert_eq!(p.lines.len(), 1);
        assert_eq!(p.lines[0].text.len(), MAX_TEXT_BYTES);
        // 時刻の後ろにだけ外す。外れない `<` は残す
        assert_eq!(strip_word_times("<<00:01.00>a<b>c<12>d<00:02"), "<a<b>cd<00:02");
        assert_eq!(strip_word_times("あ<00:01.50>い"), "あい");
        // 閉じない `<` と、遠くの `>` の組み合わせも線形
        let mixed = format!("{}>", "<".repeat(200_000));
        let t = std::time::Instant::now();
        assert_eq!(strip_word_times(&mixed), mixed);
        assert!(t.elapsed() < std::time::Duration::from_secs(2));
    }

    /// 1行に時刻が多数＋長い本文でも、複製を上限で止める
    #[test]
    fn many_times_with_long_text_are_bounded() {
        // 指摘の再現（[00:01.00] ＋ [00:02]×20000 ＋ 本文 30万字。約 1.04MB で 1MiB 以内）
        let raw = format!("[00:01.00]{}{}", "[00:02]".repeat(20_000), "a".repeat(300_000));
        assert!(raw.len() <= MAX_TEXT_BYTES);
        let t = std::time::Instant::now();
        let p = parse(&raw);
        assert!(t.elapsed() < std::time::Duration::from_secs(2), "{:?}", t.elapsed());
        let total: usize = p.lines.iter().map(|l| l.text.len()).sum();
        assert!(total <= 300_000 + MAX_DUPLICATED_BYTES, "{total}");
        assert!(p.lines.len() <= MAX_TIMES_PER_LINE);
        assert_eq!(p.lines[0].time_ms, Some(1_000));
        assert!(p.warnings.contains(&warn::TIMESTAMPS_TRUNCATED.to_string()));
        // 短い本文でも、1行の時刻は 100 個まで
        let q = parse(&format!("{}x", "[00:01]".repeat(150)));
        assert_eq!(q.lines.len(), MAX_TIMES_PER_LINE);
        assert!(q.warnings.contains(&warn::TIMESTAMPS_TRUNCATED.to_string()));
        // 普通の繰り返しは警告しない
        assert!(!parse("[00:01][00:02][00:03]x").warnings.contains(&warn::TIMESTAMPS_TRUNCATED.to_string()));
    }

    /// offset の最小値・最大値で桁あふれしない（debug ビルドのテストで panic しない）
    #[test]
    fn extreme_offsets_do_not_overflow() {
        let p = parse("[offset:-9223372036854775808]\n[00:01.00]a");
        assert_eq!(p.offset_ms, i64::MIN);
        assert_eq!(times(&p), vec![i64::MAX]);
        assert!(!p.warnings.contains(&warn::OFFSET_CLAMPED.to_string()));
        let q = parse("[offset:9223372036854775807]\n[00:01.00]a");
        assert_eq!(times(&q), vec![1_000 - i64::MAX]);
        assert!(q.warnings.contains(&warn::OFFSET_CLAMPED.to_string()));
    }

    #[test]
    fn single_digit_minute() {
        let p = parse("[1:02.5]一桁の分");
        assert_eq!(times(&p), vec![62_500]);
    }

    #[test]
    fn plain_text_keeps_blank_lines() {
        let raw = "一番の歌詞\n二行目\n\n二番の歌詞\n\n\n終わり\n";
        let p = parse(raw);
        assert_eq!(p.format, LyricsFormat::Plain);
        assert_eq!(texts(&p), vec!["一番の歌詞", "二行目", "", "二番の歌詞", "", "", "終わり"]);
        assert_eq!(p.plain_text, "一番の歌詞\n二行目\n\n二番の歌詞\n\n\n終わり");
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn untimed_lines_inside_timed_go_last() {
        let p = parse("[00:01.00]一\n時刻の無い行\n[00:02.00]二");
        assert_eq!(texts(&p), vec!["一", "二", "時刻の無い行"]);
        assert_eq!(p.lines[2].kind, LineKind::Untimed);
        assert_eq!(p.lines[2].time_ms, None);
        assert!(p.warnings.contains(&warn::UNTIMED_LINES.to_string()));
    }

    #[test]
    fn first_valid_offset_wins_and_negative_delays() {
        let p = parse("[offset:abc]\n[offset:-300]\n[offset:+900]\n[00:01.00]一");
        assert_eq!(p.offset_ms, -300);
        assert_eq!(times(&p), vec![1_300]);
    }

    #[test]
    fn repeated_chorus_on_one_line_is_not_a_reorder() {
        let p = parse("[00:10.00][00:40.00]サビ\n[00:20.00]二番");
        assert_eq!(texts(&p), vec!["サビ", "二番", "サビ"]);
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    }

    #[test]
    fn empty_and_info_only_have_no_content() {
        assert!(!parse("").has_content());
        assert!(!parse("  \n\t\n").has_content());
        assert!(!parse("[ti:題]\n[ar:人]").has_content());
        assert!(!parse("[00:01.00]\n[00:02.00]").has_content());
        assert!(parse("一行").has_content());
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の10・作者指示）：行は 5,000 行まで。超えた分は捨てて警告
    #[test]
    fn too_many_lines_are_cut_with_warning() {
        let timed: String = (0..6000).map(|i| format!("[{:02}:{:02}.00]行{i}\n", i / 60, i % 60)).collect();
        let p = parse(&timed);
        assert_eq!(p.lines.len(), MAX_LINES);
        assert!(p.warnings.iter().any(|w| w == warn::LINES_TRUNCATED));
        assert_eq!(p.lines[0].text, "行0");
        assert_eq!(p.lines.last().unwrap().text, "行4999");
        assert_eq!(p.plain_text.lines().count(), MAX_LINES);
        let plain: String = (0..6000).map(|i| format!("行{i}\n")).collect();
        let p = parse(&plain);
        assert_eq!(p.lines.len(), MAX_LINES);
        assert!(p.warnings.iter().any(|w| w == warn::LINES_TRUNCATED));
        // ちょうど 5,000 行なら警告しない
        let exact: String = (0..MAX_LINES).map(|i| format!("行{i}\n")).collect();
        let p = parse(&exact);
        assert_eq!(p.lines.len(), MAX_LINES);
        assert!(p.warnings.is_empty());
        // 1MiB 近い原文（1行 1 文字。約 50万行）でも 5,000 行で止まる
        let many = "a\n".repeat(MAX_TEXT_BYTES / 2 - 1);
        assert_eq!(parse(&many).lines.len(), MAX_LINES);
    }
}
