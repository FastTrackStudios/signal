//! REAPER integration test: load FTS Compressor plugin and verify params.
//!
//! Run with: `cargo xtask reaper-test comp_load`

use reaper_test::reaper_test;

/// CLAP browser name for FTS Compressor in REAPER.
const FTS_COMP_CLAP: &str = "CLAP: FTS Compressor";

/// Expected parameter names (must match FtsCompParams field names in lib.rs).
const EXPECTED_PARAMS: &[&str] = &[
    "Threshold",
    "Ratio",
    "Attack",
    "Release",
    "Knee",
    "Auto Gain",
    "Feedback",
    "Stereo Link",
    "Detector RMS",
    "Inertia",
    "Inertia Decay",
    "Ceiling",
    "Drive",
    "Character",
    "Mix",
    "Multiband",
    "Input",
    "Output",
    "SC HPF",
    "SC LPF",
    "Range",
    "Gate Threshold",
    "Gate Ratio",
    "Up Threshold",
    "Up Ratio",
    "Hold",
    "Lookahead",
    "Style",
    "Profile",
    "Profile Drive",
    "Profile Output",
];

#[reaper_test(isolated)]
async fn comp_load(ctx: &reaper_test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    // Create a track and load the compressor
    let track = project.tracks().add("Comp Test", None).await?;
    ctx.log("Created track: Comp Test");

    let fx = match track.fx_chain().add(FTS_COMP_CLAP).await {
        Ok(fx) => fx,
        Err(e) => {
            ctx.log(&format!("FAILED to add FX '{}': {:?}", FTS_COMP_CLAP, e));
            return Err(eyre::eyre!("Failed to add FX: {:?}", e));
        }
    };
    ctx.log(&format!("Loaded FTS Compressor: {:?}", fx));

    // Verify it's on the chain
    let fx_count = track.fx_chain().count().await?;
    assert_eq!(fx_count, 1, "Expected exactly 1 FX on track");

    // Enumerate all parameters
    let params = fx.parameters().await?;
    ctx.log(&format!("Total parameters: {}", params.len()));

    for p in &params {
        ctx.log(&format!(
            "  [{:>2}] {:<20} = {:.4}",
            p.index, p.name, p.value
        ));
    }

    // Verify all expected params exist
    for expected_name in EXPECTED_PARAMS {
        let found = params.iter().any(|p| p.name == *expected_name);
        assert!(
            found,
            "Expected parameter '{}' not found. Available: {:?}",
            expected_name,
            params.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
    }
    ctx.log(&format!(
        "All {} expected parameters found",
        EXPECTED_PARAMS.len()
    ));

    // ── Verify defaults ─────────────────────────────────────────────

    // Threshold: default -20 dB → normalized 0.6667 (linear -60..0)
    let threshold = params.iter().find(|p| p.name == "Threshold").unwrap();
    assert!(
        (threshold.value - (40.0 / 60.0)).abs() < 0.01,
        "Threshold should default to ~0.6667 (-20 dB), got {}",
        threshold.value
    );
    ctx.log(&format!(
        "Threshold default: {:.4} (expected ~0.6667)",
        threshold.value
    ));

    // Ratio: default 4:1 on skewed range 1..20
    let ratio = params.iter().find(|p| p.name == "Ratio").unwrap();
    assert!(
        ratio.value > 0.0 && ratio.value < 1.0,
        "Ratio should be between 0 and 1 normalized, got {}",
        ratio.value
    );
    ctx.log(&format!("Ratio default: {:.4}", ratio.value));

    // Feedback: default 0%
    let feedback = params.iter().find(|p| p.name == "Feedback").unwrap();
    assert!(
        feedback.value < 0.01,
        "Feedback should default to 0%, got {}",
        feedback.value
    );

    // Stereo Link: default 100%
    let link = params.iter().find(|p| p.name == "Stereo Link").unwrap();
    assert!(
        (link.value - 1.0).abs() < 0.01,
        "Stereo Link should default to 100%, got {}",
        link.value
    );

    // Input/Output gain: default 0 dB → normalized 0.5 (linear -24..24)
    let input = params.iter().find(|p| p.name == "Input").unwrap();
    assert!(
        (input.value - 0.5).abs() < 0.01,
        "Input gain should default to 0.5 (0 dB), got {}",
        input.value
    );

    let output = params.iter().find(|p| p.name == "Output").unwrap();
    assert!(
        (output.value - 0.5).abs() < 0.01,
        "Output gain should default to 0.5 (0 dB), got {}",
        output.value
    );

    // Newly added mode controls default to bypass/off.
    for name in [
        "Auto Gain",
        "Detector RMS",
        "Drive",
        "Multiband",
        "SC LPF",
        "Gate Ratio",
        "Up Ratio",
        "Hold",
        "Lookahead",
        "Style",
        "Profile",
    ] {
        let param = params.iter().find(|p| p.name == name).unwrap();
        assert!(
            param.value < 0.01,
            "{name} should default to bypass/off/first option, got {}",
            param.value
        );
    }

    for name in ["Profile Drive", "Profile Output"] {
        let param = params.iter().find(|p| p.name == name).unwrap();
        assert!(
            (param.value - 0.5).abs() < 0.01,
            "{name} should default to midpoint, got {}",
            param.value
        );
    }

    let range = params.iter().find(|p| p.name == "Range").unwrap();
    assert!(
        (range.value - 1.0).abs() < 0.01,
        "Range should default to maximum normalized value, got {}",
        range.value
    );

    let gate_threshold = params.iter().find(|p| p.name == "Gate Threshold").unwrap();
    assert!(
        (gate_threshold.value - 0.2).abs() < 0.01,
        "Gate Threshold should default to -80 dB normalized to 0.2, got {}",
        gate_threshold.value
    );

    let up_threshold = params.iter().find(|p| p.name == "Up Threshold").unwrap();
    assert!(
        (up_threshold.value - 0.4).abs() < 0.01,
        "Up Threshold should default to -60 dB normalized to 0.4, got {}",
        up_threshold.value
    );

    // SC HPF: default 85 Hz → normalized 85/300 on linear 0..300 Hz.
    let sc_hpf = params.iter().find(|p| p.name == "SC HPF").unwrap();
    assert!(
        (sc_hpf.value - (85.0 / 300.0)).abs() < 0.01,
        "SC HPF should default to normalized 85/300, got {}",
        sc_hpf.value
    );

    ctx.log("comp_load: PASSED");
    Ok(())
}
