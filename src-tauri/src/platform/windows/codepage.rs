//! 古い文字コード（Shift_JIS など）のバイト列を読む（v3.1.2）。Windows の MultiByteToWideChar を使う（依存を足さないため）。
//! 使い道は `library::legacy_text`（タグの文字化けを読むときに直す）だけ。

use windows::Win32::Globalization::{MultiByteToWideChar, MB_ERR_INVALID_CHARS};

/// Shift_JIS（Windows の CP932）
pub const CP_SHIFT_JIS: u32 = 932;
/// EUC-JP（Windows の 20932）
pub const CP_EUC_JP: u32 = 20932;
/// GBK（Windows の CP936）
pub const CP_GBK: u32 = 936;
/// Big5（Windows の CP950）
pub const CP_BIG5: u32 = 950;

/// これより長いバイト列は読まない（タグの1項目としてはありえない長さ。MultiByteToWideChar の長さは i32 で渡すため）
pub const MAX_BYTES: usize = 1024 * 1024;

/// `bytes` を `codepage` として読む。読めないバイトが1つでもあれば None（MB_ERR_INVALID_CHARS。代わりの文字で埋めない）。
/// 空・長すぎるときも None
pub fn decode_strict(codepage: u32, bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return None;
    }
    // 1回目は大きさを聞く。1バイトから2文字以上にはならないので、バイト数を上限にして確かめる
    let need = unsafe { MultiByteToWideChar(codepage, MB_ERR_INVALID_CHARS, bytes, None) };
    if need <= 0 || need as usize > bytes.len() {
        return None;
    }
    let mut buf = vec![0u16; need as usize];
    let n = unsafe { MultiByteToWideChar(codepage, MB_ERR_INVALID_CHARS, bytes, Some(&mut buf)) };
    if n <= 0 {
        return None;
    }
    buf.truncate(n as usize);
    String::from_utf16(&buf).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_refuses_invalid_bytes() {
        // 「あ」は Shift_JIS で 82 A0、EUC-JP で A4 A2、GBK で A4 A2。「中」は Big5 で A4 A4
        assert_eq!(decode_strict(CP_SHIFT_JIS, &[0x82, 0xA0]).as_deref(), Some("あ"));
        assert_eq!(decode_strict(CP_EUC_JP, &[0xA4, 0xA2]).as_deref(), Some("あ"));
        assert_eq!(decode_strict(CP_GBK, &[0xA4, 0xA2]).as_deref(), Some("あ"));
        assert_eq!(decode_strict(CP_BIG5, &[0xA4, 0xA4]).as_deref(), Some("中"));
        // Windows の CP950 は、Big5 の拡張（ETEN のかな C6 A6 など・香港の拡張）を私用領域の文字に読む。
        // 読んだ結果に私用領域があれば legacy_text が採らない（ファイル名とも一致しない）
        assert_eq!(decode_strict(CP_BIG5, &[0xC6, 0xA6]).as_deref(), Some("\u{F6B6}"));
        assert!(decode_strict(CP_BIG5, &[0x88, 0x62]).is_some_and(|s| s.chars().all(|c| ('\u{E000}'..='\u{F8FF}').contains(&c))));
        // 末尾が切れた2バイト文字・続かないバイトは読まない
        assert_eq!(decode_strict(CP_SHIFT_JIS, &[0x41, 0x82]), None);
        assert_eq!(decode_strict(CP_SHIFT_JIS, &[0x82, 0x20]), None);
        // 空・長すぎるものは読まない（落ちない）
        assert_eq!(decode_strict(CP_SHIFT_JIS, &[]), None);
        assert_eq!(decode_strict(CP_SHIFT_JIS, &vec![0x41; MAX_BYTES + 1]), None);
        assert_eq!(decode_strict(CP_SHIFT_JIS, &vec![0x41; MAX_BYTES]).map(|s| s.len()), Some(MAX_BYTES));
    }
}
