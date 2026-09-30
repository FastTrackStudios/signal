//! **The browser** — the rig's left sidebar: the shared
//! [`SoundBrowser`](signal_widgets::SoundBrowser), the same one the guitar
//! rig loads its module and block presets through.
//!
//! It is **contextual**: it lists what the selection can hold, and the
//! levels above it are one pick away in its header.
//!
//! | Selected | Levels | Click loads |
//! |---|---|---|
//! | nothing | the profile's stacks | presses that stack |
//! | an engine | engine presets | the whole engine (its lanes) |
//! | a lane | layer presets · module presets · engine presets | the whole lane · module A · its engine |
//! | a module | module presets · layer presets · engine presets | that module · its lane · its engine |
//!
//! A layer level lists soundsources too — a soundsource is a fine thing to
//! put in a lane, it just fills module A. The header's ⋯ saves the lane or
//! the engine as it sounds now; saved presets list first and can be renamed,
//! duplicated and deleted. ↑ ↓ in the search field audition (the rig
//! remembers how it was and Esc puts it back), Enter keeps.

use dioxus::prelude::*;
use signal_keys_proto::keys::KeysRigClient;
use signal_keys_proto::{KeysMixer, KeysPreset};
use signal_widgets::kit::{MenuItem, Picked};
use signal_widgets::{BrowseChip, BrowseEntry, BrowseScope, SoundBrowser};

use crate::control::engine_color;
use crate::selection::{Selection, use_selection};
use crate::state::KeysViewState;

/// How many rows the list draws before it asks you to narrow it. The library
/// is ~41k presets — drawing all of them is what made the rig take half a
/// minute to open.
const MAX_ROWS: usize = 150;

/// A level of the browser.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Level {
    Stacks,
    Engine,
    Layer,
    Module,
}

impl Level {
    const fn id(self) -> &'static str {
        match self {
            Self::Stacks => "stacks",
            Self::Engine => "engine",
            Self::Layer => "layer",
            Self::Module => "module",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        [Self::Stacks, Self::Engine, Self::Layer, Self::Module]
            .into_iter()
            .find(|l| l.id() == id)
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Stacks => "Stacks",
            Self::Engine => "Engine presets",
            Self::Layer => "Layer presets",
            Self::Module => "Module presets",
        }
    }

    /// Whether a library entry of `scope` lists at this level.
    fn accepts(self, scope: &str) -> bool {
        match self {
            Self::Stacks => false,
            Self::Engine => scope == "engine",
            Self::Layer | Self::Module => scope == "layer" || scope == "module",
        }
    }
}

/// The levels a selection can load at — its own first.
fn levels(sel: &Selection) -> Vec<Level> {
    match sel {
        Selection::None => vec![Level::Stacks],
        Selection::Engine(_) => vec![Level::Engine],
        Selection::Layer { .. } => vec![Level::Layer, Level::Module, Level::Engine],
        Selection::Module { .. } => vec![Level::Module, Level::Layer, Level::Engine],
    }
}

fn module_letter(m: u32) -> char {
    (b'A' + m.min(25) as u8) as char
}

/// Where a load at `level` lands, for this selection.
fn target(sel: &Selection, level: Level) -> String {
    match (level, sel) {
        (Level::Stacks, _) => String::new(),
        (Level::Engine, s) => s.engine().map(|e| format!("→ {e}")).unwrap_or_default(),
        (Level::Layer, s) => s.layer().map(|l| format!("→ {l}")).unwrap_or_default(),
        (Level::Module, s) => s
            .layer()
            .map(|l| format!("→ {l} · module {}", module_letter(s.module())))
            .unwrap_or_default(),
    }
}

/// What a lane plays, from the live mixer: its preset and each module's
/// source.
fn lane_now(mixer: &KeysMixer, layer: &str) -> (String, Vec<String>) {
    mixer
        .engines
        .iter()
        .flat_map(|e| &e.layers)
        .find(|l| l.name == layer)
        .map(|l| (l.preset.clone(), l.modules.iter().map(|m| m.patch.clone()).collect()))
        .unwrap_or_default()
}

/// Whether preset `p` is what the selection plays at `level`.
fn is_live(p: &KeysPreset, level: Level, sel: &Selection, mixer: &KeysMixer) -> bool {
    match level {
        Level::Stacks => false,
        Level::Engine => sel.engine().is_some_and(|e| {
            let lanes: Vec<&signal_keys_proto::KeysLayerModel> = mixer
                .engines
                .iter()
                .filter(|x| x.name == e)
                .flat_map(|x| &x.layers)
                .collect();
            !lanes.is_empty() && lanes.iter().all(|l| l.preset == p.name)
        }),
        Level::Layer | Level::Module => sel.layer().is_some_and(|l| {
            let (preset, sources) = lane_now(mixer, l);
            if p.user {
                return preset == p.name;
            }
            let m = if level == Level::Layer { 0 } else { sel.module() as usize };
            sources.get(m).is_some_and(|s| *s == p.name)
        }),
    }
}

/// A saved preset's actions.
fn user_menu(p: &KeysPreset, taken: &[String]) -> Vec<MenuItem> {
    let others: Vec<String> = taken
        .iter()
        .filter(|n| !n.eq_ignore_ascii_case(&p.name))
        .cloned()
        .collect();
    vec![
        MenuItem::head(format!("{} · {}", p.kind, p.name)),
        MenuItem::name("rename", "Rename…", "Rename", &p.name, others),
        MenuItem::name(
            "duplicate",
            "Duplicate…",
            "Duplicate",
            next_name(&p.name, taken),
            taken.to_vec(),
        ),
        MenuItem::delete("delete", "Delete", None),
    ]
}

/// "Warm Keys 2", "Warm Keys 3"… — the first name not taken.
fn next_name(base: &str, taken: &[String]) -> String {
    let base = base.trim();
    let base = if base.is_empty() { "New" } else { base };
    if !taken.iter().any(|t| t.eq_ignore_ascii_case(base)) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|c| !taken.iter().any(|t| t.eq_ignore_ascii_case(c)))
        .unwrap_or_else(|| base.to_string())
}

/// The saved presets' names at `scope` ("engine" / "layer").
fn saved_names(library: &[KeysPreset], scope: &str) -> Vec<String> {
    library
        .iter()
        .filter(|p| p.user && p.scope == scope)
        .map(|p| p.name.clone())
        .collect()
}

/// The rows for `level`: saved presets first, then the library, filtered by
/// the engine chip and the search, capped at [`MAX_ROWS`]. Returns the rows
/// and how many matched.
fn rows(
    library: &[KeysPreset],
    level: Level,
    sel: &Selection,
    mixer: &KeysMixer,
    query: &str,
    all_engines: bool,
) -> (Vec<BrowseEntry>, usize) {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let engine = sel.engine();
    let mut hits: Vec<(usize, &KeysPreset)> = library
        .iter()
        .enumerate()
        .filter(|(_, p)| level.accepts(&p.scope))
        .filter(|(_, p)| match (engine, all_engines) {
            (Some(e), false) => p.tags.iter().any(|t| t == e),
            _ => true,
        })
        .filter(|(_, p)| {
            let hay = format!("{} {}", p.name, p.kind).to_lowercase();
            words.iter().all(|w| hay.contains(w))
        })
        .collect();
    let total = hits.len();
    hits.sort_by(|a, b| {
        b.1.user
            .cmp(&a.1.user)
            .then_with(|| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()))
    });
    hits.truncate(MAX_ROWS);
    let taken = |p: &KeysPreset| saved_names(library, &p.scope);
    let entries = hits
        .into_iter()
        .map(|(i, p)| BrowseEntry {
            id: i.to_string(),
            name: p.name.clone(),
            sub: p.kind.clone(),
            note: String::new(),
            live: is_live(p, level, sel, mixer),
            modified: false,
            user: p.user,
            group: if p.user { "Saved".into() } else { "Library".into() },
            menu: if p.user { user_menu(p, &taken(p)) } else { Vec::new() },
            children: p
                .variants
                .iter()
                .enumerate()
                .map(|(n, v)| BrowseEntry {
                    id: format!("{i}:{n}"),
                    name: v.clone(),
                    ..BrowseEntry::default()
                })
                .collect(),
        })
        .collect();
    (entries, total)
}

/// The profile's stacks as rows.
fn stack_rows(state: KeysViewState) -> Vec<BrowseEntry> {
    state
        .perform
        .read()
        .stacks
        .iter()
        .enumerate()
        .map(|(i, s)| BrowseEntry {
            id: format!("stack:{i}"),
            name: s.name.clone(),
            sub: s.blurb.clone(),
            live: s.is_active,
            ..BrowseEntry::default()
        })
        .collect()
}

/// The left sidebar.
///
/// Takes the whole view state (`Copy`, cheap `PartialEq`) rather than the
/// preset `Vec`: the pool is ~41k entries and the rig re-renders on every
/// status push, so a `Vec` prop would deep clone and element-compare it thirty
/// times a second.
#[component]
pub fn Browser(state: KeysViewState) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let selection = use_selection();
    let query = use_signal(String::new);
    // Escape hatch: the engine filter helps until the moment it hides the one
    // sound you want, so it can be dropped without losing the selection.
    let mut all_engines = use_signal(|| false);
    let sel = selection.read().clone();
    let levels_now = levels(&sel);
    // The level showing: the selection's own, until one above it is picked.
    let mut level = use_signal(|| levels_now[0]);
    let mut seen = use_signal(|| sel.clone());
    if *seen.peek() != sel {
        seen.set(sel.clone());
        level.set(levels_now[0]);
    }
    let lvl = if levels_now.contains(&level()) {
        level()
    } else {
        levels_now[0]
    };

    // Rows, recomputed only when what they depend on changes — never on a
    // status push.
    let computed = use_memo(move || {
        let sel = selection.read().clone();
        let lvl = level();
        let q = query();
        if lvl == Level::Stacks {
            let rows = stack_rows(state);
            let n = rows.len();
            return (rows, n);
        }
        rows(
            &state.presets.read(),
            lvl,
            &sel,
            &state.mixer.read(),
            &q,
            all_engines(),
        )
    });
    let (entries, total) = computed.read().clone();

    let accent = sel.engine().map_or("#94a3b8", engine_color).to_string();
    let scopes: Vec<BrowseScope> = levels_now
        .iter()
        .map(|l| BrowseScope {
            id: l.id().to_string(),
            label: l.label().to_string(),
            target: target(&sel, *l),
        })
        .collect();

    // The level's own actions: save what the selection plays.
    let menu = {
        let library = state.presets.read();
        let mut items = Vec::new();
        if let Some(l) = sel.layer() {
            let taken = saved_names(&library, "layer");
            items.push(MenuItem::head(format!("Lane · {l}")));
            items.push(MenuItem::name(
                "save_layer",
                "Save layer preset…",
                "Save",
                next_name(l, &taken),
                taken,
            ));
        }
        if let Some(e) = sel.engine() {
            let taken = saved_names(&library, "engine");
            items.push(MenuItem::head(format!("Engine · {e}")));
            items.push(MenuItem::name(
                "save_engine",
                "Save engine preset…",
                "Save",
                next_name(e, &taken),
                taken,
            ));
        }
        items
    };
    let chips = sel
        .engine()
        .filter(|_| lvl != Level::Stacks)
        .map(|e| {
            vec![BrowseChip {
                id: "engine".into(),
                label: if all_engines() {
                    "all engines".into()
                } else {
                    format!("{e} only")
                },
                on: !all_engines(),
            }]
        })
        .unwrap_or_default();
    let empty = match lvl {
        Level::Stacks => "This profile has no stacks yet.".to_string(),
        Level::Engine => "No engine presets yet — ⋯ › Save engine preset keeps this engine as it sounds.".to_string(),
        _ if state.presets.read().is_empty() => {
            "The library is empty — build a pack, or point the rig at one.".to_string()
        }
        _ => "Nothing here for this engine. Try 'all engines'.".to_string(),
    };

    let on_load = {
        let rig = rig.clone();
        let sel = sel.clone();
        move |id: String| load(rig.clone(), sel.clone(), lvl, id, false)
    };
    let preview = {
        let rig = rig.clone();
        let sel = sel.clone();
        move |id: String| load(rig.clone(), sel.clone(), lvl, id, true)
    };
    let end = {
        let rig = rig.clone();
        move |keep: bool| {
            let rig = rig.clone();
            spawn(async move {
                if let Some(r) = rig {
                    let _ = r.audition_end(keep).await;
                }
            });
        }
    };
    let on_menu = {
        let rig = rig.clone();
        let sel = sel.clone();
        move |p: Picked| {
            let (rig, sel) = (rig.clone(), sel.clone());
            spawn(async move {
                let Some(r) = rig else { return };
                match p.id {
                    "save_layer" => {
                        if let Some(l) = sel.layer() {
                            let _ = r.save_layer_preset(l.to_string(), p.text).await;
                        }
                    }
                    "save_engine" => {
                        if let Some(e) = sel.engine() {
                            let _ = r.save_engine_preset(e.to_string(), p.text).await;
                        }
                    }
                    _ => {}
                }
            });
        }
    };
    let on_entry_menu = {
        let rig = rig.clone();
        move |(id, p): (String, Picked)| {
            let Ok(i) = id.parse::<u32>() else { return };
            let rig = rig.clone();
            spawn(async move {
                let Some(r) = rig else { return };
                let _ = match p.id {
                    "rename" => r.rename_user_preset(i, p.text).await,
                    "duplicate" => r.duplicate_user_preset(i, p.text).await,
                    "delete" => r.delete_user_preset(i).await,
                    _ => Ok(()),
                };
            });
        }
    };

    rsx! {
        SoundBrowser {
            scopes,
            scope: lvl.id().to_string(),
            on_scope: move |id: String| {
                if let Some(l) = Level::from_id(&id) {
                    level.set(l);
                }
            },
            entries,
            total,
            query,
            on_load,
            audition: lvl != Level::Stacks,
            on_preview: preview,
            on_audition_end: end,
            menu,
            on_menu,
            on_entry_menu,
            chips,
            on_chip: move |_| all_engines.toggle(),
            empty,
            accent,
            width: "260px",
        }
    }
}

/// Load row `id` at `level` into the selection. `audition`: remember the
/// rig first, so an Esc can put it back.
fn load(rig: Option<KeysRigClient>, sel: Selection, level: Level, id: String, audition: bool) {
    spawn(async move {
        let Some(r) = rig else { return };
        if let Some(i) = id.strip_prefix("stack:").and_then(|i| i.parse::<u32>().ok()) {
            let _ = r.press_stack(i).await;
            return;
        }
        let (index, variant) = match id.split_once(':') {
            Some((i, n)) => (i.parse::<u32>().ok(), n.parse::<u32>().ok()),
            None => (id.parse::<u32>().ok(), None),
        };
        let Some(index) = index else { return };
        if audition {
            let _ = r.audition_begin().await;
        }
        match level {
            Level::Stacks => {}
            Level::Engine => {
                if let Some(e) = sel.engine() {
                    let _ = r.load_engine_preset(e.to_string(), index).await;
                }
            }
            Level::Layer | Level::Module => {
                let Some(layer) = sel.layer().map(ToString::to_string) else {
                    return;
                };
                let module = if level == Level::Layer { 0 } else { sel.module() };
                match variant {
                    Some(n) => {
                        let _ = r.set_layer_variant(layer, module, index, n).await;
                    }
                    None => {
                        let _ = r.set_layer_patch(layer, module, index).await;
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(name: &str, scope: &str, tag: &str, user: bool) -> KeysPreset {
        KeysPreset {
            name: name.into(),
            kind: "Grand".into(),
            scope: scope.into(),
            tags: vec![tag.into()],
            user,
            ..KeysPreset::default()
        }
    }

    #[test]
    fn saved_presets_list_first_and_levels_filter_by_scope_and_engine() {
        let lib = vec![
            preset("C7 Grand", "layer", "Keys", false),
            preset("Choir", "module", "Pad", false),
            preset("My Piano", "layer", "Keys", true),
            preset("Worship Keys", "engine", "Keys", true),
        ];
        let sel = Selection::Layer {
            engine: "Keys".into(),
            layer: "Keys 1".into(),
        };
        let mixer = KeysMixer::default();
        let (rows_, total) = rows(&lib, Level::Layer, &sel, &mixer, "", false);
        let names: Vec<&str> = rows_.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["My Piano", "C7 Grand"]);
        assert_eq!(total, 2);
        assert_eq!(rows_[0].group, "Saved");
        assert!(!rows_[0].menu.is_empty() && rows_[1].menu.is_empty());
        // All engines lets the Pad's soundsource in.
        assert_eq!(rows(&lib, Level::Layer, &sel, &mixer, "", true).1, 3);
        // The engine level lists only engine presets.
        let (e, _) = rows(&lib, Level::Engine, &sel, &mixer, "", false);
        assert_eq!(e.len(), 1);
        // Search is every word, over name and kind.
        assert_eq!(rows(&lib, Level::Layer, &sel, &mixer, "grand c7", false).1, 1);
    }

    #[test]
    fn a_lane_can_load_at_its_module_and_engine_too() {
        let sel = Selection::Module {
            engine: "Pad".into(),
            layer: "Pad".into(),
            module: 1,
        };
        assert_eq!(levels(&sel), [Level::Module, Level::Layer, Level::Engine]);
        assert_eq!(target(&sel, Level::Module), "→ Pad · module B");
        assert_eq!(target(&sel, Level::Engine), "→ Pad");
    }
}
