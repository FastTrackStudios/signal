//! Actions List panel — browse, search, and manage keybind assignments.

use reaper_dioxus::prelude::*;

use super::data::{ActionEntry, ActionSection, BindingContext, collect_action_entries};
use crate::keybind_config::editor::{ProfileEditor, SectionEditor};
use crate::keybind_config::types::KeybindDef;
use crate::ui::tailwind::TailwindStyle;

// ---------------------------------------------------------------------------
// Add-binding dialog state
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct AddBindingState {
    /// The action ID we're adding a binding to.
    action_id: String,
    /// Key sequence input (styx format, e.g., "<C-s>", "h", "gg").
    keys: String,
    /// Description for the binding.
    desc: String,
    /// Selected section file to write to.
    section_file: String,
    /// Action search query (for picking a different action).
    action_search: String,
    /// Whether to show the action picker.
    show_action_picker: bool,
    /// Context the binding is active in ("Global", "Main", "MIDI Editor", …).
    context: String,
    /// Whether the binding runs in the main section even from a special editor.
    passthrough: bool,
    /// When editing an existing binding, the original key sequence to replace.
    /// `None` means we're adding a brand-new binding.
    editing_keys: Option<String>,
}

impl AddBindingState {
    /// A fresh add-binding dialog for `action_id`, writing to `section_file`.
    fn new_for(action_id: String, section_file: String) -> Self {
        Self {
            action_id,
            keys: String::new(),
            desc: String::new(),
            section_file,
            action_search: String::new(),
            show_action_picker: false,
            context: "Global".into(),
            passthrough: false,
            editing_keys: None,
        }
    }
}

/// Payload emitted when the user clicks "edit" on an existing binding.
#[derive(Clone, Debug, PartialEq)]
struct EditRequest {
    keys: String,
    action_id: String,
    desc: String,
}

/// Styx context labels offered in the editor (must match [`BindingContext`]).
const CONTEXT_OPTIONS: &[&str] = &[
    "Global",
    "Main",
    "MIDI Editor",
    "MIDI Inline",
    "Media Explorer",
];

/// Map an editor context label to the styx [`KeybindContext`] value.
/// `Global` maps to `None` (the styx default).
fn context_label_to_styx(label: &str) -> Option<crate::input::keybinds::KeybindContext> {
    use crate::input::keybinds::KeybindContext;
    match label {
        "Main" => Some(KeybindContext::Main),
        "MIDI Editor" => Some(KeybindContext::Midi),
        "MIDI Inline" => Some(KeybindContext::MidiInline),
        "Media Explorer" => Some(KeybindContext::MediaExplorer),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Root component
// ---------------------------------------------------------------------------

#[component]
pub fn ActionsListPanel() -> Element {
    let actions = collect_action_entries();

    let mut search = use_signal(String::new);
    let mut selected_action = use_signal(|| None::<String>);
    let mut status_msg = use_signal(|| None::<String>);
    let mut add_dialog = use_signal(|| None::<AddBindingState>);
    let mut active_context = use_signal(|| String::from("All"));

    // Get available section files for the dropdown
    let section_files = get_section_files();

    let search_term = search.read().to_lowercase();
    let filtered: Vec<&ActionEntry> = actions
        .iter()
        .filter(|a| {
            if search_term.is_empty() {
                return true;
            }
            a.action_id.to_lowercase().contains(&search_term)
                || a.description.to_lowercase().contains(&search_term)
                || a.bindings
                    .iter()
                    .any(|b| b.sequence_display.to_lowercase().contains(&search_term))
        })
        .collect();

    let total_actions = actions.len();
    let total_bindings: usize = actions.iter().map(|a| a.bindings.len()).sum();

    rsx! {
        TailwindStyle {}

        div { class: "w-full h-full flex flex-col bg-zinc-900 text-zinc-100 text-xs select-none overflow-hidden",

            // Header
            div { class: "flex items-center gap-3 px-3 py-2 border-b border-zinc-800",
                span { class: "text-zinc-500 uppercase tracking-wider text-xs flex-none", "Actions" }
                span { class: "text-zinc-500 text-xs",
                    "{total_actions} actions · {total_bindings} bindings"
                }
                if let Some(ref msg) = *status_msg.read() {
                    span { class: "text-green-400 text-xs", "{msg}" }
                }
                div { class: "ml-auto flex items-center gap-2 flex-none",
                    button {
                        class: "bg-blue-600 hover:bg-blue-500 text-white text-xs px-2.5 py-1 rounded cursor-pointer",
                        onclick: {
                            let section_files = section_files.clone();
                            move |_| {
                                let default_section = section_files.first()
                                    .cloned()
                                    .unwrap_or_else(|| "views.styx".into());
                                let mut s = AddBindingState::new_for(String::new(), default_section);
                                s.show_action_picker = true;
                                add_dialog.set(Some(s));
                            }
                        },
                        "+ New binding"
                    }
                    input {
                        class: "bg-zinc-800 border border-zinc-700 rounded px-2 py-1 text-xs text-zinc-200 w-48",
                        placeholder: "Search actions…",
                        value: "{search}",
                        oninput: move |e| search.set(e.value()),
                    }
                }
            }

            // Context selector + Section filter
            div { class: "flex items-center gap-2 px-3 py-1.5 border-b border-zinc-800/50 flex-wrap",
                // Context selector
                span { class: "text-zinc-500 text-xs uppercase tracking-wider", "Context:" }
                for ctx in BindingContext::all() {
                    {
                        let label = ctx.label();
                        let is_active = *active_context.read() == label;
                        let cls = if is_active {
                            "bg-blue-600 text-white px-2 py-0.5 rounded text-xs cursor-pointer"
                        } else {
                            "bg-zinc-800 text-zinc-400 px-2 py-0.5 rounded text-xs cursor-pointer hover:bg-zinc-700"
                        };
                        rsx! {
                            span {
                                class: "{cls}",
                                onclick: move |_| active_context.set(label.to_string()),
                                "{label}"
                            }
                        }
                    }
                }
                span {
                    class: if *active_context.read() == "All" {
                        "bg-blue-600 text-white px-2 py-0.5 rounded text-xs cursor-pointer"
                    } else {
                        "bg-zinc-800 text-zinc-400 px-2 py-0.5 rounded text-xs cursor-pointer hover:bg-zinc-700"
                    },
                    onclick: move |_| active_context.set("All".to_string()),
                    "All"
                }

                // Separator
                div { class: "w-px h-4 bg-zinc-700 mx-1" }

                // Section chips
                for section in ActionSection::all() {
                    div { class: "flex items-center gap-1 px-1.5 py-0.5 rounded text-xs cursor-pointer hover:bg-zinc-700/50",
                        div { class: "w-2 h-2 rounded-full {section.dot_class()}" }
                        span { class: "text-zinc-400", "{section.label()}" }
                    }
                }
            }

            // Add Binding Dialog (overlay when active)
            if let Some(ref state) = *add_dialog.read() {
                AddBindingDialog {
                    state: state.clone(),
                    section_files: section_files.clone(),
                    actions: actions.clone(),
                    on_change: move |new_state: AddBindingState| {
                        add_dialog.set(Some(new_state));
                    },
                    on_save: move |state: AddBindingState| {
                        let verb = if state.editing_keys.is_some() { "Updated" } else { "Added" };
                        match save_new_binding(&state) {
                            Ok(()) => {
                                status_msg.set(Some(format!("{verb} {} → {}", state.keys, state.action_id)));
                                add_dialog.set(None);
                            }
                            Err(e) => {
                                status_msg.set(Some(format!("Error: {e}")));
                            }
                        }
                    },
                    on_cancel: move |_| {
                        add_dialog.set(None);
                    },
                }
            }

            // Action list
            div { class: "flex-1 min-h-0 overflow-y-auto",
                for action in filtered.iter() {
                    ActionRow {
                        action: (*action).clone(),
                        selected: selected_action.read().as_deref() == Some(&action.action_id),
                        section_files: section_files.clone(),
                        on_select: {
                            let id = action.action_id.clone();
                            move |_| {
                                let current = selected_action.read().clone();
                                if current.as_deref() == Some(id.as_str()) {
                                    selected_action.set(None);
                                } else {
                                    selected_action.set(Some(id.clone()));
                                }
                            }
                        },
                        on_status: move |msg: String| {
                            status_msg.set(Some(msg));
                        },
                        on_add_binding: {
                            let section_files = section_files.clone();
                            move |action_id: String| {
                                let default_section = section_files.first()
                                    .cloned()
                                    .unwrap_or_else(|| "views.styx".into());
                                add_dialog.set(Some(AddBindingState::new_for(action_id, default_section)));
                            }
                        },
                        on_edit_binding: {
                            let section_files = section_files.clone();
                            move |req: EditRequest| {
                                let default_section = section_files.first()
                                    .cloned()
                                    .unwrap_or_else(|| "views.styx".into());
                                let section = find_section_for_keys(&req.keys)
                                    .unwrap_or(default_section);
                                let (context, passthrough) = binding_meta_for_keys(&req.keys);
                                add_dialog.set(Some(AddBindingState {
                                    action_id: req.action_id,
                                    keys: req.keys.clone(),
                                    desc: req.desc,
                                    section_file: section,
                                    action_search: String::new(),
                                    show_action_picker: false,
                                    context,
                                    passthrough,
                                    editing_keys: Some(req.keys),
                                }));
                            }
                        },
                    }
                }

                if filtered.is_empty() {
                    div { class: "flex items-center justify-center py-8 text-zinc-500 italic",
                        "No actions match your search"
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Action row
// ---------------------------------------------------------------------------

#[component]
fn ActionRow(
    action: ActionEntry,
    selected: bool,
    section_files: Vec<String>,
    on_select: EventHandler<()>,
    on_status: EventHandler<String>,
    on_add_binding: EventHandler<String>,
    on_edit_binding: EventHandler<EditRequest>,
) -> Element {
    let bg = if selected { "bg-zinc-800" } else { "" };
    let dot = action.section.dot_class();

    rsx! {
        div {
            class: "flex flex-col {bg} border-b border-zinc-800/50 cursor-pointer hover:bg-zinc-800/50",
            onclick: move |_| on_select.call(()),

            div { class: "flex items-center gap-2 px-3 py-1.5",
                div { class: "w-2 h-2 rounded-full flex-none {dot}" }
                div { class: "flex-1 min-w-0",
                    span { class: "font-mono text-zinc-200 text-xs", "{action.action_id}" }
                    if !action.description.is_empty() && action.description != action.action_id {
                        span { class: "text-zinc-500 ml-2 text-xs truncate", "{action.description}" }
                    }
                }
                div { class: "flex gap-1 flex-none",
                    for binding in action.bindings.iter() {
                        span { class: "bg-zinc-700 rounded px-1.5 py-0.5 font-mono text-xs text-zinc-300",
                            "{binding.sequence_display}"
                        }
                    }
                    if action.bindings.is_empty() {
                        span { class: "text-zinc-500 italic text-xs", "no binding" }
                    }
                }
            }

            if selected {
                ActionDetail {
                    action: action.clone(),
                    on_status: on_status,
                    on_add_binding: on_add_binding,
                    on_edit_binding: on_edit_binding,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Action detail
// ---------------------------------------------------------------------------

#[component]
fn ActionDetail(
    action: ActionEntry,
    on_status: EventHandler<String>,
    on_add_binding: EventHandler<String>,
    on_edit_binding: EventHandler<EditRequest>,
) -> Element {
    let section_text = action.section.text_class();
    let action_id_for_add = action.action_id.clone();

    rsx! {
        div { class: "bg-zinc-800/50 border-t border-zinc-700/50 px-4 py-2",

            div { class: "flex items-center gap-2 pb-2",
                span { class: "text-zinc-500 text-xs uppercase tracking-wider", "Action" }
                span { class: "font-mono {section_text} text-xs", "{action.action_id}" }
                span { class: "text-zinc-500 text-xs", "·" }
                span { class: "text-zinc-400 text-xs", "{action.section.label()}" }
                if !action.description.is_empty() {
                    span { class: "text-zinc-500 text-xs", "·" }
                    span { class: "text-zinc-300 text-xs", "{action.description}" }
                }
            }

            // Bindings with delete
            div { class: "flex flex-col gap-1 pb-2",
                span { class: "text-zinc-500 text-xs uppercase tracking-wider pb-1", "Key Bindings" }
                for binding in action.bindings.iter() {
                    {
                        let keys = binding.raw_keys.clone().unwrap_or(binding.sequence_display.clone());
                        let action_id = action.action_id.clone();
                        let keys_for_delete = keys.clone();
                        let keys_for_edit = keys.clone();
                        let action_for_edit = action.action_id.clone();
                        let desc_for_edit = action.description.clone();
                        rsx! {
                            div { class: "flex items-center gap-2 px-2 py-1 rounded hover:bg-zinc-700/30",
                                span { class: "font-mono font-bold text-zinc-200 text-xs", "{binding.sequence_display}" }
                                if let Some(ref src) = binding.source_file {
                                    span { class: "text-zinc-500 text-xs ml-2", "{src}" }
                                }
                                span {
                                    class: "ml-auto text-zinc-500 text-xs cursor-pointer hover:text-blue-400 px-1",
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        on_edit_binding.call(EditRequest {
                                            keys: keys_for_edit.clone(),
                                            action_id: action_for_edit.clone(),
                                            desc: desc_for_edit.clone(),
                                        });
                                    },
                                    "✎ edit"
                                }
                                span {
                                    class: "text-zinc-500 text-xs cursor-pointer hover:text-rose-400 px-1",
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        delete_binding(&keys_for_delete, &action_id);
                                        on_status.call(format!("Removed {keys_for_delete}"));
                                    },
                                    "✕ remove"
                                }
                            }
                        }
                    }
                }
                if action.bindings.is_empty() {
                    div { class: "text-zinc-500 italic text-xs px-2", "No bindings assigned" }
                }
            }

            // Add binding button
            div { class: "flex items-center gap-2 pt-2 border-t border-zinc-700/50",
                span {
                    class: "text-blue-400 text-xs cursor-pointer hover:text-blue-300",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_add_binding.call(action_id_for_add.clone());
                    },
                    "+ Add binding"
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Add Binding Dialog
// ---------------------------------------------------------------------------

#[component]
fn AddBindingDialog(
    state: AddBindingState,
    section_files: Vec<String>,
    actions: Vec<ActionEntry>,
    on_change: EventHandler<AddBindingState>,
    on_save: EventHandler<AddBindingState>,
    on_cancel: EventHandler<()>,
) -> Element {
    let is_edit = state.editing_keys.is_some();
    let can_save = !state.keys.trim().is_empty() && !state.action_id.trim().is_empty();
    let title = if is_edit {
        "Edit Binding"
    } else {
        "Add Binding"
    };

    // Duplicate-key conflict: same keys already bound to a *different* action
    // (ignoring the binding currently being edited).
    let conflict: Option<String> = if state.keys.trim().is_empty() {
        None
    } else {
        existing_key_bindings().into_iter().find_map(|(k, act)| {
            let editing_this = state.editing_keys.as_deref() == Some(k.as_str());
            (k == state.keys && act != state.action_id && !editing_this).then_some(act)
        })
    };

    // Action picker filtered list.
    let picker_query = state.action_search.to_lowercase();
    let picker_matches: Vec<&ActionEntry> = if state.show_action_picker {
        actions
            .iter()
            .filter(|a| {
                picker_query.is_empty()
                    || a.action_id.to_lowercase().contains(&picker_query)
                    || a.description.to_lowercase().contains(&picker_query)
            })
            .take(80)
            .collect()
    } else {
        Vec::new()
    };

    let action_label = if state.action_id.is_empty() {
        "(choose an action)".to_string()
    } else {
        state.action_id.clone()
    };

    rsx! {
        div { class: "bg-zinc-800 border border-blue-700/50 rounded-lg mx-3 my-2 p-4",

            // Title
            div { class: "flex items-center gap-2 pb-3 border-b border-zinc-700",
                span { class: "text-zinc-200 text-sm font-bold", "{title}" }
            }

            div { class: "flex flex-col gap-3 py-3",

                // Action selector / picker
                div { class: "flex flex-col gap-1",
                    label { class: "text-zinc-400 text-xs uppercase tracking-wider", "Action" }
                    div {
                        class: "flex items-center gap-2 bg-zinc-900 border border-zinc-600 rounded px-2 py-1.5 cursor-pointer hover:border-zinc-500",
                        onclick: {
                            let state = state.clone();
                            move |_| {
                                let mut s = state.clone();
                                s.show_action_picker = !s.show_action_picker;
                                on_change.call(s);
                            }
                        },
                        span { class: "font-mono text-sm flex-1 text-blue-400", "{action_label}" }
                        span { class: "text-zinc-500 text-xs", if state.show_action_picker { "▲" } else { "▼ change" } }
                    }

                    if state.show_action_picker {
                        div { class: "flex flex-col gap-1 mt-1 border border-zinc-700 rounded p-2 bg-zinc-900/60",
                            input {
                                class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-xs text-zinc-200 w-full",
                                placeholder: "Search actions…",
                                value: "{state.action_search}",
                                oninput: {
                                    let state = state.clone();
                                    move |e| {
                                        let mut s = state.clone();
                                        s.action_search = e.value();
                                        on_change.call(s);
                                    }
                                },
                            }
                            div { class: "max-h-48 overflow-y-auto flex flex-col",
                                for a in picker_matches.iter() {
                                    {
                                        let chosen = a.action_id.clone();
                                        let desc = a.description.clone();
                                        let dot = a.section.dot_class();
                                        rsx! {
                                            div {
                                                class: "flex items-center gap-2 px-2 py-1 rounded hover:bg-zinc-700/60 cursor-pointer",
                                                onclick: {
                                                    let state = state.clone();
                                                    let chosen = chosen.clone();
                                                    move |e| {
                                                        e.stop_propagation();
                                                        let mut s = state.clone();
                                                        s.action_id = chosen.clone();
                                                        s.show_action_picker = false;
                                                        on_change.call(s);
                                                    }
                                                },
                                                div { class: "w-2 h-2 rounded-full flex-none {dot}" }
                                                span { class: "font-mono text-xs text-zinc-200", "{chosen}" }
                                                if !desc.is_empty() && desc != chosen {
                                                    span { class: "text-zinc-500 text-xs truncate", "{desc}" }
                                                }
                                            }
                                        }
                                    }
                                }
                                if picker_matches.is_empty() {
                                    span { class: "text-zinc-500 text-xs italic px-2 py-1",
                                        "No actions match — type an action ID directly below"
                                    }
                                }
                            }
                            // Allow typing a raw action ID (e.g. a REAPER command number).
                            input {
                                class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-xs text-zinc-200 font-mono w-full mt-1",
                                placeholder: "…or enter a raw action ID (e.g. 40026, _FTS_…)",
                                value: "{state.action_id}",
                                oninput: {
                                    let state = state.clone();
                                    move |e| {
                                        let mut s = state.clone();
                                        s.action_id = e.value();
                                        on_change.call(s);
                                    }
                                },
                            }
                        }
                    }
                }

                // Key sequence input
                div { class: "flex flex-col gap-1",
                    label { class: "text-zinc-400 text-xs uppercase tracking-wider", "Key Sequence" }
                    input {
                        class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1.5 text-sm text-zinc-200 font-mono w-full",
                        placeholder: "e.g. <C-s>, h, gg, <S-Tab>",
                        value: "{state.keys}",
                        oninput: {
                            let state = state.clone();
                            move |e| {
                                let mut s = state.clone();
                                s.keys = e.value();
                                on_change.call(s);
                            }
                        },
                        onkeydown: {
                            let state = state.clone();
                            move |e: KeyboardEvent| {
                                let key = e.key();
                                if matches!(key, Key::Shift | Key::Control | Key::Alt | Key::Meta) {
                                    return;
                                }
                                let mods = e.modifiers();
                                let captured = format_captured_key(&key, &mods);
                                if !captured.is_empty() {
                                    let mut s = state.clone();
                                    s.keys = captured;
                                    on_change.call(s);
                                }
                            }
                        },
                    }
                    if let Some(other) = conflict {
                        span { class: "text-amber-400 text-xs",
                            "⚠ {state.keys} is already bound to {other} — saving will add a duplicate"
                        }
                    } else {
                        span { class: "text-zinc-500 text-xs",
                            "Type styx format or press a key combo (Ctrl+S, etc.)"
                        }
                    }
                }

                // Description input
                div { class: "flex flex-col gap-1",
                    label { class: "text-zinc-400 text-xs uppercase tracking-wider", "Description" }
                    input {
                        class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1.5 text-sm text-zinc-200 w-full",
                        placeholder: "Human-readable description",
                        value: "{state.desc}",
                        oninput: {
                            let state = state.clone();
                            move |e| {
                                let mut s = state.clone();
                                s.desc = e.value();
                                on_change.call(s);
                            }
                        },
                    }
                }

                // Section + context row
                div { class: "flex gap-3",
                    div { class: "flex flex-col gap-1 flex-1",
                        label { class: "text-zinc-400 text-xs uppercase tracking-wider", "Save to Section" }
                        select {
                            class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1.5 text-sm text-zinc-200 w-full appearance-none",
                            value: "{state.section_file}",
                            onchange: {
                                let state = state.clone();
                                move |e| {
                                    let mut s = state.clone();
                                    s.section_file = e.value();
                                    on_change.call(s);
                                }
                            },
                            for file in section_files.iter() {
                                option { value: "{file}", "{file}" }
                            }
                        }
                    }
                    div { class: "flex flex-col gap-1 flex-1",
                        label { class: "text-zinc-400 text-xs uppercase tracking-wider", "Context" }
                        select {
                            class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1.5 text-sm text-zinc-200 w-full appearance-none",
                            value: "{state.context}",
                            onchange: {
                                let state = state.clone();
                                move |e| {
                                    let mut s = state.clone();
                                    s.context = e.value();
                                    on_change.call(s);
                                }
                            },
                            for ctx in CONTEXT_OPTIONS.iter() {
                                option { value: "{ctx}", "{ctx}" }
                            }
                        }
                    }
                }

                // Passthrough toggle (only meaningful in editor contexts)
                div {
                    class: "flex items-center gap-2 cursor-pointer",
                    onclick: {
                        let state = state.clone();
                        move |_| {
                            let mut s = state.clone();
                            s.passthrough = !s.passthrough;
                            on_change.call(s);
                        }
                    },
                    span {
                        class: if state.passthrough {
                            "w-4 h-4 rounded border border-blue-500 bg-blue-600 text-white flex items-center justify-center text-[10px]"
                        } else {
                            "w-4 h-4 rounded border border-zinc-600 bg-zinc-900"
                        },
                        if state.passthrough { "✓" }
                    }
                    span { class: "text-zinc-300 text-xs",
                        "Passthrough — run in the main section even from a special editor"
                    }
                }
            }

            // Buttons
            div { class: "flex items-center gap-2 pt-3 border-t border-zinc-700",
                button {
                    class: if can_save {
                        "bg-blue-600 hover:bg-blue-500 text-white text-xs px-3 py-1.5 rounded cursor-pointer"
                    } else {
                        "bg-zinc-700 text-zinc-500 text-xs px-3 py-1.5 rounded cursor-not-allowed"
                    },
                    disabled: !can_save,
                    onclick: {
                        let state = state.clone();
                        move |e| {
                            e.stop_propagation();
                            if can_save {
                                on_save.call(state.clone());
                            }
                        }
                    },
                    if is_edit { "Save Changes" } else { "Save Binding" }
                }
                button {
                    class: "text-zinc-400 text-xs px-3 py-1.5 rounded hover:bg-zinc-700 cursor-pointer",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_cancel.call(());
                    },
                    "Cancel"
                }
                if can_save {
                    span { class: "text-zinc-500 text-xs ml-auto",
                        "Preview: {state.keys} → {state.action_id}"
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Config operations
// ---------------------------------------------------------------------------

fn save_new_binding(state: &AddBindingState) -> Result<(), String> {
    if state.keys.trim().is_empty() {
        return Err("key sequence is empty".into());
    }
    if state.action_id.trim().is_empty() {
        return Err("no action selected".into());
    }

    let profile_dir = resolve_profile_dir(&state.section_file).ok_or_else(|| {
        format!(
            "Section file '{}' not found in any profile",
            state.section_file
        )
    })?;

    // Edit mode: remove the original binding first (it may live in a different
    // section than the new target).
    if let Some(orig_keys) = &state.editing_keys
        && let Ok(profile) = ProfileEditor::open(&profile_dir)
        && let Some(orig_section) = profile.find_binding_section(orig_keys)
    {
        let mut ed = profile
            .open_section(&orig_section)
            .map_err(|e| format!("{e}"))?;
        ed.remove_binding(orig_keys);
        ed.save().map_err(|e| format!("{e}"))?;
    }

    let section_path = profile_dir.join(&state.section_file);
    let mut editor = SectionEditor::open(&section_path).map_err(|e| format!("{e}"))?;
    editor.add_binding(KeybindDef {
        keys: state.keys.clone(),
        action: state.action_id.clone(),
        desc: if state.desc.is_empty() {
            None
        } else {
            Some(state.desc.clone())
        },
        context: context_label_to_styx(&state.context),
        passthrough: state.passthrough.then_some(true),
        mnemonic: None,
        why: None,
    });
    editor.save_and_reload().map_err(|e| format!("{e}"))?;
    tracing::info!(
        keys = state.keys.as_str(),
        action = state.action_id.as_str(),
        section = state.section_file.as_str(),
        edit = state.editing_keys.is_some(),
        "Saved binding"
    );
    Ok(())
}

/// Resolve the profile directory to write to: the focused profile when one is
/// selected, otherwise the first profile directory that contains `section_file`.
fn resolve_profile_dir(section_file: &str) -> Option<std::path::PathBuf> {
    if let Some(dir) = super::source::active_profile_dir() {
        return Some(dir);
    }
    let keybinds = get_config_dir().join("keybinds");
    std::fs::read_dir(keybinds)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir() && p.join(section_file).exists())
}

/// Profile directories to search for an existing binding (focused profile, or
/// all profiles when none is focused).
fn search_profile_dirs() -> Vec<std::path::PathBuf> {
    if let Some(dir) = super::source::active_profile_dir() {
        return vec![dir];
    }
    std::fs::read_dir(get_config_dir().join("keybinds"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

/// Find which section file currently holds the binding for `keys`.
fn find_section_for_keys(keys: &str) -> Option<String> {
    for dir in search_profile_dirs() {
        if let Ok(profile) = ProfileEditor::open(&dir)
            && let Some(section) = profile.find_binding_section(keys)
        {
            return Some(section);
        }
    }
    None
}

/// Read the styx context label and passthrough flag of the binding for `keys`.
/// Defaults to `("Global", false)` when not found.
fn binding_meta_for_keys(keys: &str) -> (String, bool) {
    use crate::input::keybinds::KeybindContext;
    for dir in search_profile_dirs() {
        if let Ok(profile) = ProfileEditor::open(&dir) {
            for (_section, b) in profile.all_bindings() {
                if b.keys == keys {
                    let ctx = match b.context {
                        Some(KeybindContext::Main) => "Main",
                        Some(KeybindContext::Midi) => "MIDI Editor",
                        Some(KeybindContext::MidiInline) => "MIDI Inline",
                        Some(KeybindContext::MediaExplorer) => "Media Explorer",
                        _ => "Global",
                    };
                    return (ctx.to_string(), b.passthrough.unwrap_or(false));
                }
            }
        }
    }
    ("Global".to_string(), false)
}

/// Keys already bound in the searched profile(s), with their action — for
/// duplicate-key conflict warnings in the dialog.
fn existing_key_bindings() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for dir in search_profile_dirs() {
        if let Ok(profile) = ProfileEditor::open(&dir) {
            for (_section, b) in profile.all_bindings() {
                out.push((b.keys.clone(), b.action.clone()));
            }
        }
    }
    out
}

fn delete_binding(keys: &str, _action_id: &str) {
    let config_dir = get_config_dir();

    if let Ok(entries) = std::fs::read_dir(config_dir.join("keybinds")) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            if let Ok(profile) = ProfileEditor::open(entry.path())
                && let Some(section_name) = profile.find_binding_section(keys)
                && let Ok(mut editor) = profile.open_section(&section_name)
                && editor.remove_binding(keys).is_some()
            {
                if let Err(e) = editor.save_and_reload() {
                    tracing::error!("Failed to save after delete: {e}");
                } else {
                    tracing::info!(keys, section = section_name.as_str(), "Deleted binding");
                    return;
                }
            }
        }
    }
    tracing::warn!(keys, "Binding not found in any config file");
}

/// Convert a captured keyboard event into styx key format.
fn format_captured_key(key: &Key, mods: &Modifiers) -> String {
    let key_str = match key {
        Key::Character(c) => c.to_lowercase(),
        Key::Escape => "Esc".into(),
        Key::Enter => "Enter".into(),
        Key::Tab => "Tab".into(),
        Key::Backspace => "BS".into(),
        Key::Delete => "Del".into(),
        Key::ArrowUp => "Up".into(),
        Key::ArrowDown => "Down".into(),
        Key::ArrowLeft => "Left".into(),
        Key::ArrowRight => "Right".into(),
        Key::Home => "Home".into(),
        Key::End => "End".into(),
        Key::PageUp => "PgUp".into(),
        Key::PageDown => "PgDn".into(),
        Key::F1 => "F1".into(),
        Key::F2 => "F2".into(),
        Key::F3 => "F3".into(),
        Key::F4 => "F4".into(),
        Key::F5 => "F5".into(),
        Key::F6 => "F6".into(),
        Key::F7 => "F7".into(),
        Key::F8 => "F8".into(),
        Key::F9 => "F9".into(),
        Key::F10 => "F10".into(),
        Key::F11 => "F11".into(),
        Key::F12 => "F12".into(),
        _ => return String::new(),
    };

    let has_mods = mods.contains(Modifiers::CONTROL)
        || mods.contains(Modifiers::SHIFT)
        || mods.contains(Modifiers::ALT)
        || mods.contains(Modifiers::META);

    if !has_mods && key_str.len() == 1 {
        // Simple character key, no modifiers
        return key_str;
    }

    // Build <C-S-A-M-key> format
    let mut result = String::from("<");
    if mods.contains(Modifiers::CONTROL) {
        result.push_str("C-");
    }
    if mods.contains(Modifiers::SHIFT) {
        result.push_str("S-");
    }
    if mods.contains(Modifiers::ALT) {
        result.push_str("A-");
    }
    if mods.contains(Modifiers::META) {
        result.push_str("M-");
    }
    result.push_str(&key_str);
    result.push('>');
    result
}

fn get_config_dir() -> std::path::PathBuf {
    super::source::config_dir()
}

fn get_section_files() -> Vec<String> {
    let config_dir = get_config_dir();
    let mut files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(config_dir.join("keybinds")) {
        for entry in entries.flatten() {
            if entry.path().is_dir()
                && let Ok(profile) = ProfileEditor::open(entry.path())
            {
                for name in profile.section_names() {
                    if !files.contains(name) {
                        files.push(name.clone());
                    }
                }
            }
        }
    }

    if files.is_empty() {
        files.push("views.styx".into());
    }
    files
}
