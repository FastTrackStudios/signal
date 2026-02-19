//! Multi-column cascading browser for the signal domain.
//!
//! Column 1 is a fixed navigation sidebar (Presets / Engines / Modules / Blocks).
//! Everything is scoped to the active **rig type** (status bar selector).
//!
//! | Nav      | Col 2 (auto)              | Col 3 (click)     | Col 4 (click)  |
//! |----------|---------------------------|-------------------|----------------|
//! | Presets  | Presets for rig type      | Scenes            | —              |
//! | Engines  | Engines for rig type      | Layers for engine | —              |
//! | Modules  | Module types (color dots) | Presets for type  | —              |
//! | Blocks   | Block types (color dots)  | Presets for type  | Snapshots      |

mod data_fetching;
mod detail_panel;
mod grid_conversion;
mod inspector;
mod toolbar;
mod types;

use dioxus::prelude::*;
use signal::rig::RigType;
use signal::tagging::TagSet;
use signal::{BlockType, ALL_BLOCK_TYPES, ALL_MODULE_TYPES};

use data_fetching::{
    build_param_lookup, fetch_col2, fetch_col3, resolve_layer_detail, resolve_scene_detail,
};
use detail_panel::{
    collect_available_tags, filter_and_sort, find_detail, rig_type_display, DetailPanel,
};
use grid_conversion::ParamLookup;
use toolbar::Toolbar;
use types::{ColumnItem, DetailData, DetailParam, NavCategory, SortMode, RIG_TYPES};

// Re-export public API types used by grid_conversion (needed by other views).
pub use data_fetching::rig_type_to_engine_type;
pub use grid_conversion::ParamLookup as EngineParamLookup;
pub use grid_conversion::{engines_to_grid_slots, RigGridPanel};
pub use types::{EngineFlowData, LayerFlowData, ModuleChainData};

/// Public API: resolve a rig scene into engine flow data and parameter lookup
/// for rendering in `RigGridPanel`.
///
/// Loads the rig, finds the matching scene, resolves engines + params.
/// Returns `None` if the rig or scene is not found.
pub async fn resolve_scene_engines(
    signal: &signal::Signal,
    rig_id: &str,
    scene_id: &str,
) -> Option<(Vec<EngineFlowData>, ParamLookup)> {
    resolve_scene_detail(signal, rig_id, scene_id).await
}

/// Public API: resolve a layer variant into engine flow data and parameter lookup
/// for rendering in `RigGridPanel`.
///
/// Loads the layer, resolves module chains for the given variant (or default),
/// wraps them in a synthetic `EngineFlowData`.
/// Returns `None` if the layer or variant is not found.
pub async fn resolve_layer_engines(
    signal: &signal::Signal,
    layer_id: &str,
    variant_id: Option<&str>,
) -> Option<(Vec<EngineFlowData>, ParamLookup)> {
    resolve_layer_detail(signal, layer_id, variant_id).await
}

// region: --- Public API

/// Which domain level to browse. Kept for external API compatibility.
#[derive(Debug, Clone, PartialEq)]
pub enum BrowseLevel {
    Presets,
    Engines,
    Modules,
    Blocks(BlockType),
}

impl BrowseLevel {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Presets => "Presets",
            Self::Engines => "Engines",
            Self::Modules => "Modules",
            Self::Blocks(_) => "Block Presets",
        }
    }
}

// endregion

// region: --- CollectionBrowser

#[component]
pub fn CollectionBrowser() -> Element {
    let signal = crate::use_signal_service();
    let mut nav = use_signal(|| NavCategory::Presets);
    let mut rig_type = use_signal(|| RigType::Guitar);

    let mut col2_items = use_signal(Vec::<ColumnItem>::new);
    let mut col2_selected = use_signal(|| None::<usize>);
    let mut col3_items = use_signal(Vec::<ColumnItem>::new);
    let mut col3_selected = use_signal(|| None::<usize>);
    let mut col4_items = use_signal(Vec::<ColumnItem>::new);
    let mut col4_selected = use_signal(|| None::<usize>);

    // Track the selected col2 item's ID for lazy scene resolution.
    let mut col2_current_id = use_signal(String::new);

    // Cache of raw Preset objects from the last Blocks col3 fetch.
    // Used by col4 to look up snapshots without re-querying the DB.
    let mut block_presets_cache = use_signal(Vec::<signal::Preset>::new);

    // Pre-resolved block parameters for the detail grid inspector.
    let mut param_lookup = use_signal(ParamLookup::new);

    // Search / sort / filter state
    let mut search_text = use_signal(String::new);
    let mut sort_mode = use_signal(|| SortMode::Name);
    let mut active_tag_filters = use_signal(Vec::<String>::new);
    let mut show_tag_panel = use_signal(|| false);

    let nav_memo = use_memo(move || nav());

    // Auto-fetch col2 when nav or rig_type changes.
    {
        let signal = signal.clone();
        use_effect(move || {
            let signal = signal.clone();
            let category = nav_memo();
            let rt = rig_type();
            col2_selected.set(None);
            col3_items.set(Vec::new());
            col3_selected.set(None);
            col4_items.set(Vec::new());
            col4_selected.set(None);
            block_presets_cache.set(Vec::new());
            param_lookup.set(ParamLookup::new());
            search_text.set(String::new());
            active_tag_filters.set(Vec::new());
            spawn(async move {
                let items = fetch_col2(&signal, category, rt).await;
                // Auto-select the first item so detail panel is populated on load.
                if !items.is_empty() && category == NavCategory::Presets {
                    let first_id = items[0].id.clone();
                    let first_tag = items[0].tag;
                    col2_selected.set(Some(0));
                    col2_current_id.set(first_id.clone());
                    let (v, presets) = fetch_col3(&signal, category, &first_id, first_tag).await;
                    // Auto-select first scene too
                    if !v.is_empty() {
                        col3_selected.set(Some(0));
                    }
                    // Resolve block parameters for the detail grid
                    let params = build_param_lookup(&signal, &v).await;
                    param_lookup.set(params);
                    col3_items.set(v);
                    block_presets_cache.set(presets);
                }
                col2_items.set(items);
            });
        });
    }

    let current_nav = nav_memo();
    let current_rt = rig_type();

    // Track which column has keyboard focus (2, 3, or 4).
    let mut focus_col = use_signal(|| 2u8);

    // Apply search + tag filter + sort to col2 items.
    let all_col2 = filter_and_sort(
        &col2_items(),
        &search_text(),
        &active_tag_filters(),
        sort_mode(),
    );
    let all_col3 = col3_items();
    let all_col4 = col4_items();

    let has_col4 = current_nav == NavCategory::Blocks;

    // Detail panel: deepest selection
    let (detail_name, detail_meta, detail_data) = find_detail(
        &all_col4,
        col4_selected(),
        &all_col3,
        col3_selected(),
        &all_col2,
        col2_selected(),
    );

    let col2_header = match current_nav {
        NavCategory::Presets => "Presets",
        NavCategory::Engines => "Engines",
        NavCategory::Layers => "Layers",
        NavCategory::Modules => "Module Types",
        NavCategory::Blocks => "Block Types",
    };
    let col3_header = match current_nav {
        NavCategory::Presets => "Scenes",
        NavCategory::Engines => "Layers",
        NavCategory::Layers => "Variants",
        NavCategory::Modules => "Presets",
        NavCategory::Blocks => "Presets",
    };

    let accent = current_nav.accent();
    let show_type_dots = current_nav == NavCategory::Blocks || current_nav == NavCategory::Modules;

    // Compute available tags from the unfiltered col2 items for the tag panel.
    let available_tags = collect_available_tags(&col2_items());
    let current_sort = sort_mode();
    let current_search = search_text();
    let tag_panel_open = show_tag_panel();
    let current_filters = active_tag_filters();
    let has_active_filters = !current_search.is_empty() || !current_filters.is_empty();

    rsx! {
        div {
            class: "h-full w-full flex flex-col overflow-hidden outline-none",
            tabindex: "0",
            onkeydown: move |evt: KeyboardEvent| {
                let key = evt.key();
                match key {
                    Key::ArrowUp => {
                        evt.prevent_default();
                        match focus_col() {
                            2 => {
                                let idx = col2_selected().unwrap_or(0);
                                if idx > 0 {
                                    col2_selected.set(Some(idx - 1));
                                }
                            }
                            3 => {
                                let idx = col3_selected().unwrap_or(0);
                                if idx > 0 {
                                    col3_selected.set(Some(idx - 1));
                                }
                            }
                            4 => {
                                let idx = col4_selected().unwrap_or(0);
                                if idx > 0 {
                                    col4_selected.set(Some(idx - 1));
                                }
                            }
                            _ => {}
                        }
                    }
                    Key::ArrowDown => {
                        evt.prevent_default();
                        match focus_col() {
                            2 => {
                                let len = col2_items().len();
                                let idx = col2_selected().map(|i| i + 1).unwrap_or(0);
                                if idx < len {
                                    col2_selected.set(Some(idx));
                                }
                            }
                            3 => {
                                let len = col3_items().len();
                                let idx = col3_selected().map(|i| i + 1).unwrap_or(0);
                                if idx < len {
                                    col3_selected.set(Some(idx));
                                }
                            }
                            4 => {
                                let len = col4_items().len();
                                let idx = col4_selected().map(|i| i + 1).unwrap_or(0);
                                if idx < len {
                                    col4_selected.set(Some(idx));
                                }
                            }
                            _ => {}
                        }
                    }
                    Key::ArrowRight => {
                        evt.prevent_default();
                        let fc = focus_col();
                        if fc < 4 {
                            focus_col.set(fc + 1);
                        }
                    }
                    Key::ArrowLeft => {
                        evt.prevent_default();
                        let fc = focus_col();
                        if fc > 2 {
                            focus_col.set(fc - 1);
                        }
                    }
                    Key::Enter => {
                        // Trigger click on selected item in focused column
                        match focus_col() {
                            2 => {
                                if let Some(idx) = col2_selected() {
                                    let items = col2_items();
                                    if let Some(item) = items.get(idx) {
                                        let signal = signal.clone();
                                        let nav_val = nav();
                                        let id = item.id.clone();
                                        let tag = item.tag;
                                        col2_current_id.set(id.clone());
                                        col3_selected.set(None);
                                        col4_items.set(Vec::new());
                                        col4_selected.set(None);
                                        block_presets_cache.set(Vec::new());
                                        spawn(async move {
                                            let (v, presets) = fetch_col3(&signal, nav_val, &id, tag).await;
                                            let params = build_param_lookup(&signal, &v).await;
                                            param_lookup.set(params);
                                            col3_items.set(v);
                                            block_presets_cache.set(presets);
                                        });
                                        focus_col.set(3);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Key::Escape => {
                        // Go back: clear selection in focused column
                        match focus_col() {
                            4 => {
                                col4_selected.set(None);
                                focus_col.set(3);
                            }
                            3 => {
                                col3_selected.set(None);
                                col3_items.set(Vec::new());
                                focus_col.set(2);
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            },

            div { class: "h-[2px] w-full bg-gradient-to-r {accent} flex-shrink-0" }

            // ── Toolbar: search + sort + filter ──
            Toolbar {
                current_nav: current_nav,
                current_search: current_search.clone(),
                current_sort: current_sort,
                tag_panel_open: tag_panel_open,
                active_filters: current_filters.clone(),
                available_tags: available_tags,
                on_search_change: move |text: String| {
                    search_text.set(text);
                },
                on_sort_change: move |mode: SortMode| {
                    sort_mode.set(mode);
                },
                on_toggle_tag_panel: move |_| {
                    show_tag_panel.set(!tag_panel_open);
                },
                on_filters_change: move |filters: Vec<String>| {
                    active_tag_filters.set(filters);
                },
            }

            div { class: "flex-1 flex min-h-0 overflow-hidden",

                // ── Col 1: Nav ──
                div { class: "w-36 flex-shrink-0 border-r border-zinc-800 flex flex-col min-h-0 bg-zinc-950/60",
                    div { class: "px-3 py-2 border-b border-zinc-800",
                        h3 { class: "text-[10px] font-semibold text-zinc-500 uppercase tracking-wider", "Browse" }
                    }
                    div { class: "flex-1 overflow-y-auto py-1",
                        for cat in NavCategory::ALL.iter() {
                            {
                                let c = *cat;
                                let is_active = current_nav == c;
                                rsx! {
                                    button {
                                        key: "{c.label()}",
                                        class: if is_active {
                                            "w-full text-left px-3 py-2 text-sm font-medium bg-zinc-700/70 text-zinc-100"
                                        } else {
                                            "w-full text-left px-3 py-2 text-sm text-zinc-400 hover:bg-zinc-800/60 hover:text-zinc-200"
                                        },
                                        onclick: move |_| nav.set(c),
                                        "{c.label()}"
                                    }
                                }
                            }
                        }
                    }
                }

                // ── Col 2: Items (auto-fetched) ──
                div { class: "w-64 flex-shrink-0 border-r border-zinc-800 flex flex-col min-h-0 bg-zinc-950/50",
                    div { class: "px-3 py-2 border-b border-zinc-800",
                        h3 { class: "text-[10px] font-semibold text-zinc-500 uppercase tracking-wider", "{col2_header}" }
                    }
                    div { class: "flex-1 overflow-y-auto",
                        if all_col2.is_empty() {
                            div { class: "text-xs text-zinc-600 text-center py-6", "No items" }
                        }
                        for (idx, item) in all_col2.iter().enumerate() {
                            {
                                let is_sel = col2_selected() == Some(idx);
                                let name = item.name.clone();
                                let subtitle = item.subtitle.clone();
                                let badge = item.badge.clone();
                                let color_bg = if show_type_dots {
                                    match current_nav {
                                        NavCategory::Blocks => item.tag.and_then(|t| ALL_BLOCK_TYPES.get(t)).map(|bt| bt.color().bg.to_string()),
                                        NavCategory::Modules => item.tag.and_then(|t| ALL_MODULE_TYPES.get(t)).map(|mt| mt.color().bg.to_string()),
                                        _ => None,
                                    }
                                } else {
                                    None
                                };
                                let signal = signal.clone();
                                let item_id = item.id.clone();
                                let item_tag = item.tag;
                                rsx! {
                                    button {
                                        key: "{item_id}",
                                        class: if is_sel {
                                            "w-full text-left px-3 py-2 border-b border-zinc-800/50 bg-zinc-700/60"
                                        } else {
                                            "w-full text-left px-3 py-2 border-b border-zinc-800/50 hover:bg-zinc-800/60"
                                        },
                                        onclick: move |_| {
                                            col2_selected.set(Some(idx));
                                            col2_current_id.set(item_id.clone());
                                            col3_selected.set(None);
                                            col4_items.set(Vec::new());
                                            col4_selected.set(None);
                                            block_presets_cache.set(Vec::new());
                                            let signal = signal.clone();
                                            let nav = nav();
                                            let id = item_id.clone();
                                            let tag = item_tag;
                                            spawn(async move {
                                                let (v, presets) = fetch_col3(&signal, nav, &id, tag).await;
                                                let params = build_param_lookup(&signal, &v).await;
                                                param_lookup.set(params);
                                                col3_items.set(v);
                                                block_presets_cache.set(presets);
                                            });
                                        },
                                        div { class: "flex items-center gap-1.5",
                                            if let Some(ref bg) = color_bg {
                                                span {
                                                    class: "w-2 h-2 rounded-full flex-shrink-0",
                                                    style: "background-color: {bg}",
                                                }
                                            }
                                            span { class: "text-sm text-zinc-200 truncate flex-1", "{name}" }
                                            if let Some(ref b) = badge {
                                                span { class: "text-[10px] text-zinc-500 flex-shrink-0", "{b}" }
                                            }
                                        }
                                        if let Some(ref sub) = subtitle {
                                            div { class: "text-xs text-zinc-500 truncate", "{sub}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "px-3 py-1 border-t border-zinc-800 flex-shrink-0",
                        if has_active_filters {
                            span { class: "text-[10px] text-zinc-600",
                                "{all_col2.len()} / {col2_items().len()}"
                            }
                        } else {
                            span { class: "text-[10px] text-zinc-600", "{all_col2.len()}" }
                        }
                    }
                }

                // ── Col 3: Children (on col2 click) ──
                div { class: "w-64 flex-shrink-0 border-r border-zinc-800 flex flex-col min-h-0 bg-zinc-950/40",
                    div { class: "px-3 py-2 border-b border-zinc-800",
                        h3 { class: "text-[10px] font-semibold text-zinc-500 uppercase tracking-wider",
                            {if col2_selected().is_some() { col3_header } else { "—" }}
                        }
                    }
                    div { class: "flex-1 overflow-y-auto",
                        if all_col3.is_empty() {
                            div { class: "text-xs text-zinc-600 text-center py-6",
                                {if col2_selected().is_some() { "No items" } else { "Select from left" }}
                            }
                        }
                        for (cidx, child) in all_col3.iter().enumerate() {
                            {
                                let is_sel = col3_selected() == Some(cidx);
                                let name = child.name.clone();
                                let subtitle = child.subtitle.clone();
                                let badge = child.badge.clone();
                                let child_id = child.id.clone();
                                let child_engines_empty = child.detail.engines.is_empty();
                                let signal = signal.clone();
                                rsx! {
                                    button {
                                        key: "{child_id}",
                                        class: if is_sel {
                                            "w-full text-left px-3 py-2 border-b border-zinc-800/50 bg-zinc-700/60"
                                        } else {
                                            "w-full text-left px-3 py-2 border-b border-zinc-800/50 hover:bg-zinc-800/60"
                                        },
                                        onclick: move |_| {
                                            col3_selected.set(Some(cidx));
                                            col4_selected.set(None);
                                            // Lazy scene resolution: if this is a Presets scene
                                            // that wasn't resolved eagerly, resolve it now.
                                            if current_nav == NavCategory::Presets && child_engines_empty {
                                                let signal = signal.clone();
                                                let rig_id = col2_current_id().clone();
                                                let scene_id = child_id.clone();
                                                spawn(async move {
                                                    if let Some((engines, params)) =
                                                        resolve_scene_detail(&signal, &rig_id, &scene_id).await
                                                    {
                                                        let mut items = col3_items();
                                                        if let Some(item) = items.get_mut(cidx) {
                                                            item.detail.engines = engines;
                                                        }
                                                        param_lookup.set(params);
                                                        col3_items.set(items);
                                                    }
                                                });
                                            }
                                            if has_col4 {
                                                let items = col3_items();
                                                if let Some(item) = items.get(cidx) {
                                                    let item_id = &item.id;
                                                    // Look up snapshots directly from the cached presets
                                                    let cached = block_presets_cache();
                                                    let snap_items = cached.iter()
                                                        .find(|p| p.id().to_string() == *item_id)
                                                        .map(|preset| {
                                                            preset.snapshots().iter().map(|s| ColumnItem {
                                                                id: s.id().to_string(),
                                                                name: s.name().to_string(),
                                                                subtitle: Some(format!("{} param(s)", s.block().parameters().len())),
                                                                badge: None,
                                                                metadata: None,
                                                                structured_tags: TagSet::default(),
                                                                detail: DetailData {
                                                                    params: s.block().parameters().iter().map(|p| DetailParam {
                                                                        name: p.name().to_string(),
                                                                        value: p.value().get(),
                                                                    }).collect(),
                                                                    ..Default::default()
                                                                },
                                                                tag: None,
                                                            }).collect::<Vec<_>>()
                                                        })
                                                        .unwrap_or_default();
                                                    col4_items.set(snap_items);
                                                }
                                            }
                                        },
                                        div { class: "flex items-center gap-1.5",
                                            span { class: "text-sm text-zinc-200 truncate flex-1", "{name}" }
                                            if let Some(ref b) = badge {
                                                span { class: "text-[10px] text-zinc-500 flex-shrink-0", "{b}" }
                                            }
                                        }
                                        if let Some(ref sub) = subtitle {
                                            div { class: "text-xs text-zinc-500 truncate", "{sub}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "px-3 py-1 border-t border-zinc-800 flex-shrink-0",
                        span { class: "text-[10px] text-zinc-600", "{all_col3.len()}" }
                    }
                }

                // ── Col 4: Snapshots (only for Blocks) ──
                if has_col4 {
                    div { class: "w-64 flex-shrink-0 border-r border-zinc-800 flex flex-col min-h-0 bg-zinc-950/30",
                        div { class: "px-3 py-2 border-b border-zinc-800",
                            h3 { class: "text-[10px] font-semibold text-zinc-500 uppercase tracking-wider",
                                {if col3_selected().is_some() { "Snapshots" } else { "—" }}
                            }
                        }
                        div { class: "flex-1 overflow-y-auto",
                            if all_col4.is_empty() {
                                div { class: "text-xs text-zinc-600 text-center py-6",
                                    {if col3_selected().is_some() { "No items" } else { "Select from left" }}
                                }
                            }
                            for (didx, item) in all_col4.iter().enumerate() {
                                {
                                    let is_sel = col4_selected() == Some(didx);
                                    let name = item.name.clone();
                                    let subtitle = item.subtitle.clone();
                                    rsx! {
                                        button {
                                            key: "{item.id}",
                                            class: if is_sel {
                                                "w-full text-left px-3 py-2 border-b border-zinc-800/50 bg-zinc-700/60"
                                            } else {
                                                "w-full text-left px-3 py-2 border-b border-zinc-800/50 hover:bg-zinc-800/60"
                                            },
                                            onclick: move |_| {
                                                col4_selected.set(Some(didx));
                                            },
                                            span { class: "text-sm text-zinc-200 truncate", "{name}" }
                                            if let Some(ref sub) = subtitle {
                                                div { class: "text-xs text-zinc-500 truncate", "{sub}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "px-3 py-1 border-t border-zinc-800 flex-shrink-0",
                            span { class: "text-[10px] text-zinc-600", "{all_col4.len()}" }
                        }
                    }
                }

                // ── Detail ──
                DetailPanel {
                    detail_name: detail_name.clone(),
                    detail_meta: detail_meta.cloned(),
                    detail_data: detail_data.cloned(),
                    param_lookup: param_lookup(),
                }
            }

            // Status bar
            div { class: "px-4 py-1.5 border-t border-zinc-800 flex items-center gap-3 flex-shrink-0 bg-zinc-950/60",
                div { class: "w-1.5 h-1.5 rounded-full bg-green-500" }
                span { class: "text-[10px] text-zinc-500", "{current_nav.label()}" }
                div { class: "flex-1" }
                span { class: "text-[10px] text-zinc-600 mr-1", "Rig:" }
                for rt in RIG_TYPES.iter() {
                    {
                        let t = *rt;
                        let is_active = current_rt == t;
                        rsx! {
                            button {
                                key: "{t.as_str()}",
                                class: if is_active {
                                    "px-1.5 py-0.5 text-[10px] rounded bg-zinc-600 text-zinc-100"
                                } else {
                                    "px-1.5 py-0.5 text-[10px] rounded text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800"
                                },
                                onclick: move |_| rig_type.set(t),
                                "{rig_type_display(t)}"
                            }
                        }
                    }
                }
            }
        }
    }
}

// endregion: --- CollectionBrowser
