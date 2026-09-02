//! In-memory log ring — the phone's flight recorder.
//!
//! A `tracing` layer captures every event (and a panic hook the
//! panics) into a bounded ring; the keys view's Logs tab renders and
//! copies it. No files, no network — the diagnostics are wherever the
//! problem is.

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
/// crashed thread leaves its trace on the Logs tab.
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        push(format!("PANIC {info}"));
        default(info);
    }));
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
