//! Tempo sync utilities — beat division table and phase calculation.
//!
//! Based on tiagolr's sync system shared across gate12, filtr, time12, reevr.

/// Beat division for tempo-synced modulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SyncDivision {
    /// Free-running at a rate in Hz.
    FreeHz,
    /// Musical division: numerator/denominator in quarter notes.
    Beat(f64),
    /// Triplet division.
    Triplet(f64),
    /// Dotted division.
    Dotted(f64),
}

impl SyncDivision {
    /// Duration in quarter notes.
    pub fn quarter_notes(&self) -> f64 {
        match *self {
            SyncDivision::FreeHz => 0.0,
            SyncDivision::Beat(qn) => qn,
            SyncDivision::Triplet(qn) => qn * 2.0 / 3.0,
            SyncDivision::Dotted(qn) => qn * 1.5,
        }
    }

    /// Whether this is free-running (not tempo-synced).
    pub fn is_free(&self) -> bool {
        matches!(self, SyncDivision::FreeHz)
    }
}

/// Standard sync division table matching tiagolr's plugins.
///
/// Index 0 = Free Hz, then musical divisions from 1/256 to 4/1,
/// plus triplets and dotted variants.
pub const SYNC_TABLE: &[SyncDivision] = &[
    SyncDivision::FreeHz,
    // Straight divisions (quarter-note values)
    SyncDivision::Beat(1.0 / 64.0), // 1/256 (256th note)
    SyncDivision::Beat(1.0 / 32.0), // 1/128
    SyncDivision::Beat(1.0 / 16.0), // 1/64
    SyncDivision::Beat(1.0 / 8.0),  // 1/32
    SyncDivision::Beat(1.0 / 4.0),  // 1/16
    SyncDivision::Beat(1.0 / 2.0),  // 1/8
    SyncDivision::Beat(1.0),        // 1/4
    SyncDivision::Beat(2.0),        // 1/2
    SyncDivision::Beat(4.0),        // 1/1 (one bar)
    SyncDivision::Beat(8.0),        // 2/1
    SyncDivision::Beat(16.0),       // 4/1
    // Triplets
    SyncDivision::Triplet(1.0 / 4.0), // 1/16t
    SyncDivision::Triplet(1.0 / 2.0), // 1/8t
    SyncDivision::Triplet(1.0),       // 1/4t
    SyncDivision::Triplet(2.0),       // 1/2t
    SyncDivision::Triplet(4.0),       // 1/1t
    // Dotted
    SyncDivision::Dotted(1.0 / 4.0), // 1/16.
    SyncDivision::Dotted(1.0 / 2.0), // 1/8.
    SyncDivision::Dotted(1.0),       // 1/4.
    SyncDivision::Dotted(2.0),       // 1/2.
    SyncDivision::Dotted(4.0),       // 1/1.
];

/// Transport state from the host DAW.
#[derive(Debug, Clone, Copy, Default)]
pub struct TransportInfo {
    /// Current position in quarter notes (beats).
    pub position_qn: f64,
    /// Tempo in BPM.
    pub tempo_bpm: f64,
    /// Whether the transport is playing.
    pub playing: bool,
}

impl TransportInfo {
    /// Beats per sample at the current tempo.
    pub fn beats_per_sample(&self, sample_rate: f64) -> f64 {
        if sample_rate > 0.0 && self.tempo_bpm > 0.0 {
            self.tempo_bpm / (60.0 * sample_rate)
        } else {
            0.0
        }
    }
}

/// Compute the pattern phase (0..1) from the current beat position.
///
/// `sync_qn` is the pattern length in quarter notes, `phase_offset` is 0..1.
#[inline]
pub fn beat_to_phase(beat_pos: f64, sync_qn: f64, phase_offset: f64) -> f64 {
    if sync_qn <= 0.0 {
        return 0.0;
    }
    let raw = beat_pos / sync_qn + phase_offset;
    raw - raw.floor()
}

/// Compute the pattern phase from a free-running rate in Hz.
#[inline]
pub fn rate_to_phase(rate_pos: f64, phase_offset: f64) -> f64 {
    let raw = rate_pos + phase_offset;
    raw - raw.floor()
}

/// Calculate the countdown in samples until the next pattern-sync boundary.
///
/// Used for quantized pattern switching (e.g., switch on next beat).
pub fn pattern_sync_countdown(
    beat_pos: f64,
    sync_qn: f64,
    sample_rate: f64,
    tempo_bpm: f64,
) -> usize {
    if sync_qn <= 0.0 || tempo_bpm <= 0.0 || sample_rate <= 0.0 {
        return 0;
    }
    let samples_per_qn = 60.0 * sample_rate / tempo_bpm;
    let interval = (sync_qn * samples_per_qn) as usize;
    if interval == 0 {
        return 0;
    }
    let current = (beat_pos * samples_per_qn) as usize;
    (interval - current % interval) % interval
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_table_has_expected_entries() {
        assert_eq!(SYNC_TABLE.len(), 22);
        assert!(SYNC_TABLE[0].is_free());
        // 1/4 note = 1.0 QN
        assert_eq!(SYNC_TABLE[7].quarter_notes(), 1.0);
        // 1/1 bar = 4.0 QN
        assert_eq!(SYNC_TABLE[9].quarter_notes(), 4.0);
    }

    #[test]
    fn beat_to_phase_wraps() {
        let p = beat_to_phase(5.0, 4.0, 0.0);
        assert!((p - 0.25).abs() < 1e-10);
    }

    #[test]
    fn beat_to_phase_with_offset() {
        let p = beat_to_phase(0.0, 4.0, 0.5);
        assert!((p - 0.5).abs() < 1e-10);
    }

    #[test]
    fn transport_beats_per_sample() {
        let t = TransportInfo {
            position_qn: 0.0,
            tempo_bpm: 120.0,
            playing: true,
        };
        let bps = t.beats_per_sample(48000.0);
        // 120 BPM = 2 beats/sec, so 2/48000 per sample
        assert!((bps - 2.0 / 48000.0).abs() < 1e-15);
    }

    #[test]
    fn triplet_duration() {
        // 1/4t = 1.0 * 2/3 = 0.6667 QN
        let d = SyncDivision::Triplet(1.0).quarter_notes();
        assert!((d - 2.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn dotted_duration() {
        // 1/4. = 1.0 * 1.5 = 1.5 QN
        let d = SyncDivision::Dotted(1.0).quarter_notes();
        assert!((d - 1.5).abs() < 1e-10);
    }
}
