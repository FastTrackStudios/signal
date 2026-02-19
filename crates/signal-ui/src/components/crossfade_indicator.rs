//! Crossfade progress indicator.
//!
//! A compact progress bar with phase label shown during active crossfades:
//!
//! ```text
//! +--------------------------------------------+
//! |  ~ Crossfading...  [========----] 65%      |
//! |  Clean -> Dirty  .  EqualPower  .  50ms    |
//! +--------------------------------------------+
//! ```

use dioxus::prelude::*;

/// A compact crossfade progress indicator.
///
/// Only visible when `active` is true. Shows the transition progress,
/// phase label, source/target names, curve type, and duration.
#[component]
pub fn CrossfadeIndicator(
    /// Whether a crossfade is currently active.
    active: bool,
    /// Normalized progress [0.0, 1.0].
    progress: f64,
    /// Name of the phase (e.g., "Fading Out", "Swapping", "Fading In").
    phase_label: String,
    /// Source name (fading out).
    #[props(default = String::new())]
    source_name: String,
    /// Target name (fading in).
    #[props(default = String::new())]
    target_name: String,
    /// Crossfade curve name for display.
    #[props(default = String::from("EqualPower"))]
    curve_name: String,
    /// Duration in milliseconds.
    #[props(default = 50)]
    duration_ms: u64,
) -> Element {
    if !active {
        return rsx! {};
    }

    let progress_pct = (progress * 100.0).round();
    let has_names = !source_name.is_empty() || !target_name.is_empty();

    rsx! {
        div {
            class: "flex flex-col gap-0.5 px-3 py-2 rounded-lg \
                    bg-indigo-950/60 border border-indigo-800/40 \
                    animate-pulse",
            // Top row: icon + phase + progress bar + percentage
            div { class: "flex items-center gap-2",
                // Spinning icon
                svg {
                    class: "w-4 h-4 text-indigo-400 animate-spin",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 \
                            11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 \
                            2H15",
                    }
                }

                span { class: "text-xs font-medium text-indigo-300",
                    "{phase_label}"
                }

                // Progress bar
                div { class: "flex-1 h-1.5 bg-zinc-700/60 rounded-full overflow-hidden",
                    div {
                        class: "h-full bg-gradient-to-r from-indigo-500 to-violet-500 \
                                rounded-full transition-all duration-75",
                        style: "width: {progress_pct}%",
                    }
                }

                span { class: "text-[10px] font-mono text-indigo-400 min-w-[32px] text-right",
                    "{progress_pct:.0}%"
                }
            }

            // Bottom row: source -> target . curve . duration
            if has_names {
                div { class: "flex items-center gap-1.5 text-[10px] text-zinc-500",
                    if !source_name.is_empty() && !target_name.is_empty() {
                        span { class: "text-zinc-400",
                            "{source_name}"
                        }
                        span { "\u{2192}" }
                        span { class: "text-zinc-400",
                            "{target_name}"
                        }
                        span { "\u{00B7}" }
                    }
                    span { "{curve_name}" }
                    span { "\u{00B7}" }
                    span { "{duration_ms}ms" }
                }
            }
        }
    }
}
