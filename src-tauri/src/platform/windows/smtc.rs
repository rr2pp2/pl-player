//! SMTC（System Media Transport Controls）。メディアキーと、Windows のメディア操作（クイック設定・音量の表示など）。
//!
//! 自前のエンジンで鳴らしているので、自動の連携は使えない。`ISystemMediaTransportControlsInterop::GetForWindow`
//! でウィンドウに結びつけて手動で登録する（技術調査 5章）。
//! 表示の更新は専用のスレッドで行う（画像の読み込みで待つため。再生側を待たせない）。

use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;

use tauri::AppHandle;
use windows::core::HSTRING;
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, PlaybackPositionChangeRequestedEventArgs,
    SystemMediaTransportControls, SystemMediaTransportControlsButton,
    SystemMediaTransportControlsButtonPressedEventArgs, SystemMediaTransportControlsTimelineProperties,
};
use windows::Storage::StorageFile;
use windows::Storage::Streams::RandomAccessStreamReference;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;

use crate::playback::{PlaybackListener, Snapshot, Status};

enum Msg {
    Snapshot(Box<Snapshot>),
    Position(u64, u64),
}

pub struct SmtcListener {
    tx: Sender<Msg>,
}

impl PlaybackListener for SmtcListener {
    fn on_snapshot(&self, s: &Snapshot) {
        let _ = self.tx.send(Msg::Snapshot(Box::new(s.clone())));
    }
    fn on_position(&self, p: u64, d: u64) {
        let _ = self.tx.send(Msg::Position(p, d));
    }
}

fn ts(ms: u64) -> TimeSpan {
    TimeSpan {
        Duration: (ms as i64) * 10_000,
    }
}

/// ウィンドウに SMTC を結びつけ、ボタンの操作を受ける。失敗したら None（メディアキーが効かないだけで、アプリは動く）
pub fn attach(app: &AppHandle, hwnd: HWND) -> Option<Arc<dyn PlaybackListener>> {
    let smtc: SystemMediaTransportControls = (|| -> windows::core::Result<SystemMediaTransportControls> {
        let interop = windows::core::factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()?;
        let s: SystemMediaTransportControls = unsafe { interop.GetForWindow(hwnd)? };
        s.SetIsEnabled(true)?;
        s.SetIsPlayEnabled(true)?;
        s.SetIsPauseEnabled(true)?;
        s.SetIsNextEnabled(true)?;
        s.SetIsPreviousEnabled(true)?;
        s.SetIsStopEnabled(false)?;
        s.SetPlaybackStatus(MediaPlaybackStatus::Closed)?;
        Ok(s)
    })()
    .map_err(|e| tracing::warn!("SMTC attach failed: {e}"))
    .ok()?;

    let app_b = app.clone();
    let _ = smtc.ButtonPressed(&TypedEventHandler::<
        SystemMediaTransportControls,
        SystemMediaTransportControlsButtonPressedEventArgs,
    >::new(move |_, args| {
        if let Ok(a) = args.ok() {
            let action = match a.Button()? {
                SystemMediaTransportControlsButton::Play => "play",
                SystemMediaTransportControlsButton::Pause => "pause",
                SystemMediaTransportControlsButton::Next => "next",
                SystemMediaTransportControlsButton::Previous => "previous",
                _ => "",
            };
            let app = app_b.clone();
            // ボタンのコールバックの中では待たない
            std::thread::spawn(move || {
                use tauri::Manager;
                let state = app.state::<crate::commands::AppState>();
                match action {
                    "play" => state.controller.play(),
                    "pause" => state.controller.pause(),
                    "next" => state.controller.next(),
                    "previous" => state.controller.previous(),
                    _ => {}
                }
            });
        }
        Ok(())
    }));
    let app_s = app.clone();
    let _ = smtc.PlaybackPositionChangeRequested(&TypedEventHandler::<
        SystemMediaTransportControls,
        PlaybackPositionChangeRequestedEventArgs,
    >::new(move |_, args| {
        if let Ok(a) = args.ok() {
            let ms = (a.RequestedPlaybackPosition()?.Duration / 10_000).max(0) as u64;
            let app = app_s.clone();
            std::thread::spawn(move || {
                use tauri::Manager;
                app.state::<crate::commands::AppState>().controller.seek(ms);
            });
        }
        Ok(())
    }));

    // 表示の更新は専用スレッドで
    let (tx, rx) = channel::<Msg>();
    let smtc_t = smtc.clone();
    std::thread::Builder::new()
        .name("pl-smtc".into())
        .spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            // SMTC は agile なオブジェクトなので、別スレッドから使える
            let smtc = smtc_t;
            let mut last_track: Option<i64> = None;
            let mut last_art: Option<String> = None;
            let mut duration = 0u64;
            while let Ok(m) = rx.recv() {
                // 溜まっていれば最後のものだけを使う
                let mut latest_snap: Option<Box<Snapshot>> = None;
                let mut latest_pos: Option<(u64, u64)> = None;
                let mut handle = |m: Msg| match m {
                    Msg::Snapshot(s) => latest_snap = Some(s),
                    Msg::Position(p, d) => latest_pos = Some((p, d)),
                };
                handle(m);
                while let Ok(m) = rx.try_recv() {
                    handle(m);
                }
                if let Some(s) = latest_snap {
                    duration = s.duration_ms;
                    let status = match (s.status, &s.track) {
                        (_, None) => MediaPlaybackStatus::Closed,
                        (Status::Playing, _) => MediaPlaybackStatus::Playing,
                        (Status::Paused, _) => MediaPlaybackStatus::Paused,
                        (Status::Stopped, _) => MediaPlaybackStatus::Stopped,
                    };
                    let _ = smtc.SetPlaybackStatus(status);
                    let id = s.track.as_ref().map(|t| t.id);
                    let art = s.track.as_ref().and_then(|t| t.art_path.clone());
                    if id != last_track || art != last_art {
                        last_track = id;
                        last_art = art.clone();
                        if let Ok(up) = smtc.DisplayUpdater() {
                            let _ = up.ClearAll();
                            if let Some(t) = &s.track {
                                let _ = up.SetType(MediaPlaybackType::Music);
                                if let Ok(mp) = up.MusicProperties() {
                                    let _ = mp.SetTitle(&HSTRING::from(t.title.as_str()));
                                    let _ = mp.SetArtist(&HSTRING::from(t.artist.as_str()));
                                    let _ = mp.SetAlbumTitle(&HSTRING::from(t.album.as_str()));
                                }
                                if let Some(p) = art {
                                    let r = StorageFile::GetFileFromPathAsync(&HSTRING::from(p.as_str()))
                                        .and_then(|op| op.join())
                                        .and_then(|f| RandomAccessStreamReference::CreateFromFile(&f));
                                    if let Ok(r) = r {
                                        let _ = up.SetThumbnail(&r);
                                    }
                                }
                            }
                            let _ = up.Update();
                        }
                    }
                    latest_pos.get_or_insert((s.position_ms, s.duration_ms));
                }
                if let Some((p, d)) = latest_pos {
                    let d = if d > 0 { d } else { duration };
                    if let Ok(tl) = SystemMediaTransportControlsTimelineProperties::new() {
                        let _ = tl.SetStartTime(ts(0));
                        let _ = tl.SetMinSeekTime(ts(0));
                        let _ = tl.SetEndTime(ts(d));
                        let _ = tl.SetMaxSeekTime(ts(d));
                        let _ = tl.SetPosition(ts(p.min(d.max(p))));
                        let _ = smtc.UpdateTimelineProperties(&tl);
                    }
                }
            }
        })
        .ok()?;
    Some(Arc::new(SmtcListener { tx }))
}
