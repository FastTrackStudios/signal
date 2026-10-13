//! What a hardware knob is *made of*, as data — ported from
//! `fts-audio-ui::hardware::{knob_kit, knob_parts}`.
//!
//! A knob is a short stack of concentric **tiers** (outermost first),
//! sometimes a **knurl** cut into one band, an **index** you read the value
//! by, and a **highlight** where the room's light falls. Every unit's knob
//! is that same anatomy with different numbers, so a style is one
//! [`KnobSpec`] and the builder in `mod.rs` walks it without knowing which
//! unit it is drawing.
//!
//! Radii are **fractions of the knob's outer radius**; widths and offsets
//! marked "units" are in the source kit's design space, where the knob body
//! has radius [`BODY_R`] (30) — so a spec is independent of the diameter a
//! panel asks for. Colours are the source's CSS literals (`#hex` or
//! `rgba(…)`).

/// The knob body's radius in design units (the source's `-55 -55 110 110`
/// viewBox).
pub const BODY_R: f64 = 30.0;

/// A tier's outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edge {
    Round,
    /// Rounded lobes: the Pultec's scalloped skirt, the 1073's geared cap.
    /// `depth` is a fraction of the knob's outer radius.
    Toothed {
        teeth: u32,
        depth: f64,
    },
}

/// How a surface takes the light — what makes it read as a material. Only
/// the hue moves when a control is tinted; the gradient's *shape* is the
/// material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    /// A dome: moulded plastic, lit from the upper left.
    Moulded,
    /// A flat top lit as a face rather than a sphere.
    FlatTop,
    /// Painted metal: an even face with the light only at the very top.
    Matte,
    /// Turned or brushed metal: two bright lobes opposite each other.
    Brushed,
    /// Soft-touch rubber: dead flat middle, a diffuse ring at the edge.
    SoftTouch,
}

impl Finish {
    /// How hard the tier's edge is shaded, as `(under, lip)` alphas.
    #[must_use]
    pub const fn relief(self) -> (f64, f64) {
        match self {
            Self::Moulded => (0.45, 0.16),
            Self::Brushed => (0.30, 0.12),
            Self::Matte => (0.20, 0.08),
            Self::FlatTop => (0.16, 0.07),
            Self::SoftTouch => (0.12, 0.05),
        }
    }
}

/// A gradient, in the source's CSS terms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Grad {
    /// `linear-gradient(<angle>deg, …)`.
    Linear(f64, &'static [(f64, &'static str)]),
    /// `radial-gradient(circle at <x> <y>, …)` — farthest-corner sizing.
    Radial(f64, f64, &'static [(f64, &'static str)]),
    /// The kit's turned aluminium: glint, anisotropic lobes, metal.
    Brushed,
}

/// How a tier is painted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TierPaint {
    /// A lit material. `tint`: the variant's colour replaces the gradient
    /// (through the finish's own gradient shape).
    Surface {
        grad: Grad,
        finish: Finish,
        tint: bool,
    },
    /// A flat fill — a shadowed step between tiers.
    Flat(&'static str),
    /// Stroke only: a turned groove.
    Groove { color: &'static str, width: f64 },
}

/// One concentric tier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tier {
    pub r: f64,
    pub edge: Edge,
    pub paint: TierPaint,
    /// A contact shadow dropped this far below the tier (fraction of the
    /// outer radius). Zero for none.
    pub shadow: f64,
    /// An outline: `(colour, width in units)`.
    pub stroke: Option<(&'static str, f64)>,
}

impl Tier {
    const fn new(r: f64, paint: TierPaint) -> Self {
        Self {
            r,
            edge: Edge::Round,
            paint,
            shadow: 0.0,
            stroke: None,
        }
    }
    const fn surface(r: f64, grad: Grad, finish: Finish, tint: bool) -> Self {
        Self::new(r, TierPaint::Surface { grad, finish, tint })
    }
    const fn flat(r: f64, color: &'static str) -> Self {
        Self::new(r, TierPaint::Flat(color))
    }
    const fn groove(r: f64, color: &'static str, width: f64) -> Self {
        Self::new(r, TierPaint::Groove { color, width })
    }
    const fn toothed(mut self, teeth: u32, depth: f64) -> Self {
        self.edge = Edge::Toothed { teeth, depth };
        self
    }
    const fn shadowed(mut self, drop: f64) -> Self {
        self.shadow = drop;
        self
    }
    const fn outlined(mut self, color: &'static str, width: f64) -> Self {
        self.stroke = Some((color, width));
        self
    }
}

/// What the value is read by. Everything here turns with the value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Index {
    /// A bar between two radii (fractions) — most knobs. `width` in units.
    Bar {
        from: f64,
        to: f64,
        width: f64,
        color: &'static str,
    },
    /// A tapered blade running out from the hub.
    Blade {
        to: f64,
        half_width: f64,
        color: &'static str,
    },
    /// A Marconi wing: a coloured moulding laid across the knob and
    /// overhanging it, with a line down the middle. `body` = the disc the
    /// wing's width is proportioned to.
    Wing { color: &'static str, body: f64 },
    /// A pointer knob's moulded nose, reaching past the body.
    Nose { color: &'static str },
    /// A pointer that widens toward the rim (Rogan PT). Half-widths in units.
    Taper {
        from: f64,
        to: f64,
        hub_half: f64,
        tip_half: f64,
        color: &'static str,
    },
    /// A chicken-head: the moulding is the pointer. `color` is the line on
    /// the beak; the moulding takes the variant colour.
    Beak { color: &'static str, body: f64 },
    /// No pointer: the numerals on the skirt turn past a fixed panel mark.
    None,
}

/// A knurl cut into one band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flutes {
    pub count: u32,
    pub from: f64,
    pub to: f64,
    /// The lit ridge.
    pub stroke: &'static str,
    /// Ridge width, units.
    pub width: f64,
    /// The shadowed side of each ridge, half a flute over.
    pub shadow: Option<&'static str>,
}

/// How the room's light sits on the face (fixed — it does not turn).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpecFill {
    /// `radial-gradient(ellipse at 50% 50%, …)`.
    Ellipse(&'static [(f64, &'static str)]),
    /// `linear-gradient(<angle>deg, …)`.
    Linear(f64, &'static [(f64, &'static str)]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Specular {
    /// Size and centre offset, fractions of the knob's **diameter**.
    pub w: f64,
    pub h: f64,
    pub dx: f64,
    pub dy: f64,
    pub fill: SpecFill,
    pub rotate: f64,
    /// A hairline of the same light along the lit tier's top rim.
    pub rim: Option<&'static str>,
    /// The lit tier's radius (fraction of the knob).
    pub r: f64,
}

/// The soft highlight on a moulded or turned surface, sized to the lit tier.
const fn dome(body: f64) -> Specular {
    Specular {
        w: 0.62 * body,
        h: 0.30 * body,
        dx: -0.02 * body,
        dy: -0.24 * body,
        fill: SpecFill::Ellipse(&[
            (0.0, "rgba(255,255,255,0.15)"),
            (0.52, "rgba(255,255,255,0.06)"),
            (0.80, "rgba(255,255,255,0.0)"),
        ]),
        rotate: 0.0,
        rim: Some("rgba(255,255,255,0.20)"),
        r: body,
    }
}

/// The light on a *matte* part: almost nothing, and a thread along the top.
const fn matte_light(r: f64) -> Specular {
    Specular {
        w: 0.66 * r,
        h: 0.26 * r,
        dx: -0.01 * r,
        dy: -0.22 * r,
        fill: SpecFill::Ellipse(&[
            (0.0, "rgba(255,255,255,0.10)"),
            (0.54, "rgba(255,255,255,0.035)"),
            (0.82, "rgba(255,255,255,0.0)"),
        ]),
        rotate: 0.0,
        rim: Some("rgba(255,255,255,0.17)"),
        r,
    }
}

/// How the panel prints the scale around the knob (the source's `rack::Ring`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingKind {
    /// A faint arc with tick marks (major at each label, one minor between)
    /// and numerals outside — the compressor faces.
    Ticks,
    /// 21 dots along the sweep with numerals just outside — the 1073 / SSL.
    Dots,
    /// Numerals only, hugging the skirt — the Pultec.
    Numerals,
    /// Numerals printed on the knob's own skirt, turning with it; a fixed
    /// index on the panel — the Distressor.
    OnKnob,
}

impl RingKind {
    /// `(tick radius, numeral radius)` in units.
    #[must_use]
    pub const fn geometry(self) -> (f64, f64) {
        match self {
            Self::Numerals => (31.0, 37.0),
            Self::Dots => (38.0, 48.0),
            Self::Ticks | Self::OnKnob => (41.0, 50.0),
        }
    }
}

/// A knob, as data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KnobSpec {
    /// Outermost first; a later tier paints over an earlier one.
    pub tiers: &'static [Tier],
    pub index: Index,
    /// The 1073's second index, on the collar.
    pub collar_index: Option<Index>,
    pub flutes: Option<Flutes>,
    pub specular: Option<Specular>,
    /// How far out (units) the printed scale moves to clear an overhang.
    pub ring_offset: f64,
    /// A small fixed shadow at the centre — the shaft.
    pub hub: Option<&'static str>,
    /// Travel in degrees, centred on straight up.
    pub sweep: f64,
    /// The printed scale this style's panel carries when `Place::scale` is
    /// not given.
    pub ring: RingKind,
    pub scale: &'static [&'static str],
    /// Colour variant names; the first is the default.
    pub variants: &'static [&'static str],
    /// Each variant's tint, in the same order; `None` keeps the style's own
    /// colours.
    pub tints: &'static [Option<&'static str>],
}

/// Turned aluminium (see [`Grad::Brushed`]).
const BRUSHED: Grad = Grad::Brushed;
const LIGHT: &str = "#f2f2f0";
const DARK: &str = "#1c1c1e";
const HUB: Option<&str> = Some("rgba(0,0,0,0.35)");
/// Every hardware pot in the source kit travels 300° (−150°…+150°).
const SWEEP: f64 = 300.0;
const ZERO_TEN: &[&str] = &["0", "2", "4", "6", "8", "10"];

/// A knob that is one surface all the way out.
const fn solid(grad: Grad, finish: Finish) -> Tier {
    Tier::surface(1.0, grad, finish, true)
}

// ── Bakelite — the LA-2A / 1176 knob: black, a white line ────────────────
pub static BAKELITE: KnobSpec = KnobSpec {
    tiers: &[
        // The base, a hair wider and much darker, with a contact shadow: the
        // pair reads as a truncated cone standing on the panel.
        Tier::flat(1.0, "#070709")
            .shadowed(0.05)
            .outlined("rgba(255,255,255,0.09)", 0.7),
        // The cone's wall: top-lit, matte.
        Tier::surface(
            0.94,
            Grad::Linear(
                178.0,
                &[
                    (0.0, "#34343b"),
                    (0.42, "#212126"),
                    (0.78, "#131317"),
                    (1.0, "#0b0b0e"),
                ],
            ),
            Finish::Matte,
            true,
        ),
        // The step up to the top face, as a shadowed wall.
        Tier::flat(0.76, "rgba(0,0,0,0.5)"),
        Tier::surface(
            0.73,
            Grad::Linear(
                178.0,
                &[(0.0, "#3c3c44"), (0.46, "#26262c"), (1.0, "#16161a")],
            ),
            Finish::FlatTop,
            true,
        ),
    ],
    index: Index::Bar {
        from: 0.05,
        to: 0.88,
        width: 3.2,
        color: LIGHT,
    },
    collar_index: None,
    flutes: None,
    specular: Some(matte_light(0.94)),
    ring_offset: 0.0,
    hub: None,
    sweep: SWEEP,
    ring: RingKind::Ticks,
    scale: ZERO_TEN,
    variants: &["black", "brown", "red"],
    tints: &[None, Some("#3b2a20"), Some("#7a1f1c")],
};

// ── Metal — brushed, with a dark blade ───────────────────────────────────
pub static METAL: KnobSpec = KnobSpec {
    tiers: &[
        Tier::flat(1.0, "#3a3a38").shadowed(0.05),
        Tier::surface(0.93, BRUSHED, Finish::Brushed, true).outlined("rgba(255,255,255,0.16)", 0.7),
    ],
    index: Index::Blade {
        to: 0.83,
        half_width: 0.113,
        color: DARK,
    },
    collar_index: None,
    flutes: None,
    specular: Some(dome(1.0)),
    ring_offset: 0.0,
    hub: HUB,
    sweep: SWEEP,
    ring: RingKind::Ticks,
    scale: &["0", "", "", "", "", "10"],
    variants: &["silver", "gold", "black"],
    tints: &[None, Some("#b59a5a"), Some("#3c3d41")],
};

// ── Skirted — Davies 1910/1913: a wide skirt, a ribbed body, a flat top ──
pub static SKIRTED: KnobSpec = KnobSpec {
    tiers: &[
        Tier::flat(1.0, "#0a0a0c")
            .shadowed(0.055)
            .outlined("rgba(255,255,255,0.10)", 0.8),
        Tier::surface(
            0.955,
            Grad::Linear(
                178.0,
                &[
                    (0.0, "#33333a"),
                    (0.44, "#212127"),
                    (0.76, "#141418"),
                    (1.0, "#0d0d10"),
                ],
            ),
            Finish::Matte,
            true,
        ),
        // The shadow the body drops onto the skirt — the step is the read.
        Tier::flat(0.70, "rgba(0,0,0,0.55)"),
        Tier::surface(
            0.66,
            Grad::Linear(
                178.0,
                &[(0.0, "#3d3d45"), (0.46, "#26262c"), (1.0, "#17171b")],
            ),
            Finish::Matte,
            true,
        ),
        Tier::surface(
            0.50,
            Grad::Linear(
                178.0,
                &[(0.0, "#47474f"), (0.50, "#2e2e35"), (1.0, "#1c1c21")],
            ),
            Finish::FlatTop,
            false,
        ),
    ],
    // Down the body and out over the skirt, to the numbers on the panel.
    index: Index::Bar {
        from: 0.30,
        to: 0.955,
        width: 3.0,
        color: LIGHT,
    },
    collar_index: None,
    flutes: Some(Flutes {
        count: 30,
        from: 0.52,
        to: 0.66,
        stroke: "rgba(255,255,255,0.11)",
        width: 0.8,
        shadow: Some("rgba(0,0,0,0.34)"),
    }),
    specular: Some(matte_light(0.955)),
    ring_offset: 0.0,
    hub: None,
    sweep: SWEEP,
    ring: RingKind::Ticks,
    scale: ZERO_TEN,
    variants: &["black", "brown"],
    tints: &[None, Some("#3b2a20")],
};

// ── Daka-Ware — the Pultec EQP-1A's ──────────────────────────────────────
pub static DAKA: KnobSpec = KnobSpec {
    tiers: &[
        Tier::surface(
            1.0,
            Grad::Radial(
                0.36,
                0.22,
                &[(0.0, "#2a2a30"), (0.46, "#17171b"), (1.0, "#0c0c0f")],
            ),
            Finish::Moulded,
            false,
        )
        .toothed(22, 0.08)
        .shadowed(0.06)
        .outlined("rgba(0,0,0,0.7)", 0.7),
        Tier::flat(0.74, "#08080a").outlined("rgba(255,255,255,0.11)", 0.6),
        Tier::surface(
            0.70,
            Grad::Radial(
                0.34,
                0.24,
                &[(0.0, "#3d3d45"), (0.48, "#24242a"), (1.0, "#131317")],
            ),
            Finish::Moulded,
            false,
        )
        .outlined("rgba(255,255,255,0.11)", 0.7),
        Tier::surface(
            0.52,
            Grad::Radial(
                0.34,
                0.24,
                &[(0.0, "#46464f"), (0.52, "#2b2b32"), (1.0, "#191920")],
            ),
            Finish::Moulded,
            false,
        )
        .outlined("rgba(255,255,255,0.11)", 0.6),
        // The dome on top, lit hardest.
        Tier::surface(
            0.30,
            Grad::Radial(
                0.32,
                0.22,
                &[(0.0, "#5a5a64"), (0.56, "#34343c"), (1.0, "#1d1d24")],
            ),
            Finish::Moulded,
            false,
        ),
    ],
    // Bold: the only thing a Pultec's printed 0–10 is read against.
    index: Index::Bar {
        from: 0.24,
        to: 0.96,
        width: 3.2,
        color: "#eceae4",
    },
    collar_index: None,
    flutes: Some(Flutes {
        count: 40,
        from: 0.60,
        to: 0.70,
        stroke: "rgba(255,255,255,0.10)",
        width: 0.7,
        shadow: None,
    }),
    specular: Some(Specular {
        w: 0.46,
        h: 0.30,
        dx: -0.11,
        dy: -0.15,
        fill: SpecFill::Ellipse(&[
            (0.0, "rgba(255,255,255,0.13)"),
            (0.72, "rgba(255,255,255,0.0)"),
        ]),
        rotate: -32.0,
        rim: Some("rgba(255,255,255,0.12)"),
        r: 1.0,
    }),
    ring_offset: 0.0,
    hub: HUB,
    sweep: SWEEP,
    ring: RingKind::Numerals,
    scale: &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"],
    variants: &["black"],
    tints: &[None],
};

// ── Marconi — the 1073's gain switch and high-pass: a coloured wing ──────
pub static MARCONI: KnobSpec = KnobSpec {
    tiers: &[
        Tier::surface(
            1.0,
            Grad::Radial(
                0.36,
                0.22,
                &[(0.0, "#4e4e55"), (0.40, "#232328"), (1.0, "#0f0f12")],
            ),
            Finish::Moulded,
            false,
        ),
        Tier::surface(
            0.72,
            Grad::Radial(
                0.36,
                0.22,
                &[(0.0, "#3c3c40"), (0.46, "#1e1e21"), (1.0, "#121214")],
            ),
            Finish::Moulded,
            true,
        ),
    ],
    index: Index::Wing {
        color: LIGHT,
        body: 0.72,
    },
    collar_index: None,
    flutes: None,
    specular: Some(dome(0.72)),
    ring_offset: 5.0,
    hub: HUB,
    sweep: SWEEP,
    ring: RingKind::Dots,
    scale: &["0", "2", "4", "6", "8", "10"],
    // Neve's colour coding: red gain, blue filters, grey shelf.
    variants: &["red", "blue", "grey", "black"],
    tints: &[Some("#9c1f27"), Some("#2b4a6d"), Some("#7f858c"), None],
};

// ── Collet — the SSL 4000 channel: flat cap, fluted rim, white bar ───────
pub static COLLET: KnobSpec = KnobSpec {
    tiers: &[solid(
        Grad::Linear(
            162.0,
            &[(0.0, "#4a4a4e"), (0.42, "#303034"), (1.0, "#202024")],
        ),
        Finish::FlatTop,
    )],
    index: Index::Bar {
        from: 0.06,
        to: 0.97,
        width: 3.4,
        color: LIGHT,
    },
    collar_index: None,
    flutes: Some(Flutes {
        count: 28,
        from: 0.86,
        to: 1.0,
        stroke: "rgba(0,0,0,0.42)",
        width: 1.2,
        shadow: Some("rgba(0,0,0,0.40)"),
    }),
    specular: Some(Specular {
        w: 1.0,
        h: 1.0,
        dx: 0.0,
        dy: 0.0,
        fill: SpecFill::Linear(
            152.0,
            &[
                (0.0, "rgba(255,255,255,0.22)"),
                (0.34, "rgba(255,255,255,0.07)"),
                (0.62, "rgba(255,255,255,0.0)"),
            ],
        ),
        rotate: 0.0,
        rim: None,
        r: 1.0,
    }),
    ring_offset: 0.0,
    hub: None,
    sweep: SWEEP,
    ring: RingKind::Dots,
    scale: &["-", "", "", "", "", "0", "", "", "", "", "+"],
    // The 4000's band colours, as the source's SSL face codes them.
    variants: &["grey", "black", "cream", "red", "blue", "green", "magenta"],
    tints: &[
        None,
        Some("#2b2d31"),
        Some("#d8d4c6"),
        Some("#b3302b"),
        Some("#2b7fc0"),
        Some("#2c8f5a"),
        Some("#a8438f"),
    ],
};

// ── Silver-top — the UREI 1176: black collar, brushed knurled cap ────────
pub static SILVER_TOP: KnobSpec = KnobSpec {
    tiers: &[
        Tier::surface(
            1.0,
            Grad::Radial(
                0.38,
                0.28,
                &[
                    (0.0, "#35353a"),
                    (0.46, "#1c1c20"),
                    (0.78, "#101013"),
                    (1.0, "#0a0a0c"),
                ],
            ),
            Finish::Moulded,
            false,
        ),
        // The cap: half the knob — a dark annulus around a bright disc.
        Tier::surface(0.50, BRUSHED, Finish::Brushed, true).outlined("rgba(0,0,0,0.55)", 0.8),
        // Two turned rings on the brushed face.
        Tier::groove(0.310, "rgba(0,0,0,0.16)", 0.7),
        Tier::groove(0.150, "rgba(0,0,0,0.13)", 0.6),
    ],
    // On the collar, cap edge to rim — outside the cap, not across it.
    index: Index::Bar {
        from: 0.56,
        to: 0.97,
        width: 2.8,
        color: "#f4f4f2",
    },
    collar_index: None,
    flutes: Some(Flutes {
        count: 54,
        from: 0.42,
        to: 0.50,
        stroke: "rgba(0,0,0,0.34)",
        width: 1.2,
        shadow: Some("rgba(0,0,0,0.40)"),
    }),
    specular: Some(dome(0.50)),
    ring_offset: 0.0,
    hub: HUB,
    sweep: SWEEP,
    ring: RingKind::Dots,
    scale: &["∞", "48", "36", "30", "24", "18", "12", "6", "0"],
    variants: &["silver", "gold"],
    tints: &[None, Some("#b59a5a")],
};

// ── Metal-fluted — dbx and its generation ────────────────────────────────
pub static METAL_FLUTED: KnobSpec = KnobSpec {
    tiers: &[
        solid(BRUSHED, Finish::Brushed),
        Tier::flat(0.42, "#3a3c40").outlined("rgba(0,0,0,0.5)", 0.8),
    ],
    index: Index::Bar {
        from: 0.16,
        to: 0.93,
        width: 2.6,
        color: LIGHT,
    },
    collar_index: None,
    flutes: Some(Flutes {
        count: 40,
        from: 0.86,
        to: 1.0,
        stroke: "rgba(0,0,0,0.38)",
        width: 1.2,
        shadow: Some("rgba(0,0,0,0.40)"),
    }),
    specular: Some(dome(1.0)),
    ring_offset: 0.0,
    hub: HUB,
    sweep: SWEEP,
    ring: RingKind::Ticks,
    scale: ZERO_TEN,
    variants: &["silver", "gold", "black"],
    tints: &[None, Some("#b59a5a"), Some("#3c3d41")],
};

// ── Pointer — Teletronix LA-2A: a plain black knob with a moulded nose ───
pub static POINTER: KnobSpec = KnobSpec {
    tiers: &[
        Tier::flat(1.0, "#08080a")
            .shadowed(0.05)
            .outlined("rgba(255,255,255,0.08)", 0.7),
        Tier::surface(
            0.94,
            Grad::Linear(
                178.0,
                &[
                    (0.0, "#33333a"),
                    (0.44, "#212127"),
                    (0.78, "#131317"),
                    (1.0, "#0c0c0f"),
                ],
            ),
            Finish::Matte,
            true,
        ),
    ],
    index: Index::Nose { color: LIGHT },
    collar_index: None,
    flutes: None,
    specular: Some(matte_light(0.94)),
    // The nose reaches past the body by design.
    ring_offset: 13.0,
    hub: HUB,
    sweep: SWEEP,
    ring: RingKind::Ticks,
    scale: ZERO_TEN,
    variants: &["black", "brown"],
    tints: &[None, Some("#3b2a20")],
};

// ── Rogan PT — the modular knob: soft-touch rubber, a widening pointer ───
pub static SOFT_POINTER: KnobSpec = KnobSpec {
    tiers: &[
        Tier::flat(1.0, "#08080a")
            .shadowed(0.055)
            .outlined("rgba(255,255,255,0.08)", 0.8),
        Tier::surface(
            0.94,
            Grad::Radial(
                0.5,
                0.5,
                &[
                    (0.0, "#26262b"),
                    (0.62, "#26262b"),
                    (0.88, "#34343b"),
                    (1.0, "#17171b"),
                ],
            ),
            Finish::SoftTouch,
            true,
        ),
        // The flat top, a hair proud of it — one moulding, not a stack.
        Tier::surface(
            0.80,
            Grad::Radial(
                0.5,
                0.5,
                &[
                    (0.0, "#2c2c32"),
                    (0.66, "#2c2c32"),
                    (0.90, "#3a3a42"),
                    (1.0, "#202026"),
                ],
            ),
            Finish::SoftTouch,
            true,
        ),
    ],
    index: Index::Taper {
        from: 0.06,
        to: 0.86,
        hub_half: 1.5,
        tip_half: 3.6,
        color: "#f2f2ef",
    },
    collar_index: None,
    flutes: None,
    // Rubber takes no gloss blob; the tiers' own edge rings are the light.
    specular: None,
    ring_offset: 0.0,
    hub: None,
    sweep: SWEEP,
    ring: RingKind::Ticks,
    scale: &["", "", "", "", "", "", "", "", "", "", ""],
    variants: &["black", "grey", "white", "red", "blue"],
    tints: &[
        None,
        Some("#5a5d63"),
        Some("#dcdcd6"),
        Some("#a8322c"),
        Some("#2f5f9e"),
    ],
};

// ── Chicken-head — tweed Fender, Ampeg, EMS, the meter selector ──────────
pub static CHICKEN_HEAD: KnobSpec = KnobSpec {
    tiers: &[
        // The hub it turns on: small and dark.
        Tier::flat(0.44, "#0b0b0d").outlined("rgba(255,255,255,0.07)", 0.7),
        Tier::surface(
            0.38,
            Grad::Linear(
                178.0,
                &[(0.0, "#2c2c32"), (0.46, "#1c1c21"), (1.0, "#101014")],
            ),
            Finish::Matte,
            false,
        ),
    ],
    index: Index::Beak {
        color: "#2a2621",
        body: 0.46,
    },
    collar_index: None,
    flutes: None,
    specular: None,
    ring_offset: 8.0,
    hub: None,
    sweep: SWEEP,
    ring: RingKind::Ticks,
    scale: &["1", "2", "3", "4", "5"],
    variants: &["cream", "black", "red"],
    tints: &[None, Some("#1f1f23"), Some("#8e2320")],
};

// ── Neve — the 1073 module: a smooth ring around a geared cap ────────────
pub static NEVE: KnobSpec = KnobSpec {
    tiers: &[
        // Painted metal, deliberately flat — brushed reads as chrome here.
        Tier::surface(
            1.0,
            Grad::Linear(
                160.0,
                &[(0.0, "#c8ccd1"), (0.38, "#b4b9bf"), (1.0, "#979da4")],
            ),
            Finish::Matte,
            false,
        )
        .shadowed(0.053)
        .outlined("rgba(0,0,0,0.42)", 0.8),
        Tier::groove(0.893, "rgba(0,0,0,0.13)", 0.8),
        // The GEARED cap — which part is toothed is the whole tell.
        Tier::surface(
            0.62,
            Grad::Linear(
                160.0,
                &[(0.0, "#5e646c"), (0.40, "#4e535a"), (1.0, "#383c42")],
            ),
            Finish::Matte,
            true,
        )
        .toothed(16, 0.095)
        .shadowed(0.04)
        .outlined("rgba(0,0,0,0.62)", 1.1),
        // The flat of the cap inside the teeth.
        Tier::flat(0.527, "rgba(255,255,255,0.06)"),
    ],
    // Out at the cap's teeth, where it reads against the ring.
    index: Index::Bar {
        from: 0.248,
        to: 0.633,
        width: 3.4,
        color: "#f4f4f2",
    },
    // Slim and dark, on the ring.
    collar_index: Some(Index::Bar {
        from: 0.747,
        to: 0.967,
        width: 2.2,
        color: "#2c3036",
    }),
    flutes: None,
    specular: None,
    ring_offset: 0.0,
    hub: HUB,
    sweep: SWEEP,
    ring: RingKind::Dots,
    scale: &["-16", "-8", "0", "+8", "+16"],
    variants: &["grey", "red", "blue", "green"],
    tints: &[None, Some("#9c1f27"), Some("#2b4a6d"), Some("#2c6a4a")],
};

// ── Dial — the Empirical Labs Distressor: numerals on the skirt ──────────
pub static DIAL: KnobSpec = KnobSpec {
    tiers: &[
        Tier::surface(1.0, BRUSHED, Finish::Brushed, false),
        Tier::surface(
            0.58,
            Grad::Radial(
                0.38,
                0.28,
                &[(0.0, "#55575c"), (0.46, "#303236"), (1.0, "#1c1e21")],
            ),
            Finish::Moulded,
            true,
        ),
    ],
    index: Index::None,
    collar_index: None,
    flutes: Some(Flutes {
        count: 72,
        from: 0.44,
        to: 0.58,
        stroke: "rgba(0,0,0,0.30)",
        width: 1.2,
        shadow: Some("rgba(0,0,0,0.40)"),
    }),
    specular: Some(dome(0.58)),
    ring_offset: 0.0,
    hub: HUB,
    sweep: SWEEP,
    ring: RingKind::OnKnob,
    scale: &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"],
    variants: &["black", "blue", "red"],
    tints: &[None, Some("#2b4a6d"), Some("#7a1f1c")],
};
