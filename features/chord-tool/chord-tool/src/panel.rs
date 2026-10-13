//! The chord panel.
//!
//! One component, two homes: `examples/panel.rs` runs it in a desktop
//! window for iteration, and `fts-extensions` will register it as a
//! REAPER panel rendered by Blitz. Propless is what allows both.
//!
//! Layout follows `ChordGun`: seven columns, one per scale degree, and down
//! each column the whole chord vocabulary. Out-of-scale types are shown
//! dimmed rather than hidden — you reach for a chromatic chord on
//! purpose, and a grid that drops rows as the key changes reflows under
//! you.
//!
//! ## Interactions
//!
//! Modelled on TK's `ChordGun`, where click is safe and modifiers commit:
//!
//! - **click** — select and preview
//! - **ctrl+click** — cycle the inversion
//! - **shift+click** — commit (insert; wired to the DAW later)
//! - **alt+click** — append to the progression
//!
//! That escalation matters once insert is real: the cheap gesture must
//! not be the destructive one.
//!
//! No music theory lives here. Grid, labels, pitches and inversions all
//! come from `keyflow::chord::palette`.

use architect_ui::components::{Select, SelectContent, SelectItem};
use dioxus::prelude::*;
use keyflow::chord::palette::{ChordCandidate, grid};
use keyflow::key::Key;
use keyflow::primitives::MusicalNote;

use crate::sink::{LogSink, SinkHandle};

const FTS_THEME: &str = architect_ui::THEME_CSS;
const APP_TAILWIND: &str = include_str!("../../../../apps/desktop/assets/tailwind-signal.css");

/// Without `html,body{height:100%}` an `h-full` root resolves against
/// `auto` and the panel collapses to its content. Mirrors the reset
/// `apps/desktop` injects.
const HOST_RESET: &str = r"
html, body { margin:0; padding:0; height:100%; width:100%; overflow:hidden; }
* { box-sizing: border-box; }
body > div { height:100%; }
";

const TONICS: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];
const MAJOR_NUMERALS: [&str; 7] = ["I", "ii", "iii", "IV", "V", "vi", "vii°"];
const MINOR_NUMERALS: [&str; 7] = ["i", "ii°", "III", "iv", "v", "VI", "VII"];

/// One chord placed in the progression, with how long it lasts.
///
/// `beats` and `repeats` are separate because they mean different things
/// when it comes out as MIDI: a chord held for four beats is one note
/// event, the same chord repeated four times is four.
#[derive(Clone, PartialEq)]
struct Slot {
    label: String,
    notes: Vec<u8>,
    beats: u32,
    repeats: u32,
}

fn build_key(tonic: &str, minor: bool) -> Key {
    let root = MusicalNote::from_string(tonic).unwrap_or_else(MusicalNote::c);
    if minor {
        Key::minor(root)
    } else {
        Key::major(root)
    }
}

/// Cell styling carries three facts at once: whether it's the selection,
/// whether it's in the key, and (via ring) whether it's sounding.
fn cell_class(selected: bool, in_scale: bool, playing: bool) -> String {
    let base =
        "rounded-md border px-1 py-1.5 text-xs text-center cursor-pointer truncate select-none";
    let tone = match (selected, in_scale) {
        (true, _) => "bg-primary text-primary-foreground border-primary font-semibold",
        (false, true) => "bg-secondary text-secondary-foreground border-border hover:bg-accent",
        // Out of key: present but visibly an outsider.
        (false, false) => {
            "bg-transparent text-muted-foreground/50 border-border/40 hover:bg-accent/40 italic"
        }
    };
    let ring = if playing { " ring-2 ring-ring" } else { "" };
    format!("{base} {tone}{ring}")
}

#[component]
pub fn ChordToolPanel() -> Element {
    let tonic = use_signal(|| "C".to_string());
    let mode = use_signal(|| "major".to_string());
    let mut octave = use_signal(|| 4i32);
    let mut inversion = use_signal(|| 0usize);
    let mut selected = use_signal(|| None::<(usize, usize)>);
    let mut playing = use_signal(Vec::<u8>::new);
    let mut status =
        use_signal(|| "click to preview · ctrl invert · shift insert · alt add".to_string());
    let mut progression = use_signal(Vec::<Slot>::new);
    // Whoever mounted the panel decides where chords go. Nothing
    // provided (the standalone example) falls back to a sink that
    // reports instead of writing.
    let sink = use_context_provider(|| {
        try_consume_context::<SinkHandle>().unwrap_or_else(|| SinkHandle::new(LogSink))
    });

    let minor = mode() == "minor";
    let key = build_key(&tonic(), minor);
    let columns = grid(&key);
    let numerals = if minor {
        MINOR_NUMERALS
    } else {
        MAJOR_NUMERALS
    };

    rsx! {
        document::Style { {FTS_THEME} }
        document::Style { {APP_TAILWIND} }
        document::Style { {HOST_RESET} }

        div { class: "h-full w-full flex flex-col gap-3 p-3 bg-background text-foreground overflow-hidden",

            // ── Controls ────────────────────────────────────────────
            div { class: "flex items-center gap-3 flex-wrap shrink-0",
                span { class: "text-[11px] uppercase tracking-wide text-muted-foreground", "Key" }
                div { class: "w-24",
                    Select {
                        value: tonic,
                        placeholder: "C".to_string(),
                        SelectContent {
                            for (i, t) in TONICS.iter().enumerate() {
                                SelectItem { value: t.to_string(), index: i, "{t}" }
                            }
                        }
                    }
                }
                div { class: "w-28",
                    Select {
                        value: mode,
                        placeholder: "major".to_string(),
                        SelectContent {
                            SelectItem { value: "major".to_string(), index: 0, "major" }
                            SelectItem { value: "minor".to_string(), index: 1, "minor" }
                        }
                    }
                }
                span { class: "text-[11px] uppercase tracking-wide text-muted-foreground ml-2", "Oct" }
                button {
                    class: "px-2 py-1 rounded-md border border-border bg-card hover:bg-accent text-sm",
                    onclick: move |_| octave -= 1, "−"
                }
                span { class: "text-sm tabular-nums w-4 text-center", "{octave}" }
                button {
                    class: "px-2 py-1 rounded-md border border-border bg-card hover:bg-accent text-sm",
                    onclick: move |_| octave += 1, "+"
                }
                span { class: "text-[11px] uppercase tracking-wide text-muted-foreground ml-2", "Inv" }
                span { class: "text-sm tabular-nums w-4 text-center", "{inversion}" }
            }

            // ── Degree × variation grid ─────────────────────────────
            div { class: "flex-1 min-h-0 flex gap-1.5 overflow-auto",
                for (d, column) in columns.iter().enumerate() {
                    div { class: "flex-1 min-w-0 flex flex-col gap-1",
                        div { class: "rounded-md border border-border bg-card px-1 py-1.5 text-center shrink-0",
                            div { class: "text-sm font-bold leading-tight", "{numerals[d]}" }
                            div { class: "text-[10px] text-muted-foreground leading-tight",
                                {column.first().map(|c: &ChordCandidate| c.label.clone()).unwrap_or_default()}
                            }
                        }
                        for (v, chord) in column.iter().enumerate() {
                            {
                                let chord = chord.clone();
                                let sink = sink.clone();
                                let is_sel = selected() == Some((d, v));
                                let notes_now = chord.notes_inverted(octave(), inversion());
                                let is_playing = !notes_now.is_empty() && playing() == notes_now;
                                let class = cell_class(is_sel, chord.in_scale, is_playing);
                                rsx! {
                                    div {
                                        class: "{class}",
                                        title: if chord.in_scale { "" } else { "outside the key" },
                                        onclick: move |e: Event<MouseData>| {
                                            let m = e.modifiers();
                                            selected.set(Some((d, v)));
                                            // ctrl cycles the inversion before voicing
                                            let inv = if m.ctrl() {
                                                let next = inversion() + 1;
                                                inversion.set(next);
                                                next
                                            } else {
                                                inversion()
                                            };
                                            let notes = chord.notes_inverted(octave(), inv);
                                            if m.alt() {
                                                progression.write().push(Slot {
                                                    label: chord.label.clone(),
                                                    notes: notes.clone(),
                                                    beats: 4,
                                                    repeats: 1,
                                                });
                                                status.set(format!("added {} to progression", chord.label));
                                            } else if m.shift() {
                                                // The commit gesture — this is the one
                                                // that writes.
                                                match sink.0.insert(&notes, 4) {
                                                    Ok(()) => status
                                                        .set(format!("inserted {}", chord.label)),
                                                    Err(why) => status.set(why),
                                                }
                                            } else {
                                                sink.0.preview(&notes);
                                                status.set(format!("{} {notes:?}", chord.label));
                                            }
                                            playing.set(notes);
                                        },
                                        "{chord.label}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Progression strip ───────────────────────────────────
            div { class: "shrink-0 flex flex-col gap-1",
                div { class: "flex items-center gap-2",
                    span { class: "text-[11px] uppercase tracking-wide text-muted-foreground", "Progression" }
                    if !progression().is_empty() {
                        button {
                            class: "px-2 py-0.5 rounded border border-primary bg-primary text-primary-foreground text-[11px]",
                            onclick: {
                                let sink = sink;
                                move |_| {
                                // Insert in order; each call advances the
                                // cursor, so the progression lays itself out.
                                let slots = progression();
                                let mut placed = 0usize;
                                for slot in &slots {
                                    for _ in 0..slot.repeats.max(1) {
                                        match sink.0.insert(&slot.notes, slot.beats) {
                                            Ok(()) => placed += 1,
                                            Err(why) => { status.set(why); return; }
                                        }
                                    }
                                }
                                status.set(format!("inserted {placed} chords"));
                                }
                            },
                            "insert all"
                        }
                        button {
                            class: "px-2 py-0.5 rounded border border-border bg-card hover:bg-accent text-[11px]",
                            onclick: move |_| { progression.write().clear(); },
                            "clear"
                        }
                    }
                }
                div { class: "flex gap-1.5 flex-wrap min-h-[42px] rounded-md border border-dashed border-border/60 p-1.5",
                    if progression().is_empty() {
                        span { class: "text-[11px] text-muted-foreground/70 self-center px-1",
                            "alt-click a chord to add it here"
                        }
                    }
                    for (i, slot) in progression().iter().enumerate() {
                        div { class: "rounded-md border border-border bg-card px-2 py-1 flex items-center gap-2",
                            div { class: "flex flex-col leading-tight",
                                span { class: "text-xs font-semibold", "{slot.label}" }
                                span { class: "text-[10px] text-muted-foreground",
                                    "{slot.beats}b ×{slot.repeats}"
                                }
                            }
                            div { class: "flex flex-col",
                                button {
                                    class: "text-[10px] px-1 leading-none text-muted-foreground hover:text-foreground",
                                    onclick: move |_| {
                                        if let Some(s) = progression.write().get_mut(i) { s.beats += 1; }
                                    },
                                    "+"
                                }
                                button {
                                    class: "text-[10px] px-1 leading-none text-muted-foreground hover:text-foreground",
                                    onclick: move |_| {
                                        if let Some(s) = progression.write().get_mut(i) {
                                            s.beats = s.beats.saturating_sub(1).max(1);
                                        }
                                    },
                                    "−"
                                }
                            }
                            button {
                                class: "text-[11px] text-muted-foreground hover:text-destructive px-1",
                                onclick: move |_| { progression.write().remove(i); },
                                "×"
                            }
                        }
                    }
                }
            }

            div { class: "shrink-0 border-t border-border pt-2 text-[11px] font-mono text-muted-foreground truncate",
                "{status()}"
            }
        }
    }
}
