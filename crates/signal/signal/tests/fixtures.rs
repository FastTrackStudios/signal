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
    let _ = signal.save_built_rig(built).await;
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
use signal::metadata::Metadata;
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

/// Seed the full guitar library the higher-level tests expect: the JM megarig
/// plus a "Worship" profile (Clean/Lead patches targeting the megarig), a song
/// whose sections resolve, and a setlist referencing that song.
///
/// Everything targets the JM megarig (which fully resolves), so `resolve_target`
/// succeeds for every patch/section. The old seed put the worship profile on a
/// separate worship rig; here it targets the megarig so we only need one
/// resolvable rig — the tests only assert the profile name, patch names/targets,
/// and that the Clean patch drives amp gain low.
pub async fn seed_guitar_library(signal: &Signal) {
    use signal::profile::{Patch, Profile};
    use signal::setlist::{Setlist, SetlistEntry};
    use signal::song::{Section, Song};

    seed_jm_megarig(signal).await;

    let amp_gain = || {
        NodePath::engine("guitar-engine")
            .with_layer("guitar-layer-archetype-jm")
            .with_block("amp")
            .with_parameter("gain")
    };

    // ── "Worship" profile — 8 patches on the megarig ──
    let clean = Patch::from_rig_scene(
        seed_id("guitar-worship-clean"),
        "Clean",
        guitar_megarig_id(),
        guitar_megarig_default_scene(),
    )
    .with_override(Override::set(amp_gain(), 0.18));
    let mut worship = Profile::new(seed_id("guitar-worship-profile"), "Worship", clean);
    worship.add_patch(
        Patch::from_rig_scene(
            seed_id("guitar-worship-crunch"),
            "Crunch",
            guitar_megarig_id(),
            guitar_megarig_default_scene(),
        )
        .with_override(Override::set(amp_gain(), 0.48)),
    );
    worship.add_patch(
        Patch::from_rig_scene(
            seed_id("guitar-worship-drive"),
            "Drive",
            guitar_megarig_id(),
            guitar_megarig_default_scene(),
        )
        .with_override(Override::set(amp_gain(), 0.60)),
    );
    worship.add_patch(
        Patch::from_rig_scene(
            seed_id("guitar-worship-lead"),
            "Lead",
            guitar_megarig_id(),
            guitar_megarig_lead_scene(),
        )
        .with_override(Override::set(amp_gain(), 0.75)),
    );
    worship.add_patch(Patch::from_rig_scene(
        seed_id("guitar-worship-ambient"),
        "Ambient",
        guitar_megarig_id(),
        guitar_megarig_lead_scene(),
    ));
    worship.add_patch(Patch::from_rig_scene(
        seed_id("guitar-worship-tremolo"),
        "Tremolo",
        guitar_megarig_id(),
        guitar_megarig_default_scene(),
    ));
    worship.add_patch(Patch::from_rig_scene(
        seed_id("guitar-worship-delay"),
        "Delay",
        guitar_megarig_id(),
        guitar_megarig_default_scene(),
    ));
    worship.add_patch(
        Patch::from_rig_scene(
            seed_id("guitar-worship-solo"),
            "Solo",
            guitar_megarig_id(),
            guitar_megarig_lead_scene(),
        )
        .with_override(Override::set(amp_gain(), 0.72))
        // A second override on a *different* path (delay mix) so the Solo patch
        // has strictly more effective overrides than the bare lead rig scene.
        .with_override(Override::set(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-archetype-jm")
                .with_block("dream-delay")
                .with_parameter("mix"),
            0.30,
        )),
    );
    let worship = worship.with_metadata(
        Metadata::new()
            .with_tag("guitar")
            .with_tag("worship")
            .with_tag("setlist"),
    );
    signal.profiles().save(worship).await.unwrap();

    // ── A song with resolvable sections (rig-scene + patch sourced) ──
    let mut song = Song::new(
        seed_id("guitar-worship-song"),
        "Worship Set Opener",
        Section::from_rig_scene(
            seed_id("gws-intro"),
            "Intro",
            guitar_megarig_id(),
            guitar_megarig_default_scene(),
        ),
    );
    song.add_section(Section::from_patch(
        seed_id("gws-verse"),
        "Verse",
        seed_id("guitar-worship-clean"),
    ));
    song.add_section(Section::from_rig_scene(
        seed_id("gws-chorus"),
        "Chorus",
        guitar_megarig_id(),
        guitar_megarig_lead_scene(),
    ));
    signal.songs().save(song).await.unwrap();

    // ── A setlist referencing that song ──
    //
    // Named "Worship Set" with 2 entries; the first entry's id is
    // `worship-set-worship-song` and its name matches the setlist name — the
    // shape the setlists-browser tests assert on.
    let mut setlist = Setlist::new(
        seed_id("worship-set"),
        "Worship Set",
        SetlistEntry::new(
            seed_id("worship-set-worship-song"),
            "Worship Set",
            seed_id("guitar-worship-song"),
        ),
    );
    setlist.add_entry(SetlistEntry::new(
        seed_id("worship-set-encore"),
        "Encore",
        seed_id("guitar-worship-song"),
    ));
    signal.setlists().save(setlist).await.unwrap();
}

/// Build one guitar patch targeting a megarig scene with an amp-gain override.
fn megarig_patch(id: &str, name: &str, lead: bool, gain: f32) -> signal::profile::Patch {
    use signal::profile::Patch;
    let scene = if lead {
        guitar_megarig_lead_scene()
    } else {
        guitar_megarig_default_scene()
    };
    Patch::from_rig_scene(seed_id(id), name, guitar_megarig_id(), scene).with_override(
        Override::set(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-archetype-jm")
                .with_block("amp")
                .with_parameter("gain"),
            gain,
        ),
    )
}

/// Seed the full set of guitar profiles the profile/song tests expect:
/// Worship (default Clean), Blues (default Crunch), Rock (default Drive), and
/// All-Around (default Clean, 8 named patches). Also seeds the megarig + song +
/// setlist via [`seed_guitar_library`].
///
/// The old seed drew these from separate rigs / RfxChain block snapshots; here
/// every patch targets the JM megarig so they all resolve, and the tests'
/// assertions (patch counts, default-patch names, per-patch gain overrides,
/// activate/resolve success) hold.
pub async fn seed_guitar_profiles(signal: &Signal) {
    use signal::profile::Profile;

    seed_guitar_library(signal).await;

    // ── Blues — default Crunch ──
    let mut blues = Profile::new(
        seed_id("guitar-blues-profile"),
        "Blues",
        megarig_patch("guitar-blues-clean", "Clean", false, 0.15),
    );
    blues.add_patch(megarig_patch("guitar-blues-crunch", "Crunch", false, 0.48));
    blues.add_patch(megarig_patch("guitar-blues-drive", "Drive", false, 0.60));
    blues.add_patch(megarig_patch("guitar-blues-lead", "Lead", true, 0.65));
    blues.default_patch_id = seed_id("guitar-blues-crunch").into();
    signal.profiles().save(blues).await.unwrap();

    // ── Rock — default Drive, 8 patches for slot-remap coverage ──
    let mut rock = Profile::new(
        seed_id("guitar-rock-profile"),
        "Rock",
        megarig_patch("guitar-rock-clean", "Clean", false, 0.20),
    );
    rock.add_patch(megarig_patch("guitar-rock-crunch", "Crunch", false, 0.52));
    rock.add_patch(megarig_patch("guitar-rock-drive", "Drive", false, 0.68));
    rock.add_patch(megarig_patch("guitar-rock-lead", "Lead", true, 0.72));
    rock.add_patch(megarig_patch("guitar-rock-ambient", "Ambient", false, 0.40));
    rock.add_patch(megarig_patch("guitar-rock-phaser", "Phaser", false, 0.45));
    rock.add_patch(megarig_patch("guitar-rock-dly-lead", "DLY Lead", true, 0.70));
    rock.add_patch(megarig_patch("guitar-rock-solo", "Solo", true, 0.78));
    rock.default_patch_id = seed_id("guitar-rock-drive").into();
    signal.profiles().save(rock).await.unwrap();

    // ── All-Around — default Clean, 8 named patches ──
    let mut all_around = Profile::new(
        seed_id("guitar-allaround-profile"),
        "All-Around",
        megarig_patch("guitar-allaround-clean", "Clean", false, 0.22),
    );
    all_around.add_patch(megarig_patch(
        "guitar-allaround-crunch",
        "Crunch",
        false,
        0.50,
    ));
    all_around.add_patch(megarig_patch("guitar-allaround-drive", "Drive", false, 0.62));
    all_around.add_patch(megarig_patch("guitar-allaround-lead", "Lead", true, 0.75));
    all_around.add_patch(megarig_patch("guitar-allaround-funk", "Funk", false, 0.30));
    all_around.add_patch(megarig_patch(
        "guitar-allaround-ambient",
        "Ambient",
        false,
        0.35,
    ));
    all_around.add_patch(megarig_patch(
        "guitar-allaround-qtron",
        "Q-Tron",
        false,
        0.34,
    ));
    all_around.add_patch(megarig_patch("guitar-allaround-solo", "Solo", true, 0.72));
    signal.profiles().save(all_around).await.unwrap();
}

// ─── Keys megarig fixture ───────────────────────────────────────
//
// A 4-engine Keys megarig (Keys / Synth / Organ / Pad) with the layer/scene
// counts the keys tests assert, built so every scene fully resolves. Each layer
// references a single shared module preset (`keys-mod` → block preset
// `keys-tone`), which is enough for resolution to produce engines + layers.

fn keys_block_preset() -> Preset {
    Preset::new(
        seed_id("keys-tone"),
        "Keys Tone",
        BlockType::Amp,
        Snapshot::new(
            seed_id("keys-tone-default"),
            "Default",
            Block::from_parameters(vec![
                BlockParameter::new("brightness", "Brightness", 0.5),
                BlockParameter::new("warmth", "Warmth", 0.6),
            ]),
        ),
        vec![],
    )
}

fn keys_module_preset() -> ModulePreset {
    ModulePreset::new(
        seed_id("keys-mod"),
        "Keys Module",
        ModuleType::Custom,
        ModuleSnapshot::new(
            seed_id("keys-mod-default"),
            "Default",
            Module::from_blocks(vec![ModuleBlock::new(
                "tone",
                "Keys Tone",
                BlockType::Amp,
                ModuleBlockSource::PresetDefault {
                    preset_id: PresetId::from(seed_id("keys-tone")),
                    saved_at_version: None,
                },
            )]),
        ),
        vec![],
    )
}

fn keys_layer(layer_seed: &str, name: &str, etype: EngineType) -> Layer {
    let snap = LayerSnapshot::new(seed_id(&format!("{layer_seed}-default")), "Default")
        .with_module(ModuleRef::new(seed_id("keys-mod")));
    Layer::new(seed_id(layer_seed), name, etype, snap)
}

fn keys_make_engine(
    engine_seed: &str,
    name: &str,
    etype: EngineType,
    layers: &[&str],
    scenes: &[(&str, &str)],
) -> Engine {
    let layer_ids: Vec<LayerId> = layers.iter().map(|l| LayerId::from(seed_id(l))).collect();
    let mk_scene = |scene_seed: &str, scene_name: &str| {
        let mut s = EngineScene::new(seed_id(scene_seed), scene_name);
        for l in layers {
            s = s.with_layer(LayerSelection::new(
                seed_id(l),
                seed_id(&format!("{l}-default")),
            ));
        }
        s
    };
    let (fs_seed, fs_name) = scenes[0];
    let mut engine = Engine::new(
        seed_id(engine_seed),
        name,
        etype,
        layer_ids,
        mk_scene(fs_seed, fs_name),
    );
    for (ss, sn) in &scenes[1..] {
        engine.add_variant(mk_scene(ss, sn));
    }
    engine
}

fn keys_rig_scene(scene_seed: &str, scene_name: &str, keys_engine_scene: &str) -> RigScene {
    RigScene::new(seed_id(scene_seed), scene_name)
        .with_engine(EngineSelection::new(
            seed_id("keys-engine"),
            seed_id(keys_engine_scene),
        ))
        .with_engine(EngineSelection::new(
            seed_id("synth-engine"),
            seed_id("synth-engine-default"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("organ-engine"),
            seed_id("organ-engine-default"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("pad-engine"),
            seed_id("pad-engine-default"),
        ))
        .with_metadata(Metadata::new().with_tag("megarig").with_tag("keys"))
}

/// Build + persist the Keys megarig hierarchy (block/module preset → 9 layers →
/// 4 engines → rig) plus the "Keys Feature" profile and "Feature-Demo Song".
pub async fn seed_keys_megarig(signal: &Signal) {
    use signal::profile::{Patch, Profile};
    use signal::song::{Section, Song};

    signal
        .block_presets()
        .save(keys_block_preset())
        .await
        .unwrap();
    signal
        .module_presets()
        .save(keys_module_preset())
        .await
        .unwrap();

    let layers = [
        ("keys-layer-core", "Keys Core", EngineType::Keys),
        ("keys-layer-space", "Keys Space", EngineType::Keys),
        ("synth-layer-osc", "Synth Osc", EngineType::Synth),
        ("synth-layer-motion", "Synth Motion", EngineType::Synth),
        ("synth-layer-texture", "Synth Texture", EngineType::Synth),
        ("organ-layer-body", "Organ Body", EngineType::Organ),
        ("organ-layer-air", "Organ Air", EngineType::Organ),
        ("pad-layer-foundation", "Pad Foundation", EngineType::Pad),
        ("pad-layer-shimmer", "Pad Shimmer", EngineType::Pad),
    ];
    for (seed, name, etype) in layers {
        signal
            .layers()
            .save(keys_layer(seed, name, etype))
            .await
            .unwrap();
    }

    // Engines: layer counts (2/3/2/2) and scene counts (2/2/1/1) are asserted.
    signal
        .engines()
        .save(keys_make_engine(
            "keys-engine",
            "Keys Engine",
            EngineType::Keys,
            &["keys-layer-core", "keys-layer-space"],
            &[
                ("keys-engine-default", "Default"),
                ("keys-engine-bright", "Bright"),
            ],
        ))
        .await
        .unwrap();
    signal
        .engines()
        .save(keys_make_engine(
            "synth-engine",
            "Synth Engine",
            EngineType::Synth,
            &["synth-layer-osc", "synth-layer-motion", "synth-layer-texture"],
            &[
                ("synth-engine-default", "Default"),
                ("synth-engine-scene-b", "Scene B"),
            ],
        ))
        .await
        .unwrap();
    signal
        .engines()
        .save(keys_make_engine(
            "organ-engine",
            "Organ Engine",
            EngineType::Organ,
            &["organ-layer-body", "organ-layer-air"],
            &[("organ-engine-default", "Default")],
        ))
        .await
        .unwrap();
    signal
        .engines()
        .save(keys_make_engine(
            "pad-engine",
            "Pad Engine",
            EngineType::Pad,
            &["pad-layer-foundation", "pad-layer-shimmer"],
            &[("pad-engine-default", "Default")],
        ))
        .await
        .unwrap();

    // Rig: 4 engines, 4 scenes (Wide swaps the keys engine to its Bright scene).
    let mut rig = Rig::new(
        seed_id("keys-megarig"),
        "MegaRig",
        vec![
            EngineId::from(seed_id("keys-engine")),
            EngineId::from(seed_id("synth-engine")),
            EngineId::from(seed_id("organ-engine")),
            EngineId::from(seed_id("pad-engine")),
        ],
        keys_rig_scene("keys-megarig-default", "Default", "keys-engine-default"),
    )
    .with_rig_type(RigType::Keys)
    .with_metadata(Metadata::new().with_tag("megarig").with_tag("keys"));
    rig.add_variant(keys_rig_scene(
        "keys-megarig-wide",
        "Wide",
        "keys-engine-bright",
    ));
    rig.add_variant(keys_rig_scene(
        "keys-megarig-focus",
        "Focus",
        "keys-engine-default",
    ));
    rig.add_variant(keys_rig_scene(
        "keys-megarig-air",
        "Air",
        "keys-engine-default",
    ));
    signal.rigs().save(rig).await.unwrap();

    // "Keys Feature" profile — 4 patches, one per scene.
    let keys_patch = |id: &str, name: &str, scene: &str| {
        Patch::from_rig_scene(seed_id(id), name, seed_id("keys-megarig"), seed_id(scene))
    };
    let mut profile = Profile::new(
        seed_id("keys-feature-profile"),
        "Keys Feature",
        keys_patch(
            "keys-feature-foundation",
            "Foundation",
            "keys-megarig-default",
        ),
    );
    profile.add_patch(keys_patch("keys-feature-wide", "Wide", "keys-megarig-wide"));
    profile.add_patch(keys_patch(
        "keys-feature-focus",
        "Focus",
        "keys-megarig-focus",
    ));
    profile.add_patch(keys_patch("keys-feature-air", "Air", "keys-megarig-air"));
    signal.profiles().save(profile).await.unwrap();

    // "Feature-Demo Song" — 4 sections, one per scene.
    let mut song = Song::new(
        seed_id("keys-feature-song"),
        "Feature-Demo Song",
        Section::from_rig_scene(
            seed_id("kfs-intro"),
            "Intro",
            seed_id("keys-megarig"),
            seed_id("keys-megarig-default"),
        ),
    );
    song.add_section(Section::from_rig_scene(
        seed_id("kfs-wide"),
        "Wide",
        seed_id("keys-megarig"),
        seed_id("keys-megarig-wide"),
    ));
    song.add_section(Section::from_rig_scene(
        seed_id("kfs-focus"),
        "Focus",
        seed_id("keys-megarig"),
        seed_id("keys-megarig-focus"),
    ));
    song.add_section(Section::from_rig_scene(
        seed_id("kfs-air"),
        "Air",
        seed_id("keys-megarig"),
        seed_id("keys-megarig-air"),
    ));
    signal.songs().save(song).await.unwrap();

    // A setlist containing the keys song, for the full setlist sweep.
    use signal::setlist::{Setlist, SetlistEntry};
    let setlist = Setlist::new(
        seed_id("keys-feature-setlist"),
        "Feature Set",
        SetlistEntry::new(
            seed_id("kfsl-1"),
            "Feature-Demo Song",
            seed_id("keys-feature-song"),
        ),
    );
    signal.setlists().save(setlist).await.unwrap();
}

/// Seed both the guitar library (profiles/song/setlist) and the keys megarig —
/// the union the cross-rig runtime tests need.
pub async fn seed_everything(signal: &Signal) {
    seed_guitar_profiles(signal).await;
    seed_keys_megarig(signal).await;
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

    // No engine-scene override here: engine-scope overrides merge *after* patch
    // overrides in resolution, so an amp-gain override at this level would clobber
    // a patch's own amp-gain (e.g. the worship Solo patch's 0.72). The rig lead
    // scene carries the scene-level override instead.
    let lead_scene = EngineScene::new(seed_id("guitar-engine-lead"), "Lead").with_layer(
        LayerSelection::new(
            seed_id("guitar-layer-archetype-jm"),
            seed_id("guitar-layer-archetype-jm-lead"),
        ),
    );

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
    .with_rig_type(RigType::Guitar)
    .with_metadata(Metadata::new().with_tag("megarig").with_tag("guitar"));
    rig.add_variant(lead_scene);
    rig
}
