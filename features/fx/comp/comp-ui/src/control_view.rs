//! Comp editor — Dioxus GUI root component.
//!
//! The editor is the shared [`PluginShell`]: a left rail carrying the plugin
//! identity and the profile list, and one surface that gets everything else.
//! Every FTS plugin wears the same shell, so the profile switch is in the same
//! place in all of them — and the surface keeps the full window height, which
//! is the dimension a graph or a faceplate actually wants.
//!
//! The rail's footer holds the Basic/Advanced toggle, which belongs to the FTS
//! surface only — a hardware unit has the controls it has.
//!
//! The surface itself comes from [`crate::faces`]:
//!
//! - **Control** — the FTS surface: the compressor graph over a row of
//!   labelled sections, plus the GR and I/O meters.
//! - **LA-2A / 1176 / SSL Bus** — the unit's own front panel, drawn from
//!   [`crate::hardware`] and driven through [`crate::profile_handle`].
//!
//! Selecting a profile therefore swaps the whole UI rather than recolouring
//! it. The `profile` param persists, so the face survives a session reload.
//!
//! Reusable widgets (knobs, meters, drag provider) come from [`fts_ui_audio`];
//! theme + layout primitives from [`fts_ui`]; the section wrappers from
//! [`crate::sections`].

use audiocore_core::prelude::*;
use nice_plug::editor::dpi::LogicalSize;
use nice_plug::editor::ResizeHint;
use fts_ui::prelude::{ThemeMode, ThemeProvider, ThemeState, default_theme_preset};
use fts_ui_audio::prelude::*;

use crate::faces::{profile_id_for_index, Face};
use crate::param_adapter::param_handle;
use crate::params::{CompUiState, PROFILE_LABELS};
use crate::profile_view::{ProfileSkin, profile_skin};

/// Width the shell rail takes out of the window, in CSS px.
///
/// Hardware faces need it as a number, not as layout: a faceplate is fitted to
/// the space *beside* the rail, so the fit has to know how much of the window
/// the rail already took. There is no header any more, so the faces get the
/// window's full height.
pub const RAIL_W: f64 = fts_ui_audio::shell::RAIL_W;

/// Editor size the plugin shell requests from the host on open.
///
/// This is the *starting* size, not a ceiling — the editor opts into host
/// resizing through [`resize_hint`]. It lives here rather than in
/// `comp-plugin` because the surface is what constrains it: blitz does not
/// overflow-scroll a height-constrained container, so a section that does not
/// fit collapses to 0×0 and becomes unreachable rather than being clipped.
/// `advanced_page_fits_the_plugin_editor_size` guards the default.
pub const EDITOR_W: u32 = 980;
pub const EDITOR_H: u32 = 660;

/// Smallest size the surface still works at.
///
/// Below this the graph and the section row stop fitting, and — per the note on
/// [`EDITOR_W`] — not fitting means collapsing, not clipping. So this floor is
/// what keeps a host-driven resize from making controls unreachable, and it is
/// enforced rather than advisory: `DioxusEditorHandle::set_size` refuses
/// anything smaller.
pub const MIN_EDITOR_W: f32 = 720.0;
/// Low enough for the rack faces, which are 4:1 drawings and ask the host for
/// roughly a third of the FTS surface's height (see
/// [`crate::faces::preferred_editor_size`]). The FTS surface survives it
/// because its controls float over the graph rather than sitting under it —
/// they overlap it when the window is squeezed, but nothing collapses.
pub const MIN_EDITOR_H: f32 = 300.0;

/// Largest size the editor accepts.
///
/// Not cosmetic: with no maximum, `ResizeHint::adjust_size` rubber-stamps
/// whatever a host proposes, and hosts do propose absurd sizes — REAPER opened
/// this editor at 3371x1017 (full screen width) because it asked "is this size
/// OK?" and an unbounded hint said yes. Anything past roughly twice the design
/// size is stretched chrome around a fixed control surface, so the cap is
/// generous but real.
pub const MAX_EDITOR_W: f32 = 1960.0;
pub const MAX_EDITOR_H: f32 = 1320.0;

/// How the host may resize this editor.
///
/// Freely resizable on both axes above [`MIN_EDITOR_W`] x [`MIN_EDITOR_H`],
/// with no aspect-ratio lock — the layout is a graph over a row of sections and
/// both are happy to grow in either direction.
pub fn resize_hint() -> ResizeHint {
    ResizeHint::RESIZABLE.with_min_max_logical_size(
        Some(LogicalSize::new(MIN_EDITOR_W, MIN_EDITOR_H)),
        Some(LogicalSize::new(MAX_EDITOR_W, MAX_EDITOR_H)),
    )
}

/// Root editor component.
///
/// Wraps the comp shell in `fts_ui::ThemeProvider` so themed widgets pick up
/// the active preset. The plugin embedded path and any standalone path both
/// go through here.
#[component]
pub fn App() -> Element {
    let theme_state = use_signal(|| ThemeState::new(default_theme_preset(), ThemeMode::Dark));
    rsx! {
        document::Style { {nice_plug_dioxus::TAILWIND_CSS} }
        // comp-ui's own compiled utilities + theme tokens — the framework CSS
        // above only covers nice-plug-dioxus's widgets, so without this every
        // layout-critical class (flex-1, min-h-0, …) is undefined and the
        // layout collapses in DAW hosts. `just tailwind-comp`.
        document::Style { {include_str!("../assets/tailwind.css")} }
        ThemeProvider { state: theme_state, AppShell {} }
    }
}

/// Inner shell component — runs after the ThemeProvider context is in scope
/// so themed primitives can resolve their tokens.
#[component]
fn AppShell() -> Element {
    let _theme = use_init_theme();

    let shared = use_context::<SharedState>();
    let ui = shared.get::<CompUiState>().expect("CompUiState missing");
    let ctx = use_param_context();
    let params = &ui.params;

    // Redraw tick. AppShell owns the read-side of every Param atomic and the
    // meter atomics, so it must re-render for fresh values to reach the DOM.
    // Spawn the OS thread exactly once via use_hook and have it call
    // schedule_update on this scope — same driver as eq-ui's control view.
    let mut app_tick: Signal<u64> = use_signal(|| 0);
    use_hook(|| {
        let updater = dioxus_core::schedule_update();
        std::thread::spawn(move || {
            loop {
                // ~30 Hz — plenty for meter ballistics; keeps the headless
                // event loop unclogged.
                std::thread::sleep(std::time::Duration::from_millis(33));
                updater();
            }
        });
    });
    app_tick += 1;
    // Frame counter sneaks into a `data-frame` attribute on the root — the
    // DOM mutation forces blitz to consider the document dirty every render,
    // which forces a window.request_redraw. Without it idle redraws collapse
    // and the meters freeze.
    let frame_counter = *app_tick.read();

    let mut advanced = use_signal(|| false);

    // Profile → editor size. Faces are different shapes, so a face swap is a
    // resize request to the host (see `faces::preferred_editor_size`). Tracked
    // through a plain Cell rather than an effect because the profile lives in
    // a plugin param, not a signal: this shell re-renders on the redraw tick
    // anyway, so comparing here also catches the host automating the param.
    let last_profile: std::rc::Rc<std::cell::Cell<Option<usize>>> =
        use_hook(|| std::rc::Rc::new(std::cell::Cell::new(None)));

    // The face comes from the *resolved* profile: the persisted id when the
    // session has one, the index otherwise. See `CompParams::profile_id`.
    let profile_idx = params.resolved_profile_index();
    let profile_id = profile_id_for_index(profile_idx);
    let skin = profile_skin(profile_id);
    let is_control_face = profile_id == "control";

    if last_profile.get() != Some(profile_idx) {
        last_profile.set(Some(profile_idx));
        // A session restored from its id can land with the index parameter
        // still on whatever number that id used to be; put it back in line so
        // the host reads the same face the editor shows.
        if params.profile.value().max(0) as usize != profile_idx {
            let ptr = params.profile.as_ptr();
            let count = PROFILE_LABELS.len();
            let normalized = if count > 1 {
                profile_idx as f32 / (count - 1) as f32
            } else {
                0.0
            };
            ctx.begin_set_raw(ptr);
            ctx.set_normalized_raw(ptr, normalized);
            ctx.end_set_raw(ptr);
        }
        // Absent when the editor is embedded without a nice-plug window
        // (headless tests): nothing to resize, so nothing to do.
        if let Some(state) = try_consume_context::<std::sync::Arc<nice_plug_dioxus::DioxusState>>() {
            let (w, h) = crate::faces::preferred_editor_size(profile_idx);
            if state.size() != (w, h) {
                state.request_resize(w, h);
            }
        }
    }

    let base_css = "*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; } \
         html, body { width: 100%; height: 100%; overflow: hidden; \
         background: var(--background); color: var(--foreground); \
         font-family: var(--font-sans, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, sans-serif); \
         font-size: 13px; }";
    let root_style = "width:100vw; height:100vh; \
         display:flex; flex-direction:column; \
         color:var(--foreground); \
         background:var(--background); \
         font-family:var(--font-sans, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, sans-serif); \
         font-size:13px; \
         user-select:none; position:relative;";

    let is_advanced = *advanced.read();

    // The rail writes the profile param through the same host-gesture path as
    // any other control, so switching faces is automatable and undoable like
    // everything else.
    let profile_handle = param_handle(params.profile.as_ptr(), ctx.clone());
    let params_for_id = ui.params.clone();
    let profile_count = PROFILE_LABELS.len();
    // One rail entry per compressor family, badged with the active unit.
    let items = crate::faces::rail_items(profile_idx);
    let active_category = comp_profiles::category_of(profile_id)
        .map(|(c, _)| c)
        .unwrap_or(0);

    rsx! {
        document::Style { {base_css} }

        DragProvider {
            div {
                style: format!("{root_style} overflow:hidden;"),
                "data-frame": "{frame_counter}",

                PluginShell {
                    title: "FTS Comp".to_string(),
                    subtitle: "Stereo Compressor".to_string(),
                    brand: "CMP".to_string(),
                    items,
                    selected: active_category,
                    accent: skin.accent.to_string(),
                    on_select: move |category: usize| {
                        // Clicking the active family cycles the units inside
                        // it; clicking another lands on its first unit.
                        let index = crate::faces::rail_click_target(profile_idx, category);
                        let normalized = if profile_count > 1 {
                            index as f32 / (profile_count - 1) as f32
                        } else {
                            0.0
                        };
                        profile_handle.begin_edit();
                        profile_handle.set_normalized(normalized);
                        profile_handle.end_edit();
                        // What the session restores from.
                        params_for_id.store_profile_id(index);
                    },
                    rail_footer: rsx! {
                        // Basic/Advanced disclosure. Local UI state — never a
                        // plugin param, so switching pages does not show up as
                        // an automatable change or dirty the host's project.
                        if is_control_face {
                            RailButton {
                                testid: "advanced-toggle".to_string(),
                                label: "ADV".to_string(),
                                title: if is_advanced {
                                    "Advanced controls (on)".to_string()
                                } else {
                                    "Advanced controls".to_string()
                                },
                                active: is_advanced,
                                accent: skin.accent.to_string(),
                                on_click: move |_| advanced.toggle(),
                            }
                        }
                    },

                    Face { profile_index: profile_idx, advanced: is_advanced, frame: frame_counter }
                }
            }
        }
    }
}

/// Re-exported so callers (and tests) can resolve a profile index to its skin
/// the same way [`AppShell`] does.
pub fn skin_for_profile_index(index: usize) -> ProfileSkin {
    profile_skin(profile_id_for_index(index))
}
