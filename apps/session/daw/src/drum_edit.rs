//! Drum editing's judgement, apart from the engine: which of a drum's
//! hits a grid line claims, and where the fills are.
//!
//! The grid scan follows 80icio's Perfect Timing! (a REAPER script for
//! exactly this: multitrack drum quantizing by splits and crossfades):
//! one hit for each grid line, the loudest near it, within a
//! **tolerance** — a flam, a bounce, a ring's second peak are not cut on,
//! and a hit nowhere near a line is not cut at all. The hits themselves
//! are the hit markers' (`crate::hits`).
//!
//! Fills are this module's own. A fill is where the drummer leaves the
//! groove, and it is the part of a performance a blanket quantize ruins:
//! it is played against the grid on purpose, often in another
//! subdivision, so its hits are found and left for a hand to edit.

/// The lane a track's edits are made on: its `EDIT` lane, once the kit
/// has been given one (`DRUMS_EDIT_LANES`) — whatever lane is playing,
/// so the comp is never cut while it is being listened to.
#[must_use]
pub fn edit_lane(track: &daw_proto::Track) -> Option<u32> {
    track
        .lane_names
        .iter()
        .position(|n| n.trim().eq_ignore_ascii_case("EDIT"))
        .and_then(|i| u32::try_from(i).ok())
        .filter(|&i| i < track.lane_count)
}

/// Whether `item` is one of the takes an edit of `track` works on: on its
/// `EDIT` lane if it has one, else on a lane that plays.
#[must_use]
pub fn is_edited(track: &daw_proto::Track, item: &daw_proto::Item) -> bool {
    edit_lane(track).map_or_else(
        || daw_ui::studio::project::on_playing_lane(track, item),
        |lane| item.fixed_lane.unwrap_or(0) == lane,
    )
}

/// A hit on the kit: when, and how loud against its own drum's loudest
/// (dB, 0 the loudest) — so a kick and a snare are weighed alike.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KitHit {
    pub time: f64,
    pub relative_db: f64,
}

/// One hit for each grid line: the loudest within `tolerance` of it —
/// `line_of` gives a time's nearest line. A hit no line claims is left
/// out, and so not cut on.
#[must_use]
pub fn grid_scan(hits: &[KitHit], line_of: impl Fn(f64) -> f64, tolerance: f64) -> Vec<KitHit> {
    let mut out: Vec<(f64, KitHit)> = Vec::new();
    for hit in hits {
        let line = line_of(hit.time);
        if (hit.time - line).abs() > tolerance {
            continue;
        }
        match out.iter_mut().find(|(l, _)| (l - line).abs() < 1e-6) {
            Some((_, kept)) if hit.relative_db > kept.relative_db => *kept = *hit,
            Some(_) => {}
            None => out.push((line, *hit)),
        }
    }
    let mut out: Vec<KitHit> = out.into_iter().map(|(_, h)| h).collect();
    out.sort_by(|a, b| a.time.total_cmp(&b.time));
    out
}

/// What a drum is, to a fill's eye.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    Kick,
    Snare,
    Tom,
    Other,
}

impl Piece {
    /// By a piece's name: `Kick`, `Snare`, `Tom 2`, `Floor Tom`…
    #[must_use]
    pub fn of(name: &str) -> Self {
        let name = name.to_ascii_lowercase();
        if name.contains("kick") || name.contains("bass drum") {
            Self::Kick
        } else if name.contains("snare") {
            Self::Snare
        } else if name.contains("tom") {
            Self::Tom
        } else {
            Self::Other
        }
    }

    /// Where its twelve slots start in a beat's shape.
    const fn shift(self) -> u32 {
        match self {
            Self::Kick => 0,
            Self::Snare => 12,
            Self::Tom => 24,
            Self::Other => 36,
        }
    }
}

/// One beat of the song.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beat {
    pub start: f64,
    pub length: f64,
    /// Which beat of its bar, from 0.
    pub in_bar: u32,
}

/// Where in a beat a hit can be: twelve places, so sixteenths and
/// triplets each have their own.
pub(crate) const SLOTS: f64 = 12.0;

/// How many bars either side the groove is learned from.
const AROUND_BARS: usize = 8;

/// A stretch of the song played as a fill, with why it was called one.
#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    pub from: f64,
    pub to: f64,
    pub beats: u32,
    /// `toms`, `busier`, `different` — each beat's reasons, merged.
    pub why: Vec<&'static str>,
}

impl Fill {
    /// Whether the hit at `time` is the fill's: a hit belongs to the beat
    /// it is nearest the start of, as [`fills`] counted it, so a landing
    /// pushed a little early is still the next bar's.
    #[must_use]
    pub fn holds(&self, time: f64, beat_length: f64) -> bool {
        let shift = beat_length / SLOTS / 2.0;
        time >= self.from - shift && time < self.to - shift
    }
}

/// What a beat holds: a bit for each piece's slot, and how many hits.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Shape {
    bits: u64,
    count: u32,
}

impl Shape {
    const fn of(self, piece: Piece) -> u64 {
        (self.bits >> piece.shift()) & 0xfff
    }

    /// The snare and toms' slots: the hands — a kick moved is a variation,
    /// a run on the snare is a fill.
    const fn hands(self) -> u64 {
        self.bits & !0xfff
    }
}

/// The fills among `beats` (in order, covering the song), from the kit's
/// hits by piece.
///
/// A beat is a fill's when, against the groove (the shape most often
/// played at that place in the bar over the eight bars either side), it
/// has toms the groove does not (`toms`), three or more hits more than
/// it (`busier`), or its hands play another pattern where the groove is
/// steady (`different`). Beats in a row join into one fill; a lone beat
/// that is only `different` is a variation, not a fill.
#[must_use]
pub fn fills(hits: &[(Piece, f64)], beats: &[Beat]) -> Vec<Fill> {
    let shapes = shapes(hits, beats);
    let per_bar = beats
        .iter()
        .map(|b| b.in_bar)
        .max()
        .map_or(4, |m| m.saturating_add(1));
    let around = AROUND_BARS.saturating_mul(usize::try_from(per_bar).unwrap_or(4));
    let marked: Vec<Vec<&'static str>> = beats
        .iter()
        .zip(&shapes)
        .enumerate()
        .map(|(i, (beat, shape))| {
            // The groove before and the groove after, apart: a fill
            // leaves both, where a new section's first bars only leave
            // the one before.
            let side = |after: bool| -> Vec<Shape> {
                beats
                    .iter()
                    .zip(&shapes)
                    .enumerate()
                    .filter(|(j, (b, _))| {
                        (*j > i) == after
                            && *j != i
                            && b.in_bar == beat.in_bar
                            && j.abs_diff(i) <= around
                    })
                    .map(|(_, (_, s))| *s)
                    .collect()
            };
            match (mode(&side(false)), mode(&side(true))) {
                (Some(before), Some(after)) => {
                    let later = reasons(*shape, after);
                    reasons(*shape, before)
                        .into_iter()
                        .filter(|w| later.contains(w))
                        .collect()
                }
                (Some(one), None) | (None, Some(one)) => reasons(*shape, one),
                (None, None) => Vec::new(),
            }
        })
        .collect();
    let mut out: Vec<Fill> = Vec::new();
    let mut run: Option<(Beat, Beat, u32, Vec<&'static str>)> = None;
    for entry in beats.iter().zip(&marked).map(Some).chain([None]) {
        match (entry, run.as_mut()) {
            (Some((beat, why)), Some((_, last, count, all))) if !why.is_empty() => {
                *last = *beat;
                *count = count.saturating_add(1);
                for w in why {
                    if !all.contains(w) {
                        all.push(w);
                    }
                }
            }
            (Some((beat, why)), None) if !why.is_empty() => {
                run = Some((*beat, *beat, 1, why.clone()));
            }
            _ => {
                // A lone beat that is only another pattern is a variation.
                if let Some((first, last, count, why)) = run.take()
                    && !(count == 1 && why == ["different"])
                {
                    out.push(Fill {
                        from: first.start,
                        to: last.start + last.length,
                        beats: count,
                        why,
                    });
                }
            }
        }
    }
    out
}

/// Why a beat of `shape` is a fill's, against the `groove` there (and
/// whether that groove is steady) — none when it is not.
fn reasons(shape: Shape, (groove, steady): (Shape, bool)) -> Vec<&'static str> {
    let mut why = Vec::new();
    if shape.count == 0 {
        return why;
    }
    if shape.of(Piece::Tom) != 0 && groove.of(Piece::Tom) == 0 {
        why.push("toms");
    }
    if shape.count >= groove.count.saturating_add(3) {
        why.push("busier");
    }
    let (played, usual) = (shape.hands(), groove.hands());
    let union = (played | usual).count_ones();
    if steady && union > 0 && f64::from((played & usual).count_ones()) / f64::from(union) < 0.5 {
        why.push("different");
    }
    why
}

/// Each beat's shape: a hit is the beat's it is nearest the start of, by
/// half a slot, and in the slot it is nearest.
fn shapes(hits: &[(Piece, f64)], beats: &[Beat]) -> Vec<Shape> {
    let mut shapes = vec![Shape::default(); beats.len()];
    for &(piece, time) in hits {
        let at = beats.partition_point(|b| b.start - b.length / SLOTS / 2.0 <= time);
        let Some(i) = at.checked_sub(1) else { continue };
        let (Some(beat), Some(shape)) = (beats.get(i), shapes.get_mut(i)) else {
            continue;
        };
        if time >= beat.start + beat.length - beat.length / SLOTS / 2.0 {
            continue;
        }
        let slot = ((time - beat.start) / beat.length * SLOTS)
            .round()
            .clamp(0.0, SLOTS - 1.0);
        let slot = num_traits::ToPrimitive::to_u32(&slot).unwrap_or(0);
        shape.bits |= 1_u64
            .checked_shl(piece.shift().saturating_add(slot))
            .unwrap_or(0);
        shape.count = shape.count.saturating_add(1);
    }
    shapes
}

/// The most common shape, and whether it is steady (played in at least
/// two in five of them).
fn mode(shapes: &[Shape]) -> Option<(Shape, bool)> {
    let mut counts: Vec<(Shape, usize)> = Vec::new();
    for s in shapes {
        match counts.iter_mut().find(|(c, _)| c == s) {
            Some((_, n)) => *n = n.saturating_add(1),
            None => counts.push((*s, 1)),
        }
    }
    let (shape, n) = counts.into_iter().max_by_key(|(_, n)| *n)?;
    Some((shape, n.saturating_mul(5) >= shapes.len().saturating_mul(2)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Once a track has an EDIT lane its edits are made there, whichever
    /// lane plays; before, on what plays.
    #[test]
    fn edits_are_made_on_the_edit_lane() {
        let item = |lane| daw_proto::Item {
            fixed_lane: Some(lane),
            ..daw_proto::Item::default()
        };
        let mut track = daw_proto::Track {
            lane_count: 3,
            lane_names: vec!["COMP".into(), "1".into(), "EDIT".into()],
            lane_play_mask: 1,
            ..daw_proto::Track::default()
        };
        assert!(is_edited(&track, &item(2)), "EDIT, with COMP playing");
        assert!(!is_edited(&track, &item(0)), "never the comp");
        track.lane_names = vec!["C1".into(), "1".into(), "2".into()];
        assert!(is_edited(&track, &item(0)), "no EDIT: what plays");
        assert!(!is_edited(&track, &item(2)));
    }

    #[test]
    fn a_grid_line_keeps_its_loudest_and_far_hits_are_left() {
        let line = |t: f64| (t * 8.0).round() / 8.0;
        let hit = |time, relative_db| KitHit { time, relative_db };
        let hits = [
            hit(0.99, -12.0), // a flam's grace note
            hit(1.004, -1.0), // its stroke
            hit(1.06, -3.0),  // halfway: no line's
            hit(1.248, -6.0),
        ];
        let kept = grid_scan(&hits, line, 0.03);
        assert_eq!(kept, vec![hits[1], hits[3]]);
    }

    /// `n` bars of 4/4 at 120…
    fn bars(n: u32) -> Vec<Beat> {
        (0..n.saturating_mul(4))
            .map(|i| Beat {
                start: f64::from(i) * 0.5,
                length: 0.5,
                in_bar: i % 4,
            })
            .collect()
    }

    /// …of a rock groove: kick on 1 and 3, snare on 2 and 4.
    fn groove(beats: &[Beat], skip: impl Fn(usize) -> bool) -> Vec<(Piece, f64)> {
        beats
            .iter()
            .enumerate()
            .filter(|(i, _)| !skip(*i))
            .map(|(_, b)| {
                let piece = if b.in_bar % 2 == 0 {
                    Piece::Kick
                } else {
                    Piece::Snare
                };
                (piece, b.start + 0.004)
            })
            .collect()
    }

    #[test]
    fn a_steady_groove_has_no_fills() {
        let beats = bars(12);
        assert!(fills(&groove(&beats, |_| false), &beats).is_empty());
    }

    /// Bar 8's last two beats are a tom run in sixteenths: one fill, those
    /// two beats, its landing on the next downbeat outside it.
    #[test]
    fn a_tom_run_is_a_fill_and_its_landing_is_not() {
        let beats = bars(12);
        let fill_beats = [30, 31];
        let mut hits = groove(&beats, |i| fill_beats.contains(&i));
        for &i in &fill_beats {
            for k in 0..4 {
                hits.push((Piece::Tom, f64::from(k).mul_add(0.125, beats[i].start)));
            }
        }
        let found = fills(&hits, &beats);
        assert_eq!(found.len(), 1, "{found:?}");
        let fill = &found[0];
        assert!((fill.from - 15.0).abs() < 1e-9 && (fill.to - 16.0).abs() < 1e-9);
        assert!(fill.why.contains(&"toms") && fill.why.contains(&"busier"));
        assert!(fill.holds(15.6, 0.5));
        assert!(
            !fill.holds(15.99, 0.5),
            "a landing pushed early is the next bar's"
        );
    }

    /// A snare run in sixteenths on beat 4 — no toms — is a fill too.
    #[test]
    fn a_snare_run_is_a_fill() {
        let beats = bars(12);
        let mut hits = groove(&beats, |i| i == 23);
        for k in 0..4 {
            hits.push((Piece::Snare, f64::from(k).mul_add(0.125, beats[23].start)));
        }
        let found = fills(&hits, &beats);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].beats, 1);
    }

    /// A new section's busier groove is a new groove, not eight bars of
    /// fill: its first bars are like the ones after them.
    #[test]
    fn a_new_groove_is_not_a_fill() {
        let beats = bars(16);
        let mut hits = groove(&beats, |_| false);
        for beat in beats.iter().skip(32).filter(|b| b.in_bar == 1) {
            for k in 1..4 {
                hits.push((Piece::Snare, f64::from(k).mul_add(0.125, beat.start)));
            }
        }
        assert!(
            fills(&hits, &beats).is_empty(),
            "{:?}",
            fills(&hits, &beats)
        );
    }

    /// A kick moved off the beat is a variation of the groove, not a fill.
    #[test]
    fn a_moved_kick_is_not_a_fill() {
        let beats = bars(12);
        let mut hits = groove(&beats, |i| i == 20);
        hits.push((Piece::Kick, beats[20].start + 0.25));
        assert!(fills(&hits, &beats).is_empty());
    }
}
