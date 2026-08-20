//! The reverb's two embedded EQ views — Post EQ and Decay Rate EQ
//! (`fx.reverb.post-eq`, `fx.reverb.decay-eq`, `fx.reverb.eq-display`),
//! on the shared EQ surface (`fx.embed-eq.one-surface`).
//!
//! - **Post EQ**: the yellow-curve idea from Pro-R 2 — six bands on the
//!   final reverb sound (Bell / Shelves / Cuts), dB scale ±24, wet gain
//!   auto-compensated downstream.
//! - **Decay Rate EQ**: the blue-curve idea — six curves of decay-TIME
//!   multipliers. The gain axis IS the rate: ±12 dB ≡ ×0.25…×4
//!   (20·log10 r), so the graph needs no second scale type.
//!
//! v1 deviation from `fx.reverb.eq-display`: the two curves are two views
//! behind one toggle rather than one shared display — the graph component
//! draws one band set today. All six bands of each kind are always present
//! (at 0 they sit idle on the centre line); dragging a dot off the graph
//! resets it rather than deleting it.

use std::sync::Arc;

use dioxus::prelude::*;
use eq_ui::eq_graph::EqGraph;
use eq_ui::eq_graph_model::{EqBand, EqBandShape};
use nice_plug::prelude::{Param, ParamPtr};
use nice_plug_dioxus::prelude::{use_param_context, ParamContext};

use crate::params::ReverbParams;

/// Which embedded EQ the view is editing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EqViewMode {
    Post,
    Decay,
}

fn post_shape_to_index(shape: EqBandShape) -> i32 {
    match shape {
        EqBandShape::LowShelf => 1,
        EqBandShape::HighShelf => 2,
        EqBandShape::LowCut => 3,
        EqBandShape::HighCut => 4,
        _ => 0,
    }
}

fn post_index_to_shape(i: i32) -> EqBandShape {
    match i {
        1 => EqBandShape::LowShelf,
        2 => EqBandShape::HighShelf,
        3 => EqBandShape::LowCut,
        4 => EqBandShape::HighCut,
        _ => EqBandShape::Bell,
    }
}

fn decay_shape_to_index(shape: EqBandShape) -> i32 {
    match shape {
        EqBandShape::LowShelf | EqBandShape::LowCut => 1,
        EqBandShape::HighShelf | EqBandShape::HighCut => 2,
        _ => 0,
    }
}

fn decay_index_to_shape(i: i32) -> EqBandShape {
    match i {
        1 => EqBandShape::LowShelf,
        2 => EqBandShape::HighShelf,
        _ => EqBandShape::Bell,
    }
}

/// One band's four param pointers plus its graph mapping, mode-resolved.
struct BandPtrs {
    shape: ParamPtr,
    freq: ParamPtr,
    gain: ParamPtr,
    q: ParamPtr,
}

fn band_ptrs(params: &ReverbParams, mode: EqViewMode, i: usize) -> BandPtrs {
    match mode {
        EqViewMode::Post => {
            let b = &params.post_eq[i];
            BandPtrs {
                shape: b.shape.as_ptr(),
                freq: b.freq_hz.as_ptr(),
                gain: b.gain_db.as_ptr(),
                q: b.q.as_ptr(),
            }
        }
        EqViewMode::Decay => {
            let b = &params.decay_eq[i];
            BandPtrs {
                shape: b.shape.as_ptr(),
                freq: b.freq_hz.as_ptr(),
                gain: b.rate_db.as_ptr(),
                q: b.q.as_ptr(),
            }
        }
    }
}

fn write(ctx: &ParamContext, ptr: ParamPtr, normalized: f32) {
    ctx.begin_set_raw(ptr);
    ctx.set_normalized_raw(ptr, normalized);
    ctx.end_set_raw(ptr);
}

/// The reverb layer's sidecar: BOTH curves side by side — Post EQ on the
/// left (what the reverb sounds like), Decay Rate EQ on the right (how long
/// each band rings) — per `fx.reverb.eq-display`.
#[component]
pub fn ReverbEqSidecar(frame: u64) -> Element {
    rsx! {
        div {
            "data-testid": "reverb-eq-sidecar",
            style: "position:absolute; inset:0; display:flex; overflow:hidden;",
            div {
                style: "position:relative; flex:1; min-width:0; overflow:hidden;",
                ReverbEqView { mode_is_decay: false, frame }
            }
            div {
                style: "position:relative; flex:1; min-width:0; overflow:hidden; \
                        border-left:1px solid var(--border, rgba(148,163,184,0.3));",
                ReverbEqView { mode_is_decay: true, frame }
            }
        }
    }
}

/// One curve: an EqGraph over the chosen band set.
#[component]
pub fn ReverbEqView(mode_is_decay: bool, frame: u64) -> Element {
    let _ = frame;
    let mode = if mode_is_decay { EqViewMode::Decay } else { EqViewMode::Post };
    let shared = use_context::<nice_plug_dioxus::SharedState>();
    let ui = shared
        .get::<crate::control_view::ReverbUi>()
        .expect("the editor was mounted without its ReverbUi");
    let params = ui.params.clone();
    let ctx = use_param_context();

    let bands_vec: Vec<EqBand> = (0..6usize)
        .map(|i| match mode {
            EqViewMode::Post => {
                let b = &params.post_eq[i];
                EqBand {
                    index: i,
                    used: true,
                    enabled: true,
                    frequency: b.freq_hz.value(),
                    gain: b.gain_db.value(),
                    q: b.q.value(),
                    shape: post_index_to_shape(b.shape.value()),
                    solo: false,
                    stereo_mode: Default::default(),
                    name: format!("P{}", i + 1),
                }
            }
            EqViewMode::Decay => {
                let b = &params.decay_eq[i];
                EqBand {
                    index: i,
                    used: true,
                    enabled: true,
                    frequency: b.freq_hz.value(),
                    gain: b.rate_db.value(),
                    q: b.q.value(),
                    shape: decay_index_to_shape(b.shape.value()),
                    solo: false,
                    stereo_mode: Default::default(),
                    name: format!("D{}", i + 1),
                }
            }
        })
        .collect();
    let mut bands = use_signal(|| bands_vec.clone());
    *bands.write() = bands_vec;

    // No instrument cheat-sheet in an embedded EQ.
    let overlay_off = use_signal(|| eq_ui::eq_graph::OverlayChoice::Off);

    let params_change: Arc<ReverbParams> = params.clone();
    let ctx_change = ctx.clone();
    let params_remove = params.clone();
    let ctx_remove = ctx.clone();

    let (label, db_range) = match mode {
        EqViewMode::Post => ("Post EQ — the reverb sound", 24.0),
        EqViewMode::Decay => ("Decay EQ — ring time ×0.25…×4", 12.0),
    };

    rsx! {
        div {
            "data-testid": if mode == EqViewMode::Decay { "decay-eq-view" } else { "post-eq-view" },
            style: "position:absolute; inset:0; overflow:hidden;",

            EqGraph {
                bands,
                db_range,
                auto_range: false,
                show_hints: false,
                overlay_sel: overlay_off,
                sample_rate: 48_000.0,
                on_band_change: move |(idx, band): (usize, EqBand)| {
                    if idx >= 6 {
                        return;
                    }
                    let ptrs = band_ptrs(&params_change, mode, idx);
                    let (shape_idx, gain) = match mode {
                        EqViewMode::Post => (
                            post_shape_to_index(band.shape),
                            band.gain.clamp(-24.0, 24.0),
                        ),
                        EqViewMode::Decay => (
                            decay_shape_to_index(band.shape),
                            band.gain.clamp(-12.0, 12.0),
                        ),
                    };
                    unsafe {
                        write(&ctx_change, ptrs.freq, ptrs.freq.preview_normalized(band.frequency));
                        write(&ctx_change, ptrs.gain, ptrs.gain.preview_normalized(gain));
                        write(&ctx_change, ptrs.q, ptrs.q.preview_normalized(band.q));
                        write(
                            &ctx_change,
                            ptrs.shape,
                            ptrs.shape.preview_normalized(shape_idx as f32),
                        );
                    }
                },
                // The six bands are fixed slots: dragging one off the graph
                // resets it to neutral.
                on_band_remove: move |idx: usize| {
                    if idx >= 6 {
                        return;
                    }
                    let ptrs = band_ptrs(&params_remove, mode, idx);
                    unsafe {
                        write(&ctx_remove, ptrs.gain, ptrs.gain.preview_normalized(0.0));
                    }
                },
            }

            div {
                style: "position:absolute; top:6px; left:0; right:0; margin:0 auto; \
                        width:max-content; font-size:10px; letter-spacing:0.08em; \
                        text-transform:uppercase; color:var(--muted-foreground); \
                        background:color-mix(in oklab, var(--card, #101216) 80%, transparent); \
                        border:1px solid var(--border, rgba(148,163,184,0.3)); \
                        border-radius:6px; padding:2px 8px; pointer-events:none;",
                "{label}"
            }
        }
    }
}
