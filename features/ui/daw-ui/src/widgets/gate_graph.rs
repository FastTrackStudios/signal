//! Gate graph widget.
//!
//! A Dioxus component that renders a noise gate transfer curve with interactive controls,
//! inspired by FabFilter Pro-G. Features:
//! - Transfer curve visualization (input dB vs output dB)
//! - Compact knob layout with threshold, ratio, range
//! - Envelope controls (attack, release, hold, knee, lookahead)
//! - Real-time gain reduction meter
//! - Blue-themed color scheme

use crate::prelude::*;

use crate::theming::{ThemeContext, ThemeProvider};
use crate::widgets::knob::Knob;

/// Gate operating mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GateMode {
    /// Standard downward gate (attenuate below threshold).
    #[default]
    Gate,
    /// Expander mode (more gradual attenuation).
    Expander,
    /// Ducker mode (attenuate above threshold, for ducking/sidechain).
    Ducker,
}

impl GateMode {
    /// Get display label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Gate => "Gate",
            Self::Expander => "Expander",
            Self::Ducker => "Ducker",
        }
    }

    /// All available modes.
    pub fn all() -> &'static [GateMode] {
        &[Self::Gate, Self::Expander, Self::Ducker]
    }
}

/// Gate parameters for the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct GateParams {
    /// Operating mode.
    pub mode: GateMode,
    /// Threshold in dB (-60 to 0).
    pub threshold: f32,
    /// Expansion ratio (1:1 to infinity:1).
    pub ratio: f32,
    /// Range/floor in dB (how much to attenuate when closed).
    pub range: f32,
    /// Knee width in dB (0 = hard knee).
    pub knee: f32,
    /// Attack time in milliseconds.
    pub attack: f32,
    /// Hold time in milliseconds.
    pub hold: f32,
    /// Release time in milliseconds.
    pub release: f32,
    /// Lookahead in milliseconds.
    pub lookahead: f32,
    /// Bypass state.
    pub bypass: bool,
}

impl Default for GateParams {
    fn default() -> Self {
        Self {
            mode: GateMode::Gate,
            threshold: -30.0,
            ratio: 10.0,
            range: -80.0,
            knee: 6.0,
            attack: 0.5,
            hold: 50.0,
            release: 100.0,
            lookahead: 0.0,
            bypass: false,
        }
    }
}

/// Real-time metering data for the gate.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GateMetering {
    /// Current input level in dB.
    pub input_level: f32,
    /// Current output level in dB.
    pub output_level: f32,
    /// Current gain reduction in dB (negative value).
    pub gain_reduction: f32,
    /// Peak input level.
    pub input_peak: f32,
    /// Peak output level.
    pub output_peak: f32,
    /// Peak gain reduction.
    pub gr_peak: f32,
    /// Gate state: 0.0 = closed, 1.0 = open.
    pub gate_state: f32,
    /// History of gain reduction values for waveform display.
    pub gr_history: Vec<f32>,
    /// History of input level values.
    pub input_history: Vec<f32>,
}

/// Default number of history samples to display.
pub const DEFAULT_HISTORY_SIZE: usize = 128;

/// dB range options for the graph.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GateDbRange {
    /// -48 to 0 dB.
    Range48,
    /// -60 to 0 dB.
    #[default]
    Range60,
    /// -80 to 0 dB.
    Range80,
    /// -96 to 0 dB.
    Range96,
}

impl GateDbRange {
    /// Get the minimum dB value.
    pub fn min_db(&self) -> f32 {
        match self {
            Self::Range48 => -48.0,
            Self::Range60 => -60.0,
            Self::Range80 => -80.0,
            Self::Range96 => -96.0,
        }
    }

    /// Get display label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Range48 => "48 dB",
            Self::Range60 => "60 dB",
            Self::Range80 => "80 dB",
            Self::Range96 => "96 dB",
        }
    }
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
    fn new(min_db: f32, size: f64) -> Self {
        let padding = 24.0;
        let graph_size = size - 2.0 * padding;
        Self {
            width: size,
            height: size,
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
}

/// Compute gate transfer function output for a given input.
fn compute_gate_transfer(
    input_db: f64,
    mode: GateMode,
    threshold: f64,
    ratio: f64,
    knee: f64,
    range: f64,
) -> f64 {
    let knee_w = knee / 2.0;
    let low_th = threshold - knee_w;
    let high_th = threshold + knee_w;

    match mode {
        GateMode::Gate | GateMode::Expander => {
            if input_db >= high_th {
                input_db
            } else if input_db > low_th && knee > 0.0 {
                let knee_factor = (high_th - input_db) / knee;
                let knee_factor_sq = knee_factor * knee_factor;
                let gain_reduction =
                    knee_factor_sq * (ratio - 1.0) * (input_db - threshold) / ratio;
                (input_db + gain_reduction).max(range)
            } else {
                let expanded = threshold + (input_db - threshold) * ratio;
                expanded.max(range)
            }
        }
        GateMode::Ducker => {
            if input_db <= low_th {
                input_db
            } else if input_db < high_th && knee > 0.0 {
                let knee_factor = (input_db - low_th) / knee;
                let knee_factor_sq = knee_factor * knee_factor;
                let gain_reduction = knee_factor_sq * (1.0 - 1.0 / ratio) * (threshold - input_db);
                (input_db + gain_reduction).max(range)
            } else {
                let ducked = threshold + (input_db - threshold) / ratio;
                ducked.max(range)
            }
        }
    }
}

/// Props for the GateGraph component.
#[derive(Props, Clone, PartialEq)]
pub struct GateGraphProps {
    /// Signal for gate parameters (allows two-way binding).
    pub params: Signal<GateParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: GateMetering,
    /// dB range for the graph.
    #[props(default)]
    pub db_range: GateDbRange,
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
    #[props(default = false)]
    pub show_gr_trace: bool,
    /// Whether to show the knob controls panel.
    #[props(default = true)]
    pub show_controls: bool,
    /// Whether interaction is enabled.
    #[props(default = true)]
    pub interactive: bool,
}

/// Gate graph component.
///
/// Renders a noise gate interface inspired by FabFilter Pro-G.
#[component]
pub fn GateGraph(props: GateGraphProps) -> Element {
    let layout = GraphLayout::new(props.db_range.min_db(), props.graph_size as f64);
    let mut params = props.params;

    // Create local signals bound to params
    let mut threshold_sig = use_signal(|| params.read().threshold);
    let mut ratio_sig = use_signal(|| params.read().ratio);
    let mut range_sig = use_signal(|| params.read().range);
    let mut knee_sig = use_signal(|| params.read().knee);
    let mut attack_sig = use_signal(|| params.read().attack);
    let mut hold_sig = use_signal(|| params.read().hold);
    let mut release_sig = use_signal(|| params.read().release);
    let mut lookahead_sig = use_signal(|| params.read().lookahead);

    // Sync from params to signals when params change externally
    let params_clone = params.read().clone();
    use_effect(move || {
        threshold_sig.set(params_clone.threshold);
        ratio_sig.set(params_clone.ratio);
        range_sig.set(params_clone.range);
        knee_sig.set(params_clone.knee);
        attack_sig.set(params_clone.attack);
        hold_sig.set(params_clone.hold);
        release_sig.set(params_clone.release);
        lookahead_sig.set(params_clone.lookahead);
    });

    // Use signal values for rendering (reactive)
    let threshold = threshold_sig() as f64;
    let ratio = ratio_sig() as f64;
    let knee = knee_sig() as f64;
    let range = range_sig() as f64;
    let mode = params.read().mode;
    let min_db = layout.min_db;

    // Generate transfer curve path
    let curve_path = {
        let mut path = String::new();
        let num_points = 100;
        for i in 0..=num_points {
            let input_db = min_db + (i as f64 / num_points as f64) * -min_db;
            let output_db = compute_gate_transfer(input_db, mode, threshold, ratio, knee, range);
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

    // Unity gain reference line
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

    // Range floor line
    let range_y = layout.db_to_y(range.max(min_db));

    // Current input/output for level visualization
    let input_level = props.metering.input_level.clamp(min_db as f32, 0.0) as f64;
    let input_x = layout.db_to_x(input_level);
    let input_y = layout.db_to_y(input_level);
    let output_db = compute_gate_transfer(input_level, mode, threshold, ratio, knee, range);
    let output_y = layout.db_to_y(output_db.clamp(min_db, 0.0));

    // GR meter dimensions
    let gr_meter_width = 8.0;
    let gr_meter_x = layout.width - layout.padding + 4.0;

    // Grid lines
    let grid_lines = if props.show_grid {
        let step = 12.0;
        let mut lines = Vec::new();
        let mut db = 0.0;
        while db > min_db {
            db -= step;
            if db > min_db {
                let pos = layout.db_to_x(db);
                lines.push(format!(
                    "M {:.1} {:.1} L {:.1} {:.1}",
                    pos,
                    layout.padding,
                    pos,
                    layout.padding + layout.graph_size
                ));
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

    // GR history trace
    let gr_trace_path = if props.show_gr_trace && !props.metering.gr_history.is_empty() {
        let history = &props.metering.gr_history;
        let num_samples = history.len();
        let samples_to_show = num_samples.min(DEFAULT_HISTORY_SIZE);
        let start_idx = num_samples.saturating_sub(samples_to_show);

        let mut path = String::new();
        let x_step = layout.graph_size / (samples_to_show.max(1) as f64 - 1.0).max(1.0);
        let right_x = layout.padding + layout.graph_size;
        let unity_y = layout.db_to_y(0.0);

        path.push_str(&format!("M {:.1} {:.1}", right_x, unity_y));
        for i in 0..samples_to_show {
            let x = right_x - (i as f64) * x_step;
            path.push_str(&format!(" L {:.1} {:.1}", x, unity_y));
        }
        for i in (0..samples_to_show).rev() {
            let sample_idx = start_idx + (samples_to_show - 1 - i);
            let gr = history.get(sample_idx).copied().unwrap_or(0.0);
            let output_db = gr.clamp(min_db as f32, 0.0) as f64;
            let x = right_x - (i as f64) * x_step;
            let y = layout.db_to_y(output_db);
            path.push_str(&format!(" L {:.1} {:.1}", x, y));
        }
        path.push_str(" Z");
        path
    } else {
        String::new()
    };

    // Gate state indicator
    let gate_state = props.metering.gate_state;
    let gate_indicator_color = if gate_state > 0.5 {
        "rgba(59, 130, 246, 0.9)"
    } else {
        "rgba(59, 130, 246, 0.2)"
    };

    // GR meter fill
    let gr_db = props.metering.gain_reduction.abs().min(-min_db as f32);
    let gr_height = (gr_db as f64 / -min_db) * layout.graph_size;

    // Blue theme colors
    let curve_color = "#3b82f6";
    let unity_color = "rgba(255, 255, 255, 0.12)";
    let grid_color = "rgba(255, 255, 255, 0.05)";
    let threshold_color = "#60a5fa";
    let range_color = "rgba(96, 165, 250, 0.2)";
    let gr_color = "#2563eb";
    let level_color = "#93c5fd";
    let _text_color = "rgba(255, 255, 255, 0.4)";

    // Value formatters
    let format_db = |v: f32| format!("{v:.0}dB");
    let format_ratio = |v: f32| {
        if v >= 100.0 {
            "∞:1".to_string()
        } else {
            format!("{v:.0}:1")
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

    rsx! {
        ThemeProvider { theme: ThemeContext::new(),
            div {
                class: "gate-graph flex gap-3 items-start",
                style: "background: linear-gradient(180deg, #0f172a 0%, #1e293b 100%); border-radius: 8px; padding: 12px;",

                // Left: Threshold + Ratio/Range knobs
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

                        // Ratio and Range row
                        div {
                            class: "flex gap-1",

                            Knob {
                                value: ratio_sig,
                                min: 1.0,
                                max: 100.0,
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
                                value: range_sig,
                                min: -96.0,
                                max: 0.0,
                                size: small_knob,
                                label: Some("RANGE".to_string()),
                                value_display: Some(format_db(range_sig())),
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    range_sig.set(v);
                                    params.write().range = v;
                                },
                            }
                        }
                    }
                }

                // Center: Transfer curve graph
                div {
                    class: "gate-graph-svg",
                    style: "width: {props.graph_size}px; height: {props.graph_size}px;",

                    svg {
                        width: "100%",
                        height: "100%",
                        view_box: "0 0 {layout.width} {layout.height}",
                        preserve_aspect_ratio: "xMidYMid meet",
                        style: "background: #080c14; border-radius: 6px;",

                        // Background
                        rect {
                            x: "{layout.padding}",
                            y: "{layout.padding}",
                            width: "{layout.graph_size}",
                            height: "{layout.graph_size}",
                            fill: "#050810",
                            rx: "3",
                        }

                        // Grid
                        if props.show_grid && !grid_lines.is_empty() {
                            path {
                                d: "{grid_lines}",
                                stroke: "{grid_color}",
                                stroke_width: "1",
                                fill: "none",
                            }
                        }

                        // Range floor region
                        rect {
                            x: "{layout.padding}",
                            y: "{range_y}",
                            width: "{layout.graph_size}",
                            height: "{(layout.padding + layout.graph_size - range_y).max(0.0)}",
                            fill: "{range_color}",
                        }

                        // GR trace
                        if props.show_gr_trace && !gr_trace_path.is_empty() {
                            path {
                                d: "{gr_trace_path}",
                                fill: "rgba(37, 99, 235, 0.2)",
                                stroke: "none",
                            }
                        }

                        // Unity line
                        path {
                            d: "{unity_path}",
                            stroke: "{unity_color}",
                            stroke_width: "1",
                            stroke_dasharray: "3,3",
                            fill: "none",
                        }

                        // Transfer curve
                        path {
                            d: "{curve_path}",
                            stroke: "{curve_color}",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            fill: "none",
                        }

                        // Threshold line
                        line {
                            x1: "{threshold_x}",
                            y1: "{layout.padding}",
                            x2: "{threshold_x}",
                            y2: "{layout.padding + layout.graph_size}",
                            stroke: "{threshold_color}",
                            stroke_width: "1",
                            stroke_dasharray: "3,2",
                        }

                        // Threshold point with gate state glow
                        circle {
                            cx: "{threshold_x}",
                            cy: "{threshold_y}",
                            r: "10",
                            fill: "none",
                            stroke: "{gate_indicator_color}",
                            stroke_width: "2",
                        }
                        circle {
                            cx: "{threshold_x}",
                            cy: "{threshold_y}",
                            r: "5",
                            fill: "{threshold_color}",
                            stroke: "#fff",
                            stroke_width: "1.5",
                        }

                        // Level indicator
                        if props.show_levels && props.metering.input_level > min_db as f32 {
                            line {
                                x1: "{input_x}",
                                y1: "{input_y}",
                                x2: "{input_x}",
                                y2: "{output_y}",
                                stroke: "{level_color}",
                                stroke_width: "1.5",
                                opacity: "0.6",
                            }
                            circle {
                                cx: "{input_x}",
                                cy: "{input_y}",
                                r: "2.5",
                                fill: "{level_color}",
                            }
                            circle {
                                cx: "{input_x}",
                                cy: "{output_y}",
                                r: "3.5",
                                fill: "{curve_color}",
                                stroke: "#fff",
                                stroke_width: "1",
                            }
                        }

                        // GR meter
                        if props.show_gr_meter {
                            rect {
                                x: "{gr_meter_x}",
                                y: "{layout.padding}",
                                width: "{gr_meter_width}",
                                height: "{layout.graph_size}",
                                fill: "#050810",
                                rx: "2",
                            }
                            rect {
                                x: "{gr_meter_x}",
                                y: "{layout.padding}",
                                width: "{gr_meter_width}",
                                height: "{gr_height}",
                                fill: "{gr_color}",
                                rx: "2",
                            }
                        }

                        // Threshold label
                        text {
                            x: "{threshold_x}",
                            y: "{layout.padding - 4.0}",
                            text_anchor: "middle",
                            fill: "{threshold_color}",
                            font_size: "8",
                            font_family: "system-ui",
                            "{threshold_sig():.0}dB"
                        }
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
                                min: 0.01,
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

                        // Hold row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: hold_sig,
                                min: 0.0,
                                max: 500.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    hold_sig.set(v);
                                    params.write().hold = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "HOLD" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_ms(hold_sig())}" }
                            }
                        }

                        // Release row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: release_sig,
                                min: 1.0,
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

                        // Knee row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: knee_sig,
                                min: 0.0,
                                max: 24.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    knee_sig.set(v);
                                    params.write().knee = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "KNEE" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_db(knee_sig())}" }
                            }
                        }

                        // Lookahead row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: lookahead_sig,
                                min: 0.0,
                                max: 10.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    lookahead_sig.set(v);
                                    params.write().lookahead = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "LOOK" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_ms(lookahead_sig())}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
