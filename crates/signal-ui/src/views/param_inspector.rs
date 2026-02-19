//! Parameter Inspector — table view of all parameters across rig blocks.
//!
//! Sortable, filterable grid with inline value editing and source indicators.

use dioxus::prelude::*;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Source of a parameter's current value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParamSource {
    Preset,
    Override,
    Automation,
    Default,
}

impl ParamSource {
    fn label(self) -> &'static str {
        match self {
            Self::Preset => "Preset",
            Self::Override => "Override",
            Self::Automation => "Auto",
            Self::Default => "Default",
        }
    }

    fn badge_class(self) -> &'static str {
        match self {
            Self::Preset => "bg-signal-slot-a/20 text-signal-slot-a",
            Self::Override => "bg-signal-override/20 text-signal-override",
            Self::Automation => "bg-signal-automation/20 text-signal-automation",
            Self::Default => "bg-muted text-muted-foreground",
        }
    }
}

/// A parameter row in the inspector.
#[derive(Clone, PartialEq)]
pub struct ParamRow {
    pub id: String,
    pub block_name: String,
    pub param_name: String,
    pub value: f64,
    pub display_value: String,
    pub source: ParamSource,
    pub has_modulation: bool,
}

/// Sort column for the inspector.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortColumn {
    Block,
    Parameter,
    Value,
    Source,
}

/// Sort direction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortDirection {
    Asc,
    Desc,
}

// ---------------------------------------------------------------------------
// Parameter Row Component
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
struct ParamRowViewProps {
    row: ParamRow,

    #[props(default)]
    on_value_change: Option<Callback<(String, f64)>>,
}

#[component]
fn ParamRowView(props: ParamRowViewProps) -> Element {
    let row = &props.row;
    let pct = (row.value.clamp(0.0, 1.0) * 100.0) as u32;
    let row_id = row.id.clone();

    rsx! {
        div {
            class: "grid grid-cols-[1fr_1.5fr_2fr_80px_16px] gap-2 items-center px-3 py-1.5 rounded hover:bg-muted group text-xs",

            // Block name
            span {
                class: "truncate font-medium",
                "{row.block_name}"
            }

            // Parameter name
            span {
                class: "truncate",
                "{row.param_name}"
            }

            // Value with inline slider
            div {
                class: "flex items-center gap-2",

                // Mini track
                div {
                    class: "relative flex-1 h-4 rounded bg-muted overflow-hidden",

                    div {
                        class: "absolute inset-y-0 left-0 bg-primary/40 transition-all",
                        style: "width: {pct}%;",
                    }

                    input {
                        r#type: "range",
                        class: "absolute inset-0 w-full h-full opacity-0 cursor-pointer",
                        min: "0",
                        max: "100",
                        value: "{pct}",
                        oninput: move |evt: FormEvent| {
                            if let Some(cb) = &props.on_value_change {
                                if let Ok(v) = evt.value().parse::<f64>() {
                                    cb.call((row_id.clone(), v / 100.0));
                                }
                            }
                        },
                    }
                }

                span {
                    class: "text-[10px] text-muted-foreground w-12 text-right",
                    "{row.display_value}"
                }
            }

            // Source badge
            span {
                class: format!(
                    "px-1.5 py-0.5 rounded text-[10px] font-medium text-center {}",
                    row.source.badge_class()
                ),
                {row.source.label()}
            }

            // Modulation indicator
            if row.has_modulation {
                span {
                    class: "w-2 h-2 rounded-full bg-signal-mod",
                    title: "Modulated",
                }
            } else {
                span { class: "w-2" }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Column Header
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
struct ColumnHeaderProps {
    label: String,
    column: SortColumn,
    current_sort: SortColumn,
    current_direction: SortDirection,

    #[props(default)]
    on_sort: Option<Callback<SortColumn>>,

    #[props(default)]
    class: String,
}

#[component]
fn ColumnHeader(props: ColumnHeaderProps) -> Element {
    let is_active = props.column == props.current_sort;
    let arrow = if is_active {
        match props.current_direction {
            SortDirection::Asc => " \u{25B2}",
            SortDirection::Desc => " \u{25BC}",
        }
    } else {
        ""
    };
    let col = props.column;

    rsx! {
        button {
            class: format!(
                "text-xs font-semibold text-left hover:text-foreground transition-colors {} {}",
                if is_active { "text-foreground" } else { "text-muted-foreground" },
                props.class
            ),
            onclick: move |_| {
                if let Some(cb) = &props.on_sort {
                    cb.call(col);
                }
            },
            "{props.label}{arrow}"
        }
    }
}

// ---------------------------------------------------------------------------
// Parameter Inspector
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub struct ParamInspectorProps {
    /// All parameter rows.
    params: Vec<ParamRow>,

    /// Current search/filter query.
    #[props(default)]
    filter_query: String,

    /// Current sort column.
    #[props(default = SortColumn::Block)]
    sort_column: SortColumn,

    /// Current sort direction.
    #[props(default = SortDirection::Asc)]
    sort_direction: SortDirection,

    /// Callback when filter changes.
    #[props(default)]
    on_filter_change: Option<Callback<String>>,

    /// Callback when sort changes.
    #[props(default)]
    on_sort_change: Option<Callback<SortColumn>>,

    /// Callback when a parameter value is edited.
    #[props(default)]
    on_value_change: Option<Callback<(String, f64)>>,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn ParamInspector(props: ParamInspectorProps) -> Element {
    let total = props.params.len();
    let override_count = props
        .params
        .iter()
        .filter(|p| p.source == ParamSource::Override)
        .count();
    let mod_count = props.params.iter().filter(|p| p.has_modulation).count();

    rsx! {
        div {
            class: format!("flex flex-col gap-2 {}", props.class),

            // Toolbar
            div {
                class: "flex items-center gap-3 px-3 py-2",

                // Search
                input {
                    class: "flex-1 px-2 py-1 text-xs rounded border border-input bg-background text-foreground placeholder:text-muted-foreground",
                    placeholder: "Filter parameters...",
                    value: "{props.filter_query}",
                    oninput: move |evt: FormEvent| {
                        if let Some(cb) = &props.on_filter_change {
                            cb.call(evt.value().clone());
                        }
                    },
                }

                // Summary stats
                div {
                    class: "flex items-center gap-2 text-[10px] text-muted-foreground",
                    span { {format!("{} params", total)} }
                    if override_count > 0 {
                        span {
                            class: "px-1 rounded bg-signal-override/20 text-signal-override",
                            {format!("{} overrides", override_count)}
                        }
                    }
                    if mod_count > 0 {
                        span {
                            class: "px-1 rounded bg-signal-mod/20 text-signal-mod",
                            {format!("{} modulated", mod_count)}
                        }
                    }
                }
            }

            // Column headers
            div {
                class: "grid grid-cols-[1fr_1.5fr_2fr_80px_16px] gap-2 px-3 py-1 border-b border-border",

                ColumnHeader {
                    label: "Block".to_string(),
                    column: SortColumn::Block,
                    current_sort: props.sort_column,
                    current_direction: props.sort_direction,
                    on_sort: props.on_sort_change.clone(),
                }
                ColumnHeader {
                    label: "Parameter".to_string(),
                    column: SortColumn::Parameter,
                    current_sort: props.sort_column,
                    current_direction: props.sort_direction,
                    on_sort: props.on_sort_change.clone(),
                }
                ColumnHeader {
                    label: "Value".to_string(),
                    column: SortColumn::Value,
                    current_sort: props.sort_column,
                    current_direction: props.sort_direction,
                    on_sort: props.on_sort_change.clone(),
                }
                ColumnHeader {
                    label: "Source".to_string(),
                    column: SortColumn::Source,
                    current_sort: props.sort_column,
                    current_direction: props.sort_direction,
                    on_sort: props.on_sort_change.clone(),
                }
                span {}
            }

            // Rows
            div {
                class: "flex flex-col overflow-y-auto max-h-[70vh]",

                if props.params.is_empty() {
                    div {
                        class: "text-xs text-muted-foreground text-center py-8",
                        "No parameters to display."
                    }
                }

                for row in props.params.iter() {
                    ParamRowView {
                        row: row.clone(),
                        on_value_change: props.on_value_change.clone(),
                    }
                }
            }
        }
    }
}
