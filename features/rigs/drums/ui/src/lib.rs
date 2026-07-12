//! Drum-rig Dioxus components — the remote GUI half of the detachable rig.
//! Renders purely from `signal-drums-proto` via the generated vox clients,
//! consumed from Dioxus context (provided by the host app root). Inline styles
//! only (Blitz-safe), no external CSS.

use dioxus::prelude::*;
use signal_drums_proto::drum::{DrumEvent, DrumRigClient, DrumRigStreamClient};
use signal_drums_proto::{DrumStatus, InputMap, KitInfo, MixerStrip, PieceInfo, StripKind};

/// Live drum-rig view-state: seeded once, then folded from the event stream.
#[derive(Clone, Copy)]
struct DrumState {
    status: Signal<DrumStatus>,
    kits: Signal<Vec<KitInfo>>,
    pieces: Signal<Vec<PieceInfo>>,
    mixer: Signal<Vec<MixerStrip>>,
    ports: Signal<Vec<String>>,
}

fn use_drum_state() -> (DrumState, Option<DrumRigClient>) {
    let rig = use_hook(try_consume_context::<DrumRigClient>);
    let stream = use_hook(try_consume_context::<DrumRigStreamClient>);

    let mut status = use_signal(DrumStatus::default);
    let mut kits = use_signal(Vec::<KitInfo>::new);
    let mut pieces = use_signal(Vec::<PieceInfo>::new);
    let mut mixer = use_signal(Vec::<MixerStrip>::new);
    let mut ports = use_signal(Vec::<String>::new);

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
                let (mut status, mut kits, mut pieces, mut mixer) = (status, kits, pieces, mixer);
                match ev {
                    DrumEvent::Status(s) => status.set(s),
                    DrumEvent::Library(k) => kits.set(k),
                    DrumEvent::Kit(p) => pieces.set(p),
                    DrumEvent::Mixer(m) => mixer.set(m),
                    DrumEvent::Midi(_) => {}
                }
            },
        );
    }

    (DrumState { status, kits, pieces, mixer, ports }, rig)
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
                // ── pads + mixer ──
                div { style: "display:flex; flex-direction:column; gap:12px; flex:1; min-height:0; overflow:auto;",
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
                    div {
                        span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Mixer" }
                        div { style: "display:flex; flex-wrap:wrap; gap:6px; margin-top:6px;",
                            for strip in strips.iter() {
                                {
                                    let rig = rig.clone();
                                    let is_bus = strip.kind == StripKind::Bus;
                                    let pct = (strip.peak.clamp(0.0, 1.0) * 100.0) as u32;
                                    let accent = if is_bus { "#7c3aed" } else { "#2563eb" };
                                    let muted = strip.muted;
                                    let idx = strip.idx;
                                    rsx!{ div {
                                        key: "{strip.kind:?}-{strip.idx}",
                                        style: "display:flex; flex-direction:column; align-items:center; gap:4px; width:70px; padding:6px; border-radius:8px; background:#111113; border:1px solid #27272a;",
                                        span { style: "font-size:9px; color:#a1a1aa; text-align:center; height:24px; overflow:hidden;", "{strip.label}" }
                                        div { style: "width:8px; height:60px; background:#18181b; border-radius:2px; display:flex; flex-direction:column-reverse; overflow:hidden;",
                                            div { style: "width:100%; height:{pct}%; background:{accent};" }
                                        }
                                        button {
                                            style: mute_btn(muted),
                                            onclick: move |_| {
                                                let rig = rig.clone();
                                                spawn(async move { if let Some(r) = rig {
                                                    if is_bus { let _ = r.set_bus_mute(idx, !muted).await; }
                                                    else { let _ = r.set_channel_mute(idx, !muted).await; }
                                                } });
                                            },
                                            "M"
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
