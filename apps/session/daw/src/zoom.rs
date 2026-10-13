//! Zoom commands: what the profile's zoom keys (`z` and its tree, `+`,
//! `-`) ask of the view, and the arithmetic that turns "frame this"
//! into a zoom and a scroll.
//!
//! Split between the two halves that each know half of it. The widget
//! knows the CONTENT (which tracks are selected, where the items are, the
//! time selection), so it resolves a [`Command`] into a [`Request`] in
//! session units: seconds across, unzoomed pixels down. The panel
//! (`studio.rs`) knows the FRAME (how big the lanes are on screen, the
//! zoom limits) and owns the zoom and scroll signals, so it turns a
//! request into a [`Target`] with [`frame`] and keeps the [`History`]
//! that `z u` / `z r` and the toggles walk.

use std::cell::RefCell;
use std::rc::Rc;

/// A zoom, as a key asks for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// `z t`: the selected tracks fill the lanes (and the time selection,
    /// when there is one). Again, to go back.
    ToggleTracks,
    /// `z v`: every track, top to bottom.
    FitTracks,
    /// `z x`: the whole session, start to end.
    Project,
    /// `z s`: the time selection, or else the selected items.
    Selection,
    /// `z z`: [`Self::Selection`], and again to go back.
    ToggleSelection,
    /// `z f`: the selected items, across and down.
    Items,
    /// `z u` / `z r`: back and forward through the zooms.
    Back,
    Forward,
    /// `+` / `-` and `Shift` with them: a step in or out, on one axis.
    Step {
        vertical: bool,
        inward: bool,
    },
}

impl Command {
    /// The profile's action ids (REAPER's, and SWS's) for the zooms this
    /// window does.
    #[must_use]
    pub fn of(id: &str) -> Option<Self> {
        Some(match id {
            "_SWS_TOGZOOMTTMIN" => Self::ToggleTracks,
            "_SWS_VZOOMFITMIN" => Self::FitTracks,
            "40295" => Self::Project,
            "_SWS_ZOOMSITMIN" => Self::Selection,
            "_SWS_TOGZOOMIMIN" => Self::ToggleSelection,
            "_SWS_ITEMZOOMMIN" => Self::Items,
            "40848" => Self::Back,
            "40762" => Self::Forward,
            "1012" => Self::Step {
                vertical: false,
                inward: true,
            },
            "1011" => Self::Step {
                vertical: false,
                inward: false,
            },
            "40111" => Self::Step {
                vertical: true,
                inward: true,
            },
            "40112" => Self::Step {
                vertical: true,
                inward: false,
            },
            _ => return None,
        })
    }
}

/// A zoom for the panel to carry out, in session units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Request {
    /// Fill the lanes with a span of time and/or a band of rows. `None`
    /// on an axis leaves that axis as it is. Rows are unzoomed pixels,
    /// the units the arrangement lays rows out in.
    Frame {
        time: Option<(f64, f64)>,
        rows: Option<(f64, f64)>,
        /// Pressed again while showing what it framed, it goes back.
        toggle: bool,
    },
    /// The view a session opens on: `time` across and `rows` down, as
    /// [`Request::Frame`] does, but zoomed in no further out than `floor`
    /// (the least zoom on each axis) — rows a finger can hit or an eye can
    /// read, items wider than slivers — even where that leaves some of
    /// the song off screen. Only the opening view: zooming out by hand
    /// still goes as far as it ever did.
    Open {
        time: Option<(f64, f64)>,
        rows: Option<(f64, f64)>,
        floor: (f64, f64),
    },
    Back,
    Forward,
    /// The track panel compact (names) or expanded: a finger swiping it.
    /// Not a zoom, but the widget's one way to ask the panel for
    /// anything, so it travels this way.
    Shape {
        compact: bool,
    },
    /// Scroll by `dx` across and `dy` down, in screen pixels: a finger
    /// dragging the lanes.
    ScrollBy {
        dx: f64,
        dy: f64,
    },
    /// Multiply one axis's zoom by `by`, about the middle of the lanes.
    Scale {
        vertical: bool,
        by: f64,
    },
    /// Two fingers: each axis's zoom multiplied by how far they spread on
    /// it (`sx` across, `sy` down), about `at` (where they are, in screen
    /// pixels from the lanes' corner), and the view carried `dx`, `dy`
    /// with them.
    Pinch {
        sx: f64,
        sy: f64,
        at: (f64, f64),
        dx: f64,
        dy: f64,
    },
}

/// The queue from the widget to the panel, drained once a frame.
pub type Requests = Rc<RefCell<Vec<Request>>>;

/// Where the view is: the zoom on each axis, and the scroll in zoomed
/// pixels. The panel's own four numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub zoom_x: f64,
    pub zoom_y: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
}

impl Target {
    /// The same view, near enough, for a toggle's "am I showing it".
    #[must_use]
    pub fn near(&self, other: &Self) -> bool {
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-6_f64.max(a.abs().max(b.abs()) * 1e-3);
        close(self.zoom_x, other.zoom_x)
            && close(self.zoom_y, other.zoom_y)
            && (self.scroll_x - other.scroll_x).abs() < 1.0
            && (self.scroll_y - other.scroll_y).abs() < 1.0
    }
}

/// The panel's side of a frame: the lanes' size on screen, the base
/// pixels a second, and the zoom limits on each axis.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub width: f64,
    pub height: f64,
    pub pps: f64,
    pub limits_x: (f64, f64),
    pub limits_y: (f64, f64),
}

/// The view that fills the frame with `time` across and `rows` down,
/// starting from `now` for any axis left as it is. A span too small to
/// fill it stops at the zoom limit, starting where the span starts.
#[must_use]
pub fn frame(now: Target, at: Frame, time: Option<(f64, f64)>, rows: Option<(f64, f64)>) -> Target {
    let mut to = now;
    if let Some((t0, t1)) = time
        && t1 > t0
        && at.pps > 0.0
    {
        to.zoom_x = (at.width / ((t1 - t0) * at.pps)).clamp(at.limits_x.0, at.limits_x.1);
        to.scroll_x = (t0 * at.pps * to.zoom_x).max(0.0);
    }
    if let Some((y0, y1)) = rows
        && y1 > y0
    {
        to.zoom_y = (at.height / (y1 - y0)).clamp(at.limits_y.0, at.limits_y.1);
        to.scroll_y = (y0 * to.zoom_y).max(0.0);
    }
    to
}

/// The zooms already been to, for `z u` / `z r` and the toggles.
#[derive(Debug, Default)]
pub struct History {
    back: Vec<Target>,
    forward: Vec<Target>,
    /// What the last toggle framed, so pressing it again while still
    /// there goes back instead.
    toggled: Option<Target>,
}

impl History {
    /// Where to go for `request` from `now`, remembering where it was.
    /// `None` when there is nowhere to go.
    pub fn go(
        &mut self,
        now: Target,
        request: Request,
        framed: impl Fn() -> Target,
    ) -> Option<Target> {
        match request {
            Request::Back => {
                let to = self.back.pop()?;
                self.forward.push(now);
                self.toggled = None;
                Some(to)
            }
            Request::Forward => {
                let to = self.forward.pop()?;
                self.back.push(now);
                self.toggled = None;
                Some(to)
            }
            Request::Frame { toggle: true, .. } if self.toggled.is_some_and(|t| t.near(&now)) => {
                self.toggled = None;
                let to = self.back.pop()?;
                self.forward.push(now);
                Some(to)
            }
            Request::Frame { toggle, .. } => {
                let to = framed();
                self.toggled = toggle.then_some(to);
                self.visit(now);
                Some(to)
            }
            Request::ScrollBy { .. } | Request::Shape { .. } | Request::Pinch { .. } => {
                Some(framed())
            }
            Request::Open { .. } => {
                self.toggled = None;
                Some(framed())
            }
            Request::Scale { .. } => {
                self.toggled = None;
                Some(framed())
            }
        }
    }

    fn visit(&mut self, now: Target) {
        self.back.push(now);
        self.forward.clear();
        // A long session of zooming is not a history anyone walks back
        // through all of.
        if self.back.len() > 64 {
            self.back.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Frame = Frame {
        width: 1000.0,
        height: 500.0,
        pps: 100.0,
        limits_x: (0.01, 32.0),
        limits_y: (0.1, 6.0),
    };
    const HOME: Target = Target {
        zoom_x: 1.0,
        zoom_y: 1.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    };

    #[test]
    fn framing_rows_fills_the_lanes_with_them() {
        // A row 100px tall at y 400: zoom 5, scrolled to 2000.
        let to = frame(HOME, AT, None, Some((400.0, 500.0)));
        assert!((to.zoom_y - 5.0).abs() < 1e-9);
        assert!((to.scroll_y - 2000.0).abs() < 1e-9);
        assert_eq!((to.zoom_x, to.scroll_x), (1.0, 0.0), "time untouched");
    }

    #[test]
    fn framing_time_fills_the_lanes_with_it() {
        // Two seconds at 100 px/s into 1000px: zoom 5, from 10s.
        let to = frame(HOME, AT, Some((10.0, 12.0)), None);
        assert!((to.zoom_x - 5.0).abs() < 1e-9);
        assert!((to.scroll_x - 5000.0).abs() < 1e-9);
    }

    #[test]
    fn a_tiny_span_stops_at_the_limit() {
        let to = frame(HOME, AT, None, Some((0.0, 1.0)));
        assert!((to.zoom_y - 6.0).abs() < 1e-9);
    }

    #[test]
    fn a_toggle_pressed_again_goes_back() {
        let mut history = History::default();
        let framed = Target {
            zoom_y: 5.0,
            scroll_y: 2000.0,
            ..HOME
        };
        let request = Request::Frame {
            time: None,
            rows: Some((400.0, 500.0)),
            toggle: true,
        };
        assert_eq!(history.go(HOME, request, || framed), Some(framed));
        assert_eq!(history.go(framed, request, || framed), Some(HOME));
    }

    #[test]
    fn back_and_forward_walk_the_zooms() {
        let mut history = History::default();
        let a = Target {
            zoom_x: 2.0,
            ..HOME
        };
        let frame_a = Request::Frame {
            time: Some((0.0, 1.0)),
            rows: None,
            toggle: false,
        };
        history.go(HOME, frame_a, || a);
        assert_eq!(history.go(a, Request::Back, || a), Some(HOME));
        assert_eq!(history.go(HOME, Request::Forward, || a), Some(a));
        assert_eq!(history.go(a, Request::Forward, || a), None, "nothing ahead");
    }

    #[test]
    fn the_profile_ids_are_the_zooms() {
        assert_eq!(
            Command::of("_SWS_TOGZOOMTTMIN"),
            Some(Command::ToggleTracks)
        );
        assert_eq!(Command::of("_SWS_VZOOMFITMIN"), Some(Command::FitTracks));
        assert_eq!(Command::of("40848"), Some(Command::Back));
        assert_eq!(Command::of("_SWS_ZOOMPREFS"), None, "not this window's");
    }
}
