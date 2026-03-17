//! Profile Editor — list + detail view for editing profiles and their patches.
//!
//! Left panel: profile list with selection. Right panel: patch editor showing
//! the patches in the selected profile with drag-to-reorder and override editing.

use dioxus::prelude::*;

/// A profile entry for the list panel.
#[derive(Clone, PartialEq)]
pub struct ProfileListEntry {
    pub id: String,
    pub name: String,
    pub patch_count: usize,
    pub tags: Vec<String>,
}

/// A patch entry within a profile.
#[derive(Clone, PartialEq)]
pub struct PatchEntry {
    pub id: String,
    pub name: String,
    pub rig_scene_name: Option<String>,
    pub override_count: usize,
}

/// A parameter override in a patch.
#[derive(Clone, PartialEq)]
pub struct OverrideEntry {
    pub param_name: String,
    pub inherited_value: String,
    pub override_value: String,
}

/// Profile list panel (left side).
#[derive(Props, Clone, PartialEq)]
pub struct ProfileListProps {
    /// Available profiles.
    profiles: Vec<ProfileListEntry>,

    /// Currently selected profile ID.
    #[props(default)]
    selected_id: Option<String>,

    /// Callback when a profile is selected.
    #[props(default)]
    on_select: Option<Callback<String>>,

    /// Callback to create a new profile.
    #[props(default)]
    on_create: Option<Callback<()>>,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn ProfileList(props: ProfileListProps) -> Element {
    rsx! {
        div {
            class: format!("flex flex-col gap-1 {}", props.class),

            // Header with create button
            div {
                class: "flex items-center justify-between px-2 py-1",
                h3 {
                    class: "text-xs uppercase tracking-widest text-muted-foreground font-semibold",
                    "Profiles"
                }
                button {
                    class: "px-2 py-0.5 text-xs rounded bg-primary text-primary-foreground hover:bg-primary/90",
                    onclick: move |_| {
                        if let Some(cb) = &props.on_create {
                            cb.call(());
                        }
                    },
                    "+ New"
                }
            }

            // Profile list
            div {
                class: "flex flex-col gap-0.5 overflow-y-auto",
                for profile in props.profiles.iter() {
                    {
                        let id = profile.id.clone();
                        let is_selected = props.selected_id.as_ref() == Some(&profile.id);
                        let selected_class = if is_selected {
                            "bg-accent text-accent-foreground"
                        } else {
                            "hover:bg-muted"
                        };
                        rsx! {
                            button {
                                class: format!("flex items-center justify-between px-2 py-1.5 rounded text-left transition-colors {selected_class}"),
                                onclick: move |_| {
                                    if let Some(cb) = &props.on_select {
                                        cb.call(id.clone());
                                    }
                                },
                                div {
                                    class: "flex flex-col",
                                    span {
                                        class: "text-xs font-medium",
                                        "{profile.name}"
                                    }
                                    span {
                                        class: "text-[10px] text-muted-foreground",
                                        "{profile.patch_count} patches"
                                    }
                                }
                                if !profile.tags.is_empty() {
                                    div {
                                        class: "flex gap-0.5",
                                        for tag in profile.tags.iter().take(2) {
                                            span {
                                                class: "px-1 py-0.5 rounded text-[9px] bg-muted text-muted-foreground",
                                                "{tag}"
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

/// Patch editor panel (right side).
#[derive(Props, Clone, PartialEq)]
pub struct PatchEditorProps {
    /// Profile name (header).
    profile_name: String,

    /// Patches in this profile.
    patches: Vec<PatchEntry>,

    /// Currently selected patch ID.
    #[props(default)]
    selected_patch_id: Option<String>,

    /// Parameter overrides for the selected patch.
    #[props(default)]
    overrides: Vec<OverrideEntry>,

    /// Callback when a patch is selected.
    #[props(default)]
    on_select_patch: Option<Callback<String>>,

    /// Callback to reorder patches (new ordered IDs).
    #[props(default)]
    on_reorder: Option<Callback<Vec<String>>>,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn PatchEditor(props: PatchEditorProps) -> Element {
    rsx! {
        div {
            class: format!("flex flex-col gap-3 p-4 {}", props.class),

            // Header
            h3 {
                class: "text-xs uppercase tracking-widest text-muted-foreground font-semibold",
                "{props.profile_name}"
            }

            // Patch list
            div {
                class: "flex flex-col gap-1",
                for (i, patch) in props.patches.iter().enumerate() {
                    {
                        let patch_id = patch.id.clone();
                        let is_selected = props.selected_patch_id.as_ref() == Some(&patch.id);
                        let selected_class = if is_selected {
                            "border-primary bg-primary/5"
                        } else {
                            "border-border hover:border-border/80"
                        };
                        rsx! {
                            div {
                                class: format!(
                                    "flex items-center gap-2 px-3 py-2 rounded border cursor-pointer transition-colors {selected_class}"
                                ),
                                onclick: move |_| {
                                    if let Some(cb) = &props.on_select_patch {
                                        cb.call(patch_id.clone());
                                    }
                                },

                                // Order number
                                span {
                                    class: "text-xs text-muted-foreground w-4 text-right",
                                    {(i + 1).to_string()}
                                }

                                // Patch info
                                div {
                                    class: "flex-1",
                                    div {
                                        class: "text-xs font-medium",
                                        "{patch.name}"
                                    }
                                    if let Some(scene) = &patch.rig_scene_name {
                                        div {
                                            class: "text-[10px] text-muted-foreground",
                                            "Scene: {scene}"
                                        }
                                    }
                                }

                                // Override count
                                if patch.override_count > 0 {
                                    span {
                                        class: "px-1.5 py-0.5 rounded text-[10px] bg-accent text-accent-foreground",
                                        "{patch.override_count} overrides"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Override details for selected patch
            if !props.overrides.is_empty() {
                div {
                    class: "border-t border-border pt-3",
                    h4 {
                        class: "text-xs font-semibold mb-2",
                        "Parameter Overrides"
                    }
                    div {
                        class: "flex flex-col gap-1",
                        for ovr in props.overrides.iter() {
                            div {
                                class: "flex items-center justify-between px-2 py-1 rounded text-xs hover:bg-muted",
                                span {
                                    class: "font-medium",
                                    "{ovr.param_name}"
                                }
                                div {
                                    class: "flex items-center gap-2",
                                    span {
                                        class: "text-muted-foreground line-through",
                                        "{ovr.inherited_value}"
                                    }
                                    span { "\u{2192}" }
                                    span {
                                        class: "text-primary",
                                        "{ovr.override_value}"
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
