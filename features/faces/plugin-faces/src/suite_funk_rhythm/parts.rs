//! What the amp-suite faces share: sizing the page to the unit's form,
//! rendered pedal enclosures, the one knob helper, placing kit parts by
//! their centre with the face's own legends, the printed lettering (real
//! families and weights), colourway layers, and the catalog entries a unit
//! declares.
//!
//! Used by both artist-style suites (`suite_funk_rhythm`,
//! `suite_boutique_clean`).

use document_proto::{NodeId, NodeKind, TextAlign, TextCase};
use surface_kit::{Ink, Place, TOUCH_MIN, draw};
use surface_proto::{Axis, Condition, Control, ControlKind, HandleAxis, ParamInfo, Range, Unit};
use surface_widgets::Builder;

use crate::reverb::ui;

// ── form factors (docs/form-factors.md, 60 design px per inch) ────────────

/// Every pedal's height (the Standard class's depth).
pub const PEDAL_H: f64 = 286.0;
/// Standard pedal width.
pub const STANDARD_W: f64 = 156.0;
/// Big-box (double) pedal width.
pub const BIGBOX_W: f64 = 404.0;
/// The treadle (wah / expression) class.
pub const TREADLE: (f64, f64) = (241.0, 600.0);
/// An amp head's width; the height follows the faceplate's aspect.
pub const HEAD_W: f64 = 1200.0;
/// The cab page.
pub const CAB: f64 = 900.0;
/// A 19-inch rack face without ears.
pub const RACK_W: f64 = 1140.0;

// ── placing ──────────────────────────────────────────────────────────────

/// Size the page's root to the unit.
pub fn page(b: &mut Builder<'_>, root: NodeId, w: f64, h: f64) {
    ui::fit_root(b, root, w, h);
}

/// A rendered pedal's lid: the frame its parts go on, and its size.
#[derive(Clone, Copy)]
pub struct Lid {
    pub id: NodeId,
    pub w: f64,
    pub h: f64,
}

impl Lid {
    /// A point given as fractions of the lid.
    #[must_use]
    pub fn at(&self, fx: f64, fy: f64) -> (f64, f64) {
        (self.w * fx, self.h * fy)
    }
}

/// A rendered stompbox (`panel.pedal_3d`) filling the page (`w × h`): the
/// lid in `finish` and `color`, its front face and base plate in `base`,
/// the jacks standing out of its sides. Returns the lid — the pedal's parts
/// go on it, in its own coordinates.
///
/// # Panics
/// If the kit's enclosure has no `lid` frame.
pub fn stompbox(
    b: &mut Builder<'_>,
    root: NodeId,
    size: (f64, f64),
    color: &str,
    finish: &str,
    base: &str,
    radius: f64,
) -> Lid {
    stompbox_with(b, root, size, color, finish, base, radius, &[])
}

/// [`stompbox`] with more `panel.pedal_3d` options (`jack_inset=`, `front=`).
///
/// # Panics
/// If the kit's enclosure has no `lid` frame.
#[allow(clippy::too_many_arguments)]
pub fn stompbox_with(
    b: &mut Builder<'_>,
    root: NodeId,
    (w, h): (f64, f64),
    color: &str,
    finish: &str,
    base: &str,
    radius: f64,
    extra: &[&str],
) -> Lid {
    page(b, root, w, h);
    let mut opts = vec![
        format!("w={w}"),
        format!("h={h}"),
        format!("color={color}"),
        format!("finish={finish}"),
        format!("base={base}"),
        format!("radius={radius}"),
        "jacks=side".into(),
    ];
    opts.extend(extra.iter().map(ToString::to_string));
    let class = if w > 300.0 { "bigbox" } else { "standard" };
    let p = Place {
        options: Some(opts),
        ..Place::new("Enclosure").variant(class)
    };
    let body = ui::put_at(b, root, "panel.pedal_3d", &p, 0.0, 0.0);
    let lid = b
        .scene
        .node(body)
        .and_then(|n| {
            n.children
                .iter()
                .copied()
                .find(|c| b.scene.node(*c).is_some_and(|k| k.name == "lid"))
        })
        .expect("panel.pedal_3d has a lid");
    let (lw, lh) = b
        .scene
        .node(lid)
        .map_or((w, h), |n| (n.size.width, n.size.height));
    Lid {
        id: lid,
        w: lw,
        h: lh,
    }
}

/// How a rendered knob looks: `knob.render`'s cap variant and options.
#[derive(Clone, Copy)]
pub struct KnobLook<'a> {
    pub cap: &'a str,
    pub opts: &'a [&'a str],
}

/// Every knob of both artist suites is placed here — one place to switch
/// them all to another knob renderer: a rendered knob (`knob.render`) of
/// diameter `d` on `addr`, its shaft at (`cx`, `cy`), its hit box tight
/// round the body, its printed ring in `ink`. The face prints the legend.
#[allow(clippy::too_many_arguments)]
pub fn knob(
    b: &mut Builder<'_>,
    parent: NodeId,
    look: &KnobLook<'_>,
    addr: &str,
    name: &str,
    d: f64,
    (cx, cy): (f64, f64),
    ink: &str,
) -> NodeId {
    let mut p = Place::new(name)
        .size(d)
        .variant(look.cap)
        .bind(ui::bind(addr))
        .ink(Ink::Color(ink.into()))
        .keep_case()
        .touch(TOUCH_MIN);
    if !look.opts.is_empty() {
        p = p.options(look.opts);
    }
    let k = ui::put(b, parent, "knob.render", &p, cx, cy);
    ui::strip_legend(b, k);
    k
}

/// [`knob`] as a GPU knob (`knob.3d`): `look.cap` names its preset,
/// `look.opts` its options. Where the GPU stage doesn't run it draws as
/// `knob.render` (the preset's fallback).
#[allow(clippy::too_many_arguments)]
pub fn knob3d(
    b: &mut Builder<'_>,
    parent: NodeId,
    look: &KnobLook<'_>,
    addr: &str,
    name: &str,
    d: f64,
    (cx, cy): (f64, f64),
    ink: &str,
) -> NodeId {
    let mut p = Place::new(name)
        .size(d)
        .variant(look.cap)
        .bind(ui::bind(addr))
        .ink(Ink::Color(ink.into()))
        .keep_case()
        .touch(TOUCH_MIN);
    if !look.opts.is_empty() {
        p = p.options(look.opts);
    }
    let k = ui::put(b, parent, "knob.3d", &p, cx, cy);
    ui::strip_legend(b, k);
    k
}

/// Give the readout control `id` its own value format.
pub fn format(b: &mut Builder<'_>, id: NodeId, f: surface_proto::ValueFormat) {
    if let Some(c) = b.controls.get_mut(&id) {
        c.format = Some(f);
    }
}

/// A kit control placed by the centre of its child named one of `anchor`
/// (or its box), legend stripped.
#[allow(clippy::too_many_arguments)]
pub fn part(
    b: &mut Builder<'_>,
    parent: NodeId,
    id: &str,
    p: Place,
    anchor: &[&str],
    (cx, cy): (f64, f64),
) -> NodeId {
    let k = ui::put_by(b, parent, id, &p, anchor, cx, cy);
    ui::strip_legend(b, k);
    k
}

/// A two-position toggle (`switch.toggle_nut`, chrome or black bat) on
/// `addr`, its nut at (`cx`, `cy`); legends are the face's.
pub fn toggle(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    name: &str,
    size: f64,
    variant: &str,
    at: (f64, f64),
) -> NodeId {
    let p = Place::new(name)
        .size(size)
        .variant(variant)
        .bind(ui::bind(addr))
        .legend("")
        .options(&["legend_right"])
        .touch(TOUCH_MIN);
    part(b, parent, "switch.toggle_nut", p, &["bushing"], at)
}

/// Move a placed control so its box is centred on (`cx`, `cy`).
pub fn centre_on(b: &mut Builder<'_>, id: NodeId, (cx, cy): (f64, f64)) {
    let (w, h) = b
        .scene
        .node(id)
        .map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    ui::move_to(b, id, cx - w / 2.0, cy - h / 2.0);
}

/// A stomp footswitch on toggle `addr` (no LED of its own — the pedal's
/// LED is a separate [`status_led`]), its actuator `d` across centred at
/// (`cx`, `cy`). `momentary` for a tap switch.
#[allow(clippy::too_many_arguments)]
pub fn footswitch(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    name: &str,
    d: f64,
    at: (f64, f64),
    momentary: bool,
) -> NodeId {
    let mut p = Place::new(name)
        .size(d)
        .variant("none")
        .bind(ui::bind(addr));
    if momentary {
        p = p.options(&["momentary"]);
    }
    let k = ui::put_by(b, parent, "switch.footswitch", &p, &["cap"], at.0, at.1);
    ui::strip_legend(b, k);
    k
}

/// An LED showing toggle `addr`, lens `d` across, in `color`, bezel
/// `variant` (`light`, `dark`, `chrome`); `pulse` makes it breathe at a
/// rate (a motion expression in Hz).
#[allow(clippy::too_many_arguments)]
pub fn status_led(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    d: f64,
    at: (f64, f64),
    color: &str,
    variant: &str,
    pulse: Option<&str>,
) -> NodeId {
    let mut opts = vec![format!("color={color}")];
    if let Some(r) = pulse {
        opts.push(format!("pulse={r}"));
    }
    let p = Place {
        options: Some(opts),
        ..Place::new("LED")
            .size(d)
            .variant(variant)
            .bind(ui::bind(addr))
    };
    let k = surface_kit::place(b, parent, "meter.status_led", &p).unwrap_or_else(|e| panic!("{e}"));
    centre_on(b, k, at);
    k
}

// ── colourways ───────────────────────────────────────────────────────────

/// A layer of a unit's art shown only while choice `addr` is option
/// `index` (a colourway, a cab's look): a frame `(x, y, w, h)` under
/// `parent` carrying a `Readout` of the choice with a `show_if`, so the
/// runtime hides it for the other options. Paint the variant into it; put
/// the unit's controls outside it.
pub fn variant_layer(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    index: usize,
    (x, y, w, h): (f64, f64, f64, f64),
) -> NodeId {
    let layer = b.add(
        parent,
        draw::frame(&format!("{addr} = {index}"), x, y, w, h),
    );
    // The readout's value: the option's name, printed clear (nothing on
    // the unit shows it; the layer's art is the reading).
    let value = b.add(
        layer,
        draw::text(
            "colourway",
            "",
            0.0,
            0.0,
            8.0,
            1.0,
            "#00000000",
            TextAlign::Left,
        ),
    );
    let mut c = Control::new(ControlKind::Readout).slot(surface_proto::SlotRole::Value, value.0);
    c.binding = Some(ui::bind(addr));
    c.show_if = Some(Condition::any_of(addr, &[index as f64]));
    b.controls.insert(layer, c);
    layer
}

// ── cab mics ─────────────────────────────────────────────────────────────

/// A mic seen from the front, on a `Handle` over the speaker at
/// (`cx`, `cy`), radius `r`: x is its position (`pos`: the dust cap out to
/// the cone's edge), y its distance (`dist`: up and away from the cone).
pub fn mic_handle(
    b: &mut Builder<'_>,
    parent: NodeId,
    (cx, cy, r): (f64, f64, f64),
    label: &str,
    pos: &str,
    dist: &str,
) -> NodeId {
    let lane = b.add(
        parent,
        draw::frame(&format!("{label} lane"), cx, cy - r * 0.9, r, r * 0.9),
    );
    let touch = 44.0;
    let handle = b.add(lane, draw::frame(label, 0.0, 0.0, touch, touch));
    let c = touch / 2.0;
    // A shadow on the cone, the body falling away below the head, the
    // head's mesh.
    b.add(
        handle,
        draw::with_effects(
            draw::circle(
                "mic shadow",
                c + 6.0,
                c + 9.0,
                13.0,
                draw::solid("#00000050"),
            ),
            vec![draw::drop_shadow("#00000040", 0.0, 0.0, 5.0)],
        ),
    );
    b.add(
        handle,
        draw::rounded(
            draw::rect(
                "mic body",
                c - 7.0,
                c,
                14.0,
                40.0,
                draw::linear(
                    90.0,
                    &[
                        (0.0, "#050505"),
                        (0.45, "#4a4a4e"),
                        (0.6, "#1a1a1b"),
                        (1.0, "#050505"),
                    ],
                ),
            ),
            5.0,
        ),
    );
    b.add(
        handle,
        draw::with_effects(
            draw::circle(
                "mic head",
                c,
                c,
                13.0,
                draw::radial_at(
                    0.38,
                    0.32,
                    0.8,
                    &[(0.0, "#8a8a8e"), (0.5, "#3a3a3d"), (1.0, "#0b0b0c")],
                ),
            ),
            vec![draw::drop_shadow("#00000080", 0.0, 2.0, 3.0)],
        ),
    );
    let mesh: Vec<Vec<(f64, f64)>> = (1..4).map(|k| ring(c, c, 3.5 * f64::from(k))).collect();
    b.add(
        handle,
        ui::strokes("mic mesh", &mesh, 0.7, draw::solid("#b8b8bc80")),
    );
    let tag = label
        .chars()
        .last()
        .map(|ch| ch.to_string())
        .unwrap_or_default();
    centred(
        b,
        handle,
        &tag,
        (c, c + 27.0),
        Type::new(8.0, "#f4f4f2", 800.0),
    );
    let kind = ControlKind::Handle {
        x: Some(ui::bind(pos)),
        y: Some(ui::bind(dist)),
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
    };
    b.controls.insert(handle, Control::new(kind));
    handle
}

// ── lettering ────────────────────────────────────────────────────────────

/// How a line of lettering is set: size, colour, weight, tracking, case,
/// and the family (Inter unless set) — set in the font's own weight, never
/// faked.
#[derive(Clone, Copy)]
pub struct Type<'a> {
    pub size: f64,
    pub color: &'a str,
    pub weight: f64,
    /// Letter spacing, in ems.
    pub track: f64,
    pub upper: bool,
    pub family: Option<&'a str>,
    pub italic: bool,
    /// Percent of normal width (families with a width axis).
    pub width: Option<f64>,
}

impl<'a> Type<'a> {
    #[must_use]
    pub const fn new(size: f64, color: &'a str, weight: f64) -> Self {
        Self {
            size,
            color,
            weight,
            track: 0.0,
            upper: false,
            family: None,
            italic: false,
            width: None,
        }
    }
    #[must_use]
    pub const fn track(mut self, em: f64) -> Self {
        self.track = em;
        self
    }
    #[must_use]
    pub const fn upper(mut self) -> Self {
        self.upper = true;
        self
    }
    #[must_use]
    pub const fn family(mut self, family: &'a str) -> Self {
        self.family = Some(family);
        self
    }
    #[must_use]
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    #[must_use]
    pub const fn width(mut self, percent: f64) -> Self {
        self.width = Some(percent);
        self
    }
}

/// The text node for `s` set as `t`, in a box `w` wide from `x`, centred
/// on `cy`.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn set(
    s: &str,
    x: f64,
    cy: f64,
    w: f64,
    align: TextAlign,
    t: Type<'_>,
) -> document_proto::Node {
    let mut n = ui::text(
        "print", s, x, cy, w, t.size, t.color, align, t.weight, false,
    );
    if let NodeKind::Text(d) = &mut n.kind {
        d.letter_spacing = t.size * t.track;
        if t.upper {
            d.text_case = TextCase::Upper;
        }
    }
    let mut l = t
        .family
        .map_or_else(draw::Lettering::default, draw::Lettering::family)
        .weight(t.weight);
    if t.italic {
        l = l.italic();
    }
    if let Some(wd) = t.width {
        l = l.width(wd);
    }
    draw::lettered(n, &l)
}

/// A line of lettering in a box `w` wide from `x`, aligned `align`,
/// vertically centred on `cy`.
#[allow(clippy::too_many_arguments)]
pub fn words(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    x: f64,
    cy: f64,
    w: f64,
    align: TextAlign,
    t: Type<'_>,
) -> NodeId {
    b.add(parent, set(s, x, cy, w, align, t))
}

/// A generous box width for `s` set as `t`.
fn box_w(s: &str, t: Type<'_>) -> f64 {
    (s.chars().count() as f64 * t.size * (0.72 + t.track) + t.size * 2.0).max(8.0)
}

/// Lettering centred on (`cx`, `cy`).
pub fn centred(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    (cx, cy): (f64, f64),
    t: Type<'_>,
) -> NodeId {
    let w = box_w(s, t);
    words(b, parent, s, cx - w / 2.0, cy, w, TextAlign::Center, t)
}

/// Stacked lines set from the left at `x`, the first centred on `cy`,
/// `lead` px apart (a pedal's name in the corner).
pub fn stacked(
    b: &mut Builder<'_>,
    parent: NodeId,
    lines: &[&str],
    (x, cy): (f64, f64),
    lead: f64,
    t: Type<'_>,
) {
    for (i, l) in lines.iter().enumerate() {
        words(
            b,
            parent,
            l,
            x,
            cy + lead * i as f64,
            box_w(l, t),
            TextAlign::Left,
            t,
        );
    }
}

/// Lettering with a cartoon outline (a maker's mark): one line of text
/// whose `outline` is a crisp glow `px` wide round the glyphs, dropped a
/// little so the letters sit on a thicker lower edge.
pub fn outlined(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    (cx, cy): (f64, f64),
    t: Type<'_>,
    outline: &str,
    px: f64,
) -> NodeId {
    let w = box_w(s, t);
    let mut n = set(s, cx - w / 2.0, cy, w, TextAlign::Center, t);
    let glow = |dy: f64| document_proto::Effect {
        kind: document_proto::EffectKind::DropShadow,
        visible: true,
        color: draw::hex(outline),
        offset_x: 0.0,
        offset_y: dy,
        radius: px * 0.25,
        spread: px,
    };
    n.effects = vec![glow(px * 0.7), glow(0.0)];
    b.add(parent, n)
}

/// A stroked line drawing (icons, printed artwork), points in `parent`'s
/// space, round caps.
pub fn art(
    b: &mut Builder<'_>,
    parent: NodeId,
    lines: &[Vec<(f64, f64)>],
    weight: f64,
    color: &str,
) -> NodeId {
    b.add(
        parent,
        ui::strokes("art", lines, weight, draw::solid(color)),
    )
}

/// A circle's outline as a polyline (for [`art`]).
#[must_use]
pub fn ring(cx: f64, cy: f64, r: f64) -> Vec<(f64, f64)> {
    ui::ellipse_pts(cx, cy, r, r, 48)
}

/// A rounded rectangle's outline as a polyline (for [`art`]).
#[must_use]
pub fn round_rect(x: f64, y: f64, w: f64, h: f64, r: f64) -> Vec<(f64, f64)> {
    let mut pts = Vec::new();
    let corners = [
        (x + w - r, y + r, -90.0),
        (x + w - r, y + h - r, 0.0),
        (x + r, y + h - r, 90.0),
        (x + r, y + r, 180.0),
    ];
    for (cx, cy, a0) in corners {
        for i in 0..=6 {
            let a = (a0 + 15.0 * f64::from(i)).to_radians();
            pts.push((cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    pts.push(pts[0]);
    pts
}

/// A small screw head.
pub fn screw(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64, metal: &str) {
    let head = draw::circle(
        "screw",
        cx,
        cy,
        r,
        draw::radial_at(0.35, 0.3, 0.8, &[(0.0, "#f6f5f0"), (1.0, metal)]),
    );
    b.add(
        parent,
        draw::with_effects(
            head,
            vec![
                draw::inner_shadow("#00000080", 0.0, 0.0, r * 0.35),
                draw::drop_shadow("#00000066", 0.0, r * 0.2, r * 0.3),
            ],
        ),
    );
    b.add(
        parent,
        draw::rotated(
            draw::rect(
                "slot",
                cx - r * 0.62,
                cy - r * 0.12,
                r * 1.24,
                r * 0.24,
                draw::solid("#00000099"),
            ),
            35.0,
        ),
    );
}

/// An input jack with a cable running out of it and down off the unit.
pub fn cable(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, to: &[(f64, f64)]) {
    let nut = draw::path(
        "jack nut",
        cx - r * 1.3,
        cy - r * 1.3,
        r * 2.6,
        r * 2.6,
        draw::regular(r * 1.3, r * 1.3, r * 1.3, 6, 0.0),
        draw::linear(
            160.0,
            &[(0.0, "#f2f2ee"), (0.5, "#8d8e8b"), (1.0, "#d8d8d4")],
        ),
    );
    b.add(
        parent,
        draw::with_effects(nut, vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)]),
    );
    b.add(
        parent,
        draw::circle(
            "jack",
            cx,
            cy,
            r * 0.8,
            draw::radial(&[(0.0, "#1a1a1a"), (1.0, "#444444")]),
        ),
    );
    let mut pts = vec![(cx, cy)];
    pts.extend_from_slice(to);
    b.add(
        parent,
        draw::with_effects(
            ui::strokes("cable", &[pts.clone()], r * 0.9, draw::solid("#1b1b1d")),
            vec![draw::drop_shadow("#00000070", 0.0, 3.0, 4.0)],
        ),
    );
    b.add(
        parent,
        ui::strokes(
            "cable sheen",
            &[pts
                .iter()
                .map(|(x, y)| (x - r * 0.15, y - r * 0.15))
                .collect()],
            r * 0.2,
            draw::solid("#ffffff22"),
        ),
    );
    b.add(
        parent,
        draw::circle(
            "plug",
            cx,
            cy,
            r * 0.62,
            draw::radial_at(0.4, 0.35, 0.7, &[(0.0, "#e8e8e4"), (1.0, "#6a6a68")]),
        ),
    );
}

// ── catalog ──────────────────────────────────────────────────────────────

/// A 0–10 amp-style control (printed 0…10), default `v`.
#[must_use]
pub fn dial10(addr: &str, name: &str, v: f64) -> ParamInfo {
    ui::param(addr, name, Range::linear(0.0, 10.0, Unit::None), v).featured()
}

/// A 0–100 % control.
#[must_use]
pub fn pct(addr: &str, name: &str, v: f64) -> ParamInfo {
    ui::percent(addr, name, v).featured()
}

/// A dB control over `lo..hi`.
#[must_use]
pub fn db(addr: &str, name: &str, lo: f64, hi: f64, v: f64) -> ParamInfo {
    let p = ui::param(addr, name, Range::linear(lo, hi, Unit::Db), v).featured();
    if lo < 0.0 && hi > 0.0 { p.bipolar() } else { p }
}

/// A frequency control over `lo..hi` Hz, logarithmic.
#[must_use]
pub fn hz(addr: &str, name: &str, lo: f64, hi: f64, v: f64) -> ParamInfo {
    ui::param(addr, name, Range::log(lo, hi, Unit::Hz), v).featured()
}

/// A time control in ms over `lo..hi`, logarithmic.
#[must_use]
pub fn ms(addr: &str, name: &str, lo: f64, hi: f64, v: f64) -> ParamInfo {
    ui::param(addr, name, Range::log(lo, hi, Unit::Milliseconds), v).featured()
}

/// The value of a log range `lo..hi` at position `t` (0..1): for setting a
/// default to where the reference's knob points.
#[must_use]
pub fn log_at(lo: f64, hi: f64, t: f64) -> f64 {
    lo * (hi / lo).powf(t)
}

/// An on/off.
#[must_use]
pub fn switch(addr: &str, name: &str, on: bool) -> ParamInfo {
    ui::toggle(addr, name, on).featured()
}

/// A stepped choice.
#[must_use]
pub fn pick(addr: &str, name: &str, options: &[&str], default: usize) -> ParamInfo {
    ui::choice(addr, name, options, default).featured()
}
