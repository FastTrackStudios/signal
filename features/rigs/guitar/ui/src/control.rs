//! The Control view — the guitar instrument panel.
//!
//! Layout: input meter on the far left, output + headphone metering on the
//! far right (with the main-output mute between them), and the surface in
//! the middle — song strip, the full six-band EQ with the live input
//! spectrum behind it, the compressor with gain-reduction metering, the
//! gate, then the bottom rail: always-on mini tuner, volume + expression
//! pedals, reserved module chips (Env Filter / Wah / Pitch / Doubler /
//! Drive), mini MIDI monitor, and the headphone-cue module. Every panel is
//! a [`ZoomPanel`]: the card *is* the editor; zooming just gives it the
//! whole screen.

use std::time::Duration;

use dioxus::prelude::*;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{BlockParam, LiveBlock, PerformanceModel};
use signal_proto::block::BlockType;

use crate::state::RigViewState;

/// Find a chain block by (type, name).
fn find_block(blocks: &[LiveBlock], bt: BlockType, name: &str) -> Option<LiveBlock> {
    blocks
        .iter()
        .find(|b| b.block_type == bt && b.name.eq_ignore_ascii_case(name))
        .cloned()
}

fn param(block: &LiveBlock, name: &str) -> Option<BlockParam> {
    block.params.iter().find(|p| p.name == name).cloned()
}

fn param_v(block: &LiveBlock, name: &str, dflt: f32) -> f32 {
    param(block, name).map(|p| p.value).unwrap_or(dflt)
}

/// Fire a param write without blocking the UI.
fn send_param(rig: &Option<RigClient>, id: &str, name: &str, value: f32) {
    if let Some(r) = rig.clone() {
        let (id, name) = (id.to_string(), name.to_string());
        spawn(async move {
            let _ = r.set_block_param(id, name, value).await;
        });
    }
}

/// A quick-access module card that zooms to a full-screen editor. The same
/// children render in both sizes — panels are written to scale.
#[component]
pub fn ZoomPanel(title: String, children: Element) -> Element {
    let mut zoomed = use_signal(|| false);
    rsx! {
        div { class: "relative flex flex-col flex-1 border border-border bg-card min-h-0 overflow-hidden",
            div { class: "flex-1 min-h-0", {children.clone()} }
            // Floating zoom control — no chrome, just the corner icon.
            button {
                class: "absolute top-1 right-1.5 z-20 text-muted-foreground/60 hover:text-foreground text-sm leading-none",
                title: "{title}",
                onclick: move |_| zoomed.set(true),
                "⤢"
            }
        }
        if zoomed() {
            div { class: "fixed inset-0 z-50 flex flex-col bg-black/95 p-6",
                button {
                    class: "absolute top-3 right-4 z-10 text-muted-foreground hover:text-foreground text-xl",
                    onclick: move |_| zoomed.set(false),
                    "✕"
                }
                div { class: "flex-1 min-h-0", {children} }
            }
        }
    }
}

/// One labelled slider bound to a block param over the rig service.
#[component]
fn ParamSlider(block_id: String, p: BlockParam, #[props(default)] fmt_hz: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let label = if fmt_hz {
        if p.value >= 1000.0 {
            format!("{:.1}k", p.value / 1000.0)
        } else {
            format!("{:.0}", p.value)
        }
    } else {
        format!("{:.1}", p.value)
    };
    let name = p.name.clone();
    rsx! {
        div { class: "flex flex-col gap-0.5 min-w-0",
            div { class: "flex items-center justify-between gap-1",
                span { class: "text-[9px] font-mono text-muted-foreground truncate", "{p.name}" }
                span { class: "text-[9px] font-mono", "{label}" }
            }
            input {
                r#type: "range",
                class: "w-full h-1 accent-primary",
                min: "{p.min}",
                max: "{p.max}",
                step: "any",
                value: "{p.value}",
                oninput: move |e| {
                    if let Ok(v) = e.value().parse::<f32>() {
                        send_param(&rig, &block_id, &name, v);
                    }
                },
            }
        }
    }
}

/// A vertical level meter with a dB readout.
#[component]
fn VMeter(label: &'static str, level_db: f32, #[props(default)] muted: bool) -> Element {
    let pct = ((level_db + 60.0) / 60.0 * 100.0).clamp(0.0, 100.0);
    let color = if muted {
        "#3f3f46"
    } else if level_db > -6.0 {
        "#ef4444"
    } else if level_db > -18.0 {
        "#eab308"
    } else {
        "#22c55e"
    };
    rsx! {
        div { class: "flex flex-col items-center gap-1 h-full min-h-0",
            span { class: "text-[9px] font-semibold uppercase tracking-wider text-muted-foreground", "{label}" }
            div { class: "relative flex-1 w-3 rounded bg-black/60 border border-border overflow-hidden min-h-0",
                div {
                    class: "absolute inset-x-0 bottom-0 transition-[height] duration-75",
                    style: "height: {pct}%; background-color: {color};",
                }
            }
            span { class: "text-[8px] font-mono text-muted-foreground",
                if level_db <= -89.0 { "−∞" } else { {format!("{level_db:.0}")} }
            }
        }
    }
}

// ── Gate ────────────────────────────────────────────────────────────────────

/// The gate, tall and slim: a vertical level bar with the threshold line —
/// drag anywhere on the bar to set the threshold. Attack/release knobs
/// beneath.
#[component]
fn GatePanel(block: LiveBlock, in_db: f32) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let mut tracking = use_signal(|| false);

    let thr = param_v(&block, "threshold", -50.0);
    let open = in_db >= thr && !block.bypassed;
    let level_pct = ((in_db + 90.0) / 90.0 * 100.0).clamp(0.0, 100.0);
    let thr_pct = ((thr + 90.0) / 90.0 * 100.0).clamp(0.0, 100.0);

    let set_thr = {
        let rig = rig.clone();
        let id = block.id.clone();
        move |coords: dioxus::html::geometry::ElementPoint| {
            let el = el();
            let (rig, id) = (rig.clone(), id.clone());
            spawn(async move {
                let Some(el) = el else { return };
                let Ok(rect) = el.get_client_rect().await else { return };
                let frac = (1.0 - coords.y / rect.height()).clamp(0.0, 1.0) as f32;
                if let Some(r) = rig {
                    let _ = r.set_block_param(id, "threshold".into(), frac * 90.0 - 90.0).await;
                }
            });
        }
    };

    rsx! {
        div { class: "flex flex-col items-center gap-1 h-full min-h-0 pt-4",
            span {
                class: "text-[9px] font-bold uppercase tracking-wider rounded px-1 py-0.5 flex-shrink-0",
                style: if block.bypassed {
                    "background-color: #3f3f46; color: #a1a1aa;"
                } else if open {
                    "background-color: #22c55e; color: #052e16;"
                } else {
                    "background-color: #27272a; color: #71717a;"
                },
                if block.bypassed { "Byp" } else if open { "Open" } else { "Closed" }
            }
            // The tall bar: level fill from the bottom, threshold marker on
            // top — drag to move the threshold.
            div {
                class: "relative flex-1 w-6 rounded bg-black/60 border border-border overflow-hidden min-h-0 cursor-ns-resize touch-none",
                onmounted: move |e| el.set(Some(e.data())),
                onpointerdown: {
                    let set_thr = set_thr.clone();
                    move |e: PointerEvent| {
                        tracking.set(true);
                        set_thr(e.element_coordinates());
                    }
                },
                onpointermove: move |e: PointerEvent| {
                    if tracking() {
                        set_thr(e.element_coordinates());
                    }
                },
                onpointerup: move |_| tracking.set(false),
                onpointerleave: move |_| tracking.set(false),
                div {
                    class: "absolute inset-x-0 bottom-0 transition-[height] duration-75",
                    style: if open { "height: {level_pct}%; background-color: #22c55e;" }
                           else { "height: {level_pct}%; background-color: #52525b;" },
                }
                div {
                    class: "absolute inset-x-0 h-0.5",
                    style: "bottom: {thr_pct}%; background-color: #eab308;",
                }
            }
            span { class: "text-[8px] font-mono text-muted-foreground flex-shrink-0", "{thr:.0} dB" }
            div { class: "flex flex-col gap-0.5 flex-shrink-0",
                for name in ["attack", "release"] {
                    if let Some(p) = param(&block, name) {
                        {
                            let rig = rig.clone();
                            let id = block.id.clone();
                            let pname = name.to_string();
                            rsx! {
                                crate::knob::Knob {
                                    label: if name == "attack" { "Atk".to_string() } else { "Rel".to_string() },
                                    value: p.value,
                                    min: p.min,
                                    max: p.max,
                                    size: crate::knob::KnobSize::Small,
                                    on_change: Callback::new(move |v: f32| {
                                        if let Some(r) = rig.clone() {
                                            let (id, pname) = (id.clone(), pname.clone());
                                            spawn(async move { let _ = r.set_block_param(id, pname, v).await; });
                                        }
                                    }),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Time & modulation visualizers ──────────────────────────────────────────

const DELAY_COLORS: [&str; 2] = ["#38bdf8", "#818cf8"];
const VERB_COLORS: [&str; 2] = ["#2dd4bf", "#a78bfa"];

/// Note-division labels — order matches the backend's `div_factor`.
const DIV_LABELS: [&str; 8] = ["1/1", "1/2.", "1/2", "1/4.", "1/4", "1/8.", "1/8", "1/16"];

fn div_factor(idx: f32) -> f32 {
    [4.0, 3.0, 2.0, 1.5, 1.0, 0.75, 0.5, 0.25][(idx as usize).min(7)]
}

const DELAY_ALGOS: [&str; 4] = ["Digital", "Tape", "Analog", "Diffuse"];
const VERB_ALGOS: [&str; 4] = ["Hall", "Plate", "Room", "Shimmer"];

/// A small labelled dropdown bound to an enum-style block param.
#[component]
fn ParamSelect(
    block_id: String,
    name: &'static str,
    label: &'static str,
    value: f32,
    options: Vec<&'static str>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    rsx! {
        div { class: "flex flex-col gap-0.5 min-w-0",
            span { style: "font-size:8px; font-weight:600; text-transform:uppercase; color:#8a8a92;", "{label}" }
            select {
                class: "bg-transparent border border-border rounded-sm text-[10px] px-0.5 py-0",
                value: "{value as usize}",
                onchange: move |e: FormEvent| {
                    if let Ok(v) = e.value().parse::<usize>() {
                        send_param(&rig, &block_id, name, v as f32);
                    }
                },
                for (i, o) in options.iter().enumerate() {
                    option { key: "{i}", value: "{i}", selected: i == value as usize, "{o}" }
                }
            }
        }
    }
}

/// A small param knob shorthand.
#[component]
fn PKnob(block_id: String, name: &'static str, label: &'static str, p: BlockParam) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    rsx! {
        crate::knob::Knob {
            label: label.to_string(),
            value: p.value,
            min: p.min,
            max: p.max,
            size: crate::knob::KnobSize::Small,
            on_change: Callback::new(move |v: f32| {
                if let Some(r) = rig.clone() {
                    let (id, name) = (block_id.clone(), name.to_string());
                    spawn(async move { let _ = r.set_block_param(id, name, v).await; });
                }
            }),
        }
    }
}

/// The stereo delay module: multitap visualization (left taps up, right taps
/// down) driven by the live blocks' note divisions + tempo, with the full
/// parameter surface — timing per side, HP/LP, ducking, mix, algorithm —
/// per delay (1/2 selector).
#[component]
fn DelayPanel(blocks: Vec<LiveBlock>, tempo_bpm: u32) -> Element {
    let mut sel = use_signal(|| 0usize);
    const W: f32 = 220.0;
    let delays: Vec<LiveBlock> = blocks
        .iter()
        .filter(|b| b.block_type == BlockType::Delay)
        .cloned()
        .collect();
    if delays.is_empty() {
        return rsx! { span { class: "text-xs text-muted-foreground italic p-2", "No delays in the chain." } };
    }
    let quarter = 60_000.0 / tempo_bpm.max(1) as f32;
    let win_ms = quarter * 8.0; // two bars of quarters

    let cur = delays[sel().min(delays.len() - 1)].clone();
    let cur_id = cur.id.clone();

    rsx! {
        div { class: "flex h-full min-h-0", style: "background: #080808;",
            // ── Stereo multitap viz ──
            svg { class: "h-full min-h-0", style: "flex: 1 1 0%;", view_box: "0 0 220 100", preserve_aspect_ratio: "none",
                line { x1: "0", y1: "50", x2: "220", y2: "50", stroke: "#3f3f46", stroke_width: "1" }
                // Dry impulse through the middle.
                rect { x: "5", y: "26", width: "2", height: "48", fill: "#e4e4e7", rx: "1" }
                for (di, b) in delays.iter().enumerate() {
                    {
                        let fb = param_v(b, "feedback", 0.3).clamp(0.0, 0.98);
                        let mix = param_v(b, "mix", 0.1).clamp(0.05, 1.0);
                        let t_l = quarter * div_factor(param_v(b, "div_l", 4.0));
                        let t_r = quarter * div_factor(param_v(b, "div_r", 4.0));
                        let color = DELAY_COLORS[di % 2];
                        let dim = b.bypassed;
                        let stems = |side_t: f32, up: bool| -> Vec<(f32, f32, bool)> {
                            let mut out = Vec::new();
                            let (mut amp, mut t) = (mix, side_t);
                            while t <= win_ms && amp > 0.015 && out.len() < 24 {
                                out.push((t, amp, up));
                                amp *= fb;
                                t += side_t;
                            }
                            out
                        };
                        let mut taps = stems(t_l, true);
                        taps.extend(stems(t_r, false));
                        rsx! {
                            for (i, (t, amp, up)) in taps.iter().enumerate() {
                                rect {
                                    key: "{di}-{i}",
                                    x: "{5.0 + t / win_ms * (W - 10.0):.1}",
                                    y: if *up { format!("{:.1}", 50.0 - amp * 44.0) } else { "50".to_string() },
                                    width: "2",
                                    height: "{amp * 44.0:.1}",
                                    fill: "{color}",
                                    fill_opacity: if dim { "0.18" } else { "0.85" },
                                    rx: "1",
                                }
                            }
                        }
                    }
                }
                text { x: "8", y: "12", fill: "#52525b", font_size: "8", "L" }
                text { x: "8", y: "96", fill: "#52525b", font_size: "8", "R" }
            }

            // ── Parameter surface for the selected delay ──
            div { class: "flex flex-col gap-1 p-1.5 border-l border-border flex-shrink-0", style: "width: 210px;",
                div { class: "flex items-center gap-1",
                    span { style: "font-size:8px; font-weight:600; text-transform:uppercase; color:#8a8a92;", "Delay" }
                    for i in 0..delays.len() {
                        button {
                            key: "{i}",
                            class: if sel() == i { "w-4 h-4 rounded-sm text-[9px] font-bold" } else { "w-4 h-4 rounded-sm text-[9px] text-muted-foreground border border-border" },
                            style: if sel() == i { format!("background-color: {}; color: #000;", DELAY_COLORS[i % 2]) } else { String::new() },
                            onclick: move |_| sel.set(i),
                            "{i + 1}"
                        }
                    }
                    if cur.bypassed {
                        span { class: "text-[8px] text-muted-foreground", "· bypassed" }
                    }
                    div { class: "ml-auto",
                        ParamSelect {
                            block_id: cur_id.clone(),
                            name: "algo",
                            label: "Algo",
                            value: param_v(&cur, "algo", 0.0),
                            options: DELAY_ALGOS.to_vec(),
                        }
                    }
                }
                div { class: "flex gap-2",
                    ParamSelect {
                        block_id: cur_id.clone(),
                        name: "div_l",
                        label: "Time L",
                        value: param_v(&cur, "div_l", 4.0),
                        options: DIV_LABELS.to_vec(),
                    }
                    ParamSelect {
                        block_id: cur_id.clone(),
                        name: "div_r",
                        label: "Time R",
                        value: param_v(&cur, "div_r", 4.0),
                        options: DIV_LABELS.to_vec(),
                    }
                }
                div { class: "flex gap-1 justify-between",
                    if let Some(p) = param(&cur, "hp") {
                        PKnob { block_id: cur_id.clone(), name: "hp", label: "HP", p }
                    }
                    if let Some(p) = param(&cur, "lp") {
                        PKnob { block_id: cur_id.clone(), name: "lp", label: "LP", p }
                    }
                    if let Some(p) = param(&cur, "duck") {
                        PKnob { block_id: cur_id.clone(), name: "duck", label: "Duck", p }
                    }
                    if let Some(p) = param(&cur, "mix") {
                        PKnob { block_id: cur_id.clone(), name: "mix", label: "Mix", p }
                    }
                }
            }
        }
    }
}

/// The stereo reverb module: mirrored decay tails per reverb plus the
/// parameter surface — mix, time, HP/LP, modulation, algorithm.
#[component]
fn ReverbPanel(blocks: Vec<LiveBlock>) -> Element {
    let mut sel = use_signal(|| 0usize);
    const W: f32 = 220.0;
    let verbs: Vec<LiveBlock> = blocks
        .iter()
        .filter(|b| b.block_type == BlockType::Reverb)
        .cloned()
        .collect();
    if verbs.is_empty() {
        return rsx! { span { class: "text-xs text-muted-foreground italic p-2", "No reverbs in the chain." } };
    }
    let cur = verbs[sel().min(verbs.len() - 1)].clone();
    let cur_id = cur.id.clone();

    rsx! {
        div { class: "flex h-full min-h-0", style: "background: #080808;",
            // ── Stereo tail viz (mirrored) ──
            svg { class: "h-full min-h-0", style: "flex: 1 1 0%;", view_box: "0 0 220 100", preserve_aspect_ratio: "none",
                line { x1: "0", y1: "50", x2: "220", y2: "50", stroke: "#3f3f46", stroke_width: "1" }
                for (vi, b) in verbs.iter().enumerate() {
                    {
                        let decay = param_v(b, "decay", 0.4).clamp(0.02, 1.0);
                        let size = param_v(b, "size", 0.5);
                        let mix = param_v(b, "mix", 0.1).clamp(0.05, 1.0);
                        let md = param_v(b, "mod", 0.2);
                        let color = VERB_COLORS[vi % 2];
                        let dim = b.bypassed;
                        let tau = 0.08 + decay * size.max(0.1) * 0.9;
                        // Mirrored tail with a little modulation wiggle.
                        let mut top = String::from("M 5 50 ");
                        let mut bot = String::from("M 5 50 ");
                        for px in 0..=64 {
                            let t = px as f32 / 64.0;
                            let wig = 1.0 + (t * 24.0).sin() * md * 0.18;
                            let h = mix * (-t / tau).exp() * wig * 44.0;
                            let x = 5.0 + t * (W - 10.0);
                            top.push_str(&format!("L {x:.1} {:.1} ", 50.0 - h));
                            bot.push_str(&format!("L {x:.1} {:.1} ", 50.0 + h));
                        }
                        top.push_str("L 215 50 Z");
                        bot.push_str("L 215 50 Z");
                        rsx! {
                            path { key: "t{vi}", d: "{top}", fill: "{color}", fill_opacity: if dim { "0.08" } else { "0.22" },
                                stroke: "{color}", stroke_opacity: if dim { "0.2" } else { "0.75" }, stroke_width: "1" }
                            path { key: "b{vi}", d: "{bot}", fill: "{color}", fill_opacity: if dim { "0.06" } else { "0.16" },
                                stroke: "{color}", stroke_opacity: if dim { "0.15" } else { "0.5" }, stroke_width: "1" }
                        }
                    }
                }
            }

            // ── Parameter surface for the selected reverb ──
            div { class: "flex flex-col gap-1 p-1.5 border-l border-border flex-shrink-0", style: "width: 210px;",
                div { class: "flex items-center gap-1",
                    span { style: "font-size:8px; font-weight:600; text-transform:uppercase; color:#8a8a92;", "Reverb" }
                    for i in 0..verbs.len() {
                        button {
                            key: "{i}",
                            class: if sel() == i { "w-4 h-4 rounded-sm text-[9px] font-bold" } else { "w-4 h-4 rounded-sm text-[9px] text-muted-foreground border border-border" },
                            style: if sel() == i { format!("background-color: {}; color: #000;", VERB_COLORS[i % 2]) } else { String::new() },
                            onclick: move |_| sel.set(i),
                            "{i + 1}"
                        }
                    }
                    if cur.bypassed {
                        span { class: "text-[8px] text-muted-foreground", "· bypassed" }
                    }
                    div { class: "ml-auto",
                        ParamSelect {
                            block_id: cur_id.clone(),
                            name: "algo",
                            label: "Algo",
                            value: param_v(&cur, "algo", 0.0),
                            options: VERB_ALGOS.to_vec(),
                        }
                    }
                }
                div { class: "flex gap-1 justify-between",
                    if let Some(p) = param(&cur, "mix") {
                        PKnob { block_id: cur_id.clone(), name: "mix", label: "Mix", p }
                    }
                    if let Some(p) = param(&cur, "decay") {
                        PKnob { block_id: cur_id.clone(), name: "decay", label: "Time", p }
                    }
                    if let Some(p) = param(&cur, "hp") {
                        PKnob { block_id: cur_id.clone(), name: "hp", label: "HP", p }
                    }
                    if let Some(p) = param(&cur, "lp") {
                        PKnob { block_id: cur_id.clone(), name: "lp", label: "LP", p }
                    }
                    if let Some(p) = param(&cur, "mod") {
                        PKnob { block_id: cur_id.clone(), name: "mod", label: "Mod", p }
                    }
                }
            }
        }
    }
}

/// Modulation visualizer: the active modulation block's LFO (rate × depth)
/// over a two-second window. Dormant flat line when the module is bypassed.
#[component]
fn ModViz(blocks: Vec<LiveBlock>) -> Element {
    const W: f32 = 200.0;
    let mods: Vec<LiveBlock> = blocks
        .iter()
        .filter(|b| {
            matches!(
                b.block_type,
                BlockType::Chorus | BlockType::Flanger | BlockType::Phaser
                    | BlockType::Trem | BlockType::Vibrato | BlockType::Rotary
            )
        })
        .cloned()
        .collect();
    let active = mods.iter().find(|b| !b.bypassed);
    let (label, d) = match active {
        Some(b) => {
            let rate = param_v(b, "rate", 0.3);
            let depth = param_v(b, "depth", 0.5).clamp(0.05, 1.0);
            // rate 0..1 → 0.25..8 Hz over a 2 s window.
            let hz = 0.25 + rate * 7.75;
            let cycles = hz * 2.0;
            let mut d = String::new();
            for px in 0..=96 {
                let t = px as f32 / 96.0;
                let y = 40.0 - (t * cycles * std::f32::consts::TAU).sin() * depth * 30.0;
                d.push_str(if px == 0 { "M " } else { "L " });
                d.push_str(&format!("{:.1} {:.1} ", 6.0 + t * (W - 12.0), y));
            }
            (b.name.clone(), d)
        }
        None => (String::new(), format!("M 6 40 L {} 40", W - 6.0)),
    };
    rsx! {
        div { class: "relative flex flex-col h-full min-h-0", style: "background: #080808;",
            svg { class: "w-full flex-1 min-h-0", view_box: "0 0 200 80", preserve_aspect_ratio: "none",
                line { x1: "0", y1: "40", x2: "200", y2: "40", stroke: "#27272a", stroke_width: "1" }
                path {
                    d: "{d}",
                    fill: "none",
                    stroke: if active.is_some() { "#f472b6" } else { "#3f3f46" },
                    stroke_width: "1.5",
                }
            }
            div { class: "absolute top-0.5 left-1.5 flex items-baseline gap-2",
                span { style: "font-size:8px; font-weight:600; text-transform:uppercase; color:#8a8a92;", "Mod" }
                if !label.is_empty() {
                    span { style: "font-size:8px; color:#f472b6;", "{label}" }
                } else {
                    span { style: "font-size:8px; color:#52525b;", "none active" }
                }
            }
        }
    }
}

// ── The Control view ────────────────────────────────────────────────────────

/// The guitar instrument panel — see the module docs for the layout.
#[component]
pub fn ControlView(
    model: PerformanceModel,
    state: RigViewState,
    on_open_tuner: Callback<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let blocks = state.blocks.cloned();
    let in_db = state.in_peak_db.cloned();
    let out_db = state.out_peak_db.cloned();
    let gr_db = state.comp_gr_db.cloned();
    let spectrum = state.spectrum.cloned();
    let comp_wave = state.comp_wave.cloned();

    let eq = find_block(&blocks, BlockType::Eq, "Amp EQ");
    let comp = find_block(&blocks, BlockType::Compressor, "Compressor");
    let gate = find_block(&blocks, BlockType::Gate, "Gate");

    let hp = model.headphone.clone();
    // Headphone meter: main signal scaled by phones volume (the physical bus
    // lands with engine multi-out; the meter shows what it will carry).
    let hp_db = out_db + 20.0 * hp.volume.max(0.001).log10();
    let main_db = if hp.main_mute { -90.0 } else { out_db };


    rsx! {
        div { class: "flex gap-1 h-full min-h-0 overflow-hidden",
            // ── Input meter rail ──
            div { class: "w-9 flex-shrink-0", VMeter { label: "In", level_db: in_db } }

            // ── Center surface ──
            div { class: "flex flex-col gap-1 flex-1 min-w-0 min-h-0",
                // Main modules, in signal order: Compressor → Gate → Amp EQ,
                // with the time section (Delay | Reverb) docked flush beneath.
                div { class: "flex flex-col gap-0 min-h-0",
                    div { class: "flex gap-0 min-h-0 w-full", style: "aspect-ratio: 25 / 9; max-height: 52%;",
                        // Height-driven square: width follows the row height.
                        div { class: "min-h-0 h-full aspect-square flex flex-col flex-shrink-0",
                            ZoomPanel { title: "Compressor".to_string(),
                                if let Some(comp) = comp {
                                    crate::comp_surface::CompSurface {
                                        block: comp,
                                        wave: comp_wave,
                                        in_db,
                                        gr_db,
                                    }
                                } else {
                                    span { class: "text-xs text-muted-foreground italic", "No Compressor in the chain." }
                                }
                            }
                        }
                        // The gate, tall and slim — level vs threshold at a glance.
                        div { class: "min-h-0 h-full w-14 flex flex-col flex-shrink-0",
                            ZoomPanel { title: "Gate".to_string(),
                                if let Some(gate) = gate {
                                    GatePanel { block: gate, in_db }
                                } else {
                                    span { class: "text-xs text-muted-foreground italic", "No Gate." }
                                }
                            }
                        }
                        div { class: "min-h-0 flex flex-col", style: "flex: 1 1 0%;",
                            ZoomPanel { title: "Amp EQ".to_string(),
                                if let Some(eq) = eq {
                                    crate::eq_surface::EqProSurface { block: eq, spectrum }
                                } else {
                                    span { class: "text-xs text-muted-foreground italic", "No Amp EQ in the chain." }
                                }
                            }
                        }
                    }
                    // Time section: stereo delay + stereo reverb.
                    div { class: "flex gap-0 min-h-0 w-full", style: "height: 128px;",
                        div { class: "min-h-0 h-full flex flex-col", style: "flex: 1 1 0%;",
                            ZoomPanel { title: "Delay".to_string(),
                                DelayPanel { blocks: blocks.clone(), tempo_bpm: model.tempo_bpm }
                            }
                        }
                        div { class: "min-h-0 h-full flex flex-col", style: "flex: 1 1 0%;",
                            ZoomPanel { title: "Reverb".to_string(),
                                ReverbPanel { blocks: blocks.clone() }
                            }
                        }
                    }
                }

                div { class: "flex-1 min-h-0" }

                // Bottom rail: the modulation visualizer, rest open.
                div { class: "flex gap-1 flex-shrink-0 items-end", style: "height: 96px;",
                    div { class: "min-h-0 h-full flex flex-col w-72",
                        ZoomPanel { title: "Modulation".to_string(),
                            ModViz { blocks: blocks.clone() }
                        }
                    }
                    div { class: "flex-1" }
                }

                // The tuner, gum-stick style: one short strip across the
                // bottom — note left, needle bar center, cents right.
                MiniTuner { on_open: on_open_tuner }
            }

            // ── Output rail: mute on top, then FOH trim + out meter,
            // then the phones group — mix fader | phones meter | guitar
            // (self) fader.
            div { class: "w-24 flex-shrink-0 flex flex-col items-center gap-1 min-h-0",
                button {
                    class: if hp.main_mute {
                        "w-12 rounded-md px-1 py-1 text-[9px] font-bold uppercase ring-2 ring-red-500"
                    } else {
                        "w-12 rounded-md px-1 py-1 text-[9px] font-bold uppercase border border-border text-muted-foreground hover:text-foreground"
                    },
                    style: if hp.main_mute { "background-color: #ef4444; color: #fff;" } else { "" },
                    onclick: {
                        let rig = rig.clone();
                        move |_| {
                            if let Some(r) = rig.clone() {
                                spawn(async move { let _ = r.toggle_main_mute().await; });
                            }
                        }
                    },
                    if hp.main_mute { "Muted" } else { "Mute" }
                }
                div { class: "flex-1 min-h-0 w-full flex justify-center gap-1.5",
                    VFader {
                        label: "Trim",
                        value: (model.master_trim_db + 24.0) / 36.0,
                        readout: format!("{:+.0}dB", model.master_trim_db),
                        on_change: Callback::new({
                            let rig = rig.clone();
                            move |v: f32| {
                                if let Some(r) = rig.clone() {
                                    spawn(async move { let _ = r.set_master_trim(v * 36.0 - 24.0).await; });
                                }
                            }
                        }),
                    }
                    VMeter { label: "Out", level_db: main_db, muted: hp.main_mute }
                }
                div { class: "flex-1 min-h-0 w-full flex justify-center gap-1.5",
                    VFader {
                        label: "Mix",
                        value: hp.volume,
                        readout: format!("{:.0}%", hp.volume * 100.0),
                        on_change: Callback::new({
                            let rig = rig.clone();
                            let self_mix = hp.self_mix;
                            move |v: f32| {
                                if let Some(r) = rig.clone() {
                                    spawn(async move { let _ = r.set_headphone(v, self_mix).await; });
                                }
                            }
                        }),
                    }
                    VMeter { label: "Phns", level_db: hp_db }
                    VFader {
                        label: "Gtr",
                        value: hp.self_mix,
                        readout: format!("{:.0}%", hp.self_mix * 100.0),
                        on_change: Callback::new({
                            let rig = rig.clone();
                            let vol = hp.volume;
                            move |v: f32| {
                                if let Some(r) = rig.clone() {
                                    spawn(async move { let _ = r.set_headphone(vol, v).await; });
                                }
                            }
                        }),
                    }
                }
            }
        }
    }
}
