//! Modulation route editor — LFO, envelope, MIDI CC, and expression routing.

use dioxus::prelude::*;

use signal::modulation::{ModulationRoute, ModulationSource};

#[derive(Props, Clone, PartialEq)]
pub struct BlockModulationProps {
    pub block: signal::Block,
}

/// Renders modulation routes as a list with source config and controls.
///
/// Each route shows: source type icon, target parameter, amount slider,
/// and enable toggle. Supports LFO, Envelope, MIDI CC, and Expression sources.
#[component]
pub fn BlockModulation(props: BlockModulationProps) -> Element {
    let modulation = &props.block.modulation;

    let Some(ref mod_set) = modulation else {
        return rsx! {
            div { class: "flex flex-col items-center justify-center h-32 text-center px-4",
                div { class: "text-zinc-600 text-xs", "No Modulation" }
                div { class: "text-zinc-700 text-[10px] mt-1",
                    "Add modulation routes to automate parameters with LFOs, envelopes, or MIDI."
                }
                button {
                    class: "mt-3 px-3 py-1.5 text-[11px] rounded \
                            bg-zinc-800 hover:bg-zinc-700 text-zinc-400 \
                            hover:text-zinc-200 border border-zinc-700 border-dashed \
                            transition-all duration-150",
                    "Add Route"
                }
            }
        };
    };

    rsx! {
        div { class: "p-3 space-y-3",
            // Header
            div { class: "flex items-center justify-between",
                div { class: "flex items-center gap-2",
                    span { class: "text-[10px] font-bold uppercase tracking-widest text-zinc-400",
                        "Modulation Routes"
                    }
                    span { class: "text-[10px] text-zinc-600",
                        "({mod_set.routes.len()})"
                    }
                }
                button {
                    class: "px-2 py-1 text-[10px] rounded \
                            bg-zinc-800 hover:bg-zinc-700 text-zinc-500 \
                            hover:text-zinc-300 border border-zinc-700 \
                            transition-all duration-150",
                    "Add Route"
                }
            }

            if mod_set.routes.is_empty() {
                div { class: "text-[10px] text-zinc-600 text-center py-4",
                    "No routes configured."
                }
            } else {
                // Route list
                div { class: "space-y-2",
                    for route in mod_set.routes.iter() {
                        { render_route(route) }
                    }
                }
            }
        }
    }
}

/// Renders a single modulation route card.
fn render_route(route: &ModulationRoute) -> Element {
    let source_icon = source_icon(&route.source);
    let source_name = route.source.display_name();
    let target_param = &route.target.param_id;
    let amount_pct = (route.amount * 100.0).round() as i32;
    let enabled = route.enabled;

    rsx! {
        div {
            key: "{route.id}",
            class: if enabled {
                "relative overflow-hidden rounded-lg border border-zinc-800/60 bg-zinc-900/40"
            } else {
                "relative overflow-hidden rounded-lg border border-zinc-800/40 bg-zinc-900/20 opacity-50"
            },
            div { class: "px-3 py-2.5 space-y-2",
                // Top row: source + target + enable toggle
                div { class: "flex items-center gap-2",
                    // Source icon/badge
                    span {
                        class: "px-1.5 py-0.5 rounded text-[9px] font-medium",
                        style: "background-color: {source_color(&route.source)}20; color: {source_color(&route.source)};",
                        "{source_icon} {source_name}"
                    }
                    // Arrow
                    span { class: "text-zinc-600 text-[10px]", "\u{2192}" }
                    // Target
                    span { class: "text-[11px] text-zinc-300 truncate flex-1", "{target_param}" }
                    // Enable toggle
                    button {
                        class: if enabled {
                            "w-5 h-5 rounded text-[10px] bg-emerald-900/40 text-emerald-400"
                        } else {
                            "w-5 h-5 rounded text-[10px] bg-zinc-800 text-zinc-600"
                        },
                        if enabled { "\u{2713}" } else { "\u{2715}" }
                    }
                }

                // Amount slider
                div { class: "flex items-center gap-2",
                    span { class: "text-[10px] text-zinc-500 w-10 flex-shrink-0", "Amount" }
                    input {
                        r#type: "range",
                        min: "-100",
                        max: "100",
                        value: "{amount_pct}",
                        class: "flex-1 h-1",
                        style: "accent-color: {source_color(&route.source)};",
                    }
                    span { class: "text-[10px] text-zinc-500 tabular-nums w-8 text-right flex-shrink-0",
                        "{amount_pct}%"
                    }
                }

                // Source-specific config summary
                { render_source_config(&route.source) }
            }
        }
    }
}

/// Renders source-specific configuration summary.
fn render_source_config(source: &ModulationSource) -> Element {
    match source {
        ModulationSource::Lfo(lfo) => rsx! {
            div { class: "flex items-center gap-3 text-[10px] text-zinc-500",
                span { "{lfo.waveform.display_name()}" }
                span { "{lfo.rate_hz:.1} Hz" }
                span { "Depth: {(lfo.depth * 100.0) as i32}%" }
                if lfo.tempo_sync {
                    span { class: "px-1 rounded bg-zinc-800 text-zinc-400",
                        "Sync: {lfo.sync_division.map(|d| d.display_name()).unwrap_or(\"1/4\")}"
                    }
                }
            }
        },
        ModulationSource::Envelope(env) => rsx! {
            div { class: "flex items-center gap-3 text-[10px] text-zinc-500",
                span { "A: {env.attack_s:.2}s" }
                span { "D: {env.decay_s:.2}s" }
                span { "S: {(env.sustain * 100.0) as i32}%" }
                span { "R: {env.release_s:.2}s" }
            }
        },
        ModulationSource::MidiCc { cc_number } => rsx! {
            div { class: "text-[10px] text-zinc-500",
                "CC #{cc_number}"
            }
        },
        ModulationSource::Expression => rsx! {
            div { class: "text-[10px] text-zinc-500",
                "Expression pedal input"
            }
        },
        ModulationSource::Macro { knob_id } => rsx! {
            div { class: "text-[10px] text-zinc-500",
                "Macro: {knob_id}"
            }
        },
        ModulationSource::Follower(cfg) => rsx! {
            div { class: "flex items-center gap-3 text-[10px] text-zinc-500",
                span { "Atk: {cfg.attack_ms:.0}ms" }
                span { "Rel: {cfg.release_ms:.0}ms" }
                span { "Depth: {(cfg.depth * 100.0) as i32}%" }
            }
        },
        ModulationSource::Random(cfg) => rsx! {
            div { class: "flex items-center gap-3 text-[10px] text-zinc-500",
                span { "{cfg.rate_hz:.1} Hz" }
                span { "Smooth: {(cfg.smoothing * 100.0) as i32}%" }
                span { "Depth: {(cfg.depth * 100.0) as i32}%" }
            }
        },
    }
}

/// Icon character for a modulation source type.
fn source_icon(source: &ModulationSource) -> &'static str {
    match source {
        ModulationSource::Lfo(_) => "\u{223F}",        // ∿ sine wave
        ModulationSource::Envelope(_) => "\u{25B3}",    // △ triangle
        ModulationSource::MidiCc { .. } => "\u{266A}",  // ♪ music note
        ModulationSource::Expression => "\u{21C5}",     // ⇅ up down arrows
        ModulationSource::Macro { .. } => "\u{25C9}",   // ◉ macro knob
        ModulationSource::Follower(_) => "\u{2261}",    // ≡ audio level
        ModulationSource::Random(_) => "\u{2684}",      // ⚄ die
    }
}

/// Accent color for a modulation source type.
fn source_color(source: &ModulationSource) -> &'static str {
    match source {
        ModulationSource::Lfo(_) => "#A855F7",          // purple
        ModulationSource::Envelope(_) => "#F97316",      // orange
        ModulationSource::MidiCc { .. } => "#3B82F6",    // blue
        ModulationSource::Expression => "#22C55E",       // green
        ModulationSource::Macro { .. } => "#EAB308",     // yellow
        ModulationSource::Follower(_) => "#EC4899",      // pink
        ModulationSource::Random(_) => "#14B8A6",        // teal
    }
}
