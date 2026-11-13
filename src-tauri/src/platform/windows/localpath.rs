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
//!
//! v3.3.0：ごみ箱へ移すときは、上の確かめのあと、ファイルの本当の場所（`canonicalize`。途中のジャンクション・リンクをたどる）の
//! 形を `real_place` で見て、ドライブの文字で始まるときだけ、その場所を含むボリュームの種類を `volume_drive_type_of_real` で聞く

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

/// v3.3.0：本当の場所（`std::fs::canonicalize` の結果。`\\?\C:\…` や `\\?\UNC\…` の形）がどこにあるか。文字だけで見る（触らない）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RealPlace {
    /// この PC のドライブの文字で始まる（`C:\…`・`\\?\C:\…`）
    Disk,
    /// ネットワーク上（`\\ホスト\共有`・`\\?\UNC\…`）
    Network,
    /// それ以外（ドライブの文字の無いボリューム `\\?\Volume{…}`・`\\.\…` の装置・相対パスなど）
    Unknown,
}

pub(crate) fn real_place(real: &Path) -> RealPlace {
    use std::path::{Component, Prefix};
    let mut c = real.components();
    match (c.next(), c.next()) {
        (Some(Component::Prefix(p)), Some(Component::RootDir)) => match p.kind() {
            Prefix::Disk(_) | Prefix::VerbatimDisk(_) => RealPlace::Disk,
            Prefix::UNC(..) | Prefix::VerbatimUNC(..) => RealPlace::Network,
            _ => RealPlace::Unknown,
        },
        (Some(Component::Prefix(p)), _) if matches!(p.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..)) => RealPlace::Network,
        _ => RealPlace::Unknown,
    }
}

/// v3.3.0：本当の場所（`canonicalize` の結果）を含むボリュームの種類。ごみ箱へ移すときに使う。
/// ドライブの文字ではなく、その場所を含むボリュームの根（`GetVolumePathNameW`。フォルダに差し込んだボリュームも、
/// そのボリュームの根を返す）を `GetDriveTypeW` に聞く。
/// ドライブの文字で始まらない場所（ネットワーク上・形の分からない場所）は、何も聞かずに None（`GetVolumePathNameW` は
/// ネットワーク上の場所だと共有につなぎに行くため。呼ぶ側は先に `real_place` で断る）。
/// 返った根がドライブの文字で始まらないとき（`\\.\COM2\` のような装置）も None
pub(crate) fn volume_drive_type_of_real(real: &Path) -> Option<u32> {
    if real_place(real) != RealPlace::Disk {
        return None;
    }
    let root = volume_root(real)?;
    if real_place(Path::new(&root)) != RealPlace::Disk {
        return None;
    }
    Some(drive_type_of_root(&root))
}

#[cfg(windows)]
fn volume_root(p: &Path) -> Option<std::ffi::OsString> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetVolumePathNameW;
    let wide: Vec<u16> = p.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // 根は元のパスより長くならない。末尾の \ の分を足す（文書：1文字足りないと \ が欠け、それより短いと失敗する）
    let mut buf = vec![0u16; wide.len() + 2];
    unsafe { GetVolumePathNameW(PCWSTR(wide.as_ptr()), &mut buf) }.ok()?;
    let n = buf.iter().position(|&c| c == 0)?;
    let mut root = buf[..n].to_vec();
    if root.is_empty() {
        return None;
    }
    if root.last() != Some(&(b'\\' as u16)) {
        root.push(b'\\' as u16);
    }
    Some(std::ffi::OsString::from_wide(&root))
}

#[cfg(not(windows))]
fn volume_root(p: &Path) -> Option<std::ffi::OsString> {
    p.ancestors().last().map(|r| r.as_os_str().to_owned())
}

#[cfg(windows)]
fn drive_type_of_root(root: &std::ffi::OsStr) -> u32 {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    let w: Vec<u16> = root.encode_wide().chain(std::iter::once(0)).collect();
    unsafe { GetDriveTypeW(PCWSTR(w.as_ptr())) }
}

#[cfg(not(windows))]
fn drive_type_of_root(_root: &std::ffi::OsStr) -> u32 {
    drive::FIXED
}

/// v3.3.0：曲をごみ箱へ移してよいドライブの種類か。固定ディスクだけ。
/// USB メモリなどの取り外せるドライブ・CD・RAM ディスクには、ふつうごみ箱が無く、移すと完全に消えるので断る
pub(crate) fn recycle_drive_allowed(t: u32) -> bool {
    t == drive::FIXED
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
        // v3.3.0：ごみ箱へ移せるのは固定ディスクだけ
        assert!(recycle_drive_allowed(drive::FIXED));
        for no in [drive::REMOVABLE, drive::CDROM, drive::RAMDISK, drive::REMOTE, drive::NO_ROOT_DIR, drive::UNKNOWN] {
            assert!(!recycle_drive_allowed(no), "{no}");
        }
        // このプロジェクトのあるドライブ（固定ディスク）は通る
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(is_local(here), "{}", here.display());
    }

    /// v3.3.0：本当の場所の形（文字だけ。ここに並べたパスには触らない。ネットワーク上・形の分からない場所は
    /// ボリュームの種類も聞かない）
    #[test]
    fn real_place_by_text() {
        for disk in [r"\\?\C:\Music\a.mp3", r"\\?\d:\a.flac", r"C:\Music\a.mp3", r"\\?\C:\"] {
            assert_eq!(real_place(Path::new(disk)), RealPlace::Disk, "{disk}");
        }
        for net in [
            r"\\?\UNC\pl-player-test.invalid\share\a.mp3",
            r"\\pl-player-test.invalid\share\a.mp3",
            r"\\127.0.0.1\c$\a.mp3",
            r"\\?\UNC\pl-player-test.invalid\share",
        ] {
            assert_eq!(real_place(Path::new(net)), RealPlace::Network, "{net}");
            assert_eq!(volume_drive_type_of_real(Path::new(net)), None, "{net}");
        }
        for odd in [
            r"\\?\Volume{00000000-0000-0000-0000-000000000000}\a.mp3",
            r"\\.\C:\a.mp3",
            r"\\.\COM2",
            r"\\?\GLOBALROOT\Device\HarddiskVolume1\a.mp3",
            "C:a.mp3",
            r"Music\a.mp3",
            r"\Music\a.mp3",
            "",
        ] {
            assert_eq!(real_place(Path::new(odd)), RealPlace::Unknown, "{odd}");
            assert_eq!(volume_drive_type_of_real(Path::new(odd)), None, "{odd}");
        }
    }

    /// v3.3.0：本当の場所のボリュームの種類。このプロジェクトのあるドライブは固定ディスク（`\\?\` の付いた形でも、付かない形でも。
    /// まだ無いファイルの名前でも、そのボリューム）。根はドライブの文字で始まり、\ で終わる
    #[test]
    fn volume_drive_type_of_real_location() {
        let here = std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
        assert!(here.to_string_lossy().starts_with(r"\\?\"), "{}", here.display());
        assert_eq!(volume_drive_type_of_real(&here), Some(drive::FIXED), "{}", here.display());
        assert_eq!(volume_drive_type_of_real(Path::new(env!("CARGO_MANIFEST_DIR"))), Some(drive::FIXED));
        assert_eq!(volume_drive_type_of_real(&here.join("no-such-file.mp3")), Some(drive::FIXED));
        let root = volume_root(&here).unwrap();
        assert!(root.to_string_lossy().ends_with('\\'), "{root:?}");
        assert_eq!(real_place(Path::new(&root)), RealPlace::Disk, "{root:?}");
    }
}
