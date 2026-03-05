//! Searchable dropdown for picking a block or module type to place on the grid.
//!
//! Uses `position: fixed` with the click coordinates so it escapes
//! the CSS `transform: scale()` stacking context.

use dioxus::prelude::*;
use fts_ui::prelude::*;
use signal::block::BlockCategory;
use signal::defaults::{
    archetype_label, archetype_seed_slug, archetype_x_templates, NDSP_ARCHETYPE_X_PLUGIN_NAMES,
};
use signal::plugin_block::PluginBlockDef;
use signal::{BlockType, ModuleType, ALL_BLOCK_TYPES};
use uuid::Uuid;

use super::layout::module_type_color;
use super::types::GridSlot;

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub struct BlockPickerDropdownProps {
    pub col: usize,
    pub row: usize,
    pub click_x: f64,
    pub click_y: f64,
    pub on_add_slot: EventHandler<GridSlot>,
    #[props(default)]
    pub on_add_slots: Option<EventHandler<Vec<GridSlot>>>,
    pub on_close: EventHandler<()>,
}

// ---------------------------------------------------------------------------
// Picker subtab
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerTab {
    All,
    Blocks,
    Modules,
    Plugins,
}

/// Module type definitions for the picker (guitar signal chain order).
fn picker_module_types() -> Vec<(ModuleType, &'static str, &'static str)> {
    vec![
        (ModuleType::Eq, "EQ", "Tone shaping"),
        (ModuleType::Dynamics, "Dynamics", "Comp, gate, limiter"),
        (ModuleType::Drive, "Drive", "Boost, OD, distortion"),
        (ModuleType::Amp, "Amp", "Amp + cabinet + room"),
        (ModuleType::PostEq, "Post EQ", "Post-amp shaping"),
        (
            ModuleType::Modulation,
            "Modulation",
            "Chorus, flanger, phaser",
        ),
        (ModuleType::Time, "Time", "Delay, reverb, freeze"),
        (ModuleType::Motion, "Motion", "Tremolo, vibrato, rotary"),
        (ModuleType::Special, "Special", "Wah, pitch, filter"),
        (ModuleType::Master, "Master", "Final output stage"),
    ]
}

// ---------------------------------------------------------------------------
// Plugin → GridSlot conversion
// ---------------------------------------------------------------------------

/// Convert a `PluginBlockDef` into `Vec<GridSlot>` starting at `(col, row)`.
///
/// Modules flow left-to-right (each module advances `current_col`).
/// Blocks within a module stack vertically (incrementing row).
fn plugin_def_to_grid_slots(def: &PluginBlockDef, col: usize, row: usize) -> Vec<GridSlot> {
    let slug = archetype_seed_slug(&def.plugin_name);
    let mut slots = Vec::new();
    let mut current_col = col;

    for vm in &def.modules {
        let module_group = format!("ndsp:{}/{}", slug, vm.id);
        let module_type = Some(vm.module_type);

        for vb in &vm.blocks {
            let parameters: Vec<(String, f32)> = vb
                .params
                .iter()
                .map(|p| (p.name.clone(), p.default_value))
                .collect();

            slots.push(GridSlot {
                id: Uuid::new_v4(),
                block_type: vb.block_type,
                block_preset_name: Some(vb.label.clone()),
                plugin_name: Some(def.plugin_name.clone()),
                col: current_col,
                row,
                module_group: Some(module_group.clone()),
                module_type,
                layer_group: None,
                engine_group: None,
                is_template: true,
                bypassed: false,
                is_phantom: false,
                parameters,
                preset_id: None,
                snapshot_id: None,
            });
            current_col += 1;
        }
    }

    slots
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

#[component]
pub fn BlockPickerDropdown(props: BlockPickerDropdownProps) -> Element {
    let mut search = use_signal(String::new);
    let mut active_tab = use_signal(|| PickerTab::All);
    let col = props.col;
    let row = props.row;
    let tab = active_tab();

    // Cached archetype templates — parsed once per picker lifetime.
    let templates = use_signal(archetype_x_templates);

    // Tracks which archetype index is expanded in the Plugins tab.
    let mut expanded_plugin = use_signal(|| None::<usize>);

    let search_lower = search().to_lowercase();

    // Filter block types by search text
    let filtered_blocks: Vec<BlockType> = ALL_BLOCK_TYPES
        .iter()
        .copied()
        .filter(|bt| {
            if search_lower.is_empty() {
                return true;
            }
            bt.display_name().to_lowercase().contains(&search_lower)
                || bt
                    .category()
                    .display_name()
                    .to_lowercase()
                    .contains(&search_lower)
                || bt.as_str().contains(&search_lower)
        })
        .collect();

    // Filter module types by search text
    let module_types = picker_module_types();
    let filtered_modules: Vec<&(ModuleType, &str, &str)> = module_types
        .iter()
        .filter(|(_, name, desc)| {
            if search_lower.is_empty() {
                return true;
            }
            name.to_lowercase().contains(&search_lower)
                || desc.to_lowercase().contains(&search_lower)
                || "module".contains(&search_lower)
        })
        .collect();

    // Filter plugins by search text
    let filtered_plugins: Vec<(usize, &str)> = NDSP_ARCHETYPE_X_PLUGIN_NAMES
        .iter()
        .enumerate()
        .filter(|(_, name)| {
            if search_lower.is_empty() {
                return true;
            }
            archetype_label(name).to_lowercase().contains(&search_lower)
                || "plugin".contains(&search_lower)
                || "neural".contains(&search_lower)
                || "ndsp".contains(&search_lower)
        })
        .map(|(i, name)| (i, *name))
        .collect();

    let show_blocks = tab == PickerTab::All || tab == PickerTab::Blocks;
    let show_modules = tab == PickerTab::All || tab == PickerTab::Modules;
    let show_plugins = tab == PickerTab::Plugins;
    let has_results = (show_blocks && !filtered_blocks.is_empty())
        || (show_modules && !filtered_modules.is_empty())
        || (show_plugins && !filtered_plugins.is_empty());

    let input_id = use_signal(|| format!("grid-picker-input-{}", Uuid::new_v4().as_simple()));
    let iid = input_id();

    let left = props.click_x;
    let top = props.click_y;
    let panel_style = format!("position: fixed; left: {left}px; top: {top}px; z-index: 9999;");

    let focus_js = format!(
        r#"(function(){{ var el = document.getElementById('{iid}'); if(el) el.focus(); }})()"#
    );

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0",
            style: "z-index: 9998;",
            onmousedown: move |evt| {
                evt.stop_propagation();
                props.on_close.call(());
            },
        }
        // Dropdown panel
        div {
            class: "w-60 max-h-80 \
                    bg-zinc-900 border border-zinc-700/60 rounded-xl shadow-2xl shadow-black/50 \
                    flex flex-col overflow-hidden",
            style: "{panel_style}",
            onclick: move |evt| evt.stop_propagation(),
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    props.on_close.call(());
                }
                evt.stop_propagation();
            },
            // Subtab bar
            div { class: "flex items-center gap-0.5 px-2 pt-2 pb-1",
                {
                    let tabs = [
                        (PickerTab::All, "All"),
                        (PickerTab::Blocks, "Blocks"),
                        (PickerTab::Modules, "Modules"),
                        (PickerTab::Plugins, "Plugins"),
                    ];
                    rsx! {
                        for (t, label) in tabs {
                            button {
                                key: "{label}",
                                class: if tab == t {
                                    "px-2.5 py-1 rounded-md text-[10px] font-semibold text-zinc-200 bg-zinc-700/70 transition-colors"
                                } else {
                                    "px-2.5 py-1 rounded-md text-[10px] font-medium text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50 transition-colors"
                                },
                                onclick: move |_| active_tab.set(t),
                                "{label}"
                            }
                        }
                    }
                }
            }
            // Search input
            div { class: "px-3 py-1.5 border-b border-zinc-800/60",
                input {
                    id: "{iid}",
                    class: "w-full bg-zinc-800/80 border border-zinc-700/50 rounded-md px-2.5 py-1.5 \
                            text-[11px] text-zinc-200 outline-none focus:border-purple-500/40 \
                            placeholder:text-zinc-600 transition-all",
                    r#type: "text",
                    placeholder: if show_plugins {
                        "Search plugins..."
                    } else if show_modules && !show_blocks {
                        "Search modules..."
                    } else if show_blocks && !show_modules {
                        "Search blocks..."
                    } else {
                        "Search blocks & modules..."
                    },
                    value: "{search}",
                    oninput: move |evt| search.set(evt.value().clone()),
                }
            }
            script { "{focus_js}" }
            // Results
            div { class: "flex-1 overflow-y-auto min-h-0 px-1.5 py-1.5",
                if !has_results {
                    div { class: "flex items-center justify-center py-4",
                        p { class: "text-[10px] text-zinc-600", "No results" }
                    }
                } else {
                    // Plugins section
                    if show_plugins && !filtered_plugins.is_empty() {
                        div { class: "mb-2",
                            SectionHeader { size: SectionHeaderSize::Small, class: "px-1.5", label: "NDSP Archetype X" }
                            for (idx, plugin_name) in filtered_plugins.iter() {
                                {
                                    let idx = *idx;
                                    let plugin_name = *plugin_name;
                                    let label = archetype_label(plugin_name);
                                    let is_expanded = expanded_plugin() == Some(idx);
                                    rsx! {
                                        // Plugin header row
                                        button {
                                            key: "plugin-{idx}",
                                            class: "w-full flex items-center gap-2 px-2 py-1.5 rounded-md text-left \
                                                    hover:bg-zinc-800/60 transition-all duration-100",
                                            onclick: move |_| {
                                                if expanded_plugin() == Some(idx) {
                                                    expanded_plugin.set(None);
                                                } else {
                                                    expanded_plugin.set(Some(idx));
                                                }
                                            },
                                            StatusDot { color: StatusDotColor::Custom("rgb(168 85 247 / 0.8)".to_string()), round: false }
                                            div { class: "flex-1 min-w-0",
                                                span { class: "text-[11px] font-medium text-zinc-200 block", "{label}" }
                                                span { class: "text-[9px] text-zinc-500", "Neural DSP" }
                                            }
                                            span {
                                                class: "text-[9px] text-zinc-500 flex-shrink-0",
                                                if is_expanded { "\u{25B4}" } else { "\u{25BE}" }
                                            }
                                        }
                                        // Expanded detail
                                        if is_expanded {
                                            {
                                                let defs = templates();
                                                let def = &defs[idx];
                                                let module_preview: Vec<(String, usize, ModuleType)> = def.modules.iter().map(|m| {
                                                    (m.label.clone(), m.blocks.len(), m.module_type)
                                                }).collect();
                                                let slots = plugin_def_to_grid_slots(def, col, row);
                                                rsx! {
                                                    div { class: "ml-5 mr-1 mb-1.5 border-l border-zinc-800/60 pl-2",
                                                        // Module list preview
                                                        for (m_label, block_count, mt) in module_preview.iter() {
                                                            {
                                                                let color = module_type_color(*mt);
                                                                rsx! {
                                                                    div {
                                                                        key: "mod-preview-{m_label}",
                                                                        class: "flex items-center gap-1.5 py-0.5",
                                                                        StatusDot { color: StatusDotColor::Custom(color.bg.to_string()), size: StatusDotSize::Small }
                                                                        span { class: "text-[10px] text-zinc-400", "{m_label}" }
                                                                        span { class: "text-[9px] text-zinc-600", "({block_count})" }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        // Add Full Chain button
                                                        button {
                                                            class: "mt-1.5 w-full px-2 py-1.5 rounded-md text-[10px] font-semibold \
                                                                    bg-purple-600/30 text-purple-300 hover:bg-purple-600/50 \
                                                                    transition-colors text-center",
                                                            onclick: move |_| {
                                                                if let Some(ref cb) = props.on_add_slots {
                                                                    cb.call(slots.clone());
                                                                }
                                                            },
                                                            "Add Full Chain"
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
                    // Module types section
                    if show_modules && !filtered_modules.is_empty() {
                        div { class: "mb-2",
                            SectionHeader { size: SectionHeaderSize::Small, class: "px-1.5", label: "Modules" }
                            for (mt, name, desc) in filtered_modules.iter() {
                                {
                                    let mt = *mt;
                                    let name = *name;
                                    let desc = *desc;
                                    let color = module_type_color(mt);
                                    rsx! {
                                        button {
                                            key: "mod-{name}",
                                            class: "w-full flex items-center gap-2 px-2 py-1.5 rounded-md text-left \
                                                    hover:bg-zinc-800/60 transition-all duration-100",
                                            onclick: move |_| {
                                                let new_slot = GridSlot {
                                                    id: Uuid::new_v4(),
                                                    block_type: BlockType::Custom,
                                                    block_preset_name: Some(format!("{} Module", name)),
                                                    plugin_name: Some(format!("module:{}", mt.display_name())),
                                                    col,
                                                    row,
                                                    module_group: None,
                                                    module_type: None,
                                                    layer_group: None,
                                                    engine_group: None,
                                                    is_template: true,
                                                    bypassed: false,
                                                    is_phantom: false,
                                                    parameters: Vec::new(),
                                                    preset_id: None,
                                                    snapshot_id: None,
                                                };
                                                props.on_add_slot.call(new_slot);
                                            },
                                            StatusDot { color: StatusDotColor::Custom(color.bg.to_string()), round: false }
                                            div { class: "flex-1 min-w-0",
                                                span { class: "text-[11px] font-medium text-zinc-200 block", "{name}" }
                                                span { class: "text-[9px] text-zinc-500", "{desc}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Block types section (grouped by category)
                    if show_blocks && !filtered_blocks.is_empty() {
                        for category in BlockCategory::all().iter() {
                            {
                                let cat_items: Vec<&BlockType> = filtered_blocks
                                    .iter()
                                    .filter(|bt| bt.category() == *category)
                                    .collect();
                                if cat_items.is_empty() {
                                    return rsx! {};
                                }
                                let cat_name = category.display_name();
                                rsx! {
                                    div { class: "mb-1.5",
                                        SectionHeader { size: SectionHeaderSize::Small, class: "px-1.5", label: "{cat_name}" }
                                        for bt in cat_items.iter() {
                                            {
                                                let bt = **bt;
                                                let name = bt.display_name();
                                                let desc = bt.category().display_name();
                                                let color = bt.color();
                                                rsx! {
                                                    button {
                                                        key: "{name}",
                                                        class: "w-full flex items-center gap-2 px-2 py-1.5 rounded-md text-left \
                                                                hover:bg-zinc-800/60 transition-all duration-100",
                                                        onclick: move |_| {
                                                            let new_slot = GridSlot {
                                                                id: Uuid::new_v4(),
                                                                block_type: bt,
                                                                block_preset_name: None,
                                                                plugin_name: None,
                                                                col,
                                                                row,
                                                                module_group: None,
                                                                module_type: None,
                                                                layer_group: None,
                                                                engine_group: None,
                                                                is_template: true,
                                                                bypassed: false,
                                                                is_phantom: false,
                                                                parameters: Vec::new(),
                                                                preset_id: None,
                                                                snapshot_id: None,
                                                            };
                                                            props.on_add_slot.call(new_slot);
                                                        },
                                                        StatusDot { color: StatusDotColor::Custom(color.bg.to_string()) }
                                                        div { class: "flex-1 min-w-0",
                                                            span { class: "text-[11px] font-medium text-zinc-200 block", "{name}" }
                                                            span { class: "text-[9px] text-zinc-500", "{desc}" }
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
            }
        }
    }
}
