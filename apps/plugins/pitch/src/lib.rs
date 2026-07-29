//! FTS Pitch — CLAP/VST3 pitch shifter + stereo doubler.
//!
//! One plugin, two modes over the `pitch-dsp` engines (PSOLA with the
//! fixed period-recentering, WSOLA, granular):
//!
//! - **Shift**: classic stereo pitch shift, semitones + fine cents.
//! - **Doubler**: the studio double-track — dry stays center, two
//!   detuned voices (±detune cents) with a short haas offset pan hard
//!   left/right. Detune and delay are per-side mirrored, so the image
//!   stays centered while widening.

use nice_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::Arc;

use audiocore_dsp::{AudioConfig, Processor};
use pitch_dsp::chain::{Algorithm, PitchChain};

const PLUGIN_NAME: &str = "FTS Pitch";
/// Max doubler voice delay (ms) — sized into the ring at prepare.
const MAX_DELAY_MS: f64 = 60.0;

#[derive(Params)]
pub struct PitchParams {
    /// 0 = Shift, 1 = Doubler.
    #[id = "mode"]
    pub mode: IntParam,
    /// Coarse shift in semitones (Shift mode).
    #[id = "semitones"]
    pub semitones: IntParam,
    /// Fine shift in cents (Shift mode; also the doubler detune base).
    #[id = "cents"]
    pub cents: FloatParam,
    /// Doubler voice detune (± cents around unison).
    #[id = "detune"]
    pub detune: FloatParam,
    /// Doubler voice delay (haas offset, ms; L gets it, R gets 1.5x).
    #[id = "delay"]
    pub delay_ms: FloatParam,
    /// Shifting algorithm: 0 PSOLA (voice), 1 WSOLA (poly), 2 Granular.
    #[id = "algo"]
    pub algo: IntParam,
    /// Wet mix (Shift: shifted vs dry; Doubler: voices vs dry).
    #[id = "mix"]
    pub mix: FloatParam,
    /// Output trim.
    #[id = "output"]
    pub output_db: FloatParam,
}

impl Default for PitchParams {
    fn default() -> Self {
        Self {
            mode: IntParam::new("Mode", 0, IntRange::Linear { min: 0, max: 1 })
                .with_value_to_string(Arc::new(|v| {
                    match v {
                        1 => "Doubler",
                        _ => "Shift",
                    }
                    .to_string()
                })),
            semitones: IntParam::new(
                "Semitones",
                0,
                IntRange::Linear { min: -24, max: 24 },
            ),
            cents: FloatParam::new(
                "Fine",
                0.0,
                FloatRange::Linear { min: -100.0, max: 100.0 },
            )
            .with_unit(" ct")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            detune: FloatParam::new(
                "Detune",
                12.0,
                FloatRange::Linear { min: 0.0, max: 50.0 },
            )
            .with_unit(" ct")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            delay_ms: FloatParam::new(
                "Delay",
                18.0,
                FloatRange::Linear { min: 0.0, max: 40.0 },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            algo: IntParam::new("Engine", 0, IntRange::Linear { min: 0, max: 2 })
                .with_value_to_string(Arc::new(|v| {
                    match v {
                        1 => "WSOLA",
                        2 => "Granular",
                        _ => "PSOLA",
                    }
                    .to_string()
                })),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_unit("%"),
            output_db: FloatParam::new(
                "Output",
                0.0,
                FloatRange::Linear { min: -24.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
        }
    }
}

pub struct FtsPitch {
    params: Arc<PitchParams>,
    /// Voice A (Shift mode's engine; Doubler's +detune voice → left).
    chain_a: PitchChain,
    /// Voice B (Doubler's −detune voice → right).
    chain_b: PitchChain,
    /// Haas delay rings for the doubler voices.
    ring_l: Vec<f64>,
    ring_r: Vec<f64>,
    ring_pos: usize,
    scratch_al: Vec<f64>,
    scratch_ar: Vec<f64>,
    scratch_bl: Vec<f64>,
    scratch_br: Vec<f64>,
    sample_rate: f64,
}

impl Default for FtsPitch {
    fn default() -> Self {
        let mk = || {
            let mut c = PitchChain::new();
            c.algorithm = Algorithm::Psola;
            c.semitones = 0.0;
            c.mix = 1.0;
            c
        };
        Self {
            params: Arc::new(PitchParams::default()),
            chain_a: mk(),
            chain_b: mk(),
            ring_l: Vec::new(),
            ring_r: Vec::new(),
            ring_pos: 0,
            scratch_al: Vec::new(),
            scratch_ar: Vec::new(),
            scratch_bl: Vec::new(),
            scratch_br: Vec::new(),
            sample_rate: 48_000.0,
        }
    }
}

impl Plugin for FtsPitch {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        let cfg = AudioConfig {
            sample_rate: self.sample_rate,
            max_buffer_size: buffer_config.max_buffer_size as usize,
        };
        self.chain_a.update(cfg);
        self.chain_b.update(cfg);
        let ring = ((MAX_DELAY_MS / 1000.0) * self.sample_rate) as usize + 2;
        self.ring_l = vec![0.0; ring];
        self.ring_r = vec![0.0; ring];
        self.ring_pos = 0;
        let cap = buffer_config.max_buffer_size as usize;
        self.scratch_al = vec![0.0; cap];
        self.scratch_ar = vec![0.0; cap];
        self.scratch_bl = vec![0.0; cap];
        self.scratch_br = vec![0.0; cap];
        true
    }

    fn reset(&mut self) {
        self.chain_a.reset();
        self.chain_b.reset();
        self.ring_l.fill(0.0);
        self.ring_r.fill(0.0);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let n = buffer.samples();
        if n == 0 || self.scratch_al.len() < n {
            return ProcessStatus::Normal;
        }
        let mode = self.params.mode.value();
        let algo = match self.params.algo.value() {
            1 => Algorithm::Wsola,
            2 => Algorithm::Granular,
            _ => Algorithm::Psola,
        };
        let mix = self.params.mix.value() as f64;
        let out_gain = util::db_to_gain(self.params.output_db.value()) as f64;
        let shift_st =
            self.params.semitones.value() as f64 + self.params.cents.value() as f64 / 100.0;
        let detune_st = self.params.detune.value() as f64 / 100.0;

        self.chain_a.algorithm = algo;
        self.chain_b.algorithm = algo;
        self.chain_a.mix = 1.0;
        self.chain_b.mix = 1.0;

        // Deinterleave into the voice scratches.
        for (i, frame) in buffer.iter_samples().enumerate() {
            let mut it = frame.into_iter();
            let l = it.next().map(|s| *s as f64).unwrap_or(0.0);
            let r = it.next().map(|s| *s as f64).unwrap_or(l);
            self.scratch_al[i] = l;
            self.scratch_ar[i] = r;
            self.scratch_bl[i] = l;
            self.scratch_br[i] = r;
        }

        match mode {
            // ── Doubler ──────────────────────────────────────────────
            1 => {
                self.chain_a.semitones = detune_st;
                self.chain_b.semitones = -detune_st;
                self.chain_a
                    .process(&mut self.scratch_al[..n], &mut self.scratch_ar[..n]);
                self.chain_b
                    .process(&mut self.scratch_bl[..n], &mut self.scratch_br[..n]);
                let ring = self.ring_l.len();
                let d_l =
                    ((self.params.delay_ms.value() as f64 / 1000.0) * self.sample_rate) as usize;
                let d_r = (d_l * 3) / 2; // 1.5x on the right — decorrelated haas
                for (i, mut frame) in buffer.iter_samples().enumerate() {
                    // Voice A (up-detuned) delayed → left; voice B → right.
                    self.ring_l[self.ring_pos] = self.scratch_al[i];
                    self.ring_r[self.ring_pos] = self.scratch_br[i];
                    let read_l = (self.ring_pos + ring - d_l.min(ring - 1)) % ring;
                    let read_r = (self.ring_pos + ring - d_r.min(ring - 1)) % ring;
                    let voice_l = self.ring_l[read_l];
                    let voice_r = self.ring_r[read_r];
                    self.ring_pos = (self.ring_pos + 1) % ring;

                    let mut it = frame.iter_mut();
                    if let Some(s) = it.next() {
                        let dry = *s as f64;
                        *s = ((dry + voice_l * mix) * out_gain) as f32;
                    }
                    if let Some(s) = it.next() {
                        let dry = *s as f64;
                        *s = ((dry + voice_r * mix) * out_gain) as f32;
                    }
                }
            }
            // ── Shift ────────────────────────────────────────────────
            _ => {
                self.chain_a.semitones = shift_st.clamp(-24.0, 24.0);
                self.chain_a
                    .process(&mut self.scratch_al[..n], &mut self.scratch_ar[..n]);
                for (i, mut frame) in buffer.iter_samples().enumerate() {
                    let mut it = frame.iter_mut();
                    if let Some(s) = it.next() {
                        let dry = *s as f64;
                        *s = ((dry + (self.scratch_al[i] - dry) * mix) * out_gain) as f32;
                    }
                    if let Some(s) = it.next() {
                        let dry = *s as f64;
                        *s = ((dry + (self.scratch_ar[i] - dry) * mix) * out_gain) as f32;
                    }
                }
            }
        }

        context.set_latency_samples(self.chain_a.latency() as u32);
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsPitch {
    const CLAP_ID: &'static str = "com.fasttrackstudio.pitch";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Pitch shifter + stereo doubler (PSOLA/WSOLA/granular)");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::PitchShifter,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsPitch {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsPitchPlug0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::PitchShift];
}

nice_export_clap!(FtsPitch);
nice_export_vst3!(FtsPitch);
