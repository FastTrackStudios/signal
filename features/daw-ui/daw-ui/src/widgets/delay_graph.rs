use crate::prelude::*;

// =============================================================================
// Data Types
// =============================================================================

/// Note value for tempo-synced delay.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum NoteValue {
    Whole,
    Half,
    #[default]
    Quarter,
    Eighth,
    Sixteenth,
    DottedQuarter,
    DottedEighth,
    TripletQuarter,
    TripletEighth,
}

impl NoteValue {
    pub fn as_str(&self) -> &'static str {
        match self {
            NoteValue::Whole => "1/1",
            NoteValue::Half => "1/2",
            NoteValue::Quarter => "1/4",
            NoteValue::Eighth => "1/8",
            NoteValue::Sixteenth => "1/16",
            NoteValue::DottedQuarter => "1/4.",
            NoteValue::DottedEighth => "1/8.",
            NoteValue::TripletQuarter => "1/4T",
            NoteValue::TripletEighth => "1/8T",
        }
    }
}

/// Parameters for the delay effect.
#[derive(Clone, PartialEq, Debug)]
pub struct DelayParams {
    pub time_left: f32,  // 1-2000 ms
    pub time_right: f32, // 1-2000 ms
    pub feedback: f32,   // 0-100%
    pub mix: f32,        // 0-100%
    pub ping_pong: bool, // Alternating L/R
    pub sync: bool,      // Tempo sync
    pub note_value: NoteValue,
    pub low_cut: f32,    // 20-500 Hz
    pub high_cut: f32,   // 1000-20000 Hz
    pub modulation: f32, // 0-100%
}

impl Default for DelayParams {
    fn default() -> Self {
        Self {
            time_left: 250.0,
            time_right: 250.0,
            feedback: 30.0,
            mix: 30.0,
            ping_pong: false,
            sync: false,
            note_value: NoteValue::default(),
            low_cut: 20.0,
            high_cut: 20000.0,
            modulation: 0.0,
        }
    }
}

/// Real-time metering data for the delay.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct DelayMetering {
    pub tap_levels: Vec<f32>, // Level of each tap (up to 8 taps, 0-1)
    pub current_tap: usize,   // Currently playing tap index
}

// =============================================================================
// DelayGraph - Graph-only component
// =============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct DelayGraphProps {
    /// Current delay parameters.
    pub params: Signal<DelayParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: DelayMetering,
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
pub fn DelayGraph(props: DelayGraphProps) -> Element {
    let params = props.params.read();

    let width = props.width as f64;
    let height = props.height as f64;
    let padding = 30.0;
    let graph_width = width - padding * 2.0;
    let graph_height = height - padding * 2.0;

    // Calculate number of taps to show (based on feedback)
    let max_taps = 8;
    let tap_count = if params.feedback > 0.0 {
        ((params.feedback / 100.0) * max_taps as f32).max(1.0) as usize
    } else {
        1
    }
    .min(max_taps);

    // Calculate tap levels with decay
    let feedback_factor = params.feedback / 100.0;
    let mut tap_levels: Vec<f32> = (0..tap_count)
        .map(|i| feedback_factor.powi(i as i32))
        .collect();

    // If we have metering data, use it
    if !props.metering.tap_levels.is_empty() {
        for (i, level) in props.metering.tap_levels.iter().enumerate() {
            if i < tap_levels.len() {
                tap_levels[i] = *level;
            }
        }
    }

    // Bar width and spacing
    let bar_spacing = graph_width / (tap_count as f64 + 1.0);
    let bar_width = bar_spacing * 0.6;

    // L/R markers for ping-pong
    let ping_pong_pattern: Vec<&str> = if params.ping_pong {
        (0..tap_count)
            .map(|i| if i % 2 == 0 { "L" } else { "R" })
            .collect()
    } else {
        vec!["L/R"; tap_count]
    };

    // Pre-compute bar positions
    struct BarPosition {
        x: f64,
        y: f64,
        height: f64,
        is_current: bool,
        label: &'static str,
    }
    let bar_positions: Vec<BarPosition> = tap_levels
        .iter()
        .enumerate()
        .map(|(i, level)| {
            let x = padding + bar_spacing * (i as f64 + 0.5) - bar_width / 2.0;
            let bar_height = graph_height * (*level as f64);
            let y = padding + graph_height - bar_height;
            let is_current = i == props.metering.current_tap;
            let label = ping_pong_pattern[i];
            BarPosition {
                x,
                y,
                height: bar_height,
                is_current,
                label,
            }
        })
        .collect();

    // Theme colors (sky blue)
    let bar_color = "#0ea5e9";
    let active_color = "#38bdf8";
    let grid_color = "rgba(255, 255, 255, 0.08)";
    let text_color = "rgba(255, 255, 255, 0.6)";

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
            for i in 0..=4 {
                line {
                    x1: "{padding}",
                    y1: "{padding + (graph_height * i as f64 / 4.0)}",
                    x2: "{padding + graph_width}",
                    y2: "{padding + (graph_height * i as f64 / 4.0)}",
                    stroke: "{grid_color}",
                    stroke_width: "1",
                }
            }

            // Tap bars
            for (i, bar) in bar_positions.iter().enumerate() {
                // Bar
                rect {
                    x: "{bar.x}",
                    y: "{bar.y}",
                    width: "{bar_width}",
                    height: "{bar.height}",
                    fill: if bar.is_current { "{active_color}" } else { "{bar_color}" },
                    rx: "2",
                }

                // Tap number
                text {
                    x: "{bar.x + bar_width / 2.0}",
                    y: "{padding + graph_height + 12.0}",
                    text_anchor: "middle",
                    fill: "{text_color}",
                    font_size: "9",
                    font_family: "system-ui, -apple-system, sans-serif",
                    "{i + 1}"
                }

                // L/R indicator (for ping-pong)
                if params.ping_pong {
                    text {
                        x: "{bar.x + bar_width / 2.0}",
                        y: "{padding + graph_height + 22.0}",
                        text_anchor: "middle",
                        fill: "{bar_color}",
                        font_size: "8",
                        font_family: "system-ui, -apple-system, sans-serif",
                        "{bar.label}"
                    }
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
                if params.ping_pong { "Tap (Ping-Pong)" } else { "Tap" }
            }
            text {
                x: "12",
                y: "{height / 2.0}",
                text_anchor: "middle",
                fill: "{text_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                transform: "rotate(-90, 12, {height / 2.0})",
                "Level"
            }

            // Delay time label
            text {
                x: "{width / 2.0}",
                y: "{padding - 12.0}",
                text_anchor: "middle",
                fill: "{bar_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                font_weight: "600",
                if params.time_left == params.time_right {
                    "{params.time_left:.0}ms"
                } else {
                    "L:{params.time_left:.0} R:{params.time_right:.0}ms"
                }
            }
        }
    }
}

// =============================================================================
// DelayWidget - Full widget with controls
// =============================================================================

use crate::theming::{ThemeContext, ThemeProvider};
use crate::widgets::knob::Knob;

#[derive(Props, Clone, PartialEq)]
pub struct DelayWidgetProps {
    /// Signal for delay parameters (allows two-way binding).
    pub params: Signal<DelayParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: DelayMetering,
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
pub fn DelayWidget(props: DelayWidgetProps) -> Element {
    let mut params = props.params;

    // Create local signals bound to params
    let mut time_left_sig = use_signal(|| params.read().time_left);
    let mut time_right_sig = use_signal(|| params.read().time_right);
    let mut feedback_sig = use_signal(|| params.read().feedback);
    let mut mix_sig = use_signal(|| params.read().mix);
    let mut low_cut_sig = use_signal(|| params.read().low_cut);
    let mut high_cut_sig = use_signal(|| params.read().high_cut);
    let mut modulation_sig = use_signal(|| params.read().modulation);

    // Sync from params to signals when params change externally
    let params_clone = params.read().clone();
    use_effect(move || {
        time_left_sig.set(params_clone.time_left);
        time_right_sig.set(params_clone.time_right);
        feedback_sig.set(params_clone.feedback);
        mix_sig.set(params_clone.mix);
        low_cut_sig.set(params_clone.low_cut);
        high_cut_sig.set(params_clone.high_cut);
        modulation_sig.set(params_clone.modulation);
    });

    // Value formatters
    let format_ms = |v: f32| {
        if v >= 1000.0 {
            format!("{:.1}s", v / 1000.0)
        } else {
            format!("{v:.0}ms")
        }
    };
    let format_percent = |v: f32| format!("{v:.0}%");
    let format_hz = |v: f32| {
        if v >= 1000.0 {
            format!("{:.1}k", v / 1000.0)
        } else {
            format!("{v:.0}Hz")
        }
    };

    // Knob sizes
    let large_knob = 56_u32;
    let tiny_knob = 32_u32;

    // Build current params for the graph
    let mut current_params_sig = use_signal(|| DelayParams {
        time_left: time_left_sig(),
        time_right: time_right_sig(),
        feedback: feedback_sig(),
        mix: mix_sig(),
        low_cut: low_cut_sig(),
        high_cut: high_cut_sig(),
        modulation: modulation_sig(),
        ..params.read().clone()
    });

    // Update signal when params change
    use_effect(move || {
        current_params_sig.set(DelayParams {
            time_left: time_left_sig(),
            time_right: time_right_sig(),
            feedback: feedback_sig(),
            mix: mix_sig(),
            low_cut: low_cut_sig(),
            high_cut: high_cut_sig(),
            modulation: modulation_sig(),
            ..params.read().clone()
        });
    });

    rsx! {
        ThemeProvider { theme: ThemeContext::new(),
            div {
                class: "delay-widget flex gap-3 items-start",
                style: "background: linear-gradient(180deg, #111111 0%, #1a1a1a 100%); border-radius: 8px; padding: 12px;",

                // Left: Time L/R + Feedback knobs
                if props.show_controls {
                    div {
                        class: "flex flex-col items-center gap-2",
                        style: "min-width: 70px;",

                        // Time Left knob
                        Knob {
                            value: time_left_sig,
                            min: 1.0,
                            max: 2000.0,
                            size: large_knob,
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                time_left_sig.set(v);
                                params.write().time_left = v;
                            },
                        }
                        div {
                            class: "flex flex-col items-center gap-0",
                            span { class: "text-[10px] text-gray-500 leading-none", "TIME L" }
                            span { class: "text-[11px] text-gray-300 leading-tight font-mono", "{format_ms(time_left_sig())}" }
                        }

                        // Time Right knob
                        Knob {
                            value: time_right_sig,
                            min: 1.0,
                            max: 2000.0,
                            size: large_knob,
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                time_right_sig.set(v);
                                params.write().time_right = v;
                            },
                        }
                        div {
                            class: "flex flex-col items-center gap-0",
                            span { class: "text-[10px] text-gray-500 leading-none", "TIME R" }
                            span { class: "text-[11px] text-gray-300 leading-tight font-mono", "{format_ms(time_right_sig())}" }
                        }

                        // Feedback knob
                        Knob {
                            value: feedback_sig,
                            min: 0.0,
                            max: 100.0,
                            size: large_knob,
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                feedback_sig.set(v);
                                params.write().feedback = v;
                            },
                        }
                        div {
                            class: "flex flex-col items-center gap-0",
                            span { class: "text-[10px] text-gray-500 leading-none", "FDBK" }
                            span { class: "text-[11px] text-gray-300 leading-tight font-mono", "{format_percent(feedback_sig())}" }
                        }
                    }
                }

                // Center: Graph
                DelayGraph {
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

                        // Low Cut row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: low_cut_sig,
                                min: 20.0,
                                max: 500.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    low_cut_sig.set(v);
                                    params.write().low_cut = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "LO" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_hz(low_cut_sig())}" }
                            }
                        }

                        // High Cut row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: high_cut_sig,
                                min: 1000.0,
                                max: 20000.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    high_cut_sig.set(v);
                                    params.write().high_cut = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "HI" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_hz(high_cut_sig())}" }
                            }
                        }

                        // Modulation row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: modulation_sig,
                                min: 0.0,
                                max: 100.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    modulation_sig.set(v);
                                    params.write().modulation = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "MOD" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_percent(modulation_sig())}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
