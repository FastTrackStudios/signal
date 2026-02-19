//! Comprehensive tests for the Rack + Director API.
//!
//! Tests exercise the full vocal rack lifecycle: creating racks with multiple
//! rig slots, slot management, active slot switching, FX send buses, and
//! integration with the existing rig/engine hierarchy.
//!
//! All tests use `bootstrap_in_memory_controller_async()` — no REAPER dependency.

use signal::{
    bootstrap_in_memory_controller_async,
    fx_send::{FxSend, FxSendBus, FxSendBusId, FxSendCategory, FxSendId},
    rack::{Rack, RackId, RackSlot},
    rig::{Rig, RigId, RigScene},
    seed_id, BlockType,
};

// ─── Helpers ─────────────────────────────────────────────────────

fn rkid(name: &str) -> RackId {
    RackId::from_uuid(seed_id(name))
}

fn rid(name: &str) -> RigId {
    RigId::from_uuid(seed_id(name))
}

/// Guitar megarig from seed data.
fn guitar_rig_id() -> RigId {
    RigId::from_uuid(seed_id("guitar-megarig"))
}

/// Keys megarig from seed data.
fn keys_rig_id() -> RigId {
    RigId::from_uuid(seed_id("keys-megarig"))
}

/// Create a minimal rig for testing (not from seed data).
fn test_rig(name: &str) -> Rig {
    let scene = RigScene::new(seed_id(&format!("{name}-scene-default")), "Default");
    Rig::new(seed_id(name), name, vec![], scene)
}

/// Create a vocal rack with two rig slots (lead + harmony).
fn vocal_rack() -> Rack {
    let mut rack = Rack::new(seed_id("vocal-rack"), "Vocal Rack");
    rack.slots.push(RackSlot {
        position: 0,
        rig_id: rid("vox-lead-rig"),
        active: true,
    });
    rack.slots.push(RackSlot {
        position: 1,
        rig_id: rid("vox-harmony-rig"),
        active: true,
    });
    rack.active_slot = Some(0);
    rack
}

// ═══════════════════════════════════════════════════════════════════
// Group A: Basic Rack CRUD (5 tests)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a1_create_and_load_rack() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    let rack = vocal_rack();
    signal.racks().save(rack.clone()).await;

    let loaded = signal.racks().load(rkid("vocal-rack")).await;
    let loaded = loaded.expect("should find saved rack");
    assert_eq!(loaded.name, "Vocal Rack");
    assert_eq!(loaded.slots.len(), 2);
    assert_eq!(loaded.active_slot, Some(0));
}

#[tokio::test]
async fn a2_list_racks_empty_initially() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    let racks = signal.racks().list().await;
    assert!(racks.is_empty(), "no racks should be seeded by default");
}

#[tokio::test]
async fn a3_list_racks_returns_saved() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    signal.racks().save(Rack::new(seed_id("r1"), "Rack A")).await;
    signal.racks().save(Rack::new(seed_id("r2"), "Rack B")).await;
    signal.racks().save(Rack::new(seed_id("r3"), "Rack C")).await;

    let racks = signal.racks().list().await;
    assert_eq!(racks.len(), 3);
}

#[tokio::test]
async fn a4_delete_rack() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    signal.racks().save(vocal_rack()).await;
    signal.racks().delete(rkid("vocal-rack")).await;

    let loaded = signal.racks().load(rkid("vocal-rack")).await;
    assert!(loaded.is_none(), "rack should be deleted");
}

#[tokio::test]
async fn a5_load_nonexistent_returns_none() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    let loaded = signal.racks().load(rkid("does-not-exist")).await;
    assert!(loaded.is_none());
}

// ═══════════════════════════════════════════════════════════════════
// Group B: Rack Slot Management (6 tests)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn b1_rack_with_single_slot() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    let mut rack = Rack::new(seed_id("guitar-rack"), "Guitar Rack");
    rack.slots.push(RackSlot {
        position: 0,
        rig_id: guitar_rig_id(),
        active: true,
    });
    rack.active_slot = Some(0);
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("guitar-rack")).await.unwrap();
    assert_eq!(loaded.slots.len(), 1);
    assert_eq!(loaded.active_rig_id(), Some(&guitar_rig_id()));
}

#[tokio::test]
async fn b2_rack_with_multiple_slots() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    let mut rack = Rack::new(seed_id("multi-rack"), "Multi Rack");
    for i in 0..4 {
        rack.slots.push(RackSlot {
            position: i,
            rig_id: rid(&format!("rig-slot-{i}")),
            active: i < 2, // first two active, last two inactive
        });
    }
    rack.active_slot = Some(1);
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("multi-rack")).await.unwrap();
    assert_eq!(loaded.slots.len(), 4);
    assert_eq!(loaded.active_slot, Some(1));
    // Active rig is slot 1 (which is active=true)
    assert_eq!(loaded.active_rig_id(), Some(&rid("rig-slot-1")));
}

#[tokio::test]
async fn b3_add_slot_to_existing_rack() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    signal.racks().save(vocal_rack()).await;

    // Load, add a third slot, save back
    let mut rack = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    rack.slots.push(RackSlot {
        position: 2,
        rig_id: rid("vox-background-rig"),
        active: false,
    });
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    assert_eq!(loaded.slots.len(), 3);
    assert!(!loaded.slots[2].active);
}

#[tokio::test]
async fn b4_remove_slot_from_rack() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    signal.racks().save(vocal_rack()).await;

    let mut rack = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    rack.slots.retain(|s| s.position != 1); // Remove harmony slot
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    assert_eq!(loaded.slots.len(), 1);
    assert_eq!(loaded.slots[0].rig_id, rid("vox-lead-rig"));
}

#[tokio::test]
async fn b5_switch_active_slot() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    signal.racks().save(vocal_rack()).await;

    let mut rack = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    assert_eq!(rack.active_slot, Some(0));
    assert_eq!(rack.active_rig_id(), Some(&rid("vox-lead-rig")));

    // Switch to harmony slot
    rack.active_slot = Some(1);
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    assert_eq!(loaded.active_slot, Some(1));
    assert_eq!(loaded.active_rig_id(), Some(&rid("vox-harmony-rig")));
}

#[tokio::test]
async fn b6_deactivate_slot() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    signal.racks().save(vocal_rack()).await;

    let mut rack = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    // Deactivate the active slot — should return None for active rig
    rack.slots[0].active = false;
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    // Slot 0 is inactive, so even though active_slot=0, active_rig_id returns None
    assert!(loaded.active_rig_id().is_none());
}

// ═══════════════════════════════════════════════════════════════════
// Group C: FX Send Buses (4 tests)
// ═══════════════════════════════════════════════════════════════════

fn make_fx_send(name: &str, category: FxSendCategory, block_type: BlockType) -> FxSend {
    FxSend {
        id: FxSendId::new(),
        name: name.into(),
        category,
        block_type,
        enabled: true,
        mix: 0.5,
        track_ref: None,
    }
}

fn make_fx_bus(name: &str, sub_cat: Option<&str>, sends: Vec<FxSend>) -> FxSendBus {
    FxSendBus {
        id: FxSendBusId::new(),
        name: name.into(),
        sends,
        sub_category: sub_cat.map(String::from),
    }
}

#[tokio::test]
async fn c1_rack_with_fx_send_buses() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    let mut rack = vocal_rack();
    rack.fx_send_buses.push(make_fx_bus(
        "Vocal AUX",
        Some("AUX"),
        vec![
            make_fx_send("Reverb", FxSendCategory::Reverb, BlockType::Reverb),
            make_fx_send("Chorus", FxSendCategory::Chorus, BlockType::Chorus),
        ],
    ));
    rack.fx_send_buses.push(make_fx_bus(
        "Vocal TIME",
        Some("TIME"),
        vec![make_fx_send(
            "Delay",
            FxSendCategory::Delay,
            BlockType::Delay,
        )],
    ));
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    assert_eq!(loaded.fx_send_buses.len(), 2);
    assert_eq!(loaded.fx_send_buses[0].name, "Vocal AUX");
    assert_eq!(loaded.fx_send_buses[0].sends.len(), 2);
    assert_eq!(loaded.fx_send_buses[1].sends.len(), 1);
}

#[tokio::test]
async fn c2_fx_send_categories_round_trip() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    let mut rack = Rack::new(seed_id("fx-rack"), "FX Rack");
    rack.fx_send_buses.push(make_fx_bus(
        "Mixed",
        None,
        vec![
            make_fx_send("Reverb", FxSendCategory::Reverb, BlockType::Reverb),
            make_fx_send("Delay", FxSendCategory::Delay, BlockType::Delay),
            make_fx_send("Pitch", FxSendCategory::Pitch, BlockType::Pitch),
            make_fx_send(
                "Custom EQ",
                FxSendCategory::Custom("Custom EQ".into()),
                BlockType::Eq,
            ),
        ],
    ));
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("fx-rack")).await.unwrap();
    let sends = &loaded.fx_send_buses[0].sends;
    assert_eq!(sends.len(), 4);
    assert_eq!(sends[0].category, FxSendCategory::Reverb);
    assert_eq!(sends[1].category, FxSendCategory::Delay);
    assert_eq!(sends[2].category, FxSendCategory::Pitch);
    assert!(matches!(&sends[3].category, FxSendCategory::Custom(s) if s == "Custom EQ"));
}

#[tokio::test]
async fn c3_add_bus_to_existing_rack() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    signal.racks().save(vocal_rack()).await;

    let mut rack = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    assert!(rack.fx_send_buses.is_empty());

    rack.fx_send_buses.push(make_fx_bus(
        "New Bus",
        Some("AUX"),
        vec![make_fx_send(
            "Chorus",
            FxSendCategory::Chorus,
            BlockType::Chorus,
        )],
    ));
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    assert_eq!(loaded.fx_send_buses.len(), 1);
    assert_eq!(loaded.fx_send_buses[0].sends[0].name, "Chorus");
}

#[tokio::test]
async fn c4_fx_send_mix_and_enabled_round_trip() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    let mut rack = Rack::new(seed_id("mix-rack"), "Mix Rack");
    let mut send = make_fx_send("Low Send", FxSendCategory::Reverb, BlockType::Reverb);
    send.mix = 0.25;
    send.enabled = false;
    rack.fx_send_buses
        .push(make_fx_bus("Bus", None, vec![send]));
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("mix-rack")).await.unwrap();
    let send = &loaded.fx_send_buses[0].sends[0];
    assert!((send.mix - 0.25).abs() < f32::EPSILON);
    assert!(!send.enabled);
}

// ═══════════════════════════════════════════════════════════════════
// Group D: Rack with Seeded Rigs (4 tests)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn d1_rack_referencing_seeded_guitar_rig() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();

    // Verify the seeded rig exists
    let rig = signal.rigs().load(guitar_rig_id()).await;
    assert!(rig.is_some(), "guitar megarig should exist in seeds");

    let mut rack = Rack::new(seed_id("guitar-rack"), "Guitar Rack");
    rack.slots.push(RackSlot {
        position: 0,
        rig_id: guitar_rig_id(),
        active: true,
    });
    rack.active_slot = Some(0);
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("guitar-rack")).await.unwrap();
    assert_eq!(loaded.active_rig_id(), Some(&guitar_rig_id()));

    // Verify we can still load the rig referenced by the rack
    let rig = signal.rigs().load(guitar_rig_id()).await;
    assert!(rig.is_some());
}

#[tokio::test]
async fn d2_rack_referencing_seeded_keys_rig() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();

    let mut rack = Rack::new(seed_id("keys-rack"), "Keys Rack");
    rack.slots.push(RackSlot {
        position: 0,
        rig_id: keys_rig_id(),
        active: true,
    });
    rack.active_slot = Some(0);
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("keys-rack")).await.unwrap();
    assert_eq!(loaded.active_rig_id(), Some(&keys_rig_id()));
}

#[tokio::test]
async fn d3_multi_instrument_rack() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();

    // A rack with both guitar and keys rigs (unusual but valid)
    let mut rack = Rack::new(seed_id("multi-inst"), "Multi Instrument");
    rack.slots.push(RackSlot {
        position: 0,
        rig_id: guitar_rig_id(),
        active: true,
    });
    rack.slots.push(RackSlot {
        position: 1,
        rig_id: keys_rig_id(),
        active: true,
    });
    rack.active_slot = Some(0);
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("multi-inst")).await.unwrap();
    assert_eq!(loaded.slots.len(), 2);
    // Can switch between instruments
    assert_eq!(loaded.active_rig_id(), Some(&guitar_rig_id()));
}

#[tokio::test]
async fn d4_rack_slot_with_dynamically_created_rig() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();

    // Create a brand new rig, then reference it from a rack slot
    let rig = test_rig("vox-lead-rig");
    signal.rigs().save(rig).await;

    let mut rack = Rack::new(seed_id("dynamic-rack"), "Dynamic Rack");
    rack.slots.push(RackSlot {
        position: 0,
        rig_id: rid("vox-lead-rig"),
        active: true,
    });
    rack.active_slot = Some(0);
    signal.racks().save(rack).await;

    // Verify rack loads correctly
    let loaded = signal.racks().load(rkid("dynamic-rack")).await.unwrap();
    assert_eq!(loaded.active_rig_id(), Some(&rid("vox-lead-rig")));

    // Verify the rig itself is loadable
    let rig = signal.rigs().load(rid("vox-lead-rig")).await;
    assert!(rig.is_some());
}

// ═══════════════════════════════════════════════════════════════════
// Group E: Vocal Rack Workflow (5 tests)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn e1_vocal_rack_full_lifecycle() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();

    // Create lead vocal rig with scenes
    let lead_scene_clean = RigScene::new(seed_id("lead-clean"), "Clean");
    let lead_scene_heavy = RigScene::new(seed_id("lead-heavy"), "Heavy");
    let mut lead_rig = Rig::new(seed_id("vox-lead"), "Vocal Lead", vec![], lead_scene_clean);
    lead_rig.add_variant(lead_scene_heavy);
    signal.rigs().save(lead_rig).await;

    // Create harmony vocal rig
    let harmony_scene = RigScene::new(seed_id("harmony-default"), "Default");
    let harmony_rig = Rig::new(
        seed_id("vox-harmony"),
        "Vocal Harmony",
        vec![],
        harmony_scene,
    );
    signal.rigs().save(harmony_rig).await;

    // Build the vocal rack
    let mut rack = Rack::new(seed_id("vocal-live"), "Vocal Live Rack");
    rack.slots.push(RackSlot {
        position: 0,
        rig_id: rid("vox-lead"),
        active: true,
    });
    rack.slots.push(RackSlot {
        position: 1,
        rig_id: rid("vox-harmony"),
        active: true,
    });
    rack.active_slot = Some(0);
    rack.fx_send_buses.push(make_fx_bus(
        "Shared Reverb",
        Some("AUX"),
        vec![make_fx_send(
            "Hall Verb",
            FxSendCategory::Reverb,
            BlockType::Reverb,
        )],
    ));
    signal.racks().save(rack).await;

    // Verify the full structure
    let loaded = signal.racks().load(rkid("vocal-live")).await.unwrap();
    assert_eq!(loaded.slots.len(), 2);
    assert_eq!(loaded.fx_send_buses.len(), 1);

    // Both rigs are independently accessible
    let lead = signal.rigs().load(rid("vox-lead")).await.unwrap();
    assert_eq!(lead.variants.len(), 2);
    let harmony = signal.rigs().load(rid("vox-harmony")).await.unwrap();
    assert_eq!(harmony.variants.len(), 1);
}

#[tokio::test]
async fn e2_vocal_rack_scene_switching_via_active_slot() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    signal.racks().save(vocal_rack()).await;

    // Simulate switching from lead to harmony
    let mut rack = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    assert_eq!(rack.active_slot, Some(0)); // Lead
    rack.active_slot = Some(1); // Switch to harmony
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    assert_eq!(loaded.active_rig_id(), Some(&rid("vox-harmony-rig")));
}

#[tokio::test]
async fn e3_vocal_rack_mute_unmute_slots() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();

    let mut rack = Rack::new(seed_id("vox-mute"), "Vocal Mute Test");
    rack.slots.push(RackSlot {
        position: 0,
        rig_id: rid("lead"),
        active: true,
    });
    rack.slots.push(RackSlot {
        position: 1,
        rig_id: rid("harmony"),
        active: true,
    });
    rack.slots.push(RackSlot {
        position: 2,
        rig_id: rid("background"),
        active: true,
    });
    rack.active_slot = Some(0);
    signal.racks().save(rack).await;

    // Mute the background vocal
    let mut rack = signal.racks().load(rkid("vox-mute")).await.unwrap();
    rack.slots[2].active = false;
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("vox-mute")).await.unwrap();
    assert!(loaded.slots[0].active);
    assert!(loaded.slots[1].active);
    assert!(!loaded.slots[2].active);
}

#[tokio::test]
async fn e4_vocal_rack_reorder_slots() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    signal.racks().save(vocal_rack()).await;

    // Reorder: move harmony to position 0, lead to position 1
    let mut rack = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    rack.slots.reverse();
    rack.slots[0].position = 0;
    rack.slots[1].position = 1;
    rack.active_slot = Some(0); // Now points to harmony
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    assert_eq!(loaded.slots[0].rig_id, rid("vox-harmony-rig"));
    assert_eq!(loaded.slots[1].rig_id, rid("vox-lead-rig"));
    assert_eq!(loaded.active_rig_id(), Some(&rid("vox-harmony-rig")));
}

#[tokio::test]
async fn e5_no_active_slot_returns_none() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    let mut rack = vocal_rack();
    rack.active_slot = None;
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    assert!(loaded.active_rig_id().is_none());
}

// ═══════════════════════════════════════════════════════════════════
// Group F: Multiple Racks (3 tests)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn f1_multiple_racks_independent() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();

    let mut guitar_rack = Rack::new(seed_id("guitar-rack"), "Guitar Rack");
    guitar_rack.slots.push(RackSlot {
        position: 0,
        rig_id: guitar_rig_id(),
        active: true,
    });
    guitar_rack.active_slot = Some(0);

    let mut keys_rack = Rack::new(seed_id("keys-rack"), "Keys Rack");
    keys_rack.slots.push(RackSlot {
        position: 0,
        rig_id: keys_rig_id(),
        active: true,
    });
    keys_rack.active_slot = Some(0);

    signal.racks().save(guitar_rack).await;
    signal.racks().save(keys_rack).await;

    let racks = signal.racks().list().await;
    assert_eq!(racks.len(), 2);

    // Each rack references its own rig
    let g = signal.racks().load(rkid("guitar-rack")).await.unwrap();
    let k = signal.racks().load(rkid("keys-rack")).await.unwrap();
    assert_eq!(g.active_rig_id(), Some(&guitar_rig_id()));
    assert_eq!(k.active_rig_id(), Some(&keys_rig_id()));
}

#[tokio::test]
async fn f2_delete_one_rack_leaves_others() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();

    signal.racks().save(Rack::new(seed_id("r1"), "Rack 1")).await;
    signal.racks().save(Rack::new(seed_id("r2"), "Rack 2")).await;
    signal.racks().save(Rack::new(seed_id("r3"), "Rack 3")).await;

    signal.racks().delete(rkid("r2")).await;

    let racks = signal.racks().list().await;
    assert_eq!(racks.len(), 2);
    assert!(racks.iter().all(|r| r.name != "Rack 2"));
}

#[tokio::test]
async fn f3_racks_share_rig_reference() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();

    // Two racks can reference the same rig
    let mut rack_a = Rack::new(seed_id("ra"), "Rack A");
    rack_a.slots.push(RackSlot {
        position: 0,
        rig_id: guitar_rig_id(),
        active: true,
    });
    rack_a.active_slot = Some(0);

    let mut rack_b = Rack::new(seed_id("rb"), "Rack B");
    rack_b.slots.push(RackSlot {
        position: 0,
        rig_id: guitar_rig_id(),
        active: true,
    });
    rack_b.active_slot = Some(0);

    signal.racks().save(rack_a).await;
    signal.racks().save(rack_b).await;

    let a = signal.racks().load(rkid("ra")).await.unwrap();
    let b = signal.racks().load(rkid("rb")).await.unwrap();
    assert_eq!(a.active_rig_id(), b.active_rig_id());
}

// ═══════════════════════════════════════════════════════════════════
// Group G: Edge Cases (4 tests)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn g1_empty_rack_no_slots() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    let rack = Rack::new(seed_id("empty"), "Empty Rack");
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("empty")).await.unwrap();
    assert!(loaded.slots.is_empty());
    assert!(loaded.active_rig_id().is_none());
    assert!(loaded.fx_send_buses.is_empty());
}

#[tokio::test]
async fn g2_active_slot_out_of_range() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    let mut rack = vocal_rack();
    rack.active_slot = Some(99); // No slot at position 99
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    assert_eq!(loaded.active_slot, Some(99));
    // active_rig_id returns None because no slot matches position 99
    assert!(loaded.active_rig_id().is_none());
}

#[tokio::test]
async fn g3_save_overwrites_completely() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    let mut rack = vocal_rack();
    rack.fx_send_buses.push(make_fx_bus(
        "Bus",
        None,
        vec![make_fx_send(
            "Reverb",
            FxSendCategory::Reverb,
            BlockType::Reverb,
        )],
    ));
    signal.racks().save(rack).await;

    // Save again with completely different content
    let rack2 = Rack::new(seed_id("vocal-rack"), "Renamed Rack");
    signal.racks().save(rack2).await;

    let loaded = signal.racks().load(rkid("vocal-rack")).await.unwrap();
    assert_eq!(loaded.name, "Renamed Rack");
    assert!(loaded.slots.is_empty());
    assert!(loaded.fx_send_buses.is_empty());
}

#[tokio::test]
async fn g4_with_fx_send_bus_builder() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    let bus = make_fx_bus(
        "Vocal Bus",
        Some("AUX"),
        vec![make_fx_send(
            "Reverb",
            FxSendCategory::Reverb,
            BlockType::Reverb,
        )],
    );
    let rack = Rack::new(seed_id("builder-rack"), "Builder Rack").with_fx_send_bus(bus);
    signal.racks().save(rack).await;

    let loaded = signal.racks().load(rkid("builder-rack")).await.unwrap();
    assert_eq!(loaded.fx_send_buses.len(), 1);
    assert_eq!(loaded.fx_send_buses[0].sub_category.as_deref(), Some("AUX"));
}

// ═══════════════════════════════════════════════════════════════════
// Group H: Cache Behavior (3 tests)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn h1_list_caches_and_invalidates_on_save() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();

    // First call populates cache
    let racks1 = signal.racks().list().await;
    assert_eq!(racks1.len(), 0);

    // Save should invalidate cache
    signal.racks().save(Rack::new(seed_id("new"), "New")).await;

    // Second call should see the new rack
    let racks2 = signal.racks().list().await;
    assert_eq!(racks2.len(), 1);
}

#[tokio::test]
async fn h2_list_caches_and_invalidates_on_delete() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    signal.racks().save(Rack::new(seed_id("d1"), "Delete Me")).await;

    let racks1 = signal.racks().list().await;
    assert_eq!(racks1.len(), 1);

    signal.racks().delete(rkid("d1")).await;

    let racks2 = signal.racks().list().await;
    assert_eq!(racks2.len(), 0);
}

#[tokio::test]
async fn h3_repeated_list_calls_consistent() {
    let signal = bootstrap_in_memory_controller_async().await.unwrap();
    signal.racks().save(Rack::new(seed_id("r1"), "R1")).await;
    signal.racks().save(Rack::new(seed_id("r2"), "R2")).await;

    // Multiple list calls should return the same result
    let a = signal.racks().list().await;
    let b = signal.racks().list().await;
    assert_eq!(a.len(), b.len());
    assert_eq!(a[0].name, b[0].name);
    assert_eq!(a[1].name, b[1].name);
}
