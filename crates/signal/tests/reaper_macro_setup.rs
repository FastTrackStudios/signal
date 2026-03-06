//! REAPER integration test: MacroBank → FTS Macros JSFX bridge with ReaComp.
//!
//! Uses REAPER's built-in ReaComp compressor (guaranteed available) as the target
//! plugin. Sets up macro bindings to ReaComp parameters, verifies plink config,
//! then moves macro sliders and asserts that ReaComp parameters respond.
//!
//! Run with:
//!   cargo xtask reaper-test macro_setup

use std::time::Duration;

use base64::Engine as _;
use reaper_test::reaper_test;
use signal::macro_bank::{MacroBank, MacroKnob};
use signal::{Block, BlockParameter, MacroBinding};
use signal_live::macro_setup;

/// Trim null bytes from a REAPER config string.
/// REAPER's `TrackFX_GetNamedConfigParm` returns null-padded fixed buffers.
fn trim_nulls(s: &str) -> &str {
    s.split('\0').next().unwrap_or(s)
}

/// ReaComp plugin name in REAPER's FX browser.
const REACOMP: &str = "VST: ReaComp (Cockos)";

/// Small sleep to let REAPER process changes.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// Longer settle for plink/MIDI CC processing (requires audio engine cycle).
async fn settle_long() {
    tokio::time::sleep(Duration::from_millis(2000)).await;
}

/// Ensure REAPER's audio engine is running (required for plink CC processing).
async fn ensure_audio(ctx: &reaper_test::ReaperTestContext) {
    if !ctx.daw.audio_engine().is_running().await.unwrap_or(false) {
        let _ = ctx.daw.audio_engine().init().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Build a test Block with a MacroBank targeting ReaComp parameters.
///
/// ReaComp params (VST): Thresh, Ratio, Attack, Release, Pre-Comp, ...
/// We bind:
///   Knob 0 ("compress") → "Ratio" (value 0.6)
///   Knob 1 ("dynamics") → "Attack" AND "Release" (value 0.4)
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
// Test: Full macro setup with ReaComp — insert JSFX, resolve bindings,
//       write plink, set sliders, then move sliders and verify param response
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn macro_setup_reacomp_full_roundtrip(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    // 1. Create track and add ReaComp as the target plugin.
    let track = project.tracks().add("ReaComp Macro Test", None).await?;
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

    // 3. Run macro setup.
    let result = macro_setup::setup_macros_for_block(&track, &target_fx, &block)
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    settle().await;

    let setup = result.ok_or_else(|| eyre::eyre!("Expected MacroSetupResult, got None"))?;

    ctx.log(&format!(
        "Macro setup: {} bindings resolved",
        setup.bindings.len()
    ));
    for b in &setup.bindings {
        ctx.log(&format!(
            "  knob={} param_idx={} cc={}",
            b.knob_id, b.target_param_index, b.cc_number
        ));
    }

    // ─── Verify JSFX insertion ─────────────────────────────────────

    let fx_count = track.fx_chain().count().await?;
    assert_eq!(
        fx_count, 2,
        "should have 2 FX (FTS Macros + ReaComp), got {fx_count}"
    );

    let macros_fx = track
        .fx_chain()
        .by_index(0)
        .await?
        .ok_or_else(|| eyre::eyre!("No FX at index 0"))?;

    let macros_info = macros_fx.info().await?;
    assert!(
        macros_info.name.contains("FTS Macros"),
        "FX at index 0 should be FTS Macros, got '{}'",
        macros_info.name
    );

    ctx.log("PASS: FTS Macros JSFX inserted at position 0");

    // ─── Verify bindings resolved ──────────────────────────────────

    // Should have 3 bindings: Ratio, Attack, Release
    assert!(
        setup.bindings.len() >= 3,
        "should have at least 3 resolved bindings (Ratio, Attack, Release), got {}",
        setup.bindings.len()
    );

    // Verify CC numbers are sequential from 1.
    for (i, binding) in setup.bindings.iter().enumerate() {
        assert_eq!(
            binding.cc_number,
            (i + 1) as u32,
            "binding {i} should have CC {}, got {}",
            i + 1,
            binding.cc_number
        );
    }

    ctx.log("PASS: bindings resolved with sequential CC numbers");

    // ─── Verify plink config ───────────────────────────────────────

    // Re-acquire target FX by GUID (may have shifted after JSFX insert at 0).
    let target_fx_readback = track
        .fx_chain()
        .by_guid(target_fx.guid())
        .await?
        .ok_or_else(|| eyre::eyre!("Target FX not found by GUID after JSFX insert"))?;

    for binding in &setup.bindings {
        let prefix = format!("param.{}.plink", binding.target_param_index);

        let active = target_fx_readback
            .get_config(&format!("{prefix}.active"))
            .await?;
        assert_eq!(
            active.as_deref().map(trim_nulls),
            Some("1"),
            "plink.active should be '1' for param {}, got {:?}",
            binding.target_param_index,
            active.as_deref().map(trim_nulls)
        );

        let midi_bus = target_fx_readback
            .get_config(&format!("{prefix}.midi_bus"))
            .await?;
        assert_eq!(
            midi_bus.as_deref().map(trim_nulls),
            Some("15"),
            "plink.midi_bus should be '15' for param {}, got {:?}",
            binding.target_param_index,
            midi_bus.as_deref().map(trim_nulls)
        );

        let midi_chan = target_fx_readback
            .get_config(&format!("{prefix}.midi_chan"))
            .await?;
        assert_eq!(
            midi_chan.as_deref().map(trim_nulls),
            Some("16"),
            "plink.midi_chan should be '16' for param {}, got {:?}",
            binding.target_param_index,
            midi_chan.as_deref().map(trim_nulls)
        );

        let midi_msg = target_fx_readback
            .get_config(&format!("{prefix}.midi_msg"))
            .await?;
        assert_eq!(
            midi_msg.as_deref().map(trim_nulls),
            Some("176"),
            "plink.midi_msg should be '176' (CC) for param {}, got {:?}",
            binding.target_param_index,
            midi_msg.as_deref().map(trim_nulls)
        );

        let midi_msg2 = target_fx_readback
            .get_config(&format!("{prefix}.midi_msg2"))
            .await?;
        assert_eq!(
            midi_msg2.as_deref().map(trim_nulls),
            Some(binding.cc_number.to_string().as_str()),
            "plink.midi_msg2 should be '{}' for param {}, got {:?}",
            binding.cc_number,
            binding.target_param_index,
            midi_msg2.as_deref().map(trim_nulls)
        );
    }

    ctx.log("PASS: plink config written correctly on all bound params");

    // ─── Diagnostic: decode <JS_SER> to verify P.Inst was written ──
    {
        let track_chunk = track.get_chunk().await?;
        let fx_guid = setup.macros_fx_guid.trim_matches(|c| c == '{' || c == '}');
        let guid_pattern = format!("FXID {{{fx_guid}}}");
        if let Some(guid_pos) = track_chunk.find(&guid_pattern) {
            let before = &track_chunk[..guid_pos];
            if let Some(ser_start) = before.rfind("<JS_SER") {
                let after_ser = &track_chunk[ser_start..];
                if let Some(close) = after_ser.find("\n>") {
                    let ser_block = &after_ser[..close];
                    // Extract base64 content (skip "<JS_SER\n" header)
                    if let Some(b64_start) = ser_block.find('\n') {
                        let b64_content: String = ser_block[b64_start + 1..]
                            .lines()
                            .collect::<Vec<_>>()
                            .join("");
                        match base64::engine::general_purpose::STANDARD.decode(&b64_content) {
                            Ok(bytes) => {
                                let count = bytes.len() / 8;
                                eprintln!("=== JS_SER decoded: {} f64 values ===", count);
                                for i in 0..count.min(20) {
                                    let offset = i * 8;
                                    let val = f64::from_le_bytes(
                                        bytes[offset..offset + 8].try_into().unwrap(),
                                    );
                                    eprintln!("  [{i}] = {val}");
                                }
                                if count > 20 {
                                    eprintln!("  ... ({} more)", count - 20);
                                }
                            }
                            Err(e) => eprintln!("=== JS_SER base64 decode error: {e} ==="),
                        }
                    }
                }
            } else {
                eprintln!("=== No <JS_SER> block found for JSFX ===");
            }
        } else {
            eprintln!("=== FXID not found in track chunk ===");
        }
    }

    // ─── Verify initial macro slider values ────────────────────────

    let slider0 = macros_fx.param(0).get().await?;
    let slider1 = macros_fx.param(1).get().await?;

    assert!(
        (slider0 - 0.6).abs() < 0.01,
        "macro slider 0 (compress) should be ~0.6, got {slider0}"
    );
    assert!(
        (slider1 - 0.4).abs() < 0.01,
        "macro slider 1 (dynamics) should be ~0.4, got {slider1}"
    );

    ctx.log("PASS: macro slider values set correctly (compress=0.6, dynamics=0.4)");

    // ─── Diagnostic: dump track chunk to check for PLINK entries ────
    {
        let chunk = track.get_chunk().await?;
        let plink_lines: Vec<&str> = chunk
            .lines()
            .filter(|l| {
                let t = l.trim();
                t.starts_with("PLINK") || t.contains("plink")
            })
            .collect();
        if plink_lines.is_empty() {
            ctx.log("DIAGNOSTIC: No PLINK lines found in track chunk");
            // Dump ReaComp FX block section for inspection
            if let Some(reacomp_pos) = chunk.find("ReaComp") {
                let start = chunk[..reacomp_pos].rfind('<').unwrap_or(0);
                let end = (reacomp_pos + 500).min(chunk.len());
                eprintln!("=== ReaComp chunk section ===\n{}", &chunk[start..end]);
            }
        } else {
            ctx.log(&format!(
                "DIAGNOSTIC: Found {} PLINK lines in chunk",
                plink_lines.len()
            ));
            for line in &plink_lines {
                eprintln!("  PLINK: {}", line.trim());
            }
        }
    }

    // ─── Move macro sliders and verify ReaComp params respond ──────
    //
    // The JSFX outputs MIDI CC on bus 15, ch 16. Plink routes CC → param.
    // REAPER needs the audio engine processing @block to send MIDI CC.
    // Start playback to ensure audio buffers are processed.

    // Start playback to trigger JSFX @block execution.
    let transport = project.transport();
    transport.play().await.map_err(|e| eyre::eyre!("{e}"))?;
    ctx.log("Transport: playback started");
    settle_long().await;

    // Check if Ratio already changed from initial @block CC sends.
    // With P.Inst=3 and ModAmt set, CalculateTotalOut should produce
    // non-zero SendAmt immediately.
    let ratio_binding = setup.bindings.iter().find(|b| b.knob_id == "compress");
    let attack_binding = setup
        .bindings
        .iter()
        .find(|b| b.knob_id == "dynamics" && b.cc_number == 2);

    if let Some(rb) = ratio_binding {
        let initial = target_fx_readback
            .param(rb.target_param_index)
            .get()
            .await?;
        ctx.log(&format!(
            "Ratio param after playback start: {initial:.4} (idx={})",
            rb.target_param_index
        ));

        // Move compress macro to 0.9 (was 0.6).
        macros_fx.param(0).set(0.9).await?;
        settle_long().await;

        let after = target_fx_readback
            .param(rb.target_param_index)
            .get()
            .await?;

        ctx.log(&format!(
            "Ratio param after slider move to 0.9: {after:.4}"
        ));

        // The param should have changed — either from initial CC sends
        // or from the slider move. Check against the original default (0.0303).
        assert!(
            (after - 0.0303_f64).abs() > 0.001 || (initial - 0.0303_f64).abs() > 0.001,
            "Ratio param must respond to macro modulation. \
             Initial after playback: {initial:.4}, after slider move: {after:.4}. \
             Both still at default 0.0303 — CC pipeline not working."
        );
        ctx.log("PASS: Ratio param responded to macro modulation");
    }

    if let Some(ab) = attack_binding {
        let initial = target_fx_readback
            .param(ab.target_param_index)
            .get()
            .await?;
        ctx.log(&format!(
            "Attack param after playback start: {initial:.4} (idx={})",
            ab.target_param_index
        ));

        // Move dynamics macro to 0.8 (was 0.4).
        macros_fx.param(1).set(0.8).await?;
        settle_long().await;

        let after = target_fx_readback
            .param(ab.target_param_index)
            .get()
            .await?;

        ctx.log(&format!(
            "Attack param after slider move to 0.8: {after:.4}"
        ));

        assert!(
            (after - initial).abs() > 0.001 || (initial - 0.5_f64).abs() > 0.01,
            "Attack param must respond to macro modulation. \
             Initial: {initial:.4}, after slider: {after:.4}"
        );
        ctx.log("PASS: Attack param responded to macro modulation");
    }

    // Stop playback.
    transport.stop().await.map_err(|e| eyre::eyre!("{e}"))?;

    ctx.log("macro_setup_reacomp_full_roundtrip: ALL PASS");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: No macro bank → returns None (no JSFX inserted)
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn macro_setup_skips_block_without_macros(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    let track = project.tracks().add("No Macros Test", None).await?;
    settle().await;

    let target_fx = track.fx_chain().add(REACOMP).await?;
    settle().await;

    // Block with no macro bank.
    let block = Block::from_parameters(vec![BlockParameter::new("ratio", "Ratio", 0.5)]);

    let result = macro_setup::setup_macros_for_block(&track, &target_fx, &block)
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    assert!(
        result.is_none(),
        "should return None for block without macro bank"
    );

    // Verify no JSFX was inserted.
    let fx_count = track.fx_chain().count().await?;
    assert_eq!(fx_count, 1, "should still have only 1 FX (ReaComp)");

    ctx.log("macro_setup_skips_block_without_macros: PASS");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Re-run macro setup reuses existing JSFX instead of inserting a second
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn macro_setup_reuses_existing_jsfx(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    let track = project.tracks().add("Reuse JSFX Test", None).await?;
    settle().await;

    let target_fx = track.fx_chain().add(REACOMP).await?;
    settle().await;

    let block = build_reacomp_block_with_macros();

    // Run setup twice.
    let result1 = macro_setup::setup_macros_for_block(&track, &target_fx, &block)
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .ok_or_else(|| eyre::eyre!("First setup returned None"))?;
    settle().await;

    let result2 = macro_setup::setup_macros_for_block(&track, &target_fx, &block)
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .ok_or_else(|| eyre::eyre!("Second setup returned None"))?;
    settle().await;

    // Should still have only 2 FX (not 3).
    let fx_count = track.fx_chain().count().await?;
    assert_eq!(
        fx_count, 2,
        "should still have 2 FX after running setup twice, got {fx_count}"
    );

    // Both should reference the same JSFX GUID.
    assert_eq!(
        result1.macros_fx_guid, result2.macros_fx_guid,
        "JSFX GUID should be the same across both setups"
    );

    ctx.log("macro_setup_reuses_existing_jsfx: PASS");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Discover ReaComp params — diagnostic test to log all param names/indices
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn macro_setup_discover_reacomp_params(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    let track = project.tracks().add("ReaComp Discovery", None).await?;
    settle().await;

    let fx = track.fx_chain().add(REACOMP).await?;
    settle().await;

    let params = fx.parameters().await?;
    ctx.log(&format!("ReaComp parameter inventory ({} params):", params.len()));
    for p in &params {
        ctx.log(&format!(
            "  [{}] {} = {:.4} ({})",
            p.index, p.name, p.value, p.formatted
        ));
    }

    assert!(
        params.len() >= 5,
        "ReaComp should have at least 5 params, got {}",
        params.len()
    );

    // Verify known params exist.
    let has_ratio = params.iter().any(|p| p.name.contains("Ratio"));
    let has_attack = params.iter().any(|p| p.name.contains("Attack"));
    let has_release = params.iter().any(|p| p.name.contains("Release"));

    assert!(has_ratio, "ReaComp should have a 'Ratio' param");
    assert!(has_attack, "ReaComp should have an 'Attack' param");
    assert!(has_release, "ReaComp should have a 'Release' param");

    ctx.log("macro_setup_discover_reacomp_params: PASS");
    Ok(())
}
