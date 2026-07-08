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

/// Approximate reverb tail (RT60, seconds) for a decay setting — the Hall
/// algorithm's feedback law (g = 0.5 + 0.48·d) over an ~80 ms loop.
fn decay_t60_secs(decay: f32) -> f64 {
    let g = (0.5 + 0.48 * decay.clamp(0.0, 1.0) as f64).min(0.995);
    0.08 * (0.001f64).ln() / g.ln()
}

fn decay_seconds_label(decay: f32) -> String {
    let t60 = decay_t60_secs(decay);
    if t60 >= 20.0 {
        "20s+".to_string()
    } else if t60 >= 10.0 {
        format!("{t60:.0}s")
    } else {
        format!("{t60:.1}s")
    }
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
    /// Bypass-all control shown beside the zoom icon: `Some(engaged)` +
    /// `on_power` renders the power button.
    #[props(default)]
    power_on: Option<bool>,
    #[props(default)]
    on_power: Option<Callback<()>>,
) -> Element {
    let mut zoomed = use_signal(|| false);
    rsx! {
        div { class: "relative flex flex-col flex-1 border border-border bg-card min-h-0 overflow-hidden",
            div { class: "flex-1 min-h-0", {children.clone()} }
            // Floating corner controls — power (bypass all) + zoom.
            div { class: "absolute top-1 right-1.5 z-20 flex items-center gap-1.5",
                if let (Some(on), Some(cb)) = (power_on, on_power) {
                    button {
                        class: "text-sm leading-none",
                        style: if on { "color: #4ade80;" } else { "color: #52525b;" },
                        title: if on { "Bypass all" } else { "Engage" },
                        onclick: move |_| cb.call(()),
                        "⏻"
                    }
                }
                button {
                    class: "text-muted-foreground/60 hover:text-foreground text-sm leading-none",
                    title: "{title}",
                    onclick: move |_| zoomed.set(true),
                    "⤢"
                }
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
            // Two thin bars — the pair reads as one meter's width.
            div { class: "flex flex-1 min-h-0 bg-black/60 border border-border overflow-hidden",
                style: "width: 17px;",
                div { class: "relative h-full", style: "width: 8px;",
                    div { class: "absolute inset-x-0 bottom-0 transition-[height] duration-75",
                        style: "height: {lp}%; background-color: {lc};" }
                }
                div { class: "w-px bg-black h-full" }
                div { class: "relative h-full", style: "width: 8px;",
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
/// `chorus::EngineType` order — the modulation algorithms.
const MOD_ENGINES: [&str; 5] = ["Cubic", "BBD", "Tape", "Orbit", "Juno"];
/// `TremMode` order.
const TREM_MODES: [&str; 3] = ["Mono", "Stereo", "Harmonic"];

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
    // (top_y, height) of the bar while dragging.
    let mut tracking = use_signal(|| None::<(f64, f64)>);

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
        move |frac: f32| {
            let (rig, id) = (rig.clone(), id.clone());
            spawn(async move {
                if let Some(r) = rig {
                    let _ = r
                        .set_block_param(id, "threshold".into(), frac.clamp(0.0, 1.0) * 90.0 - 90.0)
                        .await;
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
                        let y = e.client_coordinates().y;
                        let el = el();
                        let set_thr = set_thr.clone();
                        spawn(async move {
                            let Some(el) = el else { return };
                            let Ok(rect) = el.get_client_rect().await else { return };
                            let (top, h) = (rect.origin.y, rect.height());
                            tracking.set(Some((top, h)));
                            set_thr((1.0 - (y - top) / h) as f32);
                        });
                    }
                },
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
            // Drag shield — threshold keeps following outside the bar.
            if let Some((top, h)) = tracking() {
                div {
                    class: "fixed inset-0",
                    style: "z-index: 1000; cursor: ns-resize;",
                    onpointermove: {
                        let set_thr = set_thr.clone();
                        move |e: PointerEvent| {
                            set_thr((1.0 - (e.client_coordinates().y - top) / h) as f32);
                        }
                    },
                    onpointerup: move |_| tracking.set(None),
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

/// A slim vertical fader: drag to set, small readout below. While a drag
/// is live a fullscreen shield owns the pointer, so leaving the fader
/// never drops the gesture.
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
    // (top_y, height) of the bar, cached at pointer-down; None = idle.
    let mut tracking = use_signal(|| None::<(f64, f64)>);
    let pct = (value * 100.0).clamp(0.0, 100.0);
    rsx! {
        // Fixed column width = the bar itself; labels overflow either side
        // without pushing the neighbouring meter away.
        div { class: "flex flex-col items-center h-full min-h-0 min-w-0 flex-shrink-0", style: "width: 9px;",
            span { class: "text-[7px] font-semibold uppercase text-muted-foreground whitespace-nowrap", "{label}" }
            div {
                class: "relative flex-1 w-2 bg-black/60 border border-border min-h-0 cursor-ns-resize touch-none",
                onmounted: move |e| el.set(Some(e.data())),
                onpointerdown: move |e: PointerEvent| {
                    let y = e.client_coordinates().y;
                    let el = el();
                    spawn(async move {
                        let Some(el) = el else { return };
                        let Ok(rect) = el.get_client_rect().await else { return };
                        let (top, h) = (rect.origin.y, rect.height());
                        tracking.set(Some((top, h)));
                        on_change.call((1.0 - (y - top) / h).clamp(0.0, 1.0) as f32);
                    });
                },
                div {
                    class: "absolute inset-x-0 h-2 border border-zinc-500",
                    style: "bottom: calc({pct}% - 4px); background-color: #3f3f46;",
                }
            }
            if let Some((top, h)) = tracking() {
                div {
                    class: "fixed inset-0",
                    style: "z-index: 1000; cursor: ns-resize;",
                    onpointermove: move |e: PointerEvent| {
                        let y = e.client_coordinates().y;
                        on_change.call((1.0 - (y - top) / h).clamp(0.0, 1.0) as f32);
                    },
                    onpointerup: move |_| tracking.set(None),
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
            class: "relative flex items-center justify-center w-7 h-7 rounded-md border border-border text-muted-foreground hover:text-foreground",
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
fn PKnob(
    block_id: String,
    name: &'static str,
    label: &'static str,
    p: BlockParam,
    #[props(default)] fmt: Option<fn(f32) -> String>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    rsx! {
        crate::knob::Knob {
            label: label.to_string(),
            value: p.value,
            min: p.min,
            max: p.max,
            size: crate::knob::KnobSize::Small,
            fmt,
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
    let rig = use_hook(try_consume_context::<RigClient>);
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
                    let mix = param_v(b, "mix", 0.08).clamp(0.02, 1.0);
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
                            div { class: "absolute top-0.5 left-1.5 flex items-center gap-1.5",
                                button {
                                    style: if dim { "font-size:10px; line-height:1; color:#52525b;" } else { "font-size:10px; line-height:1; color:#4ade80;" },
                                    title: if dim { "Engage" } else { "Bypass" },
                                    onclick: {
                                        let rig = rig.clone();
                                        let id = b.id.clone();
                                        move |e: MouseEvent| {
                                            e.stop_propagation();
                                            if let Some(r) = rig.clone() {
                                                let id = id.clone();
                                                spawn(async move { let _ = r.toggle_block_bypass(id).await; });
                                            }
                                        }
                                    },
                                    "⏻"
                                }
                                span { style: "font-size:8px; font-weight:700; color:{color};", "{di + 1}" }
                                if dim {
                                    span { style: "font-size:8px; color:#52525b;", "bypassed" }
                                }
                            }
                            // Per-lane machine + timing, embedded at the
                            // lane's right edge.
                            div {
                                class: "absolute right-0 inset-y-0 flex items-center gap-1 pr-1 pl-3",
                                style: "background: linear-gradient(to left, rgba(8,8,8,0.95) 65%, transparent);",
                                onclick: move |e: MouseEvent| e.stop_propagation(),
                                // One timing division per delay (drives
                                // both sides).
                                {
                                    let rig = rig.clone();
                                    let id = b.id.clone();
                                    let div = param_v(b, "tap_div_l", 0.0);
                                    rsx! {
                                        select {
                                            class: "bg-transparent border border-border rounded-sm text-[10px] px-0.5 py-0",
                                            value: "{div as usize}",
                                            onchange: move |e: FormEvent| {
                                                if let Ok(v) = e.value().parse::<usize>() {
                                                    send_param(&rig, &id, "tap_div_l", v as f32);
                                                    send_param(&rig, &id, "tap_div_r", v as f32);
                                                }
                                            },
                                            for (i, o) in DIV_LABELS.iter().enumerate() {
                                                option { key: "{i}", value: "{i}", selected: i == div as usize, "{o}" }
                                            }
                                        }
                                    }
                                }
                                AlgoPicker {
                                    block_id: b.id.clone(),
                                    name: "style",
                                    value: param_v(b, "style", 1.0),
                                    options: DELAY_ALGOS.to_vec(),
                                    accent: color.to_string(),
                                }
                            }
                        }
                    }
                }
            }


            // ── Knobs for the selected delay (machine + timing live on
            // the lanes) ──
            div { class: "flex items-end justify-around gap-1.5 px-1.5 py-1 border-y border-border flex-shrink-0", style: "order: 1;",
                if let Some(p) = param(&cur, "high_pass") {
                    PKnob { block_id: cur_id.clone(), name: "high_pass", label: "HP", p }
                }
                if let Some(p) = param(&cur, "repeat_dyn") {
                    PKnob { block_id: cur_id.clone(), name: "repeat_dyn", label: "Duck", p }
                }
                if let Some(p) = param(&cur, "feedback") {
                    PKnob { block_id: cur_id.clone(), name: "feedback", label: "FB", p }
                }
                if let Some(p) = param(&cur, "pan") {
                    PKnob { block_id: cur_id.clone(), name: "pan", label: "Pan", p }
                }
                if let Some(p) = param(&cur, "mix") {
                    PKnob { block_id: cur_id.clone(), name: "mix", label: "Mix", p }
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
    let rig = use_hook(try_consume_context::<RigClient>);
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
                    let mix = param_v(b, "mix", 0.08).clamp(0.02, 1.0).max(0.15);
                    let md = param_v(b, "modulation", 0.2);
                    let color = VERB_COLORS[vi % 2];
                    let dim = b.bypassed;
                    let is_sel = sel() == vi;
                    // Real time axis (log, 0.1–20 s): the tail is the RT60
                    // estimate rendered in dB (straight to −60 at t60), the
                    // size opening the early bloom.
                    let t60 = decay_t60_secs(decay) * (0.6 + 0.8 * size as f64);
                    let x_of_t = |t: f64| -> f32 {
                        let (t_min, t_max) = (0.1f64, 20.0f64);
                        (4.0 + ((t.max(t_min) / t_min).ln() / (t_max / t_min).ln()).clamp(0.0, 1.0)
                            * (W as f64 - 8.0)) as f32
                    };
                    let mut top = String::from("M 4 28 ");
                    let mut bot = String::from("M 4 28 ");
                    for px in 0..=96 {
                        let frac = px as f64 / 96.0;
                        let t = 0.1 * (20.0f64 / 0.1).powf(frac);
                        let wig = 1.0 + ((t * 12.0) as f32).sin() * md * 0.18;
                        // dB-linear tail: 1 at t=0 → 0 at t60.
                        let a = (1.0 - t / t60).max(0.0) as f32;
                        let h = mix * a * wig * 26.0;
                        let x = x_of_t(t);
                        top.push_str(&format!("L {x:.1} {:.1} ", 28.0 - h));
                        bot.push_str(&format!("L {x:.1} {:.1} ", 28.0 + h));
                    }
                    top.push_str("L 456 28 Z");
                    bot.push_str("L 456 28 Z");
                    // Time markers along the tail scale.
                    let markers: Vec<(f32, &'static str)> = [
                        (0.5, ".5"), (1.0, "1"), (1.5, "1.5"), (2.0, "2"),
                        (4.0, "4"), (6.0, "6"), (8.0, "8"), (16.0, "16"),
                    ]
                    .iter()
                    .map(|(t, l)| (x_of_t(*t as f64), *l))
                    .collect();
                    rsx! {
                        div {
                            key: "lane{vi}",
                            class: if is_sel { "relative flex-1 min-h-0 cursor-pointer" } else { "relative flex-1 min-h-0 cursor-pointer opacity-60 hover:opacity-90" },
                            style: if is_sel { format!("order: {}; border-left: 2px solid {color}; background: {color}0a;", vi * 2) } else { format!("order: {}; border-left: 2px solid transparent;", vi * 2) },
                            onclick: move |_| sel.set(vi),
                            svg { class: "w-full h-full", view_box: "0 0 460 56", preserve_aspect_ratio: "none",
                                line { x1: "0", y1: "28", x2: "460", y2: "28", stroke: "#27272a", stroke_width: "1" }
                                // Time scale: seconds gridlines + labels.
                                for (mx, ml) in markers.iter() {
                                    line { x1: "{mx:.1}", y1: "4", x2: "{mx:.1}", y2: "52",
                                        stroke: "#ffffff", stroke_opacity: "0.06", stroke_width: "1" }
                                    text { x: "{mx + 1.5:.1}", y: "52", fill: "#52525b", font_size: "7",
                                        "{ml}" }
                                }
                                path { d: "{top}", fill: "{color}", fill_opacity: if dim { "0.08" } else { "0.25" },
                                    stroke: "{color}", stroke_opacity: if dim { "0.25" } else { "0.8" }, stroke_width: "1" }
                                path { d: "{bot}", fill: "{color}", fill_opacity: if dim { "0.06" } else { "0.18" },
                                    stroke: "{color}", stroke_opacity: if dim { "0.2" } else { "0.55" }, stroke_width: "1" }
                                // t60 tick: where the tail dies.
                                line { x1: "{x_of_t(t60):.1}", y1: "10", x2: "{x_of_t(t60):.1}", y2: "46",
                                    stroke: "{color}", stroke_opacity: if dim { "0.2" } else { "0.55" },
                                    stroke_width: "1", stroke_dasharray: "2,2" }
                            }
                            div { class: "absolute top-0.5 left-1.5 flex items-center gap-1.5",
                                button {
                                    style: if dim { "font-size:10px; line-height:1; color:#52525b;" } else { "font-size:10px; line-height:1; color:#4ade80;" },
                                    title: if dim { "Engage" } else { "Bypass" },
                                    onclick: {
                                        let rig = rig.clone();
                                        let id = b.id.clone();
                                        move |e: MouseEvent| {
                                            e.stop_propagation();
                                            if let Some(r) = rig.clone() {
                                                let id = id.clone();
                                                spawn(async move { let _ = r.toggle_block_bypass(id).await; });
                                            }
                                        }
                                    },
                                    "⏻"
                                }
                                span { style: "font-size:8px; font-weight:700; color:{color};", "{vi + 1}" }
                                if dim {
                                    span { style: "font-size:8px; color:#52525b;", "bypassed" }
                                }
                            }
                            // Per-lane algorithm + decay-time readout at the
                            // lane's right edge (the knob lives in the strip).
                            div {
                                class: "absolute right-0 inset-y-0 flex items-center gap-1.5 pr-1 pl-3",
                                style: "background: linear-gradient(to left, rgba(8,8,8,0.95) 65%, transparent);",
                                onclick: move |e: MouseEvent| e.stop_propagation(),
                                div { class: "flex flex-col items-end",
                                    span { style: "font-size:7px; text-transform:uppercase; color:#8a8a92;", "Time" }
                                    span { style: "font-family:ui-monospace,monospace; font-size:10px; color:{color};",
                                        {format!("{:.2}", param_v(b, "decay", 0.4))}
                                    }
                                }
                                AlgoPicker {
                                    block_id: b.id.clone(),
                                    name: "algorithm",
                                    value: param_v(b, "algorithm", 1.0),
                                    options: VERB_ALGOS.to_vec(),
                                    accent: color.to_string(),
                                }
                            }
                        }
                    }
                }
            }


            // ── Knobs for the selected reverb (algorithm lives on the lanes) ──
            div { class: "flex items-end justify-around gap-1.5 px-1.5 py-1 border-y border-border flex-shrink-0", style: "order: 1;",
                if let Some(p) = param(&cur, "mix") {
                    PKnob { block_id: cur_id.clone(), name: "mix", label: "Mix", p }
                }
                if let Some(p) = param(&cur, "decay") {
                    PKnob {
                        block_id: cur_id.clone(),
                        name: "decay",
                        label: "Time",
                        p,
                        // RT60 estimate from the Hall feedback law
                        // (g = 0.5 + 0.48·d, ~80 ms loop) — a readable tail
                        // length, not a lab measurement.
                        fmt: Some(decay_seconds_label as fn(f32) -> String),
                    }
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
                if let Some(p) = param(&cur, "pan_a") {
                    PKnob { block_id: cur_id.clone(), name: "pan_a", label: "Pan", p }
                }
            }

        }
    }
}

/// One modulation group (Modulation: chorus/phaser/flanger; Motion:
/// trem/vibrato/rotary): arrows rotate which member is engaged (rarely
/// more than one at a time), an LFO visualization of the active effect,
/// and two intelligently-mapped knobs — Mix and Speed.
#[component]
fn ModGroupPanel(
    title: &'static str,
    /// Member block types, in rotation order.
    kinds: Vec<BlockType>,
    blocks: Vec<LiveBlock>,
    tempo_bpm: u32,
    /// Speed as tempo divisions (Motion) instead of a Hz knob (Modulation).
    #[props(default)] tempo_divisions: bool,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let members: Vec<LiveBlock> = kinds
        .iter()
        .filter_map(|k| blocks.iter().find(|b| b.block_type == *k).cloned())
        .collect();
    if members.is_empty() {
        return rsx! { span { class: "text-xs text-muted-foreground italic p-2", "No {title} blocks." } };
    }
    let active_idx = members.iter().position(|b| !b.bypassed);
    let shown = active_idx.unwrap_or(0);
    let cur = members[shown].clone();
    let engaged = active_idx.is_some();

    // Rotate: engage the target member, bypass its siblings.
    let rotate = {
        let rig = rig.clone();
        let members = members.clone();
        move |dir: i32| {
            let n = members.len() as i32;
            let next = (((shown as i32 + dir) % n) + n) % n;
            let (rig, members) = (rig.clone(), members.clone());
            spawn(async move {
                let Some(r) = rig else { return };
                for (i, m) in members.iter().enumerate() {
                    let _ = r
                        .set_block_bypass(m.id.clone(), i != next as usize)
                        .await;
                }
            });
        }
    };

    // LFO viz of the shown member.
    let rate = param_v(&cur, "rate", 1.0);
    let depth = param_v(&cur, "depth", 0.5).clamp(0.1, 1.0);
    let mut d = String::new();
    for px in 0..=96 {
        let t = px as f32 / 96.0;
        // Two seconds of LFO at the actual rate.
        let y = 26.0 - (t * rate * 2.0 * std::f32::consts::TAU).sin() * depth * 18.0;
        d.push_str(if px == 0 { "M " } else { "L " });
        d.push_str(&format!("{:.1} {:.1} ", 4.0 + t * 192.0, y));
    }
    let color = if engaged { "#f472b6" } else { "#3f3f46" };

    // Motion speed: current rate expressed as the nearest tempo division.
    let quarter_hz = tempo_bpm.max(1) as f32 / 60.0;
    let div_hz: Vec<f32> = [1.0f32, 0.75, 0.5, 1.0 / 3.0, 0.25, 0.618, 0.414]
        .iter()
        .map(|beats| quarter_hz / beats)
        .collect();
    let cur_div = div_hz
        .iter()
        .enumerate()
        .min_by(|a, b| {
            (a.1 - rate).abs().partial_cmp(&(b.1 - rate).abs()).unwrap()
        })
        .map(|(i, _)| i)
        .unwrap_or(0);

    rsx! {
        div { class: "flex flex-col h-full min-h-0", style: "background: #080808;",
            // Header: arrows rotate the engaged member.
            div { class: "flex items-center gap-1 px-1.5 pt-1 flex-shrink-0",
                button {
                    style: if engaged { "font-size:10px; line-height:1; color:#4ade80;" } else { "font-size:10px; line-height:1; color:#52525b;" },
                    title: if engaged { "Bypass group" } else { "Engage" },
                    onclick: {
                        let rig = rig.clone();
                        let members = members.clone();
                        move |_| {
                            let (rig, members) = (rig.clone(), members.clone());
                            spawn(async move {
                                let Some(r) = rig else { return };
                                if engaged {
                                    for m in &members {
                                        let _ = r.set_block_bypass(m.id.clone(), true).await;
                                    }
                                } else {
                                    let _ = r.set_block_bypass(members[shown].id.clone(), false).await;
                                }
                            });
                        }
                    },
                    "⏻"
                }
                span { style: "font-size:8px; font-weight:600; text-transform:uppercase; color:#8a8a92;", "{title}" }
                button {
                    class: "ml-auto w-4 h-4 rounded-sm border border-border text-[9px] text-muted-foreground hover:text-foreground leading-none",
                    onclick: {
                        let rotate = rotate.clone();
                        move |_| rotate(-1)
                    },
                    "‹"
                }
                button {
                    class: if engaged {
                        "px-1.5 h-4 rounded-sm text-[9px] font-bold leading-none"
                    } else {
                        "px-1.5 h-4 rounded-sm text-[9px] text-muted-foreground border border-border leading-none"
                    },
                    style: if engaged { "background-color: #f472b6; color: #000;" } else { "" },
                    // Tap the name to engage/bypass the shown member.
                    onclick: {
                        let rig = rig.clone();
                        let id = cur.id.clone();
                        move |_| {
                            if let Some(r) = rig.clone() {
                                let id = id.clone();
                                spawn(async move { let _ = r.toggle_block_bypass(id).await; });
                            }
                        }
                    },
                    "{cur.name}"
                }
                button {
                    class: "w-4 h-4 rounded-sm border border-border text-[9px] text-muted-foreground hover:text-foreground leading-none",
                    onclick: move |_| rotate(1),
                    "›"
                }
                // Algorithm picker for the active member (chorus engines,
                // trem modes; passthroughs have none yet).
                match cur.block_type {
                    BlockType::Chorus | BlockType::Flanger | BlockType::Vibrato => rsx! {
                        AlgoPicker {
                            block_id: cur.id.clone(),
                            name: "engine",
                            value: param_v(&cur, "engine", 0.0),
                            options: MOD_ENGINES.to_vec(),
                            accent: "#f472b6".to_string(),
                        }
                    },
                    BlockType::Trem => rsx! {
                        AlgoPicker {
                            block_id: cur.id.clone(),
                            name: "mode",
                            value: param_v(&cur, "mode", 1.0),
                            options: TREM_MODES.to_vec(),
                            accent: "#f472b6".to_string(),
                        }
                    },
                    _ => rsx! {},
                }
            }
            // LFO trace.
            svg { class: "w-full flex-1 min-h-0", view_box: "0 0 200 52", preserve_aspect_ratio: "none",
                line { x1: "0", y1: "26", x2: "200", y2: "26", stroke: "#27272a", stroke_width: "1" }
                path { d: "{d}", fill: "none", stroke: "{color}", stroke_width: "1.5" }
            }
            // Mix + Speed.
            div { class: "flex items-end justify-around px-1 pb-0.5 flex-shrink-0 gap-1",
                if let Some(p) = param(&cur, "mix") {
                    PKnob { block_id: cur.id.clone(), name: "mix", label: "Mix", p }
                } else if let Some(p) = param(&cur, "depth") {
                    PKnob { block_id: cur.id.clone(), name: "depth", label: "Mix", p }
                }
                if tempo_divisions {
                    // Speed as a note division, mapped to Hz from the tempo.
                    div { class: "flex flex-col gap-0.5",
                        span { style: "font-size:8px; font-weight:600; text-transform:uppercase; color:#8a8a92;", "Speed" }
                        select {
                            class: "bg-transparent border border-border rounded-sm text-[10px] px-0.5 py-0",
                            value: "{cur_div}",
                            onchange: {
                                let rig = rig.clone();
                                let id = cur.id.clone();
                                move |e: FormEvent| {
                                    if let Ok(i) = e.value().parse::<usize>() {
                                        let hz = div_hz.get(i).copied().unwrap_or(2.0);
                                        if let Some(r) = rig.clone() {
                                            let id = id.clone();
                                            spawn(async move {
                                                let _ = r.set_block_param(id, "rate".into(), hz).await;
                                            });
                                        }
                                    }
                                }
                            },
                            for (i, l) in ["1/4", "1/8.", "1/8", "1/4T", "1/16", "Golden", "Silver"].iter().enumerate() {
                                option { key: "{i}", value: "{i}", selected: i == cur_div, "{l}" }
                            }
                        }
                    }
                } else if let Some(p) = param(&cur, "rate") {
                    PKnob {
                        block_id: cur.id.clone(),
                        name: "rate",
                        label: "Speed",
                        p,
                        fmt: Some((|v| format!("{v:.2}Hz")) as fn(f32) -> String),
                    }
                }
            }
        }
    }
}

// ── The drive board rail ──// ── The drive board rail ───────────────────────────────────────────────────

/// One drive-board chunk: the whole widget is a horizontal level fader —
/// the red gradient fills with how hard the block is pushed (default
/// center). Tap toggles the pedal; drag sets the level. Shows the block's
/// preset name and its engaged state.
#[component]
fn DriveChunk(
    /// Display name (the block preset).
    name: String,
    /// Level 0..1 (drive amount / how hard the amp is pushed).
    level: f32,
    engaged: bool,
    /// None → an empty slot (e.g. Amp R until dual-amp lands).
    #[props(default)] block_id: Option<String>,
    /// The wire param the bar writes.
    #[props(default = "drive")] param: &'static str,
    /// Map bar position 0..1 → param value.
    #[props(default = (0.0, 1.0))] range: (f32, f32),
    /// Amber accent for the amps instead of drive red.
    #[props(default)] amp_style: bool,
    /// The block preset's NAM options + current selection (quick switch).
    #[props(default)] options: Vec<String>,
    #[props(default)] option: u32,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    // (start_x, moved) while a pointer is down — a motionless release is a
    // tap (bypass toggle), movement is a level drag.
    let mut gesture = use_signal(|| None::<(f64, bool)>);

    let pct = (level * 100.0).clamp(0.0, 100.0);
    let (c_hi, c_lo) = if amp_style {
        ("rgba(245,158,11,0.30)", "rgba(245,158,11,0.05)")
    } else {
        ("rgba(220,60,50,0.32)", "rgba(220,60,50,0.06)")
    };
    let empty = block_id.is_none();

    let set_level = {
        let rig = rig.clone();
        let block_id = block_id.clone();
        move |coords: dioxus::html::geometry::ElementPoint| {
            let el = el();
            let (rig, block_id) = (rig.clone(), block_id.clone());
            spawn(async move {
                let Some(el) = el else { return };
                let Ok(rect) = el.get_client_rect().await else { return };
                let frac = (coords.x / rect.width()).clamp(0.0, 1.0) as f32;
                if let (Some(r), Some(id)) = (rig, block_id) {
                    let v = range.0 + frac * (range.1 - range.0);
                    let _ = r.set_block_param(id, param.to_string(), v).await;
                }
            });
        }
    };

    rsx! {
        div {
            class: if empty {
                "relative flex-1 min-w-0 border border-dashed border-border/40 overflow-hidden select-none"
            } else {
                "relative flex-1 min-w-0 border border-border overflow-hidden cursor-ew-resize touch-none select-none"
            },
            style: "background: #0a0a0a;",
            onmounted: move |e| el.set(Some(e.data())),
            onpointerdown: move |e: PointerEvent| {
                if !empty {
                    gesture.set(Some((e.client_coordinates().x, false)));
                }
            },
            onpointermove: {
                let set_level = set_level.clone();
                move |e: PointerEvent| {
                    if let Some((x0, moved)) = gesture() {
                        let dx = (e.client_coordinates().x - x0).abs();
                        if moved || dx > 4.0 {
                            gesture.set(Some((x0, true)));
                            set_level(e.element_coordinates());
                        }
                    }
                }
            },
            onpointerup: {
                let rig = rig.clone();
                let block_id = block_id.clone();
                move |_| {
                    if let Some((_, moved)) = gesture() {
                        if !moved {
                            // A tap: toggle the pedal.
                            if let (Some(r), Some(id)) = (rig.clone(), block_id.clone()) {
                                spawn(async move { let _ = r.toggle_block_bypass(id).await; });
                            }
                        }
                    }
                    gesture.set(None);
                }
            },
            onpointerleave: move |_| gesture.set(None),

            // The level fill — a subtle gradient, more push = more fill.
            if !empty {
                div {
                    class: "absolute inset-y-0 left-0",
                    style: if engaged {
                        "width: {pct}%; background: linear-gradient(to right, {c_lo}, {c_hi});"
                    } else {
                        "width: {pct}%; background: linear-gradient(to right, rgba(120,120,125,0.05), rgba(120,120,125,0.14));"
                    },
                }
                // Center detent tick.
                div { class: "absolute top-0 bottom-0 w-px", style: "left: 50%; background: rgba(255,255,255,0.08);" }
            }

            div { class: "relative flex items-center gap-1.5 h-full px-2 pointer-events-none",
                span {
                    class: "w-1.5 h-1.5 rounded-full flex-shrink-0",
                    style: if empty {
                        "background-color: #27272a;"
                    } else if engaged {
                        if amp_style { "background-color: #f59e0b;" } else { "background-color: #ef4444;" }
                    } else {
                        "background-color: #3f3f46;"
                    },
                }
                span {
                    class: if engaged { "text-[10px] font-semibold truncate" } else { "text-[10px] truncate text-muted-foreground" },
                    "{name}"
                }
                // NAM option quick-switch (captures within the preset).
                if options.len() > 1 {
                    select {
                        class: "ml-auto pointer-events-auto bg-transparent border border-border/60 rounded-sm text-[8px] px-0 py-0 text-muted-foreground flex-shrink-0", style: "max-width: 58px;",
                        value: "{option}",
                        onpointerdown: move |e: PointerEvent| e.stop_propagation(),
                        onchange: {
                            let rig = rig.clone();
                            let block_id = block_id.clone();
                            move |e: FormEvent| {
                                if let (Some(r), Some(id), Ok(v)) =
                                    (rig.clone(), block_id.clone(), e.value().parse::<u32>())
                                {
                                    spawn(async move { let _ = r.set_block_option(id, v).await; });
                                }
                            }
                        },
                        for (i, o) in options.iter().enumerate() {
                            option { key: "{i}", value: "{i}", selected: i as u32 == option, "{o}" }
                        }
                    }
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
    // The drive board: Boost + the three drives, plus the amps.
    let board: Vec<LiveBlock> = blocks
        .iter()
        .filter(|b| matches!(b.block_type, BlockType::Boost | BlockType::Drive))
        .cloned()
        .collect();
    let amp_l = blocks.iter().find(|b| b.block_type == BlockType::Amp && b.name.eq_ignore_ascii_case("Amp L")).cloned();
    // The amp chunk shows the active patch's preset (the pool preset the
    // patch points at), not the raw block name.
    let amp_preset = model
        .stacks
        .iter()
        .find(|st| st.is_active)
        .map(|st| st.preset.clone())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "Amp L".to_string());
    let comp = find_block(&blocks, BlockType::Compressor, "Compressor");
    let gate = find_block(&blocks, BlockType::Gate, "Gate");

    let hp = model.headphone.clone();
    let _ = out_db;


    rsx! {
        div { class: "flex gap-0 h-full min-h-0 overflow-hidden",
            // ── Input meter rail ──
            div { class: "w-6 flex-shrink-0", StereoMeter { label: "In", l_db: in_l, r_db: in_r } }

            // ── Center surface ──
            div { class: "flex flex-col gap-1 flex-1 min-w-0 min-h-0",
                // Main modules, in signal order: Compressor → Gate → Amp EQ,
                // with the time section (Delay | Reverb) docked flush beneath.
                div { class: "flex flex-col gap-0 min-h-0",
                    // ── The drive board: 4 drives + 2 amps, one sliver each.
                    // The whole chunk is the drive-level fader. ──
                    div { class: "flex gap-0 flex-shrink-0", style: "height: 34px;",
                        for b in board.iter() {
                            DriveChunk {
                                key: "{b.id}",
                                name: if b.preset.is_empty() { b.name.clone() } else { b.preset.clone() },
                                level: b.params.iter().find(|p| p.name == "drive").map(|p| p.value).unwrap_or(0.5),
                                engaged: !b.bypassed,
                                block_id: Some(b.id.clone()),
                                options: b.options.clone(),
                                option: b.option,
                            }
                        }
                        if let Some(amp) = amp_l {
                            DriveChunk {
                                name: amp_preset.clone(),
                                // Constant-loudness drive: the bar pushes the
                                // capture harder while calibration holds the
                                // level; center = the capture at unity.
                                level: amp.params.iter().find(|p| p.name == "drive").map(|p| p.value).unwrap_or(0.5),
                                engaged: !amp.bypassed,
                                block_id: Some(amp.id.clone()),
                                amp_style: true,
                            }
                        }
                        DriveChunk {
                            name: "Amp R".to_string(),
                            level: 0.5,
                            engaged: false,
                            amp_style: true,
                        }
                    }
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
                    div { class: "flex gap-0 min-h-0 w-full", style: "flex: 1 1 0%; min-height: 150px;",
                        div { class: "min-h-0 h-full flex flex-col", style: "flex: 2 1 0%;",
                            ZoomPanel {
                                title: "Delay".to_string(),
                                power_on: Some(blocks.iter().any(|b| b.block_type == BlockType::Delay && !b.bypassed)),
                                on_power: Some(Callback::new({
                                    let rig = rig.clone();
                                    let blocks = blocks.clone();
                                    move |_: ()| {
                                        let ids: Vec<(String, bool)> = blocks
                                            .iter()
                                            .filter(|b| b.block_type == BlockType::Delay)
                                            .map(|b| (b.id.clone(), b.bypassed))
                                            .collect();
                                        let any_on = ids.iter().any(|(_, byp)| !byp);
                                        if let Some(r) = rig.clone() {
                                            spawn(async move {
                                                for (id, _) in ids {
                                                    let _ = r.set_block_bypass(id, any_on).await;
                                                }
                                            });
                                        }
                                    }
                                })),
                                DelayPanel { blocks: blocks.clone(), tempo_bpm: model.tempo_bpm }
                            }
                        }
                        div { class: "min-h-0 h-full flex flex-col", style: "flex: 2 1 0%;",
                            ZoomPanel {
                                title: "Reverb".to_string(),
                                power_on: Some(blocks.iter().any(|b| b.block_type == BlockType::Reverb && !b.bypassed)),
                                on_power: Some(Callback::new({
                                    let rig = rig.clone();
                                    let blocks = blocks.clone();
                                    move |_: ()| {
                                        let ids: Vec<(String, bool)> = blocks
                                            .iter()
                                            .filter(|b| b.block_type == BlockType::Reverb)
                                            .map(|b| (b.id.clone(), b.bypassed))
                                            .collect();
                                        let any_on = ids.iter().any(|(_, byp)| !byp);
                                        if let Some(r) = rig.clone() {
                                            spawn(async move {
                                                for (id, _) in ids {
                                                    let _ = r.set_block_bypass(id, any_on).await;
                                                }
                                            });
                                        }
                                    }
                                })),
                                ReverbPanel { blocks: blocks.clone() }
                            }
                        }
                        div { class: "min-h-0 h-full flex flex-col gap-0", style: "flex: 1 1 0%;",
                            ZoomPanel { title: "Modulation".to_string(),
                                ModGroupPanel {
                                    title: "Mod",
                                    kinds: vec![BlockType::Chorus, BlockType::Phaser, BlockType::Flanger],
                                    blocks: blocks.clone(),
                                    tempo_bpm: model.tempo_bpm,
                                }
                            }
                            ZoomPanel { title: "Motion".to_string(),
                                ModGroupPanel {
                                    title: "Motion",
                                    kinds: vec![BlockType::Trem, BlockType::Vibrato, BlockType::Rotary],
                                    blocks: blocks.clone(),
                                    tempo_bpm: model.tempo_bpm,
                                    tempo_divisions: true,
                                }
                            }
                        }
                    }
                }


            }

            // ── Output rail: mute on top, then FOH trim + out meter,
            // then the phones group — mix fader | phones meter | guitar
            // (self) fader.
            div {
                style: "width: 46px; flex-shrink: 0; display: flex; flex-direction: column; align-items: center; gap: 2px; min-height: 0;",
                button {
                    class: if hp.main_mute {
                        "w-9 rounded px-0.5 py-0.5 text-[8px] font-bold uppercase ring-2 ring-red-500"
                    } else {
                        "w-9 rounded px-0.5 py-0.5 text-[8px] font-bold uppercase border border-border text-muted-foreground hover:text-foreground"
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
                div { style: "flex: 1 1 0%; min-height: 0; width: 100%; display: flex; justify-content: center;",
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
                div { style: "flex: 1 1 0%; min-height: 0; width: 100%; display: flex; justify-content: center;",
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
