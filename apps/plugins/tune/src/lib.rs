//! FTS Tune — CLAP/VST3 monophonic pitch-correction plugin.
//!
//! A nice-plug shell that wires the [`tune`] engine's realtime path:
//!
//! ```text
//! block mono ─▶ YIN detect ─▶ nearest scale degree ─▶ shift Δ (× strength)
//!                                                        │
//!            stereo in ─▶ pitch-dsp PitchChain (formant-preserving) ─▶ out
//! ```
//!
//! Detection runs once per block over a sliding window; the resulting
//! semitone offset is slewed (the "retune speed" knob) and fed to the shared
//! `pitch-dsp` chain, so the actual formant-aware shifting is not reinvented
//! here. This is the *monophonic* foundation — polyphonic detection and a
//! note-graph editing UI are the larger follow-ups that make `tune` a full
//! Melodyne competitor.
//!
//! GUI is deliberately absent for now (headless, host-generic params), matching
//! `signal-sampler-clap`.

use nice_plug::prelude::*;
use std::sync::Arc;

use audiocore_dsp::{AudioConfig, Processor};
use tune::shifter::chain::{Algorithm, PitchChain};
use tune::{hz_to_midi, Scale, YinConfig, YinDetector};

const PLUGIN_NAME: &str = "FTS Tune";

// ── Parameters ────────────────────────────────────────────────────────────

#[derive(Params)]
pub struct TuneParams {
    /// Musical key root (0 = C … 11 = B).
    #[id = "key"]
    pub key: IntParam,
    /// Scale: 0 = Chromatic, 1 = Major, 2 = Minor.
    #[id = "scale"]
    pub scale: IntParam,
    /// Correction strength (0 = off, 1 = fully snapped).
    #[id = "strength"]
    pub strength: FloatParam,
    /// Retune speed — smoothing of the applied shift, ms (low = hard tune).
    #[id = "retune_ms"]
    pub retune_ms: FloatParam,
    /// Dry/wet mix.
    #[id = "mix"]
    pub mix: FloatParam,
}

impl Default for TuneParams {
    fn default() -> Self {
        Self {
            key: IntParam::new("Key", 0, IntRange::Linear { min: 0, max: 11 })
                .with_value_to_string(Arc::new(|v| {
                    const N: [&str; 12] = [
                        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                    ];
                    N[(v as usize) % 12].to_string()
                })),
            scale: IntParam::new("Scale", 0, IntRange::Linear { min: 0, max: 2 })
                .with_value_to_string(Arc::new(|v| match v {
                    1 => "Major".to_string(),
                    2 => "Minor".to_string(),
                    _ => "Chromatic".to_string(),
                })),
            strength: FloatParam::new(
                "Strength",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0)),
            retune_ms: FloatParam::new(
                "Retune",
                20.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 400.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsTune {
    params: Arc<TuneParams>,
    detector: YinDetector,
    chain: PitchChain,
    sample_rate: f64,
    /// Sliding mono history for detection (length == YIN window).
    hist: Vec<f64>,
    /// f64 scratch for the two channels (sized in `initialize`).
    left: Vec<f64>,
    right: Vec<f64>,
    /// Smoothed applied shift, semitones.
    shift_semitones: f64,
}

impl Default for FtsTune {
    fn default() -> Self {
        let yin = YinConfig::default();
        let detector = YinDetector::new(48_000.0, yin);
        let hist = vec![0.0; detector.window()];
        Self {
            params: Arc::new(TuneParams::default()),
            detector,
            chain: PitchChain::new(),
            sample_rate: 48_000.0,
            hist,
            left: Vec::new(),
            right: Vec::new(),
            shift_semitones: 0.0,
        }
    }
}

impl FtsTune {
    fn current_scale(&self) -> Scale {
        let root = self.params.key.value() as u8;
        match self.params.scale.value() {
            1 => Scale::major(root),
            2 => Scale::minor(root),
            _ => Scale::CHROMATIC,
        }
    }

    /// Detect f0 over the current history and update the target shift.
    fn update_shift(&mut self, block_len: usize) {
        let frame = self.detector.detect(&self.hist);
        let target_shift = match frame.f0_hz {
            Some(hz) => {
                let detected = hz_to_midi(hz);
                let snapped = self.current_scale().snap(detected);
                (snapped - detected) * self.params.strength.value() as f64
            }
            None => 0.0, // unvoiced: relax toward no shift
        };
        // One-pole slew toward the target (retune speed), settled by
        // the block's REAL duration so the retune time is
        // buffer-size-independent (the old per-sample coefficient
        // applied once per block made retune ~8x faster at 64-sample
        // buffers than at 512).
        let t = (self.params.retune_ms.value() as f64 / 1000.0).max(1e-4);
        let coeff = (-(block_len as f64) / (t * self.sample_rate)).exp();
        self.shift_semitones = target_shift + coeff * (self.shift_semitones - target_shift);
    }
}

impl Plugin for FtsTune {
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

    // No editor yet — the host shows its generic parameter UI.
    type Editor = ();
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        self.detector = YinDetector::new(self.sample_rate, YinConfig::default());
        self.hist = vec![0.0; self.detector.window()];
        let max = (buffer_config.max_buffer_size as usize).max(1);
        self.left = vec![0.0; max];
        self.right = vec![0.0; max];
        self.chain.formant_linked = true; // preserve formants while retuning
        self.chain.algorithm = Algorithm::Psola;
        self.chain.update(AudioConfig {
            sample_rate: self.sample_rate,
            max_buffer_size: max,
        });
        true
    }

    fn reset(&mut self) {
        self.chain.reset();
        self.shift_semitones = 0.0;
        for s in &mut self.hist {
            *s = 0.0;
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let frames = buffer.samples();
        if frames == 0 {
            return ProcessStatus::Normal;
        }
        let channels = buffer.as_slice();
        if channels.is_empty() {
            return ProcessStatus::Normal;
        }

        // Copy channels into f64 scratch; build the block's mono for detection.
        let ch0 = &channels[0];
        for i in 0..frames {
            let l = ch0[i] as f64;
            let r = channels.get(1).map(|c| c[i] as f64).unwrap_or(l);
            self.left[i] = l;
            self.right[i] = r;
        }

        // Slide the detection history: drop `frames`, append this block's mono.
        let w = self.hist.len();
        if frames >= w {
            for i in 0..w {
                let src = frames - w + i;
                self.hist[i] = 0.5 * (self.left[src] + self.right[src]);
            }
        } else {
            self.hist.copy_within(frames..w, 0);
            let base = w - frames;
            for i in 0..frames {
                self.hist[base + i] = 0.5 * (self.left[i] + self.right[i]);
            }
        }

        // Detect + update the shift once per block, then drive the chain.
        self.update_shift(frames);
        self.chain.semitones = self.shift_semitones;
        self.chain.mix = self.params.mix.value() as f64;
        context.set_latency_samples(self.chain.latency() as u32);

        self.chain
            .process(&mut self.left[..frames], &mut self.right[..frames]);

        // Write back.
        let out = buffer.as_slice();
        for i in 0..frames {
            out[0][i] = self.left[i] as f32;
            if out.len() > 1 {
                out[1][i] = self.right[i] as f32;
            }
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsTune {
    const CLAP_ID: &'static str = "com.fasttrackstudio.tune";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Monophonic pitch correction — YIN detection, scale snapping, formant-preserving shift");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::PitchShifter,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsTune {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsTunePluginv01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::PitchShift];
}

nice_export_clap!(FtsTune);
nice_export_vst3!(FtsTune);
