//! Pro-Q 4 → FTS-EQ translation.
//!
//! Decodes the 600-float [`FFBS`](super::ffbs) parameter vector a Pro-Q 4
//! instance writes into a project, and emits the equivalent `signal_fx::NativeEq`
//! parameters by name.
//!
//! The translation is close to exact rather than approximate: `eq-dsp` carries
//! the Pro-Q 4 ZPK design pipeline (`proq4_peak`, `proq4_mzt`) recovered from
//! the binary, and `NativeEq` exposes the same 24-band surface with the same
//! thirteen filter shapes. So a decoded band transfers field-for-field.
//!
//! # Where the field map comes from
//!
//! Not guesswork: Pro-Q 4's `.ffp` presets are the **text** INI format, and
//! their `[Parameters]` section lists every parameter by name in the same
//! order as the binary float vector. `Band 1 Used` is float 0, `Band 1
//! Frequency` is float 2, and so on for 24 bands × 23 fields, with the
//! globals following at float 552. Verified index-by-index against a real
//! project instance.
//!
//! Names are emitted as strings rather than `signal-fx` types on purpose:
//! `signal-import` sits below `signal-fx`, and `NativeEq::set_named` is the
//! by-name entry point the native-block registry already uses.
//!
//! See `spec/project-state-formats.md` for the byte-level layout.

use super::ffbs::FfbsState;

/// Floats per band record.
pub const BAND_STRIDE: usize = 23;
/// Bands in a Pro-Q 4 instance.
pub const BANDS: usize = 24;
/// Where the per-instance globals begin, after the band records.
pub const GLOBALS_OFFSET: usize = BANDS * BAND_STRIDE;
/// Total floats in a Pro-Q 4 state vector written by a current build.
pub const PARAM_COUNT: usize = 600;
/// Total floats written by older builds: the bands plus 24 globals.
pub const PARAM_COUNT_V1: usize = GLOBALS_OFFSET + 24;

/// Float indices of the instance-wide globals, which follow the band records
/// in the order the `.ffp` text lists them.
///
/// Read off a real preset rather than assumed: dumping the key order of
/// `Fast Food Notch.ffp` puts `Processing Mode` at 552 and `Auto Gain` ten
/// slots later, and 555 agrees with the `Gain Scale` slot the plugin probes
/// already write.
pub mod global {
    use super::GLOBALS_OFFSET;
    pub const CHARACTER: usize = GLOBALS_OFFSET + 2;
    pub const GAIN_SCALE: usize = GLOBALS_OFFSET + 3;
    pub const OUTPUT_LEVEL: usize = GLOBALS_OFFSET + 4;
    pub const OUTPUT_PAN: usize = GLOBALS_OFFSET + 5;
    pub const OUTPUT_PAN_MODE: usize = GLOBALS_OFFSET + 6;
    pub const AUTO_GAIN: usize = GLOBALS_OFFSET + 9;

    /// Decibels per unit of the stored [`OUTPUT_LEVEL`] float — measured, see
    /// [`super::ProQ4::output_level_db`].
    pub const OUTPUT_LEVEL_DB_PER_UNIT: f64 = 36.0;
}

/// Where the per-band `Spectral Tilt` values begin, in states long enough to
/// have them.
///
/// Newer Pro-Q 4 builds append one per band **after** the globals rather than
/// widening the band record — the bands stay 23 fields and the globals stay at
/// [`GLOBALS_OFFSET`], so both layouts read identically up to this point. That
/// is worth stating because the opposite guess (a wider band record) is the
/// natural one, costs nothing to believe, and silently shifts every band after
/// the first.
pub const SPECTRAL_TILT_OFFSET: usize = PARAM_COUNT_V1;

/// Field offsets within a band record, named from Pro-Q 4's own parameter list.
mod field {
    pub const USED: usize = 0;
    pub const ENABLED: usize = 1;
    pub const FREQUENCY: usize = 2;
    pub const GAIN: usize = 3;
    pub const Q: usize = 4;
    pub const SHAPE: usize = 5;
    pub const SLOPE: usize = 6;
    pub const STEREO_PLACEMENT: usize = 7;
    pub const DYNAMIC_RANGE: usize = 9;
    pub const DYNAMICS_ENABLED: usize = 10;
    pub const DYNAMICS_AUTO: usize = 11;
    pub const THRESHOLD: usize = 12;
    /// Which surround channels the band addresses.
    pub const SPEAKERS: usize = 8;
    /// The detector listens to a custom range rather than the band's own.
    pub const SIDE_CHAIN_FILTERING: usize = 16;
    /// Custom side-chain range, stored as log2(Hz).
    pub const SIDE_CHAIN_LOW_FREQ: usize = 17;
    pub const SIDE_CHAIN_HIGH_FREQ: usize = 18;
    /// The band's dynamics act per-FFT-bin rather than on the whole band.
    pub const SPECTRAL_ENABLED: usize = 20;
    /// Per-bin selectivity, 0–100.
    pub const SPECTRAL_DENSITY: usize = 21;
    pub const ATTACK: usize = 13;
    pub const RELEASE: usize = 14;
    // 8 Speakers, 15..19 side-chain, 20..21 spectral, 22 solo — decoded into
    // `Band` where they have a NativeEq counterpart, ignored where they do not.
}

/// Pro-Q 4's stereo-placement selector.
///
/// The numbering differs from `NativeEq`'s, so it must be translated rather
/// than passed through — see [`Band::native_placement`].
///
/// Established from the preset corpus: 2 is overwhelmingly the most common
/// value and is Pro-Q's default (Stereo); 3 and 4 appear together in
/// mid/side-style presets, matching `NativeEq`'s own Mid/Side at 3/4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Left,
    Right,
    Stereo,
    Mid,
    Side,
}

impl Placement {
    fn from_raw(v: f32) -> Self {
        match v.round() as i32 {
            0 => Self::Left,
            1 => Self::Right,
            3 => Self::Mid,
            4 => Self::Side,
            // 2 is Stereo, and anything unrecognized is safest as Stereo —
            // a band on the wrong side of the image is worse than a band
            // across the whole of it.
            _ => Self::Stereo,
        }
    }

    /// The equivalent `NativeEq` placement index
    /// (0 Stereo, 1 Left, 2 Right, 3 Mid, 4 Side).
    #[must_use]
    pub fn native_index(self) -> f64 {
        match self {
            Self::Stereo => 0.0,
            Self::Left => 1.0,
            Self::Right => 2.0,
            Self::Mid => 3.0,
            Self::Side => 4.0,
        }
    }
}

/// One decoded Pro-Q 4 band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    /// Index within the instance, 0-based.
    pub index: usize,
    /// The band slot is in use. Pro-Q keeps all 24 records populated; this
    /// flag is what separates a real band from an untouched slot.
    pub used: bool,
    /// The band is enabled (not bypassed).
    pub enabled: bool,
    /// Centre/corner frequency in Hz (decoded from the stored `log2(Hz)`).
    pub freq_hz: f32,
    /// Gain in dB.
    pub gain_db: f32,
    /// Q, in Pro-Q's normalized 0..1 units. Use [`Band::q_value`] for the
    /// real thing.
    pub q: f32,
    /// Filter shape, in **Pro-Q's** numbering. Use [`Band::native_shape`] for
    /// the value the engine wants.
    pub shape: u32,
    /// Slope in Pro-Q's units — **continuous**, `slope * 6` dB/oct up to 36.
    pub slope: f32,
    /// Stereo placement.
    pub placement: Placement,
    /// Which surround channels the band addresses. Read back from the plugin:
    /// 0 All Speakers, 1 All (excl. LFE), 2 LFE, 3 Center, 4 L/R (Front),
    /// 5 Lc/Rc, 6 Lss/Rss.
    pub speakers: u32,
    /// Dynamic-EQ range in dB. Zero means the band is static.
    pub dynamic_range_db: f32,
    /// The band's dynamics section is switched on.
    pub dynamics_enabled: bool,
    /// Dynamic-EQ auto-threshold.
    pub dynamics_auto: bool,
    /// Dynamic-EQ threshold, in Pro-Q's normalized units.
    pub threshold: f32,
    /// Dynamic-EQ attack, 0–100.
    pub attack: f32,
    /// Dynamic-EQ release, 0–100.
    pub release: f32,
    /// The detector listens to a custom range rather than the band's own.
    pub side_filtered: bool,
    /// Custom side-chain range, stored as log2(Hz) like `Frequency`.
    pub side_lo: f32,
    pub side_hi: f32,
    /// The band's dynamics run per-FFT-bin (Pro-Q 4 "Spectral") rather than
    /// as one gain ride over the whole band.
    pub spectral: bool,
    /// Per-bin selectivity, 0–100.
    pub spectral_density: f32,
    /// Pink-noise tilt on the spectral trigger spectrum.
    pub spectral_tilt: bool,
}

impl Band {
    /// The band's Q, as a Q rather than as Pro-Q stores it.
    ///
    /// Pro-Q keeps Q normalized 0..1 and maps it logarithmically over its
    /// 0.025 .. 40 range, so the stored 0.5 that sits on most untouched bands
    /// is Q = 1.0, not Q = 0.5. Passing the stored number through as a Q — as
    /// this translator did — narrows or widens **every band in every preset**.
    ///
    /// Measured, not inferred: the hosted plugin reports a stored 0.384688 as
    /// "0.427" and 0.596290 as "2.035", both of which are
    /// `0.025 * 1600^x` to three decimals.
    #[must_use]
    pub fn q_value(&self) -> f64 {
        const MIN_Q: f64 = 0.025;
        const MAX_Q: f64 = 40.0;
        MIN_Q * (MAX_Q / MIN_Q).powf((self.q as f64).clamp(0.0, 1.0))
    }

    /// The dynamics threshold in dB, or `None` when the band is on
    /// auto-threshold.
    ///
    /// Pro-Q stores this normalized over a **three-segment** curve, with the
    /// very top of the range reserved for Auto. Swept through the hosted
    /// plugin by writing the slot directly into its state (it refuses host
    /// writes to this parameter) and reading back what it then calls the
    /// value; every point below fits to the 0.1 dB the display shows:
    ///
    /// ```text
    ///   x < 0.1     -90 + 180x          0.00 -> -90.0   0.05 -> -81.0
    ///   0.1 .. 0.2  -72 + 240(x - 0.1)  0.12 -> -67.2   0.18 -> -52.8
    ///   x >= 0.2    (x - 1) * 60        0.25 -> -45.0   0.50 -> -30.0
    ///   x == 1.0    Auto
    /// ```
    ///
    /// The Auto sentinel is the part that matters in practice: **525 of the
    /// 528 dynamic bands in the factory library sit at 1.0**, and reading that
    /// as "0 dB" — which the plain linear reading of the top segment gives —
    /// pins them to a threshold nothing ever crosses instead of letting the
    /// plugin find its own.
    #[must_use]
    pub fn threshold_db(&self) -> Option<f64> {
        let x = (self.threshold as f64).clamp(0.0, 1.0);
        if x >= 1.0 {
            return None;
        }
        Some(if x < 0.1 {
            -90.0 + 180.0 * x
        } else if x < 0.2 {
            -72.0 + 240.0 * (x - 0.1)
        } else {
            (x - 1.0) * 60.0
        })
    }

    /// Whether the band's threshold is left to the plugin.
    ///
    /// Two things say so and either is enough: the dedicated flag, and the
    /// threshold knob parked at its Auto position.
    #[must_use]
    pub fn threshold_is_auto(&self) -> bool {
        self.dynamics_auto || self.threshold_db().is_none()
    }

    /// Whether this band actually shapes a stereo signal.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.used && self.enabled && self.addresses_stereo()
    }

    /// Whether the band's dynamics are doing anything.
    #[must_use]
    pub fn is_dynamic(&self) -> bool {
        self.dynamics_enabled && self.dynamic_range_db != 0.0
    }

    /// The `NativeEq` placement index for this band.
    #[must_use]
    pub fn native_placement(&self) -> f64 {
        self.placement.native_index()
    }

    /// The side-chain range in Hz, stored the same way `Frequency` is.
    #[must_use]
    pub fn side_range_hz(&self) -> (f64, f64) {
        let hz = |v: f32| (v as f64).exp2().clamp(20.0, 20_000.0);
        (hz(self.side_lo), hz(self.side_hi))
    }

    /// Whether this band addresses channels a stereo bus actually has.
    ///
    /// Pro-Q can aim a band at one surround channel, and on a stereo instance
    /// a band aimed at Center or LFE simply does nothing — the plugin passes
    /// audio through untouched. Translating such a band anyway applies it to
    /// both channels, which is not a near miss: the Surround bank's presets
    /// measure as flat through Pro-Q and as several dB of EQ through ours.
    ///
    /// Only "All Speakers", "All (excl. LFE)" and "L/R (Front)" reach the two
    /// channels a stereo bus has.
    #[must_use]
    pub fn addresses_stereo(&self) -> bool {
        matches!(self.speakers, 0 | 1 | 4)
    }

    /// The engine's shape index for this band.
    ///
    /// Pro-Q and `eq_dsp::slope::FilterShape` agree on every shape except two:
    /// Pro-Q numbers **2 = Low Cut, 3 = High Shelf** (read back from the
    /// plugin itself), and the engine's canonical order has them the other way
    /// round. Passing the raw index through therefore turns every high shelf
    /// into a low cut and every low cut into a high shelf — which is not a
    /// subtle mis-tuning but a completely different filter, and it affected
    /// **188 of the 864 active bands** in the factory library.
    ///
    /// Translated rather than passed through, exactly as `Placement` is, and
    /// for the same reason.
    #[must_use]
    pub fn native_shape(&self) -> f64 {
        match self.shape {
            2 => 3.0, // Pro-Q Low Cut
            3 => 2.0, // Pro-Q High Shelf
            other => other as f64,
        }
    }
}

/// A decoded Pro-Q 4 instance.
#[derive(Debug, Clone, PartialEq)]
pub struct ProQ4 {
    /// All 24 band slots, in instance order.
    pub bands: Vec<Band>,
    /// The instance-wide Gain Scale knob — every band's gain and dynamic
    /// range is multiplied by it. Neutral at 1.0; ten of the 171 factory
    /// presets ship with it somewhere else.
    pub gain_scale: f32,
    /// Output Level trim in dB.
    ///
    /// **The stored float is not decibels.** It is a fraction of the knob's
    /// range, and the range is 36 dB: written into the plugin and measured
    /// back, 0.1 gives +3.60 dB, 0.3 gives +10.80 and 0.6 gives +21.60. Read
    /// as dB it looked like a rounding error worth carrying for tidiness; read
    /// correctly, **68 of the 171 factory presets** carry one, and the largest
    /// is 10.87 dB — the whole error on the worst preset in the library.
    ///
    /// (The linearity gives out near the bottom of the travel, where the knob
    /// runs to silence: -0.8 measures -33.64 rather than -28.80. Nothing in
    /// the factory library goes past -0.31, so the straight line is what is
    /// modelled.)
    pub output_level_db: f32,
    /// Character mode: 0 Clean, 1 Subtle, 2 Warm.
    pub character: u32,
    /// Output Pan, -1..1, and whether it works on mid/side rather than
    /// left/right. Five factory presets set a non-zero pan and all five are
    /// mid/side; on "Room 01" it is 2.54 dB of a 3.23 dB error.
    pub output_pan: f32,
    pub output_pan_mid_side: bool,
    /// Auto Gain: the plugin compensates its own curve so the broadband
    /// level does not move. Ten factory presets have it on, and without it
    /// they measure as a flat offset — 4.71 dB on "Fast Food Notch".
    pub auto_gain: bool,
    /// Preset name from the FFBS trailer, if any.
    pub preset_name: Option<String>,
}

impl ProQ4 {
    /// The bands that actually shape the signal.
    pub fn active_bands(&self) -> impl Iterator<Item = &Band> {
        self.bands.iter().filter(|b| b.is_active())
    }
}

/// Errors from decoding a Pro-Q 4 state vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProQ4Error {
    /// The float vector is too short to hold 24 band records.
    UnexpectedParamCount { got: usize },
}

impl std::fmt::Display for ProQ4Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedParamCount { got } => write!(
                f,
                "expected at least {GLOBALS_OFFSET} floats in a Pro-Q 4 state, found {got}"
            ),
        }
    }
}

impl std::error::Error for ProQ4Error {}

/// Decode a parsed [`FfbsState`] as a Pro-Q 4 instance.
///
/// # Errors
///
/// Returns an error if the parameter vector is too short to contain a full Pro-Q 4 state.
pub fn decode(state: &FfbsState) -> Result<ProQ4, ProQ4Error> {
    let p = &state.params;
    if p.len() < GLOBALS_OFFSET {
        return Err(ProQ4Error::UnexpectedParamCount { got: p.len() });
    }

    let bands = (0..BANDS)
        .map(|i| {
            let b = &p[i * BAND_STRIDE..][..BAND_STRIDE];
            Band {
                index: i,
                used: b[field::USED] != 0.0,
                enabled: b[field::ENABLED] != 0.0,
                // Stored as log2(Hz); clamp so a corrupt exponent yields a
                // usable frequency rather than inf or 0.
                freq_hz: b[field::FREQUENCY].exp2().clamp(1.0, 40_000.0),
                gain_db: b[field::GAIN],
                q: b[field::Q],
                shape: b[field::SHAPE].max(0.0) as u32,
                slope: b[field::SLOPE].max(0.0),
                placement: Placement::from_raw(b[field::STEREO_PLACEMENT]),
                speakers: b[field::SPEAKERS].max(0.0) as u32,
                dynamic_range_db: b[field::DYNAMIC_RANGE],
                dynamics_enabled: b[field::DYNAMICS_ENABLED] != 0.0,
                dynamics_auto: b[field::DYNAMICS_AUTO] != 0.0,
                threshold: b[field::THRESHOLD],
                attack: b[field::ATTACK],
                release: b[field::RELEASE],
                side_filtered: b[field::SIDE_CHAIN_FILTERING] > 0.5,
                side_lo: b[field::SIDE_CHAIN_LOW_FREQ],
                side_hi: b[field::SIDE_CHAIN_HIGH_FREQ],
                spectral: b[field::SPECTRAL_ENABLED] > 0.5,
                spectral_density: b[field::SPECTRAL_DENSITY],
                // Appended after the globals by newer builds; absent (and so
                // off) in states written before it existed.
                spectral_tilt: p
                    .get(SPECTRAL_TILT_OFFSET + i)
                    .is_some_and(|v| *v > 0.5),
            }
        })
        .collect();

    Ok(ProQ4 {
        bands,
        gain_scale: p.get(global::GAIN_SCALE).copied().unwrap_or(1.0),
        output_level_db: p.get(global::OUTPUT_LEVEL).copied().unwrap_or(0.0)
            * global::OUTPUT_LEVEL_DB_PER_UNIT as f32,
        character: p.get(global::CHARACTER).copied().unwrap_or(0.0).max(0.0) as u32,
        output_pan: p.get(global::OUTPUT_PAN).copied().unwrap_or(0.0),
        output_pan_mid_side: p.get(global::OUTPUT_PAN_MODE).is_some_and(|v| *v > 0.5),
        auto_gain: p.get(global::AUTO_GAIN).is_some_and(|v| *v > 0.5),
        preset_name: state.metadata.preset_name.clone(),
    })
}

/// Translate a decoded Pro-Q 4 instance into `NativeEq` parameters.
///
/// Returns `(param_name, value)` pairs for `NativeEq::set_named`. Active bands
/// are packed into the low slots in source order, so a Pro-Q using bands 1, 4
/// and 8 becomes `b1_*`, `b2_*`, `b3_*`.
///
/// Every slot the source did not fill is explicitly cleared (`b{n}_used = 0`),
/// so applying the result to a reused `NativeEq` leaves no stale bands behind.
#[must_use]
pub fn to_native_eq_params(eq: &ProQ4) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    let mut slot = 0usize;

    for band in eq.active_bands() {
        if slot >= BANDS {
            break;
        }
        let n = slot + 1; // NativeEq band names are 1-based
        let mut set = |field: &str, v: f64| out.push((format!("b{n}_{field}"), v));

        set("used", 1.0);
        set("on", 1.0);
        set("freq", band.freq_hz as f64);
        set("gain", band.gain_db as f64);
        set("q", band.q_value());
        set("shape", band.native_shape());
        set("slope", band.slope as f64);
        set("placement", band.native_placement());

        // Dynamics only when the section is on *and* dialled in — Pro-Q leaves
        // "Dynamics Enabled" set on untouched bands, so range is the real test.
        if band.is_dynamic() {
            set("dyn_range", band.dynamic_range_db as f64);
            set("dyn_atk", band.attack as f64);
            set("dyn_rel", band.release as f64);
            let auto = band.threshold_is_auto();
            set("dyn_auto", if auto { 1.0 } else { 0.0 });
            // Under auto the stored number is the Auto sentinel, not a
            // threshold, so there is nothing to carry across; the engine
            // learns its own and `dyn_auto` tells it to.
            if let Some(thr) = band.threshold_db() {
                set("dyn_thr", thr);
            }
            // Spectral turns the same range and threshold into a per-bin
            // action instead of one gain ride over the band — the region the
            // spectral engine works from is built out of exactly those two,
            // so the flag is all that has to cross.
            if band.spectral {
                set("spectral", 1.0);
                // Density is per band in Pro-Q, and the factory library uses
                // 25 distinct values across its spectral bands.
                set("spectral_density", band.spectral_density as f64);
                if band.spectral_tilt {
                    set("spectral_tilt", 1.0);
                }
            }
            // A custom side-chain range: the band triggers on what it is
            // pointed at rather than on itself.
            if band.side_filtered {
                let (lo, hi) = band.side_range_hz();
                set("dyn_side", 1.0);
                set("dyn_side_lo", lo);
                set("dyn_side_hi", hi);
            }
        }

        slot += 1;
    }

    for n in slot + 1..=BANDS {
        out.push((format!("b{n}_used"), 0.0));
    }

    // Instance-wide globals. These were dropped entirely, and two of them are
    // not cosmetic: Gain Scale multiplies every band's gain and range, and
    // Auto Gain moves the whole output.
    out.push(("gain_scale".into(), eq.gain_scale as f64));
    out.push(("output_gain".into(), eq.output_level_db as f64));
    out.push(("auto_gain".into(), if eq.auto_gain { 1.0 } else { 0.0 }));
    out.push(("character".into(), eq.character as f64));
    out.push((
        "output_pan_mode".into(),
        if eq.output_pan_mid_side { 1.0 } else { 0.0 },
    ));
    out.push(("output_pan".into(), eq.output_pan as f64));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fabfilter::ffbs::FfbsMetadata;

    /// Pro-Q's untouched slot: 1 kHz, 0 dB, Q 0.5, Peak, Used=0.
    const DEFAULT_FREQ_LOG2: f32 = 9.965_784;

    /// A band record in the real field order.
    #[allow(clippy::too_many_arguments)]
    fn band_record(
        used: f32,
        enabled: f32,
        freq_log2: f32,
        gain: f32,
        q: f32,
        shape: f32,
        slope: f32,
        placement: f32,
    ) -> [f32; BAND_STRIDE] {
        let mut b = [0.0f32; BAND_STRIDE];
        b[field::USED] = used;
        b[field::ENABLED] = enabled;
        b[field::FREQUENCY] = freq_log2;
        b[field::GAIN] = gain;
        b[field::Q] = q;
        b[field::SHAPE] = shape;
        b[field::SLOPE] = slope;
        b[field::STEREO_PLACEMENT] = placement;
        b[field::DYNAMICS_ENABLED] = 1.0; // Pro-Q's default, even when unused
        b[field::DYNAMICS_AUTO] = 1.0;
        b[field::ATTACK] = 50.0;
        b[field::RELEASE] = 50.0;
        b
    }

    fn state(bands: &[[f32; BAND_STRIDE]]) -> FfbsState {
        let mut p = vec![0.0f32; PARAM_COUNT];
        for i in 0..BANDS {
            let rec = &mut p[i * BAND_STRIDE..][..BAND_STRIDE];
            match bands.get(i) {
                Some(b) => rec.copy_from_slice(b),
                // Untouched slot: Used = 0.
                None => rec.copy_from_slice(&band_record(
                    0.0,
                    1.0,
                    DEFAULT_FREQ_LOG2,
                    0.0,
                    0.5,
                    0.0,
                    2.0,
                    2.0,
                )),
            }
        }
        FfbsState {
            version: 1,
            params: p,
            metadata: FfbsMetadata {
                signature: "FQ4p".into(),
                preset_name: Some("Default Setting".into()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn decodes_the_real_project_instance() {
        // The first four bands of Pro-Q 4 instance 0 in
        // `02 LORD OF THE FIGHT.RPP`, which has exactly four used bands.
        let st = state(&[
            band_record(1.0, 1.0, 7.589, -1.5042, 0.5, 0.0, 2.0, 2.0),
            band_record(1.0, 1.0, 5.9206, 4.3475, 0.5, 0.0, 2.0, 2.0),
            band_record(1.0, 1.0, 13.007, 3.797, 0.5699, 0.0, 2.0, 2.0),
            band_record(1.0, 1.0, 9.092, 2.326, 0.4196, 0.0, 2.0, 2.0),
        ]);
        let eq = decode(&st).unwrap();

        assert_eq!(eq.active_bands().count(), 4);
        assert!((eq.bands[0].freq_hz - 192.5).abs() < 0.5);
        assert!((eq.bands[1].freq_hz - 60.6).abs() < 0.5);
        assert!((eq.bands[2].freq_hz - 8233.0).abs() < 5.0);
        assert_eq!(eq.bands[0].gain_db, -1.5042);
        assert_eq!(eq.preset_name.as_deref(), Some("Default Setting"));
    }

    #[test]
    fn used_not_a_value_heuristic_decides_which_bands_are_active() {
        // A band parked at the 1 kHz / 0 dB default but flagged Used must be
        // kept; an identical-looking one with Used=0 must not.
        let mut st = state(&[band_record(
            1.0,
            1.0,
            DEFAULT_FREQ_LOG2,
            0.0,
            0.5,
            0.0,
            2.0,
            2.0,
        )]);
        let eq = decode(&st).unwrap();
        assert!(eq.bands[0].is_active(), "Used=1 band must survive");
        assert!(!eq.bands[1].is_active(), "Used=0 band must not");

        // Flipping only Used flips the outcome.
        st.params[field::USED] = 0.0;
        assert!(!decode(&st).unwrap().bands[0].is_active());
    }

    #[test]
    fn a_bypassed_band_is_not_active() {
        let st = state(&[band_record(1.0, 0.0, 7.0, 3.0, 0.7, 0.0, 2.0, 2.0)]);
        let eq = decode(&st).unwrap();
        assert!(eq.bands[0].used);
        assert!(!eq.bands[0].enabled);
        assert!(!eq.bands[0].is_active());
    }

    #[test]
    fn placement_is_translated_not_passed_through() {
        // Pro-Q's default 2 is Stereo, which is NativeEq's 0 — passing the
        // raw value through would put every band hard right.
        let st = state(&[
            band_record(1.0, 1.0, 7.0, 1.0, 0.5, 0.0, 2.0, 2.0), // Stereo
            band_record(1.0, 1.0, 7.0, 1.0, 0.5, 0.0, 2.0, 0.0), // Left
            band_record(1.0, 1.0, 7.0, 1.0, 0.5, 0.0, 2.0, 3.0), // Mid
            band_record(1.0, 1.0, 7.0, 1.0, 0.5, 0.0, 2.0, 4.0), // Side
        ]);
        let eq = decode(&st).unwrap();
        assert_eq!(eq.bands[0].placement, Placement::Stereo);
        assert_eq!(eq.bands[0].native_placement(), 0.0);
        assert_eq!(eq.bands[1].placement, Placement::Left);
        assert_eq!(eq.bands[1].native_placement(), 1.0);
        // Mid and Side happen to align between the two numberings.
        assert_eq!(eq.bands[2].native_placement(), 3.0);
        assert_eq!(eq.bands[3].native_placement(), 4.0);
    }

    #[test]
    fn dynamics_ride_on_range_not_on_the_enabled_flag() {
        // Pro-Q leaves "Dynamics Enabled" set on untouched bands, so emitting
        // dynamics off that flag alone would make every band dynamic.
        let mut with_range = band_record(1.0, 1.0, 8.0, -3.0, 0.6, 0.0, 2.0, 2.0);
        with_range[field::DYNAMIC_RANGE] = -6.0;
        with_range[field::ATTACK] = 25.0;
        with_range[field::RELEASE] = 75.0;

        let st = state(&[
            band_record(1.0, 1.0, 7.0, 1.0, 0.5, 0.0, 2.0, 2.0), // enabled, no range
            with_range,
        ]);
        let eq = decode(&st).unwrap();
        assert!(eq.bands[0].dynamics_enabled && !eq.bands[0].is_dynamic());
        assert!(eq.bands[1].is_dynamic());

        let params = to_native_eq_params(&eq);
        let get = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
        assert_eq!(get("b1_dyn_range"), None, "static band emits no dynamics");
        assert_eq!(get("b2_dyn_range"), Some(-6.0));
        assert_eq!(get("b2_dyn_atk"), Some(25.0));
        assert_eq!(get("b2_dyn_rel"), Some(75.0));
    }

    #[test]
    fn packs_active_bands_into_low_slots_and_clears_the_rest() {
        // Band 1 used, band 2 unused, band 3 used → b1_*, b2_*.
        let mut p = state(&[band_record(1.0, 1.0, 7.589, -1.5, 0.5, 0.0, 2.0, 2.0)]);
        let third = band_record(1.0, 1.0, 6.0, 3.0, 0.7, 8.0, 4.0, 2.0);
        p.params[2 * BAND_STRIDE..3 * BAND_STRIDE].copy_from_slice(&third);

        let eq = decode(&p).unwrap();
        let params = to_native_eq_params(&eq);
        let get = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| *v);

        assert_eq!(get("b1_used"), Some(1.0));
        assert!((get("b1_freq").unwrap() - 192.5).abs() < 0.5);
        assert_eq!(get("b2_shape"), Some(8.0));
        assert_eq!(get("b2_slope"), Some(4.0));
        assert_eq!(get("b3_used"), Some(0.0));
        assert_eq!(get("b24_used"), Some(0.0));
    }

    #[test]
    fn rejects_a_short_vector() {
        let st = FfbsState {
            version: 1,
            params: vec![0.0; 10],
            metadata: Default::default(),
        };
        assert_eq!(
            decode(&st),
            Err(ProQ4Error::UnexpectedParamCount { got: 10 })
        );
    }

    /// The dynamics threshold crosses on the curve the plugin reports.
    ///
    /// Every point here was read back out of the hosted plugin after writing
    /// the slot into its state — Pro-Q refuses host writes to this parameter,
    /// so a sweep through the automation interface silently reports the
    /// default and a range "inferred" from preset files comes out plausible
    /// and wrong. The first version of this translator did exactly that.
    #[test]
    fn the_dynamics_threshold_crosses_on_its_measured_curve() {
        let threshold_of = |normalized: f32| {
            let mut b = band_record(1.0, 1.0, 8.0, -3.0, 0.6, 0.0, 2.0, 2.0);
            b[field::DYNAMIC_RANGE] = -6.0;
            b[field::THRESHOLD] = normalized;
            decode(&state(&[b])).unwrap().bands[0].threshold_db()
        };

        for (normalized, want_db) in [
            // Bottom segment.
            (0.0f32, -90.0f64),
            (0.05, -81.0),
            (0.08, -75.6),
            // Middle segment — the steepest of the three.
            (0.12, -67.2),
            (0.18, -52.8),
            // Top segment, where the plain (x-1)*60 reading holds.
            (0.2, -48.0),
            (0.25, -45.0),
            (0.5, -30.0),
            (0.666_667, -20.0),
            (0.9, -6.0),
        ] {
            let got = threshold_of(normalized).expect("a real threshold");
            assert!(
                (got - want_db).abs() < 0.05,
                "{normalized} should be {want_db} dB, got {got}",
            );
        }

        // The top of the range is Auto, not 0 dB. 525 of the 528 dynamic
        // bands in the factory library sit here.
        assert_eq!(threshold_of(1.0), None, "1.0 is the Auto position");
    }

    /// A band parked on the Auto threshold is translated as auto.
    ///
    /// Reading the sentinel as a number instead pins the band to a threshold
    /// nothing crosses, so the dynamics never engage — the preset loads, looks
    /// right, and does nothing.
    #[test]
    fn the_auto_threshold_sentinel_becomes_auto_not_zero_db() {
        let mut b = band_record(1.0, 1.0, 8.0, -3.0, 0.6, 0.0, 2.0, 2.0);
        b[field::DYNAMIC_RANGE] = -6.0;
        b[field::THRESHOLD] = 1.0;
        // The dedicated flag is OFF: only the knob position says auto, which
        // is the case for 226 bands in the library.
        b[field::DYNAMICS_AUTO] = 0.0;

        let eq = decode(&state(&[b])).unwrap();
        assert!(eq.bands[0].threshold_is_auto());

        let params = to_native_eq_params(&eq);
        let get = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
        assert_eq!(get("b1_dyn_auto"), Some(1.0), "the band must arrive on auto");
        assert_eq!(get("b1_dyn_thr"), None, "and carry no fixed threshold");
    }

    /// A band aimed at a channel a stereo bus does not have is inert.
    ///
    /// Pro-Q's Speakers control targets one surround channel. On a stereo
    /// instance a band aimed at Center or LFE does nothing at all — the
    /// plugin measures flat. Translating it anyway applies it to both
    /// channels, which is how the Surround bank came out several dB of EQ
    /// away from a reference that was not filtering.
    #[test]
    fn a_band_aimed_off_the_stereo_bus_is_not_translated() {
        let with_speakers = |speakers: f32| {
            let mut b = band_record(1.0, 1.0, 8.0, -6.0, 0.5, 0.0, 2.0, 2.0);
            b[field::SPEAKERS] = speakers;
            decode(&state(&[b])).unwrap().bands[0].is_active()
        };

        // Reach both stereo channels.
        for reaches in [0.0f32, 1.0, 4.0] {
            assert!(with_speakers(reaches), "speakers {reaches} addresses stereo");
        }
        // Do not exist on a stereo bus.
        for inert in [2.0f32, 3.0, 5.0, 6.0] {
            assert!(!with_speakers(inert), "speakers {inert} is inert in stereo");
        }

        // And an inert band contributes nothing to the translation.
        let mut b = band_record(1.0, 1.0, 8.0, -6.0, 0.5, 0.0, 2.0, 2.0);
        b[field::SPEAKERS] = 3.0; // Center
        let params = to_native_eq_params(&decode(&state(&[b])).unwrap());
        assert_eq!(
            params.iter().find(|(n, _)| n == "b1_gain").map(|(_, v)| *v),
            None,
            "a Center-only band must not become a stereo band",
        );
    }

    /// Shape is translated, not passed through.
    ///
    /// The two numberings agree everywhere except 2 and 3. Read back from the
    /// hosted plugin, Pro-Q's order is Bell, Low Shelf, **Low Cut**, **High
    /// Shelf**, High Cut, Notch, Band Pass, Tilt Shelf, Flat Tilt, All Pass;
    /// the engine's canonical order swaps that middle pair. Passing the index
    /// through turned every high shelf into a low cut — measured as a -40 dB
    /// high-pass where the plugin was flat.
    #[test]
    fn shape_is_translated_not_passed_through() {
        let shape_of = |raw: f32| {
            let mut b = band_record(1.0, 1.0, 8.0, 3.0, 0.5, 0.0, 2.0, 2.0);
            b[field::SHAPE] = raw;
            decode(&state(&[b])).unwrap().bands[0].native_shape()
        };

        // The swapped pair.
        assert_eq!(shape_of(2.0), 3.0, "Pro-Q's Low Cut is the engine's 3");
        assert_eq!(shape_of(3.0), 2.0, "Pro-Q's High Shelf is the engine's 2");
        // Everything else is already aligned.
        for same in [0.0f32, 1.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0] {
            assert_eq!(shape_of(same), same as f64, "{same} should pass through");
        }

        // And the translated value is what reaches the engine, where it must
        // resolve to the filter the preset asked for.
        let mut b = band_record(1.0, 1.0, 8.0, 3.0, 0.5, 0.0, 2.0, 2.0);
        b[field::SHAPE] = 3.0; // Pro-Q High Shelf
        let params = to_native_eq_params(&decode(&state(&[b])).unwrap());
        let shape = params.iter().find(|(n, _)| n == "b1_shape").map(|(_, v)| *v);
        assert_eq!(shape, Some(2.0));
        assert_eq!(
            reverb_shape_name(shape.unwrap()),
            "HighShelf",
            "a Pro-Q high shelf must arrive as a high shelf",
        );
    }

    /// Name the engine shape an index resolves to, for readable assertions.
    fn reverb_shape_name(idx: f64) -> String {
        format!(
            "{:?}",
            eq_dsp::slope::FilterShape::from_canonical_index(idx as u32)
        )
    }

    /// Q crosses as a Q, not as Pro-Q's normalized storage.
    ///
    /// Both anchors are readings from the hosted plugin: it reports a stored
    /// 0.384688 as "0.427" and 0.596290 as "2.035". Passing the stored number
    /// straight through — which this translator did — mis-sets the width of
    /// every band in every preset, and does it quietly: 0.5 is a perfectly
    /// plausible Q, it is just not the Q the preset asked for (1.0).
    #[test]
    fn q_crosses_as_a_q_not_as_normalized_storage() {
        let q_of = |normalized: f32| {
            let mut b = band_record(1.0, 1.0, 8.0, 0.0, normalized, 0.0, 2.0, 2.0);
            b[field::Q] = normalized;
            decode(&state(&[b])).unwrap().bands[0].q_value()
        };

        for (normalized, want) in [
            (0.0f32, 0.025f64),
            (0.384_688, 0.427),
            (0.5, 1.0),
            (0.596_29, 2.035),
            (1.0, 40.0),
        ] {
            let got = q_of(normalized);
            assert!(
                (got - want).abs() < 0.01 * want.max(1.0),
                "{normalized} should be Q {want}, got {got}",
            );
        }

        // And it is the converted value that reaches the engine.
        let mut b = band_record(1.0, 1.0, 8.0, 0.0, 0.5, 0.0, 2.0, 2.0);
        b[field::Q] = 0.5;
        let params = to_native_eq_params(&decode(&state(&[b])).unwrap());
        let q = params.iter().find(|(n, _)| n == "b1_q").map(|(_, v)| *v);
        assert_eq!(q, Some(1.0), "the stored 0.5 that most bands carry is Q = 1.0");
    }

    /// A spectral band crosses as a flag on top of its dynamics.
    ///
    /// Pro-Q 4's Spectral makes the band's dynamics act per-FFT-bin instead of
    /// as one gain ride. The region the spectral engine works from is built
    /// out of the band's range and threshold, both of which already cross, so
    /// the flag is the whole of what is left — and 42 of the 171 factory
    /// presets need it.
    #[test]
    fn a_spectral_band_carries_its_flag() {
        let mut plain = band_record(1.0, 1.0, 8.0, -3.0, 0.6, 0.0, 2.0, 2.0);
        plain[field::DYNAMIC_RANGE] = -6.0;
        let mut spectral = plain;
        spectral[field::SPECTRAL_ENABLED] = 1.0;
        spectral[field::SPECTRAL_DENSITY] = 80.0;

        let eq = decode(&state(&[plain, spectral])).unwrap();
        assert!(!eq.bands[0].spectral);
        assert!(eq.bands[1].spectral);
        assert!((eq.bands[1].spectral_density - 80.0).abs() < 1e-6);

        let params = to_native_eq_params(&eq);
        let get = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
        assert_eq!(get("b1_spectral"), None, "an ordinary band is not spectral");
        assert_eq!(get("b2_spectral"), Some(1.0));
        // And both still carry the dynamics the region is built from.
        assert_eq!(get("b2_dyn_range"), Some(-6.0));
        assert!(get("b2_dyn_thr").is_some());
    }
}
