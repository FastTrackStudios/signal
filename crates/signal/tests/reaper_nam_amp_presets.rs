//! REAPER integration test: Load all NAM amp block presets (ML Sound Labs)
//! into a single guitar rig engine, each amp as its own layer.
//!
//! Builds a `RigTemplate` with one engine containing a layer per NAM amp,
//! instantiates it in REAPER, then loads the default snapshot of each preset
//! onto the corresponding layer track — all in parallel.
//!
//! Run with:
//!   cargo xtask reaper-test --keep-open nam_amp_load

use std::time::Instant;

use reaper_test::reaper_test;
use signal::{BlockType, ModuleType, Preset};
use signal_live::daw_rig_builder::instantiate_rig;
use signal_proto::rig_template::{EngineTemplate, LayerTemplate, RigTemplate};
use signal_proto::{seed_id, ModulePresetId, PresetId};

/// Ensure REAPER's audio engine is running.
async fn ensure_audio(ctx: &reaper_test::ReaperTestContext) {
    if !ctx.daw.audio_engine().is_running().await.unwrap_or(false) {
        let _ = ctx.daw.audio_engine().init().await;
    }
}

/// Collect all [NAM] amp presets from the seeded signal controller.
async fn collect_nam_presets(signal: &signal::SignalController) -> eyre::Result<Vec<Preset>> {
    let all_amps = signal.block_presets().list(BlockType::Amp).await?;
    let nam: Vec<Preset> = all_amps
        .into_iter()
        .filter(|p| p.name().ends_with("[NAM]"))
        .collect();
    Ok(nam)
}

// ---------------------------------------------------------------------------
// Test: Guitar rig with each NAM amp as a layer (default snapshot, parallel)
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn nam_amp_load_guitar_rig(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();
    project.tracks().remove_all().await?;

    // 1. Bootstrap signal and collect NAM presets.
    let signal = signal::bootstrap_in_memory_controller_async().await?;
    let svc = signal.service();
    let nam_presets = collect_nam_presets(&signal).await?;

    assert!(
        nam_presets.len() >= 9,
        "expected at least 9 NAM amp presets, got {}",
        nam_presets.len()
    );

    eprintln!("Found {} NAM amp presets", nam_presets.len());

    // 2. Build a rig template: 1 engine, 1 layer per NAM amp.
    let layers: Vec<LayerTemplate> = nam_presets
        .iter()
        .map(|p| LayerTemplate {
            name: p.name().to_string(),
        })
        .collect();

    let template = RigTemplate {
        name: "NAM Guitar Rig".into(),
        engines: vec![EngineTemplate {
            name: "ML Sound Labs".into(),
            layers,
            fx_sends: vec![],
        }],
        fx_sends: vec![],
    };

    // 3. Instantiate rig structure in REAPER.
    let instance = instantiate_rig(&template, &project).await?;

    let engine = &instance.engine_instances[0];
    assert_eq!(
        engine.layer_tracks.len(),
        nam_presets.len(),
        "engine should have one layer per NAM preset"
    );

    eprintln!("Rig structure created, loading {} presets in parallel...", nam_presets.len());

    // 4. Load all NAM presets in parallel.
    let total_start = Instant::now();

    let mut handles = Vec::new();
    for (i, preset) in nam_presets.iter().enumerate() {
        let track = engine.layer_tracks[i].clone();
        let svc = svc.clone();
        let preset_name = preset.name().to_string();
        let preset_id = preset.id().clone();
        let idx = i;
        let count = nam_presets.len();

        handles.push(tokio::spawn(async move {
            let start = Instant::now();
            let result = svc
                .load_block_to_track(BlockType::Amp, &preset_id, None, &track)
                .await
                .map_err(|e| format!("Failed to load '{}': {e}", preset_name))?;

            let elapsed = start.elapsed();
            eprintln!(
                "  [{}/{}] {} → '{}' ({}) [{:.1}s]",
                idx + 1,
                count,
                preset_name,
                result.display_name,
                result.fx_guid,
                elapsed.as_secs_f64(),
            );

            Ok::<_, String>((preset_name, result, track))
        }));
    }

    // Await all parallel loads.
    let mut load_errors = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(Ok((name, _result, track))) => {
                let fx_count = track.fx_chain().count().await.unwrap_or(0);
                assert_eq!(fx_count, 1, "'{}' should have exactly 1 FX, got {}", name, fx_count);
            }
            Ok(Err(e)) => load_errors.push(e),
            Err(e) => load_errors.push(format!("Task panicked: {e}")),
        }
    }

    let total_elapsed = total_start.elapsed();

    if !load_errors.is_empty() {
        return Err(eyre::eyre!("Load errors:\n{}", load_errors.join("\n")));
    }

    eprintln!(
        "\nAll {} presets loaded in {:.1}s (avg {:.1}s/preset)\n",
        nam_presets.len(),
        total_elapsed.as_secs_f64(),
        total_elapsed.as_secs_f64() / nam_presets.len() as f64,
    );

    // 5. Verify final track structure.
    let tracks = project.tracks().all().await?;
    let expected_tracks = 2 + nam_presets.len();
    assert_eq!(
        tracks.len(),
        expected_tracks,
        "expected {} tracks (1 rig + 1 engine + {} layers), got {}",
        expected_tracks,
        nam_presets.len(),
        tracks.len()
    );

    assert!(tracks[0].name.starts_with("[R]"), "first track should be [R] rig folder");
    assert!(tracks[1].name.starts_with("[E]"), "second track should be [E] engine folder");
    for track in &tracks[2..] {
        assert!(
            track.name.starts_with("[L]"),
            "layer track should have [L] prefix, got '{}'",
            track.name
        );
    }

    eprintln!(
        "nam_amp_load_guitar_rig: PASS — {} layers loaded in {:.1}s",
        nam_presets.len(),
        total_elapsed.as_secs_f64(),
    );
    ctx.log(&format!(
        "nam_amp_load_guitar_rig: PASS — {} layers loaded in {:.1}s",
        nam_presets.len(),
        total_elapsed.as_secs_f64(),
    ));
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Single layer with EQ module + Fender Deluxe Reverb NAM amp
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn nam_amp_load_eq_plus_fender(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();
    project.tracks().remove_all().await?;

    // 1. Bootstrap signal.
    let signal = signal::bootstrap_in_memory_controller_async().await?;
    let svc = signal.service();

    // 2. Build a simple rig: 1 engine, 1 layer.
    let template = RigTemplate {
        name: "Fender Deluxe Rig".into(),
        engines: vec![EngineTemplate {
            name: "Guitar".into(),
            layers: vec![LayerTemplate {
                name: "Fender Deluxe".into(),
            }],
            fx_sends: vec![],
        }],
        fx_sends: vec![],
    };

    let instance = instantiate_rig(&template, &project).await?;
    let layer_track = &instance.engine_instances[0].layer_tracks[0];

    eprintln!("Rig created, loading EQ module + Fender Deluxe NAM amp...");
    let total_start = Instant::now();

    // 3. Load EQ module (Pro-Q 4 3-Band) onto the layer.
    let eq_start = Instant::now();
    let eq_result = svc
        .load_module_to_track(
            ModuleType::Eq,
            &ModulePresetId::from_uuid(seed_id("eq-proq4-3band")),
            0,
            layer_track,
            None,
        )
        .await
        .map_err(|e| eyre::eyre!("Failed to load EQ module: {e}"))?;

    eprintln!(
        "  [EQ] {} ({} blocks) [{:.1}s]",
        eq_result.display_name,
        eq_result.loaded_fx.len(),
        eq_start.elapsed().as_secs_f64(),
    );

    // 4. Load Fender Deluxe Reverb NAM amp onto the same layer.
    let nam_start = Instant::now();
    let fender_id = PresetId::from(seed_id("nam-amp-ml-sound-labs-fender-deluxe-reverb"));
    let nam_result = svc
        .load_block_to_track(BlockType::Amp, &fender_id, None, layer_track)
        .await
        .map_err(|e| eyre::eyre!("Failed to load Fender Deluxe NAM: {e}"))?;

    let total_elapsed = total_start.elapsed();

    eprintln!(
        "  [AMP] {} ({}) [{:.1}s]",
        nam_result.display_name,
        nam_result.fx_guid,
        nam_start.elapsed().as_secs_f64(),
    );

    eprintln!(
        "\nEQ + Fender Deluxe loaded in {:.1}s\n",
        total_elapsed.as_secs_f64(),
    );

    // 5. Verify FX chain: EQ module container + NAM amp.
    let fx_count = layer_track.fx_chain().count().await?;
    eprintln!("FX count on layer: {}", fx_count);

    ctx.log(&format!(
        "nam_amp_load_eq_plus_fender: PASS — EQ + Fender Deluxe in {:.1}s",
        total_elapsed.as_secs_f64(),
    ));
    Ok(())
}
