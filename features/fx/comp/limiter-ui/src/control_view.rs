//! Limiter editor — Dioxus GUI root component.
//!
//! Header, the scrolling gain-reduction trace, then one row of sections
//! (Gain / Release / Ceiling) beside the I/O + GR meters. Everything but the
//! trace comes from [`fts_plug_ui`]'s shared chrome.

use audiocore_core::prelude::*;
use fts_plug_ui::prelude::*;
use nice_plug::editor::dpi::LogicalSize;
use nice_plug::editor::ResizeHint;
use nice_plug_dioxus::SharedState;
use std::sync::atomic::Ordering;

use crate::gr_trace::{GRAPH_H, GrTrace};
use crate::params::LimiterUiState;

/// Editor size requested from the host on open.
///
/// The starting size, not a ceiling — the editor opts into host resizing
/// through [`resize_hint`].
pub const EDITOR_W: u32 = 720;
pub const EDITOR_H: u32 = 560;

/// Smallest size the surface still works at.
///
/// Enforced by `DioxusEditorHandle::set_size` rather than advisory: blitz
/// collapses a container that does not fit to 0×0 instead of clipping it, so
/// too small a minimum yields unreachable controls rather than a cramped
/// editor. `surface_survives_the_declared_minimum_size` keeps it honest.
pub const MIN_EDITOR_W: f32 = 560.0;
pub const MIN_EDITOR_H: f32 = 460.0;

/// How the host may resize this editor: freely on both axes above the minimum.
/// The trace is the part that benefits from extra width — a longer time window
/// makes the limiter's release behaviour much easier to read.
pub fn resize_hint() -> ResizeHint {
    ResizeHint::RESIZABLE.with_min_logical_size(LogicalSize::new(MIN_EDITOR_W, MIN_EDITOR_H))
}

/// The limiter's identity colour.
pub fn skin() -> Skin {
    Skin::accented(accents::LIMITER)
}

#[component]
pub fn App() -> Element {
    rsx! {
        PluginApp {
            tailwind_css: include_str!("../assets/tailwind.css").to_string(),
            AppShell {}
        }
    }
}

#[component]
fn AppShell() -> Element {
    let shared = use_context::<SharedState>();
    let ui = shared
        .get::<LimiterUiState>()
        .expect("LimiterUiState missing");
    let ctx = use_param_context();
    let params = &ui.params;
    let skin = skin();

    // Owned here, not by the chrome: this is the scope that loads the meter
    // atomics below, and `schedule_update` only dirties the scope it is called
    // in. See `fts_plug_ui::chrome::use_redraw_tick`.
    let frame = use_redraw_tick();

    let gr_db = ui.gain_reduction_db.load(Ordering::Relaxed);
    let input_db = ui.input.db();
    let output_db = ui.output.db();

    rsx! {
        PluginRoot {
            title: "FTS Limiter".to_string(),
            subtitle: "Brickwall Limiter".to_string(),
            skin,
            frame,

            // Height pinned to exactly GRAPH_H CSS px so the trace's fixed
            // viewBox maps 1:1 onto the element.
            div {
                "data-testid": "limiter-trace",
                style: "height:{GRAPH_H}px; flex:none; position:relative; overflow:hidden;",
                GrTrace { skin, frame }
            }

            ControlSurface {
                aside: rsx! {
                    IoGrMeters { input_db, output_db, gain_reduction_db: gr_db }
                },

                // How hard the signal is pushed into the ceiling.
                Section { label: "Gain".to_string(), skin,
                    ParamKnob {
                        handle: param_handle(params.input_gain.as_ptr(), ctx.clone()),
                        testid: "ingain".to_string(),
                    }
                }

                // Attack is instantaneous by construction, so release is the
                // only time constant the user has.
                Section { label: "Release".to_string(), skin,
                    ParamKnob {
                        handle: param_handle(params.release_ms.as_ptr(), ctx.clone()),
                        testid: "release".to_string(),
                    }
                }

                // The ceiling stage: where it sits, how it is reached, and
                // whether inter-sample peaks count against it.
                Section { label: "Ceiling".to_string(), skin,
                    ParamKnob {
                        handle: param_handle(params.ceiling.as_ptr(), ctx.clone()),
                        testid: "ceiling".to_string(),
                    }
                    ParamKnob {
                        handle: param_handle(params.character.as_ptr(), ctx.clone()),
                        testid: "character".to_string(),
                    }
                    div {
                        style: "display:flex; flex-direction:column; gap:4px; align-self:center;",
                        div {
                            style: "font-size:10px; color:{skin.text}; letter-spacing:0.06em;",
                            "True Peak"
                        }
                        ParamToggle {
                            handle: param_handle(params.true_peak.as_ptr(), ctx.clone()),
                            testid: "truepeak".to_string(),
                            skin,
                        }
                    }
                }
            }
        }
    }
}
