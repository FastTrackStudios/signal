//! The 1176 face — the black FET panel.
//!
//! Four knobs, four ratio buttons and a blue meter. The two things that make
//! it behave like the unit rather than look like it: INPUT is a compound
//! control (it drives *into* compression — input gain, threshold and drive
//! together, which is why the 1176 has no threshold knob), and ATTACK and
//! RELEASE run backwards, fastest at the clockwise stop.

use std::sync::atomic::Ordering;

use audiocore_core::prelude::*;

use crate::faces::use_face_context;
use crate::hardware::knob::{HardwareKnob, KnobStyle};
use crate::hardware::knob_svg::{linear_scale_label, scale_ring};
use crate::hardware::panel::{panel_scale, Panel, PanelSlot, Silkscreen};
use crate::hardware::switches::RatioButtons;
use crate::hardware::vu::{VuFace, VuMeter, VuMode};

const W: f64 = 900.0;
const H: f64 = 300.0;

/// Black wrinkle paint with the usual top-lit sheen.
const PAINT: &str = "linear-gradient(178deg, #2b2b2f 0%, #1a1a1e 52%, #101013 100%)";
const INK: &str = "#ded7c9";
const DIM_INK: &str = "#9a9384";

const ROW_Y: f64 = 152.0;
const LEGEND_Y: f64 = 236.0;

#[component]
pub fn Urei1176Face(
    /// The shell's redraw tick — see [`crate::faces::Face`]. Not read; its
    /// only job is to change, so this face re-renders against fresh param and
    /// meter values instead of being memoized away.
    frame: u64,
) -> Element {
    let _ = frame;
    let face = use_face_context(&comp_profiles::UREI_1176);
    let gr_db = face.ui.gain_reduction_db.load(Ordering::Relaxed);
    let scale = panel_scale(W, H, crate::control_view::RAIL_W);

    let input = face.handle("input");
    let output = face.handle("output");
    let attack = face.handle("attack");
    let release = face.handle("release");
    let ratio = face.handle("ratio");

    // INPUT is printed in dB of drive into the compressor; OUTPUT in makeup
    // dB. ATTACK and RELEASE take the unit's bare 1–7 scale, fastest at 7.
    let input_ring = scale_ring(5, 1, linear_scale_label(-48.0, 12.0));
    let output_ring = scale_ring(5, 1, linear_scale_label(-12.0, 24.0));
    let time_ring = scale_ring(7, 1, linear_scale_label(1.0, 7.0));

    rsx! {
        Panel {
            design_w: W,
            design_h: H,
            scale,
            background: PAINT.to_string(),
            chrome: "#8d8a84".to_string(),

            // ── Meter ────────────────────────────────────────────────────
            PanelSlot { scale, x: 186.0, y: 140.0, w: 240.0, h: 170.0,
                VuMeter {
                    scale,
                    width: 228.0,
                    face: VuFace::Blue,
                    mode: VuMode::GainReduction,
                    value_db: gr_db,
                    legend: "Gain Reduction".to_string(),
                }
            }

            // ── Ratio buttons ────────────────────────────────────────────
            PanelSlot { scale, x: 352.0, y: ROW_Y, w: 90.0, h: 170.0,
                RatioButtons {
                    handle: ratio,
                    testid: "ratio".to_string(),
                    scale,
                    labels: vec![
                        "4".to_string(),
                        "8".to_string(),
                        "12".to_string(),
                        "20".to_string(),
                        "All".to_string(),
                    ],
                    ink: INK.to_string(),
                }
            }
            Silkscreen { scale, x: 352.0, y: LEGEND_Y, text: "Ratio".to_string(), color: INK.to_string() }

            // ── Knobs ────────────────────────────────────────────────────
            PanelSlot { scale, x: 470.0, y: ROW_Y, w: 130.0, h: 140.0,
                HardwareKnob {
                    handle: input,
                    testid: "input".to_string(),
                    scale,
                    diameter: 58.0,
                    style: KnobStyle::Bakelite,
                    ink: INK.to_string(),
                    marks: input_ring,
                }
            }
            Silkscreen { scale, x: 470.0, y: LEGEND_Y, text: "Input".to_string(), color: INK.to_string() }

            PanelSlot { scale, x: 583.0, y: ROW_Y, w: 130.0, h: 140.0,
                HardwareKnob {
                    handle: output,
                    testid: "output".to_string(),
                    scale,
                    diameter: 58.0,
                    style: KnobStyle::Bakelite,
                    ink: INK.to_string(),
                    marks: output_ring,
                }
            }
            Silkscreen { scale, x: 583.0, y: LEGEND_Y, text: "Output".to_string(), color: INK.to_string() }

            PanelSlot { scale, x: 696.0, y: ROW_Y, w: 130.0, h: 140.0,
                HardwareKnob {
                    handle: attack,
                    testid: "attack".to_string(),
                    scale,
                    diameter: 58.0,
                    style: KnobStyle::Bakelite,
                    ink: INK.to_string(),
                    marks: time_ring.clone(),
                }
            }
            Silkscreen { scale, x: 696.0, y: LEGEND_Y, text: "Attack".to_string(), color: INK.to_string() }

            PanelSlot { scale, x: 809.0, y: ROW_Y, w: 130.0, h: 140.0,
                HardwareKnob {
                    handle: release,
                    testid: "release".to_string(),
                    scale,
                    diameter: 58.0,
                    style: KnobStyle::Bakelite,
                    ink: INK.to_string(),
                    marks: time_ring,
                }
            }
            Silkscreen { scale, x: 809.0, y: LEGEND_Y, text: "Release".to_string(), color: INK.to_string() }

            // ── Panel legends ────────────────────────────────────────────
            Silkscreen {
                scale, x: 600.0, y: 60.0,
                text: "Peak Limiter".to_string(),
                width: 320.0, size: 14.0, tracking: 0.24,
                color: INK.to_string(),
            }
            Silkscreen {
                scale, x: 600.0, y: 84.0,
                text: "FTS Comp · FET".to_string(),
                width: 320.0, size: 9.0, weight: 600,
                color: DIM_INK.to_string(),
            }
        }
    }
}
