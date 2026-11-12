//! この PC のドライブにある場所か（v3.1.1。`docs\ペネトレーションテスト_v3.0.2.md` の5）。
//!
//! UNC（`\\ホスト\共有`）やネットワークドライブのパスに `exists()` などで触ると、Windows が SMB で相手につなぎ、
//! 相手に NTLM の認証情報が渡りうる。取り込むフォルダ・設定 zip のライブラリのフォルダ・エクスプローラーで開くパスは、
//! **ファイルに触る前に**ここで確かめ、ネットワーク上の場所なら断る。
//!
//! 決め方（ファイルには触らない）：
//! 1. 文字だけで見る。`X:\` か `X:/` で始まるものだけを通す。`\\` `//` で始まるもの（UNC・`\\?\`・`\\.\`）、
//!    `\` だけで始まるもの、ドライブの無い相対パス、`C:foo`（ドライブの今のフォルダからの相対）は断る
//! 2. ドライブの種類を `GetDriveTypeW` で聞く。ネットワークドライブ（DRIVE_REMOTE）・無いドライブ・種類が分からないものは断る。
//!    固定ディスクのほか、USB メモリなどの取り外せるドライブ・CD・RAM ディスクは通す（音楽を USB に置く使い方があるため）

use std::path::Path;

use crate::error::{AppError, ErrorKind};

/// GetDriveTypeW の値（Win32 の定数と同じ）。断る側の値は、名前を付けて残すためだけに置く（使うのはテスト）
#[allow(dead_code)]
pub(crate) mod drive {
    pub const UNKNOWN: u32 = 0;
    pub const NO_ROOT_DIR: u32 = 1;
    pub const REMOVABLE: u32 = 2;
    pub const FIXED: u32 = 3;
    pub const REMOTE: u32 = 4;
    pub const CDROM: u32 = 5;
    pub const RAMDISK: u32 = 6;
}

/// 文字だけで見て、`X:\` か `X:/` で始まるならそのドライブの文字を返す（ファイルに触らない）
pub(crate) fn drive_letter(p: &Path) -> Option<char> {
    let s = p.as_os_str().to_string_lossy();
    let mut c = s.chars();
    let letter = c.next()?;
    if !letter.is_ascii_alphabetic() || c.next()? != ':' {
        return None;
    }
    match c.next()? {
        '\\' | '/' => Some(letter.to_ascii_uppercase()),
        _ => None,
    }
}

/// ドライブの種類として通してよいか
pub(crate) fn drive_type_allowed(t: u32) -> bool {
    matches!(t, drive::FIXED | drive::REMOVABLE | drive::CDROM | drive::RAMDISK)
}

#[cfg(windows)]
fn drive_type(letter: char) -> u32 {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    let root: Vec<u16> = format!("{letter}:\\").encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) }
}

#[cfg(not(windows))]
fn drive_type(_letter: char) -> u32 {
    drive::FIXED
}

/// この PC のドライブにある場所なら true（ネットワーク上の場所・形の分からないパスは false）
pub fn is_local(p: &Path) -> bool {
    match drive_letter(p) {
        Some(l) => drive_type_allowed(drive_type(l)),
        None => false,
    }
}

/// ネットワーク上の場所ならエラー（`network_path_unsupported`）
pub fn check_local(p: &Path) -> Result<(), AppError> {
    if is_local(p) {
        Ok(())
    } else {
        Err(AppError::new(ErrorKind::NetworkPathUnsupported, p.display().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 文字だけで断る。ここに並べたパスには一度も触らない（`drive_letter` は文字を見るだけ。
    /// `is_local` もドライブの文字が取れないものは `GetDriveTypeW` を呼ばずに false）。
    /// 実在しない名前（`.invalid`）とループバックを混ぜているが、SMB にも名前解決にも行かない
    #[test]
    fn unc_and_odd_forms_are_rejected_by_text() {
        for bad in [
            r"\\127.0.0.1\share\music",
            r"\\localhost\c$\Users",
            r"\\pl-player-test.invalid\share",
            "//pl-player-test.invalid/share/music",
            r"\\?\UNC\pl-player-test.invalid\share",
            r"\\?\C:\Music",
            r"\\.\C:\Music",
            r"\??\UNC\pl-player-test.invalid\share",
            r"\Music",
            "/Music",
            r"Music\a.mp3",
            "C:Music",
            "C:",
            "",
            "1:\\Music",
            "ＣＣ:\\Music",
        ] {
            assert_eq!(drive_letter(Path::new(bad)), None, "{bad}");
            assert!(!is_local(Path::new(bad)), "{bad}");
            assert_eq!(check_local(Path::new(bad)).unwrap_err().kind, ErrorKind::NetworkPathUnsupported, "{bad}");
        }
        assert_eq!(drive_letter(Path::new(r"c:\Music")), Some('C'));
        assert_eq!(drive_letter(Path::new("D:/Music")), Some('D'));
    }

    /// ドライブの種類：ネットワークドライブ・無いドライブ・分からないものは断る
    #[test]
    fn drive_types() {
        assert!(!drive_type_allowed(drive::REMOTE));
        assert!(!drive_type_allowed(drive::NO_ROOT_DIR));
        assert!(!drive_type_allowed(drive::UNKNOWN));
        for ok in [drive::FIXED, drive::REMOVABLE, drive::CDROM, drive::RAMDISK] {
            assert!(drive_type_allowed(ok));
        }
        // このプロジェクトのあるドライブ（固定ディスク）は通る
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(is_local(here), "{}", here.display());
    }
}
