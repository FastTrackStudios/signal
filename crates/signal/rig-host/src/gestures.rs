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

/// Which CCs and notes mean what — the rig's `midi.styx` projection.
#[derive(Clone, Debug, Default)]
pub struct FootswitchMap {
    /// Gesture switches, in switch order: `tap_ccs[i]` is switch `i`.
    pub tap_ccs: Vec<u32>,
    /// The same switches as notes: `tap_notes[i]` is switch `i` — Note On
    /// presses it, Note Off (or Note On at velocity 0) releases it. For
    /// pedals that send a note per switch (an AIRSTEP set to Note On on
    /// press, Note Off on release); tap and hold come from the timing, the
    /// same as for CCs.
    pub tap_notes: Vec<u32>,
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
    #[must_use]
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
        let gesture = map.tap_ccs.iter().position(|c| *c == u32::from(cc));
        let direct = map
            .direct
            .iter()
            .find(|(dc, _)| *dc == u32::from(cc))
            .map(|(_, slot)| *slot);
        self.edge(gesture, direct, value > 0)
    }

    /// Feed one note: `down` for Note On, `false` for Note Off (a Note On at
    /// velocity 0 is a Note Off — the caller folds that in). Same gestures as
    /// [`on_cc`](Self::on_cc): tap on a short release, hold at the threshold.
    pub fn on_note(&mut self, map: &FootswitchMap, note: u8, down: bool) -> Option<FootswitchAction> {
        let gesture = map.tap_notes.iter().position(|n| *n == u32::from(note));
        self.edge(gesture, None, down)
    }

    /// Whether the switch `note` maps to is down — for resolving a Note On at
    /// velocity 0, which pedals send both as a *press* (an AIRSTEP set to
    /// Note On/Note Off with velocity 0 sends `90 n 00` then `80 n 00`) and,
    /// per the MIDI spec, as a *release*. Read against the switch's own
    /// state, it is a press when up and a release when down — right for both.
    #[must_use]
    pub fn note_switch_is_down(&self, map: &FootswitchMap, note: u8) -> bool {
        map.tap_notes
            .iter()
            .position(|n| *n == u32::from(note))
            .is_some_and(|i| self.cc_down.get(i).copied().unwrap_or(false))
    }

    /// A press or release of gesture switch `gesture` / direct slot `direct`.
    fn edge(
        &mut self,
        gesture: Option<usize>,
        direct: Option<u32>,
        down: bool,
    ) -> Option<FootswitchAction> {
        let idx = gesture.or_else(|| direct.map(|s| self.switches + s as usize))?;
        let idx = idx.min(self.cc_down.len().saturating_sub(1));
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
            tap_notes: vec![1, 2, 3, 4, 5],
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
    fn a_note_pedal_taps_and_holds_like_a_cc_pedal() {
        let m = map();
        let mut e = engine();
        assert_eq!(e.on_note(&m, 2, true), None);
        assert_eq!(e.on_note(&m, 2, false), Some(FootswitchAction::Tap(1)));

        let mut e = FootswitchEngine::new(5, 5, Duration::from_millis(0));
        assert_eq!(e.on_note(&m, 5, true), None);
        assert_eq!(e.poll_holds(), vec![FootswitchAction::Hold(4)]);
        assert_eq!(e.on_note(&m, 5, false), None);
    }

    #[test]
    fn a_note_and_its_cc_are_the_same_switch() {
        // Note 1 pressed, CC 101 released: one switch, one tap — a pedal
        // mapped both ways cannot double-fire.
        let (m, mut e) = (map(), engine());
        assert_eq!(e.on_note(&m, 1, true), None);
        assert_eq!(e.on_cc(&m, 101, 0), Some(FootswitchAction::Tap(0)));
    }

    #[test]
    fn a_velocity_zero_note_on_presses_when_up_and_releases_when_down() {
        // AIRSTEP with velocity 0: `90 02 00` then `80 02 00`.
        let (m, mut e) = (map(), engine());
        let down = !e.note_switch_is_down(&m, 2);
        assert!(down, "an up switch takes a velocity-0 Note On as a press");
        assert_eq!(e.on_note(&m, 2, down), None);
        assert_eq!(e.on_note(&m, 2, false), Some(FootswitchAction::Tap(1)));
        // And the spec's convention — velocity 0 *is* the release.
        assert_eq!(e.on_note(&m, 3, true), None);
        let down = !e.note_switch_is_down(&m, 3);
        assert!(!down, "a down switch takes it as the release");
        assert_eq!(e.on_note(&m, 3, down), Some(FootswitchAction::Tap(2)));
    }

    #[test]
    fn unmapped_notes_are_ignored() {
        let (m, mut e) = (map(), engine());
        assert_eq!(e.on_note(&m, 60, true), None);
    }

    #[test]
    fn unmapped_ccs_are_ignored() {
        let (m, mut e) = (map(), engine());
        assert_eq!(e.on_cc(&m, 64, 127), None);
    }
}
