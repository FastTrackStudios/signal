//! Waveform display — renders audio sample data as a centered waveform.

use dioxus::prelude::*;

/// A waveform visualization.
#[derive(Props, Clone, PartialEq)]
pub struct WaveformDisplayProps {
    /// Normalized sample peaks (-1.0 to 1.0). Each entry renders as one column.
    #[props(default)]
    samples: Vec<f64>,

    /// Width in pixels.
    #[props(default = 200)]
    width: u32,

    /// Height in pixels.
    #[props(default = 64)]
    height: u32,

    /// Color CSS class for the waveform bars.
    #[props(default = "bg-primary".to_string())]
    color: String,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn WaveformDisplay(props: WaveformDisplayProps) -> Element {
    let w = props.width;
    let h = props.height;
    let hf = h as f64;
    let mid = hf / 2.0;
    let count = props.samples.len().max(1);
    let bar_w = (w as f64 / count as f64).max(1.0);

    rsx! {
        div {
            class: format!("relative overflow-hidden rounded bg-muted/30 shadow-[inset_0_1px_4px_rgba(0,0,0,0.2)] {}", props.class),
            style: "width: {w}px; height: {h}px;",

            // Center line
            div {
                class: "absolute bg-muted-foreground/20",
                style: "left: 0; top: 50%; width: 100%; height: 1px;",
            }

            // Waveform bars
            for (i, sample) in props.samples.iter().enumerate() {
                {
                    let amp = sample.abs().clamp(0.0, 1.0);
                    let bar_h = (amp * mid).max(1.0);
                    let bar_top = mid - bar_h;
                    let bar_full_h = bar_h * 2.0;
                    let left = i as f64 * bar_w;
                    rsx! {
                        div {
                            class: format!("{} opacity-80", props.color),
                            style: "position: absolute; left: {left:.1}px; top: {bar_top:.1}px; width: {bar_w:.1}px; height: {bar_full_h:.1}px; border-radius: 1px;",
                        }
                    }
                }
            }
        }
    }
}
