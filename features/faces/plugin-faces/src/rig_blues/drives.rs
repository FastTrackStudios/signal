//! The blues rig's drives: recreations of the four real pedals the suite's
//! drives are modelled on (docs/rig-sections.md) — their enclosures,
//! colours, knobs and lettering, under names of our own.
//!
//! - `blade` — the dual-FET clean boost: gloss black, a white hairline
//!   round the lid, its volume knob standing out of the right side.
//!   172 × 286.
//! - `pegasus` — the gold overdrive: three oxblood domes, serif capitals,
//!   landscape. 400 × 286.
//! - `british-blues` — the 1991 British overdrive: black crinkle, a ledge
//!   carrying three knobs with blue lines, the blue logo between rules, a
//!   script embossed in the lid. 222 × 286.
//! - `tube-drive` — the green overdrive whose switch is a ribbed rubber
//!   treadle. 172 × 286.

use document_proto::motion::{Dynamics, Motion, Num, Target, Wave};
use document_proto::{CornerRadii, NodeId};
use surface_kit::Place;
use surface_kit::draw::{self, Lettering, family};
use surface_kit::motion;
use surface_proto::{Binding, ButtonMode, Control, ControlKind, Range, SlotRole, Unit};
use surface_widgets::Builder;

use crate::comp_tube::ui as place;
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::suite_studio_blues::parts::{self, set};

use super::PLUGIN;

/// Every pedal's height (docs/form-factors.md).
pub const H: f64 = 286.0;

/// The rendered enclosure, `w` wide, in `color` and `finish` (plus
/// `extra` options), sized into `root`. Returns its lid (the parent for
/// controls and print) and the lid's size.
pub(super) fn enclosure(
    b: &mut Builder<'_>,
    root: NodeId,
    w: f64,
    color: &str,
    finish: &str,
    extra: &[&str],
) -> (NodeId, f64, f64) {
    ui::fit_root(b, root, w, H);
    let mut opts = vec![
        format!("w={w}"),
        format!("h={H}"),
        format!("color={color}"),
        format!("finish={finish}"),
    ];
    opts.extend(extra.iter().map(ToString::to_string));
    let p = Place {
        options: Some(opts),
        ..Place::new("Enclosure").variant("standard")
    };
    let enc = ui::put_at(b, root, "panel.pedal_3d", &p, 0.0, 0.0);
    let lid = b
        .scene
        .node(enc)
        .and_then(|n| {
            n.children
                .iter()
                .copied()
                .find(|c| b.scene.node(*c).is_some_and(|c| c.name == "lid"))
        })
        .expect("pedal_3d has a lid");
    let (lw, lh) = b
        .scene
        .node(lid)
        .map_or((w, H), |n| (n.size.width, n.size.height));
    (lid, lw, lh)
}

/// A rendered knob: preset `variant` with `options` over it, `d` across,
/// its shaft at (`cx`, `cy`), bound to `addr`. The face letters its own.
#[allow(clippy::too_many_arguments)]
pub(super) fn knob(
    b: &mut Builder<'_>,
    parent: NodeId,
    variant: &str,
    options: &[&str],
    addr: &str,
    name: &str,
    d: f64,
    cx: f64,
    cy: f64,
) -> NodeId {
    let mut opts = vec!["touch=44", "ticks=none"];
    opts.extend_from_slice(options);
    let p = Place::new(name)
        .legend(" ")
        .size(d)
        .variant(variant)
        .options(&opts)
        .bind(ui::bind(addr));
    place::centred_at(b, parent, "knob.3d", &p, cx, cy)
}

pub(super) fn inter(weight: f64) -> Lettering {
    Lettering::family(family::INTER).weight(weight)
}

/// The capture's drive — the rig block's `drive`, 0..1.
fn drive(ns: &str, name: &str) -> surface_proto::ParamInfo {
    ui::param(
        &format!("{ns}/drive"),
        name,
        Range::linear(0.0, 1.0, Unit::None),
        0.5,
    )
    .featured()
}

/// The pedal's tone — the drive block's own EQ (to come), 0..1.
fn tone(ns: &str, name: &str) -> surface_proto::ParamInfo {
    ui::param(
        &format!("{ns}/tone"),
        name,
        Range::linear(0.0, 1.0, Unit::None),
        0.5,
    )
}

/// The output — the rig block's output level, dB.
pub fn level(ns: &str) -> surface_proto::ParamInfo {
    ui::param(
        &format!("{ns}/level"),
        "Output",
        Range::linear(-24.0, 12.0, Unit::Db),
        0.0,
    )
    .featured()
}

pub(super) fn on(ns: &str) -> surface_proto::ParamInfo {
    ui::toggle(&format!("{ns}/on"), "Bypass", true).featured()
}

/// The footswitch alone (its LED is where the pedal puts it: [`led`]),
/// `d` across the nut, at (`cx`, `cy`).
pub(super) fn stomp(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    d: f64,
    cx: f64,
    cy: f64,
) -> NodeId {
    let p = Place::new("Bypass")
        .size(d)
        .variant("none")
        .legend("")
        .bind(Binding::param(format!("{ns}/on")))
        .options(&["touch=44"]);
    place::centred_at(b, parent, "switch.stomp", &p, cx, cy)
}

/// The status LED at (`cx`, `cy`), `d` across, lit while `ns/on` is.
pub(super) fn led(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    cx: f64,
    cy: f64,
    d: f64,
    color: &str,
) -> NodeId {
    let r = d / 2.0;
    let f = b.add(parent, draw::frame("LED", cx - d, cy - d, 2.0 * d, 2.0 * d));
    b.add(
        f,
        draw::with_effects(
            draw::circle(
                "bezel",
                d,
                d,
                r + (d * 0.16).max(1.2),
                draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#060607")]),
            ),
            vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)],
        ),
    );
    let off = b.add(
        f,
        draw::circle(
            "lens",
            d,
            d,
            r,
            draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#ffffff30"), (1.0, "#000000d0")]),
        ),
    );
    let lit = draw::circle(
        "lit",
        d,
        d,
        r,
        draw::radial_at(
            0.4,
            0.35,
            0.9,
            &[(0.0, "#ffffff"), (0.35, color), (1.0, color)],
        ),
    );
    let lit = b.add(f, draw::emissive(lit, draw::hex(color), 0.9, d * 2.5));
    let mut c = Control::new(ControlKind::Readout)
        .slot(SlotRole::Off, off.0)
        .slot(SlotRole::On, lit.0);
    c.binding = Some(Binding::param(format!("{ns}/on")));
    b.controls.insert(f, c);
    f
}

/// The dual-FET clean boost. Gloss black, a white hairline round the lid,
/// the blue LED at its head. The real one's volume knob stands out of its
/// side; here it is on the lid, big and centred, where a finger finds it.
#[must_use]
pub fn blade() -> Face {
    blade_at(crate::rig_time::kit::Tier::Desktop)
}

/// At `tier`: the same pedal, narrower where four share a phone's width
/// (its knobs sized to the box).
#[must_use]
pub fn blade_at(tier: crate::rig_time::kit::Tier) -> Face {
    let small = matches!(
        tier,
        crate::rig_time::kit::Tier::Phone | crate::rig_time::kit::Tier::Strip
    );
    #[allow(non_snake_case)]
    let W: f64 = if small { 165.0 } else { 172.0 };
    const NS: &str = "katana";
    const INK: &str = "#e9e9ec";
    let scene = canvas("Katana", |b, root| {
        let (lid, lw, lh) = enclosure(
            b,
            root,
            W,
            "#111113",
            "gloss",
            &["jack_inset=10", "radius=14"],
        );
        b.add(
            lid,
            draw::stroked(
                draw::rounded(
                    draw::rect(
                        "border",
                        6.0,
                        16.0,
                        lw - 12.0,
                        lh - 26.0,
                        draw::solid("#00000000"),
                    ),
                    9.0,
                ),
                INK,
                1.4,
            ),
        );
        led(b, lid, NS, lw * 0.5, 9.0, 5.0, "#3d6bff");
        knob(
            b,
            lid,
            "knurled",
            &[
                "color=#161618",
                "ribs=44",
                "index=line",
                "pointer=#f2f2f4",
                "index_w=0.08",
                "skirt=none",
            ],
            &format!("{NS}/drive"),
            "Volume",
            if small { 78.0 } else { 86.0 },
            lw * 0.5,
            lh * 0.34,
        );
        parts::print(
            b,
            lid,
            "VOLUME",
            lw * 0.5,
            lh * 0.34 + 52.0,
            9.0,
            INK,
            650.0,
            0.14,
        );
        set(
            b,
            lid,
            "KATANA",
            lw * 0.5,
            lh * 0.64,
            24.0,
            "#f4f4f6",
            &inter(500.0).width(92.0),
            0.04,
        );
        stomp(b, lid, NS, 34.0, lw * 0.5, lh * 0.79);
        let (mx, my, ms) = (lw * 0.5 - 11.0, lh * 0.88, 20.0);
        b.add(
            lid,
            draw::stroked(
                draw::rounded(
                    draw::rect("mark", mx, my, ms, ms, draw::solid("#00000000")),
                    3.0,
                ),
                INK,
                1.4,
            ),
        );
        set(
            b,
            lid,
            "K",
            lw * 0.5,
            my + ms * 0.52,
            14.0,
            INK,
            &inter(800.0).italic(),
            0.0,
        );
    });
    let params = vec![drive(NS, "Volume"), level(NS), on(NS)];
    Face {
        file: crate::rig_time::kit::tier_file("11-drive-blade", tier, false),
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The gold overdrive. Landscape, gold with the paint worn soft, three
/// oxblood domes with pointers, spaced serif capitals under them, the LED
/// by the gain, the name split round a small chrome stomp.
#[must_use]
pub fn pegasus() -> Face {
    pegasus_at(crate::rig_time::kit::Tier::Desktop)
}

/// At `tier`: on a phone the gold box stands upright — Gain big on top,
/// Treble and Output under it, the centaur, the stomp, the name across the
/// foot — the same paint, knobs and lettering.
#[must_use]
pub fn pegasus_at(tier: crate::rig_time::kit::Tier) -> Face {
    let small = matches!(
        tier,
        crate::rig_time::kit::Tier::Phone | crate::rig_time::kit::Tier::Strip
    );
    #[allow(non_snake_case)]
    let W: f64 = if small { 165.0 } else { 400.0 };
    const NS: &str = "klon";
    const INK: &str = "#2b1c08";
    let serif = Lettering::family(family::PLAYFAIR_DISPLAY).weight(700.0);
    let scene = canvas("Klon Centaur", |b, root| {
        let (lid, lw, lh) = enclosure(b, root, W, "#c99038", "satin", &["jack_inset=8"]);
        b.add(
            lid,
            draw::rect(
                "sheen",
                0.0,
                0.0,
                lw,
                lh,
                draw::linear(
                    115.0,
                    &[
                        (0.0, "#ffe2a02a"),
                        (0.4, "#ffffff00"),
                        (0.75, "#4a2c0620"),
                        (1.0, "#ffe2a014"),
                    ],
                ),
            ),
        );
        let dome = [
            "color=#3a0f0c",
            "pointer=#e8d2b0",
            "index=line",
            "index_w=0.07",
            "skirt=plain",
            "skirt_color=#2a0a08",
            "dome=0.35",
            "coat=0.8",
        ];
        if small {
            knob(
                b,
                lid,
                "domed",
                &dome,
                &format!("{NS}/drive"),
                "GAIN",
                56.0,
                lw * 0.5,
                lh * 0.19,
            );
            set(
                b,
                lid,
                "GAIN",
                lw * 0.5,
                lh * 0.19 + 38.0,
                11.0,
                INK,
                &serif,
                0.12,
            );
            for (i, (id, name)) in [("tone", "TREBLE"), ("level", "OUTPUT")].iter().enumerate() {
                let cx = lw * (0.27 + 0.46 * i as f64);
                knob(
                    b,
                    lid,
                    "domed",
                    &dome,
                    &format!("{NS}/{id}"),
                    name,
                    44.0,
                    cx,
                    lh * 0.46,
                );
                set(b, lid, name, cx, lh * 0.46 + 32.0, 10.0, INK, &serif, 0.1);
            }
            led(b, lid, NS, lw * 0.14, lh * 0.08, 5.0, "#d0261a");
            centaur(b, lid, (lw * 0.5, lh * 0.68), 42.0, INK);
            stomp(b, lid, NS, 36.0, lw * 0.5, lh * 0.83);
            set(
                b,
                lid,
                "KLON CENTAUR",
                lw * 0.5,
                lh * 0.94,
                11.0,
                INK,
                &serif,
                0.14,
            );
            return;
        }
        for (i, (id, name)) in [("drive", "GAIN"), ("tone", "TREBLE"), ("level", "OUTPUT")]
            .iter()
            .enumerate()
        {
            let cx = lw * (0.2 + 0.3 * i as f64);
            knob(
                b,
                lid,
                "domed",
                &[
                    "color=#3a0f0c",
                    "pointer=#e8d2b0",
                    "index=line",
                    "index_w=0.07",
                    "skirt=plain",
                    "skirt_color=#2a0a08",
                    "dome=0.35",
                    "coat=0.8",
                ],
                &format!("{NS}/{id}"),
                name,
                84.0,
                cx,
                lh * 0.3,
            );
            set(b, lid, name, cx, lh * 0.3 + 58.0, 15.0, INK, &serif, 0.12);
        }
        led(b, lid, NS, lw * 0.07, lh * 0.44, 6.0, "#d0261a");
        centaur(b, lid, (lw * 0.36, lh * 0.78), 76.0, INK);
        stomp(b, lid, NS, 34.0, lw * 0.64, lh * 0.8);
        set(b, lid, "KLON", lw * 0.16, lh * 0.8, 15.0, INK, &serif, 0.16);
        set(
            b,
            lid,
            "CENTAUR",
            lw * 0.84,
            lh * 0.8,
            15.0,
            INK,
            &serif,
            0.16,
        );
    });
    let params = vec![drive(NS, "Gain"), tone(NS, "Treble"), level(NS), on(NS)];
    Face {
        file: crate::rig_time::kit::tier_file("12-drive-pegasus", tier, false),
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The 1991 British overdrive. Black crinkle; the knobs stand on a raised
/// ledge across the top, black with blue lines; the logo in blue italic
/// between rules; a red LED over the stomp; the maker's script embossed
/// low in the lid.
#[must_use]
pub fn british_blues() -> Face {
    british_blues_at(crate::rig_time::kit::Tier::Desktop)
}

/// At `tier`: the same pedal, narrower where four share a phone's width
/// (its knobs sized to the box).
#[must_use]
pub fn british_blues_at(tier: crate::rig_time::kit::Tier) -> Face {
    let small = matches!(
        tier,
        crate::rig_time::kit::Tier::Phone | crate::rig_time::kit::Tier::Strip
    );
    #[allow(non_snake_case)]
    let W: f64 = if small { 165.0 } else { 240.0 };
    const NS: &str = "bluesbreaker";
    const BLUE: &str = "#4fb8ea";
    const BODY: &str = "#2a2c30";
    let scene = canvas("Blues Breaker", |b, root| {
        // Smooth matte charcoal, as the original's paint is.
        let (lid, lw, lh) = enclosure(b, root, W, BODY, "satin", &["jack_inset=10"]);
        // The raised back panel the knobs stand on: the same paint, a lit
        // edge along its front.
        let ledge_h = lh * 0.4;
        b.add(
            lid,
            draw::rect(
                "ledge",
                0.0,
                0.0,
                lw,
                ledge_h,
                draw::linear(180.0, &[(0.0, "#34363b"), (1.0, "#2c2e33")]),
            ),
        );
        b.add(
            lid,
            draw::rect(
                "ledge edge",
                0.0,
                ledge_h - 2.0,
                lw,
                2.0,
                draw::linear(180.0, &[(0.0, "#4a4d53"), (1.0, "#16171a")]),
            ),
        );
        b.add(
            lid,
            draw::with_effects(
                draw::rect(
                    "ledge shadow",
                    0.0,
                    ledge_h,
                    lw,
                    5.0,
                    draw::linear(180.0, &[(0.0, "#00000070"), (1.0, "#00000000")]),
                ),
                vec![],
            ),
        );
        for (i, (id, name)) in [("drive", "Gain"), ("tone", "Tone"), ("level", "Volume")]
            .iter()
            .enumerate()
        {
            let cx = lw * (0.18 + 0.32 * i as f64);
            let kd = if small { 44.0 } else { 56.0 };
            knob(
                b,
                lid,
                "knurled",
                &[
                    "color=#141416",
                    &format!("pointer={BLUE}"),
                    "index=line",
                    "index_w=0.12",
                    "ribs=36",
                ],
                &format!("{NS}/{id}"),
                name,
                kd,
                cx,
                ledge_h * 0.5,
            );
            parts::print(
                b,
                lid,
                name,
                cx,
                ledge_h * 0.5 + kd * 0.5 + 10.0,
                if small { 9.5 } else { 9.0 },
                "#d9dadd",
                600.0,
                0.02,
            );
        }
        // The logo between two blue double rules.
        let logo = Lettering::family(family::OSWALD).weight(700.0).italic();
        let ly = lh * 0.58;
        for dy in [-15.0, -12.0, 12.0, 15.0] {
            b.add(
                lid,
                draw::rect("rule", 10.0, ly + dy, lw - 20.0, 1.3, draw::solid(BLUE)),
            );
        }
        set(
            b,
            lid,
            "BLUES BREAKER",
            lw * 0.5,
            ly,
            if small { 16.0 } else { 20.0 },
            BLUE,
            &logo,
            0.02,
        );
        led(b, lid, NS, lw * 0.5, lh * 0.7, 5.0, "#e0281c");
        stomp(b, lid, NS, 32.0, lw * 0.5, lh * 0.8);
    });
    let params = vec![drive(NS, "Gain"), tone(NS, "Tone"), level(NS), on(NS)];
    Face {
        file: crate::rig_time::kit::tier_file("13-drive-british-blues", tier, false),
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The green overdrive. Three black knobs with broad mint pointers, the
/// grey name band, and a black rubber treadle ribbed in chevrons — the
/// whole treadle is the switch.
#[must_use]
pub fn tube_drive() -> Face {
    tube_drive_at(crate::rig_time::kit::Tier::Desktop)
}

/// At `tier`: the same pedal, narrower where four share a phone's width
/// (its knobs sized to the box).
#[must_use]
pub fn tube_drive_at(tier: crate::rig_time::kit::Tier) -> Face {
    let small = matches!(
        tier,
        crate::rig_time::kit::Tier::Phone | crate::rig_time::kit::Tier::Strip
    );
    #[allow(non_snake_case)]
    let W: f64 = if small { 165.0 } else { 200.0 };
    const NS: &str = "tubescreamer";
    const GREEN: &str = "#57c23f";
    const DARK: &str = "#0d2a08";
    let scene = canvas("Tube Screamer", |b, root| {
        let (lid, lw, lh) = enclosure(
            b,
            root,
            W,
            GREEN,
            "gloss",
            &["jack_inset=6", "front=0", "radius=10"],
        );
        led(b, lid, NS, lw * 0.5, 8.0, 5.0, "#e01818");
        for (i, (id, name)) in [("drive", "DRIVE"), ("tone", "TONE"), ("level", "LEVEL")]
            .iter()
            .enumerate()
        {
            let cx = lw * (0.18 + 0.32 * i as f64);
            let cy = lh * 0.23;
            let kd = if small { 44.0 } else { 54.0 };
            knob(
                b,
                lid,
                "flat",
                &[
                    "color=#101012",
                    "pointer=#7ee8b4",
                    "index=full",
                    "index_w=0.42",
                    "index_in=0",
                    "index_out=0.95",
                    "skirt=plain",
                ],
                &format!("{NS}/{id}"),
                name,
                kd,
                cx,
                cy,
            );
            parts::print(
                b,
                lid,
                name,
                cx,
                cy + kd * 0.5 + 10.0,
                if small { 9.0 } else { 8.5 },
                DARK,
                750.0,
                0.08,
            );
        }
        // The name band.
        let (by, bh) = (lh * 0.41, lh * 0.1);
        b.add(
            lid,
            draw::rect(
                "band",
                0.0,
                by,
                lw,
                bh,
                draw::linear(180.0, &[(0.0, "#46494c"), (1.0, "#2a2c2f")]),
            ),
        );
        set(
            b,
            lid,
            "TUBE SCREAMER",
            lw * 0.5,
            by + bh * 0.5,
            if small { 12.5 } else { 15.0 },
            "#f1f1ef",
            &inter(700.0).width(88.0),
            0.02,
        );
        // The treadle: the whole of it is the switch.
        let (tx, ty) = (lw * 0.08, by + bh + 6.0);
        let (tw, th) = (lw * 0.84, lh - ty - 8.0);
        let mut tread = draw::frame("treadle", tx, ty, tw, th);
        tread.fills = vec![draw::linear(
            180.0,
            &[(0.0, "#2c2c2e"), (0.08, "#18181a"), (1.0, "#0c0c0d")],
        )];
        tread.corner_radii = CornerRadii::uniform(6.0);
        tread.clips_content = true;
        tread.effects = vec![draw::drop_shadow("#000000c0", 0.0, 3.0, 6.0)];
        let tread = b.add(lid, tread);
        let mut lit = Vec::new();
        let mut dark = Vec::new();
        for r in 0..8 {
            let y = th * 0.08 + f64::from(r) * th * 0.1;
            for k in 0..3 {
                let (x0, x1) = (
                    tw * (0.06 + 0.3 * f64::from(k)),
                    tw * (0.3 + 0.3 * f64::from(k)),
                );
                let xm = (x0 + x1) / 2.0;
                dark.push(vec![(x0, y + 5.0), (xm, y), (x1, y + 5.0)]);
                lit.push(vec![(x0, y + 6.4), (xm, y + 1.4), (x1, y + 6.4)]);
            }
        }
        b.add(
            tread,
            ui::strokes("rib shadow", &dark, 2.4, draw::solid("#050506")),
        );
        b.add(
            tread,
            ui::strokes("rib light", &lit, 1.0, draw::solid("#3a3a3e")),
        );
        let brand = Lettering::family(family::DANCING_SCRIPT)
            .weight(700.0)
            .italic();
        set(
            b,
            tread,
            "Ibanez",
            tw * 0.5,
            th * 0.9,
            17.0,
            "#303034",
            &brand,
            0.0,
        );
        b.controls.insert(
            tread,
            Control::new(ControlKind::Button {
                mode: ButtonMode::Toggle,
            })
            .bound(Binding::param(format!("{NS}/on"))),
        );
    });
    let params = vec![drive(NS, "Drive"), tone(NS, "Tone"), level(NS), on(NS)];
    Face {
        file: crate::rig_time::kit::tier_file("14-drive-tube-drive", tier, false),
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The centaur of the gold overdrive's lid, drawn in its ink: a horse's
/// body at a gallop, the archer's torso rising from its shoulders, the bow
/// drawn forward. `size` across, centred on (`cx`, `cy`).
pub(crate) fn centaur(
    b: &mut Builder<'_>,
    parent: NodeId,
    (cx, cy): (f64, f64),
    size: f64,
    ink: &str,
) {
    let k = size / 100.0;
    let (x0, y0) = (cx - 50.0 * k, cy - 50.0 * k);
    let p = |pts: &[(f64, f64)]| {
        draw::polygon(&pts.iter().map(|(x, y)| (x * k, y * k)).collect::<Vec<_>>())
    };
    let shapes = [
        // The body, barrel-deep, the chest high.
        p(&[
            (30.0, 50.0),
            (36.0, 43.0),
            (52.0, 41.0),
            (70.0, 42.0),
            (80.0, 47.0),
            (82.0, 56.0),
            (76.0, 64.0),
            (58.0, 67.0),
            (40.0, 66.0),
            (31.0, 60.0),
        ]),
        // Forelegs, one reaching, one tucked.
        p(&[
            (33.0, 62.0),
            (39.0, 63.0),
            (28.0, 78.0),
            (20.0, 82.0),
            (19.0, 79.0),
            (25.0, 76.0),
        ]),
        p(&[
            (41.0, 64.0),
            (46.0, 65.0),
            (46.0, 80.0),
            (43.0, 90.0),
            (39.0, 90.0),
            (41.0, 80.0),
        ]),
        // Hind legs, driving.
        p(&[
            (66.0, 62.0),
            (73.0, 61.0),
            (76.0, 76.0),
            (84.0, 88.0),
            (80.0, 90.0),
            (71.0, 78.0),
        ]),
        p(&[
            (72.0, 60.0),
            (79.0, 58.0),
            (88.0, 72.0),
            (92.0, 84.0),
            (88.0, 85.0),
            (82.0, 73.0),
        ]),
        // The tail, streaming.
        p(&[
            (80.0, 47.0),
            (90.0, 44.0),
            (98.0, 50.0),
            (94.0, 51.0),
            (88.0, 50.0),
            (82.0, 53.0),
        ]),
        // The torso and the arm holding the bow.
        p(&[
            (31.0, 50.0),
            (26.0, 36.0),
            (24.0, 22.0),
            (30.0, 17.0),
            (38.0, 19.0),
            (39.0, 32.0),
            (37.0, 46.0),
        ]),
        p(&[(26.0, 24.0), (12.0, 27.0), (12.0, 31.0), (26.0, 29.0)]),
        p(&[(37.0, 21.0), (48.0, 25.0), (47.0, 29.0), (36.0, 25.0)]),
        draw::regular(31.0 * k, 10.0 * k, 6.5 * k, 14, 0.0),
    ];
    let d = shapes.join(" ");
    b.add(
        parent,
        draw::with_opacity(
            draw::path("centaur", x0, y0, size, size, d, draw::solid(ink)),
            0.9,
        ),
    );
    // The bow and its string, drawn back to the archer's hand.
    let bow: Vec<(f64, f64)> = (0..=16)
        .map(|i| {
            let t = f64::from(i) / 16.0;
            let a = (-60.0 + 120.0 * t).to_radians();
            (
                x0 + (20.0 - 12.0 * a.cos()) * k,
                y0 + (29.0 + 19.0 * a.sin()) * k,
            )
        })
        .collect();
    let (top, bottom) = (bow[0], bow[16]);
    b.add(
        parent,
        ui::strokes(
            "bow",
            &[bow, vec![top, (x0 + 47.0 * k, y0 + 27.0 * k), bottom]],
            1.4,
            draw::solid(ink),
        ),
    );
}

/// King of Tone: the ivory box, its two sides' knobs (VOLUME, DRIVE,
/// TONE — the capture's drive and output, the block EQ's tone), the crown
/// and the name in black serif, a red LED and a yellow one.
#[must_use]
pub fn king_of_tone() -> Face {
    king_of_tone_at(crate::rig_time::kit::Tier::Desktop)
}

/// At `tier`: the same pedal, narrower where four share a phone's width
/// (its knobs sized to the box).
#[must_use]
pub fn king_of_tone_at(tier: crate::rig_time::kit::Tier) -> Face {
    let small = matches!(
        tier,
        crate::rig_time::kit::Tier::Phone | crate::rig_time::kit::Tier::Strip
    );
    #[allow(non_snake_case)]
    let W: f64 = if small { 165.0 } else { 240.0 };
    const NS: &str = "kingoftone";
    const INK: &str = "#1b1714";
    let serif = Lettering::family(family::PLAYFAIR_DISPLAY).weight(800.0);
    let scene = canvas("King of Tone", |b, root| {
        let (lid, lw, lh) = enclosure(b, root, W, "#ece3cf", "satin", &["jack_inset=10"]);
        led(b, lid, NS, lw * 0.3, 10.0, 5.0, "#e0281c");
        let other = b.add(
            lid,
            draw::with_effects(
                draw::circle("yellow led", lw * 0.7, 10.0, 2.5, draw::solid("#5a4a10")),
                vec![draw::drop_shadow("#00000060", 0.0, 1.0, 1.0)],
            ),
        );
        let _ = other;
        for (i, (id, name)) in [("level", "VOLUME"), ("drive", "DRIVE"), ("tone", "TONE")]
            .iter()
            .enumerate()
        {
            let cx = lw * (0.18 + 0.32 * i as f64);
            let cy = lh * 0.2;
            let kd = if small { 44.0 } else { 56.0 };
            knob(
                b,
                lid,
                "flat",
                &[
                    "color=#141414",
                    "pointer=#f4f0e6",
                    "index=line",
                    "index_w=0.1",
                    "skirt=plain",
                    "skirt_color=#0c0c0c",
                ],
                &format!("{NS}/{id}"),
                name,
                kd,
                cx,
                cy,
            );
            parts::print(
                b,
                lid,
                name,
                cx,
                cy + kd * 0.5 + 10.0,
                8.5,
                INK,
                750.0,
                0.08,
            );
        }
        // The crown over the name.
        let (cx, cy, k) = (lw * 0.5, lh * 0.45, 1.3);
        let crown = draw::polygon(&[
            (cx - 18.0 * k, cy + 8.0 * k),
            (cx - 20.0 * k, cy - 8.0 * k),
            (cx - 9.0 * k, cy),
            (cx, cy - 12.0 * k),
            (cx + 9.0 * k, cy),
            (cx + 20.0 * k, cy - 8.0 * k),
            (cx + 18.0 * k, cy + 8.0 * k),
        ]);
        b.add(
            lid,
            draw::path("crown", 0.0, 0.0, lw, lh, crown, draw::solid("#b8201a")),
        );
        set(
            b,
            lid,
            "KING OF TONE",
            lw * 0.5,
            lh * 0.56,
            if small { 15.0 } else { 19.0 },
            INK,
            &serif,
            0.04,
        );
        stomp(b, lid, NS, 34.0, lw * 0.5, lh * 0.76);
        parts::print(
            b,
            lid,
            "ANALOGMAN",
            lw * 0.5,
            lh * 0.92,
            8.0,
            INK,
            800.0,
            0.2,
        );
    });
    let params = vec![drive(NS, "Drive"), tone(NS, "Tone"), level(NS), on(NS)];
    Face {
        file: crate::rig_time::kit::tier_file("15-drive-king-of-tone", tier, false),
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// JHS Morning Glory: the blue box, VOLUME, DRIVE and TONE in black, the
/// gain switch choosing the capture (Low, Medium, High Gain), the name in
/// white script.
#[must_use]
pub fn morning_glory() -> Face {
    morning_glory_at(crate::rig_time::kit::Tier::Desktop)
}

/// At `tier`: the same pedal, narrower where four share a phone's width
/// (its knobs sized to the box).
#[must_use]
pub fn morning_glory_at(tier: crate::rig_time::kit::Tier) -> Face {
    let small = matches!(
        tier,
        crate::rig_time::kit::Tier::Phone | crate::rig_time::kit::Tier::Strip
    );
    #[allow(non_snake_case)]
    let W: f64 = if small { 165.0 } else { 200.0 };
    const NS: &str = "morningglory";
    const INK: &str = "#f4f6fa";
    let scene = canvas("Morning Glory", |b, root| {
        let (lid, lw, lh) = enclosure(b, root, W, "#3f7fc4", "gloss", &["jack_inset=8"]);
        led(b, lid, NS, lw * 0.5, 10.0, 5.0, "#e0281c");
        for (i, (id, name)) in [("level", "VOLUME"), ("drive", "DRIVE"), ("tone", "TONE")]
            .iter()
            .enumerate()
        {
            let cx = lw * (0.19 + 0.31 * i as f64);
            let cy = lh * 0.21;
            let kd = if small { 44.0 } else { 52.0 };
            knob(
                b,
                lid,
                "flat",
                &[
                    "color=#131315",
                    "pointer=#f4f6fa",
                    "index=line",
                    "index_w=0.1",
                    "skirt=plain",
                    "skirt_color=#0b0b0c",
                ],
                &format!("{NS}/{id}"),
                name,
                kd,
                cx,
                cy,
            );
            parts::print(
                b,
                lid,
                name,
                cx,
                cy + kd * 0.5 + 10.0,
                8.5,
                INK,
                750.0,
                0.08,
            );
        }
        // The gain switch: which capture plays.
        parts::print(b, lid, "GAIN", lw * 0.5, lh * 0.43, 8.0, INK, 750.0, 0.1);
        let gain = Place::new("Gain")
            .size(22.0)
            .variant("chrome")
            .bind(ui::bind(&format!("{NS}/option")))
            .legend("")
            .options(&["touch=44"]);
        place::centred_at(b, lid, "switch.brass_toggle3", &gain, lw * 0.5, lh * 0.5);
        let script = Lettering::family(family::DANCING_SCRIPT).weight(700.0);
        set(
            b,
            lid,
            "Morning Glory",
            lw * 0.5,
            lh * 0.63,
            26.0,
            INK,
            &script,
            0.0,
        );
        stomp(b, lid, NS, 34.0, lw * 0.5, lh * 0.77);
        parts::print(b, lid, "JHS", lw * 0.5, lh * 0.92, 10.0, INK, 900.0, 0.2);
    });
    let params = vec![
        drive(NS, "Drive"),
        tone(NS, "Tone"),
        level(NS),
        ui::choice(
            &format!("{NS}/option"),
            "Gain",
            &["Low Gain", "Medium Gain", "High Gain"],
            1,
        ),
        on(NS),
    ];
    Face {
        file: crate::rig_time::kit::tier_file("16-drive-morning-glory", tier, false),
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The Aqua-Puss (analog-voiced slapback): the turquoise box, DELAY,
/// FEEDBACK and BLEND on white caps, the name in a bubbly hand. Binds the
/// rig's delay block: DELAY its `time`, FEEDBACK its `feedback`, BLEND its
/// `level` (the wet's level).
#[must_use]
pub fn aqua_puss() -> Face {
    const W: f64 = 200.0;
    const NS: &str = "aquapuss";
    const INK: &str = "#0b2a33";
    let scene = canvas("Aqua-Puss", |b, root| {
        let (lid, lw, lh) = enclosure(b, root, W, "#3fb4c8", "gloss", &["jack_inset=8"]);
        led(b, lid, NS, lw * 0.5, 10.0, 5.0, "#e0281c");
        for (i, (id, name)) in [
            ("time", "DELAY"),
            ("feedback", "FEEDBACK"),
            ("level", "BLEND"),
        ]
        .iter()
        .enumerate()
        {
            let cx = lw * (0.19 + 0.31 * i as f64);
            let cy = lh * 0.22;
            knob(
                b,
                lid,
                "flat",
                &[
                    "color=#f2f1ec",
                    "pointer=#0b2a33",
                    "index=line",
                    "index_w=0.1",
                    "skirt=plain",
                    "skirt_color=#d6d4cc",
                ],
                &format!("{NS}/{id}"),
                name,
                50.0,
                cx,
                cy,
            );
            parts::print(b, lid, name, cx, cy + 35.0, 8.0, INK, 800.0, 0.06);
        }
        let bubbly = Lettering::family(family::KNEWAVE);
        set(
            b,
            lid,
            "Aqua-Puss",
            lw * 0.5,
            lh * 0.49,
            30.0,
            "#f4fbfd",
            &bubbly,
            0.0,
        );
        parts::print(
            b,
            lid,
            "ANALOG DELAY",
            lw * 0.5,
            lh * 0.62,
            7.5,
            INK,
            800.0,
            0.2,
        );
        stomp(b, lid, NS, 34.0, lw * 0.5, lh * 0.77);
        parts::print(
            b,
            lid,
            "WAY HUGE",
            lw * 0.5,
            lh * 0.92,
            9.0,
            INK,
            900.0,
            0.2,
        );
    });
    let params = vec![
        ui::param(
            &format!("{NS}/time"),
            "Delay",
            Range::log(20.0, 600.0, Unit::Milliseconds),
            120.0,
        )
        .featured(),
        ui::param(
            &format!("{NS}/feedback"),
            "Feedback",
            Range::linear(0.0, 0.95, Unit::None),
            0.15,
        )
        .featured(),
        ui::param(
            &format!("{NS}/level"),
            "Blend",
            Range::linear(-60.0, 12.0, Unit::Db),
            -16.0,
        )
        .featured(),
        on(NS),
    ];
    Face {
        file: "17-pedal-aqua-puss",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// A Cry Baby wah seen from above, rocking: the black casting, the chrome
/// rocker with its ribbed rubber tread and badge. The rocker tips about
/// its axle with `wah/position` — the toe half foreshortening and falling
/// into shadow as the toe goes down, the heel half rising toward you — and
/// the tread is the control (drag toward the toe for treble). Pressing the
/// toe engages it, its LED in the casting above.
#[must_use]
pub fn cry_baby() -> Face {
    const W: f64 = 150.0;
    const H: f64 = 330.0;
    const NS: &str = "wah";
    let scene = canvas("Cry Baby", |b, root| {
        ui::fit_root(b, root, W, H);
        let pos = format!("norm({NS}/position)");
        // The casting.
        let base = draw::rounded(
            draw::rect(
                "casting",
                6.0,
                0.0,
                W - 12.0,
                H,
                draw::linear(
                    90.0,
                    &[
                        (0.0, "#0c0c0d"),
                        (0.3, "#232326"),
                        (0.7, "#1b1b1e"),
                        (1.0, "#0a0a0b"),
                    ],
                ),
            ),
            18.0,
        );
        b.add(
            root,
            draw::with_effects(base, vec![draw::drop_shadow("#000000a0", 0.0, 4.0, 10.0)]),
        );
        led(b, root, NS, W / 2.0, 12.0, 5.0, "#e0281c");
        // The rocker, in two halves about the axle (62 % down).
        let (rx, ry, rw, rh) = (12.0, 24.0, W - 24.0, H - 36.0);
        let axle = ry + rh * 0.62;
        let half = |b: &mut Builder<'_>, name: &str, y0: f64, y1: f64, toe: bool| {
            let g = b.add(root, draw::frame(name, rx, y0, rw, y1 - y0));
            let (gh, gw) = (y1 - y0, rw);
            let chrome = draw::rect(
                "chrome",
                0.0,
                0.0,
                gw,
                gh,
                draw::linear(
                    90.0,
                    &[
                        (0.0, "#5e6166"),
                        (0.18, "#e9ebed"),
                        (0.5, "#a8abaf"),
                        (0.82, "#f0f2f4"),
                        (1.0, "#5e6166"),
                    ],
                ),
            );
            let mut chrome = chrome;
            chrome.corner_radii = if toe {
                document_proto::CornerRadii {
                    top_left: 14.0,
                    top_right: 14.0,
                    bottom_right: 0.0,
                    bottom_left: 0.0,
                }
            } else {
                document_proto::CornerRadii {
                    top_left: 0.0,
                    top_right: 0.0,
                    bottom_right: 14.0,
                    bottom_left: 14.0,
                }
            };
            b.add(g, chrome);
            let (tx, tw) = (9.0, gw - 18.0);
            let (ty, th) = if toe {
                (9.0, gh - 9.0)
            } else {
                (0.0, gh - 38.0)
            };
            b.add(
                g,
                draw::rounded(
                    draw::rect(
                        "tread",
                        tx,
                        ty,
                        tw,
                        th,
                        draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#121315")]),
                    ),
                    5.0,
                ),
            );
            let ribs: Vec<Vec<(f64, f64)>> = (0..((th / 11.0) as usize))
                .map(|i| {
                    let y = ty + 6.0 + i as f64 * 11.0;
                    vec![(tx + 5.0, y), (tx + tw - 5.0, y)]
                })
                .collect();
            b.add(g, ui::strokes("ribs", &ribs, 2.2, draw::solid("#3a3b3f")));
            if !toe {
                let script = Lettering::family(family::PACIFICO);
                set(
                    b,
                    g,
                    "cry baby",
                    gw / 2.0,
                    gh - 20.0,
                    17.0,
                    "#101012",
                    &script,
                    0.0,
                );
            }
            // The shade it falls into as it tips away.
            let shade = draw::rect(
                "shade",
                0.0,
                0.0,
                gw,
                gh,
                draw::solid(if toe { "#000000" } else { "#000000" }),
            );
            let shade = b.add(g, draw::with_opacity(shade, 0.0));
            let tip = if toe {
                pos.clone()
            } else {
                format!("1 - {pos}")
            };
            motion::add(
                b,
                shade,
                Motion::new(Target::Opacity)
                    .input(tip.clone())
                    .range(0.0, 0.45)
                    .dynamics(Dynamics::spring(500.0, 40.0)),
            );
            // Foreshortening about the axle: this half stretches toward you
            // as it rises and flattens as it falls.
            let py = if toe { 1.0 } else { 0.0 };
            motion::add(
                b,
                g,
                Motion::new(Target::StretchY)
                    .input(tip.clone())
                    .range(1.06, 0.86)
                    .pivot(0.5, py)
                    .dynamics(Dynamics::spring(500.0, 40.0)),
            );
            motion::add(
                b,
                g,
                Motion::new(Target::StretchX)
                    .input(tip)
                    .range(1.04, 0.96)
                    .pivot(0.5, py)
                    .dynamics(Dynamics::spring(500.0, 40.0)),
            );
            g
        };
        half(b, "Toe", ry, axle, true);
        half(b, "Heel", axle, ry + rh, false);
        // The axle's line across.
        b.add(
            root,
            draw::rect("axle", rx, axle - 1.0, rw, 2.0, draw::solid("#00000080")),
        );
        // The tread is the control: a light rides it toe-ward with the
        // position.
        let lane = b.add(root, draw::frame("lane", rx, ry + 10.0, rw, rh - 20.0));
        let band_h = 70.0;
        let band = b.add(lane, draw::frame("tilt", 0.0, 0.0, rw, band_h));
        b.add(
            band,
            draw::rect(
                "sheen",
                0.0,
                0.0,
                rw,
                band_h,
                draw::linear(
                    180.0,
                    &[(0.0, "#ffffff00"), (0.5, "#ffffff22"), (1.0, "#ffffff00")],
                ),
            ),
        );
        let mut c = Control::new(ControlKind::Fader {
            orientation: surface_proto::Orientation::Vertical,
        })
        .slot(SlotRole::Thumb, band.0);
        c.binding = Some(ui::bind(&format!("{NS}/position")));
        b.controls.insert(lane, c);
        // The toe engages it.
        let toe = b.add(
            root,
            draw::frame("Toe switch", rx + 20.0, ry, rw - 40.0, 16.0),
        );
        b.controls.insert(
            toe,
            Control::new(ControlKind::footswitch()).bound(Binding::param(format!("{NS}/on"))),
        );
    });
    let params = vec![
        ui::param(
            &format!("{NS}/position"),
            "Position",
            Range::linear(0.0, 1.0, Unit::None),
            0.5,
        )
        .featured(),
        on(NS),
    ];
    Face {
        file: "30-wah-cry-baby",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The Q-Tron+: the black box, six knobs across the top — MODE (LP, BP,
/// HP, MIX), RESPONSE (fast, slow), RANGE (hi, lo), PEAK, GAIN, BOOST —
/// the SWEEP switch and the OVERLOAD lamp, the maker's script, and the
/// name in a psychedelic magenta Q and orange TRON+.
#[must_use]
pub fn q_tron() -> Face {
    const W: f64 = 340.0;
    const NS: &str = "qtron";
    const INK: &str = "#f4f4f2";
    let scene = canvas("Q-Tron+", |b, root| {
        let (lid, lw, lh) = enclosure(b, root, W, "#111113", "satin", &["jack_inset=10"]);
        parts::print(b, lid, "24V", lw * 0.5, 9.0, 6.5, INK, 700.0, 0.06);
        let row = lh * 0.19;
        for (i, (id, name)) in [
            ("mode", "MODE"),
            ("response", "RESPONSE"),
            ("range", "RANGE"),
            ("peak", "PEAK"),
            ("gain", "GAIN"),
            ("boost", "BOOST"),
        ]
        .iter()
        .enumerate()
        {
            let cx = lw * (0.09 + 0.164 * i as f64);
            knob(
                b,
                lid,
                "flat",
                &[
                    "color=#1a1a1c",
                    "pointer=#f4f4f2",
                    "index=line",
                    "index_w=0.12",
                    "skirt=plain",
                    "skirt_color=#0a0a0b",
                ],
                &format!("{NS}/{id}"),
                name,
                42.0,
                cx,
                row,
            );
            parts::print(b, lid, name, cx, row + 30.0, 7.5, INK, 800.0, 0.04);
        }
        parts::print(b, lid, "SWEEP", lw * 0.2, lh * 0.385, 7.5, INK, 800.0, 0.06);
        parts::toggle(
            b,
            lid,
            &format!("{NS}/sweep"),
            "Sweep",
            11.0,
            lw * 0.3,
            lh * 0.385,
        );
        parts::print(
            b,
            lid,
            "OVERLOAD",
            lw * 0.67,
            lh * 0.385,
            7.5,
            INK,
            800.0,
            0.06,
        );
        b.add(
            lid,
            draw::with_effects(
                draw::circle(
                    "overload",
                    lw * 0.77,
                    lh * 0.385,
                    4.0,
                    draw::solid("#e0281c"),
                ),
                vec![draw::drop_shadow("#ff2a1890", 0.0, 0.0, 5.0)],
            ),
        );
        set(
            b,
            lid,
            "electro-harmonix",
            lw * 0.66,
            lh * 0.48,
            15.0,
            INK,
            &Lettering::family(family::PLAYFAIR_DISPLAY).weight(500.0),
            0.0,
        );
        // The name: the big magenta Q, TRON in orange, the +.
        let bubble = Lettering::family(family::KNEWAVE);
        set(
            b,
            lid,
            "Q",
            lw * 0.21,
            lh * 0.66,
            110.0,
            "#d6247f",
            &bubble,
            0.0,
        );
        set(
            b,
            lid,
            "TRON",
            lw * 0.62,
            lh * 0.64,
            62.0,
            "#f7811e",
            &bubble,
            0.04,
        );
        set(
            b,
            lid,
            "+",
            lw * 0.9,
            lh * 0.8,
            40.0,
            INK,
            &Lettering::family(family::INTER).weight(900.0),
            0.0,
        );
        led(b, lid, NS, lw * 0.43, lh * 0.72, 5.0, "#e0281c");
        stomp(b, lid, NS, 30.0, lw * 0.43, lh * 0.86);
    });
    let params = vec![
        ui::choice(&format!("{NS}/mode"), "Mode", &["LP", "BP", "HP", "Mix"], 1),
        ui::choice(&format!("{NS}/response"), "Response", &["Fast", "Slow"], 0),
        ui::choice(&format!("{NS}/range"), "Range", &["Hi", "Lo"], 1),
        ui::param(
            &format!("{NS}/peak"),
            "Peak",
            Range::linear(0.0, 1.0, Unit::None),
            0.6,
        )
        .featured(),
        ui::param(
            &format!("{NS}/gain"),
            "Gain",
            Range::linear(0.0, 1.0, Unit::None),
            0.5,
        )
        .featured(),
        ui::choice(&format!("{NS}/boost"), "Boost", &["Off", "On"], 0),
        ui::choice(&format!("{NS}/sweep"), "Sweep", &["Down", "Up"], 1),
        on(NS),
    ];
    Face {
        file: "31-filter-q-tron",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The pitch stage: up to four voices, each its interval (semitones, the
/// number read out under it) and its own level on a fader, then the dry
/// level and the overall mix. Voices A and B are the rig's pitch block
/// (`semitones`/`a_level`, `b_semitones`/`b_level`); C and D are drawn for
/// the voices to come.
#[must_use]
pub fn pitch_voices() -> Face {
    const W: f64 = 460.0;
    const H: f64 = 286.0;
    const NS: &str = "pitch";
    const INK: &str = "#e8e6e1";
    const DIM: &str = "#8a8d94";
    let scene = canvas("Pitch", |b, root| {
        ui::fit_root(b, root, W, H);
        let mut panel = draw::frame("Panel", 0.0, 0.0, W, H);
        panel.fills = vec![draw::linear(180.0, &[(0.0, "#1b1d22"), (1.0, "#121317")])];
        panel.corner_radii = CornerRadii::uniform(10.0);
        let panel = b.add(root, panel);
        let head = Lettering::family(family::INTER).weight(800.0);
        set(b, panel, "PITCH", 34.0, 18.0, 11.0, "#facc15", &head, 0.3);
        let voices = [
            ("A", "semitones", "a_level", true),
            ("B", "b_semitones", "b_level", true),
            ("C", "c_semitones", "c_level", false),
            ("D", "d_semitones", "d_level", false),
        ];
        let col = (W - 110.0) / 4.0;
        for (i, (v, st, lv, live)) in voices.iter().enumerate() {
            let cx = 18.0 + col * (i as f64 + 0.5);
            let ink = if *live { INK } else { DIM };
            parts::print(
                b,
                panel,
                &format!("VOICE {v}"),
                cx,
                42.0,
                8.0,
                ink,
                800.0,
                0.1,
            );
            knob(
                b,
                panel,
                "flat",
                &[
                    "color=#222428",
                    "pointer=#facc15",
                    "index=line",
                    "index_w=0.1",
                    "skirt=plain",
                    "skirt_color=#16171a",
                ],
                &format!("{NS}/{st}"),
                "Interval",
                46.0,
                cx,
                82.0,
            );
            let value = b.add(
                panel,
                draw::text(
                    "interval",
                    "+12",
                    cx - 24.0,
                    106.0,
                    48.0,
                    11.0,
                    "#facc15",
                    document_proto::TextAlign::Center,
                ),
            );
            let c = Control::new(ControlKind::Readout)
                .slot(SlotRole::Value, value.0)
                .bound(ui::bind(&format!("{NS}/{st}")))
                .formatted(surface_proto::ValueFormat::decimals(0));
            b.controls.insert(value, c);
            let f = Place::new("Level")
                .size(120.0)
                .variant("black")
                .legend("")
                .bind(ui::bind(&format!("{NS}/{lv}")))
                .options(&["touch=44"]);
            place::centred_at(b, panel, "lever.fader", &f, cx, 186.0);
        }
        // Dry and mix, at the right.
        let x = W - 70.0;
        b.add(
            panel,
            draw::rect(
                "divider",
                x - 30.0,
                36.0,
                1.0,
                H - 56.0,
                draw::solid("#ffffff14"),
            ),
        );
        let f = Place::new("Dry")
            .size(110.0)
            .variant("black")
            .legend("")
            .bind(ui::bind(&format!("{NS}/dry")))
            .options(&["touch=44"]);
        place::centred_at(b, panel, "lever.fader", &f, x - 8.0, 122.0);
        knob(
            b,
            panel,
            "flat",
            &[
                "color=#222428",
                "pointer=#e8e6e1",
                "index=line",
                "skirt=plain",
            ],
            &format!("{NS}/mix"),
            "Mix",
            34.0,
            x - 8.0,
            232.0,
        );
        parts::print(b, panel, "MIX", x - 8.0, 264.0, 7.5, INK, 700.0, 0.1);
    });
    let semis = |id: &str, name: &str, d: f64| {
        ui::param(
            &format!("{NS}/{id}"),
            name,
            Range::linear(-24.0, 24.0, Unit::None),
            d,
        )
    };
    let level = |id: &str, name: &str, d: f64| {
        ui::param(
            &format!("{NS}/{id}"),
            name,
            Range::linear(0.0, 1.0, Unit::None),
            d,
        )
    };
    let params = vec![
        semis("semitones", "Voice A", 12.0).featured(),
        level("a_level", "Voice A level", 0.7).featured(),
        semis("b_semitones", "Voice B", -12.0),
        level("b_level", "Voice B level", 0.7),
        semis("c_semitones", "Voice C", 7.0),
        level("c_level", "Voice C level", 0.0),
        semis("d_semitones", "Voice D", 5.0),
        level("d_level", "Voice D level", 0.0),
        level("dry", "Dry", 1.0),
        level("mix", "Mix", 0.5),
    ];
    Face {
        file: "33-pitch-voices",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// What the Q-Tron is doing: a dark screen on a log-frequency grid, the
/// filter's resonant peak riding up and down it with the envelope (the
/// input's level, gained by GAIN — idling on a slow sweep with no input),
/// narrower and taller with PEAK, starting higher with RANGE.
#[must_use]
pub fn filter_viz() -> Face {
    const W: f64 = 560.0;
    const H: f64 = 286.0;
    let scene = canvas("Filter picture", |b, root| {
        ui::fit_root(b, root, W, H);
        let mut screen = draw::frame("Screen", 0.0, 0.0, W, H);
        screen.fills = vec![draw::linear(180.0, &[(0.0, "#0c1220"), (1.0, "#060a12")])];
        screen.clips_content = true;
        screen.corner_radii = CornerRadii::uniform(8.0);
        let screen = b.add(root, screen);
        filter_screen(b, screen, W, H);
    });
    let params = vec![
        ui::param(
            "qtron/gain",
            "Gain",
            Range::linear(0.0, 1.0, Unit::None),
            0.5,
        ),
        ui::param(
            "qtron/peak",
            "Peak",
            Range::linear(0.0, 1.0, Unit::None),
            0.6,
        ),
        ui::toggle("qtron/range", "Range high", false),
    ];
    Face {
        file: "32-filter-viz",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The Q-Tron's picture on `screen` (`W × H`): the grid, the resonant
/// peak riding it with the envelope, the title.
fn filter_screen(b: &mut Builder<'_>, screen: NodeId, w: f64, h: f64) {
    // The grid: octaves across, levels up.
    let mut grid = Vec::new();
    for i in 1..10 {
        let x = w * f64::from(i) / 10.0;
        grid.push(vec![(x, 12.0), (x, h - 22.0)]);
    }
    for i in 1..4 {
        let y = 12.0 + (h - 34.0) * f64::from(i) / 4.0;
        grid.push(vec![(0.0, y), (w, y)]);
    }
    b.add(
        screen,
        ui::strokes("grid", &grid, 1.0, draw::solid("#ffffff10")),
    );
    for (i, l) in ["100", "300", "1k", "3k"].iter().enumerate() {
        b.add(
            screen,
            draw::text(
                "freq",
                l,
                w * (0.12 + 0.22 * i as f64),
                h - 18.0,
                30.0,
                8.0,
                "#6b7a99",
                document_proto::TextAlign::Left,
            ),
        );
    }
    // The response: a band-pass bump, drawn once across twice the
    // screen and slid under it by the sweep.
    let (floor, base) = (h - 30.0, h * 0.55);
    let pts: Vec<(f64, f64)> = (0..=120)
        .map(|i| {
            let x = -w * 0.5 + 2.0 * w * f64::from(i) / 120.0;
            let d = (x - w * 0.5) / (w * 0.12);
            let y = floor - (floor - base) * 0.2 - (floor - 30.0) * 0.8 / (1.0 + d * d);
            (x, y)
        })
        .collect();
    let curve = b.add(screen, draw::frame("Response", 0.0, 0.0, w, h));
    let mut fill = pts.clone();
    fill.push((1.5 * w, floor));
    fill.push((-0.5 * w, floor));
    b.add(
        curve,
        draw::path(
            "area",
            0.0,
            0.0,
            w,
            h,
            draw::polygon(&fill),
            draw::linear(180.0, &[(0.0, "#4dd0ff55"), (1.0, "#4dd0ff05")]),
        ),
    );
    b.add(
        curve,
        draw::emissive(
            ui::strokes("line", &[pts], 2.2, draw::solid("#8fe3ff")),
            draw::hex("#4dd0ff"),
            0.6,
            8.0,
        ),
    );
    // The sweep: the envelope, or a slow idle sweep with no input.
    // The idle sweep runs only while the filter is on (its input is
    // the on switch): off, the picture holds still.
    motion::add(
        b,
        curve,
        Motion::new(Target::None)
            .named("idle")
            .input("param(qtron/on)")
            .dynamics(Dynamics::Lfo {
                rate: Num::Value(0.35),
                shape: Wave::Sine,
                phase: 0.0,
            }),
    );
    let sweep = format!(
        "({span}) * (0.15*norm(qtron/range) + or(level(qtron/in) * (0.3 + norm(qtron/gain)), 0.5 + 0.35*motion(idle))) - {half}",
        span = w * 0.8,
        half = w * 0.4
    );
    motion::add(
        b,
        curve,
        Motion::new(Target::X)
            .input(sweep)
            .dynamics(Dynamics::spring(400.0, 30.0)),
    );
    motion::add(
        b,
        curve,
        Motion::new(Target::StretchY)
            .input("0.6 + 0.7*norm(qtron/peak)")
            .dynamics(Dynamics::spring(400.0, 30.0)),
    );
    b.add(
        screen,
        draw::text(
            "title",
            "FILTER",
            10.0,
            8.0,
            80.0,
            9.0,
            "#8fa3c8",
            document_proto::TextAlign::Left,
        ),
    );
}

/// The Q-Tron's picture composed for a block of Signal's routing grid.
#[must_use]
pub fn filter_block() -> Face {
    let s = crate::rig_cells::S;
    let scene = crate::rig_cells::block(
        "Filter",
        &[(0.0, "#0c1220"), (1.0, "#060a12")],
        |b, v, _| filter_screen(b, v, s, s),
    );
    let mut f = filter_viz();
    f.params.push(ui::toggle("qtron/on", "Bypass", true));
    Face {
        file: "32-filter-viz-block",
        plugin: crate::rig_faces::PLUGIN,
        scene,
        ..f
    }
}

/// The four, in chain order.
#[must_use]
pub fn all() -> Vec<Face> {
    vec![blade(), pegasus(), british_blues(), tube_drive()]
}

/// Every drive face.
#[must_use]
pub fn every() -> Vec<Face> {
    vec![
        blade(),
        pegasus(),
        british_blues(),
        tube_drive(),
        king_of_tone(),
        morning_glory(),
    ]
}

/// The compressor at the head of the board: a satin red box, two big
/// knobs (SUSTAIN, LEVEL) over two small ones (ATTACK, BLEND), the name in
/// white script, the LED and the stomp. Bound to the rig's Pre Comp block:
/// `precomp/sustain` 0–10 maps onto its threshold (the host's map, more
/// sustain lower), the rest are its own params.
#[must_use]
pub fn compressor() -> Face {
    const W: f64 = 196.0;
    const NS: &str = "precomp";
    const INK: &str = "#fbeee9";
    let scene = canvas("Compressor", |b, root| {
        let (lid, lw, lh) = enclosure(
            b,
            root,
            W,
            "#b8221c",
            "satin",
            &["jack_inset=8", "radius=12"],
        );
        b.add(
            lid,
            draw::rect(
                "sheen",
                0.0,
                0.0,
                lw,
                lh,
                draw::linear(
                    120.0,
                    &[(0.0, "#ffffff1c"), (0.45, "#ffffff00"), (1.0, "#00000026")],
                ),
            ),
        );
        led(b, lid, NS, lw * 0.5, 10.0, 5.0, "#ff3a2e");
        let big = [
            "color=#141416",
            "pointer=#f6f2ee",
            "index=line",
            "index_w=0.09",
            "skirt=plain",
            "skirt_color=#0b0b0c",
        ];
        for (i, (id, name)) in [("sustain", "SUSTAIN"), ("makeup", "LEVEL")]
            .iter()
            .enumerate()
        {
            let cx = lw * (0.28 + 0.44 * i as f64);
            knob(
                b,
                lid,
                "flat",
                &big,
                &format!("{NS}/{id}"),
                name,
                52.0,
                cx,
                lh * 0.16,
            );
            parts::print(b, lid, name, cx, lh * 0.16 + 35.0, 8.5, INK, 750.0, 0.12);
        }
        let small = [
            "color=#141416",
            "pointer=#f6f2ee",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#0b0b0c",
        ];
        for (i, (id, name)) in [("attack", "ATTACK"), ("mix", "BLEND")].iter().enumerate() {
            let cx = lw * (0.28 + 0.44 * i as f64);
            knob(
                b,
                lid,
                "flat",
                &small,
                &format!("{NS}/{id}"),
                name,
                36.0,
                cx,
                lh * 0.43,
            );
            parts::print(b, lid, name, cx, lh * 0.43 + 25.0, 7.5, INK, 750.0, 0.12);
        }
        let script = Lettering::family(family::DANCING_SCRIPT).weight(700.0);
        set(
            b,
            lid,
            "Compressor",
            lw * 0.5,
            lh * 0.62,
            26.0,
            INK,
            &script,
            0.0,
        );
        stomp(b, lid, NS, 34.0, lw * 0.5, lh * 0.8);
    });
    let params = vec![
        ui::param(
            &format!("{NS}/sustain"),
            "Sustain",
            Range::linear(0.0, 10.0, Unit::None),
            6.0,
        )
        .featured(),
        ui::param(
            &format!("{NS}/makeup"),
            "Level",
            Range::linear(-24.0, 24.0, Unit::Db),
            0.0,
        )
        .featured(),
        ui::param(
            &format!("{NS}/attack"),
            "Attack",
            Range::log(0.1, 200.0, Unit::Milliseconds),
            20.0,
        ),
        ui::param(
            &format!("{NS}/mix"),
            "Blend",
            Range::linear(0.0, 1.0, Unit::None),
            1.0,
        ),
        on(NS),
    ];
    Face {
        file: "10-drive-compressor",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}
