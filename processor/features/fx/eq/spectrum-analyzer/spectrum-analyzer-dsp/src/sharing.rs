//! Cross-instance spectrum sharing.
//!
//! Pro-Q 4's SC/Ext mode can display the spectrum of *another* plugin instance.
//! Plugins all load into one host process, so a process-global registry keyed by
//! instance id is enough: each instance publishes its latest post-EQ dB spectrum
//! and frequency axis, and any other instance can read a published snapshot.
//!
//! The registry holds `Arc<SharedSpectrum>` per instance; each `SharedSpectrum`
//! double-buffers a dB vector behind a short lock — publishing copies in,
//! reading copies out. Instance ids must be unique per instance.

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

/// Stable identifier for a plugin instance.
pub type InstanceId = u64;

/// A published spectrum slot, shared between a publisher and any readers.
#[derive(Default)]
pub struct SharedSpectrum {
    inner: Mutex<SharedInner>,
}

#[derive(Default, Clone)]
struct SharedInner {
    /// Display label (e.g. track name) for the UI's source picker.
    label: String,
    /// Latest post-EQ dB spectrum.
    db: Vec<f32>,
    /// Frequency (Hz) for each dB bin.
    freq_hz: Vec<f32>,
    /// Bumped on every publish so readers can detect staleness.
    generation: u64,
}

impl SharedSpectrum {
    /// Replace the published spectrum.
    pub fn publish(&self, label: &str, db: &[f32], freq_hz: &[f32]) {
        let mut inner = self.inner.lock();
        if inner.label != label {
            inner.label = label.to_string();
        }
        inner.db.clear();
        inner.db.extend_from_slice(db);
        inner.freq_hz.clear();
        inner.freq_hz.extend_from_slice(freq_hz);
        inner.generation = inner.generation.wrapping_add(1);
    }

    /// Copy the latest spectrum out into `db_out` / `freq_out`. Returns the
    /// publish generation (0 if never published).
    pub fn read_into(&self, db_out: &mut Vec<f32>, freq_out: &mut Vec<f32>) -> u64 {
        let inner = self.inner.lock();
        db_out.clear();
        db_out.extend_from_slice(&inner.db);
        freq_out.clear();
        freq_out.extend_from_slice(&inner.freq_hz);
        inner.generation
    }

    /// Current display label.
    pub fn label(&self) -> String {
        self.inner.lock().label.clone()
    }
}

static REGISTRY: Lazy<Mutex<HashMap<InstanceId, Arc<SharedSpectrum>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Register (or fetch the existing) shared slot for an instance.
pub fn register(id: InstanceId) -> Arc<SharedSpectrum> {
    let mut reg = REGISTRY.lock();
    reg.entry(id)
        .or_insert_with(|| Arc::new(SharedSpectrum::default()))
        .clone()
}

/// Remove an instance's slot (call on plugin teardown).
pub fn unregister(id: InstanceId) {
    REGISTRY.lock().remove(&id);
}

/// Fetch another instance's slot for reading, if it exists.
pub fn get(id: InstanceId) -> Option<Arc<SharedSpectrum>> {
    REGISTRY.lock().get(&id).cloned()
}

/// List `(id, label)` of all instances other than `exclude`, for the source picker.
pub fn list_others(exclude: InstanceId) -> Vec<(InstanceId, String)> {
    REGISTRY
        .lock()
        .iter()
        .filter(|(id, _)| **id != exclude)
        .map(|(id, slot)| (*id, slot.label()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_then_read_across_handles() {
        let a = register(42);
        let b = get(42).expect("registered");
        a.publish("track A", &[-3.0, -6.0], &[100.0, 200.0]);
        let mut db = Vec::new();
        let mut fz = Vec::new();
        let generation = b.read_into(&mut db, &mut fz);
        assert_eq!(db, vec![-3.0, -6.0]);
        assert_eq!(fz, vec![100.0, 200.0]);
        assert!(generation >= 1);
        unregister(42);
        assert!(get(42).is_none());
    }
}
