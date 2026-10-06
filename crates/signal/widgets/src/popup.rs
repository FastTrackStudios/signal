//! Menus that are not clipped by the panel they open from.
//!
//! A dropdown drawn inside its control is an absolute box in a panel, and a
//! panel that clips its overflow (every zoomable rig panel does) cuts the
//! menu off at its edge — the reverb and delay algorithm grids were most of
//! their options hidden. Blitz has no `position: fixed` and no portals, so a
//! menu cannot escape from where it is declared.
//!
//! The host moves the drawing to the app root instead: a control measures
//! its button and hands the host a render function and a window position
//! ([`PopupHost::open`]); [`PopupLayer`], mounted at the root, draws it there
//! over a backdrop that covers the whole app — so a click anywhere else
//! closes it, which a menu inside a panel could never offer.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use dioxus::prelude::*;

type Render = Rc<dyn Fn() -> Element>;
type Wake = Arc<dyn Fn() + Send + Sync>;

/// Marks the scope a popup was opened from, kept in that scope's owner.
///
/// A popup's render function holds its opener's handlers and signals, but is
/// drawn by [`PopupLayer`] at the root. When the opener goes (a page switched,
/// a face remounted by the pick just made) those die with it, and drawing them
/// again panics the app. The mark drops with the opener: the popup is
/// forgotten, and the layer redrawn without it.
struct Opener {
    alive: Rc<Cell<bool>>,
    wake: Option<Wake>,
}

impl Drop for Opener {
    fn drop(&mut self) {
        if self.alive.replace(false)
            && let Some(wake) = &self.wake
        {
            wake();
        }
    }
}

#[derive(Clone)]
struct Popup {
    /// Top-left, window (client) coordinates.
    x: f64,
    y: f64,
    /// The width of what opened it — the menu is at least this wide.
    min_width: f64,
    render: Render,
    on_close: Option<Rc<dyn Fn()>>,
    /// Grows upward from `y` (its bottom edge there) instead of down.
    above: bool,
    /// A sheet: nearly the whole root, over a dimmed backdrop (an expanded
    /// browser), rather than a menu at a point.
    sheet: bool,
    /// False once the scope that opened it has dropped (see [`Opener`]).
    alive: Rc<Cell<bool>>,
    /// The mark in the opener's scope, freed when the popup closes.
    mark: CopyValue<Opener>,
}

impl Popup {
    fn new(x: f64, y: f64, min_width: f64, render: Render, on_close: Rc<dyn Fn()>, above: bool, sheet: bool, wake: Option<Wake>) -> Self {
        let alive = Rc::new(Cell::new(true));
        // Created in the scope opening it — an event handler runs in its own
        // component's scope — so it drops with that scope.
        let mark = CopyValue::new(Opener { alive: Rc::clone(&alive), wake });
        Self { x, y, min_width, render, on_close: Some(on_close), above, sheet, alive, mark }
    }
}

/// The app root's popup layer. `Copy`: a signal handle.
#[derive(Clone, Copy)]
pub struct PopupHost {
    open: Signal<Option<Popup>>,
    /// Redraws the [`PopupLayer`] (set when it mounts).
    wake: CopyValue<Option<Wake>>,
}

impl PopupHost {
    /// Provide a host for everything below the calling component; mount a
    /// [`PopupLayer`] inside the (positioned) root element.
    pub fn provide() -> Self {
        use_context_provider(|| Self {
            open: Signal::new(None),
            wake: CopyValue::new(None),
        })
    }

    /// The host above, if the app provides one.
    #[must_use]
    pub fn try_use() -> Option<Self> {
        try_use_context::<Self>()
    }

    /// Show `render` with its top-left at client `(x, y)`, at least
    /// `min_width` wide. Replaces any open popup (closing it first).
    /// `on_close` runs however it closes — a pick, a click away, Escape.
    pub fn open(
        self,
        x: f64,
        y: f64,
        min_width: f64,
        render: impl Fn() -> Element + 'static,
        on_close: impl Fn() + 'static,
    ) {
        self.show(x, y, min_width, Rc::new(render), on_close, false);
    }

    /// As [`open`](Self::open), but growing **upward**: the popup's bottom
    /// edge at client `y`, and no taller than the room above it (scrolling
    /// past that). For controls at the bottom of the window — the rigs'
    /// footswitches — where a menu that drops down has nowhere to go.
    pub fn open_up(
        self,
        x: f64,
        y: f64,
        min_width: f64,
        render: impl Fn() -> Element + 'static,
        on_close: impl Fn() + 'static,
    ) {
        self.show(x, y, min_width, Rc::new(render), on_close, true);
    }

    /// Show `render` as a **sheet**: nearly the whole root, over a dimmed
    /// backdrop — the way a docked panel opens out full size (the sound
    /// browser's expand). Closes like any popup: a click on the backdrop,
    /// Escape, or [`close`](Self::close).
    pub fn open_sheet(self, render: impl Fn() -> Element + 'static, on_close: impl Fn() + 'static) {
        let mut this = self;
        this.close();
        let wake = (*this.wake.peek()).clone();
        this.open.set(Some(Popup::new(0.0, 0.0, 0.0, Rc::new(render), Rc::new(on_close), false, true, wake)));
    }

    /// Whether a sheet is showing.
    #[must_use]
    pub fn sheet_open(self) -> bool {
        self.open.read().as_ref().is_some_and(|p| p.sheet)
    }

    fn show(
        mut self,
        x: f64,
        y: f64,
        min_width: f64,
        render: Render,
        on_close: impl Fn() + 'static,
        above: bool,
    ) {
        self.close();
        let wake = (*self.wake.peek()).clone();
        self.open.set(Some(Popup::new(x, y, min_width, render, Rc::new(on_close), above, false, wake)));
    }

    /// Close whatever is open.
    pub fn close(mut self) {
        let Some(popup) = self.open.write().take() else { return };
        // An opener that is gone takes its `on_close` with it: what it would
        // reset no longer exists.
        if !popup.alive.get() {
            return;
        }
        if let Some(cb) = popup.on_close {
            cb();
        }
        popup.mark.manually_drop();
    }

    /// Drop a popup whose opener has gone, without its `on_close`.
    fn forget(mut self) {
        if self.open.peek().as_ref().is_some_and(|p| !p.alive.get()) {
            self.open.write().take();
        }
    }
}

/// Draws the host's popup. Mount once inside the root, which must be
/// `position: relative` (the layer covers it; its z-index puts it on top).
#[component]
pub fn PopupLayer() -> Element {
    let Some(host) = PopupHost::try_use() else {
        return rsx! {};
    };
    let mut layer = use_signal(|| None::<Rc<MountedData>>);
    // The layer's own window origin, to turn window coordinates into
    // positions inside it (the root sits under the app's chrome).
    let mut origin = use_signal(|| (0.0f64, 0.0f64, 0.0f64, 0.0f64));
    let wake = use_hook(dioxus::core::schedule_update);
    use_hook(move || {
        let mut slot = host.wake;
        slot.set(Some(wake));
    });
    let popup = host.open.read().clone();
    // Its opener gone: not drawn (its handlers are gone too), and forgotten.
    let popup = match popup {
        Some(p) if !p.alive.get() => {
            spawn(async move { host.forget() });
            None
        }
        p => p,
    };
    let showing = popup.is_some();
    // Measured on mount and again whenever a menu opens (the window may have
    // been resized since), so the first frame is already in place.
    use_effect(move || {
        let _opening = host.open.read().is_some();
        let Some(el) = layer() else { return };
        spawn(async move {
            if let Ok(r) = el.get_client_rect().await {
                origin.set((r.origin.x, r.origin.y, r.width(), r.height()));
            }
        });
    });
    // One element, always: only its style and children change. Swapping the
    // layer's root between an idle box and a backdrop, while the panels
    // around it re-render, is the kind of replacement the renderer's tree
    // updates have tripped on.
    let (ox, oy, ow, oh) = origin();
    let place = match &popup {
        Some(p) if p.sheet => {
            "left: 3%; top: 3%; right: 3%; bottom: 3%; display: flex;".to_string()
        }
        Some(p) => {
            // Keep the menu on screen — only against a width actually
            // measured (clamping to an unmeasured zero pinned it left).
            let mut left = (p.x - ox).max(0.0);
            if ow > p.min_width {
                left = left.min(ow - p.min_width);
            }
            let at = (p.y - oy).max(0.0);
            if p.above && oh > 0.0 {
                // Bottom-anchored: it grows up from `at` without knowing its
                // own height, and scrolls rather than leave the window.
                format!(
                    "left: {left}px; bottom: {}px; min-width: {}px; max-height: {}px; overflow-y: auto;",
                    (oh - at).max(0.0),
                    p.min_width,
                    (at - 8.0).max(80.0)
                )
            } else {
                format!("left: {left}px; top: {at}px; min-width: {}px;", p.min_width)
            }
        }
        None => String::new(),
    };
    let dim = popup.as_ref().is_some_and(|p| p.sheet);
    let layer_style = if dim {
        // A sheet dims what it covers.
        "position: absolute; inset: 0; z-index: 900; background: rgba(0,0,0,0.55);"
    } else if showing {
        // Backdrop: the whole app, so a click anywhere else closes the menu.
        "position: absolute; inset: 0; z-index: 900;"
    } else {
        // Full size (so its measurement is the layer's), and transparent to
        // the pointer while nothing is open.
        "position: absolute; inset: 0; pointer-events: none;"
    };
    rsx! {
        div {
            style: "{layer_style}",
            tabindex: "-1",
            onmounted: move |e| layer.set(Some(e.data())),
            // On click — the last event of the press — and on the next tick:
            // removing the node under the pointer while the renderer is still
            // handling its press left it tracking a node that no longer
            // existed, and the release that followed crashed the app.
            onclick: move |_| {
                if showing {
                    spawn(async move { host.close() });
                }
            },
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    host.close();
                }
            },
            if let Some(p) = popup {
                div {
                    style: "position: absolute; {place}",
                    // Presses inside the menu are the menu's.
                    onpointerdown: move |e: PointerEvent| e.stop_propagation(),
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    {(p.render)()}
                }
            }
        }
    }
}
