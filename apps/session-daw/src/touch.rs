//! Touch mode: controls sized for a finger, and gestures made for one.
//!
//! A mouse can hit a 21-pixel mute button and drag a fader from anywhere
//! on its groove without anyone noticing. A finger covers the button and
//! its neighbours, and a finger landing on a groove is far more often the
//! start of a scroll than a decision to move the level. So touch mode is
//! not the same controls made bigger: it is bigger controls, a fader that
//! only moves when its cap is taken, and empty space that scrolls.
//!
//! On by default where the screen is the pointer (iOS, Android, a browser
//! whose primary pointer is coarse), and a switch everywhere, since a
//! touchscreen laptop is both.

use dioxus::prelude::*;

/// How much bigger everything is in touch mode. REAPER's mute is 21 by
/// 20; at this it is 37 by 35, the far side of the smallest target a
/// fingertip hits without looking.
pub const ZOOM: f64 = 1.75;

/// How much bigger the arrangement is drawn in touch mode: its track
/// panel's 21-pixel buttons to 28, its ruler's 15-pixel lanes to 20, its
/// nine-point names to twelve. Less than the mixer's [`ZOOM`]: the
/// arrangement is mostly lanes, and every pixel of chrome it gains is one
/// the music loses.
pub const ARRANGE_ZOOM: f64 = 1.35;

/// How much bigger to draw the arrangement, as touch mode is, in a panel
/// `width` CSS pixels wide (0 while it is not yet measured).
///
/// A phone held upright is some four hundred wide, and the full zoom would
/// give its track panel more of that than its lanes; there the zoom eases
/// down, reaching the full [`ARRANGE_ZOOM`] at a tablet's width.
#[must_use]
pub fn arrange_zoom(touch: bool, width: f64) -> f64 {
    if !touch {
        return 1.0;
    }
    if width <= 0.0 {
        return ARRANGE_ZOOM;
    }
    let narrow = 1.1;
    let t = ((width - 400.0) / (700.0 - 400.0)).clamp(0.0, 1.0);
    narrow + (ARRANGE_ZOOM - narrow) * t
}

/// How much bigger to draw the mixer's strips, as touch mode is, in a
/// panel `width` CSS pixels wide: whatever puts [`STRIPS_ACROSS`] default
/// strips across it — ten on an 11-inch iPad on its side, at about 1.36 —
/// but never below [`MIXER_ZOOM_MIN`], where a finger stops hitting what
/// it aims at (so an upright tablet or a phone scrolls instead), nor
/// above [`ZOOM`].
#[must_use]
pub fn mixer_zoom(touch: bool, width: f64) -> f64 {
    if !touch {
        return 1.0;
    }
    let across = STRIPS_ACROSS * (crate::mcp::STRIP_W + crate::mcp::STRIP_GAP);
    (width / across).clamp(MIXER_ZOOM_MIN, ZOOM)
}

/// How many default strips a touchscreen's mixer aims to show across.
pub const STRIPS_ACROSS: f64 = 10.0;
/// The least a touchscreen's strips are zoomed: its buttons still a
/// fingertip.
pub const MIXER_ZOOM_MIN: f64 = 1.35;

/// A scroll still moving after the finger let go: the speed it left at,
/// dying away as a thrown list does on a phone.
#[derive(Clone, Copy, Debug)]
pub struct Fling {
    /// CSS pixels a millisecond, on each axis, in the scroll's direction.
    pub velocity: (f64, f64),
    at: web_time::Instant,
}

/// How fast a fling dies: the time for its speed to fall to a third,
/// near enough what iOS's own scroll views take.
const FLING_DECAY_MS: f64 = 325.0;
/// Below this (CSS pixels a millisecond) a fling has stopped.
const FLING_REST: f64 = 0.02;
/// The fastest a fling starts, however hard the flick.
const FLING_MAX: f64 = 6.0;

impl Fling {
    /// A fling at `velocity` (CSS pixels a millisecond, in the scroll's
    /// direction), or none if that is too slow to be one.
    #[must_use]
    pub fn thrown(velocity: (f64, f64)) -> Option<Self> {
        let velocity = (
            velocity.0.clamp(-FLING_MAX, FLING_MAX),
            velocity.1.clamp(-FLING_MAX, FLING_MAX),
        );
        (velocity.0.hypot(velocity.1) > FLING_REST * 5.0).then(|| Self {
            velocity,
            at: web_time::Instant::now(),
        })
    }

    /// How far it has carried since it was last asked, and whether it is
    /// still going.
    pub fn step(&mut self) -> ((f64, f64), bool) {
        let now = web_time::Instant::now();
        let dt = now.duration_since(self.at).as_secs_f64() * 1000.0;
        self.at = now;
        // The distance under an exponentially dying speed over `dt`.
        let decay = (-dt / FLING_DECAY_MS).exp();
        let carried = FLING_DECAY_MS * (1.0 - decay);
        let moved = (self.velocity.0 * carried, self.velocity.1 * carried);
        self.velocity = (self.velocity.0 * decay, self.velocity.1 * decay);
        (moved, self.velocity.0.hypot(self.velocity.1) > FLING_REST)
    }
}

/// A way to ask for another frame, for a widget still moving with nothing
/// to prompt a paint (a fling): the window's `request_redraw` under
/// Blitz, which paints a widget only after an event; nothing on a page,
/// which paints every frame anyway. Read from context, so call it where a
/// context can be read (a component, or a hook's closure) — not in a
/// widget's constructor, which tests call with no runtime.
#[must_use]
pub fn redraw_hook() -> Option<std::rc::Rc<dyn Fn()>> {
    #[cfg(feature = "native")]
    {
        let window = try_consume_context::<std::sync::Arc<dyn winit::window::Window>>()?;
        Some(std::rc::Rc::new(move || window.request_redraw()))
    }
    #[cfg(not(feature = "native"))]
    None
}

/// A finger's speed, from its last few moves: CSS pixels a millisecond on
/// each axis. Kept short, so a finger that slowed before letting go
/// throws gently.
#[derive(Clone, Debug, Default)]
pub struct Speed {
    samples: Vec<(web_time::Instant, (f64, f64))>,
}

impl Speed {
    /// The finger is at `at`, now.
    pub fn moved(&mut self, at: (f64, f64)) {
        let now = web_time::Instant::now();
        self.samples.push((now, at));
        self.samples
            .retain(|(when, _)| now.duration_since(*when).as_millis() <= 100);
    }

    /// Its speed over the samples kept.
    #[must_use]
    pub fn velocity(&self) -> (f64, f64) {
        let (Some(first), Some(last)) = (self.samples.first(), self.samples.last()) else {
            return (0.0, 0.0);
        };
        let ms = last.0.duration_since(first.0).as_secs_f64() * 1000.0;
        if ms < 1.0 {
            return (0.0, 0.0);
        }
        ((last.1.0 - first.1.0) / ms, (last.1.1 - first.1.1) / ms)
    }
}

/// How far a finger may wander, in CSS pixels, and still be a tap. Past
/// it the press is a scroll.
pub const SLOP: f64 = 10.0;

/// Whether touch mode is on: a context the shell provides.
#[derive(Clone, Copy, PartialEq)]
pub struct Touch(pub Signal<bool>);

impl Touch {
    /// Touch mode as the device suggests: on where the screen is the
    /// pointer.
    #[must_use]
    pub fn detect() -> Self {
        Self(Signal::new(detected()))
    }
}

/// Whether touch mode is on here. Off with no [`Touch`] provided.
#[must_use]
pub fn use_touch() -> bool {
    try_use_context::<Touch>().is_some_and(|touch| (touch.0)())
}

/// Whether the device's own pointer is a finger.
#[must_use]
pub fn detected() -> bool {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        return true;
    }
    coarse_pointer()
}

#[cfg(feature = "web")]
fn coarse_pointer() -> bool {
    web_sys::window()
        .and_then(|window| window.match_media("(pointer: coarse)").ok().flatten())
        .is_some_and(|query| query.matches())
}

#[cfg(not(feature = "web"))]
const fn coarse_pointer() -> bool {
    false
}

/// One step of a two-finger gesture: how far the fingers spread on each
/// axis since the last (`sx` across, `sy` down — 1.0 is no change), where
/// they are now (`mid`), and how far that moved (`dx`, `dy`). What any
/// view that pinches and pans does with it is its own: a timeline zooms
/// time across and rows down; a page zooms both alike.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub sx: f64,
    pub sy: f64,
    pub mid: (f64, f64),
    pub dx: f64,
    pub dy: f64,
}

/// How far apart two fingers must be on an axis for a pinch to spread it:
/// nearer than this they are side by side, not apart, and that axis's zoom
/// stays.
pub const PINCH_SPAN: f64 = 40.0;

/// The step between two fingers at `a0`, `b0` and the same two at `a1`,
/// `b1`. Each axis's spread is held to a quarter either way per step, so a
/// finger that jumps (a dropped event) does not throw the zoom.
#[must_use]
pub fn pinch_step(a0: (f64, f64), b0: (f64, f64), a1: (f64, f64), b1: (f64, f64)) -> Step {
    let spread = |old: f64, new: f64| {
        if old.abs() < PINCH_SPAN || new.abs() < 1.0 {
            1.0
        } else {
            (new.abs() / old.abs()).clamp(0.8, 1.25)
        }
    };
    let mid0 = ((a0.0 + b0.0) / 2.0, (a0.1 + b0.1) / 2.0);
    let mid1 = ((a1.0 + b1.0) / 2.0, (a1.1 + b1.1) / 2.0);
    Step {
        sx: spread(a0.0 - b0.0, a1.0 - b1.0),
        sy: spread(a0.1 - b0.1, a1.1 - b1.1),
        mid: mid1,
        dx: mid1.0 - mid0.0,
        dy: mid1.1 - mid0.1,
    }
}

/// The fingers on a view, for one that only pans and pinches (a chart, a
/// page): one finger drags it, two pinch and drag it together. Fed the
/// view's pointer events; says what each move did.
#[derive(Clone, Debug, Default)]
pub struct Fingers {
    down: Vec<(blitz_traits::events::BlitzPointerId, (f64, f64))>,
}

impl Fingers {
    /// A finger down at `at`. The third and later are not followed.
    pub fn down(&mut self, id: blitz_traits::events::BlitzPointerId, at: (f64, f64)) {
        if self.down.len() < 2 && !self.down.iter().any(|(each, _)| *each == id) {
            self.down.push((id, at));
        }
    }

    /// A finger moved to `at`: what the view should do, if it is one of
    /// the fingers followed — one finger, a drag (spread 1.0); two, a pinch.
    pub fn moved(
        &mut self,
        id: blitz_traits::events::BlitzPointerId,
        at: (f64, f64),
    ) -> Option<Step> {
        let i = self.down.iter().position(|(each, _)| *each == id)?;
        let was = self.down[i].1;
        self.down[i].1 = at;
        Some(match self.down.as_slice() {
            [(_, other0), (_, other1)] => {
                // The other finger stayed where it was.
                let other = if i == 0 { *other1 } else { *other0 };
                pinch_step(was, other, at, other)
            }
            _ => Step {
                sx: 1.0,
                sy: 1.0,
                mid: at,
                dx: at.0 - was.0,
                dy: at.1 - was.1,
            },
        })
    }

    /// A finger lifted (or cancelled). The other, if any, carries on as a
    /// drag from where it is.
    pub fn up(&mut self, id: blitz_traits::events::BlitzPointerId) {
        self.down.retain(|(each, _)| *each != id);
    }

    /// Whether any finger is down.
    #[must_use]
    pub fn any(&self) -> bool {
        !self.down.is_empty()
    }
}

#[cfg(test)]
mod gesture_tests {
    use super::*;

    #[test]
    fn a_pinch_spreads_each_axis_on_its_own_and_carries_the_middle() {
        // Apart across only: time zooms, rows do not.
        let step = pinch_step(
            (100.0, 200.0),
            (200.0, 200.0),
            (80.0, 200.0),
            (220.0, 200.0),
        );
        assert!((step.sx - 1.25).abs() < 1e-9, "{step:?}");
        assert!((step.sy - 1.0).abs() < 1e-9);
        assert_eq!((step.dx, step.dy), (0.0, 0.0), "spread about its middle");
        // Both fingers moved together: a drag, no zoom.
        let step = pinch_step(
            (100.0, 100.0),
            (200.0, 200.0),
            (110.0, 90.0),
            (210.0, 190.0),
        );
        assert_eq!((step.sx, step.sy), (1.0, 1.0));
        assert_eq!((step.dx, step.dy), (10.0, -10.0));
    }

    #[test]
    fn one_finger_drags_and_a_second_makes_it_a_pinch() {
        use blitz_traits::events::BlitzPointerId::Finger;
        let mut fingers = Fingers::default();
        fingers.down(Finger(1), (100.0, 100.0));
        let drag = fingers.moved(Finger(1), (110.0, 105.0)).expect("a drag");
        assert_eq!((drag.sx, drag.dx, drag.dy), (1.0, 10.0, 5.0));
        fingers.down(Finger(2), (210.0, 105.0));
        let pinch = fingers.moved(Finger(2), (310.0, 105.0)).expect("a pinch");
        assert!(pinch.sx > 1.0, "{pinch:?}");
        fingers.up(Finger(1));
        assert!(fingers.any(), "the other finger carries on");
        assert!(
            fingers.moved(Finger(3), (0.0, 0.0)).is_none(),
            "an unfollowed finger"
        );
    }
}
