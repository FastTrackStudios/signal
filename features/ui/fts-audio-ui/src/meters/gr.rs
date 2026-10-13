//! Gain reduction meter — vertical bar that fills from the top down.
//!
//! Color zones: green <6 dB, amber 6–15 dB, red >15 dB.

use crate::theme::*;
use dioxus::prelude::*;

#[component]
pub fn GrMeter(
    /// Current gain reduction in dB (positive = reducing).
    gain_reduction_db: f32,
    #[props(default = 30.0)] max_gr_db: f32,
    #[props(default = 200.0)] height: f32,
    #[props(default = 16.0)] width: f32,
) -> Element {
    let clamped = gain_reduction_db.clamp(0.0, max_gr_db);
    let fill_pct = (clamped / max_gr_db) * 100.0;
    let gr_text = format!("{:.1}", -gain_reduction_db);

    let color = if clamped < 6.0 {
        SIGNAL_SAFE
    } else if clamped < 15.0 {
        SIGNAL_WARN
    } else {
        SIGNAL_DANGER
    };

    rsx! {
        div {
            style: "display:flex; flex-direction:column; align-items:center; gap:4px; min-width:36px;",
            div {
                style: format!("font-size:10px; color:{TEXT_DIM}; text-transform:uppercase;"),
                "GR"
            }
            div {
                style: format!(
                    "width:{width}px; height:{height}px; background:{SURFACE}; \
                     border-radius:3px; position:relative; overflow:hidden; \
                     border:1px solid {BORDER};"
                ),
                div {
                    style: format!(
                        "position:absolute; top:0; left:0; right:0; \
                         height:{fill_pct}%; background:{color};"
                    ),
                }
            }
            div {
                style: format!(
                    "font-size:11px; color:{TEXT}; font-variant-numeric:tabular-nums; \
                     min-width:36px; text-align:center;"
                ),
                "{gr_text}"
            }
        }
    }
}
