use crate::theme::use_theme;
use nice_plug_dioxus::prelude::*;

/// Scrollable selectable list.
#[component]
pub fn ListBox(
    items: Vec<String>,
    #[props(default)] selected: Option<usize>,
    on_select: EventHandler<usize>,
    #[props(default = "100%")] width: &'static str,
    #[props(default = "200px")] height: &'static str,
) -> Element {
    let t = use_theme();
    let t = *t.read();
    let mut hovered_idx = use_signal(|| None::<usize>);

    rsx! {
        div {
            style: format!(
                "width:{width}; height:{height}; overflow-y:auto; {INSET}",
                INSET = t.style_inset(),
            ),

            for (idx, item) in items.iter().enumerate() {
                {
                    let is_selected = selected == Some(idx);
                    let is_hovered = *hovered_idx.read() == Some(idx);
                    let row_bg = if is_selected {
                        t.accent_dim
                    } else if is_hovered {
                        t.surface_hover
                    } else if idx % 2 == 0 {
                        t.surface
                    } else {
                        t.card_bg
                    };
                    let row_color = if is_selected { t.text_bright } else { t.text };

                    rsx! {
                        div {
                            key: "{idx}",
                            style: format!(
                                "padding:4px 8px; cursor:pointer; \
                                 font-size:{FSIZE}; color:{row_color}; \
                                 background:{row_bg};",
                                FSIZE = t.font_size_value,
                            ),
                            onmouseenter: move |_| hovered_idx.set(Some(idx)),
                            onmouseleave: move |_| hovered_idx.set(None),
                            onclick: move |_| on_select.call(idx),
                            "{item}"
                        }
                    }
                }
            }
        }
    }
}
