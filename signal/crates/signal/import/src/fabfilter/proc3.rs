//! Reading a `FabFilter` Pro-C 3 instance.
//!
//! Pro-C 3 stores a hundred floats, and unlike Pro-Q 4 it hands over their
//! names for free: a text `.ffp` preset lists exactly the same hundred values
//! under readable keys, in the same order the plugin publishes its
//! parameters. That settles the layout without a single measurement.
//!
//! **The names are free; the units are not.** `Ratio=0.56` is not a ratio and
//! `Attack=0.0993` is not milliseconds — both are positions along a curve the
//! plugin owns, and reading them at face value is how a translation ends up
//! confidently wrong. Every encoding here was read off the plugin with
//! `signal-analyzer`'s `proc3_params` example, which sweeps a parameter
//! across its declared range and prints the stored float beside the plugin's
//! own display text. Where a comment gives numbers, those numbers came from
//! that sweep.
//!
//! ## Decode by index here, by name from a preset file
//!
//! The binary state in a project is written by the installed plugin, so its
//! hundred floats are positional and stable. A `.ffp` file is not: six of the
//! 122 factory presets are an older Pro-C 3 layout carrying 69 keys in a
//! different order, under the same `FC3p` signature. Anything reading preset
//! *files* has to go by key name. This module reads state, so it goes by
//! index — worth saying, because the two look interchangeable and are not.

use crate::fabfilter::ffbs::FfbsState;

/// Parameters in a current Pro-C 3 state.
pub const PARAM_COUNT: usize = 100;
/// Side-chain EQ bands.
pub const SC_BANDS: usize = 6;

/// Index of each parameter in the state vector.
///
/// Taken from the `[Parameters]` section of a current factory preset, which
/// is in the same order as the plugin's own parameter list — both were read
/// back and confirmed to be a hundred entries deep.
pub mod field {
    pub const STYLE: usize = 0;
    pub const THRESHOLD: usize = 1;
    pub const AUTO_THRESHOLD: usize = 2;
    pub const LOCK_AUTO_THRESHOLD: usize = 3;
    pub const RATIO: usize = 4;
    pub const KNEE: usize = 5;
    pub const RANGE: usize = 6;
    pub const ATTACK: usize = 7;
    pub const RELEASE: usize = 8;
    pub const AUTO_RELEASE: usize = 9;
    pub const LOOKAHEAD: usize = 10;
    pub const HOLD: usize = 11;
    pub const CHARACTER: usize = 12;
    pub const CHARACTER_ROUTING: usize = 13;
    pub const CHARACTER_DRIVE: usize = 14;
    pub const WET_GAIN: usize = 15;
    pub const WET_PAN: usize = 16;
    pub const DRY_GAIN: usize = 17;
    pub const DRY_PAN: usize = 18;
    pub const AUTO_GAIN: usize = 19;
    pub const SHOW_SIDE_CHAIN: usize = 20;
    pub const SIDE_CHAIN_INPUT: usize = 21;
    pub const SIDE_CHAIN_LEVEL: usize = 22;
    pub const HOST_TRIGGER_SYNC: usize = 23;
    pub const HOST_TRIGGER_OFFSET: usize = 24;
    pub const HOST_TRIGGER_LENGTH: usize = 25;
    pub const STEREO_LINK: usize = 26;
    pub const STEREO_LINK_MODE: usize = 27;
    pub const STEREO_LINK_CENTER: usize = 28;
    pub const STEREO_LINK_SURROUNDS: usize = 29;
    pub const STEREO_LINK_TOPS: usize = 30;
    pub const STEREO_LINK_LFE: usize = 31;
    /// First side-chain EQ band; six bands of [`SC_STRIDE`] follow.
    pub const SC_EQ: usize = 32;
    pub const AUDITION_SIDE_CHAIN: usize = 86;
    pub const AUDITION_TRIGGERING: usize = 87;
    pub const MIX: usize = 88;
    pub const INPUT_LEVEL: usize = 89;
    pub const INPUT_PAN: usize = 90;
    pub const OUTPUT_LEVEL: usize = 91;
    pub const OUTPUT_PAN: usize = 92;
    pub const BYPASS: usize = 93;
    pub const OVERSAMPLING: usize = 94;
    pub const MAXIMUM_LOOKAHEAD: usize = 95;
    pub const MIDI_STATE: usize = 96;
    pub const METER_SCALE: usize = 97;
    pub const KNEE_DISPLAY: usize = 98;
    pub const SHOW_INPUT_METER: usize = 99;

    /// Fields per side-chain EQ band.
    pub const SC_STRIDE: usize = 9;

    pub mod sc {
        pub const USED: usize = 0;
        pub const ENABLED: usize = 1;
        pub const FREQUENCY: usize = 2;
        pub const GAIN: usize = 3;
        pub const Q: usize = 4;
        pub const SHAPE: usize = 5;
        pub const SLOPE: usize = 6;
        pub const STEREO_PLACEMENT: usize = 7;
        pub const SPEAKERS: usize = 8;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProC3Error {
    /// The blob carries a signature that is not Pro-C 3's.
    WrongSignature(String),
    /// Fewer parameters than a Pro-C 3 state carries.
    Truncated { found: usize },
}

impl std::fmt::Display for ProC3Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongSignature(s) => write!(f, "signature {s:?} is not FC3p"),
            Self::Truncated { found } => {
                write!(f, "{found} parameters, expected at least {PARAM_COUNT}")
            }
        }
    }
}

impl std::error::Error for ProC3Error {}

/// One band of the side-chain EQ.
///
/// The encodings are Pro-Q's — frequency as log2 Hz, Q along the same
/// normalized curve — because it is the same equalizer, cut to six bands and
/// pointed at the detector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScBand {
    pub used: bool,
    pub enabled: bool,
    pub freq_hz: f64,
    pub gain_db: f64,
    /// A real Q, not the stored 0..1 position.
    pub q: f64,
    pub shape: u32,
    pub slope: f64,
    pub placement: u32,
}

impl ScBand {
    /// In the preset, and switched on.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.used && self.enabled
    }
}

/// A decoded Pro-C 3 instance, in real units.
#[derive(Debug, Clone, PartialEq)]
pub struct ProC3 {
    /// Style index. Pro-C 3 ships fourteen; the factory library uses all of
    /// them, which is why this stays a number here and is mapped where the
    /// target's own style set is known.
    pub style: u32,
    pub threshold_db: f64,
    pub auto_threshold: bool,
    pub ratio: f64,
    pub knee_db: f64,
    pub range_db: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub auto_release: bool,
    pub lookahead_ms: f64,
    pub hold_ms: f64,
    pub character: u32,
    /// Character placed before the compressor rather than after it.
    pub character_pre: bool,
    pub character_drive_db: f64,
    /// Wet and dry levels in dB. `None` is the knob at its bottom stop, which
    /// is off rather than merely quiet.
    pub wet_gain_db: Option<f64>,
    pub dry_gain_db: Option<f64>,
    pub auto_gain: bool,
    /// 0 internal, 1 external side chain.
    pub side_chain_input: u32,
    pub side_chain_level_db: f64,
    /// 0 fully unlinked to 1 fully linked.
    pub stereo_link: f64,
    pub stereo_link_mode: u32,
    pub sc_eq: Vec<ScBand>,
    /// Dry/wet as a fraction, 1.0 fully wet.
    pub mix: f64,
    pub input_level_db: f64,
    pub output_level_db: f64,
    pub bypassed: bool,
    pub oversampling: u32,
    pub preset_name: Option<String>,
}

impl ProC3 {
    /// The side-chain EQ bands that actually do something.
    pub fn active_sc_bands(&self) -> impl Iterator<Item = &ScBand> {
        self.sc_eq.iter().filter(|b| b.is_active())
    }

    /// True when the instance leaves the signal alone.
    #[must_use]
    pub fn is_transparent(&self) -> bool {
        self.bypassed || self.ratio <= 1.0001 || self.range_db <= 0.0
    }
}

// ── Encodings ───────────────────────────────────────────────────────────
//
// Everything below was read off the plugin with `signal-analyzer`'s
// `proc3_params` example: it asks `value_to_text` what a stored value means
// at sixty points across each parameter's declared range. The stored float,
// the `.ffp` text value and what a host's `set_param` takes are all the same
// number, confirmed by setting a parameter and reading the state back.

/// Faders — Wet, Dry, Input and Output Level — in dB.
///
/// **36 dB per unit over the working range**, the same constant Pro-Q's
/// Output Level turned out to use, and the same trap: read as decibels the
/// stored 0.3 looks like a third of a dB and is really +10.8. Below -0.6 the
/// fader leaves the linear region and tapers to silence at -1; the two
/// branches meet exactly at -21.6 dB, so the taper's offset is derived from
/// the linear slope rather than fitted.
///
/// Measured: 0.0667 -> +2.40, 0.2 -> +7.20, 1.0 -> +36.00, -0.6 -> -21.60,
/// -0.7333 -> -28.64, -0.8667 -> -40.68, -1 -> silence.
pub const FADER_DB_PER_UNIT: f64 = 36.0;
/// Where the fader stops being linear in dB.
const FADER_TAPER_AT: f64 = -0.6;

/// A fader position in dB. `None` is the bottom stop, which is off.
#[must_use]
pub fn fader_db(stored: f64) -> Option<f64> {
    if stored <= -1.0 {
        return None;
    }
    if stored >= FADER_TAPER_AT {
        return Some(stored * FADER_DB_PER_UNIT);
    }
    let hinge = FADER_TAPER_AT * FADER_DB_PER_UNIT;
    Some(40.0 * ((stored + 1.0) / (FADER_TAPER_AT + 1.0)).log10() + hinge)
}

/// Attack in milliseconds.
///
/// A cube law, exact to the plugin's own display across all sixty sampled
/// points: 0.05 shows 0.036 ms, 0.4 shows 16.00, 0.8 shows 128.0, 1.0 shows
/// 250.0. Worth having in closed form rather than a table — it is the one
/// encoding here that has one.
#[must_use]
pub fn attack_ms(stored: f64) -> f64 {
    0.005 + 250.0 * stored.clamp(0.0, 1.0).powi(3)
}

/// Side-chain EQ Q.
///
/// `0.025 * 1600^x` — the same curve Pro-Q uses, because it is the same
/// equalizer cut down to six bands and pointed at the detector.
#[must_use]
pub fn sc_q(stored: f64) -> f64 {
    0.025 * 1600.0f64.powf(stored.clamp(0.0, 1.0))
}

/// Stereo link, as a fraction.
///
/// The knob does two things in sequence: 0 to 0.5 links the channels from 0
/// to 100%, and 0.5 to 1.0 holds full link while dialling in "Mid-only" from
/// 0 to 100%. Only the first half is a link amount, so only the first half
/// is what comes back here.
#[must_use]
pub fn stereo_link(stored: f64) -> f64 {
    (stored * 2.0).clamp(0.0, 1.0)
}

/// How much of the second half of the stereo-link knob is dialled in.
#[must_use]
pub fn mid_only(stored: f64) -> f64 {
    ((stored - 0.5) * 2.0).clamp(0.0, 1.0)
}

/// Stored position to compression ratio, read off the plugin.
const RATIO_CURVE: [(f64, f64); 61] = [
    (0.0000, 1.0),
    (0.0167, 1.02),
    (0.0333, 1.03),
    (0.0500, 1.05),
    (0.0667, 1.07),
    (0.0833, 1.08),
    (0.1000, 1.1),
    (0.1167, 1.12),
    (0.1333, 1.15),
    (0.1500, 1.17),
    (0.1667, 1.2),
    (0.1833, 1.23),
    (0.2000, 1.25),
    (0.2167, 1.29),
    (0.2333, 1.33),
    (0.2500, 1.38),
    (0.2667, 1.42),
    (0.2833, 1.46),
    (0.3000, 1.5),
    (0.3167, 1.58),
    (0.3333, 1.67),
    (0.3500, 1.75),
    (0.3667, 1.83),
    (0.3833, 1.92),
    (0.4000, 2.0),
    (0.4167, 2.12),
    (0.4333, 2.25),
    (0.4500, 2.38),
    (0.4667, 2.5),
    (0.4833, 2.62),
    (0.5000, 2.75),
    (0.5167, 2.96),
    (0.5333, 3.17),
    (0.5500, 3.38),
    (0.5667, 3.58),
    (0.5833, 3.79),
    (0.6000, 4.0),
    (0.6167, 4.33),
    (0.6333, 4.67),
    (0.6500, 5.0),
    (0.6667, 5.33),
    (0.6833, 5.67),
    (0.7000, 6.0),
    (0.7167, 6.33),
    (0.7333, 6.67),
    (0.7500, 7.0),
    (0.7667, 7.33),
    (0.7833, 7.67),
    (0.8000, 8.0),
    (0.8167, 8.33),
    (0.8333, 8.67),
    (0.8500, 9.0),
    (0.8667, 9.33),
    (0.8833, 9.67),
    (0.9000, 10.0),
    (0.9167, 12.1),
    (0.9333, 17.8),
    (0.9500, 24.4),
    (0.9667, 39.4),
    (0.9833, 75.4),
    (1.0000, 100.0),
];

/// Stored position to release time in milliseconds.
const RELEASE_CURVE_MS: [(f64, f64); 61] = [
    (0.0000, 10.0),
    (0.0167, 10.32),
    (0.0333, 11.29),
    (0.0500, 12.9),
    (0.0667, 15.16),
    (0.0833, 18.07),
    (0.1000, 21.62),
    (0.1167, 25.81),
    (0.1333, 30.66),
    (0.1500, 36.14),
    (0.1667, 42.28),
    (0.1833, 49.07),
    (0.2000, 56.5),
    (0.2167, 64.59),
    (0.2333, 73.34),
    (0.2500, 82.74),
    (0.2667, 92.81),
    (0.2833, 103.5),
    (0.3000, 115.0),
    (0.3167, 127.1),
    (0.3333, 139.8),
    (0.3500, 153.3),
    (0.3667, 167.5),
    (0.3833, 182.5),
    (0.4000, 198.2),
    (0.4167, 214.6),
    (0.4333, 231.9),
    (0.4500, 250.0),
    (0.4667, 268.9),
    (0.4833, 288.7),
    (0.5000, 309.5),
    (0.5167, 331.2),
    (0.5333, 353.9),
    (0.5500, 377.7),
    (0.5667, 402.7),
    (0.5833, 428.8),
    (0.6000, 456.3),
    (0.6167, 485.2),
    (0.6333, 515.6),
    (0.6500, 547.7),
    (0.6667, 581.5),
    (0.6833, 617.4),
    (0.7000, 655.4),
    (0.7167, 695.8),
    (0.7333, 738.8),
    (0.7500, 784.9),
    (0.7667, 834.3),
    (0.7833, 887.4),
    (0.8000, 944.9),
    (0.8167, 1007.0),
    (0.8333, 1076.0),
    (0.8500, 1151.0),
    (0.8667, 1233.0),
    (0.8833, 1325.0),
    (0.9000, 1429.0),
    (0.9167, 1546.0),
    (0.9333, 1680.0),
    (0.9500, 1835.0),
    (0.9667, 2017.0),
    (0.9833, 2234.0),
    (1.0000, 2500.0),
];

/// Stored position to hold time in milliseconds.
const HOLD_CURVE_MS: [(f64, f64); 61] = [
    (0.0000, 0.0),
    (0.0167, 0.417),
    (0.0333, 0.833),
    (0.0500, 1.25),
    (0.0667, 1.667),
    (0.0833, 2.083),
    (0.1000, 2.5),
    (0.1167, 2.917),
    (0.1333, 3.333),
    (0.1500, 3.75),
    (0.1667, 4.167),
    (0.1833, 4.583),
    (0.2000, 5.0),
    (0.2167, 5.833),
    (0.2333, 6.667),
    (0.2500, 7.5),
    (0.2667, 8.333),
    (0.2833, 9.167),
    (0.3000, 10.0),
    (0.3167, 12.5),
    (0.3333, 15.0),
    (0.3500, 17.5),
    (0.3667, 20.0),
    (0.3833, 22.5),
    (0.4000, 25.0),
    (0.4167, 27.5),
    (0.4333, 30.0),
    (0.4500, 32.5),
    (0.4667, 35.0),
    (0.4833, 37.5),
    (0.5000, 40.0),
    (0.5167, 46.67),
    (0.5333, 53.33),
    (0.5500, 60.0),
    (0.5667, 66.67),
    (0.5833, 73.33),
    (0.6000, 80.0),
    (0.6167, 90.0),
    (0.6333, 100.0),
    (0.6500, 110.0),
    (0.6667, 120.0),
    (0.6833, 130.0),
    (0.7000, 140.0),
    (0.7167, 150.0),
    (0.7333, 160.0),
    (0.7500, 170.0),
    (0.7667, 180.0),
    (0.7833, 190.0),
    (0.8000, 200.0),
    (0.8167, 225.0),
    (0.8333, 250.0),
    (0.8500, 275.0),
    (0.8667, 300.0),
    (0.8833, 325.0),
    (0.9000, 350.0),
    (0.9167, 375.0),
    (0.9333, 400.0),
    (0.9500, 425.0),
    (0.9667, 450.0),
    (0.9833, 475.0),
    (1.0000, 500.0),
];

/// Read a measured curve at `stored`, interpolating between its points.
///
/// The tables above are measurements, not fits. Ratio and Release and Hold
/// have no obvious closed form — Release passes through 10 ms, 21.62, 56.50
/// and 2.5 s, which no power law or exponential reaches — and a fitted curve
/// that is wrong in the middle is worse than sixty points that are right
/// everywhere. Between points, linear.
fn read_curve(curve: &[(f64, f64)], stored: f64) -> f64 {
    let x = stored.clamp(curve[0].0, curve[curve.len() - 1].0);
    match curve.windows(2).find(|w| x <= w[1].0) {
        None => curve[curve.len() - 1].1,
        Some(w) => {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            if (x1 - x0).abs() < 1.0e-12 {
                y0
            } else {
                y0 + (y1 - y0) * (x - x0) / (x1 - x0)
            }
        }
    }
}

/// Compression ratio. 1:1 at the bottom, 100:1 — Pro-C's limiting — at the top.
#[must_use]
pub fn ratio(stored: f64) -> f64 {
    read_curve(&RATIO_CURVE, stored)
}

/// Release in milliseconds, 10 ms to 2.5 s.
#[must_use]
pub fn release_ms(stored: f64) -> f64 {
    read_curve(&RELEASE_CURVE_MS, stored)
}

/// Hold in milliseconds, 0 to 500.
#[must_use]
pub fn hold_ms(stored: f64) -> f64 {
    read_curve(&HOLD_CURVE_MS, stored)
}

/// Pro-C 3's fourteen styles, in stored order, as the plugin names them.
pub const STYLE_NAMES: [&str; 14] = [
    "Clean",
    "Versatile",
    "Smooth",
    "Punch",
    "Upward",
    "TTM",
    "Op-El",
    "Vari-Mu",
    "Classic",
    "Opto",
    "Vocal",
    "Mastering",
    "Bus",
    "Pumping",
];

/// The Character stage's four settings, in stored order.
pub const CHARACTER_NAMES: [&str; 4] = ["Off", "Tube", "Diode", "Bright"];

/// Read a Pro-C 3 instance out of its saved state.
///
/// # Errors
/// Returns an error if the state has wrong signature or insufficient parameters.
pub fn decode(state: &FfbsState) -> Result<ProC3, ProC3Error> {
    let sig = &state.metadata.signature;
    if !sig.is_empty() && sig != "FC3p" {
        return Err(ProC3Error::WrongSignature(sig.clone()));
    }
    let p = &state.params;
    if p.len() < PARAM_COUNT {
        return Err(ProC3Error::Truncated { found: p.len() });
    }
    let at = |i: usize| p[i] as f64;

    let mut sc_eq = Vec::with_capacity(SC_BANDS);
    for b in 0..SC_BANDS {
        let base = field::SC_EQ + b * field::SC_STRIDE;
        let f = |o: usize| at(base + o);
        sc_eq.push(ScBand {
            used: f(field::sc::USED) >= 0.5,
            enabled: f(field::sc::ENABLED) >= 0.5,
            // Frequency is stored as log2 Hz, as it is in Pro-Q.
            freq_hz: 2.0f64.powf(f(field::sc::FREQUENCY)),
            gain_db: f(field::sc::GAIN),
            q: sc_q(f(field::sc::Q)),
            shape: f(field::sc::SHAPE).round().max(0.0) as u32,
            slope: f(field::sc::SLOPE),
            placement: f(field::sc::STEREO_PLACEMENT).round().max(0.0) as u32,
        });
    }

    Ok(ProC3 {
        style: at(field::STYLE).round().clamp(0.0, 13.0) as u32,
        threshold_db: at(field::THRESHOLD),
        auto_threshold: at(field::AUTO_THRESHOLD) >= 0.5,
        ratio: ratio(at(field::RATIO)),
        knee_db: at(field::KNEE),
        range_db: at(field::RANGE),
        attack_ms: attack_ms(at(field::ATTACK)),
        release_ms: release_ms(at(field::RELEASE)),
        auto_release: at(field::AUTO_RELEASE) >= 0.5,
        // Stored in milliseconds already — the one control whose display
        // never moved under the probe, so it is taken at face value and
        // flagged rather than quietly converted.
        lookahead_ms: at(field::LOOKAHEAD),
        hold_ms: hold_ms(at(field::HOLD)),
        character: at(field::CHARACTER).round().clamp(0.0, 3.0) as u32,
        character_pre: at(field::CHARACTER_ROUTING) < 0.5,
        character_drive_db: at(field::CHARACTER_DRIVE),
        wet_gain_db: fader_db(at(field::WET_GAIN)),
        dry_gain_db: fader_db(at(field::DRY_GAIN)),
        auto_gain: at(field::AUTO_GAIN) >= 0.5,
        side_chain_input: at(field::SIDE_CHAIN_INPUT).round().max(0.0) as u32,
        side_chain_level_db: at(field::SIDE_CHAIN_LEVEL),
        stereo_link: stereo_link(at(field::STEREO_LINK)),
        stereo_link_mode: at(field::STEREO_LINK_MODE).round().max(0.0) as u32,
        sc_eq,
        mix: at(field::MIX),
        input_level_db: fader_db(at(field::INPUT_LEVEL)).unwrap_or(-96.0),
        output_level_db: fader_db(at(field::OUTPUT_LEVEL)).unwrap_or(-96.0),
        bypassed: at(field::BYPASS) >= 0.5,
        oversampling: at(field::OVERSAMPLING).round().max(0.0) as u32,
        preset_name: state.metadata.preset_name.clone(),
    })
}
