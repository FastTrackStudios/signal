//! Panels drawn from the vector controls, with no WALTER in the loop.
//!
//! The panels beside this one render a *REAPER theme*: they read a layout
//! from `rtconfig`, position everything by its anchor coordinates, and blit
//! the theme's PNG atlas. That is the right way to show somebody else's
//! theme, and the wrong way to build our own — the layout is data we do not
//! author and the art is bitmaps we cannot scale.
//!
//! These draw the same controls as live SVG from [`daw_theme_art`], which is
//! the same source the REAPER theme's PNGs are rasterised from. One
//! definition, two renderings: if the app and the exported theme ever
//! disagree, one of them is a bug rather than a divergence.

use crate::prelude::*;
use daw_theme_art::vector_controls as v;

/// The transport bar.
///
/// Sizes are the art's own — a transport button is 36 by 26 and the cycle
/// button 32 by 24 — laid out in a row rather than by anchor coordinates.
/// The bar scales by zooming the whole row, not by stretching the parts,
/// which is the thing bitmaps could not do.
#[component]
pub fn NativeTransportBar(
    playing: Signal<bool>,
    #[props(default)] recording: Signal<bool>,
    #[props(default)] repeat: Signal<bool>,
    #[props(default)] on_play: Option<EventHandler<()>>,
    #[props(default)] on_stop: Option<EventHandler<()>>,
    #[props(default)] position: Option<Signal<f64>>,
    #[props(default = 120.0)] bpm: f64,
    /// Pixels per art pixel. 1.0 draws the art at its measured size.
    #[props(default = 1.0)]
    zoom: f32,
) -> Element {
    let is_playing = playing();
    let is_recording = recording();
    let is_repeat = repeat();

    let position = position.map(|p| p()).unwrap_or(0.0);
    let m = (position / 60.0).floor() as i64;
    let s = position - m as f64 * 60.0;

    let scale = |px: f32| px * zoom;
    let button = |glyph: v::TransportGlyph, on: bool| {
        rsx! {
            div {
                style: "line-height:0;",
                v::TransportButton {
                    glyph,
                    on,
                    width: scale(36.0) as u32,
                    height: scale(26.0) as u32,
                }
            }
        }
    };

    rsx! {
        div {
            style: format!(
                "display:flex; align-items:center; gap:0; height:{h}px; \
                 user-select:none;",
                h = scale(26.0),
            ),
            {button(v::TransportGlyph::Home, false)}
            {button(v::TransportGlyph::End, false)}
            {button(v::TransportGlyph::Stop, false)}
            div {
                onclick: move |_| { if let Some(h) = &on_play { h.call(()); } },
                {button(v::TransportGlyph::Play, is_playing)}
            }
            div {
                onclick: move |_| { if let Some(h) = &on_stop { h.call(()); } },
                {button(v::TransportGlyph::Pause, false)}
            }
            {button(v::TransportGlyph::Record, is_recording)}

            // The cycle button and the readout are one pill: the button caps
            // its left end and the well continues it, which is why they sit
            // flush rather than in the button row's rhythm.
            div {
                style: format!("display:flex; align-items:stretch; margin-left:{}px;",
                               scale(8.0)),
                div {
                    style: "line-height:0;",
                    v::TransportButton {
                        glyph: v::TransportGlyph::Repeat,
                        on: is_repeat,
                        cell: (32.0, 24.0),
                        width: scale(32.0) as u32,
                        height: scale(24.0) as u32,
                    }
                }
                div {
                    style: format!(
                        "display:flex; align-items:center; padding:0 {pad}px; \
                         background:#282828; color:#d6d6d6; \
                         font-size:{fs}px; font-weight:700; \
                         font-variant-numeric:tabular-nums; white-space:nowrap;",
                        pad = scale(8.0),
                        fs = scale(13.0),
                    ),
                    "{m}.{s:05.2} / {bpm:.2}"
                }
            }
        }
    }
}
