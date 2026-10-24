//! Time effects locked to the tempo, on the rig's side: a delay locks
//! itself (the engine derives its time from `tempo_bpm` and its tap
//! division); a reverb is given the decay that rings for its beats at the
//! tempo — `sync_div` ([`REVERB_BEATS`]) — and is given it again whenever
//! the tempo or its algorithm changes.

pub use crate::proto::time::{MOD_BEATS, REVERB_BEATS};
use signal_proto::BlockType;

/// A block whose rate is a modulation's (the tempo can lock it).
#[must_use]
pub fn is_modulation(bt: BlockType) -> bool {
    matches!(
        bt,
        BlockType::Chorus
            | BlockType::Flanger
            | BlockType::Vibrato
            | BlockType::Trem
            | BlockType::Phaser
            | BlockType::Rotary
    )
}

/// The rate (Hz) for a modulation locked to `sync_div` at `bpm` — a cycle
/// in its beats — or `None` when it runs free.
#[must_use]
pub fn mod_synced_rate(sync_div: f32, bpm: f32) -> Option<f32> {
    let beats = MOD_BEATS
        .get(sync_div.round().max(0.0) as usize)
        .map(|b| b.1)
        .filter(|b| *b > 0.0)?;
    (bpm > 0.0).then(|| (f64::from(bpm) / 60.0 / beats) as f32)
}

/// The Hall algorithm's feedback law (g = 0.5 + 0.48·d over an ~80 ms
/// loop): the tail of an algorithm whose decay is not a calibrated time.
fn estimate_seconds(decay: f64) -> f64 {
    let g = 0.48f64.mul_add(decay.clamp(0.0, 1.0), 0.5).min(0.995);
    0.08 * (0.001f64).ln() / g.ln()
}

/// A reverb's tail (RT60, seconds) at `decay`: the engine's own law where
/// its decay is a calibrated time, the estimate otherwise.
#[must_use]
pub fn reverb_seconds(algorithm: f32, variant: f32, decay: f32) -> f64 {
    let alg =
        dsp::reverb::algorithm::AlgorithmType::from_index(algorithm.round().max(0.0) as usize);
    dsp::reverb::algorithm::decay_seconds(alg, variant.round().max(0.0) as usize, f64::from(decay))
        .unwrap_or_else(|| estimate_seconds(f64::from(decay)))
}

/// The decay that rings for `seconds`: [`reverb_seconds`] inverted (it
/// rises with the decay), held to the decay's range.
#[must_use]
pub fn reverb_decay_for(algorithm: f32, variant: f32, seconds: f64) -> f32 {
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if reverb_seconds(algorithm, variant, mid) < seconds {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// The decay for a reverb locked to `sync_div` at `bpm`, or `None` when it
/// runs free.
#[must_use]
pub fn reverb_synced_decay(algorithm: f32, variant: f32, sync_div: f32, bpm: f32) -> Option<f32> {
    let beats = REVERB_BEATS
        .get(sync_div.round().max(0.0) as usize)
        .map(|b| b.1)
        .filter(|b| *b > 0.0)?;
    (bpm > 0.0).then(|| reverb_decay_for(algorithm, variant, beats * 60.0 / f64::from(bpm)))
}
