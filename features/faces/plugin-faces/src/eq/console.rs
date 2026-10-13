//! The large-format console's channel EQ (after the SSL 4000 E "black"
//! EQ), laid out from photographs of the strip.
//!
//! The strip runs FILTERS, HF, HMF, LMF, LF top to bottom; here each
//! section stands as a column in the same order, left to right, and keeps
//! the strip's arrangement inside it — gain top left, frequency staggered
//! down on the right, Q under the gain, the grey key beside them — with
//! the strip's white hairlines between the
//! sections. The caps are the console's: a dark grey collet skirt with a
//! coloured top — red HF, green HMF, blue LMF, black LF, grey filters —
//! inside dotted scales with every position numbered, white legends on the
//! warm dark grey paint. Right of the EQ: the IN and phase keys, the input
//! trim, and the channel's output meter.
//!
//! Every frequency pot follows its printed scale — the marks are where the
//! console prints them, not an even log spread.

use frame::document::proto::NodeId;
use frame::surface::kit::{Place, TOUCH_MIN};
use frame::surface::proto::{Binding, Source, Unit, ValueFormat};
use frame::surface::widgets::Builder;

use super::paint;
use crate::comp_vca::ui::{self, bind};
use crate::face::{Face, canvas};

const W: f64 = 1080.0;
const H: f64 = 344.0;
const INK: &str = "#f1f0ea";
const HEAD: f64 = 26.0;
const R1: f64 = 96.0;
const R2: f64 = 186.0;
const R3: f64 = 280.0;
const CAP: f64 = 46.0;
const SECTION: f64 = 164.0;
const LEFT: f64 = 16.0;

const GAIN_SCALE: &[&str] = &["-15", "12", "9", "6", "3", "0", "3", "6", "9", "12", "+15"];
const HPF: &[f64] = &[16.0, 20.0, 70.0, 120.0, 200.0, 300.0, 350.0];
const LPF: &[f64] = &[3000.0, 3500.0, 4000.0, 8000.0, 12000.0, 22000.0];
const LF: &[f64] = &[30.0, 50.0, 100.0, 200.0, 300.0, 400.0, 450.0];
const LMF: &[f64] = &[200.0, 300.0, 800.0, 1000.0, 1500.0, 2000.0, 2500.0];
const HMF: &[f64] = &[600.0, 800.0, 1500.0, 3000.0, 4500.0, 6000.0, 7000.0];
const HF: &[f64] = &[1500.0, 2000.0, 5000.0, 8000.0, 10000.0, 14000.0, 16000.0];
const QS: &[f64] = &[0.5, 1.0, 1.5, 2.0, 3.0];

/// How the console prints a frequency: Hz under 1 k, else kHz with the
/// leading zero dropped (`.8`, `1.5`, `16`).
fn mark(v: f64, khz: bool) -> String {
    if !khz {
        return format!("{v}");
    }
    let s = format!("{}", v / 1000.0);
    s.strip_prefix('0').map_or(s.clone(), ToString::to_string)
}

fn marks(values: &[f64], khz: bool) -> Vec<String> {
    values.iter().map(|v| mark(*v, khz)).collect()
}

/// The left and right columns of section `i`.
fn cols(i: usize) -> (f64, f64) {
    let x = LEFT + SECTION * i as f64;
    (x + 44.0, x + 118.0)
}

/// A console cap with its dotted, numbered scale and its legend under it.
#[allow(clippy::too_many_arguments)]
fn cap(
    b: &mut Builder<'_>,
    panel: NodeId,
    addr: &str,
    legend: &str,
    color: &str,
    scale: &[String],
    cx: f64,
    cy: f64,
) -> NodeId {
    let scale: Vec<&str> = scale.iter().map(String::as_str).collect();
    let p = Place::new(format!("{addr} {legend}"))
        .size(CAP)
        .variant(color)
        .legend(legend)
        .keep_case()
        .touch(TOUCH_MIN)
        .bind(bind(addr))
        .scale(&scale);
    ui::put(b, panel, "knob.console_cap", &p, cx, cy)
}

fn gain(b: &mut Builder<'_>, panel: NodeId, addr: &str, color: &str, cx: f64, cy: f64) {
    let s: Vec<String> = GAIN_SCALE.iter().map(ToString::to_string).collect();
    cap(b, panel, addr, "dB", color, &s, cx, cy);
}

/// A grey key with its legend on the cap; `lamp` gives it a status LED.
#[allow(clippy::too_many_arguments)]
fn key(
    b: &mut Builder<'_>,
    panel: NodeId,
    addr: &str,
    legend: &str,
    lamp: Option<&str>,
    cx: f64,
    cy: f64,
) {
    let opts: Vec<String> =
        lamp.map_or_else(|| vec!["nolamp".into()], |c| vec![format!("color={c}")]);
    let p = Place {
        options: Some(opts),
        ..Place::new(format!("{addr} key"))
            .size(48.0)
            .variant("grey")
            .legend(legend)
            .keep_case()
            .bind(bind(addr))
    };
    ui::put_at(b, panel, "button.key", &p, cx - 24.0, cy - 12.0);
}

/// The strip's white section hairline, down the panel at `x`.
fn hairline(b: &mut Builder<'_>, panel: NodeId, x: f64) {
    ui::rule(b, panel, x, 10.0, 1.3, H - 20.0, "#e9e8e2");
}

fn title(b: &mut Builder<'_>, panel: NodeId, s: &str, i: usize) {
    ui::silk_left(
        b,
        panel,
        s,
        LEFT + SECTION * i as f64 + 10.0,
        HEAD,
        12.0,
        INK,
        800.0,
    );
}

pub fn face() -> Face {
    let scene = canvas("Console Channel EQ", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "charcoal", W, H, "none");
        paint(
            b,
            panel,
            &[(0.0, "#5e5c58"), (0.5, "#504f4b"), (1.0, "#43423f")],
        );
        let (hz, khz) = ("Hz", "kHz");

        // ── Filters ────────────────────────────────────────────────────
        title(b, panel, "Filters", 0);
        let (l, r) = cols(0);
        let lpf: Vec<String> = ["3", "3.5", "4", "8", "12", "Out"]
            .map(ToString::to_string)
            .to_vec();
        cap(b, panel, "consoleeq/lpf", khz, "grey", &lpf, r, R1);
        let hpf: Vec<String> = ["Out", "20", "70", "", "200", "300", "350"]
            .map(ToString::to_string)
            .to_vec();
        cap(b, panel, "consoleeq/hpf", hz, "grey", &hpf, l, R2);
        key(b, panel, "consoleeq/filters_in", "IN", None, r, R3 - 6.0);
        hairline(b, panel, LEFT + SECTION - 2.0);

        // ── HF ─────────────────────────────────────────────────────────
        title(b, panel, "HF", 1);
        let (l, r) = cols(1);
        gain(b, panel, "consoleeq/hf_gain", "red", l, R1);
        key(b, panel, "consoleeq/hf_bell", "BELL", None, r, R1 - 30.0);
        cap(
            b,
            panel,
            "consoleeq/hf_freq",
            khz,
            "red",
            &marks(HF, true),
            r,
            R2 - 10.0,
        );
        hairline(b, panel, LEFT + 2.0 * SECTION - 2.0);

        // ── HMF ────────────────────────────────────────────────────────
        title(b, panel, "HMF", 2);
        let (l, r) = cols(2);
        gain(b, panel, "consoleeq/hmf_gain", "green", l, R1);
        cap(
            b,
            panel,
            "consoleeq/hmf_freq",
            khz,
            "green",
            &marks(HMF, true),
            r,
            R2 - 12.0,
        );
        cap(
            b,
            panel,
            "consoleeq/hmf_q",
            "Q",
            "green",
            &marks(QS, false),
            l,
            R3 - 16.0,
        );
        key(b, panel, "consoleeq/hmf_x3", "×3", None, r, R1 - 30.0);
        hairline(b, panel, LEFT + 3.0 * SECTION - 2.0);

        // ── LMF ────────────────────────────────────────────────────────
        title(b, panel, "LMF", 3);
        let (l, r) = cols(3);
        gain(b, panel, "consoleeq/lmf_gain", "blue", l, R1);
        cap(
            b,
            panel,
            "consoleeq/lmf_freq",
            khz,
            "blue",
            &marks(LMF, true),
            r,
            R2 - 12.0,
        );
        cap(
            b,
            panel,
            "consoleeq/lmf_q",
            "Q",
            "blue",
            &marks(QS, false),
            l,
            R3 - 16.0,
        );
        key(b, panel, "consoleeq/lmf_div3", "÷3", None, r, R1 - 30.0);
        hairline(b, panel, LEFT + 4.0 * SECTION - 2.0);

        // ── LF ─────────────────────────────────────────────────────────
        title(b, panel, "LF", 4);
        let (l, r) = cols(4);
        cap(
            b,
            panel,
            "consoleeq/lf_freq",
            hz,
            "black",
            &marks(LF, false),
            r - 8.0,
            R1,
        );
        gain(b, panel, "consoleeq/lf_gain", "black", l, R2 - 30.0);
        key(b, panel, "consoleeq/lf_bell", "BELL", None, r, R2 - 20.0);
        hairline(b, panel, LEFT + 5.0 * SECTION - 2.0);

        // ── In, phase and trim ─────────────────────────────────────────
        let x = LEFT + 5.0 * SECTION + 56.0;
        ui::silk(b, panel, "Equaliser", x, HEAD, 9.0, INK, 800.0);
        key(b, panel, "consoleeq/eq_in", "IN", Some("#39d353"), x, 62.0);
        key(b, panel, "consoleeq/phase", "Ø", Some("#ff3b2f"), x, 128.0);
        let trim = cap(
            b,
            panel,
            "consoleeq/trim",
            "Trim",
            "grey",
            &["-24", "", "", "0", "", "", "+24"].map(ToString::to_string),
            x,
            R3 - 26.0,
        );
        ui::add_value(b, trim, 8.0, INK);
        if let Some(c) = b.controls.get_mut(&trim) {
            c.format = Some(ValueFormat::decimals(1));
        }
        hairline(b, panel, LEFT + 5.0 * SECTION + 112.0);

        // ── The output meter: two LED bars on a printed dB scale ───────
        let mx = LEFT + 5.0 * SECTION + 150.0;
        ui::silk(b, panel, "Out", mx + 18.0, HEAD, 9.0, INK, 800.0);
        let (top, bottom, floor, ceiling) = (52.0, 306.0, -40.0, 6.0);
        for (name, ch, dx) in [("Output L", 0, 0.0), ("Output R", 1, 22.0)] {
            let bind = Binding {
                source: Source::Stream {
                    id: frame::surface::proto::EntryId::new("consoleeq/out"),
                    channel: Some(ch),
                },
                ..Binding::stream("consoleeq/out")
            };
            let col = Place::new(name)
                .size(bottom - top)
                .options(&["floor=-40", "ceiling=6", "segments=23", "thick=14"])
                .bind(bind);
            ui::put_at(b, panel, "meter.led_column", &col, mx + dx, top);
        }
        for db in [6.0, 0.0, -10.0, -20.0, -30.0, -40.0] {
            let y = top + 2.0 + (bottom - top - 4.0) * (ceiling - db) / (ceiling - floor);
            let label = if db > 0.0 {
                format!("+{db}")
            } else {
                format!("{db}")
            };
            ui::silk_right(b, panel, &label, mx - 6.0, y, 7.5, INK, 700.0);
            ui::rule(b, panel, mx - 4.0, y - 0.5, 3.0, 1.0, "#f1f0ea99");
        }
        ui::silk(b, panel, "L", mx + 7.0, bottom + 14.0, 7.5, INK, 700.0);
        ui::silk(b, panel, "R", mx + 29.0, bottom + 14.0, 7.5, INK, 700.0);
    });

    let params = vec![
        ui::param("consoleeq/hpf", "HPF", ui::printed(HPF, Unit::Hz), 16.0),
        ui::param("consoleeq/lpf", "LPF", ui::printed(LPF, Unit::Hz), 22000.0),
        ui::toggle("consoleeq/filters_in", "Filters In", true),
        ui::db("consoleeq/lf_gain", "LF Gain", -15.0, 15.0, 0.0).featured(),
        ui::param(
            "consoleeq/lf_freq",
            "LF Freq",
            ui::printed(LF, Unit::Hz),
            100.0,
        ),
        ui::toggle("consoleeq/lf_bell", "LF Bell", false),
        ui::db("consoleeq/lmf_gain", "LMF Gain", -15.0, 15.0, 0.0).featured(),
        ui::param(
            "consoleeq/lmf_freq",
            "LMF Freq",
            ui::printed(LMF, Unit::Hz),
            800.0,
        ),
        ui::param("consoleeq/lmf_q", "LMF Q", ui::printed(QS, Unit::None), 1.0),
        ui::toggle("consoleeq/lmf_div3", "LMF ÷3", false),
        ui::db("consoleeq/hmf_gain", "HMF Gain", -15.0, 15.0, 0.0).featured(),
        ui::param(
            "consoleeq/hmf_freq",
            "HMF Freq",
            ui::printed(HMF, Unit::Hz),
            3000.0,
        ),
        ui::param("consoleeq/hmf_q", "HMF Q", ui::printed(QS, Unit::None), 1.0),
        ui::toggle("consoleeq/hmf_x3", "HMF ×3", false),
        ui::db("consoleeq/hf_gain", "HF Gain", -15.0, 15.0, 0.0).featured(),
        ui::param(
            "consoleeq/hf_freq",
            "HF Freq",
            ui::printed(HF, Unit::Hz),
            8000.0,
        ),
        ui::toggle("consoleeq/hf_bell", "HF Bell", false),
        ui::toggle("consoleeq/eq_in", "EQ In", true).featured(),
        ui::toggle("consoleeq/phase", "Phase", false),
        ui::db("consoleeq/trim", "Trim", -24.0, 24.0, 0.0),
    ];
    let streams = vec![ui::level_stream("consoleeq/out", "Output Level", 2)];
    Face {
        file: "02-console-eq",
        plugin: "eq",
        scene,
        params,
        streams,
    }
}
