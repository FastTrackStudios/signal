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
use signal_synth_proto::{
    SynthEnvelope, SynthFilter, SynthLayer, SynthMapping, SynthNode, SynthPreset, SynthStatus,
    SynthZone,
};
use signal_ui::components::Piano;

/// Which top-level view the remote is showing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Live performance: preset browser + MIDI monitor + keyboard.
    Play,
    /// The sampler **Setup** — the KODA-style keymap / zone editor.
    Setup,
    /// The **Edit** page — the Vital-style single-layer editor (A/B/C/D:
    /// source, filter, envelopes, LFOs, modulation).
    Edit,
    // (A top-down **Control** view over the whole rig comes later.)
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
                // PLAY / SETUP / EDIT mode toggle
                div { style: "display:flex; gap:2px; margin-left:6px; background:#18181b; border:1px solid #27272a; border-radius:6px; padding:2px;",
                    button {
                        style: mode_btn(mode() == Mode::Play),
                        onclick: move |_| mode.set(Mode::Play),
                        "PLAY"
                    }
                    button {
                        style: mode_btn(mode() == Mode::Setup),
                        onclick: move |_| mode.set(Mode::Setup),
                        "SETUP"
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
            if mode() == Mode::Setup {
                MappingView {}
            } else if mode() == Mode::Edit {
                LayerEditView {}
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

// ── Edit page (Vital-style single-layer editor) ───────────────────────────────
//
// Everything is inline-styled SVG (Blitz-safe) drawn from `SynthLayer` data
// pulled over the wire (`SynthRigClient::layers`). Edits are LOCAL only — they
// mutate a `use_signal<Vec<SynthLayer>>` seeded from the fetched layers and
// redraw live; no proto setters exist yet, so nothing is persisted back to the
// engine. Seeding: an effect copies the fetched layers into the edit buffer
// whenever their identity (names+sources) changes, so loading a new preset
// reseeds but a live drag is never clobbered.

// Filter-response graph geometry (viewBox user units).
const FILT_W: f64 = 520.0;
const FILT_H: f64 = 230.0;
const FMIN: f64 = 20.0;
const FMAX: f64 = 20000.0;
const DBMAX: f64 = 18.0;
const DBMIN: f64 = -48.0;

// Envelope graph geometry.
const ENV_W: f64 = 520.0;
const ENV_H: f64 = 132.0;
const ENV_TOP: f64 = 12.0;
const ENV_BOT: f64 = 12.0;
const ENV_LEFT: f64 = 6.0;
const ENV_RIGHT: f64 = 8.0;

fn freq_to_x(f: f64) -> f64 {
    let (lm, lx) = (FMIN.log10(), FMAX.log10());
    ((f.max(FMIN).log10() - lm) / (lx - lm)) * FILT_W
}
fn x_to_freq(x: f64) -> f64 {
    let (lm, lx) = (FMIN.log10(), FMAX.log10());
    10f64.powf(lm + (x / FILT_W).clamp(0.0, 1.0) * (lx - lm))
}
fn db_to_y(db: f64) -> f64 {
    ((DBMAX - db.clamp(DBMIN, DBMAX)) / (DBMAX - DBMIN)) * FILT_H
}
fn fmt_hz(f: f64) -> String {
    if f >= 1000.0 {
        format!("{:.1}k", f / 1000.0)
    } else {
        format!("{:.0}", f)
    }
}

/// Analytic filter magnitude (dB) at frequency `f`. A resonant 2-pole analog
/// prototype (peak = Q at cutoff) with extra 1-pole factors for steeper slopes
/// — enough to *see* the shape, not a sample-accurate response.
fn filter_mag_db(mode: &str, cutoff: f64, res: f64, poles: u32, f: f64) -> f64 {
    let fc = cutoff.max(1.0);
    let w = f / fc;
    let q = 0.5 + res.clamp(0.0, 1.0) * 8.0; // 0.5 … 8.5
    let denom2 = |w: f64| {
        let a = 1.0 - w * w;
        (a * a + (w / q) * (w / q)).sqrt().max(1e-6)
    };
    let extra = poles.saturating_sub(2);
    let lp_extra = |w: f64| (1.0 / (1.0 + w * w).sqrt()).powi(extra as i32);
    let hp_extra = |w: f64| (w / (1.0 + w * w).sqrt()).powi(extra as i32);
    let lin = match mode {
        "highpass" => {
            if poles <= 1 {
                w / (1.0 + w * w).sqrt()
            } else {
                (w * w) / denom2(w) * hp_extra(w)
            }
        }
        "bandpass" => (w / q) / denom2(w),
        "notch" => (1.0 - w * w).abs() / denom2(w),
        _ => {
            if poles <= 1 {
                1.0 / (1.0 + w * w).sqrt()
            } else {
                1.0 / denom2(w) * lp_extra(w)
            }
        }
    };
    20.0 * lin.max(1e-6).log10()
}

/// The filter response as an SVG stroke path across the log-freq axis.
fn filter_path(f: &SynthFilter) -> String {
    let n = 96usize;
    let mut s = String::new();
    for i in 0..n {
        let t = i as f64 / (n - 1) as f64;
        let freq = 10f64.powf(FMIN.log10() + t * (FMAX.log10() - FMIN.log10()));
        let db = filter_mag_db(
            &f.mode,
            f.cutoff_hz as f64,
            f.resonance as f64,
            f.poles,
            freq,
        );
        let (x, y) = (freq_to_x(freq), db_to_y(db));
        if i == 0 {
            s.push_str(&format!("M{x:.1} {y:.1}"));
        } else {
            s.push_str(&format!(" L{x:.1} {y:.1}"));
        }
    }
    s
}

/// Derived DAHDSR geometry for one envelope, in the env viewBox.
struct EnvGeom {
    sec_per_x: f64,
    xa: f64,
    xc: f64,
    xh: f64,
    xr: f64,
    ysus: f64,
    y0: f64,
    ytop: f64,
    stroke: String,
    fill: String,
}

fn env_geom(e: &SynthEnvelope) -> EnvGeom {
    let a = (e.attack.max(0.0)) as f64;
    let d = (e.decay.max(0.0)) as f64;
    let r = (e.release.max(0.0)) as f64;
    let sus = (e.sustain.clamp(0.0, 1.0)) as f64;
    // A visual sustain-hold segment so the S corner has room; scales with the
    // envelope so short and long envelopes both read.
    let hold = 0.25 * (a + d + r).max(0.08) + 0.05;
    let total = (a + d + hold + r).max(0.1);
    let usable_w = ENV_W - ENV_LEFT - ENV_RIGHT;
    let sec_per_x = total / usable_w;
    let xa = ENV_LEFT + a / sec_per_x;
    let xc = xa + d / sec_per_x;
    let xh = xc + hold / sec_per_x;
    let xr = xh + r / sec_per_x;
    let usable_h = ENV_H - ENV_TOP - ENV_BOT;
    let ytop = ENV_TOP;
    let ysus = ENV_TOP + (1.0 - sus) * usable_h;
    let y0 = ENV_TOP + usable_h;
    let stroke = format!(
        "M{ENV_LEFT:.1} {y0:.1} L{xa:.1} {ytop:.1} L{xc:.1} {ysus:.1} L{xh:.1} {ysus:.1} L{xr:.1} {y0:.1}"
    );
    let fill = format!("{stroke} L{ENV_LEFT:.1} {y0:.1} Z");
    EnvGeom {
        sec_per_x,
        xa,
        xc,
        xh,
        xr,
        ysus,
        y0,
        ytop,
        stroke,
        fill,
    }
}

/// Which draggable handle is live.
#[derive(Clone, Copy, PartialEq)]
enum Handle {
    /// Filter cutoff dot: X = cutoff (log freq), Y = resonance.
    Cutoff,
    /// Amp/Filter envelope: attack peak (X = attack), corner (X = decay, Y =
    /// sustain), release end (X = release).
    AmpAttack,
    AmpCorner,
    AmpRelease,
    FiltAttack,
    FiltCorner,
    FiltRelease,
}

/// In-flight drag: the target handle + the cached client rect / viewBox dims of
/// the SVG it lives in, so pointer-move (on a full-window shield) can map global
/// coords into graph space. `sec_per_x` + `base_x` freeze the env time-scale for
/// the duration of a time drag so it stays linear.
#[derive(Clone, Copy)]
struct Drag {
    handle: Handle,
    ox: f64,
    oy: f64,
    w: f64,
    h: f64,
    vb_w: f64,
    vb_h: f64,
    sec_per_x: f64,
    base_x: f64,
}

fn arc_pt(cx: f64, cy: f64, r: f64, deg: f64) -> (f64, f64) {
    let rad = deg * std::f64::consts::PI / 180.0;
    (cx + r * rad.cos(), cy + r * rad.sin())
}
fn knob_arc(cx: f64, cy: f64, r: f64, a0: f64, a1: f64) -> String {
    let (x1, y1) = arc_pt(cx, cy, r, a0);
    let (x2, y2) = arc_pt(cx, cy, r, a1);
    let large = if (a1 - a0).abs() > 180.0 { 1 } else { 0 };
    format!("M {x1:.1} {y1:.1} A {r:.1} {r:.1} 0 {large} 1 {x2:.1} {y2:.1}")
}

/// Small inline-styled SVG arc knob (Blitz-safe, no Tailwind, no external deps).
/// A transparent range input over the arc handles the drag; `value` is the
/// normalized 0..1 position and `on_change` fires the new value.
#[component]
fn MiniKnob(value: f64, label: String, display: String, color: String, on_change: Callback<f64>) -> Element {
    let (cx, cy, r) = (22.0, 22.0, 18.0);
    let (start, sweep) = (135.0, 270.0);
    let val = value.clamp(0.0, 1.0);
    let track = knob_arc(cx, cy, r, start, start + sweep);
    let end = start + val * sweep;
    let filled = if val > 0.001 {
        knob_arc(cx, cy, r, start, end)
    } else {
        String::new()
    };
    let (tx, ty) = arc_pt(cx, cy, r - 6.0, end);
    let (tx2, ty2) = arc_pt(cx, cy, r + 1.0, end);
    rsx! {
        div { style: "position:relative; display:inline-flex; flex-direction:column; align-items:center; gap:2px; width:54px;",
            div { style: "position:relative; width:44px; height:44px;",
                svg { width: "44", height: "44", view_box: "0 0 44 44", style: "display:block;",
                    path { d: "{track}", fill: "none", stroke: "#27272a", stroke_width: "3.5", stroke_linecap: "round" }
                    if !filled.is_empty() {
                        path { d: "{filled}", fill: "none", stroke: "{color}", stroke_width: "4", stroke_linecap: "round" }
                    }
                    line { x1: "{tx:.1}", y1: "{ty:.1}", x2: "{tx2:.1}", y2: "{ty2:.1}", stroke: "#e4e4e7", stroke_width: "2", stroke_linecap: "round" }
                }
                input {
                    r#type: "range",
                    min: "0",
                    max: "1",
                    step: "0.005",
                    value: "{val}",
                    style: "position:absolute; inset:0; width:100%; height:100%; opacity:0; cursor:pointer; margin:0;",
                    oninput: move |ev: FormEvent| {
                        if let Ok(v) = ev.value().parse::<f64>() {
                            on_change.call(v.clamp(0.0, 1.0));
                        }
                    },
                }
            }
            span { style: "font-size:10px; color:#e4e4e7; font-family:monospace;", "{display}" }
            span { style: "font-size:9px; color:#71717a; text-transform:uppercase; letter-spacing:0.04em;", "{label}" }
        }
    }
}

/// The **Edit** page — the Vital-style single-layer editor. A/B/C/D selector
/// across the top; for the picked layer: source + level, an interactive filter
/// response curve, amp + filter ADSR graphs, and the modulation routes.
#[component]
fn LayerEditView() -> Element {
    let rig = use_hook(try_consume_context::<SynthRigClient>);

    // The loaded preset's four layers (fetched once; re-fetched on preset change
    // via the resource's own reactivity is not wired — a manual reload button
    // could call this again later).
    let layers_res = {
        let rig = rig.clone();
        use_resource(move || {
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => r.layers().await.unwrap_or_default(),
                    None => Vec::new(),
                }
            }
        })
    };

    // Local editable copy + selection. Seeded from the fetched layers whenever
    // their identity changes (new preset) — never mid-drag, since the key only
    // shifts on a real reload.
    let mut edit = use_signal(Vec::<SynthLayer>::new);
    let mut seeded = use_signal(String::new);
    let mut sel = use_signal(|| 0usize);
    let mut drag = use_signal(|| None::<Drag>);
    // SVG element handles, for mapping client coords → viewBox on drag.
    let mut filt_svg = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let amp_svg = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let fenv_svg = use_signal(|| None::<std::rc::Rc<MountedData>>);

    use_effect(move || {
        let fetched = layers_res.read().clone().unwrap_or_default();
        let key = fetched
            .iter()
            .map(|l| format!("{}~{}", l.name, l.source))
            .collect::<Vec<_>>()
            .join("|");
        if key != seeded.peek().clone() {
            seeded.set(key);
            // Default selection: first active layer.
            let first = fetched.iter().position(|l| l.active).unwrap_or(0);
            sel.set(first);
            edit.set(fetched);
        }
    });

    // Start a drag: cache the target SVG's client rect (async) + freeze the env
    // time-scale, then arm the shield. A `Callback` so the envelope sub-graphs
    // can arm the shared drag state through context.
    type BeginArgs = (
        Signal<Option<std::rc::Rc<MountedData>>>,
        Handle,
        f64,
        f64,
        f64,
        f64,
    );
    let begin = use_callback(move |(svg, handle, vb_w, vb_h, sec_per_x, base_x): BeginArgs| {
        let mut drag = drag;
        spawn(async move {
            let Some(el) = svg.peek().clone() else { return };
            if let Ok(rect) = el.get_client_rect().await {
                drag.set(Some(Drag {
                    handle,
                    ox: rect.origin.x,
                    oy: rect.origin.y,
                    w: rect.width(),
                    h: rect.height(),
                    vb_w,
                    vb_h,
                    sec_per_x,
                    base_x,
                }));
            }
        });
    });
    use_context_provider(|| EnvHooks { amp_svg, fenv_svg, begin });

    let layers = edit.read().clone();
    if layers.is_empty() {
        return rsx! {
            div { style: "display:flex; align-items:center; justify-content:center; flex:1; color:#52525b; font-size:13px;",
                "No layers — load a preset."
            }
        };
    }
    let li = sel().min(layers.len() - 1);
    let layer = layers[li].clone();
    let filter = layer.filters.first().cloned().unwrap_or_default();

    let filt_stroke = filter_path(&filter);
    let filt_fill = format!("{filt_stroke} L{FILT_W:.1} {FILT_H:.1} L0 {FILT_H:.1} Z");
    let cut_x = freq_to_x(filter.cutoff_hz as f64);
    let cut_db = filter_mag_db(
        &filter.mode,
        filter.cutoff_hz as f64,
        filter.resonance as f64,
        filter.poles,
        filter.cutoff_hz as f64,
    );
    let cut_y = db_to_y(cut_db);
    // Modulated-cutoff ghost: where the filter envelope pushes the cutoff
    // (env_depth as octaves of upward sweep) — Vital's "you can see the mod".
    let mod_cut = (filter.cutoff_hz as f64) * 2f64.powf((filter.env_depth as f64) * 4.0);
    let mod_x = freq_to_x(mod_cut.clamp(FMIN, FMAX));
    let has_mod = filter.env_depth.abs() > 0.001;

    // Cutoff-knob normalized position (log).
    let cut_norm =
        ((filter.cutoff_hz as f64).max(FMIN).log10() - FMIN.log10()) / (FMAX.log10() - FMIN.log10());

    rsx! {
        div { style: "display:flex; flex-direction:column; gap:0; flex:1; min-height:0;",

            // ── layer selector + source + level ──
            div { style: "display:flex; align-items:center; gap:10px; padding:8px 12px; border-bottom:1px solid #1c1c1f;",
                div { style: "display:flex; gap:4px;",
                    for (i, l) in layers.iter().enumerate() {
                        {
                            let active = l.active;
                            let is_sel = i == li;
                            let letter = ["A", "B", "C", "D"].get(i).copied().unwrap_or("?");
                            rsx! { button {
                                key: "{i}",
                                style: layer_btn(is_sel, active),
                                disabled: !active,
                                onclick: move |_| if active { sel.set(i); },
                                "{letter}"
                            } }
                        }
                    }
                }
                div { style: "display:flex; flex-direction:column; gap:1px; margin-left:6px;",
                    span { style: "font-size:9px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Source" }
                    span { style: "font-size:13px; font-weight:700; color:#e4e4e7;",
                        {if layer.source.is_empty() { "—".to_string() } else { layer.source.clone() }}
                    }
                }
                div { style: "flex:1;" }
                {
                    let level = layer.level as f64;
                    rsx! { MiniKnob {
                        value: level,
                        label: "level".to_string(),
                        display: format!("{:.0}%", level * 100.0),
                        color: "#38bdf8".to_string(),
                        on_change: move |v: f64| { edit.with_mut(|ls| { if let Some(l) = ls.get_mut(li) { l.level = v as f32; } }); },
                    } }
                }
            }

            // ── body: filter | envelopes+mod ──
            div { style: "display:flex; gap:0; flex:1; min-height:0;",

                // ── FILTER (response curve) ──
                div { style: "display:flex; flex-direction:column; gap:8px; flex:1; min-width:0; padding:12px; border-right:1px solid #1c1c1f; overflow:auto;",
                    div { style: "display:flex; align-items:baseline; gap:8px;",
                        span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.06em;", "Filter" }
                        span { style: "font-size:10px; color:#52525b;", "{filter.mode} · {filter.poles}-pole" }
                        div { style: "flex:1;" }
                        span { style: "font-size:11px; color:#38bdf8; font-family:monospace;", "{fmt_hz(filter.cutoff_hz as f64)} Hz  ·  res {(filter.resonance*100.0) as i32}%" }
                    }
                    div { style: "position:relative; width:100%; height:230px; background:#09090b; border:1px solid #1c1c1f; border-radius:6px; overflow:hidden;",
                        svg {
                            width: "100%",
                            height: "100%",
                            view_box: "0 0 520 230",
                            preserve_aspect_ratio: "none",
                            style: "display:block; touch-action:none;",
                            onmounted: move |e| filt_svg.set(Some(e.data())),

                            // grid: 0 dB line + decade freq lines
                            {
                                let y0 = db_to_y(0.0);
                                rsx! { line { x1: "0", y1: "{y0:.1}", x2: "520", y2: "{y0:.1}", stroke: "#27272a", stroke_width: "0.6" } }
                            }
                            for f in [100.0, 1000.0, 10000.0] {
                                {
                                    let x = freq_to_x(f);
                                    rsx! { line { key: "g{f}", x1: "{x:.1}", y1: "0", x2: "{x:.1}", y2: "230", stroke: "#18181b", stroke_width: "0.6" } }
                                }
                            }
                            for (f, lbl) in [(100.0, "100"), (1000.0, "1k"), (10000.0, "10k")] {
                                {
                                    let x = freq_to_x(f);
                                    rsx! { text { key: "t{lbl}", x: "{x+3.0:.1}", y: "224", fill: "#3f3f46", font_size: "9", "{lbl}" } }
                                }
                            }

                            // response fill + stroke
                            path { d: "{filt_fill}", fill: "#38bdf815", stroke: "none" }
                            path { d: "{filt_stroke}", fill: "none", stroke: "#38bdf8", stroke_width: "2" }

                            // modulated-cutoff ghost (filter env → cutoff)
                            if has_mod {
                                line { x1: "{mod_x:.1}", y1: "0", x2: "{mod_x:.1}", y2: "230", stroke: "#a78bfa", stroke_width: "1", stroke_dasharray: "3 3", opacity: "0.7" }
                                text { x: "{mod_x+3.0:.1}", y: "12", fill: "#a78bfa", font_size: "8", "filt env → cutoff" }
                            }

                            // draggable cutoff handle (X=cutoff, Y=resonance)
                            circle {
                                cx: "{cut_x:.1}",
                                cy: "{cut_y:.1}",
                                r: "7",
                                fill: "#38bdf8",
                                fill_opacity: "0.9",
                                stroke: "#e4e4e7",
                                stroke_width: "1.5",
                                style: "cursor:grab;",
                                onpointerdown: move |_| begin.call((filt_svg, Handle::Cutoff, FILT_W, FILT_H, 0.0, 0.0)),
                            }
                        }
                    }
                    // filter knobs
                    div { style: "display:flex; gap:14px; align-items:flex-start; padding-top:4px;",
                        MiniKnob {
                            value: cut_norm,
                            label: "cutoff".to_string(),
                            display: format!("{} Hz", fmt_hz(filter.cutoff_hz as f64)),
                            color: "#38bdf8".to_string(),
                            on_change: move |v: f64| {
                                let f = 10f64.powf(FMIN.log10() + v * (FMAX.log10() - FMIN.log10()));
                                edit.with_mut(|ls| { if let Some(l) = ls.get_mut(li) { if let Some(fl) = l.filters.first_mut() { fl.cutoff_hz = f as f32; } } });
                            },
                        }
                        MiniKnob {
                            value: filter.resonance as f64,
                            label: "res".to_string(),
                            display: format!("{}%", (filter.resonance * 100.0) as i32),
                            color: "#38bdf8".to_string(),
                            on_change: move |v: f64| { edit.with_mut(|ls| { if let Some(l) = ls.get_mut(li) { if let Some(fl) = l.filters.first_mut() { fl.resonance = v as f32; } } }); },
                        }
                        span { style: "font-size:9px; color:#52525b; max-width:180px; line-height:1.4;",
                            "Drag the dot: ← → cutoff, ↑ ↓ resonance."
                        }
                    }
                }

                // ── ENVELOPES + MODULATION ──
                div { style: "display:flex; flex-direction:column; gap:10px; width:560px; min-width:340px; padding:12px; overflow:auto;",

                    EnvBlock {
                        title: "Amp Env".to_string(),
                        env: layer.amp_env.clone(),
                        color: "#22c55e".to_string(),
                    }
                    EnvBlock {
                        title: "Filter Env".to_string(),
                        env: layer.filter_env.clone(),
                        color: "#a78bfa".to_string(),
                    }

                    // modulation routes
                    div {
                        span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.06em;", "Modulation ({layer.routes.len()})" }
                        div { style: "display:flex; flex-direction:column; gap:3px; margin-top:6px;",
                            if layer.routes.is_empty() {
                                span { style: "font-size:11px; color:#52525b;", "No routes in this layer." }
                            }
                            for (ri, route) in layer.routes.iter().enumerate() {
                                {
                                    let pct = (route.depth * 100.0) as i32;
                                    let to_cut = route.source.to_lowercase().contains("env")
                                        && (route.target.to_lowercase().contains("cut") || route.target.to_lowercase().contains("freq"));
                                    rsx! { div {
                                        key: "{ri}",
                                        style: "display:flex; align-items:center; gap:8px; padding:5px 8px; background:#111113; border:1px solid #1c1c1f; border-radius:5px;",
                                        span { style: "font-size:11px; color:#e4e4e7; font-weight:600;", "{route.source}" }
                                        span { style: "font-size:11px; color:#52525b;", "→" }
                                        span { style: "font-size:11px; color:#a1a1aa;", "{route.target}" }
                                        if to_cut { span { style: "font-size:8px; color:#a78bfa; border:1px solid #a78bfa55; border-radius:3px; padding:0 3px;", "cutoff" } }
                                        div { style: "flex:1;" }
                                        span { style: "font-size:11px; color:#38bdf8; font-family:monospace;", "{pct}%" }
                                    } }
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── drag shield: tracks pointer across the whole window ──
        if drag.read().is_some() {
            div {
                style: "position:fixed; inset:0; z-index:1000; cursor:grabbing;",
                onpointermove: move |e: PointerEvent| {
                    let Some(d) = drag.peek().as_ref().copied() else { return };
                    let c = e.client_coordinates();
                    let vb_x = (c.x - d.ox) / d.w * d.vb_w;
                    let vb_y = (c.y - d.oy) / d.h * d.vb_h;
                    edit.with_mut(|ls| {
                        let Some(l) = ls.get_mut(li) else { return };
                        match d.handle {
                            Handle::Cutoff => {
                                if let Some(f) = l.filters.first_mut() {
                                    f.cutoff_hz = x_to_freq(vb_x).clamp(FMIN, FMAX) as f32;
                                    f.resonance = (1.0 - (vb_y / d.vb_h)).clamp(0.0, 1.0) as f32;
                                }
                            }
                            Handle::AmpAttack => { l.amp_env.attack = ((vb_x - d.base_x) * d.sec_per_x).clamp(0.0, 20.0) as f32; }
                            Handle::AmpCorner => {
                                l.amp_env.decay = ((vb_x - d.base_x) * d.sec_per_x).clamp(0.0, 20.0) as f32;
                                let usable = ENV_H - ENV_TOP - ENV_BOT;
                                l.amp_env.sustain = (1.0 - ((vb_y - ENV_TOP) / usable)).clamp(0.0, 1.0) as f32;
                            }
                            Handle::AmpRelease => { l.amp_env.release = ((vb_x - d.base_x) * d.sec_per_x).clamp(0.0, 20.0) as f32; }
                            Handle::FiltAttack => { l.filter_env.attack = ((vb_x - d.base_x) * d.sec_per_x).clamp(0.0, 20.0) as f32; }
                            Handle::FiltCorner => {
                                l.filter_env.decay = ((vb_x - d.base_x) * d.sec_per_x).clamp(0.0, 20.0) as f32;
                                let usable = ENV_H - ENV_TOP - ENV_BOT;
                                l.filter_env.sustain = (1.0 - ((vb_y - ENV_TOP) / usable)).clamp(0.0, 1.0) as f32;
                            }
                            Handle::FiltRelease => { l.filter_env.release = ((vb_x - d.base_x) * d.sec_per_x).clamp(0.0, 20.0) as f32; }
                        }
                    });
                },
                onpointerup: move |_| drag.set(None),
            }
        }
    }
}

/// One envelope graph (amp or filter) with three draggable markers. Reads its
/// SVG ref + drag arming from the parent via context: to keep this self-
/// contained we re-derive geometry and re-fetch the rect on each grab.
#[component]
fn EnvBlock(title: String, env: SynthEnvelope, color: String) -> Element {
    // The parent owns the edit buffer + drag state; EnvBlock reaches them from
    // context. To avoid threading callbacks, the parent exposes an `EnvHooks`
    // bundle in context.
    let hooks = use_context::<EnvHooks>();
    let is_amp = title.starts_with("Amp");
    let g = env_geom(&env);
    let svg_ref = if is_amp { hooks.amp_svg } else { hooks.fenv_svg };
    let (h_attack, h_corner, h_release) = if is_amp {
        (Handle::AmpAttack, Handle::AmpCorner, Handle::AmpRelease)
    } else {
        (Handle::FiltAttack, Handle::FiltCorner, Handle::FiltRelease)
    };
    let begin = hooks.begin;
    let (xa, xc, xh, xr) = (g.xa, g.xc, g.xh, g.xr);
    let (ytop, ysus, y0, spx) = (g.ytop, g.ysus, g.y0, g.sec_per_x);

    rsx! {
        div {
            div { style: "display:flex; align-items:baseline; gap:8px;",
                span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.06em;", "{title}" }
                div { style: "flex:1;" }
                span { style: "font-size:10px; color:#52525b; font-family:monospace;",
                    "A {env.attack:.2}s  D {env.decay:.2}s  S {(env.sustain*100.0) as i32}%  R {env.release:.2}s"
                }
            }
            div { style: "position:relative; width:100%; height:132px; background:#09090b; border:1px solid #1c1c1f; border-radius:6px; overflow:hidden; margin-top:4px;",
                svg {
                    width: "100%",
                    height: "100%",
                    view_box: "0 0 520 132",
                    preserve_aspect_ratio: "none",
                    style: "display:block; touch-action:none;",
                    onmounted: {
                        let mut svg_ref = svg_ref;
                        move |e| svg_ref.set(Some(e.data()))
                    },

                    // baseline + top gridline
                    line { x1: "0", y1: "{y0:.1}", x2: "520", y2: "{y0:.1}", stroke: "#18181b", stroke_width: "0.6" }
                    line { x1: "0", y1: "{ytop:.1}", x2: "520", y2: "{ytop:.1}", stroke: "#141416", stroke_width: "0.6" }

                    // area + curve
                    path { d: "{g.fill}", fill: "{color}18", stroke: "none" }
                    path { d: "{g.stroke}", fill: "none", stroke: "{color}", stroke_width: "2", stroke_linejoin: "round" }

                    // markers
                    circle {
                        cx: "{xa:.1}", cy: "{ytop:.1}", r: "6",
                        fill: "{color}", stroke: "#e4e4e7", stroke_width: "1.5", style: "cursor:grab;",
                        onpointerdown: move |_| begin.call((svg_ref, h_attack, ENV_W, ENV_H, spx, ENV_LEFT)),
                    }
                    circle {
                        cx: "{xc:.1}", cy: "{ysus:.1}", r: "6",
                        fill: "{color}", stroke: "#e4e4e7", stroke_width: "1.5", style: "cursor:grab;",
                        onpointerdown: move |_| begin.call((svg_ref, h_corner, ENV_W, ENV_H, spx, xa)),
                    }
                    circle {
                        cx: "{xr:.1}", cy: "{y0:.1}", r: "6",
                        fill: "{color}", stroke: "#e4e4e7", stroke_width: "1.5", style: "cursor:grab;",
                        onpointerdown: move |_| begin.call((svg_ref, h_release, ENV_W, ENV_H, spx, xh)),
                    }
                }
            }
        }
    }
}

/// Hooks the parent shares with `EnvBlock` via context so the envelope graphs
/// can arm a drag against the shared drag state.
#[derive(Clone, Copy)]
struct EnvHooks {
    amp_svg: Signal<Option<std::rc::Rc<MountedData>>>,
    fenv_svg: Signal<Option<std::rc::Rc<MountedData>>>,
    begin: Callback<(Signal<Option<std::rc::Rc<MountedData>>>, Handle, f64, f64, f64, f64)>,
}

fn layer_btn(selected: bool, active: bool) -> String {
    let (bg, br, fg) = if !active {
        ("#0d0d0f", "#18181b", "#3f3f46")
    } else if selected {
        ("#0c2733", "#38bdf8", "#e4e4e7")
    } else {
        ("#18181b", "#27272a", "#a1a1aa")
    };
    format!("width:32px; height:32px; border-radius:6px; background:{bg}; color:{fg}; border:1px solid {br}; font-size:13px; font-weight:700; cursor:{};", if active { "pointer" } else { "default" })
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
