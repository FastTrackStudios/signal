//! Spectrum analyzer — bar-graph frequency visualization.

use crate::theme::*;
use dioxus::prelude::*;

#[component]
pub fn SpectrumAnalyzer(
    /// Frequency-bin magnitudes (0–1 normalized). One bar per entry.
    #[props(default)]
    bins: Vec<f64>,
    #[props(default = 200)] width: u32,
    #[props(default = 64)] height: u32,
    #[props(default = 1)] gap: u32,
) -> Element {
    let w = width;
    let h = height;
    let hf = h as f64;
    let count = bins.len().max(1);
    let gap_f = gap as f64;
    let bar_w = ((w as f64 - gap_f * (count as f64 - 1.0)) / count as f64).max(1.0);

    rsx! {
        div {
            style: format!(
                "position:relative; overflow:hidden; border-radius:4px; \
                 background:rgba(34,34,64,0.3); display:flex; align-items:flex-end; \
                 width:{w}px; height:{h}px; gap:{gap}px;"
            ),

            for bin in bins.iter() {
                {
                    let mag = bin.clamp(0.0, 1.0);
                    let bar_h = (mag * hf).max(1.0);
                    let color = if mag > 0.85 {
                        SIGNAL_DANGER
                    } else if mag > 0.6 {
                        SIGNAL_WARN
                    } else {
                        SIGNAL_SAFE
                    };
                    rsx! {
                        div {
                            style: format!(
                                "width:{bar_w:.1}px; height:{bar_h:.1}px; \
                                 border-radius:2px 2px 0 0; background:{color};"
                            ),
                        }
                    }
                }
            }
        }
    }
}
