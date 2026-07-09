//! FTS Trem — nih-plug entry point.
//!
//! Tremolator-inspired tremolo/auto-gate with groove, feel, accent,
//! dynamics envelope, and analog-style saturation.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use atomic_float::AtomicF32;
use fts_dsp::{AudioConfig, Processor};
use fts_modulation::tempo::{TransportInfo, SYNC_TABLE};
use fts_modulation::trigger::TriggerMode;
use fts_plugin_core::prelude::*;
use trem_dsp::chain::TremChain;
use trem_dsp::dynamics::DynMode;
use trem_dsp::tremolo::{AnalogStyle, TremMode};

// ── Helpers ──────────────────────────────────────────────────────────

fn bool_formatter() -> Arc<dyn Fn(f32) -> String + Send + Sync> {
    Arc::new(|v| {
        if v > 0.5 {
            "On".to_string()
        } else {
            "Off".to_string()
        }
    })
}

fn bool_parser() -> Arc<dyn Fn(&str) -> Option<f32> + Send + Sync> {
    Arc::new(|s| match s.trim().to_lowercase().as_str() {
        "on" | "1" | "true" => Some(1.0),
        "off" | "0" | "false" => Some(0.0),
        _ => s.parse().ok(),
    })
}

fn sync_division_formatter() -> Arc<dyn Fn(i32) -> String + Send + Sync> {
    Arc::new(|v| {
        let labels = [
            "Free",  // 0
            "1/256", // 1
            "1/128", // 2
            "1/64",  // 3
            "1/32",  // 4
            "1/16",  // 5
            "1/8",   // 6
            "1/4",   // 7
            "1/2",   // 8
            "1/1",   // 9
            "2/1",   // 10
            "4/1",   // 11
            "1/16t", // 12
            "1/8t",  // 13
            "1/4t",  // 14
            "1/2t",  // 15
            "1/1t",  // 16
            "1/16.", // 17
            "1/8.",  // 18
            "1/4.",  // 19
            "1/2.",  // 20
            "1/1.",  // 21
        ];
        labels.get(v as usize).unwrap_or(&"1/4").to_string()
    })
}

// ── Parameters ──────────────────────────────────────────────────────

#[derive(Params)]
pub struct FtsTremParams {
    // --- Main Panel ---
    #[id = "depth"]
    pub depth: FloatParam,
    #[id = "rate"]
    pub rate: FloatParam,
    #[id = "sync_enable"]
    pub sync_enable: FloatParam,
    #[id = "rhythm"]
    pub rhythm: IntParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "mode"]
    pub mode: IntParam,
    #[id = "groove"]
    pub groove: FloatParam,
    #[id = "feel"]
    pub feel: FloatParam,
    #[id = "accent"]
    pub accent: FloatParam,

    // --- Tweak (Dynamics) ---
    #[id = "threshold"]
    pub threshold: FloatParam,
    #[id = "attack"]
    pub attack: FloatParam,
    #[id = "release"]
    pub release: FloatParam,
    #[id = "dyn_mode"]
    pub dyn_mode: IntParam,
    #[id = "rate_mod"]
    pub rate_mod: FloatParam,
    #[id = "depth_mod"]
    pub depth_mod: FloatParam,

    // --- Output ---
    #[id = "width"]
    pub width: FloatParam,
    #[id = "analog_style"]
    pub analog_style: IntParam,
    #[id = "stereo_phase"]
    pub stereo_phase: FloatParam,
    #[id = "crossover"]
    pub crossover: FloatParam,
}

impl Default for FtsTremParams {
    fn default() -> Self {
        Self {
            // --- Main Panel ---
            depth: FloatParam::new("Depth", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            rate: FloatParam::new(
                "Rate",
                120.0,
                FloatRange::Skewed {
                    min: 30.0,
                    max: 240.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" BPM")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            sync_enable: FloatParam::new("Sync", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(bool_formatter())
                .with_string_to_value(bool_parser()),

            rhythm: IntParam::new(
                "Rhythm",
                7, // 1/4 note
                IntRange::Linear {
                    min: 0,
                    max: (SYNC_TABLE.len() - 1) as i32,
                },
            )
            .with_value_to_string(sync_division_formatter()),

            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            mode: IntParam::new("Mode", 0, IntRange::Linear { min: 0, max: 2 })
                .with_value_to_string(Arc::new(|v| match v {
                    0 => "Mono".to_string(),
                    1 => "Stereo".to_string(),
                    2 => "Harmonic".to_string(),
                    _ => "Mono".to_string(),
                })),

            groove: FloatParam::new(
                "Groove",
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(0)),

            feel: FloatParam::new(
                "Feel",
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(0)),

            accent: FloatParam::new(
                "Accent",
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(0)),

            // --- Tweak (Dynamics) ---
            threshold: FloatParam::new(
                "Threshold",
                -20.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            attack: FloatParam::new(
                "Attack",
                10.0,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 5000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            release: FloatParam::new(
                "Release",
                200.0,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 5000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            dyn_mode: IntParam::new("Dyn Mode", 0, IntRange::Linear { min: 0, max: 1 })
                .with_value_to_string(Arc::new(|v| match v {
                    0 => "Env".to_string(),
                    1 => "Gate".to_string(),
                    _ => "Env".to_string(),
                })),

            rate_mod: FloatParam::new(
                "Rate Mod",
                0.0,
                FloatRange::Linear {
                    min: -4.0,
                    max: 4.0,
                },
            )
            .with_unit(" oct")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            depth_mod: FloatParam::new(
                "Depth Mod",
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(0)),

            // --- Output ---
            width: FloatParam::new("Width", 1.0, FloatRange::Linear { min: 0.0, max: 2.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            analog_style: IntParam::new("Analog", 0, IntRange::Linear { min: 0, max: 6 })
                .with_value_to_string(Arc::new(|v| match v {
                    0 => "Clean".to_string(),
                    1 => "Fat".to_string(),
                    2 => "Squash".to_string(),
                    3 => "Dirt".to_string(),
                    4 => "Crunch".to_string(),
                    5 => "Shred".to_string(),
                    6 => "Pump".to_string(),
                    _ => "Clean".to_string(),
                })),

            stereo_phase: FloatParam::new(
                "Stereo Phase",
                0.0,
                FloatRange::Linear {
                    min: -180.0,
                    max: 180.0,
                },
            )
            .with_unit("°")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            crossover: FloatParam::new(
                "Crossover",
                800.0,
                FloatRange::Skewed {
                    min: 200.0,
                    max: 5000.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
        }
    }
}

// ── Plugin ──────────────────────────────────────────────────────────

struct FtsTrem {
    params: Arc<FtsTremParams>,
    input_peak_db: AtomicF32,
    output_peak_db: AtomicF32,
    chain: TremChain,
    sample_rate: f64,
    host_bpm: f64,
    host_pos_qn: f64,
    host_playing: bool,
}

impl Default for FtsTrem {
    fn default() -> Self {
        Self {
            params: Arc::new(FtsTremParams::default()),
            input_peak_db: AtomicF32::new(-100.0),
            output_peak_db: AtomicF32::new(-100.0),
            chain: TremChain::new(),
            sample_rate: 48000.0,
            host_bpm: 120.0,
            host_pos_qn: 0.0,
            host_playing: false,
        }
    }
}

impl FtsTrem {
    fn sync_params(&mut self) {
        let p = &self.params;
        let c = &mut self.chain;

        // --- Mode ---
        let trem_mode = match p.mode.value() {
            1 => TremMode::Stereo,
            2 => TremMode::Harmonic,
            _ => TremMode::Mono,
        };
        c.set_mode(trem_mode);

        // --- Depth & Mix ---
        c.set_depth(p.depth.value() as f64);
        c.mix = p.mix.value() as f64;

        // --- Sync / Rate ---
        let sync = p.sync_enable.value() > 0.5;
        if sync {
            c.modulator.trigger.mode = TriggerMode::Sync;
            let idx = p.rhythm.value() as usize;
            c.modulator.trigger.sync_index = idx.min(SYNC_TABLE.len() - 1);
        } else {
            c.modulator.trigger.mode = TriggerMode::Free;
            c.modulator.trigger.sync_index = 0; // Free Hz
                                                // Convert BPM to Hz: BPM / 60
            let rate_hz = p.rate.value() as f64 / 60.0;
            c.modulator.trigger.rate_hz = rate_hz;
        }

        // --- Groove / Feel / Accent ---
        c.groove = p.groove.value() as f64;
        c.feel = p.feel.value() as f64;
        c.accent = p.accent.value() as f64;

        // --- Dynamics ---
        c.dynamics.threshold_db = p.threshold.value() as f64;
        c.dynamics.attack_ms = p.attack.value() as f64;
        c.dynamics.release_ms = p.release.value() as f64;
        c.dynamics.mode = match p.dyn_mode.value() {
            1 => DynMode::Gate,
            _ => DynMode::Env,
        };
        c.dynamics.rate_mod = p.rate_mod.value() as f64;
        c.dynamics.depth_mod = p.depth_mod.value() as f64;

        // --- Output ---
        c.stereo_phase = p.stereo_phase.value() as f64;

        let analog = match p.analog_style.value() {
            1 => AnalogStyle::Fat,
            2 => AnalogStyle::Squash,
            3 => AnalogStyle::Dirt,
            4 => AnalogStyle::Crunch,
            5 => AnalogStyle::Shred,
            6 => AnalogStyle::Pump,
            _ => AnalogStyle::Clean,
        };
        c.set_analog_style(analog);

        // Crossover frequency (for harmonic mode)
        let xover = p.crossover.value() as f64;
        c.tremolo_l.crossover_freq = xover;
        c.tremolo_r.crossover_freq = xover;

        // Update DSP coefficients
        c.update(AudioConfig {
            sample_rate: self.sample_rate,
            max_buffer_size: 512,
        });
    }
}

impl Plugin for FtsTrem {
    const NAME: &'static str = "FTS Trem";
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "";
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
        self.chain.update(AudioConfig {
            sample_rate: self.sample_rate,
            max_buffer_size: buffer_config.max_buffer_size as usize,
        });
        true
    }

    fn reset(&mut self) {
        self.chain.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Read host transport
        let transport = context.transport();
        if let Some(tempo) = transport.tempo {
            if tempo > 0.0 {
                self.host_bpm = tempo;
            }
        }
        if let Some(pos) = transport.pos_beats() {
            self.host_pos_qn = pos;
        }
        self.host_playing = transport.playing;

        // Set transport on chain
        self.chain.set_transport(TransportInfo {
            position_qn: self.host_pos_qn,
            tempo_bpm: self.host_bpm,
            playing: self.host_playing,
        });

        // Sync all parameters to DSP
        self.sync_params();

        // Process in 128-sample chunks, converting f32 <-> f64
        const CHUNK: usize = 128;
        let num_samples = buffer.samples();
        let mut offset = 0;
        let mut input_peak: f32 = 0.0;
        let mut output_peak: f32 = 0.0;

        while offset < num_samples {
            let end = (offset + CHUNK).min(num_samples);
            let len = end - offset;

            let mut left_f64 = [0.0f64; CHUNK];
            let mut right_f64 = [0.0f64; CHUNK];

            // f32 -> f64 + measure input
            let channel_slices = buffer.as_slice();
            for i in 0..len {
                let l = channel_slices[0][offset + i];
                let r = channel_slices[1][offset + i];
                input_peak = input_peak.max(l.abs()).max(r.abs());
                left_f64[i] = l as f64;
                right_f64[i] = r as f64;
            }

            // Process
            self.chain
                .process(&mut left_f64[..len], &mut right_f64[..len]);

            // f64 -> f32 + measure output
            let channel_slices = buffer.as_slice();
            for i in 0..len {
                let l = left_f64[i] as f32;
                let r = right_f64[i] as f32;
                output_peak = output_peak.max(l.abs()).max(r.abs());
                channel_slices[0][offset + i] = l;
                channel_slices[1][offset + i] = r;
            }

            offset = end;
        }

        // Update metering with decay
        let in_db = if input_peak > 0.0 {
            20.0 * input_peak.log10()
        } else {
            -100.0
        };
        let out_db = if output_peak > 0.0 {
            20.0 * output_peak.log10()
        } else {
            -100.0
        };

        let prev_in = self.input_peak_db.load(Ordering::Relaxed);
        self.input_peak_db.store(
            if in_db > prev_in {
                in_db
            } else {
                prev_in - 0.3
            },
            Ordering::Relaxed,
        );

        let prev_out = self.output_peak_db.load(Ordering::Relaxed);
        self.output_peak_db.store(
            if out_db > prev_out {
                out_db
            } else {
                prev_out - 0.3
            },
            Ordering::Relaxed,
        );

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsTrem {
    const CLAP_ID: &'static str = "com.fasttrackstudio.trem";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Tremolo/auto-gate with groove, dynamics, and analog saturation");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Custom("tremolo"),
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsTrem {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsTremPlugin_01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nih_export_clap!(FtsTrem);
nih_export_vst3!(FtsTrem);
