//! **Perform strip** — the footswitch row that lives at the bottom of every
//! view, exactly like the guitar rig's.
//!
//! A keys stack is a *scene*: pressing "Verse" rides every layer to that
//! stack's levels (and loads any patch the scene pins). Level-only recalls
//! are instant — the mixer's live cells, no audio gap.

use dioxus::prelude::*;
use signal_keys_proto::keys::KeysRigClient;
use signal_keys_proto::KeysPerform;

/// Per-stack color — the worship set's shape, left to right: intimate →
/// full → intimate again.
pub fn stack_color(name: &str) -> (&'static str, &'static str) {
    match name {
        "Spotlight" => ("#1e3a5f", "#7dd3fc"),
        "Verse" => ("#14324a", "#67e8f9"),
        "Energy" => ("#3b2708", "#fde047"),
        "Hooks" => ("#3f1d38", "#f0abfc"),
        "Underscore" => ("#14321e", "#86efac"),
        _ => ("#1c1c20", "#a1a1aa"),
    }
}

/// The stack row + the perform-mode selector.
#[component]
pub fn PerformStrip(perform: KeysPerform) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 6px; padding: 8px 12px; \
                    border-top: 1px solid #1c1c1f; background: #0a0a0c;",
            // Mode selector — Preset browses the library, Profile plays the
            // stacks, Setlist follows the song.
            div { style: "display: flex; align-items: center; gap: 6px;",
                for (mode, label) in [(0u32, "Preset"), (1, "Profile"), (2, "Setlist")] {
                    button {
                        key: "{label}",
                        style: format!(
                            "appearance: none; border: none; border-radius: 6px; padding: 3px 10px; \
                             font-size: 10px; font-weight: 700; letter-spacing: 0.06em; background: {}; color: {};",
                            if perform.perform_mode == mode { "#101821" } else { "transparent" },
                            if perform.perform_mode == mode { "#38bdf8" } else { "#52525b" },
                        ),
                        onclick: {
                            let rig = rig.clone();
                            move |_| {
                                let rig = rig.clone();
                                spawn(async move {
                                    if let Some(r) = rig {
                                        let _ = r.set_perform_mode(mode).await;
                                    }
                                });
                            }
                        },
                        "{label}"
                    }
                }
                div { style: "flex: 1;" }
                span { style: "font-size: 10px; color: #52525b;", "{perform.profile_name}" }
            }
            // The footswitch row.
            div { style: "display: flex; gap: 8px;",
                for (i, stack) in perform.stacks.iter().enumerate() {
                    {
                        let (bg, fg) = stack_color(&stack.name);
                        let active = stack.is_active;
                        let rig = rig.clone();
                        rsx! {
                            button {
                                key: "{stack.name}",
                                style: format!(
                                    "flex: 1; position: relative; appearance: none; text-align: left; \
                                     border: 1px solid {}; border-radius: 10px; padding: 10px 12px; \
                                     background: {}; color: {}; display: flex; flex-direction: column; gap: 3px; \
                                     min-height: 58px; overflow: hidden;",
                                    if active { fg } else { "#1f1f23" },
                                    if active { bg } else { "#111114" },
                                    if active { fg } else { "#71717a" },
                                ),
                                onclick: move |_| {
                                    let rig = rig.clone();
                                    spawn(async move {
                                        if let Some(r) = rig {
                                            let _ = r.press_stack(i as u32).await;
                                        }
                                    });
                                },
                                // Edge LED, like the stompbox rows.
                                if active {
                                    span {
                                        style: format!(
                                            "position: absolute; left: 0; top: 10px; bottom: 10px; width: 3px; \
                                             border-radius: 0 2px 2px 0; background: {fg}; box-shadow: 0 0 10px {fg};",
                                        ),
                                    }
                                }
                                span { style: "font-size: 12px; font-weight: 700; letter-spacing: 0.04em;",
                                    {stack.name.to_uppercase()}
                                }
                                span { style: "font-size: 9px; opacity: 0.75; line-height: 1.25;", "{stack.blurb}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
