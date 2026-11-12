//! 文字の正規化（NFKC）。Windows の NormalizeString を使う（依存を足さないため）。
//! 歌詞をネットから取るときの名前の照合（lyrics::lrclib::norm_name）で、全角・半角などをそろえるのに使う。

use windows::Win32::Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER};
use windows::Win32::Globalization::{NormalizeString, NormalizationKC};

/// NFKC に正規化する。失敗したら元の文字列のまま返す
pub fn nfkc(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let src: Vec<u16> = s.encode_utf16().collect();
    // 1回目は大きさの見積もり。足りなければ、返ってきた見積もりで数回までやり直す
    let mut cap = unsafe { NormalizeString(NormalizationKC, &src, None) };
    for _ in 0..4 {
        if cap <= 0 {
            return s.to_string();
        }
        let mut dst = vec![0u16; cap as usize];
        let n = unsafe { NormalizeString(NormalizationKC, &src, Some(&mut dst)) };
        if n > 0 {
            dst.truncate(n as usize);
            return String::from_utf16(&dst).unwrap_or_else(|_| s.to_string());
        }
        if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
            return s.to_string();
        }
        cap = n.checked_neg().unwrap_or(0).max(cap.saturating_mul(2));
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfkc_folds_width_and_compat() {
        assert_eq!(nfkc("ＡＢＣ１２３"), "ABC123");
        assert_eq!(nfkc("ｱｲｳｴｵｶﾞ"), "アイウエオガ");
        assert_eq!(nfkc("ﬁ①²"), "fi12");
        assert_eq!(nfkc(""), "");
        assert_eq!(nfkc("ふつうの文字 abc"), "ふつうの文字 abc");
        let long = "あ".repeat(5000);
        assert_eq!(nfkc(&long), long);
    }
}
