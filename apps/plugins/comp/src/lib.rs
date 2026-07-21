//! FTS Comp — CLAP/VST3 compressor plugin.
//!
//! A nice-plug shell over the [`comp`] engine's stereo chain
//! ([`comp::CompChain`]: linked detector → gain curve → hermite-smoothed gain
//! reduction → makeup → parallel mix). The classic parameter set only —
//! threshold / ratio / attack / release / knee / makeup / mix, plus the
//! chain's stereo-link amount. The engine's extended surface (styles,
//! character/drive, expander, upward comp, sidechain EQ, lookahead,
//! multiband) is deliberately not exposed here.
//!
//! Detection is stereo linked by default: `CompChain` feeds both channels a
//! max-linked key (blended by `channel_link`), while gain smoothing and
//! metering stay per channel inside the shared `ProC3Compressor` core.
//!
//! Params + shared UI state live in [`comp_ui::params`] (like `eq-ui`), so
//! the Dioxus editor ([`comp_ui::control_view::App`]) renders against them
//! without a circular dep.

use audiocore_core::prelude::*;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use comp::CompChain;
use comp_ui::params::{CompParams, CompUiState};

const PLUGIN_NAME: &str = "FTS Comp";

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsComp {
    params: Arc<CompParams>,
    ui_state: Arc<CompUiState>,
    editor_state: Arc<DioxusState>,
    /// One stereo chain: linked detection, per-channel gain state inside.
    chain: CompChain,
    sample_rate: f64,
}

impl Default for FtsComp {
    fn default() -> Self {
        let params = Arc::new(CompParams::default());
        let ui_state = Arc::new(CompUiState::new(params.clone()));
        Self {
            params,
            ui_state,
            // Tall enough for the 300 px graph + knob rows + meters.
            editor_state: DioxusState::new(|| (800, 640)),
            chain: CompChain::new(),
            sample_rate: 48_000.0,
        }
    }
}

impl FtsComp {
    /// Push the current params into the chain (no allocation).
    ///
    /// The core setters propagate into the gain curve and only reset the
    /// hermite smoother when a value actually changed, so this is safe to
    /// call once per block.
    fn sync_params(&mut self) {
        let c = &mut self.chain.comp;
        c.set_threshold(self.params.threshold_db.value() as f64);
        c.set_ratio(self.params.ratio.value() as f64);
        c.set_attack_ms(self.params.attack_ms.value() as f64);
        c.set_release_ms(self.params.release_ms.value() as f64);
        c.set_knee(self.params.knee_db.value() as f64);
        c.set_fold(self.params.mix.value() as f64);
        c.output_gain_db = self.params.makeup_db.value() as f64;
        c.channel_link = self.params.stereo_link.value() as f64;
        c.update(self.sample_rate);
    }
}

impl Plugin for FtsComp {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Audio effect: stereo in, stereo out.
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

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            self.ui_state.clone(),
            comp_ui::control_view::App,
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        self.ui_state
            .sample_rate
            .store(buffer_config.sample_rate, Ordering::Relaxed);
        self.chain.update_sample_rate(self.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.chain.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync_params();

        let mut input_peak: f32 = 0.0;
        let mut output_peak: f32 = 0.0;
        for mut frame in buffer.iter_samples() {
            let mut it = frame.iter_mut();
            let (Some(l), r) = (it.next(), it.next()) else {
                continue;
            };
            let mut left = *l as f64;
            // Mono buses: feed the single channel to both sides of the chain
            // (the linked detector then behaves as plain mono detection).
            let mut right = r.as_ref().map(|s| **s as f64).unwrap_or(left);
            input_peak = input_peak.max(left.abs().max(right.abs()) as f32);
            self.chain.process_sample(&mut left, &mut right);
            output_peak = output_peak.max(left.abs().max(right.abs()) as f32);
            *l = left as f32;
            if let Some(r) = r {
                *r = right as f32;
            }
        }

        // ── UI metering (lock-free atomics; ~0.3 dB/block decay) ────────
        let gr_db = self.chain.comp.gain_reduction_db() as f32;
        self.ui_state.gain_reduction_db.store(gr_db, Ordering::Relaxed);

        // Graph history rings: one input peak + one GR value per block —
        // lock-free stores, no allocation on the audio thread.
        self.ui_state.input_wave.push(input_peak);
        self.ui_state.gr_wave.push(gr_db);

        let prev_in = self.ui_state.input_peak_db.load(Ordering::Relaxed);
        let in_db = if input_peak > 0.0 {
            20.0 * input_peak.log10()
        } else {
            -100.0
        };
        self.ui_state.input_peak_db.store(
            if in_db > prev_in { in_db } else { prev_in - 0.3 },
            Ordering::Relaxed,
        );

        let prev_out = self.ui_state.output_peak_db.load(Ordering::Relaxed);
        let out_db = if output_peak > 0.0 {
            20.0 * output_peak.log10()
        } else {
            -100.0
        };
        self.ui_state.output_peak_db.store(
            if out_db > prev_out { out_db } else { prev_out - 0.3 },
            Ordering::Relaxed,
        );

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsComp {
    const CLAP_ID: &'static str = "com.fasttrackstudio.comp";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Compressor: threshold, ratio, attack, release, knee, makeup, and parallel mix");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Compressor,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsComp {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsCompPlugin001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nice_export_clap!(FtsComp);
nice_export_vst3!(FtsComp);
