//! midicore's reusable MIDI-monitor display.
//!
//! [`MidiMonitorPanel`] renders a rolling list of [`MidiEvent`]s — colour-coded
//! by [`MidiKind`], one line each via the event's `Display` — so any rig's
//! remote GUI shows "what MIDI is arriving" identically. Inline styles only
//! (Blitz-safe), no external CSS.

use dioxus::prelude::*;
use midicore_proto::{MidiEvent, MidiKind};

/// Accent colour for a MIDI message category (drives the monitor's per-line
/// dot + text tint). Public so callers can match the taxonomy elsewhere.
pub fn kind_color(kind: MidiKind) -> &'static str {
    match kind {
        MidiKind::NoteOn => "#22c55e",
        MidiKind::NoteOff => "#64748b",
        MidiKind::ControlChange => "#3b82f6",
        MidiKind::Aftertouch => "#a855f7",
        MidiKind::PitchBend => "#eab308",
        MidiKind::Program => "#ec4899",
        MidiKind::System => "#71717a",
    }
}

/// A MIDI monitor panel: newest events at the bottom, colour-coded, with a
/// running total. `events` is oldest-first (as [`MidiMonitor::recent`] returns).
/// `count` is the running total seen (0 if unknown). `title` labels the panel.
#[component]
pub fn MidiMonitorPanel(events: Vec<MidiEvent>, count: u64, title: String) -> Element {
    rsx! {
        div { style: "display:flex; flex-direction:column; min-height:0; gap:4px;",
            div { style: "display:flex; align-items:baseline; gap:8px;",
                span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "{title}" }
                span { style: "font-size:10px; color:#52525b;", "{count} total" }
            }
            div { style: "flex:1; min-height:80px; max-height:220px; overflow:auto; background:#0c0c0e; border:1px solid #1c1c1f; border-radius:6px; padding:6px; font-family:ui-monospace,Menlo,monospace; font-size:11px;",
                if events.is_empty() {
                    div { style: "color:#52525b; padding:4px;", "waiting for MIDI…" }
                } else {
                    for (i, ev) in events.iter().enumerate() {
                        {
                            let color = kind_color(ev.kind());
                            rsx!{ div {
                                key: "{i}",
                                style: "display:flex; align-items:center; gap:6px; padding:1px 2px; white-space:pre;",
                                span { style: "width:7px; height:7px; border-radius:999px; background:{color}; flex:none;" }
                                span { style: "color:#d4d4d8;", "{ev}" }
                            } }
                        }
                    }
                }
            }
        }
    }
}
