//! 古い文字コードで書かれたタグの文字化けを、読むときに直す（v3.1.2）。曲ファイルは書き換えない。
//!
//! ID3v1 や ID3v2 の Latin-1 の枠に Shift_JIS（CP932）のバイトが入っていると、lofty は1バイトを1文字（U+0000〜U+00FF）として
//! 読むので、「‚ ‚¢‚¤」のように化ける。この形を見分け、Latin-1 のバイトに戻して日本語として読み直す。
//!
//! 見分けの決まり（作者の手元のライブラリで当てて、誤りが無かったもの）：
//! 1. 値が U+0000〜U+00FF の文字だけでできている
//! 2. U+0080〜U+009F（C1 制御文字）を含む、または U+0080〜U+00FF の文字が3つ以上ある
//! 3. Latin-1 のバイトに戻して CP932 として読める（末尾の2バイト文字が切れて 1〜2 バイト足りないときは、そこを落として読む）
//! 4. 読んだ結果に私用領域の文字・置換文字（U+FFFD）が無く、半角カナが2つ以上混ざらない
//!
//! 実装で足した守り（どれも「直さない」側にだけ倒す。v3.1.2 で決めた。根拠は各関数の説明）：
//! - 読んだ結果に制御文字が無いこと、ASCII 以外の文字が1つはあること（`plausible`）
//! - 読んだ並びが日本語らしいこと（`looks_japanese`）。かな・全角英数が1つでもあれば日本語。
//!   無ければ（漢字と英数字だけの値）、ほかの1バイトの文字コード（ロシア語の CP1251・ギリシャ語の CP1253・東欧の CP1250・
//!   西欧の CP1252）や韓国語（CP949）の化けと見分けるため、漢字の並び方を見る。
//!   決まり 1〜4 だけでは「Небо」「Αθήνα」「Škoda Čaj」「사랑」「Démon Épée」「Don’t Stop」「80’s」を漢字まじりに変えてしまう（テストで確かめた）
//! - UTF-8 のバイトを Latin-1 で読んだ形は Shift_JIS として読まない（`utf8_mojibake`）
//!
//! Shift_JIS で読めないときだけ、EUC-JP・GBK・Big5 も試す。誤りやすいので、読んだ結果がファイル名
//! （拡張子と先頭の曲順の数字を除いたもの、または拡張子だけを除いたもの）と同じときだけ採る。
//!
//! 変換は Windows の MultiByteToWideChar（`platform::windows::codepage`）。

use std::path::Path;

use super::tags_read::TagValues;
use crate::platform::windows::codepage::{self, CP_BIG5, CP_EUC_JP, CP_GBK, CP_SHIFT_JIS};

/// これより長い値は見ない（タグの1項目としてはありえない長さ。巨大な値で時間とメモリを使わない）
pub const MAX_CHARS: usize = codepage::MAX_BYTES;

/// 値を直す。直したら Some（直した値）、直さないなら None。
/// `file_stem` はファイル名から拡張子を除いたもの（EUC-JP・GBK・Big5 の照合に使う。無ければこの3つは試さない）
pub fn repair(value: &str, file_stem: Option<&str>) -> Option<String> {
    let bytes = latin1_bytes(value)?;
    let high = bytes.iter().filter(|&&b| b >= 0x80).count();
    let c1 = bytes.iter().any(|&b| (0x80..=0x9F).contains(&b));
    if !(c1 || high >= 3) {
        return None;
    }
    if !utf8_mojibake(&bytes) {
        if let Some((s, read)) = decode_allow_cut(CP_SHIFT_JIS, &bytes) {
            if plausible(&s) && half_width_kana(&s) < 2 && looks_japanese(&bytes, read) {
                return finish(&s);
            }
        }
    }
    let stem = file_stem?;
    for cp in [CP_EUC_JP, CP_GBK, CP_BIG5] {
        if let Some((s, _)) = decode_allow_cut(cp, &bytes) {
            if plausible(&s) && matches_file_name(s.trim(), stem) {
                return finish(&s);
            }
        }
    }
    None
}

/// タグの値をまとめて直す（曲名・アーティスト・アルバム・アルバムアーティスト・ジャンル・作曲者・コメント）。
/// 直した項目があれば true
pub fn repair_values(v: &mut TagValues, path: &Path) -> bool {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
    let stem = stem.as_deref();
    let mut changed = false;
    for field in [
        &mut v.title,
        &mut v.artist,
        &mut v.album,
        &mut v.album_artist,
        &mut v.genre,
        &mut v.composer,
        &mut v.comment,
    ] {
        if let Some(fixed) = repair(field, stem) {
            *field = fixed;
            changed = true;
        }
    }
    changed
}

/// 全部の文字が U+0000〜U+00FF なら、Latin-1 のバイトに戻したもの。空・長すぎ・それ以外の文字があれば None
fn latin1_bytes(value: &str) -> Option<Vec<u8>> {
    // UTF-8 で U+00FF 以下の文字は 2 バイト以下なので、バイト数で先に長すぎるものを外す
    if value.is_empty() || value.len() > MAX_CHARS * 2 {
        return None;
    }
    let mut out = Vec::with_capacity(value.len());
    for c in value.chars() {
        let n = c as u32;
        if n > 0xFF {
            return None;
        }
        out.push(n as u8);
    }
    if out.len() > MAX_CHARS {
        return None;
    }
    Some(out)
}

/// 厳密に読む。読めなければ、末尾の 1〜2 バイト（0x80 以上のものだけ。2バイト文字が途中で切れた形）を落として読む。
/// 読んだ文字列と、読んだバイト数（落とさなければ `bytes.len()`）を返す
fn decode_allow_cut(cp: u32, bytes: &[u8]) -> Option<(String, usize)> {
    if let Some(s) = codepage::decode_strict(cp, bytes) {
        return Some((s, bytes.len()));
    }
    for cut in 1..=2usize {
        if bytes.len() <= cut || bytes[bytes.len() - cut..].iter().any(|&b| b < 0x80) {
            return None;
        }
        let read = bytes.len() - cut;
        if let Some(s) = codepage::decode_strict(cp, &bytes[..read]) {
            return Some((s, read));
        }
    }
    None
}

/// 読んだ結果として使えるか：私用領域・置換文字・制御文字が無く、ASCII 以外の文字が1つはある。
/// 制御文字：CP932 は 0x80 を U+0080 に読むなど、日本語にならなかったバイトが制御文字として残る（守りとして足した）。
/// ASCII 以外が無い：末尾を落とした結果が英数字だけになった形（例「Hello」＋切れた 1 バイト）。日本語を読み出せていない
fn plausible(s: &str) -> bool {
    let mut non_ascii = false;
    for c in s.chars() {
        let n = c as u32;
        if (0xE000..=0xF8FF).contains(&n) || n >= 0xF0000 || c == '\u{FFFD}' || c.is_control() {
            return false;
        }
        if n >= 0x80 {
            non_ascii = true;
        }
    }
    non_ascii
}

/// 半角カナ（U+FF61〜U+FF9F）の数。2つ以上なら Shift_JIS 以外の化け方（UTF-8 や EUC-JP を Shift_JIS で読んだ形など）とみなす
fn half_width_kana(s: &str) -> usize {
    s.chars().filter(|c| ('\u{FF61}'..='\u{FF9F}').contains(c)).count()
}

/// Shift_JIS で読んだ1文字ぶんのバイトの種類（`looks_japanese` で使う）
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// ASCII（1バイト）。英字かどうかは別に見る
    Ascii(u8),
    /// かな・全角英数（1バイト目 0x82、または 0x83 で2バイト目が 0x96 まで）
    Kana,
    /// 漢字のうち、ほかの文字コードの化けでは出にくいもの（下の `Unsure` 以外の漢字）。1バイト目と2バイト目を持つ
    Kanji(u8, u8),
    /// 1バイト目が 0xE0 以上で、2バイト目も 0x80 以上の漢字（第二水準・拡張の漢字）。
    /// ロシア語・ギリシャ語の小文字や韓国語の2バイトが続くと、この形になる。日本語にもあるが少ない
    Unsure(u8, u8),
    /// 半角カナ（0xA1〜0xDF の1バイト）、全角のギリシャ文字・キリル文字（0x83 の後半・0x84）など、日本語の曲名にはまず出ないもの。
    /// ロシア語・ギリシャ語の大文字、韓国語の1バイト目がこれになる
    Foreign,
    /// 記号（0x81・0x87）。どちらにも数えない
    Symbol,
}

/// 読んだバイト（CP932 として厳密に読めたもの）を1文字ずつに分けて種類を付ける。
/// CP932 の2バイト文字の1バイト目は 0x81〜0x9F・0xE0〜0xFC。読めたあとなので、1バイト目の次には必ず2バイト目がある
fn sjis_kinds(bytes: &[u8]) -> Vec<Kind> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let lead = (0x81..=0x9F).contains(&b) || (0xE0..=0xFC).contains(&b);
        if !lead || i + 1 >= bytes.len() {
            out.push(if b < 0x80 { Kind::Ascii(b) } else { Kind::Foreign });
            i += 1;
            continue;
        }
        let t = bytes[i + 1];
        out.push(match b {
            0x82 => Kind::Kana,
            0x83 if t <= 0x96 => Kind::Kana,
            0x83 | 0x84 => Kind::Foreign,
            0x81 | 0x87 => Kind::Symbol,
            _ if b >= 0xE0 && t >= 0x80 => Kind::Unsure(b, t),
            _ => Kind::Kanji(b, t),
        });
        i += 2;
    }
    out
}

/// Shift_JIS として読んだ並びが日本語らしいか（v3.1.2 の検査で足した。どれも「直さない」側にだけ倒す）。
/// `all` は値の全部のバイト、`read` は読めたバイト数（末尾の切れた 1〜2 バイトを落としたときは `all.len()` より短い）。
///
/// なぜ要るか：ほかの文字コードの文字も、CP932 の2バイト文字として読めてしまう。決まり 1〜4 だけでは
/// ロシア語（CP1251）「Небо」→「ﾍ裔」、ギリシャ語（CP1253）「Αθήνα」→「ﾁ鞐涖」、東欧（CP1250）「Škoda Čaj」→「殻oda ﾈaj」、
/// 韓国語（CP949）「사랑」→「ｻ邯」、西欧（CP1252）「Démon」→「D駑on」・「“A”」→「鄭」になる（テストで確かめた）。
///
/// 見分け方（バイトの並びで見る。上から順に）：
/// 1. かな・全角英数（1バイト目 0x82・0x83）が1つでもあれば日本語。ほかの文字コードの文字の並びは、まずこの形にならない
///    （ロシア語・ギリシャ語の文字は 0xC0 以上、韓国語は 0xA1 以上、西欧のアクセント付きの文字は 0xC0 以上）
/// 2. かなが無い（漢字と英数字だけ）とき、`Kanji` が `Unsure` と `Foreign` を合わせた数より多いこと。
///    ロシア語・ギリシャ語・韓国語の化けは `Unsure` と `Foreign`（半角カナ）ばかりになり、`Kanji` は引用符の “ などからしか出ない
/// 3. 値の全体が “…” か ‘…’（Windows-1252 の 0x93…0x94・0x91…0x92）で囲まれ、閉じの ” ’ が西欧などの文字の並びの終わりに
///    見えるときは直さない（「“Hello”」「“Säg”」「“Да”」。見分け方は `quote_closes_western`）。
///    v3.1.2 の検査で絞った。前は先頭と末尾のバイトだけを見ていて、東・南・内・投・登・得・読…（0x93）で始まり
///    部・数・株・券・灯…（2バイト目が 0x94）で終わる日本語（「東部」「投票数」「特別券」）も直らなかった
/// 4. 漢字が ASCII の英字と隣り合い、その漢字のバイトが西欧・東欧の文字の並びに見えるときは直さない。見るのは次の3つ
///    （v3.1.2 の検査で絞った。前は「漢字の隣に英字があれば直さない」で、「A面」「TV版」「東京DOME」も直らなかった）：
///    - 2バイト目が ASCII の漢字が、前か後ろの英字と隣り合う。アクセント付きの文字＋英字の形（「D駑on」「殻oda」「Don譁 Stop」）
///    - 2バイト目が西欧・東欧の文字のバイト（`western_letter`）の漢字の、すぐ後ろに英字がある。
///      引用符・ダッシュ・アポストロフィ＋大文字のアクセント付きの文字で単語が始まる形（「L’été」「—Élan」「L’Œuvre」「—Źle」）
///    - 1バイト目が西欧・東欧の文字のバイト（`western_letter_lead`）の漢字の、すぐ前に英字がある。
///      単語の終わりにアクセント付きの文字が2つ続く形（ポーランド語「pieśń」「cześć」）
///    日本語で英字の後ろに漢字が来る形（「A面」「TV版」「Love夜」）は、漢字の1バイト目がふつうは記号のバイトなので直る。
///    英字の前に漢字が来る形（「東京DOME」）は、漢字の2バイト目が文字のバイトでなければ直る。
///    直らない日本語：「DJ吉田」（吉 8B 67 は2バイト目が ASCII）、「A面B面」（面 96 CA の後ろに B）、「A館」（館 8A D9 の1バイト目が Š）
/// 5. 末尾を落として読んだのに ASCII 以外が1文字しかなければ直さない（「“A”」→「鄭」＋落とした ”）。
///    引用符で囲んだ1文字と見分けられないため（ID3v1 で切れた日本語の値でも、漢字が1文字だけ残る形は直らない）
/// 6. 漢字がちょうど1つで、それが 0x91・0x92（‘ ’）＋ASCII の小文字 s・t・d・m のとき直さない（v3.1.2 の検査で足した）。
///    英語の「’s」「’t」「’d」「’m」が数字・空白・記号の隣に来た形（「80’s」→「80痴」、「’t」→「稚」）。
///    この形の漢字は 壮・奏・租・僧・痴・稚・壇・知 の8つで、これがかなも別の漢字も無い値にただ1つある日本語（「奏」「LIVE 知」）は直らない
///
/// 直らない日本語（まれ。どれも直さない側）：上の 4・6 に書いたもの、かなが無く第二水準の漢字が半分以上の値（「髙橋」）、
/// “ ‘ と同じバイトの漢字で始まり 株・券・諸・飼 など（1バイト目が Š・Œ と同じ）で終わり、2バイト目が英字の漢字（新 90 56 など）を含む値（「東新株」）。
/// 2バイトだけの値（漢字1文字）は、上の8つのほかは直す（「池」92 72）
///
/// 決まり 3 の絞り方で増える誤りは、決まり 4 と同じやり方の約46万通り（引用符で囲んだ西欧・東欧・ロシア語・ギリシャ語・
/// 韓国語の単語と2語の組み合わせ）で 0、大文字の Š・Ž・Ś・Ћ などで終わる語を前置きと組み合わせた約7千通りで、
/// 同じ大文字を2つ並べただけの形（「“ŠŠ”」「“ЏЏ”」「“ОЏ”」）の 12。試した案のうち、大文字の Š・Œ などと組になる閉じを
/// いつも日本語とみなす案は「“MÁŠ”」「“A LUKÁŠ”」「“UŽ”」「“НОЋ”」など 108 を漢字に変え、いつも西欧とみなす案は「特別券」が直らず、
/// 中身に英字があるときだけ囲みを見る案は「登録数」（登 93 6F の 6F が o）が直らず「“Дом’Я”」などを漢字に変えたので採らなかった
///
/// 決まり 4 の絞り方で増える誤り（西欧・東欧・ロシア語・ギリシャ語・韓国語の単語を引用符・ダッシュ・英字と組み合わせた約15万通りで数えた）：
/// ダッシュ・中黒・アポストロフィのすぐ後ろに、大文字のアクセント付きの文字が1文字だけで来る形（「Love•É」「l’Ô」）、
/// 「Love–Řád」のようにダッシュの後ろにアクセント付きの文字が2つ続く形、英字の後ろのダッシュ2つ（「Love——」）。
/// 実在しそうな単語の並び（「L’été」「—Élan」「pieśń」など）では増えなかった。
/// 試した中で、案そのまま（2バイト目が ASCII の漢字だけを見る）は「L’été」「L’Île」「—Élan」を漢字に変えたので採らなかった
fn looks_japanese(all: &[u8], read: usize) -> bool {
    let kinds = sjis_kinds(&all[..read]);
    let count = |k: Kind| kinds.iter().filter(|&&x| x == k).count();
    // 1
    if count(Kind::Kana) > 0 {
        return true;
    }
    // 2
    let kanji = kinds.iter().filter(|k| matches!(k, Kind::Kanji(..))).count();
    let unsure = kinds.iter().filter(|k| matches!(k, Kind::Unsure(..))).count();
    if kanji == 0 || unsure + count(Kind::Foreign) >= kanji {
        return false;
    }
    // 3
    if all.len() >= 3
        && matches!((all[0], all[all.len() - 1]), (0x93, 0x94) | (0x91, 0x92))
        && quote_closes_western(&kinds, read == all.len())
    {
        return false;
    }
    // 4
    let is_letter = |k: Option<&Kind>| matches!(k, Some(Kind::Ascii(b)) if b.is_ascii_alphabetic());
    for (i, k) in kinds.iter().enumerate() {
        let before = is_letter(i.checked_sub(1).and_then(|j| kinds.get(j)));
        let after = is_letter(kinds.get(i + 1));
        let stop = match *k {
            Kind::Kanji(_, t) if t < 0x80 => before || after,
            Kind::Kanji(l, t) | Kind::Unsure(l, t) => {
                (after && western_letter(t)) || (before && western_letter_lead(l))
            }
            _ => false,
        };
        if stop {
            return false;
        }
    }
    // 5
    if read < all.len() && kinds.iter().filter(|k| !matches!(k, Kind::Ascii(_))).count() == 1 {
        return false;
    }
    // 6
    if kanji == 1
        && kinds.iter().any(|k| matches!(k, Kind::Kanji(0x91 | 0x92, b's' | b't' | b'd' | b'm')))
    {
        return false;
    }
    true
}

/// 決まり 3 で使う：値の全体を囲む “…” ‘…’ の閉じ（0x94・0x92）が、西欧などの文字の並びの終わりに見えるか（v3.1.2 の検査で足した）。
/// `whole` は値を全部読めたか（末尾を落とさずに読めたか）。
///
/// 引用符で囲んだ西欧などの値では、閉じは組にならずに残る（前が英字・数字・記号なので、読むときに落とす）か、
/// 前のアクセント付きの文字・キリル文字・ギリシャ文字のバイトと組になる。日本語の「東部」（93 8C 95 94）では、
/// 閉じに見えるバイトは「部」（95 94）の2バイト目で、組になる1バイト目は第一水準の漢字のバイト（0x88〜0x98）。ここで見分ける。
/// - 閉じが組にならずに残った：西欧など（「“Hello”」「“Säg”」「“Дом”」）
/// - 組になる1バイト目が 0xE0 以上か、小文字の š・ś・ť・ž・ź（0x9A・0x9C〜0x9F）：西欧など（「“Да”」「“Lukáš”」）
/// - 組になる1バイト目が大文字の Š・Œ・Ś・Ť・Ž・Ź（0x8A・0x8C〜0x8F。CP1251 では Љ・Њ・Ќ・Ћ・Џ）：
///   日本語の「株」「券」「諸」「飼」の1バイト目と同じなので、ほかにも西欧などの文字の並びに見える文字（半角カナ・全角のギリシャ文字や
///   キリル文字・第二水準の漢字・2バイト目が英字の漢字）があるときだけ西欧などとみなす（「“UŽ”」「“MÁŠ”」「“НОЋ”」は直さず、「特別券」は直す）
/// - 最後の文字が漢字でない（記号など）：西欧など（直さない側に倒す）
fn quote_closes_western(kinds: &[Kind], whole: bool) -> bool {
    if !whole {
        return true;
    }
    let lead = match kinds.last() {
        Some(Kind::Kanji(l, _) | Kind::Unsure(l, _)) => *l,
        _ => return true,
    };
    if lead >= 0xE0 || matches!(lead, 0x9A | 0x9C..=0x9F) {
        return true;
    }
    matches!(lead, 0x8A | 0x8C..=0x8F)
        && kinds.iter().any(|k| {
            matches!(k, Kind::Foreign | Kind::Unsure(..))
                || matches!(k, Kind::Kanji(_, t) if t.is_ascii_alphabetic())
        })
}

/// 2バイト目が、Windows-1250・1252 で文字として読めるバイトか（0xA0 以上と、Š・Œ・Ś・Ť・Ž・Ź の大文字・小文字）。
/// ž の 0x9E だけは除く：「京」（8B 9E）が英字の前に来る形（「東京DOME」）を直すため。ž が引用符・ダッシュのすぐ後ろで単語を始めることはまず無い
fn western_letter(t: u8) -> bool {
    t >= 0xA0 || matches!(t, 0x8A | 0x8C..=0x8F | 0x9A | 0x9C | 0x9D | 0x9F)
}

/// 1バイト目が、Windows-1250・1252 でアクセント付きの文字として読めるバイトか（0xE0 以上と、Š・Œ・Ś・Ť・Ž・Ź の大文字・小文字）。
/// CP932 の1バイト目は 0x81〜0x9F・0xE0〜0xFC なので、0xC0〜0xDF の大文字はここに来ない
fn western_letter_lead(l: u8) -> bool {
    l >= 0xE0 || matches!(l, 0x8A | 0x8C..=0x8F | 0x9A | 0x9C..=0x9F)
}

/// UTF-8 のバイトを Latin-1 で読んだ形（「さくら」を UTF-8 にしたバイト E3 81 95 … を1バイト1文字で読んだもの）か。
/// これを CP932 で読むと「縺輔￥繧」のように半角カナを含まない漢字の列になり、決まり 4 だけでは弾けない（テストで確かめた）。
/// 日本語の Shift_JIS は、かな（先頭バイト 0x82・0x83）や多くの漢字（0x88〜0x9F）が UTF-8 の続きのバイトと同じ範囲で始まるので、
/// ASCII 以外を含んで UTF-8 として正しく読めることは、まず無い。だから UTF-8 として読めるものは Shift_JIS として読まない（守りとして足した）
fn utf8_mojibake(bytes: &[u8]) -> bool {
    bytes.iter().any(|&b| b >= 0x80) && std::str::from_utf8(bytes).is_ok()
}

/// 前後の空白を外す。空になったら直さない
fn finish(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// 読んだ結果がファイル名と同じか。ファイル名は拡張子を除いたもの（`file_stem`）。
/// 先頭の曲順の数字（`01 `・`01. `・`01 - `・`1-01 `・`01_` など）を除いたものとも比べる
fn matches_file_name(decoded: &str, stem: &str) -> bool {
    if decoded.is_empty() {
        return false;
    }
    let stem = stem.trim();
    decoded == stem || strip_track_number(stem).is_some_and(|rest| decoded == rest.trim())
}

/// 先頭の曲順の数字と区切りを除いた残り。曲順の形でなければ None
fn strip_track_number(stem: &str) -> Option<&str> {
    let b = stem.as_bytes();
    let digits = |from: usize| b[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    let is_sep = |c: u8| matches!(c, b' ' | b'.' | b'-' | b'_' | b')');
    let mut i = digits(0);
    if i == 0 || i > 4 {
        return None;
    }
    // ディスク番号つき（1-01・1.01）
    if i < b.len() && (b[i] == b'-' || b[i] == b'.') {
        let d = digits(i + 1);
        if (1..=3).contains(&d) && i + 1 + d < b.len() && is_sep(b[i + 1 + d]) {
            i += 1 + d;
        }
    }
    let start = i;
    while i < b.len() && is_sep(b[i]) {
        i += 1;
    }
    // 区切りが無いときは、すぐ後が ASCII 以外（「01あいう」）のときだけ曲順とみなす（「1999」「3D」は曲名の一部）
    if i == start && (i >= b.len() || b[i].is_ascii()) {
        return None;
    }
    // ここまでは ASCII だけなので、i は文字の境目
    let rest = &stem[i..];
    if rest.trim().is_empty() {
        None
    } else {
        Some(rest)
    }
}

/// テスト用：古い文字コードのバイト列を、lofty と同じく1バイト1文字（Latin-1）で読んだ「化けた形」にする
#[cfg(test)]
pub(crate) fn test_mojibake(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 架空の曲名・名前だけを使う。バイトは各文字コードの表から手で引いたもの（下の `roundtrip` で読み戻せることも確かめる）
    /// 「さくら色の坂道」（Shift_JIS）
    const SJIS_SAKAMICHI: &[u8] = b"\x82\xb3\x82\xad\x82\xe7\x90\x46\x82\xcc\x8d\xe2\x93\xb9";
    /// 「アオイ」（Shift_JIS。カタカナは2バイト目が ASCII の範囲。0x80 以上のバイトは 0x83 の3つだけ）
    const SJIS_AOI: &[u8] = b"\x83\x41\x83\x49\x83\x43";
    /// 「夜」（Shift_JIS 1文字。0x80 以上は2つだけで、C1（0x96）で当たる形）
    const SJIS_YORU: &[u8] = b"\x96\xe9";
    /// 「ＡＢＣ」（Shift_JIS の全角英字。0x80 以上は 0x82 の3つだけ）
    const SJIS_FULLWIDTH: &[u8] = b"\x82\x60\x82\x61\x82\x62";

    fn sjis(bytes: &[u8]) -> String {
        codepage::decode_strict(CP_SHIFT_JIS, bytes).unwrap()
    }

    #[test]
    fn roundtrip_of_test_bytes() {
        assert_eq!(sjis(SJIS_SAKAMICHI), "さくら色の坂道");
        assert_eq!(sjis(SJIS_AOI), "アオイ");
        assert_eq!(sjis(SJIS_YORU), "夜");
        assert_eq!(sjis(SJIS_FULLWIDTH), "ＡＢＣ");
    }

    #[test]
    fn shift_jis_mojibake_is_repaired() {
        // C1 を含み、0x80 以上が3つ以上
        let m = test_mojibake(SJIS_SAKAMICHI);
        assert!(m.chars().all(|c| (c as u32) <= 0xFF));
        assert_eq!(repair(&m, None).as_deref(), Some("さくら色の坂道"));
        // 0x80 以上が3つ（C1 は 0x83 なので C1 でも当たる）
        assert_eq!(repair(&test_mojibake(SJIS_AOI), None).as_deref(), Some("アオイ"));
        // 1文字だけ（C1 を含む）
        assert_eq!(repair(&test_mojibake(SJIS_YORU), None).as_deref(), Some("夜"));
        assert_eq!(repair(&test_mojibake(SJIS_FULLWIDTH), None).as_deref(), Some("ＡＢＣ"));
        // 2バイト目が ASCII の範囲の漢字だけの短い値（西欧の言葉らしい形と同じバイトの並び）：「本」「吉田」「池田」
        assert_eq!(repair(&test_mojibake(b"\x96\x7b"), None).as_deref(), Some("本"));
        assert_eq!(repair(&test_mojibake(b"\x8b\x67\x93\x63"), None).as_deref(), Some("吉田"));
        assert_eq!(repair(&test_mojibake(b"\x92\x72\x93\x63"), None).as_deref(), Some("池田"));
        // 英数字と混ざっていても読む
        let mixed = [b"Track ".as_slice(), SJIS_SAKAMICHI, b" (Live)"].concat();
        assert_eq!(repair(&test_mojibake(&mixed), None).as_deref(), Some("Track さくら色の坂道 (Live)"));
    }

    #[test]
    fn cut_tail_is_dropped() {
        // ID3v1 の 30 バイトで2バイト文字が切れた形：先頭バイトだけが末尾に残る
        let mut cut = SJIS_SAKAMICHI.to_vec();
        cut.push(0x82);
        assert_eq!(repair(&test_mojibake(&cut), None).as_deref(), Some("さくら色の坂道"));
        // 末尾が ASCII なら落とさない（読めないものは直さない）
        let mut bad = SJIS_SAKAMICHI.to_vec();
        bad.extend_from_slice(b"\x82 ");
        assert_eq!(repair(&test_mojibake(&bad), None), None);
    }

    #[test]
    fn real_latin1_and_english_are_kept() {
        for v in [
            "Raison d'être",
            "Raison d’être",
            "A × B",
            "Café × Crème",
            "Ñandú Brûlé Ça",
            "Grüne Möwe",
            "Rós og ást - Ágæt saga",
            "Björkin góða við fjörðinn",
            "ÀÉÎÕÜ ÀÉÎÕÜ",
            "Hello World",
            "Don\u{92}t Stop",
            "\u{93}Hello\u{94}",
            "Rock \u{96} Roll\u{85}",
            "Hello\u{85}",
            "Café élan",
            // 決まり 1〜4 だけでは漢字に変わってしまう西欧の名前（looks_japanese で止める）
            "Démon Épée",
            "Lumière éphémère",
            "Ça va déjà",
            "Hélène\u{92}s Song",
            "Mélodie lointaine",
            "Des rêves misérables",
            "Écoute la pluie",
            "Façon française",
            "Señorita éxito",
            "Pièce à vivre",
            "Été indien",
            "Schöne Müde Grüße",
            "Öl und Kümmel",
            "Mÿrella",
            "Valse Nº1",
            "Café con leche ½",
            "Rosé ¿Qué?",
            "Smörgåsbord",
            "",
            "   ",
        ] {
            assert_eq!(repair(v, None), None, "{v:?}");
            assert_eq!(repair(v, Some(v)), None, "{v:?} with the same file name");
        }
        // UTF-8 を Latin-1 で読んだ形は直さない。半角カナが混ざらない形（「縺輔￥繧」になる）もある
        let utf8_as_latin1 = test_mojibake("さくら".as_bytes());
        assert_eq!(repair(&utf8_as_latin1, None), None);
        assert_eq!(repair(&test_mojibake("Démon".as_bytes()), None), None);
        // 日本語として読める正しい値（U+00FF を超える文字を含む）は触らない
        assert_eq!(repair("さくら色の坂道", None), None);
    }

    /// ほかの文字コードで書かれた値を、Latin-1 で読んだ形。日本語ではないので直さない（v3.1.2 の検査で足した）。
    /// バイトは各文字コードの表から引いたもの（Python の str.encode で作った）
    #[test]
    fn other_single_byte_scripts_are_kept() {
        for (bytes, word) in [
            // ロシア語ほか（CP1251）
            // ふつうの単語だけ（空・夜・機械・夏・こんにちは世界・町と海・星・ウクライナ・畑）
            (b"\xcd\xe5\xe1\xee".as_slice(), "Небо"),
            (b"\xcd\xee\xf7\xfc", "Ночь"),
            (b"\xcc\xe0\xf8\xe8\xed\xe0", "Машина"),
            (b"\xcb\xe5\xf2\xee", "Лето"),
            (b"\xcf\xf0\xe8\xe2\xe5\xf2 \xec\xe8\xf0", "Привет мир"),
            (b"\xc3\xee\xf0\xee\xe4 \xe8 \xec\xee\xf0\xe5", "Город и море"),
            (b"\xc7\xe2\xb8\xe7\xe4\xfb", "Звёзды"),
            (b"\xd3\xea\xf0\xe0\xbf\xed\xe0", "Україна"),
            (b"\x8c\xe8\xe2\xe0", "Њива"),
            (b"\x93\xcd\xe5\xe1\xee\x94", "“Небо”"),
            (b"\xab\xcd\xe5\xe1\xee\xbb", "«Небо»"),
            (b"\x93\xce\xed\x94", "“Он”"),
            (b"\x93\xc4\xe0\x94", "“Да”"),
            (b"\x93\xdf\x94", "“Я”"),
            // ギリシャ語（CP1253）
            (b"\xc1\xe8\xde\xed\xe1", "Αθήνα"),
            (b"\xca\xe1\xeb\xe7\xec\xdd\xf1\xe1", "Καλημέρα"),
            (b"\xc8\xdc\xeb\xe1\xf3\xf3\xe1", "Θάλασσα"),
            (b"\x93\xc1\xe8\xde\xed\xe1\x94", "“Αθήνα”"),
            // 東欧（CP1250）
            (b"\x8akoda \xc8aj", "Škoda Čaj"),
            (b"\x8eu\x9eek", "Žužek"),
            (b"Szcz\xea\x9ccie", "Szczęście"),
            (b"\x8a\x9dastn\xfd", "Šťastný"),
            (b"\xa3\xf3d\x9f", "Łódź"),
            (b"Po\xf8\xe1dek", "Pořádek"),
            (b"Za\xbf\xf3\xb3\xe6 g\xea\x9cl\xb9 ja\x9f\xf1", "Zażółć gęślą jaźń"),
            // 単語の終わりにアクセント付きの文字が2つ続く形・ダッシュのすぐ後ろで単語が始まる形（v3.1.2 の検査で足した）
            (b"pie\x9c\xf1", "pieśń"),
            (b"cze\x9c\xe6", "cześć"),
            (b"\x97\x8fle", "—Źle"),
            // 韓国語（CP949）
            (b"\xbb\xe7\xb6\xfb", "사랑"),
            (b"\xbe\xc8\xb3\xe7\xc7\xcf\xbc\xbc\xbf\xe4", "안녕하세요"),
            (b"\xbc\xad\xbf\xef", "서울"),
            (b"\xb3\xca\xb8\xa6 \xbb\xe7\xb6\xfb\xc7\xd8", "너를 사랑해"),
            (b"\xb5\xb5\xbc\xad\xb0\xfc\xbf\xa1\xbc\xad", "도서관에서"),
            // 西欧（CP1252）の引用符・アポストロフィ
            (b"\x93A\x94", "“A”"),
            (b"\x91A\x92", "‘A’"),
            (b"\x91N\x92", "‘N’"),
            (b"\x93Hello\x94", "“Hello”"),
            (b"Raison d\x92\xeatre", "Raison d’être"),
            (b"Rock \x91n\x92 Roll", "Rock ‘n’ Roll"),
            (b"\x93S\xe4g\x94", "“Säg”"),
            // 引用符・ダッシュ・アポストロフィ＋アクセント付きの文字で単語が始まる形（v3.1.2 の検査で足した）
            (b"L\x92\xe9t\xe9", "L’été"),
            (b"L\x92\xcele", "L’Île"),
            (b"L\x92\x8cuvre", "L’Œuvre"),
            (b"\x97\xc9lan", "—Élan"),
            (b"Pro\x96\xd1and\xfa", "Pro–Ñandú"),
            (b"\x93\xc9o", "“Éo"),
            // 英語の ’s・’t・’d・’m が数字・空白の隣に来る形（v3.1.2 の検査で足した）
            (b"80\x92s", "80’s"),
            (b"Hits of the 90\x92s", "Hits of the 90’s"),
            (b"\x92s", "’s"),
            (b"\x92t", "’t"),
            (b"\x92d", "’d"),
            (b"\x92m", "’m"),
            // 閉じの ” ’ がアクセント付きの文字と組になる形（v3.1.2 の検査で足した。決まり 3 の `quote_closes_western`）
            (b"\x93Luk\xe1\x9a\x94", "“Lukáš”"),
            (b"\x93U\x8e\x94", "“UŽ”"),
            (b"\x91U\x8e\x92", "‘UŽ’"),
            (b"\x93M\xc1\x8a\x94", "“MÁŠ”"),
            (b"\x93A LUK\xc1\x8a\x94", "“A LUKÁŠ”"),
            (b"\x93G\xca\x8c\x94", "“GĘŚ”"),
            (b"\x93\xcd\xce\x8e\x94", "“НОЋ”"),
        ] {
            let m = test_mojibake(bytes);
            assert_eq!(repair(&m, None), None, "{word}");
            assert_eq!(repair(&m, Some("x")), None, "{word} with a file name");
        }
    }

    /// 決まり 1〜4 だけでは日本語として読めてしまう値（looks_japanese が止めていることを確かめる。v3.1.2 の検査で足した）。
    /// テストの単語を差し替えても、守りを通っているかが分からなくならないように
    #[test]
    fn looks_japanese_is_what_keeps_them() {
        let base_rules_accept = |bytes: &[u8]| {
            let c1 = bytes.iter().any(|&b| (0x80..=0x9F).contains(&b));
            let high = bytes.iter().filter(|&&b| b >= 0x80).count();
            (c1 || high >= 3)
                && !utf8_mojibake(bytes)
                && decode_allow_cut(CP_SHIFT_JIS, bytes).is_some_and(|(s, _)| plausible(&s) && half_width_kana(&s) < 2)
        };
        for (bytes, word) in [
            (b"\xcd\xe5\xe1\xee".as_slice(), "Небо"),
            (b"\xcd\xee\xf7\xfc", "Ночь"),
            (b"\xcb\xe5\xf2\xee", "Лето"),
            (b"\x8c\xe8\xe2\xe0", "Њива"),
            (b"\x93\xcd\xe5\xe1\xee\x94", "“Небо”"),
            (b"\x8eu\x9eek", "Žužek"),
            (b"pie\x9c\xf1", "pieśń"),
            (b"cze\x9c\xe6", "cześć"),
            (b"\x97\x8fle", "—Źle"),
            (b"D\xe9mon \xc9p\xe9e", "Démon Épée"),
            (b"Lumi\xe8re \xe9ph\xe9m\xe8re", "Lumière éphémère"),
            (b"L\x92\xe9t\xe9", "L’été"),
            (b"L\x92\xcele", "L’Île"),
            (b"L\x92\x8cuvre", "L’Œuvre"),
            (b"\x97\xc9lan", "—Élan"),
            (b"Pro\x96\xd1and\xfa", "Pro–Ñandú"),
            (b"\x93\xc9o", "“Éo"),
            (b"80\x92s", "80’s"),
            (b"Hits of the 90\x92s", "Hits of the 90’s"),
            (b"\x92s", "’s"),
            (b"\x92t", "’t"),
            (b"\x92d", "’d"),
            (b"\x92m", "’m"),
            (b"\x93S\xe4g\x94", "“Säg”"),
            (b"\x93Luk\xe1\x9a\x94", "“Lukáš”"),
            (b"\x93U\x8e\x94", "“UŽ”"),
            (b"\x91U\x8e\x92", "‘UŽ’"),
            (b"\x93M\xc1\x8a\x94", "“MÁŠ”"),
            (b"\x93A LUK\xc1\x8a\x94", "“A LUKÁŠ”"),
            (b"\x93G\xca\x8c\x94", "“GĘŚ”"),
            (b"\x93\xcd\xce\x8e\x94", "“НОЋ”"),
        ] {
            assert!(base_rules_accept(bytes), "{word}: 決まり 1〜4 で止まってしまい、守りを確かめられない");
            assert_eq!(repair(&test_mojibake(bytes), None), None, "{word}");
        }
    }

    /// 日本語に英数字が混ざった値（v3.1.2 で直した）。漢字と英字の間に空白か記号があれば直す。
    /// 空白が無くても、漢字のバイトが西欧の文字の並びに見えなければ直す（looks_japanese の決まり 4）
    #[test]
    fn japanese_with_ascii_is_repaired() {
        for (bytes, want) in [
            (b"NEW \x96{".as_slice(), "NEW 本"),
            (b"Vol.1 \x96{", "Vol.1 本"),
            (b"DJ \x92r\x93c", "DJ 池田"),
            (b"\x92r\x93c DJ", "池田 DJ"),
            (b"Mr.\x8bg\x93c", "Mr.吉田"),
            (b"2nd \x94~", "2nd 梅"),
            (b"Hello \x8bg\x93c world", "Hello 吉田 world"),
            (b"Love \x96\xe9", "Love 夜"),
            (b"LIVE \x95\x90\x93\xb9\x8a\xd9", "LIVE 武道館"),
            (b"\x91\xe61\x98b", "第1話"),
            (b"T\x83V\x83\x83\x83c", "Tシャツ"),
            (b"\xe2V\x93\xa1", "齋藤"),
            // 漢字1文字（2バイト目が英字の形。0x91・0x92＋s・t・d・m の8つのほかは直す）
            (b"\x92r", "池"),
            (b"\x8ft", "春"),
            // 漢字と英字が空白なしで並ぶ形のうち、西欧の文字の並びに見えないもの（v3.1.2 の検査で直した）
            (b"A\x96\xca", "A面"),
            (b"B\x96\xca", "B面"),
            (b"TV\x94\xc5", "TV版"),
            (b"EP\x94\xc5", "EP版"),
            (b"LP\x94\xc5", "LP版"),
            (b"S\x90\xc8", "S席"),
            (b"LIVE\x95\x90\x93\xb9\x8a\xd9", "LIVE武道館"),
            (b"Love\x96\xe9", "Love夜"),
            (b"\x93\x8c\x8b\x9eDOME", "東京DOME"),
        ] {
            assert_eq!(sjis(bytes), want);
            assert_eq!(repair(&test_mojibake(bytes), None).as_deref(), Some(want), "{want}");
        }
    }

    /// かなが無く、“ ‘ と同じバイト（0x93・0x91）の漢字で始まり、2バイト目が ” ’ と同じバイト（0x94・0x92）の漢字で終わる値
    /// （v3.1.2 の検査で直した。決まり 3 が値の両端のバイトだけで「引用符で囲まれた値」とみなしていた）
    #[test]
    fn japanese_starting_like_a_quote_is_repaired() {
        for (bytes, want) in [
            (b"\x93\x8c\x95\x94".as_slice(), "東部"),
            (b"\x93\xec\x95\x94", "南部"),
            (b"\x93\xe0\x95\x94", "内部"),
            (b"\x93\x8a\x95[\x90\x94", "投票数"),
            (b"\x93\x8c\x96k\x95\x94", "東北部"),
            (b"\x93o\x98^\x90\x94", "登録数"),
            (b"\x93\xbe\x95[\x90\x94", "得票数"),
            (b"\x93\xb9\x98H\x93\x94", "道路灯"),
            (b"\x93\xc1\x95\xca\x8c\x94", "特別券"),
            (b"\x93\x8c\x95\x94/\x93\xec\x95\x94", "東部/南部"),
            (b"\x91K\x93\x92", "銭湯"),
            (b"\x91\x8d\x93\x92", "総湯"),
        ] {
            assert_eq!(sjis(bytes), want);
            assert_eq!(repair(&test_mojibake(bytes), None).as_deref(), Some(want), "{want}");
        }
    }

    /// 直らない日本語（決めたうえで諦めたもの。looks_japanese の説明を参照）
    #[test]
    fn japanese_left_unrepaired_by_design() {
        for (bytes, word) in [
            // 漢字と英字の間に空白が無く、西欧の文字の並びと同じ形（2バイト目が ASCII・後ろに英字・1バイト目が Š）
            (b"DJ\x8bg\x93c".as_slice(), "DJ吉田"),
            (b"A\x96\xcaB\x96\xca", "A面B面"),
            (b"\x96\xcaDX", "面DX"),
            (b"A\x8a\xd9", "A館"),
            // 0x91・0x92＋s・t・d・m の漢字が、かなも別の漢字も無い値にただ1つ（英語の ’s・’t と同じ形）
            (b"\x91t", "奏"),
            (b"LIVE \x92m", "LIVE 知"),
            // かなが無く、第二水準・拡張の漢字が半分以上
            (b"\xee\xe0\x8b\xb4", "髙橋"),
            (b"\xed\x95\x8eR", "﨑山"),
            // “ と同じバイトの漢字で始まり、株・券など（1バイト目が Š・Œ と同じ）で終わり、2バイト目が英字の漢字を含む（「“UŽ”」と同じ形）
            (b"\x93\x8c\x90V\x8a\x94", "東新株"),
        ] {
            assert_eq!(sjis(bytes), word);
            assert_eq!(repair(&test_mojibake(bytes), None), None, "{word}");
        }
    }

    #[test]
    fn extreme_inputs_do_not_panic_or_change() {
        // 巨大な値：上限を超えたら見ない（すぐ返る）
        let huge = "\u{82}\u{a0}".repeat(MAX_CHARS);
        let t = std::time::Instant::now();
        assert_eq!(repair(&huge, None), None);
        assert!(t.elapsed().as_millis() < 2000);
        // 上限ちょうどの「あ」の列は読める
        let ok = "\u{82}\u{a0}".repeat(MAX_CHARS / 2);
        assert_eq!(repair(&ok, None).map(|s| s.chars().count()), Some(MAX_CHARS / 2));
        // 壊れたバイト（続かない先頭バイト・0xFF・0x00）
        for v in ["\u{81}\u{20}\u{81}\u{20}\u{81}", "\u{82}\u{7f}\u{82}\u{7f}", "\u{ff}\u{ff}\u{ff}", "\u{81}\u{0}\u{81}\u{0}", "\u{a0}\u{a0}\u{a0}", "\u{80}\u{80}\u{80}"] {
            assert_eq!(repair(v, Some("x")), None, "{v:?}");
        }
        // 全部が化けたように見える英語（0x80 以上だけでできた値）
        let all_high: String = (0x80u32..=0xFF).filter_map(char::from_u32).collect();
        assert_eq!(repair(&all_high, Some(&all_high)), None);
    }

    #[test]
    fn other_codepages_need_the_file_name() {
        // 「さくら」を EUC-JP で（A4 B5 A4 AF A4 E9）。Shift_JIS では半角カナになるので Shift_JIS としては採らない
        let euc = b"\xa4\xb5\xa4\xaf\xa4\xe9";
        let m = test_mojibake(euc);
        assert_eq!(repair(&m, None), None);
        assert_eq!(repair(&m, Some("さくら")).as_deref(), Some("さくら"));
        assert_eq!(repair(&m, Some("01 さくら")).as_deref(), Some("さくら"));
        assert_eq!(repair(&m, Some("01 - さくら")).as_deref(), Some("さくら"));
        assert_eq!(repair(&m, Some("1-01 さくら")).as_deref(), Some("さくら"));
        assert_eq!(repair(&m, Some("01さくら")).as_deref(), Some("さくら"));
        // ファイル名が違えば採らない
        assert_eq!(repair(&m, Some("ちがう名前")), None);
        // 「春风」（GBK：B4 BA B7 E7）
        let gbk = b"\xb4\xba\xb7\xe7";
        assert_eq!(codepage::decode_strict(CP_GBK, gbk).as_deref(), Some("春风"));
        assert_eq!(repair(&test_mojibake(gbk), Some("02. 春风")).as_deref(), Some("春风"));
        assert_eq!(repair(&test_mojibake(gbk), None), None);
        // 「春風」（Big5：AC 4B AD B7）
        let big5 = b"\xac\x4b\xad\xb7";
        assert_eq!(codepage::decode_strict(CP_BIG5, big5).as_deref(), Some("春風"));
        assert_eq!(repair(&test_mojibake(big5), Some("春風")).as_deref(), Some("春風"));
    }

    #[test]
    fn track_number_forms() {
        assert_eq!(strip_track_number("01 曲"), Some("曲"));
        assert_eq!(strip_track_number("01. 曲"), Some("曲"));
        assert_eq!(strip_track_number("01_曲"), Some("曲"));
        assert_eq!(strip_track_number("2-03 - 曲"), Some("曲"));
        assert_eq!(strip_track_number("01曲"), Some("曲"));
        assert_eq!(strip_track_number("1999"), None);
        assert_eq!(strip_track_number("3D"), None);
        assert_eq!(strip_track_number("曲"), None);
        assert_eq!(strip_track_number("01 "), None);
        assert_eq!(strip_track_number("12345 曲"), None);
    }

    #[test]
    fn repair_values_touches_only_broken_fields() {
        let mut v = TagValues {
            title: test_mojibake(SJIS_SAKAMICHI),
            artist: "Raison d'être".into(),
            album: test_mojibake(SJIS_AOI),
            ..Default::default()
        };
        assert!(repair_values(&mut v, Path::new("x.mp3")));
        assert_eq!(v.title, "さくら色の坂道");
        assert_eq!(v.artist, "Raison d'être");
        assert_eq!(v.album, "アオイ");
        let mut plain = TagValues { title: "Plain".into(), ..Default::default() };
        assert!(!repair_values(&mut plain, Path::new("x.mp3")));
    }
}
