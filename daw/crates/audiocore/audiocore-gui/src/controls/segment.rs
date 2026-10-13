//! Pill-style segment button for mutually exclusive selections.
//!
//! Recessed panel background with raised active segment.

use crate::theme::use_theme;
use nice_plug_dioxus::prelude::*;

/// Pill-style segment button for enum-style selections.
///
/// Typically used in a row for mutually exclusive options.
/// The parent is responsible for tracking the selected index and calling
/// `ParamContext` to set the param value.
#[component]
pub fn SegmentButton(label: &'static str, selected: bool, on_click: EventHandler<()>) -> Element {
    let t = use_theme();
    let t = *t.read();

    let bg = if selected { t.accent } else { t.surface_raised };
    let border_color = if selected { t.accent } else { t.border };
    let color = if selected { "#fff" } else { t.text_dim };
    let shadow = if selected {
        format!(
            "{SUBTLE}, 0 0 6px {GLOW}",
            SUBTLE = t.shadow_subtle,
            GLOW = t.accent_glow,
        )
    } else {
        t.shadow_inset.to_string()
    };

    rsx! {
        div {
            style: format!(
                "padding:4px 8px; border-radius:{RADIUS}; font-size:11px; font-weight:500; \
                 cursor:pointer; border:1px solid {border_color}; background:{bg}; \
                 color:{color}; box-shadow:{shadow}; transition:{TRANS};",
                RADIUS = t.radius_button,
                TRANS = t.transition_fast,
            ),
            onclick: move |_| on_click.call(()),
            "{label}"
        }
    }
}
