//! The standard-size stompboxes (156 × 286, the Standard class) as
//! rendered enclosures (`panel.pedal_3d`: the metallic lid, the copper
//! front face and base, the jacks out of the sides): black ridged knobs
//! with white teardrop caps inside rings of white dots, all at noon as the
//! reference shows them, the engage LED between the knobs, a white line
//! icon, arrows for the signal path, the stomp, and the name set lowercase
//! in a heavy sans, stacked in the bottom-left corner.
//!
//! Positions are fractions of the lid, measured off the reference.
//!
//! - `02-envelope-standard-156x286`: *the envelope pusher* — an envelope
//!   filter (Range, Sensitivity, Attack, Decay).
//! - `03-squeeze-standard-156x286`: *the fourth slot compressor* — a
//!   blend compressor (Blend, Tone, Compression, Volume).
//! - `04-glass-standard-156x286`: *the glass bottle* — a tube drive
//!   (Tone, Drive, Level).
//! - `05-haul-standard-156x286`: *the heavy haul overdrive* (Tone, Drive,
//!   Level).
//! - `11-neon-standard-156x286`: *The Neon* — a chorus (Mix, Rate, Width;
//!   the LED breathes at the rate).
//! - `13-rinse-standard-156x286`: *The Rinse* — a reverb with a shimmer
//!   button (Mix, Decay, low cut, high cut).

use document_proto::NodeId;
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::ParamInfo;
use surface_widgets::Builder;

use super::parts::{self, KnobLook, Lid, PEDAL_H, STANDARD_W, Type};
use crate::face::{Face, canvas};
use crate::reverb::ui;

pub(super) const PLUGIN: &str = "suite-funk-rhythm";
const WHITE: &str = "#fbfaf6";
/// The engage LED's light.
const LED: &str = "#e8f6ff";
/// The enclosures' front face and base: a rosy copper.
pub(super) const BASE: &str = "#b06f68";
/// The pedals' knob (`knob.3d`): a black fluted body on a flared skirt,
/// a cream teardrop insert on top for the index.
pub(super) const KNOB: KnobLook<'static> = KnobLook {
    cap: "pedal_chicken",
    opts: &[
        "insert_color=#f1ede4",
        "insert_metal=0",
        "insert_rough=0.3",
        "insert_r=0.8",
        "insert_h=0.14",
        "cap_r=0.72",
        "fallback_color=#f2efe8",
        "material=plastic",
        "skirt_color=#18181a",
        "ticks=dots",
    ],
};
/// The lid sits in from the sides as far as the reference's jacks stand out.
pub(super) const LID: &[&str] = &["jack_inset=13"];
/// Knob diameter on a standard lid.
const KNOB_D: f64 = 33.0;

/// A knob on a pedal: address, legend, centre (lid fractions), legend
/// above (else below).
pub(super) struct K<'a>(pub &'a str, pub &'a str, pub (f64, f64), pub bool);

/// The shared top half: knobs and their legends.
pub(super) fn knobs(b: &mut Builder<'_>, lid: &Lid, ks: &[K<'_>], d: f64) {
    let t = Type::new(7.8, WHITE, 800.0);
    for K(addr, legend, (fx, fy), above) in ks {
        let (x, y) = lid.at(*fx, *fy);
        parts::knob3d(b, lid.id, &KNOB, addr, legend, d, (x, y), WHITE);
        let ly = if *above {
            y - d * 0.5 - 12.5
        } else {
            y + d * 0.5 + 12.5
        };
        if !legend.is_empty() {
            parts::centred(b, lid.id, legend, (x, ly), t);
        }
    }
}

/// The in/out arrows either side at `y`.
pub(super) fn arrows(b: &mut Builder<'_>, lid: &Lid, y: f64) {
    for x in [lid.w * 0.05, lid.w * 0.95 - 4.5] {
        b.add(
            lid.id,
            draw::path(
                "arrow",
                0.0,
                0.0,
                lid.w,
                lid.h,
                draw::polygon(&[(x, y - 4.0), (x + 4.5, y), (x, y + 4.0)]),
                draw::solid(WHITE),
            ),
        );
    }
}

/// The DC jack's printed symbol and legend.
pub(super) fn dc(b: &mut Builder<'_>, lid: NodeId, (cx, cy): (f64, f64)) {
    parts::centred(b, lid, "9VDC", (cx, cy - 6.0), Type::new(3.8, WHITE, 800.0));
    parts::art(
        b,
        lid,
        &[
            parts::ring(cx - 5.5, cy, 1.9),
            parts::ring(cx, cy, 2.3),
            parts::ring(cx + 5.5, cy, 1.9),
            vec![(cx - 3.6, cy), (cx - 2.3, cy)],
            vec![(cx + 2.3, cy), (cx + 3.6, cy)],
        ],
        0.8,
        WHITE,
    );
}

/// A standard pedal: `color` enclosure, knobs, LED, icon, stomp and name.
pub(super) struct Pedal<'a> {
    pub title: &'a str,
    pub ns: &'a str,
    pub color: &'a str,
    pub knobs: &'a [K<'a>],
    pub name: &'a [&'a str],
    pub icon: fn(&mut Builder<'_>, &Lid, (f64, f64)),
    pub pulse: Option<&'a str>,
    /// Extra controls (a push button): run after the knobs.
    pub extra: Option<fn(&mut Builder<'_>, &Lid)>,
}

pub(super) fn standard(p: &Pedal<'_>) -> document_proto::Scene {
    canvas(p.title, |b, root| {
        let lid = parts::stompbox_with(
            b,
            root,
            (STANDARD_W, PEDAL_H),
            p.color,
            "powder",
            BASE,
            9.0,
            LID,
        );
        knobs(b, &lid, p.knobs, KNOB_D);
        if let Some(extra) = p.extra {
            extra(b, &lid);
        }
        parts::status_led(
            b,
            lid.id,
            &format!("{}/on", p.ns),
            5.0,
            lid.at(0.5, 0.46),
            LED,
            "light",
            p.pulse,
        );
        (p.icon)(b, &lid, lid.at(0.5, 0.615));
        arrows(b, &lid, lid.h * 0.72);
        parts::footswitch(
            b,
            lid.id,
            &format!("{}/on", p.ns),
            "Bypass",
            24.0,
            lid.at(0.5, 0.8),
            false,
        );
        dc(b, lid.id, lid.at(0.865, 0.862));
        let lines = p.name.len() as f64;
        let lead = 9.4;
        parts::stacked(
            b,
            lid.id,
            p.name,
            (lid.w * 0.06, lid.h * 0.955 - lead * (lines - 1.0)),
            lead,
            Type::new(10.6, WHITE, 800.0).track(-0.03),
        );
    })
}

fn four(ns: &'static str, names: [(&'static str, &'static str); 4]) -> Vec<K<'static>> {
    // Leaked once per face: the layout table lives as long as the face.
    let a =
        |i: usize| -> &'static str { Box::leak(format!("{ns}/{}", names[i].0).into_boxed_str()) };
    vec![
        K(a(0), names[0].1, (0.215, 0.16), true),
        K(a(1), names[1].1, (0.785, 0.16), true),
        K(a(2), names[2].1, (0.215, 0.342), false),
        K(a(3), names[3].1, (0.785, 0.342), false),
    ]
}

fn three(ns: &'static str, names: [(&'static str, &'static str); 3]) -> Vec<K<'static>> {
    let a =
        |i: usize| -> &'static str { Box::leak(format!("{ns}/{}", names[i].0).into_boxed_str()) };
    vec![
        K(a(0), names[0].1, (0.5, 0.16), true),
        K(a(1), names[1].1, (0.215, 0.342), false),
        K(a(2), names[2].1, (0.785, 0.342), false),
    ]
}

fn face(file: &'static str, scene: document_proto::Scene, params: Vec<ParamInfo>) -> Face {
    ui::formatted(Face {
        file,
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    })
}

// ── icons (white line art, ~34 px) ───────────────────────────────────────

fn envelope_icon(b: &mut Builder<'_>, lid: &Lid, (cx, cy): (f64, f64)) {
    let (w, h) = (30.0, 20.0);
    let (x, y) = (cx - w / 2.0, cy - h / 2.0);
    let body = parts::round_rect(x, y, w, h, 2.0);
    let flap = vec![
        (x + 1.0, y + 1.5),
        (cx, y + h * 0.58),
        (x + w - 1.0, y + 1.5),
    ];
    let sweep: Vec<(f64, f64)> = (0..=24)
        .map(|i| {
            let u = f64::from(i) / 24.0;
            (
                x + 3.0 + (w - 6.0) * u,
                y + h + 7.0 - 5.0 * (u * std::f64::consts::PI).sin().powf(3.0),
            )
        })
        .collect();
    parts::art(b, lid.id, &[body, flap, sweep], 1.8, WHITE);
}

fn selector_icon(b: &mut Builder<'_>, lid: &Lid, (cx, cy): (f64, f64)) {
    let base = cy + 7.0;
    let mut lines = vec![vec![(cx - 25.0, base), (cx + 25.0, base)]];
    for i in 0..5 {
        let x = cx - 20.0 + 10.0 * f64::from(i);
        if i == 3 {
            // The blade, leaning into the fourth slot.
            lines.push(vec![
                (x - 2.0, base - 1.0),
                (x - 5.0, base - 12.0),
                (x - 2.0, base - 20.0),
                (x + 3.0, base - 17.0),
                (x + 2.0, base - 6.0),
                (x - 2.0, base - 1.0),
            ]);
        } else {
            lines.push(vec![(x - 2.5, base - 8.0), (x + 2.5, base - 3.0)]);
            lines.push(vec![(x + 2.5, base - 8.0), (x - 2.5, base - 3.0)]);
        }
        // The slot's numeral as a tick below the line.
        lines.push(vec![(x, base + 3.0), (x, base + 6.5)]);
    }
    parts::art(b, lid.id, &lines, 1.5, WHITE);
}

fn tube_icon(b: &mut Builder<'_>, lid: &Lid, (cx, cy): (f64, f64)) {
    let (w, h) = (16.0, 28.0);
    let (x, y) = (cx - w / 2.0, cy - h / 2.0 - 2.0);
    let mut envelope = vec![(x, y + h), (x, y + 6.0)];
    envelope.extend((0..=12).map(|i| {
        let a = std::f64::consts::PI * (1.0 + f64::from(i) / 12.0);
        (cx + w / 2.0 * a.cos(), y + 6.0 + 6.0 * a.sin())
    }));
    envelope.push((x + w, y + h));
    envelope.push((x, y + h));
    let mut lines = vec![
        envelope,
        vec![(cx - 1.5, y - 0.5), (cx - 1.5, y - 3.0)],
        vec![(cx + 1.5, y - 0.5), (cx + 1.5, y - 3.0)],
    ];
    // Plates and grid.
    lines.push(parts::round_rect(x + 3.5, y + 9.0, w - 7.0, 11.0, 1.0));
    for i in 0..4 {
        let gx = x + 5.5 + 1.7 * f64::from(i);
        lines.push(vec![(gx, y + 10.5), (gx, y + 18.5)]);
    }
    // The base and its pins.
    lines.push(vec![(x - 1.0, y + h + 2.0), (x + w + 1.0, y + h + 2.0)]);
    for dx in [-4.5, -1.5, 1.5, 4.5] {
        lines.push(vec![(cx + dx, y + h + 2.0), (cx + dx, y + h + 6.0)]);
    }
    parts::art(b, lid.id, &lines, 1.4, WHITE);
}

fn truck_icon(b: &mut Builder<'_>, lid: &Lid, (cx, cy): (f64, f64)) {
    let (x0, y0) = (cx - 22.0, cy - 13.0);
    // The dump body, the cab, the chassis and two big wheels.
    let bed = vec![
        (x0 + 14.0, y0 + 2.0),
        (x0 + 44.0, y0),
        (x0 + 42.0, y0 + 13.0),
        (x0 + 16.0, y0 + 14.0),
        (x0 + 14.0, y0 + 2.0),
    ];
    let cab = vec![
        (x0 + 2.0, y0 + 16.0),
        (x0 + 5.0, y0 + 7.0),
        (x0 + 14.0, y0 + 7.0),
        (x0 + 14.0, y0 + 16.0),
    ];
    let chassis = vec![(x0, y0 + 18.0), (x0 + 44.0, y0 + 18.0)];
    let window = vec![
        (x0 + 6.5, y0 + 13.0),
        (x0 + 7.5, y0 + 9.5),
        (x0 + 12.0, y0 + 9.5),
        (x0 + 12.0, y0 + 13.0),
        (x0 + 6.5, y0 + 13.0),
    ];
    let lines = vec![
        bed,
        cab,
        chassis,
        window,
        parts::ring(x0 + 10.0, y0 + 24.0, 5.5),
        parts::ring(x0 + 32.0, y0 + 24.0, 5.5),
        parts::ring(x0 + 10.0, y0 + 24.0, 2.0),
        parts::ring(x0 + 32.0, y0 + 24.0, 2.0),
    ];
    parts::art(b, lid.id, &lines, 1.7, WHITE);
}

fn coil_icon(b: &mut Builder<'_>, lid: &Lid, (cx, cy): (f64, f64)) {
    // A spring toy arched over: loops round a semicircle.
    let mut lines = Vec::new();
    for i in 0..=26 {
        let a = std::f64::consts::PI * (1.0 - f64::from(i) / 26.0);
        let (ix, iy) = (cx + 13.0 * a.cos(), cy + 8.0 - 13.0 * a.sin());
        let (ox, oy) = (cx + 25.0 * a.cos(), cy + 8.0 - 25.0 * a.sin());
        lines.push(vec![(ix, iy), (ox, oy)]);
    }
    parts::art(b, lid.id, &lines, 1.3, WHITE);
}

fn wave_icon(b: &mut Builder<'_>, lid: &Lid, (cx, cy): (f64, f64)) {
    let mut lines = Vec::new();
    for k in 0..4 {
        let off = f64::from(k) * 3.2;
        lines.push(
            (0..=40)
                .map(|i| {
                    let u = f64::from(i) / 40.0;
                    let x = cx - 16.0 + 32.0 * u;
                    let env = (u * std::f64::consts::PI).sin();
                    (
                        x,
                        cy + off - 4.0 - 12.0 * env * (u * std::f64::consts::TAU * 1.5).cos(),
                    )
                })
                .collect(),
        );
    }
    parts::art(b, lid.id, &lines, 1.3, WHITE);
}

// ── the pedals ───────────────────────────────────────────────────────────

pub(super) fn envelope() -> Face {
    let ks = four(
        "envelope",
        [
            ("range", "Range"),
            ("sens", "Sensitivity"),
            ("attack", "Attack"),
            ("decay", "Decay"),
        ],
    );
    let scene = standard(&Pedal {
        title: "Envelope Pusher",
        ns: "envelope",
        color: "#d46c9b",
        knobs: &ks,
        name: &["the", "envelope", "pusher"],
        icon: envelope_icon,
        pulse: None,
        extra: None,
    });
    // Every knob at noon, as the reference sets them.
    let params = vec![
        parts::pct("envelope/range", "Range", 50.0),
        parts::pct("envelope/sens", "Sensitivity", 50.0),
        parts::ms(
            "envelope/attack",
            "Attack",
            1.0,
            200.0,
            parts::log_at(1.0, 200.0, 0.5),
        ),
        parts::ms(
            "envelope/decay",
            "Decay",
            20.0,
            2000.0,
            parts::log_at(20.0, 2000.0, 0.5),
        ),
        parts::switch("envelope/on", "Bypass", true),
    ];
    face("02-envelope-standard-156x286", scene, params)
}

pub(super) fn squeeze() -> Face {
    let ks = four(
        "squeeze",
        [
            ("blend", "Blend"),
            ("tone", "Tone"),
            ("comp", "Compression"),
            ("volume", "Volume"),
        ],
    );
    let scene = standard(&Pedal {
        title: "Fourth Slot Compressor",
        ns: "squeeze",
        color: "#4b72c2",
        knobs: &ks,
        name: &["the fourth", "slot", "compressor"],
        icon: selector_icon,
        pulse: None,
        extra: None,
    });
    let params = vec![
        parts::pct("squeeze/blend", "Blend", 50.0),
        parts::pct("squeeze/tone", "Tone", 50.0),
        parts::pct("squeeze/comp", "Compression", 50.0),
        parts::db("squeeze/volume", "Volume", -12.0, 12.0, 0.0),
        parts::switch("squeeze/on", "Bypass", true),
    ];
    face("03-squeeze-standard-156x286", scene, params)
}

pub(super) fn glass() -> Face {
    let ks = three(
        "glass",
        [("tone", "Tone"), ("drive", "Drive"), ("level", "Level")],
    );
    let scene = standard(&Pedal {
        title: "Glass Bottle Drive",
        ns: "glass",
        color: "#5cb86b",
        knobs: &ks,
        name: &["the", "glass bottle"],
        icon: tube_icon,
        pulse: None,
        extra: None,
    });
    let params = vec![
        parts::pct("glass/tone", "Tone", 50.0),
        parts::pct("glass/drive", "Drive", 50.0),
        parts::db("glass/level", "Level", -12.0, 12.0, 0.0),
        parts::switch("glass/on", "Bypass", true),
    ];
    face("04-glass-standard-156x286", scene, params)
}

pub(super) fn haul() -> Face {
    let ks = three(
        "haul",
        [("tone", "Tone"), ("drive", "Drive"), ("level", "Level")],
    );
    let scene = standard(&Pedal {
        title: "Heavy Haul Overdrive",
        ns: "haul",
        color: "#c29e3a",
        knobs: &ks,
        name: &["the heavy haul", "overdrive"],
        icon: truck_icon,
        pulse: None,
        extra: None,
    });
    let params = vec![
        parts::pct("haul/tone", "Tone", 50.0),
        parts::pct("haul/drive", "Drive", 50.0),
        parts::db("haul/level", "Level", -12.0, 12.0, 0.0),
        parts::switch("haul/on", "Bypass", true),
    ];
    face("05-haul-standard-156x286", scene, params)
}

pub(super) fn neon() -> Face {
    let ks = three(
        "neon",
        [("mix", "Mix"), ("rate", "Rate"), ("width", "Width")],
    );
    let scene = standard(&Pedal {
        title: "The Neon Chorus",
        ns: "neon",
        color: "#a0c0d6",
        knobs: &ks,
        name: &["The", "Neon"],
        icon: coil_icon,
        pulse: Some("param(neon/rate)"),
        extra: None,
    });
    let params = vec![
        parts::pct("neon/mix", "Mix", 50.0),
        ui::param(
            "neon/rate",
            "Rate",
            surface_proto::Range::log(0.1, 10.0, surface_proto::Unit::Hz),
            1.0,
        )
        .featured(),
        parts::pct("neon/width", "Width", 50.0),
        parts::switch("neon/on", "Bypass", true),
    ];
    face("11-neon-standard-156x286", scene, params)
}

fn shimmer_button(b: &mut Builder<'_>, lid: &Lid) {
    let (cx, cy) = lid.at(0.5, 0.158);
    let p = Place::new("Shimmer")
        .size(14.0)
        .variant("cream")
        .bind(ui::bind("rinse/shimmer"))
        .options(&["nolamp"])
        .legend("")
        .touch(TOUCH_MIN);
    let k = surface_kit::place(b, lid.id, "button.round_cap", &p).unwrap_or_else(|e| panic!("{e}"));
    ui::strip_legend(b, k);
    parts::centre_on(b, k, (cx, cy));
    parts::centred(
        b,
        lid.id,
        "Shimmer",
        (cx, lid.h * 0.078),
        Type::new(7.2, WHITE, 800.0),
    );
    for (dx, s) in [(-10.0, "Off"), (10.0, "On")] {
        parts::centred(
            b,
            lid.id,
            s,
            (cx + dx, cy + 23.0),
            Type::new(4.6, WHITE, 800.0),
        );
    }
    let (y0, x0, x1) = (cy + 18.0, cx - 15.0, cx + 5.0);
    parts::art(
        b,
        lid.id,
        &[
            vec![
                (x0, y0),
                (x0 + 1.0, y0 - 3.0),
                (x0 + 9.0, y0 - 3.0),
                (x0 + 10.0, y0),
                (x0, y0),
            ],
            vec![(x1, y0 - 2.0), (x1 + 10.0, y0 - 2.0)],
            vec![(x1, y0), (x1 + 10.0, y0)],
        ],
        0.9,
        WHITE,
    );
}

pub(super) fn rinse() -> Face {
    let ks = vec![
        K("rinse/mix", "Mix", (0.16, 0.15), true),
        K("rinse/decay", "Decay", (0.84, 0.15), true),
        K("rinse/low", "", (0.2, 0.345), false),
        K("rinse/high", "", (0.8, 0.345), false),
    ];
    let scene = standard(&Pedal {
        title: "The Rinse Reverb",
        ns: "rinse",
        color: "#3ab4c6",
        knobs: &ks,
        name: &["The", "Rinse"],
        icon: rinse_icon,
        pulse: None,
        extra: Some(shimmer_button),
    });
    // Mix and decay at noon; the low cut turned to the left and the high
    // cut to the right, as in the reference.
    let params = vec![
        parts::pct("rinse/mix", "Mix", 50.0),
        ui::param(
            "rinse/decay",
            "Decay",
            surface_proto::Range::log(0.3, 20.0, surface_proto::Unit::Seconds),
            parts::log_at(0.3, 20.0, 0.5),
        )
        .featured(),
        parts::hz(
            "rinse/low",
            "Low Cut",
            20.0,
            1000.0,
            parts::log_at(20.0, 1000.0, 0.2),
        ),
        parts::hz(
            "rinse/high",
            "High Cut",
            1000.0,
            20000.0,
            parts::log_at(1000.0, 20000.0, 0.8),
        ),
        parts::switch("rinse/shimmer", "Shimmer", false),
        parts::switch("rinse/on", "Bypass", true),
    ];
    face("13-rinse-standard-156x286", scene, params)
}

/// The rinse's icon, plus the filter curves printed under its tone knobs.
fn rinse_icon(b: &mut Builder<'_>, lid: &Lid, at: (f64, f64)) {
    wave_icon(b, lid, at);
    filter_marks(b, lid.id, lid.at(0.2, 0.47), lid.at(0.8, 0.47));
}

/// The low-cut and high-cut curves printed under a pair of knobs.
pub(super) fn filter_marks(b: &mut Builder<'_>, lid: NodeId, lo: (f64, f64), hi: (f64, f64)) {
    parts::art(
        b,
        lid,
        &[
            vec![
                (lo.0 - 8.0, lo.1 + 3.0),
                (lo.0 - 4.0, lo.1 - 2.0),
                (lo.0 + 8.0, lo.1 - 2.0),
            ],
            vec![
                (hi.0 - 8.0, hi.1 - 2.0),
                (hi.0 + 4.0, hi.1 - 2.0),
                (hi.0 + 8.0, hi.1 + 3.0),
            ],
        ],
        1.4,
        WHITE,
    );
}
