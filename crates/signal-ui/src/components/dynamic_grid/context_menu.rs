//! Grid context menu — right-click menu for block and module save actions.
//!
//! Uses lumen-blocks `ContextMenuItem`, `ContextMenuLabel`, and
//! `ContextMenuSeparator` for visual consistency, rendered inside the
//! parent's `ContextMenuContent`.

use dioxus::prelude::*;
use lumen_blocks::components::context_menu::{
    ContextMenuItem, ContextMenuLabel, ContextMenuSeparator,
};

use super::types::GridSlot;
use super::GridSelection;

// ── Props ────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct GridContextMenuProps {
    /// Which block/module was right-clicked.
    pub target: Option<GridSelection>,
    /// Current chain (to look up slot data).
    pub chain: Vec<GridSlot>,

    // Block-level callbacks
    #[props(default)]
    pub on_save: Option<EventHandler<GridSlot>>,
    #[props(default)]
    pub on_save_as_new: Option<EventHandler<(GridSlot, String)>>,
    #[props(default)]
    pub on_save_block_snapshot: Option<EventHandler<GridSlot>>,
    #[props(default)]
    pub on_save_block_snapshot_as: Option<EventHandler<(GridSlot, String)>>,

    // Module-level callbacks
    #[props(default)]
    pub on_save_module_preset_as: Option<EventHandler<(Vec<GridSlot>, String, signal::ModuleType)>>,
    #[props(default)]
    pub on_save_module_snapshot: Option<EventHandler<Vec<GridSlot>>>,
    #[props(default)]
    pub on_save_module_snapshot_as:
        Option<EventHandler<(Vec<GridSlot>, String, signal::ModuleType)>>,

    /// Called to close the menu after an action completes.
    pub on_close: EventHandler<()>,
}

// ── Component ────────────────────────────────────────────────────

#[component]
pub fn GridContextMenu(props: GridContextMenuProps) -> Element {
    let mut save_name: Signal<String> = use_signal(String::new);
    let mut name_action: Signal<Option<&'static str>> = use_signal(|| None);

    match &props.target {
        Some(GridSelection::Block(id)) => {
            let slot = props.chain.iter().find(|s| s.id == *id).cloned();
            if let Some(slot) = slot {
                render_block_menu(slot, name_action, save_name, &props)
            } else {
                rsx! {}
            }
        }
        Some(GridSelection::Module(name)) => {
            let module_slots: Vec<GridSlot> = props
                .chain
                .iter()
                .filter(|s| s.module_group.as_deref() == Some(name.as_str()))
                .cloned()
                .collect();

            if module_slots.is_empty() {
                return rsx! {};
            }

            let module_type = module_slots
                .first()
                .and_then(|s| s.module_type)
                .unwrap_or_default();

            render_module_menu(
                name,
                module_slots,
                module_type,
                name_action,
                save_name,
                &props,
            )
        }
        None => rsx! {},
    }
}

// ── Block menu ───────────────────────────────────────────────────

fn render_block_menu(
    slot: GridSlot,
    mut name_action: Signal<Option<&'static str>>,
    mut save_name: Signal<String>,
    props: &GridContextMenuProps,
) -> Element {
    let has_preset = slot.preset_id.is_some();
    let label = slot
        .block_preset_name
        .clone()
        .unwrap_or_else(|| format!("{:?}", slot.block_type));

    let active_action = name_action();
    let current_name = save_name();
    let is_name_empty = current_name.is_empty();

    let on_save = props.on_save.clone();
    let on_save_as_new = props.on_save_as_new.clone();
    let on_save_block_snapshot = props.on_save_block_snapshot.clone();
    let on_save_block_snapshot_as = props.on_save_block_snapshot_as.clone();
    let on_close = props.on_close.clone();

    let action_label = match active_action {
        Some("save_block_preset_as") => "New Preset Name",
        Some("save_block_snapshot_as") => "Snapshot Name",
        _ => "Name",
    };
    let name_for_keydown = current_name.clone();
    let name_for_btn = current_name.clone();

    rsx! {
        ContextMenuLabel { "Block: {label}" }
        ContextMenuSeparator {}

        // Save Block Preset (update existing)
        if has_preset {
            ContextMenuItem {
                value: "save_block_preset",
                index: 0usize,
                on_select: {
                    let slot = slot.clone();
                    let on_save = on_save.clone();
                    let on_close = on_close.clone();
                    move |_: String| {
                        if let Some(ref cb) = on_save {
                            cb.call(slot.clone());
                        }
                        on_close.call(());
                    }
                },
                "Save Block Preset"
            }
        }

        // Save Block Preset As...
        div {
            class: format!(
                "relative flex cursor-pointer select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none transition-colors hover:bg-accent hover:text-accent-foreground {}",
                if active_action == Some("save_block_preset_as") { "bg-accent text-accent-foreground" } else { "" }
            ),
            onclick: move |evt| {
                evt.stop_propagation();
                name_action.set(Some("save_block_preset_as"));
            },
            "Save Block Preset As\u{2026}"
        }

        if has_preset {
            ContextMenuSeparator {}

            // Save Block Snapshot
            ContextMenuItem {
                value: "save_block_snapshot",
                index: 2usize,
                on_select: {
                    let slot = slot.clone();
                    let on_snap = on_save_block_snapshot.clone();
                    let on_close = on_close.clone();
                    move |_: String| {
                        if let Some(ref cb) = on_snap {
                            cb.call(slot.clone());
                        }
                        on_close.call(());
                    }
                },
                "Save Block Snapshot"
            }

            // Save Block Snapshot As...
            div {
                class: format!(
                    "relative flex cursor-pointer select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none transition-colors hover:bg-accent hover:text-accent-foreground {}",
                    if active_action == Some("save_block_snapshot_as") { "bg-accent text-accent-foreground" } else { "" }
                ),
                onclick: move |evt| {
                    evt.stop_propagation();
                    name_action.set(Some("save_block_snapshot_as"));
                },
                "Save Block Snapshot As\u{2026}"
            }
        }

        // Inline name input
        if active_action.is_some() {
            div {
                class: "px-2 py-1.5 border-t border-border",
                onclick: move |evt| evt.stop_propagation(),
                p {
                    class: "text-[10px] text-muted-foreground mb-1",
                    "{action_label}"
                }
                input {
                    r#type: "text",
                    class: "w-full px-2 py-1 text-xs bg-background border border-border rounded text-foreground focus:outline-none focus:ring-1 focus:ring-ring",
                    placeholder: "{action_label}",
                    value: "{current_name}",
                    autofocus: true,
                    oninput: move |evt| save_name.set(evt.value()),
                    onkeydown: {
                        let slot = slot.clone();
                        let on_save_as_new = on_save_as_new.clone();
                        let on_save_block_snapshot_as = on_save_block_snapshot_as.clone();
                        let on_close = on_close.clone();
                        move |evt: KeyboardEvent| {
                            if evt.key() == Key::Enter {
                                let name = name_for_keydown.clone();
                                if !name.is_empty() {
                                    match active_action {
                                        Some("save_block_preset_as") => {
                                            if let Some(ref cb) = on_save_as_new {
                                                cb.call((slot.clone(), name));
                                            }
                                        }
                                        Some("save_block_snapshot_as") => {
                                            if let Some(ref cb) = on_save_block_snapshot_as {
                                                cb.call((slot.clone(), name));
                                            }
                                        }
                                        _ => {}
                                    }
                                    on_close.call(());
                                }
                            } else if evt.key() == Key::Escape {
                                name_action.set(None);
                                save_name.set(String::new());
                            }
                        }
                    },
                }
                div {
                    class: "flex gap-1 mt-1",
                    button {
                        class: "flex-1 px-2 py-0.5 text-[10px] rounded bg-primary text-primary-foreground hover:bg-primary/90 font-medium",
                        disabled: is_name_empty,
                        onclick: {
                            let slot = slot.clone();
                            let on_save_as_new = on_save_as_new.clone();
                            let on_save_block_snapshot_as = on_save_block_snapshot_as.clone();
                            let on_close = on_close.clone();
                            move |_| {
                                let name = name_for_btn.clone();
                                if !name.is_empty() {
                                    match active_action {
                                        Some("save_block_preset_as") => {
                                            if let Some(ref cb) = on_save_as_new {
                                                cb.call((slot.clone(), name));
                                            }
                                        }
                                        Some("save_block_snapshot_as") => {
                                            if let Some(ref cb) = on_save_block_snapshot_as {
                                                cb.call((slot.clone(), name));
                                            }
                                        }
                                        _ => {}
                                    }
                                    on_close.call(());
                                }
                            }
                        },
                        "Save"
                    }
                    button {
                        class: "flex-1 px-2 py-0.5 text-[10px] rounded bg-muted text-muted-foreground hover:bg-muted/80 font-medium",
                        onclick: move |_| {
                            name_action.set(None);
                            save_name.set(String::new());
                        },
                        "Cancel"
                    }
                }
            }
        }
    }
}

// ── Module menu ──────────────────────────────────────────────────

fn render_module_menu(
    name: &str,
    module_slots: Vec<GridSlot>,
    module_type: signal::ModuleType,
    mut name_action: Signal<Option<&'static str>>,
    mut save_name: Signal<String>,
    props: &GridContextMenuProps,
) -> Element {
    let active_action = name_action();
    let current_name = save_name();
    let is_name_empty = current_name.is_empty();

    let on_save_module_preset_as = props.on_save_module_preset_as.clone();
    let on_save_module_snapshot_as = props.on_save_module_snapshot_as.clone();
    let on_close = props.on_close.clone();

    let action_label = match active_action {
        Some("save_module_preset_as") => "Module Preset Name",
        Some("save_module_snapshot_as") => "Module Snapshot Name",
        _ => "Name",
    };
    let name_for_keydown = current_name.clone();
    let name_for_btn = current_name.clone();

    rsx! {
        ContextMenuLabel { "Module: {name}" }
        ContextMenuSeparator {}

        // Save Module Preset As...
        div {
            class: format!(
                "relative flex cursor-pointer select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none transition-colors hover:bg-accent hover:text-accent-foreground {}",
                if active_action == Some("save_module_preset_as") { "bg-accent text-accent-foreground" } else { "" }
            ),
            onclick: move |evt| {
                evt.stop_propagation();
                name_action.set(Some("save_module_preset_as"));
            },
            "Save Module Preset As\u{2026}"
        }

        ContextMenuSeparator {}

        // Save Module Snapshot As...
        div {
            class: format!(
                "relative flex cursor-pointer select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none transition-colors hover:bg-accent hover:text-accent-foreground {}",
                if active_action == Some("save_module_snapshot_as") { "bg-accent text-accent-foreground" } else { "" }
            ),
            onclick: move |evt| {
                evt.stop_propagation();
                name_action.set(Some("save_module_snapshot_as"));
            },
            "Save Module Snapshot As\u{2026}"
        }

        // Inline name input
        if active_action.is_some() {
            div {
                class: "px-2 py-1.5 border-t border-border",
                onclick: move |evt| evt.stop_propagation(),
                p {
                    class: "text-[10px] text-muted-foreground mb-1",
                    "{action_label}"
                }
                input {
                    r#type: "text",
                    class: "w-full px-2 py-1 text-xs bg-background border border-border rounded text-foreground focus:outline-none focus:ring-1 focus:ring-ring",
                    placeholder: "{action_label}",
                    value: "{current_name}",
                    autofocus: true,
                    oninput: move |evt| save_name.set(evt.value()),
                    onkeydown: {
                        let slots = module_slots.clone();
                        let on_save_module_preset_as = on_save_module_preset_as.clone();
                        let on_save_module_snapshot_as = on_save_module_snapshot_as.clone();
                        let on_close = on_close.clone();
                        move |evt: KeyboardEvent| {
                            if evt.key() == Key::Enter {
                                let name = name_for_keydown.clone();
                                if !name.is_empty() {
                                    match active_action {
                                        Some("save_module_preset_as") => {
                                            if let Some(ref cb) = on_save_module_preset_as {
                                                cb.call((slots.clone(), name, module_type));
                                            }
                                        }
                                        Some("save_module_snapshot_as") => {
                                            if let Some(ref cb) = on_save_module_snapshot_as {
                                                cb.call((slots.clone(), name, module_type));
                                            }
                                        }
                                        _ => {}
                                    }
                                    on_close.call(());
                                }
                            } else if evt.key() == Key::Escape {
                                name_action.set(None);
                                save_name.set(String::new());
                            }
                        }
                    },
                }
                div {
                    class: "flex gap-1 mt-1",
                    button {
                        class: "flex-1 px-2 py-0.5 text-[10px] rounded bg-primary text-primary-foreground hover:bg-primary/90 font-medium",
                        disabled: is_name_empty,
                        onclick: {
                            let slots = module_slots.clone();
                            let on_save_module_preset_as = on_save_module_preset_as.clone();
                            let on_save_module_snapshot_as = on_save_module_snapshot_as.clone();
                            let on_close = on_close.clone();
                            move |_| {
                                let name = name_for_btn.clone();
                                if !name.is_empty() {
                                    match active_action {
                                        Some("save_module_preset_as") => {
                                            if let Some(ref cb) = on_save_module_preset_as {
                                                cb.call((slots.clone(), name, module_type));
                                            }
                                        }
                                        Some("save_module_snapshot_as") => {
                                            if let Some(ref cb) = on_save_module_snapshot_as {
                                                cb.call((slots.clone(), name, module_type));
                                            }
                                        }
                                        _ => {}
                                    }
                                    on_close.call(());
                                }
                            }
                        },
                        "Save"
                    }
                    button {
                        class: "flex-1 px-2 py-0.5 text-[10px] rounded bg-muted text-muted-foreground hover:bg-muted/80 font-medium",
                        onclick: move |_| {
                            name_action.set(None);
                            save_name.set(String::new());
                        },
                        "Cancel"
                    }
                }
            }
        }
    }
}
