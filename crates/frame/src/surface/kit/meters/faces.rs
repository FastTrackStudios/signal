//! Every VU face and every frame, as data — ported from
//! `fts-audio-ui::hardware::{vu_faces, vu_kit}`.
//!
//! A period movement (Weston, Modutec, Sifam) is an **ivory card with a black
//! scale and a red stretch above 0**, lit from behind by a bulb or two; the
//! lamp is what varies between units, not the card. The frame is a separate
//! part: the same movement sits in a pressed-steel rack frame on one unit and
//! a chromed ring on another, so a face names a default frame and a variant
//! can ask for another.
//!
//! Colours are the source's CSS values verbatim (`#hex` or `rgba(…)`); the
//! builder parses them.

use super::scale::Card;

/// An angled linear gradient (CSS angle, stops in CSS colours).
#[derive(Clone, Copy, Debug)]
pub struct Lin {
    pub angle: f64,
    pub stops: &'static [(f64, &'static str)],
}

/// `radial-gradient(ellipse at x% y%, …)` — farthest-corner sized.
#[derive(Clone, Copy, Debug)]
pub struct Ellipse {
    pub x: f64,
    pub y: f64,
    pub stops: &'static [(f64, &'static str)],
}

#[derive(Clone, Copy, Debug)]
pub enum CardPaint {
    Linear(Lin),
    Radial(Ellipse),
}

/// The needle and the hub it swings on (widths in card units).
#[derive(Clone, Copy, Debug)]
pub struct Needle {
    pub color: &'static str,
    pub width: f64,
    /// The hub sits on the card's bottom edge, so only its top half shows.
    pub hub_r: f64,
    pub hub_opacity: f64,
    /// The bloom off a lit needle: (colour, how many needle-widths wide).
    pub halo: Option<(&'static str, f64)>,
}

/// The bulb behind the card: a broad wash and an optional tight hotspot, as
/// (x %, y %, reach %) — the CSS `radial-gradient(ellipse at …)` it was.
#[derive(Clone, Copy, Debug)]
pub struct Lamp {
    pub color: &'static str,
    pub x: f64,
    pub y: f64,
    pub reach: f64,
    pub core: Option<(&'static str, f64, f64, f64)>,
    /// The light the lamp throws past the card, as `(wash, hotspot)` bloom
    /// intensities — zero where the real meter's lamp is only a tint.
    pub light: (f64, f64),
}

/// The frame lip's shadow on the card: from the top lip and the left side.
#[derive(Clone, Copy, Debug)]
pub struct Shade {
    pub color: &'static str,
    pub top: f64,
    pub side: f64,
}

/// Light spilling around a lit movement's opening.
#[derive(Clone, Copy, Debug)]
pub struct Glow {
    pub color: &'static str,
    pub spread: f64,
    pub core: Option<&'static str>,
}

/// The louvre under the glass.
#[derive(Clone, Copy, Debug)]
pub struct Vent {
    pub dark: &'static str,
    pub light: &'static str,
    pub pitch: f64,
}

/// The frame a movement mounted through a panel sits in. Lengths in design
/// px against a 190 px card. The chamfer is lit from above: its top face in
/// shadow and its bottom catching the light, which is what makes it an
/// opening rather than a boss.
#[derive(Clone, Copy, Debug)]
pub struct Bezel {
    pub name: &'static str,
    pub frame: Lin,
    pub radius: f64,
    pub glow: Option<Glow>,
    pub top: &'static str,
    pub left: &'static str,
    pub right: &'static str,
    pub bottom: &'static str,
    pub depth: f64,
    pub vent: Option<Vent>,
    pub sheen: Option<Lin>,
    pub shade: Option<Shade>,
}

/// A VU face.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    pub card_paint: CardPaint,
    pub ink: &'static str,
    pub hot: &'static str,
    pub needle: Needle,
    pub lamp: Option<Lamp>,
    pub glass: Option<Lin>,
    pub bezel: &'static Bezel,
    /// The card this face prints by default.
    pub card: Card,
    /// Takes a lamp colour from a variant (only the backlit movement).
    pub tintable: bool,
}

// ── bezels ───────────────────────────────────────────────────────────────

/// The reflection a moulded frame catches off the room, from the top left.
const FRAME_SHEEN: Lin = Lin {
    angle: 158.0,
    stops: &[
        (0.0, "rgba(255,255,255,0.20)"),
        (0.20, "rgba(255,255,255,0.07)"),
        (0.40, "rgba(255,255,255,0.015)"),
        (0.58, "rgba(255,255,255,0.0)"),
        (1.0, "rgba(0,0,0,0.14)"),
    ],
};

/// The same on polished metal: harder, with a second catch low down.
const METAL_SHEEN: Lin = Lin {
    angle: 158.0,
    stops: &[
        (0.0, "rgba(255,255,255,0.34)"),
        (0.16, "rgba(255,255,255,0.10)"),
        (0.38, "rgba(255,255,255,0.0)"),
        (0.74, "rgba(0,0,0,0.16)"),
        (1.0, "rgba(255,255,255,0.14)"),
    ],
};

const RACK_SHADE: Shade = Shade {
    color: "rgba(0,0,0,0.42)",
    top: 7.0,
    side: 4.0,
};
const LOUVRE: Vent = Vent {
    dark: "#000000",
    light: "#2a2c2e",
    pitch: 4.0,
};

/// Pressed steel, matte black, vented — the rack default.
pub const BLACK_RACK: Bezel = Bezel {
    name: "black_rack",
    frame: Lin {
        angle: 180.0,
        stops: &[(0.0, "#141517"), (1.0, "#0a0b0c")],
    },
    radius: 2.0,
    glow: None,
    top: "#050506",
    left: "#0c0d0e",
    right: "#2b2d30",
    bottom: "#3b3d41",
    depth: 9.0,
    sheen: Some(FRAME_SHEEN),
    vent: Some(LOUVRE),
    shade: Some(RACK_SHADE),
};

/// A thin dark surround and nothing else.
pub const SLIM: Bezel = Bezel {
    name: "slim",
    frame: Lin {
        angle: 180.0,
        stops: &[(0.0, "#1b1c1f"), (1.0, "#101113")],
    },
    radius: 3.0,
    glow: None,
    top: "#0a0b0c",
    left: "#101113",
    right: "#26282b",
    bottom: "#303236",
    depth: 4.0,
    sheen: Some(FRAME_SHEEN),
    vent: None,
    shade: Some(Shade {
        color: "rgba(0,0,0,0.30)",
        top: 3.0,
        side: 2.0,
    }),
};

const CHROME_FRAME: Lin = Lin {
    angle: 180.0,
    stops: &[
        (0.0, "#f2f4f6"),
        (0.44, "#b9bec4"),
        (0.72, "#7e848b"),
        (1.0, "#d8dce0"),
    ],
};

/// A bright chromed ring, shallow, no vent.
pub const CHROME: Bezel = Bezel {
    name: "chrome",
    frame: CHROME_FRAME,
    radius: 4.0,
    glow: None,
    top: "#6d737a",
    left: "#878d94",
    right: "#dfe3e7",
    bottom: "#f4f6f8",
    depth: 7.0,
    sheen: Some(METAL_SHEEN),
    vent: None,
    shade: Some(Shade {
        color: "rgba(0,0,0,0.34)",
        top: 5.0,
        side: 3.0,
    }),
};

/// Warm brass, a little tarnished in the shadowed faces.
pub const BRASS: Bezel = Bezel {
    name: "brass",
    frame: Lin {
        angle: 180.0,
        stops: &[(0.0, "#d9b25f"), (0.46, "#a67f34"), (1.0, "#6d5220")],
    },
    radius: 3.0,
    glow: None,
    top: "#4a3714",
    left: "#6a5122",
    right: "#c9a253",
    bottom: "#e8c877",
    depth: 7.0,
    sheen: Some(METAL_SHEEN),
    vent: None,
    shade: Some(Shade {
        color: "rgba(0,0,0,0.36)",
        top: 5.0,
        side: 3.0,
    }),
};

/// Satin nickel, the Teletronix-era meter plate: lit broadly, not in a band.
pub const SATIN_PLATE: Bezel = Bezel {
    name: "satin_plate",
    frame: Lin {
        angle: 180.0,
        stops: &[
            (0.0, "#e6e7e4"),
            (0.42, "#cbccc8"),
            (0.78, "#aeafab"),
            (1.0, "#d6d7d3"),
        ],
    },
    radius: 2.0,
    glow: None,
    top: "#7e807c",
    left: "#8f918d",
    right: "#dcddd9",
    bottom: "#f0f1ee",
    depth: 8.0,
    sheen: Some(METAL_SHEEN),
    vent: None,
    shade: Some(Shade {
        color: "rgba(0,0,0,0.38)",
        top: 8.0,
        side: 5.0,
    }),
};

/// A deep rectangular recess — the modern console look, no vent.
pub const RECESSED: Bezel = Bezel {
    name: "recessed",
    frame: Lin {
        angle: 180.0,
        stops: &[(0.0, "#212429"), (0.60, "#15181c"), (1.0, "#0d0f12")],
    },
    radius: 3.0,
    glow: None,
    top: "#07080a",
    left: "#0d0f12",
    right: "#31353b",
    bottom: "#454a51",
    depth: 13.0,
    sheen: Some(FRAME_SHEEN),
    vent: None,
    shade: Some(Shade {
        color: "rgba(0,0,0,0.52)",
        top: 10.0,
        side: 6.0,
    }),
};

/// The rack frame with the bulb turned up — warm light around the opening.
pub const LIT_AMBER: Bezel = Bezel {
    name: "lit_amber",
    glow: Some(Glow {
        color: "rgba(255,186,86,0.42)",
        spread: 22.0,
        core: Some("rgba(255,214,140,0.34)"),
    }),
    ..BLACK_RACK
};

/// The same, lit cold.
pub const LIT_BLUE: Bezel = Bezel {
    name: "lit_blue",
    glow: Some(Glow {
        color: "rgba(110,180,255,0.40)",
        spread: 22.0,
        core: Some("rgba(180,220,255,0.32)"),
    }),
    ..BLACK_RACK
};

/// A chromed ring around a lit movement.
pub const LIT_CHROME: Bezel = Bezel {
    name: "lit_chrome",
    glow: Some(Glow {
        color: "rgba(255,244,214,0.38)",
        spread: 18.0,
        core: Some("rgba(255,255,255,0.30)"),
    }),
    ..CHROME
};

/// Every frame, in the source's order.
pub const BEZELS: [&Bezel; 9] = [
    &BLACK_RACK,
    &SLIM,
    &CHROME,
    &BRASS,
    &SATIN_PLATE,
    &RECESSED,
    &LIT_AMBER,
    &LIT_BLUE,
    &LIT_CHROME,
];

// ── faces ────────────────────────────────────────────────────────────────

/// The reflection on the glass over a card.
const GLASS: Lin = Lin {
    angle: 160.0,
    stops: &[
        (0.0, "rgba(255,255,255,0.22)"),
        (0.38, "rgba(255,255,255,0.04)"),
        (1.0, "rgba(0,0,0,0.10)"),
    ],
};

const DARK_NEEDLE: Needle = Needle {
    color: "#241a0c",
    width: 1.1,
    hub_r: 4.0,
    hub_opacity: 0.9,
    halo: None,
};

/// Warm ivory under a yellow lamp — the LA-2A's, and most tube gear's.
pub static AMBER: Face = Face {
    card_paint: CardPaint::Linear(Lin {
        angle: 180.0,
        stops: &[(0.0, "#f6d79a"), (0.62, "#e8b45f"), (1.0, "#d99a3c")],
    }),
    ink: "#3a2a12",
    hot: "#8f2010",
    needle: DARK_NEEDLE,
    lamp: Some(Lamp {
        color: "rgba(255,214,120,0.50)",
        x: 50.0,
        y: 8.0,
        reach: 70.0,
        // The spot you can see plainly in the middle of an LA-2A's face.
        core: Some(("rgba(255,236,186,0.42)", 50.0, 56.0, 30.0)),
        light: (0.05, 0.0),
    }),
    glass: Some(GLASS),
    bezel: &BLACK_RACK,
    card: Card::Vu,
    tintable: false,
};

/// Neutral ivory under a white lamp — the 1176's Modutec, an SSL's.
pub static IVORY: Face = Face {
    card_paint: CardPaint::Linear(Lin {
        angle: 180.0,
        stops: &[(0.0, "#f4f2ea"), (1.0, "#ddd9cd")],
    }),
    ink: "#2a241c",
    hot: "#a8281c",
    needle: Needle {
        color: "#1d1a15",
        ..DARK_NEEDLE
    },
    lamp: Some(Lamp {
        color: "rgba(255,246,224,0.28)",
        x: 50.0,
        y: 8.0,
        reach: 68.0,
        core: Some(("rgba(255,252,242,0.26)", 50.0, 54.0, 28.0)),
        light: (0.0, 0.0),
    }),
    glass: Some(GLASS),
    bezel: &BLACK_RACK,
    card: Card::Vu,
    tintable: false,
};

/// The dbx's: an amber card printed in blue — a decibel readout, not a VU.
pub static AMBER_BLUE: Face = Face {
    card_paint: CardPaint::Linear(Lin {
        angle: 180.0,
        stops: &[(0.0, "#f7cf86"), (0.58, "#eab259"), (1.0, "#d99b3f")],
    }),
    ink: "#1c4f96",
    hot: "#1c4f96",
    needle: Needle {
        color: "#14161a",
        ..DARK_NEEDLE
    },
    lamp: Some(Lamp {
        color: "rgba(255,216,126,0.50)",
        x: 50.0,
        y: 8.0,
        reach: 68.0,
        core: Some(("rgba(255,238,190,0.34)", 50.0, 55.0, 28.0)),
        light: (0.0, 0.0),
    }),
    glass: Some(GLASS),
    bezel: &BLACK_RACK,
    card: Card::Decibels,
    tintable: false,
};

/// A blue-lit card printed in white.
pub static BLUE: Face = Face {
    card_paint: CardPaint::Linear(Lin {
        angle: 180.0,
        stops: &[(0.0, "#2f5f8f"), (1.0, "#16324e")],
    }),
    ink: "#e8f2ff",
    hot: "#ff6a5c",
    needle: Needle {
        color: "#f4f8ff",
        ..DARK_NEEDLE
    },
    lamp: Some(Lamp {
        color: "rgba(150,205,255,0.32)",
        x: 50.0,
        y: 8.0,
        reach: 68.0,
        core: Some(("rgba(206,232,255,0.26)", 50.0, 55.0, 28.0)),
        light: (0.0, 0.0),
    }),
    glass: Some(GLASS),
    bezel: &BLACK_RACK,
    card: Card::Vu,
    tintable: false,
};

/// The modern console movement: a near-black card behind glass with the
/// lamp behind the needle, so the light pools at the pivot and falls off
/// upward; the scale printed white, no red stretch.
pub static BACKLIT: Face = Face {
    card_paint: CardPaint::Radial(Ellipse {
        x: 0.5,
        y: 1.04,
        stops: &[(0.0, "#12294a"), (0.62, "#05070a")],
    }),
    ink: "#eef3f8",
    hot: "#eef3f8",
    needle: Needle {
        color: "#ffffff",
        width: 1.5,
        hub_r: 3.0,
        hub_opacity: 0.55,
        halo: Some(("rgba(190,222,255,0.34)", 6.5)),
    },
    // The pool behind the needle is the one lamp you can see glowing.
    lamp: Some(Lamp {
        color: "rgba(130,190,255,0.50)",
        x: 50.0,
        y: 102.0,
        reach: 60.0,
        core: None,
        light: (0.8, 0.0),
    }),
    glass: Some(Lin {
        angle: 146.0,
        stops: &[
            (0.0, "rgba(255,255,255,0.26)"),
            (0.15, "rgba(255,255,255,0.10)"),
            (0.30, "rgba(255,255,255,0.02)"),
            (0.44, "rgba(255,255,255,0.0)"),
            (1.0, "rgba(0,0,0,0.26)"),
        ],
    }),
    bezel: &RECESSED,
    card: Card::Vu,
    tintable: true,
};

/// The lamp colours a backlit movement can be lit in (the source sheet's).
pub const TINTS: [(&str, &str); 4] = [
    ("blue", "#4a9eff"),
    ("red", "#c0392b"),
    ("green", "#39c07a"),
    ("amber", "#e8a33d"),
];
