//! Preset bar — screenset selector with save/load and persistence.

use crate::hooks::use_dock_actions;
use crate::prelude::*;
use crate::signals::*;

/// Screenset selector bar showing available presets with save/load controls.
#[component]
pub fn PresetBar() -> Element {
    let presets = DOCK_PRESETS.read();
    let active_index = *DOCK_ACTIVE_PRESET_INDEX.read();
    let actions = use_dock_actions();
    let mut show_save_dialog = use_signal(|| false);
    let mut new_preset_name = use_signal(String::new);

    rsx! {
        div {
            class: "flex items-center gap-1.5 px-3 py-1.5 bg-background border-b border-border text-xs relative",

            // Label
            span {
                class: "text-muted-foreground font-medium mr-1",
                "Layouts"
            }

            // Preset buttons
            for (i, preset) in presets.presets.iter().enumerate() {
                {
                    let is_active = i == active_index;
                    let bg = if is_active {
                        "bg-blue-600/90 text-white shadow-sm"
                    } else {
                        "bg-muted text-muted-foreground hover:bg-accent hover:text-foreground"
                    };
                    let hotkey = preset.hotkey_hint.clone();
                    let name = preset.name.clone();
                    rsx! {
                        button {
                            class: "px-2.5 py-1 rounded-md {bg} transition-all duration-150",
                            title: if let Some(ref hk) = hotkey {
                                format!("{name} ({hk})")
                            } else {
                                name.clone()
                            },
                            onclick: {
                                let actions = actions.clone();
                                move |_| {
                                    actions.load_preset.call(i);
                                }
                            },
                            if let Some(ref hk) = hotkey {
                                span {
                                    class: "mr-1 text-[10px] opacity-60",
                                    "{hk}"
                                }
                            }
                            "{name}"
                        }
                    }
                }
            }

            // Separator
            div { class: "w-px h-4 bg-border mx-1" }

            // Save button (saves current layout to active preset)
            button {
                class: "px-2 py-1 rounded-md bg-muted text-muted-foreground hover:bg-accent hover:text-foreground transition-colors",
                title: "Save current layout to active preset",
                onclick: {
                    let actions = actions.clone();
                    move |_| {
                        actions.save_current_preset.call(());
                        actions.persist_presets.call(());
                    }
                },
                "Save"
            }

            // Save As button
            button {
                class: "px-2 py-1 rounded-md bg-muted text-muted-foreground hover:bg-accent hover:text-foreground transition-colors",
                title: "Save layout as new preset",
                onclick: move |_| {
                    show_save_dialog.set(true);
                    new_preset_name.set(String::new());
                },
                "Save As"
            }

            // Prev/Next cycle buttons
            div { class: "w-px h-4 bg-border mx-1" }

            button {
                class: "px-1.5 py-1 rounded-md bg-muted text-muted-foreground hover:bg-accent hover:text-foreground transition-colors",
                title: "Previous preset",
                onclick: move |_| {
                    let (len, current) = {
                        let presets = DOCK_PRESETS.read();
                        (presets.presets.len(), presets.active_index)
                    };
                    if len == 0 {
                        return;
                    }
                    let idx = if current == 0 { len - 1 } else { current - 1 };
                    actions.load_preset.call(idx);
                },
                "\u{25C0}" // left triangle
            }
            button {
                class: "px-1.5 py-1 rounded-md bg-muted text-muted-foreground hover:bg-accent hover:text-foreground transition-colors",
                title: "Next preset",
                onclick: move |_| {
                    let (len, current) = {
                        let presets = DOCK_PRESETS.read();
                        (presets.presets.len(), presets.active_index)
                    };
                    if len == 0 {
                        return;
                    }
                    let idx = (current + 1) % len;
                    actions.load_preset.call(idx);
                },
                "\u{25B6}" // right triangle
            }

            // Save-as dialog overlay
            if show_save_dialog() {
                SavePresetDialog {
                    name: new_preset_name(),
                    on_name_change: move |n: String| new_preset_name.set(n),
                    on_save: {
                        let actions = actions.clone();
                        move |name: String| {
                            if !name.is_empty() {
                                actions.save_as_new_preset.call(name);
                                actions.persist_presets.call(());
                            }
                            show_save_dialog.set(false);
                        }
                    },
                    on_cancel: move || show_save_dialog.set(false),
                }
            }
        }
    }
}

/// Inline save-as dialog.
#[component]
fn SavePresetDialog(
    name: String,
    on_name_change: EventHandler<String>,
    on_save: EventHandler<String>,
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 bg-black/30 z-40",
            onclick: move |_| on_cancel.call(()),
        }
        // Dialog
        div {
            class: "absolute left-1/2 top-full mt-2 -translate-x-1/2 z-50 bg-card border border-border rounded-lg shadow-xl p-4 min-w-[250px]",

            div {
                class: "text-foreground text-sm font-medium mb-3",
                "Save Layout As"
            }

            input {
                class: "w-full bg-muted border border-border rounded-md px-3 py-1.5 text-sm text-foreground placeholder-muted-foreground focus:outline-none focus:border-blue-500 mb-3",
                placeholder: "Preset name...",
                value: "{name}",
                autofocus: true,
                oninput: move |e: FormEvent| {
                    on_name_change.call(e.value());
                },
                onkeypress: {
                    let name = name.clone();
                    move |e: KeyboardEvent| {
                        if e.key() == Key::Enter {
                            on_save.call(name.clone());
                        }
                    }
                },
            }

            div {
                class: "flex justify-end gap-2",
                button {
                    class: "px-3 py-1 rounded-md bg-muted text-muted-foreground hover:bg-accent text-xs",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
                button {
                    class: "px-3 py-1 rounded-md bg-blue-600 text-white hover:bg-blue-500 text-xs",
                    onclick: {
                        let name = name.clone();
                        move |_| on_save.call(name.clone())
                    },
                    "Save"
                }
            }
        }
    }
}
