//! **The preset sidebar** — the left sidebar in Preset mode.
//!
//! Preset mode plays the pool directly (a click is `play_preset`, no stacks),
//! so the sidebar is the pool: a search field and every amp preset in the
//! active profile, the one sounding lit.
//!
//! ```text
//! PRESETS · Worship
//! [ search presets…      ]
//! ● Fender Clean     3 patches   ← sounding
//!   AA Crunch        2 patches
//!   Arena Lead       tone3000 · 3 patches
//! [ Library ] [ Get tones ]
//! ```
//!
//! Enter in the search plays the first match, so "arena⏎" is a preset
//! change without the mouse. Managing presets (rename, delete, repoint) is
//! the library's; new ones come from Tones.

use dioxus::prelude::*;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{PerformanceModel, PresetInfo};

use crate::library::Kind;

const LINE: &str = "#1f1f24";
const TEXT: &str = "#e4e4e7";
const MUTED: &str = "#a1a1aa";
const FAINT: &str = "#63636b";
const ON_BG: &str = "#1b2331";
const ON_FG: &str = "#bfdbfe";
const LIVE: &str = "#22c55e";

/// Every word of the query in the preset's name or credits.
fn matches(p: &PresetInfo, query: &str) -> bool {
    let hay = format!("{} {} {}", p.name, p.creator, p.gear).to_lowercase();
    query.split_whitespace().all(|w| hay.contains(&w.to_lowercase()))
}

#[component]
pub fn PresetSidebar(
    model: PerformanceModel,
    on_browse: EventHandler<Kind>,
    on_tones: EventHandler<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut query = use_signal(String::new);

    let mut rev = use_signal(|| model.revision);
    if *rev.peek() != model.revision {
        rev.set(model.revision);
    }
    let presets = use_resource({
        let rig = rig.clone();
        move || {
            let _ = rev();
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => r.presets().await.unwrap_or_default(),
                    None => Vec::new(),
                }
            }
        }
    });
    let pool: Vec<PresetInfo> = presets.read().clone().unwrap_or_default();
    let q = query();
    // Keep the pool index: `play_preset` addresses the pool by position.
    let hits: Vec<(usize, PresetInfo)> = pool
        .iter()
        .cloned()
        .enumerate()
        .filter(|(_, p)| q.trim().is_empty() || matches(p, &q))
        .collect();

    let play = {
        let rig = rig.clone();
        move |i: usize| {
            if let Some(r) = rig.clone() {
                spawn(async move {
                    let _ = r.play_preset(i as u32).await;
                });
            }
        }
    };

    rsx! {
        aside {
            style: "width: 272px; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; \
                    border-right: 1px solid {LINE}; background: #0e0e11; color: {TEXT};",
            div { style: "display: flex; flex-direction: column; gap: 8px; padding: 12px 12px 10px; \
                          border-bottom: 1px solid {LINE}; flex-shrink: 0;",
                div { style: "display: flex; align-items: baseline; gap: 6px;",
                    span { style: "font-size: 9px; font-weight: 700; letter-spacing: 0.14em; \
                                   text-transform: uppercase; color: {FAINT};",
                        "Presets"
                    }
                    span { style: "font-size: 11px; color: {FAINT}; white-space: nowrap; overflow: hidden;",
                        "· {model.profile_name}"
                    }
                    div { style: "flex: 1;" }
                    span { style: "font-size: 10px; color: {FAINT};",
                        if q.trim().is_empty() { "{pool.len()}" } else { "{hits.len()}/{pool.len()}" }
                    }
                }
                input {
                    style: "width: 100%; font-size: 12px; color: {TEXT}; background: #0a0a0d; \
                            border: 1px solid #2a2a31; border-radius: 7px; padding: 6px 9px; outline: none;",
                    placeholder: "Search presets…",
                    value: "{q}",
                    oninput: move |e| query.set(e.value()),
                    onkeydown: {
                        let first = hits.first().map(|(i, _)| *i);
                        let play = play.clone();
                        move |e: KeyboardEvent| match e.key() {
                            // The first match plays: type, Enter, playing.
                            Key::Enter => {
                                if let Some(i) = first {
                                    play(i);
                                }
                            }
                            Key::Escape => query.set(String::new()),
                            _ => {}
                        }
                    },
                }
            }
            div { style: "flex: 1 1 0; min-height: 0; overflow-y: scroll; padding: 6px 8px;",
                if hits.is_empty() {
                    div { style: "padding: 10px 6px; font-size: 12px; color: {FAINT}; line-height: 1.5;",
                        if pool.is_empty() { "No presets in this profile yet." } else { "No preset matches." }
                    }
                }
                for (i, p) in hits.iter().cloned() {
                    {
                        let play = play.clone();
                        let credit = match (p.creator.is_empty(), p.used_by) {
                            (true, 1) => "1 patch".to_string(),
                            (true, n) => format!("{n} patches"),
                            (false, 1) => format!("{} · 1 patch", p.creator),
                            (false, n) => format!("{} · {n} patches", p.creator),
                        };
                        rsx! {
                            button {
                                key: "{i}-{p.name}",
                                style: format!(
                                    "display: flex; align-items: center; gap: 8px; width: 100%; padding: 7px 8px; \
                                     margin-bottom: 1px; border-radius: 7px; border: none; cursor: pointer; \
                                     justify-content: flex-start; text-align: left; background: {}; color: {};",
                                    if p.active { ON_BG } else { "transparent" },
                                    if p.active { ON_FG } else { TEXT },
                                ),
                                onclick: move |_| play(i),
                                span {
                                    style: format!(
                                        "width: 6px; height: 6px; border-radius: 999px; flex-shrink: 0; background: {};",
                                        if p.active { LIVE } else { "transparent" },
                                    ),
                                }
                                span { style: "flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; gap: 1px;",
                                    span { style: "font-size: 12px; font-weight: 600; white-space: nowrap; overflow: hidden;",
                                        "{p.name}"
                                    }
                                    span { style: "font-size: 10px; color: {FAINT}; white-space: nowrap; overflow: hidden;",
                                        "{credit}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { style: "display: flex; gap: 6px; padding: 10px 12px; border-top: 1px solid {LINE}; flex-shrink: 0;",
                button {
                    style: "flex: 1 1 0; min-width: 0; padding: 7px 8px; border-radius: 7px; cursor: pointer; \
                            font-size: 11px; font-weight: 600; border: 1px solid {LINE}; background: transparent; color: {MUTED};",
                    title: "Rename, delete, see what uses each — in the library",
                    onclick: move |_| on_browse.call(Kind::Presets),
                    "Manage"
                }
                button {
                    style: "flex: 1 1 0; min-width: 0; padding: 7px 8px; border-radius: 7px; cursor: pointer; \
                            font-size: 11px; font-weight: 600; border: 1px solid {LINE}; background: transparent; color: {MUTED};",
                    title: "Find a new capture on TONE3000",
                    onclick: move |_| on_tones.call(()),
                    "Get tones"
                }
            }
        }
    }
}
