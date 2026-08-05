//! FTS Level — CLAP/VST3 vocal-leveling plugin.
//!
//! A thin nice-plug shell over the [`level`] engine's realtime chain
//! ([`level::VocalLeveler`]: gate → de-breath → ride → de-ess). All four stages
//! run every block; each is driven by a host parameter and bypasses
//! transparently at its neutral value, so there are no enable switches.
//!
//! Detection is mono per channel (one leveler instance per channel). The offline
//! analyze→envelope path in `level` is a separate, DAW-side feature and is not
//! wired here — this crate is the *live* insert.
//!
//! GUI is deliberately absent for now (headless, host-generic params), matching
//! `signal-sampler-clap`; the nice-plug-dioxus editor is a follow-up.

use nice_plug::prelude::*;
use std::sync::Arc;

use level::{
    DeBreathConfig, DeEssConfig, GateConfig, LevelerConfig, RiderConfig, VocalLeveler,
};

const PLUGIN_NAME: &str = "FTS Level";

// ── Parameters ────────────────────────────────────────────────────────────

#[derive(Params)]
pub struct LevelParams {
    /// Level the rider drives tonal material toward.
    #[id = "ride_target"]
    pub ride_target: FloatParam,
    /// Rider correction amount (0 = off, 1 = full).
    #[id = "ride_amount"]
    pub ride_amount: FloatParam,
    /// Gate open threshold (very low = effectively bypassed).
    #[id = "gate_thresh"]
    pub gate_threshold: FloatParam,
    /// De-esser high-band threshold (0 dB = effectively bypassed).
    #[id = "deess_thresh"]
    pub deess_threshold: FloatParam,
    /// Breath attenuation (0 dB = off).
    #[id = "debreath"]
    pub debreath_db: FloatParam,
}

impl Default for LevelParams {
    fn default() -> Self {
        Self {
            ride_target: FloatParam::new(
                "Ride Target",
                -18.0,
                FloatRange::Linear { min: -40.0, max: 0.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            ride_amount: FloatParam::new(
                "Ride Amount",
                0.6,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_unit("%"),
            gate_threshold: FloatParam::new(
                "Gate",
                -80.0,
                FloatRange::Linear { min: -80.0, max: 0.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            deess_threshold: FloatParam::new(
                "De-Ess",
                0.0,
                FloatRange::Linear { min: -50.0, max: 0.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            debreath_db: FloatParam::new(
                "De-Breath",
                0.0,
                FloatRange::Linear { min: 0.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsLevel {
    params: Arc<LevelParams>,
    /// One leveler per channel (mono detection each).
    channels: Vec<VocalLeveler>,
    sample_rate: f64,
}

impl Default for FtsLevel {
    fn default() -> Self {
        Self {
            params: Arc::new(LevelParams::default()),
            channels: Vec::new(),
            sample_rate: 48_000.0,
        }
    }
}

impl FtsLevel {
    /// Push the current params into every channel's chain (no allocation).
    fn sync_params(&mut self) {
        let gate = GateConfig {
            threshold_db: self.params.gate_threshold.value() as f64,
            ..GateConfig::default()
        };
        let debreath = DeBreathConfig {
            reduction_db: self.params.debreath_db.value() as f64,
            ..DeBreathConfig::default()
        };
        let rider = RiderConfig {
            target_db: self.params.ride_target.value() as f64,
            amount: self.params.ride_amount.value() as f64,
            ..RiderConfig::default()
        };
        let deess = DeEssConfig {
            threshold_db: self.params.deess_threshold.value() as f64,
            ..DeEssConfig::default()
        };
        for ch in &mut self.channels {
            ch.set_stage_configs(gate, debreath, rider, deess);
        }
    }
}

impl Plugin for FtsLevel {
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

    // No editor yet — the host shows its generic parameter UI.
    type Editor = ();
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn activate(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        let ch = audio_io_layout
            .main_output_channels
            .map(|n| n.get() as usize)
            .unwrap_or(2)
            .max(1);
        self.channels = (0..ch)
            .map(|_| VocalLeveler::new(self.sample_rate, LevelerConfig::default()))
            .collect();
        true
    }

    fn reset(&mut self) {
        for ch in &mut self.channels {
            ch.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        if self.channels.is_empty() {
            return ProcessStatus::Normal;
        }
        self.sync_params();

        for mut frame in buffer.iter_samples() {
            for (c, sample) in frame.iter_mut().enumerate() {
                if let Some(ch) = self.channels.get_mut(c) {
                    *sample = ch.process_sample(*sample as f64) as f32;
                }
            }
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsLevel {
    const CLAP_ID: &'static str = "com.fasttrackstudio.level";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Vocal leveling: automatic riding, gate, de-ess, and de-breath");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Utility,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsLevel {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsLevelPlugn001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nice_export_clap!(FtsLevel);
nice_export_vst3!(FtsLevel);
