//! The rhythmic delay: a multi-tap laid out like a step sequencer.
//!
//! Sixteen step keys are the taps — press one and a repeat lands on that
//! sixteenth of the bar; its LED lights. The keys come in four colours of
//! four, a bar read at a glance, the default pattern the dotted-eighth
//! every guitar part wants (1, 4, 7, 10, 13, 16). An LED window gives the
//! step time in ms and as a note value, TAP beside it. Below, the row:
//! step time and swing, feedback, stereo spread, tone, duck, mix.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, draw};
use surface_proto::{ButtonMode, Control, ControlKind, SlotRole};
use surface_widgets::Builder;

use super::parts::{self, Ink, Screen, bind};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 320.0;
const INK: &str = "#eef1f5";
const DIM: &str = "#9aa3ae";
const LED: &str = "#ff8a3c";
const KEYS: [&str; 4] = ["#d9482b", "#ee7d36", "#efb437", "#e9e2d2"];
/// Steps on by default: the dotted eighth.
const PATTERN: [usize; 6] = [0, 3, 6, 9, 12, 15];
const T_MIN: f64 = 10.0;
const T_MAX: f64 = 1000.0;

fn a(s: &str) -> String {
    format!("rhythmic/{s}")
}

/// A step key: a coloured cap with its LED above, one toggle.
fn step_key(b: &mut Builder<'_>, parent: NodeId, i: usize, (cx, top): (f64, f64)) -> NodeId {
    let (w, h) = (30.0, 58.0);
    let root = b.add(
        parent,
        draw::frame(&format!("Step {}", i + 1), cx - w / 2.0, top, w, h),
    );
    let c = w / 2.0;
    parts::lamp(b, root, (c, 8.0), 7.0, LED, false);
    let lit = b.add(root, parts::lens(c, 8.0, 3.5, LED));
    let cap_c = KEYS[i / 4];
    let well = draw::rounded(
        draw::rect("key well", 1.0, 20.0, w - 2.0, 36.0, draw::solid("#0c0d0f")),
        4.0,
    );
    b.add(
        root,
        draw::with_effects(
            well,
            vec![
                draw::inner_shadow("#000000", 0.0, 1.0, 2.0),
                draw::drop_shadow("#ffffff14", 0.0, 1.0, 0.0),
            ],
        ),
    );
    let cap = draw::rounded(
        draw::rect(
            "key cap",
            3.0,
            21.0,
            w - 6.0,
            32.0,
            draw::linear(
                180.0,
                &[
                    (0.0, &parts::mixed(cap_c, "#ffffff", 0.3)),
                    (0.12, cap_c),
                    (0.85, &parts::mixed(cap_c, "#000000", 0.18)),
                    (1.0, &parts::mixed(cap_c, "#000000", 0.45)),
                ],
            ),
        ),
        3.0,
    );
    b.add(
        root,
        draw::with_effects(
            cap,
            vec![
                draw::drop_shadow("#000000b0", 0.0, 2.0, 2.5),
                draw::inner_shadow("#ffffff40", 0.0, 1.0, 0.0),
            ],
        ),
    );
    let ink = if i >= 12 { "#3b3a36" } else { "#1a0f0a" };
    let num = b.add(
        root,
        parts::text_node(
            &format!("{}", i + 1),
            0.0,
            45.0,
            w,
            &Ink::new(7.0, ink).weight(800.0).track(0.0),
            TextAlign::Center,
        ),
    );
    let mut ctl = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .slot(SlotRole::Led, lit.0)
    .slot(SlotRole::Label, num.0);
    ctl.binding = Some(bind(&a(&format!("step_{:02}", i + 1))));
    b.controls.insert(root, ctl);
    root
}

pub fn face() -> Face {
    let scene = canvas("Rhythmic Delay", |b, root| {
        parts::fit_root(b, root, W, H);
        let panel = parts::panel(b, root, "panel.painted", "charcoal", W, H, "ears");
        parts::repaint(
            b,
            panel,
            &[(0.0, "#44474e"), (0.5, "#33363c"), (1.0, "#25282d")],
        );

        // ── Model line ─────────────────────────────────────────────────
        parts::silk_left(
            b,
            panel,
            "Multi-Tap",
            50.0,
            34.0,
            Ink::new(15.0, INK).weight(800.0).track(0.08),
        );
        parts::silk_left(
            b,
            panel,
            "Pattern echo · 16 steps",
            50.0,
            52.0,
            Ink::new(6.8, DIM).weight(600.0).track(0.16),
        );

        // ── The step row ───────────────────────────────────────────────
        let (x0, pitch) = (238.0, 32.0);
        parts::well(
            b,
            panel,
            "Step bed",
            (x0 - 20.0, 22.0, pitch * 16.0 + 40.0, 118.0),
            draw::linear(180.0, &[(0.0, "#1c1e22"), (1.0, "#15171a")]),
            6.0,
        );
        for i in 0..16 {
            let cx = x0 + pitch * (i as f64 + 0.5);
            if i % 4 == 0 {
                parts::silk(
                    b,
                    panel,
                    &format!("{}", i / 4 + 1),
                    cx,
                    38.0,
                    Ink::new(8.0, INK).weight(800.0).track(0.0),
                );
                parts::rule(
                    b,
                    panel,
                    cx - 12.0,
                    cx + pitch * 4.0 - 20.0,
                    48.0,
                    1.0,
                    "#eef1f540",
                );
            } else {
                let sub = ["", "e", "+", "a"][i % 4];
                parts::silk(
                    b,
                    panel,
                    sub,
                    cx,
                    38.0,
                    Ink::new(7.0, DIM).weight(600.0).track(0.0).plain(),
                );
            }
            step_key(b, panel, i, (cx, 56.0));
        }
        parts::silk(
            b,
            panel,
            "Taps · one bar of sixteenths",
            x0 + pitch * 8.0,
            128.0,
            Ink::new(6.5, DIM).weight(700.0).track(0.22),
        );

        // ── Time window and tap ────────────────────────────────────────
        let win = parts::screen(
            b,
            panel,
            "Step time",
            (790.0, 30.0, 118.0, 52.0),
            Screen {
                glass: ("#170903", "#241006"),
                ink: LED,
                mesh: false,
            },
        );
        parts::readout(
            b,
            win,
            "Step time",
            bind(&a("time")),
            (4.0, 17.0, 110.0),
            17.0,
            LED,
            TextAlign::Center,
        );
        parts::note_picker(
            b,
            win,
            "Note",
            &a("div"),
            (26.0, 39.0, 66.0),
            10.0,
            LED,
            "#7a3a14",
        );
        let sync = Place::new("Time mode")
            .size(30.0)
            .bind(bind(&a("sync")))
            .options(parts::TIME_MODES);
        parts::put(b, panel, "button.square_bank", &sync, 822.0, 112.0);
        let tap = Place::new("Tap")
            .size(34.0)
            .variant("amber")
            .legend("Tap")
            .options(&["momentary"])
            .bind(bind(&a("tap")));
        parts::put(b, panel, "button.illuminated", &tap, 887.0, 112.0);

        // A small key: mute the dry signal (the taps alone).
        let kill = Place::new("Kill dry")
            .size(34.0)
            .variant("red")
            .legend("Kill")
            .bind(bind(&a("kill_dry")));
        parts::put(b, panel, "button.square", &kill, 72.0, 100.0);
        parts::silk(
            b,
            panel,
            "Dry",
            72.0,
            126.0,
            Ink::new(6.5, DIM).weight(700.0).track(0.2),
        );
        // Wet level, beside the steps it comes from.
        for (i, x) in [150.0, 166.0].into_iter().enumerate() {
            parts::led_column(
                b,
                panel,
                if i == 0 { "Wet L" } else { "Wet R" },
                &a("wet"),
                i as u8,
                (x, 70.0, 12.0, 64.0),
                9,
                (-36.0, 3.0),
            );
        }
        parts::silk(
            b,
            panel,
            "Wet",
            164.0,
            146.0,
            Ink::new(6.5, DIM).weight(700.0).track(0.2),
        );

        // ── Knob row ───────────────────────────────────────────────────
        parts::groove(b, panel, 40.0, W - 40.0, 160.0);
        let row = 222.0;
        let legend = Ink::new(8.0, INK).weight(700.0).track(0.14);
        let value = Ink::new(7.5, DIM).weight(600.0).track(0.02).plain();
        let scale = Ink::new(6.5, DIM).weight(600.0).track(0.0).plain();
        let tm = parts::time_marks(T_MIN, T_MAX);
        let knobs: [(&str, &str, f64, f64, &[(f64, &str)]); 7] = [
            ("time", "Step Time", 100.0, 56.0, &tm),
            ("swing", "Swing", 214.0, 56.0, &[(0.0, "0"), (1.0, "75")]),
            ("feedback", "Feedback", 328.0, 56.0, parts::FB_MARKS),
            ("spread", "Spread", 460.0, 44.0, parts::PCT_MARKS),
            ("tone", "Tone", 564.0, 44.0, parts::PCT_MARKS),
            ("duck", "Duck", 668.0, 44.0, parts::PCT_MARKS),
            ("mix", "Mix", 836.0, 54.0, parts::PCT_MARKS),
        ];
        for (addr, name, x, d, marks) in knobs {
            let k = parts::knob(b, panel, "knob.metal", "black", &a(addr), name, (x, row), d);
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, panel, (x, row), d / 2.0 + 5.0, sweep, marks, 11, scale);
            parts::legend_at(b, k, name, 276.0, legend);
            if addr != "time" {
                parts::value_at(b, k, 291.0, value);
            }
        }
        parts::vgroove(b, panel, 394.0, 176.0, 298.0);
        parts::vgroove(b, panel, 740.0, 176.0, 298.0);
    });

    let mut params = vec![
        parts::time_ms(&a("time"), "Step Time", T_MIN, T_MAX, 125.0).featured(),
        parts::choice(&a("sync"), "Time Mode", parts::TIME_MODES, 1),
        parts::choice(&a("div"), "Step Note", parts::DIVISIONS, 12),
        parts::toggle(&a("tap"), "Tap Tempo", false),
        parts::param(
            &a("swing"),
            "Swing",
            surface_proto::Range {
                steps: Some(76),
                ..surface_proto::Range::linear(0.0, 75.0, surface_proto::Unit::Percent)
            },
            0.0,
        ),
        parts::feedback(&a("feedback"), "Feedback", 30.0).featured(),
        parts::percent(&a("spread"), "Spread", 50.0),
        parts::percent(&a("tone"), "Tone", 50.0),
        parts::percent(&a("duck"), "Duck", 0.0),
        parts::percent(&a("mix"), "Mix", 35.0).featured(),
        parts::toggle(&a("kill_dry"), "Kill Dry", false),
    ];
    for i in 0..16 {
        let p = parts::toggle(
            &a(&format!("step_{:02}", i + 1)),
            &format!("Step {}", i + 1),
            PATTERN.contains(&i),
        );
        params.push(if i == 0 { p.featured() } else { p });
    }
    let streams = vec![parts::level_stream(&a("wet"), "Wet Level", 2)];
    Face {
        file: "05-rhythmic",
        plugin: "delay",
        scene,
        params,
        streams,
    }
}
