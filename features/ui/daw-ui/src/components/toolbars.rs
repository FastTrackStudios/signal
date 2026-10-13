//! Icon rails around the arrangement view — the session workflow-mode
//! switcher (`ModeDropdown`, left of the header row) and action bars
//! (top/right). The mode's OWN top-toolbar content (what buttons show
//! for Organize vs. Edit vs. Mix, ...) is entirely caller-supplied — see
//! `ArrangementView`'s `top_actions` prop, filled in by `apps/desktop`.
//!
//! `daw-ui` doesn't know what a "session mode" is — `Organize`/`Write`/
//! `Produce`/.../`Master` are `session::modes::Mode`, a real domain type
//! that lives in the `session` repo (which depends on `daw`, not the
//! other way around — `daw-ui` taking a dependency on `session` would be
//! a backwards/cyclic edge). So `ModeDropdown`/`ModeIndicator` take a
//! plain `Vec<ModeOption>` the caller builds from whatever its own mode
//! vocabulary is; `apps/desktop` is the one that actually maps
//! `session::modes::Mode::ALL` into these.
//!
//! `apps/desktop` owns mode STATE locally rather than calling
//! `session::modes::set_mode` — that function unconditionally calls into
//! `daw::reaper::Reaper` (`HighReaper::get()`), which panics
//! (`"Reaper::load().setup() must be called before Reaper::get()"`) when
//! there's no real REAPER process, which the standalone
//! `daw-standalone`-backed app never has. So this is purely a display/
//! selection switcher today — nothing (yet) reads the selection to
//! change window layout the way REAPER's screenset-backed modes do.
//!
//! The keybind PROFILE (`ProfilePicker`) is a separate axis from the
//! session mode — it picks which physical keys do what (FastTrackStudio/
//! Logic/Ableton/Pro Tools/REAPER bindings), not which workflow you're
//! in. `ArrangementView` is what actually loads the active profile's
//! real `transport.styx`/`navigation.styx` data and dispatches matched
//! keys (space, h/j/k/l, ...) — see its module doc.

use crate::prelude::*;

/// One mode a `LeftToolbar`/`ModeIndicator` can show, entirely owned by
/// the caller — see the module doc for why `daw-ui` doesn't define the
/// real mode list itself.
#[derive(Clone, PartialEq)]
pub struct ModeOption {
    /// Stable id (`session::modes::Mode::slug()`, e.g. `"organize"`).
    pub slug: String,
    pub label: String,
    /// A single glyph — `daw-ui` has no icon-font dependency, so this is
    /// plain text/emoji, not an SVG set.
    pub glyph: String,
}

/// One button in a `TopToolbar`/`RightToolbar` action list.
#[derive(Clone, PartialEq)]
pub struct ToolbarAction {
    /// Stable id — the same id a keybind's `InputCommand::Action` would
    /// carry, once the input engine is wired to more than the toolbar's
    /// own actions (see the module doc on mouse-modifier panning for why
    /// that's not yet the case).
    /// Stable id, owned rather than `&'static str` — a per-time-signature
    /// button (`"timesig-4-4"`, ...) is built at runtime from data, not
    /// known at compile time the way the zoom/mode buttons are.
    pub id: String,
    pub label: String,
    pub glyph: String,
    pub active: bool,
    /// Pill background, CSS color string — REAPER's own section-marker
    /// colors for the Organize toolbar's section/marker buttons
    /// (`session::keyflow::actions::section_type_color` /
    /// `MarkerKind::css_color`), `None` for the ordinary neutral button
    /// chrome (zoom, mixer switch, record toggle, ...).
    pub color: Option<String>,
    /// Receives the raw click so a caller can branch on modifiers — the
    /// time-signature buttons insert one bar with Shift held, the whole
    /// signature change otherwise (mirrors `fts-extensions`' REAPER
    /// action of the same shape).
    pub on_click: EventHandler<MouseEvent>,
}

fn rail_button_style(active: bool, enabled: bool) -> String {
    let t = daw_theme::Theme::default();
    let (bg, fg, border) = if !enabled {
        (
            t.chrome.surface.css(),
            t.chrome.text_faint.css(),
            t.chrome.surface_sunken.shade(-0.1).css(),
        )
    } else if active {
        (
            t.chrome.accent.shade(-0.5).css(),
            t.chrome.text.css(),
            t.chrome.accent.css(),
        )
    } else {
        (
            t.chrome.surface_raised.css(),
            t.chrome.text_dim.css(),
            t.chrome.surface_sunken.shade(-0.25).css(),
        )
    };
    format!(
        "display:flex; flex-direction:column; align-items:center; justify-content:center; \
         gap:2px; width:100%; padding:8px 2px; background:{bg}; color:{fg}; \
         border:1px solid {border}; border-radius:6px; cursor:{cursor}; font-size:9px; \
         font-family:Fira Sans, DejaVu Sans, sans-serif;",
        cursor = if enabled { "pointer" } else { "default" },
    )
}

/// The session-mode switcher — one column down the left edge, one
/// button per caller-supplied [`ModeOption`] (see the module doc).
#[component]
pub fn LeftToolbar(
    modes: Vec<ModeOption>,
    active_slug: String,
    on_mode_change: EventHandler<String>,
) -> Element {
    let t = daw_theme::Theme::default();
    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:4px; width:44px; \
                    flex:0 0 auto; padding:4px; background:{t.chrome.surface.css()}; \
                    border-right:1px solid {t.chrome.surface_sunken.shade(-0.1).css()};",
            for m in modes {
                {
                    let active = m.slug == active_slug;
                    let slug = m.slug.clone();
                    rsx! {
                        button {
                            key: "{m.slug}",
                            style: rail_button_style(active, true),
                            title: "{m.label}",
                            onclick: move |_| on_mode_change.call(slug.clone()),
                            span { style: "font-size:14px;", "{m.glyph}" }
                            span { "{m.label}" }
                        }
                    }
                }
            }
        }
    }
}

/// The session-mode switcher as a dropdown — shows the active mode,
/// click to see the others and switch. Same interaction shape as
/// `ProfilePicker`; a dropdown scales far better than a button rail once
/// there are ten modes (`Mode::ALL`) instead of two or three.
#[component]
pub fn ModeDropdown(
    modes: Vec<ModeOption>,
    active_slug: String,
    on_mode_change: EventHandler<String>,
) -> Element {
    let t = daw_theme::Theme::default();
    let mut open = use_signal(|| false);
    let active = modes.iter().find(|m| m.slug == active_slug).cloned();

    rsx! {
        div { style: "position:relative;",
            button {
                style: "display:flex; align-items:center; gap:6px; padding:4px 10px; \
                        background:{t.chrome.surface_raised.css()}; color:{t.chrome.text.css()}; \
                        border:1px solid {t.chrome.accent.css()}; border-radius:6px; \
                        cursor:pointer; font-size:11px; font-weight:600; \
                        font-family:Fira Sans, DejaVu Sans, sans-serif;",
                onclick: move |_| open.toggle(),
                if let Some(active) = &active {
                    span { "{active.glyph}" }
                    span { "{active.label}" }
                } else {
                    span { "Mode" }
                }
                span { style: "color:{t.chrome.text_faint.css()};", "\u{25be}" }
            }
            if open() {
                div {
                    style: "position:fixed; inset:0; z-index:20;",
                    onclick: move |_| open.set(false),
                }
                div {
                    style: "position:absolute; top:100%; left:0; margin-top:4px; z-index:30; \
                            min-width:160px; background:{t.chrome.surface_raised.css()}; \
                            border:1px solid {t.chrome.surface_sunken.shade(-0.25).css()}; \
                            border-radius:8px; box-shadow:0 10px 30px rgba(0,0,0,0.5); \
                            padding:6px; display:flex; flex-direction:column; gap:2px;",
                    for m in modes {
                        {
                            let selected = m.slug == active_slug;
                            let slug = m.slug.clone();
                            let row_bg = if selected {
                                t.chrome.accent.shade(-0.6).css()
                            } else {
                                "transparent".to_string()
                            };
                            rsx! {
                                button {
                                    key: "{m.slug}",
                                    style: "display:flex; align-items:center; gap:8px; \
                                            padding:6px 8px; width:100%; text-align:left; \
                                            background:{row_bg}; color:{t.chrome.text.css()}; \
                                            border:none; border-radius:6px; cursor:pointer; \
                                            font-size:12px; \
                                            font-family:Fira Sans, DejaVu Sans, sans-serif;",
                                    onclick: move |_| {
                                        on_mode_change.call(slug.clone());
                                        open.set(false);
                                    },
                                    span { "{m.glyph}" }
                                    span { "{m.label}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A horizontal action bar — REAPER's per-mode toolbar near the
/// transport. Actions are entirely caller-supplied; this just lays them
/// out and applies the shared rail-button look. No background/border of
/// its own — callers compose it into a header row that already has one
/// (see `ArrangementView`), so a second border here would double up.
///
/// Independently horizontally scrollable (`overflow-x:scroll`) with
/// `min-width:0` so a mode with many buttons (Organize's section/marker/
/// time-signature set) scrolls within its own strip instead of forcing
/// the whole header row — and the window — wider. This is a real
/// scroll like the arrange view's, not virtualized or wrapped.
#[component]
pub fn TopToolbar(actions: Vec<ToolbarAction>) -> Element {
    let t = daw_theme::Theme::default();
    rsx! {
        div {
            style: "display:flex; flex-direction:row; gap:4px; height:36px; \
                    flex:1 1 0; min-width:0; overflow-x:scroll; overflow-y:hidden; \
                    padding:4px 6px; align-items:center;",
            for action in actions {
                {
                    let bg = match (&action.color, action.active) {
                        (Some(color), _) => color.clone(),
                        (None, true) => t.chrome.accent.shade(-0.5).css(),
                        (None, false) => t.chrome.surface_raised.css(),
                    };
                    let fg = if action.color.is_some() {
                        "#ffffff".to_string()
                    } else {
                        t.chrome.text_dim.css()
                    };
                    rsx! {
                        button {
                            key: "{action.id}",
                            style: "display:flex; align-items:center; gap:6px; padding:5px 10px; \
                                    background:{bg}; color:{fg}; \
                                    border:1px solid {t.chrome.surface_sunken.shade(-0.25).css()}; \
                                    border-radius:6px; cursor:pointer; font-size:11px; font-weight:600; \
                                    font-family:Fira Sans, DejaVu Sans, sans-serif;",
                            onclick: move |evt| action.on_click.call(evt),
                            if !action.glyph.is_empty() {
                                span { "{action.glyph}" }
                            }
                            span { "{action.label}" }
                        }
                    }
                }
            }
        }
    }
}

/// A vertical action rail on the right edge — the same shape as
/// `LeftToolbar`, for actions rather than mode selection.
#[component]
pub fn RightToolbar(actions: Vec<ToolbarAction>) -> Element {
    let t = daw_theme::Theme::default();
    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:4px; width:40px; \
                    flex:0 0 auto; padding:4px; background:{t.chrome.surface.css()}; \
                    border-left:1px solid {t.chrome.surface_sunken.shade(-0.1).css()};",
            for action in actions {
                button {
                    key: "{action.id}",
                    style: rail_button_style(action.active, true),
                    title: "{action.label}",
                    onclick: move |evt| action.on_click.call(evt),
                    span { style: "font-size:14px;", "{action.glyph}" }
                    span { "{action.label}" }
                }
            }
        }
    }
}

/// A plain, non-interactive badge showing the current [`ModeOption`] —
/// `LeftToolbar` is how you CHANGE mode, this is how you can always SEE
/// which one you're in without hunting for the highlighted rail button.
#[component]
pub fn ModeIndicator(mode: ModeOption) -> Element {
    let t = daw_theme::Theme::default();
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:6px; padding:4px 10px; \
                    background:{t.chrome.surface_raised.css()}; color:{t.chrome.text.css()}; \
                    border:1px solid {t.chrome.accent.css()}; border-radius:6px; \
                    font-size:11px; font-weight:600; \
                    font-family:Fira Sans, DejaVu Sans, sans-serif;",
            span { "{mode.glyph}" }
            span { "{mode.label}" }
        }
    }
}

/// A keybind profile a user can pick — the same `profile.styx`
/// (name/description) `reaper-input` loads, just named without pulling
/// `input-config-proto`'s type into this module's public API.
#[cfg(feature = "web")]
#[derive(Clone, PartialEq)]
pub struct KeybindProfile {
    /// The subdirectory name — what actually gets passed back to
    /// `input_keybinds::load_profile_keymap`.
    pub slug: String,
    pub name: String,
    pub description: String,
}

#[cfg(feature = "web")]
impl KeybindProfile {
    /// List every profile under `root` (one subdirectory per profile,
    /// each with its own `profile.styx`) — see
    /// `input_keybinds::list_profiles`, which this wraps.
    pub fn list(root: &std::path::Path) -> Vec<Self> {
        input_keybinds::list_profiles(root)
            .into_iter()
            .map(|(slug, cfg)| Self {
                slug,
                name: cfg.name,
                description: cfg.description,
            })
            .collect()
    }
}

/// A dropdown badge for the active keybind profile — "FastTrackStudio",
/// "Logic", "Ableton", "Pro Tools", "REAPER" — the same idea as an
/// organization/workspace switcher: shows what you're in, click to see
/// the others and switch.
#[cfg(feature = "web")]
#[component]
pub fn ProfilePicker(
    profiles: Vec<KeybindProfile>,
    active_slug: String,
    on_select: EventHandler<String>,
) -> Element {
    let t = daw_theme::Theme::default();
    let mut open = use_signal(|| false);
    let active_name = profiles
        .iter()
        .find(|p| p.slug == active_slug)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| active_slug.clone());

    rsx! {
        div { style: "position:relative;",
            button {
                style: "display:flex; align-items:center; gap:6px; padding:4px 10px; \
                        background:{t.chrome.surface_raised.css()}; color:{t.chrome.text_dim.css()}; \
                        border:1px solid {t.chrome.surface_sunken.shade(-0.25).css()}; \
                        border-radius:6px; cursor:pointer; font-size:11px; \
                        font-family:Fira Sans, DejaVu Sans, sans-serif;",
                onclick: move |_| open.toggle(),
                span { style: "color:{t.chrome.text_faint.css()};", "Profile" }
                span { style: "font-weight:600; color:{t.chrome.text.css()};", "{active_name}" }
                span { "\u{25be}" }
            }
            if open() {
                div {
                    style: "position:fixed; inset:0; z-index:20;",
                    onclick: move |_| open.set(false),
                }
                div {
                    style: "position:absolute; top:100%; right:0; margin-top:4px; z-index:30; \
                            min-width:220px; background:{t.chrome.surface_raised.css()}; \
                            border:1px solid {t.chrome.surface_sunken.shade(-0.25).css()}; \
                            border-radius:8px; box-shadow:0 10px 30px rgba(0,0,0,0.5); \
                            padding:6px; display:flex; flex-direction:column; gap:2px;",
                    for profile in profiles {
                        {
                            let selected = profile.slug == active_slug;
                            let slug = profile.slug.clone();
                            let row_bg = if selected {
                                t.chrome.accent.shade(-0.6).css()
                            } else {
                                "transparent".to_string()
                            };
                            rsx! {
                                button {
                                    key: "{profile.slug}",
                                    style: "display:flex; flex-direction:column; align-items:flex-start; \
                                            gap:2px; padding:6px 8px; width:100%; text-align:left; \
                                            background:{row_bg}; \
                                            color:{t.chrome.text.css()}; border:none; border-radius:6px; \
                                            cursor:pointer; font-family:Fira Sans, DejaVu Sans, sans-serif;",
                                    onclick: move |_| {
                                        on_select.call(slug.clone());
                                        open.set(false);
                                    },
                                    span { style: "font-size:12px; font-weight:600;", "{profile.name}" }
                                    span { style: "font-size:10px; color:{t.chrome.text_dim.css()};", "{profile.description}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
