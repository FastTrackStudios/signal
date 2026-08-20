//! A stage's sidecar: the sidechain EQ (`fx.embed-eq.one-surface`).
//!
//! The shared EQ surface bound to ONE stage's `sc_eq` bands — the curve on
//! what this compressor's detector hears, not on the audio. Boost 8 kHz and
//! the stage de-esses; cut everything below 150 Hz and the kick stops
//! pumping the bus. Rendered inside the stage's row, under its face, so
//! every layer of the stack carries its own.

use std::sync::Arc;

use audiocore_core::prelude::*;
use eq_ui::eq_graph::{EqGraph, OverlayChoice};
use eq_ui::eq_graph_model::{EqBand, EqBandShape};

use crate::params::{CompParams, CompUiState, SC_EQ_BANDS};

fn shape_to_index(shape: EqBandShape) -> i32 {
    match shape {
        EqBandShape::LowShelf => 1,
        EqBandShape::HighShelf => 2,
        EqBandShape::LowCut => 3,
        EqBandShape::HighCut => 4,
        _ => 0,
    }
}

fn index_to_shape(i: i32) -> EqBandShape {
    match i {
        1 => EqBandShape::LowShelf,
        2 => EqBandShape::HighShelf,
        3 => EqBandShape::LowCut,
        4 => EqBandShape::HighCut,
        _ => EqBandShape::Bell,
    }
}

/// The sidecar strip: header + graph, bound to `stage`'s sidechain bands.
#[component]
pub fn SidechainEqView(stage: usize, frame: u64, accent: String) -> Element {
    let _ = frame;
    let shared = use_context::<SharedState>();
    let ui = shared.get::<CompUiState>().expect("CompUiState missing");
    let ctx = use_param_context();
    let params: Arc<CompParams> = ui.params.clone();

    let bands_vec: Vec<EqBand> = (0..SC_EQ_BANDS)
        .map(|i| {
            let bp = &params.stage(stage).sc_eq[i];
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
                name: format!("SC{}", i + 1),
            }
        })
        .collect();
    let mut bands = use_signal(|| bands_vec.clone());
    *bands.write() = bands_vec;

    let overlay_off = use_signal(|| OverlayChoice::Off);

    let params_change = params.clone();
    let ctx_change = ctx.clone();
    let params_remove = params.clone();
    let ctx_remove = ctx.clone();

    rsx! {
        div {
            "data-testid": "sc-eq-view-{stage + 1}",
            style: "position:absolute; inset:0; display:flex; \
                    flex-direction:column; overflow:hidden;",

            // Header bar — the label lives above the graph, not over it.
            div {
                style: format!(
                    "flex:none; height:20px; display:flex; align-items:center; \
                     padding:0 10px; font-size:9px; letter-spacing:0.08em; \
                     text-transform:uppercase; font-weight:700; color:{accent}; \
                     border-bottom:1px solid var(--border, rgba(148,163,184,0.2));"
                ),
                "Sidechain EQ — what S{stage + 1} listens to"
            }

            // The graph, framed with breathing room so the curve does not
            // run into the column edges.
            div {
                "data-testid": "sc-eq-graph-{stage + 1}",
                style: "flex:1; min-height:0; position:relative; margin:8px; \
                        border:1px solid var(--border, rgba(148,163,184,0.25)); \
                        border-radius:6px; overflow:hidden; \
                        background:color-mix(in oklab, var(--card, #101216) 30%, transparent);",

            EqGraph {
                bands,
                db_range: 24.0,
                auto_range: false,
                show_hints: false,
                overlay_sel: overlay_off,
                sample_rate: 48_000.0,
                on_band_change: move |(idx, band): (usize, EqBand)| {
                    if idx >= SC_EQ_BANDS {
                        return;
                    }
                    let bp = &params_change.stage(stage).sc_eq[idx];
                    let writes = [
                        (bp.freq_hz.as_ptr(), bp.freq_hz.preview_normalized(band.frequency)),
                        (
                            bp.gain_db.as_ptr(),
                            bp.gain_db.preview_normalized(band.gain.clamp(-24.0, 24.0)),
                        ),
                        (bp.q.as_ptr(), bp.q.preview_normalized(band.q)),
                        (
                            bp.shape.as_ptr(),
                            bp.shape
                                .preview_normalized(shape_to_index(band.shape)),
                        ),
                    ];
                    for (ptr, normalized) in writes {
                        ctx_change.begin_set_raw(ptr);
                        ctx_change.set_normalized_raw(ptr, normalized);
                        ctx_change.end_set_raw(ptr);
                    }
                },
                // Fixed slots: dragging a band off the graph resets it.
                on_band_remove: move |idx: usize| {
                    if idx >= SC_EQ_BANDS {
                        return;
                    }
                    let bp = &params_remove.stage(stage).sc_eq[idx];
                    let ptr = bp.gain_db.as_ptr();
                    let normalized = bp.gain_db.preview_normalized(0.0);
                    ctx_remove.begin_set_raw(ptr);
                    ctx_remove.set_normalized_raw(ptr, normalized);
                    ctx_remove.end_set_raw(ptr);
                },
            }
            }
        }
    }
}
