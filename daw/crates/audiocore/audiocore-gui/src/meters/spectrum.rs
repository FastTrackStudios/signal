//! Spectrum analyzer — bar-graph frequency visualization.
//!
//! Ported from FastTrackStudio signal-ui, adapted for Blitz inline styles.

use crate::theme::use_theme;
use nice_plug_dioxus::prelude::*;

/// A frequency spectrum bar graph.
///
/// Each entry in `bins` renders as one vertical bar. Color gradient
/// follows metering convention: safe (green), warn (amber), danger (red).
#[component]
pub fn SpectrumAnalyzer(
    /// Frequency bin magnitudes (0.0–1.0 normalized). Each entry is one bar.
    #[props(default)]
    bins: Vec<f64>,
    /// Width in pixels.
    #[props(default = 200)]
    width: u32,
    /// Height in pixels.
    #[props(default = 64)]
    height: u32,
    /// Bar gap in pixels.
    #[props(default = 1)]
    gap: u32,
) -> Element {
    let t = use_theme();
    let t = *t.read();

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
                        t.signal_danger
                    } else if mag > 0.6 {
                        t.signal_warn
                    } else {
                        t.signal_safe
                    };
                    rsx! {
                        div {
                            style: format!(
                                "width:{bar_w:.1}px; height:{bar_h:.1}px; \
                                 border-radius:2px 2px 0 0; background:{color}; \
                                 transition:height 0.05s;"
                            ),
                        }
                    }
                }
            }
        }
    }
}
