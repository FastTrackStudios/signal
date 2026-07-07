//! Chorus editor — Dioxus GUI root component.
//!
//! Layout: header, effect type selector, engine selector, main controls
//! (rate, depth, feedback, mix), secondary controls (color, width, voices),
//! and level meters.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use audio_gui::controls::knob::Knob;
use audio_gui::controls::segment::SegmentButton;
use audio_gui::prelude::{
    use_init_theme, ControlGroup, Divider, DragProvider, KnobSize, LevelMeterDb,
};
use fts_plugin_core::prelude::*;

use crate::{ChorusUiState, FtsChorusParams};

/// Root editor component.
#[component]
pub fn App() -> Element {
    let t = use_init_theme();
    let t = *t.read();

    let shared = use_context::<SharedState>();
    let ui: Arc<ChorusUiState> = shared
        .get::<ChorusUiState>()
        .expect("ChorusUiState missing");
    let params: Arc<FtsChorusParams> = ui.params.clone();
    let params_arc = params.clone();
    let ctx = use_param_context();

    // Read metering values
    let input_db = ui.input_peak_db.load(Ordering::Relaxed);
    let output_db = ui.output_peak_db.load(Ordering::Relaxed);

    // Current state
    let effect_type = params.effect_type.value();
    let engine = params.engine.value();

    let effect_name = match effect_type {
        0 => "Chorus",
        1 => "Flanger",
        2 => "Vibrato",
        _ => "Chorus",
    };

    // Effect type setter (max=2)
    let type_setter = |value: f32| {
        let ctx = ctx.clone();
        let p = params_arc.clone();
        move |_: ()| {
            ctx.begin_set_raw(p.effect_type.as_ptr());
            ctx.set_normalized_raw(p.effect_type.as_ptr(), value / 2.0);
            ctx.end_set_raw(p.effect_type.as_ptr());
        }
    };

    // Engine setter (max=8)
    let engine_setter = |value: f32| {
        let ctx = ctx.clone();
        let p = params_arc.clone();
        move |_: ()| {
            ctx.begin_set_raw(p.engine.as_ptr());
            ctx.set_normalized_raw(p.engine.as_ptr(), value / 8.0);
            ctx.end_set_raw(p.engine.as_ptr());
        }
    };

    // Effect type labels
    let type_labels: &[(i32, &str)] = &[(0, "Chorus"), (1, "Flanger"), (2, "Vibrato")];

    // Engine labels
    let engine_labels: &[(i32, &str)] = &[
        (0, "Cubic"),
        (1, "BBD"),
        (2, "Tape"),
        (3, "Orbit"),
        (4, "Juno"),
        (5, "Ensemble"),
        (6, "Detune"),
        (7, "Diffuse"),
        (8, "Drift"),
    ];

    // Pre-bind theme variables
    let border = t.border;
    let spacing_section = t.spacing_section;
    let spacing_card = t.spacing_card;
    let spacing_label = t.spacing_label;
    let spacing_tight = t.spacing_tight;
    let font_size_title = t.font_size_title;
    let font_size_label = t.font_size_label;
    let letter_spacing_label = t.letter_spacing_label;
    let text_bright = t.text_bright;
    let text_dim = t.text_dim;
    let style_card = t.style_card();
    let root_style = t.root_style();
    let base_css = t.base_css();
    let style_header_value = t.style_header_value("70px");

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
                        "FTS CHORUS"
                    }
                    div {
                        style: format!(
                            "{style_header_value} color:{text_dim};",
                        ),
                        "{effect_name}"
                    }
                }
                div {
                    style: format!("font-size:{font_size_label}; color:{text_dim};"),
                    "FastTrackStudio"
                }
            }

            // ── Effect Type Selector ─────────────────────────────
            div {
                style: format!(
                    "display:flex; gap:{spacing_label}; justify-content:center;",
                ),
                for &(idx, label) in type_labels.iter() {
                    SegmentButton {
                        label: label,
                        selected: effect_type == idx,
                        on_click: type_setter(idx as f32),
                    }
                }
            }

            // ── Engine Selector ──────────────────────────────────
            div {
                style: format!(
                    "display:flex; gap:{spacing_tight}; justify-content:center; flex-wrap:wrap;",
                ),
                for &(idx, label) in engine_labels.iter() {
                    SegmentButton {
                        label: label,
                        selected: engine == idx,
                        on_click: engine_setter(idx as f32),
                    }
                }
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

                    // ── Row 1: Main controls ─────────────────────
                    div {
                        style: "display:flex; gap:20px; justify-content:center;",

                        ControlGroup {
                            label: "Main",
                            Knob { param_ptr: params.rate.as_ptr(), size: KnobSize::Large }
                            Knob { param_ptr: params.depth.as_ptr(), size: KnobSize::Large }
                            Knob { param_ptr: params.feedback.as_ptr(), size: KnobSize::Large }
                            Knob { param_ptr: params.mix.as_ptr(), size: KnobSize::Large }
                        }
                    }

                    // ── Row 2: Secondary controls ────────────────
                    div {
                        style: "display:flex; gap:20px; justify-content:center; align-items:flex-start;",

                        ControlGroup {
                            label: "Color & Stereo",
                            Knob { param_ptr: params.color.as_ptr(), size: KnobSize::Medium }
                            Knob { param_ptr: params.width.as_ptr(), size: KnobSize::Medium }
                        }

                        Divider {}

                        ControlGroup {
                            label: "Voices",
                            Knob { param_ptr: params.voices.as_ptr(), size: KnobSize::Small }
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
