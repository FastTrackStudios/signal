//! Solo, record arm, phase and input monitoring.
//!
//! Repetition, deliberately. The spine was built once in [`super::mute`] —
//! pointer state as a Signal, track from the store, write through
//! `daw_control`, and the backend's event is what changes what is drawn —
//! so every control that already has its field is the same three moves with
//! a different field and a different piece of art.
//!
//! What is *not* repetition is called out where it happens: solo has a third
//! state the track model cannot express, and input monitoring cycles rather
//! than toggles.

use daw_proto::track::InputMonitoringMode;
use daw_theme_art::dress::Panel;
use daw_theme_art::slice;
use daw_theme_art::vector_controls as art;

use crate::controls::reach::write_track;
use crate::controls::use_track;
use crate::prelude::*;

/// Enter and leave handlers, which every control here wires identically.
///
/// Hover has to be a prop rather than a `:hover` rule — see [`super`] — so
/// each control owns a Signal for it. Press is *not* here: it carries the
/// write, which is the one thing that differs per control.
macro_rules! pointer {
    ($at:ident) => {
        (
            move |_: MouseEvent| $at.set(art::Interaction::Hover),
            move |_: MouseEvent| $at.set(art::Interaction::Normal),
        )
    };
}

/// A track's solo button.
///
/// Two states, not three. REAPER's art has a third — solo *defeat*, the blue
/// one — and the component draws it, but `Track` carries `soloed: bool` and
/// nothing else, so there is no way to know which of the two lit meanings
/// applies. Rather than pick one and be wrong half the time, this shows lit
/// or unlit and leaves defeat to whoever adds the field.
#[component]
pub fn SoloButton(
    /// The track's GUID.
    track: String,
    #[props(default)] panel: Panel,
    #[props(default = 1.0)] scale: f32,
) -> Element {
    let mut at = use_signal(art::Interaction::default);
    let (enter, leave) = pointer!(at);
    let info = use_track(track.clone());
    let on = info.read().as_ref().is_some_and(|t| t.soloed);

    let named = solo_art(panel, on);
    let (w, h) = px(named, scale);
    let guid = track.clone();

    rsx! {
        div {
            style: box_style(w, h),
            onmouseenter: enter,
            onmouseleave: leave,
            onmousedown: move |_| {
                at.set(art::Interaction::Pressed);
                write_track(guid.clone(), "solo", |t| async move { t.toggle_solo().await });
            },
            onmouseup: move |_| at.set(art::Interaction::Hover),
            art::SoloButton {
                state: if on { art::Solo::On } else { art::Solo::Off },
                art: named,
                body: daw_theme_art::dress::label_body(panel),
                legend: daw_theme_art::dress::label_legend(panel, on, true),
                unlit: daw_theme_art::dress::label_unlit(panel),
                sinks: !panel.is_track(),
                hover: 0.35,
                depth: 0.11,
                width: Some(w),
                height: Some(h),
                at: at(),
            }
        }
    }
}

/// A track's record-arm button.
///
/// The art knows six states — armed, auto-arm, and the "armed but recording
/// disabled" variants REAPER draws differently on purpose. `Track` carries
/// `armed: bool`, so this shows two of them; the rest arrive with the record
/// mode they describe.
#[component]
pub fn RecordArmButton(
    /// The track's GUID.
    track: String,
    #[props(default)] panel: Panel,
    #[props(default = 1.0)] scale: f32,
) -> Element {
    let mut at = use_signal(art::Interaction::default);
    let (enter, leave) = pointer!(at);
    let info = use_track(track.clone());
    let armed = info.read().as_ref().is_some_and(|t| t.armed);

    let named = slice::expect_art(match (panel, armed) {
        (Panel::Mixer, false) => "mcp_recarm_off",
        (Panel::Mixer, true) => "mcp_recarm_on",
        (Panel::Track, false) => "track_recarm_off",
        (Panel::Track, true) => "track_recarm_on",
    });
    let (w, h) = px(named, scale);
    let guid = track.clone();

    rsx! {
        div {
            style: box_style(w, h),
            onmouseenter: enter,
            onmouseleave: leave,
            onmousedown: move |_| {
                at.set(art::Interaction::Pressed);
                write_track(guid.clone(), "arm", |t| async move { t.toggle_arm().await });
            },
            onmouseup: move |_| at.set(art::Interaction::Hover),
            art::RecordArmButton {
                state: if armed { art::RecordArm::On } else { art::RecordArm::Off },
                art: named,
                // The mixer seats its ring in a moulded housing; the track
                // panel draws it bare on the strip.
                housing: !panel.is_track(),
                width: Some(w),
                height: Some(h),
                at: at(),
            }
        }
    }
}

/// A track's polarity (phase invert) button.
#[component]
pub fn PhaseButton(
    /// The track's GUID.
    track: String,
    #[props(default = 1.0)] scale: f32,
) -> Element {
    let mut at = use_signal(art::Interaction::default);
    let (enter, leave) = pointer!(at);
    let info = use_track(track.clone());
    let inverted = info.read().as_ref().is_some_and(|t| t.phase_inverted);

    let named = slice::expect_art(if inverted {
        "mcp_phase_inv"
    } else {
        "mcp_phase_norm"
    });
    let (w, h) = px(named, scale);
    let guid = track.clone();

    rsx! {
        div {
            style: box_style(w, h),
            onmouseenter: enter,
            onmouseleave: leave,
            onmousedown: move |_| {
                at.set(art::Interaction::Pressed);
                write_track(guid.clone(), "phase", move |t| async move {
                    t.set_phase_inverted(!inverted).await
                });
            },
            onmouseup: move |_| at.set(art::Interaction::Hover),
            art::PhaseButton {
                inverted,
                art: named,
                width: Some(w),
                height: Some(h),
                at: at(),
            }
        }
    }
}

/// A track's input-monitoring indicator.
///
/// Cycles rather than toggles, because the underlying mode has three values:
/// off, on, and "not when playing" — REAPER's tape-style monitoring. Click
/// order follows REAPER's own cycle so muscle memory carries over.
#[component]
pub fn MonitorButton(
    /// The track's GUID.
    track: String,
    #[props(default)] panel: Panel,
    #[props(default = 1.0)] scale: f32,
) -> Element {
    let mut at = use_signal(art::Interaction::default);
    let (enter, leave) = pointer!(at);
    let info = use_track(track.clone());
    let mode = info
        .read()
        .as_ref()
        .map(|t| t.input_monitor)
        .unwrap_or(InputMonitoringMode::Off);

    let named = slice::expect_art(match (panel, mode) {
        (Panel::Mixer, InputMonitoringMode::Off) => "mcp_monitor_off",
        (Panel::Mixer, _) => "mcp_monitor_on",
        (Panel::Track, InputMonitoringMode::Off) => "track_monitor_off",
        (Panel::Track, _) => "track_monitor_on",
    });
    let (w, h) = px(named, scale);
    let guid = track.clone();
    let next = cycle(mode);

    rsx! {
        div {
            style: box_style(w, h),
            onmouseenter: enter,
            onmouseleave: leave,
            onmousedown: move |_| {
                at.set(art::Interaction::Pressed);
                write_track(guid.clone(), "monitor", move |t| async move {
                    t.set_input_monitor(next).await
                });
            },
            onmouseup: move |_| at.set(art::Interaction::Hover),
            art::InputMonitorIndicator {
                state: match mode {
                    InputMonitoringMode::Off => art::Monitoring::Off,
                    InputMonitoringMode::Normal => art::Monitoring::On,
                    InputMonitoringMode::NotWhenPlaying => art::Monitoring::Auto,
                },
                art: named,
                // The mixer's waves radiate downward, the track panel's
                // rightward: same geometry, turned a quarter turn.
                axis: if panel.is_track() { art::Axis::Horizontal } else { art::Axis::Vertical },
                width: Some(w),
                height: Some(h),
                at: at(),
            }
        }
    }
}

/// A track's IO button.
///
/// Shows what the track is wired to: sends, receives, and — the `_dis`
/// variants — whether it still reaches its parent at all. That last one is
/// `parent_send`, which until now was a per-track routing call nothing on a
/// strip could afford to make, so the indicator could not show it.
///
/// Sends and receives are not on the track model, so they are props: a
/// strip that knows its routing passes them, and one that does not draws
/// the plain button rather than guessing.
#[component]
pub fn IoButton(
    /// The track's GUID.
    track: String,
    #[props(default)] panel: Panel,
    #[props(default)] has_sends: bool,
    #[props(default)] has_receives: bool,
    #[props(default = 1.0)] scale: f32,
) -> Element {
    let mut at = use_signal(art::Interaction::default);
    let (enter, leave) = pointer!(at);
    let info = use_track(track.clone());
    // Defaults to sending: a track the store has not seen yet is not a
    // track cut off from the master, and drawing it as one would put a
    // `_dis` badge on every strip for the first frame.
    let sends_to_parent = info.read().as_ref().map(|t| t.parent_send).unwrap_or(true);

    let named = slice::expect_art(io_art(panel, has_sends, has_receives, !sends_to_parent));
    let (w, h) = px(named, scale);
    let guid = track.clone();

    rsx! {
        div {
            style: box_style(w, h),
            onmouseenter: enter,
            onmouseleave: leave,
            onmousedown: move |_| {
                at.set(art::Interaction::Pressed);
                write_track(guid.clone(), "parent send", move |t| async move {
                    t.set_parent_send(!sends_to_parent).await
                });
            },
            onmouseup: move |_| at.set(art::Interaction::Hover),
            art::RoutingButton {
                has_sends,
                has_receives,
                disabled: !sends_to_parent,
                art: named,
                // The mixer stacks the lanes; the track panel sets them in
                // a row, which is the whole difference between the two
                // images and the reason the component takes an axis.
                axis: if panel.is_track() { art::Axis::Horizontal } else { art::Axis::Vertical },
                width: Some(w),
                height: Some(h),
                at: at(),
            }
        }
    }
}

/// REAPER's eight IO images: sends and receives crossed with whether the
/// track still reaches its parent.
fn io_art(panel: Panel, sends: bool, receives: bool, disabled: bool) -> &'static str {
    if panel.is_track() {
        return match (sends, receives, disabled) {
            (false, false, false) => "track_io",
            (false, false, true) => "track_io_dis",
            (true, false, false) => "track_io_s",
            (true, false, true) => "track_io_s_dis",
            (false, true, false) => "track_io_r",
            (false, true, true) => "track_io_r_dis",
            (true, true, false) => "track_io_s_r",
            (true, true, true) => "track_io_s_r_dis",
        };
    }
    match (sends, receives, disabled) {
        (false, false, false) => "mcp_io",
        (false, false, true) => "mcp_io_dis",
        (true, false, false) => "mcp_io_s",
        (true, false, true) => "mcp_io_s_dis",
        (false, true, false) => "mcp_io_r",
        (false, true, true) => "mcp_io_r_dis",
        (true, true, false) => "mcp_io_s_r",
        (true, true, true) => "mcp_io_s_r_dis",
    }
}

/// REAPER's own monitoring cycle: off → on → tape → off.
fn cycle(mode: InputMonitoringMode) -> InputMonitoringMode {
    match mode {
        InputMonitoringMode::Off => InputMonitoringMode::Normal,
        InputMonitoringMode::Normal => InputMonitoringMode::NotWhenPlaying,
        InputMonitoringMode::NotWhenPlaying => InputMonitoringMode::Off,
    }
}

/// The lit and unlit images of a solo button, per panel.
fn solo_art(panel: Panel, on: bool) -> slice::NamedArt {
    slice::expect_art(match (panel, on) {
        (Panel::Mixer, false) => "mcp_solo_off",
        (Panel::Mixer, true) => "mcp_solo_on",
        (Panel::Track, false) => "track_solo_off",
        (Panel::Track, true) => "track_solo_on",
    })
}

/// The art's source box in output pixels.
fn px(art: slice::NamedArt, scale: f32) -> (u32, u32) {
    (
        (art.source.0 * scale).round() as u32,
        (art.source.1 * scale).round() as u32,
    )
}

/// The box every control here sits in.
///
/// Explicit pixels, inline: no stylesheet is assumed to arrive, because
/// Blitz — which renders the REAPER panels — does not reliably load one.
fn box_style(w: u32, h: u32) -> String {
    format!("display:inline-block; line-height:0; width:{w}px; height:{h}px; cursor:pointer;")
}

/// The envelope button — `mcp.env` and `tcp.env`.
///
/// Its mode is not something the DAW facade reports yet, so it draws the
/// off state rather than inventing one. It is drawn regardless, because
/// REAPER reserves its box in both panels and the phase button's position
/// is measured *from* it: leaving the slot empty moves phase to where
/// REAPER never puts it.
#[component]
pub fn EnvelopeButton(
    #[props(default = 21)] width: u32,
    #[props(default = 30)] height: u32,
    /// The track panel draws it on a scrim; the mixer draws it bare.
    #[props(default)]
    panel: Panel,
) -> Element {
    let mut at = use_signal(art::Interaction::default);
    let (enter, leave) = pointer!(at);
    rsx! {
        div {
            style: "display:inline-block; line-height:0; cursor:pointer;",
            onmouseenter: enter,
            onmouseleave: leave,
            art::EnvelopeButton {
                mode: art::EnvelopeMode::Off,
                scrim: panel.is_track(),
                cell: (width as f32, height as f32),
                width: Some(width),
                height: Some(height),
                at: at(),
            }
        }
    }
}
