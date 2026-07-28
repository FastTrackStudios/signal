//! Console fader — the keys rig's primary control (where the guitar rig
//! reaches for a knob, a keys mixer reaches for a fader).
//!
//! Drag feel matches the guitar `Knob`: pointer-down captures, a fullscreen
//! shield owns the drag, 150 px of travel per full sweep. The scale is
//! console-like: unity sits high, the taper opens up around 0 dB where the
//! playing decisions actually happen.

use dioxus::prelude::*;

/// Fader travel (dB) — mirrors the backend's clamp.
pub const MIN_DB: f32 = -60.0;
pub const MAX_DB: f32 = 6.0;
/// Pixels of drag for the full sweep.
use signal_widgets::arc::SENSITIVITY;

/// dB → 0..1 fader position, with the taper opened up near unity.
pub fn db_to_pos(db: f32) -> f32 {
    let t = (db - MIN_DB) / (MAX_DB - MIN_DB);
    t.clamp(0.0, 1.0).powf(0.6)
}

/// 0..1 fader position → dB.
pub fn pos_to_db(pos: f32) -> f32 {
    let t = pos.clamp(0.0, 1.0).powf(1.0 / 0.6);
    MIN_DB + t * (MAX_DB - MIN_DB)
}

/// Format a fader value the way a console does.
pub fn fmt_db(db: f32) -> String {
    if db <= MIN_DB {
        "−∞".into()
    } else {
        format!("{db:+.1}")
    }
}

/// **The card's left edge, as a fader.** Absolutely positioned, so it fills
/// whatever card it is dropped into: the engine's level is not a control
/// sitting in the card, it *is* the card's edge, and its height is the card's
/// height. Drag feel matches [`Fader`] — 150 px per full sweep.
///
/// The parent must be `position: relative` and leave room on the left.
#[component]
pub fn EdgeFader(
    db: f32,
    #[props(default = 9)] width_px: u32,
    /// Matches the card's corner radius so the edge is the outline.
    #[props(default = 12)] radius_px: u32,
    #[props(default = "#38bdf8".to_string())] accent: String,
    #[props(default = false)] dimmed: bool,
    on_change: EventHandler<f32>,
) -> Element {
    let mut drag = use_signal(|| None::<(f64, f32)>);
    let send = crate::throttle::use_throttle(on_change);

    let pos = db_to_pos(db);
    let pct = pos * 100.0;
    let unity = db_to_pos(0.0) * 100.0;
    let opacity = if dimmed { "0.4" } else { "1" };

    rsx! {
        div {
            style: "position: absolute; left: 0; top: 0; bottom: 0; width: {width_px}px; \
                    border-radius: {radius_px}px 0 0 {radius_px}px; background: #131316; \
                    border-right: 1px solid #1f1f23; overflow: hidden; cursor: ns-resize; \
                    opacity: {opacity}; touch-action: none;",
            title: "{fmt_db(db)} dB",
            onpointerdown: move |e: PointerEvent| {
                drag.set(Some((e.client_coordinates().y, pos)));
            },
            div {
                style: "position: absolute; left: 0; right: 0; bottom: 0; height: {pct}%; \
                        background: linear-gradient(180deg, {accent}, #1e3a5f);",
            }
            // Unity mark — the level the patch was saved at.
            div {
                style: "position: absolute; left: 0; right: 0; bottom: {unity}%; height: 1px; \
                        background: #52525b;",
            }
        }
        if drag().is_some() {
            div {
                style: "position: fixed; inset: 0; z-index: 999; cursor: ns-resize;",
                onpointermove: move |e: PointerEvent| {
                    if let Some((y0, p0)) = drag() {
                        let dy = y0 - e.client_coordinates().y;
                        let next = (p0 as f64 + dy / SENSITIVITY).clamp(0.0, 1.0) as f32;
                        send.queue(pos_to_db(next));
                    }
                },
                onpointerup: move |_| {
                    send.flush();
                    drag.set(None);
                },
                onpointerleave: move |_| {
                    send.flush();
                    drag.set(None);
                },
            }
        }
    }
}

/// A vertical fader. `height_px` sizes the track; the cap and fill follow.
#[component]
pub fn Fader(
    /// Current value in dB.
    db: f32,
    /// Track height in px.
    #[props(default = 120)]
    height_px: u32,
    /// Track width. The default is the console strip; the engine's own fader
    /// runs slim down the edge of its card, where it is a trim you set once
    /// rather than a lane you ride.
    #[props(default = 26)]
    width_px: u32,
    /// Accent color for the fill + cap.
    #[props(default = "#38bdf8".to_string())]
    accent: String,
    /// Dimmed rendering (muted lane).
    #[props(default = false)]
    dimmed: bool,
    /// Show the dB readout under the track.
    #[props(default = true)]
    show_value: bool,
    on_change: EventHandler<f32>,
) -> Element {
    // (start_y, start_pos) while a drag is live.
    let mut drag = use_signal(|| None::<(f64, f32)>);
    let send = crate::throttle::use_throttle(on_change);

    let pos = db_to_pos(db);
    let h = height_px as f32;
    let fill_h = (pos * h).round() as i32;
    let cap_bottom = (pos * h).round() as i32 - 6;
    let opacity = if dimmed { "0.45" } else { "1" };

    rsx! {
        div {
            style: "position: relative; display: flex; flex-direction: column; align-items: center; \
                    gap: 4px; opacity: {opacity}; touch-action: none;",
            // Track + fill + cap.
            div {
                style: "position: relative; width: {width_px}px; height: {height_px}px; border-radius: 4px; \
                        background: #131316; border: 1px solid #26262b; cursor: ns-resize;",
                onpointerdown: move |e: PointerEvent| {
                    drag.set(Some((e.client_coordinates().y, pos)));
                },
                // Unity mark.
                {
                    let unity = (db_to_pos(0.0) * h).round() as i32;
                    rsx! {
                        div {
                            style: "position: absolute; left: 2px; right: 2px; bottom: {unity}px; \
                                    height: 1px; background: #3f3f46;",
                        }
                    }
                }
                div {
                    style: "position: absolute; left: 3px; right: 3px; bottom: 0; height: {fill_h}px; \
                            border-radius: 3px; background: linear-gradient(180deg, {accent}, #1e3a5f);",
                }
                div {
                    style: "position: absolute; left: -3px; right: -3px; bottom: {cap_bottom}px; \
                            height: 12px; border-radius: 3px; background: linear-gradient(180deg, #4a4a52, #232328); \
                            border: 1px solid #55555f; box-shadow: 0 1px 3px #000a;",
                }
            }
            if show_value {
                span {
                    style: "font-size: 9px; font-variant-numeric: tabular-nums; color: #a1a1aa; min-width: 34px; text-align: center;",
                    {fmt_db(db)}
                }
            }
            // Drag shield: while dragging, a fullscreen layer owns the pointer
            // so the value keeps tracking outside the fader's bounds.
            if drag().is_some() {
                div {
                    style: "position: fixed; inset: 0; z-index: 999; cursor: ns-resize;",
                    onpointermove: move |e: PointerEvent| {
                        if let Some((y0, p0)) = drag() {
                            let dy = y0 - e.client_coordinates().y;
                            let next = (p0 as f64 + dy / SENSITIVITY).clamp(0.0, 1.0) as f32;
                            send.queue(pos_to_db(next));
                        }
                    },
                    onpointerup: move |_| {
                        send.flush();
                        drag.set(None);
                    },
                    onpointerleave: move |_| {
                        send.flush();
                        drag.set(None);
                    },
                }
            }
        }
    }
}
