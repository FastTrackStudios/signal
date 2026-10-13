//! The drum part as written, held up to the part as played: which note
//! each hit is.
//!
//! A drummer sits ahead of the click or behind it for a whole section —
//! a pre-chorus pushed 40 ms early, a breakdown laid back — and inside
//! that feel a hit can wander a long way: a kick written on 4, played
//! 100 ms early, is nearer the sixteenth before 4 than 4 itself. The
//! nearest line puts it there, a sixteenth early for good, and every bar
//! of the section the same. The written part knows it is on 4.
//!
//! So each piece's hits are matched to that piece's own notes ([`align`]):
//! first the feel — how far the notes sit from the hits nearby that are
//! plainly theirs ([`feel_at`]) — then each hit, moved by the feel, to the
//! nearest note not already taken, one to one, within a sixteenth. A hit with no
//! note near (a variation, a fill the chart does not have) is matched to
//! nothing and placed some other way. A note with no hit is not played.

/// How near a note a hit must be to say what the feel is, sixteenths.
const PLAIN: f64 = 0.5;
/// How far from a hit, its feel taken out, its note may be, sixteenths:
/// one — a drummer pushing a section 40 ms puts some hits 60 to 100 ms
/// ahead of their notes, past the line before. Taken one to one, in
/// order, a hit the chart does not have takes no note from one it does.
const REACH: f64 = 1.0;
/// How far a hit and a note that are each other's nearest may be,
/// sixteenths: a little over one.
const MUTUAL: f64 = 1.1;

/// Fewer plain matches than this and there is no feel to speak of.
const FEEL_FROM: usize = 4;

/// How many of the nearest plain matches a hit's feel is read from: a
/// drummer drifts — a pre-chorus pushed in its first half, dragged in its
/// second — so the feel is local.
const FEEL_OF: usize = 8;

/// One piece's hits matched to its notes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Aligned {
    /// Each hit's note, in the hits' order.
    pub notes: Vec<Option<f64>>,
}

/// The note nearest `t` in `notes` (by time).
fn nearest(notes: &[f64], t: f64) -> Option<f64> {
    let at = notes.partition_point(|n| *n < t);
    [at.checked_sub(1), Some(at)]
        .into_iter()
        .flatten()
        .filter_map(|i| notes.get(i).copied())
        .min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs()))
}

/// The hits plainly their notes — within half a sixteenth (`step`
/// seconds) — and how far each note sits after its hit: `(hit, offset)`.
#[must_use]
pub fn plain(hits: &[f64], notes: &[f64], step: f64) -> Vec<(f64, f64)> {
    hits.iter()
        .filter_map(|h| nearest(notes, *h).map(|n| (*h, n - h)))
        .filter(|(_, d)| d.abs() <= PLAIN * step)
        .collect()
}

/// The feel at `t`: how far the notes sit after the hits there.
///
/// Positive when the drummer plays ahead: the median of the [`FEEL_OF`]
/// plain matches (`(hit, offset)`, by time) nearest it; none with fewer
/// than [`FEEL_FROM`].
#[must_use]
pub fn feel_at(plain: &[(f64, f64)], t: f64) -> f64 {
    if plain.len() < FEEL_FROM {
        return 0.0;
    }
    let at = plain.partition_point(|(h, _)| *h < t);
    let (mut lo, mut hi) = (at, at);
    while hi.saturating_sub(lo) < FEEL_OF {
        let left = lo
            .checked_sub(1)
            .and_then(|i| plain.get(i))
            .map(|p| t - p.0);
        let right = plain.get(hi).map(|p| p.0 - t);
        match (left, right) {
            (Some(l), Some(r)) if l <= r => lo = lo.saturating_sub(1),
            (Some(_), None) => lo = lo.saturating_sub(1),
            (_, Some(_)) => hi = hi.saturating_add(1),
            (None, None) => break,
        }
    }
    let mut near: Vec<f64> = plain
        .get(lo..hi)
        .unwrap_or_default()
        .iter()
        .map(|p| p.1)
        .collect();
    near.sort_by(f64::total_cmp);
    near.get(near.len() / 2).copied().unwrap_or(0.0)
}

/// `hits` (one piece's, by time) matched to `notes` (the same piece's
/// written notes, by time), a sixteenth being `step` seconds and the feel
/// at a time `feel` (see [`feel_at`]).
#[must_use]
pub fn align(hits: &[f64], notes: &[f64], step: f64, feel: impl Fn(f64) -> f64) -> Aligned {
    // Every pairing within reach, nearest first; each hit and each note
    // taken once.
    let reach = REACH * step;
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (i, h) in hits.iter().enumerate() {
        let c = h + feel(*h);
        let from = notes.partition_point(|n| *n < c - reach);
        for (j, n) in notes.iter().enumerate().skip(from) {
            if *n > c + reach {
                break;
            }
            pairs.push(((n - c).abs(), i, j));
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut of_hit: Vec<Option<usize>> = vec![None; hits.len()];
    let mut taken = vec![false; notes.len()];
    for (_, i, j) in pairs {
        if of_hit.get(i).copied().flatten().is_some() || taken.get(j).copied().unwrap_or(true) {
            continue;
        }
        // Never out of order: a hit after another is never given a note
        // before that one's.
        let crosses = of_hit
            .iter()
            .enumerate()
            .any(|(k, other)| other.is_some_and(|o| (k < i && o > j) || (k > i && o < j)));
        if crosses {
            continue;
        }
        if let Some(slot) = of_hit.get_mut(i) {
            *slot = Some(j);
        }
        if let Some(t) = taken.get_mut(j) {
            *t = true;
        }
    }
    // A hit the feel left out of reach — played against the drift around
    // it — is still its note's when they are each other's nearest: no
    // other hit of the piece nearer that note, no other note nearer the
    // hit, within a sixteenth and a little.
    for (i, h) in hits.iter().enumerate() {
        if of_hit.get(i).copied().flatten().is_some() {
            continue;
        }
        let Some((j, n)) = notes
            .iter()
            .enumerate()
            .min_by(|a, b| (a.1 - h).abs().total_cmp(&(b.1 - h).abs()))
        else {
            continue;
        };
        let theirs = hits
            .iter()
            .min_by(|a, b| (*a - n).abs().total_cmp(&(*b - n).abs()))
            .is_some_and(|nearest| (nearest - h).abs() < 1e-12);
        let free = !taken.get(j).copied().unwrap_or(true);
        let crosses = of_hit
            .iter()
            .enumerate()
            .any(|(k, other)| other.is_some_and(|o| (k < i && o > j) || (k > i && o < j)));
        if theirs && free && !crosses && (n - h).abs() <= MUTUAL * step {
            if let Some(slot) = of_hit.get_mut(i) {
                *slot = Some(j);
            }
            if let Some(t) = taken.get_mut(j) {
                *t = true;
            }
        }
    }
    Aligned {
        notes: of_hit
            .into_iter()
            .map(|j| j.and_then(|j| notes.get(j).copied()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 160 bpm: a sixteenth is 93.75 ms.
    const STEP: f64 = 0.093_75;

    /// The pre-chorus of Unbreakable: the kick written on 1, the "and" of
    /// 2 and 4, played 40 ms early — and the one on 4 100 ms early, nearer
    /// the sixteenth before it. Its note is still 4.
    /// r[verify quantize.written]
    #[test]
    fn a_hit_pushed_past_the_line_before_is_still_its_notes() {
        let bar = 61.5;
        let beat = 0.375;
        let notes: Vec<f64> = (0..4)
            .flat_map(|b| {
                let at = (f64::from(b) * 4.0).mul_add(beat, bar);
                [at, 1.5f64.mul_add(beat, at), 3.0f64.mul_add(beat, at)]
            })
            .collect();
        let mut hits: Vec<f64> = notes.iter().map(|n| n - 0.040).collect();
        hits[2] = notes[2] - 0.100;
        let plain = plain(&hits, &notes, STEP);
        assert!((feel_at(&plain, hits[2]) - 0.040).abs() < 1e-9);
        let aligned = align(&hits, &notes, STEP, |t| feel_at(&plain, t));
        for (k, n) in aligned.notes.iter().enumerate() {
            assert_eq!(*n, Some(notes[k]), "hit {k}");
        }
        let nearest_line = (hits[2] / STEP).round() * STEP;
        assert!(
            (nearest_line - (notes[2] - STEP)).abs() < 1e-6,
            "the nearest line is the sixteenth before"
        );
    }

    /// A hit the chart does not have is nobody's note — and never takes
    /// one from the hit that is.
    #[test]
    fn an_extra_hit_takes_no_note() {
        let notes = [1.0, 1.375, 1.75, 2.125];
        let hits = [0.99, 1.37, 1.47, 1.75, 2.12];
        let aligned = align(&hits, &notes, STEP, |_| 0.0);
        assert_eq!(
            aligned.notes,
            [Some(1.0), Some(1.375), None, Some(1.75), Some(2.125)]
        );
    }

    /// Too few plain matches is no feel; nothing written is nothing
    /// matched.
    #[test]
    fn no_notes_no_matches() {
        let aligned = align(&[1.0, 2.0], &[], STEP, |_| 0.0);
        assert_eq!(
            aligned,
            Aligned {
                notes: vec![None, None]
            }
        );
        let plain = plain(&[1.0, 2.0], &[1.01, 2.01], STEP);
        assert!(
            feel_at(&plain, 1.5).abs() < 1e-12,
            "two matches are no feel"
        );
        let aligned = align(&[1.0, 2.0], &[1.01, 2.01], STEP, |t| feel_at(&plain, t));
        assert_eq!(aligned.notes, [Some(1.01), Some(2.01)]);
    }

    /// A hit dragged late among hits pushed early is out of the feel's
    /// reach, and still its note's: they are each other's nearest.
    #[test]
    fn a_hit_against_the_drift_is_its_nearest_notes() {
        let notes = [0.0, 0.375, 0.5625, 0.75, 1.125, 1.5];
        let mut hits: Vec<f64> = notes.iter().map(|n| n - 0.030).collect();
        hits[2] = notes[2] + 0.079;
        let plain = plain(&hits, &notes, STEP);
        let aligned = align(&hits, &notes, STEP, |t| feel_at(&plain, t));
        assert_eq!(aligned.notes[2], Some(notes[2]));
        // Two hits either side of one note: only the nearer is its.
        let aligned = align(&[0.98, 1.06], &[1.0], STEP, |_| 0.0);
        assert_eq!(aligned.notes, [Some(1.0), None]);
    }

    /// A section pushed in its first half and dragged in its second has
    /// no feel on the whole; each half has its own, and the hit pushed
    /// past the line before is still its note's.
    /// r[verify quantize.written]
    #[test]
    fn the_feel_is_read_where_the_hit_is() {
        let beat = 0.375;
        let notes: Vec<f64> = (0..32).map(|b| f64::from(b) * 2.0 * beat).collect();
        let mut hits: Vec<f64> = notes
            .iter()
            .enumerate()
            .map(|(k, n)| if k < 16 { n - 0.040 } else { n + 0.040 })
            .collect();
        hits[3] = notes[3] - 0.100;
        let plain = plain(&hits, &notes, STEP);
        let mut all: Vec<f64> = plain.iter().map(|p| p.1).collect();
        all.sort_by(f64::total_cmp);
        assert!(
            all[all.len() / 2].abs() <= 0.040,
            "the section on the whole"
        );
        assert!((feel_at(&plain, hits[3]) - 0.040).abs() < 1e-9);
        assert!((feel_at(&plain, hits[28]) + 0.040).abs() < 1e-9);
        let aligned = align(&hits, &notes, STEP, |t| feel_at(&plain, t));
        assert_eq!(aligned.notes[3], Some(notes[3]));
        let whole = align(&hits, &notes, STEP, |_| 0.0);
        assert_eq!(
            whole.notes[3],
            Some(notes[3]),
            "read on the whole it is out of the feel's reach, and its note's as the nearest"
        );
    }
}
