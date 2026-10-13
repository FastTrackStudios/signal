//! Audio that isn't there, so the panels that show audio can be seen.
//!
//! Three things in the rack only move when signal moves: the mixer's
//! meters, the EQ's spectrum and the compressor's waveform. A template
//! session has no media, a session being laid out has nothing playing,
//! and a machine with no audio device has nothing at all — so the
//! panels that exist to show a signal are the panels you cannot look
//! at while you build them.
//!
//! This is a signal. Not a recording and not noise: a deterministic
//! function of `(track, seconds)` that behaves enough like a drum kit
//! to exercise every path a real one would. Deterministic because a
//! picture you cannot reproduce is a picture you cannot compare — the
//! same second gives the same frame on every machine, which is what
//! lets a screenshot be a test.
//!
//! # It is not a fake meter
//!
//! It feeds the SAME inputs the engine feeds: a `TrackLevels` for the
//! meters and a dB-per-bin spectrum for the EQ. Nothing downstream
//! knows the difference, which is the only way a simulation is worth
//! having — one that took a shortcut past the real path would prove
//! the shortcut works.

use std::f64::consts::TAU;

use crate::live::Meters;

/// How many bins the spectrum carries.
///
/// Log-spaced across the audible range by the painter, so this is a
/// resolution rather than a bandwidth: 96 puts about a bin every eighth
/// of an octave, which is finer than a strip's EQ panel can draw and
/// about right for one at a focus width.
pub const BINS: usize = 96;

/// One track's simulated signal at one instant.
#[derive(Clone, Debug)]
pub struct Frame {
    /// The peak, linear 0..1 — what a meter and the compressor's
    /// waveform read.
    pub peak: f32,
    /// The spectrum, in dB, one value per bin.
    pub spectrum: Vec<f32>,
}

/// The kind of source a track stands in for.
///
/// Drawn from its INDEX rather than its name, so the simulation is
/// stable across a rename and works on a session whose tracks are
/// called things this file has never heard of. The point is variety
/// that looks like a kit, not a classifier.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Voice {
    /// Low, slow, loud — a kick or a floor tom.
    Low,
    /// Mid-forward with a fast decay — a snare.
    Mid,
    /// Bright and busy — cymbals and overheads.
    High,
    /// Broad and steady — a room, a bus, a pad.
    Broad,
}

impl Voice {
    const fn of(track: usize) -> Self {
        match track % 4 {
            0 => Self::Low,
            1 => Self::Mid,
            2 => Self::High,
            _ => Self::Broad,
        }
    }

    /// Where its energy sits, in Hz.
    const fn centre(self) -> f64 {
        match self {
            Self::Low => 80.0,
            Self::Mid => 400.0,
            Self::High => 6_000.0,
            Self::Broad => 800.0,
        }
    }

    /// How wide that peak is, in octaves.
    const fn width(self) -> f64 {
        match self {
            Self::Low => 1.2,
            Self::Mid => 1.6,
            Self::High => 2.2,
            Self::Broad => 3.5,
        }
    }

    /// How many times a second it is struck.
    const fn rate(self) -> f64 {
        match self {
            Self::Low => 2.0,
            Self::Mid => 1.0,
            Self::High => 4.0,
            Self::Broad => 0.5,
        }
    }

    /// Where this voice RINGS: two narrow peaks that are always there —
    /// a drum shell's mode, a body resonance, a room's — in Hz. What a
    /// resonance suppressor exists to find, and what the simulation
    /// had none of, so its panel could not show a thing.
    const fn rings(self) -> [f64; 2] {
        match self {
            Self::Low => [240.0, 3_100.0],
            Self::Mid => [900.0, 2_400.0],
            Self::High => [1_200.0, 7_500.0],
            Self::Broad => [420.0, 5_000.0],
        }
    }

    /// Whether this voice has sibilance: a burst of energy at 5–9 kHz
    /// on each hit, which is what a de-esser fires on.
    const fn sibilant(self) -> bool {
        matches!(self, Self::Mid | Self::High)
    }

    /// And how fast it decays, as a share of its own period.
    const fn decay(self) -> f64 {
        match self {
            Self::Low => 0.35,
            Self::Mid => 0.25,
            Self::High => 0.12,
            Self::Broad => 0.9,
        }
    }
}

/// The signal on `track` at `seconds`.
///
/// Pure: the same arguments give the same frame forever, on every
/// machine. That is what lets a screenshot of a moving panel be
/// compared against another one.
#[must_use]
pub fn frame(track: usize, seconds: f64) -> Frame {
    let voice = Voice::of(track);
    let envelope = envelope(voice, track, seconds);
    Frame {
        peak: crate::mcp::f64_to_f32(envelope),
        spectrum: spectrum(voice, envelope, seconds),
    }
}

/// A struck envelope: a sharp attack every period, decaying after it.
///
/// Offset per track by an irrational-ish step so no two tracks land on
/// the same beat — a mixer where every meter jumps together is one
/// gradient, and the thing being looked at is how tracks differ.
/// The level of a track's audio at a moment, linear 0..1 — what an
/// item's waveform is drawn from until peaks stream in from the
/// engine. The same envelope the meters run on, so the picture in
/// the lane and the meter on the strip agree.
#[must_use]
pub fn waveform(track: usize, seconds: f64) -> f64 {
    envelope(Voice::of(track), track, seconds)
}

fn envelope(voice: Voice, track: usize, seconds: f64) -> f64 {
    let period = 1.0 / voice.rate();
    let offset = crate::num::coord(track) * 0.6180339887 * period;
    let since = (seconds + offset).rem_euclid(period) / period;
    // A quiet floor under it, so a meter never reads absolute silence
    // between hits — a room mic does not, and a display that drops to
    // nothing between beats reads as a dropout.
    //
    // The numbers are in LINEAR peak because that is what a meter
    // takes, and the displays log-scale them: a floor of 0.004 is −48
    // dBFS and a hit of 0.7 is −3, which is the range a recorded track
    // actually covers. A floor of 0.04 put every trace above half the
    // display and turned the compressor's waveform into a wall.
    let hit = (-since / voice.decay()).exp();
    let floor = 0.004;
    // A slow swell over the whole simulation, so a long look is not a
    // loop: the kit gets louder and quieter the way a performance does.
    let swell = 0.75 + 0.25 * (TAU * seconds / 19.0).sin();
    (floor + hit * 0.7 * swell).clamp(0.0, 1.0)
}

/// The spectrum at this instant, in dB per bin.
///
/// A tilted floor with the voice's own resonance on it, and a wobble
/// so the picture is alive between hits. Tilted because real programme
/// material is: energy falls with frequency, and a flat spectrum reads
/// as noise rather than as music.
fn spectrum(voice: Voice, envelope: f64, seconds: f64) -> Vec<f32> {
    spectrum_with(voice, envelope, Some(seconds))
}

/// How wide a ring is, in octaves — a quarter, which is two of the
/// analyser's bins: narrow enough to stand proud of a third-octave
/// average the way a real mode does, wide enough to land on a bin.
const RING_OCTAVES: f64 = 0.25;

/// How far above its neighbourhood a ring stands, in dB.
const RING_DB: f64 = 14.0;

/// The spectrum, with or without the moment's shimmer.
///
/// `None` is the spectrum's own long-term average: the shimmer is a
/// sine in time and averages to nothing, so what is left is the tilt
/// and the voice's resonance at the envelope given — which is what a
/// suppressor that has learned for three seconds is comparing against.
fn spectrum_with(voice: Voice, envelope: f64, shimmer_at: Option<f64>) -> Vec<f32> {
    let (low, high) = (20.0_f64, 20_000.0_f64);
    let (log_low, log_high) = (low.log10(), high.log10());
    let centre = voice.centre().log10();
    let width = voice.width() * 0.301;
    (0..BINS)
        .map(|i| {
            let t = crate::num::coord(i) / crate::num::coord(BINS.saturating_sub(1).max(1));
            let log_f = t.mul_add(log_high - log_low, log_low);
            // The tilt: programme material loses energy with
            // frequency, and a flat spectrum reads as noise rather
            // than as music. Gentle, because a steeper one buries
            // every voice's own resonance under the bottom octave and
            // every track looks like a kick.
            let tilt = (log_low - log_f) * 5.0;
            // The voice's resonance, as a bell in log-frequency.
            let away = (log_f - centre) / width;
            let bump = 18.0 * (-away * away).exp();
            // Its rings: narrow, fixed, always there.
            let rings: f64 = voice
                .rings()
                .iter()
                .map(|hz| {
                    let off = (log_f - hz.log10()) / (RING_OCTAVES * 0.301);
                    RING_DB * (-off * off).exp()
                })
                .sum();
            // Sibilance: a burst on the hit, in the de-esser's band,
            // that fades over the first fifth of the envelope. An
            // event, so it is part of the moment and not of the
            // long-term spectrum.
            let hiss = shimmer_at.map_or(0.0, |_| {
                if !voice.sibilant() {
                    return 0.0;
                }
                let off = (log_f - 7_000.0_f64.log10()) / (0.25 * 0.301);
                let burst = ((envelope - 0.25) / 0.45).clamp(0.0, 1.0);
                12.0 * burst * (-off * off).exp()
            });
            // And a shimmer that moves, different per bin, so the
            // spectrum is never two frames the same.
            let shimmer = shimmer_at.map_or(0.0, |seconds| {
                3.5 * (TAU * seconds.mul_add(1.7, t * 9.0)).sin()
            });
            // Sat on the display's own window rather than on absolute
            // dBFS. A strip's EQ panel shows ±18 dB; a spectrum drawn
            // in real dBFS puts everything but the loudest peak on the
            // floor, which reads as one spike on a flat line rather
            // than as the shape of a sound.
            let loud = 24.0 * envelope.max(1e-3).log10() / 3.0;
            let level = tilt + bump + rings + hiss + shimmer + loud;
            crate::mcp::f64_to_f32(level.clamp(-40.0, 16.0))
        })
        .collect()
}

/// Everything in `track`'s rack that moves, at `seconds`, as the engine
/// would publish it.
///
/// The suppressors' reduction is the one place this is a STAND-IN
/// rather than the engine's own number. The real curve is
/// `eq_dsp::dynamics::spectral::SpectralEngine::gain_curve`, and it
/// needs audio; a simulation has a spectrum and no samples, so it
/// approximates the engine's rule — a bin stands proud of its
/// neighbourhood by more than the threshold — on the spectrum it has.
/// Once the rack is bound to a chain this function is replaced, not
/// consulted: the drawing reads [`Meters`] and does not know which.
#[must_use]
pub fn meters(track: usize, seconds: f64, tone: &crate::tone::Tone) -> Meters {
    const TAPS: usize = 8;
    let voice = Voice::of(track);
    let now = frame(track, seconds);
    // The settled curve: the same rule over the spectrum's own long-term
    // average, which is what a three-second learn comes to. The shimmer
    // averages out and the resonances stay, which is the point of the
    // settled curve — and it is one spectrum, not eight: this runs for
    // every track every meter frame, and the bench counts it.
    // The wet returns: what the delay and reverb are putting out is
    // what went in earlier, scaled by how much of it survives. A
    // repeat is the peak one delay time ago times the feedback, and so
    // on down the series; a tail is the recent peaks weighted by an
    // exponential to −60 dB over the decay. Envelopes only — the
    // spectrum is not needed for a level, and building it would be.
    let delay_wet = {
        let time = f64::from(tone.delay.time.max(1.0)) / 1000.0;
        let feedback = f64::from(tone.delay.feedback.clamp(0.0, 0.99));
        let mut level = feedback;
        let mut wet = 0.0_f64;
        for k in 1..=6 {
            wet = wet
                .max(envelope(voice, track, crate::num::coord(k).mul_add(-time, seconds)) * level);
            level *= feedback;
        }
        crate::mcp::f64_to_f32(wet * f64::from(tone.delay.mix.clamp(0.0, 1.0)))
    };
    let reverb_wet = {
        let decay = f64::from(tone.reverb.decay.max(0.05));
        let mut wet = 0.0_f64;
        for k in 1..=TAPS {
            let into = crate::num::coord(k) / crate::num::coord(TAPS);
            let level = 10.0_f64.powf(-3.0 * into);
            wet = wet.max(envelope(voice, track, into.mul_add(-decay, seconds)) * level);
        }
        crate::mcp::f64_to_f32(wet * f64::from(tone.reverb.mix.clamp(0.0, 1.0)))
    };
    let (ess_db, ess_ref_db) = band_levels(&now.spectrum, tone.de_ess);
    Meters {
        sat_peak: now.peak,
        deess_db: suppression(&now.spectrum, tone.de_ess),
        ess_db,
        ess_ref_db,
        delay_wet,
        reverb_wet,
        spectrum: now.spectrum,
    }
}

/// The analyser's bin centres, in Hz, built once.
fn bin_hz() -> &'static [f64; BINS] {
    static HZ: std::sync::OnceLock<[f64; BINS]> = std::sync::OnceLock::new();
    HZ.get_or_init(|| {
        let full = (20_000.0_f64 / 20.0).log10();
        let last = crate::num::coord(BINS.saturating_sub(1).max(1));
        let mut out = [0.0_f64; BINS];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = 20.0 * 10.0_f64.powf(crate::num::coord(i) / last * full);
        }
        out
    })
}

/// The de-esser's band as two numbers: the peak inside it, and the
/// average of the octave either side of it — what the peak is judged
/// against. The trace in the strip is these two over time.
fn band_levels(spectrum: &[f32], set: crate::tone::Suppress) -> (f32, f32) {
    let (low, high) = (f64::from(set.low), f64::from(set.high));
    let mut peak = -60.0_f32;
    let (mut sum, mut count) = (0.0_f64, 0.0_f64);
    for (hz, db) in bin_hz().iter().zip(spectrum) {
        if (low..=high).contains(hz) {
            peak = peak.max(*db);
        } else if (low / 2.0..high * 2.0).contains(hz) {
            sum += f64::from(*db);
            count += 1.0;
        }
    }
    (peak, crate::mcp::f64_to_f32(sum / count.max(1.0)))
}

/// The suppressor's rule, applied to a spectrum: how much comes off
/// each bin, in dB.
///
/// A bin is compared with the average of its neighbourhood over
/// `sharpness` of an octave; what stands proud of that by more than the
/// threshold is cut by `depth` of the excess, inside the band only, and
/// the cut is smoothed over a few bins because a filter with finite Q
/// cannot cut one frequency and not the one beside it.
#[must_use]
pub fn suppression(spectrum: &[f32], set: crate::tone::Suppress) -> Vec<f32> {
    if spectrum.len() < 4 {
        return Vec::new();
    }
    let bins = crate::num::coord(spectrum.len().saturating_sub(1).max(1));
    let full = (20_000.0_f64 / 20.0).log10();
    // The bins' centre frequencies, worked out once: three suppressions
    // per track per meter frame, each over ninety-six bins, is a few
    // hundred thousand `powf`s a second that never change.
    let table = bin_hz();
    let hz_at = |i: usize| {
        if spectrum.len() == BINS {
            table.get(i).copied().unwrap_or(20_000.0)
        } else {
            20.0 * 10.0_f64.powf(crate::num::coord(i) / bins * full)
        }
    };
    // The baseline the peaks are judged against: the spectrum's own
    // average over a span set by the sharpness. Wide, because it has
    // to IGNORE the peaks — narrow, it tracks them, and then everything
    // stands the same tiny amount proud of its own neighbourhood.
    //
    // The bins are log-spaced, so a span in octaves is a fixed number
    // of bins either side, and the average is a running window rather
    // than a resample per bin. This runs for every track on every
    // meter frame; it has to be linear in the bins.
    let octaves = f64::from(set.sharpness).mul_add(-0.9, 1.2);
    let bins_per_octave = bins / (full / 2.0_f64.log10());
    let half = crate::num::index((octaves / 2.0 * bins_per_octave).round()).max(1);
    // Prefix sums, so a window is two reads whatever its width.
    let mut prefix = Vec::with_capacity(spectrum.len().saturating_add(1));
    prefix.push(0.0_f64);
    for db in spectrum {
        let last = prefix.last().copied().unwrap_or(0.0);
        prefix.push(last + f64::from(*db));
    }
    // The neighbourhood WITHOUT the bin and its two neighbours: a peak
    // compared against an average that includes it is a peak compared
    // against half of itself, and a fourteen-decibel ring came out two
    // decibels proud. What surrounds a resonance is what it is judged
    // against.
    let window = |from: usize, to: usize| {
        prefix.get(to.saturating_add(1)).copied().unwrap_or(0.0)
            - prefix.get(from).copied().unwrap_or(0.0)
    };
    let last = spectrum.len().saturating_sub(1);
    let average_at = |i: usize| {
        let from = i.saturating_sub(half);
        let to = i.saturating_add(half).min(last);
        let inner_from = i.saturating_sub(1);
        let inner_to = i.saturating_add(1).min(last);
        let sum = window(from, to) - window(inner_from, inner_to);
        let count = to
            .saturating_sub(from)
            .saturating_add(1)
            .saturating_sub(inner_to.saturating_sub(inner_from).saturating_add(1));
        if count == 0 {
            f64::from(spectrum.get(i).copied().unwrap_or(0.0))
        } else {
            sum / crate::num::coord(count)
        }
    };
    // The cut, as prefix sums too — so the skirt below is two reads per
    // bin and the whole thing is three allocations. It runs for every
    // track on every meter frame.
    let (low, high) = (f64::from(set.low), f64::from(set.high));
    let mut cut = Vec::with_capacity(spectrum.len().saturating_add(1));
    cut.push(0.0_f64);
    for (i, db) in spectrum.iter().enumerate() {
        let hz = hz_at(i);
        let proud = if hz < low || hz > high {
            0.0
        } else {
            (f64::from(*db) - average_at(i) - f64::from(set.threshold)).max(0.0)
                * f64::from(set.depth)
        };
        let last = cut.last().copied().unwrap_or(0.0);
        cut.push(last + proud);
    }
    // The filter's skirt: a one-bin peak produces a dip with shoulders
    // rather than a square notch nobody can build.
    (0..spectrum.len())
        .map(|i| {
            let from = i.saturating_sub(SMOOTH);
            let to = i
                .saturating_add(SMOOTH)
                .min(spectrum.len().saturating_sub(1));
            let sum = cut.get(to.saturating_add(1)).copied().unwrap_or(0.0)
                - cut.get(from).copied().unwrap_or(0.0);
            crate::mcp::f64_to_f32(
                sum / crate::num::coord(to.saturating_sub(from).saturating_add(1)),
            )
        })
        .collect()
}

/// How many bins either side a simulated cut is smoothed over.
///
/// One: a filter's skirt, not a blur. Two averaged a one-bin notch
/// down to a fifth of itself, and the teeth it should have grown never
/// cleared the half-decibel that earns one.
const SMOOTH: usize = 1;

#[cfg(test)]
mod tests {
    use super::{BINS, frame, meters, suppression};

    /// Deterministic, which is what makes a screenshot of a moving
    /// panel worth comparing to another one.
    #[test]
    fn the_same_moment_is_the_same_signal() {
        let a = frame(3, 1.25);
        let b = frame(3, 1.25);
        assert!((a.peak - b.peak).abs() < f32::EPSILON);
        assert_eq!(a.spectrum, b.spectrum);
    }

    /// It moves — a still simulation would exercise none of the paths
    /// it exists to exercise.
    #[test]
    fn it_moves() {
        let peaks: Vec<f32> = (0..60)
            .map(|i| frame(0, f64::from(i) / 30.0).peak)
            .collect();
        let spread = peaks.iter().copied().fold(f32::MIN, f32::max)
            - peaks.iter().copied().fold(f32::MAX, f32::min);
        assert!(spread > 0.4, "the meter barely moved: {spread}");

        let one = frame(0, 0.0).spectrum;
        let two = frame(0, 0.3).spectrum;
        assert_ne!(one, two, "the spectrum stood still");
    }

    /// No two tracks are in step, so a mixer of them is a mixer rather
    /// than one gradient.
    #[test]
    fn tracks_differ_from_each_other() {
        let at = |track| frame(track, 0.4).peak;
        let peaks: Vec<f32> = (0..8).map(at).collect();
        let same = peaks
            .windows(2)
            .filter(|w| (w[0] - w[1]).abs() < 1e-4)
            .count();
        assert!(same < 2, "too many tracks share a level: {peaks:?}");
    }

    /// Every value stays in the range its consumer accepts, so what is
    /// measured is drawing and never clamping.
    #[test]
    fn nothing_leaves_its_range() {
        for step in 0..120 {
            let f = frame(step % 7, crate::num::coord(step) / 20.0);
            assert!((0.0..=1.0).contains(&f.peak), "{}", f.peak);
            assert_eq!(f.spectrum.len(), BINS);
            for db in &f.spectrum {
                assert!((-40.0..=16.0).contains(db), "{db}");
            }
        }
    }

    /// The de-esser's rule finds the high voice's sibilance and leaves
    /// the kick alone; the settled curve is smoother than the instant
    /// one.
    #[test]
    fn the_suppressors_act_where_the_energy_is() {
        let tone = crate::tone::placeholder(2);
        let m = meters(2, 1.25, &tone);
        assert_eq!(m.spectrum.len(), BINS);
        assert_eq!(m.deess_db.len(), BINS);
        let ess = crate::tone::Suppress::sibilance();
        let high = suppression(&frame(2, 1.25).spectrum, ess);
        let low = suppression(&frame(0, 1.25).spectrum, ess);
        let deepest = |v: &[f32]| v.iter().copied().fold(0.0_f32, f32::max);
        assert!(
            deepest(&high) >= deepest(&low),
            "the cymbal should be the sibilant one"
        );
        // Outside the band nothing comes off.
        assert!(high[..20].iter().all(|v| *v <= f32::EPSILON));
        assert!((0.0..=1.0).contains(&m.delay_wet) && (0.0..=1.0).contains(&m.reverb_wet));
    }

    /// A voice's energy lands where its name says. A kick whose
    /// spectrum peaked at 6 kHz would look like a cymbal, and the
    /// point of the simulation is that a mixer of it looks like a kit.
    #[test]
    fn each_voice_peaks_in_its_own_register() {
        let brightest = |track: usize| {
            let s = frame(track, 0.0).spectrum;
            s.iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map_or(0, |(i, _)| i)
        };
        // Track 0 is the low voice and track 2 the high one.
        assert!(
            brightest(0) < brightest(2),
            "the low voice was brighter than the high one"
        );
    }
}

/// The simulated signal fed into a mixer, frame by frame — the engine
/// and the analysers it stands in for, for a window with nothing
/// playing (`FTS_SIMULATE=1`), and for the bench's pictures.
///
/// Fed through exactly what the engine feeds: each track's
/// [`TrackLevels`](daw_proto::TrackLevels) for its meter, and its rack's
/// level history and spectrum, at [`PUBLISH_HZ`](crate::tone::PUBLISH_HZ)
/// like the engine publishes. What it is fed is a function of the
/// seconds it is given, so the same second is the same picture.
#[derive(Debug, Default)]
pub struct Feed {
    /// The last publish fed to each track's rack, by guid: a strip fed
    /// only while it is on screen catches up from here when it comes back.
    published: std::collections::HashMap<String, usize>,
}

impl Feed {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything up to `seconds`, for the rows in `shown` — the strips on
    /// screen; nothing else is drawn, so nothing else is fed. Each rack's
    /// history gets every publish since it was last fed (a first feed, or
    /// one after more than a history's worth off screen, fills it whole,
    /// ending at `seconds`); its spectrum and the returned meter levels —
    /// indexed by project track index — are the signal at `seconds`.
    pub fn feed(
        &mut self,
        seconds: f64,
        rows: &[(daw_proto::Track, u32)],
        shown: std::ops::Range<usize>,
        settings: &crate::tone::Store,
        history: &mut std::collections::HashMap<String, crate::tone::Levels>,
        spectra: &mut std::collections::HashMap<String, crate::tone::Analyser>,
    ) -> Vec<daw_proto::TrackLevels> {
        let hz = f64::from(crate::tone::PUBLISH_HZ);
        // Publishes are whole frames of the clock: the n-th at n / hz.
        let now = crate::num::index(seconds.max(0.0) * hz);
        let oldest = now.saturating_sub(crate::tone::HISTORY - 1);
        let mut levels = Vec::new();
        let start = shown.start;
        for (i, (track, _)) in rows.iter().enumerate().skip(start).take(shown.len()) {
            let Some(tone) = settings.get(&track.guid) else {
                continue;
            };
            let last = self.published.get(&track.guid).copied();
            let from = last.map_or(oldest, |last| (last + 1).max(oldest));
            self.published.insert(track.guid.clone(), now);
            let entry = history.entry(track.guid.clone()).or_default();
            for n in from..=now {
                let meters = meters(i, crate::num::coord(n) / hz, tone);
                entry.push(meters.sat_peak);
                entry.push_fire(meters.deess_deepest());
                entry.push_ess(meters.ess_db, meters.ess_ref_db);
            }
            let meters = meters(i, seconds, tone);
            let peak = meters.sat_peak;
            if let Ok(index) = usize::try_from(track.index) {
                if levels.len() <= index {
                    levels.resize(index + 1, daw_proto::TrackLevels::default());
                }
                levels[index] = daw_proto::TrackLevels {
                    peak_left: peak,
                    peak_right: peak * 0.85,
                    hold_left: peak,
                    hold_right: peak,
                };
            }
            // A new spectrum only with a new publish, as the engine sends
            // one: setting it is what makes the rack draw again, and a
            // frame between two publishes has nothing new to show.
            if last != Some(now) {
                spectra.entry(track.guid.clone()).or_default().set(meters);
            }
        }
        levels
    }
}

#[cfg(test)]
mod feed_tests {
    use super::Feed;

    fn rows() -> Vec<(daw_proto::Track, u32)> {
        (0..4)
            .map(|n| {
                (
                    daw_proto::Track {
                        guid: format!("g{n}"),
                        index: n * 2,
                        name: format!("T{n}"),
                        ..daw_proto::Track::default()
                    },
                    0,
                )
            })
            .collect()
    }

    fn fed(seconds: f64) -> Vec<daw_proto::TrackLevels> {
        let rows = rows();
        let mut store = crate::tone::Store::default();
        store.seed(&rows);
        let (mut history, mut spectra) = (
            std::collections::HashMap::new(),
            std::collections::HashMap::new(),
        );
        let levels = Feed::new().feed(
            seconds,
            &rows,
            0..rows.len(),
            &store,
            &mut history,
            &mut spectra,
        );
        assert_eq!(spectra.len(), rows.len(), "every rack gets a spectrum");
        assert_eq!(history.len(), rows.len(), "and a history");
        levels
    }

    #[test]
    fn the_meters_are_indexed_by_project_track() {
        let levels = fed(2.5);
        // Tracks at 0, 2, 4, 6: a slot per project index, the gaps silent.
        assert_eq!(levels.len(), 7);
        assert!(levels[0].peak_left > 0.0 && levels[6].peak_left > 0.0);
        assert_eq!(levels[1].peak_left, 0.0);
    }

    #[test]
    fn the_same_second_is_the_same_signal_and_another_is_not() {
        let peaks = |levels: Vec<daw_proto::TrackLevels>| {
            levels.iter().map(|l| l.peak_left).collect::<Vec<_>>()
        };
        assert_eq!(peaks(fed(2.5)), peaks(fed(2.5)));
        assert_ne!(peaks(fed(2.5)), peaks(fed(3.1)));
    }
}
