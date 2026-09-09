//! The time control on the delay's face.
//!
//! The arithmetic is covered in `tempo_sync.rs`; this is the part the user
//! touches — that the control is on the face, that its buttons move the
//! parameters, and that it says what the current setting works out to.
//!
//! ## What this can and cannot see
//!
//! The editor's root renders ONCE under the harness: the real plugin
//! re-renders every frame from a tick, and nothing drives that tick here. So
//! a click's effect on the *parameters* is observable, but its effect on the
//! DOM is not. Anything about what the face draws is therefore checked by
//! mounting with that state already in place, and anything about what a
//! button does is checked on the parameter it writes.

#![cfg(feature = "native")]

use std::sync::Arc;

#[path = "support/mod.rs"]
mod support;

use delay_ui::params::{DelayParams, DelayUiState};
use musical_time::{Flavour, MusicalTime, NoteValue};
use nice_plug::prelude::Param;
use support::{Fixture, mount_with, mount_with_state};

/// Mount with the host reporting 120 BPM, optionally already synced to a
/// given note.
///
/// The state has to be in place before the mount — see the module note.
fn mounted(tempo: Option<f32>, synced: Option<MusicalTime>) -> Fixture {
    mounted_with_link(tempo, synced, true)
}

/// Same, choosing whether the two sides are linked. Link has to be set before
/// the mount for the same reason the tempo does — see the module note.
fn mounted_with_link(
    tempo: Option<f32>,
    synced: Option<MusicalTime>,
    linked: bool,
) -> Fixture {
    let state = Arc::new(DelayUiState::default());
    if let Some(bpm) = tempo {
        state
            .tempo_bpm
            .store(bpm, std::sync::atomic::Ordering::Relaxed);
    }
    let params = Arc::new(DelayParams::default());
    if let Some(div) = synced {
        // SAFETY: nothing else holds these yet, and this is one thread.
        unsafe {
            params.time_sync.as_ptr()._internal_set_normalized_value(1.0);
            let d = &params.div_l;
            d.as_ptr()._internal_set_normalized_value(
                d.preview_normalized(i32::try_from(div.index()).unwrap()),
            );
        }
    }
    if !linked {
        // SAFETY: nothing else holds this yet, and this is one thread.
        unsafe {
            params.link.as_ptr()._internal_set_normalized_value(0.0);
        }
    }
    mount_with_state(params, state, 1280, 440)
}

fn html(fx: &Fixture) -> String {
    fx.tester.root().inner_html()
}

/// Click an element by test id and let the edit reach the parameter.
async fn click(fx: &mut Fixture, testid: &str) {
    let el = fx
        .tester
        .query(dioxus_test::by_testid(testid))
        .immediately()
        .unwrap_or_else(|_| panic!("no element {testid}"));
    el.click();
    fx.settle().await;
}

fn div_of(fx: &Fixture) -> &'static str {
    MusicalTime::from_param(fx.params.div_l.value()).label()
}

#[tokio::test]
async fn the_face_carries_the_time_control() -> dioxus_test::Result<()> {
    let mut fx = mount_with(Arc::new(DelayParams::default()), 1280, 440);
    fx.settle().await;

    let h = html(&fx);
    assert!(h.contains("delay-time"), "the time control should be on the face");
    assert!(
        h.contains("MS") && h.contains("NOTE"),
        "both modes should be offered"
    );
    Ok(())
}

/// The text of a knob's value readout, markup stripped.
fn readout(fx: &Fixture, param: &str) -> Option<String> {
    let html = fx
        .tester
        .query_all(dioxus_test::by_testid(format!("knob-value-{param}")))
        .immediately()
        .first()
        .map(dioxus_test::ResolvedElement::inner_html)?;
    let mut text = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    Some(text.trim().to_string())
}

/// In milliseconds the knob is in charge, and its readout is the time.
#[tokio::test]
async fn it_starts_in_milliseconds_showing_the_dialled_time() -> dioxus_test::Result<()> {
    let mut fx = mount_with(Arc::new(DelayParams::default()), 1280, 440);
    fx.settle().await;

    assert!(!fx.params.time_sync.value(), "it should start in milliseconds");
    let text = readout(&fx, "time-l").expect("no Time L readout");
    assert!(
        text.contains("375"),
        "the readout should show the dialled default, got {text:?}"
    );
    Ok(())
}

/// Locked to the tempo, the knob is no longer what sets the time — so its
/// readout has to show what the NOTE works out to, not the milliseconds the
/// parameter still happens to hold.
#[tokio::test]
async fn synced_the_readout_shows_the_note_not_the_stale_milliseconds()
-> dioxus_test::Result<()> {
    // A dotted sixteenth at 120 BPM is 187.5 ms; the free-running parameter
    // is still sitting at its 375 ms default.
    let mut fx = mounted(
        Some(120.0),
        Some(MusicalTime::new(NoteValue::Sixteenth, Flavour::Dotted)),
    );
    fx.settle().await;

    let text = readout(&fx, "time-l").expect("no Time L readout");
    assert!(
        text.contains("188"),
        "expected the resolved time, got {text:?}"
    );
    assert!(
        !text.contains("375"),
        "the face should not print a delay time the delay is not using: {text:?}"
    );
    Ok(())
}

/// The SYNC button sets the mode when there is a tempo to sync to.
#[tokio::test]
async fn the_sync_button_sets_the_mode() -> dioxus_test::Result<()> {
    let mut fx = mounted(Some(120.0), None);
    fx.settle().await;

    click(&mut fx, "delay-time-note").await;
    assert!(fx.params.time_sync.value(), "SYNC should set the mode");

    click(&mut fx, "delay-time-ms").await;
    assert!(!fx.params.time_sync.value(), "FREE should clear it again");
    Ok(())
}

/// Sync is refused when the host has no tempo. A switch that flips and then
/// silently does nothing looks like a bug; one that will not flip says why.
#[tokio::test]
async fn sync_is_refused_when_the_host_has_no_tempo() -> dioxus_test::Result<()> {
    let mut fx = mounted(None, None);
    fx.settle().await;

    click(&mut fx, "delay-time-note").await;
    assert!(
        !fx.params.time_sync.value(),
        "with no tempo there is nothing to sync to, so the switch should not take"
    );
    Ok(())
}

/// The note picker is drawn only when the control is actually synced — a row
/// of note buttons on a control running in milliseconds is just noise.
#[tokio::test]
async fn the_note_picker_is_drawn_only_when_synced() -> dioxus_test::Result<()> {
    let mut free = mounted(Some(120.0), None);
    free.settle().await;
    assert!(
        !html(&free).contains("delay-time-l-note"),
        "the picker should not be drawn while the times are in milliseconds"
    );

    let mut synced = mounted(Some(120.0), Some(MusicalTime::default()));
    synced.settle().await;
    assert!(
        html(&synced).contains("delay-time-l-note"),
        "the picker should take the knob's place once locked to the tempo"
    );
    Ok(())
}

/// The two gestures the picker is built around: the arrows step the note
/// value and keep the flavour, the D/T chips set the flavour and keep the
/// note. Between them every one of the 21 combinations is two clicks away.
#[tokio::test]
async fn the_arrows_step_the_note_and_keep_the_flavour() -> dioxus_test::Result<()> {
    let start = MusicalTime::new(NoteValue::Eighth, Flavour::Dotted);
    let mut fx = mounted(Some(120.0), Some(start));
    fx.settle().await;
    assert_eq!(div_of(&fx), "1/8D");

    click(&mut fx, "delay-time-l-note-next").await;
    assert_eq!(div_of(&fx), "1/16D", "stepping should keep the dot");

    Ok(())
}

#[tokio::test]
async fn the_chips_set_the_flavour_and_keep_the_note() -> dioxus_test::Result<()> {
    let start = MusicalTime::new(NoteValue::Eighth, Flavour::Straight);
    let mut fx = mounted(Some(120.0), Some(start));
    fx.settle().await;
    assert_eq!(div_of(&fx), "1/8");

    click(&mut fx, "delay-time-l-note-triplet").await;
    assert_eq!(div_of(&fx), "1/8T", "the chip should keep the note value");
    Ok(())
}

/// Pressing the lit chip goes back to straight — the only way back without
/// cycling through the other flavour.
#[tokio::test]
async fn pressing_the_lit_chip_returns_to_straight() -> dioxus_test::Result<()> {
    let start = MusicalTime::new(NoteValue::Eighth, Flavour::Dotted);
    let mut fx = mounted(Some(120.0), Some(start));
    fx.settle().await;

    click(&mut fx, "delay-time-l-note-dotted").await;
    assert_eq!(div_of(&fx), "1/8");
    Ok(())
}

/// The readout is what makes a note value usable: "1/8D" alone does not tell
/// you whether the delay is 100 ms or a second.
#[tokio::test]
async fn the_readout_shows_what_the_note_works_out_to() -> dioxus_test::Result<()> {
    // A dotted eighth at 120 BPM is 375 ms.
    let mut dotted = mounted(
        Some(120.0),
        Some(MusicalTime::new(NoteValue::Eighth, Flavour::Dotted)),
    );
    dotted.settle().await;
    assert!(html(&dotted).contains("375 ms"), "expected the resolved time");

    // A straight eighth is 250 ms.
    let mut straight = mounted(
        Some(120.0),
        Some(MusicalTime::new(NoteValue::Eighth, Flavour::Straight)),
    );
    straight.settle().await;
    assert!(
        html(&straight).contains("250 ms"),
        "the readout should follow the note"
    );
    Ok(())
}

/// Left and right carry their own note. A delay with a dotted eighth on one
/// side and a quarter on the other is a whole family of sounds, so the two
/// pickers have to be independent — and unlinked, the two times differ.
#[tokio::test]
async fn the_two_sides_take_different_notes() -> dioxus_test::Result<()> {
    let mut fx = mounted_with_link(
        Some(120.0),
        Some(MusicalTime::new(NoteValue::Eighth, Flavour::Dotted)),
        false,
    );
    fx.settle().await;

    // Step the right picker down a note; the left must not follow.
    click(&mut fx, "delay-time-r-note-next").await;

    let l = MusicalTime::from_param(fx.params.div_l.value());
    let r = MusicalTime::from_param(fx.params.div_r.value());
    assert_eq!(l.label(), "1/8D", "the left side should not have moved");
    assert_ne!(r.label(), l.label(), "the two sides should differ");

    // And the resolved times differ too.
    let tempo = Some(120.0);
    assert!(
        (fx.params.time_l_ms(tempo) - fx.params.time_r_ms(tempo)).abs() > 1.0,
        "unlinked, the two sides should resolve to different times"
    );
    Ok(())
}

/// The dial is gone while the times are notes — there is no milliseconds
/// value to enter then, and a knob that turns without changing anything is
/// worse than no knob.
#[tokio::test]
async fn the_time_dials_are_replaced_not_merely_ignored() -> dioxus_test::Result<()> {
    let mut ms = mount_with(Arc::new(DelayParams::default()), 1280, 440);
    ms.settle().await;
    let plain = ms.tester.root().inner_html();
    assert!(
        plain.contains(r#"data-testid="hw-knob-time-l""#),
        "the Time L dial should be there in milliseconds mode"
    );

    let mut synced = mounted(Some(120.0), Some(MusicalTime::default()));
    synced.settle().await;
    let locked = synced.tester.root().inner_html();
    assert!(
        !locked.contains(r#"data-testid="hw-knob-time-l""#),
        "the Time L dial should be gone once the time is a note"
    );
    assert!(
        locked.contains("delay-time-l-note") && locked.contains("delay-time-r-note"),
        "both sides should offer a note picker"
    );
    Ok(())
}

/// Link had no control anywhere in the UI: it defaults ON, so the right time
/// always followed the left and the right control was inert without saying
/// so. It has a button now, and it works.
#[tokio::test]
async fn link_has_a_control_and_it_toggles() -> dioxus_test::Result<()> {
    let mut fx = mount_with(Arc::new(DelayParams::default()), 1280, 440);
    fx.settle().await;
    assert!(fx.params.link.value(), "Link defaults on");

    click(&mut fx, "delay-link").await;
    assert!(!fx.params.link.value(), "the LINK button should turn it off");
    Ok(())
}

/// Linked, the right picker shows the note it is following but will not take
/// an edit — a control you can move that changes nothing is exactly what this
/// mode was meant to avoid.
#[tokio::test]
async fn the_right_picker_is_slaved_while_linked() -> dioxus_test::Result<()> {
    let mut fx = mounted(
        Some(120.0),
        Some(MusicalTime::new(NoteValue::Eighth, Flavour::Dotted)),
    );
    fx.settle().await;
    assert!(fx.params.link.value(), "this test wants Link on");

    let before = fx.params.div_r.value();
    click(&mut fx, "delay-time-r-note-next").await;
    assert_eq!(
        fx.params.div_r.value(),
        before,
        "a slaved picker should not take an edit"
    );
    Ok(())
}
