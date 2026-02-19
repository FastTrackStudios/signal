//! Neural DSP "Archetype: John Mayer X" — block and module presets.
//!
//! A single plugin instance with ~204 parameters decomposed into virtual
//! modules and blocks for the grid UI. Each virtual block becomes a real
//! `Preset` with named snapshots; each virtual module becomes a `ModulePreset`
//! referencing those block presets.

use signal_proto::{
    seed_id, Block, BlockParameter, BlockType, Module, ModuleBlock, ModuleBlockSource,
    ModulePreset, ModuleSnapshot, ModuleType, Preset, PresetId, Snapshot, SnapshotId,
};

// ═══════════════════════════════════════════════════════════════════
//  Block presets (one per virtual block)
// ═══════════════════════════════════════════════════════════════════

pub fn block_presets() -> Vec<Preset> {
    vec![
        justa_boost(),
        antelope_filter(),
        halfman_od(),
        tealbreaker(),
        millipede_delay(),
        harmonic_tremolo(),
        spring_reverb(),
        jm_amp(),
        jm_cab(),
        jm_eq(),
        dream_delay(),
        studio_verb(),
    ]
}

// ─── Pedals ─────────────────────────────────────────────────────

fn justa_boost() -> Preset {
    Preset::new(
        seed_id("jm-justa-boost"),
        "Justa Boost",
        BlockType::Boost,
        Snapshot::new(
            seed_id("jm-justa-boost-default"),
            "Default",
            Block::from_parameters(vec![
                BlockParameter::new("level", "Level", 0.50),
                BlockParameter::new("tone", "Tone", 0.50),
                BlockParameter::new("on-off", "On/Off", 1.0),
            ]),
        ),
        vec![
            Snapshot::new(
                seed_id("jm-justa-boost-clean"),
                "Clean Lift",
                Block::from_parameters(vec![
                    BlockParameter::new("level", "Level", 0.65),
                    BlockParameter::new("tone", "Tone", 0.45),
                    BlockParameter::new("on-off", "On/Off", 1.0),
                ]),
            ),
            Snapshot::new(
                seed_id("jm-justa-boost-edge"),
                "Edge",
                Block::from_parameters(vec![
                    BlockParameter::new("level", "Level", 0.78),
                    BlockParameter::new("tone", "Tone", 0.60),
                    BlockParameter::new("on-off", "On/Off", 1.0),
                ]),
            ),
        ],
    )
}

fn antelope_filter() -> Preset {
    Preset::new(
        seed_id("jm-antelope-filter"),
        "Antelope Filter",
        BlockType::Filter,
        Snapshot::new(
            seed_id("jm-antelope-filter-default"),
            "Default",
            Block::from_parameters(vec![
                BlockParameter::new("frequency", "Frequency", 0.50),
                BlockParameter::new("resonance", "Resonance", 0.30),
                BlockParameter::new("on-off", "On/Off", 0.0),
            ]),
        ),
        vec![Snapshot::new(
            seed_id("jm-antelope-filter-sweep"),
            "Sweep",
            Block::from_parameters(vec![
                BlockParameter::new("frequency", "Frequency", 0.70),
                BlockParameter::new("resonance", "Resonance", 0.55),
                BlockParameter::new("on-off", "On/Off", 1.0),
            ]),
        )],
    )
}

fn halfman_od() -> Preset {
    Preset::new(
        seed_id("jm-halfman-od"),
        "Halfman OD",
        BlockType::Drive,
        Snapshot::new(
            seed_id("jm-halfman-od-default"),
            "Default",
            Block::from_parameters(vec![
                BlockParameter::new("gain", "Gain", 0.40),
                BlockParameter::new("tone", "Tone", 0.50),
                BlockParameter::new("volume", "Volume", 0.60),
                BlockParameter::new("on-off", "On/Off", 0.0),
            ]),
        ),
        vec![
            Snapshot::new(
                seed_id("jm-halfman-od-crunch"),
                "Crunch",
                Block::from_parameters(vec![
                    BlockParameter::new("gain", "Gain", 0.62),
                    BlockParameter::new("tone", "Tone", 0.55),
                    BlockParameter::new("volume", "Volume", 0.55),
                    BlockParameter::new("on-off", "On/Off", 1.0),
                ]),
            ),
            Snapshot::new(
                seed_id("jm-halfman-od-lead"),
                "Lead",
                Block::from_parameters(vec![
                    BlockParameter::new("gain", "Gain", 0.78),
                    BlockParameter::new("tone", "Tone", 0.48),
                    BlockParameter::new("volume", "Volume", 0.50),
                    BlockParameter::new("on-off", "On/Off", 1.0),
                ]),
            ),
        ],
    )
}

fn tealbreaker() -> Preset {
    Preset::new(
        seed_id("jm-tealbreaker"),
        "Tealbreaker",
        BlockType::Drive,
        Snapshot::new(
            seed_id("jm-tealbreaker-default"),
            "Default",
            Block::from_parameters(vec![
                BlockParameter::new("drive", "Drive", 0.50),
                BlockParameter::new("tone", "Tone", 0.50),
                BlockParameter::new("level", "Level", 0.50),
                BlockParameter::new("on-off", "On/Off", 1.0),
            ]),
        ),
        vec![
            Snapshot::new(
                seed_id("jm-tealbreaker-edge-of-breakup"),
                "Edge of Breakup",
                Block::from_parameters(vec![
                    BlockParameter::new("drive", "Drive", 0.35),
                    BlockParameter::new("tone", "Tone", 0.55),
                    BlockParameter::new("level", "Level", 0.58),
                    BlockParameter::new("on-off", "On/Off", 1.0),
                ]),
            ),
            Snapshot::new(
                seed_id("jm-tealbreaker-pushed"),
                "Pushed",
                Block::from_parameters(vec![
                    BlockParameter::new("drive", "Drive", 0.72),
                    BlockParameter::new("tone", "Tone", 0.45),
                    BlockParameter::new("level", "Level", 0.48),
                    BlockParameter::new("on-off", "On/Off", 1.0),
                ]),
            ),
        ],
    )
}

fn millipede_delay() -> Preset {
    Preset::new(
        seed_id("jm-millipede-delay"),
        "Millipede Delay",
        BlockType::Delay,
        Snapshot::new(
            seed_id("jm-millipede-delay-default"),
            "Default",
            Block::from_parameters(vec![
                BlockParameter::new("time", "Time", 0.40),
                BlockParameter::new("feedback", "Feedback", 0.30),
                BlockParameter::new("mix", "Mix", 0.25),
                BlockParameter::new("on-off", "On/Off", 0.0),
            ]),
        ),
        vec![Snapshot::new(
            seed_id("jm-millipede-delay-slapback"),
            "Slapback",
            Block::from_parameters(vec![
                BlockParameter::new("time", "Time", 0.18),
                BlockParameter::new("feedback", "Feedback", 0.15),
                BlockParameter::new("mix", "Mix", 0.35),
                BlockParameter::new("on-off", "On/Off", 1.0),
            ]),
        )],
    )
}

// ─── Pre-FX ─────────────────────────────────────────────────────

fn harmonic_tremolo() -> Preset {
    Preset::new(
        seed_id("jm-harmonic-tremolo"),
        "Harmonic Tremolo",
        BlockType::Tremolo,
        Snapshot::new(
            seed_id("jm-harmonic-tremolo-default"),
            "Default",
            Block::from_parameters(vec![
                BlockParameter::new("rate", "Rate", 0.30),
                BlockParameter::new("depth", "Depth", 0.60),
                BlockParameter::new("mix", "Mix", 0.50),
                BlockParameter::new("on-off", "On/Off", 0.0),
            ]),
        ),
        vec![Snapshot::new(
            seed_id("jm-harmonic-tremolo-slow-pulse"),
            "Slow Pulse",
            Block::from_parameters(vec![
                BlockParameter::new("rate", "Rate", 0.15),
                BlockParameter::new("depth", "Depth", 0.80),
                BlockParameter::new("mix", "Mix", 0.65),
                BlockParameter::new("on-off", "On/Off", 1.0),
            ]),
        )],
    )
}

fn spring_reverb() -> Preset {
    Preset::new(
        seed_id("jm-spring-reverb"),
        "Spring Reverb",
        BlockType::Reverb,
        Snapshot::new(
            seed_id("jm-spring-reverb-default"),
            "Default",
            Block::from_parameters(vec![
                BlockParameter::new("decay", "Decay", 0.40),
                BlockParameter::new("tone", "Tone", 0.50),
                BlockParameter::new("mix", "Mix", 0.30),
                BlockParameter::new("on-off", "On/Off", 0.0),
            ]),
        ),
        vec![Snapshot::new(
            seed_id("jm-spring-reverb-drip"),
            "Surf Drip",
            Block::from_parameters(vec![
                BlockParameter::new("decay", "Decay", 0.65),
                BlockParameter::new("tone", "Tone", 0.55),
                BlockParameter::new("mix", "Mix", 0.55),
                BlockParameter::new("on-off", "On/Off", 1.0),
            ]),
        )],
    )
}

// ─── Amp ────────────────────────────────────────────────────────

fn jm_amp() -> Preset {
    Preset::new(
        seed_id("jm-amp"),
        "JM Amp",
        BlockType::Amp,
        Snapshot::new(
            seed_id("jm-amp-default"),
            "Default",
            Block::from_parameters(vec![
                BlockParameter::new("gain", "Gain", 0.45),
                BlockParameter::new("bass", "Bass", 0.50),
                BlockParameter::new("mid", "Mid", 0.55),
                BlockParameter::new("treble", "Treble", 0.60),
                BlockParameter::new("presence", "Presence", 0.50),
                BlockParameter::new("master", "Master", 0.50),
            ]),
        ),
        vec![
            Snapshot::new(
                seed_id("jm-amp-clean"),
                "Crystal Clean",
                Block::from_parameters(vec![
                    BlockParameter::new("gain", "Gain", 0.25),
                    BlockParameter::new("bass", "Bass", 0.45),
                    BlockParameter::new("mid", "Mid", 0.50),
                    BlockParameter::new("treble", "Treble", 0.65),
                    BlockParameter::new("presence", "Presence", 0.55),
                    BlockParameter::new("master", "Master", 0.55),
                ]),
            ),
            Snapshot::new(
                seed_id("jm-amp-crunch"),
                "Crunch",
                Block::from_parameters(vec![
                    BlockParameter::new("gain", "Gain", 0.62),
                    BlockParameter::new("bass", "Bass", 0.52),
                    BlockParameter::new("mid", "Mid", 0.58),
                    BlockParameter::new("treble", "Treble", 0.55),
                    BlockParameter::new("presence", "Presence", 0.48),
                    BlockParameter::new("master", "Master", 0.48),
                ]),
            ),
            Snapshot::new(
                seed_id("jm-amp-lead"),
                "Lead",
                Block::from_parameters(vec![
                    BlockParameter::new("gain", "Gain", 0.75),
                    BlockParameter::new("bass", "Bass", 0.48),
                    BlockParameter::new("mid", "Mid", 0.62),
                    BlockParameter::new("treble", "Treble", 0.52),
                    BlockParameter::new("presence", "Presence", 0.45),
                    BlockParameter::new("master", "Master", 0.45),
                ]),
            ),
        ],
    )
}

// ─── Cab ────────────────────────────────────────────────────────

fn jm_cab() -> Preset {
    Preset::new(
        seed_id("jm-cab"),
        "JM Cabinet",
        BlockType::Cabinet,
        Snapshot::new(
            seed_id("jm-cab-default"),
            "Default",
            Block::from_parameters(vec![
                BlockParameter::new("mic-position", "Mic Position", 0.50),
                BlockParameter::new("room", "Room", 0.30),
                BlockParameter::new("low-cut", "Low Cut", 0.20),
                BlockParameter::new("high-cut", "High Cut", 0.80),
            ]),
        ),
        vec![
            Snapshot::new(
                seed_id("jm-cab-close"),
                "Close Mic",
                Block::from_parameters(vec![
                    BlockParameter::new("mic-position", "Mic Position", 0.25),
                    BlockParameter::new("room", "Room", 0.15),
                    BlockParameter::new("low-cut", "Low Cut", 0.25),
                    BlockParameter::new("high-cut", "High Cut", 0.85),
                ]),
            ),
            Snapshot::new(
                seed_id("jm-cab-room"),
                "Room",
                Block::from_parameters(vec![
                    BlockParameter::new("mic-position", "Mic Position", 0.65),
                    BlockParameter::new("room", "Room", 0.60),
                    BlockParameter::new("low-cut", "Low Cut", 0.18),
                    BlockParameter::new("high-cut", "High Cut", 0.75),
                ]),
            ),
        ],
    )
}

// ─── EQ ─────────────────────────────────────────────────────────

fn jm_eq() -> Preset {
    Preset::new(
        seed_id("jm-eq"),
        "JM EQ",
        BlockType::Eq,
        Snapshot::new(
            seed_id("jm-eq-default"),
            "Default",
            Block::from_parameters(vec![
                BlockParameter::new("low", "Low", 0.50),
                BlockParameter::new("low-mid", "Low-Mid", 0.50),
                BlockParameter::new("high-mid", "High-Mid", 0.50),
                BlockParameter::new("high", "High", 0.50),
                BlockParameter::new("on-off", "On/Off", 1.0),
            ]),
        ),
        vec![Snapshot::new(
            seed_id("jm-eq-presence-cut"),
            "Presence Cut",
            Block::from_parameters(vec![
                BlockParameter::new("low", "Low", 0.48),
                BlockParameter::new("low-mid", "Low-Mid", 0.52),
                BlockParameter::new("high-mid", "High-Mid", 0.42),
                BlockParameter::new("high", "High", 0.55),
                BlockParameter::new("on-off", "On/Off", 1.0),
            ]),
        )],
    )
}

// ─── Post-FX ────────────────────────────────────────────────────

fn dream_delay() -> Preset {
    Preset::new(
        seed_id("jm-dream-delay"),
        "Dream Delay",
        BlockType::Delay,
        Snapshot::new(
            seed_id("jm-dream-delay-default"),
            "Default",
            Block::from_parameters(vec![
                BlockParameter::new("time", "Time", 0.50),
                BlockParameter::new("feedback", "Feedback", 0.40),
                BlockParameter::new("mod-rate", "Mod Rate", 0.30),
                BlockParameter::new("mod-depth", "Mod Depth", 0.20),
                BlockParameter::new("mix", "Mix", 0.30),
                BlockParameter::new("on-off", "On/Off", 1.0),
            ]),
        ),
        vec![
            Snapshot::new(
                seed_id("jm-dream-delay-ambient"),
                "Ambient",
                Block::from_parameters(vec![
                    BlockParameter::new("time", "Time", 0.65),
                    BlockParameter::new("feedback", "Feedback", 0.55),
                    BlockParameter::new("mod-rate", "Mod Rate", 0.20),
                    BlockParameter::new("mod-depth", "Mod Depth", 0.35),
                    BlockParameter::new("mix", "Mix", 0.45),
                    BlockParameter::new("on-off", "On/Off", 1.0),
                ]),
            ),
            Snapshot::new(
                seed_id("jm-dream-delay-dotted"),
                "Dotted Eighth",
                Block::from_parameters(vec![
                    BlockParameter::new("time", "Time", 0.375),
                    BlockParameter::new("feedback", "Feedback", 0.35),
                    BlockParameter::new("mod-rate", "Mod Rate", 0.25),
                    BlockParameter::new("mod-depth", "Mod Depth", 0.15),
                    BlockParameter::new("mix", "Mix", 0.28),
                    BlockParameter::new("on-off", "On/Off", 1.0),
                ]),
            ),
        ],
    )
}

fn studio_verb() -> Preset {
    Preset::new(
        seed_id("jm-studio-verb"),
        "Studio Verb",
        BlockType::Reverb,
        Snapshot::new(
            seed_id("jm-studio-verb-default"),
            "Default",
            Block::from_parameters(vec![
                BlockParameter::new("decay", "Decay", 0.50),
                BlockParameter::new("pre-delay", "Pre-Delay", 0.20),
                BlockParameter::new("damping", "Damping", 0.50),
                BlockParameter::new("size", "Size", 0.60),
                BlockParameter::new("mix", "Mix", 0.25),
                BlockParameter::new("on-off", "On/Off", 1.0),
            ]),
        ),
        vec![
            Snapshot::new(
                seed_id("jm-studio-verb-room"),
                "Room",
                Block::from_parameters(vec![
                    BlockParameter::new("decay", "Decay", 0.30),
                    BlockParameter::new("pre-delay", "Pre-Delay", 0.10),
                    BlockParameter::new("damping", "Damping", 0.55),
                    BlockParameter::new("size", "Size", 0.35),
                    BlockParameter::new("mix", "Mix", 0.20),
                    BlockParameter::new("on-off", "On/Off", 1.0),
                ]),
            ),
            Snapshot::new(
                seed_id("jm-studio-verb-hall"),
                "Hall",
                Block::from_parameters(vec![
                    BlockParameter::new("decay", "Decay", 0.72),
                    BlockParameter::new("pre-delay", "Pre-Delay", 0.30),
                    BlockParameter::new("damping", "Damping", 0.42),
                    BlockParameter::new("size", "Size", 0.80),
                    BlockParameter::new("mix", "Mix", 0.32),
                    BlockParameter::new("on-off", "On/Off", 1.0),
                ]),
            ),
        ],
    )
}

// ═══════════════════════════════════════════════════════════════════
//  Module presets (one per virtual module grouping)
// ═══════════════════════════════════════════════════════════════════

pub fn module_presets() -> Vec<ModulePreset> {
    vec![
        jm_pedals(),
        jm_pre_fx(),
        jm_amp_module(),
        jm_cab_module(),
        jm_eq_module(),
        jm_post_fx(),
    ]
}

fn jm_pedals() -> ModulePreset {
    ModulePreset::new(
        seed_id("jm-pedals"),
        "JM Pedals",
        ModuleType::PreFx,
        ModuleSnapshot::new(
            seed_id("jm-pedals-default"),
            "Default",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "justa-boost",
                    "Justa Boost",
                    BlockType::Boost,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("jm-justa-boost")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "antelope-filter",
                    "Antelope Filter",
                    BlockType::Filter,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("jm-antelope-filter")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "halfman-od",
                    "Halfman OD",
                    BlockType::Drive,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("jm-halfman-od")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "tealbreaker",
                    "Tealbreaker",
                    BlockType::Drive,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("jm-tealbreaker")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "millipede-delay",
                    "Millipede Delay",
                    BlockType::Delay,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("jm-millipede-delay")),
                        saved_at_version: None,
                    },
                ),
            ]),
        ),
        vec![ModuleSnapshot::new(
            seed_id("jm-pedals-lead"),
            "Lead",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "justa-boost",
                    "Justa Boost",
                    BlockType::Boost,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("jm-justa-boost")),
                        snapshot_id: SnapshotId::from(seed_id("jm-justa-boost-edge")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "antelope-filter",
                    "Antelope Filter",
                    BlockType::Filter,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("jm-antelope-filter")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "halfman-od",
                    "Halfman OD",
                    BlockType::Drive,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("jm-halfman-od")),
                        snapshot_id: SnapshotId::from(seed_id("jm-halfman-od-crunch")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "tealbreaker",
                    "Tealbreaker",
                    BlockType::Drive,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("jm-tealbreaker")),
                        snapshot_id: SnapshotId::from(seed_id("jm-tealbreaker-pushed")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "millipede-delay",
                    "Millipede Delay",
                    BlockType::Delay,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("jm-millipede-delay")),
                        saved_at_version: None,
                    },
                ),
            ]),
        )],
    )
}

fn jm_pre_fx() -> ModulePreset {
    ModulePreset::new(
        seed_id("jm-pre-fx"),
        "JM Pre-FX",
        ModuleType::PreFx,
        ModuleSnapshot::new(
            seed_id("jm-pre-fx-default"),
            "Default",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "harmonic-tremolo",
                    "Harmonic Tremolo",
                    BlockType::Tremolo,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("jm-harmonic-tremolo")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "spring-reverb",
                    "Spring Reverb",
                    BlockType::Reverb,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("jm-spring-reverb")),
                        saved_at_version: None,
                    },
                ),
            ]),
        ),
        vec![],
    )
}

fn jm_amp_module() -> ModulePreset {
    ModulePreset::new(
        seed_id("jm-amp-module"),
        "JM Amp",
        ModuleType::Amp,
        ModuleSnapshot::new(
            seed_id("jm-amp-module-default"),
            "Default",
            Module::from_blocks(vec![ModuleBlock::new(
                "amp",
                "JM Amp",
                BlockType::Amp,
                ModuleBlockSource::PresetDefault {
                    preset_id: PresetId::from(seed_id("jm-amp")),
                    saved_at_version: None,
                },
            )]),
        ),
        vec![
            ModuleSnapshot::new(
                seed_id("jm-amp-module-clean"),
                "Clean",
                Module::from_blocks(vec![ModuleBlock::new(
                    "amp",
                    "JM Amp",
                    BlockType::Amp,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("jm-amp")),
                        snapshot_id: SnapshotId::from(seed_id("jm-amp-clean")),
                        saved_at_version: None,
                    },
                )]),
            ),
            ModuleSnapshot::new(
                seed_id("jm-amp-module-crunch"),
                "Crunch",
                Module::from_blocks(vec![ModuleBlock::new(
                    "amp",
                    "JM Amp",
                    BlockType::Amp,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("jm-amp")),
                        snapshot_id: SnapshotId::from(seed_id("jm-amp-crunch")),
                        saved_at_version: None,
                    },
                )]),
            ),
            ModuleSnapshot::new(
                seed_id("jm-amp-module-lead"),
                "Lead",
                Module::from_blocks(vec![ModuleBlock::new(
                    "amp",
                    "JM Amp",
                    BlockType::Amp,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("jm-amp")),
                        snapshot_id: SnapshotId::from(seed_id("jm-amp-lead")),
                        saved_at_version: None,
                    },
                )]),
            ),
        ],
    )
}

fn jm_cab_module() -> ModulePreset {
    ModulePreset::new(
        seed_id("jm-cab-module"),
        "JM Cab",
        ModuleType::Amp,
        ModuleSnapshot::new(
            seed_id("jm-cab-module-default"),
            "Default",
            Module::from_blocks(vec![ModuleBlock::new(
                "cab",
                "JM Cabinet",
                BlockType::Cabinet,
                ModuleBlockSource::PresetDefault {
                    preset_id: PresetId::from(seed_id("jm-cab")),
                    saved_at_version: None,
                },
            )]),
        ),
        vec![
            ModuleSnapshot::new(
                seed_id("jm-cab-module-close"),
                "Close Mic",
                Module::from_blocks(vec![ModuleBlock::new(
                    "cab",
                    "JM Cabinet",
                    BlockType::Cabinet,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("jm-cab")),
                        snapshot_id: SnapshotId::from(seed_id("jm-cab-close")),
                        saved_at_version: None,
                    },
                )]),
            ),
            ModuleSnapshot::new(
                seed_id("jm-cab-module-room"),
                "Room",
                Module::from_blocks(vec![ModuleBlock::new(
                    "cab",
                    "JM Cabinet",
                    BlockType::Cabinet,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("jm-cab")),
                        snapshot_id: SnapshotId::from(seed_id("jm-cab-room")),
                        saved_at_version: None,
                    },
                )]),
            ),
        ],
    )
}

fn jm_eq_module() -> ModulePreset {
    ModulePreset::new(
        seed_id("jm-eq-module"),
        "JM EQ",
        ModuleType::Eq,
        ModuleSnapshot::new(
            seed_id("jm-eq-module-default"),
            "Default",
            Module::from_blocks(vec![ModuleBlock::new(
                "eq",
                "JM EQ",
                BlockType::Eq,
                ModuleBlockSource::PresetDefault {
                    preset_id: PresetId::from(seed_id("jm-eq")),
                    saved_at_version: None,
                },
            )]),
        ),
        vec![],
    )
}

fn jm_post_fx() -> ModulePreset {
    ModulePreset::new(
        seed_id("jm-post-fx"),
        "JM Post-FX",
        ModuleType::Time,
        ModuleSnapshot::new(
            seed_id("jm-post-fx-default"),
            "Default",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "dream-delay",
                    "Dream Delay",
                    BlockType::Delay,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("jm-dream-delay")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "studio-verb",
                    "Studio Verb",
                    BlockType::Reverb,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("jm-studio-verb")),
                        saved_at_version: None,
                    },
                ),
            ]),
        ),
        vec![ModuleSnapshot::new(
            seed_id("jm-post-fx-ambient"),
            "Ambient",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "dream-delay",
                    "Dream Delay",
                    BlockType::Delay,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("jm-dream-delay")),
                        snapshot_id: SnapshotId::from(seed_id("jm-dream-delay-ambient")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "studio-verb",
                    "Studio Verb",
                    BlockType::Reverb,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("jm-studio-verb")),
                        snapshot_id: SnapshotId::from(seed_id("jm-studio-verb-hall")),
                        saved_at_version: None,
                    },
                ),
            ]),
        )],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_preset_count() {
        assert_eq!(block_presets().len(), 12);
    }

    #[test]
    fn module_preset_count() {
        assert_eq!(module_presets().len(), 6);
    }

    #[test]
    fn all_block_presets_have_snapshots() {
        for p in block_presets() {
            assert!(
                !p.snapshots().is_empty(),
                "preset {} has no snapshots",
                p.name()
            );
        }
    }

    #[test]
    fn pedals_module_has_5_blocks() {
        let pedals = jm_pedals();
        let default = pedals.default_snapshot();
        assert_eq!(default.module().chain().nodes().len(), 5);
    }

    #[test]
    fn amp_module_has_variant_snapshots() {
        let amp = jm_amp_module();
        // default + clean + crunch + lead = 4
        assert_eq!(amp.snapshots().len(), 4);
    }
}
