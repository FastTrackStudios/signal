use crate::theme::use_theme;
use nice_plug_dioxus::prelude::*;

/// Position of the tooltip relative to the wrapped element.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TooltipPosition {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

#[component]
pub fn Tooltip(
    text: String,
    #[props(default)] position: TooltipPosition,
    children: Element,
) -> Element {
    let t = use_theme();
    let t = *t.read();
    let mut hovered = use_signal(|| false);

    let is_hovered = *hovered.read();
    let opacity = if is_hovered { "1" } else { "0" };

    let pos_style = match position {
        TooltipPosition::Top => "bottom:calc(100% + 6px); left:50%; transform:translateX(-50%);",
        TooltipPosition::Bottom => "top:calc(100% + 6px); left:50%; transform:translateX(-50%);",
        TooltipPosition::Left => "right:calc(100% + 6px); top:50%; transform:translateY(-50%);",
        TooltipPosition::Right => "left:calc(100% + 6px); top:50%; transform:translateY(-50%);",
    };

    rsx! {
        div {
            style: "position:relative; display:inline-flex;",
            onmouseenter: move |_| hovered.set(true),
            onmouseleave: move |_| hovered.set(false),

            {children}

            div {
                style: format!(
                    "position:absolute; z-index:9999; pointer-events:none; \
                     {pos_style} \
                     background:{BG}; color:{TEXT}; \
                     border:1px solid {BORDER}; border-radius:{RADIUS}; \
                     padding:4px 8px; font-size:{FSIZE}; white-space:nowrap; \
                     box-shadow:{SHADOW}; opacity:{opacity}; \
                     transition:opacity 0.15s;",
                    BG = t.card_bg,
                    TEXT = t.text,
                    BORDER = t.border,
                    RADIUS = t.radius_small,
                    FSIZE = t.font_size_tiny,
                    SHADOW = t.shadow_raised,
                ),
                "{text}"
            }
        }
    }
}
