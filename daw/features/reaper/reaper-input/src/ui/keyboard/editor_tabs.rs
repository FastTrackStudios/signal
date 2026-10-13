//! Editor tabs beyond the keyboard/actions views: profile management,
//! wheel & mouse-modifier bindings, which-key trees, and workflows.
//!
//! All tabs operate on the focused profile (see [`super::source`]). Reads come
//! straight from the `.styx` files each render; writes go through the
//! [`editor`](crate::keybind_config::editor) types and hot-reload.

use std::path::PathBuf;

use reaper_dioxus::prelude::*;

use super::source;
use crate::keybind_config::editor::{ProfileEditor, ProfileMetaEditor, create_profile};
use crate::keybind_config::types::{MouseBindDef, WheelBindDef};

/// Shown when no profile is focused.
#[component]
fn NoProfile() -> Element {
    rsx! {
        div { class: "w-full h-full flex items-center justify-center text-zinc-500 text-xs italic",
            "Select a profile to edit"
        }
    }
}

fn active_dir() -> Option<PathBuf> {
    source::active_profile_dir()
}

fn bump(mut rev: Signal<u32>) {
    let n = rev();
    rev.set(n + 1);
}

/// Open the focused profile's metadata, apply `f`, save, and refresh.
fn apply_meta(
    mut status: Signal<Option<String>>,
    rev: Signal<u32>,
    f: impl FnOnce(&mut ProfileMetaEditor),
) {
    let Some(dir) = active_dir() else { return };
    match ProfileMetaEditor::open(&dir) {
        Ok(mut m) => {
            f(&mut m);
            match m.save() {
                Ok(()) => status.set(Some("Saved".into())),
                Err(e) => status.set(Some(format!("Error: {e}"))),
            }
            bump(rev);
        }
        Err(e) => status.set(Some(format!("Error: {e}"))),
    }
}

/// Remove a wheel binding from `section` in the focused profile.
fn remove_wheel_binding(
    mut status: Signal<Option<String>>,
    rev: Signal<u32>,
    section: String,
    index: usize,
) {
    if let Some(dir) = active_dir()
        && let Ok(profile) = ProfileEditor::open(&dir)
        && let Ok(mut ed) = profile.open_section(&section)
    {
        ed.remove_wheel(index);
        let _ = ed.save_and_reload();
        status.set(Some("Removed wheel binding".into()));
        bump(rev);
    }
}

/// Remove a mouse-modifier binding from `section` in the focused profile.
fn remove_mouse_binding(
    mut status: Signal<Option<String>>,
    rev: Signal<u32>,
    section: String,
    index: usize,
) {
    if let Some(dir) = active_dir()
        && let Ok(profile) = ProfileEditor::open(&dir)
        && let Ok(mut ed) = profile.open_section(&section)
    {
        ed.remove_mouse(index);
        let _ = ed.save_and_reload();
        status.set(Some("Removed mouse binding".into()));
        bump(rev);
    }
}

// ===========================================================================
// Profile tab
// ===========================================================================

#[component]
pub fn ProfileTab() -> Element {
    let rev = use_signal(|| 0u32);
    let _ = rev(); // re-read on mutation
    let mut status = use_signal(|| None::<String>);
    let mut new_id = use_signal(String::new);
    let mut new_name = use_signal(String::new);
    let mut new_section = use_signal(String::new);

    let Some(dir) = active_dir() else {
        return rsx! { NoProfile {} };
    };

    let Ok(meta) = ProfileMetaEditor::open(&dir) else {
        return rsx! {
            div { class: "p-4 text-rose-400 text-xs", "Failed to read profile.styx in {dir.display()}" }
        };
    };

    let sections = meta.config.sections.clone();
    let name = meta.config.name.clone();
    let description = meta.config.description.clone();
    let version = meta.config.version.clone();
    let keybinds_dir = source::keybinds_dir();

    rsx! {
        div { class: "w-full h-full overflow-y-auto p-4 flex flex-col gap-4 text-xs",
            if let Some(ref m) = *status.read() {
                div { class: "text-green-400", "{m}" }
            }

            // Metadata
            div { class: "flex flex-col gap-2",
                span { class: "text-zinc-500 uppercase tracking-wider", "Profile Metadata" }
                LabeledInput {
                    label: "Display name", value: name,
                    on_commit: move |v: String| apply_meta(status, rev, move |m| m.set_name(v)),
                }
                LabeledInput {
                    label: "Description", value: description,
                    on_commit: move |v: String| apply_meta(status, rev, move |m| m.set_description(v)),
                }
                LabeledInput {
                    label: "Version", value: version,
                    on_commit: move |v: String| apply_meta(status, rev, move |m| m.set_version(v)),
                }
            }

            // Sections (load order)
            div { class: "flex flex-col gap-1",
                span { class: "text-zinc-500 uppercase tracking-wider", "Sections (load order)" }
                for (i, sec) in sections.iter().enumerate() {
                    {
                        let sec = sec.clone();
                        let n = sections.len();
                        rsx! {
                            div { class: "flex items-center gap-2 px-2 py-1 bg-zinc-800/50 rounded",
                                span { class: "text-zinc-500 w-5", "{i + 1}" }
                                span { class: "font-mono text-zinc-200 flex-1", "{sec}" }
                                button {
                                    class: "text-zinc-500 hover:text-zinc-200 px-1 disabled:opacity-30",
                                    disabled: i == 0,
                                    onclick: move |_| apply_meta(status, rev, move |m| m.move_section(i, -1)),
                                    "▲"
                                }
                                button {
                                    class: "text-zinc-500 hover:text-zinc-200 px-1 disabled:opacity-30",
                                    disabled: i + 1 >= n,
                                    onclick: move |_| apply_meta(status, rev, move |m| m.move_section(i, 1)),
                                    "▼"
                                }
                                button {
                                    class: "text-zinc-500 hover:text-rose-400 px-1",
                                    onclick: {
                                        let sec = sec.clone();
                                        move |_| {
                                            let sec = sec.clone();
                                            apply_meta(status, rev, move |m| m.remove_section(&sec))
                                        }
                                    },
                                    "✕"
                                }
                            }
                        }
                    }
                }
                div { class: "flex items-center gap-2 mt-1",
                    input {
                        class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-zinc-200 font-mono flex-1",
                        placeholder: "new-section.styx",
                        value: "{new_section}",
                        oninput: move |e| new_section.set(e.value()),
                    }
                    button {
                        class: "bg-zinc-700 hover:bg-zinc-600 text-zinc-100 px-2 py-1 rounded",
                        onclick: move |_| {
                            let v = new_section.read().trim().to_string();
                            if !v.is_empty() {
                                apply_meta(status, rev, move |m| m.add_section(v));
                                new_section.set(String::new());
                            }
                        },
                        "+ Add section"
                    }
                }
            }

            // Create profile
            div { class: "flex flex-col gap-1 border-t border-zinc-800 pt-3",
                span { class: "text-zinc-500 uppercase tracking-wider", "Create New Profile" }
                div { class: "flex items-center gap-2",
                    input {
                        class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-zinc-200 font-mono w-40",
                        placeholder: "id (dir name)",
                        value: "{new_id}",
                        oninput: move |e| new_id.set(e.value()),
                    }
                    input {
                        class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-zinc-200 flex-1",
                        placeholder: "Display name",
                        value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()),
                    }
                    button {
                        class: "bg-blue-600 hover:bg-blue-500 text-white px-2 py-1 rounded",
                        onclick: move |_| {
                            let id = new_id.read().trim().to_string();
                            let nm = new_name.read().trim().to_string();
                            if id.is_empty() {
                                status.set(Some("Enter an id".into()));
                                return;
                            }
                            let nm = if nm.is_empty() { id.clone() } else { nm };
                            match create_profile(&keybinds_dir, &id, &nm) {
                                Ok(_) => {
                                    status.set(Some(format!("Created '{id}' — select it from the profile dropdown")));
                                    new_id.set(String::new());
                                    new_name.set(String::new());
                                }
                                Err(e) => status.set(Some(format!("Error: {e}"))),
                            }
                        },
                        "Create"
                    }
                }
            }
        }
    }
}

#[component]
fn LabeledInput(label: String, value: String, on_commit: EventHandler<String>) -> Element {
    // The parent tab is keyed by profile, so this remounts (re-seeding `draft`)
    // whenever the focused profile changes.
    let mut draft = use_signal(|| value.clone());
    rsx! {
        div { class: "flex items-center gap-2",
            span { class: "text-zinc-400 w-28", "{label}" }
            input {
                class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-zinc-200 flex-1",
                value: "{draft}",
                oninput: move |e| draft.set(e.value()),
                onchange: move |e| on_commit.call(e.value()),
            }
        }
    }
}

// ===========================================================================
// Wheel & mouse tab
// ===========================================================================

#[component]
pub fn WheelMouseTab() -> Element {
    let rev = use_signal(|| 0u32);
    let _ = rev();
    let mut status = use_signal(|| None::<String>);

    let Some(dir) = active_dir() else {
        return rsx! { NoProfile {} };
    };
    let Ok(profile) = ProfileEditor::open(&dir) else {
        return rsx! { div { class: "p-4 text-rose-400 text-xs", "Failed to read profile" } };
    };

    // Collect (section, index, bind) tuples across all sections.
    let mut wheel: Vec<(String, usize, WheelBindDef)> = Vec::new();
    let mut mouse: Vec<(String, usize, MouseBindDef)> = Vec::new();
    for sec in profile.section_names() {
        if let Ok(ed) = profile.open_section(sec) {
            for (i, w) in ed.wheel().iter().enumerate() {
                wheel.push((sec.clone(), i, w.clone()));
            }
            for (i, m) in ed.mouse().iter().enumerate() {
                mouse.push((sec.clone(), i, m.clone()));
            }
        }
    }

    let section_names: Vec<String> = profile.section_names().to_vec();

    rsx! {
        div { class: "w-full h-full overflow-y-auto p-4 flex flex-col gap-4 text-xs",
            if let Some(ref m) = *status.read() {
                div { class: "text-green-400", "{m}" }
            }

            // Wheel bindings
            div { class: "flex flex-col gap-1",
                span { class: "text-zinc-500 uppercase tracking-wider", "Mouse-Wheel Bindings" }
                for (section, index, w) in wheel.iter() {
                    {
                        let section = section.clone();
                        let index = *index;
                        let dir = if w.horizontal.unwrap_or(false) { "horiz" } else { "vert" };
                        let mods = if w.modifiers.is_empty() { "(none)".to_string() } else { w.modifiers.clone() };
                        let desc = w.desc.clone().unwrap_or_default();
                        let action = w.action.clone();
                        rsx! {
                            div { class: "flex items-center gap-2 px-2 py-1 bg-zinc-800/50 rounded",
                                span { class: "font-mono text-zinc-300 w-20", "{mods}" }
                                span { class: "text-zinc-500 w-12", "{dir}" }
                                span { class: "font-mono text-blue-400 flex-1", "{action}" }
                                span { class: "text-zinc-500 truncate flex-1", "{desc}" }
                                span { class: "text-zinc-600 w-24 truncate", "{section}" }
                                button {
                                    class: "text-zinc-500 hover:text-rose-400 px-1",
                                    onclick: {
                                        let section = section.clone();
                                        move |_| remove_wheel_binding(status, rev, section.clone(), index)
                                    },
                                    "✕"
                                }
                            }
                        }
                    }
                }
                if wheel.is_empty() {
                    span { class: "text-zinc-600 italic px-2 py-1", "No wheel bindings" }
                }
                AddWheelForm {
                    sections: section_names.clone(),
                    on_added: move |_| { status.set(Some("Added wheel binding".into())); bump(rev); },
                }
            }

            // Mouse modifier bindings
            div { class: "flex flex-col gap-1 border-t border-zinc-800 pt-3",
                span { class: "text-zinc-500 uppercase tracking-wider", "Mouse Modifier Bindings" }
                for (section, index, m) in mouse.iter() {
                    {
                        let section = section.clone();
                        let index = *index;
                        let ctx = format!("{:?}", m.ctx);
                        let mods = if m.modifiers.is_empty() { "(none)".to_string() } else { m.modifiers.clone() };
                        let action = m.action.clone();
                        let desc = m.desc.clone().unwrap_or_default();
                        rsx! {
                            div { class: "flex items-center gap-2 px-2 py-1 bg-zinc-800/50 rounded",
                                span { class: "font-mono text-zinc-400 w-40 truncate", "{ctx}" }
                                span { class: "font-mono text-zinc-300 w-20", "{mods}" }
                                span { class: "font-mono text-blue-400 flex-1", "{action}" }
                                span { class: "text-zinc-500 truncate flex-1", "{desc}" }
                                span { class: "text-zinc-600 w-24 truncate", "{section}" }
                                button {
                                    class: "text-zinc-500 hover:text-rose-400 px-1",
                                    onclick: {
                                        let section = section.clone();
                                        move |_| remove_mouse_binding(status, rev, section.clone(), index)
                                    },
                                    "✕"
                                }
                            }
                        }
                    }
                }
                if mouse.is_empty() {
                    span { class: "text-zinc-600 italic px-2 py-1", "No mouse modifier bindings" }
                }
                span { class: "text-zinc-600 italic px-2 pt-1",
                    "Mouse modifier contexts are added via the mouse-profile import (just mousemap)."
                }
            }
        }
    }
}

#[component]
fn AddWheelForm(sections: Vec<String>, on_added: EventHandler<()>) -> Element {
    let default_section = sections.first().cloned().unwrap_or_default();
    let mut modifiers = use_signal(String::new);
    let mut action = use_signal(String::new);
    let mut desc = use_signal(String::new);
    let mut horizontal = use_signal(|| false);
    let mut section = use_signal(|| default_section.clone());

    rsx! {
        div { class: "flex items-center gap-2 mt-1 flex-wrap",
            input {
                class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-zinc-200 font-mono w-24",
                placeholder: "mods (<C->)",
                value: "{modifiers}",
                oninput: move |e| modifiers.set(e.value()),
            }
            input {
                class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-zinc-200 font-mono w-28",
                placeholder: "action",
                value: "{action}",
                oninput: move |e| action.set(e.value()),
            }
            input {
                class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-zinc-200 flex-1",
                placeholder: "description",
                value: "{desc}",
                oninput: move |e| desc.set(e.value()),
            }
            span {
                class: "flex items-center gap-1 cursor-pointer text-zinc-400",
                onclick: move |_| { let v = *horizontal.read(); horizontal.set(!v); },
                span {
                    class: if *horizontal.read() {
                        "w-4 h-4 rounded border border-blue-500 bg-blue-600 text-white flex items-center justify-center text-[10px]"
                    } else {
                        "w-4 h-4 rounded border border-zinc-600 bg-zinc-900"
                    },
                    if *horizontal.read() { "✓" }
                }
                "horiz"
            }
            select {
                class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-zinc-200 appearance-none",
                value: "{section}",
                onchange: move |e| section.set(e.value()),
                for s in sections.iter() {
                    option { value: "{s}", "{s}" }
                }
            }
            button {
                class: "bg-blue-600 hover:bg-blue-500 text-white px-2 py-1 rounded",
                onclick: move |_| {
                    let act = action.read().trim().to_string();
                    if act.is_empty() { return; }
                    if let Some(dir) = active_dir()
                        && let Ok(profile) = ProfileEditor::open(&dir)
                        && let Ok(mut ed) = profile.open_section(&section.read())
                    {
                        ed.add_wheel(WheelBindDef {
                            modifiers: modifiers.read().clone(),
                            action: act,
                            horizontal: (*horizontal.read()).then_some(true),
                            desc: { let d = desc.read().clone(); (!d.is_empty()).then_some(d) },
                            context: None,
            mnemonic: None,
            why: None,
                        });
                        if ed.save_and_reload().is_ok() {
                            modifiers.set(String::new());
                            action.set(String::new());
                            desc.set(String::new());
                            on_added.call(());
                        }
                    }
                },
                "+ Add wheel"
            }
        }
    }
}

// ===========================================================================
// Which-key tab (read-only tree view)
// ===========================================================================

#[component]
pub fn WhichKeyTab() -> Element {
    let Some(dir) = active_dir() else {
        return rsx! { NoProfile {} };
    };
    let Ok(profile) = ProfileEditor::open(&dir) else {
        return rsx! { div { class: "p-4 text-rose-400 text-xs", "Failed to read profile" } };
    };

    let mut trees = Vec::new();
    for sec in profile.section_names() {
        if let Ok(ed) = profile.open_section(sec) {
            for t in ed.config.which_key() {
                trees.push((sec.clone(), t.clone()));
            }
        }
    }

    rsx! {
        div { class: "w-full h-full overflow-y-auto p-4 flex flex-col gap-3 text-xs",
            span { class: "text-zinc-500 uppercase tracking-wider", "Which-Key Trees" }
            for (section, tree) in trees.iter() {
                div { class: "border border-zinc-800 rounded p-2 flex flex-col gap-1",
                    div { class: "flex items-center gap-2",
                        span { class: "font-mono font-bold text-amber-400", "{tree.prefix}" }
                        span { class: "text-zinc-300", "{tree.label}" }
                        span { class: "text-zinc-600 ml-auto", "{section}" }
                    }
                    for entry in tree.entries.iter() {
                        WhichKeyEntryView { entry: entry.clone(), depth: 1 }
                    }
                }
            }
            if trees.is_empty() {
                span { class: "text-zinc-600 italic", "No which-key trees in this profile" }
            }
            span { class: "text-zinc-600 italic pt-2",
                "Tree editing is file-based for now — edit the section .styx directly."
            }
        }
    }
}

#[component]
fn WhichKeyEntryView(
    entry: crate::keybind_config::types::WhichKeyEntryDef,
    depth: usize,
) -> Element {
    let pad = format!("padding-left: {}px;", depth * 16);
    rsx! {
        div { class: "flex flex-col",
            div { class: "flex items-center gap-2", style: "{pad}",
                span { class: "font-mono text-zinc-300 w-10", "{entry.key}" }
                span { class: "text-zinc-400 flex-1", "{entry.label}" }
                if let Some(ref a) = entry.action {
                    span { class: "font-mono text-blue-400", "{a}" }
                }
            }
            if let Some(ref children) = entry.children {
                for child in children.iter() {
                    WhichKeyEntryView { entry: child.clone(), depth: depth + 1 }
                }
            }
        }
    }
}

// ===========================================================================
// Workflows tab (read-only list)
// ===========================================================================

#[component]
pub fn WorkflowsTab() -> Element {
    let workflows_dir = source::config_dir().join("workflows");
    let mut files: Vec<(String, String)> = Vec::new(); // (filename, contents)
    if let Ok(entries) = std::fs::read_dir(&workflows_dir) {
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "styx").unwrap_or(false))
            .collect();
        paths.sort();
        for p in paths {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let body = std::fs::read_to_string(&p).unwrap_or_default();
            files.push((name, body));
        }
    }

    rsx! {
        div { class: "w-full h-full overflow-y-auto p-4 flex flex-col gap-3 text-xs",
            span { class: "text-zinc-500 uppercase tracking-wider",
                "Workflows · {workflows_dir.display()}"
            }
            for (name, body) in files.iter() {
                details { class: "border border-zinc-800 rounded",
                    summary { class: "px-2 py-1 cursor-pointer font-mono text-zinc-200", "{name}" }
                    pre { class: "px-3 py-2 text-zinc-400 font-mono whitespace-pre-wrap overflow-x-auto bg-zinc-900/50",
                        "{body}"
                    }
                }
            }
            if files.is_empty() {
                span { class: "text-zinc-600 italic", "No workflow files found" }
            }
            span { class: "text-zinc-600 italic pt-2",
                "Workflow editing is file-based for now — edit the .styx directly; changes hot-reload."
            }
        }
    }
}
