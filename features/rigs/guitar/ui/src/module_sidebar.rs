//! The right sidebar: the selected module's presets, to dial a patch in by
//! combining modules — and to keep what you dialled.
//!
//! Select a module on the Control surface (its label strip, or its panel)
//! and this shows what it plays in a [`PresetBar`](crate::kit::PresetBar) —
//! preset · snapshot, amber when the patch has edits on the module's blocks —
//! over every preset of that module with its snapshots. A tap plays another
//! *on the active patch* (`choose_module`, the patch's own module pick, saved
//! with the profile), so a patch is built by taking the amp from one preset,
//! the time effects from another, and so on.
//!
//! The bar's ⋯ keeps what you dialled: save it over the snapshot, as a new
//! snapshot or a new preset, or revert it; and manages the preset itself
//! (rename, duplicate, delete — refused, with the reason, while anything
//! uses it). Each row has the same menu for its own preset or snapshot. The
//! same menus sit on the control surface's module strips (`control.rs`),
//! built by [`module_menu`] and handled by [`module_act`].

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{BlockPresetEntry, CompositionModel, ModulePick, ModulePresetEntry};

use crate::kit::{Button, MenuItem, PickOption, Picked, PresetBar, SectionHeader};
use crate::preset_look::Look;
use crate::theme::{FAINT, FIELD, INSPECTOR_W, LINE, LINE_STRONG, MUTED, SIDEBAR, TEXT};

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

/// What the rig opens with selected, when a host provides one (the
/// screenshot tool, to picture the sidebar).
#[derive(Clone)]
pub struct InitialSelection(pub Option<Selection>);

impl SelectedModule {
    /// Select `sel` (on a context captured at render).
    pub fn set(self, sel: Selection) {
        let mut s = self.0;
        if s.peek().as_ref() != Some(&sel) {
            s.set(Some(sel));
        }
    }
}

/// One live block, as far as the sidebar needs it: its name, its id and
/// whether the patch has edits on it.
pub type ChainRef = (String, String, bool);

/// Fire a rig call without waiting — the result arrives as the next event.
fn send<F, Fut>(rig: &Option<RigClient>, call: F)
where
    F: FnOnce(RigClient) -> Fut + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    if let Some(r) = rig.clone() {
        spawn(async move { call(r).await });
    }
}

/// Whether a module pick plays differently from its saved snapshot: the
/// patch has edits on a block it owns.
#[must_use]
pub fn pick_modified(pick: Option<&ModulePick>, chain: &[ChainRef]) -> bool {
    pick.is_some_and(|p| {
        p.blocks
            .iter()
            .any(|b| chain.iter().any(|(n, _, edited)| *edited && n.eq_ignore_ascii_case(b)))
    })
}

/// The snapshot a pick plays: its own, or its preset's first.
fn played_snapshot(pick: &ModulePick, presets: &[ModulePresetEntry]) -> String {
    if !pick.snapshot.is_empty() {
        return pick.snapshot.clone();
    }
    presets
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(&pick.preset))
        .and_then(|p| p.snapshots.first().cloned())
        .unwrap_or_default()
}

/// A name for a copy that is not taken: "Clean 2", "Clean 3"…
#[must_use]
pub fn next_name(base: &str, taken: &[String]) -> String {
    let base = base.trim();
    let base = if base.is_empty() { "New" } else { base };
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|c| !taken.iter().any(|t| t.eq_ignore_ascii_case(c)))
        .unwrap_or_else(|| base.to_string())
}

/// "In use: Preset Fender, Patch Lead (Blues)".
fn in_use(users: &[String]) -> Option<String> {
    (!users.is_empty()).then(|| format!("In use: {}", users.join(", ")))
}

/// The menu for one snapshot of a module preset: rename, delete (refused
/// for the only one, and while anything picks it).
#[must_use]
pub fn snapshot_items(entry: &ModulePresetEntry, snap: &str) -> Vec<MenuItem> {
    let i = entry
        .snapshots
        .iter()
        .position(|s| s.eq_ignore_ascii_case(snap));
    let others: Vec<String> = entry
        .snapshots
        .iter()
        .filter(|s| !s.eq_ignore_ascii_case(snap))
        .cloned()
        .collect();
    let users = i
        .and_then(|i| entry.snapshot_used_by.get(i))
        .filter(|u| !u.is_empty())
        .map(|u| format!("In use: {u}"));
    let refused = if entry.snapshots.len() <= 1 {
        Some("The only snapshot — delete the preset instead".to_string())
    } else {
        users
    };
    vec![
        MenuItem::head(format!("Snapshot · {snap}")),
        MenuItem::name("rename_snapshot", "Rename snapshot…", "Rename", snap, others),
        MenuItem::delete("delete_snapshot", "Delete snapshot", refused),
    ]
}

/// The menu for one module preset (and, given one, one of its snapshots
/// first): rename, duplicate, delete.
#[must_use]
pub fn preset_items(entry: &ModulePresetEntry, siblings: &[String], snapshot: Option<&str>) -> Vec<MenuItem> {
    let others: Vec<String> = siblings
        .iter()
        .filter(|n| !n.eq_ignore_ascii_case(&entry.name))
        .cloned()
        .collect();
    let mut items = snapshot.map(|s| snapshot_items(entry, s)).unwrap_or_default();
    items.push(MenuItem::head(format!("Preset · {}", entry.name)));
    items.push(MenuItem::name("rename_preset", "Rename preset…", "Rename", &entry.name, others));
    items.push(MenuItem::name(
        "duplicate_preset",
        "Duplicate preset…",
        "Duplicate",
        next_name(&entry.name, siblings),
        siblings.to_vec(),
    ));
    items.push(MenuItem::delete("delete_preset", "Delete preset", in_use(&entry.used_by)));
    items
}

/// The menu for what `module` plays: keep the live edits (save over the
/// snapshot, as a new snapshot, as a new preset), drop them, then manage the
/// preset and snapshot that play.
#[must_use]
pub fn module_menu(
    module: &str,
    pick: Option<&ModulePick>,
    presets: &[ModulePresetEntry],
    modified: bool,
) -> Vec<MenuItem> {
    let names: Vec<String> = presets.iter().map(|p| p.name.clone()).collect();
    let entry = pick.and_then(|p| presets.iter().find(|e| e.name.eq_ignore_ascii_case(&p.preset)));
    let snap = pick.map(|p| played_snapshot(p, presets)).unwrap_or_default();
    let no_edits = (!modified).then(|| "No edits on this patch".to_string());
    let mut items = vec![MenuItem::head(format!("{module} on this patch"))];
    match entry {
        Some(e) => {
            items.push(MenuItem::run("save", format!("Save to “{snap}”")).unless(no_edits.clone()));
            items.push(MenuItem::name(
                "save_snapshot",
                "Save as new snapshot…",
                "Save",
                next_name(&snap, &e.snapshots),
                e.snapshots.clone(),
            ));
        }
        None => {}
    }
    items.push(MenuItem::name(
        "save_preset",
        "Save as new preset…",
        "Save",
        next_name(entry.map_or(module, |e| e.name.as_str()), &names),
        names.clone(),
    ));
    items.push(MenuItem::run("revert", "Revert edits").unless(no_edits));
    if let Some(e) = entry {
        items.push(MenuItem::sep());
        items.extend(preset_items(e, &names, Some(&snap)));
    }
    items.push(MenuItem::sep());
    items.push(MenuItem::run("manage", format!("All {module} presets in the library")));
    items
}

/// Do what a module menu item says, for `module`'s preset `preset` (and
/// snapshot `snapshot`, which the snapshot items and "save" act on).
///
/// `library` is the picker's open state, captured at render ("manage").
pub fn module_act(
    rig: &Option<RigClient>,
    library: Option<crate::library::OpenLibrary>,
    module: &str,
    preset: &str,
    snapshot: &str,
    p: Picked,
) {
    let (m, pr, sn, text) = (module.to_string(), preset.to_string(), snapshot.to_string(), p.text);
    match p.id {
        "save" => send(rig, move |r| async move { let _ = r.save_module_snapshot(m, pr, sn).await; }),
        "save_snapshot" => send(rig, move |r| async move { let _ = r.save_module_snapshot(m, pr, text).await; }),
        "save_preset" => {
            let sn = if sn.is_empty() { "Default".to_string() } else { sn };
            send(rig, move |r| async move { let _ = r.save_module_snapshot(m, text, sn).await; });
        }
        "revert" => send(rig, move |r| async move { let _ = r.revert_module(m).await; }),
        "rename_preset" => send(rig, move |r| async move { let _ = r.rename_module_preset(m, pr, text).await; }),
        "duplicate_preset" => send(rig, move |r| async move { let _ = r.duplicate_module_preset(m, pr, text).await; }),
        "delete_preset" => send(rig, move |r| async move { let _ = r.delete_module_preset(m, pr).await; }),
        "rename_snapshot" => send(rig, move |r| async move { let _ = r.rename_module_snapshot(m, pr, sn, text).await; }),
        "delete_snapshot" => send(rig, move |r| async move { let _ = r.delete_module_snapshot(m, pr, sn).await; }),
        "manage" => {
            if let (Some(crate::library::OpenLibrary(mut open)), Some(kind)) =
                (library, crate::library::Kind::for_module(module))
            {
                open.set(Some(kind));
            }
        }
        _ => {}
    }
}

/// Every (preset, snapshot) of a module as a preset bar's list, grouped by
/// preset.
#[must_use]
pub fn module_options(presets: &[ModulePresetEntry], pick: Option<&ModulePick>) -> (Vec<PickOption>, Vec<(String, String)>) {
    let mut options = Vec::new();
    let mut targets = Vec::new();
    for p in presets {
        for (i, s) in p.snapshots.iter().enumerate() {
            let live = pick.is_some_and(|k| {
                k.preset.eq_ignore_ascii_case(&p.name)
                    && (k.snapshot.eq_ignore_ascii_case(s) || (k.snapshot.is_empty() && i == 0))
            });
            options.push(PickOption {
                label: s.clone(),
                group: p.name.clone(),
                note: String::new(),
                live,
            });
            targets.push((p.name.clone(), s.clone()));
        }
    }
    (options, targets)
}

/// The sidebar. `revision` is the performance model's, so a pick made here
/// (or anywhere) re-reads what is playing; `chain` is the live blocks, for
/// what is edited.
#[component]
pub fn ModuleSidebar(revision: u64, #[props(default)] chain: Vec<ChainRef>) -> Element {
    let Some(SelectedModule(mut selected)) = try_use_context::<SelectedModule>() else {
        return rsx! {};
    };
    let rig = use_hook(try_consume_context::<RigClient>);
    let library = try_use_context::<crate::library::OpenLibrary>();
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
    let mut query = use_signal(String::new);
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
        return rsx! {
            BlockPresets {
                block: name.clone(),
                block_type: block_type.clone(),
                comp,
                chain,
                refresh,
                on_close: move |()| selected.set(None),
            }
        };
    }
    let Selection::Module(module) = selection else {
        return rsx! {};
    };
    let presets: Vec<ModulePresetEntry> = comp
        .as_ref()
        .map(|c| {
            c.modules
                .iter()
                .filter(|m| m.module.eq_ignore_ascii_case(&module))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let names: Vec<String> = presets.iter().map(|p| p.name.clone()).collect();
    let pick: Option<ModulePick> = comp.as_ref().and_then(|c| {
        c.active_modules
            .iter()
            .find(|a| a.module.eq_ignore_ascii_case(&module))
            .cloned()
    });
    let modified = pick_modified(pick.as_ref(), &chain);
    let played = pick.as_ref().map(|p| played_snapshot(p, &presets)).unwrap_or_default();
    let (options, targets) = module_options(&presets, pick.as_ref());
    let choose = {
        let rig = rig.clone();
        let module = module.clone();
        move |preset: String, snapshot: String| {
            let m = module.clone();
            if let Some(r) = rig.clone() {
                let mut refresh = refresh;
                spawn(async move {
                    let _ = r.choose_module(m, preset, snapshot).await;
                    refresh += 1;
                });
            }
        }
    };

    rsx! {
        div {
            style: "width: {INSPECTOR_W}; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; \
                    border-left: 1px solid {LINE}; background: {SIDEBAR}; color: {TEXT};",
            // Header: what the module plays, and every way to change it.
            div { style: "display: flex; flex-direction: column; gap: 8px; padding: 10px 12px 12px; \
                          border-bottom: 1px solid {LINE}; flex-shrink: 0;",
                SectionHeader { label: "Module",
                    CloseButton { on_close: move |()| selected.set(None) }
                }
                PresetBar {
                    label: module.clone(),
                    name: pick.as_ref().map(|p| p.preset.clone()).unwrap_or_default(),
                    sub: played.clone(),
                    modified,
                    live: pick.is_some(),
                    placeholder: "Nothing picked",
                    options,
                    on_pick: {
                        let choose = choose.clone();
                        move |i: usize| {
                            if let Some((p, s)) = targets.get(i).cloned() {
                                choose(p, s);
                            }
                        }
                    },
                    on_step: {
                        let rig = rig.clone();
                        let module = module.clone();
                        move |delta: i32| {
                            let m = module.clone();
                            if let Some(r) = rig.clone() {
                                let mut refresh = refresh;
                                spawn(async move {
                                    let _ = r.step_module(m, delta).await;
                                    refresh += 1;
                                });
                            }
                        }
                    },
                    menu: module_menu(&module, pick.as_ref(), &presets, modified),
                    on_menu: {
                        let rig = rig.clone();
                        let module = module.clone();
                        let preset = pick.as_ref().map(|p| p.preset.clone()).unwrap_or_default();
                        let played = played.clone();
                        move |p: Picked| module_act(&rig, library, &module, &preset, &played, p)
                    },
                }
                PickPicture {
                    look: comp
                        .as_ref()
                        .zip(pick.as_ref())
                        .and_then(|(c, p)| pick_look(c, p))
                        .unwrap_or_default(),
                }
                // Edited: the two things to do about it, on the bar's edge
                // rather than in its menu.
                if modified && pick.is_some() {
                    div { style: "display: flex; gap: 6px;",
                        Button {
                            label: format!("Save to “{played}”"),
                            primary: true,
                            grow: true,
                            small: true,
                            title: "Every patch playing this snapshot gets these edits",
                            onclick: {
                                let rig = rig.clone();
                                let (m, p, s) = (module.clone(), pick.as_ref().map(|p| p.preset.clone()).unwrap_or_default(), played.clone());
                                move |()| module_act(&rig, library, &m, &p, &s, Picked { id: "save", text: String::new() })
                            },
                        }
                        Button {
                            label: "Revert",
                            small: true,
                            onclick: {
                                let rig = rig.clone();
                                let m = module.clone();
                                move |()| module_act(&rig, library, &m, "", "", Picked { id: "revert", text: String::new() })
                            },
                        }
                    }
                }
            }
            // Find: every preset of the module, searchable by preset or
            // snapshot name.
            div { style: "display: flex; flex-direction: column; gap: 6px; padding: 10px 12px 4px; flex-shrink: 0;",
                SearchField {
                    value: query(),
                    placeholder: format!("Search {} {module} presets…", presets.len()),
                    on_input: move |v: String| query.set(v),
                }
            }
            if presets.is_empty() {
                span { style: "padding: 4px 12px 12px; font-size: 11px; color: {FAINT}; line-height: 1.5;",
                    "No {module} presets yet — dial one in and use ⋯ › Save as new preset."
                }
            }
            // One group per preset, its snapshots as rows that show what
            // they hold: a Delay or Reverb snapshot its block's picture, a
            // Time snapshot its Delay and Reverb, an Amp snapshot its
            // captures.
            div { style: "flex: 1 1 0%; min-height: 0; overflow-y: scroll; padding: 0 6px 12px 4px; \
                          display: flex; flex-direction: column; gap: 1px;",
                for entry in presets.iter().cloned() {
                    {
                        let here = pick.as_ref().is_some_and(|p| p.preset.eq_ignore_ascii_case(&entry.name));
                        let words: Vec<String> = query().split_whitespace().map(str::to_lowercase).collect();
                        let snaps: Vec<(usize, String)> = entry
                            .snapshots
                            .iter()
                            .cloned()
                            .enumerate()
                            .filter(|(_, s)| hit(&words, &[&entry.name, s]))
                            .collect();
                        let comp_all = comp.clone().unwrap_or_default();
                        rsx! {
                            if !snaps.is_empty() {
                                div { key: "{entry.name}", style: "display: flex; flex-direction: column; gap: 1px;",
                                    PresetHeader {
                                        name: entry.name.clone(),
                                        count: entry.snapshots.len(),
                                        live: here,
                                        menu: preset_items(&entry, &names, None),
                                        on_menu: {
                                            let rig = rig.clone();
                                            let (module, preset) = (module.clone(), entry.name.clone());
                                            move |p: Picked| module_act(&rig, library, &module, &preset, "", p)
                                        },
                                    }
                                    for (i, snap) in snaps {
                                        {
                                            let lit = here && pick.as_ref().is_some_and(|p| {
                                                p.snapshot.eq_ignore_ascii_case(&snap) || (p.snapshot.is_empty() && i == 0)
                                            });
                                            let info = entry.snapshot_info.get(i).cloned().unwrap_or_default();
                                            let look = info
                                                .blocks
                                                .first()
                                                .and_then(|b| block_look(&comp_all, &b.preset))
                                                .unwrap_or_default();
                                            // A Time snapshot: its Delay and Reverb picks.
                                            let subs: Vec<(String, Look)> = info
                                                .modules
                                                .iter()
                                                .filter_map(|m| {
                                                    pick_look(&comp_all, m).map(|l| (m.module.clone(), l))
                                                })
                                                .collect();
                                            let choose = choose.clone();
                                            let preset = entry.name.clone();
                                            rsx! {
                                                crate::preset_look::PresetRow {
                                                    key: "{snap}",
                                                    name: snap.clone(),
                                                    look,
                                                    indent: 8,
                                                    live: lit,
                                                    modified: lit && modified,
                                                    subline: info.captures.iter().take(2).cloned().collect::<Vec<_>>().join(" · "),
                                                    macros: info.macros.clone(),
                                                    onclick: {
                                                        let (p, s) = (preset.clone(), snap.clone());
                                                        move |()| choose(p.clone(), s.clone())
                                                    },
                                                    menu: preset_items(&entry, &names, Some(&snap)),
                                                    on_menu: {
                                                        let rig = rig.clone();
                                                        let (module, preset, snap) = (module.clone(), preset.clone(), snap.clone());
                                                        move |p: Picked| module_act(&rig, library, &module, &preset, &snap, p)
                                                    },
                                                    for (m, l) in subs {
                                                        SubPick { key: "{m}", module: m, look: l, lit }
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
            span { style: "padding: 8px 12px; border-top: 1px solid {LINE}; font-size: 10px; color: {FAINT}; line-height: 1.5; flex-shrink: 0;",
                "Picks play on this patch. Saving into a preset changes every patch that plays it."
            }
        }
    }
}

/// How block preset `name` reads, when the library has it.
fn block_look(comp: &CompositionModel, name: &str) -> Option<Look> {
    comp.block_presets
        .iter()
        .find(|b| b.name.eq_ignore_ascii_case(name))
        .map(crate::preset_look::look)
}

/// How a module pick reads: its snapshot's first block preset (a Delay
/// pick's DLY 1).
fn pick_look(comp: &CompositionModel, pick: &ModulePick) -> Option<Look> {
    let entry = comp
        .modules
        .iter()
        .find(|m| m.module.eq_ignore_ascii_case(&pick.module) && m.name.eq_ignore_ascii_case(&pick.preset))?;
    let i = entry
        .snapshots
        .iter()
        .position(|s| s.eq_ignore_ascii_case(&pick.snapshot))
        .unwrap_or(0);
    let block = entry.snapshot_info.get(i)?.blocks.first()?;
    block_look(comp, &block.preset)
}

/// A preset's header in the module list: its name in small caps, hard left,
/// with how many snapshots it has; its menu hard right.
#[component]
fn PresetHeader(
    name: String,
    count: usize,
    live: bool,
    menu: Vec<MenuItem>,
    on_menu: EventHandler<Picked>,
) -> Element {
    let ink = if live { crate::theme::LIVE } else { FAINT };
    rsx! {
        div { class: "group", style: "display: flex; align-items: center; gap: 6px; padding: 12px 6px 3px 10px;",
            span { style: "font-size: 9px; font-weight: 700; letter-spacing: 0.14em; text-transform: uppercase; \
                           color: {ink}; white-space: nowrap; overflow: hidden; min-width: 0;",
                "{name}"
            }
            span { style: "font-size: 10px; font-family: monospace; color: {crate::theme::DIM};", "{count}" }
            div { style: "flex: 1 1 0;" }
            div { class: "opacity-25 group-hover:opacity-100", style: "display: flex; flex-shrink: 0;",
                crate::kit::ActionMenu { items: menu, on_pick: on_menu, size: 18, bare: true, title: "Preset actions" }
            }
        }
    }
}

/// A Time snapshot's Delay or Reverb pick, as a chip: the picture and the
/// value, small.
#[component]
fn SubPick(module: String, look: Look, lit: bool) -> Element {
    if look.group == crate::preset_look::OFF {
        return rsx! {
            span { style: "flex-shrink: 0; width: 60px; font-size: 9px; color: {crate::theme::DIM}; text-align: center;",
                title: "{module}: off", "—"
            }
        };
    }
    rsx! {
        span {
            style: "flex-shrink: 0; display: flex; align-items: center; gap: 3px; width: 60px; overflow: hidden;",
            title: "{module}: {look.engine} {look.value}",
            crate::preset_look::ShapeView { shape: look.shape.clone(), w: 20, h: 10, lit }
            span { style: "font-size: 9px; font-family: monospace; color: {MUTED}; white-space: nowrap;", "{look.value}" }
        }
    }
}

/// The sidebar's close button.
#[component]
fn CloseButton(on_close: EventHandler<()>) -> Element {
    rsx! {
        button {
            style: "display: flex; align-items: center; appearance: none; border: none; background: transparent; \
                    color: {MUTED}; cursor: pointer; padding: 2px;",
            title: "Close",
            onclick: move |_| on_close.call(()),
            fts_chrome::Glyph { icon: fts_chrome::Icon::Close, size: 12 }
        }
    }
}

/// The menu for one block preset: rename, duplicate, delete.
fn block_preset_items(p: &BlockPresetEntry, siblings: &[String]) -> Vec<MenuItem> {
    let others: Vec<String> = siblings
        .iter()
        .filter(|n| !n.eq_ignore_ascii_case(&p.name))
        .cloned()
        .collect();
    vec![
        MenuItem::head(format!("Preset · {}", p.name)),
        MenuItem::name("rename", "Rename…", "Rename", &p.name, others),
        MenuItem::name("duplicate", "Duplicate…", "Duplicate", next_name(&p.name, siblings), siblings.to_vec()),
        MenuItem::delete("delete", "Delete", in_use(&p.used_by)),
    ]
}

/// Do what a block preset menu item says. `block` is the live block (for
/// saving and reverting), `preset` the preset the item is about.
fn block_act(rig: &Option<RigClient>, block: &str, block_id: &str, preset: &str, p: Picked) {
    let (b, id, pr, text) = (block.to_string(), block_id.to_string(), preset.to_string(), p.text);
    match p.id {
        "save" => send(rig, move |r| async move { let _ = r.save_block_preset(b, pr).await; }),
        "save_as" => send(rig, move |r| async move { let _ = r.save_block_preset(b, text).await; }),
        "revert" => send(rig, move |r| async move { let _ = r.clear_block_overrides(id).await; }),
        "rename" => send(rig, move |r| async move { let _ = r.rename_block_preset(pr, text).await; }),
        "duplicate" => send(rig, move |r| async move { let _ = r.duplicate_block_preset(pr, text).await; }),
        "delete" => send(rig, move |r| async move { let _ = r.delete_block_preset(pr).await; }),
        _ => {}
    }
}

/// Whether a preset matches every word of a search, over its name, group
/// and engine.
fn hit(words: &[String], parts: &[&str]) -> bool {
    let hay = parts.join(" ").to_lowercase();
    words.iter().all(|w| hay.contains(w))
}

/// The search field of a sidebar list.
#[component]
pub(crate) fn SearchField(value: String, placeholder: String, on_input: EventHandler<String>) -> Element {
    // The hint is drawn under the field rather than as its `placeholder`,
    // which the renderer does not paint.
    let empty = value.is_empty();
    rsx! {
        div { style: "position: relative; display: flex;",
        if empty {
            span {
                style: "position: absolute; left: 10px; top: 0; bottom: 0; display: flex; align-items: center; \
                        font-size: 12px; color: {FAINT}; pointer-events: none; white-space: nowrap; overflow: hidden;",
                "{placeholder}"
            }
        }
        input {
            style: "width: 100%; font-size: 12px; color: {TEXT}; background: {FIELD}; border: 1px solid {LINE_STRONG}; \
                    border-radius: 6px; padding: 6px 9px; outline: none;",
            placeholder: "{placeholder}",
            value: "{value}",
            oninput: move |e| on_input.call(e.value()),
            onkeydown: move |e: KeyboardEvent| {
                e.stop_propagation();
                if e.key() == Key::Escape {
                    on_input.call(String::new());
                }
            },
        }
        }
    }
}

/// The current pick, large: its picture across the sidebar's width, the
/// value and engine under it.
#[component]
fn PickPicture(look: Look) -> Element {
    if look.group == crate::preset_look::OFF || look.shape == crate::preset_look::Shape::None {
        return rsx! {};
    }
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 6px; padding: 2px 2px 0;",
            crate::preset_look::ShapeView { shape: look.shape.clone(), w: 268, h: 28, lit: true }
            div { style: "display: flex; align-items: center; gap: 6px;",
                crate::preset_look::EngineChip { engine: look.engine.clone(), default: look.engine_default }
                span { style: "font-size: 10px; color: {FAINT};", "{look.group}" }
                div { style: "flex: 1 1 0;" }
                crate::preset_look::ValueChip { value: look.value.clone(), lit: true }
            }
        }
    }
}

/// A block's presets (every block preset of its type), grouped by what they
/// do and drawn: the one playing on top, large, with the bar's actions;
/// then a search and the group chips; then the groups. A tap puts another
/// on the active patch's block (`choose_block`).
#[component]
fn BlockPresets(
    block: String,
    block_type: String,
    comp: Option<CompositionModel>,
    chain: Vec<ChainRef>,
    /// Bumped once a pick has landed, so the sidebar re-reads what plays.
    refresh: Signal<u32>,
    on_close: EventHandler<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut query = use_signal(String::new);
    let mut only = use_signal(String::new);
    let presets: Vec<BlockPresetEntry> = comp
        .as_ref()
        .map(|c| {
            c.block_presets
                .iter()
                .filter(|b| b.block_type.eq_ignore_ascii_case(&block_type))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let names: Vec<String> = presets.iter().map(|p| p.name.clone()).collect();
    let playing = comp.as_ref().and_then(|c| {
        c.active_blocks
            .iter()
            .find(|b| b.block.eq_ignore_ascii_case(&block))
            .map(|b| b.preset.clone())
    });
    let live = chain.iter().find(|(n, _, _)| n.eq_ignore_ascii_case(&block)).cloned();
    let modified = live.as_ref().is_some_and(|(_, _, e)| *e);
    let block_id = live.map(|(_, id, _)| id).unwrap_or_default();
    let kind = match block_type.as_str() {
        "delay" => "Delay".to_string(),
        "reverb" => "Reverb".to_string(),
        other => {
            let mut c = other.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        }
    };
    let choose = {
        let rig = rig.clone();
        let block = block.clone();
        move |name: String| {
            let b = block.clone();
            if let Some(r) = rig.clone() {
                let mut refresh = refresh;
                spawn(async move {
                    let _ = r.choose_block(b, name).await;
                    refresh += 1;
                });
            }
        }
    };
    let at = playing
        .as_ref()
        .and_then(|p| names.iter().position(|n| n.eq_ignore_ascii_case(p)));
    let current_look = at.and_then(|i| presets.get(i)).map(crate::preset_look::look).unwrap_or_default();
    let no_edits = (!modified).then(|| "No edits on this block".to_string());
    let mut menu = vec![MenuItem::head(format!("{block} on this patch"))];
    if let Some(p) = &playing {
        menu.push(MenuItem::run("save", format!("Save to “{p}”")).unless(no_edits.clone()));
    }
    menu.push(MenuItem::name(
        "save_as",
        "Save as new preset…",
        "Save",
        next_name(playing.as_deref().unwrap_or(&kind), &names),
        names.clone(),
    ));
    menu.push(MenuItem::run("revert", "Revert edits").unless(no_edits));
    if let Some(entry) = at.and_then(|i| presets.get(i)) {
        menu.push(MenuItem::sep());
        menu.extend(block_preset_items(entry, &names));
    }

    // The list: grouped, then narrowed by the chip and the search.
    let groups = crate::preset_look::grouped(&presets);
    let chips: Vec<(String, usize)> = groups
        .iter()
        .filter(|(g, _)| !g.is_empty() && g != crate::preset_look::OFF)
        .map(|(g, v)| (g.clone(), v.len()))
        .collect();
    let words: Vec<String> = query().split_whitespace().map(str::to_lowercase).collect();
    let shown: Vec<(String, Vec<(BlockPresetEntry, Look)>)> = groups
        .into_iter()
        .filter(|(g, _)| only().is_empty() || *g == only() || g == crate::preset_look::OFF)
        .map(|(g, v)| {
            let v: Vec<_> = v
                .into_iter()
                .filter(|(p, l)| hit(&words, &[&p.name, &l.group, &l.engine]))
                .collect();
            (g, v)
        })
        .filter(|(_, v)| !v.is_empty())
        .collect();

    rsx! {
        div {
            style: "width: {INSPECTOR_W}; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; \
                    border-left: 1px solid {LINE}; background: {SIDEBAR}; color: {TEXT};",
            // What plays, large, and every way to change or keep it.
            div { style: "display: flex; flex-direction: column; gap: 8px; padding: 10px 12px 12px; \
                          border-bottom: 1px solid {LINE}; flex-shrink: 0;",
                SectionHeader { label: format!("{kind} · {block}"),
                    CloseButton { on_close: move |()| on_close.call(()) }
                }
                PresetBar {
                    label: block.clone(),
                    name: playing.clone().unwrap_or_default(),
                    modified,
                    placeholder: "As the module sets it",
                    options: presets
                        .iter()
                        .map(|p| PickOption {
                            label: p.name.clone(),
                            group: crate::preset_look::look(p).group,
                            live: playing.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(&p.name)),
                            ..Default::default()
                        })
                        .collect::<Vec<_>>(),
                    on_pick: {
                        let choose = choose.clone();
                        let names = names.clone();
                        move |i: usize| {
                            if let Some(n) = names.get(i).cloned() {
                                choose(n);
                            }
                        }
                    },
                    on_step: {
                        let choose = choose.clone();
                        let names = names.clone();
                        move |delta: i32| {
                            if names.is_empty() {
                                return;
                            }
                            let n = names.len() as i32;
                            let next = at.map_or(0, |i| (i as i32 + delta).rem_euclid(n)) as usize;
                            choose(names[next].clone());
                        }
                    },
                    menu,
                    on_menu: {
                        let rig = rig.clone();
                        let (block, id, preset) = (block.clone(), block_id.clone(), playing.clone().unwrap_or_default());
                        move |p: Picked| block_act(&rig, &block, &id, &preset, p)
                    },
                }
                PickPicture { look: current_look }
                if modified {
                    div { style: "display: flex; gap: 6px;",
                        if let Some(p) = playing.clone() {
                            Button {
                                label: format!("Save to “{p}”"),
                                primary: true,
                                grow: true,
                                small: true,
                                onclick: {
                                    let rig = rig.clone();
                                    let (block, id) = (block.clone(), block_id.clone());
                                    move |()| block_act(&rig, &block, &id, &p, Picked { id: "save", text: String::new() })
                                },
                            }
                        }
                        Button {
                            label: "Revert",
                            small: true,
                            onclick: {
                                let rig = rig.clone();
                                let (block, id) = (block.clone(), block_id.clone());
                                move |()| block_act(&rig, &block, &id, "", Picked { id: "revert", text: String::new() })
                            },
                        }
                    }
                }
            }
            // Find: a search and the groups as chips.
            div { style: "display: flex; flex-direction: column; gap: 6px; padding: 10px 12px 4px; flex-shrink: 0;",
                SearchField {
                    value: query(),
                    placeholder: format!("Search {} {} presets…", presets.len(), kind.to_lowercase()),
                    on_input: move |v: String| query.set(v),
                }
                if chips.len() > 1 {
                    crate::preset_look::GroupChips {
                        groups: chips,
                        selected: only(),
                        on_pick: move |g: String| only.set(if only() == g { String::new() } else { g }),
                    }
                }
            }
            div { style: "flex: 1 1 0%; min-height: 0; overflow-y: scroll; padding: 0 6px 12px 4px; display: flex; flex-direction: column; gap: 1px;",
                if shown.is_empty() {
                    span { style: "padding: 12px 10px; font-size: 11px; color: {FAINT}; line-height: 1.5;",
                        if presets.is_empty() { "No {kind} presets yet — dial the block in and use ⋯ › Save as new preset." } else { "No preset matches." }
                    }
                }
                for (group, rows) in shown {
                    div { key: "{group}", style: "display: flex; flex-direction: column; gap: 1px;",
                        // Off sits alone at the top, without a header.
                        if group != crate::preset_look::OFF && !group.is_empty() {
                            crate::preset_look::GroupHeader { label: group.clone(), count: rows.len() }
                        }
                        for (p, look) in rows {
                            {
                                let lit = playing.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(&p.name));
                                let choose = choose.clone();
                                rsx! {
                                    crate::preset_look::PresetRow {
                                        key: "{p.name}",
                                        name: p.name.clone(),
                                        look,
                                        used_by: p.used_by.clone(),
                                        macros: p.macros.clone(),
                                        live: lit,
                                        modified: lit && modified,
                                        onclick: {
                                            let name = p.name.clone();
                                            move |()| choose(name.clone())
                                        },
                                        menu: block_preset_items(&p, &names),
                                        on_menu: {
                                            let rig = rig.clone();
                                            let (block, id, preset) = (block.clone(), block_id.clone(), p.name.clone());
                                            move |x: Picked| block_act(&rig, &block, &id, &preset, x)
                                        },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(blocks: &[&str]) -> ModulePick {
        ModulePick {
            module: "Delay".into(),
            preset: "Slap".into(),
            snapshot: String::new(),
            blocks: blocks.iter().map(|b| (*b).to_string()).collect(),
        }
    }

    #[test]
    fn a_module_is_modified_when_a_block_it_owns_is_edited() {
        let chain = vec![
            ("DLY 1".to_string(), "b1".to_string(), true),
            ("Amp L".to_string(), "b2".to_string(), false),
        ];
        assert!(pick_modified(Some(&pick(&["dly 1"])), &chain));
        assert!(!pick_modified(Some(&pick(&["Amp L"])), &chain));
        assert!(!pick_modified(None, &chain));
    }

    #[test]
    fn a_copy_gets_the_next_free_number() {
        let taken = vec!["Clean".to_string(), "Clean 2".to_string()];
        assert_eq!(next_name("Clean", &taken), "Clean 3");
        assert_eq!(next_name("", &[]), "New 2");
    }

    #[test]
    fn saving_needs_edits_and_deleting_the_only_snapshot_is_refused() {
        let presets = vec![ModulePresetEntry {
            module: "Delay".into(),
            name: "Slap".into(),
            snapshots: vec!["Short".into()],
            used_by: vec!["Preset Fender".into()],
            snapshot_used_by: vec![String::new()],
            snapshot_info: Vec::new(),
        }];
        let p = pick(&[]);
        let items = module_menu("Delay", Some(&p), &presets, false);
        let find = |id: &str| items.iter().find(|i| i.id == id).cloned().unwrap();
        assert!(find("save").disabled.is_some(), "nothing to save");
        assert!(find("revert").disabled.is_some());
        assert!(find("delete_snapshot").disabled.is_some(), "the only snapshot");
        assert!(find("delete_preset").disabled.as_deref().is_some_and(|w| w.contains("Preset Fender")));
        assert!(module_menu("Delay", Some(&p), &presets, true).iter().any(|i| i.id == "save" && i.disabled.is_none()));
    }
}
