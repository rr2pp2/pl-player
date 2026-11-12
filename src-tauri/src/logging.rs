//! ログ：手元のファイルだけに書く（`%APPDATA%\<識別子>\logs\pl-player.log`）。外へは送らない。
//!
//! tracing の受け手（Subscriber）を小さく自前で持つ（承認済みの依存に tracing-subscriber が無いため）。
//! 1MB を超えたら .old に回して1つだけ残す。

use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};

struct FileLog {
    path: PathBuf,
    lock: Mutex<()>,
    next_id: AtomicU64,
}

struct Msg(String);

impl Visit for Msg {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        } else {
            let _ = write!(self.0, " {}={value:?}", field.name());
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            let _ = write!(self.0, " {}={value}", field.name());
        }
    }
}

impl Subscriber for FileLog {
    fn enabled(&self, m: &Metadata<'_>) -> bool {
        *m.level() <= Level::INFO && m.target().starts_with("pl_player")
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(self.next_id.fetch_add(1, Ordering::Relaxed).max(1))
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, e: &Event<'_>) {
        let mut m = Msg(String::new());
        e.record(&mut m);
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let line = format!("{secs} {} {} {}\n", e.metadata().level(), e.metadata().target(), m.0);
        let _g = self.lock.lock();
        if fs::metadata(&self.path).map(|m| m.len() > 1_000_000).unwrap_or(false) {
            let _ = fs::rename(&self.path, self.path.with_extension("log.old"));
        }
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&self.path) {
            let _ = f.write_all(line.as_bytes());
        }
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

pub fn init(dir: PathBuf) {
    let _ = fs::create_dir_all(&dir);
    let sub = FileLog {
        path: dir.join("pl-player.log"),
        lock: Mutex::new(()),
        next_id: AtomicU64::new(1),
    };
    let _ = tracing::subscriber::set_global_default(sub);
}
