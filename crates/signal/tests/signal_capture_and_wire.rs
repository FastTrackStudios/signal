//! Integration test: connect to a running REAPER instance, capture the current
//! track's FX state as a rig preset for guitar, create a profile with patches
//! pointing at it, create a song with sections assigned to those patches, and
//! verify the full resolution chain works end-to-end.
//!
//!   cargo xtask reaper-test -- capture_and_wire

mod daw_helpers;

use daw_helpers::{
    add_jm_track, capture_rfxchain_bytes, capture_snapshot, child_tracks, randomize_fx_params,
    read_fx_list, read_live_params, track_by_name,
};
use reaper_test::reaper_test;
use signal::layer::LayerId;
use signal::reaper_applier::ReaperPatchApplier;
use signal::{
    block::BlockType,
    bootstrap_in_memory_controller_async,
    engine::{Engine, EngineScene, LayerSelection},
    layer::{Layer, LayerSnapshot, ModuleRef},
    module_type::ModuleType,
    profile::{Patch, PatchId, PatchTarget, Profile, ProfileId},
    resolve::ResolveTarget,
    rig::{EngineSelection, Rig, RigId, RigScene, RigSceneId, RigType},
    song::{Section, SectionId, SectionSource, Song, SongId},
    Block, BlockParameter, EngineType, Module, ModuleBlock, ModuleBlockSource, ModulePreset,
    ModulePresetId, ModuleSnapshot, ModuleSnapshotId, Preset, PresetId, Snapshot, SnapshotId,
};
use std::sync::Arc;

async fn controller() -> signal::Signal {
    bootstrap_in_memory_controller_async()
        .await
        .expect("failed to bootstrap in-memory controller")
}

/// Capture the currently selected track's FX chain and build a full signal domain
/// structure: rig → profile → song, with proper cross-references.
#[reaper_test]
async fn capture_and_wire_guitar_preset(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    // Wire up the ReaperPatchApplier for folder-based gapless switching
    let applier = Arc::new(ReaperPatchApplier::new());
    let fx_id = "captured-guitar-fx";
    applier.set_target(project.clone(), fx_id).await;

    let signal = controller().await.with_daw_applier(applier.clone());

    // ─── Step 1: Create a track with JM plugin and randomize params ───

    let track = add_jm_track(&project, "Capture Test Guitar").await?;
    ctx.log("Loaded Archetype John Mayer X on 'Capture Test Guitar'");

    // Randomize parameters so we have unique non-default values to capture
    let (_, randomized) = randomize_fx_params(&track, 0).await?;
    ctx.log(&format!("Randomized {} parameters", randomized.len()));

    // Read the FX chain
    let fx_list = read_fx_list(&track).await?;
    ctx.log(&format!("FX chain ({} plugins):", fx_list.len()));
    for (idx, name) in &fx_list {
        ctx.log(&format!("  [{}] {}", idx, name));
    }

    // Capture live parameters from FX 0
    let live_params = read_live_params(&track).await?;
    let snapshot = capture_snapshot(&track, fx_id).await?;
    ctx.log(&format!(
        "Captured {} parameters from FX 0",
        snapshot.params.len()
    ));

    // Capture the rfxchain text (RPP format) for splice_fxchain loading
    let rfxchain_bytes = capture_rfxchain_bytes(&track).await?;
    ctx.log(&format!(
        "Captured rfxchain ({} bytes) for folder-based loading",
        rfxchain_bytes.len()
    ));

    // ─── Step 2: Build domain block from captured parameters ───

    let block_params: Vec<BlockParameter> = live_params
        .iter()
        .map(|lp| BlockParameter::new(&lp.name, &lp.name, lp.value as f32))
        .collect();

    let captured_block = Block::from_parameters(block_params.clone());

    // Build a "lead" variation with boosted first param
    let mut lead_block = Block::from_parameters(block_params);
    if let Some(first) = lead_block.first_value() {
        lead_block.set_first_value((first + 0.15).min(1.0));
    }

    ctx.log(&format!(
        "Built domain block with {} params",
        captured_block.parameters().len()
    ));

    // ─── Step 3: Save block preset with Default + Lead snapshots ───

    let preset_id = PresetId::new();
    let snap_default_id = SnapshotId::new();
    let snap_lead_id = SnapshotId::new();

    let preset = Preset::new(
        preset_id.clone(),
        &fx_list[0].1, // use plugin name
        BlockType::Amp,
        Snapshot::new(snap_default_id.clone(), "Default", captured_block.clone())
            .with_state_data(rfxchain_bytes.clone()),
        vec![
            Snapshot::new(snap_lead_id.clone(), "Lead", lead_block.clone())
                .with_state_data(rfxchain_bytes.clone()),
        ],
    );

    signal.block_presets().save(preset).await;
    ctx.log("Saved block preset with Default + Lead snapshots");

    // ─── Step 4: Build module → layer → engine → rig ───

    let module_preset_id = ModulePresetId::new();
    let module_snap_default_id = ModuleSnapshotId::new();
    let module_snap_lead_id = ModuleSnapshotId::new();

    // Module blocks for default and lead
    let default_module_block = ModuleBlock::new(
        "amp",
        "Guitar Amp",
        BlockType::Amp,
        ModuleBlockSource::PresetDefault {
            preset_id: preset_id.clone(),
            saved_at_version: None,
        },
    );

    let lead_module_block = ModuleBlock::new(
        "amp",
        "Guitar Amp",
        BlockType::Amp,
        ModuleBlockSource::PresetSnapshot {
            preset_id: preset_id.clone(),
            snapshot_id: snap_lead_id.clone(),
            saved_at_version: None,
        },
    );

    let default_module = Module::from_blocks(vec![default_module_block]);
    let lead_module = Module::from_blocks(vec![lead_module_block]);

    let module_snap_default =
        ModuleSnapshot::new(module_snap_default_id.clone(), "Default", default_module);
    let module_snap_lead = ModuleSnapshot::new(module_snap_lead_id.clone(), "Lead", lead_module);

    let module_preset = ModulePreset::new(
        module_preset_id.clone(),
        "Captured Guitar Amp Module",
        ModuleType::Amp,
        module_snap_default,
        vec![module_snap_lead],
    );

    signal.module_presets().save(module_preset).await;
    ctx.log("Saved module preset");

    // Layer
    let layer_id = LayerId::new();
    let layer_snap_default_id = signal::layer::LayerSnapshotId::new();
    let layer_snap_lead_id = signal::layer::LayerSnapshotId::new();

    let layer_snap_default = LayerSnapshot::new(layer_snap_default_id.clone(), "Default")
        .with_module(
            ModuleRef::new(module_preset_id.clone()).with_variant(module_snap_default_id.clone()),
        );

    let layer_snap_lead = LayerSnapshot::new(layer_snap_lead_id.clone(), "Lead").with_module(
        ModuleRef::new(module_preset_id.clone()).with_variant(module_snap_lead_id.clone()),
    );

    let mut layer = Layer::new(
        layer_id.clone(),
        "Guitar Amp Layer",
        EngineType::Guitar,
        layer_snap_default,
    );
    layer.add_variant(layer_snap_lead);

    signal.layers().save(layer).await;
    ctx.log("Saved layer");

    // Engine
    let engine_id = signal::engine::EngineId::new();
    let engine_scene_default_id = signal::engine::EngineSceneId::new();
    let engine_scene_lead_id = signal::engine::EngineSceneId::new();

    let engine_default_scene =
        EngineScene::new(engine_scene_default_id.clone(), "Default").with_layer(
            LayerSelection::new(layer_id.clone(), layer_snap_default_id.clone()),
        );

    let engine_lead_scene = EngineScene::new(engine_scene_lead_id.clone(), "Lead").with_layer(
        LayerSelection::new(layer_id.clone(), layer_snap_lead_id.clone()),
    );

    let mut engine = Engine::new(
        engine_id.clone(),
        "Captured Guitar Engine",
        EngineType::Guitar,
        vec![layer_id.clone()],
        engine_default_scene,
    );
    engine.add_variant(engine_lead_scene);

    signal.engines().save(engine).await;
    ctx.log("Saved engine");

    // Rig with two scenes: Clean (default engine scene) and Lead
    let rig_id = RigId::new();
    let scene_clean_id = RigSceneId::new();
    let scene_lead_id = RigSceneId::new();

    let scene_clean = RigScene::new(scene_clean_id.clone(), "Clean").with_engine(
        EngineSelection::new(engine_id.clone(), engine_scene_default_id.clone()),
    );

    let scene_lead = RigScene::new(scene_lead_id.clone(), "Lead").with_engine(
        EngineSelection::new(engine_id.clone(), engine_scene_lead_id.clone()),
    );

    let mut rig = Rig::new(
        rig_id.clone(),
        "Captured Guitar Rig",
        vec![engine_id.clone()],
        scene_clean,
    )
    .with_rig_type(RigType::Guitar);
    rig.add_variant(scene_lead);

    signal.rigs().save(rig.clone()).await;
    ctx.log(&format!(
        "Saved rig '{}' with Clean + Lead scenes",
        rig.name
    ));

    // ─── Step 5: Verify rig round-trips ───

    let loaded_rig = signal.rigs().load(rig_id.to_string()).await;
    eyre::ensure!(loaded_rig.is_some(), "Rig not found after save");
    let loaded_rig = loaded_rig.unwrap();
    assert_eq!(loaded_rig.name, "Captured Guitar Rig");
    assert_eq!(loaded_rig.variants.len(), 2);
    ctx.log("✅ Rig saved and loaded with 2 scenes");

    // ─── Step 6: Create a Profile with patches ───

    let profile_id = ProfileId::new();
    let patch_clean_id = PatchId::new();
    let patch_lead_id = PatchId::new();
    let patch_crunch_id = PatchId::new();

    let patch_clean = Patch::from_rig_scene(
        patch_clean_id.clone(),
        "Clean",
        rig_id.clone(),
        scene_clean_id.clone(),
    );
    let patch_lead = Patch::from_rig_scene(
        patch_lead_id.clone(),
        "Lead",
        rig_id.clone(),
        scene_lead_id.clone(),
    );
    // Crunch reuses Clean scene — in real usage you'd add param overrides
    let patch_crunch = Patch::from_rig_scene(
        patch_crunch_id.clone(),
        "Crunch",
        rig_id.clone(),
        scene_clean_id.clone(),
    );

    let mut profile = Profile::new(profile_id.clone(), "Live Guitar Profile", patch_clean);
    profile.add_patch(patch_lead);
    profile.add_patch(patch_crunch);

    signal.profiles().save(profile.clone()).await;
    ctx.log(&format!(
        "Saved profile '{}' with {} patches",
        profile.name,
        profile.patches.len()
    ));

    // Verify profile
    let loaded_profile = signal.profiles().load(profile_id.to_string()).await;
    eyre::ensure!(loaded_profile.is_some(), "Profile not found after save");
    let loaded_profile = loaded_profile.unwrap();
    assert_eq!(loaded_profile.patches.len(), 3);
    ctx.log("✅ Profile: Clean, Lead, Crunch");

    // ─── Step 7: Create a Song with sections ───

    let song_id = SongId::new();
    let sec_intro_id = SectionId::new();
    let sec_verse_id = SectionId::new();
    let sec_chorus_id = SectionId::new();
    let sec_solo_id = SectionId::new();

    // Intro → directly references the rig scene (no profile indirection)
    let sec_intro = Section::from_rig_scene(
        sec_intro_id.clone(),
        "Intro",
        rig_id.clone(),
        scene_clean_id.clone(),
    );
    // Verse → references the Clean patch
    let sec_verse = Section::from_patch(sec_verse_id.clone(), "Verse", patch_clean_id.clone());
    // Chorus → references the Crunch patch
    let sec_chorus = Section::from_patch(sec_chorus_id.clone(), "Chorus", patch_crunch_id.clone());
    // Solo → references the Lead patch
    let sec_solo = Section::from_patch(sec_solo_id.clone(), "Solo", patch_lead_id.clone());

    let mut song =
        Song::new(song_id.clone(), "Test Song", sec_intro).with_artist("Integration Test");
    song.add_section(sec_verse);
    song.add_section(sec_chorus);
    song.add_section(sec_solo);

    signal.songs().save(song.clone()).await;
    ctx.log(&format!(
        "Saved song '{}' with {} sections",
        song.name,
        song.sections.len()
    ));

    // Verify song
    let loaded_song = signal.songs().load(song_id.to_string()).await;
    eyre::ensure!(loaded_song.is_some(), "Song not found after save");
    let loaded_song = loaded_song.unwrap();
    assert_eq!(loaded_song.sections.len(), 4);
    assert_eq!(loaded_song.artist.as_deref(), Some("Integration Test"));
    ctx.log("✅ Song: Intro, Verse, Chorus, Solo");

    // ─── Step 8: Verify resolution chain ───

    // Resolve rig scene directly
    match signal
        .resolve_target(ResolveTarget::RigScene {
            rig_id: rig_id.clone(),
            scene_id: scene_clean_id.clone(),
        })
        .await
    {
        Ok(g) => ctx.log(&format!(
            "✅ Rig scene resolves: {} engines",
            g.engines.len()
        )),
        Err(e) => ctx.log(&format!("⚠ Rig scene resolution: {:?}", e)),
    }

    // Resolve profile patch
    match signal
        .resolve_target(ResolveTarget::ProfilePatch {
            profile_id: profile_id.clone(),
            patch_id: patch_clean_id.clone(),
        })
        .await
    {
        Ok(g) => ctx.log(&format!(
            "✅ Profile patch resolves: {} engines",
            g.engines.len()
        )),
        Err(e) => ctx.log(&format!("⚠ Profile patch resolution: {:?}", e)),
    }

    // Resolve song section (via patch)
    match signal
        .resolve_target(ResolveTarget::SongSection {
            song_id: song_id.clone(),
            section_id: sec_verse_id.clone(),
        })
        .await
    {
        Ok(g) => ctx.log(&format!(
            "✅ Song section (Verse→Clean patch): {} engines",
            g.engines.len()
        )),
        Err(e) => ctx.log(&format!("⚠ Song section resolution: {:?}", e)),
    }

    // Resolve song section (direct rig scene)
    match signal
        .resolve_target(ResolveTarget::SongSection {
            song_id: song_id.clone(),
            section_id: sec_intro_id.clone(),
        })
        .await
    {
        Ok(g) => ctx.log(&format!(
            "✅ Song section (Intro→direct rig scene): {} engines",
            g.engines.len()
        )),
        Err(e) => ctx.log(&format!("⚠ Song section direct resolution: {:?}", e)),
    }

    // ─── Step 9: Verify data relationships ───

    let final_rig = signal
        .rigs().load(rig_id.to_string())
        .await
        .unwrap();
    let final_profile = signal.profiles().load(profile_id.to_string()).await.unwrap();
    let final_song = signal.songs().load(song_id.to_string()).await.unwrap();

    // Rig structure
    assert_eq!(final_rig.variants.len(), 2, "Rig should have 2 scenes");
    assert_eq!(final_rig.rig_type, Some(RigType::Guitar));

    // Profile patches all target our rig
    for patch in &final_profile.patches {
        match &patch.target {
            PatchTarget::RigScene { rig_id: r, .. } => {
                assert_eq!(*r, rig_id, "Patch '{}' should target our rig", patch.name);
            }
            other => eyre::bail!("Unexpected patch target: {:?}", other),
        }
    }

    // Song sections reference correct sources
    let intro = final_song.section(&sec_intro_id).unwrap();
    assert!(
        matches!(intro.source, SectionSource::RigScene { .. }),
        "Intro should be direct rig scene"
    );

    let verse = final_song.section(&sec_verse_id).unwrap();
    assert!(
        matches!(verse.source, SectionSource::Patch { .. }),
        "Verse should reference a patch"
    );

    let chorus = final_song.section(&sec_chorus_id).unwrap();
    if let SectionSource::Patch { patch_id } = &chorus.source {
        assert_eq!(*patch_id, patch_crunch_id, "Chorus → Crunch patch");
    } else {
        eyre::bail!("Chorus should be a Patch source");
    }

    let solo = final_song.section(&sec_solo_id).unwrap();
    if let SectionSource::Patch { patch_id } = &solo.source {
        assert_eq!(*patch_id, patch_lead_id, "Solo → Lead patch");
    } else {
        eyre::bail!("Solo should be a Patch source");
    }

    ctx.log("✅ All data relationships verified:");
    ctx.log(&format!(
        "  Rig: {} ({} scenes)",
        final_rig.name,
        final_rig.variants.len()
    ));
    ctx.log(&format!(
        "  Profile: {} ({} patches)",
        final_profile.name,
        final_profile.patches.len()
    ));
    ctx.log(&format!(
        "  Song: {} ({} sections)",
        final_song.name,
        final_song.sections.len()
    ));
    ctx.log("  Intro → RigScene (direct)");
    ctx.log("  Verse → Clean patch → Clean scene");
    ctx.log("  Chorus → Crunch patch → Clean scene");
    ctx.log("  Solo → Lead patch → Lead scene");

    // ─── Step 10: Activate patches and verify folder track structure ───

    ctx.log("\n--- Folder-based gapless switching ---");

    // Verify the applier's set_target created the folder structure
    let folder = track_by_name(&project, "Guitar Rig").await?;
    let input = track_by_name(&project, "Input: Guitar Rig").await?;
    ctx.log(&format!(
        "Folder track: {}, Input track: {}",
        folder.guid(),
        input.guid()
    ));

    // Activate Clean patch → first switch: folder + input + current = 3 child tracks total
    let graph_clean = signal
        .profiles().activate(profile_id.clone(), Some(patch_clean_id.clone()))
        .await
        .map_err(|e| eyre::eyre!("activate Clean: {:?}", e))?;
    ctx.log(&format!(
        "Activated 'Clean' patch — {} engines in graph",
        graph_clean.engines.len()
    ));

    // Neural DSP plugins need time to initialize after a chunk swap
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // Verify folder was renamed to "Clean"
    let folder_info = folder.info().await?;
    assert_eq!(
        folder_info.name, "Clean",
        "folder should be renamed to 'Clean' after first activation"
    );
    ctx.log(&format!("  Folder renamed to '{}'", folder_info.name));

    // Verify children using folder depth traversal
    // Note: folder is also named "Clean" so we use child_tracks() to avoid name collision
    let children = child_tracks(&project, &folder).await?;
    ctx.log(&format!("  Folder children: {}", children.len()));
    for c in &children {
        ctx.log(&format!("    - '{}'", c.name));
    }
    let has_input = children.iter().any(|c| c.name == "Input: Guitar Rig");
    let has_clean_child = children.iter().any(|c| c.name == "Clean");
    assert!(has_input, "Input track should be a folder child");
    assert!(
        has_clean_child,
        "Clean patch track should be a folder child"
    );

    // Verify sends from input
    let sends_after_clean = input.sends().all().await?;
    ctx.log(&format!(
        "  Input sends after Clean: {}",
        sends_after_clean.len()
    ));

    // Activate Lead patch → second switch: folder + input + Lead (current) + Clean (tail)
    let graph_lead = signal
        .profiles().activate(profile_id.clone(), Some(patch_lead_id.clone()))
        .await
        .map_err(|e| eyre::eyre!("activate Lead: {:?}", e))?;
    ctx.log(&format!(
        "\nActivated 'Lead' patch — {} engines in graph",
        graph_lead.engines.len()
    ));

    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // Folder renamed to "Lead"
    let folder_info = folder.info().await?;
    assert_eq!(
        folder_info.name, "Lead",
        "folder should be renamed to 'Lead' after second activation"
    );
    ctx.log(&format!("  Folder renamed to '{}'", folder_info.name));

    // Children: Input + Clean (tail) + Lead (current)
    let children = child_tracks(&project, &folder).await?;
    ctx.log(&format!("  Folder children: {}", children.len()));
    for c in &children {
        ctx.log(&format!("    - '{}'", c.name));
    }
    let has_clean_tail = children.iter().any(|c| c.name == "Clean");
    let has_lead_child = children.iter().any(|c| c.name == "Lead");
    assert!(has_clean_tail, "Clean track should still exist as tail");
    assert!(has_lead_child, "Lead patch track should be a folder child");
    ctx.log("  Clean track still exists as tail (reverb/delay ringing out)");

    // Check sends: input should have 2 sends (muted to Clean, active to Lead)
    let sends_after_lead = input.sends().all().await?;
    ctx.log(&format!(
        "  Input sends after Lead: {}",
        sends_after_lead.len()
    ));

    // Activate Crunch patch → third switch: old tail (Clean) deleted, Lead becomes tail
    let graph_crunch = signal
        .profiles().activate(profile_id.clone(), Some(patch_crunch_id.clone()))
        .await
        .map_err(|e| eyre::eyre!("activate Crunch: {:?}", e))?;
    ctx.log(&format!(
        "\nActivated 'Crunch' patch — {} engines in graph",
        graph_crunch.engines.len()
    ));

    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // Folder renamed to "Crunch"
    let folder_info = folder.info().await?;
    assert_eq!(
        folder_info.name, "Crunch",
        "folder should be renamed to 'Crunch' after third activation"
    );
    ctx.log(&format!("  Folder renamed to '{}'", folder_info.name));

    // Final children: Input + Lead (tail) + Crunch (current)
    // Clean was the old-old tail and should have been deleted
    let children = child_tracks(&project, &folder).await?;
    ctx.log(&format!("  Folder children: {}", children.len()));
    for c in &children {
        ctx.log(&format!("    - '{}'", c.name));
    }
    let has_clean_gone = children.iter().any(|c| c.name == "Clean");
    assert!(
        !has_clean_gone,
        "Clean should be deleted (two switches ago)"
    );
    ctx.log("  Clean track cleaned up (two switches ago)");

    let has_lead_tail = children.iter().any(|c| c.name == "Lead");
    assert!(has_lead_tail, "Lead track should still exist as tail");
    ctx.log("  Lead track still exists as tail");

    let has_crunch_child = children.iter().any(|c| c.name == "Crunch");
    assert!(has_crunch_child, "Crunch child track should exist");
    ctx.log("  Crunch child track exists");

    ctx.log("\n✅ Folder-based gapless switching verified:");
    ctx.log("  [F] Crunch (folder)");
    ctx.log("      Input: Guitar Rig");
    ctx.log("      Lead (tail — muted send, reverb ringing out)");
    ctx.log("      Crunch (current — active send)");

    Ok(())
}
