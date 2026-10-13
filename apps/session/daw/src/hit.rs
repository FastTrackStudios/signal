//! What is under the pointer.
//!
//! One function per surface, each answering the same question: given a
//! point in the window, what did you just touch, and in what
//! [`MouseModifierContext`]?
//!
//! # Why the context comes back with the target
//!
//! REAPER's mouse behaviour is not "this widget handles clicks". It is a
//! table: for each CONTEXT — a media item's left edge, a fade, an
//! envelope segment, the ruler — and each modifier combination, an
//! action. That is what makes it configurable, and it is why the answer
//! here is a context and not a callback.
//!
//! So a hit is two things: a [`Target`] the app can act on directly
//! (this strip, this row, this rail button), and a context to look up
//! when the action should be configurable. Simple controls — a mute
//! button, a rail button — carry [`MouseModifierContext::Custom`] and
//! nobody is expected to rebind them. The ones REAPER makes
//! configurable, because the same click means different things in
//! different places, carry the real contexts.
//!
//! # Why this is not on the recorded scene
//!
//! The panels are recorded once and replayed under a transform, so
//! there is no retained tree of widgets to ask. Hit testing is
//! therefore arithmetic over the same layout the recording used —
//! `Columns`, `Frame`, the row offsets — which has the useful property
//! that a hit and a draw cannot disagree unless the arithmetic does.

use input_config_proto::MouseModifierContext as Context;

/// What the pointer is over.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Target {
    /// A mode button in the corner above the track panel.
    Mode(usize),
    /// The timeline ruler, at this many seconds, and what is under the
    /// pointer there — a mark, a band, an empty lane, or the bars.
    ///
    /// The LANE is what a press on empty space creates in: the marks
    /// lane makes a marker, a region lane makes a region. That is why
    /// there is no tool to choose — the ruler already says what you
    /// meant by where you pressed.
    Ruler { seconds: f64, on: crate::ruler::On },
    /// A track's row in the panel, or its strip in the mixer.
    Track { row: usize },
    /// The lane area of a row, at a time and a height (in session units
    /// from the top of the lanes — what a marquee spans).
    Lane { row: usize, seconds: f64, y: f64 },
    /// An item on a row, by the arrangement's index, and which part.
    Item {
        row: usize,
        index: usize,
        zone: crate::arrangement::ItemZone,
        seconds: f64,
    },
    /// Somewhere with nothing on it.
    Empty,
}

/// A hit: what was touched, and the context to resolve modifiers in.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Hit {
    pub target: Target,
    pub context: Context,
}

impl Hit {
    const fn new(target: Target, context: Context) -> Self {
        Self { target, context }
    }

    const fn empty() -> Self {
        Self::new(Target::Empty, Context::Custom("empty"))
    }
}

/// What the pointer is over in the arrangement.
#[must_use]
pub fn arrangement(
    scene: &crate::arrangement::Arrangement,
    view: crate::arrangement::Viewport,
    modes: usize,
    sections: &[daw_ui::studio::project::Section],
    markers: &[daw_ui::studio::project::Marker],
    x: f64,
    y: f64,
) -> Hit {
    let (rail_x, rail_y) = (crate::rails::SIDE, crate::rails::TOP);
    // Where the lanes start — the view says, in the panel's shape.
    let panel_w = view.panel_w;

    // The corner above the track panel, before the ruler — it is drawn
    // over it, so it is hit before it too.
    let corner_w = crate::arrangement::TCP_WIDTH;
    if y >= rail_y
        && y < rail_y + crate::ruler::ruler_h()
        && x >= rail_x
        && x < rail_x + corner_w
        && modes > 0
    {
        let each = corner_w / crate::num::coord(modes);
        let index = crate::num::index((x - rail_x) / each).min(modes.saturating_sub(1));
        return Hit::new(Target::Mode(index), Context::Custom("mode"));
    }

    // The ruler: the rest of that band. Asked what is under the
    // pointer with the same lists the ruler DRAWS from, so a press
    // lands on what it looks like it landed on.
    if y >= rail_y && y < rail_y + crate::ruler::ruler_h() {
        return Hit::new(
            Target::Ruler {
                seconds: seconds_at(x - rail_x, panel_w, view),
                on: crate::ruler::on(
                    x,
                    y,
                    rail_y,
                    rail_x + panel_w,
                    view.pps,
                    view.scroll_x,
                    sections,
                    markers,
                ),
            },
            Context::Ruler,
        );
    }

    // In screen pixels from the top of the lanes. `row_at_screen`
    // divides by the vertical zoom; `item_at` below wants the same
    // number in session units, which is that divided again.
    let screen_y = y - rail_y - crate::ruler::ruler_h() + view.scroll_y;
    let Some(row) = scene.row_at_screen(screen_y, view) else {
        return Hit::empty();
    };
    let content_y = screen_y / if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
    if x < rail_x + panel_w {
        return Hit::new(Target::Track { row }, Context::TrackControlPanel);
    }
    let seconds = seconds_at(x - rail_x, panel_w, view);
    // An item under the pointer narrows the empty arrange area to one
    // of the item contexts: its body, an edge, a fade's handle.
    if let Some((index, zone)) = scene.item_at(view, row, x - rail_x - panel_w, content_y) {
        use crate::arrangement::ItemZone as Z;
        let context = match zone {
            // The dial is the widget's to turn; to the mouse map it is
            // the body it sits on.
            Z::Body | Z::Volume => Context::MediaItemBottomHalf,
            Z::LeftEdge => Context::MediaItemLeftEdge,
            Z::RightEdge => Context::MediaItemRightEdge,
            Z::FadeIn | Z::FadeOut => Context::MediaItemFade,
        };
        return Hit::new(
            Target::Item {
                row,
                index,
                zone,
                seconds,
            },
            context,
        );
    }
    Hit::new(
        Target::Lane {
            row,
            seconds,
            y: content_y,
        },
        Context::ArrangeView,
    )
}

/// The time at a horizontal position in the content.
fn seconds_at(content_x: f64, panel_w: f64, view: crate::arrangement::Viewport) -> f64 {
    if view.pps <= 0.0 {
        return 0.0;
    }
    ((content_x - panel_w + view.scroll_x) / view.pps).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contexts REAPER makes configurable come back as real
    /// contexts; the ones nobody rebinds come back as `Custom`. That
    /// distinction is the whole point of returning a context at all.
    #[test]
    fn configurable_things_carry_a_real_context() {
        assert_eq!(Hit::empty().context, Context::Custom("empty"));
    }
}
