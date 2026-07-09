//! Compressor hardware profiles — parameter mappings and constraints.
//!
//! A profile defines:
//! - Which controls appear (knobs, switches, stepped selectors)
//! - How each control maps to [`comp_dsp::CompChain`] parameters
//! - Constraints (locked ratios, attack/release curves, linked params)
//!
//! Profiles are pure data + mapping functions. No GUI, no framework deps.

pub mod control;
pub mod core;
pub mod la2a;
pub mod presets;
pub mod ssl_bus;
pub mod urei_1176;

pub use self::core::{Constraint, ParamMapping, Profile, ProfileControl};
pub use control::ControlProfile;
pub use la2a::La2aProfile;
pub use presets::{all_factory_presets, FactoryPreset, PresetParam, FACTORY_PRESETS};
pub use ssl_bus::SslBusProfile;
pub use urei_1176::Urei1176Profile;

pub static CONTROL: ControlProfile = ControlProfile;
pub static LA2A: La2aProfile = La2aProfile;
pub static SSL_BUS: SslBusProfile = SslBusProfile;
pub static UREI_1176: Urei1176Profile = Urei1176Profile;

pub fn all_profiles() -> [&'static dyn Profile; 4] {
    [&CONTROL, &LA2A, &SSL_BUS, &UREI_1176]
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

    fn assert_known_param(profile_id: &str, control_id: &str, param: &str) {
        assert!(
            CORE_PARAMS.contains(&param),
            "{profile_id}.{control_id} references unknown core param {param}"
        );
    }
}
