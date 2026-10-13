//! The drum stack as elements, for renderers that are fast with them.
//!
//! The same picture as [`super::paint`], from the same
//! [`super::geometry::LaneView`]s — that is the point. `geometry.rs`
//! decides where everything goes; this turns those numbers into markup
//! and the painter turns them into a scene, and neither knows about the
//! other.
//!
//! Two presentations because the renderers are opposites. Blitz charges
//! style and box construction per node, so the stack's ~1900 elements
//! cost it 154 ms a frame and a painted scene costs 6.5 — which is why
//! `paint.rs` exists. A browser engine is built for exactly these
//! elements, and painting for it instead means a rasterizer, a worker
//! thread, a byte protocol and a canvas, all to avoid the thing the
//! engine does natively. So: paint on native, markup on a WebView and on
//! the web.
//!
//! What that buys, beyond speed: `rsx!` edits hot-reload into a running
//! window, devtools can inspect and tweak a lane, and CSS applies. This
//! is the surface the design work happens on.

use dioxus::prelude::*;

use super::geometry::CHROME_ROW_H;
use super::geometry::LaneView;
use super::paint::StackChrome;
use super::view::{MIC_CHIP_TOP, MIC_ITEM_H, MIC_MENU_TOP, MIC_MENU_W};
use crate::canvas;
use crate::theme;

/// The stack's picture, as SVG children.
///
/// Returns the drawing only — no `<svg>` wrapper and no gestures. The
/// view owns those, so that the element carrying the handlers is the
/// same one in both presentations.
pub fn stack_markup(
    lanes: &[LaneView],
    chrome: &StackChrome<'_>,
    mut editor: Signal<expression_editor_core::Editor>,
    vp: expression_editor_core::Viewport,
) -> Element {
    let ruler_h = chrome.ruler_h;
    let sections = chrome.sections;
    let marks = chrome.marks;
    let ticks = chrome.ticks;
    let fill_bands = chrome.fill_bands;
    let chrome_rows = chrome.chrome_rows;
    let mark_row_h = chrome.mark_row_h;
    let mic_menu = chrome.mic_menu;
    let lanes = lanes;
    rsx! {
            // The stack sits on the roll's own ink — the darkest step,
            // so the lanes read as material on a desk rather than
            // panels on a panel.
            rect {
                x: 0, y: 0,
                width: "{vp.w + canvas::GUTTER_W}",
                height: "{vp.h + ruler_h}",
                fill: theme::GUTTER_BG,
            }

            // One ruler for the whole stack — the shared axis is the
            // reason the view exists, so it is drawn once rather than
            // per lane. Bar starts get their number; beats get a short
            // tick, exactly like the roll's ruler.
            rect {
                x: 0, y: 0,
                width: "{vp.w + canvas::GUTTER_W}",
                height: "{ruler_h}",
                fill: theme::SURFACE_BAR,
            }
            g {
                transform: "translate({canvas::GUTTER_W}, 0)",
                // The section strip: the top half of the ruler is the
                // song's own map — INTRO, VS 1, CH 1 — in the colours
                // the arrange view already taught the band.
                for (x0, x1, label, color, row) in sections.iter() {
                    rect {
                        key: "s{x0:.1}",
                        x: "{x0:.1}",
                        y: "{*row as f64 * CHROME_ROW_H:.1}",
                        width: "{(x1 - x0).max(0.0):.1}",
                        height: "{CHROME_ROW_H:.1}",
                        fill: "{color}",
                        opacity: "0.85",
                    }
                    line {
                        x1: "{x0:.1}", x2: "{x0:.1}",
                        y1: "{*row as f64 * CHROME_ROW_H:.1}",
                        y2: "{(*row + 1) as f64 * CHROME_ROW_H:.1}",
                        stroke: theme::GUTTER_BG,
                        stroke_width: 1,
                    }
                    if !label.is_empty() {
                        text {
                            x: "{x0 + 4.0:.1}",
                            y: "{*row as f64 * CHROME_ROW_H + 11.0:.1}",
                            font_size: "8",
                            fill: "#0b0b10",
                            "{label}"
                        }
                    }
                }
                // r[impl drums.chrome.markers]
                //
                // A marker is a point, so it gets a tick and a label
                // rather than a band: the label sits to the right of
                // its line, which is where the thing it names starts.
                for (x, label, color, row) in marks.iter() {
                    // One shelf per ruler lane. The tick spans only its
                    // own row, so which lane a marker is filed under is
                    // read off its height — that is the whole point of
                    // lanes, and it is information these projects get
                    // wrong in a way worth being able to see.
                    line {
                        key: "mk{x:.1}",
                        x1: "{x:.1}", x2: "{x:.1}",
                        y1: "{*row as f64 * mark_row_h:.1}",
                        y2: "{(*row + 1) as f64 * mark_row_h:.1}",
                        stroke: "{color}",
                        stroke_width: 2,
                    }
                    text {
                        x: "{x + 3.0:.1}",
                        y: "{(*row + 1) as f64 * mark_row_h - 2.0:.1}",
                        font_size: 9,
                        fill: "{color}",
                        "{label}"
                    }
                }
                // The lane names, once, down the left edge — one per
                // shelf, so a shelf says which ruler lane it is.
                for (i, (_, _, name)) in chrome_rows.iter().enumerate() {
                    text {
                        key: "shelf{i}",
                        x: 2,
                        y: "{(i + 1) as f64 * CHROME_ROW_H - 2.0:.1}",
                        font_size: 7,
                        fill: theme::TEXT_DIM,
                        opacity: "0.7",
                        "{name}"
                    }
                }
                for t in ticks.iter() {
                    line {
                        key: "rt{t.x:.1}",
                        x1: "{t.x:.1}", x2: "{t.x:.1}",
                        y1: if t.bar { "{ruler_h - 10.0:.1}" } else { "{ruler_h - 5.0:.1}" },
                        y2: "{ruler_h}",
                        stroke: if t.bar { theme::TEXT_DIM } else { theme::TEXT_FAINT },
                        stroke_width: 1,
                    }
                    if let Some(label) = t.label.as_ref() {
                        text {
                            x: "{t.x + 3.0:.1}", y: "{ruler_h - 4.0:.1}",
                            font_size: "8",
                            fill: theme::TEXT_DIM,
                            "{label}"
                        }
                    }
                }
            }

            g {
                transform: "translate(0, {ruler_h})",
                for lane in lanes.iter() {
                    g {
                        key: "lane{lane.lane}",
                        // A lane's own background, so the active one
                        // reads as the foreground even when a neighbour
                        // is busier.
                        rect {
                            x: 0, y: "{lane.y:.1}",
                            width: "{vp.w + canvas::GUTTER_W}",
                            height: "{lane.h:.1}",
                            fill: if lane.active { theme::ROW_WHITE } else { theme::ROW_BLACK },
                        }
                        line {
                            x1: 0, x2: "{vp.w + canvas::GUTTER_W}",
                            y1: "{lane.y:.1}", y2: "{lane.y:.1}",
                            stroke: theme::OCTAVE_LINE, stroke_width: 1,
                        }
                        // The armed-lane rail: the one bright fixture,
                        // down the gutter edge of the lane you are
                        // editing — a console's channel-select, not a
                        // second highlight fighting the hits.
                        if lane.active {
                            rect {
                                x: 0, y: "{lane.y:.1}",
                                width: 3,
                                height: "{lane.h:.1}",
                                fill: lane.role_color.unwrap_or(theme::ACCENT),
                            }
                        }
                        g {
                            transform: "translate({canvas::GUTTER_W}, 0)",
                            // The beat grid, under the audio: reading a
                            // hit's distance from the beat is the whole
                            // job, so the beat must be drawn where the
                            // hits are, not only in the ruler.
                            for t in ticks.iter() {
                                line {
                                    key: "g{t.x:.1}",
                                    x1: "{t.x:.1}", x2: "{t.x:.1}",
                                    y1: "{lane.y:.1}", y2: "{lane.y + lane.h:.1}",
                                    stroke: if t.bar { theme::GRID_BEAT } else { theme::GRID_SUB },
                                    stroke_width: 1,
                                }
                            }
                            // Section boundaries carry down through the
                            // material, faintly, in the section's own
                            // colour — the ruler says where you are,
                            // these say it where you are looking.
                            for (x0, _, _, color, _) in sections.iter() {
                                line {
                                    key: "sb{x0:.1}",
                                    x1: "{x0:.1}", x2: "{x0:.1}",
                                    y1: "{lane.y:.1}", y2: "{lane.y + lane.h:.1}",
                                    stroke: "{color}",
                                    stroke_width: 1,
                                    opacity: "0.3",
                                }
                            }
                            // r[impl drums.fills.draw]
                            //
                            // Behind everything, and a wash rather than
                            // an outline: a fill is a *region* of the
                            // take, and the hits inside it still have to
                            // read as hits. An edge strong enough to
                            // notice would compete with the markers it
                            // sits under.
                            for (x0, x1) in fill_bands.iter() {
                                rect {
                                    key: "fb{x0:.1}",
                                    x: "{x0:.1}",
                                    y: "{lane.y:.1}",
                                    width: "{(x1 - x0).max(0.0):.1}",
                                    height: "{lane.h:.1}",
                                    fill: theme::TEXT_DIM,
                                    opacity: "0.10",
                                }
                            }
                            // Markers carry down the same way — the
                            // ruler says where you are, these say it
                            // where you are looking.
                            // r[impl drums.chrome.markers]
                            for (x, _, color, _) in marks.iter() {
                                line {
                                    key: "mb{x:.1}",
                                    x1: "{x:.1}", x2: "{x:.1}",
                                    y1: "{lane.y:.1}", y2: "{lane.y + lane.h:.1}",
                                    stroke: "{color}",
                                    stroke_width: 1,
                                    opacity: "0.3",
                                }
                            }
                            // A role lane's audio, behind everything
                            // else — the hits draw over it, in the
                            // same hue: the lane is one thing, and its
                            // colour says which drum from across the
                            // room. Lanes without a role keep the
                            // neutral peaks blue.
                            // r[impl drums.lanes.summed]
                            if let Some(w) = lane.waveform.as_ref() {
                                polygon {
                                    points: "{super::geometry::points_attr(w)}",
                                    fill: lane.role_color.unwrap_or(theme::PEAKS),
                                    opacity: if lane.active { "0.5" } else { "0.32" },
                                }
                            }
                            // r[impl drums.lanes.trigger-overlay]
                            //
                            // Drawn over the mics, not beside them, and
                            // outlined rather than filled: a trigger is
                            // near-silent between hits, so a filled one
                            // would read as a hole punched in the mics'
                            // waveform instead of a second view of it.
                            for (i, o) in lane.overlays.iter().enumerate() {
                                polygon {
                                    key: "ov{i}",
                                    points: "{super::geometry::points_attr(o)}",
                                    fill: "none",
                                    stroke: lane.role_color.unwrap_or(theme::PEAKS),
                                    stroke_width: "1",
                                    opacity: if lane.active { "0.75" } else { "0.5" },
                                }
                            }
                            // r[impl drums.lanes.toms-split]
                            for s in lane.sub_lanes.iter() {
                                for (i, o) in s.overlays.iter().enumerate() {
                                    polygon {
                                        key: "so{i}",
                                        points: "{super::geometry::points_attr(o)}",
                                        fill: "none",
                                        stroke: lane.role_color.unwrap_or(theme::PEAKS),
                                        stroke_width: "1",
                                        opacity: if s.faded { "0.20" } else { "0.6" },
                                    }
                                }
                                if let Some(p) = s.points.as_ref() {
                                    polygon {
                                        points: "{super::geometry::points_attr(p)}",
                                        fill: lane.role_color.unwrap_or(theme::PEAKS),
                                        opacity: if s.faded { "0.10" } else { "0.32" },
                                    }
                                }
                            }
                            for d in lane.dividers.iter() {
                                line {
                                    key: "dv{d:.1}",
                                    x1: 0, x2: "{vp.w}",
                                    y1: "{d:.1}", y2: "{d:.1}",
                                    stroke: theme::GRID_SUB, stroke_width: 1,
                                }
                            }
                            // The lane's hits over the waveform.
                            // r[impl drums.lanes.hits]
                            for n in lane.notes.iter() {
                                // A grace note draws at two-thirds
                                // height, the way engraving shrinks one
                                // — so a flam reads as one gesture with
                                // a light hit rather than two equal
                                // ones.
                                {
                                    let gh = if n.grace { n.h * 0.62 } else { n.h };
                                    let gy = n.y + (n.h - gh) / 2.0;
                                    rsx! {
                                        // One key for the whole hit,
                                        // keyed by its onset — which is
                                        // what a hit IS. Without it a
                                        // zoom, which changes how many
                                        // hits are in view, leaves dioxus
                                        // diffing them positionally and
                                        // pairing a hit with whichever
                                        // one now sits at its index.
                                        g {
                                        key: "n{n.at_secs:.4}",
                                        if n.hit_line {
                                            // r[impl drums.lanes.hits]
                                            line {
                                                x1: "{n.x:.1}", y1: "{gy:.1}",
                                                x2: "{n.x:.1}", y2: "{gy + gh:.1}",
                                                stroke: "{n.fill}",
                                                stroke_width: "{lane.hit_width:.2}",
                                                opacity: "0.9",
                                            }
                                            // r[impl drums.lanes.hit-density]
                                            if lane.hit_flag {
                                                polygon {
                                                    points: "{n.x:.1},{gy:.1} {n.x + n.w.min(9.0):.1},{gy + 5.0:.1} {n.x:.1},{gy + 10.0:.1}",
                                                    fill: "{n.fill}",
                                                }
                                            }
                                        } else if n.triangle {
                                            polygon {
                                                points: "{n.x:.1},{gy:.1} {n.x + n.w:.1},{gy + gh / 2.0:.1} {n.x:.1},{gy + gh:.1}",
                                                fill: "{n.fill}",
                                                opacity: if n.grace { "0.75" } else { "1" },
                                            }
                                        } else {
                                            rect {
                                                x: "{n.x:.1}", y: "{gy:.1}",
                                                width: "{n.w:.1}", height: "{gh:.1}",
                                                rx: 1,
                                                fill: "{n.fill}",
                                                opacity: if n.grace { "0.75" } else { "1" },
                                            }
                                        }
                                        // The engraver's slash through
                                        // the grace note's stem.
                                        if n.grace {
                                            line {
                                                x1: "{n.x - 1.0:.1}", y1: "{gy + gh + 2.0:.1}",
                                                x2: "{n.x + 5.0:.1}", y2: "{gy - 2.0:.1}",
                                                stroke: "{n.fill}", stroke_width: 1,
                                            }
                                        }
                                        // The principal is badged, not
                                        // shrunk: it is the note you
                                        // played.
                                        if n.flam {
                                            text {
                                                x: "{n.x + n.w + 2.0:.1}",
                                                y: "{n.y + n.h * 0.5:.1}",
                                                font_size: "7",
                                                fill: theme::TEXT_DIM,
                                                "fl"
                                            }
                                        }
                                        }
                                    }
                                }
                            }
                        }
                        // The name last, so it sits over the material
                        // rather than under it. A role lane's is an
                        // eyebrow — small caps, spaced, quiet — because
                        // the label is furniture and the audio is the
                        // content.
                        if lane.is_role {
                            text {
                                x: 7, y: "{lane.y + 12.0:.1}",
                                font_size: "8",
                                letter_spacing: "2",
                                fill: lane
                                    .role_color
                                    .unwrap_or(if lane.active { theme::TEXT_BRIGHT } else { theme::TEXT_DIM }),
                                opacity: if lane.active { "1" } else { "0.75" },
                                {lane.name.to_uppercase()}
                            }
                            // The lane's mic, and the way to change it:
                            // a chip under the eyebrow, opening a list
                            // of the lane's members. This replaces the
                            // old chip row across the top — the choice
                            // belongs to the lane it changes.
                            if lane.members.len() > 1 {
                                rect {
                                    x: 4, y: "{lane.y + MIC_CHIP_TOP:.1}",
                                    width: "{canvas::GUTTER_W - 8.0}",
                                    height: "{MIC_ITEM_H - 2.0}",
                                    rx: 2,
                                    fill: theme::SURFACE_BAR,
                                    stroke: theme::PANEL_BORDER,
                                    stroke_width: 1,
                                }
                                text {
                                    x: 8, y: "{lane.y + MIC_CHIP_TOP + 10.0:.1}",
                                    font_size: "8",
                                    fill: theme::TEXT,
                                    {
                                        let name = lane
                                            .members
                                            .iter()
                                            .find(|(_, _, a)| *a)
                                            .map(|(_, n, _)| n.as_str())
                                            .unwrap_or_else(|| {
                                                lane.members
                                                    .first()
                                                    .map(|(_, n, _)| n.as_str())
                                                    .unwrap_or("")
                                            });
                                        let fit: String = name.chars().take(6).collect();
                                        format!("{fit} ▾")
                                    }
                                }
                            }
                        } else {
                            text {
                                x: 4, y: "{lane.y + 11.0:.1}",
                                font_size: "9",
                                fill: if lane.active { theme::TEXT } else { theme::TEXT_DIM },
                                "{lane.name}"
                            }
                        }
                        // Each tom's name at its own sub-row, indented
                        // under the lane's role label.
                        // r[impl drums.lanes.toms-split]
                        for s in lane.sub_lanes.iter() {
                            text {
                                key: "sl{s.label}",
                                // Clear of the role eyebrow, which owns
                                // the first ~60px of the lane's top row.
                                x: 64, y: "{s.label_y:.1}",
                                font_size: "8",
                                fill: theme::TEXT_DIM,
                                opacity: if s.faded { "0.5" } else { "1" },
                                "{s.label}"
                            }
                        }
                        // Two-handed pieces carry a hand affordance in
                        // the header. Only they do: a hi-hat showing a
                        // split control that does nothing is worse than
                        // no control.
                        if let Some(row) = lane.two_handed_row {
                            text {
                                x: "{canvas::GUTTER_W - 12.0:.1}",
                                y: "{lane.y + 11.0:.1}",
                                font_size: "8",
                                fill: theme::TEXT_DIM,
                                onclick: move |_| {
                                    editor.write().toggle_piece_split(row);
                                },
                                if lane.split { "L|R" } else { "L+R" }
                            }
                        }
                    }
                }
            }

            // The open mic menu, over everything: the lane's members,
            // the active one marked. Geometry mirrored exactly by the
            // press handler above.
            if let Some(open) = mic_menu {
                if let Some(lane) = lanes.iter().find(|l| l.lane == open) {
                    g {
                        transform: "translate(0, {ruler_h})",
                        rect {
                            x: 4, y: "{lane.y + MIC_MENU_TOP - 2.0:.1}",
                            width: "{MIC_MENU_W}",
                            height: "{(lane.members.len() + 1) as f64 * MIC_ITEM_H + 4.0:.1}",
                            rx: 3,
                            fill: theme::PANEL,
                            stroke: theme::BORDER_STRONG,
                            stroke_width: 1,
                        }
                        for (i, (_, name, active)) in lane.members.iter().enumerate() {
                            // The menu row: highlight and label together,
                            // keyed by the mic it names.
                            g {
                            key: "mic{name}",
                            if *active {
                                rect {
                                    x: 5, y: "{lane.y + MIC_MENU_TOP + i as f64 * MIC_ITEM_H:.1}",
                                    width: "{MIC_MENU_W - 2.0}",
                                    height: "{MIC_ITEM_H}",
                                    fill: theme::CONTROL_SELECTED,
                                }
                            }
                            text {
                                x: 12, y: "{lane.y + MIC_MENU_TOP + i as f64 * MIC_ITEM_H + 11.0:.1}",
                                font_size: "9",
                                fill: if *active { theme::TEXT_BRIGHT } else { theme::TEXT },
                                "{name}"
                            }
                            }
                        }
                        // The footer: draw only this mic's waveform,
                        // instead of the members' sum. A view flag —
                        // detection and edits still take the lane whole.
                        {
                            let sy = lane.y + MIC_MENU_TOP + lane.members.len() as f64 * MIC_ITEM_H;
                            rsx! {
                                line {
                                    x1: 5, x2: "{2.0 + MIC_MENU_W}",
                                    y1: "{sy:.1}", y2: "{sy:.1}",
                                    stroke: theme::PANEL_BORDER, stroke_width: 1,
                                }
                                text {
                                    x: 12, y: "{sy + 11.0:.1}",
                                    font_size: "9",
                                    fill: if lane.solo_mic { theme::TEXT_BRIGHT } else { theme::TEXT_DIM },
                                    if lane.solo_mic { "✓ solo this mic" } else { "solo this mic" }
                                }
                            }
                        }
                    }
                }
            }

    }
}
