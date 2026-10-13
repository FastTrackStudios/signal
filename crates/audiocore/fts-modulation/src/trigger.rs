//! Trigger engine — manages phase advancement across trigger modes.
//!
//! Handles Sync (DAW-locked), Free (independent), MIDI (note-triggered),
//! and Audio (transient-triggered) modes.
//!
//! Based on tiagolr's trigger system shared across gate12, filtr, time12, reevr.

use crate::tempo::{self, SyncDivision, TransportInfo, SYNC_TABLE};

/// Trigger mode for pattern playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    /// Phase locked to DAW transport. Stops when transport stops.
    Sync,
    /// Free-running, continues even when transport stops.
    Free,
    /// One-shot triggered by MIDI note-on.
    Midi,
    /// One-shot triggered by audio transient detection.
    Audio,
}

/// Anti-click crossfade duration on retrigger.
const ANTICLICK_MS: f64 = 5.0;

/// Trigger engine state.
pub struct TriggerEngine {
    /// Current trigger mode.
    pub mode: TriggerMode,
    /// Sync division index into SYNC_TABLE.
    pub sync_index: usize,
    /// Free-running rate in Hz (used when sync_index = 0).
    pub rate_hz: f64,
    /// Phase offset (0..1).
    pub phase_offset: f64,
    /// Whether to loop in MIDI/Audio mode (vs one-shot).
    pub always_playing: bool,

    // Internal state
    /// Current phase position (0..1).
    xpos: f64,
    /// Free-running position accumulator.
    rate_pos: f64,
    /// Beat position accumulator.
    beat_pos: f64,
    /// Trigger progress (0..1) for one-shot modes.
    trig_pos: f64,
    /// Whether a one-shot trigger is active.
    trig_active: bool,
    /// Per-sample increment for one-shot modes.
    trig_inc: f64,

    // Anti-click
    anticlick_remaining: usize,
    anticlick_from: f64,
    anticlick_samples: usize,

    // Pattern queuing
    queued_pattern: Option<usize>,
    queue_countdown: usize,

    sample_rate: f64,
    prev_playing: bool,
}

impl TriggerEngine {
    pub fn new() -> Self {
        Self {
            mode: TriggerMode::Sync,
            sync_index: 9, // 1/1 bar default
            rate_hz: 1.0,
            phase_offset: 0.0,
            always_playing: false,
            xpos: 0.0,
            rate_pos: 0.0,
            beat_pos: 0.0,
            trig_pos: 0.0,
            trig_active: false,
            trig_inc: 0.0,
            anticlick_remaining: 0,
            anticlick_from: 0.0,
            anticlick_samples: 0,
            queued_pattern: None,
            queue_countdown: 0,
            sample_rate: 48000.0,
            prev_playing: false,
        }
    }

    /// Update for new sample rate.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.anticlick_samples = (ANTICLICK_MS * 0.001 * sample_rate) as usize;
        self.update_trig_inc();
    }

    /// Get the current sync division.
    pub fn sync_division(&self) -> &SyncDivision {
        &SYNC_TABLE[self.sync_index.min(SYNC_TABLE.len() - 1)]
    }

    /// Calculate per-sample increment for one-shot modes.
    fn update_trig_inc(&mut self) {
        let div = self.sync_division();
        if div.is_free() {
            self.trig_inc = self.rate_hz / self.sample_rate;
        } else {
            // Will be updated per-block when we have tempo info
            self.trig_inc = 0.0;
        }
    }

    /// Advance the phase by one sample. Returns the current phase (0..1).
    ///
    /// Call this once per sample in the processing loop.
    pub fn tick(&mut self, transport: &TransportInfo) -> f64 {
        // Handle pattern queue countdown
        if self.queue_countdown > 0 {
            self.queue_countdown -= 1;
        }

        match self.mode {
            TriggerMode::Sync => self.tick_sync(transport),
            TriggerMode::Free => self.tick_free(transport),
            TriggerMode::Midi | TriggerMode::Audio => self.tick_oneshot(transport),
        }

        self.xpos
    }

    /// Handle a MIDI note-on trigger.
    pub fn midi_trigger(&mut self) {
        if self.mode != TriggerMode::Midi {
            return;
        }
        self.start_trigger();
    }

    /// Handle an audio transient trigger.
    pub fn audio_trigger(&mut self) {
        if self.mode != TriggerMode::Audio {
            return;
        }
        self.start_trigger();
    }

    /// Queue a pattern switch at the next sync boundary.
    pub fn queue_pattern(&mut self, index: usize, sync_qn: f64, transport: &TransportInfo) {
        if sync_qn <= 0.0 {
            // Immediate switch
            self.queued_pattern = Some(index);
            self.queue_countdown = 0;
        } else {
            self.queued_pattern = Some(index);
            self.queue_countdown = tempo::pattern_sync_countdown(
                transport.position_qn,
                sync_qn,
                self.sample_rate,
                transport.tempo_bpm,
            );
        }
    }

    /// Check if a queued pattern switch is ready. Returns the pattern index if so.
    pub fn poll_pattern_switch(&mut self) -> Option<usize> {
        if self.queue_countdown == 0 {
            self.queued_pattern.take()
        } else {
            None
        }
    }

    /// Get the current phase (0..1).
    pub fn phase(&self) -> f64 {
        self.xpos
    }

    /// Whether a one-shot trigger is currently active.
    pub fn is_triggered(&self) -> bool {
        self.trig_active
    }

    /// Get the anti-click blend factor (0..1, 1 = no blending needed).
    pub fn anticlick_factor(&self) -> f64 {
        if self.anticlick_remaining > 0 && self.anticlick_samples > 0 {
            let t = 1.0 - (self.anticlick_remaining as f64 / self.anticlick_samples as f64);
            // Quadratic ease-in-out
            if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            }
        } else {
            1.0
        }
    }

    /// Get the anti-click origin value (what we're fading from).
    pub fn anticlick_from(&self) -> f64 {
        self.anticlick_from
    }

    pub fn reset(&mut self) {
        self.xpos = 0.0;
        self.rate_pos = 0.0;
        self.beat_pos = 0.0;
        self.trig_pos = 0.0;
        self.trig_active = false;
        self.anticlick_remaining = 0;
        self.queued_pattern = None;
        self.queue_countdown = 0;
        self.prev_playing = false;
    }

    // ── Private tick implementations ──────────────────────────────────

    fn tick_sync(&mut self, transport: &TransportInfo) {
        if !transport.playing {
            self.prev_playing = false;
            return;
        }

        // Restart on play
        if !self.prev_playing {
            self.beat_pos = transport.position_qn;
        }
        self.prev_playing = true;

        let div = *self.sync_division();
        if div.is_free() {
            self.rate_pos += self.rate_hz / self.sample_rate;
            self.xpos = tempo::rate_to_phase(self.rate_pos, self.phase_offset);
        } else {
            self.beat_pos = transport.position_qn;
            self.xpos = tempo::beat_to_phase(self.beat_pos, div.quarter_notes(), self.phase_offset);
        }
    }

    fn tick_free(&mut self, transport: &TransportInfo) {
        let div = *self.sync_division();
        if div.is_free() {
            self.rate_pos += self.rate_hz / self.sample_rate;
            self.xpos = tempo::rate_to_phase(self.rate_pos, self.phase_offset);
        } else {
            let bps = transport.beats_per_sample(self.sample_rate);
            self.beat_pos += bps;
            self.xpos = tempo::beat_to_phase(self.beat_pos, div.quarter_notes(), self.phase_offset);
        }
    }

    fn tick_oneshot(&mut self, transport: &TransportInfo) {
        if !self.trig_active && !self.always_playing {
            return;
        }

        let div = *self.sync_division();
        let inc = if div.is_free() {
            self.rate_hz / self.sample_rate
        } else {
            let bps = transport.beats_per_sample(self.sample_rate);
            if div.quarter_notes() > 0.0 {
                bps / div.quarter_notes()
            } else {
                0.0
            }
        };

        self.trig_pos += inc;
        self.xpos = (self.xpos + inc).fract();

        // Update anti-click
        if self.anticlick_remaining > 0 {
            self.anticlick_remaining -= 1;
        }

        // One-shot complete
        if self.trig_pos >= 1.0 && !self.always_playing {
            self.trig_active = false;
            self.xpos = self.phase_offset;
        }
    }

    fn start_trigger(&mut self) {
        // Store current value for anti-click crossfade
        self.anticlick_from = self.xpos;
        self.anticlick_remaining = self.anticlick_samples;

        self.trig_pos = 0.0;
        self.xpos = self.phase_offset;
        self.trig_active = true;
    }
}

impl Default for TriggerEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    fn transport_120bpm(pos_qn: f64) -> TransportInfo {
        TransportInfo {
            position_qn: pos_qn,
            tempo_bpm: 120.0,
            playing: true,
        }
    }

    #[test]
    fn sync_mode_follows_transport() {
        let mut t = TriggerEngine::new();
        t.mode = TriggerMode::Sync;
        t.sync_index = 9; // 1/1 bar = 4 QN
        t.update(SR);

        // At beat 0, phase should be 0
        let p = t.tick(&transport_120bpm(0.0));
        assert!((p - 0.0).abs() < 1e-10);

        // At beat 2 (halfway through bar), phase should be 0.5
        let p = t.tick(&transport_120bpm(2.0));
        assert!((p - 0.5).abs() < 1e-10);
    }

    #[test]
    fn sync_mode_stops_when_not_playing() {
        let mut t = TriggerEngine::new();
        t.mode = TriggerMode::Sync;
        t.update(SR);

        let stopped = TransportInfo {
            position_qn: 0.0,
            tempo_bpm: 120.0,
            playing: false,
        };
        let p1 = t.tick(&stopped);
        let p2 = t.tick(&stopped);
        assert_eq!(p1, p2, "Phase should not advance when stopped");
    }

    #[test]
    fn free_mode_runs_independently() {
        let mut t = TriggerEngine::new();
        t.mode = TriggerMode::Free;
        t.sync_index = 0; // Free Hz
        t.rate_hz = 1.0; // 1 Hz
        t.update(SR);

        let stopped = TransportInfo {
            position_qn: 0.0,
            tempo_bpm: 120.0,
            playing: false,
        };

        // Run for 1 second = one full cycle at 1 Hz
        let mut last_phase = 0.0;
        for _ in 0..48000 {
            last_phase = t.tick(&stopped);
        }
        // Should have completed approximately one cycle
        assert!(
            !(0.01..=0.99).contains(&last_phase),
            "Should be near wrap point after 1 second: {last_phase}"
        );
    }

    #[test]
    fn midi_trigger_starts_oneshot() {
        let mut t = TriggerEngine::new();
        t.mode = TriggerMode::Midi;
        t.sync_index = 0;
        t.rate_hz = 2.0; // 2 Hz = 0.5s per cycle
        t.update(SR);

        let transport = transport_120bpm(0.0);

        // Not triggered yet
        assert!(!t.is_triggered());

        // Trigger
        t.midi_trigger();
        assert!(t.is_triggered());

        // Run for a few samples — phase should advance
        for _ in 0..480 {
            t.tick(&transport);
        }
        assert!(t.phase() > 0.0, "Phase should advance after trigger");
    }

    #[test]
    fn phase_offset_applied() {
        let mut t = TriggerEngine::new();
        t.mode = TriggerMode::Sync;
        t.sync_index = 9; // 1/1 bar
        t.phase_offset = 0.25;
        t.update(SR);

        let p = t.tick(&transport_120bpm(0.0));
        assert!(
            (p - 0.25).abs() < 1e-10,
            "Phase offset should be applied: {p}"
        );
    }

    #[test]
    fn anticlick_factor() {
        let mut t = TriggerEngine::new();
        t.mode = TriggerMode::Midi;
        t.sync_index = 0;
        t.rate_hz = 1.0;
        t.update(SR);

        // Before trigger: no anti-click
        assert_eq!(t.anticlick_factor(), 1.0);

        // After trigger: anti-click active
        t.midi_trigger();
        assert!(
            t.anticlick_factor() < 1.0,
            "Anti-click should be active after trigger"
        );

        // After enough samples: anti-click complete
        let transport = transport_120bpm(0.0);
        for _ in 0..480 {
            t.tick(&transport);
        }
        assert_eq!(t.anticlick_factor(), 1.0, "Anti-click should complete");
    }
}
