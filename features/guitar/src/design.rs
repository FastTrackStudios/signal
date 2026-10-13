//! A guitar that isn't there — the signal design mode plays against.
//!
//! Every meter, analyser and telemetry surface in the rig UI is driven by a
//! real instrument, which makes them impossible to lay out without one: an
//! interface is exclusive, so a second copy of the app cannot open it, and a
//! silent input draws every surface at rest, which is the one state you do not
//! need to design for.
//!
//! So design mode plays this instead. Not noise: a plucked note every so often
//! with a decaying envelope, because what a meter has to be legible against is
//! transients — the attack that pins it and the decay that lets it fall. A
//! compressor's gain-reduction trace is meaningless without something for it to
//! reduce, and a spectrum with no harmonics tells you nothing about whether the
//! analyser's ballistics look right.
//!
//! Deterministic in time, so two windows side by side show the same thing and a
//! screenshot is reproducible.

/// How often the imaginary player plucks, seconds.
const PLUCK_EVERY: f32 = 1.4;

/// One frame of the fake instrument.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame {
    /// Peak input level, linear.
    pub input: f32,
    /// Peak output level, linear — a little hotter and a little flatter than
    /// the input, the way an amp and a compressor leave it.
    pub output: f32,
    /// Compressor gain reduction, dB positive.
    pub gain_reduction_db: f32,
}

/// The instrument at `t` seconds.
#[must_use]
pub fn frame(t: f32) -> Frame {
    let env = envelope(t);
    // The amp adds level and the compressor takes the top off, so the output
    // rides higher but with less range than the input — which is exactly the
    // relationship the two meters exist to show.
    let output = 0.35 + 0.5 * env.powf(0.6);
    Frame {
        input: env,
        output: output.clamp(0.0, 1.0),
        gain_reduction_db: gain_reduction_db(env),
    }
}

/// A plucked envelope: fast attack, exponential decay, never quite silent.
///
/// The floor matters — a real guitar under an amp is never at zero, and a meter
/// designed against true silence gets its noise floor wrong.
#[must_use]
pub fn envelope(t: f32) -> f32 {
    let phase = (t / PLUCK_EVERY).fract().abs();
    let since = phase * PLUCK_EVERY;
    // 8 ms attack, then a ~0.6 s decay.
    let attack = (since / 0.008).min(1.0);
    let decay = (-since / 0.6).exp();
    // Plucks vary: a player does not hit every note the same.
    let velocity = 0.55
        + 0.45
            * ((t / (PLUCK_EVERY * 3.0)) * std::f32::consts::TAU)
                .sin()
                .abs();
    (0.04 + 0.92 * attack * decay * velocity).clamp(0.0, 1.0)
}

/// Gain reduction for a given input: nothing until the threshold, then a ratio.
#[must_use]
pub fn gain_reduction_db(env: f32) -> f32 {
    // −18 dBFS threshold, 4:1 — the compressor's own defaults.
    let db = 20.0 * env.max(1e-5).log10();
    let over = db + 18.0;
    if over <= 0.0 {
        0.0
    } else {
        over * (1.0 - 1.0 / 4.0)
    }
}

/// A guitar-shaped spectrum in `bins` log-spaced bands, dBFS.
///
/// A body resonance low down, the fundamental and its harmonics, and the
/// high-end roll-off a cabinet imposes — enough shape that an analyser's
/// smoothing, decay and colour mapping can be judged.
#[must_use]
pub fn spectrum(t: f32, bins: usize) -> Vec<f32> {
    let env = envelope(t);
    let level = 20.0 * env.max(1e-5).log10();
    (0..bins)
        .map(|i| {
            let frac = if bins > 1 {
                i as f32 / (bins - 1) as f32
            } else {
                0.0
            };
            // 20 Hz … 20 kHz, log.
            let hz = 20.0 * 1000f32.powf(frac);
            // The cabinet: flat to ~4 kHz, then away steeply.
            let roll = if hz > 4000.0 {
                -24.0 * (hz / 4000.0).log10()
            } else {
                0.0
            };
            // Body resonance and the harmonic series of a low E.
            let mut shape = -6.0 * ((hz / 120.0).log10()).abs();
            for n in 1..=6 {
                let partial = 82.41 * n as f32;
                let width = partial * 0.06;
                let d = (hz - partial) / width;
                shape += 14.0 * (-d * d).exp() / n as f32;
            }
            // A little motion so nothing looks frozen between plucks.
            let shimmer = 1.5 * ((t * 3.0 + frac * 20.0) * std::f32::consts::TAU).sin();
            (level + roll + shape + shimmer - 6.0).clamp(-90.0, 0.0)
        })
        .collect()
}

/// A rolling window of `n` frames ending at `t`: `(input, gain reduction)`,
/// both 0..1, oldest first — the shape the compressor surface draws.
#[must_use]
pub fn traces(t: f32, n: usize) -> (Vec<f32>, Vec<f32>) {
    // ~4 seconds, the window the real telemetry keeps.
    let span = 4.0;
    let step = span / n.max(1) as f32;
    let mut input = Vec::with_capacity(n);
    let mut reduction = Vec::with_capacity(n);
    for i in 0..n {
        let at = t - (n - 1 - i) as f32 * step;
        let env = envelope(at.max(0.0));
        input.push(env);
        // Normalised against the compressor's own range, the way the live
        // trace arrives — 0..1 over 0..24 dB of reduction.
        reduction.push((gain_reduction_db(env) / 24.0).clamp(0.0, 1.0));
    }
    (input, reduction)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The envelope plucks: loud right after the attack, quiet before the next.
    #[test]
    fn it_plucks_rather_than_drones() {
        let attacked = envelope(0.02);
        let decayed = envelope(PLUCK_EVERY - 0.05);
        assert!(
            attacked > decayed * 3.0,
            "attack {attacked} should tower over the tail {decayed}"
        );
    }

    /// Never silent and never clipped — a meter designed against true zero
    /// gets its floor wrong, and one designed against a pinned input gets its
    /// headroom wrong.
    #[test]
    fn it_stays_inside_the_meter() {
        for i in 0..500 {
            let t = i as f32 * 0.01;
            let f = frame(t);
            assert!(f.input > 0.0 && f.input <= 1.0, "input {} at {t}", f.input);
            assert!(
                f.output > 0.0 && f.output <= 1.0,
                "output {} at {t}",
                f.output
            );
            assert!(f.gain_reduction_db >= 0.0);
        }
    }

    /// The compressor only works when there is something to work on.
    #[test]
    fn gain_reduction_follows_the_note() {
        assert_eq!(gain_reduction_db(0.0), 0.0, "silence is not compressed");
        let quiet = gain_reduction_db(0.05);
        let loud = gain_reduction_db(0.9);
        assert!(loud > quiet, "{loud} should exceed {quiet}");
    }

    /// The spectrum has the shape of an instrument: energy in the harmonics,
    /// far less above the cabinet's roll-off.
    #[test]
    fn the_spectrum_looks_like_a_guitar() {
        let bins = spectrum(0.02, 64);
        assert_eq!(bins.len(), 64);
        assert!(bins.iter().all(|d| (-90.0..=0.0).contains(d)));
        // Low-mid (harmonics) against the top octave (past the cabinet).
        let low: f32 = bins[4..24].iter().sum::<f32>() / 20.0;
        let high: f32 = bins[56..64].iter().sum::<f32>() / 8.0;
        assert!(low > high + 6.0, "low {low} should beat high {high}");
    }

    /// The traces are a window ending now, oldest first, both normalised.
    #[test]
    fn the_traces_roll() {
        let (input, reduction) = traces(6.0, 120);
        assert_eq!(input.len(), 120);
        assert_eq!(reduction.len(), 120);
        assert!(input.iter().all(|v| (0.0..=1.0).contains(v)));
        assert!(reduction.iter().all(|v| (0.0..=1.0).contains(v)));
        // The window moves: a later window is not the same window.
        assert_ne!(traces(6.0, 32).0, traces(6.5, 32).0);
        // And it ends at now.
        let end = *input.last().expect("non-empty");
        assert!((end - envelope(6.0)).abs() < 1e-6);
    }

    /// Deterministic: two windows show the same instrument.
    #[test]
    fn it_is_the_same_guitar_every_time() {
        assert_eq!(frame(0.37), frame(0.37));
        assert_eq!(spectrum(0.37, 32), spectrum(0.37, 32));
    }
}
