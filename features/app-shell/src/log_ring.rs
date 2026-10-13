//! In-memory log ring — the app's flight recorder.
//!
//! A `tracing` layer ([`RingLayer`]) captures every event, and a panic hook
//! ([`install_panic_hook`]) the panics, into a bounded ring the app's Logs
//! view renders and copies. Two files outlive a run:
//!
//! - the **crash log** ([`crash_log`]): each panic's trace, its backtrace
//!   and what was logged just before it, appended — `crash.log` in the
//!   app's data folder;
//! - the **flight log**: the ring itself, written on a panic and whenever
//!   the app calls [`persist`] (the rig's audio watcher does, every few
//!   seconds), so a run the system kills without a panic still leaves its
//!   last lines; [`restore_previous`] puts them on the Logs view at the next
//!   launch. It lives at `$XDG_CONFIG_HOME/flight-log.txt` when that is set
//!   (the phone sets it before the engine starts).

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

const CAP: usize = 600;

static RING: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// The app's name, for its data folder (`install_panic_hook` sets it).
static APP: OnceLock<&'static str> = OnceLock::new();

pub fn push(line: String) {
    if let Ok(mut ring) = RING.lock() {
        if ring.len() >= CAP {
            ring.pop_front();
        }
        ring.push_back(line);
    }
}

/// Newest last.
#[must_use]
pub fn snapshot() -> Vec<String> {
    RING.lock()
        .map(|r| r.iter().cloned().collect())
        .unwrap_or_default()
}

/// Route panics into the ring (chained onto the default hook), the crash
/// log and the flight log, so a crashed thread leaves its trace on the Logs
/// view and on disk — a panic that takes the app down (a panic on the UI
/// thread aborts on iOS) is read afterwards. `app` names the app's data
/// folder (`Session`, `Signal`).
pub fn install_panic_hook(app: &'static str) {
    let _ = APP.set(app);
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        push(format!("PANIC on {}: {info}", thread.name().unwrap_or("?")));
        record_crash(&info.to_string());
        persist();
        default(info);
    }));
}

/// Where crashes are kept: `crash.log` in the app's data folder (on a phone
/// or a simulator, inside the app's container). `None` before
/// [`install_panic_hook`] named the app, and in the browser.
#[must_use]
pub fn crash_log() -> Option<std::path::PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Some(dirs::data_dir()?.join(APP.get()?).join("crash.log"))
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// Append a crash to [`crash_log`], keeping the file to its last few.
fn record_crash(what: &str) {
    use std::io::Write as _;
    /// Past this the file starts over: the newest crashes are the ones read.
    const KEEP: u64 = 512 * 1024;
    let Some(path) = crash_log() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > KEEP) {
        let _ = std::fs::remove_file(&path);
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    let when = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let thread = std::thread::current();
    let backtrace = std::backtrace::Backtrace::force_capture();
    let recent = snapshot();
    let tail = &recent[recent.len().saturating_sub(40)..];
    let _ = writeln!(
        file,
        "=== crash at unix {when} on thread {} ===\n{what}\n\n{backtrace}\n--- logged before it ---\n{}\n",
        thread.name().unwrap_or("<unnamed>"),
        tail.join("\n"),
    );
}

/// Where the ring is kept between runs: `$XDG_CONFIG_HOME/flight-log.txt`.
fn persist_path() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME").map(|d| std::path::PathBuf::from(d).join("flight-log.txt"))
}

/// Write the ring to the flight log, so a run that ends without warning (a
/// crash, the system killing it for memory) leaves its last lines behind.
pub fn persist() {
    let Some(path) = persist_path() else { return };
    let text = snapshot().join("\n");
    let _ = std::fs::write(path, text);
}

/// At launch: the previous run's last lines, into the ring under a marker —
/// what it was doing when it stopped is on the Logs view, copyable with the
/// rest. The file is then moved aside so the next crash writes fresh.
pub fn restore_previous() {
    let Some(path) = persist_path() else { return };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let lines: Vec<&str> = text.lines().collect();
    let tail = &lines[lines.len().saturating_sub(200)..];
    push("──── the previous run's last lines ────".to_string());
    for l in tail {
        push(format!("prev| {l}"));
    }
    push("──── this run ────".to_string());
    let _ = std::fs::rename(&path, path.with_file_name("flight-log-previous.txt"));
}

/// The capture layer — timestamps relative to process start.
pub struct RingLayer {
    start: std::time::Instant,
}

impl RingLayer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
}

impl Default for RingLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for RingLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        use std::fmt::Write as _;
        struct Visitor<'a>(&'a mut String);
        impl tracing::field::Visit for Visitor<'_> {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    let _ = write!(self.0, "{value:?} ");
                } else {
                    let _ = write!(self.0, "{}={:?} ", field.name(), value);
                }
            }
        }
        let mut msg = String::new();
        event.record(&mut Visitor(&mut msg));
        let meta = event.metadata();
        push(format!(
            "[{:9.3}] {:5} {}: {}",
            self.start.elapsed().as_secs_f64(),
            meta.level().as_str(),
            meta.target(),
            msg.trim_end(),
        ));
    }
}
