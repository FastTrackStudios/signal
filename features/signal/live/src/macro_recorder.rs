//! Real-time macro value recording and playback.
//!
//! Records macro knob value changes with timestamps during performance,
//! allowing playback of recorded sequences with accurate timing.
//!
//! # Use Cases
//!
//! - **Performance automation**: Record knob movements during a live performance
//! - **Jamming**: Capture spontaneous parameter tweaks and replay them
//! - **Layering**: Record multiple macro sequences and stack them
//! - **Live FX**: Use recordings as modulation sources in future enhancements
//!
//! # Thread Safety
//!
//! Uses `Arc<Mutex<RecordingState>>` to:
//! - Allow sharing across async tasks without lifetime constraints
//! - Safely mutate state from multiple callers (record, start, stop)
//! - Avoid circular dependencies and borrowing issues
//!
//! # Performance
//!
//! - **Record**: O(1) append to Vec, minimal mutex contention
//! - **Stop/Peek**: O(n) clone where n = number of records
//! - **Memory**: Each record is 32 bytes (u64 + String + f32), so 100KB per 3300 records
//!
//! # Example
//!
//! ```ignore
//! let recorder = MacroRecorder::new();
//!
//! // Start recording
//! recorder.start();
//!
//! // Simulate knob movements
//! recorder.record("drive".into(), 0.5);
//! tokio::time::sleep(Duration::from_millis(10)).await;
//! recorder.record("drive".into(), 0.6);
//!
//! // Get recording
//! let sequence = recorder.stop();
//! assert_eq!(sequence.len(), 2);
//! assert_eq!(sequence[1].knob_id, "drive");
//! assert_eq!(sequence[1].value, 0.6);
//! assert!(sequence[1].time_ms >= 10);
//! ```
//!
//! # Future Enhancements
//!
//! - **Serialization**: Save/load recordings with `bincode` or `serde_json`
//! - **Playback**: Schedule recorded values with timing accuracy
//! - **Smoothing**: Interpolate recorded values for smoother playback
//! - **Compression**: Downsample or delta-encode long sequences
//! - **Named sequences**: Allow naming and organizing recordings

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
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RecordingState::Idle)),
        }
    }

    /// Start recording macro changes.
    /// Clears any previous recording.
    pub fn start(&self) {
        let now = current_time_ms();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *state = RecordingState::Recording {
            start_time_ms: now,
            records: Vec::new(),
        };
    }

    /// Record a macro value change at the current time.
    /// No-op if not recording.
    pub fn record(&self, knob_id: String, value: f32) {
        let now = current_time_ms();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

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
    #[must_use]
    pub fn stop(&self) -> Vec<MacroRecord> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
    #[must_use]
    pub fn peek(&self) -> Vec<MacroRecord> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match &*state {
            RecordingState::Recording { records, .. } => records.clone(),
            RecordingState::Idle => Vec::new(),
        }
    }

    /// Check if currently recording.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        matches!(&*state, RecordingState::Recording { .. })
    }

    /// Get the number of recorded changes without stopping.
    #[must_use]
    pub fn record_count(&self) -> usize {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match &*state {
            RecordingState::Recording { records, .. } => records.len(),
            RecordingState::Idle => 0,
        }
    }

    /// Get the duration of current recording in milliseconds.
    /// Returns None if not currently recording.
    #[must_use]
    pub fn elapsed_ms(&self) -> Option<u64> {
        let now = current_time_ms();
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match &*state {
            RecordingState::Recording { start_time_ms, .. } => {
                Some(now.saturating_sub(*start_time_ms))
            }
            RecordingState::Idle => None,
        }
    }

    /// Get statistics about the recording (count, duration, knobs touched).
    /// Returns (`record_count`, `duration_ms`, `unique_knob_ids`)
    #[must_use]
    pub fn stats(&self) -> (usize, u64, Vec<String>) {
        let now = current_time_ms();
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        match &*state {
            RecordingState::Recording {
                start_time_ms,
                records,
            } => {
                let duration = now.saturating_sub(*start_time_ms);
                let mut knobs: Vec<String> = records
                    .iter()
                    .map(|r| r.knob_id.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                knobs.sort();
                (records.len(), duration, knobs)
            }
            RecordingState::Idle => (0, 0, Vec::new()),
        }
    }
}

impl Default for MacroRecorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Get current time in milliseconds since `UNIX_EPOCH`.
fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
