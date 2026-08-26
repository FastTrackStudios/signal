//! Seed data — runtime-loaded library data only.
//!
//! All block presets are sourced from signal-library/ at runtime.
//! No presets are hardcoded in this crate.
//!
//! Two sources are scanned:
//! 1. `<fts_home>/Library/` — existing catalog.json + rfxchain presets
//! 2. `<fts_home>/Reaper/FXChains/FTS-Signal/` — REAPER-native
//!    directory of `.RfxChain` files with optional `.signal.styx` sidecars

pub mod catalog_import;
pub mod fxchains_scan;

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

/// All block presets — loaded from signal-library and FXChains at runtime.
pub fn default_block_collections() -> Vec<Preset> {
    let library_path = utils::paths::library_dir();
    let mut out = Vec::new();
    // Legacy: catalog.json + rfxchain presets from Library/
    out.extend(catalog_import::catalog_block_collections(&library_path));
    out.extend(catalog_import::rfxchain_block_collections(&library_path));
    // New: REAPER-native FXChains directory
    let fxchains_root = fxchains_scan::fxchains_root();
    out.extend(fxchains_scan::scan_blocks(&fxchains_root));
    out
}

/// Module presets — loaded from FXChains/FTS-Signal/02-Modules/ at runtime.
pub fn default_module_collections() -> Vec<ModulePreset> {
    let fxchains_root = fxchains_scan::fxchains_root();
    fxchains_scan::scan_modules(&fxchains_root)
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

/// Runtime seed bundle — block collections from signal-library + FXChains,
/// module collections from FXChains.
pub fn runtime_seed_bundle() -> SeedBundle {
    SeedBundle {
        block_collections: default_block_collections(),
        module_collections: default_module_collections(),
        layers: vec![],
        engines: vec![],
        rigs: vec![],
        profiles: vec![],
        songs: vec![],
        setlists: vec![],
    }
}
