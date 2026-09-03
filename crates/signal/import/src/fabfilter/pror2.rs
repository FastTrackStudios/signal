//! Pro-R 2 → FTS-Reverb translation.
//!
//! Decodes the 136-float [`FFBS`](super::ffbs) parameter vector a Pro-R 2
//! instance writes into a project, and emits the equivalent
//! `signal_fx::NativeReverb` parameters by name.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fabfilter::ffbs::FfbsMetadata;

    /// Build a Pro-R 2 state from `(index, value)` overrides on a zeroed vector.
    fn state(overrides: &[(usize, f32)]) -> FfbsState {
        let mut p = vec![0.0f32; PARAM_COUNT];
        for &(i, v) in overrides {
            p[i] = v;
        }
        FfbsState {
            version: 1,
            params: p,
            metadata: FfbsMetadata {
                signature: "FR2p".into(),
                preset_name: Some("Snare Mike 02".into()),
                ..Default::default()
            },
        }
    }

    /// The real Pro-R 2 instance from `02 LORD OF THE FIGHT.RPP`.
    fn project_instance() -> FfbsState {
        state(&[
            (field::SPACE, 0.399),
            (field::DECAY_RATE, -0.998),
            (field::DISTANCE, 0.071),
            (field::BRIGHTNESS, 0.48),
            (field::STYLE, 2.0),
            (field::CHARACTER, 0.583),
            (field::THICKNESS, -0.788),
            (field::STEREO_WIDTH, 0.44),
            (field::DUCKING, 3.852),
            (field::MIX, 100.0),
            (field::PREDELAY, 0.233),
            // Decay EQ band 1: used, enabled, 214 Hz, rate -0.833
            (field::DECAY_EQ_BASE, 1.0),
            (field::DECAY_EQ_BASE + 1, 1.0),
            (field::DECAY_EQ_BASE + 2, 7.746),
            (field::DECAY_EQ_BASE + 3, -0.833),
            (field::DECAY_EQ_BASE + 4, 0.653),
            // Post EQ band 1: used, enabled, 330 Hz, +2.243 dB
            (field::POST_EQ_BASE, 1.0),
            (field::POST_EQ_BASE + 1, 1.0),
            (field::POST_EQ_BASE + 2, 8.369),
            (field::POST_EQ_BASE + 3, 2.243),
            (field::POST_EQ_BASE + 4, 0.508),
        ])
    }

    #[test]
    fn decodes_the_real_project_instance() {
        let r = decode(&project_instance()).unwrap();

        assert_eq!(r.preset_name.as_deref(), Some("Snare Mike 02"));
        assert!((r.space - 0.399).abs() < 1e-6);
        assert_eq!(r.style, Style::Style2);
        assert_eq!(r.mix_percent, 100.0);
        // Mix is a percentage in the source, not a 0-1 fraction.
        assert!(r.mix_percent > 1.0);

        // log2 frequency decode.
        let b1 = &r.decay_eq[0];
        assert!(b1.is_active());
        assert!((b1.freq_hz - 214.7).abs() < 1.0, "got {}", b1.freq_hz);
        assert!((b1.rate + 0.833).abs() < 1e-5);

        let p1 = &r.post_eq[0];
        assert!(p1.is_active());
        assert!((p1.freq_hz - 330.6).abs() < 1.0, "got {}", p1.freq_hz);
        assert_eq!(p1.gain_db, 2.243);
    }

    #[test]
    fn only_band_1_of_each_eq_is_active_in_that_instance() {
        let r = decode(&project_instance()).unwrap();
        assert_eq!(r.active_decay_eq().count(), 1);
        assert_eq!(r.active_post_eq().count(), 1);
        // The remaining slots are present but unused.
        assert_eq!(r.decay_eq.len(), field::EQ_BANDS);
        assert_eq!(r.post_eq.len(), field::EQ_BANDS);
    }

    #[test]
    fn the_two_eq_sections_do_not_overlap() {
        // Decay EQ occupies 19..61 and Post EQ 61..115 — an off-by-one here
        // would silently read the wrong section.
        assert_eq!(
            field::DECAY_EQ_BASE + field::EQ_BANDS * field::DECAY_EQ_STRIDE,
            field::POST_EQ_BASE
        );
        const _: () =
            assert!(field::POST_EQ_BASE + field::EQ_BANDS * field::POST_EQ_STRIDE <= PARAM_COUNT);
    }

    #[test]
    fn mix_is_converted_from_percent() {
        let r = decode(&state(&[(field::MIX, 22.5), (field::SPACE, 0.5)])).unwrap();
        let p = to_native_reverb_params(&r);
        let get = |k: &str| p.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
        assert!((get("mix").unwrap() - 0.225).abs() < 1e-6);
    }

    #[test]
    fn decay_rate_trims_the_space_time() {
        let long = decode(&state(&[(field::SPACE, 0.6), (field::DECAY_RATE, 1.0)])).unwrap();
        let neutral = decode(&state(&[(field::SPACE, 0.6), (field::DECAY_RATE, 0.0)])).unwrap();
        let short = decode(&state(&[(field::SPACE, 0.6), (field::DECAY_RATE, -1.0)])).unwrap();

        let decay = |r: &ProR2| {
            to_native_reverb_params(r)
                .iter()
                .find(|(n, _)| n == "decay")
                .map(|(_, v)| *v)
                .unwrap()
        };
        assert!(decay(&long) > decay(&neutral));
        assert!(decay(&neutral) > decay(&short));
        assert!((decay(&neutral) - 0.6).abs() < 1e-6);
        // And it never leaves the parameter's range.
        assert!((0.0..=1.0).contains(&decay(&long)));
        assert!((0.0..=1.0).contains(&decay(&short)));
    }

    #[test]
    fn brightness_maps_inversely_to_damping() {
        let bright = decode(&state(&[(field::BRIGHTNESS, 1.0)])).unwrap();
        let dark = decode(&state(&[(field::BRIGHTNESS, 0.0)])).unwrap();
        let damping = |r: &ProR2| {
            to_native_reverb_params(r)
                .iter()
                .find(|(n, _)| n == "damping")
                .map(|(_, v)| *v)
                .unwrap()
        };
        assert_eq!(damping(&bright), 0.0);
        assert_eq!(damping(&dark), 1.0);
    }

    #[test]
    fn predelay_is_withheld_rather_than_guessed() {
        let r = decode(&project_instance()).unwrap();
        // The raw control value is preserved...
        assert!((r.predelay_normalized - 0.233).abs() < 1e-6);
        // ...but no millisecond value is invented, and none is emitted.
        assert_eq!(r.predelay_ms(), None);
        assert!(!to_native_reverb_params(&r)
            .iter()
            .any(|(n, _)| n == "predelay"));
    }

    #[test]
    fn rejects_pro_r_1_presets_explicitly() {
        let mut st = state(&[(field::SPACE, 0.5)]);
        st.metadata.signature = "FPRr".into();
        st.params.truncate(85);
        assert_eq!(decode(&st), Err(ProR2Error::ProR1NotSupported));
    }

    #[test]
    fn rejects_a_short_vector() {
        let st = FfbsState {
            version: 1,
            params: vec![0.0; 20],
            metadata: Default::default(),
        };
        assert_eq!(
            decode(&st),
            Err(ProR2Error::UnexpectedParamCount { got: 20 })
        );
    }
}
