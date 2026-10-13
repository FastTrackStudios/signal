//! The kit: every knob this crate can draw, as a [`KnobSpec`] each.
//!
//! These are the actual parts the units wear, because the shape is most of
//! what you recognise a panel by before you read a word of it. One const per
//! knob — see [`knob_kit`](crate::hardware::knob_kit) for how to add one, and
//! the `knob_sheet` test for how to look at it.

use super::knob_kit::{
    dome, matte_light, Finish, Flutes, Index, KnobSpec, Paint, Specular, Tier, Turns,
};

// ── Inks ─────────────────────────────────────────────────────────────────
/// Turned aluminium, in four layers from the light down to the metal.
///
/// 1. the room's glint, high and left;
/// 2. the *anisotropy* — a turned face throws two bright lobes opposite each
///    other and goes dark ninety degrees away, and that is what says metal;
/// 3. the turning marks. Concentric, because a knob cap is faced on a lathe
///    that spins it: the marks go round, not out. Drawn as conic spokes they
///    converge at the hub and the cap reads as a paper fan — which is what
///    happened, twice: four fat spokes made a pinwheel, and thirty-six fine
///    ones made a sunburst;
/// 4. the metal itself. Anodised aluminium, not chrome: an outboard knob is
///    darker than the white index line printed on it, or the line disappears.
const BRUSHED_METAL: &str = "radial-gradient(circle at 34% 26%, \
     rgba(255,255,255,0.26) 0%, rgba(255,255,255,0.0) 58%), \
     conic-gradient(from 208deg, rgba(255,255,255,0.14) 0deg, \
     rgba(0,0,0,0.12) 88deg, rgba(255,255,255,0.13) 180deg, \
     rgba(0,0,0,0.13) 268deg, rgba(255,255,255,0.14) 360deg), \
     repeating-radial-gradient(circle at 50% 50%, rgba(255,255,255,0.07) 0%, \
     rgba(0,0,0,0.07) 1.6%, rgba(255,255,255,0.07) 3.2%), \
     linear-gradient(162deg, #cdcdc8 0%, #b4b4af 46%, #96968f 100%)";

/// The two index colours nearly every knob uses: a painted white line, or a
/// dark groove on a light face.
const LIGHT: &str = "#f2f2f0";
const DARK: &str = "#1c1c1e";

/// The shaft the knob is pressed onto, read as a small shadow at the centre.
/// Fixed rather than rotating — it does not turn with the cap.
const HUB: Option<&str> = Some("rgba(0,0,0,0.35)");

// ── Helpers, so a spec reads as the knob and not as struct syntax ────────

/// A knob that is one surface all the way out — no skirt, no collar.
const fn solid(css: &'static str, finish: Finish) -> Tier {
    Tier::new(
        1.0,
        Paint::Surface {
            css,
            finish,
            tint: true,
        },
        Turns::Cap,
    )
}

// ─────────────────────────────────────────────────────────────────────────
// Bakelite — the LA-2A / 1176 knob. Black, with a white blade.
// ─────────────────────────────────────────────────────────────────────────
static BAKELITE_TIERS: &[Tier] = &[
    // The base. A hair wider than the body and much darker, with a contact
    // shadow under it: that pair is what makes the knob read as a truncated
    // cone standing on the panel instead of a circle printed on it. One
    // gradient disc on its own cannot, however good the gradient.
    Tier::new(1.0, Paint::Flat("#070709"), Turns::Cap)
        .shadowed(0.05)
        .outlined("rgba(255,255,255,0.09)", 0.7),
    // The cone's wall. Top-lit, not a hotspot: `radial-gradient(circle at
    // 33% 21%, ...)` puts the light beside the knob rather than above the
    // rack, and a black moulding lit from beside it is a billiard ball. An
    // LA-2A's knob is matte, and what you see of its shape is that the top
    // face is fractionally lighter than the wall.
    Tier::new(
        0.94,
        Paint::Surface {
            css: "linear-gradient(178deg, #34343b 0%, #212126 42%, #131317 78%, #0b0b0e 100%)",
            finish: Finish::Matte,
            tint: true,
        },
        Turns::Cap,
    ),
    // The step up to the top face, as a shadowed wall. Without a visible
    // edge the face reads as a bubble inside the knob rather than a machined
    // step on it — which, with a highlight thrown off to one side, is most of
    // what "offset and out of balance" was.
    Tier::new(0.76, Paint::Flat("rgba(0,0,0,0.5)"), Turns::Cap),
    Tier::new(
        0.73,
        Paint::Surface {
            css: "linear-gradient(178deg, #3c3c44 0%, #26262c 46%, #16161a 100%)",
            finish: Finish::FlatTop,
            tint: true,
        },
        Turns::Cap,
    ),
];

pub static BAKELITE: KnobSpec = KnobSpec {
    tiers: BAKELITE_TIERS,
    // A line of even width, which is what is painted on an LA-2A's knob and
    // an 1176's. A tapered blade is wide at the hub and pointed at the tip —
    // the opposite end from the one you read — and it made the pointer look
    // like a shard rather than a mark.
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
    numerals_on_knob: false,
    hub: None,
};

// ─────────────────────────────────────────────────────────────────────────
// Metal — brushed, with a dark blade.
// ─────────────────────────────────────────────────────────────────────────
static METAL_TIERS: &[Tier] = &[
    // A dark seat so the metal has an edge to end at rather than fading into
    // the panel.
    Tier::new(1.0, Paint::Flat("#3a3a38"), Turns::Cap).shadowed(0.05),
    Tier::new(
        0.93,
        Paint::Surface {
            css: BRUSHED_METAL,
            finish: Finish::Brushed,
            tint: true,
        },
        Turns::Cap,
    )
    .outlined("rgba(255,255,255,0.16)", 0.7),
];

pub static METAL: KnobSpec = KnobSpec {
    tiers: METAL_TIERS,
    index: Index::Blade {
        to: 0.83,
        half_width: 0.113,
        color: DARK,
    },
    collar_index: None,
    flutes: None,
    specular: Some(dome(1.0)),
    ring_offset: 0.0,
    numerals_on_knob: false,
    hub: HUB,
};

// ─────────────────────────────────────────────────────────────────────────
// Skirted — the Davies 1910/1913 and the shelf of phenolic skirted knobs
// beside it, on every piece of vintage outboard that is not a Pultec.
//
// The anatomy is the name: a wide flat *skirt* of moulded phenolic sitting on
// the panel, a narrower ribbed body standing on it, a flat top, and the
// indicator line engraved down the body and out across the skirt so it can be
// read against numbers printed on the panel around the skirt's edge.
//
// It was none of that. Every tier was full width, so there was no skirt, and
// the body was one hard off-centre radial that made the whole knob a glossy
// black marble — the tier at 0.62 that was meant to be the cap could not be
// seen at all. Phenolic is matte: what you read is the *step* from skirt to
// body and the shadow it drops, never a glint.
// ─────────────────────────────────────────────────────────────────────────
static SKIRTED_TIERS: &[Tier] = &[
    // The skirt, standing on the panel.
    Tier::new(1.0, Paint::Flat("#0a0a0c"), Turns::Cap)
        .shadowed(0.055)
        .outlined("rgba(255,255,255,0.10)", 0.8),
    Tier::new(
        0.955,
        Paint::Surface {
            css: "linear-gradient(178deg, #33333a 0%, #212127 44%, #141418 76%, #0d0d10 100%)",
            finish: Finish::Matte,
            tint: true,
        },
        Turns::Cap,
    ),
    // The shadow the body drops onto the skirt — the step is the whole read.
    Tier::new(0.70, Paint::Flat("rgba(0,0,0,0.55)"), Turns::Cap),
    // The ribbed body wall.
    Tier::new(
        0.66,
        Paint::Surface {
            css: "linear-gradient(178deg, #3d3d45 0%, #26262c 46%, #17171b 100%)",
            finish: Finish::Matte,
            tint: true,
        },
        Turns::Cap,
    ),
    // The flat top, a shade proud of the wall.
    Tier::new(
        0.50,
        Paint::Surface {
            css: "linear-gradient(178deg, #47474f 0%, #2e2e35 50%, #1c1c21 100%)",
            finish: Finish::FlatTop,
            tint: false,
        },
        Turns::Cap,
    ),
];

pub static SKIRTED: KnobSpec = KnobSpec {
    tiers: SKIRTED_TIERS,
    // Down the body and out over the skirt, in one line. The panel prints its
    // numbers around the skirt's edge, so the line has to reach them.
    index: Index::Bar {
        from: 0.30,
        to: 0.955,
        width: 3.0,
        color: LIGHT,
    },
    collar_index: None,
    // The ribs, on the body wall between the top face and the step.
    flutes: Some(Flutes {
        count: 30,
        from: 0.52,
        to: 0.66,
        stroke: "rgba(255,255,255,0.11)",
        width: 0.8,
        shadow: Some("rgba(0,0,0,0.34)"),
        turns: Turns::Cap,
    }),
    specular: Some(matte_light(0.955)),
    ring_offset: 0.0,
    numerals_on_knob: false,
    hub: None,
};

// ─────────────────────────────────────────────────────────────────────────
// Daka-Ware — the Pultec EQP-1A's. Black phenolic: a coarsely scalloped
// skirt you grip, a raised ridged body, a domed top, and the index engraved
// from the dome out to the skirt's edge and filled white. Long, because it is
// the only thing the panel's printed 0–10 is read against.
// ─────────────────────────────────────────────────────────────────────────
static DAKA_TIERS: &[Tier] = &[
    // Every tier was a flat fill, which is the one thing this crate's own
    // notes say cannot read as a material: five grey discs stacked up look
    // like five grey discs. Phenolic is dark and slightly glossy, and each
    // step of the stack catches the light at its own angle.
    Tier::new(
        1.0,
        Paint::Surface {
            css: "radial-gradient(circle at 36% 22%, #2a2a30 0%, #17171b 46%, #0c0c0f 100%)",
            finish: Finish::Moulded,
            tint: false,
        },
        Turns::Cap,
    )
    .toothed(22, 0.08)
    .shadowed(0.06)
    .outlined("rgba(0,0,0,0.7)", 0.7),
    // The step up to the body, read as a shadowed wall rather than an edge.
    Tier::new(0.74, Paint::Flat("#08080a"), Turns::Cap).outlined("rgba(255,255,255,0.11)", 0.6),
    Tier::new(
        0.70,
        Paint::Surface {
            css: "radial-gradient(circle at 34% 24%, #3d3d45 0%, #24242a 48%, #131317 100%)",
            finish: Finish::Moulded,
            tint: false,
        },
        Turns::Cap,
    )
    .outlined("rgba(255,255,255,0.11)", 0.7),
    Tier::new(
        0.52,
        Paint::Surface {
            css: "radial-gradient(circle at 34% 24%, #46464f 0%, #2b2b32 52%, #191920 100%)",
            finish: Finish::Moulded,
            tint: false,
        },
        Turns::Cap,
    )
    .outlined("rgba(255,255,255,0.11)", 0.6),
    // The dome on top, lit hardest — it is the nearest thing to the light.
    Tier::new(
        0.30,
        Paint::Surface {
            css: "radial-gradient(circle at 32% 22%, #5a5a64 0%, #34343c 56%, #1d1d24 100%)",
            finish: Finish::Moulded,
            tint: false,
        },
        Turns::Cap,
    ),
];

pub static DAKA: KnobSpec = KnobSpec {
    tiers: DAKA_TIERS,
    // Bold. This line is the only thing a Pultec's printed 0–10 is read
    // against, and on a 96 px boost knob a 2.4-wide engraving reads as a
    // scratch rather than a pointer.
    index: Index::Bar {
        from: 0.24,
        to: 0.96,
        width: 3.2,
        color: "#eceae4",
    },
    collar_index: None,
    // Fine ridging around the raised body's wall, above the skirt.
    flutes: Some(Flutes {
        count: 40,
        from: 0.60,
        to: 0.70,
        stroke: "rgba(255,255,255,0.10)",
        width: 0.7,
        shadow: None,
        turns: Turns::Cap,
    }),
    specular: Some(Specular {
        w: 0.46,
        h: 0.30,
        dx: -0.11,
        dy: -0.15,
        fill: "radial-gradient(ellipse at 50% 50%, rgba(255,255,255,0.13) 0%, \
               rgba(255,255,255,0.0) 72%)",
        rotate: -32.0,
        rim: Some("rgba(255,255,255,0.12)"),
        r: 1.0,
    }),
    ring_offset: 0.0,
    numerals_on_knob: false,
    hub: HUB,
};

// ─────────────────────────────────────────────────────────────────────────
// Marconi — the 1073's gain switch and high-pass. A coloured wing laid
// across a dark disc and overhanging it; the overhang IS the silhouette.
// ─────────────────────────────────────────────────────────────────────────
static MARCONI_TIERS: &[Tier] = &[
    Tier::new(
        1.0,
        Paint::Surface {
            css: "radial-gradient(circle at 36% 22%, #4e4e55 0%, #232328 40%, #0f0f12 100%)",
            finish: Finish::Moulded,
            tint: false,
        },
        Turns::Cap,
    ),
    Tier::new(
        0.72,
        Paint::Surface {
            css: "radial-gradient(circle at 36% 22%, #3c3c40 0%, #1e1e21 46%, #121214 100%)",
            finish: Finish::Moulded,
            tint: true,
        },
        Turns::Cap,
    ),
];

pub static MARCONI: KnobSpec = KnobSpec {
    tiers: MARCONI_TIERS,
    index: Index::Wing {
        color: LIGHT,
        body: 0.72,
    },
    collar_index: None,
    flutes: None,
    specular: Some(dome(0.72)),
    // The wing overhangs the skirt, so the dots and numerals move out to
    // clear its tip — but by less than the full overhang: the knob's viewBox
    // stops at 55, and pushing the scale the whole distance put the numerals
    // outside it, where they clipped and the ring read as lopsided.
    ring_offset: 5.0,
    numerals_on_knob: false,
    hub: HUB,
};

// ─────────────────────────────────────────────────────────────────────────
// Collet — the SSL 4000 channel. A flat-topped coloured cap with a fluted
// rim and one white bar across it. No skirt: the panel prints the travel as
// dots around it instead.
// ─────────────────────────────────────────────────────────────────────────
static COLLET_TIERS: &[Tier] = &[solid(
    "linear-gradient(162deg, #4a4a4e 0%, #303034 42%, #202024 100%)",
    Finish::FlatTop,
)];

pub static COLLET: KnobSpec = KnobSpec {
    tiers: COLLET_TIERS,
    // The bar reaches the flutes. On the desk it runs the cap's full radius —
    // stopping at 0.90 with a hub dot showing through left it reading as a
    // short dash beside a screw, which is a different knob entirely.
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
        turns: Turns::Cap,
    }),
    // Flat top: a sheen across the whole face, not a blob on a dome.
    //
    // Full size, and shaped by the gradient rather than by the box. A linear
    // sweep painted into a small offset ellipse does not fade at the
    // ellipse's edge, so what you saw was the blob's own outline sitting on
    // the cap — bright right up to a hard elliptical boundary, and past the
    // rim at the top corner. The sweep now runs the face and is gone by 62%.
    specular: Some(Specular {
        w: 1.0,
        h: 1.0,
        dx: 0.0,
        dy: 0.0,
        fill: "linear-gradient(152deg, rgba(255,255,255,0.22) 0%, \
               rgba(255,255,255,0.07) 34%, rgba(255,255,255,0.0) 62%)",
        rotate: 0.0,
        rim: None,
        r: 1.0,
    }),
    ring_offset: 0.0,
    numerals_on_knob: false,
    // A collet cap is pressed on from above: no shaft shows.
    hub: None,
};

// ─────────────────────────────────────────────────────────────────────────
// Silver-top — the UREI 1176. A wide matte black collar with a brushed,
// knurled aluminium cap set into the middle of it, and the index a white line
// on the *collar*, outside the cap. That arrangement is what makes an 1176's
// knobs read as rings from across a room: a dark annulus around a bright disc.
// ─────────────────────────────────────────────────────────────────────────
static SILVER_TOP_TIERS: &[Tier] = &[
    Tier::new(
        1.0,
        Paint::Surface {
            css: "radial-gradient(circle at 38% 28%, #35353a 0%, #1c1c20 46%, \
                  #101013 78%, #0a0a0c 100%)",
            finish: Finish::Moulded,
            tint: false,
        },
        Turns::Cap,
    ),
    // The cap. Half the knob, not five-ninths: the 1176 reads as a dark
    // annulus around a bright disc, and every tenth the cap gains is a tenth
    // off the collar the index line is printed on.
    Tier::new(
        0.50,
        Paint::Surface {
            css: BRUSHED_METAL,
            finish: Finish::Brushed,
            tint: true,
        },
        Turns::Cap,
    )
    .outlined("rgba(0,0,0,0.55)", 0.8),
    // Two turned rings on the brushed face.
    Tier::new(
        0.310,
        Paint::Groove {
            color: "rgba(0,0,0,0.16)",
            width: 0.7,
        },
        Turns::Cap,
    ),
    Tier::new(
        0.150,
        Paint::Groove {
            color: "rgba(0,0,0,0.13)",
            width: 0.6,
        },
        Turns::Cap,
    ),
];

pub static SILVER_TOP: KnobSpec = KnobSpec {
    tiers: SILVER_TOP_TIERS,
    // On the collar, and it has to *read* as a line: at 0.677–0.917 it was a
    // 5 px stub on a 44 px knob — you could not tell where a Shimmer's DECAY
    // was pointing, on a panel of eight of them. Now it runs the collar's
    // whole width, cap edge to rim.
    index: Index::Bar {
        from: 0.56,
        to: 0.97,
        width: 2.8,
        color: "#f4f4f2",
    },
    collar_index: None,
    // Fine knurling around the cap's edge.
    flutes: Some(Flutes {
        count: 54,
        from: 0.42,
        to: 0.50,
        stroke: "rgba(0,0,0,0.34)",
        width: 1.2,
        shadow: Some("rgba(0,0,0,0.40)"),
        turns: Turns::Cap,
    }),
    specular: Some(dome(0.50)),
    ring_offset: 0.0,
    numerals_on_knob: false,
    hub: HUB,
};

// ─────────────────────────────────────────────────────────────────────────
// Metal-fluted — dbx and its generation. Brushed aluminium, a fluted rim, a
// dark centre cap, read by a line across the metal.
// ─────────────────────────────────────────────────────────────────────────
static METAL_FLUTED_TIERS: &[Tier] = &[
    solid(BRUSHED_METAL, Finish::Brushed),
    Tier::new(0.42, Paint::Flat("#3a3c40"), Turns::Cap).outlined("rgba(0,0,0,0.5)", 0.8),
];

pub static METAL_FLUTED: KnobSpec = KnobSpec {
    tiers: METAL_FLUTED_TIERS,
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
        turns: Turns::Cap,
    }),
    specular: Some(dome(1.0)),
    ring_offset: 0.0,
    numerals_on_knob: false,
    hub: HUB,
};

// ─────────────────────────────────────────────────────────────────────────
// Pointer — Teletronix LA-2A and contemporaries. A plain black round knob
// with a moulded nose that points at a scale printed on the panel. No skirt,
// no flutes: you read the nose.
// ─────────────────────────────────────────────────────────────────────────
static POINTER_TIERS: &[Tier] = &[
    Tier::new(1.0, Paint::Flat("#08080a"), Turns::Cap)
        .shadowed(0.05)
        .outlined("rgba(255,255,255,0.08)", 0.7),
    // Matte, top-lit. An LA-2A's knob is a plain black moulding: the hard
    // off-centre radial it had made it a billiard ball with a nose stuck to
    // the side.
    Tier::new(
        0.94,
        Paint::Surface {
            css: "linear-gradient(178deg, #33333a 0%, #212127 44%, #131317 78%, #0c0c0f 100%)",
            finish: Finish::Matte,
            tint: true,
        },
        Turns::Cap,
    ),
];

pub static POINTER: KnobSpec = KnobSpec {
    tiers: POINTER_TIERS,
    index: Index::Nose { color: LIGHT },
    collar_index: None,
    flutes: None,
    specular: Some(matte_light(0.94)),
    // The nose reaches past the body by design — that is how it points — so a
    // ring drawn for a flush knob lands underneath it.
    ring_offset: 13.0,
    numerals_on_knob: false,
    hub: HUB,
};

// ─────────────────────────────────────────────────────────────────────────
// Rogan PT — the modular knob. Make Noise, Mutable, and most of what has been
// built since: soft-touch rubber over polymer, a matte cylinder with a flat
// top and a pointer moulded into it, widening toward the rim.
//
// The only part in the kit that is not from the 1960s, and the two things
// that say so are the material — rubber scatters, so the light sits in a
// diffuse ring at the edge and the middle is dead flat — and the pointer,
// which is a shape moulded *in* rather than a line painted *on*. Every other
// index here is constant-width or narrows outward.
// ─────────────────────────────────────────────────────────────────────────
static SOFT_POINTER_TIERS: &[Tier] = &[
    // The base standing on the panel.
    Tier::new(1.0, Paint::Flat("#08080a"), Turns::Cap)
        .shadowed(0.055)
        .outlined("rgba(255,255,255,0.08)", 0.8),
    // The rubber wall.
    Tier::new(
        0.94,
        Paint::Surface {
            css: "radial-gradient(circle at 50% 50%, #26262b 0%, #26262b 62%, \
                  #34343b 88%, #17171b 100%)",
            finish: Finish::SoftTouch,
            tint: true,
        },
        Turns::Cap,
    ),
    // The flat top, a hair proud of it. Barely a step: a soft-touch knob is
    // one moulding, not a stack, and a hard edge here reads as machined.
    Tier::new(
        0.80,
        Paint::Surface {
            css: "radial-gradient(circle at 50% 50%, #2c2c32 0%, #2c2c32 66%, \
                  #3a3a42 90%, #202026 100%)",
            finish: Finish::SoftTouch,
            tint: true,
        },
        Turns::Cap,
    ),
];

pub static SOFT_POINTER: KnobSpec = KnobSpec {
    tiers: SOFT_POINTER_TIERS,
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
    numerals_on_knob: false,
    hub: None,
};

// ─────────────────────────────────────────────────────────────────────────
// Chicken-head — tweed Fender, Ampeg, the EMS VCS3, and the meter selector on
// half the outboard ever built. The pointer IS the knob: a flat cream
// moulding with a broad tail and a beak, turning on a small dark hub.
//
// The only shape in the kit that reads as a switch rather than a control.
// ─────────────────────────────────────────────────────────────────────────
static CHICKEN_HEAD_TIERS: &[Tier] = &[
    // The hub it turns on. Small and dark: on the panel you see the moulding
    // and its shadow, not a disc.
    Tier::new(0.44, Paint::Flat("#0b0b0d"), Turns::Cap).outlined("rgba(255,255,255,0.07)", 0.7),
    Tier::new(
        0.38,
        Paint::Surface {
            css: "linear-gradient(178deg, #2c2c32 0%, #1c1c21 46%, #101014 100%)",
            finish: Finish::Matte,
            tint: false,
        },
        Turns::Cap,
    ),
];

pub static CHICKEN_HEAD: KnobSpec = KnobSpec {
    tiers: CHICKEN_HEAD_TIERS,
    index: Index::Beak {
        color: "#2a2621",
        body: 0.46,
    },
    collar_index: None,
    flutes: None,
    // Flat moulded plastic: a thread of light along the beak's top edge, no
    // more. The moulding is drawn with its own shadow and outline.
    specular: None,
    // The beak overhangs, so the printed scale moves out to clear its tip —
    // the same problem the Marconi's wing has, and the same fix.
    ring_offset: 8.0,
    numerals_on_knob: false,
    hub: None,
};

// ─────────────────────────────────────────────────────────────────────────
// Neve — the 1073 and its module family. A smooth turned outer ring around a
// GEARED cap, with a painted white index out at the cap's teeth.
//
// Which part is toothed is the whole tell: teeth outside a smooth cap is
// somebody else's knob. The metal is deliberately flat — a brushed gradient
// here reads as chrome and makes a row of these look plated rather than
// painted.
//
// The only concentric knob in the kit: on the module the ring and the cap are
// two different controls (a band's frequency and that band's gain), so the
// ring carries its own slim index.
// ─────────────────────────────────────────────────────────────────────────
static NEVE_TIERS: &[Tier] = &[
    // Painted metal, not a flat fill. The module's knobs are matte — a
    // brushed sweep here reads as chrome and makes a row of 1073s look
    // plated — but flat discs read as a sticker, which is what the whole
    // knob looked like: two grey circles and a scallop.
    Tier::new(
        1.0,
        Paint::Surface {
            css: "linear-gradient(160deg, #c8ccd1 0%, #b4b9bf 38%, #979da4 100%)",
            finish: Finish::Matte,
            tint: false,
        },
        Turns::Collar,
    )
    .shadowed(0.053)
    .outlined("rgba(0,0,0,0.42)", 0.8),
    // A turned groove near the rim, which is most of what a plain ring has
    // to say for itself.
    Tier::new(
        0.893,
        Paint::Groove {
            color: "rgba(0,0,0,0.13)",
            width: 0.8,
        },
        Turns::Collar,
    ),
    // Dark. The cap used to be #8f949b, a mid grey inside a near-white ring,
    // and the white line cut into it had nothing to read against — the whole
    // knob went to one pale washer at panel size. A 1073's cap is the dark
    // half of the pair; the ring is the light one.
    Tier::new(
        0.62,
        Paint::Surface {
            css: "linear-gradient(160deg, #5e646c 0%, #4e535a 40%, #383c42 100%)",
            finish: Finish::Matte,
            tint: true,
        },
        Turns::Cap,
    )
    // Deeper teeth and a harder edge. At 0.073 with a hairline outline the
    // gear merged into the ring behind it and the pair read as one washer —
    // and which part is toothed is the whole tell on this knob.
    .toothed(16, 0.095)
    .shadowed(0.04)
    .outlined("rgba(0,0,0,0.62)", 1.1),
    // The flat of the cap inside the teeth.
    Tier::new(0.527, Paint::Flat("rgba(255,255,255,0.06)"), Turns::Cap),
];

pub static NEVE: KnobSpec = KnobSpec {
    tiers: NEVE_TIERS,
    // Out at the cap's teeth, where it reads against the ring around it.
    index: Index::Bar {
        from: 0.248,
        to: 0.633,
        width: 3.4,
        color: "#f4f4f2",
    },
    // Slim, and dark: the collar is read off the printed dots, and the cap's
    // white line should be the one that catches the eye.
    collar_index: Some(Index::Bar {
        from: 0.747,
        to: 0.967,
        width: 2.2,
        color: "#2c3036",
    }),
    // The gear's teeth are the texture. Knurl lines over them read as dirt.
    flutes: None,
    // Matte painted metal takes no gloss blob.
    specular: None,
    ring_offset: 0.0,
    numerals_on_knob: false,
    hub: HUB,
};

// ─────────────────────────────────────────────────────────────────────────
// Dial — the Empirical Labs Distressor. A wide brushed dial whose numerals
// are printed on the skirt and turn WITH it, around a dark centre cap. The
// scale moving rather than a pointer moving is the whole look, and it is why
// the panel around a Distressor knob is bare.
// ─────────────────────────────────────────────────────────────────────────
static DIAL_TIERS: &[Tier] = &[
    Tier::new(
        1.0,
        Paint::Surface {
            css: BRUSHED_METAL,
            finish: Finish::Brushed,
            tint: false,
        },
        Turns::Cap,
    ),
    Tier::new(
        0.58,
        Paint::Surface {
            css: "radial-gradient(circle at 38% 28%, #55575c 0%, #303236 46%, #1c1e21 100%)",
            finish: Finish::Moulded,
            tint: true,
        },
        Turns::Cap,
    ),
];

pub static DIAL: KnobSpec = KnobSpec {
    tiers: DIAL_TIERS,
    index: Index::None,
    collar_index: None,
    flutes: Some(Flutes {
        count: 72,
        from: 0.44,
        to: 0.58,
        stroke: "rgba(0,0,0,0.30)",
        width: 1.2,
        shadow: Some("rgba(0,0,0,0.40)"),
        turns: Turns::Cap,
    }),
    specular: Some(dome(0.58)),
    ring_offset: 0.0,
    numerals_on_knob: true,
    hub: HUB,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::knob::KnobStyle;
    use crate::hardware::knob_kit::Edge;

    /// Every style in the enum resolves to a spec, and every spec is
    /// coherent: tiers run outermost-in, radii are fractions, and anything
    /// that says it is concentric actually has a cap to press.
    #[test]
    fn every_knob_in_the_kit_is_well_formed() {
        for style in KnobStyle::ALL {
            let spec = style.spec();
            let name = format!("{style:?}");

            assert!(
                !spec.tiers.is_empty(),
                "{name} has no tiers, so it draws nothing",
            );
            // The knob has to fill its own radius — but a chicken-head's
            // body IS its index, turning on a hub under half the width, so
            // the rule is about the silhouette rather than about tiers.
            let overhangs = matches!(
                spec.index,
                Index::Wing { .. } | Index::Nose { .. } | Index::Beak { .. }
            );
            assert!(
                spec.tiers[0].r == 1.0 || overhangs,
                "{name}'s widest tier is {} and no part of it overhangs, \
                 so the knob does not fill its own radius",
                spec.tiers[0].r,
            );
            for pair in spec.tiers.windows(2) {
                assert!(
                    pair[1].r <= pair[0].r,
                    "{name}'s tiers are not ordered outermost-first: \
                     {} follows {}",
                    pair[1].r,
                    pair[0].r,
                );
            }
            for tier in spec.tiers {
                assert!(
                    tier.r > 0.0 && tier.r <= 1.0,
                    "{name} has a tier at r={}, which is not a fraction of the knob",
                    tier.r,
                );
                if let Edge::Toothed { teeth, depth } = tier.edge {
                    assert!(teeth >= 3, "{name} has a {teeth}-toothed tier");
                    assert!(
                        depth > 0.0 && depth < tier.r,
                        "{name}'s teeth are deeper than the tier they are cut into",
                    );
                }
            }
            if let Some(f) = spec.flutes {
                assert!(
                    f.count > 0 && f.from < f.to && f.to <= 1.0,
                    "{name}'s flute band {}..{} is not a band",
                    f.from,
                    f.to,
                );
            }
        }
    }

    /// A knob whose halves are separate controls must have a cap wide enough
    /// to press *and* a ring left around it — otherwise one of the two
    /// controls is unreachable however the panel binds it.
    #[test]
    fn a_concentric_knob_leaves_room_to_press_both_halves() {
        for style in KnobStyle::ALL {
            let spec = style.spec();
            if !spec.is_concentric() {
                continue;
            }
            let cap = spec.cap_fraction();
            assert!(
                (0.35..=0.80).contains(&cap),
                "{style:?}'s cap is {cap} of the knob — too small to hit, or \
                 too big to leave a collar",
            );
            assert!(
                spec.tiers.iter().any(|t| t.turns == Turns::Collar),
                "{style:?} says it is concentric but has no collar tier",
            );
        }
    }

    /// Only the concentric knob carries a second index. Everything else reads
    /// as one control and a stray collar index would be a second, wrong,
    /// pointer on the face.
    #[test]
    fn only_a_concentric_knob_has_two_indices() {
        for style in KnobStyle::ALL {
            let spec = style.spec();
            assert_eq!(
                spec.collar_index.is_some(),
                spec.is_concentric(),
                "{style:?} disagrees with itself about being concentric",
            );
        }
    }

    /// An index has to sit on the knob, not float outside it — except a wing
    /// or a nose, whose overhang is the point and which buy room for it with
    /// `ring_offset`.
    #[test]
    fn an_index_stays_on_the_knob_unless_it_is_meant_to_overhang() {
        for style in KnobStyle::ALL {
            let spec = style.spec();
            match spec.index {
                Index::Bar { from, to, .. } => {
                    assert!(from < to, "{style:?}'s index runs backwards");
                    assert!(to <= 1.0, "{style:?}'s index runs off the knob");
                }
                Index::Blade { to, .. } => assert!(to <= 1.0),
                Index::Taper { from, to, .. } => {
                    assert!(from < to, "{style:?}'s index runs backwards");
                    assert!(to <= 1.0, "{style:?}'s index runs off the knob");
                }
                Index::Wing { .. } | Index::Nose { .. } | Index::Beak { .. } => assert!(
                    spec.ring_offset > 0.0,
                    "{style:?} overhangs the knob but does not move the \
                     printed scale out of its way",
                ),
                Index::None => assert!(
                    spec.numerals_on_knob,
                    "{style:?} has no index and no numerals — nothing reads it",
                ),
            }
        }
    }
}
