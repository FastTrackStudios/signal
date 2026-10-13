//! This process's monotonic clock, in microseconds.
//!
//! Monotonic (never jumps with NTP or a user setting the time) and
//! high-resolution (`Instant` is nanoseconds on every desktop OS). Its
//! epoch is the first call in the process, so two processes — even on one
//! machine — read different numbers: that is what the clock offset is for.
//! Everything that stamps a time for sync (the audio thread's snapshot, a
//! ping, a pong) must read *this* clock, so their stamps compare.

use std::sync::OnceLock;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
/// In a browser, `performance.now()` — `std`'s would panic.
#[cfg(target_arch = "wasm32")]
pub use web_time::Instant;

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

/// Microseconds since this process's clock epoch, with the fraction kept.
#[must_use]
pub fn now_micros_f64() -> f64 {
    epoch().elapsed().as_secs_f64() * 1e6
}

/// Microseconds since this process's clock epoch.
#[must_use]
pub fn now_micros() -> i64 {
    i64::try_from(epoch().elapsed().as_micros()).unwrap_or(i64::MAX)
}

/// The clock reading of an `Instant` taken earlier (a callback that read
/// `Instant::now()` on entry and publishes later).
#[must_use]
pub fn micros_of(instant: Instant) -> f64 {
    let e = epoch();
    if instant >= e {
        instant.duration_since(e).as_secs_f64() * 1e6
    } else {
        -(e.duration_since(instant).as_secs_f64() * 1e6)
    }
}
