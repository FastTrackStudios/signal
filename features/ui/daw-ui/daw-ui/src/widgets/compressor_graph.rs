//! Compressor graph widget.
//!
//! A Dioxus component that renders a compressor transfer curve with interactive controls:
//! - Transfer curve visualization (input dB vs output dB)
//! - Draggable threshold point
//! - Draggable ratio control
//! - Adjustable knee width
//! - Real-time gain reduction meter
//! - Input/output level visualization
//!
//! Inspired by ZLCompressor and Pro-C style interfaces.

use crate::prelude::*;

/// Compressor operating mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CompressorMode {
    /// Standard downward compression.
    #[default]
    Compress,
    /// Upward expansion (gate-like).
    Expand,
    /// Upward compression (bring up quiet signals).
    Inflate,
    /// Waveshaping/saturation mode.
    Shape,
}

impl CompressorMode {
    /// Get display label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Compress => "Compress",
            Self::Expand => "Expand",
            Self::Inflate => "Inflate",
            Self::Shape => "Shape",
        }
    }

    /// All available modes.
    pub fn all() -> &'static [CompressorMode] {
        &[Self::Compress, Self::Expand, Self::Inflate, Self::Shape]
    }
}

/// Detection mode for level sensing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DetectionMode {
    /// Peak level detection.
    Peak,
    /// RMS level detection.
    #[default]
    Rms,
    /// True peak detection (oversampled).
    TruePeak,
}

impl DetectionMode {
    /// Get display label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Peak => "Peak",
            Self::Rms => "RMS",
            Self::TruePeak => "True Peak",
        }
    }
}

/// Stereo link mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StereoLink {
    /// Fully linked (same GR on both channels).
    #[default]
    Linked,
    /// Independent processing per channel.
    Unlinked,
    /// Mid-side processing.
    MidSide,
}

impl StereoLink {
    /// Get display label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Linked => "Linked",
            Self::Unlinked => "Unlinked",
            Self::MidSide => "M/S",
        }
    }
}

/// Compressor parameters for the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct CompressorParams {
    /// Operating mode.
    pub mode: CompressorMode,
    /// Threshold in dB (-60 to 0).
    pub threshold: f32,
    /// Compression ratio (1:1 to infinity:1, stored as ratio value, e.g. 4.0 for 4:1).
    pub ratio: f32,
    /// Knee width in dB (0 = hard knee, up to ~24 dB soft knee).
    pub knee: f32,
    /// Attack time in milliseconds.
    pub attack: f32,
    /// Release time in milliseconds.
    pub release: f32,
    /// Makeup gain in dB.
    pub makeup: f32,
    /// Mix/dry-wet (0.0 to 1.0).
    pub mix: f32,
    /// Detection mode.
    pub detection: DetectionMode,
    /// Stereo link mode.
    pub stereo_link: StereoLink,
    /// Range/floor in dB (for expanders, or max GR limit).
    pub range: f32,
    /// Hold time in milliseconds.
    pub hold: f32,
    /// Lookahead in milliseconds.
    pub lookahead: f32,
    /// Curve shape (-1.0 to 1.0, 0 = linear, positive = aggressive, negative = soft).
    pub curve: f32,
    /// Bypass state.
    pub bypass: bool,
}

impl Default for CompressorParams {
    fn default() -> Self {
        Self {
            mode: CompressorMode::Compress,
            threshold: -18.0,
            ratio: 4.0,
            knee: 6.0,
            attack: 10.0,
            release: 100.0,
            makeup: 0.0,
            mix: 1.0,
            detection: DetectionMode::Rms,
            stereo_link: StereoLink::Linked,
            range: -60.0,
            hold: 0.0,
            lookahead: 0.0,
            curve: 0.0,
            bypass: false,
        }
    }
}

/// Real-time metering data for the compressor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompressorMetering {
    /// Current input level in dB.
    pub input_level: f32,
    /// Current output level in dB.
    pub output_level: f32,
    /// Current gain reduction in dB (negative value).
    pub gain_reduction: f32,
    /// Peak input level (with hold).
    pub input_peak: f32,
    /// Peak output level (with hold).
    pub output_peak: f32,
    /// Peak gain reduction.
    pub gr_peak: f32,
    /// History of gain reduction values for waveform display.
    /// Each value is GR in dB (negative). Newest values at the end.
    /// The graph will display the most recent `gr_history_size` values.
    pub gr_history: Vec<f32>,
    /// History of input level values for waveform display.
    /// Each value is input level in dB. Newest values at the end.
    pub input_history: Vec<f32>,
}

/// Default number of history samples to display.
pub const DEFAULT_HISTORY_SIZE: usize = 128;

/// dB range options for the graph.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DbRange {
    /// -30 to 0 dB.
    Range30,
    /// -48 to 0 dB.
    #[default]
    Range48,
    /// -60 to 0 dB.
    Range60,
    /// -72 to 0 dB.
    Range72,
}

impl DbRange {
    /// Get the minimum dB value.
    pub fn min_db(&self) -> f32 {
        match self {
            Self::Range30 => -30.0,
            Self::Range48 => -48.0,
            Self::Range60 => -60.0,
            Self::Range72 => -72.0,
        }
    }

    /// Get display label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Range30 => "30 dB",
            Self::Range48 => "48 dB",
            Self::Range60 => "60 dB",
            Self::Range72 => "72 dB",
        }
    }
}

/// What the user is currently dragging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragTarget {
    /// Dragging the threshold point.
    Threshold,
    /// Dragging the ratio (adjusting slope above threshold).
    Ratio,
    /// Dragging the knee width.
    Knee,
}

/// Graph layout constants
struct GraphLayout {
    width: f64,
    height: f64,
    padding: f64,
    graph_size: f64,
    min_db: f64,
}

impl GraphLayout {
    fn new(min_db: f32) -> Self {
        let width = 400.0;
        let height = 400.0;
        let padding = 40.0;
        let graph_size = width - 2.0 * padding;
        Self {
            width,
            height,
            padding,
            graph_size,
            min_db: min_db as f64,
        }
    }

    fn db_to_x(&self, db: f64) -> f64 {
        self.padding + ((db - self.min_db) / -self.min_db) * self.graph_size
    }

    fn db_to_y(&self, db: f64) -> f64 {
        self.padding + (1.0 - (db - self.min_db) / -self.min_db) * self.graph_size
    }

    fn x_to_db(&self, x: f64) -> f64 {
        self.min_db + ((x - self.padding) / self.graph_size) * -self.min_db
    }
}

/// Compute compressor transfer function output for a given input.
fn compute_transfer(
    input_db: f64,
    mode: CompressorMode,
    threshold: f64,
    ratio: f64,
    knee: f64,
    curve: f64,
    range: f64,
) -> f64 {
    let knee_w = knee / 2.0; // Half-width for symmetry
    let low_th = threshold - knee_w;
    let high_th = threshold + knee_w;

    match mode {
        CompressorMode::Compress | CompressorMode::Shape => {
            if input_db <= low_th {
                // Below knee: unity gain
                input_db
            } else if input_db < high_th {
                // In knee region: quadratic interpolation
                let a0 = (1.0 / ratio - 1.0) / (4.0 * knee_w);
                let x_offset = input_db - low_th;
                input_db + a0 * x_offset * x_offset
            } else {
                // Above knee: compression with optional curve
                let linear_output = threshold + (input_db - threshold) / ratio;

                if curve.abs() < 0.001 {
                    linear_output
                } else if curve > 0.0 {
                    // Aggressive curve (more compression at high levels)
                    let alpha = 1.0 - curve;
                    let curved_output = {
                        let temp = 0.5 / ratio;
                        let a = temp / (threshold + knee_w).min(-0.0001);
                        let c = temp * (knee_w - threshold) + threshold;
                        a * input_db * input_db + c
                    };
                    alpha * linear_output + curve * curved_output
                } else {
                    // Soft curve (less compression at high levels)
                    let alpha = 1.0 + curve;
                    let beta = -curve;
                    let curved_output = {
                        let temp = 0.5 * (1.0 - ratio) / ratio;
                        let a = temp / (threshold + knee_w).min(-0.0001);
                        let c = temp * (knee_w - threshold);
                        a * input_db * input_db + input_db + c
                    };
                    alpha * linear_output + beta * curved_output
                }
            }
        }
        CompressorMode::Expand => {
            // Downward expansion (gate-like)
            if input_db >= high_th {
                input_db
            } else if input_db > low_th {
                // Knee region
                let a0 = (ratio - 1.0) / (4.0 * knee_w);
                let x_offset = high_th - input_db;
                input_db - a0 * x_offset * x_offset
            } else {
                // Below threshold: expansion
                let expanded = threshold + (input_db - threshold) * ratio;
                expanded.max(range)
            }
        }
        CompressorMode::Inflate => {
            // Upward compression (bring up quiet signals)
            if input_db >= threshold {
                input_db
            } else if input_db > low_th {
                // Knee region
                let a0 = (1.0 - 1.0 / ratio) / (4.0 * knee_w);
                let x_offset = threshold - input_db;
                input_db + a0 * x_offset * x_offset
            } else {
                // Below threshold: upward compression
                let output = threshold - (threshold - input_db) / ratio;
                output.max(range)
            }
        }
    }
}

/// Props for the CompressorGraph component.
#[derive(Props, Clone, PartialEq)]
pub struct CompressorGraphProps {
    /// Current compressor parameters.
    pub params: CompressorParams,
    /// Real-time metering data.
    #[props(default)]
    pub metering: CompressorMetering,
    /// dB range for the graph.
    #[props(default)]
    pub db_range: DbRange,
    /// Whether to show the grid.
    #[props(default = true)]
    pub show_grid: bool,
    /// Whether to show gain reduction meter.
    #[props(default = true)]
    pub show_gr_meter: bool,
    /// Whether to show input/output level visualization on curve.
    #[props(default = true)]
    pub show_levels: bool,
    /// Whether to show the GR history trace (scrolling waveform).
    #[props(default = true)]
    pub show_gr_trace: bool,
    /// Whether to show the input level history trace.
    #[props(default = true)]
    pub show_input_trace: bool,
    /// Whether interaction is enabled.
    #[props(default = true)]
    pub interactive: bool,
    /// Callback when threshold changes.
    #[props(default)]
    pub on_threshold_change: Option<EventHandler<f32>>,
    /// Callback when ratio changes.
    #[props(default)]
    pub on_ratio_change: Option<EventHandler<f32>>,
    /// Callback when knee changes.
    #[props(default)]
    pub on_knee_change: Option<EventHandler<f32>>,
    /// Callback when any parameter changes.
    #[props(default)]
    pub on_params_change: Option<EventHandler<CompressorParams>>,
}

/// Compressor graph component.
///
/// Renders an interactive compressor transfer curve with real-time metering.
#[component]
pub fn CompressorGraph(props: CompressorGraphProps) -> Element {
    let layout = GraphLayout::new(props.db_range.min_db());

    // Drag state
    let mut drag_target = use_signal(|| None::<DragTarget>);
    let mut drag_start_y = use_signal(|| 0.0_f64);
    let mut drag_start_value = use_signal(|| 0.0_f32);
    let mut hovered = use_signal(|| false);

    // Pre-compute values we need
    let threshold = props.params.threshold as f64;
    let ratio = props.params.ratio as f64;
    let knee = props.params.knee as f64;
    let curve = props.params.curve as f64;
    let range = props.params.range as f64;
    let mode = props.params.mode;
    let min_db = layout.min_db;

    // Generate transfer curve path
    let curve_path = {
        let mut path = String::new();
        let num_points = 200;
        for i in 0..=num_points {
            let input_db = min_db + (i as f64 / num_points as f64) * -min_db;
            let output_db = compute_transfer(input_db, mode, threshold, ratio, knee, curve, range);
            let x = layout.db_to_x(input_db);
            let y = layout.db_to_y(output_db.clamp(min_db, 0.0));
            if i == 0 {
                path.push_str(&format!("M {x:.1} {y:.1}"));
            } else {
                path.push_str(&format!(" L {x:.1} {y:.1}"));
            }
        }
        path
    };

    // Unity gain reference line (diagonal)
    let unity_path = format!(
        "M {:.1} {:.1} L {:.1} {:.1}",
        layout.padding,
        layout.padding + layout.graph_size,
        layout.padding + layout.graph_size,
        layout.padding
    );

    // Threshold marker position
    let threshold_x = layout.db_to_x(threshold);
    let threshold_y = layout.db_to_y(threshold);

    // Knee region boundaries
    let knee_half = knee / 2.0;
    let knee_low_x = layout.db_to_x(threshold - knee_half);
    let knee_high_x = layout.db_to_x(threshold + knee_half);

    // Current input/output positions for level visualization
    let input_level = props.metering.input_level.clamp(min_db as f32, 0.0) as f64;
    let input_x = layout.db_to_x(input_level);
    let input_y = layout.db_to_y(input_level);
    let output_db = compute_transfer(input_level, mode, threshold, ratio, knee, curve, range);
    let output_y = layout.db_to_y(output_db.clamp(min_db, 0.0));

    // GR meter dimensions
    let gr_meter_width = 12.0;
    let gr_meter_x = layout.width - layout.padding + 8.0;

    // Grid lines
    let grid_lines = if props.show_grid {
        let step = if min_db <= -60.0 { 12.0 } else { 6.0 };
        let mut lines = Vec::new();
        let mut db = 0.0;
        while db > min_db {
            db -= step;
            if db > min_db {
                let pos = layout.db_to_x(db);
                // Vertical line
                lines.push(format!(
                    "M {:.1} {:.1} L {:.1} {:.1}",
                    pos,
                    layout.padding,
                    pos,
                    layout.padding + layout.graph_size
                ));
                // Horizontal line
                let ypos = layout.db_to_y(db);
                lines.push(format!(
                    "M {:.1} {:.1} L {:.1} {:.1}",
                    layout.padding,
                    ypos,
                    layout.padding + layout.graph_size,
                    ypos
                ));
            }
        }
        lines.join(" ")
    } else {
        String::new()
    };

    // dB scale markers
    let db_markers: Vec<f64> = {
        let step = if min_db <= -60.0 { 12.0 } else { 6.0 };
        let mut m = vec![0.0];
        let mut db = 0.0;
        while db > min_db {
            db -= step;
            if db >= min_db {
                m.push(db);
            }
        }
        m
    };

    // Pre-compute marker positions
    let marker_positions: Vec<(f64, f64, f64, i32)> = db_markers
        .iter()
        .map(|&db| (layout.db_to_x(db), layout.db_to_y(db), db, db as i32))
        .collect();

    // Generate GR history trace path (scrolling waveform from right to left)
    // The trace is drawn as a filled area from the unity line down to the GR level
    let gr_trace_path = if props.show_gr_trace && !props.metering.gr_history.is_empty() {
        let history = &props.metering.gr_history;
        let num_samples = history.len();
        let samples_to_show = num_samples.min(DEFAULT_HISTORY_SIZE);
        let start_idx = num_samples.saturating_sub(samples_to_show);

        let mut path = String::new();
        let x_step = layout.graph_size / (samples_to_show.max(1) as f64 - 1.0).max(1.0);

        // Start at the right edge (newest sample), go left (older samples)
        // Draw the top line (unity/0dB line)
        let right_x = layout.padding + layout.graph_size;
        let unity_y = layout.db_to_y(0.0);

        path.push_str(&format!("M {:.1} {:.1}", right_x, unity_y));

        // Draw line along unity from right to left
        for i in 0..samples_to_show {
            let x = right_x - (i as f64) * x_step;
            path.push_str(&format!(" L {:.1} {:.1}", x, unity_y));
        }

        // Now draw down and back along the GR curve (from left/oldest to right/newest)
        for i in (0..samples_to_show).rev() {
            let sample_idx = start_idx + (samples_to_show - 1 - i);
            let gr = history.get(sample_idx).copied().unwrap_or(0.0);
            // GR is negative, so output = input + gr, at unity input (0dB), output = gr
            let output_db = gr.clamp(min_db as f32, 0.0) as f64;
            let x = right_x - (i as f64) * x_step;
            let y = layout.db_to_y(output_db);
            path.push_str(&format!(" L {:.1} {:.1}", x, y));
        }

        path.push_str(" Z"); // Close the path
        path
    } else {
        String::new()
    };

    // Generate input level history trace path
    let input_trace_path = if props.show_input_trace && !props.metering.input_history.is_empty() {
        let history = &props.metering.input_history;
        let num_samples = history.len();
        let samples_to_show = num_samples.min(DEFAULT_HISTORY_SIZE);
        let start_idx = num_samples.saturating_sub(samples_to_show);

        let mut path = String::new();
        let x_step = layout.graph_size / (samples_to_show.max(1) as f64 - 1.0).max(1.0);
        let right_x = layout.padding + layout.graph_size;

        // Draw line from oldest (left) to newest (right)
        for i in 0..samples_to_show {
            let sample_idx = start_idx + i;
            let level = history.get(sample_idx).copied().unwrap_or(min_db as f32);
            let level_clamped = level.clamp(min_db as f32, 0.0) as f64;
            let x = right_x - ((samples_to_show - 1 - i) as f64) * x_step;
            let y = layout.db_to_y(level_clamped);

            if i == 0 {
                path.push_str(&format!("M {:.1} {:.1}", x, y));
            } else {
                path.push_str(&format!(" L {:.1} {:.1}", x, y));
            }
        }
        path
    } else {
        String::new()
    };

    // Capture layout values for closures
    let padding = layout.padding;
    let graph_size = layout.graph_size;

    // Mouse event handlers
    let onmousedown = {
        let threshold_f32 = props.params.threshold;
        let ratio_f32 = props.params.ratio;
        let interactive = props.interactive;
        move |evt: MouseEvent| {
            if !interactive {
                return;
            }
            let coords = evt.element_coordinates();
            let mx = coords.x;
            let my = coords.y;

            // Check if clicking on threshold point
            let th_x = padding + ((threshold_f32 as f64 - min_db) / -min_db) * graph_size;
            let th_y = padding + (1.0 - (threshold_f32 as f64 - min_db) / -min_db) * graph_size;
            let dist_to_threshold = ((mx - th_x).powi(2) + (my - th_y).powi(2)).sqrt();

            if dist_to_threshold < 15.0 {
                drag_target.set(Some(DragTarget::Threshold));
                drag_start_y.set(my);
                drag_start_value.set(threshold_f32);
            } else {
                // Check if in ratio adjustment zone (above threshold on curve)
                let x_db = min_db + ((mx - padding) / graph_size) * -min_db;
                if x_db > threshold_f32 as f64 && my < th_y {
                    drag_target.set(Some(DragTarget::Ratio));
                    drag_start_y.set(my);
                    drag_start_value.set(ratio_f32);
                }
            }
        }
    };

    let onmousemove = {
        let threshold_f32 = props.params.threshold;
        let on_threshold_change = props.on_threshold_change;
        let on_ratio_change = props.on_ratio_change;
        move |evt: MouseEvent| {
            let target = { *drag_target.read() };
            if target.is_none() {
                return;
            }

            let coords = evt.element_coordinates();
            let mx = coords.x;
            let my = coords.y;

            let shift = evt.modifiers().shift();
            let sensitivity = if shift { 0.1 } else { 1.0 };

            match target {
                Some(DragTarget::Threshold) => {
                    // Drag threshold: horizontal = threshold dB
                    let x_db = min_db + ((mx - padding) / graph_size) * -min_db;
                    let new_threshold = (x_db as f32 * sensitivity as f32
                        + threshold_f32 * (1.0 - sensitivity as f32))
                        .clamp(min_db as f32, 0.0);
                    if let Some(cb) = &on_threshold_change {
                        cb.call(new_threshold);
                    }
                }
                Some(DragTarget::Ratio) => {
                    // Drag ratio: vertical movement adjusts ratio
                    let start_y = { *drag_start_y.read() };
                    let start_val = { *drag_start_value.read() };
                    let delta_y = start_y - my; // Positive = dragging up = more compression
                    let delta_ratio = (delta_y as f32 / 50.0) * sensitivity as f32;
                    let new_ratio = (start_val + delta_ratio * start_val).clamp(1.0, 100.0);
                    if let Some(cb) = &on_ratio_change {
                        cb.call(new_ratio);
                    }
                }
                Some(DragTarget::Knee) | None => {}
            }
        }
    };

    let onmouseup = move |_evt: MouseEvent| {
        drag_target.set(None);
    };

    let onmouseleave = move |_evt: MouseEvent| {
        drag_target.set(None);
        hovered.set(false);
    };

    let onmouseenter = move |_evt: MouseEvent| {
        hovered.set(true);
    };

    let onwheel = {
        let knee_f32 = props.params.knee;
        let on_knee_change = props.on_knee_change;
        let interactive = props.interactive;
        move |evt: WheelEvent| {
            if !interactive {
                return;
            }
            evt.prevent_default();

            // Get the Y delta from wheel event (strip units to get raw value)
            let delta_vec = evt.delta().strip_units();
            let delta_y = delta_vec.y;

            let shift = evt.modifiers().shift();
            let sensitivity = if shift { 0.1 } else { 1.0 };

            // Wheel adjusts knee width (wheel up = increase, wheel down = decrease)
            let delta_knee = (-delta_y as f32 / 50.0) * sensitivity as f32;
            let new_knee = (knee_f32 + delta_knee * 6.0).clamp(0.0, 24.0);
            if let Some(cb) = &on_knee_change {
                cb.call(new_knee);
            }
        }
    };

    // Colors
    let curve_color = "#00d4ff";
    let unity_color = "rgba(255, 255, 255, 0.2)";
    let grid_color = "rgba(255, 255, 255, 0.08)";
    let threshold_color = "#ff9500";
    let knee_color = "rgba(255, 149, 0, 0.15)";
    let gr_color = "#ff3b30";
    let level_color = "#30d158";
    let text_color = "rgba(255, 255, 255, 0.6)";

    // GR meter fill height
    let gr_db = props.metering.gain_reduction.abs().min(-min_db as f32);
    let gr_height = (gr_db as f64 / -min_db) * layout.graph_size;

    rsx! {
        svg {
            width: "100%",
            height: "100%",
            view_box: "0 0 {layout.width} {layout.height}",
            preserve_aspect_ratio: "xMidYMid meet",
            style: "background: #1a1a1a; border-radius: 8px; user-select: none;",
            onmousedown: onmousedown,
            onmousemove: onmousemove,
            onmouseup: onmouseup,
            onmouseleave: onmouseleave,
            onmouseenter: onmouseenter,
            onwheel: onwheel,

            // Background
            rect {
                x: "{layout.padding}",
                y: "{layout.padding}",
                width: "{layout.graph_size}",
                height: "{layout.graph_size}",
                fill: "#0d0d0d",
                rx: "4",
            }

            // Grid lines
            if props.show_grid && !grid_lines.is_empty() {
                path {
                    d: "{grid_lines}",
                    stroke: "{grid_color}",
                    stroke_width: "1",
                    fill: "none",
                }
            }

            // Knee region highlight
            if props.params.knee > 0.1 {
                rect {
                    x: "{knee_low_x}",
                    y: "{layout.padding}",
                    width: "{knee_high_x - knee_low_x}",
                    height: "{layout.graph_size}",
                    fill: "{knee_color}",
                }
            }

            // GR history trace (filled area showing gain reduction over time)
            if props.show_gr_trace && !gr_trace_path.is_empty() {
                path {
                    d: "{gr_trace_path}",
                    fill: "rgba(255, 59, 48, 0.3)",
                    stroke: "none",
                }
            }

            // Input level history trace (line showing input level over time)
            if props.show_input_trace && !input_trace_path.is_empty() {
                path {
                    d: "{input_trace_path}",
                    stroke: "rgba(48, 209, 88, 0.6)",
                    stroke_width: "1.5",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    fill: "none",
                }
            }

            // Unity gain line (diagonal)
            path {
                d: "{unity_path}",
                stroke: "{unity_color}",
                stroke_width: "1",
                stroke_dasharray: "4,4",
                fill: "none",
            }

            // Transfer curve
            path {
                d: "{curve_path}",
                stroke: "{curve_color}",
                stroke_width: "2.5",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                fill: "none",
            }

            // Threshold vertical line
            line {
                x1: "{threshold_x}",
                y1: "{layout.padding}",
                x2: "{threshold_x}",
                y2: "{layout.padding + layout.graph_size}",
                stroke: "{threshold_color}",
                stroke_width: "1",
                stroke_dasharray: "4,2",
            }

            // Threshold point (draggable)
            if props.interactive {
                circle {
                    cx: "{threshold_x}",
                    cy: "{threshold_y}",
                    r: "8",
                    fill: "{threshold_color}",
                    stroke: "#fff",
                    stroke_width: "2",
                    style: "cursor: pointer;",
                }
            }

            // Input level indicator on curve
            if props.show_levels && props.metering.input_level > min_db as f32 {
                // Vertical line from input to curve
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

            // GR meter
            if props.show_gr_meter {
                // GR meter background
                rect {
                    x: "{gr_meter_x}",
                    y: "{layout.padding}",
                    width: "{gr_meter_width}",
                    height: "{layout.graph_size}",
                    fill: "#0d0d0d",
                    rx: "2",
                }
                // GR meter fill (from top, grows downward)
                rect {
                    x: "{gr_meter_x}",
                    y: "{layout.padding}",
                    width: "{gr_meter_width}",
                    height: "{gr_height}",
                    fill: "{gr_color}",
                    rx: "2",
                }
            }

            // Axis labels
            // X-axis label (Input)
            text {
                x: "{layout.width / 2.0}",
                y: "{layout.height - 8.0}",
                text_anchor: "middle",
                fill: "{text_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                "Input (dB)"
            }
            // Y-axis label (Output)
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

            // Threshold label
            text {
                x: "{threshold_x}",
                y: "{layout.padding - 8.0}",
                text_anchor: "middle",
                fill: "{threshold_color}",
                font_size: "10",
                font_family: "system-ui, -apple-system, sans-serif",
                "{props.params.threshold:.1} dB"
            }

            // Ratio label (above threshold on curve)
            text {
                x: "{threshold_x + 40.0}",
                y: "{threshold_y - 20.0}",
                text_anchor: "start",
                fill: "{curve_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                "{props.params.ratio:.1}:1"
            }

            // GR value label
            if props.show_gr_meter {
                text {
                    x: "{gr_meter_x + gr_meter_width / 2.0}",
                    y: "{layout.padding - 8.0}",
                    text_anchor: "middle",
                    fill: "{gr_color}",
                    font_size: "10",
                    font_family: "system-ui, -apple-system, sans-serif",
                    "GR"
                }
                text {
                    x: "{gr_meter_x + gr_meter_width / 2.0}",
                    y: "{layout.padding + layout.graph_size + 14.0}",
                    text_anchor: "middle",
                    fill: "{text_color}",
                    font_size: "9",
                    font_family: "system-ui, -apple-system, sans-serif",
                    "{props.metering.gain_reduction:.1}"
                }
            }

            // dB scale markers
            for (x_pos, y_pos, _db, db_int) in marker_positions.iter().cloned() {
                // X-axis markers
                text {
                    x: "{x_pos}",
                    y: "{layout.padding + layout.graph_size + 14.0}",
                    text_anchor: "middle",
                    fill: "{text_color}",
                    font_size: "9",
                    font_family: "system-ui, -apple-system, sans-serif",
                    "{db_int}"
                }
                // Y-axis markers
                text {
                    x: "{layout.padding - 6.0}",
                    y: "{y_pos + 3.0}",
                    text_anchor: "end",
                    fill: "{text_color}",
                    font_size: "9",
                    font_family: "system-ui, -apple-system, sans-serif",
                    "{db_int}"
                }
            }

            // Knee width indicator (when hovered or dragging)
            if *hovered.read() || drag_target.read().is_some() {
                text {
                    x: "{(knee_low_x + knee_high_x) / 2.0}",
                    y: "{layout.padding + layout.graph_size - 8.0}",
                    text_anchor: "middle",
                    fill: "{threshold_color}",
                    font_size: "9",
                    font_family: "system-ui, -apple-system, sans-serif",
                    opacity: "0.8",
                    "Knee: {props.params.knee:.1} dB"
                }
            }
        }
    }
}

// =============================================================================
// CompressorWidget - Full compressor UI with knobs (Pro-C style)
// =============================================================================

use crate::theming::{ThemeContext, ThemeProvider};
use crate::widgets::knob::Knob;

/// Props for the CompressorWidget component.
#[derive(Props, Clone, PartialEq)]
pub struct CompressorWidgetProps {
    /// Signal for compressor parameters (allows two-way binding).
    pub params: Signal<CompressorParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: CompressorMetering,
    /// dB range for the graph.
    #[props(default)]
    pub db_range: DbRange,
    /// Size of the graph in pixels.
    #[props(default = 200)]
    pub graph_size: u32,
    /// Whether to show the grid.
    #[props(default = true)]
    pub show_grid: bool,
    /// Whether to show gain reduction meter.
    #[props(default = true)]
    pub show_gr_meter: bool,
    /// Whether to show input/output level visualization.
    #[props(default = true)]
    pub show_levels: bool,
    /// Whether to show the GR history trace.
    #[props(default = true)]
    pub show_gr_trace: bool,
    /// Whether to show the knob controls panel.
    #[props(default = true)]
    pub show_controls: bool,
    /// Whether interaction is enabled.
    #[props(default = true)]
    pub interactive: bool,
}

/// Compressor widget with integrated knob controls.
///
/// A complete compressor UI inspired by Pro-C with:
/// - Transfer curve graph
/// - Large threshold knob with smaller ratio/knee knobs
/// - Envelope controls (attack, release, makeup)
/// - Cyan/teal color theme
#[component]
pub fn CompressorWidget(props: CompressorWidgetProps) -> Element {
    let mut params = props.params;

    // Create local signals bound to params
    let mut threshold_sig = use_signal(|| params.read().threshold);
    let mut ratio_sig = use_signal(|| params.read().ratio);
    let mut knee_sig = use_signal(|| params.read().knee);
    let mut attack_sig = use_signal(|| params.read().attack);
    let mut release_sig = use_signal(|| params.read().release);
    let mut makeup_sig = use_signal(|| params.read().makeup);

    // Sync from params to signals when params change externally
    let params_clone = params.read().clone();
    use_effect(move || {
        threshold_sig.set(params_clone.threshold);
        ratio_sig.set(params_clone.ratio);
        knee_sig.set(params_clone.knee);
        attack_sig.set(params_clone.attack);
        release_sig.set(params_clone.release);
        makeup_sig.set(params_clone.makeup);
    });

    // Value formatters
    let format_db = |v: f32| format!("{v:.0}dB");
    let format_ratio = |v: f32| {
        if v >= 100.0 {
            "∞:1".to_string()
        } else {
            format!("{v:.1}:1")
        }
    };
    let format_ms = |v: f32| {
        if v >= 1000.0 {
            format!("{:.1}s", v / 1000.0)
        } else {
            format!("{v:.0}ms")
        }
    };

    // Knob sizes
    let large_knob = 56_u32;
    let small_knob = 36_u32;
    let tiny_knob = 32_u32;

    // Build current params for the graph
    let current_params = CompressorParams {
        threshold: threshold_sig(),
        ratio: ratio_sig(),
        knee: knee_sig(),
        attack: attack_sig(),
        release: release_sig(),
        makeup: makeup_sig(),
        ..params.read().clone()
    };

    rsx! {
        ThemeProvider { theme: ThemeContext::new(),
            div {
                class: "compressor-widget flex gap-3 items-start",
                style: "background: linear-gradient(180deg, #111111 0%, #1a1a1a 100%); border-radius: 8px; padding: 12px;",

                // Left: Threshold + Ratio/Knee knobs
                if props.show_controls {
                    div {
                        class: "flex flex-col items-center gap-2",
                        style: "min-width: 70px;",

                        // Large threshold knob
                        Knob {
                            value: threshold_sig,
                            min: -60.0,
                            max: 0.0,
                            size: large_knob,
                            label: Some("THRESH".to_string()),
                            value_display: Some(format_db(threshold_sig())),
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                threshold_sig.set(v);
                                params.write().threshold = v;
                            },
                        }

                        // Ratio and Knee row
                        div {
                            class: "flex gap-1",

                            Knob {
                                value: ratio_sig,
                                min: 1.0,
                                max: 20.0,
                                size: small_knob,
                                label: Some("RATIO".to_string()),
                                value_display: Some(format_ratio(ratio_sig())),
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    ratio_sig.set(v);
                                    params.write().ratio = v;
                                },
                            }

                            Knob {
                                value: knee_sig,
                                min: 0.0,
                                max: 24.0,
                                size: small_knob,
                                label: Some("KNEE".to_string()),
                                value_display: Some(format_db(knee_sig())),
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    knee_sig.set(v);
                                    params.write().knee = v;
                                },
                            }
                        }
                    }
                }

                // Center: Transfer curve graph
                div {
                    class: "compressor-graph-area",
                    style: "width: {props.graph_size}px; height: {props.graph_size}px;",

                    CompressorGraph {
                        params: current_params,
                        metering: props.metering.clone(),
                        db_range: props.db_range,
                        show_grid: props.show_grid,
                        show_gr_meter: props.show_gr_meter,
                        show_levels: props.show_levels,
                        show_gr_trace: props.show_gr_trace,
                        show_input_trace: false,
                        interactive: false, // Disable graph drag, use knobs instead
                    }
                }

                // Right: Envelope controls (compact horizontal rows)
                if props.show_controls {
                    div {
                        class: "flex flex-col gap-0",
                        style: "padding-left: 8px; border-left: 1px solid rgba(255,255,255,0.08);",

                        // Attack row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: attack_sig,
                                min: 0.1,
                                max: 250.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    attack_sig.set(v);
                                    params.write().attack = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "ATK" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_ms(attack_sig())}" }
                            }
                        }

                        // Release row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: release_sig,
                                min: 10.0,
                                max: 2000.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    release_sig.set(v);
                                    params.write().release = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "REL" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_ms(release_sig())}" }
                            }
                        }

                        // Makeup row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: makeup_sig,
                                min: 0.0,
                                max: 24.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    makeup_sig.set(v);
                                    params.write().makeup = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "GAIN" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_db(makeup_sig())}" }
                            }
                        }

                        // GR meter display
                        div {
                            class: "flex items-center gap-1 mt-1 pt-1",
                            style: "border-top: 1px solid rgba(255,255,255,0.05);",
                            div {
                                class: "w-8 h-8 rounded flex items-center justify-center",
                                style: "background: rgba(255, 59, 48, 0.15);",
                                span {
                                    class: "text-[10px] font-mono",
                                    style: "color: #ff3b30;",
                                    "{props.metering.gain_reduction:.1}"
                                }
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "GR" }
                                span { class: "text-[10px] text-gray-400 leading-tight", "dB" }
                            }
                        }
                    }
                }
            }
        }
    }
}
