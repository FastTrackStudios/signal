//! The three-band proportional-Q EQ module (after the API 550A), laid out
//! from photographs of the 500-series module: a narrow black panel, the
//! three bands stacked HF over MF over LF, each a concentric pair — the
//! cream winged knob sets boost/cut in the module's uneven clicks (2, 4, 6,
//! 9, 12 dB, printed in white round it) and the blue three-lobed ring in it
//! picks one of seven frequencies (printed in blue on a wider ring) — then
//! the FLTR toggle, the IN lamp and the three round push caps: low shelf or
//! peak, high shelf or peak, IN.
//!
//! The module is a tall strip; the face keeps its column but widens it a
//! little and closes up the bands, so it opens at a plugin-sized portrait.

use document_proto::NodeId;
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{ParamInfo, Unit};
use surface_widgets::Builder;

use super::paint;
use crate::comp_vca::ui::{self, Glyph, bind};
use crate::face::{Face, canvas};

const W: f64 = 380.0;
const H: f64 = 950.0;
const WHITE: &str = "#f2f2ef";
const BLUE: &str = "#3fb1ec";
const KNOB: f64 = 100.0;
const BANDS: [f64; 3] = [164.0, 392.0, 620.0];

const GAIN: &[&str] = &["12", "9", "6", "4", "2", "", "2", "4", "6", "9", "12"];
const GAIN_DB: &[f64] = &[-12.0, -9.0, -6.0, -4.0, -2.0, 0.0, 2.0, 4.0, 6.0, 9.0, 12.0];
const LOW: &[&str] = &["30", "40", "50", "100", "200", "300", "400"];
const MID: &[&str] = &[".2", ".4", ".6", ".8", "1.5", "3", "5"];
const HIGH: &[&str] = &["2.5", "5", "7", "10", "12.5", "15", "20"];

/// One band: the winged gain knob with the blue frequency ring in it, the
/// band's name and unit on the row under it, − and + over it.
#[allow(clippy::too_many_arguments)]
fn band(b: &mut Builder<'_>, panel: NodeId, band: &str, unit: &str, freqs: &[&str], cy: f64) {
    let lower = band.to_lowercase();
    let freq = Place::new(format!("{band} Freq"))
        .options(freqs)
        .bind(bind(&format!("propeq/{lower}_freq")));
    let p = Place::new(format!("{band} Gain"))
        .size(KNOB)
        .scale(GAIN)
        .touch(TOUCH_MIN)
        .bind(bind(&format!("propeq/{lower}_gain")))
        .inner(freq);
    let root = ui::put(b, panel, "knob.winged_pair", &p, W / 2.0, cy);
    // The module prints its own captions: no legends from the kit.
    let pair: Vec<NodeId> = b
        .scene
        .walk(root)
        .into_iter()
        .filter(|id| b.controls.contains_key(id))
        .collect();
    for c in pair {
        ui::strip_legend(b, c);
    }
    let name = if band == "Mid" {
        "MF"
    } else if band == "Low" {
        "LF"
    } else {
        "HF"
    };
    ui::silk_left(b, panel, name, 18.0, cy + KNOB * 1.12, 13.0, WHITE, 500.0);
    ui::plain(
        b,
        panel,
        unit,
        W - 34.0,
        cy + KNOB * 1.12,
        13.0,
        BLUE,
        600.0,
    );
    ui::plain(
        b,
        panel,
        "−",
        W / 2.0 - KNOB * 0.5,
        cy - KNOB * 1.02,
        16.0,
        WHITE,
        600.0,
    );
    ui::plain(
        b,
        panel,
        "+",
        W / 2.0 + KNOB * 0.5,
        cy - KNOB * 1.02,
        16.0,
        WHITE,
        600.0,
    );
}

/// Gain in the module's clicks: 2, 4, 6, 9, 12 dB each way.
fn gain(addr: &str, name: &str) -> ParamInfo {
    let mut r = ui::printed(GAIN_DB, Unit::Db);
    r.steps = Some(GAIN_DB.len() as u32);
    ui::param(addr, name, r, 0.0).bipolar().featured()
}

/// A round push cap; its caption is printed by the face.
fn push(b: &mut Builder<'_>, panel: NodeId, addr: &str, name: &str, lamp: bool, cx: f64) -> NodeId {
    let opts = if lamp {
        vec!["color=#ff2a1a".to_string()]
    } else {
        vec!["nolamp".to_string()]
    };
    let p = Place {
        options: Some(opts),
        ..Place::new(name)
            .size(58.0)
            .variant("cream")
            .legend(name)
            .bind(bind(addr))
    };
    // The cap's centre sits `1.5 d` (lamp) or `0.55 d` below the box top.
    let top = if lamp { 1.5 * 58.0 } else { 0.55 * 58.0 };
    let w = 58.0 * 1.2;
    let root = ui::put_at(b, panel, "button.round_cap", &p, cx - w / 2.0, 830.0 - top);
    if !lamp {
        ui::strip_legend(b, root);
    }
    root
}

pub fn face() -> Face {
    let scene = canvas("Proportional EQ", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "black", W, H, "none");
        paint(
            b,
            panel,
            &[(0.0, "#1f1f22"), (0.5, "#19191b"), (1.0, "#141416")],
        );

        // The rack screws, top and bottom centre, in their holes.
        for y in [26.0, H - 26.0] {
            b.add(
                panel,
                draw::with_effects(
                    draw::circle("screw hole", W / 2.0, y, 13.0, draw::solid("#050506")),
                    vec![draw::inner_shadow("#000000", 0.0, 1.0, 3.0)],
                ),
            );
            b.add(
                panel,
                draw::circle(
                    "screw",
                    W / 2.0,
                    y,
                    8.5,
                    draw::radial_at(0.38, 0.3, 0.85, &[(0.0, "#5b5c5f"), (1.0, "#101012")]),
                ),
            );
            b.add(
                panel,
                draw::rect(
                    "recess",
                    W / 2.0 - 5.5,
                    y - 1.0,
                    11.0,
                    2.0,
                    draw::solid("#000000"),
                ),
            );
            b.add(
                panel,
                draw::rect(
                    "recess",
                    W / 2.0 - 1.0,
                    y - 5.5,
                    2.0,
                    11.0,
                    draw::solid("#000000"),
                ),
            );
        }
        ui::silk_left(b, panel, "EQ", 22.0, 30.0, 15.0, BLUE, 600.0);
        ui::silk_right(b, panel, "3 Band", W - 20.0, 30.0, 12.0, BLUE, 600.0);

        band(b, panel, "High", "kHz", HIGH, BANDS[0]);
        band(b, panel, "Mid", "kHz", MID, BANDS[1]);
        band(b, panel, "Low", "Hz", LOW, BANDS[2]);

        // ── Filter toggle, the switching ───────────────────────────────
        let fl = Place {
            options: Some(vec!["legend_right".into()]),
            ..Place::new("Filter")
                .size(26.0)
                .variant("chrome")
                .legend("Fltr")
                .bind(bind("propeq/filter"))
        };
        ui::put(b, panel, "switch.toggle_nut", &fl, 110.0, 758.0);
        push(b, panel, "propeq/low_peak", "LF Peak", false, 74.0);
        push(b, panel, "propeq/high_peak", "HF Peak", false, W / 2.0);
        let inb = push(b, panel, "propeq/eq_in", "In", true, W - 74.0);
        let _ = inb;
        for (x, g, letters) in [
            (74.0, Glyph::LowShelf, ("L", "F")),
            (W / 2.0, Glyph::HighShelf, ("H", "F")),
        ] {
            ui::silk(b, panel, letters.0, x - 20.0, 886.0, 14.0, WHITE, 800.0);
            ui::glyph(b, panel, g, x, 886.0, 22.0, WHITE);
            ui::silk(b, panel, letters.1, x + 21.0, 886.0, 14.0, WHITE, 800.0);
        }
    });

    let params = vec![
        ui::choice(
            "propeq/low_freq",
            "Low Freq",
            &[
                "30 Hz", "40 Hz", "50 Hz", "100 Hz", "200 Hz", "300 Hz", "400 Hz",
            ],
            3,
        ),
        gain("propeq/low_gain", "Low Gain"),
        ui::choice(
            "propeq/mid_freq",
            "Mid Freq",
            &[
                "200 Hz", "400 Hz", "600 Hz", "800 Hz", "1.5 kHz", "3 kHz", "5 kHz",
            ],
            4,
        ),
        gain("propeq/mid_gain", "Mid Gain"),
        ui::choice(
            "propeq/high_freq",
            "High Freq",
            &[
                "2.5 kHz", "5 kHz", "7 kHz", "10 kHz", "12.5 kHz", "15 kHz", "20 kHz",
            ],
            3,
        ),
        gain("propeq/high_gain", "High Gain"),
        ui::toggle("propeq/low_peak", "LF Peak", false),
        ui::toggle("propeq/high_peak", "HF Peak", false),
        ui::toggle("propeq/filter", "Filter", false),
        ui::toggle("propeq/eq_in", "EQ In", true).featured(),
    ];
    Face {
        file: "03-proportional-eq",
        plugin: "eq",
        scene,
        params,
        streams: Vec::new(),
    }
}
