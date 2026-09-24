//! The macro bar — a strip of macro knobs across the stage view, directly
//! above the footswitch grid in Profile and Setlist modes.
//!
//! A port of the legacy desktop app's macro bar
//! (`apps/desktop/src/signal_views/macro_bar.rs`: `MacroBar` / `MacroCell` /
//! `DropdownPanel` / `SubMacroDropdown` / `DualRowDropdown`), look and
//! behaviour kept: a label in the knob's colour with a ▾ when it has a
//! panel, a mini knob, a mono readout; hovering a cell drops its panel of
//! child knobs, bridged across the gap so the pointer can cross into it.
//!
//! What moved: the engine is the rig's (`signal_guitar::macros`) — the bar
//! draws [`MacroKnobView`]s and sends [`set_macro`](RigClient::set_macro),
//! so a footswitch or MIDI can turn the same knobs. And Blitz has no
//! Tailwind named groups for us (the app's sheet is compiled, and
//! `group-hover/macro` is not in it) and no `window.innerWidth`: hover is a
//! pair of `onmouseenter`/`onmouseleave` signals, the panel's entrance a
//! keyframe animation, and its nudge inside the window comes from where its
//! cell sits in the row.

use dioxus::prelude::*;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{MacroChildView, MacroKnobView};
use signal_widgets::arc::{angle_for_value, arc_path, arc_point, SENSITIVITY};
use signal_widgets::drag_bus::{DragBus, DragEvent};

use crate::param_writer::ParamWriter;

/// Hold a macro's panel open whatever the pointer does — a picture of the
/// hover state (`rig_shot`'s `RIG_SHOT_MACRO`). Provided by the host.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MacroPanelOpen(pub Option<String>);

/// A knob label with no colour of its own (legacy `#94a3b8`).
const MUTED: &str = "#94a3b8";
/// The panel's entrance: the legacy `opacity-0 scale-95 translate-y-[-4px]`
/// → shown, 150 ms ease-out, as a keyframe (a panel mounts on hover, so
/// there is no earlier state to transition from).
const CSS: &str = "@keyframes macro-drop{from{opacity:0;transform:scale(0.95) translateY(-4px)}to{opacity:1;transform:scale(1) translateY(0)}}\
@keyframes macro-drop-up{from{opacity:0;transform:scale(0.95) translateY(4px)}to{opacity:1;transform:scale(1) translateY(0)}}";

/// The bar's wire: knob moves coalesced (a drag is an edit per pointer
/// event), pads sent straight, and the bar's state for optimistic moves.
#[derive(Clone)]
struct Wire {
    rig: Option<RigClient>,
    writer: ParamWriter,
    macros: Signal<Vec<MacroKnobView>>,
}

impl Wire {
    /// Move knob `id` — here at once, on the rig as fast as it answers.
    fn set(&self, id: &str, value: f32) {
        let mut macros = self.macros;
        macros.with_mut(|ks| {
            for k in ks.iter_mut() {
                if k.id == id {
                    k.value = value;
                }
                for c in k.children.iter_mut().filter(|c| c.id == id) {
                    c.value = value;
                }
            }
        });
        self.writer.set(id, "", value);
    }

    fn pad(&self, id: &str, on: bool) {
        let mut macros = self.macros;
        macros.with_mut(|ks| {
            for c in ks.iter_mut().flat_map(|k| k.children.iter_mut()).filter(|c| c.id == id) {
                c.bypassed = !on;
            }
        });
        if let Some(r) = self.rig.clone() {
            let id = id.to_string();
            spawn(async move {
                let _ = r.set_macro_pad(id, on).await;
            });
        }
    }
}

/// When a dual-row panel's Type or Time link is on, the knob in the other
/// row that follows `id` — the legacy `linked_mirror`, unchanged.
#[must_use]
pub fn linked_mirror(id: &str, prefix: &str, type_linked: bool, time_linked: bool) -> Option<String> {
    if type_linked {
        let (t1, t2) = (format!("{prefix}-type1"), format!("{prefix}-type2"));
        if id == t1 {
            return Some(t2);
        }
        if id == t2 {
            return Some(t1);
        }
    }
    if time_linked {
        let (t1, t2) = (format!("{prefix}-time1"), format!("{prefix}-time2"));
        if id == t1 {
            return Some(t2);
        }
        if id == t2 {
            return Some(t1);
        }
    }
    None
}

/// A child's readout, in the rig's own units.
#[must_use]
pub fn child_readout(c: &MacroChildView) -> String {
    let v = c.param;
    let pick = |names: &[&str]| names.get(v.round().max(0.0) as usize).copied().unwrap_or("—").to_string();
    match c.fmt.as_str() {
        "db" => crate::control::level_fmt(v),
        "db_gain" => format!("{v:+.1} dB"),
        "hz" => crate::control::cut_fmt(v),
        "ms" if v >= 1000.0 => format!("{:.2} s", v / 1000.0),
        "ms" if v < 10.0 => format!("{v:.1} ms"),
        "ms" => format!("{v:.0} ms"),
        "s" => format!("{v:.2} s"),
        "verb_s" => crate::control::decay_fmt_for(c.aux, 0.0)(v),
        "div" => pick(&crate::control::DIV_LABELS),
        "pct" => format!("{:.0}%", v * 100.0),
        "ratio" => format!("{v:.1}:1"),
        "delay_style" => pick(&crate::control::DELAY_ALGOS),
        "verb_algo" => pick(&crate::control::VERB_ALGOS),
        "semitones" => signed(v.round() as i32),
        "interval" => interval_label(v.round() as i32),
        _ => format!("{:.0}%", c.value * 100.0),
    }
}

/// `+12`, `−12`, `0` — a real minus, so the column of intervals lines up.
fn signed(n: i32) -> String {
    match n {
        0 => "0".to_string(),
        n if n > 0 => format!("+{n}"),
        n => format!("\u{2212}{}", -n),
    }
}

/// The Ice machine's interval menu (the TimeLine MX's): −12..−1, ±25/50
/// cents, +1..+12, +19, +24, then Free.
fn interval_label(i: i32) -> String {
    match i {
        0..=11 => signed(i - 12),
        12 => "\u{2212}50c".into(),
        13 => "\u{2212}25c".into(),
        14 => "+25c".into(),
        15 => "+50c".into(),
        16..=27 => signed(i - 15),
        28 => "+19".into(),
        29 => "+24".into(),
        _ => "Free".into(),
    }
}

// ============================================================================
// MacroBar — horizontal strip of macro knobs
// ============================================================================

#[component]
pub fn MacroBar(
    macros: Signal<Vec<MacroKnobView>>,
    /// Open the panels upward (no room below: the grid is short or gone).
    #[props(default)]
    drop_up: bool,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let forced = use_hook(|| try_consume_context::<MacroPanelOpen>().and_then(|o| o.0));
    use_context_provider(|| {
        let rig = rig.clone();
        let send = rig.clone();
        Wire {
            rig,
            writer: ParamWriter::new(
                move |(id, _, value)| {
                    let r = send.clone();
                    Box::pin(async move {
                        if let Some(r) = r {
                            let _ = r.set_macro(id, value).await;
                        }
                    })
                },
                |task| {
                    spawn(task);
                },
            ),
            macros,
        }
    });
    let knobs = macros.read().clone();
    if knobs.is_empty() {
        return rsx! {};
    }
    let count = knobs.len();
    let at = |id: &str| knobs.iter().position(|k| k.id == id);

    rsx! {
        style { {CSS} }
        div {
            // Above the grid it overhangs: a positioned layer of its own.
            style: "flex-shrink: 0; padding: 8px 12px; position: relative; z-index: 40; \
                    border-bottom: 1px solid rgba(39,39,42,0.5); background: rgba(9,9,11,0.3);",
            div { style: "display: flex; align-items: flex-start; width: 100%;",
                for (i, k) in knobs.iter().enumerate() {
                    MacroCell {
                        key: "{k.id}",
                        knob: k.clone(),
                        index: i,
                        count,
                        // Clarity hangs under the Delay knob.
                        anchor_cells: at(&k.anchor).map_or(0, |a| i.saturating_sub(a)),
                        drop_up,
                        forced: forced.as_deref() == Some(k.id.as_str()),
                    }
                }
            }
        }
    }
}

// ============================================================================
// MacroCell — one knob cell; hovering it drops its panel
// ============================================================================

#[component]
fn MacroCell(
    knob: MacroKnobView,
    index: usize,
    count: usize,
    anchor_cells: usize,
    drop_up: bool,
    forced: bool,
) -> Element {
    let wire = use_context::<Wire>();
    // The pointer is over the cell or its panel (`group-hover/macro`).
    let mut hovered = use_signal(|| false);
    // Over the knob cell itself (its own `hover:` background).
    let mut over_cell = use_signal(|| false);
    // A knob in the cell or its panel is being dragged: the panel stays
    // while the pointer wanders off it.
    let dragging = use_signal(|| false);
    let has_children = !knob.children.is_empty();
    let open = forced || hovered() || dragging();
    let group_on = open;
    let color = if knob.color.is_empty() { MUTED.to_string() } else { knob.color.clone() };
    let at_rest = (knob.value - knob.rest).abs() < 0.005;
    let id = knob.id.clone();

    rsx! {
        div {
            style: "position: relative; flex: 1 1 0%; min-width: 0;",
            onmouseenter: move |_| hovered.set(true),
            onmouseleave: move |_| hovered.set(false),

            // Main knob cell
            div {
                style: format!(
                    "display: flex; flex-direction: column; align-items: center; gap: 2px; \
                     padding: 6px 0; border-radius: 12px; cursor: pointer; \
                     border: 1px solid transparent; background: {};",
                    if over_cell() { "rgba(39,39,42,0.4)" } else { "transparent" },
                ),
                onmouseenter: move |_| over_cell.set(true),
                onmouseleave: move |_| over_cell.set(false),

                // Label (above knob)
                div { style: "display: flex; align-items: center; justify-content: center; gap: 2px; width: 100%;",
                    span {
                        style: "font-size: 10px; font-weight: 500; max-width: 56px; overflow: hidden; \
                                white-space: nowrap; color: {color};",
                        "{knob.label}"
                    }
                    if has_children {
                        span {
                            style: format!(
                                "font-size: 8px; transition: color 150ms; color: {};",
                                if group_on { "#a1a1aa" } else { "#52525b" },
                            ),
                            "\u{25BE}"
                        }
                    }
                }

                MiniKnob {
                    value: knob.value,
                    color: color.clone(),
                    bipolar: knob.bipolar,
                    spread: knob.style == "spread",
                    rest: knob.rest,
                    dragging,
                    on_change: {
                        let wire = wire.clone();
                        let id = id.clone();
                        move |v: f32| wire.set(&id, v)
                    },
                }

                // Value readout: grey at rest, bright once moved off it.
                span {
                    style: format!(
                        "font-size: 9px; font-family: ui-monospace, monospace; \
                         font-variant-numeric: tabular-nums; white-space: nowrap; color: {};",
                        if at_rest { "#a1a1aa" } else { "#e4e4e7" },
                    ),
                    "{knob.readout}"
                }
            }

            if has_children && open {
                DropdownPanel { index, count, anchor_cells, drop_up, still: forced,
                    match knob.layout.as_str() {
                        "dual" => rsx! {
                            DualRowDropdown {
                                prefix: knob.id.clone(),
                                headers: knob.headers.clone(),
                                children_knobs: knob.children.clone(),
                                dragging,
                            }
                        },
                        "grouped" => rsx! {
                            GroupedDropdown { children_knobs: knob.children.clone(), dragging }
                        },
                        _ => rsx! {
                            SubMacroDropdown { children_knobs: knob.children.clone(), dragging }
                        },
                    }
                }
            }
        }
    }
}

// ============================================================================
// DropdownPanel — the panel under a cell, kept inside the window
// ============================================================================

/// The panel and its hover bridge. The legacy panel measured itself against
/// `window.innerWidth` (a webview eval) and nudged; Blitz has neither, so
/// the panel hangs from the side of its cell that keeps it in: the left
/// edge in the first third of the row, the right edge in the last third,
/// centred between — or, for a panel that belongs to another cell (Clarity,
/// under Delay), from that cell's left edge.
#[component]
fn DropdownPanel(
    index: usize,
    count: usize,
    anchor_cells: usize,
    drop_up: bool,
    /// Held open by the host: shown without the entrance (a picture is
    /// taken at time zero, where the entrance has not begun).
    still: bool,
    children: Element,
) -> Element {
    let place = if anchor_cells > 0 {
        // From the anchor cell's left edge, and no further right than the
        // end of the row.
        format!(
            "left: -{}%; max-width: {}%; overflow: hidden;",
            anchor_cells * 100,
            (count - index + anchor_cells) * 100,
        )
    } else if index * 3 < count {
        "left: 0;".to_string()
    } else if index * 3 >= count * 2 {
        "right: 0;".to_string()
    } else {
        "left: 50%; transform: translateX(-50%);".to_string()
    };
    let (edge, gap, anim, origin) = if drop_up {
        ("bottom: 100%;", "margin-bottom: 8px;", "macro-drop-up", "bottom center")
    } else {
        ("top: 100%;", "margin-top: 8px;", "macro-drop", "top center")
    };
    let anim = if still { "none".to_string() } else { format!("{anim} 150ms ease-out") };
    rsx! {
        // Invisible bridge: fills the gap between the cell and the panel.
        div { style: "position: absolute; {edge} left: 0; width: 100%; height: 8px;" }
        // The panel
        div { style: "position: absolute; {edge} {place} {gap} z-index: 50;",
            div {
                style: "border-radius: 12px; border: 1px solid rgba(63,63,70,0.8); \
                        background: rgba(24,24,27,0.95); padding: 8px; \
                        box-shadow: 0 20px 25px -5px rgba(0,0,0,0.5), 0 8px 10px -6px rgba(0,0,0,0.5); \
                        animation: {anim}; transform-origin: {origin};",
                {children}
            }
        }
    }
}

// ============================================================================
// SubMacroDropdown — a row of child knobs, with bypass pads
// ============================================================================

#[component]
fn SubMacroDropdown(children_knobs: Vec<MacroChildView>, dragging: Signal<bool>) -> Element {
    rsx! {
        div { style: "display: flex; align-items: flex-start; gap: 4px;",
            for child in children_knobs.iter() {
                ChildCell { key: "{child.id}", child: child.clone(), dragging, width: 68 }
            }
        }
    }
}

/// One knob of a panel: an ON/OFF pad when it has one, the label, the
/// knob, the readout.
#[component]
fn ChildCell(
    child: MacroChildView,
    dragging: Signal<bool>,
    /// Fixed width (`w-[68px]`), or 0 to fill a grid column.
    width: u32,
    /// Dual-row links: the knob in the other row that follows this one.
    #[props(default)]
    mirror: Option<String>,
) -> Element {
    let wire = use_context::<Wire>();
    let mut over = use_signal(|| false);
    let color = if child.color.is_empty() { MUTED.to_string() } else { child.color.clone() };
    let dim = child.has_pad && child.bypassed;
    let at_rest = child.steps > 0 || (child.value - child.rest).abs() < 0.005;
    let readout = child_readout(&child);
    let id = child.id.clone();
    let width_css = if width > 0 { format!("width: {width}px;") } else { String::new() };

    rsx! {
        div {
            style: format!(
                "{width_css} display: flex; flex-direction: column; align-items: center; gap: 4px; \
                 padding: 6px 0; border-radius: 8px; cursor: pointer; \
                 border: 1px solid transparent; background: {}; opacity: {};",
                if over() { "rgba(63,63,70,0.4)" } else { "transparent" },
                if dim { "0.4" } else { "1" },
            ),
            onmouseenter: move |_| over.set(true),
            onmouseleave: move |_| over.set(false),

            // Bypass pad
            if child.has_pad {
                {
                    let wire = wire.clone();
                    let id = id.clone();
                    let on = child.bypassed;
                    rsx! {
                        div {
                            style: if child.bypassed {
                                "width: 48px; height: 16px; border-radius: 6px; font-size: 8px; font-weight: 700; \
                                 display: flex; align-items: center; justify-content: center; \
                                 border: 1px solid #52525b; background: rgba(39,39,42,0.6); color: #52525b; \
                                 text-transform: uppercase; letter-spacing: 0.05em;".to_string()
                            } else {
                                format!(
                                    "width: 48px; height: 16px; border-radius: 6px; font-size: 8px; font-weight: 700; \
                                     display: flex; align-items: center; justify-content: center; \
                                     border: 1px solid transparent; background: {color}; color: #18181b; \
                                     text-transform: uppercase; letter-spacing: 0.05em;"
                                )
                            },
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                wire.pad(&id, on);
                            },
                            if child.bypassed { "OFF" } else { "ON" }
                        }
                    }
                }
            }

            // Label
            span {
                style: "font-size: 9px; font-weight: 500; max-width: 56px; overflow: hidden; \
                        white-space: nowrap; color: {color};",
                "{child.label}"
            }

            MiniKnob {
                value: child.value,
                color: color.clone(),
                spread: false,
                rest: child.rest,
                dragging,
                on_change: {
                    let id = id.clone();
                    move |v: f32| {
                        wire.set(&id, v);
                        if let Some(m) = mirror.as_deref() {
                            wire.set(m, v);
                        }
                    }
                },
            }

            // Value readout
            span {
                style: format!(
                    "font-size: 8px; font-family: ui-monospace, monospace; \
                     font-variant-numeric: tabular-nums; white-space: nowrap; color: {};",
                    if at_rest { "#71717a" } else { "#d4d4d8" },
                ),
                "{readout}"
            }
        }
    }
}

// ============================================================================
// DualRowDropdown — Delay / Reverb: a header row, a row per block, and
// links for Type and Time
// ============================================================================

#[component]
fn DualRowDropdown(
    /// Knob id prefix, `delay` or `reverb` — for the link mirror lookups.
    prefix: String,
    /// The five column headers.
    headers: Vec<String>,
    children_knobs: Vec<MacroChildView>,
    dragging: Signal<bool>,
) -> Element {
    let mut type_linked = use_signal(|| false);
    let mut time_linked = use_signal(|| false);
    // One row per block, in the columns the headers name.
    let mut rows: Vec<(String, Vec<MacroChildView>)> = Vec::new();
    for c in &children_knobs {
        match rows.iter_mut().find(|(g, _)| *g == c.group) {
            Some((_, r)) => r.push(c.clone()),
            None => rows.push((c.group.clone(), vec![c.clone()])),
        }
    }
    // The columns, by the knobs' ids (`delay-fb1`): a block without one
    // leaves its cell empty rather than shifting the row.
    let keys: [&str; 5] = if prefix == "reverb" {
        ["type", "time", "predelay", "character", "level"]
    } else {
        ["type", "time", "fb", "filter", "level"]
    };
    let cols = |row: &[MacroChildView]| -> Vec<Option<MacroChildView>> {
        keys.iter()
            .map(|key| {
                let stem = format!("{prefix}-{key}");
                row.iter()
                    .find(|c| c.id.strip_prefix(&stem).is_some_and(|n| n.chars().all(|ch| ch.is_ascii_digit())))
                    .cloned()
            })
            .collect()
    };
    let cell = |c: Option<MacroChildView>| {
        let prefix = prefix.clone();
        match c {
            Some(c) => {
                let mirror = linked_mirror(&c.id, &prefix, type_linked(), time_linked());
                rsx! { ChildCell { key: "{c.id}", child: c.clone(), dragging, width: 0, mirror } }
            }
            None => rsx! { div {} },
        }
    };
    let link = |on: bool| {
        format!(
            "display: flex; align-items: center; justify-content: center; padding: 2px 4px; border-radius: 4px; \
             cursor: pointer; background: {};",
            if on { "rgba(22,78,99,0.4)" } else { "transparent" },
        )
    };

    rsx! {
        div { style: "display: grid; grid-template-columns: repeat(5, 68px); column-gap: 4px; row-gap: 0;",
            // ── Header row ──
            for header in headers.iter() {
                div {
                    style: "text-align: center; font-size: 9px; font-weight: 600; color: #71717a; \
                            text-transform: uppercase; letter-spacing: 0.05em; padding: 4px 0;",
                    "{header}"
                }
            }
            for (ri, (_, row)) in rows.iter().enumerate() {
                if ri == 1 {
                    // ── Link buttons row ──
                    div { style: "display: flex; align-items: center; justify-content: center; padding: 2px 0;",
                        div {
                            title: "Link Type 1 and 2",
                            style: link(type_linked()),
                            onclick: move |_| type_linked.set(!type_linked()),
                            LinkGlyph { on: type_linked() }
                        }
                    }
                    div { style: "display: flex; align-items: center; justify-content: center; padding: 2px 0;",
                        div {
                            title: "Link Time 1 and 2",
                            style: link(time_linked()),
                            onclick: move |_| time_linked.set(!time_linked()),
                            LinkGlyph { on: time_linked() }
                        }
                    }
                    div {}
                    div {}
                    div {}
                }
                for c in cols(row) {
                    {cell(c)}
                }
            }
        }
    }
}

/// The legacy 🔗, drawn: Blitz has no colour-emoji font to draw it with.
#[component]
fn LinkGlyph(on: bool) -> Element {
    let stroke = if on { "#22d3ee" } else { "#52525b" };
    rsx! {
        svg {
            width: "12",
            height: "12",
            view_box: "0 0 24 24",
            style: "width: 12px; height: 12px;",
            path {
                d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71",
                fill: "none",
                stroke: "{stroke}",
                stroke_width: "2.2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
            }
            path {
                d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71",
                fill: "none",
                stroke: "{stroke}",
                stroke_width: "2.2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
            }
        }
    }
}

// ============================================================================
// GroupedDropdown — Pitch / Clarity: the knobs by the block they turn, a
// quiet header over each block's row, two blocks to a line
// ============================================================================

#[component]
fn GroupedDropdown(children_knobs: Vec<MacroChildView>, dragging: Signal<bool>) -> Element {
    let mut groups: Vec<(String, Vec<MacroChildView>)> = Vec::new();
    for c in &children_knobs {
        match groups.iter_mut().find(|(g, _)| *g == c.group) {
            Some((_, r)) => r.push(c.clone()),
            None => groups.push((c.group.clone(), vec![c.clone()])),
        }
    }
    // Two blocks to a column, filled down then across — DLY 1 over DLY 2,
    // VERB 1 over VERB 2 — keeps the panel short enough to hang under the
    // bar (four rows of Clarity would run off the bottom of the window) and
    // narrow enough to fit from the Delay knob to the end of the row.
    let rows = groups.len().min(2);
    rsx! {
        div {
            style: "display: grid; grid-template-rows: repeat({rows}, auto); grid-auto-flow: column; \
                    grid-auto-columns: max-content; column-gap: 16px; row-gap: 6px; align-items: start;",
            for (group, row) in groups.iter() {
                div { key: "{group}", style: "display: flex; flex-direction: column; gap: 0;",
                    div {
                        style: "font-size: 9px; font-weight: 600; color: #71717a; text-transform: uppercase; \
                                letter-spacing: 0.05em; white-space: nowrap; padding: 2px 2px 0;",
                        "{group}"
                    }
                    // Bottom-aligned, so a cell with an ON/OFF pad over it
                    // keeps its label and knob on the line of the others.
                    div { style: "display: flex; align-items: flex-end; gap: 4px;",
                        for c in row.iter() {
                            ChildCell { key: "{c.id}", child: c.clone(), dragging, width: 64 }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// MiniKnob — the legacy mini knob (36 px, arc + pointer), on the drag bus
// ============================================================================

/// The legacy `MiniKnob`: a 36 px arc knob — a grey track, the value arc
/// in the knob's colour (from the centre when bipolar), a dark cap and a
/// pointer. `spread` is Width's: at 0 a single mark at 12 o'clock, the arc
/// opening both ways from it as the value rises. Drags ride the rig root's
/// drag bus (Blitz has no pointer capture); wheel steps 1 %; a double-click
/// goes back to rest.
#[component]
fn MiniKnob(
    value: f32,
    color: String,
    #[props(default)] bipolar: bool,
    #[props(default)] spread: bool,
    /// Where a double-click puts it back: the patch as dialled.
    #[props(default = 0.5)]
    rest: f32,
    /// Set while a drag is live.
    dragging: Signal<bool>,
    on_change: Callback<f32>,
) -> Element {
    let bus = DragBus::try_use();
    let mut shield = use_signal(|| None::<(f64, f64)>);
    let display = value.clamp(0.0, 1.0);
    let v = f64::from(display);

    let size: f64 = 36.0;
    let center: f64 = size / 2.0;
    let radius: f64 = 14.0;
    let track_path = arc_path(center, center, radius, angle_for_value(0.0), angle_for_value(1.0));
    let value_path = if spread {
        if v > 0.001 {
            arc_path(center, center, radius, angle_for_value(0.5 - v / 2.0), angle_for_value(0.5 + v / 2.0))
        } else {
            String::new()
        }
    } else if bipolar {
        if v > 0.501 {
            arc_path(center, center, radius, angle_for_value(0.5), angle_for_value(v))
        } else if v < 0.499 {
            arc_path(center, center, radius, angle_for_value(v), angle_for_value(0.5))
        } else {
            String::new()
        }
    } else if v > 0.001 {
        arc_path(center, center, radius, angle_for_value(0.0), angle_for_value(v))
    } else {
        String::new()
    };
    // Centre tick at 12 o'clock (bipolar), or Width's mono mark.
    let (tick_x, tick_y) = arc_point(center, center, radius + 2.0, angle_for_value(0.5));
    let (tick_x2, tick_y2) = arc_point(center, center, radius - if spread { 5.0 } else { 1.0 }, angle_for_value(0.5));
    let (px, py) = arc_point(center, center, radius - 3.0, angle_for_value(v));

    let apply = move |n: f64| on_change.call(n.clamp(0.0, 1.0) as f32);

    rsx! {
        div {
            style: "width: 36px; height: 36px; position: relative; cursor: pointer; touch-action: none;",
            onpointerdown: move |e: PointerEvent| {
                let y0 = e.client_coordinates().y;
                let mut dragging = dragging;
                dragging.set(true);
                match bus {
                    Some(bus) => bus.begin(move |ev| match ev {
                        DragEvent::Move { y, .. } => apply(v + (y0 - y) / SENSITIVITY),
                        DragEvent::End => {
                            let mut dragging = dragging;
                            dragging.set(false);
                        }
                    }),
                    None => shield.set(Some((y0, v))),
                }
            },
            onwheel: move |e: WheelEvent| {
                let step = if e.modifiers().contains(Modifiers::SHIFT) { 0.002 } else { 0.01 };
                let up = e.delta().strip_units().y < 0.0;
                apply(if up { v + step } else { v - step });
            },
            ondoubleclick: move |_| apply(f64::from(rest)),
            svg {
                width: "36",
                height: "36",
                view_box: "0 0 {size} {size}",
                style: "width: 36px; height: 36px; position: absolute; pointer-events: none;",
                // Background track arc
                path { d: "{track_path}", fill: "none", stroke: "#374151", stroke_width: "3", stroke_linecap: "round" }
                // Value arc
                if !value_path.is_empty() {
                    path { d: "{value_path}", fill: "none", stroke: "{color}", stroke_width: "3", stroke_linecap: "round" }
                }
                if bipolar {
                    line {
                        x1: "{tick_x:.1}", y1: "{tick_y:.1}", x2: "{tick_x2:.1}", y2: "{tick_y2:.1}",
                        stroke: "#6B7280", stroke_width: "1.5", stroke_linecap: "round",
                    }
                }
                // Center circle
                circle { cx: "{center}", cy: "{center}", r: "{radius - 4.0}", fill: "#1F2937" }
                if spread {
                    // Width reads from the top: its mark stays at 12.
                    line {
                        x1: "{tick_x:.1}", y1: "{tick_y:.1}", x2: "{tick_x2:.1}", y2: "{tick_y2:.1}",
                        stroke: "{color}", stroke_width: "2", stroke_linecap: "round",
                    }
                } else {
                    // Pointer
                    line {
                        x1: "{center}", y1: "{center}", x2: "{px:.1}", y2: "{py:.1}",
                        stroke: "{color}", stroke_width: "2", stroke_linecap: "round",
                    }
                }
            }
            // No drag bus above (a test mounting the bar alone): a local
            // shield owns the pointer while the drag is live.
            if shield().is_some() {
                div {
                    style: "position: absolute; inset: -2000px; z-index: 1000; cursor: ns-resize;",
                    onpointermove: move |e: PointerEvent| {
                        if let Some((y0, v0)) = shield() {
                            apply(v0 + (y0 - e.client_coordinates().y) / SENSITIVITY);
                        }
                    },
                    onpointerup: move |_| {
                        shield.set(None);
                        let mut dragging = dragging;
                        dragging.set(false);
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    /// The Type and Time links pair the two rows' knobs — and only those.
    #[test]
    fn links_mirror_type_and_time_only() {
        assert_eq!(linked_mirror("delay-type1", "delay", true, false).as_deref(), Some("delay-type2"));
        assert_eq!(linked_mirror("delay-type2", "delay", true, false).as_deref(), Some("delay-type1"));
        assert_eq!(linked_mirror("reverb-time1", "reverb", false, true).as_deref(), Some("reverb-time2"));
        assert_eq!(linked_mirror("reverb-time2", "reverb", false, true).as_deref(), Some("reverb-time1"));
        assert_eq!(linked_mirror("delay-time1", "delay", true, false), None, "time not linked");
        assert_eq!(linked_mirror("delay-fb1", "delay", true, true), None, "only type and time link");
        assert_eq!(linked_mirror("delay-type1", "reverb", true, true), None, "its own panel only");
    }

    fn child(fmt: &str, param: f32) -> MacroChildView {
        MacroChildView {
            id: "x".into(),
            label: "X".into(),
            color: String::new(),
            value: 0.5,
            rest: 0.5,
            group: String::new(),
            has_pad: false,
            bypassed: false,
            fmt: fmt.into(),
            param,
            aux: 0.0,
            steps: 0,
            tune: None,
        }
    }

    /// Readouts in the rig's units: intervals signed with a real minus,
    /// the Ice menu's cents and Free, divisions, dB, Hz.
    #[test]
    fn readouts_speak_the_rigs_units() {
        assert_eq!(child_readout(&child("semitones", 12.0)), "+12");
        assert_eq!(child_readout(&child("semitones", -12.0)), "\u{2212}12");
        assert_eq!(child_readout(&child("semitones", 7.0)), "+7");
        assert_eq!(child_readout(&child("interval", 27.0)), "+12");
        assert_eq!(child_readout(&child("interval", 0.0)), "\u{2212}12");
        assert_eq!(child_readout(&child("interval", 14.0)), "+25c");
        assert_eq!(child_readout(&child("interval", 30.0)), "Free");
        assert_eq!(child_readout(&child("div", 1.0)), "1/8.");
        assert_eq!(child_readout(&child("db", -60.0)), "Off");
        assert_eq!(child_readout(&child("hz", 250.0)), "250 Hz");
        assert_eq!(child_readout(&child("ms", 1500.0)), "1.50 s");
        assert_eq!(child_readout(&child("pct", 0.42)), "42%");
    }
}
