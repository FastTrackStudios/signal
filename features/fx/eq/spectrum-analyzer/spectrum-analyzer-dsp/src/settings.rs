//! Pro-Q 4 analyzer settings model.
//!
//! Every user-facing analyzer control maps to a field here. Values and ranges
//! follow the FabFilter Pro-Q 4 help: Resolution 1024/2048/4096/8192, Range
//! 60/90/120 dB (default 90), Tilt around 1 kHz in dB/oct (default 4.5).

/// FFT resolution. Higher = more low-frequency detail but slower update rate
/// (more incoming samples needed per spectrum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Resolution {
    /// 1024-point FFT.
    Low,
    /// 2048-point FFT.
    #[default]
    Medium,
    /// 4096-point FFT.
    High,
    /// 8192-point FFT.
    Maximum,
}

impl Resolution {
    /// FFT size in samples.
    pub fn fft_size(self) -> usize {
        match self {
            Resolution::Low => 1024,
            Resolution::Medium => 2048,
            Resolution::High => 4096,
            Resolution::Maximum => 8192,
        }
    }

    /// Largest FFT size across all resolutions — used to size the audio-thread
    /// ring once so it never has to reallocate when the user changes resolution.
    pub const MAX_FFT_SIZE: usize = 8192;
}

/// Vertical range of the analyzer display, in dB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Range {
    /// 60 dB span.
    Db60,
    /// 90 dB span (Pro-Q 4 default).
    #[default]
    Db90,
    /// 120 dB span.
    Db120,
}

impl Range {
    /// Span in dB.
    pub fn db(self) -> f32 {
        match self {
            Range::Db60 => 60.0,
            Range::Db90 => 90.0,
            Range::Db120 => 120.0,
        }
    }
}

/// Release speed of the spectrum, as a falloff time in seconds. A fast release
/// shows dynamic changes clearly; a slow release lingers for inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Speed {
    /// ~1.5 s release.
    Slow,
    /// ~0.8 s release.
    #[default]
    Medium,
    /// ~0.4 s release.
    Fast,
    /// ~0.2 s release.
    VeryFast,
}

impl Speed {
    /// Release time constant in seconds (larger = slower fall = calmer, nicer
    /// looking display). These lean slow on purpose — community-favorite
    /// analyzer settings prize a smooth, settled spectrum over a twitchy one.
    pub fn release_seconds(self) -> f32 {
        match self {
            Speed::Slow => 2.0,
            Speed::Medium => 1.0,
            Speed::Fast => 0.5,
            Speed::VeryFast => 0.25,
        }
    }
}

/// Attack time constant (seconds) for the display ballistics. Short so
/// transients still read, but not instant — an instant attack snaps to every
/// FFT frame and looks like noise.
pub const ATTACK_SECONDS: f32 = 0.05;

/// Which input feeds a spectrum slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpectrumSlot {
    /// Signal before the EQ.
    PreEq,
    /// Signal after the EQ.
    PostEq,
    /// Sidechain input.
    Sidechain,
    /// Spectrum received from another plugin instance (SC/Ext).
    External,
}

/// How a frame magnitude is reduced to a single value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MagType {
    /// Peak magnitude.
    #[default]
    Peak,
    /// RMS magnitude.
    Rms,
}

/// Channel reduction applied before analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StereoType {
    /// Average of both channels.
    #[default]
    Stereo,
    /// Left channel only.
    Left,
    /// Right channel only.
    Right,
    /// Mid (L+R).
    Mid,
    /// Side (L-R).
    Side,
}

/// Full analyzer settings, shared between the audio and UI threads (the UI owns
/// it; only the resolution/speed/tilt drive the UI-side `tick`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnalyzerSettings {
    pub resolution: Resolution,
    pub speed: Speed,
    /// Tilt slope in dB/octave around 1 kHz.
    pub tilt_db_per_oct: f32,
    pub range: Range,
    /// Hold the spectrum and accumulate the running maximum while true.
    pub freeze: bool,
    /// Show the pre-EQ spectrum overlay.
    pub show_pre: bool,
    /// Show the post-EQ spectrum overlay.
    pub show_post: bool,
    /// Show sidechain / external spectrum.
    pub show_external: bool,
    /// Highlight pre/post collision regions in red.
    pub show_collisions: bool,
    /// Octave width of the display smoothing (0 = none).
    pub smoothing_oct: f32,
    pub mag_type: MagType,
    pub stereo_type: StereoType,
}

impl Default for AnalyzerSettings {
    fn default() -> Self {
        Self {
            resolution: Resolution::default(),
            speed: Speed::default(),
            tilt_db_per_oct: 4.5,
            range: Range::default(),
            freeze: false,
            show_pre: true,
            show_post: true,
            show_external: false,
            show_collisions: false,
            // Base smoothing on by default (~1/4 octave): clean, "nice" curve
            // while keeping enough detail to read peaks. 0.0 = off (surgical).
            smoothing_oct: 0.25,
            mag_type: MagType::default(),
            stereo_type: StereoType::default(),
        }
    }
}
