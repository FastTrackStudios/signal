//! One front panel per profile.
//!
//! Selecting a profile does not re-tint the FTS surface any more — it swaps
//! the whole UI for the unit's own front panel. `control` is the FTS surface
//! (graph + sections), unchanged; the other three are faceplates built from
//! [`crate::hardware`].
//!
//! Every control on a hardware face is driven through
//! [`crate::profile_handle`], so what the knobs write is the profile data in
//! `comp-profiles` — the panel is a view of the mapping, not a second copy of
//! it.

pub mod control;
pub mod la2a;
pub mod ssl_bus;
pub mod urei_1176;

use std::sync::Arc;

use audiocore_core::prelude::*;
use comp_profiles::Profile;
use fts_ui_audio::ParamHandle;
use nice_plug_dioxus::prelude::ParamContext;

use crate::params::CompUiState;
use crate::profile_handle::handle_for;

/// Profile ids in [`crate::params::PROFILE_LABELS`] order — the index the
/// `profile` param holds maps onto `comp_profiles::all_profiles()` through
/// this table.
pub const PROFILE_IDS: &[&str] = &["control", "la2a", "ssl_bus", "urei_1176"];

/// The profile a `profile` param index selects.
pub fn profile_for_index(index: usize) -> &'static (dyn Profile + Sync) {
    match PROFILE_IDS.get(index).copied() {
        Some("la2a") => &comp_profiles::LA2A,
        Some("ssl_bus") => &comp_profiles::SSL_BUS,
        Some("urei_1176") => &comp_profiles::UREI_1176,
        _ => &comp_profiles::CONTROL,
    }
}

/// The editor body for a profile index.
///
/// `advanced` is the FTS surface's page selection; the hardware faces ignore
/// it (a unit has the controls it has).
///
/// `frame` is the shell's redraw tick, and it is load-bearing rather than
/// decorative: faces read plugin params and meter atomics directly rather than
/// through signals, and Dioxus memoizes a component whose props have not
/// changed — so without a prop that changes every tick, a face would render
/// once and then sit there with a frozen VU and stale knob positions.
#[component]
pub fn Face(profile_index: usize, advanced: bool, frame: u64) -> Element {
    match PROFILE_IDS.get(profile_index).copied() {
        Some("la2a") => rsx! { la2a::La2aFace { frame } },
        Some("ssl_bus") => rsx! { ssl_bus::SslBusFace { frame } },
        Some("urei_1176") => rsx! { urei_1176::Urei1176Face { frame } },
        _ => rsx! { control::ControlFace { advanced, frame } },
    }
}

/// What a hardware face needs out of context: the shared UI state (params and
/// live meters) and the host parameter context.
pub struct FaceContext {
    pub ui: Arc<CompUiState>,
    pub ctx: ParamContext,
    pub profile: &'static (dyn Profile + Sync),
}

impl FaceContext {
    /// A handle for one of the profile's controls, by id.
    ///
    /// Panics if the control is not on the profile or writes nothing this
    /// plugin exposes — both are authoring mistakes in a face, not runtime
    /// conditions, and a silently missing knob is worse than a loud one.
    pub fn handle(&self, control_id: &str) -> ParamHandle {
        handle_for(
            self.profile,
            control_id,
            self.ui.params.clone(),
            self.ctx.clone(),
        )
        .unwrap_or_else(|| {
            panic!(
                "{} has no drivable control {control_id}",
                self.profile.id()
            )
        })
    }
}

/// Pull the face's context out of the editor's Dioxus contexts.
pub fn use_face_context(profile: &'static (dyn Profile + Sync)) -> FaceContext {
    let shared = use_context::<SharedState>();
    let ui = shared.get::<CompUiState>().expect("CompUiState missing");
    FaceContext {
        ui,
        ctx: use_param_context(),
        profile,
    }
}
