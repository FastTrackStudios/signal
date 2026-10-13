//! Whether track groups gang at all — REAPER's grouping override, one
//! switch for the whole engine. Off, arming, muting, soloing or moving a
//! grouped track moves only that track; the groups are kept, unchanged,
//! for when it is on again.

use std::sync::atomic::{AtomicBool, Ordering};

static ON: AtomicBool = AtomicBool::new(true);

/// Turn grouping on or off.
pub fn set_enabled(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

/// Whether grouping is on (it is, until turned off).
#[must_use]
pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}
