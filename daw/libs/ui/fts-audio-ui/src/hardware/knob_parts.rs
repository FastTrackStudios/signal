//! The kit: every knob this crate can draw, as a [`KnobSpec`] each.
//!
//! These are the actual parts the units wear, because the shape is most of
//! what you recognise a panel by before you read a word of it. One const per
//! knob — see [`knob_kit`](crate::hardware::knob_kit) for how to add one, and
//! the `knob_sheet` test for how to look at it.

use super::knob_kit::{dome, Finish, Flutes, Index, KnobSpec, Paint, Specular, Tier, Turns};

// ── Inks ─────────────────────────────────────────────────────────────────
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
static BAKELITE_TIERS: &[Tier] = &[solid(
    "radial-gradient(circle at 34% 26%, #4a4a4e 0%, #17171a 62%, #0b0b0d 100%)",
    Finish::Moulded,
)];

pub static BAKELITE: KnobSpec = KnobSpec {
    tiers: BAKELITE_TIERS,
    index: Index::Blade {
        to: 0.83,
        half_width: 0.113,
        color: LIGHT,
    },
    collar_index: None,
    flutes: None,
    specular: Some(dome(1.0)),
    ring_offset: 0.0,
    numerals_on_knob: false,
    hub: HUB,
};

// ─────────────────────────────────────────────────────────────────────────
// Metal — brushed, with a dark blade.
// ─────────────────────────────────────────────────────────────────────────
static METAL_TIERS: &[Tier] = &[solid(
    "radial-gradient(circle at 34% 26%, #d8d8d4 0%, #9a9a96 58%, #6d6d69 100%)",
    Finish::Brushed,
)];

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
// Skirted — the generic vintage outboard knob: a fluted body with a smooth
// cap inside it, read by a short bar on the rim.
// ─────────────────────────────────────────────────────────────────────────
static SKIRTED_TIERS: &[Tier] = &[
    solid(
        "radial-gradient(circle at 38% 24%, #4c4c50 0%, #232326 38%, #101012 72%, #0a0a0c 100%)",
        Finish::Moulded,
    ),
    Tier::new(0.62, Paint::Flat("rgba(255,255,255,0.035)"), Turns::Cap)
        .outlined("rgba(0,0,0,0.45)", 0.8),
];

pub static SKIRTED: KnobSpec = KnobSpec {
    tiers: SKIRTED_TIERS,
    index: Index::Bar {
        from: 0.633,
        to: 0.967,
        width: 3.8,
        color: LIGHT,
    },
    collar_index: None,
    flutes: None,
    specular: Some(dome(1.0)),
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
    Tier::new(1.0, Paint::Flat("#131316"), Turns::Cap)
        .toothed(22, 0.08)
        .shadowed(0.06)
        .outlined("rgba(0,0,0,0.7)", 0.7),
    // The step up to the body, read as a shadowed wall rather than an edge.
    Tier::new(0.74, Paint::Flat("#0d0d10"), Turns::Cap),
    Tier::new(0.70, Paint::Flat("#242429"), Turns::Cap),
    Tier::new(0.52, Paint::Flat("#2b2b31"), Turns::Cap),
    Tier::new(0.30, Paint::Flat("#313138"), Turns::Cap),
];

pub static DAKA: KnobSpec = KnobSpec {
    tiers: DAKA_TIERS,
    index: Index::Bar {
        from: 0.29,
        to: 0.95,
        width: 2.4,
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
        dx: -0.34,
        dy: -0.30,
        fill: "radial-gradient(ellipse at 50% 50%, rgba(255,255,255,0.13) 0%, \
               rgba(255,255,255,0.0) 72%)",
        rotate: -32.0,
        rim: Some("rgba(255,255,255,0.12)"),
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
            css: "radial-gradient(circle at 44% 36%, #202024 0%, #101013 58%, #08080a 100%)",
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
    index: Index::Bar {
        from: 0.20,
        to: 0.90,
        width: 3.6,
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
    // Flat top: a sheen across the face, not a highlight on a dome.
    specular: Some(Specular {
        w: 0.62,
        h: 0.42,
        dx: -0.46,
        dy: -0.40,
        fill: "linear-gradient(150deg, rgba(255,255,255,0.20) 0%, \
               rgba(255,255,255,0.04) 46%, rgba(255,255,255,0.0) 72%)",
        rotate: 0.0,
        rim: None,
    }),
    ring_offset: 0.0,
    numerals_on_knob: false,
    hub: HUB,
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
    Tier::new(
        0.56,
        Paint::Surface {
            css: "linear-gradient(148deg, #e2e2e0 0%, #b4b4b2 34%, #8e8e8c 62%, #cfcfcd 100%)",
            finish: Finish::Brushed,
            tint: true,
        },
        Turns::Cap,
    ),
    // Two turned rings on the brushed face.
    Tier::new(
        0.347,
        Paint::Groove {
            color: "rgba(0,0,0,0.16)",
            width: 0.7,
        },
        Turns::Cap,
    ),
    Tier::new(
        0.168,
        Paint::Groove {
            color: "rgba(0,0,0,0.13)",
            width: 0.6,
        },
        Turns::Cap,
    ),
];

pub static SILVER_TOP: KnobSpec = KnobSpec {
    tiers: SILVER_TOP_TIERS,
    // On the collar: from just outside the cap to just inside the rim.
    index: Index::Bar {
        from: 0.677,
        to: 0.917,
        width: 3.0,
        color: "#f4f4f2",
    },
    collar_index: None,
    // Fine knurling around the cap's edge.
    flutes: Some(Flutes {
        count: 54,
        from: 0.48,
        to: 0.56,
        stroke: "rgba(0,0,0,0.34)",
        width: 1.2,
        shadow: Some("rgba(0,0,0,0.40)"),
        turns: Turns::Cap,
    }),
    specular: Some(dome(0.56)),
    ring_offset: 0.0,
    numerals_on_knob: false,
    hub: HUB,
};

// ─────────────────────────────────────────────────────────────────────────
// Metal-fluted — dbx and its generation. Brushed aluminium, a fluted rim, a
// dark centre cap, read by a line across the metal.
// ─────────────────────────────────────────────────────────────────────────
static METAL_FLUTED_TIERS: &[Tier] = &[
    solid(
        "linear-gradient(152deg, #e8e8e6 0%, #c0c0be 30%, #979795 62%, #d2d2d0 100%)",
        Finish::Brushed,
    ),
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
static POINTER_TIERS: &[Tier] = &[solid(
    "radial-gradient(circle at 34% 24%, #48484d 0%, #232327 44%, #0f0f12 100%)",
    Finish::Moulded,
)];

pub static POINTER: KnobSpec = KnobSpec {
    tiers: POINTER_TIERS,
    index: Index::Nose { color: LIGHT },
    collar_index: None,
    flutes: None,
    specular: Some(dome(1.0)),
    // The nose reaches past the body by design — that is how it points — so a
    // ring drawn for a flush knob lands underneath it.
    ring_offset: 13.0,
    numerals_on_knob: false,
    hub: HUB,
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
    Tier::new(1.0, Paint::Flat("#c3c8cd"), Turns::Collar)
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
    Tier::new(0.62, Paint::Tinted("#8f949b"), Turns::Cap)
        .toothed(16, 0.073)
        .shadowed(0.04)
        .outlined("rgba(0,0,0,0.5)", 0.7),
    // The flat of the cap inside the teeth.
    Tier::new(0.527, Paint::Flat("rgba(255,255,255,0.07)"), Turns::Cap),
];

pub static NEVE: KnobSpec = KnobSpec {
    tiers: NEVE_TIERS,
    // Out at the cap's teeth, where it reads against the ring around it.
    index: Index::Bar {
        from: 0.248,
        to: 0.633,
        width: 3.0,
        color: "#f4f4f2",
    },
    // Slim, and dark: the collar is read off the printed dots, and the cap's
    // white line should be the one that catches the eye.
    collar_index: Some(Index::Bar {
        from: 0.747,
        to: 0.967,
        width: 2.2,
        color: "#3c4046",
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
            css: "radial-gradient(circle at 40% 26%, #e8e8e6 0%, #c2c2c0 44%, \
                  #9a9a98 78%, #cbcbc9 100%)",
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
            assert_eq!(
                spec.tiers[0].r, 1.0,
                "{name}'s outermost tier must fill the knob",
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
                Index::Wing { .. } | Index::Nose { .. } => assert!(
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
