//! A rolling MIDI monitor — the shared "what MIDI is arriving" log.
//!
//! Tapping every event into a small ring buffer is a cross-cutting MIDI
//! concern, not a rig-specific one, so it lives in the midicore facade: any
//! rig (guitar, drums, …) records into one and any front-end renders it
//! (`midicore-ui`'s monitor panel), formatting through [`MidiEvent`]'s
//! `Display`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use midicore_proto::MidiEvent;

/// How many recent messages the monitor keeps.
pub const MIDI_MONITOR_CAP: usize = 64;

/// A cheap-to-clone handle onto a shared rolling MIDI log. Clone it into a MIDI
/// input callback and `record` each event; read it from a UI/meter loop with
/// `recent` / `count`.
#[derive(Clone, Default)]
pub struct MidiMonitor {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    count: u64,
    recent: VecDeque<MidiEvent>,
}

impl MidiMonitor {
    /// A fresh, empty monitor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a message (tap from a MIDI input callback). Lock-light; safe to
    /// call from the MIDI thread.
    pub fn record(&self, msg: &MidiEvent) {
        if let Ok(mut g) = self.inner.lock() {
            g.count = g.count.saturating_add(1);
            if g.recent.len() >= MIDI_MONITOR_CAP {
                g.recent.pop_front();
            }
            g.recent.push_back(msg.clone());
        }
    }

    /// Total messages seen since the monitor was created.
    pub fn count(&self) -> u64 {
        self.inner.lock().map(|g| g.count).unwrap_or(0)
    }

    /// Snapshot of the most recent messages, oldest first.
    pub fn recent(&self) -> Vec<MidiEvent> {
        self.inner
            .lock()
            .map(|g| g.recent.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Snapshot rendered to display strings (via [`MidiEvent`]'s `Display`).
    pub fn recent_formatted(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|g| g.recent.iter().map(|e| e.to_string()).collect())
            .unwrap_or_default()
    }

    /// Clear the log (keeps the running count).
    pub fn clear(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.recent.clear();
        }
    }
}
