//! **Control view** — the mixer.
//!
//! Where the guitar rig's Control view is a pedalboard (FX blocks and their
//! params), the keys rig's is a console: one strip per layer, grouped under
//! its engine, plus an engine master and the rig master. This is the surface
//! a keys player actually performs on — ride the pad under a verse, pull the
//! piano back for a hook, mute the organ until the last chorus.
//!
//! Each strip's patch button opens the library picker: choosing a preset
//! loads that pack into the layer's Sampler block — the block/module system's
//! ordinary load path, addressed by layer.

use dioxus::prelude::*;
use signal_keys_proto::keys::KeysRigClient;
use signal_keys_proto::{KeysEngineModel, KeysLayerModel, KeysMixer, KeysPreset};

use crate::fader::{Fader, fmt_db};
use crate::zoom::{OpenButton, Zoom};

/// Accent per engine — the same color language the Perform strip uses.
pub fn engine_color(name: &str) -> &'static str {
    match name {
        "Keys" => "#38bdf8",
        "Synth" => "#a78bfa",
        "Organ" => "#fb923c",
        "Pad" => "#34d399",
        _ => "#94a3b8",
    }
}

/// The mixer. `presets` feeds the per-layer patch picker.
#[component]
pub fn ControlView(mixer: KeysMixer, presets: Vec<KeysPreset>) -> Element {
    // Which layer's patch picker is open.
    let picking = use_signal(|| None::<String>);

    rsx! {
        div {
            style: "flex: 1; min-height: 0; overflow: auto; padding: 12px; display: flex; \
                    align-items: flex-start; gap: 14px;",
            for engine in mixer.engines.iter() {
                EngineStrip {
                    key: "{engine.name}",
                    engine: engine.clone(),
                    presets: presets.clone(),
                    picking,
                }
            }
            div { style: "flex: 1;" }
            MasterStrip { master_db: mixer.master_db }
        }
    }
}

/// One engine: its layers side by side, then the engine's own fader.
#[component]
fn EngineStrip(
    engine: KeysEngineModel,
    presets: Vec<KeysPreset>,
    picking: Signal<Option<String>>,
) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let mut zoom = crate::zoom::use_zoom();
    let accent = engine_color(&engine.name);
    let muted = engine.muted;
    let name = engine.name.clone();
    let open_name = engine.name.clone();
    let dbl_name = engine.name.clone();

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 8px; padding: 10px; \
                    border: 1px solid #1f1f23; border-radius: 12px; background: #0e0e11;",
            // Double-clicking the card body (not a control) zooms in.
            ondoubleclick: move |_| zoom.set(Zoom::Engine(dbl_name.clone())),
            // Engine header: name + mute.
            div { style: "display: flex; align-items: center; gap: 8px;",
                span {
                    style: "width: 8px; height: 8px; border-radius: 2px; background: {accent};",
                }
                span { style: "font-size: 12px; font-weight: 700; color: #e4e4e7;", "{engine.name}" }
                div { style: "flex: 1;" }
                OpenButton {
                    title: format!("Open {}", engine.name),
                    on_open: move |_| zoom.set(Zoom::Engine(open_name.clone())),
                }
                button {
                    style: mute_style(muted),
                    onclick: {
                        let rig = rig.clone();
                        let name = name.clone();
                        move |_| {
                            let (rig, name) = (rig.clone(), name.clone());
                            spawn(async move {
                                if let Some(r) = rig {
                                    let _ = r.set_engine_mute(name, !muted).await;
                                }
                            });
                        }
                    },
                    "M"
                }
            }
            // Layer strips.
            div { style: "display: flex; gap: 10px; align-items: flex-start;",
                for layer in engine.layers.iter() {
                    LayerStrip {
                        key: "{layer.name}",
                        layer: layer.clone(),
                        accent: accent.to_string(),
                        presets: presets.clone(),
                        picking,
                    }
                }
            }
            // Engine fader — rides every layer under it.
            div { style: "display: flex; align-items: center; gap: 8px; padding-top: 6px; \
                          border-top: 1px solid #1c1c20;",
                span { style: "font-size: 9px; letter-spacing: 0.1em; color: #52525b; text-transform: uppercase;",
                    "Engine"
                }
                div { style: "flex: 1;" }
                Fader {
                    db: engine.gain_db,
                    height_px: 54,
                    accent: accent.to_string(),
                    dimmed: muted,
                    on_change: {
                        let rig = rig.clone();
                        let name = name.clone();
                        move |db: f32| {
                            let (rig, name) = (rig.clone(), name.clone());
                            spawn(async move {
                                if let Some(r) = rig {
                                    let _ = r.set_engine_gain(name, db).await;
                                }
                            });
                        }
                    },
                }
            }
        }
    }
}

/// One layer lane: patch button, fader, mute/solo.
#[component]
fn LayerStrip(
    layer: KeysLayerModel,
    accent: String,
    presets: Vec<KeysPreset>,
    picking: Signal<Option<String>>,
) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let mut zoom = crate::zoom::use_zoom();
    let mut picking = picking;
    let name = layer.name.clone();
    let open_lane = layer.name.clone();
    let dbl_lane = layer.name.clone();
    let open = picking().as_deref() == Some(name.as_str());
    let patch_label = if layer.patch.is_empty() { "empty".to_string() } else { layer.patch.clone() };
    let split = if layer.key_lo == 0 && layer.key_hi == 127 {
        String::new()
    } else {
        format!("{}–{}", note_name(layer.key_lo), note_name(layer.key_hi))
    };

    rsx! {
        div {
            style: "position: relative; display: flex; flex-direction: column; align-items: center; \
                    gap: 6px; width: 92px;",
            ondoubleclick: move |_| zoom.set(Zoom::Layer(dbl_lane.clone())),
            // Lane name + key split.
            div { style: "display: flex; flex-direction: column; align-items: center; gap: 1px;",
                span { style: "font-size: 11px; font-weight: 600; color: #d4d4d8;", "{layer.name}" }
                if !split.is_empty() {
                    span { style: "font-size: 8px; color: #52525b;", "{split}" }
                }
            }
            // Patch button — opens the picker.
            button {
                style: format!(
                    "width: 100%; appearance: none; border: 1px solid {}; border-radius: 8px; \
                     padding: 5px 6px; background: {}; color: {}; font-size: 9px; font-weight: 600; \
                     line-height: 1.2; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                    if layer.live { "#2b3a4d" } else { "#26262b" },
                    if layer.live { "#101821" } else { "#131316" },
                    if layer.live { "#7dd3fc" } else { "#52525b" },
                ),
                onclick: move |_| {
                    let cur = picking();
                    picking.set(if cur.as_deref() == Some(name.as_str()) {
                        None
                    } else {
                        Some(name.clone())
                    });
                },
                "{patch_label}"
            }
            Fader {
                db: layer.gain_db,
                height_px: 120,
                accent: accent.clone(),
                dimmed: layer.muted || !layer.live,
                on_change: {
                    let rig = rig.clone();
                    let lane = layer.name.clone();
                    move |db: f32| {
                        let (rig, lane) = (rig.clone(), lane.clone());
                        spawn(async move {
                            if let Some(r) = rig {
                                let _ = r.set_layer_gain(lane, db).await;
                            }
                        });
                    }
                },
            }
            // Open the layer zoom — quiet, beneath the fader.
            OpenButton {
                title: format!("Open {}", layer.name),
                on_open: move |_| zoom.set(Zoom::Layer(open_lane.clone())),
            }
            // Mute / solo.
            div { style: "display: flex; gap: 4px;",
                button {
                    style: mute_style(layer.muted),
                    onclick: {
                        let rig = rig.clone();
                        let lane = layer.name.clone();
                        let muted = layer.muted;
                        move |_| {
                            let (rig, lane) = (rig.clone(), lane.clone());
                            spawn(async move {
                                if let Some(r) = rig {
                                    let _ = r.set_layer_mute(lane, !muted).await;
                                }
                            });
                        }
                    },
                    "M"
                }
                button {
                    style: solo_style(layer.soloed),
                    onclick: {
                        let rig = rig.clone();
                        let lane = layer.name.clone();
                        let soloed = layer.soloed;
                        move |_| {
                            let (rig, lane) = (rig.clone(), lane.clone());
                            spawn(async move {
                                if let Some(r) = rig {
                                    let _ = r.set_layer_solo(lane, !soloed).await;
                                }
                            });
                        }
                    },
                    "S"
                }
            }
            // The library picker, anchored under the strip.
            if open {
                PatchPicker {
                    layer: layer.name.clone(),
                    presets: presets.clone(),
                    picking,
                }
            }
        }
    }
}

/// The per-layer library picker — every preset in the library, plus Clear.
#[component]
fn PatchPicker(
    layer: String,
    presets: Vec<KeysPreset>,
    picking: Signal<Option<String>>,
) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let mut picking = picking;
    rsx! {
        div {
            style: "position: absolute; top: 100%; left: 0; z-index: 40; width: 220px; max-height: 280px; \
                    overflow-y: auto; display: flex; flex-direction: column; gap: 2px; padding: 6px; \
                    background: #0b0b0d; border: 1px solid #2b2b31; border-radius: 10px; \
                    box-shadow: 0 12px 32px #000c;",
            span { style: "font-size: 9px; letter-spacing: 0.1em; text-transform: uppercase; color: #52525b; padding: 2px 4px;",
                "Load into {layer}"
            }
            button {
                style: "appearance: none; text-align: left; border: none; border-radius: 6px; \
                        padding: 6px 8px; background: transparent; color: #a1a1aa; font-size: 11px;",
                onclick: {
                    let rig = rig.clone();
                    let layer = layer.clone();
                    move |_| {
                        let (rig, layer) = (rig.clone(), layer.clone());
                        picking.set(None);
                        spawn(async move {
                            if let Some(r) = rig {
                                let _ = r.clear_layer(layer).await;
                            }
                        });
                    }
                },
                "— empty —"
            }
            for (i, preset) in presets.iter().enumerate() {
                button {
                    key: "{preset.name}",
                    style: "appearance: none; text-align: left; border: none; border-radius: 6px; \
                            padding: 6px 8px; background: transparent; color: #e4e4e7; font-size: 11px; \
                            display: flex; flex-direction: column; gap: 1px;",
                    onclick: {
                        let rig = rig.clone();
                        let layer = layer.clone();
                        move |_| {
                            let (rig, layer) = (rig.clone(), layer.clone());
                            picking.set(None);
                            spawn(async move {
                                if let Some(r) = rig {
                                    let _ = r.set_layer_patch(layer, i as u32).await;
                                }
                            });
                        }
                    },
                    span { "{preset.name}" }
                    span { style: "font-size: 9px; color: #52525b;", "{preset.kind}" }
                }
            }
        }
    }
}

/// The rig master strip.
#[component]
fn MasterStrip(master_db: f32) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    rsx! {
        div {
            style: "display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 10px 14px; \
                    border: 1px solid #1f1f23; border-radius: 12px; background: #0e0e11;",
            span { style: "font-size: 11px; font-weight: 700; color: #e4e4e7;", "Master" }
            Fader {
                db: master_db,
                height_px: 160,
                accent: "#e4e4e7".to_string(),
                on_change: move |db: f32| {
                    let rig = rig.clone();
                    spawn(async move {
                        if let Some(r) = rig {
                            let _ = r.set_master_gain(db).await;
                        }
                    });
                },
            }
            span { style: "font-size: 9px; color: #52525b;", {fmt_db(master_db)} }
        }
    }
}

fn mute_style(on: bool) -> String {
    format!(
        "appearance: none; border: 1px solid {}; border-radius: 5px; width: 24px; height: 20px; \
         background: {}; color: {}; font-size: 9px; font-weight: 700;",
        if on { "#7f1d1d" } else { "#26262b" },
        if on { "#3f1414" } else { "#131316" },
        if on { "#fca5a5" } else { "#52525b" },
    )
}

fn solo_style(on: bool) -> String {
    format!(
        "appearance: none; border: 1px solid {}; border-radius: 5px; width: 24px; height: 20px; \
         background: {}; color: {}; font-size: 9px; font-weight: 700;",
        if on { "#a16207" } else { "#26262b" },
        if on { "#3b2708" } else { "#131316" },
        if on { "#fde047" } else { "#52525b" },
    )
}

/// MIDI note → name, for key-split labels.
fn note_name(note: u32) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let n = note as i32;
    format!("{}{}", NAMES[(n % 12) as usize], n / 12 - 1)
}
