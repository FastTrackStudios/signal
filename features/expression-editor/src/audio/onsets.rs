//! Onset detection — where the hits are in unpitched audio.
//!
//! The pitched path segments a take by tracking f0 and cutting where it
//! changes. On a kit there is no f0 to track, so the segmentation has to
//! come from the other axis: sudden increases in energy.
//!
//! Spectral flux, which is the standard answer, in three steps that each
//! exist because the obvious version fails:
//!
//! 1. **Sum only the *positive* changes** in per-bin magnitude between
//!    consecutive frames. Counting decays too makes every hit fire
//!    twice — once on the attack, once on the release.
//! 2. **Subtract a moving median** rather than a fixed threshold. A
//!    fixed one cannot survive a track that gets louder: set it for the
//!    verse and the choruses smear, set it for the chorus and the verse
//!    goes missing.
//! 3. **Enforce a minimum spacing** after picking peaks. A single hit
//!    with a messy attack — a flam, a snare with a long buzz — otherwise
//!    arrives as three or four onsets a few milliseconds apart.
//!
//! Magnitudes are **log-compressed** before any of that. Without it the
//! function is dominated by whichever bins are loudest, which on a kit
//! means the low ones — and a kick's pitch sweeping downward through
//! them produces a second, entirely genuine burst of positive flux
//! partway through the decay. Compression puts a quiet hat's spectral
//! change on comparable footing with a kick's, which is what the
//! detector is actually trying to compare.
//!
//! What this deliberately does not do is chase every ghost note. The
//! second strike of a flam rises out of the first one's decay rather
//! than out of silence, so its spectral change is small and it falls
//! below the threshold. Lowering the threshold until it is caught also
//! catches a great deal that is not there. A conservative detector whose
//! misses are obvious (a slice too long, split by hand) beats a
//! trigger-happy one whose false hits have to be found and deleted.
//!
//! The same pass yields each hit's **spectral centroid**, which is what
//! sorts it into a band. Doing both here is not a shortcut: they need
//! the same spectra, and computing them twice would be the odd choice.

use realfft::RealFftPlanner;

/// Log-compression strength. The usual choice in the onset-detection
/// literature, and the exact value matters little — what matters is that
/// there is one.
const COMPRESSION: f64 = 1_000.0;

/// How long after an onset a hit's peak is looked for, in seconds.
///
/// A struck sound reaches its maximum within a few milliseconds; 20 ms
/// is generous even for a soft mallet, and short enough that a fast
/// pattern never reads its neighbour's attack.
const ATTACK_SECS: f64 = 0.02;

/// One detected hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Onset {
    /// Frame index of the attack.
    pub frame: usize,
    /// Detection-function value at the peak, after median subtraction.
    /// Higher is more clearly an onset — used to rank, not to threshold
    /// a second time.
    pub strength: f64,
    /// Energy-weighted mean frequency at the attack, in Hz. What sorts
    /// a hit into a band.
    pub centroid_hz: f64,
    /// Peak linear amplitude of this hit's attack.
    pub peak: f64,
}

/// How onsets are picked.
#[derive(Clone, Copy, Debug)]
pub struct OnsetConfig {
    /// STFT window, in samples. 1024 at 44.1 kHz is ~23 ms — short
    /// enough to place an attack, long enough to have bins worth
    /// comparing.
    pub window: usize,
    /// Hop, in samples.
    pub hop: usize,
    /// Half-width of the moving median, in frames.
    pub median_frames: usize,
    /// How far above the local median a peak must rise, as a fraction
    /// of the largest such rise in the take.
    pub threshold: f64,
    /// Closest two onsets may be, in seconds.
    ///
    /// A real parameter, not a constant to hide: 50 ms is right for a
    /// kit and much too long for a shaker or a 32nd-note hi-hat.
    pub min_spacing_secs: f64,
    /// Frames quieter than this are never onsets, whatever the flux
    /// says. Without it, the noise floor between phrases produces
    /// perfectly good-looking peaks out of nothing.
    pub silence_rms: f64,
}

impl Default for OnsetConfig {
    fn default() -> Self {
        Self {
            window: 1024,
            hop: 256,
            median_frames: 8,
            threshold: 0.12,
            min_spacing_secs: 0.05,
            silence_rms: 0.001,
        }
    }
}

/// The detection function and the spectra behind it.
struct Flux {
    /// Positive spectral flux per frame.
    values: Vec<f64>,
    /// Spectral centroid per frame, Hz.
    centroid: Vec<f64>,
    /// RMS per frame, for the silence gate.
    rms: Vec<f64>,
}

fn analyse_flux(samples: &[f64], sample_rate: f64, cfg: &OnsetConfig) -> Flux {
    let window = cfg.window.max(16);
    let hop = cfg.hop.max(1);

    // A window of silence in front of the take.
    //
    // Two things break without it, and both look like the detector
    // failing rather than like a framing problem. A hit at the very
    // start has nothing before it to be an increase over, so it lands in
    // frame 0 — which cannot be a local maximum, having no left
    // neighbour, and is therefore never picked. And the enormous flux of
    // the first frame (every bin rising from nothing) inflates the local
    // median around it, suppressing whatever follows.
    //
    // Padding by exactly one window also fixes the alignment: with it,
    // energy at original sample `s` first appears in frame `s / hop`, so
    // a reported frame means what `onset_seconds` says it does. Any
    // other padding puts every onset off by a constant.
    let mut padded = Vec::with_capacity(samples.len() + window);
    padded.resize(window, 0.0);
    padded.extend_from_slice(samples);
    let samples = &padded[..];
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(window);
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();

    // Hann. Without a window the flux is dominated by the discontinuity
    // at the frame edges, which moves with the signal and looks a great
    // deal like onsets.
    let taper: Vec<f64> = (0..window)
        .map(|i| {
            let x = i as f64 / (window - 1).max(1) as f64;
            0.5 - 0.5 * (core::f64::consts::TAU * x).cos()
        })
        .collect();

    let frames = if samples.len() < window {
        0
    } else {
        (samples.len() - window) / hop + 1
    };
    let bin_hz = sample_rate / window as f64;

    let mut values = Vec::with_capacity(frames);
    let mut centroid = Vec::with_capacity(frames);
    let mut rms = Vec::with_capacity(frames);
    let mut previous: Vec<f64> = vec![0.0; output.len()];

    for f in 0..frames {
        let start = f * hop;
        let slice = &samples[start..start + window];
        for (i, s) in input.iter_mut().enumerate() {
            *s = slice[i] * taper[i];
        }
        if fft.process(&mut input, &mut output).is_err() {
            break;
        }

        let mut flux = 0.0;
        let mut weighted = 0.0;
        let mut total = 0.0;
        for (bin, c) in output.iter().enumerate() {
            let raw = (c.re * c.re + c.im * c.im).sqrt();
            // Log compression before differencing — see the module note.
            let mag = (1.0 + COMPRESSION * raw).ln();
            // Positive part only — see the module note.
            let delta = mag - previous[bin];
            if delta > 0.0 {
                flux += delta;
            }
            previous[bin] = mag;
            // The centroid is a property of the sound, so it is measured
            // on the real spectrum: compression is a detector trick and
            // would drag every centroid toward the middle.
            weighted += raw * bin as f64 * bin_hz;
            total += raw;
        }

        values.push(flux);
        centroid.push(if total > 0.0 { weighted / total } else { 0.0 });
        let energy: f64 = slice.iter().map(|s| s * s).sum();
        rms.push((energy / window as f64).sqrt());
    }

    Flux {
        values,
        centroid,
        rms,
    }
}

/// Median of a window around `i`, clamped at the ends.
fn local_median(values: &[f64], i: usize, half: usize) -> f64 {
    let lo = i.saturating_sub(half);
    let hi = (i + half + 1).min(values.len());
    let mut w: Vec<f64> = values[lo..hi].to_vec();
    if w.is_empty() {
        return 0.0;
    }
    w.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    w[w.len() / 2]
}

/// Find the hits in a mono take.
///
/// Frames are this function's own — the STFT it needs is not the one the
/// pitch tracker runs — so [`Onset::frame`] indexes `cfg.hop`, and
/// [`onset_seconds`] is the way to get back to time.
pub fn detect(samples: &[f64], sample_rate: f64, cfg: OnsetConfig) -> Vec<Onset> {
    if samples.is_empty() || sample_rate <= 0.0 {
        return Vec::new();
    }
    let flux = analyse_flux(samples, sample_rate, &cfg);
    if flux.values.len() < 3 {
        return Vec::new();
    }

    // Detection function: flux above its own local median, floored at
    // zero. The median is what makes this survive a track that changes
    // level — it tracks the trend, so only a *local* excess counts.
    let mut detection = Vec::with_capacity(flux.values.len());
    for i in 0..flux.values.len() {
        let m = local_median(&flux.values, i, cfg.median_frames);
        detection.push((flux.values[i] - m).max(0.0));
    }

    // Scale against the largest excess in the take. A percentile is the
    // tempting choice — one stray frame should not set the bar — but it
    // is wrong for sparse material: four hits in two seconds put every
    // non-zero value inside the top few percent, so the "typical" value
    // *is* a hit and the threshold lands above all of them. On a take
    // with one hit it finds nothing at all.
    //
    // The max is safe here precisely because the median subtraction
    // already removed the trend: what remains is how far each attack
    // stands out locally, and a loud passage does not inflate that.
    let loudest = detection.iter().copied().fold(0.0f64, f64::max);
    if loudest <= 0.0 {
        return Vec::new();
    }
    let floor = cfg.threshold * loudest;

    // Local maxima above the floor, loudest first, then greedily keep
    // the ones far enough apart. Taking them in strength order means a
    // strong hit wins the spacing contest against a weak neighbour,
    // which is the right way round — the alternative keeps whichever
    // came first, so a flam's ghost note suppresses the hit itself.
    let min_gap =
        ((cfg.min_spacing_secs * sample_rate / cfg.hop.max(1) as f64).round() as usize).max(1);
    let mut peaks: Vec<usize> = (1..detection.len() - 1)
        .filter(|&i| {
            detection[i] >= floor
                && detection[i] > detection[i - 1]
                && detection[i] >= detection[i + 1]
                && flux.rms.get(i).is_some_and(|r| *r >= cfg.silence_rms)
        })
        .collect();
    peaks.sort_by(|&a, &b| {
        detection[b]
            .partial_cmp(&detection[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut kept: Vec<usize> = Vec::new();
    for p in peaks {
        if kept.iter().all(|&k| p.abs_diff(k) >= min_gap) {
            kept.push(p);
        }
    }
    kept.sort_unstable();

    // Peak amplitude per hit, measured over its *attack* rather than
    // over the whole slice.
    //
    // Measuring to the next onset is the obvious version and it is
    // wrong, because both ends are quantised to a hop: a hit is detected
    // in the frame its energy first appears in, so its true attack can
    // begin up to a hop *before* `frame * hop`. Reading one slice all
    // the way to the next boundary therefore catches the front of the
    // next hit — and since a percussive attack is the loudest thing in
    // its neighbourhood, every slice ended up reporting the level of the
    // one after it.
    //
    // So: stop a hop short of the next onset, and in any case within the
    // attack window. A struck sound peaks within a few milliseconds;
    // looking further finds only other people's hits.
    let hop = cfg.hop.max(1);
    let attack = (ATTACK_SECS * sample_rate) as usize;
    kept.iter()
        .enumerate()
        .map(|(n, &frame)| {
            let from = frame * hop;
            let to = kept
                .get(n + 1)
                .map(|&next| (next * hop).saturating_sub(hop))
                .unwrap_or(samples.len())
                .min(from + attack)
                .min(samples.len())
                .max(from + 1);
            let peak = samples
                .get(from..to)
                .map(|s| s.iter().fold(0.0f64, |m, v| m.max(v.abs())))
                .unwrap_or(0.0);
            Onset {
                frame,
                strength: detection[frame],
                centroid_hz: flux.centroid.get(frame).copied().unwrap_or(0.0),
                peak,
            }
        })
        .collect()
}

/// Seconds an onset frame lands at.
pub fn onset_seconds(onset: &Onset, sample_rate: f64, cfg: &OnsetConfig) -> f64 {
    onset.frame as f64 * cfg.hop.max(1) as f64 / sample_rate.max(1.0)
}
