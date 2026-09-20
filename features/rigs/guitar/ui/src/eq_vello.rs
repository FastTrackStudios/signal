//! The EQ plugin's own editor, in the rig — the real one, not a drawing of it.
//!
//! [`eq_ui::eq_graph::EqGraph`] is the surface the FTS-EQ plugin puts on
//! screen: its vello painter, its hit-testing, its interaction rules — drag a
//! node for frequency and gain, wheel for Q, double-click to add a band with
//! the shape inferred from where you clicked, drag one out to remove it. Its
//! props are `bands` in and band events out, with no parameter tree, so the
//! only thing standing between it and a detached rig was a renderer that can
//! be handed a painted scene. The desktop app is on Blitz now, so it can.
//!
//! # Why not keep re-hosting it
//!
//! [`crate::eq_surface`] draws the same model with the same maths in hand-
//! written SVG, because that was the only thing a WebView could show. It cost
//! more than duplication: an SVG re-host has to measure its own element to map
//! a pointer into graph space, which is asynchronous and unreliable under
//! Blitz, so the surface could be looked at but not edited. And a curve that
//! disagrees with the plugin's curve is worse than no curve, because it will be
//! believed.
//!
//! The SVG path stays for wasm, where there is no GPU surface to paint into.
//! Both read the same [`eq_ui::eq_graph_model::EqBand`], so they cannot drift
//! about what a band *is* — only about how it is drawn.
//!
//! # Wire scheme
//!
//! 24 bands × `b{i}_{used,on,freq,gain,q,shape,slope}` on the EQ block, one
//! `set_block_param` per changed field. The graph hands back a whole `EqBand`,
//! so this writes only the fields that actually moved — a drag is a stream of
//! events, and sending seven parameters per frame would put six of them on the
//! wire for nothing.

use dioxus::prelude::*;
use eq_ui::eq_graph::EqGraph;
use eq_ui::eq_graph_model::EqBand;
use signal_guitar_proto::LiveBlock;
use signal_guitar_proto::rig::RigClient;

use crate::eq_surface::{NUM_BANDS, bands_of, shape_index};

/// The plugin's graph, driving one rig block over the wire.
#[component]
pub fn EqVelloSurface(block: LiveBlock, spectrum: Vec<f32>) -> Element {
    // The analyser moves without the DOM changing — the spectrum lives inside
    // the widget's scene — so the same clock the other painted panels use.
    // Without it the graph draws once and the spectrum never appears, which
    // looks like the data not arriving rather than the frame not being asked
    // for.
    crate::fx_viz::use_repaint_clock();
    let rig = use_hook(try_consume_context::<RigClient>);
    let block_id = block.id.clone();

    // The graph owns a signal, so the wire's values are copied in whenever the
    // rig reports something different. Not on every render: the graph writes to
    // this signal as the pointer moves, and overwriting it from a status event
    // mid-drag would fight the hand.
    let from_wire = bands_of(&block);
    let mut bands = use_signal(|| from_wire.clone());
    let mut mirrored = use_signal(|| from_wire.clone());
    if *mirrored.peek() != from_wire {
        mirrored.set(from_wire.clone());
        bands.set(from_wire);
    }

    // The display range is the editor's, not the rig's — it changes what you
    // see, not what you hear, so it stays local rather than becoming a param.
    let mut db_range = use_signal(|| 12.0f64);

    let write = use_hook(move || {
        let rig = rig.clone();
        let block_id = block_id.clone();
        move |band: usize, field: &'static str, value: f32| {
            if let Some(r) = rig.clone() {
                let id = block_id.clone();
                let name = format!("b{}_{}", band + 1, field);
                spawn(async move {
                    let _ = r.set_block_param(id, name, value).await;
                });
            }
        }
    });

    rsx! {
        EqGraph {
            bands,
            db_range: db_range(),
            on_db_range_change: move |r: f64| db_range.set(r),
            spectrum_db: (!spectrum.is_empty()).then(|| spectrum.clone()),
            // The rig's EQ sits in a chain; the mix-EQ teaching furniture is
            // authored for the plugin's own window and its words do not apply
            // to a hundred-pixel-tall panel in a guitar rack.
            show_hints: false,
            on_band_change: {
                let write = write.clone();
                move |(index, band): (usize, EqBand)| {
                    // Only what moved. A drag is a stream, and seven
                    // parameters a frame is six wasted round-trips.
                    let before = mirrored.peek().get(index).cloned();
                    for (field, now, was) in changed_fields(&band, before.as_ref()) {
                        let _ = was;
                        write(index, field, now);
                    }
                }
            },
            on_band_add: {
                let write = write.clone();
                move |band: EqBand| {
                    // The rig's chain has a fixed 24 slots; adding a band is
                    // claiming a free one, not growing the list.
                    let free = bands.peek().iter().position(|b| !b.used);
                    let Some(index) = free.filter(|i| *i < NUM_BANDS) else {
                        // Every slot claimed — the chain's 24 are all in use.
                        return;
                    };
                    write(index, "freq", band.frequency as f32);
                    write(index, "gain", band.gain as f32);
                    write(index, "q", band.q as f32);
                    write(index, "shape", shape_index(band.shape));
                    write(index, "on", 1.0);
                    // `used` last: it is what makes the band real, and a band
                    // that becomes real before its frequency arrives is a band
                    // that is briefly audible at the wrong pitch.
                    write(index, "used", 1.0);
                }
            },
            on_band_remove: {
                let write = write.clone();
                move |index: usize| write(index, "used", 0.0)
            },
        }
    }
}

/// Which of a band's wire fields differ from what the rig last reported.
///
/// `None` for `was` — a band the rig has not told us about yet — sends
/// everything, which is what claiming a fresh slot needs.
fn changed_fields(now: &EqBand, was: Option<&EqBand>) -> Vec<(&'static str, f32, f32)> {
    let mut out = Vec::new();
    let mut push = |field: &'static str, a: f32, b: f32| {
        // A wire value only has so much precision, and a drag reports a
        // continuous position; anything under this is not a move.
        if (a - b).abs() > 1e-4 {
            out.push((field, a, b));
        }
    };
    let (f, g, q) = (now.frequency as f32, now.gain as f32, now.q as f32);
    let slope = now.slope.unwrap_or(2.0) as f32;
    match was {
        Some(was) => {
            push("freq", f, was.frequency as f32);
            push("gain", g, was.gain as f32);
            push("q", q, was.q as f32);
            push("slope", slope, was.slope.unwrap_or(2.0) as f32);
            if now.shape != was.shape {
                out.push(("shape", shape_index(now.shape), shape_index(was.shape)));
            }
            if now.used != was.used {
                out.push(("used", f32::from(u8::from(now.used)), 0.0));
            }
            if now.enabled != was.enabled {
                out.push(("on", f32::from(u8::from(now.enabled)), 0.0));
            }
        }
        None => {
            out.push(("freq", f, 0.0));
            out.push(("gain", g, 0.0));
            out.push(("q", q, 0.0));
            out.push(("shape", shape_index(now.shape), 0.0));
            out.push(("on", f32::from(u8::from(now.enabled)), 0.0));
            out.push(("used", f32::from(u8::from(now.used)), 0.0));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use eq_ui::eq_graph_model::EqBandShape;

    fn band(freq: f32, gain: f32) -> EqBand {
        EqBand {
            index: 0,
            used: true,
            enabled: true,
            frequency: freq,
            gain,
            q: 0.707,
            shape: EqBandShape::Bell,
            slope: Some(2.0),
            focus: false,
            stereo_mode: Default::default(),
            name: String::new(),
        }
    }

    /// A drag sends what moved and nothing else — the whole point, since the
    /// graph reports a complete band on every pointer event.
    #[test]
    fn only_the_moved_field_goes_on_the_wire() {
        let was = band(1000.0, 0.0);
        let mut now = was.clone();
        now.gain = 3.0;
        let fields: Vec<&str> = changed_fields(&now, Some(&was))
            .iter()
            .map(|(f, _, _)| *f)
            .collect();
        assert_eq!(fields, vec!["gain"]);
    }

    /// A band the rig has not seen sends everything, so claiming a free slot
    /// leaves nothing at a stale value.
    #[test]
    fn a_new_band_sends_every_field() {
        let fields: Vec<&str> = changed_fields(&band(220.0, -2.0), None)
            .iter()
            .map(|(f, _, _)| *f)
            .collect();
        for field in ["freq", "gain", "q", "shape", "on", "used"] {
            assert!(fields.contains(&field), "{field} missing from {fields:?}");
        }
    }

    /// Pointer noise is not a move: an unchanged band sends nothing at all.
    #[test]
    fn an_unmoved_band_sends_nothing() {
        let was = band(1000.0, 0.0);
        assert!(changed_fields(&was.clone(), Some(&was)).is_empty());
    }

    /// A shape change is a change even though it is not a number that drifts.
    #[test]
    fn a_shape_change_is_sent() {
        let was = band(1000.0, 0.0);
        let mut now = was.clone();
        now.shape = EqBandShape::HighShelf;
        let fields: Vec<&str> = changed_fields(&now, Some(&was))
            .iter()
            .map(|(f, _, _)| *f)
            .collect();
        assert_eq!(fields, vec!["shape"]);
    }
}
