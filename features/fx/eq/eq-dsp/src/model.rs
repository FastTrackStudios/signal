//! Model-specific controls composed into a linear cascade and coloration.
use crate::hardware::hardware_eq::{PultecEqp1aSettings, build_pultec_eqp1a_sections};
use crate::{Error, FilterProcessor, PreparedFilter, ProcessSpec};

/// Per-channel coloration state. Implementations must not allocate in processing
/// or reset.
///
/// A custom stage can model hysteresis or oversampled saturation;
/// its declared latency must include any buffering it introduces.
pub trait Coloration: Clone {
    fn process(&mut self, sample: f64) -> f64;
    fn reset(&mut self);
    /// Copy prepared controls without allocating or replacing stream history.
    fn update(&mut self, prepared: &Self);
    fn latency_samples(&self) -> usize {
        0
    }
}
/// An explicit approximation, not a measured tube or transformer model.
#[derive(Debug, Clone, Copy, Default)]
pub enum AnalogColoration {
    #[default]
    Clean,
    Arctangent {
        drive: f64,
    },
}
impl Coloration for AnalogColoration {
    fn process(&mut self, sample: f64) -> f64 {
        match *self {
            Self::Clean => sample,
            Self::Arctangent { drive } => {
                let amount = drive.mul_add(4.0, 1.0);
                (sample * amount).atan() / amount.atan()
            }
        }
    }
    fn reset(&mut self) {}
    fn update(&mut self, prepared: &Self) {
        *self = *prepared;
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorationPlacement {
    BeforeFilters,
    AfterFilters,
}

/// An immutable model design. Custom models can construct this directly from
/// their own filter design and coloration type.
#[derive(Clone)]
pub struct PreparedModel<C: Coloration = AnalogColoration> {
    filter: PreparedFilter,
    coloration: C,
    placement: ColorationPlacement,
    output_gain: f64,
    spec: ProcessSpec,
}
impl<C: Coloration> PreparedModel<C> {
    ///
    /// # Errors
    /// Returns an error if configuration or buffer dimensions are invalid.
    pub fn new(
        filter: PreparedFilter,
        coloration: C,
        placement: ColorationPlacement,
        output_db: f64,
        spec: ProcessSpec,
    ) -> Result<Self, Error> {
        if filter.sample_rate().to_bits() != spec.sample_rate().to_bits() {
            return Err(Error::IncompatiblePreparation);
        }
        let output_gain = 10.0f64.powf(output_db / 20.0);
        if !output_db.is_finite() || !output_gain.is_finite() {
            return Err(Error::InvalidGain);
        }
        Ok(Self {
            filter,
            coloration,
            placement,
            output_gain,
            spec,
        })
    }
    /// Small-signal filter cascade only; excludes coloration and output trim.
    pub const fn linear_filter(&self) -> &PreparedFilter {
        &self.filter
    }
    pub fn latency_samples(&self) -> usize {
        self.coloration.latency_samples()
    }
    pub fn processor(&self) -> ModelProcessor<C> {
        let mut coloration = core::array::from_fn(|_| self.coloration.clone());
        for stage in &mut coloration {
            stage.reset();
        }
        ModelProcessor {
            filter: FilterProcessor::new(&self.filter),
            coloration,
            placement: self.placement,
            output_gain: self.output_gain,
            target_gain: self.output_gain,
            gain_coefficient: 1.0 - (-1.0 / (self.spec.sample_rate() * 0.005)).exp(),
            spec: self.spec,
        }
    }
}
/// Two coloration histories and one stereo cascade. Construction occurs off
/// the callback; process/reset use the storage supplied at construction.
pub struct ModelProcessor<C: Coloration = AnalogColoration> {
    target_gain: f64,
    gain_coefficient: f64,
    filter: FilterProcessor,
    coloration: [C; 2],
    placement: ColorationPlacement,
    output_gain: f64,
    spec: ProcessSpec,
}
impl<C: Coloration> ModelProcessor<C> {
    /// Apply a compatible model design while retaining histories. The filter
    /// crossfades, output trim ramps, and each coloration handles its own controls.
    /// # Errors
    /// Different stream specs, stage order or latency require a new processor.
    pub fn apply(&mut self, prepared: &PreparedModel<C>) -> Result<(), Error> {
        if self.spec != prepared.spec
            || self.placement != prepared.placement
            || self
                .coloration
                .iter()
                .any(|stage| stage.latency_samples() != prepared.latency_samples())
        {
            return Err(Error::IncompatiblePreparation);
        }
        self.filter.install(&prepared.filter);
        for stage in &mut self.coloration {
            stage.update(&prepared.coloration);
        }
        self.target_gain = prepared.output_gain;
        Ok(())
    }
    pub fn reset(&mut self) {
        self.output_gain = self.target_gain;
        self.filter.reset();
        for stage in &mut self.coloration {
            stage.reset();
        }
    }
    pub fn process_frame(&mut self, mut frame: [f64; 2]) -> [f64; 2] {
        if self.placement == ColorationPlacement::BeforeFilters {
            for (sample, stage) in frame.iter_mut().zip(&mut self.coloration) {
                *sample = stage.process(*sample);
            }
        }
        frame = self.filter.process_frame(frame);
        if self.placement == ColorationPlacement::AfterFilters {
            for (sample, stage) in frame.iter_mut().zip(&mut self.coloration) {
                *sample = stage.process(*sample);
            }
        }
        self.output_gain =
            (self.target_gain - self.output_gain).mul_add(self.gain_coefficient, self.output_gain);
        frame.map(|sample| sample * self.output_gain)
    }
    ///
    /// # Errors
    /// Returns an error if configuration or buffer dimensions are invalid.
    pub fn process_stereo(&mut self, left: &mut [f64], right: &mut [f64]) -> Result<(), Error> {
        if left.len() != right.len() {
            return Err(Error::ChannelLengthMismatch);
        }
        if left.len() > self.spec.max_block_size() {
            return Err(Error::BlockTooLarge);
        }
        for (l, r) in left.iter_mut().zip(right) {
            [*l, *r] = self.process_frame([*l, *r]);
        }
        Ok(())
    }
    ///
    /// # Errors
    /// Returns an error if configuration or buffer dimensions are invalid.
    pub fn process_mono(&mut self, audio: &mut [f64]) -> Result<(), Error> {
        if audio.len() > self.spec.max_block_size() {
            return Err(Error::BlockTooLarge);
        }
        for sample in audio {
            *sample = self.process_frame([*sample, *sample])[0];
        }
        Ok(())
    }
}

impl PultecEqp1aSettings {
    /// Prepare interacting boost/attenuation filters and a measured normalized
    /// coloration. Knobs use the physical units in the settings fields, not
    /// normalized plugin positions. EQ-out retains the modeled amplifier response.
    ///
    /// # Errors
    /// Returns an error if configuration or buffer dimensions are invalid.
    pub fn prepare(
        self,
        spec: ProcessSpec,
    ) -> Result<PreparedModel<crate::pultec_color::PultecColoration>, Error> {
        let in_range = |x: f64, lo: f64, hi: f64| x.is_finite() && (lo..=hi).contains(&x);
        if ![20.0, 30.0, 60.0, 100.0].contains(&self.low_freq_hz)
            || ![3000.0, 4000.0, 5000.0, 8000.0, 10000.0, 12000.0, 16000.0]
                .contains(&self.high_boost_freq_hz)
            || ![5000.0, 10000.0, 20000.0].contains(&self.high_atten_freq_hz)
            || self.high_boost_freq_hz >= spec.sample_rate() * 0.5
            || self.high_atten_freq_hz >= spec.sample_rate() * 0.5
        {
            return Err(Error::InvalidFrequency);
        }
        if ![self.low_boost_db, self.low_atten_db]
            .iter()
            .all(|&x| in_range(x, 0.0, 13.0))
            || ![self.high_boost_db, self.high_atten_db]
                .iter()
                .all(|&x| in_range(x, 0.0, 16.0))
            || !in_range(self.high_bandwidth, 0.0, 10.0)
            || !in_range(self.drive_percent, 0.0, 100.0)
            || !in_range(self.trim_db, -60.0, 24.0)
        {
            return Err(Error::InvalidGain);
        }
        let sections = build_pultec_eqp1a_sections(self, spec.sample_rate());
        let filter = PreparedFilter::from_sections(spec.sample_rate(), &sections)?;
        let coloration =
            crate::pultec_color::PultecColoration::new(self.drive_percent, spec.sample_rate())?;
        PreparedModel::new(
            filter,
            coloration,
            ColorationPlacement::AfterFilters,
            self.trim_db,
            spec,
        )
    }
}
