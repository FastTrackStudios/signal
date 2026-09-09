//! The reverb's pre-delay, read as a note value against the host's tempo.
//!
//! `ReverbParams::predelay_ms` is what the audio thread calls, so this is the
//! contract that matters. Keeping the decision in the params rather than in
//! the plugin's `sync_params` is what lets the face show the same number the
//! audio thread is using.

#![cfg(feature = "native")]

use musical_time::{Flavour, MusicalTime, NoteValue};
use nice_plug::prelude::Param;
use reverb_ui::params::{MAX_PREDELAY_MS, ReverbParams};

/// How close two times have to be to count as the same, in milliseconds. A
/// value dialled into a skewed range makes a round trip through normalization
/// and comes back a hair off; nothing about a pre-delay cares.
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
    let p = ReverbParams::default();
    set_float(&p.predelay, 40.0);
    assert!((p.predelay_ms(None) - 40.0).abs() < EPS_MS);
    assert!((p.predelay_ms(Some(120.0)) - 40.0).abs() < EPS_MS);
}

#[test]
fn a_synced_pre_delay_follows_the_tempo() {
    let p = ReverbParams::default();
    set_bool(&p.predelay_sync, true);
    set_int(&p.predelay_div, div(NoteValue::Sixteenth, Flavour::Straight));

    // A sixteenth at 120 BPM is 125 ms.
    assert!((p.predelay_ms(Some(120.0)) - 125.0).abs() < EPS_MS);
    // ...and at 90 it is longer, in proportion.
    assert!((p.predelay_ms(Some(90.0)) - 60_000.0 / 90.0 / 4.0).abs() < EPS_MS);
}

#[test]
fn dotted_and_triplet_reach_the_pre_delay() {
    let p = ReverbParams::default();
    set_bool(&p.predelay_sync, true);

    for (flavour, expected) in [
        (Flavour::Straight, 125.0),
        (Flavour::Dotted, 187.5),
        (Flavour::Triplet, 250.0 / 3.0),
    ] {
        set_int(&p.predelay_div, div(NoteValue::Sixteenth, flavour));
        let got = p.predelay_ms(Some(120.0));
        assert!(
            (got - expected).abs() < EPS_MS,
            "a sixteenth {flavour:?} at 120 BPM should be {expected} ms, got {got}"
        );
    }
}

/// A host with no transport reports no tempo. The reverb has to keep working,
/// at the pre-delay the user dialled.
#[test]
fn a_host_with_no_tempo_falls_back_to_the_free_time() {
    let p = ReverbParams::default();
    set_bool(&p.predelay_sync, true);
    set_float(&p.predelay, 40.0);
    assert!((p.predelay_ms(None) - 40.0).abs() < EPS_MS);
}

/// The pre-delay stops at 250 ms, and a quarter note at any sane tempo is
/// longer than that. The note stays selectable; the time stops where the
/// control stops, rather than wrapping or reading as zero.
#[test]
fn a_note_longer_than_the_control_clamps() {
    let p = ReverbParams::default();
    set_bool(&p.predelay_sync, true);
    set_int(&p.predelay_div, div(NoteValue::Quarter, Flavour::Straight));
    assert!((p.predelay_ms(Some(120.0)) - MAX_PREDELAY_MS).abs() < EPS_MS);
}
