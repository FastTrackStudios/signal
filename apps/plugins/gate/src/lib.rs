//! FTS Gate — CLAP/VST3 noise gate / drum gate plugin.
//!
//! Two styles behind one plugin:
//!
//! - **Classic** (the default, and what older sessions load as): a thin
//!   shell over the `level` engine's standalone gate stage — described
//!   below.
//! - **Drum**: the [`gate::DrumGate`] engine — source modes (kick, snare
//!   top/bottom, toms) with look-ahead and a voiced detector, Length-shaped
//!   release, Ghost, and the HF de-bleed. It can be keyed from the stereo
//!   sidechain input. Reports its look-ahead as latency.
//!
//! Classic: a thin nice-plug shell over the `level` engine's standalone gate stage
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

use gate::{DrumGate, Mode, Settings as DrumSettings};
use level::{Gate, GateConfig};

const PLUGIN_NAME: &str = "FTS Gate";

// ── Parameters ────────────────────────────────────────────────────────────

#[derive(Params)]
pub struct GateParams {
    /// What the detector listens to: the track itself, or the sidechain
    /// input — a drum's recorded trigger, so the close mic opens on the
    /// hit and not on the cymbals bleeding into it.
    #[id = "key"]
    pub key: EnumParam<Key>,
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
    /// Classic gate or Drum gate.
    #[id = "style"]
    pub style: IntParam,
    /// The Drum style's controls (IDs unprefixed, stable).
    #[nested(group = "Drum")]
    pub drum: DrumParams,
}

/// The Drum style's controls. Threshold and Range are shared with Classic.
#[derive(Params)]
pub struct DrumParams {
    /// Source voicing (look-ahead, detector filter, de-bleed split).
    #[id = "mode"]
    pub mode: IntParam,
    /// Release duration from open to the Range floor, ms.
    #[id = "length"]
    pub length_ms: FloatParam,
    /// HF de-bleed amount.
    #[id = "debleed"]
    pub debleed: FloatParam,
    /// Let quieter (ghost) hits through — threshold −20 dB.
    #[id = "ghost"]
    pub ghost: BoolParam,
    /// Output gain, dB.
    #[id = "output"]
    pub output_db: FloatParam,
    /// Key the detector from the sidechain input.
    #[id = "sidechain"]
    pub sidechain: BoolParam,
}

/// `Style` labels.
const STYLE_LABELS: [&str; 2] = ["Classic", "Drum"];
/// Index of the Drum style.
const STYLE_DRUM: i32 = 1;

/// An int param's display formatter.
type Show = Arc<dyn Fn(i32) -> String + Send + Sync>;
/// An int param's parser.
type Parse = Arc<dyn Fn(&str) -> Option<i32> + Send + Sync>;

/// A label list as an int-param display, and its by-name parser.
fn labels(list: &'static [&'static str]) -> (Show, Parse) {
    (
        Arc::new(move |v| {
            usize::try_from(v)
                .ok()
                .and_then(|i| list.get(i))
                .map_or_else(|| v.to_string(), |s| (*s).to_string())
        }),
        Arc::new(move |t| {
            let t = t.trim();
            list.iter()
                .position(|l| l.eq_ignore_ascii_case(t))
                .and_then(|i| i32::try_from(i).ok())
                .or_else(|| t.parse().ok())
        }),
    )
}

/// Mode names, in parameter order.
const fn mode_labels() -> &'static [&'static str] {
    const NAMES: [&str; 4] = [
        Mode::ALL[0].name(),
        Mode::ALL[1].name(),
        Mode::ALL[2].name(),
        Mode::ALL[3].name(),
    ];
    &NAMES
}

impl Default for DrumParams {
    fn default() -> Self {
        Self {
            mode: {
                let (show, parse) = labels(mode_labels());
                IntParam::new("Mode", 0, IntRange::Linear { min: 0, max: 3 })
                    .with_value_to_string(show)
                    .with_string_to_value(parse)
            },
            length_ms: FloatParam::new(
                "Length",
                500.0,
                FloatRange::Linear {
                    min: 50.0,
                    max: 2000.0,
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            debleed: FloatParam::new(
                "Debleed",
                0.0,
                FloatRange::Linear {
                    min: -100.0,
                    max: 100.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            ghost: BoolParam::new("Ghost", false),
            output_db: FloatParam::new(
                "Output",
                0.0,
                FloatRange::Linear {
                    min: -48.0,
                    max: 6.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            sidechain: BoolParam::new("Sidechain", false),
        }
    }
}

impl Default for GateParams {
    fn default() -> Self {
        Self {
            key: EnumParam::new("Key", Key::Input),
            threshold_db: FloatParam::new(
                "Threshold",
                -40.0,
                FloatRange::Linear {
                    min: -80.0,
                    max: 0.0,
                },
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
                FloatRange::Linear {
                    min: 0.0,
                    max: 500.0,
                },
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
                FloatRange::Linear {
                    min: 0.0,
                    max: 90.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            hysteresis_db: FloatParam::new(
                "Hysteresis",
                4.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            style: {
                let (show, parse) = labels(&STYLE_LABELS);
                IntParam::new("Style", 0, IntRange::Linear { min: 0, max: 1 })
                    .with_value_to_string(show)
                    .with_string_to_value(parse)
            },
            drum: DrumParams::default(),
        }
    }
}

/// The gate's key: what opens it.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// The signal being gated.
    Input,
    /// The sidechain input (the host's aux port), linked across its channels.
    Sidechain,
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsGate {
    params: Arc<GateParams>,
    /// One gate for the whole frame — detection is stereo-linked (max of
    /// channel magnitudes keys the detector; one gain feeds every channel).
    gate: Option<Gate>,
    /// The Drum style's engine (allocated in `activate`).
    drum: Option<DrumGate>,
    /// Style the last block ran, to reset the other engine on a switch.
    last_drum: bool,
    /// Latency last reported to the host, samples.
    reported_latency: u32,
    sample_rate: f64,
}

impl Default for FtsGate {
    fn default() -> Self {
        Self {
            params: Arc::new(GateParams::default()),
            gate: None,
            drum: None,
            last_drum: false,
            reported_latency: 0,
            sample_rate: 48_000.0,
        }
    }
}

impl FtsGate {
    fn current_config(&self) -> GateConfig {
        GateConfig {
            threshold_db: f64::from(self.params.threshold_db.value()),
            hysteresis_db: f64::from(self.params.hysteresis_db.value()),
            attack_ms: f64::from(self.params.attack_ms.value()),
            hold_ms: f64::from(self.params.hold_ms.value()),
            release_ms: f64::from(self.params.release_ms.value()),
            // UI exposes attenuation as a positive amount; the DSP floor is
            // negative dB.
            range_db: -f64::from(self.params.range_db.value()),
        }
    }

    fn drum_style(&self) -> bool {
        self.params.style.value() == STYLE_DRUM
    }

    fn drum_settings(&self) -> DrumSettings {
        let p = &self.params;
        let mode = usize::try_from(p.drum.mode.value())
            .ok()
            .and_then(|i| Mode::ALL.get(i).copied())
            .unwrap_or_default();
        DrumSettings {
            mode,
            threshold_db: f64::from(p.threshold_db.value()),
            // Range is the attenuation amount; the engine wants the floor.
            reduction_db: -f64::from(p.range_db.value()),
            length_ms: f64::from(p.drum.length_ms.value()),
            debleed: f64::from(p.drum.debleed.value()),
            ghost: p.drum.ghost.value(),
            output_db: f64::from(p.drum.output_db.value()),
        }
    }

    /// Latency the current style needs, samples.
    fn wanted_latency(&self) -> u32 {
        if self.drum_style() {
            u32::try_from(DrumGate::latency_for(
                self.drum_settings().mode,
                self.sample_rate,
            ))
            .unwrap_or(0)
        } else {
            0
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
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        // With a stereo sidechain (used by the Drum style's Sidechain switch).
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2)],
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];

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
        context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = f64::from(buffer_config.sample_rate);
        let cfg = self.current_config();
        self.gate = Some(Gate::new(self.sample_rate, cfg));
        self.drum = Some(DrumGate::new(self.sample_rate, 2, self.drum_settings()));
        self.last_drum = self.drum_style();
        self.reported_latency = self.wanted_latency();
        context.set_latency_samples(self.reported_latency);
        true
    }

    fn reset(&mut self) {
        if let Some(g) = &mut self.gate {
            g.reset();
        }
        if let Some(d) = &mut self.drum {
            d.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let latency = self.wanted_latency();
        if latency != self.reported_latency {
            self.reported_latency = latency;
            context.set_latency_samples(latency);
        }
        let drum = self.drum_style();
        if drum != self.last_drum {
            // Switching styles: start the newly active engine from silence.
            self.last_drum = drum;
            self.reset();
        }
        if drum {
            self.process_drum(buffer, aux);
            return ProcessStatus::Normal;
        }
        self.sync_params();
        let Some(gate) = &mut self.gate else {
            return ProcessStatus::Normal;
        };
        let sidechain = match self.params.key.value() {
            Key::Sidechain => aux.inputs.first().map(Buffer::as_slice_immutable),
            Key::Input => None,
        };

        for (index, mut frame) in buffer.iter_samples().enumerate() {
            // Linked detection: key on the loudest channel of the frame —
            // the sidechain's when it keys the gate, else the input's.
            let mut key = 0.0f32;
            match sidechain {
                Some(channels) => {
                    for channel in channels {
                        key = key.max(channel.get(index).map_or(0.0, |s| s.abs()));
                    }
                }
                None => {
                    for sample in frame.iter_mut() {
                        key = key.max(sample.abs());
                    }
                }
            }
            // Advance detector + envelope once per frame, then apply the one
            // resulting gain to every channel.
            let _ = gate.process_sample_keyed(0.0, f64::from(key));
            #[expect(
                clippy::cast_possible_truncation,
                clippy::as_conversions,
                reason = "f64 gain to the host's f32 samples"
            )]
            let gain = gate.gain() as f32;
            for sample in frame.iter_mut() {
                *sample *= gain;
            }
        }
        ProcessStatus::Normal
    }
}

impl FtsGate {
    /// The Drum style: one linked engine over the stereo frame, keyed from
    /// the sidechain when it is switched on and connected.
    fn process_drum(&mut self, buffer: &mut Buffer, aux: &mut AuxiliaryBuffers) {
        let settings = self.drum_settings();
        let use_sidechain = self.params.drum.sidechain.value();
        let Some(drum) = &mut self.drum else {
            return;
        };
        if drum.settings() != settings {
            drum.set_settings(settings);
        }
        let sidechain = if use_sidechain {
            aux.inputs.first().map(Buffer::as_slice_immutable)
        } else {
            None
        };
        for (n, mut frame) in buffer.iter_samples().enumerate() {
            let mut io = [0.0f64; 2];
            for (slot, sample) in io.iter_mut().zip(frame.iter_mut()) {
                *slot = f64::from(*sample);
            }
            let key = sidechain.map(|sc| {
                let mut k = [0.0f64; 2];
                for (slot, ch) in k.iter_mut().zip(sc.iter()) {
                    *slot = ch.get(n).copied().map_or(0.0, f64::from);
                }
                // A mono sidechain keys both sides.
                if sc.len() == 1 {
                    k[1] = k[0];
                }
                k
            });
            drum.process_frame(&mut io, key.as_ref().map(<[f64; 2]>::as_slice));
            for (sample, v) in frame.iter_mut().zip(io) {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::as_conversions,
                    reason = "f64 engine back to the host's f32 buffer"
                )]
                {
                    *sample = v as f32;
                }
            }
        }
    }
}

impl ClapPlugin for FtsGate {
    const CLAP_ID: &'static str = "com.fasttrackstudio.gate";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Noise gate and drum gate: classic attack/hold/release, or drum modes with look-ahead, shaped release and HF de-bleed",
    );
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

#[cfg(feature = "export")]
nice_export_clap!(FtsGate);
#[cfg(feature = "export")]
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
        let amp = 10f64.powi(-3);
        let mut max_out = 0.0f64;
        for n in 0..48_000 {
            let x = amp * (2.0 * core::f64::consts::PI * 440.0 * f64::from(n) / SR).sin();
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
            let x = amp * (2.0 * core::f64::consts::PI * 440.0 * f64::from(n) / SR).sin();
            g.process_sample(x);
        }
        assert!(g.gain() > 0.95, "gate failed to open: gain = {}", g.gain());
    }

    /// After the signal stops, hold elapses and the gain decays toward the
    /// range floor at the release rate.
    #[test]
    fn release_decays_after_hold() {
        let mut g = Gate::new(SR, test_cfg());
        let amp = 10f64.powf(-6.0 / 20.0);
        for n in 0..4800 {
            let x = amp * (2.0 * core::f64::consts::PI * 440.0 * f64::from(n) / SR).sin();
            g.process_sample(x);
        }
        assert!(g.gain() > 0.95);
        // Feed silence: hold (10 ms) + several release constants (100 ms).
        for _ in 0..48_000 {
            g.process_sample(0.0);
        }
        assert!(g.gain() < 0.01, "gain failed to release: {}", g.gain());
    }
}
