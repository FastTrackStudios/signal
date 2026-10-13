//! The stacked multitrack view — every track at once, on one timeline.
//!
//! Each track gets a horizontal lane and is drawn in *its own* mode: a
//! vocal as blobs, its reference MIDI as a roll, a guitar as tab, a kit
//! as slices. Time is shared; vertical space is divided.
//!
//! That sharing is the entire feature, so it has to be exact. Two things
//! make it non-trivial:
//!
//! **Tracks do not agree on a time unit.** A pitched-audio document is
//! in analysis frames at the pitch tracker's hop; a percussive one is in
//! frames at the onset hop; a MIDI one is in ticks. Drawing all three
//! against the active track's camera without converting puts them at
//! wildly different scales — and it *looks* plausible, which is worse
//! than looking broken. Everything is converted through seconds.
//!
//! **Row spaces have different extents.** 128 pitch rows, 6 strings, 3
//! bands. Giving each lane the same rows-per-pixel would leave the kit
//! occupying a twentieth of its lane. Each lane instead fits its own
//! content to its own height.

use expression_editor_core::doc::{ExpressionDoc, Note};
use expression_editor_core::kit;
use expression_editor_core::rows::RowSpace;
use expression_editor_core::tracks::StackRow;
use expression_editor_core::{Editor, Mode};

use dioxus::prelude::*;
use dioxus_elements::input_data::MouseButton;
use keyboard_types::{Key, Modifiers};

use crate::{canvas, theme};

/// One track's lane in the stack.
pub struct LaneView {
    /// Index into the workspace.
    pub track: usize,
    pub name: String,
    pub mode: Mode,
    /// Top edge and height, in viewport pixels.
    pub y: f64,
    pub h: f64,
    /// The track being edited. Drawn brighter; the only one that takes
    /// gestures.
    pub active: bool,
    /// The track other tracks are aligned against, if any.
    pub reference: bool,
    /// Drum row index when this lane is one hand of a two-handed piece.
    pub two_handed_row: Option<usize>,
    /// Whether that piece is currently showing both hands.
    pub split: bool,
    pub notes: Vec<LaneNote>,
    /// Row dividers inside the lane, for spaces where a row is a named
    /// thing (strings, bands, drum lanes). Empty for pitch, which has
    /// too many to draw and a keyboard to read instead.
    pub dividers: Vec<f64>,
    /// Labels down the left edge, paired with their y.
    pub labels: Vec<(f64, String)>,
    /// Whether this lane is a folded role lane (kick, snare, toms,
    /// other) — the lanes whose hits take the slip drag.
    pub is_role: bool,
    /// The role's hue, when the lane has one — the drum map's kit
    /// palette, tinting waveform, hits and label alike so the lane
    /// reads as one thing.
    pub role_color: Option<&'static str>,
    /// Index into the workspace's lane layout — the stable identity a
    /// popup keyed on this lane survives re-renders by.
    pub lane: usize,
    /// The lane's member tracks: `(track index, name, is_active)`, in
    /// draw order. What the gutter's mic selector offers.
    pub members: Vec<(usize, String, bool)>,
    /// Whether the lane draws only its current mic instead of the sum.
    pub solo_mic: bool,
    /// The summed waveform of a role lane (kick, snare, other), as
    /// mirrored polygon points — the same shape
    /// [`canvas::take_waveform`] builds for the roll. `None` when the
    /// lane has no role, no members with peaks, or splits its members.
    pub waveform: Option<String>,
    /// One sub-row per member for a split role lane (toms). Empty
    /// otherwise.
    pub sub_lanes: Vec<SubLane>,
}

/// One member's sub-row inside a split role lane (toms).
pub struct SubLane {
    /// The member's own waveform polygon, if it carries peaks.
    pub points: Option<String>,
    /// The member track's name, drawn small in the gutter.
    pub label: String,
    /// Baseline y of that label, in viewport pixels.
    pub label_y: f64,
    /// Unused or hidden members draw at half opacity.
    pub faded: bool,
}

/// A note as it appears in a lane.
pub struct LaneNote {
    pub x: f64,
    pub w: f64,
    pub y: f64,
    pub h: f64,
    /// Onset in seconds, for gestures that leave the pixel domain —
    /// the slip drag hands the host times, not x coordinates.
    pub at_secs: f64,
    pub fill: String,
    /// Slices and drum hits draw as triangles; everything else as bars.
    pub triangle: bool,
    /// This note ornaments another — draw it small and slashed, the way
    /// a grace note is engraved.
    pub grace: bool,
    /// This hit *has* a grace note, so it is a flam.
    ///
    /// Carried separately from `grace` because the principal is drawn
    /// full size and only badged; conflating them would shrink the note
    /// you actually played.
    pub flam: bool,
    /// Draw as a full-height trigger line with a small onset flag —
    /// how a role lane marks a hit on its waveform. Off everywhere
    /// else, where the note body is the content.
    pub hit_line: bool,
}

/// One hand edit leaving the stack, in seconds — the host decides what
/// it means on the daw. `Slip` cuts and slides (SPLIT), `Stretch`
/// writes a marker map (WARP); `Add`/`Remove` edit the hit list only.
///
/// One enum rather than one callback per gesture, because every arm
/// shares a fate: they land on the *group*, as one undo step, through
/// the drum host — and a host that takes one takes them all.
#[derive(Clone, Debug, PartialEq)]
pub enum HitGesture {
    /// r[impl drums.manual.slip]
    Slip {
        hit: f64,
        /// `f64::INFINITY` when the hit is the lane's last — the host
        /// clamps to its take length.
        next: f64,
        delta: f64,
    },
    /// r[impl drums.manual.stretch]
    Stretch {
        hit: f64,
        /// `f64::NEG_INFINITY` when the hit is the lane's first.
        prev: f64,
        next: f64,
        delta: f64,
        /// The BothStretch law: pin the take's ends, not the
        /// neighbours.
        both: bool,
    },
    /// r[impl drums.manual.add-remove]
    Add { lane: String, at: f64 },
    /// r[impl drums.manual.add-remove]
    Remove { lane: String, hit: f64 },
}

/// One slip/stretch drag: which hit is being slid, and where the
/// pointer is.
///
/// Everything the release needs is captured at the press, so a lane
/// re-layout mid-drag cannot re-target the gesture.
#[derive(Clone, Debug)]
struct SlipDrag {
    /// The dragged hit's onset, seconds.
    hit_secs: f64,
    /// The previous hit's onset — `f64::NEG_INFINITY` for the first.
    prev_secs: f64,
    /// The next hit's onset, seconds — `f64::INFINITY` for the last.
    next_secs: f64,
    /// The lane the hit lives in, by its drawn name (`"Kick"`).
    lane_name: String,
    /// Where the press landed, element x.
    x0: f64,
    /// Where the pointer is now, element x.
    x: f64,
    /// Shift as of the last pointer event — the BothStretch law.
    shift: bool,
    /// The hit line's own x at the press (gutter-relative), so the
    /// ghost draws on the line rather than under the finger.
    hit_x: f64,
    /// The lane's strip, for the ghost lines (y from the lane group's
    /// origin).
    lane_y: f64,
    lane_h: f64,
}

/// The selected hit — the unit of keyboard editing. Selection follows
/// the *time*, not a note index: a nudge moves the audio under the
/// drawing, and the next nudge must move from where the hit now is.
// r[impl drums.manual.nudge]
#[derive(Clone, Debug)]
struct SelectedHit {
    lane_name: String,
    hit_secs: f64,
    prev_secs: f64,
    next_secs: f64,
    lane_y: f64,
    lane_h: f64,
}

/// How close to a hit line a press must land to pick it up, px.
const SLIP_PICK_PX: f64 = 6.0;

/// Vertical padding inside a lane, in pixels, top and bottom.
const LANE_PAD: f64 = 3.0;

// ── the gutter's mic selector ────────────────────────────────────────
//
// Shared by the renderer and the press handler, which must agree on
// this geometry exactly or clicks land beside what they aim at.
/// The chip's top, below the role eyebrow, lane-relative.
const MIC_CHIP_TOP: f64 = 18.0;
/// The menu's first item top, lane-relative.
const MIC_MENU_TOP: f64 = 36.0;
/// One row of chip or menu.
const MIC_ITEM_H: f64 = 16.0;
/// The open menu's width — wider than the gutter, over the lane.
const MIC_MENU_W: f64 = 120.0;

/// Rows of headroom added around a lane's content before fitting.
///
/// Without it a track whose notes are all on one row fits to zero height
/// and draws as a hairline, and a melody that touches its own extremes
/// has notes flush against the lane edge where they read as clipped.
const FIT_PAD: f64 = 1.0;

/// Lay out every visible track over the viewport.
///
/// `active_boost` and `min_row` are passed through to
/// [`expression_editor_core::tracks::Workspace::stack`].
pub fn lanes(ed: &Editor, active_boost: f32, min_row: f32) -> Vec<LaneView> {
    let rows = ed.tracks.stack(ed.viewport.h as f32, active_boost, min_row);
    rows.iter().filter_map(|row| lane_view(ed, row)).collect()
}

fn lane_view(ed: &Editor, row: &StackRow) -> Option<LaneView> {
    // A lane may hold several tracks, and all of them draw — that is the
    // point of pairing a vocal with its guide. The lane's *primary*
    // track (the active one if it is here, else the first visible)
    // decides the labelling and the row space, because a lane needs one
    // vertical axis and its members share a pitch range by construction.
    let members: Vec<usize> = ed
        .tracks
        .lane_tracks(row.lane)
        .into_iter()
        .filter(|&i| ed.tracks.track(i).is_some_and(|t| !t.hidden))
        .collect();
    let track_index = members
        .iter()
        .copied()
        .find(|&i| i == ed.tracks.active())
        .or_else(|| members.first().copied())?;
    let track = ed.tracks.track(track_index)?;
    let active = track_index == ed.tracks.active();
    // The active track's parked copy is stale by design — the live
    // document is the editor's. `doc_of` refuses to hand out the stale
    // one, which is exactly the guard that keeps a stacked view from
    // drawing a track's previous state.
    let doc = if active {
        &ed.doc
    } else {
        ed.tracks.doc_of(track_index)?
    };

    let y0 = row.y as f64 - ed.stack_scroll + LANE_PAD;
    let h = (row.height as f64 - LANE_PAD * 2.0).max(1.0);
    // Read the lane's stored vertical camera rather than re-deriving a
    // fit from content. That distinction *is* the feature: deriving it
    // here meant the lane silently rescaled whenever a note moved.
    // Falling back to a fresh fit only covers the frame before the
    // editor has fitted anything.
    let cam = ed.lane_camera(row.lane).unwrap_or_else(|| {
        let (lo, hi) = ed
            .tracks
            .lane_row_range(row.lane, &ed.doc, Editor::LANE_FIT_PAD)
            .unwrap_or_else(|| fit(doc, &track.mode));
        expression_editor_core::camera::VerticalCamera::fitted(lo, hi, h)
    });
    let row_h = cam.px_per_row;
    // Rows run bottom-up in every space this draws: higher pitch higher,
    // brighter band higher, and a string roll reads like tab with the
    // lowest string at the bottom.
    let y_of = move |r: f64| cam.y(r, y0, h);

    // Every member's notes, primary last so the track you are editing
    // draws on top of the guide rather than under it.
    let mut notes: Vec<LaneNote> = Vec::new();
    for &i in members
        .iter()
        .filter(|&&i| i != track_index)
        .chain([&track_index])
    {
        let Some(member) = ed.tracks.track(i) else {
            continue;
        };
        let member_doc = if i == ed.tracks.active() {
            &ed.doc
        } else {
            match ed.tracks.doc_of(i) {
                Some(d) => d,
                None => continue,
            }
        };
        let member_active = i == ed.tracks.active();
        notes.extend(member_doc.notes.iter().map(|n| {
            lane_note(
                ed,
                member_doc,
                n,
                &member_doc.row_space,
                &member.mode,
                member_active,
                y_of,
                row_h,
            )
        }));
    }

    // A role lane (kick / snare / toms / other) is labelled by its role
    // and drawn as its members' audio, not by any one member's row
    // space — so the row-space guides give way to the waveform.
    let lane_def = ed.tracks.layout().lane(row.lane);
    let role = lane_def.and_then(|l| l.role);
    let role_split = role.is_some() && lane_def.is_some_and(|l| l.split);

    let mut waveform = None;
    let mut sub_lanes = Vec::new();
    let mut sub_dividers = Vec::new();
    if let Some((v0, v1)) = view_span_secs(ed) {
        let count = (ed.viewport.w.ceil() as usize).clamp(2, 2048);
        if role.is_some() && !role_split {
            // r[impl drums.lanes.summed]
            //
            // Solo-mic shows just the lane's current mic — the one the
            // gutter chip names — instead of the members' mean. A view
            // choice only: detection and edits still take the lane
            // whole.
            let solo = lane_def.is_some_and(|l| l.solo_mic);
            let mems: Vec<(&[f32], f64, f64)> = members
                .iter()
                .filter(|&&i| !solo || i == track_index)
                .filter_map(|&i| {
                    let d = doc_for(ed, i)?;
                    let (s, e) = doc_span_secs(ed, d)?;
                    Some((d.peaks.as_slice(), s, e))
                })
                .collect();
            let cols = summed_columns(&mems, v0, v1, count);
            waveform = columns_polygon(&cols, ed.viewport.w, y0, h);
        } else if role_split {
            // r[impl drums.lanes.toms-split]
            //
            // Every member gets a sub-row, hidden and `Unused` ones
            // included — a tom that is parked still holds its place in
            // the kit, it just draws faded.
            let all = ed.tracks.lane_tracks(row.lane);
            let k = all.len().max(1);
            let sub_h = h / k as f64;
            for (j, &i) in all.iter().enumerate() {
                let Some(member) = ed.tracks.track(i) else {
                    continue;
                };
                let sy = y0 + sub_h * j as f64;
                if j > 0 {
                    sub_dividers.push(sy);
                }
                let points = doc_for(ed, i).and_then(|d| {
                    let (s, e) = doc_span_secs(ed, d)?;
                    let cols = summed_columns(&[(d.peaks.as_slice(), s, e)], v0, v1, count);
                    columns_polygon(&cols, ed.viewport.w, sy, sub_h)
                });
                sub_lanes.push(SubLane {
                    points,
                    label: member.name.clone(),
                    label_y: sy + 9.0,
                    faded: kit::is_unused_name(&member.name) || member.hidden,
                });
            }
        }
    }

    // In a role lane the audio is the content and a hit is a *marker*
    // on it: full-height trigger lines with a small onset flag, the way
    // a drum editor draws them — not band-height wedges, which at this
    // lane height bury the waveform they annotate. The markers take the
    // role's hue, brighter on the armed lane, so "red line" *means*
    // kick from across the room.
    // r[impl drums.lanes.hits]
    if let Some(role) = role {
        let color = role.color();
        for n in &mut notes {
            n.hit_line = true;
            n.y = y0;
            n.h = h;
            n.fill = if active {
                color.to_string()
            } else {
                format!("{color}b0")
            };
        }
    }

    // Guides span whatever the camera is actually showing, which is not
    // necessarily the content range any more — an edit can push content
    // past the edge, and the lane deliberately does not chase it.
    let (lo, hi) = cam.span(h);
    let (mut dividers, labels) = if role.is_some() {
        // The role label and the waveform are the lane's furniture; the
        // row space's band names underneath them would just collide.
        (Vec::new(), Vec::new())
    } else {
        guides(&doc.row_space, lo, hi, y_of)
    };
    dividers.extend(sub_dividers);

    // A drum lane's header offers a hand split only when the piece has
    // two hands to offer.
    let (two_handed_row, split) = match &doc.row_space {
        RowSpace::Drums(m) => {
            let row = doc
                .notes
                .first()
                .map(|n| n.row.max(0) as usize)
                .unwrap_or(0);
            if m.is_two_handed(row) {
                (Some(row), ed.split_pieces.contains(&row))
            } else {
                (None, false)
            }
        }
        _ => (None, false),
    };

    Some(LaneView {
        track: track_index,
        // A role lane is the role, not whichever member happens to be
        // primary — "Kick", not "Kick In".
        name: role
            .map(|r| r.label().to_string())
            .unwrap_or_else(|| track.name.clone()),
        mode: track.mode,
        y: row.y as f64 - ed.stack_scroll,
        h: row.height as f64,
        active,
        reference: track.reference,
        notes,
        dividers,
        labels,
        two_handed_row,
        split,
        is_role: role.is_some(),
        role_color: role.map(|r| r.color()),
        lane: row.lane,
        members: members
            .iter()
            .filter_map(|&i| {
                let t = ed.tracks.track(i)?;
                Some((i, t.name.clone(), i == ed.tracks.active()))
            })
            .collect(),
        solo_mic: lane_def.is_some_and(|l| l.solo_mic),
        waveform,
        sub_lanes,
    })
}

/// The sounding pitch of a note, for colour that means harmony.
///
/// On a string roll the row is a string, not a pitch, so pitch-class
/// colour has to go through the tuning or every note on one string
/// would share a colour — which is the thing the toggle exists to turn
/// off.
fn pitch_of(_space: &RowSpace, n: &Note) -> i32 {
    // Every row space now measures pitch on its row axis, strings
    // included — the string is on the note, not the row.
    n.row
}

/// The row range a lane shows: its own content, padded.
fn fit(doc: &ExpressionDoc, mode: &Mode) -> (f64, f64) {
    let (bound_lo, bound_hi) = doc.row_space.bounds();
    // A space with few rows shows all of them — three bands or six
    // strings are the axis, and fitting to content would move a kit's
    // lanes around as hits come and go.
    if !matches!(doc.row_space, RowSpace::Pitch) {
        return (bound_lo as f64 - 0.5, bound_hi as f64 + 0.5);
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for n in &doc.notes {
        lo = lo.min(n.row as f64);
        hi = hi.max(n.row as f64);
    }
    if !lo.is_finite() || !hi.is_finite() {
        // Nothing to fit to. An octave around middle C is a better blank
        // lane than the full 128 rows, which would draw everything
        // subsequently loaded as a smear at the bottom.
        let centre = 60.0;
        return (centre - 6.0, centre + 6.0);
    }
    // A blob needs room above and below for its pitch excursions; a bar
    // does not.
    let pad = if mode.draws_blobs() {
        FIT_PAD + 1.0
    } else {
        FIT_PAD
    };
    (lo - pad, hi + pad + 1.0)
}

#[allow(clippy::too_many_arguments)]
fn lane_note(
    ed: &Editor,
    doc: &ExpressionDoc,
    n: &Note,
    space: &RowSpace,
    mode: &Mode,
    active: bool,
    y_of: impl Fn(f64) -> f64,
    row_h: f64,
) -> LaneNote {
    let x0 = ed.camera.x(to_editor_time(ed, doc, n.start));
    let x1 = ed.camera.x(to_editor_time(ed, doc, n.end));
    // Slices are often a frame or two long at this zoom. A note that
    // rounds to zero width is a note the eye cannot find, which defeats
    // the purpose of showing the track at all.
    let w = (x1 - x0).max(1.5);

    // On a string roll the row is the pitch, so the string has to
    // colour the *note* — that is what tells you a phrase moved across
    // the neck rather than up it. Turned off, pitch-class colour reads
    // harmony instead.
    let by_row = match (space, n.string) {
        (RowSpace::Strings(_), Some(s)) if ed.color_by_string => {
            Some(expression_editor_core::rows::string_color(s))
        }
        (RowSpace::Strings(_), _) => None,
        _ => space.row_color(n.row),
    };
    let base = by_row
        .map(str::to_string)
        .unwrap_or_else(|| theme::pitch_class_color(pitch_of(space, n)).to_string());
    let fill = if active {
        base
    } else {
        // Parked lanes are context, not content. Dimming them keeps the
        // lane you are editing legible when six are on screen.
        format!("{base}80")
    };

    let ups = doc.time_base.units_per_second(ed.bpm);
    LaneNote {
        x: x0,
        w,
        y: y_of(n.row as f64 + 1.0),
        h: (row_h * 0.9).max(1.0),
        at_secs: if ups.abs() < 1e-9 { 0.0 } else { n.start / ups },
        fill,
        grace: n.grace_of.is_some(),
        flam: doc.is_flam(n.id),
        triangle: matches!(
            space.note_shape(),
            expression_editor_core::rows::NoteShape::Triangle
        ) || mode.draws_slices(),
        hit_line: false,
    }
}

/// Convert a track's own document time into the editor's units.
///
/// Through seconds, because the two documents need not share a time
/// base: a percussive take is in frames at the onset hop, a pitched one
/// at the pitch hop, a MIDI take in ticks. Drawing them against one
/// camera without this is off by the ratio between the two rates — and
/// it produces a plausible-looking picture, which is the dangerous kind
/// of wrong for a view whose whole job is answering "is this in time".
fn to_editor_time(ed: &Editor, doc: &ExpressionDoc, t: f64) -> f64 {
    let bpm = ed.bpm;
    let from = doc.time_base.units_per_second(bpm);
    let to = ed.doc.time_base.units_per_second(bpm);
    if from.abs() < 1e-9 {
        return t;
    }
    t / from * to
}

/// A track's current document — the editor's live one when the track is
/// active, its parked copy otherwise.
fn doc_for(ed: &Editor, i: usize) -> Option<&ExpressionDoc> {
    if i == ed.tracks.active() {
        Some(&ed.doc)
    } else {
        ed.tracks.doc_of(i)
    }
}

/// The viewport's visible time span, in seconds.
///
/// Seconds for the same reason [`to_editor_time`] goes through them:
/// each member's peaks are indexed in its *own* document's units, and
/// seconds are the one base they all share.
fn view_span_secs(ed: &Editor) -> Option<(f64, f64)> {
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let ups = ed.doc.time_base.units_per_second(ed.bpm);
    if ups.abs() < 1e-9 {
        return None;
    }
    Some((t0 / ups, t1 / ups))
}

/// A document's `[start, end]` span, in seconds.
fn doc_span_secs(ed: &Editor, doc: &ExpressionDoc) -> Option<(f64, f64)> {
    let ups = doc.time_base.units_per_second(ed.bpm);
    if ups.abs() < 1e-9 {
        return None;
    }
    Some((doc.start / ups, doc.end / ups))
}

/// A member's mean peak value over one column's time span.
///
/// The member's peaks are uniform over `[start, end]` seconds; the
/// column covers `[t0, t1)`. Bins overlapping the column are averaged,
/// and a column entirely outside the member is silence — the member
/// simply isn't sounding there.
fn member_column(peaks: &[f32], start: f64, end: f64, t0: f64, t1: f64) -> f32 {
    if peaks.is_empty() || end <= start {
        return 0.0;
    }
    let n = peaks.len();
    let span = end - start;
    let lo = ((t0 - start) / span * n as f64).floor() as isize;
    let hi = ((t1 - start) / span * n as f64).ceil() as isize;
    if hi <= 0 || lo >= n as isize {
        return 0.0;
    }
    let lo = lo.max(0) as usize;
    let hi = (hi.min(n as isize) as usize).max(lo + 1);
    peaks[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
}

/// The summed waveform of a role lane, one value per viewport column.
///
/// Per column, the **mean** of the members' peaks — the members are
/// phase-aligned mics of one source, so the mean is the mix a SUM bus
/// would render — normalised so the lane's loudest column is 1.0.
/// Members with no peaks are skipped rather than dragging the mean
/// down; a lane with one member yields that member.
///
/// `members` is `(peaks, start_secs, end_secs)` per member;
/// `view_start`/`view_end` bound the columns, in seconds.
// r[impl drums.lanes.summed]
pub fn summed_columns(
    members: &[(&[f32], f64, f64)],
    view_start: f64,
    view_end: f64,
    columns: usize,
) -> Vec<f32> {
    let columns = columns.max(2);
    let mut out = vec![0.0f32; columns];
    let live: Vec<_> = members.iter().filter(|(p, _, _)| !p.is_empty()).collect();
    if live.is_empty() || view_end <= view_start {
        return out;
    }
    let step = (view_end - view_start) / columns as f64;
    for (i, v) in out.iter_mut().enumerate() {
        let t0 = view_start + step * i as f64;
        let sum: f32 = live
            .iter()
            .map(|(p, s, e)| member_column(p, *s, *e, t0, t0 + step))
            .sum();
        *v = sum / live.len() as f32;
    }
    let max = out.iter().copied().fold(0.0f32, f32::max);
    if max > 0.0 {
        for v in &mut out {
            *v /= max;
        }
    }
    out
}

/// Column values as one mirrored polygon, the shape
/// [`canvas::take_waveform`] draws: zero on the row's midline, full
/// scale just short of its edges. `None` when there is nothing to draw.
fn columns_polygon(cols: &[f32], w: f64, y0: f64, h: f64) -> Option<String> {
    if cols.len() < 2 || cols.iter().all(|&v| v <= 0.0) {
        return None;
    }
    let mid = y0 + h * 0.5;
    let max_half = h * 0.46;
    let mut top = String::new();
    let mut bottom = Vec::with_capacity(cols.len());
    for (i, &v) in cols.iter().enumerate() {
        let x = w * (i as f64 / (cols.len() - 1) as f64);
        let half = max_half * (v as f64).clamp(0.0, 1.0);
        if i > 0 {
            top.push(' ');
        }
        top.push_str(&format!("{x:.1},{:.1}", mid - half));
        bottom.push(format!("{x:.1},{:.1}", mid + half));
    }
    bottom.reverse();
    Some(format!("{top} {}", bottom.join(" ")))
}

/// Dividers and labels for a lane's row space.
fn guides(
    space: &RowSpace,
    lo: f64,
    hi: f64,
    y_of: impl Fn(f64) -> f64,
) -> (Vec<f64>, Vec<(f64, String)>) {
    // Pitch space has 128 rows and a keyboard of its own; drawing a line
    // per semitone inside a 60 px lane is a grey block. A string roll
    // is a pitch roll now, so it is in the same position — the string
    // shows as note colour, not as a row it owns.
    if matches!(space, RowSpace::Pitch | RowSpace::Strings(_)) {
        return (Vec::new(), Vec::new());
    }
    let (bound_lo, bound_hi) = space.bounds();
    let mut dividers = Vec::new();
    let mut labels = Vec::new();
    for r in bound_lo..=bound_hi {
        let rf = r as f64;
        if rf < lo - 1.0 || rf > hi + 1.0 {
            continue;
        }
        dividers.push(y_of(rf + 0.5));
        labels.push((y_of(rf), space.row_label(r)));
    }
    (dividers, labels)
}

/// How much taller the lane being edited is than the rest.
/// Kept in core so the editor's auto-scroll lays lanes out exactly
/// as the renderer does; a mismatch would scroll to the wrong place.
use expression_editor_core::Editor as CoreEditor;
const ACTIVE_BOOST: f32 = CoreEditor::ACTIVE_BOOST;

/// Shortest a lane may be, in pixels.
///
/// Enough to click, because clicking a lane is how you make it the
/// active one — a lane too small to hit is a track you cannot get back
/// to.
const MIN_LANE: f32 = 22.0;

/// Every track at once, on one timeline.
///
/// Read-only by design. The stack answers "which track needs work" and
/// "is this in time with that"; the roll is where the work happens.
/// Clicking a lane makes it active, which is the handover between the
/// two — and it means the one gesture the stack does take is the one
/// that gets you out of it.
#[component]
pub fn StackView(
    editor: Signal<Editor>,
    /// Called when a hand edit leaves the stack — a slip or stretch
    /// drag released, a nudge key, a hit added or thrown out. `None`
    /// disables all of them: without a writer the gestures would lie.
    // r[impl drums.manual.slip]
    #[props(default)]
    on_hit: Option<EventHandler<HitGesture>>,
    /// Whether a drag/nudge stretches (WARP) instead of slipping
    /// (SPLIT) — the quantize panel's write mode, threaded down so the
    /// hand gesture and the Apply button agree about what an edit is.
    // r[impl drums.manual.stretch]
    #[props(default)]
    warp: bool,
    /// One grid division in seconds — the nudge step, and what a
    /// double-clicked hit snaps to. `<= 0` disables nudge and snap.
    // r[impl drums.manual.nudge]
    #[props(default)]
    grid_secs: f64,
    /// The transport's position, seconds, when a host has one — the
    /// stack draws it as a playhead so "where am I" survives leaving
    /// the arrangement. `None` (a demo scene, a test) draws nothing.
    #[props(default)]
    playhead_secs: Option<Signal<f64>>,
) -> Element {
    let mut editor = editor;
    // Where a middle-drag pan last was.
    let mut panning = use_signal(|| None::<(f64, f64)>);
    // An in-flight slip/stretch drag on a role lane's hit.
    let mut slipping = use_signal(|| None::<SlipDrag>);
    // The selected hit, if any — what the keys act on.
    let mut selected = use_signal(|| None::<SelectedHit>);
    // The previous press, for double-click detection: no dblclick
    // event reaches this renderer, so two presses within the window
    // and the pick radius are the gesture.
    let mut last_press = use_signal(|| None::<(std::time::Instant, f64, f64)>);
    // The lane whose mic menu is open, by layout-lane index. One at a
    // time: a second chip click moves the menu rather than stacking.
    let mut mic_menu = use_signal(|| None::<usize>);
    // Where the pointer last was, for anchoring wheel zoom. (Focus
    // needs no handling: Blitz focuses the nearest focusable ancestor
    // on pointer-down — an FTS patch in the fork.)
    let mut wheel_anchor = use_signal(|| None::<(f64, f64)>);
    let ed = editor.read();
    let vp = ed.viewport;
    let lanes = lanes(&ed, ACTIVE_BOOST, ed.lane_floor().max(MIN_LANE));
    let ticks = canvas::ruler(&ed);
    // The song's sections across the ruler — clipped to the view, with
    // the label given only the room its span actually has.
    let sections: Vec<(f64, f64, String, String)> = {
        let (t0, t1) = ed.camera.time_span(ed.viewport);
        ed.doc
            .regions
            .iter()
            .filter(|r| r.end > t0 && r.start < t1)
            .map(|r| {
                let x0 = ed.camera.x(r.start.max(t0)).max(0.0);
                let x1 = ed.camera.x(r.end.min(t1)).min(vp.w);
                let fit = (((x1 - x0) - 6.0) / 5.5).max(0.0) as usize;
                let label: String = r.label.chars().take(fit).collect();
                let color = r.color.clone().unwrap_or_else(|| theme::SURFACE_BAR.into());
                (x0, x1, label, color)
            })
            .collect()
    };
    let (view0, px_per_sec) = view_span_secs(&ed)
        .map(|(v0, v1)| {
            if (v1 - v0).abs() < 1e-9 {
                (v0, 0.0)
            } else {
                (v0, vp.w / (v1 - v0))
            }
        })
        .unwrap_or((0.0, 0.0));
    drop(ed);

    // One place turns a released drag (or a synthetic delta from a key)
    // into the outgoing gesture, so the drag, the nudge and the snap
    // cannot disagree about what an edit is in each write mode.
    let emit_move = {
        let on_hit = on_hit;
        move |s: &SelectedHit, delta: f64, both: bool| {
            let Some(h) = &on_hit else { return };
            let g = if warp {
                // r[impl drums.manual.stretch]
                HitGesture::Stretch {
                    hit: s.hit_secs,
                    prev: s.prev_secs,
                    next: s.next_secs,
                    delta,
                    both,
                }
            } else {
                // r[impl drums.manual.slip]
                HitGesture::Slip {
                    hit: s.hit_secs,
                    next: s.next_secs,
                    delta,
                }
            };
            h.call(g);
        }
    };

    rsx! {
        div {
            // Focusable so the nudge keys have somewhere to land; the
            // roll's canvas cell does the same. Keys are bound here,
            // locally — the stacked view's bindings must not leak into
            // the roll's keymap.
            style: "display: block; width: 100%; height: 100%; outline: none;",
            tabindex: "0",
            "data-testid": "stack-cell",
            onkeydown: move |e: KeyboardEvent| {
                if e.is_auto_repeating() {
                    return;
                }
                // Zoom keys work with nothing selected — asking for a
                // selection before you may look at something closer
                // gets the order of operations backwards.
                if let Key::Character(c) = e.key() {
                    let factor = match c.as_str() {
                        "+" | "=" => Some(1.4),
                        "-" | "_" => Some(1.0 / 1.4),
                        _ => None,
                    };
                    if let Some(f) = factor {
                        editor.write().zoom_time_at(vp.w * 0.5, f);
                        e.prevent_default();
                        return;
                    }
                }
                let Some(sel) = selected() else { return };
                let shift = e.modifiers().contains(Modifiers::SHIFT);
                match e.key() {
                    // r[impl drums.manual.nudge]
                    Key::ArrowLeft | Key::ArrowRight if on_hit.is_some() => {
                        // The division with Shift for the fine step:
                        // 1 ms, the sample-accurate trim.
                        let step = if shift { 0.001 } else { grid_secs };
                        if step <= 0.0 {
                            return;
                        }
                        let delta = if e.key() == Key::ArrowLeft { -step } else { step };
                        emit_move(&sel, delta, false);
                        let mut sel = sel;
                        sel.hit_secs += delta;
                        selected.set(Some(sel));
                    }
                    // r[impl drums.manual.add-remove]
                    Key::Delete | Key::Backspace => {
                        if let Some(h) = &on_hit {
                            h.call(HitGesture::Remove {
                                lane: sel.lane_name.clone(),
                                hit: sel.hit_secs,
                            });
                        }
                        selected.set(None);
                    }
                    _ => {}
                }
            },
        svg {
            style: "display: block; width: 100%; height: 100%; \
                    touch-action: none; user-select: none; cursor: pointer;",
            view_box: "0 0 {vp.w + canvas::GUTTER_W:.0} {vp.h + canvas::RULER_H:.0}",
            preserve_aspect_ratio: "none",
            // No `onmounted` measure here, deliberately.
            //
            // This used to `spawn` and `await get_client_rect()` from the
            // mount handler — the re-entrancy pattern that caused #167,
            // and the one the Blitz testing notes say never to use: the
            // await re-enters the document while the mount that started
            // it is still on the stack.
            //
            // It is also unnecessary. `ExpressionEditor` already keeps
            // `ed.viewport` in step with the host's space through
            // `sizing::viewport_within`, in one effect, for whichever
            // view is showing. The stack was measuring itself only
            // because `chrome_of` charged it for a lane strip it does
            // not render, which made the shared answer wrong — so the
            // fix was to make the shared answer right rather than to
            // keep a second one.
            onpointermove: move |e: PointerEvent| {
                {
                    let c = e.data().element_coordinates();
                    wheel_anchor.set(Some((c.x, c.y)));
                }
                if let Some(mut s) = slipping() {
                    s.x = e.data().element_coordinates().x;
                    s.shift = e.data().modifiers().contains(Modifiers::SHIFT);
                    slipping.set(Some(s));
                    return;
                }
                let Some((lx, ly)) = panning() else { return };
                let c = e.data().element_coordinates();
                let mut ed = editor.write();
                // Time on the shared camera, and the stack's own scroll
                // for vertical — the tracks are stacked in a list, not
                // laid out on a pitch axis.
                ed.pan_px(c.x - lx, 0.0);
                ed.stack_scroll = (ed.stack_scroll - (c.y - ly)).max(0.0);
                drop(ed);
                panning.set(Some((c.x, c.y)));
            },
            onpointerup: move |_| {
                if let Some(s) = slipping() {
                    slipping.set(None);
                    let delta = if px_per_sec > 0.0 {
                        (s.x - s.x0) / px_per_sec
                    } else {
                        0.0
                    };
                    let sel = SelectedHit {
                        lane_name: s.lane_name.clone(),
                        hit_secs: s.hit_secs,
                        prev_secs: s.prev_secs,
                        next_secs: s.next_secs,
                        lane_y: s.lane_y,
                        lane_h: s.lane_h,
                    };
                    // A press that never travelled is a click, not a
                    // drag — it *selects* the hit. Half a millisecond
                    // is below anything a hand meant.
                    if delta.abs() > 0.0005 {
                        // r[impl drums.manual.slip]
                        // r[impl drums.manual.stretch]
                        emit_move(&sel, delta, s.shift);
                        let mut sel = sel;
                        sel.hit_secs += delta;
                        selected.set(Some(sel));
                    } else {
                        selected.set(Some(sel));
                    }
                    return;
                }
                panning.set(None);
            },
            onpointerleave: move |_| {
                slipping.set(None);
                panning.set(None);
            },
            // The wheel, on the shared bindings (`scroll::action_for`)
            // with the stack's meanings: horizontal zoom is the shared
            // time camera, vertical scroll is the lane stack. Pitch
            // zoom has no meaning over a stack of lanes and is ignored
            // rather than remapped — a gesture that does something
            // different per view is how schemes rot.
            onwheel: move |e: WheelEvent| {
                let (dx, dy) = crate::scroll::notches(&e.delta());
                let m = e.data().modifiers();
                let mods = expression_editor_core::Mods {
                    ctrl: m.contains(Modifiers::CONTROL) || m.contains(Modifiers::META),
                    shift: m.contains(Modifiers::SHIFT),
                    alt: m.contains(Modifiers::ALT),
                };
                let Some(action) = crate::scroll::action_for(dx, dy, mods) else {
                    return;
                };
                let (ax, _) =
                    wheel_anchor().unwrap_or((canvas::GUTTER_W + vp.w * 0.5, vp.h * 0.5));
                let x = (ax - canvas::GUTTER_W).max(0.0);
                let travel = if dx.abs() > dy.abs() { dx } else { dy };
                let factor = (travel.abs() / crate::interaction::ZOOM_DIVISOR).exp();
                let zoom_in = travel < 0.0;
                let mut ed = editor.write();
                match action.as_str() {
                    "view.hscroll" => {
                        ed.pan_px(-travel * crate::interaction::PAN_GAIN, 0.0);
                    }
                    "view.vscroll" => {
                        ed.stack_scroll =
                            (ed.stack_scroll + dy * crate::interaction::PAN_GAIN).max(0.0);
                    }
                    "view.zoom_h" | "view.zoom_both" => {
                        ed.zoom_time_at(x, if zoom_in { factor } else { 1.0 / factor });
                    }
                    _ => {}
                }
                drop(ed);
                e.prevent_default();
            },
            onpointerdown: move |e: PointerEvent| {
                let c = e.data().element_coordinates();
                // Middle-drag pans, the same as it does on the roll. The
                // stack is the view with the *most* to scroll — every
                // track at once — and it was the one view with no way to
                // move around at all.
                if matches!(e.trigger_button(), Some(MouseButton::Auxiliary)) {
                    panning.set(Some((c.x, c.y)));
                    return;
                }
                // The gutter's mic selector, ahead of every other
                // gesture: an open menu owns the next press, and a
                // press on a lane's chip opens its menu instead of
                // switching lanes.
                {
                    let ed = editor.read();
                    let views = self::lanes(&ed, ACTIVE_BOOST, ed.lane_floor().max(MIN_LANE));
                    drop(ed);
                    let ly = c.y - canvas::RULER_H;
                    if let Some(open) = mic_menu() {
                        enum Pick {
                            Mic(usize),
                            Solo,
                        }
                        let picked = views.iter().find(|l| l.lane == open).and_then(|l| {
                            let row = |i: usize| l.y + MIC_MENU_TOP + i as f64 * MIC_ITEM_H;
                            if c.x < 4.0 || c.x > 4.0 + MIC_MENU_W {
                                return None;
                            }
                            for (i, (ti, ..)) in l.members.iter().enumerate() {
                                if ly >= row(i) && ly < row(i) + MIC_ITEM_H {
                                    return Some(Pick::Mic(*ti));
                                }
                            }
                            let solo_row = row(l.members.len());
                            (ly >= solo_row && ly < solo_row + MIC_ITEM_H).then_some(Pick::Solo)
                        });
                        mic_menu.set(None);
                        match picked {
                            Some(Pick::Mic(track)) => {
                                editor.write().switch_track(track);
                            }
                            Some(Pick::Solo) => {
                                let mut ed = editor.write();
                                if let Some(l) = ed.tracks.layout_mut().lane_mut(open) {
                                    l.solo_mic = !l.solo_mic;
                                }
                            }
                            None => {}
                        }
                        return;
                    }
                    if let Some(l) = views.iter().find(|l| {
                        l.is_role
                            && l.members.len() > 1
                            && c.x >= 4.0
                            && c.x <= canvas::GUTTER_W
                            && ly >= l.y + MIC_CHIP_TOP
                            && ly < l.y + MIC_CHIP_TOP + MIC_ITEM_H
                    }) {
                        mic_menu.set(Some(l.lane));
                        return;
                    }
                }
                // A press near a role lane's hit line picks the hit up
                // for a slip/stretch instead of switching lanes. Only
                // where a host is listening — without a writer the
                // gesture would be a lie.
                // r[impl drums.manual.slip]
                if on_hit.is_some() {
                    let ed = editor.read();
                    let views = self::lanes(&ed, ACTIVE_BOOST, ed.lane_floor().max(MIN_LANE));
                    drop(ed);
                    let ly = c.y - canvas::RULER_H;
                    let lx = c.x - canvas::GUTTER_W;
                    let mods = e.data().modifiers();
                    // Two presses inside the window and the pick radius
                    // are a double click.
                    let now = std::time::Instant::now();
                    let double = last_press().is_some_and(|(t0, px, py)| {
                        now.duration_since(t0).as_millis() < 400
                            && (c.x - px).abs() <= SLIP_PICK_PX
                            && (c.y - py).abs() <= SLIP_PICK_PX
                    });
                    last_press.set(Some((now, c.x, c.y)));
                    let picked = views
                        .iter()
                        .filter(|l| l.is_role && ly >= l.y && ly < l.y + l.h)
                        .flat_map(|l| l.notes.iter().map(move |n| (l, n)))
                        .filter(|(_, n)| (n.x - lx).abs() <= SLIP_PICK_PX)
                        .min_by(|(_, a), (_, b)| {
                            (a.x - lx).abs().total_cmp(&(b.x - lx).abs())
                        })
                        .map(|(l, n)| {
                            let next = l
                                .notes
                                .iter()
                                .map(|m| m.at_secs)
                                .filter(|&t| t > n.at_secs + 1e-9)
                                .fold(f64::INFINITY, f64::min);
                            let prev = l
                                .notes
                                .iter()
                                .map(|m| m.at_secs)
                                .filter(|&t| t < n.at_secs - 1e-9)
                                .fold(f64::NEG_INFINITY, f64::max);
                            SlipDrag {
                                hit_secs: n.at_secs,
                                prev_secs: prev,
                                next_secs: next,
                                lane_name: l.name.clone(),
                                x0: c.x,
                                x: c.x,
                                shift: mods.contains(Modifiers::SHIFT),
                                hit_x: n.x,
                                lane_y: l.y,
                                lane_h: l.h,
                            }
                        });
                    if let Some(s) = picked {
                        let sel = SelectedHit {
                            lane_name: s.lane_name.clone(),
                            hit_secs: s.hit_secs,
                            prev_secs: s.prev_secs,
                            next_secs: s.next_secs,
                            lane_y: s.lane_y,
                            lane_h: s.lane_h,
                        };
                        // Double-click snaps the hit to its nearest
                        // division — the fastest way to fix one hit
                        // without opening the panel.
                        // r[impl drums.manual.nudge]
                        if double && grid_secs > 0.0 {
                            let target = (s.hit_secs / grid_secs).round() * grid_secs;
                            let delta = target - s.hit_secs;
                            if delta.abs() > 1e-9 {
                                emit_move(&sel, delta, false);
                            }
                            let mut sel = sel;
                            sel.hit_secs = target;
                            selected.set(Some(sel));
                            return;
                        }
                        slipping.set(Some(s));
                        return;
                    }
                    // Alt+click on empty role-lane audio adds a hit
                    // where the click meant — the host refines it to
                    // the nearest attack. The hit list changes; the daw
                    // does not, until a drag or Apply.
                    // r[impl drums.manual.add-remove]
                    if mods.contains(Modifiers::ALT) && px_per_sec > 0.0 {
                        let lane = views
                            .iter()
                            .find(|l| l.is_role && ly >= l.y && ly < l.y + l.h);
                        if let (Some(l), Some(h)) = (lane, &on_hit) {
                            h.call(HitGesture::Add {
                                lane: l.name.clone(),
                                at: view0 + lx / px_per_sec,
                            });
                            return;
                        }
                    }
                }
                let y = c.y - canvas::RULER_H + editor.read().stack_scroll;
                // Resolve against a snapshot: the read guard has to be
                // gone before the write below.
                let hit = {
                    let ed = editor.read();
                    let rows = ed
                        .tracks
                        .stack(ed.viewport.h as f32, ACTIVE_BOOST, ed.lane_floor().max(MIN_LANE));
                    // `row_at` resolves to a *lane*. Clicking targets a
                    // lane, never a track within one: with a vocal and
                    // its guide a few pixels apart, picking by proximity
                    // silently edits the reference you were tuning
                    // against. If the click lands on the lane you are
                    // already in, nothing changes — cycling within a
                    // lane is a key, not a click.
                    expression_editor_core::tracks::Workspace::row_at(&rows, y as f32)
                        .filter(|&lane| Some(lane) != ed.tracks.active_lane())
                        .and_then(|lane| {
                            ed.tracks
                                .lane_tracks(lane)
                                .into_iter()
                                .find(|&i| ed.tracks.track(i).is_some_and(|t| !t.hidden))
                        })
                };
                if let Some(track) = hit {
                    editor.write().switch_track(track);
                }
            },

            // The stack sits on the roll's own ink — the darkest step,
            // so the lanes read as material on a desk rather than
            // panels on a panel.
            rect {
                x: 0, y: 0,
                width: "{vp.w + canvas::GUTTER_W}",
                height: "{vp.h + canvas::RULER_H}",
                fill: theme::GUTTER_BG,
            }

            // One ruler for the whole stack — the shared axis is the
            // reason the view exists, so it is drawn once rather than
            // per lane. Bar starts get their number; beats get a short
            // tick, exactly like the roll's ruler.
            rect {
                x: 0, y: 0,
                width: "{vp.w + canvas::GUTTER_W}",
                height: "{canvas::RULER_H}",
                fill: theme::SURFACE_BAR,
            }
            g {
                transform: "translate({canvas::GUTTER_W}, 0)",
                // The section strip: the top half of the ruler is the
                // song's own map — INTRO, VS 1, CH 1 — in the colours
                // the arrange view already taught the band.
                for (x0, x1, label, color) in sections.iter() {
                    rect {
                        x: "{x0:.1}", y: 0,
                        width: "{(x1 - x0).max(0.0):.1}",
                        height: "{canvas::RULER_H - 13.0}",
                        fill: "{color}",
                        opacity: "0.85",
                    }
                    line {
                        x1: "{x0:.1}", x2: "{x0:.1}",
                        y1: 0, y2: "{canvas::RULER_H - 13.0}",
                        stroke: theme::GUTTER_BG,
                        stroke_width: 1,
                    }
                    if !label.is_empty() {
                        text {
                            x: "{x0 + 4.0:.1}", y: 11,
                            font_size: "8",
                            fill: "#0b0b10",
                            "{label}"
                        }
                    }
                }
                for t in ticks.iter() {
                    line {
                        x1: "{t.x:.1}", x2: "{t.x:.1}",
                        y1: if t.bar { "{canvas::RULER_H - 10.0}" } else { "{canvas::RULER_H - 5.0}" },
                        y2: "{canvas::RULER_H}",
                        stroke: if t.bar { theme::TEXT_DIM } else { theme::TEXT_FAINT },
                        stroke_width: 1,
                    }
                    if let Some(label) = t.label.as_ref() {
                        text {
                            x: "{t.x + 3.0:.1}", y: "{canvas::RULER_H - 4.0}",
                            font_size: "8",
                            fill: theme::TEXT_DIM,
                            "{label}"
                        }
                    }
                }
            }

            g {
                transform: "translate(0, {canvas::RULER_H})",
                for lane in lanes.iter() {
                    g {
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
                            for (x0, _, _, color) in sections.iter() {
                                line {
                                    x1: "{x0:.1}", x2: "{x0:.1}",
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
                                    points: "{w}",
                                    fill: lane.role_color.unwrap_or(theme::PEAKS),
                                    opacity: if lane.active { "0.5" } else { "0.32" },
                                }
                            }
                            // r[impl drums.lanes.toms-split]
                            for s in lane.sub_lanes.iter() {
                                if let Some(p) = s.points.as_ref() {
                                    polygon {
                                        points: "{p}",
                                        fill: lane.role_color.unwrap_or(theme::PEAKS),
                                        opacity: if s.faded { "0.10" } else { "0.32" },
                                    }
                                }
                            }
                            for d in lane.dividers.iter() {
                                line {
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
                                        if n.hit_line {
                                            // r[impl drums.lanes.hits]
                                            line {
                                                x1: "{n.x:.1}", y1: "{gy:.1}",
                                                x2: "{n.x:.1}", y2: "{gy + gh:.1}",
                                                stroke: "{n.fill}",
                                                stroke_width: "1.5",
                                                opacity: "0.9",
                                            }
                                            polygon {
                                                points: "{n.x:.1},{gy:.1} {n.x + n.w.min(9.0):.1},{gy + 5.0:.1} {n.x:.1},{gy + 10.0:.1}",
                                                fill: "{n.fill}",
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
            if let Some(open) = mic_menu() {
                if let Some(lane) = lanes.iter().find(|l| l.lane == open) {
                    g {
                        transform: "translate(0, {canvas::RULER_H})",
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

            // The transport's playhead, over every lane. A leaf
            // component: position ticks arrive many times a second
            // while playing and must move one line, not re-render the
            // stack.
            if let Some(ph) = playhead_secs {
                if px_per_sec > 0.0 {
                    StackPlayhead {
                        playhead: ph,
                        view0,
                        px_per_sec,
                        height: vp.h,
                    }
                }
            }

            // The selected hit — the unit of keyboard editing — marked
            // with a bracket at the lane's top edge so it reads against
            // the hit line without hiding it.
            // r[impl drums.manual.nudge]
            if let Some(sel) = selected() {
                if px_per_sec > 0.0 {
                    g {
                        transform: "translate({canvas::GUTTER_W}, {canvas::RULER_H})",
                        {
                            let x = (sel.hit_secs - view0) * px_per_sec;
                            rsx! {
                                line {
                                    x1: "{x:.1}", x2: "{x:.1}",
                                    y1: "{sel.lane_y:.1}", y2: "{sel.lane_y + sel.lane_h:.1}",
                                    stroke: theme::ACCENT, stroke_width: 1,
                                    opacity: "0.8",
                                }
                                rect {
                                    x: "{x - 3.0:.1}", y: "{sel.lane_y:.1}",
                                    width: 6, height: 4,
                                    fill: theme::ACCENT,
                                }
                            }
                        }
                    }
                }
            }

            // The drag's ghost: the hit's origin stays dim while the
            // dragged line follows the pointer, so the gesture reads as
            // "this hit is moving there".
            // r[impl drums.manual.slip]
            // r[impl drums.manual.stretch]
            if let Some(s) = slipping() {
                g {
                    transform: "translate({canvas::GUTTER_W}, {canvas::RULER_H})",
                    line {
                        x1: "{s.hit_x:.1}", x2: "{s.hit_x:.1}",
                        y1: "{s.lane_y:.1}", y2: "{s.lane_y + s.lane_h:.1}",
                        stroke: theme::TEXT_DIM, stroke_width: 1,
                        opacity: "0.5",
                    }
                    line {
                        x1: "{s.hit_x + (s.x - s.x0):.1}", x2: "{s.hit_x + (s.x - s.x0):.1}",
                        y1: "{s.lane_y:.1}", y2: "{s.lane_y + s.lane_h:.1}",
                        stroke: theme::ACCENT, stroke_width: 2,
                    }
                }
            }
        }
        }
    }
}

/// The playhead line, isolated so a transport tick re-renders one
/// element. Rendered inside the stack's svg, in content coordinates.
#[component]
fn StackPlayhead(playhead: Signal<f64>, view0: f64, px_per_sec: f64, height: f64) -> Element {
    let x = canvas::GUTTER_W + (playhead() - view0) * px_per_sec;
    if x < canvas::GUTTER_W {
        return rsx! {};
    }
    rsx! {
        line {
            x1: "{x:.1}", x2: "{x:.1}",
            y1: "{canvas::RULER_H}",
            y2: "{canvas::RULER_H + height:.1}",
            stroke: "#f8fafc",
            stroke_width: 1,
            opacity: "0.7",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::summed_columns;

    // r[verify drums.lanes.summed]
    #[test]
    fn summed_columns_means_members_and_normalises() {
        // Two members over the same 4-second span, viewed whole in four
        // columns: column values are the per-column MEAN of the two,
        // then scaled so the loudest column is exactly 1.0.
        let a = [1.0f32, 1.0, 0.0, 0.0];
        let b = [0.0f32, 1.0, 0.0, 0.0];
        let cols = summed_columns(&[(&a, 0.0, 4.0), (&b, 0.0, 4.0)], 0.0, 4.0, 4);
        assert_eq!(cols, vec![0.5, 1.0, 0.0, 0.0]);
        assert_eq!(cols.iter().copied().fold(0.0f32, f32::max), 1.0);
    }

    // r[verify drums.lanes.summed]
    #[test]
    fn summed_columns_skips_empty_members_and_scales_one() {
        // A member with no peaks is skipped, not averaged in as silence.
        let a = [1.0f32, 1.0, 0.0, 0.0];
        let none: [f32; 0] = [];
        let cols = summed_columns(&[(&a, 0.0, 4.0), (&none, 0.0, 4.0)], 0.0, 4.0, 4);
        assert_eq!(cols, vec![1.0, 1.0, 0.0, 0.0]);

        // A single quiet member still fills the lane: normalised so its
        // own loudest column is 1.0.
        let quiet = [0.2f32, 0.4];
        let cols = summed_columns(&[(&quiet, 0.0, 2.0)], 0.0, 2.0, 2);
        assert_eq!(cols, vec![0.5, 1.0]);
    }

    // r[verify drums.lanes.summed]
    #[test]
    fn summed_columns_is_silent_outside_a_member() {
        // A member spanning only the first half of the view contributes
        // silence to columns past its end.
        let a = [1.0f32, 1.0];
        let cols = summed_columns(&[(&a, 0.0, 2.0)], 0.0, 4.0, 4);
        assert_eq!(cols, vec![1.0, 1.0, 0.0, 0.0]);
    }
}
