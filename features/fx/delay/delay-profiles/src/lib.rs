//! Delay profiles — the six families a delay comes in, and which engine each
//! one is.
//!
//! `delay-dsp` ships fourteen engines, which is a list of implementations. The
//! useful question is not which one you want but *what happens to the repeat on
//! its way round*, and there are only six answers:
//!
//! Digital · Tape · Analog · Pitch · Rhythmic · Special
//!
//! Nothing happens to it (Digital). The medium colours it (Tape). It degrades
//! through a bucket brigade (Analog). It comes back at a different pitch
//! (Pitch). It comes back on a pattern (Rhythmic). Or it stops being a repeat
//! at all — played backwards, smeared across frequency, turned into a tail
//! (Special).
//!
//! Grouping this way collapses distinctions that are implementation details:
//! LoFi is the bucket-brigade path with the degradation turned up, not a
//! family of its own; Shimmer is the pitched delay at an octave; OilCan is a
//! tape delay with a stranger transport. The line that *is* worth drawing is
//! whether you get your signal back at all, which is what separates Rhythmic
//! from Special.
//!
//! Pure data — no GUI, no framework deps — like `reverb-profiles` and
//! `comp-profiles`.

use delay_dsp::DelayStyle;

/// One selectable delay: a name, the engine behind it, and its family.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Profile {
    /// Stable id. **Persisted** — a session records this, never an index, so
    /// adding or reordering profiles cannot change what an old project opens
    /// with.
    pub id: &'static str,
    /// What the rail and the panel call it.
    pub name: &'static str,
    /// The engine it selects.
    pub style: DelayStyle,
    /// One line on what it is for.
    pub voice: &'static str,
}

/// A rail entry: one family, and the profiles that cycle inside it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Category {
    pub id: &'static str,
    pub label: &'static str,
    pub badge: &'static str,
    /// Profile ids, in cycling order.
    pub profiles: &'static [&'static str],
}

pub static PROFILES: &[Profile] = &[
    // ── Digital: the repeat is the signal ────────────────────────────────
    Profile {
        id: "digital",
        name: "Digital",
        style: DelayStyle::Clean,
        voice: "Exact. What went in comes back, as many times as you ask.",
    },
    Profile {
        id: "filter",
        name: "Filtered",
        style: DelayStyle::Filter,
        voice: "The same repeat with tone controls inside the loop — each pass darker than the last.",
    },
    // ── Tape: a medium, with all that implies ────────────────────────────
    Profile {
        id: "tape",
        name: "Tape",
        style: DelayStyle::Tape,
        voice: "Wow, flutter and saturation. The repeats compress and blur as they pile up.",
    },
    Profile {
        id: "oilcan",
        name: "Oil Can",
        style: DelayStyle::OilCan,
        voice: "The odd one from the workshop: a smeared, chorused echo with no clean setting.",
    },
    // ── Analog: a bucket brigade, and how far it has fallen ──────────────
    Profile {
        id: "bbd",
        name: "Bucket Brigade",
        style: DelayStyle::Bbd,
        voice: "Charge handed down a chain of capacitors. Dark, noisy, and warm about it.",
    },
    Profile {
        id: "lofi",
        name: "Lo-Fi",
        style: DelayStyle::LoFi,
        voice: "The same circuit with the bandwidth and the bit depth taken away.",
    },
    // ── Pitch: it comes back somewhere else ──────────────────────────────
    Profile {
        id: "pitch",
        name: "Pitch",
        style: DelayStyle::Pitch,
        voice: "Each repeat shifted — a fifth up, an octave down, or something that does not settle.",
    },
    Profile {
        id: "shimmer",
        name: "Shimmer",
        style: DelayStyle::Shimmer,
        voice: "The octave case, fed back on itself: repeats that climb until they leave.",
    },
    // ── Rhythmic: it comes back on a pattern ─────────────────────────────
    Profile {
        id: "multitap",
        name: "Multi-Tap",
        style: DelayStyle::MultiTap,
        voice: "Several taps at once, placed where you want them across the stereo field.",
    },
    Profile {
        id: "rhythm",
        name: "Rhythmic",
        style: DelayStyle::Rhythm,
        voice: "Repeats on subdivisions — dotted eighths and triplets against the tempo.",
    },
    Profile {
        id: "drum",
        name: "Drum",
        style: DelayStyle::Drum,
        voice: "A groove rather than a subdivision: taps with their own accents and swing.",
    },
    // ── Special: it stops being a repeat ─────────────────────────────────
    Profile {
        id: "reverse",
        name: "Reverse",
        style: DelayStyle::Reverse,
        voice: "Buffered, turned around, played back. The repeat arrives before its own attack.",
    },
    Profile {
        id: "spectral",
        name: "Spectral",
        style: DelayStyle::Spectral,
        voice: "Every band delayed by a different amount — the sound smears across frequency.",
    },
    Profile {
        id: "reverb_delay",
        name: "Diffuse",
        style: DelayStyle::Reverb,
        voice: "The repeats diffuse into each other until there is a tail instead of an echo.",
    },
];

/// The rail, in order: the two that give your signal back untouched, then the
/// two media that colour it, then the two that change it, then the ones that
/// do not give it back at all.
pub static CATEGORIES: &[Category] = &[
    Category {
        id: "digital",
        label: "Digital",
        badge: "DIG",
        profiles: &["digital", "filter"],
    },
    Category {
        id: "tape",
        label: "Tape",
        badge: "TAPE",
        profiles: &["tape", "oilcan"],
    },
    Category {
        id: "analog",
        label: "Analog",
        badge: "ANA",
        profiles: &["bbd", "lofi"],
    },
    Category {
        id: "pitch",
        label: "Pitch",
        badge: "PCH",
        profiles: &["pitch", "shimmer"],
    },
    Category {
        id: "rhythmic",
        label: "Rhythmic",
        badge: "RHY",
        profiles: &["multitap", "rhythm", "drum"],
    },
    Category {
        id: "special",
        label: "Special",
        badge: "SPC",
        profiles: &["reverse", "spectral", "reverb_delay"],
    },
];

pub fn profile_by_id(id: &str) -> Option<&'static Profile> {
    PROFILES.iter().find(|p| p.id == id)
}

pub fn profile_index(id: &str) -> Option<usize> {
    PROFILES.iter().position(|p| p.id == id)
}

/// The family a profile belongs to, and its position inside it.
pub fn category_of(profile_id: &str) -> Option<(usize, usize)> {
    CATEGORIES.iter().enumerate().find_map(|(ci, category)| {
        category
            .profiles
            .iter()
            .position(|id| *id == profile_id)
            .map(|vi| (ci, vi))
    })
}

/// The profile index a rail click selects: clicking the family you are in
/// advances through it and wraps, clicking another lands on its first.
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
        assert_eq!(profile_by_id("time_machine"), None);
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
                assert!(
                    profile_by_id(id).is_some(),
                    "{} names {id}, which is not a profile",
                    category.id,
                );
            }
        }
    }

    /// The point of six families over fourteen engines: no engine becomes
    /// unreachable in the regrouping. Add one to `delay-dsp` and this fails
    /// until it has a home.
    #[test]
    fn every_engine_the_dsp_has_is_reachable_from_some_family() {
        for index in 0..DelayStyle::COUNT {
            let style = DelayStyle::from_index(index);
            assert!(
                PROFILES.iter().any(|p| p.style == style),
                "{style:?} is implemented and unreachable — give it a family",
            );
        }
    }

    /// …and no engine is offered twice, which would be two rail entries for
    /// one circuit.
    #[test]
    fn no_engine_is_offered_under_two_names() {
        for index in 0..DelayStyle::COUNT {
            let style = DelayStyle::from_index(index);
            let offering: Vec<_> = PROFILES
                .iter()
                .filter(|p| p.style == style)
                .map(|p| p.id)
                .collect();
            assert!(
                offering.len() <= 1,
                "{style:?} is offered as {offering:?}",
            );
        }
    }

    #[test]
    fn clicking_a_family_lands_on_it_and_clicking_again_cycles_inside_it() {
        let tape = CATEGORIES.iter().position(|c| c.id == "tape").unwrap();
        let first = rail_click_target(profile_index("digital").unwrap(), tape);
        assert_eq!(PROFILES[first].id, "tape");
        let second = rail_click_target(first, tape);
        assert_eq!(PROFILES[second].id, "oilcan");
        // …and wraps rather than running off the end.
        let wrapped = rail_click_target(second, tape);
        assert_eq!(PROFILES[wrapped].id, "tape");
    }
}
