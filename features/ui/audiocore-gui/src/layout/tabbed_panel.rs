use crate::theme::use_theme;
use nice_plug_dioxus::prelude::*;

#[component]
pub fn TabBar(tabs: Vec<&'static str>, active: usize, on_change: EventHandler<usize>) -> Element {
    let t = use_theme();
    let t = *t.read();

    rsx! {
        div {
            style: format!(
                "display:flex; gap:2px; padding:2px; {INSET}",
                INSET = t.style_inset(),
            ),

            for (idx, tab) in tabs.iter().enumerate() {
                {
                    let is_active = idx == active;
                    let bg = if is_active { t.accent } else { t.surface_raised };
                    let color = if is_active { t.text_bright } else { t.text_dim };
                    let border_bottom = if is_active {
                        format!("border-bottom:2px solid {};", t.accent)
                    } else {
                        String::new()
                    };

                    rsx! {
                        div {
                            key: "{tab}",
                            style: format!(
                                "padding:6px 12px; cursor:pointer; font-size:11px; \
                                 font-weight:500; border-radius:{RADIUS}; \
                                 background:{bg}; color:{color}; \
                                 transition:{TRANS}; {border_bottom}",
                                RADIUS = t.radius_button,
                                TRANS = t.transition_fast,
                            ),
                            onclick: move |_| on_change.call(idx),
                            "{tab}"
                        }
                    }
                }
            }
        }
    }
}
