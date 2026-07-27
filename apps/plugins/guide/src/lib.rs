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
//! ## v0 scope — click only
//!
//! Count-in voices and spoken/TTS section cues are NOT wired here. Both are
//! driven by the engine's [`session_guide::CueSchedule`], which is built from
//! song *sections* with absolute song times ([`session_guide::GuideSection`] +
//! [`session_guide::GuideSongTiming`], via `GuideEngine::set_sections`) — the
//! host transport alone carries no section information, so there is nothing
//! to schedule against. The engine sides are `config.enable_count` /
//! `config.enable_guide` (held `false` here). Follow-ups:
//!
//! - **Section source**: load song sections from a setlist/song file (styx
//!   library or a `session_proto::Song` export) via a file-path param or a
//!   nice-plug-dioxus editor, then call `engine.set_sections(...)` — count-in
//!   and section-cue chimes light up with no further engine work.
//! - **TTS cues**: pre-rendered wavs from `session_guide::CueBank` (TTS is
//!   not realtime-safe; cues are cached by text hash at setlist-build time)
//!   loaded into the bank with `insert_guide`.
//!
//! GUI is deliberately absent (headless, host-generic params), matching the
//! other apps/plugins shells; the nice-plug-dioxus editor is a follow-up.

use nice_plug::prelude::*;
use std::sync::Arc;

use session_guide::{BlockClock, GuideConfig, GuideEngine};

const PLUGIN_NAME: &str = "FTS Guide";

/// dB → linear gain.
fn db_to_gain(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

// ── Parameters ────────────────────────────────────────────────────────────

#[derive(Params)]
pub struct GuideParams {
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
            click_db: FloatParam::new(
                "Click",
                0.0,
                FloatRange::Linear { min: -60.0, max: 6.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            accent_db: FloatParam::new(
                "Accent",
                0.0,
                FloatRange::Linear { min: -60.0, max: 6.0 },
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

    /// Generator: no input, stereo click bus out.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
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
