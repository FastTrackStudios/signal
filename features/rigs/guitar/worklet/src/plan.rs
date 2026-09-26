//! Where each NAM model of a chain runs — the render thread, or a worker —
//! and at what latency.
//!
//! One A2 model costs about half a 128-frame quantum in wasm, and a chain's
//! models are mostly in series (boost → drives → amp): each needs the one
//! before it, so they cannot run side by side within a quantum. The plan:
//!
//! 1. Models run **inline**, on the render thread, while the chain still fits
//!    the render budget. No latency.
//! 2. **Amp R** runs on a worker *at the same time as Amp L* — both hear the
//!    dry guitar, so its input is known the moment Amp L starts. No latency,
//!    and it costs the render thread only the wait beyond Amp L.
//! 3. A serial model that does not fit runs on a worker **one quantum
//!    behind** ([`Lag::One`]): it gets a whole quantum to itself, for 2.7 ms
//!    at 48 kHz. Each such model adds one quantum.
//!
//! Costs are estimates to begin with and measurements once the models have
//! run; a host re-plans when a worker starts missing.

use crate::remote::Lag;

/// Where one model runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Inline,
    /// On a worker, at this lag. `Lag::Zero` is only planned for Amp R.
    Worker(Lag),
}

/// One NAM model in chain order, as the planner sees it.
#[derive(Clone, Debug)]
pub struct Model {
    /// Its slot in the chain.
    pub slot: usize,
    /// Microseconds per quantum.
    pub cost_us: u32,
    /// Amp R of a dual-amp stage — runs beside Amp L, not after it.
    pub parallel: bool,
    /// Amp L of a dual-amp stage. It is summed with Amp R, so both must be
    /// at the same lag — zero — or the blend combs.
    pub pinned: bool,
    /// Playing now (not bypassed). A bypassed model costs nothing until it
    /// is switched on, so it waits on a worker.
    pub active: bool,
}

/// A chain's placements, parallel to the `models` it was planned from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The chain slot of each placed model.
    pub slots: Vec<usize>,
    pub places: Vec<Place>,
    /// Added latency as planned — the lagged models that are playing — in
    /// quanta. Switching a lagged model on or off moves it by one.
    pub lag_quanta: u32,
}

/// What the render thread may spend on models per quantum, in µs.
///
/// A quantum is `quantum_frames / sample_rate`; the browser keeps some of it
/// for itself, and the rest of the chain (EQ, delay, reverb) needs its share,
/// so the models get `share` of it (0.55 is the default the host uses).
#[must_use]
pub fn model_budget_us(quantum_frames: u32, sample_rate: f64, share: f64) -> u32 {
    (f64::from(quantum_frames) / sample_rate * 1e6 * share) as u32
}

/// Plan `models` against `budget_us` (see [`model_budget_us`]).
///
/// The dual-amp stage is placed first and never lags: Amp L inline, Amp R
/// on a zero-lag worker beside it. Then the serial models (boost, drives, a
/// single amp) that are *playing* go inline in chain order while the budget
/// lasts; then the bypassed ones, if room is left; everything else waits on
/// a lag-one worker.
#[must_use]
pub fn plan(models: &[Model], budget_us: u32) -> Plan {
    let mut spent: u32 = models.iter().filter(|m| m.pinned).map(|m| m.cost_us).sum();
    let mut places: Vec<Option<Place>> = models
        .iter()
        .map(|m| {
            if m.parallel {
                Some(Place::Worker(Lag::Zero))
            } else if m.pinned {
                Some(Place::Inline)
            } else {
                None
            }
        })
        .collect();
    for active in [true, false] {
        for (m, place) in models.iter().zip(places.iter_mut()) {
            if place.is_some() || m.active != active {
                continue;
            }
            if spent + m.cost_us <= budget_us {
                spent += m.cost_us;
                *place = Some(Place::Inline);
            }
        }
    }
    let places: Vec<Place> = places
        .into_iter()
        .map(|p| p.unwrap_or(Place::Worker(Lag::One)))
        .collect();
    let lag_quanta = models
        .iter()
        .zip(&places)
        .filter(|(m, p)| m.active && **p == Place::Worker(Lag::One))
        .count() as u32;
    Plan {
        slots: models.iter().map(|m| m.slot).collect(),
        places,
        lag_quanta,
    }
}

impl Plan {
    /// Where the block in chain slot `slot` runs (a non-model: inline).
    #[must_use]
    pub fn place(&self, slot: usize) -> Place {
        self.slots
            .iter()
            .position(|&s| s == slot)
            .map_or(Place::Inline, |i| self.places[i])
    }

    /// The slots that run on a worker, with their lag.
    pub fn remote(&self) -> impl Iterator<Item = (usize, Lag)> + '_ {
        self.slots
            .iter()
            .zip(&self.places)
            .filter_map(|(&s, p)| match p {
                Place::Worker(lag) => Some((s, *lag)),
                Place::Inline => None,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serial(costs: &[u32]) -> Vec<Model> {
        costs
            .iter()
            .enumerate()
            .map(|(slot, &cost_us)| Model {
                slot,
                cost_us,
                parallel: false,
                pinned: false,
                active: true,
            })
            .collect()
    }

    /// 128 frames at 48 kHz is 2667 µs; 55% of that is ~1466 µs.
    #[test]
    fn the_budget_is_a_share_of_the_quantum() {
        assert_eq!(model_budget_us(128, 48_000.0, 0.55), 1466);
    }

    /// One full-size amp fits: nothing moves, nothing is late.
    #[test]
    fn a_lone_amp_stays_inline() {
        let p = plan(&serial(&[1300]), 1466);
        assert_eq!(p.places, vec![Place::Inline]);
        assert_eq!(p.lag_quanta, 0);
    }

    /// Boost → three drives → amp at full size: one fits, four lag.
    #[test]
    fn a_full_board_lags_what_does_not_fit() {
        let p = plan(&serial(&[1300; 5]), 1466);
        assert_eq!(p.places[0], Place::Inline);
        assert!(p.places[1..].iter().all(|&x| x == Place::Worker(Lag::One)));
        assert_eq!(p.lag_quanta, 4);
    }

    /// Slimmed models (≈650 µs) pack two to the render thread.
    #[test]
    fn slim_models_pack_inline() {
        let p = plan(&serial(&[650, 650, 650]), 1466);
        assert_eq!(
            p.places,
            vec![Place::Inline, Place::Inline, Place::Worker(Lag::One)]
        );
        assert_eq!(p.lag_quanta, 1);
    }

    /// Amp R runs beside Amp L on a worker and adds nothing.
    #[test]
    fn amp_r_runs_in_parallel_for_free() {
        let mut models = serial(&[1300, 1300]);
        models[0].pinned = true;
        models[1].parallel = true;
        let p = plan(&models, 1466);
        assert_eq!(p.places, vec![Place::Inline, Place::Worker(Lag::Zero)]);
        assert_eq!(p.lag_quanta, 0);
    }

    /// With two amps, the amps keep the render thread and a drive ahead of
    /// them lags — never Amp L, which would comb against Amp R.
    #[test]
    fn the_amp_pair_is_never_split_by_a_lag() {
        let mut models = serial(&[1300, 1300, 1300]);
        models[1].pinned = true;
        models[2].parallel = true;
        let p = plan(&models, 1466);
        assert_eq!(
            p.places,
            vec![
                Place::Worker(Lag::One),
                Place::Inline,
                Place::Worker(Lag::Zero)
            ]
        );
        assert_eq!(p.lag_quanta, 1);
    }

    /// The shipped rig's shape: two drives bypassed ahead of the amp. The
    /// amp gets the render thread and the bypassed drives wait on workers —
    /// adding nothing until one is switched on.
    #[test]
    fn bypassed_models_wait_on_workers_for_free() {
        let mut models = serial(&[1250, 1250, 1250]);
        models[0].active = false;
        models[1].active = false;
        let p = plan(&models, 1466);
        assert_eq!(
            p.places,
            vec![
                Place::Worker(Lag::One),
                Place::Worker(Lag::One),
                Place::Inline
            ]
        );
        assert_eq!(p.lag_quanta, 0);
    }

    /// Room left after what is playing goes to a bypassed model, so
    /// switching it on costs no latency.
    #[test]
    fn spare_budget_goes_to_a_bypassed_model() {
        let mut models = serial(&[600, 600]);
        models[0].active = false;
        let p = plan(&models, 1466);
        assert_eq!(p.places, vec![Place::Inline, Place::Inline]);
    }
}
