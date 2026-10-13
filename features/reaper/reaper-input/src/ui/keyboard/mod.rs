//! Keyboard visualizer dock panel.
//!
//! Renders a color-coded QWERTY keyboard showing current keybind assignments,
//! with a binding reference table below.

pub mod actions_list;
pub mod conflicts;
pub mod data;
pub mod editor_app;
pub mod editor_tabs;
pub mod layout;
pub mod source;

pub use editor_app::EditorApp;

use reaper_dioxus::prelude::*;

use architect_ui::prelude::{Kbd, ThemeMode, ThemeProvider, ThemeState, default_theme_preset};

use crate::ui::tailwind::TailwindStyle;
use data::{ActionSection, KeyBindingInfo, bindings_by_key, collect_current_bindings};
use layout::{KEY_GAP, KEY_UNIT, KeyBlock, KeyDef, KeyRow, qwerty_layout};

use crate::input::processor;

// ---------------------------------------------------------------------------
// Shortcut rendering via the architect-ui Kbd component
// ---------------------------------------------------------------------------

/// Render a key sequence (e.g. `<C-s>`, `g g`, `<S-Tab>`) as a row of architect-ui
/// [`Kbd`] badges. Multi-step sequences are separated by a thin gap.
#[component]
fn ShortcutKbd(seq: String) -> Element {
    let steps = shortcut_steps(&seq);
    rsx! {
        span { class: "inline-flex items-center gap-1.5 flex-wrap",
            for (si, step) in steps.iter().enumerate() {
                if si > 0 {
                    span { class: "text-muted-foreground", style: "font-size:9px;", "›" }
                }
                span { class: "inline-flex items-center gap-1",
                    for badge in step.iter() {
                        Kbd { "{badge}" }
                    }
                }
            }
        }
    }
}

/// Split a sequence into steps, each step into its key/modifier badge labels.
/// `"<C-s> g"` → `[["Ctrl","S"], ["G"]]`.
fn shortcut_steps(seq: &str) -> Vec<Vec<String>> {
    seq.split_whitespace().map(parse_chord).collect()
}

fn parse_chord(step: &str) -> Vec<String> {
    let bracketed = step.len() >= 2 && step.starts_with('<') && step.ends_with('>');
    let body = if bracketed {
        &step[1..step.len() - 1]
    } else {
        step
    };

    // Plain single character key (no modifiers).
    if !bracketed && body.chars().count() == 1 {
        return vec![display_key(body)];
    }

    let mut badges = Vec::new();
    let mut rest = body;
    // Peel leading "C-", "S-", "A-", "M-" modifier prefixes.
    while rest.len() >= 2
        && matches!(&rest[..1], "C" | "S" | "A" | "M")
        && rest.as_bytes()[1] == b'-'
    {
        badges.push(mod_name(&rest[..1]));
        rest = &rest[2..];
    }
    if !rest.is_empty() {
        badges.push(display_key(rest));
    }
    if badges.is_empty() {
        badges.push(display_key(body));
    }
    badges
}

fn mod_name(m: &str) -> String {
    match m {
        "C" => "Ctrl",
        "S" => "Shift",
        "A" => "Alt",
        "M" => "Cmd",
        other => other,
    }
    .to_string()
}

fn display_key(k: &str) -> String {
    match k.to_lowercase().as_str() {
        "space" => "Space".into(),
        "cr" | "enter" => "Enter".into(),
        "tab" => "Tab".into(),
        "esc" => "Esc".into(),
        "bs" => "Bksp".into(),
        "del" => "Del".into(),
        s if s.chars().count() == 1 => s.to_uppercase(),
        _ => k.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Root component
// ---------------------------------------------------------------------------

/// Root component for the keyboard visualizer dock panel.
#[component]
pub fn KeyboardPanel() -> Element {
    let modes = data::list_modes();
    let mut selected_mode = use_signal(|| None::<String>);

    // Standalone has no live processor — read bindings from the `.styx` files.
    let mut bindings = if source::is_standalone() {
        data::collect_bindings_with_sources()
    } else {
        collect_current_bindings()
    };
    // Overlay the selected mode's bindings on top of the base profile.
    let active_mode = selected_mode.read().clone();
    if let Some(ref mode) = active_mode {
        bindings.extend(data::collect_workflow_bindings(mode));
    }
    let by_key = bindings_by_key(&bindings);
    let blocks = qwerty_layout();
    let selected_key = use_signal(|| None::<String>);
    let mut search = use_signal(String::new);

    let profile = crate::current_profile()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| source::active_profile().unwrap_or_else(|| "—".into()));
    let preset = processor::active_preset_name();

    // Compute which keys match the search for highlighting
    let search_term = search.read().to_lowercase();
    let highlighted_keys: Vec<String> = if search_term.is_empty() {
        Vec::new()
    } else {
        by_key
            .iter()
            .filter(|(_, bindings)| {
                bindings.iter().any(|b| {
                    b.action_id.to_lowercase().contains(&search_term)
                        || b.description.to_lowercase().contains(&search_term)
                        || b.sequence_display.to_lowercase().contains(&search_term)
                })
            })
            .map(|(k, _)| k.clone())
            .collect()
    };

    let theme_state = use_signal(|| ThemeState::new(default_theme_preset(), ThemeMode::Dark));

    rsx! {
        TailwindStyle {}

        ThemeProvider { state: theme_state, class: "w-full h-full",
        div { class: "w-full h-full flex flex-col bg-zinc-900 text-zinc-100 text-xs select-none overflow-hidden",

            // Header
            div { class: "flex items-center gap-3 px-3 py-2 border-b border-zinc-800",
                span { class: "text-zinc-500 uppercase tracking-wider text-xs", "Keyboard" }
                span { class: "text-zinc-400", "{profile}" }
                span { class: "text-zinc-500", "·" }
                span { class: "text-zinc-400", "{preset}" }

                // Search
                div { class: "ml-auto flex-none",
                    input {
                        class: "bg-zinc-800 border border-zinc-700 rounded px-2 py-1 text-xs text-zinc-200 w-40",
                        placeholder: "Search bindings…",
                        value: "{search}",
                        oninput: move |e| search.set(e.value()),
                    }
                }

                // Section legend
                div { class: "flex gap-1.5 flex-wrap flex-none",
                    for section in ActionSection::all() {
                        SectionChip { section: *section }
                    }
                }
            }

            // Mode selector — overlay a session mode's bindings on the keyboard
            div { class: "flex items-center gap-1.5 px-3 py-1.5 border-b border-zinc-800 flex-wrap flex-none",
                span { class: "text-zinc-500 uppercase tracking-wider text-xs", "Mode" }
                {
                    let is_base = active_mode.is_none();
                    let cls = if is_base {
                        "bg-violet-500 text-white px-2.5 py-1 rounded text-xs cursor-pointer"
                    } else {
                        "bg-zinc-800 text-zinc-400 px-2.5 py-1 rounded text-xs cursor-pointer hover:bg-zinc-700"
                    };
                    rsx! {
                        span { class: "{cls}", onclick: move |_| selected_mode.set(None), "Base" }
                    }
                }
                for (file, display) in modes.iter() {
                    {
                        let file = file.clone();
                        let is_active = active_mode.as_deref() == Some(file.as_str());
                        let cls = if is_active {
                            "bg-violet-500 text-white px-2.5 py-1 rounded text-xs cursor-pointer"
                        } else {
                            "bg-zinc-800 text-zinc-400 px-2.5 py-1 rounded text-xs cursor-pointer hover:bg-zinc-700"
                        };
                        rsx! {
                            span {
                                class: "{cls}",
                                onclick: move |_| selected_mode.set(Some(file.clone())),
                                "{display}"
                            }
                        }
                    }
                }
            }

            // Keyboard + details + table
            div { class: "flex-1 flex flex-col min-h-0 overflow-auto p-3 gap-3",

                // Keyboard grid
                div { class: "flex gap-4 flex-none",
                    for block in blocks.iter() {
                        KeyBlockView {
                            block: block.clone(),
                            by_key: by_key.clone(),
                            selected_key: selected_key,
                            highlighted: highlighted_keys.clone(),
                        }
                    }
                }

                // Selected key details panel
                if let Some(ref key_label) = *selected_key.read() {
                    KeyDetailPanel {
                        key_label: key_label.clone(),
                        bindings: by_key.get(key_label).cloned().unwrap_or_default(),
                    }
                }

                // Binding reference table
                div { class: "flex-1 min-h-0 overflow-y-auto",
                    BindingTable { bindings: bindings.clone() }
                }
            }
        }
        }
    }
}

// ---------------------------------------------------------------------------
// Keyboard rendering
// ---------------------------------------------------------------------------

#[component]
fn KeyBlockView(
    block: KeyBlock,
    by_key: std::collections::HashMap<String, Vec<KeyBindingInfo>>,
    selected_key: Signal<Option<String>>,
    highlighted: Vec<String>,
) -> Element {
    rsx! {
        div { class: "flex flex-col gap-0.5",
            for row in block.rows.iter() {
                KeyRowView { row: row.clone(), by_key: by_key.clone(), selected_key, highlighted: highlighted.clone() }
            }
        }
    }
}

#[component]
fn KeyRowView(
    row: KeyRow,
    by_key: std::collections::HashMap<String, Vec<KeyBindingInfo>>,
    selected_key: Signal<Option<String>>,
    highlighted: Vec<String>,
) -> Element {
    let h = (KEY_UNIT * row.height) as u32;

    rsx! {
        div {
            class: "flex gap-0.5",
            style: "height: {h}px;",
            for key_def in row.keys.iter() {
                KeyCap { key_def: key_def.clone(), row_height: row.height, by_key: by_key.clone(), selected_key, highlighted: highlighted.clone() }
            }
        }
    }
}

#[component]
fn KeyCap(
    key_def: KeyDef,
    row_height: f64,
    by_key: std::collections::HashMap<String, Vec<KeyBindingInfo>>,
    mut selected_key: Signal<Option<String>>,
    highlighted: Vec<String>,
) -> Element {
    let w = ((KEY_UNIT * key_def.width) - KEY_GAP) as u32;
    let label = key_def.label;

    // Is this a spacer?
    if key_def.code.is_none() {
        return rsx! {
            div { style: "width: {w}px;", class: "flex-none" }
        };
    }

    // Look up all bindings for this key.
    let key_lookup = label.to_lowercase();
    let empty = Vec::new();
    let bindings = by_key.get(&key_lookup).unwrap_or(&empty);
    let has_bindings = !bindings.is_empty();

    // Tint the cap by the first binding's section; keep a neutral cap otherwise.
    let bg_class = bindings
        .first()
        .map(|b| b.section.bg_class())
        .unwrap_or("bg-zinc-800");

    let is_selected = selected_key.read().as_deref() == Some(&key_lookup);
    let ring = if is_selected {
        "ring-2 ring-blue-400"
    } else {
        ""
    };

    // Search highlighting: dim non-matching keys.
    let is_searching = !highlighted.is_empty();
    let is_highlighted = highlighted.contains(&key_lookup);
    let opacity = if is_searching && !is_highlighted {
        "opacity-30"
    } else {
        ""
    };

    let count = bindings.len();
    let key_for_click = key_lookup.clone();
    rsx! {
        div {
            class: "flex-none flex flex-col rounded border border-zinc-700/50 cursor-pointer overflow-hidden {bg_class} {ring} {opacity}",
            style: "width: {w}px;",
            onclick: move |_| {
                let current = selected_key.read().clone();
                if current.as_deref() == Some(key_for_click.as_str()) {
                    selected_key.set(None);
                } else {
                    selected_key.set(Some(key_for_click.clone()));
                }
            },

            // Header strip: key label + binding count
            div { class: "flex items-center justify-between px-1.5 pt-1 flex-none",
                span { class: "font-bold text-zinc-100 leading-none", style: "font-size: 13px;", "{label}" }
                if count > 1 {
                    span { class: "text-zinc-400 leading-none", style: "font-size: 9px;", "{count}" }
                }
            }

            // Binding list — one line per binding on this key.
            if has_bindings {
                div { class: "flex-1 min-h-0 overflow-hidden flex flex-col px-1 pb-1 pt-1",
                    for b in bindings.iter() {
                        BindingChip { binding: b.clone() }
                    }
                }
            }
        }
    }
}

/// One binding line inside a key cap: its full chord + description.
#[component]
fn BindingChip(binding: KeyBindingInfo) -> Element {
    let seq = binding.sequence_display.clone();
    let desc = if binding.description.is_empty() {
        binding.action_id.clone()
    } else {
        binding.description.clone()
    };
    let dot = binding.section.dot_class();
    let text = binding.section.text_class();
    rsx! {
        div { class: "flex items-center gap-1 leading-tight",
            div { class: "rounded-full flex-none {dot}", style: "width:5px;height:5px;" }
            span { class: "font-mono text-zinc-300 flex-none", style: "font-size: 8px;", "{seq}" }
            span { class: "truncate {text}", style: "font-size: 8px;", "{desc}" }
        }
    }
}

// ---------------------------------------------------------------------------
// Key detail / edit panel
// ---------------------------------------------------------------------------

/// Shows details for a selected key and allows editing its binding.
#[component]
fn KeyDetailPanel(key_label: String, bindings: Vec<KeyBindingInfo>) -> Element {
    rsx! {
        div { class: "bg-zinc-800 border border-zinc-700 rounded p-3",
            div { class: "flex items-center gap-3 pb-2 border-b border-zinc-700/50",
                // Key badge
                div { class: "bg-zinc-700 rounded px-3 py-1 font-mono font-bold text-zinc-100 text-sm",
                    "{key_label}"
                }

                if bindings.is_empty() {
                    span { class: "text-zinc-500 italic", "No bindings" }
                } else {
                    {
                        let count = bindings.len();
                        let suffix = if count != 1 { "s" } else { "" };
                        rsx! { span { class: "text-zinc-400", "{count} binding{suffix}" } }
                    }
                }
            }

            // Binding list for this key
            if !bindings.is_empty() {
                div { class: "flex flex-col gap-1 py-2",
                    for binding in bindings.iter() {
                        div { class: "flex items-center gap-2 px-2 py-1 rounded hover:bg-zinc-700/50",
                            div { class: "w-2 h-2 rounded-full flex-none {binding.section.dot_class()}" }
                            ShortcutKbd { seq: binding.sequence_display.clone() }
                            span { class: "text-zinc-500", "→" }
                            span { class: "flex-1 truncate {binding.section.text_class()} text-xs",
                                "{binding.description}"
                            }
                            span { class: "text-zinc-500 text-xs font-mono",
                                "{binding.action_id}"
                            }
                            if binding.is_prefix {
                                span { class: "text-zinc-500 text-xs", "prefix" }
                            }
                        }
                    }
                }
            }

            // Edit hint
            div { class: "flex items-center gap-2 pt-2 border-t border-zinc-700/50 text-zinc-500",
                span { class: "text-xs italic",
                    "Use the Actions tab to add, edit, or remove bindings."
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Section filter chips
// ---------------------------------------------------------------------------

#[component]
fn SectionChip(section: ActionSection) -> Element {
    rsx! {
        div { class: "flex items-center gap-1 px-1.5 py-0.5 rounded text-xs cursor-pointer hover:bg-zinc-700/50",
            div { class: "w-2 h-2 rounded-full {section.dot_class()}" }
            span { class: "text-zinc-400", "{section.label()}" }
        }
    }
}

// ---------------------------------------------------------------------------
// Binding reference table
// ---------------------------------------------------------------------------

#[component]
fn BindingTable(bindings: Vec<KeyBindingInfo>) -> Element {
    rsx! {
        div { class: "border border-zinc-800 rounded overflow-hidden",
            // Header
            div { class: "flex bg-zinc-800/50 px-3 py-1.5 border-b border-zinc-800 text-zinc-500 uppercase tracking-wider",
                span { class: "w-8 flex-none", "#" }
                div { class: "flex-1", "Key" }
                div { class: "flex-1", "Action" }
                div { class: "flex-none w-8", "" }
            }

            // Rows
            for (i, binding) in bindings.iter().enumerate() {
                BindingRow { index: i, binding: binding.clone() }
            }
        }
    }
}

#[component]
fn BindingRow(index: usize, binding: KeyBindingInfo) -> Element {
    let border = if index > 0 {
        "border-t border-zinc-800/50"
    } else {
        ""
    };
    let dot = binding.section.dot_class();
    let text = binding.section.text_class();

    rsx! {
        div { class: "flex items-center px-3 py-1 hover:bg-zinc-800/50 {border}",
            // Row number
            span { class: "w-8 flex-none text-zinc-500", "{index + 1}" }

            // Key sequence
            div { class: "flex-1 flex items-center gap-1.5",
                ShortcutKbd { seq: binding.sequence_display.clone() }
                if binding.is_prefix {
                    span { class: "text-zinc-500 ml-1", "+" }
                }
            }

            // Action / description
            div { class: "flex-1 flex items-center gap-1.5",
                div { class: "w-2 h-2 rounded-full flex-none {dot}" }
                span { class: "truncate {text}", "{binding.description}" }
            }
        }
    }
}
