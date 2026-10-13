//! Putting events on a grid — one tool, every mode.
//!
//! Written once over [`Timed`]. A MIDI note, an audio transient and a
//! pitch-detected note quantize through the same body, so the three can
//! never disagree about which division a thing belongs to.
//!
//! The output is a [`Plan`]: a list of events and where each should end
//! up. Nothing is written here. What is done with the plan is the
//! domain's business — MIDI applies it with [`apply`], audio warps or
//! splits the take with it (see `expression_editor_audio::quantize`) —
//! and both are built from the same decisions.

use crate::event::{Sustained, Timed};

/// How events are matched to grid divisions.
///
/// All four time-shaped fields are in **the events' own time unit** —
/// seconds for transients, ticks or frames for notes. See the module
/// docs on [`crate::event`] for why this seam is not in seconds.
#[derive(Clone, Copy, Debug)]
pub struct QuantizeConfig {
    /// Distance between grid divisions.
    pub grid: f64,
    /// Where the grid starts, measured from zero in the events' unit.
    ///
    /// Takes do not begin on a division, and assuming they do puts every
    /// event out by the remainder.
    pub grid_offset: f64,
    /// Half-width of the window each division scans. `None` turns grid
    /// scan off: every event snaps to its own nearest division instead.
    pub tolerance: Option<f64>,
    /// `0.0` leaves the events alone, `1.0` puts every one on its
    /// division. Not to be confused with [`Timed::strength`], which is
    /// the *event's* weight — this is how hard the tool pulls.
    pub strength: f64,
    /// Events weaker than this are left alone entirely.
    ///
    /// The sensitivity filter. Audio gets one for free upstream — the
    /// detector's gate never reports the bleed — but MIDI has no
    /// detector, so this is where a ghost note on a programmed kit is
    /// excluded from the grid without being deleted.
    pub min_strength: f64,
}

impl Default for QuantizeConfig {
    fn default() -> Self {
        Self {
            grid: 1.0,
            grid_offset: 0.0,
            tolerance: None,
            strength: 1.0,
            min_strength: 0.0,
        }
    }
}

/// One event and where it is going.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    /// Which event this move is for — its index in the slice handed to
    /// [`plan`].
    ///
    /// An index rather than a position, because applying a plan by
    /// matching float positions back to events is how a hit that moved
    /// two milliseconds ends up applied to its neighbour.
    pub index: usize,
    /// Where the event is now.
    pub from: f64,
    /// Where it should be afterwards. Strength is already applied —
    /// this is the final position, not the division.
    pub to: f64,
    /// The division it was matched to, for display. Unaffected by
    /// strength.
    pub division: f64,
    /// The event's length, when it has one, so a preview can be drawn
    /// without going back to the events.
    ///
    /// Quantize never changes it: a note keeps its length and its end
    /// moves with its start, and a transient has no end to keep.
    pub length: Option<f64>,
}

impl Move {
    /// How far the event moves. Positive is later.
    pub fn shift(&self) -> f64 {
        self.to - self.from
    }

    /// Where the event will sit afterwards, as `(start, end)` — `None`
    /// for an event that is only a moment.
    pub fn span(&self) -> Option<(f64, f64)> {
        self.length.map(|len| (self.to, self.to + len))
    }
}

/// What quantizing will do, before anything is written.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub moves: Vec<Move>,
    /// Events that were considered and deliberately left alone: no
    /// division claimed them, or they lost one to something stronger.
    ///
    /// Kept rather than dropped because "why did that not move" is the
    /// first question a user asks, and the answer is here.
    pub unmatched: Vec<f64>,
}

/// Match events to grid divisions.
///
/// With a tolerance set, **each division takes at most one event**, the
/// strongest inside its window. That one rule does most of the work: a
/// buzz roll cannot produce eight hits fighting over one division, a
/// ghost note between divisions is left alone rather than dragged onto a
/// beat it was never near, and a division with nothing near it stays
/// empty — silence is not quantized onto.
///
/// Without one, every event snaps to its own nearest division, which is
/// right for material that is not on a grid and wrong for a kit.
pub fn plan<E: Timed>(events: &[E], cfg: QuantizeConfig) -> Plan {
    if events.is_empty() || cfg.grid <= 0.0 {
        return Plan::default();
    }
    let strength = cfg.strength.clamp(0.0, 1.0);
    let division_of = |t: f64| {
        let n = ((t - cfg.grid_offset) / cfg.grid).round();
        cfg.grid_offset + n * cfg.grid
    };
    let move_to = |i: usize, e: &E, division: f64| Move {
        index: i,
        from: e.onset(),
        to: e.onset() + (division - e.onset()) * strength,
        division,
        length: e.length(),
    };

    // Below the sensitivity filter is not "unmatched by accident": the
    // user asked for these to be ignored, and they are excluded before
    // any division sees them, so a ghost note cannot win a division
    // from a hit that is merely quieter than it is strong.
    let eligible: Vec<usize> = (0..events.len())
        .filter(|&i| events[i].strength() >= cfg.min_strength)
        .collect();
    let Some((&first_i, &last_i)) = eligible.first().zip(eligible.last()) else {
        return Plan {
            moves: Vec::new(),
            unmatched: events.iter().map(|e| e.onset()).collect(),
        };
    };

    let mut moves = Vec::new();
    let mut matched = vec![false; events.len()];

    match cfg.tolerance {
        Some(tolerance) => {
            // Walk divisions, not events. Every division from the first
            // event to the last, so a division with nothing near it is
            // visited and correctly produces nothing.
            let first = events[first_i].onset();
            let last = events[last_i].onset();
            let n0 = ((first - cfg.grid_offset) / cfg.grid).floor() as i64;
            let n1 = ((last - cfg.grid_offset) / cfg.grid).ceil() as i64;

            for n in n0..=n1 {
                let division = cfg.grid_offset + n as f64 * cfg.grid;
                // The strongest event in the window, not the nearest. A
                // ghost note a hair closer to the division than the
                // backbeat must not be the one that gets quantized —
                // same reasoning as the spacing contest in the audio
                // detector's `onsets`.
                let winner = eligible
                    .iter()
                    .map(|&i| (i, &events[i]))
                    .filter(|(i, e)| !matched[*i] && (e.onset() - division).abs() <= tolerance)
                    .max_by(|a, b| {
                        a.1.strength()
                            .partial_cmp(&b.1.strength())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                if let Some((i, e)) = winner {
                    matched[i] = true;
                    moves.push(move_to(i, e, division));
                }
            }
        }
        None => {
            for &i in &eligible {
                let e = &events[i];
                matched[i] = true;
                moves.push(move_to(i, e, division_of(e.onset())));
            }
        }
    }

    moves.sort_by(|a, b| {
        a.from
            .partial_cmp(&b.from)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // Two events quantized past each other would need the take to run
    // backwards. Drop the later one rather than reorder: it is the one
    // whose division was already taken by something stronger.
    let mut ordered: Vec<Move> = Vec::with_capacity(moves.len());
    let mut dropped: Vec<f64> = Vec::new();
    for m in moves {
        if ordered.last().is_some_and(|last| m.to <= last.to) {
            dropped.push(m.from);
            continue;
        }
        ordered.push(m);
    }

    let mut unmatched: Vec<f64> = events
        .iter()
        .enumerate()
        .filter(|(i, _)| !matched[*i])
        .map(|(_, e)| e.onset())
        .collect();
    unmatched.extend(dropped);
    unmatched.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    Plan {
        moves: ordered,
        unmatched,
    }
}

/// Apply a plan to the events it was planned from.
///
/// The whole seam in three lines: no length branch, no domain check, no
/// second implementation for MIDI. A note takes its end, its curves and
/// its zone splits with it because [`Timed::move_to`] says so; a
/// transient has nothing to take.
///
/// Audio does not go through here — a transient is a measurement, and
/// moving the measurement does not move the audio. The audio domain
/// renders the same plan as a warp map or as split pieces instead.
pub fn apply<E: Timed>(events: &mut [E], plan: &Plan) {
    for m in &plan.moves {
        if let Some(event) = events.get_mut(m.index) {
            event.move_to(m.to);
        }
    }
}

/// Where the plan puts each moved event, as `(start, end)`.
///
/// What a preview draws: the ghost rectangle behind the note. Takes
/// [`Sustained`] because a rectangle needs an end — asking for this over
/// transients is a compile error rather than a row of zero-width ghosts.
pub fn spans<E: Sustained>(events: &[E], plan: &Plan) -> Vec<(f64, f64)> {
    plan.moves
        .iter()
        .filter_map(|m| {
            let e = events.get(m.index)?;
            Some((m.to, m.to + (e.end() - e.onset()).max(0.0)))
        })
        .collect()
}

/// One hit as a preview draws it: where it is, where the plan puts it,
/// and whether it moves at all.
///
/// Defined here rather than in the UI or the audio crate because both
/// need the same list — the panel draws it and the bridge computes it —
/// and two copies of "what counts as moved" is how they drift apart.
/// The unit is the events' own, exactly as everything else in this
/// module.
// r[impl drums.quantize.preview]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitPreview {
    /// Where the hit is now.
    pub at: f64,
    /// Where the plan puts it. Equal to `at` when it does not move.
    pub to: f64,
    /// Whether the plan actually moves it — a matched hit already on
    /// its division is drawn like one that stays put.
    pub moved: bool,
    /// Whether the plan deliberately left it alone: outside tolerance,
    /// lost its window to something stronger, or below the sensitivity
    /// filter. Excluded hits are dimmed, never hidden.
    pub excluded: bool,
}

/// Flatten a plan into the per-hit list a preview draws.
///
/// Sorted by current position, so a strip can draw it left to right
/// without re-sorting.
// r[impl drums.quantize.preview]
pub fn hit_previews(plan: &Plan) -> Vec<HitPreview> {
    let mut out: Vec<HitPreview> = plan
        .moves
        .iter()
        .map(|m| HitPreview {
            at: m.from,
            to: m.to,
            moved: (m.to - m.from).abs() > 1e-9,
            excluded: false,
        })
        .collect();
    out.extend(plan.unmatched.iter().map(|&at| HitPreview {
        at,
        to: at,
        moved: false,
        excluded: true,
    }));
    out.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Swing the plan's off-beat targets later.
///
/// `amount` is `0.0..=1.0`: `0` is straight, `1` moves every odd
/// division to the triplet position (two thirds of the pair span). It is
/// target placement only — the matching is unchanged, which is why this
/// is a pass over an existing plan rather than a knob on [`plan`]: the
/// planner never needs to know the grid is swung.
// r[impl drums.quantize.grid-options]
pub fn swing(plan: &mut Plan, cfg: QuantizeConfig, amount: f64) {
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 || cfg.grid <= 0.0 {
        return;
    }
    // 100 % swing puts the off-beat at 2/3 of the two-division span:
    // its target moves from grid (1/2 of the span) to 4/3 grid, a delay
    // of grid/3.
    let delay = amount * cfg.grid / 3.0;
    let strength = cfg.strength.clamp(0.0, 1.0);
    for m in &mut plan.moves {
        let n = ((m.division - cfg.grid_offset) / cfg.grid).round() as i64;
        if n.rem_euclid(2) == 1 {
            m.division += delay;
            // `to` already had strength applied toward the straight
            // division; pull it the same fraction of the extra distance.
            m.to += delay * strength;
        }
    }
}
