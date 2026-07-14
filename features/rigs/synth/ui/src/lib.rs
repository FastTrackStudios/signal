//! Synth-rig Dioxus components — the remote GUI half of the detachable synth
//! rig. Renders purely from `signal-synth-proto` via the generated vox clients
//! (provided in Dioxus context by the host). Inline styles only (Blitz-safe).
//!
//! Sibling of `signal-keys-ui`: a **preset browser** on the left (the imported
//! Omnisphere patches), the **control view** in the middle (boxes for the
//! Quadzone + its layers, from the composition tree), and a **performance**
//! strip (the piano) at the bottom.

use dioxus::prelude::*;
use midicore_proto::MidiEvent;
use midicore_ui::MidiMonitorPanel;
use signal_synth_proto::synth::{SynthEvent, SynthRigClient, SynthRigStreamClient};
use signal_synth_proto::{SynthMapping, SynthNode, SynthPreset, SynthStatus, SynthZone};
use signal_ui::components::Piano;

/// Which top-level view the remote is showing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Live performance: preset browser + MIDI monitor + keyboard.
    Play,
    /// Read-only keymap editor (KODA-style zone grid).
    Edit,
}

/// Live synth-rig view-state: seeded once, then folded from the event stream.
#[derive(Clone, Copy)]
struct SynthState {
    status: Signal<SynthStatus>,
    presets: Signal<Vec<SynthPreset>>,
    tree: Signal<SynthNode>,
    midi: Signal<Vec<MidiEvent>>,
}

fn use_synth_state() -> (SynthState, Option<SynthRigClient>) {
    let rig = use_hook(try_consume_context::<SynthRigClient>);
    let stream = use_hook(try_consume_context::<SynthRigStreamClient>);

    let mut status = use_signal(SynthStatus::default);
    let mut presets = use_signal(Vec::<SynthPreset>::new);
    let mut tree = use_signal(SynthNode::default);
    let mut midi = use_signal(Vec::<MidiEvent>::new);

    // Seed once — start the rig, then pull the current state.
    {
        let rig = rig.clone();
        use_future(move || {
            let rig = rig.clone();
            async move {
                let Some(rig) = rig else { return };
                let _ = rig.start().await;
                if let Ok(s) = rig.status().await {
                    status.set(s);
                }
                if let Ok(p) = rig.presets().await {
                    presets.set(p);
                }
                if let Ok(t) = rig.tree().await {
                    tree.set(t);
                }
                if let Ok(m) = rig.midi_recent().await {
                    midi.set(m);
                }
            }
        });
    }

    // Live updates.
    {
        let stream = stream.clone();
        architect::use_stream(
            move |sink| {
                let stream = stream.clone();
                async move {
                    match stream {
                        Some(s) => s.events(sink).await.is_ok(),
                        None => false,
                    }
                }
            },
            move |ev: SynthEvent| {
                let (mut status, mut presets, mut tree, mut midi) = (status, presets, tree, midi);
                match ev {
                    SynthEvent::Status(s) => status.set(s),
                    SynthEvent::Library(p) => presets.set(p),
                    SynthEvent::Tree(t) => tree.set(t),
                    SynthEvent::Midi(m) => midi.set(m),
                }
            },
        );
    }

    (SynthState { status, presets, tree, midi }, rig)
}

/// The synth-rig remote view. Mount inside a host that has provided
/// `SynthRigClient` + `SynthRigStreamClient` in context.
#[component]
pub fn SynthRigRemote() -> Element {
    let (state, rig) = use_synth_state();
    let mut mode = use_signal(|| Mode::Play);
    let status = state.status.read().clone();
    let presets = state.presets.read().clone();
    let midi = state.midi.read().clone();
    let midi_count = midi.len() as u64;

    // Currently-held notes light the piano.
    let lit: Vec<u8> = {
        let mut held = std::collections::BTreeSet::<u8>::new();
        for e in midi.iter() {
            match e {
                MidiEvent::NoteOn { key, velocity, .. } if velocity.get() > 0 => {
                    held.insert(key.get());
                }
                MidiEvent::NoteOn { key, .. } | MidiEvent::NoteOff { key, .. } => {
                    held.remove(&key.get());
                }
                _ => {}
            }
        }
        held.into_iter().collect()
    };

    let master_pct = (status.master_peak.clamp(0.0, 1.0) * 100.0) as u32;
    let clipping = status.master_peak >= 0.999;
    let vol_milli = (status.volume.clamp(0.0, 1.0) * 1000.0) as u32;
    let vol_pct = (status.volume.clamp(0.0, 1.0) * 100.0) as u32;

    rsx! {
        div { style: "display:flex; flex-direction:column; gap:0; flex:1; min-height:0; color:#e4e4e7; font-family:sans-serif;",
            // ── top bar ──
            div { style: "display:flex; align-items:center; gap:10px; padding:6px 12px; border-bottom:1px solid #1c1c1f;",
                span { style: "font-weight:700; font-size:13px;", "Synth" }
                span { style: "font-size:11px; color:#a1a1aa;", {status.loaded_preset.clone().unwrap_or_else(|| "—".into())} }
                // PLAY / EDIT mode toggle
                div { style: "display:flex; gap:2px; margin-left:6px; background:#18181b; border:1px solid #27272a; border-radius:6px; padding:2px;",
                    button {
                        style: mode_btn(mode() == Mode::Play),
                        onclick: move |_| mode.set(Mode::Play),
                        "PLAY"
                    }
                    button {
                        style: mode_btn(mode() == Mode::Edit),
                        onclick: move |_| mode.set(Mode::Edit),
                        "EDIT"
                    }
                }
                div { style: "flex:1;" }
                // volume slider
                span { style: "font-size:10px; color:#71717a;", "VOL {vol_pct}%" }
                {
                    let rig_vol = rig.clone();
                    rsx!{ input {
                        r#type: "range",
                        min: "0",
                        max: "1000",
                        step: "10",
                        value: "{vol_milli}",
                        style: "width:96px; accent-color:#38bdf8;",
                        oninput: move |e| {
                            let rig = rig_vol.clone();
                            let v = e.value().parse::<u32>().unwrap_or(250);
                            spawn(async move { if let Some(r) = rig { let _ = r.set_volume(v).await; } });
                        },
                    } }
                }
                // master meter (red at clip)
                div { style: "width:80px; height:8px; background:#18181b; border-radius:2px; overflow:hidden;",
                    div { style: format!("height:100%; width:{master_pct}%; background:{};", if clipping { "#ef4444" } else { "#22c55e" }) }
                }
            }
            if mode() == Mode::Edit {
                MappingView {}
            } else {
            div { style: "display:flex; gap:12px; flex:1; min-height:0;",
                // ── preset browser (left) ──
                div { style: "display:flex; flex-direction:column; gap:4px; width:220px; min-width:220px; overflow:auto; border-right:1px solid #1c1c1f; padding:8px;",
                    span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Presets ({presets.len()})" }
                    for (i, preset) in presets.iter().enumerate() {
                        {
                            let rig = rig.clone();
                            rsx!{ button {
                                key: "{preset.name}",
                                style: preset_btn(preset.loaded),
                                onclick: move |_| { let rig = rig.clone(); spawn(async move { if let Some(r) = rig { let _ = r.load_preset(i as u32).await; } }); },
                                span { style: "font-size:12px; font-weight:600;", "{preset.name}" }
                                span { style: "font-size:9px; color:#71717a;", "{preset.kind}" }
                            } }
                        }
                    }
                }
                // ── MIDI monitor (is it coming through?) + keyboard ──
                div { style: "display:flex; flex-direction:column; gap:12px; flex:1; min-height:0; overflow:auto; padding:10px;",
                    // The live count is the quickest "is MIDI arriving?" signal.
                    div { style: "display:flex; align-items:baseline; gap:8px;",
                        span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "MIDI in" }
                        span { style: "font-size:11px; color:#52525b;", {status.midi_port.clone().unwrap_or_else(|| "omni (all inputs)".into())} }
                        div { style: "flex:1;" }
                        span { style: "font-size:12px; font-weight:700; color:#38bdf8;", "{midi_count} events" }
                    }
                    div { style: "flex:1; min-height:120px;",
                        MidiMonitorPanel { events: midi, count: midi_count, title: "MIDI monitor".to_string() }
                    }
                    // performance: piano
                    div {
                        span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Keyboard" }
                        {
                            let rig_on = rig.clone();
                            let rig_off = rig.clone();
                            rsx!{ Piano {
                                start_note: 21,
                                end_note: 108,
                                active_notes: lit,
                                show_labels: false,
                                waterfall: false,
                                accent_color: "#38bdf8".to_string(),
                                height: "150px",
                                on_note_on: move |n: u8| { let rig = rig_on.clone(); spawn(async move { if let Some(r) = rig { let _ = r.trigger(n as u32, 100).await; } }); },
                                on_note_off: move |n: u8| { let rig = rig_off.clone(); spawn(async move { if let Some(r) = rig { let _ = r.trigger(n as u32, 0).await; } }); },
                            } }
                        }
                    }
                }
            }
            }
        }
    }
}

fn mode_btn(active: bool) -> String {
    let (bg, fg) = if active { ("#0c2733", "#e4e4e7") } else { ("transparent", "#71717a") };
    format!("padding:3px 12px; border:none; border-radius:4px; background:{bg}; color:{fg}; font-size:10px; font-weight:700; letter-spacing:0.05em; cursor:pointer;")
}

fn preset_btn(loaded: bool) -> String {
    let (bg, br, fg) = if loaded { ("#0c2733", "#0ea5e9", "#e4e4e7") } else { ("#111113", "#27272a", "#a1a1aa") };
    format!("display:flex; flex-direction:column; text-align:left; padding:6px 8px; border-radius:6px; background:{bg}; color:{fg}; border:1px solid {br}; font-size:12px; cursor:pointer;")
}

// ── Mapping (keymap) editor ───────────────────────────────────────────────────

/// Sidebar filter — dims non-matching zones in the grid.
#[derive(Clone, PartialEq)]
enum Filter {
    All,
    Artic(String),
    Group(String),
}

/// Grid geometry (SVG user units; the viewBox is stretched to the container).
const GRID_W: f64 = 1280.0;
const GRID_H: f64 = 480.0;

/// One drawable zone rectangle (round-robin stripe) in the grid.
#[derive(Clone)]
struct RectDraw {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: String,
    zi: usize,
    dim: bool,
}

/// MIDI note number → scientific pitch name (60 = "C4").
fn note_name(n: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let oct = n as i32 / 12 - 1;
    format!("{}{}", NAMES[(n % 12) as usize], oct)
}

/// Which string a zone is colored by: articulation, then mic, then group, then file.
fn color_key(z: &SynthZone) -> String {
    if !z.articulation.is_empty() {
        z.articulation.clone()
    } else if !z.mic.is_empty() {
        z.mic.clone()
    } else if !z.group.is_empty() {
        z.group.clone()
    } else {
        z.file.clone()
    }
}

/// Stable hue from a label (FNV-1a) → an HSL fill.
fn color_for(key: &str) -> String {
    let mut h: u32 = 2166136261;
    for b in key.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    format!("hsl({}, 58%, 52%)", h % 360)
}

fn filter_matches(f: &Filter, z: &SynthZone) -> bool {
    match f {
        Filter::All => true,
        Filter::Artic(a) => &z.articulation == a,
        Filter::Group(g) => &z.group == g,
    }
}

/// KODA/Kontakt-style read-only keymap editor: soundsource selector, articulation
/// / group sidebar, a key×velocity zone grid (round-robins striped), a piano
/// aligned under the X axis, and a per-zone inspector.
#[component]
fn MappingView() -> Element {
    let rig = use_hook(try_consume_context::<SynthRigClient>);

    // Soundsource names (fetched once).
    let sources = {
        let rig = rig.clone();
        use_resource(move || {
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => r.soundsources().await.unwrap_or_default(),
                    None => Vec::new(),
                }
            }
        })
    };

    let mut selected_src = use_signal(String::new);
    let mut selected_zone = use_signal(|| Option::<usize>::None);
    let mut filter = use_signal(|| Filter::All);

    // Keymap for the selected soundsource (empty ⇒ first of the loaded preset).
    let mapping = {
        let rig = rig.clone();
        use_resource(move || {
            let sel = selected_src();
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => r.mapping(sel).await.unwrap_or_default(),
                    None => SynthMapping::default(),
                }
            }
        })
    };

    let src_list: Vec<String> = sources.read().clone().unwrap_or_default();
    let loading = mapping.read().is_none();
    let m: SynthMapping = mapping.read().clone().unwrap_or_default();
    let sel_src = selected_src();
    let effective_src = if sel_src.is_empty() {
        src_list.first().cloned().unwrap_or_default()
    } else {
        sel_src.clone()
    };

    let header_name = if m.name.is_empty() { effective_src.clone() } else { m.name.clone() };
    let zones = m.zones.clone();
    let fv = filter();
    let sel = selected_zone();

    // ── Build round-robin-aware rectangles ──
    // Group zones by their exact key×velocity window; members differing by
    // rr_index become vertical stripes so every RR slot is visible.
    let mut windows: std::collections::BTreeMap<(u8, u8, u8, u8), Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, z) in zones.iter().enumerate() {
        windows
            .entry((z.key_min, z.key_max, z.vel_min, z.vel_max))
            .or_default()
            .push(i);
    }
    let mut rects: Vec<RectDraw> = Vec::new();
    for ((kmin, kmax, vmin, vmax), idxs) in windows.iter() {
        let mut idxs = idxs.clone();
        idxs.sort_by_key(|&i| zones[i].rr_index);
        let n = idxs.len().max(1);
        let base_x = *kmin as f64 / 128.0 * GRID_W;
        let base_w = (*kmax as f64 - *kmin as f64 + 1.0) / 128.0 * GRID_W;
        let y = (127.0 - *vmax as f64) / 128.0 * GRID_H;
        let h = ((*vmax as f64 - *vmin as f64 + 1.0) / 128.0 * GRID_H).max(2.0);
        let stripe_w = base_w / n as f64;
        for (slot, &zi) in idxs.iter().enumerate() {
            let z = &zones[zi];
            rects.push(RectDraw {
                x: base_x + slot as f64 * stripe_w,
                y,
                w: (stripe_w - if n > 1 { 1.0 } else { 0.0 }).max(0.5),
                h,
                color: color_for(&color_key(z)),
                zi,
                dim: !filter_matches(&fv, z),
            });
        }
    }

    // Keys with (matching) zones tint the piano; the selected zone lights up.
    let mapped_keys: Vec<u8> = {
        let mut hs = std::collections::BTreeSet::<u8>::new();
        for z in zones.iter() {
            if filter_matches(&fv, z) {
                for k in z.key_min..=z.key_max {
                    hs.insert(k);
                }
            }
        }
        hs.into_iter().collect()
    };
    let active_keys: Vec<u8> = sel
        .and_then(|i| zones.get(i))
        .map(|z| (z.key_min..=z.key_max).collect())
        .unwrap_or_default();

    // Inspector rows for the selected zone.
    let dash = |s: &str| if s.is_empty() { "—".to_string() } else { s.to_string() };
    let sel_zone = sel.and_then(|i| zones.get(i).cloned());
    let rows: Vec<(&'static str, String)> = match &sel_zone {
        Some(z) => vec![
            ("File", z.file.clone()),
            ("Key range", format!("{} – {}", note_name(z.key_min), note_name(z.key_max))),
            ("Root", note_name(z.root_key)),
            ("Velocity", format!("{} – {}", z.vel_min, z.vel_max)),
            (
                "Round-robin",
                format!(
                    "#{}{}",
                    z.rr_index,
                    if z.rr_mode.is_empty() { String::new() } else { format!(" ({})", z.rr_mode) }
                ),
            ),
            ("Gain", format!("{:+.1} dB", z.gain_db)),
            ("Pan", format!("{:+.2}", z.pan)),
            ("Tune", format!("{:+.0} cents", z.tune_cents)),
            (
                "Loop",
                if z.loop_end > z.loop_start {
                    format!("{} – {}", z.loop_start, z.loop_end)
                } else {
                    "—".to_string()
                },
            ),
            ("Trigger", dash(&z.trigger_mode)),
            ("Mic", dash(&z.mic)),
            ("Articulation", dash(&z.articulation)),
            ("Dynamic", dash(&z.dynamic)),
            ("Group", dash(&z.group)),
            ("Variant", dash(&z.variant)),
        ],
        None => Vec::new(),
    };

    rsx! {
        div { style: "display:flex; gap:0; flex:1; min-height:0;",
            // ── left sidebar: articulations + groups ──
            div { style: "display:flex; flex-direction:column; gap:2px; width:200px; min-width:200px; overflow:auto; border-right:1px solid #1c1c1f; padding:8px;",
                span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Articulations" }
                button {
                    style: sidebar_btn(matches!(fv, Filter::All)),
                    onclick: move |_| filter.set(Filter::All),
                    "All zones ({zones.len()})"
                }
                for a in m.articulations.iter() {
                    {
                        let id = a.id.clone();
                        let is_sel = matches!(&fv, Filter::Artic(x) if x == &id);
                        let count = zones.iter().filter(|z| z.articulation == id).count();
                        rsx!{ button {
                            key: "{a.id}",
                            style: sidebar_btn(is_sel),
                            onclick: move |_| filter.set(Filter::Artic(id.clone())),
                            span { style: "flex:1;", "{a.label}" }
                            span { style: "color:#52525b;", "{count}" }
                        } }
                    }
                }
                if !m.groups.is_empty() {
                    span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em; margin-top:10px;", "Groups" }
                    for g in m.groups.iter() {
                        {
                            let id = g.clone();
                            let is_sel = matches!(&fv, Filter::Group(x) if x == &id);
                            let count = zones.iter().filter(|z| &z.group == &id).count();
                            rsx!{ button {
                                key: "{g}",
                                style: sidebar_btn(is_sel),
                                onclick: move |_| filter.set(Filter::Group(id.clone())),
                                span { style: "flex:1;", "{g}" }
                                span { style: "color:#52525b;", "{count}" }
                            } }
                        }
                    }
                }
            }

            // ── center: soundsource tabs + grid + piano ──
            div { style: "display:flex; flex-direction:column; gap:8px; flex:1; min-width:0; padding:10px; overflow:auto;",
                // soundsource selector
                div { style: "display:flex; flex-wrap:wrap; gap:4px; align-items:center;",
                    span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em; margin-right:4px;", "Soundsource" }
                    if src_list.is_empty() {
                        span { style: "font-size:11px; color:#52525b;", "none loaded" }
                    }
                    for name in src_list.iter() {
                        {
                            let nm = name.clone();
                            let is_sel = &effective_src == name;
                            rsx!{ button {
                                key: "{name}",
                                style: src_tab(is_sel),
                                onclick: move |_| { selected_src.set(nm.clone()); selected_zone.set(None); filter.set(Filter::All); },
                                "{name}"
                            } }
                        }
                    }
                }
                // header
                div { style: "display:flex; align-items:baseline; gap:10px;",
                    span { style: "font-size:13px; font-weight:700; color:#e4e4e7;", "{header_name}" }
                    if !m.vendor.is_empty() {
                        span { style: "font-size:10px; color:#71717a;", "{m.vendor}" }
                    }
                    span { style: "font-size:10px; color:#52525b;", "{zones.len()} zones" }
                    if loading {
                        span { style: "font-size:10px; color:#38bdf8;", "loading…" }
                    }
                }

                // grid
                div { style: "position:relative; width:100%; height:340px; background:#09090b; border:1px solid #1c1c1f; border-radius:6px; overflow:hidden;",
                    svg {
                        width: "100%",
                        height: "100%",
                        view_box: "0 0 1280 480",
                        preserve_aspect_ratio: "none",
                        style: "display:block;",
                        // velocity rows
                        for v in [0u8, 32, 64, 96, 127] {
                            {
                                let y = (127.0 - v as f64) / 128.0 * GRID_H;
                                rsx!{ line { key: "v{v}", x1: "0", y1: "{y:.1}", x2: "1280", y2: "{y:.1}", stroke: "#18181b", stroke_width: "0.5" } }
                            }
                        }
                        // octave (C) gridlines
                        for oct in 0u8..11 {
                            {
                                let k = oct * 12;
                                let x = k as f64 / 128.0 * GRID_W;
                                rsx!{ line { key: "k{k}", x1: "{x:.1}", y1: "0", x2: "{x:.1}", y2: "480", stroke: "#27272a", stroke_width: "0.6" } }
                            }
                        }
                        // zone rectangles (round-robin stripes)
                        for r in rects.iter().cloned() {
                            {
                                let zi = r.zi;
                                let is_sel = sel == Some(zi);
                                let op = if r.dim { "0.12" } else if is_sel { "0.92" } else { "0.55" };
                                let stroke = if is_sel { "#38bdf8" } else { "#09090b" };
                                let sw = if is_sel { "2.5" } else { "0.5" };
                                rsx!{ rect {
                                    key: "{zi}",
                                    x: "{r.x:.1}",
                                    y: "{r.y:.1}",
                                    width: "{r.w:.1}",
                                    height: "{r.h:.1}",
                                    fill: "{r.color}",
                                    fill_opacity: "{op}",
                                    stroke: "{stroke}",
                                    stroke_width: "{sw}",
                                    style: "cursor:pointer;",
                                    onclick: move |_| selected_zone.set(Some(zi)),
                                } }
                            }
                        }
                    }
                }
                // axis hint
                div { style: "display:flex; justify-content:space-between; font-size:9px; color:#52525b;",
                    span { "key 0 (C-1)" }
                    span { "velocity 127 top → 0 bottom" }
                    span { "127 (G9)" }
                }
                // piano aligned under the grid X axis
                Piano {
                    start_note: 0,
                    end_note: 127,
                    active_notes: active_keys,
                    highlight_keys: mapped_keys,
                    show_labels: false,
                    waterfall: false,
                    accent_color: "#38bdf8".to_string(),
                    height: "90px",
                    on_note_on: move |_n: u8| {},
                    on_note_off: move |_n: u8| {},
                }
            }

            // ── right: inspector ──
            div { style: "display:flex; flex-direction:column; gap:2px; width:260px; min-width:260px; overflow:auto; border-left:1px solid #1c1c1f; padding:10px;",
                span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Inspector" }
                if sel_zone.is_none() {
                    span { style: "font-size:11px; color:#52525b; margin-top:8px;", "Click a zone in the grid to inspect it." }
                }
                for (k, v) in rows.iter() {
                    div {
                        key: "{k}",
                        style: "display:flex; gap:8px; padding:4px 0; border-bottom:1px solid #141416;",
                        span { style: "font-size:10px; color:#71717a; width:96px; min-width:96px;", "{k}" }
                        span { style: "font-size:11px; color:#e4e4e7; word-break:break-all;", "{v}" }
                    }
                }
            }
        }
    }
}

fn sidebar_btn(active: bool) -> String {
    let (bg, br, fg) = if active { ("#0c2733", "#0ea5e9", "#e4e4e7") } else { ("transparent", "#1c1c1f", "#a1a1aa") };
    format!("display:flex; align-items:center; gap:6px; text-align:left; padding:5px 8px; border-radius:5px; background:{bg}; color:{fg}; border:1px solid {br}; font-size:11px; cursor:pointer;")
}

fn src_tab(active: bool) -> String {
    let (bg, br, fg) = if active { ("#0c2733", "#0ea5e9", "#e4e4e7") } else { ("#111113", "#27272a", "#a1a1aa") };
    format!("padding:4px 10px; border-radius:5px; background:{bg}; color:{fg}; border:1px solid {br}; font-size:11px; font-weight:600; cursor:pointer;")
}
