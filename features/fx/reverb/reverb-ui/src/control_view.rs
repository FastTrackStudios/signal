//! The reverb editor: a rail of seven families, and the panel of whichever
//! one is selected.
//!
//! Same shell as the compressor, deliberately — the two plugins are the same
//! instrument as far as your hands are concerned. Clicking the family you are
//! already on cycles the spaces inside it.

use std::collections::HashMap;
use std::sync::Arc;

use dioxus::prelude::*;
use architect_ui::prelude::{default_theme_preset, ThemeMode, ThemeProvider, ThemeState};
use fts_audio_ui::prelude::*;
use fts_audio_ui::shell::{PluginShell, RailButton, ShellItem};
use nice_plug::editor::dpi::LogicalSize;
use nice_plug::prelude::Param;
use nice_plug::editor::ResizeHint;
use nice_plug_dioxus::prelude::use_param_context;

use crate::faces::SpaceFace;
use crate::param_adapter::param_handle;
use crate::params::{ReverbParams, ReverbUiState};

pub use fts_audio_ui::shell::RAIL_W;

/// Editor size on open. The faces are drawn 2U, and this is that drawing plus
/// the rail and a margin — see the compressor's note on why every face asks
/// for the same box.
pub const EDITOR_W: u32 = 1280;
pub const EDITOR_H: u32 = 440;

fn bounds() -> ((u32, u32), (u32, u32)) {
    fts_audio_ui::EditorForm::size_bounds(RAIL_W, (EDITOR_W, EDITOR_H))
}

pub fn min_editor_size() -> (f32, f32) {
    let ((w, h), _) = bounds();
    (w as f32, h as f32)
}

pub fn max_editor_size() -> (f32, f32) {
    let (_, (w, h)) = bounds();
    ((w as f32 * 2.0).max(1960.0), (h as f32 * 1.4).max(1320.0))
}

/// Freely resizable between the extremes of the size presets — a bound tighter
/// than a preset the rail offers is a button that does nothing.
pub fn resize_hint() -> ResizeHint {
    let (min_w, min_h) = min_editor_size();
    let (max_w, max_h) = max_editor_size();
    ResizeHint::RESIZABLE.with_min_max_logical_size(
        Some(LogicalSize::new(min_w, min_h)),
        Some(LogicalSize::new(max_w, max_h)),
    )
}

/// Everything the editor needs from the plugin.
#[derive(Clone)]
pub struct ReverbUi {
    pub params: Arc<ReverbParams>,
    pub state: Arc<ReverbUiState>,
}

/// The editor's size for a profile and a chosen form.
pub fn editor_size_for(_profile_index: usize, form: fts_audio_ui::EditorForm) -> (u32, u32) {
    form.editor_size(RAIL_W, (EDITOR_W, EDITOR_H))
}

/// Root editor component. Takes no props — the plugin puts [`ReverbUi`] in
/// context — so the same component serves the plugin editor and any standalone
/// mount.
#[component]
pub fn App() -> Element {
    // `create_dioxus_editor_with_state` hands its argument over as a
    // type-erased `SharedState`, so this is where it comes back out. The
    // headless harness provides the same thing, so one mount serves both.
    let shared = use_context::<nice_plug_dioxus::SharedState>();
    let ui = shared
        .get::<ReverbUi>()
        .expect("the editor was mounted without its ReverbUi");
    let params = ui.params.clone();
    let ctx = use_param_context();
    let theme = use_signal(|| ThemeState::new(default_theme_preset(), ThemeMode::Dark));

    // The redraw tick. A DOM mutation every frame is what keeps blitz
    // considering the document dirty, and it is also what stops the face
    // being memoized against stale parameter values.
    let mut app_tick = use_signal(|| 0u64);
    app_tick += 1;
    let frame = *app_tick.read();

    let profile_index = params.resolved_profile_index();
    let profile = params.resolved_profile();
    let design = crate::faces::design_for(profile.id);
    let form = params.resolved_editor_form();

    // Profile or form change → ask the host to resize. A plain Cell rather
    // than an effect: the profile lives in a plugin param, not a signal, so
    // comparing here also catches the host automating it.
    #[allow(clippy::type_complexity)]
    let last: std::rc::Rc<std::cell::Cell<Option<(usize, fts_audio_ui::EditorForm)>>> =
        use_hook(|| std::rc::Rc::new(std::cell::Cell::new(None)));
    if last.get() != Some((profile_index, form)) {
        last.set(Some((profile_index, form)));
        // Keep the automatable index in step with the persisted id.
        if params.profile.value().max(0) as usize != profile_index {
            let ptr = params.profile.as_ptr();
            let count = reverb_profiles::PROFILES.len();
            let normalized = if count > 1 {
                profile_index as f32 / (count - 1) as f32
            } else {
                0.0
            };
            ctx.begin_set_raw(ptr);
            ctx.set_normalized_raw(ptr, normalized);
            ctx.end_set_raw(ptr);
        }
        if let Some(state) = try_consume_context::<Arc<nice_plug_dioxus::DioxusState>>() {
            let (w, h) = editor_size_for(profile_index, form);
            if state.size() != (w, h) {
                state.request_resize(w, h);
            }
        }
    }

    // Every control the faces can bind, by name.
    let handles: HashMap<String, ParamHandle> = [
        ("decay", params.decay.as_ptr()),
        ("size", params.size.as_ptr()),
        ("predelay", params.predelay.as_ptr()),
        ("damping", params.damping.as_ptr()),
        ("tone", params.tone.as_ptr()),
        ("width", params.width.as_ptr()),
        ("mix", params.mix.as_ptr()),
        ("diffusion", params.diffusion.as_ptr()),
        ("modulation", params.modulation.as_ptr()),
        ("bass", params.bass.as_ptr()),
        ("character_a", params.character_a.as_ptr()),
        ("character_b", params.character_b.as_ptr()),
        ("shimmer_interval", params.shimmer_interval.as_ptr()),
        ("springs", params.springs.as_ptr()),
        ("harmonics", params.harmonics.as_ptr()),
        ("singers", params.singers.as_ptr()),
        ("regen", params.regen.as_ptr()),
        ("chop", params.chop.as_ptr()),
    ]
    .into_iter()
    .map(|(name, ptr)| (name.to_string(), param_handle(ptr, ctx.clone())))
    .collect();

    // One rail entry per family, badged with the space that is active in it.
    let active_category = reverb_profiles::category_of(profile.id)
        .map(|(c, _)| c)
        .unwrap_or(0);
    let items: Vec<ShellItem> = reverb_profiles::CATEGORIES
        .iter()
        .enumerate()
        .map(|(index, category)| {
            let badge = if index == active_category {
                profile_badge(profile.id)
            } else {
                category.badge.to_string()
            };
            // The dots say how many units are stacked behind this family and
            // which one is showing — clicking cycles them, and nothing else
            // on the rail admits that.
            let at = if index == active_category {
                reverb_profiles::category_of(profile.id).map(|(_, v)| v).unwrap_or(0)
            } else {
                0
            };
            ShellItem::new(category.id, category.label)
                .with_badge(badge)
                .with_cycle(category.profiles.len(), at)
        })
        .collect();

    let profile_handle = param_handle(params.profile.as_ptr(), ctx.clone());
    let params_for_id = ui.params.clone();
    let params_for_form = ui.params.clone();
    let profile_count = reverb_profiles::PROFILES.len();
    let accent = design.accent.to_string();
    let accent_for_form = accent.clone();
    let accent_for_eq = accent.clone();

    // 0 = the space's face, 1 = Post EQ, 2 = Decay Rate EQ
    // (`fx.reverb.eq-display`).
    let mut eq_view = use_signal(|| 0usize);
    let eq_page = *eq_view.read();

    rsx! {
        ThemeProvider { state: theme,
            // Knobs capture the pointer through the shared drag provider —
            // without it a knob panics the moment it is drawn.
            DragProvider {
            PluginShell {
                title: "FTS Reverb".to_string(),
                subtitle: profile.voice.to_string(),
                brand: "RVB".to_string(),
                items,
                selected: active_category,
                accent: accent.clone(),
                on_select: move |category: usize| {
                    let index = reverb_profiles::rail_click_target(profile_index, category);
                    let normalized = if profile_count > 1 {
                        index as f32 / (profile_count - 1) as f32
                    } else {
                        0.0
                    };
                    profile_handle.begin_edit();
                    profile_handle.set_normalized(normalized);
                    profile_handle.end_edit();
                    params_for_id.store_profile_id(index);
                },
                rail_footer: rsx! {
                    // The two embedded EQs (`fx.reverb.eq-display`): one
                    // button cycles Space → Post EQ → Decay Rate EQ. Local
                    // UI state, never a plugin param.
                    RailButton {
                        testid: "eq-view-cycle".to_string(),
                        label: match eq_page {
                            1 => "PEQ".to_string(),
                            2 => "DEQ".to_string(),
                            _ => "EQ".to_string(),
                        },
                        title: match eq_page {
                            1 => "Post EQ (click for Decay Rate EQ)".to_string(),
                            2 => "Decay Rate EQ (click for the space)".to_string(),
                            _ => "EQ — Post EQ / Decay Rate EQ".to_string(),
                        },
                        active: eq_page != 0,
                        accent: accent_for_eq.clone(),
                        on_click: move |_| {
                            let next = (*eq_view.peek() + 1) % 3;
                            eq_view.set(next);
                        },
                    }
                    RailButton {
                        testid: "form-cycle".to_string(),
                        label: form.badge().to_string(),
                        title: format!("Editor size — {} (click to cycle)", form.label()),
                        active: form != fts_audio_ui::EditorForm::default(),
                        accent: accent_for_form.clone(),
                        on_click: move |_| {
                            let forms = fts_audio_ui::EDITOR_FORMS;
                            let index = forms.iter().position(|f| *f == form).unwrap_or(0);
                            params_for_form.store_editor_form(forms[(index + 1) % forms.len()]);
                        },
                    }
                },

                // Keyed list of one: swapping a face (or an EQ view) swaps a
                // whole subtree, and blitz's mutator wants a stable, keyed
                // node to land on.
                if eq_page != 0 {
                    for key in [format!("eq-{eq_page}")] {
                        crate::eq_view::ReverbEqView {
                            key: "{key}",
                            mode_is_decay: eq_page == 2,
                            frame,
                        }
                    }
                } else {
                    for id in [profile.id] {
                        SpaceFace {
                            key: "{id}",
                            profile_id: id.to_string(),
                            handles: handles.clone(),
                            frame,
                        }
                    }
                }
            }
            }
        }
    }
}

/// The rail badge for a space: short enough for a 48px rail.
pub fn profile_badge(profile_id: &str) -> String {
    match profile_id {
        "ir" => "IR",
        "hall_concert" => "CON",
        "hall_cathedral" => "CTH",
        "hall_arena" => "ARN",
        "plate_classic" => "PLT",
        "plate_224" => "224",
        "plate_progenitor" => "PRG",
        "room_medium" => "ROOM",
        "room_chamber" => "CHM",
        "room_studio" => "STU",
        "spring_classic" => "SPR",
        "spring_vintage" => "VTG",
        "cloud" => "CLD",
        "bloom" => "BLM",
        "swell" => "SWL",
        "velvet" => "VLV",
        "shimmer" => "SHM",
        "chorale" => "CHO",
        "magneto" => "MAG",
        "nonlinear" => "NL",
        "reflections" => "REF",
        "freeverb" => "FRV",
        _ => "RVB",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_has_a_badge_that_fits_the_rail() {
        for profile in reverb_profiles::PROFILES {
            let badge = profile_badge(profile.id);
            assert!(
                !badge.is_empty() && badge.len() <= 4,
                "{}'s badge {badge:?} does not fit a 48px rail",
                profile.id,
            );
            assert_ne!(badge, "RVB", "{} fell through to the default badge", profile.id);
        }
    }

    #[test]
    fn every_family_has_a_panel() {
        for category in reverb_profiles::CATEGORIES {
            for id in category.profiles {
                let design = crate::faces::design_for(id);
                assert_eq!(
                    design.family, category.id,
                    "{id} draws the {} panel",
                    design.family,
                );
                assert!(!design.knobs.is_empty(), "{} has no controls", design.family);
            }
        }
    }
}
