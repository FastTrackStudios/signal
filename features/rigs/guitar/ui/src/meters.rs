//! Compact IN/OUT level meters for the top bar.

use dioxus::prelude::*;

/// Map a linear peak (0..1) to a perceptual meter level (0..1) via a sqrt
/// curve, so quiet-but-present signal is clearly visible.
pub fn meter_level(peak: f32) -> f64 {
    (peak.max(0.0).sqrt() as f64).min(1.0)
}

/// Paired IN / OUT level meters (confirms signal passthrough).
#[component]
pub fn MeterPair(input: f64, output: f64) -> Element {
    rsx! {
        div { class: "flex items-center gap-3",
            MeterBar { label: "IN", level: input }
            MeterBar { label: "OUT", level: output }
        }
    }
}

/// A single horizontal level meter with an explicit, always-visible fill.
#[component]
pub fn MeterBar(label: &'static str, level: f64) -> Element {
    let clamped = level.clamp(0.0, 1.0);
    let pct = (clamped * 100.0) as u32;
    let color = if clamped > 0.9 {
        "#ef4444"
    } else if clamped > 0.7 {
        "#eab308"
    } else {
        "#22c55e"
    };
    rsx! {
        div { class: "flex items-center gap-1.5",
            span { class: "text-[10px] font-semibold text-muted-foreground w-7 text-right", "{label}" }
            div { class: "relative w-32 h-3 rounded bg-black/50 overflow-hidden border border-border",
                div {
                    class: "absolute inset-y-0 left-0 transition-[width] duration-75",
                    style: "width: {pct}%; background-color: {color};",
                }
            }
        }
    }
}
