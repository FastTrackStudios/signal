//! What every time face shares: its size, its params (signal's Delay and
//! Reverb block params, in their own units), and the pieces most of them
//! draw — a live echo train, a live decay, the machine selector, bypass.
//!
//! The pictures move with the params through motion expressions: an echo
//! train's repeats sit at multiples of `dly/time` and fall by
//! `dly/feedback` each time; a decay stretches with `verb/decay`. Faces
//! are free to draw their own instead.

use document_proto::motion::{Dynamics, Motion, Target};
use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, draw, motion};
use surface_proto::{
    ButtonMode, Condition, Control, ControlKind, ParamInfo, Range, SlotRole, Unit,
};
use surface_widgets::Builder;

use crate::face::{Face, canvas};
use crate::reverb::ui::{self, Dial};
use crate::rig_faces::PLUGIN;

/// A lane of the Time module: every time face's size.
pub const W: f64 = 960.0;
pub const H: f64 = 164.0;

/// The corner signal keeps for its machine and division pickers (top
/// right, `CORNER.0` wide × `CORNER.1` tall): faces draw nothing there that
/// matters.
pub const CORNER: (f64, f64) = (170.0, 26.0);

/// The delay faces' namespace (signal's Delay block).
pub const DLY: &str = "dly";
/// The reverb faces' namespace (signal's Reverb block).
pub const VERB: &str = "verb";

// The section's colours, as Signal draws the Time module: every delay is
// blue and every reverb purple — a face's own finish can be anything, but
// its accents (lamps, displays, pointers, the bypass) are these, so a
// glance tells which section you're reading.
/// Delay blue (Signal's `DELAY_COLORS[0]`), a lighter tint, a deep shade,
/// and a lamp's glow.
pub const BLUE: &str = "#3b82f6";
pub const BLUE_LIGHT: &str = "#bcd6ff";
pub const BLUE_DEEP: &str = "#1e3a8a";
pub const BLUE_LAMP: &str = "#4aa0f4";
/// Reverb purple (Signal's `VERB_COLORS[0]`), likewise.
pub const PURPLE: &str = "#a78bfa";
pub const PURPLE_LIGHT: &str = "#ddd0ff";
pub const PURPLE_DEEP: &str = "#3b1d6e";
pub const PURPLE_LAMP: &str = "#b58cff";

// The section's light. Every Time face lights its picture the same way:
// what sounds (a trace, a tap, a wavefront, an LED) gives off light, and
// on the GPU it blooms; without the GPU it is a soft vector glow. Two
// radii only, so a face's glows share the stage's bloom passes (one per
// radius): a tight halo for a line or a point, a wide one for a source, a
// lamp or a field. The light is in the picture only — never over the
// whole panel.
/// The tight halo: a trace, a tap, an LED.
pub const GLOW: f64 = 6.0;
/// The wide one: a source, a lamp, a field.
pub const BLOOM: f64 = 16.0;

/// A GPU effect (`name`, with its `params`) over `id` and what it holds:
/// see the effect contract. Renderers without the stage draw it plainly.
pub fn effect(b: &mut Builder<'_>, id: NodeId, name: &str, params: &[(&str, f64)]) {
    if let Some(n) = b.scene.node_mut(id) {
        n.shader = Some(document_proto::ShaderFx {
            shader: name.into(),
            params: params.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
            inputs: Vec::new(),
        });
    }
}

/// The pane over a picture's well (`pane.wgsl`, the face's own effect):
/// flat glass — a sheen at its left end, a rim, its edges shaded — the
/// same over every Time face's picture. Flat, not a lens: a handle in the
/// picture stays where it is drawn.
pub fn glass(b: &mut Builder<'_>, well: NodeId) {
    let pane = b
        .scene
        .add_asset("text/wgsl", include_bytes!("pane.wgsl").to_vec());
    effect(
        b,
        well,
        &format!("asset:{}", pane.0),
        &[("strength", 0.6), ("amount", 0.5)],
    );
}

/// Brushed steel that catches the light (`steel.wgsl`) on `sheet`: its
/// paint's light bands flare in streaks across the brushing.
pub fn steel(b: &mut Builder<'_>, sheet: NodeId) {
    let steel = b
        .scene
        .add_asset("text/wgsl", include_bytes!("steel.wgsl").to_vec());
    effect(
        b,
        sheet,
        &format!("asset:{}", steel.0),
        &[("strength", 0.9), ("amount", 0.6)],
    );
}

/// The delay's machines, in the block's `style` order.
pub const DELAY_ALGOS: [&str; 13] = [
    "Tape", "Digital", "Analog", "Lo-Fi", "Shimmer", "Reverse", "Ice", "Rhythm", "Drum", "Oil Can",
    "MultiTap", "Spectral", "Filter",
];
/// The reverb's algorithms, in the block's `algorithm` order.
pub const VERB_ALGOS: [&str; 16] = [
    "Room",
    "Hall",
    "Plate",
    "Spring",
    "Cloud",
    "Bloom",
    "Shimmer",
    "Chorale",
    "Magneto",
    "NonLinear",
    "Swell",
    "Reflections",
    "Velvet",
    "FreeVerb",
    "Convolution",
    "Random",
];
/// A delay's tempo divisions, in the block's `tap_div_l` order.
pub const DIVS: [&str; 11] = [
    "1/4", "1/8.", "1/8", "1/8T", "1/16", "Golden", "Silver", "Free", "1/4.", "1/2", "1/4T",
];

fn a(ns: &str, id: &str) -> String {
    format!("{ns}/{id}")
}

/// The Delay block's params, as `dly/…`.
#[must_use]
pub fn delay_params() -> Vec<ParamInfo> {
    delay_params_in(DLY)
}

/// The Delay block's params under `n` (a Pre FX lane's own namespace).
#[must_use]
pub fn delay_params_in(n: &str) -> Vec<ParamInfo> {
    vec![
        ui::param(
            &a(n, "time"),
            "Time",
            Range::log(20.0, 2500.0, Unit::Milliseconds),
            350.0,
        ),
        ui::param(
            &a(n, "feedback"),
            "Feedback",
            Range::linear(0.0, 0.95, Unit::None),
            0.3,
        ),
        ui::param(
            &a(n, "level"),
            "Level",
            Range::linear(-60.0, 12.0, Unit::Db),
            -16.0,
        ),
        ui::choice(&a(n, "style"), "Machine", &DELAY_ALGOS, 1),
        ui::choice(&a(n, "tap_div_l"), "Division", &DIVS, 0),
        ui::param(
            &a(n, "high_pass"),
            "Low Cut",
            Range::linear(0.0, 900.0, Unit::Hz),
            0.0,
        ),
        ui::param(
            &a(n, "high_cut"),
            "High Cut",
            Range::log(500.0, 20_000.0, Unit::Hz),
            8000.0,
        ),
        ui::param(
            &a(n, "duck_sens"),
            "Duck",
            Range::linear(0.0, 18.0, Unit::Db),
            0.0,
        ),
        ui::param(
            &a(n, "duck_release"),
            "Duck Release",
            Range::log(0.05, 1.0, Unit::Seconds),
            0.2,
        ),
        ui::param(
            &a(n, "mod_rate"),
            "Mod Rate",
            Range::log(0.05, 8.0, Unit::Hz),
            0.6,
        ),
        ui::param(
            &a(n, "mod_depth"),
            "Mod Depth",
            Range::linear(0.0, 1.0, Unit::None),
            0.0,
        ),
        ui::param(
            &a(n, "pan"),
            "Pan",
            Range::linear(-1.0, 1.0, Unit::None),
            0.0,
        ),
        ui::param(
            &a(n, "dry"),
            "Dry",
            Range::linear(0.0, 1.0, Unit::None),
            1.0,
        ),
        ui::param(
            &a(n, "repeat_dyn"),
            "Repeat Dynamics",
            Range::linear(0.0, 1.0, Unit::None),
            0.0,
        ),
        ui::param(
            &a(n, "interval"),
            "Interval",
            Range::linear(0.0, 30.0, Unit::None),
            30.0,
        ),
        ui::param(
            &a(n, "blend"),
            "Blend",
            Range::linear(0.0, 1.0, Unit::None),
            1.0,
        ),
        ui::toggle(&a(n, "on"), "Bypass", true),
    ]
}

/// The Reverb block's params, as `verb/…`.
#[must_use]
pub fn reverb_params() -> Vec<ParamInfo> {
    reverb_params_in(VERB)
}

/// The Reverb block's params under `n` (a Pre FX lane's own namespace).
#[must_use]
pub fn reverb_params_in(n: &str) -> Vec<ParamInfo> {
    vec![
        ui::param(
            &a(n, "decay"),
            "Decay",
            Range::linear(0.0, 1.0, Unit::None),
            0.4,
        ),
        ui::param(
            &a(n, "size"),
            "Size",
            Range::linear(0.0, 1.0, Unit::None),
            0.5,
        ),
        ui::param(
            &a(n, "level"),
            "Level",
            Range::linear(-60.0, 12.0, Unit::Db),
            -16.0,
        ),
        ui::choice(&a(n, "algorithm"), "Algorithm", &VERB_ALGOS, 1),
        ui::param(
            &a(n, "predelay"),
            "Pre-Delay",
            Range::linear(0.0, 200.0, Unit::Milliseconds),
            0.0,
        ),
        ui::param(
            &a(n, "damping"),
            "Damping",
            Range::linear(0.0, 1.0, Unit::None),
            0.3,
        ),
        ui::param(
            &a(n, "tone"),
            "Tone",
            Range::linear(-1.0, 1.0, Unit::None),
            0.0,
        ),
        ui::param(
            &a(n, "modulation"),
            "Modulation",
            Range::linear(0.0, 1.0, Unit::None),
            0.2,
        ),
        ui::param(
            &a(n, "low_cut"),
            "Low Cut",
            Range::log(20.0, 2000.0, Unit::Hz),
            20.0,
        ),
        ui::param(
            &a(n, "high_cut"),
            "High Cut",
            Range::log(1000.0, 20_000.0, Unit::Hz),
            20_000.0,
        ),
        ui::param(
            &a(n, "duck"),
            "Duck",
            Range::linear(0.0, 1.0, Unit::None),
            0.0,
        ),
        ui::param(
            &a(n, "duck_threshold"),
            "Duck Threshold",
            Range::linear(-60.0, 0.0, Unit::Db),
            -20.0,
        ),
        ui::param(
            &a(n, "duck_release"),
            "Duck Release",
            Range::log(20.0, 2000.0, Unit::Milliseconds),
            120.0,
        ),
        ui::param(
            &a(n, "pan_a"),
            "Pan",
            Range::linear(-1.0, 1.0, Unit::None),
            0.0,
        ),
        ui::param(
            &a(n, "dry"),
            "Dry",
            Range::linear(0.0, 1.0, Unit::None),
            1.0,
        ),
        ui::param(
            &a(n, "shim_shift1"),
            "Shift",
            Range::linear(-12.0, 12.0, Unit::None),
            12.0,
        ),
        ui::param(
            &a(n, "shim_amount"),
            "Shimmer",
            Range::linear(0.0, 1.0, Unit::None),
            0.35,
        ),
        ui::toggle(&a(n, "on"), "Bypass", true),
    ]
}

/// A delay face: its scene under `file`, with the delay's params.
#[must_use]
pub fn delay_face(file: &'static str, scene: document_proto::Scene) -> Face {
    Face {
        file,
        plugin: PLUGIN,
        scene,
        params: delay_params(),
        streams: Vec::new(),
    }
}

/// A reverb face: its scene under `file`, with the reverb's params.
#[must_use]
pub fn reverb_face(file: &'static str, scene: document_proto::Scene) -> Face {
    Face {
        file,
        plugin: PLUGIN,
        scene,
        params: reverb_params(),
        streams: Vec::new(),
    }
}

/// A face's file: `base` for its light finish, `base-dark` for its dark.
#[must_use]
pub fn file(base: &'static str, dark: bool) -> &'static str {
    if dark {
        Box::leak(format!("{base}-dark").into_boxed_str())
    } else {
        base
    }
}

/// The room a face is drawn in, smallest first: a face is one design
/// (one builder, its look written once) laid out for each tier it has — the
/// app draws the version for the room it is given, whatever the device.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    /// A channel strip's width.
    Strip,
    /// A phone held sideways: a lane about 760 × 150 points.
    Phone,
    /// A pane of the desktop's grid (a MacBook's, the module sidebar
    /// open): a lane about 450 × 145 points — the phone's controls, closer
    /// together, on a lane [`PANE_W`] wide.
    Ipad,
    /// The base layout.
    Desktop,
    Fullscreen,
}

impl Tier {
    /// Its name in the manifest's `tiers`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Strip => "strip",
            Self::Phone => "phone",
            Self::Ipad => "ipad",
            Self::Desktop => "desktop",
            Self::Fullscreen => "fullscreen",
        }
    }

    /// A lane's width at this tier (its height is always `H`).
    #[must_use]
    pub const fn lane_w(self) -> f64 {
        match self {
            Self::Strip | Self::Phone => 760.0,
            Self::Ipad => PANE_W,
            Self::Desktop | Self::Fullscreen => W,
        }
    }

    /// A small room: the controls at a finger's size, the picture slim
    /// (a phone's lane, a desktop pane's).
    #[must_use]
    pub const fn small(self) -> bool {
        matches!(self, Self::Strip | Self::Phone | Self::Ipad)
    }
}

/// A desktop pane's lane width (`Tier::Ipad`): 3.1 : 1 at `H`, a MacBook
/// grid's lane — drawn at the room's own shape, since a face laid out in a
/// squarer box than it was drawn for gives the difference to its picture.
/// The picture's room comes from a mouse-sized TIME | BEAT and smaller
/// knobs.
pub const PANE_W: f64 = 510.0;

/// [`PHONE_TIME`] at `tier`: the Time knob's centre and size.
#[must_use]
pub const fn time_spot(tier: Tier) -> (f64, f64, f64) {
    match tier {
        Tier::Ipad => (186.0, 54.0, 58.0),
        _ => PHONE_TIME,
    }
}

/// [`PHONE_KNOBS`] at `tier`: up to three knobs in a row (centres, their
/// line, size).
#[must_use]
pub const fn knob_row(tier: Tier) -> ([f64; 3], f64, f64) {
    match tier {
        Tier::Ipad => ([258.0, 312.0, 366.0], 80.0, 44.0),
        _ => PHONE_KNOBS,
    }
}

/// [`PHONE_PICTURE_R`] at `tier`: where the picture stops.
#[must_use]
pub const fn picture_r(tier: Tier) -> f64 {
    match tier {
        Tier::Ipad => 144.0,
        _ => PHONE_PICTURE_R,
    }
}

/// The knob legends' size at a small `tier`.
#[must_use]
pub const fn legend_size(tier: Tier) -> f64 {
    match tier {
        Tier::Ipad => 8.5,
        _ => 10.5,
    }
}

/// The nameplate's (y, h, title_w, preset_w, size) in a small room.
#[must_use]
pub const fn small_nameplate(tier: Tier) -> (f64, f64, f64, f64, f64) {
    match tier {
        Tier::Ipad => (8.0, 24.0, 62.0, 136.0, 10.5),
        _ => (8.0, 30.0, 74.0, 210.0, 12.0),
    }
}

/// Where every Time face puts its controls on a phone, in lane
/// coordinates, so paging from one to the next nothing moves: the picture
/// keeps to the left of `PHONE_PICTURE_R`, the Time knob (centre, size),
/// and up to three knobs in a row (centres, their line, size).
pub const PHONE_PICTURE_R: f64 = 190.0;
/// The Time knob centred between the picture and the knob row.
pub const PHONE_TIME: (f64, f64, f64) = (280.0, 58.0, 74.0);
pub const PHONE_KNOBS: ([f64; 3], f64, f64) = ([400.0, 505.0, 610.0], 82.0, 58.0);

/// A face's file at `tier`: `base` (`-dark`) at the base tier, else
/// `base-<tier>` (`-dark`).
#[must_use]
pub fn tier_file(base: &'static str, tier: Tier, dark: bool) -> &'static str {
    if tier == Tier::Desktop {
        return file(base, dark);
    }
    let t = tier.name();
    Box::leak(
        if dark {
            format!("{base}-{t}-dark")
        } else {
            format!("{base}-{t}")
        }
        .into_boxed_str(),
    )
}

/// A lane's canvas at `tier`: `build` draws into the root, already
/// `tier.lane_w()` × `H`.
pub fn tier_lane(
    name: &str,
    tier: Tier,
    build: impl FnOnce(&mut Builder<'_>, NodeId),
) -> document_proto::Scene {
    let w = tier.lane_w();
    canvas(name, |b, root| {
        ui::fit_root(b, root, w, H);
        build(b, root);
        ui::flush(b, root, w, H);
    })
}

/// A lane's canvas: `build` draws into the root, already `W` × `H`.
pub fn lane(name: &str, build: impl FnOnce(&mut Builder<'_>, NodeId)) -> document_proto::Scene {
    canvas(name, |b, root| {
        ui::fit_root(b, root, W, H);
        build(b, root);
        ui::flush(b, root, W, H);
    })
}

/// The nameplate every Time face carries in the same place — the top
/// right, one row `h` tall, its right end 12 in from the face's edge, `y`
/// down (see [`nameplate_top_right`]): at the right the algorithm's name,
/// lettered as the face likes (`title` draws it into a `title_w` × `h`
/// box), a press on it opening the algorithm menu; left of it the preset
/// playing (`preset_w` wide: its name, then ‹ › to step it), a press on
/// its name opening the presets. The host fills `<ns>/preset` and answers the presses
/// ([`nameplate_params`]).
#[allow(clippy::too_many_arguments)]
pub fn nameplate(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    (x, y, h): (f64, f64, f64),
    title_w: f64,
    preset_w: f64,
    size: f64,
    (accent, dim): (&str, &str),
    title: impl FnOnce(&mut Builder<'_>, NodeId),
) {
    let press = |b: &mut Builder<'_>, node: NodeId, id: &str| {
        b.controls.insert(
            node,
            Control::new(ControlKind::Button {
                mode: ButtonMode::Momentary,
            })
            .bound(ui::bind(&a(ns, id))),
        );
    };
    // The preset first: its name, set against the arrows (names differ in
    // length; the row stays together at the right), then ‹ ›.
    let arrow = size + 6.0;
    let nw = preset_w - 2.0 * arrow;
    let field = b.add(parent, draw::frame("preset", x, y, nw, h));
    let mut name = draw::text(
        "value",
        "",
        0.0,
        (h - size * 1.3) / 2.0,
        nw,
        size,
        accent,
        TextAlign::Right,
    );
    if let document_proto::NodeKind::Text(d) = &mut name.kind {
        d.font_family = Some(draw::family::INTER.into());
        d.font_weight = Some(650.0);
    }
    let name = b.add(field, name);
    b.controls.insert(
        field,
        Control::new(ControlKind::Readout)
            .slot(SlotRole::Value, name.0)
            .bound(ui::bind(&a(ns, "preset"))),
    );
    // Over the name, a press opens the presets.
    let open = b.add(parent, draw::frame("preset browse", x, y, nw, h));
    press(b, open, "preset_browse");
    let prev = b.add(parent, draw::frame("preset prev", x + nw, y, arrow, h));
    ui::line(
        b,
        prev,
        "chevron",
        &[
            (arrow / 2.0 + 2.0, h / 2.0 - 4.0),
            (arrow / 2.0 - 2.0, h / 2.0),
            (arrow / 2.0 + 2.0, h / 2.0 + 4.0),
        ],
        1.4,
        dim,
    );
    press(b, prev, "preset_prev");
    let next = b.add(
        parent,
        draw::frame("preset next", x + nw + arrow, y, arrow, h),
    );
    ui::line(
        b,
        next,
        "chevron",
        &[
            (arrow / 2.0 - 2.0, h / 2.0 - 4.0),
            (arrow / 2.0 + 2.0, h / 2.0),
            (arrow / 2.0 - 2.0, h / 2.0 + 4.0),
        ],
        1.4,
        dim,
    );
    press(b, next, "preset_next");
    // Then the algorithm, at the right: its name, and a caret saying it
    // opens.
    let t = b.add(
        parent,
        draw::frame("algorithm", x + preset_w + 6.0, y, title_w, h),
    );
    title(b, t);
    let (cx, cy) = (title_w - 6.0, h / 2.0);
    ui::line(
        b,
        t,
        "caret",
        &[(cx - 4.0, cy - 2.0), (cx, cy + 2.0), (cx + 4.0, cy - 2.0)],
        1.4,
        dim,
    );
    press(b, t, "algo_menu");
}

/// [`nameplate`] in its standard place: the top right of a face at
/// `tier`, `y` down.
#[allow(clippy::too_many_arguments)]
pub fn nameplate_top_right(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    tier: Tier,
    (y, h): (f64, f64),
    title_w: f64,
    preset_w: f64,
    size: f64,
    inks: (&str, &str),
    title: impl FnOnce(&mut Builder<'_>, NodeId),
) {
    nameplate_right(
        b,
        parent,
        ns,
        tier.lane_w(),
        (y, h),
        title_w,
        preset_w,
        size,
        inks,
        title,
    );
}

/// [`nameplate_top_right`] on a lane `w` wide (a Pre FX lane's 720).
#[allow(clippy::too_many_arguments)]
pub fn nameplate_right(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    w: f64,
    (y, h): (f64, f64),
    title_w: f64,
    preset_w: f64,
    size: f64,
    inks: (&str, &str),
    title: impl FnOnce(&mut Builder<'_>, NodeId),
) {
    let x = w - 12.0 - preset_w - 6.0 - title_w;
    nameplate(
        b,
        parent,
        ns,
        (x, y, h),
        title_w,
        preset_w,
        size,
        inks,
        title,
    );
}

/// The Time knob every Time face has, the same at every face: the face's
/// own knob on `<ns>/time_knob` (`knob` draws it, bare, at (`cx`, `cy`), `d`
/// across), the time read out under it ("2.2 s", "1 bar · 2.0 s",
/// "540 ms", "1/8. · 375 ms" — the host's `<ns>/time_text`), and for its
/// legend the TIME | BEAT switch on `<ns>/sync`: free, or locked to the
/// tempo. The host maps it onto the block either way (not the block's own `time`,
/// which the pictures animate from).
#[allow(clippy::too_many_arguments)]
pub fn time_knob(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    (cx, cy, d): (f64, f64, f64),
    size: f64,
    (ink, dim, lit): (&str, &str, &str),
    knob: impl FnOnce(&mut Builder<'_>, NodeId, &str),
) {
    knob(b, parent, &a(ns, "time_knob"));
    // The time, under the knob.
    let (rw, rh) = (d + 70.0, (size * 1.35).ceil());
    let ry = cy + d / 2.0 + 3.0;
    let field = b.add(
        parent,
        draw::frame("time readout", cx - rw / 2.0, ry, rw, rh),
    );
    let mut t = draw::text("value", "", 0.0, 0.0, rw, size, ink, TextAlign::Center);
    if let document_proto::NodeKind::Text(td) = &mut t.kind {
        td.font_family = Some(draw::family::INTER.into());
        td.font_weight = Some(650.0);
    }
    let t = b.add(field, t);
    b.controls.insert(
        field,
        Control::new(ControlKind::Readout)
            .slot(SlotRole::Value, t.0)
            .bound(ui::bind(&a(ns, "time_text"))),
    );
    // TIME | BEAT, the side it is on lit; each side its own target (a
    // press picks it), at a finger's size where the face is for one.
    // A finger's size from 10.5 up (a phone); a desktop pane's is a mouse's.
    let touch = size >= 10.5;
    let (sw, sh) = if touch {
        (120.0, 28.0)
    } else {
        ((size * 5.4).max(70.0), (size * 0.8 + 8.0).ceil())
    };
    let sy = ry + rh + 2.0;
    let pill = b.add(
        parent,
        draw::frame("time switch", cx - sw / 2.0, sy, sw, sh),
    );
    b.add(
        pill,
        draw::rounded(
            draw::rect("well", 0.0, 0.0, sw, sh, draw::solid(&ui::alpha(dim, 0.22))),
            sh / 2.0,
        ),
    );
    for (i, (word, on)) in [
        ("TIME", format!("1 - param({ns}/sync)")),
        ("BEAT", format!("param({ns}/sync)")),
    ]
    .iter()
    .enumerate()
    {
        let x = i as f64 * sw / 2.0;
        let half = b.add(pill, draw::frame(word, x, 0.0, sw / 2.0, sh));
        let glow = b.add(
            half,
            draw::rounded(
                draw::rect("lit", 2.0, 2.0, sw / 2.0 - 4.0, sh - 4.0, draw::solid(lit)),
                (sh - 4.0) / 2.0,
            ),
        );
        motion::add(
            b,
            glow,
            Motion::new(Target::Opacity)
                .input(on.clone())
                .range(0.0, 1.0)
                .dynamics(Dynamics::spring(400.0, 36.0)),
        );
        ui::silk(
            b,
            half,
            word,
            sw / 4.0,
            sh / 2.0,
            if touch { 10.5 } else { size * 0.8 },
            ink,
            800.0,
        );
        b.controls.insert(
            half,
            Control::new(ControlKind::Button {
                mode: ButtonMode::Select {
                    index: i as u32,
                    count: 2,
                },
            })
            .bound(ui::bind(&a(ns, "sync"))),
        );
    }
}

/// A `knob.3d` with no value or legend of its own (the [`time_knob`]
/// prints them), in the kit's options.
pub fn knob3d_bare(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    d: f64,
    (cx, cy): (f64, f64),
    options: &[&str],
) -> NodeId {
    let mut opts = vec!["touch=44", "ticks=none"];
    opts.extend_from_slice(options);
    let p = Place::new("Time")
        .legend(" ")
        .size(d)
        .variant("flat")
        .options(&opts)
        .bind(ui::bind(addr));
    let k = ui::put(b, parent, "knob.3d", &p, cx, cy);
    ui::strip_legend(b, k);
    k
}

/// The params a [`time_knob`] binds: its position, the switch, and the
/// time the host reads out.
#[must_use]
pub fn time_params(ns: &str) -> Vec<ParamInfo> {
    vec![
        ui::param(
            &a(ns, "time_knob"),
            "Time",
            Range::linear(0.0, 1.0, Unit::None),
            0.5,
        )
        .featured(),
        ui::toggle(&a(ns, "sync"), "Beat", false),
        ui::param(
            &a(ns, "time_text"),
            "Time",
            Range::linear(0.0, 1.0, Unit::None),
            0.0,
        )
        .text(),
    ]
}

/// Every Time face's bypass in the same place: on the right edge, at mid
/// height, the same size at a tier (a finger's 44 points on a phone) —
/// whatever the face, it is there to hit.
pub fn face_bypass(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    tier: Tier,
    variant: &str,
    legend: &str,
) -> NodeId {
    face_bypass_w(b, parent, ns, tier, tier.lane_w(), variant, legend)
}

/// [`face_bypass`] on a lane `w` wide.
pub fn face_bypass_w(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    tier: Tier,
    w: f64,
    variant: &str,
    legend: &str,
) -> NodeId {
    let (cx, cy, d) = bypass_spot_w(tier, w);
    bypass(b, parent, ns, cx, cy, d, variant, legend)
}

/// Where a Time face's bypass goes at `tier` — its centre and size — for
/// a face whose own key sits there in its own style (the tape echo's
/// power key).
#[must_use]
pub fn bypass_spot(tier: Tier) -> (f64, f64, f64) {
    bypass_spot_w(tier, tier.lane_w())
}

/// [`bypass_spot`] on a lane `w` wide.
#[must_use]
pub fn bypass_spot_w(tier: Tier, w: f64) -> (f64, f64, f64) {
    // In from the edge far enough for the flip arrow beside it.
    let (d, inset) = match tier {
        Tier::Phone | Tier::Strip => (44.0, 78.0),
        Tier::Ipad => (36.0, 64.0),
        _ => (28.0, 56.0),
    };
    (w - inset, H * 0.56, d)
}

/// The arrow that turns a Time face over: right of the bypass, the same at
/// every face — on the front "More" pointing down (its back carries the
/// advanced controls), on the back "Back" pointing up. A press on
/// `<ns>/flip`; the host shows the other side.
pub fn flip_arrow(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    tier: Tier,
    back: bool,
    inks: (&str, &str),
) -> NodeId {
    flip_arrow_w(b, parent, ns, tier, tier.lane_w(), back, inks)
}

/// [`flip_arrow`] on a lane `lane_w` wide.
pub fn flip_arrow_w(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    tier: Tier,
    lane_w: f64,
    back: bool,
    inks: (&str, &str),
) -> NodeId {
    let (bx, by, bd) = bypass_spot_w(tier, lane_w);
    let small_room = tier.small();
    let size = if small_room { 9.0 } else { 6.5 };
    // Right of the bypass, as tall: its word over a chevron.
    let x = bx + bd / 2.0 + 6.0;
    flip_arrow_at(
        b,
        parent,
        ns,
        (x, by - bd / 2.0, lane_w - 8.0 - x, bd),
        back,
        size,
        inks,
    )
}

/// The flip arrow in a box of its own (`x`, `y`, `w`, `h`), lettered at
/// `size` — for a face whose room is not a lane (the gate's pane).
#[allow(clippy::too_many_arguments)]
pub fn flip_arrow_at(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    back: bool,
    size: f64,
    (ink, well): (&str, &str),
) -> NodeId {
    let g = b.add(parent, draw::frame("flip", x, y, w, h));
    b.add(
        g,
        draw::rounded(draw::rect("well", 0.0, 0.0, w, h, draw::solid(well)), 6.0),
    );
    ui::silk(
        b,
        g,
        if back { "Back" } else { "More" },
        w / 2.0,
        h * 0.34,
        size,
        ink,
        800.0,
    );
    let (cx, cy) = (w / 2.0, h * 0.68);
    let dy = if back { -2.5 } else { 2.5 };
    ui::line(
        b,
        g,
        "chevron",
        &[(cx - 4.0, cy - dy), (cx, cy + dy), (cx + 4.0, cy - dy)],
        1.5,
        ink,
    );
    // Stacked, the host shows both sides at once: nothing to turn over.
    b.controls.insert(
        g,
        Control::new(ControlKind::Button {
            mode: ButtonMode::Momentary,
        })
        .bound(ui::bind(&a(ns, "flip")))
        .shown_if(standalone(ns)),
    );
    g
}

/// While a face stands alone: `<ns>/stacked` below ½. The host sets it to 1
/// when it shows the front with its back directly under it, in one column —
/// then the front's More arrow and the back's head (nameplate, Back arrow)
/// are not drawn and take no presses: the back is just its controls.
#[must_use]
pub fn standalone(ns: &str) -> Condition {
    Condition::on(a(ns, "stacked")).negated()
}

/// A face's head — its nameplate and what sits with it (on a back, its
/// bypass too: the front's is right above it) — built by `build` into
/// `parent`; on a back (`back`) into a layer over the whole face that is
/// shown only while the face stands alone ([`standalone`]), so stacked
/// under its front the back does not repeat it. Returns where it was
/// built.
pub fn head(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    back: bool,
    build: impl FnOnce(&mut Builder<'_>, NodeId),
) -> NodeId {
    if !back {
        build(b, parent);
        return parent;
    }
    let (w, h) = b
        .scene
        .node(parent)
        .map_or((W, H), |n| (n.size.width, n.size.height));
    let layer = b.add(parent, draw::frame("standalone head", 0.0, 0.0, w, h));
    // A layer carries its condition on a control: a readout of the switch
    // itself, its text never seen.
    let mut seen = draw::text(
        "stacked",
        "",
        0.0,
        0.0,
        1.0,
        1.0,
        "#00000000",
        TextAlign::Left,
    );
    seen.opacity = 0.0;
    let seen = b.add(layer, seen);
    b.controls.insert(
        layer,
        Control::new(ControlKind::Readout)
            .slot(SlotRole::Value, seen.0)
            .bound(ui::bind(&a(ns, "stacked")))
            .shown_if(standalone(ns)),
    );
    build(b, layer);
    layer
}

/// A face's back, in its own finish: "Advanced" at the top left, a row of
/// `knobs` (id, legend) each drawn by `knob` at (cx, cy) `d` across — up to
/// seven at a finger's size on a phone, as many as fit elsewhere — across
/// the whole face (no picture on the back) up to the bypass spot. The caller adds the nameplate, the bypass and the flip
/// arrow, so they sit where they do on the front.
pub fn back_knobs(
    b: &mut Builder<'_>,
    parent: NodeId,
    tier: Tier,
    knobs: &[(&str, &str)],
    ink: &str,
    knob: impl FnMut(&mut Builder<'_>, NodeId, &str, &str, f64, (f64, f64)),
) {
    back_knobs_w(b, parent, tier, tier.lane_w(), knobs, ink, knob);
}

/// [`back_knobs`] on a lane `w` wide.
pub fn back_knobs_w(
    b: &mut Builder<'_>,
    parent: NodeId,
    tier: Tier,
    w: f64,
    knobs: &[(&str, &str)],
    ink: &str,
    mut knob: impl FnMut(&mut Builder<'_>, NodeId, &str, &str, f64, (f64, f64)),
) {
    let small_room = tier.small();
    ui::silk_left(
        b,
        parent,
        "Advanced",
        16.0,
        20.0,
        if small_room { 10.0 } else { 7.5 },
        ink,
        800.0,
    );
    let (bx, _, bd) = bypass_spot_w(tier, w);
    let (d, cy) = match tier {
        Tier::Ipad => (42.0, 84.0),
        _ if small_room => (50.0, 84.0),
        _ => (36.0, 88.0),
    };
    let shown = match tier {
        Tier::Ipad => knobs.len().min(5),
        _ if small_room => knobs.len().min(7),
        _ => knobs.len(),
    };
    let (x0, x1) = (16.0 + d / 2.0 + 10.0, bx - bd / 2.0 - d / 2.0 - 18.0);
    let step = if shown > 1 {
        (x1 - x0) / (shown - 1) as f64
    } else {
        0.0
    };
    for (i, (id, legend)) in knobs.iter().take(shown).enumerate() {
        knob(b, parent, id, legend, d, (x0 + step * i as f64, cy));
    }
}

/// The params a [`flip_arrow`] binds: the press, and whether the host is
/// showing the face stacked over its back ([`standalone`]; the host's to
/// set, 0 = standing alone).
#[must_use]
pub fn flip_params(ns: &str) -> Vec<ParamInfo> {
    vec![
        ui::toggle(&a(ns, "flip"), "More", false),
        ui::toggle(&a(ns, "stacked"), "Stacked", false),
    ]
}

/// The params a [`nameplate`] binds: the preset's name (text the host
/// fills) and the four presses.
#[must_use]
pub fn nameplate_params(ns: &str) -> Vec<ParamInfo> {
    let mut v = vec![
        ui::param(
            &a(ns, "preset"),
            "Preset",
            Range::linear(0.0, 1.0, Unit::None),
            0.0,
        )
        .text(),
    ];
    for (id, name) in [
        ("algo_menu", "Algorithm"),
        ("preset_prev", "Previous preset"),
        ("preset_next", "Next preset"),
        ("preset_browse", "Presets"),
    ] {
        v.push(ui::toggle(&a(ns, id), name, false));
    }
    v
}

/// The machine selector: a rotary switch over the block's `style` (delay)
/// or `algorithm` (reverb) — turning it swaps the face — in `ink`.
pub fn selector(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    cx: f64,
    cy: f64,
    d: f64,
    variant: &str,
    ink: &str,
    dim: &str,
) -> NodeId {
    let (addr, names): (String, &[&str]) = if ns == DLY {
        (a(ns, "style"), &DELAY_ALGOS)
    } else {
        (a(ns, "algorithm"), &VERB_ALGOS)
    };
    let s = Dial::new("switch.rotary", variant, d, ink, dim)
        .options(names)
        .sweep(300.0)
        .legend(0.0);
    ui::dial(b, parent, &addr, "Machine", cx, cy, &s)
}

/// An illuminated bypass key over `ns/on`, lit while the block plays.
pub fn bypass(
    b: &mut Builder<'_>,
    parent: NodeId,
    ns: &str,
    cx: f64,
    cy: f64,
    d: f64,
    variant: &str,
    legend: &str,
) -> NodeId {
    let p = Place::new("Bypass")
        .size(d)
        .variant(variant)
        .legend(legend)
        .bind(ui::bind(&a(ns, "on")));
    ui::put(b, parent, "button.illuminated", &p, cx, cy)
}

/// A knob on `addr`, printed in the face's ink.
#[allow(clippy::too_many_arguments)]
pub fn knob(
    b: &mut Builder<'_>,
    parent: NodeId,
    style: &str,
    variant: &str,
    addr: &str,
    legend: &str,
    d: f64,
    cx: f64,
    cy: f64,
    ink: &str,
    dim: &str,
) -> NodeId {
    let s = Dial::new(style, variant, d, ink, dim).legend((d * 0.2).clamp(7.0, 10.0));
    ui::dial(b, parent, addr, legend, cx, cy, &s)
}

/// A live echo train in (`x`, `y`, `w`, `h`): `taps` repeats, each at a
/// multiple of `dly/time` across a window of `window_ms`, each lower than
/// the last by `dly/feedback`, drawn as glowing bars in `color`.
pub fn echo_train(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    taps: u32,
    window_ms: f64,
    color: &str,
) -> NodeId {
    let mut f = draw::frame("Echoes", x, y, w, h);
    f.clips_content = true;
    let f = b.add(parent, f);
    // The dry hit at zero, then the repeats.
    ui::line(
        b,
        f,
        "dry",
        &[(2.0, h), (2.0, 0.0)],
        2.0,
        &ui::alpha(color, 0.5),
    );
    for i in 1..=taps {
        let bar = draw::rect(&format!("tap {i}"), 0.0, 0.0, 3.0, h, draw::solid(color));
        let bar = b.add(f, ui::glowing(bar, color, 1.1, GLOW));
        let at = format!("clamp({i} * param({DLY}/time) / {window_ms}, 0, 1)");
        motion::add(
            b,
            bar,
            Motion::new(Target::X)
                .input(at)
                .range(2.0, w - 3.0)
                .dynamics(Dynamics::spring(300.0, 30.0)),
        );
        let fall = format!("pow(param({DLY}/feedback), {})", i - 1);
        motion::add(
            b,
            bar,
            Motion::new(Target::StretchY)
                .input(fall)
                .range(0.02, 1.0)
                .pivot(0.5, 1.0)
                .dynamics(Dynamics::spring(300.0, 30.0)),
        );
    }
    f
}

/// A live decay envelope in (`x`, `y`, `w`, `h`): an exponential tail in
/// `color`, stretched across the box as `verb/decay` rises.
pub fn decay(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    color: &str,
) -> NodeId {
    let mut f = draw::frame("Decay", x, y, w, h);
    f.clips_content = true;
    let f = b.add(parent, f);
    let g = b.add(f, draw::frame("tail", 0.0, 0.0, w, h));
    let pts: Vec<(f64, f64)> = (0..=64)
        .map(|i| {
            let t = f64::from(i) / 64.0;
            (t * w, h - h * (-5.0 * t).exp())
        })
        .collect();
    let mut d = format!("M 0 {h:.1} ");
    for (px, py) in &pts {
        d += &format!("L {px:.1} {py:.1} ");
    }
    d += &format!("L {w:.1} {h:.1} Z");
    let fill = draw::path(
        "envelope",
        0.0,
        0.0,
        w,
        h,
        d,
        draw::linear(
            180.0,
            &[
                (0.0, &ui::alpha(color, 0.55)),
                (1.0, &ui::alpha(color, 0.05)),
            ],
        ),
    );
    b.add(g, fill);
    b.add(
        g,
        ui::glowing(
            ui::strokes("edge", &[pts], 1.6, draw::solid(color)),
            color,
            0.9,
            GLOW,
        ),
    );
    motion::add(
        b,
        g,
        Motion::new(Target::StretchX)
            .input(format!("norm({VERB}/decay)"))
            .range(0.15, 1.0)
            .pivot(0.0, 0.5)
            .dynamics(Dynamics::spring(300.0, 30.0)),
    );
    f
}

/// Printed legend, centred.
pub fn legend(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    cx: f64,
    cy: f64,
    size: f64,
    color: &str,
) -> NodeId {
    ui::silk(b, parent, s, cx, cy, size, color, 800.0)
}

/// A placeholder unit until the machine's own face is drawn: its name, the
/// selector, the main knobs and the picture.
#[must_use]
pub fn stub(ns: &str, name: &str, color: &str, dark: bool) -> document_proto::Scene {
    let ns = ns.to_string();
    let name = name.to_string();
    // The section's colour, whatever the placeholder asked for.
    let _ = color;
    let color = if ns == DLY { BLUE } else { PURPLE }.to_string();
    lane(&name.clone(), move |b, root| {
        let (ink, dim, top, bottom) = if dark {
            ("#e8e6e1", "#8a8d94", "#1d1f24", "#121317")
        } else {
            ("#1d1f24", "#686c74", "#eceef1", "#d7dae0")
        };
        let mut panel = draw::frame("Panel", 0.0, 0.0, W, H);
        panel.fills = vec![draw::linear(180.0, &[(0.0, top), (1.0, bottom)])];
        panel.corner_radii = document_proto::CornerRadii::uniform(8.0);
        let panel = b.add(root, panel);
        b.add(
            panel,
            ui::text(
                "name",
                &name,
                22.0,
                30.0,
                200.0,
                20.0,
                &color,
                TextAlign::Left,
                800.0,
                true,
            ),
        );
        bypass(
            b,
            panel,
            &ns,
            60.0,
            100.0,
            34.0,
            if ns == DLY { "blue" } else { "purple" },
            "On",
        );
        if ns == DLY {
            echo_train(b, panel, (220.0, 24.0, 380.0, 116.0), 8, 2000.0, &color);
            for (i, (id, l)) in [
                ("time", "Time"),
                ("feedback", "Feedback"),
                ("level", "Level"),
            ]
            .iter()
            .enumerate()
            {
                knob(
                    b,
                    panel,
                    "knob.3d",
                    "flat",
                    &a(&ns, id),
                    l,
                    46.0,
                    670.0 + 100.0 * i as f64,
                    70.0,
                    ink,
                    dim,
                );
            }
        } else {
            decay(b, panel, (220.0, 24.0, 380.0, 116.0), &color);
            for (i, (id, l)) in [("decay", "Decay"), ("size", "Size"), ("level", "Level")]
                .iter()
                .enumerate()
            {
                knob(
                    b,
                    panel,
                    "knob.3d",
                    "flat",
                    &a(&ns, id),
                    l,
                    46.0,
                    670.0 + 100.0 * i as f64,
                    70.0,
                    ink,
                    dim,
                );
            }
        }
    })
}

/// [`stub`] in a small room (a phone's lane, a desktop pane's): the Time
/// faces' layout — the echoes (a delay) or the decay (a reverb) as the
/// picture, the Time knob (the delay time, or the decay), three knobs in
/// the row, the nameplate top right, the bypass at its spot.
#[must_use]
pub fn stub_small(ns: &str, name: &str, dark: bool, tier: Tier) -> document_proto::Scene {
    let ns = ns.to_string();
    let name = name.to_string();
    let color = if ns == DLY { BLUE } else { PURPLE }.to_string();
    let w = tier.lane_w();
    tier_lane(&name.clone(), tier, move |b, root| {
        let (ink, dim, top, bottom, well) = if dark {
            (
                "#e8e6e1",
                "#8a8d94",
                "#1d1f24",
                "#121317",
                ("#0b0c0f", "#060708"),
            )
        } else {
            (
                "#1d1f24",
                "#686c74",
                "#eceef1",
                "#d7dae0",
                ("#14161a", "#0b0c0f"),
            )
        };
        let mut panel = draw::frame("Panel", 0.0, 0.0, w, H);
        panel.fills = vec![draw::linear(180.0, &[(0.0, top), (1.0, bottom)])];
        panel.corner_radii = document_proto::CornerRadii::uniform(8.0);
        let panel = b.add(root, panel);
        // The picture, in its window.
        let (vx, vy, vw, vh) = (10.0, 10.0, picture_r(tier) - 18.0, H - 20.0);
        let mut v = draw::frame("View", vx, vy, vw, vh);
        v.fills = vec![draw::linear(180.0, &[(0.0, well.0), (1.0, well.1)])];
        v.clips_content = true;
        v.corner_radii = document_proto::CornerRadii::uniform(6.0);
        v.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 6.0)];
        let v = b.add(panel, v);
        if ns == DLY {
            echo_train(b, v, (4.0, 8.0, vw - 8.0, vh - 16.0), 6, 2000.0, &color);
        } else {
            decay(b, v, (4.0, 8.0, vw - 8.0, vh - 16.0), &color);
        }
        let opts = [
            "color=#2a2d31",
            "pointer=#e9ecef",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#3a3e44",
        ];
        let (tx, ty, td) = time_spot(tier);
        time_knob(
            b,
            panel,
            &ns,
            (tx, ty, td),
            if tier == Tier::Ipad { 10.0 } else { 11.0 },
            (ink, dim, &ui::alpha(&color, 0.45)),
            |b, p, addr| {
                knob3d_bare(b, p, addr, td, (tx, ty), &opts);
            },
        );
        let (xs, ky, kd) = knob_row(tier);
        let knobs: [(&str, &str); 3] = if ns == DLY {
            [
                ("feedback", "Feedback"),
                ("high_cut", "Hi Cut"),
                ("level", "Level"),
            ]
        } else {
            [
                ("size", "Size"),
                ("predelay", "Pre-Delay"),
                ("level", "Level"),
            ]
        };
        for (i, (id, legend)) in knobs.iter().enumerate() {
            knob3d(
                b,
                panel,
                &a(&ns, id),
                legend,
                kd,
                (xs[i], ky),
                &opts,
                ink,
                dim,
            );
        }
        let (np_y, np_h, title_w, preset_w, size) = small_nameplate(tier);
        let title_w = title_w.max(name.chars().count() as f64 * (size * 0.75) + 22.0);
        nameplate_top_right(
            b,
            panel,
            &ns,
            tier,
            (np_y, np_h),
            title_w,
            preset_w,
            size,
            (&color, dim),
            |b, t| {
                ui::plain(
                    b,
                    t,
                    &name,
                    (title_w - 14.0) / 2.0,
                    np_h / 2.0,
                    size + 5.0,
                    ink,
                    800.0,
                );
            },
        );
        face_bypass(
            b,
            panel,
            &ns,
            tier,
            if ns == DLY { "blue" } else { "purple" },
            "On",
        );
        responsive(b, panel, &[v]);
    })
}

/// A rendered knob (`knob.3d`) on `addr` with the kit's `options` (colour,
/// pointer, skirt…), `d` across at (`cx`, `cy`), its legend under it and
/// its live value between, in the face's inks.
#[allow(clippy::too_many_arguments)]
pub fn knob3d(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    legend_text: &str,
    d: f64,
    (cx, cy): (f64, f64),
    options: &[&str],
    ink: &str,
    dim: &str,
) -> NodeId {
    let mut opts = vec!["touch=44", "ticks=none"];
    opts.extend_from_slice(options);
    let p = Place::new(legend_text)
        .legend(" ")
        .size(d)
        .variant("flat")
        .options(&opts)
        .bind(ui::bind(addr));
    let k = ui::put(b, parent, "knob.3d", &p, cx, cy);
    ui::strip_legend(b, k);
    ui::add_value(b, k, (d * 0.16).clamp(7.0, 9.0), dim);
    let size = (d * 0.19).clamp(7.0, 10.0);
    ui::silk(
        b,
        parent,
        legend_text,
        cx,
        cy + d / 2.0 + 20.0,
        size,
        ink,
        800.0,
    );
    k
}

/// A seven-segment readout of `addr` (to `decimals`, no unit), lit in
/// `color` over its unlit segments ("888…", `digits` wide), right-aligned
/// in the box (`x`, `cy`, `w`) at `size`.
#[allow(clippy::too_many_arguments)]
pub fn seg_readout(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    (x, cy, w): (f64, f64, f64),
    size: f64,
    digits: usize,
    decimals: u8,
    color: &str,
) -> NodeId {
    use document_proto::{NodeKind, TextData};
    let h = (size * 1.3).ceil();
    let root = b.add(
        parent,
        draw::frame(&format!("{addr} readout"), x, cy - h / 2.0, w, h),
    );
    let text = |s: &str, fill: &str| {
        let mut n = draw::text("segments", s, 0.0, 0.0, w, size, fill, TextAlign::Right);
        if let NodeKind::Text(t) = &mut n.kind {
            *t = TextData {
                characters: s.to_string(),
                font_family: Some(draw::family::DSEG7.into()),
                font_size: size,
                align: TextAlign::Right,
                ..TextData::default()
            };
        }
        n
    };
    let unlit = if decimals == 0 {
        "8".repeat(digits)
    } else {
        format!(
            "{}.{}",
            "8".repeat(digits - usize::from(decimals)),
            "8".repeat(usize::from(decimals))
        )
    };
    b.add(root, text(&unlit, &ui::alpha(color, 0.12)));
    let lit = b.add(
        root,
        draw::emissive(text("", color), draw::hex(color), 0.9, size * 0.5),
    );
    let c = surface_proto::Control::new(surface_proto::ControlKind::Readout)
        .slot(surface_proto::SlotRole::Value, lit.0)
        .bound(ui::bind(addr))
        .formatted(surface_proto::ValueFormat::decimals(decimals).unit(""));
    b.controls.insert(root, c);
    root
}

/// Make a face responsive about its picture: `stretch` (the picture's
/// window, a bay of knobs) takes a change of width; the rest of `parent`'s
/// children pin to whichever side of it they sit on — left of it to the
/// left edge, right of it to the right — so controls keep their size and
/// spacing while the picture widens or narrows. Anything spanning the
/// whole face (its ground) stretches of itself.
pub fn responsive(b: &mut Builder<'_>, parent: NodeId, stretch: &[NodeId]) {
    use document_proto::ResizeX;
    let span = |b: &Builder<'_>, id: NodeId| {
        b.scene
            .node(id)
            .map(|n| (n.transform.tx, n.transform.tx + n.size.width))
    };
    let Some((l, r)) = stretch
        .iter()
        .filter_map(|id| span(b, *id))
        .reduce(|a, c| (a.0.min(c.0), a.1.max(c.1)))
    else {
        return;
    };
    let kids = b
        .scene
        .node(parent)
        .map(|n| n.children.clone())
        .unwrap_or_default();
    let pw = b.scene.node(parent).map_or(0.0, |n| n.size.width);
    for id in kids {
        let Some(n) = b.scene.node_mut(id) else {
            continue;
        };
        if n.layout_child.resize_x.is_some() {
            continue;
        }
        let (x, w) = (n.transform.tx, n.size.width);
        n.layout_child.resize_x = if stretch.contains(&id) {
            Some(ResizeX::Stretch)
        } else if w >= pw * 0.95 {
            None
        } else if x + w / 2.0 <= l {
            Some(ResizeX::Left)
        } else if x + w / 2.0 >= r {
            Some(ResizeX::Right)
        } else {
            None
        };
    }
}

/// `id` with its sibling `offset` places along (−1: the one drawn just
/// before it, a window's bezel; +1: just after, its rim), for
/// [`responsive`].
#[must_use]
pub fn with_sibling(b: &Builder<'_>, parent: NodeId, id: NodeId, offset: isize) -> Vec<NodeId> {
    let kids = b
        .scene
        .node(parent)
        .map(|n| n.children.clone())
        .unwrap_or_default();
    let mut v = vec![id];
    if let Some(i) = kids.iter().position(|k| *k == id)
        && let Some(s) = kids.get(i.wrapping_add_signed(offset))
    {
        v.push(*s);
    }
    v
}

/// A drawing that should scale with its window (a plate's sheet and
/// pickups, a hall's plan): `parent`'s children take position and width in
/// proportion when the window resizes (their own children keep their size);
/// what spans the window stretches with it instead.
pub fn scale_children(b: &mut Builder<'_>, parent: NodeId) {
    let kids = b
        .scene
        .node(parent)
        .map(|n| n.children.clone())
        .unwrap_or_default();
    let pw = b.scene.node(parent).map_or(0.0, |n| n.size.width);
    for id in kids {
        if let Some(n) = b.scene.node_mut(id)
            && n.layout_child.resize_x.is_none()
            // What spans the window (its frame, its glass) stretches with it.
            && n.size.width < pw * 0.9
        {
            n.layout_child.resize_x = Some(document_proto::ResizeX::Scale);
        }
    }
}

// ── The tall layout ─────────────────────────────────────────────────────

/// A tall face: one Time face standing alone in a lane column, two phone
/// lanes high — its picture the whole top row, every control it has (the
/// front's and the back's) across the bottom row, one nameplate, no flip.
pub const TALL_W: f64 = 760.0;
pub const TALL_H: f64 = 2.0 * H;
/// The picture's box in a tall face: the top row, inset.
pub const TALL_PICTURE: (f64, f64, f64, f64) = (10.0, 10.0, TALL_W - 20.0, H - 16.0);

/// The delay's advanced controls, as its backs print them.
pub const DLY_MORE: [(&str, &str); 9] = [
    ("high_pass", "Low Cut"),
    ("high_cut", "High Cut"),
    ("mod_rate", "Mod Rate"),
    ("mod_depth", "Mod"),
    ("duck_sens", "Duck"),
    ("duck_release", "Duck Rel"),
    ("pan", "Pan"),
    ("repeat_dyn", "Dynamics"),
    ("dry", "Dry"),
];
/// The reverb's.
pub const VERB_MORE: [(&str, &str); 10] = [
    ("predelay", "Pre-Delay"),
    ("damping", "HF Damp"),
    ("modulation", "Mod"),
    ("low_cut", "Low Cut"),
    ("high_cut", "High Cut"),
    ("tone", "Tone"),
    ("duck", "Duck"),
    ("duck_threshold", "Duck Thr"),
    ("duck_release", "Duck Rel"),
    ("pan_a", "Pan"),
];

/// A tall face's file: `base-tall` (`-dark`).
#[must_use]
pub fn tall_file(base: &str, dark: bool) -> &'static str {
    Box::leak(
        if dark {
            format!("{base}-tall-dark")
        } else {
            format!("{base}-tall")
        }
        .into_boxed_str(),
    )
}

/// A tall face's canvas: `build` draws into the root, already
/// [`TALL_W`] × [`TALL_H`].
pub fn tall_lane(
    name: &str,
    build: impl FnOnce(&mut Builder<'_>, NodeId),
) -> document_proto::Scene {
    canvas(name, |b, root| {
        ui::fit_root(b, root, TALL_W, TALL_H);
        build(b, root);
        ui::flush(b, root, TALL_W, TALL_H);
    })
}

/// What a tall face's chrome needs from its face: its namespace, the
/// knobs of its first line (its front's; the advanced ones follow on the
/// second, [`DLY_MORE`] / [`VERB_MORE`] less these), its inks (ink, dim,
/// the TIME | BEAT light, the nameplate's accent), the scrim the nameplate
/// sits on over the picture, its bypass (variant, legend) and its title's
/// width.
pub struct Tall<'a> {
    pub ns: &'a str,
    pub main: &'a [(&'a str, &'a str)],
    pub inks: (&'a str, &'a str, &'a str, &'a str),
    pub scrim: &'a str,
    pub bypass: (&'a str, &'a str),
    pub title_w: f64,
}

/// A tall face's advanced controls: the namespace's, less those `main`
/// already has.
#[must_use]
pub fn tall_more<'a>(ns: &str, main: &[(&str, &str)]) -> Vec<(&'a str, &'a str)> {
    let all: &[(&'a str, &'a str)] = if ns == DLY { &DLY_MORE } else { &VERB_MORE };
    all.iter()
        .filter(|(id, _)| !main.iter().any(|(m, _)| m == id))
        .copied()
        .collect()
}

/// Where a tall face's controls sit: the `split` they start below, and
/// each line's centre (below the split) and knob diameter.
#[derive(Clone, Copy)]
pub struct TallRows {
    pub split: f64,
    pub main: (f64, f64),
    pub more: (f64, f64),
}

/// The tall faces' rows: the picture the top half, the controls the
/// bottom.
pub const TALL_ROWS: TallRows = TallRows {
    split: H,
    main: (32.0, 40.0),
    more: (110.0, 30.0),
};

/// A tall face's chrome on its ground `p` (the picture already drawn in
/// [`TALL_PICTURE`]): the nameplate over the picture's top right on its
/// scrim (`title` letters the name), and across the bottom row the Time
/// knob (`time` draws the knob, bare, on its address at (cx, cy) `d`
/// across), the front's knobs on the first line and the advanced ones on
/// the second (`knob` draws each, as a back's), and the bypass at the
/// right.
pub fn tall_chrome(
    b: &mut Builder<'_>,
    p: NodeId,
    t: &Tall<'_>,
    knob: impl FnMut(&mut Builder<'_>, NodeId, &str, &str, f64, (f64, f64)),
    time: impl FnOnce(&mut Builder<'_>, NodeId, &str, (f64, f64, f64)),
    title: impl FnOnce(&mut Builder<'_>, NodeId),
) {
    tall_chrome_rows(b, p, t, TALL_ROWS, knob, time, title);
}

/// [`tall_chrome`] with its controls laid out by `rows`: a face whose
/// picture needs less than the top half gives its knobs the rest.
pub fn tall_chrome_rows(
    b: &mut Builder<'_>,
    p: NodeId,
    t: &Tall<'_>,
    rows: TallRows,
    mut knob: impl FnMut(&mut Builder<'_>, NodeId, &str, &str, f64, (f64, f64)),
    time: impl FnOnce(&mut Builder<'_>, NodeId, &str, (f64, f64, f64)),
    title: impl FnOnce(&mut Builder<'_>, NodeId),
) {
    let (ink, dim, lit, accent) = t.inks;
    // The nameplate, on its scrim over the picture.
    let (np_y, np_h, preset_w) = (16.0, 30.0, 200.0);
    let right = TALL_W - 10.0;
    let x = right - 12.0 - preset_w - 6.0 - t.title_w;
    b.add(
        p,
        draw::with_effects(
            draw::rounded(
                draw::rect(
                    "nameplate scrim",
                    x - 10.0,
                    np_y - 4.0,
                    right - 4.0 - (x - 10.0),
                    np_h + 8.0,
                    draw::solid(t.scrim),
                ),
                8.0,
            ),
            vec![draw::drop_shadow("#00000040", 0.0, 2.0, 6.0)],
        ),
    );
    nameplate_right(
        b,
        p,
        t.ns,
        right,
        (np_y, np_h),
        t.title_w,
        preset_w,
        12.0,
        (accent, dim),
        title,
    );
    // The bottom row: the Time knob at the left…
    let y0 = rows.split;
    let mid = (TALL_H - y0) / 2.0;
    let (tx, ty, td) = (78.0, y0 + mid - 30.0, 64.0);
    time_knob(
        b,
        p,
        t.ns,
        (tx, ty, td),
        11.0,
        (ink, dim, lit),
        |b, pp, addr| time(b, pp, addr, (tx, ty, td)),
    );
    ui::rule(
        b,
        p,
        160.0,
        y0 + 14.0,
        1.0,
        TALL_H - y0 - 28.0,
        &ui::alpha(dim, 0.35),
    );
    // …the bypass at the right…
    let (bx, by, bd) = (TALL_W - 52.0, y0 + mid - 26.0, 44.0);
    bypass(b, p, t.ns, bx, by, bd, t.bypass.0, t.bypass.1);
    // …and between, the knobs: the front's above, the advanced below.
    let (x0, x1) = (182.0, bx - bd / 2.0 - 18.0);
    let more = tall_more(t.ns, t.main);
    for (list, (cy, d)) in [(t.main, rows.main), (more.as_slice(), rows.more)] {
        let cy = y0 + cy;
        let n = list.len().max(1) as f64;
        let step = ((x1 - x0) / n).min(92.0);
        let left = x0 + ((x1 - x0) - step * n) / 2.0;
        for (i, (id, legend)) in list.iter().enumerate() {
            knob(
                b,
                p,
                &a(t.ns, id),
                legend,
                d,
                (left + step * (i as f64 + 0.5), cy),
            );
        }
    }
}

/// The params a tall face binds besides its block's: its nameplate's and
/// its Time knob's.
#[must_use]
pub fn tall_params(ns: &str) -> Vec<ParamInfo> {
    let mut v = nameplate_params(ns);
    v.extend(time_params(ns));
    v
}

/// A placeholder's tall face, until its machine is drawn: its plain panel,
/// the section's live picture (an echo train or a decay) as the top row,
/// and the chrome; `base` its face's file.
#[must_use]
pub fn stub_tall(ns: &str, name: &str, base: &str, dark: bool) -> Face {
    let (ink, dim, top, bottom) = if dark {
        ("#e8e6e1", "#8a8d94", "#1d1f24", "#121317")
    } else {
        ("#1d1f24", "#686c74", "#eceef1", "#d7dae0")
    };
    let color = if ns == DLY { BLUE } else { PURPLE };
    let main: &[(&str, &str)] = if ns == DLY {
        &[("feedback", "Feedback"), ("level", "Level")]
    } else {
        &[("size", "Size"), ("level", "Level")]
    };
    let scene = tall_lane(name, |b, root| {
        let mut panel = draw::frame("Panel", 0.0, 0.0, TALL_W, TALL_H);
        panel.fills = vec![draw::linear(180.0, &[(0.0, top), (1.0, bottom)])];
        panel.corner_radii = document_proto::CornerRadii::uniform(8.0);
        let panel = b.add(root, panel);
        let (px, py, pw, ph) = TALL_PICTURE;
        let mut well = draw::frame("Picture", px, py, pw, ph);
        well.fills = vec![draw::linear(180.0, &[(0.0, "#0c0d10"), (1.0, "#16181d")])];
        well.corner_radii = document_proto::CornerRadii::uniform(6.0);
        well.clips_content = true;
        let well = b.add(panel, well);
        if ns == DLY {
            echo_train(
                b,
                well,
                (16.0, 44.0, pw - 32.0, ph - 56.0),
                10,
                2500.0,
                color,
            );
        } else {
            decay(b, well, (16.0, 44.0, pw - 32.0, ph - 56.0), color);
        }
        glass(b, well);
        let opts = [
            "color=#2a2c31",
            "pointer=#ffffff",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#111215",
        ];
        let t = Tall {
            ns,
            main,
            inks: (ink, dim, &ui::alpha(color, 0.55), color),
            scrim: if dark { "#1d1f24e6" } else { "#eceef1e6" },
            bypass: (if ns == DLY { "blue" } else { "purple" }, "On"),
            title_w: name.chars().count() as f64 * 11.0 + 24.0,
        };
        tall_chrome(
            b,
            panel,
            &t,
            |b, p, addr, legend, d, at| {
                knob3d(b, p, addr, legend, d, at, &opts, ink, dim);
            },
            |b, p, addr, (cx, cy, d)| {
                knob3d_bare(b, p, addr, d, (cx, cy), &opts);
            },
            |b, tn| {
                ui::plain(
                    b,
                    tn,
                    name,
                    (name.chars().count() as f64 * 11.0 + 12.0) / 2.0,
                    15.0,
                    18.0,
                    ink,
                    800.0,
                );
            },
        );
    });
    let file = tall_file(base, dark);
    let mut f = if ns == DLY {
        delay_face(file, scene)
    } else {
        reverb_face(file, scene)
    };
    f.params.extend(tall_params(ns));
    f
}
