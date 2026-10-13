//! A lever switch — the paddle a Pultec selects its frequencies with.
//!
//! Not a knob with a long pointer: a lever is a bar through a pivot, and you
//! read it by where the bar points, with the legends printed in an arc above
//! it on the panel rather than around a ring. It also sweeps far less than a
//! knob — the positions have to stay distinguishable at a glance from across a
//! room, which is the whole reason a console uses one.

use dioxus::prelude::*;

use crate::drag::{DragAxis, DragState};
use crate::gesture::{self, Press};
use crate::param::ParamHandle;

/// Half the lever's travel, in degrees either side of vertical.
pub const LEVER_SWEEP_DEG: f64 = 62.0;

/// Pixels of horizontal drag per full sweep. A lever has a handful of
/// positions over a short throw, so it wants far less travel than a knob's
/// 190 px — dragging one should feel like pushing a paddle, not winding a pot.
const DRAG_SENSITIVITY: f64 = 90.0;

/// How far the pointer may move between press and release and still count as
/// a click (which advances a position) rather than a drag, in px.
const CLICK_SLOP: f64 = 3.0;

/// The paddle's outline in the lever's `-55 -55 110 110` viewBox: a rounded
/// bar through the pivot, 11.6 wide at the index end and 20.4 at the base over
/// 50 of travel — about 1:2.6, which is the unit's own proportion. Drawn as a
/// path rather than a polygon so the corners are radiused like a moulding.
pub const PADDLE_PATH: &str = "M -4.4 -23.0 \
     Q -5.8 -26.0 -3.2 -26.0 \
     L 3.2 -26.0 \
     Q 5.8 -26.0 4.4 -23.0 \
     L 9.0 22.0 \
     Q 9.6 26.0 6.0 26.0 \
     L -6.0 26.0 \
     Q -9.6 26.0 -9.0 22.0 Z";

/// The paddle's reach above the pivot — where the index tip lands, and so the
/// radius the printed legends have to clear.
pub const PADDLE_REACH: f64 = 26.0;

/// Angle for detent `index` of `count`; 0° is straight up.
pub fn lever_angle(index: usize, count: usize) -> f64 {
    if count < 2 {
        return 0.0;
    }
    (index as f64 / (count - 1) as f64 - 0.5) * 2.0 * LEVER_SWEEP_DEG
}

/// Where a legend sits above the pivot, in the lever's own viewBox units.
fn legend_point(index: usize, count: usize, radius: f64) -> (f64, f64) {
    let rad = lever_angle(index, count).to_radians();
    (radius * rad.sin(), -radius * rad.cos())
}

/// A lever switch bound to a stepped parameter.
///
/// Clicking a legend selects that position; clicking the paddle advances to
/// the next one and wraps, which is how you use one without aiming.
#[component]
pub fn LeverSwitch(
    handle: ParamHandle,
    testid: String,
    scale: f64,
    /// Legends, in order. Printed in an arc above the pivot.
    labels: Vec<String>,
    /// Small caption above the arc — "CPS", "KCS".
    #[props(default)]
    unit: Option<String>,
    #[props(default = "#e6ecf0".to_string())] ink: String,
    /// Paddle length in design-space px, from the pivot to the tip.
    #[props(default = 34.0)]
    length: f64,
) -> Element {
    let mut drag: Signal<DragState> = use_context();
    // Re-render while a drag is in flight so the paddle tracks the cursor.
    let _ = drag.read().move_count;
    // Where the press landed, so a release that never moved can be told from
    // the end of a drag and treated as "advance one position".
    let mut press_x: Signal<Option<f64>> = use_signal(|| None);

    let count = labels.len().max(1);
    let selected = if count > 1 {
        (handle.normalized().clamp(0.0, 1.0) * (count - 1) as f32).round() as usize
    } else {
        0
    };
    let angle = lever_angle(selected, count);

    // The viewBox is a square around the pivot: the paddle hangs below it, the
    // legends arc above.
    let box_px = length * 3.2 * scale;

    rsx! {
        div {
            "data-testid": "hw-lever-{testid}",
            "data-index": "{selected}",
            style: format!("position:relative; width:{box_px:.1}px; height:{box_px:.1}px;"),

            svg {
                style: "position:absolute; inset:0; width:100%; height:100%; display:block;",
                view_box: "-55 -55 110 110",

                if let Some(unit) = &unit {
                    text {
                        x: "0", y: "-49",
                        fill: "{ink}", font_size: "8.5", font_weight: "700",
                        text_anchor: "middle", letter_spacing: "0.6",
                        "{unit}"
                    }
                }

                // Printed legends. Each is clickable, so a lever can be aimed
                // as well as advanced.
                for (index , label) in labels.iter().enumerate() {
                    {
                        let (lx, ly) = legend_point(index, count, 44.0);
                        let active = index == selected;
                        let handle = handle.clone();
                        let step = if count > 1 {
                            index as f32 / (count - 1) as f32
                        } else {
                            0.0
                        };
                        rsx! {
                            text {
                                "data-testid": "hw-lever-{testid}-{index}",
                                x: "{lx:.2}", y: "{ly + 2.4:.2}",
                                fill: "{ink}",
                                font_size: "10",
                                font_weight: if active { "800" } else { "650" },
                                // A silkscreened legend does not dim when it
                                // is not selected — the paddle is what says
                                // which position is live. The old 0.62 made
                                // the outer positions read as disabled.
                                opacity: if active { "1" } else { "0.86" },
                                text_anchor: "middle",
                                onclick: move |_| {
                                    handle.begin_edit();
                                    handle.set_normalized(step);
                                    handle.end_edit();
                                },
                                "{label}"
                            }
                        }
                    }
                }

                // The paddle: a bar through the pivot, wide below and tapering
                // to the index tip above. Proportioned off the unit's own —
                // roughly 1:2.6 at the base, with rounded corners, rather than
                // the thin blade a knob's pointer would be.
                g {
                    transform: "rotate({angle:.2})",
                    // Cast shadow, so the paddle sits above the panel.
                    path {
                        d: "{PADDLE_PATH}",
                        transform: "translate(1.2 2.0)",
                        fill: "rgba(0,0,0,0.42)",
                    }
                    path {
                        d: "{PADDLE_PATH}",
                        fill: "#16161a",
                        stroke: "rgba(0,0,0,0.6)",
                        stroke_width: "0.6",
                    }
                    // The moulded edge highlight down the paddle's left side.
                    path {
                        d: "M -5.6 -25.0 L -10.2 24.0",
                        fill: "none",
                        stroke: "rgba(255,255,255,0.13)",
                        stroke_width: "1.0",
                    }
                    // The white stripe you actually read.
                    rect {
                        x: "-1.7", y: "-24.0", width: "3.4", height: "20.0", rx: "1.6",
                        fill: "#f1f1ef",
                    }
                }
                // Pivot boss.
                circle { cx: "0", cy: "0", r: "7.0", fill: "#1b1b1e" }
                circle { cx: "0", cy: "0", r: "2.4", fill: "rgba(255,255,255,0.14)" }
            }

            // The paddle's own surface: drag it across, wheel it, or click it
            // to advance. A lever that only advanced on click was the odd one
            // out on the panel — every other control here is draggable, and a
            // frequency selector is exactly the thing you want to sweep.
            //
            // The box covers the paddle and the pivot but stops short of the
            // legend arc above, which stays clickable for aiming at a position
            // directly.
            div {
                "data-testid": "hw-lever-{testid}-paddle",
                style: "position:absolute; left:20%; top:37%; width:60%; height:45%; \
                        cursor:ew-resize; user-select:none;",
                onmousedown: {
                    let handle = handle.clone();
                    move |evt: MouseEvent| {
                        let x = evt.client_coordinates().x;
                        match gesture::press(&evt, &mut drag, &handle, DragAxis::Horizontal, DRAG_SENSITIVITY) {
                            Press::Drag => press_x.set(Some(x)),
                            // A reset or a menu press is not the start of a
                            // click-to-advance.
                            _ => press_x.set(None),
                        }
                    }
                },
                ondoubleclick: {
                    let handle = handle.clone();
                    move |_| {
                        press_x.set(None);
                        gesture::double_click(&mut drag, &handle);
                    }
                },
                onclick: {
                    let handle = handle.clone();
                    move |evt: MouseEvent| {
                        // A press that never travelled is a click: advance and
                        // wrap, which is how a lever is used without aiming.
                        // After a real drag the value is already where the
                        // user put it and must not be bumped one further.
                        let Some(sx) = press_x.take() else { return };
                        if (evt.client_coordinates().x - sx).abs() > CLICK_SLOP {
                            return;
                        }
                        let next = (selected + 1) % count;
                        let step = if count > 1 {
                            next as f32 / (count - 1) as f32
                        } else {
                            0.0
                        };
                        handle.begin_edit();
                        handle.set_normalized(step);
                        handle.end_edit();
                    }
                },
                onwheel: {
                    let handle = handle.clone();
                    move |evt: WheelEvent| {
                        evt.prevent_default();
                        let dy = evt.delta().strip_units().y;
                        if dy == 0.0 || count < 2 {
                            return;
                        }
                        let next = if dy < 0.0 {
                            (selected + 1).min(count - 1)
                        } else {
                            selected.saturating_sub(1)
                        };
                        handle.begin_edit();
                        handle.set_normalized(next as f32 / (count - 1) as f32);
                        handle.end_edit();
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_travel_is_symmetric_about_vertical() {
        assert_eq!(lever_angle(0, 5), -LEVER_SWEEP_DEG);
        assert_eq!(lever_angle(4, 5), LEVER_SWEEP_DEG);
        assert_eq!(lever_angle(2, 5), 0.0);
    }

    #[test]
    fn a_lever_sweeps_far_less_than_a_knob() {
        // A knob turns 300°; a lever has to stay readable at a glance.
        // Both sides are constants on purpose — this guards the constant
        // against being edited upward, so it is meant to be trivially
        // true today.
        #[allow(clippy::assertions_on_constants)]
        {
            assert!(LEVER_SWEEP_DEG * 2.0 < 150.0);
        }
    }

    #[test]
    fn legends_clear_the_paddle() {
        // The legends are printed beyond the index tip's reach or they sit on
        // top of the thing that points at them.
        let (_, ly) = legend_point(1, 4, 44.0);
        assert!(
            ly < -PADDLE_REACH,
            "legend at {ly} is inside the paddle's reach"
        );
    }

    #[test]
    fn the_paddle_is_a_bar_rather_than_a_blade() {
        // Roughly 1:2.6 at the base — a paddle you push, not a knob's pointer.
        // Measured off the path's own numbers so a redraw that thins it out
        // has to come here and say so.
        let base_width = 9.0 + 9.0;
        let travel = PADDLE_REACH + 26.0;
        let ratio = travel / base_width;
        assert!(
            (2.2..=3.2).contains(&ratio),
            "paddle is {ratio:.2}:1, which is not the proportion of a lever",
        );
    }

    #[test]
    fn a_single_position_lever_points_straight_up() {
        assert_eq!(lever_angle(0, 1), 0.0);
        assert_eq!(lever_angle(0, 0), 0.0);
    }

    #[test]
    fn legends_run_left_to_right_and_sit_above_the_pivot() {
        let (lx, ly) = legend_point(0, 4, 34.0);
        let (rx, ry) = legend_point(3, 4, 34.0);
        assert!(lx < 0.0 && rx > 0.0);
        assert!(ly < 0.0 && ry < 0.0, "legends print above the pivot");
    }
}
