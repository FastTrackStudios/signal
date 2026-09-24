//! **What a preset sounds like, drawn** — so a list of 39 delays reads as
//! slaps, rhythms, leads and washes before a single name is read.
//!
//! A block preset carries its saved params on the wire; this reads them into
//! a [`Look`]: the group it is hunted by (Slap / Rhythmic / … for a delay,
//! the algorithm family for a reverb, the engine or style for anything
//! else), its engine as a short word, the one value a player reads (a
//! division or a time, a decay in seconds), and a tiny picture — echo ticks
//! spaced by the time and fading by the feedback, or a decay wedge as long
//! as the tail. The same pieces draw the sidebar rows, the current pick's
//! header at full width, and the chips on a Time snapshot.

use dioxus::prelude::*;
use signal_guitar_proto::{BlockPresetEntry, PresetParam};

use crate::control::{DELAY_ALGOS, DIV_LABELS, MOD_ENGINES, VERB_ALGOS, div_factor, verb_seconds};
use crate::theme::{DIM, FAINT, LIVE, MUTED, TEXT};

/// A saved param, or `default`.
fn param(params: &[PresetParam], name: &str, default: f32) -> f32 {
    params
        .iter()
        .find(|p| p.name == name)
        .map_or(default, |p| p.value)
}

fn has(params: &[PresetParam], name: &str) -> bool {
    params.iter().any(|p| p.name == name)
}

/// The groups a delay list is hunted by, in list order.
pub const DELAY_GROUPS: [&str; 6] = ["Slap", "Rhythmic", "Lead", "Ambient", "Modulated", "Special"];
/// A reverb's algorithm families, in list order.
pub const VERB_GROUPS: [&str; 5] = ["Rooms", "Halls", "Plates", "Springs", "Ambient"];
/// The group of an "off" preset — pinned at the top, quiet.
pub const OFF: &str = "Off";

/// The division index that runs free on the time knob (`Free`).
const FREE_DIV: usize = 7;
/// The quarter note the tick pictures assume, ms (120 bpm): a picture of a
/// division has to be drawn at some tempo, and the list compares shapes.
const QUARTER_MS: f32 = 500.0;

/// How a preset reads in a list.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Look {
    /// The group it is listed under.
    pub group: String,
    /// Its engine or style, short ("Tape", "Hall"); empty when it has none.
    pub engine: String,
    /// The engine is the block's default (drawn grey).
    pub engine_default: bool,
    /// The one value a player reads ("1/8.", "120 ms", "2.4 s").
    pub value: String,
    /// The picture.
    pub shape: Shape,
}

/// The tiny picture of a preset.
#[derive(Clone, PartialEq, Debug, Default)]
pub enum Shape {
    /// Echoes: their delay (ms) and feedback.
    Echo { time_ms: f32, feedback: f32 },
    /// A reverb tail: its length (s) and pre-delay (ms).
    Tail { seconds: f32, predelay_ms: f32 },
    #[default]
    None,
}

/// A delay's time, ms: its division at [`QUARTER_MS`], else its own time.
fn delay_ms(params: &[PresetParam]) -> f32 {
    let div = param(params, "tap_div_l", FREE_DIV as f32).round().max(0.0) as usize;
    let f = div_factor(div as f32);
    if div != FREE_DIV && f > 0.0 {
        f * QUARTER_MS
    } else {
        param(params, "time", 350.0)
    }
}

/// The group a delay is hunted by, from what it does: a name that says
/// lead, a long or swelling tail, an unusual machine, a short single
/// repeat, heavy modulation — else a rhythm.
fn delay_group(name: &str, params: &[PresetParam]) -> &'static str {
    let lower = name.to_lowercase();
    let fb = param(params, "feedback", 0.3);
    let style = param(params, "style", 0.0).round() as i32;
    let div = param(params, "tap_div_l", FREE_DIV as f32).round() as usize;
    let time = param(params, "time", 350.0);
    if lower.contains("lead") {
        "Lead"
    } else if ["ambient", "swell", "wash", "pad"].iter().any(|w| lower.contains(w))
        || fb >= 0.55
        || param(params, "swell", 0.0) >= 0.5
    {
        "Ambient"
    } else if !(0..=2).contains(&style) {
        "Special"
    } else if div == FREE_DIV && time <= 160.0 {
        "Slap"
    } else if param(params, "mod_depth", 0.0) >= 0.3 {
        "Modulated"
    } else {
        "Rhythmic"
    }
}

/// A reverb algorithm's family.
fn verb_group(algorithm: usize) -> &'static str {
    match algorithm {
        0 => "Rooms",
        1 => "Halls",
        2 => "Plates",
        3 => "Springs",
        _ => "Ambient",
    }
}

/// "2.4 s", "650 ms".
fn seconds(s: f64) -> String {
    if s < 1.0 {
        format!("{:.0} ms", s * 1000.0)
    } else if s < 10.0 {
        format!("{s:.1} s")
    } else {
        format!("{s:.0} s")
    }
}

/// How `p` reads.
#[must_use]
pub fn look(p: &BlockPresetEntry) -> Look {
    let params = &p.params;
    if p.bypass {
        return Look {
            group: OFF.to_string(),
            ..Look::default()
        };
    }
    match p.block_type.as_str() {
        "delay" => {
            let style = param(params, "style", 0.0).round().max(0.0) as usize;
            let div = param(params, "tap_div_l", FREE_DIV as f32).round().max(0.0) as usize;
            let value = if div != FREE_DIV && div_factor(div as f32) > 0.0 {
                DIV_LABELS.get(div).copied().unwrap_or("").to_string()
            } else {
                format!("{:.0} ms", param(params, "time", 350.0))
            };
            Look {
                group: delay_group(&p.name, params).to_string(),
                engine: DELAY_ALGOS.get(style).copied().unwrap_or("").to_string(),
                engine_default: style == 0,
                value,
                shape: Shape::Echo {
                    time_ms: delay_ms(params),
                    feedback: param(params, "feedback", 0.3),
                },
            }
        }
        "reverb" => {
            let alg = param(params, "algorithm", 0.0).round().max(0.0) as usize;
            let (secs, exact) = verb_seconds(
                param(params, "algorithm", 0.0),
                param(params, "variant", 0.0),
                param(params, "decay", 0.5),
            );
            Look {
                group: verb_group(alg).to_string(),
                engine: VERB_ALGOS.get(alg).copied().unwrap_or("").to_string(),
                engine_default: false,
                value: if exact { seconds(secs) } else { format!("≈{}", seconds(secs)) },
                shape: Shape::Tail {
                    seconds: secs as f32,
                    predelay_ms: param(params, "predelay", 0.0),
                },
            }
        }
        other => {
            // Anything else: by its engine or style, else one A–Z list.
            let (engine, default) = if other == "compressor" && has(params, "style") {
                let i = param(params, "style", 0.0).round().max(0.0) as usize;
                (crate::comp_surface::COMP_STYLES.get(i).copied().unwrap_or(""), i == 0)
            } else if has(params, "engine") {
                let i = param(params, "engine", 0.0).round().max(0.0) as usize;
                (MOD_ENGINES.get(i).copied().unwrap_or(""), i == 0)
            } else {
                ("", false)
            };
            Look {
                group: engine.to_string(),
                engine: engine.to_string(),
                engine_default: default,
                ..Look::default()
            }
        }
    }
}

/// The order `block_type`'s groups are listed in; others follow A–Z.
#[must_use]
pub fn group_rank(block_type: &str, group: &str) -> usize {
    if group == OFF {
        return 0;
    }
    let order: &[&str] = match block_type {
        "delay" => &DELAY_GROUPS,
        "reverb" => &VERB_GROUPS,
        _ => &[],
    };
    order.iter().position(|g| *g == group).map_or(100, |i| i + 1)
}

/// A block preset's list: its presets with their looks, grouped in list
/// order (Off first), library order within a group.
#[must_use]
pub fn grouped(presets: &[BlockPresetEntry]) -> Vec<(String, Vec<(BlockPresetEntry, Look)>)> {
    let mut rows: Vec<(BlockPresetEntry, Look)> = presets.iter().map(|p| (p.clone(), look(p))).collect();
    let ty = presets.first().map(|p| p.block_type.clone()).unwrap_or_default();
    // Stable: library order is kept inside a group.
    rows.sort_by(|a, b| {
        group_rank(&ty, &a.1.group)
            .cmp(&group_rank(&ty, &b.1.group))
            .then_with(|| a.1.group.cmp(&b.1.group))
    });
    let mut out: Vec<(String, Vec<(BlockPresetEntry, Look)>)> = Vec::new();
    for r in rows {
        match out.last_mut() {
            Some((g, v)) if *g == r.1.group => v.push(r),
            _ => out.push((r.1.group.clone(), vec![r])),
        }
    }
    // Nothing but an Off and one unnamed group: an A–Z list.
    for (g, v) in &mut out {
        if g.is_empty() {
            v.sort_by_key(|(p, _)| p.name.to_lowercase());
        }
    }
    out
}

/// The picture, `w` × `h` px. `lit` draws it in the accent (the preset that
/// plays); otherwise grey — a picture is a shape to compare, not a signal.
#[component]
pub fn ShapeView(shape: Shape, #[props(default = 44)] w: u32, #[props(default = 12)] h: u32, #[props(default)] lit: bool) -> Element {
    let ink = if lit { LIVE } else { MUTED };
    let (wf, hf) = (f64::from(w), f64::from(h));
    match shape {
        Shape::Echo { time_ms, feedback } => {
            // One tick per audible repeat, spaced by the time (a 1 s gap
            // spans the width), each shorter and fainter by the feedback.
            let gap = (f64::from(time_ms) / 1000.0 * wf).clamp(2.5, wf / 2.0);
            let fb = f64::from(feedback.clamp(0.0, 0.95));
            let mut ticks = Vec::new();
            let (mut amp, mut x) = (1.0f64, 1.0f64);
            while x <= wf - 1.0 && amp > 0.08 && ticks.len() < 24 {
                ticks.push((x, amp));
                amp *= fb.max(0.0);
                x += gap;
                if fb <= 0.0 {
                    break;
                }
            }
            rsx! {
                svg {
                    width: "{w}", height: "{h}", view_box: "0 0 {w} {h}",
                    style: "display: block; flex-shrink: 0; width: {w}px; height: {h}px;",
                    line { x1: "0", y1: "{hf - 0.5}", x2: "{wf}", y2: "{hf - 0.5}", stroke: DIM, stroke_width: "1" }
                    for (i, (x, a)) in ticks.into_iter().enumerate() {
                        line {
                            key: "{i}",
                            // √ so a quiet repeat still reads as a tick.
                            x1: "{x:.1}", y1: "{hf - 1.0}", x2: "{x:.1}", y2: "{(hf - 1.0) * (1.0 - a.sqrt()):.1}",
                            stroke: ink, stroke_width: "1.6", stroke_linecap: "round",
                            opacity: "{0.4 + 0.6 * a:.2}",
                        }
                    }
                }
            }
        }
        Shape::Tail { seconds, predelay_ms } => {
            // A wedge: the gap is the pre-delay, the length the tail on a
            // log scale (0.3 s … 20 s across the width).
            let pre = (f64::from(predelay_ms) / 200.0 * wf * 0.25).clamp(0.0, wf * 0.25);
            let t = f64::from(seconds.max(0.1));
            let span = ((t / 0.3).ln() / (20.0f64 / 0.3).ln()).clamp(0.08, 1.0) * (wf - pre);
            let end = pre + span;
            rsx! {
                svg {
                    width: "{w}", height: "{h}", view_box: "0 0 {w} {h}",
                    style: "display: block; flex-shrink: 0; width: {w}px; height: {h}px;",
                    line { x1: "0", y1: "{hf - 0.5}", x2: "{wf}", y2: "{hf - 0.5}", stroke: DIM, stroke_width: "1" }
                    path {
                        d: "M{pre:.1} {hf - 1.0} L{pre:.1} 1 L{end:.1} {hf - 1.0} Z",
                        fill: ink, opacity: "0.55",
                    }
                }
            }
        }
        Shape::None => rsx! {
            span { style: "display: block; flex-shrink: 0; width: {w}px; height: {h}px;" }
        },
    }
}

/// The value, as a mono chip: bright when the preset plays, else muted.
#[component]
pub fn ValueChip(value: String, #[props(default)] lit: bool) -> Element {
    if value.is_empty() {
        return rsx! {};
    }
    let ink = if lit { TEXT } else { MUTED };
    rsx! {
        span {
            style: "flex-shrink: 0; min-width: 40px; text-align: right; font-size: 10px; \
                    font-family: monospace; color: {ink}; white-space: nowrap;",
            "{value}"
        }
    }
}

/// The engine, as a quiet word chip — grey at the block's default engine.
#[component]
pub fn EngineChip(engine: String, #[props(default)] default: bool) -> Element {
    if engine.is_empty() {
        return rsx! {};
    }
    let ink = if default { FAINT } else { MUTED };
    rsx! {
        span {
            style: "flex-shrink: 0; padding: 0 5px; border-radius: 4px; border: 1px solid #2b2b31; \
                    font-size: 9px; line-height: 14px; color: {ink}; white-space: nowrap;",
            "{engine}"
        }
    }
}

/// An engine's swatch colour — muted, and the same for one engine wherever
/// it appears, so Tape reads as Tape down a list before its chip is read.
#[must_use]
pub fn engine_swatch(engine: &str) -> &'static str {
    const SWATCHES: [&str; 8] = [
        "#a16207", "#0e7490", "#7c3aed", "#be185d", "#15803d", "#1d4ed8", "#b45309", "#4d7c0f",
    ];
    if engine.is_empty() {
        return DIM;
    }
    let h = engine.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    SWATCHES[(h as usize) % SWATCHES.len()]
}

/// "×3" when a preset is shared, with who shares it as the tooltip; nothing
/// for one user or none.
fn usage(used_by: &[String]) -> (String, String) {
    if used_by.len() > 1 {
        (format!("×{}", used_by.len()), used_by.join("\n"))
    } else {
        (String::new(), used_by.join("\n"))
    }
}

/// One preset in a list: swatch and name hard left, the engine and usage on
/// the subline beneath; value and picture hard right; ⋯ faint until the row
/// is hovered. The one that plays has the green edge; edited, the amber dot.
#[component]
pub fn PresetRow(
    name: String,
    look: Look,
    #[props(default)] used_by: Vec<String>,
    #[props(default)] live: bool,
    #[props(default)] modified: bool,
    /// Indent, px — a snapshot under its preset.
    #[props(default)]
    indent: u32,
    /// A line of its own under the name, instead of the engine (an amp's
    /// captures).
    #[props(default)]
    subline: String,
    /// Extra pieces at the right, before the value (a Time snapshot's picks).
    #[props(default)]
    children: Element,
    onclick: EventHandler<()>,
    #[props(default)] menu: Vec<crate::kit::MenuItem>,
    #[props(default)] on_menu: Option<EventHandler<crate::kit::Picked>>,
) -> Element {
    let host = signal_widgets::PopupHost::try_use();
    let off = look.group == OFF;
    let (count, who) = usage(&used_by);
    let edge = if live { LIVE } else { "transparent" };
    let bg = if live { crate::theme::LIVE_BG } else { "transparent" };
    let ink = if off && !live { MUTED } else { TEXT };
    let pad = if off { "4px" } else { "6px" };
    // The engine chip, unless the group already says it (a Room among Rooms).
    let engine = if look.group.starts_with(look.engine.as_str()) { String::new() } else { look.engine.clone() };
    rsx! {
        div {
            class: if live { "group" } else { "group hover:bg-accent/30" },
            style: "display: flex; align-items: center; gap: 8px; min-width: 0; margin-left: {indent}px; \
                    padding: {pad} 6px {pad} 8px; border-left: 2px solid {edge}; border-radius: 0 6px 6px 0; \
                    background: {bg}; cursor: pointer;",
            title: "{who}",
            onclick: move |_| onclick.call(()),
            oncontextmenu: {
                let menu = menu.clone();
                move |e: MouseEvent| {
                    if let Some(h) = on_menu {
                        e.prevent_default();
                        e.stop_propagation();
                        crate::kit::context_menu(host, &e, menu.clone(), h);
                    }
                }
            },
            if !off && !look.engine.is_empty() {
                span { style: "width: 3px; height: 22px; border-radius: 2px; flex-shrink: 0; background: {engine_swatch(&look.engine)};" }
            }
            div { style: "flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                div { style: "display: flex; align-items: center; gap: 5px; min-width: 0;",
                    if modified {
                        crate::kit::Dot { modified: true, size: 6 }
                    }
                    span { style: "font-size: 12px; font-weight: 600; color: {ink}; white-space: nowrap; overflow: hidden; min-width: 0;",
                        "{name}"
                    }
                }
                if !off && (!subline.is_empty() || !engine.is_empty() || !count.is_empty()) {
                    div { style: "display: flex; align-items: center; gap: 5px; min-width: 0; overflow: hidden;",
                        if subline.is_empty() {
                            EngineChip { engine: engine.clone(), default: look.engine_default }
                        } else {
                            span { style: "font-size: 10px; color: {FAINT}; white-space: nowrap; overflow: hidden;", "{subline}" }
                        }
                        if !count.is_empty() {
                            span { style: "font-size: 10px; font-family: monospace; color: {FAINT};", "{count}" }
                        }
                    }
                }
            }
            {children}
            if !off {
                ValueChip { value: look.value.clone(), lit: live }
                ShapeView { shape: look.shape.clone(), w: 52, h: 14, lit: live }
            }
            if let Some(h) = on_menu.filter(|_| !menu.is_empty()) {
                div { class: "opacity-25 group-hover:opacity-100", style: "display: flex; flex-shrink: 0;",
                    crate::kit::ActionMenu { items: menu.clone(), on_pick: h, size: 20, bare: true, title: "Actions" }
                }
            }
        }
    }
}

/// A quiet group header: small caps and a count, hard left.
#[component]
pub fn GroupHeader(label: String, count: usize) -> Element {
    rsx! {
        div { style: "display: flex; align-items: baseline; gap: 6px; padding: 12px 8px 4px 10px;",
            span { style: "{crate::theme::EYEBROW}", "{label}" }
            span { style: "font-size: 10px; font-family: monospace; color: {DIM};", "{count}" }
        }
    }
}

/// The filter chips under a search: All, then each group with its count. A
/// chip is where the filter changes.
#[component]
pub fn GroupChips(groups: Vec<(String, usize)>, selected: String, on_pick: EventHandler<String>) -> Element {
    let total: usize = groups.iter().map(|(_, n)| n).sum();
    let all = std::iter::once(("All".to_string(), total)).chain(groups);
    rsx! {
        div { style: "display: flex; flex-wrap: wrap; gap: 4px;",
            for (g, n) in all {
                {
                    let on = (g == "All" && selected.is_empty()) || g == selected;
                    let pick = if g == "All" { String::new() } else { g.clone() };
                    rsx! {
                        button {
                            key: "{g}",
                            style: format!(
                                "display: flex; align-items: baseline; gap: 4px; padding: 2px 8px; border-radius: 999px; \
                                 font-size: 11px; font-weight: 600; cursor: pointer; border: 1px solid {}; background: {}; color: {};",
                                if on { crate::theme::FOCUS_FG } else { crate::theme::LINE },
                                if on { crate::theme::FOCUS_BG } else { "transparent" },
                                if on { crate::theme::FOCUS_FG } else { MUTED },
                            ),
                            onclick: move |_| on_pick.call(pick.clone()),
                            "{g}"
                            span { style: "font-size: 9px; font-family: monospace; opacity: 0.6;", "{n}" }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(ty: &str, name: &str, params: &[(&str, f32)]) -> BlockPresetEntry {
        BlockPresetEntry {
            block_type: ty.into(),
            name: name.into(),
            bypass: false,
            used_by: Vec::new(),
            params: params
                .iter()
                .map(|(n, v)| PresetParam { name: (*n).into(), value: *v })
                .collect(),
        }
    }

    #[test]
    fn delays_group_by_what_they_do() {
        let slap = preset("delay", "Sun Slap", &[("tap_div_l", 7.0), ("time", 120.0), ("feedback", 0.05)]);
        let dotted = preset("delay", "Dotted Eighth", &[("tap_div_l", 1.0), ("feedback", 0.35)]);
        let lead = preset("delay", "Lead Quarter", &[("tap_div_l", 0.0), ("feedback", 0.45)]);
        let wash = preset("delay", "Flute Wash", &[("tap_div_l", 8.0), ("feedback", 0.7)]);
        let warm = preset("delay", "Warm Analog", &[("style", 2.0), ("tap_div_l", 0.0), ("feedback", 0.45), ("mod_depth", 0.3)]);
        let rev = preset("delay", "Reverse", &[("style", 5.0), ("tap_div_l", 9.0), ("feedback", 0.15)]);
        let groups: Vec<String> = [&slap, &dotted, &lead, &wash, &warm, &rev].iter().map(|p| look(p).group).collect();
        assert_eq!(groups, ["Slap", "Rhythmic", "Lead", "Ambient", "Modulated", "Special"]);
        assert_eq!(look(&slap).value, "120 ms");
        assert_eq!(look(&dotted).value, "1/8.");
    }

    #[test]
    fn off_is_pinned_first_and_reverbs_group_by_family() {
        let mut off = preset("reverb", "Reverb Off", &[]);
        off.bypass = true;
        let hall = preset("reverb", "Worship Hall", &[("algorithm", 1.0), ("decay", 0.42)]);
        let room = preset("reverb", "Glue Room", &[("algorithm", 0.0), ("decay", 0.41)]);
        let bloom = preset("reverb", "Bloom Pad", &[("algorithm", 5.0), ("decay", 0.6)]);
        let g = grouped(&[hall, bloom, off, room]);
        let names: Vec<&str> = g.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Off", "Rooms", "Halls", "Ambient"]);
    }
}
