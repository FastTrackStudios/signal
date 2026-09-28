//! Breakpoint envelopes — Omnisphere's envelope model, measured against the
//! real plugin (see `features/rigs/synth/tools/omni_probe`).
//!
//! An envelope is a run of points (`level` at `time`). The segment leaving a
//! point takes that point's shape: a curve `k` (`0` linear, `> 0`
//! fast-then-slow, `< 0` slow-then-fast; `g(x) = (1 − e^(−kx)) / (1 − e^(−k))`
//! on amplitude) or a step (hold, then jump at the next point). An amp or
//! filter envelope has a **sustain** point (the penultimate) where it holds
//! while the key is down; on release it runs the final segment from wherever
//! it is, over that segment's whole duration. A mod envelope has none and
//! runs free, optionally looping.

/// One breakpoint: `level` at `time` (seconds, or beats when synced). Its
/// `curve` and `step` shape the segment that LEAVES it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegPoint {
    pub time: f32,
    pub level: f32,
    /// Shape of the segment from this point: `0` linear, `> 0`
    /// fast-then-slow, `< 0` slow-then-fast.
    pub curve: f32,
    /// Hold this level until the next point, then jump (a step).
    pub step: bool,
}

/// The shape `g(x)` (0..1 → 0..1) of a segment with curve `k`.
#[inline]
#[must_use]
pub fn shape(k: f32, x: f32) -> f32 {
    if k.abs() < 1e-3 {
        x
    } else {
        (1.0 - (-k * x).exp()) / (1.0 - (-k).exp())
    }
}

/// An envelope's points and structure.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Breakpoints {
    pub points: Vec<SegPoint>,
    /// Hold here while the key is down (an amp/filter envelope's penultimate
    /// point); `None` runs free.
    pub sustain: Option<usize>,
    /// Free-running: start over at the end.
    pub looping: bool,
}

impl Breakpoints {
    /// Points sorted by time; `sustain` is an index into them.
    #[must_use]
    pub fn new(mut points: Vec<SegPoint>, sustain: Option<usize>, looping: bool) -> Self {
        points.sort_by(|a, b| a.time.total_cmp(&b.time));
        let sustain = sustain.filter(|&s| s + 1 < points.len());
        Self {
            points,
            sustain,
            looping,
        }
    }

    /// An amp/filter envelope: sustain at the penultimate point.
    #[must_use]
    pub fn with_penultimate_sustain(points: Vec<SegPoint>) -> Self {
        let n = points.len();
        Self::new(points, n.checked_sub(2), false)
    }

    #[must_use]
    pub fn end(&self) -> f32 {
        self.points.last().map_or(0.0, |p| p.time)
    }

    /// The level at `t` along the (unreleased) run.
    #[must_use]
    pub fn level_at(&self, t: f32) -> f32 {
        let Some(first) = self.points.first() else {
            return 0.0;
        };
        if t < first.time {
            return first.level;
        }
        for w in self.points.windows(2) {
            let (a, b) = (w[0], w[1]);
            if t < b.time {
                if a.step {
                    return a.level;
                }
                let x = (t - a.time) / (b.time - a.time).max(1e-9);
                return a.level + (b.level - a.level) * shape(a.curve, x);
            }
        }
        self.points.last().map_or(0.0, |p| p.level)
    }
}

/// One playing instance of a [`Breakpoints`] envelope.
#[derive(Clone, Copy, Debug, Default)]
pub struct EnvPlayer {
    /// Position in the run, `None` before the first note.
    t: Option<f32>,
    /// Released: `(time since release, level at release)`.
    released: Option<(f32, f32)>,
    level: f32,
}

impl EnvPlayer {
    pub fn note_on(&mut self) {
        self.t = Some(0.0);
        self.released = None;
    }

    /// Start the release from the current level (only an envelope with a
    /// sustain releases; a free-running one ignores the key).
    pub fn note_off(&mut self, bp: &Breakpoints) {
        if bp.sustain.is_some() && self.t.is_some() && self.released.is_none() {
            self.released = Some((0.0, self.level));
        }
    }

    /// Finished: released past its end (or never started).
    #[must_use]
    pub fn is_idle(&self, bp: &Breakpoints) -> bool {
        match (self.t, self.released, bp.sustain) {
            (None, _, _) => true,
            (Some(_), Some((rt, _)), Some(s)) => {
                let last = bp.points.len() - 1;
                rt >= bp.points[last].time - bp.points[s].time
            }
            _ => false,
        }
    }

    /// The level now, then advance by `dt`.
    pub fn tick(&mut self, bp: &Breakpoints, dt: f32) -> f32 {
        let Some(t) = self.t else {
            return 0.0;
        };
        let v = match (self.released, bp.sustain) {
            (Some((rt, from)), Some(s)) => {
                let last = bp.points.len() - 1;
                let (a, b) = (bp.points[s], bp.points[last]);
                let dur = (b.time - a.time).max(1e-6);
                let x = (rt / dur).min(1.0);
                self.released = Some((rt + dt, from));
                if a.step && x < 1.0 {
                    from
                } else {
                    from + (b.level - from) * shape(a.curve, x)
                }
            }
            _ => {
                // Holding at the sustain point: its own level (several
                // points can share its time — an instant attack).
                let v = match bp.sustain {
                    Some(s) if t >= bp.points[s].time => bp.points[s].level,
                    _ => bp.level_at(t),
                };
                let mut next = t + dt;
                if let Some(s) = bp.sustain {
                    next = next.min(bp.points[s].time);
                } else if bp.looping && bp.end() > 0.0 && next >= bp.end() {
                    next %= bp.end();
                }
                self.t = Some(next);
                v
            }
        };
        self.level = v;
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(time: f32, level: f32, curve: f32, step: bool) -> SegPoint {
        SegPoint {
            time,
            level,
            curve,
            step,
        }
    }

    #[test]
    fn it_holds_at_sustain_and_releases_from_where_it_is() {
        let bp = Breakpoints::with_penultimate_sustain(vec![
            pt(0.0, 0.0, 0.0, false),
            pt(0.1, 1.0, 0.0, false),
            pt(0.5, 0.5, 0.0, false),
            pt(1.5, 0.0, 0.0, false),
        ]);
        let mut p = EnvPlayer::default();
        p.note_on();
        let dt = 0.01;
        let mut v = 0.0;
        for _ in 0..200 {
            v = p.tick(&bp, dt);
        }
        assert!((v - 0.5).abs() < 1e-3, "sustains at 0.5: {v}");
        p.note_off(&bp);
        // Half the release (0.5 s of 1 s): linear from 0.5 → 0.25.
        for _ in 0..50 {
            v = p.tick(&bp, dt);
        }
        assert!((v - 0.25).abs() < 0.02, "{v}");
        for _ in 0..60 {
            p.tick(&bp, dt);
        }
        assert!(p.is_idle(&bp));
    }

    #[test]
    fn coincident_points_hold_the_sustain_level() {
        // An instant attack: three points at t = 0, the last the sustain.
        let bp = Breakpoints::with_penultimate_sustain(vec![
            pt(0.0, 0.0, 0.0, false),
            pt(0.0, 1.0, 0.0, false),
            pt(0.0, 1.0, 0.0, false),
            pt(0.05, 0.0, 0.0, false),
        ]);
        let mut p = EnvPlayer::default();
        p.note_on();
        let mut v = 0.0;
        for _ in 0..10 {
            v = p.tick(&bp, 0.01);
        }
        assert!((v - 1.0).abs() < 1e-6, "{v}");
    }

    #[test]
    fn a_free_running_envelope_ignores_the_key_and_loops() {
        let bp = Breakpoints::new(
            vec![pt(0.0, 0.0, 0.0, false), pt(1.0, 1.0, 0.0, false)],
            None,
            true,
        );
        let mut p = EnvPlayer::default();
        p.note_on();
        p.note_off(&bp);
        let mut v = 0.0;
        for _ in 0..150 {
            v = p.tick(&bp, 0.01);
        }
        assert!((v - 0.49).abs() < 0.02, "looped to half way: {v}");
        assert!(!p.is_idle(&bp));
    }
}
