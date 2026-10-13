use crate::prelude::*;

// =============================================================================
// Data Types
// =============================================================================

/// Parameters for the flanger effect.
#[derive(Clone, PartialEq, Debug)]
pub struct FlangerParams {
    pub rate: f32,     // 0.01-10 Hz (LFO frequency)
    pub depth: f32,    // 0-100% (modulation depth)
    pub delay: f32,    // 0.1-10 ms (base delay time)
    pub feedback: f32, // -100 to +100% (bipolar feedback)
    pub mix: f32,      // 0-100% (wet/dry mix)
    pub manual: f32,   // 0-1 normalized (manual phase offset)
    pub stereo: f32,   // 0-100% (stereo width/phase offset)
}

impl Default for FlangerParams {
    fn default() -> Self {
        Self {
            rate: 0.5,
            depth: 50.0,
            delay: 2.0,
            feedback: 50.0,
            mix: 50.0,
            manual: 0.5,
            stereo: 50.0,
        }
    }
}

/// Real-time metering data for the flanger.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct FlangerMetering {
    pub lfo_phase: f32,             // 0-1 (current LFO phase)
    pub comb_frequencies: Vec<f32>, // Notch frequencies in Hz
}

// =============================================================================
// Graph Layout Helper
// =============================================================================

#[derive(Copy, Clone)]
struct GraphLayout {
    width: f64,
    height: f64,
    padding: f64,
    graph_width: f64,
    graph_height: f64,
}

impl GraphLayout {
    fn new(width: u32, height: u32) -> Self {
        let width = width as f64;
        let height = height as f64;
        let padding = 40.0;
        let graph_width = width - padding * 2.0;
        let graph_height = height - padding * 2.0;
        Self {
            width,
            height,
            padding,
            graph_width,
            graph_height,
        }
    }

    /// Convert frequency in Hz to X position.
    fn freq_to_x(&self, freq: f64) -> f64 {
        let min_freq = 20.0_f64.ln();
        let max_freq = 20000.0_f64.ln();
        let normalized = (freq.ln() - min_freq) / (max_freq - min_freq);
        self.padding + normalized * self.graph_width
    }

    /// Convert gain in dB to Y position.
    fn db_to_y(&self, db: f64) -> f64 {
        let min_db = -24.0;
        let max_db = 6.0;
        let normalized = (db - min_db) / (max_db - min_db);
        self.padding + self.graph_height * (1.0 - normalized)
    }
}

// =============================================================================
// Comb Filter Response Calculation
// =============================================================================

/// Compute the frequency response of a comb filter.
/// Comb filters create notches at integer multiples of 1/delay.
fn comb_response(freq: f32, delay_ms: f32, feedback: f32, phase_offset: f32) -> f32 {
    let sample_rate = 48000.0;
    let delay_samples = (delay_ms / 1000.0) * sample_rate;

    // Phase for this frequency through the delay line
    let phase = 2.0 * std::f32::consts::PI * freq * delay_samples / sample_rate + phase_offset;

    // Comb filter transfer function magnitude
    // H(z) = 1 + feedback * z^(-delay)
    let fb = feedback / 100.0;
    let magnitude = ((1.0 + fb * phase.cos()).powi(2) + (fb * phase.sin()).powi(2)).sqrt();

    // Convert to dB
    20.0 * magnitude.log10()
}

// =============================================================================
// FlangerGraph - Graph-only component
// =============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct FlangerGraphProps {
    /// Current flanger parameters.
    pub params: Signal<FlangerParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: FlangerMetering,
    /// Graph width in pixels.
    #[props(default = 280)]
    pub width: u32,
    /// Graph height in pixels.
    #[props(default = 200)]
    pub height: u32,
    /// Whether to show the grid.
    #[props(default = true)]
    pub show_grid: bool,
    /// Whether interaction is enabled.
    #[props(default = true)]
    pub interactive: bool,
}

#[component]
pub fn FlangerGraph(props: FlangerGraphProps) -> Element {
    let params = props.params.read();
    let layout = GraphLayout::new(props.width, props.height);

    // Calculate modulated delay time based on LFO phase
    let lfo_value = (props.metering.lfo_phase * 2.0 * std::f32::consts::PI).sin();
    let modulation_amount = params.depth / 100.0;
    let current_delay = params.delay * (1.0 + lfo_value * modulation_amount);

    // Manual phase offset
    let phase_offset = params.manual * 2.0 * std::f32::consts::PI;

    // Generate frequency response curve (showing comb filter effect)
    let num_points = 200;
    let mut curve_path = String::new();

    for i in 0..=num_points {
        let t = i as f64 / num_points as f64;
        let freq = (20.0_f64 * (20000.0_f64 / 20.0_f64).powf(t)) as f32;

        // Comb filter response with current modulated delay
        let response = comb_response(freq, current_delay, params.feedback, phase_offset);

        let x = layout.freq_to_x(freq as f64);
        let y = layout.db_to_y(response as f64);

        if i == 0 {
            curve_path.push_str(&format!("M {x:.1} {y:.1}"));
        } else {
            curve_path.push_str(&format!(" L {x:.1} {y:.1}"));
        }
    }

    // Unity line (0 dB)
    let unity_y = layout.db_to_y(0.0);

    // LFO phase indicator (small circle on left side)
    let lfo_indicator_x = layout.padding - 20.0;
    let lfo_indicator_y =
        layout.padding + layout.graph_height / 2.0 + (layout.graph_height / 3.0) * lfo_value as f64;

    // Frequency grid markers (logarithmic)
    let freq_markers = vec![100.0, 1000.0, 10000.0];

    // Theme colors (magenta)
    let curve_color = "#ec4899";
    let unity_color = "rgba(255, 255, 255, 0.2)";
    let grid_color = "rgba(255, 255, 255, 0.08)";
    let lfo_color = "#f472b6";
    let text_color = "rgba(255, 255, 255, 0.6)";

    rsx! {
        svg {
            width: "{props.width}",
            height: "{props.height}",
            view_box: "0 0 {layout.width} {layout.height}",

            // Background
            rect {
                width: "{layout.width}",
                height: "{layout.height}",
                fill: "#0a0a0a",
            }

            // Grid
            if props.show_grid {
                // Frequency grid lines
                for freq in freq_markers.iter() {
                    line {
                        x1: "{layout.freq_to_x(*freq as f64)}",
                        y1: "{layout.padding}",
                        x2: "{layout.freq_to_x(*freq as f64)}",
                        y2: "{layout.padding + layout.graph_height}",
                        stroke: "{grid_color}",
                        stroke_width: "1",
                    }
                }
                // dB grid lines
                for db in [-18.0, -12.0, -6.0, 0.0].iter() {
                    line {
                        x1: "{layout.padding}",
                        y1: "{layout.db_to_y(*db)}",
                        x2: "{layout.padding + layout.graph_width}",
                        y2: "{layout.db_to_y(*db)}",
                        stroke: "{grid_color}",
                        stroke_width: "1",
                    }
                }
            }

            // Unity line (0 dB)
            line {
                x1: "{layout.padding}",
                y1: "{unity_y}",
                x2: "{layout.padding + layout.graph_width}",
                y2: "{unity_y}",
                stroke: "{unity_color}",
                stroke_width: "1.5",
                stroke_dasharray: "4,4",
            }

            // Frequency response curve
            path {
                d: "{curve_path}",
                stroke: "{curve_color}",
                stroke_width: "2.5",
                stroke_linecap: "round",
                fill: "none",
            }

            // LFO phase indicator
            // Background track
            line {
                x1: "{lfo_indicator_x}",
                y1: "{layout.padding}",
                x2: "{lfo_indicator_x}",
                y2: "{layout.padding + layout.graph_height}",
                stroke: "{grid_color}",
                stroke_width: "4",
            }
            // Moving indicator
            circle {
                cx: "{lfo_indicator_x}",
                cy: "{lfo_indicator_y}",
                r: "6",
                fill: "{lfo_color}",
                stroke: "#fff",
                stroke_width: "1.5",
            }

            // Axis labels
            text {
                x: "{layout.width / 2.0}",
                y: "{layout.height - 8.0}",
                text_anchor: "middle",
                fill: "{text_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                "Frequency (Hz)"
            }
            text {
                x: "12",
                y: "{layout.height / 2.0}",
                text_anchor: "middle",
                fill: "{text_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                transform: "rotate(-90, 12, {layout.height / 2.0})",
                "Gain (dB)"
            }

            // Delay time label
            text {
                x: "{layout.width / 2.0}",
                y: "{layout.padding - 12.0}",
                text_anchor: "middle",
                fill: "{curve_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                font_weight: "600",
                "Delay: {current_delay:.2}ms"
            }

            // Frequency markers
            for freq in freq_markers.iter() {
                text {
                    x: "{layout.freq_to_x(*freq as f64)}",
                    y: "{layout.padding + layout.graph_height + 14.0}",
                    text_anchor: "middle",
                    fill: "{text_color}",
                    font_size: "9",
                    font_family: "system-ui, -apple-system, sans-serif",
                    if *freq >= 1000.0 {
                        "{(*freq / 1000.0) as i32}k"
                    } else {
                        "{*freq as i32}"
                    }
                }
            }

            // LFO label
            text {
                x: "{lfo_indicator_x}",
                y: "{layout.padding - 8.0}",
                text_anchor: "middle",
                fill: "{lfo_color}",
                font_size: "9",
                font_family: "system-ui, -apple-system, sans-serif",
                "LFO"
            }
        }
    }
}

// =============================================================================
// FlangerWidget - Full widget with controls
// =============================================================================

use crate::theming::{ThemeContext, ThemeProvider};
use crate::widgets::knob::Knob;

#[derive(Props, Clone, PartialEq)]
pub struct FlangerWidgetProps {
    /// Signal for flanger parameters (allows two-way binding).
    pub params: Signal<FlangerParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: FlangerMetering,
    /// Size of the graph in pixels.
    #[props(default = 280)]
    pub graph_width: u32,
    #[props(default = 200)]
    pub graph_height: u32,
    /// Whether to show the grid.
    #[props(default = true)]
    pub show_grid: bool,
    /// Whether to show the knob controls panel.
    #[props(default = true)]
    pub show_controls: bool,
    /// Whether interaction is enabled.
    #[props(default = true)]
    pub interactive: bool,
}

#[component]
pub fn FlangerWidget(props: FlangerWidgetProps) -> Element {
    let mut params = props.params;

    // Create local signals bound to params
    let mut rate_sig = use_signal(|| params.read().rate);
    let mut depth_sig = use_signal(|| params.read().depth);
    let mut manual_sig = use_signal(|| params.read().manual);
    let mut feedback_sig = use_signal(|| params.read().feedback);
    let mut mix_sig = use_signal(|| params.read().mix);
    let mut stereo_sig = use_signal(|| params.read().stereo);

    // Sync from params to signals when params change externally
    let params_clone = params.read().clone();
    use_effect(move || {
        rate_sig.set(params_clone.rate);
        depth_sig.set(params_clone.depth);
        manual_sig.set(params_clone.manual);
        feedback_sig.set(params_clone.feedback);
        mix_sig.set(params_clone.mix);
        stereo_sig.set(params_clone.stereo);
    });

    // Value formatters
    let format_hz = |v: f32| format!("{v:.2}Hz");
    let format_percent = |v: f32| format!("{v:.0}%");
    let format_bipolar = |v: f32| {
        if v >= 0.0 {
            format!("+{v:.0}%")
        } else {
            format!("{v:.0}%")
        }
    };
    let format_normalized = |v: f32| format!("{:.0}", v * 100.0);

    // Knob sizes
    let large_knob = 56_u32;
    let tiny_knob = 32_u32;

    // Build current params for the graph
    let mut current_params_sig = use_signal(|| FlangerParams {
        rate: rate_sig(),
        depth: depth_sig(),
        manual: manual_sig(),
        feedback: feedback_sig(),
        mix: mix_sig(),
        stereo: stereo_sig(),
        ..params.read().clone()
    });

    // Update signal when params change
    use_effect(move || {
        current_params_sig.set(FlangerParams {
            rate: rate_sig(),
            depth: depth_sig(),
            manual: manual_sig(),
            feedback: feedback_sig(),
            mix: mix_sig(),
            stereo: stereo_sig(),
            ..params.read().clone()
        });
    });

    rsx! {
        ThemeProvider { theme: ThemeContext::new(),
            div {
                class: "flanger-widget flex gap-3 items-start",
                style: "background: linear-gradient(180deg, #111111 0%, #1a1a1a 100%); border-radius: 8px; padding: 12px;",

                // Left: Rate, Depth, Manual knobs
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

                        // Manual knob
                        Knob {
                            value: manual_sig,
                            min: 0.0,
                            max: 1.0,
                            size: large_knob,
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                manual_sig.set(v);
                                params.write().manual = v;
                            },
                        }
                        div {
                            class: "flex flex-col items-center gap-0",
                            span { class: "text-[10px] text-gray-500 leading-none", "MANUAL" }
                            span { class: "text-[11px] text-gray-300 leading-tight font-mono", "{format_normalized(manual_sig())}" }
                        }
                    }
                }

                // Center: Graph
                FlangerGraph {
                    params: current_params_sig,
                    metering: props.metering.clone(),
                    width: props.graph_width,
                    height: props.graph_height,
                    show_grid: props.show_grid,
                    interactive: props.interactive,
                }

                // Right: Feedback (bipolar), Mix, Stereo
                if props.show_controls {
                    div {
                        class: "flex flex-col gap-0",
                        style: "padding-left: 8px; border-left: 1px solid rgba(255,255,255,0.08);",

                        // Feedback row (bipolar)
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: feedback_sig,
                                min: -100.0,
                                max: 100.0,
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
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_bipolar(feedback_sig())}" }
                            }
                        }

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

                        // Stereo row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: stereo_sig,
                                min: 0.0,
                                max: 100.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    stereo_sig.set(v);
                                    params.write().stereo = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "STEREO" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_percent(stereo_sig())}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
