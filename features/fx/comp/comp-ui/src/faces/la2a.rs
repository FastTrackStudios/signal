//! The LA-2A face — an optical leveling amplifier.
//!
//! Two knobs and a switch, which is the whole point of the unit: PEAK
//! REDUCTION decides how hard it works, GAIN decides how loud it comes back,
//! and the meter watches the reduction. PEAK REDUCTION is the compound
//! control — one knob writing threshold, ratio, knee, range and drive on
//! linked curves — so it is the clearest case of a hardware control that is
//! not a parameter.

use std::sync::atomic::Ordering;

use audiocore_core::prelude::*;

use crate::faces::use_face_context;
use crate::hardware::knob::{HardwareKnob, KnobStyle};
use crate::hardware::knob_svg::{linear_scale_label, scale_ring};
use crate::hardware::panel::{panel_scale, Panel, PanelSlot, Silkscreen};
use crate::hardware::switches::ToggleSwitch;
use crate::hardware::vu::{VuFace, VuMeter, VuMode};

/// Panel drawing size. Everything below is placed in these coordinates.
const W: f64 = 900.0;
const H: f64 = 300.0;

/// Warm cream paint, lit from above.
const PAINT: &str = "linear-gradient(178deg, #efe6cf 0%, #e4d8bb 46%, #d6c9a9 100%)";
const INK: &str = "#3a3228";

/// Vertical centre line the controls sit on.
const ROW_Y: f64 = 152.0;
/// Where each control's legend is silkscreened.
const LEGEND_Y: f64 = 236.0;

#[component]
pub fn La2aFace(
    /// The shell's redraw tick — see [`crate::faces::Face`]. Not read; its
    /// only job is to change, so this face re-renders against fresh param and
    /// meter values instead of being memoized away.
    frame: u64,
) -> Element {
    let _ = frame;
    let face = use_face_context(&comp_profiles::LA2A);
    let gr_db = face.ui.gain_reduction_db.load(Ordering::Relaxed);
    let scale = panel_scale(W, H, crate::control_view::HEADER_H);

    let peak_reduction = face.handle("peak_reduction");
    let gain = face.handle("gain");
    let mode = face.handle("mode");

    // Both knobs are silkscreened 0–10, like the unit — the numbers are the
    // operator's reference, not the engine's dB.
    let ring = scale_ring(6, 1, linear_scale_label(0.0, 10.0));

    rsx! {
        Panel {
            design_w: W,
            design_h: H,
            scale,
            background: PAINT.to_string(),
            chrome: "#c9c2b0".to_string(),

            // ── Meter ────────────────────────────────────────────────────
            PanelSlot { scale, x: 218.0, y: 140.0, w: 250.0, h: 170.0,
                VuMeter {
                    scale,
                    width: 240.0,
                    face: VuFace::Amber,
                    mode: VuMode::GainReduction,
                    value_db: gr_db,
                    legend: "Gain Reduction".to_string(),
                }
            }

            // ── Controls ─────────────────────────────────────────────────
            PanelSlot { scale, x: 470.0, y: ROW_Y, w: 160.0, h: 160.0,
                HardwareKnob {
                    handle: gain,
                    testid: "gain".to_string(),
                    scale,
                    diameter: 66.0,
                    style: KnobStyle::Bakelite,
                    ink: INK.to_string(),
                    marks: ring.clone(),
                }
            }
            Silkscreen { scale, x: 470.0, y: LEGEND_Y, text: "Gain".to_string(), color: INK.to_string() }

            PanelSlot { scale, x: 650.0, y: ROW_Y, w: 160.0, h: 160.0,
                HardwareKnob {
                    handle: peak_reduction,
                    testid: "peak-reduction".to_string(),
                    scale,
                    diameter: 66.0,
                    style: KnobStyle::Bakelite,
                    ink: INK.to_string(),
                    marks: ring,
                }
            }
            Silkscreen { scale, x: 650.0, y: LEGEND_Y, text: "Peak Reduction".to_string(), color: INK.to_string() }

            PanelSlot { scale, x: 800.0, y: ROW_Y, w: 120.0, h: 110.0,
                ToggleSwitch {
                    handle: mode,
                    testid: "mode".to_string(),
                    scale,
                    labels: ["Comp".to_string(), "Limit".to_string()],
                    ink: INK.to_string(),
                }
            }
            Silkscreen { scale, x: 800.0, y: LEGEND_Y, text: "Mode".to_string(), color: INK.to_string() }

            // ── Panel legends ────────────────────────────────────────────
            Silkscreen {
                scale, x: 640.0, y: 62.0,
                text: "Leveling Amplifier".to_string(),
                width: 320.0, size: 15.0, tracking: 0.22,
                color: INK.to_string(),
            }
            Silkscreen {
                scale, x: 640.0, y: 88.0,
                text: "FTS Comp · Optical".to_string(),
                width: 320.0, size: 9.0, weight: 600,
                color: "#6b6053".to_string(),
            }
        }
    }
}
