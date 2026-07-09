//! Delay editor — Dioxus GUI root component.
//!
//! Layout: header, main controls (time, feedback, stereo, EQ, tape, diffusion, ducking), meters.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use audio_gui::controls::knob::Knob;
use audio_gui::controls::segment::SegmentButton;
use audio_gui::controls::toggle::Toggle;
use audio_gui::prelude::{
    use_init_theme, ControlGroup, DragProvider, KnobSize, LevelMeterDb, SectionLabel,
};
use fts_dsp::note_sync::NoteValue;
use fts_plugin_core::prelude::*;

use delay_dsp::engine::DelayStyle;
use delay_dsp::modulation::WobbleShape;
use delay_dsp::SaturationType;

use crate::{DelayUiState, FtsDelayParams};

/// Root editor component.
#[component]
pub fn App() -> Element {
    let t = use_init_theme();
    let t = *t.read();

    let shared = use_context::<SharedState>();
    let ui: Arc<DelayUiState> = shared.get::<DelayUiState>().expect("DelayUiState missing");
    let params: Arc<FtsDelayParams> = ui.params.clone();
    let ctx = use_param_context();

    // Read metering values
    let input_db = ui.input_peak_db.load(Ordering::Relaxed);
    let output_db = ui.output_peak_db.load(Ordering::Relaxed);

    // State
    let style_idx = params.style.value() as usize;
    let style = DelayStyle::from_index(style_idx);
    let is_tape = style == DelayStyle::Tape;
    let is_rhythm = style == DelayStyle::Rhythm;
    let mode = params.stereo_mode.value() as i32;
    let head = params.head_mode.value() as i32;
    let link = params.link_lr.value() > 0.5;
    let sync = params.sync_enable.value() > 0.5;
    let diff_on = params.diff_enable.value() > 0.5;
    let duck_on = params.duck_enable.value() > 0.5;

    // Style setter
    let style_setter = |value: f32| {
        let ctx = ctx.clone();
        let p = params.clone();
        move |_: ()| {
            let max = (DelayStyle::COUNT - 1) as f32;
            ctx.begin_set_raw(p.style.as_ptr());
            ctx.set_normalized_raw(p.style.as_ptr(), value / max);
            ctx.end_set_raw(p.style.as_ptr());
        }
    };

    // Stereo mode setter
    let mode_setter = |value: f32| {
        let ctx = ctx.clone();
        let p = params.clone();
        move |_: ()| {
            ctx.begin_set_raw(p.stereo_mode.as_ptr());
            ctx.set_normalized_raw(p.stereo_mode.as_ptr(), value / 2.0);
            ctx.end_set_raw(p.stereo_mode.as_ptr());
        }
    };

    // Head mode setter
    let head_setter = |value: f32| {
        let ctx = ctx.clone();
        let p = params.clone();
        move |_: ()| {
            ctx.begin_set_raw(p.head_mode.as_ptr());
            ctx.set_normalized_raw(p.head_mode.as_ptr(), value / 3.0);
            ctx.end_set_raw(p.head_mode.as_ptr());
        }
    };

    // Saturation type setter
    let sat_type_idx = params.sat_type.value() as usize;
    let sat_setter = |value: f32| {
        let ctx = ctx.clone();
        let p = params.clone();
        move |_: ()| {
            let max = (SaturationType::COUNT - 1) as f32;
            ctx.begin_set_raw(p.sat_type.as_ptr());
            ctx.set_normalized_raw(p.sat_type.as_ptr(), value / max);
            ctx.end_set_raw(p.sat_type.as_ptr());
        }
    };

    // Wobble shape setter
    let wow_shape_idx = params.wow_shape.value() as usize;
    let wow_shape_setter = |value: f32| {
        let ctx = ctx.clone();
        let p = params.clone();
        move |_: ()| {
            let max = (WobbleShape::COUNT - 1) as f32;
            ctx.begin_set_raw(p.wow_shape.as_ptr());
            ctx.set_normalized_raw(p.wow_shape.as_ptr(), value / max);
            ctx.end_set_raw(p.wow_shape.as_ptr());
        }
    };

    // Note value setter for L/R
    let note_setter_l = |idx: i32| {
        let ctx = ctx.clone();
        let p = params.clone();
        move |_: ()| {
            ctx.begin_set(&p.note_l);
            ctx.set(&p.note_l, idx);
            ctx.end_set(&p.note_l);
        }
    };
    let note_setter_r = |idx: i32| {
        let ctx = ctx.clone();
        let p = params.clone();
        move |_: ()| {
            ctx.begin_set(&p.note_r);
            ctx.set(&p.note_r, idx);
            ctx.end_set(&p.note_r);
        }
    };

    let note_l_idx = params.note_l.value();
    let note_r_idx = params.note_r.value();

    // Common note values for the compact selector (8 most useful)
    let common_notes: &[(i32, &str)] = &[
        (NoteValue::Half.to_index() as i32, "1/2"),
        (NoteValue::DottedQuarter.to_index() as i32, "1/4."),
        (NoteValue::Quarter.to_index() as i32, "1/4"),
        (NoteValue::DottedEighth.to_index() as i32, "1/8."),
        (NoteValue::TripletQuarter.to_index() as i32, "1/4T"),
        (NoteValue::Eighth.to_index() as i32, "1/8"),
        (NoteValue::TripletEighth.to_index() as i32, "1/8T"),
        (NoteValue::Sixteenth.to_index() as i32, "1/16"),
    ];

    let border = t.border;
    let spacing_section = t.spacing_section;
    let spacing_card = t.spacing_card;
    let spacing_label = t.spacing_label;
    let spacing_control = t.spacing_control;
    let spacing_tight = t.spacing_tight;
    let font_size_title = t.font_size_title;
    let font_size_label = t.font_size_label;
    let letter_spacing_label = t.letter_spacing_label;
    let text_bright = t.text_bright;
    let text_dim = t.text_dim;
    let style_card = t.style_card();
    let root_style = t.root_style();
    let base_css = t.base_css();

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
                        "FTS DELAY"
                    }
                }
                div {
                    style: format!("font-size:{font_size_label}; color:{text_dim};"),
                    "FastTrackStudio"
                }
            }

            // ── Style selector ────────────────────────────────────
            div {
                style: format!(
                    "display:flex; gap:{spacing_label}; justify-content:center; flex-wrap:wrap;",
                ),
                for i in 0..DelayStyle::COUNT {
                    SegmentButton {
                        label: DelayStyle::from_index(i).label(),
                        selected: style_idx == i,
                        on_click: style_setter(i as f32),
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

                    // ── Row 1: Time & Feedback & Mix ─────────────────
                    div {
                        style: "display:flex; gap:20px; justify-content:center;",

                        // Time section
                        div {
                            style: format!(
                                "display:flex; flex-direction:column; gap:{spacing_section}; align-items:center;",
                            ),
                            div {
                                style: format!(
                                    "display:flex; gap:{spacing_section}; align-items:center;",
                                ),
                                SectionLabel { text: "Time" }
                                Toggle { param_ptr: params.sync_enable.as_ptr(), label: "Sync" }
                                Toggle { param_ptr: params.link_lr.as_ptr(), label: "Link" }
                            }

                            if sync {
                                // Note value selectors
                                div {
                                    style: format!(
                                        "display:flex; flex-direction:column; gap:{spacing_label}; align-items:center;",
                                    ),
                                    // L note selector
                                    div {
                                        style: format!(
                                            "display:flex; gap:{spacing_tight}; flex-wrap:wrap; justify-content:center;",
                                        ),
                                        for &(idx, label) in common_notes.iter() {
                                            SegmentButton {
                                                label: label,
                                                selected: note_l_idx == idx,
                                                on_click: note_setter_l(idx),
                                            }
                                        }
                                    }
                                    // R note selector (only when unlinked)
                                    if !link {
                                        div {
                                            style: format!(
                                                "display:flex; gap:{spacing_tight}; flex-wrap:wrap; justify-content:center; \
                                                 padding-top:{spacing_label}; border-top:1px solid {border};",
                                            ),
                                            span {
                                                style: format!(
                                                    "font-size:{font_size_label}; color:{text_dim}; margin-right:4px; align-self:center;",
                                                ),
                                                "R"
                                            }
                                            for &(idx, label) in common_notes.iter() {
                                                SegmentButton {
                                                    label: label,
                                                    selected: note_r_idx == idx,
                                                    on_click: note_setter_r(idx),
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                // Manual ms knobs
                                div {
                                    style: format!(
                                        "display:flex; gap:{spacing_control}; justify-content:center;",
                                    ),
                                    Knob { param_ptr: params.time_l.as_ptr(), size: KnobSize::Large }
                                    if !link {
                                        Knob { param_ptr: params.time_r.as_ptr(), size: KnobSize::Large }
                                    }
                                }
                            }
                        }

                        // Divider
                        div {
                            style: format!(
                                "width:1px; background:{border}; align-self:stretch;",
                            ),
                        }

                        ControlGroup {
                            label: "Feedback & Mix",
                            Knob { param_ptr: params.feedback.as_ptr(), size: KnobSize::Large }
                            Knob { param_ptr: params.mix.as_ptr(), size: KnobSize::Large }
                        }
                    }

                    // ── Row 2: Stereo mode + EQ + Drive ──────────────
                    div {
                        style: "display:flex; gap:20px; justify-content:center; align-items:flex-start;",

                        // Stereo
                        div {
                            style: format!(
                                "display:flex; flex-direction:column; gap:{spacing_section}; align-items:center;",
                            ),
                            SectionLabel { text: "Stereo" }
                            div {
                                style: format!("display:flex; gap:{spacing_label};"),
                                SegmentButton { label: "Stereo", selected: mode == 0, on_click: mode_setter(0.0) }
                                SegmentButton { label: "PingPong", selected: mode == 1, on_click: mode_setter(1.0) }
                                SegmentButton { label: "Mono", selected: mode == 2, on_click: mode_setter(2.0) }
                            }
                            div {
                                style: format!("display:flex; gap:{spacing_control};"),
                                Knob { param_ptr: params.width.as_ptr(), size: KnobSize::Small }
                                if mode == 1 {
                                    Knob { param_ptr: params.pp_feedback.as_ptr(), size: KnobSize::Small }
                                }
                            }
                        }

                        div {
                            style: format!(
                                "width:1px; background:{border}; align-self:stretch;",
                            ),
                        }

                        // Head Mode (RE-201 style, Tape only)
                        if is_tape {
                            div {
                                style: format!(
                                    "display:flex; flex-direction:column; gap:{spacing_section}; align-items:center;",
                                ),
                                SectionLabel { text: "Heads" }
                                div {
                                    style: format!("display:flex; gap:{spacing_label};"),
                                    SegmentButton { label: "1", selected: head == 0, on_click: head_setter(0.0) }
                                    SegmentButton { label: "2", selected: head == 1, on_click: head_setter(1.0) }
                                    SegmentButton { label: "3", selected: head == 2, on_click: head_setter(2.0) }
                                    SegmentButton { label: "4", selected: head == 3, on_click: head_setter(3.0) }
                                }
                            }

                            div {
                                style: format!(
                                    "width:1px; background:{border}; align-self:stretch;",
                                ),
                            }
                        }

                        // Feedback EQ
                        ControlGroup {
                            label: "Feedback EQ",
                            Knob { param_ptr: params.hicut.as_ptr(), size: KnobSize::Medium }
                            Knob { param_ptr: params.locut.as_ptr(), size: KnobSize::Medium }
                            Knob { param_ptr: params.decay_tilt.as_ptr(), size: KnobSize::Medium }
                        }

                        // Saturation (Tape only)
                        if is_tape {
                            div {
                                style: format!(
                                    "width:1px; background:{border}; align-self:stretch;",
                                ),
                            }
                            div {
                                style: format!(
                                    "display:flex; flex-direction:column; gap:{spacing_section}; align-items:center;",
                                ),
                                SectionLabel { text: "Saturation" }
                                div {
                                    style: format!("display:flex; gap:{spacing_tight}; flex-wrap:wrap; justify-content:center;"),
                                    for i in 0..SaturationType::COUNT {
                                        SegmentButton {
                                            label: SaturationType::from_index(i).label(),
                                            selected: sat_type_idx == i,
                                            on_click: sat_setter(i as f32),
                                        }
                                    }
                                }
                                Knob { param_ptr: params.drive.as_ptr(), size: KnobSize::Medium }
                            }
                        }
                    }

                    // ── Row 3: Tape Modulation (Tape only) ────────────
                    if is_tape {
                        div {
                            style: "display:flex; gap:20px; justify-content:center;",

                            div {
                                style: format!(
                                    "display:flex; flex-direction:column; gap:{spacing_section}; align-items:center;",
                                ),
                                SectionLabel { text: "Wow" }
                                div {
                                    style: format!("display:flex; gap:{spacing_tight}; flex-wrap:wrap; justify-content:center;"),
                                    for i in 0..WobbleShape::COUNT {
                                        SegmentButton {
                                            label: WobbleShape::from_index(i).label(),
                                            selected: wow_shape_idx == i,
                                            on_click: wow_shape_setter(i as f32),
                                        }
                                    }
                                }
                                div {
                                    style: format!("display:flex; gap:{spacing_control};"),
                                    Knob { param_ptr: params.wow_depth.as_ptr(), size: KnobSize::Medium }
                                    Knob { param_ptr: params.wow_rate.as_ptr(), size: KnobSize::Small }
                                    Knob { param_ptr: params.wow_drift.as_ptr(), size: KnobSize::Small }
                                    Knob { param_ptr: params.wow_phase_offset.as_ptr(), size: KnobSize::Small }
                                }
                            }

                            div {
                                style: format!(
                                    "width:1px; background:{border}; align-self:stretch;",
                                ),
                            }

                            ControlGroup {
                                label: "Flutter",
                                Knob { param_ptr: params.flutter_depth.as_ptr(), size: KnobSize::Medium }
                                Knob { param_ptr: params.flutter_rate.as_ptr(), size: KnobSize::Small }
                            }
                        }
                    }

                    // ── Row 3b: Rhythm Taps (Rhythm style only) ──────
                    if is_rhythm {
                        ControlGroup {
                            label: "Rhythm Taps",
                            Knob { param_ptr: params.rhythm_tap_1.as_ptr(), size: KnobSize::Small }
                            Knob { param_ptr: params.rhythm_tap_2.as_ptr(), size: KnobSize::Small }
                            Knob { param_ptr: params.rhythm_tap_3.as_ptr(), size: KnobSize::Small }
                            Knob { param_ptr: params.rhythm_tap_4.as_ptr(), size: KnobSize::Small }
                            Knob { param_ptr: params.rhythm_tap_5.as_ptr(), size: KnobSize::Small }
                            Knob { param_ptr: params.rhythm_tap_6.as_ptr(), size: KnobSize::Small }
                            Knob { param_ptr: params.rhythm_tap_7.as_ptr(), size: KnobSize::Small }
                            Knob { param_ptr: params.rhythm_tap_8.as_ptr(), size: KnobSize::Small }
                        }
                    }

                    // ── Row 4: Accent / Groove / Feel ──────────────────
                    div {
                        style: "display:flex; gap:20px; justify-content:center;",

                        ControlGroup {
                            label: "Rhythm",
                            Knob { param_ptr: params.accent.as_ptr(), size: KnobSize::Small }
                            Knob { param_ptr: params.groove.as_ptr(), size: KnobSize::Small }
                            Knob { param_ptr: params.feel.as_ptr(), size: KnobSize::Small }
                        }

                        div {
                            style: format!(
                                "width:1px; background:{border}; align-self:stretch;",
                            ),
                        }

                        ControlGroup {
                            label: "Options",
                            Toggle { param_ptr: params.prime_numbers.as_ptr(), label: "Prime" }
                            Knob { param_ptr: params.lr_offset.as_ptr(), size: KnobSize::Small }
                        }

                        div {
                            style: format!(
                                "width:1px; background:{border}; align-self:stretch;",
                            ),
                        }

                        ControlGroup {
                            label: "Levels",
                            Knob { param_ptr: params.input_level.as_ptr(), size: KnobSize::Small }
                            Knob { param_ptr: params.output_level.as_ptr(), size: KnobSize::Small }
                        }
                    }

                    // ── Row 5: Diffusion + Ducking ───────────────────
                    div {
                        style: "display:flex; gap:20px; justify-content:center;",

                        // Diffusion
                        div {
                            style: format!(
                                "display:flex; flex-direction:column; gap:{spacing_section}; align-items:center;",
                            ),
                            div {
                                style: format!(
                                    "display:flex; gap:{spacing_section}; align-items:center;",
                                ),
                                SectionLabel { text: "Diffusion" }
                                Toggle { param_ptr: params.diff_enable.as_ptr(), label: "" }
                            }
                            if diff_on {
                                div {
                                    style: format!("display:flex; gap:{spacing_control}; align-items:center;"),
                                    Knob { param_ptr: params.diff_size.as_ptr(), size: KnobSize::Small }
                                    Knob { param_ptr: params.diff_smear.as_ptr(), size: KnobSize::Small }
                                    Toggle { param_ptr: params.diff_loop.as_ptr(), label: "Loop" }
                                }
                            }
                        }

                        div {
                            style: format!(
                                "width:1px; background:{border}; align-self:stretch;",
                            ),
                        }

                        // Ducking
                        div {
                            style: format!(
                                "display:flex; flex-direction:column; gap:{spacing_section}; align-items:center;",
                            ),
                            div {
                                style: format!(
                                    "display:flex; gap:{spacing_section}; align-items:center;",
                                ),
                                SectionLabel { text: "Ducking" }
                                Toggle { param_ptr: params.duck_enable.as_ptr(), label: "" }
                            }
                            if duck_on {
                                div {
                                    style: format!("display:flex; gap:{spacing_control};"),
                                    Knob { param_ptr: params.duck_amount.as_ptr(), size: KnobSize::Small }
                                    Knob { param_ptr: params.duck_threshold.as_ptr(), size: KnobSize::Small }
                                    Knob { param_ptr: params.duck_attack.as_ptr(), size: KnobSize::Small }
                                    Knob { param_ptr: params.duck_release.as_ptr(), size: KnobSize::Small }
                                }
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
