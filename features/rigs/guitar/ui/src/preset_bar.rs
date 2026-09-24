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

use crate::kit::{Button, ListRow, MenuItem, Picked, SectionHeader};
use crate::library::Kind;
use crate::theme::{FAINT, FIELD, LINE, LINE_STRONG, SIDEBAR, SIDEBAR_W, TEXT};

/// A preset's menu: rename, duplicate, delete (refused while a patch plays
/// it).
fn preset_items(p: &PresetEntry, all: &[String]) -> Vec<MenuItem> {
    let others: Vec<String> = all.iter().filter(|n| !n.eq_ignore_ascii_case(&p.name)).cloned().collect();
    vec![
        MenuItem::head(format!("Preset · {}", p.name)),
        MenuItem::name("rename", "Rename…", "Rename", &p.name, others),
        MenuItem::name(
            "duplicate",
            "Duplicate…",
            "Duplicate",
            crate::module_sidebar::next_name(&p.name, all),
            all.to_vec(),
        ),
        MenuItem::delete(
            "delete",
            "Delete preset",
            (!p.used_by.is_empty()).then(|| format!("In use: {}", p.used_by.join(", "))),
        ),
    ]
}

/// Every word of the query in the text.
fn hit(text: &str, query: &str) -> bool {
    let hay = text.to_lowercase();
    query
        .split_whitespace()
        .all(|w| hay.contains(&w.to_lowercase()))
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

    let names: Vec<String> = comp.presets.iter().map(|p| p.name.clone()).collect();
    let manage = {
        let rig = rig.clone();
        move |name: String, x: Picked| {
            let Some(r) = rig.clone() else { return };
            let text = x.text;
            spawn(async move {
                let _ = match x.id {
                    "rename" => r.rename_rig_preset(name, text).await,
                    "duplicate" => r.duplicate_rig_preset(name, text).await,
                    "delete" => r.delete_rig_preset(name).await,
                    _ => return,
                };
            });
        }
    };

    rsx! {
        aside {
            style: "width: {SIDEBAR_W}; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; \
                    border-right: 1px solid {LINE}; background: {SIDEBAR}; color: {TEXT};",
            div { style: "display: flex; flex-direction: column; gap: 8px; padding: 12px 12px 10px; \
                          border-bottom: 1px solid {LINE}; flex-shrink: 0;",
                SectionHeader {
                    label: "Presets",
                    count: if searching { format!("{}/{total}", hits.len()) } else { format!("{total}") },
                }
                // The hint is drawn over the field: the renderer does not
                // paint a `placeholder`.
                div { style: "position: relative; display: flex;",
                if q.is_empty() {
                    span {
                        style: "position: absolute; left: 10px; top: 0; bottom: 0; display: flex; align-items: center; \
                                font-size: 12px; color: {FAINT}; pointer-events: none;",
                        "Search presets and snapshots…"
                    }
                }
                input {
                    style: "width: 100%; font-size: 12px; color: {TEXT}; background: {FIELD}; \
                            border: 1px solid {LINE_STRONG}; border-radius: 6px; padding: 6px 9px; outline: none;",
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
            }
            div { style: "flex: 1 1 0; min-height: 0; overflow-y: scroll; padding: 6px 8px; display: flex; flex-direction: column; gap: 1px;",
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
                            div { key: "{p.name}", style: "display: flex; flex-direction: column; gap: 1px;",
                                ListRow {
                                    title: p.name.clone(),
                                    note: format!("{}", p.snapshots.len()),
                                    live: playing,
                                    onclick: {
                                        let name = name.clone();
                                        move |()| {
                                            let mut o = opened.write();
                                            if let Some(i) = o.iter().position(|x| x == &name) {
                                                o.remove(i);
                                            } else {
                                                o.push(name.clone());
                                            }
                                        }
                                    },
                                    menu: preset_items(&p, &names),
                                    on_menu: {
                                        let manage = manage.clone();
                                        let name = name.clone();
                                        move |x: Picked| manage(name.clone(), x)
                                    },
                                }
                                if open {
                                    for (i, snap) in snaps.iter().enumerate() {
                                        {
                                            let lit = playing
                                                && (active_snap.eq_ignore_ascii_case(snap)
                                                    || (active_snap.is_empty() && i == 0));
                                            let (pn, sn) = (name.clone(), snap.clone());
                                            let play = play.clone();
                                            rsx! {
                                                ListRow {
                                                    key: "{snap}",
                                                    title: snap.clone(),
                                                    small: true,
                                                    indent: 18,
                                                    live: lit,
                                                    onclick: move |()| play(pn.clone(), sn.clone()),
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
                Button {
                    label: "Manage",
                    grow: true,
                    small: true,
                    title: "Every preset and its module picks — in the library",
                    onclick: move |()| on_browse.call(Kind::Compositions),
                }
                Button {
                    label: "Get tones",
                    grow: true,
                    small: true,
                    title: "Find a new capture on TONE3000",
                    onclick: move |()| on_tones.call(()),
                }
            }
        }
    }
}
