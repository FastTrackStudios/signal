//! Outgoing edit coalescing. A drag produces a value per pointer event
//! (often 120+/s); the engine needs ~25/s. Changes to one parameter within
//! the interval collapse to the latest; gesture begin/end are never dropped
//! and always flush, so the engine always ends on the exact final value.

use std::collections::BTreeMap;

use surface_proto::{EditPhase, ParamAddr};

#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    pub addr: ParamAddr,
    pub value: f64,
    pub phase: EditPhase,
}

#[derive(Debug, Clone)]
pub struct Coalescer {
    interval_ms: f64,
    pending: BTreeMap<ParamAddr, f64>,
    last_sent: BTreeMap<ParamAddr, f64>,
}

impl Default for Coalescer {
    fn default() -> Self {
        Self::new(40.0)
    }
}

impl Coalescer {
    #[must_use]
    pub fn new(interval_ms: f64) -> Self {
        Self { interval_ms, pending: BTreeMap::new(), last_sent: BTreeMap::new() }
    }

    /// Queue an edit at time `t` (ms); returns what to send now.
    pub fn push(&mut self, addr: ParamAddr, value: f64, phase: EditPhase, t: f64) -> Vec<Edit> {
        match phase {
            EditPhase::Begin | EditPhase::End => {
                self.pending.remove(&addr);
                self.last_sent.insert(addr.clone(), t);
                vec![Edit { addr, value, phase }]
            }
            EditPhase::Change => {
                let due = self.last_sent.get(&addr).is_none_or(|last| t - last >= self.interval_ms);
                if due {
                    self.pending.remove(&addr);
                    self.last_sent.insert(addr.clone(), t);
                    vec![Edit { addr, value, phase }]
                } else {
                    self.pending.insert(addr, value);
                    Vec::new()
                }
            }
        }
    }

    /// Send whatever has waited out the interval.
    pub fn poll(&mut self, t: f64) -> Vec<Edit> {
        let due: Vec<ParamAddr> = self
            .pending
            .keys()
            .filter(|a| self.last_sent.get(*a).is_none_or(|last| t - last >= self.interval_ms))
            .cloned()
            .collect();
        due.into_iter()
            .filter_map(|addr| {
                let value = self.pending.remove(&addr)?;
                self.last_sent.insert(addr.clone(), t);
                Some(Edit { addr, value, phase: EditPhase::Change })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_bursts_and_never_drops_the_end() {
        let a = ParamAddr::new("amp/gain");
        let mut c = Coalescer::new(40.0);
        assert_eq!(c.push(a.clone(), 0.1, EditPhase::Begin, 0.0).len(), 1);
        // 8 ms apart: all but the first within the window collapse.
        let mut sent = Vec::new();
        for i in 1..=10 {
            let t = f64::from(i) * 8.0;
            sent.extend(c.push(a.clone(), 0.1 + f64::from(i) * 0.01, EditPhase::Change, t));
            sent.extend(c.poll(t));
        }
        assert!(sent.len() <= 3, "80 ms of drag at 40 ms interval, got {}", sent.len());
        let end = c.push(a.clone(), 0.2, EditPhase::End, 81.0);
        assert_eq!(end, vec![Edit { addr: a, value: 0.2, phase: EditPhase::End }]);
        assert!(c.poll(500.0).is_empty(), "the end superseded anything pending");
    }

    #[test]
    fn independent_params_dont_block_each_other() {
        let mut c = Coalescer::new(40.0);
        assert_eq!(c.push(ParamAddr::new("a"), 0.5, EditPhase::Change, 0.0).len(), 1);
        assert_eq!(c.push(ParamAddr::new("b"), 0.5, EditPhase::Change, 1.0).len(), 1);
    }
}
