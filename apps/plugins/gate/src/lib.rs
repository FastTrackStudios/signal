//! FTS Gate — CLAP/VST3 classic noise gate plugin.
//!
//! A thin nice-plug shell over the `level` engine's standalone gate stage
//! ([`level::Gate`]: peak detector → threshold-with-hysteresis →
//! attack/hold/release → range floor). The gate stage is cleanly separable
//! from the vocal chain, so this crate reuses it via the `level` facade
//! rather than duplicating the DSP.
//!
//! Detection is **stereo-linked**: a single gate instance is keyed on the
//! per-frame maximum of the channel magnitudes, and the one resulting gain
//! is applied to every channel — the image never wanders when only one side
//! crosses the threshold.
//!
//! With Range below full scale the gate behaves as a downward expander: the
//! closed gain falls only to `-range` dB instead of silence.
//!
//! GUI is deliberately absent for now (headless, host-generic params),
//! matching `level-plugin`; the nice-plug-dioxus editor is a follow-up.

use nice_plug::prelude::*;
use std::sync::Arc;

use level::{Gate, GateConfig};

const PLUGIN_NAME: &str = "FTS Gate";

// ── Parameters ────────────────────────────────────────────────────────────

#[derive(Params)]
pub struct GateParams {
    /// Open threshold, dBFS.
    #[id = "threshold"]
    pub threshold_db: FloatParam,
    /// Open (attack) time, ms.
    #[id = "attack"]
    pub attack_ms: FloatParam,
    /// Minimum hold-open time once opened, ms.
    #[id = "hold"]
    pub hold_ms: FloatParam,
    /// Close (release) time, ms.
    #[id = "release"]
    pub release_ms: FloatParam,
    /// Maximum attenuation, dB (90 = full gate, less = downward expander).
    #[id = "range"]
    pub range_db: FloatParam,
    /// Hysteresis below the open threshold at which the gate closes, dB.
    #[id = "hysteresis"]
    pub hysteresis_db: FloatParam,
}

impl Default for GateParams {
    fn default() -> Self {
        Self {
            threshold_db: FloatParam::new(
                "Threshold",
                -40.0,
                FloatRange::Linear { min: -80.0, max: 0.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            attack_ms: FloatParam::new(
                "Attack",
                0.5,
                FloatRange::Skewed {
                    min: 0.01,
                    max: 50.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            hold_ms: FloatParam::new(
                "Hold",
                10.0,
                FloatRange::Linear { min: 0.0, max: 500.0 },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            release_ms: FloatParam::new(
                "Release",
                100.0,
                FloatRange::Skewed {
                    min: 5.0,
                    max: 2000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            range_db: FloatParam::new(
                "Range",
                90.0,
                FloatRange::Linear { min: 0.0, max: 90.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            hysteresis_db: FloatParam::new(
                "Hysteresis",
                4.0,
                FloatRange::Linear { min: 0.0, max: 12.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsGate {
    params: Arc<GateParams>,
    /// One gate for the whole frame — detection is stereo-linked (max of
    /// channel magnitudes keys the detector; one gain feeds every channel).
    gate: Option<Gate>,
    sample_rate: f64,
}

impl Default for FtsGate {
    fn default() -> Self {
        Self {
            params: Arc::new(GateParams::default()),
            gate: None,
            sample_rate: 48_000.0,
        }
    }
}

impl FtsGate {
    fn current_config(&self) -> GateConfig {
        GateConfig {
            threshold_db: self.params.threshold_db.value() as f64,
            hysteresis_db: self.params.hysteresis_db.value() as f64,
            attack_ms: self.params.attack_ms.value() as f64,
            hold_ms: self.params.hold_ms.value() as f64,
            release_ms: self.params.release_ms.value() as f64,
            // UI exposes attenuation as a positive amount; the DSP floor is
            // negative dB.
            range_db: -(self.params.range_db.value() as f64),
        }
    }

    /// Push the current params into the gate (no allocation).
    fn sync_params(&mut self) {
        let cfg = self.current_config();
        if let Some(g) = &mut self.gate {
            g.set_config(cfg);
        }
    }
}

impl Plugin for FtsGate {
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

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        let cfg = self.current_config();
        self.gate = Some(Gate::new(self.sample_rate, cfg));
        true
    }

    fn reset(&mut self) {
        if let Some(g) = &mut self.gate {
            g.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync_params();
        let Some(gate) = &mut self.gate else {
            return ProcessStatus::Normal;
        };

        for mut frame in buffer.iter_samples() {
            // Linked detection: key on the loudest channel of the frame.
            let mut key = 0.0f32;
            for sample in frame.iter_mut() {
                key = key.max(sample.abs());
            }
            // Advance detector + envelope once per frame, then apply the one
            // resulting gain to every channel.
            let _ = gate.process_sample_keyed(0.0, key as f64);
            let gain = gate.gain() as f32;
            for sample in frame.iter_mut() {
                *sample *= gain;
            }
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsGate {
    const CLAP_ID: &'static str = "com.fasttrackstudio.gate";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Classic noise gate: threshold/hysteresis, attack, hold, release, range");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Gate,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsGate {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsGatePlugin001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nice_export_clap!(FtsGate);
nice_export_vst3!(FtsGate);

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    fn test_cfg() -> GateConfig {
        GateConfig {
            threshold_db: -40.0,
            hysteresis_db: 4.0,
            attack_ms: 0.5,
            hold_ms: 10.0,
            release_ms: 100.0,
            range_db: -90.0,
        }
    }

    /// Sub-threshold input stays attenuated at (near) the range floor.
    #[test]
    fn silence_stays_gated() {
        let mut g = Gate::new(SR, test_cfg());
        // -60 dBFS tone, well under the -40 dB threshold.
        let amp = 10f64.powf(-60.0 / 20.0);
        let mut max_out = 0.0f64;
        for n in 0..48_000 {
            let x = amp * (2.0 * core::f64::consts::PI * 440.0 * n as f64 / SR).sin();
            max_out = max_out.max(g.process_sample(x).abs());
        }
        // Fully closed gain is -90 dB; output must stay far below the input.
        assert!(
            max_out < amp * 0.01,
            "gated output leaked: {max_out} vs input {amp}"
        );
    }

    /// Loud input opens the gate to (near) unity within a few ms.
    #[test]
    fn loud_signal_opens() {
        let mut g = Gate::new(SR, test_cfg());
        // -6 dBFS tone, far over threshold.
        let amp = 10f64.powf(-6.0 / 20.0);
        for n in 0..4800 {
            let x = amp * (2.0 * core::f64::consts::PI * 440.0 * n as f64 / SR).sin();
            g.process_sample(x);
        }
        assert!(
            g.gain() > 0.95,
            "gate failed to open: gain = {}",
            g.gain()
        );
    }

    /// After the signal stops, hold elapses and the gain decays toward the
    /// range floor at the release rate.
    #[test]
    fn release_decays_after_hold() {
        let mut g = Gate::new(SR, test_cfg());
        let amp = 10f64.powf(-6.0 / 20.0);
        for n in 0..4800 {
            let x = amp * (2.0 * core::f64::consts::PI * 440.0 * n as f64 / SR).sin();
            g.process_sample(x);
        }
        assert!(g.gain() > 0.95);
        // Feed silence: hold (10 ms) + several release constants (100 ms).
        for _ in 0..48_000 {
            g.process_sample(0.0);
        }
        assert!(
            g.gain() < 0.01,
            "gain failed to release: {}",
            g.gain()
        );
    }
}
