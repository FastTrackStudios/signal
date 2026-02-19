//! Automation Lane UI — timeline-based parameter automation curves.
//!
//! Displays automation lanes for parameters with editable breakpoints.
//! Each lane shows the parameter name, current value, and a timeline
//! with interpolation curves between breakpoints.

use dioxus::prelude::*;

/// A single automation breakpoint.
#[derive(Clone, PartialEq)]
pub struct AutomationPoint {
    /// Position in beats (or seconds).
    pub time: f64,
    /// Parameter value (0.0–1.0).
    pub value: f64,
}

/// An automation lane for a single parameter.
#[derive(Clone, PartialEq)]
pub struct AutomationLaneData {
    pub id: String,
    pub param_name: String,
    pub module_name: String,
    pub current_value: f64,
    pub points: Vec<AutomationPoint>,
    pub is_enabled: bool,
}

/// Automation lane strip (one per parameter).
#[derive(Props, Clone, PartialEq)]
pub struct AutomationLaneProps {
    /// Lane data.
    lane: AutomationLaneData,

    /// Visible time range (start, end) in beats.
    #[props(default = (0.0, 16.0))]
    time_range: (f64, f64),

    /// Width of the timeline area in pixels.
    #[props(default = 400)]
    width: u32,

    /// Height of the lane in pixels.
    #[props(default = 60)]
    height: u32,

    /// Callback to toggle lane enabled.
    #[props(default)]
    on_toggle: Option<Callback<String>>,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn AutomationLane(props: AutomationLaneProps) -> Element {
    let wf = props.width as f64;
    let hf = props.height as f64;
    let (t_start, t_end) = props.time_range;
    let t_range = (t_end - t_start).max(0.001);

    let enabled_class = if props.lane.is_enabled {
        ""
    } else {
        " opacity-40"
    };

    // Build SVG polyline from points
    let points_str: String = props
        .lane
        .points
        .iter()
        .map(|p| {
            let x = ((p.time - t_start) / t_range * wf).clamp(0.0, wf);
            let y = ((1.0 - p.value) * hf).clamp(0.0, hf);
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ");

    let lane_id = props.lane.id.clone();

    rsx! {
        div {
            class: format!("flex items-stretch gap-0 border-b border-border{enabled_class} {}", props.class),

            // Label column
            div {
                class: "flex flex-col justify-center px-2 py-1 w-32 border-r border-border bg-muted/30",
                div {
                    class: "flex items-center gap-1",
                    button {
                        class: format!(
                            "w-3 h-3 rounded-sm border {}",
                            if props.lane.is_enabled { "bg-primary border-primary" } else { "border-muted-foreground" }
                        ),
                        onclick: move |_| {
                            if let Some(cb) = &props.on_toggle {
                                cb.call(lane_id.clone());
                            }
                        },
                    }
                    span {
                        class: "text-[10px] font-medium truncate",
                        "{props.lane.param_name}"
                    }
                }
                span {
                    class: "text-[10px] text-muted-foreground truncate",
                    "{props.lane.module_name}"
                }
                span {
                    class: "text-[10px] font-mono text-primary",
                    {format!("{:.1}%", props.lane.current_value * 100.0)}
                }
            }

            // Timeline area
            div {
                class: "relative",
                style: "width: {props.width}px; height: {props.height}px;",

                svg {
                    width: "{props.width}",
                    height: "{props.height}",
                    view_box: "0 0 {wf} {hf}",

                    // Grid lines (25%, 50%, 75%)
                    for pct in [0.25, 0.5, 0.75] {
                        {
                            let y = (1.0 - pct) * hf;
                            rsx! {
                                line {
                                    x1: "0",
                                    y1: "{y:.1}",
                                    x2: "{wf}",
                                    y2: "{y:.1}",
                                    stroke: "var(--border, #333)",
                                    stroke_width: "0.5",
                                    stroke_dasharray: "2 2",
                                }
                            }
                        }
                    }

                    // Automation curve
                    if !points_str.is_empty() {
                        polyline {
                            points: "{points_str}",
                            fill: "none",
                            stroke: "var(--primary, #3b82f6)",
                            stroke_width: "1.5",
                            stroke_linejoin: "round",
                        }
                    }

                    // Breakpoint dots
                    for point in props.lane.points.iter() {
                        {
                            let cx = ((point.time - t_start) / t_range * wf).clamp(0.0, wf);
                            let cy = ((1.0 - point.value) * hf).clamp(0.0, hf);
                            rsx! {
                                circle {
                                    cx: "{cx:.1}",
                                    cy: "{cy:.1}",
                                    r: "3",
                                    fill: "var(--background, #000)",
                                    stroke: "var(--primary, #3b82f6)",
                                    stroke_width: "1.5",
                                }
                            }
                        }
                    }
                }

                // Playhead position indicator (current value)
                {
                    let cur_y = ((1.0 - props.lane.current_value) * hf).clamp(0.0, hf);
                    rsx! {
                        div {
                            class: "absolute left-0 w-full h-[1px] bg-foreground/30",
                            style: "top: {cur_y:.1}px;",
                        }
                    }
                }
            }
        }
    }
}

/// Container for multiple automation lanes.
#[derive(Props, Clone, PartialEq)]
pub struct AutomationLaneListProps {
    /// Lanes to display.
    lanes: Vec<AutomationLaneData>,

    /// Visible time range.
    #[props(default = (0.0, 16.0))]
    time_range: (f64, f64),

    /// Timeline width.
    #[props(default = 400)]
    width: u32,

    /// Callback to toggle a lane.
    #[props(default)]
    on_toggle: Option<Callback<String>>,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn AutomationLaneList(props: AutomationLaneListProps) -> Element {
    rsx! {
        div {
            class: format!("flex flex-col border-t border-border {}", props.class),
            for lane in props.lanes.iter() {
                AutomationLane {
                    lane: lane.clone(),
                    time_range: props.time_range,
                    width: props.width,
                    on_toggle: props.on_toggle,
                }
            }
        }
    }
}
