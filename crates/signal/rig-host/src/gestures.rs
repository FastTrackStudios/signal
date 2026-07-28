//! Declarative footswitch gestures — the tap / hold / edge state machine
//! every rig's MIDI pedal handling needs, extracted from the guitar rig's
//! hand-rolled pump loop.
//!
//! A [`FootswitchMap`] declares which CCs are **gesture switches** (tap on
//! short release, a hold action at the hold threshold) and which are
//! **direct** slots (fire on press). The [`FootswitchEngine`] holds the
//! per-switch state (edge detection for momentary switches that repeat while
//! held, press timestamps, hold-fired latches); the backend feeds it drained
//! CCs + a per-tick [`poll_holds`](FootswitchEngine::poll_holds) and maps the
//! returned [`FootswitchAction`]s onto its own service calls.

use std::time::{Duration, Instant};

/// Which CCs mean what — the rig's `midi.styx` projection.
#[derive(Clone, Debug, Default)]
pub struct FootswitchMap {
    /// Gesture switches, in switch order: `tap_ccs[i]` is switch `i`.
    pub tap_ccs: Vec<u32>,
    /// Direct slots: `(cc, slot)` — pressing `cc` fires `Direct(slot)`.
    pub direct: Vec<(u32, u32)>,
}

/// An action produced by the state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FootswitchAction {
    /// Gesture switch `i` released before the hold threshold.
    Tap(usize),
    /// Gesture switch `i` crossed the hold threshold (fires once per press).
    Hold(usize),
    /// Direct slot pressed.
    Direct(u32),
}

/// Per-switch gesture state. One per backend pump (thread-local scratch).
#[derive(Debug)]
pub struct FootswitchEngine {
    hold: Duration,
    /// Press timestamp per gesture switch (None = up).
    down: Vec<Option<Instant>>,
    /// Whether this press already fired its hold action.
    hold_fired: Vec<bool>,
    /// Last seen down-state per mapped control (gesture switches then direct
    /// slots) — edge detection, momentary switches repeat while held.
    cc_down: Vec<bool>,
    switches: usize,
}

impl FootswitchEngine {
    /// `switches` gesture switches + up to `directs` direct slots, firing
    /// holds at `hold` (the pedalboard convention is 500 ms).
    pub fn new(switches: usize, directs: usize, hold: Duration) -> Self {
        Self {
            hold,
            down: vec![None; switches],
            hold_fired: vec![false; switches],
            cc_down: vec![false; switches + directs],
            switches,
        }
    }

    /// Feed one drained CC. Returns the action it completes, if any (a
    /// gesture press returns nothing — its Tap/Hold comes on release or via
    /// [`poll_holds`](Self::poll_holds)).
    pub fn on_cc(&mut self, map: &FootswitchMap, cc: u8, value: u8) -> Option<FootswitchAction> {
        let gesture = map.tap_ccs.iter().position(|c| *c == cc as u32);
        let direct = map
            .direct
            .iter()
            .find(|(dc, _)| *dc == cc as u32)
            .map(|(_, slot)| *slot);
        let idx = gesture.or_else(|| direct.map(|s| self.switches + s as usize))?;
        let idx = idx.min(self.cc_down.len().saturating_sub(1));
        let down = value > 0;
        if down == self.cc_down[idx] {
            return None; // momentary repeat — not an edge
        }
        self.cc_down[idx] = down;
        match gesture {
            Some(sw) if sw < self.switches => {
                if down {
                    self.down[sw] = Some(Instant::now());
                    self.hold_fired[sw] = false;
                    None
                } else {
                    let tapped = !self.hold_fired[sw];
                    self.down[sw] = None;
                    tapped.then_some(FootswitchAction::Tap(sw))
                }
            }
            _ => down.then_some(FootswitchAction::Direct(direct?)),
        }
    }

    /// Fire due holds — call once per pump tick. Each returned `Hold(i)`
    /// fires at most once per press.
    pub fn poll_holds(&mut self) -> Vec<FootswitchAction> {
        let mut fired = Vec::new();
        for sw in 0..self.switches {
            if let Some(t) = self.down[sw] {
                if !self.hold_fired[sw] && t.elapsed() >= self.hold {
                    self.hold_fired[sw] = true;
                    fired.push(FootswitchAction::Hold(sw));
                }
            }
        }
        fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> FootswitchMap {
        FootswitchMap {
            tap_ccs: vec![101, 102, 103, 104, 105],
            direct: (0..5).map(|i| (106 + i, i)).collect(),
        }
    }

    fn engine() -> FootswitchEngine {
        FootswitchEngine::new(5, 5, Duration::from_millis(500))
    }

    #[test]
    fn short_press_taps_on_release() {
        let (m, mut e) = (map(), engine());
        assert_eq!(e.on_cc(&m, 101, 127), None);
        assert_eq!(e.on_cc(&m, 101, 0), Some(FootswitchAction::Tap(0)));
    }

    #[test]
    fn momentary_repeats_are_not_edges() {
        let (m, mut e) = (map(), engine());
        assert_eq!(e.on_cc(&m, 102, 127), None);
        // Held switch repeating its down value: no edge, no action.
        assert_eq!(e.on_cc(&m, 102, 127), None);
        assert_eq!(e.on_cc(&m, 102, 0), Some(FootswitchAction::Tap(1)));
        // Repeated release is not an edge either.
        assert_eq!(e.on_cc(&m, 102, 0), None);
    }

    #[test]
    fn hold_fires_once_and_suppresses_the_tap() {
        let m = map();
        let mut e = FootswitchEngine::new(5, 5, Duration::from_millis(0));
        assert_eq!(e.on_cc(&m, 103, 127), None);
        assert_eq!(e.poll_holds(), vec![FootswitchAction::Hold(2)]);
        // Only once per press.
        assert_eq!(e.poll_holds(), Vec::new());
        // The release after a fired hold is not a tap.
        assert_eq!(e.on_cc(&m, 103, 0), None);
    }

    #[test]
    fn direct_slots_fire_on_press_only() {
        let (m, mut e) = (map(), engine());
        assert_eq!(e.on_cc(&m, 108, 127), Some(FootswitchAction::Direct(2)));
        assert_eq!(e.on_cc(&m, 108, 0), None);
    }

    #[test]
    fn unmapped_ccs_are_ignored() {
        let (m, mut e) = (map(), engine());
        assert_eq!(e.on_cc(&m, 64, 127), None);
    }
}
