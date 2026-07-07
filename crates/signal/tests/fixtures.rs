//! Shared test fixtures for signal integration tests.
//!
//! Provides:
//! - [`controller()`] — bootstraps an in-memory controller
//! - Seed data lookup helpers for pre-existing megarigs
//! - [`save_built_rig()`] — persists a [`BuiltRig`] via the controller
//!
//! **Convention**: Tests should NOT use `seed_id()` directly. For new test
//! data, use [`RigBuilder`] and access IDs via `built.scene_id("Clean")` etc.
//! The seed ID helpers below are ONLY for referencing pre-existing seeded
//! data (the megarigs that ship with the app).
//!
//! Usage: `mod fixtures;` in each test file, then `use fixtures::*;`

#![allow(dead_code)]

use signal::Signal;
use signal::builder::BuiltRig;
use signal::rig::{RigId, RigSceneId};
use signal::seed_id;

// ─── Controller bootstrap ───────────────────────────────────────

pub async fn controller() -> Signal {
    signal::bootstrap_in_memory_controller_async()
        .await
        .expect("failed to bootstrap in-memory controller")
}

// ─── Guitar MegaRig seed IDs ────────────────────────────────────

pub fn guitar_megarig_id() -> RigId {
    seed_id("guitar-megarig").into()
}

pub fn guitar_megarig_default_scene() -> RigSceneId {
    seed_id("guitar-megarig-default").into()
}

pub fn guitar_megarig_lead_scene() -> RigSceneId {
    seed_id("guitar-megarig-lead").into()
}

// ─── Keys MegaRig seed IDs ─────────────────────────────────────

pub fn keys_megarig_id() -> RigId {
    seed_id("keys-megarig").into()
}

pub fn keys_megarig_default_scene() -> RigSceneId {
    seed_id("keys-megarig-default").into()
}

pub fn keys_megarig_wide_scene() -> RigSceneId {
    seed_id("keys-megarig-wide").into()
}

pub fn keys_megarig_focus_scene() -> RigSceneId {
    seed_id("keys-megarig-focus").into()
}

pub fn keys_megarig_air_scene() -> RigSceneId {
    seed_id("keys-megarig-air").into()
}

// ─── BuiltRig save helper ──────────────────────────────────────

/// Save all entities from a [`BuiltRig`] to the controller's storage.
///
/// **Prefer `signal.save_built_rig(&built)` instead** — same logic, lives on the controller.
#[deprecated(note = "use signal.save_built_rig(&built) instead")]
pub async fn save_built_rig(signal: &Signal, built: &BuiltRig) {
    signal.save_built_rig(built).await;
}

// ─── JM ("Archetype: John Mayer X") megarig fixture ─────────────
//
// The static seed dataset for rigs/profiles/layers/engines was emptied
// (`default_seed_rigs()` etc. now return `vec![]`), so the JM megarig no longer
// ships in the DB. These builders reconstruct that hierarchy under the exact
// same deterministic `seed_id`s the old seed used, and save it to the
// controller — the build-your-own equivalent of the removed seed. Tests call
// [`seed_jm_megarig`] to get the same data they used to resolve from the seed.

use signal::block::BlockType;
use signal::engine::{Engine, EngineId, EngineScene, LayerSelection};
use signal::layer::{Layer, LayerId, LayerSnapshot, ModuleRef};
use signal::module_type::ModuleType;
use signal::overrides::{NodePath, Override};
use signal::rig::{EngineSelection, Rig, RigScene, RigType};
use signal::{
    Block, BlockParameter, EngineType, Module, ModuleBlock, ModuleBlockSource, ModulePreset,
    ModuleSnapshot, Preset, PresetId, Snapshot, SnapshotId,
};

/// Build + persist the complete JM megarig hierarchy (block presets → module
/// presets → layer → engine → rig) under the canonical `seed_id`s.
///
/// Call this at the top of any test that references the JM megarig by seed id.
pub async fn seed_jm_megarig(signal: &Signal) {
    for preset in jm_block_presets() {
        signal.block_presets().save(preset).await.unwrap();
    }
    for module in jm_module_presets() {
        signal.module_presets().save(module).await.unwrap();
    }
    signal.layers().save(jm_layer()).await.unwrap();
    signal.engines().save(guitar_engine()).await.unwrap();
    signal.rigs().save(guitar_megarig()).await.unwrap();
}

// ── Block presets (one per virtual block) ──

pub fn jm_block_presets() -> Vec<Preset> {
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

fn harmonic_tremolo() -> Preset {
    Preset::new(
        seed_id("jm-harmonic-tremolo"),
        "Harmonic Tremolo",
        BlockType::Trem,
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

// ── Module presets (one per virtual module grouping) ──

pub fn jm_module_presets() -> Vec<ModulePreset> {
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
                    BlockType::Trem,
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

// ── Layer (Archetype JM), 6 module refs, default + lead variants ──

fn jm_layer() -> Layer {
    let default_variant =
        LayerSnapshot::new(seed_id("guitar-layer-archetype-jm-default"), "Default")
            .with_module(ModuleRef::new(seed_id("jm-pedals")))
            .with_module(ModuleRef::new(seed_id("jm-pre-fx")))
            .with_module(ModuleRef::new(seed_id("jm-amp-module")))
            .with_module(ModuleRef::new(seed_id("jm-cab-module")))
            .with_module(ModuleRef::new(seed_id("jm-eq-module")))
            .with_module(ModuleRef::new(seed_id("jm-post-fx")));

    let lead_variant = LayerSnapshot::new(seed_id("guitar-layer-archetype-jm-lead"), "Lead")
        .with_module(ModuleRef::new(seed_id("jm-pedals")).with_variant(seed_id("jm-pedals-lead")))
        .with_module(ModuleRef::new(seed_id("jm-pre-fx")))
        .with_module(
            ModuleRef::new(seed_id("jm-amp-module")).with_variant(seed_id("jm-amp-module-crunch")),
        )
        .with_module(ModuleRef::new(seed_id("jm-cab-module")))
        .with_module(ModuleRef::new(seed_id("jm-eq-module")))
        .with_module(
            ModuleRef::new(seed_id("jm-post-fx")).with_variant(seed_id("jm-post-fx-ambient")),
        );

    let mut layer = Layer::new(
        seed_id("guitar-layer-archetype-jm"),
        "Archetype JM",
        EngineType::Guitar,
        default_variant,
    );
    layer.add_variant(lead_variant);
    layer
}

// ── Engine (references only the JM layer) ──

fn guitar_engine() -> Engine {
    let default_scene = EngineScene::new(seed_id("guitar-engine-default"), "Default").with_layer(
        LayerSelection::new(
            seed_id("guitar-layer-archetype-jm"),
            seed_id("guitar-layer-archetype-jm-default"),
        ),
    );

    let lead_scene = EngineScene::new(seed_id("guitar-engine-lead"), "Lead")
        .with_layer(LayerSelection::new(
            seed_id("guitar-layer-archetype-jm"),
            seed_id("guitar-layer-archetype-jm-lead"),
        ))
        .with_override(Override::set(
            NodePath::layer("guitar-layer-archetype-jm")
                .with_module("jm-amp-module")
                .with_block("amp")
                .with_parameter("gain"),
            0.75,
        ));

    let mut engine = Engine::new(
        seed_id("guitar-engine"),
        "Guitar Engine",
        EngineType::Guitar,
        vec![LayerId::from(seed_id("guitar-layer-archetype-jm"))],
        default_scene,
    );
    engine.add_variant(lead_scene);
    engine
}

// ── Rig (guitar megarig, default + lead scenes) ──

fn guitar_megarig() -> Rig {
    let default_scene = RigScene::new(seed_id("guitar-megarig-default"), "Default").with_engine(
        EngineSelection::new(seed_id("guitar-engine"), seed_id("guitar-engine-default")),
    );

    let lead_scene = RigScene::new(seed_id("guitar-megarig-lead"), "Lead")
        .with_engine(EngineSelection::new(
            seed_id("guitar-engine"),
            seed_id("guitar-engine-lead"),
        ))
        .with_override(Override::set(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-archetype-jm")
                .with_module("jm-amp-module")
                .with_block("amp")
                .with_parameter("gain"),
            0.80,
        ));

    let mut rig = Rig::new(
        seed_id("guitar-megarig"),
        "MegaRig",
        vec![EngineId::from(seed_id("guitar-engine"))],
        default_scene,
    )
    .with_rig_type(RigType::Guitar);
    rig.add_variant(lead_scene);
    rig
}
