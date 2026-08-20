//! The emphasis EQ view — the shared EQ surface embedded in the saturator
//! (`fx.sat.emphasis.display`, `fx.embed-eq.one-surface`).
//!
//! The same `EqGraph` the EQ plugin uses, bound to the six emphasis bands:
//! drag a dot and the curve chooses what distorts. The curve is drawn in the
//! drive colour by the graph's band palette; the scale is ±12 dB of drive
//! emphasis — the de-emphasis mirror is implicit and never editable
//! (`fx.sat.emphasis.mirror`).
//!
//! All six bands are always on the graph (at 0 dB they sit on the centre
//! line and do nothing — a flat band costs nothing in DSP). Dragging a band
//! off the graph resets it to 0 dB rather than deleting it; shapes coerce to
//! Bell / Low Shelf / High Shelf, the only invertible ones.

use std::collections::HashMap;
use std::sync::Arc;

use dioxus::prelude::*;
use eq_ui::eq_graph::EqGraph;
use eq_ui::eq_graph_model::{EqBand, EqBandShape};
use fts_audio_ui::ParamHandle;
use nice_plug::prelude::Param;
use nice_plug_dioxus::prelude::{use_param_context, ParamContext};

use crate::params::SatParams;

/// Coerce any graph shape onto the invertible three
/// (`fx.sat.emphasis.mirror`).
fn coerce_shape(shape: EqBandShape) -> EqBandShape {
    match shape {
        EqBandShape::LowShelf | EqBandShape::LowCut => EqBandShape::LowShelf,
        EqBandShape::HighShelf | EqBandShape::HighCut => EqBandShape::HighShelf,
        _ => EqBandShape::Bell,
    }
}

fn shape_to_index(shape: EqBandShape) -> i32 {
    match coerce_shape(shape) {
        EqBandShape::LowShelf => 1,
        EqBandShape::HighShelf => 2,
        _ => 0,
    }
}

fn index_to_shape(i: i32) -> EqBandShape {
    match i {
        1 => EqBandShape::LowShelf,
        2 => EqBandShape::HighShelf,
        _ => EqBandShape::Bell,
    }
}

/// Write one graph band back into its params, as host gestures.
fn commit_band(params: &Arc<SatParams>, ctx: &ParamContext, idx: usize, band: &EqBand) {
    let Some(bp) = params.emph.get(idx) else {
        return;
    };
    let writes: [(nice_plug::prelude::ParamPtr, f32); 4] = [
        (bp.freq_hz.as_ptr(), bp.freq_hz.preview_normalized(band.frequency)),
        (bp.gain_db.as_ptr(), bp.gain_db.preview_normalized(band.gain.clamp(-12.0, 12.0))),
        (bp.q.as_ptr(), bp.q.preview_normalized(band.q)),
        (
            bp.shape.as_ptr(),
            bp.shape.preview_normalized(shape_to_index(coerce_shape(band.shape))),
        ),
    ];
    for (ptr, normalized) in writes {
        ctx.begin_set_raw(ptr);
        ctx.set_normalized_raw(ptr, normalized);
        ctx.end_set_raw(ptr);
    }
}

/// The full-surface emphasis EQ view. Swapped in for the face by the rail's
/// EQ toggle.
#[component]
pub fn EmphasisView(
    /// The shell's redraw tick — params are read directly, not via signals.
    frame: u64,
    /// Bound handles, so the view carries the drive knob alongside the graph
    /// (the two are one workflow: choose what distorts, then how hard).
    handles: HashMap<String, ParamHandle>,
) -> Element {
    let _ = frame;
    let shared = use_context::<nice_plug_dioxus::SharedState>();
    let ui = shared
        .get::<crate::control_view::SatUi>()
        .expect("the editor was mounted without its SatUi");
    let params = ui.params.clone();
    let ctx = use_param_context();

    // The graph's working copy, refreshed from params every render — the
    // graph edits the signal live mid-drag and commits through the
    // callbacks (the eq plugin's own pattern).
    let bands_vec: Vec<EqBand> = (0..saturate_dsp::emphasis::BANDS)
        .map(|i| {
            let bp = &params.emph[i];
            EqBand {
                index: i,
                used: true,
                enabled: true,
                frequency: bp.freq_hz.value(),
                gain: bp.gain_db.value(),
                q: bp.q.value(),
                shape: index_to_shape(bp.shape.value()),
                solo: false,
                stereo_mode: Default::default(),
                name: format!("E{}", i + 1),
            }
        })
        .collect();
    let mut bands = use_signal(|| bands_vec.clone());
    *bands.write() = bands_vec;

    // No instrument cheat-sheet in an embedded EQ — the zone labels are
    // about MIX moves, and this curve is a drive control.
    let overlay_off = use_signal(|| eq_ui::eq_graph::OverlayChoice::Off);

    let params_change = params.clone();
    let ctx_change = ctx.clone();
    let params_remove = params.clone();
    let ctx_remove = ctx.clone();

    rsx! {
        div {
            "data-testid": "emphasis-view",
            style: "position:absolute; inset:0; overflow:hidden;",

            EqGraph {
                bands,
                db_range: 12.0,
                auto_range: false,
                show_hints: false,
                overlay_sel: overlay_off,
                sample_rate: 48_000.0,
                on_band_change: move |(idx, band): (usize, EqBand)| {
                    commit_band(&params_change, &ctx_change, idx, &band);
                },
                // Dragging a band off the graph resets it to 0 dB — the six
                // bands are fixed slots, not a growable list.
                on_band_remove: move |idx: usize| {
                    if let Some(bp) = params_remove.emph.get(idx) {
                        let ptr = bp.gain_db.as_ptr();
                        let normalized = bp.gain_db.preview_normalized(0.0);
                        ctx_remove.begin_set_raw(ptr);
                        ctx_remove.set_normalized_raw(ptr, normalized);
                        ctx_remove.end_set_raw(ptr);
                    }
                },
            }

            // The label that says what this scale MEANS: drive emphasis, not
            // output tone (`fx.sat.emphasis.display`).
            div {
                style: "position:absolute; top:6px; left:0; right:0; margin:0 auto; \
                        width:max-content; font-size:10px; letter-spacing:0.08em; \
                        text-transform:uppercase; color:var(--muted-foreground); \
                        background:color-mix(in oklab, var(--card, #101216) 80%, transparent); \
                        border:1px solid var(--border, rgba(148,163,184,0.3)); \
                        border-radius:6px; padding:2px 8px; pointer-events:none;",
                "Emphasis — drives the stage, mirrored out"
            }

            // Drive rides along at the bottom so shaping and pushing are one
            // motion.
            if let Some(drive) = handles.get("drive") {
                div {
                    style: "position:absolute; right:16px; bottom:12px;",
                    fts_audio_ui::controls::Knob { handle: drive.clone() }
                }
            }
        }
    }
}
