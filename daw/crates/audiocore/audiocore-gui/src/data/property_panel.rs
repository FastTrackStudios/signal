use crate::controls::{Dropdown, Knob, KnobSize, ParamSlider, Toggle};
use crate::theme::use_theme;
use nice_plug::prelude::ParamPtr;
use nice_plug_dioxus::prelude::*;

/// What control to render for a property row.
#[derive(Clone, PartialEq)]
pub enum PropertyControl {
    Knob(ParamPtr),
    Slider(ParamPtr),
    Toggle(ParamPtr),
    Dropdown {
        param_ptr: ParamPtr,
        items: Vec<String>,
    },
    ReadOnly(String),
}

/// A single property definition.
#[derive(Clone, PartialEq)]
pub struct PropertyDef {
    pub label: &'static str,
    pub control: PropertyControl,
}

/// Key-value parameter editor panel.
///
/// Each row has a label on the left and a control on the right.
#[component]
pub fn PropertyPanel(
    properties: Vec<PropertyDef>,
    #[props(default)] title: Option<&'static str>,
) -> Element {
    let t = use_theme();
    let t = *t.read();

    rsx! {
        div {
            style: format!("{CARD}", CARD = t.style_card()),

            if let Some(title) = title {
                div {
                    style: format!(
                        "{LABEL} margin-bottom:8px;",
                        LABEL = t.style_label(),
                    ),
                    "{title}"
                }
            }

            for (idx, prop) in properties.iter().enumerate() {
                {
                    let is_last = idx == properties.len() - 1;
                    let border_bottom = if is_last {
                        String::new()
                    } else {
                        format!("border-bottom:1px solid {};", t.border_subtle)
                    };

                    rsx! {
                        div {
                            key: "{idx}",
                            style: format!(
                                "display:flex; align-items:center; \
                                 justify-content:space-between; \
                                 padding:6px 0; {border_bottom}",
                            ),

                            // Label
                            span {
                                style: format!(
                                    "font-size:{FSIZE}; color:{DIM};",
                                    FSIZE = t.font_size_value,
                                    DIM = t.text_dim,
                                ),
                                "{prop.label}"
                            }

                            // Control
                            div {
                                style: "max-width:120px;",
                                {match &prop.control {
                                    PropertyControl::Knob(ptr) => rsx! {
                                        Knob { param_ptr: *ptr, size: KnobSize::Small }
                                    },
                                    PropertyControl::Slider(ptr) => rsx! {
                                        ParamSlider { param_ptr: *ptr, height: 20.0 }
                                    },
                                    PropertyControl::Toggle(ptr) => rsx! {
                                        Toggle { param_ptr: *ptr }
                                    },
                                    PropertyControl::Dropdown { param_ptr, items } => {
                                        let normalized = unsafe { param_ptr.modulated_normalized_value() };
                                        let step_count = items.len();
                                        let selected = if step_count > 0 {
                                            (normalized * (step_count - 1) as f32).round() as usize
                                        } else {
                                            0
                                        };
                                        rsx! {
                                            Dropdown {
                                                items: items.clone(),
                                                selected: selected,
                                                width: "100px",
                                                on_change: move |_idx: usize| {},
                                            }
                                        }
                                    },
                                    PropertyControl::ReadOnly(text) => rsx! {
                                        span {
                                            style: format!("{VALUE}", VALUE = t.style_value()),
                                            "{text}"
                                        }
                                    },
                                }}
                            }
                        }
                    }
                }
            }
        }
    }
}
