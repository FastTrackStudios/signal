//! The class-A preamp/EQ module (after the Neve 1073), laid out from
//! photographs of its horizontal rack version, in the module's blue-grey
//! paint with white legends.
//!
//! Left to right, as the rack unit has it: the red gain selector with its
//! dotted scale; the fixed 12 kHz shelf, a grey fluted turret in a clear
//! skirt; the mid and low bands as **concentric pairs** — a knurled
//! aluminium collar clicking through the stepped frequencies (OFF · ·36 …
//! 7·2 kHz, OFF · 35 … 220 Hz) with the band's gain on the grey turret in
//! it; the blue high-pass selector; and the two cream keys, phase over EQ.
//! Each band carries the shape the module prints over it instead of a name.
//! The colour is the index: red gain, blue filter, grey bands.
//!
//! The module itself is a tall strip; the rack version's single row is the
//! layout that fits a plugin window, so that is the one followed here.

use frame::document::proto::NodeId;
use frame::surface::kit::{Ink, Place, TOUCH_MIN, draw};
use frame::surface::widgets::Builder;

use super::{paint, screw};
use crate::comp_vca::ui::{self, Glyph, bind};
use crate::face::{Face, canvas};

const W: f64 = 1150.0;
const H: f64 = 252.0;
const INK: &str = "#eef1f4";
const ROW: f64 = 138.0;
const GLYPH_Y: f64 = 26.0;
const UNIT_Y: f64 = 238.0;

const MID_FREQS: &[&str] = &["Off", "·36", "·7", "1·6", "3·2", "4·8", "7·2"];
const LOW_FREQS: &[&str] = &["Off", "35", "60", "110", "220"];
const HPF_FREQS: &[&str] = &["Off", "50", "80", "160", "300"];

fn ink() -> Ink {
    Ink::Color(INK.into())
}

/// A concentric pair: a stepped frequency collar with the gain turret in
/// it — the turret's control nested in the collar's, so each has its disc.
fn pair(
    b: &mut Builder<'_>,
    panel: NodeId,
    freq: &str,
    gain: &str,
    labels: &[&str],
    unit: &str,
    cx: f64,
) {
    let cap = Place::new(format!("{unit} gain")).bind(bind(gain));
    let p = Place::new(format!("{unit} frequency"))
        .size(96.0)
        .legend(unit)
        .keep_case()
        .options(labels)
        .ink(ink())
        .touch(TOUCH_MIN)
        .bind(bind(freq))
        .inner(cap);
    let collar = ui::put(b, panel, "knob.turret_pair", &p, cx, ROW);
    let all: Vec<NodeId> = b
        .scene
        .walk(collar)
        .into_iter()
        .filter(|id| b.controls.contains_key(id))
        .collect();
    for c in all {
        ui::strip_legend(b, c);
    }
    ui::plain(b, panel, unit, cx, UNIT_Y, 11.0, INK, 600.0);
}

/// A turret (grey) or wing selector (red / blue) with its dotted scale.
#[allow(clippy::too_many_arguments)]
fn turret(
    b: &mut Builder<'_>,
    panel: NodeId,
    addr: &str,
    name: &str,
    variant: &str,
    d: f64,
    scale: &[&str],
    stepped: bool,
    cx: f64,
) -> NodeId {
    let p = Place::new(name)
        .size(d)
        .variant(variant)
        .legend(name)
        .keep_case()
        .ink(ink())
        .touch(TOUCH_MIN)
        .bind(bind(addr));
    let p = if stepped {
        p.options(scale)
    } else {
        p.scale(scale)
    };
    let k = ui::put(b, panel, "knob.turret", &p, cx, ROW);
    ui::strip_legend(b, k);
    k
}

/// A cream key on the right-hand strip.
fn key(b: &mut Builder<'_>, panel: NodeId, addr: &str, name: &str, legend: &str, cy: f64) {
    let p = Place {
        options: Some(vec!["nolamp".into(), "h=40".into()]),
        ..Place::new(name)
            .size(44.0)
            .variant("cream")
            .legend(legend)
            .bind(bind(addr))
    };
    ui::put(b, panel, "button.key", &p, 0.893 * W, cy);
}

pub fn face() -> Face {
    let scene = canvas("Class-A Preamp EQ", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "neve", W, H, "none");
        paint(
            b,
            panel,
            &[(0.0, "#56626f"), (0.5, "#4a5561"), (1.0, "#3e4854")],
        );
        for (x, y) in [
            (22.0, 26.0),
            (22.0, H - 26.0),
            (W - 22.0, 26.0),
            (W - 22.0, H - 26.0),
        ] {
            screw(b, panel, x, y, 15.0, 35.0);
        }
        b.add(
            panel,
            draw::with_effects(
                draw::circle("hole", W - 36.0, H / 2.0, 6.0, draw::solid("#0c0e11")),
                vec![draw::inner_shadow("#000000", 0.0, 1.0, 2.0)],
            ),
        );

        // ── Gain: the red selector, the biggest thing on the panel ─────
        let g = turret(
            b,
            panel,
            "classaeq/drive",
            "Gain",
            "red",
            108.0,
            &["0", "", "2", "", "4", "", "6", "", "8", "", "10"],
            false,
            0.135 * W,
        );
        let _ = g;
        ui::plain(b, panel, "dB", 0.135 * W, UNIT_Y, 11.0, INK, 600.0);

        // ── High shelf, fixed at 12 kHz ────────────────────────────────
        ui::glyph(b, panel, Glyph::HighShelf, 0.30 * W, GLYPH_Y, 36.0, INK);
        turret(
            b,
            panel,
            "classaeq/high_gain",
            "12 kHz",
            "grey",
            92.0,
            &["-16", "", "-8", "", "0", "", "+8", "", "+16"],
            false,
            0.30 * W,
        );
        ui::plain(b, panel, "12 kHz", 0.30 * W, UNIT_Y, 11.0, INK, 600.0);

        // ── Mid and low: concentric pairs ──────────────────────────────
        ui::glyph(b, panel, Glyph::Bell, 0.46 * W, GLYPH_Y, 36.0, INK);
        pair(
            b,
            panel,
            "classaeq/mid_freq",
            "classaeq/mid_gain",
            MID_FREQS,
            "kHz",
            0.46 * W,
        );
        ui::glyph(b, panel, Glyph::LowShelf, 0.62 * W, GLYPH_Y, 36.0, INK);
        pair(
            b,
            panel,
            "classaeq/low_freq",
            "classaeq/low_gain",
            LOW_FREQS,
            "Hz",
            0.62 * W,
        );

        // ── High pass: the blue selector, stepped ─────────────────────
        ui::glyph(b, panel, Glyph::HighPass, 0.78 * W, GLYPH_Y, 36.0, INK);
        turret(
            b,
            panel,
            "classaeq/hpf",
            "High Pass",
            "blue",
            96.0,
            HPF_FREQS,
            true,
            0.78 * W,
        );
        ui::plain(b, panel, "Hz", 0.78 * W, UNIT_Y, 11.0, INK, 600.0);

        // ── Phase over EQ, on cream keys ───────────────────────────────
        key(b, panel, "classaeq/phase", "Phase", "Ø", ROW - 42.0);
        key(b, panel, "classaeq/eq_in", "EQ", "EQ", ROW + 22.0);
    });

    let params = vec![
        ui::percent("classaeq/drive", "Gain", 0.0).featured(),
        ui::db("classaeq/high_gain", "High Gain", -16.0, 16.0, 0.0).featured(),
        ui::choice(
            "classaeq/mid_freq",
            "Mid Freq",
            &[
                "Off", "360 Hz", "700 Hz", "1.6 kHz", "3.2 kHz", "4.8 kHz", "7.2 kHz",
            ],
            0,
        )
        .featured(),
        ui::db("classaeq/mid_gain", "Mid Gain", -18.0, 18.0, 0.0).featured(),
        ui::choice(
            "classaeq/low_freq",
            "Low Freq",
            &["Off", "35 Hz", "60 Hz", "110 Hz", "220 Hz"],
            0,
        ),
        ui::db("classaeq/low_gain", "Low Gain", -16.0, 16.0, 0.0),
        ui::choice(
            "classaeq/hpf",
            "High Pass",
            &["Off", "50 Hz", "80 Hz", "160 Hz", "300 Hz"],
            0,
        ),
        ui::toggle("classaeq/phase", "Phase", false),
        ui::toggle("classaeq/eq_in", "EQ In", true).featured(),
    ];
    Face {
        file: "04-classa-eq",
        plugin: "eq",
        scene,
        params,
        streams: Vec::new(),
    }
}
