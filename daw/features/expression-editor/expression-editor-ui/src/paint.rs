//! The roll, drawn as an [`anyrender::Scene`].
//!
//! This is the whole drawing, and it is *only* the drawing: no DOM, no
//! renderer, no window, no dioxus. A [`Scene`] is a plain recording of
//! draw commands, so this module compiles anywhere the geometry does —
//! including wasm — and whatever anyrender backend is present replays
//! it. `paint.rs` is portable; only the [`crate::roll_widget`] seam that
//! puts a scene on screen is native.
//!
//! ## Why this exists at all
//!
//! The roll used to be an inline `<svg>` subtree. That cost two things
//! it could not afford:
//!
//! - **Correctness.** Blitz paints an inline svg as a *replaced element*
//!   with a hardcoded `object-fit: contain`, so everything drawn is
//!   scaled by (element box / declared size) — and an svg that declares
//!   no size takes it from its own content. The drawing therefore
//!   rescaled as the roll scrolled, and the pointer mapping with it.
//!   See `crate::sizing` for the full account.
//! - **Speed.** Every camera move rebuilt the roll's markup, which Blitz
//!   re-parsed into a usvg tree before drawing a pixel. That cost scales
//!   with the note count and there is no way to tune it from above.
//!
//! A scene has neither problem. Nothing is scaled unless this module
//! scales it, and the recording is rebuilt only when the state it draws
//! actually changes — the renderer replays it every frame regardless.
//!
//! ## Coordinates
//!
//! Element space: `(0, 0)` is the top-left of the roll's own box, so the
//! keyboard gutter occupies the first [`canvas::GUTTER_W`] pixels and
//! the ruler the first [`canvas::RULER_H`]. Roll content is translated
//! past both and clipped to what is left, which is exactly what the
//! `<g transform=… clip-path=…>` did.

use anyrender::{PaintScene, Scene};
use expression_editor_core::razor::RazorArea;
use expression_editor_core::{Dimension, Editor};
use kurbo::{Affine, BezPath, Line, Point, Rect, Stroke};
use peniko::{Color, Fill};

use crate::canvas;
use crate::text::{self, Labeller};
use crate::theme;

/// View state the document does not carry.
///
/// Everything else is read from the [`Editor`]. These are the few things
/// that live in component signals — a drag in progress, a modal drawing
/// — and they are passed in rather than reached for, so this function
/// stays a pure map from state to picture.
#[derive(Default)]
pub struct Overlay {
    /// The marquee rectangle, in roll space, while one is being dragged.
    pub marquee: Option<(f64, f64, f64, f64)>,
    /// An open pitch drawing, which owns the surface while it is up.
    pub draft: Option<canvas::DraftView>,
    /// The razor area a sweep in progress would commit, already snapped.
    ///
    /// A razor used to appear only on release, so the most destructive
    /// selection on the surface was the one you could not see before
    /// committing to it. Drawn from the same resolved area the release
    /// will add, so what is previewed is what lands.
    pub razor: Option<RazorArea>,
    /// Where a string roll draws its bend flow (#161).
    pub flow: crate::guitar::BendFlow,
}

/// Parse a theme colour.
///
/// The palette is CSS strings because it is shared with the parts of the
/// surface that are still DOM. Falling back to transparent rather than
/// panicking: a mistyped colour should cost a shape, not the window.
pub fn color(css: &str) -> Color {
    peniko::color::parse_color(css)
        .map(|c| c.to_alpha_color())
        .unwrap_or(Color::TRANSPARENT)
}

fn with_alpha(c: Color, alpha: f64) -> Color {
    c.with_alpha(alpha as f32)
}

/// Parse the `"x,y x,y "` point lists the geometry layer produces.
///
/// Those strings exist because they were svg `points` attributes. They
/// are parsed back here rather than changed at the source so that this
/// port does not also rewrite every producer in `canvas.rs`; the
/// producers can grow structured forms one at a time, and this goes away
/// with the last of them.
fn points_of(s: &str) -> Vec<Point> {
    s.split_whitespace()
        .filter_map(|pair| {
            let (x, y) = pair.split_once(',')?;
            Some(Point::new(x.parse().ok()?, y.parse().ok()?))
        })
        .collect()
}

fn polyline(s: &str) -> BezPath {
    path_of(points_of(s).into_iter(), false)
}

/// A polyline straight from the numbers, with no text in between.
fn path_of(points: impl Iterator<Item = Point>, close: bool) -> BezPath {
    let mut path = BezPath::new();
    for (i, p) in points.enumerate() {
        if i == 0 {
            path.move_to(p);
        } else {
            path.line_to(p);
        }
    }
    if close && !path.is_empty() {
        path.close_path();
    }
    path
}

/// The geometry layer's polyline type, as a path.
fn line_of(points: &[(f64, f64)]) -> BezPath {
    path_of(points.iter().map(|&(x, y)| Point::new(x, y)), false)
}

fn polygon(s: &str) -> BezPath {
    let mut path = polyline(s);
    if !path.is_empty() {
        path.close_path();
    }
    path
}

fn stroke_of(width: f64) -> Stroke {
    Stroke::new(width)
}

/// Shapes that share a paint, gathered into one path.
///
/// A roll of two thousand notes emitted about ten thousand draw
/// commands, one per rectangle, line and glyph run — and each command
/// allocates a path. That was six of the seven milliseconds a frame
/// cost, and it is work that buys nothing: the notes share a dozen
/// pitch-class colours between them, the rows share two, the gridlines
/// two.
///
/// Gathering by paint collapses those thousands of commands into a
/// handful. It is also what the renderer wants — one large path is far
/// cheaper for Vello to process than hundreds of small ones.
///
/// Ordering within a batch is lost, which is why a batch only ever
/// covers shapes that sit at the same depth: every row, or every note
/// body. Anything drawn *over* something else goes in a later batch.
#[derive(Default)]
struct Batch {
    // A `Vec` rather than a map: there are only ever a handful of
    // distinct paints, so a linear scan beats hashing a colour.
    entries: Vec<(Color, BezPath)>,
}

impl Batch {
    fn add(&mut self, color: Color, shape: &impl kurbo::Shape) {
        let path = match self.entries.iter_mut().find(|(c, _)| *c == color) {
            Some((_, path)) => path,
            None => {
                self.entries.push((color, BezPath::new()));
                &mut self.entries.last_mut().expect("just pushed").1
            }
        };
        path.extend(shape.path_elements(0.1));
    }

    fn fill(self, scene: &mut Scene, at: Affine) {
        for (color, path) in self.entries {
            scene.fill(Fill::NonZero, at, color, None, &path);
        }
    }

    fn stroke(self, scene: &mut Scene, at: Affine, width: f64) {
        let stroke = stroke_of(width);
        for (color, path) in self.entries {
            scene.stroke(&stroke, at, color, None, &path);
        }
    }
}

/// Draw the roll into a scene sized `w` x `h` in CSS pixels.
///
/// `w` and `h` are the element's box, handed down by the widget. Nothing
/// here derives a size from the content, which is the property the svg
/// could not offer.
pub fn roll_scene(ed: &Editor, w: f64, h: f64, overlay: &Overlay, labels: &mut Labeller) -> Scene {
    let mut scene = Scene::new();
    let vp = ed.viewport;

    // The background covers the whole element, gutter and ruler included.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color(theme::BG),
        None,
        &Rect::new(0.0, 0.0, w, h),
    );

    // ── the roll proper ──────────────────────────────────────────────
    //
    // Translated past the chrome and clipped to what is left, so a pitch
    // curve that travels off-screen cannot paint over the ruler or the
    // keyboard.
    let roll_at = Affine::translate((canvas::GUTTER_W, canvas::RULER_H));
    scene.push_clip_layer(
        roll_at,
        &Rect::new(
            0.0,
            0.0,
            (w - canvas::GUTTER_W).max(0.0),
            (h - canvas::RULER_H).max(0.0),
        ),
    );

    rows(&mut scene, ed, roll_at, vp.w);
    grid(&mut scene, ed, roll_at, vp.h);
    lanes(&mut scene, ed, roll_at, vp.w);
    guides(&mut scene, ed, roll_at, vp.w, labels);
    audio(&mut scene, ed, roll_at);
    time_selection(&mut scene, ed, roll_at, vp.h);
    razors(&mut scene, ed, roll_at, overlay.razor.as_ref());
    references(&mut scene, ed, roll_at);
    notes(&mut scene, ed, roll_at, labels);
    curves(&mut scene, ed, roll_at);
    strings(&mut scene, ed, roll_at, overlay, labels);
    draft(&mut scene, overlay, roll_at);
    controllers(&mut scene, ed, roll_at, vp.w, vp.h, labels);
    playhead(&mut scene, ed, roll_at, vp.h);

    if let Some((x, y, mw, mh)) = overlay.marquee {
        let r = Rect::new(x, y, x + mw, y + mh);
        scene.fill(
            Fill::NonZero,
            roll_at,
            with_alpha(color(theme::ACCENT), 0.15),
            None,
            &r,
        );
        scene.stroke(&stroke_of(1.0), roll_at, color(theme::ACCENT), None, &r);
    }

    scene.pop_layer();

    // ── chrome ───────────────────────────────────────────────────────
    //
    // Painted after the roll, so anything that overflowed is covered
    // rather than showing through the keyboard.
    keyboard(&mut scene, ed, h, labels);
    ruler(&mut scene, ed, w, labels);

    scene
}

/// Draw a label, shaping it if it has not been seen before.
fn label(
    scene: &mut Scene,
    labels: &mut Labeller,
    s: &str,
    x: f64,
    y: f64,
    size: f32,
    align: text::Align,
    c: Color,
    at: Affine,
) {
    let shaped = labels.shape(s, size);
    text::draw(scene, &shaped, x, y, align, c, at);
}

/// The velocity / CC strip under the roll, as a scene.
///
/// Painted for the same reason the roll is, and it turned out to matter
/// more: the strip drew **one svg `<rect>` per note**, so a project with
/// two thousand notes put two thousand DOM nodes under the roll and
/// Blitz restyled and re-laid-out every one of them on every camera
/// move. That was most of the twelve milliseconds a pan cost at that
/// note count — the drawing was never the problem, the *elements* were.
pub fn strip_scene(ed: &Editor, w: f64, h: f64, labels: &mut Labeller) -> Scene {
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color(theme::SURFACE_BAR),
        None,
        &Rect::new(0.0, 0.0, w, h),
    );

    // The gutter column, so the strip lines up with the roll.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color(theme::GUTTER_BG),
        None,
        &Rect::new(0.0, 0.0, canvas::GUTTER_W, h),
    );
    label(
        scene_mut(&mut scene),
        labels,
        ed.strip_lane.label(),
        6.0,
        14.0,
        9.0,
        text::Align::Left,
        color(theme::TEXT_DIM),
        Affine::IDENTITY,
    );

    let at = Affine::translate((canvas::GUTTER_W, 0.0));
    let vp_w = ed.viewport.w;

    let mut guides = Batch::default();
    for (y, major) in canvas::strip_guides(h) {
        guides.add(
            color(if major {
                theme::GRID_BEAT
            } else {
                theme::GRID_SUB
            }),
            &Line::new((0.0, y), (vp_w, y)),
        );
    }
    guides.stroke(&mut scene, at, 1.0);

    // One path per colour, rather than one command per note.
    let mut stems = Batch::default();
    let mut caps = Batch::default();
    for s in canvas::stems(ed, h) {
        stems.add(
            with_alpha(color(s.color), if s.muted { 0.2 } else { 0.85 }),
            &Rect::new(s.x, s.y, s.x + s.w, s.y + s.h.max(1.0)),
        );
        // A cap on the selected stems, so the ones a drag will actually
        // move are obvious.
        if s.selected {
            caps.add(
                color(theme::SELECTED),
                &Rect::new(s.x - 1.0, s.y - 2.0, s.x + s.w + 1.0, s.y + 1.0),
            );
        }
    }
    stems.fill(&mut scene, at);
    caps.fill(&mut scene, at);

    let mut quiet = Batch::default();
    let mut selected = Batch::default();
    for c in canvas::strip_curves(ed, h) {
        let path = line_of(&c.points);
        if path.is_empty() {
            continue;
        }
        if c.selected {
            selected.add(color(c.color), &path);
        } else {
            quiet.add(with_alpha(color(c.color), 0.6), &path);
        }
    }
    quiet.stroke(&mut scene, at, 1.5);
    selected.stroke(&mut scene, at, 1.5);

    scene
}

/// `label` takes `&mut Scene`; this is only here to keep the call above
/// readable rather than reborrowing inline.
fn scene_mut(scene: &mut Scene) -> &mut Scene {
    scene
}

fn rows(scene: &mut Scene, ed: &Editor, at: Affine, w: f64) {
    let mut bands = Batch::default();
    let mut dividers = Batch::default();
    for r in canvas::rows(ed) {
        bands.add(color(r.fill), &Rect::new(0.0, r.y, w, r.y + r.h));
        // One divider per group rather than per row: evenly-ruled lanes
        // give the eye nothing to steer by.
        if r.starts_group {
            dividers.add(color(theme::PANEL_BORDER), &Line::new((0.0, r.y), (w, r.y)));
        }
    }
    bands.fill(scene, at);
    dividers.stroke(scene, at, 1.0);
}

fn grid(scene: &mut Scene, ed: &Editor, at: Affine, h: f64) {
    let mut lines = Batch::default();
    for g in canvas::grid_lines(ed) {
        let c = if g.beat {
            theme::GRID_BEAT
        } else {
            theme::GRID_SUB
        };
        lines.add(color(c), &Line::new((g.x, 0.0), (g.x, h)));
    }
    lines.stroke(scene, at, 1.0);
}

fn lanes(scene: &mut Scene, ed: &Editor, at: Affine, w: f64) {
    let _ = w;
    for b in canvas::lane_boxes(ed) {
        let r = Rect::new(b.x, b.y, b.x + b.w, b.y + b.h);
        scene.fill(
            Fill::NonZero,
            at,
            with_alpha(color(theme::SURFACE_INSET), 0.55),
            None,
            &r,
        );
        scene.stroke(&stroke_of(1.0), at, color(theme::PANEL_BORDER), None, &r);
    }
}

fn guides(scene: &mut Scene, ed: &Editor, at: Affine, w: f64, labels: &mut Labeller) {
    for g in canvas::tuning_guides(ed) {
        scene.stroke(
            &stroke_of(1.0),
            at,
            with_alpha(color(theme::GOLD), 0.7),
            None,
            &Line::new((0.0, g.y), (w, g.y)),
        );
        label(
            scene,
            labels,
            &g.label,
            4.0,
            g.y - 3.0,
            9.0,
            text::Align::Left,
            color(theme::GOLD),
            at,
        );
    }
    for z in canvas::zone_guides(ed) {
        scene.stroke(
            &stroke_of(1.0),
            at,
            with_alpha(color(theme::ZONE), 0.8),
            None,
            &Line::new((z.x0, z.y), (z.x1, z.y)),
        );
    }
    for s in canvas::separators(ed) {
        scene.stroke(
            &stroke_of(2.0),
            at,
            color(s.color),
            None,
            &Line::new((s.x, s.tick_y - 6.0), (s.x, s.tick_y + 6.0)),
        );
    }
}

/// The take's waveform, and the sibilants inside it.
///
/// Both are audio-mode furniture: the waveform says where the sound is,
/// and the shaded bands say which spans an amplitude drag will hit —
/// "the dark areas in the waveform" the manual describes. A band over an
/// already-dark backdrop is nearly invisible, so it is edged as well as
/// filled.
fn audio(scene: &mut Scene, ed: &Editor, at: Affine) {
    if let Some(wave) = canvas::take_waveform(ed) {
        scene.fill(
            Fill::NonZero,
            at,
            with_alpha(color(theme::REFERENCE), 0.13),
            None,
            &polygon(&wave),
        );
    }
    if !ed.sibilant_scope {
        return;
    }
    for (sx, ex) in canvas::sibilant_bands(ed) {
        let r = Rect::new(sx, 0.0, sx + (ex - sx).max(1.0), ed.viewport.h);
        scene.fill(Fill::NonZero, at, with_alpha(Color::BLACK, 0.35), None, &r);
        scene.stroke(
            &stroke_of(1.0),
            at,
            with_alpha(color(theme::ACCENT), 0.5),
            None,
            &r,
        );
    }
}

/// The string roll's bend flow (#161 prototype).
///
/// The string's own line, lifted off its row by the bend, drawn thick
/// and in the string's colour so it reads as "the string moved" rather
/// than as an overlay. A guitarist reads bends as "full" and "half", so
/// the peak carries the number and the curve only shows how it got
/// there.
fn strings(scene: &mut Scene, ed: &Editor, at: Affine, overlay: &Overlay, labels: &mut Labeller) {
    if overlay.flow.on_row() {
        for f in crate::guitar::flow_paths(ed) {
            scene.stroke(
                &stroke_of(if f.selected { 3.5 } else { 2.5 }),
                at,
                with_alpha(color(f.color), 0.95),
                None,
                &polyline(&f.points),
            );
            if let Some(peak) = &f.peak_label {
                label(
                    scene,
                    labels,
                    peak,
                    f.peak_at.0 + 3.0,
                    f.peak_at.1 - 4.0,
                    9.0,
                    text::Align::Left,
                    color(theme::ACCENT),
                    at,
                );
            }
        }
    }
    // Joins between two notes on one string: a hammer-on gets an arc, a
    // slide a straight connector — deliberately two different marks.
    for j in crate::guitar::joins(ed) {
        if let Ok(path) = kurbo::BezPath::from_svg(&j.d) {
            scene.stroke(&stroke_of(1.5), at, color(j.color), None, &path);
        }
    }
}

/// An open pitch drawing: the line being drawn, what it is replacing,
/// and the anchors that shape it.
fn draft(scene: &mut Scene, overlay: &Overlay, at: Affine) {
    let Some(d) = &overlay.draft else { return };
    // The curve as it was before drawing began — the thin line
    // underneath, which is how you can see what you are changing.
    scene.stroke(
        &stroke_of(1.0),
        at,
        with_alpha(color(theme::TEXT_DIM), 0.6),
        None,
        &polyline(&d.original),
    );
    scene.stroke(
        &stroke_of(2.0),
        at,
        color(theme::ACCENT),
        None,
        &polyline(&d.line),
    );
    for (x, y) in &d.anchors {
        let r = 3.5;
        scene.fill(
            Fill::NonZero,
            at,
            color(theme::ACCENT),
            None,
            &kurbo::Circle::new((*x, *y), r),
        );
    }
}

/// The time selection: a span with no pitch.
///
/// Full height and unfilled but for a wash, which is what distinguishes
/// it from a razor at a glance. A razor is a *rectangle* — it has rows,
/// and what it holds is what it will carve. A time selection is two
/// points on the timeline and says nothing about pitch, so drawing it as
/// a box would claim a boundary it does not have.
fn time_selection(scene: &mut Scene, ed: &Editor, at: Affine, h: f64) {
    let Some((t0, t1)) = ed.time_selection else {
        return;
    };
    let (x0, x1) = (ed.camera.x(t0), ed.camera.x(t1));
    if x1 <= x0 {
        return;
    }
    scene.fill(
        Fill::NonZero,
        at,
        with_alpha(color(theme::SELECTED), 0.10),
        None,
        &Rect::new(x0, 0.0, x1, h),
    );
    // The edges carry the weight: a 10% wash is easy to lose over a
    // dense roll, and where the range *ends* is the thing being read.
    for x in [x0, x1] {
        scene.stroke(
            &stroke_of(1.0),
            at,
            with_alpha(color(theme::SELECTED), 0.8),
            None,
            &Line::new((x, 0.0), (x, h)),
        );
    }
}

fn razors(scene: &mut Scene, ed: &Editor, at: Affine, pending: Option<&RazorArea>) {
    for r in canvas::razor_rects(ed) {
        scene.fill(
            Fill::NonZero,
            at,
            with_alpha(color(theme::RAZOR), 0.25),
            None,
            &Rect::new(r.x, r.y, r.x + r.w, r.y + r.h),
        );
    }

    // The sweep in progress. Same fill so there is no question about
    // what it will become, plus a bright edge on the two sides that are
    // still moving — a razor is a *cut*, and where its boundaries fall
    // is the entire decision being made. Without the edges a snapped
    // rectangle and an unsnapped one look identical until you release.
    let Some(area) = pending else { return };
    let r = canvas::razor_rect(ed, area);
    let box_ = Rect::new(r.x, r.y, r.x + r.w, r.y + r.h);
    scene.fill(
        Fill::NonZero,
        at,
        with_alpha(color(theme::RAZOR), 0.22),
        None,
        &box_,
    );
    for x in [r.x, r.x + r.w] {
        scene.stroke(
            &stroke_of(1.5),
            at,
            with_alpha(color(theme::RAZOR), 0.95),
            None,
            &kurbo::Line::new((x, r.y), (x, r.y + r.h)),
        );
    }
    scene.stroke(
        &stroke_of(1.0),
        at,
        with_alpha(color(theme::RAZOR), 0.6),
        None,
        &box_,
    );
}

fn references(scene: &mut Scene, ed: &Editor, at: Affine) {
    let opacity = if ed.refs_to_front { 0.95 } else { 0.45 };
    for r in canvas::reference_rects(ed) {
        let box_ = Rect::new(r.x, r.y, r.x + r.w, r.y + r.h);
        // `None` is outline-only — `RefColor::Shadow`, a reference that
        // shows its shape without competing with the notes in front.
        if let Some(fill) = &r.fill {
            scene.fill(
                Fill::NonZero,
                at,
                with_alpha(color(fill), opacity * 0.5),
                None,
                &box_,
            );
        }
        scene.stroke(
            &stroke_of(1.0),
            at,
            with_alpha(color(&r.stroke), opacity),
            None,
            &box_,
        );
    }
    for r in canvas::midi_reference_rects(ed) {
        scene.stroke(
            &stroke_of(1.0),
            at,
            with_alpha(color(theme::TEXT_DIM), 0.8),
            None,
            &Rect::new(r.x, r.y, r.x + r.w, r.y + r.h),
        );
    }
}

fn notes(scene: &mut Scene, ed: &Editor, at: Affine, labels: &mut Labeller) {
    // Notes recede while a controller is being edited: the roll is that
    // dimension's editing surface for the moment, and full-strength
    // notes would compete with the curve for the same pixels.
    let dim = if ed.cc_editing() {
        ed.cc_display.note_dim
    } else {
        1.0
    };

    // Gathered by paint and emitted once each, rather than three
    // commands per note. Depth is preserved because each batch is a
    // layer: every body, then every outline over them.
    let mut bodies = Batch::default();
    let mut ribbons = Batch::default();
    let mut centres = Batch::default();
    let mut outlines = Batch::default();
    let mut thick = Batch::default();
    // Ambiguity bars. Their own batch because they are filled, not
    // stroked, and because they must land on top of both outlines.
    let mut warnings = Batch::default();
    let mut zones = Batch::default();
    let mut zones_active = Batch::default();
    // Labels are glyph runs and cannot be batched, so they are held back
    // and drawn after the bodies they sit on.
    let mut deferred: Vec<(String, f64, f64, f32, Color)> = Vec::new();

    for n in canvas::note_rects(ed) {
        let alpha = n.opacity * dim;
        let fill = with_alpha(color(n.fill), alpha);

        // A mode that draws sung blobs or struck heads draws those
        // *instead* of the bar, in that order of precedence.
        if let Some(blob) = &n.blob {
            bodies.add(fill, &polygon(blob));
            if let Some(cy) = n.blob_center {
                centres.add(
                    with_alpha(color(theme::TEXT), 0.5 * alpha),
                    &Line::new((n.x, cy), (n.x + n.w, cy)),
                );
            }
        } else if let Some(head) = &n.head {
            bodies.add(fill, &polygon(head));
        } else {
            let r = Rect::new(n.x, n.y, n.x + n.w, n.y + n.h);
            bodies.add(fill, &r);
            if let Some(ribbon) = &n.ribbon {
                ribbons.add(
                    with_alpha(color(theme::SELECTED), 0.18 * alpha),
                    &polygon(ribbon),
                );
            }
            // Zones, and which one a write would land on.
            for (x0, _x1, active) in &n.zones {
                let line = Line::new((*x0, n.y), (*x0, n.y + n.h));
                if *active {
                    zones_active.add(color(theme::ACCENT), &line);
                } else {
                    zones.add(color(theme::ZONE), &line);
                }
            }
            // The outline says one thing and one thing only: whether
            // this note is selected.
            //
            // Ambiguity used to claim it too, and won — so selecting a
            // red note appeared to do nothing, and on MPE material where
            // most notes are flagged the selection was invisible
            // wholesale. Two independent facts cannot share one channel;
            // one of them has to lose, and the one you are actively
            // changing must not be it.
            let edge = color(if n.selected {
                theme::SELECTED
            } else {
                theme::BORDER_STRONG
            });
            // Two widths, so two batches: a stroke width belongs to the
            // command, not to the path.
            if n.selected {
                thick.add(edge, &r);
            } else {
                outlines.add(edge, &r);
            }
            // Ambiguity gets its own mark instead: a red bar along the
            // top edge, which composes with any outline rather than
            // replacing it. It stays legible on a note only a few pixels
            // tall, which an inset stroke would not.
            if n.ambiguous {
                let bar = (n.h * 0.25).clamp(1.5, 3.0);
                warnings.add(
                    with_alpha(color(theme::ZONE), 0.95 * alpha),
                    &Rect::new(n.x, n.y, n.x + n.w, n.y + bar),
                );
            }
        }

        // What the body prints — note name, fret number, or lyric.
        //
        // Dark on the body, because these sit on a saturated fill and
        // light text on a yellow string is unreadable.
        //
        // Black rather than the panel's dark grey, and at full weight of
        // alpha, because the svg drew this at `font-weight: 600` and the
        // shaper here has only the regular face. At the seven pixels a
        // dense roll gives a note, a mid-grey regular is illegible where
        // a dark-grey semibold was fine.
        //
        // Only where the body can actually hold it. A ten-pixel label on
        // a four-pixel note is a smear rather than a name, and on a
        // dense roll that smear was also the single largest thing in the
        // frame — one glyph run per note, thousands of them, none of
        // them readable.
        if let Some(text_) = &n.label
            && n.h >= LABEL_MIN_H
            && n.w >= LABEL_MIN_W
        {
            deferred.push((
                text_.clone(),
                n.x + 5.0,
                n.y + n.h * 0.5 + 3.5,
                10.0,
                with_alpha(Color::BLACK, alpha),
            ));
        }
        if let Some(badge) = n.badge {
            deferred.push((
                badge.to_string(),
                n.x,
                n.y - 3.0,
                8.0,
                with_alpha(color(theme::TEXT_DIM), alpha),
            ));
        }
    }

    // Bodies, then what sits on them, then the outlines over both.
    bodies.fill(scene, at);
    ribbons.fill(scene, at);
    centres.stroke(scene, at, 1.0);
    zones.stroke(scene, at, 1.0);
    zones_active.stroke(scene, at, 2.0);
    outlines.stroke(scene, at, 1.0);
    thick.stroke(scene, at, 2.0);
    warnings.fill(scene, at);

    for (s, x, y, size, c) in deferred {
        label(scene, labels, &s, x, y, size, text::Align::Left, c, at);
    }

    handles(scene, ed, at);
}

/// How far a black key reaches across the gutter, as a fraction of it.
///
/// Roughly the ratio on a real keyboard, and the point is only that it is
/// clearly less than one: the white keys have to stay visibly continuous
/// underneath or the shape stops reading as a keyboard.
const BLACK_KEY_W: f64 = 0.62;

/// The smallest note body worth printing a name on.
///
/// Below this the glyphs overlap the rows above and below and read as
/// noise, so the name is simply not drawn — which is both what it looked
/// like it was doing anyway and, on a dense roll, most of the frame.
const LABEL_MIN_H: f64 = 9.0;
const LABEL_MIN_W: f64 = 18.0;

fn handles(scene: &mut Scene, ed: &Editor, at: Affine) {
    for set in canvas::note_handles(ed) {
        // The temporary-note range, when a drawing is open on this note.
        if let Some((x0, x1)) = set.scope {
            scene.stroke(
                &stroke_of(1.0),
                at,
                with_alpha(color(theme::ACCENT), 0.6),
                None,
                &Line::new((x0, 0.0), (x1, 0.0)),
            );
        }
        for h in &set.rects {
            let c = Rect::new(h.x, h.y, h.x + h.w, h.y + h.h);
            scene.fill(
                Fill::NonZero,
                at,
                with_alpha(color(theme::HANDLE), 0.9),
                None,
                &c,
            );
            scene.stroke(&stroke_of(1.0), at, color(theme::ACCENT), None, &c);
            // Each mark says what the handle *does* rather than naming
            // it. At fourteen pixels there is no room for a word, and a
            // shape is faster to read than one anyway.
            let (cx, cy) = (h.x + h.w * 0.5, h.y + h.h * 0.5);
            let half = h.w.min(h.h) * 0.5;
            let hollow =
                matches!(h.handle, expression_editor_core::Handle::Amplitude) && ed.sibilant_scope;
            if let Ok(mark) =
                kurbo::BezPath::from_svg(&crate::handle_mark(h.handle, cx, cy, half, hollow))
            {
                scene.stroke(&stroke_of(1.2), at, color(theme::TEXT), None, &mark);
            }
        }
    }
}

fn curves(scene: &mut Scene, ed: &Editor, at: Affine) {
    // Two widths, so two batches. Every curve of a given colour and
    // weight becomes one path — a pitch track per note is a thousand
    // strokes otherwise.
    let mut quiet = Batch::default();
    let mut active = Batch::default();
    for c in canvas::curve_paths(ed) {
        let path = line_of(&c.points);
        if path.is_empty() {
            continue;
        }
        if c.active {
            active.add(color(c.color), &path);
        } else {
            quiet.add(color(c.color), &path);
        }
    }
    quiet.stroke(scene, at, 1.5);
    active.stroke(scene, at, 2.5);
}

fn controllers(scene: &mut Scene, ed: &Editor, at: Affine, w: f64, h: f64, labels: &mut Labeller) {
    let _ = (w, h);
    for (i, c) in canvas::cc_paths(ed).into_iter().enumerate() {
        let fill = polygon(&c.fill);
        if !fill.is_empty() {
            scene.fill(
                Fill::NonZero,
                at,
                with_alpha(color(c.color), c.opacity * 0.25),
                None,
                &fill,
            );
        }
        let line = polyline(&c.points);
        if !line.is_empty() {
            scene.stroke(
                &stroke_of(if c.active { 2.0 } else { 1.0 }),
                at,
                with_alpha(color(c.color), c.opacity),
                None,
                &line,
            );
            // Stacked down the left edge, one line per lane: a pinned
            // lane spans the whole roll height, so there is no single
            // "its own" y to hang the name off.
            label(
                scene,
                labels,
                &c.label,
                3.0,
                12.0 + i as f64 * 11.0,
                9.0,
                text::Align::Left,
                with_alpha(color(c.color), c.opacity),
                at,
            );
        }
    }
}

fn playhead(scene: &mut Scene, ed: &Editor, at: Affine, h: f64) {
    let Some(t) = ed.playhead else { return };
    let x = ed.camera.x(t);
    scene.stroke(
        &stroke_of(2.0),
        at,
        color(theme::ACCENT),
        None,
        &Line::new((x, 0.0), (x, h)),
    );
}

fn keyboard(scene: &mut Scene, ed: &Editor, h: f64, labels: &mut Labeller) {
    let at = Affine::translate((0.0, canvas::RULER_H));

    // The keyboard occupies exactly the band the note area does.
    //
    // It used to be painted over the element's full height while the
    // rows only covered `vp.h`, so whenever those two disagreed the
    // piano ran on past the last row and left the note area beside it
    // blank. They are the same rows — `canvas::rows` and
    // `canvas::keyboard` iterate the same span — so any difference on
    // screen was the painting, not the geometry.
    //
    // Clipped as well as sized, so the half row at the top and bottom is
    // cut the way the rows themselves are.
    let band = Rect::new(0.0, 0.0, canvas::GUTTER_W, ed.viewport.h);
    scene.push_clip_layer(at, &band);
    let keys = canvas::keyboard(ed);

    // A pitch gutter is a *keyboard*; every other row space is a list of
    // named lanes.
    //
    // The difference is the whole reason this reads at a glance. Drawn
    // as equal full-width bands — which is what it was — a piano gutter
    // is a grey ladder you have to count, and the eye has nothing to
    // land on. Inset black keys give it the shape everyone already knows
    // how to read, so "which octave am I in" stops being a question.
    // Drum and string spaces keep the full-width bands, because there
    // the rows genuinely are a list and a piano would be a lie.
    let piano = matches!(ed.row_space, expression_editor_core::RowSpace::Pitch);

    // The white keys are one continuous bed, and the black ones sit *on*
    // it. Painting each key as its own band instead leaves the strip
    // beside an accidental showing the panel behind, which is what made
    // the gutter look like a table rather than an instrument.
    scene.fill(
        Fill::NonZero,
        at,
        color(if piano {
            theme::KEY_WHITE
        } else {
            theme::SURFACE_BAR
        }),
        None,
        &band,
    );

    let mut faces = Batch::default();
    let mut edges = Batch::default();
    for k in &keys {
        if piano {
            if k.black {
                faces.add(
                    color(theme::KEY_BLACK),
                    &Rect::new(0.0, k.y, canvas::GUTTER_W * BLACK_KEY_W, k.y + k.h),
                );
            } else {
                // Only the white keys are parted. A rule drawn across a
                // black key is exactly what made this read as a grid.
                edges.add(
                    with_alpha(color(theme::PANEL_BORDER), 0.8),
                    &Line::new((0.0, k.y), (canvas::GUTTER_W, k.y)),
                );
            }
        } else {
            faces.add(
                color(if k.black {
                    theme::KEY_BLACK
                } else {
                    theme::KEY_WHITE
                }),
                &Rect::new(0.0, k.y, canvas::GUTTER_W, k.y + k.h),
            );
            edges.add(
                with_alpha(color(theme::PANEL_BORDER), 0.8),
                &Line::new((0.0, k.y), (canvas::GUTTER_W, k.y)),
            );
        }
    }
    faces.fill(scene, at);
    edges.stroke(scene, at, 0.5);

    for k in &keys {
        // Only C rows are labelled, so the gutter stays readable when
        // the rows get short — and only when the row can hold the text.
        if let Some(name) = &k.label
            && k.h >= 9.0
        {
            label(
                scene,
                labels,
                name,
                canvas::GUTTER_W - 4.0,
                k.y + k.h * 0.5 + 3.0,
                9.0,
                text::Align::Right,
                color(theme::KEY_LABEL),
                at,
            );
        }
    }

    // Braces over the rows of a split piece, so `L` and `R` read as two
    // hands of one instrument rather than two unrelated lanes.
    for g in canvas::key_groups(ed, &canvas::keyboard(ed)) {
        label(
            scene,
            labels,
            &g.label,
            3.0,
            g.y + g.h * 0.5 + 3.0,
            9.0,
            text::Align::Left,
            color(theme::TEXT_DIM),
            at,
        );
    }
    scene.pop_layer();

    // The gutter's right edge, which is also the roll's left edge. Drawn
    // over the whole element, ruler included, because it separates two
    // columns rather than bounding the keys.
    scene.stroke(
        &stroke_of(1.0),
        Affine::IDENTITY,
        color(theme::PANEL_BORDER),
        None,
        &Line::new((canvas::GUTTER_W, 0.0), (canvas::GUTTER_W, h)),
    );
}

fn ruler(scene: &mut Scene, ed: &Editor, w: f64, labels: &mut Labeller) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color(theme::SURFACE_BAR),
        None,
        &Rect::new(0.0, 0.0, w, canvas::RULER_H),
    );
    let at = Affine::translate((canvas::GUTTER_W, 0.0));
    for t in canvas::ruler(ed) {
        let top = if t.bar { 6.0 } else { canvas::RULER_H - 7.0 };
        scene.stroke(
            &stroke_of(1.0),
            at,
            color(if t.bar {
                theme::TEXT_DIM
            } else {
                theme::GRID_SUB
            }),
            None,
            &Line::new((t.x, top), (t.x, canvas::RULER_H)),
        );
        if let Some(n) = &t.label {
            label(
                scene,
                labels,
                n,
                t.x + 3.0,
                11.0,
                9.0,
                text::Align::Left,
                color(theme::TEXT_DIM),
                at,
            );
        }
    }
    for m in canvas::markers(ed) {
        scene.fill(
            Fill::NonZero,
            at,
            color(theme::GOLD),
            None,
            &Rect::new(m.x, 2.0, m.x + 6.0, 8.0),
        );
        label(
            scene,
            labels,
            &m.label,
            m.x + 8.0,
            canvas::RULER_H - 6.0,
            9.0,
            text::Align::Left,
            color(theme::GOLD),
            at,
        );
    }
    scene.stroke(
        &stroke_of(1.0),
        Affine::IDENTITY,
        color(theme::PANEL_BORDER),
        None,
        &Line::new((0.0, canvas::RULER_H), (w, canvas::RULER_H)),
    );
    let _ = Dimension::Pitch;
}
