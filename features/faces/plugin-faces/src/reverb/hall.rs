//! The hall: a digital reverberator rack of the late '70s / early '80s.
//!
//! Left, the phosphor display: the program on top, the decay drawn as the
//! machine computes it — a pre-delay gap, a handful of early reflections,
//! then the diffuse tail under its envelope — and the RT60, pre-delay, size
//! and mix read out along the bottom, live. The output ladder sits beside
//! it. Under the display, the program keys (Concert, Cathedral, Arena) and
//! the IN key.
//!
//! Right, the parameters on long-throw faders, the way such a unit's remote
//! put them: DECAY and SIZE with white caps, MIX red, the rest black.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, draw};
use surface_widgets::Builder;

use super::ui::{self, bind};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 320.0;
const INK: &str = "#e2ded4";
const DIM: &str = "#8f8b83";
/// Phosphor: blue-green vacuum fluorescent.
const VFD: &str = "#5ff0cf";
const VFD_DIM: &str = "#2f8f7b";

const PROGRAMS: &[&str] = &["Concert Hall", "Cathedral", "Arena"];

/// Faders: address, legend, cap.
const FADERS: &[(&str, &str, &str)] = &[
    ("hall/decay", "Decay", "white"),
    ("hall/predelay", "Pre-Dly", "black"),
    ("hall/size", "Size", "white"),
    ("hall/diffusion", "Diffuse", "black"),
    ("hall/bass", "Bass", "black"),
    ("hall/damping", "HF Damp", "black"),
    ("hall/reflections", "Early", "black"),
    ("hall/mix", "Mix", "red"),
];

/// The decay picture: pre-delay gap, early reflections, the tail.
fn decay_graphic(b: &mut Builder<'_>, face: NodeId, x0: f64, x1: f64, top: f64, base: f64) {
    let h = base - top;
    // Faint grid and a time axis, 0…2 s.
    let grid: Vec<Vec<(f64, f64)>> = (1..4)
        .map(|i| {
            let y = base - h * f64::from(i) / 4.0;
            vec![(x0, y), (x1, y)]
        })
        .collect();
    let mut grid = ui::strokes("grid", &grid, 0.8, draw::solid(&ui::alpha(VFD, 0.16)));
    grid.stroke.cap = document_proto::StrokeCap::None;
    grid.stroke.dash_pattern = vec![2.0, 4.0];
    b.add(face, grid);
    ui::line(
        b,
        face,
        "axis",
        &[(x0, base), (x1, base)],
        1.0,
        &ui::alpha(VFD, 0.55),
    );
    let secs = 2.0;
    for i in 0..=4 {
        let t = f64::from(i) * 0.5;
        let x = x0 + (x1 - x0) * t / secs;
        ui::line(
            b,
            face,
            "tick",
            &[(x, base), (x, base + 4.0)],
            1.0,
            &ui::alpha(VFD, 0.55),
        );
        let l = if i == 4 {
            "2.0 s".to_string()
        } else {
            format!("{t:.1}")
        };
        ui::lit_text(
            b,
            face,
            &l,
            x - 20.0,
            base + 11.0,
            40.0,
            7.0,
            VFD_DIM,
            TextAlign::Center,
            600.0,
        );
    }
    let px = |t: f64| x0 + (x1 - x0) * t / secs;
    // Pre-delay: nothing, then the direct-path marker.
    let pre = 0.024;
    // Early reflections: sparse, loud, irregular.
    let mut er = Vec::new();
    let taps = [
        (0.031, 0.92),
        (0.043, 0.7),
        (0.052, 0.83),
        (0.067, 0.58),
        (0.079, 0.66),
        (0.094, 0.47),
        (0.108, 0.52),
        (0.121, 0.38),
    ];
    for (t, a) in taps {
        let x = px(t + pre);
        er.push(vec![(x, base), (x, base - h * a)]);
    }
    b.add(
        face,
        ui::glowing(ui::bars("early reflections", &er, 1.8, VFD), VFD, 0.9, 4.0),
    );
    // The tail: dense, under an exponential envelope (RT60 ≈ 2.4 s).
    let mut tail = Vec::new();
    let mut env = Vec::new();
    let rt = 2.4;
    let mut t = 0.13;
    let mut i = 0.0;
    while t < secs {
        let e = 0.62 * (-6.9 * (t - 0.13) / rt).exp() * (1.0 - (-(t - 0.1) * 14.0).exp());
        let a = e * (0.35 + 0.65 * super::jitter(i));
        let x = px(t + pre);
        if x > x1 {
            break;
        }
        tail.push(vec![(x, base), (x, base - h * a)]);
        env.push((x, base - h * e - 1.5));
        t += 0.011;
        i += 1.0;
    }
    b.add(
        face,
        ui::glowing(
            ui::bars("tail", &tail, 1.6, &ui::alpha(VFD, 0.62)),
            VFD,
            0.5,
            3.0,
        ),
    );
    b.add(
        face,
        ui::glowing(ui::line_node("envelope", &env, 1.2, VFD), VFD, 0.8, 3.0),
    );
}

pub fn face() -> Face {
    let scene = canvas("Hall Reverberator", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.brushed", "black", W, H, "ears");

        // ── Badge ──────────────────────────────────────────────────────
        ui::plain(b, panel, "Hall", 84.0, 36.0, 17.0, INK, 800.0);
        ui::silk_left(
            b,
            panel,
            "Digital Reverberator",
            116.0,
            33.0,
            7.5,
            INK,
            700.0,
        );
        ui::silk_left(
            b,
            panel,
            "Stereo · 2 in / 2 out · 24-bit",
            116.0,
            45.0,
            6.0,
            DIM,
            600.0,
        );

        // ── The display ────────────────────────────────────────────────
        let (dx, dy, dw, dh) = (56.0, 62.0, 420.0, 150.0);
        let face = ui::display(b, panel, "Display", dx, dy, dw, dh, VFD, "crt");
        ui::lit_text(
            b,
            face,
            "P1",
            12.0,
            17.0,
            26.0,
            10.0,
            VFD_DIM,
            TextAlign::Left,
            700.0,
        );
        ui::lit_readout(
            b,
            face,
            "Program",
            bind("hall/program"),
            36.0,
            17.0,
            200.0,
            13.0,
            VFD,
            TextAlign::Left,
        );
        ui::lit_text(
            b,
            face,
            "Bank A · Halls",
            dw - 142.0,
            17.0,
            130.0,
            8.0,
            VFD_DIM,
            TextAlign::Right,
            600.0,
        );
        decay_graphic(b, face, 16.0, dw - 16.0, 36.0, 96.0);
        // The readouts: label over live value, four columns.
        let cols = [
            ("RT60", "hall/decay"),
            ("Pre-Delay", "hall/predelay"),
            ("Size", "hall/size"),
            ("Mix", "hall/mix"),
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
                7.0,
                VFD_DIM,
                TextAlign::Left,
                700.0,
            );
            ui::lit_readout(
                b,
                face,
                label,
                bind(addr),
                x,
                136.0,
                cw - 6.0,
                12.0,
                VFD,
                TextAlign::Left,
            );
            if i > 0 {
                ui::rule(b, face, x - 7.0, 116.0, 1.0, 28.0, &ui::alpha(VFD, 0.2));
            }
        }
        ui::glass(b, face);

        // Output ladder beside the display.
        let mx = dx + dw + 16.0;
        for (ch, x) in [(0u8, mx), (1u8, mx + 14.0)] {
            ui::led_column(
                b,
                panel,
                if ch == 0 { "Out L" } else { "Out R" },
                "hall/out",
                ch,
                (x, dy - 2.0, 10.0, dh + 4.0),
                18,
                -42.0,
                3.0,
            );
        }
        ui::silk(b, panel, "L", mx + 5.0, dy + dh + 14.0, 6.5, DIM, 700.0);
        ui::silk(b, panel, "R", mx + 19.0, dy + dh + 14.0, 6.5, DIM, 700.0);

        // ── Program keys and IN ────────────────────────────────────────
        // Numbered keys, as such a unit's were, the program printed under.
        let prog = Place::new("Program")
            .size(40.0)
            .variant("amber")
            .bind(bind("hall/program"))
            .options(&["1", "2", "3"]);
        let pk = ui::put(b, panel, "button.illuminated_bank", &prog, 300.0, 258.0);
        ui::strip_legend(b, pk);
        ui::bank_labels(
            b,
            panel,
            pk,
            &["Concert", "Cathedral", "Arena"],
            290.0,
            6.5,
            DIM,
        );
        ui::silk(b, panel, "Program", 300.0, 306.0, 7.5, INK, 800.0);
        let inb = Place::new("In")
            .size(40.0)
            .variant("green")
            .legend("In")
            .bind(bind("hall/on"));
        ui::put(b, panel, "button.illuminated", &inb, 96.0, 264.0);
        ui::silk(b, panel, "Engage", 96.0, 296.0, 7.5, INK, 800.0);

        // ── The fader bank ─────────────────────────────────────────────
        let (fx0, pitch) = (556.0, 47.0);
        ui::rule(b, panel, fx0 - 34.0, 36.0, 1.0, 256.0, "#00000099");
        ui::rule(b, panel, fx0 - 33.0, 36.0, 1.0, 256.0, "#ffffff14");
        ui::silk(
            b,
            panel,
            "Parameters",
            fx0 + pitch * 3.5,
            44.0,
            7.5,
            DIM,
            800.0,
        );
        for (i, (addr, legend, cap)) in FADERS.iter().enumerate() {
            let cx = fx0 + pitch * i as f64;
            let p = Place::new(*legend)
                .size(134.0)
                .variant(*cap)
                .legend(*legend)
                .bind(bind(addr));
            let f = ui::put(b, panel, "lever.fader", &p, cx, 0.0);
            let (fw, _) = b
                .scene
                .node(f)
                .map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
            ui::move_to(b, f, cx - fw / 2.0, 62.0);
            ui::add_value(b, f, 7.5, DIM);
        }
    });

    let params = vec![
        ui::choice("hall/program", "Program", PROGRAMS, 0).featured(),
        super::decay("hall/decay", "Decay", 0.3, 12.0, 2.4),
        super::predelay("hall/predelay", "Pre-Delay", 24.0),
        super::pct("hall/size", "Size", 60.0).featured(),
        super::pct("hall/diffusion", "Diffusion", 75.0),
        super::bass("hall/bass", "Bass Multiply"),
        super::pct("hall/damping", "HF Damping", 30.0),
        super::pct("hall/reflections", "Early Reflections", 50.0),
        super::mix("hall/mix", 30.0),
        ui::toggle("hall/on", "Engage", true),
    ];
    Face {
        file: "02-hall",
        plugin: "reverb",
        scene,
        params,
        streams: super::levels("hall"),
    }
}
