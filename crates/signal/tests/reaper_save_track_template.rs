//! REAPER integration test: Save layers and rigs as `.RTrackTemplate` files.
//!
//! Builds a guitar layer (track with modules loaded), captures its track chunk,
//! and saves it as a `.RTrackTemplate` in `TrackTemplates/FTS-Signal/Guitar/01-Layers/`.
//! Then builds a full rig (folder track + layer tracks) and saves as a rig template.
//!
//! Run with:
//!   cargo xtask reaper-test save_track_template

use std::time::Duration;

use reaper_test::reaper_test;
use signal::track_template::{self, Instrument, TemplateTier};
use signal::{BlockRepo, BlockType};

/// Sleep to let REAPER process changes.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

async fn ensure_audio(ctx: &reaper_test::ReaperTestContext) {
    if !ctx.daw.audio_engine().is_running().await.unwrap_or(false) {
        let _ = ctx.daw.audio_engine().init().await;
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

// ---------------------------------------------------------------------------
// Test: Save a layer as a track template
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn save_track_template(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    // Bootstrap signal with the real DB for block presets
    let db_path = utils::paths::signal_db().to_string_lossy().to_string();
    let signal = signal::connect_db_seeded(&db_path).await?;
    let svc = signal.service();

    // ── Build a layer: track with a module loaded ──

    let layer_name = "Test Clean Layer";
    let track = project.tracks().add(layer_name, None).await?;
    settle().await;

    // Load blocks directly from the DB to build a layer
    let db_url = format!("sqlite:{db_path}?mode=ro");
    let db = signal::Database::connect(&db_url).await?;
    let block_repo = signal::BlockRepoLive::new(db);
    block_repo.init_schema().await?;

    // Load an amp block
    let amp_collections = block_repo.list_block_collections(BlockType::Amp).await?;
    if let Some(amp) = amp_collections.first() {
        ctx.log(&format!("Loading amp: {}", amp.name()));
        svc.load_block_to_track(BlockType::Amp, amp.id(), None, &track)
            .await
            .map_err(|e| eyre::eyre!("{e}"))?;
        settle().await;
    }

    // Load a drive block
    let drive_collections = block_repo.list_block_collections(BlockType::Drive).await?;
    if let Some(drive) = drive_collections.first() {
        ctx.log(&format!("Loading drive: {}", drive.name()));
        svc.load_block_to_track(BlockType::Drive, drive.id(), None, &track)
            .await
            .map_err(|e| eyre::eyre!("{e}"))?;
        settle().await;
    }

    // Verify we have FX on the track
    let fx_count = track.fx_chain().count().await?;
    ctx.log(&format!("Layer has {fx_count} FX"));
    assert!(fx_count > 0, "layer should have at least 1 FX");

    // ── Capture and save as track template ──

    let track_chunk = track.get_chunk().await?;
    ctx.log(&format!("Track chunk: {} bytes", track_chunk.len()));

    let template_path = track_template::save_track_template(
        layer_name,
        "Default",
        Instrument::Guitar,
        TemplateTier::Layer,
        &track_chunk,
        &uuid::Uuid::new_v4().to_string(),
        &["test".to_string(), "clean".to_string()],
        Some("Test clean guitar layer"),
    )?;

    ctx.log(&format!(
        "Saved layer template: {}",
        template_path.display()
    ));
    assert!(template_path.exists(), "template file should exist");

    // Verify sidecar was written
    let sidecar_path = signal::sidecar::sidecar_path(&template_path);
    assert!(sidecar_path.exists(), "sidecar file should exist");

    // Verify it can be read back
    let content = std::fs::read_to_string(&template_path)?;
    assert!(
        content.contains("<TRACK"),
        "template should contain <TRACK block"
    );

    // ── Now build a rig: folder track + 2 layer tracks ──

    // Clean up first
    let _ = project.tracks().remove_all().await;
    settle().await;

    let rig_name = "Test Guitar Rig";

    // Create folder track
    let folder_track = project.tracks().add(rig_name, None).await?;
    settle().await;

    // Create layer tracks
    let clean_layer = project.tracks().add("Clean Layer", None).await?;
    settle().await;

    let drive_layer = project.tracks().add("Drive Layer", None).await?;
    settle().await;

    // Load some FX on each layer (reuse the block_repo from earlier)
    if let Some(amp) = amp_collections.first() {
        let _ = svc
            .load_block_to_track(BlockType::Amp, amp.id(), None, &clean_layer)
            .await;
        settle().await;
        let _ = svc
            .load_block_to_track(BlockType::Amp, amp.id(), None, &drive_layer)
            .await;
        settle().await;
    }

    // Capture all tracks as a rig template
    let mut rig_chunks = String::new();
    rig_chunks.push_str(&folder_track.get_chunk().await?);
    rig_chunks.push('\n');
    rig_chunks.push_str(&clean_layer.get_chunk().await?);
    rig_chunks.push('\n');
    rig_chunks.push_str(&drive_layer.get_chunk().await?);

    let rig_path = track_template::save_track_template(
        rig_name,
        "Default",
        Instrument::Guitar,
        TemplateTier::Rig,
        &rig_chunks,
        &uuid::Uuid::new_v4().to_string(),
        &["test".to_string(), "guitar".to_string()],
        Some("Test guitar rig with clean and drive layers"),
    )?;

    ctx.log(&format!("Saved rig template: {}", rig_path.display()));
    assert!(rig_path.exists(), "rig template file should exist");

    // Verify rig template contains multiple tracks
    let rig_content = std::fs::read_to_string(&rig_path)?;
    let track_count = rig_content.matches("<TRACK").count();
    ctx.log(&format!("Rig template has {track_count} tracks"));
    assert!(
        track_count >= 3,
        "rig should have at least 3 tracks (folder + 2 layers), got {track_count}"
    );

    // ── Verify scanner finds our templates ──

    let root = track_template::track_templates_root();
    let scanned = track_template::scan_track_templates(&root);
    ctx.log(&format!("Scanner found {} track templates", scanned.len()));
    assert!(
        scanned.len() >= 2,
        "should find at least 2 templates (layer + rig)"
    );

    // Verify the layer template is in Guitar/01-Layers/
    let layer_found = scanned.iter().any(|t| {
        t.preset_name == layer_name
            && t.instrument.as_deref() == Some("Guitar")
            && t.tier == Some(TemplateTier::Layer)
    });
    assert!(
        layer_found,
        "should find the layer template in scanner results"
    );

    // Verify the rig template is in Guitar/03-Rigs/
    let rig_found = scanned.iter().any(|t| {
        t.preset_name == rig_name
            && t.instrument.as_deref() == Some("Guitar")
            && t.tier == Some(TemplateTier::Rig)
    });
    assert!(rig_found, "should find the rig template in scanner results");

    // ── Verify the saved template is valid ──

    // The template file contains a valid <TRACK> block — REAPER can load it
    // via its native Track Template menu. We verify the content is well-formed.
    let saved_content = std::fs::read_to_string(&template_path)?;
    assert!(
        saved_content.contains("<FXCHAIN"),
        "template should contain FXCHAIN block"
    );
    assert!(
        saved_content.contains("<VST ") || saved_content.contains("<CLAP "),
        "template should contain plugin blocks"
    );
    ctx.log("Layer template validated: contains FXCHAIN + plugin blocks");

    // Clean up test templates
    let _ = std::fs::remove_file(&template_path);
    let _ = std::fs::remove_file(signal::sidecar::sidecar_path(&template_path));
    let _ = std::fs::remove_file(&rig_path);
    let _ = std::fs::remove_file(signal::sidecar::sidecar_path(&rig_path));

    ctx.log("save_track_template: PASS");
    Ok(())
}
