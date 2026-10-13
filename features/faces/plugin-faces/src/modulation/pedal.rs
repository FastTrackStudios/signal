//! What every stompbox shares: the enclosure, the rate lamp, the stomp,
//! and the catalog entries a modulator binds.

use document_proto::motion::{Dynamics, Motion, Num, Target, Wave};
use document_proto::{NodeId, TextAlign};
use surface_kit::{draw, motion};
use surface_proto::SlotRole;
use surface_proto::{ParamInfo, Range, StreamInfo, Unit};
use surface_widgets::Builder;

use crate::reverb::ui::{self, Dial};

/// Enclosure size.
pub const W: f64 = 380.0;
pub const H: f64 = 600.0;
/// The lid's size (inside the cast edge).
pub const LW: f64 = W - 2.0 * ui::EDGE;
pub const LH: f64 = H - 2.0 * ui::EDGE;

/// The enclosure in `paint` (top, middle, bottom); returns the lid.
pub fn body(b: &mut Builder<'_>, root: NodeId, paint: [&str; 3]) -> NodeId {
    ui::enclosure(b, root, W, H, paint, "#6c6d6a")
}

/// The rate lamp with its legend under it. It follows the engine's LFO
/// feed through a spring (a bulb warming and cooling); until the feed
/// sends anything it pulses on its own at the RATE param — an LFO motion.
#[allow(clippy::too_many_arguments)]
pub fn rate_lamp(
    b: &mut Builder<'_>,
    lid: NodeId,
    face: &str,
    cx: f64,
    cy: f64,
    color: &str,
    ink: &str,
) {
    let lamp = ui::stream_lamp(
        b,
        lid,
        "Rate lamp",
        &format!("{face}/lfo"),
        "Rate",
        cx,
        cy,
        26.0,
        color,
        ink,
    );
    let glow = b
        .controls
        .get(&lamp)
        .and_then(|c| c.slots_of(SlotRole::Glow).next())
        .map(NodeId);
    if let Some(glow) = glow {
        motion::add(
            b,
            glow,
            Motion::new(Target::None)
                .named("lfo")
                .dynamics(Dynamics::Lfo {
                    rate: Num::Expr(format!("param({face}/rate)")),
                    shape: Wave::Sine,
                    phase: 0.0,
                }),
        );
        motion::add(
            b,
            glow,
            Motion::new(Target::Opacity)
                .input(format!("or(scalar({face}/lfo), motion(lfo))"))
                .dynamics(Dynamics::spring(900.0, 50.0)),
        );
    }
}

/// The bypass stomp, centred low on the lid, its LED above it.
pub fn stomp(b: &mut Builder<'_>, lid: NodeId, face: &str, led: &str) {
    ui::stomp(
        b,
        lid,
        "Bypass",
        &format!("{face}/on"),
        LW / 2.0,
        LH - 84.0,
        52.0,
        led,
    );
}

/// The model name, big, centred at `cy`, with the maker's line under it.
pub fn title(
    b: &mut Builder<'_>,
    lid: NodeId,
    name: &str,
    line: &str,
    cy: f64,
    ink: &str,
    dim: &str,
) {
    let mut t = ui::text(
        "model",
        name,
        0.0,
        cy,
        LW,
        30.0,
        ink,
        TextAlign::Center,
        800.0,
        false,
    );
    if let document_proto::NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = -0.5;
    }
    b.add(lid, t);
    ui::silk(b, lid, line, LW / 2.0, cy + 24.0, 7.0, dim, 700.0);
}

/// A row of knobs across the lid at `cy`.
pub fn knob_row(
    b: &mut Builder<'_>,
    lid: NodeId,
    knobs: &[(&str, &str)],
    cy: f64,
    dial: &Dial<'_>,
) {
    let n = knobs.len() as f64;
    let margin = if n > 3.0 {
        40.0
    } else if n > 2.0 {
        62.0
    } else {
        96.0
    };
    let pitch = if n > 1.0 {
        (LW - 2.0 * margin) / (n - 1.0)
    } else {
        0.0
    };
    for (i, (addr, legend)) in knobs.iter().enumerate() {
        let cx = if n > 1.0 {
            margin + pitch * i as f64
        } else {
            LW / 2.0
        };
        ui::dial(b, lid, addr, legend, cx, cy, dial);
    }
}

/// A printed sine-family wave across `x0..x1` centred on `cy`: `cycles`
/// cycles, `amp` high, `phase` in turns — the lid's artwork.
#[allow(clippy::too_many_arguments)]
pub fn wave(
    b: &mut Builder<'_>,
    lid: NodeId,
    x0: f64,
    x1: f64,
    cy: f64,
    amp: f64,
    cycles: f64,
    phase: f64,
    weight: f64,
    color: &str,
    shape: &dyn Fn(f64) -> f64,
) {
    let pts: Vec<(f64, f64)> = (0..=160)
        .map(|i| {
            let u = f64::from(i) / 160.0;
            (
                x0 + (x1 - x0) * u,
                cy - amp * shape((u * cycles + phase) * std::f64::consts::TAU),
            )
        })
        .collect();
    ui::line(b, lid, "art", &pts, weight, color);
}

/// A silkscreened rule with a dot at each end.
pub fn rule(b: &mut Builder<'_>, lid: NodeId, y: f64, color: &str) {
    ui::rule(b, lid, 34.0, y, LW - 68.0, 1.0, color);
    b.add(
        lid,
        draw::circle("dot", 34.0, y + 0.5, 2.0, draw::solid(color)),
    );
    b.add(
        lid,
        draw::circle("dot", LW - 34.0, y + 0.5, 2.0, draw::solid(color)),
    );
}

// ── catalog ──────────────────────────────────────────────────────────────

/// LFO rate, Hz, logarithmic 0.05–20.
pub fn rate(face: &str, default_hz: f64) -> ParamInfo {
    ui::param(
        &format!("{face}/rate"),
        "Rate",
        Range::log(0.05, 20.0, Unit::Hz),
        default_hz,
    )
    .featured()
}

/// Modulation depth.
pub fn depth(face: &str, default: f64) -> ParamInfo {
    ui::percent(&format!("{face}/depth"), "Depth", default).featured()
}

/// Output trim, -24…+12 dB.
pub fn output(face: &str) -> ParamInfo {
    ui::param(
        &format!("{face}/output"),
        "Output",
        Range::linear(-24.0, 12.0, Unit::Db),
        0.0,
    )
    .bipolar()
}

/// A 0–100 % voicing control.
pub fn amount(face: &str, id: &str, name: &str, default: f64) -> ParamInfo {
    ui::percent(&format!("{face}/{id}"), name, default)
}

/// The stomp.
pub fn engaged(face: &str) -> ParamInfo {
    ui::toggle(&format!("{face}/on"), "Bypass", true).featured()
}

/// The LFO, for the rate lamp.
pub fn lfo(face: &str) -> Vec<StreamInfo> {
    vec![ui::lfo_stream(&format!("{face}/lfo"), "LFO")]
}
