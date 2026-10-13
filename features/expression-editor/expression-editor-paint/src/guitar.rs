//! **Prototype (#161).** Bend flow on a six-string roll.
//!
//! Throwaway by design: this exists to be *looked at*, so that four
//! questions can be answered by picture rather than by argument —
//!
//! 1. do bends draw **on** the string row, deflecting the line, or in a
//!    dimension below?
//! 2. do slides and hammer-ons/pull-offs use the same curve mechanism,
//!    or distinct glyphs?
//! 3. does [`RowSpace::Strings`] survive contact with real tab?
//! 4. at what zoom does it stop being readable, and what degrades
//!    first?
//!
//! The one structural claim it makes is this: **in a string roll the
//! vertical axis carries no pitch.** A row is a string; the fret is a
//! label inside the note. So a bend has to invent its own vertical
//! scale ([`RowSpace::semitones_per_row`]) and a fret *change* — a
//! slide, a hammer-on — has no vertical distance to travel at all, even
//! though it is exactly the same pitch motion. Everything awkward below
//! follows from that.

use expression_editor_core::doc::Note;
use expression_editor_core::{Editor, RowSpace};

use crate::theme;

/// Where the pitch motion of a guitar part gets drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BendFlow {
    /// On the string row: the line lifts off its string and comes back.
    #[default]
    OnRow,
    /// In a dedicated strip under the roll, on an absolute semitone
    /// axis. The roll stays a clean grid of fret numbers.
    Lane,
    /// Both at once — the honest way to find out whether either one
    /// makes the other redundant.
    Both,
}

impl BendFlow {
    pub fn on_row(self) -> bool {
        matches!(self, BendFlow::OnRow | BendFlow::Both)
    }
    pub fn draws_in_lane(self) -> bool {
        matches!(self, BendFlow::Lane | BendFlow::Both)
    }
}

/// Samples per bend curve. A bend is two to five authored points but
/// reads as a *shape*, so it is resampled rather than drawn as its own
/// control polygon.
const SAMPLES: usize = 64;

/// One note's pitch motion, ready to stroke.
pub struct FlowPath {
    pub points: String,
    pub color: &'static str,
    /// Peak deflection in semitones — what the readout labels.
    pub peak: f64,
    /// How a player would say it. `None` when the line never moved
    /// enough to be worth a number.
    pub peak_label: Option<String>,
    /// Where to hang that readout.
    pub peak_at: (f64, f64),
    pub selected: bool,
}

/// A bend height in the words a guitarist uses.
///
/// "Full" and "half" are the datum; the curve is only how it got there.
/// Anything off those two lands as a signed number, which is itself a
/// finding: quarter-tone bends have no name and read as noise.
fn peak_label(peak: f64) -> Option<String> {
    let a = peak.abs();
    if a < 0.25 {
        return None;
    }
    Some(if (a - 2.0).abs() < 0.15 {
        "full".into()
    } else if (a - 1.0).abs() < 0.15 {
        "½".into()
    } else if (a - 3.0).abs() < 0.15 {
        "1½".into()
    } else {
        format!("{peak:+.1}")
    })
}

/// Whether this editor is showing a string roll at all.
pub fn is_string_roll(ed: &Editor) -> bool {
    matches!(ed.row_space, RowSpace::Strings(_))
}

/// The colour of a note, by the string it is fingered on.
///
/// A row is a pitch and several strings reach it, so the colour comes
/// from the note rather than its row — `row_color` returns `None` for a
/// string roll for exactly that reason.
fn string_color(ed: &Editor, note: &expression_editor_core::Note) -> &'static str {
    match note.string {
        Some(s) if ed.color_by_string => expression_editor_core::rows::string_color(s),
        _ => theme::PITCH_TRACK,
    }
}

/// Sample a note's pitch curve as `(t, semitones)` pairs.
///
/// A note with no authored curve still yields two points, so a plain
/// fretted note draws as a flat segment of the same line as a bent one
/// — which is the whole argument for "flow": one continuous reading of
/// the string, not two kinds of mark.
fn samples(n: &Note) -> Vec<(f64, f64)> {
    if n.pitch.is_empty() {
        return vec![(n.start, 0.0), (n.end, 0.0)];
    }
    (0..SAMPLES)
        .map(|i| {
            let f = i as f64 / (SAMPLES - 1) as f64;
            let t = n.start + (n.end - n.start) * f;
            (t, n.pitch.sample(t, 0.0))
        })
        .collect()
}

/// The on-row flow: each note's line, lifted off its string by the bend.
pub fn flow_paths(ed: &Editor) -> Vec<FlowPath> {
    if !is_string_roll(ed) {
        return Vec::new();
    }
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let spr = ed.row_space.semitones_per_row();
    ed.doc
        .notes
        .iter()
        .filter(|n| n.end >= t0 && n.start <= t1)
        .map(|n| {
            let pts = samples(n);
            let mut s = String::new();
            let mut peak = 0.0f64;
            let mut peak_at = (0.0, 0.0);
            for &(t, v) in &pts {
                let x = ed.camera.x(t);
                // Bend is *up* from the string, so the line rises: the
                // sign convention has to match the physical gesture or
                // nobody trusts the picture.
                let y = ed.camera.y(n.row as f64 + 0.5 + v / spr, ed.viewport);
                if v.abs() > peak.abs() {
                    peak = v;
                    peak_at = (x, y);
                }
                s.push_str(&format!("{x:.1},{y:.1} "));
            }
            FlowPath {
                points: s,
                color: string_color(ed, n),
                peak,
                peak_label: peak_label(peak),
                peak_at,
                selected: ed.selection.contains(n.id),
            }
        })
        .collect()
}

/// How two consecutive notes on one string are joined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinKind {
    /// Hammer-on / pull-off: an arc plus a letter, the way tab writes
    /// it. There is no continuous pitch motion to draw — the pitch just
    /// changes — so a curve here would be a lie.
    Hopo(&'static str),
    /// Slide: real, continuous, audible pitch motion between two frets.
    Slide,
}

/// A join between two notes on the same string.
pub struct Join {
    pub kind: JoinKind,
    /// The connector path (arc for a hopo, straight for a slide).
    pub d: String,
    /// Where the letter goes.
    pub label_at: (f64, f64),
    pub color: &'static str,
}

/// Joins between consecutive notes on the same string.
///
/// This is where the row model bites. A slide from fret 5 to fret 7 is
/// two semitones of continuous pitch motion. When the string was the
/// row this connector was dead horizontal and showed none of it, and
/// the only way to draw the motion was to let the origin note's own
/// bend curve carry it and snap back — which read as a bend, not a
/// slide. Now that a row is a pitch the two notes sit at different
/// heights and the connector slopes, which is what a slide looks like.
pub fn joins(ed: &Editor) -> Vec<Join> {
    use expression_editor_core::Articulation;
    if !is_string_roll(ed) {
        return Vec::new();
    }
    let spr = ed.row_space.semitones_per_row();
    let mut by_string: std::collections::BTreeMap<u8, Vec<&Note>> = Default::default();
    for n in &ed.doc.notes {
        // By string, not by row. The row is the sounding pitch now, so
        // a slide from the 5th to the 7th fret is two rows and would
        // never pair.
        let Some(string) = n.string else { continue };
        by_string.entry(string).or_default().push(n);
    }
    let mut out = Vec::new();
    for (_string, mut notes) in by_string {
        notes.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
        for pair in notes.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let Some(art) = a.articulation else { continue };
            let kind = match art {
                Articulation::HammerOn => JoinKind::Hopo("H"),
                Articulation::PullOff => JoinKind::Hopo("P"),
                Articulation::LegatoSlide | Articulation::SlideGuitar | Articulation::SlideOut => {
                    JoinKind::Slide
                }
                _ => continue,
            };
            let x0 = ed.camera.x(a.end);
            let x1 = ed.camera.x(b.start);
            // Where each end of the join actually sits: the origin's
            // last bend value, the target's first.
            let ya = ed.camera.y(
                a.row as f64 + 0.5 + a.pitch.sample(a.end, 0.0) / spr,
                ed.viewport,
            );
            let yb = ed.camera.y(
                b.row as f64 + 0.5 + b.pitch.sample(b.start, 0.0) / spr,
                ed.viewport,
            );
            let mid = ((x0 + x1) * 0.5, (ya + yb) * 0.5);
            let d = match kind {
                JoinKind::Hopo(_) => {
                    let lift = (ed.camera.vertical.px_per_row * 0.55).clamp(5.0, 16.0);
                    format!(
                        "M {x0:.1} {ya:.1} Q {:.1} {:.1} {x1:.1} {yb:.1}",
                        mid.0,
                        mid.1 - lift
                    )
                }
                JoinKind::Slide => format!("M {x0:.1} {ya:.1} L {x1:.1} {yb:.1}"),
            };
            out.push(Join {
                kind,
                d,
                label_at: (mid.0, mid.1 - 6.0),
                color: string_color(ed, a),
            });
        }
    }
    out
}

// ── the dimension variant ─────────────────────────────────────────────────

/// Full-scale of the bend dimension, in semitones either way. Three is a
/// tone-and-a-half up and a whammy dive down; GP's own ceiling is six,
/// but a dimension scaled to six makes a half-step bend invisible.
pub const LANE_SEMITONES: f64 = 3.0;

/// The bend dimension: every string's pitch motion on one absolute axis.
pub struct BendLaneView {
    pub y: f64,
    pub h: f64,
    /// Horizontal guides: `(y, label)` at whole and half steps.
    pub guides: Vec<(f64, &'static str)>,
    pub paths: Vec<FlowPath>,
}

/// Height the dimension takes out of the roll.
pub fn lane_height(vp_h: f64) -> f64 {
    (vp_h * 0.28).clamp(48.0, 130.0)
}

pub fn bend_lane(ed: &Editor) -> Option<BendLaneView> {
    if !is_string_roll(ed) {
        return None;
    }
    let h = lane_height(ed.viewport.h);
    // Below the lowest string if the roll leaves room, pinned to the
    // bottom otherwise. A dimension that covers the low E is not a dimension, it
    // is a second thing competing for the same pixels — and whether
    // there is room is exactly the cost this variant has to be judged
    // on.
    let (rlo, _) = ed.row_space.bounds();
    let below = ed.camera.y(rlo as f64 - 0.5, ed.viewport) + 6.0;
    let y = below.min(ed.viewport.h - h).max(0.0);
    let mid = y + h * 0.5;
    let to_y = |v: f64| mid - (v / LANE_SEMITONES).clamp(-1.0, 1.0) * h * 0.5;

    let guides = vec![
        (to_y(2.0), "full"),
        (to_y(1.0), "½"),
        (mid, "0"),
        (to_y(-1.0), "½"),
    ];

    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let paths = ed
        .doc
        .notes
        .iter()
        .filter(|n| n.end >= t0 && n.start <= t1)
        // Only notes that actually move. A flat line per fretted note
        // would fill the dimension with six overlapping horizontals at zero
        // and bury the one thing it exists to show.
        .filter(|n| !n.pitch.is_empty())
        .map(|n| {
            let mut s = String::new();
            let mut peak = 0.0f64;
            let mut peak_at = (0.0, 0.0);
            for (t, v) in samples(n) {
                let (x, py) = (ed.camera.x(t), to_y(v));
                if v.abs() > peak.abs() {
                    peak = v;
                    peak_at = (x, py);
                }
                s.push_str(&format!("{x:.1},{py:.1} "));
            }
            FlowPath {
                points: s,
                color: string_color(ed, n),
                peak,
                peak_label: peak_label(peak),
                peak_at,
                selected: ed.selection.contains(n.id),
            }
        })
        .collect();

    Some(BendLaneView {
        y,
        h,
        guides,
        paths,
    })
}
