//! The strip under the roll — velocity, or a pinned controller lane.
//!
//! A second surface with the same coordinate contract as the roll above
//! it: no `viewBox`, box from layout, so `element_coordinates()` is a
//! document coordinate. It had `preserve_aspect_ratio: none` instead,
//! which stretched rather than letterboxed — visually tidy, and wrong in
//! the way that is hardest to notice, because it wrote the velocity of
//! whichever note sat under the *scaled* position.

use dioxus::prelude::*;
use dioxus_elements::input_data::MouseButton;
use expression_editor_core::Editor;

use crate::{canvas, paint, roll_widget, text, theme};

/// Write a strip value from a pointer position.
///
/// A free function over `Signal` (which is `Copy`) rather than a shared
/// closure — otherwise both pointer handlers fight over one `FnMut`.
fn strip_write(mut editor: Signal<Editor>, h: f64, x: f64, y: f64) {
    let v = (1.0 - y / h).clamp(0.0, 1.0);
    let hit: Vec<_> = {
        let ed = editor.read();
        let rx = x - canvas::GUTTER_W;
        let t = ed.camera.t_at(rx);
        ed.doc
            .notes
            .iter()
            // A generous grab window around the onset: the stem is only
            // a few pixels wide, and this is a value edit, not a
            // precision selection.
            .filter(|n| {
                let dx = (ed.camera.x(n.start) - rx).abs();
                dx <= 8.0 || (n.start <= t && n.end > t && dx <= 40.0)
            })
            .map(|n| n.id)
            .collect()
    };
    if hit.is_empty() {
        return;
    }
    let edit = match editor.read().strip_lane {
        expression_editor_core::StripLane::OffVelocity => {
            expression_editor_core::Edit::SetOffVelocity {
                notes: hit,
                velocity: v,
            }
        }
        _ => expression_editor_core::Edit::SetVelocity {
            notes: hit,
            velocity: v,
        },
    };
    editor.write().apply_live(&edit);
}

/// The velocity / CC lane strip below the roll.
///
/// Shares the roll's horizontal camera exactly — a stem must sit under
/// its note — but has its own vertical scale, because the value being
/// edited has nothing to do with pitch.
///
/// Painted rather than emitted, like the roll. This drew one svg `<rect>`
/// per note, so two thousand notes put two thousand DOM nodes under the
/// roll — and Blitz restyled and re-laid-out every one of them on every
/// camera move, which was most of what a pan cost. The scene is one
/// element and a handful of draw commands however many notes there are.
#[component]
pub fn LaneStrip(editor: Signal<Editor>) -> Element {
    let mut editor = editor;
    let mut drag = use_signal(|| None::<(f64, f64)>);
    // Where a middle-drag pan last was.
    let mut panning = use_signal(|| None::<(f64, f64)>);

    // Where each render leaves the drawing, and the shaper for its one
    // label. Both kept across renders — see `roll.rs` for why the
    // widget itself must be built exactly once.
    let slot = use_hook(roll_widget::SceneSlot::new);
    let labels = use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(text::Labeller::new())));
    let widget = use_hook(|| {
        dioxus_native_dom::CustomWidgetAttr::new(roll_widget::SceneWidget::new(slot.clone()))
    });

    let ed = editor.read();
    let h = ed.lane_strip_h;
    if h <= 0.0 {
        return rsx! {};
    }
    let vp = ed.viewport;
    let per_note = ed.strip_lane.is_per_note();
    let w = vp.w + canvas::GUTTER_W;
    slot.put(paint::strip_scene(&ed, w, h, &mut labels.borrow_mut()));
    drop(ed);

    rsx! {
        div {
            "data-testid": "lane-strip",
            style: "position: relative; flex: 0 0 auto; box-sizing: border-box; \
                    height: {h}px; overflow: hidden; \
                    background: {theme::SURFACE_BAR}; border-top: 1px solid {theme::PANEL_BORDER};",
            object {
                "data": widget,
                // Explicit and out of flow, the same box the scene was
                // built for. A widget reports no intrinsic size, and
                // blitz-paint skips one whose box is zero.
                style: "position: absolute; left: 0; top: 0; display: block; \
                        width: {w:.0}px; height: {h:.0}px; \
                        touch-action: none; user-select: none; cursor: ns-resize;",
                onpointerdown: move |e: PointerEvent| {
                    let c = e.data().element_coordinates();
                    // Middle-drag pans here too. The strip shares the
                    // roll's time axis, so being able to scroll the roll
                    // but not the lane under it left the two showing
                    // different bars.
                    if matches!(e.trigger_button(), Some(MouseButton::Auxiliary)) {
                        panning.set(Some((c.x, c.y)));
                        return;
                    }
                    if !per_note {
                        return;
                    }
                    editor.write().begin_gesture();
                    drag.set(Some((c.x, c.y)));
                    strip_write(editor, h, c.x, c.y);
                },
                onpointermove: move |e: PointerEvent| {
                    let c = e.data().element_coordinates();
                    if let Some((lx, _ly)) = panning() {
                        // Time only. The strip's vertical axis is a
                        // *value*, not pitch, so dragging up and down
                        // here would scroll something that has no
                        // scroll — the roll's own vertical is unrelated.
                        editor.write().pan_px(c.x - lx, 0.0);
                        panning.set(Some((c.x, c.y)));
                        return;
                    }
                    if drag.read().is_none() {
                        return;
                    }
                    strip_write(editor, h, c.x, c.y);
                },
                onpointerup: move |_| {
                    drag.set(None);
                    panning.set(None);
                },
                onpointerleave: move |_| {
                    drag.set(None);
                    panning.set(None);
                },
            }
        }
    }
}
