//! グローバルホットキー（アプリがバックグラウンドでも効く）。Tauri 公式プラグインで登録する。
//!
//! 既定では1つも登録しない（Space などをグローバルに取ると他のアプリが壊れるため）。
//! 他のアプリが使っているキーは登録に失敗するので、その結果を画面へ返して「競合」として知らせる。
//!
//! ## スレッドの決まり（v3.8.1。設定の保存とキーを押す瞬間が重なると、アプリが止まった形の直し）
//!
//! プラグイン（tauri-plugin-global-shortcut 2.4.0）は、登録の一覧を1つの鍵（`shortcuts` の Mutex）で守っている。
//! - `unregister_all`（プラグインの lib.rs 234〜241行）は、その鍵を取ったまま、メインのスレッドで外し終わるのを待つ
//!   （`run_main_thread!`、85〜96行。`run_on_main_thread` に仕事を渡して `rx.recv()` で待つ）
//! - キーが押されると、メインのスレッドの窓の処理（global-hotkey 0.8.0 の platform_impl/windows/mod.rs 146〜156行。
//!   その窓はプラグインの setup＝メインのスレッドで作る）から、プラグインの受け口（lib.rs 477〜486行）が同じ鍵を取り、
//!   鍵を持ったまま、ここで登録した関数を呼ぶ
//!
//! 設定の保存（`settings_set`・`settings_import`・`hotkeys_status` は async のコマンドなので、メインでないスレッドで走る）が
//! 鍵を取ってメインのスレッドを待つ間に、メインのスレッドがキーの知らせを先に処理すると、鍵を待って止まり、
//! 頼まれた仕事にたどり着けない。互いに待って、どちらも動かなくなる。だから、次の3つを守る。
//!
//! 1. **プラグインの登録・解除を呼ぶのは、メインのスレッドだけ。**`apply` はどのスレッドから呼ばれても、
//!    登録し直しをまるごとメインのスレッドへ渡して結果を待つ（`run_on_main`）。メインのスレッドの上では、
//!    プラグインの「メインのスレッドで行う」頼みがその場で走る（tauri-runtime-wry 2.12.1 の lib.rs 263〜273行）ので、
//!    鍵を持ったまま別のスレッドを待つことが無くなる。キーの知らせも同じメインのスレッドで処理されるので、
//!    登録し直しの途中に割り込めない
//! 2. **押されたときの処理は、専用のスレッド（pl-hotkeys）へ渡すだけ。**受け口の中（プラグインの鍵を持ったメインのスレッド）で、
//!    司令塔の鍵を取らない・登録し直しを呼ばない（呼ぶと、同じスレッドが同じ鍵をもう一度取って止まる）。
//!    離したときの知らせ（global-hotkey が1回ごとに作るスレッドから来る。mod.rs 157〜170行）は何もしない
//! 3. **`apply` を呼ぶ側は、鍵を持っていてはいけない。**メインのスレッドを待つ間に、メインのスレッドがその鍵を欲しがると止まる
//!
//! この作りは、上の版の中身を読んで決めた。版を上げたら読み直す（`tests::pinned_versions` が落ちて知らせる）。

use std::collections::BTreeMap;
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::commands::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    pub action: String,
    pub accelerator: String,
    pub registered: bool,
    pub error: Option<String>,
}

static LAST: Mutex<Vec<HotkeyStatus>> = Mutex::new(Vec::new());

/// 動作の名前 → 再生の司令塔の操作
pub fn dispatch(app: &AppHandle, action: &str) {
    let state = app.state::<AppState>();
    let c = &state.controller;
    match action {
        "playPause" => c.toggle(),
        "next" => c.next(),
        "previous" => c.previous(),
        "seekForward" => c.seek_relative(5_000),
        "seekBackward" => c.seek_relative(-5_000),
        "volumeUp" => c.volume_step(0.05),
        "volumeDown" => c.volume_step(-0.05),
        "mute" => c.toggle_mute(),
        "shuffle" => c.toggle_shuffle(),
        "repeat" => c.cycle_repeat(),
        // v3.8.0（13.5 の11）：詳細表示の切替は Rust（司令塔）が持つので、窓が隠れていてもグローバルにできる。
        // 画面へは pl://outputs で知らせる。出力 1〜3 のオン/オフ（空の枠・切断中の枠は何もしない）
        "toggleDetail" => c.toggle_detail(),
        "toggleOutput1" => c.toggle_output(0),
        "toggleOutput2" => c.toggle_output(1),
        "toggleOutput3" => c.toggle_output(2),
        _ => {}
    }
}

/// 登録し直す（全部外してから、設定にあるものを1つずつ）。どのスレッドから呼んでもよい（頭の決まり1）。
/// 呼ぶ側は鍵を持っていてはいけない（決まり3）
pub fn apply(app: &AppHandle, map: &BTreeMap<String, String>) -> Vec<HotkeyStatus> {
    let on_main = app.clone();
    let map_on_main = map.clone();
    let done = run_on_main(
        |task| app.run_on_main_thread(task).is_ok(),
        move || {
            let out = register_on_main(&on_main, &map_on_main);
            // 結果の控えもメインのスレッドで書く（登録し直しが続いても、控えは最後に登録したものと同じ順で残る）
            *LAST.lock().unwrap_or_else(|p| p.into_inner()) = out.clone();
            out
        },
    );
    done.unwrap_or_else(|| {
        // 終了の途中などで、メインのスレッドへ渡せなかった（何も登録していない）
        tracing::warn!("hotkeys: could not run on the main thread");
        map.iter()
            .filter(|(_, a)| !a.trim().is_empty())
            .map(|(action, accel)| HotkeyStatus {
                action: action.clone(),
                accelerator: accel.trim().to_string(),
                registered: false,
                error: Some("main thread unavailable".into()),
            })
            .collect()
    })
}

/// 登録し直しの中身。**メインのスレッドでだけ呼ぶ**（`apply` の中から）。プラグインを呼ぶのはここだけ
fn register_on_main(app: &AppHandle, map: &BTreeMap<String, String>) -> Vec<HotkeyStatus> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut out = vec![];
    // 同じキーを2つの動作に割り当てていないか
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for (action, accel) in map {
        let accel = accel.trim().to_string();
        if accel.is_empty() {
            continue;
        }
        let norm = accel.to_ascii_uppercase();
        if let Some(other) = seen.get(&norm) {
            out.push(HotkeyStatus {
                action: action.clone(),
                accelerator: accel,
                registered: false,
                error: Some(format!("duplicate:{other}")),
            });
            continue;
        }
        let act = action.clone();
        // 受け口はプラグインの鍵を持ったメインのスレッドで呼ばれる。渡すだけで、待たない（頭の決まり2）
        let r = gs.on_shortcut(accel.as_str(), move |app, _sc, event| {
            if event.state() == ShortcutState::Pressed {
                pressed(app, &act);
            }
        });
        match r {
            Ok(()) => {
                seen.insert(norm, action.clone());
                out.push(HotkeyStatus {
                    action: action.clone(),
                    accelerator: accel,
                    registered: true,
                    error: None,
                });
            }
            Err(e) => {
                tracing::info!("hotkey {accel} for {action} failed: {e}");
                out.push(HotkeyStatus {
                    action: action.clone(),
                    accelerator: accel,
                    registered: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }
    out
}

pub fn status(app: &AppHandle, map: &BTreeMap<String, String>) -> Vec<HotkeyStatus> {
    let last = LAST.lock().unwrap_or_else(|p| p.into_inner()).clone();
    if last.is_empty() && !map.is_empty() {
        return apply(app, map);
    }
    last
}

/// 押された動作を、専用のスレッド（pl-hotkeys）へ渡す。待たない。押された順に1つずつ司令塔へ渡る
fn pressed(app: &AppHandle, action: &str) {
    static TX: OnceLock<Sender<(AppHandle, String)>> = OnceLock::new();
    let tx = TX.get_or_init(|| worker("pl-hotkeys", |(app, action): (AppHandle, String)| dispatch(&app, &action)));
    let _ = tx.send((app.clone(), action.to_string()));
}

/// 受け取ったものを、1本の専用スレッドで届いた順に処理する。送る側は待たない（チャネルに上限が無い）。
/// 1つが panic しても、次からも処理する（1回の失敗で、以後のホットキーが効かなくならないように）
fn worker<T: Send + 'static>(name: &str, f: impl Fn(T) + Send + 'static) -> Sender<T> {
    let (tx, rx) = mpsc::channel::<T>();
    let r = std::thread::Builder::new().name(name.into()).spawn(move || {
        while let Ok(x) = rx.recv() {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(x))).is_err() {
                tracing::warn!("hotkey action panicked");
            }
        }
    });
    if let Err(e) = r {
        tracing::warn!("hotkey worker could not start: {e}");
    }
    tx
}

/// `f` をメインのスレッドで走らせて、結果を待つ。`post` はメインのスレッドへ仕事を渡す（`AppHandle::run_on_main_thread`）。
/// メインのスレッドの上で呼んだときは、`post` がその場で走らせる（tauri-runtime-wry 2.12.1 の lib.rs 263〜273行）ので、
/// 自分を待って止まることは無い。渡せなかった・走らずに捨てられた（終了の途中）ときは None
fn run_on_main<T: Send + 'static>(
    post: impl FnOnce(Box<dyn FnOnce() + Send>) -> bool,
    f: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    let task: Box<dyn FnOnce() + Send> = Box::new(move || {
        let _ = tx.send(f());
    });
    if !post(task) {
        return None;
    }
    rx.recv().ok()
}

#[cfg(test)]
mod tests {
    //! 実機のキーは押さない（別の窓へキーを送ることもしない）。プラグインを本当に動かすと PC 全体のホットキーを取るので、
    //! プラグインとメインのスレッドの作りを、読んだ行のとおりに小さく写した見立てで、順番の決まりを確かめる。
    //! 本物のコードが見立てと同じ決まりで書かれていることは、ソースを読んで確かめる（`plugin_is_called_only_on_the_main_thread`）

    use super::*;
    use std::sync::Arc;
    use std::thread::ThreadId;
    use std::time::{Duration, Instant};

    type Task = Box<dyn FnOnce() + Send>;

    /// メインのスレッドの見立て。届いた仕事を1つずつ順に走らせる（Tauri のイベントループ）。
    /// 渡し口がみな捨てられると終わる（見立ての仕事は全部時間を切ってあるので、残っても止まったままにはならない）
    struct FakeMain {
        poster: Poster,
    }

    /// 仕事の渡し方。メインのスレッドの上ならその場で走らせ、ほかのスレッドなら列に積む
    /// （tauri-runtime-wry 2.12.1 の lib.rs 263〜279行 `send_user_message` と同じ分け方）
    #[derive(Clone)]
    struct Poster {
        tx: Sender<Task>,
        main: ThreadId,
    }

    impl Poster {
        fn post(&self, t: Task) -> bool {
            if std::thread::current().id() == self.main {
                t();
                true
            } else {
                self.tx.send(t).is_ok()
            }
        }
    }

    impl FakeMain {
        fn start() -> Self {
            let (tx, rx) = mpsc::channel::<Task>();
            let join = std::thread::spawn(move || {
                while let Ok(t) = rx.recv() {
                    t();
                }
            });
            Self { poster: Poster { tx, main: join.thread().id() } }
        }
    }

    /// プラグインの見立て（tauri-plugin-global-shortcut 2.4.0 の lib.rs）。待つ所には時間を切ってある（止まる代わりに Err）
    struct FakePlugin {
        shortcuts: Arc<Mutex<Vec<u32>>>,
        poster: Poster,
    }

    impl FakePlugin {
        /// lib.rs 234〜241行：鍵を取ったまま、メインのスレッドの仕事を待つ。`locked` は鍵を取った直後に呼ぶ（順番を作るため）
        fn unregister_all(&self, wait: Duration, locked: impl FnOnce()) -> Result<(), &'static str> {
            let mut list = self.shortcuts.lock().unwrap();
            let old = std::mem::take(&mut *list);
            locked();
            let (tx, rx) = mpsc::channel();
            self.poster.post(Box::new(move || {
                let _ = tx.send(old.len());
            }));
            rx.recv_timeout(wait).map(|_| ()).map_err(|_| "waited for the main thread while holding the lock")
        }

        /// lib.rs 99〜112行：メインのスレッドで登録し終えてから、鍵を取って一覧に入れる
        fn register(&self, id: u32, wait: Duration) -> Result<(), &'static str> {
            let (tx, rx) = mpsc::channel();
            self.poster.post(Box::new(move || {
                let _ = tx.send(());
            }));
            rx.recv_timeout(wait).map_err(|_| "waited for the main thread")?;
            self.shortcuts.lock().unwrap().push(id);
            Ok(())
        }

        /// lib.rs 477〜486行（メインのスレッドの窓の処理から呼ばれる）：鍵を取ったまま受け口を呼ぶ。
        /// 鍵を `limit` のあいだ取れなければ false（本物はそこで止まったまま戻らない）
        fn key_pressed(&self, limit: Duration, handler: impl FnOnce()) -> bool {
            let t0 = Instant::now();
            loop {
                if let Ok(_list) = self.shortcuts.try_lock() {
                    handler();
                    return true;
                }
                if t0.elapsed() > limit {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    /// 前の作り（設定の保存のスレッドから、プラグインを直に呼ぶ）では、キーの知らせが頼みより先に列にあると、
    /// 保存のスレッドは鍵を持ったままメインのスレッドを待ち、メインのスレッドは鍵を待つ。止まった形がこの順で起きることを確かめる
    #[test]
    fn old_order_waits_for_each_other() {
        let main = FakeMain::start();
        let plugin = Arc::new(FakePlugin { shortcuts: Arc::new(Mutex::new(vec![1, 2])), poster: main.poster.clone() });
        // メインのスレッドを一度止めて、列の順番を作る：[止め] → [キーの知らせ] → [保存のスレッドの頼み]
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        main.poster.post(Box::new(move || {
            let _ = gate_rx.recv_timeout(Duration::from_secs(5));
        }));
        let (key_tx, key_rx) = mpsc::channel();
        let p = plugin.clone();
        main.poster.post(Box::new(move || {
            let _ = key_tx.send(p.key_pressed(Duration::from_secs(3), || {}));
        }));
        let (locked_tx, locked_rx) = mpsc::channel();
        let p = plugin.clone();
        let saver = std::thread::spawn(move || {
            p.unregister_all(Duration::from_millis(800), || {
                let _ = locked_tx.send(());
            })
        });
        locked_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let _ = gate_tx.send(());
        // 保存のスレッドは、鍵を持ったままメインのスレッドを待ち続けた（本物では戻らない。見立ては 0.8 秒で切った）
        assert!(saver.join().unwrap().is_err());
        // メインのスレッドは、保存のスレッドが鍵を離すまで、キーの知らせの中で待っていた（見立てが時間を切ったので、そのあと取れた）
        assert!(key_rx.recv_timeout(Duration::from_secs(5)).unwrap());
    }

    /// 今の作り：保存のスレッドは登録し直しをまるごとメインのスレッドへ渡す（`run_on_main`）。
    /// キーの知らせが先に列にあっても、メインのスレッドは知らせを処理してから登録し直しをし、どちらも止まらない
    #[test]
    fn new_order_finishes_even_if_a_key_comes_first() {
        let main = FakeMain::start();
        let plugin = Arc::new(FakePlugin { shortcuts: Arc::new(Mutex::new(vec![1, 2])), poster: main.poster.clone() });
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        main.poster.post(Box::new(move || {
            let _ = gate_rx.recv_timeout(Duration::from_secs(5));
        }));
        let (key_tx, key_rx) = mpsc::channel();
        let p = plugin.clone();
        main.poster.post(Box::new(move || {
            let _ = key_tx.send(p.key_pressed(Duration::from_millis(800), || {}));
        }));
        let (done_tx, done_rx) = mpsc::channel();
        let p = plugin.clone();
        let poster = main.poster.clone();
        let main_id = main.poster.main;
        std::thread::spawn(move || {
            let r = run_on_main(
                |t| poster.post(t),
                move || {
                    // 登録し直しは、メインのスレッドの上で走る
                    assert_eq!(std::thread::current().id(), main_id);
                    p.unregister_all(Duration::from_millis(800), || {})?;
                    p.register(7, Duration::from_millis(800))?;
                    p.register(8, Duration::from_millis(800))
                },
            );
            let _ = done_tx.send(r);
        });
        let _ = gate_tx.send(());
        assert!(key_rx.recv_timeout(Duration::from_secs(5)).unwrap(), "the key event waited for the lock");
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(5)).unwrap(), Some(Ok(())));
        assert_eq!(*plugin.shortcuts.lock().unwrap(), vec![7, 8]);
    }

    /// メインのスレッドの上から呼んでも（起動のときの setup）、自分を待たずにその場で走る
    #[test]
    fn run_on_main_from_the_main_thread_runs_in_place() {
        let main = FakeMain::start();
        let poster = main.poster.clone();
        let (tx, rx) = mpsc::channel();
        main.poster.post(Box::new(move || {
            let p2 = poster.clone();
            let _ = tx.send(run_on_main(|t| p2.post(t), || 42));
        }));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Some(42));
    }

    /// メインのスレッドへ渡せない（終了の途中）・渡した仕事が走らずに捨てられたときは、待ち続けずに None
    #[test]
    fn run_on_main_gives_up_when_the_task_cannot_run() {
        assert_eq!(run_on_main(|_t| false, || 1), None);
        // 受け取ったが走らせずに捨てた
        assert_eq!(
            run_on_main(
                |t| {
                    drop(t);
                    true
                },
                || 1
            ),
            None
        );
    }

    /// 押されたときの処理を受けるスレッド：送る側は、処理が詰まっていても待たない。届いた順に処理し、panic のあとも続ける
    #[test]
    fn worker_keeps_order_and_never_blocks_the_sender() {
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let release_rx = Mutex::new(release_rx);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let tx = worker("pl-hotkeys-test", move |x: u32| {
            if x == 1 {
                // 1つ目の処理が詰まっている間に、残りが送られる
                let _ = release_rx.lock().unwrap().recv_timeout(Duration::from_secs(5));
            }
            if x == 3 {
                panic!("one action fails");
            }
            seen2.lock().unwrap().push(x);
            if x == 5 {
                let _ = done_tx.send(());
            }
        });
        let t0 = Instant::now();
        for x in 1..=5 {
            tx.send(x).unwrap();
        }
        // 送る側（本物ではプラグインの鍵を持ったメインのスレッド）は、1つ目が詰まっていてもすぐ戻る
        assert!(t0.elapsed() < Duration::from_secs(1));
        let _ = release_tx.send(());
        done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(*seen.lock().unwrap(), vec![1, 2, 4, 5]);
    }

    fn manifest_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                rust_files(&p, out);
            } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                out.push(p);
            }
        }
    }

    /// テストの部分を除いたソース（hotkeys.rs 用。`#[cfg(test)]` はテストの mod の頭にしか無い）
    fn code_of(text: &str) -> &str {
        text.split("#[cfg(test)]").next().unwrap()
    }

    /// 関数の中身（`fn 名前` から、次の頭の `fn`・`pub fn` まで）
    fn body<'a>(code: &'a str, name: &str) -> &'a str {
        let start = code.find(&format!("fn {name}")).unwrap_or_else(|| panic!("fn {name} not found"));
        let rest = &code[start + 3..];
        let end = ["\nfn ", "\npub fn "].iter().filter_map(|m| rest.find(m)).min().unwrap_or(rest.len());
        &rest[..end]
    }

    /// 頭の決まりが、本物のコードで守られている：
    /// - プラグインを呼ぶ（`global_shortcut()`）のは、`src` 全体で `register_on_main` の中の1か所だけ
    /// - `register_on_main` を呼ぶのは `apply` だけで、`run_on_main` に渡す仕事の中
    /// - 受け口の中は `pressed`（渡すだけ）を呼び、`dispatch`・`apply` を直に呼ばない
    /// - `pressed` は送るだけで、司令塔も登録し直しも呼ばない
    #[test]
    fn plugin_is_called_only_on_the_main_thread() {
        let mut files = vec![];
        rust_files(&manifest_dir().join("src"), &mut files);
        assert!(files.len() > 30, "sources not found");
        let mut calls = vec![];
        for f in &files {
            let text = std::fs::read_to_string(f).unwrap();
            // このファイルだけはテストの部分（この検査の文字）を除く。ほかのファイルは全部を見る
            let n = if f.ends_with("hotkeys.rs") { code_of(&text) } else { &text }.matches("global_shortcut()").count();
            if n > 0 {
                calls.push((f.file_name().unwrap().to_string_lossy().into_owned(), n));
            }
        }
        assert_eq!(calls, vec![("hotkeys.rs".to_string(), 1)]);

        let text = std::fs::read_to_string(manifest_dir().join("src/platform/windows/hotkeys.rs")).unwrap();
        let code = code_of(&text);
        assert!(body(code, "register_on_main").contains("global_shortcut()"));
        assert_eq!(code.matches("register_on_main(").count(), 2, "defined once and called once");
        let apply = body(code, "apply");
        let in_main = &apply[apply.find("run_on_main(").expect("apply hands the work to run_on_main")..];
        assert!(in_main.contains("register_on_main("));

        let reg = body(code, "register_on_main");
        let handler = &reg[reg.find("on_shortcut(").unwrap()..];
        let handler = &handler[..handler.find("});").unwrap()];
        assert!(handler.contains("pressed(app"));
        assert!(!handler.contains("dispatch(") && !handler.contains("apply(") && !handler.contains(".lock()"));

        let pressed = body(code, "pressed");
        assert!(pressed.contains("tx.send("));
        assert!(!pressed.contains("apply(") && !pressed.contains("global_shortcut") && !pressed.contains("state::<"));
    }

    /// この作りは、次の版の中身を読んで決めた（頭の説明の行の番号もこの版のもの）。版が変わったら読み直してから、ここを直す
    #[test]
    fn pinned_versions() {
        let toml = std::fs::read_to_string(manifest_dir().join("Cargo.toml")).unwrap();
        assert!(toml.contains("tauri-plugin-global-shortcut = \"=2.4.0\""));
        let lock = std::fs::read_to_string(manifest_dir().join("Cargo.lock")).unwrap().replace("\r\n", "\n");
        for (name, ver) in [("tauri-plugin-global-shortcut", "2.4.0"), ("global-hotkey", "0.8.0"), ("tauri-runtime-wry", "2.12.1")] {
            assert!(lock.contains(&format!("name = \"{name}\"\nversion = \"{ver}\"\n")), "{name} is not {ver}: re-read the thread rules in hotkeys.rs");
        }
    }
}
