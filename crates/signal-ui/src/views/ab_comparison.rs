//! A/B Comparison View — side-by-side preset parameter comparison.
//!
//! Split-pane view with diff highlighting, quick-swap, and per-parameter merge.

use dioxus::prelude::*;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Comparison direction for a parameter diff.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiffDirection {
    /// Value is higher in A.
    HigherInA,
    /// Value is higher in B.
    HigherInB,
    /// Values are equal.
    Equal,
}

impl DiffDirection {
    fn color_class(self) -> &'static str {
        match self {
            Self::HigherInA => "text-signal-safe",
            Self::HigherInB => "text-signal-danger",
            Self::Equal => "text-muted-foreground",
        }
    }

    fn bg_class(self) -> &'static str {
        match self {
            Self::HigherInA => "bg-signal-safe/5",
            Self::HigherInB => "bg-signal-danger/5",
            Self::Equal => "",
        }
    }
}

/// A parameter comparison row.
#[derive(Clone, PartialEq)]
pub struct ComparisonRow {
    pub param_id: String,
    pub param_name: String,
    pub block_name: String,
    pub value_a: f64,
    pub value_b: f64,
    pub display_a: String,
    pub display_b: String,
    pub diff: DiffDirection,
}

/// Header info for a preset side.
#[derive(Clone, PartialEq)]
pub struct PresetHeader {
    pub name: String,
    pub scene_name: Option<String>,
    pub param_count: usize,
}

// ---------------------------------------------------------------------------
// Comparison Row Component
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
struct ComparisonRowViewProps {
    row: ComparisonRow,

    #[props(default)]
    on_copy_a_to_b: Option<Callback<String>>,

    #[props(default)]
    on_copy_b_to_a: Option<Callback<String>>,
}

#[component]
fn ComparisonRowView(props: ComparisonRowViewProps) -> Element {
    let row = &props.row;
    let pct_a = (row.value_a.clamp(0.0, 1.0) * 100.0) as u32;
    let pct_b = (row.value_b.clamp(0.0, 1.0) * 100.0) as u32;
    let param_id_a = row.param_id.clone();
    let param_id_b = row.param_id.clone();

    rsx! {
        div {
            class: format!(
                "grid grid-cols-[1fr_80px_2fr_40px_2fr_80px] gap-2 items-center px-3 py-1 rounded hover:bg-muted/50 group text-xs {}",
                row.diff.bg_class()
            ),

            // Parameter name (spanning block + param)
            div {
                class: "flex flex-col",
                span {
                    class: "text-[10px] text-muted-foreground truncate",
                    "{row.block_name}"
                }
                span {
                    class: "truncate font-medium",
                    "{row.param_name}"
                }
            }

            // Value A display
            span {
                class: format!("text-right font-mono {}", row.diff.color_class()),
                "{row.display_a}"
            }

            // Bar A
            div {
                class: "relative h-3 rounded bg-muted overflow-hidden",
                div {
                    class: "absolute inset-y-0 left-0 bg-signal-safe/40 transition-all",
                    style: "width: {pct_a}%;",
                }
            }

            // Merge buttons (visible on hover)
            div {
                class: "flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity",

                button {
                    class: "px-1 text-[10px] rounded hover:bg-signal-safe/20 text-signal-safe",
                    title: "Copy A to B",
                    onclick: move |_| {
                        if let Some(cb) = &props.on_copy_a_to_b {
                            cb.call(param_id_a.clone());
                        }
                    },
                    "\u{2192}"
                }
                button {
                    class: "px-1 text-[10px] rounded hover:bg-signal-danger/20 text-signal-danger",
                    title: "Copy B to A",
                    onclick: move |_| {
                        if let Some(cb) = &props.on_copy_b_to_a {
                            cb.call(param_id_b.clone());
                        }
                    },
                    "\u{2190}"
                }
            }

            // Bar B
            div {
                class: "relative h-3 rounded bg-muted overflow-hidden",
                div {
                    class: "absolute inset-y-0 left-0 bg-signal-danger/40 transition-all",
                    style: "width: {pct_b}%;",
                }
            }

            // Value B display
            span {
                class: format!("font-mono {}", row.diff.color_class()),
                "{row.display_b}"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Preset Header
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
struct PresetHeaderViewProps {
    header: PresetHeader,
    side: String,
    color: String,

    #[props(default)]
    class: String,
}

#[component]
fn PresetHeaderView(props: PresetHeaderViewProps) -> Element {
    rsx! {
        div {
            class: format!("flex items-center gap-2 px-3 py-2 {}", props.class),

            span {
                class: format!("text-sm font-bold {}", props.color),
                "{props.side}"
            }
            span {
                class: "text-sm font-semibold",
                "{props.header.name}"
            }
            if let Some(scene) = &props.header.scene_name {
                span {
                    class: "text-[10px] text-muted-foreground",
                    "({scene})"
                }
            }
            span {
                class: "text-[10px] text-muted-foreground ml-auto",
                {format!("{} params", props.header.param_count)}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Filter tabs for diff view
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiffFilter {
    All,
    DiffsOnly,
    HigherInA,
    HigherInB,
}

impl DiffFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::DiffsOnly => "Diffs",
            Self::HigherInA => "A > B",
            Self::HigherInB => "B > A",
        }
    }
}

// ---------------------------------------------------------------------------
// A/B Comparison View
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub struct ABComparisonProps {
    /// Preset A header info.
    preset_a: PresetHeader,

    /// Preset B header info.
    preset_b: PresetHeader,

    /// Comparison rows.
    rows: Vec<ComparisonRow>,

    /// Current diff filter.
    #[props(default = DiffFilter::All)]
    filter: DiffFilter,

    /// Callback: swap A and B.
    #[props(default)]
    on_swap: Option<Callback<()>>,

    /// Callback: change diff filter.
    #[props(default)]
    on_filter_change: Option<Callback<DiffFilter>>,

    /// Callback: copy param from A to B.
    #[props(default)]
    on_copy_a_to_b: Option<Callback<String>>,

    /// Callback: copy param from B to A.
    #[props(default)]
    on_copy_b_to_a: Option<Callback<String>>,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn ABComparison(props: ABComparisonProps) -> Element {
    let diff_count = props
        .rows
        .iter()
        .filter(|r| r.diff != DiffDirection::Equal)
        .count();
    let total = props.rows.len();

    let filters = [
        DiffFilter::All,
        DiffFilter::DiffsOnly,
        DiffFilter::HigherInA,
        DiffFilter::HigherInB,
    ];

    rsx! {
        div {
            class: format!("flex flex-col gap-2 {}", props.class),

            // Headers row
            div {
                class: "flex items-center gap-2",

                PresetHeaderView {
                    header: props.preset_a.clone(),
                    side: "A".to_string(),
                    color: "text-signal-safe".to_string(),
                    class: "flex-1 rounded-lg bg-signal-safe/5 border border-signal-safe/20".to_string(),
                }

                // Swap button
                button {
                    class: "px-3 py-1.5 rounded-lg border border-border bg-card hover:bg-accent text-sm font-bold",
                    title: "Swap A and B",
                    onclick: move |_| {
                        if let Some(cb) = &props.on_swap {
                            cb.call(());
                        }
                    },
                    "\u{21C4}"
                }

                PresetHeaderView {
                    header: props.preset_b.clone(),
                    side: "B".to_string(),
                    color: "text-signal-danger".to_string(),
                    class: "flex-1 rounded-lg bg-signal-danger/5 border border-signal-danger/20".to_string(),
                }
            }

            // Filter toolbar
            div {
                class: "flex items-center gap-2 px-3 py-1.5",

                for f in filters.iter() {
                    {
                        let filter = *f;
                        let is_active = filter == props.filter;
                        let active_class = if is_active {
                            "bg-primary text-primary-foreground"
                        } else {
                            "bg-muted text-muted-foreground hover:bg-accent"
                        };
                        rsx! {
                            button {
                                class: format!("px-2 py-0.5 rounded text-xs font-medium transition-colors {active_class}"),
                                onclick: move |_| {
                                    if let Some(cb) = &props.on_filter_change {
                                        cb.call(filter);
                                    }
                                },
                                {filter.label()}
                            }
                        }
                    }
                }

                span {
                    class: "ml-auto text-[10px] text-muted-foreground",
                    {format!("{} diffs / {} total", diff_count, total)}
                }
            }

            // Column headers
            div {
                class: "grid grid-cols-[1fr_80px_2fr_40px_2fr_80px] gap-2 px-3 py-1 border-b border-border text-[10px] text-muted-foreground font-semibold uppercase tracking-wider",
                span { "Parameter" }
                span { class: "text-right", "A" }
                span {}
                span {}
                span {}
                span { "B" }
            }

            // Comparison rows
            div {
                class: "flex flex-col overflow-y-auto max-h-[70vh]",

                if props.rows.is_empty() {
                    div {
                        class: "text-xs text-muted-foreground text-center py-8",
                        "No parameters to compare."
                    }
                }

                for row in props.rows.iter() {
                    ComparisonRowView {
                        row: row.clone(),
                        on_copy_a_to_b: props.on_copy_a_to_b.clone(),
                        on_copy_b_to_a: props.on_copy_b_to_a.clone(),
                    }
                }
            }
        }
    }
}
