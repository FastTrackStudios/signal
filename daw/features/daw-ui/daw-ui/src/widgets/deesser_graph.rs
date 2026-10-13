use crate::prelude::*;

// =============================================================================
// Data Types
// =============================================================================

/// De-esser mode.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum DeEsserMode {
    /// Wideband compression (affects entire signal).
    #[default]
    Wideband,
    /// Split mode (only reduces sibilant frequencies).
    Split,
}

impl DeEsserMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeEsserMode::Wideband => "Wideband",
            DeEsserMode::Split => "Split",
        }
    }
}

/// Parameters for the de-esser effect.
#[derive(Clone, PartialEq, Debug)]
pub struct DeEsserParams {
    pub frequency: f32, // 2000-16000 Hz (center freq of detection band)
    pub range: f32,     // 0-24 dB (max reduction)
    pub threshold: f32, // -60 to 0 dB
    pub ratio: f32,     // 1:1 to 10:1
    pub attack: f32,    // 0.01-10 ms
    pub release: f32,   // 10-500 ms
    pub listen: bool,   // Solo the sibilance band
    pub mode: DeEsserMode,
}

impl Default for DeEsserParams {
    fn default() -> Self {
        Self {
            frequency: 8000.0,
            range: 12.0,
            threshold: -24.0,
            ratio: 4.0,
            attack: 0.5,
            release: 100.0,
            listen: false,
            mode: DeEsserMode::default(),
        }
    }
}

/// Real-time metering data for the de-esser.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct DeEsserMetering {
    pub input_level: f32,     // dB (broadband)
    pub sibilance_level: f32, // dB (level in detection band)
    pub gain_reduction: f32,  // dB (current GR)
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

    /// Convert X position to frequency in Hz.
    fn x_to_freq(&self, x: f64) -> f64 {
        let min_freq = 20.0_f64.ln();
        let max_freq = 20000.0_f64.ln();
        let normalized = (x - self.padding) / self.graph_width;
        (min_freq + normalized * (max_freq - min_freq)).exp()
    }
}

// =============================================================================
// Bell Filter Response Calculation
// =============================================================================

/// Compute the gain response of a bell filter at a given frequency.
fn bell_response(freq: f32, center_freq: f32, gain_db: f32, q: f32) -> f32 {
    if gain_db.abs() < 0.01 {
        return 0.0;
    }

    let w0 = 2.0 * std::f32::consts::PI * center_freq / 48000.0;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let alpha = sin_w0 / (2.0 * q);
    let a = 10_f32.powf(gain_db / 40.0);

    let b0 = 1.0 + alpha * a;
    let b1 = -2.0 * cos_w0;
    let b2 = 1.0 - alpha * a;
    let a0 = 1.0 + alpha / a;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha / a;

    let w = 2.0 * std::f32::consts::PI * freq / 48000.0;
    let cos_w = w.cos();
    let sin_w = w.sin();

    let numerator_re = b0 + b1 * cos_w + b2 * (2.0 * cos_w * cos_w - 1.0);
    let numerator_im = b1 * sin_w + b2 * 2.0 * cos_w * sin_w;
    let denominator_re = a0 + a1 * cos_w + a2 * (2.0 * cos_w * cos_w - 1.0);
    let denominator_im = a1 * sin_w + a2 * 2.0 * cos_w * sin_w;

    let magnitude = ((numerator_re * numerator_re + numerator_im * numerator_im)
        / (denominator_re * denominator_re + denominator_im * denominator_im))
        .sqrt();

    20.0 * magnitude.log10()
}

// =============================================================================
// DeEsserGraph - Graph-only component
// =============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct DeEsserGraphProps {
    /// Current de-esser parameters.
    pub params: Signal<DeEsserParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: DeEsserMetering,
    /// Graph width in pixels.
    #[props(default = 280)]
    pub width: u32,
    /// Graph height in pixels.
    #[props(default = 200)]
    pub height: u32,
    /// Whether to show the grid.
    #[props(default = true)]
    pub show_grid: bool,
    /// Whether to show the GR meter.
    #[props(default = true)]
    pub show_gr_meter: bool,
    /// Whether interaction is enabled.
    #[props(default = true)]
    pub interactive: bool,
    /// Callback when frequency is changed via dragging.
    #[props(default)]
    pub on_frequency_change: Option<Callback<f32>>,
}

#[component]
pub fn DeEsserGraph(props: DeEsserGraphProps) -> Element {
    let params = props.params.read();
    let layout = GraphLayout::new(props.width, props.height);

    let mut dragging = use_signal(|| false);

    // Generate frequency response curve (showing reduction in detection band)
    let num_points = 200;
    let mut curve_path = String::new();

    for i in 0..=num_points {
        let t = i as f64 / num_points as f64;
        let freq = (20.0_f64 * (20000.0_f64 / 20.0_f64).powf(t)) as f32;

        // Inverted bell curve for de-essing (shows reduction)
        let response = -bell_response(freq, params.frequency, params.range, 2.0);

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

    // Center frequency marker
    let center_x = layout.freq_to_x(params.frequency as f64);

    // GR meter dimensions (right side)
    let gr_meter_width = 12.0;
    let gr_meter_x = layout.padding + layout.graph_width + 8.0;
    let gr_normalized = (props.metering.gain_reduction.abs() / 24.0).clamp(0.0, 1.0);
    let gr_height = layout.graph_height * gr_normalized as f64;

    // Frequency grid markers (logarithmic)
    let freq_markers = vec![100.0, 1000.0, 10000.0];

    // Theme colors (purple/violet)
    let curve_color = "#8b5cf6";
    let unity_color = "rgba(255, 255, 255, 0.2)";
    let grid_color = "rgba(255, 255, 255, 0.08)";
    let center_color = "#a78bfa";
    let gr_color = "#c084fc";
    let text_color = "rgba(255, 255, 255, 0.6)";

    // Mouse event handlers for dragging frequency
    let onmousedown = {
        let interactive = props.interactive;
        move |evt: MouseEvent| {
            if !interactive {
                return;
            }
            let coords = evt.element_coordinates();
            let mx = coords.x;

            // Check if clicking near center frequency line
            let dist = (mx - center_x).abs();
            if dist < 20.0 {
                dragging.set(true);
            }
        }
    };

    let onmousemove = {
        let on_frequency_change = props.on_frequency_change.clone();
        move |evt: MouseEvent| {
            if !*dragging.read() {
                return;
            }

            let coords = evt.element_coordinates();
            let new_freq = layout.x_to_freq(coords.x) as f32;
            let clamped_freq = new_freq.clamp(2000.0, 16000.0);

            if let Some(cb) = &on_frequency_change {
                cb.call(clamped_freq);
            }
        }
    };

    let onmouseup = move |_evt: MouseEvent| {
        dragging.set(false);
    };

    let onmouseleave = move |_evt: MouseEvent| {
        dragging.set(false);
    };

    rsx! {
        svg {
            width: "{props.width}",
            height: "{props.height}",
            view_box: "0 0 {layout.width} {layout.height}",
            onmousedown,
            onmousemove,
            onmouseup,
            onmouseleave,

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

            // Detection band highlight (filled area under curve)
            path {
                d: "{curve_path} L {layout.padding + layout.graph_width} {unity_y} L {layout.padding} {unity_y} Z",
                fill: "{curve_color}",
                fill_opacity: "0.15",
            }

            // Frequency response curve
            path {
                d: "{curve_path}",
                stroke: "{curve_color}",
                stroke_width: "2.5",
                stroke_linecap: "round",
                fill: "none",
            }

            // Center frequency marker (draggable)
            line {
                x1: "{center_x}",
                y1: "{layout.padding}",
                x2: "{center_x}",
                y2: "{layout.padding + layout.graph_height}",
                stroke: "{center_color}",
                stroke_width: "2",
                stroke_dasharray: "4,2",
            }

            if props.interactive {
                circle {
                    cx: "{center_x}",
                    cy: "{unity_y}",
                    r: "8",
                    fill: "{center_color}",
                    stroke: "#fff",
                    stroke_width: "2",
                    style: "cursor: ew-resize;",
                }
            }

            // GR meter
            if props.show_gr_meter {
                // GR meter background
                rect {
                    x: "{gr_meter_x}",
                    y: "{layout.padding}",
                    width: "{gr_meter_width}",
                    height: "{layout.graph_height}",
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
                // GR label
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
                    y: "{layout.padding + layout.graph_height + 14.0}",
                    text_anchor: "middle",
                    fill: "{text_color}",
                    font_size: "9",
                    font_family: "system-ui, -apple-system, sans-serif",
                    "{props.metering.gain_reduction:.1}"
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
                "Reduction (dB)"
            }

            // Center frequency label
            text {
                x: "{center_x}",
                y: "{layout.padding - 12.0}",
                text_anchor: "middle",
                fill: "{center_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                font_weight: "600",
                "{params.frequency:.0} Hz"
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
        }
    }
}

// =============================================================================
// DeEsserWidget - Full widget with controls
// =============================================================================

use crate::theming::{ThemeContext, ThemeProvider};
use crate::widgets::knob::Knob;

#[derive(Props, Clone, PartialEq)]
pub struct DeEsserWidgetProps {
    /// Signal for de-esser parameters (allows two-way binding).
    pub params: Signal<DeEsserParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: DeEsserMetering,
    /// Size of the graph in pixels.
    #[props(default = 280)]
    pub graph_width: u32,
    #[props(default = 200)]
    pub graph_height: u32,
    /// Whether to show the grid.
    #[props(default = true)]
    pub show_grid: bool,
    /// Whether to show the GR meter.
    #[props(default = true)]
    pub show_gr_meter: bool,
    /// Whether to show the knob controls panel.
    #[props(default = true)]
    pub show_controls: bool,
    /// Whether interaction is enabled.
    #[props(default = true)]
    pub interactive: bool,
}

#[component]
pub fn DeEsserWidget(props: DeEsserWidgetProps) -> Element {
    let mut params = props.params;

    // Create local signals bound to params
    let mut frequency_sig = use_signal(|| params.read().frequency);
    let mut range_sig = use_signal(|| params.read().range);
    let mut threshold_sig = use_signal(|| params.read().threshold);
    let mut attack_sig = use_signal(|| params.read().attack);
    let mut release_sig = use_signal(|| params.read().release);

    // Sync from params to signals when params change externally
    let params_clone = params.read().clone();
    use_effect(move || {
        frequency_sig.set(params_clone.frequency);
        range_sig.set(params_clone.range);
        threshold_sig.set(params_clone.threshold);
        attack_sig.set(params_clone.attack);
        release_sig.set(params_clone.release);
    });

    // Value formatters
    let format_hz = |v: f32| format!("{v:.0}Hz");
    let format_db = |v: f32| format!("{v:.1}dB");
    let format_ms = |v: f32| {
        if v >= 1000.0 {
            format!("{:.1}s", v / 1000.0)
        } else {
            format!("{v:.1}ms")
        }
    };

    // Knob sizes
    let large_knob = 56_u32;
    let tiny_knob = 32_u32;

    // Build current params for the graph
    let mut current_params_sig = use_signal(|| DeEsserParams {
        frequency: frequency_sig(),
        range: range_sig(),
        threshold: threshold_sig(),
        attack: attack_sig(),
        release: release_sig(),
        ..params.read().clone()
    });

    // Update signal when params change
    use_effect(move || {
        current_params_sig.set(DeEsserParams {
            frequency: frequency_sig(),
            range: range_sig(),
            threshold: threshold_sig(),
            attack: attack_sig(),
            release: release_sig(),
            ..params.read().clone()
        });
    });

    rsx! {
        ThemeProvider { theme: ThemeContext::new(),
            div {
                class: "deesser-widget flex gap-3 items-start",
                style: "background: linear-gradient(180deg, #111111 0%, #1a1a1a 100%); border-radius: 8px; padding: 12px;",

                // Left: Frequency + Range knobs
                if props.show_controls {
                    div {
                        class: "flex flex-col items-center gap-2",
                        style: "min-width: 70px;",

                        // Frequency knob
                        Knob {
                            value: frequency_sig,
                            min: 2000.0,
                            max: 16000.0,
                            size: large_knob,
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                frequency_sig.set(v);
                                params.write().frequency = v;
                            },
                        }
                        div {
                            class: "flex flex-col items-center gap-0",
                            span { class: "text-[10px] text-gray-500 leading-none", "FREQ" }
                            span { class: "text-[11px] text-gray-300 leading-tight font-mono", "{format_hz(frequency_sig())}" }
                        }

                        // Range knob
                        Knob {
                            value: range_sig,
                            min: 0.0,
                            max: 24.0,
                            size: large_knob,
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                range_sig.set(v);
                                params.write().range = v;
                            },
                        }
                        div {
                            class: "flex flex-col items-center gap-0",
                            span { class: "text-[10px] text-gray-500 leading-none", "RANGE" }
                            span { class: "text-[11px] text-gray-300 leading-tight font-mono", "{format_db(range_sig())}" }
                        }
                    }
                }

                // Center: Graph
                DeEsserGraph {
                    params: current_params_sig,
                    metering: props.metering.clone(),
                    width: props.graph_width,
                    height: props.graph_height,
                    show_grid: props.show_grid,
                    show_gr_meter: props.show_gr_meter,
                    interactive: props.interactive,
                    on_frequency_change: move |v: f32| {
                        frequency_sig.set(v);
                        params.write().frequency = v;
                    },
                }

                // Right: Secondary controls (compact horizontal rows)
                if props.show_controls {
                    div {
                        class: "flex flex-col gap-0",
                        style: "padding-left: 8px; border-left: 1px solid rgba(255,255,255,0.08);",

                        // Threshold row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: threshold_sig,
                                min: -60.0,
                                max: 0.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    threshold_sig.set(v);
                                    params.write().threshold = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "THR" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_db(threshold_sig())}" }
                            }
                        }

                        // Attack row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: attack_sig,
                                min: 0.01,
                                max: 10.0,
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
                                max: 500.0,
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
                    }
                }
            }
        }
    }
}
