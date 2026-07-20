//! FTS Trigger — CLAP/VST3 drum-trigger plugin.
//!
//! A nice-plug shell over the legacy FTS-Trigger engine
//! ([`trigger::TriggerChain`]): each stereo frame runs through the chain's
//! detection sidechain (`detect_tick` — HPF/LPF → onset detector → velocity
//! curve) while the audio passes through untouched, and every detected onset
//! becomes a sample-accurate `NoteOn` (channel 0, configurable note) with a
//! matching `NoteOff` a few ms later — scheduled across block boundaries when
//! needed. `MIDI_OUTPUT = MidiConfig::Basic` declares the note-out port (the
//! mirror of `signal-sampler-clap`'s note *input*).
//!
//! Detection can be the zero-latency time-domain peak envelope or one of the
//! six FFT onset detection functions (spectral flux / SuperFlux / HFC /
//! complex domain / rectified complex domain / modified KL); FFT modes report
//! their latency to the host.
//!
//! Listen mode mutes the passthrough and replaces it with a short 1 kHz click
//! per hit (scaled by velocity) for threshold tuning.
//!
//! The chain's sample playback engine (`sampler` — velocity layers +
//! round-robin) is deliberately NOT wired yet: loading samples needs
//! file-management UI/state, so this shell is MIDI-out + passthrough only.
//! GUI is also deliberately absent (headless, host-generic params), matching
//! `level-plugin`; both are follow-ups.

use nice_plug::prelude::*;
use std::sync::Arc;

use trigger::detector::DetectAlgorithm;
use trigger::velocity::{VelocityCurve, VelocityMapper};
use trigger::{AudioConfig, Processor, TriggerChain};

const PLUGIN_NAME: &str = "FTS Trigger";

/// Emitted note length. MUST stay shorter than the minimum retrigger guard
/// (5 ms) so at most one NoteOff is ever pending per hit ordering-wise —
/// a NoteOn can never be sent with a timing before the previous NoteOff.
const NOTE_LEN_MS: f32 = 4.0;
/// Listen-mode click: length and frequency of the per-hit sine burst.
const CLICK_LEN_MS: f32 = 2.0;
const CLICK_FREQ_HZ: f32 = 1_000.0;

// ── Parameters ────────────────────────────────────────────────────────────

#[derive(Params)]
pub struct TriggerParams {
    /// Absolute onset threshold.
    #[id = "threshold"]
    pub threshold_db: FloatParam,
    /// Detection confirmation window (the legacy engine's "sensitivity"):
    /// the level must hold above the threshold this long before the trigger
    /// fires. 0 = fire immediately; longer = fewer false triggers from spikes.
    #[id = "sensitivity"]
    pub sensitivity_ms: FloatParam,
    /// Retrigger-guard window.
    #[id = "retrigger"]
    pub retrigger_ms: FloatParam,
    /// MIDI note to emit (default 36 = C1 kick).
    #[id = "note"]
    pub note: IntParam,
    /// Velocity floor (output clamp, 0-1).
    #[id = "vel_min"]
    pub vel_min: FloatParam,
    /// Velocity ceiling (output clamp, 0-1).
    #[id = "vel_max"]
    pub vel_max: FloatParam,
    /// Mute passthrough and click on every hit (threshold tuning).
    #[id = "listen"]
    pub listen: BoolParam,
    /// Sidechain HPF frequency; 0 = off. Isolates the drum from low bleed.
    #[id = "sc_hpf"]
    pub sc_hpf_hz: FloatParam,
    /// Sidechain LPF frequency; 0 = off. Rejects cymbal/hat bleed.
    #[id = "sc_lpf"]
    pub sc_lpf_hz: FloatParam,
    /// Detection algorithm: peak envelope (zero latency) or an FFT onset
    /// detection function.
    #[id = "algorithm"]
    pub algorithm: IntParam,
    /// Velocity curve.
    #[id = "vel_curve"]
    pub vel_curve: IntParam,
    /// Velocity dynamics: 0 = fixed velocity, 1 = full dynamic range.
    #[id = "dynamics"]
    pub dynamics: FloatParam,
}

fn algorithm_from_index(i: i32) -> DetectAlgorithm {
    match i {
        1 => DetectAlgorithm::SpectralFlux,
        2 => DetectAlgorithm::SuperFlux,
        3 => DetectAlgorithm::Hfc,
        4 => DetectAlgorithm::ComplexDomain,
        5 => DetectAlgorithm::RectifiedComplexDomain,
        6 => DetectAlgorithm::ModifiedKl,
        _ => DetectAlgorithm::PeakEnvelope,
    }
}

fn curve_from_index(i: i32) -> VelocityCurve {
    match i {
        1 => VelocityCurve::Logarithmic,
        2 => VelocityCurve::Exponential,
        3 => VelocityCurve::Fixed,
        _ => VelocityCurve::Linear,
    }
}

impl Default for TriggerParams {
    fn default() -> Self {
        Self {
            threshold_db: FloatParam::new(
                "Threshold",
                -30.0,
                FloatRange::Linear { min: -60.0, max: 0.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            sensitivity_ms: FloatParam::new(
                "Sensitivity",
                1.0,
                FloatRange::Linear { min: 0.0, max: 10.0 },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            retrigger_ms: FloatParam::new(
                "Retrigger",
                40.0,
                FloatRange::Skewed {
                    min: 5.0,
                    max: 200.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            note: IntParam::new("Note", 36, IntRange::Linear { min: 0, max: 127 })
                .with_value_to_string(formatters::v2s_i32_note_formatter()),
            vel_min: FloatParam::new("Vel Min", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            vel_max: FloatParam::new("Vel Max", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            listen: BoolParam::new("Listen", false),
            sc_hpf_hz: FloatParam::new(
                "SC HPF",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1_000.0 },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            sc_lpf_hz: FloatParam::new(
                "SC LPF",
                0.0,
                FloatRange::Linear { min: 0.0, max: 20_000.0 },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            algorithm: IntParam::new("Algorithm", 0, IntRange::Linear { min: 0, max: 6 })
                .with_value_to_string(Arc::new(|v| {
                    match v {
                        1 => "Spectral Flux",
                        2 => "SuperFlux",
                        3 => "HFC",
                        4 => "Complex",
                        5 => "Rect Complex",
                        6 => "Mod KL",
                        _ => "Peak Env",
                    }
                    .to_string()
                })),
            vel_curve: IntParam::new("Curve", 0, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(Arc::new(|v| {
                    match v {
                        1 => "Log",
                        2 => "Exp",
                        3 => "Fixed",
                        _ => "Linear",
                    }
                    .to_string()
                })),
            dynamics: FloatParam::new("Dynamics", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

/// A NoteOff owed to a hit whose note length spilled past the block end.
#[derive(Clone, Copy)]
struct PendingOff {
    /// Samples from the start of the *next* processed block.
    remaining: u32,
    note: u8,
}

/// Cached copy of the last-synced param values so the chain (filter design,
/// spectral detector) is only rebuilt when something actually changed.
#[derive(Clone, Copy, PartialEq)]
struct SyncedParams {
    threshold_db: f32,
    sensitivity_ms: f32,
    retrigger_ms: f32,
    vel_min: f32,
    vel_max: f32,
    sc_hpf_hz: f32,
    sc_lpf_hz: f32,
    algorithm: i32,
    vel_curve: i32,
    dynamics: f32,
}

pub struct FtsTrigger {
    params: Arc<TriggerParams>,
    chain: TriggerChain,
    synced: Option<SyncedParams>,
    sample_rate: f32,
    note_len_samples: u32,
    /// NoteOffs carried across block boundaries. NOTE_LEN < min retrigger
    /// guard, so this can only ever hold one live entry — sized for slack.
    pending_offs: [Option<PendingOff>; 4],
    // Listen-mode click state.
    click_remaining: u32,
    click_phase: f32,
    click_step: f32,
    click_gain: f32,
}

impl Default for FtsTrigger {
    fn default() -> Self {
        Self {
            params: Arc::new(TriggerParams::default()),
            chain: TriggerChain::new(),
            synced: None,
            sample_rate: 48_000.0,
            note_len_samples: (NOTE_LEN_MS * 48.0) as u32,
            pending_offs: [None; 4],
            click_remaining: 0,
            click_phase: 0.0,
            click_step: 0.0,
            click_gain: 0.0,
        }
    }
}

impl FtsTrigger {
    fn audio_config(&self) -> AudioConfig {
        AudioConfig {
            sample_rate: self.sample_rate as f64,
            max_buffer_size: 512,
        }
    }

    /// Push the current params into the chain. Filter/detector rebuilds only
    /// happen when a value actually changed (`chain.update` redesigns the
    /// sidechain filters and can reallocate the spectral detector).
    fn sync_params(&mut self, context: &mut impl ProcessContext<Self>) {
        let now = SyncedParams {
            threshold_db: self.params.threshold_db.value(),
            sensitivity_ms: self.params.sensitivity_ms.value(),
            retrigger_ms: self.params.retrigger_ms.value(),
            vel_min: self.params.vel_min.value(),
            vel_max: self.params.vel_max.value(),
            sc_hpf_hz: self.params.sc_hpf_hz.value(),
            sc_lpf_hz: self.params.sc_lpf_hz.value(),
            algorithm: self.params.algorithm.value(),
            vel_curve: self.params.vel_curve.value(),
            dynamics: self.params.dynamics.value(),
        };
        if self.synced == Some(now) {
            return;
        }

        let algorithm_changed =
            self.synced.map(|s| s.algorithm) != Some(now.algorithm);
        self.synced = Some(now);

        self.chain.threshold_db = now.threshold_db as f64;
        self.chain.detect_time_ms = now.sensitivity_ms as f64;
        self.chain.retrigger_ms = now.retrigger_ms as f64;
        self.chain.dynamics = now.dynamics as f64;
        self.chain.velocity_curve = curve_from_index(now.vel_curve);
        self.chain.velocity.min_velocity = now.vel_min as f64;
        self.chain.velocity.max_velocity = now.vel_max.max(now.vel_min) as f64;
        self.chain.detector.algorithm = algorithm_from_index(now.algorithm);
        self.chain.set_sc_hpf(now.sc_hpf_hz as f64);
        self.chain.set_sc_lpf(now.sc_lpf_hz as f64);
        self.chain.update(self.audio_config());

        if algorithm_changed {
            context.set_latency_samples(self.chain.latency_samples() as u32);
        }
    }

    fn schedule_off(&mut self, off: PendingOff) {
        for slot in &mut self.pending_offs {
            if slot.is_none() {
                *slot = Some(off);
                return;
            }
        }
    }
}

impl Plugin for FtsTrigger {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Audio effect with a note output: stereo in/out (mono fallback).
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    /// The whole point: a note-event output port.
    const MIDI_OUTPUT: MidiConfig = MidiConfig::Basic;

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
        self.sample_rate = buffer_config.sample_rate;
        self.synced = None; // force a full re-sync at the new rate
        self.chain.update(AudioConfig {
            sample_rate: self.sample_rate as f64,
            max_buffer_size: buffer_config.max_buffer_size as usize,
        });
        self.note_len_samples =
            ((NOTE_LEN_MS * 0.001 * self.sample_rate) as u32).max(1);
        self.click_step =
            2.0 * std::f32::consts::PI * CLICK_FREQ_HZ / self.sample_rate;
        true
    }

    fn reset(&mut self) {
        self.chain.reset();
        self.pending_offs = [None; 4];
        self.click_remaining = 0;
        self.click_phase = 0.0;
        self.click_gain = 0.0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync_params(context);

        let block_len = buffer.samples() as u32;
        let note = self.params.note.value().clamp(0, 127) as u8;
        let listen = self.params.listen.value();
        let click_len =
            ((CLICK_LEN_MS * 0.001 * self.sample_rate) as u32).max(1);
        // Confirmation window delay: the detector fires at the END of the
        // detect window, so place the note back at the onset (clamped to the
        // block start when the onset was in the previous block). FFT modes
        // have additional latency, reported to the host instead.
        let confirm_samples = (self.params.sensitivity_ms.value() * 0.001
            * self.sample_rate) as u32;

        // Flush NoteOffs owed from previous blocks.
        for slot in &mut self.pending_offs {
            if let Some(off) = *slot {
                if off.remaining < block_len {
                    context.send_event(NoteEvent::NoteOff {
                        timing: off.remaining,
                        voice_id: None,
                        channel: 0,
                        note: off.note,
                        velocity: 0.0,
                    });
                    *slot = None;
                } else {
                    *slot = Some(PendingOff {
                        remaining: off.remaining - block_len,
                        note: off.note,
                    });
                }
            }
        }

        for (i, mut frame) in buffer.iter_samples().enumerate() {
            // The chain sidechain wants a stereo pair; duplicate mono.
            let mut it = frame.iter_mut();
            let l = it.next().map(|s| *s as f64).unwrap_or(0.0);
            let r = it.next().map(|s| *s as f64).unwrap_or(l);

            if let Some(vel) = self.chain.detect_tick(l, r) {
                let onset = (i as u32).saturating_sub(confirm_samples);
                let velocity =
                    (VelocityMapper::to_midi(vel) as f32 / 127.0).clamp(0.0, 1.0);
                context.send_event(NoteEvent::NoteOn {
                    timing: onset,
                    voice_id: None,
                    channel: 0,
                    note,
                    velocity,
                });
                let off_at = onset + self.note_len_samples;
                if off_at < block_len {
                    context.send_event(NoteEvent::NoteOff {
                        timing: off_at,
                        voice_id: None,
                        channel: 0,
                        note,
                        velocity: 0.0,
                    });
                } else {
                    self.schedule_off(PendingOff {
                        remaining: off_at - block_len,
                        note,
                    });
                }
                if listen {
                    self.click_remaining = click_len;
                    self.click_phase = 0.0;
                    self.click_gain = 0.25 + 0.5 * velocity;
                }
            }

            if listen {
                // Mute passthrough; output the tuning click instead.
                let click = if self.click_remaining > 0 {
                    self.click_remaining -= 1;
                    let s = self.click_phase.sin() * self.click_gain;
                    self.click_phase += self.click_step;
                    s
                } else {
                    0.0
                };
                for sample in frame.iter_mut() {
                    *sample = click;
                }
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsTrigger {
    const CLAP_ID: &'static str = "com.fasttrackstudio.trigger";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Drum trigger: audio in, sample-accurate MIDI notes out with velocity from hit level");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::NoteDetector,
        ClapFeature::Drum,
        ClapFeature::Utility,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsTrigger {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsTriggerPlug01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nice_export_clap!(FtsTrigger);
nice_export_vst3!(FtsTrigger);
