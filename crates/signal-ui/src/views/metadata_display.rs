//! Metadata display component.
//!
//! Renders tags, description, and notes from domain entities that implement
//! the `HasMetadata` trait. Used by collection cards, editors, and browsers.

use dioxus::prelude::*;

/// Displays metadata (tags, description, notes) for any entity.
///
/// Pass pre-extracted metadata fields rather than the entity itself,
/// keeping this component usable with any domain type.
#[component]
pub fn MetadataDisplay(
    /// Tag strings to show as pills.
    #[props(default)]
    tags: Vec<String>,
    /// Optional description text.
    #[props(default)]
    description: Option<String>,
    /// Optional notes text.
    #[props(default)]
    notes: Option<String>,
    /// Whether to show section labels. Default: true.
    #[props(default = true)]
    show_labels: bool,
) -> Element {
    let has_tags = !tags.is_empty();
    let has_description = description.as_ref().is_some_and(|d| !d.is_empty());
    let has_notes = notes.as_ref().is_some_and(|n| !n.is_empty());
    let has_anything = has_tags || has_description || has_notes;

    if !has_anything {
        return rsx! {
            div { class: "text-xs text-zinc-500 italic py-1", "No metadata" }
        };
    }

    rsx! {
        div { class: "space-y-2 text-sm",
            // Tags
            if has_tags {
                div {
                    if show_labels {
                        span { class: "text-xs font-semibold text-zinc-400 uppercase tracking-wider mr-2",
                            "Tags"
                        }
                    }
                    div { class: "flex flex-wrap gap-1.5 mt-1",
                        for tag in tags.iter() {
                            span {
                                key: "{tag}",
                                class: "text-xs px-2 py-0.5 rounded-full bg-zinc-700/80 text-zinc-300",
                                "{tag}"
                            }
                        }
                    }
                }
            }

            // Description
            if has_description {
                div {
                    if show_labels {
                        div { class: "text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-0.5",
                            "Description"
                        }
                    }
                    p { class: "text-zinc-300 text-sm leading-relaxed",
                        {description.as_deref().unwrap_or_default()}
                    }
                }
            }

            // Notes
            if has_notes {
                div {
                    if show_labels {
                        div { class: "text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-0.5",
                            "Notes"
                        }
                    }
                    p { class: "text-zinc-400 text-xs leading-relaxed italic",
                        {notes.as_deref().unwrap_or_default()}
                    }
                }
            }
        }
    }
}
