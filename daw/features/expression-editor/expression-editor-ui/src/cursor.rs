//! Resolving the pointer glyph, and drawing it.
//!
//! [`expression_editor_core::cursor`] decides what a resolved action
//! *means*; this decides what is under the pointer in the first place,
//! and draws the result.
//!
//! ## Why the glyphs are painted rather than named
//!
//! Blitz maps CSS `cursor` keywords onto winit's `CursorIcon` and
//! supports no `cursor: url(…)` (`blitz-dom::stylo_to_cursor_icon`). The
//! keyword set has no `[`, no `]`, no pencil, no razor and no eraser —
//! which is to say it has none of the shapes that carry the information.
//! `ew-resize` over a note end tells you an edge resizes; it does not
//! tell you *which* end you have, and grabbing the wrong one is the
//! mistake the bracket exists to prevent.
//!
//! So the OS cursor is hidden over the roll and the glyph is drawn into
//! a scene, which also means it looks identical standalone, in a VST3
//! editor and in REAPER — the same reason the roll itself is painted.
//!
//! ## Why it is its own layer
//!
//! A pointer move is the highest-frequency event the surface sees. If
//! the roll's own component read the hover position, every move would
//! rebuild the whole roll scene — every note, every curve, every label —
//! to move one 24-pixel glyph. [`CursorLayer`] is the only component
//! that reads hover, so a move re-renders that and nothing else, and it
//! sits under `pointer-events: none` so the roll underneath still gets
//! every event.

use anyrender::{PaintScene, Scene};
use dioxus::prelude::*;
use expression_editor_core::cursor::{Aim, Cursor};
use expression_editor_core::mouse::{Action, Context, Gesture};
use expression_editor_core::tools::{Hit, Mods};
use expression_editor_core::{Editor, Handle};
use kurbo::{Affine, BezPath, Circle, Line, Point, Rect, Stroke};
use peniko::{Color, Fill};

use crate::canvas;
use crate::interaction;
use crate::roll_widget::SceneSlot;
use crate::theme;

/// The glyph for the pointer at roll-local `(x, y)`.
///
/// Mirrors [`interaction::pointer_down`]'s precedence exactly, and that
/// is not incidental — a cursor resolved by a different order than the
/// press is a cursor that lies. The order is: chrome, then timing
/// separators, then note handles, then the mouse map.
///
/// `locked` is the drawer's modal state, which blocks editing gestures
/// while leaving navigation alone.
pub fn cursor_at(ed: &Editor, x: f64, y: f64, mods: Mods, locked: bool) -> Cursor {
    // Chrome first: the ruler and the key gutter are outside the roll's
    // coordinate space, and `roll.rs` routes presses there before
    // `interaction` ever sees them.
    if y < 0.0 {
        return Cursor::Playhead;
    }
    if x < 0.0 {
        return Cursor::Audition;
    }

    // Timing separators outrank the notes they sit between, the way
    // `separator_press` does.
    if ed.timing_mode {
        let grab = canvas::SEPARATOR_GRAB_PX;
        if canvas::separators(ed)
            .iter()
            .any(|s| (s.x - x).abs() <= grab)
        {
            return if locked {
                Cursor::Forbidden
            } else {
                Cursor::Split
            };
        }
    }

    // Handles are drawn in front and pressed first, so they are resolved
    // first here too.
    if ed.mode.has_handles()
        && let Some(handle) = canvas::note_handles(ed)
            .iter()
            .find_map(|s| expression_editor_core::handles::hit(&s.rects, x, y))
    {
        return if locked {
            Cursor::Forbidden
        } else {
            Cursor::for_action(Action::None, Context::Note, Aim::handle(handle))
        };
    }

    let context = interaction::context_at(ed, x, y);
    // `Gesture::Drag` is the honest question to ask the map: a press
    // that never moves is a Click, but nothing knows that yet at hover
    // time, and the drag binding is the one with consequences.
    let action = ed.mouse.resolve_for(context, Gesture::Drag, mods, ed.tool);

    // A *view* tool is never forbidden. `is_edit` is asked of the
    // resolved action, and an armed tool resolves to `ActiveTool` — one
    // answer for all seven tools, which is right for six of them and
    // wrong for zoom. The lock exists to stop you changing the material
    // while a preview is up; looking around is not that.
    if locked && action.is_edit() && !ed.tool.is_view() {
        return Cursor::Forbidden;
    }

    // Which end of a note the pointer has, for the brackets.
    let aim = match ed.hit_test(x, y) {
        Hit::NoteEdge { start_edge, .. } => Aim::edge(start_edge),
        _ => Aim::NONE,
    };

    match action {
        // The map handed the gesture to the armed tool; only the tool
        // knows what that looks like.
        Action::ActiveTool => Cursor::for_tool(ed.tool, context),
        other => Cursor::for_action(other, context, aim),
    }
}

// ── drawing ─────────────────────────────────────────────────────────

/// The glyph's box, in CSS pixels. Every shape below is drawn inside
/// `0..SIZE` on both axes and positioned by the layer.
pub const SIZE: f64 = 24.0;

/// Where inside the box the pointer actually is.
///
/// A crosshair is centred; a pencil points from its tip. Getting this
/// wrong is the difference between a cursor that aims and one that
/// hovers a few pixels off whatever you meant to hit.
fn hotspot(cursor: Cursor) -> (f64, f64) {
    let c = SIZE * 0.5;
    match cursor {
        // Centred: everything that means "this point".
        Cursor::Crosshair
        | Cursor::MarqueeAdd
        | Cursor::MarqueeToggle
        | Cursor::EdgeLeft
        | Cursor::EdgeRight
        | Cursor::StretchLeft
        | Cursor::StretchRight
        | Cursor::Move
        | Cursor::MoveH
        | Cursor::MoveV
        | Cursor::Velocity
        | Cursor::Scale
        | Cursor::Split
        | Cursor::Playhead
        | Cursor::Handle(_)
        | Cursor::RazorEdge => (c, c),
        // Tip-anchored: the drawing tools aim with their point, which is
        // drawn at the bottom-left of the box.
        Cursor::Pencil | Cursor::Curve | Cursor::Brush | Cursor::Razor => (2.0, SIZE - 2.0),
        // Everything else aims from its top-left, like an arrow.
        _ => (2.0, 2.0),
    }
}

fn stroke(w: f64) -> Stroke {
    Stroke::new(w)
}

/// Draw `cursor` into `scene` with its hotspot at `(x, y)`.
///
/// Two passes for every shape: a dark halo underneath and the bright
/// glyph on top. A single-colour cursor disappears the moment it crosses
/// something its own colour, and the roll is full of both dark grid and
/// bright notes.
pub fn draw(scene: &mut Scene, cursor: Cursor, x: f64, y: f64) {
    let (hx, hy) = hotspot(cursor);
    let at = Affine::translate((x - hx, y - hy));
    let halo = Color::from_rgba8(0, 0, 0, 190);
    // The brightest text colour, not the accent: the cursor has to stay
    // legible over selected notes, which are painted in the accent.
    let ink = crate::paint::color(theme::TEXT_BRIGHT);
    // Halo first, fat; then the glyph, thin, on top.
    for (color, width) in [(halo, 3.4), (ink, 1.5)] {
        shape(scene, cursor, at, color, width);
    }
}

fn shape(scene: &mut Scene, cursor: Cursor, at: Affine, color: Color, w: f64) {
    let s = stroke(w);
    let c = SIZE * 0.5;
    match cursor {
        Cursor::Arrow => arrow(scene, at, color, w),
        Cursor::Crosshair => cross(scene, at, color, w, c, c, 8.0),
        Cursor::MarqueeAdd => {
            cross(scene, at, color, w, c, c, 8.0);
            plus(scene, at, color, w, SIZE - 4.0, 4.0, 3.0);
        }
        Cursor::MarqueeToggle => {
            cross(scene, at, color, w, c, c, 8.0);
            // A dot rather than a plus: toggling is not adding.
            scene.fill(
                Fill::NonZero,
                at,
                color,
                None,
                &Circle::new(Point::new(SIZE - 4.0, 4.0), w * 0.9),
            );
        }

        // `[` and `]`, drawn as REAPER draws them: a full-height bracket
        // with a vertical stem through it, so the stem marks the exact
        // sample the edge would land on.
        Cursor::EdgeLeft => bracket(scene, at, color, w, true, false),
        Cursor::EdgeRight => bracket(scene, at, color, w, false, false),
        Cursor::StretchLeft => bracket(scene, at, color, w, true, true),
        Cursor::StretchRight => bracket(scene, at, color, w, false, true),

        Cursor::Move => {
            arrow_line(scene, at, color, w, (c, c), (c - 9.0, c), 3.0);
            arrow_line(scene, at, color, w, (c, c), (c + 9.0, c), 3.0);
            arrow_line(scene, at, color, w, (c, c), (c, c - 9.0), 3.0);
            arrow_line(scene, at, color, w, (c, c), (c, c + 9.0), 3.0);
        }
        Cursor::MoveH | Cursor::RazorEdge => {
            arrow_line(scene, at, color, w, (c, c), (c - 9.0, c), 3.5);
            arrow_line(scene, at, color, w, (c, c), (c + 9.0, c), 3.5);
        }
        Cursor::MoveV | Cursor::Velocity => {
            arrow_line(scene, at, color, w, (c, c), (c, c - 9.0), 3.5);
            arrow_line(scene, at, color, w, (c, c), (c, c + 9.0), 3.5);
        }
        Cursor::Scale => {
            // The same double arrow, but pinned to a pivot bar: what
            // scaling does is spread about a line, not translate.
            arrow_line(scene, at, color, w, (c, c), (c, c - 9.0), 3.5);
            arrow_line(scene, at, color, w, (c, c), (c, c + 9.0), 3.5);
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(Point::new(c - 7.0, c), Point::new(c + 7.0, c)),
            );
        }
        Cursor::Copy => {
            arrow(scene, at, color, w);
            plus(scene, at, color, w, SIZE - 5.0, 5.0, 4.0);
        }

        Cursor::Pencil => pencil(scene, at, color, w),
        Cursor::Brush => {
            pencil(scene, at, color, w);
            // Three grid ticks trailing the tip: a brush lays down a run
            // of notes, not one.
            for i in 0..3 {
                let x = 5.0 + i as f64 * 4.0;
                scene.stroke(
                    &s,
                    at,
                    color,
                    None,
                    &Line::new(Point::new(x, SIZE - 1.0), Point::new(x + 2.5, SIZE - 1.0)),
                );
            }
        }
        Cursor::Curve => {
            // An S from the tip: the shaped-ramp gesture.
            let mut p = BezPath::new();
            p.move_to(Point::new(2.0, SIZE - 2.0));
            p.curve_to(
                Point::new(8.0, SIZE - 4.0),
                Point::new(10.0, 6.0),
                Point::new(SIZE - 3.0, 3.0),
            );
            scene.stroke(&s, at, color, None, &p);
        }
        Cursor::Eraser | Cursor::NoteEraser => {
            // A tilted block, and for notes a cross through it: wiping a
            // controller back to default is not the same as deleting.
            let mut p = BezPath::new();
            p.move_to(Point::new(3.0, SIZE - 6.0));
            p.line_to(Point::new(11.0, SIZE - 14.0));
            p.line_to(Point::new(SIZE - 4.0, SIZE - 20.0));
            p.line_to(Point::new(SIZE - 12.0, SIZE - 12.0));
            p.close_path();
            scene.stroke(&s, at, color, None, &p);
            if cursor == Cursor::NoteEraser {
                cross_diag(scene, at, color, w, SIZE - 6.0, 6.0, 3.5);
            }
        }

        Cursor::Hand | Cursor::HandClosed => {
            hand(scene, at, color, w, cursor == Cursor::HandClosed)
        }
        Cursor::Zoom => {
            scene.stroke(&s, at, color, None, &Circle::new(Point::new(9.0, 9.0), 6.0));
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(Point::new(13.5, 13.5), Point::new(SIZE - 3.0, SIZE - 3.0)),
            );
            plus(scene, at, color, w, 9.0, 9.0, 3.0);
        }
        Cursor::Playhead | Cursor::Split => {
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(Point::new(c, 2.0), Point::new(c, SIZE - 2.0)),
            );
            // The playhead carries its flag; a split carries opposed
            // arrows, because it pushes material both ways.
            if cursor == Cursor::Playhead {
                let mut p = BezPath::new();
                p.move_to(Point::new(c, 2.0));
                p.line_to(Point::new(c + 7.0, 5.0));
                p.line_to(Point::new(c, 8.0));
                p.close_path();
                scene.fill(Fill::NonZero, at, color, None, &p);
            } else {
                arrow_line(scene, at, color, w, (c - 2.0, c), (c - 8.0, c), 3.0);
                arrow_line(scene, at, color, w, (c + 2.0, c), (c + 8.0, c), 3.0);
            }
        }
        Cursor::Audition => {
            // A speaker cone with one wave.
            let mut p = BezPath::new();
            p.move_to(Point::new(4.0, c - 3.0));
            p.line_to(Point::new(8.0, c - 3.0));
            p.line_to(Point::new(12.0, c - 8.0));
            p.line_to(Point::new(12.0, c + 8.0));
            p.line_to(Point::new(8.0, c + 3.0));
            p.line_to(Point::new(4.0, c + 3.0));
            p.close_path();
            scene.stroke(&s, at, color, None, &p);
            let mut wave = BezPath::new();
            wave.move_to(Point::new(15.0, c - 5.0));
            wave.curve_to(
                Point::new(19.0, c - 2.0),
                Point::new(19.0, c + 2.0),
                Point::new(15.0, c + 5.0),
            );
            scene.stroke(&s, at, color, None, &wave);
        }
        Cursor::Mute => {
            arrow(scene, at, color, w);
            cross_diag(scene, at, color, w, SIZE - 6.0, 6.0, 4.0);
        }
        Cursor::Text => {
            // An I-beam, at the pointer rather than the corner.
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(Point::new(c, 3.0), Point::new(c, SIZE - 3.0)),
            );
            for y in [3.0, SIZE - 3.0] {
                scene.stroke(
                    &s,
                    at,
                    color,
                    None,
                    &Line::new(Point::new(c - 3.0, y), Point::new(c + 3.0, y)),
                );
            }
        }
        Cursor::Razor => {
            // A blade: the rectangle is the handle, the line is the cut.
            scene.stroke(&s, at, color, None, &Rect::new(6.0, 4.0, SIZE - 3.0, 11.0));
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(Point::new(2.0, SIZE - 2.0), Point::new(SIZE - 3.0, 11.0)),
            );
        }

        Cursor::Handle(handle) => handle_glyph(scene, at, color, w, handle),

        Cursor::Forbidden => {
            scene.stroke(&s, at, color, None, &Circle::new(Point::new(c, c), 8.0));
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(Point::new(c - 5.6, c + 5.6), Point::new(c + 5.6, c - 5.6)),
            );
        }
    }
}

/// The seven note handles, each drawn as what it does to the note.
///
/// They share a common frame — a short note bar with the affected part
/// marked — so the set reads as one family rather than seven unrelated
/// icons.
fn handle_glyph(scene: &mut Scene, at: Affine, color: Color, w: f64, handle: Handle) {
    let s = stroke(w);
    let c = SIZE * 0.5;
    match handle {
        // Coarse pitch: the note, moving between rows.
        Handle::Pitch => {
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Rect::new(5.0, c - 2.5, SIZE - 5.0, c + 2.5),
            );
            arrow_line(scene, at, color, w, (c, c - 4.0), (c, 2.0), 3.0);
            arrow_line(scene, at, color, w, (c, c + 4.0), (c, SIZE - 2.0), 3.0);
        }
        // Fine pitch: the same, dotted — cents, not rows.
        Handle::FinePitch => {
            for i in 0..4 {
                let x = 5.0 + i as f64 * 3.6;
                scene.stroke(
                    &s,
                    at,
                    color,
                    None,
                    &Line::new(Point::new(x, c), Point::new(x + 2.0, c)),
                );
            }
            arrow_line(scene, at, color, w, (c + 5.0, c - 3.0), (c + 5.0, 3.0), 2.5);
            arrow_line(
                scene,
                at,
                color,
                w,
                (c + 5.0, c + 3.0),
                (c + 5.0, SIZE - 3.0),
                2.5,
            );
        }
        // The slopes: the transition tilting in or out.
        Handle::LeftSlope | Handle::RightSlope => {
            let mut p = BezPath::new();
            if handle == Handle::LeftSlope {
                p.move_to(Point::new(3.0, SIZE - 4.0));
                p.curve_to(
                    Point::new(8.0, SIZE - 6.0),
                    Point::new(9.0, 6.0),
                    Point::new(14.0, 5.0),
                );
                p.line_to(Point::new(SIZE - 3.0, 5.0));
            } else {
                p.move_to(Point::new(3.0, 5.0));
                p.line_to(Point::new(10.0, 5.0));
                p.curve_to(
                    Point::new(15.0, 6.0),
                    Point::new(16.0, SIZE - 6.0),
                    Point::new(SIZE - 3.0, SIZE - 4.0),
                );
            }
            scene.stroke(&s, at, color, None, &p);
        }
        // Formant: the note with a shifted overtone above it.
        Handle::Formant => {
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Rect::new(5.0, c + 2.0, SIZE - 5.0, c + 6.0),
            );
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(Point::new(7.0, c - 4.0), Point::new(SIZE - 7.0, c - 4.0)),
            );
            arrow_line(scene, at, color, w, (c, c - 6.0), (c, 2.0), 3.0);
        }
        // Amplitude: a level bar.
        Handle::Amplitude => {
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(
                    Point::new(6.0, SIZE - 4.0),
                    Point::new(SIZE - 6.0, SIZE - 4.0),
                ),
            );
            scene.fill(
                Fill::NonZero,
                at,
                color,
                None,
                &Rect::new(c - 3.0, c - 2.0, c + 3.0, SIZE - 4.0),
            );
            arrow_line(scene, at, color, w, (c, c - 4.0), (c, 3.0), 3.0);
        }
        // Vibrato: the wave, deepening.
        Handle::Vibrato => {
            let mut p = BezPath::new();
            p.move_to(Point::new(3.0, c));
            for i in 0..3 {
                let x = 3.0 + i as f64 * 6.0;
                p.curve_to(
                    Point::new(x + 1.5, c - 6.0),
                    Point::new(x + 4.5, c + 6.0),
                    Point::new(x + 6.0, c),
                );
            }
            scene.stroke(&s, at, color, None, &p);
        }
    }
}

// ── primitives ──────────────────────────────────────────────────────

/// The `[` / `]` bracket. `arrowed` adds the outward arrow that marks a
/// stretch — a gesture that reaches past the note you grabbed.
fn bracket(scene: &mut Scene, at: Affine, color: Color, w: f64, left: bool, arrowed: bool) {
    let s = stroke(w);
    let c = SIZE * 0.5;
    let top = 3.0;
    let bottom = SIZE - 3.0;
    // The stem sits on the hotspot, so the bracket's spine *is* the
    // sample the edge would snap to.
    let arm = 5.0;
    let dir = if left { 1.0 } else { -1.0 };
    let mut p = BezPath::new();
    p.move_to(Point::new(c + arm * dir, top));
    p.line_to(Point::new(c, top));
    p.line_to(Point::new(c, bottom));
    p.line_to(Point::new(c + arm * dir, bottom));
    scene.stroke(&s, at, color, None, &p);
    if arrowed {
        arrow_line(scene, at, color, w, (c, c), (c - 8.0 * dir, c), 3.0);
    }
}

fn cross(scene: &mut Scene, at: Affine, color: Color, w: f64, x: f64, y: f64, r: f64) {
    let s = stroke(w);
    scene.stroke(
        &s,
        at,
        color,
        None,
        &Line::new(Point::new(x - r, y), Point::new(x + r, y)),
    );
    scene.stroke(
        &s,
        at,
        color,
        None,
        &Line::new(Point::new(x, y - r), Point::new(x, y + r)),
    );
}

fn cross_diag(scene: &mut Scene, at: Affine, color: Color, w: f64, x: f64, y: f64, r: f64) {
    let s = stroke(w);
    scene.stroke(
        &s,
        at,
        color,
        None,
        &Line::new(Point::new(x - r, y - r), Point::new(x + r, y + r)),
    );
    scene.stroke(
        &s,
        at,
        color,
        None,
        &Line::new(Point::new(x - r, y + r), Point::new(x + r, y - r)),
    );
}

fn plus(scene: &mut Scene, at: Affine, color: Color, w: f64, x: f64, y: f64, r: f64) {
    cross(scene, at, color, w, x, y, r);
}

/// The plain pointer, drawn from its tip at the top-left.
fn arrow(scene: &mut Scene, at: Affine, color: Color, w: f64) {
    let mut p = BezPath::new();
    p.move_to(Point::new(2.0, 2.0));
    p.line_to(Point::new(2.0, 16.0));
    p.line_to(Point::new(6.0, 12.5));
    p.line_to(Point::new(9.0, 18.5));
    p.line_to(Point::new(11.5, 17.0));
    p.line_to(Point::new(8.5, 11.5));
    p.line_to(Point::new(13.0, 11.0));
    p.close_path();
    scene.stroke(&stroke(w), at, color, None, &p);
}

/// A line from `from` to `to` with a head at `to`.
fn arrow_line(
    scene: &mut Scene,
    at: Affine,
    color: Color,
    w: f64,
    from: (f64, f64),
    to: (f64, f64),
    head: f64,
) {
    let s = stroke(w);
    let (fx, fy) = from;
    let (tx, ty) = to;
    scene.stroke(
        &s,
        at,
        color,
        None,
        &Line::new(Point::new(fx, fy), Point::new(tx, ty)),
    );
    let (dx, dy) = (tx - fx, ty - fy);
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / len, dy / len);
    // Perpendicular, for the two barbs.
    let (px, py) = (-uy, ux);
    let mut p = BezPath::new();
    p.move_to(Point::new(tx, ty));
    p.line_to(Point::new(
        tx - ux * head + px * head * 0.7,
        ty - uy * head + py * head * 0.7,
    ));
    p.line_to(Point::new(
        tx - ux * head - px * head * 0.7,
        ty - uy * head - py * head * 0.7,
    ));
    p.close_path();
    scene.fill(Fill::NonZero, at, color, None, &p);
}

fn pencil(scene: &mut Scene, at: Affine, color: Color, w: f64) {
    let s = stroke(w);
    // Body.
    let mut body = BezPath::new();
    body.move_to(Point::new(6.0, SIZE - 6.0));
    body.line_to(Point::new(SIZE - 5.0, 3.0));
    body.line_to(Point::new(SIZE - 2.0, 6.0));
    body.line_to(Point::new(9.0, SIZE - 3.0));
    body.close_path();
    scene.stroke(&s, at, color, None, &body);
    // Tip, at the hotspot.
    let mut tip = BezPath::new();
    tip.move_to(Point::new(2.0, SIZE - 2.0));
    tip.line_to(Point::new(6.0, SIZE - 6.0));
    tip.line_to(Point::new(9.0, SIZE - 3.0));
    tip.close_path();
    scene.fill(Fill::NonZero, at, color, None, &tip);
}

fn hand(scene: &mut Scene, at: Affine, color: Color, w: f64, closed: bool) {
    let s = stroke(w);
    // Palm.
    let palm = Rect::new(6.0, 11.0, SIZE - 5.0, SIZE - 3.0);
    scene.stroke(&s, at, color, None, &palm);
    // Fingers, short when the hand has closed on the canvas.
    let top = if closed { 8.0 } else { 3.0 };
    for i in 0..3 {
        let x = 8.0 + i as f64 * 4.0;
        scene.stroke(
            &s,
            at,
            color,
            None,
            &Line::new(Point::new(x, 11.0), Point::new(x, top)),
        );
    }
    // Thumb.
    scene.stroke(
        &s,
        at,
        color,
        None,
        &Line::new(Point::new(6.0, 13.0), Point::new(2.5, 16.0)),
    );
}

// ── the layer ───────────────────────────────────────────────────────

/// The painted cursor, in its own element.
///
/// Sized to one glyph and moved by its inline style, so a pointer move
/// costs this component's render and nothing else — see the module note.
/// `pointer-events: none` keeps it transparent to the roll beneath.
#[component]
pub fn CursorLayer(
    editor: Signal<Editor>,
    /// Roll-local pointer position, or `None` when the pointer has left.
    hover: Signal<Option<(f64, f64)>>,
    /// Modifiers as of the last pointer or key event: the glyph has to
    /// change the instant Alt is held, without waiting for a move.
    mods: Signal<Mods>,
    /// The live gesture, for the closed hand.
    drag: Signal<crate::interaction::Drag>,
    locked: bool,
) -> Element {
    let slot = use_hook(SceneSlot::new);
    let widget = use_hook(|| {
        dioxus_native_dom::CustomWidgetAttr::new(crate::roll_widget::SceneWidget::new(slot.clone()))
    });

    // Mounted unconditionally, from the very first render.
    //
    // This used to return an empty `rsx!` until the pointer arrived, and
    // that is what made the roll go blank: `CustomWidgetAttr` is
    // write-once — the DOM takes the widget out of it on the first
    // mutation — so an `<object>` created on a *later* render gets no
    // `data` attribute at all. With no widget it is a replaced element
    // with no intrinsic size, so it lays out 0x0, and blitz-paint skips
    // a zero-box widget silently. The roll's own object carries a
    // comment about exactly this trap; the cursor walked into it.
    //
    // So the node is permanent and only its *style* changes. Before the
    // pointer has ever been over the roll there is nothing to draw, and
    // `visibility: hidden` says so without disturbing the box — a
    // `display: none` toggle would put the same relayout back.
    let hovering = hover();

    let cursor = hovering.map(|(x, y)| {
        let ed = editor.read();
        let resolved = cursor_at(&ed, x, y, mods(), locked);
        if drag.read().is_active() {
            resolved.while_dragging()
        } else {
            resolved
        }
    });

    let mut scene = Scene::new();
    if let Some(cursor) = cursor {
        draw(&mut scene, cursor, SIZE, SIZE);
    }
    slot.put(scene);

    // The box is 2xSIZE so a glyph whose hotspot is at a corner still
    // has room to draw in every direction from it; the drawing above is
    // centred on (SIZE, SIZE) to match.
    let box_ = SIZE * 2.0;
    let (left, top) = match hovering {
        Some((x, y)) => (x + canvas::GUTTER_W - SIZE, y + canvas::RULER_H - SIZE),
        None => (0.0, 0.0),
    };
    let visibility = if hovering.is_some() {
        "visible"
    } else {
        "hidden"
    };
    let label = cursor.map(|c| c.label()).unwrap_or("none");

    rsx! {
        object {
            "data-testid": "cursor",
            "data-cursor": "{label}",
            "data": widget,
            style: "position: absolute; pointer-events: none; display: block; \
                    visibility: {visibility}; \
                    left: {left:.1}px; top: {top:.1}px; \
                    width: {box_}px; height: {box_}px;",
        }
    }
}
