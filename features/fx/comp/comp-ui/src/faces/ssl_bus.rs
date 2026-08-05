//! The SSL-style bus compressor face — a console centre-section strip.
//!
//! Grey panel, metal knobs, and the detail that matters for how it is used:
//! RATIO, ATTACK and RELEASE are rotary *switches*, not pots, so their rings
//! print the unit's detent legends (including AUTO at the release stop)
//! rather than a continuous scale.

use std::sync::atomic::Ordering;

use audiocore_core::prelude::*;

use crate::faces::use_face_context;
use crate::hardware::knob::{HardwareKnob, KnobStyle};
use crate::hardware::knob_svg::{detent_ring, linear_scale_label, scale_ring};
use crate::hardware::panel::{panel_scale, Panel, PanelSlot, Silkscreen};
use crate::hardware::vu::{VuFace, VuMeter, VuMode};

const W: f64 = 900.0;
const H: f64 = 300.0;

const PAINT: &str = "linear-gradient(178deg, #4a4d52 0%, #34373b 50%, #26282c 100%)";
const INK: &str = "#e6e8ea";
const DIM_INK: &str = "#9aa0a6";

const ROW_Y: f64 = 150.0;
const LEGEND_Y: f64 = 232.0;

#[component]
pub fn SslBusFace(
    /// The shell's redraw tick — see [`crate::faces::Face`]. Not read; its
    /// only job is to change, so this face re-renders against fresh param and
    /// meter values instead of being memoized away.
    frame: u64,
) -> Element {
    let _ = frame;
    let face = use_face_context(&comp_profiles::SSL_BUS);
    let gr_db = face.ui.gain_reduction_db.load(Ordering::Relaxed);
    let scale = panel_scale(W, H, crate::control_view::HEADER_H);

    let threshold = face.handle("threshold");
    let ratio = face.handle("ratio");
    let attack = face.handle("attack");
    let release = face.handle("release");
    let makeup = face.handle("makeup");
    let mix = face.handle("mix");

    let threshold_ring = scale_ring(7, 1, linear_scale_label(-30.0, 0.0));
    let makeup_ring = scale_ring(7, 1, linear_scale_label(0.0, 18.0));
    let mix_ring = scale_ring(6, 1, linear_scale_label(0.0, 100.0));
    let ratio_ring = detent_ring(&["2", "4", "10"]);
    let attack_ring = detent_ring(&["0.1", "0.3", "1", "3", "10", "30"]);
    let release_ring = detent_ring(&["0.1", "0.3", "0.6", "1.2", "A"]);

    rsx! {
        Panel {
            design_w: W,
            design_h: H,
            scale,
            background: PAINT.to_string(),
            chrome: "#a9adb2".to_string(),

            PanelSlot { scale, x: 180.0, y: 140.0, w: 230.0, h: 170.0,
                VuMeter {
                    scale,
                    width: 218.0,
                    face: VuFace::Blue,
                    mode: VuMode::GainReduction,
                    value_db: gr_db,
                    legend: "Compression".to_string(),
                }
            }

            PanelSlot { scale, x: 360.0, y: ROW_Y, w: 120.0, h: 140.0,
                HardwareKnob {
                    handle: threshold, testid: "threshold".to_string(), scale,
                    diameter: 54.0, style: KnobStyle::Metal,
                    ink: INK.to_string(), marks: threshold_ring,
                }
            }
            Silkscreen { scale, x: 360.0, y: LEGEND_Y, text: "Threshold".to_string(), color: INK.to_string() }

            PanelSlot { scale, x: 470.0, y: ROW_Y, w: 120.0, h: 140.0,
                HardwareKnob {
                    handle: ratio, testid: "ratio".to_string(), scale,
                    diameter: 54.0, style: KnobStyle::Metal,
                    ink: INK.to_string(), marks: ratio_ring,
                }
            }
            Silkscreen { scale, x: 470.0, y: LEGEND_Y, text: "Ratio".to_string(), color: INK.to_string() }

            PanelSlot { scale, x: 580.0, y: ROW_Y, w: 120.0, h: 140.0,
                HardwareKnob {
                    handle: attack, testid: "attack".to_string(), scale,
                    diameter: 54.0, style: KnobStyle::Metal,
                    ink: INK.to_string(), marks: attack_ring,
                }
            }
            Silkscreen { scale, x: 580.0, y: LEGEND_Y, text: "Attack ms".to_string(), color: INK.to_string() }

            PanelSlot { scale, x: 690.0, y: ROW_Y, w: 120.0, h: 140.0,
                HardwareKnob {
                    handle: release, testid: "release".to_string(), scale,
                    diameter: 54.0, style: KnobStyle::Metal,
                    ink: INK.to_string(), marks: release_ring,
                }
            }
            Silkscreen { scale, x: 690.0, y: LEGEND_Y, text: "Release s".to_string(), color: INK.to_string() }

            PanelSlot { scale, x: 776.0, y: ROW_Y, w: 110.0, h: 140.0,
                HardwareKnob {
                    handle: makeup, testid: "makeup".to_string(), scale,
                    diameter: 54.0, style: KnobStyle::Metal,
                    ink: INK.to_string(), marks: makeup_ring,
                }
            }
            Silkscreen { scale, x: 776.0, y: LEGEND_Y, text: "Makeup".to_string(), color: INK.to_string() }

            PanelSlot { scale, x: 836.0, y: 82.0, w: 80.0, h: 110.0,
                HardwareKnob {
                    handle: mix, testid: "mix".to_string(), scale,
                    diameter: 36.0, style: KnobStyle::Metal,
                    ink: INK.to_string(), marks: mix_ring,
                }
            }
            Silkscreen { scale, x: 836.0, y: 130.0, width: 80.0, size: 9.0,
                text: "Mix".to_string(), color: DIM_INK.to_string() }

            Silkscreen {
                scale, x: 560.0, y: 56.0,
                text: "Bus Compressor".to_string(),
                width: 340.0, size: 14.0, tracking: 0.22,
                color: INK.to_string(),
            }
            Silkscreen {
                scale, x: 560.0, y: 80.0,
                text: "FTS Comp · VCA".to_string(),
                width: 340.0, size: 9.0, weight: 600,
                color: DIM_INK.to_string(),
            }
        }
    }
}
