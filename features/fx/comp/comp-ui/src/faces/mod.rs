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

/// The editor size a face wants, in logical px — including the shell rail.
///
/// A face is not a page in a fixed window; it is a different instrument, and
/// the instruments are different shapes. The FTS surface is a graph and wants
/// height. A rack unit is 4:1 and wants none — given a tall window it just
/// draws black above and below itself. So switching profile asks the host to
/// resize, the same way the plugin asks on open.
pub fn preferred_editor_size(profile_index: usize) -> (u32, u32) {
    match PROFILE_IDS.get(profile_index).copied() {
        // Rack units: the panel's 900x300 drawing plus the rail, with a little
        // air around it.
        Some("la2a") | Some("ssl_bus") | Some("urei_1176") => (1000, 348),
        _ => (
            crate::control_view::EDITOR_W,
            crate::control_view::EDITOR_H,
        ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_view::{
        EDITOR_H, EDITOR_W, MAX_EDITOR_H, MAX_EDITOR_W, MIN_EDITOR_H, MIN_EDITOR_W,
    };

    #[test]
    fn the_rack_faces_ask_for_a_shorter_window_than_the_fts_surface() {
        let (_, control_h) = preferred_editor_size(0);
        assert_eq!((preferred_editor_size(0)), (EDITOR_W, EDITOR_H));
        for index in 1..PROFILE_IDS.len() {
            let (_, face_h) = preferred_editor_size(index);
            assert!(
                face_h < control_h,
                "{} asks for {face_h}px, no shorter than the FTS surface's {control_h}px",
                PROFILE_IDS[index],
            );
        }
    }

    #[test]
    fn every_face_asks_for_a_size_the_host_is_allowed_to_give_it() {
        // A preferred size outside the declared resize bounds is a request the
        // host will clamp or refuse — the face would open at the wrong size
        // with nothing in the log to say why.
        for index in 0..PROFILE_IDS.len() {
            let (w, h) = preferred_editor_size(index);
            let (w, h) = (w as f32, h as f32);
            assert!(
                (MIN_EDITOR_W..=MAX_EDITOR_W).contains(&w)
                    && (MIN_EDITOR_H..=MAX_EDITOR_H).contains(&h),
                "{} wants {w}x{h}, outside the editor's {MIN_EDITOR_W}x{MIN_EDITOR_H}..\
                 {MAX_EDITOR_W}x{MAX_EDITOR_H} bounds",
                PROFILE_IDS[index],
            );
        }
    }
}
