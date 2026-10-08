//! The macro bar, for fingers — the prototype's `dock/MacroBar.tsx` on the
//! rig's macros (`Rig::macros`). Two rows of eight flush cells; each reads
//! one of three ways (`MacroKnobView::scale`):
//!
//!   level     0–100%, 0 is off. Gate, Pre-Comp, Pitch, Drive, Comp, Mod,
//!             Motion, Boost, Clarity.
//!   wet       0–200%, its normal level in the middle; past it the dry falls
//!             away (hatched). Delay, Reverb, Space.
//!   relative  ± around its rest. Gain, Tone, Width, Output.
//!
//! Slide sideways anywhere on a cell to turn it (a full sweep is 2.5 cells);
//! double-tap puts it back to rest; a tap on a knob with sub-macros (▾)
//! opens them — a panel dropping over the main area, a row per block.
//! A glow rises behind a knob while its effect works, and the gate shows
//! the guitar's level against its threshold.

use std::collections::BTreeMap;
use std::time::Duration;

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{MacroChildView, MacroKnobView};
use signal_widgets::drag_bus::{DragBus, DragEvent};

use super::tokens::*;
use crate::state::RigViewState;

/// The bar's height with the rule under it: two 44pt rows, the hairline
/// between, plus 1. The setlist's header matches it.
pub const MACRO_BAR_H: u32 = 44 * 2 + 1 + 1;

/// What the rig is hearing, as the bar reacts to it (0..1 each).
#[derive(Clone, Copy, PartialEq, Default)]
struct Heard {
    input: f64,
    output: f64,
    squash: f64,
}

/// A knob's glow while its effect works, and the gate's meter.
fn reaction(id: &str, v: f64, h: Heard) -> (f64, Option<(f64, f64)>) {
    match id {
        "gate" => (0.0, Some((h.input, v))),
        "pre-comp" | "comp" => (h.squash * v, None),
        "drive" => (h.input * v, None),
        "boost" | "pitch" => (if v > 0.0 { h.input * 0.7 } else { 0.0 }, None),
        "gain" => (h.input * 0.35, None),
        "output" => (h.output * 0.3, None),
        _ => (0.0, None),
    }
}

/// The colours the bar draws grey (its label in ink, not lifted).
fn greyish(colour: &str) -> bool {
    matches!(
        colour.to_ascii_uppercase().as_str(),
        "#6B7280" | "#94A3B8" | "#E5E7EB" | "#FAFAF9" | "#F3F4F6" | "#D1D5DB" | "#F9FAFB" | "#E2E8F0" | "#F1F5F9" | "#CBD5E1"
    )
}

#[component]
pub fn TouchMacroBar(state: RigViewState) -> Element {
    let knobs = state.macros.read().clone();
    let mut open = use_signal(|| None::<String>);
    let heard = Heard {
        input: *state.in_level.read(),
        output: *state.out_level.read(),
        squash: (f64::from(*state.comp_gr_db.read()) / 12.0).clamp(0.0, 1.0),
    };
    let shown = open().and_then(|id| knobs.iter().find(|k| k.id == id).cloned());
    rsx! {
        div { style: "position: relative;",
            if let Some(k) = shown.filter(|k| !k.children.is_empty()) {
                Panel { knob: k, on_close: move |()| open.set(None) }
            }
            div { style: "display: grid; grid-template-columns: repeat(8, minmax(0, 1fr)); gap: 1px; background: #000;",
                for k in knobs.iter().cloned() {
                    Cell {
                        key: "{k.id}",
                        id: k.id.clone(),
                        label: k.label.clone(),
                        colour: k.color.clone(),
                        value: f64::from(k.value),
                        rest: f64::from(k.rest),
                        scale: k.scale.clone(),
                        more: !k.children.is_empty(),
                        open: open() == Some(k.id.clone()),
                        heard,
                        on_tap: {
                            let id = k.id.clone();
                            let more = !k.children.is_empty();
                            move |()| if more {
                                let now = open();
                                open.set(if now.as_deref() == Some(id.as_str()) { None } else { Some(id.clone()) });
                            }
                        },
                    }
                }
            }
        }
    }
}

/// A knob's sub-macros: a row per block (Delay: DLY 1, DLY 2), headed when
/// there are several.
#[component]
fn Panel(knob: MacroKnobView, on_close: EventHandler<()>) -> Element {
    let mut rows: BTreeMap<String, Vec<MacroChildView>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for c in &knob.children {
        if !rows.contains_key(&c.group) {
            order.push(c.group.clone());
        }
        rows.entry(c.group.clone()).or_default().push(c.clone());
    }
    let heads = order.len() > 1 && order.iter().any(|g| !g.is_empty());
    let cols = rows.values().map(Vec::len).max().unwrap_or(1);
    let template = if heads { format!("64px repeat({cols}, minmax(0, 1fr))") } else { format!("repeat({cols}, minmax(0, 1fr))") };
    let label_ink = lift(&knob.color);
    rsx! {
        div { style: "position: absolute; left: 0; right: 0; top: 100%; z-index: 5; background: #0d0d10; border-bottom: 2px solid {knob.color}; box-shadow: 0 16px 32px rgba(0,0,0,0.55);",
            div { style: "display: flex; align-items: center; gap: 10px; height: 36px; padding: 0 6px 0 12px;",
                span { style: "font-size: 12px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {label_ink};", "{knob.label}" }
                span { style: "flex: 1;" }
                button {
                    "aria-label": "Close {knob.label}",
                    style: "width: 44px; height: 44px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer;",
                    onclick: move |_| on_close.call(()),
                    svg { width: "12", height: "12", view_box: "0 0 12 12",
                        path { d: "M2 2l8 8M10 2l-8 8", stroke: INK_3, stroke_width: "1.6", stroke_linecap: "round" }
                    }
                }
            }
            div { style: "display: grid; grid-template-columns: {template}; gap: 1px; background: #000; border-top: 1px solid #000;",
                for g in order.iter() {
                    if heads {
                        span { key: "h{g}", style: "display: flex; align-items: center; padding: 0 10px; font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; white-space: nowrap; color: {INK_3}; background: #111114;", "{g}" }
                    }
                    for c in rows.get(g).cloned().unwrap_or_default() {
                        Cell {
                            key: "{c.id}",
                            id: c.id.clone(),
                            label: c.label.clone(),
                            colour: c.color.clone(),
                            value: f64::from(c.value),
                            rest: f64::from(c.rest),
                            scale: "relative".to_string(),
                            more: false,
                            open: false,
                            heard: Heard::default(),
                            on_tap: move |()| {},
                        }
                    }
                    if let Some(r) = rows.get(g) {
                        for k in r.len()..cols {
                            span { key: "pad{g}{k}", style: "background: #111114;" }
                        }
                    }
                }
            }
        }
    }
}

/// One macro: the whole cell is the control, drawn by its scale — a level
/// fills from the left (0 is off); a wet knob fills from the left with its
/// normal level marked in the middle and the stretch past it hatched; a
/// relative knob fills from the centre (its rest).
#[component]
fn Cell(
    id: String,
    label: String,
    colour: String,
    value: f64,
    rest: f64,
    scale: String,
    more: bool,
    open: bool,
    heard: Heard,
    on_tap: EventHandler<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let bus = DragBus::try_use();
    let active = use_signal(|| false);
    // While a finger turns it, the cell shows where the finger is (the rig
    // answers a moment later).
    let mut live = use_signal(|| None::<f64>);
    let mut width = use_signal(|| 120.0_f64);
    let mut last_tap = use_signal(|| false);
    let v = live().unwrap_or(value).clamp(0.0, 1.0);
    let (wet, relative) = (scale == "wet", scale == "relative");
    let offset = ((v - rest) * 200.0).round() as i32;
    let pct = (v * if wet { 200.0 } else { 100.0 }).round() as i32;
    let readout = if relative {
        if offset > 0 { format!("+{offset}") } else { format!("{offset}") }
    } else if pct == 0 {
        "off".to_string()
    } else if wet && pct == 200 {
        "wet".to_string()
    } else {
        format!("{pct}%")
    };
    let quiet = if relative { offset == 0 } else { pct == 0 };
    let (lo, hi) = if relative { (v.min(rest), v.max(rest)) } else { (0.0, v) };
    let fill_hi = if wet { hi.min(0.5) } else { hi };
    let (glow, meter) = reaction(&id, v, heard);
    let label_ink = if greyish(&colour) { INK_2.to_string() } else { lift(&colour) };
    let value_ink = if quiet { INK_3.to_string() } else if wet && v > 0.5 { lift(&colour) } else { INK.to_string() };
    let bg = if open { "#1c1c22" } else if active() { "#18181d" } else { "#111114" };
    let underline = if open { format!("box-shadow: inset 0 -2px 0 {colour};") } else { String::new() };
    let fill = format!("color-mix(in oklab, {colour} {}%, transparent)", if active() { 32 } else { 22 });
    let hatch = format!(
        "repeating-linear-gradient(135deg, color-mix(in oklab, {colour} {a}%, transparent) 0px, color-mix(in oklab, {colour} {a}%, transparent) 4px, color-mix(in oklab, {colour} {b}%, transparent) 4px, color-mix(in oklab, {colour} {b}%, transparent) 8px)",
        a = if active() { 46 } else { 36 },
        b = if active() { 24 } else { 16 }
    );
    let glow_bg = format!("color-mix(in oklab, {colour} {}%, transparent)", (glow * 26.0).round() as i32);
    let mid_line = if wet { ("4px", "#45454d") } else { ("8px", RULE_STRONG) };
    let send = {
        let rig = rig.clone();
        let id = id.clone();
        move |x: f64| {
            if let Some(r) = rig.clone() {
                let id = id.clone();
                spawn(async move {
                    let _ = r.set_macro(id, x as f32).await;
                });
            }
        }
    };
    rsx! {
        div {
            role: "slider",
            "aria-label": "{label}",
            "aria-valuetext": "{readout}",
            style: "position: relative; height: 44px; overflow: hidden; background: {bg}; {underline} touch-action: none; user-select: none; cursor: ew-resize;",
            onmounted: move |e| {
                let el = e.data();
                spawn(async move {
                    if let Ok(r) = el.get_client_rect().await {
                        width.set(r.width().max(40.0));
                    }
                });
            },
            onpointerdown: {
                let send = send.clone();
                let rig = rig.clone();
                let id = id.clone();
                move |e: PointerEvent| {
                    // The press is the cell's to drag: no panning under it.
                    e.prevent_default();
                    e.stop_propagation();
                    // A second tap within 300 ms: back to rest.
                    if last_tap() {
                        last_tap.set(false);
                        live.set(None);
                        if let Some(r) = rig.clone() {
                            let id = id.clone();
                            spawn(async move {
                                let _ = r.reset_macro(id).await;
                            });
                        }
                        return;
                    }
                    let x0 = e.client_coordinates().x;
                    let v0 = v;
                    let span = width() * 2.5;
                    let send = send.clone();
                    let Some(bus) = bus else { return };
                    let moved_cell = std::rc::Rc::new(std::cell::Cell::new(false));
                    let flag = moved_cell.clone();
                    bus.begin(move |ev| {
                    // Signals are handles: copied in, set through the copy.
                    let (mut active, mut live, mut last_tap) = (active, live, last_tap);
                    match ev {
                        DragEvent::Move { x, .. } => {
                            if !flag.get() && (x - x0).abs() < 4.0 {
                                return;
                            }
                            if !flag.get() {
                                active.set(true);
                            }
                            flag.set(true);
                            let nv = (v0 + (x - x0) / span).clamp(0.0, 1.0);
                            live.set(Some(nv));
                            send(nv);
                        }
                        DragEvent::End => {
                            active.set(false);
                            live.set(None);
                            if !flag.get() {
                                // A tap: wait out a second one before it opens.
                                last_tap.set(true);
                                spawn(async move {
                                    architect::platform::sleep(Duration::from_millis(300)).await;
                                    if last_tap() {
                                        last_tap.set(false);
                                        on_tap.call(());
                                    }
                                });
                            }
                        }
                    }
                    });
                }
            },
            // Its effect at work: a glow that rises and falls with it.
            if glow > 0.01 {
                span { style: "position: absolute; left: 0; right: 0; top: 0; bottom: 0; background: {glow_bg};" }
            }
            // The gate's meter along the foot: the level, the threshold, green while open.
            if let Some((level, mark)) = meter {
                span { style: "position: absolute; left: 0; right: 0; bottom: 0; height: 4px; background: rgba(0,0,0,0.55);",
                    span { style: "position: absolute; left: 0; top: 0; bottom: 0; width: {pct_of(level)}%; background: {pick(level >= mark, LIVE, GATE_SHUT)};" }
                    span { style: "position: absolute; top: -2px; bottom: 0; left: calc({pct_of(mark)}% - 1px); width: 2px; background: #f4f4f5;" }
                }
            }
            // The middle: rest for a relative knob, the normal level for a wet one.
            if scale != "level" {
                span { style: "position: absolute; top: {mid_line.0}; bottom: {mid_line.0}; left: 50%; width: 1px; background: {mid_line.1};" }
            }
            span { style: "position: absolute; top: 0; bottom: 0; left: {pct_of(lo)}%; width: {pct_of(fill_hi - lo)}%; background: {fill};" }
            if wet && v > 0.5 {
                span { style: "position: absolute; top: 0; bottom: 0; left: 50%; width: {pct_of(v - 0.5)}%; background: {hatch};" }
            }
            if !quiet {
                span { style: "position: absolute; top: 0; bottom: 0; left: calc({pct_of(v)}% - 1px); width: 2px; background: {colour};" }
            }
            span { style: "position: relative; height: 100%; display: flex; flex-direction: column; justify-content: center; gap: 3px; padding: 0 8px; box-sizing: border-box;",
                span { style: "min-width: 0; display: flex; align-items: center; gap: 3px; font-size: 11px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: {label_ink}; white-space: nowrap; overflow: hidden;",
                    span { style: "overflow: hidden; text-overflow: ellipsis;", "{label}" }
                    if more {
                        svg { key: "{open}", width: "8", height: "5", view_box: "0 0 8 5", style: "flex-shrink: 0; opacity: 0.8; {pick(open, ROTATED, NOTHING)}",
                            path { d: "M1 1l3 3 3-3", fill: "none", stroke: "{label_ink}", stroke_width: "1.4", stroke_linecap: "round", stroke_linejoin: "round" }
                        }
                    }
                }
                span { style: "font-size: {pick(active(), 16, 13)}px; line-height: 1; font-weight: 700; color: {value_ink}; font-variant-numeric: tabular-nums;",
                    "{readout}"
                }
            }
        }
    }
}

/// 0..1 as a whole percentage, for a style.
fn pct_of(x: f64) -> f64 {
    (x.clamp(0.0, 1.0) * 1000.0).round() / 10.0
}

const ROTATED: &str = "transform: rotate(180deg);";
/// The gate's level while it is shut.
const GATE_SHUT: &str = "#52525b";
