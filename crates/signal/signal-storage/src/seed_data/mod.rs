//! Seed data — default block and module collections for development/demo.
//!
//! Each sub-module exports a `Vec<Preset>` or `Vec<ModulePreset>` of factory
//! presets with realistic parameter values modeled after real gear.

pub mod amp;
pub mod archetype_jm;
pub mod boost;
pub mod catalog_import;
pub mod chorus;
pub mod compressor;
pub mod de_esser;
pub mod delay;
pub mod doubler;
pub mod drive;
pub mod engine;
pub mod eq;
pub mod filter;
pub mod flanger;
pub mod gate;
pub mod layer;
pub mod limiter;
pub mod module;
pub mod phaser;
pub mod pitch;
pub mod profile;
pub mod reverb;
pub mod rig;
pub mod rotary;
pub mod saturator;
pub mod setlist;
pub mod song;
pub mod tremolo;
pub mod tuner;
pub mod vibrato;
pub mod volume;
pub mod wah;

use signal_proto::{
    engine::Engine, layer::Layer, profile::Profile, rig::Rig, seed_id, setlist::Setlist,
    song::Song, Block, BlockParameter, BlockType, Module, ModuleBlock, ModuleBlockSource,
    ModulePreset, ModulePresetId, ModuleSnapshot, ModuleSnapshotId, ModuleType, Preset, PresetId,
    SignalChain, SignalNode, Snapshot, SnapshotId,
};
use std::collections::HashMap;

const PHANTOM_KEYS_BLOCK_PRESET: &str = "__phantom__keys-megarig-space-verb";
const PHANTOM_KEYS_BLOCK_SNAPSHOT_DEFAULT: &str = "__phantom__keys-megarig-space-verb-default";
const PHANTOM_KEYS_BLOCK_SNAPSHOT_WIDE: &str = "__phantom__keys-megarig-space-verb-wide";
const PHANTOM_KEYS_MODULE_PRESET: &str = "__phantom__keys-megarig-time";
const PHANTOM_KEYS_MODULE_SNAPSHOT_DEFAULT: &str = "__phantom__keys-megarig-time-default";
const PHANTOM_KEYS_LAYER_SNAPSHOT: &str = "__phantom__keys-megarig-keys-layer-space";

#[derive(Debug, Clone)]
pub struct SeedBundle {
    pub block_collections: Vec<Preset>,
    pub module_collections: Vec<ModulePreset>,
    pub layers: Vec<Layer>,
    pub engines: Vec<Engine>,
    pub rigs: Vec<Rig>,
    pub profiles: Vec<Profile>,
    pub songs: Vec<Song>,
    pub setlists: Vec<Setlist>,
}

/// All default block collections (presets) across every block type.
pub fn default_block_collections() -> Vec<Preset> {
    let mut out = Vec::new();
    out.extend(amp::presets());
    out.extend(drive::presets());
    out.extend(compressor::presets());
    out.extend(delay::presets());
    out.extend(reverb::presets());
    out.extend(limiter::presets());
    out.extend(de_esser::presets());
    out.extend(eq::presets());
    out.extend(gate::presets());
    out.extend(filter::presets());
    out.extend(chorus::presets());
    out.extend(boost::presets());
    out.extend(volume::presets());
    out.extend(wah::presets());
    out.extend(pitch::presets());
    out.extend(doubler::presets());
    out.extend(flanger::presets());
    out.extend(phaser::presets());
    out.extend(tremolo::presets());
    out.extend(vibrato::presets());
    out.extend(rotary::presets());
    out.extend(saturator::presets());
    out.extend(tuner::presets());
    out.extend(archetype_jm::block_presets());

    // Neural DSP catalog from disk (graceful skip if missing)
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let library_path = std::path::PathBuf::from(home).join("Music/FastTrackStudio/Library");
    out.extend(catalog_import::catalog_block_collections(&library_path));

    // RfxChain files from blocks/ and profiles/ directories
    out.extend(catalog_import::rfxchain_block_collections(&library_path));

    out
}

/// All default module collections.
pub fn default_module_collections() -> Vec<ModulePreset> {
    let mut out = module::presets();
    out.extend(archetype_jm::module_presets());
    out
}

/// All default layer collections.
pub fn default_seed_layers() -> Vec<Layer> {
    layer::layers()
}

/// All default engine collections.
pub fn default_seed_engines() -> Vec<Engine> {
    engine::engines()
}

/// All default rig collections.
pub fn default_seed_rigs() -> Vec<Rig> {
    rig::rigs()
}

/// All default profile collections.
pub fn default_seed_profiles() -> Vec<Profile> {
    profile::profiles()
}

/// All default song collections.
pub fn default_seed_songs() -> Vec<Song> {
    song::songs()
}

/// All default setlist collections.
pub fn default_seed_setlists() -> Vec<Setlist> {
    setlist::setlists()
}

/// Runtime seed bundle with globally-unique IDs for private ("phantom") entries.
///
/// Built-in library seeds remain deterministic. Private entries are generated
/// with fresh UUID IDs so they are safe to share/export without name collisions.
pub fn runtime_seed_bundle() -> SeedBundle {
    let mut block_collections = default_block_collections();
    let mut module_collections = default_module_collections();
    let mut layers = default_seed_layers();
    let engines = default_seed_engines();
    let mut rigs = default_seed_rigs();
    let profiles = default_seed_profiles();
    let songs = default_seed_songs();
    let setlists = default_seed_setlists();

    let private_block_preset_id = PresetId::new();
    let private_block_default_snapshot_id = SnapshotId::new();
    let private_block_wide_snapshot_id = SnapshotId::new();

    let private_block_default = Block::from_parameters(vec![
        BlockParameter::new("size", "Size", 0.92),
        BlockParameter::new("decay", "Decay", 0.88),
        BlockParameter::new("mix", "Mix", 0.68),
        BlockParameter::new("pre_delay", "Pre-Delay", 0.40),
        BlockParameter::new("mod", "Mod", 0.56),
    ]);
    let private_block_wide = Block::from_parameters(vec![
        BlockParameter::new("size", "Size", 0.95),
        BlockParameter::new("decay", "Decay", 0.92),
        BlockParameter::new("mix", "Mix", 0.72),
        BlockParameter::new("pre_delay", "Pre-Delay", 0.45),
        BlockParameter::new("mod", "Mod", 0.62),
    ]);
    block_collections.push(Preset::new(
        private_block_preset_id.clone(),
        "__phantom__ Keys Space Verb",
        BlockType::Reverb,
        Snapshot::new(
            private_block_default_snapshot_id.clone(),
            "Default",
            private_block_default,
        ),
        vec![Snapshot::new(
            private_block_wide_snapshot_id.clone(),
            "Wide",
            private_block_wide,
        )],
    ));

    let private_module_preset_id = ModulePresetId::new();
    let private_module_default_snapshot_id = ModuleSnapshotId::new();
    module_collections.push(ModulePreset::new(
        private_module_preset_id.clone(),
        "__phantom__ Keys Space Time",
        ModuleType::Time,
        ModuleSnapshot::new(
            private_module_default_snapshot_id.clone(),
            "Default",
            Module::from_chain(SignalChain::new(vec![
                SignalNode::Split {
                    lanes: vec![
                        SignalChain::serial(vec![ModuleBlock::new(
                            "dly-1",
                            "Timeline",
                            BlockType::Delay,
                            ModuleBlockSource::PresetSnapshot {
                                preset_id: PresetId::from(seed_id("delay-timeline")),
                                snapshot_id: SnapshotId::from(seed_id("delay-timeline-ambient")),
                                saved_at_version: None,
                            },
                        )]),
                        SignalChain::serial(vec![ModuleBlock::new(
                            "dly-2",
                            "DD-8",
                            BlockType::Delay,
                            ModuleBlockSource::PresetSnapshot {
                                preset_id: PresetId::from(seed_id("delay-dd8")),
                                snapshot_id: SnapshotId::from(seed_id("delay-dd8-shimmer")),
                                saved_at_version: None,
                            },
                        )]),
                        SignalChain::new(vec![]),
                    ],
                },
                SignalNode::Split {
                    lanes: vec![
                        SignalChain::serial(vec![ModuleBlock::new(
                            "verb-1",
                            "Space",
                            BlockType::Reverb,
                            ModuleBlockSource::PresetDefault {
                                preset_id: private_block_preset_id.clone(),
                                saved_at_version: None,
                            },
                        )]),
                        SignalChain::serial(vec![ModuleBlock::new(
                            "verb-2",
                            "RV-6",
                            BlockType::Reverb,
                            ModuleBlockSource::PresetSnapshot {
                                preset_id: PresetId::from(seed_id("reverb-rv6")),
                                snapshot_id: SnapshotId::from(seed_id("reverb-rv6-modulate")),
                                saved_at_version: None,
                            },
                        )]),
                        SignalChain::new(vec![]),
                    ],
                },
            ])),
        ),
        vec![],
    ));

    let private_layer_snapshot_id = signal_proto::layer::LayerSnapshotId::new();

    let mut id_map = HashMap::new();
    id_map.insert(
        seed_id(PHANTOM_KEYS_BLOCK_PRESET).to_string(),
        private_block_preset_id.to_string(),
    );
    id_map.insert(
        seed_id(PHANTOM_KEYS_BLOCK_SNAPSHOT_DEFAULT).to_string(),
        private_block_default_snapshot_id.to_string(),
    );
    id_map.insert(
        seed_id(PHANTOM_KEYS_BLOCK_SNAPSHOT_WIDE).to_string(),
        private_block_wide_snapshot_id.to_string(),
    );
    id_map.insert(
        seed_id(PHANTOM_KEYS_MODULE_PRESET).to_string(),
        private_module_preset_id.to_string(),
    );
    id_map.insert(
        seed_id(PHANTOM_KEYS_MODULE_SNAPSHOT_DEFAULT).to_string(),
        private_module_default_snapshot_id.to_string(),
    );
    id_map.insert(
        seed_id(PHANTOM_KEYS_LAYER_SNAPSHOT).to_string(),
        private_layer_snapshot_id.to_string(),
    );

    remap_uuids(&mut layers, &id_map);
    remap_uuids(&mut rigs, &id_map);

    SeedBundle {
        block_collections,
        module_collections,
        layers,
        engines,
        rigs,
        profiles,
        songs,
        setlists,
    }
}

fn remap_uuids<T>(value: &mut T, id_map: &HashMap<String, String>)
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let mut json = serde_json::to_value(&*value).expect("seed serialization failed");
    remap_value(&mut json, id_map);
    *value = serde_json::from_value(json).expect("seed deserialization failed");
}

fn remap_value(value: &mut serde_json::Value, id_map: &HashMap<String, String>) {
    match value {
        serde_json::Value::String(s) => {
            if let Some(next) = id_map.get(s) {
                *s = next.clone();
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                remap_value(item, id_map);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                remap_value(v, id_map);
            }
        }
        _ => {}
    }
}
