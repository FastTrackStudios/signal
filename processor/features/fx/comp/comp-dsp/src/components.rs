//! Composable mono compressor components. Levels are dBFS; reduction is positive dB.

/// A detector owns the history needed to turn audio into a level.
pub trait LevelDetector {
    fn level_db(&mut self, sample: f64) -> f64;
    fn reset(&mut self);
}

/// A memoryless relationship between input level and requested attenuation.
pub trait GainComputer {
    fn reduction_db(&self, level_db: f64) -> f64;
}

/// A gain element's response to requested attenuation, in positive dB.
pub trait Envelope {
    fn reduction_db(&mut self, target_db: f64) -> f64;
    fn reset(&mut self);
}

/// A coloration stage; each channel owns its own instance.
pub trait Coloration {
    fn process(&mut self, sample: f64) -> f64;
    fn reset(&mut self);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Transparent;
impl Coloration for Transparent {
    fn process(&mut self, sample: f64) -> f64 {
        sample
    }
    fn reset(&mut self) {}
}

/// Peak follower with instantaneous rise and exponential decay.
#[derive(Debug, Clone)]
pub struct PeakDetector {
    coefficient: f64,
    peak: f64,
}
impl PeakDetector {
    /// Caller supplies a finite positive rate and decay time.
    #[must_use]
    pub fn new(sample_rate: f64, decay_ms: f64) -> Self {
        Self {
            coefficient: (-1.0 / (decay_ms * 0.001 * sample_rate).max(1.0)).exp(),
            peak: 0.0,
        }
    }
}
impl LevelDetector for PeakDetector {
    fn level_db(&mut self, sample: f64) -> f64 {
        let rectified = sample.abs();
        self.peak = if rectified > self.peak {
            rectified
        } else {
            (self.peak - rectified).mul_add(self.coefficient, rectified)
        };
        20.0 * self.peak.max(1e-9).log10()
    }
    fn reset(&mut self) {
        self.peak = 0.0;
    }
}

/// Hard-knee downward compression. Use a different gain computer for a measured law.
#[derive(Debug, Clone, Copy)]
pub struct HardKnee {
    pub threshold_db: f64,
    pub ratio: f64,
}
impl GainComputer for HardKnee {
    fn reduction_db(&self, level_db: f64) -> f64 {
        (level_db - self.threshold_db).max(0.0) * (1.0 - self.ratio.recip())
    }
}

#[derive(Debug, Clone)]
pub struct AttackRelease {
    attack: f64,
    release: f64,
    reduction: f64,
}
impl AttackRelease {
    #[must_use]
    pub fn new(sample_rate: f64, attack_ms: f64, release_ms: f64) -> Self {
        let coefficient = |ms: f64| {
            if ms == 0.0 {
                0.0
            } else {
                (-1000.0 / (ms * sample_rate)).exp()
            }
        };
        Self {
            attack: coefficient(attack_ms),
            release: coefficient(release_ms),
            reduction: 0.0,
        }
    }
}
impl AttackRelease {
    pub(crate) const fn apply_coefficients(&mut self, prepared: &Self) {
        self.attack = prepared.attack;
        self.release = prepared.release;
    }
}
impl Envelope for AttackRelease {
    fn reduction_db(&mut self, target_db: f64) -> f64 {
        let coefficient = if target_db > self.reduction {
            self.attack
        } else {
            self.release
        };
        self.reduction = coefficient.mul_add(self.reduction, (1.0 - coefficient) * target_db);
        self.reduction
    }
    fn reset(&mut self) {
        self.reduction = 0.0;
    }
}
impl Envelope for crate::opto::OptoCell {
    fn reduction_db(&mut self, target_db: f64) -> f64 {
        self.process(target_db)
    }
    fn reset(&mut self) {
        Self::reset(self);
    }
}

/// Feed-forward compressor topology with independently replaceable components.
/// A model with feedback or a different stage order can implement its own topology.
#[derive(Debug, Clone)]
pub struct Compressor<D, G, E, C = Transparent> {
    pub detector: D,
    pub gain_computer: G,
    pub envelope: E,
    pub coloration: C,
    pub makeup_db: f64,
    reduction_db: f64,
}
impl<D: LevelDetector, G: GainComputer, E: Envelope, C: Coloration> Compressor<D, G, E, C> {
    pub const fn new(detector: D, gain_computer: G, envelope: E, coloration: C) -> Self {
        Self {
            detector,
            gain_computer,
            envelope,
            coloration,
            makeup_db: 0.0,
            reduction_db: 0.0,
        }
    }
    pub fn process(&mut self, input: f64) -> f64 {
        self.process_with_sidechain(input, input)
    }
    pub fn process_with_sidechain(&mut self, input: f64, sidechain: f64) -> f64 {
        let target = self
            .gain_computer
            .reduction_db(self.detector.level_db(sidechain));
        self.reduction_db = self.envelope.reduction_db(target);
        self.coloration
            .process(input * 10.0f64.powf((self.makeup_db - self.reduction_db) / 20.0))
    }
    pub const fn gain_reduction_db(&self) -> f64 {
        self.reduction_db
    }
    pub fn reset(&mut self) {
        self.detector.reset();
        self.envelope.reset();
        self.coloration.reset();
        self.reduction_db = 0.0;
    }
}
