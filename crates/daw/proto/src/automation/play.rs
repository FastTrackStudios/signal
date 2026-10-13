//! How an envelope plays: the value its points give at a time, each
//! segment bent by its point's shape, and the automation items laid over
//! it in their windows.
//!
//! One definition for every reader — the engine evaluates the curve it
//! plays with these, and an arrange view draws the curve it shows with
//! the same ones, so what is drawn is what is heard.

use super::{AutomationItem, EnvelopePoint, EnvelopeShape};
use crate::primitives::PositionInSeconds;

/// How many straight pieces stand in for a bent segment cut by an
/// automation item's edge (a shape spans its whole segment, so the part
/// of it left over has no shape of its own).
const PIECES: u32 = 16;

impl EnvelopeShape {
    /// How far a segment has come from its first value toward its next,
    /// `f` of the way along it in time (`0..=1`), bent by this shape.
    ///
    /// - **Linear**: straight.
    /// - **Square**: holds the first value to the next point.
    /// - **SlowStartEnd**: an S — eases out and in.
    /// - **FastStart** / **FastEnd**: most of the move early / late.
    /// - **Bezier**: `tension` (`-1..=1`) bends it, toward a fast start
    ///   below zero and a fast end above; straight at zero.
    pub fn ease(self, f: f64, tension: f64) -> f64 {
        let f = f.clamp(0.0, 1.0);
        match self {
            Self::Linear => f,
            Self::Square => 0.0,
            Self::SlowStartEnd => f * f * 2.0f64.mul_add(-f, 3.0),
            Self::FastStart => 1.0 - (1.0 - f).powi(3),
            Self::FastEnd => f.powi(3),
            Self::Bezier => f.powf(4.0f64.powf(tension.clamp(-1.0, 1.0))),
        }
    }
}

/// The value `points` (sorted by time) give at `t` seconds: the first
/// point's value before it, the last's after it, and between two points
/// the first's shape. Where two points share a time the later one holds
/// from there on, so a jump is two points at one time. `None` when there
/// are no points.
pub fn value_at(points: &[EnvelopePoint], t: f64) -> Option<f64> {
    let after = points.partition_point(|p| p.time.as_seconds() <= t);
    let before = after.checked_sub(1).and_then(|i| points.get(i));
    match (before, points.get(after)) {
        (None, next) => next.map(|p| p.value),
        (Some(p), None) => Some(p.value),
        (Some(a), Some(b)) => Some(between(a, b, t)),
    }
}

/// The value of the segment from `a` to `b` at `t`.
fn between(a: &EnvelopePoint, b: &EnvelopePoint, t: f64) -> f64 {
    let (from, to) = (a.time.as_seconds(), b.time.as_seconds());
    let span = to - from;
    if span <= 0.0 {
        return b.value;
    }
    let f = a.shape.ease((t - from) / span, a.tension);
    (b.value - a.value).mul_add(f, a.value)
}

/// An automation item's value at `t` on the timeline: its source's points
/// (`points`, in seconds into the source) read through its offset, rate
/// and loop, then its amplitude and baseline. `None` outside its window
/// or when its source is empty.
///
/// The baseline and amplitude are REAPER's: `0.5` and `1` leave the
/// curve as drawn; the amplitude scales it about the middle (negative
/// turns it over), the baseline moves it. A looped source repeats every
/// `source length`, which is taken as its last point's time — standalone
/// keeps no separate length for a pool.
pub fn item_value_at(item: &AutomationItem, points: &[EnvelopePoint], t: f64) -> Option<f64> {
    let (start, length) = (item.position.as_seconds(), item.length.as_seconds());
    if t < start || t > start + length {
        return None;
    }
    let source = source_time(item, points, t);
    value_at(points, source).map(|v| scaled(item, v))
}

/// Where in its source an item is at timeline time `t`.
fn source_time(item: &AutomationItem, points: &[EnvelopePoint], t: f64) -> f64 {
    let into =
        (t - item.position.as_seconds()).mul_add(item.play_rate, item.start_offset.as_seconds());
    let period = points.last().map_or(0.0, |p| p.time.as_seconds());
    if item.loop_source && period > 0.0 {
        into.rem_euclid(period)
    } else {
        into
    }
}

/// A source value through an item's amplitude and baseline.
fn scaled(item: &AutomationItem, v: f64) -> f64 {
    if (item.amplitude - 1.0).abs() < f64::EPSILON && (item.baseline - 0.5).abs() < f64::EPSILON {
        return v;
    }
    (v - 0.5).mul_add(item.amplitude, item.baseline)
}

/// The curve an envelope plays: its own points, and over each automation
/// item's window that item's curve instead (a later item over an earlier
/// one where they overlap, as REAPER draws them). Flattened into one
/// sorted list of points, so a reader evaluates it with [`value_at`] and
/// never needs to know there were items.
///
/// The curves keep their shapes; a bent segment cut by a window's edge is
/// carried as short straight pieces. Where the envelope has no points of
/// its own, the items' values hold out to either side of them.
pub fn played(
    points: &[EnvelopePoint],
    items: &[(AutomationItem, Vec<EnvelopePoint>)],
) -> Vec<EnvelopePoint> {
    let windows: Vec<(f64, f64, usize)> = items
        .iter()
        .enumerate()
        .filter(|(_, (item, source))| item.length.as_seconds() > 0.0 && !source.is_empty())
        .map(|(i, (item, _))| {
            let start = item.position.as_seconds();
            (start, start + item.length.as_seconds(), i)
        })
        .collect();
    if windows.is_empty() {
        return points.to_vec();
    }
    // Every edge, then each stretch between two takes the topmost
    // window covering it, or the envelope itself.
    let mut edges: Vec<f64> = windows.iter().flat_map(|&(s, e, _)| [s, e]).collect();
    edges.sort_by(f64::total_cmp);
    edges.dedup();
    let mut out = Vec::new();
    let first = edges.first().copied().unwrap_or(0.0);
    let last = edges.last().copied().unwrap_or(0.0);
    if points.is_empty() {
        // Nothing underneath: the items' values hold either side.
        for pair in edges.windows(2) {
            if let [s, e] = *pair {
                if let Some(&(_, _, i)) = topmost(&windows, s, e) {
                    stretch(&timeline(items, i), s, e, &mut out);
                }
            }
        }
        return renumber(out);
    }
    stretch(points, f64::NEG_INFINITY, first, &mut out);
    for pair in edges.windows(2) {
        if let [s, e] = *pair {
            match topmost(&windows, s, e) {
                Some(&(_, _, i)) => stretch(&timeline(items, i), s, e, &mut out),
                None => stretch(points, s, e, &mut out),
            }
        }
    }
    stretch(points, last, f64::INFINITY, &mut out);
    renumber(out)
}

/// The window drawn on top over `s..e`: the latest one covering it.
fn topmost(windows: &[(f64, f64, usize)], s: f64, e: f64) -> Option<&(f64, f64, usize)> {
    windows
        .iter()
        .filter(|&&(ws, we, _)| ws <= s && we >= e)
        .max_by_key(|&&(_, _, i)| i)
}

/// Item `i`'s curve on the timeline over its window, as points: its
/// source's points mapped through the offset, rate and loop, scaled, with
/// a point at each end of the window. A bent segment cut by either end
/// goes in as straight pieces.
fn timeline(items: &[(AutomationItem, Vec<EnvelopePoint>)], i: usize) -> Vec<EnvelopePoint> {
    let Some((item, source)) = items.get(i) else {
        return Vec::new();
    };
    let start = item.position.as_seconds();
    let end = start + item.length.as_seconds();
    let rate = if item.play_rate > 0.0 {
        item.play_rate
    } else {
        1.0
    };
    let offset = item.start_offset.as_seconds();
    let period = source.last().map_or(0.0, |p| p.time.as_seconds());
    let mut inner = Vec::new();
    let mut lap = 0.0_f64;
    loop {
        for p in source {
            let at = start + lap.mul_add(period, p.time.as_seconds() - offset) / rate;
            if at > start && at < end {
                inner.push(EnvelopePoint {
                    time: PositionInSeconds::from_seconds(at),
                    value: scaled(item, p.value),
                    ..p.clone()
                });
            }
        }
        lap += 1.0;
        let next = start + lap.mul_add(period, -offset) / rate;
        if !item.loop_source || period <= 0.0 || next >= end {
            break;
        }
    }
    // Stable, so where a lap ends where the next begins, the next's
    // first point comes second and holds.
    inner.sort_by(|a, b| a.time.as_seconds().total_cmp(&b.time.as_seconds()));
    // The shape the source has at a timeline time.
    let bend_at = |t: f64| {
        let at = source_time(item, source, t);
        let after = source.partition_point(|p| p.time.as_seconds() <= at);
        after
            .checked_sub(1)
            .and_then(|k| source.get(k))
            .filter(|p| after < source.len() || p.shape == EnvelopeShape::Square)
            .map_or((EnvelopeShape::Linear, 0.0), |p| (p.shape, p.tension))
    };
    let mut out = Vec::new();
    let first_inner = inner.first().map_or(end, |p| p.time.as_seconds());
    let (shape, tension) = bend_at(start);
    if matches!(shape, EnvelopeShape::Linear | EnvelopeShape::Square) {
        if let Some(value) = item_value_at(item, source, start) {
            out.push(EnvelopePoint {
                time: PositionInSeconds::from_seconds(start),
                value,
                shape,
                tension,
                ..EnvelopePoint::default()
            });
        }
    } else {
        cut(item, source, start, first_inner, &mut out);
    }
    let last_bent = inner
        .last()
        .is_some_and(|p| bent(p) && bend_at(p.time.as_seconds()).0 == p.shape);
    match inner.pop() {
        Some(last) if last_bent => {
            out.extend(inner);
            cut(item, source, last.time.as_seconds(), end, &mut out);
        }
        Some(last) => {
            out.extend(inner);
            out.push(last);
        }
        None => {}
    }
    if let Some(value) = item_value_at(item, source, end) {
        out.push(EnvelopePoint {
            time: PositionInSeconds::from_seconds(end),
            value,
            ..EnvelopePoint::default()
        });
    }
    out
}

/// Straight pieces of an item's curve from `from` to `to` (the last
/// excluded).
fn cut(
    item: &AutomationItem,
    source: &[EnvelopePoint],
    from: f64,
    to: f64,
    out: &mut Vec<EnvelopePoint>,
) {
    for k in 0..PIECES {
        let at = (to - from).mul_add(f64::from(k) / f64::from(PIECES), from);
        if let Some(value) = item_value_at(item, source, at) {
            out.push(EnvelopePoint {
                time: PositionInSeconds::from_seconds(at),
                value,
                ..EnvelopePoint::default()
            });
        }
    }
}

/// The part of `curve` over `s..e` into `out`: its value at `s`, its own
/// points inside, and its value just short of `e` at `e`. A bent segment
/// cut at either end goes in as straight pieces.
fn stretch(curve: &[EnvelopePoint], s: f64, e: f64, out: &mut Vec<EnvelopePoint>) {
    let inside = |t: f64| t > s && t < e;
    if s.is_finite() {
        let after = curve.partition_point(|p| p.time.as_seconds() <= s);
        let before = after.checked_sub(1).and_then(|i| curve.get(i));
        match (before, curve.get(after)) {
            (Some(a), Some(b)) if bent(a) && a.time.as_seconds() < s => {
                let to = b.time.as_seconds().min(e);
                pieces(a, b, s, to, out);
            }
            _ => {
                let value = value_at(curve, s).unwrap_or(0.0);
                let shape = before.map_or(EnvelopeShape::Linear, |a| a.shape);
                let tension = before.map_or(0.0, |a| a.tension);
                out.push(EnvelopePoint {
                    time: PositionInSeconds::from_seconds(s),
                    value,
                    shape,
                    tension,
                    ..EnvelopePoint::default()
                });
            }
        }
    }
    let mut inner = curve
        .iter()
        .filter(|p| inside(p.time.as_seconds()))
        .peekable();
    while let Some(p) = inner.next() {
        let next = curve
            .iter()
            .find(|q| q.time.as_seconds() > p.time.as_seconds());
        match next {
            // The segment runs past the end, bent: pieces up to it.
            Some(n)
                if inner.peek().is_none()
                    && n.time.as_seconds() > e
                    && bent(p)
                    && e.is_finite() =>
            {
                pieces(p, n, p.time.as_seconds(), e, out);
            }
            _ => out.push(p.clone()),
        }
    }
    if e.is_finite() {
        // The value arriving at `e`, not the one leaving it.
        let after = curve.partition_point(|p| p.time.as_seconds() < e);
        let before = after.checked_sub(1).and_then(|i| curve.get(i));
        let value = match (before, curve.get(after)) {
            (Some(a), Some(b)) => between(a, b, e),
            (Some(a), None) => a.value,
            (None, b) => b.map_or(0.0, |b| b.value),
        };
        out.push(EnvelopePoint {
            time: PositionInSeconds::from_seconds(e),
            value,
            ..EnvelopePoint::default()
        });
    }
}

/// Whether a segment from `p` bends (a cut through it needs pieces).
fn bent(p: &EnvelopePoint) -> bool {
    !matches!(p.shape, EnvelopeShape::Linear | EnvelopeShape::Square)
}

/// The segment `a`→`b` from `from` to `to` (the last excluded) as
/// straight pieces.
fn pieces(a: &EnvelopePoint, b: &EnvelopePoint, from: f64, to: f64, out: &mut Vec<EnvelopePoint>) {
    for k in 0..PIECES {
        let at = (to - from).mul_add(f64::from(k) / f64::from(PIECES), from);
        out.push(EnvelopePoint {
            time: PositionInSeconds::from_seconds(at),
            value: between(a, b, at),
            ..EnvelopePoint::default()
        });
    }
}

/// Points numbered by their place.
fn renumber(mut points: Vec<EnvelopePoint>) -> Vec<EnvelopePoint> {
    for (i, p) in points.iter_mut().enumerate() {
        p.index = u32::try_from(i).unwrap_or(u32::MAX);
        p.selected = false;
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::Duration;

    fn pt(time: f64, value: f64) -> EnvelopePoint {
        EnvelopePoint {
            time: PositionInSeconds::from_seconds(time),
            value,
            ..EnvelopePoint::default()
        }
    }

    fn shaped(time: f64, value: f64, shape: EnvelopeShape) -> EnvelopePoint {
        EnvelopePoint {
            shape,
            ..pt(time, value)
        }
    }

    fn item(at: f64, length: f64) -> AutomationItem {
        AutomationItem {
            position: PositionInSeconds::from_seconds(at),
            length: Duration::from_seconds(length),
            ..AutomationItem::default()
        }
    }

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-6)
    }

    #[test]
    fn a_segment_bends_the_way_its_point_says() {
        for (shape, half) in [
            (EnvelopeShape::Linear, 0.5),
            (EnvelopeShape::Square, 0.0),
            (EnvelopeShape::SlowStartEnd, 0.5),
            (EnvelopeShape::FastStart, 0.875),
            (EnvelopeShape::FastEnd, 0.125),
        ] {
            let points = [shaped(0.0, 0.0, shape), pt(2.0, 1.0)];
            assert!(
                close(value_at(&points, 1.0), half),
                "{shape:?}: {:?}",
                value_at(&points, 1.0)
            );
        }
        // The S eases: slower than straight a quarter of the way.
        let s = [shaped(0.0, 0.0, EnvelopeShape::SlowStartEnd), pt(4.0, 1.0)];
        assert!(value_at(&s, 1.0).is_some_and(|v| v < 0.25));
    }

    #[test]
    fn two_points_at_one_time_are_a_jump() {
        let points = [pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0), pt(2.0, 1.0)];
        assert!(close(value_at(&points, 0.999), 0.0));
        assert!(close(value_at(&points, 1.0), 1.0));
        assert!(close(value_at(&[], 1.0).or(Some(-1.0)), -1.0));
    }

    /// Over its window an item's curve plays, not the envelope's; either
    /// side, the envelope's.
    #[test]
    fn an_item_plays_over_its_window() {
        let under = [pt(0.0, 0.2), pt(10.0, 0.2)];
        let items = vec![(item(4.0, 2.0), vec![pt(0.0, 0.8), pt(2.0, 0.8)])];
        let played = played(&under, &items);
        assert!(close(value_at(&played, 3.9), 0.2), "{played:?}");
        assert!(close(value_at(&played, 4.0), 0.8));
        assert!(close(value_at(&played, 5.0), 0.8));
        assert!(
            close(value_at(&played, 6.0), 0.2),
            "after its end, the envelope again"
        );
        assert!(close(value_at(&played, 9.0), 0.2));
        // Approaching the window, the envelope's value, not a ramp to it.
        assert!(close(value_at(&played, 3.99), 0.2));
    }

    #[test]
    fn a_later_item_is_on_top() {
        let under = [pt(0.0, 0.0)];
        let items = vec![
            (item(0.0, 4.0), vec![pt(0.0, 0.3)]),
            (item(1.0, 1.0), vec![pt(0.0, 0.9)]),
        ];
        let played = played(&under, &items);
        assert!(close(value_at(&played, 0.5), 0.3));
        assert!(close(value_at(&played, 1.5), 0.9), "{played:?}");
        assert!(close(value_at(&played, 3.0), 0.3));
        assert!(close(value_at(&played, 5.0), 0.0));
    }

    #[test]
    fn an_item_loops_and_scales() {
        let looped = AutomationItem {
            loop_source: true,
            amplitude: -1.0,
            ..item(0.0, 4.0)
        };
        // A ramp from 0 to 1 over a second, repeating, turned over.
        let source = vec![pt(0.0, 0.0), pt(1.0, 1.0)];
        assert!(close(item_value_at(&looped, &source, 0.25), 0.75));
        assert!(
            close(item_value_at(&looped, &source, 2.25), 0.75),
            "the second lap"
        );
        let played = played(&[], &[(looped, source)]);
        assert!(close(value_at(&played, 2.5), 0.5), "{played:?}");
    }

    #[test]
    fn a_bent_segment_cut_by_a_window_keeps_its_bend() {
        let under = [shaped(0.0, 0.0, EnvelopeShape::SlowStartEnd), pt(8.0, 1.0)];
        let items = vec![(item(4.0, 1.0), vec![pt(0.0, 0.5)])];
        let played = played(&under, &items);
        for t in [1.0, 3.0, 6.0, 7.0] {
            let want = value_at(&under, t).unwrap_or(0.0);
            let got = value_at(&played, t);
            assert!(
                got.is_some_and(|g| (g - want).abs() < 0.01),
                "{t}: {got:?} vs {want}"
            );
        }
    }
}
