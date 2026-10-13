//! Dioxus component for the FTS Input Status dockable panel.
//!
//! Now uses Tailwind CSS classes via `TailwindStyle` instead of inline styles.

use reaper_dioxus::prelude::*;

use crate::input::processor;
use crate::input::workflows;
use crate::keybind_config::undo;
use crate::ui::tailwind::TailwindStyle;

// ---------------------------------------------------------------------------
// Root component
// ---------------------------------------------------------------------------

#[component]
pub fn InputStatusPanel() -> Element {
    rsx! {
        TailwindStyle {}

        div {
            class: "w-full h-full flex flex-col gap-1.5 p-3 overflow-y-auto select-none bg-zinc-900 text-zinc-100 text-xs",

            SystemToggles {}
            ProfileSection {}
            OverridesSection {}
            WorkflowSection {}
            KeySequenceSection {}
        }
    }
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

#[component]
fn SystemToggles() -> Element {
    let enabled = crate::is_enabled();
    let intercepting = crate::is_intercepting();
    let debug = crate::is_debug_logging();

    rsx! {
        Card { title: "System",
            div { class: "flex gap-4 flex-wrap",
                StatusBadge { label: "Input", active: enabled }
                StatusBadge { label: "Intercept", active: intercepting }
                StatusBadge { label: "Debug", active: debug }
            }
        }
    }
}

#[component]
fn ProfileSection() -> Element {
    let current = crate::current_profile()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "None".to_string());
    let preset = processor::active_preset_name();

    // Discover available profiles from the config directory
    let available_profiles = discover_profiles();

    rsx! {
        Card { title: "Profile",
            div { class: "flex flex-col gap-2",
                // Profile dropdown
                div { class: "flex items-center gap-2",
                    span { class: "text-zinc-500 text-xs", "Profile:" }
                    select {
                        class: "bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-xs text-zinc-200 appearance-none cursor-pointer flex-1",
                        value: "{current}",
                        onchange: move |e| {
                            let name = e.value();
                            let _ = match name.as_str() {
                                "FastTrackStudio" => crate::set_profile(crate::InputProfile::FastTrackStudio),
                                "Logic" => crate::set_profile(crate::InputProfile::Logic),
                                "ProTools" => crate::set_profile(crate::InputProfile::ProTools),
                                _ => Ok(()),
                            };
                        },
                        for profile in available_profiles.iter() {
                            option {
                                value: "{profile}",
                                selected: *profile == current,
                                "{profile}"
                            }
                        }
                    }
                }

                // Active preset
                LabelValue { label: "Active Preset", value: preset }

                // Undo/Redo
                if undo::can_undo() || undo::can_redo() {
                    div { class: "flex items-center gap-2 pt-1 border-t border-zinc-700/50",
                        if undo::can_undo() {
                            span {
                                class: "text-blue-400 text-xs cursor-pointer hover:text-blue-300",
                                onclick: |_| {
                                    if let Some(desc) = undo::undo() {
                                        tracing::info!("Undo: {desc}");
                                    }
                                },
                                "↩ Undo"
                            }
                            if let Some(desc) = undo::peek_undo() {
                                span { class: "text-zinc-500 text-xs", "{desc}" }
                            }
                        }
                        if undo::can_redo() {
                            span {
                                class: "text-blue-400 text-xs cursor-pointer hover:text-blue-300",
                                onclick: |_| {
                                    if let Some(desc) = undo::redo() {
                                        tracing::info!("Redo: {desc}");
                                    }
                                },
                                "↪ Redo"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Discover available profile directories from the config path.
fn discover_profiles() -> Vec<String> {
    use reaper_high::Reaper;

    let config_dir = std::env::var("FTS_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            Reaper::get()
                .resource_path()
                .join("fasttrackstudio/input")
                .into()
        });

    let mut profiles = Vec::new();
    if let Ok(entries) = std::fs::read_dir(config_dir.join("keybinds")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Check if it has a profile.styx
                if path.join("profile.styx").exists()
                    && let Some(name) = path.file_name().and_then(|n| n.to_str())
                {
                    profiles.push(name.to_string());
                }
            }
        }
    }
    profiles.sort();

    // Always include the built-in profiles even if not on disk
    for builtin in &["FastTrackStudio", "Logic", "ProTools"] {
        let lower = builtin.to_lowercase();
        if !profiles.iter().any(|p| p.to_lowercase() == lower) {
            profiles.push(builtin.to_string());
        }
    }

    profiles
}

#[component]
fn OverridesSection() -> Element {
    let overrides = processor::available_overrides();
    if overrides.is_empty() {
        return rsx! {};
    }

    rsx! {
        Card { title: "Overrides",
            div { class: "flex gap-2 flex-wrap",
                for name in overrides.iter() {
                    {
                        let active = processor::is_override_active(name);
                        rsx! { StatusBadge { label: "{name}", active: active } }
                    }
                }
            }
        }
    }
}

#[component]
fn WorkflowSection() -> Element {
    let active_name = workflows::get_active_workflow_display_name();
    let workflow_list = workflows::list_workflows();

    rsx! {
        Card { title: "Workflows",
            div { class: "flex flex-col gap-1",
                div { class: "flex gap-2 items-center",
                    span { class: "text-zinc-500 text-xs", "Active:" }
                    span {
                        class: if active_name.is_some() { "text-blue-400 font-bold" } else { "text-zinc-500" },
                        {active_name.as_deref().unwrap_or("None")}
                    }
                }
                if !workflow_list.is_empty() {
                    div { class: "flex gap-1.5 flex-wrap",
                        for (id, display_name, _desc) in workflow_list.iter() {
                            {
                                let active = workflows::active_workflow_name()
                                    .as_deref() == Some(id.as_str());
                                rsx! { StatusBadge { label: "{display_name}", active: active } }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn KeySequenceSection() -> Element {
    let pending = processor::pending_display();
    let timeout = processor::needs_timeout();

    rsx! {
        Card { title: "Key Sequence",
            div { class: "flex gap-4 items-center",
                div {
                    class: if pending.is_some() { "font-mono text-sm font-bold text-amber-400" } else { "font-mono text-sm font-bold text-zinc-500" },
                    {pending.as_deref().unwrap_or("—")}
                }
                if timeout {
                    span { class: "text-xs text-amber-400 px-2 py-0.5 rounded bg-amber-800/60",
                        "waiting…"
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reusable primitives
// ---------------------------------------------------------------------------

#[component]
fn Card(title: String, children: Element) -> Element {
    rsx! {
        div { class: "bg-zinc-800 border border-zinc-700 rounded px-3 py-2",
            div { class: "text-zinc-500 text-xs uppercase tracking-wider pb-1.5",
                "{title}"
            }
            {children}
        }
    }
}

#[component]
fn StatusBadge(label: String, active: bool) -> Element {
    let classes = if active {
        "flex items-center gap-1.5 px-2 py-0.5 rounded text-xs bg-green-800/60 text-green-400"
    } else {
        "flex items-center gap-1.5 px-2 py-0.5 rounded text-xs bg-zinc-700/60 text-zinc-500"
    };

    let dot = if active {
        "w-2 h-2 rounded-full bg-green-500"
    } else {
        "w-2 h-2 rounded-full bg-zinc-600"
    };

    rsx! {
        div { class: "{classes}",
            div { class: "{dot}" }
            "{label}"
        }
    }
}

#[component]
fn LabelValue(label: String, value: String) -> Element {
    rsx! {
        div { class: "flex flex-col gap-0.5",
            span { class: "text-zinc-500 text-xs uppercase tracking-wider", "{label}" }
            span { class: "text-zinc-100 text-sm font-medium", "{value}" }
        }
    }
}
