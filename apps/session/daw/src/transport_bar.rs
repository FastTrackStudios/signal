//! The transport: go to start, play/stop, record, loop, go to end, and
//! where the song is — bar.beat, clock time, tempo, key.
//!
//! It lives in the bottom bar, beside the views, when the arrangement is
//! showing ([`crate::shell::BottomBar`]'s context) — big, for a finger
//! (`big`). The Performance view has its own buttons along its foot
//! (`session-ui`'s `TransportControlBar`); this one is for editing, where
//! the numbers matter as much as the buttons.

use dioxus::prelude::*;

use crate::engine::{Move, Transport, transport};
use crate::studio::StudioSession;

const RULE: &str = "#2a2c31";
const BG: &str = "#17181b";
const TEXT: &str = "#e5e7eb";
const DIM: &str = "#8b9099";
const PLAY: &str = "#34d399";
const REC: &str = "#ef4444";
const LOOP: &str = "#3aa0ff";

/// The bar.
#[cfg(feature = "native")]
#[component]
pub fn TransportBar(
    /// Sized for a finger: the bottom bar's.
    #[props(default)]
    big: bool,
) -> Element {
    let mut reading = use_signal(crate::engine::Reading::default);
    // A read a frame, published only when something a person can see
    // changed: the clock to the hundredth, the flags, the tempo.
    dioxus_native::use_window_event(move |event, _| {
        if !matches!(event, winit::event::WindowEvent::RedrawRequested) {
            return;
        }
        let Some(now) = Transport::shared().map(Transport::reading) else {
            return;
        };
        publish(&mut reading, now);
    });
    rsx! { TransportBarView { reading: reading(), big } }
}

/// A new reading, published only when something a person can see changed:
/// the clock to the hundredth, the flags, the tempo.
fn publish(reading: &mut Signal<crate::engine::Reading>, now: crate::engine::Reading) {
    let was = *reading.peek();
    if (now.at - was.at).abs() >= 0.01
        || now.playing != was.playing
        || now.looping != was.looping
        || now.recording != was.recording
        || (now.bpm - was.bpm).abs() > 1e-6
    {
        reading.set(now);
    }
}

/// The bar in a browser: the same view, polled on a timer (a page has no
/// redraw event to hang it on).
#[cfg(feature = "web")]
#[component]
pub fn WebTransportBar(
    /// Sized for a finger: the bottom bar's.
    #[props(default)]
    big: bool,
) -> Element {
    let mut reading = use_signal(crate::engine::Reading::default);
    use_future(move || async move {
        loop {
            if let Some(now) = Transport::shared().map(Transport::reading) {
                publish(&mut reading, now);
            }
            gloo_timers::future::TimeoutFuture::new(33).await;
        }
    });
    rsx! { TransportBarView { reading: reading(), big } }
}

/// What the bar shows, for a reading — as much as the bar has room for
/// (see [`crate::shell::Density`]).
#[component]
fn TransportBarView(reading: crate::engine::Reading, big: bool) -> Element {
    use crate::shell::Density;
    let session: StudioSession = use_context();
    let density = crate::shell::use_density();
    let r = reading;
    let mut stop_menu = use_signal(|| false);
    // Open above the bottom bar's buttons, below a small bar's.
    let menu_side = if big { "bottom:100%;" } else { "top:100%;" };
    // Counted from the song, as the ruler counts.
    let (bar, beat) = crate::ruler::song_bar_beat(
        &session.project.tempo,
        crate::ruler::song_start_of(&session.project),
        r.at,
    );
    let key = crate::arrangement::key_at(&session.project, r.at)
        .map(|name| short_key(&name))
        .unwrap_or_else(|| "—".to_owned());
    let minutes = (r.at / 60.0).floor();
    let seconds = r.at - minutes * 60.0;
    let clock = format!("{minutes:.0}:{seconds:05.2}");
    let bpm = tempo_text(r.bpm);
    let ends = if big {
        density == Density::Full
    } else {
        density != Density::Narrow
    };
    // A finger's sizes, or a mouse's.
    let (tall, card, beat_px, clock_px, pill_px) = if big {
        (40, 40, 20, 13, 15)
    } else {
        (26, 30, 15, 12, 12)
    };
    // Big, it is part of the bottom bar: flat, the bar's full height, its
    // parts between hairlines rather than cards. Small, each is a card.
    let (row, card_look) = if big {
        (
            "height:100%; flex:none; display:flex; align-items:stretch;",
            String::new(),
        )
    } else {
        (
            "height:100%; flex:none; display:flex; align-items:center; gap:4px;",
            format!("border-radius:6px; background:#0b0c0e; border:1px solid {RULE};"),
        )
    };
    let high = if big {
        "100%".to_owned()
    } else {
        format!("{tall}px")
    };
    let card_high = if big {
        "100%".to_owned()
    } else {
        format!("{card}px")
    };
    rsx! {
        div {
            style: row,
            // A press here is a button, not a drag of the window the bar
            // it sits in is the title bar of.
            onmousedown: move |event| event.stop_propagation(),
            if ends {
                Button { title: "Go to start", on: false, color: TEXT, glyph: Glyph::Home, big,
                    onpress: move |()| transport(Move::Home, 0.0) }
            }
            // Right-click: what Stop does with the cursor.
            div {
                style: "position:relative; height:100%; flex:none; display:flex; align-items:center;",
                oncontextmenu: move |e| {
                    e.prevent_default();
                    stop_menu.set(true);
                },
                Button { title: "Play / stop (right-click: what stop does)", on: r.playing, color: PLAY, big,
                    glyph: if r.playing { Glyph::Stop } else { Glyph::Play },
                    onpress: move |()| transport(Move::PlayStop, 0.0) }
                if stop_menu() {
                    // A press outside closes it.
                    div {
                        style: "{crate::shell::SCRIM} z-index:40;",
                        onclick: move |_| stop_menu.set(false),
                    }
                    div {
                        style: "position:absolute; left:0; {menu_side} z-index:41; min-width:210px; \
                                padding:6px; background:{BG}; border:1px solid {RULE}; \
                                border-radius:10px; display:flex; flex-direction:column; gap:2px; \
                                box-shadow:0 10px 30px rgba(0,0,0,0.5);",
                        for each in crate::play_return::Stop::ALL {
                            button {
                                key: "{each.slug()}",
                                style: stop_option(crate::play_return::get() == each),
                                onclick: move |_| {
                                    crate::play_return::set(each);
                                    stop_menu.set(false);
                                },
                                "{each.label()}"
                            }
                        }
                    }
                }
            }
            Button { title: "Record", on: r.recording, color: REC, glyph: Glyph::Record, big,
                onpress: move |()| transport(Move::ToggleRecord, 0.0) }
            Button { title: "Loop", on: r.looping, color: LOOP, glyph: Glyph::Loop, big,
                onpress: move |()| transport(Move::ToggleLoop, 0.0) }
            if ends {
                Button { title: "Go to end", on: false, color: TEXT, glyph: Glyph::End, big,
                    onpress: move |()| transport(Move::End, 0.0) }
            }
            if big {
                Hairline {}
            }
            crate::bar_step::Stepper { big }
                        if big {
                Hairline {}
            } else {
                div { style: "width:4px;" }
            }
            // Where the song is: the bar and beat first, the clock under it
            // in weight — the bar is what an editor counts in. Narrow, the
            // clock is the tooltip.
            div {
                title: "{clock}",
                                style: "height:{high}; box-sizing:border-box; display:flex; align-items:center; \
                        gap:10px; padding:0 12px; {card_look} font-family:ui-monospace, monospace;",
                span { style: "font-size:{beat_px}px; font-weight:600; color:{TEXT};", "{bar}.{beat}" }
                if density != Density::Narrow {
                    span { style: "font-size:{clock_px}px; color:{DIM};", "{clock}" }
                }
            }
            if density == Density::Full {
                // Tempo and key as one card, each a small label over its
                // value — what the song is doing where the playhead is.
                                if big {
                    Hairline {}
                }
                div {
                    style: "height:{card_high}; box-sizing:border-box; display:flex; align-items:center; \
                            {card_look}",
                    Reading { label: "BPM", value: bpm, mono: true, big }
                    div { style: "width:1px; margin:5px 0; background:{RULE};" }
                    Reading { label: "KEY", value: key, big }
                }
            } else {
                // One small pill: `68 · F`, a note glyph for what the
                // first number is.
                                if big {
                    Hairline {}
                }
                div {
                    title: "Tempo {bpm} BPM, key {key}",
                    style: "height:{high}; box-sizing:border-box; display:flex; align-items:center; \
                            gap:6px; padding:0 12px; {card_look} white-space:nowrap;",
                    NoteGlyph {}
                    span { style: "font-size:{pill_px}px; color:{TEXT}; font-family:ui-monospace, monospace;", "{bpm}" }
                    span { style: "font-size:{pill_px}px; color:{DIM};", "·" }
                    span { style: "font-size:{pill_px}px; font-weight:700; color:{TEXT};", "{key}" }
                }
            }
        }
    }
}

/// One row of the stop menu: ticked when it is the behaviour in force.
fn stop_option(on: bool) -> String {
    let (ink, wash) = if on {
        (PLAY, "#34d3991f")
    } else {
        (TEXT, "transparent")
    };
    format!(
        "text-align:left; padding:7px 10px; border:none; border-radius:6px; font-size:12px; \
         cursor:pointer; color:{ink}; background:{wash};"
    )
}

/// A hairline between the big transport's parts.
#[component]
fn Hairline() -> Element {
    rsx! {
        div { style: "flex:none; width:1px; margin:12px 0; background:{RULE};" }
    }
}

/// One labelled number in the tempo/key card.
#[component]
fn Reading(
    label: &'static str,
    value: String,
    #[props(default)] mono: bool,
    #[props(default)] big: bool,
) -> Element {
    let (label_px, value_px) = if big { (9, 17) } else { (8, 13) };
    let family = if mono {
        "ui-monospace, monospace"
    } else {
        "system-ui, sans-serif"
    };
    rsx! {
        div {
            style: "display:flex; flex-direction:column; justify-content:center; align-items:flex-start; \
                    padding:0 10px; min-width:34px;",
            span {
                style: "font-size:{label_px}px; line-height:{label_px + 1}px; letter-spacing:0.8px; font-weight:700; color:{DIM};",
                "{label}"
            }
            span {
                style: "font-size:{value_px}px; line-height:{value_px + 2}px; font-weight:700; color:{TEXT}; font-family:{family};",
                "{value}"
            }
        }
    }
}

/// A tempo as a person reads it: `68`, `72.5` — no `.0`.
pub(crate) fn tempo_text(bpm: f64) -> String {
    if bpm <= 0.0 {
        "—".to_owned()
    } else if (bpm - bpm.round()).abs() < 0.05 {
        format!("{:.0}", bpm.round())
    } else {
        format!("{bpm:.1}")
    }
}

/// A quarter note, filled — the tempo pill's unit glyph.
#[component]
fn NoteGlyph() -> Element {
    rsx! {
        svg {
            width: "12",
            height: "14",
            view_box: "0 0 14 18",
            fill: "none",
            ellipse {
                cx: "4", cy: "14.5", rx: "3.6", ry: "2.7",
                fill: TEXT,
                transform: "rotate(-18 4 14.5)",
            }
            path {
                d: "M7.3 14.2 V1.5",
                stroke: TEXT, stroke_width: "1.6", stroke_linecap: "round",
            }
        }
    }
}

/// The KEY track's item name ("F major", "Eb minor", "F dorian"), shortened
/// for the transport bar: the tonic alone for major ("F"), the tonic with
/// a trailing `m` for minor ("Fm"), and the mode's first three letters for
/// anything else ("F dor") — spelling out "major" every time the song
/// hasn't changed key is wasted width the pill doesn't have.
pub(crate) fn short_key(name: &str) -> String {
    let Some(key) = session::key::parse_key(name) else {
        return name.to_owned();
    };
    let full = session::key::format_key(&key);
    let Some((root, mode)) = full.split_once(' ') else {
        return full;
    };
    match mode {
        "major" => root.to_owned(),
        "minor" => format!("{root}m"),
        other => format!("{root} {}", &other[..other.len().min(3)]),
    }
}

/// What a transport button shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Glyph {
    Home,
    Play,
    Stop,
    Record,
    Loop,
    End,
}

#[component]
fn Button(
    title: &'static str,
    on: bool,
    color: &'static str,
    glyph: Glyph,
    onpress: EventHandler<()>,
    #[props(default)] big: bool,
) -> Element {
    let fg = if on { color } else { DIM };
    let border = if on { color } else { RULE };
    // A finger's button — flat, the bar's full height, lit by a wash of
    // its colour — or a mouse's small card.
    let (look, icon) = if big {
        let wash = if on {
            format!("{color}24")
        } else {
            "transparent".to_owned()
        };
        (
            format!("width:54px; height:100%; border:none; border-radius:0; background:{wash};"),
            22,
        )
    } else {
        (
            format!(
                "width:30px; height:24px; border-radius:5px; border:1px solid {border}; background:#0b0c0e;"
            ),
            16,
        )
    };
    rsx! {
        button {
            title,
            style: "{look} padding:0; display:flex; align-items:center; justify-content:center; cursor:pointer;",
            onclick: move |_| onpress.call(()),
            svg {
                width: "{icon}",
                height: "{icon}",
                view_box: "0 0 24 24",
                match glyph {
                    Glyph::Home => rsx! {
                        path { d: "M6 5 V19", stroke: fg, stroke_width: "2.2", fill: "none" }
                        path { d: "M19 5 L9 12 L19 19 Z", fill: fg }
                    },
                    Glyph::Play => rsx! { path { d: "M7 4 L20 12 L7 20 Z", fill: fg } },
                    Glyph::Stop => rsx! { path { d: "M6 6 H18 V18 H6 Z", fill: fg } },
                    Glyph::Record => rsx! { circle { cx: "12", cy: "12", r: "7", fill: fg } },
                    Glyph::Loop => rsx! {
                        path {
                            d: "M4 12 A6 6 0 0 1 10 6 H18 M15 3 L18 6 L15 9 M20 12 A6 6 0 0 1 14 18 H6 M9 21 L6 18 L9 15",
                            stroke: fg, stroke_width: "2", fill: "none",
                            stroke_linecap: "round", stroke_linejoin: "round",
                        }
                    },
                    Glyph::End => rsx! {
                        path { d: "M5 5 L15 12 L5 19 Z", fill: fg }
                        path { d: "M18 5 V19", stroke: fg, stroke_width: "2.2", fill: "none" }
                    },
                }
            }
        }
    }
}
