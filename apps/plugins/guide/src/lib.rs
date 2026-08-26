//! FTS Guide — CLAP/VST3 click-track generator plugin.
//!
//! A thin nice-plug shell over the [`session_guide`] engine (the portable
//! port of the legacy fts-guide REAPER plugin). The engine is a synchronous
//! processing core driven by a per-block [`BlockClock`]; this shell builds
//! that clock from the HOST transport (tempo, time signature, playhead) each
//! block, so the click follows the DAW: accented downbeats, quarter-note
//! ticks, silence when the transport is stopped, and correct re-anchoring on
//! seeks/loops (the engine detects discontinuities itself).
//!
//! No audio input; stereo generator output. Click sounds are the engine's
//! synthesized defaults (`SampleBank::synthesize_defaults`) — audible with
//! zero assets on disk.
//!
//! ## Two ways to drive it
//!
//! The **Source** parameter picks one, and they are mutually exclusive on
//! purpose — running both double-triggers every beat, because a stamped
//! guide track carries the same clicks and cues the internal grid would
//! generate.
//!
//! - **Host Transport** — the engine derives the click grid from the
//!   host's tempo and time signature. Self-contained, nothing to route.
//!   Count-in and section cues stay off in this mode: they need a
//!   `CueSchedule` built from song *sections*, and the transport alone
//!   carries no section information.
//! - **MIDI** — the plugin plays incoming notes and nothing else. This is
//!   what `session::guide`'s generate-guide-tracks action produces: the
//!   click grid, count-in and section announcements as notes on the
//!   Click / Count / Guide tracks. One table maps notes to sounds in both
//!   directions (`session_guide::midi`), so a track FTS stamps is a track
//!   this plugin reads back. Count and section cues work here, because
//!   the notes *are* the schedule — editable and movable in the piano
//!   roll.
//!
//! Section-cue audio still has to be in the bank to be heard: the
//! synthesized defaults cover clicks and count ticks, but guide
//! announcements come from real recorded samples (`load_guide_dir`) or
//! pre-rendered TTS (`session_guide::CueBank`). A Guide note with no
//! sample behind it is silently skipped. Loading those is a follow-up.
//!
//! GUI is deliberately absent (headless, host-generic params), matching the
//! other apps/plugins shells; the nice-plug-dioxus editor is a follow-up.

use nice_plug::prelude::*;
use std::sync::Arc;

use session_guide::midi::{note_names, trigger_for_midi_note};
use session_guide::{BlockClock, GuideConfig, GuideEngine, TriggerSource};

const PLUGIN_NAME: &str = "FTS Guide";

/// dB → linear gain.
fn db_to_gain(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

// ── Parameters ────────────────────────────────────────────────────────────

/// What drives the engine. Mirrors [`TriggerSource`] as a host-visible
/// enum param.
#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Source {
    /// Follow the host's tempo and time signature. Click only.
    #[id = "transport"]
    #[name = "Host Transport"]
    HostTransport,
    /// Play incoming MIDI notes — click, count and section cues.
    #[id = "midi"]
    #[name = "MIDI"]
    Midi,
}

impl From<Source> for TriggerSource {
    fn from(source: Source) -> Self {
        match source {
            Source::HostTransport => TriggerSource::HostTransport,
            Source::Midi => TriggerSource::Midi,
        }
    }
}

#[derive(Params)]
pub struct GuideParams {
    /// Where triggers come from.
    #[id = "source"]
    pub source: EnumParam<Source>,
    /// Count-voice volume. Only audible in MIDI mode.
    #[id = "count_vol"]
    pub count_db: FloatParam,
    /// Section-announcement volume. Only audible in MIDI mode.
    #[id = "guide_vol"]
    pub guide_db: FloatParam,
    /// Click (quarter-note tick) volume.
    #[id = "click_vol"]
    pub click_db: FloatParam,
    /// Downbeat accent volume (beat 1 of each measure).
    #[id = "accent_vol"]
    pub accent_db: FloatParam,
}

impl Default for GuideParams {
    fn default() -> Self {
        Self {
            source: EnumParam::new("Source", Source::HostTransport),
            count_db: FloatParam::new(
                "Count",
                0.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 6.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            guide_db: FloatParam::new(
                "Guide",
                0.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 6.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            click_db: FloatParam::new(
                "Click",
                0.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 6.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            accent_db: FloatParam::new(
                "Accent",
                0.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 6.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsGuide {
    params: Arc<GuideParams>,
    engine: GuideEngine,
    sample_rate: f64,

    /// Unity-gain copies of the synthesized beat/accent PCM (mono). The
    /// engine has ONE `click_gain` covering both the beat tick and the
    /// measure accent, so per-sound volume is applied by rescaling the
    /// bank's PCM in place from these bases (fixed length — no allocation
    /// on the audio thread; `click_gain` stays 1.0).
    base_beat: Vec<f32>,
    base_accent: Vec<f32>,
    /// Last gains written into the bank, to skip the rescan when unchanged.
    applied_click_db: f32,
    applied_accent_db: f32,
}

impl Default for FtsGuide {
    fn default() -> Self {
        // v0: click + downbeat accent only. Count-in and section guide need
        // a section-built CueSchedule (see crate docs) — hold them off so an
        // accidentally installed schedule can't speak.
        let config = GuideConfig {
            enable_count: false,
            enable_guide: false,
            enable_eighth: false,
            enable_sixteenth: false,
            enable_triplet: false,
            ..Default::default()
        };
        Self {
            params: Arc::new(GuideParams::default()),
            engine: GuideEngine::new(config),
            sample_rate: 48_000.0,
            base_beat: Vec::new(),
            base_accent: Vec::new(),
            applied_click_db: f32::NAN,
            applied_accent_db: f32::NAN,
        }
    }
}

impl FtsGuide {
    /// Apply the current volume params by rescaling the bank PCM in place.
    fn sync_params(&mut self) {
        // Count and guide have real gain knobs on the engine, so they just
        // get set. Click doesn't: the engine has ONE `click_gain` covering
        // both the beat tick and the measure accent, which is why those two
        // are applied by rescaling the bank PCM below.
        self.engine.config.count_gain = db_to_gain(self.params.count_db.value());
        self.engine.config.guide_gain = db_to_gain(self.params.guide_db.value());

        let click_db = self.params.click_db.value();
        let accent_db = self.params.accent_db.value();
        if click_db == self.applied_click_db && accent_db == self.applied_accent_db {
            return;
        }
        let bank = self.engine.bank_mut();
        if let Some(beat) = bank.beat.as_mut() {
            let g = db_to_gain(click_db);
            for (dst, src) in beat.data.iter_mut().flatten().zip(self.base_beat.iter()) {
                *dst = src * g;
            }
        }
        if let Some(accent) = bank.measure_accent.as_mut() {
            let g = db_to_gain(accent_db);
            for (dst, src) in accent
                .data
                .iter_mut()
                .flatten()
                .zip(self.base_accent.iter())
            {
                *dst = src * g;
            }
        }
        self.applied_click_db = click_db;
        self.applied_accent_db = accent_db;
    }
}

impl Plugin for FtsGuide {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Generator: no audio input, stereo click bus out.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    /// Note events in — this is how a stamped guide track drives the
    /// plugin. Basic is enough: only note-ons matter, and only their
    /// pitch and timing.
    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;

    // No editor yet — the host shows its generic parameter UI.
    type Editor = ();
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    /// Label the guide layout in the host's piano roll: "Chorus" rather
    /// than "C6", "Count 1" rather than "C5", "Click: Accent" rather than
    /// "C4".
    ///
    /// Same table the stamper and the MIDI input use, so what REAPER
    /// shows on a note is exactly what this plugin will play for it.
    fn note_names(&self) -> Vec<NoteName> {
        note_names()
            .into_iter()
            .map(|(note, name)| NoteName::new(note, name))
            .collect()
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;

        // (Re)build the click PCM at the render rate. Synthesized defaults
        // only fill EMPTY slots, so clear first for a rate change.
        let bank = self.engine.bank_mut();
        bank.beat = None;
        bank.measure_accent = None;
        bank.synthesize_defaults(buffer_config.sample_rate as u32);
        self.base_beat = bank
            .beat
            .as_ref()
            .map(|s| s.data.iter().flatten().copied().collect())
            .unwrap_or_default();
        self.base_accent = bank
            .measure_accent
            .as_ref()
            .map(|s| s.data.iter().flatten().copied().collect())
            .unwrap_or_default();
        // Force a rescale on the first block.
        self.applied_click_db = f32::NAN;
        self.applied_accent_db = f32::NAN;
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync_params();

        // Generator: the output buffer is not guaranteed silent.
        let output = buffer.as_slice();
        for ch in output.iter_mut() {
            ch.fill(0.0);
        }

        let source = self.params.source.value();
        self.engine.config.source = source.into();
        // Count-in and section cues only have a schedule to fire from in
        // MIDI mode; under the host transport there are no sections, so
        // leaving them on would just be dead config.
        let midi = source == Source::Midi;
        self.engine.config.enable_count = midi;
        self.engine.config.enable_guide = midi;

        // Drain this block's notes. `timing` is the event's offset within
        // the block, which is passed straight through so a cue lands where
        // it was played instead of at the block boundary — quantising to
        // the block audibly flams against a click.
        while let Some(event) = context.next_event() {
            if let NoteEvent::NoteOn { timing, note, .. } = event {
                if let Some(trigger) = trigger_for_midi_note(note) {
                    self.engine.trigger(timing as usize, trigger);
                }
            }
        }

        let t = context.transport();
        let clock = BlockClock {
            playing: t.playing,
            pos_seconds: t.pos_seconds().unwrap_or(0.0),
            pos_beats: t.pos_beats().unwrap_or(0.0),
            tempo_bpm: t.tempo.unwrap_or(120.0),
            time_sig_num: t.time_sig_numerator.unwrap_or(4).max(1) as u32,
            time_sig_den: t.time_sig_denominator.unwrap_or(4).max(1) as u32,
            sample_rate: self.sample_rate,
        };

        // The engine flushes voice tails on stopped transport and re-anchors
        // its beat grid on seeks — render every block, playing or not.
        let mut channels = output.iter_mut();
        if let (Some(left), Some(right)) = (channels.next(), channels.next()) {
            self.engine.render_stereo(left, right, &clock);
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsGuide {
    const CLAP_ID: &'static str = "com.fasttrackstudio.guide";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Click / count-in / guide-cue generator that follows the host transport");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Utility,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsGuide {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsGuidePlugn001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Tools];
}

nice_export_clap!(FtsGuide);
nice_export_vst3!(FtsGuide);
