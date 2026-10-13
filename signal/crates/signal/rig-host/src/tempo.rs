//! The rig's tempo, shared by every rig in the process.
//!
//! One band, one tempo: the guitar rig's tap (or its song's tempo) is the
//! tempo the keys rig's synced delays follow too. Whoever sets it writes
//! here; readers poll [`generation`] to notice a change.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// BPM as f32 bits; 0 = none set.
static BPM: AtomicU32 = AtomicU32::new(0);
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Set the tempo (clamped to 20..400 BPM).
pub fn set(bpm: f32) {
    let bpm = bpm.clamp(20.0, 400.0);
    if BPM.swap(bpm.to_bits(), Ordering::AcqRel) != bpm.to_bits() {
        GENERATION.fetch_add(1, Ordering::AcqRel);
    }
}

/// The tempo, if anything has set one.
#[must_use]
pub fn get() -> Option<f32> {
    let bits = BPM.load(Ordering::Acquire);
    (bits != 0).then(|| f32::from_bits(bits))
}

/// Bumped on every change.
#[must_use]
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    #[test]
    fn set_get_and_change_count() {
        let g = super::generation();
        super::set(128.0);
        assert_eq!(super::get(), Some(128.0));
        assert!(super::generation() > g);
        let g = super::generation();
        super::set(128.0);
        assert_eq!(super::generation(), g, "same tempo is not a change");
    }
}
