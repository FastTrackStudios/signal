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

use signal_widgets::{BrowseChip, BrowseEntry, BrowseScope, SoundBrowser};

use crate::kit::{Button, MenuItem, PickOption, Picked, PresetBar};
use crate::preset_look::Look;
use crate::theme::{FAINT, MUTED, TEXT};

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
        p.blocks.iter().any(|b| {
            chain
                .iter()
                .any(|(n, _, edited)| *edited && n.eq_ignore_ascii_case(b))
        })
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
        MenuItem::name(
            "rename_snapshot",
            "Rename snapshot…",
            "Rename",
            snap,
            others,
        ),
        MenuItem::delete("delete_snapshot", "Delete snapshot", refused),
    ]
}

/// The menu for one module preset (and, given one, one of its snapshots
/// first): rename, duplicate, delete.
#[must_use]
pub fn preset_items(
    entry: &ModulePresetEntry,
    siblings: &[String],
    snapshot: Option<&str>,
) -> Vec<MenuItem> {
    let others: Vec<String> = siblings
        .iter()
        .filter(|n| !n.eq_ignore_ascii_case(&entry.name))
        .cloned()
        .collect();
    let mut items = snapshot
        .map(|s| snapshot_items(entry, s))
        .unwrap_or_default();
    items.push(MenuItem::head(format!("Preset · {}", entry.name)));
    items.push(MenuItem::name(
        "rename_preset",
        "Rename preset…",
        "Rename",
        &entry.name,
        others,
    ));
    items.push(MenuItem::name(
        "duplicate_preset",
        "Duplicate preset…",
        "Duplicate",
        next_name(&entry.name, siblings),
        siblings.to_vec(),
    ));
    items.push(MenuItem::delete(
        "delete_preset",
        "Delete preset",
        in_use(&entry.used_by),
    ));
    items
}

/// Whether the right sidebar is full (a phone's width) or minimal — the
/// bar's toggle steps it; provided at the rig's root.
#[derive(Clone, Copy)]
pub struct RightFull(pub Signal<bool>);

/// The sidebar's width now: a phone's when full, else the minimal width.
fn sidebar_w() -> &'static str {
    crate::theme::sidebar_w(try_consume_context::<RightFull>().is_some_and(|f| (f.0)()))
}

/// The sidebar's tabs, two levels deep: the three things a sound is made
/// of, then the blocks a module holds ("All" is the module itself).
const FAMILIES: [(&str, &[&str]); 3] = [
    ("Preset", &[]),
    ("Core", &["Drive", "Amp"]),
    ("Time", &["Delay", "Reverb"]),
];

/// The family a module's tab sits under.
fn family_of(module: &str) -> Option<(&'static str, &'static [&'static str])> {
    FAMILIES.iter().copied().find(|(f, subs)| {
        f.eq_ignore_ascii_case(module) || subs.iter().any(|m| m.eq_ignore_ascii_case(module))
    })
}

/// Whether `module` is the Core (its presets are the rig presets).
fn is_core(module: &str) -> bool {
    module.eq_ignore_ascii_case("Core")
}

/// Whether `module` is the presets' (`tones.styx`): sounds made of a Core,
/// a Time, block presets — shared by every patch naming one.
fn is_tone(module: &str) -> bool {
    module.eq_ignore_ascii_case("Preset")
}

/// The preset a module's menu acts on: the one the patch plays — or, for
/// a patch that plays no Core, the Core its tone would be saved into.
fn menu_target(module: &str, pick: Option<&ModulePick>, presets: &[ModulePresetEntry]) -> String {
    pick.map(|p| p.preset.clone())
        .or_else(|| is_core(module).then(|| presets.last().map(|p| p.name.clone())).flatten())
        .unwrap_or_default()
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
    let entry = pick.and_then(|p| {
        presets
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(&p.preset))
    });
    let snap = pick
        .map(|p| played_snapshot(p, presets))
        .unwrap_or_default();
    // A preset is the whole patch: saving it always takes what plays.
    let no_edits = (!modified && !is_tone(module)).then(|| "No edits on this patch".to_string());
    let core = is_core(module) || is_tone(module);
    let mut items = vec![MenuItem::head(format!("{module} on this patch"))];
    // A patch that plays no Core yet can still add its tone to one — the
    // Core presets are built a patch at a time.
    if is_core(module)
        && entry.is_none()
        && let Some(target) = presets.last()
    {
        items.push(MenuItem::name(
            "save_snapshot",
            format!("Save into “{}” as…", target.name),
            "Save",
            next_name("Snapshot", &target.snapshots),
            target.snapshots.clone(),
        ));
    }
    match entry {
        Some(e) => {
            items.push(MenuItem::run("save", if is_tone(module) { format!("Save to “{}”", e.name) } else { format!("Save to “{snap}”") }).unless(no_edits.clone()));
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
        // A Core's snapshots are renamed and deleted in the library.
        items.extend(preset_items(e, &names, (!core).then_some(snap.as_str())));
    }
    items.push(MenuItem::sep());
    items.push(MenuItem::run(
        "manage",
        format!("All {module} presets in the library"),
    ));
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
    let (m, pr, sn, text) = (
        module.to_string(),
        preset.to_string(),
        snapshot.to_string(),
        p.text,
    );
    // The presets (`tones.styx`): their own calls.
    if is_tone(module) {
        match p.id {
            "save" => send(rig, move |r| async move {
                let _ = r.save_tone(pr).await;
            }),
            "save_preset" | "save_snapshot" => send(rig, move |r| async move {
                let _ = r.save_tone(text).await;
            }),
            "rename_preset" => send(rig, move |r| async move {
                let _ = r.rename_tone(pr, text).await;
            }),
            "duplicate_preset" => send(rig, move |r| async move {
                let _ = r.duplicate_tone(pr, text).await;
            }),
            "delete_preset" => send(rig, move |r| async move {
                let _ = r.delete_tone(pr).await;
            }),
            _ => {}
        }
        return;
    }
    // The Core's presets are the rig presets, not a module's: their own
    // calls. (Revert and the library work as for any module; a Core's
    // snapshots have no rename or delete call yet.)
    if is_core(module) && !matches!(p.id, "revert" | "manage") {
        match p.id {
            "save" => send(rig, move |r| async move {
                let _ = r.save_core_snapshot(pr, sn).await;
            }),
            "save_snapshot" => send(rig, move |r| async move {
                let _ = r.save_core_snapshot(pr, text).await;
            }),
            "save_preset" => send(rig, move |r| async move {
                let _ = r.save_core_snapshot(text, "Main".to_string()).await;
            }),
            "rename_preset" => send(rig, move |r| async move {
                let _ = r.rename_rig_preset(pr, text).await;
            }),
            "duplicate_preset" => send(rig, move |r| async move {
                let _ = r.duplicate_rig_preset(pr, text).await;
            }),
            "delete_preset" => send(rig, move |r| async move {
                let _ = r.delete_rig_preset(pr).await;
            }),
            _ => {}
        }
        return;
    }
    match p.id {
        "save" => send(rig, move |r| async move {
            let _ = r.save_module_snapshot(m, pr, sn).await;
        }),
        "save_snapshot" => send(rig, move |r| async move {
            let _ = r.save_module_snapshot(m, pr, text).await;
        }),
        "save_preset" => {
            let sn = if sn.is_empty() {
                "Default".to_string()
            } else {
                sn
            };
            send(rig, move |r| async move {
                let _ = r.save_module_snapshot(m, text, sn).await;
            });
        }
        "revert" => send(rig, move |r| async move {
            let _ = r.revert_module(m).await;
        }),
        "rename_preset" => send(rig, move |r| async move {
            let _ = r.rename_module_preset(m, pr, text).await;
        }),
        "duplicate_preset" => send(rig, move |r| async move {
            let _ = r.duplicate_module_preset(m, pr, text).await;
        }),
        "delete_preset" => send(rig, move |r| async move {
            let _ = r.delete_module_preset(m, pr).await;
        }),
        "rename_snapshot" => send(rig, move |r| async move {
            let _ = r.rename_module_snapshot(m, pr, sn, text).await;
        }),
        "delete_snapshot" => send(rig, move |r| async move {
            let _ = r.delete_module_snapshot(m, pr, sn).await;
        }),
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
pub fn module_options(
    presets: &[ModulePresetEntry],
    pick: Option<&ModulePick>,
) -> (Vec<PickOption>, Vec<(String, String)>) {
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

/// Between a preset and a snapshot in a browser row's id.
const SEP: char = '\u{1f}';

/// The id's preset and snapshot (empty: the preset itself, its first).
fn split_id(id: &str) -> (String, String) {
    match id.split_once(SEP) {
        Some((p, s)) => (p.to_string(), s.to_string()),
        None => (id.to_string(), String::new()),
    }
}

/// A row's picture: a Time snapshot's picks, else its block's shape and
/// value.
#[component]
fn RowArt(look: Look, subs: Vec<(String, Look)>, lit: bool) -> Element {
    if !subs.is_empty() {
        return rsx! {
            for (m, l) in subs {
                SubPick { key: "{m}", module: m, look: l, lit }
            }
        };
    }
    if look.group == crate::preset_look::OFF || look.shape == crate::preset_look::Shape::None {
        return rsx! { crate::preset_look::ValueChip { value: look.value.clone(), lit } };
    }
    rsx! {
        crate::preset_look::ShapeView { shape: look.shape.clone(), w: 40, h: 12, lit }
        crate::preset_look::ValueChip { value: look.value.clone(), lit }
    }
}

/// Pictures by row id, for the browser's `art`.
fn art_of(arts: std::collections::HashMap<String, (Look, Vec<(String, Look)>, bool)>) -> Callback<String, Element> {
    Callback::new(move |id: String| match arts.get(&id).cloned() {
        Some((look, subs, lit)) => rsx! { RowArt { look, subs, lit } },
        None => rsx! {},
    })
}

/// The sidebar — the shared [`SoundBrowser`](signal_widgets::SoundBrowser),
/// the same one the keys rig loads its layer and engine presets through.
/// `revision` is the performance model's, so a pick made here (or anywhere)
/// re-reads what is playing; `chain` is the live blocks, for what is edited.
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
    let query = use_signal(String::new);
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
    let played = pick
        .as_ref()
        .map(|p| played_snapshot(p, &presets))
        .unwrap_or_default();
    let (options, targets) = module_options(&presets, pick.as_ref());
    // Play a row: `audition` first remembers the patch, so Esc can put it
    // back.
    let play = {
        let rig = rig.clone();
        let module = module.clone();
        move |id: String, audition: bool| {
            let (m, (preset, snapshot)) = (module.clone(), split_id(&id));
            if let Some(r) = rig.clone() {
                let mut refresh = refresh;
                spawn(async move {
                    if audition {
                        let _ = r.browse_audition_begin().await;
                    }
                    let _ = r.choose_module(m, preset, snapshot).await;
                    refresh += 1;
                });
            }
        }
    };
    let choose = {
        let play = play.clone();
        move |preset: String, snapshot: String| play(format!("{preset}{SEP}{snapshot}"), false)
    };

    // The rows: each preset, its snapshots under it, narrowed by the search
    // (preset or snapshot name).
    let words: Vec<String> = query().split_whitespace().map(str::to_lowercase).collect();
    let comp_all = comp.clone().unwrap_or_default();
    let mut arts = std::collections::HashMap::new();
    let mut entries = Vec::new();
    for entry in &presets {
        let here = pick.as_ref().is_some_and(|p| p.preset.eq_ignore_ascii_case(&entry.name));
        let children: Vec<BrowseEntry> = entry
            .snapshots
            .iter()
            .enumerate()
            .filter(|(_, s)| hit(&words, &[&entry.name, s]))
            .map(|(i, snap)| {
                let lit = here
                    && pick.as_ref().is_some_and(|p| {
                        p.snapshot.eq_ignore_ascii_case(snap) || (p.snapshot.is_empty() && i == 0)
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
                    .filter_map(|m| pick_look(&comp_all, m).map(|l| (m.module.clone(), l)))
                    .collect();
                let id = format!("{}{SEP}{snap}", entry.name);
                arts.insert(id.clone(), (look, subs, lit));
                BrowseEntry {
                    id,
                    name: snap.clone(),
                    sub: info.captures.iter().take(2).cloned().collect::<Vec<_>>().join(" · "),
                    live: lit,
                    modified: lit && modified,
                    menu: preset_items(entry, &names, Some(snap)),
                    ..BrowseEntry::default()
                }
            })
            .collect();
        if children.is_empty() {
            continue;
        }
        entries.push(BrowseEntry {
            id: entry.name.clone(),
            name: entry.name.clone(),
            note: entry.snapshots.len().to_string(),
            live: here,
            menu: preset_items(entry, &names, None),
            children,
            ..BrowseEntry::default()
        });
    }
    let total = entries.len();

    rsx! {
        SoundBrowser {
            scopes: vec![BrowseScope {
                id: "module".into(),
                label: if is_tone(&module) { "Presets".to_string() } else { format!("{module} presets") },
                target: format!("→ {module} on this patch"),
            }],
            scope: "module",
            on_scope: |_| {},
            entries,
            total,
            query,
            on_load: {
                let play = play.clone();
                move |id: String| play(id, false)
            },
            audition: true,
            on_preview: {
                let play = play.clone();
                move |id: String| play(id, true)
            },
            on_audition_end: {
                let rig = rig.clone();
                move |keep: bool| {
                    if let Some(r) = rig.clone() {
                        let mut refresh = refresh;
                        spawn(async move {
                            let _ = r.browse_audition_end(keep).await;
                            refresh += 1;
                        });
                    }
                }
            },
            menu: module_menu(&module, pick.as_ref(), &presets, modified),
            on_menu: {
                let rig = rig.clone();
                let module = module.clone();
                let preset = menu_target(&module, pick.as_ref(), &presets);
                let played = played.clone();
                move |p: Picked| module_act(&rig, library, &module, &preset, &played, p)
            },
            on_entry_menu: {
                let rig = rig.clone();
                let module = module.clone();
                move |(id, p): (String, Picked)| {
                    let (preset, snap) = split_id(&id);
                    module_act(&rig, library, &module, &preset, &snap, p);
                }
            },
            art: art_of(arts),
            empty: if presets.is_empty() {
                if is_tone(&module) {
                    "No presets yet. In Presets mode, ⋯ › Make presets from this profile's patches turns each patch's sound into one.".to_string()
                } else {
                    format!("No {module} presets yet — dial one in and use ⋯ › Save as new preset.")
                }
            } else {
                "No preset matches.".to_string()
            },
            width: sidebar_w(),
            right: true,
            on_close: move |()| selected.set(None),
            // Preset · Core · Time, then the blocks of the one picked —
            // the pick pressed in, as in the bar.
            {
                let family = family_of(&module);
                let subs: &[&str] = family.map_or(&[], |(_, subs)| subs);
                rsx! {
                    div { style: "display: flex; gap: 2px; padding: 4px 0 2px;",
                        for (f, _) in FAMILIES {
                            {
                                let on = family.is_some_and(|(x, _)| x == f);
                                rsx! {
                                    button { key: "{f}",
                                        class: if on { "" } else { "sg-hover" },
                                        style: format!(
                                            "flex: 1 1 0; min-width: 0; padding: 5px 0; border: none; border-radius: {}; font-size: {}; \
                                             font-weight: 600; cursor: pointer; {}",
                                            crate::theme::R_SM,
                                            crate::theme::T_BODY,
                                            if on { crate::theme::PRESSED.to_string() } else { format!("background: transparent; color: {MUTED};") },
                                        ),
                                        onclick: move |_| selected.set(Some(Selection::Module(f.to_string()))),
                                        "{f}"
                                    }
                                }
                            }
                        }
                    }
                    if let Some((f, _)) = family.filter(|_| !subs.is_empty()) {
                        div { style: "display: flex; gap: 2px; padding: 2px 0 6px;",
                            for (label, target) in std::iter::once(("All", f)).chain(subs.iter().map(|m| (*m, *m))) {
                                {
                                    let on = module.eq_ignore_ascii_case(target);
                                    rsx! {
                                        button { key: "{label}",
                                            class: if on { "" } else { "sg-ink" },
                                            style: format!(
                                                "padding: 2px 9px; border: none; border-radius: {}; font-size: {}; font-weight: 600; \
                                                 cursor: pointer; {}",
                                                crate::theme::R_SM,
                                                crate::theme::T_SMALL,
                                                if on { crate::theme::PRESSED } else { "background: transparent;" },
                                            ),
                                            title: if label == "All" { format!("The {f} module") } else { label.to_string() },
                                            onclick: move |_| selected.set(Some(Selection::Module(target.to_string()))),
                                            "{label}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // What the module plays, and every way to change or keep it.
            PresetBar {
                label: module.clone(),
                name: pick.as_ref().map(|p| p.preset.clone()).unwrap_or_default(),
                sub: played.clone(),
                modified,
                live: pick.is_some(),
                // A patch on no preset plays its own sound — say so,
                // rather than "nothing", which reads as silence.
                placeholder: if module.eq_ignore_ascii_case("Preset") { "Its own sound" } else { "Nothing picked" },
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
                    let preset = menu_target(&module, pick.as_ref(), &presets);
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
    let entry = comp.modules.iter().find(|m| {
        m.module.eq_ignore_ascii_case(&pick.module) && m.name.eq_ignore_ascii_case(&pick.preset)
    })?;
    let i = entry
        .snapshots
        .iter()
        .position(|s| s.eq_ignore_ascii_case(&pick.snapshot))
        .unwrap_or(0);
    let block = entry.snapshot_info.get(i)?.blocks.first()?;
    block_look(comp, &block.preset)
}


/// A Time snapshot's Delay or Reverb pick, compact: a dot in its engine's
/// family colour (delay blue, reverb purple) and its one value, in a
/// fixed-width slot so the values line up down the list and the name keeps
/// the room.
#[component]
fn SubPick(module: String, look: Look, lit: bool) -> Element {
    let slot = "flex-shrink: 0; width: 46px; display: flex; align-items: center; gap: 4px; overflow: hidden;";
    if look.group == crate::preset_look::OFF {
        return rsx! {
            span { style: "{slot}", title: "{module}: off",
                span { style: "width: 6px; height: 6px; border-radius: 999px; flex-shrink: 0; border: 1px solid {crate::theme::DIM};" }
                span { style: "font-size: {crate::theme::T_META}; color: {crate::theme::FAINT};", "off" }
            }
        };
    }
    let dot = crate::preset_look::engine_swatch(&look.block_type, &look.engine);
    let ink = if lit { TEXT } else { MUTED };
    rsx! {
        span { style: "{slot}", title: "{module}: {look.engine} {look.value}",
            span { style: "width: 6px; height: 6px; border-radius: 999px; flex-shrink: 0; background: {dot};" }
            span { style: "font-size: {crate::theme::T_META}; font-family: monospace; color: {ink}; white-space: nowrap;", "{look.value}" }
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
        MenuItem::name(
            "duplicate",
            "Duplicate…",
            "Duplicate",
            next_name(&p.name, siblings),
            siblings.to_vec(),
        ),
        MenuItem::delete("delete", "Delete", in_use(&p.used_by)),
    ]
}

/// Do what a block preset menu item says. `block` is the live block (for
/// saving and reverting), `preset` the preset the item is about.
fn block_act(rig: &Option<RigClient>, block: &str, block_id: &str, preset: &str, p: Picked) {
    let (b, id, pr, text) = (
        block.to_string(),
        block_id.to_string(),
        preset.to_string(),
        p.text,
    );
    match p.id {
        "save" => send(rig, move |r| async move {
            let _ = r.save_block_preset(b, pr).await;
        }),
        "save_as" => send(rig, move |r| async move {
            let _ = r.save_block_preset(b, text).await;
        }),
        "revert" => send(rig, move |r| async move {
            let _ = r.clear_block_overrides(id).await;
        }),
        "rename" => send(rig, move |r| async move {
            let _ = r.rename_block_preset(pr, text).await;
        }),
        "duplicate" => send(rig, move |r| async move {
            let _ = r.duplicate_block_preset(pr, text).await;
        }),
        "delete" => send(rig, move |r| async move {
            let _ = r.delete_block_preset(pr).await;
        }),
        _ => {}
    }
}

/// Whether a preset matches every word of a search, over its name, group
/// and engine.
fn hit(words: &[String], parts: &[&str]) -> bool {
    let hay = parts.join(" ").to_lowercase();
    words.iter().all(|w| hay.contains(w))
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
            crate::preset_look::ShapeView { shape: look.shape.clone(), w: 370, h: 28, lit: true }
            div { style: "display: flex; align-items: center; gap: 6px;",
                crate::preset_look::EngineChip { engine: look.engine.clone(), default: look.engine_default }
                span { style: "font-size: {crate::theme::T_META}; color: {FAINT};", "{look.group}" }
                div { style: "flex: 1 1 0;" }
                crate::preset_look::ValueChip { value: look.value.clone(), lit: true }
            }
        }
    }
}

/// A block's presets (every block preset of its type), grouped by what they
/// do and drawn, in the shared browser: what plays on top with the bar's
/// actions, the groups as chips. A tap puts another on the active patch's
/// block (`choose_block`); arrows audition, Esc puts the patch back.
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
    let query = use_signal(String::new);
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
    let live = chain
        .iter()
        .find(|(n, _, _)| n.eq_ignore_ascii_case(&block))
        .cloned();
    let modified = live.as_ref().is_some_and(|(_, _, e)| *e);
    let block_id = live.map(|(_, id, _)| id).unwrap_or_default();
    let kind = match block_type.as_str() {
        "delay" => "Delay".to_string(),
        "reverb" => "Reverb".to_string(),
        other => {
            let mut c = other.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        }
    };
    let play = {
        let rig = rig.clone();
        let block = block.clone();
        move |name: String, audition: bool| {
            let b = block.clone();
            if let Some(r) = rig.clone() {
                let mut refresh = refresh;
                spawn(async move {
                    if audition {
                        let _ = r.browse_audition_begin().await;
                    }
                    let _ = r.choose_block(b, name).await;
                    refresh += 1;
                });
            }
        }
    };
    let choose = {
        let play = play.clone();
        move |name: String| play(name, false)
    };
    let at = playing
        .as_ref()
        .and_then(|p| names.iter().position(|n| n.eq_ignore_ascii_case(p)));
    let current_look = at
        .and_then(|i| presets.get(i))
        .map(crate::preset_look::look)
        .unwrap_or_default();
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

    // The rows: grouped, then narrowed by the chip and the search.
    let groups = crate::preset_look::grouped(&presets);
    let chips: Vec<BrowseChip> = groups
        .iter()
        .filter(|(g, _)| !g.is_empty() && g != crate::preset_look::OFF)
        .map(|(g, v)| BrowseChip {
            id: g.clone(),
            label: format!("{g} {}", v.len()),
            on: only() == *g,
        })
        .collect();
    let words: Vec<String> = query().split_whitespace().map(str::to_lowercase).collect();
    let mut arts = std::collections::HashMap::new();
    let mut entries = Vec::new();
    for (group, rows) in groups {
        if !(only().is_empty() || group == only() || group == crate::preset_look::OFF) {
            continue;
        }
        for (p, look) in rows {
            if !hit(&words, &[&p.name, &look.group, &look.engine]) {
                continue;
            }
            let lit = playing.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(&p.name));
            arts.insert(p.name.clone(), (look.clone(), Vec::new(), lit));
            entries.push(BrowseEntry {
                id: p.name.clone(),
                name: p.name.clone(),
                sub: if p.used_by.is_empty() {
                    look.engine.clone()
                } else {
                    format!("{} · in {}", look.engine, p.used_by.len())
                },
                live: lit,
                modified: lit && modified,
                // Off sits alone at the top, without a heading.
                group: if group == crate::preset_look::OFF { String::new() } else { group.clone() },
                menu: block_preset_items(&p, &names),
                ..BrowseEntry::default()
            });
        }
    }
    let total = entries.len();

    rsx! {
        SoundBrowser {
            scopes: vec![BrowseScope {
                id: "block".into(),
                label: format!("{kind} presets"),
                target: format!("→ {block} on this patch"),
            }],
            scope: "block",
            on_scope: |_| {},
            entries,
            total,
            query,
            on_load: {
                let play = play.clone();
                move |id: String| play(id, false)
            },
            audition: true,
            on_preview: {
                let play = play.clone();
                move |id: String| play(id, true)
            },
            on_audition_end: {
                let rig = rig.clone();
                move |keep: bool| {
                    if let Some(r) = rig.clone() {
                        let mut refresh = refresh;
                        spawn(async move {
                            let _ = r.browse_audition_end(keep).await;
                            refresh += 1;
                        });
                    }
                }
            },
            menu: menu.clone(),
            on_menu: {
                let rig = rig.clone();
                let (block, id, preset) = (block.clone(), block_id.clone(), playing.clone().unwrap_or_default());
                move |p: Picked| block_act(&rig, &block, &id, &preset, p)
            },
            on_entry_menu: {
                let rig = rig.clone();
                let (block, id) = (block.clone(), block_id.clone());
                move |(preset, p): (String, Picked)| block_act(&rig, &block, &id, &preset, p)
            },
            chips: if chips.len() > 1 { chips } else { Vec::new() },
            on_chip: move |g: String| only.set(if only() == g { String::new() } else { g }),
            art: art_of(arts),
            empty: if presets.is_empty() {
                format!("No {kind} presets yet — dial the block in and use ⋯ › Save as new preset.")
            } else {
                "No preset matches.".to_string()
            },
            width: sidebar_w(),
            right: true,
            on_close: move |()| on_close.call(()),
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
        assert!(
            find("delete_snapshot").disabled.is_some(),
            "the only snapshot"
        );
        assert!(
            find("delete_preset")
                .disabled
                .as_deref()
                .is_some_and(|w| w.contains("Preset Fender"))
        );
        assert!(
            module_menu("Delay", Some(&p), &presets, true)
                .iter()
                .any(|i| i.id == "save" && i.disabled.is_none())
        );
    }
}
