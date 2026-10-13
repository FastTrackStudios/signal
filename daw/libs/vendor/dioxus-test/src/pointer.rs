//! Pointer input for tests — FTS extension over the vendored upstream.
//!
//! Upstream `dioxus-test` only synthesizes `click` directly on a target
//! element (no hit-testing, no coordinates). These helpers feed real
//! [`blitz_traits::events::UiEvent`] pointer events through
//! `DioxusDocument::handle_ui_event` — the same entry point a windowing
//! shell drives — so events are hit-tested against the resolved layout,
//! bubble through the DOM, and carry real element-relative coordinates.
//! A `pointer_down` → `pointer_move`(s) → `pointer_up` sequence is a DRAG,
//! exactly as a user performs one in the plugin editor window.
//!
//! Companion to the keyboard extension in `src/keyboard.rs`.

use blitz_dom::Document as _;
use blitz_traits::events::{
    BlitzPointerEvent, BlitzPointerId, BlitzWheelDelta, BlitzWheelEvent, MouseEventButton,
    MouseEventButtons, Point, PointerCoords, UiEvent,
};
use keyboard_types::Modifiers;

use crate::{DocumentTester, ResolvedElement};

/// Builds a mouse pointer event at document coordinates `(x, y)`.
///
/// All coordinate spaces (page/screen/client) are set to the same value —
/// the headless document has no scrolling or window chrome, so they
/// coincide. The `element` field is left at zero: the blitz event driver
/// recomputes it per target during dispatch.
fn pointer_event(
    x: f64,
    y: f64,
    button: MouseEventButton,
    buttons: MouseEventButtons,
    mods: Modifiers,
) -> BlitzPointerEvent {
    let (x, y) = (x as f32, y as f32);
    BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: PointerCoords {
            page_x: x,
            page_y: y,
            screen_x: x,
            screen_y: y,
            client_x: x,
            client_y: y,
        },
        button,
        buttons,
        mods,
        details: Default::default(),
        element: Point::default(),
        active_pointers: Default::default(),
    }
}

impl DocumentTester {
    /// Presses the primary mouse button at document coordinates `(x, y)`.
    ///
    /// The event is hit-tested: whichever element the layout places under
    /// that point receives it (respecting `pointer-events: none`), and it
    /// bubbles from there. Call [`DocumentTester::pump`] afterwards to run
    /// the Dioxus event handlers.
    pub fn pointer_down(&self, x: f64, y: f64) {
        self.send_ui_event(UiEvent::PointerDown(pointer_event(
            x,
            y,
            MouseEventButton::Main,
            MouseEventButtons::Primary,
            Modifiers::empty(),
        )));
    }

    /// Moves the mouse pointer to document coordinates `(x, y)`.
    ///
    /// `held` reports whether the primary button is currently pressed —
    /// pass `true` for the intermediate events of a drag (components
    /// commonly check `held_buttons()` to distinguish dragging from
    /// hovering).
    pub fn pointer_move(&self, x: f64, y: f64, held: bool) {
        let buttons = if held {
            MouseEventButtons::Primary
        } else {
            MouseEventButtons::None
        };
        self.send_ui_event(UiEvent::PointerMove(pointer_event(
            x,
            y,
            MouseEventButton::Main,
            buttons,
            Modifiers::empty(),
        )));
    }

    /// [`DocumentTester::pointer_down`] with modifiers held.
    ///
    /// Surfaces whose gesture map is keyed on modifiers — an editor
    /// where Alt+drag draws and a plain drag marquees — cannot be driven
    /// without this, and the plain variants above hardcode
    /// `Modifiers::empty()`.
    pub fn pointer_down_mods(&self, x: f64, y: f64, mods: Modifiers) {
        self.send_ui_event(UiEvent::PointerDown(pointer_event(
            x,
            y,
            MouseEventButton::Main,
            MouseEventButtons::Primary,
            mods,
        )));
    }

    /// [`DocumentTester::pointer_move`] with modifiers held.
    pub fn pointer_move_mods(&self, x: f64, y: f64, held: bool, mods: Modifiers) {
        let buttons = if held {
            MouseEventButtons::Primary
        } else {
            MouseEventButtons::None
        };
        self.send_ui_event(UiEvent::PointerMove(pointer_event(
            x,
            y,
            MouseEventButton::Main,
            buttons,
            mods,
        )));
    }

    /// [`DocumentTester::pointer_up`] with modifiers held.
    pub fn pointer_up_mods(&self, x: f64, y: f64, mods: Modifiers) {
        self.send_ui_event(UiEvent::PointerUp(pointer_event(
            x,
            y,
            MouseEventButton::Main,
            MouseEventButtons::None,
            mods,
        )));
    }

    /// Scrolls the mouse wheel over document coordinates `(x, y)`.
    ///
    /// `delta_y` is in pixels (negative = wheel up, the direction that
    /// increases a knob). Blitz targets wheel events at the *hovered* node,
    /// so this first moves the (unpressed) pointer there.
    pub fn wheel_mods(&self, x: f64, y: f64, delta_y: f64, mods: Modifiers) {
        self.pointer_move_mods(x, y, false, mods);
        let (fx, fy) = (x as f32, y as f32);
        self.send_ui_event(UiEvent::Wheel(BlitzWheelEvent {
            delta: BlitzWheelDelta::Pixels(0.0, delta_y),
            coords: PointerCoords {
                page_x: fx,
                page_y: fy,
                screen_x: fx,
                screen_y: fy,
                client_x: fx,
                client_y: fy,
            },
            buttons: MouseEventButtons::None,
            mods,
            element: Point::default(),
        }));
    }

    /// [`DocumentTester::wheel_mods`] with no modifiers.
    pub fn wheel(&self, x: f64, y: f64, delta_y: f64) {
        self.wheel_mods(x, y, delta_y, Modifiers::empty());
    }

    /// Releases the primary mouse button at document coordinates `(x, y)`.
    pub fn pointer_up(&self, x: f64, y: f64) {
        self.send_ui_event(UiEvent::PointerUp(pointer_event(
            x,
            y,
            MouseEventButton::Main,
            MouseEventButtons::None,
            Modifiers::empty(),
        )));
    }
}

impl ResolvedElement {
    /// Document-space coordinates of this element's top-left corner.
    ///
    /// Unlike [`ResolvedElement::upper_left`] (which reports the
    /// parent-relative taffy layout position), this walks the layout
    /// ancestry via `Node::absolute_position`, yielding the coordinates
    /// the pointer helpers above expect.
    pub fn document_origin(&self) -> (f64, f64) {
        let guard = self.document.borrow();
        let doc = guard.inner();
        let node = self.node_id.resolve(&doc);
        let p = node.absolute_position(0.0, 0.0);
        (p.x as f64, p.y as f64)
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;
    use test_that::prelude::*;

    use crate::{matchers::inner_html, render};

    #[tokio::test]
    async fn pointer_drag_drives_mouse_handlers_with_element_coordinates() {
        #[component]
        fn Draggable() -> Element {
            let mut state = use_signal(|| "idle".to_string());
            rsx! {
                div {
                    style: "width: 200px; height: 100px;",
                    "data-testid": "surface",
                    onmousedown: move |evt| {
                        let c = evt.element_coordinates();
                        state.set(format!("down {} {}", c.x, c.y));
                    },
                    onmousemove: move |evt| {
                        let c = evt.element_coordinates();
                        state.set(format!("move {} {}", c.x, c.y));
                    },
                    onmouseup: move |evt| {
                        let c = evt.element_coordinates();
                        state.set(format!("up {} {}", c.x, c.y));
                    },
                    "{state}"
                }
            }
        }

        let tester = render(Draggable).build();
        // The UA stylesheet gives <body> a margin, so resolve the surface's
        // document-space origin (this also exercises `document_origin`).
        let (ox, oy) = tester
            .query(crate::by_testid("surface"))
            .immediately()
            .unwrap()
            .document_origin();
        tester.pointer_down(ox + 10.0, oy + 20.0);
        tester
            .query(crate::by_testid("surface"))
            .expect(inner_html(contains_substring("down 10 20")))
            .await
            .unwrap();
        tester.pointer_move(ox + 50.0, oy + 60.0, true);
        tester
            .query(crate::by_testid("surface"))
            .expect(inner_html(contains_substring("move 50 60")))
            .await
            .unwrap();
        tester.pointer_up(ox + 50.0, oy + 60.0);
        tester
            .query(crate::by_testid("surface"))
            .expect(inner_html(contains_substring("up 50 60")))
            .await
            .unwrap();
    }
}
