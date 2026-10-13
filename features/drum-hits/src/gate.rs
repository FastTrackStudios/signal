//! The envelope gate: a hit is where a fast envelope outruns a slow one
//! by the crest, above the threshold.
//!
//! The envelopes are `session-daw`'s `drum_edit::transients`, after
//! 80icio's Perfect Timing! — a 1 ms / 10 ms fast follower over a
//! 7 ms / 15 ms slow one — so a setting means the same in either. One
//! thing is stated more strictly: a hit fires at the first sample of a
//! **stretch** where the fast envelope outruns the slow one by the
//! crest, once it is over the threshold, and a stretch fires once. (The
//! script fires again inside a stretch that outlasts the retrigger; a
//! stretch that long is a swell, not a stroke.)
//!
//! Stated that way, the work splits two ways:
//!
//! - **Across settings.** [`Scan`] walks a signal once and keeps, for
//!   each crest in [`CRESTS`], every stretch with the sample at which it
//!   first rose past each threshold in [`THRESHOLDS`]; a setting on that
//!   grid is then a walk over a few thousand stretches ([`Scan::gate`]).
//! - **Across the song.** Where a stretch starts depends only on the
//!   envelopes, which forget anything older than a few tens of ms. So a
//!   signal is walked in [`CHUNK`]-long pieces in parallel, each warmed
//!   on the [`WARM`] before it, and the retrigger — the one thing that
//!   carries from hit to hit — is applied once, in order, after.
//!
//! A whole song's mic is scanned in a few tens of ms on a many-core
//! machine. Nothing the length of the song is kept but the signal.

use rayon::prelude::*;

/// The gate's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gate {
    /// A hit's fast envelope is over this, dBFS.
    pub threshold_db: f64,
    /// How far the fast envelope outruns the slow one at a hit, dB:
    /// what tells a stroke from a ring that is merely loud.
    pub crest_db: f64,
    /// After a hit, how long until the next can start, seconds.
    pub retrig_secs: f64,
}

impl Default for Gate {
    fn default() -> Self {
        Self {
            threshold_db: -60.0,
            crest_db: 5.0,
            retrig_secs: 0.04,
        }
    }
}

/// Thresholds a [`Scan`] can answer for, dBFS, ascending.
pub const THRESHOLDS: [f64; 27] = [
    -66.0, -64.0, -62.0, -60.0, -58.0, -56.0, -54.0, -52.0, -50.0, -48.0, -46.0, -44.0, -42.0,
    -40.0, -38.0, -36.0, -34.0, -32.0, -30.0, -28.0, -26.0, -24.0, -22.0, -20.0, -18.0, -16.0,
    -14.0,
];
/// Crests a [`Scan`] can answer for, dB.
pub const CRESTS: [f64; 5] = [3.0, 5.0, 8.0, 12.0, 16.0];

/// How much of a signal one worker walks, seconds.
pub const CHUNK: f64 = 5.0;
/// How long a worker warms the envelopes before its piece, seconds. The
/// slowest follower forgets by a factor of e every 15 ms; after this,
/// what came before is under a millionth of what it was.
pub const WARM: f64 = 0.25;

/// Not crossed.
const NEVER: u32 = u32::MAX;

/// The two envelope followers, stepping sample by sample.
struct Followers {
    fast: f64,
    slow: f64,
    fast_attack: f64,
    fast_release: f64,
    slow_attack: f64,
    slow_release: f64,
}

impl Followers {
    fn new(rate: f64, first: f32) -> Self {
        let coeff = |seconds: f64| (-1.0 / (rate.max(1.0) * seconds)).exp();
        let first = f64::from(first.abs());
        Self {
            fast: first,
            slow: first,
            fast_attack: coeff(0.001),
            fast_release: coeff(0.010),
            slow_attack: coeff(0.007),
            slow_release: coeff(0.015),
        }
    }

    /// Step to `x`; the two envelopes after it.
    fn step(&mut self, x: f32) -> (f64, f64) {
        let x = f64::from(x.abs());
        let g = if self.fast < x {
            self.fast_attack
        } else {
            self.fast_release
        };
        self.fast = g.mul_add(self.fast - x, x);
        let g = if self.slow < x {
            self.slow_attack
        } else {
            self.slow_release
        };
        self.slow = g.mul_add(self.slow - x, x);
        (self.fast, self.slow)
    }
}

/// One stretch where the fast envelope outran the slow one by a crest.
#[derive(Clone, Copy, Debug)]
struct Stretch {
    start: usize,
    /// For each level walked, how far into the stretch it was first
    /// crossed, or [`NEVER`].
    crossed: [u32; THRESHOLDS.len()],
}

/// A stretch being walked, and how many levels it has crossed.
type Open = Option<(Stretch, usize)>;

/// Walk `samples[from..to]` for stretches at each of `crests` (linear),
/// recording where each crossed each of `levels` (linear, ascending,
/// at most [`THRESHOLDS`]'s count).
///
/// The envelopes are warmed on the [`WARM`] before `from`, and a
/// stretch already under way at `from` is the previous piece's. One
/// that starts before `to` is followed past it to its end.
fn walk(
    samples: &[f32],
    rate: f64,
    (from, to): (usize, usize),
    crests: &[f64],
    levels: &[f64],
) -> Vec<Vec<Stretch>> {
    let warm_from = from.saturating_sub(super::samples(WARM, rate));
    let mut followers = Followers::new(rate, samples.get(warm_from).copied().unwrap_or(0.0));
    let mut inherited = vec![false; crests.len()];
    for x in samples.get(warm_from..from).unwrap_or_default() {
        let (fast, slow) = followers.step(*x);
        for (crest, was) in crests.iter().zip(inherited.iter_mut()) {
            *was = fast > crest * slow.max(1e-12);
        }
    }
    let mut stretches: Vec<Vec<Stretch>> = vec![Vec::new(); crests.len()];
    let mut open: Vec<Open> = vec![None; crests.len()];
    for (i, x) in samples.iter().enumerate().skip(from) {
        if i >= to && open.iter().all(Option::is_none) {
            break;
        }
        let (fast, slow) = followers.step(*x);
        for (((crest, now), done), was) in crests
            .iter()
            .zip(open.iter_mut())
            .zip(stretches.iter_mut())
            .zip(inherited.iter_mut())
        {
            if fast > crest * slow.max(1e-12) {
                if *was || (now.is_none() && i >= to) {
                    continue;
                }
                let (stretch, upto) = now.get_or_insert((
                    Stretch {
                        start: i,
                        crossed: [NEVER; THRESHOLDS.len()],
                    },
                    0,
                ));
                while let Some(level) = levels.get(*upto) {
                    if fast <= *level {
                        break;
                    }
                    if let Some(c) = stretch.crossed.get_mut(*upto) {
                        *c = u32::try_from(i.saturating_sub(stretch.start)).unwrap_or(NEVER);
                    }
                    *upto = upto.saturating_add(1);
                }
            } else {
                *was = false;
                // Kept only if it rose past the lowest level: anything
                // quieter can never fire.
                if let Some((stretch, upto)) = now.take()
                    && upto > 0
                {
                    done.push(stretch);
                }
            }
        }
    }
    for (now, done) in open.into_iter().zip(stretches.iter_mut()) {
        if let Some((stretch, upto)) = now
            && upto > 0
        {
            done.push(stretch);
        }
    }
    stretches
}

/// Walk all of `samples` in parallel pieces; each crest's stretches in
/// time order.
fn walk_all(samples: &[f32], rate: f64, crests: &[f64], levels: &[f64]) -> Vec<Vec<Stretch>> {
    let chunk = super::samples(CHUNK, rate).max(1);
    let starts: Vec<usize> = (0..samples.len()).step_by(chunk).collect();
    let pieces: Vec<Vec<Vec<Stretch>>> = starts
        .par_iter()
        .map(|&from| {
            walk(
                samples,
                rate,
                (from, from.saturating_add(chunk).min(samples.len())),
                crests,
                levels,
            )
        })
        .collect();
    let mut out: Vec<Vec<Stretch>> = vec![Vec::new(); crests.len()];
    for piece in pieces {
        for (all, some) in out.iter_mut().zip(piece) {
            all.extend(some);
        }
    }
    out
}

/// The hits among `fires` (in order): each at least `retrig` samples
/// after the last kept.
fn retrigger(fires: impl Iterator<Item = usize>, retrig: usize) -> Vec<usize> {
    let mut open_from = 0_usize;
    let mut at = Vec::new();
    for fire in fires {
        if fire >= open_from {
            at.push(fire);
            open_from = fire.saturating_add(retrig).saturating_add(1);
        }
    }
    at
}

/// Where a stretch fires at the level it crossed `level`-th.
fn fires_at(stretch: &Stretch, level: usize) -> Option<usize> {
    let offset = *stretch.crossed.get(level)?;
    (offset != NEVER).then(|| {
        stretch
            .start
            .saturating_add(usize::try_from(offset).unwrap_or(usize::MAX))
    })
}

/// A signal walked once, answering the gate at any setting on the grid.
#[derive(Clone, Debug, Default)]
pub struct Scan {
    rate: f64,
    len: usize,
    /// One list per crest in [`CRESTS`], in time order.
    stretches: Vec<Vec<Stretch>>,
}

impl Scan {
    /// Walk `samples`, read at `rate`.
    #[must_use]
    pub fn of(samples: &[f32], rate: f64) -> Self {
        let levels: Vec<f64> = THRESHOLDS.iter().map(|t| super::gain(*t)).collect();
        let crests: Vec<f64> = CRESTS.iter().map(|c| super::gain(*c)).collect();
        Self {
            rate,
            len: samples.len(),
            stretches: walk_all(samples, rate, &crests, &levels),
        }
    }

    /// The rate the signal was read at.
    #[must_use]
    pub const fn rate(&self) -> f64 {
        self.rate
    }

    /// How many samples long the signal was.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether there was no signal.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Where the gate fires at `CRESTS[crest]`, `THRESHOLDS[threshold]`
    /// and `retrig_secs`: the sample of each hit. Empty off the grid.
    #[must_use]
    pub fn gate(&self, crest: usize, threshold: usize, retrig_secs: f64) -> Vec<usize> {
        let retrig = super::samples(retrig_secs.max(0.0), self.rate);
        self.stretches
            .get(crest)
            .map_or_else(Vec::new, |stretches| {
                retrigger(
                    stretches.iter().filter_map(|s| fires_at(s, threshold)),
                    retrig,
                )
            })
    }

    /// The gate at `gate`, if it is on the grid.
    #[must_use]
    pub fn gate_at(&self, gate: &Gate) -> Option<Vec<usize>> {
        let on = |grid: &[f64], v: f64| grid.iter().position(|g| (g - v).abs() < 1e-9);
        Some(self.gate(
            on(&CRESTS, gate.crest_db)?,
            on(&THRESHOLDS, gate.threshold_db)?,
            gate.retrig_secs,
        ))
    }
}

/// Where the gate fires on `samples` at any setting: one parallel walk.
#[must_use]
pub fn gate(samples: &[f32], rate: f64, gate: &Gate) -> Vec<usize> {
    let stretches = walk_all(
        samples,
        rate,
        &[super::gain(gate.crest_db)],
        &[super::gain(gate.threshold_db)],
    );
    let retrig = super::samples(gate.retrig_secs.max(0.0), rate);
    stretches.first().map_or_else(Vec::new, |s| {
        retrigger(s.iter().filter_map(|s| fires_at(s, 0)), retrig)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A struck decay: silence, then an exponential burst.
    fn strike(rate: f64, at: usize, len: usize, peak: f32) -> Vec<f32> {
        let mut out = vec![0.0_f32; len];
        for (i, s) in out.iter_mut().enumerate().skip(at) {
            let n = i.saturating_sub(at);
            let t = super::super::seconds(n, rate);
            let alternate = if n.is_multiple_of(2) { 1.0 } else { -1.0 };
            let decay = num_traits::ToPrimitive::to_f32(&(-t / 0.05).exp()).unwrap();
            *s = peak * decay * alternate;
        }
        out
    }

    #[test]
    fn fires_once_on_a_strike() {
        let rate = 48_000.0;
        let sig = strike(rate, 4_800, 48_000, 0.5);
        let hits = gate(&sig, rate, &Gate::default());
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(hits[0] >= 4_800 && hits[0] < 4_800 + 100, "{hits:?}");
    }

    #[test]
    fn the_threshold_refuses_a_quiet_strike() {
        let rate = 48_000.0;
        let sig = strike(rate, 4_800, 48_000, 0.001);
        let g = Gate {
            threshold_db: -40.0,
            ..Gate::default()
        };
        assert!(gate(&sig, rate, &g).is_empty());
    }

    /// Strikes every 0.7 s at falling levels, across several chunks.
    fn strikes(rate: f64) -> Vec<f32> {
        let len = 16 * 48_000;
        let mut sig = vec![0.0_f32; len];
        let mut peak = 0.8_f32;
        for at in (1_000..len).step_by(33_600).take(22) {
            for (s, t) in sig.iter_mut().zip(strike(rate, at, len, peak)) {
                *s += t;
            }
            peak *= 0.7;
        }
        sig
    }

    #[test]
    fn a_scan_answers_as_the_gate_does() {
        let rate = 48_000.0;
        let sig = strikes(rate);
        let scan = Scan::of(&sig, rate);
        for (c, crest_db) in CRESTS.iter().enumerate() {
            for (t, threshold_db) in THRESHOLDS.iter().enumerate() {
                let g = Gate {
                    threshold_db: *threshold_db,
                    crest_db: *crest_db,
                    retrig_secs: 0.04,
                };
                assert_eq!(
                    scan.gate(c, t, 0.04),
                    gate(&sig, rate, &g),
                    "{threshold_db} dB / {crest_db} dB"
                );
            }
        }
    }

    #[test]
    fn walking_in_pieces_is_walking_whole() {
        let rate = 48_000.0;
        let sig = strikes(rate);
        let g = Gate {
            threshold_db: -50.0,
            ..Gate::default()
        };
        let whole = walk(
            &sig,
            rate,
            (0, sig.len()),
            &[crate::gain(g.crest_db)],
            &[crate::gain(g.threshold_db)],
        );
        let whole: Vec<usize> = whole[0].iter().filter_map(|s| fires_at(s, 0)).collect();
        assert!(whole.len() > 10, "{whole:?}");
        assert_eq!(gate(&sig, rate, &g), whole);
    }
}
