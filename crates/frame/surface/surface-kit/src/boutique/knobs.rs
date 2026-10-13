//! Boutique pedal and amp knobs, as [`KnobSpec`]s walked by the kit's knob
//! builder — plus the chevron cap, whose stripes are drawn onto the turning
//! body after the builder has made it.

use document_proto::{NodeId, StrokeCap};
use surface_widgets::Builder;

use crate::draw;
use crate::knobs::spec::{
    Edge, Finish, Flutes, Grad, Index, KnobSpec, RingKind, SpecFill, Specular, Tier, TierPaint,
};
use crate::registry::{ComponentDef, Family, Place};

const fn tier(r: f64, paint: TierPaint) -> Tier {
    Tier {
        r,
        edge: Edge::Round,
        paint,
        shadow: 0.0,
        stroke: None,
    }
}
const fn flat(r: f64, c: &'static str) -> Tier {
    tier(r, TierPaint::Flat(c))
}
const fn surface(r: f64, grad: Grad, finish: Finish, tint: bool) -> Tier {
    tier(r, TierPaint::Surface { grad, finish, tint })
}
const fn shadowed(mut t: Tier, s: f64) -> Tier {
    t.shadow = s;
    t
}
const fn outlined(mut t: Tier, c: &'static str, w: f64) -> Tier {
    t.stroke = Some((c, w));
    t
}
const fn toothed(mut t: Tier, teeth: u32, depth: f64) -> Tier {
    t.edge = Edge::Toothed { teeth, depth };
    t
}

/// A glossy moulded dome's highlight: a soft oval up and left.
const fn gloss(r: f64, a: &'static [(f64, &'static str)]) -> Specular {
    Specular {
        w: 0.62 * r,
        h: 0.36 * r,
        dx: -0.1 * r,
        dy: -0.2 * r,
        fill: SpecFill::Ellipse(a),
        rotate: -18.0,
        rim: Some("rgba(255,255,255,0.18)"),
        r,
    }
}
const GLOSS: &[(f64, &str)] = &[
    (0.0, "rgba(255,255,255,0.34)"),
    (0.5, "rgba(255,255,255,0.10)"),
    (0.85, "rgba(255,255,255,0.0)"),
];
const SOFT: &[(f64, &str)] = &[
    (0.0, "rgba(255,255,255,0.16)"),
    (0.55, "rgba(255,255,255,0.05)"),
    (0.85, "rgba(255,255,255,0.0)"),
];
const NONE: &[&str] = &[];

// ── Dome: the smooth, glossy pedal cap (pink, teal, black) ──────────────
pub static DOME: KnobSpec = KnobSpec {
    tiers: &[
        outlined(shadowed(flat(1.0, "#0b0b0e"), 0.09), "rgba(0,0,0,0.6)", 0.8),
        surface(
            0.96,
            Grad::Radial(
                0.4,
                0.3,
                &[(0.0, "#5a5a62"), (0.55, "#2a2a30"), (1.0, "#101014")],
            ),
            Finish::Moulded,
            true,
        ),
        surface(
            0.82,
            Grad::Radial(
                0.4,
                0.3,
                &[(0.0, "#62626b"), (0.6, "#303036"), (1.0, "#18181c")],
            ),
            Finish::Moulded,
            true,
        ),
    ],
    index: Index::Bar {
        from: 0.52,
        to: 0.94,
        width: 2.8,
        color: "#f4f2ee",
    },
    collar_index: None,
    flutes: None,
    specular: Some(gloss(0.9, GLOSS)),
    ring_offset: 0.0,
    hub: None,
    sweep: 300.0,
    ring: RingKind::Ticks,
    scale: NONE,
    variants: &["black", "pink", "teal", "brown", "cream", "blue", "red"],
    tints: &[
        None,
        Some("#b2477f"),
        Some("#1f8793"),
        Some("#3a2c27"),
        Some("#e6cdb0"),
        Some("#2e5c9a"),
        Some("#a8322c"),
    ],
};

// ── Bar dome: a tall black cap with a white bar across its top ──────────
pub static BAR_DOME: KnobSpec = KnobSpec {
    tiers: &[
        outlined(
            shadowed(flat(1.0, "#060608"), 0.1),
            "rgba(255,255,255,0.08)",
            0.7,
        ),
        surface(
            0.94,
            Grad::Linear(
                90.0,
                &[
                    (0.0, "#050506"),
                    (0.3, "#2a2a2e"),
                    (0.5, "#4a4a50"),
                    (0.7, "#2a2a2e"),
                    (1.0, "#050506"),
                ],
            ),
            Finish::Matte,
            false,
        ),
        surface(
            0.7,
            Grad::Radial(
                0.42,
                0.3,
                &[(0.0, "#44444a"), (0.6, "#1e1e22"), (1.0, "#0e0e10")],
            ),
            Finish::Moulded,
            true,
        ),
    ],
    index: Index::Bar {
        from: 0.0,
        to: 0.92,
        width: 3.6,
        color: "#f4f2ee",
    },
    collar_index: None,
    flutes: None,
    specular: Some(gloss(0.7, SOFT)),
    ring_offset: 0.0,
    hub: None,
    sweep: 300.0,
    ring: RingKind::Ticks,
    scale: NONE,
    variants: &["black"],
    tints: &[None],
};

// ── Knurled dome: a dark cap with a knurled skirt (a vintage overdrive) ──
pub static KNURLED_DOME: KnobSpec = KnobSpec {
    tiers: &[
        outlined(shadowed(flat(1.0, "#0b0908"), 0.09), "rgba(0,0,0,0.6)", 0.8),
        surface(
            0.97,
            Grad::Radial(
                0.4,
                0.3,
                &[(0.0, "#4a3a32"), (0.6, "#251c18"), (1.0, "#110c0a")],
            ),
            Finish::Moulded,
            true,
        ),
        surface(
            0.8,
            Grad::Radial(
                0.4,
                0.28,
                &[(0.0, "#6a544a"), (0.55, "#3a2c26"), (1.0, "#1a1310")],
            ),
            Finish::Moulded,
            true,
        ),
    ],
    index: Index::Bar {
        from: 0.84,
        to: 1.08,
        width: 2.2,
        color: "#f4f2ee",
    },
    collar_index: None,
    flutes: Some(Flutes {
        count: 44,
        from: 0.8,
        to: 0.97,
        stroke: "rgba(255,255,255,0.10)",
        width: 1.1,
        shadow: Some("rgba(0,0,0,0.45)"),
    }),
    specular: Some(gloss(0.8, SOFT)),
    ring_offset: 0.0,
    hub: None,
    sweep: 300.0,
    ring: RingKind::Ticks,
    scale: NONE,
    variants: &["brown", "black"],
    tints: &[None, Some("#1c1c20")],
};

// ── Chevron: a black skirt under a coloured cap printed with chevrons ───
pub static CHEVRON: KnobSpec = KnobSpec {
    tiers: &[
        outlined(
            shadowed(flat(1.0, "#08080a"), 0.09),
            "rgba(255,255,255,0.08)",
            0.7,
        ),
        surface(
            0.97,
            Grad::Radial(
                0.4,
                0.3,
                &[(0.0, "#3a3a40"), (0.6, "#16161a"), (1.0, "#08080a")],
            ),
            Finish::Moulded,
            false,
        ),
        outlined(
            surface(
                0.8,
                Grad::Radial(
                    0.4,
                    0.3,
                    &[(0.0, "#c8f0cc"), (0.55, "#86cf93"), (1.0, "#4f9a5e")],
                ),
                Finish::Moulded,
                true,
            ),
            "rgba(0,0,0,0.35)",
            0.8,
        ),
    ],
    index: Index::Bar {
        from: 0.0,
        to: 0.8,
        width: 2.6,
        color: "#101012",
    },
    collar_index: None,
    flutes: None,
    specular: Some(gloss(0.8, GLOSS)),
    ring_offset: 0.0,
    hub: None,
    sweep: 300.0,
    ring: RingKind::Ticks,
    scale: NONE,
    variants: &["green", "pink", "teal"],
    tints: &[None, Some("#e59ac0"), Some("#7fd0d6")],
};

// ── Ring insert: a black cap in stacked coral rings (a studio reverb) ───
pub static RING_INSERT: KnobSpec = KnobSpec {
    tiers: &[
        outlined(shadowed(flat(1.0, "#5a1d14"), 0.08), "rgba(0,0,0,0.5)", 0.8),
        surface(
            0.97,
            Grad::Radial(
                0.4,
                0.32,
                &[(0.0, "#f08a68"), (0.6, "#d8583e"), (1.0, "#8e2c1e")],
            ),
            Finish::Moulded,
            false,
        ),
        surface(
            0.84,
            Grad::Radial(
                0.4,
                0.32,
                &[(0.0, "#f6b184"), (0.7, "#ee905e"), (1.0, "#c0603e")],
            ),
            Finish::Moulded,
            false,
        ),
        surface(
            0.7,
            Grad::Radial(
                0.4,
                0.32,
                &[(0.0, "#fbd4a8"), (0.7, "#f2b27c"), (1.0, "#c77e52")],
            ),
            Finish::Moulded,
            false,
        ),
        outlined(
            surface(
                0.56,
                Grad::Radial(
                    0.45,
                    0.35,
                    &[(0.0, "#2a2524"), (0.7, "#141111"), (1.0, "#0a0808")],
                ),
                Finish::FlatTop,
                false,
            ),
            "rgba(0,0,0,0.6)",
            1.2,
        ),
    ],
    index: Index::Bar {
        from: 0.54,
        to: 0.98,
        width: 2.4,
        color: "#1a0f0c",
    },
    collar_index: None,
    flutes: None,
    specular: Some(gloss(0.97, SOFT)),
    ring_offset: 0.0,
    hub: None,
    sweep: 300.0,
    ring: RingKind::Ticks,
    scale: NONE,
    variants: &["coral"],
    tints: &[None],
};

// ── Fluted white: a big white scalloped cap with a black line (a boost) ──
pub static FLUTED_WHITE: KnobSpec = KnobSpec {
    tiers: &[
        outlined(
            shadowed(
                toothed(
                    surface(
                        1.0,
                        Grad::Radial(
                            0.4,
                            0.3,
                            &[(0.0, "#ffffff"), (0.6, "#e6e4df"), (1.0, "#a9a7a2")],
                        ),
                        Finish::Moulded,
                        false,
                    ),
                    9,
                    0.1,
                ),
                0.08,
            ),
            "rgba(0,0,0,0.25)",
            0.8,
        ),
        surface(
            0.7,
            Grad::Radial(
                0.42,
                0.32,
                &[(0.0, "#ffffff"), (0.7, "#efede9"), (1.0, "#cfcdc8")],
            ),
            Finish::FlatTop,
            false,
        ),
    ],
    index: Index::Bar {
        from: 0.12,
        to: 0.84,
        width: 3.8,
        color: "#141414",
    },
    collar_index: None,
    flutes: None,
    specular: Some(gloss(0.7, SOFT)),
    ring_offset: 0.0,
    hub: None,
    sweep: 300.0,
    ring: RingKind::Ticks,
    scale: NONE,
    variants: &["white"],
    tints: &[None],
};

// ── Cream cap: a big flat-topped cream knob with a dot (a reverb/trem unit)
pub static CREAM_CAP: KnobSpec = KnobSpec {
    tiers: &[
        outlined(shadowed(flat(1.0, "#3a2418"), 0.1), "rgba(0,0,0,0.5)", 0.8),
        surface(
            0.97,
            Grad::Linear(
                180.0,
                &[(0.0, "#f3d8bf"), (0.5, "#d9ad8a"), (1.0, "#9c6d50")],
            ),
            Finish::Moulded,
            false,
        ),
        outlined(
            surface(
                0.72,
                Grad::Radial(
                    0.45,
                    0.35,
                    &[(0.0, "#f8e3cf"), (0.7, "#ecc9a9"), (1.0, "#cf9f7c")],
                ),
                Finish::FlatTop,
                false,
            ),
            "rgba(90,50,30,0.35)",
            1.0,
        ),
    ],
    index: Index::Bar {
        from: 0.52,
        to: 0.6,
        width: 3.4,
        color: "#3a2418",
    },
    collar_index: None,
    flutes: None,
    specular: Some(gloss(0.97, SOFT)),
    ring_offset: 0.0,
    hub: None,
    sweep: 300.0,
    ring: RingKind::Ticks,
    scale: NONE,
    variants: &["cream"],
    tints: &[None],
};

// ── Numbered skirt: a black skirt printed 1–10, a chrome cap (a '60s combo)
pub static NUMBERED_SKIRT: KnobSpec = KnobSpec {
    tiers: &[
        outlined(
            shadowed(flat(1.0, "#050506"), 0.08),
            "rgba(255,255,255,0.1)",
            0.7,
        ),
        surface(
            0.97,
            Grad::Radial(
                0.42,
                0.3,
                &[(0.0, "#2e2e33"), (0.6, "#141417"), (1.0, "#070708")],
            ),
            Finish::Moulded,
            false,
        ),
        flat(0.6, "rgba(0,0,0,0.6)"),
        outlined(
            surface(0.52, Grad::Brushed, Finish::Brushed, false),
            "rgba(0,0,0,0.5)",
            0.8,
        ),
    ],
    index: Index::None,
    collar_index: None,
    flutes: None,
    specular: Some(gloss(0.52, GLOSS)),
    ring_offset: 0.0,
    hub: None,
    sweep: 300.0,
    ring: RingKind::OnKnob,
    scale: &["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"],
    variants: &["black"],
    tints: &[None],
};

fn dome(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    crate::knobs::build(b, parent, p, &DOME, "Dome", 44.0)
}
fn bar_dome(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    crate::knobs::build(b, parent, p, &BAR_DOME, "Bar dome", 44.0)
}
fn knurled_dome(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    crate::knobs::build(b, parent, p, &KNURLED_DOME, "Knurled dome", 48.0)
}
fn ring_insert(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    crate::knobs::build(b, parent, p, &RING_INSERT, "Ring insert", 44.0)
}
fn fluted_white(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    crate::knobs::build(b, parent, p, &FLUTED_WHITE, "Fluted white", 48.0)
}
fn cream_cap(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    crate::knobs::build(b, parent, p, &CREAM_CAP, "Cream cap", 64.0)
}
/// The numbered skirt: the builder prints a skirt's numerals dark (for a
/// silver dial); on this black skirt they are white.
fn numbered_skirt(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let root = crate::knobs::build(b, parent, p, &NUMBERED_SKIRT, "Numbered skirt", 40.0);
    if let Some(body) = find(b, root, "body") {
        recolor_text(b, body, "#ece7dc");
    }
    root
}

fn recolor_text(b: &mut Builder<'_>, id: NodeId, color: &str) {
    let kids = b
        .scene
        .node(id)
        .map(|n| n.children.clone())
        .unwrap_or_default();
    for k in kids {
        if let Some(n) = b.scene.node_mut(k)
            && matches!(n.kind, document_proto::NodeKind::Text(_))
        {
            n.fills = vec![draw::solid(color)];
        }
        recolor_text(b, k, color);
    }
}

/// The chevron cap: the spec's knob, then stripes printed on the cap (on
/// the turning body, so they turn with it) — Vs opening toward the index.
fn chevron(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let root = crate::knobs::build(b, parent, p, &CHEVRON, "Chevron", 40.0);
    let Some(body) = find(b, root, "body") else {
        return root;
    };
    let (side, _) = b
        .scene
        .node(body)
        .map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    let d = p.size.unwrap_or(40.0).max(12.0);
    let (c, r) = (side / 2.0, d / 2.0 * 0.8);
    let ink = match p.variant.as_deref() {
        Some("pink") => "#8a3a64",
        Some("teal") => "#2f7f86",
        _ => "#3f8a50",
    };
    let mut lines: Vec<Vec<(f64, f64)>> = Vec::new();
    let step = r / 3.2;
    let mut k = -3.0;
    while k <= 4.0 {
        // A V whose point is at (c, c + k·step), arms rising at 45°.
        let apex = c + k * step;
        for dir in [-1.0, 1.0] {
            let mut run: Vec<(f64, f64)> = Vec::new();
            for i in 0..=40 {
                let t = f64::from(i) / 40.0 * r * 1.6;
                let (x, y) = (c + dir * t, apex - t * 0.8);
                if (x - c).hypot(y - c) < r * 0.93 {
                    run.push((x, y));
                } else if run.len() > 1 {
                    lines.push(std::mem::take(&mut run));
                } else {
                    run.clear();
                }
            }
            if run.len() > 1 {
                lines.push(run);
            }
        }
        k += 1.0;
    }
    let mut dpath = String::new();
    for l in &lines {
        for (i, (x, y)) in l.iter().enumerate() {
            dpath.push_str(&format!(
                "{} {x:.2} {y:.2} ",
                if i == 0 { "M" } else { "L" }
            ));
        }
    }
    let mut n = draw::path(
        "chevrons",
        0.0,
        0.0,
        side,
        side,
        dpath,
        draw::solid("#00000000"),
    );
    n.fills.clear();
    n.stroke = document_proto::Stroke {
        paints: vec![draw::solid(ink)],
        weight: (d * 0.035).max(0.8),
        cap: StrokeCap::Round,
        ..document_proto::Stroke::default()
    };
    // Under the index: the stripes are printed, the index is on top.
    let first = b.scene.node(body).and_then(|n| n.children.first().copied());
    let id = b.add(body, n);
    if let (Some(first), Some(bn)) = (first, b.scene.node_mut(body)) {
        bn.children.retain(|c| *c != id);
        let at = bn.children.iter().position(|c| *c == first).unwrap_or(0);
        bn.children.insert(at, id);
    }
    root
}

/// The first node named `name` under `root`, depth first.
pub(crate) fn find(b: &Builder<'_>, root: NodeId, name: &str) -> Option<NodeId> {
    let n = b.scene.node(root)?;
    for c in n.children.clone() {
        if b.scene.node(c).is_some_and(|k| k.name == name) {
            return Some(c);
        }
        if let Some(f) = find(b, c, name) {
            return Some(f);
        }
    }
    None
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    let knob = |id: &'static str,
                title: &'static str,
                description: &'static str,
                spec: &'static KnobSpec,
                default_size: f64,
                build: crate::registry::BuildFn| ComponentDef {
        id,
        family: Family::Knob,
        title,
        description,
        variants: spec.variants,
        default_size,
        binds: "param",
        build,
    };
    vec![
        knob(
            "knob.dome",
            "Dome",
            "A smooth glossy moulded dome on a dark skirt with a short white line at the rim — the cap on hand-built boutique pedals (pink filters, teal delays, black anything). No printed scale by default.",
            &DOME,
            44.0,
            dome,
        ),
        knob(
            "knob.bar_dome",
            "Bar dome",
            "A tall black moulded cap with a white bar running across its top, lit down its sides — a boutique delay's knob. No printed scale by default.",
            &BAR_DOME,
            44.0,
            bar_dome,
        ),
        knob(
            "knob.knurled_dome",
            "Knurled dome",
            "A dark brown domed cap with a knurled grip round its skirt and a white tick at the edge — a vintage-style overdrive's knob.",
            &KNURLED_DOME,
            48.0,
            knurled_dome,
        ),
        knob(
            "knob.chevron",
            "Chevron cap",
            "A pastel cap printed with chevron stripes over a glossy black skirt, a black line for the index — a transparent-overdrive pedal's knob (green, pink, teal).",
            &CHEVRON,
            40.0,
            chevron,
        ),
        knob(
            "knob.ring_insert",
            "Ring insert",
            "A black cap set in stacked coral and peach rings with a dark index line — a studio reverb pedal's knob.",
            &RING_INSERT,
            44.0,
            ring_insert,
        ),
        knob(
            "knob.fluted_white",
            "Fluted white",
            "A big white scalloped knob with a bold black line — the single knob on a clean boost.",
            &FLUTED_WHITE,
            48.0,
            fluted_white,
        ),
        knob(
            "knob.cream_cap",
            "Cream cap",
            "A large flat-topped cream knob on a tan skirt with a small dark dot for the index — a tabletop reverb/tremolo unit.",
            &CREAM_CAP,
            64.0,
            cream_cap,
        ),
        knob(
            "knob.numbered_skirt",
            "Numbered skirt",
            "A black skirted knob with 1–10 printed round its own skirt and a chrome cap — a '60s American combo amp. The numerals turn past a printed mark.",
            &NUMBERED_SKIRT,
            40.0,
            numbered_skirt,
        ),
    ]
}
