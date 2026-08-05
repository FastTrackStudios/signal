//! Profile view — renders a hardware profile's controls as a themed GUI.
//!
//! This is a generic renderer that takes a `Profile` definition and creates
//! appropriate UI controls for each `ProfileControl` entry.

use comp_profiles::{map_control_value, ParamMapping, Profile};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfileSkin {
    pub profile_id: &'static str,
    pub macro_group_label: &'static str,
    pub strip_groups: &'static [ProfileSkinGroup],
    pub text: &'static str,
    pub accent: &'static str,
    pub border: &'static str,
    pub highlight: &'static str,
    pub strip_bg: &'static str,
    pub overlay_gradient: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileSkinGroup {
    pub label: &'static str,
    pub controls: &'static [&'static str],
}

const CONTROL_STRIP: &[ProfileSkinGroup] = &[
    ProfileSkinGroup {
        label: "I/O",
        controls: &["input_gain_db", "output_gain_db", "auto_makeup"],
    },
    ProfileSkinGroup {
        label: "Mix",
        controls: &[
            "fold",
            "channel_link",
            "detector_rms_mix",
            "multiband_amount",
        ],
    },
    ProfileSkinGroup {
        label: "Character",
        controls: &[
            "feedback",
            "ceiling",
            "drive",
            "character_mode",
            "expander_threshold_db",
            "expander_ratio",
            "upward_threshold_db",
            "upward_ratio",
        ],
    },
    ProfileSkinGroup {
        label: "Sidechain",
        controls: &["sidechain_freq", "sidechain_lowpass_freq"],
    },
    ProfileSkinGroup {
        label: "Advanced",
        controls: &["inertia", "inertia_decay"],
    },
];

const PROFILE_STRIP: &[ProfileSkinGroup] = &[
    ProfileSkinGroup {
        label: "macro",
        controls: &["profile_drive", "profile_output"],
    },
    ProfileSkinGroup {
        label: "Timing",
        controls: &["attack_ms", "release_ms"],
    },
    ProfileSkinGroup {
        label: "Blend",
        controls: &[
            "fold",
            "channel_link",
            "detector_rms_mix",
            "multiband_amount",
        ],
    },
    ProfileSkinGroup {
        label: "Tone",
        controls: &["feedback", "ceiling", "drive", "character_mode"],
    },
    ProfileSkinGroup {
        label: "Key",
        controls: &["sidechain_freq", "sidechain_lowpass_freq", "lookahead_ms"],
    },
];

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileView {
    pub profile_id: &'static str,
    pub profile_name: &'static str,
    pub groups: Vec<ProfileControlGroup>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileControlGroup {
    pub label: &'static str,
    pub controls: Vec<ProfileControlView>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileControlView {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: ProfileControlKind,
    pub writes: Vec<ProfileParamWrite>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProfileControlKind {
    Knob {
        min: f64,
        max: f64,
        default: f64,
    },
    Selector {
        labels: Vec<&'static str>,
        default_index: usize,
    },
    Macro {
        min: f64,
        max: f64,
        default: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileParamWrite {
    pub param: &'static str,
    pub value: f64,
}

pub fn profile_skin(profile_id: &str) -> ProfileSkin {
    match profile_id {
        "la2a" => ProfileSkin {
            profile_id: "la2a",
            macro_group_label: "Opto",
            strip_groups: PROFILE_STRIP,
            text: "#f4ead4",
            accent: "#d6a84f",
            border: "rgba(214,168,79,0.42)",
            highlight: "rgba(255,236,184,0.16)",
            strip_bg: "linear-gradient(180deg, rgba(58,43,25,0.98), rgba(30,24,18,0.98))",
            overlay_gradient: "linear-gradient(to top, rgba(52,38,24,0.74) 0%, rgba(52,38,24,0.28) 55%, transparent 100%)",
        },
        "ssl_bus" => ProfileSkin {
            profile_id: "ssl_bus",
            macro_group_label: "Bus",
            strip_groups: PROFILE_STRIP,
            text: "#e8f1ec",
            accent: "#5ebc8a",
            border: "rgba(94,188,138,0.38)",
            highlight: "rgba(176,255,216,0.13)",
            strip_bg: "linear-gradient(180deg, rgba(24,43,36,0.98), rgba(15,27,24,0.98))",
            overlay_gradient: "linear-gradient(to top, rgba(18,43,34,0.72) 0%, rgba(18,43,34,0.26) 55%, transparent 100%)",
        },
        "cl1b" => ProfileSkin {
            profile_id: "cl1b",
            macro_group_label: "Opto",
            strip_groups: PROFILE_STRIP,
            text: "#eef4fa",
            accent: "#6fb3e8",
            border: "rgba(111,179,232,0.40)",
            highlight: "rgba(180,222,255,0.13)",
            strip_bg: "linear-gradient(180deg, rgba(28,60,86,0.98), rgba(16,34,50,0.98))",
            overlay_gradient: "linear-gradient(to top, rgba(20,48,70,0.74) 0%, rgba(20,48,70,0.26) 55%, transparent 100%)",
        },
        "fairchild670" => ProfileSkin {
            profile_id: "fairchild670",
            macro_group_label: "Vari-Mu",
            strip_groups: PROFILE_STRIP,
            text: "#f2ede0",
            accent: "#c9a86a",
            border: "rgba(201,168,106,0.40)",
            highlight: "rgba(255,236,196,0.13)",
            strip_bg: "linear-gradient(180deg, rgba(58,52,40,0.98), rgba(32,29,23,0.98))",
            overlay_gradient: "linear-gradient(to top, rgba(52,46,34,0.74) 0%, rgba(52,46,34,0.26) 55%, transparent 100%)",
        },
        "manley_vari_mu" => ProfileSkin {
            profile_id: "manley_vari_mu",
            macro_group_label: "Vari-Mu",
            strip_groups: PROFILE_STRIP,
            text: "#f0ece2",
            accent: "#e0b551",
            border: "rgba(224,181,81,0.40)",
            highlight: "rgba(255,229,163,0.13)",
            strip_bg: "linear-gradient(180deg, rgba(46,42,34,0.98), rgba(26,24,20,0.98))",
            overlay_gradient: "linear-gradient(to top, rgba(44,38,26,0.74) 0%, rgba(44,38,26,0.26) 55%, transparent 100%)",
        },
        "dbx160" => ProfileSkin {
            profile_id: "dbx160",
            macro_group_label: "VCA",
            strip_groups: PROFILE_STRIP,
            text: "#eef0f2",
            accent: "#7fa8d0",
            border: "rgba(127,168,208,0.40)",
            highlight: "rgba(196,220,245,0.13)",
            strip_bg: "linear-gradient(180deg, rgba(52,56,60,0.98), rgba(30,32,35,0.98))",
            overlay_gradient: "linear-gradient(to top, rgba(44,48,52,0.74) 0%, rgba(44,48,52,0.26) 55%, transparent 100%)",
        },
        "distressor" => ProfileSkin {
            profile_id: "distressor",
            macro_group_label: "Hybrid",
            strip_groups: PROFILE_STRIP,
            text: "#e4edf4",
            accent: "#54c6d8",
            border: "rgba(84,198,216,0.40)",
            highlight: "rgba(168,240,252,0.13)",
            strip_bg: "linear-gradient(180deg, rgba(22,44,56,0.98), rgba(13,26,34,0.98))",
            overlay_gradient: "linear-gradient(to top, rgba(18,40,52,0.74) 0%, rgba(18,40,52,0.26) 55%, transparent 100%)",
        },
        "urei_1176" => ProfileSkin {
            profile_id: "urei_1176",
            macro_group_label: "FET",
            strip_groups: PROFILE_STRIP,
            text: "#f3e9df",
            accent: "#d66f45",
            border: "rgba(214,111,69,0.40)",
            highlight: "rgba(255,189,142,0.13)",
            strip_bg: "linear-gradient(180deg, rgba(53,31,23,0.98), rgba(28,20,18,0.98))",
            overlay_gradient: "linear-gradient(to top, rgba(54,29,20,0.74) 0%, rgba(54,29,20,0.26) 55%, transparent 100%)",
        },
        _ => ProfileSkin {
            profile_id: "control",
            macro_group_label: "Profile",
            strip_groups: CONTROL_STRIP,
            text: "#f2f4f8",
            accent: "#8aa4ff",
            border: "rgba(148,163,184,0.30)",
            highlight: "rgba(255,255,255,0.10)",
            strip_bg: "rgba(16,18,22,0.98)",
            overlay_gradient: "linear-gradient(to top, rgba(0,0,0,0.65) 0%, rgba(0,0,0,0.25) 55%, transparent 100%)",
        },
    }
}

impl ProfileView {
    pub fn from_profile(profile: &dyn Profile) -> Self {
        let controls = profile
            .controls()
            .iter()
            .map(|control| {
                let default_normalized = match &control.mapping {
                    ParamMapping::Stepped { values, .. } if values.len() > 1 => 0.0,
                    _ => 0.5,
                };
                let writes = map_writes(profile, control.id, default_normalized);

                ProfileControlView {
                    id: control.id,
                    label: control.label,
                    kind: match &control.mapping {
                        ParamMapping::Direct { range, .. } => ProfileControlKind::Knob {
                            min: *range.start(),
                            max: *range.end(),
                            default: lerp(*range.start(), *range.end(), default_normalized),
                        },
                        ParamMapping::Stepped { labels, .. } => ProfileControlKind::Selector {
                            labels: labels.to_vec(),
                            default_index: 0,
                        },
                        ParamMapping::Compound { range, .. } => ProfileControlKind::Macro {
                            min: *range.start(),
                            max: *range.end(),
                            default: lerp(*range.start(), *range.end(), default_normalized),
                        },
                    },
                    writes,
                }
            })
            .collect();

        Self {
            profile_id: profile.id(),
            profile_name: profile.name(),
            groups: vec![ProfileControlGroup {
                label: profile.name(),
                controls,
            }],
        }
    }

    pub fn control(&self, id: &str) -> Option<&ProfileControlView> {
        self.groups
            .iter()
            .flat_map(|group| group.controls.iter())
            .find(|control| control.id == id)
    }
}

impl ProfileControlView {
    pub fn writes_for(
        &self,
        profile: &dyn Profile,
        normalized_value: f64,
    ) -> Vec<ProfileParamWrite> {
        map_writes(profile, self.id, normalized_value)
    }
}

fn map_writes(
    profile: &dyn Profile,
    control_id: &str,
    normalized_value: f64,
) -> Vec<ProfileParamWrite> {
    map_control_value(profile, control_id, normalized_value)
        .unwrap_or_default()
        .into_iter()
        .map(|(param, value)| ProfileParamWrite { param, value })
        .collect()
}

fn lerp(min: f64, max: f64, normalized: f64) -> f64 {
    min + (max - min) * normalized.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use comp_profiles::{all_profiles, LA2A, SSL_BUS, UREI_1176};

    const SKIN_CONTROL_PARAMS: &[&str] = &[
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
        "profile_drive",
        "profile_output",
    ];

    #[test]
    fn builds_view_for_every_profile() {
        for profile in all_profiles() {
            let view = ProfileView::from_profile(profile);
            assert_eq!(view.profile_id, profile.id());
            assert_eq!(view.profile_name, profile.name());
            assert_eq!(view.groups.len(), 1);
            assert_eq!(view.groups[0].controls.len(), profile.controls().len());
        }
    }

    #[test]
    fn classifies_control_shapes_for_skins() {
        let ssl = ProfileView::from_profile(&SSL_BUS);
        assert!(matches!(
            ssl.control("threshold").unwrap().kind,
            ProfileControlKind::Knob { .. }
        ));
        assert!(matches!(
            ssl.control("ratio").unwrap().kind,
            ProfileControlKind::Selector { .. }
        ));

        let la2a = ProfileView::from_profile(&LA2A);
        assert!(matches!(
            la2a.control("peak_reduction").unwrap().kind,
            ProfileControlKind::Macro { .. }
        ));
    }

    #[test]
    fn control_views_emit_parameter_writes() {
        let view = ProfileView::from_profile(&UREI_1176);
        let input = view.control("input").unwrap();
        assert_eq!(
            input.writes_for(&UREI_1176, 1.0),
            vec![
                ProfileParamWrite {
                    param: "input_gain_db",
                    value: 24.0,
                },
                ProfileParamWrite {
                    param: "threshold_db",
                    value: -44.0,
                },
                ProfileParamWrite {
                    param: "drive",
                    value: 0.8,
                },
            ]
        );
    }

    #[test]
    fn profile_skins_are_registered_for_all_profiles() {
        for profile in all_profiles() {
            let skin = profile_skin(profile.id());
            assert_eq!(skin.profile_id, profile.id());
            assert!(!skin.macro_group_label.is_empty());
            assert!(!skin.accent.is_empty());
            assert!(!skin.strip_bg.is_empty());
            assert!(!skin.strip_groups.is_empty());
        }
    }

    #[test]
    fn profile_skins_expose_expected_strip_shapes() {
        let control = profile_skin("control");
        assert_eq!(control.strip_groups[0].label, "I/O");
        assert!(control.strip_groups[0].controls.contains(&"auto_makeup"));

        let la2a = profile_skin("la2a");
        assert_eq!(la2a.strip_groups[0].label, "macro");
        assert_eq!(
            la2a.strip_groups[0].controls,
            &["profile_drive", "profile_output"]
        );
    }

    #[test]
    fn profile_skin_strips_reference_known_controls() {
        for profile in all_profiles() {
            let skin = profile_skin(profile.id());

            for group in skin.strip_groups {
                assert!(
                    !group.controls.is_empty(),
                    "{}.{} should expose at least one control",
                    skin.profile_id,
                    group.label
                );

                for control in group.controls {
                    assert!(
                        SKIN_CONTROL_PARAMS.contains(control),
                        "{}.{} references unknown skin control {}",
                        skin.profile_id,
                        group.label,
                        control
                    );
                }
            }
        }
    }
}
