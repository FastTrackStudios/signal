//! What two takes are matched on.
//!
//! One [`Frame`] per analysis hop, holding everything the matcher has an
//! opinion about. The set is deliberately wider than a single energy
//! envelope, because an envelope is ambiguous in exactly the place
//! alignment is hardest: a kick and a floor tom have the same shape and
//! the same level, and a broadband follower cannot tell a sung "ah" from
//! the "sss" in front of it.
//!
//! ## Why four bands and not a spectrogram
//!
//! Full spectral distance (MFCCs, chroma, a raw magnitude spectrum) is
//! the textbook answer and it is a worse fit here. Two takes of the same
//! part are sung by the same person into the same mic *at different
//! moments*, and the fine spectral detail differs from take to take in
//! ways that have nothing to do with timing — vibrato phase, mic
//! distance, how open the vowel happened to be. Four broad bands keep
//! what is stable (where the energy sits) and discard what is not, and
//! they cost one biquad cascade rather than an FFT per frame.
//!
//! The split points are chosen for voice and translate acceptably to a
//! kit: sub/body, mid, presence, air. A kick lands almost entirely in
//! band 0, a snare across 1 and 2, a hat in 3 — which is the property
//! that makes percussion alignable at all without onset detection.
//!
//! ## Normalization is per take, on purpose
//!
//! Each take is normalized against its own peak rather than against a
//! level shared by the pair. A dub recorded three dB quieter is the same
//! performance, and matching absolute levels would make the quieter take
//! read as "not sounding" wherever the reference is merely quiet. The
//! cost of the choice is that a take whose loudest moment is a cough
//! normalizes badly — which is what [`FeatureConfig::silence_level`] is
//! for.

use audiocore_dsp::biquad::{Biquad, FilterType};

/// One frame's alignable features.
///
/// Everything is normalized to `0.0..=1.0` (except `pitch`) so weights in
/// [`crate::CostConfig`] are comparable to one another and a preset moves
/// between material types by changing weights alone.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame {
    /// Broadband level, log scale, normalized against the take's peak.
    /// The only feature here that carries loudness.
    pub level: f64,
    /// Spectral shape: how much each of sub, mid, presence and air is
    /// carrying *relative to this frame's own average*, so the vector
    /// says where the energy sits and says nothing about how much of it
    /// there is.
    ///
    /// Loudness lives in `level` alone, deliberately. Bands measured
    /// against the take's peak would rise and fall together every time
    /// the performance did, which makes them a second, noisier copy of
    /// `level` rather than four independent cues.
    ///
    /// Undefined for a silent frame, which has no shape to speak of —
    /// [`crate::dtw`] skips the term rather than reading noise.
    pub bands: [f64; 4],
    /// Rise in air-band energy since the previous frame, normalized.
    ///
    /// The cheap stand-in for spectral flux, and the sharpest onset cue
    /// there is: consonants, picks and sticks all announce themselves up
    /// here before the body of the note arrives.
    pub flux: f64,
    /// Rise in broadband energy since the previous frame, normalized.
    /// Slower than `flux` and survives on material with no top end.
    pub delta: f64,
    /// Zero-crossing rate, normalized. Separates noise from tone without
    /// needing a pitch detector.
    pub zcr: f64,
    /// Pitch relative to the take's own median, in semitones, where the
    /// take has a pitch track. `None` for unvoiced frames — and for every
    /// frame when the caller has no pitch detection at all.
    pub pitch: Option<f64>,
    /// Whether this frame carries a pitch. Kept separately from `pitch`
    /// so a caller with voicing but no pitch value can still say so.
    pub voiced: bool,
    /// Below the silence threshold: nothing is sounding.
    pub silent: bool,
}

/// A take's features, and the rate they were taken at.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Features {
    pub frames: Vec<Frame>,
    /// Frames per second.
    pub frame_rate: f64,
}

impl Features {
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// The broadband level envelope, which is what the macro offset
    /// search correlates.
    pub fn envelope(&self) -> Vec<f64> {
        self.frames.iter().map(|f| f.level).collect()
    }
}

/// How features are extracted.
#[derive(Clone, Copy, Debug)]
pub struct FeatureConfig {
    /// Band edges in Hz: sub|mid, mid|presence, presence|air.
    pub crossovers: [f64; 3],
    /// Length of the window each frame is measured over, in milliseconds.
    ///
    /// Deliberately **not** the caller's analysis window. Only the hop and
    /// the frame count have to match a caller's own framing for frame *i*
    /// to mean the same moment in both; how much audio is averaged to
    /// describe that moment is this crate's business, and it wants far
    /// less than a pitch detector does.
    ///
    /// A YIN window has to span two periods of the lowest note it can
    /// find, so it runs ~46 ms. Measuring energy over 46 ms smears a
    /// transient across four frames: the rise is averaged out, `flux`
    /// peaks broad and late, and the local-maximum test that picks
    /// anchors compares frames that mostly contain the same audio. Ten
    /// milliseconds is short enough to see an attack and long enough to
    /// hold two cycles of anything above 200 Hz.
    pub measure_ms: f64,
    /// Percentile of frame levels taken as the take's noise floor.
    pub noise_percentile: f64,
    /// How far above the measured noise floor, in dB, still reads as
    /// silence.
    pub silence_margin_db: f64,
    /// Highest the silence gate may ever sit, in dB below the take's peak.
    ///
    /// The backstop for material that never stops sounding: a sustained
    /// pad has no gaps, so its tenth percentile is real audio, and a gate
    /// placed above it would mark the performance as silence.
    pub silence_ceiling_db: f64,
    /// Where the normalized level scale bottoms out, in dB below peak.
    /// Frames quieter than this all read as 0.
    pub floor_db: f64,
    /// How far below a frame's own average level a band may sit before it
    /// reads as empty, in dB. Sets the resolution of the spectral shape.
    pub shape_floor_db: f64,
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            // Chest/body, vowel, consonant, air. The top split sits above
            // the sung fundamental range so band 3 is sibilance and stick
            // noise rather than pitch.
            crossovers: [300.0, 2_500.0, 6_000.0],
            measure_ms: 10.0,
            noise_percentile: 0.10,
            silence_margin_db: 6.0,
            silence_ceiling_db: -30.0,
            floor_db: -90.0,
            shape_floor_db: -40.0,
        }
    }
}

/// Extract features from a mono buffer.
///
/// `hop` and `window` describe the caller's *framing* — frames are taken
/// every `hop` samples while a whole `window` fits, which is `tune_dsp`'s
/// convention and therefore what keeps the two frame counts equal, so
/// frame *i* here is frame *i* there.
///
/// The audio each frame is actually measured over is
/// [`FeatureConfig::measure_ms`], centred on the framing window, and is
/// normally much shorter. See that field for why.
pub fn extract(
    samples: &[f64],
    sample_rate: f64,
    hop: usize,
    window: usize,
    cfg: FeatureConfig,
) -> Features {
    let hop = hop.max(1);
    let window = window.max(1);
    let frame_rate = sample_rate / hop as f64;
    if samples.len() < window {
        return Features {
            frames: Vec::new(),
            frame_rate,
        };
    }

    let bands = split_bands(samples, sample_rate, cfg.crossovers);

    // Never longer than the framing window — a measurement that reached
    // outside it would describe audio the caller's frame *i* does not
    // cover — and never so short that an RMS is meaningless.
    let measure = ((cfg.measure_ms / 1000.0 * sample_rate).round() as usize).clamp(8, window);
    let inset = (window - measure) / 2;

    let count = (samples.len() - window) / hop + 1;
    let mut raw: Vec<RawFrame> = Vec::with_capacity(count);
    for i in 0..count {
        // Centred on the framing window, so a feature and the pitch
        // measured at the same index describe the same instant. Off-centre
        // would put a fixed bias between the onset times this finds and
        // the note boundaries the caller found.
        let start = i * hop + inset;
        let end = (start + measure).min(samples.len());
        raw.push(RawFrame {
            band_rms: [
                rms(&bands[0][start..end]),
                rms(&bands[1][start..end]),
                rms(&bands[2][start..end]),
                rms(&bands[3][start..end]),
            ],
            zcr: zero_crossing_rate(&samples[start..end]),
        });
    }

    normalize(raw, frame_rate, cfg)
}

/// One frame before anything is normalized. Kept separate because
/// normalization needs the whole take's peak, which is not known until
/// every frame has been measured.
struct RawFrame {
    band_rms: [f64; 4],
    zcr: f64,
}

fn normalize(raw: Vec<RawFrame>, frame_rate: f64, cfg: FeatureConfig) -> Features {
    let broadband: Vec<f64> = raw
        .iter()
        .map(|f| f.band_rms.iter().sum::<f64>() / 4.0)
        .collect();
    let peak = broadband.iter().copied().fold(0.0_f64, f64::max);

    // A take with no signal at all: every frame is silence, and saying so
    // is better than dividing by the floor and calling noise a
    // performance.
    if peak <= 1e-9 {
        return Features {
            frames: raw
                .iter()
                .map(|_| Frame {
                    silent: true,
                    ..Frame::default()
                })
                .collect(),
            frame_rate,
        };
    }

    let to_db = |v: f64| 20.0 * (v.max(1e-12) / peak).log10();
    let to_norm = |v: f64| ((to_db(v) - cfg.floor_db) / -cfg.floor_db).clamp(0.0, 1.0);

    // The silence gate is measured, not assumed.
    //
    // A fixed level below the peak — the obvious choice, and the one this
    // had — works on a fixture and fails on a recording. Anything tracked
    // with a room, an amp or a live band has a floor well above 60 dB down
    // from its own peak, so no frame is ever silent, and every rule built
    // on silence (pairing gaps at zero cost, discounting the step penalty
    // through them) quietly stops applying on exactly the material it
    // exists for. The tenth percentile of frame levels is what the noise
    // floor actually is; a few dB above it is the first thing that could
    // be a performance.
    let mut sorted: Vec<f64> = broadband.iter().map(|&v| to_db(v)).collect();
    sorted.sort_by(f64::total_cmp);
    let at = ((sorted.len() as f64 * cfg.noise_percentile) as usize).min(sorted.len() - 1);
    let noise_db = sorted[at];
    // Held strictly above the floor. A take with real digital silence in
    // it has a noise floor *at* the floor, and a gate sitting exactly
    // there is a test nothing can pass — which is the same way the fixed
    // threshold failed, arrived at from the other direction.
    let gate_db = (noise_db + cfg.silence_margin_db).clamp(
        cfg.floor_db + cfg.silence_margin_db,
        cfg.silence_ceiling_db.min(0.0),
    );
    let silence = ((gate_db - cfg.floor_db) / -cfg.floor_db).clamp(0.0, 1.0);

    // Rises are measured on the normalized scale, then scaled by the
    // largest rise in the take: what matters is which onsets are the
    // strong ones *within this take*, not how loud the take is.
    //
    // Flux comes off the air band's *absolute* level, not its share of the
    // frame — an onset is energy arriving, and a share can rise while
    // everything gets quieter.
    let air: Vec<f64> = raw.iter().map(|f| to_norm(f.band_rms[3])).collect();
    let level: Vec<f64> = broadband.iter().map(|&v| to_norm(v)).collect();
    // The frame before the take is treated as silence rather than as
    // nothing.
    //
    // Measured against a previous frame that does not exist, the first
    // frame can never show a rise — so a take trimmed to its own first
    // transient, which is how anyone crops an item, has an opening
    // attack that no onset test can see. Its partner take then finds no
    // agreement there, drops the anchor, and the whole opening phrase is
    // left to interpolate from the take's start instead of being pinned
    // to its downbeat. A take that begins in silence is unaffected: its
    // first frame is quiet, so the rise from nothing is nothing.
    let rise = |series: &[f64], i: usize| {
        let previous = if i == 0 { 0.0 } else { series[i - 1] };
        (series[i] - previous).max(0.0)
    };
    let max_flux = (0..raw.len())
        .map(|i| rise(&air, i))
        .fold(0.0_f64, f64::max)
        .max(1e-9);
    let max_delta = (0..raw.len())
        .map(|i| rise(&level, i))
        .fold(0.0_f64, f64::max)
        .max(1e-9);

    // Each band's level relative to its own frame's average, so `bands`
    // describes spectral *shape* and nothing else.
    //
    // Measured against the take's peak — as this did — every band moves
    // whenever the singer gets louder, in lockstep with `level`, so four
    // of the five terms in the cost were re-weighting the same fact.
    // Against the frame's own average, "the energy is up top" means that
    // whether the frame is loud or quiet, which is the only reading that
    // tells a hat from a kick.
    let shape = |band: f64, frame_average: f64| {
        let db = 20.0 * (band.max(1e-12) / frame_average.max(1e-12)).log10();
        ((db - cfg.shape_floor_db) / -cfg.shape_floor_db).clamp(0.0, 1.0)
    };

    let frames = raw
        .iter()
        .enumerate()
        .map(|(i, f)| Frame {
            level: level[i],
            bands: [
                shape(f.band_rms[0], broadband[i]),
                shape(f.band_rms[1], broadband[i]),
                shape(f.band_rms[2], broadband[i]),
                shape(f.band_rms[3], broadband[i]),
            ],
            flux: (rise(&air, i) / max_flux).clamp(0.0, 1.0),
            delta: (rise(&level, i) / max_delta).clamp(0.0, 1.0),
            zcr: f.zcr.clamp(0.0, 1.0),
            pitch: None,
            voiced: false,
            silent: level[i] < silence,
        })
        .collect();

    Features { frames, frame_rate }
}

/// Split into four bands with cascaded second-order Butterworth
/// crossovers.
///
/// Each stage lowpasses off one band and highpasses the remainder into
/// the next, so the bands sum back to something close to the input and no
/// frequency is counted twice. The phase response is not flat, which does
/// not matter: every measurement downstream is an energy over a window.
fn split_bands(samples: &[f64], sample_rate: f64, crossovers: [f64; 3]) -> [Vec<f64>; 4] {
    let nyquist = sample_rate / 2.0;
    // Butterworth Q for a single second-order section.
    const Q: f64 = core::f64::consts::FRAC_1_SQRT_2;

    let mut out = [
        vec![0.0; samples.len()],
        vec![0.0; samples.len()],
        vec![0.0; samples.len()],
        vec![0.0; samples.len()],
    ];
    let mut low = [Biquad::new(), Biquad::new(), Biquad::new()];
    let mut high = [Biquad::new(), Biquad::new(), Biquad::new()];
    for k in 0..3 {
        // A crossover at or above Nyquist is meaningless; park it just
        // below so the filter stays stable and the band above it is
        // simply empty.
        let f = crossovers[k].clamp(1.0, nyquist * 0.99);
        low[k].set(FilterType::Lowpass, f, Q, sample_rate);
        high[k].set(FilterType::Highpass, f, Q, sample_rate);
    }

    for (i, &s) in samples.iter().enumerate() {
        out[0][i] = low[0].tick(s, 0);
        let rest = high[0].tick(s, 0);
        out[1][i] = low[1].tick(rest, 0);
        let rest = high[1].tick(rest, 0);
        out[2][i] = low[2].tick(rest, 0);
        out[3][i] = high[2].tick(rest, 0);
    }
    out
}

fn rms(window: &[f64]) -> f64 {
    if window.is_empty() {
        return 0.0;
    }
    (window.iter().map(|s| s * s).sum::<f64>() / window.len() as f64).sqrt()
}

/// Zero crossings per sample, DC-corrected.
///
/// The mean is removed first because a window sitting on an offset — a
/// low note, or any signal that has not been high-passed — crosses zero
/// far less often than its waveform suggests, and the measurement would
/// read as "more tonal" purely from the offset.
fn zero_crossing_rate(window: &[f64]) -> f64 {
    if window.len() < 2 {
        return 0.0;
    }
    let mean = window.iter().sum::<f64>() / window.len() as f64;
    let mut crossings = 0usize;
    let mut previous = window[0] - mean >= 0.0;
    for &s in &window[1..] {
        let sign = s - mean >= 0.0;
        if sign != previous {
            crossings += 1;
        }
        previous = sign;
    }
    crossings as f64 / (window.len() - 1) as f64
}

impl Features {
    /// Attach a pitch track measured elsewhere.
    ///
    /// Pitch is not extracted here: a caller that wants it already has a
    /// detector (this repo's is `tune_dsp`'s YIN), and one that does not
    /// — a drum track, a batch aligner — should not pay for one. The
    /// values are semitones; they are re-centred on the take's own median
    /// so that a harmony part and the lead it doubles read as the same
    /// *shape* of melody rather than as a constant interval apart.
    ///
    /// `semitones` must be one entry per frame; extra entries are ignored
    /// and missing ones read as unvoiced.
    pub fn with_pitch(mut self, semitones: &[Option<f64>]) -> Self {
        let mut sorted: Vec<f64> = semitones.iter().flatten().copied().collect();
        if sorted.is_empty() {
            return self;
        }
        sorted.sort_by(f64::total_cmp);
        let median = sorted[sorted.len() / 2];
        for (i, frame) in self.frames.iter_mut().enumerate() {
            match semitones.get(i).copied().flatten() {
                Some(v) => {
                    frame.pitch = Some(v - median);
                    frame.voiced = true;
                }
                None => {
                    frame.pitch = None;
                    frame.voiced = false;
                }
            }
        }
        self
    }
}
