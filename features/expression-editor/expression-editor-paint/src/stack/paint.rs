//! The drum stack, painted into a scene instead of emitted as SVG.
//!
//! Same picture, no elements. The stack drew a node per tick, per hit,
//! per label, per lane band — around nineteen hundred of them for a
//! four-bar window on a real kit — and Blitz charged style and box
//! construction for every one of them on every camera move. Measured
//! with blitz-dom's own phase timer, a re-rendering frame of the
//! workstation spent 18 ms in `style` and 23 ms in `construct` against
//! **2 ms** in Taffy layout, and a frame that damaged nothing at all
//! cost 2.6 ms. The elements were the cost, never the drawing.
//!
//! So this is the same port [`crate::paint`] made for the roll and
//! the strip painter made for the strip, applied to the densest
//! surface of the three. The geometry is untouched: `super::geometry`
//! still decides where everything goes, `super::view` still owns the
//! gestures, and this only turns the resulting numbers into paint.
//!
//! **Element space**: `(0, 0)` is the top-left of the stack's own box,
//! so the gutter occupies the first [`canvas::GUTTER_W`] pixels and the
//! ruler the first `ruler_h`. Lane content is translated past both,
//! exactly as the `<g transform=…>` groups did.
//!
//! What is deliberately NOT here: the playhead, the selected-hit
//! bracket, the slip ghost and the zoom marquee. Those move on their own
//! clocks — a transport tick arrives many times a second — and they stay
//! as a handful of overlay elements so that moving one of them does not
//! rebuild this scene. See `super::view`.

use anyrender::{PaintScene, Scene};
use kurbo::{Affine, Rect, RoundedRect};
use peniko::{Color, Fill};

use super::geometry::LaneView;
use crate::canvas::{self, Tick};
use crate::paint::{Batch, Look, color, stroke_of, with_alpha};
use crate::text::{Align, Labeller};

/// The ruler's furniture and the lane-independent layers, as
/// `super::view` already resolved them.
///
/// Passed in rather than recomputed: the view needs these for hit
/// testing anyway, and computing them twice would make the picture and
/// the gestures two opinions about where things are.
pub struct StackChrome<'a> {
    /// One shelf per ruler lane: `(lane index, is_region, name)`.
    pub chrome_rows: &'a [(Option<u32>, bool, String)],
    /// `(x0, x1, label, colour, shelf row)`.
    pub sections: &'a [(f64, f64, String, String, usize)],
    /// `(x, label, colour, shelf row)`.
    pub marks: &'a [(f64, String, String, usize)],
    pub ticks: &'a [Tick],
    /// Fill regions, in lane-local x.
    pub fill_bands: &'a [(f64, f64)],
    /// The lane whose mic menu is open, if any.
    pub mic_menu: Option<usize>,
    pub ruler_h: f64,
    pub chrome_row_h: f64,
    pub mark_row_h: f64,
    pub mic_chip_top: f64,
    pub mic_menu_top: f64,
    pub mic_item_h: f64,
    pub mic_menu_w: f64,
    /// How far past a lane's edges a hit is still drawn.
    pub hit_margin: f64,
    /// Whether the host draws the lanes' headers (`Stack::host_lanes`).
    pub host_lanes: bool,
}

/// Draw the stack into a scene sized `w` x `h` in CSS pixels.
///
/// `w` and `h` are the element's box, handed down by the widget —
/// nothing here derives a size from the content.
pub fn stack_scene(
    lanes: &[LaneView],
    chrome: &StackChrome<'_>,
    w: f64,
    h: f64,
    labels: &mut Labeller,
    look: &Look,
) -> Scene {
    let mut scene = Scene::new();
    let ruler_h = chrome.ruler_h;
    let gutter = canvas::GUTTER_W;
    // Past the gutter, and past the gutter and the ruler — the two
    // groups the SVG wrapped everything in.
    let across = Affine::translate((gutter, 0.0));
    let down = Affine::translate((0.0, ruler_h));
    let lane_space = Affine::translate((gutter, ruler_h));

    // The stack sits on the roll's own ink — the darkest step, so the
    // lanes read as material on a desk rather than panels on a panel.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        look.gutter_bg,
        None,
        &Rect::new(0.0, 0.0, w, h),
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        look.surface_bar,
        None,
        &Rect::new(0.0, 0.0, w, ruler_h),
    );

    ruler(&mut scene, look, chrome, across, labels);
    // Lane by lane, ground then content — the SVG's own order, restored.
    //
    // It was briefly three passes (every ground, then one shared
    // timebase, then every lane's material) to stop the beat grid
    // costing `lanes × ticks` DOM nodes. That reason is gone: this is
    // paint, the grid costs a few hundred path segments however it is
    // ordered, and `Batch` collapses them by colour anyway. What the
    // three passes DID cost was correctness — a note taller than its
    // lane used to be painted over by the next lane's opaque ground, and
    // hoisting the grounds out took that clipping away, bleeding a note
    // a full lane down. Clipping it back with a layer is not an option
    // either: `push_clip_layer` in a replayed widget scene discarded
    // everything drawn before it (the ruler's section band vanished).
    // Paint order is the clip, so paint in order.
    for lane in lanes {
        lane_ground(&mut scene, look, lane, w, down, chrome.host_lanes);
        timebase(&mut scene, look, lane, chrome, lane_space);
        lane_material(
            &mut scene, look, lane, chrome, w, gutter, down, lane_space, labels,
        );
    }
    mic_menu(&mut scene, look, lanes, chrome, down, labels);

    scene
}

/// A lane polygon as a closed path.
fn polygon(points: &[(f64, f64)]) -> kurbo::BezPath {
    let mut path = kurbo::BezPath::new();
    for (i, &(x, y)) in points.iter().enumerate() {
        if i == 0 {
            path.move_to((x, y));
        } else {
            path.line_to((x, y));
        }
    }
    if !path.is_empty() {
        path.close_path();
    }
    path
}

/// The section strip, the marker shelves, the shelf names and the bar
/// ticks — the top of the stack, in gutter-relative space.
fn ruler(
    scene: &mut Scene,
    look: &Look,
    chrome: &StackChrome<'_>,
    at: Affine,
    labels: &mut Labeller,
) {
    let row_h = chrome.chrome_row_h;

    // The section strip: the song's own map — INTRO, VS 1, CH 1 — in the
    // colours the arrange view already taught the band.
    for (x0, x1, label, colour, row) in chrome.sections {
        let top = *row as f64 * row_h;
        scene.fill(
            Fill::NonZero,
            at,
            with_alpha(color(colour), 0.85),
            None,
            &Rect::new(*x0, top, x0 + (x1 - x0).max(0.0), top + row_h),
        );
        scene.stroke(
            &stroke_of(1.0),
            at,
            look.gutter_bg,
            None,
            &kurbo::Line::new((*x0, top), (*x0, top + row_h)),
        );
        if !label.is_empty() {
            text(
                scene,
                labels,
                label,
                x0 + 4.0,
                top + 11.0,
                8.0,
                color("#0b0b10"),
                at,
            );
        }
    }

    // r[impl drums.chrome.markers]
    //
    // A marker is a point, so it gets a tick and a label rather than a
    // band: the label sits to the right of its line, which is where the
    // thing it names starts. One shelf per ruler lane, so which lane a
    // marker is filed under is read off its height.
    for (x, label, colour, row) in chrome.marks {
        let c = color(colour);
        let top = *row as f64 * chrome.mark_row_h;
        let bottom = (*row + 1) as f64 * chrome.mark_row_h;
        scene.stroke(
            &stroke_of(2.0),
            at,
            c,
            None,
            &kurbo::Line::new((*x, top), (*x, bottom)),
        );
        text(scene, labels, label, x + 3.0, bottom - 2.0, 9.0, c, at);
    }

    // The lane names, once, down the left edge — one per shelf, so a
    // shelf says which ruler lane it is.
    for (i, (_, _, name)) in chrome.chrome_rows.iter().enumerate() {
        text(
            scene,
            labels,
            name,
            2.0,
            (i + 1) as f64 * row_h - 2.0,
            7.0,
            with_alpha(look.text_dim, 0.7),
            at,
        );
    }

    // Bar starts get a full-height tick and a number; beats get a short
    // one. Gathered by paint: there are only two colours between them.
    let mut ticks = Batch::default();
    for t in chrome.ticks {
        let ink = if t.bar {
            look.text_dim
        } else {
            look.text_faint
        };
        let top = chrome.ruler_h - if t.bar { 10.0 } else { 5.0 };
        ticks.add(ink, &kurbo::Line::new((t.x, top), (t.x, chrome.ruler_h)));
    }
    ticks.stroke(scene, at, 1.0);
    for t in chrome.ticks {
        if let Some(label) = t.label.as_ref() {
            text(
                scene,
                labels,
                label,
                t.x + 3.0,
                chrome.ruler_h - 4.0,
                8.0,
                look.text_dim,
                at,
            );
        }
    }
}

/// A lane's ground: its band, the rule along its top, and the armed
/// rail down its gutter edge.
fn lane_ground(scene: &mut Scene, look: &Look, lane: &LaneView, w: f64, at: Affine, hosted: bool) {
    // A lane's own background, so the active one reads as the foreground
    // even when a neighbour is busier. Opaque, which is also what clips
    // the previous lane's overflow — see `stack_scene`.
    let ink = if lane.active { look.row_a } else { look.row_b };
    scene.fill(
        Fill::NonZero,
        at,
        ink,
        None,
        &Rect::new(0.0, lane.y, w, lane.y + lane.h),
    );
    scene.stroke(
        &stroke_of(1.0),
        at,
        look.octave_line,
        None,
        &kurbo::Line::new((0.0, lane.y), (w, lane.y)),
    );
    // The armed-lane rail: the one bright fixture, down the gutter edge
    // of the lane you are editing — a console's channel-select, not a
    // second highlight fighting the hits.
    if lane.active && !hosted {
        let accent = lane.role_color.map_or(look.accent, color);
        scene.fill(
            Fill::NonZero,
            at,
            accent,
            None,
            &Rect::new(0.0, lane.y, 3.0, lane.y + lane.h),
        );
    }
}

/// The beat grid, section boundaries, fill washes and markers across
/// this lane — under its audio, because reading a hit's distance from
/// the beat is the whole job.
fn timebase(scene: &mut Scene, look: &Look, lane: &LaneView, chrome: &StackChrome<'_>, at: Affine) {
    let (top, bottom) = (lane.y, lane.y + lane.h);

    let mut grid = Batch::default();
    for t in chrome.ticks {
        let ink = if t.bar { look.grid_beat } else { look.grid_sub };
        grid.add(ink, &kurbo::Line::new((t.x, top), (t.x, bottom)));
    }
    // Section boundaries and markers carry down through the material,
    // faintly, in their own colours — the ruler says where you are,
    // these say it where you are looking.
    for (x0, _, _, colour, _) in chrome.sections {
        grid.add(
            with_alpha(color(colour), 0.3),
            &kurbo::Line::new((*x0, top), (*x0, bottom)),
        );
    }
    // r[impl drums.chrome.markers]
    for (x, _, colour, _) in chrome.marks {
        grid.add(
            with_alpha(color(colour), 0.3),
            &kurbo::Line::new((*x, top), (*x, bottom)),
        );
    }
    grid.stroke(scene, at, 1.0);

    // r[impl drums.fills.draw]
    //
    // A wash rather than an outline: a fill is a *region* of the take,
    // and the hits inside it still have to read as hits.
    let wash = with_alpha(look.text_dim, 0.10);
    let mut fills = Batch::default();
    for (x0, x1) in chrome.fill_bands {
        fills.add(wash, &Rect::new(*x0, top, x0 + (x1 - x0).max(0.0), bottom));
    }
    fills.fill(scene, at);
}

/// Pass three: each lane's own material, then its labels.
#[allow(clippy::too_many_arguments)]
fn lane_material(
    scene: &mut Scene,
    look: &Look,
    lane: &LaneView,
    chrome: &StackChrome<'_>,
    w: f64,
    gutter: f64,
    down: Affine,
    at: Affine,
    labels: &mut Labeller,
) {
    let lane_w = w - gutter;
    {
        let hue = lane.role_color.map_or(look.peaks, color);

        // r[impl drums.lanes.summed]
        //
        // A role lane's audio, behind everything else, in the same hue:
        // the lane is one thing, and its colour says which drum from
        // across the room.
        if let Some(points) = lane.waveform.as_ref() {
            let alpha = if lane.active { 0.5 } else { 0.32 };
            scene.fill(
                Fill::NonZero,
                at,
                with_alpha(hue, alpha),
                None,
                &polygon(points),
            );
        }
        // r[impl drums.lanes.trigger-overlay]
        //
        // Outlined rather than filled: a trigger is near-silent between
        // hits, so a filled one would read as a hole punched in the
        // mics' waveform instead of a second view of it.
        let alpha = if lane.active { 0.75 } else { 0.5 };
        for o in &lane.overlays {
            scene.stroke(
                &stroke_of(1.0),
                at,
                with_alpha(hue, alpha),
                None,
                &polygon(o),
            );
        }
        // r[impl drums.lanes.toms-split]
        for s in &lane.sub_lanes {
            let stroke_alpha = if s.faded { 0.20 } else { 0.6 };
            for o in &s.overlays {
                scene.stroke(
                    &stroke_of(1.0),
                    at,
                    with_alpha(hue, stroke_alpha),
                    None,
                    &polygon(o),
                );
            }
            if let Some(p) = s.points.as_ref() {
                let fill_alpha = if s.faded { 0.10 } else { 0.32 };
                scene.fill(
                    Fill::NonZero,
                    at,
                    with_alpha(hue, fill_alpha),
                    None,
                    &polygon(p),
                );
            }
        }
        let mut dividers = Batch::default();
        for d in &lane.dividers {
            dividers.add(look.grid_sub, &kurbo::Line::new((0.0, *d), (lane_w, *d)));
        }
        dividers.stroke(scene, at, 1.0);

        hits(scene, look, lane, chrome, lane_w, at, labels);
        lane_labels(scene, look, lane, chrome, down, labels);
    }
}

/// The lane's hits over the waveform, drawn only where they can be seen.
///
/// A take carries every hit in the song and the lane is a window onto a
/// few bars of it; the rest are off both edges.
/// r[impl drums.lanes.hits]
fn hits(
    scene: &mut Scene,
    look: &Look,
    lane: &LaneView,
    chrome: &StackChrome<'_>,
    lane_w: f64,
    at: Affine,
    labels: &mut Labeller,
) {
    let margin = chrome.hit_margin;
    let mut lines = Batch::default();
    let mut flags = Batch::default();
    let mut bodies = Batch::default();
    let mut faded = Batch::default();
    let mut slashes = Batch::default();
    let mut flams: Vec<(f64, f64)> = Vec::new();

    for n in lane
        .notes
        .iter()
        .filter(|n| n.x + n.w >= -margin && n.x <= lane_w + margin)
    {
        // A grace note draws at two-thirds height, the way engraving
        // shrinks one — so a flam reads as one gesture with a light hit
        // rather than two equal ones.
        let gh = if n.grace { n.h * 0.62 } else { n.h };
        let gy = n.y + (n.h - gh) / 2.0;
        let ink = color(&n.fill);
        if n.hit_line {
            lines.add(
                with_alpha(ink, 0.9),
                &kurbo::Line::new((n.x, gy), (n.x, gy + gh)),
            );
            // r[impl drums.lanes.hit-density]
            if lane.hit_flag {
                let mut flag = kurbo::BezPath::new();
                flag.move_to((n.x, gy));
                flag.line_to((n.x + n.w.min(9.0), gy + 5.0));
                flag.line_to((n.x, gy + 10.0));
                flag.close_path();
                flags.add(ink, &flag);
            }
        } else if n.triangle {
            let mut tri = kurbo::BezPath::new();
            tri.move_to((n.x, gy));
            tri.line_to((n.x + n.w, gy + gh / 2.0));
            tri.line_to((n.x, gy + gh));
            tri.close_path();
            if n.grace { &mut faded } else { &mut bodies }.add(ink, &tri);
        } else if n.x + n.w > 0.0 {
            // Trimmed at the lane's left edge: a note that began before
            // the view would otherwise run on into the gutter, under
            // the lane's name. (Nothing clips this scene — see
            // `stack_scene` — so the geometry has to stop itself.)
            let body = RoundedRect::new(n.x.max(0.0), gy, n.x + n.w, gy + gh, 1.0);
            if n.grace { &mut faded } else { &mut bodies }.add(ink, &body);
        }
        // The engraver's slash through the grace note's stem.
        if n.grace {
            slashes.add(
                ink,
                &kurbo::Line::new((n.x - 1.0, gy + gh + 2.0), (n.x + 5.0, gy - 2.0)),
            );
        }
        // The principal is badged, not shrunk: it is the note you played.
        if n.flam {
            flams.push((n.x + n.w + 2.0, n.y + n.h * 0.5));
        }
    }

    lines.stroke(scene, at, lane.hit_width);
    bodies.fill(scene, at);
    // Grace bodies carry the engraving's lighter weight.
    for (c, path) in faded.take() {
        scene.fill(Fill::NonZero, at, with_alpha(c, 0.75), None, &path);
    }
    flags.fill(scene, at);
    slashes.stroke(scene, at, 1.0);
    for (x, y) in flams {
        text(scene, labels, "fl", x, y, 7.0, look.text_dim, at);
    }
}

/// The lane's name and its gutter furniture, over the material rather
/// than under it.
fn lane_labels(
    scene: &mut Scene,
    look: &Look,
    lane: &LaneView,
    chrome: &StackChrome<'_>,
    at: Affine,
    labels: &mut Labeller,
) {
    if chrome.host_lanes {
        // The host's header carries the name.
    } else if lane.is_role {
        // A role lane's name is an eyebrow — small caps, spaced, quiet —
        // because the label is furniture and the audio is the content.
        let ink = lane.role_color.map_or(
            if lane.active {
                look.text_bright
            } else {
                look.text_dim
            },
            color,
        );
        let ink = if lane.active {
            ink
        } else {
            with_alpha(ink, 0.75)
        };
        text(
            scene,
            labels,
            &spaced(&lane.name.to_uppercase()),
            7.0,
            lane.y + 12.0,
            8.0,
            ink,
            at,
        );
        // The lane's mic, and the way to change it: a chip under the
        // eyebrow, opening a list of the lane's members.
        if lane.members.len() > 1 {
            let top = lane.y + chrome.mic_chip_top;
            let chip = RoundedRect::new(
                4.0,
                top,
                canvas::GUTTER_W - 4.0,
                top + chrome.mic_item_h - 2.0,
                2.0,
            );
            scene.fill(Fill::NonZero, at, look.surface_bar, None, &chip);
            scene.stroke(&stroke_of(1.0), at, look.panel_border, None, &chip);
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
            text(
                scene,
                labels,
                &format!("{fit} ▾"),
                8.0,
                top + 10.0,
                8.0,
                look.text,
                at,
            );
        }
    } else {
        let ink = if lane.active {
            look.text
        } else {
            look.text_dim
        };
        text(scene, labels, &lane.name, 4.0, lane.y + 11.0, 9.0, ink, at);
    }

    // Each tom's name at its own sub-row, indented clear of the role
    // eyebrow, which owns the first ~60px of the lane's top row.
    // r[impl drums.lanes.toms-split]
    for s in &lane.sub_lanes {
        let ink = look.text_dim;
        let ink = if s.faded { with_alpha(ink, 0.5) } else { ink };
        text(scene, labels, &s.label, 64.0, s.label_y, 8.0, ink, at);
    }

    // Two-handed pieces carry a hand affordance in the header. Only they
    // do: a hi-hat showing a split control that does nothing is worse
    // than no control.
    if lane.two_handed_row.is_some() {
        let mark = if lane.split { "L|R" } else { "L+R" };
        text(
            scene,
            labels,
            mark,
            canvas::GUTTER_W - 12.0,
            lane.y + 11.0,
            8.0,
            look.text_dim,
            at,
        );
    }
}

/// The open mic menu, over everything: the lane's members, the active
/// one marked. Geometry mirrored exactly by `super::view`'s press
/// handler — the two must agree or the menu would answer a click meant
/// for the item above it.
fn mic_menu(
    scene: &mut Scene,
    look: &Look,
    lanes: &[LaneView],
    chrome: &StackChrome<'_>,
    at: Affine,
    labels: &mut Labeller,
) {
    let Some(open) = chrome.mic_menu else { return };
    let Some(lane) = lanes.iter().find(|l| l.lane == open) else {
        return;
    };
    let top = lane.y + chrome.mic_menu_top;
    let panel = RoundedRect::new(
        4.0,
        top - 2.0,
        4.0 + chrome.mic_menu_w,
        top - 2.0 + (lane.members.len() + 1) as f64 * chrome.mic_item_h + 4.0,
        3.0,
    );
    scene.fill(Fill::NonZero, at, look.panel, None, &panel);
    scene.stroke(&stroke_of(1.0), at, look.border_strong, None, &panel);

    for (i, (_, name, active)) in lane.members.iter().enumerate() {
        let y = top + i as f64 * chrome.mic_item_h;
        if *active {
            scene.fill(
                Fill::NonZero,
                at,
                look.control_selected,
                None,
                &Rect::new(5.0, y, 3.0 + chrome.mic_menu_w, y + chrome.mic_item_h),
            );
        }
        let ink = if *active { look.text_bright } else { look.text };
        text(scene, labels, name, 12.0, y + 11.0, 9.0, ink, at);
    }

    // The footer: draw only this mic's waveform, instead of the members'
    // sum. A view flag — detection and edits still take the lane whole.
    let sy = top + lane.members.len() as f64 * chrome.mic_item_h;
    scene.stroke(
        &stroke_of(1.0),
        at,
        look.panel_border,
        None,
        &kurbo::Line::new((5.0, sy), (2.0 + chrome.mic_menu_w, sy)),
    );
    let (mark, ink) = if lane.solo_mic {
        ("✓ solo this mic", look.text_bright)
    } else {
        ("solo this mic", look.text_dim)
    };
    text(scene, labels, mark, 12.0, sy + 11.0, 9.0, ink, at);
}

/// SVG's `letter-spacing: 2` on the role eyebrow, which the shaper has
/// no attribute for: the spacing IS the eyebrow, so it is spelled into
/// the string rather than dropped.
fn spaced(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for (i, c) in s.chars().enumerate() {
        if i > 0 {
            out.push('\u{2009}');
        }
        out.push(c);
    }
    out
}

/// Draw a label on the baseline `y`, shaping it if it is new — the same
/// contract SVG's `text` y has, so the coordinates port unchanged.
fn text(
    scene: &mut Scene,
    labels: &mut Labeller,
    s: &str,
    x: f64,
    y: f64,
    size: f32,
    c: Color,
    at: Affine,
) {
    let shaped = labels.shape(s, size);
    // SVG's `text y` is the BASELINE, and every coordinate in this file
    // came from an SVG attribute. `crate::text::draw` positions the
    // run's TOP — parley's positioned glyphs already carry the ascent —
    // so passing an SVG y straight through drops the label by exactly
    // one ascent. Measured against the same fixture rendered both ways,
    // that was nine pixels on every lane label. Converted here rather
    // than by re-tuning the numbers, because those numbers are shared
    // with the press handler's hit testing.
    crate::text::draw(scene, &shaped, x, y - shaped.ascent, Align::Left, c, at);
}
