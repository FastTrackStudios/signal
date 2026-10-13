//! The quantize grid in force across the song, as the edit sees it.
//!
//! The kit's default (eighths, `drum_settings`), a part's grid where a
//! Grid region says (a section of sixteenths, the snare's quarters in a
//! breakdown), a fill's own grid inside it — the most specific wins, as
//! the quantize decides. Editing, the grid lines are drawn by it rather
//! than by the zoom (four bars across fit 32nds, which no part here
//! plays), and the transport bar says which grid the cursor is on.

use std::sync::Mutex;

use crate::ruler::GridRegion;

/// The grid in force: `default` everywhere `spans` (sorted, apart) do not
/// say otherwise.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InForce {
    /// Divisions of a whole note: 8 eighths, 16 sixteenths, 12 eighth
    /// triplets.
    pub default: u32,
    pub spans: Vec<(u64, u64, u32)>,
}

/// Seconds as whole microseconds: what a span's edges are kept in, so
/// two edges that are one compare as one.
fn micros(t: f64) -> u64 {
    num_traits::ToPrimitive::to_u64(&(t.max(0.0) * 1e6).round()).unwrap_or(u64::MAX)
}

/// Back to seconds.
fn seconds(us: u64) -> f64 {
    num_traits::ToPrimitive::to_f64(&us).unwrap_or(0.0) / 1e6
}

impl InForce {
    /// `default` with `regions` over it, the shortest holding each stretch
    /// — a fill inside a part, a figure inside a fill.
    #[must_use]
    pub fn new(default: u32, regions: &[GridRegion]) -> Self {
        let mut edges: Vec<u64> = regions
            .iter()
            .flat_map(|&(a, b, _)| [micros(a), micros(b)])
            .collect();
        edges.sort_unstable();
        edges.dedup();
        let mut spans: Vec<(u64, u64, u32)> = Vec::new();
        for pair in edges.windows(2) {
            let [a, b] =
                [pair.first().copied(), pair.get(1).copied()].map(Option::unwrap_or_default);
            let held = regions
                .iter()
                .filter(|&&(ra, rb, _)| micros(ra) <= a && micros(rb) >= b)
                .min_by(|x, y| (x.1 - x.0).total_cmp(&(y.1 - y.0)))
                .map(|r| r.2);
            let Some(division) = held.filter(|d| *d != default) else {
                continue;
            };
            match spans.last_mut() {
                Some(last) if last.1 == a && last.2 == division => last.1 = b,
                _ => spans.push((a, b, division)),
            }
        }
        Self { default, spans }
    }

    /// The grid at `t` seconds.
    #[must_use]
    pub fn at(&self, t: f64) -> u32 {
        let t = micros(t);
        self.spans
            .iter()
            .find(|(a, b, _)| *a <= t && t < *b)
            .map_or(self.default, |s| s.2)
    }

    /// Each stretch from `from` to `to` seconds with its grid, in order.
    #[must_use]
    pub fn between(&self, from: f64, to: f64) -> Vec<GridRegion> {
        let mut out = Vec::new();
        let mut at = from;
        for &(a, b, division) in &self.spans {
            let (a, b) = (seconds(a), seconds(b));
            if b <= at || a >= to {
                continue;
            }
            if a > at {
                out.push((at, a, self.default));
            }
            out.push((a.max(at), b.min(to), division));
            at = b;
        }
        if at < to {
            out.push((at, to, self.default));
        }
        out
    }
}

/// The grid in force while editing, `None` otherwise: set by the window
/// each frame it draws, read by the transport bar.
static SHOWN: Mutex<Option<InForce>> = Mutex::new(None);

/// Show `in_force` (or nothing, not editing).
pub fn set(in_force: Option<InForce>) {
    if let Ok(mut shown) = SHOWN.lock()
        && *shown != in_force
    {
        *shown = in_force;
    }
}

/// The grid in force, while editing.
#[must_use]
pub fn current() -> Option<InForce> {
    SHOWN.lock().ok().and_then(|s| s.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shortest_region_says_and_eighths_elsewhere() {
        // A section of sixteenths, a fill of sixteenth triplets in it.
        let grid = InForce::new(8, &[(10.0, 20.0, 16), (14.0, 15.0, 24)]);
        assert_eq!(grid.at(5.0), 8);
        assert_eq!(grid.at(12.0), 16);
        assert_eq!(grid.at(14.5), 24);
        assert_eq!(grid.at(15.0), 16);
        assert_eq!(grid.at(20.0), 8);
        assert_eq!(
            grid.between(9.0, 21.0),
            vec![
                (9.0, 10.0, 8),
                (10.0, 14.0, 16),
                (14.0, 15.0, 24),
                (15.0, 20.0, 16),
                (20.0, 21.0, 8),
            ]
        );
    }

    #[test]
    fn a_region_of_the_default_is_no_span() {
        let grid = InForce::new(8, &[(10.0, 20.0, 8)]);
        assert!(grid.spans.is_empty());
        assert_eq!(grid.between(0.0, 30.0), vec![(0.0, 30.0, 8)]);
    }
}
