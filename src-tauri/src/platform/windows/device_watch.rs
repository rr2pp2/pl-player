//! 出力デバイスの抜き差しの通知（IMMNotificationClient）。
//!
//! 公式の制約：コールバックの中では待たない・登録解除しない。だから、知らせを通り道へ流すだけにする。
//!
//! COM のインターフェースを実装するマクロ（`#[implement]`）は `windows-core` を直接の依存として要求する。
//! 承認済みの依存に `windows-core` は無いので、関数表（vtable）を手で組む。

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::{GUID, HRESULT, PCWSTR};
use windows::Win32::Foundation::{E_NOINTERFACE, E_POINTER, PROPERTYKEY, S_OK};
use windows::Win32::Media::Audio::{
    eRender, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Vtbl,
    MMDeviceEnumerator, DEVICE_STATE,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::core::{IUnknown, IUnknown_Vtbl, Interface};

#[repr(C)]
struct Client {
    vtbl: *const IMMNotificationClient_Vtbl,
    refs: AtomicU32,
    on_change: Box<dyn Fn() + Send + Sync>,
}

unsafe extern "system" fn query_interface(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() || iid.is_null() {
        return E_POINTER;
    }
    let iid = unsafe { *iid };
    if iid == IUnknown::IID || iid == IMMNotificationClient::IID {
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
    let c = unsafe { &*(this as *const Client) };
    c.refs.fetch_add(1, Ordering::SeqCst) + 1
}

unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    let c = unsafe { &*(this as *const Client) };
    let n = c.refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(unsafe { Box::from_raw(this as *mut Client) });
    }
    n
}

fn fire(this: *mut c_void) {
    let c = unsafe { &*(this as *const Client) };
    (c.on_change)();
}

unsafe extern "system" fn on_state_changed(this: *mut c_void, _id: PCWSTR, _s: DEVICE_STATE) -> HRESULT {
    fire(this);
    S_OK
}
unsafe extern "system" fn on_added(this: *mut c_void, _id: PCWSTR) -> HRESULT {
    fire(this);
    S_OK
}
unsafe extern "system" fn on_removed(this: *mut c_void, _id: PCWSTR) -> HRESULT {
    fire(this);
    S_OK
}
unsafe extern "system" fn on_default_changed(this: *mut c_void, flow: EDataFlow, _r: ERole, _id: PCWSTR) -> HRESULT {
    if flow == eRender {
        fire(this);
    }
    S_OK
}
unsafe extern "system" fn on_property_changed(_this: *mut c_void, _id: PCWSTR, _k: PROPERTYKEY) -> HRESULT {
    S_OK
}

static VTBL: IMMNotificationClient_Vtbl = IMMNotificationClient_Vtbl {
    base__: IUnknown_Vtbl {
        QueryInterface: query_interface,
        AddRef: add_ref,
        Release: release,
    },
    OnDeviceStateChanged: on_state_changed,
    OnDeviceAdded: on_added,
    OnDeviceRemoved: on_removed,
    OnDefaultDeviceChanged: on_default_changed,
    OnPropertyValueChanged: on_property_changed,
};

/// 通知の登録を、専用のスレッドで行い、アプリが終わるまで保つ
pub fn start(on_change: impl Fn() + Send + Sync + 'static) {
    std::thread::Builder::new()
        .name("pl-device-watch".into())
        .spawn(move || unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator: IMMDeviceEnumerator = match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("IMMDeviceEnumerator: {e}");
                    return;
                }
            };
            let raw = Box::into_raw(Box::new(Client {
                vtbl: &VTBL,
                refs: AtomicU32::new(1),
                on_change: Box::new(on_change),
            }));
            let client = IMMNotificationClient::from_raw(raw as *mut c_void);
            if let Err(e) = enumerator.RegisterEndpointNotificationCallback(&client) {
                tracing::warn!("RegisterEndpointNotificationCallback: {e}");
                return;
            }
            // 登録したまま眠る（MTA なのでメッセージを回す必要はない）
            loop {
                std::thread::park();
            }
        })
        .ok();
}
