//! Lane layout and time conversion, independent of component state.
use super::waveform::{columns_polygon, summed_columns};
use crate::theme;
use expression_editor_core::doc::{ExpressionDoc, Note};
use expression_editor_core::{Editor, Mode, kit, rows::RowSpace, tracks::StackRow};
const LANE_PAD: f64 = 3.0;
/// How far past the view a hit is still laid out — the painter's own
/// margin, so a marker sliding in from the edge is already there.
const HIT_MARGIN_PX: f64 = 96.0;
const FIT_PAD: f64 = 1.0;
/// A closed polygon in lane space — a waveform's outline, top edge
/// left to right then bottom edge back.
///
/// Points, not the `"x,y x,y"` string they used to be: the painter
/// turned that string straight back into numbers, twice per lane per
/// frame, and a renderer that wants the attribute formats it once.
pub type Polygon = Vec<(f64, f64)>;

/// The svg `points` attribute for a polygon.
#[must_use]
pub fn points_attr(polygon: &[(f64, f64)]) -> String {
    let mut s = String::with_capacity(polygon.len().saturating_mul(12));
    for (i, (x, y)) in polygon.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        use std::fmt::Write as _;
        let _ = write!(s, "{x:.1},{y:.1}");
    }
    s
}

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
    /// other).
    pub is_role: bool,
    /// Whether hits are detected and edited on this lane — kick,
    /// snare and toms. The other lane is context and takes no hit
    /// gesture.
    pub detects: bool,
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
    /// [`crate::canvas::take_waveform`] builds for the roll. `None` when the
    /// lane has no role, no members with peaks, or splits its members.
    pub waveform: Option<Polygon>,
    /// Trigger tracks drawn over `waveform` in the same space, rather
    /// than averaged into it. Empty for a split lane, whose triggers
    /// ride on their own tom's sub-row instead.
    pub overlays: Vec<Polygon>,
    /// One sub-row per member for a split role lane (toms). Empty
    /// otherwise.
    pub sub_lanes: Vec<SubLane>,
    /// Stroke width for this lane's hit markers, thinned as they crowd.
    pub hit_width: f64,
    /// Whether hit markers still draw their onset flag — dropped once
    /// the flags would overlap into a solid band.
    pub hit_flag: bool,
}

/// One member's sub-row inside a split role lane (toms).
pub struct SubLane {
    /// The member's own waveform polygon, if it carries peaks.
    pub points: Option<Polygon>,
    /// Triggers drawn over this member in the same space — a trigger is
    /// the same drum sensed a second way, not another drum.
    pub overlays: Vec<Polygon>,
    /// The member track's name, drawn small in the gutter.
    pub label: String,
    /// Baseline y of that label, in viewport pixels.
    pub label_y: f64,
    /// Unused or hidden members draw at half opacity.
    pub faded: bool,
}

/// The ruler's shelves. One per (ruler lane, kind) pair actually in
/// use, ordered by lane, regions above markers within a lane.
///
/// Keyed by kind as well as lane because REAPER lets both live on one
/// lane and these projects do exactly that: `The ballad` files 15
/// regions *and* 2 markers on `SONG`. Grouping by lane alone drew
/// them on the same shelf, where a marker tick lands inside a region
/// band and the two fight for the same pixels. They are different
/// things — a span and a point — and they get different rows.
/// r[impl drums.chrome.markers]
pub fn chrome_shelves(ed: &Editor) -> Vec<(Option<u32>, bool, String)> {
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let mut seen: Vec<(Option<u32>, bool, String)> = Vec::new();
    let mut note = |lane: &Option<(u32, String)>, is_region: bool| {
        let key = lane.as_ref().map(|(i, _)| *i);
        if !seen.iter().any(|(i, r, _)| *i == key && *r == is_region) {
            let name = lane.as_ref().map_or(String::new(), |(_, n)| n.clone());
            seen.push((key, is_region, name));
        }
    };
    for r in ed.doc.regions.iter().filter(|r| r.end > t0 && r.start < t1) {
        note(&r.lane, true);
    }
    for m in ed.doc.markers.iter().filter(|m| m.t >= t0 && m.t <= t1) {
        note(&m.lane, false);
    }
    // Lane order first, then regions above markers within a lane.
    seen.sort_by_key(|(i, is_region, _)| (i.unwrap_or(0), !*is_region));
    seen
}

/// Height of the ruler for `ed` — its shelves plus the tick strip.
///
/// The lanes, the playhead and the pointer maths that turns a click into
/// a lane all take their offset from this. They must take it from the
/// *same* place: if the layout grows a shelf and the hit-testing does
/// not, the lanes and the mouse end up in different coordinate systems,
/// which shows up as clicks landing on the wrong lane rather than as
/// anything visibly wrong.
pub fn ruler_height(ed: &Editor) -> f64 {
    CHROME_ROW_H * chrome_shelves(ed).len().max(1) as f64 + RULER_TICKS_H
}

/// Height of one ruler shelf — a region row or a marker row.
///
/// Matches the fixed band the ruler used before shelves existed, so a
/// project with a single lane of chrome renders identically to how it
/// always did; extra shelves grow the ruler rather than shrinking each
/// other into illegibility.
pub const CHROME_ROW_H: f64 = 15.0;
/// Height of the tick/timecode strip below the shelves.
pub const RULER_TICKS_H: f64 = 13.0;

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
    /// Which member track this hit was detected on. In a split lane it
    /// is what puts the marker in that member's own sub-row, so a tom
    /// hit says *which tom* rather than "a tom".
    pub member: usize,
}

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
        let from = notes.len();
        // Only what can be seen, plus the margin the painter draws
        // past the edges. A song's worth of hits is thousands of
        // notes, and at a four-bar zoom a hundred of them are on
        // screen; converting the rest was most of the frame.
        let (t0, t1) = ed.camera.time_span(ed.viewport);
        let margin = HIT_MARGIN_PX * ed.camera.units_per_px;
        let in_view = |n: &&Note| {
            to_editor_time(ed, member_doc, n.end) >= t0 - margin
                && to_editor_time(ed, member_doc, n.start) <= t1 + margin
        };
        notes.extend(member_doc.notes.iter().filter(in_view).map(|n| {
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
        for n in &mut notes[from..] {
            n.member = i;
        }
    }

    // A role lane (kick / snare / toms / other) is labelled by its role
    // and drawn as its members' audio, not by any one member's row
    // space — so the row-space guides give way to the waveform.
    let lane_def = ed.tracks.layout().lane(row.lane);
    let role = lane_def.and_then(|l| l.role);
    let role_split = role.is_some() && lane_def.is_some_and(|l| l.split);

    // The split lane's sub-rows, resolved once: the waveform draws them
    // and the hit markers are placed into them, and the two must agree
    // or a marker would sit over the wrong tom — worse than the
    // full-height marker it replaces, because it would be confidently
    // wrong rather than merely vague.
    // r[impl drums.lanes.trigger-overlay]
    let sub_rows: Vec<(usize, Vec<usize>)> = if role_split {
        let all = ed.tracks.lane_tracks(row.lane);
        let names: Vec<String> = all
            .iter()
            .map(|&i| {
                ed.tracks
                    .track(i)
                    .map(|t| t.name.clone())
                    .unwrap_or_default()
            })
            .collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        kit::trigger_sub_rows(&refs)
            .into_iter()
            .map(|(h, o)| (all[h], o.into_iter().map(|i| all[i]).collect()))
            .collect()
    } else {
        Vec::new()
    };
    // Which sub-row a member draws in — its own, or the tom it
    // triggers. `None` when the lane is not split.
    let sub_row_of = |member: usize| -> Option<usize> {
        sub_rows
            .iter()
            .position(|(host, over)| *host == member || over.contains(&member))
    };

    let mut waveform = None;
    let mut overlays: Vec<Polygon> = Vec::new();
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
            let shown: Vec<usize> = members
                .iter()
                .copied()
                .filter(|&i| !solo || i == track_index)
                .collect();
            // r[impl drums.lanes.trigger-overlay]
            //
            // A kick or snare trigger is the same drum sensed a second
            // way, so it is drawn over the mics' mean rather than
            // averaged into it: a trigger is near-silent between hits
            // and would only drag the mean down.
            let is_trig = |i: usize| {
                ed.tracks
                    .track(i)
                    .is_some_and(|t| kit::is_trigger_name(&t.name))
            };
            let peaks_of = |i: usize| {
                let d = doc_for(ed, i)?;
                let (s, e) = doc_span_secs(ed, d)?;
                Some((d.peaks.as_slice(), s, e))
            };
            let (trigs, mics): (Vec<usize>, Vec<usize>) = shown.iter().partition(|&&i| is_trig(i));
            // A lane of nothing but triggers still has to draw something,
            // and then the triggers are the waveform — not an overlay on
            // top of an empty one.
            let (summed, over): (&[usize], &[usize]) = if mics.is_empty() {
                (&trigs, &[])
            } else {
                (&mics, &trigs)
            };

            let mems: Vec<(&[f32], f64, f64)> =
                summed.iter().filter_map(|&i| peaks_of(i)).collect();
            let cols = summed_columns(&mems, v0, v1, count);
            waveform = columns_polygon(&cols, ed.viewport.w, y0, h);
            overlays = over
                .iter()
                .filter_map(|&i| {
                    let cols = summed_columns(&[peaks_of(i)?], v0, v1, count);
                    columns_polygon(&cols, ed.viewport.w, y0, h)
                })
                .collect();
        } else if role_split {
            // r[impl drums.lanes.toms-split]
            //
            // Every member gets a sub-row, hidden and `Unused` ones
            // included — a tom that is parked still holds its place in
            // the kit, it just draws faded.
            let rows = &sub_rows;
            let k = rows.len().max(1);
            let sub_h = h / k as f64;
            for (j, (i, overlays)) in rows.iter().enumerate() {
                let Some(member) = ed.tracks.track(*i) else {
                    continue;
                };
                let sy = y0 + sub_h * j as f64;
                if j > 0 {
                    sub_dividers.push(sy);
                }
                let polygon_for = |i: usize| {
                    let d = doc_for(ed, i)?;
                    let (s, e) = doc_span_secs(ed, d)?;
                    let cols = summed_columns(&[(d.peaks.as_slice(), s, e)], v0, v1, count);
                    columns_polygon(&cols, ed.viewport.w, sy, sub_h)
                };
                sub_lanes.push(SubLane {
                    points: polygon_for(*i),
                    // Triggers share the tom's sub-row, drawn over it in
                    // the same space rather than beside it.
                    overlays: overlays.iter().filter_map(|&o| polygon_for(o)).collect(),
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
    //
    // Marker weight is a function of how crowded they are. A fixed
    // 1.5px line with a 9px flag is right for a handful of hits and
    // useless for a song's worth: zoomed out to a whole take, sixteenth
    // kicks land a couple of pixels apart, and the markers merge into a
    // solid bar that hides the very waveform they annotate. Thinning
    // them with density keeps the same drawing legible at both ends —
    // and thick markers stay thick when there is room for them, which
    // is the case they are good at.
    // r[impl drums.lanes.hit-density]
    let visible = notes
        .iter()
        .filter(|n| n.x >= 0.0 && n.x <= ed.viewport.w)
        .count();
    // Average px between markers across the viewport.
    let spacing = if visible > 1 {
        ed.viewport.w / visible as f64
    } else {
        ed.viewport.w
    };
    // Floored at one whole pixel. Below that the rasterizer cannot draw
    // a line, only a fraction of one: a 0.4px marker comes out as ~40%
    // alpha and washes into the waveform behind it, which is the
    // problem being solved, not the fix. One crisp pixel is the thinnest
    // *visible* marker, so that is the floor.
    let hit_width = (spacing / 8.0).clamp(1.0, 2.5);
    // The flag is 9px wide; below roughly that spacing the flags overlap
    // into a band and read as fill rather than as onsets.
    let hit_flag = spacing >= 14.0;

    if let Some(role) = role {
        let color = role.color();
        // In a split lane a marker is confined to the sub-row of the
        // drum it was detected on, so the picture answers *which tom*.
        // Detection is already per tom; drawing every hit across the
        // whole lane threw that answer away at the last step.
        // r[impl drums.lanes.hits-per-sub-row]
        let sub_h = h / sub_rows.len().max(1) as f64;
        for n in &mut notes {
            n.hit_line = true;
            match sub_row_of(n.member) {
                Some(sub) => {
                    n.y = y0 + sub_h * sub as f64;
                    n.h = sub_h;
                }
                None => {
                    n.y = y0;
                    n.h = h;
                }
            }
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
        detects: role.is_some_and(kit::LaneRole::detects),
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
        overlays,
        sub_lanes,
        hit_width,
        hit_flag,
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
        member: 0,
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
pub fn view_span_secs(ed: &Editor) -> Option<(f64, f64)> {
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
