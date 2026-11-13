//! 曲ファイルを Windows のごみ箱へ移す（v3.3.0）。**完全には消さない。**
//!
//! 作り：シェルの `IFileOperation` で1ファイルずつ消す（`FOF_ALLOWUNDO`・`FOFX_RECYCLEONDELETE`＝ごみ箱へ。画面は出さない）。
//! ごみ箱へ入れられない項目（ごみ箱を使わない設定のドライブ・ごみ箱に入らない大きさ など）は、シェルが確かめずに
//! 完全に消してしまう（`FOF_NO_UI` は確認も出さない）。それを止めるため、進み具合の受け手（`IFileOperationProgressSink`）の
//! `PreDeleteItem` で、渡される印に `TSF_DELETE_RECYCLE_IF_POSSIBLE`（0x80）が無ければエラーを返して取りやめる
//! （エラーを返すと、その削除と後に続く操作は取りやめになる：Microsoft の `PreDeleteItem` の文書。
//! Electron の `shell.trashItem` も同じ確かめ方）。ドライブの種類による断り（USB メモリなど）は、呼ぶ側（`library::trash`）が先に行う。
//!
//! COM のインターフェースを実装するマクロ（`#[implement]`）は `windows-core` を直接の依存として要求するので、
//! `device_watch.rs` と同じく関数表（vtable）を手で組む。シェルの操作は STA が要るので、専用のスレッドで行う。
//!
//! 通信しない。開くのは渡されたパスだけ（`SHCreateItemFromParsingName` は `X:\` で始まる場所だけを受ける。呼ぶ側が確かめる）。
//!
//! 通常のファイルか（フォルダ・リンク・ジャンクションでないか）は、呼ぶ側の確かめ（`library::trash::plan`）のあとにも
//! 2回確かめる：シェルを呼ぶ直前と、シェルが消す直前（`PreDeleteItem` に渡された項目）。確かめたあとで同じ名前のフォルダに
//! 差し替えられても、フォルダごとごみ箱へ入れない。
//!
//! テスト用の「ごみ箱へ移したあと、その項目だけをごみ箱から消す」（`move_to_recycle_bin_then_purge`）は、
//! テストのときと、この crate の機能 `test-support` を付けてビルドしたときだけ入る（配布物には入らない）。

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::{IUnknown, IUnknown_Vtbl, Interface, GUID, HRESULT, PCWSTR};
use windows::Win32::Foundation::{E_ABORT, E_NOINTERFACE, E_POINTER, S_OK};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Shell::{
    FileOperation, IFileOperation, IFileOperationProgressSink, IFileOperationProgressSink_Vtbl, IShellItem, SHCreateItemFromParsingName,
    COPYENGINE_E_ACCESS_DENIED_SRC, COPYENGINE_E_PATH_NOT_FOUND_SRC, COPYENGINE_E_RECYCLE_BIN_NOT_FOUND, COPYENGINE_E_SHARING_VIOLATION_SRC,
    FOFX_EARLYFAILURE, FOFX_RECYCLEONDELETE, FOF_ALLOWUNDO, FOF_NO_UI, SIGDN_FILESYSPATH,
};

use crate::error::{AppError, ErrorKind};

/// `PreDeleteItem` に渡される印のうち、「ごみ箱へ移す」を表すもの（`TRANSFER_SOURCE_FLAGS` の値）
pub(crate) const TSF_DELETE_RECYCLE_IF_POSSIBLE: u32 = 0x80;

/// 通常のファイルか（フォルダ・シンボリックリンク・ジャンクションは断る）。リンクはたどらずに、その場所そのものを見る。
/// 断るときの種別：通常のファイルでない → `TrashNotAllowed`、無い・開けない → `AppError::from_io` の種別
pub(crate) fn check_regular_file(path: &Path) -> Result<(), AppError> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_file() => Ok(()),
        Ok(_) => Err(AppError::new(ErrorKind::TrashNotAllowed, format!("{}: not a regular file", path.display()))),
        Err(e) => Err(AppError::from_io(&e, path)),
    }
}

/// この項目を消してよいか：ごみ箱へ移すときだけ true（完全に消えるなら false で取りやめる）
pub(crate) fn pre_delete_allowed(flags: u32) -> bool {
    flags & TSF_DELETE_RECYCLE_IF_POSSIBLE != 0
}

const fn win32(code: u32) -> i32 {
    (0x8007_0000 | code) as i32
}

/// シェルの返した HRESULT を、画面に出すエラーの種別へ。`nuke_refused` は「完全に消えるので取りやめた」
pub(crate) fn classify(hr: i32, nuke_refused: bool) -> ErrorKind {
    if nuke_refused || hr == COPYENGINE_E_RECYCLE_BIN_NOT_FOUND.0 {
        return ErrorKind::TrashUnavailable;
    }
    match hr {
        // ERROR_SHARING_VIOLATION・ERROR_LOCK_VIOLATION
        h if h == win32(32) || h == win32(33) || h == COPYENGINE_E_SHARING_VIOLATION_SRC.0 => ErrorKind::FileInUse,
        // ERROR_ACCESS_DENIED・ERROR_WRITE_PROTECT
        h if h == win32(5) || h == win32(19) || h == COPYENGINE_E_ACCESS_DENIED_SRC.0 => ErrorKind::AccessDenied,
        // ERROR_FILE_NOT_FOUND・ERROR_PATH_NOT_FOUND
        h if h == win32(2) || h == win32(3) || h == COPYENGINE_E_PATH_NOT_FOUND_SRC.0 => ErrorKind::FileNotFound,
        _ => ErrorKind::Other,
    }
}

// ---------------- 進み具合の受け手（PreDeleteItem / PostDeleteItem だけを使う） ----------------

#[repr(C)]
struct Sink {
    vtbl: *const IFileOperationProgressSink_Vtbl,
    refs: AtomicU32,
    /// PreDeleteItem で、ごみ箱へ移せない（完全に消える）ので取りやめた
    nuke_refused: Cell<bool>,
    /// PreDeleteItem で、消す項目が通常のファイルでない（フォルダ・リンクに差し替えられた・場所が取れない）ので取りやめた
    not_file_refused: Cell<bool>,
    /// PostDeleteItem の結果
    hr_delete: Cell<Option<i32>>,
    /// ごみ箱に入った項目（テストで、ごみ箱から消すのに使う）
    recycled: RefCell<Option<IShellItem>>,
}

unsafe extern "system" fn query_interface(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() || iid.is_null() {
        return E_POINTER;
    }
    let iid = unsafe { *iid };
    if iid == IUnknown::IID || iid == IFileOperationProgressSink::IID {
        unsafe {
            *out = this;
            add_ref(this);
        }
        S_OK
    } else {
        unsafe { *out = std::ptr::null_mut() };
        E_NOINTERFACE
    }
}

unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
    let s = unsafe { &*(this as *const Sink) };
    s.refs.fetch_add(1, Ordering::SeqCst) + 1
}

unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    let s = unsafe { &*(this as *const Sink) };
    let n = s.refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(unsafe { Box::from_raw(this as *mut Sink) });
    }
    n
}

unsafe extern "system" fn ok0(_this: *mut c_void) -> HRESULT {
    S_OK
}
unsafe extern "system" fn finish(_this: *mut c_void, _hr: HRESULT) -> HRESULT {
    S_OK
}
unsafe extern "system" fn pre_rename(_t: *mut c_void, _f: u32, _i: *mut c_void, _n: PCWSTR) -> HRESULT {
    S_OK
}
unsafe extern "system" fn post_rename(_t: *mut c_void, _f: u32, _i: *mut c_void, _n: PCWSTR, _hr: HRESULT, _c: *mut c_void) -> HRESULT {
    S_OK
}
unsafe extern "system" fn pre_move(_t: *mut c_void, _f: u32, _i: *mut c_void, _d: *mut c_void, _n: PCWSTR) -> HRESULT {
    S_OK
}
unsafe extern "system" fn post_move(_t: *mut c_void, _f: u32, _i: *mut c_void, _d: *mut c_void, _n: PCWSTR, _hr: HRESULT, _c: *mut c_void) -> HRESULT {
    S_OK
}
unsafe extern "system" fn pre_delete(this: *mut c_void, flags: u32, item: *mut c_void) -> HRESULT {
    let s = unsafe { &*(this as *const Sink) };
    if !pre_delete_allowed(flags) {
        // 完全に消えてしまう：取りやめる（この削除と、後に続く操作が取りやめになる）
        s.nuke_refused.set(true);
        return E_ABORT;
    }
    // シェルが消す直前に、もう一度「通常のファイルか」（確かめたあとで差し替えられていないか）。場所が取れなければ取りやめる
    let regular = unsafe { IShellItem::from_raw_borrowed(&item) }
        .and_then(|i| unsafe { display_path(i) })
        .is_some_and(|p| check_regular_file(&p).is_ok());
    if !regular {
        s.not_file_refused.set(true);
        return E_ABORT;
    }
    S_OK
}
unsafe extern "system" fn post_delete(this: *mut c_void, _flags: u32, _item: *mut c_void, hr: HRESULT, created: *mut c_void) -> HRESULT {
    let s = unsafe { &*(this as *const Sink) };
    s.hr_delete.set(Some(hr.0));
    if !created.is_null() {
        let item = unsafe { IShellItem::from_raw_borrowed(&created) }.cloned();
        *s.recycled.borrow_mut() = item;
    }
    S_OK
}
unsafe extern "system" fn pre_new(_t: *mut c_void, _f: u32, _d: *mut c_void, _n: PCWSTR) -> HRESULT {
    S_OK
}
unsafe extern "system" fn post_new(_t: *mut c_void, _f: u32, _d: *mut c_void, _n: PCWSTR, _tn: PCWSTR, _a: u32, _hr: HRESULT, _c: *mut c_void) -> HRESULT {
    S_OK
}
unsafe extern "system" fn update_progress(_t: *mut c_void, _a: u32, _b: u32) -> HRESULT {
    S_OK
}

static VTBL: IFileOperationProgressSink_Vtbl = IFileOperationProgressSink_Vtbl {
    base__: IUnknown_Vtbl {
        QueryInterface: query_interface,
        AddRef: add_ref,
        Release: release,
    },
    StartOperations: ok0,
    FinishOperations: finish,
    PreRenameItem: pre_rename,
    PostRenameItem: post_rename,
    PreMoveItem: pre_move,
    PostMoveItem: post_move,
    PreCopyItem: pre_move,
    PostCopyItem: post_move,
    PreDeleteItem: pre_delete,
    PostDeleteItem: post_delete,
    PreNewItem: pre_new,
    PostNewItem: post_new,
    UpdateProgress: update_progress,
    ResetTimer: ok0,
    PauseTimer: ok0,
    ResumeTimer: ok0,
};

// ---------------- 本体 ----------------

/// 1つのファイルをごみ箱へ移す。移せなければ、種別つきのエラー（ファイルはそのまま）
pub fn move_to_recycle_bin(path: &Path) -> Result<(), AppError> {
    on_sta_thread(path, false).map(|_| ())
}

/// テスト用：ごみ箱へ移したあと、**その項目だけ**をごみ箱から消す。ごみ箱の中にあった場所を返す（消せたかの確かめに使う）。
/// 画面からは呼べない（窓口が無い）。実機のテスト（tests\device_playback.rs）からも使うので、機能 `test-support` のときだけ公開する
/// （`cargo test --features test-support --test device_playback`）。配布物（機能なしのビルド）には入らない
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn move_to_recycle_bin_then_purge(path: &Path) -> Result<Option<std::path::PathBuf>, AppError> {
    on_sta_thread(path, true)
}

fn on_sta_thread(path: &Path, purge: bool) -> Result<Option<std::path::PathBuf>, AppError> {
    let p = path.to_path_buf();
    std::thread::Builder::new()
        .name("pl-recycle".into())
        .spawn(move || unsafe { run(&p, purge) })
        .map_err(|e| AppError::other(format!("recycle thread: {e}")))?
        .join()
        .unwrap_or_else(|_| Err(AppError::other("recycle thread panicked")))
}

struct ComGuard;
impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

unsafe fn run(path: &Path, purge: bool) -> Result<Option<std::path::PathBuf>, AppError> {
    let shown = path.display().to_string();
    let fail = |kind: ErrorKind, what: &str| AppError::new(kind, format!("{shown}: {what}"));
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    if hr.is_err() {
        return Err(fail(ErrorKind::Other, &format!("CoInitializeEx {hr:?}")));
    }
    let _com = ComGuard;
    // 確かめ（library::trash::plan）のあと・再生中の曲を止めている間に、差し替えられていないか。シェルを呼ぶ直前にもう一度
    check_regular_file(path)?;
    // シェルは `/` を区切りとして読まないので、Windows の形にそろえる
    let wide: Vec<u16> = path.to_string_lossy().replace('/', "\\").encode_utf16().chain(std::iter::once(0)).collect();
    let item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }
        .map_err(|e| fail(classify(e.code().0, false), &format!("SHCreateItemFromParsingName {:#010x}", e.code().0)))?;
    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL) }
        .map_err(|e| fail(ErrorKind::Other, &format!("FileOperation {e}")))?;
    unsafe { op.SetOperationFlags(FOF_NO_UI | FOF_ALLOWUNDO | FOFX_RECYCLEONDELETE | FOFX_EARLYFAILURE) }
        .map_err(|e| fail(ErrorKind::Other, &format!("SetOperationFlags {e}")))?;
    let raw = Box::into_raw(Box::new(Sink {
        vtbl: &VTBL,
        refs: AtomicU32::new(1),
        nuke_refused: Cell::new(false),
        not_file_refused: Cell::new(false),
        hr_delete: Cell::new(None),
        recycled: RefCell::new(None),
    }));
    // 参照を1つ持つ（落とすと Release で片付く）
    let sink = unsafe { IFileOperationProgressSink::from_raw(raw as *mut c_void) };
    let state = unsafe { &*(raw as *const Sink) };
    let cookie = unsafe { op.Advise(&sink) }.map_err(|e| fail(ErrorKind::Other, &format!("Advise {e}")))?;
    let performed = unsafe { op.DeleteItem(&item, None) }.and_then(|_| unsafe { op.PerformOperations() });
    let aborted = unsafe { op.GetAnyOperationsAborted() }.map(|b| b.as_bool()).unwrap_or(true);
    let _ = unsafe { op.Unadvise(cookie) };

    let nuke = state.nuke_refused.get();
    let not_file = state.not_file_refused.get();
    let hr_delete = state.hr_delete.get();
    let recycled = state.recycled.borrow_mut().take();
    drop(sink);

    if nuke {
        return Err(fail(ErrorKind::TrashUnavailable, "not recyclable; cancelled before deleting"));
    }
    if not_file {
        return Err(fail(ErrorKind::TrashNotAllowed, "not a regular file when deleting; cancelled"));
    }
    if let Some(h) = hr_delete.filter(|h| *h < 0) {
        return Err(fail(classify(h, false), &format!("delete {h:#010x}")));
    }
    if let Err(e) = performed {
        return Err(fail(classify(e.code().0, false), &format!("PerformOperations {:#010x}", e.code().0)));
    }
    if aborted {
        return Err(fail(ErrorKind::Other, "operation aborted"));
    }
    // 元の場所から消えたことを確かめる（消えていなければ、移せていない）
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err(fail(ErrorKind::Other, "still exists after the operation")),
    }
    if recycled.is_none() {
        // 元の場所からは消えたが、ごみ箱の項目が返らなかった（ふつうは起きない）。記録だけ残す
        tracing::warn!("recycle: the shell did not return the recycled item");
    }
    let in_bin = recycled.as_ref().and_then(|r| unsafe { display_path(r) });
    #[cfg(any(test, feature = "test-support"))]
    if purge {
        if let Some(r) = recycled {
            unsafe { purge_item(&r, path) }?;
        }
    }
    #[cfg(not(any(test, feature = "test-support")))]
    let _ = (purge, recycled);
    Ok(in_bin)
}

unsafe fn display_path(item: &IShellItem) -> Option<std::path::PathBuf> {
    let p = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.ok()?;
    let s = unsafe { p.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(p.0 as *const c_void)) };
    s.map(std::path::PathBuf::from)
}

/// ごみ箱の中の項目の場所か：`X:\$Recycle.Bin\<SID>\$R…` の形だけ（大文字小文字は問わない。`\\?\X:\` の形も受ける）。
/// テストの後片付けで消す前に確かめる（シェルが元の場所の項目を返すような取り違えがあっても、ごみ箱の外は消さない）
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn is_in_recycle_bin(p: &Path) -> bool {
    use std::path::{Component, Prefix};
    let c: Vec<Component> = p.components().collect();
    let [Component::Prefix(pre), Component::RootDir, Component::Normal(bin), Component::Normal(sid), Component::Normal(name)] = c.as_slice() else {
        return false;
    };
    let disk = matches!(pre.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_));
    let bin_ok = bin.to_str().is_some_and(|b| b.eq_ignore_ascii_case("$Recycle.Bin"));
    let sid_ok = sid.to_str().is_some_and(|s| s.len() > 2 && s.is_char_boundary(2) && s[..2].eq_ignore_ascii_case("S-"));
    let name_ok = name.to_str().is_some_and(|n| n.len() > 2 && n.is_char_boundary(2) && n[..2].eq_ignore_ascii_case("$R"));
    disk && bin_ok && sid_ok && name_ok
}

/// ごみ箱の中身（`$R…`）と対になる情報のファイル（`$I…`。元の場所と消した日時が入る）の場所
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn info_file_of(in_bin: &Path) -> Option<std::path::PathBuf> {
    let name = in_bin.file_name()?.to_str()?;
    let rest = name.strip_prefix("$R")?;
    Some(in_bin.with_file_name(format!("$I{rest}")))
}

/// テスト用：ごみ箱の中の項目を消す（ごみ箱から戻せなくなる。テストで作ったファイルにだけ使う）。
/// シェルに中身（`$R…`）を消させたあと、対になる `$I…` も消す（シェルは中身だけを消し、`$I…` が残るため。2026-11-13 実測）。
/// `$I…` は、中に書かれた元の場所が `original` と同じときだけ消す。
/// **消す前に、項目がごみ箱の中（`is_in_recycle_bin`）にあることを確かめる。**違えば何も消さずにエラー
#[cfg(any(test, feature = "test-support"))]
unsafe fn purge_item(item: &IShellItem, original: &Path) -> Result<(), AppError> {
    let in_bin = unsafe { display_path(item) }.ok_or_else(|| AppError::other("purge: no path of the recycled item"))?;
    if !is_in_recycle_bin(&in_bin) {
        return Err(AppError::other(format!("purge: {} is not in the recycle bin; nothing deleted", in_bin.display())));
    }
    unsafe { purge_shell(item) }?;
    let Some(info) = info_file_of(&in_bin) else { return Ok(()) };
    let bytes = match std::fs::read(&info) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(AppError::from_io(&e, &info)),
    };
    let utf16: Vec<u16> = bytes.get(24..).unwrap_or(&[]).chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let text = String::from_utf16_lossy(&utf16).to_lowercase();
    let want = original.to_string_lossy().replace('/', "\\").to_lowercase();
    if !text.contains(&want) {
        return Err(AppError::other(format!("purge: {} does not describe the test file", info.display())));
    }
    std::fs::remove_file(&info).map_err(|e| AppError::from_io(&e, &info))
}

#[cfg(any(test, feature = "test-support"))]
unsafe fn purge_shell(item: &IShellItem) -> Result<(), AppError> {
    let op: IFileOperation =
        unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL) }.map_err(|e| AppError::other(format!("FileOperation {e}")))?;
    unsafe { op.SetOperationFlags(FOF_NO_UI) }.map_err(|e| AppError::other(format!("SetOperationFlags {e}")))?;
    unsafe { op.DeleteItem(item, None) }
        .and_then(|_| unsafe { op.PerformOperations() })
        .map_err(|e| AppError::other(format!("purge {e}")))?;
    if unsafe { op.GetAnyOperationsAborted() }.map(|b| b.as_bool()).unwrap_or(true) {
        return Err(AppError::other("purge aborted"));
    }
    Ok(())
}

// テストは library::trash（一時フォルダの曲で、実際のごみ箱へ移して消す）と、ここの決まりの表
#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Shell::COPYENGINE_E_USER_CANCELLED;

    #[test]
    fn pre_delete_only_when_recycling() {
        assert!(pre_delete_allowed(TSF_DELETE_RECYCLE_IF_POSSIBLE));
        assert!(pre_delete_allowed(TSF_DELETE_RECYCLE_IF_POSSIBLE | 0x40));
        // ごみ箱へ入らない（完全に消える）：印が無い
        assert!(!pre_delete_allowed(0));
        assert!(!pre_delete_allowed(0x40 | 0x800));
    }

    /// テストの後片付けで消してよいのは、ごみ箱の中の項目（`X:\$Recycle.Bin\<SID>\$R…`）だけ
    #[test]
    fn purge_only_inside_recycle_bin() {
        for ok in [
            r"C:\$Recycle.Bin\S-1-5-21-1-2-3-1001\$RAB12CD.mp3",
            r"d:\$RECYCLE.BIN\s-1-5-21-1-2-3-1001\$rx.flac",
            r"\\?\C:\$Recycle.Bin\S-1-5-21-1-2-3-1001\$R0.ogg",
        ] {
            assert!(is_in_recycle_bin(Path::new(ok)), "{ok}");
        }
        for bad in [
            // 元の場所（取り違え）
            r"C:\Music\01 song.mp3",
            r"C:\Users\taro-test\Music\$R0.mp3",
            // ごみ箱そのもの・SID のフォルダ・対になる $I…
            r"C:\$Recycle.Bin",
            r"C:\$Recycle.Bin\S-1-5-21-1-2-3-1001",
            r"C:\$Recycle.Bin\S-1-5-21-1-2-3-1001\$IAB12CD.mp3",
            // ごみ箱の中のフォルダの、さらに中
            r"C:\$Recycle.Bin\S-1-5-21-1-2-3-1001\$RAB12CD\a.mp3",
            // ごみ箱に似た名前・途中にあるだけ・上へ戻る
            r"C:\$Recycle.Bin2\S-1-5-21\$R0.mp3",
            r"C:\Music\$Recycle.Bin\S-1-5-21\$R0.mp3",
            r"C:\$Recycle.Bin\..\Music\$R0.mp3",
            r"C:\$Recycle.Bin\S-1-5-21\..\$R0.mp3",
            // ドライブの無い形・ネットワーク上の場所
            r"$Recycle.Bin\S-1-5-21\$R0.mp3",
            r"\\server\share\$Recycle.Bin\S-1-5-21\$R0.mp3",
            "",
        ] {
            assert!(!is_in_recycle_bin(Path::new(bad)), "{bad}");
        }
    }

    /// 移す直前の確かめ：フォルダ・無いファイルはシェルを呼ぶ前に断る（ごみ箱へは何も入らない）
    #[test]
    fn refuses_non_files_right_before_moving() {
        let dir = crate::test_tmp().join(format!("plp-recycle-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("folder.mp3")).unwrap();
        std::fs::write(dir.join("folder.mp3").join("inner.mp3"), b"x").unwrap();
        let e = move_to_recycle_bin(&dir.join("folder.mp3")).unwrap_err();
        assert_eq!(e.kind, ErrorKind::TrashNotAllowed, "{e:?}");
        assert!(dir.join("folder.mp3").join("inner.mp3").exists());
        let e = move_to_recycle_bin(&dir.join("gone.mp3")).unwrap_err();
        assert_eq!(e.kind, ErrorKind::FileNotFound, "{e:?}");
        std::fs::write(dir.join("a.mp3"), b"x").unwrap();
        assert!(check_regular_file(&dir.join("a.mp3")).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hresults_map_to_kinds() {
        assert_eq!(classify(win32(32), false), ErrorKind::FileInUse);
        assert_eq!(classify(win32(33), false), ErrorKind::FileInUse);
        assert_eq!(classify(COPYENGINE_E_SHARING_VIOLATION_SRC.0, false), ErrorKind::FileInUse);
        assert_eq!(classify(win32(5), false), ErrorKind::AccessDenied);
        assert_eq!(classify(COPYENGINE_E_ACCESS_DENIED_SRC.0, false), ErrorKind::AccessDenied);
        assert_eq!(classify(win32(2), false), ErrorKind::FileNotFound);
        assert_eq!(classify(win32(3), false), ErrorKind::FileNotFound);
        assert_eq!(classify(COPYENGINE_E_RECYCLE_BIN_NOT_FOUND.0, false), ErrorKind::TrashUnavailable);
        // 取りやめた理由が「完全に消える」なら、返ってきた値にかかわらず「ごみ箱の無い場所」
        assert_eq!(classify(E_ABORT.0, true), ErrorKind::TrashUnavailable);
        assert_eq!(classify(COPYENGINE_E_USER_CANCELLED.0, true), ErrorKind::TrashUnavailable);
        assert_eq!(classify(E_ABORT.0, false), ErrorKind::Other);
    }
}
