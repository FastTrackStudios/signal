//! The kit: every VU face this crate can draw, as a [`VuSpec`] each.
//!
//! A period movement — Weston, Modutec, Sifam — has an **ivory card with a
//! black scale and a red stretch above 0**, lit from behind by one or two
//! bulbs. The lamp is what varies between units, not the card: the LA-2A's is
//! warm, a rackmount's whiter. Blue-faced meters are largely a plugin
//! aesthetic rather than something these units wore, so ivory is the default
//! and blue is kept for a unit that genuinely has one.
//!
//! One const per face — see [`vu_kit`](crate::hardware::vu_kit) for how to
//! add one, and the `vu_sheet` test for how to look at it.

use super::vu_kit::{Bezel, Core, Glow, Halo, Lamp, Needle, Shade, Vent, VuSpec, Wash};

// ─────────────────────────────────────────────────────────────────────────
// Bezels
//
// The frame is a separate part from the movement: the same Modutec sits in a
// pressed-steel rack frame on one unit and a chromed ring on another. So a
// face names a default and a panel can ask for a different one, the way it
// would order a different part.
//
// The chamfer is what sells it. Lit from above, a sunken opening has its top
// face in shadow and its bottom face catching the light — the opposite of a
// raised boss, and the whole difference between a movement set into a panel
// and one printed on it.
// ─────────────────────────────────────────────────────────────────────────

/// The reflection a moulded frame catches off the room, swept from the top
/// left the way every panel here is lit.
const FRAME_SHEEN: &str = "linear-gradient(158deg, rgba(255,255,255,0.20) 0%, \
                           rgba(255,255,255,0.07) 20%, rgba(255,255,255,0.015) 40%, \
                           rgba(255,255,255,0.0) 58%, rgba(0,0,0,0.14) 100%)";

/// The same on polished metal: harder, and with a second catch low down.
const METAL_SHEEN: &str = "linear-gradient(158deg, rgba(255,255,255,0.34) 0%, \
                           rgba(255,255,255,0.10) 16%, rgba(255,255,255,0.0) 38%, \
                           rgba(0,0,0,0.16) 74%, rgba(255,255,255,0.14) 100%)";

/// The shadow a deep rack lip throws across the card behind it.
const RACK_SHADE: Shade = Shade {
    color: "rgba(0,0,0,0.42)",
    top: 7.0,
    side: 4.0,
};

/// The louvre under the glass on a rack frame.
const LOUVRE: Vent = Vent {
    dark: "#000",
    light: "#2a2c2e",
    pitch: 4.0,
};

/// Pressed steel, matte black, with a vent under the glass. The frame nearly
/// every rack meter is mounted in, and the default.
pub const BLACK_RACK: Bezel = Bezel {
    frame: "linear-gradient(180deg, #141517, #0a0b0c)",
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

/// A thin dark surround and nothing else — a movement set almost flush, for
/// a panel that does not want a frame competing with it.
pub const SLIM: Bezel = Bezel {
    frame: "linear-gradient(180deg, #1b1c1f, #101113)",
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

/// A chromed ring: bright, shallow, and no vent — the look of a meter set
/// into a polished front rather than a painted one.
pub const CHROME: Bezel = Bezel {
    frame: "linear-gradient(180deg, #f2f4f6 0%, #b9bec4 44%, #7e848b 72%, #d8dce0 100%)",
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

/// Brass, as the era's better-dressed units wore it: warm, and a little
/// tarnished at the shadowed faces.
pub const BRASS: Bezel = Bezel {
    frame: "linear-gradient(180deg, #d9b25f 0%, #a67f34 46%, #6d5220 100%)",
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

/// Satin nickel, as a Teletronix-era meter is framed: a light plate on a
/// brushed grey panel, with a thin dark line inside the lip and a long shadow
/// thrown down the card behind it. Not chrome — it is satin, so it is lit
/// broadly rather than in a hard band.
pub const SATIN_PLATE: Bezel = Bezel {
    frame: "linear-gradient(180deg, #e6e7e4 0%, #cbccc8 42%, #aeafab 78%, #d6d7d3 100%)",
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

/// The rack frame with the bulb turned up: warm light spilling around the
/// opening, which on a dark panel is most of what says the meter is *on*.
pub const LIT_AMBER: Bezel = Bezel {
    glow: Some(Glow {
        color: "rgba(255,186,86,0.42)",
        spread: 22.0,
        core: Some("rgba(255,214,140,0.34)"),
    }),
    ..BLACK_RACK
};

/// The same, lit cold — for a blue-carded movement.
pub const LIT_BLUE: Bezel = Bezel {
    glow: Some(Glow {
        color: "rgba(110,180,255,0.40)",
        spread: 22.0,
        core: Some("rgba(180,220,255,0.32)"),
    }),
    ..BLACK_RACK
};

/// A deep rectangular recess: the movement set well back behind glass, with
/// the opening's walls doing the work rather than a frame around it. The
/// modern console look — no vent, because there is no bulb to cool.
pub const RECESSED: Bezel = Bezel {
    frame: "linear-gradient(180deg, #212429 0%, #15181c 60%, #0d0f12 100%)",
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

/// A chromed ring around a lit movement — the halo reads doubly well off
/// polished metal.
pub const LIT_CHROME: Bezel = Bezel {
    glow: Some(Glow {
        color: "rgba(255,244,214,0.38)",
        spread: 18.0,
        core: Some("rgba(255,255,255,0.30)"),
    }),
    ..CHROME
};

/// The reflection on the glass over a card.
const GLASS: &str = "linear-gradient(160deg, rgba(255,255,255,0.22) 0%, \
                     rgba(255,255,255,0.04) 38%, rgba(0,0,0,0.10) 100%)";

/// The needle on a dark-printed ivory card.
const DARK_NEEDLE: Needle = Needle {
    color: "#241a0c",
    width: 1.1,
    hub_r: 4.0,
    hub_opacity: 0.9,
    halo: None,
};

// ─────────────────────────────────────────────────────────────────────────
// Amber — warm ivory under a yellow lamp. The LA-2A's, and most tube gear's.
// ─────────────────────────────────────────────────────────────────────────
pub static AMBER: VuSpec = VuSpec {
    card: "linear-gradient(180deg, #f6d79a 0%, #e8b45f 62%, #d99a3c 100%)",
    ink: "#3a2a12",
    hot: "#8f2010",
    needle: DARK_NEEDLE,
    lamp: Some(Lamp {
        color: "rgba(255,214,120,0.50)",
        x: 50.0,
        y: 8.0,
        reach: 70.0,
        // The spot you can see plainly in the middle of an LA-2A's face.
        core: Some(Core {
            color: "rgba(255,236,186,0.42)",
            x: 50.0,
            y: 56.0,
            reach: 30.0,
        }),
    }),
    glass: Some(GLASS),
    bezel: BLACK_RACK,
    wash: Wash::None,
};

// ─────────────────────────────────────────────────────────────────────────
// Ivory — neutral, under a white lamp. The 1176's Modutec, an SSL's.
// ─────────────────────────────────────────────────────────────────────────
pub static IVORY: VuSpec = VuSpec {
    card: "linear-gradient(180deg, #f4f2ea 0%, #ddd9cd 100%)",
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
        core: Some(Core {
            color: "rgba(255,252,242,0.26)",
            x: 50.0,
            y: 54.0,
            reach: 28.0,
        }),
    }),
    glass: Some(GLASS),
    bezel: BLACK_RACK,
    wash: Wash::None,
};

// ─────────────────────────────────────────────────────────────────────────
// Amber-blue — the dbx's, which is not a VU at all but a decibel readout,
// and prints like one: an amber card in blue ink, and no red stretch,
// because there is no 0 VU on it to be over.
// ─────────────────────────────────────────────────────────────────────────
pub static AMBER_BLUE: VuSpec = VuSpec {
    card: "linear-gradient(180deg, #f7cf86 0%, #eab259 58%, #d99b3f 100%)",
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
        core: Some(Core {
            color: "rgba(255,238,190,0.34)",
            x: 50.0,
            y: 55.0,
            reach: 28.0,
        }),
    }),
    glass: Some(GLASS),
    bezel: BLACK_RACK,
    wash: Wash::None,
};

// ─────────────────────────────────────────────────────────────────────────
// Blue — a blue-lit card printed in white.
// ─────────────────────────────────────────────────────────────────────────
pub static BLUE: VuSpec = VuSpec {
    card: "linear-gradient(180deg, #2f5f8f 0%, #16324e 100%)",
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
        core: Some(Core {
            color: "rgba(206,232,255,0.26)",
            x: 50.0,
            y: 55.0,
            reach: 28.0,
        }),
    }),
    glass: Some(GLASS),
    bezel: BLACK_RACK,
    wash: Wash::None,
};

// ─────────────────────────────────────────────────────────────────────────
// Backlit — the modern console movement: a near-black card behind glass with
// the lamp *behind the needle*, so the light pools at the pivot and falls off
// upward, and the scale printed white over it.
//
// The one face in the kit that takes a colour from the call site. On the
// others the card is the colour it is; here the whole look is a lamp behind
// smoked glass, and which colour that lamp is is a decision the panel makes.
// Pass `tint` to `VuMeter` and the glow, the pool in the card and the bloom
// off the needle all follow it. The print stays white — a lit meter is read
// by contrast against the glow.
// ─────────────────────────────────────────────────────────────────────────
pub static BACKLIT: VuSpec = VuSpec {
    card: "radial-gradient(ellipse at 50% 104%, #12294a 0%, #05070a 62%)",
    ink: "#eef3f8",
    // No red stretch. This is a compression meter, not a level meter — there
    // is no 0 VU on it to be over, and the reference prints the whole scale
    // in one colour.
    hot: "#eef3f8",
    needle: Needle {
        color: "#ffffff",
        width: 1.5,
        hub_r: 3.0,
        hub_opacity: 0.55,
        halo: Some(Halo {
            color: "rgba(190,222,255,0.34)",
            spread: 6.5,
        }),
    },
    // Behind the needle, at the pivot, falling off upward — not a wash across
    // the top like a bulb-lit card. Tight: the corners of the card stay near
    // black, which is what makes the pool read as a lamp rather than a colour.
    lamp: Some(Lamp {
        color: "rgba(130,190,255,0.50)",
        x: 50.0,
        y: 102.0,
        reach: 60.0,
        // No hotspot: the light on a backlit movement comes from an even
        // panel behind the card, not from a bulb in a box.
        core: None,
    }),
    // Heavier than a period meter's: this one is read through real glass, and
    // the sweep across the top-left corner is most of what says so.
    glass: Some(
        "linear-gradient(146deg, rgba(255,255,255,0.26) 0%, \
         rgba(255,255,255,0.10) 15%, rgba(255,255,255,0.02) 30%, \
         rgba(255,255,255,0.0) 44%, rgba(0,0,0,0.26) 100%)",
    ),
    bezel: RECESSED,
    wash: Wash::Backlit,
};
