//! The VU movement — the thing at the centre of both hardware faces.
//!
//! Geometry (the arc, the crowded scale, the needle) comes from
//! [`crate::hardware::vu_svg`]; this is the drawn face over it: the lamp
//! colour, the bezel, the printed scale and the legend. The LA-2A's is warm
//! and amber-lit; the 1176's is the blue-lit UREI face.

use audiocore_core::prelude::*;

use crate::hardware::vu_svg::{
    db_to_vu, gr_to_vu, needle_tip, scale_arc_path, tick_point, PIVOT_X, PIVOT_Y, VU_H, VU_TICKS,
    VU_W,
};

/// What the needle is reading.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VuMode {
    /// Gain reduction — rests at 0 on the right and swings left as the
    /// compressor works, like the hardware.
    GainReduction,
    /// Output level, with -18 dBFS aligned to 0 VU.
    Level,
}

/// The face's colour scheme. Two units, two lamps.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VuFace {
    /// Teletronix: cream card, warm lamp, black needle.
    Amber,
    /// UREI: blue-lit card, white printing, white needle.
    Blue,
}

impl VuFace {
    fn card(self) -> &'static str {
        match self {
            Self::Amber => "linear-gradient(180deg, #f6ecd2 0%, #e8d9b4 100%)",
            Self::Blue => "linear-gradient(180deg, #2f5f8f 0%, #16324e 100%)",
        }
    }
    fn ink(self) -> &'static str {
        match self {
            Self::Amber => "#2a241c",
            Self::Blue => "#e8f2ff",
        }
    }
    fn needle(self) -> &'static str {
        match self {
            Self::Amber => "#1d1a15",
            Self::Blue => "#f4f8ff",
        }
    }
    /// The lamp glow across the top of the card.
    fn lamp(self) -> &'static str {
        match self {
            Self::Amber => "rgba(255,196,92,0.30)",
            Self::Blue => "rgba(150,205,255,0.32)",
        }
    }
    /// Colour of the over-zero part of the scale (red on every VU ever made).
    fn hot(self) -> &'static str {
        match self {
            Self::Amber => "#a8281c",
            Self::Blue => "#ff6a5c",
        }
    }
}

/// A VU meter drawn at panel scale.
///
/// `value_db` is gain reduction in dB (positive) in
/// [`VuMode::GainReduction`], or a level in dBFS in [`VuMode::Level`].
#[component]
pub fn VuMeter(
    scale: f64,
    /// Width of the meter in design-space px; height follows the face aspect.
    #[props(default = 190.0)]
    width: f64,
    face: VuFace,
    mode: VuMode,
    value_db: f32,
    #[props(default = "VU".to_string())] legend: String,
) -> Element {
    let vu = match mode {
        VuMode::GainReduction => gr_to_vu(value_db as f64),
        VuMode::Level => db_to_vu(value_db as f64),
    };
    let (nx, ny) = needle_tip(vu);
    let arc = scale_arc_path(7.0);

    let w = width * scale;
    let h = width * (VU_H / VU_W) * scale;

    rsx! {
        div {
            "data-testid": "vu-meter",
            "data-vu": "{vu:.2}",
            style: format!(
                "position:relative; width:{w:.1}px; height:{h:.1}px; \
                 background:{}; border-radius:{:.1}px; overflow:hidden; \
                 border:{:.1}px solid rgba(0,0,0,0.55); \
                 box-shadow:inset 0 0 {:.1}px rgba(0,0,0,0.45);",
                face.card(),
                3.0 * scale,
                (1.5 * scale).max(1.0),
                10.0 * scale,
            ),

            // Lamp wash — the meter is lit from behind the top of the card.
            div {
                style: format!(
                    "position:absolute; inset:0; background:radial-gradient(\
                     ellipse at 50% 8%, {} 0%, rgba(0,0,0,0) 68%); \
                     pointer-events:none;",
                    face.lamp(),
                ),
            }

            svg {
                style: "position:absolute; inset:0; width:100%; height:100%; display:block;",
                view_box: "0 0 {VU_W} {VU_H}",
                preserve_aspect_ratio: "none",

                // The printed arc, and the red stretch above 0 VU.
                path {
                    d: "{arc}",
                    fill: "none",
                    stroke: "{face.ink()}",
                    stroke_width: "0.8",
                    opacity: "0.85",
                }
                path {
                    d: "{hot_arc_path()}",
                    fill: "none",
                    stroke: "{face.hot()}",
                    stroke_width: "1.6",
                }

                // Scale ticks. Majors are longer and numbered.
                for (v , label , major) in VU_TICKS.iter().copied() {
                    {
                        let (x1, y1) = tick_point(v, 7.0);
                        let (x2, y2) = tick_point(v, if major { 13.0 } else { 10.5 });
                        let (lx, ly) = tick_point(v, 19.0);
                        let color = if v > 0.0 { face.hot() } else { face.ink() };
                        rsx! {
                            line {
                                x1: "{x1:.2}", y1: "{y1:.2}", x2: "{x2:.2}", y2: "{y2:.2}",
                                stroke: "{color}",
                                stroke_width: if major { "1.1" } else { "0.6" },
                            }
                            if major {
                                text {
                                    x: "{lx:.2}", y: "{ly + 2.0:.2}",
                                    fill: "{color}", font_size: "5.5",
                                    text_anchor: "middle", font_weight: "600",
                                    "{label}"
                                }
                            }
                        }
                    }
                }

                // Legend under the scale — "VU", "GAIN REDUCTION".
                text {
                    x: "{VU_W * 0.5:.2}", y: "{VU_H * 0.86:.2}",
                    fill: "{face.ink()}", font_size: "5.0",
                    text_anchor: "middle", letter_spacing: "0.6",
                    "{legend}"
                }

                // Needle + hub.
                line {
                    "data-testid": "vu-needle",
                    x1: "{PIVOT_X:.2}", y1: "{PIVOT_Y:.2}",
                    x2: "{nx:.2}", y2: "{ny:.2}",
                    stroke: "{face.needle()}",
                    stroke_width: "1.1",
                    stroke_linecap: "round",
                }
                circle {
                    cx: "{PIVOT_X:.2}", cy: "{VU_H:.2}", r: "3.2",
                    fill: "{face.needle()}", opacity: "0.9",
                }
            }

            // Glass: a soft highlight across the top of the bezel.
            div {
                style: "position:absolute; inset:0; pointer-events:none; \
                        background:linear-gradient(160deg, rgba(255,255,255,0.22) 0%, \
                        rgba(255,255,255,0.04) 38%, rgba(0,0,0,0.10) 100%);",
            }
        }
    }
}

/// The red stretch of the scale, from 0 VU to the right stop.
fn hot_arc_path() -> String {
    let (x0, y0) = tick_point(0.0, 7.0);
    let (x1, y1) = tick_point(3.0, 7.0);
    let r = crate::hardware::vu_svg::NEEDLE_LEN - 7.0;
    format!("M {x0:.2} {y0:.2} A {r:.2} {r:.2} 0 0 1 {x1:.2} {y1:.2}")
}
