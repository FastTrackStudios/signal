//! **The preset sidebar** — the left sidebar in Preset mode.
//!
//! Every preset (a composition of module presets) with its snapshots: the
//! playing one open and lit, a click on a snapshot plays it on the current
//! patch, a click on a preset opens it. Enter in the search plays the first
//! match — preset or snapshot — so "deluxe edge⏎" is a sound change
//! without the mouse. Building and editing presets is the library's.

use dioxus::prelude::*;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{CompositionModel, PerformanceModel, PresetEntry};

use crate::library::Kind;

const LINE: &str = "#1f1f24";
const TEXT: &str = "#e4e4e7";
const MUTED: &str = "#a1a1aa";
const FAINT: &str = "#63636b";
const ON_BG: &str = "#1b2331";
const ON_FG: &str = "#bfdbfe";
const LIVE: &str = "#22c55e";

/// Every word of the query in the text.
fn hit(text: &str, query: &str) -> bool {
    let hay = text.to_lowercase();
    query.split_whitespace().all(|w| hay.contains(&w.to_lowercase()))
}

/// The preset's snapshots the query keeps: all of them when the preset's
/// own name matches, else those whose "preset snapshot" text matches.
fn kept(p: &PresetEntry, query: &str) -> Vec<String> {
    if query.trim().is_empty() || hit(&p.name, query) {
        return p.snapshots.iter().map(|s| s.name.clone()).collect();
    }
    p.snapshots
        .iter()
        .filter(|s| hit(&format!("{} {}", p.name, s.name), query))
        .map(|s| s.name.clone())
        .collect()
}

#[component]
pub fn PresetSidebar(
    model: PerformanceModel,
    on_browse: EventHandler<Kind>,
    on_tones: EventHandler<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut query = use_signal(String::new);
    // Presets opened by hand, beyond the playing one.
    let mut opened = use_signal(Vec::<String>::new);

    let mut rev = use_signal(|| model.revision);
    if *rev.peek() != model.revision {
        rev.set(model.revision);
    }
    let comp = use_resource({
        let rig = rig.clone();
        move || {
            let _ = rev();
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => r.compositions().await.unwrap_or_default(),
                    None => CompositionModel::default(),
                }
            }
        }
    });
    let comp: CompositionModel = comp.read().clone().unwrap_or_default();
    let q = query();
    let searching = !q.trim().is_empty();
    let hits: Vec<(PresetEntry, Vec<String>)> = comp
        .presets
        .iter()
        .map(|p| (p.clone(), kept(p, &q)))
        .filter(|(_, snaps)| !snaps.is_empty())
        .collect();

    let play = {
        let rig = rig.clone();
        move |preset: String, snapshot: String| {
            if let Some(r) = rig.clone() {
                spawn(async move {
                    let _ = r.choose_preset(preset, snapshot).await;
                });
            }
        }
    };
    let total: usize = comp.presets.len();

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
                    div { style: "flex: 1;" }
                    span { style: "font-size: 10px; color: {FAINT};",
                        if searching { "{hits.len()}/{total}" } else { "{total}" }
                    }
                }
                input {
                    style: "width: 100%; font-size: 12px; color: {TEXT}; background: #0a0a0d; \
                            border: 1px solid #2a2a31; border-radius: 7px; padding: 6px 9px; outline: none;",
                    placeholder: "Search presets and snapshots…",
                    value: "{q}",
                    oninput: move |e| query.set(e.value()),
                    onkeydown: {
                        let first = hits.first().map(|(p, snaps)| (p.name.clone(), snaps.first().cloned().unwrap_or_default()));
                        let play = play.clone();
                        move |e: KeyboardEvent| match e.key() {
                            Key::Enter => {
                                if let Some((p, s)) = first.clone() {
                                    play(p, s);
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
                        if total == 0 { "No presets yet — build them in the Library." } else { "No preset matches." }
                    }
                }
                for (p, snaps) in hits.iter().cloned() {
                    {
                        let playing = comp.active_preset.eq_ignore_ascii_case(&p.name);
                        let open = searching || playing || opened().iter().any(|o| o == &p.name);
                        let name = p.name.clone();
                        let active_snap = comp.active_snapshot.clone();
                        let play = play.clone();
                        rsx! {
                            div { key: "{p.name}", style: "margin-bottom: 2px;",
                                button {
                                    style: format!(
                                        "display: flex; align-items: center; gap: 8px; width: 100%; padding: 7px 8px; \
                                         border-radius: 7px; border: none; cursor: pointer; justify-content: flex-start; \
                                         text-align: left; background: {}; color: {};",
                                        if playing { ON_BG } else { "transparent" },
                                        if playing { ON_FG } else { TEXT },
                                    ),
                                    onclick: {
                                        let name = name.clone();
                                        move |_| {
                                            let mut o = opened.write();
                                            if let Some(i) = o.iter().position(|x| x == &name) {
                                                o.remove(i);
                                            } else {
                                                o.push(name.clone());
                                            }
                                        }
                                    },
                                    span {
                                        style: format!(
                                            "width: 6px; height: 6px; border-radius: 999px; flex-shrink: 0; background: {};",
                                            if playing { LIVE } else { "transparent" },
                                        ),
                                    }
                                    span { style: "flex: 1 1 0; min-width: 0; font-size: 12px; font-weight: 600; white-space: nowrap; overflow: hidden;",
                                        "{p.name}"
                                    }
                                    span { style: "font-size: 10px; color: {FAINT}; flex-shrink: 0;",
                                        if open { "▾" } else { "{p.snapshots.len()} ▸" }
                                    }
                                }
                                if open {
                                    div { style: "display: flex; flex-direction: column; padding: 1px 0 4px 20px;",
                                        for (i, snap) in snaps.iter().enumerate() {
                                            {
                                                let lit = playing
                                                    && (active_snap.eq_ignore_ascii_case(snap)
                                                        || (active_snap.is_empty() && i == 0));
                                                let (pn, sn) = (name.clone(), snap.clone());
                                                let play = play.clone();
                                                rsx! {
                                                    button {
                                                        key: "{snap}",
                                                        style: format!(
                                                            "display: flex; align-items: center; gap: 7px; width: 100%; padding: 5px 8px; \
                                                             border-radius: 6px; border: none; cursor: pointer; justify-content: flex-start; \
                                                             text-align: left; font-size: 12px; background: {}; color: {};",
                                                            if lit { "rgba(34,197,94,0.12)" } else { "transparent" },
                                                            if lit { TEXT } else { MUTED },
                                                        ),
                                                        onclick: move |_| play(pn.clone(), sn.clone()),
                                                        span {
                                                            style: format!(
                                                                "width: 5px; height: 5px; border-radius: 999px; flex-shrink: 0; background: {};",
                                                                if lit { LIVE } else { "#3f3f46" },
                                                            ),
                                                        }
                                                        "{snap}"
                                                    }
                                                }
                                            }
                                        }
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
                    title: "Every preset and its module picks — in the library",
                    onclick: move |_| on_browse.call(Kind::Compositions),
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
