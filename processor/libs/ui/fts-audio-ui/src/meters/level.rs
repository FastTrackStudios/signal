//! Level meter — vertical or horizontal bar with peak hold and color zones.

use crate::theme::*;
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LevelMeterOrientation {
    #[default]
    Vertical,
    Horizontal,
}

/// Multi-zone level meter with optional peak-hold indicator.
///
/// Color zones: safe (green) <75%, warn (amber) 75–90%, danger (red) >90%.
/// Accepts a normalized 0–1 level. Use [`LevelMeterDb`] for dBFS input.
#[component]
pub fn LevelMeter(
    #[props(default = 0.0)] level: f64,
    #[props(default)] peak: Option<f64>,
    #[props(default)] orientation: LevelMeterOrientation,
    #[props(default = true)] show_clip: bool,
    #[props(default = 10.0)] thickness: f32,
    #[props(default = 200.0)] length: f32,
    #[props(default)] label: Option<String>,
) -> Element {
    let level = level.clamp(0.0, 1.0);
    let pct = level * 100.0;
    let is_clip = level >= 0.99;
    let is_vertical = orientation == LevelMeterOrientation::Vertical;

    let bar_color = if level > 0.9 {
        SIGNAL_DANGER
    } else if level > 0.75 {
        SIGNAL_WARN
    } else {
        SIGNAL_SAFE
    };

    let container_style = if is_vertical {
        "display:flex; flex-direction:column; align-items:center; gap:4px;".to_string()
    } else {
        "display:flex; align-items:center; gap:4px;".to_string()
    };

    let meter_style = if is_vertical {
        format!(
            "position:relative; width:{thickness}px; height:{length}px; \
             background:{SURFACE}; border-radius:3px; overflow:hidden; \
             border:1px solid {BORDER};"
        )
    } else {
        format!(
            "position:relative; height:{thickness}px; width:{length}px; \
             background:{SURFACE}; border-radius:3px; overflow:hidden; \
             border:1px solid {BORDER};"
        )
    };

    let bar_style = if is_vertical {
        format!(
            "position:absolute; bottom:0; left:0; right:0; height:{pct}%; \
             background:{bar_color};"
        )
    } else {
        format!("width:{pct}%; height:100%; background:{bar_color};")
    };

    rsx! {
        div {
            style: container_style,

            if let Some(label) = &label {
                div {
                    style: format!(
                        "font-size:10px; color:{TEXT_DIM}; text-transform:uppercase;"
                    ),
                    "{label}"
                }
            }

            div {
                style: meter_style,
                div { style: bar_style }

                if let Some(peak) = peak {
                    {
                        let peak_pct = peak.clamp(0.0, 1.0) * 100.0;
                        let peak_style = if is_vertical {
                            format!(
                                "position:absolute; bottom:{peak_pct}%; left:0; right:0; \
                                 height:2px; background:rgba(224,224,224,0.6);"
                            )
                        } else {
                            format!(
                                "position:absolute; left:{peak_pct}%; top:0; bottom:0; \
                                 width:2px; background:rgba(224,224,224,0.6);"
                            )
                        };
                        rsx! { div { style: peak_style } }
                    }
                }

                if show_clip && is_clip {
                    div {
                        style: "position:absolute; inset:0; background:rgba(248,113,113,0.3);"
                    }
                }
            }
        }
    }
}

/// Vertical level meter that accepts dBFS input.
#[component]
pub fn LevelMeterDb(
    level_db: f32,
    #[props(default)] label: Option<String>,
    #[props(default = -60.0)] min_db: f32,
    #[props(default = 200.0)] height: f32,
    #[props(default = 10.0)] width: f32,
) -> Element {
    let range = 0.0 - min_db;
    let normalized = ((level_db - min_db) / range).clamp(0.0, 1.0) as f64;
    let level_text = format!("{:.1}", level_db);

    rsx! {
        div {
            style: "display:flex; flex-direction:column; align-items:center; gap:4px; min-width:36px;",
            LevelMeter {
                level: normalized,
                orientation: LevelMeterOrientation::Vertical,
                thickness: width,
                length: height,
                label,
            }
            div {
                style: format!(
                    "font-size:10px; color:{TEXT_DIM}; font-variant-numeric:tabular-nums; \
                     min-width:36px; text-align:center;"
                ),
                "{level_text}"
            }
        }
    }
}
