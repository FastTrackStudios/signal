//! REAPER integration test: Build a layer, save, verify file content, save update.
//!
//! Validates that:
//! 1. `save_track_template()` writes the exact captured chunk to disk
//! 2. Saving a new variation doesn't overwrite the original
//! 3. The sidecar `.signal.styx` is written alongside
//! 4. Track modifications (volume change) produce different chunk data
//!
//! Note: We don't test REAPER's native template loading here (that's REAPER's
//! responsibility). We verify our save infrastructure writes correct files.
//!
//! Run with:
//!   cargo xtask reaper-test profile_update_roundtrip

use std::time::Duration;

use reaper_test::reaper_test;
use signal::track_template::{self, Instrument, TemplateTier};
use signal::{ModuleRepo, ModuleRepoLive};

async fn settle() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

async fn ensure_audio(ctx: &reaper_test::ReaperTestContext) {
    if !ctx.daw.audio_engine().is_running().await.unwrap_or(false) {
        let _ = ctx.daw.audio_engine().init().await;
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

#[reaper_test(isolated)]
async fn profile_update_roundtrip(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    // ── Step 1: Build a layer with modules ──

    let db_path = utils::paths::signal_db().to_string_lossy().to_string();
    let signal = signal::connect_db_seeded(&db_path).await?;
    let svc = signal.service();

    let db_url = format!("sqlite:{db_path}?mode=ro");
    let db = signal::Database::connect(&db_url).await?;
    let module_repo = ModuleRepoLive::new(db);
    module_repo.init_schema().await?;
    let all_modules = module_repo.list_module_collections().await?;

    let mut module_by_type = std::collections::HashMap::new();
    for module in &all_modules {
        let type_key = module.module_type().as_str().to_string();
        module_by_type.entry(type_key).or_insert(module);
    }

    let track = project.tracks().add("[L] Roundtrip Test", None).await?;
    settle().await;

    for type_key in &["input", "amp", "master"] {
        if let Some(mp) = module_by_type.get(*type_key) {
            let _ = svc
                .load_module_to_track(mp.module_type(), &mp.id().clone(), 0, &track)
                .await;
            settle().await;
        }
    }

    let fx_count = track.fx_chain().count().await?;
    ctx.log(&format!("Built layer with {} FX", fx_count));
    assert!(fx_count > 0, "layer should have FX");

    // ── Step 2: Capture and save as "Default" variation ──

    let chunk_v1 = track.get_chunk().await?;
    ctx.log(&format!("V1 chunk: {} bytes", chunk_v1.len()));
    assert!(chunk_v1.contains("<FXCHAIN"), "chunk should have FXCHAIN");
    assert!(
        chunk_v1.contains("<CONTAINER")
            || chunk_v1.contains("<VST ")
            || chunk_v1.contains("<CLAP "),
        "chunk should contain plugin data"
    );

    let path_v1 = track_template::save_track_template(
        "Roundtrip-Test",
        "Default",
        Instrument::Guitar,
        TemplateTier::Layer,
        &chunk_v1,
        &uuid::Uuid::new_v4().to_string(),
        &["test".to_string()],
        Some("Original layer state"),
    )?;
    ctx.log(&format!("Saved V1: {}", path_v1.display()));

    // Verify file matches chunk exactly
    let on_disk_v1 = std::fs::read_to_string(&path_v1)?;
    assert_eq!(on_disk_v1, chunk_v1, "V1 file should match captured chunk");

    // Verify sidecar exists
    let sidecar_v1 = signal::sidecar::sidecar_path(&path_v1);
    assert!(sidecar_v1.exists(), "V1 sidecar should exist");

    // Parse sidecar and verify
    let sc = signal::sidecar::read_sidecar(&path_v1).expect("sidecar should parse");
    assert_eq!(sc.version, 1);
    assert_eq!(sc.kind, signal::sidecar::PresetKind::Layer);
    assert!(sc.tags.contains(&"test".to_string()));
    ctx.log("V1 sidecar: valid ✓");

    // ── Step 3: Modify the track and save as "Updated" variation ──

    track.set_volume(0.42).await?;
    track.set_pan(-0.3).await?;
    settle().await;

    let chunk_v2 = track.get_chunk().await?;
    ctx.log(&format!("V2 chunk: {} bytes", chunk_v2.len()));

    // V2 should differ from V1 (volume/pan changed)
    assert_ne!(
        chunk_v1, chunk_v2,
        "modified chunk should differ from original"
    );

    // FXCHAIN should be identical (we only changed track-level params)
    let fxchain_v1 = extract_fxchain(&chunk_v1).expect("V1 FXCHAIN");
    let fxchain_v2 = extract_fxchain(&chunk_v2).expect("V2 FXCHAIN");
    assert_eq!(
        fxchain_v1, fxchain_v2,
        "FXCHAIN should be identical (only track params changed)"
    );
    ctx.log("FXCHAIN unchanged after track-level edits ✓");

    let path_v2 = track_template::save_track_template(
        "Roundtrip-Test",
        "Updated",
        Instrument::Guitar,
        TemplateTier::Layer,
        &chunk_v2,
        &uuid::Uuid::new_v4().to_string(),
        &["test".to_string(), "updated".to_string()],
        Some("Layer with modified volume and pan"),
    )?;
    ctx.log(&format!("Saved V2: {}", path_v2.display()));

    // ── Step 4: Verify both variations coexist ──

    assert!(path_v1.exists(), "V1 should still exist");
    assert!(path_v2.exists(), "V2 should exist");

    // V1 should be unchanged
    let still_v1 = std::fs::read_to_string(&path_v1)?;
    assert_eq!(still_v1, chunk_v1, "V1 file should be unchanged");

    // V2 should match the modified chunk
    let on_disk_v2 = std::fs::read_to_string(&path_v2)?;
    assert_eq!(on_disk_v2, chunk_v2, "V2 file should match modified chunk");
    ctx.log("Both variations coexist correctly ✓");

    // ── Step 5: Overwrite V1 with a new save ──

    track.set_volume(1.0).await?; // Reset volume
    settle().await;

    let chunk_v1_updated = track.get_chunk().await?;
    let path_v1_again = track_template::save_track_template(
        "Roundtrip-Test",
        "Default", // Same variation name — should overwrite
        Instrument::Guitar,
        TemplateTier::Layer,
        &chunk_v1_updated,
        &uuid::Uuid::new_v4().to_string(),
        &["test".to_string()],
        Some("Overwritten default"),
    )?;

    // Should be the same path
    assert_eq!(path_v1, path_v1_again, "same variation should overwrite");

    // Content should be the new chunk
    let overwritten = std::fs::read_to_string(&path_v1)?;
    assert_eq!(
        overwritten, chunk_v1_updated,
        "overwritten file should have new content"
    );
    assert_ne!(
        overwritten, chunk_v1,
        "overwritten should differ from original V1"
    );
    ctx.log("Overwrite of Default variation: correct ✓");

    // ── Step 6: Verify scanner finds both variations ──

    let root = track_template::track_templates_root();
    let scanned = track_template::scan_track_templates(&root);
    let roundtrip_variants: Vec<_> = scanned
        .iter()
        .filter(|t| t.preset_name == "Roundtrip-Test")
        .collect();
    ctx.log(&format!(
        "Scanner found {} Roundtrip-Test variations",
        roundtrip_variants.len()
    ));
    for t in &roundtrip_variants {
        ctx.log(&format!("  {} / {}", t.preset_name, t.variation_name));
    }
    assert_eq!(roundtrip_variants.len(), 2, "should have Default + Updated");

    // ── Cleanup ──

    let _ = std::fs::remove_file(&path_v1);
    let _ = std::fs::remove_file(signal::sidecar::sidecar_path(&path_v1));
    let _ = std::fs::remove_file(&path_v2);
    let _ = std::fs::remove_file(signal::sidecar::sidecar_path(&path_v2));
    if let Some(parent) = path_v1.parent() {
        let _ = std::fs::remove_dir(parent);
    }
    ctx.log("Cleaned up");

    ctx.log("\nprofile_update_roundtrip: PASS");
    Ok(())
}

fn extract_fxchain(chunk: &str) -> Option<&str> {
    let start = chunk.find("<FXCHAIN")?;
    let rest = &chunk[start..];
    let mut depth = 0i32;
    for (i, ch) in rest.char_indices() {
        if ch == '<' {
            depth += 1;
        } else if ch == '>' {
            depth -= 1;
            if depth == 0 {
                return Some(&chunk[start..start + i + 1]);
            }
        }
    }
    None
}
