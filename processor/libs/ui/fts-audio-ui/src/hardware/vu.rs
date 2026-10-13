//! The VU movement — the thing at the centre of both hardware faces.
//!
//! Geometry (the arc, the crowded scale, the needle) comes from
//! [`crate::hardware::vu_svg`]; this is the drawn face over it: the lamp
//! colour, the bezel, the printed scale and the legend. The LA-2A's is warm
//! and amber-lit; the 1176's is the blue-lit UREI face.

use dioxus::prelude::*;

use crate::hardware::vu_kit::{Bezel, VuSpec, Wash};
use crate::hardware::vu_svg::{
    scale_arc_for, scale_needle_tip, scale_point, VuScale, PIVOT_X, PIVOT_Y, VU_H, VU_W,
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

/// The face's colour scheme.
///
/// A period VU movement — Weston, Modutec, Sifam — has an **ivory card with a
/// black scale and a red stretch above 0**, lit from behind by one or two
/// bulbs. The lamp is what varies between units, not the card: the LA-2A's is
/// warm, a rackmount's is whiter. Blue-faced meters are largely a plugin
/// aesthetic rather than something these units wore, so the default is ivory
/// and blue is kept only for a unit that genuinely has one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VuFace {
    /// Warm ivory under a yellow lamp — the LA-2A's, and most tube gear's.
    Amber,
    /// Neutral ivory under a white lamp — the 1176's Modutec, an SSL.
    Ivory,
    /// Amber card printed in *blue* — the dbx's, which is not a VU at all but
    /// a decibel readout, and prints like one.
    AmberBlue,
    /// Blue-lit card, white printing.
    Blue,
    /// The modern console movement: a near-black card behind glass with the
    /// lamp *behind the needle*, so the light pools at the pivot and falls
    /// off upward, and the scale printed white over it.
    ///
    /// The one face that takes `tint` — the glow, the pool in the card and
    /// the bloom off the needle all follow the colour a panel asks for.
    Backlit,
}

impl VuFace {
    /// Every face in the kit, so a test or a contact sheet can walk them all
    /// without anyone having to remember to add the new one.
    pub const ALL: [VuFace; 5] = [
        Self::Amber,
        Self::Ivory,
        Self::AmberBlue,
        Self::Blue,
        Self::Backlit,
    ];

    /// How this face is printed and lit — see
    /// [`vu_kit`](crate::hardware::vu_kit).
    ///
    /// The only place a face maps to an appearance. The renderer below walks
    /// the spec, so a new face is a variant here plus one const in
    /// [`vu_faces`](crate::hardware::vu_faces).
    pub fn spec(self) -> &'static VuSpec {
        use crate::hardware::vu_faces as kit;
        match self {
            Self::Amber => &kit::AMBER,
            Self::Ivory => &kit::IVORY,
            Self::AmberBlue => &kit::AMBER_BLUE,
            Self::Blue => &kit::BLUE,
            Self::Backlit => &kit::BACKLIT,
        }
    }

    /// Whether this face takes a colour from the call site.
    ///
    /// Almost none do. A period card is the colour it is, and recolouring one
    /// would be a different unit rather than the same one under a different
    /// lamp — which is exactly what a backlit movement *is*.
    pub fn is_tintable(self) -> bool {
        self.spec().wash != Wash::None
    }
}

/// Which frame the movement is mounted in.
///
/// A separate part from the face: the same movement sits in a pressed-steel
/// rack frame on one unit and a chromed ring on another. A [`VuFace`] names
/// its default, and a panel can ask for a different one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BezelStyle {
    /// Pressed steel, matte black, vented. The rack default.
    BlackRack,
    /// A thin dark surround and nothing else.
    Slim,
    /// A bright chromed ring.
    Chrome,
    /// Warm brass, a little tarnished in the shadows.
    Brass,
    /// Satin nickel on a brushed panel — the Teletronix-era meter plate.
    SatinPlate,
    /// A deep rectangular recess, the movement set well back behind glass.
    Recessed,
    /// The rack frame with the bulb turned up — warm light around the opening.
    LitAmber,
    /// The same, lit cold, for a blue-carded movement.
    LitBlue,
    /// A chromed ring around a lit movement.
    LitChrome,
}

impl BezelStyle {
    /// Every frame in the kit, so a contact sheet can walk them all.
    pub const ALL: [BezelStyle; 9] = [
        Self::BlackRack,
        Self::Slim,
        Self::Chrome,
        Self::Brass,
        Self::SatinPlate,
        Self::Recessed,
        Self::LitAmber,
        Self::LitBlue,
        Self::LitChrome,
    ];

    pub fn spec(self) -> &'static Bezel {
        use crate::hardware::vu_faces as kit;
        match self {
            Self::BlackRack => &kit::BLACK_RACK,
            Self::Slim => &kit::SLIM,
            Self::Chrome => &kit::CHROME,
            Self::Brass => &kit::BRASS,
            Self::SatinPlate => &kit::SATIN_PLATE,
            Self::Recessed => &kit::RECESSED,
            Self::LitAmber => &kit::LIT_AMBER,
            Self::LitBlue => &kit::LIT_BLUE,
            Self::LitChrome => &kit::LIT_CHROME,
        }
    }

    /// Whether this frame throws light around its opening.
    pub fn is_lit(self) -> bool {
        self.spec().glow.is_some()
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
    /// Wrap the movement in a black bezel with a vent below it, as a meter
    /// mounted through a panel rather than printed on one.
    #[props(default = false)]
    bezel: bool,
    /// Which frame to mount it in. Defaults to the face's own.
    #[props(default)]
    bezel_style: Option<BezelStyle>,
    /// What the card is printed with — a VU standard or a plain decibel scale.
    #[props(default)]
    card: VuScale,
    /// The frame's lip shadow, cast onto this card.
    ///
    /// Set by the bezel wrapper below rather than by a panel: the movement
    /// draws the shadow, because the shadow falls *inside* it, but which
    /// shadow is the frame's business.
    #[props(default)]
    shade: Option<String>,
    /// The colour a backlit movement is lit in.
    ///
    /// Only [`VuFace::Backlit`] takes one: on the period faces the card is
    /// the colour it is, and a tint would be a recolour of a real part. Here
    /// the glow, the pool in the card and the bloom off the needle all follow
    /// it, and the printed scale stays white.
    #[props(default)]
    tint: Option<String>,
) -> Element {
    let value = match mode {
        VuMode::GainReduction => card.from_gain_reduction_db(value_db as f64),
        VuMode::Level => card.from_level_db(value_db as f64),
    };
    let (nx, ny) = scale_needle_tip(card, value);
    let arc = scale_arc_for(card, 6.0);

    let w = width * scale;
    let h = width * (VU_H / VU_W) * scale;
    let spec = face.spec();
    // A tint only reaches a face that says it takes one.
    let wash = tint.as_deref().filter(|_| spec.wash != Wash::None);
    let card_css = wash
        .and_then(|c| spec.wash.card(c))
        .unwrap_or_else(|| spec.card.to_string());
    let lamp_css = match wash.and_then(|c| spec.wash.lamp(c)) {
        Some(color) => spec
            .lamp
            .map(|l| crate::hardware::vu_kit::lamp_layers(l, &color)),
        None => spec.lamp_css(),
    };
    let halo_color = wash
        .and_then(|c| spec.wash.halo(c))
        .or_else(|| spec.needle.halo.map(|h| h.color.to_string()));

    if bezel {
        let b = match bezel_style {
            Some(style) => *style.spec(),
            None => spec.bezel,
        };
        return rsx! {
            div {
                "data-testid": "vu-bezel",
                "data-lit": "{b.glow.is_some()}",
                // The frame body: matte black, sitting on the panel.
                style: format!(
                    "position:relative; display:inline-flex; flex-direction:column; \
                     align-items:center; \
                     padding:{:.1}px {:.1}px {:.1}px; border-radius:{:.1}px; \
                     background:{}; \
                     box-shadow:0 {:.1}px {:.1}px rgba(0,0,0,0.55), \
                       inset 0 {:.1}px 0 rgba(255,255,255,0.07){};",
                    5.0 * scale,
                    5.0 * scale,
                    4.0 * scale,
                    b.radius * scale,
                    b.frame,
                    3.0 * scale,
                    7.0 * scale,
                    1.0 * scale,
                    // A lit frame spills light onto the panel around it.
                    match b.glow {
                        Some(g) => format!(", {}", g.css(scale)),
                        None => String::new(),
                    },
                ),

                // The opening, chamfered *inward*. Four border colours, so the
                // browser mitres the corners at 45° and the faces read as
                // angles rather than as a flat outline.
                //
                // Lit from above, a sunken opening has its **top** face in
                // shadow and its **bottom** face catching the light — the
                // opposite of a raised boss, and the whole difference between
                // a movement set into a panel and one printed on it.
                div {
                    style: format!(
                        "display:flex; line-height:0; \
                         border-top:{0:.1}px solid {1}; \
                         border-left:{0:.1}px solid {2}; \
                         border-right:{0:.1}px solid {3}; \
                         border-bottom:{0:.1}px solid {4}; \
                         box-shadow:inset 0 {5:.1}px {6:.1}px rgba(0,0,0,0.75);",
                        b.depth * scale,
                        b.top,
                        b.left,
                        b.right,
                        b.bottom,
                        2.0 * scale,
                        5.0 * scale,
                    ),
                    VuMeter {
                        scale, width, face, mode, value_db, legend, card, tint,
                        shade: b.shade.map(|sh| sh.css(scale)),
                    }
                }

                // The room on the frame's own surface. Over the frame and the
                // vent, under nothing — it is the outermost thing there is,
                // because it is a reflection off the front of the moulding.
                if let Some(sheen) = b.sheen {
                    div {
                        "data-testid": "vu-frame-sheen",
                        style: format!(
                            "position:absolute; inset:0; pointer-events:none; \
                             border-radius:{:.1}px; background:{sheen};",
                            b.radius * scale,
                        ),
                    }
                }

                // The vent under the glass, which is most of what says the
                // movement is mounted through the panel.
                if let Some(vent) = b.vent {
                    div {
                        style: format!(
                            "margin-top:{:.1}px; width:{:.1}px; height:{:.1}px; \
                             border-radius:{:.1}px; background:{};",
                            4.0 * scale,
                            width * 0.30 * scale,
                            4.0 * scale,
                            1.0 * scale,
                            vent.css(scale),
                        ),
                    }
                }
            }
        };
    }

    rsx! {
        div {
            "data-testid": "vu-meter",
            "data-vu": "{value:.2}",
            style: format!(
                "position:relative; width:{w:.1}px; height:{h:.1}px; \
                 background:{}; border-radius:{:.1}px; overflow:hidden; \
                 border:{:.1}px solid rgba(0,0,0,0.55); \
                 box-shadow:inset 0 0 {:.1}px rgba(0,0,0,0.45);",
                card_css,
                3.0 * scale,
                (1.5 * scale).max(1.0),
                10.0 * scale,
            ),

            // Lamp wash — the meter is lit from behind the card.
            if let Some(lamp) = lamp_css {
                div {
                    style: format!(
                        "position:absolute; inset:0; background:{lamp}; \
                         pointer-events:none;",
                    ),
                }
            }

            svg {
                style: "position:absolute; inset:0; width:100%; height:100%; display:block;",
                view_box: "0 0 {VU_W} {VU_H}",
                preserve_aspect_ratio: "none",

                // The printed arc, and the red stretch above 0 VU.
                path {
                    d: "{arc}",
                    fill: "none",
                    stroke: "{spec.ink}",
                    stroke_width: "0.8",
                    opacity: "0.85",
                }
                // Only a VU prints a red stretch; a decibel readout does not.
                if card.hot_from().is_some() {
                    path {
                        d: "{hot_arc_path()}",
                        fill: "none",
                        stroke: "{spec.hot}",
                        stroke_width: "1.6",
                    }
                }

                // Scale ticks. Majors are longer and numbered.
                for (v , label , major) in card.ticks().iter().copied() {
                    {
                        let (x1, y1) = scale_point(card, v, 6.0);
                        let (x2, y2) = scale_point(card, v, if major { 13.0 } else { 10.0 });
                        let (lx, ly) = scale_point(card, v, 22.0);
                        let hot = card.hot_from().map(|from| v > from).unwrap_or(false);
                        let color = if hot { spec.hot } else { spec.ink };
                        rsx! {
                            line {
                                x1: "{x1:.2}", y1: "{y1:.2}", x2: "{x2:.2}", y2: "{y2:.2}",
                                stroke: "{color}",
                                stroke_width: if major { "1.1" } else { "0.6" },
                            }
                            if major {
                                text {
                                    x: "{lx:.2}", y: "{ly + 2.2:.2}",
                                    fill: "{color}", font_size: "6",
                                    text_anchor: "middle", font_weight: "600",
                                    "{label}"
                                }
                            }
                        }
                    }
                }

                // Legend under the scale. A real card prints the *instrument*
                // — "VU", "dB" — and leaves what it is metering to a switch
                // on the panel, so this is short or absent. A blank legend
                // prints nothing rather than an empty text node.
                if !legend.trim().is_empty() {
                    text {
                        x: "{VU_W * 0.5:.2}", y: "{VU_H * 0.93:.2}",
                        fill: "{spec.ink}", font_size: "5.0",
                        text_anchor: "middle", letter_spacing: "0.6",
                        "{legend}"
                    }
                }

                // Needle + hub.
                // The bloom off a lit needle: wide faint strokes under the
                // crisp one. A needle in front of a lamp is not a line.
                if let (Some(halo), Some(color)) = (spec.needle.halo, halo_color.as_deref()) {
                    for (i , w) in halo.widths(spec.needle.width).into_iter().enumerate() {
                        line {
                            key: "halo{i}",
                            x1: "{PIVOT_X:.2}", y1: "{PIVOT_Y:.2}",
                            x2: "{nx:.2}", y2: "{ny:.2}",
                            stroke: "{color}",
                            stroke_width: "{w:.2}",
                            stroke_linecap: "round",
                        }
                    }
                }
                line {
                    "data-testid": "vu-needle",
                    x1: "{PIVOT_X:.2}", y1: "{PIVOT_Y:.2}",
                    x2: "{nx:.2}", y2: "{ny:.2}",
                    stroke: "{spec.needle.color}",
                    stroke_width: "{spec.needle.width:.2}",
                    stroke_linecap: "round",
                }
                circle {
                    cx: "{PIVOT_X:.2}", cy: "{VU_H:.2}", r: "{spec.needle.hub_r:.2}",
                    fill: "{spec.needle.color}", opacity: "{spec.needle.hub_opacity:.2}",
                }
            }

            // The frame's lip, shadowing the card behind it. Over the print,
            // because the shadow falls on the whole face — under the glass,
            // because the glass is in front of all of it.
            if let Some(shade) = &shade {
                div {
                    "data-testid": "vu-shade",
                    style: format!(
                        "position:absolute; inset:0; pointer-events:none; box-shadow:{shade};",
                    ),
                }
            }

            // Glass: a soft highlight across the top of the bezel.
            if let Some(glass) = spec.glass {
                div {
                    style: format!(
                        "position:absolute; inset:0; pointer-events:none; background:{glass};",
                    ),
                }
            }
        }
    }
}

/// The red stretch of the scale, from 0 VU to the right stop.
fn hot_arc_path() -> String {
    let (x0, y0) = scale_point(VuScale::Vu, 0.0, 6.0);
    let (x1, y1) = scale_point(VuScale::Vu, 3.0, 6.0);
    let r = crate::hardware::vu_svg::NEEDLE_LEN - 6.0;
    format!("M {x0:.2} {y0:.2} A {r:.2} {r:.2} 0 0 1 {x1:.2} {y1:.2}")
}
