//! Real-time macro value recording and playback.
//!
//! Records macro knob value changes with timestamps during performance,
//! allowing playback of recorded sequences with accurate timing.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// A single recorded macro value change with timestamp.
#[derive(Clone, Debug)]
pub struct MacroRecord {
    /// Milliseconds since recording started.
    pub time_ms: u64,
    /// ID of the macro knob (e.g., "drive").
    pub knob_id: String,
    /// Normalized value (0.0–1.0).
    pub value: f32,
}

/// Real-time macro recording state.
#[derive(Clone, Debug)]
enum RecordingState {
    Idle,
    Recording {
        start_time_ms: u64,
        records: Vec<MacroRecord>,
    },
}

/// Macro recorder — captures knob changes during performance.
pub struct MacroRecorder {
    state: Arc<Mutex<RecordingState>>,
}

impl MacroRecorder {
    /// Create a new recorder in idle state.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RecordingState::Idle)),
        }
    }

    /// Start recording macro changes.
    /// Clears any previous recording.
    pub fn start(&self) {
        let now = current_time_ms();
        let mut state = self.state.lock().expect("lock poisoned");
        *state = RecordingState::Recording {
            start_time_ms: now,
            records: Vec::new(),
        };
    }

    /// Record a macro value change at the current time.
    /// No-op if not recording.
    pub fn record(&self, knob_id: String, value: f32) {
        let now = current_time_ms();
        let mut state = self.state.lock().expect("lock poisoned");

        if let RecordingState::Recording {
            start_time_ms,
            records,
        } = &mut *state
        {
            let time_ms = now.saturating_sub(*start_time_ms);
            records.push(MacroRecord {
                time_ms,
                knob_id,
                value,
            });
        }
    }

    /// Stop recording and return the captured sequence.
    /// Returns empty vec if not recording.
    pub fn stop(&self) -> Vec<MacroRecord> {
        let mut state = self.state.lock().expect("lock poisoned");
        match &*state {
            RecordingState::Recording { records, .. } => {
                let captured = records.clone();
                *state = RecordingState::Idle;
                captured
            }
            RecordingState::Idle => Vec::new(),
        }
    }

    /// Get current recording state without stopping.
    pub fn peek(&self) -> Vec<MacroRecord> {
        let state = self.state.lock().expect("lock poisoned");
        match &*state {
            RecordingState::Recording { records, .. } => records.clone(),
            RecordingState::Idle => Vec::new(),
        }
    }

    /// Check if currently recording.
    pub fn is_recording(&self) -> bool {
        let state = self.state.lock().expect("lock poisoned");
        matches!(&*state, RecordingState::Recording { .. })
    }
}

impl Default for MacroRecorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Get current time in milliseconds since UNIX_EPOCH.
fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_record_and_stop() {
        let recorder = MacroRecorder::new();
        assert!(!recorder.is_recording());

        recorder.start();
        assert!(recorder.is_recording());

        recorder.record("drive".into(), 0.5);
        thread::sleep(Duration::from_millis(10));
        recorder.record("tone".into(), 0.7);

        let records = recorder.stop();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].knob_id, "drive");
        assert_eq!(records[0].value, 0.5);
        assert_eq!(records[1].knob_id, "tone");
        assert_eq!(records[1].value, 0.7);
        assert!(records[1].time_ms >= 10);

        assert!(!recorder.is_recording());
    }

    #[test]
    fn test_peek_while_recording() {
        let recorder = MacroRecorder::new();
        recorder.start();

        recorder.record("drive".into(), 0.5);
        recorder.record("tone".into(), 0.7);

        let peeked = recorder.peek();
        assert_eq!(peeked.len(), 2);

        // Recording still active
        assert!(recorder.is_recording());

        // Can still add more
        recorder.record("gain".into(), 0.3);
        let peeked2 = recorder.peek();
        assert_eq!(peeked2.len(), 3);
    }

    #[test]
    fn test_stop_when_idle_returns_empty() {
        let recorder = MacroRecorder::new();
        let records = recorder.stop();
        assert!(records.is_empty());
    }

    #[test]
    fn test_start_clears_previous_recording() {
        let recorder = MacroRecorder::new();

        recorder.start();
        recorder.record("drive".into(), 0.5);
        let _ = recorder.stop();

        // Start again
        recorder.start();
        assert!(recorder.is_recording());
        recorder.record("tone".into(), 0.7);
        let records = recorder.stop();

        // Should only have the new recording
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].knob_id, "tone");
    }
}
