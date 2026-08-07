//! Compressor hardware profiles — parameter mappings and constraints.
//!
//! A profile defines:
//! - Which controls appear (knobs, switches, stepped selectors)
//! - How each control maps to [`comp_dsp::CompChain`] parameters
//! - Constraints (locked ratios, attack/release curves, linked params)
//!
//! Profiles are pure data + mapping functions. No GUI, no framework deps.

pub mod cl1b;
pub mod control;
pub mod core;
pub mod dbx160;
pub mod distressor;
pub mod fairchild;
pub mod la2a;
pub mod manley;
pub mod presets;
pub mod ssl_bus;
pub mod urei_1176;

pub use self::core::{Constraint, ParamMapping, Profile, ProfileControl};
pub use cl1b::Cl1bProfile;
pub use control::ControlProfile;
pub use dbx160::Dbx160Profile;
pub use distressor::DistressorProfile;
pub use fairchild::Fairchild670Profile;
pub use la2a::La2aProfile;
pub use manley::ManleyVariMuProfile;
pub use presets::{all_factory_presets, FactoryPreset, PresetParam, FACTORY_PRESETS};
pub use ssl_bus::SslBusProfile;
pub use urei_1176::Urei1176Profile;

pub static CONTROL: ControlProfile = ControlProfile;
pub static LA2A: La2aProfile = La2aProfile;
pub static CL1B: Cl1bProfile = Cl1bProfile;
pub static FAIRCHILD_670: Fairchild670Profile = Fairchild670Profile;
pub static MANLEY_VARI_MU: ManleyVariMuProfile = ManleyVariMuProfile;
pub static SSL_BUS: SslBusProfile = SslBusProfile;
pub static DBX_160: Dbx160Profile = Dbx160Profile;
pub static DISTRESSOR: DistressorProfile = DistressorProfile;
/// The three finishes the FET limiter came in. Same circuit, same controls —
/// see [`Urei1176Profile`].
pub static UREI_1176: Urei1176Profile = Urei1176Profile {
    id: "urei_1176",
    name: "FET Limiter · Blackface",
};
pub static UREI_1176_SILVER: Urei1176Profile = Urei1176Profile {
    id: "urei_1176_silver",
    name: "FET Limiter · Silver",
};
pub static UREI_1176_LN: Urei1176Profile = Urei1176Profile {
    id: "urei_1176_ln",
    name: "FET Limiter · LN",
};

/// Every profile, in the order the UI lists them: the FTS surface first, then
/// each compressor family, and within a family the units in the order the
/// category cycles through them.
///
/// This order IS the `profile` parameter's value order, so **append only** —
/// hosts persist the parameter, and reordering silently repoints a saved
/// session at a different unit.
pub fn all_profiles() -> [&'static (dyn Profile + Sync); 11] {
    [
        &CONTROL,
        &UREI_1176,
        &LA2A,
        &CL1B,
        &FAIRCHILD_670,
        &MANLEY_VARI_MU,
        &SSL_BUS,
        &DBX_160,
        &DISTRESSOR,
        // Appended: the parameter's value order is this order, and the id is
        // what a session restores from either way.
        &UREI_1176_SILVER,
        &UREI_1176_LN,
        // NOTE: keep in sync with `CATEGORIES` below — the test
        // `every_profile_belongs_to_exactly_one_category` is the guard.
    ]
}

/// A compressor family — how the units are grouped in the UI, and the shape of
/// the rail: one button per family, clicking it again cycles the units inside.
///
/// The grouping is by *topology*, because that is what actually predicts how a
/// unit behaves: a FET is fast and gritty, an opto is slow and self-correcting,
/// a variable-mu glues, a VCA punches. Which specific unit you want inside the
/// family is a finer decision than which family you want, so it is the second
/// click rather than the first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Category {
    /// Stable id, used for test ids and rail keys.
    pub id: &'static str,
    /// "Opto".
    pub label: &'static str,
    /// Rail badge when no unit in the family is active.
    pub badge: &'static str,
    /// Profile ids, in cycling order.
    pub profiles: &'static [&'static str],
}

pub static CATEGORIES: &[Category] = &[
    Category {
        id: "main",
        label: "Main",
        badge: "MAIN",
        profiles: &["control"],
    },
    Category {
        id: "fet",
        label: "FET",
        badge: "FET",
        // Three finishes of one unit — the second click changes the paint,
        // not the circuit.
        profiles: &["urei_1176_silver", "urei_1176", "urei_1176_ln"],
    },
    Category {
        id: "opto",
        label: "Opto",
        badge: "OPT",
        profiles: &["la2a", "cl1b"],
    },
    Category {
        id: "vari_mu",
        label: "Vari-Mu",
        badge: "MU",
        profiles: &["fairchild670", "manley_vari_mu"],
    },
    Category {
        id: "vca",
        label: "VCA",
        badge: "VCA",
        profiles: &["ssl_bus", "dbx160"],
    },
    Category {
        id: "hybrid",
        label: "Hybrid",
        badge: "HYB",
        profiles: &["distressor"],
    },
];

/// The profile with this id, if there is one.
pub fn profile_by_id(id: &str) -> Option<&'static (dyn Profile + Sync)> {
    all_profiles().into_iter().find(|p| p.id() == id)
}

/// Index of a profile id in [`all_profiles`] — the value the `profile`
/// parameter holds.
pub fn profile_index(id: &str) -> Option<usize> {
    all_profiles().iter().position(|p| p.id() == id)
}

/// The category a profile belongs to, and its position within it.
pub fn category_of(profile_id: &str) -> Option<(usize, usize)> {
    CATEGORIES.iter().enumerate().find_map(|(ci, category)| {
        category
            .profiles
            .iter()
            .position(|id| *id == profile_id)
            .map(|vi| (ci, vi))
    })
}

pub fn map_control_value(
    profile: &dyn Profile,
    control_id: &str,
    normalized_value: f64,
) -> Option<Vec<(&'static str, f64)>> {
    let control = profile
        .controls()
        .iter()
        .find(|control| control.id == control_id)?;
    let normalized_value = normalized_value.clamp(0.0, 1.0);

    match &control.mapping {
        ParamMapping::Direct { param, range } => {
            let start = *range.start();
            let end = *range.end();
            Some(vec![(*param, start + (end - start) * normalized_value)])
        }
        ParamMapping::Stepped { param, values, .. } => {
            if values.is_empty() {
                return Some(Vec::new());
            }
            let index = (normalized_value * (values.len() - 1) as f64).round() as usize;
            Some(vec![(*param, values[index])])
        }
        ParamMapping::Compound { mappings, .. } => Some(
            mappings
                .iter()
                .map(|(param, map)| (*param, map(normalized_value)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORE_PARAMS: &[&str] = &[
        "threshold_db",
        "ratio",
        "attack_ms",
        "release_ms",
        "knee_db",
        "auto_makeup",
        "feedback",
        "channel_link",
        "detector_rms_mix",
        "inertia",
        "inertia_decay",
        "ceiling",
        "drive",
        "character_mode",
        "fold",
        "multiband_amount",
        "input_gain_db",
        "output_gain_db",
        "sidechain_freq",
        "sidechain_lowpass_freq",
        "range_db",
        "expander_threshold_db",
        "expander_ratio",
        "upward_threshold_db",
        "upward_ratio",
        "hold_ms",
        "lookahead_ms",
        "style",
        "profile",
    ];

    #[test]
    fn all_profiles_have_controls() {
        let profiles = all_profiles();
        let mut ids = Vec::new();

        for profile in profiles {
            assert!(!profile.id().is_empty());
            assert!(!profile.name().is_empty());
            assert!(
                !profile.controls().is_empty(),
                "{} should expose at least one control",
                profile.id()
            );
            assert!(
                ids.iter().all(|id| id != &profile.id()),
                "duplicate profile id {}",
                profile.id()
            );
            ids.push(profile.id());
        }
    }

    #[test]
    fn profile_mappings_reference_known_core_params() {
        for profile in all_profiles() {
            for control in profile.controls() {
                match &control.mapping {
                    ParamMapping::Direct { param, .. } | ParamMapping::Stepped { param, .. } => {
                        assert_known_param(profile.id(), control.id, param);
                    }
                    ParamMapping::Compound { mappings, .. } => {
                        for (param, _) in mappings.iter() {
                            assert_known_param(profile.id(), control.id, param);
                        }
                    }
                }
            }

            for constraint in profile.constraints() {
                match constraint {
                    Constraint::Fixed { param, .. }
                    | Constraint::Clamped { param, .. }
                    | Constraint::SteppedOnly { param, .. } => {
                        assert_known_param(profile.id(), "constraint", param);
                    }
                }
            }
        }
    }

    #[test]
    fn map_control_value_expands_direct_stepped_and_compound_controls() {
        assert_eq!(
            map_control_value(&CONTROL, "threshold_db", 0.5),
            Some(vec![("threshold_db", -30.0)])
        );
        assert_eq!(
            map_control_value(&SSL_BUS, "ratio", 0.6),
            Some(vec![("ratio", 4.0)])
        );

        let la2a = map_control_value(&LA2A, "peak_reduction", 1.0).unwrap();
        assert_eq!(la2a.len(), 5);
        assert!(la2a.contains(&("threshold_db", -48.0)));
        assert!(la2a.contains(&("ratio", 6.0)));
        assert!(la2a.contains(&("drive", 0.25)));

        let input = map_control_value(&UREI_1176, "input", 1.0).unwrap();
        assert_eq!(
            input,
            vec![
                ("input_gain_db", 24.0),
                ("threshold_db", -44.0),
                ("drive", 0.8)
            ]
        );
    }

    #[test]
    fn factory_presets_cover_core_workflows_and_known_params() {
        let presets = all_factory_presets();
        assert!(
            presets.len() >= 8,
            "factory presets should cover the main product workflows"
        );

        let mut ids = Vec::new();
        let mut saw_sidechain = false;
        let mut saw_expander = false;
        let mut saw_upward = false;
        let mut saw_parallel = false;

        for preset in presets {
            assert!(!preset.id.is_empty());
            assert!(!preset.name.is_empty());
            assert!(!preset.description.is_empty());
            assert!(
                ids.iter().all(|id| id != &preset.id),
                "duplicate preset id {}",
                preset.id
            );
            ids.push(preset.id);

            assert!(
                all_profiles()
                    .iter()
                    .any(|profile| profile.id() == preset.profile_id),
                "{} references unknown profile {}",
                preset.id,
                preset.profile_id
            );
            assert!(
                preset.params.len() >= 8,
                "{} should be a useful parameter snapshot",
                preset.id
            );

            for param in preset.params {
                assert_known_param(preset.id, "factory preset", param.param);
            }

            saw_sidechain |= preset
                .params
                .iter()
                .any(|param| param.param == "sidechain_lowpass_freq");
            saw_expander |= preset
                .params
                .iter()
                .any(|param| param.param == "expander_ratio");
            saw_upward |= preset
                .params
                .iter()
                .any(|param| param.param == "upward_ratio");
            saw_parallel |= preset
                .params
                .iter()
                .any(|param| param.param == "fold" && param.value < 0.75);
        }

        assert!(
            saw_sidechain,
            "preset pack should include keyed/sidechain use"
        );
        assert!(saw_expander, "preset pack should include expansion/gating");
        assert!(saw_upward, "preset pack should include upward compression");
        assert!(
            saw_parallel,
            "preset pack should include parallel compression"
        );
    }

    #[test]
    fn every_profile_belongs_to_exactly_one_category() {
        for profile in all_profiles() {
            let hits = CATEGORIES
                .iter()
                .filter(|c| c.profiles.contains(&profile.id()))
                .count();
            assert_eq!(
                hits, 1,
                "{} appears in {hits} categories, expected exactly 1",
                profile.id()
            );
        }
    }

    #[test]
    fn every_category_entry_names_a_real_profile() {
        for category in CATEGORIES {
            assert!(!category.profiles.is_empty(), "{} is empty", category.id);
            for id in category.profiles {
                assert!(
                    profile_by_id(id).is_some(),
                    "{} lists unknown profile {id}",
                    category.id
                );
            }
        }
    }

    #[test]
    fn a_profile_resolves_back_to_its_category_and_position() {
        assert_eq!(category_of("control").map(|(c, _)| CATEGORIES[c].id), Some("main"));
        assert_eq!(category_of("la2a"), Some((2, 0)));
        assert_eq!(category_of("cl1b"), Some((2, 1)));
        assert_eq!(category_of("dbx160").map(|(_, v)| v), Some(1));
        assert_eq!(category_of("nope"), None);
    }

    #[test]
    fn profile_indices_are_stable_and_start_with_the_fts_surface() {
        // The parameter's value order. Appending is fine; reordering silently
        // repoints saved sessions at a different unit.
        assert_eq!(profile_index("control"), Some(0));
        assert_eq!(profile_index("urei_1176"), Some(1));
        assert_eq!(profile_index("la2a"), Some(2));
    }

    fn assert_known_param(profile_id: &str, control_id: &str, param: &str) {
        assert!(
            CORE_PARAMS.contains(&param),
            "{profile_id}.{control_id} references unknown core param {param}"
        );
    }
}
