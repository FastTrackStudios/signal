//! **The browser** — the rig's left sidebar, like the guitar rig's.
//!
//! Everything in the library is a **default with variations behind it**: "C7
//! Grand" is the entry you load, and "Felt", "Lid Down", "Close Mic'd" are
//! variations of that same instrument rather than four separate library
//! entries. So the browser is two levels deep everywhere — a list of defaults,
//! and inside any of them, its variations. (Packs do not author variations
//! yet; the navigation is here so they land somewhere when they do.)
//!
//! Above that, it is **contextual**: it lists what the selection can hold.
//!
//! | Selected | Root shows | Click loads |
//! |---|---|---|
//! | nothing | the profile — the rig's stacks | presses that stack |
//! | an engine | engine programs | the whole engine (`load_preset`) |
//! | a lane | layer presets | into that lane's module A |
//! | a module | module presets | into that module |
//!
//! Items are [`signal_browser::ColumnItem`]s and the level names and sort
//! modes are that crate's [`NavCategory`] / [`SortMode`] — the collection
//! browser's vocabulary, so the rig sidebar and the full browser describe the
//! library the same way.

use dioxus::prelude::*;
use signal_browser::{ColumnItem, NavCategory, SortMode};
use signal_keys_proto::keys::KeysRigClient;
use signal_keys_proto::KeysPreset;

use crate::control::engine_color;
use crate::selection::{use_selection, Selection};
use crate::state::KeysViewState;

/// How many rows the list draws before it asks you to narrow it. The library
/// is ~41k presets — drawing all of them is what made the rig take half a
/// minute to open.
const MAX_ROWS: usize = 150;

/// A library entry as the collection browser sees it: name, category, and how
/// many variations sit behind it.
fn column_item(index: usize, p: &KeysPreset) -> ColumnItem {
    ColumnItem {
        id: index.to_string(),
        name: p.name.clone(),
        subtitle: Some(p.kind.clone()),
        badge: (!p.variants.is_empty()).then(|| format!("{}", p.variants.len() + 1)),
        metadata: None,
        structured_tags: Default::default(),
        detail: Default::default(),
        tag: Some(index),
        folder: p.tags.first().cloned(),
    }
}

/// Which [`NavCategory`] a selection is browsing.
fn nav_for(sel: &Selection) -> NavCategory {
    match sel {
        Selection::None => NavCategory::Presets,
        Selection::Engine(_) => NavCategory::Engines,
        Selection::Layer { .. } => NavCategory::Layers,
        Selection::Module { .. } => NavCategory::Modules,
    }
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
    let sort = use_signal(|| SortMode::Name);
    // The entry whose variations are open, by library index.
    let mut inside = use_signal(|| None::<usize>);
    // Escape hatch: the tag filter helps until the moment it hides the one
    // sound you want, so it can be dropped without losing the selection.
    let all_engines = use_signal(|| false);

    let sel = selection.read().clone();
    let engine = sel.engine().map(|e| e.to_string());
    let accent = engine
        .as_deref()
        .map(engine_color)
        .unwrap_or("#94a3b8")
        .to_string();
    let loadable = !matches!(sel, Selection::None);

    // Leaving the level closes whatever variation list was open under it.
    use_effect(move || {
        let _ = selection.read();
        if inside.peek().is_some() {
            inside.set(None);
        }
    });

    rsx! {
        div {
            style: "display: flex; flex-direction: column; width: 246px; flex-shrink: 0; \
                    min-height: 0; border-right: 1px solid #1c1c1f; background: #0a0a0d;",
            match (&sel, inside()) {
                // Nothing selected: the profile, exactly like the guitar rig's
                // sidebar — the stacks are the rig's own top level.
                (Selection::None, _) => rsx! { ProfileList { state } },
                // Inside an entry: its default and its variations.
                (_, Some(i)) => rsx! {
                    VariationList {
                        state,
                        index: i,
                        accent: accent.clone(),
                        loadable,
                        on_back: move |_| inside.set(None),
                        on_load: {
                            let rig = rig.clone();
                            let sel = sel.clone();
                            move |(i, variant): (usize, Option<usize>)| {
                                load_variant(rig.clone(), sel.clone(), i, variant)
                            }
                        },
                    }
                },
                // A level of the library.
                _ => rsx! {
                    LibraryList {
                        state,
                        sel: sel.clone(),
                        accent: accent.clone(),
                        engine: engine.clone(),
                        all_engines,
                        query,
                        sort,
                        loadable,
                        on_open: move |i: usize| inside.set(Some(i)),
                        on_load: {
                            let rig = rig.clone();
                            let sel = sel.clone();
                            move |i: usize| load_into(rig.clone(), sel.clone(), i)
                        },
                    }
                },
            }
        }
    }
}

/// Load a variation of `index` (or its default, when `variant` is `None`).
fn load_variant(rig: Option<KeysRigClient>, sel: Selection, index: usize, variant: Option<usize>) {
    let Some(n) = variant else {
        load_into(rig, sel, index);
        return;
    };
    let Some(layer) = sel.layer().map(|l| l.to_string()) else {
        // Variations belong to a lane's module; an engine program has none.
        load_into(rig, sel, index);
        return;
    };
    let module = sel.module();
    spawn(async move {
        if let Some(r) = rig {
            let _ = r
                .set_layer_variant(layer, module, index as u32, n as u32)
                .await;
        }
    });
}

/// Load library entry `index` at whatever level is selected.
fn load_into(rig: Option<KeysRigClient>, sel: Selection, index: usize) {
    spawn(async move {
        let Some(r) = rig else { return };
        match &sel {
            // A whole engine program.
            Selection::Engine(_) => {
                let _ = r.load_preset(index as u32).await;
            }
            // A lane, or one module of it.
            Selection::Layer { layer, .. } | Selection::Module { layer, .. } => {
                let _ = r
                    .set_layer_patch(layer.clone(), sel.module(), index as u32)
                    .await;
            }
            Selection::None => {}
        }
    });
}

/// The profile: the rig's stacks, and which one is sounding. This is the
/// browser's resting state — with nothing selected there is no "load into",
/// so it shows the thing you actually reach for between songs.
#[component]
fn ProfileList(state: KeysViewState) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let perform = state.perform.read().clone();
    let name = if perform.profile_name.is_empty() {
        "Profile".to_string()
    } else {
        perform.profile_name.clone()
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 3px; padding: 12px; \
                    border-bottom: 1px solid #1c1c1f;",
            span {
                style: "font-size: 10px; font-weight: 700; letter-spacing: 0.1em; \
                        text-transform: uppercase; color: #a1a1aa;",
                "Profile"
            }
            span { style: "font-size: 12px; font-weight: 600; color: #e4e4e7;", "{name}" }
            span { style: "font-size: 9px; color: #52525b; line-height: 1.4;",
                "Pick an engine, a lane or a module to browse what fits it."
            }
        }
        div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 6px;",
            if perform.stacks.is_empty() {
                span { style: "display: block; padding: 10px; font-size: 10px; color: #52525b;",
                    "This profile has no stacks yet."
                }
            }
            for (i, stack) in perform.stacks.iter().enumerate() {
                {
                    let rig = rig.clone();
                    let (bg, fg) = crate::perform::stack_color(&stack.name);
                    let active = stack.is_active;
                    rsx! {
                        button {
                            key: "{stack.name}",
                            style: format!(
                                "width: 100%; appearance: none; text-align: left; cursor: pointer; \
                                 border: 1px solid {}; border-radius: 8px; padding: 8px 9px; \
                                 margin-bottom: 4px; display: flex; flex-direction: column; gap: 2px; \
                                 background: {}; color: {};",
                                if active { fg } else { "transparent" },
                                if active { bg } else { "#0d0d10" },
                                if active { fg } else { "#a1a1aa" },
                            ),
                            onclick: move |_| {
                                let rig = rig.clone();
                                spawn(async move {
                                    if let Some(r) = rig { let _ = r.press_stack(i as u32).await; }
                                });
                            },
                            span { style: "font-size: 11px; font-weight: 700;", "{stack.name}" }
                            if !stack.blurb.is_empty() {
                                span { style: "font-size: 9px; color: #52525b;", "{stack.blurb}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One level of the library: the defaults that fit the selection.
#[component]
fn LibraryList(
    state: KeysViewState,
    sel: Selection,
    accent: String,
    engine: Option<String>,
    all_engines: Signal<bool>,
    query: Signal<String>,
    sort: Signal<SortMode>,
    loadable: bool,
    on_open: EventHandler<usize>,
    on_load: EventHandler<usize>,
) -> Element {
    let mut all_engines = all_engines;
    let mut query = query;
    let mut sort = sort;

    let library = state.presets.read();
    let q = query().to_lowercase();
    let mut hits: Vec<(usize, ColumnItem, usize)> = library
        .iter()
        .enumerate()
        .filter(|(_, p)| sel.accepts(&p.scope))
        .filter(|(_, p)| match (&engine, all_engines()) {
            (Some(e), false) => p.tags.iter().any(|t| t == e),
            _ => true,
        })
        .filter(|(_, p)| {
            q.is_empty() || p.name.to_lowercase().contains(&q) || p.kind.to_lowercase().contains(&q)
        })
        .take(MAX_ROWS * 4)
        .map(|(i, p)| (i, column_item(i, p), p.variants.len()))
        .collect();
    match sort() {
        SortMode::Name => hits.sort_by(|a, b| a.1.name.cmp(&b.1.name)),
        SortMode::NameDesc => hits.sort_by(|a, b| b.1.name.cmp(&a.1.name)),
        SortMode::Variants => hits.sort_by(|a, b| b.2.cmp(&a.2).then(a.1.name.cmp(&b.1.name))),
        SortMode::BlockType => hits.sort_by(|a, b| {
            a.1.subtitle
                .cmp(&b.1.subtitle)
                .then(a.1.name.cmp(&b.1.name))
        }),
    }
    let truncated = hits.len() > MAX_ROWS;
    hits.truncate(MAX_ROWS);

    // Where a click lands, spelled out — the browser is the one surface that
    // loads sounds, so it says what it is about to do.
    let target = match &sel {
        Selection::None => "select a lane to load into".to_string(),
        Selection::Engine(e) => format!("→ {e}"),
        Selection::Layer { layer, .. } => format!("→ {layer}"),
        Selection::Module { layer, module, .. } => {
            format!("→ {layer} · module {}", (b'A' + *module as u8) as char)
        }
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 8px; padding: 12px; \
                    border-bottom: 1px solid #1c1c1f;",
            div { style: "display: flex; align-items: center; gap: 8px;",
                span {
                    style: "font-size: 10px; font-weight: 700; letter-spacing: 0.1em; \
                            text-transform: uppercase; color: #a1a1aa;",
                    {nav_for(&sel).label()}
                }
                div { style: "flex: 1;" }
                span { style: "font-size: 9px; color: #52525b;",
                    if truncated { "{MAX_ROWS}+" } else { "{hits.len()}" }
                }
            }
            span {
                style: format!(
                    "font-size: 10px; color: {}; overflow: hidden; text-overflow: ellipsis; \
                     white-space: nowrap;",
                    if loadable { accent.clone() } else { "#52525b".to_string() },
                ),
                "{target}"
            }
            input {
                style: "background: #131316; border: 1px solid #1f1f23; border-radius: 8px; \
                        padding: 7px 9px; color: #e4e4e7; font-size: 11px;",
                placeholder: "search",
                value: "{query}",
                oninput: move |e| query.set(e.value()),
            }
            div { style: "display: flex; align-items: center; gap: 6px; flex-wrap: wrap;",
                if let Some(e) = engine.clone() {
                    button {
                        style: format!(
                            "appearance: none; border: 1px solid {}; border-radius: 999px; \
                             padding: 2px 9px; cursor: pointer; font-size: 9px; font-weight: 700; \
                             background: {}; color: {};",
                            if all_engines() { "#26262b" } else { "#1f2b3a" },
                            if all_engines() { "#131316" } else { "#101821" },
                            if all_engines() { "#52525b".to_string() } else { accent.clone() },
                        ),
                        title: "Filter to this engine's sounds",
                        onclick: move |_| all_engines.toggle(),
                        if all_engines() { "all engines" } else { "{e}" }
                    }
                }
                button {
                    style: "appearance: none; border: 1px solid #26262b; border-radius: 999px; \
                            padding: 2px 9px; cursor: pointer; font-size: 9px; font-weight: 700; \
                            background: #131316; color: #71717a;",
                    title: "Sort",
                    onclick: move |_| {
                        let next = match sort() {
                            SortMode::Name => SortMode::NameDesc,
                            SortMode::NameDesc => SortMode::Variants,
                            SortMode::Variants => SortMode::BlockType,
                            SortMode::BlockType => SortMode::Name,
                        };
                        sort.set(next);
                    },
                    {sort().label()}
                }
            }
        }
        div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 6px;",
            if hits.is_empty() {
                span { style: "display: block; padding: 10px; font-size: 10px; color: #52525b; line-height: 1.5;",
                    if library.is_empty() {
                        "The library is empty — build a pack, or point the rig at one."
                    } else {
                        "Nothing here for this engine. Try 'all engines'."
                    }
                }
            }
            for (i, item, variants) in hits {
                div {
                    key: "{i}",
                    style: "display: flex; align-items: stretch; gap: 2px; margin-bottom: 2px;",
                    button {
                        style: format!(
                            "flex: 1; min-width: 0; appearance: none; text-align: left; border: none; \
                             border-radius: 7px; padding: 7px 9px; cursor: pointer; \
                             display: flex; flex-direction: column; gap: 2px; \
                             background: transparent; color: {};",
                            if loadable { "#e4e4e7" } else { "#71717a" },
                        ),
                        disabled: !loadable,
                        onclick: move |_| on_load.call(i),
                        span {
                            style: "font-size: 11px; font-weight: 600; line-height: 1.25; \
                                    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                            "{item.name}"
                        }
                        span { style: "font-size: 9px; color: #52525b;",
                            {item.subtitle.clone().unwrap_or_default()}
                        }
                    }
                    // Variations live behind the entry, not beside it.
                    if variants > 0 {
                        button {
                            style: format!(
                                "appearance: none; border: none; border-radius: 7px; cursor: pointer; \
                                 width: 30px; background: #101821; color: {accent}; font-size: 9px; \
                                 font-weight: 700;",
                            ),
                            title: "{variants} variations",
                            onclick: move |_| on_open.call(i),
                            "{variants}›"
                        }
                    }
                }
            }
            if truncated {
                span { style: "display: block; padding: 10px; font-size: 10px; color: #52525b; line-height: 1.5;",
                    "First {MAX_ROWS} — search to narrow it."
                }
            }
        }
    }
}

/// Inside one entry: its default, then the variations authored on it.
#[component]
fn VariationList(
    state: KeysViewState,
    index: usize,
    accent: String,
    loadable: bool,
    on_back: EventHandler<()>,
    on_load: EventHandler<(usize, Option<usize>)>,
) -> Element {
    let library = state.presets.read();
    let Some(preset) = library.get(index).cloned() else {
        return rsx! {};
    };
    // Which variation the selected module is holding, so the list says where
    // you are as well as where you can go.
    let selection = use_selection();
    let here = match &*selection.read() {
        Selection::Layer { layer, .. } | Selection::Module { layer, .. } => state
            .mixer
            .read()
            .engines
            .iter()
            .flat_map(|e| e.layers.iter())
            .find(|l| l.name == *layer)
            .and_then(|l| l.modules.get(selection.read().module() as usize))
            .map(|m| m.variant.clone())
            .unwrap_or_default(),
        _ => String::new(),
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 6px; padding: 12px; \
                    border-bottom: 1px solid #1c1c1f;",
            button {
                style: "appearance: none; align-self: flex-start; background: transparent; \
                        border: none; cursor: pointer; padding: 0; color: #71717a; \
                        font-size: 10px; font-weight: 600;",
                onclick: move |_| on_back.call(()),
                "‹ back"
            }
            span { style: "font-size: 12px; font-weight: 700; color: #e4e4e7;", "{preset.name}" }
            span { style: "font-size: 9px; color: #52525b;", "{preset.kind}" }
        }
        div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 6px;",
            button {
                style: format!(
                    "width: 100%; appearance: none; text-align: left; border: none; cursor: pointer; \
                     border-radius: 7px; padding: 7px 9px; margin-bottom: 2px; \
                     background: {}; color: {}; font-size: 11px; font-weight: 600;",
                    if here.is_empty() { "#101821" } else { "transparent" },
                    if here.is_empty() { accent.clone() } else { "#a1a1aa".to_string() },
                ),
                disabled: !loadable,
                onclick: move |_| on_load.call((index, None)),
                "Default"
            }
            for (n, variant) in preset.variants.iter().enumerate() {
                button {
                    key: "{n}",
                    style: format!(
                        "width: 100%; appearance: none; text-align: left; border: none; \
                         cursor: pointer; border-radius: 7px; padding: 7px 9px; \
                         margin-bottom: 2px; font-size: 11px; background: {}; color: {};",
                        if *variant == here { "#101821" } else { "transparent" },
                        if *variant == here { accent.clone() } else { "#e4e4e7".to_string() },
                    ),
                    disabled: !loadable,
                    onclick: move |_| on_load.call((index, Some(n))),
                    "{variant}"
                }
            }
        }
    }
}
