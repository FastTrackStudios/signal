//! FTS Trigger — CLAP/VST3 drum-trigger plugin.
//!
//! A thin nice-plug shell over the [`trigger`] engine's
//! [`trigger::TriggerDetector`]: the input is mono-summed for detection,
//! passes through untouched, and every detected hit becomes a sample-accurate
//! `NoteOn` (channel 0, configurable note) with a matching `NoteOff` a few ms
//! later — scheduled across block boundaries when needed. `MIDI_OUTPUT =
//! MidiConfig::Basic` declares the note-out port (the mirror of
//! `signal-sampler-clap`'s note *input*).
//!
//! Listen mode mutes the passthrough and replaces it with a short 1 kHz click
//! per hit (scaled by velocity) for threshold tuning.
//!
//! GUI is deliberately absent for now (headless, host-generic params),
//! matching `level-plugin`; the nice-plug-dioxus editor is a follow-up.

use nice_plug::prelude::*;
use std::sync::Arc;

use trigger::TriggerDetector;

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
    /// Dynamics margin above the recent floor (ring-out rejection).
    #[id = "sensitivity"]
    pub sensitivity_db: FloatParam,
    /// Retrigger-guard window.
    #[id = "retrigger"]
    pub retrigger_ms: FloatParam,
    /// MIDI note to emit (default 36 = C1 kick).
    #[id = "note"]
    pub note: IntParam,
    /// Peak level mapped to velocity 1.
    #[id = "vel_min"]
    pub vel_min_db: FloatParam,
    /// Peak level mapped to velocity 127.
    #[id = "vel_max"]
    pub vel_max_db: FloatParam,
    /// Mute passthrough and click on every hit (threshold tuning).
    #[id = "listen"]
    pub listen: BoolParam,
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
            sensitivity_db: FloatParam::new(
                "Sensitivity",
                6.0,
                FloatRange::Linear { min: 0.0, max: 24.0 },
            )
            .with_unit(" dB")
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
            vel_min_db: FloatParam::new(
                "Vel Min",
                -40.0,
                FloatRange::Linear { min: -60.0, max: 0.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            vel_max_db: FloatParam::new(
                "Vel Max",
                -6.0,
                FloatRange::Linear { min: -60.0, max: 0.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            listen: BoolParam::new("Listen", false),
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

pub struct FtsTrigger {
    params: Arc<TriggerParams>,
    detector: TriggerDetector,
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
            detector: TriggerDetector::new(48_000.0),
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
    /// Push the current params into the detector (no allocation).
    fn sync_params(&mut self) {
        self.detector
            .set_threshold_db(self.params.threshold_db.value());
        self.detector
            .set_dynamics_db(self.params.sensitivity_db.value());
        self.detector
            .set_retrigger_ms(self.params.retrigger_ms.value());
        self.detector.set_velocity_range_db(
            self.params.vel_min_db.value(),
            self.params.vel_max_db.value(),
        );
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
        self.detector.set_sample_rate(self.sample_rate);
        self.note_len_samples =
            ((NOTE_LEN_MS * 0.001 * self.sample_rate) as u32).max(1);
        self.click_step =
            2.0 * std::f32::consts::PI * CLICK_FREQ_HZ / self.sample_rate;
        true
    }

    fn reset(&mut self) {
        self.detector.reset();
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
        self.sync_params();

        let block_len = buffer.samples() as u32;
        let note = self.params.note.value().clamp(0, 127) as u8;
        let listen = self.params.listen.value();
        let click_len =
            ((CLICK_LEN_MS * 0.001 * self.sample_rate) as u32).max(1);

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
            // Mono sum for detection; the audio itself is untouched.
            let mut sum = 0.0f32;
            let mut channels = 0u32;
            for sample in frame.iter_mut() {
                sum += *sample;
                channels += 1;
            }
            let mono = if channels > 0 { sum / channels as f32 } else { 0.0 };

            if let Some(hit) = self.detector.process_sample(mono) {
                // The detector reports at the end of its ~1.5 ms capture
                // window; place the note back at the onset sample (clamped
                // to the block start if the onset was in the previous block).
                let onset = (i as u32).saturating_sub(hit.latency_samples);
                let velocity = hit.velocity as f32 / 127.0;
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
