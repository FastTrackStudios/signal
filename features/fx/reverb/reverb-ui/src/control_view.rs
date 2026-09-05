//! The reverb editor: a rail of seven families, and the panel of whichever
//! one is selected.
//!
//! Same shell as the compressor, deliberately — the two plugins are the same
//! instrument as far as your hands are concerned. Clicking the family you are
//! already on cycles the spaces inside it.

use std::collections::HashMap;
use std::sync::Arc;

use architect_ui::prelude::{ThemeMode, ThemeProvider, ThemeState, default_theme_preset};
use dioxus::prelude::*;
use fts_audio_ui::prelude::*;
use fts_audio_ui::shell::{PluginShell, RailButton, ShellItem};
use nice_plug::editor::ResizeHint;
use nice_plug::editor::dpi::LogicalSize;
use nice_plug::prelude::Param;
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

#[must_use]
pub fn min_editor_size() -> (f32, f32) {
    let ((w, h), _) = bounds();
    (w as f32, h as f32)
}

#[must_use]
pub fn max_editor_size() -> (f32, f32) {
    let (_, (w, h)) = bounds();
    ((w as f32 * 2.0).max(1960.0), (h as f32 * 1.4).max(1320.0))
}

/// Freely resizable between the extremes of the size presets — a bound tighter
/// than a preset the rail offers is a button that does nothing.
#[must_use]
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
#[must_use]
pub fn editor_size_for(_profile_index: usize, form: fts_audio_ui::EditorForm) -> (u32, u32) {
    form.editor_size(RAIL_W, (EDITOR_W, EDITOR_H))
}

/// Width of the EQ sidecar (`fx.reverb.eq-display` — the Post / Decay Rate
/// EQ pair EXTENDS the window rightward, stacked in a column; the space's
/// panel keeps its size).
pub const EQ_SIDECAR_W: u32 = 560;

/// The preset browser opens as its own narrower column, on the same principle
/// as the EQ pair: the panel keeps its box and the window grows rightward.
pub const PRESET_SIDECAR_W: u32 = 340;

/// The editor size with the EQ sidecar open, capped at the resize bounds.
#[must_use]
pub fn editor_size_with_eq(
    profile_index: usize,
    form: fts_audio_ui::EditorForm,
    eq_open: bool,
) -> (u32, u32) {
    editor_size_with_sidecars(profile_index, form, eq_open, false)
}

/// The editor size with any combination of sidecars open, capped at the
/// resize bounds.
#[must_use]
pub fn editor_size_with_sidecars(
    profile_index: usize,
    form: fts_audio_ui::EditorForm,
    eq_open: bool,
    preset_open: bool,
) -> (u32, u32) {
    let (w, h) = editor_size_for(profile_index, form);
    let extra =
        if eq_open { EQ_SIDECAR_W } else { 0 } + if preset_open { PRESET_SIDECAR_W } else { 0 };
    ((w + extra).min(max_editor_size().0 as u32), h)
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

    // The layer's sidecar: Post EQ + Decay Rate EQ side by side, in a strip
    // UNDER the panel (`fx.reverb.eq-display`). Local UI state.
    let mut eq_view = use_signal(|| false);
    let eq_open = *eq_view.read();

    // The preset browser, on the same footing: local UI state, its own column.
    let mut preset_view = use_signal(|| false);
    let preset_open = *preset_view.read();

    // One browser behind both preset surfaces — the strip at the top and the
    // sidecar show the same selection, because they are the same thing seen
    // two ways. Scanned once per mount: walking a directory is not something
    // to do sixty times a second.
    let mut preset_browser = use_signal(preset_browser::PresetBrowser::default);
    let preset_note = use_signal(String::new);
    let mut presets_loaded = use_signal(|| false);
    if !*presets_loaded.read() {
        presets_loaded.set(true);
        let (library, note) = crate::preset_view::load_library();
        preset_browser.set(library);
        let mut n = preset_note;
        n.set(note);
    }

    // Profile / form / EQ-strip change → ask the host to resize. A plain
    // Cell rather than an effect: the profile lives in a plugin param, not a
    // signal, so comparing here also catches the host automating it.
    #[expect(clippy::type_complexity)]
    let last: std::rc::Rc<
        std::cell::Cell<Option<(usize, fts_audio_ui::EditorForm, bool, bool)>>,
    > = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(None)));
    if last.get() != Some((profile_index, form, eq_open, preset_open)) {
        last.set(Some((profile_index, form, eq_open, preset_open)));
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
            let (w, h) = editor_size_with_sidecars(profile_index, form, eq_open, preset_open);
            if state.size() != (w, h) {
                state.request_resize(w, h);
            }
        }
    }

    // The face's box: the window minus the EQ sidecar when it is open.
    let (win_w, win_h) = fts_audio_ui::hardware::panel::window_logical_size().unwrap_or({
        let (w, h) = editor_size_with_sidecars(profile_index, form, eq_open, preset_open);
        (f64::from(w), f64::from(h))
    });
    let sidecar_w = if eq_open {
        f64::from(EQ_SIDECAR_W).min(win_w * 0.45)
    } else {
        0.0
    };
    let preset_w = if preset_open {
        f64::from(PRESET_SIDECAR_W).min(win_w * 0.35)
    } else {
        0.0
    };
    let face_w = (win_w - sidecar_w - preset_w).max(240.0);

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
    let active_category = reverb_profiles::category_of(profile.id).map_or(0, |(c, _)| c);
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
                reverb_profiles::category_of(profile.id).map_or(0, |(_, v)| v)
            } else {
                0
            };
            ShellItem::new(category.id, category.label)
                .with_badge(badge)
                .with_cycle(category.profiles.len(), at)
        })
        .collect();

    let profile_handle = param_handle(params.profile.as_ptr(), ctx);
    let params_for_id = ui.params.clone();
    let params_for_form = ui.params.clone();
    let profile_count = reverb_profiles::PROFILES.len();
    let accent = design.accent.to_string();
    let accent_for_form = accent.clone();
    let accent_for_eq = accent.clone();
    let accent_for_presets = accent.clone();

    rsx! {
        // The embedded EQ surface's DOM parts (band popup, menus) style
        // themselves with eq-ui's compiled utilities — without these the
        // popup's layout classes are undefined and collapse
        // (`fx.embed-eq.one-surface`).
        document::Style { {nice_plug_dioxus::TAILWIND_CSS} }
        document::Style { {eq_ui::TAILWIND_CSS} }
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
                accent: accent,
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
                    // The layer's sidecar toggle (`fx.reverb.eq-display`):
                    // Post EQ + Decay Rate EQ, together, under the panel.
                    // Local UI state, never a plugin param.
                    RailButton {
                        testid: "eq-view-cycle".to_string(),
                        label: "EQ".to_string(),
                        title: "Post EQ + Decay Rate EQ".to_string(),
                        active: eq_open,
                        accent: accent_for_eq,
                        on_click: move |()| eq_view.toggle(),
                    }
                    RailButton {
                        testid: "preset-view-cycle".to_string(),
                        label: "PRE".to_string(),
                        title: "Preset browser".to_string(),
                        active: preset_open,
                        accent: accent_for_presets,
                        on_click: move |()| preset_view.toggle(),
                    }
                    RailButton {
                        testid: "form-cycle".to_string(),
                        label: form.badge().to_string(),
                        title: format!("Editor size — {} (click to cycle)", form.label()),
                        active: form != fts_audio_ui::EditorForm::default(),
                        accent: accent_for_form,
                        on_click: move |()| {
                            let forms = fts_audio_ui::EDITOR_FORMS;
                            let index = forms.iter().position(|f| *f == form).unwrap_or(0);
                            params_for_form.store_editor_form(forms[(index + 1) % forms.len()]);
                        },
                    }
                },

                // The panel keeps its box; the Post + Decay pair opens as a
                // SIDECAR column to its right (`fx.reverb.eq-display`) — the
                // window grew rightward to make room. Both keyed: swapping a
                // face swaps a whole subtree, and blitz's mutator wants a
                // stable, keyed node to land on.
                div {
                    style: "position:absolute; inset:0; display:flex; flex-direction:column; \
                            overflow:hidden;",

                    // Always visible: what is loaded, and how to move off it.
                    // The side rail selects the space; this selects the preset.
                    preset_browser_ui::PresetBar {
                        browser: preset_browser,
                        browsing: preset_open,
                        ink: design.ink.to_string(),
                        accent: design.accent.to_string(),
                        on_browse: move |()| preset_view.toggle(),
                        on_apply: {
                            let handles = handles.clone();
                            move |p: Vec<(String, f64)>| {
                                crate::preset_view::apply(&p, &handles, preset_note);
                            }
                        },
                    }

                    div {
                    style: "flex:1; display:flex; overflow:hidden; min-height:0;",
                    div {
                        style: format!(
                            "position:relative; flex:none; width:{face_w}px; overflow:hidden;"
                        ),
                        for id in [profile.id] {
                            FaceInBox {
                                key: "{id}",
                                profile_id: id.to_string(),
                                handles: handles.clone(),
                                frame,
                                box_w: face_w,
                                box_h: win_h,
                            }
                        }
                    }
                    if eq_open {
                        div {
                            style: format!(
                                "position:relative; flex:none; width:{sidecar_w}px; \
                                 overflow:hidden; \
                                 border-left:1px solid var(--border, rgba(148,163,184,0.3)); \
                                 background:var(--background);"
                            ),
                            for key in ["eq-sidecar"] {
                                crate::eq_view::ReverbEqSidecar {
                                    key: "{key}",
                                    frame,
                                }
                            }
                        }
                    }
                    if preset_open {
                        div {
                            style: format!(
                                "position:relative; flex:none; width:{preset_w}px; \
                                 overflow:hidden; \
                                 border-left:1px solid var(--border, rgba(148,163,184,0.3)); \
                                 background:var(--background);"
                            ),
                            for key in ["preset-sidecar"] {
                                crate::preset_view::ReverbPresetSidecar {
                                    key: "{key}",
                                    browser: preset_browser,
                                    note: preset_note,
                                    handles: handles.clone(),
                                    ink: design.ink.to_string(),
                                    accent: design.accent.to_string(),
                                }
                            }
                        }
                    }
                    }
                }
            }
            }
        }
    }
}

/// [`SpaceFace`] wrapped with a [`fts_audio_ui::hardware::panel::PanelBox`]
/// of its row, so the panel scales to the space above the EQ strip.
#[component]
fn FaceInBox(
    profile_id: String,
    handles: HashMap<String, ParamHandle>,
    frame: u64,
    box_w: f64,
    box_h: f64,
) -> Element {
    use_context_provider(|| fts_audio_ui::hardware::panel::PanelBox(box_w, box_h));
    rsx! {
        SpaceFace { profile_id, handles, frame }
    }
}

/// The rail badge for a space: short enough for a 48px rail.
#[must_use]
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
        "random" => "RND",
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
            assert_ne!(
                badge, "RVB",
                "{} fell through to the default badge",
                profile.id
            );
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
                assert!(
                    !design.knobs.is_empty(),
                    "{} has no controls",
                    design.family
                );
            }
        }
    }
}
