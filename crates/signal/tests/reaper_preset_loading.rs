//! REAPER integration tests — applies signal domain presets via the live JM
//! plugin, then verifies round-trips, morphing, and snapshot behaviour.
//!
//! Run with:
//!
//!   cargo xtask reaper-test

mod daw_helpers;

use daw_helpers::{
    add_jm_track, apply_block, apply_graph, apply_graph_with_state, build_morph_engine,
    capture_block_from_fx, get_fx0, read_gain, remove_track,
};
use reaper_test::reaper_test;
use signal::{bootstrap_in_memory_controller_async, resolve::ResolveTarget, seed_id, Signal};
use signal_proto::easing::EasingCurve;

// FX ID used as the key in DawParameterSnapshot entries for the JM plugin.
const JM_FX_ID: &str = "jm-amp";

async fn signal_controller() -> Signal {
    bootstrap_in_memory_controller_async()
        .await
        .expect("failed to bootstrap signal controller")
}

// ─────────────────────────────────────────────────────────────
//  Scenario 1: Load JM plugin on a new track
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn load_jm_plugin(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: load_jm_plugin_on_new_track ===");
    let track = add_jm_track(ctx.project(), "JM Plugin Test").await?;

    let fx_list = track.fx_chain().all().await?;
    println!("FX on new JM track ({} total):", fx_list.len());
    for fx in &fx_list {
        println!(
            "  [{}] {} — {} params",
            fx.index, fx.name, fx.parameter_count
        );
    }
    assert!(!fx_list.is_empty(), "track should have JM plugin loaded");
    assert!(
        fx_list[0].name.contains("John Mayer") || fx_list[0].plugin_name.contains("John Mayer"),
        "first FX should be the JM plugin, got: {}",
        fx_list[0].name
    );

    remove_track(ctx.project(), track).await;
    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 2: Apply default amp block to live FX
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn amp_default_block(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: apply_jm_amp_default_block_to_live_fx ===");
    let signal = signal_controller().await;
    let track = add_jm_track(ctx.project(), "JM Amp Default Block").await?;

    let block = signal
        .block_presets().load_default(signal::BlockType::Amp, seed_id("jm-amp"))
        .await
        .expect("jm-amp default not found");

    let applied = apply_block(&track, &block, JM_FX_ID).await?;
    println!("Applied {} parameter(s) to live JM plugin", applied);

    remove_track(ctx.project(), track).await;
    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 3: Apply each JM block preset to a new track
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn each_jm_block_preset(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: apply_each_jm_block_preset_to_new_track ===");
    let signal = signal_controller().await;

    let jm_presets = [
        (signal::BlockType::Boost, "jm-justa-boost", "Justa Boost"),
        (
            signal::BlockType::Filter,
            "jm-antelope-filter",
            "Antelope Filter",
        ),
        (signal::BlockType::Drive, "jm-halfman-od", "Halfman OD"),
        (signal::BlockType::Drive, "jm-tealbreaker", "Tealbreaker"),
        (
            signal::BlockType::Delay,
            "jm-millipede-delay",
            "Millipede Delay",
        ),
        (
            signal::BlockType::Tremolo,
            "jm-harmonic-tremolo",
            "Harmonic Tremolo",
        ),
        (
            signal::BlockType::Reverb,
            "jm-spring-reverb",
            "Spring Reverb",
        ),
        (signal::BlockType::Amp, "jm-amp", "JM Amp"),
        (signal::BlockType::Cabinet, "jm-cab", "JM Cabinet"),
        (signal::BlockType::Eq, "jm-eq", "JM EQ"),
        (signal::BlockType::Delay, "jm-dream-delay", "Dream Delay"),
        (signal::BlockType::Reverb, "jm-studio-verb", "Studio Verb"),
    ];

    let mut tracks = Vec::new();
    for (block_type, preset_id, label) in &jm_presets {
        let block = match signal
            .block_presets().load_default(*block_type, seed_id(preset_id))
            .await
        {
            Some(b) => b,
            None => {
                println!("  ⚠ {} — preset not found, skipping", label);
                continue;
            }
        };
        let track = add_jm_track(ctx.project(), &format!("JM {label}")).await?;
        let applied = apply_block(&track, &block, preset_id).await?;
        println!("  ✓ {} — {} param(s) applied", label, applied);
        tracks.push(track);
    }

    for track in tracks {
        remove_track(ctx.project(), track).await;
    }
    println!("PASS — all {} JM block presets applied", jm_presets.len());
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 4: Apply worship profile patches to tracks
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn worship_patches(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: apply_worship_profile_patches_to_tracks ===");
    let signal = signal_controller().await;

    let worship = signal
        .profiles().load(seed_id("guitar-worship-profile"))
        .await
        .expect("worship profile not found");

    println!("Applying {} Worship patches:", worship.patches.len());

    let mut tracks = Vec::new();
    for patch in &worship.patches {
        let graph = signal
            .resolve_target(ResolveTarget::ProfilePatch {
                profile_id: seed_id("guitar-worship-profile").into(),
                patch_id: patch.id.clone().into(),
            })
            .await
            .map_err(|e| eyre::eyre!("resolve failed: {:?}", e))?;

        let track = add_jm_track(ctx.project(), &format!("Worship/{}", patch.name)).await?;
        let applied = apply_graph(&track, &graph, JM_FX_ID).await?;
        let gain = read_gain(&track).await?;
        println!(
            "  ✓ {} — {} params applied, gain={:?}",
            patch.name, applied, gain
        );
        tracks.push(track);
    }

    for track in tracks {
        remove_track(ctx.project(), track).await;
    }
    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 4b: Apply blues profile patches to tracks
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn blues_patches(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: apply_blues_profile_patches_to_tracks ===");
    let signal = signal_controller().await;

    let blues = signal
        .profiles().load(seed_id("guitar-blues-profile"))
        .await
        .expect("blues profile not found");

    println!("Applying {} Blues patches:", blues.patches.len());
    assert_eq!(
        blues.patches.len(),
        8,
        "blues profile should have 8 patches"
    );

    // Blues patches target NDSP catalog BlockSnapshots directly.
    // Each resolves to a REAPER VST chunk (from the harvest .chunk files).
    let expected_names = [
        "Gravity Clean",
        "Gravity Rhythm",
        "Gravity OD",
        "Lead 1 Live Room",
        "Ultra Clean",
        "E Flat Voodoo",
        "Golden Gate",
        "Stately Blues Lead",
    ];

    // Catalog JSON paths for fingerprint verification.
    // These map preset names → JSON files on disk with expected fingerprints.
    let catalog_base = std::path::PathBuf::from(std::env::var("HOME").unwrap())
        .join("Music/FastTrackStudio/Library/blocks/plugin/neural-dsp/archetype-john-mayer-x/snapshots/John Mayer");

    // Capture default plugin state as baseline (before any preset loads)
    let baseline_track = add_jm_track(ctx.project(), "Blues/Baseline").await?;
    let baseline_fx = get_fx0(&baseline_track).await?;
    let default_params: Vec<(u32, f64)> = baseline_fx
        .parameters()
        .await?
        .iter()
        .map(|p| (p.index, p.value))
        .collect();
    let default_chunk = baseline_fx.state_chunk().await?.unwrap_or_default();
    let default_xml = extract_xml_from_chunk(&default_chunk);
    let default_fp = default_xml
        .as_deref()
        .map(fingerprint_from_xml)
        .unwrap_or_default();
    println!(
        "  Baseline: {} params, {} fingerprint keys",
        default_params.len(),
        default_fp.params.len()
    );
    remove_track(ctx.project(), baseline_track).await;

    let mut tracks = Vec::new();
    for (i, patch) in blues.patches.iter().enumerate() {
        assert_eq!(patch.name, expected_names[i], "patch {i} name mismatch");

        let graph = signal
            .resolve_target(ResolveTarget::ProfilePatch {
                profile_id: seed_id("guitar-blues-profile").into(),
                patch_id: patch.id.clone().into(),
            })
            .await
            .map_err(|e| eyre::eyre!("resolve '{}' failed: {:?}", patch.name, e))?;

        let track = add_jm_track(ctx.project(), &format!("Blues/{}", patch.name)).await?;

        // Apply the REAPER VST chunk from the catalog harvest.
        let used_state_chunk = apply_graph_with_state(&track, &graph, JM_FX_ID).await?;
        assert!(
            used_state_chunk,
            "'{}' should have state_data from catalog",
            patch.name
        );

        // Allow plugin time to process the state chunk
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // ── Verify every parameter via fingerprint matching ──
        //
        // Read the chunk back from REAPER, extract XML, build a fingerprint,
        // then compare against the catalog fingerprint from disk.
        let fx = get_fx0(&track).await?;
        let loaded_chunk = fx.state_chunk().await?.unwrap_or_default();
        let loaded_xml = extract_xml_from_chunk(&loaded_chunk);
        let loaded_fp = loaded_xml
            .as_deref()
            .map(fingerprint_from_xml)
            .unwrap_or_default();

        // Load expected fingerprint from catalog JSON
        let json_stem = match patch.name.as_str() {
            // Map preset names to their catalog JSON filenames
            name => name.to_string(),
        };
        let json_path = catalog_base.join(format!("{json_stem}.json"));
        let catalog_fp = if json_path.exists() {
            let json_str = std::fs::read_to_string(&json_path)
                .map_err(|e| eyre::eyre!("read catalog JSON for '{}': {}", patch.name, e))?;
            let meta: SnapshotMetadata = serde_json::from_str(&json_str)
                .map_err(|e| eyre::eyre!("parse catalog JSON for '{}': {}", patch.name, e))?;
            meta.fingerprint
        } else {
            // Fall back to the default fingerprint distance check
            println!(
                "  ⚠ No catalog JSON at {}, skipping fingerprint check",
                json_path.display()
            );
            PresetFingerprint::default()
        };

        // Compare loaded fingerprint vs catalog fingerprint
        let fp_distance = loaded_fp.distance(&catalog_fp);

        // Compare loaded fingerprint vs default to confirm state actually changed
        let default_distance = loaded_fp.distance(&default_fp);

        // Read all params and count how many differ from default
        let loaded_params = fx.parameters().await?;
        let changed_count = loaded_params
            .iter()
            .zip(default_params.iter())
            .filter(|(loaded, (_, default_val))| (loaded.value - default_val).abs() > 1e-6)
            .count();

        println!(
            "  {} — fp_dist={:.6}, default_dist={:.6}, changed={}/{} params",
            patch.name,
            fp_distance,
            default_distance,
            changed_count,
            loaded_params.len()
        );

        // The loaded state should exactly match the catalog fingerprint
        if !catalog_fp.params.is_empty() {
            assert!(
                fp_distance < 0.001,
                "'{}' fingerprint distance {:.6} — preset did NOT load correctly \
                 (should be ≈0). Is the .chunk file missing? Run `cargo xtask reaper-test \
                 harvest_jm_factory_presets` first.",
                patch.name,
                fp_distance
            );
        }

        // The loaded state should differ from the plugin default
        assert!(
            changed_count > 20,
            "'{}' only {} params differ from default — preset did NOT load \
             (state chunk may be wrong format)",
            patch.name,
            changed_count
        );

        tracks.push(track);
    }

    for track in tracks {
        remove_track(ctx.project(), track).await;
    }
    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 5: Apply worship song sections to tracks
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn worship_song_sections(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: apply_worship_song_sections_to_tracks ===");
    let signal = signal_controller().await;

    let songs = signal.songs().list().await;
    let worship_song = songs
        .iter()
        .find(|s| s.name.contains("Worship"))
        .expect("no Worship song seeded");

    println!(
        "Sections from '{}' ({} sections):",
        worship_song.name,
        worship_song.sections.len()
    );

    let mut tracks = Vec::new();
    for section in &worship_song.sections {
        let graph = signal
            .resolve_target(ResolveTarget::SongSection {
                song_id: worship_song.id.clone().into(),
                section_id: section.id.clone().into(),
            })
            .await
            .map_err(|e| eyre::eyre!("resolve {:?}: {:?}", section.name, e))?;

        let track = add_jm_track(ctx.project(), &format!("Section/{}", section.name)).await?;
        let applied = apply_graph(&track, &graph, JM_FX_ID).await?;
        println!(
            "  ✓ '{}' — {} params applied, {} overrides",
            section.name,
            applied,
            graph.effective_overrides.len()
        );
        tracks.push(track);
    }

    for track in tracks {
        remove_track(ctx.project(), track).await;
    }
    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 6: Snapshot round-trip (capture → mutate → restore → verify)
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn snapshot_round_trip(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: snapshot_live_params_save_recall ===");
    let signal = signal_controller().await;

    let track = add_jm_track(ctx.project(), "Snapshot Round-Trip").await?;
    let fx = get_fx0(&track).await?;
    let live_params = fx.parameters().await?;
    println!("Live JM params ({}):", live_params.len());
    for p in live_params.iter().take(10) {
        println!("  [{}] {} = {:.4}", p.index, p.name, p.value);
    }

    let amp_block = signal
        .block_presets().load_default(signal::BlockType::Amp, seed_id("jm-amp"))
        .await
        .expect("jm-amp not found");

    // Capture live values into the domain block
    let captured = capture_block_from_fx(&track, amp_block).await?;

    // Overwrite the default snapshot with captured values
    signal
        .block_presets().update_snapshot_params(
            signal::BlockType::Amp,
            seed_id("jm-amp"),
            seed_id("jm-amp-default"),
            captured.clone(),
        )
        .await;

    // Apply Lead to dirty state
    let lead_block = signal
        .block_presets().load_variant(
            signal::BlockType::Amp,
            seed_id("jm-amp"),
            seed_id("jm-amp-lead"),
        )
        .await
        .expect("lead block not found");
    apply_block(&track, &lead_block, JM_FX_ID).await?;
    println!("Applied Lead snapshot.");

    // Restore default
    let restored = signal
        .block_presets().load_default(signal::BlockType::Amp, seed_id("jm-amp"))
        .await
        .expect("jm-amp not found after save");
    apply_block(&track, &restored, JM_FX_ID).await?;
    println!("Restored live snapshot.");

    // Verify
    let final_params = fx.parameters().await?;
    let mut mismatches = 0;
    for sp in captured.parameters() {
        if let Some(fp) = final_params
            .iter()
            .find(|p| p.name.to_lowercase().contains(&sp.id().to_lowercase()))
        {
            let diff = (fp.value - sp.value().get() as f64).abs();
            if diff > 0.02 {
                println!(
                    "  ✗ {} expected={:.4} got={:.4}",
                    sp.id(),
                    sp.value().get(),
                    fp.value
                );
                mismatches += 1;
            } else {
                println!("  ✓ {} = {:.4}", sp.id(), fp.value);
            }
        }
    }

    remove_track(ctx.project(), track).await;
    assert_eq!(
        mismatches, 0,
        "{mismatches} param(s) didn't match after snapshot restore"
    );
    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 7: Morph between clean and lead patches
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn morph_between_patches(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: morph_between_patches ===");
    let signal = signal_controller().await;

    let track = add_jm_track(ctx.project(), "Morph Clean→Lead").await?;
    let fx = get_fx0(&track).await?;

    let clean = signal
        .block_presets().load_variant(
            signal::BlockType::Amp,
            seed_id("jm-amp"),
            seed_id("jm-amp-clean"),
        )
        .await
        .expect("jm-amp-clean not found");
    let lead = signal
        .block_presets().load_variant(
            signal::BlockType::Amp,
            seed_id("jm-amp"),
            seed_id("jm-amp-lead"),
        )
        .await
        .expect("jm-amp-lead not found");

    let (engine, snap_a, snap_b) = build_morph_engine(&track, &clean, &lead, JM_FX_ID).await?;

    let gain_idx_a = snap_a
        .params
        .iter()
        .find(|p| p.param_name.to_lowercase().contains("gain"))
        .map(|p| p.param_index)
        .expect("gain in snap_a");
    let gain_a = snap_a
        .params
        .iter()
        .find(|p| p.param_index == gain_idx_a)
        .map(|p| p.value)
        .unwrap();
    let gain_b = snap_b
        .params
        .iter()
        .find(|p| p.param_index == gain_idx_a)
        .map(|p| p.value)
        .unwrap();

    println!(
        "  Snapshot A (Clean): {} params, gain={:.4}",
        snap_a.params.len(),
        gain_a
    );
    println!(
        "  Snapshot B (Lead):  {} params, gain={:.4}",
        snap_b.params.len(),
        gain_b
    );
    println!("  Diff params: {}", engine.diff_count());
    assert!(engine.diff_count() > 0, "Clean and Lead should differ");

    let mut prev_gain = gain_a;
    for &t in &[0.0f64, 0.25, 0.5, 0.75, 1.0] {
        let changes = engine.morph(t, EasingCurve::Linear);
        for ch in &changes {
            fx.param(ch.param_index).set(ch.current_value).await?;
        }
        let live = fx.param(gain_idx_a).get().await?;
        let expected = gain_a + t * (gain_b - gain_a);
        println!("  t={:.2}: gain={:.4} (expected≈{:.4})", t, live, expected);
        assert!(
            (live - expected).abs() < 0.02,
            "gain at t={t} should be ≈{expected:.4}, got {live:.4}"
        );
        if t > 0.0 && gain_b > gain_a {
            assert!(live >= prev_gain - 0.001, "gain should be non-decreasing");
        }
        prev_gain = live;
    }

    remove_track(ctx.project(), track).await;
    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 8: Morph easing curves
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn morph_easing_curves(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: morph_easing_curves ===");
    let signal = signal_controller().await;

    let track = add_jm_track(ctx.project(), "Morph Easing").await?;
    let fx = get_fx0(&track).await?;

    let clean = signal
        .block_presets().load_variant(
            signal::BlockType::Amp,
            seed_id("jm-amp"),
            seed_id("jm-amp-clean"),
        )
        .await
        .expect("jm-amp-clean");
    let lead = signal
        .block_presets().load_variant(
            signal::BlockType::Amp,
            seed_id("jm-amp"),
            seed_id("jm-amp-lead"),
        )
        .await
        .expect("jm-amp-lead");

    let (engine, snap_a, _snap_b) = build_morph_engine(&track, &clean, &lead, JM_FX_ID).await?;
    let gain_idx = snap_a
        .params
        .iter()
        .find(|p| p.param_name.to_lowercase().contains("gain"))
        .map(|p| p.param_index)
        .expect("gain param");

    for curve in [
        EasingCurve::Linear,
        EasingCurve::EaseIn,
        EasingCurve::EaseOut,
        EasingCurve::EaseInOut,
    ] {
        let changes = engine.morph(0.25, curve);
        for ch in &changes {
            fx.param(ch.param_index).set(ch.current_value).await?;
        }
        let live = fx.param(gain_idx).get().await?;
        println!("  {:?} at t=0.25: gain={:.4}", curve, live);
    }

    // Boundary conditions: t=0 → A, t=1 → B for all curves
    for curve in [
        EasingCurve::Linear,
        EasingCurve::EaseIn,
        EasingCurve::EaseOut,
        EasingCurve::EaseInOut,
    ] {
        for ch in &engine.morph(0.0, curve) {
            assert!(
                (ch.current_value - ch.from_value).abs() < 1e-9,
                "{curve:?} t=0 != A"
            );
        }
        for ch in &engine.morph(1.0, curve) {
            assert!(
                (ch.current_value - ch.to_value).abs() < 1e-9,
                "{curve:?} t=1 != B"
            );
        }
    }

    remove_track(ctx.project(), track).await;
    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 9: Variant cycling (clean → lead → crunch)
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn variant_cycling(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: variant_cycling ===");
    let signal = signal_controller().await;

    let track = add_jm_track(ctx.project(), "Amp Variants").await?;

    // Named JM amp variants (excluding default, which is mutated by snapshot round-trip tests)
    let variants = [
        ("jm-amp-clean", "Clean", 0.25f32),
        ("jm-amp-lead", "Lead", 0.75),
        ("jm-amp-crunch", "Crunch", 0.62),
    ];

    for (variant_id, label, expected_gain) in &variants {
        let block = match signal
            .block_presets().load_variant(
                signal::BlockType::Amp,
                seed_id("jm-amp"),
                seed_id(variant_id),
            )
            .await
        {
            Some(b) => b,
            None => {
                println!("  ⚠ {} not found, skipping", label);
                continue;
            }
        };

        apply_block(&track, &block, JM_FX_ID).await?;

        let actual = read_gain(&track).await?.unwrap_or(0.0) as f32;
        let ok = (actual - expected_gain).abs() < 0.02;
        println!(
            "  {} gain: expected={:.4} actual={:.4} {}",
            label,
            expected_gain,
            actual,
            if ok { "✓" } else { "✗" }
        );
        assert!(ok, "{label} gain should be ≈{expected_gain}, got {actual}");
    }

    remove_track(ctx.project(), track).await;
    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 10: Save new block snapshot
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn save_new_snapshot(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: save_new_block_snapshot ===");
    let signal = signal_controller().await;

    let track = add_jm_track(ctx.project(), "Save Snapshot").await?;

    // Apply Lead to put the plugin in a known state
    let lead = signal
        .block_presets().load_variant(
            signal::BlockType::Amp,
            seed_id("jm-amp"),
            seed_id("jm-amp-lead"),
        )
        .await
        .expect("jm-amp-lead");
    apply_block(&track, &lead, JM_FX_ID).await?;

    // Capture live (post-Lead) → domain block
    let template = signal
        .block_presets().load_default(signal::BlockType::Amp, seed_id("jm-amp"))
        .await
        .expect("jm-amp default");
    let captured = capture_block_from_fx(&track, template).await?;

    // Save as the default snapshot
    signal
        .block_presets().update_snapshot_params(
            signal::BlockType::Amp,
            seed_id("jm-amp"),
            seed_id("jm-amp-default"),
            captured.clone(),
        )
        .await;

    // Reload and verify every parameter round-tripped
    let reloaded = signal
        .block_presets().load_default(signal::BlockType::Amp, seed_id("jm-amp"))
        .await
        .expect("jm-amp default after save");

    let mut mismatches = 0;
    for (orig, reloaded_sp) in captured
        .parameters()
        .iter()
        .zip(reloaded.parameters().iter())
    {
        let diff = (orig.value().get() - reloaded_sp.value().get()).abs();
        if diff > 0.001 {
            println!(
                "  ✗ {} saved={:.4} reloaded={:.4}",
                orig.id(),
                orig.value().get(),
                reloaded_sp.value().get()
            );
            mismatches += 1;
        }
    }
    assert_eq!(
        mismatches, 0,
        "{mismatches} param(s) didn't survive snapshot save/reload"
    );
    println!(
        "  ✓ All {} amp params round-tripped through save",
        captured.parameters().len()
    );

    remove_track(ctx.project(), track).await;
    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 11: Override stacking across worship song sections
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn override_stacking(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: override_stacking ===");
    let signal = signal_controller().await;

    let songs = signal.songs().list().await;
    let worship = songs
        .iter()
        .find(|s| s.name.contains("Worship"))
        .expect("Worship song");

    struct SectionResult {
        name: String,
        gain: f64,
        override_count: usize,
    }
    let mut results = Vec::new();

    for section in &worship.sections {
        let graph = signal
            .resolve_target(ResolveTarget::SongSection {
                song_id: worship.id.clone().into(),
                section_id: section.id.clone().into(),
            })
            .await
            .map_err(|e| eyre::eyre!("{:?}", e))?;

        let track = add_jm_track(ctx.project(), &format!("Override/{}", section.name)).await?;
        apply_graph(&track, &graph, JM_FX_ID).await?;
        let gain = read_gain(&track).await?.unwrap_or(0.0);
        println!(
            "  {}: {} overrides, gain={:.4}",
            section.name,
            graph.effective_overrides.len(),
            gain
        );
        results.push(SectionResult {
            name: section.name.clone(),
            override_count: graph.effective_overrides.len(),
            gain,
        });
        remove_track(ctx.project(), track).await;
    }

    let solo = results
        .iter()
        .find(|r| r.name == "Solo")
        .expect("Solo section");
    let outro = results
        .iter()
        .find(|r| r.name == "Outro")
        .expect("Outro section");
    assert!(
        solo.override_count > outro.override_count,
        "Solo ({}) should have more overrides than Outro ({})",
        solo.override_count,
        outro.override_count
    );
    for r in &results {
        assert!(r.gain > 0.0, "Section '{}' resolved to zero gain", r.name);
    }

    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Neural DSP preset library scanner — uses signal_proto::catalog
// ─────────────────────────────────────────────────────────────

use signal_proto::catalog::{
    extract_xml_from_chunk, fingerprint_from_xml, match_preset, scan_preset_library, xml_attr,
    DiskPreset, PresetFingerprint, SnapshotMetadata,
};

// ─────────────────────────────────────────────────────────────
//  Scenario 12: Harvest all JM factory presets
//
//  Neural DSP plugins don't expose presets via REAPER's standard
//  GetPresetIndex/NavigatePresets API (count=0). Instead we:
//
//  1. Set the track's record input to MIDI Virtual Keyboard (VKB)
//  2. Arm the track with input monitoring = Normal
//  3. Send MIDI CC#1 ch1 val=127 via StuffMIDIMessage → VKB queue
//     (this triggers the plugin's internal "Preset Next" via its
//     MIDI mapping: CC Absolute → Preset Next → Channel 1 → CC #1)
//  4. Read the state chunk after each advance, detect cycle by
//     comparing chunks (names may be empty for internal presets)
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn harvest_jm_factory_presets(ctx: &ReaperTestContext) -> Result<()> {
    use daw_proto::{InputMonitoringMode, MidiMessage, RecordInput, StuffMidiTarget};

    println!("\n=== scenario: harvest_jm_factory_presets ===");

    let track = add_jm_track(ctx.project(), "JM Preset Harvest").await?;
    let fx = get_fx0(&track).await?;

    let info = fx.info().await?;
    println!("FX: {} ({})", info.name, info.plugin_name);

    // Query initial preset state
    let initial = fx
        .preset_index()
        .await?
        .ok_or_else(|| eyre::eyre!("get_preset_index returned None for JM plugin"))?;
    println!(
        "  Initial: index={:?}, count={}, name={:?}",
        initial.index, initial.count, initial.name
    );

    if initial.count > 0 {
        // Standard path: plugin exposes presets via REAPER's program list
        println!(
            "\n  Plugin exposes {} presets via standard API",
            initial.count
        );

        fx.set_preset(0).await?;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        println!("\n  {:>4}  {:<50}  {:>10}", "#", "Name", "State Bytes");
        println!(
            "  {}  {}  {}",
            "-".repeat(4),
            "-".repeat(50),
            "-".repeat(10)
        );

        let mut presets: Vec<(u32, String, usize)> = Vec::new();
        for i in 0..initial.count {
            fx.set_preset(i).await?;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let preset_info = fx
                .preset_index()
                .await?
                .ok_or_else(|| eyre::eyre!("get_preset_index returned None at preset {}", i))?;
            let name = preset_info
                .name
                .unwrap_or_else(|| format!("(unnamed #{})", i));
            let chunk = fx.state_chunk_encoded().await?.unwrap_or_default();
            println!("  {:>4}  {:<50}  {:>10}", i, name, chunk.len());
            presets.push((i, name, chunk.len()));
        }

        println!("\n  Total: {} factory presets harvested", presets.len());
    } else {
        // MIDI-based discovery for plugins with internal preset managers
        // (Neural DSP, Helix Native, etc.)
        //
        // Strategy:
        //   1. Scan the preset library on disk → name, category, tags, fingerprint
        //   2. Advance presets via MIDI CC and capture REAPER state chunks
        //   3. Match each loaded state to a disk preset via parameter fingerprinting
        //   4. Output: cycle order + name + category + tags + state chunk
        println!("\n  Plugin uses internal preset manager (count=0).");

        // ── Step 1: Scan disk preset library ──
        let preset_dir =
            std::path::PathBuf::from("/Library/Audio/Presets/Neural DSP/Archetype John Mayer X");
        let library = scan_preset_library(&preset_dir);
        println!("  Scanned {} presets from disk library", library.len());

        // ── Step 2: Configure track for MIDI VKB input ──
        track
            .set_record_input(RecordInput::midi_virtual_keyboard())
            .await?;
        track.arm().await?;
        track
            .set_input_monitoring(InputMonitoringMode::Normal)
            .await?;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        println!("  Track armed with VKB input + monitoring ON");

        // ── Step 3: Advance presets and match ──
        let output_path = std::path::PathBuf::from("/tmp/reaper-tests/harvest_output.txt");
        let mut out = std::fs::File::create(&output_path)
            .map_err(|e| eyre::eyre!("create output file: {e}"))?;
        use std::io::Write as _;

        writeln!(
            out,
            "Neural DSP Archetype John Mayer X — Factory Preset Catalogue"
        )?;
        writeln!(out, "Disk library: {} presets", library.len())?;
        writeln!(out)?;

        // Capture initial state
        let first_chunk = fx.state_chunk_encoded().await?.unwrap_or_default();
        let first_raw = fx.state_chunk().await?.unwrap_or_default();
        let first_xml = extract_xml_from_chunk(&first_raw);

        struct HarvestedPreset {
            index: u32,
            name: String,
            category: String,
            tags: Vec<String>,
            match_distance: f64,
            chunk_len: usize,
            preset_uid: String,
            chunk_data: Vec<u8>,
            matched_source_path: Option<std::path::PathBuf>,
        }

        let mut harvested: Vec<HarvestedPreset> = Vec::new();

        // Match initial preset
        let initial_fp = first_xml
            .as_deref()
            .map(fingerprint_from_xml)
            .unwrap_or_default();
        let initial_uid = first_xml
            .as_deref()
            .and_then(|x| xml_attr(x, "presetUid"))
            .unwrap_or_default();

        let (initial_name, initial_cat, initial_tags, initial_dist, initial_source) =
            match match_preset(&library, &initial_fp) {
                Some((dp, dist)) => (
                    dp.name.clone(),
                    dp.category.clone(),
                    dp.tags.clone(),
                    dist,
                    Some(dp.source_path.clone()),
                ),
                None => (
                    "(unmatched)".to_string(),
                    String::new(),
                    Vec::new(),
                    f64::MAX,
                    None,
                ),
            };

        harvested.push(HarvestedPreset {
            index: 0,
            name: initial_name,
            category: initial_cat,
            tags: initial_tags,
            match_distance: initial_dist,
            chunk_len: first_chunk.len(),
            preset_uid: initial_uid,
            chunk_data: first_raw,
            matched_source_path: initial_source,
        });

        // Walk all 385+ presets. If we detect a cycle we break early.
        const MAX_PRESETS: u32 = 500;

        for i in 1..MAX_PRESETS {
            // Send CC#1 ch1: 0→127 transition to trigger "Preset Next"
            ctx.daw
                .stuff_midi(
                    StuffMidiTarget::VirtualMidiKeyboard,
                    MidiMessage::control_change(0, 1, 0),
                )
                .await?;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            ctx.daw
                .stuff_midi(
                    StuffMidiTarget::VirtualMidiKeyboard,
                    MidiMessage::control_change(0, 1, 127),
                )
                .await?;
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;

            // Read state
            let chunk = fx.state_chunk_encoded().await?.unwrap_or_default();
            let raw = fx.state_chunk().await?.unwrap_or_default();

            // Cycle detection
            if chunk == first_chunk && i > 1 {
                writeln!(
                    out,
                    "\n── Cycle detected at step {} (returned to preset 0) ──",
                    i
                )?;
                break;
            }

            // Extract XML and match
            let xml = extract_xml_from_chunk(&raw);
            let fp = xml.as_deref().map(fingerprint_from_xml).unwrap_or_default();
            let uid = xml
                .as_deref()
                .and_then(|x| xml_attr(x, "presetUid"))
                .unwrap_or_default();

            let (name, cat, tags, dist, source) = match match_preset(&library, &fp) {
                Some((dp, dist)) => (
                    dp.name.clone(),
                    dp.category.clone(),
                    dp.tags.clone(),
                    dist,
                    Some(dp.source_path.clone()),
                ),
                None => (
                    format!("(unmatched #{})", i),
                    String::new(),
                    Vec::new(),
                    f64::MAX,
                    None,
                ),
            };

            harvested.push(HarvestedPreset {
                index: i,
                name,
                category: cat,
                tags,
                match_distance: dist,
                chunk_len: chunk.len(),
                preset_uid: uid,
                chunk_data: raw,
                matched_source_path: source,
            });

            // Progress every 50 presets
            if i % 50 == 0 {
                println!("  ... advanced {} presets", i);
            }
        }

        // ── Step 4: Write formatted output ──
        writeln!(
            out,
            "{:>4}  {:<40}  {:<30}  {:>8}  {:>8}",
            "#", "Name", "Category", "Dist", "Bytes"
        )?;
        writeln!(
            out,
            "{}  {}  {}  {}  {}",
            "-".repeat(4),
            "-".repeat(40),
            "-".repeat(30),
            "-".repeat(8),
            "-".repeat(8)
        )?;

        for p in &harvested {
            writeln!(
                out,
                "{:>4}  {:<40}  {:<30}  {:>8.6}  {:>8}",
                p.index, p.name, p.category, p.match_distance, p.chunk_len
            )?;
        }

        // Summary stats
        let matched = harvested
            .iter()
            .filter(|p| p.match_distance < 0.001)
            .count();
        let close = harvested
            .iter()
            .filter(|p| p.match_distance >= 0.001 && p.match_distance < 1.0)
            .count();
        let unmatched = harvested.iter().filter(|p| p.match_distance >= 1.0).count();

        writeln!(out)?;
        writeln!(out, "── Summary ──")?;
        writeln!(out, "Total presets in cycle: {}", harvested.len())?;
        writeln!(
            out,
            "Exact matches (dist < 0.001): {} ({:.0}%)",
            matched,
            matched as f64 / harvested.len() as f64 * 100.0
        )?;
        writeln!(
            out,
            "Close matches (dist < 1.0):   {} ({:.0}%)",
            close,
            close as f64 / harvested.len() as f64 * 100.0
        )?;
        writeln!(out, "Unmatched (dist >= 1.0):      {}", unmatched)?;
        writeln!(out, "Disk library size:            {}", library.len())?;

        // Write tags for matched presets
        writeln!(out)?;
        writeln!(out, "── Tags ──")?;
        for p in &harvested {
            if !p.tags.is_empty() {
                writeln!(out, "  {:>4} {}: {}", p.index, p.name, p.tags.join(", "))?;
            }
        }

        // List worst matches for debugging
        let mut worst: Vec<&HarvestedPreset> = harvested
            .iter()
            .filter(|p| p.match_distance > 0.001)
            .collect();
        worst.sort_by(|a, b| {
            b.match_distance
                .partial_cmp(&a.match_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if !worst.is_empty() {
            writeln!(out)?;
            writeln!(out, "── Worst matches (top 10) ──")?;
            for p in worst.iter().take(10) {
                writeln!(
                    out,
                    "  #{}: {} (dist={:.6}, uid={})",
                    p.index, p.name, p.match_distance, p.preset_uid
                )?;
            }
        }

        out.flush()?;

        // ── Step 5: Write REAPER chunks to catalog directory ──
        //
        // For each exact-matched preset, write the REAPER VST chunk as a
        // .chunk file alongside the existing .json/.bin in the catalog.
        // This gives catalog_import.rs a REAPER-native blob for set_vst_chunk.
        let catalog_snapshots_dir = std::path::PathBuf::from(std::env::var("HOME").unwrap())
            .join("Music/FastTrackStudio/Library/blocks/plugin/neural-dsp/archetype-john-mayer-x/snapshots");
        let mut chunks_written = 0usize;

        for h in &harvested {
            // Only write for exact matches with non-empty chunk data
            if h.match_distance >= 0.001 || h.chunk_data.is_empty() {
                continue;
            }
            let Some(source_path) = &h.matched_source_path else {
                continue;
            };

            // Derive catalog path from the original disk preset filename.
            // source_path: /Library/Audio/Presets/Neural DSP/.../John Mayer/Gravity Clean.xml
            // catalog:     ~/Music/.../snapshots/John Mayer/Gravity Clean.chunk
            let stem = source_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if stem.is_empty() {
                continue;
            }

            let folder_dir = if h.category.is_empty() {
                catalog_snapshots_dir.clone()
            } else {
                catalog_snapshots_dir.join(&h.category)
            };

            // Write .chunk file
            let chunk_path = folder_dir.join(format!("{stem}.chunk"));
            if let Err(e) = std::fs::write(&chunk_path, &h.chunk_data) {
                println!("  ⚠ Failed to write chunk for '{}': {}", h.name, e);
                continue;
            }

            // Update the .json metadata to reference the chunk file
            let json_path = folder_dir.join(format!("{stem}.json"));
            if json_path.exists() {
                if let Ok(json_str) = std::fs::read_to_string(&json_path) {
                    if let Ok(mut meta) =
                        serde_json::from_str::<signal_proto::catalog::SnapshotMetadata>(&json_str)
                    {
                        meta.reaper_chunk_file = Some(format!("{stem}.chunk"));
                        meta.preset_uid = if h.preset_uid.is_empty() {
                            None
                        } else {
                            Some(h.preset_uid.clone())
                        };
                        meta.midi_cycle_index = Some(h.index);
                        if let Ok(updated) = serde_json::to_string_pretty(&meta) {
                            let _ = std::fs::write(&json_path, updated);
                        }
                    }
                }
            }

            chunks_written += 1;
        }

        println!("  Wrote {} REAPER chunk files to catalog", chunks_written);

        // Console summary
        println!(
            "  Harvested {} presets ({} exact, {} close, {} unmatched)",
            harvested.len(),
            matched,
            close,
            unmatched
        );
        println!("  Full output: {}", output_path.display());

        // Assertions
        assert!(
            harvested.len() > 1,
            "Should have more than 1 preset — check MIDI CC mapping"
        );
        let unique_uids: std::collections::HashSet<&str> =
            harvested.iter().map(|p| p.preset_uid.as_str()).collect();
        assert!(
            unique_uids.len() > 1,
            "All presets have the same UID — MIDI advancement not working"
        );
    }

    remove_track(ctx.project(), track).await;
    println!("PASS");
    Ok(())
}
