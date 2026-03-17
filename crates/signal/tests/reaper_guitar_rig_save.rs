//! REAPER integration test: Build a full guitar rig layer and save as track templates.
//!
//! Loads all modules from signal.db onto a single layer track in the standard
//! guitar signal chain order, then captures the track as a `.RTrackTemplate`.
//!
//! Chain order: Input → Drive → Amp → Modulation → Time → Dynamics → Master
//!
//! Run with:
//!   cargo xtask reaper-test guitar_rig_save

use std::time::Duration;

use reaper_test::reaper_test;
use signal::{ModuleRepo, ModuleRepoLive};
use signal::track_template::{self, Instrument, TemplateTier};

async fn settle() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

async fn ensure_audio(ctx: &reaper_test::ReaperTestContext) {
    if !ctx.daw.audio_engine().is_running().await.unwrap_or(false) {
        let _ = ctx.daw.audio_engine().init().await;
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

/// Module load order for a standard guitar signal chain.
const CHAIN_ORDER: &[(&str, &str)] = &[
    // (module_type storage key, description)
    ("input", "Input gate/gain"),
    ("drive", "Drive/overdrive"),
    ("amp", "Amp sim + cab"),
    ("modulation", "Chorus/flanger/phaser"),
    ("time", "Delay + reverb"),
    ("dynamics", "Compressor"),
    ("master", "EQ + limiter"),
];

// ---------------------------------------------------------------------------
// Build and save a complete guitar layer
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn guitar_rig_save(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    // Connect to the real signal.db
    let db_path = utils::paths::signal_db().to_string_lossy().to_string();
    let signal = signal::connect_db_seeded(&db_path).await?;
    let svc = signal.service();

    // Get all modules from the DB
    let db_url = format!("sqlite:{db_path}?mode=ro");
    let db = signal::Database::connect(&db_url).await?;
    let module_repo = ModuleRepoLive::new(db);
    module_repo.init_schema().await?;
    let all_modules = module_repo.list_module_collections().await?;

    ctx.log(&format!("Found {} module presets in DB", all_modules.len()));

    // Build a lookup: module_type → first module preset of that type
    let mut module_by_type = std::collections::HashMap::new();
    for module in &all_modules {
        let type_key = module.module_type().as_str().to_string();
        module_by_type.entry(type_key).or_insert(module);
    }

    ctx.log(&format!(
        "Unique module types: {:?}",
        module_by_type.keys().collect::<Vec<_>>()
    ));

    // ── Build the guitar layer track ──

    let layer_name = "[L] Guitar Layer — Full Chain";
    let track = project.tracks().add(layer_name, None).await?;
    settle().await;

    let mut loaded_modules = Vec::new();

    for &(type_key, desc) in CHAIN_ORDER {
        let Some(module_preset) = module_by_type.get(type_key) else {
            ctx.log(&format!("  SKIP {type_key} ({desc}) — no module found"));
            continue;
        };

        let module_type = module_preset.module_type();
        let module_id = module_preset.id().clone();
        let name = module_preset.name().to_string();

        ctx.log(&format!("  Loading [{type_key}] {name}..."));

        match svc
            .load_module_to_track(module_type, &module_id, 0, &track)
            .await
        {
            Ok(result) => {
                settle().await;
                ctx.log(&format!(
                    "    OK — {} FX ({})",
                    result.loaded_fx.len(),
                    result.display_name
                ));
                loaded_modules.push((type_key.to_string(), name, result.loaded_fx.len()));
            }
            Err(e) => {
                ctx.log(&format!("    FAIL — {e}"));
            }
        }
    }

    // Verify we loaded something
    let total_fx = track.fx_chain().count().await?;
    ctx.log(&format!(
        "\nLayer complete: {} modules, {} total FX on track",
        loaded_modules.len(),
        total_fx
    ));
    assert!(total_fx > 0, "layer should have FX loaded");

    // ── Save the layer as a track template ──

    let track_chunk = track.get_chunk().await?;
    ctx.log(&format!("Track chunk: {} bytes", track_chunk.len()));

    let layer_path = track_template::save_track_template(
        "Guitar Full Chain",
        "Default",
        Instrument::Guitar,
        TemplateTier::Layer,
        &track_chunk,
        &uuid::Uuid::new_v4().to_string(),
        &[
            "guitar".to_string(),
            "full-chain".to_string(),
        ],
        Some("Full guitar signal chain: input → drive → amp → modulation → time → dynamics → master"),
    )?;

    ctx.log(&format!("Saved layer: {}", layer_path.display()));

    // ── Now build a proper rig with folder structure ──

    // Clear and rebuild
    let _ = project.tracks().remove_all().await;
    settle().await;

    let rig_name = "Guitar Rig";

    // Create the rig folder track (no FX, just a label)
    let folder = project.tracks().add(&format!("RIG: {rig_name}"), None).await?;
    settle().await;

    // Re-create the layer track with all modules
    let layer = project.tracks().add("[L] Guitar Main", None).await?;
    settle().await;

    for &(type_key, _desc) in CHAIN_ORDER {
        let Some(module_preset) = module_by_type.get(type_key) else {
            continue;
        };
        let module_type = module_preset.module_type();
        let module_id = module_preset.id().clone();
        let _ = svc
            .load_module_to_track(module_type, &module_id, 0, &layer)
            .await;
        settle().await;
    }

    let layer_fx = layer.fx_chain().count().await?;
    ctx.log(&format!("Rig layer has {layer_fx} FX"));

    // Capture the rig as a multi-track template
    let mut rig_chunks = String::new();
    rig_chunks.push_str(&folder.get_chunk().await?);
    rig_chunks.push('\n');
    rig_chunks.push_str(&layer.get_chunk().await?);

    let rig_path = track_template::save_track_template(
        rig_name,
        "Default",
        Instrument::Guitar,
        TemplateTier::Rig,
        &rig_chunks,
        &uuid::Uuid::new_v4().to_string(),
        &["guitar".to_string(), "rig".to_string()],
        Some("Guitar rig: folder + full-chain layer"),
    )?;

    ctx.log(&format!("Saved rig: {}", rig_path.display()));

    // ── Summary ──

    let root = track_template::track_templates_root();
    let scanned = track_template::scan_track_templates(&root);
    ctx.log(&format!("\nScanner found {} track templates:", scanned.len()));
    for t in &scanned {
        ctx.log(&format!(
            "  {}/{:?}: {} / {} ({})",
            t.instrument.as_deref().unwrap_or("?"),
            t.tier,
            t.preset_name,
            t.variation_name,
            t.path.display()
        ));
    }

    // Verify files exist
    assert!(layer_path.exists(), "layer template should exist");
    assert!(rig_path.exists(), "rig template should exist");

    // Verify content
    let layer_content = std::fs::read_to_string(&layer_path)?;
    assert!(layer_content.contains("<FXCHAIN"), "layer should have FXCHAIN");

    let rig_content = std::fs::read_to_string(&rig_path)?;
    let track_count = rig_content.matches("<TRACK").count();
    assert_eq!(track_count, 2, "rig should have 2 tracks (folder + layer)");

    ctx.log("\nguitar_rig_save: PASS");
    Ok(())
}
