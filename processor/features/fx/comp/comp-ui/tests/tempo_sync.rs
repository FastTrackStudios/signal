//! The compressor's attack and release, read as notes against the tempo.
//!
//! `CompStageParams::attack_ms_at` / `release_ms_at` are what the audio
//! thread calls, so this is the contract that matters. Release is the one
//! that earns its keep — a release timed to the beat is how a compressor
//! breathes with the track instead of against it — but both are here because
//! a stage doing rhythmic pumping wants both ends on the grid.

#![cfg(feature = "native")]

use comp_ui::params::{CompParams, MAX_RELEASE_MS, MIN_ATTACK_MS};
use musical_time::{Flavour, MusicalTime, NoteValue};
use nice_plug::prelude::Param;

/// A tolerance with a unit. A value dialled into a skewed range makes a round
/// trip through normalization and returns a hair off; nothing about a
/// compressor's timing cares about a hundredth of a millisecond.
const EPS_MS: f64 = 0.01;

/// SAFETY on every call: the parameter outlives the pointer, and these run on
/// one thread with no audio thread reading concurrently.
fn set_bool(p: &nice_plug::prelude::BoolParam, v: bool) {
    unsafe {
        p.as_ptr()
            ._internal_set_normalized_value(if v { 1.0 } else { 0.0 });
    }
}
fn set_int(p: &nice_plug::prelude::IntParam, v: i32) {
    unsafe {
        p.as_ptr()
            ._internal_set_normalized_value(p.preview_normalized(v));
    }
}
fn set_float(p: &nice_plug::prelude::FloatParam, v: f32) {
    unsafe {
        p.as_ptr()
            ._internal_set_normalized_value(p.preview_normalized(v));
    }
}

fn div(value: NoteValue, flavour: Flavour) -> i32 {
    i32::try_from(MusicalTime::new(value, flavour).index()).unwrap()
}

#[test]
fn free_running_is_the_dialled_number_whatever_the_tempo() {
    let p = CompParams::default();
    let s = &p.stage1;
    set_float(&s.attack_ms, 12.0);
    set_float(&s.release_ms, 250.0);

    for tempo in [None, Some(120.0), Some(90.0)] {
        assert!((s.attack_ms_at(tempo) - 12.0).abs() < EPS_MS);
        assert!((s.release_ms_at(tempo) - 250.0).abs() < EPS_MS);
    }
}

/// The case this exists for: a release on the beat.
#[test]
fn a_synced_release_follows_the_tempo() {
    let p = CompParams::default();
    let s = &p.stage1;
    set_bool(&s.release_sync, true);
    set_int(&s.release_div, div(NoteValue::Quarter, Flavour::Straight));

    assert!((s.release_ms_at(Some(120.0)) - 500.0).abs() < EPS_MS);
    assert!((s.release_ms_at(Some(90.0)) - 60_000.0 / 90.0).abs() < EPS_MS);
}

#[test]
fn attack_and_release_sync_independently() {
    let p = CompParams::default();
    let s = &p.stage1;
    set_float(&s.attack_ms, 12.0);
    set_bool(&s.release_sync, true);
    set_int(&s.release_div, div(NoteValue::Eighth, Flavour::Straight));

    // Release on the grid, attack still where it was dialled — the common
    // pairing, and the reason the two have separate switches.
    assert!((s.release_ms_at(Some(120.0)) - 250.0).abs() < EPS_MS);
    assert!((s.attack_ms_at(Some(120.0)) - 12.0).abs() < EPS_MS);
}

#[test]
fn dotted_and_triplet_reach_the_release() {
    let p = CompParams::default();
    let s = &p.stage1;
    set_bool(&s.release_sync, true);

    for (flavour, expected) in [
        (Flavour::Straight, 250.0),
        (Flavour::Dotted, 375.0),
        (Flavour::Triplet, 500.0 / 3.0),
    ] {
        set_int(&s.release_div, div(NoteValue::Eighth, flavour));
        let got = s.release_ms_at(Some(120.0));
        assert!(
            (got - expected).abs() < EPS_MS,
            "an eighth {flavour:?} at 120 BPM should be {expected} ms, got {got}"
        );
    }
}

/// A host with no transport reports no tempo. The compressor has to keep
/// compressing, at the timings the user dialled.
#[test]
fn a_host_with_no_tempo_falls_back_to_the_free_times() {
    let p = CompParams::default();
    let s = &p.stage1;
    set_bool(&s.attack_sync, true);
    set_bool(&s.release_sync, true);
    set_float(&s.attack_ms, 12.0);
    set_float(&s.release_ms, 250.0);

    assert!((s.attack_ms_at(None) - 12.0).abs() < EPS_MS);
    assert!((s.release_ms_at(None) - 250.0).abs() < EPS_MS);
}

/// A whole note at 40 BPM is six seconds; the release stops at three. The
/// note stays selectable, the time stops where the control stops.
#[test]
fn a_note_longer_than_the_control_clamps() {
    let p = CompParams::default();
    let s = &p.stage1;
    set_bool(&s.release_sync, true);
    set_int(&s.release_div, div(NoteValue::Whole, Flavour::Straight));
    assert!((s.release_ms_at(Some(40.0)) - MAX_RELEASE_MS).abs() < EPS_MS);
}

/// And the other end: an attack cannot go below its floor either. A 1/64 at
/// 20 BPM is 187 ms, well inside range — but the clamp has to hold from both
/// directions, so check the floor with an absurd tempo.
#[test]
fn the_attack_clamps_at_its_floor() {
    let p = CompParams::default();
    let s = &p.stage1;
    set_bool(&s.attack_sync, true);
    set_int(&s.attack_div, div(NoteValue::SixtyFourth, Flavour::Triplet));
    // 100000 BPM is nonsense, which is the point: whatever a host reports,
    // the attack lands inside the range the engine accepts.
    let got = s.attack_ms_at(Some(100_000.0));
    assert!(
        got >= MIN_ATTACK_MS - EPS_MS,
        "attack fell below its floor: {got}"
    );
}

/// Every stage has its own pair. Eight compressors in a stack, and syncing
/// one must not sync the rest.
#[test]
fn each_stage_syncs_on_its_own() {
    let p = CompParams::default();
    set_bool(&p.stage1.release_sync, true);
    set_int(
        &p.stage1.release_div,
        div(NoteValue::Quarter, Flavour::Straight),
    );
    set_float(&p.stage2.release_ms, 250.0);

    assert!((p.stage1.release_ms_at(Some(120.0)) - 500.0).abs() < EPS_MS);
    assert!(
        (p.stage2.release_ms_at(Some(120.0)) - 250.0).abs() < EPS_MS,
        "stage 2 should still be free-running"
    );
}
