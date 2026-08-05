//! Comp editor — Dioxus GUI root component.
//!
//! The editor is a header over a *face*. The header is the only part every
//! profile shares: plugin identity, the hardware-profile picker, and the
//! Basic/Advanced toggle (which belongs to the FTS surface and is hidden on
//! the hardware faces, where there is no such thing as an advanced page).
//!
//! Everything below the header comes from [`crate::faces`]:
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

use crate::faces::{Face, PROFILE_IDS};
use crate::param_adapter::param_handle;
use crate::params::{CompUiState, PROFILE_LABELS};
use crate::profile_view::{ProfileSkin, profile_skin};
use crate::sections::ParamSelector;

/// Height of the header strip, in CSS px.
///
/// Hardware faces need it as a number, not as layout: a faceplate is fitted to
/// the space *below* the header, so the fit has to know how much of the window
/// the header already took.
pub const HEADER_H: f64 = 52.0;

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
pub const MIN_EDITOR_H: f32 = 560.0;

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

    // Advanced disclosure. Local UI state — deliberately not a plugin param,
    // so switching views never shows up as an automatable change or dirties
    // the host's project state.
    let mut advanced = use_signal(|| false);

    // The profile param picks the face; the header tints from its skin.
    let profile_idx = params.profile.value().max(0) as usize;
    let profile_id = PROFILE_IDS.get(profile_idx).copied().unwrap_or("control");
    let skin = profile_skin(profile_id);
    let is_control_face = profile_id == "control";

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

    rsx! {
        document::Style { {base_css} }

        DragProvider {
            div {
                style: format!("{root_style} overflow:hidden;"),
                "data-frame": "{frame_counter}",

                // ── Header ───────────────────────────────────────────
                div {
                    class: "flex justify-between items-center px-4 border-b border-border bg-card/50",
                    // Pinned rather than padded: the hardware faces fit
                    // themselves into what is left, and they need that number.
                    style: "height:{HEADER_H}px; flex:none;",
                    div { class: "flex items-baseline gap-3 shrink-0",
                        div {
                            class: "text-base font-bold tracking-wide text-foreground",
                            "FTS Comp"
                        }
                        div {
                            class: "text-xs text-muted-foreground uppercase tracking-wider",
                            "Stereo Compressor"
                        }
                    }

                    div {
                        style: "display:flex; align-items:center; gap:14px;",

                        // Hardware profile picker — this is the face switch.
                        ParamSelector {
                            handle: param_handle(params.profile.as_ptr(), ctx.clone()),
                            testid: "profile".to_string(),
                            label: "Profile".to_string(),
                            options: PROFILE_LABELS.iter().map(|s| s.to_string()).collect(),
                            skin,
                        }

                        // Basic/Advanced disclosure. Only the FTS surface has
                        // pages; a faceplate has the controls the unit has.
                        if is_control_face {
                            div {
                                "data-testid": "advanced-toggle",
                                style: format!(
                                    "cursor:pointer; padding:5px 12px; border-radius:6px; \
                                     font-size:11px; font-weight:600; letter-spacing:0.06em; \
                                     text-transform:uppercase; border:1px solid {}; color:{}; background:{};",
                                    skin.border,
                                    if is_advanced { "#fff" } else { skin.text },
                                    if is_advanced { skin.accent } else { "transparent" },
                                ),
                                onclick: move |_| advanced.toggle(),
                                if is_advanced { "Advanced" } else { "Basic" }
                            }
                        }
                    }
                }

                // ── The face ─────────────────────────────────────────
                Face { profile_index: profile_idx, advanced: is_advanced, frame: frame_counter }
            }
        }
    }
}

/// Re-exported so callers (and tests) can resolve a profile index to its skin
/// the same way [`AppShell`] does.
pub fn skin_for_profile_index(index: usize) -> ProfileSkin {
    profile_skin(PROFILE_IDS.get(index).copied().unwrap_or("control"))
}
