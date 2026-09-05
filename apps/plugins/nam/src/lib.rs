//! FTS NAM — CLAP/VST3 Neural Amp Modeler plugin.
//!
//! A thin nice-plug shell over [`neural_amp_modeler::NamModel`]: input gain →
//! neural amp inference → output gain, the classic NAM plugin shape.
//!
//! **Model loading**: the editor is a TONE3000 tone browser — search the
//! catalog, download a capture, play it, without leaving the insert. The
//! chosen model reaches the audio thread through [`swap`], which loads and
//! frees on a loader thread so `process()` does neither. `FTS_NAM_MODEL` (or
//! `$HOME/.local/share/fts/nam/default.nam`) still seeds the model at
//! `activate()`, so a headless host or an automated render behaves exactly as
//! before. If no model loads, `process()` passes audio through dry (unity,
//! gains not applied) so the insert is transparent.
//!
//! **Channel strategy**: NAM models are mono and [`NamModel`] is a single
//! opaque inference instance (not `Clone` — it wraps the vendored C++ core).
//! Rather than paying for one model per channel, the plugin sums stereo input
//! to mono, runs one inference pass, and writes the result to both outputs —
//! matching how a guitar amp sim is actually used (mono source on a stereo
//! track). Scratch buffers are preallocated to the host's max buffer size in
//! `initialize()`; `process()` never allocates.
//!
use audiocore_core::prelude::{create_dioxus_editor_with_state, DioxusState};
use nice_plug::prelude::*;
use std::sync::Arc;

use neural_amp_modeler::NamModel;

pub mod engine;
pub mod state;
pub mod swap;
pub mod ui;

use state::NamUi;

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
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            output_db: FloatParam::new(
                "Output Gain",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
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
    /// What the editor shows and asks for.
    ui: Arc<NamUi>,
    /// Editor window state (size, open flag).
    editor_state: Arc<DioxusState>,
    /// The audio end of the model handoff — `None` until `activate()`, which
    /// is when the sample rate and block size the loader must prime with are
    /// finally known.
    swap: Option<swap::AudioEnd>,
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
            ui: Arc::new(NamUi::new()),
            editor_state: DioxusState::new(|| ui::SIZE),
            swap: None,
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

    // `Plugin::Editor` is an associated type as of the baseview 0.3 rework,
    // so the editor is named here rather than returned as a trait object.
    type Editor = audiocore_core::nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_dioxus_editor_with_state(self.editor_state.clone(), self.ui.clone(), ui::App)
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = f64::from(buffer_config.sample_rate);
        let max_block = buffer_config.max_buffer_size as usize;

        // Preallocate the mono scratch buffers — process() never allocates.
        self.scratch_in = vec![0.0; max_block];
        self.scratch_out = vec![0.0; max_block];

        // The handoff has to know the block shape it is priming models for,
        // which is only settled here.
        let (audio, editor) = swap::start(self.sample_rate, max_block);
        self.swap = Some(audio);
        self.ui.attach(editor);

        // A model already chosen in the editor (or restored with the
        // session) takes precedence over the environment seed below.
        if self.ui.loaded_path().is_some() {
            // `attach` above re-requested it; the loader is already reading
            // it, and `process` will pick it up. Nothing to load here.
            self.model = None;
            return true;
        }

        // Seed from the environment (see module docs), on the main thread.
        // Failure is non-fatal — the plugin becomes a dry insert.
        self.model = if let Some(path) = Self::model_path() { match NamModel::load(&path) {
            Ok(mut model) => {
                model.reset(self.sample_rate, max_block);
                Some(model)
            }
            Err(e) => {
                eprintln!("[{PLUGIN_NAME}] failed to load NAM model {path:?}: {e} — passing audio through dry");
                None
            }
        } } else {
            eprintln!(
                "[{PLUGIN_NAME}] no model path ({MODEL_ENV_VAR} unset, no $HOME) — passing audio through dry"
            );
            None
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
        // Take a newly loaded model if the loader has one ready. Non-blocking,
        // no allocation, no free — see `swap`.
        if let Some(swap) = &mut self.swap {
            self.model = swap.take(self.model.take());
        }

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
                sum += f64::from(ch[i]);
            }
            *slot = sum * inv_ch * f64::from(util::db_to_gain(gain));
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
