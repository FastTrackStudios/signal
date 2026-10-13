//! Knobs of three pieces of 1970s–2000s outboard, matched from photographs:
//!
//! - `knob.knurled_insert` — the desktop VCA compressor's: a glossy black
//!   cylinder knurled all round its side, with a small polished aluminium
//!   insert in the top carrying a black line.
//! - `knob.capped_skirt` — the rack bus compressor's: a pale grey fluted
//!   skirt with a flat coloured cap (blue by default) set into it and a
//!   white line across the cap, inside a printed dot ring.
//! - `knob.numbered_cream` — the character compressor's big dial: a cream
//!   skirt engraved with fine graduations and 0–10 numerals that turn past
//!   a fixed panel index, a raised cream grip, a black ring and a textured
//!   cream disc on top.
//!
//! Each is a [`KnobSpec`] walked by the family's own builder, so they follow
//! every kit rule (the Needle is the turning `body`, the light stays still).

use crate::document::proto::NodeId;
use crate::surface::widgets::Builder;

use super::spec::{
    Edge, Finish, Flutes, Grad, Index, KnobSpec, RingKind, SpecFill, Specular, Tier, TierPaint,
};
use crate::surface::kit::registry::{BuildFn, ComponentDef, Family, Place};

const fn tier(r: f64, paint: TierPaint) -> Tier {
    Tier {
        r,
        edge: Edge::Round,
        paint,
        shadow: 0.0,
        stroke: None,
    }
}

const fn surface(r: f64, grad: Grad, finish: Finish, tint: bool) -> Tier {
    tier(r, TierPaint::Surface { grad, finish, tint })
}

const fn shadowed(mut t: Tier, drop: f64) -> Tier {
    t.shadow = drop;
    t
}

const fn outlined(mut t: Tier, color: &'static str, width: f64) -> Tier {
    t.stroke = Some((color, width));
    t
}

/// A soft highlight sized to the lit tier `r` (fraction of the knob).
const fn highlight(
    r: f64,
    strength: &'static [(f64, &'static str)],
    rim: Option<&'static str>,
) -> Specular {
    Specular {
        w: 0.62 * r,
        h: 0.30 * r,
        dx: -0.02 * r,
        dy: -0.24 * r,
        fill: SpecFill::Ellipse(strength),
        rotate: 0.0,
        rim,
        r,
    }
}

// ── the desktop VCA's knurled knob with an aluminium insert ─────────────
pub static KNURLED_INSERT: KnobSpec = KnobSpec {
    tiers: &[
        // The knurled wall, seen from the front: glossy black, lit on top.
        outlined(
            shadowed(
                surface(
                    1.0,
                    Grad::Linear(
                        178.0,
                        &[
                            (0.0, "#3a3b40"),
                            (0.30, "#1b1c20"),
                            (0.70, "#0c0c0f"),
                            (1.0, "#050507"),
                        ],
                    ),
                    Finish::Moulded,
                    true,
                ),
                0.06,
            ),
            "rgba(0,0,0,0.85)",
            0.8,
        ),
        // The flat black top face the insert sits in.
        outlined(
            surface(
                0.74,
                Grad::Radial(
                    0.40,
                    0.30,
                    &[(0.0, "#2a2b30"), (0.55, "#141518"), (1.0, "#0a0a0c")],
                ),
                Finish::FlatTop,
                true,
            ),
            "rgba(255,255,255,0.10)",
            0.6,
        ),
        // The polished insert.
        outlined(
            surface(0.56, Grad::Brushed, Finish::Brushed, false),
            "rgba(0,0,0,0.60)",
            0.9,
        ),
        tier(
            0.30,
            TierPaint::Groove {
                color: "rgba(0,0,0,0.12)",
                width: 0.6,
            },
        ),
    ],
    // A black line across the insert, centre to rim.
    index: Index::Bar {
        from: 0.0,
        to: 0.54,
        width: 2.4,
        color: "#0c0d10",
    },
    collar_index: None,
    // The knurl: fine ribs round the wall, catching the light on one side.
    flutes: Some(Flutes {
        count: 44,
        from: 0.76,
        to: 1.0,
        stroke: "rgba(255,255,255,0.16)",
        width: 1.1,
        shadow: Some("rgba(0,0,0,0.55)"),
    }),
    specular: Some(highlight(
        0.56,
        &[
            (0.0, "rgba(255,255,255,0.30)"),
            (0.5, "rgba(255,255,255,0.10)"),
            (0.8, "rgba(255,255,255,0.0)"),
        ],
        Some("rgba(255,255,255,0.35)"),
    )),
    ring_offset: 0.0,
    hub: None,
    sweep: 300.0,
    ring: RingKind::Ticks,
    scale: &["", "", "", "", "", ""],
    variants: &["black"],
    tints: &[None],
};

// ── the rack bus compressor's capped skirt ───────────────────────────────
pub static CAPPED_SKIRT: KnobSpec = KnobSpec {
    tiers: &[
        // The pale grey skirt: moulded plastic, fluted round its edge.
        outlined(
            shadowed(
                surface(
                    1.0,
                    Grad::Linear(
                        165.0,
                        &[(0.0, "#f1f3f5"), (0.45, "#d5d9de"), (1.0, "#a9aeb5")],
                    ),
                    Finish::Matte,
                    false,
                ),
                0.06,
            ),
            "rgba(0,0,0,0.35)",
            0.7,
        ),
        // The flat coloured cap, set into it.
        outlined(
            surface(
                0.80,
                Grad::Linear(
                    162.0,
                    &[(0.0, "#4f93dd"), (0.45, "#2a6fc4"), (1.0, "#1d4f93")],
                ),
                Finish::FlatTop,
                true,
            ),
            "rgba(0,0,0,0.40)",
            0.8,
        ),
    ],
    index: Index::Bar {
        from: 0.04,
        to: 0.79,
        width: 2.8,
        color: "#f7f8f8",
    },
    collar_index: None,
    flutes: Some(Flutes {
        count: 30,
        from: 0.82,
        to: 1.0,
        stroke: "rgba(0,0,0,0.22)",
        width: 1.3,
        shadow: Some("rgba(255,255,255,0.35)"),
    }),
    specular: Some(Specular {
        w: 1.0,
        h: 1.0,
        dx: 0.0,
        dy: 0.0,
        fill: SpecFill::Linear(
            150.0,
            &[
                (0.0, "rgba(255,255,255,0.26)"),
                (0.36, "rgba(255,255,255,0.06)"),
                (0.62, "rgba(255,255,255,0.0)"),
            ],
        ),
        rotate: 0.0,
        rim: None,
        r: 0.80,
    }),
    ring_offset: 0.0,
    hub: None,
    sweep: 300.0,
    ring: RingKind::Dots,
    scale: &["0", "", "", "", "", "", "", "", "", "", "10"],
    variants: &["blue", "grey", "red", "green", "black", "yellow"],
    tints: &[
        None,
        Some("#8b9097"),
        Some("#b3302b"),
        Some("#2c8f5a"),
        Some("#26282c"),
        Some("#e0b52e"),
    ],
};

// ── the character compressor's big cream dial ────────────────────────────
pub static NUMBERED_CREAM: KnobSpec = KnobSpec {
    tiers: &[
        // The numbered skirt.
        outlined(
            shadowed(
                surface(
                    1.0,
                    Grad::Linear(
                        172.0,
                        &[(0.0, "#f6f1e0"), (0.5, "#e6dfc8"), (1.0, "#c9c0a4")],
                    ),
                    Finish::Matte,
                    true,
                ),
                0.05,
            ),
            "rgba(0,0,0,0.45)",
            0.7,
        ),
        // The raised grip, standing proud of the skirt.
        outlined(
            shadowed(
                surface(
                    0.64,
                    Grad::Radial(
                        0.38,
                        0.28,
                        &[(0.0, "#fbf7ea"), (0.55, "#e9e2cc"), (1.0, "#cfc6aa")],
                    ),
                    Finish::Moulded,
                    true,
                ),
                0.05,
            ),
            "rgba(60,50,30,0.45)",
            0.8,
        ),
        // The black ring round the top.
        tier(0.50, TierPaint::Flat("#121214")),
        // The textured cream disc.
        outlined(
            surface(
                0.43,
                Grad::Linear(
                    170.0,
                    &[(0.0, "#f3eedd"), (0.5, "#e4ddc6"), (1.0, "#d2c9ad")],
                ),
                Finish::FlatTop,
                true,
            ),
            "rgba(0,0,0,0.35)",
            0.6,
        ),
    ],
    index: Index::None,
    collar_index: None,
    // The engraved graduations round the skirt's edge, turning with it.
    flutes: Some(Flutes {
        count: 110,
        from: 0.90,
        to: 0.985,
        stroke: "rgba(20,20,20,0.72)",
        width: 0.55,
        shadow: None,
    }),
    specular: Some(highlight(
        0.64,
        &[
            (0.0, "rgba(255,255,255,0.28)"),
            (0.52, "rgba(255,255,255,0.08)"),
            (0.80, "rgba(255,255,255,0.0)"),
        ],
        Some("rgba(255,255,255,0.30)"),
    )),
    ring_offset: 0.0,
    hub: None,
    sweep: 300.0,
    ring: RingKind::OnKnob,
    scale: &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"],
    variants: &["cream", "white", "black"],
    tints: &[None, Some("#f4f3ee"), Some("#2a2b2f")],
};

fn knurled_insert(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    super::build(b, parent, p, &KNURLED_INSERT, "Knurled insert", 60.0)
}

fn capped_skirt(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    super::build(b, parent, p, &CAPPED_SKIRT, "Capped skirt", 44.0)
}

fn numbered_cream(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    super::build(b, parent, p, &NUMBERED_CREAM, "Numbered cream dial", 110.0)
}

/// These styles' components.
pub(crate) fn defs() -> Vec<ComponentDef> {
    let def = |id, title, description, spec: &'static KnobSpec, default_size, build: BuildFn| {
        ComponentDef {
            id,
            family: Family::Knob,
            title,
            description,
            variants: spec.variants,
            default_size,
            binds: "param",
            build,
        }
    };
    vec![
        def(
            "knob.knurled_insert",
            "Knurled insert",
            "Glossy black cylinder knurled all round, with a small polished aluminium insert in its top carrying a black line — the 1970s desktop VCA compressor's knob. Prints a tick ring; pass numerals as `scale`.",
            &KNURLED_INSERT,
            60.0,
            knurled_insert,
        ),
        def(
            "knob.capped_skirt",
            "Capped skirt",
            "A pale grey fluted skirt with a flat coloured cap set into it and a white line across the cap, inside a printed dot ring — the rack bus compressor's small knob. Cap colours are variants (blue default).",
            &CAPPED_SKIRT,
            44.0,
            capped_skirt,
        ),
        def(
            "knob.numbered_cream",
            "Numbered cream dial",
            "A big cream dial: a skirt engraved with fine graduations and 0–10 numerals that turn past a fixed panel index, a raised grip, a black ring and a textured disc on top — the character compressor's. The panel around it is bare.",
            &NUMBERED_CREAM,
            110.0,
            numbered_cream,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_a_tint_entry() {
        for spec in [&KNURLED_INSERT, &CAPPED_SKIRT, &NUMBERED_CREAM] {
            assert_eq!(spec.variants.len(), spec.tints.len());
        }
    }
}
