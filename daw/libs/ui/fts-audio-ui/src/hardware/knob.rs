//! A hardware knob — pointer, skirt, and a scale ring printed on the panel.
//!
//! Different from [`fts_audio_ui::Knob`] in the way that matters: an FTS knob
//! draws its value as an arc, because the value is the point. A hardware knob
//! draws a *pointer*, and the value is read off numbers silkscreened on the
//! panel around it — which is why the ring here is drawn even where the
//! pointer is not, and why the numbers are the unit's own (LA-2A GAIN reads
//! 0–10, 1176 INPUT reads -48…+12) rather than the engine's.
//!
//! Geometry lives in [`crate::hardware::knob_svg`]. Dragging goes through the
//! same [`DragProvider`](fts_audio_ui::drag::DragProvider) as every other FTS
//! control, so a hardware face behaves like the rest of the editor.

use crate::drag::DragState;
use crate::gesture::{self, Press};
use crate::prelude::*;
use dioxus::prelude::*;

use crate::hardware::knob_kit::{Edge, Index, KnobSpec, Paint, Turns};
use crate::hardware::knob_svg::{
    knob_angle, pointer_polygon, ring_arc_path, ring_point, ScaleMark,
};

/// Design-space radii inside the knob's own `-55 -55 110 110` viewBox.
const BODY_R: f64 = 30.0;
/// Default radii for the printed ring and its numerals. Panels that print
/// their numbers close in around the skirt override them — see
/// [`Ring::geometry`](crate::hardware::rack::Ring::geometry).
const RING_R: f64 = 41.0;
const LABEL_R: f64 = 50.0;

/// How far a Marconi wing overhangs the skirt it is mounted on, as a multiple
/// of the skirt's radius, and how far its tail runs the other way.
///
/// The wing is a bar laid *across* the knob and overhanging it — that overhang
/// is the whole silhouette, and a wing contained inside its own disc reads as
/// a stripe painted on a circle. `ring_offset` moves the printed scale out to
/// make room for it.
const WING_REACH: f64 = 1.26;
const WING_TAIL: f64 = 0.66;

/// Pixels of vertical drag per full sweep. Looser than the FTS knob's 150 —
/// these are big knobs with printed scales, and a coarse feel suits them.
/// A hardware knob is bigger than a flat one, so a sweep is a little longer.
const SENSITIVITY: f64 = 190.0;

/// How the knob is built.
///
/// These are the actual knobs the units wear, because the shape is most of
/// what you recognise a panel by before you read a word of it:
///
/// - **Daka-Ware** (Pultec EQP-1A): black phenolic, a 1⅛" ridged body sitting
///   on a 1½" skirt — so the skirt is a third wider than the grip — with the
///   indicator *engraved into the body and filled white* rather than moulded
///   as a pointer.
/// - **Marconi** (Neve 1073, and the 1081/1272): a coloured two-piece knob
///   whose body carries a raised wing to grip, over a wider skirt, with a
///   white line down the wing. Neve's colour coding rides on these — red gain,
///   blue filters, grey shelf.
/// - **Collet** (SSL 4000 channel): a flat-topped coloured cap with a fluted
///   rim and a single white bar across the top. No skirt: the panel prints the
///   travel as dots around it instead.
/// - **Silver-top** (UREI 1176): a wide matte black collar with a *brushed,
///   knurled aluminium cap* set into the middle of it, and the index a white
///   line on the **collar** — outside the cap, not across it. That is the
///   arrangement that makes an 1176's knobs read as rings from across a room:
///   a dark annulus around a bright disc. INPUT and OUTPUT take the large
///   one, ATTACK and RELEASE the small.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KnobStyle {
    /// Black bakelite with a white pointer line — the LA-2A / 1176 knob.
    Bakelite,
    /// Brushed metal with a dark pointer.
    Metal,
    /// Generic skirted knob with a rim index mark.
    Skirted,
    /// Daka-Ware phenolic: ridged body on a wider skirt, engraved index line.
    Daka,
    /// Marconi wing knob: coloured body and wing over a skirt.
    Marconi,
    /// SSL collet cap: flat top, fluted rim, white bar.
    Collet,
    /// UREI 1176: black body, brushed silver top, clear collar.
    SilverTop,
    /// dbx and its generation: a brushed aluminium knob with a fluted rim and
    /// a dark centre cap, read by a line across the metal.
    MetalFluted,
    /// Teletronix LA-2A and its contemporaries: a plain black round knob with
    /// a moulded *nose* that points at a scale printed on the panel. No skirt,
    /// no flutes — you read the nose.
    Pointer,
    /// Neve 1073 and its module family: a smooth turned outer ring around a
    /// **geared** cap — bumps coarse enough to count, on the inner knob — with
    /// a painted white index out at the cap's teeth. It sits inside a ring of
    /// white dots printed on the panel (see
    /// [`Ring::Dots`](crate::hardware::rack::Ring::Dots)).
    ///
    /// Which part is toothed is the whole tell: teeth outside a smooth cap is
    /// somebody else's knob. So the cap is a path rather than a plain disc,
    /// and the metal is deliberately flat — a brushed gradient here reads as
    /// chrome and makes a row of these look plated rather than painted.
    ///
    /// Pairs with `inner_handle` on [`HardwareKnob`]: on the module the ring
    /// and the cap are two different controls, and the toothed one is the cap.
    Neve,
    /// Empirical Labs Distressor: a wide brushed dial whose *numerals are
    /// printed on the skirt* and turn with it, around a dark centre cap. The
    /// scale moving rather than a pointer moving is the whole look, and it is
    /// why the panel around a Distressor knob is bare.
    Dial,
}

impl KnobStyle {
    /// Every knob in the kit, so a test or a contact sheet can walk them all
    /// without anyone having to remember to add the new one.
    pub const ALL: [KnobStyle; 11] = [
        Self::Bakelite,
        Self::Metal,
        Self::Skirted,
        Self::Daka,
        Self::Marconi,
        Self::Collet,
        Self::SilverTop,
        Self::MetalFluted,
        Self::Pointer,
        Self::Neve,
        Self::Dial,
    ];

    /// What this knob is made of — see
    /// [`knob_kit`](crate::hardware::knob_kit).
    ///
    /// This is the only place a style maps to an appearance. The renderer
    /// below walks the spec and knows nothing about which unit it is drawing,
    /// so a new knob is a variant here plus one const in
    /// [`knob_parts`](crate::hardware::knob_parts).
    pub fn spec(self) -> &'static KnobSpec {
        use crate::hardware::knob_parts as kit;
        match self {
            Self::Bakelite => &kit::BAKELITE,
            Self::Metal => &kit::METAL,
            Self::Skirted => &kit::SKIRTED,
            Self::Daka => &kit::DAKA,
            Self::Marconi => &kit::MARCONI,
            Self::Collet => &kit::COLLET,
            Self::SilverTop => &kit::SILVER_TOP,
            Self::MetalFluted => &kit::METAL_FLUTED,
            Self::Pointer => &kit::POINTER,
            Self::Neve => &kit::NEVE,
            Self::Dial => &kit::DIAL,
        }
    }

    /// How far out the panel's printed scale has to sit for this knob, in the
    /// knob's viewBox units.
    pub fn ring_offset(self) -> f64 {
        self.spec().ring_offset
    }

    /// Whether the printed scale belongs to the knob rather than the panel.
    pub fn numerals_on_knob(self) -> bool {
        self.spec().numerals_on_knob
    }

    /// The cap's diameter as a fraction of the knob's — what a dual-concentric
    /// knob sizes its inner drag region to.
    pub fn cap_fraction(self) -> f64 {
        self.spec().cap_fraction()
    }
}

/// The Daka-Ware skirt's outline: a ring of coarse rounded lobes rather than a
/// circle, which is what gives the knob its scalloped silhouette and the grip
/// you actually turn it by.
///
/// Drawn as a closed path of quadratic curves — one control point per lobe,
/// pushed out past the rim — so the lobes are round rather than sawtoothed.
fn scallop_path(r: f64, lobes: usize, depth: f64) -> String {
    use std::f64::consts::TAU;
    if lobes < 3 {
        return String::new();
    }
    let point = |a: f64, rr: f64| (rr * a.sin(), -rr * a.cos());
    let inner = r - depth;
    let (sx, sy) = point(0.0, inner);
    let mut d = format!("M {sx:.2} {sy:.2}");
    for i in 0..lobes {
        let a0 = (i as f64 / lobes as f64) * TAU;
        let a1 = ((i + 1) as f64 / lobes as f64) * TAU;
        let mid = (a0 + a1) * 0.5;
        // The control point rides outside the rim, which rounds the lobe.
        let (cx, cy) = point(mid, r + depth * 0.55);
        let (ex, ey) = point(a1, inner);
        d.push_str(&format!(" Q {cx:.2} {cy:.2} {ex:.2} {ey:.2}"));
    }
    d.push_str(" Z");
    d
}

/// The pointer knob's nose: a teardrop reaching past the body, which is what
/// the panel's numbers are read against.
fn nose_points(body_r: f64) -> String {
    let w = body_r * 0.30;
    format!(
        "{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2}",
        -w,
        -(body_r * 0.55),
        w,
        -(body_r * 0.55),
        w * 0.34,
        -(body_r + 9.0),
        -w * 0.34,
        -(body_r + 9.0),
    )
}

/// The Marconi wing, as an SVG polygon in the knob's viewBox: a raised grip
/// that runs across the body and tapers to the indicator end.
fn wing_points(body_r: f64) -> String {
    let w = body_r * 0.46;
    format!(
        "{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2}",
        -w,
        BODY_R * WING_TAIL,
        w,
        BODY_R * WING_TAIL,
        w * 0.64,
        -(BODY_R * WING_REACH),
        -w * 0.64,
        -(BODY_R * WING_REACH),
    )
}

/// The lit flank of a [`wing_points`] moulding: a narrow sliver down its left
/// side, so the wing reads as a raised bar rather than a painted stripe.
fn wing_highlight_points(body_r: f64) -> String {
    let w = body_r * 0.46;
    format!(
        "{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2}",
        -w,
        BODY_R * WING_TAIL,
        -w * 0.55,
        BODY_R * WING_TAIL,
        -w * 0.36,
        -(BODY_R * WING_REACH),
        -w * 0.64,
        -(BODY_R * WING_REACH),
    )
}

/// One index, in the knob's viewBox, before rotation.
///
/// Every style's pointer goes through here: a bar between two radii, a
/// tapered blade, a Marconi wing, a moulded nose, or nothing at all. Adding a
/// silhouette means an [`Index`] variant and an arm here — and nothing else,
/// because the renderer only ever calls this.
fn draw_index(index: Index, tint: Option<&str>) -> Element {
    match index {
        Index::None => rsx! {},
        Index::Bar {
            from,
            to,
            width,
            color,
        } => rsx! {
            rect {
                x: "{-width / 2.0:.2}",
                y: "{-(BODY_R * to):.2}",
                width: "{width:.2}",
                height: "{BODY_R * (to - from):.2}",
                rx: "{(width / 2.4).min(1.6):.2}",
                fill: "{color}",
            }
        },
        Index::Blade {
            to,
            half_width,
            color,
        } => rsx! {
            polygon {
                points: "{pointer_polygon(BODY_R * to, BODY_R * half_width)}",
                fill: "{color}",
            }
        },
        Index::Nose { color } => rsx! {
            polygon {
                points: "{nose_points(BODY_R)}",
                fill: "#232327",
                stroke: "rgba(0,0,0,0.55)",
                stroke_width: "0.8",
            }
            rect {
                x: "-1.2",
                y: "{-(BODY_R + 7.0):.1}",
                width: "2.4",
                height: "{BODY_R * 0.55:.1}",
                rx: "1.0",
                fill: "{color}",
            }
        },
        // The wing takes the control's colour: on a 1073 the wing IS the
        // colour, and a translucent sliver over a coloured body read as a
        // smudge on a circle rather than a bar standing off it.
        Index::Wing { color, body } => rsx! {
            polygon {
                points: "{wing_points(BODY_R * body)}",
                transform: "translate(0.8 1.6)",
                fill: "rgba(0,0,0,0.45)",
            }
            polygon {
                points: "{wing_points(BODY_R * body)}",
                fill: "{tint.unwrap_or(\"#3a3a40\")}",
                stroke: "rgba(0,0,0,0.6)",
                stroke_width: "0.9",
            }
            polygon {
                points: "{wing_highlight_points(BODY_R * body)}",
                fill: "rgba(255,255,255,0.20)",
            }
            rect {
                x: "-1.3",
                y: "{-(BODY_R - 2.0):.1}",
                width: "2.6",
                height: "{BODY_R * 0.62:.1}",
                rx: "1.0",
                fill: "{color}",
            }
        },
    }
}

/// A knob on a hardware faceplate.
///
/// `marks` is the printed scale ring — build it with
/// [`scale_ring`](crate::hardware::knob_svg::scale_ring).
#[component]
pub fn HardwareKnob(
    handle: ParamHandle,
    /// Stable test id; rendered as `hw-knob-{testid}`.
    testid: String,
    scale: f64,
    /// Knob diameter in design-space px, excluding the printed ring.
    #[props(default = 62.0)]
    diameter: f64,
    #[props(default = KnobStyle::Bakelite)] style: KnobStyle,
    /// Colour of the silkscreened ring numbers.
    #[props(default = "#2b2620".to_string())]
    ink: String,
    #[props(default)] marks: Vec<ScaleMark>,
    /// Radius of the printed tick ring, in the knob's viewBox units.
    #[props(default = RING_R)]
    ring_r: f64,
    /// Radius the numerals are printed at.
    #[props(default = LABEL_R)]
    label_r: f64,
    /// Draw tick marks, or numerals alone.
    #[props(default = true)]
    ticks: bool,
    /// Dots printed evenly along the sweep instead of ticks, and how many.
    ///
    /// The 1073's rings are dots, not dashes, and they are what the panel
    /// reads as before you can make out a number — a small bright arc around
    /// each knob. Zero draws none.
    #[props(default = 0)]
    dots: usize,
    /// Override the knob body's colour.
    ///
    /// A console colour-codes its bands — the SSL's blue LMF, green HMF,
    /// magenta HF — and that colour is how you find the band you want without
    /// reading anything. The [`KnobStyle`] still decides the finish.
    #[props(default)]
    tint: Option<String>,
    /// The *inner* control of a dual-concentric knob.
    ///
    /// A 1073's EQ knobs are two controls in one place: the bright collar
    /// selects the band's frequency, and the grey cap sitting inside it sets
    /// that band's gain. They turn independently, so this draws two indices at
    /// two angles and hands the cap its own drag region — press the collar and
    /// you are on `handle`, press the cap and you are on this one.
    ///
    /// `None` is an ordinary knob, where the whole thing turns together.
    #[props(default)]
    inner_handle: Option<ParamHandle>,
) -> Element {
    let mut drag: Signal<DragState> = use_context();
    // Re-render while a drag is in flight so the pointer tracks the cursor.
    let _ = drag.read().move_count;
    // Right-click / Enter open the readout's text entry.
    let mut readout_open = use_signal(|| false);

    let normalized = handle.normalized() as f64;
    // The collar's angle, and the cap's. They are the same knob unless an
    // inner control was given, in which case the two halves move apart.
    let angle = knob_angle(normalized);
    let inner_angle = match &inner_handle {
        Some(inner) => knob_angle(inner.normalized() as f64),
        None => angle,
    };
    let display = handle.display_value();
    let name = handle.name();

    // The printed ring is drawn outside the body, so the box is wider than
    // the knob — the viewBox spans -55..55 with the body at r = 30.
    let box_px = diameter * (110.0 / (BODY_R * 2.0)) * scale;
    let ring = ring_arc_path(ring_r);
    // What the knob is made of. Everything below is a walk over this — the
    // renderer never asks which unit it is drawing.
    let spec = style.spec();
    // The cap: what a dual-concentric knob's inner drag region is sized to.
    let body_px = diameter * spec.cap_fraction().max(0.001) * scale;

    rsx! {
        div {
            "data-testid": "hw-knob-{testid}",
            "data-normalized": "{normalized:.4}",
            title: format!("{name} — {display}\nDrag · Ctrl/Shift=fine · Wheel · Dbl-click=reset · Right-click=type"),
            style: format!(
                "position:relative; width:{box_px:.1}px; height:{box_px:.1}px;"
            ),

            svg {
                style: "position:absolute; inset:0; width:100%; height:100%; display:block;",
                view_box: "-55 -55 110 110",

                // The printed scale ring: a faint band with the unit's own
                // numbers around it.
                if ticks {
                    path {
                        d: "{ring}",
                        fill: "none",
                        stroke: "{ink}",
                        stroke_width: "0.6",
                        opacity: "0.35",
                    }
                }
                // The printed dot ring — the 1073's, and the thing you see of
                // one of its knobs before any number resolves. Evenly spaced
                // along the sweep, on the panel and so not turning.
                for i in 0..dots {
                    {
                        let n = if dots > 1 {
                            i as f64 / (dots - 1) as f64
                        } else {
                            0.5
                        };
                        let (dx, dy) = ring_point(n, ring_r);
                        rsx! {
                            circle {
                                cx: "{dx:.2}", cy: "{dy:.2}", r: "1.35",
                                fill: "{ink}",
                                opacity: "0.92",
                            }
                        }
                    }
                }

                // A dial's scale is printed on its own skirt, so it is drawn
                // with the rotating parts below rather than here on the panel.
                for mark in marks.iter().filter(|_| !style.numerals_on_knob()) {
                    {
                        let (x1, y1) = ring_point(mark.normalized, ring_r - 1.0);
                        let (x2, y2) = ring_point(
                            mark.normalized,
                            if mark.major { ring_r + 5.0 } else { ring_r + 3.0 },
                        );
                        let (lx, ly) = ring_point(mark.normalized, label_r);
                        rsx! {
                            if ticks {
                            line {
                                x1: "{x1:.2}", y1: "{y1:.2}", x2: "{x2:.2}", y2: "{y2:.2}",
                                stroke: "{ink}",
                                stroke_width: if mark.major { "1.4" } else { "0.8" },
                                opacity: if mark.major { "0.9" } else { "0.55" },
                            }
                            }
                            if let Some(label) = &mark.label {
                                text {
                                    x: "{lx:.2}", y: "{ly + 2.6:.2}",
                                    fill: "{ink}", font_size: "7",
                                    font_weight: "700", text_anchor: "middle",
                                    "{label}"
                                }
                            }
                        }
                    }
                }

                // Body shadow — the knob sits proud of the panel.
                circle {
                    cx: "0", cy: "1.5", r: "{BODY_R:.1}",
                    fill: "rgba(0,0,0,0.35)",
                }
            }

            // ── The knob itself, walked off its spec ─────────────────────
            //
            // Tiers outermost-first. Each is either a CSS div (a gradient is
            // the only thing that reads as a material) or an SVG shape in a
            // rotating group. A tier turns with the collar or with the cap,
            // which on an ordinary knob is the same angle and on a
            // concentric one is not.
            for (i , tier) in spec.tiers.iter().enumerate() {
                {
                    let a = if tier.turns == Turns::Cap { inner_angle } else { angle };
                    let r = BODY_R * tier.r;
                    let px = diameter * tier.r * scale;
                    rsx! {
                        match tier.paint {
                            // A gradient surface: a div, so it can carry one.
                            Paint::Surface { css, finish, tint: tintable } => rsx! {
                                div {
                                    key: "tier-{i}",
                                    style: format!(
                                        "position:absolute; left:50%; top:50%; \
                                         width:{0:.1}px; height:{0:.1}px; \
                                         margin-left:{1:.1}px; margin-top:{1:.1}px; \
                                         border-radius:50%; background:{2}; \
                                         border:{3:.1}px solid rgba(0,0,0,0.45); \
                                         box-shadow:0 {4:.1}px {5:.1}px rgba(0,0,0,0.55), \
                                           0 {6:.1}px {7:.1}px rgba(0,0,0,0.30), \
                                           inset 0 {8:.1}px {9:.1}px rgba(0,0,0,0.45), \
                                           inset 0 {10:.1}px {11:.1}px rgba(255,255,255,0.16);",
                                        px,
                                        -px / 2.0,
                                        match (tintable, tint.as_deref()) {
                                            (true, Some(c)) => finish.tinted(c),
                                            _ => css.to_string(),
                                        },
                                        (0.5 * scale).max(0.6),
                                        1.5 * scale,
                                        3.0 * scale,
                                        4.0 * scale,
                                        10.0 * scale,
                                        -px * 0.10,
                                        px * 0.16,
                                        px * 0.05,
                                        px * 0.10,
                                    ),
                                }
                            },
                            // Flat fills and grooves are SVG, in a group that
                            // turns with whichever control owns the tier.
                            _ => rsx! {
                                svg {
                                    key: "tier-{i}",
                                    style: "position:absolute; inset:0; width:100%; \
                                            height:100%; display:block; pointer-events:none;",
                                    view_box: "-55 -55 110 110",
                                    g {
                                        transform: "rotate({a:.2})",
                                        if tier.shadow > 0.0 {
                                            match tier.edge {
                                                Edge::Toothed { teeth, depth } => rsx! {
                                                    path {
                                                        d: "{scallop_path(r, teeth, BODY_R * depth)}",
                                                        transform: "translate(0 {BODY_R * tier.shadow:.2})",
                                                        fill: "rgba(0,0,0,0.45)",
                                                    }
                                                },
                                                Edge::Round => rsx! {
                                                    circle {
                                                        cx: "0", cy: "{BODY_R * tier.shadow:.2}",
                                                        r: "{r:.2}",
                                                        fill: "rgba(0,0,0,0.45)",
                                                    }
                                                },
                                            }
                                        }
                                        {
                                            let (fill, stroke, sw) = match tier.paint {
                                                Paint::Flat(c) => (c.to_string(), None, 0.0),
                                                Paint::Tinted(fallback) => (
                                                    tint.clone().unwrap_or_else(|| fallback.to_string()),
                                                    None,
                                                    0.0,
                                                ),
                                                Paint::Groove { color, width } => {
                                                    ("none".to_string(), Some(color), width)
                                                }
                                                Paint::Surface { .. } => unreachable!(),
                                            };
                                            let (stroke, sw) = match (stroke, tier.stroke) {
                                                (Some(c), _) => (c.to_string(), sw),
                                                (None, Some((c, w))) => (c.to_string(), w),
                                                (None, None) => ("none".to_string(), 0.0),
                                            };
                                            rsx! {
                                                match tier.edge {
                                                    Edge::Toothed { teeth, depth } => rsx! {
                                                        path {
                                                            d: "{scallop_path(r, teeth, BODY_R * depth)}",
                                                            fill: "{fill}",
                                                            stroke: "{stroke}",
                                                            stroke_width: "{sw:.2}",
                                                        }
                                                    },
                                                    Edge::Round => rsx! {
                                                        circle {
                                                            cx: "0", cy: "0", r: "{r:.2}",
                                                            fill: "{fill}",
                                                            stroke: "{stroke}",
                                                            stroke_width: "{sw:.2}",
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                        }
                    }
                }
            }

            // The room's light. Outside every rotating group on purpose: a
            // reflection that turns with the knob is the first thing that
            // reads as wrong, because the lamp above the rack does not move
            // when you turn a control.
            if let Some(lit) = spec.specular {
                div {
                    "data-testid": "hw-knob-{testid}-light",
                    style: format!(
                        "position:absolute; left:50%; top:50%; \
                         width:{:.1}px; height:{:.1}px; \
                         margin-left:{:.1}px; margin-top:{:.1}px; \
                         border-radius:50%; pointer-events:none; \
                         background:{}; transform:rotate({:.1}deg);",
                        diameter * lit.w * scale,
                        diameter * lit.h * scale,
                        diameter * lit.dx * scale,
                        diameter * lit.dy * scale,
                        lit.fill,
                        lit.rotate,
                    ),
                }
            }

            // ── Everything that turns: knurl, numerals, indices ──────────
            svg {
                style: "position:absolute; inset:0; width:100%; height:100%; \
                        display:block; pointer-events:none;",
                view_box: "-55 -55 110 110",

                if let Some(lit) = spec.specular {
                    if let Some(rim) = lit.rim {
                        path {
                            d: "M {-BODY_R * 0.72:.2} {-BODY_R * 0.58:.2} \
                                A {BODY_R * 0.93:.2} {BODY_R * 0.93:.2} 0 0 1 \
                                {BODY_R * 0.40:.2} {-BODY_R * 0.84:.2}",
                            fill: "none",
                            stroke: "{rim}",
                            stroke_width: "1.0",
                        }
                    }
                }

                // A dial's numerals: printed around its own skirt and turning
                // with it. The reading is where they line up with the panel's
                // index, not where a pointer lands.
                if spec.numerals_on_knob {
                    g {
                        transform: "rotate({angle:.2})",
                        for mark in marks.iter() {
                            {
                                let (lx, ly) = ring_point(mark.normalized, BODY_R - 6.0);
                                rsx! {
                                    if let Some(label) = &mark.label {
                                        text {
                                            x: "{lx:.2}", y: "{ly + 2.2:.2}",
                                            fill: "#1a1c1f", font_size: "6.5",
                                            font_weight: "700", text_anchor: "middle",
                                            "{label}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // The knurl, on whichever band and whichever half owns it.
                if let Some(fl) = spec.flutes {
                    {
                        let fa = if fl.turns == Turns::Cap { inner_angle } else { angle };
                        rsx! {
                    g {
                        transform: "rotate({fa:.2})",
                        for i in 0..fl.count {
                            {
                                let count = fl.count as f64;
                                let a = (i as f64 / count) * std::f64::consts::TAU;
                                // Half a flute over, for the shadowed side.
                                let b = a + std::f64::consts::TAU / (count * 2.0);
                                let (sx, sy) = (a.sin(), -a.cos());
                                let (bx, by) = (b.sin(), -b.cos());
                                let (inner, outer) = (BODY_R * fl.from, BODY_R * fl.to);
                                rsx! {
                                    line {
                                        x1: "{sx * inner:.2}", y1: "{sy * inner:.2}",
                                        x2: "{sx * outer:.2}", y2: "{sy * outer:.2}",
                                        stroke: "{fl.stroke}",
                                        stroke_width: "{fl.width:.2}",
                                    }
                                    if let Some(dark) = fl.shadow {
                                        line {
                                            x1: "{bx * inner:.2}", y1: "{by * inner:.2}",
                                            x2: "{bx * outer:.2}", y2: "{by * outer:.2}",
                                            stroke: "{dark}",
                                            stroke_width: "{fl.width:.2}",
                                        }
                                    }
                                }
                            }
                        }
                    }
                        }
                    }
                }

                // The collar's index, where the two halves are separate
                // controls. Drawn before the cap's so the cap's sits on top.
                if let Some(idx) = spec.collar_index {
                    g {
                        transform: "rotate({angle:.2})",
                        {draw_index(idx, tint.as_deref())}
                    }
                }

                // The cap's index — what an ordinary knob is read by.
                g {
                    transform: "rotate({inner_angle:.2})",
                    {draw_index(spec.index, tint.as_deref())}
                }

                // The hub, over everything and turning with nothing.
                if let Some(hub) = spec.hub {
                    circle { cx: "0", cy: "0", r: "4.5", fill: "{hub}" }
                }
            }


            // The panel's index, which a dial's moving numerals are read
            // against. Fixed, unlike everything else on the knob.
            if style.numerals_on_knob() {
                svg {
                    style: "position:absolute; inset:0; width:100%; height:100%; \
                            display:block; pointer-events:none;",
                    view_box: "-55 -55 110 110",
                    polygon {
                        points: "0,{-(BODY_R + 6.0):.1} -3.2,{-(BODY_R + 12.0):.1} 3.2,{-(BODY_R + 12.0):.1}",
                        fill: "#e8eaec",
                    }
                }
            }

            // A faceplate knob has no printed readout; right-click / Enter
            // pops one up over the cap for typing a value
            // (`fx.control.text-entry`).
            if *readout_open.read() {
                div {
                    style: "position:absolute; left:50%; bottom:-4px; transform:translateX(-50%); z-index:5;",
                    ValueReadout {
                        handle: handle.clone(),
                        open: readout_open,
                        testid: "hw-knob-{testid}",
                    }
                }
            }

            // Interaction overlay — the shared gesture layer, like every FTS
            // control (`fx.control.shared-gesture`).
            div {
                style: "position:absolute; inset:0; cursor:ns-resize; user-select:none;",
                tabindex: "0",
                onmousedown: {
                    let handle = handle.clone();
                    move |evt: MouseEvent| {
                        if gesture::press_vertical(&evt, &mut drag, &handle, SENSITIVITY) == Press::Menu {
                            readout_open.set(true);
                        }
                    }
                },
                ondoubleclick: {
                    let handle = handle.clone();
                    move |_| gesture::double_click(&mut drag, &handle)
                },
                onwheel: {
                    let handle = handle.clone();
                    move |evt: WheelEvent| gesture::wheel(&evt, &handle)
                },
                onkeydown: {
                    let handle = handle.clone();
                    move |evt: KeyboardEvent| {
                        if gesture::key(&evt, &handle) == gesture::KeyOutcome::OpenTextEntry {
                            readout_open.set(true);
                        }
                    }
                },
            }

            // The cap's own drag region, on a dual-concentric knob.
            //
            // Laid over the collar's overlay and sized to the cap, so the two
            // controls are hit exactly where they are drawn: press the grey
            // middle and you have the inner control, press the bright ring
            // around it and the press falls through to the outer one. That is
            // how the real thing is operated, and it needs no modifier key.
            if let Some(inner) = &inner_handle {
                div {
                    "data-testid": "hw-knob-{testid}-inner",
                    style: format!(
                        "position:absolute; left:50%; top:50%; \
                         width:{0:.1}px; height:{0:.1}px; \
                         margin-left:{1:.1}px; margin-top:{1:.1}px; \
                         border-radius:50%; cursor:ns-resize; user-select:none;",
                        body_px,
                        -body_px / 2.0,
                    ),
                    onmousedown: {
                        let inner = inner.clone();
                        move |evt: MouseEvent| {
                            evt.stop_propagation();
                            let _ = gesture::press_vertical(&evt, &mut drag, &inner, SENSITIVITY);
                        }
                    },
                    ondoubleclick: {
                        let inner = inner.clone();
                        move |evt: MouseEvent| {
                            evt.stop_propagation();
                            gesture::double_click(&mut drag, &inner);
                        }
                    },
                    onwheel: {
                        let inner = inner.clone();
                        move |evt: WheelEvent| {
                            evt.stop_propagation();
                            gesture::wheel(&evt, &inner);
                        }
                    },
                }
            }
        }
    }
}
