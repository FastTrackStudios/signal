//! FTS Delay — CLAP/VST3 stereo delay plugin.
//!
//! A thin nice-plug shell over the [`delay`] facade's [`DelayChain`] — the
//! full stereo processor (per-side [`delay::DelayEngine`]s with runtime style
//! switching, ducking, mix). This shell exposes the CORE knob set: time L/R
//! (free-running ms; tempo sync is a follow-up), link, feedback, style,
//! tone (feedback hi-cut), drive, wow/flutter wobble, duck amount, and mix.
//! The chain's deeper surface (ping-pong, diffusion, accent/groove/feel,
//! heads, per-style extras) stays at its defaults until a richer shell lands.
//!
//! GUI is deliberately absent for now (headless, host-generic params),
//! matching level-plugin; the nice-plug-dioxus editor is a follow-up.

use nice_plug::prelude::*;
use std::sync::Arc;

use audiocore_dsp::{AudioConfig, Processor};
use delay::{DelayChain, DelayStyle};

const PLUGIN_NAME: &str = "FTS Delay";

/// Delay-time range in ms. The engines pre-allocate 5 s lines, but every
/// style clamps to at most 2.5 s ([`DelayStyle::time_range_ms`]), so the
/// host range mirrors the widest usable span.
const TIME_MIN_MS: f32 = 20.0;
const TIME_MAX_MS: f32 = 2500.0;

// ── Parameters ────────────────────────────────────────────────────────────

/// The delay-style selector, mirrored onto [`DelayStyle`].
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleParam {
    #[name = "Tape"]
    Tape,
    #[name = "Digital"]
    Clean,
    #[name = "BBD"]
    Bbd,
    #[name = "Lo-Fi"]
    LoFi,
    #[name = "Shimmer"]
    Shimmer,
    #[name = "Reverse"]
    Reverse,
    #[name = "Pitch"]
    Pitch,
    #[name = "Rhythm"]
    Rhythm,
    #[name = "Drum"]
    Drum,
    #[name = "Oil Can"]
    OilCan,
    #[name = "MultiTap"]
    MultiTap,
    #[name = "Spectral"]
    Spectral,
    #[name = "Filter"]
    Filter,
}

impl From<StyleParam> for DelayStyle {
    fn from(value: StyleParam) -> Self {
        match value {
            StyleParam::Tape => DelayStyle::Tape,
            StyleParam::Clean => DelayStyle::Clean,
            StyleParam::Bbd => DelayStyle::Bbd,
            StyleParam::LoFi => DelayStyle::LoFi,
            StyleParam::Shimmer => DelayStyle::Shimmer,
            StyleParam::Reverse => DelayStyle::Reverse,
            StyleParam::Pitch => DelayStyle::Pitch,
            StyleParam::Rhythm => DelayStyle::Rhythm,
            StyleParam::Drum => DelayStyle::Drum,
            StyleParam::OilCan => DelayStyle::OilCan,
            StyleParam::MultiTap => DelayStyle::MultiTap,
            StyleParam::Spectral => DelayStyle::Spectral,
            StyleParam::Filter => DelayStyle::Filter,
        }
    }
}

#[derive(Params)]
pub struct DelayParams {
    /// Delay character (tape / digital / BBD / …).
    #[id = "style"]
    pub style: EnumParam<StyleParam>,
    /// Left delay time in ms (free-running; styles clamp to their range).
    #[id = "time_l"]
    pub time_l: FloatParam,
    /// Right delay time in ms (ignored while Link is on).
    #[id = "time_r"]
    pub time_r: FloatParam,
    /// Link R to L (single-knob operation).
    #[id = "link"]
    pub link: BoolParam,
    /// Regeneration amount (0 = single repeat, 1 = self-oscillation edge).
    #[id = "feedback"]
    pub feedback: FloatParam,
    /// Feedback-loop hi-cut in Hz — repeats darken as it comes down.
    #[id = "tone"]
    pub tone: FloatParam,
    /// Saturation drive in the loop (0 = clean).
    #[id = "drive"]
    pub drive: FloatParam,
    /// Wow depth — slow tape-speed wobble (0 = off).
    #[id = "wow"]
    pub wow: FloatParam,
    /// Flutter depth — fast tape-speed wobble (0 = off).
    #[id = "flutter"]
    pub flutter: FloatParam,
    /// Duck amount: wet pulls down while the dry input is playing (0 = off).
    #[id = "duck"]
    pub duck: FloatParam,
    /// Dry/wet mix.
    #[id = "mix"]
    pub mix: FloatParam,
}

impl Default for DelayParams {
    fn default() -> Self {
        Self {
            style: EnumParam::new("Style", StyleParam::Tape),
            time_l: FloatParam::new(
                "Time L",
                250.0,
                FloatRange::Skewed {
                    min: TIME_MIN_MS,
                    max: TIME_MAX_MS,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            time_r: FloatParam::new(
                "Time R",
                250.0,
                FloatRange::Skewed {
                    min: TIME_MIN_MS,
                    max: TIME_MAX_MS,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            link: BoolParam::new("Link L/R", true),
            feedback: FloatParam::new(
                "Feedback",
                0.4,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0)),
            tone: FloatParam::new(
                "Tone",
                8000.0,
                FloatRange::Skewed {
                    min: 500.0,
                    max: 20000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            drive: FloatParam::new("Drive", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
            wow: FloatParam::new("Wow", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
            flutter: FloatParam::new(
                "Flutter",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0)),
            duck: FloatParam::new("Duck", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
            mix: FloatParam::new("Mix", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsDelay {
    params: Arc<DelayParams>,
    /// The full stereo chain (inherently stereo — one instance).
    chain: DelayChain,
    sample_rate: f64,
    max_buffer_size: usize,
}

impl Default for FtsDelay {
    fn default() -> Self {
        Self {
            params: Arc::new(DelayParams::default()),
            chain: DelayChain::new(),
            sample_rate: 48_000.0,
            max_buffer_size: 512,
        }
    }
}

impl FtsDelay {
    /// Push the current params into the chain and refresh coefficients.
    ///
    /// `DelayChain::update` only reallocates when the sample rate grows
    /// beyond what `initialize()` already provisioned, so calling it per
    /// block is allocation-free on the audio thread.
    fn sync_params(&mut self) {
        let p = &self.params;
        let c = &mut self.chain;

        c.set_style(DelayStyle::from(p.style.value()));

        // Time — free-running ms; Link mirrors L onto R.
        let time_l = p.time_l.value() as f64;
        c.delay_l.time_ms = time_l;
        c.delay_r.time_ms = if p.link.value() {
            time_l
        } else {
            p.time_r.value() as f64
        };

        let fb = p.feedback.value() as f64;
        c.delay_l.feedback = fb;
        c.delay_r.feedback = fb;

        // Tone = feedback-loop hi-cut.
        let tone = p.tone.value() as f64;
        c.delay_l.hicut_freq = tone;
        c.delay_r.hicut_freq = tone;

        let drive = p.drive.value() as f64;
        c.delay_l.drive = drive;
        c.delay_r.drive = drive;

        // Wobble (rates stay at the engine defaults).
        let wow = p.wow.value() as f64;
        c.delay_l.wow_depth = wow;
        c.delay_r.wow_depth = wow;
        let flutter = p.flutter.value() as f64;
        c.delay_l.flutter_depth = flutter;
        c.delay_r.flutter_depth = flutter;

        // Ducking: the amount knob doubles as the enable (0 = off).
        let duck = p.duck.value() as f64;
        c.ducking_enabled = duck > 0.001;
        c.ducker.amount = duck;
        c.ducker.threshold = 0.1;

        c.mix = p.mix.value() as f64;

        c.update(AudioConfig {
            sample_rate: self.sample_rate,
            max_buffer_size: self.max_buffer_size,
        });
    }
}

impl Plugin for FtsDelay {
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
        self.max_buffer_size = buffer_config.max_buffer_size as usize;
        // First update at the real sample rate provisions every delay line
        // (the engines size for their 5 s maximum) so process() never
        // allocates.
        self.sync_params();
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

        // The chain processes f64 stereo slices; bridge from the host's f32
        // buffers in fixed stack chunks (no heap allocation).
        const CHUNK: usize = 128;
        let num_samples = buffer.samples();
        let channels = buffer.channels();
        if channels < 2 {
            return ProcessStatus::Normal;
        }

        let mut offset = 0;
        while offset < num_samples {
            let len = (num_samples - offset).min(CHUNK);

            let mut left = [0.0f64; CHUNK];
            let mut right = [0.0f64; CHUNK];

            {
                let slices = buffer.as_slice();
                for i in 0..len {
                    left[i] = slices[0][offset + i] as f64;
                    right[i] = slices[1][offset + i] as f64;
                }
            }

            self.chain.process(&mut left[..len], &mut right[..len]);

            {
                let slices = buffer.as_slice();
                for i in 0..len {
                    slices[0][offset + i] = left[i] as f32;
                    slices[1][offset + i] = right[i] as f32;
                }
            }

            offset += len;
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsDelay {
    const CLAP_ID: &'static str = "com.fasttrackstudio.delay";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Stereo delay: tape/digital/BBD/lo-fi styles with wobble, saturation, and ducking");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Delay,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsDelay {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsDelayPlugn001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Delay];
}

nice_export_clap!(FtsDelay);
nice_export_vst3!(FtsDelay);
