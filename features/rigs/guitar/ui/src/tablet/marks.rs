//! The small marks the tablet draws — the prototype's `OverrideIcon`,
//! `InheritIcon`, `moduleIcons` and `profileIcons`. Colours are explicit
//! values: Blitz does not re-resolve `currentColor` inside an SVG.

use dioxus::prelude::*;

/// Each module's colour — the macros' for its effect; a part's own edits
/// stay amber.
pub fn module_colour(kind: &str) -> &'static str {
    match kind {
        "Core" => "#D6B36A",
        "Amp" => "#f97316",
        "Drive" => "#ef4444",
        "Time" => "#6366F1",
        "Delay" => "#3B82F6",
        "Reverb" => "#8B5CF6",
        _ => "#f59e0b",
    }
}

/// A block type's colour (the browser's block kinds).
pub fn block_colour(kind: &str) -> &'static str {
    match kind {
        "compressor" => "#E5E7EB",
        "gate" => "#94A3B8",
        "eq" => "#22C55E",
        "delay" => "#3B82F6",
        "reverb" => "#8B5CF6",
        "chorus" | "modulation" => "#7DD3FC",
        _ => "#a1a1aa",
    }
}

/// The override icon: a square laid over another — something placed over
/// what the preset has.
#[component]
pub fn OverrideIcon(colour: String, size: u32) -> Element {
    rsx! {
        svg { width: "{size}", height: "{size}", view_box: "0 0 12 12", style: "flex-shrink: 0; display: block;",
            rect { x: "1", y: "1", width: "7", height: "7", rx: "1.6", fill: "none", stroke: "{colour}", stroke_width: "1.3", opacity: "0.45" }
            rect { x: "4", y: "4", width: "7", height: "7", rx: "1.6", fill: "{colour}" }
        }
    }
}

/// The inherit icon: an arrow coming down from above — what a preset higher
/// up chose, not set here.
#[component]
pub fn InheritIcon(colour: String, size: u32) -> Element {
    rsx! {
        svg { width: "{size}", height: "{size}", view_box: "0 0 12 12", style: "flex-shrink: 0; display: block;",
            path { d: "M3 1.5v4.2a2 2 0 0 0 2 2h5", fill: "none", stroke: "{colour}", stroke_width: "1.4", stroke_linecap: "round" }
            path { d: "M7.8 5.2 10.3 7.7 7.8 10.2", fill: "none", stroke: "{colour}", stroke_width: "1.4", stroke_linecap: "round", stroke_linejoin: "round" }
        }
    }
}

/// A module's glyph (grid-ui's `module_paths`): one stroke on a 24 grid.
#[component]
pub fn ModuleIcon(kind: String, size: u32, colour: String) -> Element {
    let paths: &[&str] = match kind.as_str() {
        "Core" => &["M3 7h18v12H3z", "M3 11h18", "M7 15h.01", "M11 15h.01"],
        "Amp" => &["M11 5 6 9H3v6h3l5 4z", "M15.5 8.5a5 5 0 0 1 0 7"],
        "Drive" => &["M13 2 4 14h7l-1 8 9-12h-7z"],
        "Time" => &["M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z", "M12 7v5l3 2"],
        "Delay" => &["M21 12a9 9 0 1 1-3-6.7", "M21 4v5h-5"],
        "Reverb" => &["M12 3v4", "M12 17v4", "M3 12h4", "M17 12h4", "M6 6l2.5 2.5", "M15.5 15.5 18 18", "M18 6l-2.5 2.5", "M8.5 15.5 6 18"],
        "edits" => &["M6 20V10", "M12 20V4", "M18 20v-6"],
        _ => &["M12 10a2 2 0 1 0 0 4 2 2 0 0 0 0-4z"],
    };
    rsx! {
        svg { width: "{size}", height: "{size}", view_box: "0 0 24 24", style: "flex-shrink: 0; display: block;",
            for (i, d) in paths.iter().enumerate() {
                path { key: "{i}", d: "{d}", fill: "none", stroke: "{colour}", stroke_width: "2.2", stroke_linecap: "round", stroke_linejoin: "round" }
            }
        }
    }
}

/// A profile's icon — a sound-world you can tell apart at a glance; a
/// profile with none of its own shows the stack of layers every one is.
#[component]
pub fn ProfileIcon(name: String, colour: String, size: u32) -> Element {
    let c = colour.clone();
    let line = move |d: &'static str| rsx! { path { d: "{d}", fill: "none", stroke: "{c}", stroke_width: "1.5", stroke_linecap: "round", stroke_linejoin: "round" } };
    rsx! {
        svg { width: "{size}", height: "{size}", view_box: "0 0 16 16", style: "flex-shrink: 0; display: block;",
            match name.as_str() {
                "Worship" => line("M8 14.5c-2.8 0-4.5-1.9-4.5-4.3 0-2.6 2.2-3.9 2.6-6.7 1.5 1 2.2 2.4 2.2 3.6.7-.5 1.1-1.3 1.2-2.2 1.6 1.3 3 3.1 3 5.3 0 2.4-1.7 4.3-4.5 4.3Z"),
                "Blues" => line("M12.8 10.4A5.6 5.6 0 0 1 5.6 3.2 5.6 5.6 0 1 0 12.8 10.4Z"),
                "Metal" => line("M9.2 1.5 3.5 9h4.1l-.9 5.5L12.5 7H8.4l.8-5.5Z"),
                "Funk" => line("M8 1.5c.5 3.4 1.9 4.9 5.5 5.5-3.6.6-5 2.1-5.5 6.5-.5-4.4-1.9-5.9-5.5-6.5C6.1 6.4 7.5 4.9 8 1.5Z"),
                "Rock" => rsx! {
                    rect { x: "2.5", y: "2", width: "11", height: "12", rx: "1.5", fill: "none", stroke: "{colour}", stroke_width: "1.5" }
                    circle { cx: "8", cy: "9", r: "3", fill: "none", stroke: "{colour}", stroke_width: "1.5" }
                },
                "Jazz" => rsx! {
                    path { d: "M5.5 11.5v-8l7-1.5v8", fill: "none", stroke: "{colour}", stroke_width: "1.5", stroke_linecap: "round", stroke_linejoin: "round" }
                    ellipse { cx: "4", cy: "11.8", rx: "1.9", ry: "1.5", fill: "{colour}" }
                    ellipse { cx: "11", cy: "10.3", rx: "1.9", ry: "1.5", fill: "{colour}" }
                },
                "Indie" => rsx! {
                    circle { cx: "8", cy: "8", r: "6.2", fill: "none", stroke: "{colour}", stroke_width: "1.5" }
                    circle { cx: "8", cy: "8", r: "2", fill: "none", stroke: "{colour}", stroke_width: "1.5" }
                },
                _ => rsx! {
                    path { d: "M8 1.6 14.4 5 8 8.4 1.6 5Z", fill: "none", stroke: "{colour}", stroke_width: "1.5", stroke_linejoin: "round" }
                    path { d: "M1.6 8.2 8 11.6l6.4-3.4", fill: "none", stroke: "{colour}", stroke_width: "1.5", stroke_linecap: "round", stroke_linejoin: "round" }
                    path { d: "M1.6 11.2 8 14.6l6.4-3.4", fill: "none", stroke: "{colour}", stroke_width: "1.5", stroke_linecap: "round", stroke_linejoin: "round", opacity: "0.6" }
                },
            }
        }
    }
}
