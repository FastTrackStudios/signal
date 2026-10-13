//! In-memory log ring — the phone's flight recorder.
//!
//! A `tracing` layer captures every event (and a panic hook the
//! panics) into a bounded ring; the keys view's Logs tab renders and
//! copies it. No network — the diagnostics are wherever the problem is;
//! the one file is the crash log, a panic's trace kept past the crash.

use std::collections::VecDeque;
use std::sync::Mutex;

const CAP: usize = 600;

static RING: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

pub fn push(line: String) {
    if let Ok(mut ring) = RING.lock() {
        if ring.len() >= CAP {
            ring.pop_front();
        }
        ring.push_back(line);
    }
}

/// Newest last.
pub fn snapshot() -> Vec<String> {
    RING.lock()
        .map(|r| r.iter().cloned().collect())
        .unwrap_or_default()
}

/// Route panics into the ring (chained onto the default hook), so a
/// crashed thread leaves its trace on the Logs tab — and onto the end of
/// [`crash_log`], so a crash that takes the app down (a panic on the UI
/// thread aborts on iOS) can still be read afterwards: the panic, where,
/// the backtrace, and what was logged just before it.
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        push(format!("PANIC {info}"));
        record_crash(&info.to_string());
        default(info);
    }));
}

/// Where crashes are kept: `crash.log` in the app's data folder (on a
/// phone or a simulator, inside the app's container).
#[must_use]
pub fn crash_log() -> Option<std::path::PathBuf> {
    #[cfg(feature = "native")]
    {
        Some(dirs::data_dir()?.join("Session").join("crash.log"))
    }
    #[cfg(not(feature = "native"))]
    {
        None
    }
}

/// Append a crash to [`crash_log`], keeping the file to its last few.
fn record_crash(what: &str) {
    use std::io::Write as _;
    /// Past this the file starts over: the newest crashes are the ones
    /// read.
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

/// The capture layer — timestamps relative to process start.
pub struct RingLayer {
    start: std::time::Instant,
}

impl RingLayer {
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for RingLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        use std::fmt::Write as _;
        let mut msg = String::new();
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
