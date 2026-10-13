//! Turning pointer events into things that happened.
//!
//! A window gets presses, moves and releases. A mixer needs clicks,
//! double-clicks and drags — and the difference between them is timing
//! and distance, not anything the window can tell you.
//!
//! # Why a press is not a click
//!
//! The obvious implementation acts on press. It is wrong in three ways
//! that all show up the first time somebody uses it: a press that turns
//! into a drag has already toggled the mute it started on; a
//! double-click has toggled it twice on the way to being a
//! double-click; and a press you drag away from and release elsewhere
//! still counts, when every other application treats that as "I changed
//! my mind".
//!
//! So a click is decided on RELEASE, by what happened in between.
//!
//! # Fine adjustment
//!
//! A fader is a few hundred pixels and its range is sixty decibels, so
//! one pixel is a fifth of a dB — fine enough to mix with and far too
//! coarse to match a level by ear. Holding the fine modifier scales the
//! movement down rather than changing what is being dragged, which is
//! REAPER's behaviour and everyone else's.

use std::time::Duration;
use web_time::Instant;

use crate::hit::Hit;

/// How far the pointer may move and still be a click rather than a drag.
///
/// Three pixels: a mouse moves one or two on a firm click and a trackpad
/// moves more, and a control that needed a perfectly still press would
/// feel broken on a laptop.
pub const SLOP: f64 = 3.0;

/// How long between two clicks for the second to be a double.
pub const DOUBLE: Duration = Duration::from_millis(400);

/// How much a drag is scaled by while the fine modifier is held.
///
/// A quarter. Enough that a whole fader's travel becomes four screens
/// of movement — which sounds absurd until you are matching a level by
/// ear, where the useful range is about two dB.
pub const FINE: f64 = 0.25;

/// What the pointer did.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Event {
    Click(Hit),
    DoubleClick(Hit),
    /// A drag in progress. `delta` is since the last report, already
    /// scaled for fine adjustment.
    Drag {
        hit: Hit,
        delta: (f64, f64),
    },
    /// The drag ended. Sent so a control can commit, or push one undo
    /// step for the whole gesture rather than one per pixel.
    DragEnd(Hit),
}

/// The pointer's state between events.
#[derive(Clone, Copy, Debug, Default)]
pub struct Gestures {
    press: Option<Press>,
    last_click: Option<(Instant, (f64, f64))>,
}

#[derive(Clone, Copy, Debug)]
struct Press {
    hit: Hit,
    at: (f64, f64),
    /// Where the last drag delta was measured from.
    last: (f64, f64),
    dragging: bool,
}

impl Gestures {
    /// The pointer went down on `hit`.
    pub fn press(&mut self, hit: Hit, x: f64, y: f64) {
        self.press = Some(Press {
            hit,
            at: (x, y),
            last: (x, y),
            dragging: false,
        });
    }

    /// The pointer moved. Returns a drag once it has moved far enough
    /// to be one.
    pub fn moved(&mut self, x: f64, y: f64, fine: bool) -> Option<Event> {
        let press = self.press.as_mut()?;
        let far = (x - press.at.0).abs() > SLOP || (y - press.at.1).abs() > SLOP;
        if !press.dragging && !far {
            return None;
        }
        // The first drag measures from where the press STARTED, not
        // from the slop threshold — otherwise the control jumps by the
        // slop distance the moment a drag begins.
        press.dragging = true;
        let scale = if fine { FINE } else { 1.0 };
        let delta = ((x - press.last.0) * scale, (y - press.last.1) * scale);
        press.last = (x, y);
        Some(Event::Drag {
            hit: press.hit,
            delta,
        })
    }

    /// The pointer came up. Returns the click, if it was one.
    pub fn release(&mut self, x: f64, y: f64, now: Instant) -> Option<Event> {
        let press = self.press.take()?;
        if press.dragging {
            return Some(Event::DragEnd(press.hit));
        }
        // Released somewhere else entirely: not a click on anything.
        // Every other application treats leaving the control as "I
        // changed my mind", and a mute that fired anyway would be the
        // one control in the mixer that could not be backed out of.
        if (x - press.at.0).abs() > SLOP || (y - press.at.1).abs() > SLOP {
            return None;
        }

        let double = self.last_click.is_some_and(|(when, at)| {
            now.saturating_duration_since(when) <= DOUBLE
                && (x - at.0).abs() <= SLOP
                && (y - at.1).abs() <= SLOP
        });
        if double {
            // Consumed, so three clicks are a double and a single
            // rather than two doubles.
            self.last_click = None;
            return Some(Event::DoubleClick(press.hit));
        }
        self.last_click = Some((now, (x, y)));
        Some(Event::Click(press.hit))
    }

    /// Whether a drag is in progress — for a cursor shape, or to keep
    /// dragging a fader whose pointer has left the strip.
    #[must_use]
    pub const fn dragging(&self) -> bool {
        matches!(self.press, Some(Press { dragging: true, .. }))
    }

    /// Abandon whatever was in progress. For focus loss, which is not a
    /// release and must not commit anything.
    pub fn cancel(&mut self) {
        self.press = None;
    }
}

/// How far a vertical drag moves a control, as a fraction of its range.
///
/// `travel` is what the control has on screen: a fader's groove, or the
/// notional travel a knob is worth. Dividing by it means a control
/// twice as tall takes twice the movement, which is what makes a big
/// fader feel precise and a small one feel quick.
///
/// UP is positive, because down is negative in screen coordinates and
/// every control in a mixer goes up to mean more.
#[must_use]
pub fn drag_fraction(delta_y: f64, travel: f64) -> f64 {
    if travel <= 0.0 {
        return 0.0;
    }
    -delta_y / travel
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hit::{Hit, Target};
    use input_config_proto::MouseModifierContext as Context;

    fn hit(row: usize) -> Hit {
        Hit {
            target: Target::Track { row },
            context: Context::MixerStrip,
        }
    }

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    /// A press is not a click. The click happens on release, so a
    /// control cannot fire and then be dragged away from.
    #[test]
    fn a_click_is_decided_on_release() {
        let now = Instant::now();
        let mut g = Gestures::default();
        g.press(hit(3), 100.0, 100.0);
        assert!(
            g.moved(101.0, 100.0, false).is_none(),
            "that was not a drag"
        );
        assert_eq!(g.release(101.0, 100.0, now), Some(Event::Click(hit(3))));
    }

    /// Released away from where it started: nothing happened.
    #[test]
    fn dragging_off_a_control_cancels_it() {
        let now = Instant::now();
        let mut g = Gestures::default();
        g.press(hit(1), 100.0, 100.0);
        // Far enough to be a drag, so this is a drag that ended.
        assert!(g.moved(140.0, 100.0, false).is_some());
        assert_eq!(g.release(140.0, 100.0, now), Some(Event::DragEnd(hit(1))));

        // And a press released elsewhere WITHOUT ever becoming a drag
        // is nothing at all.
        let mut g = Gestures::default();
        g.press(hit(1), 100.0, 100.0);
        g.press(hit(1), 100.0, 100.0);
        assert_eq!(g.release(200.0, 100.0, now), None);
    }

    /// Two clicks in the same place, quickly, are a double — and the
    /// double is not also reported as a second single.
    #[test]
    fn two_quick_clicks_are_a_double() {
        let base = Instant::now();
        let mut g = Gestures::default();
        g.press(hit(2), 50.0, 50.0);
        assert_eq!(g.release(50.0, 50.0, base), Some(Event::Click(hit(2))));
        g.press(hit(2), 50.0, 50.0);
        assert_eq!(
            g.release(50.0, 50.0, at(base, 120)),
            Some(Event::DoubleClick(hit(2)))
        );
    }

    /// Too slow, or too far apart, and they are two singles.
    #[test]
    fn a_slow_or_distant_second_click_is_its_own_click() {
        let base = Instant::now();
        let mut g = Gestures::default();
        g.press(hit(2), 50.0, 50.0);
        g.release(50.0, 50.0, base);
        g.press(hit(2), 50.0, 50.0);
        assert_eq!(
            g.release(50.0, 50.0, at(base, 900)),
            Some(Event::Click(hit(2))),
            "400ms apart should not double"
        );

        let mut g = Gestures::default();
        g.press(hit(2), 50.0, 50.0);
        g.release(50.0, 50.0, base);
        g.press(hit(2), 400.0, 50.0);
        assert_eq!(
            g.release(400.0, 50.0, at(base, 100)),
            Some(Event::Click(hit(2))),
            "a click across the window should not double"
        );
    }

    /// Three clicks are a double and then a single, not two doubles —
    /// otherwise a rapid triple-click renames a track twice.
    #[test]
    fn a_third_click_starts_over() {
        let base = Instant::now();
        let mut g = Gestures::default();
        for (i, expected) in [
            Event::Click(hit(0)),
            Event::DoubleClick(hit(0)),
            Event::Click(hit(0)),
        ]
        .into_iter()
        .enumerate()
        {
            g.press(hit(0), 10.0, 10.0);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::as_conversions,
                reason = "a small test index"
            )]
            let ms = (i as u64) * 100;
            assert_eq!(g.release(10.0, 10.0, at(base, ms)), Some(expected));
        }
    }

    /// The first drag delta is measured from the PRESS, not from where
    /// the slop was crossed — otherwise a fader jumps by the slop
    /// distance the instant it starts moving.
    #[test]
    fn a_drag_does_not_jump_when_it_starts() {
        let mut g = Gestures::default();
        g.press(hit(0), 0.0, 100.0);
        let Some(Event::Drag { delta, .. }) = g.moved(0.0, 90.0, false) else {
            panic!("expected a drag");
        };
        assert!((delta.1 + 10.0).abs() < f64::EPSILON, "delta was {delta:?}");
    }

    /// Fine adjustment scales the movement, it does not change what is
    /// being moved.
    #[test]
    fn the_fine_modifier_scales_the_movement() {
        let mut g = Gestures::default();
        g.press(hit(0), 0.0, 100.0);
        let Some(Event::Drag { delta, .. }) = g.moved(0.0, 60.0, true) else {
            panic!("expected a drag");
        };
        assert!((delta.1 + 40.0 * FINE).abs() < 1e-9, "delta was {delta:?}");
    }

    /// Up is more. Down is negative in screen coordinates and every
    /// control in a mixer goes up to mean more, so the sign flips here
    /// once rather than at every call site.
    #[test]
    fn dragging_up_increases() {
        assert!(drag_fraction(-10.0, 100.0) > 0.0);
        assert!(drag_fraction(10.0, 100.0) < 0.0);
        assert!((drag_fraction(-50.0, 100.0) - 0.5).abs() < f64::EPSILON);
        // A control with no travel cannot be dragged, rather than
        // dividing by zero.
        assert!(drag_fraction(-10.0, 0.0).abs() < f64::EPSILON);
    }

    /// Losing focus mid-drag abandons it rather than committing it.
    #[test]
    fn cancelling_commits_nothing() {
        let now = Instant::now();
        let mut g = Gestures::default();
        g.press(hit(0), 0.0, 0.0);
        g.moved(0.0, 50.0, false);
        assert!(g.dragging());
        g.cancel();
        assert!(!g.dragging());
        assert_eq!(g.release(0.0, 50.0, now), None);
    }
}
