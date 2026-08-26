//! Finding a song's hits, so a light show can be written against them.
//!
//! This asks a different question from [`trigger_dsp`], which it borrows
//! its onset detection from. A drum replacer runs live on an isolated
//! close mic and has to answer "fire a sample, now?" within a couple of
//! milliseconds. This runs offline on a finished stereo mix and asks
//! "where are this song's hits, and how hard is it hitting?" — the
//! answer is allowed to arrive slowly, may look at the whole recording
//! before deciding anything, and is read by a human before it matters.
//!
//! That difference buys accuracy the live path cannot have:
//!
//! - **Normalisation over the whole file.** A live detector thresholds
//!   against a moving average because the future has not happened yet.
//!   Here it has, so hit strength and the dynamics curve are scaled
//!   against the loudest thing in the song, and mean the same at the
//!   first bar as at the last.
//! - **Band classification.** Onsets are found once on the whole mix,
//!   then each is placed in a band by the energy that *follows* its
//!   attack. Attacks themselves are broadband — that is what makes them
//!   attacks — so detecting per band finds every hit three times rather
//!   than telling a kick from a crash.
//!
//! Two outputs, from one pass:
//!
//! - [`Analysis::hits`] — discrete events with a band and a strength.
//! - [`Analysis::dynamics`] — the 0..1 "dynamic indicator", an envelope
//!   of how hard the song is going at every moment.
//!
//! Both are in **seconds**. Snapping them to bars is deliberately not
//! done here: that needs a tempo map, which is a musical object this
//! crate has no business knowing about. The caller owns the grid.
//!
//! ```
//! use hit_detect_dsp::{analyze, Config};
//! let samples: Vec<f32> = vec![0.0; 48_000];
//! let analysis = analyze(&samples, &Config::for_rate(48_000.0));
//! for hit in analysis.hits.iter().filter(|h| h.strength > 0.6) {
//!     println!("{:.2}s {:?} {:.2}", hit.secs, hit.band, hit.strength);
//! }
//! ```

use trigger_dsp::spectral_flux::{FluxMode, SpectralFluxDetector};

/// Which part of the spectrum a hit was found in.
///
/// Not instrument names, because this cannot know: a floor tom and a
/// kick both live in `Low`. It is a frequency band, and for charting a
/// song that is enough — `Low` on a downbeat is a kick often enough to
/// be worth a light cue, and being wrong costs one deleted marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Band {
    /// Below ~120 Hz — kick, low toms, bass drops.
    Low,
    /// ~200 Hz to 2 kHz — snare body, most of the backbeat.
    Mid,
    /// Above ~4 kHz — cymbals, snare crack, hat.
    High,
}

impl Band {
    pub const ALL: [Band; 3] = [Band::Low, Band::Mid, Band::High];

    /// The band's filter, as (kind, corner frequency in Hz).
    fn filter(self) -> (Shape, f64) {
        match self {
            Band::Low => (Shape::LowPass, 120.0),
            Band::Mid => (Shape::BandPass, 700.0),
            Band::High => (Shape::HighPass, 4000.0),
        }
    }
}

/// One detected hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub secs: f64,
    /// 0..1, against the strongest onset anywhere in the recording.
    ///
    /// One scale for the whole song, so "0.9" means the same thing in
    /// the first bar as in the last — the thing a live detector, which
    /// can only threshold against the recent past, cannot offer.
    pub strength: f32,
    pub band: Band,
}

/// The result of one pass over a recording.
pub struct Analysis {
    /// Every hit found, in time order.
    pub hits: Vec<Hit>,
    /// The 0..1 dynamic indicator, one value per frame.
    pub dynamics: Vec<f32>,
    /// Frames per second of `dynamics` — `sample_rate / hop_size`.
    pub frame_rate: f64,
}

impl Analysis {
    /// The dynamic indicator at a moment, linearly interpolated.
    ///
    /// Interpolated rather than nearest-frame because this drives light
    /// levels: stepping between frames at ~86 Hz is visible as a stutter
    /// on a slow fade, which is exactly what this curve is for.
    pub fn dynamics_at(&self, secs: f64) -> f32 {
        if self.dynamics.is_empty() {
            return 0.0;
        }
        let frame = (secs * self.frame_rate).max(0.0);
        let i = frame.floor() as usize;
        if i + 1 >= self.dynamics.len() {
            return *self.dynamics.last().expect("non-empty");
        }
        let t = (frame - i as f64) as f32;
        self.dynamics[i] * (1.0 - t) + self.dynamics[i + 1] * t
    }

    /// The strongest hit in a window, if any — "was there a hit here?"
    pub fn strongest_between(&self, from: f64, to: f64) -> Option<&Hit> {
        self.hits
            .iter()
            .filter(|h| h.secs >= from && h.secs < to)
            .max_by(|a, b| a.strength.total_cmp(&b.strength))
    }
}

/// Analysis settings.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub sample_rate: f64,
    /// FFT window. 2048 at 44.1k is ~46 ms — long enough to resolve a
    /// kick's fundamental, short enough not to smear a fast backbeat.
    pub fft_size: usize,
    /// Hop between windows. 512 gives ~86 frames a second, which is
    /// about 11 ms of timing resolution — well under the ~30 ms at which
    /// a light cue starts to read as late.
    pub hop_size: usize,
    /// How far above the local average an onset must rise to count.
    /// Lower finds more and invents more.
    pub sensitivity: f64,
    /// Hits closer together than this are one hit.
    /// A snare is one event even though its onset smears across a few
    /// frames.
    pub min_gap_secs: f64,
    /// Seconds of smoothing on the dynamics curve. Long, deliberately:
    /// this is meant to describe *sections*, not individual beats, so it
    /// should ride over the drums rather than bounce with them.
    pub dynamics_smoothing_secs: f64,
}

impl Config {
    pub fn for_rate(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            fft_size: 2048,
            hop_size: 512,
            sensitivity: 2.1,
            min_gap_secs: 0.09,
            dynamics_smoothing_secs: 0.35,
        }
    }
}

/// Analyses a mono recording.
///
/// Stereo should be summed to mono first: hits are not a stereo
/// phenomenon, and summing halves the work.
pub fn analyze(samples: &[f32], config: &Config) -> Analysis {
    let frame_rate = config.sample_rate / config.hop_size as f64;

    // Detection and classification are separate problems, and running
    // them together was a mistake worth recording: detecting per band
    // and emitting a hit from each reported *three* hits for every drum
    // stroke. The attack of any percussive sound is broadband — that is
    // what makes it an attack — so a 60 Hz kick lights up the 4 kHz
    // detector just as reliably as a cymbal does.
    //
    // So: find onsets once, on the whole mix. Then ask separately which
    // band each one actually lives in, by measuring energy rather than
    // flux. Energy after the attack is what differs between a kick and a
    // crash; the attacks themselves barely do.
    let curve = onset_curve(samples, None, config);
    let mut hits = pick(&curve, frame_rate, config);

    let energy = band_energy(samples, config);
    for hit in hits.iter_mut() {
        hit.band = classify(&energy, hit.secs, frame_rate);
    }

    Analysis {
        hits,
        dynamics: dynamics(samples, config),
        frame_rate,
    }
}

/// Per-band energy, each normalised by its own median.
///
/// The normalisation is what makes the bands comparable. Raw energy
/// would label almost everything `Low`, because almost every mix has
/// more energy at 60 Hz than at 8 kHz — that is how music is mixed, not
/// a fact about any particular hit. Against its own median, each band
/// reports how unusual *this moment* is for *that* band, which is the
/// question worth asking.
fn band_energy(samples: &[f32], config: &Config) -> Vec<(Band, Vec<f32>)> {
    Band::ALL
        .iter()
        .map(|&band| {
            let (shape, freq) = band.filter();
            let mut filter = Biquad::new(shape, freq, config.sample_rate);
            let filtered: Vec<f32> = samples.iter().map(|s| filter.tick(f64::from(*s)) as f32).collect();
            let mut frames: Vec<f32> = filtered
                .chunks(config.hop_size.max(1))
                .map(|chunk| {
                    let sum: f64 = chunk.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
                    (sum / chunk.len().max(1) as f64).sqrt() as f32
                })
                .collect();
            let mut sorted = frames.clone();
            sorted.sort_by(|a, b| a.total_cmp(b));
            let median = sorted
                .get(sorted.len() / 2)
                .copied()
                .unwrap_or(0.0)
                .max(1e-9);
            for value in frames.iter_mut() {
                *value /= median;
            }
            (band, frames)
        })
        .collect()
}

/// Which band a hit belongs to.
///
/// Measured over a short window *after* the onset rather than at it,
/// for the same reason detection cannot classify: at the attack every
/// band is loud. A few frames later the kick still has its fundamental
/// and the cymbal still has its wash, and they no longer look alike.
fn classify(energy: &[(Band, Vec<f32>)], secs: f64, frame_rate: f64) -> Band {
    let start = (secs * frame_rate) as usize;
    // ~35 ms at the default hop — past the click, inside the body.
    let end = start + 3;
    energy
        .iter()
        .max_by(|(_, a), (_, b)| {
            let score = |frames: &Vec<f32>| {
                frames
                    .get(start..end.min(frames.len()))
                    .unwrap_or_default()
                    .iter()
                    .copied()
                    .fold(0.0_f32, f32::max)
            };
            score(a).total_cmp(&score(b))
        })
        .map(|(band, _)| *band)
        .unwrap_or(Band::Mid)
}

/// The onset detection function, one value per hop.
///
/// `band` filters the input first; `None` runs on the full mix.
fn onset_curve(samples: &[f32], band: Option<Band>, config: &Config) -> Vec<f64> {
    let mut filter = band.map(|b| {
        let (shape, freq) = b.filter();
        Biquad::new(shape, freq, config.sample_rate)
    });
    // SuperFlux rather than plain spectral flux: its maximum-filter step
    // suppresses vibrato and pitch drift, which on a full mix is the
    // difference between detecting the drums and detecting the singer.
    let mut detector = SpectralFluxDetector::new(
        FluxMode::SuperFlux,
        config.fft_size,
        config.hop_size,
        config.sample_rate,
    );
    let mut curve = Vec::with_capacity(samples.len() / config.hop_size + 1);
    for &sample in samples {
        let input = match filter.as_mut() {
            Some(f) => f.tick(f64::from(sample)),
            None => f64::from(sample),
        };
        if let Some(odf) = detector.tick(input) {
            curve.push(odf);
        }
    }
    curve
}

/// Peaks in an onset curve, as hits.
///
/// Adaptive thresholding against a local window rather than one global
/// number: a quiet verse and a loud chorus both have hits, and a fixed
/// threshold either misses the verse or fills the chorus with noise.
fn pick(curve: &[f64], frame_rate: f64, config: &Config) -> Vec<Hit> {
    if curve.is_empty() {
        return Vec::new();
    }
    // Roughly a second either side — long enough to span a bar at most
    // tempos, so the threshold tracks the arrangement and not the beat.
    let window = (frame_rate as usize).max(1);
    let peak = curve.iter().copied().fold(0.0_f64, f64::max);
    if peak <= f64::EPSILON {
        return Vec::new();
    }
    let min_gap = (config.min_gap_secs * frame_rate) as usize;

    let mut hits: Vec<Hit> = Vec::new();
    for i in 1..curve.len().saturating_sub(1) {
        // A local maximum, or it is the shoulder of one already taken.
        if curve[i] < curve[i - 1] || curve[i] < curve[i + 1] {
            continue;
        }
        let from = i.saturating_sub(window);
        let to = (i + window).min(curve.len());
        let mean = curve[from..to].iter().sum::<f64>() / (to - from) as f64;
        if curve[i] < mean * config.sensitivity {
            continue;
        }
        // Within the gap of the previous hit this is the same event;
        // keep whichever is stronger so a hit lands on its own peak.
        if let Some(last) = hits.last_mut() {
            let last_frame = (last.secs * frame_rate) as usize;
            if i.saturating_sub(last_frame) < min_gap {
                if (curve[i] / peak) as f32 > last.strength {
                    last.secs = i as f64 / frame_rate;
                    last.strength = (curve[i] / peak) as f32;
                }
                continue;
            }
        }
        hits.push(Hit {
            secs: i as f64 / frame_rate,
            strength: (curve[i] / peak) as f32,
            // Filled in by `classify` once every onset is known.
            band: Band::Mid,
        });
    }
    hits
}

/// The 0..1 dynamic indicator.
///
/// RMS per hop, smoothed, then scaled so the song's loudest stretch is
/// 1.0. Scaled against a high percentile rather than the maximum: one
/// clipped sample or a stray crash should not push the whole song's
/// curve down, which is what normalising by the true peak does.
fn dynamics(samples: &[f32], config: &Config) -> Vec<f32> {
    let hop = config.hop_size.max(1);
    let mut rms: Vec<f32> = samples
        .chunks(hop)
        .map(|chunk| {
            let sum: f64 = chunk.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
            (sum / chunk.len().max(1) as f64).sqrt() as f32
        })
        .collect();
    if rms.is_empty() {
        return rms;
    }

    // One-pole smoothing, run forwards then backwards. Two passes
    // because a single one lags, and a dynamics curve that peaks after
    // the chorus does is worse than no curve at all.
    let frame_rate = config.sample_rate / hop as f64;
    let coeff = (-1.0 / (config.dynamics_smoothing_secs * frame_rate)).exp() as f32;
    let mut state = rms[0];
    for value in rms.iter_mut() {
        state = *value + coeff * (state - *value);
        *value = state;
    }
    state = *rms.last().expect("non-empty");
    for value in rms.iter_mut().rev() {
        state = *value + coeff * (state - *value);
        *value = state;
    }

    let mut sorted = rms.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let reference = sorted[sorted.len() * 95 / 100].max(f32::EPSILON);
    for value in rms.iter_mut() {
        *value = (*value / reference).clamp(0.0, 1.0);
    }
    rms
}

/// Filter shapes this crate needs.
#[derive(Clone, Copy)]
enum Shape {
    LowPass,
    BandPass,
    HighPass,
}

/// A second-order section, RBJ cookbook coefficients.
///
/// Hand-rolled rather than pulled from `eq-dsp`: that crate's `Band`
/// carries gain staging, placement and bypass smoothing that a band
/// split for analysis has no use for, and the setup to say "just a
/// lowpass at 120" is longer than the filter.
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn new(shape: Shape, freq: f64, sample_rate: f64) -> Self {
        // Q = 0.707 is Butterworth: maximally flat, no resonant bump at
        // the corner. A bump here would be read as extra onset energy.
        let q = 0.707_f64;
        let w0 = 2.0 * std::f64::consts::PI * freq / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b1, b2) = match shape {
            Shape::LowPass => (
                (1.0 - cos) / 2.0,
                1.0 - cos,
                (1.0 - cos) / 2.0,
            ),
            Shape::HighPass => (
                (1.0 + cos) / 2.0,
                -(1.0 + cos),
                (1.0 + cos) / 2.0,
            ),
            Shape::BandPass => (alpha, 0.0, -alpha),
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn tick(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    /// A click train at a known tempo, in a chosen band.
    fn clicks(at: &[f64], freq: f64, secs: f64) -> Vec<f32> {
        let mut samples = vec![0.0_f32; (RATE * secs) as usize];
        for &t in at {
            let start = (t * RATE) as usize;
            // A short decaying tone rather than an impulse: an impulse
            // is broadband and would appear in every band at once, which
            // would make the band test prove nothing.
            for i in 0..(RATE * 0.05) as usize {
                let Some(slot) = samples.get_mut(start + i) else {
                    break;
                };
                let phase = 2.0 * std::f64::consts::PI * freq * (i as f64 / RATE);
                let decay = (-(i as f64) / (RATE * 0.01)).exp();
                *slot += (phase.sin() * decay) as f32;
            }
        }
        samples
    }

    #[test]
    fn finds_hits_where_they_were_put() {
        let at = [0.5, 1.0, 1.5, 2.0];
        let analysis = analyze(&clicks(&at, 60.0, 3.0), &Config::for_rate(RATE));
        for &t in &at {
            let found = analysis
                .hits
                .iter()
                .any(|h| (h.secs - t).abs() < 0.05 && h.band == Band::Low);
            assert!(found, "no low hit near {t}s in {:?}", analysis.hits);
        }
    }

    /// The point of splitting: a kick and a cymbal must not be reported
    /// as the same kind of event, or the chart cannot tell a downbeat
    /// from a hi-hat.
    #[test]
    fn separates_low_hits_from_high_ones() {
        let low = analyze(&clicks(&[0.5, 1.0], 60.0, 2.0), &Config::for_rate(RATE));
        let high = analyze(&clicks(&[0.5, 1.0], 8000.0, 2.0), &Config::for_rate(RATE));
        let count = |a: &Analysis, b: Band| a.hits.iter().filter(|h| h.band == b).count();
        assert!(
            count(&low, Band::Low) > count(&low, Band::High),
            "a 60 Hz hit should read as Low"
        );
        assert!(
            count(&high, Band::High) > count(&high, Band::Low),
            "an 8 kHz hit should read as High"
        );
    }

    /// Silence has no hits. Obvious, and the case a threshold expressed
    /// as a multiple of a local mean gets wrong — the mean is zero, and
    /// every frame is trivially "above" it.
    #[test]
    fn silence_produces_nothing() {
        let analysis = analyze(&vec![0.0; (RATE * 2.0) as usize], &Config::for_rate(RATE));
        assert!(analysis.hits.is_empty(), "{:?}", analysis.hits);
    }

    /// The dynamic indicator has to track arrangement: quiet section
    /// low, loud section high. This is the whole contract.
    #[test]
    fn dynamics_follow_the_arrangement() {
        let mut samples = vec![0.0_f32; (RATE * 4.0) as usize];
        for (i, s) in samples.iter_mut().enumerate() {
            let t = i as f64 / RATE;
            let level = if t < 2.0 { 0.05 } else { 0.8 };
            *s = (level * (2.0 * std::f64::consts::PI * 220.0 * t).sin()) as f32;
        }
        let analysis = analyze(&samples, &Config::for_rate(RATE));
        let quiet = analysis.dynamics_at(1.0);
        let loud = analysis.dynamics_at(3.0);
        assert!(quiet < 0.3, "quiet section read {quiet}");
        assert!(loud > 0.7, "loud section read {loud}");
    }

    /// Normalised over the whole file, so the same hit means the same
    /// thing wherever it falls — the thing a live detector cannot do.
    #[test]
    fn strength_is_bounded() {
        let analysis = analyze(&clicks(&[0.5, 1.0, 1.5], 60.0, 2.5), &Config::for_rate(RATE));
        assert!(!analysis.hits.is_empty());
        for hit in &analysis.hits {
            assert!(
                (0.0..=1.0).contains(&hit.strength),
                "strength {} out of range",
                hit.strength
            );
        }
    }

    #[test]
    fn dynamics_interpolate_rather_than_step() {
        let analysis = Analysis {
            hits: Vec::new(),
            dynamics: vec![0.0, 1.0],
            frame_rate: 1.0,
        };
        assert!((analysis.dynamics_at(0.5) - 0.5).abs() < 1e-6);
        // Past the end holds the last value rather than falling to zero,
        // so a light does not blink off at the final frame.
        assert!((analysis.dynamics_at(99.0) - 1.0).abs() < 1e-6);
    }
}

