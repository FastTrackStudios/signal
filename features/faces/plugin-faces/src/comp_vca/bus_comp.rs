//! The console bus compressor in its 1U rack form — laid out from
//! photographs of the SSL XLogic G-Series rack compressor: a pale
//! silver-grey front, the gain-reduction meter in a tall black housing at
//! the left, then THRESHOLD, ATTACK, RATIO, RELEASE and MAKE-UP on small
//! grey-skirted knobs with blue caps inside printed dot rings, the square
//! backlit COMPRESSOR IN key, and — past hairline dividers — the external
//! side-chain key and the auto-fade key with its RATE knob.
//!
//! RATIO, ATTACK and RELEASE are rotary *switches*: they click through their
//! printed detents (2 · 4 · 10, 0.1 → 30 ms, 0.1 s → AUTO) and turn exactly
//! as far as those detents go. The meter's needle rests at the left stop
//! and swings right as the reduction deepens (0 → 20 dB).

use frame::document::proto::{NodeId, TextAlign};
use frame::surface::kit::{Ink, Place, TOUCH_MIN, draw};
use frame::surface::proto::{Binding, Unit};
use frame::surface::widgets::Builder;

use super::logo;
use super::ui::{self, bind};
use crate::face::{Face, canvas};

pub mod lane;

const W: f64 = 1480.0;
const H: f64 = 130.0;
/// Silkscreen: black on the grey front.
const INK: &str = "#17191c";
const ROW: f64 = 72.0;
const LABEL: f64 = 15.0;
const KNOB: f64 = 40.0;

const RATIOS: &[&str] = &["2", "4", "10"];
const ATTACKS: &[&str] = &[".1", ".3", "1", "3", "10", "30"];
const RELEASES: &[&str] = &[".1", ".3", ".6", "1.2", "Auto"];
/// The auto-fade RATE knob's printing, seconds.
const RATES: &[&str] = &["1", "3", "6", "10", "20", "40", "60"];
const RATE_VALUES: &[f64] = &[1.0, 3.0, 6.0, 10.0, 20.0, 40.0, 60.0];

/// A position across the front, as a fraction of its width (measured off
/// the photograph).
fn fx(f: f64) -> f64 {
    W * f
}

/// A capped-skirt knob with its header above it.
fn knob(b: &mut Builder<'_>, panel: NodeId, p: Place, header: &str, cx: f64) -> NodeId {
    ui::silk(b, panel, header, cx, LABEL, 9.0, INK, 800.0);
    let p = p
        .size(KNOB)
        .variant("blue")
        .ink(Ink::Color(INK.into()))
        .touch(TOUCH_MIN)
        .keep_case();
    let k = ui::put(b, panel, "knob.capped_skirt", &p, cx, ROW);
    ui::strip_legend(b, k);
    k
}

/// A rotary switch through printed detents.
fn switch(b: &mut Builder<'_>, panel: NodeId, addr: &str, header: &str, labels: &[&str], cx: f64) {
    knob(
        b,
        panel,
        Place::new(header).options(labels).bind(bind(addr)),
        header,
        cx,
    );
}

/// A square backlit key in its raised grey bezel, with a header above it.
fn key(b: &mut Builder<'_>, panel: NodeId, addr: &str, header: &[&str], legend: &str, cx: f64) {
    let n = header.len() as f64;
    for (i, l) in header.iter().enumerate().filter(|(_, l)| !l.is_empty()) {
        ui::silk(
            b,
            panel,
            l,
            cx,
            LABEL - 4.0 * (n - 1.0) + i as f64 * 10.0,
            9.0,
            INK,
            800.0,
        );
    }
    let s = 38.0;
    let bz = draw::rect(
        "key bezel",
        cx - s * 0.66,
        ROW - s * 0.66,
        s * 1.32,
        s * 1.32,
        draw::linear(
            180.0,
            &[(0.0, "#e6e8ea"), (0.5, "#c3c6ca"), (1.0, "#9ea2a7")],
        ),
    );
    b.add(
        panel,
        draw::with_effects(
            draw::stroked(draw::rounded(bz, 2.5), "#00000055", 1.0),
            vec![
                draw::drop_shadow("#00000059", 0.0, 1.5, 3.0),
                draw::inner_shadow("#ffffffa0", 0.0, 1.0, 0.0),
            ],
        ),
    );
    let p = Place::new(legend)
        .size(s)
        .variant("white")
        .legend(legend)
        .bind(bind(addr));
    ui::put(b, panel, "button.square", &p, cx, ROW);
}

/// A thin dark divider down the front.
fn divider(b: &mut Builder<'_>, panel: NodeId, x: f64) {
    ui::rule(b, panel, x, 10.0, 1.0, H - 18.0, "#00000080");
    ui::rule(b, panel, x + 1.0, 10.0, 1.0, H - 18.0, "#ffffff66");
}

pub fn face() -> Face {
    let scene = canvas("Bus Compressor", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "api", W, H, "ears");
        // The unit's paint: a pale, faintly cool silver-grey.
        if let Some(n) = b.scene.node_mut(panel) {
            n.fills = vec![draw::linear(
                178.0,
                &[(0.0, "#dde1e5"), (0.5, "#cdd1d6"), (1.0, "#b4b9bf")],
            )];
        }

        // ── The maker's corner: the name stacked in three lines, as the
        // unit stacks its maker's ─────────────────────────────────────
        let stack = logo::Style {
            size: 15.0,
            weight: 600.0,
            italic: false,
            tracking: -0.01,
        };
        for (i, l) in ["Stereo", "Bus", "Comp"].iter().enumerate() {
            logo::mark(
                b,
                panel,
                l,
                52.0,
                50.0 + i as f64 * 16.0,
                80.0,
                stack,
                INK,
                TextAlign::Left,
            );
        }

        // ── Gain reduction: a tall black housing ───────────────────────
        let meter = Place::new("Compression")
            .size(148.0)
            .legend("Compression")
            .options(&["glass"])
            .bind(Binding::stream("buscomp/gr"));
        ui::put(
            b,
            panel,
            "meter.gr_compression",
            &meter,
            fx(0.1435),
            H / 2.0,
        );

        // ── The five controls ─────────────────────────────────────────
        knob(
            b,
            panel,
            Place::new("Threshold")
                .scale(&["-30", "", "", "-15", "", "", "0"])
                .bind(bind("buscomp/threshold")),
            "Threshold",
            fx(0.249),
        );
        switch(
            b,
            panel,
            "buscomp/attack",
            "Attack - mS",
            ATTACKS,
            fx(0.329),
        );
        switch(b, panel, "buscomp/ratio", "Ratio", RATIOS, fx(0.409));
        switch(
            b,
            panel,
            "buscomp/release",
            "Release - S",
            RELEASES,
            fx(0.490),
        );
        knob(
            b,
            panel,
            Place::new("Make-up")
                .scale(&["0", "", "", "9", "", "", "18"])
                .bind(bind("buscomp/makeup")),
            "Make-up",
            fx(0.572),
        );

        // ── The keys and the auto fade ─────────────────────────────────
        key(b, panel, "buscomp/in", &["Compressor"], "In", fx(0.652));
        divider(b, panel, fx(0.692));
        key(
            b,
            panel,
            "buscomp/ext_sc",
            &["External", "Side - Chain"],
            "In",
            fx(0.733),
        );
        divider(b, panel, fx(0.772));
        key(
            b,
            panel,
            "buscomp/autofade",
            &["Auto Fade"],
            "In",
            fx(0.813),
        );
        let rate = Place::new("Rate")
            .scale(RATES)
            .bind(bind("buscomp/fade_rate"));
        knob(b, panel, rate, "Rate - S", fx(0.894));
        // The line tying AUTO FADE to its RATE, under both.
        let (l, r, y) = (fx(0.813), fx(0.894), ROW + 36.0);
        ui::rule(b, panel, l, ROW + 26.0, 1.0, 10.0, INK);
        ui::rule(b, panel, l, y, r - l + 1.0, 1.0, INK);
        ui::rule(b, panel, r, ROW + 26.0, 1.0, 10.0, INK);

        // ── The series badge at the right, set as the unit's is: a big
        // slanted word over a rule, spaced small capitals, and a black tag
        // reversed out in white.
        let bx = fx(0.953);
        let big = logo::Style {
            size: 24.0,
            weight: 700.0,
            italic: true,
            tracking: -0.02,
        };
        logo::mark(
            b,
            panel,
            "Bus",
            bx - 36.0,
            56.0,
            72.0,
            big,
            INK,
            TextAlign::Center,
        );
        ui::rule(b, panel, bx - 34.0, 70.0, 68.0, 1.2, INK);
        ui::silk(b, panel, "Stereo", bx, 76.0, 6.0, INK, 600.0);
        b.add(
            panel,
            draw::rect("badge tag", bx - 34.0, 81.0, 68.0, 9.0, draw::solid(INK)),
        );
        ui::silk(b, panel, "Compressor", bx, 86.0, 5.8, "#dde1e5", 700.0);
    });

    let params = vec![
        ui::db("buscomp/threshold", "Threshold", -30.0, 0.0, -12.0).featured(),
        ui::choice("buscomp/ratio", "Ratio", &["2:1", "4:1", "10:1"], 1).featured(),
        ui::choice(
            "buscomp/attack",
            "Attack",
            &["0.1 ms", "0.3 ms", "1 ms", "3 ms", "10 ms", "30 ms"],
            3,
        ),
        ui::choice(
            "buscomp/release",
            "Release",
            &["0.1 s", "0.3 s", "0.6 s", "1.2 s", "Auto"],
            4,
        ),
        ui::db("buscomp/makeup", "Make-up", 0.0, 18.0, 4.0).featured(),
        ui::toggle("buscomp/in", "Compressor In", true).featured(),
        ui::toggle("buscomp/ext_sc", "External Side-Chain", false),
        ui::toggle("buscomp/autofade", "Auto Fade", false),
        ui::param(
            "buscomp/fade_rate",
            "Fade Rate",
            ui::printed(RATE_VALUES, Unit::Seconds),
            10.0,
        ),
    ];
    let streams = vec![ui::gr_stream("buscomp/gr", "Gain Reduction")];
    Face {
        file: "08-bus-comp",
        plugin: "comp",
        scene,
        params,
        streams,
    }
}
