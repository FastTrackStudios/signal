//! Drum-rig Dioxus components — the remote GUI half of the detachable rig.
//! Renders purely from `signal-drums-proto` via the generated vox clients,
//! consumed from Dioxus context (provided by the host app root). Inline styles
//! only (Blitz-safe), no external CSS.

use std::collections::HashMap;

use dioxus::prelude::*;
use midicore_proto::MidiEvent;
use midicore_ui::MidiMonitorPanel;
use signal_drums_proto::drum::{DrumEvent, DrumRigClient, DrumRigStreamClient};
use signal_drums_proto::{DrumStatus, InputMap, KitInfo, KitSlot, LibraryPiece, MeterSnapshot,
    MixerStrip, PieceInfo, StripKind};
use signal_ui::components::Piano;

/// Live drum-rig view-state: seeded once, then folded from the event stream.
#[derive(Clone, Copy)]
struct DrumState {
    status: Signal<DrumStatus>,
    kits: Signal<Vec<KitInfo>>,
    pieces: Signal<Vec<PieceInfo>>,
    mixer: Signal<Vec<MixerStrip>>,
    /// High-rate peaks — separate from `mixer` (control state) so meter updates
    /// never re-render / clobber a fader being dragged.
    meters: Signal<MeterSnapshot>,
    /// Kit-designer rows: each piece slot + the instrument currently in it.
    slots: Signal<Vec<KitSlot>>,
    /// The whole sample library (swappable pieces), grouped by kind on render.
    library: Signal<Vec<LibraryPiece>>,
    /// Available MM2 mix presets (names) to import onto the kit.
    mixes: Signal<Vec<String>>,
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
    let mut meters = use_signal(MeterSnapshot::default);
    let mut slots = use_signal(Vec::<KitSlot>::new);
    let mut library = use_signal(Vec::<LibraryPiece>::new);
    let mut mixes = use_signal(Vec::<String>::new);
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
                if let Ok(m) = rig.meters().await {
                    meters.set(m);
                }
                if let Ok(s) = rig.kit_slots().await {
                    slots.set(s);
                }
                if let Ok(l) = rig.library().await {
                    library.set(l);
                }
                if let Ok(m) = rig.mm2_mixes().await {
                    mixes.set(m);
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
                let (mut status, mut kits, mut pieces, mut mixer, mut meters, mut slots, mut midi) =
                    (status, kits, pieces, mixer, meters, slots, midi);
                match ev {
                    DrumEvent::Status(s) => status.set(s),
                    DrumEvent::Library(k) => kits.set(k),
                    DrumEvent::Kit(p) => pieces.set(p),
                    DrumEvent::Design(s) => slots.set(s),
                    DrumEvent::Mixer(m) => mixer.set(m),
                    DrumEvent::Meters(m) => meters.set(m),
                    DrumEvent::Midi(m) => midi.set(m),
                }
            },
        );
    }

    (DrumState { status, kits, pieces, mixer, meters, slots, library, mixes, ports, midi }, rig)
}

/// The drum-rig remote view. Mount inside a host that has provided
/// `DrumRigClient` + `DrumRigStreamClient` in context.
#[component]
pub fn DrumRigRemote() -> Element {
    let (state, rig) = use_drum_state();
    let status = state.status.read().clone();
    let running = status.running;
    let preload = status.preload;
    let pieces = state.pieces.read().clone();
    let strips = state.mixer.read().clone();
    // Writable handle for optimistic fader/send updates (track the finger with
    // no network round-trip; the engine sync is fire-and-forget).
    let mixer_sig = state.mixer;
    let meters = state.meters.read().clone();
    let slots = state.slots.read().clone();
    let library = state.library.read().clone();
    let mixes = state.mixes.read().clone();
    let ports = state.ports.read().clone();
    let midi = state.midi.read().clone();
    let midi_count = midi.len() as u64;
    // Label each mapped key with the piece/sample it plays (icon + name).
    let piece_labels: HashMap<u8, String> = pieces
        .iter()
        .map(|p| (p.note as u8, format!("{} {}", piece_icon(&p.id), p.id)))
        .collect();
    // Light only keys currently *held*: fold the recent event buffer (oldest→
    // newest) — a NoteOn(vel>0) presses a key, a NoteOff or NoteOn(vel 0)
    // releases it. So the light tracks press/release, not the last-N struck.
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
    // Most-recently-played key and the sample it maps to, for the readout.
    let last_played: Option<(u8, Option<String>)> = midi.iter().rev().find_map(|e| match e {
        MidiEvent::NoteOn { key, .. } => {
            Some((key.get(), piece_labels.get(&key.get()).cloned()))
        }
        _ => None,
    });
    let master_pct = (meters.master.clamp(0.0, 1.0) * 100.0) as u32;
    let master_color = meter_color(meters.master);

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
                        option { value: "—", selected: current_port.is_none(), "Omni (all MIDI)" }
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
                // ── presets: a preset = a kit + its mix (levels + FX) ──
                div { style: "display:flex; flex-direction:column; gap:4px; width:220px; min-width:220px; overflow:auto; border-right:1px solid #1c1c1f; padding-right:8px;",
                    span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Presets" }
                    for preset in mixes.iter() {
                        {
                            let rig = rig.clone();
                            let name = preset.clone();
                            let loaded = status.loaded_kit.as_deref().map(|k| k.eq_ignore_ascii_case(&name)).unwrap_or(false);
                            rsx!{ button {
                                key: "{preset}",
                                style: kit_btn(loaded),
                                onclick: move |_| { let rig = rig.clone(); let name = name.clone(); spawn(async move { if let Some(r) = rig { let _ = r.import_mm2_mix(name).await; } }); },
                                "{preset}"
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
                                active_notes: lit.clone(),
                                labels: piece_labels,
                                show_labels: true,
                                waterfall: false,
                                accent_color: "#22c55e".to_string(),
                                height: "132px",
                                on_note_on: move |n: u8| { let rig = rig_on.clone(); spawn(async move { if let Some(r) = rig { let _ = r.trigger(n as u32, 110).await; } }); },
                                on_note_off: move |n: u8| { let rig = rig_off.clone(); spawn(async move { if let Some(r) = rig { let _ = r.trigger(n as u32, 0).await; } }); },
                            } }
                        }
                    }
                    // ── Kit designer: swap any piece for another of its type ──
                    div {
                        span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Kit designer" }
                        div { style: "display:flex; flex-wrap:wrap; gap:6px; margin-top:6px;",
                            for slot in slots.iter() {
                                {
                                    let rig = rig.clone();
                                    let slot_id = slot.slot_id.clone();
                                    let icon = piece_icon(&slot.label);
                                    // Library options matching this slot's kind (plus the
                                    // current one, so it's always selectable even if its
                                    // kind reads oddly).
                                    let opts: Vec<LibraryPiece> = library
                                        .iter()
                                        .filter(|p| p.kind == slot.kind)
                                        .cloned()
                                        .collect();
                                    let cur = slot.current_path.clone();
                                    rsx!{ div {
                                        key: "{slot.slot_id}",
                                        style: "display:flex; flex-direction:column; gap:3px; width:150px; padding:7px; border-radius:8px; background:#111113; border:1px solid #27272a;",
                                        span { style: "font-size:10px; color:#e4e4e7; font-weight:600;", "{icon} {slot.label}" }
                                        select {
                                            style: "background:#18181b; color:#e4e4e7; border:1px solid #27272a; border-radius:5px; padding:3px 5px; font-size:10px; width:100%;",
                                            onchange: move |e| {
                                                let (rig, slot_id, path) = (rig.clone(), slot_id.clone(), e.value());
                                                if !path.is_empty() {
                                                    spawn(async move { if let Some(r) = rig { let _ = r.swap_piece(slot_id, path).await; } });
                                                }
                                            },
                                            if opts.is_empty() {
                                                option { value: "", selected: true, "{slot.current_name}" }
                                            }
                                            for opt in opts.iter() {
                                                option {
                                                    value: "{opt.path}",
                                                    selected: opt.path == cur,
                                                    "{opt.name}"
                                                }
                                            }
                                        }
                                    } }
                                }
                            }
                        }
                    }
                    div {
                        span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Kit" }
                        {
                            let notes: Vec<(String, u32)> = pieces.iter().map(|p| (p.id.clone(), p.note)).collect();
                            let rig_hit = rig.clone();
                            rsx!{ DrumKit {
                                slots: slots.clone(),
                                notes: notes,
                                lit: lit.clone(),
                                on_hit: EventHandler::new(move |note: u32| {
                                    let rig = rig_hit.clone();
                                    spawn(async move { if let Some(r) = rig {
                                        let _ = r.trigger(note, 110).await;
                                        let _ = r.trigger(note, 0).await;
                                    }});
                                }),
                            } }
                        }
                    }
                    MidiMonitorPanel { events: midi, count: midi_count, title: "MIDI monitor".to_string() }
                    div {
                        span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Mixer" }
                        div { style: "display:flex; flex-wrap:wrap; gap:6px; margin-top:6px; align-items:flex-start;",
                            for (i, strip) in strips.iter().enumerate() {
                                {
                                    let rig = rig.clone();
                                    let kind = strip.kind;
                                    let is_bus = kind == StripKind::Bus;
                                    let is_ch = kind == StripKind::Channel;
                                    let peak = meters.strips.get(i).copied().unwrap_or(0.0);
                                    let pct = (peak.clamp(0.0, 1.0) * 100.0) as u32;
                                    let mcolor = meter_color(peak);
                                    let accent = match kind { StripKind::Bus => "#7c3aed", StripKind::Channel => "#3b82f6", _ => "#2563eb" };
                                    let muted = strip.muted;
                                    let soloed = strip.soloed;
                                    let idx = strip.idx;
                                    let gain_db = strip.gain_db;
                                    // Piece folders show an icon; mic channels just the mic name.
                                    let label = if is_ch { strip.label.clone() } else { format!("{} {}", piece_icon(&strip.label), strip.label) };
                                    let sends = strip.sends.clone();
                                    // Fader fill: map -60..+12 dB onto 0..100%.
                                    let fader_pct = (((gain_db + 60.0) / 72.0).clamp(0.0, 1.0) * 100.0) as u32;
                                    let (rg, rm, rs) = (rig.clone(), rig.clone(), rig.clone());
                                    // Folder = wider + bright border; mic channel = narrower, indented,
                                    // dimmer (nested under its piece); bus = purple.
                                    let (width, mh, border, bg) = if is_ch { (52, 70, "#27272a", "#0d0d0f") }
                                        else if is_bus { (64, 90, "#3b2f5c", "#111113") }
                                        else { (64, 90, "#3f5178", "#14161c") };
                                    let ml = if is_ch { "margin-left:-2px;" } else { "" };
                                    rsx!{ div {
                                        key: "{strip.kind:?}-{strip.idx}",
                                        style: format!("display:flex; flex-direction:column; align-items:center; gap:4px; width:{width}px; padding:6px; border-radius:8px; background:{bg}; border:1px solid {border}; {ml}"),
                                        span { style: "font-size:9px; color:#e4e4e7; text-align:center; height:22px; overflow:hidden; font-weight:600;", "{label}" }
                                        // meter + fader side by side
                                        div { style: "display:flex; gap:5px; height:{mh}px; align-items:flex-end;",
                                            // peak meter
                                            div { style: "width:8px; height:{mh}px; background:#18181b; border-radius:2px; display:flex; flex-direction:column-reverse; overflow:hidden;",
                                                div { style: "width:100%; height:{pct}%; background:{mcolor};" }
                                            }
                                            // vertical fader: visible track+fill+thumb, invisible range input on top
                                            div { style: "position:relative; width:22px; height:{mh}px; display:flex; justify-content:center;",
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
                                                            // Optimistic: move the fader now, sync the engine async.
                                                            let mut m = mixer_sig;
                                                            if let Some(s) = m.write().get_mut(i) { s.gain_db = db; }
                                                            spawn(async move { if let Some(r) = rig {
                                                                match kind {
                                                                    StripKind::Channel => { let _ = r.set_channel_gain(idx, db).await; }
                                                                    StripKind::Bus => { let _ = r.set_bus_gain(idx, db).await; }
                                                                    _ => { let _ = r.set_piece_gain(idx, db).await; }
                                                                }
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
                                                        match kind {
                                                            StripKind::Channel => { let _ = r.set_channel_solo(idx, !soloed).await; }
                                                            StripKind::Bus => { let _ = r.set_bus_solo(idx, !soloed).await; }
                                                            _ => { let _ = r.set_piece_solo(idx, !soloed).await; }
                                                        }
                                                    }});
                                                },
                                                "S"
                                            }
                                            button {
                                                style: mute_btn(muted),
                                                onclick: move |_| {
                                                    let rig = rm.clone();
                                                    spawn(async move { if let Some(r) = rig {
                                                        match kind {
                                                            StripKind::Channel => { let _ = r.set_channel_mute(idx, !muted).await; }
                                                            StripKind::Bus => { let _ = r.set_bus_mute(idx, !muted).await; }
                                                            _ => { let _ = r.set_piece_mute(idx, !muted).await; }
                                                        }
                                                    }});
                                                },
                                                "M"
                                            }
                                        }
                                        // per-piece bus sends (kick → overhead / room …)
                                        if !sends.is_empty() {
                                            div { style: "display:flex; flex-direction:column; gap:2px; width:100%; margin-top:2px; border-top:1px solid #27272a; padding-top:3px;",
                                                for send in sends.iter() {
                                                    {
                                                        let rig = rig.clone();
                                                        let sidx = send.idx;
                                                        let slvl = send.level_db;
                                                        let sabbr = bus_abbr(&send.bus_label);
                                                        let spct = (((slvl + 60.0) / 72.0).clamp(0.0, 1.0) * 100.0) as u32;
                                                        rsx!{ div {
                                                            key: "s{sidx}",
                                                            style: "display:flex; align-items:center; gap:3px;",
                                                            span { style: "font-size:7px; color:#8b8b93; width:16px;", "{sabbr}" }
                                                            div { style: "position:relative; flex:1; height:10px; display:flex; align-items:center;",
                                                                div { style: "position:absolute; left:0; right:0; height:3px; background:#27272a; border-radius:2px;" }
                                                                div { style: "position:absolute; left:0; width:{spct}%; height:3px; background:#a16207; border-radius:2px;" }
                                                                input {
                                                                    r#type: "range", min: "-60", max: "12", step: "1",
                                                                    value: "{slvl}",
                                                                    style: "position:absolute; inset:0; width:100%; opacity:0; cursor:pointer;",
                                                                    oninput: move |e| {
                                                                        let rig = rig.clone();
                                                                        if let Ok(db) = e.value().parse::<f32>() {
                                                                            let mut m = mixer_sig;
                                                                            if let Some(s) = m.write().get_mut(i) {
                                                                                if let Some(sd) = s.sends.iter_mut().find(|x| x.idx == sidx) { sd.level_db = db; }
                                                                            }
                                                                            spawn(async move { if let Some(r) = rig { let _ = r.set_send_level(sidx, db).await; } });
                                                                        }
                                                                    },
                                                                }
                                                            }
                                                        } }
                                                    }
                                                }
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

/// A top-down drum-kit diagram: each piece drawn in a realistic layout
/// (kick(s) center, snare + toms, cymbals around), labelled with its current
/// instrument. Pieces flash when played (held MIDI notes) and trigger on click.
#[component]
fn DrumKit(slots: Vec<KitSlot>, notes: Vec<(String, u32)>, lit: Vec<u8>, on_hit: EventHandler<u32>) -> Element {
    let note_of = |id: &str| notes.iter().find(|(i, _)| i == id).map(|(_, n)| *n).unwrap_or(0);
    // Assign each slot a position; count occurrences so a 2nd kick/snare gets
    // its own spot.
    let mut occ: HashMap<String, usize> = HashMap::new();
    let mut placed: Vec<Placed> = Vec::new();
    for s in &slots {
        let l = s.label.to_ascii_lowercase();
        let cat = if l.starts_with("kick") {
            "kick"
        } else if l.starts_with("snare") {
            "snare"
        } else {
            l.as_str()
        }
        .to_string();
        let o = *occ.get(&cat).unwrap_or(&0);
        occ.insert(cat, o + 1);
        if let Some((cx, cy, rx, ry, cym)) = kit_pos(&s.label, o) {
            let note = note_of(&s.slot_id);
            placed.push(Placed {
                label: s.label.clone(),
                instrument: instrument_short(&s.current_name),
                note,
                cx,
                cy,
                rx,
                ry,
                cymbal: cym,
            });
        }
    }
    rsx! {
        svg {
            view_box: "0 0 1000 620",
            style: "width:100%; max-width:860px; height:auto; display:block; margin:6px auto 0; background:radial-gradient(ellipse at 50% 62%, #1c1c22 0%, #101014 70%); border-radius:12px; border:1px solid #27272a;",
            // Cymbal stands (behind everything).
            for p in placed.iter().filter(|p| p.cymbal) {
                line {
                    x1: "{p.cx}", y1: "{p.cy}", x2: "{p.cx}", y2: "600",
                    style: "stroke:#3a3a42; stroke-width:3;",
                }
            }
            for p in placed.iter() {
                {
                    let note = p.note;
                    let is_lit = note > 0 && lit.contains(&(note as u8));
                    let (cx, cy, rx, ry) = (p.cx, p.cy, p.rx, p.ry);
                    rsx!{ g {
                        style: "cursor:pointer;",
                        onclick: move |_| on_hit.call(note),
                        if p.cymbal {
                            // Cymbal: gold disc + concentric grooves + bell.
                            ellipse { cx: "{cx}", cy: "{cy}", rx: "{rx}", ry: "{ry}",
                                style: format!("fill:{}; stroke:{}; stroke-width:1.5;", if is_lit {"#f2d268"} else {"#c39a3c"}, if is_lit {"#fff1c0"} else {"#7d621f"}) }
                            ellipse { cx: "{cx}", cy: "{cy}", rx: "{rx*0.66}", ry: "{ry*0.66}", style: "fill:none; stroke:#8a6d24; stroke-width:1;" }
                            ellipse { cx: "{cx}", cy: "{cy}", rx: "{rx*0.22}", ry: "{ry*0.5}", style: format!("fill:{};", if is_lit {"#ffe89a"} else {"#d9b757"}) }
                        } else {
                            // Drum: shell rim + coated head.
                            ellipse { cx: "{cx}", cy: "{cy+ry*0.12}", rx: "{rx}", ry: "{ry}", style: "fill:#2a1d10;" }
                            ellipse { cx: "{cx}", cy: "{cy}", rx: "{rx*0.9}", ry: "{ry*0.86}",
                                style: format!("fill:{}; stroke:{}; stroke-width:2;", if is_lit {"#fff6d8"} else {"#ded3bd"}, if is_lit {"#ffe9a0"} else {"#b7ab90"}) }
                        }
                        // Piece label (centered on the piece).
                        text { x: "{cx}", y: "{cy + 4.0}",
                            style: format!("text-anchor:middle; font-size:13px; font-weight:700; fill:{}; pointer-events:none;", if p.cymbal {"#2e2410"} else {"#2a1e10"}),
                            "{p.label}" }
                        // Instrument name (below the piece).
                        text { x: "{cx}", y: "{cy + ry + 15.0}",
                            style: "text-anchor:middle; font-size:11px; fill:#a9a9b4; pointer-events:none;",
                            "{p.instrument}" }
                    } }
                }
            }
        }
    }
}

/// One positioned kit piece for [`DrumKit`].
struct Placed {
    label: String,
    instrument: String,
    note: u32,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    cymbal: bool,
}

/// Layout position `(cx, cy, rx, ry, is_cymbal)` for a piece label in a
/// top-down kit (viewBox 1000×620). `occ` disambiguates a 2nd kick/snare.
fn kit_pos(label: &str, occ: usize) -> Option<(f64, f64, f64, f64, bool)> {
    let l = label.to_ascii_lowercase();
    Some(if l.starts_with("kick") {
        if occ == 0 { (500., 500., 96., 82., false) } else { (345., 508., 86., 72., false) }
    } else if l.starts_with("snare") {
        if occ == 0 { (430., 402., 54., 46., false) } else { (352., 384., 44., 38., false) }
    } else if l == "rack tom 1" {
        (452., 302., 48., 40., false)
    } else if l == "rack tom 2" {
        (572., 294., 50., 42., false)
    } else if l == "rack tom 3" {
        (512., 242., 44., 37., false)
    } else if l == "floor tom 1" {
        (692., 404., 58., 50., false)
    } else if l == "floor tom 2" {
        (780., 494., 64., 55., false)
    } else if l.starts_with("hat") {
        (300., 358., 64., 18., true)
    } else if l == "ride" {
        (780., 288., 80., 22., true)
    } else if l == "crash l" {
        (378., 226., 62., 17., true)
    } else if l == "crash r" {
        (628., 210., 64., 18., true)
    } else if l == "crash far l" {
        (236., 288., 58., 16., true)
    } else if l == "crash far r" {
        (708., 190., 60., 17., true)
    } else if l == "china" {
        (864., 246., 62., 17., true)
    } else if l == "splash" {
        (530., 206., 44., 12., true)
    } else {
        return None;
    })
}

/// Short instrument name for a kit-piece label: drop the "MM2 " prefix and
/// hyphens, truncate.
fn instrument_short(name: &str) -> String {
    let n = name.strip_prefix("MM2 ").unwrap_or(name).replace('-', " ");
    if n.chars().count() > 26 {
        format!("{}…", n.chars().take(25).collect::<String>())
    } else {
        n
    }
}

/// An emoji icon for a drum piece, chosen by keyword — shown on mixer strips
/// and piano-key labels so a kit reads at a glance.
fn piece_icon(id: &str) -> &'static str {
    let s = id.to_ascii_lowercase();
    if s.contains("kick") { "🦶" }
    else if s.contains("snare") { "🥁" }
    else if s.contains("hh") || s.contains("hat") { "🎩" }
    else if s.contains("tom") { "🪘" }
    else if s.contains("ride") { "🛎" }
    else if s.contains("crash") { "💥" }
    else if s.contains("china") { "🥢" }
    else if s.contains("splash") { "💦" }
    else if s.contains("overhead") || s.contains("oh") { "🎙" }
    else if s.contains("room") { "🏠" }
    else { "🥁" }
}

/// Short 2-3 char abbreviation for a bus label (send row is tight on space).
fn bus_abbr(label: &str) -> String {
    let l = label.to_ascii_lowercase();
    if l.contains("overhead") || l == "oh" { "OH".into() }
    else if l.contains("room close") { "RmC".into() }
    else if l.contains("room far") { "RmF".into() }
    else if l.contains("room") { "Rm".into() }
    else { label.chars().take(3).collect() }
}

fn solo_btn(soloed: bool) -> String {
    let (bg, fg) = if soloed { ("#78560f", "#fde68a") } else { ("#18181b", "#71717a") };
    format!("width:20px; height:18px; border-radius:4px; background:{bg}; color:{fg}; border:1px solid #27272a; font-size:10px; cursor:pointer;")
}
