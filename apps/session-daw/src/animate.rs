//! Every parameter moving at once, for measurement.
//!
//! The mixer's controls are drawn live so that a mute can change
//! without the mixer being re-recorded. Whether that is actually cheap
//! is a claim, and a claim about a renderer is worth exactly as much as
//! the frame that proves it.
//!
//! So this drives every parameter on every track — mutes and solos
//! toggling, arms flipping, faders sweeping, pans crossing — at rates
//! chosen to be worse than a person can produce. If the live pass holds
//! its frame rate against that, it holds against anything a user does.
//!
//! Deliberately NOT random. A stress test that cannot be replayed is a
//! stress test that cannot be bisected: the same `t` gives the same
//! session state on every run and every machine, so two measurements
//! are of the same work.

use daw_proto::Track;

/// Drive every track's parameters to where they are at `t`.
///
/// `t` runs 0..1 over the phase. Each parameter moves at its own rate
/// and each track is offset from the next, so no two controls are ever
/// in step — a mixer where every fader moves together is one long
/// gradient the GPU can predict, and predicting it is exactly what a
/// stress test must not let it do.
pub fn drive(tracks: &mut [Track], t: f64) {
    for (i, track) in tracks.iter_mut().enumerate() {
        let n = crate::num::coord(i);
        // Phase-shifted per track, by an irrational-ish step so the
        // pattern does not repeat across any run of tracks.
        let phase = n * 0.6180339887;

        // Faders sweep their whole range twice per phase.
        let sweep = (std::f64::consts::TAU * (t * 2.0 + phase)).sin();
        track.volume = ((sweep + 1.0) / 2.0).clamp(0.0, 1.0) * 2.0;

        // Pan crosses at a different rate, so a strip's two continuous
        // controls are never in step with each other either.
        track.pan = (std::f64::consts::TAU * (t * 3.0 + phase)).cos();

        // The toggles switch far faster than a person can click: about
        // twelve times a phase, each track on its own beat.
        track.muted = toggles(t, phase, 12.0);
        track.soloed = toggles(t, phase * 1.7, 9.0);
        track.armed = toggles(t, phase * 2.3, 7.0);
    }
}

/// Meter levels for `n` tracks at `t`.
///
/// Separate from [`drive`] because meters are not track state: they
/// arrive on their own subscription, indexed by project track, and the
/// engine publishes them at about 30 Hz whatever the frame rate is.
///
/// Driven per FRAME here anyway, which is four times faster than the
/// engine will ever move them. A stress test measures the case that
/// cannot happen so that the case that can is covered by it.
#[must_use]
pub fn meters(n: usize, t: f64) -> Vec<daw_proto::TrackLevels> {
    (0..n)
        .map(|i| {
            let phase = crate::num::coord(i) * 0.6180339887;
            // Fast, and out of step with the fader on the same strip:
            // a meter that tracked its own fader would let the GPU
            // predict both from one gradient.
            let wave = (std::f64::consts::TAU * (t * 7.0 + phase)).sin();
            let peak = crate::mcp::f64_to_f32(((wave + 1.0) / 2.0).clamp(0.0, 1.0));
            daw_proto::TrackLevels {
                peak_left: peak,
                // The right channel lags, so the two are never equal
                // and `max` actually has to choose.
                peak_right: peak * 0.8,
                hold_left: peak,
                hold_right: peak,
            }
        })
        .collect()
}

/// A square wave: on for half of each cycle.
fn toggles(t: f64, phase: f64, rate: f64) -> bool {
    (t * rate + phase).fract() < 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracks(n: usize) -> Vec<Track> {
        (0..n).map(|_| Track::default()).collect()
    }

    /// Replayable: the same `t` is the same state, every time. Without
    /// this two measurements are not of the same work and comparing
    /// them means nothing.
    #[test]
    fn the_same_moment_gives_the_same_session() {
        let (mut a, mut b) = (tracks(16), tracks(16));
        drive(&mut a, 0.375);
        drive(&mut b, 0.375);
        for (x, y) in a.iter().zip(&b) {
            assert!((x.volume - y.volume).abs() < f64::EPSILON);
            assert!((x.pan - y.pan).abs() < f64::EPSILON);
            assert_eq!((x.muted, x.soloed, x.armed), (y.muted, y.soloed, y.armed));
        }
    }

    /// Everything actually moves over a phase — a stress test that
    /// left a parameter alone would be measuring the cheap case and
    /// reporting it as the expensive one.
    #[test]
    fn every_parameter_moves() {
        let mut seen = (
            std::collections::BTreeSet::new(),
            std::collections::BTreeSet::new(),
            std::collections::BTreeSet::new(),
        );
        let mut volumes = Vec::new();
        let mut pans = Vec::new();
        for step in 0..200 {
            let mut t = tracks(8);
            drive(&mut t, f64::from(step) / 200.0);
            volumes.push(t[0].volume);
            pans.push(t[0].pan);
            seen.0.insert(t[0].muted);
            seen.1.insert(t[0].soloed);
            seen.2.insert(t[0].armed);
        }
        assert_eq!(seen.0.len(), 2, "mute never changed");
        assert_eq!(seen.1.len(), 2, "solo never changed");
        assert_eq!(seen.2.len(), 2, "arm never changed");

        let spread = |v: &[f64]| {
            v.iter().copied().fold(f64::MIN, f64::max) - v.iter().copied().fold(f64::MAX, f64::min)
        };
        assert!(spread(&volumes) > 1.5, "the fader barely moved");
        assert!(spread(&pans) > 1.5, "pan barely moved");
    }

    /// No two tracks are in step. A mixer where every fader moves
    /// together is one long gradient, which is the cheap case and not
    /// the one worth measuring.
    #[test]
    fn tracks_are_out_of_phase_with_each_other() {
        let mut t = tracks(32);
        drive(&mut t, 0.2);
        let same = t
            .windows(2)
            .filter(|w| (w[0].volume - w[1].volume).abs() < 1e-6 && w[0].muted == w[1].muted);
        assert!(
            same.count() < 4,
            "too many neighbouring tracks share a state"
        );
    }

    /// Every value stays inside the range its control accepts, so the
    /// stress test measures drawing and never clamping.
    #[test]
    fn nothing_leaves_its_range() {
        for step in 0..100 {
            let mut t = tracks(8);
            drive(&mut t, f64::from(step) / 100.0);
            for track in &t {
                assert!((0.0..=2.0).contains(&track.volume), "{}", track.volume);
                assert!((-1.0..=1.0).contains(&track.pan), "{}", track.pan);
            }
        }
    }
}

#[cfg(test)]
mod meter_tests {
    use super::meters;

    /// Replayable like everything else here, and inside the range the
    /// meter accepts — a level above one would measure clamping.
    #[test]
    fn levels_are_replayable_and_in_range() {
        let a = meters(16, 0.375);
        let b = meters(16, 0.375);
        assert_eq!(a.len(), 16);
        for (x, y) in a.iter().zip(&b) {
            assert!((x.peak_left - y.peak_left).abs() < f32::EPSILON);
            assert!((0.0..=1.0).contains(&x.peak_left));
            assert!((0.0..=1.0).contains(&x.peak_right));
        }
    }

    /// And they move — a stress test with still meters would be
    /// measuring the cheap case.
    #[test]
    fn the_meters_move() {
        let seen: Vec<f32> = (0..64)
            .map(|s| meters(4, f64::from(s) / 64.0)[0].peak_left)
            .collect();
        let spread = seen.iter().copied().fold(f32::MIN, f32::max)
            - seen.iter().copied().fold(f32::MAX, f32::min);
        assert!(spread > 0.9, "the meter barely moved: {spread}");
    }
}
