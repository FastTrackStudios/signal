//! Saturation profiles — the five circuits, and the curve each one clips with.
//!
//! Distortion is usually sold as a list of pedals. What actually decides how a
//! saturator sounds is narrower than that: which harmonics it makes, and how
//! it behaves on the way into them. There are five answers worth separating.
//!
//! Tube · Tape · Transformer · Transistor · Digital
//!
//! - **Tube** is asymmetric — a single-ended stage clips one half of the wave
//!   before the other, and asymmetry is what makes *even* harmonics. That is
//!   the warmth, and it is a different mechanism from everything below.
//! - **Tape** is a soft knee into compression. Odd harmonics, mostly third,
//!   arriving gradually — it is the gentlest onset of the five.
//! - **Transformer** is hysteresis in a core: it saturates on transients and
//!   low frequencies rather than on level, which is why it blooms the bottom
//!   end instead of thickening everything.
//! - **Transistor** is hard-edged solid state. Odd harmonics with a sharp
//!   corner; fuzz is this taken to its end rather than a family of its own.
//! - **Digital** is not saturation at all in the analogue sense: clipping
//!   above 0 dBFS and quantisation below it. Aliasing and bit-rot rather than
//!   harmonics that belong to the note.
//!
//! Drive is deliberately not a family. It is the control every one of these
//! has, the way feedback is on every delay.
//!
//! Pure data — no GUI, no framework deps.

pub use saturate_dsp::preamp::SideShaper;
pub use saturate_dsp::SaturationCurve;

/// What a circuit's own two knobs are wired to.
///
/// The panels name these per profile — a tube's is "Heat", a fuzz's is
/// "Starve" — but a name is not a mapping, and two vocabularies for the same
/// choice is how the plugin ended up with a `ModelParam` table that had
/// drifted away from the rail. This is the single vocabulary: the panel
/// decides what to *call* a knob, this decides what it *does*.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Character {
    /// Not offered on this circuit.
    None,
    /// Per-side onset asymmetry — one half of the wave reaches its knee
    /// before the other. This is the even-harmonic control, and it is what a
    /// "Heat" or a "Print" knob is really riding.
    Skew,
    /// Where the knee sits without touching the small-signal gain: a bigger
    /// core, a higher rail, a tape biased hotter, a clipper's ceiling.
    Headroom,
    /// How long the stage stays bent after a transient — a valve's cathode
    /// recovers faster than a tape machine's, and hysteresis slower still.
    SagTime,
    /// Blend toward a hard rail. The corner solid state has and valves do not.
    Knee,
    /// Class-B crossover deadband: neither half conducting near zero. The
    /// buzz of an underbiased transistor stage, and the gate of a starved fuzz.
    Crossover,
    /// Word length, in bits.
    Bits,
    /// Sample-rate divisor.
    Rate,
}

/// The engine settings a profile *is*.
///
/// `saturate-dsp` exposes two stages — the class-A preamp and, for the digital
/// family, a quantiser. A profile is a point in that space, and this is the
/// whole of it: nothing about a circuit lives in the plugin shell any more.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Voicing {
    /// Transfer on the positive half of the wave…
    pub positive: SideShaper,
    /// …and on the negative half. **Which curve sits on which half is the
    /// difference between even and odd harmonics** — a single-ended stage is
    /// asymmetric because its two halves are not the same circuit.
    pub negative: SideShaper,
    /// Resting operating point, −1..1.
    pub q_point: f32,
    /// How far the operating point droops under program, 0..1.
    pub sag: f32,
    /// How long that droop takes to recover, in ms.
    pub sag_ms: f32,
    /// Resting per-side onset asymmetry, −1..1.
    pub skew: f32,
    /// Resting headroom multiplier.
    pub headroom: f32,
    /// Resting blend toward a hard rail, 0..1.
    pub knee: f32,
    /// Resting crossover deadband, 0..1.
    pub crossover: f32,
    /// How hard this circuit wants to be pushed for the same drive setting.
    /// A fuzz is not a tape machine with the knob turned up.
    pub drive_scale: f32,
    /// Which band meets the knee first, in dB. Negative drives the lows into
    /// the stage harder — the transformer's bloom; positive drives the top,
    /// which is how tape has always been flattered.
    pub tilt_db: f32,
    /// Whether the quantiser runs after the preamp.
    pub digital: bool,
    /// What the panel's first circuit knob is wired to…
    pub character_a: Character,
    /// …and its second.
    pub character_b: Character,
}

impl Voicing {
    /// A neutral stage: a wire with a knee on it. Every profile below states
    /// only what makes it itself.
    const fn base() -> Self {
        Self {
            positive: SideShaper::Clean,
            negative: SideShaper::Clean,
            q_point: 0.0,
            sag: 0.0,
            sag_ms: 30.0,
            skew: 0.0,
            headroom: 1.0,
            knee: 0.0,
            crossover: 0.0,
            drive_scale: 1.0,
            tilt_db: 0.0,
            digital: false,
            character_a: Character::None,
            character_b: Character::None,
        }
    }
}

/// One selectable saturator.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Profile {
    /// Stable id. **Persisted** — a session records this, never an index.
    pub id: &'static str,
    pub name: &'static str,
    /// The transfer curve it clips with.
    ///
    /// This is the simple [`saturate_dsp::Saturator`] path — one curve, no
    /// sides — and it is still the right answer for callers that want a
    /// saturator rather than a preamp. The plugin runs [`Self::voicing`].
    pub curve: SaturationCurve,
    /// The class-A stage this profile actually is.
    pub voicing: Voicing,
    /// One line on what it is for.
    pub voice: &'static str,
}

/// A rail entry: one circuit, and the variants that cycle inside it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Category {
    pub id: &'static str,
    pub label: &'static str,
    pub badge: &'static str,
    pub profiles: &'static [&'static str],
}

pub static PROFILES: &[Profile] = &[
    // ── Tube: asymmetric, and therefore even ─────────────────────────────
    Profile {
        id: "triode",
        name: "Triode",
        curve: SaturationCurve::Tube,
        // A single-ended triode: the grid side runs out of room early and
        // gently (Tube), the plate side stays firmer (OpAmp), and the stage
        // idles biased into the soft half. Skew is the same story told again
        // in the onset, which is what keeps H2 up even at Q = 0. The cathode
        // bypass sags quickly, so the warmth blooms under a loud passage
        // rather than sitting at a fixed level.
        voicing: Voicing {
            positive: SideShaper::Tube,
            negative: SideShaper::OpAmp,
            q_point: 0.20,
            sag: 0.45,
            sag_ms: 22.0,
            skew: 0.35,
            // Headroom is doing real work here rather than being decoration.
            // Measured against the harmonic probe, a stage sitting AT its
            // knee leads with H2; one driven well past it squares up and
            // leads with H3 like anything else would. A triode wants to live
            // around the knee at ordinary drive settings, which is what this
            // buys — and it is why "warm" and "loud" are not the same knob.
            headroom: 2.6,
            character_a: Character::Skew,     // "Heat"
            character_b: Character::Headroom, // "Grid" — grid conduction clamps the peak
            ..Voicing::base()
        },
        voice: "A single-ended valve stage. Even harmonics, and a thickening you hear before you see it.",
    },
    Profile {
        id: "pentode",
        name: "Pentode",
        // Not a Triode with a different name. Biased colder (a smaller Q, so
        // it idles further from the soft knee), the plate side stiffer still
        // (Diode's faster corner is the screen clamping), less skew and so
        // more odd than even, a stiffer supply that sags less, and driven
        // harder for the same knob — which is the "edge on top".
        curve: SaturationCurve::Tube,
        voicing: Voicing {
            positive: SideShaper::Tube,
            negative: SideShaper::Diode,
            q_point: 0.06,
            sag: 0.18,
            sag_ms: 12.0,
            skew: 0.18,
            // Less room than the Triode, and driven harder into what it has:
            // the same stage run past its knee, which is the "edge on top".
            headroom: 1.5,
            drive_scale: 1.4,
            character_a: Character::Skew,     // "Heat"
            character_b: Character::Headroom, // "Screen"
            ..Voicing::base()
        },
        voice: "Pushed harder and biased colder — the same warmth with an edge on top.",
    },
    // ── Tape: a soft knee into compression ───────────────────────────────
    Profile {
        id: "tape",
        name: "Tape",
        curve: SaturationCurve::Tape,
        // Symmetric — the third harmonic is the point — with the soft-knee
        // iron curve on both halves and only the faint asymmetry real tape
        // has. It arrives gradually because the drive is scaled back and the
        // headroom is up. The tilt is the classic record pre-emphasis: the
        // top meets the knee first, which is why tape softens cymbals and not
        // kick drums. Sag is slow: this is compression, not bloom.
        voicing: Voicing {
            positive: SideShaper::Transformer,
            negative: SideShaper::Transformer,
            sag: 0.30,
            sag_ms: 90.0,
            skew: 0.06,
            headroom: 1.35,
            drive_scale: 0.8,
            tilt_db: 3.5,
            character_a: Character::Headroom, // "Bias" — overbias buys headroom
            character_b: Character::SagTime,  // "Speed" — ips, as a recovery time
            ..Voicing::base()
        },
        voice: "The gentlest onset of the five: third harmonic and a little compression, arriving gradually.",
    },
    Profile {
        id: "tape_hot",
        name: "Hot Tape",
        curve: SaturationCurve::Tape,
        // The same machine printed 6 dB louder: less headroom, more drive,
        // deeper sag, and enough remanence asymmetry that the evens come up
        // too. "The top has started to go" is the tilt — the pre-emphasised
        // highs are the first thing into the knee.
        voicing: Voicing {
            positive: SideShaper::Transformer,
            negative: SideShaper::Transformer,
            q_point: 0.05,
            sag: 0.55,
            sag_ms: 70.0,
            skew: 0.16,
            headroom: 0.85,
            drive_scale: 1.7,
            tilt_db: 5.0,
            character_a: Character::Headroom, // "Bias"
            character_b: Character::Skew,     // "Print"
            ..Voicing::base()
        },
        voice: "Printed loud. The knee is behind you and the top has started to go.",
    },
    // ── Transformer: hysteresis, not level ───────────────────────────────
    Profile {
        id: "transformer",
        name: "Transformer",
        curve: SaturationCurve::Tape,
        // The one profile whose character is a *filter*, not a curve. A core
        // saturates on flux, and flux is the integral of voltage — so it is
        // the lows that reach it first. That is the negative tilt, and it is
        // the whole reason this blooms the bottom instead of thickening
        // everything. Hysteresis is memory, so the sag is slow and deep with
        // no bias offset behind it; the remanence gives it a slight lean.
        voicing: Voicing {
            positive: SideShaper::Transformer,
            negative: SideShaper::Transformer,
            sag: 0.40,
            sag_ms: 140.0,
            skew: 0.12,
            headroom: 1.15,
            tilt_db: -6.0,
            character_a: Character::Headroom, // "Core"
            character_b: Character::SagTime,  // "Hysteresis"
            ..Voicing::base()
        },
        voice: "Saturates on transients and lows rather than on level — it blooms the bottom instead of thickening everything.",
    },
    // ── Transistor: hard-edged solid state ───────────────────────────────
    Profile {
        id: "transistor",
        name: "Transistor",
        curve: SaturationCurve::Tanh,
        // Symmetric on purpose: a push-pull pair is two matched halves, and
        // matched halves make odd harmonics only. The sharp corner is the
        // knee already part-way up rather than a different curve, and there
        // is no sag at all — a regulated rail does not droop.
        voicing: Voicing {
            positive: SideShaper::OpAmp,
            negative: SideShaper::OpAmp,
            knee: 0.30,
            drive_scale: 1.2,
            character_a: Character::Knee,      // "Edge"
            character_b: Character::Crossover, // "Gate"
            ..Voicing::base()
        },
        voice: "Odd harmonics with a sharp corner. Modern, direct, and unromantic about it.",
    },
    Profile {
        id: "fuzz",
        name: "Fuzz",
        curve: SaturationCurve::Hard,
        // The same circuit with its supply pulled out from under it: hard on
        // one half, a diode knee on the other, biased well off centre,
        // sagging fast and deep, and already gating at rest. Drive is a
        // different order of magnitude — this is the only profile where the
        // knob's top end is meant to be unusable.
        voicing: Voicing {
            positive: SideShaper::Hard,
            negative: SideShaper::Diode,
            q_point: 0.32,
            sag: 0.45,
            sag_ms: 8.0,
            skew: 0.30,
            headroom: 0.7,
            knee: 0.45,
            crossover: 0.18,
            drive_scale: 3.0,
            character_a: Character::Knee,      // "Fuzz"
            character_b: Character::Crossover, // "Starve"
            ..Voicing::base()
        },
        voice: "The same circuit at its end: squared off, gated, and barely a waveform any more.",
    },
    // ── Digital: clipping and quantisation ───────────────────────────────
    Profile {
        id: "clip",
        name: "Clip",
        curve: SaturationCurve::Hard,
        // Nothing analogue about it: no bias, no sag, no knee to speak of.
        // A ceiling, and the only question is how sharp its corner is. The
        // quantiser runs but starts transparent — a clipper is the digital
        // family's honest member.
        voicing: Voicing {
            positive: SideShaper::Hard,
            negative: SideShaper::Hard,
            knee: 1.0,
            digital: true,
            character_a: Character::Headroom, // "Ceiling"
            character_b: Character::Knee,     // "Knee"
            ..Voicing::base()
        },
        voice: "A ceiling and nothing else. What happens above 0 dBFS when nobody was asked.",
    },
    Profile {
        id: "crush",
        name: "Bitcrush",
        curve: SaturationCurve::Hard,
        // The shapers barely matter here — the sound is downstream, in the
        // quantiser. Word length and rate are the two knobs, and neither is
        // a transfer curve: no shaper can make an alias.
        voicing: Voicing {
            positive: SideShaper::Hard,
            negative: SideShaper::Hard,
            knee: 1.0,
            digital: true,
            character_a: Character::Bits,
            character_b: Character::Rate,
            ..Voicing::base()
        },
        voice: "Quantisation rather than saturation: the steps between the samples become the sound.",
    },
];

/// The rail: warmest mechanism first, least musical last.
pub static CATEGORIES: &[Category] = &[
    Category { id: "tube", label: "Tube", badge: "TUBE", profiles: &["triode", "pentode"] },
    Category { id: "tape", label: "Tape", badge: "TAPE", profiles: &["tape", "tape_hot"] },
    Category { id: "transformer", label: "Transformer", badge: "XFMR", profiles: &["transformer"] },
    Category { id: "transistor", label: "Transistor", badge: "SS", profiles: &["transistor", "fuzz"] },
    Category { id: "digital", label: "Digital", badge: "DIG", profiles: &["clip", "crush"] },
];

/// The panel's knobs, normalised the way the parameters are.
///
/// **0.5 is the circuit as designed.** Every trim here is centred, so a
/// panel with everything at noon is the voicing above and nothing else, and
/// turning a knob is always a statement about *this* circuit rather than an
/// absolute number that means something different on each of the nine.
/// `drive` and `mix` are the exceptions: they are absolute, because they are
/// the two controls that mean the same thing on every saturator ever built.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Controls {
    pub drive: f32,
    pub bias: f32,
    pub sag: f32,
    pub tilt: f32,
    pub character_a: f32,
    pub character_b: f32,
    pub mix: f32,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            drive: 0.25,
            bias: 0.5,
            sag: 0.5,
            tilt: 0.5,
            character_a: 0.5,
            character_b: 0.5,
            mix: 1.0,
        }
    }
}

/// A centred trim: 0.5 is `at_noon`, and the ends reach `lo` and `hi`.
fn trim(knob: f32, at_noon: f32, lo: f32, hi: f32) -> f32 {
    let k = knob.clamp(0.0, 1.0);
    if k >= 0.5 {
        at_noon + (hi - at_noon) * (k - 0.5) * 2.0
    } else {
        lo + (at_noon - lo) * k * 2.0
    }
}

/// A centred trim on a multiplier, so the two halves of the knob feel the
/// same: 0.5 is `at_noon`, the ends are `at_noon` times 2^∓`octaves`.
fn trim_ratio(knob: f32, at_noon: f32, octaves: f32) -> f32 {
    let k = (knob.clamp(0.0, 1.0) - 0.5) * 2.0 * octaves;
    // 2^k without `powf` — the caller is on the audio thread's setter path,
    // but this crate is plain `std`-free arithmetic and it is cheap either way.
    at_noon * libm_exp2(k)
}

/// 2^x for |x| ≤ ~8, integer part by exponent surgery and a cubic on the
/// fraction (the same approach `saturate-dsp` uses; duplicated rather than
/// exported so the DSP crate keeps its surface small).
fn libm_exp2(x: f32) -> f32 {
    let mut xi = x as i32;
    if (xi as f32) > x {
        xi -= 1;
    }
    let xf = x - xi as f32;
    let frac = 1.0 + xf * (0.695_976 + xf * (0.226_174 + xf * 0.078_024));
    frac * f32::from_bits(((127 + xi.clamp(-126, 127)) as u32) << 23)
}

/// Point the engine at a profile.
///
/// This is the only place that knows how a knob reaches the DSP. The plugin
/// calls it every block; the editor calls it to draw the curve, which is why
/// the picture on the panel is the transfer function the audio thread will
/// actually apply rather than a drawing of one.
pub fn apply(
    profile: &Profile,
    controls: &Controls,
    pre: &mut saturate_dsp::preamp::ClassAPreamp,
    digital: &mut saturate_dsp::digital::DigitalStage,
) {
    use saturate_dsp::digital::BITS_OFF;
    use saturate_dsp::preamp::TILT_MAX_DB;

    let v = profile.voicing;

    pre.positive = v.positive;
    pre.negative = v.negative;
    pre.drive = (1.0 + controls.drive.clamp(0.0, 1.0) * 15.0) * v.drive_scale;
    pre.q_point = (v.q_point + (controls.bias.clamp(0.0, 1.0) - 0.5) * 1.2).clamp(-1.0, 1.0);
    pre.skew = v.skew;
    pre.headroom = v.headroom;
    pre.knee = v.knee;
    pre.crossover = v.crossover;
    pre.mix = 1.0;
    pre.output_gain = 1.0;

    // The quantiser starts transparent and only the profiles that name it
    // move it — a `Bits` knob on a valve would be a knob that does nothing.
    digital.bits = BITS_OFF;
    digital.rate = 1.0;
    digital.dither = 0.0;

    // The sag knob is "Dither" on the digital panel, because a quantiser has
    // no operating point to droop. Same knob, different machine.
    if v.digital {
        digital.dither = controls.sag.clamp(0.0, 1.0);
        pre.sag = 0.0;
    } else {
        pre.sag = trim(controls.sag, v.sag, 0.0, 1.0);
    }
    pre.set_sag_ms(v.sag_ms);
    pre.set_tilt_db(
        (v.tilt_db + (controls.tilt.clamp(0.0, 1.0) - 0.5) * 2.0 * TILT_MAX_DB)
            .clamp(-TILT_MAX_DB, TILT_MAX_DB),
    );

    apply_character(v.character_a, controls.character_a, &v, pre, digital);
    apply_character(v.character_b, controls.character_b, &v, pre, digital);
}

fn apply_character(
    role: Character,
    knob: f32,
    v: &Voicing,
    pre: &mut saturate_dsp::preamp::ClassAPreamp,
    digital: &mut saturate_dsp::digital::DigitalStage,
) {
    let knob = knob.clamp(0.0, 1.0);
    match role {
        Character::None => {}
        Character::Skew => pre.skew = trim(knob, v.skew, 0.0, 0.9),
        Character::Headroom => pre.headroom = trim_ratio(knob, v.headroom, 1.5),
        Character::SagTime => pre.set_sag_ms(trim_ratio(knob, v.sag_ms, 2.0)),
        Character::Knee => pre.knee = trim(knob, v.knee, 0.0, 1.0),
        Character::Crossover => pre.crossover = trim(knob, v.crossover, 0.0, 1.0),
        // Word length: squared so the useful, audibly-crushed half of the
        // knob is most of its travel, and fully up is genuinely off.
        Character::Bits => digital.bits = 2.0 + knob * knob * 22.0,
        // …and rate the other way round, so both digital knobs read
        // "clockwise is cleaner" like every other control on the panel.
        Character::Rate => {
            let down = 1.0 - knob;
            digital.rate = 1.0 + down * down * 31.0;
        }
    }
}

pub fn profile_by_id(id: &str) -> Option<&'static Profile> {
    PROFILES.iter().find(|p| p.id == id)
}

pub fn profile_index(id: &str) -> Option<usize> {
    PROFILES.iter().position(|p| p.id == id)
}

pub fn category_of(profile_id: &str) -> Option<(usize, usize)> {
    CATEGORIES.iter().enumerate().find_map(|(ci, category)| {
        category
            .profiles
            .iter()
            .position(|id| *id == profile_id)
            .map(|vi| (ci, vi))
    })
}

/// Clicking the family you are in advances through it and wraps; clicking
/// another lands on its first.
pub fn rail_click_target(current_index: usize, clicked_category: usize) -> usize {
    let current_id = PROFILES.get(current_index).map(|p| p.id).unwrap_or("");
    let Some(category) = CATEGORIES.get(clicked_category) else {
        return current_index;
    };
    let next_id = match category_of(current_id) {
        Some((ci, vi)) if ci == clicked_category => {
            category.profiles[(vi + 1) % category.profiles.len()]
        }
        _ => category.profiles[0],
    };
    profile_index(next_id).unwrap_or(current_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_has_a_unique_id_that_round_trips() {
        let mut seen = Vec::new();
        for profile in PROFILES {
            assert!(!seen.contains(&profile.id), "duplicate id {}", profile.id);
            seen.push(profile.id);
            assert_eq!(profile_by_id(profile.id).map(|p| p.id), Some(profile.id));
        }
        assert_eq!(profile_by_id("germanium"), None);
    }

    #[test]
    fn every_profile_is_in_exactly_one_family() {
        for profile in PROFILES {
            let found: Vec<_> = CATEGORIES
                .iter()
                .filter(|c| c.profiles.contains(&profile.id))
                .map(|c| c.id)
                .collect();
            assert_eq!(found.len(), 1, "{} is in {found:?}", profile.id);
        }
    }

    #[test]
    fn every_family_names_profiles_that_exist() {
        for category in CATEGORIES {
            assert!(!category.profiles.is_empty(), "{} is empty", category.id);
            for id in category.profiles {
                assert!(profile_by_id(id).is_some(), "{} names {id}", category.id);
            }
        }
    }

    /// The whole point of the tube family, asserted on the engine rather
    /// than on a name: a single-ended stage is asymmetric, and asymmetry is
    /// where even harmonics come from. Two identical shapers with no skew and
    /// no bias would be a class-AB stage wearing a valve's label.
    #[test]
    fn the_tube_family_is_asymmetric_in_the_engine_and_not_just_in_the_voice() {
        for id in CATEGORIES.iter().find(|c| c.id == "tube").unwrap().profiles {
            let v = profile_by_id(id).unwrap().voicing;
            assert!(
                v.positive != v.negative || v.skew != 0.0 || v.q_point != 0.0,
                "{id} is symmetric: it cannot make even harmonics",
            );
        }
    }

    /// …and the transistor's is the mirror claim. A push-pull pair is two
    /// matched halves; matched halves make odd harmonics only. Fuzz is the
    /// stated exception — it is the circuit failing, not working.
    #[test]
    fn the_transistor_proper_is_symmetric() {
        let v = profile_by_id("transistor").unwrap().voicing;
        assert_eq!(v.positive, v.negative);
        assert_eq!(v.skew, 0.0);
        assert_eq!(v.q_point, 0.0);
    }

    /// Triode and Pentode were the same settings under two names for a
    /// while. They are different valves; if they are ever the same point in
    /// the engine again, one of them is a lie.
    #[test]
    fn no_two_profiles_are_the_same_voicing() {
        for (i, a) in PROFILES.iter().enumerate() {
            for b in &PROFILES[i + 1..] {
                assert_ne!(
                    a.voicing, b.voicing,
                    "{} and {} are the same circuit under two names",
                    a.id, b.id,
                );
            }
        }
    }

    /// Only the digital family runs the quantiser, and all of it does.
    #[test]
    fn the_quantiser_runs_exactly_where_the_rail_says_digital() {
        for profile in PROFILES {
            let family = category_of(profile.id)
                .map(|(c, _)| CATEGORIES[c].id)
                .unwrap();
            assert_eq!(
                profile.voicing.digital,
                family == "digital",
                "{} is in {family} but digital = {}",
                profile.id,
                profile.voicing.digital,
            );
        }
    }

    /// A circuit knob wired to nothing is the drift this table exists to
    /// stop: the panels draw `character_a`/`character_b` on every family and
    /// name them per profile, so every profile has to say what they do.
    #[test]
    fn every_profile_wires_both_of_its_circuit_knobs() {
        for profile in PROFILES {
            let v = profile.voicing;
            assert_ne!(v.character_a, Character::None, "{} has no A", profile.id);
            assert_ne!(v.character_b, Character::None, "{} has no B", profile.id);
            assert_ne!(
                v.character_a, v.character_b,
                "{}'s two knobs do the same thing",
                profile.id,
            );
        }
    }

    /// Bits and Rate are quantiser settings; wiring them on an analogue
    /// circuit would be a knob that does nothing at all.
    #[test]
    fn quantiser_knobs_only_appear_on_a_profile_that_has_a_quantiser() {
        for profile in PROFILES {
            for role in [profile.voicing.character_a, profile.voicing.character_b] {
                if matches!(role, Character::Bits | Character::Rate) {
                    assert!(
                        profile.voicing.digital,
                        "{} offers {role:?} without a quantiser",
                        profile.id,
                    );
                }
            }
        }
    }

    /// The transformer's character is that the LOWS reach the core first.
    /// It is the only profile whose tilt leans that way, and if it ever
    /// stops doing so it has become a second tape machine.
    #[test]
    fn only_the_transformer_drives_its_lows_hardest() {
        for profile in PROFILES {
            if profile.id == "transformer" {
                assert!(profile.voicing.tilt_db < 0.0, "the core must see lows first");
            } else {
                assert!(
                    profile.voicing.tilt_db >= 0.0,
                    "{} also leans low — what makes it not a transformer?",
                    profile.id,
                );
            }
        }
    }

    /// Every voicing must be inside the ranges the engine clamps to. A
    /// setting the DSP silently corrects is a mapping nobody can read.
    #[test]
    fn every_voicing_is_in_range() {
        for profile in PROFILES {
            let v = profile.voicing;
            assert!((-1.0..=1.0).contains(&v.q_point), "{} q_point", profile.id);
            assert!((0.0..=1.0).contains(&v.sag), "{} sag", profile.id);
            assert!((1.0..=500.0).contains(&v.sag_ms), "{} sag_ms", profile.id);
            assert!((-1.0..=1.0).contains(&v.skew), "{} skew", profile.id);
            assert!((0.05..=16.0).contains(&v.headroom), "{} headroom", profile.id);
            assert!((0.0..=1.0).contains(&v.knee), "{} knee", profile.id);
            assert!((0.0..=1.0).contains(&v.crossover), "{} crossover", profile.id);
            assert!(v.drive_scale > 0.0, "{} drive_scale", profile.id);
            assert!(
                v.tilt_db.abs() <= saturate_dsp::preamp::TILT_MAX_DB,
                "{} tilt_db",
                profile.id,
            );
        }
    }

    /// Every curve the DSP implements is reachable from some profile.
    ///
    /// The reverse does **not** hold here, and that is worth being explicit
    /// about: `saturate-dsp` has four curves and this rail offers nine
    /// profiles, so several share one. A Triode and a Pentode are the same
    /// `Tube` transfer today and differ only in the bias and drive the panel
    /// sets — the distinction is real, the implementation has not caught up
    /// yet, and the profiles are where it will land when it does.
    #[test]
    fn every_curve_the_dsp_has_is_reachable() {
        for curve in [
            SaturationCurve::Tanh,
            SaturationCurve::Tape,
            SaturationCurve::Tube,
            SaturationCurve::Hard,
        ] {
            assert!(
                PROFILES.iter().any(|p| p.curve == curve),
                "{curve:?} is implemented and unreachable",
            );
        }
    }

    /// The mechanisms do not blur into each other: whatever the DSP grows,
    /// a tube profile must never be built on the symmetric curve, because
    /// asymmetry is the entire reason tubes make even harmonics.
    #[test]
    fn the_tube_family_is_asymmetric_and_the_digital_one_is_not_soft() {
        for id in CATEGORIES.iter().find(|c| c.id == "tube").unwrap().profiles {
            assert_eq!(profile_by_id(id).unwrap().curve, SaturationCurve::Tube);
        }
        for id in CATEGORIES.iter().find(|c| c.id == "digital").unwrap().profiles {
            assert_eq!(profile_by_id(id).unwrap().curve, SaturationCurve::Hard);
        }
    }

    // ── The mapping ──────────────────────────────────────────────────────

    use saturate_dsp::digital::{DigitalStage, BITS_OFF};
    use saturate_dsp::preamp::ClassAPreamp;

    fn engine(id: &str, controls: Controls) -> (ClassAPreamp, DigitalStage) {
        let mut pre = ClassAPreamp::new(48_000.0);
        let mut digital = DigitalStage::new();
        apply(profile_by_id(id).unwrap(), &controls, &mut pre, &mut digital);
        (pre, digital)
    }

    fn db(x: f32) -> f32 {
        20.0 * x.max(1.0e-9).log10()
    }

    /// Noon is the circuit as designed. This is the contract the whole panel
    /// rests on: if a centred knob moved the voicing, nobody could reason
    /// about what a profile is.
    #[test]
    fn every_knob_at_noon_is_the_voicing_and_nothing_else() {
        for profile in PROFILES {
            let (pre, digital) = engine(profile.id, Controls::default());
            let v = profile.voicing;
            assert_eq!(pre.positive, v.positive, "{}", profile.id);
            assert_eq!(pre.negative, v.negative, "{}", profile.id);
            assert!((pre.q_point - v.q_point).abs() < 1.0e-5, "{}", profile.id);
            assert!((pre.skew - v.skew).abs() < 1.0e-5, "{}", profile.id);
            assert!((pre.headroom - v.headroom).abs() < 1.0e-3, "{}", profile.id);
            assert!((pre.knee - v.knee).abs() < 1.0e-5, "{}", profile.id);
            assert!((pre.crossover - v.crossover).abs() < 1.0e-5, "{}", profile.id);
            assert!((pre.tilt_db() - v.tilt_db).abs() < 1.0e-4, "{}", profile.id);
            if !v.digital {
                assert!((pre.sag - v.sag).abs() < 1.0e-5, "{}", profile.id);
                assert!((pre.sag_ms() - v.sag_ms).abs() < 1.0e-2, "{}", profile.id);
                assert!(digital.is_transparent(), "{} crushes", profile.id);
            }
        }
    }

    /// An analogue profile must never touch the quantiser, at any knob
    /// position. Its two circuit knobs are wired elsewhere, and a stray
    /// bit-depth would be a sound nobody asked for.
    #[test]
    fn no_analogue_profile_can_reach_the_quantiser() {
        for profile in PROFILES.iter().filter(|p| !p.voicing.digital) {
            for k in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let controls = Controls {
                    drive: k,
                    bias: k,
                    sag: k,
                    tilt: k,
                    character_a: k,
                    character_b: k,
                    mix: k,
                };
                let (_, digital) = engine(profile.id, controls);
                assert!(
                    digital.is_transparent(),
                    "{} quantises at {k}",
                    profile.id,
                );
            }
        }
    }

    /// …and the digital ones must be able to. Fully clockwise is clean,
    /// fully anticlockwise is destroyed — the direction every other knob on
    /// the panel reads in.
    #[test]
    fn the_crusher_runs_clean_clockwise_and_coarse_anticlockwise() {
        let clean = Controls { character_a: 1.0, character_b: 1.0, ..Default::default() };
        let (_, digital) = engine("crush", clean);
        assert!(digital.is_transparent(), "fully up must be off");

        let coarse = Controls { character_a: 0.0, character_b: 0.0, ..Default::default() };
        let (_, digital) = engine("crush", coarse);
        assert!(digital.bits <= 3.0, "bits: {}", digital.bits);
        assert!(digital.rate >= 16.0, "rate: {}", digital.rate);
        assert!(digital.bits < BITS_OFF);
    }

    /// The claim the tube family is sold on, measured rather than asserted:
    /// at the settings it ships with, a Triode makes more 2nd than 3rd and a
    /// Transistor does the reverse. If a refactor ever swaps the shapers
    /// around — or quietly drives the valve past its knee, where every
    /// circuit squares up and sounds the same — this is what notices.
    #[test]
    fn a_triode_is_even_and_a_transistor_is_odd() {
        let spectrum = |id: &str| {
            let (pre, _) = engine(id, Controls::default());
            let mut h = [0.0f32; 6];
            saturate_dsp::preamp::analysis::harmonic_spectrum(&pre, &mut h);
            h
        };
        let tube = spectrum("triode");
        assert!(
            db(tube[1]) > db(tube[2]),
            "a triode leads with its 2nd: H2={} H3={}",
            db(tube[1]),
            db(tube[2]),
        );
        let ss = spectrum("transistor");
        assert!(
            db(ss[2]) > db(ss[1]) + 20.0,
            "a matched pair makes odd harmonics: H2={} H3={}",
            db(ss[1]),
            db(ss[2]),
        );
    }

    /// Heat is the even-harmonic knob, and it has to actually be one.
    #[test]
    fn heat_rides_the_even_harmonics() {
        let h2 = |knob: f32| {
            let (pre, _) = engine(
                "triode",
                Controls { character_a: knob, ..Default::default() },
            );
            let mut h = [0.0f32; 4];
            saturate_dsp::preamp::analysis::harmonic_spectrum(&pre, &mut h);
            db(h[1])
        };
        assert!(
            h2(1.0) > h2(0.0) + 6.0,
            "Heat must move H2: {} → {} dB",
            h2(0.0),
            h2(1.0),
        );
    }

    /// Whatever the knobs are doing, no profile may run away. A saturator
    /// that can be driven past its own ceiling is a speaker repair.
    #[test]
    fn nothing_explodes_at_any_setting() {
        for profile in PROFILES {
            for k in [0.0, 0.5, 1.0] {
                let controls = Controls {
                    drive: 1.0,
                    bias: k,
                    sag: k,
                    tilt: k,
                    character_a: k,
                    character_b: k,
                    mix: 1.0,
                };
                let (mut pre, mut digital) = engine(profile.id, controls);
                let mut worst = 0.0f32;
                for i in 0..4_800 {
                    let x = (std::f32::consts::TAU * 220.0 * i as f32 / 48_000.0).sin();
                    let mut y = pre.process(0, x);
                    if profile.voicing.digital {
                        y = digital.process(0, y);
                    }
                    assert!(y.is_finite(), "{} went non-finite at {k}", profile.id);
                    worst = worst.max(y.abs());
                }
                assert!(worst < 4.0, "{} reached {worst} at {k}", profile.id);
            }
        }
    }

    #[test]
    fn clicking_a_family_lands_on_it_and_clicking_again_cycles_inside_it() {
        let tube = CATEGORIES.iter().position(|c| c.id == "tube").unwrap();
        let first = rail_click_target(profile_index("tape").unwrap(), tube);
        assert_eq!(PROFILES[first].id, "triode");
        let second = rail_click_target(first, tube);
        assert_eq!(PROFILES[second].id, "pentode");
        let wrapped = rail_click_target(second, tube);
        assert_eq!(PROFILES[wrapped].id, "triode");
    }
}
