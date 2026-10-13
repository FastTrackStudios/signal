//! Contextual zoom and scroll — one gesture whose result depends on
//! where it was invoked from.
//!
//! ## Attribution
//!
//! This is a port of **MeMagic** by Ilias-Timon Poulakis (FeedTheCat),
//! from `github.com/iliaspoulakis/Reaper-Tools`, used under the MIT
//! licence:
//!
//! > MIT License
//! >
//! > Copyright (c) 2020 iliaspoulakis
//! >
//! > Permission is hereby granted, free of charge, to any person
//! > obtaining a copy of this software and associated documentation
//! > files (the "Software"), to deal in the Software without
//! > restriction, including without limitation the rights to use, copy,
//! > modify, merge, publish, distribute, sublicense, and/or sell copies
//! > of the Software, and to permit persons to whom the Software is
//! > furnished to do so, subject to the following conditions:
//! >
//! > The above copyright notice and this permission notice shall be
//! > included in all copies or substantial portions of the Software.
//! >
//! > THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
//! > EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
//! > MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! > NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS
//! > BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN
//! > ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
//! > CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! > SOFTWARE.
//!
//! The algorithms — the mode pairs and their per-region defaults, the
//! inverse-distance-weighted span estimate with its gap clamp and
//! length-floored distance, the log-scale measure snapping, and the
//! guard rails — follow the Lua. What changed is the host: REAPER takes
//! and PPQ become this crate's document and camera, and the arrange-view
//! and multi-item contexts are absent because there is no arrange view
//! here yet.
//!
//! ## Why it is one action and not fifteen
//!
//! The idea worth stealing is that the *same* invocation means different
//! things depending on the region it came from, so one key does the
//! obvious thing everywhere: over the notes it frames the passage under
//! the pointer, over the piano keys it frames the whole item, over the
//! ruler it goes to the top of the range. Binding fifteen actions and
//! remembering which is which is the thing this replaces.
//!
//! ## The three ideas that make it feel right
//!
//! **The anchor is not always the mouse.** Playing transport wins over
//! the pointer, and the pointer wins over the edit cursor. Zooming while
//! the music plays should frame the music, not wherever the mouse was
//! left.
//!
//! **The span comes from note density, not a note count.** Counting
//! twenty notes outward from the cursor gives a span that lurches
//! between a dense passage and a sparse one. Instead every note
//! contributes its length-plus-gap weighted by how near it is to the
//! anchor, so the estimate degrades smoothly as the passage thins out.
//!
//! **Zoom levels snap to musical ones.** Left alone, repeated
//! invocations land on arbitrary spans that never look the same twice.
//! Snapping to a power-of-two measure count means the view settles into
//! 1, 2, 4, 8 bars — places you recognise.

use crate::camera::Content;
use crate::doc::ExpressionDoc;

/// Which region the gesture came from.
///
/// The dispatch table below turns this into a mode pair. Keeping the
/// region and the modes separate is what lets a host bind the same
/// action everywhere and still get sensible behaviour per region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// Over the notes.
    NoteArea,
    /// Over the piano keys / row gutter.
    Piano,
    /// Over the time ruler.
    Ruler,
    /// Over a controller lane.
    CcLane,
    /// From a toolbar button or a bare keypress — no position to use.
    Elsewhere,
}

/// Which notes a vertical calculation considers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Only what is on screen horizontally.
    InView,
    /// Everything in the document.
    InItem,
}

/// What to do with the time axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Horizontal {
    /// Leave it exactly as it is.
    Keep,
    /// Fit the whole item.
    FitItem,
    /// A fixed number of measures around the anchor.
    Measures { count: f64, restrict: bool },
    /// Density-weighted span around the anchor.
    Smart { restrict: bool },
    /// Density-weighted, then snapped to a musical measure count.
    SmartSnapped { restrict: bool },
    /// Keep the zoom, move the anchor to its place on screen.
    ScrollOnly,
}

/// What to do with the pitch axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Vertical {
    Keep,
    /// Fit the pitch range of the notes in `scope`.
    FitNotes {
        scope: Scope,
    },
    /// Centre on the anchor row, clamped to the notes in `scope`.
    ScrollToAnchor {
        scope: Option<Scope>,
    },
    /// Centre on the middle of the notes in `scope`.
    Center {
        scope: Scope,
    },
    /// Put the lowest note of `scope` in view.
    Lowest {
        scope: Scope,
    },
    /// Put the highest note of `scope` in view.
    Highest {
        scope: Scope,
    },
}

/// Where the gesture is pointed, in document terms.
///
/// `t` follows the priority chain the caller resolved: play cursor while
/// playing, else pointer, else edit cursor. `row` is `None` when the
/// gesture had no meaningful vertical position (a toolbar button).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub t: f64,
    pub row: Option<f64>,
}

/// The tuning constants, all of them.
///
/// Defaults are MeMagic's, which were arrived at by use rather than
/// derivation — there is no formula behind twenty notes or eight rows,
/// they are what feels right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    /// How many notes the smart span aims to show.
    pub target_notes: f64,
    /// How sharply nearby notes dominate the density estimate. Higher
    /// is more local; 0 would weight the whole take equally.
    pub smoothing: f64,
    /// Where the anchor sits across the view: 0 left, 0.5 centred, 1
    /// right.
    pub alignment: f64,
    /// Never show fewer rows than this, however narrow the notes are —
    /// a two-note passage zoomed to fit is unreadable and unclickable.
    pub min_rows: f64,
    /// Never make a row taller than this.
    pub max_px_per_row: f64,
    /// Where to centre when there is nothing to centre on. 60 is middle
    /// C.
    pub base_note: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            target_notes: 20.0,
            smoothing: 0.75,
            alignment: 0.5,
            min_rows: 8.0,
            max_px_per_row: 32.0,
            base_note: 60.0,
        }
    }
}

/// A mode pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Modes {
    pub horizontal: Horizontal,
    pub vertical: Vertical,
}

impl Region {
    /// What this region does by default.
    ///
    /// The shipped MeMagic defaults, which read as a sketch of the
    /// intent: over the notes, frame the passage you are pointing at and
    /// keep the pitch sensible; over the keys, show the whole item both
    /// ways; the ruler and the CC lane push the view to the top and
    /// bottom of the range respectively, because that is what you want
    /// visible when working in the lane below.
    pub fn modes(self) -> Modes {
        match self {
            Region::NoteArea => Modes {
                horizontal: Horizontal::Smart { restrict: true },
                vertical: Vertical::ScrollToAnchor {
                    scope: Some(Scope::InItem),
                },
            },
            Region::Piano => Modes {
                horizontal: Horizontal::FitItem,
                // In *view*, not in item: the horizontal half has just
                // framed the whole item, so "the notes in view" is the
                // item's — and when a host pairs this differently the
                // vertical still follows what is actually on screen.
                vertical: Vertical::FitNotes {
                    scope: Scope::InView,
                },
            },
            Region::Ruler => Modes {
                horizontal: Horizontal::Keep,
                vertical: Vertical::Highest {
                    scope: Scope::InView,
                },
            },
            Region::CcLane => Modes {
                horizontal: Horizontal::Keep,
                vertical: Vertical::Lowest {
                    scope: Scope::InView,
                },
            },
            Region::Elsewhere => Modes {
                horizontal: Horizontal::Keep,
                vertical: Vertical::FitNotes {
                    scope: Scope::InItem,
                },
            },
        }
    }
}

/// The horizontal span a smart zoom should show around `anchor_t`.
///
/// Shepard's inverse-distance weighting: every note contributes its own
/// length plus the gap that follows it, weighted by
/// `1 / distance^smoothing` from the anchor, and the weighted average
/// times [`Config::target_notes`] is the span.
///
/// Weighting rather than counting is the point. A literal "twenty
/// nearest notes" window jumps as the cursor crosses from a busy bar
/// into a sparse one, because the twentieth note is suddenly a long way
/// off; an inverse-distance average moves continuously. A smoothing of 0
/// would ignore the anchor entirely and average the whole take.
///
/// Three details carry more weight than they look:
///
/// - **Distance is measured to the note's centre**, not its start, so a
///   long note under the cursor is not treated as distant.
/// - **Distance is floored at the note's own length.** Without it a note
///   sitting exactly on the anchor divides by ~0 and its spacing becomes
///   the entire answer. Flooring by length rather than a constant keeps
///   the floor proportional.
/// - **Gaps are clamped** to `note_length * target_notes`. A bar of
///   silence in the middle of a take would otherwise drag the average up
///   and zoom the view out to nothing.
///
/// `None` when there is nothing to measure, which the caller should read
/// as "fit the item" rather than substituting a number.
pub fn smart_span(doc: &ExpressionDoc, anchor_t: f64, cfg: &Config) -> Option<f64> {
    if doc.notes.is_empty() {
        return None;
    }
    let mut notes: Vec<(f64, f64)> = doc.notes.iter().map(|n| (n.start, n.end)).collect();
    notes.sort_by(|a, b| a.0.total_cmp(&b.0));

    let target = cfg.target_notes.max(1.0);
    let mut length_sum = 0.0;
    let mut weight_sum = 0.0;
    let mut min_distance = f64::INFINITY;
    // Carried between notes, like the gap it describes: a note with no
    // gap before it inherits the last one measured.
    let mut gap = 0.0;
    let mut prev_end: Option<f64> = None;

    for &(start, end) in &notes {
        let length = (end - start).max(0.0);
        if let Some(pe) = prev_end
            && start >= pe
        {
            // Clamped, so one long silence cannot define the zoom.
            gap = (start - pe).min(length * target);
        }
        // Only ever advances, so overlapping notes do not produce a
        // negative gap.
        prev_end = Some(match prev_end {
            Some(pe) if end <= pe => pe,
            _ => end,
        });

        let centre = start + length / 2.0;
        // Floored by the note's own length — see above.
        let distance = (centre - anchor_t).abs().max(length);
        if distance <= 0.0 {
            continue;
        }
        min_distance = min_distance.min(distance);

        let weight = 1.0 / distance.powf(cfg.smoothing.max(0.0));
        length_sum += weight * (length + gap);
        weight_sum += weight;
    }

    if weight_sum <= 0.0 {
        return None;
    }
    let mut span = (length_sum / weight_sum) * target;

    // Zooming into empty space: if the nearest note is further away than
    // the span would show, widen until it is on screen. Otherwise the
    // gesture lands you in silence with no idea where the music went.
    if min_distance.is_finite() && span / 2.5 < min_distance {
        span = min_distance * 2.5;
    }
    Some(span.max(1e-6))
}

/// Round a span to a musical number of measures.
///
/// Under ten measures it snaps to a power of two — 1, 2, 4, 8 — and
/// above that to a whole measure. Repeated invocations then settle onto
/// the same handful of zoom levels instead of drifting, which is what
/// makes the gesture feel repeatable rather than approximate.
pub fn snap_to_measures(span: f64, units_per_bar: f64) -> f64 {
    if units_per_bar <= 0.0 || span <= 0.0 {
        return span;
    }
    let measures = span / units_per_bar;
    let snapped = if measures < 10.0 {
        // Round the exponent, not the count: musically the step from 2
        // bars to 4 is the same size as 4 to 8.
        let e = (measures.max(1e-6).log2() + 0.5).floor();
        2f64.powf(e)
    } else {
        measures.round()
    };
    snapped.max(1.0) * units_per_bar
}

/// Slide a span so it sits inside `[lo, hi]`, keeping its length.
///
/// Clipping would change the zoom the user just asked for, so overhang
/// moves the window instead. A span longer than the range gives up and
/// shows the range.
pub fn slide_into(start: f64, len: f64, lo: f64, hi: f64) -> (f64, f64) {
    if len >= hi - lo {
        return (lo, (hi - lo).max(1e-6));
    }
    let start = if start < lo {
        lo
    } else if start + len > hi {
        hi - len
    } else {
        start
    };
    (start, len)
}

/// The pitch range of the notes in a time window.
///
/// `None` when the window holds nothing, so the caller can fall back to
/// [`Config::base_note`] rather than centring on zero.
pub fn pitch_range(doc: &ExpressionDoc, from: f64, to: f64) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for n in doc.notes.iter().filter(|n| n.start < to && n.end > from) {
        lo = lo.min(n.row as f64);
        hi = hi.max(n.row as f64);
    }
    (lo <= hi).then_some((lo, hi))
}

/// Widen a pitch range to at least `min_rows`, keeping it centred.
///
/// A single-pitch passage fitted exactly gives one enormous row; the
/// floor is what keeps the result readable and clickable.
pub fn pad_to_min_rows(lo: f64, hi: f64, min_rows: f64) -> (f64, f64) {
    let span = hi - lo + 1.0;
    if span >= min_rows {
        return (lo, hi);
    }
    let mid = (lo + hi) / 2.0;
    let half = min_rows / 2.0;
    (mid - half, mid + half)
}

/// The time window the view should end up showing.
///
/// Split out from applying it so it can be tested without a camera, and
/// so a host can preview the result.
pub fn horizontal_span(
    doc: &ExpressionDoc,
    content: Content,
    mode: Horizontal,
    anchor: Anchor,
    units_per_bar: f64,
    current: (f64, f64),
    cfg: &Config,
) -> Option<(f64, f64)> {
    let item = (content.t_start, content.t_end);
    let item_len = (item.1 - item.0).max(1e-6);

    let len = match mode {
        Horizontal::Keep => return None,
        Horizontal::FitItem => item_len,
        Horizontal::Measures { count, .. } => count.max(1.0) * units_per_bar,
        Horizontal::Smart { .. } => smart_span(doc, anchor.t, cfg).unwrap_or(item_len),
        Horizontal::SmartSnapped { .. } => {
            let raw = smart_span(doc, anchor.t, cfg).unwrap_or(item_len);
            snap_to_measures(raw, units_per_bar)
        }
        Horizontal::ScrollOnly => (current.1 - current.0).max(1e-6),
    };

    // Place the anchor where the alignment says, rather than always
    // centring: 0 puts what you pointed at on the left, which is what
    // you want when reading forward.
    let start = anchor.t - len * cfg.alignment.clamp(0.0, 1.0);

    let restrict = matches!(
        mode,
        Horizontal::Measures { restrict: true, .. }
            | Horizontal::Smart { restrict: true }
            | Horizontal::SmartSnapped { restrict: true }
    ) || matches!(mode, Horizontal::FitItem);

    Some(if restrict {
        slide_into(start, len, item.0, item.1)
    } else {
        (start, len)
    })
}

/// The pitch window the view should end up showing, as `(lo, hi)` rows.
pub fn vertical_range(
    doc: &ExpressionDoc,
    content: Content,
    mode: Vertical,
    anchor: Anchor,
    view: (f64, f64),
    cfg: &Config,
) -> Option<(f64, f64)> {
    let window = |scope: Scope| match scope {
        Scope::InView => view,
        Scope::InItem => (content.t_start, content.t_end),
    };
    // What to show when the scope turns out to be empty. Centring on
    // middle C beats centring on row zero, which is a pitch nobody
    // plays.
    let fallback = || {
        let half = cfg.min_rows / 2.0;
        (cfg.base_note - half, cfg.base_note + half)
    };

    let (lo, hi) = match mode {
        Vertical::Keep => return None,
        Vertical::FitNotes { scope } => {
            pitch_range(doc, window(scope).0, window(scope).1).unwrap_or_else(fallback)
        }
        Vertical::ScrollToAnchor { scope } => {
            // Keep the current height; just move it. The anchor row is
            // what the gesture is about, so a missing one means there is
            // nothing to do.
            let row = anchor.row?;
            let half = ((view.1 - view.0) * 0.0).max(cfg.min_rows / 2.0);
            let (mut lo, mut hi) = (row - half, row + half);
            // Clamped to real notes when asked, so scrolling to the
            // pointer cannot leave the music entirely.
            if let Some(scope) = scope
                && let Some((nlo, nhi)) = pitch_range(doc, window(scope).0, window(scope).1)
            {
                let span = hi - lo;
                if lo < nlo {
                    lo = nlo;
                    hi = nlo + span;
                } else if hi > nhi {
                    hi = nhi;
                    lo = nhi - span;
                }
            }
            (lo, hi)
        }
        Vertical::Center { scope } => {
            let (nlo, nhi) =
                pitch_range(doc, window(scope).0, window(scope).1).unwrap_or_else(fallback);
            let mid = (nlo + nhi) / 2.0;
            let half = cfg.min_rows.max(nhi - nlo + 1.0) / 2.0;
            (mid - half, mid + half)
        }
        Vertical::Lowest { scope } => {
            let (nlo, _) =
                pitch_range(doc, window(scope).0, window(scope).1).unwrap_or_else(fallback);
            (nlo, nlo + cfg.min_rows)
        }
        Vertical::Highest { scope } => {
            let (_, nhi) =
                pitch_range(doc, window(scope).0, window(scope).1).unwrap_or_else(fallback);
            (nhi - cfg.min_rows, nhi)
        }
    };
    Some(pad_to_min_rows(lo, hi, cfg.min_rows))
}
