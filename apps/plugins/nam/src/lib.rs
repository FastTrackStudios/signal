//! FTS NAM — CLAP/VST3 Neural Amp Modeler plugin.
//!
//! A thin nice-plug shell over [`neural_amp_modeler::NamModel`]: input gain →
//! neural amp inference → output gain, the classic NAM plugin shape.
//!
//! **Model loading (stopgap)**: a headless plugin has no file browser, so the
//! model path comes from the `FTS_NAM_MODEL` environment variable, falling
//! back to `$HOME/.local/share/fts/nam/default.nam`. Loading happens in
//! `initialize()` (not the audio thread). Real model management — browsing the
//! signal-nam catalog, host state chunks, per-preset models — arrives with the
//! GUI + signal-rig integration. If no model loads, `process()` passes audio
//! through dry (unity, gains not applied) so the insert is transparent.
//!
//! **Channel strategy**: NAM models are mono and [`NamModel`] is a single
//! opaque inference instance (not `Clone` — it wraps the vendored C++ core).
//! Rather than paying for one model per channel, the plugin sums stereo input
//! to mono, runs one inference pass, and writes the result to both outputs —
//! matching how a guitar amp sim is actually used (mono source on a stereo
//! track). Scratch buffers are preallocated to the host's max buffer size in
//! `initialize()`; `process()` never allocates.
//!
//! GUI is deliberately absent for now (headless, host-generic params),
//! matching `signal-sampler-clap`; the nice-plug-dioxus editor is a follow-up.

use nice_plug::prelude::*;
use std::sync::Arc;

use neural_amp_modeler::NamModel;

const PLUGIN_NAME: &str = "FTS NAM";

/// Environment variable naming the `.nam` model file to load (stopgap until
/// real model management lands with the GUI).
const MODEL_ENV_VAR: &str = "FTS_NAM_MODEL";

/// Fallback model path (relative to `$HOME`) when the env var is unset.
const DEFAULT_MODEL_REL: &str = ".local/share/fts/nam/default.nam";

// ── Parameters ────────────────────────────────────────────────────────────

#[derive(Params)]
pub struct NamParams {
    /// Gain applied before the model (drives the capture harder/softer).
    #[id = "input_db"]
    pub input_db: FloatParam,
    /// Gain applied after the model (level matching).
    #[id = "output_db"]
    pub output_db: FloatParam,
}

impl Default for NamParams {
    fn default() -> Self {
        Self {
            input_db: FloatParam::new(
                "Input Gain",
                0.0,
                FloatRange::Linear { min: -24.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            output_db: FloatParam::new(
                "Output Gain",
                0.0,
                FloatRange::Linear { min: -24.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsNam {
    params: Arc<NamParams>,
    /// The loaded amp model, or `None` → dry passthrough.
    model: Option<NamModel>,
    /// Mono input scratch (f64, model domain), preallocated in `initialize()`.
    scratch_in: Vec<f64>,
    /// Mono output scratch (f64, model domain), preallocated in `initialize()`.
    scratch_out: Vec<f64>,
    sample_rate: f64,
}

impl Default for FtsNam {
    fn default() -> Self {
        Self {
            params: Arc::new(NamParams::default()),
            model: None,
            scratch_in: Vec::new(),
            scratch_out: Vec::new(),
            sample_rate: 48_000.0,
        }
    }
}

impl FtsNam {
    /// Resolve the stopgap model path: `FTS_NAM_MODEL`, else the default
    /// location under `$HOME`. Returns `None` when neither yields a path.
    fn model_path() -> Option<std::path::PathBuf> {
        if let Ok(p) = std::env::var(MODEL_ENV_VAR) {
            if !p.is_empty() {
                return Some(std::path::PathBuf::from(p));
            }
        }
        std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(DEFAULT_MODEL_REL))
    }
}

impl Plugin for FtsNam {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Audio effect: stereo in, stereo out (mono model on the mono sum).
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
        let max_block = buffer_config.max_buffer_size as usize;

        // Preallocate the mono scratch buffers — process() never allocates.
        self.scratch_in = vec![0.0; max_block];
        self.scratch_out = vec![0.0; max_block];

        // Stopgap model load (see module docs): env var / default path, on the
        // main thread. Failure is non-fatal — the plugin becomes a dry insert.
        self.model = match Self::model_path() {
            Some(path) => match NamModel::load(&path) {
                Ok(mut model) => {
                    model.reset(self.sample_rate, max_block);
                    Some(model)
                }
                Err(e) => {
                    eprintln!("[{PLUGIN_NAME}] failed to load NAM model {path:?}: {e} — passing audio through dry");
                    None
                }
            },
            None => {
                eprintln!(
                    "[{PLUGIN_NAME}] no model path ({MODEL_ENV_VAR} unset, no $HOME) — passing audio through dry"
                );
                None
            }
        };

        true
    }

    fn reset(&mut self) {
        // Re-prewarm the model's internal state (anti-pop / receptive field).
        if let Some(model) = &mut self.model {
            model.reset(self.sample_rate, self.scratch_in.len().max(1));
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let n = buffer.samples();
        let Some(model) = &mut self.model else {
            // No model loaded → dry passthrough (documented stopgap behavior).
            return ProcessStatus::Normal;
        };
        if n == 0 || n > self.scratch_in.len() {
            // Host exceeded its declared max buffer size — never allocate on
            // the audio thread; pass through instead.
            return ProcessStatus::Normal;
        }

        let channels = buffer.as_slice();
        let num_ch = channels.len().max(1);

        // Mono sum (mean) with smoothed input gain, into the f64 scratch.
        let inv_ch = 1.0 / num_ch as f64;
        for (i, slot) in self.scratch_in[..n].iter_mut().enumerate() {
            let gain = self.params.input_db.smoothed.next();
            let mut sum = 0.0f64;
            for ch in channels.iter() {
                sum += ch[i] as f64;
            }
            *slot = sum * inv_ch * util::db_to_gain(gain) as f64;
        }

        // One inference pass over the mono block.
        model.process(&self.scratch_in[..n], &mut self.scratch_out[..n]);

        // Smoothed output gain, duplicated to every output channel.
        for i in 0..n {
            let gain = util::db_to_gain(self.params.output_db.smoothed.next());
            let out = (self.scratch_out[i] as f32) * gain;
            for ch in channels.iter_mut() {
                ch[i] = out;
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsNam {
    const CLAP_ID: &'static str = "com.fasttrackstudio.nam";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Neural Amp Modeler: .nam capture playback with input/output gain");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Distortion,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsNam {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsNamPlugin0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nice_export_clap!(FtsNam);
nice_export_vst3!(FtsNam);
