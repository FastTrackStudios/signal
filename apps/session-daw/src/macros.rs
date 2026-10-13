//! A phase's macros: a few knobs that stand for the whole phase.
//!
//! Folded, a phase in the FX view is not gone — it is these. Each knob
//! drives one of the phase's settings over the span an engineer reaches
//! for (the gate's threshold, the compressor's, a reverb's send-in mix),
//! so with every phase folded there is still a hand on the track.
//!
//! The face is a frame (`fx.macro`, one knob; a theme can name a page
//! `fx.macro.<phase>` or `fx.macro.<phase>.<track>` to give a phase or a
//! single track its own), the painters' arc when no theme is loaded.

use anyrender::{PaintScene, Scene};
use session::mix_phases::MixPhase;
use vello::kurbo::{Affine, Arc, Stroke};
use vello::peniko::Color;

use crate::arrangement::Palette;
use crate::text::Font;
use crate::tone::{Panel, Tone};

/// One knob: what it is called, where it stands (0..1), how to move it,
/// and what it reads as.
pub struct Macro {
    pub name: &'static str,
    pub get: fn(&Tone) -> f64,
    pub set: fn(&mut Tone, f64),
    pub says: fn(&Tone) -> String,
}

/// `v` across `lo..hi`, as 0..1.
pub(crate) fn frac(v: f32, lo: f32, hi: f32) -> f64 {
    f64::from(((v - lo) / (hi - lo)).clamp(0.0, 1.0))
}

/// 0..1 back across `lo..hi`.
pub(crate) fn span(f: f64, lo: f32, hi: f32) -> f32 {
    let f = crate::num::narrow(f.clamp(0.0, 1.0));
    (hi - lo).mul_add(f, lo)
}

/// The first and the last band the EQ uses: its low and high shelves,
/// as a chain is seeded.
fn end_band(
    bands: &mut [eq_ui::eq_graph_model::EqBand],
    last: bool,
) -> Option<&mut eq_ui::eq_graph_model::EqBand> {
    let mut used = bands.iter_mut().filter(|b| b.used);
    if last { used.next_back() } else { used.next() }
}

fn band_gain(bands: &[eq_ui::eq_graph_model::EqBand], last: bool) -> f32 {
    let mut used = bands.iter().filter(|b| b.used);
    let band = if last { used.next_back() } else { used.next() };
    band.map_or(0.0, |b| b.gain)
}

fn db(v: f32) -> String {
    format!("{v:.1} dB")
}

const RESCUE: [Macro; 3] = [
    Macro {
        name: "Gate",
        get: |t| frac(t.gate.threshold, -80.0, 0.0),
        set: |t, f| t.gate.threshold = span(f, -80.0, 0.0),
        says: |t| db(t.gate.threshold),
    },
    Macro {
        name: "Range",
        get: |t| frac(-t.gate.range, 0.0, 60.0),
        set: |t, f| t.gate.range = -span(f, 0.0, 60.0),
        says: |t| db(t.gate.range),
    },
    Macro {
        name: "Control",
        get: |t| frac(-t.rescue_comp.threshold, 0.0, 40.0),
        set: |t, f| t.rescue_comp.threshold = -span(f, 0.0, 40.0),
        says: |t| db(t.rescue_comp.threshold),
    },
];

const TONE: [Macro; 4] = [
    Macro {
        name: "Low",
        get: |t| frac(band_gain(&t.eq, false), -12.0, 12.0),
        set: |t, f| {
            if let Some(b) = end_band(&mut t.eq, false) {
                b.gain = span(f, -12.0, 12.0);
            }
        },
        says: |t| db(band_gain(&t.eq, false)),
    },
    Macro {
        name: "High",
        get: |t| frac(band_gain(&t.eq, true), -12.0, 12.0),
        set: |t, f| {
            if let Some(b) = end_band(&mut t.eq, true) {
                b.gain = span(f, -12.0, 12.0);
            }
        },
        says: |t| db(band_gain(&t.eq, true)),
    },
    Macro {
        name: "Punch",
        get: |t| frac(-t.comp.threshold, 0.0, 40.0),
        set: |t, f| t.comp.threshold = -span(f, 0.0, 40.0),
        says: |t| db(t.comp.threshold),
    },
    Macro {
        name: "Drive",
        get: |t| frac(t.sat.drive, 0.0, 10.0),
        set: |t, f| t.sat.drive = span(f, 0.0, 10.0),
        says: |t| format!("{:.1}", t.sat.drive),
    },
];

const POLISH: [Macro; 2] = [
    Macro {
        name: "De-ess",
        get: |t| f64::from(t.de_ess.depth.clamp(0.0, 1.0)),
        set: |t, f| t.de_ess.depth = span(f, 0.0, 1.0),
        says: |t| format!("{:.0}%", t.de_ess.depth * 100.0),
    },
    Macro {
        name: "Shine",
        get: |t| frac(band_gain(&t.polish_eq, true), -12.0, 12.0),
        set: |t, f| {
            if let Some(b) = end_band(&mut t.polish_eq, true) {
                b.gain = span(f, -12.0, 12.0);
            }
        },
        says: |t| db(band_gain(&t.polish_eq, true)),
    },
];

const RELATIONAL: [Macro; 1] = [Macro {
    name: "Carve",
    get: |t| frac(-band_gain(&t.space, false), 0.0, 12.0),
    set: |t, f| {
        if let Some(b) = end_band(&mut t.space, false) {
            b.gain = -span(f, 0.0, 12.0);
        }
    },
    says: |t| db(band_gain(&t.space, false)),
}];

const DEPTH: [Macro; 4] = [
    Macro {
        name: "Verb",
        get: |t| f64::from(t.reverb.mix.clamp(0.0, 1.0)),
        set: |t, f| t.reverb.mix = span(f, 0.0, 1.0),
        says: |t| format!("{:.0}%", t.reverb.mix * 100.0),
    },
    Macro {
        name: "Decay",
        get: |t| frac(t.reverb.decay, 0.1, 10.0),
        set: |t, f| t.reverb.decay = span(f, 0.1, 10.0),
        says: |t| format!("{:.1} s", t.reverb.decay),
    },
    Macro {
        name: "Delay",
        get: |t| f64::from(t.delay.mix.clamp(0.0, 1.0)),
        set: |t, f| t.delay.mix = span(f, 0.0, 1.0),
        says: |t| format!("{:.0}%", t.delay.mix * 100.0),
    },
    Macro {
        name: "Repeats",
        get: |t| f64::from(t.delay.feedback.clamp(0.0, 1.0)),
        set: |t, f| t.delay.feedback = span(f, 0.0, 0.95),
        says: |t| format!("{:.0}%", t.delay.feedback * 100.0),
    },
];

/// The knobs a folded `phase` shows.
#[must_use]
pub const fn of(phase: MixPhase) -> &'static [Macro] {
    match phase {
        MixPhase::Rescue => &RESCUE,
        MixPhase::Tone => &TONE,
        MixPhase::Polish => &POLISH,
        MixPhase::Relational => &RELATIONAL,
        MixPhase::Depth => &DEPTH,
        _ => &[],
    }
}

/// How big one knob's cell is.
pub const CELL_W: f64 = 56.0;
pub const CELL_H: f64 = 62.0;

/// Where each of `count` knobs sits in `at`, as many to a row as fit;
/// a knob that would fall off the bottom is left out.
#[must_use]
pub fn cells(count: usize, at: Panel) -> Vec<Panel> {
    let across = crate::num::index(at.width / CELL_W).max(1);
    let gap = crate::num::coord(across).mul_add(-CELL_W, at.width) / 2.0;
    (0..count)
        .map(|i| Panel {
            x: crate::num::coord(i.checked_rem(across).unwrap_or(0)).mul_add(CELL_W, at.x + gap),
            y: crate::num::coord(i.checked_div(across).unwrap_or(0)).mul_add(CELL_H, at.y),
            width: CELL_W,
            height: CELL_H,
        })
        .filter(|cell| cell.y + cell.height <= at.y + at.height + 0.5)
        .collect()
}

/// The theme's page for a knob, most particular first.
fn page(theme: &theme_runtime::Theme, phase: MixPhase, track: &str) -> Option<String> {
    let phase = phase.display_name().to_lowercase();
    [
        format!("fx.macro.{phase}.{}", track.to_lowercase()),
        format!("fx.macro.{phase}"),
        "fx.macro".to_owned(),
    ]
    .into_iter()
    .find(|name| theme.has_role(name))
}

/// One knob to draw: whose, which, where.
pub struct Knob<'a> {
    pub tone: &'a Tone,
    pub phase: MixPhase,
    /// The track's name, for a page of its own.
    pub track: &'a str,
    pub knob: &'a Macro,
    pub cell: Panel,
    /// Being turned.
    pub held: bool,
}

/// Draw `knob`: the theme's face if it has one, the painters' if not.
pub fn draw(scene: &mut Scene, palette: &Palette, font: &Font, knob: &Knob<'_>) {
    let value = (knob.knob.get)(knob.tone);
    let says = (knob.knob.says)(knob.tone);
    let tint = crate::tone::phase_tint(knob.phase);
    let cell = knob.cell;
    let theme = crate::skin::loaded();
    if let Some(theme) = theme.as_deref()
        && let Some(role) = page(theme, knob.phase, knob.track)
    {
        let [r, g, b, a] = tint.to_rgba8().to_u8_array();
        let fields = crate::skin::values(&[
            ("macro.name", theme_runtime::Value::text(knob.knob.name)),
            ("macro.value", theme_runtime::Value::Num(value)),
            ("macro.says", theme_runtime::Value::text(&says)),
            (
                "macro.tint",
                theme_runtime::Value::Color(theme_runtime::Rgba::from_rgba8(r, g, b, a)),
            ),
            ("macro.held", theme_runtime::Value::Bool(knob.held)),
        ]);
        if theme.record(
            &role,
            (cell.width, cell.height),
            &fields,
            &mut crate::skin::canvas::Host { scene, font },
            Affine::translate((cell.x, cell.y)),
        ) {
            return;
        }
    }
    painted(scene, palette, font, knob, value, &says, tint);
}

/// The painters' knob: a track arc, the value over it, the name under.
fn painted(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    knob: &Knob<'_>,
    value: f64,
    says: &str,
    tint: Color,
) {
    use std::f64::consts::PI;
    let cell = knob.cell;
    // The name over the knob, its value under: neither on the ring.
    let (cx, cy, r) = (cell.x + cell.width / 2.0, cell.y + 28.0, 13.0);
    let (start, sweep) = (PI * 0.75, PI * 1.5);
    let stroke = Stroke::new(3.0);
    let arc = Arc::new((cx, cy), (r, r), start, sweep, 0.0);
    scene.stroke(&stroke, Affine::IDENTITY, palette.divider, None, &arc);
    let lit = Arc::new((cx, cy), (r, r), start, sweep * value.clamp(0.0, 1.0), 0.0);
    scene.stroke(&stroke, Affine::IDENTITY, tint, None, &lit);
    let ink = if knob.held {
        palette.text
    } else {
        palette.text_dim
    };
    let left = |text: &str, size: f32| cx - font.width(text, size) / 2.0;
    let name = knob.knob.name;
    crate::tcp::glyphs(scene, font, ink, name, left(name, 9.0), cell.y + 9.0, 9.0);
    crate::tcp::glyphs(
        scene,
        font,
        palette.text_faint,
        says,
        left(says, 8.0),
        cell.y + 56.0,
        8.0,
    );
}
