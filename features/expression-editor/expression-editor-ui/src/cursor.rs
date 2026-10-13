//! The painted cursor's layer: [`CursorLayer`], the one component that
//! reads hover. What the glyph is and how it is drawn live in
//! [`expression_editor_paint::cursor`] — a host that owns its window
//! rasterizes the same drawing into a real cursor.

use anyrender::Scene;
use dioxus::prelude::*;
use expression_editor_core::Editor;
use expression_editor_core::tools::Mods;

pub use expression_editor_paint::cursor::{SIZE, cursor_at, draw, hotspot};

use crate::canvas;
use crate::roll_widget::SceneSlot;

// ── the layer ───────────────────────────────────────────────────────

/// The painted cursor, in its own element.
///
/// Sized to one glyph and moved by its inline style, so a pointer move
/// costs this component's render and nothing else — see the module note.
/// `pointer-events: none` keeps it transparent to the roll beneath.
#[component]
pub fn CursorLayer(
    editor: Signal<Editor>,
    /// Roll-local pointer position, or `None` when the pointer has left.
    hover: Signal<Option<(f64, f64)>>,
    /// Modifiers as of the last pointer or key event: the glyph has to
    /// change the instant Alt is held, without waiting for a move.
    mods: Signal<Mods>,
    /// The live gesture, for the closed hand.
    drag: Signal<crate::interaction::Drag>,
    locked: bool,
) -> Element {
    let slot = use_hook(SceneSlot::new);
    // A WebView has no seam for a painted scene and panics on the
    // attribute that carries one ("Any attributes are not supported by
    // the current renderer") — not on the surface that uses it, but on
    // the first mutation, which takes the whole window with it. The
    // roll, the lane strip and the drum stack are all gated for this;
    // the painted cursor is mounted in EVERY view, so missing it meant
    // the crash regardless of which surface was showing.
    #[cfg(not(feature = "webview"))]
    let widget = use_hook(|| {
        dioxus_native_dom::CustomWidgetAttr::new(crate::roll_widget::SceneWidget::new(slot.clone()))
    });
    #[cfg(feature = "webview")]
    let widget = "";

    // Mounted unconditionally, from the very first render.
    //
    // This used to return an empty `rsx!` until the pointer arrived, and
    // that is what made the roll go blank: `CustomWidgetAttr` is
    // write-once — the DOM takes the widget out of it on the first
    // mutation — so an `<object>` created on a *later* render gets no
    // `data` attribute at all. With no widget it is a replaced element
    // with no intrinsic size, so it lays out 0x0, and blitz-paint skips
    // a zero-box widget silently. The roll's own object carries a
    // comment about exactly this trap; the cursor walked into it.
    //
    // So the node is permanent and only its *style* changes. Before the
    // pointer has ever been over the roll there is nothing to draw, and
    // `visibility: hidden` says so without disturbing the box — a
    // `display: none` toggle would put the same relayout back.
    let hovering = hover();

    let cursor = hovering.map(|(x, y)| {
        let ed = editor.read();
        let resolved = cursor_at(&ed, x, y, mods(), locked);
        if drag.read().is_active() {
            resolved.while_dragging()
        } else {
            resolved
        }
    });

    let mut scene = Scene::new();
    if let Some(cursor) = cursor {
        draw(&mut scene, cursor, SIZE, SIZE);
    }
    slot.put(scene);

    // The box is 2xSIZE so a glyph whose hotspot is at a corner still
    // has room to draw in every direction from it; the drawing above is
    // centred on (SIZE, SIZE) to match.
    let box_ = SIZE * 2.0;
    let (left, top) = match hovering {
        Some((x, y)) => (x + canvas::GUTTER_W - SIZE, y + canvas::RULER_H - SIZE),
        None => (0.0, 0.0),
    };
    let visibility = if hovering.is_some() {
        "visible"
    } else {
        "hidden"
    };
    let label = cursor.map(|c| c.label()).unwrap_or("none");

    rsx! {
        object {
            "data-testid": "cursor",
            "data-cursor": "{label}",
            "data": widget,
            style: "position: absolute; pointer-events: none; display: block; \
                    visibility: {visibility}; \
                    left: {left:.1}px; top: {top:.1}px; \
                    width: {box_}px; height: {box_}px;",
        }
    }
}
