//! Layout primitives for the comp control surface.
//!
//! The editor is a row of labelled sections, each holding a small grid of
//! knobs (plus the occasional segmented selector or toggle). These wrappers
//! keep [`crate::control_view`] declarative — it names sections and the
//! params inside them, not borders and paddings.
//!
//! Everything here is presentation only. The colors come from the active
//! [`crate::profile_view::ProfileSkin`] so a profile change re-tints the whole
//! surface without the layout code knowing which profile is active.

use audiocore_core::prelude::*;
use fts_audio_ui::prelude::*;

use crate::profile_view::ProfileSkin;

/// One labelled group of controls.
///
/// Renders a titled panel; `children` is the control grid. The heading uses
/// the skin accent so the section titles carry the profile's identity.
#[component]
pub fn Section(
    label: String,
    skin: ProfileSkin,
    /// Fixed pixel width. `None` lets the section size to its content.
    #[props(default)]
    width: Option<f64>,
    /// Drop the panel chrome — border, background, padding.
    ///
    /// A section inside a [`fts_audio_ui::shell::FloatingPanel`]
    /// is already sitting on a card; a second border around each group turns
    /// the floating bar into a row of boxes.
    #[props(default = false)]
    flat: bool,
    children: Element,
) -> Element {
    let width_style = width
        .map(|w| format!("width:{w}px;"))
        .unwrap_or_else(|| "".to_string());
    let chrome = if flat {
        "padding:0;".to_string()
    } else {
        format!(
            "padding:10px 12px 12px; border-radius:8px; \
             border:1px solid {}; background:{};",
            skin.border, skin.highlight,
        )
    };

    rsx! {
        div {
            "data-testid": "section-{label.to_lowercase().replace(' ', \"-\")}",
            style: format!(
                "display:flex; flex-direction:column; gap:6px; {width_style} {chrome}"
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
                style: format!(
                    "display:flex; align-items:flex-start; gap:10px 12px; flex-wrap:{};",
                    // A group in a floating bar has to stay one row — wrapping
                    // there grows the bar over the surface it floats on.
                    if flat { "nowrap" } else { "wrap" },
                ),
                {children}
            }
        }
    }
}

/// A knob with a stable `data-testid` for the headless tests.
///
/// `testid` is the bare param slug (`"threshold"`), matching the ids the
/// existing `gui_editor` tests drive.
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

/// A segmented selector under a small caption — used where the choices are
/// few enough to show at once (Style's four detector models).
#[component]
pub fn ParamSelector(
    handle: ParamHandle,
    testid: String,
    label: String,
    options: Vec<String>,
    skin: ProfileSkin,
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

/// A dropdown selector under a small caption.
///
/// The same contract as [`ParamSelector`], for parameters with too many
/// choices to lay out as segments — the character waveshaper's seven shapes
/// take more width than the whole Dynamics group otherwise.
#[component]
pub fn ParamDropdown(
    handle: ParamHandle,
    testid: String,
    label: String,
    options: Vec<String>,
    skin: ProfileSkin,
    #[props(default = 96.0)] width: f64,
) -> Element {
    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:4px;",
            div {
                style: format!("font-size:10px; color:{}; letter-spacing:0.06em;", skin.text),
                "{label}"
            }
            // Qualified: `architect_ui` has a `Dropdown` of its own (a menu, not a
            // param control) and both preludes are in scope here.
            fts_audio_ui::controls::Dropdown {
                handle,
                options,
                color: skin.accent.to_string(),
                width,
                testid: format!("select-{testid}"),
            }
        }
    }
}
