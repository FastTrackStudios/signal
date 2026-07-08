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
pub fn ZoomPanel(
    title: String,
    children: Element,
    /// Fullscreen-only content — when set, the overlay renders this instead
    /// of `children` (e.g. the gate's expanded editor with attack/release).
    #[props(default)]
    zoomed_view: Option<Element>,
) -> Element {
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
                div { class: "flex-1 min-h-0", {zoomed_view.unwrap_or(children)} }
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

/// A stereo vertical meter: two flush bars (L/R) with a shared dB readout.
#[component]
fn StereoMeter(label: &'static str, l_db: f32, r_db: f32, #[props(default)] muted: bool) -> Element {
    let bar = |db: f32| -> (f32, &'static str) {
        let pct = ((db + 60.0) / 60.0 * 100.0).clamp(0.0, 100.0);
        let color = if muted {
            "#3f3f46"
        } else if db > -6.0 {
            "#ef4444"
        } else if db > -18.0 {
            "#eab308"
        } else {
            "#22c55e"
        };
        (pct, color)
    };
    let (lp, lc) = bar(l_db);
    let (rp, rc) = bar(r_db);
    let max_db = l_db.max(r_db);
    rsx! {
        div { class: "flex flex-col items-center h-full min-h-0 w-full",
            span { class: "text-[8px] font-semibold uppercase tracking-wider text-muted-foreground", "{label}" }
            div { class: "flex flex-1 w-full min-h-0 bg-black/60 border border-border overflow-hidden",
                div { class: "relative flex-1 h-full",
                    div { class: "absolute inset-x-0 bottom-0 transition-[height] duration-75",
                        style: "height: {lp}%; background-color: {lc};" }
                }
                div { class: "w-px bg-black" }
                div { class: "relative flex-1 h-full",
                    div { class: "absolute inset-x-0 bottom-0 transition-[height] duration-75",
                        style: "height: {rp}%; background-color: {rc};" }
                }
            }
            span { class: "text-[8px] font-mono text-muted-foreground",
                if max_db <= -89.0 { "−∞" } else { {format!("{max_db:.0}")} }
            }
        }
    }
}

// ── Time-section constants ──────────────────────────────────────────────────

const DELAY_COLORS: [&str; 2] = ["#38bdf8", "#818cf8"];
const VERB_COLORS: [&str; 2] = ["#2dd4bf", "#a78bfa"];

/// Tempo-division labels — `delay::TapDivision` order (Quarter, dotted 8th,
/// 8th, triplet, 16th, golden ratio, silver ratio, free-running).
const DIV_LABELS: [&str; 8] = ["1/4", "1/8.", "1/8", "1/4T", "1/16", "Golden", "Silver", "Free"];

/// Division → multiple of a quarter note, for the tap visualization
/// (Free returns 0 → the caller falls back to the block's `time`).
fn div_factor(idx: f32) -> f32 {
    [1.0, 0.75, 0.5, 1.0 / 3.0, 0.25, 0.618, 0.414, 0.0][(idx as usize).min(7)]
}

/// `delay::DelayStyle` order — the TimeLine MX machines.
const DELAY_ALGOS: [&str; 13] = [
    "Tape", "Digital", "dBucket", "Lo-Fi", "Shimmer", "Reverse", "Ice",
    "Rhythm", "Drum", "Oil Can", "MultiTap", "Spectral", "Filter",
];
/// `reverb::AlgorithmType::ALL` order.
const VERB_ALGOS: [&str; 15] = [
    "Room", "Hall", "Plate", "Spring", "Cloud", "Bloom", "Shimmer", "Chorale",
    "Magneto", "NonLinear", "Swell", "Reflections", "Velvet", "FreeVerb", "Convolution",
];

/// The gate, tall and slim: the level bar with a draggable threshold and a
/// live gain-reduction strip (red, from the top) so gating is visible the
/// moment it happens. `expanded` (the zoomed view) adds attack/release.
#[component]
fn GatePanel(block: LiveBlock, in_db: f32, #[props(default)] expanded: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let mut tracking = use_signal(|| false);

    let thr = param_v(&block, "threshold", -50.0);
    let open = in_db >= thr && !block.bypassed;
    let level_pct = ((in_db + 90.0) / 90.0 * 100.0).clamp(0.0, 100.0);
    let thr_pct = ((thr + 90.0) / 90.0 * 100.0).clamp(0.0, 100.0);
    // Gain reduction estimate while closed: how far the signal sits under
    // the threshold (the gate attenuates toward silence). Red strip depth.
    // Only indicate reduction when actual signal is being clamped — at the
    // noise floor the gate is technically attenuating silence, which reads
    // as a stuck red bar.
    let gr_db = if open || block.bypassed || in_db < -75.0 {
        0.0
    } else {
        (thr - in_db).clamp(0.0, 40.0)
    };
    let gr_pct = (gr_db / 40.0 * 100.0).clamp(0.0, 100.0);

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
        div { class: "flex flex-col items-center gap-1 h-full min-h-0 pt-4 pb-1",
            // The bar: level from below, GR strip from above, threshold line.
            div {
                class: "relative flex-1 w-full max-w-10 bg-black/60 border border-border overflow-hidden min-h-0 cursor-ns-resize touch-none",
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
                // Input level.
                div {
                    class: "absolute inset-x-0 bottom-0 transition-[height] duration-75",
                    style: if open { "height: {level_pct}%; background-color: #22c55e;" }
                           else { "height: {level_pct}%; background-color: #52525b;" },
                }
                // Gain reduction — red from the top while the gate clamps.
                if gr_pct > 0.5 {
                    div {
                        class: "absolute inset-x-0 top-0 transition-[height] duration-75",
                        style: "height: {gr_pct}%; background-color: rgba(239,68,68,0.45);",
                    }
                }
                // Threshold line (the draggable thing).
                div {
                    class: "absolute inset-x-0",
                    style: "bottom: {thr_pct}%; height: 2px; background-color: #eab308; box-shadow: 0 0 4px rgba(234,179,8,0.6);",
                }
            }
            span { class: "text-[8px] font-mono text-muted-foreground flex-shrink-0",
                if gr_db > 0.5 { {format!("−{gr_db:.0}")} } else { {format!("{thr:.0}")} }
            }
            if expanded {
                div { class: "flex gap-3 flex-shrink-0",
                    for name in ["attack", "release"] {
                        if let Some(p) = param(&block, name) {
                            {
                                let rig = rig.clone();
                                let id = block.id.clone();
                                let pname = name.to_string();
                                rsx! {
                                    crate::knob::Knob {
                                        label: if name == "attack" { "Attack".to_string() } else { "Release".to_string() },
                                        value: p.value,
                                        min: p.min,
                                        max: p.max,
                                        size: crate::knob::KnobSize::Medium,
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
}

/// A slim vertical fader: drag to set, small readout below.
#[component]
fn VFader(
    label: &'static str,
    /// Normalized position 0..1.
    value: f32,
    /// Readout text.
    readout: String,
    on_change: Callback<f32>,
) -> Element {
    let mut el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let mut tracking = use_signal(|| false);
    let pct = (value * 100.0).clamp(0.0, 100.0);
    let set_from = move |coords: dioxus::html::geometry::ElementPoint| {
        let el = el();
        spawn(async move {
            let Some(el) = el else { return };
            let Ok(rect) = el.get_client_rect().await else { return };
            on_change.call((1.0 - coords.y / rect.height()).clamp(0.0, 1.0) as f32);
        });
    };
    rsx! {
        div { class: "flex flex-col items-center h-full min-h-0",
            span { class: "text-[8px] font-semibold uppercase tracking-wider text-muted-foreground", "{label}" }
            div {
                class: "relative flex-1 w-3 bg-black/60 border border-border min-h-0 cursor-ns-resize touch-none",
                onmounted: move |e| el.set(Some(e.data())),
                onpointerdown: move |e: PointerEvent| { tracking.set(true); set_from(e.element_coordinates()); },
                onpointermove: move |e: PointerEvent| { if tracking() { set_from(e.element_coordinates()); } },
                onpointerup: move |_| tracking.set(false),
                onpointerleave: move |_| tracking.set(false),
                div {
                    class: "absolute inset-x-0 h-2 border border-zinc-500",
                    style: "bottom: calc({pct}% - 4px); background-color: #3f3f46;",
                }
            }
            span { class: "text-[8px] font-mono text-muted-foreground", "{readout}" }
        }
    }
}

/// MIDI monitor behind a header icon — system-wide, out of the surface.
/// Shows a dot when events have been seen; click for the full log.
#[component]
pub fn MidiMonitorButton() -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut log = use_signal(Vec::<String>::new);
    let mut open = use_signal(|| false);
    {
        let rig = rig.clone();
        use_future(move || {
            let rig = rig.clone();
            async move {
                let Some(rig) = rig else { return };
                loop {
                    if let Ok(l) = rig.midi_recent().await {
                        log.set(l);
                    }
                    architect::platform::sleep(Duration::from_millis(800)).await;
                }
            }
        });
    }
    let entries = log();
    let seen = !entries.is_empty();
    rsx! {
        button {
            class: "relative flex items-center justify-center w-8 h-8 rounded-md border border-border text-muted-foreground hover:text-foreground",
            title: "MIDI monitor",
            onclick: move |_| open.set(true),
            span { class: "text-[10px] font-bold tracking-tight", "MIDI" }
            if seen {
                span { class: "absolute top-0.5 right-0.5 w-1.5 h-1.5 rounded-full bg-emerald-400" }
            }
        }
        if open() {
            div {
                class: "fixed inset-0 z-50 flex flex-col bg-black/95 p-8",
                onclick: move |_| open.set(false),
                div { class: "flex items-center mb-4",
                    span { class: "text-sm font-bold uppercase tracking-wider", "MIDI Monitor" }
                    span { class: "ml-auto text-xs text-muted-foreground", "tap anywhere to close" }
                }
                div { class: "flex-1 overflow-y-auto font-mono text-xs flex flex-col-reverse gap-0.5",
                    for (i, e) in entries.iter().enumerate().rev() {
                        div { key: "{i}", class: "text-muted-foreground", "{e}" }
                    }
                    if entries.is_empty() {
                        span { class: "italic", "listening — no MIDI events yet" }
                    }
                }
            }
        }
    }
}

/// Algorithm picker: the current algorithm reads as the module's title;
/// clicking opens a dialog grid — room to grow as machines get added.
#[component]
fn AlgoPicker(
    block_id: String,
    name: &'static str,
    value: f32,
    options: Vec<&'static str>,
    accent: String,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut open = use_signal(|| false);
    let current = options
        .get(value as usize)
        .copied()
        .unwrap_or(options.first().copied().unwrap_or("—"));
    rsx! {
        button {
            class: "flex items-center gap-1 rounded-sm border border-border px-1.5 py-0.5 hover:bg-accent/30",
            onclick: move |_| open.set(true),
            span {
                class: "text-[11px] font-bold tracking-wide",
                style: "color: {accent};",
                "{current}"
            }
            span { class: "text-[8px] text-muted-foreground", "▾" }
        }
        if open() {
            div {
                class: "fixed inset-0 z-50 flex items-center justify-center bg-black/80",
                onclick: move |_| open.set(false),
                div {
                    class: "grid grid-cols-3 gap-1 p-3 rounded-lg border border-border bg-card max-w-md",
                    for (i, o) in options.iter().enumerate() {
                        {
                            let rig = rig.clone();
                            let block_id = block_id.clone();
                            let is_cur = i == value as usize;
                            let accent = accent.clone();
                            rsx! {
                                button {
                                    key: "{i}",
                                    class: if is_cur { "rounded px-3 py-2 text-xs font-bold" } else { "rounded px-3 py-2 text-xs text-muted-foreground border border-border hover:bg-accent/40" },
                                    style: if is_cur { format!("background-color: {accent}; color: #000;") } else { String::new() },
                                    onclick: move |_| {
                                        send_param(&rig, &block_id, name, i as f32);
                                        open.set(false);
                                    },
                                    "{o}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

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

/// The stereo delay module, wide: one full-width visualizer per delay
/// stacked (1 top, 2 bottom) — click a lane to select it — with the
/// selected delay's controls in a strip beneath.
#[component]
fn DelayPanel(blocks: Vec<LiveBlock>, tempo_bpm: u32) -> Element {
    let mut sel = use_signal(|| 0usize);
    const W: f32 = 460.0;
    let delays: Vec<LiveBlock> = blocks
        .iter()
        .filter(|b| b.block_type == BlockType::Delay)
        .cloned()
        .collect();
    if delays.is_empty() {
        return rsx! { span { class: "text-xs text-muted-foreground italic p-2", "No delays in the chain." } };
    }
    let quarter = 60_000.0 / tempo_bpm.max(1) as f32;
    let win_ms = quarter * 8.0;

    let cur = delays[sel().min(delays.len() - 1)].clone();
    let cur_id = cur.id.clone();

    rsx! {
        div { class: "flex flex-col h-full min-h-0", style: "background: #080808;",
            // ── One lane per delay: full-width stereo multitap ──
            for (di, b) in delays.iter().enumerate() {
                {
                    let fb = param_v(b, "feedback", 0.3).clamp(0.0, 0.98);
                    let mix = param_v(b, "mix", 0.08).clamp(0.02, 0.10) / 0.10;
                    let time_ms = param_v(b, "time", 350.0);
                    let f_l = div_factor(param_v(b, "tap_div_l", 0.0));
                    let f_r = div_factor(param_v(b, "tap_div_r", 0.0));
                    let t_l = if f_l > 0.0 { quarter * f_l } else { time_ms };
                    let t_r = if f_r > 0.0 { quarter * f_r } else { time_ms };
                    let color = DELAY_COLORS[di % 2];
                    let dim = b.bypassed;
                    let is_sel = sel() == di;
                    let stems = |side_t: f32, up: bool| -> Vec<(f32, f32, bool)> {
                        let mut out = Vec::new();
                        let (mut amp, mut t) = (mix, side_t);
                        while t <= win_ms && amp > 0.015 && out.len() < 32 {
                            out.push((t, amp, up));
                            amp *= fb;
                            t += side_t;
                        }
                        out
                    };
                    let mut taps = stems(t_l, true);
                    taps.extend(stems(t_r, false));
                    rsx! {
                        div {
                            key: "lane{di}",
                            class: if is_sel { "relative flex-1 min-h-0 cursor-pointer" } else { "relative flex-1 min-h-0 cursor-pointer opacity-60 hover:opacity-90" },
                            style: if is_sel { format!("order: {}; border-left: 2px solid {color}; background: {color}0a;", di * 2) } else { format!("order: {}; border-left: 2px solid transparent;", di * 2) },
                            onclick: move |_| sel.set(di),
                            svg { class: "w-full h-full", view_box: "0 0 460 56", preserve_aspect_ratio: "none",
                                line { x1: "0", y1: "28", x2: "460", y2: "28", stroke: "#27272a", stroke_width: "1" }
                                rect { x: "4", y: "14", width: "2", height: "28", fill: "#e4e4e7", rx: "1" }
                                for (i, (t, amp, upv)) in taps.iter().enumerate() {
                                    rect {
                                        key: "{i}",
                                        x: "{4.0 + t / win_ms * (W - 8.0):.1}",
                                        y: if *upv { format!("{:.1}", 28.0 - amp * 26.0) } else { "28".to_string() },
                                        width: "2",
                                        height: "{amp * 26.0:.1}",
                                        fill: "{color}",
                                        fill_opacity: if dim { "0.25" } else { "0.9" },
                                        rx: "1",
                                    }
                                }
                            }
                            div { class: "absolute top-0.5 left-1.5 flex items-baseline gap-1.5",
                                span { style: "font-size:8px; font-weight:700; color:{color};", "{di + 1}" }
                                span { style: "font-size:8px; color:#8a8a92;",
                                    "{DELAY_ALGOS[param_v(b, \"style\", 1.0) as usize % 13]}"
                                }
                                if dim {
                                    span { style: "font-size:8px; color:#52525b;", "bypassed" }
                                }
                            }
                        }
                    }
                }
            }


            // ── Controls for the selected delay ──
            div { class: "flex items-end gap-1.5 px-1.5 py-1 border-y border-border flex-shrink-0", style: "order: 1;",
                ParamSelect {
                    block_id: cur_id.clone(),
                    name: "tap_div_l",
                    label: "Time L",
                    value: param_v(&cur, "tap_div_l", 0.0),
                    options: DIV_LABELS.to_vec(),
                }
                ParamSelect {
                    block_id: cur_id.clone(),
                    name: "tap_div_r",
                    label: "Time R",
                    value: param_v(&cur, "tap_div_r", 0.0),
                    options: DIV_LABELS.to_vec(),
                }
                if let Some(p) = param(&cur, "high_pass") {
                    PKnob { block_id: cur_id.clone(), name: "high_pass", label: "HP", p }
                }
                if let Some(p) = param(&cur, "repeat_dyn") {
                    PKnob { block_id: cur_id.clone(), name: "repeat_dyn", label: "Duck", p }
                }
                if let Some(p) = param(&cur, "feedback") {
                    PKnob { block_id: cur_id.clone(), name: "feedback", label: "FB", p }
                }
                if let Some(p) = param(&cur, "mix") {
                    PKnob { block_id: cur_id.clone(), name: "mix", label: "Mix", p }
                }
                div { class: "ml-auto",
                    AlgoPicker {
                        block_id: cur_id.clone(),
                        name: "style",
                        value: param_v(&cur, "style", 1.0),
                        options: DELAY_ALGOS.to_vec(),
                        accent: DELAY_COLORS[sel().min(1)].to_string(),
                    }
                }
            }

        }
    }
}

/// The stereo reverb module, wide: one full-width mirrored tail per reverb
/// stacked — click a lane to select — with the selected reverb's controls
/// beneath.
#[component]
fn ReverbPanel(blocks: Vec<LiveBlock>) -> Element {
    let mut sel = use_signal(|| 0usize);
    const W: f32 = 460.0;
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
        div { class: "flex flex-col h-full min-h-0", style: "background: #080808;",
            for (vi, b) in verbs.iter().enumerate() {
                {
                    let decay = param_v(b, "decay", 0.4).clamp(0.02, 1.0);
                    let size = param_v(b, "size", 0.5);
                    let mix = (param_v(b, "mix", 0.08).clamp(0.02, 0.10) / 0.10).max(0.15);
                    let md = param_v(b, "modulation", 0.2);
                    let color = VERB_COLORS[vi % 2];
                    let dim = b.bypassed;
                    let is_sel = sel() == vi;
                    let tau = 0.08 + decay * size.max(0.1) * 0.9;
                    let mut top = String::from("M 4 28 ");
                    let mut bot = String::from("M 4 28 ");
                    for px in 0..=80 {
                        let t = px as f32 / 80.0;
                        let wig = 1.0 + (t * 24.0).sin() * md * 0.18;
                        let h = mix * (-t / tau).exp() * wig * 26.0;
                        let x = 4.0 + t * (W - 8.0);
                        top.push_str(&format!("L {x:.1} {:.1} ", 28.0 - h));
                        bot.push_str(&format!("L {x:.1} {:.1} ", 28.0 + h));
                    }
                    top.push_str("L 456 28 Z");
                    bot.push_str("L 456 28 Z");
                    rsx! {
                        div {
                            key: "lane{vi}",
                            class: if is_sel { "relative flex-1 min-h-0 cursor-pointer" } else { "relative flex-1 min-h-0 cursor-pointer opacity-60 hover:opacity-90" },
                            style: if is_sel { format!("order: {}; border-left: 2px solid {color}; background: {color}0a;", vi * 2) } else { format!("order: {}; border-left: 2px solid transparent;", vi * 2) },
                            onclick: move |_| sel.set(vi),
                            svg { class: "w-full h-full", view_box: "0 0 460 56", preserve_aspect_ratio: "none",
                                line { x1: "0", y1: "28", x2: "460", y2: "28", stroke: "#27272a", stroke_width: "1" }
                                path { d: "{top}", fill: "{color}", fill_opacity: if dim { "0.08" } else { "0.25" },
                                    stroke: "{color}", stroke_opacity: if dim { "0.25" } else { "0.8" }, stroke_width: "1" }
                                path { d: "{bot}", fill: "{color}", fill_opacity: if dim { "0.06" } else { "0.18" },
                                    stroke: "{color}", stroke_opacity: if dim { "0.2" } else { "0.55" }, stroke_width: "1" }
                            }
                            div { class: "absolute top-0.5 left-1.5 flex items-baseline gap-1.5",
                                span { style: "font-size:8px; font-weight:700; color:{color};", "{vi + 1}" }
                                span { style: "font-size:8px; color:#8a8a92;",
                                    "{VERB_ALGOS[param_v(b, \"algorithm\", 1.0) as usize % 15]}"
                                }
                                if dim {
                                    span { style: "font-size:8px; color:#52525b;", "bypassed" }
                                }
                            }
                        }
                    }
                }
            }


            // ── Controls for the selected reverb ──
            div { class: "flex items-end gap-1.5 px-1.5 py-1 border-y border-border flex-shrink-0", style: "order: 1;",
                if let Some(p) = param(&cur, "mix") {
                    PKnob { block_id: cur_id.clone(), name: "mix", label: "Mix", p }
                }
                if let Some(p) = param(&cur, "decay") {
                    PKnob { block_id: cur_id.clone(), name: "decay", label: "Time", p }
                }
                if let Some(p) = param(&cur, "tone") {
                    PKnob { block_id: cur_id.clone(), name: "tone", label: "Tone", p }
                }
                if let Some(p) = param(&cur, "damping") {
                    PKnob { block_id: cur_id.clone(), name: "damping", label: "Damp", p }
                }
                if let Some(p) = param(&cur, "modulation") {
                    PKnob { block_id: cur_id.clone(), name: "modulation", label: "Mod", p }
                }
                div { class: "ml-auto",
                    AlgoPicker {
                        block_id: cur_id.clone(),
                        name: "algorithm",
                        value: param_v(&cur, "algorithm", 1.0),
                        options: VERB_ALGOS.to_vec(),
                        accent: VERB_COLORS[sel().min(1)].to_string(),
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
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let blocks = state.blocks.cloned();
    let in_db = state.in_peak_db.cloned();
    let out_db = state.out_peak_db.cloned();
    let (in_l, in_r, out_l, out_r) = state.stereo_db.cloned();
    let gr_db = state.comp_gr_db.cloned();
    let spectrum = state.spectrum.cloned();
    let comp_wave = state.comp_wave.cloned();

    let eq = find_block(&blocks, BlockType::Eq, "Amp EQ");
    let comp = find_block(&blocks, BlockType::Compressor, "Compressor");
    let gate = find_block(&blocks, BlockType::Gate, "Gate");

    let hp = model.headphone.clone();
    let _ = out_db;


    rsx! {
        div { class: "flex gap-0.5 h-full min-h-0 overflow-hidden",
            // ── Input meter rail ──
            div { class: "w-12 flex-shrink-0", StereoMeter { label: "In", l_db: in_l, r_db: in_r } }

            // ── Center surface ──
            div { class: "flex flex-col gap-1 flex-1 min-w-0 min-h-0",
                // Main modules, in signal order: Compressor → Gate → Amp EQ,
                // with the time section (Delay | Reverb) docked flush beneath.
                div { class: "flex flex-col gap-0 min-h-0",
                    div { class: "flex gap-0 min-h-0 w-full", style: "aspect-ratio: 25 / 9; max-height: 56%;",
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
                            ZoomPanel {
                                title: "Gate".to_string(),
                                zoomed_view: gate.clone().map(|g| rsx! {
                                    GatePanel { block: g, in_db, expanded: true }
                                }),
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
                    // Time section: stereo delay + stereo reverb + modulation.
                    div { class: "flex gap-0 min-h-0 w-full", style: "height: 168px;",
                        div { class: "min-h-0 h-full flex flex-col", style: "flex: 2 1 0%;",
                            ZoomPanel { title: "Delay".to_string(),
                                DelayPanel { blocks: blocks.clone(), tempo_bpm: model.tempo_bpm }
                            }
                        }
                        div { class: "min-h-0 h-full flex flex-col", style: "flex: 2 1 0%;",
                            ZoomPanel { title: "Reverb".to_string(),
                                ReverbPanel { blocks: blocks.clone() }
                            }
                        }
                        div { class: "min-h-0 h-full flex flex-col", style: "flex: 1 1 0%;",
                            ZoomPanel { title: "Modulation".to_string(),
                                ModViz { blocks: blocks.clone() }
                            }
                        }
                    }
                }

                div { class: "flex-1 min-h-0" }

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
                div { class: "flex-1 min-h-0 w-full flex justify-center gap-0",
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
                    StereoMeter {
                        label: "Out",
                        l_db: if hp.main_mute { -90.0 } else { out_l },
                        r_db: if hp.main_mute { -90.0 } else { out_r },
                        muted: hp.main_mute,
                    }
                }
                div { class: "flex-1 min-h-0 w-full flex justify-center gap-0",
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
                    StereoMeter {
                        label: "Phns",
                        l_db: out_l + 20.0 * hp.volume.max(0.001).log10(),
                        r_db: out_r + 20.0 * hp.volume.max(0.001).log10(),
                    }
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
