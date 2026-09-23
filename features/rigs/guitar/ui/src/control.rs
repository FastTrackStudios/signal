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

use crate::param_writer::WriteParam;
use std::fmt::Write;
use std::time::Duration;

use dioxus::prelude::*;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{BlockParam, LiveBlock, PerformanceModel};
use signal_proto::block::BlockType;
use signal_widgets::{Picker, PickerSize};

use crate::state::RigViewState;

/// A quiet placeholder for a block that isn't in the active patch's chain.
/// Keeps the panel's footprint (so the layout doesn't jump) and reads as an
/// intentional empty slot — a dashed outline + dimmed label — rather than an
/// error message.
fn empty_slot(label: &str) -> Element {
    rsx! {
        div {
            class: "flex-1 min-h-0 w-full flex flex-col items-center justify-center gap-1.5 opacity-40 select-none",
            div { class: "w-6 h-6 rounded-md border border-dashed border-zinc-600" }
            span { class: "text-[9px] font-semibold uppercase tracking-[0.14em] text-zinc-500", "{label}" }
        }
    }
}

/// The one way a bypassed visualizer says so: a quiet grey BYPASSED badge. `small` for a single lane inside a grouped panel; the full size is
/// drawn by [`ZoomPanel`] over a whole panel whose block is off.
#[component]
fn BypassedBadge(#[props(default)] small: bool) -> Element {
    let style = if small {
        "font-size: 7px; letter-spacing: 0.12em; padding: 1px 4px; border-radius: 3px;"
    } else {
        "font-size: 10px; letter-spacing: 0.22em; padding: 4px 10px; border-radius: 4px;"
    };
    rsx! {
        span {
            style: "{style} font-weight: 600; text-transform: uppercase; \
                    color: #8a8a92; border: 1px solid rgba(255,255,255,0.10); \
                    background: rgba(10,10,12,0.7); white-space: nowrap;",
            "Bypassed"
        }
    }
}

/// Every member of a grouped panel (Mod, Motion) is bypassed — the group is
/// off. False for a group with no members: that is an empty slot, not a
/// bypass.
fn group_bypassed(blocks: &[LiveBlock], kinds: &[BlockType], pre: bool) -> bool {
    let mut members = blocks
        .iter()
        .filter(|b| kinds.contains(&b.block_type) && is_pre_fx(b) == pre)
        .peekable();
    members.peek().is_some() && members.all(|b| b.bypassed)
}

/// Find a chain block by (type, name).
fn find_block(blocks: &[LiveBlock], bt: BlockType, name: &str) -> Option<LiveBlock> {
    blocks
        .iter()
        .find(|b| b.block_type == bt && b.name.eq_ignore_ascii_case(name))
        .cloned()
}

/// A block of the Pre FX module (in front of the amp). The Time, Mod and
/// Motion panels are the end of the chain, so they skip these — a spring
/// reverb before the amp is not "the reverb".
fn is_pre_fx(b: &LiveBlock) -> bool {
    b.name.starts_with("Pre ") && !b.name.eq_ignore_ascii_case("Pre Comp")
}

fn param(block: &LiveBlock, name: &str) -> Option<BlockParam> {
    block.params.iter().find(|p| p.name == name).cloned()
}

fn param_v(block: &LiveBlock, name: &str, dflt: f32) -> f32 {
    param(block, name).map_or(dflt, |p| p.value)
}

/// Approximate reverb tail (RT60, seconds) for a decay setting — the Hall
/// algorithm's feedback law (g = 0.5 + 0.48·d) over an ~80 ms loop.
fn decay_t60_secs(decay: f32) -> f64 {
    let g = 0.48f64
        .mul_add(f64::from(decay.clamp(0.0, 1.0)), 0.5)
        .min(0.995);
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
            let _ = r.write_param(id, name, value).await;
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
    /// Bypass-all control shown beside the zoom icon (top-right): `Some(engaged)`
    /// + `on_power` renders the power button. Used by the grouped panels
    /// (Delay/Reverb), which draw their own per-member power button at the
    /// left inside `children`, so a second one here would be redundant.
    #[props(default)]
    power_on: Option<bool>,
    #[props(default)] on_power: Option<Callback<()>>,
    /// A single block's own bypass, shown at the top-LEFT instead — for a
    /// panel that is one block and has no internal header of its own to put
    /// it in (Compressor, Gate, Amp EQ). `Some(engaged)` + `on_left_power`
    /// renders it; omit both for a panel with nothing to bypass as a whole.
    #[props(default)]
    left_power_on: Option<bool>,
    #[props(default)] on_left_power: Option<Callback<()>>,
    /// The panel's block is off, for a panel with no power control of its
    /// own here (Mod, Motion). A panel with `power_on`/`left_power_on` is
    /// bypassed exactly when that reads off.
    #[props(default)]
    bypassed: bool,
    /// The module this panel belongs to: clicking the panel shows that
    /// module's presets in the right sidebar.
    #[props(default)]
    module: Option<&'static str>,
) -> Element {
    let mut zoomed = use_signal(|| false);
    let select = try_use_context::<crate::module_sidebar::SelectedModule>();
    // One bypass look for every visualizer: the content dimmed (still
    // editable) under a BYPASSED badge that lets clicks through.
    let off = bypassed || power_on == Some(false) || left_power_on == Some(false);
    let dim = if off { "opacity: 0.3;" } else { "" };
    rsx! {
        div { class: "relative flex flex-col flex-1 border border-border bg-card min-h-0 overflow-hidden",
            onclick: move |_| {
                if let (Some(m), Some(sel)) = (module, select) {
                    sel.set(crate::module_sidebar::Selection::Module(m.to_string()));
                }
            },
            div { class: "flex-1 min-h-0", style: "{dim}", {children.clone()} }
            if off {
                div {
                    class: "absolute inset-0 flex items-center justify-center",
                    style: "pointer-events: none;",
                    BypassedBadge {}
                }
            }
            if let (Some(on), Some(cb)) = (left_power_on, on_left_power) {
                div { class: "absolute top-1 left-1.5 flex items-center gap-1.5",
                    button {
                        class: "text-sm leading-none",
                        style: if on { "color: #4ade80;" } else { "color: #52525b;" },
                        title: if on { "Bypass" } else { "Engage" },
                        onclick: move |_| cb.call(()),
                        fts_chrome::Glyph { icon: fts_chrome::Icon::Power, size: 12 }
                    }
                }
            }
            // Floating corner controls — power (bypass all) + zoom.
            div { class: "absolute top-1 right-1.5 flex items-center gap-1.5",
                if let (Some(on), Some(cb)) = (power_on, on_power) {
                    button {
                        class: "text-sm leading-none",
                        style: if on { "color: #4ade80;" } else { "color: #52525b;" },
                        title: if on { "Bypass all" } else { "Engage" },
                        onclick: move |_| cb.call(()),
                        fts_chrome::Glyph { icon: fts_chrome::Icon::Power, size: 12 }
                    }
                }
                button {
                    class: "text-muted-foreground/60 hover:text-foreground text-sm leading-none",
                    title: "{title}",
                    onclick: move |_| zoomed.set(true),
                    fts_chrome::Glyph { icon: fts_chrome::Icon::Expand, size: 12 }
                }
            }
        }
        if zoomed() {
            div { class: "fixed inset-0 z-50 flex flex-col bg-black/95 p-6",
                button {
                    class: "absolute top-3 right-4 z-10 text-muted-foreground hover:text-foreground text-xl",
                    onclick: move |_| zoomed.set(false),
                    fts_chrome::Glyph { icon: fts_chrome::Icon::Close, size: 16 }
                }
                div { class: "relative flex-1 min-h-0",
                    div { class: "h-full", style: "{dim}", {zoomed_view.unwrap_or(children)} }
                    if off {
                        div {
                            class: "absolute inset-0 flex items-center justify-center",
                            style: "pointer-events: none;",
                            BypassedBadge {}
                        }
                    }
                }
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
                // A parameter the active patch has moved away from what the
                // chain builds. Every knob move is recorded as an override
                // the instant it happens, so without this a player cannot
                // tell their own changes from what came with the preset —
                // and clicking the dot is the only way back.
                if p.overridden {
                    button {
                        class: "shrink-0 leading-none text-[10px] text-accent-foreground/90 hover:text-foreground",
                        title: "Changed from the preset — click to put {p.name} back",
                        onclick: {
                            let (rig, block_id, name) = (rig.clone(), block_id.clone(), p.name.clone());
                            move |_| {
                                let Some(rig) = rig.clone() else { return };
                                let (block_id, name) = (block_id.clone(), name.clone());
                                spawn(async move {
                                    let _ = rig.clear_block_param(block_id, name).await;
                                });
                            }
                        },
                        "●"
                    }
                }
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
fn StereoMeter(
    label: &'static str,
    l_db: f32,
    r_db: f32,
    #[props(default)] muted: bool,
) -> Element {
    let bar = |db: f32| -> (f32, &'static str) {
        let pct = ((db + 60.0) / 60.0 * 100.0).clamp(0.0, 100.0);
        let color = if muted {
            "#3f3f46"
        // Red means near clipping, not "loud": at −6 it lit on every hard
        // strum of a patch peaking with 6 dB of clean headroom left.
        } else if db > -3.0 {
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
            span { class: "text-[6px] font-semibold uppercase text-muted-foreground whitespace-nowrap", style: "letter-spacing: 0.2px;", "{label}" }
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
            span { class: "text-[6px] font-mono text-muted-foreground",
                if max_db <= -89.0 { "−∞" } else { {format!("{max_db:.0}")} }
            }
        }
    }
}

// ── Time-section constants ──────────────────────────────────────────────────

// Deep blue, not sky: the delay lanes sit a row below the modulation lane,
// which is cyan-led, and `#38bdf8` was close enough to it that the two read
// as the same family of thing. Blue and indigo are far enough from cyan to
// be told apart at a glance and from each other up close.
const DELAY_COLORS: [&str; 2] = ["#3b82f6", "#6366f1"];
/// Reverb is purple-led, the way delay is blue-led: the two time effects sit
/// side by side and the colour is how you tell which lane you are reading
/// without going to the label.
const VERB_COLORS: [&str; 2] = ["#a78bfa", "#c084fc"];

/// Tempo-division labels — `delay::TapDivision` order (Quarter, dotted 8th,
/// 8th, triplet, 16th, golden ratio, silver ratio, free-running).
const DIV_LABELS: [&str; 8] = [
    "1/4", "1/8.", "1/8", "1/4T", "1/16", "Golden", "Silver", "Free",
];

/// What a delay block's left tap is locked to ("1/4"), or empty when it runs
/// free on its own time knob.
fn div_label(b: &LiveBlock) -> String {
    let idx = param_v(b, "tap_div_l", 0.0) as usize;
    if div_factor(idx as f32) <= 0.0 {
        return String::new();
    }
    DIV_LABELS.get(idx).copied().unwrap_or("").to_string()
}

/// Division → multiple of a quarter note, for the tap visualization
/// (Free returns 0 → the caller falls back to the block's `time`).
fn div_factor(idx: f32) -> f32 {
    [1.0, 0.75, 0.5, 1.0 / 3.0, 0.25, 0.618, 0.414, 0.0][(idx as usize).min(7)]
}

/// `delay::DelayStyle` order — the `TimeLine` MX machines.
/// `chorus::EngineType` order — the modulation algorithms.
const MOD_ENGINES: [&str; 5] = ["Cubic", "BBD", "Tape", "Orbit", "Juno"];
/// `TremMode` order.
const TREM_MODES: [&str; 3] = ["Mono", "Stereo", "Harmonic"];

const DELAY_ALGOS: [&str; 13] = [
    "Tape", "Digital", "dBucket", "Lo-Fi", "Shimmer", "Reverse", "Ice", "Rhythm", "Drum",
    "Oil Can", "MultiTap", "Spectral", "Filter",
];
/// `reverb::AlgorithmType::ALL` order.
const VERB_ALGOS: [&str; 15] = [
    "Room",
    "Hall",
    "Plate",
    "Spring",
    "Cloud",
    "Bloom",
    "Shimmer",
    "Chorale",
    "Magneto",
    "NonLinear",
    "Swell",
    "Reflections",
    "Velvet",
    "FreeVerb",
    "Convolution",
];

/// The gate, tall and slim: the level bar with a draggable threshold and a
/// live gain-reduction strip (red, from the top) so gating is visible the
/// moment it happens. `expanded` (the zoomed view) adds attack/release.
#[component]
fn GatePanel(block: LiveBlock, in_db: f32, #[props(default)] expanded: bool) -> Element {
    // Callbacks made once per site, not once per render (see `stable`).
    let cbs = crate::stable::use_stable();
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
                        .write_param(
                            id,
                            "threshold".into(),
                            frac.clamp(0.0, 1.0).mul_add(90.0, -90.0),
                        )
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
                    let set_thr = set_thr;
                    move |e: PointerEvent| {
                        let y = e.client_coordinates().y;
                        let el = el();
                        let set_thr = set_thr.clone();
                        let bus = signal_widgets::DragBus::try_use();
                        spawn(async move {
                            let Some(el) = el else { return };
                            let Ok(rect) = el.get_client_rect().await else { return };
                            let (top, h) = (rect.origin.y, rect.height());
                            set_thr((1.0 - (y - top) / h) as f32);
                            // Follow the drag across the whole window (the
                            // app root forwards it); the local shield below
                            // only covers this panel.
                            match bus {
                                Some(bus) => {
                                    let set_thr = set_thr.clone();
                                    bus.begin(move |ev| {
                                        if let signal_widgets::DragEvent::Move { y, .. } = ev {
                                            set_thr((1.0 - (y - top) / h) as f32);
                                        }
                                    });
                                }
                                None => tracking.set(Some((top, h))),
                            }
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
                                        on_change: cbs.keyed(usize::from(name == "release"), move |v: f32| {
                                            if let Some(r) = rig.clone() {
                                                let (id, pname) = (id.clone(), pname.clone());
                                                spawn(async move { let _ = r.write_param(id, pname, v).await; });
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
        div { class: "flex flex-col items-center h-full min-h-0 min-w-0 flex-shrink-0", style: "width: 15px;",
            // Top lane stays empty (meter labels own it) — fader labels
            // stack under the bar with the readout.
            span { class: "text-[6px]", style: "visibility: hidden;", "·" }
            div {
                class: "relative flex-1 w-2 bg-black/60 border border-border min-h-0 cursor-ns-resize touch-none",
                onmounted: move |e| el.set(Some(e.data())),
                onpointerdown: move |e: PointerEvent| {
                    let y = e.client_coordinates().y;
                    let el = el();
                    let bus = signal_widgets::DragBus::try_use();
                    spawn(async move {
                        let Some(el) = el else { return };
                        let Ok(rect) = el.get_client_rect().await else { return };
                        let (top, h) = (rect.origin.y, rect.height());
                        on_change.call((1.0 - (y - top) / h).clamp(0.0, 1.0) as f32);
                        match bus {
                            Some(bus) => bus.begin(move |ev| {
                                if let signal_widgets::DragEvent::Move { y, .. } = ev {
                                    on_change.call((1.0 - (y - top) / h).clamp(0.0, 1.0) as f32);
                                }
                            }),
                            None => tracking.set(Some((top, h))),
                        }
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
            span { class: "text-[6px] font-semibold uppercase text-muted-foreground whitespace-nowrap", style: "letter-spacing: 0.2px;", "{label}" }
            span { class: "text-[6px] font-mono text-muted-foreground whitespace-nowrap", "{readout}" }
        }
    }
}

/// MIDI monitor behind a header icon — system-wide, out of the surface.
/// Shows a dot when events have been seen; click for the full log.
#[component]
pub fn MidiIndicator(
    /// Open the audio & MIDI settings (the rig's device dialog).
    on_settings: Callback<()>,
) -> Element {
    // Callbacks made once per site, not once per render (see `stable`).
    let cbs = crate::stable::use_stable();
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut log = use_signal(Vec::<String>::new);
    // Lit while events are arriving: the log changed on a recent poll.
    let mut active = use_signal(|| false);
    let mut monitor = use_signal(|| false);
    {
        let rig = rig;
        use_future(move || {
            let rig = rig.clone();
            async move {
                let Some(rig) = rig else { return };
                let mut quiet = 0u32;
                loop {
                    if let Ok(l) = rig.midi_recent().await {
                        let changed = *log.peek() != l;
                        if changed {
                            log.set(l);
                            quiet = 0;
                        } else {
                            quiet += 1;
                        }
                        // Stay lit ~1.2 s past the last event.
                        let lit = quiet < 3;
                        if *active.peek() != lit {
                            active.set(lit);
                        }
                    }
                    architect::platform::sleep(Duration::from_millis(400)).await;
                }
            }
        });
    }
    let entries = log();
    let dot = if active() {
        "#34d399"
    } else if entries.is_empty() {
        "#3f3f46"
    } else {
        "#166534"
    };
    let items = vec![
        crate::indicators::IndicatorItem::new(
            if monitor() {
                "Hide MIDI monitor"
            } else {
                "MIDI monitor"
            },
            cbs.cb(move |()| monitor.toggle()),
        ),
        crate::indicators::IndicatorItem::new("Audio & MIDI settings…", on_settings),
    ];
    rsx! {
        crate::indicators::Indicator {
            label: "MIDI".to_string(),
            dot: dot.to_string(),
            title: if active() { "MIDI — receiving".to_string() } else { "MIDI".to_string() },
            items,
            pinned: monitor(),
            on_close: move |()| monitor.set(false),
            extra: rsx! {
                if monitor() {
                    div {
                        style: "margin-top: 4px; padding: 6px 8px; width: 320px; height: 220px; \
                                border-top: 1px solid #1c1c21; font-family: monospace; font-size: 10px; \
                                color: #a1a1aa; overflow-y: scroll; display: flex; flex-direction: column; gap: 2px;",
                        if entries.is_empty() {
                            span { style: "font-style: italic;", "listening — no MIDI events yet" }
                        }
                        for (i, e) in entries.iter().enumerate().rev() {
                            div { key: "{i}", style: "white-space: nowrap; overflow: hidden;", "{e}" }
                        }
                    }
                }
            },
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
    // The grid is drawn by the app root when it can be: inside the panel it
    // was clipped by the panel's edge, most of the algorithms out of sight.
    let host = signal_widgets::PopupHost::try_use();
    let current = options
        .get(value as usize)
        .copied()
        .unwrap_or(options.first().copied().unwrap_or("—"));
    rsx! {
        // Anchored under the name, not a full-screen overlay: Blitz has no
        // `position: fixed`. Closes when the pointer leaves the pair.
        div {
            style: "position: relative;",
            onmouseleave: move |_| {
                if host.is_none() {
                    open.set(false);
                }
            },
        button {
            class: "flex items-center gap-1 rounded-sm border border-border px-1.5 py-0.5 hover:bg-accent/30",
            onclick: {
                let rig = rig.clone();
                let block_id = block_id.clone();
                let options = options.clone();
                let accent = accent.clone();
                move |e: MouseEvent| {
                    let Some(h) = host else {
                        open.toggle();
                        return;
                    };
                    if open() {
                        h.close();
                        return;
                    }
                    // The button's top-left in the window, from the click
                    // itself — synchronous, nothing stored to go stale.
                    let (c, el) = (e.client_coordinates(), e.element_coordinates());
                    let (x, y) = (c.x - el.x, c.y - el.y);
                    let (rig, block_id, options, accent) =
                        (rig.clone(), block_id.clone(), options.clone(), accent.clone());
                    open.set(true);
                    h.open(
                        x,
                        y + 26.0,
                        260.0,
                        move || algo_grid(&options, value as usize, &accent, {
                            let (rig, block_id) = (rig.clone(), block_id.clone());
                            move |i| {
                                send_param(&rig, &block_id, name, i as f32);
                                // After the click is done with the button.
                                spawn(async move { h.close() });
                            }
                        }),
                        move || {
                            let mut o = open;
                            o.set(false);
                        },
                    );
                }
            },
            span {
                class: "text-[11px] font-bold tracking-wide",
                style: "color: {accent};",
                "{current}"
            }
            span { class: "text-muted-foreground",
                fts_chrome::Glyph { icon: fts_chrome::Icon::ChevronDown, size: 10 }
            }
        }
        if open() && host.is_none() {
            div {
                style: "position: absolute; top: 100%; left: 0; z-index: 60; padding-top: 4px;",
                div {
                    class: "grid grid-cols-3 gap-1 p-2 rounded-lg border border-border bg-card",
                    style: "width: 260px; box-shadow: 0 12px 32px #000c;",
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
}

/// The algorithm grid, as the popup host draws it.
fn algo_grid(
    options: &[&'static str],
    current: usize,
    accent: &str,
    pick: impl Fn(usize) + Clone + 'static,
) -> Element {
    rsx! {
        div {
            class: "grid grid-cols-3 gap-1 p-2 rounded-lg border border-border bg-card",
            style: "width: 260px; box-shadow: 0 12px 32px #000c;",
            for (i, o) in options.iter().enumerate() {
                button {
                    key: "{i}",
                    class: if i == current { "rounded px-3 py-2 text-xs font-bold" } else { "rounded px-3 py-2 text-xs text-muted-foreground border border-border hover:bg-accent/40" },
                    style: if i == current { format!("background-color: {accent}; color: #000;") } else { String::new() },
                    onclick: {
                        let pick = pick.clone();
                        move |_| pick(i)
                    },
                    "{o}"
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
            Picker {
                options: options.iter().map(|o| (*o).to_string()).collect::<Vec<String>>(),
                selected: value as u32,
                size: PickerSize::Tiny,
                on_select: move |v: u32| send_param(&rig, &block_id, name, v as f32),
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
    #[props(default)] fmt: Option<crate::knob::FmtFn>,
    /// Strip-embedded: tiny body, no numeric readout.
    #[props(default)]
    tiny: bool,
) -> Element {
    // Callbacks made once per site, not once per render (see `stable`).
    let cbs = crate::stable::use_stable();
    let rig = use_hook(try_consume_context::<RigClient>);
    rsx! {
        crate::knob::Knob {
            label: label.to_string(),
            value: p.value,
            min: p.min,
            max: p.max,
            hide_value: tiny,
            size: if tiny { crate::knob::KnobSize::Tiny } else { crate::knob::KnobSize::Small },
            fmt,
            on_change: cbs.cb(move |v: f32| {
                if let Some(r) = rig.clone() {
                    let (id, name) = (block_id.clone(), name.to_string());
                    spawn(async move { let _ = r.write_param(id, name, v).await; });
                }
            }),
        }
    }
}

/// The stereo delay module, wide: one full-width visualizer per delay
/// stacked (1 top, 2 bottom) — click a lane to select it — with the
/// selected delay's controls in a strip beneath.
#[component]
fn DelayPanel(blocks: Vec<LiveBlock>, tempo_bpm: u32, #[props(default)] pre: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let pick_block = try_use_context::<crate::module_sidebar::SelectedModule>();
    let mut sel = use_signal(|| 0usize);
    const W: f32 = 460.0;
    let delays: Vec<LiveBlock> = blocks
        .iter()
        .filter(|b| b.block_type == BlockType::Delay && is_pre_fx(b) == pre)
        .cloned()
        .collect();
    if delays.is_empty() {
        return rsx! { {empty_slot("Delay")} };
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
                            onclick: {
                                let name = b.name.clone();
                                move |e: MouseEvent| {
                                    sel.set(di);
                                    // This delay's own presets, not the Time module's.
                                    e.stop_propagation();
                                    if let Some(s) = pick_block {
                                        s.set(crate::module_sidebar::Selection::Block {
                                            name: name.clone(),
                                            block_type: "delay".into(),
                                        });
                                    }
                                }
                            },
                            {delay_lane(taps.clone(), win_ms, !dim, color, W, quarter, div_label(b),
                                param_v(b, "style", 1.0) as u32)}
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
                                    fts_chrome::Glyph { icon: fts_chrome::Icon::Power, size: 12 }
                                }
                                span { style: "font-size:8px; font-weight:700; color:{color};", "{di + 1}" }
                                if dim {
                                    BypassedBadge { small: true }
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
                                        Picker {
                                            options: DIV_LABELS.iter().map(|o| (*o).to_string()).collect::<Vec<String>>(),
                                            selected: div as u32,
                                            size: PickerSize::Tiny,
                                            on_select: move |v: u32| {
                                                send_param(&rig, &id, "tap_div_l", v as f32);
                                                send_param(&rig, &id, "tap_div_r", v as f32);
                                            },
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
            div { class: "flex items-end justify-around gap-1.5 px-1.5 py-1 border-y border-border flex-shrink-0",
                style: if cur.bypassed { "order: 1; opacity: 0.4;" } else { "order: 1;" },
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
                // The delay stage splits three ways — Delay 1, Dry, Delay 2
                // (`time_stage`): Mix is this delay's share, Dry the guitar's,
                // and the Dry lives on the stage's first delay whichever lane
                // is selected.
                if let Some(tap) = delays.first() {
                    if let Some(p) = param(tap, "dry") {
                        PKnob { block_id: tap.id.clone(), name: "dry", label: "Dry", p }
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
fn ReverbPanel(blocks: Vec<LiveBlock>, tempo_bpm: u32, #[props(default)] pre: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let pick_block = try_use_context::<crate::module_sidebar::SelectedModule>();
    let mut sel = use_signal(|| 0usize);
    const W: f32 = 460.0;
    let verbs: Vec<LiveBlock> = blocks
        .iter()
        .filter(|b| b.block_type == BlockType::Reverb && is_pre_fx(b) == pre)
        .cloned()
        .collect();
    if verbs.is_empty() {
        return rsx! { {empty_slot("Reverb")} };
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
                    let t60 = decay_t60_secs(decay) * 0.8f64.mul_add(f64::from(size), 0.6);
                    let x_of_t = |t: f64| -> f32 {
                        let (t_min, t_max) = (0.1f64, 20.0f64);
                        (t.max(t_min) / t_min).log(t_max / t_min).clamp(0.0, 1.0).mul_add(f64::from(W) - 8.0, 4.0) as f32
                    };
                    let mut top = String::from("M 4 28 ");
                    let mut bot = String::from("M 4 28 ");
                    for px in 0..=96 {
                        let frac = f64::from(px) / 96.0;
                        let t = 0.1 * (20.0f64 / 0.1).powf(frac);
                        let wig = (((t * 12.0) as f32).sin() * md).mul_add(0.18, 1.0);
                        // dB-linear tail: 1 at t=0 → 0 at t60.
                        let a = (1.0 - t / t60).max(0.0) as f32;
                        let h = mix * a * wig * 26.0;
                        let x = x_of_t(t);
                        let _ = write!(top, "L {x:.1} {:.1} ", 28.0 - h);
                        let _ = write!(bot, "L {x:.1} {:.1} ", 28.0 + h);
                    }
                    top.push_str("L 456 28 Z");
                    bot.push_str("L 456 28 Z");
                    // Time markers along the tail scale.
                    let markers: Vec<(f32, &'static str)> = [
                        (0.5, ".5"), (1.0, "1"), (1.5, "1.5"), (2.0, "2"),
                        (4.0, "4"), (6.0, "6"), (8.0, "8"), (16.0, "16"),
                    ]
                    .iter()
                    .map(|(t, l)| (x_of_t(*t), *l))
                    .collect();
                    rsx! {
                        div {
                            key: "lane{vi}",
                            class: if is_sel { "relative flex-1 min-h-0 cursor-pointer" } else { "relative flex-1 min-h-0 cursor-pointer opacity-60 hover:opacity-90" },
                            style: if is_sel { format!("order: {}; border-left: 2px solid {color}; background: {color}0a;", vi * 2) } else { format!("order: {}; border-left: 2px solid transparent;", vi * 2) },
                            onclick: {
                                let name = b.name.clone();
                                move |e: MouseEvent| {
                                    sel.set(vi);
                                    // This reverb's own presets, not the Time module's.
                                    e.stop_propagation();
                                    if let Some(s) = pick_block {
                                        s.set(crate::module_sidebar::Selection::Block {
                                            name: name.clone(),
                                            block_type: "reverb".into(),
                                        });
                                    }
                                }
                            },
                            {reverb_lane(
                                t60 as f32,
                                size,
                                param_v(b, "predelay", 0.0),
                                mix,
                                param_v(b, "damp", 0.0).clamp(0.0, 1.0),
                                param_v(b, "algorithm", 1.0) as u32,
                                !dim,
                                60_000.0 / tempo_bpm.max(1) as f32,
                                &markers,
                                color,
                                dim,
                                &top,
                                &bot,
                                x_of_t(t60),
                            )}
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
                                    fts_chrome::Glyph { icon: fts_chrome::Icon::Power, size: 12 }
                                }
                                span { style: "font-size:8px; font-weight:700; color:{color};", "{vi + 1}" }
                                if dim {
                                    BypassedBadge { small: true }
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
            div { class: "flex items-end justify-around gap-1.5 px-1.5 py-1 border-y border-border flex-shrink-0",
                style: if cur.bypassed { "order: 1; opacity: 0.4;" } else { "order: 1;" },
                if let Some(p) = param(&cur, "mix") {
                    PKnob { block_id: cur_id.clone(), name: "mix", label: "Mix", p }
                }
                // Reverb 1, Dry, Reverb 2 in parallel, as the delays: the
                // stage's Dry lives on its first reverb.
                if let Some(tap) = verbs.first() {
                    if let Some(p) = param(tap, "dry") {
                        PKnob { block_id: tap.id.clone(), name: "dry", label: "Dry", p }
                    }
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
                        fmt: Some(crate::knob::FmtFn(decay_seconds_label as fn(f32) -> String)),
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
                    PKnob { block_id: cur_id, name: "pan_a", label: "Pan", p }
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
    #[props(default)]
    tempo_divisions: bool,
    /// Show the Pre FX module's blocks (in front of the amp) instead.
    #[props(default)]
    pre: bool,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let members: Vec<LiveBlock> = kinds
        .iter()
        .filter_map(|k| {
            blocks
                .iter()
                .find(|b| b.block_type == *k && is_pre_fx(b) == pre)
                .cloned()
        })
        .collect();
    if members.is_empty() {
        return rsx! { {empty_slot(title)} };
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
                    let _ = r.set_block_bypass(m.id.clone(), i != next as usize).await;
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
        let y = ((t * rate * 2.0 * std::f32::consts::TAU).sin() * depth).mul_add(-18.0, 26.0);
        d.push_str(if px == 0 { "M " } else { "L " });
        let _ = write!(d, "{:.1} {:.1} ", 4.0 + t * 192.0, y);
    }
    // Modulation is cyan, motion is pink — the two groups sit one above the
    // other and the colour is how you tell which you are reading.
    let group_color = if tempo_divisions {
        "#f472b6"
    } else {
        "#22d3ee"
    };
    let color = if engaged { group_color } else { "#3f3f46" };

    // Motion speed: current rate expressed as the nearest tempo division.
    let quarter_hz = tempo_bpm.max(1) as f32 / 60.0;
    let div_hz: Vec<f32> = [1.0f32, 0.75, 0.5, 1.0 / 3.0, 0.25, 0.618, 0.414]
        .iter()
        .map(|beats| quarter_hz / beats)
        .collect();
    let cur_div = div_hz
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - rate).abs().partial_cmp(&(b.1 - rate).abs()).unwrap())
        .map_or(0, |(i, _)| i);

    rsx! {
        div { class: "flex flex-col h-full min-h-0", style: "background: #080808;",
            // Header: arrows rotate the engaged member. The right inset keeps
            // the engine picker clear of ZoomPanel's floating expand button.
            div { class: "flex items-center gap-1 pl-1.5 pt-1 flex-shrink-0", style: "padding-right: 22px;",
                button {
                    style: if engaged { "font-size:10px; line-height:1; color:#4ade80;" } else { "font-size:10px; line-height:1; color:#52525b;" },
                    title: if engaged { "Bypass group" } else { "Engage" },
                    onclick: {
                        let rig = rig.clone();
                        let members = members;
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
                    fts_chrome::Glyph { icon: fts_chrome::Icon::Power, size: 12 }
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
                        let rig = rig;
                        let id = cur.id;
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
            // LFO trace — flat while the group is bypassed (the panel's
            // BYPASSED badge says so).
            div { class: "relative flex-1", style: "min-height: 14px;",
                {mod_lane(&cur, rate, depth, engaged, group_color, &d, color)}
            }
            // Mix + Speed.
            div { class: "flex items-end justify-around px-1 pb-0.5 flex-shrink-0 gap-1",
                if let Some(p) = param(&cur, "mix") {
                    PKnob { block_id: cur.id.clone(), name: "mix", label: "Mix", p, tiny: true }
                } else if let Some(p) = param(&cur, "depth") {
                    PKnob { block_id: cur.id.clone(), name: "depth", label: "Mix", p, tiny: true }
                }
                if tempo_divisions {
                    // Speed as a note division, mapped to Hz from the tempo.
                    div { class: "flex flex-col gap-0.5",
                        span { style: "font-size:8px; font-weight:600; text-transform:uppercase; color:#8a8a92;", "Speed" }
                        Picker {
                            options: ["1/4", "1/8.", "1/8", "1/4T", "1/16", "Golden", "Silver"]
                                .iter().map(|l| (*l).to_string()).collect::<Vec<String>>(),
                            selected: cur_div as u32,
                            size: PickerSize::Tiny,
                            on_select: {
                                let rig = rig.clone();
                                let id = cur.id.clone();
                                let div_hz = div_hz.clone();
                                move |i: u32| {
                                    let hz = div_hz.get(i as usize).copied().unwrap_or(2.0);
                                    if let Some(r) = rig.clone() {
                                        let id = id.clone();
                                        spawn(async move {
                                            let _ = r.write_param(id, "rate".into(), hz).await;
                                        });
                                    }
                                }
                            },
                        }
                    }
                } else if let Some(p) = param(&cur, "rate") {
                    PKnob {
                        block_id: cur.id.clone(),
                        name: "rate",
                        label: "Speed",
                        p,
                        tiny: true,
                        fmt: Some(crate::knob::FmtFn((|v| format!("{v:.2}Hz")) as fn(f32) -> String)),
                    }
                }
            }
        }
    }
}

// ── The drive board rail ──// ── The drive board rail ───────────────────────────────────────────────────

/// The Pre FX module: each block in front of the amp as a strip — power,
/// name, and the knobs that matter for it.
#[component]
fn PreFxPanel(blocks: Vec<LiveBlock>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    rsx! {
        div { class: "flex flex-col gap-0 h-full min-h-0",
            for b in blocks {
                {
                    let knobs: &[(&'static str, &'static str)] = match b.block_type {
                        BlockType::Reverb => &[("mix", "Mix"), ("decay", "Decay"), ("size", "Size"), ("tone", "Tone")],
                        BlockType::Delay => &[("mix", "Mix"), ("time", "Time"), ("feedback", "Fdbk")],
                        _ => &[("depth", "Depth"), ("rate", "Rate"), ("mix", "Mix")],
                    };
                    let (rig, id) = (rig.clone(), b.id.clone());
                    rsx! {
                        div { key: "{b.id}", class: "flex items-center gap-2 px-2 border-b border-border/50 min-h-0", style: "flex: 1 1 0%;",
                            div {
                                class: "cursor-pointer select-none flex-shrink-0 text-[10px] font-bold",
                                style: if b.bypassed { "color: #52525b;" } else { "color: #22c55e;" },
                                title: "Engage / bypass",
                                onclick: move |_| {
                                    let (rig, id) = (rig.clone(), id.clone());
                                    spawn(async move {
                                        let Some(r) = rig else { return };
                                        let _ = r.toggle_block_bypass(id).await;
                                    });
                                },
                                "⏻"
                            }
                            span {
                                class: if b.bypassed { "text-[10px] w-16 flex-shrink-0 text-muted-foreground" } else { "text-[10px] w-16 flex-shrink-0 font-semibold" },
                                "{b.name}"
                            }
                            div { class: "flex items-center gap-1 flex-1 min-w-0",
                                for (pname, label) in knobs.iter().copied() {
                                    if let Some(p) = param(&b, pname) {
                                        PKnob { key: "{pname}", block_id: b.id.clone(), name: pname, label, p, tiny: true }
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

/// A module's preset controls, at the head of its row: ▾ opens the Library
/// on that module's presets, ‹ › step through the playing preset's
/// snapshots. The label is what the module plays — preset · snapshot.
#[component]
fn ModuleControls(
    kind: crate::library::Kind,
    pick: Option<signal_guitar_proto::ModulePick>,
    /// Extra inline style (width, height) — the controls sit in a row's
    /// head or a panel's corner.
    #[props(default)]
    style: String,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let open = try_use_context::<crate::library::OpenLibrary>();
    let select = try_use_context::<crate::module_sidebar::SelectedModule>();
    let module = kind.module().unwrap_or_default();
    let (preset, snapshot) = pick
        .as_ref()
        .map(|p| (p.preset.clone(), p.snapshot.clone()))
        .unwrap_or_default();
    let step = move |delta: i32| {
        let rig = rig.clone();
        move |e: MouseEvent| {
            e.stop_propagation();
            if let Some(r) = rig.clone() {
                spawn(async move {
                    let _ = r.step_module(module.to_string(), delta).await;
                });
            }
        }
    };
    let btn = "flex items-center justify-center w-5 h-full text-[10px] text-muted-foreground hover:text-foreground hover:bg-accent/40 cursor-pointer select-none flex-shrink-0";
    rsx! {
        div {
            class: "flex items-center gap-0 border border-border overflow-hidden flex-shrink-0",
            style: "background: #0d0d10; {style}",
            div {
                class: btn,
                title: "Browse {module} presets",
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    if let Some(mut o) = open.map(|o| o.0) {
                        o.set(Some(kind));
                    }
                },
                fts_chrome::Glyph { icon: fts_chrome::Icon::ChevronDown, size: 10 }
            }
            div { class: "flex flex-col justify-center min-w-0 flex-1 px-1 leading-none cursor-pointer hover:bg-accent/30",
                title: "Show {module} presets",
                // Selecting the module opens its presets in the right sidebar.
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    if let Some(sel) = select {
                        sel.set(crate::module_sidebar::Selection::Module(module.to_string()));
                    }
                },
                span { class: "text-[8px] uppercase tracking-wider text-muted-foreground", "{module}" }
                span { class: "text-[10px] font-semibold truncate",
                    if preset.is_empty() { "—" } else { "{preset}" }
                    if !snapshot.is_empty() {
                        span { class: "text-muted-foreground font-normal", " · {snapshot}" }
                    }
                }
            }
            div { class: btn, title: "Previous snapshot", onclick: step(-1), "‹" }
            div { class: btn, title: "Next snapshot", onclick: step(1), "›" }
        }
    }
}

/// The Pitch strip: the note being played (large), a vertical cents meter
/// with the in-tune zone, and a short trace of the detected pitch.
#[component]
fn PitchStrip() -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut reading = use_signal(signal_guitar_proto::TunerReading::default);
    let mut trace = use_signal(|| std::collections::VecDeque::<f32>::with_capacity(48));
    use_future(move || {
        let rig = rig.clone();
        async move {
            let Some(rig) = rig else { return };
            loop {
                if let Ok(r) = rig.tuner().await {
                    let mut t = trace.write();
                    if t.len() >= 48 {
                        t.pop_front();
                    }
                    t.push_back(if r.active { r.cents } else { f32::NAN });
                    drop(t);
                    reading.set(r);
                }
                architect::platform::sleep(std::time::Duration::from_millis(60)).await;
            }
        }
    });
    let r = reading();
    let in_tune = r.active && r.cents.abs() <= 5.0;
    let accent = if in_tune {
        "#22c55e"
    } else if r.active {
        "#eab308"
    } else {
        "#3f3f46"
    };
    // Cents → y in a 0..100 box, sharp up.
    let y = |c: f32| 50.0 - c.clamp(-50.0, 50.0);
    // One line per run of readings: a gap (no note) breaks the line rather
    // than joining across it, and a run too short to be a line is not drawn
    // — an empty or one-point polyline is invalid SVG, and the renderer
    // warns about it on every repaint (most of the time: the tuner is idle).
    let segments: Vec<String> = {
        let mut out = Vec::new();
        let mut run: Vec<String> = Vec::new();
        let flush = |run: &mut Vec<String>, out: &mut Vec<String>| {
            if run.len() >= 2 {
                out.push(run.join(" "));
            }
            run.clear();
        };
        for (i, c) in trace.read().iter().enumerate() {
            if c.is_finite() {
                run.push(format!("{:.1},{:.1}", i as f32 * (40.0 / 47.0), y(*c)));
            } else {
                flush(&mut run, &mut out);
            }
        }
        flush(&mut run, &mut out);
        out
    };
    let needle = y(r.cents);
    rsx! {
        div { style: "display: flex; flex-direction: column; align-items: center; height: 100%; padding: 18px 4px 6px; gap: 4px; min-height: 0;",
            span { style: "font-size: 15px; font-weight: 800; line-height: 1; color: {accent};",
                if r.active { "{r.note}" } else { "—" }
            }
            span { style: "font-size: 9px; font-family: monospace; color: #a1a1aa;",
                {if r.active { format!("{:+.0}¢", r.cents) } else { String::new() }}
            }
            div { style: "flex: 1 1 0%; min-height: 0; width: 100%; display: flex; gap: 3px;",
                // The cents meter: in-tune band, centre line, needle.
                div { style: "position: relative; width: 10px; height: 100%; background: #0a0a0d; border: 1px solid #26262b; border-radius: 3px;",
                    div { style: "position: absolute; left: 0; right: 0; top: 45%; height: 10%; background: rgba(34,197,94,0.18);" }
                    div { style: "position: absolute; left: 0; right: 0; top: 50%; height: 1px; background: rgba(255,255,255,0.35);" }
                    if r.active {
                        div { style: "position: absolute; left: -1px; right: -1px; top: {needle}%; height: 2px; background: {accent};" }
                    }
                }
                // Where the pitch has been.
                svg {
                    style: "flex: 1 1 0%; height: 100%; min-width: 0;",
                    view_box: "0 0 40 100",
                    preserve_aspect_ratio: "none",
                    line { x1: "0", y1: "50", x2: "40", y2: "50", stroke: "#27272a", stroke_width: "1" }
                    for points in segments {
                        polyline { points: "{points}", fill: "none", stroke: "{accent}", stroke_width: "1.5", stroke_linejoin: "round" }
                    }
                }
            }
        }
    }
}

/// The Gate: the DI level it keys from, scrolling right-to-left, against
/// its threshold. Moments under the threshold (gated) draw dim; the header
/// says whether it is open now.
#[component]
fn GateViz(block: LiveBlock, level: Signal<f32>) -> Element {
    const N: usize = 90;
    let mut hist = use_signal(|| std::collections::VecDeque::<f32>::from(vec![-90.0; N]));
    use_future(move || async move {
        loop {
            let v = *level.peek();
            let mut h = hist.write();
            h.pop_front();
            h.push_back(v);
            drop(h);
            architect::platform::sleep(std::time::Duration::from_millis(33)).await;
        }
    });
    let threshold = param(&block, "threshold").map_or(-50.0, |p| p.value);
    let floor = -90.0f32;
    let y = |db: f32| 100.0 * (1.0 - ((db.max(floor) - floor) / -floor));
    let open = !block.bypassed && *level.read() >= threshold;
    let h = hist.read();
    let bars: Vec<(f32, f32, bool)> = h
        .iter()
        .enumerate()
        .map(|(i, db)| (i as f32 * (100.0 / N as f32), y(*db), *db >= threshold))
        // A silent frame is a bar of no height, which usvg rejects (with a
        // warning, on every repaint) — draw nothing for it instead.
        .filter(|(_, top, _)| *top < 99.95)
        .collect();
    let ty = y(threshold);
    let bw = 100.0 / N as f32;
    rsx! {
        div { style: "display: flex; flex-direction: column; height: 100%; min-height: 0; padding: 18px 6px 6px; gap: 4px;",
            div { style: "display: flex; align-items: center; justify-content: space-between;",
                span {
                    style: if open {
                        "font-size: 9px; font-weight: 800; letter-spacing: 0.08em; color: #22c55e;"
                    } else {
                        "font-size: 9px; font-weight: 800; letter-spacing: 0.08em; color: #71717a;"
                    },
                    // Bypassed says itself on the panel's badge.
                    if block.bypassed { "" } else if open { "OPEN" } else { "CLOSED" }
                }
                span { style: "font-size: 9px; font-family: monospace; color: #a1a1aa;", {format!("{threshold:.0} dB")} }
            }
            svg {
                style: "flex: 1 1 0%; width: 100%; min-height: 0; background: #0a0a0d; border: 1px solid #26262b; border-radius: 4px;",
                view_box: "0 0 100 100",
                preserve_aspect_ratio: "none",
                // Gated region: everything under the threshold (none at the
                // −90 dB floor — a zero-height rect is invalid SVG).
                if ty < 99.95 {
                    rect { x: "0", y: "{ty}", width: "100", height: "{100.0 - ty}", fill: "rgba(113,113,122,0.10)" }
                }
                for (x, top, above) in bars {
                    rect {
                        x: "{x}", y: "{top}", width: "{bw}", height: "{100.0 - top}",
                        fill: if above { "rgba(34,197,94,0.75)" } else { "rgba(113,113,122,0.45)" },
                    }
                }
                line { x1: "0", y1: "{ty}", x2: "100", y2: "{ty}", stroke: "#f59e0b", stroke_width: "1.2" }
            }
        }
    }
}

/// The cab after an amp: its IR's name, lit while it convolves. A tap
/// engages/bypasses it; which IR is set on the amp's preset (Library →
/// Presets → Cab), because a cab belongs to the amp tone it was picked for.
#[component]
fn CabChunk(
    /// The live Cabinet block — `None` when no IR is loaded (the slot then
    /// passes the amp straight through: a full-rig capture needs nothing).
    cab: Option<LiveBlock>,
    /// Whether the amp before it is loaded at all.
    amp_loaded: bool,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let engaged = cab.as_ref().is_some_and(|c| !c.bypassed);
    let label = match cab.as_ref() {
        Some(c) if !c.preset.is_empty() => c.preset.clone(),
        Some(c) => c.name.clone(),
        None if amp_loaded => "No cab".to_string(),
        None => "Cab".to_string(),
    };
    let id = cab.as_ref().map(|c| c.id.clone());
    rsx! {
        div {
            class: if id.is_none() {
                "relative flex-1 min-w-0 border border-dashed border-border/40 overflow-hidden select-none"
            } else {
                "relative flex-1 min-w-0 border border-border overflow-hidden cursor-pointer select-none"
            },
            style: if engaged {
                "background: linear-gradient(to right, rgba(180,83,9,0.10), rgba(180,83,9,0.28));"
            } else {
                "background: #0a0a0a;"
            },
            title: if id.is_some() { "Tap to engage/bypass the cab" } else { "No IR on this amp's preset — set one in Library → Presets → Cab" },
            onclick: move |_| {
                if let (Some(r), Some(id)) = (rig.clone(), id.clone()) {
                    spawn(async move { let _ = r.toggle_block_bypass(id).await; });
                }
            },
            div { class: "relative flex items-center gap-1.5 h-full px-2 pointer-events-none",
                span {
                    class: "w-1.5 h-1.5 rounded-full flex-shrink-0",
                    style: if engaged { "background-color: #d97706;" } else { "background-color: #3f3f46;" },
                }
                span {
                    class: if engaged { "text-[10px] font-semibold truncate" } else { "text-[10px] truncate text-muted-foreground" },
                    "{label}"
                }
            }
        }
    }
}

/// A capture's Output Level: a readout dragged vertically. Moves the live
/// block while dragging (`set_block_level`, uncommitted) and stores it with
/// the gear on release — an amp's on its amp module snapshot, a pedal's on
/// its drive option — so every patch playing it follows.
#[component]
fn OutputLevel(block_id: String, level_db: f32) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let bus = signal_widgets::DragBus::try_use();
    // The value shown while a drag is live (the chain echoes it too, but a
    // local copy keeps the readout from lagging the pointer).
    let mut dragging = use_signal(|| None::<f32>);
    let shown = dragging().unwrap_or(level_db);
    rsx! {
        div {
            class: "ml-auto pointer-events-auto flex items-center justify-center rounded-sm border border-border/60 cursor-ns-resize touch-none select-none",
            style: "height: 18px; min-width: 52px; padding: 0 4px; background: rgba(0,0,0,0.35); font-size: 9px; font-family: ui-monospace, monospace;",
            title: "Output Level — drag up/down; saved with the amp or pedal",
            onpointerdown: move |e: PointerEvent| {
                // Not the row's fader underneath.
                e.stop_propagation();
                let (Some(bus), Some(r)) = (bus, rig.clone()) else { return };
                let (y0, start) = (e.client_coordinates().y, level_db);
                let id = block_id.clone();
                let last = std::rc::Rc::new(std::cell::Cell::new(start));
                dragging.set(Some(start));
                bus.begin(move |ev| match ev {
                    signal_widgets::DragEvent::Move { y, .. } => {
                        let v = ((start as f64 + (y0 - y) * 0.05) * 10.0).round() as f32 / 10.0;
                        if (v - last.get()).abs() < f32::EPSILON {
                            return;
                        }
                        last.set(v);
                        let mut shown = dragging;
                        shown.set(Some(v));
                        let (r, id) = (r.clone(), id.clone());
                        spawn(async move { let _ = r.set_block_level(id, v, false).await; });
                    }
                    signal_widgets::DragEvent::End => {
                        let v = last.get();
                        let mut shown = dragging;
                        shown.set(None);
                        if (v - start).abs() >= f32::EPSILON {
                            let (r, id) = (r.clone(), id.clone());
                            spawn(async move { let _ = r.set_block_level(id, v, true).await; });
                        }
                    }
                });
            },
            span { style: "color: #8a8a92; margin-right: 3px;", "OUT" }
            span { style: "color: #e8e8ec; font-variant-numeric: tabular-nums;", {format!("{shown:+.1}")} }
        }
    }
}

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
    /// None → an empty slot (e.g. Amp R until a second amp is loaded).
    #[props(default)]
    block_id: Option<String>,
    /// The wire param the bar writes.
    #[props(default = "drive")]
    param: &'static str,
    /// Map bar position 0..1 → param value.
    #[props(default = (0.0, 1.0))]
    range: (f32, f32),
    /// Amber accent for the amps instead of drive red.
    #[props(default)]
    amp_style: bool,
    /// The block preset's NAM options + current selection (quick switch).
    #[props(default)]
    options: Vec<String>,
    #[props(default)] option: u32,
    /// The capture's Output Level (dB), stored with the gear; `None` hides
    /// the control.
    #[props(default)]
    output_level: Option<f32>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    // (start_x, moved) while a pointer is down — a motionless release is a
    // tap (bypass toggle), movement is a level drag.
    let mut gesture = use_signal(|| None::<(f64, bool)>);
    let bus = signal_widgets::DragBus::try_use();

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
                let Ok(rect) = el.get_client_rect().await else {
                    return;
                };
                let frac = (coords.x / rect.width()).clamp(0.0, 1.0) as f32;
                if let (Some(r), Some(id)) = (rig, block_id) {
                    let v = range.0 + frac * (range.1 - range.0);
                    let _ = r.write_param(id, param.to_string(), v).await;
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
            onpointerdown: {
                let rig = rig.clone();
                let block_id = block_id.clone();
                move |e: PointerEvent| {
                    if empty {
                        return;
                    }
                    let x0 = e.client_coordinates().x;
                    let Some(bus) = bus else {
                        gesture.set(Some((x0, false)));
                        return;
                    };
                    // The root follows the drag across the window: a tap
                    // (no movement) toggles the pedal, movement sets the
                    // level from where the pointer is along the row.
                    let (rig, block_id, el) = (rig.clone(), block_id.clone(), el());
                    spawn(async move {
                        let Some(el) = el else { return };
                        let Ok(rect) = el.get_client_rect().await else { return };
                        let (left, width) = (rect.origin.x, rect.width().max(1.0));
                        let moved = std::rc::Rc::new(std::cell::Cell::new(false));
                        bus.begin(move |ev| match ev {
                            signal_widgets::DragEvent::Move { x, .. } => {
                                if !moved.get() && (x - x0).abs() <= 4.0 {
                                    return;
                                }
                                moved.set(true);
                                let frac = ((x - left) / width).clamp(0.0, 1.0) as f32;
                                if let (Some(r), Some(id)) = (rig.clone(), block_id.clone()) {
                                    let v = range.0 + frac * (range.1 - range.0);
                                    spawn(async move { let _ = r.write_param(id, param.to_string(), v).await; });
                                }
                            }
                            signal_widgets::DragEvent::End => {
                                if !moved.get() {
                                    if let (Some(r), Some(id)) = (rig.clone(), block_id.clone()) {
                                        spawn(async move { let _ = r.toggle_block_bypass(id).await; });
                                    }
                                }
                            }
                        });
                    });
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
                let rig = rig;
                let block_id = block_id;
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
                // Output Level: drag up/down (0.05 dB a pixel). Live while
                // dragging, stored with the gear on release.
                if let (Some(db), Some(id)) = (output_level, block_id.clone()) {
                    OutputLevel { block_id: id, level_db: db }
                }
                // Quick-switch: the captures within a drive's preset, or the
                // pool presets an amp can be. Drawn rather than a `<select>`,
                // which Blitz renders with platform chrome and no popup.
                if options.len() > 1 {
                    div { class: "ml-auto pointer-events-auto",
                        Picker {
                            options: options.clone(),
                            selected: option,
                            size: PickerSize::Tiny,
                            width: "84px".to_string(),
                            on_select: {
                                let rig = rig.clone();
                                let block_id = block_id.clone();
                                move |v: u32| {
                                    if let (Some(r), Some(id)) = (rig.clone(), block_id.clone()) {
                                        spawn(async move { let _ = r.set_block_option(id, v).await; });
                                    }
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

/// One delay lane's taps: the painted widget where a renderer can composite
/// a scene, the SVG stems where it cannot.
///
/// The browser build is the second case and not a lesser one — a DOM renderer
/// cannot be handed a painted scene at all, so the stems are what it draws.
fn delay_lane(
    taps: Vec<(f32, f32, bool)>,
    win_ms: f32,
    on: bool,
    color: &'static str,
    _w: f32,
    beat_ms: f32,
    division: String,
    style: u32,
) -> Element {
    let color = crate::fx_viz::rgb(color);
    // The style index is `DELAY_ALGOS`'s, which is the DSP's own order; the
    // effect owns the table that turns it into a machine.
    let family = delay_ui::viz::family_of_style(style);
    rsx! { crate::fx_viz::DelayViz { taps, win_ms, on, beat_ms, division, family, color } }
}

/// Which of the effect's engines a rig block is.
///
/// Signal's side of the join: `modulation-ui` owns the pictures and knows
/// nothing about `BlockType`, which is the rig's vocabulary, not the
/// effect's. Translating here is what keeps the visualiser reusable by the
/// plugins, which have no block types at all.
/// The block types each modulation slot offers, as the effect groups them.
///
/// Stated here as one list per slot so the pickers below and the visualiser
/// cannot disagree about which machines belong where — `modulation-ui` owns
/// the grouping and `modulation_slots` is checked against it.
const MOD_KINDS: [BlockType; 3] = [BlockType::Chorus, BlockType::Phaser, BlockType::Flanger];
const MOTION_KINDS: [BlockType; 3] = [BlockType::Trem, BlockType::Vibrato, BlockType::Rotary];

fn engine_of(block_type: BlockType) -> Option<crate::mod_viz::Engine> {
    use crate::mod_viz::Engine;
    Some(match block_type {
        BlockType::Chorus => Engine::Chorus,
        BlockType::Flanger => Engine::Flanger,
        BlockType::Phaser => Engine::Phaser,
        BlockType::Trem => Engine::Tremolo,
        BlockType::Vibrato => Engine::Vibrato,
        BlockType::Rotary => Engine::Rotary,
        _ => return None,
    })
}

/// One modulation lane: the engine's own painted visualiser where a renderer
/// can composite a scene, the generic LFO trace where it cannot.
fn mod_lane(
    cur: &LiveBlock,
    rate: f32,
    depth: f32,
    engaged: bool,
    group_color: &'static str,
    _d: &str,
    _stroke: &'static str,
) -> Element {
    let Some(engine) = engine_of(cur.block_type) else {
        return rsx! {};
    };
    let mix = param_v(cur, "mix", 0.5);
    let color = crate::fx_viz::rgb(group_color);
    rsx! {
        crate::mod_viz::ModViz { engine, rate, depth, mix, on: engaged, color }
    }
}

/// One reverb lane: the painted widget where a renderer can composite a
/// scene, the SVG tail where it cannot.
#[expect(
    clippy::too_many_arguments,
    reason = "a drawing and everything it needs"
)]
fn reverb_lane(
    decay: f32,
    density: f32,
    predelay: f32,
    mix: f32,
    damp: f32,
    algorithm: u32,
    on: bool,
    beat_ms: f32,
    _markers: &[(f32, &'static str)],
    color: &'static str,
    _dim: bool,
    _top: &str,
    _bot: &str,
    _t60_x: f32,
) -> Element {
    let color = crate::fx_viz::rgb(color);
    // The algorithm index is `VERB_ALGOS`'s, which is the DSP's own order;
    // the effect owns the table that turns it into a machine.
    let family = reverb_ui::viz::family_of_algorithm(algorithm);
    rsx! { crate::fx_viz::ReverbViz { decay, density, predelay, mix, damp, family, on, beat_ms, color } }
}

/// The EQ surface: the plugin's own graph, wherever the rig runs.
///
/// Natively Blitz composites the graph's scene; in a browser the same
/// widget paints onto a canvas (vello on WebGPU), glow shader and all.
/// There is one EQ, and this is it.
fn eq_panel(block: LiveBlock, spectrum: Vec<f32>) -> Element {
    rsx! { crate::eq_vello::EqVelloSurface { block, spectrum } }
}

// ── The Control view ────────────────────────────────────────────────────────

/// The guitar instrument panel — see the module docs for the layout.
#[component]
pub fn ControlView(model: PerformanceModel, state: RigViewState) -> Element {
    // Callbacks made once per site, not once per render (see `stable`).
    let cbs = crate::stable::use_stable();
    let rig = use_hook(try_consume_context::<RigClient>);
    // Which side of the amp the dynamics row shows: POST (Post Comp, Gate,
    // Amp EQ — the Amp module) or PRE (Pre Comp and the Pre FX module).
    // Page one is the everyday surface: Pre Comp, Gate, Amp EQ above the
    // post-amp Motion/Modulation and Time. The rest is on page two, below.
    let bpre = false;
    let blocks = state.blocks.cloned();
    let master_eq = find_block(&blocks, BlockType::Eq, "Master EQ");
    let limiter = find_block(&blocks, BlockType::Compressor, "Limiter");
    let in_db = state.in_peak_db.cloned();
    let out_db = state.out_peak_db.cloned();
    let (in_l, in_r, out_l, out_r) = state.stereo_db.cloned();
    let spectrum = state.spectrum.cloned();
    let comp_wave = state.comp_wave.cloned();
    // Each compressor panel draws its own block's trace and gain reduction.
    let trace_of = |name: &str| -> ((Vec<f32>, Vec<f32>), f32) {
        comp_wave
            .get(name)
            .map_or_else(|| ((Vec::new(), Vec::new()), 0.0), |(i, g, gr)| ((i.clone(), g.clone()), *gr))
    };

    let eq = find_block(&blocks, BlockType::Eq, "Amp EQ");
    // The drive board: Boost + the three drives, plus the amps.
    let board: Vec<LiveBlock> = blocks
        .iter()
        .filter(|b| matches!(b.block_type, BlockType::Boost | BlockType::Drive))
        .cloned()
        .collect();
    let amp_l = blocks
        .iter()
        .find(|b| b.block_type == BlockType::Amp && b.name.eq_ignore_ascii_case("Amp L"))
        .cloned();
    // The amp chunk names the tone that is loaded, not the slot it sits in.
    // The block's own preset is the authority — the session fills it from the
    // active patch's pool preset — and the active stack is only a fallback for
    // the moment before the first chain publish arrives. "Amp L" last, because
    // a slot name tells a player nothing about what they are hearing.
    let amp_preset = amp_l
        .as_ref()
        .map(|a| a.preset.clone())
        .filter(|p| !p.is_empty())
        .or_else(|| {
            model
                .stacks
                .iter()
                .find(|st| st.is_active)
                .map(|st| st.preset.clone())
                .filter(|p| !p.is_empty())
        })
        .unwrap_or_else(|| "Amp L".to_string());
    let amp_r = blocks
        .iter()
        .find(|b| b.block_type == BlockType::Amp && b.name.eq_ignore_ascii_case("Amp R"))
        .cloned();
    let amp_r_preset = amp_r
        .as_ref()
        .map(|a| a.preset.clone())
        .filter(|p| !p.is_empty());
    let cab_l = find_block(&blocks, BlockType::Cabinet, "Cab L");
    let cab_r = find_block(&blocks, BlockType::Cabinet, "Cab R");
    // The module picks the playing patch resolves to, for the rows' heads.
    let mut comp_rev = use_signal(|| model.revision);
    if *comp_rev.peek() != model.revision {
        comp_rev.set(model.revision);
    }
    let compositions = use_resource({
        let rig = rig.clone();
        move || {
            let _ = comp_rev();
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => r.compositions().await.unwrap_or_default(),
                    None => signal_guitar_proto::CompositionModel::default(),
                }
            }
        }
    });
    let pick_of = |module: &str| {
        compositions.read().as_ref().and_then(|c| {
            c.active_modules
                .iter()
                .find(|m| m.module.eq_ignore_ascii_case(module))
                .cloned()
        })
    };
    let (drive_pick, amp_pick, time_pick) = (pick_of("Drive"), pick_of("Amp"), pick_of("Time"));
    let comp = find_block(&blocks, BlockType::Compressor, "Pre Comp");
    let post_comp = find_block(&blocks, BlockType::Compressor, "Post Comp");
    let comp_title = "Compressor";

    let gate = find_block(&blocks, BlockType::Gate, "Gate");

    let hp = model.headphone.clone();
    let _ = out_db;

    rsx! {
        div { class: "flex gap-0 h-full min-h-0 overflow-hidden",
            style: "width: 100%; height: 100%; display: flex; min-height: 0; overflow: hidden;",
            // ── Input meter rail ──
            div { class: "w-6 flex-shrink-0", StereoMeter { label: "In", l_db: in_l, r_db: in_r } }

            // ── Center surface ──
            div { class: "flex flex-col gap-1 flex-1 min-w-0 min-h-0",
                style: "flex: 1 1 0%; min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow-y: scroll;",
                // Main modules, in signal order: Compressor → Gate → Amp EQ,
                // with the time section (Delay | Reverb) docked flush beneath.
                // Grows, so the rows below it have a height to divide. Left
                // content-sized, a `flex` weight on a child has nothing to
                // take a share OF and collapses to its basis — which is zero.
                // The main surface fills the view; what follows it scrolls in.
                div { class: "flex flex-col gap-0 min-h-0", style: "flex: 0 0 100%; min-height: 0; display: flex; flex-direction: column;",
                    // ── The board, two rows of four: the pedals (Boost +
                    // Drive 1-3), then the amp stage (Amp L, Cab L, Amp R,
                    // Cab R) in signal order. A drive or amp chunk is its
                    // level fader; a cab chunk only engages/bypasses. ──
                    div { class: "flex gap-0 flex-shrink-0", style: "height: 30px;",
                        ModuleControls { kind: crate::library::Kind::DriveModules, pick: drive_pick, style: "width: 170px;" }
                        for b in board.iter() {
                            DriveChunk {
                                key: "{b.id}",
                                name: if b.preset.is_empty() { b.name.clone() } else { b.preset.clone() },
                                level: b.params.iter().find(|p| p.name == "drive").map_or(0.5, |p| p.value),
                                engaged: !b.bypassed,
                                block_id: Some(b.id.clone()),
                                options: b.options.clone(),
                                option: b.option,
                                output_level: b.output_level_db,
                            }
                        }
                    }
                    div { class: "flex gap-0 flex-shrink-0", style: "height: 30px;",
                        ModuleControls { kind: crate::library::Kind::AmpModules, pick: amp_pick, style: "width: 170px;" }
                        DriveChunk {
                            // Constant-loudness drive: the bar pushes the
                            // capture harder while calibration holds the
                            // level; center = the capture at unity.
                            name: amp_preset,
                            level: amp_l
                                .as_ref()
                                .and_then(|a| a.params.iter().find(|p| p.name == "drive"))
                                .map_or(0.5, |p| p.value),
                            engaged: amp_l.as_ref().is_some_and(|a| !a.bypassed),
                            block_id: amp_l.as_ref().map(|a| a.id.clone()),
                            options: amp_l.as_ref().map(|a| a.options.clone()).unwrap_or_default(),
                            option: amp_l.as_ref().map_or(0, |a| a.option),
                            output_level: amp_l.as_ref().and_then(|a| a.output_level_db),
                            amp_style: true,
                        }
                        CabChunk { cab: cab_l.clone(), amp_loaded: amp_l.is_some() }
                        DriveChunk {
                            name: amp_r_preset.clone().unwrap_or_else(|| "Amp R — empty".to_string()),
                            level: amp_r
                                .as_ref()
                                .and_then(|a| a.params.iter().find(|p| p.name == "drive"))
                                .map_or(0.5, |p| p.value),
                            engaged: amp_r.as_ref().is_some_and(|a| !a.bypassed),
                            block_id: amp_r.as_ref().map(|a| a.id.clone()),
                            options: amp_r.as_ref().map(|a| a.options.clone()).unwrap_or_default(),
                            option: amp_r.as_ref().map_or(0, |a| a.option),
                            output_level: amp_r.as_ref().and_then(|a| a.output_level_db),
                            amp_style: true,
                        }
                        CabChunk { cab: cab_r.clone(), amp_loaded: amp_r_preset.is_some() }
                    }
                    // Height from the column, not from an aspect ratio.
                    //
                    // This row asked for `aspect-ratio: 25 / 9` to get its
                    // height from its width. Blitz resolves it the other way —
                    // the row took its *width* from its height and came out
                    // 1060px inside a 1285px column, which is what made the EQ
                    // look half-drawn with its ruler hanging off the end. A
                    // flex weight against the time row below gives the same
                    // proportion without the ambiguity, and the compressor's
                    // `aspect-square` still takes its width from the height.
                    div { class: "flex gap-0 min-h-0 w-full", style: "flex: 3 1 0%; min-height: 0;",
                        // Height-driven square: width follows the row height.
                        div { class: "min-h-0 h-full aspect-square flex flex-col flex-shrink-0",
                            ZoomPanel {
                                title: comp_title.to_string(),
                                left_power_on: comp.as_ref().map(|b| !b.bypassed),
                                on_left_power: comp.as_ref().map(|b| {
                                    let (rig, id) = (rig.clone(), b.id.clone());
                                    cbs.cb(move |()| {
                                        let (rig, id) = (rig.clone(), id.clone());
                                        spawn(async move {
                                            let Some(r) = rig else { return };
                                            let _ = r.toggle_block_bypass(id).await;
                                        });
                                    })
                                }),
                                if let Some(comp) = comp {
                                    crate::comp_surface::CompSurface {
                                        block: comp.clone(),
                                        wave: trace_of(&comp.name).0,
                                        in_db,
                                        gr_db: trace_of(&comp.name).1,
                                    }
                                } else {
                                    {empty_slot(comp_title)}
                                }
                            }
                        }
                        // Pitch — thin: the note being played, how far off
                        // it is, and where it has been.
                        div { class: "min-h-0 h-full flex flex-col flex-shrink-0", style: "width: 64px;",
                            ZoomPanel { title: "Pitch".to_string(),
                                PitchStrip {}
                            }
                        }
                        div { class: "min-h-0 flex flex-col", style: "flex: 1 1 0%;",
                            ZoomPanel {
                                title: "Amp EQ".to_string(),
                                left_power_on: eq.as_ref().map(|b| !b.bypassed),
                                on_left_power: eq.as_ref().map(|b| {
                                    let (rig, id) = (rig.clone(), b.id.clone());
                                    cbs.cb(move |()| {
                                        let (rig, id) = (rig.clone(), id.clone());
                                        spawn(async move {
                                            let Some(r) = rig else { return };
                                            let _ = r.toggle_block_bypass(id).await;
                                        });
                                    })
                                }),
                                if let Some(eq) = eq {
                                    // The plugin's own editor where there is a
                                    // renderer that can paint it; the portable
                                    // SVG re-host on wasm. Same band model
                                    // either way — see `eq_vello`.
                                    {eq_panel(eq, spectrum.clone())}
                                } else {
                                    {empty_slot("Amp EQ")}
                                }
                            }
                        }
                        // The gate, right of the EQ: the DI level it keys
                        // from, scrolling, against its threshold.
                        div { class: "min-h-0 h-full flex flex-col flex-shrink-0", style: "width: 132px;",
                            ZoomPanel {
                                title: "Gate".to_string(),
                                zoomed_view: gate.clone().map(|g| rsx! {
                                    GatePanel { block: g, in_db, expanded: true }
                                }),
                                left_power_on: gate.as_ref().map(|b| !b.bypassed),
                                on_left_power: gate.as_ref().map(|b| {
                                    let (rig, id) = (rig.clone(), b.id.clone());
                                    cbs.cb(move |()| {
                                        let (rig, id) = (rig.clone(), id.clone());
                                        spawn(async move {
                                            let Some(r) = rig else { return };
                                            let _ = r.toggle_block_bypass(id).await;
                                        });
                                    })
                                }),
                                if let Some(gate) = gate {
                                    GateViz { block: gate, level: state.in_peak_db }
                                } else {
                                    {empty_slot("Gate")}
                                }
                            }
                        }
                    }
                    // Time section: stereo delay + stereo reverb + modulation —
                    // or, on PRE, the Pre FX module's motion, delay and reverb.
                    div { class: "flex gap-0 min-h-0 w-full", style: "flex: 2 1 0%; min-height: 150px;",
                        div { class: "min-h-0 h-full flex flex-col gap-0", style: "flex: 1 1 0%;",
                            if !bpre {
                                ZoomPanel { title: "Modulation".to_string(),
                                    bypassed: group_bypassed(&blocks, &MOD_KINDS, false),
                                    ModGroupPanel {
                                        title: "Mod",
                                        kinds: MOD_KINDS.to_vec(),
                                        blocks: blocks.clone(),
                                        tempo_bpm: model.tempo_bpm,
                                    }
                                }
                            }
                            ZoomPanel { title: if bpre { "Pre Motion".to_string() } else { "Motion".to_string() },
                                bypassed: group_bypassed(&blocks, &MOTION_KINDS, bpre),
                                ModGroupPanel {
                                    title: "Motion",
                                    kinds: MOTION_KINDS.to_vec(),
                                    blocks: blocks.clone(),
                                    tempo_bpm: model.tempo_bpm,
                                    tempo_divisions: true,
                                    pre: bpre,
                                }
                            }
                        }
                        // The Time module — delays and reverbs — under one
                        // preset head, like the board's rows.
                        div { class: "min-h-0 h-full flex flex-col", style: "flex: 4 1 0%;",
                        div { class: "flex gap-0 min-h-0 w-full", style: "flex: 1 1 0%;",
                        div { class: "min-h-0 h-full flex flex-col", style: "flex: 2 1 0%;",
                            ZoomPanel {
                                title: if bpre { "Pre Delay".to_string() } else { "Delay".to_string() },
                                module: if bpre { None } else { Some("Time") },
                                power_on: Some(blocks.iter().any(|b| b.block_type == BlockType::Delay && is_pre_fx(b) == bpre && !b.bypassed)),
                                on_power: Some(cbs.cb({
                                    let rig = rig.clone();
                                    let blocks = blocks.clone();
                                    move |(): ()| {
                                        let ids: Vec<(String, bool)> = blocks
                                            .iter()
                                            .filter(|b| b.block_type == BlockType::Delay && is_pre_fx(b) == bpre)
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
                                DelayPanel { blocks: blocks.clone(), tempo_bpm: model.tempo_bpm, pre: bpre }
                            }
                        }
                        div { class: "min-h-0 h-full flex flex-col", style: "flex: 2 1 0%;",
                            ZoomPanel {
                                title: if bpre { "Pre Verb".to_string() } else { "Reverb".to_string() },
                                module: if bpre { None } else { Some("Time") },
                                power_on: Some(blocks.iter().any(|b| b.block_type == BlockType::Reverb && is_pre_fx(b) == bpre && !b.bypassed)),
                                on_power: Some(cbs.cb({
                                    let rig = rig.clone();
                                    let blocks = blocks.clone();
                                    move |(): ()| {
                                        let ids: Vec<(String, bool)> = blocks
                                            .iter()
                                            .filter(|b| b.block_type == BlockType::Reverb && is_pre_fx(b) == bpre)
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
                                ReverbPanel { blocks: blocks.clone(), tempo_bpm: model.tempo_bpm, pre: bpre }
                            }
                        }
                        }
                        if !bpre {
                            ModuleControls { kind: crate::library::Kind::TimeModules, pick: time_pick, style: "height: 22px; width: 100%;" }
                        }
                        }
                    }
                }

                // ── Page two, below the fold (the surface scrolls): the
                // secondary blocks, grouped by use rather than signal order.
                // Post-amp glue and what sits in front of the amp… ──
                div { class: "flex gap-0 flex-shrink-0 w-full", style: "height: 280px;",
                    div { class: "min-h-0 h-full aspect-square flex flex-col flex-shrink-0",
                        ZoomPanel {
                            title: "Post Comp".to_string(),
                            left_power_on: post_comp.as_ref().map(|b| !b.bypassed),
                            on_left_power: post_comp.as_ref().map(|b| {
                                let (rig, id) = (rig.clone(), b.id.clone());
                                cbs.cb(move |()| {
                                    let (rig, id) = (rig.clone(), id.clone());
                                    spawn(async move {
                                        let Some(r) = rig else { return };
                                        let _ = r.toggle_block_bypass(id).await;
                                    });
                                })
                            }),
                            if let Some(pc) = post_comp.clone() {
                                crate::comp_surface::CompSurface { block: pc.clone(), wave: trace_of(&pc.name).0, in_db, gr_db: trace_of(&pc.name).1 }
                            } else {
                                {empty_slot("Post Comp")}
                            }
                        }
                    }
                    div { class: "min-h-0 h-full flex flex-col", style: "flex: 1 1 0%;",
                        ZoomPanel { title: "Pre Motion".to_string(),
                            bypassed: group_bypassed(&blocks, &MOTION_KINDS, true),
                            ModGroupPanel {
                                title: "Motion",
                                kinds: MOTION_KINDS.to_vec(),
                                blocks: blocks.clone(),
                                tempo_bpm: model.tempo_bpm,
                                tempo_divisions: true,
                                pre: true,
                            }
                        }
                    }
                    div { class: "min-h-0 h-full flex flex-col", style: "flex: 2 1 0%;",
                        ZoomPanel { title: "Pre Delay".to_string(),
                            bypassed: group_bypassed(&blocks, &[BlockType::Delay], true),
                            DelayPanel { blocks: blocks.clone(), tempo_bpm: model.tempo_bpm, pre: true }
                        }
                    }
                    div { class: "min-h-0 h-full flex flex-col", style: "flex: 2 1 0%;",
                        ZoomPanel { title: "Pre Verb".to_string(),
                            bypassed: group_bypassed(&blocks, &[BlockType::Reverb], true),
                            ReverbPanel { blocks: blocks.clone(), tempo_bpm: model.tempo_bpm, pre: true }
                        }
                    }
                }
                // …and the Master module: a final EQ and the zero-latency
                // limiter.
                div { class: "flex gap-0 flex-shrink-0 w-full", style: "height: 280px;",
                    div { class: "min-h-0 h-full flex flex-col", style: "flex: 1 1 0%;",
                        ZoomPanel {
                            title: "Master EQ".to_string(),
                            left_power_on: master_eq.as_ref().map(|b| !b.bypassed),
                            on_left_power: master_eq.as_ref().map(|b| {
                                let (rig, id) = (rig.clone(), b.id.clone());
                                cbs.cb(move |()| {
                                    let (rig, id) = (rig.clone(), id.clone());
                                    spawn(async move {
                                        let Some(r) = rig else { return };
                                        let _ = r.toggle_block_bypass(id).await;
                                    });
                                })
                            }),
                            if let Some(meq) = master_eq.clone() {
                                {eq_panel(meq, spectrum.clone())}
                            } else {
                                {empty_slot("Master EQ")}
                            }
                        }
                    }
                    div { class: "min-h-0 h-full aspect-square flex flex-col flex-shrink-0",
                        ZoomPanel {
                            title: "Limiter".to_string(),
                            left_power_on: limiter.as_ref().map(|b| !b.bypassed),
                            on_left_power: limiter.as_ref().map(|b| {
                                let (rig, id) = (rig.clone(), b.id.clone());
                                cbs.cb(move |()| {
                                    let (rig, id) = (rig.clone(), id.clone());
                                    spawn(async move {
                                        let Some(r) = rig else { return };
                                        let _ = r.toggle_block_bypass(id).await;
                                    });
                                })
                            }),
                            if let Some(lim) = limiter.clone() {
                                crate::comp_surface::CompSurface { block: lim.clone(), wave: trace_of(&lim.name).0, in_db, gr_db: trace_of(&lim.name).1 }
                            } else {
                                {empty_slot("Limiter")}
                            }
                        }
                    }
                }

                // (The DSP cost readout lives in the bar's Audio indicator —
                // and the macOS menu bar — so the modules reach the bottom,
                // level with the meters either side.)
            }

            // ── Output rail: mute on top, then FOH trim + out meter,
            // then the phones group — mix fader | phones meter | guitar
            // (self) fader.
            div {
                style: "width: 58px; flex-shrink: 0; display: flex; flex-direction: column; align-items: center; gap: 3px; min-height: 0; padding: 0 2px;",
                button {
                    class: if hp.main_mute {
                        "w-9 rounded px-0.5 py-0.5 text-[8px] font-bold uppercase ring-2 ring-red-500"
                    } else {
                        "w-9 rounded px-0.5 py-0.5 text-[8px] font-bold uppercase border border-border text-muted-foreground hover:text-foreground"
                    },
                    style: if hp.main_mute { "background-color: #ef4444; color: #fff;" } else { "" },
                    onclick: {
                        let rig = rig;
                        move |_| {
                            if let Some(r) = rig.clone() {
                                spawn(async move { let _ = r.toggle_main_mute().await; });
                            }
                        }
                    },
                    if hp.main_mute { "Muted" } else { "Mute" }
                }
                div { style: "flex: 1 1 0%; min-height: 0; width: 100%; display: flex; justify-content: center; gap: 3px;",
                    VFader {
                        label: "Trim",
                        value: (model.master_trim_db + 24.0) / 36.0,
                        readout: format!("{:+.0}dB", model.master_trim_db),
                        on_change: cbs.cb({
                            let rig = rig.clone();
                            move |v: f32| {
                                if let Some(r) = rig.clone() {
                                    spawn(async move { let _ = r.set_master_trim(v.mul_add(36.0, -24.0)).await; });
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
                div { style: "flex: 1 1 0%; min-height: 0; width: 100%; display: flex; justify-content: center; gap: 3px;",
                    VFader {
                        label: "Mix",
                        value: hp.volume,
                        readout: format!("{:.0}%", hp.volume * 100.0),
                        on_change: cbs.cb({
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
                        l_db: 20.0f32.mul_add(hp.volume.max(0.001).log10(), out_l),
                        r_db: 20.0f32.mul_add(hp.volume.max(0.001).log10(), out_r),
                    }
                    VFader {
                        label: "Gtr",
                        value: hp.self_mix,
                        readout: format!("{:.0}%", hp.self_mix * 100.0),
                        on_change: cbs.cb({
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

#[cfg(test)]
// These exercise the painted widgets, which are native only.
#[cfg(not(target_arch = "wasm32"))]
mod slot_tests {
    use super::*;
    use crate::mod_viz::{Engine, Group};

    /// The rig's two modulation slots offer exactly the machines the effect
    /// says belong in them.
    ///
    /// `modulation-ui` owns the grouping — one set colours a signal where it
    /// stands, the other moves it — and the rig listing its own three per
    /// slot is the kind of duplication that drifts silently. A slot offering
    /// a tremolo where a chorus belongs is offering the wrong thing whatever
    /// colour it is drawn in.
    #[test]
    fn each_slot_offers_the_engines_its_group_holds() {
        for (kinds, group) in [
            (MOD_KINDS.as_slice(), Group::Colouring),
            (MOTION_KINDS.as_slice(), Group::Moving),
        ] {
            let mut got: Vec<Engine> = kinds
                .iter()
                .map(|k| engine_of(*k).expect("every offered kind is an engine"))
                .collect();
            let mut want = Engine::of(group).to_vec();
            got.sort_by_key(|e| format!("{e:?}"));
            want.sort_by_key(|e| format!("{e:?}"));
            assert_eq!(got, want, "the {group:?} slot offers the wrong machines");
        }
    }
}
