//! Faces: saturation — five circuits, one per family of the FTS
//! saturator's profiles (`saturate-ui/src/faces.rs`'s `SatDesign`s), each
//! drawn as the box that circuit lives in.
//!
//! - **Tube** — a valve preamp: the valves lit behind a window, the stage
//!   selector (triode / pentode), output VU, brown bakelite.
//! - **Tape** — a studio tape machine's electronics: a meter bridge with
//!   two VUs, tape speed and formulation selectors, record level and bias.
//! - **Transformer** — iron and copper: the core and its winding drawn in
//!   an open chassis, warming as it is driven; wing knobs, LED meters.
//! - **Transistor** — a fuzz pedal: hammertone enclosure, a stomp switch,
//!   the circuit's own symbol printed large.
//! - **Digital** — a clipper / bit-crusher: an LCD drawing the staircase
//!   the word length makes, bits and rate on the glass.
//!
//! Every control but drive, mix and output is a *trim* around the circuit
//! as designed (noon = the profile's voicing), so those print − 0 + and read in % off the voicing.
//! Drive, mix and output sit in the same places on every face.

mod digital;
mod hw;
mod tape;
mod transformer;
mod transistor;
mod tube;

use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, draw};
use surface_proto::{Condition, Control, ControlKind, ParamInfo, Range, SlotRole, Unit};
use surface_widgets::Builder;

use crate::delay::parts;
use crate::face::Face;

pub fn faces() -> Vec<Face> {
    vec![tube::face(), tape::face(), transformer::face(), transistor::face(), digital::face(), tube::tall_face(), tape::tall_face(), transformer::tall_face(), transistor::tall_face(), digital::tall_face()]
}

/// What a trim prints: its ends and noon (the circuit as designed).
pub(crate) const TRIM_MARKS: &[(f64, &str)] = &[(0.0, "-"), (0.5, "0"), (1.0, "+")];
/// Output's printed scale over −24 … +12 dB.
pub(crate) const OUT_MARKS: &[(f64, &str)] = &[(0.0, "-24"), (12.0 / 36.0, "-12"), (24.0 / 36.0, "0"), (1.0, "+12")];

/// Output: −24 … +12 dB in whole dB, bipolar about unity.
pub(crate) fn output(addr: &str) -> surface_proto::ParamInfo {
    let r = surface_proto::Range { steps: Some(37), ..surface_proto::Range::linear(-24.0, 12.0, surface_proto::Unit::Db) };
    crate::delay::parts::param(addr, "Output", r, 0.0).bipolar().featured()
}

// ── Emphasis ──────────────────────────────────────────────────────────────
//
// Every saturator drives its stage through an emphasis EQ (and undoes it
// after with the mirror, never edited): six fixed bands, each a Bell, Low
// Shelf or High Shelf (a cut has no inverse) over 20 Hz–20 kHz, ±12 dB,
// Q 0.1–18. An EMPHASIS key on each face (`<ns>/emphasis_view`) swaps the
// face's picture for the curve — `viz.eq_graph`, its six bands fixed, in
// the face's own colour — while the knobs stay. The bands are
// `<ns>/emph/b{n}_{shape,freq,gain,q}`, n = 1..=6.


/// Emphasis bands per saturator.
pub(crate) const EMPH_BANDS: usize = 6;
/// The bands' default frequencies: a useful spread, all idle at 0 dB.
pub(crate) const EMPH_FREQS: [f64; EMPH_BANDS] = [80.0, 250.0, 700.0, 1800.0, 4500.0, 10_000.0];
/// The shapes a band takes, by `b{n}_shape` index (the first three of the
/// EQ graph's).
pub(crate) const EMPH_SHAPES: &[&str] = &["Bell", "Low Shelf", "High Shelf"];

/// `<ns>/emphasis_view`: on, the face shows its emphasis EQ.
fn view(ns: &str) -> String {
    format!("{ns}/emphasis_view")
}

/// The emphasis params of saturator `ns`: the view toggle and six bands.
pub(crate) fn emphasis_params(ns: &str) -> Vec<ParamInfo> {
    let mut v = vec![parts::toggle(&view(ns), "Emphasis View", false).short("Emphasis")];
    for (i, f) in EMPH_FREQS.iter().enumerate() {
        let n = i + 1;
        let addr = |k: &str| format!("{ns}/emph/b{n}_{k}");
        let group = format!("Emphasis {n}");
        v.push(parts::choice(&addr("shape"), &format!("Emph {n} Shape"), EMPH_SHAPES, 0).in_group(group.clone()).short("Shape"));
        v.push(parts::param(&addr("freq"), &format!("Emph {n} Freq"), Range::log(20.0, 20_000.0, Unit::Hz), *f).in_group(group.clone()).short("Freq"));
        v.push(parts::param(&addr("gain"), &format!("Emph {n} Gain"), Range::linear(-12.0, 12.0, Unit::Db), 0.0).bipolar().in_group(group.clone()).short("Gain"));
        v.push(parts::param(&addr("q"), &format!("Emph {n} Q"), Range::log(0.1, 18.0, Unit::None), 0.707).in_group(group).short("Q"));
    }
    v
}

/// A layer the size of `parent`, drawn only while `when` holds: a frame
/// carrying the condition on a readout of `addr` whose text is never seen.
fn layer(b: &mut Builder<'_>, parent: NodeId, name: &str, (x, y, w, h): (f64, f64, f64, f64), addr: &str, when: Condition) -> NodeId {
    let layer = b.add(parent, draw::frame(name, x, y, w, h));
    let mut seen = draw::text("seen", "", 0.0, 0.0, 1.0, 1.0, "#00000000", TextAlign::Left);
    seen.opacity = 0.0;
    let seen = b.add(layer, seen);
    b.controls.insert(layer, Control::new(ControlKind::Readout).slot(SlotRole::Value, seen.0).bound(parts::bind(addr)).shown_if(when));
    layer
}

/// The layer a face draws its picture (and the meters beside it) into:
/// the whole panel `(w, h)`, hidden while the emphasis EQ shows.
pub(crate) fn picture(b: &mut Builder<'_>, panel: NodeId, ns: &str, (w, h): (f64, f64)) -> NodeId {
    layer(b, panel, "Picture", (0.0, 0.0, w, h), &view(ns), Condition::on(view(ns)).negated())
}

/// The emphasis EQ in `(x, y, w, h)` of `panel`, shown while the view is
/// on: a recessed well holding the six-band graph in `tint`.
pub(crate) fn emphasis_eq(b: &mut Builder<'_>, panel: NodeId, ns: &str, (x, y, w, h): (f64, f64, f64, f64), tint: &str) {
    let shown = layer(b, panel, "Emphasis EQ", (x, y, w, h), &view(ns), Condition::on(view(ns)));
    let well = parts::well(b, shown, "Emphasis well", (0.0, 0.0, w, h), draw::solid("#050505"), 6.0);
    let inset = 3.0;
    let opts = [
        format!("ns={ns}/emph"),
        format!("bands={EMPH_BANDS}"),
        "fixed".to_string(),
        "no_analyser".to_string(),
        format!("shapes={}", EMPH_SHAPES.len()),
        "db_range=12".to_string(),
        format!("tint={tint}"),
        format!("h={}", h - 2.0 * inset),
    ];
    let p = Place { options: Some(opts.to_vec()), ..Place::new("Emphasis").size(w - 2.0 * inset) };
    parts::put_at(b, well, "viz.eq_graph", &p, inset, inset);
    parts::silk_left(b, well, "Drive emphasis", 10.0, 12.0, parts::Ink::new(6.0, tint).weight(800.0).track(0.2));
}

/// The EMPHASIS key at (`cx`, `cy`), `w` wide: a latching console key
/// (`variant`) whose lamp lights in `tint` while the EQ shows.
pub(crate) fn emphasis_key(b: &mut Builder<'_>, panel: NodeId, ns: &str, (cx, cy): (f64, f64), w: f64, variant: &str, tint: &str) -> NodeId {
    let opts = [format!("h={}", (w * 0.36).round()), format!("color={tint}")];
    let p = Place { options: Some(opts.to_vec()), ..Place::new("Emphasis").size(w).variant(variant).legend("Emphasis").bind(parts::bind(&view(ns))) };
    parts::put(b, panel, "button.key", &p, cx, cy)
}
