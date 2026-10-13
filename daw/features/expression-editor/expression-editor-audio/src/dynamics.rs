//! Gating, compression, breath and sibilance — detected, and written as
//! automation the host owns.
//!
//! None of these resynthesise. Each produces a **gain curve in dB over
//! time**, which the host applies as an envelope on the item, so every
//! one of them is reversible, adjustable in the DAW afterwards, and
//! costs nothing in quality. That is the whole reason they are here
//! rather than in the render path.
//!
//! ## Four concerns, one shape
//!
//! Every detector answers the same question — *how much gain, at this
//! moment* — and differs only in what it looks at:
//!
//! | | looks at | typically |
//! |---|---|---|
//! | gate | level vs a threshold | cuts between phrases |
//! | compression | level above a threshold | pulls peaks down |
//! | breath | quiet + unvoiced + airy | cuts, or leaves alone |
//! | sibilance | loud + unvoiced + bright | cuts a few dB |
//!
//! Sharing the shape is what lets them be written either as four
//! separate lanes or summed into one, without any of them knowing
//! which.
//!
//! ## Why breath and sibilance are separable at all
//!
//! Both are unvoiced, so a pitch tracker sees them identically. They
//! part on two cheap features: a sibilant is *bright and not quiet*,
//! a breath is *airy and quiet*. See [`crate::frames`].

use crate::frames::FrameFeature;

/// A point on a gain curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GainPoint {
    /// Analysis frame.
    pub frame: usize,
    /// Gain in dB. Negative attenuates.
    pub db: f64,
}

/// A detected region worth marking, whether or not it is acted on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detection {
    Breath,
    Sibilance,
}

impl Detection {
    /// What a take marker for it should say.
    pub fn label(&self) -> &'static str {
        match self {
            Detection::Breath => "breath",
            Detection::Sibilance => "sibilance",
        }
    }
}

/// One detected span, in frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    pub kind: Detection,
    pub start: usize,
    pub end: usize,
    /// Loudest frame level in the span, linear.
    pub peak: f64,
}

impl Region {
    pub fn len(&self) -> usize {
        self.end - self.start + 1
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

/// How each concern behaves. Every one can be switched off on its own,
/// which is the point: a take may want de-essing and nothing else.
#[derive(Clone, Copy, Debug)]
pub struct DynamicsConfig {
    pub gate: Option<GateConfig>,
    pub compressor: Option<CompressorConfig>,
    pub breath: Option<BreathConfig>,
    pub sibilance: Option<SibilanceConfig>,
}

impl Default for DynamicsConfig {
    /// Nothing on.
    ///
    /// A take that is opened must not silently acquire four processors;
    /// each is switched on deliberately.
    fn default() -> Self {
        Self {
            gate: None,
            compressor: None,
            breath: None,
            sibilance: None,
        }
    }
}

/// Cut everything below a threshold.
#[derive(Clone, Copy, Debug)]
pub struct GateConfig {
    /// Below this, in dBFS, the gate closes.
    pub threshold_db: f64,
    /// How far it closes. Not silence by default — a hard cut between
    /// phrases is more audible than the noise it removes.
    pub floor_db: f64,
    /// Milliseconds to close and to open. Slow enough that the gate
    /// does not chatter on a breath at the threshold.
    pub attack_ms: f64,
    pub release_ms: f64,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            threshold_db: -45.0,
            floor_db: -18.0,
            attack_ms: 5.0,
            release_ms: 120.0,
        }
    }
}

/// Pull down what is above a threshold.
#[derive(Clone, Copy, Debug)]
pub struct CompressorConfig {
    pub threshold_db: f64,
    /// 2.0 means two dB in for one dB out.
    pub ratio: f64,
    /// Width of the soft knee in dB, centred on the threshold.
    pub knee_db: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
}

impl Default for CompressorConfig {
    fn default() -> Self {
        Self {
            threshold_db: -18.0,
            ratio: 2.5,
            knee_db: 6.0,
            attack_ms: 10.0,
            release_ms: 150.0,
        }
    }
}

/// Find breaths, and optionally duck them.
#[derive(Clone, Copy, Debug)]
pub struct BreathConfig {
    /// Loudest a breath gets, in dBFS.
    pub max_level_db: f64,
    /// Shortest run that counts, in milliseconds. A few frames of
    /// quiet between words is not a breath.
    pub min_ms: f64,
    /// How much to duck. Zero marks without touching the gain, which
    /// is often what is wanted: breaths carry the performance.
    pub reduction_db: f64,
}

impl Default for BreathConfig {
    fn default() -> Self {
        Self {
            max_level_db: -30.0,
            min_ms: 40.0,
            reduction_db: 0.0,
        }
    }
}

/// Find sibilants, and duck them.
#[derive(Clone, Copy, Debug)]
pub struct SibilanceConfig {
    /// Fraction of energy above the high split before a frame counts
    /// as sibilant.
    pub min_high_ratio: f64,
    /// Zero-crossing rate floor. Both are required: a bright vowel can
    /// pass one but not the other.
    pub min_zcr: f64,
    /// Quieter than this and it is a breath, not an "s".
    pub min_level_db: f64,
    pub min_ms: f64,
    pub reduction_db: f64,
}

impl Default for SibilanceConfig {
    fn default() -> Self {
        Self {
            min_high_ratio: 0.35,
            min_zcr: 0.15,
            min_level_db: -34.0,
            min_ms: 25.0,
            reduction_db: -5.0,
        }
    }
}

/// Everything the dynamics pass produced.
#[derive(Clone, Debug, Default)]
pub struct Dynamics {
    /// Per-concern gain curves, each in dB and each independently
    /// writable. Empty where a concern is off.
    pub gate: Vec<GainPoint>,
    pub compressor: Vec<GainPoint>,
    pub breath: Vec<GainPoint>,
    pub sibilance: Vec<GainPoint>,
    /// What was found, for marking on the item.
    pub regions: Vec<Region>,
}

impl Dynamics {
    /// The four curves summed, for hosts that get one lane.
    ///
    /// Summed in dB, which is the correct composition: gain stages
    /// multiply, and dB is where multiplication is addition. Summing
    /// linear gains would make two 6 dB cuts into one 6 dB cut.
    pub fn combined(&self, frames: usize) -> Vec<GainPoint> {
        if frames == 0 {
            return Vec::new();
        }
        let mut db = vec![0.0f64; frames];
        for curve in [&self.gate, &self.compressor, &self.breath, &self.sibilance] {
            for p in curve {
                if let Some(slot) = db.get_mut(p.frame) {
                    *slot += p.db;
                }
            }
        }
        db.into_iter()
            .enumerate()
            .map(|(frame, db)| GainPoint { frame, db })
            .collect()
    }

    pub fn regions_of(&self, kind: Detection) -> impl Iterator<Item = &Region> {
        self.regions.iter().filter(move |r| r.kind == kind)
    }
}

fn to_db(linear: f64) -> f64 {
    20.0 * linear.max(1e-9).log10()
}

/// Shortest time constant an envelope can actually express, in frames.
///
/// These curves become envelope points that the host joins with
/// straight lines, so the fastest transition representable is one
/// frame — and a transition that fast *is* a step. On a gate that is a
/// click, which is the exact artefact the attack and release times
/// exist to avoid.
///
/// Three frames is the floor: enough that the shape reads as a slope
/// rather than a corner, at whatever rate the analysis happens to run.
const MIN_SMOOTH_FRAMES: f64 = 3.0;

/// One-pole smoothing coefficient for a time constant in milliseconds.
///
/// Clamped rather than honoured exactly. A processor can have a 1 ms
/// attack; an envelope sampled at a couple of hundred hertz cannot, and
/// pretending otherwise produces a step the host renders as a click.
fn coeff(ms: f64, frame_rate: f64) -> f64 {
    let frames = (ms / 1000.0 * frame_rate).max(MIN_SMOOTH_FRAMES);
    (-1.0 / frames).exp()
}

/// Smooth a gain curve with separate attack and release.
///
/// Asymmetric on purpose, and it is what separates a processor from a
/// step function: gain reduction should arrive quickly and leave slowly,
/// or every syllable pumps.
fn smooth(target: &[f64], attack: f64, release: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity(target.len());
    let mut state = target.first().copied().unwrap_or(0.0);
    for &t in target {
        // "Attack" is gain going *down*, which is the direction that
        // has to be fast.
        let c = if t < state { attack } else { release };
        state = t + (state - t) * c;
        out.push(state);
    }
    out
}

/// Run the enabled detectors over a take.
pub fn analyse(
    features: &[FrameFeature],
    voiced: &[bool],
    frame_rate: f64,
    cfg: &DynamicsConfig,
) -> Dynamics {
    let n = features.len();
    let mut out = Dynamics::default();
    if n == 0 {
        return out;
    }
    let levels: Vec<f64> = features.iter().map(|f| to_db(f.rms)).collect();

    if let Some(g) = cfg.gate {
        let target: Vec<f64> = levels
            .iter()
            .map(|&l| if l < g.threshold_db { g.floor_db } else { 0.0 })
            .collect();
        out.gate = curve(&smooth(
            &target,
            coeff(g.attack_ms, frame_rate),
            coeff(g.release_ms, frame_rate),
        ));
    }

    if let Some(c) = cfg.compressor {
        let ratio = c.ratio.max(1.0);
        let target: Vec<f64> = levels
            .iter()
            .map(|&l| {
                let over = l - c.threshold_db;
                // Soft knee: a hard corner is audible on a vocal as the
                // level crosses it back and forth.
                let effective =
                    if c.knee_db > 0.0 && over > -c.knee_db / 2.0 && over < c.knee_db / 2.0 {
                        let x = over + c.knee_db / 2.0;
                        x * x / (2.0 * c.knee_db)
                    } else if over > 0.0 {
                        over
                    } else {
                        0.0
                    };
                -(effective - effective / ratio)
            })
            .collect();
        out.compressor = curve(&smooth(
            &target,
            coeff(c.attack_ms, frame_rate),
            coeff(c.release_ms, frame_rate),
        ));
    }

    // Both detectors look for runs of unvoiced frames and then ask what
    // kind of sound it was.
    if cfg.breath.is_some() || cfg.sibilance.is_some() {
        for (start, end) in unvoiced_runs(voiced, n) {
            let peak = features[start..=end]
                .iter()
                .map(|f| f.rms)
                .fold(0.0_f64, f64::max);
            let peak_db = to_db(peak);
            let ms = (end - start + 1) as f64 / frame_rate * 1000.0;
            let mean = |get: fn(&FrameFeature) -> f64| {
                features[start..=end].iter().map(get).sum::<f64>() / (end - start + 1) as f64
            };

            if cfg.sibilance.is_some_and(|s| {
                ms >= s.min_ms
                    && peak_db >= s.min_level_db
                    && mean(|f| f.high_ratio) >= s.min_high_ratio
                    && mean(|f| f.zcr) >= s.min_zcr
            }) {
                {
                    out.regions.push(Region {
                        kind: Detection::Sibilance,
                        start,
                        end,
                        peak,
                    });
                    continue;
                }
            }
            if cfg
                .breath
                .is_some_and(|b| ms >= b.min_ms && peak_db <= b.max_level_db)
            {
                {
                    out.regions.push(Region {
                        kind: Detection::Breath,
                        start,
                        end,
                        peak,
                    });
                }
            }
        }

        if let Some(b) = cfg.breath.filter(|b| b.reduction_db != 0.0) {
            out.breath = region_curve(&out.regions, Detection::Breath, n, b.reduction_db);
        }
        if let Some(s) = cfg.sibilance.filter(|s| s.reduction_db != 0.0) {
            out.sibilance = region_curve(&out.regions, Detection::Sibilance, n, s.reduction_db);
        }
    }

    out
}

/// Frames with no pitch, as runs.
fn unvoiced_runs(voiced: &[bool], n: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut run: Option<usize> = None;
    for i in 0..n {
        let v = voiced.get(i).copied().unwrap_or(true);
        match (v, run) {
            (false, None) => run = Some(i),
            (true, Some(s)) => {
                out.push((s, i - 1));
                run = None;
            }
            _ => {}
        }
    }
    if let Some(s) = run {
        out.push((s, n - 1));
    }
    out
}

/// A gain curve that is flat except inside the matching regions.
fn region_curve(
    regions: &[Region],
    kind: Detection,
    frames: usize,
    reduction_db: f64,
) -> Vec<GainPoint> {
    let mut db = vec![0.0; frames];
    for r in regions.iter().filter(|r| r.kind == kind) {
        for slot in db.iter_mut().take(r.end.min(frames - 1) + 1).skip(r.start) {
            *slot = reduction_db;
        }
    }
    curve(&db)
}

fn curve(db: &[f64]) -> Vec<GainPoint> {
    db.iter()
        .enumerate()
        .map(|(frame, &db)| GainPoint { frame, db })
        .collect()
}
