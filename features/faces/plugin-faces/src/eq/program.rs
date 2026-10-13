//! The passive program equalizer (after the Pultec EQP-1A), laid out from
//! photographs of the unit: a 3U blue-grey painted panel, slotted screws
//! and rack slots, cream silkscreen.
//!
//! Across the top, the two bands' BOOST and ATTEN — separate controls on
//! the *same* low band, which is the only reason the famous boost-and-cut
//! trick exists — each a black scalloped phenolic knob with bare numerals
//! 0–10 printed round it, and the high-cut ATTEN SEL selector at the right.
//! Below: the IN toggle, the low-frequency lever under CPS, BANDWIDTH from
//! SHARP to BROAD, the high-frequency lever under KCS, the pilot jewel and
//! (decoration) the power switch. Positions are fractions of the panel
//! measured off the photographs.

use frame::document::proto::NodeId;
use frame::surface::kit::{Place, TOUCH_MIN, draw};
use frame::surface::widgets::Builder;

use super::{paint, screw};
use crate::comp_vca::ui::{self, bind};
use crate::face::{Face, canvas};

const W: f64 = 1060.0;
const H: f64 = 292.0;
const INK: &str = "#ebe8da";
const TOP: f64 = 88.0;
const BOTTOM: f64 = 214.0;
const KNOB: f64 = 86.0;

const ZERO_TEN: &[&str] = &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"];
const LOW_FREQS: &[&str] = &["20", "30", "60", "100"];
const HIGH_FREQS: &[&str] = &["3", "4", "5", "8", "10", "12", "16"];

/// A scalloped knob with its bare 0–10 numerals; `legend` printed above.
#[allow(clippy::too_many_arguments)]
fn knob(b: &mut Builder<'_>, panel: NodeId, addr: &str, legend: &str, cx: f64, cy: f64) -> NodeId {
    let p = Place::new(legend)
        .size(KNOB)
        .legend(legend)
        .touch(TOUCH_MIN)
        .bind(bind(addr))
        .scale(ZERO_TEN);
    let k = ui::put(b, panel, "knob.scalloped", &p, cx, cy);
    ui::strip_legend(b, k);
    k
}

/// A frequency lever with its unit over its positions and its band title
/// along the panel's foot.
fn lever(
    b: &mut Builder<'_>,
    panel: NodeId,
    addr: &str,
    title: &str,
    unit: &str,
    options: &[&str],
    cx: f64,
) {
    let p = Place::new(title)
        .size(140.0)
        .legend(unit)
        .variant("light")
        .touch(TOUCH_MIN)
        .bind(bind(addr))
        .options(options);
    ui::put(b, panel, "lever.pultec", &p, cx, BOTTOM + 10.0);
    ui::silk(b, panel, title, cx, H - 14.0, 9.0, INK, 600.0);
}

pub fn face() -> Face {
    let scene = canvas("Program Equalizer", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "pultec", W, H, "none");
        paint(
            b,
            panel,
            &[(0.0, "#6d8a9a"), (0.5, "#617e8e"), (1.0, "#546f7e")],
        );

        // Rack slots at the ends, slotted screws inboard of them.
        for x in [4.0, W - 22.0] {
            for y in [58.0, H - 76.0] {
                let slot = draw::rounded(
                    draw::rect("rack slot", x, y, 18.0, 16.0, draw::solid("#15191c")),
                    8.0,
                );
                b.add(
                    panel,
                    draw::with_effects(slot, vec![draw::inner_shadow("#000000cc", 0.0, 1.5, 3.0)]),
                );
            }
        }
        for (x, y) in [
            (70.0, 50.0),
            (70.0, H - 50.0),
            (W - 70.0, 50.0),
            (W - 70.0, H - 50.0),
        ] {
            screw(b, panel, x, y, 13.0, 20.0);
        }

        // The nameplate, where the unit's maker printed it.
        ui::silk(b, panel, "Program Equalizer", 136.0, 96.0, 10.5, INK, 600.0);
        ui::silk(
            b,
            panel,
            "Passive Type · Valve Make-up",
            136.0,
            112.0,
            7.5,
            INK,
            600.0,
        );
        ui::silk(
            b,
            panel,
            "Low & High Frequency",
            136.0,
            125.0,
            6.5,
            INK,
            600.0,
        );

        // ── Top row: boost and atten, low band then high ───────────────
        for (addr, legend, x) in [
            ("progeq/low_boost", "Boost", 0.285 * W),
            ("progeq/low_atten", "Atten", 0.412 * W),
            ("progeq/high_boost", "Boost", 0.586 * W),
            ("progeq/high_atten", "Atten", 0.713 * W),
        ] {
            knob(b, panel, addr, legend, x, TOP);
            ui::silk(b, panel, legend, x, 12.0, 9.0, INK, 600.0);
        }

        // The high-cut frequency: a small bar knob at the top right.
        ui::silk(b, panel, "Atten Sel", 0.846 * W, 12.0, 9.0, INK, 600.0);
        let sel = Place::new("Atten Sel")
            .size(100.0)
            .variant("light")
            .touch(TOUCH_MIN)
            .bind(bind("progeq/high_atten_freq"))
            .options(&["5", "10", "20"]);
        let s = ui::put(b, panel, "lever.pultec", &sel, 0.846 * W, TOP + 6.0);
        ui::strip_legend(b, s);

        // ── Bottom row ─────────────────────────────────────────────────
        let sw = Place::new("EQ In")
            .size(22.0)
            .variant("black")
            .legend("In")
            .bind(bind("progeq/eq_in"));
        ui::put(b, panel, "switch.toggle_nut", &sw, 0.22 * W, BOTTOM + 2.0);
        lever(
            b,
            panel,
            "progeq/low_freq",
            "Low Frequency",
            "CPS",
            LOW_FREQS,
            0.345 * W,
        );
        knob(
            b,
            panel,
            "progeq/bandwidth",
            "Bandwidth",
            0.497 * W,
            BOTTOM - 8.0,
        );
        ui::silk(b, panel, "Bandwidth", 0.497 * W, H - 14.0, 9.0, INK, 600.0);
        ui::silk(
            b,
            panel,
            "Sharp",
            0.497 * W - 60.0,
            H - 30.0,
            6.5,
            INK,
            600.0,
        );
        ui::silk(
            b,
            panel,
            "Broad",
            0.497 * W + 62.0,
            H - 30.0,
            6.5,
            INK,
            600.0,
        );
        lever(
            b,
            panel,
            "progeq/high_boost_freq",
            "High Frequency",
            "KCS",
            HIGH_FREQS,
            0.652 * W,
        );

        // ── The pilot jewel and the power switch (decoration) ──────────
        let jewel = Place::new("Pilot")
            .size(24.0)
            .variant("red")
            .bind(bind("progeq/eq_in"));
        ui::put(b, panel, "button.jewel", &jewel, 0.80 * W, 0.55 * H);
        ui::silk(b, panel, "Off", 0.83 * W, 0.66 * H, 7.5, INK, 600.0);
        ui::silk(b, panel, "On", 0.885 * W, 0.70 * H, 7.5, INK, 600.0);
        let (px, py) = (0.855 * W, 0.78 * H);
        b.add(
            panel,
            draw::circle(
                "power shadow",
                px + 1.5,
                py + 3.0,
                15.0,
                draw::radial(&[(0.0, "#00000099"), (1.0, "#00000000")]),
            ),
        );
        b.add(
            panel,
            draw::circle(
                "power",
                px,
                py,
                13.0,
                draw::radial_at(
                    0.4,
                    0.32,
                    0.8,
                    &[(0.0, "#3a3a3d"), (0.5, "#161618"), (1.0, "#050506")],
                ),
            ),
        );
        b.add(
            panel,
            draw::rounded(
                draw::rect(
                    "power index",
                    px - 1.2,
                    py - 12.0,
                    2.4,
                    9.0,
                    draw::solid("#e9e4d4"),
                ),
                1.2,
            ),
        );
    });

    let params = vec![
        ui::db("progeq/low_boost", "Low Boost", 0.0, 13.0, 0.0).featured(),
        ui::db("progeq/low_atten", "Low Atten", 0.0, 13.0, 0.0).featured(),
        ui::choice(
            "progeq/low_freq",
            "Low Frequency",
            &["20 Hz", "30 Hz", "60 Hz", "100 Hz"],
            1,
        )
        .featured(),
        ui::db("progeq/high_boost", "High Boost", 0.0, 16.0, 0.0).featured(),
        ui::choice(
            "progeq/high_boost_freq",
            "High Frequency",
            &[
                "3 kHz", "4 kHz", "5 kHz", "8 kHz", "10 kHz", "12 kHz", "16 kHz",
            ],
            3,
        ),
        ui::param(
            "progeq/bandwidth",
            "Bandwidth",
            frame::surface::proto::Range::linear(0.0, 10.0, frame::surface::proto::Unit::None),
            5.0,
        ),
        ui::db("progeq/high_atten", "High Atten", 0.0, 16.0, 0.0),
        ui::choice(
            "progeq/high_atten_freq",
            "Atten Sel",
            &["5 kHz", "10 kHz", "20 kHz"],
            1,
        ),
        ui::toggle("progeq/eq_in", "EQ In", true).featured(),
    ];
    Face {
        file: "01-program-eq",
        plugin: "eq",
        scene,
        params,
        streams: Vec::new(),
    }
}
