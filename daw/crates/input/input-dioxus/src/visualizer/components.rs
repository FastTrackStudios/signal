//! Dioxus components for the keyboard & mouse modifier visualizer.
//!
//! Full-screen layout with section-colored keys, filter chips for action
//! sections and workflow contexts, and a scrollable binding reference table.

use std::collections::HashSet;

use dioxus::prelude::*;

use input::key::{KeyCode, Modifiers};
use input::mode::ModeId;

use super::data::{
    ActionSection, ContextBindingGroup, KeyBindingInfo, MouseBindingInfo, ScrollBindingInfo,
    collect_context_bindings, collect_key_bindings, collect_mouse_bindings,
    collect_scroll_bindings, context_display_name,
};
use super::layout::{KeyBlock, KeyDef, KeyRow, qwerty_layout};

use crate::hook::InputHandle;

/// Base key unit in pixels. All key sizes are multiples of this.
const KEY_UNIT: f32 = 52.0;

// ---------------------------------------------------------------------------
// Tab enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Keyboard,
    MouseModifiers,
}

// ---------------------------------------------------------------------------
// Context filter
// ---------------------------------------------------------------------------

/// Which workflow context layer to display.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ContextFilter {
    /// Show all bindings (base + all context layers merged).
    All,
    /// Base keymap only (no context overlays).
    Global,
    /// A specific context layer by its when-label.
    Layer(String),
}

// ---------------------------------------------------------------------------
// Root component
// ---------------------------------------------------------------------------

/// Root keyboard & mouse modifier visualizer.
///
/// Reads binding data from the `InputHandle` at render time so it always
/// reflects the active keymap config.
#[component]
pub fn InputVisualizer(handle: InputHandle) -> Element {
    let mut active_tab = use_signal(|| Tab::Keyboard);
    let active_sections = use_signal(HashSet::<ActionSection>::new);
    let context_filter = use_signal(|| ContextFilter::All);

    // Extract binding data from the processor.
    let processor = handle.processor();
    let current_mode = handle.current_mode();

    // Base keymap bindings
    let base_bindings: Vec<KeyBindingInfo> = processor
        .keymaps()
        .get(&current_mode)
        .map(collect_key_bindings)
        .unwrap_or_default();

    // Context layer groups
    let context_groups: Vec<ContextBindingGroup> = processor
        .context_keymaps()
        .get(&current_mode)
        .map(|layers| collect_context_bindings(layers))
        .unwrap_or_default();

    let mouse_bindings: Vec<MouseBindingInfo> = processor
        .mouse_bindings()
        .get(&current_mode)
        .map(collect_mouse_bindings)
        .unwrap_or_default();

    let scroll_bindings: Vec<ScrollBindingInfo> = processor
        .scroll_bindings()
        .get(&current_mode)
        .map(collect_scroll_bindings)
        .unwrap_or_default();

    let mode_ids: Vec<ModeId> = processor.keymaps().keys().cloned().collect();

    // Drop the borrow before rendering.
    drop(processor);

    // Build the effective binding set based on context filter.
    let effective_bindings =
        build_effective_bindings(&base_bindings, &context_groups, &(context_filter)());

    // Context labels for the filter dropdown
    let context_labels: Vec<String> = context_groups
        .iter()
        .map(|g| g.when_label.clone())
        .collect();

    rsx! {
        div { class: "w-full h-full flex flex-col bg-zinc-900 text-zinc-100 select-none overflow-hidden",
            // -- Sticky header
            div { class: "flex-none flex flex-col gap-2 px-4 py-3 border-b border-zinc-800",
                // Row 1: mode badge + tab switcher
                div { class: "flex items-center gap-4",
                    ModeSelector {
                        modes: mode_ids,
                        current: current_mode.clone(),
                    }
                    // Section filter chips
                    SectionFilterChips {
                        active_sections: active_sections,
                    }
                    // Context filter
                    ContextFilterSelect {
                        context_labels: context_labels,
                        context_filter: context_filter,
                    }
                    // Tab switcher (right side)
                    div { class: "flex gap-1 ml-auto",
                        TabButton {
                            label: "Keyboard",
                            active: (active_tab)() == Tab::Keyboard,
                            on_click: move |_| active_tab.set(Tab::Keyboard),
                        }
                        TabButton {
                            label: "Mouse",
                            active: (active_tab)() == Tab::MouseModifiers,
                            on_click: move |_| active_tab.set(Tab::MouseModifiers),
                        }
                    }
                }
            }

            // -- Body (fills remaining space)
            match (active_tab)() {
                Tab::Keyboard => rsx! {
                    KeyboardTab {
                        bindings: effective_bindings,
                        base_bindings: base_bindings,
                        context_groups: context_groups,
                        context_filter: context_filter,
                        active_sections: active_sections,
                    }
                },
                Tab::MouseModifiers => rsx! {
                    MouseModifiersTab {
                        mouse: mouse_bindings,
                        scroll: scroll_bindings,
                        active_sections: active_sections,
                    }
                },
            }
        }
    }
}

/// Merge base bindings with context layers based on the active context filter.
fn build_effective_bindings(
    base: &[KeyBindingInfo],
    groups: &[ContextBindingGroup],
    filter: &ContextFilter,
) -> Vec<KeyBindingInfo> {
    match filter {
        ContextFilter::Global => base.to_vec(),
        ContextFilter::All => {
            // Union: start with base, overlay each context layer
            let mut merged = base.to_vec();
            for group in groups {
                for binding in &group.bindings {
                    // Context layer overrides base bindings with the same first chord
                    if let Some(existing) =
                        merged.iter_mut().find(|b| b.sequence == binding.sequence)
                    {
                        *existing = binding.clone();
                    } else {
                        merged.push(binding.clone());
                    }
                }
            }
            merged.sort_by(|a, b| a.display.cmp(&b.display));
            merged
        }
        ContextFilter::Layer(label) => {
            // Base + specific layer overlay
            let mut merged = base.to_vec();
            if let Some(group) = groups.iter().find(|g| g.when_label == *label) {
                for binding in &group.bindings {
                    if let Some(existing) =
                        merged.iter_mut().find(|b| b.sequence == binding.sequence)
                    {
                        *existing = binding.clone();
                    } else {
                        merged.push(binding.clone());
                    }
                }
            }
            merged.sort_by(|a, b| a.display.cmp(&b.display));
            merged
        }
    }
}

// ---------------------------------------------------------------------------
// Mode selector
// ---------------------------------------------------------------------------

#[component]
fn ModeSelector(modes: Vec<ModeId>, current: ModeId) -> Element {
    rsx! {
        div { class: "flex items-center gap-2",
            span { class: "text-xs text-zinc-500 uppercase tracking-wider", "Mode" }
            span { class: "px-2 py-0.5 rounded bg-zinc-700 text-sm font-mono font-bold",
                "{current.as_str()}"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Section filter chips
// ---------------------------------------------------------------------------

#[component]
fn SectionFilterChips(active_sections: Signal<HashSet<ActionSection>>) -> Element {
    let current = (active_sections)();
    let all_active = current.is_empty(); // empty = show all

    rsx! {
        div { class: "flex items-center gap-1.5 flex-wrap",
            // "All" reset button
            button {
                class: if all_active {
                    "px-2.5 py-1 text-xs rounded-full bg-zinc-600 text-zinc-100 font-medium"
                } else {
                    "px-2.5 py-1 text-xs rounded-full bg-zinc-800 text-zinc-400 hover:bg-zinc-700 cursor-pointer"
                },
                onclick: move |_| active_sections.set(HashSet::new()),
                "All"
            }
            for &section in ActionSection::ALL {
                {
                    let is_active = !all_active && current.contains(&section);
                    let dot = section.dot_class();
                    rsx! {
                        button {
                            class: if is_active {
                                "flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-full bg-zinc-600 text-zinc-100 font-medium cursor-pointer"
                            } else {
                                "flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-full bg-zinc-800 text-zinc-400 hover:bg-zinc-700 cursor-pointer"
                            },
                            onclick: move |_| {
                                let mut s = (active_sections)();
                                if s.contains(&section) {
                                    s.remove(&section);
                                } else {
                                    s.insert(section);
                                }
                                active_sections.set(s);
                            },
                            span { class: "w-2 h-2 rounded-full {dot}" }
                            "{section.display_name()}"
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Context filter dropdown
// ---------------------------------------------------------------------------

#[component]
fn ContextFilterSelect(
    context_labels: Vec<String>,
    context_filter: Signal<ContextFilter>,
) -> Element {
    let current = (context_filter)();

    let current_label = match &current {
        ContextFilter::All => "All Contexts".to_string(),
        ContextFilter::Global => "Global".to_string(),
        ContextFilter::Layer(l) => context_display_name(l),
    };

    rsx! {
        div { class: "relative flex items-center gap-1",
            span { class: "text-xs text-zinc-500", "Context:" }
            select {
                class: "bg-zinc-800 text-zinc-200 text-xs rounded px-2 py-1 border border-zinc-700 cursor-pointer appearance-none pr-6",
                value: "{current_label}",
                onchange: move |e: Event<FormData>| {
                    let val = e.value();
                    match val.as_str() {
                        "all" => context_filter.set(ContextFilter::All),
                        "global" => context_filter.set(ContextFilter::Global),
                        other => context_filter.set(ContextFilter::Layer(other.to_string())),
                    }
                },
                option { value: "all", "All Contexts" }
                option { value: "global", "Global" }
                for label in &context_labels {
                    option {
                        value: "{label}",
                        "{context_display_name(label)}"
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tab button
// ---------------------------------------------------------------------------

#[component]
fn TabButton(label: &'static str, active: bool, on_click: EventHandler<MouseEvent>) -> Element {
    let cls = if active {
        "px-3 py-1 text-sm rounded bg-zinc-700 text-zinc-100"
    } else {
        "px-3 py-1 text-sm rounded bg-zinc-800 text-zinc-400 hover:bg-zinc-700 hover:text-zinc-200 cursor-pointer"
    };
    rsx! {
        button { class: cls, onclick: move |e| on_click.call(e), "{label}" }
    }
}

// ---------------------------------------------------------------------------
// Keyboard tab
// ---------------------------------------------------------------------------

#[component]
fn KeyboardTab(
    bindings: Vec<KeyBindingInfo>,
    base_bindings: Vec<KeyBindingInfo>,
    context_groups: Vec<ContextBindingGroup>,
    context_filter: Signal<ContextFilter>,
    active_sections: Signal<HashSet<ActionSection>>,
) -> Element {
    let active_modifiers = use_signal(Modifiers::default);
    let selected_key = use_signal(|| Option::<KeyCode>::None);
    let layout = qwerty_layout();

    let section_set = (active_sections)();

    // Build table rows with context annotation
    let table_rows = build_table_rows(&base_bindings, &context_groups, &(context_filter)());

    rsx! {
        div { class: "flex-1 flex flex-col min-h-0",
            // Modifier toggles
            div { class: "flex-none px-4 py-2",
                ModifierToggles { modifiers: active_modifiers }
            }

            // Keyboard layout (~50% height)
            div { class: "flex-none flex justify-center gap-4 px-4 pb-3",
                for block in &layout {
                    KeyBlockView {
                        block: block.clone(),
                        bindings: bindings.clone(),
                        active_modifiers: active_modifiers,
                        selected_key: selected_key,
                        active_sections: section_set.clone(),
                    }
                }
            }

            // Binding reference table (scrollable, fills remaining space)
            div { class: "flex-1 min-h-0 overflow-y-auto px-4 pb-3",
                BindingTable {
                    rows: table_rows,
                    active_sections: section_set.clone(),
                    selected_key: (selected_key)(),
                }
            }
        }
    }
}

/// A row in the binding reference table, annotated with its context source.
#[derive(Debug, Clone, PartialEq)]
struct TableRow {
    section: ActionSection,
    /// First key code in the sequence (for matching selected key).
    first_key: Option<KeyCode>,
    display: String,
    action_label: String,
    context: String,
}

/// Build annotated table rows from base + context groups.
fn build_table_rows(
    base: &[KeyBindingInfo],
    groups: &[ContextBindingGroup],
    filter: &ContextFilter,
) -> Vec<TableRow> {
    let mut rows = Vec::new();

    let include_base = matches!(filter, ContextFilter::All | ContextFilter::Global);
    let include_all_layers = matches!(filter, ContextFilter::All);

    if include_base {
        for b in base {
            rows.push(TableRow {
                section: b.section,
                first_key: b.sequence.first().map(|c| c.key.clone()),
                display: b.display.clone(),
                action_label: b.action.label(),
                context: "Global".to_string(),
            });
        }
    }

    for group in groups {
        let include = include_all_layers
            || matches!(filter, ContextFilter::Layer(l) if *l == group.when_label);
        if !include {
            continue;
        }
        let ctx_name = context_display_name(&group.when_label);
        for b in &group.bindings {
            rows.push(TableRow {
                section: b.section,
                first_key: b.sequence.first().map(|c| c.key.clone()),
                display: b.display.clone(),
                action_label: b.action.label(),
                context: ctx_name.clone(),
            });
        }
    }

    rows.sort_by(|a, b| a.display.cmp(&b.display));
    rows
}

// ---------------------------------------------------------------------------
// Modifier toggles
// ---------------------------------------------------------------------------

#[component]
fn ModifierToggles(modifiers: Signal<Modifiers>) -> Element {
    let current = (modifiers)();

    rsx! {
        div { class: "flex gap-2",
            ModifierCheckbox {
                label: "Ctrl",
                checked: current.ctrl,
                on_toggle: move |_| {
                    let mut m = (modifiers)();
                    m.ctrl = !m.ctrl;
                    modifiers.set(m);
                },
            }
            ModifierCheckbox {
                label: "Alt",
                checked: current.alt,
                on_toggle: move |_| {
                    let mut m = (modifiers)();
                    m.alt = !m.alt;
                    modifiers.set(m);
                },
            }
            ModifierCheckbox {
                label: "Shift",
                checked: current.shift,
                on_toggle: move |_| {
                    let mut m = (modifiers)();
                    m.shift = !m.shift;
                    modifiers.set(m);
                },
            }
            ModifierCheckbox {
                label: "Cmd",
                checked: current.meta,
                on_toggle: move |_| {
                    let mut m = (modifiers)();
                    m.meta = !m.meta;
                    modifiers.set(m);
                },
            }
        }
    }
}

#[component]
fn ModifierCheckbox(
    label: &'static str,
    checked: bool,
    on_toggle: EventHandler<MouseEvent>,
) -> Element {
    let cls = if checked {
        "px-2 py-0.5 text-xs rounded bg-blue-600 text-white cursor-pointer"
    } else {
        "px-2 py-0.5 text-xs rounded bg-zinc-800 text-zinc-400 hover:bg-zinc-700 cursor-pointer"
    };
    rsx! {
        button { class: cls, onclick: move |e| on_toggle.call(e), "{label}" }
    }
}

// ---------------------------------------------------------------------------
// Key block / row / key
// ---------------------------------------------------------------------------

#[component]
fn KeyBlockView(
    block: KeyBlock,
    bindings: Vec<KeyBindingInfo>,
    active_modifiers: Signal<Modifiers>,
    selected_key: Signal<Option<KeyCode>>,
    active_sections: HashSet<ActionSection>,
) -> Element {
    rsx! {
        div { class: "flex flex-col gap-0.5",
            for row in &block.rows {
                KeyRowView {
                    row: row.clone(),
                    bindings: bindings.clone(),
                    active_modifiers: active_modifiers,
                    selected_key: selected_key,
                    active_sections: active_sections.clone(),
                }
            }
        }
    }
}

#[component]
fn KeyRowView(
    row: KeyRow,
    bindings: Vec<KeyBindingInfo>,
    active_modifiers: Signal<Modifiers>,
    selected_key: Signal<Option<KeyCode>>,
    active_sections: HashSet<ActionSection>,
) -> Element {
    let height_px = (row.height * KEY_UNIT) as u32;

    rsx! {
        div {
            class: "flex gap-0.5",
            style: "height: {height_px}px;",
            for key_def in &row.keys {
                KeyCap {
                    def: key_def.clone(),
                    bindings: bindings.clone(),
                    active_modifiers: active_modifiers,
                    selected_key: selected_key,
                    height_px: height_px,
                    active_sections: active_sections.clone(),
                }
            }
        }
    }
}

#[component]
fn KeyCap(
    def: KeyDef,
    bindings: Vec<KeyBindingInfo>,
    active_modifiers: Signal<Modifiers>,
    selected_key: Signal<Option<KeyCode>>,
    height_px: u32,
    active_sections: HashSet<ActionSection>,
) -> Element {
    let width_px = (def.width * KEY_UNIT) as u32;
    let mods = (active_modifiers)();

    // Find the binding for this key with the current modifiers.
    let matched_binding = bindings.iter().find(|b| {
        b.sequence
            .first()
            .is_some_and(|chord| chord.key == def.key_code && chord.modifiers == mods)
    });

    let action_label = matched_binding.map(|b| b.action.label());
    let section = matched_binding.map(|b| b.section);

    // Check if this key's section passes the active filter.
    let section_filter_active = !active_sections.is_empty();
    let passes_filter =
        !section_filter_active || section.is_some_and(|s| active_sections.contains(&s));

    let is_selected = (selected_key)()
        .as_ref()
        .is_some_and(|k| *k == def.key_code);

    // Determine styling based on section color and filter state.
    let bg = if is_selected {
        "bg-blue-700".to_string()
    } else if let Some(sec) = section {
        if passes_filter {
            sec.bg_class().to_string()
        } else {
            format!("{} opacity-30", sec.bg_class())
        }
    } else {
        "bg-zinc-800".to_string()
    };

    let border = if is_selected {
        "ring-1 ring-blue-400"
    } else if matched_binding.is_some() && passes_filter {
        "ring-1 ring-zinc-500/50"
    } else {
        ""
    };

    let opacity = if !passes_filter && matched_binding.is_some() && !is_selected {
        "opacity-30"
    } else {
        ""
    };

    let key_code = def.key_code.clone();

    rsx! {
        div {
            class: "flex flex-col items-center justify-center rounded text-sm font-mono cursor-pointer hover:brightness-125 {bg} {border} {opacity}",
            style: "width: {width_px}px; height: {height_px}px;",
            title: "{action_label.as_deref().unwrap_or(\"\")}",
            onclick: move |_| {
                let current = (selected_key)();
                if current.as_ref() == Some(&key_code) {
                    selected_key.set(None);
                } else {
                    selected_key.set(Some(key_code.clone()));
                }
            },
            span { class: "text-zinc-300 text-sm leading-tight", "{def.label}" }
            if let Some(ref lbl) = action_label {
                span { class: "text-[10px] text-zinc-400 truncate max-w-full px-1 leading-tight", "{lbl}" }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Binding reference table
// ---------------------------------------------------------------------------

#[component]
fn BindingTable(
    rows: Vec<TableRow>,
    active_sections: HashSet<ActionSection>,
    selected_key: Option<KeyCode>,
) -> Element {
    let section_filter_active = !active_sections.is_empty();

    let filtered: Vec<&TableRow> = rows
        .iter()
        .filter(|r| !section_filter_active || active_sections.contains(&r.section))
        .collect();

    rsx! {
        div { class: "rounded bg-zinc-800/50 border border-zinc-700/50",
            if filtered.is_empty() {
                p { class: "text-xs text-zinc-500 italic p-3", "No bindings match current filters" }
            } else {
                table { class: "w-full text-xs",
                    thead {
                        tr { class: "text-zinc-400 text-left border-b border-zinc-700/50",
                            th { class: "px-3 py-1.5 w-8", "" }
                            th { class: "px-3 py-1.5", "Sequence" }
                            th { class: "px-3 py-1.5", "Action" }
                            th { class: "px-3 py-1.5", "Context" }
                        }
                    }
                    tbody {
                        for row in &filtered {
                            {
                                let is_key_match = selected_key.as_ref().is_some_and(|k| {
                                    row.first_key.as_ref().is_some_and(|fk| fk == k)
                                });
                                let row_class = if is_key_match {
                                    "bg-blue-900/40 border-b border-zinc-800"
                                } else {
                                    "hover:bg-zinc-700/50 border-b border-zinc-800"
                                };
                                rsx! {
                                    tr { class: row_class,
                                        td { class: "px-3 py-1",
                                            span {
                                                class: "inline-block w-2 h-2 rounded-full {row.section.dot_class()}",
                                                title: "{row.section.display_name()}",
                                            }
                                        }
                                        td { class: "px-3 py-1 font-mono text-zinc-300", "{row.display}" }
                                        td { class: "px-3 py-1 {row.section.text_class()}", "{row.action_label}" }
                                        td { class: "px-3 py-1 text-zinc-500", "{row.context}" }
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

// ---------------------------------------------------------------------------
// Mouse modifiers tab
// ---------------------------------------------------------------------------

#[component]
fn MouseModifiersTab(
    mouse: Vec<MouseBindingInfo>,
    scroll: Vec<ScrollBindingInfo>,
    active_sections: Signal<HashSet<ActionSection>>,
) -> Element {
    let section_set = (active_sections)();
    let section_filter_active = !section_set.is_empty();

    rsx! {
        div { class: "flex-1 min-h-0 overflow-y-auto px-4 py-3",
            div { class: "rounded bg-zinc-800/50 border border-zinc-700/50",
                if mouse.is_empty() && scroll.is_empty() {
                    p { class: "text-xs text-zinc-500 italic p-3", "No mouse or scroll bindings in this mode" }
                } else {
                    table { class: "w-full text-xs",
                        thead {
                            tr { class: "text-zinc-400 text-left border-b border-zinc-700/50",
                                th { class: "px-3 py-1.5 w-8", "" }
                                th { class: "px-3 py-1.5", "Pattern" }
                                th { class: "px-3 py-1.5", "Action" }
                            }
                        }
                        tbody {
                            // Mouse bindings
                            for binding in &mouse {
                                {
                                    let visible = !section_filter_active || section_set.contains(&binding.section);
                                    let opacity = if visible { "" } else { "opacity-30" };
                                    rsx! {
                                        tr { class: "hover:bg-zinc-700/50 border-b border-zinc-800 {opacity}",
                                            td { class: "px-3 py-1",
                                                span {
                                                    class: "inline-block w-2 h-2 rounded-full {binding.section.dot_class()}",
                                                    title: "{binding.section.display_name()}",
                                                }
                                            }
                                            td { class: "px-3 py-1 font-mono text-zinc-300", "{binding.display}" }
                                            td { class: "px-3 py-1 {binding.section.text_class()}", "{binding.action_id}" }
                                        }
                                    }
                                }
                            }
                            // Scroll bindings
                            for binding in &scroll {
                                {
                                    let visible = !section_filter_active || section_set.contains(&binding.section);
                                    let opacity = if visible { "" } else { "opacity-30" };
                                    rsx! {
                                        tr { class: "hover:bg-zinc-700/50 border-b border-zinc-800 {opacity}",
                                            td { class: "px-3 py-1",
                                                span {
                                                    class: "inline-block w-2 h-2 rounded-full {binding.section.dot_class()}",
                                                    title: "{binding.section.display_name()}",
                                                }
                                            }
                                            td { class: "px-3 py-1 font-mono text-zinc-300", "{binding.display}" }
                                            td { class: "px-3 py-1 {binding.section.text_class()}", "{binding.action_id}" }
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
