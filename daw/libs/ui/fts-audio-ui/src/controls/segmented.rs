//! Segmented control — pill-row of mutually-exclusive choices bound to a
//! [`ParamHandle`].
//!
//! The handle's normalized value maps linearly to a discrete index. Each
//! click writes `index / (count - 1)` back to the handle.

use crate::param::ParamHandle;
use crate::theme::*;
use dioxus::prelude::*;

#[component]
pub fn Segmented(
    handle: ParamHandle,
    /// Display labels, one per discrete value.
    options: Vec<String>,
    #[props(default)] color: Option<String>,
    #[props(default)] disabled: bool,
) -> Element {
    let mut bump = use_signal(|| 0u32);
    let _ = *bump.read();

    let count = options.len().max(1);
    let denom = (count.saturating_sub(1)).max(1) as f32;
    let normalized = handle.normalized().clamp(0.0, 1.0);
    let selected = ((normalized * denom).round() as usize).min(count - 1);
    let accent = color.as_deref().unwrap_or(ACCENT);
    let opacity = if disabled { "0.5" } else { "1.0" };

    rsx! {
        div {
            style: format!("display:inline-flex; gap:4px; opacity:{opacity};"),
            for (idx, label) in options.iter().enumerate() {
                {
                    let is_sel = idx == selected;
                    let bg = if is_sel { accent } else { "transparent" };
                    let border = if is_sel { accent } else { BORDER };
                    let color = if is_sel { "#fff" } else { TEXT_DIM };
                    let label = label.clone();
                    let handle = handle.clone();
                    rsx! {
                        div {
                            style: format!(
                                "padding:4px 8px; border-radius:4px; font-size:11px; \
                                 font-weight:500; cursor:pointer; \
                                 border:1px solid {border}; background:{bg}; color:{color};"
                            ),
                            onclick: move |_| {
                                if disabled { return; }
                                let v = idx as f32 / denom;
                                handle.begin_edit();
                                handle.set_normalized(v);
                                handle.end_edit();
                                bump += 1;
                            },
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}
