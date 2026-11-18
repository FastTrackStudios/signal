//! Deterministic per-note variation -- hash-based pseudo-random offsets.
//!
//! Each physical reed on a real Wurlitzer has slightly different tuning,
//! solder mass, and mounting characteristics. This module provides fixed
//! per-note offsets so note 60 always sounds the same, but differs
//! slightly from note 61.
use crate::tables::NUM_MODES;

/// Simple deterministic hash: takes MIDI note + seed, returns 0.0..1.0.
fn hash_f64(midi: u8, seed: u32) -> f64 {
    let mut h: u32 = 2166136261;
    h ^= midi as u32;
    h = h.wrapping_mul(16777619);
    h ^= seed;
    h = h.wrapping_mul(16777619);
    h ^= h >> 16;
    h = h.wrapping_mul(2654435769);
    (h & 0x00FF_FFFF) as f64 / 16777216.0
}

/// Frequency detuning factor for a note: multiplier in range [1-max, 1+max].
/// max = 0.00173 (+/-3 cents), matching factory tuning tolerance per community
/// standard and US Patent 2,919,616 (Andersen, 1960) manufacturing specs.
// TODO: A "Condition" parameter could scale this from ±3 cents (factory fresh)
// to ±14 cents (unserviced vintage) for character variation.
pub fn freq_detune(midi: u8) -> f64 {
    let r = hash_f64(midi, 0xDEAD) * 2.0 - 1.0;
    1.0 + r * 0.00173
}

/// Per-mode amplitude variation factors: multipliers in range [1-max, 1+max].
/// max = 0.08 (+/-8%).
pub fn mode_amplitude_offsets(midi: u8) -> [f64; NUM_MODES] {
    std::array::from_fn(|i| {
        let r = hash_f64(midi, 0xBEEF + i as u32) * 2.0 - 1.0;
        1.0 + r * 0.08
    })
}
