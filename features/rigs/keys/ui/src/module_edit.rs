//! **Module Edit** — the play surface for one module, laid out like a synth.
//!
//! A module IS the engine, so this is the whole instrument: its Source, one
//! Filter section, four Envelopes (ENV 1 drives the Amp, ENV 2 the Filter),
//! four LFOs, and the per-module Tone / Vibrato / Ambience / Effects. Every
//! value here belongs to the selected module — switching A/B/C/D swaps the
//! entire surface.
//!
//! Layout (Vital/Serum-ish: big graphs left-to-right, knob rows beneath):
//!
//! ```text
//! ┌ SOURCE ────────┬ FILTER ─────────────┬ ENVELOPES  1 2 3 4 ┐
//! │ patch + browse │ response curve      │ envelope graph      │
//! │ on/off · level │ cutoff res env drive│ D A H D S R         │
//! ├ TONE ──────────┼ VIBRATO ────────────┼ LFOS       1 2 3 4 ┤
//! │ knobs          │ knobs               │ shape + rate/depth  │
//! ├ AMBIENCE ──────┴ EFFECTS ────────────┴─────────────────────┤
//! ```

use dioxus::prelude::*;
use signal_keys_proto::{KeysLayerDetail, KeysMacro};

use crate::fader::Fader;
use crate::graphs::{Adsr, EnvelopeGraph, FilterCurve};
use crate::knob::Knob;

/// Panel chrome — every section shares it so the page reads as one surface.
#[component]
pub(crate) fn Panel(
    title: String,
    accent: String,
    /// Optional right-hand slot (selectors, badges).
    #[props(default)]
    trailing: Option<Element>,
    #[props(default = false)] lit: bool,
    children: Element,
) -> Element {
    rsx! {
        div {
            // `flex: 1` so a panel fills the cell it was given — in the macro
            // band the cards are flex items, and a card that hugs its content
            // leaves its knobs floating in a box that is wider than they are.
            // Grid layouts (the module surface, the layer zoom) ignore it.
            style: "display: flex; flex: 1; flex-direction: column; gap: 12px; padding: 14px 16px; \
                    border: 1px solid #1c1c21; border-radius: 12px; background: #0d0d10; \
                    min-width: 0;",
            div { style: "display: flex; align-items: center; gap: 8px; min-height: 20px;",
                span {
                    style: format!(
                        "width: 6px; height: 6px; border-radius: 999px; background: {};",
                        if lit { accent.clone() } else { "#3f3f46".to_string() },
                    ),
                }
                span {
                    style: "font-size: 10px; font-weight: 700; letter-spacing: 0.1em; \
                            text-transform: uppercase; color: #a1a1aa;",
                    "{title}"
                }
                div { style: "flex: 1;" }
                if let Some(t) = trailing { {t} }
            }
            {children}
        }
    }
}

/// A row of knobs for one macro group.
#[component]
pub(crate) fn KnobRow(
    macros: Vec<KeysMacro>,
    accent: String,
    on_change: EventHandler<(String, f32)>,
) -> Element {
    rsx! {
        // Knobs are 62px tiles around a 44px dial, so the gutter reads wider
        // than the number: 10px here lands ~28px between dials.
        div { style: "display: flex; column-gap: 10px; row-gap: 14px; flex-wrap: wrap;",
            for m in macros.iter() {
                Knob {
                    key: "{m.id}",
                    label: m.name.clone(),
                    value: m.value,
                    min: m.min,
                    max: m.max,
                    unit: m.unit.clone(),
                    live: m.live,
                    bipolar: m.bipolar,
                    // Frequencies are heard in octaves, so they are dialled in
                    // octaves: cutoffs, LFO and vibrato rates.
                    log: m.unit == "Hz" && m.min > 0.0,
                    accent: accent.clone(),
                    on_change: {
                        let id = m.id.clone();
                        move |v: f32| on_change.call((id.clone(), v))
                    },
                }
            }
        }
    }
}

/// 1/2/3/4 selector used by the envelope + LFO sections.
#[component]
fn SlotTabs(count: u32, selected: Signal<u32>, accent: String) -> Element {
    let mut selected = selected;
    rsx! {
        div { style: "display: flex; gap: 2px; padding: 2px; background: #0a0a0c; \
                      border: 1px solid #1c1c21; border-radius: 7px;",
            for i in 0..count {
                button {
                    key: "{i}",
                    style: format!(
                        "appearance: none; border: none; border-radius: 5px; min-width: 20px; \
                         padding: 2px 7px; font-size: 9px; font-weight: 700; background: {}; color: {};",
                        if selected() == i { "#101821" } else { "transparent" },
                        if selected() == i { accent.clone() } else { "#52525b".to_string() },
                    ),
                    onclick: move |_| selected.set(i),
                    "{i + 1}"
                }
            }
        }
    }
}

/// The module surface.
#[component]
pub fn ModuleEdit(
    detail: KeysLayerDetail,
    accent: String,
    module: u32,
    on_macro: EventHandler<(String, f32)>,
    on_enabled: EventHandler<bool>,
    on_gain: EventHandler<f32>,
) -> Element {
    let env_slot = use_signal(|| 0u32);
    let lfo_slot = use_signal(|| 0u32);

    let group = |name: &str| -> Vec<KeysMacro> {
        detail.macros.iter().filter(|m| m.group == name).cloned().collect()
    };
    let val = |id: &str| detail.macros.iter().find(|m| m.id == id).map(|m| m.value).unwrap_or(0.0);
    let live = |id: &str| detail.macros.iter().find(|m| m.id == id).map(|m| m.live).unwrap_or(false);

    let here = detail.modules.iter().find(|m| m.index == module).cloned();
    let enabled = here.as_ref().map(|m| m.enabled).unwrap_or(true);
    let mod_gain = here.as_ref().map(|m| m.gain_db).unwrap_or(0.0);
    let has_source = !detail.patch.is_empty();

    // The selected envelope / LFO.
    let env_id = format!("env{}", env_slot() + 1);
    let env = Adsr {
        attack_ms: val(&format!("{env_id}.attack")),
        decay_ms: val(&format!("{env_id}.decay")),
        sustain: val(&format!("{env_id}.sustain")),
        release_ms: val(&format!("{env_id}.release")),
    };
    let env_group = group(&format!("Env {}", env_slot() + 1));
    let env_binding = match env_slot() {
        0 => "→ Amp",
        1 => "→ Filter",
        _ => "unassigned",
    };
    let lfo_group = group(&format!("LFO {}", lfo_slot() + 1));

    rsx! {
        div {
            style: "flex: 1; min-height: 0; overflow: auto; padding: 16px 18px 24px; display: grid; \
                    grid-template-columns: minmax(240px, 0.7fr) minmax(320px, 1.3fr) minmax(340px, 1.6fr); \
                    gap: 16px; align-content: start;",

            // ── SOURCE ──────────────────────────────────────────────────
            Panel {
                title: "Source".to_string(),
                accent: accent.clone(),
                lit: has_source && enabled,
                trailing: rsx! {
                    span { style: "font-size: 9px; color: #52525b;",
                        {format!("module {}", here.as_ref().map(|m| m.slot.clone()).unwrap_or_default())}
                    }
                },
                div { style: "display: flex; flex-direction: column; gap: 12px;",
                    // Patch name + its origin are one block — they stay tight
                    // to each other and take the panel gap as a group.
                    div { style: "display: flex; flex-direction: column; gap: 4px;",
                        span {
                            style: format!(
                                "font-size: 13px; font-weight: 600; line-height: 1.25; color: {};",
                                if has_source { "#e4e4e7" } else { "#52525b" },
                            ),
                            if has_source { "{detail.patch}" } else { "— empty —" }
                        }
                        // The preset the layer was opened from, when there is
                        // one — the module itself is its soundsource.
                        if !detail.preset.is_empty() {
                            span { style: "font-size: 10px; color: #71717a; line-height: 1.2;",
                                "from {detail.preset}"
                            }
                        }
                    }
                    div { style: "display: flex; align-items: center; gap: 8px;",
                        button {
                            style: format!(
                                "appearance: none; border: 1px solid {}; border-radius: 6px; \
                                 padding: 4px 12px; background: {}; color: {}; font-size: 9px; \
                                 font-weight: 700; letter-spacing: 0.08em;",
                                if enabled { "#166534" } else { "#3f3f46" },
                                if enabled { "#0f2417" } else { "#131316" },
                                if enabled { "#4ade80" } else { "#52525b" },
                            ),
                            onclick: move |_| on_enabled.call(!enabled),
                            if enabled { "ON" } else { "OFF" }
                        }
                        div { style: "flex: 1;" }
                        Fader {
                            db: mod_gain,
                            height_px: 40,
                            accent: accent.clone(),
                            dimmed: !enabled,
                            on_change: move |db: f32| on_gain.call(db),
                        }
                    }
                    KnobRow {
                        macros: group("Source"),
                        accent: accent.clone(),
                        on_change: move |(id, v)| on_macro.call((id, v)),
                    }
                }
            }

            // ── FILTER ──────────────────────────────────────────────────
            Panel {
                title: "Filter".to_string(),
                accent: accent.clone(),
                lit: live("filter.cutoff"),
                div { style: "display: flex; flex-direction: column; gap: 12px;",
                    FilterCurve {
                        cutoff_hz: val("filter.cutoff"),
                        resonance: val("filter.reso"),
                        accent: accent.clone(),
                        live: live("filter.cutoff"),
                        flat: true,
                        on_change: move |(what, v): (&'static str, f32)| {
                            let id = if what == "cutoff" { "filter.cutoff" } else { "filter.reso" };
                            on_macro.call((id.to_string(), v));
                        },
                    }
                    KnobRow {
                        macros: group("Filter"),
                        accent: accent.clone(),
                        on_change: move |(id, v)| on_macro.call((id, v)),
                    }
                }
            }

            // ── ENVELOPES ───────────────────────────────────────────────
            Panel {
                title: "Envelopes".to_string(),
                accent: accent.clone(),
                lit: live("env1.attack"),
                trailing: rsx! {
                    div { style: "display: flex; align-items: center; gap: 6px;",
                        span { style: "font-size: 9px; color: #52525b;", "{env_binding}" }
                        SlotTabs { count: 4, selected: env_slot, accent: accent.clone() }
                    }
                },
                div { style: "display: flex; flex-direction: column; gap: 12px;",
                    EnvelopeGraph {
                        title: format!("Env {}", env_slot() + 1),
                        adsr: env,
                        accent: accent.clone(),
                        live: live(&format!("{env_id}.attack")),
                        flat: true,
                        on_change: {
                            let env_id = env_id.clone();
                            move |(seg, v): (&'static str, f32)| {
                                on_macro.call((format!("{env_id}.{seg}"), v));
                            }
                        },
                    }
                    KnobRow {
                        macros: env_group,
                        accent: accent.clone(),
                        on_change: move |(id, v)| on_macro.call((id, v)),
                    }
                }
            }

            // ── TONE ────────────────────────────────────────────────────
            Panel {
                title: "Tone".to_string(),
                accent: accent.clone(),
                KnobRow {
                    macros: group("Tone"),
                    accent: accent.clone(),
                    on_change: move |(id, v)| on_macro.call((id, v)),
                }
            }

            // ── VIBRATO ─────────────────────────────────────────────────
            Panel {
                title: "Vibrato".to_string(),
                accent: accent.clone(),
                KnobRow {
                    macros: group("Vibrato"),
                    accent: accent.clone(),
                    on_change: move |(id, v)| on_macro.call((id, v)),
                }
            }

            // ── LFOS ────────────────────────────────────────────────────
            Panel {
                title: "LFOs".to_string(),
                accent: accent.clone(),
                trailing: rsx! { SlotTabs { count: 4, selected: lfo_slot, accent: accent.clone() } },
                div { style: "display: flex; flex-direction: column; gap: 12px;",
                    LfoShape {
                        shape: val(&format!("lfo{}.shape", lfo_slot() + 1)),
                        depth: val(&format!("lfo{}.depth", lfo_slot() + 1)),
                        accent: accent.clone(),
                    }
                    KnobRow {
                        macros: lfo_group,
                        accent: accent.clone(),
                        on_change: move |(id, v)| on_macro.call((id, v)),
                    }
                }
            }

            // ── AMBIENCE ────────────────────────────────────────────────
            Panel {
                title: "Ambience".to_string(),
                accent: accent.clone(),
                KnobRow {
                    macros: group("Ambience"),
                    accent: accent.clone(),
                    on_change: move |(id, v)| on_macro.call((id, v)),
                }
            }

            // ── EFFECTS ─────────────────────────────────────────────────
            Panel {
                title: "Effects".to_string(),
                accent: accent.clone(),
                KnobRow {
                    macros: group("Effects"),
                    accent: accent.clone(),
                    on_change: move |(id, v)| on_macro.call((id, v)),
                }
            }
        }
    }
}

/// The LFO's waveform, drawn from its shape + depth (0 sine … 4 random).
#[component]
fn LfoShape(shape: f32, depth: f32, accent: String) -> Element {
    const W: f64 = 260.0;
    const H: f64 = 72.0;
    const PAD: f64 = 6.0;
    let d = depth.clamp(0.0, 1.0).max(0.12) as f64;
    let kind = shape.round().clamp(0.0, 4.0) as i32;
    let mut path = String::new();
    let steps = 120;
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let phase = t * 2.0; // two cycles across the panel
        let v = match kind {
            0 => (phase * std::f64::consts::TAU).sin(),
            1 => 1.0 - 4.0 * ((phase % 1.0) - 0.5).abs(), // triangle
            2 => if (phase % 1.0) < 0.5 { 1.0 } else { -1.0 }, // square
            3 => 2.0 * (phase % 1.0) - 1.0,                    // saw
            _ => ((phase * 12.9898).sin() * 43758.545).fract() * 2.0 - 1.0, // random
        };
        let x = PAD + t * (W - 2.0 * PAD);
        let y = H / 2.0 - v * d * (H / 2.0 - PAD);
        path.push_str(&format!("{} {x:.1} {y:.1} ", if i == 0 { "M" } else { "L" }));
    }
    rsx! {
        svg {
            // Stretch to the panel — the window is wide, the graph should be.
            width: "100%", height: "{H}", view_box: "0 0 {W} {H}",
            preserve_aspect_ratio: "none",
            line {
                x1: "{PAD}", y1: "{H / 2.0}", x2: "{W - PAD}", y2: "{H / 2.0}",
                stroke: "#1b1b1f", stroke_width: "1",
            }
            path { d: "{path}", fill: "none", stroke: "{accent}", stroke_width: "2", stroke_linejoin: "round" }
        }
    }
}
