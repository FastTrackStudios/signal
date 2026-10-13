//! Prepared EQ configuration and its allocation-free processor.

use crate::config::{Ballistics, BandId, DynamicsMode, EqConfig, Listen, Pan, ProcessSpec, Stream};
use crate::math::zpk::Complex;
use crate::{Error, Placement, PreparedFilter};

/// Complex stereo transfer matrix, row = output and column = input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StereoResponse {
    pub ll: Complex,
    pub lr: Complex,
    pub rl: Complex,
    pub rr: Complex,
}
#[expect(
    clippy::arithmetic_side_effects,
    reason = "complex arithmetic operates on f64 and cannot panic"
)]
impl StereoResponse {
    const IDENTITY: Self = Self {
        ll: Complex::ONE,
        lr: Complex::ZERO,
        rl: Complex::ZERO,
        rr: Complex::ONE,
    };
    #[expect(
        clippy::suspicious_operation_groupings,
        reason = "standard row-by-column complex matrix multiplication"
    )]
    fn then(self, next: Self) -> Self {
        Self {
            ll: next.ll * self.ll + next.lr * self.rl,
            lr: next.ll * self.lr + next.lr * self.rr,
            rl: next.rl * self.ll + next.rr * self.rl,
            rr: next.rl * self.lr + next.rr * self.rr,
        }
    }
    fn band(h: Complex, placement: Placement) -> Self {
        let sum = (Complex::ONE + h) * 0.5;
        let diff = (h - Complex::ONE) * 0.5;
        match placement {
            Placement::Stereo => Self {
                ll: h,
                rr: h,
                ..Self::IDENTITY
            },
            Placement::Left => Self {
                ll: h,
                ..Self::IDENTITY
            },
            Placement::Right => Self {
                rr: h,
                ..Self::IDENTITY
            },
            Placement::Mid => Self {
                ll: sum,
                rr: sum,
                lr: diff,
                rl: diff,
            },
            Placement::Side => Self {
                ll: sum,
                rr: sum,
                lr: -diff,
                rl: -diff,
            },
        }
    }
}

/// Immutable validated intent and designs. Preparation may allocate.
#[derive(Debug, Clone)]
pub struct PreparedEq {
    pub(crate) config: EqConfig,
    spec: ProcessSpec,
    filters: Vec<PreparedFilter>,
}
impl PreparedEq {
    pub(crate) fn new(config: &EqConfig, spec: ProcessSpec) -> Result<Self, Error> {
        let output = config.output;
        if ![
            output.gain_db,
            output.gain_scale,
            config.transient.transient_gain_db,
            config.transient.steady_gain_db,
        ]
        .iter()
        .all(|v| v.is_finite())
        {
            return Err(Error::InvalidGain);
        }
        if !(-50.0..=50.0).contains(&config.transient.balance)
            || ![
                config.transient.attack_percent,
                config.transient.hold_percent,
                config.transient.smooth_percent,
            ]
            .iter()
            .all(|v| (0.0..=100.0).contains(v))
        {
            return Err(Error::InvalidDynamics);
        }
        if let Pan::LeftRight(p) | Pan::MidSide(p) = output.pan
            && !(-1.0..=1.0).contains(&p)
        {
            return Err(Error::InvalidGain);
        }
        if let Some(Listen::Band(id)) = config.listen {
            config.band(id)?;
        }
        let filters = config
            .bands()
            .map(|(_, band)| {
                band.dynamics.validate(spec.sample_rate())?;
                if band.dynamics.mode == DynamicsMode::Spectral
                    && band.placement != Placement::Stereo
                {
                    return Err(Error::UnsupportedRouting);
                }
                if config.transient.enabled
                    && band.dynamics.mode != DynamicsMode::Static
                    && (band.stream != Stream::Both
                        || !matches!(
                            band.filter,
                            crate::Filter::Bell { .. }
                                | crate::Filter::LowShelf { .. }
                                | crate::Filter::HighShelf { .. }
                        ))
                {
                    return Err(Error::UnsupportedRouting);
                }
                if !(band.dynamics.range_db * config.output.gain_scale).is_finite() {
                    return Err(Error::InvalidDynamics);
                }
                let (shape, hz, gain, q, order, fraction) = band.filter.parts()?;
                // The established dynamic engine has a documented parameter domain.
                if !(10.0..=30_000.0).contains(&hz) {
                    return Err(Error::InvalidFrequency);
                }
                if !(0.025..=40.0).contains(&q) {
                    return Err(Error::InvalidQ);
                }
                if !(-30.0..=30.0).contains(&gain) {
                    return Err(Error::InvalidGain);
                }
                let q = if shape == crate::FilterType::FlatTilt {
                    q.min(1.884_955_592_153_876)
                } else {
                    q
                };
                PreparedFilter::design(
                    shape,
                    hz,
                    gain * output.gain_scale,
                    q,
                    order,
                    fraction,
                    spec.sample_rate(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            config: config.clone(),
            spec,
            filters,
        })
    }
    #[must_use]
    pub const fn spec(&self) -> ProcessSpec {
        self.spec
    }
    #[must_use]
    pub const fn config(&self) -> &EqConfig {
        &self.config
    }
    ///
    /// # Errors
    /// Returns `UnknownBand` if the identity is not in this preparation.
    pub fn filter(&self, id: BandId) -> Result<&PreparedFilter, Error> {
        self.config
            .bands
            .iter()
            .position(|(key, _)| *key == id)
            .and_then(|i| self.filters.get(i))
            .ok_or(Error::UnknownBand)
    }
    /// Configured base EQ curve, including placement. Excludes dynamics,
    /// transient splitting, output controls, listening and nonlinear character.
    /// For static bands this is the exact settled cascade response.
    ///
    /// # Errors
    /// Returns `InvalidFrequency` for a frequency outside DC through Nyquist.
    pub fn base_response(&self, hz: f64) -> Result<StereoResponse, Error> {
        if !hz.is_finite() || !(0.0..=self.spec.sample_rate() * 0.5).contains(&hz) {
            return Err(Error::InvalidFrequency);
        }
        let mut result = StereoResponse::IDENTITY;
        for ((_, band), filter) in self.config.bands.iter().zip(&self.filters) {
            if band.enabled {
                result = result.then(StereoResponse::band(filter.response(hz)?, band.placement));
            }
        }
        Ok(result)
    }
    #[must_use]
    pub fn latency_samples(&self) -> usize {
        if self.config.bands().any(|(_, b)| {
            b.enabled
                && b.dynamics.mode == DynamicsMode::Spectral
                && b.dynamics.range_db.abs() > 1e-3
        }) {
            4095
        } else {
            0
        }
    }
}

/// Audio state prepared for a fixed sample rate, maximum block size and capacity.
///
/// Construction and drop belong off the audio thread. Processing, reset and
/// applying a compatible prepared configuration do not allocate.
pub struct EqProcessor {
    engine: crate::engine::FtsEq,
    spec: ProcessSpec,
    ids: Vec<BandId>,
    reset_filters: Vec<PreparedFilter>,
    capacity: usize,
    installed: bool,
    mono_right: Vec<f64>,
}
impl EqProcessor {
    ///
    /// # Errors
    /// Returns an error if the supplied configuration or processing limits are invalid.
    pub fn new(prepared: &PreparedEq) -> Result<Self, Error> {
        let capacity = prepared.config.capacity();
        let mut engine = crate::engine::FtsEq::with_capacity(prepared.spec.sample_rate(), capacity);
        engine.prepare(
            prepared.spec.sample_rate(),
            u32::try_from(prepared.spec.max_block_size()).map_err(|_| Error::InvalidBlockSize)?,
        );
        let mut result = Self {
            engine,
            spec: prepared.spec,
            ids: Vec::with_capacity(capacity),
            reset_filters: Vec::with_capacity(capacity),
            capacity,
            installed: false,
            mono_right: vec![0.0; prepared.spec.max_block_size()],
        };
        result.apply(prepared)?;
        result.reset();
        Ok(result)
    }
    /// Apply a validated configuration at a block boundary, retaining history
    /// for unchanged identities. Latency changes require a newly constructed
    /// processor so the host can renegotiate compensation before activation.
    /// The borrowed preparation remains owned by the control side.
    ///
    /// # Errors
    /// Returns `IncompatiblePreparation` for a different sample rate, larger capacity/block specification, or changed latency. The processor is unchanged on failure.
    pub fn apply(&mut self, prepared: &PreparedEq) -> Result<(), Error> {
        if self.spec != prepared.spec || prepared.config.capacity() > self.capacity {
            return Err(Error::IncompatiblePreparation);
        }
        if self.installed && self.latency_samples() != prepared.latency_samples() {
            return Err(Error::IncompatiblePreparation);
        }
        let config = &prepared.config;
        self.engine
            .configure_globals(config.output, config.transient);
        for (i, ((id, band), filter)) in config.bands.iter().zip(&prepared.filters).enumerate() {
            if self.ids.get(i) != Some(id) {
                self.engine.reset_band(i);
            }
            let (cfg, dynamics) = crate::host::encode_band(*band)?;
            self.engine.install_band(i, cfg, dynamics, filter);
            if let Ballistics::Milliseconds { attack, release } = band.dynamics.ballistics {
                self.engine.set_ballistics_ms(i, attack, release);
            }
        }
        self.engine.disable_unused(config.bands.len());
        self.engine.finish_update();
        self.reset_filters.clear();
        self.reset_filters.extend(prepared.filters.iter().cloned());
        self.ids.clear();
        self.ids.extend(config.bands.iter().map(|(id, _)| *id));
        self.engine.set_listen(match config.listen {
            Some(Listen::Band(id)) => self.ids.iter().position(|key| *key == id).map(|i| (i, 1)),
            Some(Listen::Delta) => Some((0, 2)),
            None => None,
        });
        self.installed = true;
        Ok(())
    }
    ///
    /// # Errors
    /// Returns an error for unequal channel lengths or a block larger than prepared capacity. Audio and history are unchanged on failure.
    pub fn process_stereo(&mut self, left: &mut [f64], right: &mut [f64]) -> Result<(), Error> {
        self.spec.check(left.len(), right.len())?;
        if left.is_empty() {
            return Ok(());
        }
        self.engine.process(left, right);
        Ok(())
    }
    /// Mono is processed as a centered signal (L = R); the returned sample is
    /// the average of the stereo output, so Mid/Side and pan remain defined.
    ///
    /// # Errors
    /// Returns `BlockTooLarge` if the block exceeds prepared capacity. Audio and history are unchanged on failure.
    pub fn process_mono(&mut self, samples: &mut [f64]) -> Result<(), Error> {
        self.spec.check(samples.len(), samples.len())?;
        if samples.is_empty() {
            return Ok(());
        }
        let right = self
            .mono_right
            .get_mut(..samples.len())
            .ok_or(Error::BlockTooLarge)?;
        right.copy_from_slice(samples);
        self.engine.process(samples, right);
        for (l, r) in samples.iter_mut().zip(right) {
            *l = 0.5 * (*l + *r);
        }
        Ok(())
    }
    pub fn reset(&mut self) {
        self.engine.reset();
        self.engine.restore_filters(&self.reset_filters);
        self.mono_right.fill(0.0);
    }
    #[must_use]
    pub fn latency_samples(&self) -> usize {
        dsp_core::num::u32_to_index(self.engine.latency())
    }
    ///
    /// # Errors
    /// Returns `UnknownBand` if the identity is not currently installed.
    pub fn live_gain_db(&self, id: BandId) -> Result<Option<f64>, Error> {
        let i = self
            .ids
            .iter()
            .position(|key| *key == id)
            .ok_or(Error::UnknownBand)?;
        Ok(self.engine.live_dyn_gain_db(i))
    }
}

pub const fn stream_index(stream: Stream) -> u32 {
    match stream {
        Stream::Both => 0,
        Stream::Transient => 1,
        Stream::Steady => 2,
    }
}

impl EqProcessor {
    /// Last rejected detector-driven cascade redesign. The previous stable
    /// coefficients remain installed if a live design is invalid.
    ///
    /// # Errors
    /// Returns `UnknownBand` for an identity that is not currently installed.
    pub fn last_design_error(&self, id: BandId) -> Result<Option<Error>, Error> {
        let index = self
            .ids
            .iter()
            .position(|key| *key == id)
            .ok_or(Error::UnknownBand)?;
        Ok(self.engine.last_design_error(index))
    }
}
