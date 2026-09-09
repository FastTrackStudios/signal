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
        h.contains("FREE") && h.contains("SYNC"),
        "both modes should be offered"
    );
    Ok(())
}

/// Free by default, showing the dialled time — 375 ms.
#[tokio::test]
async fn it_starts_free_and_shows_the_dialled_time() -> dioxus_test::Result<()> {
    let mut fx = mount_with(Arc::new(DelayParams::default()), 1280, 440);
    fx.settle().await;

    assert!(!fx.params.time_sync.value(), "sync should start off");
    assert!(
        html(&fx).contains("375 ms"),
        "the readout should show the free-running default"
    );
    Ok(())
}

/// The SYNC button sets the mode when there is a tempo to sync to.
#[tokio::test]
async fn the_sync_button_sets_the_mode() -> dioxus_test::Result<()> {
    let mut fx = mounted(Some(120.0), None);
    fx.settle().await;

    click(&mut fx, "delay-time-sync").await;
    assert!(fx.params.time_sync.value(), "SYNC should set the mode");

    click(&mut fx, "delay-time-free").await;
    assert!(!fx.params.time_sync.value(), "FREE should clear it again");
    Ok(())
}

/// Sync is refused when the host has no tempo. A switch that flips and then
/// silently does nothing looks like a bug; one that will not flip says why.
#[tokio::test]
async fn sync_is_refused_when_the_host_has_no_tempo() -> dioxus_test::Result<()> {
    let mut fx = mounted(None, None);
    fx.settle().await;

    click(&mut fx, "delay-time-sync").await;
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
        !html(&free).contains("delay-time-picker"),
        "the picker should be hidden while the control is free-running"
    );

    let mut synced = mounted(Some(120.0), Some(MusicalTime::default()));
    synced.settle().await;
    assert!(
        html(&synced).contains("delay-time-picker"),
        "the picker should be drawn once synced"
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

    click(&mut fx, "delay-time-picker-next").await;
    assert_eq!(div_of(&fx), "1/16D", "stepping should keep the dot");

    Ok(())
}

#[tokio::test]
async fn the_chips_set_the_flavour_and_keep_the_note() -> dioxus_test::Result<()> {
    let start = MusicalTime::new(NoteValue::Eighth, Flavour::Straight);
    let mut fx = mounted(Some(120.0), Some(start));
    fx.settle().await;
    assert_eq!(div_of(&fx), "1/8");

    click(&mut fx, "delay-time-picker-triplet").await;
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

    click(&mut fx, "delay-time-picker-dotted").await;
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
