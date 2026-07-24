//! **Engine zoom** — one engine fullscreen: its lanes with room to breathe,
//! each opening into the layer zoom.

use dioxus::prelude::*;
use signal_keys_proto::keys::KeysRigClient;
use signal_keys_proto::{KeysEngineModel, KeysPreset};

use crate::control::engine_color;
use crate::fader::{Fader, fmt_db};
use crate::zoom::{OpenButton, Zoom, ZoomBar};

#[component]
pub fn EngineView(
    engine: KeysEngineModel,
    presets: Vec<KeysPreset>,
    zoom: Signal<Zoom>,
) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let mut zoom = zoom;
    let accent = engine_color(&engine.name).to_string();
    let live = engine.layers.iter().filter(|l| l.live && !l.muted).count();

    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
            ZoomBar {
                crumbs: vec![engine.name.clone()],
                on_back: move |_| zoom.set(Zoom::Mixer),
                trailing: rsx! {
                    div { style: "display: flex; align-items: center; gap: 10px;",
                        span { style: "font-size: 10px; color: #52525b;",
                            {format!("{live} of {} lanes live", engine.layers.len())}
                        }
                        span { style: "font-size: 10px; color: #71717a;", "Engine" }
                        div { style: "width: 120px;",
                            Fader {
                                db: engine.gain_db,
                                height_px: 26,
                                accent: accent.clone(),
                                dimmed: engine.muted,
                                on_change: {
                                    let rig = rig.clone();
                                    let name = engine.name.clone();
                                    move |db: f32| {
                                        let (rig, name) = (rig.clone(), name.clone());
                                        spawn(async move {
                                            if let Some(r) = rig { let _ = r.set_engine_gain(name, db).await; }
                                        });
                                    }
                                },
                            }
                        }
                    }
                },
            }
            div {
                style: "flex: 1; min-height: 0; overflow: auto; padding: 16px; \
                        display: flex; gap: 16px; align-items: flex-start; flex-wrap: wrap;",
                for layer in engine.layers.iter() {
                    {
                        let lane = layer.name.clone();
                        let open_lane = layer.name.clone();
                        let patch = if layer.patch.is_empty() {
                            "empty lane".to_string()
                        } else {
                            layer.patch.clone()
                        };
                        let rig_gain = rig.clone();
                        let rig_mute = rig.clone();
                        let muted = layer.muted;
                        let mute_lane = layer.name.clone();
                        rsx! {
                            div {
                                key: "{layer.name}",
                                style: "display: flex; flex-direction: column; gap: 10px; padding: 14px; \
                                        width: 210px; border: 1px solid #1f1f23; border-radius: 14px; \
                                        background: #0e0e11; cursor: pointer;",
                                ondoubleclick: {
                                    let open_lane = open_lane.clone();
                                    move |_| zoom.set(Zoom::Layer(open_lane.clone()))
                                },
                                div { style: "display: flex; align-items: center; gap: 8px;",
                                    span { style: "font-size: 13px; font-weight: 700; color: #e4e4e7;", "{layer.name}" }
                                    div { style: "flex: 1;" }
                                    OpenButton {
                                        title: format!("Open {}", layer.name),
                                        size: 14,
                                        on_open: {
                                            let open_lane = open_lane.clone();
                                            move |_| zoom.set(Zoom::Layer(open_lane.clone()))
                                        },
                                    }
                                }
                                span {
                                    style: format!(
                                        "font-size: 11px; color: {}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                        if layer.live { "#7dd3fc" } else { "#52525b" },
                                    ),
                                    "{patch}"
                                }
                                div { style: "display: flex; align-items: flex-end; gap: 12px;",
                                    Fader {
                                        db: layer.gain_db,
                                        height_px: 130,
                                        accent: accent.clone(),
                                        dimmed: layer.muted || !layer.live,
                                        on_change: {
                                            let lane = lane.clone();
                                            move |db: f32| {
                                                let (rig, lane) = (rig_gain.clone(), lane.clone());
                                                spawn(async move {
                                                    if let Some(r) = rig { let _ = r.set_layer_gain(lane, db).await; }
                                                });
                                            }
                                        },
                                    }
                                    div { style: "display: flex; flex-direction: column; gap: 6px;",
                                        span { style: "font-size: 10px; color: #71717a;", {fmt_db(layer.gain_db)} }
                                        span { style: "font-size: 9px; color: #3f3f46;",
                                            {format!("{}–{}", layer.key_lo, layer.key_hi)}
                                        }
                                        button {
                                            style: format!(
                                                "appearance: none; border: 1px solid {}; border-radius: 6px; \
                                                 padding: 4px 10px; background: {}; color: {}; font-size: 10px; font-weight: 700;",
                                                if muted { "#7f1d1d" } else { "#26262b" },
                                                if muted { "#3f1414" } else { "#131316" },
                                                if muted { "#fca5a5" } else { "#52525b" },
                                            ),
                                            onclick: {
                                                let mute_lane = mute_lane.clone();
                                                move |e: MouseEvent| {
                                                    e.stop_propagation();
                                                    let (rig, lane) = (rig_mute.clone(), mute_lane.clone());
                                                    spawn(async move {
                                                        if let Some(r) = rig {
                                                            let _ = r.set_layer_mute(lane, !muted).await;
                                                        }
                                                    });
                                                }
                                            },
                                            "MUTE"
                                        }
                                    }
                                }
                                // The lane's modules — see at a glance what
                                // makes up this layer, and balance them here.
                                div {
                                    style: "display: flex; gap: 6px; padding-top: 6px; \
                                            border-top: 1px solid #1c1c20;",
                                    for m in layer.modules.iter() {
                                        {
                                            let rig_m = rig.clone();
                                            let lane_m = layer.name.clone();
                                            let idx = m.index;
                                            let on = m.enabled;
                                            let label = if m.patch.is_empty() {
                                                "—".to_string()
                                            } else {
                                                m.patch.clone()
                                            };
                                            rsx! {
                                                div {
                                                    key: "{m.index}",
                                                    style: "display: flex; flex-direction: column; \
                                                            align-items: center; gap: 3px; flex: 1; min-width: 0;",
                                                    title: "{label}",
                                                    span {
                                                        style: format!(
                                                            "font-size: 9px; font-weight: 700; color: {};",
                                                            if m.live && on { accent.clone() } else { "#3f3f46".to_string() },
                                                        ),
                                                        "{m.slot}"
                                                    }
                                                    Fader {
                                                        db: m.gain_db,
                                                        height_px: 56,
                                                        accent: accent.clone(),
                                                        dimmed: !on || !m.live,
                                                        on_change: move |db: f32| {
                                                            let (rig, lane) = (rig_m.clone(), lane_m.clone());
                                                            spawn(async move {
                                                                if let Some(r) = rig {
                                                                    let _ = r.set_module_gain(lane, idx, db).await;
                                                                }
                                                            });
                                                        },
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                button {
                                    style: "appearance: none; border: 1px solid #1f2b3a; border-radius: 8px; \
                                            padding: 6px; background: #0d1319; color: #7dd3fc; font-size: 10px; \
                                            font-weight: 700; letter-spacing: 0.06em;",
                                    onclick: {
                                        let open_lane = open_lane.clone();
                                        move |e: MouseEvent| {
                                            e.stop_propagation();
                                            zoom.set(Zoom::Layer(open_lane.clone()));
                                        }
                                    },
                                    "OPEN LAYER"
                                }
                            }
                        }
                    }
                }
                if !presets.is_empty() {
                    span { style: "font-size: 10px; color: #3f3f46; align-self: flex-end;",
                        {format!("{} patches in the library", presets.len())}
                    }
                }
            }
        }
    }
}
