//! The right sidebar: the selected module's presets, to dial a patch in by
//! combining modules.
//!
//! Select a module on the Control surface (its label strip, or its panel)
//! and this lists every preset of that module with its variations as chips.
//! The chip playing is lit; a tap plays another *on the active patch* —
//! `choose_module`, the patch's own module pick, saved with the profile —
//! so a patch is built by taking the amp from one preset, the time effects
//! from another, and so on.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;

/// What the right sidebar shows presets for.
#[derive(Clone, PartialEq, Debug)]
pub enum Selection {
    /// A module (`Amp`, `Drive`, `Time`): its module presets and variations.
    Module(String),
    /// One chain block (`DLY 1`, `VERB 2`): the block presets of its type
    /// (`delay`, `reverb`) — the ones module snapshots point at.
    Block { name: String, block_type: String },
}

/// The sidebar's selection (`None`: closed). Provided at the rig root; any
/// panel sets it.
#[derive(Clone, Copy)]
pub struct SelectedModule(pub Signal<Option<Selection>>);

impl SelectedModule {
    /// Select `sel` (on a context captured at render).
    pub fn set(self, sel: Selection) {
        let mut s = self.0;
        if s.peek().as_ref() != Some(&sel) {
            s.set(Some(sel));
        }
    }
}


const LINE: &str = "#222228";
const TEXT: &str = "#e4e4e7";
const MUTED: &str = "#a1a1aa";
const FAINT: &str = "#63636b";
const LIVE: &str = "#22c55e";

/// The sidebar. `revision` is the performance model's, so a pick made here
/// (or anywhere) re-reads what is playing.
#[component]
pub fn ModuleSidebar(revision: u64) -> Element {
    let Some(SelectedModule(mut selected)) = try_use_context::<SelectedModule>() else {
        return rsx! {};
    };
    let rig = use_hook(try_consume_context::<RigClient>);
    // Every hook before any early return: the hook order must not depend on
    // whether a module is selected.
    //
    // What the resource re-reads on. A prop is not reactive — a resource
    // re-runs only on the signals it reads — so the model's revision is
    // mirrored into one, and every pick made here bumps `refresh` once its
    // call has landed: the lit preset follows the click.
    let mut rev = use_signal(|| revision);
    if *rev.peek() != revision {
        rev.set(revision);
    }
    let refresh = use_signal(|| 0u32);
    let comp = use_resource({
        let rig = rig.clone();
        move || {
            let rig = rig.clone();
            let _ = rev();
            let _ = refresh();
            let _ = selected();
            async move {
                match rig {
                    Some(r) => r.compositions().await.ok(),
                    None => None,
                }
            }
        }
    });
    let Some(selection) = selected() else {
        return rsx! {};
    };
    let comp = comp.read().clone().flatten();
    if let Selection::Block { name, block_type } = &selection {
        return rsx! { BlockPresets { block: name.clone(), block_type: block_type.clone(), comp, refresh, on_close: move |()| selected.set(None) } };
    }
    let Selection::Module(module) = selection else {
        return rsx! {};
    };
    let presets: Vec<signal_guitar_proto::ModulePresetEntry> = comp
        .as_ref()
        .map(|c| {
            c.modules
                .iter()
                .filter(|m| m.module.eq_ignore_ascii_case(&module))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let playing = comp.as_ref().and_then(|c| {
        c.active_modules
            .iter()
            .find(|a| a.module.eq_ignore_ascii_case(&module))
            .map(|a| (a.preset.clone(), a.snapshot.clone()))
    });

    rsx! {
        div {
            style: "width: 260px; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; \
                    border-left: 1px solid {LINE}; background: #0b0b0e;",
            // Header: the module, and close.
            div { style: "display: flex; align-items: center; gap: 8px; padding: 10px 12px; border-bottom: 1px solid {LINE};",
                div { style: "display: flex; flex-direction: column; flex: 1; min-width: 0;",
                    span { style: "font-size: 9px; letter-spacing: 0.14em; text-transform: uppercase; color: {FAINT};", "Module" }
                    span { style: "font-size: 14px; font-weight: 700; color: {TEXT};", "{module}" }
                }
                button {
                    style: "appearance: none; border: none; background: transparent; color: {MUTED}; cursor: pointer; padding: 2px;",
                    title: "Close",
                    onclick: move |_| selected.set(None),
                    fts_chrome::Glyph { icon: fts_chrome::Icon::Close, size: 12 }
                }
            }
            if presets.is_empty() {
                span { style: "padding: 14px 12px; font-size: 11px; color: {FAINT}; line-height: 1.5;",
                    "No {module} presets in the library yet."
                }
            }
            // The presets, each with its variations. Scrolls when long.
            div { style: "flex: 1 1 0%; min-height: 0; overflow-y: scroll; padding: 6px 0 12px;",
                for entry in presets {
                    {
                        let here = playing.as_ref().is_some_and(|(p, _)| p.eq_ignore_ascii_case(&entry.name));
                        let lit_snap = playing.as_ref().filter(|_| here).map(|(_, s)| s.clone());
                        rsx! {
                            div { key: "{entry.name}", style: "padding: 8px 12px; display: flex; flex-direction: column; gap: 6px;",
                                span {
                                    style: format!(
                                        "font-size: 12px; font-weight: 600; color: {};",
                                        if here { TEXT } else { MUTED }
                                    ),
                                    "{entry.name}"
                                }
                                div { style: "display: flex; flex-wrap: wrap; gap: 5px;",
                                    for (i, snap) in entry.snapshots.iter().enumerate() {
                                        {
                                            let lit = lit_snap.as_deref().is_some_and(|s| {
                                                s.eq_ignore_ascii_case(snap) || (s.is_empty() && i == 0)
                                            });
                                            let (m, p, s) = (entry.module.clone(), entry.name.clone(), snap.clone());
                                            let rig = rig.clone();
                                            rsx! {
                                                button {
                                                    key: "{snap}",
                                                    style: format!(
                                                        "padding: 4px 9px; border-radius: 6px; font-size: 11px; cursor: pointer; \
                                                         border: 1px solid {}; background: {}; color: {};",
                                                        if lit { LIVE } else { LINE },
                                                        if lit { "rgba(34,197,94,0.12)" } else { "transparent" },
                                                        if lit { TEXT } else { MUTED },
                                                    ),
                                                    onclick: move |_| {
                                                        let (m, p, s) = (m.clone(), p.clone(), s.clone());
                                                        if let Some(r) = rig.clone() {
                                                            let mut refresh = refresh;
                                                            spawn(async move {
                                                                let _ = r.choose_module(m, p, s).await;
                                                                refresh += 1;
                                                            });
                                                        }
                                                    },
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
            span { style: "padding: 10px 12px; border-top: 1px solid {LINE}; font-size: 10px; color: {FAINT}; line-height: 1.5;",
                "A variation picked here plays on this patch and is saved with it."
            }
        }
    }
}

/// A block's presets (every block preset of its type), the one it plays lit;
/// a tap puts another on the active patch's block (`choose_block`).
#[component]
fn BlockPresets(
    block: String,
    block_type: String,
    comp: Option<signal_guitar_proto::CompositionModel>,
    /// Bumped once a pick has landed, so the sidebar re-reads what plays.
    refresh: Signal<u32>,
    on_close: EventHandler<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let presets: Vec<signal_guitar_proto::BlockPresetEntry> = comp
        .as_ref()
        .map(|c| {
            c.block_presets
                .iter()
                .filter(|b| b.block_type.eq_ignore_ascii_case(&block_type))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let playing = comp.as_ref().and_then(|c| {
        c.active_blocks
            .iter()
            .find(|b| b.block.eq_ignore_ascii_case(&block))
            .map(|b| b.preset.clone())
    });
    let kind = match block_type.as_str() {
        "delay" => "Delay",
        "reverb" => "Reverb",
        other => other,
    }
    .to_string();
    rsx! {
        div {
            style: "width: 260px; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; \
                    border-left: 1px solid {LINE}; background: #0b0b0e;",
            div { style: "display: flex; align-items: center; gap: 8px; padding: 10px 12px; border-bottom: 1px solid {LINE};",
                div { style: "display: flex; flex-direction: column; flex: 1; min-width: 0;",
                    span { style: "font-size: 9px; letter-spacing: 0.14em; text-transform: uppercase; color: {FAINT};", "{kind} presets" }
                    span { style: "font-size: 14px; font-weight: 700; color: {TEXT};", "{block}" }
                }
                button {
                    style: "appearance: none; border: none; background: transparent; color: {MUTED}; cursor: pointer; padding: 2px;",
                    title: "Close",
                    onclick: move |_| on_close.call(()),
                    fts_chrome::Glyph { icon: fts_chrome::Icon::Close, size: 12 }
                }
            }
            if presets.is_empty() {
                span { style: "padding: 14px 12px; font-size: 11px; color: {FAINT}; line-height: 1.5;",
                    "No {kind} presets in the library yet."
                }
            }
            div { style: "flex: 1 1 0%; min-height: 0; overflow-y: scroll; padding: 6px 8px 12px; display: flex; flex-direction: column; gap: 3px;",
                for p in presets {
                    {
                        let lit = playing.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(&p.name));
                        let (b, name) = (block.clone(), p.name.clone());
                        let rig = rig.clone();
                        rsx! {
                            button {
                                key: "{p.name}",
                                style: format!(
                                    "text-align: left; padding: 6px 10px; border-radius: 6px; font-size: 12px; cursor: pointer; \
                                     border: 1px solid {}; background: {}; color: {};",
                                    if lit { LIVE } else { "transparent" },
                                    if lit { "rgba(34,197,94,0.12)" } else { "transparent" },
                                    if lit { TEXT } else if p.bypass { FAINT } else { MUTED },
                                ),
                                onclick: move |_| {
                                    let (b, name) = (b.clone(), name.clone());
                                    if let Some(r) = rig.clone() {
                                        let mut refresh = refresh;
                                        spawn(async move {
                                            let _ = r.choose_block(b, name).await;
                                            refresh += 1;
                                        });
                                    }
                                },
                                "{p.name}"
                            }
                        }
                    }
                }
            }
            span { style: "padding: 10px 12px; border-top: 1px solid {LINE}; font-size: 10px; color: {FAINT}; line-height: 1.5;",
                "Picked here, this block plays the preset on this patch. Editing a preset changes every module and patch that uses it."
            }
        }
    }
}
