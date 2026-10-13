//! Conflict resolution UI — surfaces composer suggestions for user action.

use reaper_dioxus::prelude::*;

use crate::keybind_config::composer::{
    ConflictGroup, ConflictSeverity, ResolutionStrategy, ResolutionSuggestion, WorkflowBindings,
    WorkflowOverride, compose,
};

/// Conflict resolution panel component.
/// Shows all conflicts between enabled workflows with ranked suggestions.
#[component]
pub fn ConflictPanel(
    workflows: Vec<WorkflowBindings>,
    overrides: Signal<Vec<WorkflowOverride>>,
    enabled_ids: Vec<String>,
) -> Element {
    let config = compose(&workflows, &overrides.read(), &enabled_ids);

    if config.conflicts.is_empty() {
        return rsx! {
            div { class: "flex items-center gap-2 px-3 py-2 text-green-400 text-xs",
                span { "✓" }
                span { "No conflicts — all {config.stats.total_bindings} bindings compose cleanly" }
            }
        };
    }

    rsx! {
        div { class: "flex flex-col gap-2",
            // Stats bar
            div { class: "flex items-center gap-3 px-3 py-1.5 bg-amber-900/20 border border-amber-700/30 rounded text-xs",
                span { class: "text-amber-400 font-bold",
                    "{config.stats.hard_conflicts} conflict(s)"
                }
                span { class: "text-zinc-400",
                    "{config.stats.total_bindings} total bindings · {config.stats.remapped_bindings} remapped"
                }
            }

            // Conflict groups
            for group in config.conflicts.iter() {
                ConflictGroupView {
                    group: group.clone(),
                    overrides: overrides,
                }
            }
        }
    }
}

#[component]
fn ConflictGroupView(group: ConflictGroup, overrides: Signal<Vec<WorkflowOverride>>) -> Element {
    let hard_count = group
        .conflicts
        .iter()
        .filter(|c| c.severity == ConflictSeverity::Hard)
        .count();
    let prefix_count = group
        .conflicts
        .iter()
        .filter(|c| c.severity == ConflictSeverity::PrefixOverlap)
        .count();

    rsx! {
        div { class: "bg-zinc-800 border border-zinc-700 rounded overflow-hidden",
            // Header
            div { class: "flex items-center gap-2 px-3 py-2 border-b border-zinc-700/50",
                span { class: "text-zinc-200 text-xs font-bold",
                    "{group.workflow_a}"
                }
                span { class: "text-rose-400 text-xs", "⚡" }
                span { class: "text-zinc-200 text-xs font-bold",
                    "{group.workflow_b}"
                }
                div { class: "ml-auto flex gap-2",
                    if hard_count > 0 {
                        span { class: "bg-rose-900/50 text-rose-400 px-1.5 py-0.5 rounded text-xs",
                            "{hard_count} hard"
                        }
                    }
                    if prefix_count > 0 {
                        span { class: "bg-amber-900/50 text-amber-400 px-1.5 py-0.5 rounded text-xs",
                            "{prefix_count} prefix"
                        }
                    }
                }
            }

            // Conflicting keys
            div { class: "px-3 py-1.5 flex gap-1 flex-wrap border-b border-zinc-700/50",
                for conflict in group.conflicts.iter() {
                    {
                        let sev_class = match conflict.severity {
                            ConflictSeverity::Hard => "bg-rose-800/60 text-rose-300",
                            ConflictSeverity::PrefixOverlap => "bg-amber-800/60 text-amber-300",
                            ConflictSeverity::None => "bg-zinc-700 text-zinc-400",
                        };
                        rsx! {
                            span { class: "font-mono text-xs px-1.5 py-0.5 rounded {sev_class}",
                                "{conflict.keys}"
                            }
                        }
                    }
                }
            }

            // Suggestions
            div { class: "px-3 py-2 flex flex-col gap-1",
                span { class: "text-zinc-500 text-xs uppercase tracking-wider pb-1",
                    "Suggested resolutions"
                }
                for suggestion in group.suggestions.iter() {
                    SuggestionRow {
                        suggestion: suggestion.clone(),
                        overrides: overrides,
                    }
                }
            }
        }
    }
}

#[component]
fn SuggestionRow(
    suggestion: ResolutionSuggestion,
    mut overrides: Signal<Vec<WorkflowOverride>>,
) -> Element {
    let score = suggestion.score;
    let score_class = if score >= 80 {
        "text-green-400"
    } else if score >= 65 {
        "text-blue-400"
    } else {
        "text-zinc-400"
    };

    let strategy_icon = match &suggestion.strategy {
        ResolutionStrategy::PriorityWins => "🏆",
        ResolutionStrategy::WhichKeyPrefix { .. } => "📁",
        ResolutionStrategy::AddModifier { .. } => "⌨",
        ResolutionStrategy::RemapKey { .. } => "🔀",
        ResolutionStrategy::SwapKeys { .. } => "↔",
        ResolutionStrategy::DisableBindings { .. } => "🚫",
    };

    let override_to_apply = suggestion.override_rule.clone();

    rsx! {
        div { class: "flex items-center gap-2 px-2 py-1 rounded hover:bg-zinc-700/50 cursor-pointer",
            onclick: move |_| {
                let mut current = overrides.read().clone();
                // Remove existing override for this workflow
                current.retain(|o| o.workflow != override_to_apply.workflow);
                current.push(override_to_apply.clone());
                overrides.set(current);
            },

            span { class: "text-sm flex-none", "{strategy_icon}" }
            span { class: "flex-1 text-zinc-300 text-xs", "{suggestion.description}" }
            span { class: "flex-none text-xs font-mono {score_class}", "{score}" }
            span { class: "flex-none text-blue-400 text-xs hover:text-blue-300", "Apply" }
        }
    }
}
