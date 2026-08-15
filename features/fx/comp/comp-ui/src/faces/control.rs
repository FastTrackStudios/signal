//! The FTS control surface — the native face, not a hardware one.
//!
//! The graph *is* the editor, the way the EQ's curve is: it fills the whole
//! surface, and the controls float on top of it — knobs along the bottom,
//! meters up the right-hand edge. That is not only a look. Blitz collapses a
//! flex child that does not fit instead of clipping it, so a control row that
//! is a *sibling* of the graph is one awkward window size away from being 0 px
//! tall and unreachable; an absolutely positioned overlay cannot be squeezed.
//!
//! There is no meter panel: the graph already *is* the metering — the input
//! level fills it from the bottom, gain reduction from the top, and both are
//! read out in the corner. A strip of IN/GR/OUT bars beside that is the same
//! information twice.
//!
//! Advanced still *swaps* the knobs rather than adding a row, so the working
//! area stays the same size whichever page you are on.

use audiocore_core::prelude::*;
use fts_audio_ui::prelude::*;

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

    let is_advanced = advanced;
    let (graph_w, graph_h) = crate::comp_graph::graph_size();

    rsx! {
        // ── The graph, full surface ─────────────────────────────────────
        // Its height is pinned to the number the component also uses for its
        // viewBox, which is what keeps pointer y mapping 1:1 onto dB.
        div {
            "data-testid": "comp-graph",
            style: format!(
                "position:absolute; inset:0; width:{graph_w}px; height:{graph_h}px; \
                 overflow:hidden;"
            ),
            crate::comp_graph::CompGraph { width: graph_w, height: graph_h }
        }

        // ── Floating controls ───────────────────────────────────────────
        FloatingPanel {
            testid: "controls".to_string(),
            position: "left:14px; right:14px; bottom:14px;".to_string(),
            justify: "center".to_string(),
            wrap: true,
            gap: 22.0,

            if is_advanced {
                // Detector internals: everything shaping *when* and *how
                // fast* the compressor reacts.
                Section { label: "Detector".to_string(), skin, flat: true,
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

                // Detector-path EQ. Both filters bypass at their 20 Hz floor,
                // so parking a knob at minimum switches the filter out.
                Section { label: "Sidechain".to_string(), skin, flat: true,
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

                // Both extra dynamics stages are inert at ratio 1:1, which is
                // where they default.
                Section { label: "Expander".to_string(), skin, flat: true,
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

                Section { label: "Upward".to_string(), skin, flat: true,
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

                // Saturation shape + the soft output ceiling, plus the input
                // trim and auto-makeup that belong with the extended gain
                // staging.
                Section { label: "Character".to_string(), skin, flat: true,
                    crate::sections::ParamDropdown {
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
                // ── Dynamics — the curve itself ─────────────────────────
                Section { label: "Dynamics".to_string(), skin, flat: true,
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

                // ── Detector — how the level is measured ────────────────
                Section { label: "Detector".to_string(), skin, flat: true,
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

                // ── Character — the saturation stage ────────────────────
                Section { label: "Character".to_string(), skin, flat: true,
                    ParamKnob {
                        handle: param_handle(params.drive.as_ptr(), ctx.clone()),
                        testid: "drive".to_string(),
                    }
                }

                // ── Output — gain staging + parallel blend ──────────────
                Section { label: "Output".to_string(), skin, flat: true,
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
    }
}
