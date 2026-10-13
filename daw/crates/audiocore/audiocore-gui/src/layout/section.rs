//! Layout components — section cards, action buttons, control groups.
//!
//! Raised panels with depth shadows and consistent spacing tokens.

use crate::theme::use_theme;
use nice_plug_dioxus::prelude::*;

/// Card-style section wrapper with uppercase title.
///
/// Raised panel with subtle top-edge highlight and drop shadow.
#[component]
pub fn Section(title: &'static str, children: Element) -> Element {
    let t = use_theme();
    let t = *t.read();
    rsx! {
        div {
            style: format!(
                "{CARD} padding:{PAD}; margin-bottom:8px;",
                CARD = t.style_card(),
                PAD = t.spacing_card,
            ),
            div {
                style: format!(
                    "{LABEL} margin-bottom:6px;",
                    LABEL = t.style_label(),
                ),
                "{title}"
            }
            {children}
        }
    }
}

/// Full-width action button.
///
/// Raised with accent background and subtle glow.
#[component]
pub fn ActionButton(label: &'static str, on_click: EventHandler<()>) -> Element {
    let t = use_theme();
    let t = *t.read();
    rsx! {
        div {
            style: format!(
                "padding:8px 0; cursor:pointer; text-align:center; \
                 background:{ACCENT}; color:#fff; border-radius:{RADIUS}; \
                 font-size:13px; font-weight:600; margin-top:4px; \
                 box-shadow:{SHADOW}, 0 0 8px {GLOW}; \
                 transition:{TRANS};",
                ACCENT = t.accent,
                RADIUS = t.radius_card,
                SHADOW = t.shadow_subtle,
                GLOW = t.accent_glow,
                TRANS = t.transition_fast,
            ),
            onclick: move |_| on_click.call(()),
            "{label}"
        }
    }
}

/// A labeled group of controls with a sub-heading.
///
/// Renders a column with a tiny uppercase label and a horizontal row
/// of child controls.
#[component]
pub fn ControlGroup(
    label: &'static str,
    children: Element,
    /// Gap between controls inside the group. Defaults to 8px.
    #[props(default = "8px")]
    gap: &'static str,
) -> Element {
    let t = use_theme();
    let t = *t.read();
    rsx! {
        div {
            style: "display:flex; flex-direction:column; align-items:center; gap:5px;",
            div {
                style: format!("{LABEL}", LABEL = t.style_label()),
                "{label}"
            }
            div {
                style: format!(
                    "display:flex; gap:{GAP}; align-items:flex-end;",
                    GAP = gap,
                ),
                {children}
            }
        }
    }
}

/// Vertical divider line between control groups or UI sections.
///
/// Subtle border with slight depth.
#[component]
pub fn Divider() -> Element {
    let t = use_theme();
    let t = *t.read();
    rsx! {
        div {
            style: format!(
                "width:1px; background:{BORDER}; align-self:stretch; \
                 box-shadow:1px 0 0 rgba(255,255,255,0.03);",
                BORDER = t.border,
            ),
        }
    }
}

/// Tiny uppercase section label for annotating UI regions.
///
/// Smaller and lighter than `Section` — used inline above knob rows
/// or visualization areas.
#[component]
pub fn SectionLabel(text: &'static str) -> Element {
    let t = use_theme();
    let t = *t.read();
    rsx! {
        div {
            style: format!("{LABEL}", LABEL = t.style_label()),
            "{text}"
        }
    }
}
