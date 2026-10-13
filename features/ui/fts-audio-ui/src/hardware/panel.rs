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

use dioxus::prelude::*;

use crate::hardware::panel_svg::fit_scale;

/// Editor size for a mount that is not a `nice-plug` window.
///
/// The plugin and standalone paths both get a reactive `Signal<(u32, u32)>`
/// from `nice-plug-dioxus`, and that is what the panel normally measures. A
/// bare `VirtualDom` mount — the headless test harness — has no window at all,
/// so an embedder can state the size it is rendering at instead.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EditorSize(pub f64, pub f64);

/// A per-subtree override of the window box.
///
/// A face rendered in a ROW of a taller editor — one stage of the comp
/// stack, the panel above an embedded EQ strip — must scale itself to its
/// row, not to the whole window. The row's owner provides this around the
/// face; everything that asks [`window_logical_size`] then sees the row.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PanelBox(pub f64, pub f64);

/// Logical window size, if anything in context knows it. A [`PanelBox`]
/// provided closer to the face wins over the real window.
pub fn window_logical_size() -> Option<(f64, f64)> {
    if let Some(PanelBox(w, h)) = try_consume_context::<PanelBox>() {
        return Some((w, h));
    }
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

/// How a panel is finished at its left and right edges.
///
/// Not decoration: it says what the thing *is*. Rack ears mean a unit that
/// lives in a rack; wooden cheeks mean one that sits on a desk, which is how
/// the dbx 160 and its generation were sold.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PanelEnds {
    #[default]
    RackEars,
    Wood,
}

/// The finish over a panel's paint.
///
/// `paint` gives a panel its colour and its top-to-bottom sheen. What it
/// cannot give is *grain*: a brushed aluminium plate is covered in fine
/// horizontal striations, and without them a silver panel reads as grey
/// plastic no matter how well the gradient is tuned.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PanelTexture {
    /// Painted steel: the paint is the finish.
    #[default]
    Painted,
    /// Brushed metal: horizontal grain under a broad diagonal sheen.
    ///
    /// `strength` is a percentage of the reference grain, which is tuned for
    /// natural (light) aluminium. It exists because the same scratches are not
    /// equally *visible* on every plate: a white striation over pale metal is
    /// a small step in luminance, and the identical striation over black
    /// anodising is a large one. At full strength a blackface panel turns into
    /// a venetian blind. Dark plates want roughly half.
    Brushed { strength: u8 },
}

/// Brushed aluminium, as background layers.
///
/// Zoom into a real plate and the finish is not a ruled pattern: it is a mess
/// of scratches left by an abrasive belt. They run *along* the brush direction
/// (horizontally, hence 0deg gradients, which vary down the panel), but they
/// vary in width, spacing and brightness, and there are longer smears of
/// slightly lighter and darker metal over the top of them.
///
/// A single repeating gradient cannot look like that — one period reads as
/// corduroy, which is exactly what the first attempt did. So this stacks
/// several at deliberately non-commensurate periods (2 / 3 / 5 / 9 / 23 px):
/// having no common multiple, they never line up twice in the same way, and
/// their interference is what sells the surface as scratched rather than
/// printed. Each is nearly invisible alone — the effect is cumulative.
///
/// The last layer is not scratches at all: it is the broad off-axis sheen, the
/// highlight that sweeps across a metal plate and is the reason it reads as
/// metal rather than grey card.
///
/// Everything here is horizontal, and deliberately so. A crossing 90deg pass
/// was tried, for the lengthwise streaking of the belt; against the horizontal
/// layers it interferes into a visible grid and the panel turns into woven
/// fabric. Brushing has one direction, and so does this.
///
/// Layer order is paint order: first listed is on top, and the raw paint
/// passed by the design goes underneath all of it.
fn brushed_grain(strength: u8) -> String {
    let k = strength.min(100) as f64 / 100.0;
    let w = |v: f64| format!("rgba(255,255,255,{:.4})", v * k);
    let b = |v: f64| format!("rgba(0,0,0,{:.4})", v * k);
    format!(
        "repeating-linear-gradient(0deg, {} 0px 1px, {} 1px 2px), \
         repeating-linear-gradient(0deg, {} 0px 1px, rgba(0,0,0,0) 1px 3px), \
         repeating-linear-gradient(0deg, {} 0px 1px, rgba(0,0,0,0) 1px 5px), \
         repeating-linear-gradient(0deg, {} 0px 2px, rgba(0,0,0,0) 2px 9px), \
         repeating-linear-gradient(0deg, {} 0px 3px, {} 3px 6px, \
           rgba(0,0,0,0) 6px 23px), \
         linear-gradient(102deg, rgba(255,255,255,0) 0%, {} 22%, \
           rgba(255,255,255,0) 41%, {} 63%, {} 84%, rgba(255,255,255,0) 100%)",
        w(0.055),
        b(0.045),
        w(0.032),
        b(0.040),
        w(0.045),
        b(0.055),
        w(0.030),
        w(0.10),
        b(0.06),
        w(0.07),
    )
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
    #[props(default = "#b9b4a8".to_string())]
    chrome: String,
    /// How the panel is finished at its edges.
    #[props(default)]
    ends: PanelEnds,
    /// The finish over the paint.
    #[props(default)]
    texture: PanelTexture,
    /// Rendered inside the panel, positioned absolutely in design space.
    children: Element,
) -> Element {
    let (w, h) = (design_w * scale, design_h * scale);

    // The grain is a *background layer of the panel*, not an overlay element.
    //
    // As an absolutely-positioned first child it painted over the meter's blue
    // section: blitz does not order positioned siblings by tree position the
    // way CSS specifies, so being "earlier in the DOM" bought nothing. A
    // background cannot be painted over its own box's children by definition,
    // which is the property actually wanted — this is the plate, and
    // everything the panel carries is mounted on it.
    let paint = match texture {
        PanelTexture::Painted => background.clone(),
        PanelTexture::Brushed { strength } => {
            format!("{}, {background}", brushed_grain(strength))
        }
    };

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
                     background:{paint}; border-radius:{:.1}px; \
                     border:{:.1}px solid rgba(0,0,0,0.55); \
                     box-shadow:0 {:.1}px {:.1}px rgba(0,0,0,0.5);",
                    3.0 * scale,
                    1.0 * scale,
                    4.0 * scale,
                    18.0 * scale,
                ),

                // (The brushed grain is a background layer of this div — see
                // `paint` above. It was a child once, and painted over things.)

                // The edges: screwed rack ears, or the walnut cheeks a
                // desktop unit of the period was sold with.
                match ends {
                    PanelEnds::RackEars => rsx! {
                        RackEar { scale, chrome: chrome.clone(), left: true, height: design_h }
                        RackEar { scale, chrome: chrome.clone(), left: false, height: design_h }
                    },
                    PanelEnds::Wood => rsx! {
                        WoodCheek { scale, left: true }
                        WoodCheek { scale, left: false }
                    },
                }

                {children}
            }
        }
    }
}

/// A walnut side cheek.
#[component]
fn WoodCheek(scale: f64, left: bool) -> Element {
    let w = 34.0 * scale;
    let side = if left { "left:0;" } else { "right:0;" };
    rsx! {
        div {
            style: format!(
                "position:absolute; top:0; bottom:0; {side} width:{w:.1}px; \
                 background:linear-gradient(90deg, #6b3f22 0%, #8a5530 26%, #7a4a28 58%, \
                   #5e3419 100%); \
                 box-shadow:inset {:.1}px 0 {:.1}px rgba(0,0,0,0.35); \
                 border-{}:1px solid rgba(0,0,0,0.5);",
                if left { 2.0 * scale } else { -2.0 * scale },
                4.0 * scale,
                if left { "right" } else { "left" },
            ),
            // Grain: a few darker streaks down the cheek.
            div {
                style: "position:absolute; inset:0; \
                        background:repeating-linear-gradient(92deg, \
                          rgba(0,0,0,0.16) 0 1px, rgba(0,0,0,0.0) 1px 7px);",
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
