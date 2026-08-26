//! Bass-rig Dioxus components — the remote GUI half of the detachable bass
//! rig. Renders purely from `signal-bass-proto` via the generated vox clients
//! (provided in Dioxus context by the host). Inline styles only (Blitz-safe).
//!
//! Layout mirrors the other rigs at a high level: a **preset browser** on the
//! left ("Bass" / "Synth Bass" — presets of the one engine), the live
//! **chain** (DI → blocks → out) with in/out meters and the master trim in
//! the middle, and the MIDI monitor below.

use dioxus::prelude::*;
use midicore_proto::MidiEvent;
use midicore_ui::MidiMonitorPanel;
use signal_bass_proto::bass::{BassEvent, BassRigClient, BassRigStreamClient};
use signal_bass_proto::{BassBlock, BassPreset, BassStatus, PresetKind};

/// Live bass-rig view-state: seeded once, then folded from the event stream.
#[derive(Clone, Copy)]
struct BassState {
    status: Signal<BassStatus>,
    presets: Signal<Vec<BassPreset>>,
    chain: Signal<Vec<BassBlock>>,
    midi: Signal<Vec<MidiEvent>>,
}

fn use_bass_state() -> (BassState, Option<BassRigClient>) {
    let rig = use_hook(try_consume_context::<BassRigClient>);
    let stream = use_hook(try_consume_context::<BassRigStreamClient>);

    let mut status = use_signal(BassStatus::default);
    let mut presets = use_signal(Vec::<BassPreset>::new);
    let mut chain = use_signal(Vec::<BassBlock>::new);
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
                if let Ok(c) = rig.chain().await {
                    chain.set(c);
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
            move |ev: BassEvent| {
                let (mut status, mut presets, mut chain, mut midi) = (status, presets, chain, midi);
                match ev {
                    BassEvent::Status(s) => status.set(s),
                    BassEvent::Library(p) => presets.set(p),
                    BassEvent::Chain(c) => chain.set(c),
                    BassEvent::Midi(m) => midi.set(m),
                }
            },
        );
    }

    (
        BassState {
            status,
            presets,
            chain,
            midi,
        },
        rig,
    )
}

/// The bass-rig remote view. Mount inside a host that has provided
/// `BassRigClient` + `BassRigStreamClient` in context.
#[component]
pub fn BassRigRemote() -> Element {
    let (state, rig) = use_bass_state();
    let status = state.status.read().clone();
    let presets = state.presets.read().clone();
    let chain = state.chain.read().clone();
    let midi = state.midi.read().clone();
    let midi_count = midi.len() as u64;

    let trim = status.master_trim_db;

    // The rig's readouts live in the app bar (fts_chrome); what stays here is
    // control — the master trim and the engine's own start/stop, which are
    // things you *do*, not things you read.
    let level = fts_chrome::use_chrome_level(2);
    level.status(vec![
        fts_chrome::StatusItem::dot(status.running, "#22c55e"),
        fts_chrome::StatusItem::text(status.active_preset.clone().unwrap_or_else(|| "—".into())),
        fts_chrome::StatusItem::text("IN"),
        fts_chrome::StatusItem::meter(status.input_peak, "#38bdf8"),
        fts_chrome::StatusItem::text("OUT"),
        fts_chrome::StatusItem::meter(status.output_peak, "#22c55e"),
    ]);

    rsx! {
        div { style: "display:flex; flex-direction:column; gap:0; flex:1; min-height:0; color:#e4e4e7; font-family:sans-serif;",
            div { style: "display:flex; align-items:center; gap:10px; padding:10px 14px 4px;",
                span { style: "font-size:9px; color:#71717a;", "TRIM {trim:+.1} dB" }
                {
                    let rig = rig.clone();
                    rsx! { input {
                        r#type: "range",
                        min: "-24",
                        max: "12",
                        step: "0.5",
                        value: "{trim}",
                        style: "width:110px;",
                        oninput: move |e| {
                            let rig = rig.clone();
                            if let Ok(db) = e.value().parse::<f32>() {
                                spawn(async move { if let Some(r) = rig { let _ = r.set_master_trim(db).await; } });
                            }
                        },
                    } }
                }
                div { style: "flex:1;" }
                {
                    let rig = rig.clone();
                    let running = status.running;
                    rsx! { button {
                        style: "padding:2px 10px; border-radius:5px; background:transparent; color:#a1a1aa; border:1px solid #27272a; font-size:11px; cursor:pointer;",
                        onclick: move |_| {
                            let rig = rig.clone();
                            spawn(async move {
                                if let Some(r) = rig {
                                    let _ = if running { r.stop().await } else { r.start().await };
                                }
                            });
                        },
                        if running { "Stop" } else { "Start" }
                    } }
                }
            }
            div { style: "display:flex; gap:12px; flex:1; min-height:0;",
                // ── preset browser (left) — presets of the one engine ──
                div { style: "display:flex; flex-direction:column; gap:4px; width:220px; min-width:220px; overflow:auto; border-right:1px solid #1c1c1f; padding:8px;",
                    span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Presets ({presets.len()})" }
                    for (i, preset) in presets.iter().enumerate() {
                        {
                            let rig = rig.clone();
                            let enabled = preset.available;
                            rsx! { button {
                                key: "{preset.name}",
                                style: preset_btn(preset.active, preset.available),
                                onclick: move |_| {
                                    if !enabled { return; }
                                    let rig = rig.clone();
                                    spawn(async move { if let Some(r) = rig { let _ = r.select_preset(i as u32).await; } });
                                },
                                span { style: "font-size:12px; font-weight:600;", "{preset.name}" }
                                span { style: "font-size:9px; color:#71717a;",
                                    {match (preset.kind, preset.available) {
                                        (PresetKind::Sample, _) => "sampled — coming".to_string(),
                                        (PresetKind::Audio, false) => format!("{} · missing capture", preset.summary),
                                        (PresetKind::Audio, true) => preset.summary.clone(),
                                    }}
                                }
                            } }
                        }
                    }
                }
                // ── chain + MIDI monitor ──
                div { style: "display:flex; flex-direction:column; gap:12px; flex:1; min-height:0; overflow:auto; padding:10px;",
                    div {
                        span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Chain" }
                        div { style: "display:flex; align-items:center; gap:8px; margin-top:6px; flex-wrap:wrap;",
                            ChainEndcap { label: "DI".to_string() }
                            if chain.is_empty() {
                                span { style: "font-size:11px; color:#52525b;", "→ clean passthrough →" }
                            }
                            for block in chain.iter() {
                                {
                                    let rig = rig.clone();
                                    let id = block.id.clone();
                                    rsx! {
                                        span { key: "{block.id}", style: "color:#3f3f46;", "→" }
                                        button {
                                            style: block_btn(block.bypassed),
                                            onclick: move |_| {
                                                let (rig, id) = (rig.clone(), id.clone());
                                                spawn(async move { if let Some(r) = rig { let _ = r.toggle_block_bypass(id).await; } });
                                            },
                                            span { style: "font-size:12px; font-weight:600;", "{block.name}" }
                                            span { style: "font-size:9px; color:#71717a;", {format!("{:?}", block.block_type)} }
                                        }
                                    }
                                }
                            }
                            span { style: "color:#3f3f46;", "→" }
                            ChainEndcap { label: "OUT".to_string() }
                        }
                        if !chain.is_empty() {
                            span { style: "font-size:10px; color:#52525b;", "tap a block to bypass it" }
                        }
                    }
                    MidiMonitorPanel { events: midi, count: midi_count, title: "MIDI monitor (program change / footswitch)".to_string() }
                }
            }
        }
    }
}

#[component]
fn ChainEndcap(label: String) -> Element {
    rsx! {
        div { style: "padding:8px 10px; border-radius:8px; background:#0c0c0e; border:1px solid #27272a; color:#71717a; font-size:11px; font-weight:700;",
            "{label}"
        }
    }
}

fn preset_btn(active: bool, available: bool) -> String {
    let (bg, br, fg, cursor) = match (active, available) {
        (true, _) => ("#122a1c", "#22c55e", "#e4e4e7", "pointer"),
        (false, true) => ("#111113", "#27272a", "#a1a1aa", "pointer"),
        (false, false) => ("#0c0c0e", "#1c1c1f", "#52525b", "default"),
    };
    format!(
        "display:flex; flex-direction:column; text-align:left; padding:6px 8px; border-radius:6px; background:{bg}; color:{fg}; border:1px solid {br}; font-size:12px; cursor:{cursor};"
    )
}

fn block_btn(bypassed: bool) -> String {
    let (bg, br, fg) = if bypassed {
        ("#0c0c0e", "#3f3f46", "#52525b")
    } else {
        ("#14321e", "#166534", "#e4e4e7")
    };
    format!(
        "display:flex; flex-direction:column; align-items:flex-start; padding:8px 10px; border-radius:8px; background:{bg}; color:{fg}; border:1px solid {br}; cursor:pointer;"
    )
}
