//! Electronic Kit pad grid (#77 M3). A 4×4 grid of category pads over a
//! built sample space: click to play, per-pad lock / randomize / similarity
//! stepping, plus whole-kit generation and morphing.

use dioxus::prelude::*;
use signal_ekit_proto::ekit::{EkitRigClient, EkitRigStreamClient};
use signal_ekit_proto::{EkitEvent, EkitStatus, Pad};
use signal_space_proto::space::SampleSpaceClient;

use crate::remote::{EngineTarget, establish};

/// Same palette as the space map, so a pad's colour matches its dot.
fn class_color(class: &str) -> &'static str {
    match class {
        "kick" => "#e05252",
        "snare" => "#e0a852",
        "clap" => "#d8e052",
        "hat-closed" => "#52e07e",
        "hat-open" => "#52e0d0",
        "cymbal" => "#52a8e0",
        "tom" => "#a852e0",
        "perc" => "#e052c8",
        "fx" => "#8a8fa3",
        _ => "#555a66",
    }
}

/// Trailing path segment — the pad label wants the sample, not its tree.
fn short_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[component]
pub fn EkitView() -> Element {
    let clients = use_resource(move || async move {
        let target = EngineTarget::current();
        loop {
            let rig: Option<EkitRigClient> = establish(&target).await;
            let stream: Option<EkitRigStreamClient> = establish(&target).await;
            let space: Option<SampleSpaceClient> = establish(&target).await;
            if let (Some(rig), Some(stream), Some(space)) = (rig, stream, space) {
                return (rig, stream, space);
            }
            architect::platform::sleep(std::time::Duration::from_millis(1200)).await;
        }
    });

    let mut pads = use_signal(Vec::<Pad>::new);
    let mut status = use_signal(EkitStatus::default);
    let mut spaces = use_signal(Vec::<String>::new);
    let mut selected = use_signal(|| 0u32);
    let flash = use_signal(|| None::<u32>);

    // Initial fetch + live event stream.
    use_effect(move || {
        if let Some((rig, _stream, space)) = clients() {
            spawn(async move {
                let _ = rig.start().await;
                if let Ok(list) = space.spaces().await {
                    spaces.set(list.into_iter().map(|s| s.name).collect());
                }
                if let Ok(p) = rig.pads().await {
                    pads.set(p);
                }
                if let Ok(s) = rig.status().await {
                    status.set(s);
                }
            });
        }
    });

    // Live pad / status / hit events.
    architect::use_stream(
        move |sink| async move {
            match clients() {
                Some((_, stream, _)) => stream.events(sink).await.is_ok(),
                None => false,
            }
        },
        move |ev: EkitEvent| {
            let (mut pads, mut status, mut flash) = (pads, status, flash);
            match ev {
                EkitEvent::Pads(p) => pads.set(p),
                EkitEvent::Status(s) => status.set(s),
                EkitEvent::Hit(i) => flash.set(Some(i)),
            }
        },
    );

    let rig_of = move || clients().map(|(r, _, _)| r);
    let cols = status().cols.max(1);

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:12px; padding:12px; height:100%; \
                    box-sizing:border-box; color:#e6e8ef; background:#14161c; min-height:0;",
            // ── toolbar ──
            div {
                style: "display:flex; flex-direction:row; gap:8px; align-items:center; flex-wrap:wrap;",
                span { style: "font-size:12px; color:#8a8fa3;", "space:" }
                for name in spaces() {
                    button {
                        style: format!(
                            "padding:4px 10px; border-radius:6px; border:1px solid #333a4a; \
                             background:{}; color:#e6e8ef; cursor:pointer;",
                            if status().space == name { "#2c3242" } else { "#1a1e28" }
                        ),
                        onclick: move |_| {
                            let name = name.clone();
                            if let Some(rig) = rig_of() {
                                spawn(async move { let _ = rig.set_space(name).await; });
                            }
                        },
                        "{name}"
                    }
                }
                div { style: "width:16px;" }
                button {
                    style: "padding:4px 12px; border-radius:6px; border:1px solid #4a6a4a; \
                            background:#1e2a1e; color:#b8e0b8; cursor:pointer; font-weight:600;",
                    onclick: move |_| {
                        if let Some(rig) = rig_of() {
                            spawn(async move { let _ = rig.new_kit().await; });
                        }
                    },
                    "New Kit"
                }
                button {
                    style: "padding:4px 10px; border-radius:6px; border:1px solid #333a4a; \
                            background:#1a1e28; color:#e6e8ef; cursor:pointer;",
                    onclick: move |_| {
                        if let Some(rig) = rig_of() {
                            spawn(async move { let _ = rig.morph_kit(-1).await; });
                        }
                    },
                    "◀ Morph"
                }
                button {
                    style: "padding:4px 10px; border-radius:6px; border:1px solid #333a4a; \
                            background:#1a1e28; color:#e6e8ef; cursor:pointer;",
                    onclick: move |_| {
                        if let Some(rig) = rig_of() {
                            spawn(async move { let _ = rig.morph_kit(1).await; });
                        }
                    },
                    "Morph ▶"
                }
                span {
                    style: "margin-left:auto; font-size:11px; color:#8a8fa3;",
                    {if status().running { "engine running" } else { "engine stopped" }}
                }
            }

            // ── pad grid ──
            div {
                style: format!(
                    "display:grid; grid-template-columns:repeat({cols}, 1fr); gap:8px; \
                     flex:1; min-height:0;"
                ),
                for pad in pads() {
                    {
                        let color = class_color(&pad.category);
                        let is_sel = selected() == pad.index;
                        let hit = flash() == Some(pad.index);
                        let idx = pad.index;
                        let empty = pad.path.is_empty();
                        rsx! {
                            div {
                                style: format!(
                                    "position:relative; display:flex; flex-direction:column; \
                                     justify-content:space-between; padding:8px; border-radius:10px; \
                                     border:{}; background:{}; cursor:pointer; overflow:hidden; \
                                     min-height:80px;",
                                    if is_sel { format!("2px solid {color}") } else { "1px solid #262b38".into() },
                                    if hit { "#2f3646" } else if empty { "#15181f" } else { "#1a1e28" },
                                ),
                                onclick: move |_| {
                                    selected.set(idx);
                                    if let Some(rig) = rig_of() {
                                        spawn(async move { let _ = rig.trigger(idx, 110).await; });
                                    }
                                },
                                // category + locks
                                div {
                                    style: "display:flex; align-items:center; gap:6px; font-size:11px;",
                                    span { style: format!("color:{color}; font-weight:600;"), "{pad.category}" }
                                    button {
                                        style: format!(
                                            "margin-left:auto; padding:0 5px; border-radius:4px; font-size:10px; \
                                             border:1px solid #333a4a; background:{}; color:#e6e8ef; cursor:pointer;",
                                            if pad.locked { "#4a4a2a" } else { "transparent" }
                                        ),
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            let locked = !pad.locked;
                                            if let Some(rig) = rig_of() {
                                                spawn(async move { let _ = rig.set_locked(idx, locked).await; });
                                            }
                                        },
                                        {if pad.locked { "🔒" } else { "🔓" }}
                                    }
                                }
                                // sample name
                                div {
                                    style: "font-size:12px; line-height:1.25; word-break:break-word; \
                                            color:#e6e8ef; flex:1; display:flex; align-items:center;",
                                    title: "{pad.path}",
                                    {if empty { "—".to_string() } else { short_name(&pad.path).to_string() }}
                                }
                                // per-pad ops
                                div {
                                    style: "display:flex; gap:4px; font-size:10px;",
                                    button {
                                        style: "padding:1px 6px; border-radius:4px; border:1px solid #333a4a; \
                                                background:#20242e; color:#e6e8ef; cursor:pointer;",
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            if let Some(rig) = rig_of() {
                                                spawn(async move { let _ = rig.step_similar(idx, -1).await; });
                                            }
                                        },
                                        "◀"
                                    }
                                    button {
                                        style: "padding:1px 6px; border-radius:4px; border:1px solid #333a4a; \
                                                background:#20242e; color:#e6e8ef; cursor:pointer;",
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            if let Some(rig) = rig_of() {
                                                spawn(async move { let _ = rig.randomize_pad(idx).await; });
                                            }
                                        },
                                        "⟳"
                                    }
                                    button {
                                        style: "padding:1px 6px; border-radius:4px; border:1px solid #333a4a; \
                                                background:#20242e; color:#e6e8ef; cursor:pointer;",
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            if let Some(rig) = rig_of() {
                                                spawn(async move { let _ = rig.step_similar(idx, 1).await; });
                                            }
                                        },
                                        "▶"
                                    }
                                    // meter
                                    div {
                                        style: format!(
                                            "margin-left:auto; align-self:center; width:36px; height:4px; \
                                             border-radius:2px; background:#262b38; overflow:hidden;"
                                        ),
                                        div {
                                            style: format!(
                                                "width:{}%; height:100%; background:{color};",
                                                (pad.peak * 140.0).clamp(0.0, 100.0)
                                            ),
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !status().last_error.is_empty() {
                div { style: "font-size:11px; color:#e08080;", "{status().last_error}" }
            }
        }
    }
}
