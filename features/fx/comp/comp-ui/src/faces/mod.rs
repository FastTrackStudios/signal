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
pub mod rack;
pub mod units;

use std::sync::Arc;

use audiocore_core::prelude::*;
use comp_profiles::Profile;
use fts_audio_ui::shell::ShellItem;
use fts_audio_ui::ParamHandle;
use nice_plug_dioxus::prelude::ParamContext;

use crate::params::CompUiState;
use crate::profile_handle::handle_for;

/// Profile ids in `comp_profiles::all_profiles()` order — which is the order
/// the `profile` parameter's values are in.
pub fn profile_ids() -> Vec<&'static str> {
    comp_profiles::all_profiles()
        .iter()
        .map(|p| p.id())
        .collect()
}

/// The profile id a `profile` param index selects.
pub fn profile_id_for_index(index: usize) -> &'static str {
    comp_profiles::all_profiles()
        .get(index)
        .map(|p| p.id())
        .unwrap_or("control")
}

/// The profile a `profile` param index selects.
pub fn profile_for_index(index: usize) -> &'static (dyn Profile + Sync) {
    comp_profiles::all_profiles()
        .get(index)
        .copied()
        .unwrap_or(&comp_profiles::CONTROL)
}

/// The rail badge for a profile — how the unit is named on its own panel.
pub fn profile_badge(profile_id: &str) -> &'static str {
    match profile_id {
        "control" => "MAIN",
        "urei_1176" => "76",
        "urei_1176_silver" => "76s",
        "urei_1176_ln" => "LN",
        "la2a" => "2A",
        "cl1b" => "CL",
        "fairchild670" => "670",
        "manley_vari_mu" => "MAN",
        "ssl_bus" => "SSL",
        "dbx160" => "160",
        "distressor" => "DIS",
        _ => "?",
    }
}

/// The rail, as the shell wants it: one entry per compressor family, badged
/// with the *active* unit when that family is the one you are on.
///
/// A family with more than one unit says so in its tooltip, because a button
/// that changes on a second click is not discoverable otherwise.
pub fn rail_items(profile_index: usize) -> Vec<ShellItem> {
    let active_id = profile_id_for_index(profile_index);
    let active = comp_profiles::category_of(active_id).map(|(c, _)| c);

    comp_profiles::CATEGORIES
        .iter()
        .enumerate()
        .map(|(index, category)| {
            let is_active = active == Some(index);
            let badge = if is_active {
                profile_badge(active_id)
            } else {
                category.badge
            };
            let names: Vec<&str> = category
                .profiles
                .iter()
                .filter_map(|id| comp_profiles::profile_by_id(id).map(|p| p.name()))
                .collect();
            let label = if names.len() > 1 {
                format!("{} — {} (click again to cycle)", category.label, names.join(" · "))
            } else {
                format!("{} — {}", category.label, names.join(""))
            };
            // The dots say how many units are stacked behind this family and
            // which one is showing. Clicking cycles them, and until now
            // nothing on the rail admitted that a family of three existed.
            let at = if is_active {
                comp_profiles::category_of(active_id).map(|(_, v)| v).unwrap_or(0)
            } else {
                0
            };
            ShellItem::new(category.id, label)
                .with_badge(badge)
                .with_cycle(category.profiles.len(), at)
        })
        .collect()
}

/// The `profile` parameter index a rail click selects.
///
/// Clicking the family you are already on advances to the next unit inside it
/// and wraps; clicking any other family lands on its first unit.
pub fn rail_click_target(profile_index: usize, clicked_category: usize) -> usize {
    let active_id = profile_id_for_index(profile_index);
    let Some(category) = comp_profiles::CATEGORIES.get(clicked_category) else {
        return profile_index;
    };
    let next_id = match comp_profiles::category_of(active_id) {
        Some((current, variant)) if current == clicked_category => {
            category.profiles[(variant + 1) % category.profiles.len()]
        }
        _ => category.profiles[0],
    };
    comp_profiles::profile_index(next_id).unwrap_or(profile_index)
}

/// The editor size a face wants, in logical px — including the shell rail.
///
/// A face is not a page in a fixed window; it is a different instrument, and
/// the instruments are different shapes. The FTS surface is a graph and wants
/// height. A rack unit is 4:1 and wants none — given a tall window it just
/// draws black above and below itself. So switching profile asks the host to
/// resize, the same way the plugin asks on open.
pub fn preferred_editor_size(_profile_index: usize) -> (u32, u32) {
    // Every face asks for the same box: the panel's 900x300 drawing plus the
    // rail, with a little air around it.
    //
    // The FTS surface used to ask for a tall window of its own, which meant
    // the editor jumped size every time you passed through Main on the way to
    // a unit. A window that changes shape under you while you are browsing is
    // worse than a graph with less height, so Main wears the rack's size too
    // and the window only moves when you ask it to with a size preset.
    (crate::control_view::EDITOR_W, crate::control_view::EDITOR_H)
}

/// The editor size for a profile *and* a chosen form: the form decides, except
/// for Responsive, which defers to the face.
pub fn editor_size_for(profile_index: usize, form: fts_audio_ui::EditorForm) -> (u32, u32) {
    form.editor_size(
        crate::control_view::RAIL_W,
        preferred_editor_size(profile_index),
    )
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
pub fn Face(
    profile_index: usize,
    advanced: bool,
    frame: u64,
    /// The editor's form factor — a face draws its panel or flows its controls
    /// depending on whether the panel fits the shape.
    form: fts_audio_ui::EditorForm,
) -> Element {
    let _ = frame;
    match units::design_for(profile_id_for_index(profile_index)) {
        // Rendered as a keyed list of one, which is the shape Dioxus honours
        // keys in. It has to *remount* rather than diff: the panels do not
        // share an item list, and diffing one design's items into another's
        // walks blitz's mutator off the end of a template path — the same
        // failure the EQ hit tearing down its graph.
        Some(design) => rsx! {
            for id in [design.id] {
                rack::RackFace { key: "{id}", design: *design, frame, form }
            }
        },
        None => rsx! { control::ControlFace { advanced, frame } },
    }
}

/// What a hardware face needs out of context: the shared UI state (params and
/// live meters) and the host parameter context.
pub struct FaceContext {
    pub ui: Arc<CompUiState>,
    pub ctx: ParamContext,
    pub profile: &'static (dyn Profile + Sync),
    /// The stack stage this face is editing (`fx.stack.focus`).
    pub stage: usize,
}

impl FaceContext {
    /// A handle for one of the profile's controls, by id.
    ///
    /// Panics if the control is not on the profile or writes nothing this
    /// plugin exposes — both are authoring mistakes in a face, not runtime
    /// conditions, and a silently missing knob is worse than a loud one.
    ///
    /// The exception is an empty id, which means a control the panel has and
    /// the DSP does not yet: it draws and does not move. Which controls those
    /// are is pinned by `the_unwired_controls_are_the_ones_we_know_about`, so
    /// a typo still lands in the panic above rather than here.
    pub fn handle(&self, control_id: &str) -> ParamHandle {
        if control_id.is_empty() {
            return ParamHandle::inert("Not wired", 0.5);
        }
        handle_for(
            self.profile,
            control_id,
            self.ui.params.clone(),
            self.stage,
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
        stage: crate::focus::use_focused_stage(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_view::{
        EDITOR_H, EDITOR_W,
    };

    #[test]
    fn every_face_asks_for_the_same_window() {
        // Browsing units should not move the window. Faces differ in what they
        // draw, not in the box they draw it in, and the size only changes when
        // you pick a size preset.
        let first = preferred_editor_size(0);
        assert_eq!(first, (EDITOR_W, EDITOR_H));
        for index in 1..profile_ids().len() {
            assert_eq!(
                preferred_editor_size(index),
                first,
                "{} asks for a different window",
                profile_id_for_index(index),
            );
        }
    }

    #[test]
    fn every_size_preset_is_reachable_on_every_face() {
        // A preset outside the declared bounds is clamped by the host, and two
        // presets clamped to the same box is a size button that does nothing.
        let (min_w, min_h) = crate::control_view::min_editor_size();
        let (max_w, max_h) = crate::control_view::max_editor_size();
        for index in 0..profile_ids().len() {
            for form in fts_audio_ui::EDITOR_FORMS {
                let (w, h) = editor_size_for(index, *form);
                let (w, h) = (w as f32, h as f32);
                assert!(
                    (min_w..=max_w).contains(&w) && (min_h..=max_h).contains(&h),
                    "{} at {} wants {w}x{h}, outside {min_w}x{min_h}..{max_w}x{max_h}",
                    profile_id_for_index(index),
                    form.id(),
                );
            }
        }
    }

    #[test]
    fn every_face_asks_for_a_size_the_host_is_allowed_to_give_it() {
        let (min_w, min_h) = crate::control_view::min_editor_size();
        let (max_w, max_h) = crate::control_view::max_editor_size();
        // A preferred size outside the declared resize bounds is a request the
        // host will clamp or refuse — the face would open at the wrong size
        // with nothing in the log to say why.
        for index in 0..profile_ids().len() {
            let (w, h) = preferred_editor_size(index);
            let (w, h) = (w as f32, h as f32);
            assert!(
                (min_w..=max_w).contains(&w) && (min_h..=max_h).contains(&h),
                "{} wants {w}x{h}, outside the editor's {min_w}x{min_h}..\
                 {max_w}x{max_h} bounds",
                profile_id_for_index(index),
            );
        }
    }
}
