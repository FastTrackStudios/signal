//! ShaperBox-style pattern (MSEG) editor.
//!
//! A dumb, reusable curve editor over `signal_proto::modulation::
//! PatternPoint` (the macromod wire type): the host owns the point
//! list; every edit fires `on_change` with the full updated list.
//! Rendering + interaction math live in the framework-free
//! `pattern-ui` crate (backed by `fts-modulation`'s curve evaluator),
//! so the same shapes the engine plays are the shapes drawn here.
//!
//! Gestures (mirroring the EQ surface idiom):
//! - double-click empty space: add a point
//! - double-click a point: delete it (endpoints survive)
//! - drag a point: move (kept x-ordered; endpoints pin to x = 0/1)
//! - wheel over a selected point: segment tension
//! - rail buttons: curve type cycle, clear-tails toggle, delete

use dioxus::prelude::*;
use fts_modulation::{CurveType, Point};
use pattern_ui::{
    adjust_tension, build_pattern, constrained_move, nearest_point, next_curve_type, pattern_paths,
    PatternMapper,
};
use signal_proto::modulation::PatternPoint;

const H: f64 = 200.0;
const HIT_RADIUS: f64 = 12.0;

fn to_engine(points: &[PatternPoint]) -> Vec<Point> {
    points
        .iter()
        .map(|p| {
            let mut point = Point::new(f64::from(p.x), f64::from(p.y));
            point.tension = f64::from(p.tension);
            point.curve_type = CurveType::from_u8(p.curve_type);
            point.clear_tails = p.clear_tails;
            point
        })
        .collect()
}

fn curve_label(curve_type: u8) -> &'static str {
    match curve_type {
        0 => "Hold",
        1 => "Curve",
        2 => "S-Curve",
        3 => "Half Sine",
        4 => "Pulse",
        5 => "Wave",
        6 => "Triangle",
        7 => "Stairs",
        _ => "Sm. Stairs",
    }
}

/// Reusable drawn-curve editor. `phase` (0..1), when set, renders a
/// playhead so a running modulator can be watched live.
#[component]
pub fn PatternEditor(
    points: Vec<PatternPoint>,
    on_change: EventHandler<Vec<PatternPoint>>,
    #[props(default)] phase: Option<f64>,
    #[props(default = String::from("#7dd3fc"))] accent: String,
) -> Element {
    let mut selected = use_signal(|| None::<usize>);
    let mut dragging = use_signal(|| None::<usize>);
    let mut drag_rect = use_signal(|| None::<(f64, f64, f64, f64)>);
    let mut svg_el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let mut vb_w = use_signal(|| 480.0f64);

    let w = vb_w();
    let mapper = PatternMapper::new(w, H);
    let engine_points = to_engine(&points);
    let (stroke, fill) = pattern_paths(&build_pattern(&engine_points, 0.0), &mapper, 160);

    let sel = selected().filter(|&i| i < points.len());
    let sel_point = sel.and_then(|i| points.get(i).copied());

    let points_for_down = engine_points.clone();
    let points_for_move = points.clone();
    let points_for_dbl = points.clone();
    let points_for_wheel = points.clone();
    let points_for_rail = points.clone();

    rsx! {
        div { class: "relative flex flex-col h-full w-full min-h-0",
            svg {
                class: "w-full flex-1 min-h-0 touch-none select-none",
                view_box: "0 0 {w:.0} {H:.0}",
                preserve_aspect_ratio: "none",
                onmounted: move |e| {
                    let data = e.data();
                    svg_el.set(Some(data.clone()));
                    spawn(async move {
                        if let Ok(rect) = data.get_client_rect().await {
                            if rect.height() > 1.0 {
                                vb_w.set((H * rect.width() / rect.height()).max(120.0));
                            }
                        }
                    });
                },
                onresize: move |e| {
                    if let Ok(size) = e.get_content_box_size() {
                        if size.height > 1.0 {
                            vb_w.set((H * size.width / size.height).max(120.0));
                        }
                    }
                },

                // Add on empty double-click; delete on point double-click.
                ondoubleclick: {
                    let on_change = on_change;
                    move |e: MouseEvent| {
                        let coords = e.element_coordinates();
                        let el = svg_el();
                        let pts = points_for_dbl.clone();
                        spawn(async move {
                            let Some(el) = el else { return };
                            let Ok(rect) = el.get_client_rect().await else { return };
                            let px = coords.x / rect.width() * w;
                            let py = coords.y / rect.height() * H;
                            let engine = to_engine(&pts);
                            let mut next = pts.clone();
                            match nearest_point(&engine, &mapper, px, py, HIT_RADIUS) {
                                // Endpoints stay — the cycle needs its rails.
                                Some(i) if i > 0 && i + 1 < next.len() => {
                                    next.remove(i);
                                    selected.set(None);
                                }
                                Some(_) => return,
                                None => {
                                    let mut p = PatternPoint::new(
                                        mapper.px_to_x(px) as f32,
                                        mapper.px_to_y(py) as f32,
                                    );
                                    // Inherit the segment's curve type.
                                    if let Some(prev) = pts
                                        .iter()
                                        .rev()
                                        .find(|q| f64::from(q.x) <= mapper.px_to_x(px))
                                    {
                                        p.curve_type = prev.curve_type;
                                    }
                                    next.push(p);
                                    next.sort_by(|a, b| a.x.total_cmp(&b.x));
                                    let added = next
                                        .iter()
                                        .position(|q| q.x == p.x && q.y == p.y);
                                    selected.set(added);
                                }
                            }
                            on_change.call(next);
                        });
                    }
                },

                onpointerdown: move |e: PointerEvent| {
                    let coords = e.element_coordinates();
                    let el = svg_el();
                    let engine = points_for_down.clone();
                    spawn(async move {
                        let Some(el) = el else { return };
                        let Ok(rect) = el.get_client_rect().await else { return };
                        drag_rect.set(Some((
                            rect.origin.x,
                            rect.origin.y,
                            rect.width(),
                            rect.height(),
                        )));
                        let px = coords.x / rect.width() * w;
                        let py = coords.y / rect.height() * H;
                        match nearest_point(&engine, &mapper, px, py, HIT_RADIUS) {
                            Some(i) => {
                                selected.set(Some(i));
                                dragging.set(Some(i));
                            }
                            None => selected.set(None),
                        }
                    });
                },

                // Tension on the wheel (selected or dragged point).
                onwheel: {
                    let on_change = on_change;
                    move |e: WheelEvent| {
                        let Some(i) = dragging().or(selected()) else { return };
                        let mut next = points_for_wheel.clone();
                        let Some(p) = next.get_mut(i) else { return };
                        p.tension = adjust_tension(
                            f64::from(p.tension),
                            -e.delta().strip_units().y.signum() * 2.0,
                        ) as f32;
                        on_change.call(next);
                    }
                },

                // Grid: quarters both ways.
                for i in 1..4u32 {
                    line {
                        key: "gv{i}",
                        x1: "{mapper.x_to_px(i as f64 / 4.0):.1}",
                        y1: "{mapper.y_to_px(1.0):.1}",
                        x2: "{mapper.x_to_px(i as f64 / 4.0):.1}",
                        y2: "{mapper.y_to_px(0.0):.1}",
                        stroke: "#27272a",
                        stroke_width: "1",
                    }
                    line {
                        x1: "{mapper.x_to_px(0.0):.1}",
                        y1: "{mapper.y_to_px(i as f64 / 4.0):.1}",
                        x2: "{mapper.x_to_px(1.0):.1}",
                        y2: "{mapper.y_to_px(i as f64 / 4.0):.1}",
                        stroke: "#27272a",
                        stroke_width: "1",
                    }
                }

                // The drawn curve.
                path { d: "{fill}", fill: "{accent}18", stroke: "none" }
                path { d: "{stroke}", fill: "none", stroke: "{accent}", stroke_width: "2" }

                // Playhead.
                if let Some(ph) = phase {
                    line {
                        x1: "{mapper.x_to_px(ph.rem_euclid(1.0)):.1}",
                        y1: "{mapper.y_to_px(1.0):.1}",
                        x2: "{mapper.x_to_px(ph.rem_euclid(1.0)):.1}",
                        y2: "{mapper.y_to_px(0.0):.1}",
                        stroke: "#fafafa",
                        stroke_opacity: "0.6",
                        stroke_width: "1",
                        pointer_events: "none",
                    }
                }

                // Handles.
                for (i, p) in engine_points.iter().enumerate() {
                    {
                        let cx = mapper.x_to_px(p.x);
                        let cy = mapper.y_to_px(p.y);
                        let is_sel = sel == Some(i);
                        let clear = p.clear_tails;
                        rsx! {
                            circle {
                                key: "p{i}",
                                cx: "{cx:.1}", cy: "{cy:.1}",
                                r: if is_sel { "8" } else { "6" },
                                fill: if clear { "#f87171" } else { "{accent}" },
                                fill_opacity: "0.85",
                                stroke: if is_sel { "#ffffff" } else { "{accent}" },
                                stroke_width: if is_sel { "2" } else { "1" },
                                class: "cursor-grab",
                            }
                        }
                    }
                }
            }

            // Drag shield.
            if dragging().is_some() {
                div {
                    class: "fixed inset-0",
                    style: "z-index: 1000; cursor: grabbing;",
                    onpointermove: {
                        let on_change = on_change;
                        move |e: PointerEvent| {
                            let Some(i) = dragging() else { return };
                            let Some((ox, oy, w_px, h_px)) = drag_rect() else { return };
                            let c = e.client_coordinates();
                            let px = (c.x - ox) / w_px * w;
                            let py = (c.y - oy) / h_px * H;
                            let mut next = points_for_move.clone();
                            let engine = to_engine(&next);
                            let (x, y) = constrained_move(
                                &engine,
                                i,
                                mapper.px_to_x(px),
                                mapper.px_to_y(py),
                            );
                            if let Some(p) = next.get_mut(i) {
                                p.x = x as f32;
                                p.y = y as f32;
                                on_change.call(next);
                            }
                        }
                    },
                    onpointerup: move |_| dragging.set(None),
                }
            }

            // Selected-point rail.
            if let Some(p) = sel_point {
                div {
                    class: "absolute bottom-1 left-1 z-10 flex items-center gap-1 rounded bg-zinc-900/90 px-1.5 py-1 text-[10px] text-zinc-300",
                    button {
                        class: "rounded bg-zinc-800 px-1.5 py-0.5 hover:bg-zinc-700",
                        onclick: {
                            let on_change = on_change;
                            let pts = points_for_rail.clone();
                            move |_| {
                                let Some(i) = sel else { return };
                                let mut next = pts.clone();
                                if let Some(q) = next.get_mut(i) {
                                    q.curve_type =
                                        next_curve_type(CurveType::from_u8(q.curve_type)) as u8;
                                    on_change.call(next);
                                }
                            }
                        },
                        "{curve_label(p.curve_type)}"
                    }
                    button {
                        class: if p.clear_tails {
                            "rounded bg-red-900/70 px-1.5 py-0.5 text-red-200 hover:bg-red-800"
                        } else {
                            "rounded bg-zinc-800 px-1.5 py-0.5 hover:bg-zinc-700"
                        },
                        onclick: {
                            let on_change = on_change;
                            let pts = points_for_rail.clone();
                            move |_| {
                                let Some(i) = sel else { return };
                                let mut next = pts.clone();
                                if let Some(q) = next.get_mut(i) {
                                    q.clear_tails = !q.clear_tails;
                                    on_change.call(next);
                                }
                            }
                        },
                        "Clear tails"
                    }
                    span { class: "text-zinc-500", "tension {p.tension:+.2}" }
                    if sel.is_some_and(|i| i > 0 && i + 1 < points.len()) {
                        button {
                            class: "rounded bg-zinc-800 px-1.5 py-0.5 text-red-300 hover:bg-red-900/60",
                            onclick: {
                                let on_change = on_change;
                                let pts = points_for_rail.clone();
                                move |_| {
                                    let Some(i) = sel else { return };
                                    let mut next = pts.clone();
                                    next.remove(i);
                                    selected.set(None);
                                    on_change.call(next);
                                }
                            },
                            "Delete"
                        }
                    }
                }
            }
        }
    }
}
