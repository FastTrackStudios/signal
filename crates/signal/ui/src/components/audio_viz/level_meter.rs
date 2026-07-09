//! Level meter — horizontal or vertical bar with peak hold and color zones.

use dioxus::prelude::*;

/// Meter orientation.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LevelMeterOrientation {
    #[default]
    Vertical,
    Horizontal,
}

/// A multi-zone level meter with optional peak hold indicator.
#[derive(Props, Clone, PartialEq)]
pub struct LevelMeterProps {
    /// Current level (0.0–1.0 normalized, 1.0 = 0 dBFS).
    #[props(default = 0.0)]
    level: f64,

    /// Peak hold level (0.0–1.0). Shown as a thin line.
    #[props(default)]
    peak: Option<f64>,

    /// Orientation.
    #[props(default)]
    orientation: LevelMeterOrientation,

    /// Whether to show the clipping indicator at 1.0.
    #[props(default = true)]
    show_clip: bool,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn LevelMeter(props: LevelMeterProps) -> Element {
    let level = props.level.clamp(0.0, 1.0);
    let pct = level * 100.0;
    let is_clip = level >= 0.99;

    let is_vertical = props.orientation == LevelMeterOrientation::Vertical;

    let container_class = if is_vertical {
        format!(
            "relative bg-muted rounded overflow-hidden w-3 h-full {}",
            props.class
        )
    } else {
        format!(
            "relative bg-muted rounded overflow-hidden h-3 w-full {}",
            props.class
        )
    };

    // Color: safe < 75%, warn 75-90%, danger 90%+
    let bar_color = if level > 0.9 {
        "background-color: var(--signal-danger)"
    } else if level > 0.75 {
        "background-color: var(--signal-warn)"
    } else {
        "background-color: var(--signal-safe)"
    };

    let bar_style = if is_vertical {
        format!("height: {pct}%; width: 100%; position: absolute; bottom: 0;")
    } else {
        format!("width: {pct}%; height: 100%;")
    };

    rsx! {
        div {
            class: container_class,
            role: "meter",
            aria_valuemin: "0",
            aria_valuemax: "1",
            aria_valuenow: "{level:.3}",

            // Fill bar
            div {
                class: "transition-all duration-75",
                style: format!("{bar_style} {bar_color}"),
            }

            // Peak hold indicator
            if let Some(peak) = props.peak {
                {
                    let peak_pct = (peak.clamp(0.0, 1.0) * 100.0);
                    let peak_style = if is_vertical {
                        format!("bottom: {peak_pct}%; width: 100%; height: 2px; position: absolute;")
                    } else {
                        format!("left: {peak_pct}%; height: 100%; width: 2px; position: absolute;")
                    };
                    rsx! {
                        div {
                            class: "bg-foreground/60",
                            style: peak_style,
                        }
                    }
                }
            }

            // Clip indicator
            if props.show_clip && is_clip {
                div {
                    class: "absolute inset-0 animate-pulse",
                    style: "background-color: color-mix(in oklch, var(--signal-danger) 30%, transparent)",
                }
            }
        }
    }
}
