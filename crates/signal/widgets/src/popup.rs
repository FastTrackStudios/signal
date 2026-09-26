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

use std::rc::Rc;

use dioxus::prelude::*;

type Render = Rc<dyn Fn() -> Element>;

#[derive(Clone)]
struct Popup {
    /// Top-left, window (client) coordinates.
    x: f64,
    y: f64,
    /// The width of what opened it — the menu is at least this wide.
    min_width: f64,
    render: Render,
    on_close: Option<Rc<dyn Fn()>>,
}

/// The app root's popup layer. `Copy`: a signal handle.
#[derive(Clone, Copy)]
pub struct PopupHost {
    open: Signal<Option<Popup>>,
}

impl PopupHost {
    /// Provide a host for everything below the calling component; mount a
    /// [`PopupLayer`] inside the (positioned) root element.
    pub fn provide() -> Self {
        use_context_provider(|| Self {
            open: Signal::new(None),
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
        mut self,
        x: f64,
        y: f64,
        min_width: f64,
        render: impl Fn() -> Element + 'static,
        on_close: impl Fn() + 'static,
    ) {
        self.close();
        self.open.set(Some(Popup {
            x,
            y,
            min_width,
            render: Rc::new(render),
            on_close: Some(Rc::new(on_close)),
        }));
    }

    /// Close whatever is open.
    pub fn close(mut self) {
        let popup = self.open.write().take();
        if let Some(cb) = popup.and_then(|p| p.on_close) {
            cb();
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
    let popup = host.open.read().clone();
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
    let (ox, oy, ow, _oh) = origin();
    let (left, top, min_w) = match &popup {
        Some(p) => {
            // Keep the menu on screen — only against a width actually
            // measured (clamping to an unmeasured zero pinned it left).
            let mut left = (p.x - ox).max(0.0);
            if ow > p.min_width {
                left = left.min(ow - p.min_width);
            }
            (left, (p.y - oy).max(0.0), p.min_width)
        }
        None => (0.0, 0.0, 0.0),
    };
    let layer_style = if showing {
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
                    style: "position: absolute; left: {left}px; top: {top}px; min-width: {min_w}px;",
                    // Presses inside the menu are the menu's.
                    onpointerdown: move |e: PointerEvent| e.stop_propagation(),
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    {(p.render)()}
                }
            }
        }
    }
}
