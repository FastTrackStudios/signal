use crate::prelude::*;

// =============================================================================
// Data Types
// =============================================================================

/// Parameters for the chorus effect.
#[derive(Clone, PartialEq, Debug)]
pub struct ChorusParams {
    pub rate: f32,     // 0.01-10 Hz
    pub depth: f32,    // 0-100%
    pub delay: f32,    // 1-40ms
    pub mix: f32,      // 0-100%
    pub voices: u8,    // 1-4
    pub spread: f32,   // 0-100%
    pub feedback: f32, // 0-50%
}

impl Default for ChorusParams {
    fn default() -> Self {
        Self {
            rate: 1.0,
            depth: 50.0,
            delay: 20.0,
            mix: 50.0,
            voices: 2,
            spread: 50.0,
            feedback: 0.0,
        }
    }
}

/// Real-time metering data for the chorus.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct ChorusMetering {
    pub lfo_phase: f32,          // 0-1, current phase of LFO
    pub current_delay: Vec<f32>, // Current delay time for each voice (ms)
}

// =============================================================================
// ChorusGraph - Graph-only component
// =============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct ChorusGraphProps {
    /// Current chorus parameters.
    pub params: Signal<ChorusParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: ChorusMetering,
    /// Graph width in pixels.
    #[props(default = 240)]
    pub width: u32,
    /// Graph height in pixels.
    #[props(default = 200)]
    pub height: u32,
    /// Whether interaction is enabled.
    #[props(default = true)]
    pub interactive: bool,
}

#[component]
pub fn ChorusGraph(props: ChorusGraphProps) -> Element {
    let params = props.params.read();

    let width = props.width as f64;
    let height = props.height as f64;
    let padding = 30.0;
    let graph_width = width - padding * 2.0;
    let graph_height = height - padding * 2.0;

    // LFO waveform points (sine wave)
    let num_points = 100;
    let mut path_data = String::from("M");

    for i in 0..=num_points {
        let x = padding + (i as f64 / num_points as f64) * graph_width;
        let phase = (i as f64 / num_points as f64) * std::f64::consts::PI * 2.0;
        let y_normalized = (phase.sin() + 1.0) / 2.0; // Normalize to 0-1
        let y = padding + graph_height - (y_normalized * graph_height);

        if i == 0 {
            path_data.push_str(&format!("{x:.2},{y:.2}"));
        } else {
            path_data.push_str(&format!(" L{x:.2},{y:.2}"));
        }
    }

    // Calculate current phase position
    let phase_x = padding + (props.metering.lfo_phase as f64) * graph_width;
    let phase_angle = (props.metering.lfo_phase as f64) * std::f64::consts::PI * 2.0;
    let phase_y_normalized = (phase_angle.sin() + 1.0) / 2.0;
    let phase_y = padding + graph_height - (phase_y_normalized * graph_height);

    // Calculate voice delay line positions
    let voice_positions: Vec<(f64, f64)> = (0..params.voices)
        .map(|i| {
            let spread_offset = if params.voices > 1 {
                (i as f32 / (params.voices - 1) as f32 - 0.5) * (params.spread / 100.0)
            } else {
                0.0
            };

            let voice_phase = props.metering.lfo_phase + spread_offset;
            let voice_phase = voice_phase - voice_phase.floor(); // Wrap to 0-1

            let x = padding + (voice_phase as f64) * graph_width;
            let angle = (voice_phase as f64) * std::f64::consts::PI * 2.0;
            let y_normalized = (angle.sin() + 1.0) / 2.0;
            let y = padding + graph_height - (y_normalized * graph_height);

            (x, y)
        })
        .collect();

    // Theme colors (green)
    let waveform_color = "#22c55e";
    let waveform_dim_color = "#166534";
    let phase_color = "#4ade80";
    let voice_color = "#86efac";
    let grid_color = "rgba(255, 255, 255, 0.08)";
    let text_color = "rgba(255, 255, 255, 0.6)";

    // Pre-compute grid line positions
    let grid_y_positions: Vec<f64> = (0..=4)
        .map(|i| padding + (graph_height * i as f64 / 4.0))
        .collect();

    // Pre-compute depth fill path
    let depth_fill_path = if params.depth > 0.0 {
        let depth_factor = params.depth as f64 / 100.0;
        let center_y = padding + graph_height / 2.0;
        let mut fill_path = String::from("M");

        // Build path for filled area
        for i in 0..=num_points {
            let x = padding + (i as f64 / num_points as f64) * graph_width;
            let phase = (i as f64 / num_points as f64) * std::f64::consts::PI * 2.0;
            let y_offset = phase.sin() * depth_factor * (graph_height / 2.0);
            let y = center_y - y_offset;

            if i == 0 {
                fill_path.push_str(&format!("{x:.2},{y:.2}"));
            } else {
                fill_path.push_str(&format!(" L{x:.2},{y:.2}"));
            }
        }

        // Close path along center line
        fill_path.push_str(&format!(
            " L{},{} L{},{} Z",
            padding + graph_width,
            center_y,
            padding,
            center_y
        ));

        fill_path
    } else {
        String::new()
    };

    // Pre-compute rate label text
    let rate_label = format!(
        "{:.2}Hz · {} voice{}",
        params.rate,
        params.voices,
        if params.voices > 1 { "s" } else { "" }
    );

    rsx! {
        svg {
            width: "{props.width}",
            height: "{props.height}",
            view_box: "0 0 {width} {height}",

            // Background
            rect {
                width: "{width}",
                height: "{height}",
                fill: "#0a0a0a",
            }

            // Horizontal grid lines
            for y in grid_y_positions.iter() {
                line {
                    x1: "{padding}",
                    y1: "{y}",
                    x2: "{padding + graph_width}",
                    y2: "{y}",
                    stroke: "{grid_color}",
                    stroke_width: "1",
                }
            }

            // Center line (no modulation)
            line {
                x1: "{padding}",
                y1: "{padding + graph_height / 2.0}",
                x2: "{padding + graph_width}",
                y2: "{padding + graph_height / 2.0}",
                stroke: "rgba(255, 255, 255, 0.15)",
                stroke_width: "1",
                stroke_dasharray: "4 4",
            }

            // Depth indicator (filled area under waveform based on depth)
            if !depth_fill_path.is_empty() {
                path {
                    d: "{depth_fill_path}",
                    fill: "{waveform_dim_color}",
                    opacity: "0.3",
                }
            }

            // LFO waveform
            path {
                d: "{path_data}",
                stroke: "{waveform_color}",
                stroke_width: "2",
                fill: "none",
            }

            // Current phase indicator (vertical line)
            line {
                x1: "{phase_x}",
                y1: "{padding}",
                x2: "{phase_x}",
                y2: "{padding + graph_height}",
                stroke: "{phase_color}",
                stroke_width: "2",
                opacity: "0.7",
            }

            // Phase position dot
            circle {
                cx: "{phase_x}",
                cy: "{phase_y}",
                r: "4",
                fill: "{phase_color}",
            }

            // Voice delay lines
            for (i, (vx, vy)) in voice_positions.iter().enumerate() {
                // Vertical line for each voice
                line {
                    x1: "{vx}",
                    y1: "{padding}",
                    x2: "{vx}",
                    y2: "{padding + graph_height}",
                    stroke: "{voice_color}",
                    stroke_width: "1",
                    opacity: "0.5",
                    stroke_dasharray: "2 2",
                }

                // Voice position dot
                circle {
                    cx: "{vx}",
                    cy: "{vy}",
                    r: "3",
                    fill: "{voice_color}",
                    opacity: "0.8",
                }

                // Voice number label
                text {
                    x: "{vx}",
                    y: "{padding - 5.0}",
                    text_anchor: "middle",
                    fill: "{voice_color}",
                    font_size: "9",
                    font_family: "system-ui, -apple-system, sans-serif",
                    "V{i + 1}"
                }
            }

            // Axis labels
            text {
                x: "{width / 2.0}",
                y: "{height - 8.0}",
                text_anchor: "middle",
                fill: "{text_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                "LFO Phase"
            }
            text {
                x: "12",
                y: "{height / 2.0}",
                text_anchor: "middle",
                fill: "{text_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                transform: "rotate(-90, 12, {height / 2.0})",
                "Delay"
            }

            // Rate label
            text {
                x: "{width / 2.0}",
                y: "{padding - 12.0}",
                text_anchor: "middle",
                fill: "{waveform_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                font_weight: "600",
                "{rate_label}"
            }
        }
    }
}

// =============================================================================
// ChorusWidget - Full widget with controls
// =============================================================================

use crate::theming::{ThemeContext, ThemeProvider};
use crate::widgets::knob::Knob;

#[derive(Props, Clone, PartialEq)]
pub struct ChorusWidgetProps {
    /// Signal for chorus parameters (allows two-way binding).
    pub params: Signal<ChorusParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: ChorusMetering,
    /// Size of the graph in pixels.
    #[props(default = 240)]
    pub graph_width: u32,
    #[props(default = 200)]
    pub graph_height: u32,
    /// Whether to show the knob controls panel.
    #[props(default = true)]
    pub show_controls: bool,
    /// Whether interaction is enabled.
    #[props(default = true)]
    pub interactive: bool,
}

#[component]
pub fn ChorusWidget(props: ChorusWidgetProps) -> Element {
    let mut params = props.params;

    // Create local signals bound to params
    let mut rate_sig = use_signal(|| params.read().rate);
    let mut depth_sig = use_signal(|| params.read().depth);
    let mut delay_sig = use_signal(|| params.read().delay);
    let mut mix_sig = use_signal(|| params.read().mix);
    let mut voices_sig = use_signal(|| params.read().voices as f32);
    let mut spread_sig = use_signal(|| params.read().spread);
    let mut feedback_sig = use_signal(|| params.read().feedback);

    // Sync from params to signals when params change externally
    let params_clone = params.read().clone();
    use_effect(move || {
        rate_sig.set(params_clone.rate);
        depth_sig.set(params_clone.depth);
        delay_sig.set(params_clone.delay);
        mix_sig.set(params_clone.mix);
        voices_sig.set(params_clone.voices as f32);
        spread_sig.set(params_clone.spread);
        feedback_sig.set(params_clone.feedback);
    });

    // Value formatters
    let format_hz = |v: f32| format!("{v:.2}Hz");
    let format_percent = |v: f32| format!("{v:.0}%");
    let format_ms = |v: f32| format!("{v:.1}ms");
    let format_voices = |v: f32| format!("{}", v as u8);

    // Knob sizes
    let large_knob = 56_u32;
    let tiny_knob = 32_u32;

    // Build current params for the graph
    let mut current_params_sig = use_signal(|| ChorusParams {
        rate: rate_sig(),
        depth: depth_sig(),
        delay: delay_sig(),
        mix: mix_sig(),
        voices: voices_sig() as u8,
        spread: spread_sig(),
        feedback: feedback_sig(),
    });

    // Update signal when params change
    use_effect(move || {
        current_params_sig.set(ChorusParams {
            rate: rate_sig(),
            depth: depth_sig(),
            delay: delay_sig(),
            mix: mix_sig(),
            voices: voices_sig() as u8,
            spread: spread_sig(),
            feedback: feedback_sig(),
        });
    });

    rsx! {
        ThemeProvider { theme: ThemeContext::new(),
            div {
                class: "chorus-widget flex gap-3 items-start",
                style: "background: linear-gradient(180deg, #111111 0%, #1a1a1a 100%); border-radius: 8px; padding: 12px;",

                // Left: Rate + Depth + Delay knobs
                if props.show_controls {
                    div {
                        class: "flex flex-col items-center gap-2",
                        style: "min-width: 70px;",

                        // Rate knob
                        Knob {
                            value: rate_sig,
                            min: 0.01,
                            max: 10.0,
                            size: large_knob,
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                rate_sig.set(v);
                                params.write().rate = v;
                            },
                        }
                        div {
                            class: "flex flex-col items-center gap-0",
                            span { class: "text-[10px] text-gray-500 leading-none", "RATE" }
                            span { class: "text-[11px] text-gray-300 leading-tight font-mono", "{format_hz(rate_sig())}" }
                        }

                        // Depth knob
                        Knob {
                            value: depth_sig,
                            min: 0.0,
                            max: 100.0,
                            size: large_knob,
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                depth_sig.set(v);
                                params.write().depth = v;
                            },
                        }
                        div {
                            class: "flex flex-col items-center gap-0",
                            span { class: "text-[10px] text-gray-500 leading-none", "DEPTH" }
                            span { class: "text-[11px] text-gray-300 leading-tight font-mono", "{format_percent(depth_sig())}" }
                        }

                        // Delay knob
                        Knob {
                            value: delay_sig,
                            min: 1.0,
                            max: 40.0,
                            size: large_knob,
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                delay_sig.set(v);
                                params.write().delay = v;
                            },
                        }
                        div {
                            class: "flex flex-col items-center gap-0",
                            span { class: "text-[10px] text-gray-500 leading-none", "DELAY" }
                            span { class: "text-[11px] text-gray-300 leading-tight font-mono", "{format_ms(delay_sig())}" }
                        }
                    }
                }

                // Center: Graph
                ChorusGraph {
                    params: current_params_sig,
                    metering: props.metering.clone(),
                    width: props.graph_width,
                    height: props.graph_height,
                    interactive: props.interactive,
                }

                // Right: Secondary controls (compact horizontal rows)
                if props.show_controls {
                    div {
                        class: "flex flex-col gap-0",
                        style: "padding-left: 8px; border-left: 1px solid rgba(255,255,255,0.08);",

                        // Mix row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: mix_sig,
                                min: 0.0,
                                max: 100.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    mix_sig.set(v);
                                    params.write().mix = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "MIX" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_percent(mix_sig())}" }
                            }
                        }

                        // Voices row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: voices_sig,
                                min: 1.0,
                                max: 4.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    let voices_val = v.round().clamp(1.0, 4.0) as u8;
                                    voices_sig.set(voices_val as f32);
                                    params.write().voices = voices_val;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "VOICES" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_voices(voices_sig())}" }
                            }
                        }

                        // Spread row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: spread_sig,
                                min: 0.0,
                                max: 100.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    spread_sig.set(v);
                                    params.write().spread = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "SPREAD" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_percent(spread_sig())}" }
                            }
                        }

                        // Feedback row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: feedback_sig,
                                min: 0.0,
                                max: 50.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    feedback_sig.set(v);
                                    params.write().feedback = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "FDBK" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_percent(feedback_sig())}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
