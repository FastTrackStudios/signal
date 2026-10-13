//! Everyone else's mouse, anywhere in the window.
//!
//! The arrangement draws a pointer over its lanes itself — as a time and a
//! track, which is finer than any screen position. Everywhere else a
//! pointer travels as a place in a named panel (`chart`, `lyrics`,
//! `panels`, `editor`, or `window`), and this overlay puts it on the same
//! place of the same panel here, whatever size this window is. It also
//! reports this peer's own mouse the same way ([`crate::ghosts`]).

use std::time::Duration;

use dioxus::prelude::*;

/// Draw the others' pointers over the whole window. Mount once, as the
/// last child of the window's root.
#[component]
pub fn CollabPointers() -> Element {
    let window = dioxus_native::use_window();
    let mut drawn = use_signal(Vec::<(f64, f64, String, u32)>::new);

    let win = window.clone();
    dioxus_native::use_window_event(move |event, _| match event {
        winit::event::WindowEvent::PointerMoved { position, .. } => {
            let scale = win.scale_factor();
            let size = win.surface_size();
            crate::ghosts::local_window_pointer(
                position.x / scale,
                position.y / scale,
                (
                    f64::from(size.width) / scale,
                    f64::from(size.height) / scale,
                ),
            );
        }
        winit::event::WindowEvent::PointerLeft { .. } => crate::ghosts::local_window_left(),
        _ => {}
    });

    use_future(move || {
        let window = window.clone();
        async move {
            let mut tick = 0u32;
            loop {
                futures_timer::Delay::new(Duration::from_millis(33)).await;
                // Panels move with the window and the layout: measured
                // twice a second, which is often enough for a hand to not
                // notice and rare enough to cost nothing.
                if tick % 15 == 0 {
                    crate::ghosts::measure_regions().await;
                }
                tick = tick.wrapping_add(1);
                let scale = window.scale_factor();
                let size = window.surface_size();
                // Test mode: a mouse that moves by itself along a slow path
                // over the whole window, through the same anchoring a real
                // one goes through.
                if crate::collab::env_set("FTS_COLLAB_PUPPET_MOUSE") {
                    let (w, h) = (
                        f64::from(size.width) / scale,
                        f64::from(size.height) / scale,
                    );
                    let t = crate::ghosts::now_ms() / 1000.0;
                    let x = w * (0.5 + 0.45 * (t * 0.23).sin());
                    let y = h * (0.5 + 0.45 * (t * 0.37).sin());
                    crate::ghosts::local_window_pointer(x, y, (w, h));
                }
                let now = crate::ghosts::window_pointers((
                    f64::from(size.width) / scale,
                    f64::from(size.height) / scale,
                ));
                if *drawn.peek() != now {
                    drawn.set(now);
                }
            }
        }
    });

    rsx! {
        div {
            style: "position:absolute; top:0; left:0; right:0; bottom:0; pointer-events:none; z-index:1000;",
            for (x, y, name, color) in drawn() {
                Arrow { x, y, name, color }
            }
        }
    }
}

#[component]
fn Arrow(x: f64, y: f64, name: String, color: u32) -> Element {
    let (r, g, b) = ((color >> 16) & 0xff, (color >> 8) & 0xff, color & 0xff);
    let fill = format!("rgb({r},{g},{b})");
    rsx! {
        div {
            style: "position:absolute; left:{x}px; top:{y}px;",
            svg {
                width: "14",
                height: "18",
                view_box: "0 0 14 18",
                path {
                    d: "M1 1 L1 15 L5 11.5 L10.5 12 Z",
                    fill: "{fill}",
                    stroke: "rgba(255,255,255,0.7)",
                    stroke_width: "1",
                }
            }
            if !name.is_empty() {
                div {
                    style: "position:absolute; left:11px; top:13px; padding:1px 5px; border-radius:3px; \
                            background:{fill}; color:#101114; font-size:10px; font-weight:600; \
                            white-space:nowrap; font-family:system-ui, sans-serif;",
                    "{name}"
                }
            }
        }
    }
}
