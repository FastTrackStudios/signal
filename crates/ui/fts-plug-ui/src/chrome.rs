//! The shared editor shell: redraw tick, root layout, header, sections.
//!
//! Every FTS plugin editor is the same shape — a titled header, an optional
//! visualizer, a row of labelled control sections, and meters — so that shape
//! lives here once. A plugin's `control_view` supplies its params and its
//! [`Skin`]; the chrome supplies everything else.
//!
//! ## Two things that are load-bearing, not cosmetic
//!
//! - **The redraw tick.** Editors read plugin params and meter atomics
//!   directly, not through signals, so nothing marks the scope dirty when the
//!   audio thread moves a value. [`use_redraw_tick`] spawns one OS thread that
//!   calls `schedule_update` at ~30 Hz. It must be called from the scope that
//!   *does the reading* — `schedule_update` only dirties its own scope — and
//!   the counter must then reach the DOM via [`PluginRoot`]'s `frame` prop:
//!   without a DOM mutation Blitz treats the document as clean, idle redraws
//!   collapse, and the meters freeze.
//! - **Fitting the editor.** Blitz will not overflow-scroll a
//!   height-constrained container: a section that does not fit is not clipped,
//!   it is allocated 0 px and collapses to 0×0, which also makes it
//!   unhittable. Editors must therefore *fit* — page the controls (see the
//!   Comp editor's Basic/Advanced split) rather than letting them wrap.

use architect_ui::prelude::{default_theme_preset, ThemeMode, ThemeProvider, ThemeState};
use audiocore_core::prelude::*;
use fts_audio_ui::prelude::*;

use crate::skin::Skin;

/// Base document CSS shared by every editor.
pub const BASE_CSS: &str = "*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; } \
     html, body { width: 100%; height: 100%; overflow: hidden; \
     background: var(--background); color: var(--foreground); \
     font-family: var(--font-sans, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, sans-serif); \
     font-size: 13px; }";

const ROOT_STYLE: &str = "width:100vw; height:100vh; \
     display:flex; flex-direction:column; \
     color:var(--foreground); background:var(--background); \
     font-family:var(--font-sans, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, sans-serif); \
     font-size:13px; user-select:none; position:relative; overflow:hidden;";

/// Start (once) the ~30 Hz repaint driver **for the calling scope** and return a
/// counter that changes every render.
///
/// Call this from the component that reads the plugin params and meter
/// atomics — normally the plugin's own shell — and pass the result to
/// [`PluginRoot`]'s `frame` prop so it reaches the DOM.
///
/// It has to be the reading scope, not the chrome: `schedule_update` only
/// marks the scope it was called in. If the tick lived inside `PluginRoot`,
/// `PluginRoot` would re-render on every tick while the shell that actually
/// loads the atomics never re-ran, and the meters would sit frozen at their
/// initial values.
pub fn use_redraw_tick() -> u64 {
    let mut tick: Signal<u64> = use_signal(|| 0);
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
    tick += 1;
    let frame = *tick.read();
    frame
}

/// Outermost wrapper: framework CSS, the plugin's own compiled Tailwind, and
/// the theme provider.
///
/// `tailwind_css` is the plugin UI crate's own compiled stylesheet, embedded
/// with `include_str!`. It is not optional: `nice_plug_dioxus::TAILWIND_CSS`
/// only covers the framework's own widgets, so without the plugin's sheet
/// every layout-critical utility (`flex-1`, `min-h-0`, …) is undefined and the
/// layout collapses in DAW hosts.
#[component]
pub fn PluginApp(tailwind_css: String, children: Element) -> Element {
    let theme_state = use_signal(|| ThemeState::new(default_theme_preset(), ThemeMode::Dark));
    rsx! {
        document::Style { {nice_plug_dioxus::TAILWIND_CSS} }
        document::Style { {tailwind_css} }
        ThemeProvider { state: theme_state, {children} }
    }
}

/// The editor body: base CSS, drag provider, and the header.
///
/// Render this *inside* [`PluginApp`] so the theme context is in scope.
/// `children` become the content below the header.
///
/// `frame` must come from [`use_redraw_tick`] called in the caller's scope —
/// see that function for why it cannot live here. The value is written to a
/// `data-frame` attribute purely so the DOM changes every tick: without a
/// mutation Blitz treats the document as clean and idle redraws collapse.
#[component]
pub fn PluginRoot(
    /// Plugin name, e.g. `"FTS Limiter"`.
    title: String,
    /// Short descriptor under the name, e.g. `"Brickwall Limiter"`.
    subtitle: String,
    skin: Skin,
    /// Redraw counter from [`use_redraw_tick`] in the calling scope.
    frame: u64,
    /// Optional controls pinned to the right of the header (page toggles,
    /// selectors).
    #[props(default)]
    header_extra: Option<Element>,
    children: Element,
) -> Element {
    let _theme = use_init_theme();

    rsx! {
        document::Style { {BASE_CSS} }
        DragProvider {
            div {
                style: "{ROOT_STYLE}",
                "data-frame": "{frame}",

                div {
                    class: "flex justify-between items-center px-4 py-3 border-b border-border bg-card/50",
                    div { class: "flex items-baseline gap-3 shrink-0",
                        div {
                            class: "text-base font-bold tracking-wide text-foreground",
                            "{title}"
                        }
                        div {
                            class: "text-xs text-muted-foreground uppercase tracking-wider",
                            "{subtitle}"
                        }
                    }
                    if let Some(extra) = header_extra {
                        div {
                            style: "display:flex; align-items:center; gap:14px;",
                            {extra}
                        }
                    }
                }

                {children}
            }
        }
    }
}

/// The control-surface row: sections on the left, meters on the right.
///
/// A plain non-wrapping flex row by design — see the module docs on why
/// editors must fit rather than wrap.
#[component]
pub fn ControlSurface(
    /// Meters (or anything else) pinned right; sized to content.
    #[props(default)]
    aside: Option<Element>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: "flex-1 min-h-0 flex items-stretch gap-6 px-5 py-4",
            div {
                style: "flex:1; min-width:0; display:flex; align-items:stretch; gap:10px;",
                {children}
            }
            if let Some(aside) = aside {
                div { class: "flex items-end gap-3 shrink-0", "data-testid": "meters", {aside} }
            }
        }
    }
}

/// One labelled group of controls.
///
/// Gets `data-testid="section-<label>"` (lowercased, spaces to dashes) so
/// headless tests can assert the section rendered with real layout.
#[component]
pub fn Section(label: String, skin: Skin, children: Element) -> Element {
    rsx! {
        div {
            "data-testid": "section-{label.to_lowercase().replace(' ', \"-\")}",
            style: format!(
                "display:flex; flex-direction:column; gap:8px; \
                 padding:10px 12px 12px; border-radius:8px; \
                 border:1px solid {}; background:{};",
                skin.border, skin.panel,
            ),
            div {
                style: format!(
                    "font-size:10px; font-weight:700; letter-spacing:0.12em; \
                     text-transform:uppercase; color:{};",
                    skin.accent,
                ),
                "{label}"
            }
            div {
                style: "display:flex; flex-wrap:wrap; align-items:flex-start; gap:10px 12px;",
                {children}
            }
        }
    }
}

/// A knob carrying a stable `data-testid="knob-<testid>"`.
#[component]
pub fn ParamKnob(
    handle: ParamHandle,
    testid: String,
    #[props(default)] size: KnobSize,
    #[props(default)] color: Option<String>,
    #[props(default)] disabled: bool,
) -> Element {
    rsx! {
        div {
            "data-testid": "knob-{testid}",
            Knob { handle, size, color, disabled }
        }
    }
}

/// A segmented selector under a caption, `data-testid="select-<testid>"`.
#[component]
pub fn ParamSelector(
    handle: ParamHandle,
    testid: String,
    label: String,
    options: Vec<String>,
    skin: Skin,
) -> Element {
    rsx! {
        div {
            "data-testid": "select-{testid}",
            style: "display:flex; flex-direction:column; gap:4px;",
            div {
                style: format!("font-size:10px; color:{}; letter-spacing:0.06em;", skin.text),
                "{label}"
            }
            Segmented { handle, options, color: skin.accent.to_string() }
        }
    }
}

/// A labelled switch, `data-testid="toggle-<testid>"`.
#[component]
pub fn ParamToggle(handle: ParamHandle, testid: String, skin: Skin) -> Element {
    rsx! {
        div {
            "data-testid": "toggle-{testid}",
            style: "align-self:center;",
            Toggle { handle, color: skin.accent.to_string() }
        }
    }
}
