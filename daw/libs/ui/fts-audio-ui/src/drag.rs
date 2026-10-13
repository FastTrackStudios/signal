//! Global drag capture for parameter-edit gestures.
//!
//! Blitz dispatches mouse events to whichever element is under the cursor (no
//! pointer capture). [`DragProvider`] wraps the editor root and routes
//! `mousemove`/`mouseup` to whichever widget started a drag. Widgets call
//! [`begin_drag`] / [`begin_drag_axis`] from their own `onmousedown`.

use crate::param::ParamHandle;
use dioxus::prelude::*;

/// Drag axis. Vertical drag = up increases value (knobs, vertical sliders).
/// Horizontal drag = right increases value (horizontal sliders).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DragAxis {
    #[default]
    Vertical,
    Horizontal,
    /// Two parameters at once (XY pad): horizontal → `handle`, vertical →
    /// `handle_y`. Right and up increase.
    Both,
}

/// In-flight drag state shared between the active widget and [`DragProvider`].
#[derive(Clone, Default)]
pub struct DragState {
    pub active: bool,
    pub handle: Option<ParamHandle>,
    /// Second parameter for [`DragAxis::Both`] (the vertical one).
    pub handle_y: Option<ParamHandle>,
    pub axis: DragAxis,
    pub start_value: f32,
    pub start_value_y: f32,
    pub start_x: f64,
    pub start_y: f64,
    /// Pixels of drag per full 0→1 sweep.
    pub sensitivity: f64,
    /// Bumped on every mousemove so subscriber widgets re-render.
    pub move_count: u64,
    /// The fine multiplier in force at the last move, so a modifier change
    /// mid-drag re-anchors instead of jumping (`fx.control.fine`). `0.0`
    /// until the first move: the press does not know the modifiers, so the
    /// first move adopts them without re-anchoring.
    pub last_mult: f64,
}

/// Begin a vertical drag (up = increase). Convenience for knobs.
pub fn begin_drag(
    drag: &mut Signal<DragState>,
    handle: ParamHandle,
    start_y: f64,
    sensitivity: f64,
) {
    begin_drag_axis(drag, handle, DragAxis::Vertical, 0.0, start_y, sensitivity);
}

/// Begin a drag along the given axis.
pub fn begin_drag_axis(
    drag: &mut Signal<DragState>,
    handle: ParamHandle,
    axis: DragAxis,
    start_x: f64,
    start_y: f64,
    sensitivity: f64,
) {
    let start_value = handle.normalized();
    handle.begin_edit();
    drag.set(DragState {
        active: true,
        handle: Some(handle),
        handle_y: None,
        axis,
        start_value,
        start_value_y: 0.0,
        start_x,
        start_y,
        sensitivity,
        move_count: 0,
        last_mult: 0.0,
    });
}

/// Begin a two-axis drag (XY pad): `handle_x` follows horizontal motion,
/// `handle_y` vertical. Both edits are bracketed.
pub fn begin_drag_xy(
    drag: &mut Signal<DragState>,
    handle_x: ParamHandle,
    handle_y: ParamHandle,
    start_x: f64,
    start_y: f64,
    sensitivity: f64,
) {
    let start_value = handle_x.normalized();
    let start_value_y = handle_y.normalized();
    handle_x.begin_edit();
    handle_y.begin_edit();
    drag.set(DragState {
        active: true,
        handle: Some(handle_x),
        handle_y: Some(handle_y),
        axis: DragAxis::Both,
        start_value,
        start_value_y,
        start_x,
        start_y,
        sensitivity,
        move_count: 0,
        last_mult: 0.0,
    });
}

/// Wrap your editor root in this. Captures `mousemove`/`mouseup` and feeds
/// them to whichever widget started a drag.
#[component]
pub fn DragProvider(children: Element) -> Element {
    let mut drag = use_signal(DragState::default);

    use_context_provider(|| drag);

    rsx! {
        div {
            style: "width:100vw; height:100vh;",

            // r[impl fx.control.capture]
            // r[impl fx.control.drag.axis]
            // r[impl fx.control.fine]
            // r[impl fx.control.bipolar]
            onmousemove: move |evt: MouseEvent| {
                let mut state = drag.read().clone();
                if !state.active {
                    return;
                }
                let Some(handle) = state.handle.clone() else { return };
                let pos = evt.client_coordinates();
                let mult = crate::gesture::fine_multiplier(evt.modifiers());

                // A modifier pressed or released mid-drag changes the ratio
                // from *here*: re-anchor at the current cursor and value so
                // the knob does not jump to where the new ratio would have
                // put it.
                if state.last_mult == 0.0 {
                    state.last_mult = mult;
                } else if mult != state.last_mult {
                    state.start_x = pos.x;
                    state.start_y = pos.y;
                    state.start_value = handle.normalized();
                    if let Some(hy) = &state.handle_y {
                        state.start_value_y = hy.normalized();
                    }
                    state.last_mult = mult;
                }

                let sens = state.sensitivity * mult;
                let delta = match state.axis {
                    DragAxis::Vertical => (state.start_y - pos.y) / sens,
                    DragAxis::Horizontal | DragAxis::Both => (pos.x - state.start_x) / sens,
                };
                if let (DragAxis::Both, Some(hy)) = (state.axis, &state.handle_y) {
                    let dy = (state.start_y - pos.y) / sens;
                    hy.set_normalized((state.start_value_y as f64 + dy).clamp(0.0, 1.0) as f32);
                }
                let mut new_val = (state.start_value as f64 + delta).clamp(0.0, 1.0);

                // Soft detent at a bipolar parameter's default (0 dB, centre):
                // within DETENT_PX of drag around it the value sticks, so
                // landing on it by hand is reliable. Fine drags skip it.
                if handle.is_bipolar() && mult == 1.0 {
                    let detent = handle.default_normalized() as f64;
                    let half = crate::gesture::DETENT_PX / state.sensitivity;
                    if (new_val - detent).abs() < half {
                        new_val = detent;
                    }
                }

                handle.set_normalized(new_val as f32);
                state.move_count += 1;
                drag.set(state);
            },

            // r[impl fx.control.capture]
            onmouseup: move |_| {
                let state = drag.read().clone();
                if state.active {
                    if let Some(handle) = &state.handle {
                        handle.end_edit();
                    }
                    if let Some(hy) = &state.handle_y {
                        hy.end_edit();
                    }
                    drag.set(DragState::default());
                }
            },

            {children}
        }
    }
}
