//! The canvas camera: time/pitch ↔ pixel mapping, zoom, and magnets.
//!
//! MPElodyne's own notes on this (`View Magnets.md`) end with a list of
//! "likely roughness sources", and every one of them has the same
//! cause: rules that *mutate an already-produced camera in sequence*,
//! so a later rule fights an earlier one every frame.
//!
//! This module inverts that. A gesture produces one base camera, then
//! declares its magnets as weighted [`Influence`]s — candidate cameras,
//! not mutations. [`blend`] resolves them in a single pass (log-space
//! for scales so zoom stays perceptually even, linear for positions),
//! and [`Camera::constrain`] clamps once at the end. Two magnets
//! pulling in opposite directions now average smoothly instead of
//! alternating.

use crate::shape::{smoothstep, smoothstep_between};

/// Canvas size in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub w: f64,
    pub h: f64,
}

impl Viewport {
    pub fn new(w: f64, h: f64) -> Self {
        Self {
            w: w.max(1.0),
            h: h.max(1.0),
        }
    }
}

/// The vertical half of a camera, owned by one lane.
///
/// Time is shared across every lane and stays on [`Camera`] — two
/// instruments doubling a line are only comparable on a common time
/// axis, and that invariant is enforced structurally here rather than
/// by convention, so no stray call can break it.
///
/// Vertical is per lane, because that is the whole feature: a bass and a
/// piccolo are both readable only if each lane fits its own range.
///
/// **Ephemeral.** Never persisted — re-fitted on load, which is what
/// makes it free to keep out of the project file. See #192's ephemeral
/// bucket.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VerticalCamera {
    /// Row at the vertical centre of the lane.
    pub center: f64,
    /// Height of one row, in pixels.
    pub px_per_row: f64,
}

impl VerticalCamera {
    /// Fit a row range into a lane of `height` pixels.
    pub fn fitted(lo: f64, hi: f64, height: f64) -> Self {
        let span = (hi - lo).max(1e-6);
        Self {
            center: (lo + hi) / 2.0,
            px_per_row: (height / span).max(1e-6),
        }
    }

    /// Screen y for a row, within a lane whose top edge is `y0`.
    pub fn y(&self, row: f64, y0: f64, height: f64) -> f64 {
        y0 + height / 2.0 - (row - self.center) * self.px_per_row
    }

    /// The row under a screen y.
    pub fn row_at(&self, y: f64, y0: f64, height: f64) -> f64 {
        self.center - (y - y0 - height / 2.0) / self.px_per_row.max(1e-6)
    }

    /// Rows visible in a lane of `height` pixels.
    pub fn span(&self, height: f64) -> (f64, f64) {
        let half = height / (2.0 * self.px_per_row.max(1e-6));
        (self.center - half, self.center + half)
    }

    /// Zoom about a fixed row, so the row under the cursor stays put.
    pub fn zoom_about(&mut self, anchor_row: f64, factor: f64) {
        let f = factor.max(1e-6);
        self.center = anchor_row + (self.center - anchor_row) / f;
        self.px_per_row = (self.px_per_row * f).max(1e-6);
    }
}

impl Default for VerticalCamera {
    fn default() -> Self {
        Self {
            center: 60.0,
            px_per_row: 8.0,
        }
    }
}

/// Where the canvas is looking.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Document time at the left edge.
    pub t0: f64,
    /// Horizontal scale.
    pub units_per_px: f64,
    /// The vertical half.
    ///
    /// One type, shared with lanes: the roll is a second consumer of
    /// vertical position, so this is a *move* rather than the deletion
    /// #197 first assumed. Two implementations of the same arithmetic
    /// was the thing worth removing, not the fields.
    pub vertical: VerticalCamera,
    /// Rows the roll folds away, so a collapsed drum piece occupies one
    /// lane instead of two.
    ///
    /// This lives on the camera rather than at the call sites because
    /// row-to-y runs through `y`/`pitch_at` in about thirty places, and
    /// a fold applied in twenty-nine of them is a roll whose notes do
    /// not sit on the lane you clicked.
    pub fold: RowFold,
}

/// A monotonic row -> slot map: hidden rows collapse onto the visible
/// row above them, and everything higher shifts down to close the gap.
///
/// Empty is the identity, which is every mode but drums.
/// A fold holds one hidden row per two-handed piece. The FTS map has
/// five (kick and four toms); the cap is generous and fixed so that
/// `Camera` stays `Copy`, which it is at every call site.
pub const MAX_FOLD: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowFold {
    /// Hidden model rows, ascending, `len` of them meaningful.
    hidden: [i32; MAX_FOLD],
    len: usize,
}

impl Default for RowFold {
    fn default() -> Self {
        Self {
            hidden: [0; MAX_FOLD],
            len: 0,
        }
    }
}

impl RowFold {
    /// `hidden` need not be sorted or unique. Rows past `MAX_FOLD` are
    /// dropped: showing an extra lane beats refusing to draw the roll.
    pub fn new(mut hidden: Vec<i32>) -> Self {
        hidden.sort_unstable();
        hidden.dedup();
        hidden.truncate(MAX_FOLD);
        let mut rows = [0; MAX_FOLD];
        rows[..hidden.len()].copy_from_slice(&hidden);
        Self {
            hidden: rows,
            len: hidden.len(),
        }
    }

    fn rows(&self) -> &[i32] {
        &self.hidden[..self.len]
    }

    pub fn is_identity(&self) -> bool {
        self.len == 0
    }

    /// How many rows this fold hides — the difference between the row
    /// range and the number of lanes actually drawn.
    pub fn hidden_count(&self) -> usize {
        self.len
    }

    fn is_hidden(&self, row: i32) -> bool {
        self.rows().binary_search(&row).is_ok()
    }

    /// Hidden rows strictly below `row`.
    fn below(&self, row: i32) -> i32 {
        self.rows().partition_point(|&h| h < row) as i32
    }

    /// Display slot for a model row. Fractional parts survive, so a
    /// note bent a quarter-row sharp still draws a quarter-row up.
    pub fn slot(&self, row: f64) -> f64 {
        if self.len == 0 {
            return row;
        }
        let base = row.floor();
        let frac = row - base;
        let mut r = base as i32;
        // A hidden row shares the slot of the first visible row above
        // it — the fold only ever hides a left hand, which sits
        // directly below its right.
        while self.is_hidden(r) {
            r += 1;
        }
        (r - self.below(r)) as f64 + frac
    }

    /// The model row occupying a display slot — the inverse of `slot`
    /// for visible rows, and the reason a click on a collapsed piece
    /// lands on the hand that stands for it.
    pub fn row(&self, slot: f64) -> f64 {
        if self.len == 0 {
            return slot;
        }
        let base = slot.floor();
        let frac = slot - base;
        let mut r = base as i32;
        for &h in self.rows() {
            if h <= r {
                r += 1;
            } else {
                break;
            }
        }
        r as f64 + frac
    }

    /// A row-space range expressed in slots.
    pub fn slot_bounds(&self, (lo, hi): (i32, i32)) -> (i32, i32) {
        (self.slot(lo as f64) as i32, self.slot(hi as f64) as i32)
    }
}

/// Hard limits applied after every navigation. These are clamps, not
/// magnets: hitting one discards the requested move, which is why they
/// are applied exactly once rather than mid-gesture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    /// Editable time span, already cushioned.
    pub t_min: f64,
    pub t_max: f64,
    /// Inclusive row range the roll can show, from the mode's row space.
    /// The vertical camera is fitted to exactly this — never wider, so
    /// the roll cannot show empty space above or below the keys.
    pub row_min: f64,
    pub row_max: f64,
    /// How many times the cushioned content span the view may zoom out
    /// to. Time is not clamped to the item: a take has to be placeable
    /// against what surrounds it, so you can pull back and see either
    /// side of it.
    pub max_zoom_out: f64,
    /// Row height floor for manual zoom — below the readable minimum
    /// Reset View uses, so an overview stays available.
    pub min_px_per_semitone: f64,
    pub max_px_per_semitone: f64,
    /// Fraction of the viewport allowed past an edge as whitespace.
    pub edge_whitespace: f64,
}

impl Default for Bounds {
    fn default() -> Self {
        Self {
            t_min: 0.0,
            t_max: 1.0,
            row_min: 0.0,
            row_max: 127.0,
            max_zoom_out: 20.0,
            min_px_per_semitone: 3.5,
            max_px_per_semitone: 400.0,
            edge_whitespace: 0.2,
        }
    }
}

impl Camera {
    pub fn x(&self, t: f64) -> f64 {
        (t - self.t0) / self.units_per_px
    }

    pub fn t_at(&self, x: f64) -> f64 {
        self.t0 + x * self.units_per_px
    }

    /// Takes a *model* row; the fold is applied here.
    pub fn y(&self, pitch: f64, vp: Viewport) -> f64 {
        let slot = self.fold.slot(pitch);
        vp.h * 0.5 - (slot - self.vertical.center) * self.vertical.px_per_row
    }

    /// Returns a *model* row.
    pub fn pitch_at(&self, y: f64, vp: Viewport) -> f64 {
        let slot = self.vertical.center + (vp.h * 0.5 - y) / self.vertical.px_per_row;
        self.fold.row(slot)
    }

    /// The visible span in *slots*, which is what a renderer iterates:
    /// folded rows have no slot of their own to draw.
    pub fn slot_span(&self, vp: Viewport) -> (f64, f64) {
        let half = vp.h * 0.5 / self.vertical.px_per_row;
        (self.vertical.center - half, self.vertical.center + half)
    }

    /// `(t_left, t_right)`.
    pub fn time_span(&self, vp: Viewport) -> (f64, f64) {
        (self.t0, self.t0 + vp.w * self.units_per_px)
    }

    /// `(pitch_low, pitch_high)`.
    pub fn pitch_span(&self, vp: Viewport) -> (f64, f64) {
        let half = vp.h * 0.5 / self.vertical.px_per_row;
        (self.vertical.center - half, self.vertical.center + half)
    }

    /// Zoom time by `factor` (>1 zooms in) keeping `anchor_t` pinned to
    /// its current pixel.
    pub fn zoom_time_about(&mut self, anchor_t: f64, factor: f64) {
        let x = self.x(anchor_t);
        self.units_per_px /= factor.max(1e-6);
        self.t0 = anchor_t - x * self.units_per_px;
    }

    /// Zoom pitch by `factor` keeping `anchor_pitch` pinned.
    pub fn zoom_pitch_about(&mut self, anchor_pitch: f64, factor: f64, vp: Viewport) {
        let y = self.y(anchor_pitch, vp);
        self.vertical.px_per_row *= factor.max(1e-6);
        self.vertical.center = anchor_pitch - (vp.h * 0.5 - y) / self.vertical.px_per_row;
    }

    pub fn pan_px(&mut self, dx: f64, dy: f64) {
        self.t0 -= dx * self.units_per_px;
        self.vertical.center += dy / self.vertical.px_per_row;
    }

    /// Clamp to `bounds`. Applied once, after blending.
    /// Hold the camera inside what the document can meaningfully show.
    ///
    /// The two axes get opposite treatment, because they mean different
    /// things. Time is open: an item sits in a timeline, and you have to
    /// be able to pull back and see what is either side of it, so the
    /// only ceiling is a sanity limit on how small the content may get.
    /// Pitch is closed: the rows *are* the instrument, and there is
    /// nothing above the top key or below the bottom one to look at, so
    /// the roll is fitted to them and never shows empty space.
    pub fn constrain(&mut self, bounds: Bounds, vp: Viewport) {
        // ── time: free, within reason ────────────────────────────────
        let span = (bounds.t_max - bounds.t_min).max(1e-9);
        // Zooming out used to stop at exactly the cushioned item, which
        // made it impossible to see past either end of the take.
        let max_upp = span * bounds.max_zoom_out / vp.w.max(1.0);
        if max_upp > 0.0 {
            self.units_per_px = self.units_per_px.min(max_upp).max(1e-9);
        }
        let visible = vp.w * self.units_per_px;
        // A whole screen of slack each side, not a fifth of one: the
        // point is to be able to look *off* the item.
        let slack = visible.max(span * bounds.edge_whitespace);
        let lo = bounds.t_min - slack;
        let hi = (bounds.t_max + slack - visible).max(lo);
        self.t0 = self.t0.clamp(lo, hi);

        // ── pitch: fitted to the keys ────────────────────────────────
        // The rows the roll actually draws: the mode's range, less
        // whatever the fold collapses away.
        let rows =
            ((bounds.row_max - bounds.row_min + 1.0) - self.fold.hidden_count() as f64).max(1.0);
        // Tall enough that `rows` of them always cover the lane. This is
        // the floor that stops a zoom-out leaving empty space where
        // there are no keys.
        let fit = vp.h / rows;
        let min_ppr = fit.max(bounds.min_px_per_semitone.min(fit));
        let max_ppr = bounds.max_px_per_semitone.max(min_ppr);
        self.vertical.px_per_row = self.vertical.px_per_row.clamp(min_ppr, max_ppr);

        // And centred so the window stays over real rows. When the whole
        // range is on screen there is only one legal centre, so the
        // clamp collapses to it rather than inverting.
        let half = (vp.h * 0.5) / self.vertical.px_per_row.max(1e-9);
        let c_lo = bounds.row_min + half;
        let c_hi = bounds.row_max + 1.0 - half;
        self.vertical.center = if c_lo <= c_hi {
            self.vertical.center.clamp(c_lo, c_hi)
        } else {
            (bounds.row_min + bounds.row_max + 1.0) / 2.0
        };
    }
}

/// A candidate camera and how strongly it pulls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Influence {
    pub camera: Camera,
    /// 0..1. Weights above 1 in aggregate are normalized.
    pub weight: f64,
}

/// Resolve `base` against every influence in one pass.
///
/// Scales blend geometrically — halving and doubling are equal-sized
/// steps to the eye, so a linear average of `units_per_px` would bias
/// every blend toward zoomed-out.
pub fn blend(base: Camera, influences: &[Influence]) -> Camera {
    let total: f64 = influences.iter().map(|i| i.weight.clamp(0.0, 1.0)).sum();
    if total <= 1e-9 {
        return base;
    }
    // Above a combined weight of 1 the influences fully replace the
    // base rather than overshooting past it.
    let norm = if total > 1.0 { 1.0 / total } else { 1.0 };
    let base_w = (1.0 - total * norm).max(0.0);

    let mut t0 = base.t0 * base_w;
    let mut center = base.vertical.center * base_w;
    let mut log_upp = base.units_per_px.max(1e-12).ln() * base_w;
    let mut log_pps = base.vertical.px_per_row.max(1e-12).ln() * base_w;

    for i in influences {
        let w = i.weight.clamp(0.0, 1.0) * norm;
        t0 += i.camera.t0 * w;
        center += i.camera.vertical.center * w;
        log_upp += i.camera.units_per_px.max(1e-12).ln() * w;
        log_pps += i.camera.vertical.px_per_row.max(1e-12).ln() * w;
    }

    Camera {
        t0,
        units_per_px: log_upp.exp(),
        vertical: VerticalCamera {
            center,
            px_per_row: log_pps.exp(),
        },
        // The fold is a property of the row space, not of any camera
        // being blended toward, so it comes from the base unchanged.
        fold: base.fold,
    }
}

/// Content the camera frames: the time span plus the pitch span that
/// actually contains notes *and* their curves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Content {
    pub t_start: f64,
    pub t_end: f64,
    pub pitch_lo: f64,
    pub pitch_hi: f64,
}

/// The idealized "Reset View" camera — what `V` snaps to and what every
/// zoom-out path converges on.
///
/// Fits the content with `pad` fractional headroom above and below
/// (0.35 in the shipped feel) and a small horizontal cushion.
pub fn reset_view(content: Content, vp: Viewport, cushion: f64, pad: f64, fold: RowFold) -> Camera {
    let span = (content.t_end - content.t_start).max(1e-6);
    let t0 = content.t_start - span * cushion;
    let units_per_px = (span * (1.0 + 2.0 * cushion)) / vp.w;

    // Framing happens in slot space: folding two lanes into one makes
    // the content shorter on screen, and fitting the unfolded height
    // would leave a band of empty roll under it.
    let (slot_lo, slot_hi) = (fold.slot(content.pitch_lo), fold.slot(content.pitch_hi));
    let pitch_span = (slot_hi - slot_lo).max(1.0) * (1.0 + 2.0 * pad);
    // Reset View keeps a readable floor of its own, above the manual
    // zoom-out floor, so `V` always lands somewhere legible.
    let px_per_row = (vp.h / pitch_span).max(7.0);

    Camera {
        t0,
        units_per_px,
        vertical: VerticalCamera {
            center: (slot_lo + slot_hi) * 0.5,
            px_per_row,
        },
        fold,
    }
}

/// Magnet that frames the nearer item edge once the pointer approaches
/// it.
///
/// Zero through the inner `dead_zone` of the item's half-span, rising
/// with smoothstep to full at the edge, where it leaves
/// `whitespace` of the viewport past the edge.
pub fn edge_magnet(
    base: Camera,
    mouse_t: f64,
    content: Content,
    vp: Viewport,
    dead_zone: f64,
    whitespace: f64,
) -> Option<Influence> {
    let center = (content.t_start + content.t_end) * 0.5;
    let radius = (content.t_end - content.t_start) * 0.5;
    if radius <= 0.0 {
        return None;
    }
    let offset = (mouse_t - center) / radius; // -1..1 inside the item
    let weight = smoothstep_between(dead_zone, 1.0, offset.abs());
    if weight <= 0.0 {
        return None;
    }
    let visible = vp.w * base.units_per_px;
    let pad = visible * whitespace;
    let t0 = if offset > 0.0 {
        // Right edge framed at the right side of the viewport.
        content.t_end + pad - visible
    } else {
        content.t_start - pad
    };
    Some(Influence {
        camera: Camera { t0, ..base },
        weight,
    })
}

/// How far a camera has travelled toward Reset View, 0..1, measured on
/// whichever axis has progressed further.
///
/// Used only while zooming *out*: zoom-in must never be pulled toward
/// the reset camera.
pub fn reset_progress(current: Camera, reset: Camera) -> f64 {
    let h = axis_progress(current.units_per_px, reset.units_per_px);
    let v = axis_progress(reset.vertical.px_per_row, current.vertical.px_per_row);
    h.max(v)
}

fn axis_progress(current: f64, target: f64) -> f64 {
    // Measured in log space so "80% of the way there" means the same
    // thing at every zoom depth.
    let (c, t) = (current.max(1e-12).ln(), target.max(1e-12).ln());
    if (t - c).abs() < 1e-9 {
        return 1.0;
    }
    (c / t).clamp(0.0, 1.0)
}

/// The reset-tail magnet: inert for the first `tail_start` of the path
/// toward Reset View, then rising to full across the remainder.
///
/// Deliberately late. Engaging it earlier is what makes a zoom-out feel
/// like it is being taken away from you.
pub fn reset_tail(current: Camera, reset: Camera, tail_start: f64) -> Option<Influence> {
    let progress = reset_progress(current, reset);
    let weight = smoothstep_between(tail_start, 1.0, progress);
    (weight > 0.0).then_some(Influence {
        camera: reset,
        weight,
    })
}

/// Guide the vertical position toward the pitch of content near the
/// pointer, with a smaller contribution from where the pointer itself
/// sits.
///
/// `local_pitch` is the weighted pitch center of notes near the mouse
/// time; `mouse_pitch` is the pointer's own pitch.
pub fn pitch_focus(
    base: Camera,
    local_pitch: Option<f64>,
    mouse_pitch: f64,
    local_weight: f64,
    mouse_weight: f64,
) -> Vec<Influence> {
    let mut out = Vec::new();
    if let Some(p) = local_pitch {
        out.push(Influence {
            camera: Camera {
                vertical: VerticalCamera {
                    center: p,
                    ..base.vertical
                },
                ..base
            },
            weight: local_weight,
        });
    }
    out.push(Influence {
        camera: Camera {
            vertical: VerticalCamera {
                center: mouse_pitch,
                ..base.vertical
            },
            ..base
        },
        weight: mouse_weight,
    });
    out
}

/// Center pull that fades in only at deep vertical zoom, so close-up
/// editing stays fluid and does not fight the pointer.
pub fn deep_zoom_center(
    base: Camera,
    content: Content,
    onset: f64,
    max_px_per_semitone: f64,
) -> Option<Influence> {
    let depth = base.vertical.px_per_row / max_px_per_semitone.max(1e-6);
    let weight = smoothstep(smoothstep_between(onset, 1.0, depth));
    (weight > 0.0).then_some(Influence {
        camera: Camera {
            vertical: VerticalCamera {
                center: (content.pitch_lo + content.pitch_hi) * 0.5,
                ..base.vertical
            },
            ..base
        },
        weight,
    })
}
