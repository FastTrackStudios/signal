//! Integration tests for RigBuilder — verifies the fluent builder API
//! produces correct, resolvable domain hierarchies.
//!
//! Demonstrates the recommended test pattern:
//! - Use `RigBuilder` for constructing test data (no `seed_id`)
//! - Use `fixtures::*` for shared helpers and seed data references
//! - Use `graph.find_param()` instead of manual graph walking
//!
//!   cargo test -p signal --test signal_builder_api -- --nocapture

mod fixtures;

use fixtures::*;
use signal::{
    block::BlockType, builder::RigBuilder, profile::PatchId, resolve::ResolveTarget, rig::RigType,
    EngineType,
};

// ═════════════════════════════════════════════════════════════
//  Builder + save + resolve round-trip
// ═════════════════════════════════════════════════════════════

/// Build a guitar rig from scratch, save it, and resolve every scene.
#[tokio::test]
async fn build_and_resolve_guitar_rig() {
    let signal = controller().await;

    let built = RigBuilder::new("Test Guitar Rig")
        .block_preset("Amp", BlockType::Amp, |bp| {
            bp.param("gain", "Gain", 0.45)
                .param("bass", "Bass", 0.5)
                .param("mid", "Mid", 0.5)
                .param("treble", "Treble", 0.5)
                .param("presence", "Presence", 0.5)
                .param("master", "Master", 0.6)
                .snapshot("Lead", |sp| {
                    sp.param("gain", "Gain", 0.8)
                        .param("bass", "Bass", 0.5)
                        .param("mid", "Mid", 0.6)
                        .param("treble", "Treble", 0.5)
                        .param("presence", "Presence", 0.5)
                        .param("master", "Master", 0.7)
                })
        })
        .block_preset("Drive", BlockType::Drive, |bp| {
            bp.param("level", "Level", 0.5).param("tone", "Tone", 0.5)
        })
        .scene("Clean")
        .scene("Lead")
        .scene("Crunch")
        .with_profile()
        .build();

    // Save everything
    signal.save_built_rig(&built).await;

    // Verify rig was saved
    let rig = signal
        .rigs().load(built.rig_id.clone())
        .await
        .expect("rig should exist after save");
    assert_eq!(rig.name, "Test Guitar Rig");
    assert_eq!(rig.variants.len(), 3);
    assert_eq!(rig.rig_type, Some(RigType::Guitar));

    // Verify profile was saved
    let profile = signal
        .profiles().load(built.profile_id.clone().unwrap())
        .await
        .expect("profile should exist after save");
    assert_eq!(profile.patches.len(), 3);
    assert_eq!(profile.default_patch().unwrap().name, "Clean");

    // Resolve each scene
    for (name, scene_id) in &built.scene_ids {
        let graph = signal
            .resolve_target(ResolveTarget::RigScene {
                rig_id: built.rig_id.clone(),
                scene_id: scene_id.clone(),
            })
            .await
            .unwrap_or_else(|e| panic!("failed to resolve scene '{name}': {e:?}"));

        assert!(
            !graph.engines.is_empty(),
            "scene '{name}' should have engines"
        );

        // Verify find_param works on the resolved graph
        let gain = graph.find_param("amp", "gain");
        assert!(
            gain.is_some(),
            "scene '{name}' should have amp gain parameter"
        );
        println!("  Scene '{name}': amp gain = {:.2}", gain.unwrap());
    }
}

/// Build a rig and activate each patch via the profile.
#[tokio::test]
async fn build_and_activate_patches() {
    let signal = controller().await;

    let built = RigBuilder::new("Activation Test")
        .block_preset("Amp", BlockType::Amp, |bp| {
            bp.param("gain", "Gain", 0.4).param("tone", "Tone", 0.5)
        })
        .scene("Clean")
        .scene("Lead")
        .with_profile()
        .build();

    signal.save_built_rig(&built).await;

    let profile_id = built.profile_id.clone().unwrap();

    // Activate default (None) — should use Clean
    let default_graph = signal
        .profiles().activate(profile_id.clone(), None::<PatchId>)
        .await
        .expect("activate_patch(default) failed");
    assert!(!default_graph.engines.is_empty());

    // Activate each patch by name
    for (name, patch_id) in &built.patch_ids {
        let graph = signal
            .profiles().activate(profile_id.clone(), Some(patch_id.clone()))
            .await
            .unwrap_or_else(|e| panic!("activate_patch('{name}') failed: {e:?}"));

        println!(
            "  Patch '{name}': {} engines, {} overrides",
            graph.engines.len(),
            graph.effective_overrides.len()
        );
    }
}

/// Minimal builder — no block presets, single scene.
#[tokio::test]
async fn build_minimal_rig() {
    let signal = controller().await;

    let built = RigBuilder::new("Minimal").build();

    signal.save_built_rig(&built).await;

    let rig = signal
        .rigs().load(built.rig_id.clone())
        .await
        .expect("minimal rig should exist");
    assert_eq!(rig.variants.len(), 1);
    assert_eq!(rig.variants[0].name, "Default");
}

/// Builder for keys rig type.
#[tokio::test]
async fn build_keys_rig() {
    let signal = controller().await;

    let built = RigBuilder::new("Keys Setup")
        .rig_type(RigType::Keys)
        .engine_type(EngineType::Keys)
        .block_preset("Piano", BlockType::Amp, |bp| {
            bp.param("brightness", "Brightness", 0.5)
                .param("warmth", "Warmth", 0.6)
        })
        .scene("Warm")
        .scene("Bright")
        .scene("Air")
        .with_named_profile("Keys Worship Profile")
        .build();

    signal.save_built_rig(&built).await;

    let rig = signal
        .rigs().load(built.rig_id.clone())
        .await
        .expect("keys rig should exist");
    assert_eq!(rig.rig_type, Some(RigType::Keys));

    let profile = signal
        .profiles().load(built.profile_id.clone().unwrap())
        .await
        .expect("profile should exist");
    assert_eq!(profile.name, "Keys Worship Profile");
    assert_eq!(profile.patches.len(), 3);
}

// ═════════════════════════════════════════════════════════════
//  Builder + find_param on seeded data
// ═════════════════════════════════════════════════════════════

/// Use find_param on the seeded guitar megarig (verifying it replaces
/// the old copy-pasted find_param_in_graph helper).
#[tokio::test]
async fn find_param_on_seeded_megarig() {
    let signal = controller().await;

    let graph = signal
        .resolve_target(ResolveTarget::RigScene {
            rig_id: guitar_megarig_id(),
            scene_id: guitar_megarig_default_scene(),
        })
        .await
        .expect("resolve should succeed");

    // find_param is now a method on ResolvedGraph
    let gain = graph.find_param("amp", "gain");
    println!("Seeded megarig default scene: amp gain = {:?}", gain);
    assert!(gain.is_some(), "seeded megarig should have amp gain");
}

/// Compare resolved params between seeded clean and lead patches.
#[tokio::test]
async fn find_param_clean_vs_lead() {
    let signal = controller().await;
    let profile_id = signal::seed_id("guitar-worship-profile");

    let clean_graph = signal
        .resolve_target(ResolveTarget::ProfilePatch {
            profile_id: profile_id.into(),
            patch_id: signal::seed_id("guitar-worship-clean").into(),
        })
        .await
        .expect("resolve clean");

    let lead_graph = signal
        .resolve_target(ResolveTarget::ProfilePatch {
            profile_id: signal::seed_id("guitar-worship-profile").into(),
            patch_id: signal::seed_id("guitar-worship-lead").into(),
        })
        .await
        .expect("resolve lead");

    let clean_gain = clean_graph.find_param("amp", "gain");
    let lead_gain = lead_graph.find_param("amp", "gain");

    println!("Clean gain: {:?}, Lead gain: {:?}", clean_gain, lead_gain);

    if let (Some(c), Some(l)) = (clean_gain, lead_gain) {
        assert!(
            l > c,
            "lead gain ({l:.3}) should exceed clean gain ({c:.3})"
        );
    }
}
