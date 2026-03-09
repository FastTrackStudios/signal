//! Toolbar components: search, sort, tag filter panel, active tag chips.

use dioxus::prelude::*;
use fts_ui::prelude::*;
use signal::tagging::TagCategory;

use super::detail_panel::{tag_category_label, tag_display_value};
use super::types::{NavCategory, SortMode};

// region: --- Toolbar

#[derive(Props, Clone, PartialEq)]
pub(super) struct ToolbarProps {
    pub current_nav: NavCategory,
    pub current_search: String,
    pub current_sort: SortMode,
    pub tag_panel_open: bool,
    pub active_filters: Vec<String>,
    pub available_tags: Vec<(TagCategory, Vec<String>)>,
    pub on_search_change: EventHandler<String>,
    pub on_sort_change: EventHandler<SortMode>,
    pub on_toggle_tag_panel: EventHandler<()>,
    pub on_filters_change: EventHandler<Vec<String>>,
}

#[component]
pub(super) fn Toolbar(props: ToolbarProps) -> Element {
    let has_active_filters = !props.current_search.is_empty() || !props.active_filters.is_empty();

    rsx! {
        // ── Search + sort + filter toggle ──
        div {
            class: "px-3 py-1.5 flex items-center gap-2 flex-shrink-0 border-b border-border bg-card/50",
            // Search input
            div { class: "flex items-center gap-1.5 flex-1 min-w-0",
                span { class: "text-muted-foreground text-xs flex-shrink-0", ">" }
                input {
                    class: "bg-transparent text-xs text-foreground outline-none flex-1 min-w-0 placeholder-muted-foreground",
                    r#type: "text",
                    placeholder: "Search {props.current_nav.label().to_ascii_lowercase()}...",
                    value: "{props.current_search}",
                    oninput: move |evt: Event<FormData>| {
                        props.on_search_change.call(evt.value().clone());
                    },
                }
                if has_active_filters {
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Small,
                        on_click: move |_| {
                            props.on_search_change.call(String::new());
                            props.on_filters_change.call(Vec::new());
                        },
                        "Clear"
                    }
                }
            }
            // Sort picker
            SegmentedControl {
                value: props.current_sort.value().to_string(),
                on_change: move |v: String| {
                    props.on_sort_change.call(SortMode::from_value(&v));
                },
                options: SortMode::ALL.iter().map(|s| (s.value().to_string(), s.label().to_string())).collect::<Vec<_>>(),
                size: SegmentedControlSize::Small,
            }
            // Tag filter toggle
            button {
                class: if props.tag_panel_open {
                    "px-2 py-0.5 text-[10px] rounded flex-shrink-0 bg-secondary text-secondary-foreground"
                } else {
                    "px-2 py-0.5 text-[10px] rounded flex-shrink-0 bg-secondary/50 text-muted-foreground"
                },
                onclick: move |_| props.on_toggle_tag_panel.call(()),
                if props.active_filters.is_empty() {
                    "Tags"
                } else {
                    "Tags ({props.active_filters.len()})"
                }
            }
        }

        // ── Active tag chips ──
        if !props.active_filters.is_empty() {
            div { class: "px-3 py-1 flex items-center gap-1 flex-shrink-0 flex-wrap border-b border-border",
                for filter_key in props.active_filters.iter() {
                    {
                        let key = filter_key.clone();
                        let display = tag_display_value(&key).to_string();
                        let current_filters = props.active_filters.clone();
                        rsx! {
                            button {
                                key: "{key}",
                                onclick: move |_| {
                                    let mut filters = current_filters.clone();
                                    filters.retain(|f| f != &key);
                                    props.on_filters_change.call(filters);
                                },
                                Badge { variant: BadgeVariant::Secondary,
                                    "{display} "
                                    span { class: "text-muted-foreground", "x" }
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── Tag filter panel (collapsible) ──
        if props.tag_panel_open {
            div { class: "px-3 py-2 flex-shrink-0 max-h-40 overflow-y-auto border-b border-border bg-card/30",
                if props.available_tags.is_empty() {
                    div { class: "text-xs text-muted-foreground italic", "No tags available" }
                }
                for (cat, keys) in props.available_tags.iter() {
                    {
                        let cat_label = tag_category_label(*cat);
                        rsx! {
                            div { class: "mb-1.5",
                                h4 { class: "text-[10px] font-semibold text-muted-foreground uppercase tracking-wider mb-0.5", "{cat_label}" }
                                div { class: "flex flex-wrap gap-1",
                                    for key in keys.iter() {
                                        {
                                            let k = key.clone();
                                            let display = tag_display_value(key).to_string();
                                            let is_active = props.active_filters.contains(key);
                                            let current_filters = props.active_filters.clone();
                                            rsx! {
                                                button {
                                                    key: "{k}",
                                                    onclick: move |_| {
                                                        let mut filters = current_filters.clone();
                                                        if is_active {
                                                            filters.retain(|f| f != &k);
                                                        } else {
                                                            filters.push(k.clone());
                                                        }
                                                        props.on_filters_change.call(filters);
                                                    },
                                                    Badge {
                                                        variant: if is_active { BadgeVariant::Default } else { BadgeVariant::Secondary },
                                                        "{display}"
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

// endregion: --- Toolbar
