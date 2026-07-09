//! Reverb editor — Dioxus GUI root component.
//!
//! Layout: header, algorithm selector, variant selector (conditional),
//! main controls, secondary controls, collapsible sections, meters.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use audio_gui::controls::knob::Knob;
use audio_gui::controls::segment::SegmentButton;
use audio_gui::prelude::{
    use_init_theme, CollapsibleSection, ControlGroup, Divider, DragProvider, KnobSize,
    LevelMeterDb, SectionLabel, Tooltip, TooltipPosition,
};
use fts_plugin_core::prelude::*;

use crate::{FtsReverbParams, ReverbUiState};

/// Root editor component.
#[component]
pub fn App() -> Element {
    let t = use_init_theme();
    let t = *t.read();

    let shared = use_context::<SharedState>();
    let ui: Arc<ReverbUiState> = shared
        .get::<ReverbUiState>()
        .expect("ReverbUiState missing");
    let params: Arc<FtsReverbParams> = ui.params.clone();
    let ctx = use_param_context();

    // Read metering values
    let input_db = ui.input_peak_db.load(Ordering::Relaxed);
    let output_db = ui.output_peak_db.load(Ordering::Relaxed);

    // Current algorithm index
    let algo_idx = params.algorithm.value() as i32;

    // Algorithm setter
    let algo_setter = |value: f32| {
        let ctx = ctx.clone();
        let p = params.clone();
        move |_: ()| {
            ctx.begin_set_raw(p.algorithm.as_ptr());
            ctx.set_normalized_raw(p.algorithm.as_ptr(), value / 11.0);
            ctx.end_set_raw(p.algorithm.as_ptr());
        }
    };

    // Variant setters
    let room_setter = |value: f32| {
        let ctx = ctx.clone();
        let p = params.clone();
        move |_: ()| {
            ctx.begin_set_raw(p.room_variant.as_ptr());
            ctx.set_normalized_raw(p.room_variant.as_ptr(), value / 2.0);
            ctx.end_set_raw(p.room_variant.as_ptr());
        }
    };

    let hall_setter = |value: f32| {
        let ctx = ctx.clone();
        let p = params.clone();
        move |_: ()| {
            ctx.begin_set_raw(p.hall_variant.as_ptr());
            ctx.set_normalized_raw(p.hall_variant.as_ptr(), value / 2.0);
            ctx.end_set_raw(p.hall_variant.as_ptr());
        }
    };

    let plate_setter = |value: f32| {
        let ctx = ctx.clone();
        let p = params.clone();
        move |_: ()| {
            ctx.begin_set_raw(p.plate_variant.as_ptr());
            ctx.set_normalized_raw(p.plate_variant.as_ptr(), value / 2.0);
            ctx.end_set_raw(p.plate_variant.as_ptr());
        }
    };

    let spring_setter = |value: f32| {
        let ctx = ctx.clone();
        let p = params.clone();
        move |_: ()| {
            ctx.begin_set_raw(p.spring_variant.as_ptr());
            ctx.set_normalized_raw(p.spring_variant.as_ptr(), value / 1.0);
            ctx.end_set_raw(p.spring_variant.as_ptr());
        }
    };

    // Current variant indices
    let room_var = params.room_variant.value() as i32;
    let hall_var = params.hall_variant.value() as i32;
    let plate_var = params.plate_variant.value() as i32;
    let spring_var = params.spring_variant.value() as i32;

    // Algorithm labels
    let algo_labels: &[(i32, &str)] = &[
        (0, "Room"),
        (1, "Hall"),
        (2, "Plate"),
        (3, "Spring"),
        (4, "Cloud"),
        (5, "Bloom"),
        (6, "Shimmer"),
        (7, "Chorale"),
        (8, "Magneto"),
        (9, "Non-Linear"),
        (10, "Swell"),
        (11, "Reflections"),
    ];

    // Algorithm display name
    let algo_name = algo_labels
        .iter()
        .find(|(idx, _)| *idx == algo_idx)
        .map(|(_, name)| *name)
        .unwrap_or("Room");

    // Pre-bind theme variables
    let border = t.border;
    let spacing_section = t.spacing_section;
    let spacing_card = t.spacing_card;
    let spacing_label = t.spacing_label;
    let spacing_control = t.spacing_control;
    let font_size_title = t.font_size_title;
    let font_size_tiny = t.font_size_tiny;
    let letter_spacing_label = t.letter_spacing_label;
    let text_bright = t.text_bright;
    let text_dim = t.text_dim;
    let style_card = t.style_card();
    let root_style = t.root_style();
    let base_css = t.base_css();
    let header_value_style = t.style_header_value("80px");

    rsx! {
        document::Style { {base_css} }

        DragProvider {
        div {
            style: format!(
                "{root_style} display:flex; flex-direction:column; gap:{spacing_section}; overflow:hidden;",
            ),

            // ── Header ───────────────────────────────────────────
            div {
                style: format!(
                    "display:flex; justify-content:space-between; align-items:center; \
                     padding-bottom:6px; border-bottom:1px solid {border};",
                ),
                div {
                    style: "display:flex; align-items:baseline; gap:12px;",
                    div {
                        style: format!(
                            "font-size:{font_size_title}; font-weight:700; letter-spacing:{letter_spacing_label}; color:{text_bright};",
                        ),
                        "FTS REVERB"
                    }
                    div {
                        style: format!(
                            "{header_value_style} color:{text_bright};",
                        ),
                        "{algo_name}"
                    }
                }
                div {
                    style: format!("font-size:{font_size_tiny}; color:{text_dim};"),
                    "FastTrackStudio"
                }
            }

            // ── Algorithm Selector ───────────────────────────────
            div {
                style: format!(
                    "display:flex; gap:{spacing_label}; justify-content:center; flex-wrap:wrap;",
                ),
                for &(idx, label) in algo_labels.iter() {
                    SegmentButton {
                        label: label,
                        selected: algo_idx == idx,
                        on_click: algo_setter(idx as f32),
                    }
                }
            }

            // ── Variant Selector (conditional) ───────────────────
            match algo_idx {
                // Room variants
                0 => rsx! {
                    div {
                        style: format!(
                            "display:flex; gap:{spacing_label}; justify-content:center;",
                        ),
                        SegmentButton { label: "Medium", selected: room_var == 0, on_click: room_setter(0.0) }
                        SegmentButton { label: "Chamber", selected: room_var == 1, on_click: room_setter(1.0) }
                        SegmentButton { label: "Studio", selected: room_var == 2, on_click: room_setter(2.0) }
                    }
                },
                // Hall variants
                1 => rsx! {
                    div {
                        style: format!(
                            "display:flex; gap:{spacing_label}; justify-content:center;",
                        ),
                        SegmentButton { label: "Concert", selected: hall_var == 0, on_click: hall_setter(0.0) }
                        SegmentButton { label: "Cathedral", selected: hall_var == 1, on_click: hall_setter(1.0) }
                        SegmentButton { label: "Arena", selected: hall_var == 2, on_click: hall_setter(2.0) }
                    }
                },
                // Plate variants
                2 => rsx! {
                    div {
                        style: format!(
                            "display:flex; gap:{spacing_label}; justify-content:center;",
                        ),
                        SegmentButton { label: "Dattorro", selected: plate_var == 0, on_click: plate_setter(0.0) }
                        SegmentButton { label: "Lexicon", selected: plate_var == 1, on_click: plate_setter(1.0) }
                        SegmentButton { label: "Progenitor", selected: plate_var == 2, on_click: plate_setter(2.0) }
                    }
                },
                // Spring variants
                3 => rsx! {
                    div {
                        style: format!(
                            "display:flex; gap:{spacing_label}; justify-content:center;",
                        ),
                        SegmentButton { label: "Classic", selected: spring_var == 0, on_click: spring_setter(0.0) }
                        SegmentButton { label: "Vintage", selected: spring_var == 1, on_click: spring_setter(1.0) }
                    }
                },
                _ => rsx! {},
            }

            // ── Main content: controls + meters ──────────────────
            div {
                style: format!("display:flex; gap:{spacing_section}; flex:1; min-height:0;"),

                // Controls area
                div {
                    style: format!(
                        "flex:1; {style_card} padding:{spacing_card}; \
                         display:flex; flex-direction:column; gap:{spacing_section}; overflow-y:auto;",
                    ),

                    // ── Main Controls: Decay, Size, Pre-Delay, Mix ───
                    div {
                        style: format!(
                            "display:flex; flex-direction:column; gap:{spacing_section};",
                        ),
                        SectionLabel { text: "Main" }
                        div {
                            style: format!(
                                "display:flex; justify-content:center; gap:{spacing_control};",
                            ),
                            Knob { param_ptr: params.decay.as_ptr(), size: KnobSize::Large }
                            Knob { param_ptr: params.size.as_ptr(), size: KnobSize::Large }
                            Knob { param_ptr: params.predelay.as_ptr(), size: KnobSize::Large }
                            Knob { param_ptr: params.mix.as_ptr(), size: KnobSize::Large }
                        }
                    }

                    // ── Secondary Controls ────────────────────────────
                    div {
                        style: "display:flex; gap:20px; justify-content:center; align-items:flex-start;",

                        ControlGroup {
                            label: "Character",
                            Knob { param_ptr: params.diffusion.as_ptr(), size: KnobSize::Medium }
                            Knob { param_ptr: params.damping.as_ptr(), size: KnobSize::Medium }
                            Knob { param_ptr: params.modulation.as_ptr(), size: KnobSize::Medium }
                        }

                        Divider {}

                        ControlGroup {
                            label: "Tone & Width",
                            Knob { param_ptr: params.tone.as_ptr(), size: KnobSize::Medium }
                            Knob { param_ptr: params.width.as_ptr(), size: KnobSize::Medium }
                        }
                    }

                    // ── Input Conditioning (collapsible) ──────────────
                    CollapsibleSection {
                        title: "INPUT CONDITIONING",
                        initially_open: false,

                        div {
                            style: format!(
                                "display:flex; gap:{spacing_control}; justify-content:center;",
                            ),
                            Tooltip {
                                text: "High-pass filter on reverb input".to_string(),
                                position: TooltipPosition::Bottom,
                                Knob { param_ptr: params.input_hp.as_ptr(), size: KnobSize::Small }
                            }
                            Tooltip {
                                text: "Low-pass filter on reverb input".to_string(),
                                position: TooltipPosition::Bottom,
                                Knob { param_ptr: params.input_lp.as_ptr(), size: KnobSize::Small }
                            }
                        }
                    }

                    // ── Extras (collapsible) ──────────────────────────
                    CollapsibleSection {
                        title: "EXTRAS",
                        initially_open: false,

                        div {
                            style: format!(
                                "display:flex; gap:{spacing_control}; justify-content:center;",
                            ),
                            Tooltip {
                                text: "Algorithm-specific parameter A".to_string(),
                                position: TooltipPosition::Top,
                                Knob { param_ptr: params.extra_a.as_ptr(), size: KnobSize::Small }
                            }
                            Tooltip {
                                text: "Algorithm-specific parameter B".to_string(),
                                position: TooltipPosition::Top,
                                Knob { param_ptr: params.extra_b.as_ptr(), size: KnobSize::Small }
                            }
                        }
                    }
                }

                // Meters
                div {
                    style: format!(
                        "{style_card} padding:8px; display:flex; gap:{spacing_section}; align-items:stretch;",
                    ),
                    LevelMeterDb { level_db: input_db, label: "IN".to_string() }
                    LevelMeterDb { level_db: output_db, label: "OUT".to_string() }
                }
            }
        }
        } // DragProvider
    }
}
