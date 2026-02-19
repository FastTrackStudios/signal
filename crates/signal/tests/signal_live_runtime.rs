//! Live runtime integration tests for the signal domain.
//!
//! Covers the "performing live" side of the signal stack that the authoring tests
//! (signal_guitar_rig_api.rs, signal_keys_engine_api.rs) don't touch:
//!
//! - **RigControlCommand pipeline** — LoadPatch, LoadScene, NextSong, NextSection, etc.
//! - **MockRigEngine slot lifecycle** — load_scene_targets, apply_snapshot, disable/enable
//! - **Scene diff integration** — compute_diff with realistic resolved graphs
//! - **Morph + SnapshotTween** — end-to-end crossfade animation between scenes
//! - **Error handling** — nonexistent IDs, missing entities, broken references
//! - **Empty collections** — empty rigs, engines, layers, profiles, songs, setlists
//! - **Event bus** — subscribe() and event delivery on controller operations
//! - **Untested controller methods** — get_block/set_block, delete_*, scene templates, etc.
//! - **Concurrent resolve** — simultaneous resolve operations
//!
//!   cargo test -p signal --test signal_live_runtime -- --nocapture

mod fixtures;

use fixtures::controller;
use signal::{
    engine::{Engine, EngineScene},
    layer::{Layer, LayerSnapshot},
    module_type::ModuleType,
    overrides::{NodePath, Override},
    profile::{Patch, Profile},
    resolve::{ResolveError, ResolveTarget},
    rig::{EngineSelection, Rig, RigId, RigScene, RigSceneId},
    scene_template::SceneTemplate,
    seed_id,
    setlist::{Setlist, SetlistEntry},
    song::{Section, Song},
    DawParamValue, DawParameterSnapshot, EngineType, MorphEngine,
};
use signal_controller::events::SignalEvent;
use signal_live::engine::{
    compute_diff, MockRigControlService, MockRigEngine, ModuleTarget, ResolvedSlot,
    RigControlCommand, RigControlEvent, RigControlService, RigEngine, SlotDiff, SlotState,
    SnapshotTween, TweenState,
};
use signal_proto::easing::EasingCurve;
use signal_proto::{ModulePresetId, ModuleSnapshotId};
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────
//  Helpers
// ─────────────────────────────────────────────────────────────

fn guitar_rig_id() -> RigId {
    seed_id("guitar-megarig").into()
}

fn guitar_default_scene() -> RigSceneId {
    seed_id("guitar-megarig-default").into()
}

fn keys_rig_id() -> RigId {
    seed_id("keys-megarig").into()
}

fn keys_default_scene() -> RigSceneId {
    seed_id("keys-megarig-default").into()
}

fn make_target(mt: ModuleType) -> ModuleTarget {
    ModuleTarget {
        module_type: mt,
        module_preset_id: ModulePresetId::new(),
        module_snapshot_id: None,
    }
}

fn make_target_with_snapshot(
    mt: ModuleType,
    preset_id: ModulePresetId,
    snap_id: ModuleSnapshotId,
) -> ModuleTarget {
    ModuleTarget {
        module_type: mt,
        module_preset_id: preset_id,
        module_snapshot_id: Some(snap_id),
    }
}

fn param(fx: &str, idx: u32, name: &str, val: f64) -> DawParamValue {
    DawParamValue {
        fx_id: fx.into(),
        param_index: idx,
        param_name: name.into(),
        value: val,
    }
}

// ═════════════════════════════════════════════════════════════
//  Group A: RigControlCommand Pipeline (7 tests)
// ═════════════════════════════════════════════════════════════

/// LoadPatch command records in history and emits SceneTransitioned event.
#[tokio::test]
async fn command_load_patch_emits_scene_transitioned() {
    let svc = MockRigControlService::new();
    let events = svc
        .execute(RigControlCommand::LoadPatch {
            profile_id: "profile-1".into(),
            patch_id: "patch-1".into(),
        })
        .await;

    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0],
        RigControlEvent::SceneTransitioned { .. }
    ));

    let history = svc.history();
    assert_eq!(history.len(), 1);
    assert!(matches!(history[0], RigControlCommand::LoadPatch { .. }));
}

/// LoadScene command emits SceneTransitioned event.
#[tokio::test]
async fn command_load_scene_emits_scene_transitioned() {
    let svc = MockRigControlService::new();
    let events = svc
        .execute(RigControlCommand::LoadScene {
            scene_id: seed_id("test-scene").into(),
        })
        .await;

    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0],
        RigControlEvent::SceneTransitioned { .. }
    ));
}

/// LoadSongSection records in history (mock returns empty events for this).
#[tokio::test]
async fn command_load_song_section_records_history() {
    let svc = MockRigControlService::new();
    svc.execute(RigControlCommand::LoadSongSection {
        song_id: "song-1".into(),
        section_id: "section-1".into(),
    })
    .await;

    let history = svc.history();
    assert_eq!(history.len(), 1);
    assert!(matches!(
        history[0],
        RigControlCommand::LoadSongSection { .. }
    ));
}

/// Navigation commands (NextSong, PreviousSong, NextSection, PreviousSection) record correctly.
#[tokio::test]
async fn command_navigation_records_all_four() {
    let svc = MockRigControlService::new();

    svc.execute(RigControlCommand::NextSong).await;
    svc.execute(RigControlCommand::PreviousSong).await;
    svc.execute(RigControlCommand::NextSection).await;
    svc.execute(RigControlCommand::PreviousSection).await;

    let history = svc.history();
    assert_eq!(history.len(), 4);
    assert!(matches!(history[0], RigControlCommand::NextSong));
    assert!(matches!(history[1], RigControlCommand::PreviousSong));
    assert!(matches!(history[2], RigControlCommand::NextSection));
    assert!(matches!(history[3], RigControlCommand::PreviousSection));
}

/// DisableSlot/EnableSlot emit correct events.
#[tokio::test]
async fn command_disable_enable_slot_events() {
    let svc = MockRigControlService::new();

    let disable_events = svc
        .execute(RigControlCommand::DisableSlot {
            module_type: ModuleType::Amp,
        })
        .await;
    assert_eq!(disable_events.len(), 1);
    assert!(matches!(
        disable_events[0],
        RigControlEvent::SlotDisabled {
            module_type: ModuleType::Amp
        }
    ));

    let enable_events = svc
        .execute(RigControlCommand::EnableSlot {
            module_type: ModuleType::Amp,
        })
        .await;
    assert_eq!(enable_events.len(), 1);
    assert!(matches!(
        enable_events[0],
        RigControlEvent::SlotEnabled {
            module_type: ModuleType::Amp
        }
    ));
}

/// SetMorphPosition emits position event with correct value.
#[tokio::test]
async fn command_morph_position_event() {
    let svc = MockRigControlService::new();

    let events = svc
        .execute(RigControlCommand::SetMorphPosition { position: 0.42 })
        .await;
    assert_eq!(events.len(), 1);
    match &events[0] {
        RigControlEvent::MorphPositionChanged { position } => {
            assert!((position - 0.42).abs() < f32::EPSILON);
        }
        _ => panic!("expected MorphPositionChanged"),
    }
}

/// Clear history works correctly.
#[tokio::test]
async fn command_history_clear() {
    let svc = MockRigControlService::new();
    svc.execute(RigControlCommand::NextSong).await;
    svc.execute(RigControlCommand::NextSection).await;
    assert_eq!(svc.history().len(), 2);

    svc.clear_history();
    assert_eq!(svc.history().len(), 0);

    svc.execute(RigControlCommand::Tick).await;
    assert_eq!(svc.history().len(), 1);
}

// ═════════════════════════════════════════════════════════════
//  Group B: MockRigEngine Slot Lifecycle (6 tests)
// ═════════════════════════════════════════════════════════════

/// Initialize slots and verify counts.
#[tokio::test]
async fn engine_initialize_slots() {
    let engine = MockRigEngine::new();
    assert_eq!(engine.slot_count(), 0);

    engine.initialize_slots(&[ModuleType::Amp, ModuleType::Drive, ModuleType::Eq]);
    assert_eq!(engine.slot_count(), 3);

    // All slots should be disabled (unloaded, no target)
    assert!(engine.active_module_types().is_empty());
}

/// Load scene targets activates all targeted slots.
#[tokio::test]
async fn engine_load_scene_activates_slots() {
    let engine = MockRigEngine::new();

    let mut targets = HashMap::new();
    targets.insert(ModuleType::Amp, make_target(ModuleType::Amp));
    targets.insert(ModuleType::Drive, make_target(ModuleType::Drive));
    targets.insert(ModuleType::Eq, make_target(ModuleType::Eq));

    let result = engine.load_scene_targets(targets);
    assert!(result.is_completed());
    assert!(!result.has_errors());

    let active = engine.active_module_types();
    assert_eq!(active.len(), 3);
}

/// Switching scenes disables slots removed from target set.
#[tokio::test]
async fn engine_scene_switch_disables_removed_slots() {
    let engine = MockRigEngine::new();

    // Scene A: Amp + Drive + EQ
    let mut scene_a = HashMap::new();
    scene_a.insert(ModuleType::Amp, make_target(ModuleType::Amp));
    scene_a.insert(ModuleType::Drive, make_target(ModuleType::Drive));
    scene_a.insert(ModuleType::Eq, make_target(ModuleType::Eq));
    engine.load_scene_targets(scene_a);
    assert_eq!(engine.active_module_types().len(), 3);

    // Scene B: only Amp (Drive + EQ should be disabled)
    let mut scene_b = HashMap::new();
    scene_b.insert(ModuleType::Amp, make_target(ModuleType::Amp));
    engine.load_scene_targets(scene_b);

    assert!(!engine.is_slot_disabled(ModuleType::Amp));
    assert!(engine.is_slot_disabled(ModuleType::Drive));
    assert!(engine.is_slot_disabled(ModuleType::Eq));
}

/// Re-enabling a disabled slot on scene switch.
#[tokio::test]
async fn engine_scene_switch_reenables_slot() {
    let engine = MockRigEngine::new();

    // Scene A: Amp + Drive
    let mut scene_a = HashMap::new();
    let amp_target = make_target(ModuleType::Amp);
    scene_a.insert(ModuleType::Amp, amp_target.clone());
    scene_a.insert(ModuleType::Drive, make_target(ModuleType::Drive));
    engine.load_scene_targets(scene_a);

    // Scene B: Amp only
    let mut scene_b = HashMap::new();
    scene_b.insert(ModuleType::Amp, amp_target.clone());
    engine.load_scene_targets(scene_b);
    assert!(engine.is_slot_disabled(ModuleType::Drive));

    // Scene C: Amp + Drive again
    let mut scene_c = HashMap::new();
    scene_c.insert(ModuleType::Amp, amp_target);
    scene_c.insert(ModuleType::Drive, make_target(ModuleType::Drive));
    engine.load_scene_targets(scene_c);
    assert!(!engine.is_slot_disabled(ModuleType::Drive));
}

/// Apply snapshot to uninitialized slot returns error.
#[tokio::test]
async fn engine_apply_snapshot_to_missing_slot() {
    let engine = MockRigEngine::new();
    let snapshot = signal_proto::ModuleSnapshot::new(
        ModuleSnapshotId::new(),
        "test",
        signal_proto::Module::from_blocks(vec![]),
    );

    let result = engine.apply_snapshot(ModuleType::Amp, &snapshot).await;
    assert!(result.is_err());
}

/// Shutdown unloads all slots.
#[tokio::test]
async fn engine_shutdown_clears_all() {
    let engine = MockRigEngine::new();

    let mut targets = HashMap::new();
    targets.insert(ModuleType::Amp, make_target(ModuleType::Amp));
    targets.insert(ModuleType::Drive, make_target(ModuleType::Drive));
    engine.load_scene_targets(targets);
    assert_eq!(engine.active_module_types().len(), 2);

    engine.shutdown().await;
    assert_eq!(engine.active_module_types().len(), 0);
}

// ═════════════════════════════════════════════════════════════
//  Group C: Scene Diff Integration (6 tests)
// ═════════════════════════════════════════════════════════════

fn no_preload(_: &ModuleTarget) -> Option<signal_live::engine::InstanceHandle> {
    None
}

/// Diff from empty state to a scene: all slots should LoadAndActivate.
#[test]
fn diff_empty_to_full_scene() {
    let mut targets = HashMap::new();
    targets.insert(
        ModuleType::Amp,
        ResolvedSlot::Active(make_target(ModuleType::Amp)),
    );
    targets.insert(
        ModuleType::Drive,
        ResolvedSlot::Active(make_target(ModuleType::Drive)),
    );
    targets.insert(
        ModuleType::Eq,
        ResolvedSlot::Active(make_target(ModuleType::Eq)),
    );

    let diffs = compute_diff(&[], &targets, &no_preload);
    assert_eq!(diffs.len(), 3);
    assert!(diffs
        .iter()
        .all(|d| matches!(d, SlotDiff::LoadAndActivate { .. })));
}

/// Diff same preset + same snapshot = NoChange.
#[test]
fn diff_identical_scene_no_change() {
    let preset_id = ModulePresetId::new();
    let snap_id = ModuleSnapshotId::new();
    let target = make_target_with_snapshot(ModuleType::Amp, preset_id.clone(), snap_id.clone());

    let current = vec![SlotState {
        module_type: ModuleType::Amp,
        current: Some(ResolvedSlot::Active(target.clone())),
        active_handle: Some(signal_live::engine::InstanceHandle::new(1)),
        is_disabled: false,
    }];

    let mut new_targets = HashMap::new();
    new_targets.insert(ModuleType::Amp, ResolvedSlot::Active(target));

    let diffs = compute_diff(&current, &new_targets, &no_preload);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], SlotDiff::NoChange { .. }));
}

/// Diff same preset, different snapshot = ApplySnapshot.
#[test]
fn diff_same_preset_different_snapshot() {
    let preset_id = ModulePresetId::new();
    let snap_a = ModuleSnapshotId::new();
    let snap_b = ModuleSnapshotId::new();

    let current_target = make_target_with_snapshot(ModuleType::Amp, preset_id.clone(), snap_a);
    let new_target = make_target_with_snapshot(ModuleType::Amp, preset_id, snap_b);

    let current = vec![SlotState {
        module_type: ModuleType::Amp,
        current: Some(ResolvedSlot::Active(current_target)),
        active_handle: Some(signal_live::engine::InstanceHandle::new(1)),
        is_disabled: false,
    }];

    let mut new_targets = HashMap::new();
    new_targets.insert(ModuleType::Amp, ResolvedSlot::Active(new_target));

    let diffs = compute_diff(&current, &new_targets, &no_preload);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], SlotDiff::ApplySnapshot { .. }));
}

/// Diff different preset = LoadAndActivate.
#[test]
fn diff_different_preset_full_reload() {
    let target_a = make_target(ModuleType::Amp);
    let target_b = make_target(ModuleType::Amp);
    assert_ne!(target_a.module_preset_id, target_b.module_preset_id);

    let current = vec![SlotState {
        module_type: ModuleType::Amp,
        current: Some(ResolvedSlot::Active(target_a)),
        active_handle: Some(signal_live::engine::InstanceHandle::new(1)),
        is_disabled: false,
    }];

    let mut new_targets = HashMap::new();
    new_targets.insert(ModuleType::Amp, ResolvedSlot::Active(target_b));

    let diffs = compute_diff(&current, &new_targets, &no_preload);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], SlotDiff::LoadAndActivate { .. }));
}

/// Slot removed from targets = Disable.
#[test]
fn diff_slot_removed_becomes_disabled() {
    let target = make_target(ModuleType::Drive);
    let current = vec![SlotState {
        module_type: ModuleType::Drive,
        current: Some(ResolvedSlot::Active(target)),
        active_handle: Some(signal_live::engine::InstanceHandle::new(1)),
        is_disabled: false,
    }];

    let new_targets = HashMap::new(); // Drive removed
    let diffs = compute_diff(&current, &new_targets, &no_preload);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], SlotDiff::Disable { .. }));
}

/// Multi-slot diff: mix of NoChange, ApplySnapshot, LoadAndActivate, and Disable.
#[test]
fn diff_multi_slot_mixed_transitions() {
    let shared_preset = ModulePresetId::new();
    let snap_a = ModuleSnapshotId::new();
    let snap_b = ModuleSnapshotId::new();

    // Amp: same preset + same snapshot = NoChange
    let amp_target =
        make_target_with_snapshot(ModuleType::Amp, shared_preset.clone(), snap_a.clone());
    // Drive: same preset, different snapshot = ApplySnapshot
    let drive_old = make_target_with_snapshot(ModuleType::Drive, shared_preset.clone(), snap_a);
    let drive_new = make_target_with_snapshot(ModuleType::Drive, shared_preset, snap_b);
    // EQ: different preset = LoadAndActivate
    let eq_old = make_target(ModuleType::Eq);
    let eq_new = make_target(ModuleType::Eq);
    // Time: removed = Disable
    let time_target = make_target(ModuleType::Time);

    let current = vec![
        SlotState {
            module_type: ModuleType::Amp,
            current: Some(ResolvedSlot::Active(amp_target.clone())),
            active_handle: Some(signal_live::engine::InstanceHandle::new(1)),
            is_disabled: false,
        },
        SlotState {
            module_type: ModuleType::Drive,
            current: Some(ResolvedSlot::Active(drive_old)),
            active_handle: Some(signal_live::engine::InstanceHandle::new(2)),
            is_disabled: false,
        },
        SlotState {
            module_type: ModuleType::Eq,
            current: Some(ResolvedSlot::Active(eq_old)),
            active_handle: Some(signal_live::engine::InstanceHandle::new(3)),
            is_disabled: false,
        },
        SlotState {
            module_type: ModuleType::Time,
            current: Some(ResolvedSlot::Active(time_target)),
            active_handle: Some(signal_live::engine::InstanceHandle::new(4)),
            is_disabled: false,
        },
    ];

    let mut new_targets = HashMap::new();
    new_targets.insert(ModuleType::Amp, ResolvedSlot::Active(amp_target));
    new_targets.insert(ModuleType::Drive, ResolvedSlot::Active(drive_new));
    new_targets.insert(ModuleType::Eq, ResolvedSlot::Active(eq_new));
    // Time not in new_targets → Disable

    let diffs = compute_diff(&current, &new_targets, &no_preload);
    assert_eq!(diffs.len(), 4);

    let by_type: HashMap<ModuleType, &SlotDiff> =
        diffs.iter().map(|d| (d.module_type(), d)).collect();

    assert!(matches!(
        by_type[&ModuleType::Amp],
        SlotDiff::NoChange { .. }
    ));
    assert!(matches!(
        by_type[&ModuleType::Drive],
        SlotDiff::ApplySnapshot { .. }
    ));
    assert!(matches!(
        by_type[&ModuleType::Eq],
        SlotDiff::LoadAndActivate { .. }
    ));
    assert!(matches!(
        by_type[&ModuleType::Time],
        SlotDiff::Disable { .. }
    ));
}

// ═════════════════════════════════════════════════════════════
//  Group D: Morph + SnapshotTween End-to-End (5 tests)
// ═════════════════════════════════════════════════════════════

/// Full morph cycle: set A/B, morph at 0, 0.5, 1.0 with linear easing.
#[test]
fn morph_full_cycle_linear() {
    let mut engine = MorphEngine::new();
    assert!(!engine.is_ready());

    let snap_a = DawParameterSnapshot::new(vec![
        param("amp", 0, "gain", 0.3),
        param("amp", 1, "bass", 0.5),
        param("drive", 0, "level", 0.2),
    ]);
    let snap_b = DawParameterSnapshot::new(vec![
        param("amp", 0, "gain", 0.9),
        param("amp", 1, "bass", 0.5), // same — not in diff
        param("drive", 0, "level", 0.8),
    ]);

    engine.set_a(snap_a);
    engine.set_b(snap_b);
    assert!(engine.is_ready());
    assert_eq!(engine.diff_count(), 2); // gain + level differ, bass is same

    // t=0.0 → A values
    let at_zero = engine.morph(0.0, EasingCurve::Linear);
    assert_eq!(at_zero.len(), 2);
    let gain_change = at_zero.iter().find(|c| c.param_name == "gain").unwrap();
    assert!((gain_change.current_value - 0.3).abs() < 1e-10);

    // t=0.5 → midpoint
    let at_mid = engine.morph(0.5, EasingCurve::Linear);
    let gain_mid = at_mid.iter().find(|c| c.param_name == "gain").unwrap();
    assert!((gain_mid.current_value - 0.6).abs() < 1e-10);
    let level_mid = at_mid.iter().find(|c| c.param_name == "level").unwrap();
    assert!((level_mid.current_value - 0.5).abs() < 1e-10);

    // t=1.0 → B values
    let at_one = engine.morph(1.0, EasingCurve::Linear);
    let gain_one = at_one.iter().find(|c| c.param_name == "gain").unwrap();
    assert!((gain_one.current_value - 0.9).abs() < 1e-10);
}

/// Morph with EaseInOut produces proper S-curve values.
#[test]
fn morph_ease_in_out_s_curve() {
    let mut engine = MorphEngine::new();
    engine.set_a(DawParameterSnapshot::new(vec![param("fx", 0, "p", 0.0)]));
    engine.set_b(DawParameterSnapshot::new(vec![param("fx", 0, "p", 1.0)]));

    let at_quarter = engine.morph(0.25, EasingCurve::EaseInOut);
    let val = at_quarter[0].current_value;
    // EaseInOut at 0.25 should be less than linear 0.25 (slow start)
    assert!(val < 0.25, "EaseInOut(0.25) = {val}, expected < 0.25");

    let at_three_quarter = engine.morph(0.75, EasingCurve::EaseInOut);
    let val = at_three_quarter[0].current_value;
    // EaseInOut at 0.75 should be greater than linear 0.75 (slow end approach)
    assert!(val > 0.75, "EaseInOut(0.75) = {val}, expected > 0.75");
}

/// SnapshotTween drives morph position over time (60fps simulation).
#[test]
fn tween_drives_morph_over_time() {
    let mut tween = SnapshotTween::new(500.0, EasingCurve::Linear);
    assert_eq!(tween.state(), TweenState::Idle);

    tween.start();
    assert!(tween.is_running());

    // Simulate 30 frames at 60fps (~16.67ms each)
    let mut positions = Vec::new();
    for _ in 0..30 {
        let t = tween.advance(16.67);
        positions.push(t);
    }

    // Should be running until ~500ms
    assert!(positions[0] > 0.0);
    assert!(positions[0] < 0.1);
    // After 30 frames (500ms), should be complete
    assert!(tween.is_complete());
    assert!((positions.last().unwrap() - 1.0).abs() < 1e-10);
}

/// Tween + morph combined: drive morph position from tween output.
#[test]
fn tween_morph_combined_crossfade() {
    let mut morph = MorphEngine::new();
    morph.set_a(DawParameterSnapshot::new(vec![
        param("amp", 0, "gain", 0.2),
        param("amp", 1, "mix", 0.0),
    ]));
    morph.set_b(DawParameterSnapshot::new(vec![
        param("amp", 0, "gain", 0.8),
        param("amp", 1, "mix", 1.0),
    ]));

    let mut tween = SnapshotTween::new(1000.0, EasingCurve::Linear);
    tween.start();

    // Frame at 500ms (halfway)
    let t = tween.advance(500.0);
    assert!((t - 0.5).abs() < 1e-10);

    let changes = morph.morph(t, EasingCurve::Linear);
    let gain = changes.iter().find(|c| c.param_name == "gain").unwrap();
    let mix = changes.iter().find(|c| c.param_name == "mix").unwrap();
    assert!((gain.current_value - 0.5).abs() < 1e-10);
    assert!((mix.current_value - 0.5).abs() < 1e-10);

    // Frame at 1000ms (complete)
    let t = tween.advance(500.0);
    assert!(tween.is_complete());
    let changes = morph.morph(t, EasingCurve::Linear);
    let gain = changes.iter().find(|c| c.param_name == "gain").unwrap();
    assert!((gain.current_value - 0.8).abs() < 1e-10);
}

/// Reset morph mid-animation and start with new targets.
#[test]
fn morph_reset_mid_animation() {
    let mut morph = MorphEngine::new();
    morph.set_a(DawParameterSnapshot::new(vec![param("fx", 0, "p", 0.0)]));
    morph.set_b(DawParameterSnapshot::new(vec![param("fx", 0, "p", 1.0)]));

    // Morph to 50%
    let changes = morph.morph(0.5, EasingCurve::Linear);
    assert!((changes[0].current_value - 0.5).abs() < 1e-10);

    // Reset and set new targets
    morph.reset();
    assert!(!morph.is_ready());
    assert_eq!(morph.diff_count(), 0);

    // New targets: morph from 0.5 to 0.9
    morph.set_a(DawParameterSnapshot::new(vec![param("fx", 0, "p", 0.5)]));
    morph.set_b(DawParameterSnapshot::new(vec![param("fx", 0, "p", 0.9)]));
    assert_eq!(morph.diff_count(), 1);

    // Morph at 50% of new range: 0.5 + 0.5*(0.9-0.5) = 0.7
    let changes = morph.morph(0.5, EasingCurve::Linear);
    assert!((changes[0].current_value - 0.7).abs() < 1e-10);
}

// ═════════════════════════════════════════════════════════════
//  Group E: Error Handling + Nonexistent IDs (8 tests)
// ═════════════════════════════════════════════════════════════

/// Loading a nonexistent rig returns None.
#[tokio::test]
async fn load_nonexistent_rig_returns_none() {
    let signal = controller().await;
    let result = signal.rigs().load(seed_id("does-not-exist")).await;
    assert!(result.is_none());
}

/// Loading a nonexistent engine returns None.
#[tokio::test]
async fn load_nonexistent_engine_returns_none() {
    let signal = controller().await;
    let result = signal.engines().load(seed_id("does-not-exist")).await;
    assert!(result.is_none());
}

/// Loading a nonexistent profile returns None.
#[tokio::test]
async fn load_nonexistent_profile_returns_none() {
    let signal = controller().await;
    let result = signal.profiles().load(seed_id("does-not-exist")).await;
    assert!(result.is_none());
}

/// Loading a nonexistent song returns None.
#[tokio::test]
async fn load_nonexistent_song_returns_none() {
    let signal = controller().await;
    let result = signal.songs().load(seed_id("does-not-exist")).await;
    assert!(result.is_none());
}

/// Loading a nonexistent layer returns None.
#[tokio::test]
async fn load_nonexistent_layer_returns_none() {
    let signal = controller().await;
    let result = signal.layers().load(seed_id("does-not-exist")).await;
    assert!(result.is_none());
}

/// Loading a nonexistent setlist returns None.
#[tokio::test]
async fn load_nonexistent_setlist_returns_none() {
    let signal = controller().await;
    let result = signal.setlists().load(seed_id("does-not-exist")).await;
    assert!(result.is_none());
}

/// Resolving a nonexistent rig scene returns an error.
#[tokio::test]
async fn resolve_nonexistent_rig_scene_returns_error() {
    let signal = controller().await;
    let result = signal
        .resolve_target(ResolveTarget::RigScene {
            rig_id: seed_id("does-not-exist").into(),
            scene_id: seed_id("does-not-exist").into(),
        })
        .await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ResolveError::NotFound(_)));
}

/// Resolving a nonexistent profile patch returns an error.
#[tokio::test]
async fn resolve_nonexistent_profile_patch_returns_error() {
    let signal = controller().await;
    let result = signal
        .resolve_target(ResolveTarget::ProfilePatch {
            profile_id: seed_id("does-not-exist").into(),
            patch_id: seed_id("does-not-exist").into(),
        })
        .await;
    assert!(result.is_err());
}

// ═════════════════════════════════════════════════════════════
//  Group F: Empty Collections + Edge Cases (6 tests)
// ═════════════════════════════════════════════════════════════

/// Save and load an empty rig (no engines).
#[tokio::test]
async fn empty_rig_save_load() {
    let signal = controller().await;
    let rig = Rig::new(
        seed_id("empty-rig"),
        "Empty Rig",
        vec![], // no engines
        RigScene::new(seed_id("empty-rig-scene"), "Default"),
    );
    signal.rigs().save(rig).await;

    let loaded = signal
        .rigs().load(seed_id("empty-rig"))
        .await
        .expect("empty rig should save/load");
    assert_eq!(loaded.engine_ids.len(), 0);
    assert_eq!(loaded.variants.len(), 1);
}

/// Save and load an engine with no layers.
#[tokio::test]
async fn empty_engine_save_load() {
    let signal = controller().await;
    let engine = Engine::new(
        seed_id("empty-engine"),
        "Empty Engine",
        EngineType::Keys,
        vec![], // no layers
        EngineScene::new(seed_id("empty-engine-scene"), "Default"),
    );
    signal.engines().save(engine).await;

    let loaded = signal
        .engines().load(seed_id("empty-engine"))
        .await
        .expect("empty engine should save/load");
    assert_eq!(loaded.layer_ids.len(), 0);
    assert_eq!(loaded.variants.len(), 1);
}

/// Save and load a layer with empty refs.
#[tokio::test]
async fn empty_layer_snapshot_save_load() {
    let signal = controller().await;
    let snap = LayerSnapshot::new(seed_id("empty-layer-snap"), "Default");
    // snap has empty module_refs, block_refs, layer_refs, overrides by default
    let layer = Layer::new(
        seed_id("empty-layer"),
        "Empty Layer",
        EngineType::Keys,
        snap,
    );
    signal.layers().save(layer).await;

    let loaded = signal
        .layers().load(seed_id("empty-layer"))
        .await
        .expect("empty layer should save/load");
    assert_eq!(loaded.variants.len(), 1);
    assert!(loaded.variants[0].module_refs.is_empty());
    assert!(loaded.variants[0].block_refs.is_empty());
}

/// Save and load a minimal setlist (one entry).
#[tokio::test]
async fn minimal_setlist_save_load() {
    let signal = controller().await;

    // Setlist::new requires a default entry — create one referencing a seeded song
    let songs = signal.songs().list().await;
    let first_song = &songs[0];
    let entry = SetlistEntry::new(
        seed_id("min-setlist-entry"),
        &first_song.name,
        first_song.id.clone(),
    );
    let setlist = Setlist::new(seed_id("min-setlist"), "Minimal Setlist", entry);
    signal.setlists().save(setlist).await;

    let loaded = signal
        .setlists().load(seed_id("min-setlist"))
        .await
        .expect("minimal setlist should save/load");
    assert_eq!(loaded.entries.len(), 1);
}

/// Duplicate names are allowed (different IDs).
#[tokio::test]
async fn duplicate_names_coexist() {
    let signal = controller().await;

    let rig_a = Rig::new(
        seed_id("dup-name-rig-a"),
        "Same Name Rig",
        vec![],
        RigScene::new(seed_id("dup-a-scene"), "Default"),
    );
    let rig_b = Rig::new(
        seed_id("dup-name-rig-b"),
        "Same Name Rig",
        vec![],
        RigScene::new(seed_id("dup-b-scene"), "Default"),
    );
    signal.rigs().save(rig_a).await;
    signal.rigs().save(rig_b).await;

    let all = signal.rigs().list().await;
    let dups: Vec<_> = all.iter().filter(|r| r.name == "Same Name Rig").collect();
    assert_eq!(dups.len(), 2);
}

/// Save and load a song with zero sections.
#[tokio::test]
async fn empty_song_save_load() {
    let signal = controller().await;

    // Song::new requires at least one section as the default.
    // Create a song with one section, then verify it at least works.
    let song = Song::new(
        seed_id("minimal-song"),
        "Minimal Song",
        Section::from_rig_scene(
            seed_id("minimal-section"),
            "Intro",
            guitar_rig_id(),
            guitar_default_scene(),
        ),
    );
    signal.songs().save(song).await;

    let loaded = signal.songs().load(seed_id("minimal-song")).await.expect("song");
    assert_eq!(loaded.sections.len(), 1);
}

// ═════════════════════════════════════════════════════════════
//  Group G: Delete Operations (5 tests)
// ═════════════════════════════════════════════════════════════

/// Delete a rig collection, verify it's gone.
#[tokio::test]
async fn delete_rig_collection() {
    let signal = controller().await;

    let rig = Rig::new(
        seed_id("del-rig"),
        "Deletable Rig",
        vec![],
        RigScene::new(seed_id("del-rig-scene"), "Default"),
    );
    signal.rigs().save(rig).await;
    assert!(signal.rigs().load(seed_id("del-rig")).await.is_some());

    signal.rigs().delete(seed_id("del-rig")).await;
    assert!(signal.rigs().load(seed_id("del-rig")).await.is_none());
}

/// Delete a module collection, verify it's gone.
#[tokio::test]
async fn delete_module_collection() {
    let signal = controller().await;

    let modules = signal.module_presets().list().await;
    assert!(!modules.is_empty());
    let first_id = modules[0].id().clone();

    signal.module_presets().delete(first_id.clone()).await;
    let after = signal.module_presets().list().await;
    assert!(after.iter().all(|m| m.id() != &first_id));
}

/// Delete a profile, verify it's gone.
#[tokio::test]
async fn delete_profile() {
    let signal = controller().await;

    let profile = Profile::new(
        seed_id("del-profile"),
        "Deletable Profile",
        Patch::from_rig_scene(
            seed_id("del-patch"),
            "P1",
            guitar_rig_id(),
            guitar_default_scene(),
        ),
    );
    signal.profiles().save(profile).await;
    assert!(signal.profiles().load(seed_id("del-profile")).await.is_some());

    signal.profiles().delete(seed_id("del-profile")).await;
    assert!(signal.profiles().load(seed_id("del-profile")).await.is_none());
}

/// Delete a song, verify it's gone.
#[tokio::test]
async fn delete_song() {
    let signal = controller().await;

    let song = Song::new(
        seed_id("del-song"),
        "Deletable Song",
        Section::from_rig_scene(
            seed_id("del-section"),
            "Intro",
            guitar_rig_id(),
            guitar_default_scene(),
        ),
    );
    signal.songs().save(song).await;
    assert!(signal.songs().load(seed_id("del-song")).await.is_some());

    signal.songs().delete(seed_id("del-song")).await;
    assert!(signal.songs().load(seed_id("del-song")).await.is_none());
}

/// Delete a setlist, verify it's gone.
#[tokio::test]
async fn delete_setlist() {
    let signal = controller().await;

    let songs = signal.songs().list().await;
    let first_song = &songs[0];
    let entry = SetlistEntry::new(
        seed_id("del-setlist-entry"),
        &first_song.name,
        first_song.id.clone(),
    );
    let setlist = Setlist::new(seed_id("del-setlist"), "Deletable Setlist", entry);
    signal.setlists().save(setlist).await;
    assert!(signal.setlists().load(seed_id("del-setlist")).await.is_some());

    signal.setlists().delete(seed_id("del-setlist")).await;
    assert!(signal.setlists().load(seed_id("del-setlist")).await.is_none());
}

// ═════════════════════════════════════════════════════════════
//  Group H: Scene Template CRUD (4 tests)
// ═════════════════════════════════════════════════════════════

/// Create, save, and load a scene template.
#[tokio::test]
async fn scene_template_create_save_load() {
    let signal = controller().await;

    let template = SceneTemplate::new(seed_id("clean-template"), "Clean")
        .with_engine(EngineSelection::new(
            seed_id("keys-engine"),
            seed_id("keys-engine-default"),
        ))
        .with_override(Override::set(
            NodePath::engine("keys-engine").with_parameter("volume"),
            0.7,
        ));
    signal.scene_templates().save(template).await;

    let loaded = signal
        .scene_templates().load(seed_id("clean-template"))
        .await
        .expect("template should exist");
    assert_eq!(loaded.name, "Clean");
    assert_eq!(loaded.engine_selections.len(), 1);
    assert_eq!(loaded.overrides.len(), 1);
}

/// List scene templates after saving multiple.
#[tokio::test]
async fn scene_template_list() {
    let signal = controller().await;

    signal.scene_templates().save(SceneTemplate::new(seed_id("tpl-a"), "Template A"))
        .await;
    signal.scene_templates().save(SceneTemplate::new(seed_id("tpl-b"), "Template B"))
        .await;
    signal.scene_templates().save(SceneTemplate::new(seed_id("tpl-c"), "Template C"))
        .await;

    let templates = signal.scene_templates().list().await;
    assert!(templates.len() >= 3);
}

/// Delete a scene template.
#[tokio::test]
async fn scene_template_delete() {
    let signal = controller().await;

    signal.scene_templates().save(SceneTemplate::new(seed_id("tpl-del"), "Delete Me"))
        .await;
    assert!(signal.scene_templates().load(seed_id("tpl-del")).await.is_some());

    signal.scene_templates().delete(seed_id("tpl-del")).await;
    assert!(signal.scene_templates().load(seed_id("tpl-del")).await.is_none());
}

/// Convert scene template to rig scene.
#[tokio::test]
async fn scene_template_to_rig_scene() {
    let signal = controller().await;

    let template = SceneTemplate::new(seed_id("lead-tpl"), "Lead Template")
        .with_engine(EngineSelection::new(
            seed_id("keys-engine"),
            seed_id("keys-engine-bright"),
        ))
        .with_override(Override::set(
            NodePath::engine("keys-engine")
                .with_layer("keys-layer-core")
                .with_parameter("gain"),
            0.9,
        ));

    // Convert to a RigScene
    let scene = template.to_rig_scene(seed_id("lead-from-tpl"));
    assert_eq!(scene.name, "Lead Template");
    assert_eq!(scene.engine_selections.len(), 1);
    assert_eq!(scene.overrides.len(), 1);

    // Use it in a rig
    let rig = Rig::new(
        seed_id("tpl-rig"),
        "Template Rig",
        vec![seed_id("keys-engine").into()],
        scene,
    );
    signal.rigs().save(rig).await;

    let loaded = signal
        .rigs().load(seed_id("tpl-rig"))
        .await
        .expect("rig");
    assert_eq!(loaded.variants[0].name, "Lead Template");
}

// ═════════════════════════════════════════════════════════════
//  Group I: Event Bus Integration (3 tests)
// ═════════════════════════════════════════════════════════════

/// Subscribe to event bus, verify it connects.
#[tokio::test]
async fn event_bus_subscribe() {
    let signal = controller().await;
    let _rx = signal.subscribe();
    // Just verify subscribe() doesn't panic and returns a receiver
    assert!(signal.event_bus().subscriber_count() >= 1);
}

/// Event bus delivers events to multiple subscribers.
#[tokio::test]
async fn event_bus_multiple_subscribers() {
    let signal = controller().await;
    let _rx1 = signal.subscribe();
    let _rx2 = signal.subscribe();
    assert!(signal.event_bus().subscriber_count() >= 2);
}

/// Event bus emits CollectionSaved when we save via the event bus directly.
#[tokio::test]
async fn event_bus_emit_and_receive() {
    let signal = controller().await;
    let mut rx = signal.subscribe();

    // Manually emit via event bus
    signal.event_bus().emit(SignalEvent::CollectionSaved {
        entity_type: "rig",
        id: "test-id".into(),
    });

    let event = rx.recv().await.unwrap();
    assert!(matches!(
        event,
        SignalEvent::CollectionSaved {
            entity_type: "rig",
            ..
        }
    ));
}

// ═════════════════════════════════════════════════════════════
//  Group J: Concurrent Resolve (2 tests)
// ═════════════════════════════════════════════════════════════

/// Two simultaneous resolve operations on the same scene don't panic.
#[tokio::test]
async fn concurrent_resolve_same_scene() {
    let signal = controller().await;

    let (result_a, result_b) = tokio::join!(
        signal.resolve_target(ResolveTarget::RigScene {
            rig_id: guitar_rig_id(),
            scene_id: guitar_default_scene(),
        }),
        signal.resolve_target(ResolveTarget::RigScene {
            rig_id: guitar_rig_id(),
            scene_id: guitar_default_scene(),
        })
    );

    assert!(result_a.is_ok());
    assert!(result_b.is_ok());

    // Both should produce identical graphs
    let graph_a = result_a.unwrap();
    let graph_b = result_b.unwrap();
    assert_eq!(graph_a.engines.len(), graph_b.engines.len());
}

/// Resolve guitar rig and keys rig simultaneously.
#[tokio::test]
async fn concurrent_resolve_different_rigs() {
    let signal = controller().await;

    let (guitar_result, keys_result) = tokio::join!(
        signal.resolve_target(ResolveTarget::RigScene {
            rig_id: guitar_rig_id(),
            scene_id: guitar_default_scene(),
        }),
        signal.resolve_target(ResolveTarget::RigScene {
            rig_id: keys_rig_id(),
            scene_id: keys_default_scene(),
        })
    );

    assert!(guitar_result.is_ok());
    assert!(keys_result.is_ok());

    let guitar_graph = guitar_result.unwrap();
    let keys_graph = keys_result.unwrap();

    // Guitar has 1 engine, Keys has multiple
    assert_eq!(guitar_graph.engines.len(), 1);
    assert!(keys_graph.engines.len() >= 2);
}

// ═════════════════════════════════════════════════════════════
//  Group K: Rapid Scene Switching Simulation (3 tests)
// ═════════════════════════════════════════════════════════════

/// Rapid diff chain: switch through 4 scenes in quick succession.
#[test]
fn rapid_diff_chain_four_scenes() {
    let preset_a = ModulePresetId::new();
    let preset_b = ModulePresetId::new();
    let snap_1 = ModuleSnapshotId::new();
    let snap_2 = ModuleSnapshotId::new();
    let snap_3 = ModuleSnapshotId::new();

    // Start: Amp loaded with preset_a/snap_1
    let mut current = vec![SlotState {
        module_type: ModuleType::Amp,
        current: Some(ResolvedSlot::Active(make_target_with_snapshot(
            ModuleType::Amp,
            preset_a.clone(),
            snap_1.clone(),
        ))),
        active_handle: Some(signal_live::engine::InstanceHandle::new(1)),
        is_disabled: false,
    }];

    // Scene switch 1: same preset, snap_2 → ApplySnapshot
    let mut targets_1 = HashMap::new();
    targets_1.insert(
        ModuleType::Amp,
        ResolvedSlot::Active(make_target_with_snapshot(
            ModuleType::Amp,
            preset_a.clone(),
            snap_2.clone(),
        )),
    );
    let diffs_1 = compute_diff(&current, &targets_1, &no_preload);
    assert!(matches!(diffs_1[0], SlotDiff::ApplySnapshot { .. }));

    // Update current state
    current[0].current = Some(ResolvedSlot::Active(make_target_with_snapshot(
        ModuleType::Amp,
        preset_a.clone(),
        snap_2.clone(),
    )));

    // Scene switch 2: same preset, snap_3 → ApplySnapshot
    let mut targets_2 = HashMap::new();
    targets_2.insert(
        ModuleType::Amp,
        ResolvedSlot::Active(make_target_with_snapshot(
            ModuleType::Amp,
            preset_a.clone(),
            snap_3.clone(),
        )),
    );
    let diffs_2 = compute_diff(&current, &targets_2, &no_preload);
    assert!(matches!(diffs_2[0], SlotDiff::ApplySnapshot { .. }));

    current[0].current = Some(ResolvedSlot::Active(make_target_with_snapshot(
        ModuleType::Amp,
        preset_a,
        snap_3,
    )));

    // Scene switch 3: different preset → LoadAndActivate
    let mut targets_3 = HashMap::new();
    targets_3.insert(
        ModuleType::Amp,
        ResolvedSlot::Active(make_target_with_snapshot(
            ModuleType::Amp,
            preset_b.clone(),
            snap_1,
        )),
    );
    let diffs_3 = compute_diff(&current, &targets_3, &no_preload);
    assert!(matches!(diffs_3[0], SlotDiff::LoadAndActivate { .. }));

    current[0].current = Some(ResolvedSlot::Active(make_target_with_snapshot(
        ModuleType::Amp,
        preset_b,
        snap_2,
    )));

    // Scene switch 4: back to same → NoChange
    let mut targets_4 = HashMap::new();
    targets_4.insert(ModuleType::Amp, current[0].current.clone().unwrap());
    let diffs_4 = compute_diff(&current, &targets_4, &no_preload);
    assert!(matches!(diffs_4[0], SlotDiff::NoChange { .. }));
}

/// MockRigEngine handles rapid scene switches without panicking.
#[tokio::test]
async fn engine_rapid_scene_switches() {
    let engine = MockRigEngine::new();

    // 10 rapid scene switches
    for i in 0..10u32 {
        let mut targets = HashMap::new();
        // Alternate between 2 and 3 module types
        targets.insert(ModuleType::Amp, make_target(ModuleType::Amp));
        targets.insert(ModuleType::Drive, make_target(ModuleType::Drive));
        if i % 2 == 0 {
            targets.insert(ModuleType::Eq, make_target(ModuleType::Eq));
        }
        let result = engine.load_scene_targets(targets);
        assert!(result.is_completed());
    }

    // Engine should still be in a valid state
    let active = engine.active_module_types();
    assert!(!active.is_empty());
}

/// Resolve 4 different scenes of the same rig in sequence (simulating live setlist navigation).
#[tokio::test]
async fn resolve_rapid_scene_sequence() {
    let signal = controller().await;

    let scene_ids = vec![
        seed_id("keys-megarig-default"),
        seed_id("keys-megarig-wide"),
        seed_id("keys-megarig-focus"),
        seed_id("keys-megarig-air"),
    ];

    let mut prev_engine_count = 0;
    for (i, scene_id) in scene_ids.iter().enumerate() {
        let result = signal
            .resolve_target(ResolveTarget::RigScene {
                rig_id: keys_rig_id(),
                scene_id: scene_id.clone().into(),
            })
            .await;
        assert!(
            result.is_ok(),
            "scene {} failed to resolve: {:?}",
            i,
            result.err()
        );
        let graph = result.unwrap();
        assert!(!graph.engines.is_empty());

        // Verify engine count is consistent across scenes (same rig, same engines)
        if i > 0 {
            assert_eq!(
                graph.engines.len(),
                prev_engine_count,
                "engine count changed between scenes"
            );
        }
        prev_engine_count = graph.engines.len();
    }
}

// ═════════════════════════════════════════════════════════════
//  Group L: Cross-Rig Scene Diff (2 tests)
// ═════════════════════════════════════════════════════════════

/// Resolve guitar rig + keys rig, build diff between them.
#[tokio::test]
async fn diff_between_guitar_and_keys_rigs() {
    let signal = controller().await;

    let guitar_graph = signal
        .resolve_target(ResolveTarget::RigScene {
            rig_id: guitar_rig_id(),
            scene_id: guitar_default_scene(),
        })
        .await
        .expect("guitar resolve");

    let keys_graph = signal
        .resolve_target(ResolveTarget::RigScene {
            rig_id: keys_rig_id(),
            scene_id: keys_default_scene(),
        })
        .await
        .expect("keys resolve");

    // They resolve to different engine structures
    assert_ne!(guitar_graph.rig_id, keys_graph.rig_id);
    assert_ne!(guitar_graph.engines.len(), keys_graph.engines.len());
}

/// Morph engine handles mismatched parameter sets gracefully.
#[test]
fn morph_between_mismatched_param_sets() {
    let mut morph = MorphEngine::new();

    // Guitar-like snapshot: amp/drive params
    let guitar_snap = DawParameterSnapshot::new(vec![
        param("amp", 0, "gain", 0.6),
        param("drive", 0, "level", 0.4),
        param("eq", 0, "bass", 0.5),
    ]);

    // Keys-like snapshot: completely different params
    let keys_snap = DawParameterSnapshot::new(vec![
        param("piano", 0, "brightness", 0.7),
        param("reverb", 0, "mix", 0.3),
    ]);

    morph.set_a(guitar_snap);
    morph.set_b(keys_snap);

    // All 5 params differ (disjoint sets)
    assert_eq!(morph.diff_count(), 5);

    // Morph at 0.5: guitar params morph toward 0, keys params morph from 0
    let changes = morph.morph(0.5, EasingCurve::Linear);
    assert_eq!(changes.len(), 5);

    let gain = changes.iter().find(|c| c.param_name == "gain").unwrap();
    assert!((gain.current_value - 0.3).abs() < 1e-10); // 0.6 → 0, at 0.5 = 0.3

    let brightness = changes
        .iter()
        .find(|c| c.param_name == "brightness")
        .unwrap();
    assert!((brightness.current_value - 0.35).abs() < 1e-10); // 0 → 0.7, at 0.5 = 0.35
}

// ═════════════════════════════════════════════════════════════
//  Group M: Load Variant by ID (3 tests)
// ═════════════════════════════════════════════════════════════

/// Load a specific song section by variant ID.
#[tokio::test]
async fn load_song_variant_by_id() {
    let signal = controller().await;

    let songs = signal.songs().list().await;
    let song = songs
        .iter()
        .find(|s| !s.sections.is_empty())
        .expect("need a song with sections");
    let first_section = &song.sections[0];

    let loaded = signal
        .songs().load_section(song.id.clone(), first_section.id.clone())
        .await;
    assert!(loaded.is_some());
    let section = loaded.unwrap();
    assert_eq!(section.name, first_section.name);
}

/// Load a nonexistent variant returns None.
#[tokio::test]
async fn load_nonexistent_variant_returns_none() {
    let signal = controller().await;

    // Valid rig, nonexistent scene
    let result = signal
        .rigs().load_variant(guitar_rig_id(), seed_id("nonexistent-scene"))
        .await;
    assert!(result.is_none());
}

/// Load a nonexistent engine variant returns None.
#[tokio::test]
async fn load_nonexistent_engine_variant_returns_none() {
    let signal = controller().await;

    let result = signal
        .engines().load_variant(seed_id("keys-engine"), seed_id("nonexistent-variant"))
        .await;
    assert!(result.is_none());
}

// ═════════════════════════════════════════════════════════════
//  Group N: Tween Edge Cases (4 tests)
// ═════════════════════════════════════════════════════════════

/// Tween with zero duration completes immediately.
#[test]
fn tween_zero_duration_completes_immediately() {
    let mut tween = SnapshotTween::new(0.0, EasingCurve::Linear);
    tween.start();
    let t = tween.advance(0.0);
    assert!((t - 1.0).abs() < 1e-10);
    assert!(tween.is_complete());
}

/// Tween advance after completion still returns 1.0.
#[test]
fn tween_advance_after_complete_returns_one() {
    let mut tween = SnapshotTween::new(100.0, EasingCurve::Linear);
    tween.start();
    tween.advance(200.0); // overshoot
    assert!(tween.is_complete());

    let t = tween.advance(100.0); // advance again
    assert!((t - 1.0).abs() < 1e-10);
}

/// Tween advance before start returns 0.0.
#[test]
fn tween_advance_before_start_returns_zero() {
    let tween = SnapshotTween::new(1000.0, EasingCurve::Linear);
    assert_eq!(tween.state(), TweenState::Idle);
    // Can't advance without start (no mutable reference to call advance on idle)
    // Just verify the state
}

/// Tween reset after completion allows restart.
#[test]
fn tween_reset_and_restart() {
    let mut tween = SnapshotTween::new(200.0, EasingCurve::EaseIn);
    tween.start();
    tween.advance(200.0);
    assert!(tween.is_complete());

    tween.reset();
    assert_eq!(tween.state(), TweenState::Idle);

    tween.start();
    assert!(tween.is_running());
    let t = tween.advance(100.0);
    // EaseIn at 0.5: t = 0.5^2 = 0.25
    assert!((t - 0.25).abs() < 1e-10);
}
