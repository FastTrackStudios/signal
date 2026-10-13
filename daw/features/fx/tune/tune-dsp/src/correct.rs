//! Pitch-correction target computation.
//!
//! Given detected [`Note`]s and a target [`Scale`], compute the per-note pitch
//! offset (in cents) that snaps each note's median pitch to the nearest scale
//! degree, scaled by a `strength` amount. A `retune_ms` controls how fast the
//! correction moves — the classic "hard tune" vs "natural" knob. This produces
//! the *target*; the actual resampling/formant-preserving shift is delegated to
//! [`pitch_dsp`], so `tune` reuses the existing PSOLA/WSOLA engine rather than
//! reinventing it.

use crate::detect::midi_to_hz;
use crate::note::Note;

/// A musical scale as a 12-bit pitch-class mask (bit `n` = semitone `n` from the
/// root is in-scale).
#[derive(Clone, Copy, Debug)]
pub struct Scale {
    /// Root pitch class, 0 = C … 11 = B.
    pub root_pc: u8,
    /// Allowed pitch classes relative to the root (bit 0 = root).
    pub mask: u16,
}

impl Scale {
    /// Chromatic scale — snap to the nearest semitone (Auto-Tune's default).
    pub const CHROMATIC: Scale = Scale {
        root_pc: 0,
        mask: 0b1111_1111_1111,
    };

    /// Major scale on a root pitch class.
    pub fn major(root_pc: u8) -> Self {
        Self {
            root_pc,
            mask: 0b1010_1011_0101, // semitones 0,2,4,5,7,9,11 from the root
        }
    }

    /// Natural-minor scale on a root pitch class.
    pub fn minor(root_pc: u8) -> Self {
        Self {
            root_pc,
            mask: 0b0101_1010_1101, // W H W W H W W
        }
    }

    /// Build from a keyflow-style STEP interval pattern (e.g. major =
    /// `[2,2,1,2,2,2,1]`) — the bridge to `keyflow_proto::ScaleMode::
    /// interval_pattern()` without a keyflow dependency here.
    pub fn from_intervals(root_pc: u8, steps: &[u8]) -> Self {
        let mut mask: u16 = 1; // degree 1
        let mut pc = 0u16;
        for &step in steps {
            pc = (pc + step as u16) % 12;
            mask |= 1 << pc;
        }
        Self {
            root_pc: root_pc % 12,
            mask,
        }
    }

    /// Whether a pitch class (relative to C) is in the scale.
    pub fn contains_pc(&self, pc: u8) -> bool {
        let rel = (((pc as i32 - self.root_pc as i32) % 12) + 12) % 12;
        self.mask & (1 << rel) != 0
    }

    /// Snap with note-boundary hysteresis: while the input stays
    /// within `0.5 + hysteresis_cents/100` semitones of the PREVIOUS
    /// target, keep it — stops chatter between two targets at the
    /// boundary. `bypass_mask` (absolute pitch classes, bit 0 = C)
    /// marks classes to leave UNCORRECTED (blue notes): returns None.
    pub fn snap_hysteresis(
        &self,
        midi: f64,
        prev_target: Option<f64>,
        hysteresis_cents: f64,
        bypass_mask: u16,
    ) -> Option<f64> {
        let nearest_pc = ((midi.round() as i32 % 12) + 12) % 12;
        if bypass_mask & (1 << nearest_pc) != 0 {
            return None;
        }
        if let Some(prev) = prev_target {
            if (midi - prev).abs() <= 0.5 + hysteresis_cents / 100.0 {
                return Some(prev);
            }
        }
        Some(self.snap(midi))
    }

    /// Nearest in-scale MIDI note to a (float) MIDI pitch.
    pub fn snap(&self, midi: f64) -> f64 {
        let nearest = midi.round() as i32;
        // Search outward for the closest allowed pitch class.
        for d in 0..=6 {
            for &s in &[nearest - d, nearest + d] {
                let pc = (((s - self.root_pc as i32) % 12) + 12) % 12;
                if self.mask & (1 << pc) != 0 {
                    return s as f64;
                }
            }
        }
        nearest as f64
    }
}

/// Correction tuning.
#[derive(Clone, Copy, Debug)]
pub struct CorrectConfig {
    /// Correction strength, 0..1 (1 = fully snapped, 0 = untouched).
    pub strength: f64,
    /// Notes whose target offset is under this (cents) are left alone.
    pub deadband_cents: f64,
}

impl Default for CorrectConfig {
    fn default() -> Self {
        Self {
            strength: 1.0,
            deadband_cents: 5.0,
        }
    }
}

/// A per-note correction directive the shifter consumes.
#[derive(Clone, Copy, Debug)]
pub struct NoteCorrection {
    /// Note this applies to (frame range echoed for convenience).
    pub start_frame: usize,
    /// Last frame index (inclusive).
    pub end_frame: usize,
    /// Detected median pitch, MIDI.
    pub detected_midi: f64,
    /// Snapped target pitch, MIDI.
    pub target_midi: f64,
    /// Pitch-shift ratio to apply (target_hz / detected_hz), 1.0 = no shift.
    pub ratio: f64,
    /// Applied correction in cents (signed), after strength/deadband.
    pub applied_cents: f64,
}

/// Compute corrections for a set of notes against a scale.
pub fn correct_notes(notes: &[Note], scale: Scale, cfg: CorrectConfig) -> Vec<NoteCorrection> {
    notes
        .iter()
        .map(|n| {
            let snapped = scale.snap(n.median_midi);
            let full_delta = snapped - n.median_midi; // semitones
            let applied = full_delta * cfg.strength;
            let applied_cents = applied * 100.0;
            let (target_midi, ratio, applied_cents) = if applied_cents.abs() < cfg.deadband_cents {
                (n.median_midi, 1.0, 0.0)
            } else {
                let target = n.median_midi + applied;
                let ratio = midi_to_hz(target) / midi_to_hz(n.median_midi);
                (target, ratio, applied_cents)
            };
            NoteCorrection {
                start_frame: n.start_frame,
                end_frame: n.end_frame,
                detected_midi: n.median_midi,
                target_midi,
                ratio,
                applied_cents,
            }
        })
        .collect()
}
