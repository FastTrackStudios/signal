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
        div { class: "flex flex-col rounded-xl border border-border bg-card min-h-0 overflow-hidden",
            div { class: "flex items-center px-3 py-1 border-b border-border flex-shrink-0",
                span { class: "text-[10px] font-semibold uppercase tracking-wider text-muted-foreground",
                    "{title}"
                }
                button {
                    class: "ml-auto text-muted-foreground hover:text-foreground text-sm",
                    title: "zoom",
                    onclick: move |_| zoomed.set(true),
                    "⤢"
                }
            }
            div { class: "flex-1 min-h-0 p-2", {children.clone()} }
        }
        if zoomed() {
            div { class: "fixed inset-0 z-50 flex flex-col bg-black/95 p-8",
                div { class: "flex items-center mb-4",
                    span { class: "text-sm font-bold uppercase tracking-wider", "{title}" }
                    button {
                        class: "ml-auto text-muted-foreground hover:text-foreground text-xl",
                        onclick: move |_| zoomed.set(false),
                        "✕"
                    }
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

// ── Compressor ──────────────────────────────────────────────────────────────

/// Transfer curve + live input dot + the gain-reduction meter.
#[component]
fn CompPanel(block: LiveBlock, in_db: f32, gr_db: f32) -> Element {
    let thr = param_v(&block, "threshold", -18.0) as f64;
    let ratio = param_v(&block, "ratio", 4.0).max(1.0) as f64;

    const W: f64 = 200.0;
    const H: f64 = 112.0;
    let x_of = |db: f64| (db + 60.0) / 60.0 * W;
    let y_of = |db: f64| H - (db + 60.0) / 60.0 * H;
    let out_of = |i: f64| if i < thr { i } else { thr + (i - thr) / ratio };
    let mut d = String::new();
    for i in 0..=60 {
        let in_pt = -60.0 + i as f64;
        d.push_str(if i == 0 { "M" } else { "L" });
        d.push_str(&format!("{:.1},{:.1} ", x_of(in_pt), y_of(out_of(in_pt))));
    }
    let live = (in_db as f64).clamp(-60.0, 0.0);
    let gr_pct = (gr_db / 24.0 * 100.0).clamp(0.0, 100.0);

    rsx! {
        div { class: "flex gap-2 h-full min-h-0",
            div { class: "flex flex-col gap-1 flex-1 min-w-0",
                svg {
                    class: "w-full flex-1 min-h-0",
                    view_box: "0 0 200 112",
                    preserve_aspect_ratio: "none",
                    line { x1: "{x_of(-60.0)}", y1: "{y_of(-60.0)}", x2: "{x_of(0.0)}", y2: "{y_of(0.0)}",
                        stroke: "#3f3f46", stroke_width: "1", stroke_dasharray: "3,3" }
                    line { x1: "{x_of(thr)}", y1: "0", x2: "{x_of(thr)}", y2: "112",
                        stroke: "#52525b", stroke_width: "1" }
                    path { d: "{d}", fill: "none", stroke: "#60a5fa", stroke_width: "2" }
                    circle { cx: "{x_of(live):.1}", cy: "{y_of(out_of(live)):.1}", r: "4", fill: "#60a5fa" }
                }
                div { class: "grid grid-cols-4 gap-2",
                    for name in ["threshold", "ratio", "attack", "release"] {
                        if let Some(p) = param(&block, name) {
                            ParamSlider { block_id: block.id.clone(), p }
                        }
                    }
                }
            }
            // Gain-reduction meter (top-down, like every comp ever).
            div { class: "flex flex-col items-center gap-1 w-8 flex-shrink-0",
                span { class: "text-[8px] font-semibold uppercase text-muted-foreground", "GR" }
                div { class: "relative flex-1 w-3 rounded bg-black/60 border border-border overflow-hidden",
                    div {
                        class: "absolute inset-x-0 top-0 transition-[height] duration-75",
                        style: "height: {gr_pct}%; background-color: #f97316;",
                    }
                }
                span { class: "text-[8px] font-mono", "{gr_db:.1}" }
            }
        }
    }
}

// ── Gate ────────────────────────────────────────────────────────────────────

/// Live input level vs threshold — the gate's whole story at a glance.
#[component]
fn GatePanel(block: LiveBlock, in_db: f32) -> Element {
    let thr = param_v(&block, "threshold", -50.0);
    let open = in_db >= thr && !block.bypassed;
    let level_pct = ((in_db + 90.0) / 90.0 * 100.0).clamp(0.0, 100.0);
    let thr_pct = ((thr + 90.0) / 90.0 * 100.0).clamp(0.0, 100.0);

    rsx! {
        div { class: "flex flex-col gap-1.5 h-full",
            div { class: "flex items-center gap-2",
                span {
                    class: "text-[10px] font-bold uppercase tracking-wider rounded px-1.5 py-0.5",
                    style: if block.bypassed {
                        "background-color: #3f3f46; color: #a1a1aa;"
                    } else if open {
                        "background-color: #22c55e; color: #052e16;"
                    } else {
                        "background-color: #27272a; color: #71717a;"
                    },
                    if block.bypassed { "Bypassed" } else if open { "Open" } else { "Closed" }
                }
                span { class: "ml-auto text-[9px] font-mono text-muted-foreground", "{in_db:.0} dB" }
            }
            div { class: "relative h-3 rounded bg-black/50 border border-border overflow-hidden",
                div {
                    class: "absolute inset-y-0 left-0 transition-[width] duration-75",
                    style: if open { "width: {level_pct}%; background-color: #22c55e;" }
                           else { "width: {level_pct}%; background-color: #52525b;" },
                }
                div { class: "absolute inset-y-0 w-0.5", style: "left: {thr_pct}%; background-color: #eab308;" }
            }
            div { class: "grid grid-cols-3 gap-2",
                for name in ["threshold", "attack", "release"] {
                    if let Some(p) = param(&block, name) {
                        ParamSlider { block_id: block.id.clone(), p }
                    }
                }
            }
        }
    }
}

// ── Pedals ──────────────────────────────────────────────────────────────────

/// A vertical pedal: drag up/down to set 0–1. Pointer events, footswitch
/// aesthetics.
#[component]
fn Pedal(label: &'static str, value: f32, enabled: bool, on_change: Callback<f32>) -> Element {
    let mut el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let mut tracking = use_signal(|| false);
    let pct = (value * 100.0).clamp(0.0, 100.0);

    let set_from = move |coords: dioxus::html::geometry::ElementPoint| {
        let el = el();
        spawn(async move {
            let Some(el) = el else { return };
            let Ok(rect) = el.get_client_rect().await else { return };
            let v = (1.0 - coords.y / rect.height()).clamp(0.0, 1.0) as f32;
            on_change.call(v);
        });
    };

    rsx! {
        div { class: "flex flex-col items-center gap-1 h-full min-h-0",
            span { class: "text-[9px] font-semibold uppercase tracking-wider text-muted-foreground", "{label}" }
            div {
                class: if enabled {
                    "relative flex-1 w-10 rounded-lg bg-black/60 border border-border overflow-hidden cursor-ns-resize touch-none min-h-0"
                } else {
                    "relative flex-1 w-10 rounded-lg bg-black/40 border border-dashed border-border/50 overflow-hidden min-h-0"
                },
                onmounted: move |e| el.set(Some(e.data())),
                onpointerdown: move |e: PointerEvent| {
                    if enabled {
                        tracking.set(true);
                        set_from(e.element_coordinates());
                    }
                },
                onpointermove: move |e: PointerEvent| {
                    if enabled && tracking() {
                        set_from(e.element_coordinates());
                    }
                },
                onpointerup: move |_| tracking.set(false),
                onpointerleave: move |_| tracking.set(false),
                div {
                    class: "absolute inset-x-0 bottom-0",
                    style: if enabled { "height: {pct}%; background-color: #38bdf880;" } else { "height: {pct}%; background-color: #3f3f4680;" },
                }
                // Treadle line.
                div { class: "absolute inset-x-1 h-0.5 bg-white/70 rounded", style: "bottom: calc({pct}% - 1px);" }
            }
            span { class: "text-[8px] font-mono text-muted-foreground",
                if enabled { {format!("{:.0}%", value * 100.0)} } else { "unassigned" }
            }
        }
    }
}

// ── Tuner + MIDI minis ──────────────────────────────────────────────────────

/// Always-on mini tuner — polls while the Control view is mounted; click for
/// the full-screen tuner.
#[component]
fn MiniTuner(on_open: Callback<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut reading = use_signal(signal_guitar_proto::TunerReading::default);
    {
        let rig = rig.clone();
        use_future(move || {
            let rig = rig.clone();
            async move {
                let Some(rig) = rig else { return };
                loop {
                    if let Ok(r) = rig.tuner().await {
                        reading.set(r);
                    }
                    architect::platform::sleep(Duration::from_millis(250)).await;
                }
            }
        });
    }
    let r = reading();
    let in_tune = r.active && r.cents.abs() <= 5.0;
    let needle = 50.0 + r.cents.clamp(-50.0, 50.0);
    rsx! {
        button {
            class: "flex flex-col rounded-xl border border-border bg-card px-3 py-2 hover:bg-accent/30 text-left h-full min-h-0",
            onclick: move |_| on_open.call(()),
            span { class: "text-[9px] font-semibold uppercase tracking-wider text-muted-foreground", "Tuner" }
            span {
                class: "text-2xl font-bold leading-tight",
                style: if in_tune { "color: #22c55e;" } else if r.active { "color: #e4e4e7;" } else { "color: #3f3f46;" },
                if r.active { "{r.note}" } else { "—" }
            }
            div { class: "relative w-full h-3 mt-auto",
                div { class: "absolute inset-x-0 top-1/2 h-px bg-zinc-700" }
                div { class: "absolute left-1/2 top-0.5 bottom-0.5 w-px bg-zinc-500" }
                if r.active {
                    div {
                        class: "absolute top-0 bottom-0 w-0.5 rounded transition-all duration-75",
                        style: if in_tune { "left: {needle}%; background-color: #22c55e;" } else { "left: {needle}%; background-color: #eab308;" },
                    }
                }
            }
        }
    }
}

/// Mini MIDI monitor — last event + count; click for the full log.
#[component]
fn MidiMini() -> Element {
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
                    architect::platform::sleep(Duration::from_millis(400)).await;
                }
            }
        });
    }
    let entries = log();
    let last = entries.last().cloned();
    rsx! {
        button {
            class: "flex flex-col rounded-xl border border-border bg-card px-3 py-2 hover:bg-accent/30 text-left h-full min-h-0 overflow-hidden",
            onclick: move |_| open.set(true),
            span { class: "text-[9px] font-semibold uppercase tracking-wider text-muted-foreground",
                "MIDI · {entries.len()}"
            }
            span { class: "text-[10px] font-mono truncate mt-auto",
                if let Some(last) = last { "{last}" } else { "no events" }
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

// ── Headphone cue ───────────────────────────────────────────────────────────

/// The headphone-cue module: phones volume + self mix, zoomable for the
/// extended mix controls.
#[component]
fn HeadphonePanel(model: PerformanceModel) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let hp = model.headphone.clone();
    let send = {
        let rig = rig.clone();
        move |vol: f32, mix: f32| {
            if let Some(r) = rig.clone() {
                spawn(async move {
                    let _ = r.set_headphone(vol, mix).await;
                });
            }
        }
    };
    let (vol, mix) = (hp.volume, hp.self_mix);
    rsx! {
        div { class: "flex flex-col gap-1.5 h-full",
            div { class: "flex flex-col gap-0.5",
                div { class: "flex justify-between",
                    span { class: "text-[9px] font-mono text-muted-foreground", "phones" }
                    span { class: "text-[9px] font-mono", {format!("{:.0}%", vol * 100.0)} }
                }
                input {
                    r#type: "range", class: "w-full h-1 accent-primary",
                    min: "0", max: "1", step: "any", value: "{vol}",
                    oninput: {
                        let send = send.clone();
                        move |e: FormEvent| {
                            if let Ok(v) = e.value().parse::<f32>() { send(v, mix); }
                        }
                    },
                }
            }
            div { class: "flex flex-col gap-0.5",
                div { class: "flex justify-between",
                    span { class: "text-[9px] font-mono text-muted-foreground", "self mix" }
                    span { class: "text-[9px] font-mono", {format!("{:.0}%", mix * 100.0)} }
                }
                input {
                    r#type: "range", class: "w-full h-1 accent-primary",
                    min: "0", max: "1", step: "any", value: "{mix}",
                    oninput: move |e: FormEvent| {
                        if let Ok(v) = e.value().parse::<f32>() { send(vol, v); }
                    },
                }
            }
            span { class: "text-[8px] text-muted-foreground/60 mt-auto",
                "routes to the hardware phones bus when it lands"
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
    on_prev_song: Callback<()>,
    on_next_song: Callback<()>,
    on_tap_tempo: Callback<()>,
    on_open_tuner: Callback<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let blocks = state.blocks.cloned();
    let in_db = state.in_peak_db.cloned();
    let out_db = state.out_peak_db.cloned();
    let gr_db = state.comp_gr_db.cloned();
    let spectrum = state.spectrum.cloned();

    let song_idx = model.song_index as usize;
    let current_song = model.songs.get(song_idx).cloned().unwrap_or_default();
    let section = model
        .sections
        .get(model.section_index as usize)
        .cloned()
        .unwrap_or_default();

    let eq = find_block(&blocks, BlockType::Eq, "Pre EQ");
    let comp = find_block(&blocks, BlockType::Compressor, "Compressor");
    let gate = find_block(&blocks, BlockType::Gate, "Gate");
    let vol_pedal = find_block(&blocks, BlockType::Volume, "Volume Pedal");

    let hp = model.headphone.clone();
    // Headphone meter: main signal scaled by phones volume (the physical bus
    // lands with engine multi-out; the meter shows what it will carry).
    let hp_db = out_db + 20.0 * hp.volume.max(0.001).log10();
    let main_db = if hp.main_mute { -90.0 } else { out_db };

    // Volume pedal position: gain −60..0 dB → 0..1.
    let pedal_pos = vol_pedal
        .as_ref()
        .map(|b| (param_v(b, "gain_db", 0.0) + 60.0) / 60.0)
        .unwrap_or(1.0);
    let vol_pedal_id = vol_pedal.as_ref().map(|b| b.id.clone());

    rsx! {
        div { class: "flex gap-3 h-full min-h-0 overflow-hidden",
            // ── Input meter rail ──
            div { class: "w-10 flex-shrink-0 py-1", VMeter { label: "In", level_db: in_db } }

            // ── Center surface ──
            div { class: "flex flex-col gap-2 flex-1 min-w-0 min-h-0",
                // Song strip.
                div { class: "grid grid-cols-4 gap-2 flex-shrink-0", style: "height: 52px;",
                    button {
                        class: "flex flex-col items-start justify-center rounded-lg border border-border bg-card px-3 hover:bg-accent/40 text-left min-w-0",
                        onclick: move |_| on_prev_song.call(()),
                        span { class: "text-[8px] uppercase tracking-wider text-muted-foreground", "‹ Prev" }
                        span { class: "text-xs font-bold truncate w-full",
                            {song_idx.checked_sub(1).and_then(|i| model.songs.get(i)).cloned().unwrap_or("—".into())}
                        }
                    }
                    div { class: "flex flex-col items-center justify-center rounded-lg border border-border bg-card px-3 min-w-0",
                        span { class: "text-[8px] uppercase tracking-wider text-muted-foreground",
                            "{model.song_index + 1}/{model.songs.len()} · {section}"
                        }
                        span { class: "text-sm font-bold truncate w-full text-center", "{current_song}" }
                    }
                    button {
                        class: "flex flex-col items-end justify-center rounded-lg border border-border bg-card px-3 hover:bg-accent/40 text-right min-w-0",
                        onclick: move |_| on_next_song.call(()),
                        span { class: "text-[8px] uppercase tracking-wider text-muted-foreground", "Next ›" }
                        span { class: "text-xs font-bold truncate w-full",
                            {model.songs.get(song_idx + 1).cloned().unwrap_or("—".into())}
                        }
                    }
                    button {
                        class: "flex flex-col items-center justify-center rounded-lg border border-border bg-card px-3 hover:bg-accent/40",
                        onclick: move |_| on_tap_tempo.call(()),
                        span { class: "text-[8px] uppercase tracking-wider text-muted-foreground", "Tap" }
                        span { class: "text-sm font-bold", "{model.tempo_bpm} BPM" }
                    }
                }

                // Main modules: EQ (16:9) + Comp, gate & reserved to the right.
                div { class: "grid grid-cols-5 gap-2 flex-1 min-h-0",
                    div { class: "col-span-2 min-h-0 flex flex-col",
                        ZoomPanel { title: "Pre-FX EQ".to_string(),
                            if let Some(eq) = eq {
                                crate::eq_surface::EqProSurface { block: eq, spectrum }
                            } else {
                                span { class: "text-xs text-muted-foreground italic", "No Pre EQ in the chain." }
                            }
                        }
                    }
                    div { class: "col-span-2 min-h-0 flex flex-col",
                        ZoomPanel { title: "Compressor".to_string(),
                            if let Some(comp) = comp {
                                CompPanel { block: comp, in_db, gr_db }
                            } else {
                                span { class: "text-xs text-muted-foreground italic", "No Compressor in the chain." }
                            }
                        }
                    }
                    div { class: "flex flex-col gap-2 min-h-0",
                        ZoomPanel { title: "Gate".to_string(),
                            if let Some(gate) = gate {
                                GatePanel { block: gate, in_db }
                            } else {
                                span { class: "text-xs text-muted-foreground italic", "No Gate." }
                            }
                        }
                        // Reserved module chips.
                        div { class: "flex flex-col gap-1 rounded-xl border border-dashed border-border/50 p-2 flex-1 min-h-0 overflow-hidden",
                            span { class: "text-[9px] font-semibold uppercase tracking-wider text-muted-foreground", "Coming" }
                            for name in ["Env Filter", "Wah", "Pitch", "Doubler", "Drive"] {
                                span { key: "{name}", class: "text-[10px] text-muted-foreground/60", "· {name}" }
                            }
                        }
                    }
                }

                // Bottom rail: tuner, pedals, MIDI, headphone cue.
                div { class: "grid gap-2 flex-shrink-0", style: "grid-template-columns: 1.2fr 0.7fr 0.7fr 1fr 1.4fr; height: 148px;",
                    MiniTuner { on_open: on_open_tuner }
                    Pedal {
                        label: "Volume",
                        value: pedal_pos,
                        enabled: vol_pedal_id.is_some(),
                        on_change: Callback::new({
                            let rig = rig.clone();
                            move |v: f32| {
                                if let Some(id) = &vol_pedal_id {
                                    send_param(&rig, id, "gain_db", v * 60.0 - 60.0);
                                }
                            }
                        }),
                    }
                    Pedal {
                        label: "Expression",
                        value: 0.0,
                        enabled: false,
                        on_change: Callback::new(|_: f32| {}),
                    }
                    MidiMini {}
                    div { class: "flex flex-col rounded-xl border border-border bg-card px-3 py-2 min-h-0",
                        span { class: "text-[9px] font-semibold uppercase tracking-wider text-muted-foreground mb-1",
                            "Headphones"
                        }
                        HeadphonePanel { model: model.clone() }
                    }
                }
            }

            // ── Output rail: main out, mute, headphone out ──
            div { class: "w-14 flex-shrink-0 flex flex-col items-center gap-2 py-1 min-h-0",
                div { class: "flex-1 min-h-0 w-full flex justify-center",
                    VMeter { label: "Out", level_db: main_db, muted: hp.main_mute }
                }
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
                div { class: "flex-1 min-h-0 w-full flex justify-center",
                    VMeter { label: "Phns", level_db: hp_db }
                }
            }
        }
    }
}
