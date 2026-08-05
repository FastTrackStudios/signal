//! The faceplate itself — chrome, and the one scale factor everything on it
//! is drawn at.
//!
//! [`Panel`] is the root of a hardware face: it measures the editor window,
//! works out the [`fit_scale`](crate::hardware::panel_svg::fit_scale) for its
//! design size, and hands that factor to its children, which multiply their
//! own pixel sizes by it. Nothing on a panel reflows — the drawing is fixed
//! and only its size changes, which is what keeps it reading as one object.
//!
//! The window size comes from the reactive `Signal<(u32, u32)>` that
//! `nice-plug-dioxus` puts in context on resize. It is absent in the headless
//! test harness and in any non-plugin mount, so the panel falls back to its
//! design size and draws unscaled.

use audiocore_core::prelude::*;

use crate::hardware::panel_svg::fit_scale;

/// Editor size for a mount that is not a `nice-plug` window.
///
/// The plugin and standalone paths both get a reactive `Signal<(u32, u32)>`
/// from `nice-plug-dioxus`, and that is what the panel normally measures. A
/// bare `VirtualDom` mount — the headless test harness — has no window at all,
/// so an embedder can state the size it is rendering at instead.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EditorSize(pub f64, pub f64);

/// Logical window size, if anything in context knows it.
pub fn window_logical_size() -> Option<(f64, f64)> {
    if let Some(sig) = try_consume_context::<Signal<(u32, u32)>>() {
        let (w, h) = *sig.read();
        return Some((w as f64, h as f64));
    }
    try_consume_context::<EditorSize>().map(|EditorSize(w, h)| (w, h))
}

/// The scale a `design_w` x `design_h` panel is drawn at in the current editor
/// window, with `reserve_w` px of window width already spent beside it (the
/// shell rail) — without that the panel is sized for space it does not have.
///
/// Faces call this themselves rather than letting [`Panel`] do it privately:
/// every control on the panel needs the same factor, and children are built
/// before the parent renders them.
pub fn panel_scale(design_w: f64, design_h: f64, reserve_w: f64) -> f64 {
    let (win_w, win_h) = window_logical_size().unwrap_or((design_w + reserve_w, design_h));
    fit_scale((win_w - reserve_w).max(1.0), win_h, design_w, design_h)
}

/// A hardware faceplate.
///
/// `design_w` / `design_h` are the panel's drawing size in CSS px at scale 1;
/// `scale` comes from [`panel_scale`].
#[component]
pub fn Panel(
    design_w: f64,
    design_h: f64,
    scale: f64,
    /// The faceplate's own background — the cream leveling-amplifier paint,
    /// the black FET panel. A full CSS background value.
    background: String,
    /// Colour of the rack ears and screws around the panel.
    #[props(default = "#b9b4a8".to_string())] chrome: String,
    /// Rendered inside the panel, positioned absolutely in design space.
    children: Element,
) -> Element {
    let (w, h) = (design_w * scale, design_h * scale);

    rsx! {
        div {
            "data-testid": "panel-frame",
            // The panel is centred in whatever space is left, with the surplus
            // as margin rather than as stretched chrome.
            style: "flex:1; min-height:0; display:flex; align-items:center; \
                    justify-content:center; overflow:hidden; background:#0b0b0d;",

            div {
                "data-testid": "hardware-panel",
                "data-scale": "{scale:.4}",
                style: format!(
                    "position:relative; width:{w:.1}px; height:{h:.1}px; \
                     background:{background}; border-radius:{:.1}px; \
                     border:{:.1}px solid rgba(0,0,0,0.55); \
                     box-shadow:0 {:.1}px {:.1}px rgba(0,0,0,0.5);",
                    3.0 * scale,
                    1.0 * scale,
                    4.0 * scale,
                    18.0 * scale,
                ),

                // Rack ears: a screwed strip down each side, the detail that
                // reads "this came out of a rack" more than anything else.
                RackEar { scale, chrome: chrome.clone(), left: true, height: design_h }
                RackEar { scale, chrome: chrome.clone(), left: false, height: design_h }

                {children}
            }
        }
    }
}

/// One screwed rack ear, on the left or right edge of the panel.
#[component]
fn RackEar(scale: f64, chrome: String, left: bool, height: f64) -> Element {
    let w = 26.0 * scale;
    let side = if left { "left:0;" } else { "right:0;" };
    rsx! {
        div {
            style: format!(
                "position:absolute; top:0; bottom:0; {side} width:{w:.1}px; \
                 background:linear-gradient(90deg, rgba(255,255,255,0.10), \
                 rgba(0,0,0,0.16)); border-{}:1px solid rgba(0,0,0,0.28); \
                 display:flex; flex-direction:column; align-items:center; \
                 justify-content:space-between; padding:{:.1}px 0;",
                if left { "right" } else { "left" },
                10.0 * scale,
            ),
            Screw { scale, chrome: chrome.clone() }
            // Tall panels get a third screw, like the real 2U units.
            if height > 260.0 {
                Screw { scale, chrome: chrome.clone() }
            }
            Screw { scale, chrome }
        }
    }
}

/// A panel screw — a slotted head, drawn small enough to be texture.
#[component]
fn Screw(scale: f64, chrome: String) -> Element {
    let d = 9.0 * scale;
    rsx! {
        div {
            style: format!(
                "width:{d:.1}px; height:{d:.1}px; border-radius:50%; \
                 background:radial-gradient(circle at 35% 30%, {chrome}, rgba(0,0,0,0.7)); \
                 box-shadow:inset 0 0 {:.1}px rgba(0,0,0,0.6); \
                 display:flex; align-items:center; justify-content:center;",
                2.0 * scale,
            ),
            div {
                style: format!(
                    "width:{:.1}px; height:{:.1}px; background:rgba(0,0,0,0.55);",
                    d * 0.62,
                    1.2 * scale,
                ),
            }
        }
    }
}

/// Silkscreened panel text — a control legend, a model number, a brand line.
///
/// Placed by the centre of a `width`-wide box, not by a CSS transform: the
/// panel is drawn at an arbitrary scale and every offset has to be arithmetic
/// we control, not layout we hope matches.
#[component]
pub fn Silkscreen(
    scale: f64,
    /// Centre of the text in design-space px from the panel's top-left.
    x: f64,
    y: f64,
    text: String,
    #[props(default = 160.0)] width: f64,
    #[props(default = 11.0)] size: f64,
    #[props(default = "#2b2620".to_string())] color: String,
    #[props(default = 0.14)] tracking: f64,
    #[props(default = 700)] weight: u32,
) -> Element {
    rsx! {
        div {
            style: format!(
                "position:absolute; left:{:.1}px; top:{:.1}px; width:{:.1}px; \
                 text-align:center; white-space:nowrap; \
                 font-size:{:.1}px; font-weight:{weight}; color:{color}; \
                 letter-spacing:{:.2}px; text-transform:uppercase; \
                 pointer-events:none;",
                (x - width / 2.0) * scale,
                (y - size * 0.7) * scale,
                width * scale,
                size * scale,
                tracking * size * scale,
            ),
            "{text}"
        }
    }
}

/// Absolutely-positioned slot on the panel, in design-space coordinates.
///
/// Everything on a faceplate is placed, not flowed — this is the placement.
/// The slot is a known-size box centred on `(x, y)`, so its position is pure
/// arithmetic at any scale.
#[component]
pub fn PanelSlot(
    scale: f64,
    /// Centre of the slot in design-space px from the panel's top-left.
    x: f64,
    y: f64,
    /// Slot size in design-space px.
    w: f64,
    h: f64,
    children: Element,
) -> Element {
    rsx! {
        div {
            style: format!(
                "position:absolute; left:{:.1}px; top:{:.1}px; \
                 width:{:.1}px; height:{:.1}px; display:flex; \
                 flex-direction:column; align-items:center; \
                 justify-content:center;",
                (x - w / 2.0) * scale,
                (y - h / 2.0) * scale,
                w * scale,
                h * scale,
            ),
            {children}
        }
    }
}
