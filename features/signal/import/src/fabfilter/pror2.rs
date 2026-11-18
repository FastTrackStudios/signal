//! Pro-R 2 → FTS-Reverb translation.
//!
//! Decodes the 136-float [`FFBS`](super::ffbs) parameter vector a Pro-R 2
//! instance writes into a project, and emits the equivalent
//! `builtin::NativeReverb` parameters by name.
//!
//! # Where the field map comes from
//!
//! Two independent sources, which agree:
//!
//! 1. Pro-R 2's `.ffp` presets are the **text** INI format, and their
//!    `[Parameters]` section lists all 136 parameters by name in binary order.
//! 2. The plugin itself, queried through `signal-plugin-host`, reports the
//!    same 136 parameters with their ranges and display text.
//!
//! The second source settled two things guessing would have got wrong:
//! `Space` is the reverb **time** (0.5 displays as "2.500 sec"), and
//! `Predelay` is a **normalized** 0–1 control, not a value in seconds.
//!
//! # Not the same as Pro-R 1
//!
//! The `Pro-R 2` preset folder also contains Pro-R **1** presets — binary,
//! magic `FPRr`, version 4, 85 floats. Pro-R 2 loads them for compatibility.
//! They are a different layout and are **not** handled here; there are no
//! text Pro-R 1 presets to name those 85 fields from.
//!
//! (Note for [`super::registry`]: it lists Pro-R 2 as a binary format with
//! signature `FRvb`/`FR2p`. Pro-R 2's own presets are text with `FR2p`;
//! `FPRr` is Pro-R 1.)
//!
//! See `spec/project-state-formats.md`.

use super::ffbs::FfbsState;

/// Floats in a Pro-R 2 state vector.
pub const PARAM_COUNT: usize = 136;

/// Global parameter offsets, named from Pro-R 2's own parameter list.
mod field {
    pub const SPACE: usize = 0;
    pub const DECAY_RATE: usize = 1;
    pub const DISTANCE: usize = 2;
    pub const BRIGHTNESS: usize = 3;
    pub const STYLE: usize = 4;
    pub const CHARACTER: usize = 5;
    pub const THICKNESS: usize = 6;
    pub const STEREO_WIDTH: usize = 7;
    pub const DUCKING: usize = 8;
    pub const MIX: usize = 9;
    pub const PREDELAY: usize = 16;

    /// First `Decay EQ Band 1 Used`. Six bands of seven fields.
    pub const DECAY_EQ_BASE: usize = 19;
    pub const DECAY_EQ_STRIDE: usize = 7;
    /// First `Post EQ Band 1 Used`. Six bands of nine fields.
    pub const POST_EQ_BASE: usize = 61;
    pub const POST_EQ_STRIDE: usize = 9;
    pub const EQ_BANDS: usize = 6;
}

/// Pro-R 2's three voicings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// Index 0 — the plugin reports this one as "Modern".
    Modern,
    /// Index 1.
    Style1,
    /// Index 2.
    Style2,
}

impl Style {
    fn from_raw(v: f32) -> Self {
        match v.round() as i32 {
            1 => Self::Style1,
            2 => Self::Style2,
            _ => Self::Modern,
        }
    }
}

/// One Decay EQ band — Pro-R's frequency-dependent decay control.
///
/// `rate` is a decay **multiplier** for the band, not a gain: the plugin
/// displays it as a percentage (`-0.415` reads as "75.00%"), so a negative
/// rate shortens that band's tail and a positive one lengthens it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecayEqBand {
    pub used: bool,
    pub enabled: bool,
    /// Centre frequency in Hz, decoded from the stored `log2(Hz)`.
    pub freq_hz: f32,
    /// Decay-rate multiplier, −3..1 in the plugin's units.
    pub rate: f32,
    pub q: f32,
    /// 0..3; the plugin names 1 as "Low Shelf".
    pub shape: u32,
}

impl DecayEqBand {
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.used && self.enabled
    }
}

/// One Post EQ band — an ordinary EQ after the reverb.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PostEqBand {
    pub used: bool,
    pub enabled: bool,
    /// Centre frequency in Hz, decoded from the stored `log2(Hz)`.
    pub freq_hz: f32,
    pub gain_db: f32,
    pub q: f32,
    pub shape: u32,
    pub slope: u32,
    /// Raw Pro-Q-style placement selector; see [`super::proq4::Placement`].
    pub stereo_placement: u32,
}

impl PostEqBand {
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.used && self.enabled
    }
}

/// A decoded Pro-R 2 instance.
#[derive(Debug, Clone, PartialEq)]
pub struct ProR2 {
    /// Reverb time control, 0–1 (0.5 reads as 2.5 s on the plugin).
    pub space: f32,
    /// Global decay-rate multiplier, −1..1.
    pub decay_rate: f32,
    /// Early/late balance, 0–1.
    pub distance: f32,
    /// Tone control, 0–1.
    pub brightness: f32,
    pub style: Style,
    /// Modulation/chorusing amount, 0–1.
    pub character: f32,
    /// Low-end weight, −1..1.
    pub thickness: f32,
    /// Stereo width, 0–1.2.
    pub stereo_width: f32,
    /// Ducking depth in dB, 0–48.
    pub ducking_db: f32,
    /// Wet/dry mix as a **percentage**, 0–100.
    pub mix_percent: f32,
    /// Pre-delay as the plugin's normalized 0–1 control. Deliberately not
    /// converted to milliseconds — see [`ProR2::predelay_ms`].
    pub predelay_normalized: f32,
    pub decay_eq: Vec<DecayEqBand>,
    pub post_eq: Vec<PostEqBand>,
    pub preset_name: Option<String>,
}

impl ProR2 {
    /// Pre-delay in milliseconds — **not implemented**, deliberately.
    ///
    /// The control is normalized and its curve is non-linear: the one
    /// measured point is `0.0645 → 0.645 ms`, which no obvious mapping
    /// reproduces (a linear 0–1 → 0–500 ms would give 32 ms). Rather than
    /// ship a number that is confidently wrong, this returns `None` until the
    /// curve is calibrated by sweeping the real plugin through the analyzer's
    /// render bridge.
    ///
    /// [`ProR2::predelay_normalized`] carries the raw value meanwhile.
    #[must_use]
    pub fn predelay_ms(&self) -> Option<f32> {
        None
    }

    /// The decay-EQ bands that are actually shaping the tail.
    pub fn active_decay_eq(&self) -> impl Iterator<Item = &DecayEqBand> {
        self.decay_eq.iter().filter(|b| b.is_active())
    }

    /// The post-EQ bands that are actually shaping the output.
    pub fn active_post_eq(&self) -> impl Iterator<Item = &PostEqBand> {
        self.post_eq.iter().filter(|b| b.is_active())
    }
}

/// Errors from decoding a Pro-R 2 state vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProR2Error {
    /// The float vector is not long enough to be a Pro-R 2 state.
    UnexpectedParamCount { got: usize },
    /// The blob is a Pro-R **1** preset (`FPRr`, 85 floats), whose layout is
    /// different and unmapped.
    ProR1NotSupported,
}

impl std::fmt::Display for ProR2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedParamCount { got } => write!(
                f,
                "expected {PARAM_COUNT} floats in a Pro-R 2 state, found {got}"
            ),
            Self::ProR1NotSupported => {
                write!(f, "this is a Pro-R 1 preset (FPRr); its layout is unmapped")
            }
        }
    }
}

impl std::error::Error for ProR2Error {}

/// Decode a parsed [`FfbsState`] as a Pro-R 2 instance.
///
/// # Errors
///
/// Returns [`ProR2Error::UnexpectedParamCount`] if the state vector is shorter than
/// [`PARAM_COUNT`], or [`ProR2Error::ProR1NotSupported`] if the blob is a Pro-R 1 preset.
pub fn decode(state: &FfbsState) -> Result<ProR2, ProR2Error> {
    if state.metadata.signature == "FPRr" {
        return Err(ProR2Error::ProR1NotSupported);
    }
    let p = &state.params;
    if p.len() < PARAM_COUNT {
        return Err(ProR2Error::UnexpectedParamCount { got: p.len() });
    }

    let hz = |v: f32| v.exp2().clamp(1.0, 40_000.0);

    let decay_eq = (0..field::EQ_BANDS)
        .map(|i| {
            let b =
                &p[field::DECAY_EQ_BASE + i * field::DECAY_EQ_STRIDE..][..field::DECAY_EQ_STRIDE];
            DecayEqBand {
                used: b[0] != 0.0,
                enabled: b[1] != 0.0,
                freq_hz: hz(b[2]),
                rate: b[3],
                q: b[4],
                shape: b[5].max(0.0) as u32,
            }
        })
        .collect();

    let post_eq = (0..field::EQ_BANDS)
        .map(|i| {
            let b = &p[field::POST_EQ_BASE + i * field::POST_EQ_STRIDE..][..field::POST_EQ_STRIDE];
            PostEqBand {
                used: b[0] != 0.0,
                enabled: b[1] != 0.0,
                freq_hz: hz(b[2]),
                gain_db: b[3],
                q: b[4],
                shape: b[5].max(0.0) as u32,
                slope: b[6].max(0.0) as u32,
                stereo_placement: b[7].max(0.0) as u32,
            }
        })
        .collect();

    Ok(ProR2 {
        space: p[field::SPACE],
        decay_rate: p[field::DECAY_RATE],
        distance: p[field::DISTANCE],
        brightness: p[field::BRIGHTNESS],
        style: Style::from_raw(p[field::STYLE]),
        character: p[field::CHARACTER],
        thickness: p[field::THICKNESS],
        stereo_width: p[field::STEREO_WIDTH],
        ducking_db: p[field::DUCKING],
        mix_percent: p[field::MIX],
        predelay_normalized: p[field::PREDELAY],
        decay_eq,
        post_eq,
        preset_name: state.metadata.preset_name.clone(),
    })
}

/// Translate a decoded Pro-R 2 instance into `NativeReverb` parameters.
///
/// Returns `(param_name, value)` pairs for `NativeReverb::set_named`.
///
/// What does **not** survive, and why:
///
/// - **Pre-delay** — the source curve is uncalibrated (see
///   [`ProR2::predelay_ms`]), so nothing is emitted rather than something
///   wrong.
/// - **Decay EQ** — Pro-R's per-band decay multipliers are exactly the
///   frequency-dependent decay `NativeReverb` lacks. `low_end` absorbs the
///   broad low-frequency part via `thickness`; the rest needs new DSP.
/// - **Post EQ** — belongs on a `NativeEq` downstream, not on the reverb.
///   [`ProR2::active_post_eq`] exposes the bands for a caller that builds one.
/// - **Distance**, **Stereo Width**, **Ducking** — no counterpart.
#[must_use]
pub fn to_native_reverb_params(r: &ProR2) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    let mut set = |k: &str, v: f64| out.push((k.to_string(), v));

    // Space is the reverb time; Decay Rate scales it. Decay Rate is -1..1
    // around a neutral 0, so fold it in as a +/-50% trim on the time.
    let decay = (r.space as f64 * (1.0 + 0.5 * r.decay_rate as f64)).clamp(0.0, 1.0);
    set("decay", decay);
    set("size", (r.space as f64).clamp(0.0, 1.0));

    set("mix", (r.mix_percent as f64 / 100.0).clamp(0.0, 1.0));

    // Brightness runs the opposite way to damping.
    set("damping", (1.0 - r.brightness as f64).clamp(0.0, 1.0));
    // ...and doubles as a tone tilt, recentred onto NativeReverb's -1..1.
    set("tone", ((r.brightness as f64) - 0.5) * 2.0);

    // Character is Pro-R's modulation/chorusing amount.
    set("modulation", (r.character as f64).clamp(0.0, 1.0));

    // Thickness is -1..1 around neutral; low_end is 0..1 around 0.5.
    set(
        "low_end",
        ((r.thickness as f64) * 0.5 + 0.5).clamp(0.0, 1.0),
    );

    out
}
