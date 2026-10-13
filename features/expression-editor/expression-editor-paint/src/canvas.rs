//! Canvas geometry — the SVG the piano roll is drawn from.
//!
//! Pure functions over the editor state, returning ready-to-render
//! primitives. Keeping them out of the rsx means the layout can be
//! asserted in tests without mounting a DOM, and it keeps the component
//! body about events rather than arithmetic.

use expression_editor_core::doc::{Dimension, Note, NoteId};
use expression_editor_core::handles;
use expression_editor_core::tools;
use expression_editor_core::{Editor, RefColor};

use crate::theme;

/// A polyline, in pixels.
///
/// Numbers, not the `"x,y x,y "` string these used to be. That string
/// was an svg `points` attribute, and once the roll was painted rather
/// than emitted the painter had to parse it straight back — 577 KB of
/// text formatted and re-read *per frame* on a wide display, which was
/// half the cost of building a frame.
pub type Points = Vec<(f64, f64)>;

/// Format a polyline as an svg `points` attribute.
///
/// For the surfaces that are still svg — the lane strip. The roll does
/// not go near this.
pub fn points_attr(points: &[(f64, f64)]) -> String {
    let mut s = String::with_capacity(points.len() * 12);
    for (x, y) in points {
        s.push_str(&format!("{x:.1},{y:.1} "));
    }
    s
}

/// A piano-roll background row.
pub struct Row {
    pub row: i32,
    pub y: f64,
    pub h: f64,
    pub fill: &'static str,
    pub is_c: bool,
    /// This row opens a new part of the kit — draw a divider above it.
    ///
    /// One line per group rather than per row: thirty-nine evenly-ruled
    /// lanes give the eye nothing to steer by, which is the whole
    /// problem banding solves.
    pub starts_group: bool,
}

/// What colour a note body is.
///
/// On a string roll the row is a pitch and several strings reach it, so
/// the colour has to come from the note — `row_color` returns `None`
/// there for exactly that reason, and going straight to it is what made
/// the "By string" toggle do nothing on the main canvas.
fn note_color(ed: &Editor, n: &expression_editor_core::Note) -> &'static str {
    match (&ed.row_space, n.string) {
        (expression_editor_core::RowSpace::Strings(_), Some(s)) if ed.color_by_string => {
            expression_editor_core::rows::string_color(s)
        }
        _ => ed
            .row_space
            .row_color(n.row)
            .unwrap_or_else(|| theme::pitch_class_color(n.row)),
    }
}

/// Background rows across the visible pitch span.
pub fn rows(ed: &Editor) -> Vec<Row> {
    let (lo, hi) = ed.camera.slot_span(ed.viewport);
    let h = ed.camera.vertical.px_per_row;
    // Clamped to the row space's own bounds, not to 0..127. A six-string
    // roll has six rows and painting a hundred and twenty-two phantom
    // ones above and below them says there is somewhere else to put a
    // note, which there is not.
    //
    // Iteration is in *slots*, so a folded piece is drawn once.
    let (rlo, rhi) = ed.camera.fold.slot_bounds(ed.row_space.bounds());
    ((lo.floor() as i32).max(rlo)..=(hi.ceil() as i32).min(rhi))
        .map(|slot| ed.camera.fold.row(slot as f64) as i32)
        .map(|row| Row {
            row,
            y: ed.camera.y(row as f64 + 0.5, ed.viewport),
            h,
            // A drum roll bands by family; everything else keeps the
            // keyboard's black/white, which is the thing a pitch roll
            // is already navigated by.
            fill: ed.row_space.row_background(row).unwrap_or_else(|| {
                if theme::is_black_key(row) {
                    theme::ROW_BLACK
                } else {
                    theme::ROW_WHITE
                }
            }),
            is_c: row.rem_euclid(12) == 0,
            starts_group: ed.row_space.starts_group(row),
        })
        .collect()
}

/// A vertical gridline.
pub struct GridLine {
    pub x: f64,
    pub beat: bool,
}

/// Gridlines at the editor's own local grid — never the project grid.
pub fn grid_lines(ed: &Editor) -> Vec<GridLine> {
    let step = ed.grid.step(ed.units_per_beat());
    let beat = ed.units_per_beat();
    if step <= 0.0 {
        return Vec::new();
    }
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    // Bail out rather than emit thousands of invisible lines when zoomed
    // far out.
    if (t1 - t0) / step > 512.0 {
        return Vec::new();
    }
    let first = ((t0 - ed.doc.start) / step).floor() * step + ed.doc.start;
    let mut out = Vec::new();
    let mut t = first;
    while t <= t1 {
        if t >= t0 {
            let on_beat = ((t - ed.doc.start) / beat).fract().abs() < 1e-6;
            out.push(GridLine {
                x: ed.camera.x(t),
                beat: on_beat,
            });
        }
        t += step;
    }
    out
}

/// A note rectangle ready to draw.
pub struct NoteRect {
    pub id: NoteId,
    pub row: i32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub fill: &'static str,
    pub opacity: f64,
    pub selected: bool,
    pub ambiguous: bool,
    /// Zone spans in pixels, and whether each is the active target.
    pub zones: Vec<(f64, f64, bool)>,
    /// Sounding detune in cents from the row's tuned center.
    pub cents: Option<f64>,
    /// Amplitude ribbon polygon, when Pressure has been authored.
    pub ribbon: Option<String>,
    /// What the body prints — note name, fret number, or lyric.
    pub label: Option<String>,
    /// Articulation badge, drawn above the note.
    pub badge: Option<&'static str>,
    /// Triangle points, when this space draws heads instead of bars.
    pub head: Option<String>,
    /// Amplitude-blob polygon, when this mode draws sung notes instead
    /// of rectangles. Takes precedence over both `head` and the bar.
    pub blob: Option<String>,
    /// Y of the blob's centre line — the note's own pitch, drawn as a
    /// hairline through the body so the pitch track can be read against
    /// it. `None` when there is no blob.
    pub blob_center: Option<f64>,
    /// Joined to the next note on its row.
    pub legato: bool,
}

/// The handle set for one note, positioned.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteHandles {
    pub id: NoteId,
    pub rects: Vec<handles::HandleRect>,
    /// The temporary-note range in pixels, when one is open here.
    pub scope: Option<(f64, f64)>,
}

/// Handles for the notes that should carry them.
///
/// Only the selected notes, and only in a mode whose notes have a
/// contour to shape. Handles on every note at once is a wall of
/// targets — Vovious draws them on what you are working on.
pub fn note_handles(ed: &Editor) -> Vec<NoteHandles> {
    if !ed.mode.has_handles() {
        return Vec::new();
    }
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let h = ed.camera.vertical.px_per_row;
    ed.doc
        .notes
        .iter()
        .filter(|n| n.end >= t0 && n.start <= t1)
        .filter(|n| ed.selection.notes.contains(&n.id))
        .map(|n| {
            let raw_x = ed.camera.x(n.start);
            let raw_end = ed.camera.x(n.end);
            // Lay the handles out over the note's *visible* extent, not
            // its true one. A note longer than the viewport — routine
            // for a held vocal at working zoom — would otherwise put
            // its right-hand handles off-screen and leave three of the
            // seven unreachable until you scrolled.
            let x = raw_x.max(0.0);
            let w = (raw_end.min(ed.viewport.w) - x).max(2.0);
            let y = ed.camera.y(n.row as f64 + 0.5, ed.viewport);
            let scope = match ed.scope_for(n.id) {
                handles::Scope::Range { t0, t1 } => Some((ed.camera.x(t0), ed.camera.x(t1))),
                handles::Scope::Note => None,
            };
            NoteHandles {
                id: n.id,
                rects: handles::layout(x, y, w, h),
                scope,
            }
        })
        .collect()
}

/// A note belonging to another track, drawn behind the active one.
///
/// Deliberately not a [`NoteRect`]: a reference has no zones, no
/// ribbon, no cents readout and no selection state, and giving it those
/// fields would invite code that treats it as editable.
#[derive(Clone, Debug, PartialEq)]
pub struct RefRect {
    /// Which track it came from, so a double-click can promote it.
    pub track: usize,
    pub id: NoteId,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub stroke: String,
    /// `None` for outline-only (`RefColor::Shadow`).
    pub fill: Option<String>,
    pub label: Option<String>,
}

/// Notes from every track marked as a reference.
///
/// Ordered back-to-front by track index so the overlay is stable: a
/// reference that jumped in front of another when you switched tracks
/// would be unreadable.
pub fn reference_rects(ed: &Editor) -> Vec<RefRect> {
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let h = ed.camera.vertical.px_per_row;
    let mut out = Vec::new();
    for (i, track) in ed.tracks.references() {
        let Some(doc) = ed.tracks.doc_of(i) else {
            continue;
        };
        // Host colour when asked for and available; the surface's own
        // reference grey otherwise, so a host that supplies no colours
        // still renders something legible.
        let colour = match track.ref_color {
            RefColor::Host => track
                .color
                .clone()
                .unwrap_or_else(|| theme::REFERENCE.into()),
            RefColor::Default | RefColor::Shadow => theme::REFERENCE.into(),
        };
        let fill = (track.ref_color != RefColor::Shadow).then(|| colour.clone());
        for n in doc.notes.iter().filter(|n| n.end >= t0 && n.start <= t1) {
            let x = ed.camera.x(n.start);
            out.push(RefRect {
                track: i,
                id: n.id,
                x,
                w: (ed.camera.x(n.end) - x).max(2.0),
                y: ed.camera.y(n.row as f64 + 0.5, ed.viewport),
                h,
                stroke: colour.clone(),
                fill: fill.clone(),
                label: n.text.clone(),
            });
        }
    }
    out
}

pub fn note_rects(ed: &Editor) -> Vec<NoteRect> {
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let h = ed.camera.vertical.px_per_row;
    ed.doc
        .notes
        .iter()
        .filter(|n| n.end >= t0 && n.start <= t1)
        .map(|n| {
            let x = ed.camera.x(n.start);
            let w = (ed.camera.x(n.end) - x).max(2.0);
            let active_zone = match n.target {
                expression_editor_core::Target::WholeNote => None,
                expression_editor_core::Target::Zone(i) => Some(i),
            };
            let zones = n
                .zones()
                .iter()
                .enumerate()
                .map(|(i, &(a, b))| {
                    // Either exactly one segment is highlighted, or
                    // every one is — never a partial state.
                    let active = active_zone.is_none_or(|z| z == i);
                    (ed.camera.x(a), ed.camera.x(b), active)
                })
                .collect();
            let cents = sounding_cents(ed, n);
            // The label comes from the row space: a note name in pitch
            // space, a fret number on a string roll, and a lyric
            // whenever the note carries one.
            let label = ed.row_space.note_label(n).filter(|l| {
                // A fret number always fits; a word may not, and a
                // clipped lyric is worse than none.
                l.len() <= 2 || (w > l.len() as f64 * 6.5 + 8.0 && h >= 12.0)
            });
            let badge = n.articulation.map(|a| a.glyph()).filter(|g| !g.is_empty());
            let head = matches!(
                ed.row_space.note_shape(),
                expression_editor_core::NoteShape::Triangle
            )
            .then(|| {
                // Flat edge on the onset, apex to the right: the attack
                // is a straight vertical line you can align to the grid
                // by eye, which is the whole reason for a head over a
                // diamond.
                let size = h.clamp(5.0, 18.0);
                let top = ed.camera.y(n.row as f64 + 0.5, ed.viewport) + (h - size) * 0.5;
                format!(
                    "{x:.1},{top:.1} {x:.1},{:.1} {:.1},{:.1}",
                    top + size,
                    x + size * 0.9,
                    top + size * 0.5,
                )
            });
            let blob_shape = ed.mode.draws_blobs().then(|| blob_polygon(ed, n)).flatten();
            // A string roll colours by string, a kit by section; pitch
            // space keeps its pitch-class hue. A sung note instead
            // colours by how far out of tune it is, which is the one
            // thing you are looking for on this surface — pitch class
            // is already the row.
            let fill = if blob_shape.is_some() {
                theme::tune_color(blob_cents(ed, n))
            } else {
                note_color(ed, n)
            };
            NoteRect {
                id: n.id,
                row: n.row,
                x,
                y: ed.camera.y(n.row as f64 + 0.5, ed.viewport),
                w,
                h,
                fill,
                opacity: 0.35 + 0.45 * n.weight.clamp(0.0, 1.0),
                selected: ed.selection.contains(n.id),
                ambiguous: n.ambiguous,
                zones,
                cents,
                // The blob already *is* the amplitude envelope, so the
                // ribbon would draw the same information twice — and
                // draw it in the wrong place, since the ribbon sits in
                // the row while the blob rides the pitch.
                ribbon: if blob_shape.is_some() {
                    None
                } else {
                    note_ribbon(ed, n)
                },
                label,
                badge,
                head,
                blob_center: blob_shape.as_ref().map(|_| blob_center_y(ed, n)),
                blob: blob_shape,
                legato: n.legato,
            }
        })
        .collect()
}

/// Samples across a blob body. Enough that a vibrato reads as a wave
/// rather than a zigzag, few enough that a screen of notes stays cheap.
const BLOB_SAMPLES: usize = 48;

/// A timing separator, positioned.
#[derive(Clone, Debug, PartialEq)]
pub struct SeparatorLine {
    pub sep: expression_editor_core::Separator,
    pub x: f64,
    /// Y of the tick that splits the two drag laws.
    pub tick_y: f64,
    /// Colour by how far off the beat the boundary sits.
    pub color: &'static str,
}

/// Where the tick sits on a separator, as a fraction of roll height.
pub const SEPARATOR_TICK: f64 = 0.5;
/// How close to a separator counts as grabbing it.
pub const SEPARATOR_GRAB_PX: f64 = 6.0;

/// Separators for the visible boundaries.
///
/// Only in timing mode: a full-height line at every note join would be
/// a picket fence across a screen that is otherwise about pitch.
pub fn separators(ed: &Editor) -> Vec<SeparatorLine> {
    if !ed.timing_mode {
        return Vec::new();
    }
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let step = ed.grid.step(ed.units_per_beat());
    expression_editor_core::timing::separators(&ed.doc, ed.camera.units_per_px * 2.0)
        .into_iter()
        .filter(|s| s.t >= t0 && s.t <= t1)
        .map(|sep| {
            // Deviation as a fraction of half a division: dead on at
            // zero, worst when it sits between two divisions.
            let dev = expression_editor_core::timing::beat_deviation(sep.t, ed.doc.start, step);
            let off = if step > 0.0 {
                (dev.abs() / (step * 0.5)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            SeparatorLine {
                sep,
                x: ed.camera.x(sep.t),
                tick_y: ed.viewport.h * SEPARATOR_TICK,
                // The same ramp the notes use, so "off" means the same
                // thing everywhere on this surface — just measured
                // against the beat instead of against a pitch.
                color: theme::TUNE_RAMP[(off * 4.0).round() as usize % 5],
            }
        })
        .collect()
}

/// A MIDI reference note, positioned.
#[derive(Clone, Debug, PartialEq)]
pub struct RefNoteRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// The loaded MIDI reference, as outlines behind the sung notes.
pub fn midi_reference_rects(ed: &Editor) -> Vec<RefNoteRect> {
    let Some(r) = ed.reference.as_ref() else {
        return Vec::new();
    };
    if !r.visible || r.is_empty() {
        return Vec::new();
    }
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let h = ed.camera.vertical.px_per_row;
    r.notes()
        .filter(|n| n.end >= t0 && n.start <= t1)
        .map(|n| {
            let x = ed.camera.x(n.start);
            RefNoteRect {
                x,
                w: (ed.camera.x(n.end) - x).max(2.0),
                y: ed.camera.y(n.row as f64 + 0.5, ed.viewport),
                h,
            }
        })
        .collect()
}

/// A pitch drawing, ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct DraftView {
    /// Anchor handles, in pixels.
    pub anchors: Vec<(f64, f64)>,
    /// The drawn line.
    pub line: String,
    /// The curve as it was before drawing began — the thin line
    /// underneath, which is how you can see what you are changing.
    pub original: String,
}

/// Geometry for an open pitch drawing.
pub fn draft_view(ed: &Editor, draft: &expression_editor_core::PitchDraft) -> DraftView {
    let row = ed
        .doc
        .note(draft.note)
        .map(|n| n.row as f64)
        .unwrap_or(60.0);
    let to_px = |p: &expression_editor_core::Point| {
        (ed.camera.x(p.t), ed.camera.y(row + p.value, ed.viewport))
    };
    let polyline = |pts: &[expression_editor_core::Point]| {
        pts.iter()
            .map(|p| {
                let (x, y) = to_px(p);
                format!("{x:.1},{y:.1}")
            })
            .collect::<Vec<_>>()
            .join(" ")
    };

    let span = draft.dirty_span();
    let line = match span {
        Some((t0, t1)) => polyline(&draft.rendered(t0, t1)),
        None => String::new(),
    };
    DraftView {
        anchors: draft.anchors().iter().map(to_px).collect(),
        line,
        original: polyline(draft.original()),
    }
}

/// The take's own waveform, drawn full-height behind the roll.
///
/// Everything *between* the notes — breaths, consonants, room tone — as
/// itself rather than as absence. That is the difference between "the
/// tracker missed a note here" and "nothing was sung here", and without
/// it the two look identical: empty canvas.
///
/// Returns the polygon points, or `None` when the take carries no
/// waveform (every domain except audio).
pub fn take_waveform(ed: &Editor) -> Option<String> {
    if ed.doc.peaks.is_empty() || !ed.mode.draws_blobs() {
        return None;
    }
    let peaks = &ed.doc.peaks;
    let span = ed.doc.end - ed.doc.start;
    if span <= 0.0 {
        return None;
    }
    // Mirrored about the middle of the roll, which is where a waveform
    // display puts zero. It is a backdrop, not a pitch reading, so it
    // deliberately does *not* follow any note.
    let mid = ed.viewport.h * 0.5;
    let max_half = ed.viewport.h * 0.46;

    let count = (ed.viewport.w.ceil() as usize).clamp(2, 2048);
    let mut top = String::new();
    let mut bottom = Vec::with_capacity(count);
    for i in 0..count {
        let x = ed.viewport.w * (i as f64 / (count - 1) as f64);
        let t = ed.camera.t_at(x);
        let f = ((t - ed.doc.start) / span).clamp(0.0, 1.0);
        let idx = ((f * (peaks.len() - 1) as f64).round() as usize).min(peaks.len() - 1);
        let half = max_half * (peaks[idx] as f64).clamp(0.0, 1.0);
        if i > 0 {
            top.push(' ');
        }
        top.push_str(&format!("{:.1},{:.1}", x, mid - half));
        bottom.push(format!("{:.1},{:.1}", x, mid + half));
    }
    bottom.reverse();
    Some(format!("{top} {}", bottom.join(" ")))
}

/// Unvoiced spans as pixel rectangles, for shading sibilants.
///
/// Only while sibilant editing is armed. Shading them all the time
/// would put dark bars across a screen that is mostly about pitch.
pub fn sibilant_bands(ed: &Editor) -> Vec<(f64, f64)> {
    if !ed.sibilant_scope || !ed.mode.draws_blobs() {
        return Vec::new();
    }
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    ed.doc
        .unvoiced
        .iter()
        .filter(|(a, b)| *b >= t0 && *a <= t1)
        .map(|(a, b)| (ed.camera.x(*a), ed.camera.x(*b)))
        .collect()
}

/// A sung note's body: the recorded waveform, drawn as the note.
///
/// This is what makes the audio surface read as audio rather than as
/// MIDI with different colours. The body sits **flat**, centred on the
/// note's own pitch the way a MIDI note sits on its row, and its
/// vertical extent is the amplitude envelope mirrored about that
/// centre — a waveform, not a smooth ribbon. A breathy tail visibly
/// thins, a hard consonant bulges, and the shape of the sound is
/// legible before you read anything else.
///
/// The pitch contour is deliberately **not** folded into this. It is
/// drawn separately as the white pitch track, which wanders through and
/// out of the body — and that relationship, between where the note is
/// and where the voice actually went, is the thing being edited.
///
/// Returns the polygon points, or `None` where the note is too small to
/// be worth the samples.
fn blob_polygon(ed: &Editor, n: &Note) -> Option<String> {
    let x0 = ed.camera.x(n.start);
    let x1 = ed.camera.x(n.end);
    if x1 - x0 < 2.0 {
        return None;
    }
    let row_h = ed.camera.vertical.px_per_row;
    // Half a row at full amplitude, with a floor so a quiet passage
    // still leaves something to aim at rather than a hairline.
    let max_half = (row_h * 0.5).max(3.0);
    let min_half = (row_h * 0.08).max(1.0);
    let cy = blob_center_y(ed, n);

    // One sample per pixel where the note is wide, so the recorded
    // envelope's jaggedness survives instead of being smoothed into a
    // lozenge by too few samples.
    let count = ((x1 - x0).ceil() as usize).clamp(BLOB_SAMPLES, 1024);

    let mut top = String::new();
    let mut bottom = Vec::with_capacity(count);
    for i in 0..count {
        let f = i as f64 / (count - 1) as f64;
        let t = n.start + (n.end - n.start) * f;
        let half = min_half + (max_half - min_half) * note_amplitude(n, f, t);
        let x = ed.camera.x(t);
        if i > 0 {
            top.push(' ');
        }
        top.push_str(&format!("{:.1},{:.1}", x, cy - half));
        bottom.push(format!("{:.1},{:.1}", x, cy + half));
    }
    // Down the top edge, back along the bottom: one closed polygon.
    bottom.reverse();
    Some(format!("{top} {}", bottom.join(" ")))
}

/// Where a sung note's body is centred, in pixels.
///
/// The note's own pitch — its row plus the contour's centre — so the
/// body sits where the note *is*, and the pitch track can be read
/// against it.
fn blob_center_y(ed: &Editor, n: &Note) -> f64 {
    let row_h = ed.camera.vertical.px_per_row;
    let center = expression_editor_core::blob::decompose(
        &n.pitch,
        n.start,
        n.end,
        BLOB_SAMPLES,
        ed.doc.time_base.units_per_second(ed.bpm),
        0.0,
    )
    .center;
    ed.camera.y(n.row as f64 + center + 0.5, ed.viewport) + row_h * 0.5
}

/// Amplitude at fraction `f` through the note, 0..1.
///
/// The recorded envelope where there is one — that is the waveform, and
/// no curve of control points could carry its detail. Authored Pressure
/// where there is not, so an MPE note still shapes. Analysis weight as
/// the last resort, so a freshly-loaded note is not a hairline.
fn note_amplitude(n: &Note, f: f64, t: f64) -> f64 {
    if !n.envelope.is_empty() {
        let i = ((f * (n.envelope.len() - 1) as f64).round() as usize).min(n.envelope.len() - 1);
        return (n.envelope[i] as f64).clamp(0.0, 1.0);
    }
    if !n.pressure.is_empty() {
        return n
            .pressure
            .sample(t, Dimension::Pressure.default_value())
            .clamp(0.0, 1.0);
    }
    n.weight.clamp(0.0, 1.0)
}

/// How far a blob sits from its target, in cents.
///
/// The *centre* of the contour, not its value at any instant — a note
/// is in or out of tune as a whole, and colouring it from a point that
/// the vibrato happens to be passing through would make a perfectly
/// good note flash red twice a second.
fn blob_cents(ed: &Editor, n: &Note) -> f64 {
    let center = expression_editor_core::blob::decompose(
        &n.pitch,
        n.start,
        n.end,
        BLOB_SAMPLES,
        ed.doc.time_base.units_per_second(ed.bpm),
        0.0,
    )
    .center;
    let target = ed.tuning.cents(n.row) / 100.0;
    (center - target) * 100.0
}

/// How far the note actually sounds from 12-TET, if it does.
///
/// Read from the pitch curve, not the tuning table: a note is off-ET
/// because of what was drawn or sung, and the badge should follow the
/// audible truth.
fn sounding_cents(ed: &Editor, n: &Note) -> Option<f64> {
    // Only where it is being asked about: the selected note always, and
    // otherwise only when a non-equal tuning makes the row's own center
    // ambiguous. A badge on every note is a wall of numbers.
    if !ed.selection.contains(n.id) && ed.tuning.temperament.is_equal() {
        return None;
    }
    // Measured against the row's tuned center, not raw 12-TET, so under
    // a temperament the badge reads "off the target" rather than
    // restating the temperament.
    let center_offset = ed.tuning.cents(n.row) / 100.0;
    let mid = (n.start + n.end) * 0.5;
    let cents = (n.pitch.sample(mid, 0.0) - center_offset) * 100.0;
    (cents.abs() > 3.0).then_some(cents)
}

/// The note body's amplitude ribbon: the Pressure curve drawn as a
/// filled shape inside the note rectangle.
///
/// This is what makes a note read as a *blob* rather than a bar — you
/// can see where it swells and where it dies without switching lanes.
pub fn note_ribbon(ed: &Editor, n: &Note) -> Option<String> {
    if n.pressure.is_empty() {
        return None;
    }
    let top = ed.camera.y(n.row as f64 + 0.5, ed.viewport);
    let h = ed.camera.vertical.px_per_row;
    if h < 6.0 {
        return None;
    }
    let mid = top + h * 0.5;
    let pts: Vec<(f64, f64)> = n
        .pressure
        .points()
        .iter()
        .map(|p| (ed.camera.x(p.t), p.value.clamp(0.0, 1.0)))
        .collect();
    if pts.len() < 2 {
        return None;
    }
    // Symmetric about the note's centre line: out along the top, back
    // along the bottom, closed.
    let mut s = String::new();
    for &(x, v) in &pts {
        s.push_str(&format!("{:.1},{:.1} ", x, mid - h * 0.46 * v));
    }
    for &(x, v) in pts.iter().rev() {
        s.push_str(&format!("{:.1},{:.1} ", x, mid + h * 0.46 * v));
    }
    Some(s)
}

/// A rendered expression curve.
pub struct CurvePath {
    pub note: NoteId,
    pub dimension: Dimension,
    pub points: Points,
    pub color: &'static str,
    pub active: bool,
    pub selected: bool,
}

/// Polyline paths for every visible note on every drawn dimension, back to
/// front.
///
/// Pitch uses the full piano-roll height; Pressure and Timbre are
/// confined to a fixed box one semitone above and below the note, so
/// their normalized shape reads the same at every zoom level.
pub fn curve_paths(ed: &Editor) -> Vec<CurvePath> {
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let mut out = Vec::new();
    // On a string roll the pitch dimension belongs to `guitar::flow_paths`,
    // which draws it as the string lifting off its row rather than as a
    // thin expression polyline. Drawing both would double the line.
    let strings = matches!(ed.row_space, expression_editor_core::RowSpace::Strings(_));
    for dimension in ed.draw_order() {
        if strings && dimension == Dimension::Pitch {
            continue;
        }
        let active = dimension == ed.dimension;
        for n in ed.doc.notes.iter().filter(|n| n.end >= t0 && n.start <= t1) {
            let curve = n.curve(dimension);
            if curve.is_empty() {
                continue;
            }
            // The pitch track stops where there was no pitch. Drawing a
            // line across a consonant invents a reading, and a line
            // through a sibilant is the single most misleading thing
            // this surface could show — the manual makes the same
            // point: no pitch track *means* unvoiced.
            let break_unvoiced = dimension == Dimension::Pitch && ed.mode.draws_blobs();
            // A curve is in semitones; a row is only a semitone in pitch
            // space. See `RowSpace::semitones_per_row`.
            let spr = ed.row_space.semitones_per_row();
            let mut s: Points = Vec::new();
            let mut prev_voiced = true;
            for p in curve.points() {
                if break_unvoiced && is_unvoiced(ed, p.t) {
                    prev_voiced = false;
                    continue;
                }
                let x = ed.camera.x(p.t);
                let y = match dimension {
                    Dimension::Pitch => ed.camera.y(n.row as f64 + p.value / spr, ed.viewport),
                    _ => tools::lane_box_y(&ed.camera, ed.viewport, n.row, p.value),
                };
                // A polyline cannot express a gap, so a run that
                // resumes after silence starts a new path rather than
                // reaching back across it.
                if !prev_voiced && !s.is_empty() {
                    out.push(CurvePath {
                        note: n.id,
                        dimension,
                        points: core::mem::take(&mut s),
                        color: track_color(ed, dimension),
                        active,
                        selected: ed.selection.contains(n.id),
                    });
                }
                prev_voiced = true;
                s.push((x, y));
            }
            if s.is_empty() {
                continue;
            }
            out.push(CurvePath {
                note: n.id,
                dimension,
                points: s,
                color: track_color(ed, dimension),
                active,
                selected: ed.selection.contains(n.id),
            });
        }
    }
    out
}

/// Whether `t` falls in a span with no detected pitch.
fn is_unvoiced(ed: &Editor, t: f64) -> bool {
    ed.doc.unvoiced.iter().any(|(a, b)| t >= *a && t <= *b)
}

/// On the audio surface the pitch track is white, not the dimension's hue.
///
/// It is the one line that has to stay legible over a body whose colour
/// is already saying something else — how far out of tune the note is —
/// and a coloured track competes with that reading.
fn track_color(ed: &Editor, dimension: Dimension) -> &'static str {
    if dimension == Dimension::Pitch && ed.mode.draws_blobs() {
        theme::PITCH_TRACK
    } else {
        theme::lane_color(dimension)
    }
}

/// The editing box Pressure and Timbre are drawn inside.
pub struct LaneBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub fn lane_boxes(ed: &Editor) -> Vec<LaneBox> {
    if ed.dimension == Dimension::Pitch {
        return Vec::new();
    }
    ed.doc
        .notes
        .iter()
        .filter(|n| ed.selection.contains(n.id))
        .map(|n| {
            let top = ed.camera.y(n.row as f64 + 1.0, ed.viewport);
            let bottom = ed.camera.y(n.row as f64 - 1.0, ed.viewport);
            let x = ed.camera.x(n.start);
            LaneBox {
                x,
                y: top,
                w: (ed.camera.x(n.end) - x).max(2.0),
                h: bottom - top,
            }
        })
        .collect()
}

/// A microtonal center guide: where a row actually sounds under the
/// active temperament.
pub struct TuningGuide {
    pub y: f64,
    pub cents: f64,
    pub label: String,
}

/// Gold center lines for every visible row whose tuned center differs
/// from 12-TET.
pub fn tuning_guides(ed: &Editor) -> Vec<TuningGuide> {
    if ed.tuning.temperament.is_equal() {
        return Vec::new();
    }
    let (lo, hi) = ed.camera.pitch_span(ed.viewport);
    ((lo.floor() as i32).max(0)..=(hi.ceil() as i32).min(127))
        .filter_map(|row| {
            let cents = ed.tuning.cents(row);
            (cents.abs() > 0.5).then(|| TuningGuide {
                y: ed.camera.y(ed.tuning.center(row), ed.viewport),
                cents,
                label: format!(
                    "{} {}{:.0}¢",
                    expression_editor_core::tuning::note_name(row),
                    if cents > 0.0 { "+" } else { "" },
                    cents
                ),
            })
        })
        .collect()
}

/// Effective-pitch guides: a horizontal line across each zone at the
/// pitch its curve actually dwells on.
///
/// This is what a zone scales around — not the note's own row, so a
/// scoop that settles a fourth above its onset still expands about
/// where it lands.
pub struct ZoneGuide {
    pub x0: f64,
    pub x1: f64,
    pub y: f64,
}

pub fn zone_guides(ed: &Editor) -> Vec<ZoneGuide> {
    if ed.dimension != Dimension::Pitch {
        return Vec::new();
    }
    let mut out = Vec::new();
    for n in ed.doc.notes.iter().filter(|n| ed.selection.contains(n.id)) {
        if n.splits.is_empty() {
            continue;
        }
        for (a, b) in n.zones() {
            let center = expression_editor_core::blob::effective_center(&n.pitch, a, b, 48, 0.0);
            out.push(ZoneGuide {
                x0: ed.camera.x(a),
                x1: ed.camera.x(b),
                y: ed.camera.y(n.row as f64 + center, ed.viewport),
            });
        }
    }
    out
}

// ── chrome: keyboard gutter and timeline ruler ───────────────────────
//
// Both live inside the same SVG as the roll, drawn in the margins the
// roll is translated away from. One element, one coordinate system, one
// set of pointer handlers — a separate scrolling keyboard element would
// have to be kept in sync with the camera every frame.

/// Width of the piano-key gutter on the left.
pub const GUTTER_W: f64 = 54.0;
/// Height of the timeline ruler on top.
pub const RULER_H: f64 = 28.0;

/// One key in the gutter.
pub struct Key {
    pub row: i32,
    pub y: f64,
    pub h: f64,
    pub black: bool,
    /// Only C rows are labelled, so the gutter stays readable when the
    /// rows get short.
    pub label: Option<String>,
}

/// A brace over the rows of one split piece, so `L` and `R` read as two
/// hands of a `T1` rather than two unrelated lanes.
pub struct KeyGroup {
    pub label: String,
    /// Top edge and height of the whole span.
    pub y: f64,
    pub h: f64,
}

/// Group braces for whatever is on screen.
///
/// Built from the already-computed keys so the brace cannot drift from
/// the rows it spans — including when a span is half scrolled off.
pub fn key_groups(ed: &Editor, keys: &[Key]) -> Vec<KeyGroup> {
    let mut out: Vec<KeyGroup> = Vec::new();
    for k in keys {
        let Some(label) = ed.row_group(k.row) else {
            continue;
        };
        // Keys arrive in slot order, so a repeat of the same label is
        // always the continuation of the span above it.
        match out.last_mut() {
            Some(g) if g.label == label => {
                let top = g.y.min(k.y);
                let bottom = (g.y + g.h).max(k.y + k.h);
                g.y = top;
                g.h = bottom - top;
            }
            _ => out.push(KeyGroup {
                label,
                y: k.y,
                h: k.h,
            }),
        }
    }
    out
}

pub fn keyboard(ed: &Editor) -> Vec<Key> {
    let (lo, hi) = ed.camera.slot_span(ed.viewport);
    let h = ed.camera.vertical.px_per_row;
    let (rlo, rhi) = ed.camera.fold.slot_bounds(ed.row_space.bounds());
    // Named rows always carry their label — a drum lane called nothing
    // is unusable, where an unlabelled piano key can still be counted.
    let named = !matches!(ed.row_space, expression_editor_core::RowSpace::Pitch);
    let label_rows = h >= 8.0;
    ((lo.floor() as i32).max(rlo)..=(hi.ceil() as i32).min(rhi))
        .map(|slot| ed.camera.fold.row(slot as f64) as i32)
        .map(|row| Key {
            row,
            y: ed.camera.y(row as f64 + 0.5, ed.viewport),
            h,
            black: ed.row_space.is_accidental(row),
            label: (label_rows && (named || row.rem_euclid(12) == 0 || h >= 18.0))
                .then(|| ed.row_header(row)),
        })
        .collect()
}

/// A ruler tick.
pub struct Tick {
    pub x: f64,
    /// Bar starts get a full-height line and a number.
    pub bar: bool,
    pub label: Option<String>,
}

pub fn ruler(ed: &Editor) -> Vec<Tick> {
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let beat = ed.units_per_beat();
    let bar = ed.units_per_bar();
    // Label bars only while they are far enough apart to read; beat
    // ticks disappear entirely once they would be a grey smear.
    let show_beats = beat / ed.camera.units_per_px >= 14.0;
    let step = if show_beats { beat } else { bar };
    if step <= 0.0 || (t1 - t0) / step > 400.0 {
        return Vec::new();
    }
    let first = ((t0 - ed.doc.start) / step).floor() * step + ed.doc.start;
    let label_bars = bar / ed.camera.units_per_px >= 40.0;
    let mut out = Vec::new();
    let mut t = first;
    while t <= t1 {
        if t >= t0 {
            let (b, beat_n) = ed.bar_beat(t + step * 0.001);
            let is_bar = beat_n == 1;
            out.push(Tick {
                x: ed.camera.x(t),
                bar: is_bar,
                label: (is_bar && label_bars).then(|| b.to_string()),
            });
        }
        t += step;
    }
    out
}

/// Marker flags the host supplied.
pub struct MarkerFlag {
    pub x: f64,
    pub label: String,
}

pub fn markers(ed: &Editor) -> Vec<MarkerFlag> {
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    ed.doc
        .markers
        .iter()
        .filter(|m| m.t >= t0 && m.t <= t1)
        .map(|m| MarkerFlag {
            x: ed.camera.x(m.t),
            label: m.label.clone().unwrap_or_default(),
        })
        .collect()
}

/// A razor area in pixels.
pub struct RazorRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Razor areas, clipped to the visible span.
///
/// Drawn as a filled rectangle with hard vertical edges: the edges are
/// where notes get sliced, so they have to read as exact boundaries
/// rather than as a soft highlight.
pub fn razor_rects(ed: &Editor) -> Vec<RazorRect> {
    ed.razor.areas.iter().map(|a| razor_rect(ed, a)).collect()
}

/// One area's rectangle.
///
/// Split out so a razor still being swept draws with exactly the
/// geometry a committed one does — the preview is the same rectangle,
/// not a second calculation that has to be kept in step.
pub fn razor_rect(ed: &Editor, a: &expression_editor_core::razor::RazorArea) -> RazorRect {
    let x = ed.camera.x(a.t0);
    let top = ed.camera.y(a.row_hi as f64 + 0.5, ed.viewport);
    RazorRect {
        x,
        y: top,
        w: (ed.camera.x(a.t1) - x).max(1.0),
        h: ed.camera.vertical.px_per_row * a.rows() as f64,
    }
}

// ── velocity / CC lane strip ─────────────────────────────────────────

/// One note's velocity stem in the strip.
pub struct Stem {
    pub note: NoteId,
    pub x: f64,
    /// Bar width — narrow, so dense passages stay countable.
    pub w: f64,
    pub y: f64,
    pub h: f64,
    pub color: &'static str,
    pub selected: bool,
    pub muted: bool,
}

/// Velocity stems for the visible notes, in a strip `h` pixels tall.
///
/// Stems are drawn at the note's *onset*, not across its length: what
/// is being edited is a single value per note, and a full-width bar
/// invites dragging a duration that does not exist here.
pub fn stems(ed: &Editor, h: f64) -> Vec<Stem> {
    use expression_editor_core::StripLane;
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let per_note = matches!(ed.strip_lane, StripLane::Velocity | StripLane::OffVelocity);
    if !per_note {
        return Vec::new();
    }
    ed.doc
        .notes
        .iter()
        .filter(|n| n.end >= t0 && n.start <= t1)
        .map(|n| {
            let v = match ed.strip_lane {
                StripLane::OffVelocity => n.off_velocity,
                _ => n.velocity,
            }
            .clamp(0.0, 1.0);
            let bar = (ed.camera.x(n.end) - ed.camera.x(n.start)).clamp(3.0, 9.0);
            Stem {
                note: n.id,
                x: ed.camera.x(n.start),
                w: bar,
                y: h * (1.0 - v),
                h: h * v,
                // Same rule as the note body: where the row itself
                // carries meaning, the strip has to agree with the roll
                // or two colours name the same note.
                color: note_color(ed, n),
                selected: ed.selection.contains(n.id),
                muted: n.muted,
            }
        })
        .collect()
}

/// The strip's continuous-lane curve, when it is showing one.
///
/// Every visible note's curve is drawn end to end, normalized into the
/// strip — so a Pressure sweep reads as one gesture across the phrase
/// rather than as a set of disconnected per-note boxes.
pub fn strip_curves(ed: &Editor, h: f64) -> Vec<CurvePath> {
    use expression_editor_core::StripLane;
    let StripLane::Expression(dimension) = ed.strip_lane else {
        return Vec::new();
    };
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let (lo, hi) = match dimension {
        Dimension::Pitch => (-2.0, 2.0),
        _ => (0.0, 1.0),
    };
    ed.doc
        .notes
        .iter()
        .filter(|n| n.end >= t0 && n.start <= t1 && !n.curve(dimension).is_empty())
        .map(|n| {
            let mut s: Points = Vec::new();
            for p in n.curve(dimension).points() {
                let y = h * (1.0 - ((p.value - lo) / (hi - lo)).clamp(0.0, 1.0));
                s.push((ed.camera.x(p.t), y));
            }
            CurvePath {
                note: n.id,
                dimension,
                points: s,
                color: theme::lane_color(dimension),
                active: true,
                selected: ed.selection.contains(n.id),
            }
        })
        .collect()
}

/// Horizontal reference lines in the strip, at eighths of full scale.
pub fn strip_guides(h: f64) -> Vec<(f64, bool)> {
    // Only quarter, half and three-quarter get a visible line; more
    // would read as noise behind the stems.
    [0.25, 0.5, 0.75]
        .iter()
        .map(|f| (h * (1.0 - f), (*f - 0.5).abs() < 1e-9))
        .collect()
}

// ── pinned controller lanes ──────────────────────────────────────────

/// A controller lane ready to draw behind the roll.
pub struct CcPath {
    pub number: u8,
    pub label: String,
    pub color: &'static str,
    /// The line.
    pub points: String,
    /// The same path closed to the bottom, for the fill.
    pub fill: String,
    pub opacity: f64,
    pub active: bool,
}

/// Pinned lanes, spanning the full roll height.
///
/// The curve is sampled at the visible edges as well as at its own
/// points, so a lane whose last authored value is off-screen still
/// draws across the whole view instead of stopping mid-canvas.
pub fn cc_paths(ed: &Editor) -> Vec<CcPath> {
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let h = ed.viewport.h;
    let d = ed.cc_display;

    ed.doc
        .cc
        .pinned()
        .map(|dimension| {
            let active = ed.cc_edit == Some(dimension.number);
            let default = dimension.default_value();

            let mut ts: Vec<f64> = vec![t0];
            ts.extend(
                dimension
                    .curve
                    .points()
                    .iter()
                    .map(|p| p.t)
                    .filter(|t| *t > t0 && *t < t1),
            );
            ts.push(t1);

            let mut line = String::new();
            for &t in &ts {
                let v = dimension.curve.sample(t, default);
                line.push_str(&format!(
                    "{:.1},{:.1} ",
                    ed.camera.x(t),
                    expression_editor_core::cc::cc_y(v, h)
                ));
            }
            // Close down to the baseline and back, so the fill is a
            // shape rather than a self-intersecting ribbon.
            let fill = format!(
                "{:.1},{:.1} {line}{:.1},{:.1}",
                ed.camera.x(t0),
                h,
                ed.camera.x(t1),
                h
            );

            CcPath {
                number: dimension.number,
                label: dimension.label(),
                color: expression_editor_core::cc::CC_COLORS
                    [dimension.color % expression_editor_core::cc::CC_COLORS.len()],
                points: line,
                fill,
                opacity: if active {
                    d.active_opacity
                } else {
                    d.background_opacity
                },
                active,
            }
        })
        .collect()
}
