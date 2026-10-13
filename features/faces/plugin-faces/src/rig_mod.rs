//! The rig's modulation: a face for the Modulation module's chorus, phaser
//! and flanger and the Motion module's tremolo, vibrato and rotary speaker,
//! each a clean picture of what the effect does to the sound, with its
//! knobs beside it. Sleek, like the Time module's Analog: a dark panel, one
//! picture, a light name.
//!
//! Each fills the body of its panel (under the panel's own header, which
//! keeps the power, the ‹ › rotation and the engine picker): [`W`] × [`H`].
//! Colour, as Signal draws the groups: Modulation light blue, Motion green.
//!
//! Binding: the chorus's, phaser's and flanger's params are
//! `mod/<block param>`, the tremolo's, vibrato's and rotary's
//! `motion/<block param>`; `<ns>/on` is the block's bypass.

use frame::document::proto::NodeId;
use frame::document::proto::motion::{Dynamics, Motion, Target};
use frame::surface::kit::{draw, motion};
use frame::surface::proto::{ParamInfo, Range, Unit};
use frame::surface::widgets::Builder;
use serde_json::json;

use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::rig_time::kit;

mod flanger;
mod phaser;
mod rotary;

pub const W: f64 = 400.0;
pub const H: f64 = 340.0;
pub const MOD: &str = "mod";
pub const MOTION: &str = "motion";
/// Modulation light blue, Motion green (Signal's group colours).
pub const SKY: &str = "#7dd3fc";
pub const GREEN: &str = "#34d399";
/// The second voice in Motion's pictures (a stereo tremolo's other side).
pub const LIME: &str = "#a3e635";
use crate::rig_faces::PLUGIN;

/// `chorus::EngineType` order.
pub const MOD_ENGINES: [&str; 11] = [
    "Cubic",
    "BBD",
    "Tape",
    "Orbit",
    "Juno",
    "CE-2",
    "Dimension",
    "Clone",
    "Tri-Chorus",
    "SCF",
    "Julia",
];
/// `TremMode` order.
pub const TREM_MODES: [&str; 3] = ["Mono", "Stereo", "Harmonic"];

fn a(ns: &str, id: &str) -> String {
    format!("{ns}/{id}")
}

/// A choice the block holds as an index (0, 1, 2…): ranged 0..n−1 so the
/// block's value lands on its option (a bare choice is ranged 0..1).
fn indexed(addr: &str, name: &str, options: &[&str], default: usize) -> ParamInfo {
    let last = options.len().saturating_sub(1).max(1) as f64;
    let mut p = ParamInfo::new(addr, name, Range::linear(0.0, last, Unit::None)).choice(options);
    p.default = default as f64 / last;
    p
}

fn chorus_params() -> Vec<ParamInfo> {
    let n = MOD;
    vec![
        ui::param(&a(n, "rate"), "Rate", Range::log(0.05, 10.0, Unit::Hz), 1.0),
        ui::param(
            &a(n, "depth"),
            "Depth",
            Range::linear(0.0, 1.0, Unit::None),
            0.4,
        ),
        ui::param(
            &a(n, "mix"),
            "Mix",
            Range::linear(0.0, 1.0, Unit::None),
            0.4,
        ),
        indexed(&a(n, "engine"), "Engine", &MOD_ENGINES, 0),
        ui::param(
            &a(n, "color"),
            "Color",
            Range::linear(0.0, 1.0, Unit::None),
            0.5,
        ),
        ui::param(
            &a(n, "feedback"),
            "Feedback",
            Range::linear(0.0, 1.0, Unit::None),
            0.0,
        ),
        ui::param(
            &a(n, "width"),
            "Width",
            Range::linear(0.0, 1.0, Unit::None),
            1.0,
        ),
        ui::toggle(&a(n, "on"), "Bypass", true),
    ]
}

fn trem_params() -> Vec<ParamInfo> {
    let n = MOTION;
    vec![
        ui::param(&a(n, "rate"), "Rate", Range::log(0.05, 12.0, Unit::Hz), 4.0),
        ui::param(
            &a(n, "depth"),
            "Depth",
            Range::linear(0.0, 1.0, Unit::None),
            0.5,
        ),
        ui::param(
            &a(n, "mix"),
            "Mix",
            Range::linear(0.0, 1.0, Unit::None),
            1.0,
        ),
        indexed(&a(n, "mode"), "Mode", &TREM_MODES, 1),
        ui::toggle(&a(n, "on"), "Bypass", true),
    ]
}

fn vibrato_params() -> Vec<ParamInfo> {
    let n = MOTION;
    vec![
        ui::param(&a(n, "rate"), "Rate", Range::log(0.05, 10.0, Unit::Hz), 5.0),
        ui::param(
            &a(n, "depth"),
            "Depth",
            Range::linear(0.0, 1.0, Unit::None),
            0.4,
        ),
        ui::param(
            &a(n, "mix"),
            "Mix",
            Range::linear(0.0, 1.0, Unit::None),
            1.0,
        ),
        indexed(&a(n, "engine"), "Engine", &MOD_ENGINES, 0),
        ui::toggle(&a(n, "on"), "Bypass", true),
    ]
}

/// How a face is finished: its panel, its picture's window, its inks and
/// its knobs. Each effect has its own, so the chorus and the tremolo can't
/// be mistaken for each other.
struct Theme {
    panel: [(f64, &'static str); 2],
    edge: &'static str,
    window: [(f64, &'static str); 2],
    ink: &'static str,
    dim: &'static str,
    accent: &'static str,
    knob: &'static str,
    skirt: &'static str,
    bypass: &'static str,
}

/// A face's canvas: `build` draws into the panel, already `W` × `H`. The
/// top-right corner ([`CORNER`]) is left for Signal's switcher pill.
fn panel(
    name: &str,
    t: &Theme,
    build: impl FnOnce(&mut Builder<'_>, NodeId),
) -> frame::document::proto::Scene {
    let (fill, edge) = (t.panel, t.edge);
    canvas(name, move |b, root| {
        ui::fit_root(b, root, W, H);
        let mut p = draw::frame("Panel", 0.0, 0.0, W, H);
        p.fills = vec![draw::linear(180.0, &fill)];
        p.corner_radii = frame::document::proto::CornerRadii::uniform(8.0);
        let p = b.add(
            root,
            draw::with_effects(
                draw::stroked(p, edge, 1.0),
                vec![draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0)],
            ),
        );
        build(b, p);
        ui::flush(b, root, W, H);
    })
}

/// How a face lays out at a tier: where its picture's window sits, and
/// its knobs' size and line. A phone gives the picture less height and the
/// knobs more.
#[derive(Clone, Copy)]
struct Lay {
    view_y: f64,
    view_h: f64,
    knob_d: f64,
    knob_y: f64,
    small: bool,
}

impl Lay {
    fn at(tier: kit::Tier) -> Self {
        if matches!(tier, kit::Tier::Phone | kit::Tier::Strip) {
            Self {
                view_y: 46.0,
                view_h: 150.0,
                knob_d: 52.0,
                knob_y: 246.0,
                small: true,
            }
        } else {
            Self {
                view_y: 40.0,
                view_h: 206.0,
                knob_d: 34.0,
                knob_y: 280.0,
                small: false,
            }
        }
    }
}

/// The picture's window across the middle, clipped.
fn view(b: &mut Builder<'_>, parent: NodeId, t: &Theme, l: Lay) -> (NodeId, f64, f64) {
    let (x, y, w, h) = (10.0, l.view_y, W - 20.0, l.view_h);
    let mut v = draw::frame("View", x, y, w, h);
    v.fills = vec![draw::linear(180.0, &t.window)];
    v.clips_content = true;
    v.corner_radii = frame::document::proto::CornerRadii::uniform(6.0);
    v.effects = vec![draw::inner_shadow("#00000080", 0.0, 2.0, 6.0)];
    (b.add(parent, v), w, h)
}

/// The face's frame of controls: its On key and name, light, across the
/// top (the top-right corner is the host's, for the unit's preset), its
/// line of description, and the knobs spread along the bottom.
fn column(
    b: &mut Builder<'_>,
    p: NodeId,
    t: &Theme,
    ns: &str,
    name: &str,
    l: Lay,
    back: Option<bool>,
    list: &[(&str, &str)],
) {
    header(b, p, t, ns, name, l, back);
    let pointer = format!("pointer={}", t.accent);
    let color = format!("color={}", t.knob);
    let skirt = format!("skirt_color={}", t.skirt);
    let opts = [
        color.as_str(),
        pointer.as_str(),
        "index=line",
        "index_w=0.1",
        "skirt=plain",
        skirt.as_str(),
    ];
    let step = (W - 20.0) / list.len() as f64;
    for (i, item) in list.iter().enumerate() {
        row_item(
            b,
            p,
            (t, ns),
            *item,
            (10.0 + step * (i as f64 + 0.5), l.knob_y, l.knob_d),
            &opts,
        );
    }
}

/// One control of a face's row at (`cx`, `cy`), `d` across: a knob — or,
/// for a two-way switch (`fast`: a rotary speaker's speed), the switch, its
/// two settings side by side, the one it is on lit; its name under it as a
/// knob's is.
fn row_item(
    b: &mut Builder<'_>,
    p: NodeId,
    (t, ns): (&Theme, &str),
    (id, legend): (&str, &str),
    (cx, cy, d): (f64, f64, f64),
    opts: &[&str],
) {
    if id != "fast" {
        kit::knob3d(b, p, &a(ns, id), legend, d, (cx, cy), opts, t.ink, t.dim);
        return;
    }
    let (sw, sh) = (d * 2.0, (d * 0.62).round());
    let pill = b.add(
        p,
        draw::frame("speed switch", cx - sw / 2.0, cy - sh / 2.0, sw, sh),
    );
    b.add(
        pill,
        draw::with_effects(
            draw::rounded(
                draw::rect(
                    "well",
                    0.0,
                    0.0,
                    sw,
                    sh,
                    draw::solid(&ui::alpha(t.dim, 0.22)),
                ),
                sh / 2.0,
            ),
            vec![draw::inner_shadow("#00000066", 0.0, 1.0, 2.0)],
        ),
    );
    let size = (d * 0.2).clamp(7.0, 11.0);
    for (i, word) in ["Slow", "Fast"].iter().enumerate() {
        let x = i as f64 * sw / 2.0;
        let half = b.add(pill, draw::frame(word, x, 0.0, sw / 2.0, sh));
        let lit = b.add(
            half,
            draw::rounded(
                draw::rect(
                    "lit",
                    2.0,
                    2.0,
                    sw / 2.0 - 4.0,
                    sh - 4.0,
                    draw::solid(&ui::alpha(t.accent, 0.55)),
                ),
                (sh - 4.0) / 2.0,
            ),
        );
        let on = if i == 0 {
            format!("1 - param({ns}/fast)")
        } else {
            format!("param({ns}/fast)")
        };
        motion::add(
            b,
            lit,
            Motion::new(Target::Opacity)
                .input(on)
                .range(0.0, 1.0)
                .dynamics(Dynamics::spring(400.0, 36.0)),
        );
        ui::silk(b, half, word, sw / 4.0, sh / 2.0, size, t.ink, 800.0);
        let mode = frame::surface::proto::ButtonMode::Select {
            index: u32::try_from(i).unwrap_or(0),
            count: 2,
        };
        b.controls.insert(
            half,
            frame::surface::proto::Control::new(frame::surface::proto::ControlKind::Button {
                mode,
            })
            .bound(ui::bind(&a(ns, "fast"))),
        );
    }
    ui::silk(
        b,
        p,
        legend,
        cx,
        cy + d / 2.0 + 20.0,
        (d * 0.19).clamp(7.0, 10.0),
        t.ink,
        800.0,
    );
}

/// A face's row of controls: (id, legend) each.
type Row = &'static [(&'static str, &'static str)];
/// A face's rows: on a wide panel, and on a phone's lane.
type Rows = (Row, Row);

/// What a modulation face is: its name (as its nameplate prints it), its
/// finish, its namespace, its picture, and its file's name.
struct Spec {
    name: &'static str,
    t: &'static Theme,
    ns: &'static str,
    picture: fn(&mut Builder<'_>, NodeId, f64, f64, &Theme),
    base: &'static str,
}

/// `s`'s face at `tier`, its row `desk` on a wide panel (`phone` on a
/// phone's lane, after the Time knob); with a back (`flips`), the arrow
/// that turns it over.
fn front(
    s: &Spec,
    (desk, phone): Rows,
    flips: bool,
    tier: kit::Tier,
    params: Vec<ParamInfo>,
) -> Face {
    if matches!(tier, kit::Tier::Phone | kit::Tier::Strip | kit::Tier::Ipad) {
        return lane(
            tier,
            s.name,
            s.t,
            s.ns,
            s.picture,
            phone,
            flips,
            file_at(s.base, tier),
            params,
        );
    }
    let (t, l) = (s.t, Lay::at(tier));
    let scene = panel(s.name, t, |b, p| {
        let (v, vw, vh) = view(b, p, t, l);
        (s.picture)(b, v, vw, vh, t);
        column(b, p, t, s.ns, s.name, l, flips.then_some(false), desk);
        // Responsive: the picture takes the width; the column pins left.
        kit::responsive(b, p, &[v]);
    });
    let mut params = params;
    params.extend(chrome_params(s.ns, flips));
    Face {
        file: file_at(s.base, tier),
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// `s`'s back at `tier`: its panel, no picture — `desk` across it (`phone`
/// on a phone's lane), the head as on the front.
fn back(s: &Spec, (desk, phone): Rows, tier: kit::Tier, params: Vec<ParamInfo>) -> Face {
    let file: &'static str = Box::leak(format!("{}-back", s.base).into_boxed_str());
    if matches!(tier, kit::Tier::Phone | kit::Tier::Strip | kit::Tier::Ipad) {
        return lane_back(tier, s.name, s.t, s.ns, phone, file_at(file, tier), params);
    }
    let (t, l) = (s.t, Lay::at(tier));
    let scene = panel(s.name, t, |b, p| {
        ui::silk_left(
            b,
            p,
            "Advanced",
            14.0,
            l.view_y + 16.0,
            if l.small { 10.0 } else { 7.5 },
            t.dim,
            800.0,
        );
        let back = Lay {
            knob_y: l.view_y + (H - l.view_y) * 0.45,
            knob_d: l.knob_d * 1.15,
            ..l
        };
        column(b, p, t, s.ns, s.name, back, Some(true), desk);
    });
    let mut params = params;
    params.extend(chrome_params(s.ns, true));
    Face {
        file: file_at(file, tier),
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// Every modulation face's head, the same at each: its On key at the top
/// left and, where it has a back, the arrow that turns it over beside it
/// (`back`: on the back, `Some(true)`); the nameplate at the top right —
/// the preset, stepped ‹ ›, then the effect's name in the face's hand, a
/// press opening its engines (or modes).
fn header(
    b: &mut Builder<'_>,
    p: NodeId,
    t: &Theme,
    ns: &str,
    name: &str,
    l: Lay,
    back: Option<bool>,
) {
    let key = if l.small { 30.0 } else { 22.0 };
    kit::head(b, p, ns, back == Some(true), |b, head| {
        kit::bypass(
            b,
            head,
            ns,
            12.0 + key / 2.0,
            8.0 + key / 2.0,
            key,
            t.bypass,
            "On",
        );
    });
    if let Some(on_back) = back {
        let (x, w, h) = (12.0 + key + 6.0, if l.small { 50.0 } else { 44.0 }, key);
        let g = b.add(p, draw::frame("flip", x, 8.0, w, h));
        b.add(
            g,
            draw::rounded(
                draw::rect("well", 0.0, 0.0, w, h, draw::solid(&ui::alpha(t.dim, 0.25))),
                h / 2.0,
            ),
        );
        ui::silk(
            b,
            g,
            if on_back { "Back" } else { "More" },
            w / 2.0 - 5.0,
            h / 2.0,
            if l.small { 9.0 } else { 7.0 },
            t.ink,
            800.0,
        );
        let (cx, cy, dy) = (w - 9.0, h / 2.0, if on_back { -2.0 } else { 2.0 });
        ui::line(
            b,
            g,
            "chevron",
            &[(cx - 3.0, cy - dy), (cx, cy + dy), (cx + 3.0, cy - dy)],
            1.4,
            t.ink,
        );
        b.controls.insert(
            g,
            frame::surface::proto::Control::new(frame::surface::proto::ControlKind::Button {
                mode: frame::surface::proto::ButtonMode::Momentary,
            })
            .bound(ui::bind(&a(ns, "flip")))
            .shown_if(kit::standalone(ns)),
        );
    }
    let sans = draw::Lettering::family(draw::family::INTER).weight(300.0);
    let (h, size) = if l.small { (30.0, 11.0) } else { (24.0, 9.0) };
    let title_w = name.chars().count() as f64 * if l.small { 12.0 } else { 10.5 } + 18.0;
    let preset_w = 150.0;
    let x = W - 10.0 - preset_w - 6.0 - title_w;
    let fs = if l.small { 21.0 } else { 18.0 };
    kit::head(b, p, ns, back == Some(true), |b, head| {
        kit::nameplate(
            b,
            head,
            ns,
            (x, 8.0, h),
            title_w,
            preset_w,
            size,
            (t.accent, t.dim),
            |b, tb| {
                crate::suite_studio_blues::parts::set(
                    b,
                    tb,
                    name,
                    (title_w - 14.0) / 2.0,
                    h / 2.0,
                    fs,
                    t.ink,
                    &sans,
                    0.02,
                );
            },
        );
    });
}

/// A modulation face's file at `tier` (and its back's).
fn file_at(base: &'static str, tier: kit::Tier) -> &'static str {
    kit::tier_file(base, tier, false)
}

/// The params every modulation face adds to its effect's: the nameplate's
/// and, where it has a back, the arrow's.
fn chrome_params(ns: &str, flips: bool) -> Vec<ParamInfo> {
    let mut v = kit::nameplate_params(ns);
    if flips {
        v.extend(kit::flip_params(ns));
    }
    v
}

// ── Chorus ───────────────────────────────────────────────────────────────

pub const PINK: &str = "#ec4899";

/// Light and airy: an ice-blue panel, a bright window.
const CHORUS: Theme = Theme {
    panel: [(0.0, "#eaf6fd"), (1.0, "#cfe6f4")],
    edge: "#a9cde4",
    window: [(0.0, "#fbfeff"), (1.0, "#e2f2fb")],
    ink: "#12314a",
    dim: "#5d7d96",
    accent: "#0ea5e9",
    knob: "#f8fbfd",
    skirt: "#9cc3dc",
    bypass: "blue",
};

/// The chorus: the note's wave, dark, and its voices — copies of it in
/// blues, each drifting ahead and behind on the LFO (the depth how far,
/// the rate how fast), spread apart with the width and as strong as the
/// mix. The thickening you hear, as you see it: light through water.
fn chorus() -> Face {
    chorus_at(kit::Tier::Desktop)
}

fn chorus_at(tier: kit::Tier) -> Face {
    if tier.small() {
        return lane(
            tier,
            "Chorus",
            &CHORUS,
            MOD,
            chorus_picture,
            &[("depth", "Depth"), ("width", "Width"), ("mix", "Mix")],
            true,
            file_at("100-mod-chorus", tier),
            chorus_params(),
        );
    }
    let t = &CHORUS;
    let l = Lay::at(tier);
    let scene = panel("Chorus", t, |b, p| {
        let (v, vw, vh) = view(b, p, t, l);
        chorus_picture(b, v, vw, vh, t);
        let knobs: &[(&str, &str)] = if l.small {
            &[("rate", "Rate"), ("depth", "Depth"), ("mix", "Mix")]
        } else {
            &[
                ("rate", "Rate"),
                ("depth", "Depth"),
                ("width", "Width"),
                ("mix", "Mix"),
            ]
        };
        column(b, p, t, MOD, "Chorus", l, Some(false), knobs);
        // Responsive: the picture takes the width; the column pins left.
        kit::responsive(b, p, &[v]);
    });
    let mut params = chorus_params();
    params.extend(chrome_params(MOD, true));
    Face {
        file: file_at("100-mod-chorus", tier),
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The chorus's back: its panel, no picture — Width, Color and Feedback
/// across it, the head as on the front.
fn chorus_back(tier: kit::Tier) -> Face {
    if tier.small() {
        return lane_back(
            tier,
            "Chorus",
            &CHORUS,
            MOD,
            &[
                ("color", "Color"),
                ("feedback", "Feedback"),
                ("width", "Width"),
            ],
            file_at("100-mod-chorus-back", tier),
            chorus_params(),
        );
    }
    let t = &CHORUS;
    let l = Lay::at(tier);
    let scene = panel("Chorus", t, |b, p| {
        ui::silk_left(
            b,
            p,
            "Advanced",
            14.0,
            l.view_y + 16.0,
            if l.small { 10.0 } else { 7.5 },
            t.dim,
            800.0,
        );
        let back = Lay {
            knob_y: l.view_y + (H - l.view_y) * 0.45,
            knob_d: l.knob_d * 1.15,
            ..l
        };
        column(
            b,
            p,
            t,
            MOD,
            "Chorus",
            back,
            Some(true),
            &[
                ("width", "Width"),
                ("color", "Color"),
                ("feedback", "Feedback"),
            ],
        );
    });
    let mut params = chorus_params();
    params.extend(chrome_params(MOD, true));
    Face {
        file: file_at("100-mod-chorus-back", tier),
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

// ── Tremolo ──────────────────────────────────────────────────────────────

/// A neon sign on black brick.
const TREMOLO: Theme = Theme {
    panel: [(0.0, "#0a1a13"), (1.0, "#040c08")],
    edge: "#13382a",
    window: [(0.0, "#05120c"), (1.0, "#020704")],
    ink: "#e2fff1",
    dim: "#76ad93",
    accent: GREEN,
    knob: "#11241b",
    skirt: "#040c08",
    bypass: "green",
};

/// The tremolo, as neon: a ring that brightens and fades with the volume
/// now, and the volume's curve as a neon tube rolling out of it, as deep
/// as the depth and as fast as the rate. Stereo and Harmonic light a
/// second ring and tube against the first.
fn tremolo() -> Face {
    tremolo_at(kit::Tier::Desktop)
}

fn tremolo_at(tier: kit::Tier) -> Face {
    if tier.small() {
        return lane(
            tier,
            "Tremolo",
            &TREMOLO,
            MOTION,
            tremolo_picture,
            &[("depth", "Depth"), ("mix", "Mix")],
            false,
            file_at("101-motion-tremolo", tier),
            trem_params(),
        );
    }
    let t = &TREMOLO;
    let l = Lay::at(tier);
    let scene = panel("Tremolo", t, |b, p| {
        let (v, vw, vh) = view(b, p, t, l);
        tremolo_picture(b, v, vw, vh, t);
        column(
            b,
            p,
            t,
            MOTION,
            "Tremolo",
            l,
            None,
            &[("rate", "Rate"), ("depth", "Depth"), ("mix", "Mix")],
        );
        // Responsive: the picture takes the width; the column pins left.
        kit::responsive(b, p, &[v]);
    });
    let mut params = trem_params();
    params.extend(chrome_params(MOTION, false));
    Face {
        file: file_at("101-motion-tremolo", tier),
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

// ── Vibrato ──────────────────────────────────────────────────────────────

/// A tuner's dark green.
const VIBRATO: Theme = Theme {
    panel: [(0.0, "#0f1d17"), (1.0, "#08110d")],
    edge: "#1f3a2e",
    window: [(0.0, "#08100c"), (1.0, "#040806")],
    ink: "#e6fff3",
    dim: "#86ad9b",
    accent: GREEN,
    knob: "#162a21",
    skirt: "#08100b",
    bypass: "green",
};

/// The vibrato: pitch, as a tuner reads it — the needle swinging sharp and
/// flat about the note, as far as the depth and as fast as the rate — over
/// the wave it bends, its cycles crowding and spreading.
fn vibrato() -> Face {
    vibrato_at(kit::Tier::Desktop)
}

fn vibrato_at(tier: kit::Tier) -> Face {
    if tier.small() {
        return lane(
            tier,
            "Vibrato",
            &VIBRATO,
            MOTION,
            vibrato_picture,
            &[("depth", "Depth"), ("mix", "Mix")],
            false,
            file_at("102-motion-vibrato", tier),
            vibrato_params(),
        );
    }
    let t = &VIBRATO;
    let l = Lay::at(tier);
    let scene = panel("Vibrato", t, |b, p| {
        let (v, vw, vh) = view(b, p, t, l);
        vibrato_picture(b, v, vw, vh, t);
        column(
            b,
            p,
            t,
            MOTION,
            "Vibrato",
            l,
            None,
            &[("rate", "Rate"), ("depth", "Depth"), ("mix", "Mix")],
        );
        // Responsive: the picture takes the width; the column pins left.
        kit::responsive(b, p, &[v]);
    });
    let mut params = vibrato_params();
    params.extend(chrome_params(MOTION, false));
    Face {
        file: file_at("102-motion-vibrato", tier),
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The chorus's picture in its window `v` (`vw × vh`).
fn chorus_picture(b: &mut Builder<'_>, v: NodeId, vw: f64, vh: f64, t: &Theme) {
    let mid = vh / 2.0 + 6.0;
    let period = 70.0;
    let wave = |amp: f64| -> Vec<(f64, f64)> {
        (0..=160)
            .map(|k| {
                let x = -period + (vw + 2.0 * period) * f64::from(k) / 160.0;
                (x, mid - amp * (x / period * std::f64::consts::TAU).sin())
            })
            .collect()
    };
    // Soft caustics behind: light through water.
    for i in 0..5 {
        let f = f64::from(i);
        let c = b.add(
            v,
            draw::circle(
                "caustic",
                40.0 + f * 58.0,
                mid + 18.0 * (f * 1.7).sin(),
                38.0,
                draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#7dd3fc40"), (1.0, "#7dd3fc00")]),
            ),
        );
        motion::add(
            b,
            c,
            Motion::new(Target::X)
                .input(format!("0.5 + 0.5 * sin(time * 0.4 + {f})"))
                .range(-10.0, 10.0),
        );
    }
    // The voices: each its own blue.
    let voices = [
        (-1.0, 0.0, "#0ea5e9"),
        (1.0, 2.1, "#38bdf8"),
        (-0.5, 4.2, "#6366f1"),
        (0.5, 1.3, "#22d3ee"),
    ];
    for (i, (spread, phase, color)) in voices.iter().enumerate() {
        let g = b.add(v, draw::frame(&format!("voice {i}"), 0.0, 0.0, vw, vh));
        b.add(
            g,
            ui::strokes("voice", &[wave(30.0)], 2.2, draw::solid(color)),
        );
        let drift = format!("0.5 + 0.5 * sin(6.2832 * time * param({MOD}/rate) + {phase})");
        motion::add(
            b,
            g,
            Motion::new(Target::X)
                .input(format!(
                    "{drift} * norm({MOD}/depth) + 0.5 * (1 - norm({MOD}/depth))"
                ))
                .range(-16.0, 16.0),
        );
        motion::add(
            b,
            g,
            Motion::new(Target::Y)
                .input(format!("0.5 + 0.5 * {spread} * norm({MOD}/width)"))
                .range(-16.0, 16.0)
                .dynamics(Dynamics::spring(200.0, 26.0)),
        );
        motion::add(
            b,
            g,
            Motion::new(Target::Opacity)
                .input(format!(
                    "(0.25 + 0.75 * param({MOD}/mix)) * (0.2 + 0.8 * param({MOD}/on))"
                ))
                .range(0.0, 0.75),
        );
    }
    // The note, in front.
    b.add(
        v,
        ui::strokes("dry", &[wave(30.0)], 2.0, draw::solid(t.ink)),
    );
    ui::silk_left(b, v, "Dry", 10.0, 12.0, 5.5, t.ink, 800.0);
    ui::silk_left(b, v, "Voices", 30.0, 12.0, 5.5, t.accent, 800.0);
}

/// The tremolo's picture in its window `v` (`vw × vh`).
fn tremolo_picture(b: &mut Builder<'_>, v: NodeId, vw: f64, vh: f64, t: &Theme) {
    // Brick behind the sign, barely lit.
    let mut rows: Vec<Vec<(f64, f64)>> = (1..8)
        .map(|i| vec![(0.0, f64::from(i) * 19.0), (vw, f64::from(i) * 19.0)])
        .collect();
    for r in 0..8 {
        for k in 0..8 {
            let x = f64::from(k) * 40.0 + if r % 2 == 0 { 0.0 } else { 20.0 };
            rows.push(vec![(x, f64::from(r) * 19.0), (x, f64::from(r + 1) * 19.0)]);
        }
    }
    b.add(
        v,
        ui::strokes("brick", &rows, 1.0, draw::solid("#ffffff0a")),
    );
    // Full volume sits under the corner pill's height, clear of it.
    // The tube swings about the window's middle, as deep as the depth.
    let (lamp_x, top, bottom) = (0.0, vh * 0.14, vh * 0.86);
    let period = 64.0;
    let span = vw - lamp_x;
    let curve = |phase: f64| -> Vec<(f64, f64)> {
        (0..=360)
            .map(|k| {
                let x = (2.0 * span + period) * f64::from(k) / 360.0;
                let g = 0.5 + 0.5 * ((x / period + phase) * std::f64::consts::TAU).cos();
                (x, top + (bottom - top) * (1.0 - g))
            })
            .collect()
    };
    for (side, phase, color) in [(0_u8, 0.0, GREEN), (1, 0.5, LIME)] {
        let mut clip = draw::frame("tube window", lamp_x, 0.0, span, vh);
        clip.clips_content = true;
        clip.layout_child.resize_x = Some(frame::document::proto::ResizeX::Stretch);
        let clip = b.add(v, clip);
        // Twice the window long, so a widened window stays full.
        let roll = b.add(clip, draw::frame("tube", 0.0, 0.0, 2.0 * span + period, vh));
        // The tube: a hot core in a wide glow.
        b.add(
            roll,
            draw::emissive(
                ui::strokes(
                    "glow",
                    &[curve(phase)],
                    5.0,
                    draw::solid(&ui::alpha(color, 0.35)),
                ),
                draw::hex(color),
                0.9,
                10.0,
            ),
        );
        b.add(
            roll,
            ui::strokes("core", &[curve(phase)], 1.6, draw::solid("#ecfdf5")),
        );
        motion::add(
            b,
            roll,
            Motion::new(Target::X)
                .input(format!(
                    "fract(time * param({MOTION}/rate) * param({MOTION}/on))"
                ))
                .range(0.0, -period),
        );
        motion::add(
            b,
            roll,
            Motion::new(Target::StretchY)
                .input(format!("norm({MOTION}/depth)"))
                .range(0.02, 1.0)
                .pivot(0.5, 0.5)
                .dynamics(Dynamics::spring(220.0, 26.0)),
        );
        if side == 1 {
            motion::add(
                b,
                clip,
                Motion::new(Target::Opacity)
                    .input(format!("clamp(param({MOTION}/mode), 0, 1)"))
                    .range(0.0, 0.9),
            );
        }
    }
    ui::silk_left(b, v, "Volume", 8.0, vh - 8.0, 5.5, t.dim, 800.0);
}

/// The vibrato's picture in its window `v` (`vw × vh`).
fn vibrato_picture(b: &mut Builder<'_>, v: NodeId, vw: f64, vh: f64, t: &Theme) {
    let r = (vw * 0.62).min(130.0);
    let (cx, cy) = (vw / 2.0, vh + r * 0.26);
    let arc: Vec<(f64, f64)> = (0..=48)
        .map(|i| draw::polar(cx, cy, r, -40.0 + 80.0 * f64::from(i) / 48.0))
        .collect();
    b.add(
        v,
        ui::strokes("scale", &[arc], 1.0, draw::solid("#ffffff33")),
    );
    for i in -4..=4 {
        let ang = 10.0 * f64::from(i);
        let (len, col) = if i == 0 {
            (14.0, GREEN)
        } else {
            (7.0, "#ffffff4d")
        };
        b.add(
            v,
            ui::strokes(
                "tick",
                &[vec![
                    draw::polar(cx, cy, r, ang),
                    draw::polar(cx, cy, r - len, ang),
                ]],
                1.2,
                draw::solid(col),
            ),
        );
    }
    ui::silk(b, v, "Flat", cx - r * 0.72, cy - r * 0.6, 6.0, t.dim, 800.0);
    ui::silk(
        b,
        v,
        "Sharp",
        cx + r * 0.72,
        cy - r * 0.6,
        6.0,
        t.dim,
        800.0,
    );
    let side = 2.0 * (r + 6.0);
    let needle = b.add(
        v,
        draw::frame("needle", cx - side / 2.0, cy - side / 2.0, side, side),
    );
    b.add(
        needle,
        ui::glowing(
            draw::rect(
                "blade",
                side / 2.0 - 1.0,
                side / 2.0 - r + 2.0,
                2.0,
                r - 48.0,
                draw::solid(GREEN),
            ),
            GREEN,
            0.8,
            5.0,
        ),
    );
    let swing = format!(
        "0.5 + 0.5 * sin(6.2832 * time * param({MOTION}/rate)) * norm({MOTION}/depth) * param({MOTION}/on)"
    );
    motion::add(
        b,
        needle,
        Motion::new(Target::Rotate)
            .input(swing.clone())
            .range(-38.0, 38.0)
            .pivot(0.5, 0.5),
    );
    let wave: Vec<(f64, f64)> = (0..=160)
        .map(|k| {
            let x = vw * f64::from(k) / 160.0;
            (
                x,
                vh - 22.0 - 9.0 * (x / 18.0 * std::f64::consts::TAU).sin(),
            )
        })
        .collect();
    let w = b.add(v, draw::frame("wave", 0.0, 0.0, vw, vh));
    b.add(
        w,
        ui::strokes("wave", &[wave], 1.4, draw::solid(&ui::alpha(t.ink, 0.7))),
    );
    motion::add(
        b,
        w,
        Motion::new(Target::StretchX)
            .input(swing)
            .range(1.12, 0.88)
            .pivot(0.0, 0.5),
    );
}

/// A modulation face's knob look, in its theme.
fn knob_opts(t: &Theme) -> Vec<String> {
    vec![
        format!("color={}", t.knob),
        format!("pointer={}", t.accent),
        "index=line".into(),
        "index_w=0.1".into(),
        "skirt=plain".into(),
        format!("skirt_color={}", t.skirt),
    ]
}

/// A modulation face on a phone: a lane, as the Time faces are — so paging
/// between them nothing moves — the picture at the left, the Rate as the
/// Time knob (its TIME | BEAT locking the cycle to the tempo), `knobs` in
/// the knob row, the bypass and (`flips`) More at the right, the nameplate
/// at the top right. Its theme, its picture, its name.
#[allow(clippy::too_many_arguments)]
fn lane(
    tier: kit::Tier,
    name: &'static str,
    t: &'static Theme,
    ns: &'static str,
    picture: fn(&mut Builder<'_>, NodeId, f64, f64, &Theme),
    knobs: &[(&str, &str)],
    flips: bool,
    file: &'static str,
    mut params: Vec<ParamInfo>,
) -> Face {
    let w = tier.lane_w();
    let opts = knob_opts(t);
    let scene = kit::tier_lane(name, tier, |b, root| {
        let p = lane_panel(b, root, t, w);
        let (vx, vy, vw, vh) = (10.0, 10.0, kit::picture_r(tier) - 10.0, kit::H - 20.0);
        let mut v = draw::frame("View", vx, vy, vw, vh);
        v.fills = vec![draw::linear(180.0, &t.window)];
        v.clips_content = true;
        v.corner_radii = frame::document::proto::CornerRadii::uniform(6.0);
        v.effects = vec![draw::inner_shadow("#00000080", 0.0, 2.0, 6.0)];
        let v = b.add(p, v);
        picture(b, v, vw, vh, t);
        let o: Vec<&str> = opts.iter().map(String::as_str).collect();
        let (tx, ty, td) = kit::time_spot(tier);
        let ts = if tier == kit::Tier::Ipad { 10.0 } else { 11.0 };
        kit::time_knob(
            b,
            p,
            ns,
            (tx, ty, td),
            ts,
            (t.ink, t.dim, &ui::alpha(t.accent, 0.45)),
            |b, pp, addr| {
                kit::knob3d_bare(b, pp, addr, td, (tx, ty), &o);
            },
        );
        let (xs, ky, kd) = kit::knob_row(tier);
        for (i, item) in knobs.iter().enumerate().take(3) {
            row_item(b, p, (t, ns), *item, (xs[i], ky, kd), &o);
        }
        lane_head(b, p, tier, t, ns, name, flips, false);
        kit::responsive(b, p, &[v]);
    });
    params.extend(chrome_params(ns, flips));
    params.extend(kit::time_params(ns));
    Face {
        file,
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// A phone lane's panel in `t`'s finish.
fn lane_panel(b: &mut Builder<'_>, root: NodeId, t: &Theme, w: f64) -> NodeId {
    let mut p = draw::frame("Panel", 0.0, 0.0, w, kit::H);
    p.fills = vec![draw::linear(180.0, &t.panel)];
    p.corner_radii = frame::document::proto::CornerRadii::uniform(8.0);
    b.add(
        root,
        draw::with_effects(
            draw::stroked(p, t.edge, 1.0),
            vec![draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0)],
        ),
    )
}

/// A phone lane's nameplate, bypass and (`flips`) arrow, where every Time
/// face has them.
#[allow(clippy::too_many_arguments)]
fn lane_head(
    b: &mut Builder<'_>,
    p: NodeId,
    tier: kit::Tier,
    t: &Theme,
    ns: &str,
    name: &str,
    flips: bool,
    back: bool,
) {
    let sans = draw::Lettering::family(draw::family::INTER).weight(300.0);
    // On a pane: the nameplate smaller, as the knobs are closer.
    let (np_h, per_char, preset_w, size, title) = if tier == kit::Tier::Ipad {
        (26.0, 10.0, 150.0, 11.0, 18.0)
    } else {
        (30.0, 12.5, 210.0, 12.0, 22.0)
    };
    let title_w = name.chars().count() as f64 * per_char + 20.0;
    kit::head(b, p, ns, back, |b, head| {
        kit::nameplate_top_right(
            b,
            head,
            ns,
            tier,
            (8.0, np_h),
            title_w,
            preset_w,
            size,
            (t.accent, t.dim),
            |b, tb| {
                crate::suite_studio_blues::parts::set(
                    b,
                    tb,
                    name,
                    (title_w - 14.0) / 2.0,
                    np_h / 2.0,
                    title,
                    t.ink,
                    &sans,
                    0.02,
                );
            },
        );
        kit::face_bypass(b, head, ns, tier, t.bypass, "On");
    });
    if flips {
        kit::flip_arrow(b, p, ns, tier, back, (t.ink, &ui::alpha(t.dim, 0.25)));
    }
}

/// A modulation lane's back on a phone: its panel, no picture — `knobs`
/// across it, the head as on the front.
fn lane_back(
    tier: kit::Tier,
    name: &'static str,
    t: &'static Theme,
    ns: &'static str,
    knobs: &[(&str, &str)],
    file: &'static str,
    mut params: Vec<ParamInfo>,
) -> Face {
    let opts = knob_opts(t);
    let scene = kit::tier_lane(name, tier, |b, root| {
        let p = lane_panel(b, root, t, tier.lane_w());
        let o: Vec<&str> = opts.iter().map(String::as_str).collect();
        kit::back_knobs(b, p, tier, knobs, t.ink, |b, pp, id, legend, d, at| {
            kit::knob3d(b, pp, &a(ns, id), legend, d, at, &o, t.ink, t.dim);
        });
        lane_head(b, p, tier, t, ns, name, true, true);
    });
    params.extend(chrome_params(ns, true));
    Face {
        file,
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// Every modulation face.
#[must_use]
// ── Flanger, Phaser, Rotary ──────────────────────────────────────────────

pub fn faces() -> Vec<Face> {
    let p = kit::Tier::Phone;
    let pane = kit::Tier::Ipad;
    let mut v = vec![
        chorus(),
        tremolo(),
        vibrato(),
        chorus_at(p),
        tremolo_at(p),
        vibrato_at(p),
        chorus_back(kit::Tier::Desktop),
        chorus_back(p),
        chorus_at(pane),
        tremolo_at(pane),
        vibrato_at(pane),
        chorus_back(pane),
    ];
    for t in [kit::Tier::Desktop, p, pane] {
        v.extend([
            phaser::face_at(t),
            phaser::back_at(t),
            flanger::face_at(t),
            flanger::back_at(t),
            rotary::face_at(t),
        ]);
    }
    v
}

/// `faces.json`'s `modulation` entry: each block type's face.
#[must_use]
pub fn manifest() -> serde_json::Value {
    let size = [W, H];
    let (p, d) = (kit::Tier::Phone, kit::Tier::Desktop);
    // On a phone each is a lane (the Time faces' form); on a desktop pane,
    // a shorter one.
    let lane = [p.lane_w(), kit::H];
    let q = kit::Tier::Ipad;
    let pane = [q.lane_w(), kit::H];
    // Each prints its own nameplate; the chorus turns over to its back.
    json!([
        { "match": ["chorus"], "face": "100-mod-chorus", "ns": MOD, "size": size, "nameplate": true, "back": chorus_back(kit::Tier::Desktop).file,
          "tiers": { "phone": { "face": chorus_at(p).file, "size": lane, "back": chorus_back(p).file, "time_knob": true },
                     "ipad": { "face": chorus_at(q).file, "size": pane, "back": chorus_back(q).file, "time_knob": true } } },
        { "match": ["trem", "tremolo"], "face": "101-motion-tremolo", "ns": MOTION, "size": size, "nameplate": true,
          "tiers": { "phone": { "face": tremolo_at(p).file, "size": lane, "time_knob": true },
                     "ipad": { "face": tremolo_at(q).file, "size": pane, "time_knob": true } } },
        { "match": ["vibrato"], "face": "102-motion-vibrato", "ns": MOTION, "size": size, "nameplate": true,
          "tiers": { "phone": { "face": vibrato_at(p).file, "size": lane, "time_knob": true },
                     "ipad": { "face": vibrato_at(q).file, "size": pane, "time_knob": true } } },
        { "match": ["phaser"], "face": phaser::face_at(d).file, "ns": MOD, "size": size, "nameplate": true, "back": phaser::back_at(d).file,
          "tiers": { "phone": { "face": phaser::face_at(p).file, "size": lane, "back": phaser::back_at(p).file, "time_knob": true },
                     "ipad": { "face": phaser::face_at(q).file, "size": pane, "back": phaser::back_at(q).file, "time_knob": true } } },
        { "match": ["flanger"], "face": flanger::face_at(d).file, "ns": MOD, "size": size, "nameplate": true, "back": flanger::back_at(d).file,
          "tiers": { "phone": { "face": flanger::face_at(p).file, "size": lane, "back": flanger::back_at(p).file, "time_knob": true },
                     "ipad": { "face": flanger::face_at(q).file, "size": pane, "back": flanger::back_at(q).file, "time_knob": true } } },
        { "match": ["rotary"], "face": rotary::face_at(d).file, "ns": MOTION, "size": size, "nameplate": true,
          "tiers": { "phone": { "face": rotary::face_at(p).file, "size": lane, "time_knob": true },
                     "ipad": { "face": rotary::face_at(q).file, "size": pane, "time_knob": true } } },
    ])
}

/// The routing grid's blocks: the chorus's voices, the tremolo's tube, the
/// vibrato's needle, each filling its block.
#[must_use]
pub(crate) fn blocks() -> Vec<Face> {
    use crate::rig_cells::S;
    let cell = |name: &str,
                t: &'static Theme,
                picture: fn(&mut Builder<'_>, NodeId, f64, f64, &Theme),
                file: &'static str,
                params: Vec<ParamInfo>| {
        let scene = crate::rig_cells::block(name, &t.window, |b, v, _| picture(b, v, S, S, t));
        Face {
            file,
            plugin: PLUGIN,
            scene,
            params,
            streams: Vec::new(),
        }
    };
    vec![
        cell(
            "Chorus",
            &CHORUS,
            chorus_picture,
            "100-mod-chorus-block",
            chorus_params(),
        ),
        cell(
            "Tremolo",
            &TREMOLO,
            tremolo_picture,
            "101-motion-tremolo-block",
            trem_params(),
        ),
        cell(
            "Vibrato",
            &VIBRATO,
            vibrato_picture,
            "102-motion-vibrato-block",
            vibrato_params(),
        ),
    ]
}
