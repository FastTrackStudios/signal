//! Dioxus settings panel for the analyzer.
//!
//! Pure presentation: takes the current [`AnalyzerSettings`] and emits a new one
//! through `on_change`. The caller (eq-ui) owns the `Analyzer` and pushes changes
//! into it via [`spectrum_analyzer_dsp::Analyzer::set_settings`].

use audiocore_core::prelude::*;
use dioxus::prelude::*;
use fts_ui::prelude::{SegmentedControl, SegmentedControlSize, Switch};

use spectrum_analyzer_dsp::{AnalyzerSettings, Range, Resolution, Speed};

#[derive(Props, Clone, PartialEq)]
pub struct AnalyzerSettingsPanelProps {
    /// Current analyzer settings.
    pub settings: AnalyzerSettings,
    /// Called with the updated settings whenever a control changes.
    pub on_change: Callback<AnalyzerSettings>,
}

fn resolution_value(r: Resolution) -> &'static str {
    match r {
        Resolution::Low => "low",
        Resolution::Medium => "med",
        Resolution::High => "high",
        Resolution::Maximum => "max",
    }
}

fn resolution_from(v: &str) -> Resolution {
    match v {
        "low" => Resolution::Low,
        "high" => Resolution::High,
        "max" => Resolution::Maximum,
        _ => Resolution::Medium,
    }
}

fn speed_value(s: Speed) -> &'static str {
    match s {
        Speed::Slow => "slow",
        Speed::Medium => "med",
        Speed::Fast => "fast",
        Speed::VeryFast => "vfast",
    }
}

fn speed_from(v: &str) -> Speed {
    match v {
        "slow" => Speed::Slow,
        "fast" => Speed::Fast,
        "vfast" => Speed::VeryFast,
        _ => Speed::Medium,
    }
}

fn range_value(r: Range) -> &'static str {
    match r {
        Range::Db60 => "60",
        Range::Db90 => "90",
        Range::Db120 => "120",
    }
}

fn range_from(v: &str) -> Range {
    match v {
        "60" => Range::Db60,
        "120" => Range::Db120,
        _ => Range::Db90,
    }
}

fn tilt_value(t: f32) -> &'static str {
    if t < 1.5 {
        "0"
    } else if t < 3.75 {
        "3"
    } else if t < 5.25 {
        "4.5"
    } else {
        "6"
    }
}

fn tilt_from(v: &str) -> f32 {
    match v {
        "0" => 0.0,
        "3" => 3.0,
        "6" => 6.0,
        _ => 4.5,
    }
}

fn smoothing_value(o: f32) -> &'static str {
    if o <= 0.001 {
        "off"
    } else if o < 0.21 {
        "light"
    } else if o < 0.37 {
        "med"
    } else {
        "heavy"
    }
}

fn smoothing_from(v: &str) -> f32 {
    match v {
        "off" => 0.0,
        "light" => 1.0 / 6.0,
        "heavy" => 0.5,
        _ => 0.25,
    }
}

/// Analyzer settings panel. Renders resolution/speed/range selectors plus
/// pre/post/external/collision/freeze toggles.
#[component]
pub fn AnalyzerSettingsPanel(props: AnalyzerSettingsPanelProps) -> Element {
    let s = props.settings;
    let on_change = props.on_change;

    // Toggle signals, kept in sync with the incoming settings each render.
    let mut pre_sig = use_signal(|| s.show_pre);
    let mut post_sig = use_signal(|| s.show_post);
    let mut ext_sig = use_signal(|| s.show_external);
    let mut coll_sig = use_signal(|| s.show_collisions);
    let mut freeze_sig = use_signal(|| s.freeze);
    if *pre_sig.read() != s.show_pre {
        pre_sig.set(s.show_pre);
    }
    if *post_sig.read() != s.show_post {
        post_sig.set(s.show_post);
    }
    if *ext_sig.read() != s.show_external {
        ext_sig.set(s.show_external);
    }
    if *coll_sig.read() != s.show_collisions {
        coll_sig.set(s.show_collisions);
    }
    if *freeze_sig.read() != s.freeze {
        freeze_sig.set(s.freeze);
    }

    let label_cls = "text-[10px] uppercase tracking-wider text-muted-foreground mb-1";
    let row_cls = "flex items-center justify-between gap-2 py-0.5";

    rsx! {
        div { class: "flex flex-col gap-2 p-2 text-xs",
            // Resolution
            div {
                div { class: "{label_cls}", "Resolution" }
                SegmentedControl {
                    value: resolution_value(s.resolution).to_string(),
                    size: SegmentedControlSize::Small,
                    options: vec![
                        ("low".to_string(), "Low".to_string()),
                        ("med".to_string(), "Med".to_string()),
                        ("high".to_string(), "High".to_string()),
                        ("max".to_string(), "Max".to_string()),
                    ],
                    on_change: move |v: String| {
                        let mut ns = s;
                        ns.resolution = resolution_from(&v);
                        on_change.call(ns);
                    },
                }
            }
            // Speed
            div {
                div { class: "{label_cls}", "Speed" }
                SegmentedControl {
                    value: speed_value(s.speed).to_string(),
                    size: SegmentedControlSize::Small,
                    options: vec![
                        ("slow".to_string(), "Slow".to_string()),
                        ("med".to_string(), "Med".to_string()),
                        ("fast".to_string(), "Fast".to_string()),
                        ("vfast".to_string(), "V.Fast".to_string()),
                    ],
                    on_change: move |v: String| {
                        let mut ns = s;
                        ns.speed = speed_from(&v);
                        on_change.call(ns);
                    },
                }
            }
            // Range
            div {
                div { class: "{label_cls}", "Range" }
                SegmentedControl {
                    value: range_value(s.range).to_string(),
                    size: SegmentedControlSize::Small,
                    options: vec![
                        ("60".to_string(), "60 dB".to_string()),
                        ("90".to_string(), "90 dB".to_string()),
                        ("120".to_string(), "120 dB".to_string()),
                    ],
                    on_change: move |v: String| {
                        let mut ns = s;
                        ns.range = range_from(&v);
                        on_change.call(ns);
                    },
                }
            }
            // Tilt
            div {
                div { class: "{label_cls}", "Tilt (dB/oct)" }
                SegmentedControl {
                    value: tilt_value(s.tilt_db_per_oct).to_string(),
                    size: SegmentedControlSize::Small,
                    options: vec![
                        ("0".to_string(), "0".to_string()),
                        ("3".to_string(), "3".to_string()),
                        ("4.5".to_string(), "4.5".to_string()),
                        ("6".to_string(), "6".to_string()),
                    ],
                    on_change: move |v: String| {
                        let mut ns = s;
                        ns.tilt_db_per_oct = tilt_from(&v);
                        on_change.call(ns);
                    },
                }
            }
            // Smoothing
            div {
                div { class: "{label_cls}", "Smoothing" }
                SegmentedControl {
                    value: smoothing_value(s.smoothing_oct).to_string(),
                    size: SegmentedControlSize::Small,
                    options: vec![
                        ("off".to_string(), "Off".to_string()),
                        ("light".to_string(), "1/6".to_string()),
                        ("med".to_string(), "1/4".to_string()),
                        ("heavy".to_string(), "1/2".to_string()),
                    ],
                    on_change: move |v: String| {
                        let mut ns = s;
                        ns.smoothing_oct = smoothing_from(&v);
                        on_change.call(ns);
                    },
                }
            }
            // Toggles
            div { class: "{row_cls}",
                span { "Pre EQ" }
                Switch {
                    checked: pre_sig,
                    on_change: move |on: bool| {
                        let mut ns = s;
                        ns.show_pre = on;
                        on_change.call(ns);
                    },
                }
            }
            div { class: "{row_cls}",
                span { "Post EQ" }
                Switch {
                    checked: post_sig,
                    on_change: move |on: bool| {
                        let mut ns = s;
                        ns.show_post = on;
                        on_change.call(ns);
                    },
                }
            }
            div { class: "{row_cls}",
                span { "SC / Ext" }
                Switch {
                    checked: ext_sig,
                    on_change: move |on: bool| {
                        let mut ns = s;
                        ns.show_external = on;
                        on_change.call(ns);
                    },
                }
            }
            div { class: "{row_cls}",
                span { "Collisions" }
                Switch {
                    checked: coll_sig,
                    on_change: move |on: bool| {
                        let mut ns = s;
                        ns.show_collisions = on;
                        on_change.call(ns);
                    },
                }
            }
            div { class: "{row_cls}",
                span { "Freeze" }
                Switch {
                    checked: freeze_sig,
                    on_change: move |on: bool| {
                        let mut ns = s;
                        ns.freeze = on;
                        on_change.call(ns);
                    },
                }
            }
        }
    }
}
