//! FTS Chorus — multi-engine chorus/flanger/vibrato.
//!
//! Five distinct chorus engines covering clean to experimental:
//! - **Cubic**: Clean Catmull-Rom interpolation (default, transparent)
//! - **BBD**: Bucket-brigade device emulation (vintage analog)
//! - **Tape**: Wow/flutter/saturation (warm tape character)
//! - **Orbit**: Dual-tap elliptical orbital modulation (experimental, spatial)
//! - **Juno**: Triangle LFO + allpass interpolation (classic Roland Juno-60)
//!
//! Each engine can operate in Chorus, Flanger, or Vibrato mode.
//!
//! Credits:
//! - Cubic interpolation: standard Catmull-Rom (fts-dsp)
//! - BBD topology: Choroboros (EsotericShadow), clock-driven S&H chain
//! - Tape modulation: ChowDSP AnalogTapeModel (wow/flutter), qdelay (tiagolr)
//! - Orbit modulation: Choroboros (EsotericShadow), elliptical 2D LFO
//! - Juno: TAL-NoiseMaker / YKChorus (SpotlightKid), allpass delay + DC block

pub mod chain;
pub mod engine;

/// Display helpers — what the modulation actually does, sampled from the
/// engines themselves.
///
/// A modulator's picture is its movement over time, and the temptation is to
/// draw a sine and be done. That is a picture of a chorus rather than of
/// *this* chorus: the tape engine's line wanders on wow and flutter that are
/// not locked to the rate at all, the orbit engine traces an ellipse whose
/// projection depends on a second slow rotation, and the Juno's is a triangle
/// through an allpass. None of those are sine waves and all of them are what
/// the listener hears.
///
/// So the shape is taken from a real voice: build one, run it, and read the
/// delay it actually chose each tick.
pub mod analysis {
    use crate::engine::{
        BbdVoice, ChorusEngine, CubicVoice, EffectType, EngineType, JunoVoice, OrbitVoice,
        TapeVoice,
    };

    /// Build one voice of `engine` at `phase_offset`, exactly as
    /// [`crate::chain::ChorusChain`] does.
    fn voice(engine: EngineType, phase_offset: f64) -> Box<dyn ChorusEngine> {
        match engine {
            EngineType::Cubic => Box::new(CubicVoice::new(phase_offset)),
            EngineType::Bbd => Box::new(BbdVoice::new(phase_offset)),
            EngineType::Tape => Box::new(TapeVoice::new(phase_offset)),
            EngineType::Orbit => Box::new(OrbitVoice::new(phase_offset)),
            EngineType::Juno => Box::new(JunoVoice::new(phase_offset)),
        }
    }

    /// The sample rate the shape is sampled at.
    ///
    /// Not the host's: a display only needs a couple of hundred points and
    /// running one cycle of a 0.05 Hz LFO at 48 kHz is a million ticks for a
    /// picture. It cannot be arbitrarily low either — the engines size their
    /// delay lines from it, and the Juno reallocates to `buf_len + 4`, so a
    /// pretend rate of a few hundred Hz leaves it with a four-sample buffer
    /// and a delay it cannot read. 4 kHz is comfortably above every engine's
    /// longest delay and cheap enough to run per frame.
    pub const SHAPE_RATE: f64 = 4_000.0;

    /// Longest run the shape will do, whatever the LFO rate. At the slowest
    /// rate on the dial this caps the picture at slightly less than one full
    /// cycle rather than costing a frame.
    const MAX_TICKS: usize = 300_000;

    /// Sample one LFO cycle of a voice's delay time into `out`, in ms.
    ///
    /// The voice is run for real, at [`SHAPE_RATE`], for as many ticks as one
    /// cycle of `rate_hz` takes — then subsampled into `out`. The input is
    /// silence, because the delay a voice reads at does not depend on what is
    /// going through it.
    ///
    /// Engines with their own free-running motion (tape's wow and flutter,
    /// orbit's second rotation) are *not* locked to that cycle and will not
    /// close the loop exactly. That is not an artefact of the sampling — it is
    /// the thing those engines are for, and the panel should show it.
    // One knob per argument; a struct here would just move the list.
    #[allow(clippy::too_many_arguments)]
    pub fn delay_cycle(
        engine: EngineType,
        effect: EffectType,
        rate_hz: f64,
        depth: f64,
        color: f64,
        feedback: f64,
        phase_offset: f64,
        out: &mut [f64],
    ) {
        let n = out.len().max(2);
        let rate = rate_hz.clamp(1.0e-3, SHAPE_RATE * 0.25);
        let ticks = ((SHAPE_RATE / rate) as usize).clamp(n, MAX_TICKS);
        let mut v = voice(engine, phase_offset);
        v.update(SHAPE_RATE);
        v.reset();
        let mut cursor = 0usize;
        for i in 0..ticks {
            v.tick(0.0, rate, depth, feedback, color, effect);
            // Subsample: take the point whenever the output index advances.
            let want = i * n / ticks;
            if want == cursor && cursor < n {
                out[cursor] = v.delay_ms();
                cursor += 1;
            }
        }
        // A short run can leave the tail unfilled; hold the last value.
        let last = out.get(cursor.saturating_sub(1)).copied().unwrap_or(0.0);
        for slot in out.iter_mut().skip(cursor) {
            *slot = last;
        }
    }
}
