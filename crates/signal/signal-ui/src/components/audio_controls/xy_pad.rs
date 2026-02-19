//! XY Pad — 2D parameter control surface.
//!
//! A square area where dragging controls two normalized parameters
//! simultaneously (X and Y, both 0.0–1.0).

use dioxus::prelude::*;

/// 2D control output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct XYValue {
    pub x: f64,
    pub y: f64,
}

/// A 2D parameter control pad.
#[derive(Props, Clone, PartialEq)]
pub struct XYPadProps {
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

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn XYPad(props: XYPadProps) -> Element {
    let s = props.size;
    let sf = s as f64;
    let px = (props.x.clamp(0.0, 1.0) * sf) as u32;
    let py = ((1.0 - props.y.clamp(0.0, 1.0)) * sf) as u32; // Flip Y (top=1)

    let disabled_class = if props.disabled {
        " opacity-50 cursor-not-allowed"
    } else {
        " cursor-crosshair"
    };

    rsx! {
        div {
            class: format!("inline-flex flex-col items-center gap-1 {}", props.class),

            div {
                class: format!(
                    "relative rounded border border-border bg-muted/30 overflow-hidden shadow-[inset_0_2px_8px_rgba(0,0,0,0.3)]{disabled_class}"
                ),
                style: "width: {s}px; height: {s}px;",

                // Crosshair lines
                div {
                    class: "absolute bg-muted-foreground/30",
                    style: "left: {px}px; top: 0; width: 1px; height: 100%;",
                }
                div {
                    class: "absolute bg-muted-foreground/30",
                    style: "left: 0; top: {py}px; width: 100%; height: 1px;",
                }

                // Dot indicator
                div {
                    class: "absolute w-3 h-3 rounded-full bg-primary border-2 border-background shadow-sm",
                    style: "left: {px}px; top: {py}px; transform: translate(-50%, -50%);",
                }
            }

            // Labels
            div {
                class: "flex justify-between w-full text-xs text-muted-foreground",
                style: "width: {s}px;",
                if let Some(x_label) = &props.x_label {
                    span { "X: {x_label}" }
                }
                if let Some(y_label) = &props.y_label {
                    span { "Y: {y_label}" }
                }
            }
        }
    }
}
