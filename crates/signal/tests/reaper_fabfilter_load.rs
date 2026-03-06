//! REAPER integration test: Load FabFilter presets onto a DAW track.
//!
//! Imports all FabFilter plugins into an in-memory signal controller, then loads
//! one specific preset per plugin onto a single track — testing both the binary
//! state injection path (most plugins) and the parameter-based path (Pro-C 3).
//!
//! Run with:
//!   cargo xtask reaper-test --keep-open fabfilter_load

use std::time::{Duration, Instant};

use futures::future::try_join_all;
use reaper_test::reaper_test;
use signal::BlockType;
use signal_import::fabfilter::FabFilterImporter;
use signal_import::{import_preset_id, IMPORT_NAMESPACE};
use signal_proto::SnapshotId;
use uuid::Uuid;

/// Small sleep to let REAPER/CLAP process changes.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// Ensure REAPER's audio engine is running.
async fn ensure_audio(ctx: &reaper_test::ReaperTestContext) {
    if !ctx.daw.audio_engine().is_running().await.unwrap_or(false) {
        let _ = ctx.daw.audio_engine().init().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// FabFilter plugins to load, with specific snapshot names.
/// (plugin_name, block_type, snapshot_name)
const FABFILTER_CHAIN: &[(&str, BlockType, &str)] = &[
    ("Pro-Q 4", BlockType::Eq, "Bell Surgical"),
    ("Pro-C 3", BlockType::Compressor, "Controlled Sides"),
    ("Pro-R 2", BlockType::Reverb, "Medium Hall 1"),
    ("Timeless 3", BlockType::Delay, "Basic - Modern Delay bM"),
    ("Saturn 2", BlockType::Saturator, "456 Demo MdB"),
    ("Pro-G", BlockType::Gate, "Worn Out Playback MTK"),
    ("Pro-L 2", BlockType::Limiter, "Punchy"),
    ("Pro-DS", BlockType::DeEsser, "Male Wide Band"),
    ("Volcano 3", BlockType::Filter, "Simple Triggered Notch bM"),
];

/// Compute a deterministic snapshot ID for a FabFilter preset snapshot.
fn fabfilter_snapshot_id(plugin_name: &str, snapshot_name: &str, folder: Option<&str>) -> SnapshotId {
    let preset_uuid = Uuid::new_v5(
        &IMPORT_NAMESPACE,
        format!("FabFilter:{plugin_name}").as_bytes(),
    );
    let snap_key = match folder {
        Some(f) => format!("{f}/{snapshot_name}"),
        None => snapshot_name.to_string(),
    };
    let snap_uuid = Uuid::new_v5(&preset_uuid, snap_key.as_bytes());
    SnapshotId::from(snap_uuid.to_string())
}

// ---------------------------------------------------------------------------
// Test: Load one of each FabFilter plugin onto a single track
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn fabfilter_load_full_chain(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    // Bootstrap signal and import all FabFilter plugins in parallel.
    let signal = signal::bootstrap_in_memory_controller_async().await?;
    let importer = FabFilterImporter::new();

    let import_start = Instant::now();
    let collections: Vec<_> = FABFILTER_CHAIN
        .iter()
        .map(|&(plugin_name, _, _)| importer.scan(plugin_name))
        .collect::<Result<Vec<_>, _>>()?;

    let import_futures: Vec<_> = collections
        .into_iter()
        .map(|c| signal_import::import_presets(&signal, c))
        .collect();
    let reports = try_join_all(import_futures).await?;
    for report in &reports {
        eprintln!(
            "  Imported {}: {} snapshots",
            report.preset_name, report.snapshots_imported
        );
    }
    eprintln!("  Imports completed in {:.1}s", import_start.elapsed().as_secs_f64());

    let svc = signal.service();

    // Build snapshot IDs for each plugin.
    let mut loads = Vec::new();
    let mut snap_ids = Vec::new();
    let mut preset_ids = Vec::new();

    for &(plugin_name, block_type, snapshot_name) in FABFILTER_CHAIN {
        let preset_id = import_preset_id("FabFilter", plugin_name);

        let presets = signal.block_presets().list(block_type).await?;
        let preset = presets
            .iter()
            .find(|p| p.id() == &preset_id)
            .ok_or_else(|| eyre::eyre!("{plugin_name} imported preset not found"))?;

        let folder = {
            let default = preset.default_snapshot();
            if default.name() == snapshot_name {
                default.metadata().folder.clone()
            } else {
                preset.snapshots().iter()
                    .find(|s| s.name() == snapshot_name)
                    .and_then(|s| s.metadata().folder.clone())
            }
        };
        let snap_id = fabfilter_snapshot_id(plugin_name, snapshot_name, folder.as_deref());
        snap_ids.push(snap_id);
        preset_ids.push(preset_id);
        loads.push(block_type);
    }

    // Create a test track and load all plugins in parallel.
    let track = project.tracks().add("FabFilter Full Chain", None).await?;
    settle().await;

    let total_start = Instant::now();

    // Build load specs with references to owned data.
    let load_specs: Vec<_> = loads.iter().enumerate()
        .map(|(i, &bt)| (bt, &preset_ids[i], Some(&snap_ids[i])))
        .collect();

    let results = svc
        .load_blocks_to_track(load_specs, &track)
        .await
        .map_err(|e| eyre::eyre!("Batch load failed: {e}"))?;

    let loaded_count = results.len() as u32;

    for (i, result) in results.iter().enumerate() {
        let &(plugin_name, block_type, snapshot_name) = &FABFILTER_CHAIN[i];
        eprintln!(
            "  [{}] {} '{}' → '{}'",
            block_type.display_name(),
            plugin_name,
            snapshot_name,
            result.display_name,
        );
    }

    settle().await;

    // Verify: all plugins loaded as FX on the track.
    let fx_count = track.fx_chain().count().await?;
    assert_eq!(
        fx_count, loaded_count,
        "expected {} FX on track, got {}",
        loaded_count, fx_count
    );

    // ── Diagnostic dump: parameters for each loaded plugin ──
    eprintln!("\n=== PARAMETER DUMP ===\n");
    for (i, &(plugin_name, _block_type, snapshot_name)) in FABFILTER_CHAIN.iter().enumerate() {
        let fx = track.fx_chain().by_index(i as u32).await?
            .ok_or_else(|| eyre::eyre!("FX at index {i} not found"))?;

        eprintln!("── {} ('{}') ──", plugin_name, snapshot_name);

        if let Ok(Some(preset_info)) = fx.preset_index().await {
            eprintln!("  Active preset: {:?} (index {:?} of {})",
                preset_info.name, preset_info.index, preset_info.count);
        }

        let params = fx.parameters().await?;
        let interesting: Vec<_> = params.iter()
            .filter(|p| (p.value - 0.0).abs() > 0.001 && (p.value - 0.5).abs() > 0.001 && (p.value - 1.0).abs() > 0.001)
            .take(15)
            .collect();
        if interesting.is_empty() {
            eprintln!("  PARAMS: all at defaults (0.0/0.5/1.0)");
        } else {
            for p in &interesting {
                eprintln!("  [{}] {} = {:.4} ({})", p.index, p.name, p.value, p.formatted);
            }
        }
        eprintln!();
    }

    let total_elapsed = total_start.elapsed();
    eprintln!(
        "\nAll {} FabFilter plugins loaded in {:.1}s (avg {:.1}s/plugin)\n",
        loaded_count,
        total_elapsed.as_secs_f64(),
        total_elapsed.as_secs_f64() / loaded_count as f64,
    );

    ctx.log(&format!(
        "fabfilter_load_full_chain: PASS — {} plugins in {:.1}s",
        loaded_count,
        total_elapsed.as_secs_f64(),
    ));
    Ok(())
}
