//! The arpeggiator panel.
//!
//! Same shape as the velocity panel: propless root, sink from context,
//! inline styles only, and a live preview of what a commit would write.
//!
//! The preview is a piano roll rather than a bar strip, because pitch is
//! what an arpeggiator is *for* — a velocity bar chart would show you
//! nothing about whether the pattern climbs, and "does this climb the way
//! I want" is the only question you ask an arp.

use std::sync::Arc;

use dioxus::prelude::*;
use expression_editor_tools::arp::{ArpNote, ArpSession, Direction, PPQ};
use expression_editor_tools::{ArpSink, DemoArpSink};

use crate::drag::Slider;
use crate::velocity_panel::{BUTTON, Chip, PanelStyles, Section};

/// Cloneable handle so an arp sink can live in Dioxus context.
#[derive(Clone)]
pub struct ArpSinkHandle(pub Arc<dyn ArpSink>);

impl ArpSinkHandle {
    pub fn new(sink: impl ArpSink) -> Self {
        Self(Arc::new(sink))
    }
}

impl PartialEq for ArpSinkHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// The rates offered, as fractions of a quarter note.
///
/// A list rather than a free slider: arps are played against a grid, and
/// a continuous rate control mostly produces values you'd never want.
/// Triplets are included because they're half of what an arpeggiator is
/// used for and awkward to reach any other way.
const RATES: [(&str, f64); 7] = [
    ("1/4", 1.0),
    ("1/4T", 2.0 / 3.0),
    ("1/8", 0.5),
    ("1/8T", 1.0 / 3.0),
    ("1/16", 0.25),
    ("1/16T", 1.0 / 6.0),
    ("1/32", 0.125),
];

/// The MIDI arpeggiator.
#[component]
pub fn ArpPanel() -> Element {
    let sink = use_context_provider(|| {
        try_consume_context::<ArpSinkHandle>()
            .unwrap_or_else(|| ArpSinkHandle::new(DemoArpSink::default()))
    });

    let mut session = use_signal(|| sink.0.open().unwrap_or_else(|_| ArpSession::default()));
    let mut status = use_signal(|| match sink.0.open() {
        Ok(s) => format!("{} chords", s.chords().len()),
        Err(e) => e,
    });

    let notes = use_memo(move || session.read().resolve());
    let notes = notes();

    let reopen = {
        let sink = sink.clone();
        move |_| match sink.0.open() {
            Ok(s) => {
                status.set(format!("{} chords", s.chords().len()));
                session.set(s);
            }
            Err(e) => status.set(e),
        }
    };

    // The arp stays explicit rather than live, unlike the velocity panel.
    //
    // Velocity editing is in-place and idempotent: committing twice with
    // the same parameters writes the same velocities. Arpeggiating is
    // generative and destructive — it DELETES the source chord and
    // replaces it with a stream of new notes. Doing that on every slider
    // frame would consume its own output, and after the first commit
    // there is no chord left to re-arpeggiate.
    //
    // The piano-roll preview is the live part; the write is the button.
    let apply = {
        let sink = sink.clone();
        move |_| {
            let result = sink.0.commit(&session.read());
            match result {
                Ok(n) => status.set(format!("wrote {n} notes")),
                Err(e) => status.set(e),
            }
        }
    };

    let rate = session.read().rate_ppq();
    let gate = session.read().gate();
    let octaves = session.read().octaves();
    let ratchet = session.read().ratchet();
    let direction = session.read().direction();

    rsx! {
        PanelStyles {}

        div {
            style: "display:flex; flex-direction:column; gap:10px; height:100%; padding:12px; overflow-y:auto; background:var(--background, #121212); color:var(--foreground, #e8e8e8); font-family:system-ui, sans-serif; font-size:12px;",

            div {
                style: "display:flex; align-items:baseline; justify-content:space-between; gap:8px;",
                div { style: "font-size:13px; font-weight:600; letter-spacing:0.04em;", "MIDI ARPEGGIATOR" }
                div { style: "opacity:0.65;", "{notes.len()} notes" }
            }

            PianoRoll { notes: notes.clone() }

            div {
                style: "display:flex; flex-wrap:wrap; gap:4px; opacity:0.7;",
                for label in session.read().chord_labels() {
                    div {
                        style: "padding:2px 7px; border-radius:3px; background:var(--secondary, #232323); border:1px solid var(--border, #333);",
                        "{label}"
                    }
                }
            }

            Section { title: "DIRECTION",
                div {
                    style: "display:flex; flex-wrap:wrap; gap:4px;",
                    for d in Direction::ALL {
                        Chip {
                            label: d.label().to_string(),
                            active: direction == d,
                            onclick: move |_| session.write().set_direction(d),
                        }
                    }
                }
            }

            Section { title: "RATE",
                div {
                    style: "display:flex; flex-wrap:wrap; gap:4px;",
                    for (label, fraction) in RATES {
                        Chip {
                            label: label.to_string(),
                            // Compared with a tolerance because the
                            // triplet rates are non-terminating: PPQ*2/3
                            // never equals a value round-tripped through
                            // the session exactly.
                            active: (rate - PPQ * fraction).abs() < 0.5,
                            onclick: move |_| session.write().set_rate_ppq(PPQ * fraction),
                        }
                    }
                }
            }

            Section { title: "SHAPE",
                Row {
                    label: "Octaves".to_string(),
                    value: format!("{octaves}"),
                    Slider {
                        value: f64::from(octaves),
                        min: 1.0,
                        max: 4.0,
                        step: 1.0,
                        on_change: move |v: f64| session.write().set_octaves(v.round() as u8),
                    }
                }
                Row {
                    label: "Gate".to_string(),
                    value: format!("{:.0}%", gate * 100.0),
                    Slider {
                        value: gate,
                        min: 0.05,
                        max: 2.0,
                        step: 0.05,
                        on_change: move |v| session.write().set_gate(v),
                    }
                }
                Row {
                    label: "Ratchet".to_string(),
                    value: if ratchet <= 1 { "off".to_string() } else { format!("×{ratchet}") },
                    Slider {
                        value: f64::from(ratchet),
                        min: 1.0,
                        max: 8.0,
                        step: 1.0,
                        on_change: move |v: f64| session.write().set_ratchet(v.round() as u8),
                    }
                }
            }

            div {
                style: "display:flex; align-items:center; gap:8px; margin-top:auto; padding-top:8px; border-top:1px solid var(--border, #2c2c2c);",
                button {
                    style: "{BUTTON} background:var(--primary, #d2691e); color:var(--primary-foreground, #fff); border-color:transparent;",
                    onclick: apply,
                    "Apply"
                }
                button { style: "{BUTTON}", onclick: reopen, "Reload" }
                div { style: "flex:1; text-align:right; opacity:0.7;", "{status}" }
            }
        }
    }
}

/// The arpeggio as a piano roll.
///
/// Pitch on the vertical, time on the horizontal, scaled to whatever the
/// arp actually spans — so the shape of the pattern is legible without
/// needing to know what bar you're on. Drawn with positioned `div`s
/// rather than a canvas or SVG, for the same reason as the velocity
/// preview: it has to render under Blitz.
#[component]
fn PianoRoll(notes: Vec<ArpNote>) -> Element {
    const HEIGHT: f64 = 150.0;

    // An empty roll still draws its box — a panel whose preview vanishes
    // when you set a rate it doesn't like looks broken rather than empty.
    let (lo, hi, start, end) = notes.iter().fold(
        (u8::MAX, u8::MIN, f64::MAX, f64::MIN),
        |(lo, hi, s, e), n| {
            (
                lo.min(n.pitch),
                hi.max(n.pitch),
                s.min(n.start_ppq),
                e.max(n.end_ppq()),
            )
        },
    );
    let span = (end - start).max(1.0);
    // Pad the pitch range so the top and bottom notes aren't flush
    // against the edges, and so a single-pitch arp still gets a sane row
    // height instead of dividing by zero.
    let (lo, hi) = (f64::from(lo) - 1.0, f64::from(hi) + 1.0);
    let rows = (hi - lo).max(1.0);
    let row_h = HEIGHT / rows;

    rsx! {
        div {
            style: "position:relative; height:{HEIGHT}px; border-radius:5px; background:var(--muted, #171717); border:1px solid var(--border, #333); overflow:hidden;",
            for (i, n) in notes.iter().enumerate() {
                div {
                    key: "{i}",
                    style: "position:absolute; left:{(n.start_ppq - start) / span * 100.0}%; width:{(n.length_ppq / span * 100.0).max(0.4)}%; bottom:{(f64::from(n.pitch) - lo) / rows * 100.0}%; height:{row_h.max(3.0)}px; border-radius:2px; background:var(--primary, #d2691e); opacity:{0.45 + f64::from(n.velocity) / 127.0 * 0.55};",
                }
            }
        }
    }
}

/// A label / control / readout row.
#[component]
fn Row(label: String, value: String, children: Element) -> Element {
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:8px;",
            div { style: "width:64px; opacity:0.75;", "{label}" }
            {children}
            div { style: "width:44px; text-align:right; opacity:0.75;", "{value}" }
        }
    }
}
