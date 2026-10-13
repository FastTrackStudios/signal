//! What the studio-blues suite's units share: the hand-built pedal (its
//! impasto-painted enclosure, paint-marker lettering, caps, toggles,
//! label-maker strip, stomp), the suite's one knob helper with its looks,
//! segment readouts, and set type in real faces.

use frame::document::proto::{NodeId, NodeKind, TextAlign};
use frame::surface::kit::draw::{self, Lettering, family};
use frame::surface::kit::{Place, TOUCH_MIN};
use frame::surface::proto::{Binding, ButtonMode, Control, ControlKind, SlotRole, ValueFormat};
use frame::surface::widgets::Builder;

use crate::reverb::ui;

/// The plugin id every unit's catalog merges into.
pub const PLUGIN: &str = "suite-studio-blues";

/// Every pedal is the Standard class's height (60 px/in).
pub const PEDAL_H: f64 = 286.0;

/// The chalky white of a paint marker on the enclosures.
pub const CHALK: &str = "#ecebf2";

// ── the pedal ────────────────────────────────────────────────────────────

/// A small deterministic generator for the brush work.
struct Rng(u64);

impl Rng {
    fn f(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / ((1_u64 << 53) as f64)
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.f()
    }
}

/// A pedal of class width `w`: the kit's painted enclosure (its base, lip
/// and shadow) under a lid of thick navy paint laid on with a brush — each
/// stroke a ridge lit along its top and shadowed under it — worn through
/// to `trim` at the corners. Sized into `root`; returns the enclosure.
pub fn pedal(b: &mut Builder<'_>, root: NodeId, w: f64, trim: Option<&str>) -> NodeId {
    ui::fit_root(b, root, w, PEDAL_H);
    let opts = vec![format!("w={w}"), format!("h={PEDAL_H}")];
    let p = Place {
        options: Some(opts),
        ..Place::new("Enclosure").variant("navy")
    };
    let enc = ui::put_at(b, root, "panel.painted_pedal", &p, 0.0, 0.0);
    let r = (w.min(PEDAL_H) * 0.07).clamp(6.0, 16.0);
    let lh = PEDAL_H - (PEDAL_H * 0.035).max(6.0);
    let mut lid = draw::frame("paint", 0.0, 0.0, w, lh);
    lid.fills = vec![draw::linear(
        172.0,
        &[(0.0, "#2c2e42"), (0.5, "#252736"), (1.0, "#1d1e2a")],
    )];
    lid.corner_radii = frame::document::proto::CornerRadii::uniform(r);
    lid.clips_content = true;
    let lid = b.add(enc, lid);
    let mut rng =
        Rng(w.to_bits() ^ trim.map_or(7, |t| t.len() as u64 * 131 + u64::from(t.as_bytes()[1])));
    // Broad strokes: the body of each brush pass, a shade lighter or darker.
    let mut bodies: [Vec<Vec<(f64, f64)>>; 2] = Default::default();
    // Ridges: the paint heaped at a bristle's edge, lit above, shadowed below.
    let mut lit = Vec::new();
    let mut shade = Vec::new();
    let n = (lh / 2.1) as usize;
    for i in 0..n {
        let y0 = lh * (i as f64 + rng.f()) / n as f64;
        let x0 = rng.range(-20.0, w * 0.5);
        let len = rng.range(w * 0.3, w * 1.1);
        let amp = rng.range(0.3, 1.8);
        let ph = rng.range(0.0, std::f64::consts::TAU);
        let freq = rng.range(0.02, 0.08);
        let tilt = rng.range(-0.025, 0.025);
        let line = |dy: f64| -> Vec<(f64, f64)> {
            (0..=16)
                .map(|k| {
                    let x = x0 + len * f64::from(k) / 16.0;
                    (x, y0 + dy + amp * (x * freq + ph).sin() + tilt * (x - x0))
                })
                .collect()
        };
        if i % 2 == 0 {
            bodies[(i / 2) % 2].push(line(1.8));
        }
        shade.push(line(0.9));
        lit.push(line(0.0));
    }
    b.add(
        lid,
        draw::with_opacity(
            ui::strokes("brush dark", &bodies[0], 6.0, draw::solid("#15172a")),
            0.5,
        ),
    );
    b.add(
        lid,
        draw::with_opacity(
            ui::strokes("brush light", &bodies[1], 5.0, draw::solid("#3a3d52")),
            0.45,
        ),
    );
    b.add(
        lid,
        draw::with_opacity(
            ui::strokes("ridge shadow", &shade, 2.0, draw::solid("#06070d")),
            0.55,
        ),
    );
    b.add(
        lid,
        draw::with_opacity(
            ui::strokes("ridge light", &lit, 1.1, draw::solid("#8d90a8")),
            0.32,
        ),
    );
    // The light across the lid, and its lit and shaded edges.
    b.add(
        lid,
        draw::rect(
            "sheen",
            0.0,
            0.0,
            w,
            lh,
            draw::radial_at(0.28, 0.18, 0.95, &[(0.0, "#ffffff16"), (1.0, "#ffffff00")]),
        ),
    );
    for e in frame::surface::kit::materials::light::edge(
        "lid edge",
        (0.0, 0.0, w, lh),
        r,
        2.5,
        1.0,
        true,
    ) {
        b.add(lid, e);
    }
    if let Some(t) = trim {
        let mut chips = Vec::new();
        for _ in 0..14 {
            let corner = ((rng.f() * 4.0) as usize).min(3);
            let (cx, cy) = [
                (r * 0.6, r * 0.6),
                (w - r * 0.6, r * 0.6),
                (w - r * 0.6, lh - r * 0.6),
                (r * 0.6, lh - r * 0.6),
            ][corner];
            let along = rng.range(0.0, r * 3.0);
            let len = rng.range(2.0, 8.0);
            if rng.f() < 0.5 {
                let x = cx + if cx < w / 2.0 { along } else { -along };
                let y = if cy < lh / 2.0 { 0.9 } else { lh - 0.9 };
                chips.push(vec![(x, y), (x + len, y)]);
            } else {
                let x = if cx < w / 2.0 { 0.9 } else { w - 0.9 };
                let y = cy + if cy < lh / 2.0 { along } else { -along };
                chips.push(vec![(x, y), (x, y + len)]);
            }
        }
        let wear = ui::strokes("wear", &chips, 1.8, draw::solid(t));
        b.add(
            lid,
            draw::with_opacity(draw::emissive(wear, draw::hex(t), 0.25, 2.0), 0.85),
        );
    }
    enc
}

/// Paint-marker lettering centred on (`cx`, `cy`), capitals `size` px tall
/// (a marker's hand, so `i` stays small as the builder wrote it).
pub fn letter(b: &mut Builder<'_>, parent: NodeId, s: &str, cx: f64, cy: f64, size: f64) -> NodeId {
    marker(b, parent, s, cx, cy, size, CHALK)
}

/// Marker lettering in any colour.
#[allow(clippy::too_many_arguments)]
pub fn marker(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    cx: f64,
    cy: f64,
    size: f64,
    color: &str,
) -> NodeId {
    let fs = size * 1.55;
    let w = s.chars().count() as f64 * fs * 0.7 + fs * 2.0;
    let mut t = draw::styled_text(
        "lettering",
        s,
        cx - w / 2.0,
        cy - fs * 0.66,
        w,
        fs,
        color,
        TextAlign::Center,
        &Lettering::family(family::CAVEAT_BRUSH),
    );
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = fs * 0.02;
    }
    b.add(parent, t)
}

// ── knobs ────────────────────────────────────────────────────────────────

/// Every knob look in the suite. All of them go through [`knob`], so the
/// suite changes knob renderer in one place.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    /// The boost's big white fluted cap.
    WhiteFluted,
    /// The envelope filter's plum domes.
    PlumDome,
    /// The vintage overdrive's dark brown domes.
    BrownDome,
    /// The transparent overdrive's mint caps with dark chevrons.
    MintChevron,
    /// The transparent overdrive's other colourway: black caps, blue pointers.
    BluePointer,
    /// The short delay's teal domes.
    TealDome,
    /// The big delay's black domes with a bar across.
    BlackBar,
    /// The reverb's black caps in coral rings.
    CoralRing,
    /// The tabletop unit's cream caps on tan skirts.
    Cream,
    /// The boutique head's chrome cap on a ribbed steel skirt.
    ChromeSkirt,
    /// The lead head's black bakelite chicken-heads.
    ChickenHead,
    /// The combo's black skirts with a chrome insert.
    BlackSkirt,
    /// The desk EQ's grey caps on knurled metal skirts.
    DeskKnurl,
    /// The desk compressor's grey caps in a numbered steel dial.
    GreyDial,
    /// The plugin's own flat arc knob (the cab page's chrome).
    ArcDisc,
}

impl Cap {
    /// The component, its variant and options: `knob.3d` (drawn per pixel
    /// on the GPU, `knob.render` without it) for every hardware cap but
    /// those whose print turns with them — the chevron, the coral rings,
    /// the numbered skirt and dial — and the plugin's own flat arc knob.
    fn look(self) -> (&'static str, &'static str, &'static [&'static str]) {
        match self {
            Self::WhiteFluted => (
                "knob.3d",
                "fluted_cream",
                &[
                    "color=#efede6",
                    "ribs=9",
                    "rib_depth=0.09",
                    "height=0.62",
                    "index=line",
                    "pointer=#161616",
                    "index_w=0.07",
                    "index_in=0.12",
                    "index_out=0.8",
                    "shadow=1.3",
                ],
            ),
            Self::PlumDome => (
                "knob.3d",
                "flat",
                &[
                    "color=#a2338f",
                    "skirt=none",
                    "cap_r=0.97",
                    "height=0.62",
                    "edge=0.16",
                    "coat=0.55",
                    "rough=0.3",
                    "index=line",
                    "pointer=#f4f4f4",
                    "index_w=0.07",
                    "index_in=0.62",
                    "index_out=0.96",
                    "shadow=1.4",
                ],
            ),
            Self::BrownDome => (
                "knob.3d",
                "knurled_dome",
                &[
                    "color=#2e1917",
                    "tilt=22",
                    "spec=0.8",
                    "fallback_color=#3b2620",
                    "index=line",
                    "pointer=#efe8e0",
                    "index_w=0.06",
                    "index_in=0.66",
                    "index_out=0.98",
                    "shadow=1.4",
                ],
            ),
            Self::MintChevron => ("knob.chevron", "green", &[]),
            Self::BluePointer => (
                "knob.3d",
                "knurled",
                &[
                    "color=#3a3a3e",
                    "ribs=44",
                    "rib_depth=0.04",
                    "knurl_band=0.62",
                    "cap_r=0.96",
                    "height=0.64",
                    "dome=0.05",
                    "edge=0.1",
                    "coat=0.3",
                    "index=line",
                    "pointer=#5b9bff",
                    "index_w=0.09",
                    "index_in=0.3",
                    "index_out=0.92",
                    "fallback_color=#26262a",
                    "shadow=1.4",
                ],
            ),
            Self::TealDome => (
                "knob.3d",
                "flat",
                &[
                    "color=#2a8290",
                    "skirt=none",
                    "cap_r=0.97",
                    "height=0.62",
                    "edge=0.16",
                    "coat=0.45",
                    "rough=0.32",
                    "index=line",
                    "pointer=#eef6f6",
                    "index_w=0.07",
                    "index_in=0.62",
                    "index_out=0.96",
                    "shadow=1.4",
                ],
            ),
            Self::BlackBar => (
                "knob.3d",
                "amp_pointer",
                &[
                    "color=#0b0b0d",
                    "skirt=none",
                    "cap_r=0.97",
                    "height=0.56",
                    "tilt=14",
                    "index=line",
                    "pointer=#d9d9dc",
                    "index_w=0.1",
                    "index_in=0.0",
                    "index_out=0.92",
                    "coat=1",
                    "rough=0.12",
                    "spec=1.4",
                    "fallback=chicken_head",
                    "shadow=1.3",
                ],
            ),
            Self::CoralRing => ("knob.ring_insert", "coral", &[]),
            Self::Cream => (
                "knob.3d",
                "stepped_cream",
                &[
                    "color=#e6d3b6",
                    "skirt=plain",
                    "skirt_color=#c7a57e",
                    "skirt_r=1.0",
                    "skirt_h=0.12",
                    "pointer=#3b2a20",
                    "shadow=1.2",
                ],
            ),
            Self::ChromeSkirt => (
                "knob.3d",
                "teardrop",
                &[
                    "color=#161617",
                    "ribs=28",
                    "rib_depth=0.015",
                    "cap_r=0.64",
                    "height=0.5",
                    "insert_color=#dccdbd",
                    "insert_metal=1",
                    "insert_rough=0.16",
                    "insert_r=0.66",
                    "insert_h=0.14",
                    "skirt=metal",
                    "skirt_color=#c6c8cc",
                    "skirt_metal=1",
                    "skirt_rough=0.22",
                    "skirt_r=1.0",
                    "skirt_h=0.2",
                    "fallback=domed",
                    "material=chrome",
                    "fallback_color=#d9dde1",
                    "shadow=1.3",
                ],
            ),
            Self::ChickenHead => (
                "knob.3d",
                "bakelite_chicken",
                &["color=#1a1818", "pointer=#e8e2d6", "shadow=1.3"],
            ),
            Self::BlackSkirt => ("knob.numbered_skirt", "black", &[]),
            Self::DeskKnurl => (
                "knob.3d",
                "flat",
                &[
                    "color=#4a4745",
                    "cap_r=0.66",
                    "height=0.6",
                    "edge=0.1",
                    "dome=0.03",
                    "rough=0.4",
                    "skirt=knurled",
                    "skirt_color=#6d655e",
                    "skirt_metal=0.8",
                    "skirt_rough=0.35",
                    "skirt_r=1.0",
                    "skirt_h=0.24",
                    "skirt_teeth=48",
                    "index=line",
                    "pointer=#f0f0f0",
                    "index_w=0.07",
                    "index_in=0.2",
                    "index_out=0.95",
                    "spec=0.6",
                    "shadow=1.3",
                ],
            ),
            Self::GreyDial => ("knob.dial", "black", &[]),
            Self::ArcDisc => ("knob.arc_disc", "dark", &[]),
        }
    }
}

/// The one way the suite places a knob: `cap`, `d` across, shaft at
/// (`cx`, `cy`), bound to `addr`, printing `scale` round it in `ink`
/// (empty for none). No legend: the face letters its own.
#[allow(clippy::too_many_arguments)]
pub fn knob_scaled(
    b: &mut Builder<'_>,
    parent: NodeId,
    cap: Cap,
    addr: &str,
    name: &str,
    d: f64,
    cx: f64,
    cy: f64,
    scale: &[&str],
    ink: &str,
) -> NodeId {
    let (id, variant, opts) = cap.look();
    let mut p = Place::new(name)
        .size(d)
        .variant(variant)
        .bind(Binding::param(addr))
        .legend("")
        .scale(scale)
        .ink(frame::surface::kit::Ink::Color(ink.into()))
        .keep_case()
        .touch(TOUCH_MIN);
    if !opts.is_empty() {
        // A printed scale brings its ring: dots alone, or ticks by the figures.
        let ticks = match scale {
            [] => "ticks=none",
            s if s.iter().all(|l| l.is_empty()) => "ticks=dots",
            _ => "ticks=lines",
        };
        let opts: Vec<&str> = opts
            .iter()
            .copied()
            .filter(|o| !o.starts_with("ticks="))
            .chain([ticks])
            .collect();
        p = p.options(&opts);
    }
    let k = ui::put(b, parent, id, &p, cx, cy);
    ui::strip_legend(b, k);
    if cap == Cap::GreyDial {
        grey_cap(b, k, d);
    }
    k
}

/// A knob with no printed scale.
#[allow(clippy::too_many_arguments)]
pub fn knob(
    b: &mut Builder<'_>,
    parent: NodeId,
    cap: Cap,
    addr: &str,
    name: &str,
    d: f64,
    cx: f64,
    cy: f64,
) -> NodeId {
    knob_scaled(b, parent, cap, addr, name, d, cx, cy, &[], CHALK)
}

/// A knob in two colourways: `caps[0]` is the control, `caps[1]` is drawn
/// inside it on the same shaft (its turning body joins the control's
/// needles) — returns the control and each colourway's art, for a
/// [`colourway`] switch to show one at a time.
#[allow(clippy::too_many_arguments)]
pub fn knob_pair(
    b: &mut Builder<'_>,
    parent: NodeId,
    caps: [Cap; 2],
    addr: &str,
    name: &str,
    d: f64,
    cx: f64,
    cy: f64,
) -> (NodeId, [NodeId; 2]) {
    let a = knob(b, parent, caps[0], addr, name, d, cx, cy);
    let shaft = b.scene.node(a).and_then(|n| {
        n.children.iter().find_map(|c| {
            b.scene.node(*c).filter(|c| c.name == "knob").map(|c| {
                (
                    c.id,
                    c.transform.tx + c.size.width / 2.0,
                    c.transform.ty + c.size.height / 2.0,
                )
            })
        })
    });
    let Some((art_a, sx, sy)) = shaft else {
        return (a, [a, a]);
    };
    let other = knob(b, a, caps[1], addr, name, d, sx, sy);
    let needles: Vec<_> = b
        .controls
        .remove(&other)
        .map(|c| c.slots_of(SlotRole::Needle).collect())
        .unwrap_or_default();
    if let Some(c) = b.controls.get_mut(&a) {
        c.slots
            .extend(needles.into_iter().map(|node| frame::surface::proto::Slot {
                role: SlotRole::Needle,
                node,
            }));
    }
    (a, [art_a, other])
}

/// A colourway switch: `holder` (a frame round all of them) shows `art[0]`
/// while choice param `addr` reads its first option, `art[1]` otherwise.
pub fn colourway(b: &mut Builder<'_>, holder: NodeId, addr: &str, art: &[[NodeId; 2]]) {
    let mut c = Control::new(ControlKind::Readout);
    for [off, on] in art {
        c = c.slot(SlotRole::Off, off.0).slot(SlotRole::On, on.0);
    }
    c.binding = Some(Binding::param(addr));
    b.controls.insert(holder, c);
}

/// The compressor dial's cap is pale grey plastic, not the kit's black:
/// a round cap (it looks the same at any angle) laid over the dial's
/// centre, lit from the upper left.
fn grey_cap(b: &mut Builder<'_>, k: NodeId, d: f64) {
    let (cx, cy) = b
        .scene
        .node(k)
        .and_then(|n| {
            n.children.iter().find_map(|c| {
                b.scene.node(*c).filter(|c| c.name == "knob").map(|c| {
                    (
                        c.transform.tx + c.size.width / 2.0,
                        c.transform.ty + c.size.height / 2.0,
                    )
                })
            })
        })
        .unwrap_or((0.0, 0.0));
    let r = d * 0.29;
    b.add(
        k,
        draw::with_effects(
            draw::circle("cap well", cx, cy, r + d * 0.012, draw::solid("#1a1b1e")),
            vec![draw::drop_shadow("#00000090", 0.0, 1.0, 3.0)],
        ),
    );
    b.add(
        k,
        draw::circle(
            "cap",
            cx,
            cy,
            r,
            draw::radial_at(
                0.36,
                0.3,
                1.0,
                &[(0.0, "#e2e1de"), (0.55, "#c4c3bf"), (1.0, "#8e8d8a")],
            ),
        ),
    );
    b.add(
        k,
        draw::circle(
            "cap face",
            cx,
            cy,
            r * 0.82,
            draw::radial_at(0.42, 0.36, 1.0, &[(0.0, "#d9d8d4"), (1.0, "#b4b3af")]),
        ),
    );
    b.add(
        k,
        draw::circle(
            "cap glint",
            cx - r * 0.3,
            cy - r * 0.34,
            r * 0.3,
            draw::radial(&[(0.0, "#ffffff55"), (1.0, "#ffffff00")]),
        ),
    );
}

/// A mini bat toggle through the lid on its nut, centred at (`cx`, `cy`).
pub fn toggle(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    name: &str,
    size: f64,
    cx: f64,
    cy: f64,
) -> NodeId {
    let p = Place::new(name)
        .size(size)
        .variant("chrome")
        .bind(Binding::param(addr))
        .legend("")
        .touch(TOUCH_MIN);
    let t = ui::put(b, parent, "switch.toggle_nut", &p, cx, cy);
    ui::strip_legend(b, t);
    t
}

/// The stomp with its LED beside it.
#[allow(clippy::too_many_arguments)]
pub fn stomp(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    name: &str,
    d: f64,
    cx: f64,
    cy: f64,
    led: &str,
    led_left: bool,
) -> NodeId {
    let side = if led_left { "side=left" } else { "side=right" };
    let p = Place::new(name)
        .size(d)
        .bind(Binding::param(addr))
        .legend("")
        .options(&[side, &format!("color={led}")]);
    let root = frame::surface::kit::place(b, parent, "switch.stomp_side_led", &p)
        .unwrap_or_else(|e| panic!("{e}"));
    // Centre the actuator (not the pair) on (cx, cy).
    let (w, h) = b
        .scene
        .node(root)
        .map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    let ax = if led_left { w - d * 0.85 } else { d * 0.85 };
    ui::move_to(b, root, cx - ax, cy - h / 2.0);
    root
}

/// A label-maker strip stuck on a hair crooked: coloured vinyl with a
/// faint gloss and a lit top edge, `text` in plain sans filling it.
#[allow(clippy::too_many_arguments)]
pub fn tape(
    b: &mut Builder<'_>,
    parent: NodeId,
    text: &str,
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
    color: &str,
    tilt: f64,
) -> NodeId {
    let mut f = draw::frame("tape", cx - w / 2.0, cy - h / 2.0, w, h);
    f.fills = vec![
        draw::solid(color),
        draw::linear(
            180.0,
            &[
                (0.0, "#ffffff38"),
                (0.18, "#ffffff14"),
                (0.6, "#ffffff00"),
                (1.0, "#0000001c"),
            ],
        ),
    ];
    f.stroke = frame::document::proto::Stroke {
        paints: vec![draw::solid("#0000002a")],
        weight: 0.7,
        ..frame::document::proto::Stroke::default()
    };
    f.corner_radii = frame::document::proto::CornerRadii::uniform(0.8);
    f.effects = vec![draw::drop_shadow("#00000080", 0.3, 1.2, 1.6)];
    let f = b.add(parent, draw::rotated(f, tilt));
    b.add(
        f,
        draw::rect("lit edge", 0.5, 0.5, w - 1.0, 0.7, draw::solid("#ffffff50")),
    );
    let size = h * 0.72;
    let mut t = draw::styled_text(
        "text",
        text,
        0.0,
        h / 2.0 - size * 0.62,
        w,
        size,
        "#141416",
        TextAlign::Center,
        &Lettering::family(family::INTER).weight(420.0),
    );
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = size * 0.01;
    }
    b.add(f, t);
    f
}

/// A radio column of small lamps, one per option of choice param `addr`,
/// each lit when its option is chosen: (`x`, `y`) is the first lamp, `dy`
/// the pitch.
#[allow(clippy::too_many_arguments)]
pub fn radio(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    n: usize,
    x: f64,
    y: f64,
    dy: f64,
    d: f64,
    color: &str,
) {
    for i in 0..n {
        let cell = b.add(
            parent,
            draw::frame(
                &format!("mode {i}"),
                x - dy / 2.0,
                y + dy * i as f64 - dy / 2.0,
                dy,
                dy,
            ),
        );
        b.add(
            cell,
            draw::stroked(
                draw::circle(
                    "ring",
                    dy / 2.0,
                    dy / 2.0,
                    d / 2.0,
                    draw::solid("#00000000"),
                ),
                "#d8d8dc",
                1.0,
            ),
        );
        let lit = draw::circle(
            "lamp",
            dy / 2.0,
            dy / 2.0,
            d / 2.0 - 0.6,
            draw::solid(color),
        );
        let lit = b.add(cell, draw::emissive(lit, draw::hex(color), 0.6, d * 1.5));
        let mut c = Control::new(ControlKind::Button {
            mode: ButtonMode::Select {
                index: i as u32,
                count: n as u32,
            },
        })
        .slot(SlotRole::Led, lit.0);
        c.binding = Some(Binding::param(addr));
        b.controls.insert(cell, c);
    }
}

// ── displays ─────────────────────────────────────────────────────────────

/// A segment display: a dark window with every segment ghosted faintly
/// behind the lit ones, `digits` wide, reading `addr` in `format`, in
/// DSEG14 (`seg14`) or DSEG7. The window is (`x`, `y`, `w`, `h`).
#[allow(clippy::too_many_arguments)]
pub fn segments(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    digits: usize,
    size: f64,
    color: &str,
    format: ValueFormat,
    seg14: bool,
) -> NodeId {
    let mut win = draw::frame("Display", x, y, w, h);
    win.fills = vec![draw::linear(180.0, &[(0.0, "#050507"), (1.0, "#0d0e12")])];
    win.corner_radii = frame::document::proto::CornerRadii::uniform((h * 0.16).min(6.0));
    win.stroke = frame::document::proto::Stroke {
        paints: vec![draw::solid("#00000099")],
        weight: 1.0,
        ..frame::document::proto::Stroke::default()
    };
    win.effects = vec![draw::drop_shadow("#ffffff22", 0.0, 0.8, 0.0)];
    win.clips_content = true;
    let win = b.add(parent, win);
    let fam = if seg14 { family::DSEG14 } else { family::DSEG7 };
    let l = Lettering::family(fam).italic();
    let ghost = if seg14 {
        "~".repeat(digits)
    } else {
        "8".repeat(digits)
    };
    let pad = h * 0.14;
    let ty = h / 2.0 - size * 0.62;
    let mut g = draw::styled_text(
        "ghost",
        &ghost,
        pad,
        ty,
        w - 2.0 * pad,
        size,
        color,
        TextAlign::Right,
        &l,
    );
    g.opacity = 0.09;
    b.add(win, g);
    let v = draw::styled_text(
        "value",
        "",
        pad,
        ty,
        w - 2.0 * pad,
        size,
        color,
        TextAlign::Right,
        &l,
    );
    let v = b.add(win, draw::emissive(v, draw::hex(color), 0.55, size * 0.35));
    // A glass sheen over the window.
    b.add(
        win,
        draw::rect(
            "glass",
            0.0,
            0.0,
            w,
            h * 0.45,
            draw::linear(180.0, &[(0.0, "#ffffff12"), (1.0, "#ffffff00")]),
        ),
    );
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, v.0);
    c.binding = Some(Binding::param(addr));
    c.format = Some(format);
    b.controls.insert(win, c);
    win
}

// ── set type ─────────────────────────────────────────────────────────────

/// Set type centred on (`cx`, `cy`) in `l`, tracked `track` em.
#[allow(clippy::too_many_arguments)]
pub fn set(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    cx: f64,
    cy: f64,
    size: f64,
    color: &str,
    l: &Lettering,
    track: f64,
) -> NodeId {
    set_aligned(
        b,
        parent,
        s,
        cx,
        cy,
        size,
        color,
        l,
        track,
        TextAlign::Center,
    )
}

/// Set type in `l` aligned `align` at `x` (its left, centre or right).
#[allow(clippy::too_many_arguments)]
pub fn set_aligned(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    x: f64,
    cy: f64,
    size: f64,
    color: &str,
    l: &Lettering,
    track: f64,
    align: TextAlign,
) -> NodeId {
    let w = s.chars().count() as f64 * size * (0.9 + track) + size * 2.0;
    let x0 = match align {
        TextAlign::Left => x,
        TextAlign::Right => x - w,
        _ => x - w / 2.0,
    };
    let mut t = draw::styled_text("print", s, x0, cy - size * 0.68, w, size, color, align, l);
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = size * track;
    }
    b.add(parent, t)
}

fn inter(weight: f64) -> Lettering {
    Lettering::family(family::INTER).weight(weight)
}

/// Set type (the units that are printed, not lettered): Inter at
/// `weight`, tracked `track` em, centred on (`cx`, `cy`).
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
    set(b, parent, s, cx, cy, size, color, &inter(weight), track)
}

/// Set type aligned left from `x`.
#[allow(clippy::too_many_arguments)]
pub fn print_left(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    x: f64,
    cy: f64,
    size: f64,
    color: &str,
    weight: f64,
    track: f64,
) -> NodeId {
    set_aligned(
        b,
        parent,
        s,
        x,
        cy,
        size,
        color,
        &inter(weight),
        track,
        TextAlign::Left,
    )
}

/// Set type aligned right to `x`.
#[allow(clippy::too_many_arguments)]
pub fn print_right(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    x: f64,
    cy: f64,
    size: f64,
    color: &str,
    weight: f64,
    track: f64,
) -> NodeId {
    set_aligned(
        b,
        parent,
        s,
        x,
        cy,
        size,
        color,
        &inter(weight),
        track,
        TextAlign::Right,
    )
}
