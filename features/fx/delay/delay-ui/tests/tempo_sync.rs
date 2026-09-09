//! The delay's times, read as note values against the host's tempo.
//!
//! `DelayParams::time_l_ms` / `time_r_ms` are what the audio thread calls, so
//! this is the contract that matters: given a tempo and the sync switch, what
//! delay time comes out. Keeping the decision in the params (rather than in
//! the plugin's `sync_params`) is what lets the editor show the same number
//! the audio thread is using.

#![cfg(feature = "native")]

use delay_ui::params::{DelayParams, MAX_TIME_MS};
use musical_time::{Flavour, MusicalTime, NoteValue};
use nice_plug::prelude::Param;

/// How close two delay times have to be to count as the same, in
/// milliseconds. A value dialled into a skewed range makes a round trip
/// through normalization and returns a hair off — 375 comes back as
/// 375.00003 — and nothing about a delay cares a hundredth of a millisecond
/// either way. Exact equality here would be testing float storage, not
/// tempo sync.
const EPS_MS: f64 = 0.01;

/// Set a parameter without a host attached.
///
/// The test drives the params tree directly — there is no `GuiContext` here
/// to route a gesture through, which is the whole point: `time_l_ms` is a
/// plain function of (params, tempo) and reads like one.
///
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

#[test]
fn free_running_is_the_dialled_number_whatever_the_tempo() {
    let p = DelayParams::default();
    set_float(&p.time_l, 375.0);

    assert!((p.time_l_ms(None) - 375.0).abs() < EPS_MS);
    assert!((p.time_l_ms(Some(120.0)) - 375.0).abs() < EPS_MS);
    assert!((p.time_l_ms(Some(90.0)) - 375.0).abs() < EPS_MS);
}

/// The default note is a dotted eighth, which at 120 BPM is the same 375 ms
/// the free-running control defaults to — so flipping Sync on a 120 BPM
/// session is a no-op rather than a jump.
#[test]
fn switching_to_sync_at_120_bpm_lands_on_the_free_default() {
    let p = DelayParams::default();
    let free = p.time_l_ms(Some(120.0));
    set_bool(&p.time_sync, true);
    let synced = p.time_l_ms(Some(120.0));
    assert!(
        (free - synced).abs() < EPS_MS,
        "free {free} and synced {synced} should agree at the defaults"
    );
}

#[test]
fn a_synced_time_follows_the_tempo() {
    let p = DelayParams::default();
    set_bool(&p.time_sync, true);
    set_int(
        &p.div_l,
        i32::try_from(MusicalTime::new(NoteValue::Quarter, Flavour::Straight).index()).unwrap(),
    );

    assert!((p.time_l_ms(Some(120.0)) - 500.0).abs() < EPS_MS);
    assert!((p.time_l_ms(Some(60.0)) - 1000.0).abs() < EPS_MS);
    assert!((p.time_l_ms(Some(140.0)) - 60_000.0 / 140.0).abs() < EPS_MS);
}

/// A host with no transport reports no tempo. The delay has to keep making
/// its sound, at the time the user dialled.
#[test]
fn a_host_with_no_tempo_falls_back_to_the_free_time() {
    let p = DelayParams::default();
    set_bool(&p.time_sync, true);
    set_float(&p.time_l, 200.0);
    assert!((p.time_l_ms(None) - 200.0).abs() < EPS_MS);
}

/// A whole note at 40 BPM is six seconds and the delay line stops at four.
/// The note stays selectable; the time stops where the control stops.
#[test]
fn a_note_longer_than_the_delay_line_clamps() {
    let p = DelayParams::default();
    set_bool(&p.time_sync, true);
    set_int(
        &p.div_l,
        i32::try_from(MusicalTime::new(NoteValue::Whole, Flavour::Straight).index()).unwrap(),
    );
    assert!((p.time_l_ms(Some(40.0)) - MAX_TIME_MS).abs() < EPS_MS);
}

/// Link mirrors the left side onto the right — including which NOTE it is on,
/// not just the resolved milliseconds. A linked pair that drifted apart the
/// moment you switched to Sync would not be linked.
#[test]
fn link_mirrors_the_note_not_just_the_milliseconds() {
    let p = DelayParams::default();
    set_bool(&p.time_sync, true);
    set_bool(&p.link, true);
    set_int(
        &p.div_l,
        i32::try_from(MusicalTime::new(NoteValue::Quarter, Flavour::Straight).index()).unwrap(),
    );
    set_int(
        &p.div_r,
        i32::try_from(MusicalTime::new(NoteValue::Sixteenth, Flavour::Triplet).index()).unwrap(),
    );

    assert!(
        (p.time_r_ms(Some(120.0)) - p.time_l_ms(Some(120.0))).abs() < EPS_MS,
        "linked, the right side should be on the left's note"
    );

    // Unlinked, each side is on its own.
    set_bool(&p.link, false);
    assert!((p.time_l_ms(Some(120.0)) - 500.0).abs() < EPS_MS);
    let sixteenth_triplet = 500.0 / 4.0 * (2.0 / 3.0);
    assert!((p.time_r_ms(Some(120.0)) - sixteenth_triplet).abs() < EPS_MS);
}

/// Dotted and triplet are the reason a plain list of note values is not
/// enough, so check them end to end through the parameter rather than only
/// in the primitive's own tests.
#[test]
fn dotted_and_triplet_reach_the_delay_time() {
    let p = DelayParams::default();
    set_bool(&p.time_sync, true);

    for (flavour, expected) in [
        (Flavour::Straight, 250.0),
        (Flavour::Dotted, 375.0),
        (Flavour::Triplet, 500.0 / 3.0),
    ] {
        set_int(
            &p.div_l,
            i32::try_from(MusicalTime::new(NoteValue::Eighth, flavour).index()).unwrap(),
        );
        let got = p.time_l_ms(Some(120.0));
        assert!(
            (got - expected).abs() < EPS_MS,
            "an eighth {flavour:?} at 120 BPM should be {expected} ms, got {got}"
        );
    }
}
