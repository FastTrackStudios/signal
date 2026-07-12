//! Drum-rig Dioxus components — the remote GUI half of the detachable rig.
//! Renders purely from `signal-drums-proto` via the generated vox clients,
//! consumed from Dioxus context (provided by the host app root). Inline styles
//! only (Blitz-safe), no external CSS.

use std::collections::HashMap;

use dioxus::prelude::*;
use midicore_proto::MidiEvent;
use midicore_ui::MidiMonitorPanel;
use signal_drums_proto::drum::{DrumEvent, DrumRigClient, DrumRigStreamClient};
use signal_drums_proto::{DrumStatus, InputMap, KitInfo, MixerStrip, PieceInfo, StripKind};
use signal_ui::components::Piano;

/// Live drum-rig view-state: seeded once, then folded from the event stream.
#[derive(Clone, Copy)]
struct DrumState {
    status: Signal<DrumStatus>,
    kits: Signal<Vec<KitInfo>>,
    pieces: Signal<Vec<PieceInfo>>,
    mixer: Signal<Vec<MixerStrip>>,
    ports: Signal<Vec<String>>,
    midi: Signal<Vec<MidiEvent>>,
}

fn use_drum_state() -> (DrumState, Option<DrumRigClient>) {
    let rig = use_hook(try_consume_context::<DrumRigClient>);
    let stream = use_hook(try_consume_context::<DrumRigStreamClient>);

    let mut status = use_signal(DrumStatus::default);
    let mut kits = use_signal(Vec::<KitInfo>::new);
    let mut pieces = use_signal(Vec::<PieceInfo>::new);
    let mut mixer = use_signal(Vec::<MixerStrip>::new);
    let mut ports = use_signal(Vec::<String>::new);
    let mut midi = use_signal(Vec::<MidiEvent>::new);

    // Seed once — the event stream carries only changes.
    {
        let rig = rig.clone();
        use_future(move || {
            let rig = rig.clone();
            async move {
                let Some(rig) = rig else { return };
                if let Ok(s) = rig.status().await {
                    status.set(s);
                }
                if let Ok(k) = rig.kits().await {
                    kits.set(k);
                }
                if let Ok(p) = rig.pieces().await {
                    pieces.set(p);
                }
                if let Ok(m) = rig.mixer().await {
                    mixer.set(m);
                }
                if let Ok(p) = rig.midi_ports().await {
                    ports.set(p);
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
            move |ev: DrumEvent| {
                let (mut status, mut kits, mut pieces, mut mixer, mut midi) =
                    (status, kits, pieces, mixer, midi);
                match ev {
                    DrumEvent::Status(s) => status.set(s),
                    DrumEvent::Library(k) => kits.set(k),
                    DrumEvent::Kit(p) => pieces.set(p),
                    DrumEvent::Mixer(m) => mixer.set(m),
                    DrumEvent::Midi(m) => midi.set(m),
                }
            },
        );
    }

    (DrumState { status, kits, pieces, mixer, ports, midi }, rig)
}

/// The drum-rig remote view. Mount inside a host that has provided
/// `DrumRigClient` + `DrumRigStreamClient` in context.
#[component]
pub fn DrumRigRemote() -> Element {
    let (state, rig) = use_drum_state();
    let status = state.status.read().clone();
    let running = status.running;
    let preload = status.preload;
    let kits = state.kits.read().clone();
    let pieces = state.pieces.read().clone();
    let strips = state.mixer.read().clone();
    let ports = state.ports.read().clone();
    let midi = state.midi.read().clone();
    let midi_count = midi.len() as u64;
    // Label each mapped key with the piece/sample it plays.
    let piece_labels: HashMap<u8, String> =
        pieces.iter().map(|p| (p.note as u8, p.id.clone())).collect();
    // Light the most-recently-struck notes (trailing highlight off the stream).
    let lit: Vec<u8> = midi
        .iter()
        .rev()
        .filter_map(|e| match e {
            MidiEvent::NoteOn { key, .. } => Some(key.get()),
            _ => None,
        })
        .take(4)
        .collect();
    // Most-recently-played key and the sample it maps to, for the readout.
    let last_played: Option<(u8, Option<String>)> = midi.iter().rev().find_map(|e| match e {
        MidiEvent::NoteOn { key, .. } => {
            Some((key.get(), piece_labels.get(&key.get()).cloned()))
        }
        _ => None,
    });
    let master_pct = (status.master_peak.clamp(0.0, 1.0) * 100.0) as u32;
    let master_color = meter_color(status.master_peak);

    let maps = [
        ("Direct", InputMap::Direct),
        ("Strata Prime", InputMap::StrataPrime),
        ("FTS", InputMap::Fts),
        ("GGD v1", InputMap::Ggd),
    ];
    let current_port = status.midi_port.clone();
    let current_map = status.input_map;

    rsx! {
        div { style: "display:flex; flex-direction:column; gap:12px; padding:12px; color:#e4e4e7; font-family:system-ui,sans-serif; flex:1; min-height:0; overflow:auto;",
            // ── transport / status ──
            div { style: "display:flex; align-items:center; gap:12px;",
                {
                    let rig = rig.clone();
                    rsx!{ button {
                        style: transport_btn(running),
                        onclick: move |_| {
                            let rig = rig.clone();
                            spawn(async move { if let Some(r) = rig { if running { let _ = r.stop().await; } else { let _ = r.start().await; } } });
                        },
                        if running { "■ Stop" } else { "▶ Start" }
                    } }
                }
                span { style: "font-size:13px; font-weight:700;",
                    { status.loaded_kit.clone().unwrap_or_else(|| "No kit loaded".into()) }
                }
                div { style: "width:120px; height:10px; background:#18181b; border:1px solid #27272a; border-radius:3px; overflow:hidden;",
                    div { style: "width:{master_pct}%; height:100%; background:{master_color};" }
                }
                span { style: "font-size:11px; color:#71717a;", "{status.voices} voices" }
                if running && preload < 0.999 {
                    span { style: "font-size:11px; color:#eab308;", "loading {(preload*100.0) as u32}%" }
                }
            }

            // ── MIDI input row ──
            div { style: "display:flex; align-items:center; gap:8px; font-size:11px; color:#a1a1aa;",
                span { "MIDI in:" }
                {
                    let rig = rig.clone();
                    rsx!{ select {
                        style: "background:#18181b; color:#e4e4e7; border:1px solid #27272a; border-radius:5px; padding:2px 6px; font-size:11px;",
                        onchange: move |e| {
                            let rig = rig.clone(); let v = e.value();
                            spawn(async move { if let Some(r) = rig { let _ = r.set_midi_port(if v == "—" { String::new() } else { v }).await; } });
                        },
                        option { value: "—", selected: current_port.is_none(), "— none —" }
                        for p in ports.iter() {
                            option { value: "{p}", selected: current_port.as_deref() == Some(p.as_str()), "{p}" }
                        }
                    } }
                }
                span { "map:" }
                {
                    let rig = rig.clone();
                    rsx!{ select {
                        style: "background:#18181b; color:#e4e4e7; border:1px solid #27272a; border-radius:5px; padding:2px 6px; font-size:11px;",
                        onchange: move |e| {
                            let rig = rig.clone();
                            let m = match e.value().as_str() {
                                "Strata Prime" => InputMap::StrataPrime,
                                "FTS" => InputMap::Fts,
                                "GGD v1" => InputMap::Ggd,
                                _ => InputMap::Direct,
                            };
                            spawn(async move { if let Some(r) = rig { let _ = r.set_input_map(m).await; } });
                        },
                        for (label, m) in maps.iter() {
                            option { value: "{label}", selected: *m == current_map, "{label}" }
                        }
                    } }
                }
            }

            div { style: "display:flex; gap:12px; flex:1; min-height:0;",
                // ── kit library ──
                div { style: "display:flex; flex-direction:column; gap:4px; width:220px; min-width:220px; overflow:auto; border-right:1px solid #1c1c1f; padding-right:8px;",
                    span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Kits ({kits.len()})" }
                    for (i, kit) in kits.iter().enumerate() {
                        {
                            let rig = rig.clone();
                            rsx!{ button {
                                key: "{kit.path}",
                                style: kit_btn(kit.loaded),
                                onclick: move |_| { let rig = rig.clone(); spawn(async move { if let Some(r) = rig { let _ = r.load_kit(i as u32).await; } }); },
                                "{kit.name}"
                            } }
                        }
                    }
                }
                // ── piano + pads + mixer ──
                div { style: "display:flex; flex-direction:column; gap:12px; flex:1; min-height:0; overflow:auto;",
                    div {
                        div { style: "display:flex; align-items:baseline; gap:10px; margin-bottom:4px;",
                            span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Keyboard" }
                            match &last_played {
                                Some((n, Some(sample))) => rsx!{ span { style: "font-size:11px; color:#22c55e;", "note {n} → {sample}" } },
                                Some((n, None)) => rsx!{ span { style: "font-size:11px; color:#71717a;", "note {n} → (no sample here)" } },
                                None => rsx!{ span { style: "font-size:11px; color:#52525b;", "play a key…" } },
                            }
                        }
                        {
                            let rig_on = rig.clone();
                            let rig_off = rig.clone();
                            rsx!{ Piano {
                                start_note: 21,
                                end_note: 108,
                                active_notes: lit,
                                labels: piece_labels,
                                show_labels: false,
                                waterfall: false,
                                accent_color: "#22c55e".to_string(),
                                height: "132px",
                                on_note_on: move |n: u8| { let rig = rig_on.clone(); spawn(async move { if let Some(r) = rig { let _ = r.trigger(n as u32, 110).await; } }); },
                                on_note_off: move |n: u8| { let rig = rig_off.clone(); spawn(async move { if let Some(r) = rig { let _ = r.trigger(n as u32, 0).await; } }); },
                            } }
                        }
                    }
                    div {
                        span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Pads" }
                        div { style: "display:flex; flex-wrap:wrap; gap:6px; margin-top:6px;",
                            for piece in pieces.iter() {
                                {
                                    let rig = rig.clone();
                                    let note = piece.note;
                                    let ready = piece.total_samples == 0 || piece.loaded_samples >= piece.total_samples;
                                    rsx!{ button {
                                        key: "{piece.id}",
                                        style: pad_btn(ready),
                                        onclick: move |_| { let rig = rig.clone(); spawn(async move { if let Some(r) = rig { let _ = r.trigger(note, 110).await; } }); },
                                        span { style: "font-size:12px; font-weight:600;", "{piece.id}" }
                                        span { style: "font-size:9px; color:#71717a;", "note {piece.note}" }
                                    } }
                                }
                            }
                        }
                    }
                    MidiMonitorPanel { events: midi, count: midi_count, title: "MIDI monitor".to_string() }
                    div {
                        span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Mixer" }
                        div { style: "display:flex; flex-wrap:wrap; gap:6px; margin-top:6px; align-items:flex-end;",
                            for strip in strips.iter() {
                                {
                                    let rig = rig.clone();
                                    let is_bus = strip.kind == StripKind::Bus;
                                    let pct = (strip.peak.clamp(0.0, 1.0) * 100.0) as u32;
                                    let mcolor = meter_color(strip.peak);
                                    let accent = if is_bus { "#7c3aed" } else { "#2563eb" };
                                    let muted = strip.muted;
                                    let soloed = strip.soloed;
                                    let idx = strip.idx;
                                    let gain_db = strip.gain_db;
                                    // Fader fill: map -60..+12 dB onto 0..100%.
                                    let fader_pct = (((gain_db + 60.0) / 72.0).clamp(0.0, 1.0) * 100.0) as u32;
                                    let (rg, rm, rs) = (rig.clone(), rig.clone(), rig.clone());
                                    rsx!{ div {
                                        key: "{strip.kind:?}-{strip.idx}",
                                        style: format!("display:flex; flex-direction:column; align-items:center; gap:4px; width:64px; padding:6px; border-radius:8px; background:#111113; border:1px solid {};", if is_bus { "#3b2f5c" } else { "#27272a" }),
                                        span { style: "font-size:9px; color:#e4e4e7; text-align:center; height:22px; overflow:hidden; font-weight:600;", "{strip.label}" }
                                        // meter + fader side by side
                                        div { style: "display:flex; gap:5px; height:90px; align-items:flex-end;",
                                            // peak meter
                                            div { style: "width:8px; height:90px; background:#18181b; border-radius:2px; display:flex; flex-direction:column-reverse; overflow:hidden;",
                                                div { style: "width:100%; height:{pct}%; background:{mcolor};" }
                                            }
                                            // vertical fader: visible track+fill+thumb, invisible range input on top
                                            div { style: "position:relative; width:22px; height:90px; display:flex; justify-content:center;",
                                                div { style: "position:absolute; bottom:0; width:4px; height:100%; background:#27272a; border-radius:2px;" }
                                                div { style: "position:absolute; bottom:0; width:4px; height:{fader_pct}%; background:{accent}; border-radius:2px;" }
                                                div { style: "position:absolute; bottom:calc({fader_pct}% - 5px); width:18px; height:10px; background:#52525b; border:1px solid #a1a1aa; border-radius:2px;" }
                                                input {
                                                    r#type: "range", min: "-60", max: "12", step: "1",
                                                    value: "{gain_db}",
                                                    style: "position:absolute; inset:0; width:100%; height:100%; opacity:0; cursor:pointer;",
                                                    oninput: move |e| {
                                                        let rig = rg.clone();
                                                        if let Ok(db) = e.value().parse::<f32>() {
                                                            spawn(async move { if let Some(r) = rig {
                                                                if is_bus { let _ = r.set_bus_gain(idx, db).await; }
                                                                else { let _ = r.set_piece_gain(idx, db).await; }
                                                            }});
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                        span { style: "font-size:8px; color:#71717a;", {format!("{:+.0} dB", gain_db)} }
                                        // solo / mute
                                        div { style: "display:flex; gap:3px;",
                                            button {
                                                style: solo_btn(soloed),
                                                onclick: move |_| {
                                                    let rig = rs.clone();
                                                    spawn(async move { if let Some(r) = rig {
                                                        if is_bus { let _ = r.set_bus_solo(idx, !soloed).await; }
                                                        else { let _ = r.set_piece_solo(idx, !soloed).await; }
                                                    }});
                                                },
                                                "S"
                                            }
                                            button {
                                                style: mute_btn(muted),
                                                onclick: move |_| {
                                                    let rig = rm.clone();
                                                    spawn(async move { if let Some(r) = rig {
                                                        if is_bus { let _ = r.set_bus_mute(idx, !muted).await; }
                                                        else { let _ = r.set_piece_mute(idx, !muted).await; }
                                                    }});
                                                },
                                                "M"
                                            }
                                        }
                                    } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn meter_color(peak: f32) -> &'static str {
    if peak > 0.95 { "#ef4444" } else if peak > 0.7 { "#eab308" } else { "#22c55e" }
}

fn transport_btn(running: bool) -> String {
    let (bg, br) = if running { ("#3f1d1d", "#7f1d1d") } else { ("#14321e", "#166534") };
    format!("padding:4px 12px; border-radius:6px; background:{bg}; color:#e4e4e7; border:1px solid {br}; font-size:12px; cursor:pointer;")
}

fn kit_btn(loaded: bool) -> String {
    let (bg, br, fg) = if loaded { ("#1e293b", "#3b82f6", "#e4e4e7") } else { ("#111113", "#27272a", "#a1a1aa") };
    format!("text-align:left; padding:6px 8px; border-radius:6px; background:{bg}; color:{fg}; border:1px solid {br}; font-size:12px; cursor:pointer;")
}

fn pad_btn(ready: bool) -> String {
    let border = if ready { "#3f3f46" } else { "#52341a" };
    format!("display:flex; flex-direction:column; align-items:center; gap:2px; width:78px; height:56px; justify-content:center; border-radius:8px; background:#161618; color:#e4e4e7; border:1px solid {border}; cursor:pointer;")
}

fn mute_btn(muted: bool) -> String {
    let (bg, fg) = if muted { ("#7f1d1d", "#fecaca") } else { ("#18181b", "#71717a") };
    format!("width:20px; height:18px; border-radius:4px; background:{bg}; color:{fg}; border:1px solid #27272a; font-size:10px; cursor:pointer;")
}

fn solo_btn(soloed: bool) -> String {
    let (bg, fg) = if soloed { ("#78560f", "#fde68a") } else { ("#18181b", "#71717a") };
    format!("width:20px; height:18px; border-radius:4px; background:{bg}; color:{fg}; border:1px solid #27272a; font-size:10px; cursor:pointer;")
}
