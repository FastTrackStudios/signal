//! The post-FX stand: two rack units (1140 wide, heights from the stand's
//! own aspect).
//!
//! - `10-echo-rack-1140x350`: *ECHO* — a pale grey tape echo in four
//!   bays: tall E C H O letters over a VU printed on the panel and the
//!   delay-time / tempo display; the three active-head lamps over the big
//!   MODE SELECTOR, the FREE / DAW / TAP sync lever; eight knobs (TIME,
//!   FEEDBACK, TAPE-MOD, MIX; TAPE DRIVE, BASS and TREBLE in orange, WIDTH)
//!   and the lit TAP key; the brush mark, the emblem and the orange POWER.
//! - `11-reverb-rack-1140x210`: *REVERB* — dark grey: tall letters, a
//!   display (mode, pre-delay, decay, filters, mix — each bar a slim slider
//!   of its param), the HALL / PLATE toggle, five pill faders and POWER.

use frame::document::proto::{NodeId, TextAlign};
use frame::surface::kit::meters::scale::Card;
use frame::surface::kit::{Place, TOUCH_MIN, draw};
use frame::surface::proto::{Binding, Control, ControlKind, Orientation, SlotRole, ValueFormat};
use frame::surface::widgets::Builder;

use super::look::{self, KnobLook};
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::suite_funk_rhythm::parts::{self, RACK_W, Type};

const ORANGE: &str = "#ee8a1e";
const INK: &str = "#1f2023";
const DIVS: &[&str] = &["1/16", "1/8T", "1/8", "1/8D", "1/4T", "1/4", "1/4D", "1/2"];

fn panel(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    fill: &[(f64, &str)],
) -> NodeId {
    let p = look::slab(
        name,
        (x, y, w, h),
        3.0,
        vec![
            draw::linear(180.0, fill),
            draw::linear(
                90.0,
                &[(0.0, "#ffffff00"), (0.5, "#ffffff10"), (1.0, "#ffffff00")],
            ),
        ],
    );
    b.add(
        parent,
        draw::with_effects(
            p,
            vec![
                draw::inner_shadow("#ffffff80", 0.0, 1.0, 0.5),
                draw::drop_shadow("#00000040", 0.0, 1.0, 2.0),
            ],
        ),
    )
}

fn tall_letters(
    b: &mut Builder<'_>,
    parent: NodeId,
    word: &str,
    cx: f64,
    y0: f64,
    step: f64,
    t: Type<'_>,
) {
    for (i, ch) in word.chars().enumerate() {
        parts::centred(b, parent, &ch.to_string(), (cx, y0 + step * i as f64), t);
    }
}

/// The VU printed on the panel: a `Gauge` on the input level whose needle
/// turns about (`px`, `py`), scale radius `r`. The scale is a real VU
/// card's law (`meters::scale::Card::Vu`: −20 crowded at the left, 0 VU at
/// about 70 % of the travel), and the binding carries the same law, so
/// the needle lands on the printed marks. Under the dB scale, the
/// percentage-of-0-VU scale (0 … 100 %).
fn vu(b: &mut Builder<'_>, parent: NodeId, (px, py): (f64, f64), r: f64) {
    let card = Card::Vu;
    let sweep = 100.0;
    let (floor, ceil) = card.range();
    let side = 2.0 * (r + 26.0);
    let c = side / 2.0;
    let root = b.add(parent, draw::frame("VU", px - c, py - c, side, side));
    let at = |db: f64| (card.fraction(db) - 0.5) * sweep;
    // The arc, its ticks and numerals, orange past 0.
    let arc: Vec<(f64, f64)> = (0..=60)
        .map(|i| draw::polar(c, c, r, at(floor + (ceil - floor) * f64::from(i) / 60.0)))
        .collect();
    b.add(root, ui::strokes("arc", &[arc], 1.4, draw::solid(INK)));
    let hot: Vec<(f64, f64)> = (0..=16)
        .map(|i| draw::polar(c, c, r + 4.0, at(3.0 * f64::from(i) / 16.0)))
        .collect();
    b.add(root, ui::strokes("over", &[hot], 8.0, draw::solid(ORANGE)));
    let numeral = Type::new(12.5, INK, 400.0).family(draw::family::SAIRA);
    for (db, s) in [
        (-20.0, "-20"),
        (-10.0, "10"),
        (-7.0, "7"),
        (-5.0, "5"),
        (-3.0, "3"),
        (-2.0, "2"),
        (-1.0, "1"),
        (0.0, "0"),
        (1.0, "1"),
        (2.0, "2"),
        (3.0, "3+"),
    ] {
        let a = at(db);
        let col = if db >= 0.0 { ORANGE } else { INK };
        b.add(
            root,
            ui::strokes(
                "tick",
                &[vec![draw::polar(c, c, r, a), draw::polar(c, c, r + 9.0, a)]],
                1.6,
                draw::solid(col),
            ),
        );
        let (x, y) = draw::polar(c, c, r + 19.0, a);
        parts::centred(
            b,
            root,
            s,
            (x, y),
            Type {
                color: col,
                ..numeral
            },
        );
    }
    // The inner percentage scale: 100 % = 0 VU, placed by the same law.
    for pc in [20.0_f64, 40.0, 60.0, 80.0, 100.0] {
        let db = 20.0 * (pc / 100.0).log10();
        let a = at(db);
        b.add(
            root,
            ui::strokes(
                "pc tick",
                &[vec![
                    draw::polar(c, c, r - 2.0, a),
                    draw::polar(c, c, r - 6.0, a),
                ]],
                1.0,
                draw::solid(INK),
            ),
        );
        let (x, y) = draw::polar(c, c, r - 13.0, a);
        let label = if pc >= 100.0 {
            "100%".to_string()
        } else {
            format!("{pc:.0}")
        };
        parts::centred(b, root, &label, (x, y), Type::new(6.5, INK, 500.0));
    }
    parts::centred(
        b,
        root,
        "VU",
        (c, c - r * 0.5),
        Type::new(24.0, INK, 400.0)
            .family(draw::family::SAIRA)
            .width(85.0),
    );
    // The needle, drawn up at mid travel, turning about the pivot.
    let needle = b.add(root, draw::frame("needle", 0.0, 0.0, side, side));
    b.add(
        needle,
        draw::with_effects(
            draw::rect(
                "blade",
                c - 1.2,
                c - r - 4.0,
                2.4,
                r + 4.0,
                draw::solid("#1a1a1c"),
            ),
            vec![draw::drop_shadow("#00000040", 2.0, 3.0, 2.0)],
        ),
    );
    let base = draw::rounded(
        draw::rect(
            "pivot cover",
            c - 16.0,
            c - 18.0,
            32.0,
            46.0,
            draw::linear(
                90.0,
                &[(0.0, "#2a2a2c"), (0.5, "#3c3c40"), (1.0, "#141416")],
            ),
        ),
        12.0,
    );
    b.add(
        root,
        draw::with_effects(base, vec![draw::drop_shadow("#00000060", 0.0, 3.0, 4.0)]),
    );
    let mut binding = Binding::stream("echo/in");
    binding.curve = card.curve();
    let mut ctl = Control::new(ControlKind::Gauge {
        sweep_degrees: sweep,
        floor_db: floor,
        ceiling_db: ceil,
    })
    .slot(SlotRole::Needle, needle.0);
    ctl.binding = Some(binding);
    ctl.reference_dbfs = Some(frame::surface::proto::VU_REFERENCE_DBFS);
    b.controls.insert(root, ctl);
}

fn screws(b: &mut Builder<'_>, parent: NodeId, pts: &[(f64, f64)]) {
    for (x, y) in pts {
        b.add(
            parent,
            draw::circle(
                "screw",
                *x,
                *y,
                6.0,
                draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#f4f4f4"), (1.0, "#8a8b8d")]),
            ),
        );
        b.add(
            parent,
            draw::rect("cross", x - 4.0, y - 0.6, 8.0, 1.2, draw::solid("#3a3a3c")),
        );
        b.add(
            parent,
            draw::rect("cross", x - 0.6, y - 4.0, 1.2, 8.0, draw::solid("#3a3a3c")),
        );
    }
}

fn glow_key(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    d: f64,
    at: (f64, f64),
    color: &str,
    momentary: bool,
) -> NodeId {
    let mut opts = vec![format!("color={color}")];
    if momentary {
        opts.push("momentary".into());
    }
    let p = Place {
        options: Some(opts),
        ..Place::new(addr)
            .size(d)
            .bind(ui::bind(addr))
            .touch(TOUCH_MIN)
    };
    parts::part(b, parent, "button.glow_key", p, &["bezel"], at)
}

pub(super) fn echo() -> Face {
    let (w, h) = (RACK_W, 350.0);
    let grey = [(0.0, "#e6e7e8"), (1.0, "#cfd0d2")];
    let scene = canvas("Echo", |b, root| {
        parts::page(b, root, w, h);
        // The top lip of the stand, and the unit.
        let unit = panel(
            b,
            root,
            "Unit",
            (0.0, 0.0, w, h),
            &[(0.0, "#bfc0c2"), (1.0, "#a9aaac")],
        );
        let lip = draw::path(
            "lip",
            0.0,
            0.0,
            w,
            30.0,
            draw::polygon(&[(0.0, 0.0), (w, 0.0), (w - 26.0, 28.0), (26.0, 28.0)]),
            draw::linear(180.0, &[(0.0, "#d8d9db"), (1.0, "#8e8f91")]),
        );
        b.add(unit, lip);
        let t = Type::new(10.5, INK, 500.0).track(0.08).upper();
        // Bay 1: letters, VU, display.
        let bay1 = panel(b, unit, "Bay 1", (28.0, 30.0, 354.0, 314.0), &grey);
        tall_letters(
            b,
            bay1,
            "ECHO",
            42.0,
            58.0,
            70.0,
            Type::new(62.0, INK, 300.0)
                .family(draw::family::SAIRA)
                .width(88.0),
        );
        vu(b, bay1, (212.0, 147.0), 100.0);
        let disp = ui::display(
            b, bay1, "Display", 118.0, 234.0, 204.0, 50.0, "#9aa0a8", "glass",
        );
        ui::lit_text(
            b,
            disp,
            "DELAY TIME",
            10.0,
            15.0,
            100.0,
            9.5,
            "#c9ccd2",
            TextAlign::Left,
            500.0,
        );
        ui::lit_text(
            b,
            disp,
            "TEMPO",
            104.0,
            15.0,
            92.0,
            9.5,
            "#c9ccd2",
            TextAlign::Right,
            500.0,
        );
        ui::lit_readout(
            b,
            disp,
            "Delay time",
            ui::bind("echo/time"),
            10.0,
            36.0,
            80.0,
            9.5,
            "#e8eaee",
            TextAlign::Left,
        );
        ui::lit_readout(
            b,
            disp,
            "Tempo",
            ui::bind("echo/bpm"),
            104.0,
            36.0,
            92.0,
            9.5,
            ORANGE,
            TextAlign::Right,
        );
        ui::glass(b, disp);
        screws(b, bay1, &[(14.0, 14.0), (14.0, 300.0)]);
        // Bay 2: heads, selector, sync.
        let bay2 = panel(b, unit, "Bay 2", (385.0, 30.0, 194.0, 314.0), &grey);
        parts::centred(
            b,
            bay2,
            "Active Heads",
            (97.0, 30.0),
            Type::new(8.0, INK, 500.0).track(0.08).upper(),
        );
        parts::art(
            b,
            bay2,
            &[
                vec![(48.0, 44.0), (48.0, 30.0), (60.0, 30.0)],
                vec![(146.0, 44.0), (146.0, 30.0), (134.0, 30.0)],
            ],
            1.0,
            INK,
        );
        for (i, x) in [54.0, 97.0, 140.0].iter().enumerate() {
            parts::status_led(
                b,
                bay2,
                &format!("echo/head{}", i + 1),
                8.0,
                (*x, 54.0),
                "#fffbe8",
                "light",
                None,
            );
        }
        let dots: Vec<(f64, f64)> = (0..5)
            .map(|i| draw::polar(97.0, 150.0, 70.0, -110.0 + 55.0 * f64::from(i)))
            .collect();
        for (x, y) in &dots {
            b.add(bay2, draw::circle("detent", *x, *y, 3.4, draw::solid(INK)));
        }
        let arc: Vec<(f64, f64)> = (0..=30)
            .map(|i| draw::polar(97.0, 150.0, 62.0, -110.0 + 220.0 * f64::from(i) / 30.0))
            .collect();
        parts::art(b, bay2, &[arc], 1.2, INK);
        // The mode selector: a big white knurled dial stepping through the
        // five detents (220° of travel).
        let dial = KnobLook {
            preset: "white_knurl",
            opts: &[
                "color=#eeede9",
                "index=none",
                "diamond=0",
                "ribs=90",
                "knurl_band=0.5",
                "rib_depth=0.02",
                "cap_r=0.97",
                "height=0.5",
                "dome=0.03",
                "edge=0.1",
                "ticks=none",
                "fallback=knurled",
            ],
        };
        let s = look::place_knob(
            b,
            bay2,
            &dial,
            "echo/mode",
            "Mode",
            92.0,
            (97.0, 150.0),
            INK,
        );
        if let Some(c) = b.controls.get_mut(&s) {
            c.kind = ControlKind::Knob {
                sweep_degrees: 220.0,
            };
        }
        parts::centred(
            b,
            bay2,
            "Mode",
            (97.0, 143.0),
            Type::new(8.5, "#3a3b3e", 600.0).track(0.08).upper(),
        );
        parts::centred(
            b,
            bay2,
            "Selector",
            (97.0, 156.0),
            Type::new(8.5, "#3a3b3e", 600.0).track(0.08).upper(),
        );
        for (x, s) in [(52.0, "Free"), (97.0, "Daw"), (142.0, "Tap")] {
            parts::centred(
                b,
                bay2,
                s,
                (x, 236.0),
                Type::new(8.5, INK, 500.0).track(0.06).upper(),
            );
        }
        parts::art(
            b,
            bay2,
            &[
                vec![(52.0, 244.0), (52.0, 252.0), (60.0, 256.0)],
                vec![(97.0, 244.0), (97.0, 252.0)],
                vec![(142.0, 244.0), (142.0, 252.0), (134.0, 256.0)],
            ],
            1.0,
            INK,
        );
        let lever = Place::new("Sync")
            .size(20.0)
            .bind(ui::bind("echo/sync"))
            .sweep(90.0)
            .options(&["up", &format!("color={ORANGE}")])
            .touch(TOUCH_MIN);
        parts::part(
            b,
            bay2,
            "switch.brass_toggle3",
            lever,
            &["bushing"],
            (97.0, 284.0),
        );
        parts::centred(b, bay2, "Sync", (97.0, 304.0), t);
        // Bay 3: eight knobs and TAP.
        let bay3 = panel(b, unit, "Bay 3", (581.0, 30.0, 405.0, 314.0), &grey);
        let knobs: [(&str, &str, &str, f64, f64); 8] = [
            ("echo/time", "Time", "black", 65.0, 88.0),
            ("echo/feedback", "Feedback", "black", 161.0, 88.0),
            ("echo/tape_mod", "Tape-Mod", "black", 247.0, 88.0),
            ("echo/mix", "Mix", "black", 332.0, 88.0),
            ("echo/drive", "Tape Drive", "black", 65.0, 176.0),
            ("echo/bass", "Bass", "orange", 161.0, 176.0),
            ("echo/treble", "Treble", "orange", 247.0, 176.0),
            ("echo/width", "Width", "black", 332.0, 176.0),
        ];
        let black = KnobLook {
            preset: "knurled",
            opts: &[
                "color=#1c1c1e",
                "pointer=#ee8a1e",
                "index=dot",
                "cap_r=0.9",
                "height=0.6",
                "dome=0.08",
                "edge=0.1",
                "ribs=48",
                "rib_depth=0.03",
                "knurl_band=0.7",
                "index_w=0.09",
                "index_in=0.66",
                "index_out=0.66",
                "coat=0.5",
                "rough=0.25",
                "ticks=none",
            ],
        };
        let orange = KnobLook {
            preset: "knurled",
            opts: &[
                "color=#ee8a1e",
                "pointer=#ffd9a0",
                "index=dot",
                "cap_r=0.9",
                "height=0.6",
                "dome=0.05",
                "edge=0.08",
                "ribs=24",
                "rib_depth=0.06",
                "knurl_band=1",
                "index_w=0.08",
                "index_in=0.66",
                "index_out=0.66",
                "coat=0.4",
                "rough=0.3",
                "ticks=none",
            ],
        };
        for (addr, legend, v, x, y) in knobs {
            let look = if v == "orange" { &orange } else { &black };
            look::knob(b, bay3, look, addr, 42.0, (x, y), 11, INK, false);
            parts::centred(
                b,
                bay3,
                legend,
                (x, y + 42.0),
                Type::new(9.0, INK, 500.0).track(0.08).upper(),
            );
        }
        glow_key(b, bay3, "echo/tap", 34.0, (74.0, 262.0), "#fff2c0", true);
        parts::centred(b, bay3, "Tap", (74.0, 304.0), t);
        // Bay 4: mark, emblem, power.
        let bay4 = panel(b, unit, "Bay 4", (988.0, 30.0, 124.0, 314.0), &grey);
        look::brush_mark(b, bay4, (62.0, 62.0), 26.0, "#8a8c90");
        look::emblem(b, bay4, (62.0, 176.0), 22.0, INK);
        parts::status_led(
            b,
            bay4,
            "echo/on",
            7.0,
            (62.0, 218.0),
            "#fffbe8",
            "light",
            None,
        );
        glow_key(b, bay4, "echo/on", 34.0, (62.0, 262.0), ORANGE, false);
        parts::centred(b, bay4, "Power", (62.0, 304.0), t);
        screws(b, bay4, &[(110.0, 14.0), (110.0, 300.0)]);
    });
    let params = vec![
        // Every knob at noon, as the reference shows them.
        parts::pick("echo/time", "Delay Time", DIVS, 1),
        parts::pct("echo/feedback", "Feedback", 50.0),
        parts::pct("echo/tape_mod", "Tape-Mod", 50.0),
        parts::pct("echo/mix", "Mix", 50.0),
        parts::pct("echo/drive", "Tape Drive", 50.0),
        parts::db("echo/bass", "Bass", -12.0, 12.0, 0.0),
        parts::db("echo/treble", "Treble", -12.0, 12.0, 0.0),
        parts::pct("echo/width", "Width", 50.0),
        parts::pick("echo/mode", "Mode", &["1", "2", "3", "1+2", "1+2+3"], 0),
        parts::pick("echo/sync", "Sync", &["Free", "DAW", "Tap"], 1),
        ui::param(
            "echo/bpm",
            "Tempo",
            frame::surface::proto::Range::linear(40.0, 240.0, frame::surface::proto::Unit::Bpm),
            120.0,
        )
        .featured(),
        parts::switch("echo/head1", "Head 1", true),
        parts::switch("echo/head2", "Head 2", false),
        parts::switch("echo/head3", "Head 3", false),
        parts::switch("echo/tap", "Tap", false),
        parts::switch("echo/on", "Power", true),
    ];
    look::face_with(
        "10-echo-rack-1140x350",
        scene,
        params,
        vec![ui::level_stream("echo/in", "Input", 2)],
    )
}

/// A thin horizontal slider drawn as a display bar: its `Fill` grows with
/// the param.
fn bar(b: &mut Builder<'_>, parent: NodeId, addr: &str, (x, cy): (f64, f64), w: f64) {
    let h = 8.0;
    let root = b.add(
        parent,
        draw::frame(&format!("{addr} bar"), x, cy - h / 2.0, w, h),
    );
    b.add(
        root,
        draw::stroked(
            draw::rect("frame", 0.0, 0.0, w, h, draw::solid("#00000000")),
            "#e8eaee",
            1.0,
        ),
    );
    let fill = b.add(
        root,
        ui::glowing(
            draw::rect("fill", 1.5, 1.5, w - 3.0, h - 3.0, draw::solid("#e8eaee")),
            "#e8eaee",
            0.3,
            3.0,
        ),
    );
    let mut c = Control::new(ControlKind::Fader {
        orientation: Orientation::Horizontal,
    })
    .slot(SlotRole::Fill, fill.0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
}

pub(super) fn reverb() -> Face {
    let (w, h) = (RACK_W, 210.0);
    let dark = [(0.0, "#737476"), (1.0, "#5c5d60")];
    let scene = canvas("Reverb", |b, root| {
        parts::page(b, root, w, h);
        let unit = panel(
            b,
            root,
            "Unit",
            (0.0, 0.0, w, h),
            &[(0.0, "#58595c"), (1.0, "#46474a")],
        );
        let pale = "#d9dadc";
        let t = Type::new(9.5, pale, 500.0).track(0.08).upper();
        let bay1 = panel(b, unit, "Bay 1", (28.0, 6.0, 551.0, 198.0), &dark);
        tall_letters(
            b,
            bay1,
            "REVERB",
            42.0,
            38.0,
            26.0,
            Type::new(21.0, pale, 400.0)
                .family(draw::family::SAIRA)
                .width(88.0),
        );
        let disp = ui::display(
            b, bay1, "Display", 118.0, 26.0, 300.0, 80.0, "#9aa0a8", "glass",
        );
        let lit = "#e8eaee";
        ui::lit_text(
            b,
            disp,
            "MODE",
            12.0,
            12.0,
            60.0,
            8.0,
            "#b9bcc2",
            TextAlign::Left,
            500.0,
        );
        ui::lit_readout(
            b,
            disp,
            "Mode",
            ui::bind("verb/mode"),
            12.0,
            32.0,
            90.0,
            13.0,
            lit,
            TextAlign::Left,
        );
        ui::lit_text(
            b,
            disp,
            "MIX",
            12.0,
            62.0,
            30.0,
            8.0,
            "#b9bcc2",
            TextAlign::Left,
            500.0,
        );
        bar(b, disp, "verb/mix", (38.0, 62.0), 40.0);
        ui::lit_readout(
            b,
            disp,
            "Mix",
            ui::bind("verb/mix"),
            80.0,
            62.0,
            34.0,
            8.0,
            lit,
            TextAlign::Left,
        );
        let rows = [
            ("PRE-DELAY", "verb/predelay", 14.0),
            ("DECAY", "verb/decay", 38.0),
        ];
        for (label, addr, y) in rows {
            ui::lit_text(
                b,
                disp,
                label,
                124.0,
                y,
                80.0,
                8.0,
                "#b9bcc2",
                TextAlign::Left,
                500.0,
            );
            bar(b, disp, addr, (206.0, y), 40.0);
            ui::lit_readout(
                b,
                disp,
                label,
                ui::bind(addr),
                250.0,
                y,
                46.0,
                8.0,
                lit,
                TextAlign::Left,
            );
        }
        ui::lit_text(
            b,
            disp,
            "FILTER",
            124.0,
            62.0,
            40.0,
            8.0,
            "#b9bcc2",
            TextAlign::Left,
            500.0,
        );
        let low = ui::lit_readout(
            b,
            disp,
            "Low cut",
            ui::bind("verb/low"),
            158.0,
            62.0,
            46.0,
            8.0,
            lit,
            TextAlign::Left,
        );
        look::set_format(b, low, ValueFormat::default().mark(20.0, "OFF"));
        bar(b, disp, "verb/high", (206.0, 62.0), 40.0);
        let high = ui::lit_readout(
            b,
            disp,
            "High cut",
            ui::bind("verb/high"),
            250.0,
            62.0,
            46.0,
            8.0,
            lit,
            TextAlign::Left,
        );
        look::set_format(b, high, ValueFormat::default().mark(20_000.0, "OFF"));
        ui::glass(b, disp);
        look::brush_mark(b, bay1, (180.0, 162.0), 28.0, ORANGE);
        parts::centred(b, bay1, "Hall", (476.0, 34.0), t);
        parts::centred(b, bay1, "Plate", (476.0, 112.0), t);
        let tg = Place::new("Hall / Plate")
            .size(20.0)
            .variant("black")
            .bind(ui::bind("verb/mode"))
            .legend("")
            .options(&["legend_right"])
            .touch(TOUCH_MIN);
        parts::part(
            b,
            bay1,
            "switch.toggle_nut",
            tg,
            &["bushing"],
            (476.0, 73.0),
        );
        // The tone pointer glyph.
        b.add(
            bay1,
            draw::circle("dot", 520.0, 172.0, 7.0, draw::solid(pale)),
        );
        b.add(
            bay1,
            draw::circle("dot", 506.0, 172.0, 3.5, draw::solid(pale)),
        );
        // Faders.
        let bay2 = panel(b, unit, "Bay 2", (581.0, 6.0, 405.0, 198.0), &dark);
        let faders = [
            ("verb/predelay", "Pre-Delay", "#ece6dc", 74.0),
            ("verb/decay", "Decay", "#ece6dc", 140.0),
            ("verb/low", "", ORANGE, 207.0),
            ("verb/high", "", ORANGE, 274.0),
            ("verb/mix", "Mix", "#ece6dc", 335.0),
        ];
        for (addr, legend, color, x) in faders {
            let p = Place {
                options: Some(vec![format!("color={color}")]),
                ..Place::new(addr).size(118.0).bind(ui::bind(addr))
            };
            let f = frame::surface::kit::place(b, bay2, "lever.pill", &p)
                .unwrap_or_else(|e| panic!("{e}"));
            parts::centre_on(b, f, (x, 88.0));
            if legend.is_empty() {
                let lo = addr.ends_with("low");
                let pts = if lo {
                    vec![(x - 9.0, 180.0), (x - 5.0, 174.0), (x + 9.0, 174.0)]
                } else {
                    vec![(x - 9.0, 174.0), (x + 5.0, 174.0), (x + 9.0, 180.0)]
                };
                parts::art(b, bay2, &[pts], 1.4, pale);
            } else {
                parts::centred(b, bay2, legend, (x, 178.0), t);
            }
        }
        let bay3 = panel(b, unit, "Bay 3", (988.0, 6.0, 124.0, 198.0), &dark);
        look::signature(b, bay3, (62.0, 48.0), 17.0, "#aeb0b4");
        parts::status_led(
            b,
            bay3,
            "verb/on",
            7.0,
            (62.0, 98.0),
            "#fffbe8",
            "dark",
            None,
        );
        glow_key(b, bay3, "verb/on", 32.0, (62.0, 136.0), ORANGE, false);
        parts::centred(b, bay3, "Power", (62.0, 178.0), t);
    });
    let params = vec![
        parts::pick("verb/mode", "Mode", &["Plate", "Hall"], 0),
        parts::ms("verb/predelay", "Pre-Delay", 1.0, 500.0, 200.0),
        ui::param(
            "verb/decay",
            "Decay",
            frame::surface::proto::Range::log(0.2, 20.0, frame::surface::proto::Unit::Seconds),
            2.0,
        )
        .featured(),
        parts::hz("verb/low", "Low Cut", 20.0, 1000.0, 150.0),
        parts::hz("verb/high", "High Cut", 1000.0, 20000.0, 3700.0),
        parts::pct("verb/mix", "Mix", 75.0),
        parts::switch("verb/on", "Power", true),
    ];
    look::face("11-reverb-rack-1140x210", scene, params)
}
