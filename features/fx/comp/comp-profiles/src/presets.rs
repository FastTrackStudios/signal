//! Factory preset catalog for FTS Compressor.
//!
//! Presets are parameter snapshots in core-profile terms. Host-specific preset
//! serialization can consume this catalog without coupling the profile crate to
//! nih-plug or any single plugin format.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresetParam {
    pub param: &'static str,
    pub value: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FactoryPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub profile_id: &'static str,
    pub description: &'static str,
    pub params: &'static [PresetParam],
}

const VOCAL_OPTO: &[PresetParam] = &[
    p("profile", 1.0),
    p("threshold_db", -28.0),
    p("ratio", 4.0),
    p("attack_ms", 12.0),
    p("release_ms", 420.0),
    p("knee_db", 18.0),
    p("detector_rms_mix", 0.72),
    p("auto_makeup", 1.0),
    p("drive", 0.18),
    p("character_mode", 1.0),
    p("sidechain_freq", 95.0),
    p("range_db", 12.0),
];

const DRUM_FET: &[PresetParam] = &[
    p("profile", 3.0),
    p("threshold_db", -34.0),
    p("ratio", 12.0),
    p("attack_ms", 0.08),
    p("release_ms", 85.0),
    p("knee_db", 1.0),
    p("feedback", 0.38),
    p("drive", 0.62),
    p("character_mode", 6.0),
    p("fold", 0.55),
    p("range_db", 18.0),
];

const MIX_BUS_VCA: &[PresetParam] = &[
    p("profile", 2.0),
    p("threshold_db", -16.0),
    p("ratio", 2.0),
    p("attack_ms", 30.0),
    p("release_ms", 300.0),
    p("knee_db", 6.0),
    p("channel_link", 1.0),
    p("detector_rms_mix", 0.35),
    p("range_db", 4.0),
    p("style", 2.0),
];

const PARALLEL_SMASH: &[PresetParam] = &[
    p("profile", 0.0),
    p("threshold_db", -44.0),
    p("ratio", 20.0),
    p("attack_ms", 0.02),
    p("release_ms", 55.0),
    p("knee_db", 0.0),
    p("feedback", 0.1),
    p("drive", 0.72),
    p("character_mode", 5.0),
    p("fold", 0.35),
    p("range_db", 30.0),
];

const SIDECHAIN_DUCK: &[PresetParam] = &[
    p("profile", 0.0),
    p("threshold_db", -32.0),
    p("ratio", 8.0),
    p("attack_ms", 0.4),
    p("release_ms", 180.0),
    p("knee_db", 3.0),
    p("sidechain_freq", 120.0),
    p("sidechain_lowpass_freq", 2500.0),
    p("lookahead_ms", 2.0),
    p("range_db", 16.0),
];

const GATE_CLEANUP: &[PresetParam] = &[
    p("profile", 0.0),
    p("threshold_db", -8.0),
    p("ratio", 1.0),
    p("attack_ms", 4.0),
    p("release_ms", 160.0),
    p("expander_threshold_db", -46.0),
    p("expander_ratio", 5.0),
    p("hold_ms", 45.0),
    p("range_db", 24.0),
];

const UPWARD_LIFT: &[PresetParam] = &[
    p("profile", 0.0),
    p("threshold_db", -10.0),
    p("ratio", 1.4),
    p("attack_ms", 18.0),
    p("release_ms", 520.0),
    p("upward_threshold_db", -52.0),
    p("upward_ratio", 3.2),
    p("detector_rms_mix", 0.8),
    p("auto_makeup", 0.0),
    p("range_db", 8.0),
];

const DE_ESS_KEY: &[PresetParam] = &[
    p("profile", 0.0),
    p("threshold_db", -26.0),
    p("ratio", 6.0),
    p("attack_ms", 0.15),
    p("release_ms", 70.0),
    p("knee_db", 8.0),
    p("sidechain_freq", 300.0),
    p("sidechain_lowpass_freq", 8500.0),
    p("detector_rms_mix", 0.2),
    p("range_db", 8.0),
];

pub static FACTORY_PRESETS: &[FactoryPreset] = &[
    preset(
        "vocal-opto-leveler",
        "Vocal Opto Leveler",
        "la2a",
        "Smooth vocal leveling with program-dependent RMS detection.",
        VOCAL_OPTO,
    ),
    preset(
        "drum-fet-grab",
        "Drum FET Grab",
        "urei_1176",
        "Fast, colored compression for close drums and room mics.",
        DRUM_FET,
    ),
    preset(
        "mix-bus-vca-glue",
        "Mix Bus VCA Glue",
        "ssl_bus",
        "Low-range stereo bus compression with linked detection.",
        MIX_BUS_VCA,
    ),
    preset(
        "parallel-smash",
        "Parallel Smash",
        "control",
        "Heavy compression and drive blended back under dry signal.",
        PARALLEL_SMASH,
    ),
    preset(
        "sidechain-duck",
        "Sidechain Duck",
        "control",
        "Fast external-key ducking with lookahead and filtered detector.",
        SIDECHAIN_DUCK,
    ),
    preset(
        "gate-cleanup",
        "Gate Cleanup",
        "control",
        "Downward expansion for noise cleanup without hard chopping.",
        GATE_CLEANUP,
    ),
    preset(
        "upward-lift",
        "Upward Lift",
        "control",
        "Upward compression for quiet detail recovery.",
        UPWARD_LIFT,
    ),
    preset(
        "de-ess-key",
        "De-Ess Key",
        "control",
        "Filtered detector settings for sibilance control workflows.",
        DE_ESS_KEY,
    ),
];

pub fn all_factory_presets() -> &'static [FactoryPreset] {
    FACTORY_PRESETS
}

const fn p(param: &'static str, value: f64) -> PresetParam {
    PresetParam { param, value }
}

const fn preset(
    id: &'static str,
    name: &'static str,
    profile_id: &'static str,
    description: &'static str,
    params: &'static [PresetParam],
) -> FactoryPreset {
    FactoryPreset {
        id,
        name,
        profile_id,
        description,
        params,
    }
}
