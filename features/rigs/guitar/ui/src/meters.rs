//! Compact IN/OUT level meters for the top bar.

use dioxus::prelude::*;
use signal_guitar_proto::RigPerf;

/// Map a linear peak (0..1) to a perceptual meter level (0..1) via a sqrt
/// curve, so quiet-but-present signal is clearly visible.
#[must_use]
pub fn meter_level(peak: f32) -> f64 {
    f64::from(peak.max(0.0).sqrt()).min(1.0)
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

/// The cost strip: what the rig is spending of its realtime budget, and
/// whether it has missed a deadline.
///
/// Two numbers because they answer different questions. The bar's fill is the
/// **mean** block — the number to compare between builds, since it does not
/// jump when the scheduler preempts one callback. The bright tick is the
/// **peak** — the number that says whether the player heard something, because
/// a single overrun is audible and a mean hides it.
///
/// `drop` is counted from our own render timing rather than the graph's xrun
/// flag: on a follower node the graph can report zero while audio is dropping,
/// so the count that is actually ours is the one shown.
#[component]
pub fn DspReadout(perf: RigPerf) -> Element {
    // No blocks rendered = the device never opened. A load bar reading 0% then
    // looks like "free", when the truth is "not running".
    if perf.blocks == 0 {
        return rsx! {
            div {
                style: "display: flex; align-items: center; gap: 6px; padding: 2px 6px; \
                        font-size: 9px; color: #6b7280; font-variant-numeric: tabular-nums;",
                "DSP — idle"
            }
        };
    }

    let mean_pct = (f64::from(perf.mean_load) * 100.0).clamp(0.0, 100.0);
    let peak_pct = (f64::from(perf.load) * 100.0).clamp(0.0, 100.0);
    // Where the worst block sat, as a share of budget — the tick's position.
    let budget = f64::from(perf.budget_us().max(1));
    let worst_pct = (f64::from(perf.peak_render_us) / budget * 100.0).clamp(0.0, 100.0);

    // Thresholds are about headroom, not neatness: above ~70% of budget a
    // transient spike lands past the deadline, so that is where amber starts.
    let colour = if worst_pct > 90.0 || perf.over_budget > 0 {
        "#ef4444"
    } else if worst_pct > 70.0 {
        "#eab308"
    } else {
        "#22c55e"
    };

    let drops = perf.over_budget;
    let latency = perf.buffer_latency_ms();

    rsx! {
        div {
            style: "display: flex; align-items: center; gap: 8px; padding: 2px 6px; \
                    font-size: 9px; color: #9ca3af; font-variant-numeric: tabular-nums; \
                    white-space: nowrap; overflow: hidden;",
            span { style: "font-weight: 600; color: #6b7280;", "DSP" }
            div {
                style: "position: relative; width: 96px; height: 6px; border-radius: 3px; \
                        background-color: rgba(0,0,0,0.5); overflow: hidden;",
                // Mean fill — the steady cost.
                div {
                    style: "position: absolute; top: 0; bottom: 0; left: 0; width: {mean_pct}%; \
                            background-color: {colour}; opacity: 0.85;",
                }
                // Peak tick — where the worst block landed.
                div {
                    style: "position: absolute; top: 0; bottom: 0; left: {worst_pct}%; width: 2px; \
                            background-color: #f9fafb;",
                }
            }
            span { "{mean_pct:.0}% avg · {peak_pct:.0}% now" }
            span { style: "color: #6b7280;", "|" }
            span { "{us_ms(perf.mean_render_us)} / {us_ms(perf.peak_render_us)} ms" }
            span { style: "color: #6b7280;", "|" }
            span { "{perf.block_frames} fr @ {perf.sample_rate / 1000} k · {latency:.1} ms rt" }
            span { style: "color: #6b7280;", "|" }
            span {
                style: if drops > 0 { "color: #ef4444; font-weight: 600;" } else { "color: #6b7280;" },
                if drops > 0 { "{drops} drop" } else { "no drops" }
            }
        }
    }
}

/// Microseconds as milliseconds, two decimals — the scale a block's render
/// time actually lives at (tens to hundreds of microseconds).
fn us_ms(us: u32) -> String {
    format!("{:.2}", f64::from(us) / 1000.0)
}
