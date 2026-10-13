//! Transport Control Components
//!
//! Transport control bar with grid layout.
//! Copied from `FastTrackStudio` for exact styling match.

use crate::prelude::*;
use lucide_dioxus::{
    Circle as RecordIcon, Mic as ArmIcon, Pause as PauseIcon, Play as PlayIcon,
    Repeat2 as LoopIcon, SkipBack as BackIcon, SkipForward as ForwardIcon,
};

/// Transport control bar component.
///
/// Provides arm, record, back, play/pause, loop, and forward controls.
/// All actions are handled via callbacks to keep the component domain-agnostic.
///
/// The six controls always lay out as a single 6-column grid. In the default
/// (desktop) mode each cell is a roomy `icon + label` row at `text-lg`. Set
/// `compact` for tight hosts (e.g. a narrow note pane): the icon stacks over a
/// small label so all six fit without clipping regardless of the frame width.
#[component]
pub fn TransportControlBar(
    is_playing: bool,
    is_looping: bool,
    is_recording: bool,
    is_armed: bool,
    /// Number of tracks currently record-armed in the active project (from
    /// a live `daw::service::Tracks` subscription, not this song's own
    /// arm-intent toggle — see `ARMED_TRACK_COUNT`). Always shown on the
    /// Record button so an operator can see at a glance whether the mics
    /// they expect to be hot actually are before rolling tape — "0 armed"
    /// is exactly as useful to see as "12 armed".
    #[props(default = 0)]
    armed_track_count: usize,
    on_play_pause: Callback<()>,
    on_loop_toggle: Callback<()>,
    on_record_toggle: Callback<()>,
    on_arm_toggle: Callback<()>,
    on_back: Callback<()>,
    on_forward: Callback<()>,
    /// Tight layout: stack a small label under a smaller icon so all six
    /// controls fit a narrow container. Defaults false (desktop row layout).
    #[props(default)]
    compact: bool,
    /// Show the Arm + Record controls. Recording environments want them;
    /// a playback-only surface (the browser session player) does not.
    /// Defaults true. When false the bar is a 4-column playback transport
    /// (Back / Play / Loop / Advance).
    #[props(default = true)]
    show_recording: bool,
    /// Icons without their words — a phone's transport, where the four
    /// shapes say enough and the width is better spent on the icons.
    #[props(default)]
    icons_only: bool,
    /// Record where Loop is: the four-button bar of a surface that records
    /// (Back / Play / Record / Advance) — a phone or a tablet in record
    /// mode, where arming is done elsewhere and looping is not wanted.
    #[props(default)]
    record_in_loop: bool,
) -> Element {
    let playing = is_playing;
    let looping = is_looping;
    let recording = is_recording;
    let armed = is_armed;

    let cols = if show_recording { 6 } else { 4 };
    let icon = if icons_only {
        24
    } else if compact {
        20
    } else {
        28
    };
    // Each cell's look, inline and whole: its layout (icon over a small
    // word when compact, beside it otherwise) and its state's colours.
    // No hover colour: a touchscreen leaves a tapped button hovered, and
    // the tint it kept read as the button being greyed out for good.
    let base = if compact {
        "display:flex; flex-direction:column; align-items:center; justify-content:center; gap:4px; \
         padding:0 4px; text-align:center; line-height:1; font-size:11px;"
    } else {
        "display:flex; align-items:center; justify-content:center; gap:12px; font-size:18px;"
    };
    let cls = |state: Look| {
        let (fg, bg) = match state {
            Look::Idle => ("#e5e7eb", "#0b0c0e"),
            Look::On => ("#0b0c0e", "#e5e7eb"),
            Look::Red => ("#ffffff", "#dc2626"),
            Look::RedWord => ("#ef4444", "#0b0c0e"),
        };
        format!(
            "{base} font-weight:500; cursor:pointer; color:{fg}; background:{bg}; \
             border-left:1px solid #2a2c31; user-select:none;"
        )
    };

    rsx! {
        div {
            // Layout-critical: state it inline so the controls always lay out
            // as one N-column row regardless of whether the Tailwind grid
            // utilities survived the consumer's CSS purge. Without this the
            // children collapse to block rows and overlap inside the caller's
            // fixed-height (`h-16 overflow-hidden`) frame.
            style: "display:grid; grid-template-columns:repeat({cols},minmax(0,1fr)); align-items:stretch; \
                    width:100%; height:100%; background:#0b0c0e; overflow:hidden;",

            // Arm + Record — recording environments only (playback surfaces
            // pass `show_recording: false`).
            if show_recording {
                // Arm Button — arms/disarms the selected tracks in the active song
                div {
                    style: if armed { cls(Look::Red) } else { cls(Look::Idle) },
                    onclick: move |_| {
                        on_arm_toggle.call(());
                    },
                    ArmIcon { size: icon, color: "currentColor" }
                    if !icons_only { "Arm" }
                }

                // Record Button — toggles recording into the active song's project
                div {
                    style: if recording { cls(Look::Red) } else { cls(Look::RedWord) },
                    onclick: move |_| {
                        on_record_toggle.call(());
                    },
                    RecordIcon { size: icon, color: "currentColor" }
                    if recording {
                        "Recording ({armed_track_count} armed)"
                    } else {
                        "Record ({armed_track_count} armed)"
                    }
                }
            }

                        // Back Button — while playing too: a press mid-song is exactly
            // when back a section is wanted.
            div {
                style: cls(Look::Idle),
                onclick: move |_| on_back.call(()),
                BackIcon { size: icon, color: "currentColor" }
                if !icons_only { "Back" }
            }

            // Play/Pause Button
            div {
                style: if playing { cls(Look::On) } else { cls(Look::Idle) },
                onclick: move |_| {
                    on_play_pause.call(());
                },
                if playing {
                    PauseIcon { size: icon, color: "currentColor" }
                } else {
                    PlayIcon { size: icon, color: "currentColor" }
                }
                if !icons_only {
                    if playing { "Pause" } else { "Play" }
                }
            }

            // Record, in Loop's place (see `record_in_loop`).
            if record_in_loop {
                div {
                    style: if recording { cls(Look::Red) } else { cls(Look::RedWord) },
                    onclick: move |_| {
                        on_record_toggle.call(());
                    },
                    // A plain round dot: Blitz draws no SVG `<circle>`,
                    // which is all the Circle icon is.
                    div {
                        style: format!(
                            "width:{dot}px; height:{dot}px; border-radius:50%; background:{fill}; flex:none;",
                            dot = icon * 7 / 10,
                            fill = if recording { "#ffffff" } else { "#ef4444" },
                        ),
                    }
                    if !icons_only {
                        if recording { "Recording" } else { "Record" }
                    }
                }
            } else {
            // Loop Button
            div {
                style: if looping { cls(Look::On) } else { cls(Look::Idle) },
                onclick: move |_| {
                    on_loop_toggle.call(());
                },
                LoopIcon { size: icon, color: "currentColor" }
                if !icons_only { "Loop" }
            }
            }

                        // Advance Button — while playing too.
            div {
                style: cls(Look::Idle),
                onclick: move |_| on_forward.call(()),
                ForwardIcon { size: icon, color: "currentColor" }
                if !icons_only { "Advance" }
            }
        }
    }
}

/// What a transport cell shows it is.
#[derive(Clone, Copy)]
enum Look {
    /// At rest.
    Idle,
    /// Switched on (playing, looping): light, with dark words.
    On,
    /// Recording, or armed: red.
    Red,
    /// Record, not recording: red words on the dark.
    RedWord,
}
