//! Detail panel rendering and helper functions.

use dioxus::prelude::*;
use signal::metadata::Metadata as MetadataModel;

use super::grid_conversion::{
    engines_to_grid_slots, module_chains_to_grid_slots, signal_chain_to_grid_slots, ParamLookup,
    RigGridPanel,
};
use super::types::{ColumnItem, DetailData, DetailParam, SortMode};

// region: --- Detail panel component

#[derive(Props, Clone, PartialEq)]
pub(super) struct DetailPanelProps {
    pub detail_name: Option<String>,
    pub detail_meta: Option<MetadataModel>,
    pub detail_data: Option<DetailData>,
    pub param_lookup: ParamLookup,
}

/// The right-side detail panel — fills all available space with the grid preview.
#[component]
pub(super) fn DetailPanel(props: DetailPanelProps) -> Element {
    rsx! {
        div { class: "flex-1 min-w-0 flex flex-col min-h-0",
            if let Some(ref data) = props.detail_data {
                // Rig-level: interactive DynamicGridView
                if !data.engines.is_empty() {
                    {
                        let grid_slots = engines_to_grid_slots(&data.engines, &props.param_lookup);
                        rsx! {
                            RigGridPanel { initial_slots: grid_slots }
                        }
                    }
                }
                // Module chains (layer/engine detail) — interactive grid
                if !data.module_chains.is_empty() {
                    {
                        let grid_slots = module_chains_to_grid_slots(&data.module_chains, &props.param_lookup);
                        rsx! {
                            RigGridPanel { initial_slots: grid_slots }
                        }
                    }
                }
                // Signal chain (module snapshot detail) — interactive grid
                if let Some(ref chain) = data.chain {
                    {
                        let name = props.detail_name.clone().unwrap_or_default();
                        let grid_slots = signal_chain_to_grid_slots(chain, &name, None, &props.param_lookup);
                        rsx! {
                            RigGridPanel { initial_slots: grid_slots }
                        }
                    }
                }
                // Flat params (block snapshot detail)
                if !data.params.is_empty() {
                    div { class: "p-4 space-y-2",
                        h4 { class: "text-[10px] font-semibold text-zinc-500 uppercase tracking-wider mb-2", "Parameters" }
                        { render_param_bars(&data.params) }
                    }
                }
            }
            if props.detail_name.is_none() {
                div { class: "flex-1 flex items-center justify-center",
                    span { class: "text-xs text-zinc-600 italic", "Select an item to see details" }
                }
            }
        }
    }
}

// endregion: --- Detail panel component

// region: --- Helpers

pub(super) fn render_param_bars(params: &[DetailParam]) -> Element {
    rsx! {
        for param in params.iter() {
            {
                let pct = (param.value * 100.0).round() as u32;
                let width_pct = format!("{}%", pct);
                let name = param.name.clone();
                rsx! {
                    div { class: "flex items-center gap-2",
                        span { class: "text-xs text-zinc-400 w-24 truncate flex-shrink-0", "{name}" }
                        div { class: "flex-1 h-1.5 rounded-full overflow-hidden",
                            style: "background: rgba(255,255,255,0.06);",
                            div {
                                class: "h-full bg-zinc-400 rounded-full",
                                style: "width: {width_pct}",
                            }
                        }
                        span { class: "text-[10px] text-zinc-600 w-8 text-right flex-shrink-0", "{pct}%" }
                    }
                }
            }
        }
    }
}

/// Find the deepest selected item's detail data.
pub(super) fn find_detail<'a>(
    col4: &'a [ColumnItem],
    col4_sel: Option<usize>,
    col3: &'a [ColumnItem],
    col3_sel: Option<usize>,
    col2: &'a [ColumnItem],
    col2_sel: Option<usize>,
) -> (
    Option<String>,
    Option<&'a MetadataModel>,
    Option<&'a DetailData>,
) {
    if let Some(item) = col4_sel.and_then(|i| col4.get(i)) {
        return (
            Some(item.name.clone()),
            item.metadata.as_ref(),
            Some(&item.detail),
        );
    }
    if let Some(item) = col3_sel.and_then(|i| col3.get(i)) {
        return (
            Some(item.name.clone()),
            item.metadata.as_ref(),
            Some(&item.detail),
        );
    }
    if let Some(item) = col2_sel.and_then(|i| col2.get(i)) {
        return (
            Some(item.name.clone()),
            item.metadata.as_ref(),
            Some(&item.detail),
        );
    }
    (None, None, None)
}

/// Filter items by text search + tag keys, then sort.
pub(super) fn filter_and_sort(
    items: &[ColumnItem],
    search: &str,
    tag_filters: &[String],
    sort: SortMode,
) -> Vec<ColumnItem> {
    let needle = search.trim().to_ascii_lowercase();
    let mut out: Vec<ColumnItem> = items
        .iter()
        .filter(|item| {
            // Text search: match name or subtitle
            if !needle.is_empty() {
                let name_match = item.name.to_ascii_lowercase().contains(&needle);
                let sub_match = item
                    .subtitle
                    .as_ref()
                    .map_or(false, |s| s.to_ascii_lowercase().contains(&needle));
                let tag_match = item
                    .structured_tags
                    .values()
                    .any(|t| t.value.contains(&needle));
                if !name_match && !sub_match && !tag_match {
                    return false;
                }
            }
            // Tag filters: item must have ALL active filter tags
            for key in tag_filters {
                if !item.structured_tags.contains_key(key) {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    match sort {
        SortMode::Name => out.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        }),
        SortMode::NameDesc => out.sort_by(|a, b| {
            b.name
                .to_ascii_lowercase()
                .cmp(&a.name.to_ascii_lowercase())
        }),
        SortMode::Variants => out.sort_by(|a, b| {
            let va = a
                .badge
                .as_ref()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            let vb = b
                .badge
                .as_ref()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            vb.cmp(&va)
        }),
        SortMode::BlockType => out.sort_by(|a, b| {
            let sa = a.subtitle.as_deref().unwrap_or("");
            let sb = b.subtitle.as_deref().unwrap_or("");
            sa.cmp(sb).then_with(|| a.name.cmp(&b.name))
        }),
    }
    out
}

/// Collect all unique tag keys from a set of items, grouped by category.
pub(super) fn collect_available_tags(
    items: &[ColumnItem],
) -> Vec<(signal::tagging::TagCategory, Vec<String>)> {
    use super::types::FILTER_CATEGORIES;
    use std::collections::BTreeMap;
    let mut by_cat: BTreeMap<signal::tagging::TagCategory, Vec<String>> = BTreeMap::new();
    for item in items {
        for tag in item.structured_tags.values() {
            let entry = by_cat.entry(tag.category).or_default();
            let key = tag.key();
            if !entry.contains(&key) {
                entry.push(key);
            }
        }
    }
    // Return only the categories we want to expose as filters, in display order.
    FILTER_CATEGORIES
        .iter()
        .filter_map(|cat| {
            by_cat.remove(cat).map(|mut vals| {
                vals.sort();
                (*cat, vals)
            })
        })
        .collect()
}

/// Display name for a tag category.
pub(super) fn tag_category_label(cat: signal::tagging::TagCategory) -> &'static str {
    match cat {
        signal::tagging::TagCategory::Tone => "Tone",
        signal::tagging::TagCategory::Character => "Character",
        signal::tagging::TagCategory::Genre => "Genre",
        signal::tagging::TagCategory::Vendor => "Vendor",
        signal::tagging::TagCategory::Plugin => "Plugin",
        signal::tagging::TagCategory::Context => "Context",
        signal::tagging::TagCategory::Instrument => "Instrument",
        signal::tagging::TagCategory::Block => "Block",
        signal::tagging::TagCategory::Module => "Module",
        signal::tagging::TagCategory::RigType => "Rig Type",
        signal::tagging::TagCategory::EngineType => "Engine Type",
        signal::tagging::TagCategory::DomainLevel => "Level",
        signal::tagging::TagCategory::Workflow => "Workflow",
        signal::tagging::TagCategory::Custom => "Custom",
    }
}

/// Extract just the value portion from a `category:value` tag key.
pub(super) fn tag_display_value(key: &str) -> &str {
    key.split_once(':').map_or(key, |(_, v)| v)
}

pub(super) fn rig_type_display(rt: signal::rig::RigType) -> &'static str {
    match rt {
        signal::rig::RigType::Guitar => "Guitar",
        signal::rig::RigType::Bass => "Bass",
        signal::rig::RigType::Keys => "Keys",
        signal::rig::RigType::Drums => "Drums",
        signal::rig::RigType::DrumEnhancement => "Drum Enh.",
        signal::rig::RigType::Vocals => "Vocals",
    }
}

// endregion: --- Helpers
