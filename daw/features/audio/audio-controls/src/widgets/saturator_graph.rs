use crate::prelude::*;

// =============================================================================
// Data Types
// =============================================================================

/// Saturation mode type.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum SaturationMode {
    #[default]
    Tape,
    Tube,
    Transistor,
    Fuzz,
    Soft,
    Hard,
}

impl SaturationMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SaturationMode::Tape => "Tape",
            SaturationMode::Tube => "Tube",
            SaturationMode::Transistor => "Transistor",
            SaturationMode::Fuzz => "Fuzz",
            SaturationMode::Soft => "Soft",
            SaturationMode::Hard => "Hard",
        }
    }
}

/// Parameters for the saturator effect.
#[derive(Clone, PartialEq, Debug)]
pub struct SaturatorParams {
    pub drive: f32,  // 0-48 dB
    pub mix: f32,    // 0-100%
    pub output: f32, // -24 to +6 dB
    pub mode: SaturationMode,
    pub tone: f32,     // -100 to +100 (low to high emphasis)
    pub dynamics: f32, // 0-100% (preserve transients)
}

impl Default for SaturatorParams {
    fn default() -> Self {
        Self {
            drive: 12.0,
            mix: 100.0,
            output: 0.0,
            mode: SaturationMode::default(),
            tone: 0.0,
            dynamics: 50.0,
        }
    }
}

/// Real-time metering data for the saturator.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct SaturatorMetering {
    pub input_level: f32,      // dB
    pub output_level: f32,     // dB
    pub harmonic_content: f32, // 0-1 (amount of added harmonics)
}

// =============================================================================
// Graph Layout Helper
// =============================================================================

struct GraphLayout {
    width: f64,
    height: f64,
    padding: f64,
    graph_size: f64,
}

impl GraphLayout {
    fn new(width: u32, height: u32) -> Self {
        let width = width as f64;
        let height = height as f64;
        let padding = 40.0;
        let graph_size = width.min(height) - padding * 2.0;
        Self {
            width,
            height,
            padding,
            graph_size,
        }
    }

    fn db_to_x(&self, db: f64, min_db: f64) -> f64 {
        self.padding + ((db - min_db) / -min_db) * self.graph_size
    }

    fn db_to_y(&self, db: f64, min_db: f64) -> f64 {
        self.padding + (1.0 - (db - min_db) / -min_db) * self.graph_size
    }
}

// =============================================================================
// Transfer Function
// =============================================================================

/// Compute saturated output given input and parameters.
fn saturate(input_db: f32, drive_db: f32, mode: SaturationMode) -> f32 {
    // Apply drive
    let driven = input_db + drive_db;

    // Convert to linear for saturation
    let linear = 10_f32.powf(driven / 20.0);

    // Apply saturation curve based on mode
    let saturated = match mode {
        SaturationMode::Tape => {
            // Soft tape saturation (tanh)
            linear.tanh()
        }
        SaturationMode::Tube => {
            // Tube-style asymmetric saturation
            if linear >= 0.0 {
                linear / (1.0 + linear.abs().powf(1.5))
            } else {
                linear / (1.0 + linear.abs().powf(2.0))
            }
        }
        SaturationMode::Transistor => {
            // Hard clipping with soft knee
            let threshold = 0.8;
            if linear.abs() < threshold {
                linear
            } else {
                let sign = linear.signum();
                sign * (threshold
                    + (linear.abs() - threshold) / (1.0 + 2.0 * (linear.abs() - threshold)))
            }
        }
        SaturationMode::Fuzz => {
            // Hard clipping
            linear.clamp(-1.0, 1.0)
        }
        SaturationMode::Soft => {
            // Soft saturation (cubic)
            if linear.abs() < 1.0 {
                linear - (linear.powi(3) / 3.0)
            } else {
                linear.signum() * 0.666
            }
        }
        SaturationMode::Hard => {
            // Arctan saturation
            (std::f32::consts::PI / 2.0) * (linear * 0.5).atan()
        }
    };

    // Convert back to dB
    (saturated.abs().max(1e-6)).log10() * 20.0
}

// =============================================================================
// SaturatorGraph - Graph-only component
// =============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct SaturatorGraphProps {
    /// Current saturator parameters.
    pub params: Signal<SaturatorParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: SaturatorMetering,
    /// Graph width in pixels.
    #[props(default = 200)]
    pub width: u32,
    /// Graph height in pixels.
    #[props(default = 200)]
    pub height: u32,
    /// Whether to show the grid.
    #[props(default = true)]
    pub show_grid: bool,
    /// Whether to show input/output level visualization.
    #[props(default = true)]
    pub show_levels: bool,
    /// Whether interaction is enabled.
    #[props(default = true)]
    pub interactive: bool,
}

#[component]
pub fn SaturatorGraph(props: SaturatorGraphProps) -> Element {
    let params = props.params.read();
    let min_db = -48.0;
    let max_db = 6.0;

    let layout = GraphLayout::new(props.width, props.height);

    // Generate transfer curve path
    let num_points = 200;
    let mut curve_path = String::new();

    for i in 0..=num_points {
        let t = i as f64 / num_points as f64;
        let input_db = min_db + t * (max_db - min_db);
        let output_db = saturate(input_db as f32, params.drive, params.mode) as f64;
        let output_clamped = output_db.clamp(min_db, max_db);

        let x = layout.db_to_x(input_db, min_db);
        let y = layout.db_to_y(output_clamped, min_db);

        if i == 0 {
            curve_path.push_str(&format!("M {x:.1} {y:.1}"));
        } else {
            curve_path.push_str(&format!(" L {x:.1} {y:.1}"));
        }
    }

    // Unity gain line (y = x)
    let unity_start_x = layout.db_to_x(min_db, min_db);
    let unity_start_y = layout.db_to_y(min_db, min_db);
    let unity_end_x = layout.db_to_x(max_db, min_db);
    let unity_end_y = layout.db_to_y(max_db, min_db);

    // Grid lines (dB markers)
    let db_markers: Vec<f64> = vec![-48.0, -36.0, -24.0, -12.0, 0.0];

    // Input/output level positions
    let input_level = props
        .metering
        .input_level
        .clamp(min_db as f32, max_db as f32);
    let output_level = saturate(input_level, params.drive, params.mode);
    let input_x = layout.db_to_x(input_level as f64, min_db);
    let input_y = layout.db_to_y(input_level as f64, min_db);
    let output_y = layout.db_to_y(output_level as f64, min_db);

    // Theme colors (orange/amber)
    let curve_color = "#f59e0b";
    let unity_color = "rgba(255, 255, 255, 0.2)";
    let grid_color = "rgba(255, 255, 255, 0.08)";
    let level_color = "#fb923c";
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
                for db in db_markers.iter() {
                    // Vertical grid line
                    line {
                        x1: "{layout.db_to_x(*db, min_db)}",
                        y1: "{layout.padding}",
                        x2: "{layout.db_to_x(*db, min_db)}",
                        y2: "{layout.padding + layout.graph_size}",
                        stroke: "{grid_color}",
                        stroke_width: "1",
                    }
                    // Horizontal grid line
                    line {
                        x1: "{layout.padding}",
                        y1: "{layout.db_to_y(*db, min_db)}",
                        x2: "{layout.padding + layout.graph_size}",
                        y2: "{layout.db_to_y(*db, min_db)}",
                        stroke: "{grid_color}",
                        stroke_width: "1",
                    }
                }
            }

            // Unity gain line
            line {
                x1: "{unity_start_x}",
                y1: "{unity_start_y}",
                x2: "{unity_end_x}",
                y2: "{unity_end_y}",
                stroke: "{unity_color}",
                stroke_width: "1.5",
                stroke_dasharray: "4,4",
            }

            // Transfer curve
            path {
                d: "{curve_path}",
                stroke: "{curve_color}",
                stroke_width: "2.5",
                stroke_linecap: "round",
                fill: "none",
            }

            // Input level indicator
            if props.show_levels && input_level > min_db as f32 {
                // Vertical line from input to output
                line {
                    x1: "{input_x}",
                    y1: "{input_y}",
                    x2: "{input_x}",
                    y2: "{output_y}",
                    stroke: "{level_color}",
                    stroke_width: "2",
                    opacity: "0.8",
                }
                // Input point
                circle {
                    cx: "{input_x}",
                    cy: "{input_y}",
                    r: "4",
                    fill: "{level_color}",
                }
                // Output point on curve
                circle {
                    cx: "{input_x}",
                    cy: "{output_y}",
                    r: "5",
                    fill: "{curve_color}",
                    stroke: "#fff",
                    stroke_width: "1.5",
                }
            }

            // Axis labels
            text {
                x: "{layout.width / 2.0}",
                y: "{layout.height - 8.0}",
                text_anchor: "middle",
                fill: "{text_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                "Input (dB)"
            }
            text {
                x: "12",
                y: "{layout.height / 2.0}",
                text_anchor: "middle",
                fill: "{text_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                transform: "rotate(-90, 12, {layout.height / 2.0})",
                "Output (dB)"
            }

            // Mode label
            text {
                x: "{layout.padding + layout.graph_size / 2.0}",
                y: "{layout.padding - 12.0}",
                text_anchor: "middle",
                fill: "{curve_color}",
                font_size: "12",
                font_family: "system-ui, -apple-system, sans-serif",
                font_weight: "600",
                "{params.mode.as_str()}"
            }

            // dB markers
            for db in db_markers.iter() {
                // X-axis marker
                text {
                    x: "{layout.db_to_x(*db, min_db)}",
                    y: "{layout.padding + layout.graph_size + 14.0}",
                    text_anchor: "middle",
                    fill: "{text_color}",
                    font_size: "9",
                    font_family: "system-ui, -apple-system, sans-serif",
                    "{*db as i32}"
                }
                // Y-axis marker
                text {
                    x: "{layout.padding - 6.0}",
                    y: "{layout.db_to_y(*db, min_db) + 3.0}",
                    text_anchor: "end",
                    fill: "{text_color}",
                    font_size: "9",
                    font_family: "system-ui, -apple-system, sans-serif",
                    "{*db as i32}"
                }
            }
        }
    }
}

// =============================================================================
// SaturatorWidget - Full widget with controls
// =============================================================================

use crate::theming::{ThemeContext, ThemeProvider};
use crate::widgets::knob::Knob;

#[derive(Props, Clone, PartialEq)]
pub struct SaturatorWidgetProps {
    /// Signal for saturator parameters (allows two-way binding).
    pub params: Signal<SaturatorParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: SaturatorMetering,
    /// Size of the graph in pixels.
    #[props(default = 200)]
    pub graph_size: u32,
    /// Whether to show the grid.
    #[props(default = true)]
    pub show_grid: bool,
    /// Whether to show input/output level visualization.
    #[props(default = true)]
    pub show_levels: bool,
    /// Whether to show the knob controls panel.
    #[props(default = true)]
    pub show_controls: bool,
    /// Whether interaction is enabled.
    #[props(default = true)]
    pub interactive: bool,
}

#[component]
pub fn SaturatorWidget(props: SaturatorWidgetProps) -> Element {
    let mut params = props.params;

    // Create local signals bound to params
    let mut drive_sig = use_signal(|| params.read().drive);
    let mut output_sig = use_signal(|| params.read().output);
    let mut mix_sig = use_signal(|| params.read().mix);
    let mut tone_sig = use_signal(|| params.read().tone);
    let mut dynamics_sig = use_signal(|| params.read().dynamics);

    // Sync from params to signals when params change externally
    let params_clone = params.read().clone();
    use_effect(move || {
        drive_sig.set(params_clone.drive);
        output_sig.set(params_clone.output);
        mix_sig.set(params_clone.mix);
        tone_sig.set(params_clone.tone);
        dynamics_sig.set(params_clone.dynamics);
    });

    // Value formatters
    let format_db = |v: f32| format!("{v:.1}dB");
    let format_percent = |v: f32| format!("{v:.0}%");
    let format_tone = |v: f32| {
        if v > 0.0 {
            format!("+{v:.0}")
        } else if v < 0.0 {
            format!("{v:.0}")
        } else {
            "0".to_string()
        }
    };

    // Knob sizes
    let large_knob = 56_u32;
    let tiny_knob = 32_u32;

    // Build current params for the graph
    let mut current_params_sig = use_signal(|| SaturatorParams {
        drive: drive_sig(),
        output: output_sig(),
        mix: mix_sig(),
        tone: tone_sig(),
        dynamics: dynamics_sig(),
        ..params.read().clone()
    });

    // Update signal when params change
    use_effect(move || {
        current_params_sig.set(SaturatorParams {
            drive: drive_sig(),
            output: output_sig(),
            mix: mix_sig(),
            tone: tone_sig(),
            dynamics: dynamics_sig(),
            ..params.read().clone()
        });
    });

    rsx! {
        ThemeProvider { theme: ThemeContext::new(),
            div {
                class: "saturator-widget flex gap-3 items-start",
                style: "background: linear-gradient(180deg, #111111 0%, #1a1a1a 100%); border-radius: 8px; padding: 12px;",

                // Left: Drive + Output knobs
                if props.show_controls {
                    div {
                        class: "flex flex-col items-center gap-2",
                        style: "min-width: 70px;",

                        // Large drive knob
                        Knob {
                            value: drive_sig,
                            min: 0.0,
                            max: 48.0,
                            size: large_knob,
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                drive_sig.set(v);
                                params.write().drive = v;
                            },
                        }
                        div {
                            class: "flex flex-col items-center gap-0",
                            span { class: "text-[10px] text-gray-500 leading-none", "DRIVE" }
                            span { class: "text-[11px] text-gray-300 leading-tight font-mono", "{format_db(drive_sig())}" }
                        }

                        // Output knob
                        Knob {
                            value: output_sig,
                            min: -24.0,
                            max: 6.0,
                            size: large_knob,
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                output_sig.set(v);
                                params.write().output = v;
                            },
                        }
                        div {
                            class: "flex flex-col items-center gap-0",
                            span { class: "text-[10px] text-gray-500 leading-none", "OUTPUT" }
                            span { class: "text-[11px] text-gray-300 leading-tight font-mono", "{format_db(output_sig())}" }
                        }
                    }
                }

                // Center: Graph
                SaturatorGraph {
                    params: current_params_sig,
                    metering: props.metering.clone(),
                    width: props.graph_size,
                    height: props.graph_size,
                    show_grid: props.show_grid,
                    show_levels: props.show_levels,
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

                        // Tone row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: tone_sig,
                                min: -100.0,
                                max: 100.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    tone_sig.set(v);
                                    params.write().tone = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "TONE" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_tone(tone_sig())}" }
                            }
                        }

                        // Dynamics row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: dynamics_sig,
                                min: 0.0,
                                max: 100.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    dynamics_sig.set(v);
                                    params.write().dynamics = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "DYN" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_percent(dynamics_sig())}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
