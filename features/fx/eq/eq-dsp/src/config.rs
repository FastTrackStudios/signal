//! Editable application configuration. No DSP history lives here.

use crate::runtime::band::Placement;
use crate::{Error, FilterType, PreparedFilter};

/// Runtime limits fixed before entering the audio callback.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessSpec {
    sample_rate: f64,
    max_block_size: usize,
}
impl ProcessSpec {
    ///
    /// # Errors
    /// Returns an error if the supplied configuration or processing limits are invalid.
    pub fn new(sample_rate: f64, max_block_size: usize) -> Result<Self, Error> {
        crate::filter::validate_sample_rate(sample_rate)?;
        if max_block_size == 0 || u32::try_from(max_block_size).is_err() {
            return Err(Error::InvalidBlockSize);
        }
        Ok(Self {
            sample_rate,
            max_block_size,
        })
    }
    #[must_use]
    pub const fn sample_rate(self) -> f64 {
        self.sample_rate
    }
    #[must_use]
    pub const fn max_block_size(self) -> usize {
        self.max_block_size
    }
    pub(crate) const fn check(self, left: usize, right: usize) -> Result<(), Error> {
        if left != right {
            return Err(Error::ChannelLengthMismatch);
        }
        if left > self.max_block_size {
            return Err(Error::BlockTooLarge);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CutSlope {
    DbPerOctave(f64),
    Brickwall,
}

/// Steepness of a bounded bell, shelf, notch or all-pass response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Steepness {
    Order1,
    Order2,
    Order3,
    Order4,
    Order5,
    Order6,
    Order8,
    Order12,
    Order16,
}
impl Steepness {
    pub(crate) const fn order(self) -> usize {
        match self {
            Self::Order1 => 1,
            Self::Order2 => 2,
            Self::Order3 => 3,
            Self::Order4 => 4,
            Self::Order5 => 5,
            Self::Order6 => 6,
            Self::Order8 => 8,
            Self::Order12 => 12,
            Self::Order16 => 16,
        }
    }
}

/// Shape-specific settings. Gain is present only where it has meaning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Filter {
    Bell {
        frequency_hz: f64,
        gain_db: f64,
        q: f64,
        steepness: Steepness,
    },
    LowShelf {
        frequency_hz: f64,
        gain_db: f64,
        q: f64,
        steepness: Steepness,
    },
    HighShelf {
        frequency_hz: f64,
        gain_db: f64,
        q: f64,
        steepness: Steepness,
    },
    HighPass {
        frequency_hz: f64,
        q: f64,
        slope: CutSlope,
    },
    LowPass {
        frequency_hz: f64,
        q: f64,
        slope: CutSlope,
    },
    Notch {
        frequency_hz: f64,
        q: f64,
        steepness: Steepness,
    },
    BandPass {
        frequency_hz: f64,
        q: f64,
        steepness: Steepness,
    },
    Tilt {
        frequency_hz: f64,
        gain_db: f64,
        q: f64,
        steepness: Steepness,
    },
    FlatTilt {
        frequency_hz: f64,
        gain_db: f64,
        q: f64,
        steepness: Steepness,
    },
    AllPass {
        frequency_hz: f64,
        q: f64,
        steepness: Steepness,
    },
    BandShelf {
        frequency_hz: f64,
        gain_db: f64,
        q: f64,
        steepness: Steepness,
    },
}

impl Filter {
    ///
    /// # Errors
    /// Returns an error for invalid parameters, unsupported slopes, or an unstable filter design.
    pub fn prepare(self, sample_rate: f64) -> Result<PreparedFilter, Error> {
        let (shape, hz, gain, q, order, fraction) = self.parts()?;
        PreparedFilter::design(shape, hz, gain, q, order, fraction, sample_rate)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive shape-to-design parameter table keeps each shape's units and supported order together"
    )]
    pub(crate) fn parts(self) -> Result<(FilterType, f64, f64, f64, usize, f64), Error> {
        if matches!(
            self,
            Self::Bell {
                steepness: Steepness::Order1,
                ..
            } | Self::Notch {
                steepness: Steepness::Order1,
                ..
            }
        ) {
            return Err(Error::InvalidSlope);
        }
        let (shape, hz, gain, q, order) = match self {
            Self::Bell {
                frequency_hz,
                gain_db,
                q,
                steepness,
            } => (
                FilterType::Peak,
                frequency_hz,
                gain_db,
                q,
                steepness.order().max(2),
            ),
            Self::LowShelf {
                frequency_hz,
                gain_db,
                q,
                steepness,
            } => (
                FilterType::LowShelf,
                frequency_hz,
                gain_db,
                q,
                steepness.order(),
            ),
            Self::HighShelf {
                frequency_hz,
                gain_db,
                q,
                steepness,
            } => (
                FilterType::HighShelf,
                frequency_hz,
                gain_db,
                q,
                steepness.order(),
            ),
            Self::Notch {
                frequency_hz,
                q,
                steepness,
            } => (
                FilterType::Notch,
                frequency_hz,
                0.0,
                q,
                steepness.order().max(2),
            ),
            Self::BandPass {
                frequency_hz,
                q,
                steepness,
            } => (
                FilterType::Bandpass,
                frequency_hz,
                0.0,
                q,
                steepness.order(),
            ),
            Self::Tilt {
                frequency_hz,
                gain_db,
                q,
                steepness,
            } => (
                FilterType::TiltShelf,
                frequency_hz,
                gain_db,
                q,
                steepness.order(),
            ),
            Self::FlatTilt {
                frequency_hz,
                gain_db,
                q,
                steepness,
            } => (
                FilterType::FlatTilt,
                frequency_hz,
                gain_db,
                q,
                steepness.order(),
            ),
            Self::AllPass {
                frequency_hz,
                q,
                steepness,
            } => (FilterType::Allpass, frequency_hz, 0.0, q, steepness.order()),
            Self::BandShelf {
                frequency_hz,
                gain_db,
                q,
                steepness,
            } => (
                FilterType::BandShelf,
                frequency_hz,
                gain_db,
                q,
                steepness.order(),
            ),
            Self::HighPass {
                frequency_hz,
                q,
                slope,
            }
            | Self::LowPass {
                frequency_hz,
                q,
                slope,
            } => {
                let shape = if matches!(self, Self::HighPass { .. }) {
                    FilterType::Highpass
                } else {
                    FilterType::Lowpass
                };
                let (order, fraction) = match slope {
                    CutSlope::Brickwall => (crate::design::slope::BRICKWALL_ORDER, 0.0),
                    CutSlope::DbPerOctave(db) if db.is_finite() && (0.0..=36.0).contains(&db) => {
                        let poles = db / 6.0;
                        (dsp_core::num::f64_to_index(poles.floor()), poles.fract())
                    }
                    CutSlope::DbPerOctave(48.0) => (8, 0.0),
                    CutSlope::DbPerOctave(72.0) => (12, 0.0),
                    CutSlope::DbPerOctave(96.0) => (16, 0.0),
                    CutSlope::DbPerOctave(_) => return Err(Error::InvalidSlope),
                };
                return Ok((shape, frequency_hz, 0.0, q, order, fraction));
            }
        };
        Ok((shape, hz, gain, q, order, 0.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stream {
    #[default]
    Both,
    Transient,
    Steady,
}
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Threshold {
    FixedDb(f64),
    #[default]
    Auto,
}
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DetectorSource {
    #[default]
    Band,
    FrequencyRange {
        low_hz: f64,
        high_hz: f64,
    },
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ballistics {
    Milliseconds {
        attack: f64,
        release: f64,
    },
    /// Frequency-dependent attack and 300 ms-centered release compatibility law.
    ProQPercent {
        attack: f64,
        release: f64,
    },
}
impl Default for Ballistics {
    fn default() -> Self {
        Self::Milliseconds {
            attack: 1.0,
            release: 300.0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DynamicsMode {
    #[default]
    Static,
    Dynamic,
    Spectral,
}

/// Settings persist when mode is switched back to Static.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dynamics {
    pub mode: DynamicsMode,
    pub range_db: f64,
    pub threshold: Threshold,
    pub ballistics: Ballistics,
    pub detector: DetectorSource,
    pub relative: bool,
    /// Spectral selectivity, 0..=1.
    pub density: f64,
    pub spectral_tilt: bool,
}
impl Default for Dynamics {
    fn default() -> Self {
        Self {
            mode: DynamicsMode::Static,
            range_db: 0.0,
            threshold: Threshold::Auto,
            ballistics: Ballistics::default(),
            detector: DetectorSource::Band,
            relative: false,
            density: 0.5,
            spectral_tilt: false,
        }
    }
}
impl Dynamics {
    pub(crate) fn validate(self, rate: f64) -> Result<(), Error> {
        if !self.range_db.is_finite()
            || !self.density.is_finite()
            || !(0.0..=1.0).contains(&self.density)
        {
            return Err(Error::InvalidDynamics);
        }
        if let Threshold::FixedDb(db) = self.threshold
            && !db.is_finite()
        {
            return Err(Error::InvalidDynamics);
        }
        let valid = match self.ballistics {
            Ballistics::Milliseconds { attack, release } => {
                attack.is_finite() && release.is_finite() && attack > 0.0 && release > 0.0
            }
            Ballistics::ProQPercent { attack, release } => {
                (0.0..=100.0).contains(&attack) && (0.0..=100.0).contains(&release)
            }
        };
        if !valid {
            return Err(Error::InvalidDynamics);
        }
        if let DetectorSource::FrequencyRange { low_hz, high_hz } = self.detector {
            crate::filter::validate_frequency(low_hz, rate)?;
            crate::filter::validate_frequency(high_hz, rate)?;
            if low_hz >= high_hz {
                return Err(Error::InvalidDynamics);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandConfig {
    pub filter: Filter,
    pub enabled: bool,
    pub placement: Placement,
    pub stream: Stream,
    pub dynamics: Dynamics,
}
impl BandConfig {
    #[must_use]
    pub fn new(filter: Filter) -> Self {
        Self {
            filter,
            enabled: true,
            placement: Placement::Stereo,
            stream: Stream::Both,
            dynamics: Dynamics::default(),
        }
    }
    #[must_use]
    pub fn bell(frequency_hz: f64, gain_db: f64, q: f64) -> Self {
        Self::new(Filter::Bell {
            frequency_hz,
            gain_db,
            q,
            steepness: Steepness::Order2,
        })
    }
    #[must_use]
    pub fn high_pass(frequency_hz: f64, slope: CutSlope) -> Self {
        Self::new(Filter::HighPass {
            frequency_hz,
            q: core::f64::consts::FRAC_1_SQRT_2,
            slope,
        })
    }
    #[must_use]
    pub fn low_pass(frequency_hz: f64, slope: CutSlope) -> Self {
        Self::new(Filter::LowPass {
            frequency_hz,
            q: core::f64::consts::FRAC_1_SQRT_2,
            slope,
        })
    }
    #[must_use]
    pub const fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }
    #[must_use]
    pub const fn dynamics(mut self, dynamics: Dynamics) -> Self {
        self.dynamics = dynamics;
        self
    }
    #[must_use]
    pub const fn stream(mut self, stream: Stream) -> Self {
        self.stream = stream;
        self
    }
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Opaque identity, monotonically assigned within a configuration lineage.
/// Clones preserve identities. Removing a band never reuses its identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BandId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Character {
    #[default]
    Clean,
    Subtle,
    Warm,
}
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Pan {
    #[default]
    Center,
    LeftRight(f64),
    MidSide(f64),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listen {
    Band(BandId),
    Delta,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Output {
    pub gain_db: f64,
    pub gain_scale: f64,
    pub auto_gain: bool,
    pub character: Character,
    pub pan: Pan,
}
impl Default for Output {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            gain_scale: 1.0,
            auto_gain: false,
            character: Character::Clean,
            pan: Pan::Center,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transient {
    pub enabled: bool,
    pub transient_gain_db: f64,
    pub steady_gain_db: f64,
    pub solo: Stream,
    pub balance: f64,
    pub attack_percent: f64,
    pub hold_percent: f64,
    pub smooth_percent: f64,
}
impl Default for Transient {
    fn default() -> Self {
        Self {
            enabled: false,
            transient_gain_db: 0.0,
            steady_gain_db: 0.0,
            solo: Stream::Both,
            balance: 0.0,
            attack_percent: 50.0,
            hold_percent: 50.0,
            smooth_percent: 50.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EqConfig {
    pub(crate) bands: Vec<(BandId, BandConfig)>,
    capacity: usize,
    next_id: u64,
    pub output: Output,
    pub transient: Transient,
    pub listen: Option<Listen>,
}
impl Default for EqConfig {
    fn default() -> Self {
        Self::with_capacity(24)
    }
}
impl EqConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bands: Vec::with_capacity(capacity),
            capacity,
            next_id: 0,
            output: Output::default(),
            transient: Transient::default(),
            listen: None,
        }
    }
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }
    ///
    /// # Errors
    /// Returns `CapacityExceeded` when no band slot or identity is available.
    pub fn add_band(&mut self, band: BandConfig) -> Result<BandId, Error> {
        if self.bands.len() == self.capacity {
            return Err(Error::CapacityExceeded);
        }
        let next = self.next_id.checked_add(1).ok_or(Error::CapacityExceeded)?;
        let id = BandId(self.next_id);
        self.next_id = next;
        self.bands.push((id, band));
        Ok(id)
    }
    ///
    /// # Errors
    /// Returns `UnknownBand` if this identity is absent or has been removed.
    pub fn band(&self, id: BandId) -> Result<&BandConfig, Error> {
        self.bands
            .iter()
            .find(|(key, _)| *key == id)
            .map(|(_, band)| band)
            .ok_or(Error::UnknownBand)
    }
    ///
    /// # Errors
    /// Returns `UnknownBand` if this identity is absent or has been removed.
    pub fn band_mut(&mut self, id: BandId) -> Result<&mut BandConfig, Error> {
        self.bands
            .iter_mut()
            .find(|(key, _)| *key == id)
            .map(|(_, band)| band)
            .ok_or(Error::UnknownBand)
    }
    ///
    /// # Errors
    /// Returns `UnknownBand` if this identity is absent or has been removed.
    pub fn remove_band(&mut self, id: BandId) -> Result<BandConfig, Error> {
        let index = self
            .bands
            .iter()
            .position(|(key, _)| *key == id)
            .ok_or(Error::UnknownBand)?;
        if self.listen == Some(Listen::Band(id)) {
            self.listen = None;
        }
        Ok(self.bands.remove(index).1)
    }
    /// Move a band to a processing position, retaining its identity.
    ///
    /// # Errors
    /// Returns `UnknownBand` for an absent identity or a position outside the current band list.
    pub fn move_band(&mut self, id: BandId, position: usize) -> Result<(), Error> {
        if position >= self.bands.len() {
            return Err(Error::UnknownBand);
        }
        let index = self
            .bands
            .iter()
            .position(|(key, _)| *key == id)
            .ok_or(Error::UnknownBand)?;
        let band = self.bands.remove(index);
        self.bands.insert(position, band);
        Ok(())
    }
    #[must_use]
    pub fn bands(&self) -> impl ExactSizeIterator<Item = (BandId, &BandConfig)> {
        self.bands.iter().map(|(id, b)| (*id, b))
    }
    ///
    /// # Errors
    /// Returns an error for invalid parameters, unsupported slopes, or an unstable filter design.
    pub fn prepare(&self, spec: ProcessSpec) -> Result<crate::PreparedEq, Error> {
        crate::PreparedEq::new(self, spec)
    }
}
