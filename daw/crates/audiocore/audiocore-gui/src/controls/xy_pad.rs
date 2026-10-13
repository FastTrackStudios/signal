//! XY Pad — 2D parameter control surface.
//!
//! Backlit grid with glowing crosshair cursor and recessed background.

use crate::theme::use_theme;
use nice_plug_dioxus::prelude::*;

/// 2D control output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct XYValue {
    pub x: f64,
    pub y: f64,
}

/// A 2D parameter control pad.
///
/// A square area where dragging controls two normalized parameters
/// simultaneously (X and Y, both 0.0–1.0). Y-axis: bottom=0, top=1.
#[component]
pub fn XYPad(
    /// Current X value (0.0–1.0).
    #[props(default = 0.5)]
    x: f64,
    /// Current Y value (0.0–1.0, bottom=0, top=1).
    #[props(default = 0.5)]
    y: f64,
    /// Size in pixels (square).
    #[props(default = 120)]
    size: u32,
    /// Whether the pad is disabled.
    #[props(default)]
    disabled: bool,
    /// X-axis label.
    #[props(default)]
    x_label: Option<String>,
    /// Y-axis label.
    #[props(default)]
    y_label: Option<String>,
    /// Callback when value changes.
    #[props(default)]
    on_change: Option<Callback<XYValue>>,
) -> Element {
    let t = use_theme();
    let t = *t.read();

    let s = size;
    let sf = s as f64;
    let px = (x.clamp(0.0, 1.0) * sf) as u32;
    let py = ((1.0 - y.clamp(0.0, 1.0)) * sf) as u32; // Flip Y (top=1)

    let opacity = if disabled { "0.5" } else { "1.0" };
    let cursor = if disabled { "not-allowed" } else { "crosshair" };

    rsx! {
        div {
            style: format!(
                "display:inline-flex; flex-direction:column; align-items:center; gap:4px; \
                 opacity:{opacity};"
            ),

            div {
                style: format!(
                    "position:relative; {INSET} overflow:hidden; cursor:{cursor}; \
                     width:{s}px; height:{s}px; \
                     background:linear-gradient(180deg, rgba(20,20,35,0.8), rgba(10,10,20,0.9));",
                    INSET = t.style_inset(),
                ),

                // Grid lines — subtle backlit grid
                div {
                    style: format!(
                        "position:absolute; left:25%; top:0; width:1px; height:100%; \
                         background:{GRID};",
                        GRID = t.grid_line,
                    ),
                }
                div {
                    style: format!(
                        "position:absolute; left:50%; top:0; width:1px; height:100%; \
                         background:{GRID};",
                        GRID = t.grid_line,
                    ),
                }
                div {
                    style: format!(
                        "position:absolute; left:75%; top:0; width:1px; height:100%; \
                         background:{GRID};",
                        GRID = t.grid_line,
                    ),
                }
                div {
                    style: format!(
                        "position:absolute; left:0; top:25%; width:100%; height:1px; \
                         background:{GRID};",
                        GRID = t.grid_line,
                    ),
                }
                div {
                    style: format!(
                        "position:absolute; left:0; top:50%; width:100%; height:1px; \
                         background:{GRID};",
                        GRID = t.grid_line,
                    ),
                }
                div {
                    style: format!(
                        "position:absolute; left:0; top:75%; width:100%; height:1px; \
                         background:{GRID};",
                        GRID = t.grid_line,
                    ),
                }

                // Crosshair lines — glowing
                div {
                    style: format!(
                        "position:absolute; left:{px}px; top:0; width:1px; height:100%; \
                         background:{ACCENT}; opacity:0.3; \
                         box-shadow:0 0 4px {GLOW};",
                        ACCENT = t.accent,
                        GLOW = t.accent_glow,
                    ),
                }
                div {
                    style: format!(
                        "position:absolute; left:0; top:{py}px; width:100%; height:1px; \
                         background:{ACCENT}; opacity:0.3; \
                         box-shadow:0 0 4px {GLOW};",
                        ACCENT = t.accent,
                        GLOW = t.accent_glow,
                    ),
                }

                // Dot indicator — glowing cursor
                div {
                    style: format!(
                        "position:absolute; left:{px}px; top:{py}px; \
                         width:12px; height:12px; border-radius:6px; \
                         background:{ACCENT}; border:2px solid {BG}; \
                         transform:translate(-50%,-50%); \
                         box-shadow:0 0 8px {GLOW};",
                        ACCENT = t.accent,
                        BG = t.bg,
                        GLOW = t.accent_glow,
                    ),
                }
            }

            // Labels
            div {
                style: format!(
                    "display:flex; justify-content:space-between; width:{s}px; \
                     {LABEL}",
                    LABEL = t.style_label(),
                ),
                if let Some(x_label) = &x_label {
                    span { "X: {x_label}" }
                }
                if let Some(y_label) = &y_label {
                    span { "Y: {y_label}" }
                }
            }
        }
    }
}
