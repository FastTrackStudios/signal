//! The FTS control surface — the native face, not a hardware one.
//!
//! The compressor graph over a row of labelled sections: Detector / Dynamics /
//! Character / Output in Basic, the rest of the engine in Advanced. This is
//! the face you get when no hardware profile is selected, and the only one
//! that exposes the whole parameter tree.
//!
//! Advanced *swaps the page* rather than appending to it: blitz will not
//! overflow-scroll a height-constrained grid — a second row of sections is
//! simply allocated 0 px and collapses — so each page is sized to fit what
//! remains under the graph.

use std::sync::atomic::Ordering;

use audiocore_core::prelude::*;
use fts_ui_audio::prelude::*;

use crate::param_adapter::param_handle;
use crate::params::{CompUiState, CHARACTER_LABELS, STYLE_LABELS};
use crate::profile_view::profile_skin;
use crate::sections::{ParamKnob, Section};

#[component]
pub fn ControlFace(
    advanced: bool,
    /// The shell's redraw tick — see [`crate::faces::Face`].
    frame: u64,
) -> Element {
    let _ = frame;
    let shared = use_context::<SharedState>();
    let ui = shared.get::<CompUiState>().expect("CompUiState missing");
    let ctx = use_param_context();
    let params = &ui.params;
    let skin = profile_skin("control");

    let gr_db = ui.gain_reduction_db.load(Ordering::Relaxed);
    let input_db = ui.input_peak_db.load(Ordering::Relaxed);
    let output_db = ui.output_peak_db.load(Ordering::Relaxed);

    let is_advanced = advanced;
    let graph_h = crate::comp_graph::graph_height();

    rsx! {
        // ── Compressor graph ────────────────────────────────────────────
        // Height pinned to exactly `graph_h` CSS px so pointer y maps 1:1
        // onto the graph's viewBox (see comp_graph.rs) — the control surface
        // below takes the remaining space.
        div {
            "data-testid": "comp-graph",
            style: format!("height:{graph_h}px; flex:none; position:relative; overflow:hidden;"),
            crate::comp_graph::CompGraph { height: graph_h }
        }

        // ── Control surface ─────────────────────────────────────────────
        div {
            class: "flex-1 min-h-0 flex items-stretch gap-6 px-5 py-4",

            div {
                style: "flex:1; min-width:0; display:flex; align-items:stretch; gap:10px;",

                if is_advanced {
                    // Detector internals: everything shaping *when* and *how
                    // fast* the compressor reacts.
                    Section { label: "Detector".to_string(), skin,
                        ParamKnob {
                            handle: param_handle(params.detector_rms_mix.as_ptr(), ctx.clone()),
                            testid: "rmsmix".to_string(),
                            size: KnobSize::Small,
                        }
                        ParamKnob {
                            handle: param_handle(params.feedback.as_ptr(), ctx.clone()),
                            testid: "feedback".to_string(),
                            size: KnobSize::Small,
                        }
                        ParamKnob {
                            handle: param_handle(params.hold_ms.as_ptr(), ctx.clone()),
                            testid: "hold".to_string(),
                            size: KnobSize::Small,
                        }
                        ParamKnob {
                            handle: param_handle(params.lookahead_ms.as_ptr(), ctx.clone()),
                            testid: "lookahead".to_string(),
                            size: KnobSize::Small,
                        }
                        ParamKnob {
                            handle: param_handle(params.inertia.as_ptr(), ctx.clone()),
                            testid: "inertia".to_string(),
                            size: KnobSize::Small,
                        }
                        ParamKnob {
                            handle: param_handle(params.inertia_decay.as_ptr(), ctx.clone()),
                            testid: "inertiadecay".to_string(),
                            size: KnobSize::Small,
                        }
                    }

                    // Detector-path EQ. Both filters bypass at their 20 Hz
                    // floor, so parking a knob at minimum switches the filter
                    // out.
                    Section { label: "Sidechain".to_string(), skin,
                        ParamKnob {
                            handle: param_handle(params.sidechain_freq.as_ptr(), ctx.clone()),
                            testid: "schp".to_string(),
                            size: KnobSize::Small,
                        }
                        ParamKnob {
                            handle: param_handle(params.sidechain_lowpass_freq.as_ptr(), ctx.clone()),
                            testid: "sclp".to_string(),
                            size: KnobSize::Small,
                        }
                        ParamKnob {
                            handle: param_handle(params.range_db.as_ptr(), ctx.clone()),
                            testid: "range".to_string(),
                            size: KnobSize::Small,
                        }
                    }

                    // Both extra dynamics stages are inert at ratio 1:1, which
                    // is where they default.
                    Section { label: "Expander".to_string(), skin,
                        ParamKnob {
                            handle: param_handle(params.expander_threshold_db.as_ptr(), ctx.clone()),
                            testid: "expthresh".to_string(),
                            size: KnobSize::Small,
                        }
                        ParamKnob {
                            handle: param_handle(params.expander_ratio.as_ptr(), ctx.clone()),
                            testid: "expratio".to_string(),
                            size: KnobSize::Small,
                        }
                    }

                    Section { label: "Upward".to_string(), skin,
                        ParamKnob {
                            handle: param_handle(params.upward_threshold_db.as_ptr(), ctx.clone()),
                            testid: "upthresh".to_string(),
                            size: KnobSize::Small,
                        }
                        ParamKnob {
                            handle: param_handle(params.upward_ratio.as_ptr(), ctx.clone()),
                            testid: "upratio".to_string(),
                            size: KnobSize::Small,
                        }
                    }

                    // Saturation shape + the soft output ceiling, plus the
                    // input trim and auto-makeup that belong with the extended
                    // gain staging.
                    Section { label: "Character".to_string(), skin,
                        crate::sections::ParamSelector {
                            handle: param_handle(params.character_mode.as_ptr(), ctx.clone()),
                            testid: "charmode".to_string(),
                            label: "Shape".to_string(),
                            options: CHARACTER_LABELS.iter().map(|s| s.to_string()).collect(),
                            skin,
                        }
                        ParamKnob {
                            handle: param_handle(params.ceiling.as_ptr(), ctx.clone()),
                            testid: "ceiling".to_string(),
                            size: KnobSize::Small,
                        }
                        ParamKnob {
                            handle: param_handle(params.input_gain_db.as_ptr(), ctx.clone()),
                            testid: "ingain".to_string(),
                            size: KnobSize::Small,
                        }
                        div {
                            "data-testid": "toggle-automake",
                            style: "align-self:center;",
                            Toggle {
                                handle: param_handle(params.auto_makeup.as_ptr(), ctx.clone()),
                                color: skin.accent.to_string(),
                            }
                        }
                    }
                } else {
                    // ── Dynamics — the curve itself ─────────────────────
                    Section { label: "Dynamics".to_string(), skin,
                        ParamKnob {
                            handle: param_handle(params.threshold_db.as_ptr(), ctx.clone()),
                            testid: "threshold".to_string(),
                        }
                        ParamKnob {
                            handle: param_handle(params.ratio.as_ptr(), ctx.clone()),
                            testid: "ratio".to_string(),
                        }
                        ParamKnob {
                            handle: param_handle(params.attack_ms.as_ptr(), ctx.clone()),
                            testid: "attack".to_string(),
                        }
                        ParamKnob {
                            handle: param_handle(params.release_ms.as_ptr(), ctx.clone()),
                            testid: "release".to_string(),
                        }
                        ParamKnob {
                            handle: param_handle(params.knee_db.as_ptr(), ctx.clone()),
                            testid: "knee".to_string(),
                        }
                    }

                    // ── Detector — how the level is measured ────────────
                    Section { label: "Detector".to_string(), skin,
                        crate::sections::ParamSelector {
                            handle: param_handle(params.style.as_ptr(), ctx.clone()),
                            testid: "style".to_string(),
                            label: "Style".to_string(),
                            options: STYLE_LABELS.iter().map(|s| s.to_string()).collect(),
                            skin,
                        }
                        ParamKnob {
                            handle: param_handle(params.stereo_link.as_ptr(), ctx.clone()),
                            testid: "link".to_string(),
                            size: KnobSize::Small,
                        }
                    }

                    // ── Character — the saturation stage ────────────────
                    Section { label: "Character".to_string(), skin,
                        ParamKnob {
                            handle: param_handle(params.drive.as_ptr(), ctx.clone()),
                            testid: "drive".to_string(),
                        }
                    }

                    // ── Output — gain staging + parallel blend ──────────
                    Section { label: "Output".to_string(), skin,
                        ParamKnob {
                            handle: param_handle(params.makeup_db.as_ptr(), ctx.clone()),
                            testid: "makeup".to_string(),
                        }
                        ParamKnob {
                            handle: param_handle(params.mix.as_ptr(), ctx.clone()),
                            testid: "mix".to_string(),
                        }
                    }
                }
            }

            // Metering — GR (fed by the audio thread through CompUiState)
            // flanked by I/O peak meters.
            div {
                class: "flex items-end gap-3 shrink-0",
                "data-testid": "meters",
                LevelMeterDb { level_db: input_db, label: "IN".to_string(), height: 160.0 }
                GrMeter { gain_reduction_db: gr_db, height: 160.0 }
                LevelMeterDb { level_db: output_db, label: "OUT".to_string(), height: 160.0 }
            }
        }
    }
}
