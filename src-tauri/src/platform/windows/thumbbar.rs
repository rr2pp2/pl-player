//! タスクバーのサムネイルに出るボタン（前・再生/一時停止・次）。
//!
//! `ITaskbarList3::ThumbBarAddButtons` は後から足し引きできないので、最初に3つとも定義する。
//! 再生と一時停止は、絵柄の差し替え（ThumbBarUpdateButtons）で切り替える。
//! 押されると WM_COMMAND（THBN_CLICKED）が来るので、ウィンドウのメッセージを横取り（サブクラス化）して受ける。
//! COM のオブジェクトはウィンドウのスレッドでしか触らない：再生側からは独自のメッセージを送るだけ。

use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

use tauri::AppHandle;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::{
    DefSubclassProc, ITaskbarList3, SetWindowSubclass, TaskbarList, THBF_ENABLED, THBN_CLICKED, THB_FLAGS,
    THB_ICON, THB_TOOLTIP, THUMBBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, PostMessageW, RegisterWindowMessageW, HICON, ICONINFO, WM_APP, WM_COMMAND,
};

use crate::playback::{PlaybackListener, Snapshot, Status};

const ID_PREV: u32 = 1;
const ID_TOGGLE: u32 = 2;
const ID_NEXT: u32 = 3;
const WM_PL_PLAYING: u32 = WM_APP + 0x51;
const SUBCLASS_ID: usize = 0x504C;

static APP: OnceLock<AppHandle> = OnceLock::new();
static HWND_RAW: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct UiState {
    taskbar: ITaskbarList3,
    icons: [HICON; 4], // prev, play, pause, next
    playing: bool,
    added: bool,
}

thread_local! {
    static UI: RefCell<Option<UiState>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy)]
enum Shape {
    Prev,
    Play,
    Pause,
    Next,
}

/// 32x32 の白い記号を描いて、アイコンにする（外部の画像ファイルを持たない）
fn make_icon(shape: Shape) -> Option<HICON> {
    const N: i32 = 32;
    let mut px = vec![0u32; (N * N) as usize];
    let mut set = |x: i32, y: i32| {
        if (0..N).contains(&x) && (0..N).contains(&y) {
            px[(y * N + x) as usize] = 0xFFFF_FFFF;
        }
    };
    // 右向きの三角形（x0..x1、中心 16、高さ 2*h）
    let mut tri = |x0: i32, x1: i32, h: i32, right: bool| {
        for x in x0..=x1 {
            let t = if right { x - x0 } else { x1 - x };
            let half = h - (t * h) / (x1 - x0).max(1);
            for y in (16 - half)..=(16 + half) {
                set(x, y);
            }
        }
    };
    match shape {
        Shape::Play => tri(10, 24, 9, true),
        Shape::Next => {
            tri(7, 19, 8, true);
            for x in 20..=23 {
                for y in 8..=24 {
                    set(x, y);
                }
            }
        }
        Shape::Prev => {
            tri(12, 24, 8, false);
            for x in 8..=11 {
                for y in 8..=24 {
                    set(x, y);
                }
            }
        }
        Shape::Pause => {
            for x in (9..=13).chain(19..=23) {
                for y in 8..=24 {
                    set(x, y);
                }
            }
        }
    }
    unsafe {
        let color = CreateBitmap(N, N, 1, 32, Some(px.as_ptr() as *const _));
        let mask_bits = vec![0u8; (N * N / 8) as usize];
        let mask = CreateBitmap(N, N, 1, 1, Some(mask_bits.as_ptr() as *const _));
        let info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info).ok();
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

fn tip(s: &str) -> [u16; 260] {
    let mut out = [0u16; 260];
    for (i, c) in s.encode_utf16().take(259).enumerate() {
        out[i] = c;
    }
    out
}

fn buttons(ui: &UiState) -> [THUMBBUTTON; 3] {
    let mk = |id: u32, icon: HICON, t: &str| THUMBBUTTON {
        dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS,
        iId: id,
        iBitmap: 0,
        hIcon: icon,
        szTip: tip(t),
        dwFlags: THBF_ENABLED,
    };
    [
        mk(ID_PREV, ui.icons[0], "前の曲"),
        if ui.playing {
            mk(ID_TOGGLE, ui.icons[2], "一時停止")
        } else {
            mk(ID_TOGGLE, ui.icons[1], "再生")
        },
        mk(ID_NEXT, ui.icons[3], "次の曲"),
    ]
}

fn add_buttons(hwnd: HWND) {
    UI.with(|u| {
        if let Some(ui) = u.borrow_mut().as_mut() {
            let b = buttons(ui);
            match unsafe { ui.taskbar.ThumbBarAddButtons(hwnd, &b) } {
                Ok(()) => ui.added = true,
                Err(e) => tracing::info!("ThumbBarAddButtons: {e}"),
            }
        }
    });
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_COMMAND && ((wparam.0 >> 16) & 0xFFFF) as u32 == THBN_CLICKED {
        let id = (wparam.0 & 0xFFFF) as u32;
        if let Some(app) = APP.get() {
            let app = app.clone();
            let action = match id {
                ID_PREV => "previous",
                ID_TOGGLE => "playPause",
                ID_NEXT => "next",
                _ => "",
            };
            // ウィンドウのスレッドを待たせない
            std::thread::spawn(move || super::hotkeys::dispatch(&app, action));
        }
        return LRESULT(0);
    }
    if msg == WM_PL_PLAYING {
        let playing = wparam.0 != 0;
        UI.with(|u| {
            if let Some(ui) = u.borrow_mut().as_mut() {
                ui.playing = playing;
                if !ui.added {
                    return;
                }
                let b = buttons(ui);
                let _ = unsafe { ui.taskbar.ThumbBarUpdateButtons(hwnd, &b) };
            }
        });
        return LRESULT(0);
    }
    let tc = TASKBAR_CREATED.load(Ordering::Relaxed);
    if tc != 0 && msg == tc {
        // エクスプローラーが再起動した・タスクバーのボタンができた：ボタンを付け直す
        UI.with(|u| {
            if let Some(ui) = u.borrow_mut().as_mut() {
                ui.added = false;
            }
        });
        add_buttons(hwnd);
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// ウィンドウのスレッド（main）で呼ぶ
pub fn attach(app: &AppHandle, hwnd: HWND) -> Option<Arc<dyn PlaybackListener>> {
    let _ = APP.set(app.clone());
    HWND_RAW.store(hwnd.0 as isize, Ordering::Relaxed);
    let taskbar: ITaskbarList3 = match unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER) } {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("ITaskbarList3: {e}");
            return None;
        }
    };
    if unsafe { taskbar.HrInit() }.is_err() {
        return None;
    }
    let icons = [
        make_icon(Shape::Prev)?,
        make_icon(Shape::Play)?,
        make_icon(Shape::Pause)?,
        make_icon(Shape::Next)?,
    ];
    UI.with(|u| {
        *u.borrow_mut() = Some(UiState {
            taskbar,
            icons,
            playing: false,
            added: false,
        })
    });
    unsafe {
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarButtonCreated")), Ordering::Relaxed);
        let _ = SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0);
    }
    // タスクバーのボタンがもうあれば、ここで付く（まだなら TaskbarButtonCreated で付く）
    add_buttons(hwnd);
    Some(Arc::new(ThumbbarListener))
}

struct ThumbbarListener;

impl PlaybackListener for ThumbbarListener {
    fn on_snapshot(&self, s: &Snapshot) {
        let raw = HWND_RAW.load(Ordering::Relaxed);
        if raw == 0 {
            return;
        }
        let playing = s.status == Status::Playing;
        unsafe {
            let _ = PostMessageW(
                Some(HWND(raw as *mut _)),
                WM_PL_PLAYING,
                WPARAM(playing as usize),
                LPARAM(0),
            );
        }
    }
}
