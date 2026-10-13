//! The impulse response: a convolution processor — a machine, not a room.
//!
//! The display is the point of it: the loaded impulse drawn as it was
//! recorded (the direct spike, the pre-delay gap before it, the recorded
//! tail under its envelope) over a time axis, its name and format on top
//! and the live pre-delay, length, tone and mix along the bottom. The file
//! bank sits under the display as a row of machined keys; the shaping
//! controls, which only re-shape what was recorded, are on the right.

use document_proto::{NodeId, TextAlign};
use surface_kit::Place;
use surface_widgets::Builder;

use super::ui::{self, Dial, bind};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 320.0;
const INK: &str = "#dfe6ee";
const DIM: &str = "#8794a3";
const LCD: &str = "#62ccf0";
const LCD_DIM: &str = "#2f7a96";

const FILES: &[&str] = &[
    "Large Hall",
    "Chamber",
    "Steel Plate",
    "Spring Tank",
    "Stone Church",
    "Studio A",
];
const NAMES: &[&str] = &["Hall", "Chamber", "Plate", "Spring", "Church", "Studio"];

/// The recorded impulse, mirrored about its axis.
fn impulse(b: &mut Builder<'_>, face: NodeId, x0: f64, x1: f64, mid: f64, amp: f64) {
    let secs = 3.2;
    let px = |t: f64| x0 + (x1 - x0) * t / secs;
    // Axis and a quiet grid.
    ui::line(
        b,
        face,
        "axis",
        &[(x0, mid), (x1, mid)],
        0.8,
        &ui::alpha(LCD, 0.45),
    );
    for i in 0..=8 {
        let t = f64::from(i) * 0.4;
        let x = px(t);
        ui::rule(b, face, x, mid - amp, 0.6, amp * 2.0, &ui::alpha(LCD, 0.08));
        if i % 2 == 0 {
            let l = if i == 8 {
                "3.2 s".to_string()
            } else {
                format!("{t:.1}")
            };
            ui::lit_text(
                b,
                face,
                &l,
                x - 20.0,
                mid + amp + 9.0,
                40.0,
                6.5,
                LCD_DIM,
                TextAlign::Center,
                600.0,
            );
        }
    }
    // Pre-delay gap, marked.
    let pre = 0.012;
    ui::line(
        b,
        face,
        "onset",
        &[(px(pre), mid - amp - 2.0), (px(pre), mid + amp + 2.0)],
        0.8,
        &ui::alpha(LCD, 0.7),
    );
    // The impulse: one column per 8 ms.
    let mut bars = Vec::new();
    let mut upper = Vec::new();
    let mut i = 0.0;
    let mut t = pre;
    while t < secs {
        let e = if t - pre < 0.004 {
            1.0
        } else {
            0.78 * (-1.35 * (t - pre)).exp() * (1.0 - (-(t - pre) * 30.0).exp() * 0.3)
        };
        let a = e * (0.4 + 0.6 * super::jitter(i));
        let x = px(t);
        bars.push(vec![(x, mid - amp * a), (x, mid + amp * a)]);
        upper.push((x, mid - amp * e));
        t += 0.008;
        i += 1.0;
    }
    b.add(
        face,
        ui::glowing(
            ui::bars("impulse", &bars, 1.4, &ui::alpha(LCD, 0.75)),
            LCD,
            0.6,
            3.0,
        ),
    );
    let lower: Vec<(f64, f64)> = upper.iter().map(|(x, y)| (*x, 2.0 * mid - y)).collect();
    b.add(
        face,
        ui::glowing(ui::line_node("envelope", &upper, 1.0, LCD), LCD, 0.8, 2.0),
    );
    b.add(
        face,
        ui::glowing(ui::line_node("envelope", &lower, 1.0, LCD), LCD, 0.8, 2.0),
    );
}

pub fn face() -> Face {
    let scene = canvas("Convolution Reverb", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.brushed", "black", W, H, "ears");

        ui::plain(b, panel, "IR", 70.0, 34.0, 18.0, INK, 800.0);
        ui::silk_left(
            b,
            panel,
            "Convolution Reverberator",
            94.0,
            30.0,
            7.5,
            INK,
            800.0,
        );
        ui::silk_left(
            b,
            panel,
            "Sampled spaces · true stereo",
            94.0,
            42.0,
            6.0,
            DIM,
            700.0,
        );

        // ── The display ────────────────────────────────────────────────
        let (dx, dy, dw, dh) = (56.0, 56.0, 500.0, 148.0);
        let face = ui::display(b, panel, "Display", dx, dy, dw, dh, LCD, "glass");
        ui::lit_text(
            b,
            face,
            "IR",
            12.0,
            16.0,
            22.0,
            9.0,
            LCD_DIM,
            TextAlign::Left,
            800.0,
        );
        ui::lit_readout(
            b,
            face,
            "Impulse",
            bind("ir/file"),
            34.0,
            16.0,
            220.0,
            12.0,
            LCD,
            TextAlign::Left,
        );
        ui::lit_text(
            b,
            face,
            "48 kHz · 24 bit · 4 ch · 3.2 s",
            dw - 232.0,
            16.0,
            220.0,
            7.5,
            LCD_DIM,
            TextAlign::Right,
            600.0,
        );
        impulse(b, face, 16.0, dw - 16.0, 68.0, 34.0);
        let cols = [
            ("Pre-Delay", "ir/predelay"),
            ("Length", "ir/length"),
            ("Tone", "ir/tone"),
            ("Mix", "ir/mix"),
        ];
        let cw = (dw - 24.0) / cols.len() as f64;
        for (i, (label, addr)) in cols.iter().enumerate() {
            let x = 12.0 + cw * i as f64;
            ui::lit_text(
                b,
                face,
                label,
                x,
                121.0,
                cw,
                6.5,
                LCD_DIM,
                TextAlign::Left,
                700.0,
            );
            ui::lit_readout(
                b,
                face,
                label,
                bind(addr),
                x,
                135.0,
                cw - 6.0,
                11.0,
                LCD,
                TextAlign::Left,
            );
            if i > 0 {
                ui::rule(b, face, x - 7.0, 119.0, 1.0, 27.0, &ui::alpha(LCD, 0.2));
            }
        }
        ui::glass(b, face);
        let mx = dx + dw + 14.0;
        for (ch, x) in [(0u8, mx), (1u8, mx + 13.0)] {
            ui::led_column(
                b,
                panel,
                if ch == 0 { "Out L" } else { "Out R" },
                "ir/out",
                ch,
                (x, dy - 2.0, 9.0, dh + 4.0),
                18,
                -42.0,
                3.0,
            );
        }
        ui::silk(b, panel, "L", mx + 4.5, dy + dh + 13.0, 6.0, DIM, 700.0);
        ui::silk(b, panel, "R", mx + 17.5, dy + dh + 13.0, 6.0, DIM, 700.0);

        // ── The file bank: a key per impulse, its name on the cap ──────
        let bank = Place::new("Impulse")
            .size(44.0)
            .variant("black")
            .bind(bind("ir/file"))
            .options(NAMES)
            .keep_case();
        let k = ui::put(b, panel, "button.metal_bank", &bank, dx + dw / 2.0, 246.0);
        ui::strip_legend(b, k);
        ui::silk(b, panel, "Impulse", dx + dw / 2.0, 300.0, 7.5, INK, 800.0);

        // ── Shaping, on the right ──────────────────────────────────────
        ui::rule(b, panel, 614.0, 34.0, 1.0, 256.0, "#00000099");
        ui::rule(b, panel, 615.0, 34.0, 1.0, 256.0, "#ffffff14");
        let knob = Dial::new("knob.metal_fluted", "silver", 44.0, INK, DIM);
        let rows: [&[(&str, &str)]; 2] = [
            &[
                ("ir/predelay", "Pre-Delay"),
                ("ir/length", "Length"),
                ("ir/tone", "Tone"),
                ("ir/damping", "Damping"),
            ],
            &[
                ("ir/motion", "Motion"),
                ("ir/bass", "Bass"),
                ("ir/width", "Width"),
                ("ir/mix", "Mix"),
            ],
        ];
        for (r, row) in rows.iter().enumerate() {
            let cy = 84.0 + 126.0 * r as f64;
            for (i, (addr, legend)) in row.iter().enumerate() {
                let cx = 666.0 + 76.0 * i as f64;
                ui::dial(b, panel, addr, legend, cx, cy, &knob.legend_at(cy + 50.0));
            }
        }
    });

    let params = vec![
        ui::choice("ir/file", "Impulse", FILES, 0).featured(),
        super::predelay("ir/predelay", "Pre-Delay", 0.0),
        super::decay("ir/length", "Length", 0.1, 6.0, 3.2),
        super::pct("ir/tone", "Tone", 50.0),
        super::pct("ir/damping", "Damping", 0.0),
        super::pct("ir/motion", "Motion", 0.0),
        super::bass("ir/bass", "Bass"),
        super::width("ir/width"),
        super::mix("ir/mix", 30.0),
    ];
    Face {
        file: "01-ir",
        plugin: "reverb",
        scene,
        params,
        streams: super::levels("ir"),
    }
}
