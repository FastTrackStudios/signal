//! Typed controls and bounded stereo processing for compressor applications.
use crate::components::{AttackRelease, Compressor, HardKnee, PeakDetector, Transparent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidSampleRate,
    InvalidBlockSize,
    InvalidControls,
    ChannelLengthMismatch,
    BlockTooLarge,
    IncompatiblePreparation,
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::InvalidSampleRate => "sample rate must be finite and positive",
            Self::InvalidBlockSize => "maximum block size must be nonzero",
            Self::InvalidControls => "model controls are outside their supported range",
            Self::ChannelLengthMismatch => "stereo channels must have equal lengths",
            Self::IncompatiblePreparation => "model or processing specification changed",
            Self::BlockTooLarge => "block exceeds prepared capacity",
        })
    }
}
impl core::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessSpec {
    sample_rate: f64,
    max_block_size: usize,
}
impl ProcessSpec {
    ///
    /// # Errors
    /// Returns an error if configuration or buffer dimensions are invalid.
    pub fn new(sample_rate: f64, max_block_size: usize) -> Result<Self, Error> {
        if !sample_rate.is_finite() || sample_rate <= 0.0 {
            return Err(Error::InvalidSampleRate);
        }
        if max_block_size == 0 {
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GenericControls {
    pub threshold_db: f64,
    pub ratio: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub makeup_db: f64,
}
impl Default for GenericControls {
    fn default() -> Self {
        Self {
            threshold_db: -18.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 100.0,
            makeup_db: 0.0,
        }
    }
}
/// Native normalized panel positions for the measured Gray model, in Compress mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct La2aControls {
    pub peak_reduction: f64,
    pub gain: f64,
}
impl Default for La2aControls {
    fn default() -> Self {
        Self {
            peak_reduction: 0.232,
            gain: 0.286,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Model {
    Generic(GenericControls),
    La2aGray(La2aControls),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressorConfig {
    pub model: Model,
    /// 0 uses independent detectors, 1 feeds the stereo peak to both detectors.
    pub stereo_link: f64,
    /// Linear wet fraction in 0..=1.
    pub mix: f64,
}
impl CompressorConfig {
    #[must_use]
    pub const fn new(model: Model) -> Self {
        Self {
            model,
            stereo_link: 1.0,
            mix: 1.0,
        }
    }
    ///
    /// # Errors
    /// Returns an error if configuration or buffer dimensions are invalid.
    pub fn prepare(self, spec: ProcessSpec) -> Result<PreparedCompressor, Error> {
        let range = |x: f64, lo: f64, hi: f64| x.is_finite() && (lo..=hi).contains(&x);
        let valid = range(self.stereo_link, 0.0, 1.0)
            && range(self.mix, 0.0, 1.0)
            && match self.model {
                Model::Generic(c) => {
                    range(c.threshold_db, -120.0, 24.0)
                        && range(c.ratio, 1.0, 1000.0)
                        && range(c.attack_ms, 0.0, 10_000.0)
                        && range(c.release_ms, 0.0, 60_000.0)
                        && range(c.makeup_db, -120.0, 40.0)
                }
                Model::La2aGray(c) => range(c.peak_reduction, 0.0, 1.0) && range(c.gain, 0.0, 1.0),
            };
        if !valid {
            return Err(Error::InvalidControls);
        }
        Ok(PreparedCompressor {
            config: self,
            spec,
            channel: ChannelModel::new(self.model, spec.sample_rate),
        })
    }
}
#[derive(Debug, Clone)]
pub struct PreparedCompressor {
    channel: ChannelModel,
    config: CompressorConfig,
    spec: ProcessSpec,
}
impl PreparedCompressor {
    #[must_use]
    pub fn processor(&self) -> CompressorProcessor {
        CompressorProcessor::new(self)
    }
    #[must_use]
    pub const fn config(&self) -> CompressorConfig {
        self.config
    }
    #[must_use]
    pub const fn spec(&self) -> ProcessSpec {
        self.spec
    }
    #[must_use]
    pub const fn latency_samples(&self) -> usize {
        0
    }
}

type Generic = Compressor<PeakDetector, HardKnee, AttackRelease, Transparent>;
#[derive(Debug, Clone)]
enum ChannelModel {
    Generic(Generic),
    La2a(crate::la2a::La2a),
}
impl ChannelModel {
    fn new(model: Model, sample_rate: f64) -> Self {
        match model {
            Model::Generic(c) => {
                let mut core = Compressor::new(
                    PeakDetector::new(sample_rate, 2.0),
                    HardKnee {
                        threshold_db: c.threshold_db,
                        ratio: c.ratio,
                    },
                    AttackRelease::new(sample_rate, c.attack_ms, c.release_ms),
                    Transparent,
                );
                core.makeup_db = c.makeup_db;
                Self::Generic(core)
            }
            Model::La2aGray(c) => {
                let mut core = crate::la2a::La2a::new(sample_rate);
                core.set_peak_reduction(c.peak_reduction);
                core.set_gain(c.gain);
                Self::La2a(core)
            }
        }
    }
    fn process(&mut self, input: f64, detector: f64) -> f64 {
        match self {
            Self::Generic(c) => c.process_with_sidechain(input, detector),
            Self::La2a(c) => c.process_with_sidechain(input, detector),
        }
    }
    fn reset(&mut self) {
        match self {
            Self::Generic(c) => c.reset(),
            Self::La2a(c) => c.reset(),
        }
    }
    const fn reduction(&self) -> f64 {
        match self {
            Self::Generic(c) => c.gain_reduction_db(),
            Self::La2a(c) => c.gain_reduction_db(),
        }
    }
}
/// Owns two independent channel histories. Construct/activate a new processor
/// for model changes; hosts choose the crossfade between different algorithms.
pub struct CompressorProcessor {
    channels: [ChannelModel; 2],
    config: CompressorConfig,
    spec: ProcessSpec,
}
impl CompressorProcessor {
    #[must_use]
    pub fn new(prepared: &PreparedCompressor) -> Self {
        Self {
            channels: core::array::from_fn(|_| prepared.channel.clone()),
            config: prepared.config,
            spec: prepared.spec,
        }
    }
    /// Install prepared controls for the current model, preserving detector and
    /// envelope history. Controls step at this boundary; hosts can ramp controls
    /// or crossfade processors when a smooth parameter transition is required.
    ///
    /// # Errors
    /// A different model or processing specification requires a new processor.
    pub fn apply(&mut self, prepared: &PreparedCompressor) -> Result<(), Error> {
        if self.spec != prepared.spec
            || core::mem::discriminant(&self.config.model)
                != core::mem::discriminant(&prepared.config.model)
        {
            return Err(Error::IncompatiblePreparation);
        }
        for channel in &mut self.channels {
            match (channel, &prepared.channel) {
                (ChannelModel::Generic(current), ChannelModel::Generic(next)) => {
                    current.gain_computer = next.gain_computer;
                    current.makeup_db = next.makeup_db;
                    current.envelope.apply_coefficients(&next.envelope);
                }
                (ChannelModel::La2a(current), ChannelModel::La2a(next)) => {
                    current.apply_controls(next);
                }
                _ => {} // Excluded by the discriminant check above.
            }
        }
        self.config = prepared.config;
        Ok(())
    }
    pub fn reset(&mut self) {
        for channel in &mut self.channels {
            channel.reset();
        }
    }
    pub fn gain_reduction_db(&self) -> [f64; 2] {
        self.channels.each_ref().map(ChannelModel::reduction)
    }
    pub fn process_frame(&mut self, input: [f64; 2]) -> [f64; 2] {
        self.process_frame_with_sidechain(input, input)
    }
    pub fn process_frame_with_sidechain(
        &mut self,
        input: [f64; 2],
        sidechain: [f64; 2],
    ) -> [f64; 2] {
        let peak = sidechain[0].abs().max(sidechain[1].abs());
        let mut output = input;
        for ((sample, detector_sample), channel) in
            output.iter_mut().zip(sidechain).zip(&mut self.channels)
        {
            let detector = self
                .config
                .stereo_link
                .mul_add(peak - detector_sample.abs(), detector_sample.abs());
            let wet = channel.process(*sample, detector);
            *sample = self.config.mix.mul_add(wet - *sample, *sample);
        }
        output
    }

    ///
    /// # Errors
    /// Returns an error if configuration or buffer dimensions are invalid.
    pub fn process_stereo(&mut self, left: &mut [f64], right: &mut [f64]) -> Result<(), Error> {
        if left.len() != right.len() {
            return Err(Error::ChannelLengthMismatch);
        }
        if left.len() > self.spec.max_block_size {
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
        if audio.len() > self.spec.max_block_size {
            return Err(Error::BlockTooLarge);
        }
        for sample in audio {
            *sample = self.process_frame([*sample, *sample])[0];
        }
        Ok(())
    }
}
