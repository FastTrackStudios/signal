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
        voice: "A single-ended valve stage. Even harmonics, and a thickening you hear before you see it.",
    },
    Profile {
        id: "pentode",
        name: "Pentode",
        curve: SaturationCurve::Tube,
        voice: "Pushed harder and biased colder — the same warmth with an edge on top.",
    },
    // ── Tape: a soft knee into compression ───────────────────────────────
    Profile {
        id: "tape",
        name: "Tape",
        curve: SaturationCurve::Tape,
        voice: "The gentlest onset of the five: third harmonic and a little compression, arriving gradually.",
    },
    Profile {
        id: "tape_hot",
        name: "Hot Tape",
        curve: SaturationCurve::Tape,
        voice: "Printed loud. The knee is behind you and the top has started to go.",
    },
    // ── Transformer: hysteresis, not level ───────────────────────────────
    Profile {
        id: "transformer",
        name: "Transformer",
        curve: SaturationCurve::Tape,
        voice: "Saturates on transients and lows rather than on level — it blooms the bottom instead of thickening everything.",
    },
    // ── Transistor: hard-edged solid state ───────────────────────────────
    Profile {
        id: "transistor",
        name: "Transistor",
        curve: SaturationCurve::Tanh,
        voice: "Odd harmonics with a sharp corner. Modern, direct, and unromantic about it.",
    },
    Profile {
        id: "fuzz",
        name: "Fuzz",
        curve: SaturationCurve::Hard,
        voice: "The same circuit at its end: squared off, gated, and barely a waveform any more.",
    },
    // ── Digital: clipping and quantisation ───────────────────────────────
    Profile {
        id: "clip",
        name: "Clip",
        curve: SaturationCurve::Hard,
        voice: "A ceiling and nothing else. What happens above 0 dBFS when nobody was asked.",
    },
    Profile {
        id: "crush",
        name: "Bitcrush",
        curve: SaturationCurve::Hard,
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
