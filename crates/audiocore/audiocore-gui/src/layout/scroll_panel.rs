use crate::theme::use_theme;
use nice_plug_dioxus::prelude::*;

/// Scrollable container with up/down arrow buttons.
///
/// Mouse wheel is not available in Blitz, so arrow buttons provide scrolling.
#[component]
pub fn ScrollPanel(
    #[props(default = "100%")] width: &'static str,
    #[props(default = "200px")] height: &'static str,
    #[props(default = 20.0)] scroll_step: f64,
    children: Element,
) -> Element {
    let t = use_theme();
    let t = *t.read();
    let mut scroll_offset = use_signal(|| 0.0f64);

    let offset = *scroll_offset.read();

    rsx! {
        div {
            style: format!(
                "position:relative; width:{width}; height:{height}; \
                 overflow:hidden; {INSET}",
                INSET = t.style_inset(),
            ),

            // Content container
            div {
                style: format!("position:relative; top:{top}px;", top = -offset),
                {children}
            }

            // Up arrow
            div {
                style: format!(
                    "position:absolute; top:2px; right:2px; z-index:10; \
                     width:20px; height:20px; border-radius:{RADIUS}; \
                     display:flex; align-items:center; justify-content:center; \
                     cursor:pointer; background:{BG}; opacity:0.8; \
                     border:1px solid {BORDER};",
                    RADIUS = t.radius_round,
                    BG = t.surface_raised,
                    BORDER = t.border_subtle,
                ),
                onclick: move |_| {
                    scroll_offset.set((offset - scroll_step).max(0.0));
                },
                svg {
                    width: "10",
                    height: "10",
                    view_box: "0 0 10 10",
                    path {
                        d: "M2 7 L5 3 L8 7",
                        stroke: "{t.accent}",
                        stroke_width: "1.5",
                        fill: "none",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                    }
                }
            }

            // Down arrow
            div {
                style: format!(
                    "position:absolute; bottom:2px; right:2px; z-index:10; \
                     width:20px; height:20px; border-radius:{RADIUS}; \
                     display:flex; align-items:center; justify-content:center; \
                     cursor:pointer; background:{BG}; opacity:0.8; \
                     border:1px solid {BORDER};",
                    RADIUS = t.radius_round,
                    BG = t.surface_raised,
                    BORDER = t.border_subtle,
                ),
                onclick: move |_| {
                    scroll_offset.set(offset + scroll_step);
                },
                svg {
                    width: "10",
                    height: "10",
                    view_box: "0 0 10 10",
                    path {
                        d: "M2 3 L5 7 L8 3",
                        stroke: "{t.accent}",
                        stroke_width: "1.5",
                        fill: "none",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                    }
                }
            }
        }
    }
}
