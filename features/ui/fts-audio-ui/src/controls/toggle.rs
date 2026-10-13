//! Toggle switch bound to a [`ParamHandle`]. Treats the param as boolean
//! (true when normalized > 0.5).

use crate::param::ParamHandle;
use crate::theme::*;
use dioxus::prelude::*;

#[component]
pub fn Toggle(
    handle: ParamHandle,
    #[props(default)] label: Option<String>,
    #[props(default)] color: Option<String>,
) -> Element {
    let mut bump = use_signal(|| 0u32);
    let _ = *bump.read();

    let on = handle.normalized() > 0.5;
    let accent = color.as_deref().unwrap_or(ACCENT);
    let track_bg = if on { accent } else { BORDER };
    let thumb_x = if on { "18px" } else { "2px" };
    let label_str = label.unwrap_or_else(|| handle.name());

    rsx! {
        div {
            style: "display:flex; align-items:center; gap:6px; cursor:pointer;",
            onclick: {
                let handle = handle.clone();
                move |_| {
                    handle.begin_edit();
                    handle.set_normalized(if on { 0.0 } else { 1.0 });
                    handle.end_edit();
                    bump += 1;
                }
            },
            div {
                style: format!(
                    "width:36px; height:20px; border-radius:10px; position:relative; \
                     background:{track_bg};"
                ),
                div {
                    style: format!(
                        "width:16px; height:16px; border-radius:8px; background:#fff; \
                         position:absolute; top:2px; left:{thumb_x};"
                    ),
                }
            }
            span {
                style: format!("font-size:12px; color:{};", if on { TEXT } else { TEXT_DIM }),
                "{label_str}"
            }
        }
    }
}
