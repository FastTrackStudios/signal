//! REAPER integration test: Capture module presets as `.RfxChain` files.
//!
//! Loads each module preset from signal.db onto a REAPER track, captures
//! the resulting FX chain chunk, and writes it as a `.RfxChain` file to
//! `FXChains/FTS-Signal/02-Modules/`.
//!
//! This is a migration harness — run once to populate the FXChains directory
//! with native REAPER module presets from the database.
//!
//! Run with:
//!   cargo xtask reaper-test module_capture

use std::path::PathBuf;
use std::time::Duration;

use reaper_test::reaper_test;
use signal::sidecar::{self, PresetKind, SignalSidecar};
use signal::{ModuleRepo, ModuleRepoLive};

/// Sleep to let REAPER process FX changes.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

/// Ensure REAPER's audio engine is running (required for plugin instantiation).
async fn ensure_audio(ctx: &reaper_test::ReaperTestContext) {
    if !ctx.daw.audio_engine().is_running().await.unwrap_or(false) {
        let _ = ctx.daw.audio_engine().init().await;
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

/// Output directory for captured module RfxChains.
fn modules_output_dir() -> PathBuf {
    utils::paths::reaper_fxchains().join("FTS-Signal/02-Modules")
}

/// Sanitize a name for use as a filename.
fn sanitize(name: &str) -> String {
    name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

use signal::strip_fxchain_wrapper;

// ---------------------------------------------------------------------------
// Main capture harness
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn module_capture(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    // Connect to the real signal.db
    let db_path = utils::paths::signal_db().to_string_lossy().to_string();
    let signal = signal::connect_db_seeded(&db_path).await?;
    let svc = signal.service();

    // List all module presets from the DB via a separate repo connection
    let db_url = format!("sqlite:{db_path}?mode=ro");
    let db = signal::Database::connect(&db_url).await?;
    let module_repo = ModuleRepoLive::new(db);
    module_repo.init_schema().await?;
    let modules = module_repo.list_module_collections().await?;
    let total = modules.len();
    ctx.log(&format!("Found {total} module presets to capture"));

    let output_dir = modules_output_dir();
    std::fs::create_dir_all(&output_dir)?;

    let mut captured = 0usize;
    let mut skipped = 0usize;
    let mut failed = Vec::new();

    for (i, module_preset) in modules.iter().enumerate() {
        let name = module_preset.name().to_string();
        let file_name = sanitize(&name);
        let rfx_path = output_dir.join(format!("{file_name}.RfxChain"));

        // Skip if already captured
        if rfx_path.exists() {
            ctx.log(&format!(
                "[{}/{}] SKIP {name} (already exists)",
                i + 1,
                total
            ));
            skipped += 1;
            continue;
        }

        ctx.log(&format!("[{}/{}] Capturing: {name}", i + 1, total));

        // Create a fresh track for this module
        let track = project
            .tracks()
            .add(&format!("Capture: {name}"), None)
            .await?;
        settle().await;

        // Load the module onto the track
        let module_id = module_preset.id().clone();
        let load_result = svc
            .load_module_to_track(
                module_preset.module_type(),
                &module_id,
                0, // default snapshot
                &track,
            )
            .await;

        match load_result {
            Ok(result) => {
                settle().await;

                // Capture the FX chain chunk text
                match track.fx_chain().fx_chain_chunk_text().await {
                    Ok(chunk_text) => {
                        // Write the .RfxChain file — strip <FXCHAIN> wrapper if present.
                        // REAPER's .RfxChain format is the INNER content of the FXCHAIN block,
                        // starting directly with BYPASS/SHOW/<VST> lines.
                        let rfx_content = strip_fxchain_wrapper(&chunk_text);
                        std::fs::write(&rfx_path, &rfx_content)?;

                        // Write .signal.styx sidecar
                        let metadata = module_preset.metadata();
                        let sc = SignalSidecar {
                            version: 1,
                            id: module_preset.id().to_string(),
                            kind: PresetKind::Module,
                            tags: metadata.tags.as_slice().to_vec(),
                            description: metadata.description.clone(),
                            parameters: vec![],
                        };
                        sidecar::write_sidecar(&rfx_path, &sc)?;

                        ctx.log(&format!(
                            "  OK — {} FX, chunk {} bytes",
                            result.loaded_fx.len(),
                            rfx_content.len()
                        ));
                        captured += 1;
                    }
                    Err(e) => {
                        ctx.log(&format!("  FAIL (no FX chain): {e}"));
                        failed.push(format!("{name}: no FX chain — {e}"));
                    }
                }
            }
            Err(e) => {
                ctx.log(&format!("  FAIL (load): {e}"));
                failed.push(format!("{name}: load failed — {e}"));
            }
        }

        // Remove all tracks to keep things clean for the next module
        let _ = project.tracks().remove_all().await;
        // Brief pause between modules to avoid overwhelming REAPER
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // Summary
    ctx.log(&format!(
        "\n=== Module Capture Complete ===\n  Total: {total}\n  Captured: {captured}\n  Skipped: {skipped}\n  Failed: {}",
        failed.len()
    ));

    if !failed.is_empty() {
        ctx.log("\nFailed modules:");
        for f in &failed {
            ctx.log(&format!("  - {f}"));
        }
    }

    // Verify scanner picks up captured modules
    let fxchains_root = output_dir.parent().unwrap();
    let scanned = signal::fxchains::scan_modules(fxchains_root);
    ctx.log(&format!(
        "\nScanner verification: found {} module presets",
        scanned.len()
    ));

    assert!(
        captured > 0 || skipped > 0,
        "should have captured or skipped at least one module"
    );

    Ok(())
}
