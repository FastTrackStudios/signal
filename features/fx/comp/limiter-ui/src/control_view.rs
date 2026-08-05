//! Limiter editor — Dioxus GUI root component.
//!
//! Header, the scrolling gain-reduction trace, then one row of sections
//! (Gain / Release / Ceiling) beside the I/O + GR meters. Everything but the
//! trace comes from [`fts_plug_ui`]'s shared chrome.

use audiocore_core::prelude::*;
use fts_plug_ui::prelude::*;
use nice_plug_dioxus::SharedState;
use std::sync::atomic::Ordering;

use crate::gr_trace::{GRAPH_H, GrTrace};
use crate::params::LimiterUiState;

/// Editor size requested from the host.
///
/// Blitz does not overflow-scroll a height-constrained container — a section
/// that does not fit collapses to 0×0 and becomes unhittable rather than being
/// clipped — so this is a constraint of the surface, not a preference.
/// `advanced_page_fits_the_plugin_editor_size`-style tests guard it.
pub const EDITOR_W: u32 = 720;
pub const EDITOR_H: u32 = 560;

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

    let gr_db = ui.gain_reduction_db.load(Ordering::Relaxed);
    let input_db = ui.input.db();
    let output_db = ui.output.db();

    rsx! {
        PluginRoot {
            title: "FTS Limiter".to_string(),
            subtitle: "Brickwall Limiter".to_string(),
            skin,

            // Height pinned to exactly GRAPH_H CSS px so the trace's fixed
            // viewBox maps 1:1 onto the element.
            div {
                "data-testid": "limiter-trace",
                style: "height:{GRAPH_H}px; flex:none; position:relative; overflow:hidden;",
                GrTrace { skin }
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
