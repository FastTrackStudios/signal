//! What the suite's units share: the satin pedal enclosure (every pedal is
//! the Standard class, 156 × 286 design px; the big-box delay 404 × 286),
//! the jack glyphs printed along a pedal's top edge, lower-case geometric
//! lettering, knobs placed by their shaft, and the catalog entries.

use frame::document::proto::{CornerRadii, NodeId, NodeKind, TextAlign};
use frame::surface::kit::materials::paint::luma;
use frame::surface::kit::materials::texture::{self, Relief};
use frame::surface::kit::{Place, TOUCH_MIN, draw};
use frame::surface::proto::{ParamInfo, Range, Unit};
use frame::surface::widgets::Builder;

use crate::reverb::ui::{self, blend};

/// The Standard pedal class (1590B-sized), design px at 60 px/in.
pub const STD_W: f64 = 156.0;
/// Every pedal's height (the Standard class's depth).
pub const PEDAL_H: f64 = 286.0;
/// The BigBox class (double-wide floor unit).
pub const BIG_W: f64 = 404.0;
/// The enclosure's bottom lip below the lid.
pub const LIP: f64 = 9.0;

/// A satin-anodised enclosure `w × h` filling `root`, painted top → bottom
/// in `paint`, with its lighter edge `rim`: the lid, and under it the
/// darker lip of the box's front side. Returns the lid (origin at the
/// root's).
pub fn pedal(
    b: &mut Builder<'_>,
    root: NodeId,
    w: f64,
    h: f64,
    paint: [&str; 3],
    rim: &str,
) -> NodeId {
    ui::fit_root(b, root, w, h);
    let mut side = draw::frame("Side", 0.0, LIP * 2.0, w, h - LIP * 2.0);
    side.fills = vec![draw::linear(
        180.0,
        &[
            (0.0, &blend(paint[2], "#000000", 0.45)),
            (1.0, &blend(paint[2], "#000000", 0.7)),
        ],
    )];
    side.corner_radii = CornerRadii::uniform(10.0);
    side.effects = vec![draw::drop_shadow("#00000099", 0.0, 4.0, 10.0)];
    b.add(root, side);
    let mut lid = draw::frame("Lid", 0.0, 0.0, w, h - LIP);
    lid.fills = vec![
        draw::linear(180.0, &[(0.0, paint[0]), (0.55, paint[1]), (1.0, paint[2])]),
        draw::linear(
            100.0,
            &[
                (0.0, "#ffffff00"),
                (0.3, "#ffffff10"),
                (0.55, "#ffffff00"),
                (1.0, "#0000000c"),
            ],
        ),
    ];
    lid.corner_radii = CornerRadii::uniform(9.0);
    lid.effects = vec![
        draw::inner_shadow(rim, 0.0, 1.2, 0.6),
        draw::inner_shadow("#00000040", 0.0, -1.5, 2.0),
        draw::drop_shadow("#00000070", 0.0, 1.5, 1.5),
    ];
    let lid = b.add(root, lid);
    let edge = draw::stroked(
        draw::rounded(draw::frame("edge", 0.5, 0.5, w - 1.0, h - LIP - 1.0), 8.5),
        rim,
        1.0,
    );
    b.add(lid, edge);
    lid
}

/// The jack glyphs along a pedal's top: ▲ input, (`plug`) the power plug, ▼ output.
pub fn jacks(
    b: &mut Builder<'_>,
    lid: NodeId,
    cx: f64,
    y: f64,
    spread: f64,
    ink: &str,
    plug: bool,
) {
    let s = 3.2;
    let up = draw::polygon(&[
        (cx - spread - s, y + s * 0.8),
        (cx - spread + s, y + s * 0.8),
        (cx - spread, y - s * 0.8),
    ]);
    let down = draw::polygon(&[
        (cx + spread - s, y - s * 0.8),
        (cx + spread + s, y - s * 0.8),
        (cx + spread, y + s * 0.8),
    ]);
    b.add(
        lid,
        draw::path(
            "jacks",
            0.0,
            0.0,
            2.0 * cx,
            2.0 * y,
            format!("{up} {down}"),
            draw::solid(ink),
        ),
    );
    if !plug {
        return;
    }
    ui::line(b, lid, "plug", &[(cx - 9.0, y), (cx + 9.0, y)], 0.8, ink);
    for dx in [-7.0, 0.0, 7.0] {
        let ring = draw::circle("plug pin", cx + dx, y, 1.8, draw::solid("#00000000"));
        b.add(lid, draw::stroked(ring, ink, 0.8));
    }
}

/// A covering `(x, y, w, h)` rounded `r` in `color` with a relief texture
/// of strength `gain` (`scale` its size), lit evenly from the front — the
/// plugin's renders light the heads and cabs flat, so no side sheen; the
/// top a touch brighter. Returns the clipped frame.
#[allow(clippy::too_many_arguments)]
pub fn covering(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    r: f64,
    color: &str,
    relief: Relief,
    gain: f64,
    scale: f64,
) -> NodeId {
    let mut f = draw::rounded(draw::frame("covering", x, y, w, h), r);
    f.clips_content = true;
    f.fills = vec![draw::linear(
        180.0,
        &[
            (0.0, &blend(color, "#ffffff", 0.08)),
            (0.5, color),
            (1.0, &blend(color, "#000000", 0.08)),
        ],
    )];
    let f = b.add(parent, f);
    texture::relief(b, f, (0.0, 0.0, w, h), 0.0, relief, gain, scale);
    // The grain as vectors too (a markup page carries no textures): a fine
    // speckle of lit and shaded tooth.
    let mut seed = 0x9e37_79b9_u64;
    let mut next = || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 11) as f64) / ((1_u64 << 53) as f64)
    };
    let (mut lit, mut dark) = (String::new(), String::new());
    let n = ((w * h) / 90.0).min(6000.0) as usize;
    for _ in 0..n {
        let (px, py, s) = (next() * w, next() * h, 0.6 + next() * 0.9);
        let d = format!("M {px:.1} {py:.1} h {s:.1} v {s:.1} h -{s:.1} Z ");
        if next() < 0.5 {
            lit.push_str(&d);
        } else {
            dark.push_str(&d);
        }
    }
    // Light coverings show the shaded tooth, dark ones barely the lit.
    let l = luma(color);
    let (lit_a, dark_a) = if l > 0.6 {
        (0.1, 0.08)
    } else if l < 0.3 {
        (0.045, 0.2)
    } else {
        (0.16, 0.14)
    };
    b.add(
        f,
        draw::path(
            "grain lit",
            0.0,
            0.0,
            w,
            h,
            lit,
            draw::solid(&ui::alpha("#ffffff", lit_a)),
        ),
    );
    b.add(
        f,
        draw::path(
            "grain dark",
            0.0,
            0.0,
            w,
            h,
            dark,
            draw::solid(&ui::alpha("#000000", dark_a)),
        ),
    );
    f
}

/// The suite's two hands: the pedals' and logos' wide geometric
/// lower-case, and the silkscreen and screen type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    /// Wide geometric sans (Saira, extended): pedal names and legends.
    Geo,
    /// The UI sans (Inter): amp silkscreen, screen labels, readouts.
    Ui,
}

impl Hand {
    #[must_use]
    pub fn lettering(self, weight: f64) -> draw::Lettering {
        match self {
            Self::Geo => draw::Lettering::family(draw::family::SAIRA)
                .weight(weight)
                .width(118.0),
            Self::Ui => draw::Lettering::family(draw::family::INTER).weight(weight),
        }
    }
}

/// Lettering in `hand`, centred on (`cx`, `cy`), tracked `track` em apart.
#[allow(clippy::too_many_arguments)]
pub fn set(
    b: &mut Builder<'_>,
    parent: NodeId,
    hand: Hand,
    s: &str,
    cx: f64,
    cy: f64,
    size: f64,
    color: &str,
    weight: f64,
    track: f64,
) -> NodeId {
    let w = s.chars().count() as f64 * size * (0.8 + track) + size * 2.0;
    let mut t = draw::lettered(
        ui::text(
            "word",
            s,
            cx - w / 2.0,
            cy,
            w,
            size,
            color,
            TextAlign::Center,
            weight,
            false,
        ),
        &hand.lettering(weight),
    );
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = size * track;
    }
    b.add(parent, t)
}

/// The pedals' wide geometric lower-case, centred on (`cx`, `cy`).
#[allow(clippy::too_many_arguments)]
pub fn word(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    cx: f64,
    cy: f64,
    size: f64,
    color: &str,
    weight: f64,
    track: f64,
) -> NodeId {
    set(b, parent, Hand::Geo, s, cx, cy, size, color, weight, track)
}

/// Silkscreen and screen type (the UI sans), centred on (`cx`, `cy`).
#[allow(clippy::too_many_arguments)]
pub fn print(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    cx: f64,
    cy: f64,
    size: f64,
    color: &str,
    weight: f64,
    track: f64,
) -> NodeId {
    set(b, parent, Hand::Ui, s, cx, cy, size, color, weight, track)
}

/// A knob's look: the component that draws it, its variant (the cap) and
/// its options. Every knob in the suite is placed through [`knob`] with one
/// of these, so a new knob renderer is a change here and nowhere else.
#[derive(Clone, Copy)]
pub struct Cap {
    pub id: &'static str,
    pub variant: &'static str,
    pub opts: &'static [&'static str],
    /// How far (a fraction of the knob's size) the shaft sits below the
    /// point asked for: a tall cap seen from the front stands up off its
    /// shaft, so its top lands where a flat knob's face would.
    pub drop: f64,
}

impl Cap {
    /// A GPU-rendered knob (`knob.3d`, `knob.render` without the GPU): a
    /// preset and its options.
    const fn gpu(preset: &'static str, opts: &'static [&'static str]) -> Self {
        Self {
            id: "knob.3d",
            variant: preset,
            opts,
            drop: 0.0,
        }
    }

    const fn drop(mut self, f: f64) -> Self {
        self.drop = f;
        self
    }
}

/// Gold knurled cylinders (the boost and compressor): polished gold, a
/// tall diamond-knurled side, a dark dot.
pub const GOLD: Cap = Cap::gpu(
    "gold_knurl",
    &[
        "color=#d9ae6a",
        "pointer=#2a1d0c",
        "ticks=none",
        "height=0.74",
        "tilt=36",
        "ribs=44",
        "index_w=0.09",
    ],
)
.drop(0.24);
/// Rose-gold knurled cylinders (the overdrive).
pub const ROSE: Cap = Cap::gpu(
    "gold_knurl",
    &[
        "color=#e4ab96",
        "pointer=#2a1d0c",
        "ticks=none",
        "height=0.74",
        "tilt=36",
        "ribs=44",
        "index_w=0.09",
    ],
)
.drop(0.24);
/// A big pearl-white cap, knurled round its foot (the chorus).
pub const PEARL: Cap = Cap::gpu(
    "white_knurl",
    &[
        "color=#e6e5e0",
        "pointer=#2f3432",
        "ticks=none",
        "diamond=0",
        "knurl_band=0.3",
        "ribs=70",
        "dome=0.06",
        "height=0.7",
        "tilt=34",
    ],
)
.drop(0.08);
/// Magenta anodised knurled caps (the reverb).
pub const MAGENTA: Cap = Cap::gpu(
    "gold_knurl",
    &[
        "color=#b0305f",
        "pointer=#1c0a12",
        "ticks=none",
        "rough=0.26",
        "height=0.74",
        "tilt=36",
        "ribs=44",
        "index_w=0.09",
    ],
)
.drop(0.24);
/// Black knurled caps with glossy domed tops, a white dot (the delay).
pub const BLACK: Cap = Cap::gpu(
    "white_knurl",
    &[
        "color=#1e1e20",
        "pointer=#f2f2f2",
        "ticks=none",
        "diamond=0",
        "ribs=44",
        "rough=0.2",
        "coat=0.7",
        "dome=0.16",
        "knurl_band=0.62",
        "height=0.8",
        "tilt=36",
        "index_w=0.1",
        "index_in=0.56",
        "index_out=0.56",
    ],
)
.drop(0.04);
/// The lead head: pink spun-metal caps in black skirts, a dark dot, a
/// printed tick ring.
pub const LEAD: Cap = Cap::gpu(
    "spun_metal",
    &[
        "color=#e7aebf",
        "pointer=#2a1219",
        "skirt_color=#1b1a1b",
        "index=dot",
        "index_w=0.07",
        "index_in=0.62",
        "index_out=0.62",
        "ticks=lines",
        "material=chrome",
    ],
);
/// The rhythm head: soft white cylinders on white skirts, a printed tick ring.
pub const RHYTHM: Cap = Cap::gpu(
    "flat",
    &[
        "color=#efefed",
        "pointer=#cfcfcc",
        "skirt_color=#e2e2e0",
        "rough=0.7",
        "coat=0",
        "spec=0.5",
        "index=dot",
        "index_in=0.6",
        "index_out=0.6",
        "ticks=lines",
    ],
);
/// The acoustic head: black knurled caps, a spun top, a white dot, a
/// printed tick ring.
pub const ACOUSTIC: Cap = Cap::gpu(
    "knurled",
    &[
        "color=#1f1f21",
        "pointer=#eeeeee",
        "index=dot",
        "index_w=0.08",
        "index_in=0.6",
        "index_out=0.6",
        "cap_r=0.86",
        "height=0.55",
        "knurl_band=0.55",
        "ribs=64",
        "aniso=0.7",
        "ticks=lines",
    ],
);
/// The EQ: big pink caps with a milled foot, a dark dot.
pub const EQ_PINK: Cap = Cap::gpu(
    "white_knurl",
    &[
        "color=#d98f9e",
        "pointer=#3a1f28",
        "ticks=none",
        "diamond=0",
        "knurl_band=1",
        "rough=0.6",
        "dome=0.02",
        "index_in=0.6",
        "index_out=0.6",
        "index_w=0.09",
        "tilt=26",
        "height=0.5",
        "ribs=80",
    ],
);
/// The white EQ's caps.
pub const EQ_WHITE: Cap = Cap::gpu(
    "white_knurl",
    &[
        "color=#eeeeec",
        "pointer=#2a2a2c",
        "ticks=none",
        "diamond=0",
        "knurl_band=1",
        "rough=0.6",
        "dome=0.02",
        "index_in=0.6",
        "index_out=0.6",
        "index_w=0.09",
        "tilt=26",
        "height=0.5",
        "ribs=80",
    ],
);
/// The black EQ's caps.
pub const EQ_BLACK: Cap = Cap::gpu(
    "white_knurl",
    &[
        "color=#202022",
        "pointer=#eeeeee",
        "ticks=none",
        "diamond=0",
        "knurl_band=1",
        "rough=0.45",
        "dome=0.02",
        "index_in=0.6",
        "index_out=0.6",
        "index_w=0.09",
        "tilt=26",
        "height=0.5",
        "ribs=80",
    ],
);
/// The cab page's plugin-style flat arc knobs.
pub const SCREEN: Cap = Cap {
    id: "knob.arc_disc",
    variant: "dark",
    opts: &[],
    drop: 0.0,
};

/// A knob in `cap`'s look with its shaft at (`cx`, `cy`), a tight touch
/// box, no printed legend (the unit letters its own); ticks in `ink`.
#[allow(clippy::too_many_arguments)]
pub fn knob(
    b: &mut Builder<'_>,
    parent: NodeId,
    cap: &Cap,
    addr: &str,
    name: &str,
    d: f64,
    cx: f64,
    cy: f64,
    ink: &str,
) -> NodeId {
    let p = Place::new(name)
        .size(d)
        .variant(cap.variant)
        .bind(ui::bind(addr))
        .legend("")
        .touch(TOUCH_MIN)
        .options(cap.opts)
        .ink(frame::surface::kit::Ink::Color(ink.into()));
    let k = ui::put(b, parent, cap.id, &p, cx, cy + cap.drop * d);
    ui::strip_legend(b, k);
    k
}

/// A switch component (footswitch, lever) centred on its child named one
/// of `anchor` at (`cx`, `cy`).
#[allow(clippy::too_many_arguments)]
pub fn switch(
    b: &mut Builder<'_>,
    parent: NodeId,
    id: &str,
    variant: &str,
    addr: &str,
    name: &str,
    d: f64,
    cx: f64,
    cy: f64,
    opts: &[&str],
    anchor: &[&str],
) -> NodeId {
    let mut p = Place::new(name).size(d).bind(ui::bind(addr)).options(opts);
    if !variant.is_empty() {
        p = p.variant(variant);
    }
    ui::put_by(b, parent, id, &p, anchor, cx, cy)
}

/// The printed high-pass / low-pass glyph (a knee), `w` wide at (`cx`, `cy`).
pub fn cut_glyph(
    b: &mut Builder<'_>,
    parent: NodeId,
    cx: f64,
    cy: f64,
    w: f64,
    high_pass: bool,
    ink: &str,
) {
    let h = w * 0.45;
    let pts = if high_pass {
        vec![
            (cx - w / 2.0, cy + h / 2.0),
            (cx - w * 0.1, cy - h / 2.0),
            (cx + w / 2.0, cy - h / 2.0),
        ]
    } else {
        vec![
            (cx - w / 2.0, cy - h / 2.0),
            (cx + w * 0.1, cy - h / 2.0),
            (cx + w / 2.0, cy + h / 2.0),
        ]
    };
    ui::line(b, parent, "cut glyph", &pts, 1.3, ink);
}

// ── catalog ──────────────────────────────────────────────────────────────

pub fn pct(ns: &str, id: &str, name: &str, default: f64) -> ParamInfo {
    ui::percent(&format!("{ns}/{id}"), name, default)
}

pub fn db(ns: &str, id: &str, name: &str, lo: f64, hi: f64, default: f64) -> ParamInfo {
    ui::param(
        &format!("{ns}/{id}"),
        name,
        Range::linear(lo, hi, Unit::Db),
        default,
    )
}

/// An amp's 0–10 dial.
pub fn dial10(ns: &str, id: &str, name: &str, default: f64) -> ParamInfo {
    ui::param(
        &format!("{ns}/{id}"),
        name,
        Range::linear(0.0, 10.0, Unit::None),
        default,
    )
}

pub fn hz(ns: &str, id: &str, name: &str, lo: f64, hi: f64, default: f64) -> ParamInfo {
    ui::param(
        &format!("{ns}/{id}"),
        name,
        Range::log(lo, hi, Unit::Hz),
        default,
    )
}

pub fn on(ns: &str, id: &str, name: &str, default: bool) -> ParamInfo {
    ui::toggle(&format!("{ns}/{id}"), name, default)
}

pub fn pick(ns: &str, id: &str, name: &str, options: &[&str], default: usize) -> ParamInfo {
    ui::choice(&format!("{ns}/{id}"), name, options, default)
}
