//! Seed data — runtime-loaded library data only.
//!
//! All block presets are sourced from signal-library/ at runtime.
//! No presets are hardcoded in this crate.

pub mod catalog_import;

use signal_proto::{
    engine::Engine, layer::Layer, profile::Profile, rig::Rig, setlist::Setlist, song::Song,
    ModulePreset, Preset,
};

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

/// All block presets — loaded from signal-library at runtime.
pub fn default_block_collections() -> Vec<Preset> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let library_path = std::path::PathBuf::from(home).join("Music/FastTrackStudio/Library");
    let mut out = Vec::new();
    out.extend(catalog_import::catalog_block_collections(&library_path));
    out.extend(catalog_import::rfxchain_block_collections(&library_path));
    out
}

pub fn default_module_collections() -> Vec<ModulePreset> {
    vec![]
}
pub fn default_seed_layers() -> Vec<Layer> {
    vec![]
}
pub fn default_seed_engines() -> Vec<Engine> {
    vec![]
}
pub fn default_seed_rigs() -> Vec<Rig> {
    vec![]
}
pub fn default_seed_profiles() -> Vec<Profile> {
    vec![]
}
pub fn default_seed_songs() -> Vec<Song> {
    vec![]
}
pub fn default_seed_setlists() -> Vec<Setlist> {
    vec![]
}

/// Runtime seed bundle — only block collections from signal-library.
pub fn runtime_seed_bundle() -> SeedBundle {
    SeedBundle {
        block_collections: default_block_collections(),
        module_collections: vec![],
        layers: vec![],
        engines: vec![],
        rigs: vec![],
        profiles: vec![],
        songs: vec![],
        setlists: vec![],
    }
}
