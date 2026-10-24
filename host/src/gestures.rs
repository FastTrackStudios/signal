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
    /// Momentary switch `i` went down (see [`FootswitchEngine::set_momentary`]).
    Press(usize),
    /// Momentary switch `i` came back up.
    Release(usize),
    /// Switch `i` held past the long-hold threshold (see
    /// [`FootswitchEngine::set_long_hold`]).
    LongHold(usize),
    /// Two switches pressed together and held (see
    /// [`FootswitchEngine::add_chord`]) — `(lower, higher)` switch index.
    Chord(usize, usize),
}

/// Two switches pressed within this of each other are pressed *together*.
const CHORD_WINDOW: Duration = Duration::from_millis(150);
/// …and held together this long, a chord.
const CHORD_HOLD: Duration = Duration::from_millis(300);

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
    /// Switches that act only while held: `Press` down, `Release` up — no
    /// tap, no hold. Latched from `momentary_want` at each press, so a
    /// press always ends the way it began.
    momentary: Vec<bool>,
    momentary_want: Vec<bool>,
    /// Switches with a second, longer hold: their `Hold` waits for the
    /// release (so holding on can still become a `LongHold` without the
    /// `Hold` having fired), and `LongHold` fires at `long_hold`.
    long: Vec<bool>,
    long_hold: Duration,
    /// The switch pairs that make chords.
    chords: Vec<(usize, usize)>,
    /// Switches taken by a chord this press: no tap, hold or long hold of
    /// their own.
    chorded: Vec<bool>,
    /// Chords already fired this press.
    chord_fired: Vec<(usize, usize)>,
    /// Actions produced outside a press/release (a momentary taken back
    /// when its switch joins a chord), handed out by `poll_holds`.
    pending: Vec<FootswitchAction>,
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
            momentary: vec![false; switches],
            momentary_want: vec![false; switches],
            long: vec![false; switches],
            long_hold: Duration::from_secs(2),
            chords: Vec::new(),
            chorded: vec![false; switches],
            chord_fired: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// Switches `a` and `b` pressed together and held make a
    /// [`FootswitchAction::Chord`] — and neither does its own tap or hold.
    pub fn add_chord(&mut self, a: usize, b: usize) {
        let pair = (a.min(b), a.max(b));
        if !self.chords.contains(&pair) {
            self.chords.push(pair);
        }
    }

    /// Give switch `sw` a long hold at `after`: a release between the hold
    /// and the long hold is its `Hold`, holding on to `after` is its
    /// `LongHold` (and nothing else).
    pub fn set_long_hold(&mut self, sw: usize, after: Duration) {
        if let Some(slot) = self.long.get_mut(sw) {
            *slot = true;
        }
        self.long_hold = after;
    }

    /// Which gesture switches have a long hold, in switch order (missing =
    /// not) — the per-switch form of [`set_long_hold`](Self::set_long_hold),
    /// for switches whose job changes (switch 5 stepping through a song's
    /// parts keeps its tuner on the long hold).
    pub fn set_long_holds(&mut self, flags: &[bool]) {
        for (sw, slot) in self.long.iter_mut().enumerate() {
            *slot = flags.get(sw).copied().unwrap_or(false);
        }
    }

    /// Which gesture switches are momentary, in switch order (missing =
    /// not). A switch changed while it is held finishes that press the way
    /// it started.
    pub fn set_momentary(&mut self, flags: &[bool]) {
        for (sw, slot) in self.momentary_want.iter_mut().enumerate() {
            *slot = flags.get(sw).copied().unwrap_or(false);
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
    pub fn on_note(
        &mut self,
        map: &FootswitchMap,
        note: u8,
        down: bool,
    ) -> Option<FootswitchAction> {
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
        if down {
            if let Some(sw) = gesture.filter(|&sw| sw < self.switches) {
                self.momentary[sw] = self.momentary_want[sw];
                // Its chord partner went down just before: pressed together.
                for &(a, b) in &self.chords {
                    let partner = if a == sw {
                        b
                    } else if b == sw {
                        a
                    } else {
                        continue;
                    };
                    let with = self.down.get(partner).copied().flatten();
                    if with.is_some_and(|t| t.elapsed() <= CHORD_WINDOW) {
                        for x in [sw, partner] {
                            self.chorded[x] = true;
                            self.hold_fired[x] = true;
                        }
                        // A momentary already down gives its press back.
                        if self.momentary[partner] {
                            self.pending.push(FootswitchAction::Release(partner));
                        }
                    }
                }
                if self.chorded[sw] {
                    self.down[sw] = Some(Instant::now());
                    return None;
                }
            }
        } else if let Some(sw) = gesture.filter(|&sw| sw < self.switches && self.chorded[sw]) {
            // A chord's switch coming up does nothing of its own; the chord
            // is over when both are.
            self.down[sw] = None;
            self.chorded[sw] = false;
            self.chord_fired.retain(|&(a, b)| a != sw && b != sw);
            return None;
        }
        match gesture {
            Some(sw) if sw < self.switches && self.momentary[sw] => {
                // Held-only: the press is the action, the release undoes it.
                if down {
                    self.down[sw] = Some(Instant::now());
                    self.hold_fired[sw] = true; // no hold for a momentary
                    Some(FootswitchAction::Press(sw))
                } else {
                    self.down[sw] = None;
                    Some(FootswitchAction::Release(sw))
                }
            }
            Some(sw) if sw < self.switches => {
                if down {
                    self.down[sw] = Some(Instant::now());
                    self.hold_fired[sw] = false;
                    None
                } else {
                    let held_for = self.down[sw].map(|t| t.elapsed());
                    let tapped = !self.hold_fired[sw];
                    self.down[sw] = None;
                    if tapped && self.long[sw] && held_for.is_some_and(|d| d >= self.hold) {
                        // Released between the hold and the long hold.
                        return Some(FootswitchAction::Hold(sw));
                    }
                    tapped.then_some(FootswitchAction::Tap(sw))
                }
            }
            _ => down.then_some(FootswitchAction::Direct(direct?)),
        }
    }

    /// Fire due holds — call once per pump tick. Each returned `Hold(i)`
    /// fires at most once per press.
    pub fn poll_holds(&mut self) -> Vec<FootswitchAction> {
        let mut fired = std::mem::take(&mut self.pending);
        for &(a, b) in &self.chords {
            let (Some(ta), Some(tb)) = (self.down[a], self.down[b]) else {
                continue;
            };
            if self.chorded[a]
                && self.chorded[b]
                && !self.chord_fired.contains(&(a, b))
                && ta.elapsed().min(tb.elapsed()) >= CHORD_HOLD
            {
                self.chord_fired.push((a, b));
                fired.push(FootswitchAction::Chord(a, b));
            }
        }
        for sw in 0..self.switches {
            if let Some(t) = self.down[sw] {
                if self.long[sw] {
                    // Its hold comes on release; only the long hold fires here.
                    if !self.hold_fired[sw] && t.elapsed() >= self.long_hold {
                        self.hold_fired[sw] = true;
                        fired.push(FootswitchAction::LongHold(sw));
                    }
                } else if !self.hold_fired[sw] && t.elapsed() >= self.hold {
                    self.hold_fired[sw] = true;
                    fired.push(FootswitchAction::Hold(sw));
                }
            }
        }
        fired
    }
}
