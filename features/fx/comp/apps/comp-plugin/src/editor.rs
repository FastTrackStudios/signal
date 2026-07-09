//! Compressor editor — Dioxus GUI root component.
//!
//! Layout:
//!   - Waveform fills the window (background layer).
//!   - Overlaid at the bottom: two rows of knobs.
//!       Row 1 (large): Threshold | Ratio | Attack | Release
//!       Row 2 (small): Knee      | Range | Lookahead | Hold
//!     Each small knob sits in a 64 px-wide column so their centres
//!     align exactly with the large knobs above.
//!   - Bottom strip (card bg): I/O, Mix, and character controls.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use audio_gui::controls::knob::Knob;
use audio_gui::controls::toggle::Toggle;
use audio_gui::prelude::{
    use_init_theme, ControlGroup, Divider, DragProvider, KnobSize, PeakWaveform,
};
use comp_ui::{profile_skin, ProfileSkin, ProfileSkinGroup};
use fts_plugin_core::prelude::*;

use crate::{CompUiState, WAVEFORM_LEN};

/// Root editor component.
#[component]
pub fn App() -> Element {
    let t = use_init_theme();
    let t = *t.read();

    // In plugin mode the Vello Background layer paints the dark background.
    // In dx-serve / standalone mode there is no OverlayRegistry so we fall
    // back to a CSS background on the root div.
    let in_plugin_mode = try_consume_context::<OverlayRegistry>().is_some();
    let root_bg = if in_plugin_mode { "transparent" } else { t.bg };

    let shared = use_context::<SharedState>();
    let ui = shared.get::<CompUiState>().expect("CompUiState missing");
    let params = &ui.params;

    let input_db = ui.input_peak_db.load(Ordering::Relaxed);
    let threshold = params.threshold_db.value();
    let ratio = params.ratio.value();
    let knee = params.knee_db.value();
    let profile = params.profile.value();
    let skin = profile_skin(profile_id(profile));

    let pos = ui.waveform_pos.load(Ordering::Relaxed) as usize % WAVEFORM_LEN;
    let scroll_phase = ui.waveform_phase.load(Ordering::Relaxed);
    let mut waveform_in = Vec::with_capacity(WAVEFORM_LEN);
    let mut waveform_gr = Vec::with_capacity(WAVEFORM_LEN);
    for i in 0..WAVEFORM_LEN {
        let idx = (pos + i) % WAVEFORM_LEN;
        waveform_in.push(ui.waveform_input[idx].load(Ordering::Relaxed));
        waveform_gr.push(ui.waveform_gr[idx].load(Ordering::Relaxed));
    }

    let input_level = if input_db > -60.0 {
        Some(input_db)
    } else {
        None
    };

    rsx! {
        document::Style { {format!(
            "*, *::before, *::after {{ box-sizing: border-box; margin: 0; padding: 0; }} \
             html, body {{ width: 100%; height: 100%; overflow: hidden; \
             background: transparent; color: {text}; \
             font-family: {font}; font-size: 13px; }}",
            text = skin.text,
            font = t.font_family,
        )} }

        DragProvider {
            div {
                style: format!(
                    "width:100vw; height:100vh; display:flex; flex-direction:column; \
                     color:{text}; font-family:{font}; font-size:13px; \
                     user-select:none; position:relative; background:{bg};",
                    text = skin.text,
                    font = t.font_family,
                    bg = root_bg,
                ),

                // ── Main area ─────────────────────────────────────────
                div {
                    style: "position:relative; flex:1; min-height:0;",

                    PeakWaveform {
                        levels: waveform_in,
                        gr_levels: waveform_gr,
                        threshold_db: Some(threshold),
                        ratio: Some(ratio),
                        knee_db: knee,
                        input_level_db: input_level,
                        fill: true,
                        scroll_phase: scroll_phase,
                        style: "border-radius:0; border:none;".to_string(),
                    }

                    // ── Knob overlay ─────────────────────────────────
                    div {
                        style: format!(
                            "position:absolute; bottom:0; left:0; right:0; \
                             display:flex; flex-direction:column; align-items:center; \
                             padding:6px 0 12px 0; gap:4px; background:{};",
                            skin.overlay_gradient,
                        ),

                        // Row 1 — large primary knobs
                        div {
                            style: "display:flex; gap:20px;",
                            Knob { param_ptr: params.threshold_db.as_ptr(), size: KnobSize::Large }
                            Knob { param_ptr: params.ratio.as_ptr(),        size: KnobSize::Large }
                            Knob { param_ptr: params.attack_ms.as_ptr(),    size: KnobSize::Large }
                            Knob { param_ptr: params.release_ms.as_ptr(),   size: KnobSize::Large }
                        }

                        // Row 2 — small secondary knobs, one per column.
                        // Each cell is 64 px wide (= large knob outer diameter) so the
                        // centres of the small and large knobs above are identical.
                        div {
                            style: "display:flex; gap:20px;",
                            div { style: "width:64px; display:flex; justify-content:center;",
                                Knob { param_ptr: params.knee_db.as_ptr(),      size: KnobSize::Small }
                            }
                            div { style: "width:64px; display:flex; justify-content:center;",
                                Knob { param_ptr: params.range_db.as_ptr(),     size: KnobSize::Small }
                            }
                            div { style: "width:64px; display:flex; justify-content:center;",
                                Knob { param_ptr: params.lookahead_ms.as_ptr(), size: KnobSize::Small }
                            }
                            div { style: "width:64px; display:flex; justify-content:center;",
                                Knob { param_ptr: params.hold_ms.as_ptr(),      size: KnobSize::Small }
                            }
                        }
                    }
                }

                // ── Bottom strip — I/O, Mix, Character ────────────────
                div {
                    style: format!(
                        "display:flex; gap:0; align-items:stretch; flex-shrink:0; \
                         border-top:1px solid {}; background:{}; box-shadow:inset 0 1px 0 {};",
                        skin.border,
                        skin.strip_bg,
                        skin.highlight,
                    ),

                    {profile_strip(params, skin)}
                }
            }
        }
    }
}

fn profile_strip(params: &Arc<crate::FtsCompParams>, skin: ProfileSkin) -> Element {
    rsx! {
        div {
            style: format!(
                "width:4px; align-self:stretch; background:{}; flex-shrink:0;",
                skin.accent,
            )
        }

        ControlGroup {
            label: "Profile",
            Knob { param_ptr: params.profile.as_ptr(), size: KnobSize::Small }
        }

        Divider {}

        for group in skin.strip_groups {
            {skin_group(params, skin, *group)}
            Divider {}
        }
    }
}

fn skin_group(
    params: &Arc<crate::FtsCompParams>,
    skin: ProfileSkin,
    group: ProfileSkinGroup,
) -> Element {
    let label = if group.label == "macro" {
        skin.macro_group_label
    } else {
        group.label
    };

    rsx! {
        ControlGroup {
            label,
            for control in group.controls {
                {skin_control(params, control)}
            }
        }
    }
}

fn skin_control(params: &Arc<crate::FtsCompParams>, control: &str) -> Element {
    match control {
        "auto_makeup" => rsx! {
            Toggle { param_ptr: params.auto_makeup.as_ptr(), label: "Auto" }
        },
        "input_gain_db" => rsx! {
            Knob { param_ptr: params.input_gain_db.as_ptr(), size: KnobSize::Small }
        },
        "output_gain_db" => rsx! {
            Knob { param_ptr: params.output_gain_db.as_ptr(), size: KnobSize::Small }
        },
        "fold" => rsx! {
            Knob { param_ptr: params.fold.as_ptr(), size: KnobSize::Small }
        },
        "multiband_amount" => rsx! {
            Knob { param_ptr: params.multiband_amount.as_ptr(), size: KnobSize::Small }
        },
        "channel_link" => rsx! {
            Knob { param_ptr: params.channel_link.as_ptr(), size: KnobSize::Small }
        },
        "detector_rms_mix" => rsx! {
            Knob { param_ptr: params.detector_rms_mix.as_ptr(), size: KnobSize::Small }
        },
        "feedback" => rsx! {
            Knob { param_ptr: params.feedback.as_ptr(), size: KnobSize::Small }
        },
        "ceiling" => rsx! {
            Knob { param_ptr: params.ceiling.as_ptr(), size: KnobSize::Small }
        },
        "drive" => rsx! {
            Knob { param_ptr: params.drive.as_ptr(), size: KnobSize::Small }
        },
        "character_mode" => rsx! {
            Knob { param_ptr: params.character_mode.as_ptr(), size: KnobSize::Small }
        },
        "expander_threshold_db" => rsx! {
            Knob { param_ptr: params.expander_threshold_db.as_ptr(), size: KnobSize::Small }
        },
        "expander_ratio" => rsx! {
            Knob { param_ptr: params.expander_ratio.as_ptr(), size: KnobSize::Small }
        },
        "upward_threshold_db" => rsx! {
            Knob { param_ptr: params.upward_threshold_db.as_ptr(), size: KnobSize::Small }
        },
        "upward_ratio" => rsx! {
            Knob { param_ptr: params.upward_ratio.as_ptr(), size: KnobSize::Small }
        },
        "sidechain_freq" => rsx! {
            Knob { param_ptr: params.sidechain_freq.as_ptr(), size: KnobSize::Small }
        },
        "sidechain_lowpass_freq" => rsx! {
            Knob { param_ptr: params.sidechain_lowpass_freq.as_ptr(), size: KnobSize::Small }
        },
        "inertia" => rsx! {
            Knob { param_ptr: params.inertia.as_ptr(), size: KnobSize::Small }
        },
        "inertia_decay" => rsx! {
            Knob { param_ptr: params.inertia_decay.as_ptr(), size: KnobSize::Small }
        },
        "profile_drive" => rsx! {
            Knob { param_ptr: params.profile_drive.as_ptr(), size: KnobSize::Small }
        },
        "profile_output" => rsx! {
            Knob { param_ptr: params.profile_output.as_ptr(), size: KnobSize::Small }
        },
        "attack_ms" => rsx! {
            Knob { param_ptr: params.attack_ms.as_ptr(), size: KnobSize::Small }
        },
        "release_ms" => rsx! {
            Knob { param_ptr: params.release_ms.as_ptr(), size: KnobSize::Small }
        },
        "lookahead_ms" => rsx! {
            Knob { param_ptr: params.lookahead_ms.as_ptr(), size: KnobSize::Small }
        },
        _ => rsx! {},
    }
}

fn profile_id(profile: i32) -> &'static str {
    match profile {
        1 => "la2a",
        2 => "ssl_bus",
        3 => "urei_1176",
        _ => "control",
    }
}
