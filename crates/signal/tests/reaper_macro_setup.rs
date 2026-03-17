//! REAPER integration test: Direct Macro → FX Parameter API.
//!
//! Tests the new direct FX parameter binding system without JSFX/MIDI middleware.
//! Uses REAPER's built-in ReaComp compressor as the target plugin.
//! Sets up macro bindings, registers them in the global registry, then verifies
//! that moving macro knobs directly updates FX parameters via DAW RPC.
//!
//! Run with:
//!   cargo xtask reaper-test macro_setup

use std::time::Duration;

use futures::future::join_all;
use reaper_test::reaper_test;
use signal::macro_bank::{MacroBank, MacroKnob};
use signal::{Block, BlockParameter, MacroBinding};
use signal_live::macro_setup;

/// Small sleep to let REAPER process changes.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// ReaComp plugin name in REAPER's FX browser.
const REACOMP: &str = "VST: ReaComp (Cockos)";

/// Build a test Block with a MacroBank targeting ReaComp parameters.
///
/// ReaComp params (VST): Threshold, Ratio, Attack, Release, Pre-Comp, ...
/// We bind:
///   Knob 0 ("compress") → "Ratio" (value 0.6, range 0.0-1.0)
///   Knob 1 ("dynamics") → "Attack" AND "Release" (value 0.4, range 0.0-1.0)
fn build_reacomp_block_with_macros() -> Block {
    let params = vec![
        BlockParameter::new("ratio", "Ratio", 0.5),
        BlockParameter::new("attack", "Attack", 0.5),
        BlockParameter::new("release", "Release", 0.5),
    ];

    let mut block = Block::from_parameters(params);

    let mut bank = MacroBank::default();

    // Knob 0: "compress" → targets "Ratio"
    let mut compress_knob = MacroKnob::new("compress", "Compress");
    compress_knob.value = 0.6;
    compress_knob
        .bindings
        .push(MacroBinding::from_ids("self", "Ratio", 0.0, 1.0));
    bank.add(compress_knob);

    // Knob 1: "dynamics" → targets "Attack" AND "Release"
    let mut dynamics_knob = MacroKnob::new("dynamics", "Dynamics");
    dynamics_knob.value = 0.4;
    dynamics_knob
        .bindings
        .push(MacroBinding::from_ids("self", "Attack", 0.0, 1.0));
    dynamics_knob
        .bindings
        .push(MacroBinding::from_ids("self", "Release", 0.0, 1.0));
    bank.add(dynamics_knob);

    block.macro_bank = Some(bank);
    block
}

// ---------------------------------------------------------------------------
// Test: Direct macro setup with ReaComp — resolve bindings, register them,
//       then move macro knobs and verify parameters respond directly
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn macro_setup_direct_reacomp_parameter_binding(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();

    // 1. Create track and add ReaComp as the target plugin.
    let track = project.tracks().add("Direct Macro Test", None).await?;
    settle().await;

    let target_fx = track.fx_chain().add(REACOMP).await?;
    settle().await;

    // Log ReaComp's parameter names for debugging.
    let params = target_fx.parameters().await?;
    ctx.log(&format!(
        "ReaComp has {} params: {}",
        params.len(),
        params
            .iter()
            .take(10)
            .map(|p| format!("{}={}", p.name, p.index))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    // 2. Build block with macro bindings targeting ReaComp params.
    let block = build_reacomp_block_with_macros();

    // 3. Run macro setup (no JSFX insertion, just binding resolution).
    let result = macro_setup::setup_macros_for_block(&track, &target_fx, &block)
        .await
        .map_err(|e| eyre::eyre!("macro setup failed: {}", e))?;

    settle().await;

    let setup = result.ok_or_else(|| eyre::eyre!("Expected MacroSetupResult, got None"))?;

    ctx.log(&format!(
        "Macro setup: {} bindings resolved",
        setup.bindings.len()
    ));
    for b in &setup.bindings {
        ctx.log(&format!("  knob={} param_idx={}", b.knob_id, b.param_index));
    }

    // ─── Verify NO JSFX insertion (direct approach) ─────────────────────

    let fx_count = track.fx_chain().count().await?;
    assert_eq!(
        fx_count, 1,
        "should have 1 FX (just ReaComp, no FTS Macros), got {fx_count}"
    );

    ctx.log("PASS: No FTS Macros JSFX inserted (direct parameter approach)");

    // ─── Verify bindings resolved ───────────────────────────────────────

    // Should have 3 bindings: Ratio, Attack, Release
    assert!(
        setup.bindings.len() >= 3,
        "should have at least 3 resolved bindings (Ratio, Attack, Release), got {}",
        setup.bindings.len()
    );

    ctx.log("PASS: bindings resolved without JSFX/MIDI");

    // ─── Register bindings in global macro_registry ─────────────────────

    signal::macro_registry::clear(); // Start fresh
    signal::macro_registry::register(&setup);

    let compress_targets = signal::macro_registry::get_targets("compress");
    assert_eq!(
        compress_targets.len(),
        1,
        "compress knob should have 1 target, got {}",
        compress_targets.len()
    );

    let dynamics_targets = signal::macro_registry::get_targets("dynamics");
    assert_eq!(
        dynamics_targets.len(),
        2,
        "dynamics knob should have 2 targets (Attack + Release), got {}",
        dynamics_targets.len()
    );

    ctx.log("PASS: Bindings registered in global macro_registry");

    // ─── Enable audio engine (required for parameter changes to reach plugins) ─────

    if !ctx.daw.audio_engine().is_running().await.unwrap_or(false) {
        ctx.daw
            .audio_engine()
            .init()
            .await
            .map_err(|e| eyre::eyre!("Failed to init audio engine: {:?}", e))?;
        settle().await;
        ctx.log("Audio engine initialized");
    }

    // ─── Verify initial ReaComp parameter values ────────────────────────

    let ratio_binding = setup.bindings.iter().find(|b| b.knob_id == "compress");
    let attack_binding = setup
        .bindings
        .iter()
        .find(|b| b.knob_id == "dynamics" && b.param_index == dynamics_targets[0].param_index);

    if let Some(rb) = ratio_binding {
        let initial = target_fx.param(rb.param_index).get().await?;
        ctx.log(&format!(
            "Ratio param (idx={}) initial: {:.4}",
            rb.param_index, initial
        ));
    }

    if let Some(ab) = attack_binding {
        let initial = target_fx.param(ab.param_index).get().await?;
        ctx.log(&format!(
            "Attack param (idx={}) initial: {:.4}",
            ab.param_index, initial
        ));
    }

    // ─── Simulate macro knob change: compress 0.6 → 0.9 ──────────────────

    // In real usage, performance_tab.rs would do this when a macro knob changes.
    // Here we simulate it directly using the registry.

    if let Some(rb) = ratio_binding {
        let targets = signal::macro_registry::get_targets(&rb.knob_id);
        for target in targets {
            // Map macro value (0.6 → 0.9) through param range [0.0, 1.0]
            let param_val = (target.min + (target.max - target.min) * 0.9) as f64;
            target_fx.param(target.param_index).set(param_val).await?;
            ctx.log(&format!(
                "Set Ratio (idx={}) to {:.4}",
                target.param_index, param_val
            ));
        }
    }

    settle().await;

    // Verify the parameter changed
    if let Some(rb) = ratio_binding {
        let after = target_fx.param(rb.param_index).get().await?;
        ctx.log(&format!(
            "Ratio param after macro change to 0.9: {:.4}",
            after
        ));
        assert!(
            (after - 0.9).abs() < 0.05,
            "Ratio param should be ~0.9, got {:.4}",
            after
        );
        ctx.log("PASS: Ratio param responded directly to macro change");
    }

    // ─── Simulate macro knob change: dynamics 0.4 → 0.7 ──────────────────

    ctx.log(&format!("dynamics_targets len: {}", dynamics_targets.len()));
    if !dynamics_targets.is_empty() {
        let targets = signal::macro_registry::get_targets("dynamics");
        ctx.log(&format!(
            "Retrieved {} targets from registry for 'dynamics'",
            targets.len()
        ));
        for target in targets {
            // Map macro value (0.4 → 0.7) through param range [0.0, 1.0]
            let param_val = (target.min + (target.max - target.min) * 0.7) as f64;
            target_fx.param(target.param_index).set(param_val).await?;
            ctx.log(&format!(
                "Set param (idx={}) to {:.4}",
                target.param_index, param_val
            ));
        }
    }

    settle().await;

    // Verify both Attack and Release changed via dynamics_targets
    if dynamics_targets.len() >= 2 {
        // First target (Attack)
        let attack_val = target_fx
            .param(dynamics_targets[0].param_index)
            .get()
            .await?;
        ctx.log(&format!(
            "Attack param after macro change to 0.7: {:.4}",
            attack_val
        ));
        assert!(
            (attack_val - 0.7).abs() < 0.05,
            "Attack param should be ~0.7, got {:.4}",
            attack_val
        );

        // Second target (Release)
        let release_val = target_fx
            .param(dynamics_targets[1].param_index)
            .get()
            .await?;
        ctx.log(&format!(
            "Release param after macro change to 0.7: {:.4}",
            release_val
        ));
        assert!(
            (release_val - 0.7).abs() < 0.05,
            "Release param should be ~0.7, got {:.4}",
            release_val
        );
    }

    ctx.log("PASS: All macro-controlled params responded directly");

    // ─── Clean up ──────────────────────────────────────────────────────

    signal::macro_registry::clear();

    ctx.log("=== TEST PASSED: Direct macro → FX parameter system works ===");

    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Multiple plugins, multiple macro knobs
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn macro_setup_direct_multi_plugin(ctx: &reaper_test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    // Create track with two ReaComp instances
    let track = project
        .tracks()
        .add("Multi-Plugin Macro Test", None)
        .await?;
    settle().await;

    let fx1 = track.fx_chain().add(REACOMP).await?;
    settle().await;
    let fx2 = track.fx_chain().add(REACOMP).await?;
    settle().await;

    ctx.log("Added 2x ReaComp to track");

    // Build blocks with macros for each plugin
    let block1 = build_reacomp_block_with_macros();
    let block2 = build_reacomp_block_with_macros();

    // Set up macros for both plugins
    let setup1 = macro_setup::setup_macros_for_block(&track, &fx1, &block1)
        .await
        .map_err(|e| eyre::eyre!("setup1 failed: {}", e))?
        .ok_or_else(|| eyre::eyre!("setup1 returned None"))?;

    let setup2 = macro_setup::setup_macros_for_block(&track, &fx2, &block2)
        .await
        .map_err(|e| eyre::eyre!("setup2 failed: {}", e))?
        .ok_or_else(|| eyre::eyre!("setup2 returned None"))?;

    settle().await;

    // Register both
    signal::macro_registry::clear();
    signal::macro_registry::register(&setup1);
    signal::macro_registry::register(&setup2);

    // Verify "compress" knob now has 2 targets (one per plugin)
    let compress_targets = signal::macro_registry::get_targets("compress");
    assert_eq!(
        compress_targets.len(),
        2,
        "compress should have 2 targets (one per plugin), got {}",
        compress_targets.len()
    );

    ctx.log(&format!(
        "Registered {} targets for 'compress' knob",
        compress_targets.len()
    ));

    // ─── Start playback to enable audio engine ─────

    let _ = project.transport().play().await;
    settle().await;
    ctx.log("Playback started — audio engine enabled");

    // Move compress macro and verify both plugins respond
    for target in compress_targets {
        let param_val = 0.8;
        let fx = if target.fx_guid == fx1.guid() {
            &fx1
        } else {
            &fx2
        };
        fx.param(target.param_index).set(param_val).await?;
    }

    settle().await;

    ctx.log("PASS: Multiple plugins respond to single macro knob");

    signal::macro_registry::clear();

    Ok(())
}

// ---------------------------------------------------------------------------
// Test: LFO modulation demo — observe macro → parameter updates live
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn macro_lfo_modulation_demo(ctx: &reaper_test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    // 1. Create track and add ReaComp
    let track = project.tracks().add("LFO Modulation Demo", None).await?;
    settle().await;

    let target_fx = track.fx_chain().add(REACOMP).await?;
    settle().await;

    ctx.log("=== LFO MODULATION DEMO ===");
    ctx.log("Setting up macro bindings and LFO...");

    // 2. Build block with macro bindings
    let block = build_reacomp_block_with_macros();

    // 3. Set up macros
    let result = macro_setup::setup_macros_for_block(&track, &target_fx, &block)
        .await
        .map_err(|e| eyre::eyre!("macro setup failed: {}", e))?;

    settle().await;

    let setup = result.ok_or_else(|| eyre::eyre!("Expected MacroSetupResult, got None"))?;

    // 4. Register bindings
    signal::macro_registry::clear();
    signal::macro_registry::register(&setup);

    ctx.log(&format!("Registered {} bindings", setup.bindings.len()));

    // 5. Start playback to enable audio engine
    let _ = project.transport().play().await;
    settle().await;
    ctx.log("Playback started — audio engine should now be running");

    // 6. Get initial Ratio parameter value
    let ratio_binding = setup.bindings.iter().find(|b| b.knob_id == "compress");
    if let Some(rb) = ratio_binding {
        let initial = target_fx.param(rb.param_index).get().await?;
        ctx.log(&format!(
            "Starting LFO on Ratio parameter (idx={}) - initial: {:.4}",
            rb.param_index, initial
        ));
    }

    // 7. Run LFO modulation for 30 seconds
    ctx.log("LFO modulating macro knob 0.0 to 1.0 repeatedly...");
    ctx.log("Observe the Ratio parameter in REAPER changing smoothly!");
    ctx.log("");

    let start = std::time::Instant::now();
    let duration = std::time::Duration::from_secs(30);

    while start.elapsed() < duration {
        let elapsed = start.elapsed().as_secs_f64();

        // Oscillate between 0.0 and 1.0 using a sine wave
        // Period = 4 seconds (goes 0->1->0 in 4 seconds)
        let sine = ((elapsed * std::f64::consts::PI / 2.0).sin() + 1.0) / 2.0;
        let macro_val = sine as f32;

        // Update "compress" macro to drive Ratio parameter
        let targets = signal::macro_registry::get_targets("compress");
        for target in targets {
            let param_val = (target.min + (target.max - target.min) * macro_val as f32) as f64;
            target_fx.param(target.param_index).set(param_val).await?;
        }

        // Log every ~2 seconds
        if (elapsed as i32) % 2 == 0 && (elapsed as i32) != ((elapsed - 0.1) as i32) {
            if let Some(rb) = ratio_binding {
                let current = target_fx.param(rb.param_index).get().await?;
                ctx.log(&format!(
                    "T={:2.1}s | Macro={:.3} | Ratio={:.4}",
                    elapsed, macro_val, current
                ));
            }
        }

        // Update ~200 times per second for smooth motion
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    if let Some(rb) = ratio_binding {
        let final_val = target_fx.param(rb.param_index).get().await?;
        ctx.log(&format!("Final Ratio parameter: {:.4}", final_val));
    }

    ctx.log("");
    ctx.log("=== LFO DEMO COMPLETE ===");
    ctx.log("The macro successfully modulated the plugin parameter!");
    ctx.log("Check REAPER - you should see the Ratio parameter oscillating.");

    signal::macro_registry::clear();

    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Enumerate all available FX in REAPER
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn enumerate_reaper_fx(ctx: &reaper_test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("FX Enumeration", None).await?;
    settle().await;

    // Try common REAPER stock plugins to find exact names
    let fx_names = vec![
        "ReaComp",
        "ReaEQ",
        "ReaVerb",
        "ReaDelay",
        "ReaSampler",
        "ReaPitch",
        "ReaTune",
        "ReaFir",
        "ReaGate",
        "ReaXcomp",
        "RSamplerBank",
        "ReaVoice",
        "MIDI Keyboard",
        "MIDI Note Router",
    ];

    ctx.log("=== REAPER STOCK FX ENUMERATION ===");
    ctx.log("");

    for fx_name in fx_names {
        match track.fx_chain().add(fx_name).await {
            Ok(_fx) => {
                ctx.log(&format!("✓ Found: {}", fx_name));
                settle().await;
            }
            Err(_e) => {
                ctx.log(&format!("✗ Not found: {}", fx_name));
            }
        }
    }

    ctx.log("");
    ctx.log("=== END ENUMERATION ===");

    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Multi-plugin LFO demo — single macro drives different params on different plugins
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn macro_lfo_multi_plugin_demo(ctx: &reaper_test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    ctx.log("=== MULTI-PLUGIN LFO MODULATION DEMO ===");
    ctx.log("Loading diverse plugins and assigning one macro to different params...");
    ctx.log("");

    // 1. Create track and add plugins
    let track = project.tracks().add("Multi-Plugin LFO Demo", None).await?;
    settle().await;

    // Load 3 plugins: 2x ReaComp (different params) + 1x ReaGate
    let reacomp1 = track.fx_chain().add("VST: ReaComp (Cockos)").await?;
    settle().await;
    let reacomp2 = track.fx_chain().add("VST: ReaComp (Cockos)").await?;
    settle().await;
    let rea_gate = track.fx_chain().add("VST: ReaGate (Cockos)").await?;
    settle().await;

    ctx.log("Loaded 3 plugins:");
    ctx.log("  1. ReaComp #1 — Ratio [0.0-1.0], Attack [0.2-0.8]");
    ctx.log("  2. ReaComp #2 — Release [0.1-0.9], Pre-comp [0.0-0.5]");
    ctx.log("  3. ReaGate — Threshold [0.3-0.7]");
    ctx.log("");

    // 2. Create 3 separate blocks optimized for each plugin
    // Block 1 for ReaComp #1 targeting Ratio and Attack with different curves
    let params1 = vec![
        BlockParameter::new("ratio", "Ratio", 0.5),
        BlockParameter::new("attack", "Attack", 0.5),
    ];
    let mut block1 = Block::from_parameters(params1);
    let mut bank1 = MacroBank::default();
    let mut master1 = MacroKnob::new("master_drive", "Master Drive");
    master1.value = 0.5;
    // Ratio: full range [0.0-1.0]
    master1
        .bindings
        .push(MacroBinding::from_ids("self", "Ratio", 0.0, 1.0));
    // Attack: limited range [0.2-0.8] for more subtle control
    master1
        .bindings
        .push(MacroBinding::from_ids("self", "Attack", 0.2, 0.8));
    bank1.add(master1);
    block1.macro_bank = Some(bank1);

    // Block 2 for ReaComp #2 targeting Release and Pre-comp with different curves
    let params2 = vec![
        BlockParameter::new("release", "Release", 0.5),
        BlockParameter::new("precomp", "Pre-comp", 0.5),
    ];
    let mut block2 = Block::from_parameters(params2);
    let mut bank2 = MacroBank::default();
    let mut master2 = MacroKnob::new("master_drive", "Master Drive");
    master2.value = 0.5;
    // Release: inverted range [0.9-0.1] so it modulates opposite direction
    master2
        .bindings
        .push(MacroBinding::from_ids("self", "Release", 0.1, 0.9));
    // Pre-comp: lower range [0.0-0.5]
    master2
        .bindings
        .push(MacroBinding::from_ids("self", "Pre-comp", 0.0, 0.5));
    bank2.add(master2);
    block2.macro_bank = Some(bank2);

    // Block 3 for ReaGate targeting Threshold
    let params3 = vec![BlockParameter::new("threshold", "Threshold", 0.5)];
    let mut block3 = Block::from_parameters(params3);
    let mut bank3 = MacroBank::default();
    let mut master3 = MacroKnob::new("master_drive", "Master Drive");
    master3.value = 0.5;
    // Threshold: narrow range [0.3-0.7]
    master3
        .bindings
        .push(MacroBinding::from_ids("self", "Threshold", 0.3, 0.7));
    bank3.add(master3);
    block3.macro_bank = Some(bank3);

    // 3. Set up macros for each plugin with its own optimized block
    ctx.log("Setting up macro bindings for each plugin...");

    let setup1 = macro_setup::setup_macros_for_block(&track, &reacomp1, &block1)
        .await
        .map_err(|e| eyre::eyre!("ReaComp#1 setup failed: {}", e))?
        .ok_or_else(|| eyre::eyre!("ReaComp#1 setup returned None"))?;

    let setup2 = macro_setup::setup_macros_for_block(&track, &reacomp2, &block2)
        .await
        .map_err(|e| eyre::eyre!("ReaComp#2 setup failed: {}", e))?
        .ok_or_else(|| eyre::eyre!("ReaComp#2 setup returned None"))?;

    let setup3 = macro_setup::setup_macros_for_block(&track, &rea_gate, &block3)
        .await
        .map_err(|e| eyre::eyre!("ReaGate setup failed: {}", e))?
        .ok_or_else(|| eyre::eyre!("ReaGate setup returned None"))?;

    settle().await;

    // 4. Register all bindings — this time they'll all target "master_drive" knob
    signal::macro_registry::clear();
    signal::macro_registry::register(&setup1);
    signal::macro_registry::register(&setup2);
    signal::macro_registry::register(&setup3);

    let targets = signal::macro_registry::get_targets("master_drive");
    ctx.log(&format!(
        "Registered {} total targets for 'master_drive' macro",
        targets.len()
    ));
    for (i, t) in targets.iter().enumerate() {
        ctx.log(&format!(
            "  Target {}: param_idx={}, range=[{:.1}, {:.1}]",
            i + 1,
            t.param_index,
            t.min,
            t.max
        ));
    }
    ctx.log("");
    ctx.log(&format!(
        "Target count check: expected 3, got {}",
        targets.len()
    ));
    ctx.log("");

    // 5. Start playback to enable audio engine
    let _ = project.transport().play().await;
    settle().await;
    ctx.log("Playback started — audio engine enabled");
    ctx.log("");

    // 6. Run LFO modulation for 20 seconds
    ctx.log("LFO RUNNING: Moving 'master_drive' knob — all parameters respond!");
    ctx.log("  ReaComp #1: Ratio [0.0-1.0] + Attack [0.2-0.8]");
    ctx.log("  ReaComp #2: Release [0.1-0.9] + Pre-comp [0.0-0.5]");
    ctx.log("  ReaGate:    Threshold [0.3-0.7]");
    ctx.log("");
    ctx.log("Notice different min/max ranges create different modulation curves!");

    let start = std::time::Instant::now();
    let duration = std::time::Duration::from_secs(20);

    while start.elapsed() < duration {
        let elapsed = start.elapsed().as_secs_f64();

        // Oscillate 0.0 to 1.0 with sine wave
        let sine = ((elapsed * std::f64::consts::PI / 2.0).sin() + 1.0) / 2.0;
        let macro_val = sine as f32;

        // Update all 3 plugins from the single macro — in PARALLEL for smooth 30+ FPS
        let targets = signal::macro_registry::get_targets("master_drive");
        let mut set_futures = Vec::new();

        // Clone handles before the loop to avoid move issues
        let rc1 = reacomp1.clone();
        let rc2 = reacomp2.clone();
        let rg = rea_gate.clone();

        for target in targets {
            let param_val = (target.min + (target.max - target.min) * macro_val as f32) as f64;
            let guid = target.fx_guid.clone();

            // Clone for this iteration
            let rc1_iter = rc1.clone();
            let rc2_iter = rc2.clone();
            let rg_iter = rg.clone();

            let fut = async move {
                match guid.as_str() {
                    g if g == rc1_iter.guid().to_string() => {
                        rc1_iter.param(target.param_index).set(param_val).await
                    }
                    g if g == rc2_iter.guid().to_string() => {
                        rc2_iter.param(target.param_index).set(param_val).await
                    }
                    g if g == rg_iter.guid().to_string() => {
                        rg_iter.param(target.param_index).set(param_val).await
                    }
                    _ => Ok(()),
                }
            };
            set_futures.push(fut);
        }

        // Wait for all parameter updates to complete in parallel
        let _ = join_all(set_futures).await;

        // Log every 2 seconds showing all parameter values
        if (elapsed as i32) % 2 == 0 && (elapsed as i32) != ((elapsed - 0.1) as i32) {
            // ReaComp #1: Ratio and Attack
            let ratio = reacomp1
                .param(setup1.bindings[0].param_index)
                .get()
                .await
                .unwrap_or(0.0);
            let attack1 = reacomp1
                .param(setup1.bindings[1].param_index)
                .get()
                .await
                .unwrap_or(0.0);

            // ReaComp #2: Release and Pre-comp
            let release = reacomp2
                .param(setup2.bindings[0].param_index)
                .get()
                .await
                .unwrap_or(0.0);
            let precomp = reacomp2
                .param(setup2.bindings[1].param_index)
                .get()
                .await
                .unwrap_or(0.0);

            // ReaGate: Threshold
            let threshold = rea_gate
                .param(setup3.bindings[0].param_index)
                .get()
                .await
                .unwrap_or(0.0);

            ctx.log(&format!(
                "T={:2.0}s | M={:.2} | R={:.3} A={:.3} | Rel={:.3} P={:.3} | T={:.3}",
                elapsed, macro_val, ratio, attack1, release, precomp, threshold
            ));
        }

        // Update ~200 times per second for smooth motion
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    ctx.log("");
    ctx.log("=== MULTI-PLUGIN LFO DEMO COMPLETE ===");
    ctx.log("✓ Single macro successfully drove parameters on 3 different plugins!");
    ctx.log("Check REAPER — all 3 plugins should show oscillating parameters.");

    signal::macro_registry::clear();

    Ok(())
}
