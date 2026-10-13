//! The view gestures the arrangement and the Editor share.
//!
//! Two surfaces that look at time by rows — items on tracks, notes on
//! pitches — and that should answer the same hand the same way: hold `z`
//! and drag and both zoom alike, sweep a box and both frame it, turn the
//! wheel and both scroll or zoom by the same amount, drag across empty
//! ground and both select what the box touches by the same rules.
//!
//! What lives here is what a gesture MEANS — how far a drag zooms, which
//! way, when a sweep is still a click, how a marquee combines with the
//! selection it started from. What does not is how either surface's
//! camera applies it, or what is under a box: those are the surfaces'
//! own (the arrangement keeps pixels-a-second and a scroll; the Editor
//! units-a-pixel and a centre row; items and notes sit on rows
//! differently). Each asks here, and applies the answer its way.
//!
//! Screen points are `(x, y)` in the surface's own pixels.

use std::collections::HashSet;
use std::hash::{BuildHasher, Hash};

/// How far a press may travel and still be a click, in pixels — the
/// line between "I clicked" and "I meant to drag a box".
pub const SLOP: f64 = 3.0;

/// Pixels of zoom-drag travel that zoom by `e`: right or up zooms in.
pub const ZOOM_GAIN: f64 = 200.0;
/// The same with Shift, for the fine control: a quarter as fast, so a
/// drag that overshot can be repeated slowly rather than undone.
pub const ZOOM_GAIN_FINE: f64 = 800.0;

/// A zoom by `pixels` of travel (positive in, negative out), as a scale
/// factor: `e^(pixels / ZOOM_GAIN)`.
#[must_use]
pub fn zoom_factor(pixels: f64) -> f64 {
    (pixels / ZOOM_GAIN).exp()
}

/// The zoom tool's drag: hold `z`, press, and drag.
///
/// Sideways zooms time, up and down zooms the rows, both exponential and
/// both about the press — the surface keeps what was under the press
/// under it. With Alt it is a sweep instead: a box, framed on release.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomDrag {
    /// Where the press was.
    pub origin: (f64, f64),
    /// Where the pointer is now.
    pub current: (f64, f64),
    /// A sweep (Alt): a box to frame, rather than a continuous zoom.
    pub sweep: bool,
}

impl ZoomDrag {
    /// A press at `at`; `sweep` with Alt held.
    #[must_use]
    pub const fn press(at: (f64, f64), sweep: bool) -> Self {
        Self {
            origin: at,
            current: at,
            sweep,
        }
    }

    /// The pointer moved.
    pub const fn moved(&mut self, at: (f64, f64)) {
        self.current = at;
    }

    /// How far to zoom from where the press found the view, on each
    /// axis: `(time, rows)` factors, above one zooming in. `None` while
    /// sweeping, when nothing moves until release.
    ///
    /// Right zooms time in and up zooms the rows in — the directions the
    /// content grows in.
    #[must_use]
    pub fn factors(&self, fine: bool) -> Option<(f64, f64)> {
        if self.sweep {
            return None;
        }
        let gain = if fine { ZOOM_GAIN_FINE } else { ZOOM_GAIN };
        let dx = self.current.0 - self.origin.0;
        let dy = self.origin.1 - self.current.1;
        Some(((dx / gain).exp(), (dy / gain).exp()))
    }

    /// The box being swept, while it is one: `(top-left, bottom-right)`.
    #[must_use]
    pub fn sweep_box(&self) -> Option<((f64, f64), (f64, f64))> {
        self.sweep.then(|| corners(self.origin, self.current))
    }

    /// Let go: the box to frame, for a sweep that went somewhere. A sweep
    /// that never moved is a click, and framing a click would zoom to the
    /// maximum for what looked like a misclick.
    #[must_use]
    pub fn release(self) -> Option<((f64, f64), (f64, f64))> {
        (self.sweep && travel(self.origin, self.current) > SLOP)
            .then(|| corners(self.origin, self.current))
    }
}

/// What a wheel turn does to a view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wheel {
    /// Scroll by `(dx, dy)` pixels: positive scrolls right and down.
    Scroll(f64, f64),
    /// Zoom time by this factor (above one: in), about the pointer.
    ZoomTime(f64),
    /// Zoom the rows by this factor, about the pointer.
    ZoomRows(f64),
}

/// A wheel turn of `(dx, dy)` pixels, the browser's way round (positive
/// is the wheel turned towards you, or a swipe left — the view scrolls
/// right and down), with `z` held (`zooming`) or not, and Shift.
///
/// A wheel scrolls, sideways with Shift; with `z` held it zooms time, and
/// with Shift as well the rows. A notch of forty pixels zooms by about
/// half, either way.
#[must_use]
pub fn wheel(dx: f64, dy: f64, zooming: bool, shift: bool) -> Wheel {
    if zooming {
        // Towards you zooms out, as a wheel over a map does.
        let factor = zoom_factor(-(dx + dy) * 2.0);
        return if shift {
            Wheel::ZoomRows(factor)
        } else {
            Wheel::ZoomTime(factor)
        };
    }
    if shift {
        Wheel::Scroll(dx + dy, 0.0)
    } else {
        Wheel::Scroll(dx, dy)
    }
}

/// How a marquee combines with the selection it started from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MarqueeMode {
    /// What the box touches, and nothing else.
    #[default]
    Replace,
    /// What the box touches, as well.
    Add,
    /// What the box touches flips: in if it was out, out if it was in.
    Toggle,
}

/// A marquee: a box swept across empty ground, selecting what it touches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marquee {
    pub origin: (f64, f64),
    pub current: (f64, f64),
    pub mode: MarqueeMode,
}

impl Marquee {
    #[must_use]
    pub const fn press(at: (f64, f64), mode: MarqueeMode) -> Self {
        Self {
            origin: at,
            current: at,
            mode,
        }
    }

    pub const fn moved(&mut self, at: (f64, f64)) {
        self.current = at;
    }

    /// The box, `(top-left, bottom-right)`.
    #[must_use]
    pub const fn rect(&self) -> ((f64, f64), (f64, f64)) {
        corners(self.origin, self.current)
    }

    /// Whether it never went anywhere: a click, not a box.
    #[must_use]
    pub fn is_click(&self) -> bool {
        travel(self.origin, self.current) <= SLOP
    }

    /// The selection it leaves: `base` (what was selected at the press)
    /// combined with `hits` (what the box touches) by its mode. A click
    /// selects nothing new: with Replace it clears, otherwise it leaves
    /// the selection as it was.
    #[must_use]
    pub fn select<K: Eq + Hash + Clone, S: BuildHasher + Default>(
        &self,
        base: &HashSet<K, S>,
        hits: impl IntoIterator<Item = K>,
    ) -> HashSet<K, S> {
        combine(self.mode, base, hits, self.is_click())
    }
}

/// A marquee's rule, for a surface that keeps its box in its own units.
///
/// `base` combined with `hits` by `mode`, and a `click` (a box that never
/// went anywhere) selecting nothing new — with Replace it clears.
#[must_use]
pub fn combine<K: Eq + Hash + Clone, S: BuildHasher + Default>(
    mode: MarqueeMode,
    base: &HashSet<K, S>,
    hits: impl IntoIterator<Item = K>,
    click: bool,
) -> HashSet<K, S> {
    let hits: HashSet<K, S> = if click {
        HashSet::default()
    } else {
        hits.into_iter().collect()
    };
    match mode {
        MarqueeMode::Replace => hits,
        MarqueeMode::Add => base.union(&hits).cloned().collect(),
        MarqueeMode::Toggle => base.symmetric_difference(&hits).cloned().collect(),
    }
}

/// A box in content space — time across, rows down — for asking what a
/// marquee touches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub t0: f64,
    pub t1: f64,
    /// The top and bottom of the rows the box covers, as continuous row
    /// positions: each surface says where a row's band lies.
    pub r0: f64,
    pub r1: f64,
}

impl Span {
    /// The box from two corners, either way round.
    #[must_use]
    pub const fn new((t_a, r_a): (f64, f64), (t_b, r_b): (f64, f64)) -> Self {
        Self {
            t0: t_a.min(t_b),
            t1: t_a.max(t_b),
            r0: r_a.min(r_b),
            r1: r_a.max(r_b),
        }
    }

    /// Whether something from `start` to `end`, on the row band `top` to
    /// `bottom`, is touched: any overlap at all, edges included.
    #[must_use]
    pub fn touches(&self, start: f64, end: f64, top: f64, bottom: f64) -> bool {
        start <= self.t1 && end >= self.t0 && top <= self.r1 && bottom >= self.r0
    }
}

const fn corners(a: (f64, f64), b: (f64, f64)) -> ((f64, f64), (f64, f64)) {
    ((a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1)))
}

fn travel(a: (f64, f64), b: (f64, f64)) -> f64 {
    (b.0 - a.0).abs() + (b.1 - a.1).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Right and up zoom in, by `e` every two hundred pixels; a quarter as
    /// fast with Shift; nothing while sweeping.
    #[test]
    fn a_zoom_drag_zooms_right_and_up() {
        let mut z = ZoomDrag::press((100.0, 100.0), false);
        z.moved((300.0, -100.0));
        let (t, r) = z.factors(false).expect("a zoom");
        assert!((t - 1.0_f64.exp()).abs() < 1e-12 && (r - 1.0_f64.exp()).abs() < 1e-12);
        let (t, _) = z.factors(true).expect("a zoom");
        assert!((t - 0.25_f64.exp()).abs() < 1e-12);
        z.moved((0.0, 200.0));
        let (t, r) = z.factors(false).expect("a zoom");
        assert!(t < 1.0 && r < 1.0, "left and down zoom out");
        let mut sweep = ZoomDrag::press((10.0, 10.0), true);
        sweep.moved((50.0, 5.0));
        assert_eq!(sweep.factors(false), None);
        assert_eq!(sweep.sweep_box(), Some(((10.0, 5.0), (50.0, 10.0))));
    }

    /// A sweep frames its box; a sweep that never moved is a click.
    #[test]
    fn a_sweep_frames_its_box_and_a_click_frames_nothing() {
        let mut sweep = ZoomDrag::press((10.0, 10.0), true);
        sweep.moved((11.0, 11.0));
        assert_eq!(sweep.release(), None);
        sweep.moved((80.0, 40.0));
        assert_eq!(sweep.release(), Some(((10.0, 10.0), (80.0, 40.0))));
        assert_eq!(ZoomDrag::press((0.0, 0.0), false).release(), None);
    }

    /// The wheel scrolls, sideways with Shift; with z it zooms time, with
    /// z and Shift the rows; towards you zooms out.
    #[test]
    fn the_wheel_scrolls_and_zooms() {
        assert_eq!(wheel(3.0, 40.0, false, false), Wheel::Scroll(3.0, 40.0));
        assert_eq!(wheel(0.0, 40.0, false, true), Wheel::Scroll(40.0, 0.0));
        let Wheel::ZoomTime(out) = wheel(0.0, 40.0, true, false) else {
            panic!("z and the wheel zoom time");
        };
        assert!(out < 1.0, "towards you zooms out");
        let Wheel::ZoomRows(inward) = wheel(0.0, -40.0, true, true) else {
            panic!("z, Shift and the wheel zoom the rows");
        };
        assert!(inward > 1.0);
    }

    /// Replace, add and toggle, from the selection at the press; a click
    /// selects nothing new.
    #[test]
    fn a_marquee_combines_with_the_selection_by_its_mode() {
        let base: HashSet<u32> = [1, 2].into_iter().collect();
        let swept = |mode| {
            let mut m = Marquee::press((0.0, 0.0), mode);
            m.moved((50.0, 50.0));
            let mut got: Vec<u32> = m.select(&base, [2, 3]).into_iter().collect();
            got.sort_unstable();
            got
        };
        assert_eq!(swept(MarqueeMode::Replace), [2, 3]);
        assert_eq!(swept(MarqueeMode::Add), [1, 2, 3]);
        assert_eq!(swept(MarqueeMode::Toggle), [1, 3]);
        let click = Marquee::press((0.0, 0.0), MarqueeMode::Add);
        assert_eq!(click.select(&base, [3]), base, "a click adds nothing");
        let click = Marquee::press((0.0, 0.0), MarqueeMode::Replace);
        assert!(click.select(&base, [3]).is_empty(), "a click clears");
    }

    /// A span touches anything overlapping it, edges included.
    #[test]
    fn a_span_touches_what_overlaps_it() {
        let s = Span::new((4.0, 3.0), (1.0, 1.0));
        assert!(s.touches(0.0, 1.0, 0.0, 1.0), "corner to corner");
        assert!(s.touches(2.0, 3.0, 1.5, 2.5));
        assert!(!s.touches(4.5, 6.0, 1.0, 2.0), "after it");
        assert!(!s.touches(2.0, 3.0, 3.5, 4.5), "below it");
    }
}
