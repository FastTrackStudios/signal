//! How big the roll is, and who decides.
//!
//! Its own module because it is the question this surface has got wrong
//! most often, and it was previously five loose items in the middle of a
//! two-thousand-line file.
//!
//! **The element box is the truth.** The roll is `position: absolute;
//! inset: 0` inside its cell, so its size comes from layout — responsive,
//! no pixel constants, and independent of its own content. It carries no
//! `viewBox`, so an svg user unit is a CSS pixel and a pointer position
//! *is* a document coordinate.
//!
//! [`Editor::viewport`] then has to follow that box. Where it does not,
//! the roll draws at the wrong size and the surplus is background or is
//! clipped — visibly wrong, which is the point: every earlier attempt
//! made a mismatch *invisible* by scaling, and a scaled pointer lands
//! somewhere the user did not click.
//!
//! ## Why the host reports rather than the editor measuring
//!
//! Measuring the element is the obvious answer and has failed four ways:
//!
//! - `onresize` never fires under dioxus-native — `convert_resize_data`
//!   is `unimplemented!()`, and that is the renderer the plugin, the
//!   REAPER panel and the desktop runner all use.
//! - `get_client_rect()` from a task re-enters dioxus's document while an
//!   event is being dispatched: "RefCell already borrowed", the #167
//!   panic that `tests/mpe_gestures.rs` guards.
//! - Measuring on a timer takes `doc_mut()` and forces a relayout every
//!   tick.
//! - Deriving the size from the content is what made the editor resize
//!   itself as the roll scrolled.
//!
//! So the host states its space — a desktop window from its winit resize
//! event, the REAPER panel from its dock callback — and the editor
//! subtracts its own chrome. That is a constant, and a constant is an
//! approximation; see [`CHROME_HEIGHT`].

use dioxus::prelude::*;
use expression_editor_core::{Editor, Viewport};

use crate::canvas;

// ── the chrome, one constant per row ─────────────────────────────────
//
// These are not descriptions of the chrome: they are what the chrome
// *is*. Each component sets its own height from the constant beside it,
// so a row cannot drift from the number the roll is sized against.
// `tests/geometry.rs` measures every one of them and fails if a
// component and its constant disagree.
//
// They are fixed rather than content-derived because the roll's box is
// this window less these rows, and a row whose height depended on its
// content — or, as the toolbar's wrapping once did, on the window's
// *width* — would make that subtraction a guess.

/// The tool bar — one row.
///
/// It was 60 for two rows: the modes on top, the verbs underneath. The
/// modes moved into the panel down the right, where they cost width we
/// already had instead of height the roll never has enough of.
pub const TOOLBAR_H: f64 = 30.0;
/// The track switcher under it.
pub const SWITCHER_H: f64 = 28.0;
/// The chord/selection row — **gone**, and kept at zero rather than
/// deleted so the arithmetic below still reads as a list of rows.
///
/// It repeated the inspector for most of its 30px; the chord name and
/// its pitches moved into the status bar, where they cost nothing.
pub const CHORD_H: f64 = 0.0;
/// The status bar along the bottom.
pub const STATUS_H: f64 = 26.0;
/// The inspector down the right, open.
pub const INSPECTOR_W: f64 = 236.0;
/// The tab it collapses to, which is still chrome the roll does not get.
pub const INSPECTOR_TAB_W: f64 = 18.0;

/// The rows that are always there: tool bar, chord row, status bar.
///
/// The track switcher and the lane strip are *not* included. Neither is
/// a constant — the switcher hides itself below two tracks, and the
/// strip's height is a document property the user can drag — so both are
/// fields on [`Chrome`] instead. A single fixed total is exactly what
/// this constant used to be, and it was wrong whenever either changed.
pub const CHROME_HEIGHT: f64 = TOOLBAR_H + CHORD_H + STATUS_H;

/// The space the host has given the editor, in CSS pixels.
///
/// The editor cannot discover this for itself in any renderer it runs
/// in, so the host states it: a desktop window from its winit resize
/// event, the REAPER panel from its dock callback. `None` until someone
/// says, which leaves the viewport the document was built with.
pub static AVAILABLE: GlobalSignal<Option<(f64, f64)>> = Signal::global(|| None);

/// Tell the editor how much room it has. Idempotent; call it as often as
/// a resize drag fires.
pub fn available_space(width: f64, height: f64) {
    let next = Some((width, height));
    if *AVAILABLE.read() != next {
        *AVAILABLE.write() = next;
    }
}

/// The viewport for whatever space the host last reported.
///
/// What a *new* document should be built with. Constructing one against
/// a fixed size — which is what the runner's constant did — resets the
/// surface to that size every time a scene or a file is opened, so the
/// editor snapped back to its opening aspect no matter how wide the
/// window had been dragged.
///
/// `fallback` covers the first load, before any host has said anything.
pub fn current_viewport(fallback: Viewport) -> Viewport {
    AVAILABLE()
        .map(|(w, h)| viewport_in(w, h))
        .unwrap_or(fallback)
}

/// What the editor draws around the roll, for a given host space.
///
/// One struct rather than several loose subtractions, because the roll's
/// box is *whatever is left* and every term has to be accounted for
/// exactly once. The width term is the one that kept being forgotten:
/// `viewport_in` subtracted only the key gutter, so the roll was drawn
/// an inspector wider than the cell it sat in and its right-hand
/// couple of hundred pixels lived permanently underneath the panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chrome {
    /// Whether the inspector is open, which decides its width.
    pub inspector_open: bool,
    /// Whether the track switcher is showing. It hides itself when there
    /// is only one track, and a row that comes and goes cannot be a
    /// constant in the subtraction.
    pub switcher: bool,
    /// The lane strip's height — a document property, not a constant.
    pub lane_strip_h: f64,
}

impl Default for Chrome {
    fn default() -> Self {
        Self {
            inspector_open: true,
            switcher: false,
            lane_strip_h: 0.0,
        }
    }
}

impl Chrome {
    /// Width taken by the panel down the right.
    pub fn side_w(&self) -> f64 {
        if self.inspector_open {
            INSPECTOR_W
        } else {
            INSPECTOR_TAB_W
        }
    }

    /// Height taken by every row above and below the roll.
    pub fn stack_h(&self) -> f64 {
        CHROME_HEIGHT + if self.switcher { SWITCHER_H } else { 0.0 } + self.lane_strip_h.max(0.0)
    }
}

/// The chrome a given document draws, with the inspector in a given
/// state.
///
/// The two terms that are not constants both come from somewhere:
/// whether the switcher takes a row is [`crate::switcher::is_shown`],
/// and the strip's height is a document property. Asking here means no
/// caller has to remember either.
pub fn chrome_of(ed: &Editor, inspector_open: bool) -> Chrome {
    Chrome {
        inspector_open,
        switcher: crate::switcher::is_shown(ed),
        // The stack view renders instead of the roll *and* its lane
        // strip, so in that mode there is no strip to subtract. Charging
        // for one left the stack short by its height — which is why it
        // used to measure itself on mount instead of trusting this.
        lane_strip_h: if ed.stacked { 0.0 } else { ed.lane_strip_h },
    }
}

/// Fit `ed` to `(width, height)` of host space.
///
/// The eager form of what the editor does on mount, for a caller that
/// wants a document already sized to the window it is about to be shown
/// in — a fixture, or a freshly loaded file.
pub fn fit(ed: &mut Editor, width: f64, height: f64, inspector_open: bool) {
    let chrome = chrome_of(ed, inspector_open);
    ed.resize(viewport_within(width, height, chrome));
}

/// The roll's viewport inside `(width, height)` of host space.
///
/// The gutter and ruler come off for the same reason the chrome does:
/// `vp` is the *note area*, while the svg drawn for it is
/// `vp.w + GUTTER_W` by `vp.h + RULER_H`. That svg has to be exactly its
/// cell — Blitz scales a replaced element to fit, so a mismatch is not
/// clipping but a silent rescale of the drawing and of every pointer
/// position with it.
pub fn viewport_in(width: f64, height: f64) -> Viewport {
    viewport_within(width, height, Chrome::default())
}

/// The roll's viewport inside `(width, height)`, for a stated chrome.
pub fn viewport_within(width: f64, height: f64, chrome: Chrome) -> Viewport {
    Viewport::new(
        (width - chrome.side_w() - canvas::GUTTER_W).max(1.0),
        (height - chrome.stack_h() - canvas::RULER_H).max(1.0),
    )
}
