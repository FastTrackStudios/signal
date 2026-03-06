//! Toolbar components: search, sort, tag filter panel, active tag chips.

use dioxus::prelude::*;
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
        div { class: "px-3 py-1.5 border-b border-white/[0.06] flex items-center gap-2 flex-shrink-0 bg-zinc-950/60",
            // Search input
            div { class: "flex items-center gap-1.5 flex-1 min-w-0",
                span { class: "text-zinc-500 text-xs flex-shrink-0", ">" }
                input {
                    class: "bg-transparent text-xs text-zinc-200 outline-none flex-1 min-w-0 placeholder-zinc-600",
                    r#type: "text",
                    placeholder: "Search {props.current_nav.label().to_ascii_lowercase()}...",
                    value: "{props.current_search}",
                    oninput: move |evt: Event<FormData>| {
                        props.on_search_change.call(evt.value().clone());
                    },
                }
                if has_active_filters {
                    button {
                        class: "text-[10px] text-zinc-500 hover:text-zinc-300 px-1",
                        onclick: move |_| {
                            props.on_search_change.call(String::new());
                            props.on_filters_change.call(Vec::new());
                        },
                        "Clear"
                    }
                }
            }
            // Sort dropdown
            select {
                class: "px-1.5 py-0.5 text-[10px] rounded bg-white/[0.06] text-zinc-300 border border-white/[0.08] outline-none cursor-pointer flex-shrink-0",
                value: "{props.current_sort.value()}",
                onchange: move |evt: Event<FormData>| {
                    props.on_sort_change.call(SortMode::from_value(&evt.value()));
                },
                for sm in SortMode::ALL.iter() {
                    {
                        let s = *sm;
                        rsx! {
                            option {
                                value: "{s.value()}",
                                selected: props.current_sort == s,
                                "{s.label()}"
                            }
                        }
                    }
                }
            }
            // Tag filter toggle
            button {
                class: if props.tag_panel_open {
                    "px-2 py-0.5 text-[10px] rounded bg-white/[0.15] text-zinc-100 flex-shrink-0"
                } else {
                    "px-2 py-0.5 text-[10px] rounded bg-white/[0.06] text-zinc-400 hover:text-zinc-200 hover:bg-white/[0.10] flex-shrink-0"
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
            div { class: "px-3 py-1 border-b border-white/[0.06] flex items-center gap-1 flex-shrink-0 flex-wrap bg-zinc-950/40",
                for filter_key in props.active_filters.iter() {
                    {
                        let key = filter_key.clone();
                        let display = tag_display_value(&key).to_string();
                        let current_filters = props.active_filters.clone();
                        rsx! {
                            button {
                                key: "{key}",
                                class: "inline-flex items-center gap-1 px-1.5 py-0.5 text-[10px] rounded bg-white/[0.08] text-zinc-200 hover:bg-white/[0.12]",
                                onclick: move |_| {
                                    let mut filters = current_filters.clone();
                                    filters.retain(|f| f != &key);
                                    props.on_filters_change.call(filters);
                                },
                                "{display}"
                                span { class: "text-zinc-400", "x" }
                            }
                        }
                    }
                }
            }
        }

        // ── Tag filter panel (collapsible) ──
        if props.tag_panel_open {
            div { class: "px-3 py-2 border-b border-white/[0.06] flex-shrink-0 bg-zinc-900/40 max-h-40 overflow-y-auto",
                if props.available_tags.is_empty() {
                    div { class: "text-xs text-zinc-600 italic", "No tags available" }
                }
                for (cat, keys) in props.available_tags.iter() {
                    {
                        let cat_label = tag_category_label(*cat);
                        rsx! {
                            div { class: "mb-1.5",
                                h4 { class: "text-[10px] font-semibold text-zinc-500 uppercase tracking-wider mb-0.5", "{cat_label}" }
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
                                                    class: if is_active {
                                                        "px-1.5 py-0.5 text-[10px] rounded bg-white/[0.15] text-zinc-100"
                                                    } else {
                                                        "px-1.5 py-0.5 text-[10px] rounded bg-white/[0.06] text-zinc-400 hover:bg-white/[0.10] hover:text-zinc-200"
                                                    },
                                                    onclick: move |_| {
                                                        let mut filters = current_filters.clone();
                                                        if is_active {
                                                            filters.retain(|f| f != &k);
                                                        } else {
                                                            filters.push(k.clone());
                                                        }
                                                        props.on_filters_change.call(filters);
                                                    },
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

// endregion: --- Toolbar
