//! Drags that keep going after the pointer leaves the control.
//!
//! Blitz has no pointer capture, and no `position: fixed` (it lays out as
//! `absolute`): a control's "drag shield" — a full-screen layer that owns the
//! pointer while a drag is live — only covers the nearest positioned
//! ancestor, typically the panel the knob sits in. Leave that box and the
//! knob stops following; release outside it and the drag is stuck on.
//!
//! The bus moves the shield's job to the app root, which every pointer event
//! bubbles to: a control [`begin`](DragBus::begin)s a drag with a handler, and
//! the root forwards each move and the release to it
//! ([`DragBus::root_move`] / [`DragBus::root_up`]) wherever in the window the
//! pointer is. Only a release ends a drag: the button state a move carries
//! is not reliable here (a check on it ended every drag on its first move).

use std::rc::Rc;

use dioxus::prelude::*;

/// What the root forwards to the control holding the drag.
#[derive(Clone, Copy, Debug)]
pub enum DragEvent {
    /// The pointer moved; client (window) coordinates.
    Move { x: f64, y: f64 },
    /// The drag is over.
    End,
}

type Handler = Rc<dyn Fn(DragEvent)>;

/// The app root's drag forwarder. `Copy`: a signal handle.
#[derive(Clone, Copy)]
pub struct DragBus {
    active: Signal<Option<Handler>>,
}

impl DragBus {
    /// Provide a bus for everything below the calling component — call it in
    /// the root component, and wire [`root_move`](Self::root_move) /
    /// [`root_up`](Self::root_up) onto the root element.
    pub fn provide() -> Self {
        use_context_provider(|| Self {
            active: Signal::new(None),
        })
    }

    /// The bus above, if the host provides one.
    #[must_use]
    pub fn try_use() -> Option<Self> {
        try_use_context::<Self>()
    }

    /// Start a drag: `handler` receives every move until the release.
    pub fn begin(mut self, handler: impl Fn(DragEvent) + 'static) {
        self.end();
        self.active.set(Some(Rc::new(handler)));
    }

    /// A drag is live.
    #[must_use]
    pub fn dragging(&self) -> bool {
        self.active.read().is_some()
    }

    /// End the live drag, if any.
    pub fn end(mut self) {
        let handler = self.active.write().take();
        if let Some(h) = handler {
            h(DragEvent::End);
        }
    }

    /// The root element's `onpointermove`.
    pub fn root_move(self, e: &PointerEvent) {
        let handler = self.active.read().clone();
        let Some(h) = handler else { return };
        let p = e.client_coordinates();
        h(DragEvent::Move { x: p.x, y: p.y });
    }

    /// The root element's `onpointerup`.
    pub fn root_up(self) {
        self.end();
    }
}
