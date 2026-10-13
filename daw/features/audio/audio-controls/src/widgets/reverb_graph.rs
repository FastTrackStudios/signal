use crate::prelude::*;

// =============================================================================
// Data Types
// =============================================================================

/// Parameters for the reverb effect.
#[derive(Clone, PartialEq, Debug)]
pub struct ReverbParams {
    pub size: f32,       // 0-100% (room size)
    pub decay: f32,      // 0.1-10 s (decay time)
    pub predelay: f32,   // 0-200 ms
    pub damping: f32,    // 0-100% (high frequency damping)
    pub mix: f32,        // 0-100%
    pub diffusion: f32,  // 0-100% (echo density)
    pub modulation: f32, // 0-100% (chorus effect)
    pub low_cut: f32,    // 20-500 Hz
    pub high_cut: f32,   // 1000-20000 Hz
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            size: 50.0,
            decay: 2.0,
            predelay: 20.0,
            damping: 50.0,
            mix: 30.0,
            diffusion: 70.0,
            modulation: 10.0,
            low_cut: 20.0,
            high_cut: 20000.0,
        }
    }
}

/// Real-time metering data for the reverb.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct ReverbMetering {
    pub impulse_response: Vec<f32>, // Impulse response (amplitude over time)
    pub wet_level: f32,             // Current wet output level (0-1)
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
        let padding = 30.0;
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

    /// Convert time in seconds to X position.
    fn time_to_x(&self, time: f64, max_time: f64) -> f64 {
        self.padding + (time / max_time) * self.graph_width
    }

    /// Convert amplitude (0-1) to Y position.
    fn amp_to_y(&self, amp: f64) -> f64 {
        self.padding + self.graph_height * (1.0 - amp)
    }
}

// =============================================================================
// Decay Envelope Calculation
// =============================================================================

/// Compute the reverb decay envelope at a given time.
fn compute_decay_envelope(time_s: f32, params: &ReverbParams) -> f32 {
    // Predelay: no reverb before predelay time
    if time_s < params.predelay / 1000.0 {
        return 0.0;
    }

    let time_after_predelay = time_s - params.predelay / 1000.0;

    // Basic exponential decay
    let decay_factor = (-time_after_predelay / params.decay).exp();

    // Apply damping (high frequencies decay faster)
    let damping_factor = 1.0 - params.damping / 100.0;
    let damped_decay =
        decay_factor * (1.0 - (1.0 - damping_factor) * time_after_predelay / params.decay);

    // Apply diffusion (affects envelope smoothness)
    let diffusion_noise = (time_after_predelay * 10.0).sin() * 0.05 * params.diffusion / 100.0;

    // Apply modulation (small variations)
    let modulation_factor =
        1.0 + (time_after_predelay * 3.0).sin() * 0.1 * params.modulation / 100.0;

    (damped_decay + diffusion_noise) * modulation_factor
}

// =============================================================================
// ReverbGraph - Graph-only component
// =============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct ReverbGraphProps {
    /// Current reverb parameters.
    pub params: Signal<ReverbParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: ReverbMetering,
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
pub fn ReverbGraph(props: ReverbGraphProps) -> Element {
    let params = props.params.read();
    let layout = GraphLayout::new(props.width, props.height);

    // Maximum time to display (based on decay time)
    let max_time = (params.decay * 1.5).max(1.0);

    // Generate decay envelope curve
    let num_points = 200;
    let mut curve_path = String::new();

    for i in 0..=num_points {
        let t = i as f64 / num_points as f64;
        let time_s = (t * max_time as f64) as f32;
        let amplitude = compute_decay_envelope(time_s, &params);

        let x = layout.time_to_x(time_s as f64, max_time as f64);
        let y = layout.amp_to_y(amplitude as f64);

        if i == 0 {
            curve_path.push_str(&format!("M {x:.1} {y:.1}"));
        } else {
            curve_path.push_str(&format!(" L {x:.1} {y:.1}"));
        }
    }

    // Predelay marker
    let predelay_x = layout.time_to_x((params.predelay / 1000.0) as f64, max_time as f64);

    // Time grid markers
    let time_markers: Vec<f32> = if max_time <= 2.0 {
        vec![0.0, 0.5, 1.0, 1.5, 2.0]
    } else if max_time <= 5.0 {
        vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]
    } else {
        vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0]
    };

    // Filter out markers beyond max_time
    let time_markers: Vec<f32> = time_markers
        .into_iter()
        .filter(|&t| t <= max_time)
        .collect();

    // Theme colors (indigo)
    let curve_color = "#6366f1";
    let predelay_color = "#818cf8";
    let grid_color = "rgba(255, 255, 255, 0.08)";
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

            // Horizontal grid lines (amplitude)
            for i in 0..=4 {
                line {
                    x1: "{layout.padding}",
                    y1: "{layout.padding + (layout.graph_height * i as f64 / 4.0)}",
                    x2: "{layout.padding + layout.graph_width}",
                    y2: "{layout.padding + (layout.graph_height * i as f64 / 4.0)}",
                    stroke: "{grid_color}",
                    stroke_width: "1",
                }
            }

            // Vertical grid lines (time)
            for time in time_markers.iter() {
                line {
                    x1: "{layout.time_to_x(*time as f64, max_time as f64)}",
                    y1: "{layout.padding}",
                    x2: "{layout.time_to_x(*time as f64, max_time as f64)}",
                    y2: "{layout.padding + layout.graph_height}",
                    stroke: "{grid_color}",
                    stroke_width: "1",
                }
            }

            // Predelay marker (vertical dashed line)
            if params.predelay > 0.0 {
                line {
                    x1: "{predelay_x}",
                    y1: "{layout.padding}",
                    x2: "{predelay_x}",
                    y2: "{layout.padding + layout.graph_height}",
                    stroke: "{predelay_color}",
                    stroke_width: "2",
                    stroke_dasharray: "4,2",
                }
            }

            // Filled area under curve
            path {
                d: "{curve_path} L {layout.padding + layout.graph_width} {layout.padding + layout.graph_height} L {layout.padding} {layout.padding + layout.graph_height} Z",
                fill: "{curve_color}",
                fill_opacity: "0.2",
            }

            // Decay envelope curve
            path {
                d: "{curve_path}",
                stroke: "{curve_color}",
                stroke_width: "2.5",
                stroke_linecap: "round",
                fill: "none",
            }

            // Wet level indicator (if metering available)
            if props.metering.wet_level > 0.0 {
                line {
                    x1: "{layout.padding}",
                    y1: "{layout.amp_to_y(props.metering.wet_level as f64)}",
                    x2: "{layout.padding + layout.graph_width}",
                    y2: "{layout.amp_to_y(props.metering.wet_level as f64)}",
                    stroke: "#a5b4fc",
                    stroke_width: "1.5",
                    opacity: "0.6",
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
                "Time (s)"
            }
            text {
                x: "12",
                y: "{layout.height / 2.0}",
                text_anchor: "middle",
                fill: "{text_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                transform: "rotate(-90, 12, {layout.height / 2.0})",
                "Level"
            }

            // Decay time label
            text {
                x: "{layout.width / 2.0}",
                y: "{layout.padding - 12.0}",
                text_anchor: "middle",
                fill: "{curve_color}",
                font_size: "11",
                font_family: "system-ui, -apple-system, sans-serif",
                font_weight: "600",
                if params.decay >= 1.0 {
                    "{params.decay:.1}s decay"
                } else {
                    "{(params.decay * 1000.0):.0}ms decay"
                }
            }

            // Time markers
            for time in time_markers.iter() {
                text {
                    x: "{layout.time_to_x(*time as f64, max_time as f64)}",
                    y: "{layout.padding + layout.graph_height + 14.0}",
                    text_anchor: "middle",
                    fill: "{text_color}",
                    font_size: "9",
                    font_family: "system-ui, -apple-system, sans-serif",
                    if *time >= 1.0 {
                        "{*time:.1}"
                    } else {
                        "{(*time * 1000.0):.0}ms"
                    }
                }
            }
        }
    }
}

// =============================================================================
// ReverbWidget - Full widget with controls
// =============================================================================

use crate::theming::{ThemeContext, ThemeProvider};
use crate::widgets::knob::Knob;

#[derive(Props, Clone, PartialEq)]
pub struct ReverbWidgetProps {
    /// Signal for reverb parameters (allows two-way binding).
    pub params: Signal<ReverbParams>,
    /// Real-time metering data.
    #[props(default)]
    pub metering: ReverbMetering,
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
pub fn ReverbWidget(props: ReverbWidgetProps) -> Element {
    let mut params = props.params;

    // Create local signals bound to params
    let mut size_sig = use_signal(|| params.read().size);
    let mut decay_sig = use_signal(|| params.read().decay);
    let mut predelay_sig = use_signal(|| params.read().predelay);
    let mut damping_sig = use_signal(|| params.read().damping);
    let mut mix_sig = use_signal(|| params.read().mix);
    let mut diffusion_sig = use_signal(|| params.read().diffusion);
    let mut modulation_sig = use_signal(|| params.read().modulation);
    let mut low_cut_sig = use_signal(|| params.read().low_cut);
    let mut high_cut_sig = use_signal(|| params.read().high_cut);

    // Sync from params to signals when params change externally
    let params_clone = params.read().clone();
    use_effect(move || {
        size_sig.set(params_clone.size);
        decay_sig.set(params_clone.decay);
        predelay_sig.set(params_clone.predelay);
        damping_sig.set(params_clone.damping);
        mix_sig.set(params_clone.mix);
        diffusion_sig.set(params_clone.diffusion);
        modulation_sig.set(params_clone.modulation);
        low_cut_sig.set(params_clone.low_cut);
        high_cut_sig.set(params_clone.high_cut);
    });

    // Value formatters
    let format_percent = |v: f32| format!("{v:.0}%");
    let format_time = |v: f32| {
        if v >= 1.0 {
            format!("{v:.1}s")
        } else {
            format!("{v:.0}ms")
        }
    };
    let format_ms = |v: f32| format!("{v:.0}ms");
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
    let mut current_params_sig = use_signal(|| ReverbParams {
        size: size_sig(),
        decay: decay_sig(),
        predelay: predelay_sig(),
        damping: damping_sig(),
        mix: mix_sig(),
        diffusion: diffusion_sig(),
        modulation: modulation_sig(),
        low_cut: low_cut_sig(),
        high_cut: high_cut_sig(),
    });

    // Update signal when params change
    use_effect(move || {
        current_params_sig.set(ReverbParams {
            size: size_sig(),
            decay: decay_sig(),
            predelay: predelay_sig(),
            damping: damping_sig(),
            mix: mix_sig(),
            diffusion: diffusion_sig(),
            modulation: modulation_sig(),
            low_cut: low_cut_sig(),
            high_cut: high_cut_sig(),
        });
    });

    rsx! {
        ThemeProvider { theme: ThemeContext::new(),
            div {
                class: "reverb-widget flex gap-3 items-start",
                style: "background: linear-gradient(180deg, #111111 0%, #1a1a1a 100%); border-radius: 8px; padding: 12px;",

                // Left: Size + Decay + Predelay knobs
                if props.show_controls {
                    div {
                        class: "flex flex-col items-center gap-2",
                        style: "min-width: 70px;",

                        // Size knob
                        Knob {
                            value: size_sig,
                            min: 0.0,
                            max: 100.0,
                            size: large_knob,
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                size_sig.set(v);
                                params.write().size = v;
                            },
                        }
                        div {
                            class: "flex flex-col items-center gap-0",
                            span { class: "text-[10px] text-gray-500 leading-none", "SIZE" }
                            span { class: "text-[11px] text-gray-300 leading-tight font-mono", "{format_percent(size_sig())}" }
                        }

                        // Decay knob
                        Knob {
                            value: decay_sig,
                            min: 0.1,
                            max: 10.0,
                            size: large_knob,
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                decay_sig.set(v);
                                params.write().decay = v;
                            },
                        }
                        div {
                            class: "flex flex-col items-center gap-0",
                            span { class: "text-[10px] text-gray-500 leading-none", "DECAY" }
                            span { class: "text-[11px] text-gray-300 leading-tight font-mono", "{format_time(decay_sig())}" }
                        }

                        // Predelay knob
                        Knob {
                            value: predelay_sig,
                            min: 0.0,
                            max: 200.0,
                            size: large_knob,
                            disabled: !props.interactive,
                            on_change: move |v: f32| {
                                predelay_sig.set(v);
                                params.write().predelay = v;
                            },
                        }
                        div {
                            class: "flex flex-col items-center gap-0",
                            span { class: "text-[10px] text-gray-500 leading-none", "PREDLY" }
                            span { class: "text-[11px] text-gray-300 leading-tight font-mono", "{format_ms(predelay_sig())}" }
                        }
                    }
                }

                // Center: Graph
                ReverbGraph {
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

                        // Damping row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: damping_sig,
                                min: 0.0,
                                max: 100.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    damping_sig.set(v);
                                    params.write().damping = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "DAMP" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_percent(damping_sig())}" }
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

                        // Diffusion row
                        div {
                            class: "flex items-center gap-1",
                            Knob {
                                value: diffusion_sig,
                                min: 0.0,
                                max: 100.0,
                                size: tiny_knob,
                                disabled: !props.interactive,
                                on_change: move |v: f32| {
                                    diffusion_sig.set(v);
                                    params.write().diffusion = v;
                                },
                            }
                            div {
                                class: "flex flex-col",
                                style: "min-width: 44px;",
                                span { class: "text-[9px] text-gray-500 leading-none", "DIFF" }
                                span { class: "text-[10px] text-gray-300 leading-tight font-mono", "{format_percent(diffusion_sig())}" }
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
                    }
                }
            }
        }
    }
}
