//! Tools and the mouse pointer.
//!
//! Two things decide what a press over the arrangement does, and the
//! pointer's shape has to say which one is in charge:
//!
//! - a **tool**: a mode the panel holds (the zoom spring on a held `z`,
//!   the hand on a held middle button). While one is up it owns the
//!   mouse outright. The arrangement's own gestures stand down, or a
//!   zoom-drag would also drag out a time selection underneath it.
//! - otherwise, the **mouse map** ([`crate::mousemap`]): the context
//!   under the pointer and the keys held pick an action, and the pointer
//!   shows what a drag from here would do (a trim arrow on an item's
//!   edge, a crosshair once Ctrl turns the drag into a razor, the copy
//!   badge on Alt), so a modifier's meaning is visible before it is used.
//!
//! [`Pointing`] is shared between the panel (which knows about tools and
//! the keyboard) and the widget (which knows what is under the pointer).
//! The shape is set on the window directly, not through CSS: Blitz reads
//! a node's `cursor` only when the pointer crosses into another node, and
//! the whole arrangement is one node.

use cursor_icon::CursorIcon;
use expression_editor_core::cursor::Cursor;

use crate::window_cursor::{Glyph, Shape};
use std::cell::RefCell;
use std::rc::Rc;

use input_config_proto::MouseModifierContext as Context;

use crate::mousemap::{self, Action, Gesture, Mods};

/// Whether the zoom key (`z`, by the physical key) is held down — read
/// at the window, whatever has the keyboard focus, so the arrangement and
/// the Editor answer one held key the same way and at once: drag zooms,
/// the wheel zooms time, with Shift the rows. Kept here rather than by
/// either surface, because a docked Editor without the focus never hears
/// the key itself.
#[cfg(not(test))]
static ZOOM_HELD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
thread_local! {
    // Per test thread: tests that hold the key run beside tests that do not.
    static ZOOM_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The window saw the zoom key go down (`true`) or up.
pub fn set_zoom_held(held: bool) {
    #[cfg(not(test))]
    ZOOM_HELD.store(held, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    ZOOM_HELD.with(|z| z.set(held));
}

/// Whether the zoom key is held.
#[must_use]
pub fn zoom_held() -> bool {
    #[cfg(not(test))]
    return ZOOM_HELD.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    ZOOM_HELD.with(std::cell::Cell::get)
}

/// A mode that owns the mouse while it is up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// None: the mouse map decides.
    #[default]
    Map,
    /// The zoom spring, held on `z`: drag or wheel zooms.
    Zoom,
    /// The hand, held on the middle button: drag pans.
    Pan,
}

/// What the pointer is over, as the widget last saw it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Over {
    /// Nothing the widget answers for (or the pointer is elsewhere).
    #[default]
    Nothing,
    /// A continuous control in the track panel: a knob, turned by
    /// dragging up and down.
    Knob,
    /// A button in the track panel.
    Button,
    /// The lanes or the ruler, in a mouse-map context, and which kind of
    /// item edge (if any) the pointer is on.
    Map(Context, Edge),
    /// A drum hit's marker (Edit mode): dragged, it moves sideways.
    Hit,
}

/// What the context cannot say about an item's edge.
///
/// Whether it is a seam two butted items share, and which way a fade
/// under the pointer runs — the shape has to differ, or it does not tell you which you
/// have.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Edge {
    #[default]
    Plain,
    /// Another item's edge sits on this one: they move as one.
    Seam,
    FadeIn,
    FadeOut,
}

/// Where a pointer shape goes: the native window's cursor, or the web
/// canvas's CSS `cursor`.
pub type Sink = crate::window_cursor::Sink;

/// The shared state, and where it sets the pointer.
#[derive(Default)]
pub struct Pointing {
    window: Option<Sink>,
    /// Written by the panel.
    pub tool: Tool,
    /// The keys held, from the panel's own modifier events (the widget
    /// only hears about keys when an event of its own carries them).
    pub mods: Mods,
    /// Whether the pointer is inside the panel.
    pub inside: bool,
    /// Written by the widget on every move.
    pub over: Over,
    /// The action of a drag the widget has taken and not yet let go of,
    /// and the edge it was taken on.
    pub gesture: Option<(Action, Edge)>,
    /// A tool was used while its key was held: a held `z` zoomed, so its
    /// release closes the which-key tree it also opened. Set by the
    /// panel, taken by the widget on the key-up.
    pub tool_used: bool,
}

pub type Shared = Rc<RefCell<Pointing>>;

impl Pointing {
    #[must_use]
    pub fn shared(window: Option<Sink>) -> Shared {
        Rc::new(RefCell::new(Self {
            window,
            ..Self::default()
        }))
    }

    /// Whether a tool has the mouse, so the arrangement's own gestures
    /// must not start.
    #[must_use]
    pub fn tool_active(&self) -> bool {
        self.tool != Tool::Map
    }

    /// The shape for the state as it stands.
    #[must_use]
    pub fn icon(&self) -> Shape {
        icon_for(self.tool, self.gesture, self.over, self.mods)
    }

    /// Put the shape on the window. Every time, not only on a change:
    /// Blitz resets it whenever the pointer crosses into another node,
    /// so a cached "already showing that" would be wrong after a
    /// crossing and nothing would say so.
    pub fn apply(&self) {
        if self.inside
            && let Some(window) = &self.window
        {
            window(self.icon());
        }
    }
}

/// The pointer shape: a tool's own, then a drag in flight, then what a
/// drag from here would do.
#[must_use]
pub fn icon_for(tool: Tool, gesture: Option<(Action, Edge)>, over: Over, mods: Mods) -> Shape {
    match tool {
        // The Editor's own magnifier and closed hand: the same pointer
        // for the same tool in both.
        Tool::Zoom => return Shape::Glyph(Glyph::Editor(Cursor::Zoom)),
        Tool::Pan => return Shape::Glyph(Glyph::Editor(Cursor::HandClosed)),
        Tool::Map => {}
    }
    if let Some((action, edge)) = gesture {
        return dragging(action, edge);
    }
    match over {
        Over::Nothing | Over::Button => Shape::Icon(CursorIcon::Default),
        Over::Knob => Shape::Icon(CursorIcon::NsResize),
        Over::Hit => Shape::Icon(CursorIcon::EwResize),
        Over::Map(context, edge) => {
            hovering(mousemap::resolve(context, Gesture::Drag, mods).action, edge)
        }
    }
}

/// What a drag from here would do, before it starts.
fn hovering(action: Action, edge: Edge) -> Shape {
    match action {
        // The item's body stays the arrow until it is actually moving:
        // a whole lane of items under a four-way arrow is noise, and a
        // click there selects as often as a drag moves.
        Action::MoveItem => Shape::Icon(CursorIcon::Default),
        other => dragging(other, edge),
    }
}

/// The shape of a drag, once it is one.
fn dragging(action: Action, edge: Edge) -> Shape {
    let icon = |icon| Shape::Icon(icon);
    match action {
        // REAPER's brackets: `[` on a start, `]` on an end, `][` on the
        // seam two items share.
        Action::TrimLeft | Action::TrimRight if edge == Edge::Seam => Shape::Glyph(Glyph::Seam),
        Action::TrimLeft => Shape::Glyph(Glyph::Editor(Cursor::EdgeLeft)),
        Action::TrimRight => Shape::Glyph(Glyph::Editor(Cursor::EdgeRight)),
        // A fade's ramp, facing the way it runs: the map resolves either
        // corner's drag to one action, so the corner under the pointer
        // says which.
        Action::FadeIn | Action::FadeOut | Action::FadeShape => match edge {
            Edge::FadeOut => Shape::Glyph(Glyph::FadeOut),
            Edge::FadeIn => Shape::Glyph(Glyph::FadeIn),
            _ if action == Action::FadeOut => Shape::Glyph(Glyph::FadeOut),
            _ => Shape::Glyph(Glyph::FadeIn),
        },
        Action::MoveItem | Action::MoveRazorArea => icon(CursorIcon::Move),
        Action::CopyItem => icon(CursorIcon::Copy),
        Action::SlipItem => icon(CursorIcon::EwResize),
        // The Editor's razor: the same blade cuts in both.
        Action::RazorArea => Shape::Glyph(Glyph::Editor(Cursor::Razor)),
        Action::ResizeRazorArea => Shape::Glyph(Glyph::Editor(Cursor::RazorEdge)),
        // The Editor's marquee pointers, on the same gesture.
        Action::MarqueeSelect => Shape::Glyph(Glyph::Editor(Cursor::Crosshair)),
        Action::MarqueeAdd => Shape::Glyph(Glyph::Editor(Cursor::MarqueeAdd)),
        Action::SetEditCursor
        | Action::TimeSelection
        | Action::SelectItem
        | Action::ToggleItemSelection
        | Action::RemoveRazorArea
        | Action::Nothing => icon(CursorIcon::Default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: false,
    };
    const CTRL: Mods = Mods {
        shift: false,
        ctrl: true,
        alt: false,
    };
    const ALT: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: true,
    };

    /// A tool's shape wins over everything under it, including a drag
    /// the map would have started there.
    #[test]
    fn a_tool_shows_its_own_shape_wherever_it_is() {
        let edge = Over::Map(Context::MediaItemLeftEdge, Edge::Plain);
        let zoom = Shape::Glyph(Glyph::Editor(Cursor::Zoom));
        assert_eq!(icon_for(Tool::Zoom, None, edge, NONE), zoom);
        assert_eq!(
            icon_for(Tool::Pan, None, edge, CTRL),
            Shape::Glyph(Glyph::Editor(Cursor::HandClosed))
        );
        assert_eq!(
            icon_for(
                Tool::Zoom,
                Some((Action::MoveItem, Edge::Plain)),
                Over::Knob,
                NONE
            ),
            zoom
        );
    }

    /// Each modifier's meaning on an item shows before the press.
    #[test]
    fn the_modifiers_show_what_a_drag_would_do() {
        let body = Over::Map(Context::MediaItemBottomHalf, Edge::Plain);
        assert_eq!(
            icon_for(Tool::Map, None, body, NONE),
            Shape::Icon(CursorIcon::Default)
        );
        assert_eq!(
            icon_for(Tool::Map, None, body, ALT),
            Shape::Icon(CursorIcon::Copy)
        );
        assert_eq!(
            icon_for(Tool::Map, None, body, CTRL),
            Shape::Glyph(Glyph::Editor(Cursor::Razor))
        );
        let chord = Mods {
            shift: false,
            ctrl: true,
            alt: true,
        };
        assert_eq!(
            icon_for(Tool::Map, None, body, chord),
            Shape::Icon(CursorIcon::EwResize)
        );
        let empty = Over::Map(Context::ArrangeView, Edge::Plain);
        assert_eq!(
            icon_for(Tool::Map, None, empty, CTRL),
            Shape::Glyph(Glyph::Editor(Cursor::Razor))
        );
    }

    /// REAPER's brackets: `[` on a start, `]` on an end, `][` where two
    /// items meet; a fade's ramp faces the way it runs.
    #[test]
    fn edges_and_fades_show_which_way_they_drag() {
        let at = |c, e| icon_for(Tool::Map, None, Over::Map(c, e), NONE);
        let glyph = |g| Shape::Glyph(g);
        assert_eq!(
            at(Context::MediaItemLeftEdge, Edge::Plain),
            glyph(Glyph::Editor(Cursor::EdgeLeft))
        );
        assert_eq!(
            at(Context::MediaItemRightEdge, Edge::Plain),
            glyph(Glyph::Editor(Cursor::EdgeRight))
        );
        assert_eq!(
            at(Context::MediaItemLeftEdge, Edge::Seam),
            glyph(Glyph::Seam)
        );
        assert_eq!(
            at(Context::MediaItemRightEdge, Edge::Seam),
            glyph(Glyph::Seam)
        );
        let fade_in = at(Context::MediaItemFade, Edge::FadeIn);
        let fade_out = at(Context::MediaItemFade, Edge::FadeOut);
        assert!(matches!(
            fade_in,
            Shape::Glyph(Glyph::FadeIn | Glyph::FadeOut)
        ));
        assert_ne!(fade_in, fade_out, "the two fades face opposite ways");
        assert_eq!(fade_out, glyph(Glyph::FadeOut));
        assert_eq!(
            at(mousemap::RAZOR_AREA, Edge::Plain),
            Shape::Icon(CursorIcon::Move)
        );
    }

    /// A drag in flight keeps its shape wherever the pointer wanders.
    #[test]
    fn a_drag_keeps_its_shape_off_the_thing_it_started_on() {
        assert_eq!(
            icon_for(
                Tool::Map,
                Some((Action::MoveItem, Edge::Plain)),
                Over::Nothing,
                NONE
            ),
            Shape::Icon(CursorIcon::Move)
        );
        assert_eq!(
            icon_for(
                Tool::Map,
                Some((Action::TrimRight, Edge::Plain)),
                Over::Knob,
                NONE
            ),
            Shape::Glyph(Glyph::Editor(Cursor::EdgeRight))
        );
    }

    #[test]
    fn a_knob_turns_up_and_down() {
        assert_eq!(
            icon_for(Tool::Map, None, Over::Knob, NONE),
            Shape::Icon(CursorIcon::NsResize)
        );
        assert_eq!(
            icon_for(Tool::Map, None, Over::Button, NONE),
            Shape::Icon(CursorIcon::Default)
        );
    }
}
