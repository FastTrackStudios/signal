use crate::theme::use_theme;
use nice_plug_dioxus::prelude::*;

#[component]
pub fn CollapsibleSection(
    title: &'static str,
    #[props(default = true)] initially_open: bool,
    children: Element,
) -> Element {
    let t = use_theme();
    let t = *t.read();
    let mut is_open = use_signal(|| initially_open);

    let open = *is_open.read();
    let chevron = if open { "\u{25be}" } else { "\u{25b8}" }; // ▾ / ▸
    let max_height = if open { "1000px" } else { "0px" };

    rsx! {
        div {
            style: format!("{CARD}", CARD = t.style_card()),

            // Header
            div {
                style: format!(
                    "display:flex; align-items:center; gap:6px; cursor:pointer; \
                     padding:{PAD}; user-select:none;",
                    PAD = t.spacing_card,
                ),
                onclick: move |_| is_open.set(!open),

                span {
                    style: format!(
                        "font-size:{FSIZE}; color:{DIM}; width:12px;",
                        FSIZE = t.font_size_value,
                        DIM = t.text_dim,
                    ),
                    "{chevron}"
                }

                span {
                    style: format!("{LABEL}", LABEL = t.style_label()),
                    "{title}"
                }
            }

            // Content
            div {
                style: format!(
                    "max-height:{max_height}; overflow:hidden; \
                     transition:max-height 0.2s ease;",
                ),

                div {
                    style: format!(
                        "padding:0 {PAD} {PAD} {PAD};",
                        PAD = t.spacing_card,
                    ),
                    {children}
                }
            }
        }
    }
}
