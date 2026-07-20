//! FTS Modulation — CLAP/VST3 multi-mode modulation plugin.
//!
//! A thin nice-plug shell over the [`modulation`] facade: one plugin, five
//! modes. Chorus / Flanger / Vibrato share a [`modulation::chorus`]
//! `ChorusChain` (Cubic engine, 1–4 voices); Tremolo runs a
//! [`modulation::trem`] `TremChain` (free-running sine LFO); Wah runs a
//! [`modulation::wah`] `WahChain` as an envelope-follower auto-wah.
//!
//! All three chains are pre-allocated up front and updated in `initialize()`;
//! `process()` only dispatches on the current mode and pushes plain field
//! values — no allocation on the audio thread (the chorus engine is never
//! switched, so `set_engine`'s reallocation path is never taken).
//!
//! GUI is deliberately absent for now (headless, host-generic params),
//! matching level-plugin; the nice-plug-dioxus editor is a follow-up.

use nice_plug::prelude::*;
use std::sync::Arc;

use audiocore_dsp::{AudioConfig, Processor};
use modulation::chorus::chain::ChorusChain;
use modulation::chorus::engine::EffectType;
use modulation::trem::chain::TremChain;
use modulation::trem::fts_modulation::trigger::TriggerMode;
use modulation::trem::tremolo::TremMode;
use modulation::wah::chain::{WahChain, WahSource};

const PLUGIN_NAME: &str = "FTS Modulation";

// ── Parameters ────────────────────────────────────────────────────────────

/// Which modulation engine processes the audio.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    #[name = "Chorus"]
    Chorus,
    #[name = "Flanger"]
    Flanger,
    #[name = "Vibrato"]
    Vibrato,
    #[name = "Tremolo"]
    Tremolo,
    #[name = "Wah"]
    Wah,
}

/// Tremolo voicing, mirrored onto [`TremMode`].
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TremVoicing {
    #[name = "Mono"]
    Mono,
    #[name = "Stereo"]
    Stereo,
    #[name = "Harmonic"]
    Harmonic,
}

impl From<TremVoicing> for TremMode {
    fn from(value: TremVoicing) -> Self {
        match value {
            TremVoicing::Mono => TremMode::Mono,
            TremVoicing::Stereo => TremMode::Stereo,
            TremVoicing::Harmonic => TremMode::Harmonic,
        }
    }
}

#[derive(Params)]
pub struct ModulationParams {
    /// Active engine.
    #[id = "mode"]
    pub mode: EnumParam<Mode>,
    /// LFO rate (chorus/flanger/vibrato delay LFO, tremolo amplitude LFO).
    /// The wah is envelope-driven, so rate has no effect there.
    #[id = "rate"]
    pub rate: FloatParam,
    /// Modulation depth (chorus family depth, tremolo depth, wah envelope amount).
    #[id = "depth"]
    pub depth: FloatParam,
    /// Dry/wet mix. Vibrato is inherently wet-only and ignores this.
    #[id = "mix"]
    pub mix: FloatParam,
    /// Chorus family only: voices per channel.
    #[id = "voices"]
    pub voices: IntParam,
    /// Tremolo only: mono / stereo (90° offset) / harmonic voicing.
    #[id = "trem_mode"]
    pub trem_mode: EnumParam<TremVoicing>,
    /// Wah only: base pedal position the envelope sweeps from.
    #[id = "wah_pos"]
    pub wah_position: FloatParam,
}

impl Default for ModulationParams {
    fn default() -> Self {
        Self {
            mode: EnumParam::new("Mode", Mode::Chorus),
            rate: FloatParam::new(
                "Rate",
                1.0,
                FloatRange::Skewed {
                    min: 0.01,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            depth: FloatParam::new("Depth", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_unit("%"),
            mix: FloatParam::new("Mix", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_unit("%"),
            voices: IntParam::new("Voices", 2, IntRange::Linear { min: 1, max: 4 }),
            trem_mode: EnumParam::new("Trem Mode", TremVoicing::Mono),
            wah_position: FloatParam::new(
                "Wah Position",
                0.3,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_unit("%"),
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsModulation {
    params: Arc<ModulationParams>,
    /// Chorus / flanger / vibrato (Cubic engine — never switched, no realloc).
    chorus: ChorusChain,
    /// Tremolo (free-running LFO).
    trem: TremChain,
    /// Envelope-follower auto-wah.
    wah: WahChain,
    /// Last mode seen by `sync_params`, so a mode switch resets the incoming
    /// chain instead of resuming from stale state.
    current_mode: Mode,
    sample_rate: f64,
}

impl Default for FtsModulation {
    fn default() -> Self {
        let mut trem = TremChain::new();
        // Free-running LFO driven by the Rate param (sync_index 0 = free Hz).
        trem.modulator.trigger.mode = TriggerMode::Free;
        trem.modulator.trigger.sync_index = 0;

        let mut wah = WahChain::new();
        wah.source = WahSource::Envelope;

        Self {
            params: Arc::new(ModulationParams::default()),
            chorus: ChorusChain::new(),
            trem,
            wah,
            current_mode: Mode::Chorus,
            sample_rate: 48_000.0,
        }
    }
}

impl FtsModulation {
    /// Push the current params into the active chain (plain field writes and
    /// `reset()` only — no allocation).
    fn sync_params(&mut self) {
        let p = &self.params;
        let mode = p.mode.value();

        // Reset the incoming chain on a mode switch so it starts clean.
        if mode != self.current_mode {
            self.current_mode = mode;
            match mode {
                Mode::Chorus | Mode::Flanger | Mode::Vibrato => self.chorus.reset(),
                Mode::Tremolo => self.trem.reset(),
                Mode::Wah => self.wah.reset(),
            }
        }

        let rate = p.rate.value() as f64;
        let depth = p.depth.value() as f64;
        let mix = p.mix.value() as f64;

        match mode {
            Mode::Chorus | Mode::Flanger | Mode::Vibrato => {
                let c = &mut self.chorus;
                c.effect_type = match mode {
                    Mode::Flanger => EffectType::Flanger,
                    Mode::Vibrato => EffectType::Vibrato,
                    _ => EffectType::Chorus,
                };
                c.rate_hz = rate;
                c.depth = depth;
                c.num_voices = p.voices.value() as usize;
                // Vibrato is wet-only; the chain enforces it, keep mix coherent.
                c.mix = if c.effect_type == EffectType::Vibrato { 1.0 } else { mix };
            }
            Mode::Tremolo => {
                let t = &mut self.trem;
                t.set_mode(p.trem_mode.value().into());
                t.set_depth(depth);
                t.mix = mix;
                t.modulator.trigger.mode = TriggerMode::Free;
                t.modulator.trigger.sync_index = 0;
                t.modulator.trigger.rate_hz = rate;
                // Stereo voicing = 90° L/R phase offset; the chain only reads
                // its stereo path when stereo_phase is nonzero.
                let (phase_deg, offset) = if p.trem_mode.value() == TremVoicing::Stereo {
                    (90.0, 0.25)
                } else {
                    (0.0, 0.0)
                };
                t.stereo_phase = phase_deg;
                t.modulator.stereo_offset = offset;
            }
            Mode::Wah => {
                let w = &mut self.wah;
                w.source = WahSource::Envelope;
                w.env_amount = depth;
                w.base_position = p.wah_position.value() as f64;
                w.mix = mix;
            }
        }
    }
}

impl Plugin for FtsModulation {
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
        let config = AudioConfig {
            sample_rate: self.sample_rate,
            max_buffer_size: buffer_config.max_buffer_size as usize,
        };
        // Pre-allocate / retune all three engines up front so process() only
        // ever dispatches.
        self.chorus.update(config);
        self.trem.update(config);
        self.wah.update(config);
        true
    }

    fn reset(&mut self) {
        self.chorus.reset();
        self.trem.reset();
        self.wah.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync_params();
        let mode = self.current_mode;

        // Process in fixed stack chunks, converting f32 <-> f64.
        const CHUNK: usize = 128;
        let num_samples = buffer.samples();
        let mut offset = 0;

        while offset < num_samples {
            let end = (offset + CHUNK).min(num_samples);
            let len = end - offset;

            let mut left = [0.0f64; CHUNK];
            let mut right = [0.0f64; CHUNK];

            let channels = buffer.as_slice();
            for i in 0..len {
                left[i] = channels[0][offset + i] as f64;
                right[i] = channels[1][offset + i] as f64;
            }

            match mode {
                Mode::Chorus | Mode::Flanger | Mode::Vibrato => {
                    self.chorus.process(&mut left[..len], &mut right[..len]);
                }
                Mode::Tremolo => {
                    self.trem.process(&mut left[..len], &mut right[..len]);
                }
                Mode::Wah => {
                    self.wah.process(&mut left[..len], &mut right[..len]);
                }
            }

            let channels = buffer.as_slice();
            for i in 0..len {
                channels[0][offset + i] = left[i] as f32;
                channels[1][offset + i] = right[i] as f32;
            }

            offset = end;
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsModulation {
    const CLAP_ID: &'static str = "com.fasttrackstudio.modulation";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Multi-mode modulation: chorus, flanger, vibrato, tremolo, and auto-wah");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Chorus,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsModulation {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsModPlugin0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nice_export_clap!(FtsModulation);
nice_export_vst3!(FtsModulation);
