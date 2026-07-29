//! FTS Meter editor — vello painter panels + numeric readouts.
//!
//! Layout is fixed to `EDITOR_W × EDITOR_H`; each `meter-ui` painter is
//! registered as a full-window background overlay and told its panel
//! rect through its own config (the painters take physical-pixel
//! rects). Numeric readouts poll the shared meter state at ~10 Hz.

use dioxus::prelude::*;
use nice_plug_dioxus::prelude::*;
use std::sync::Arc;

use meter_ui::goniometer_painter::{GoniometerConfig, GoniometerPainter};
use meter_ui::lufs_painter::{LufsConfig, LufsPainter};
use meter_ui::phase_painter::{PhaseConfig, PhasePainter};
use meter_ui::spectrum_painter::{SpectrumConfig, SpectrumPainter};

use crate::{MeterShared, EDITOR_H, EDITOR_W};

// Panel rects (CSS px in the fixed window).
const PAD: f64 = 12.0;
const SPECTRUM: (f64, f64, f64, f64) = (PAD, PAD, 640.0, 300.0);
const GONIO: (f64, f64, f64, f64) = (PAD, 330.0, 300.0, 186.0);
const PHASE: (f64, f64, f64, f64) = (330.0, 330.0, 322.0, 40.0);
const LUFS: (f64, f64, f64, f64) = (668.0, PAD, 240.0, 504.0);

pub fn App() -> Element {
    let shared = use_context::<SharedState>();
    let state: Arc<MeterShared> = shared
        .get::<MeterShared>()
        .expect("MeterShared in editor context");

    // Painters: one background overlay per panel; each paints
    // element-locally, positioned by set_rect.
    {
        let s = state.clone();
        let h = use_scene_overlay_background(move || {
            SpectrumPainter::new(
                s.spectrum.clone(),
                Arc::new(parking_lot::RwLock::new(SpectrumConfig::default())),
            )
        });
        h.set_rect(SPECTRUM.0, SPECTRUM.1, SPECTRUM.2, SPECTRUM.3);
    }
    {
        let s = state.clone();
        let h = use_scene_overlay_background(move || {
            GoniometerPainter::new(s.phase.clone(), GoniometerConfig::default())
        });
        h.set_rect(GONIO.0, GONIO.1, GONIO.2, GONIO.3);
    }
    {
        let s = state.clone();
        let h = use_scene_overlay_background(move || {
            PhasePainter::new(s.phase.clone(), PhaseConfig::default())
        });
        h.set_rect(PHASE.0, PHASE.1, PHASE.2, PHASE.3);
    }
    {
        let s = state.clone();
        let h = use_scene_overlay_background(move || {
            LufsPainter::new(s.lufs.clone(), LufsConfig::default())
        });
        h.set_rect(LUFS.0, LUFS.1, LUFS.2, LUFS.3);
    }

    // Redraw tick — the house idiom (comp-ui/eq-ui): a thread pokes
    // schedule_update at ~10 Hz, and the frame counter below dirties
    // the DOM so blitz keeps redrawing (painters + readouts refresh).
    let mut app_tick: Signal<u64> = use_signal(|| 0);
    use_hook(|| {
        let updater = dioxus_core::schedule_update();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            updater();
        });
    });
    app_tick += 1;
    let frame_counter = *app_tick.read();

    let momentary = *state.lufs.momentary_lufs.read();
    let short_term = *state.lufs.short_term_lufs.read();
    let integrated = *state.lufs.integrated_lufs.read();
    let range = *state.lufs.loudness_range.read();
    let true_peak = (*state.lufs.true_peak_l.read()).max(*state.lufs.true_peak_r.read());
    let correlation = *state.phase.correlation.read();

    let fmt_lufs = |v: f32| {
        if v.is_finite() {
            format!("{v:.1}")
        } else {
            "−∞".to_string()
        }
    };

    rsx! {
        div {
            style: "position: relative; width: {EDITOR_W}px; height: {EDITOR_H}px; background: #0b0b0e; color: #cbd5e1; font-family: sans-serif; font-size: 12px;",
            "data-frame": "{frame_counter}",
            // Panel captions + numeric readouts (painters draw beneath).
            div {
                style: "position: absolute; left: {SPECTRUM.0}px; top: {SPECTRUM.1 + SPECTRUM.3 + 2.0}px; color: #64748b;",
                "SPECTRUM"
            }
            div {
                style: "position: absolute; left: {GONIO.0}px; top: {GONIO.1 - 16.0}px; color: #64748b;",
                "STEREO FIELD"
            }
            div {
                style: "position: absolute; left: {PHASE.0}px; top: {PHASE.1 - 16.0}px; color: #64748b;",
                "CORRELATION  {correlation:.2}"
            }
            div {
                style: "position: absolute; left: {LUFS.0}px; top: {LUFS.1 - 0.0}px; width: {LUFS.2}px; text-align: right; color: #64748b;",
                "LOUDNESS"
            }
            // LUFS numeric block (right column, under the caption).
            div {
                style: "position: absolute; left: {PHASE.0}px; top: {PHASE.1 + PHASE.3 + 18.0}px; width: {PHASE.2}px; display: flex; flex-direction: column; gap: 6px;",
                div { style: "display: flex; justify-content: space-between;",
                    span { style: "color: #64748b;", "M" }
                    span { style: "font-size: 20px; color: #e2e8f0;", "{fmt_lufs(momentary)} LUFS" }
                }
                div { style: "display: flex; justify-content: space-between;",
                    span { style: "color: #64748b;", "S" }
                    span { "{fmt_lufs(short_term)} LUFS" }
                }
                div { style: "display: flex; justify-content: space-between;",
                    span { style: "color: #64748b;", "I" }
                    span { style: "font-size: 20px; color: #7dd3fc;", "{fmt_lufs(integrated)} LUFS" }
                }
                div { style: "display: flex; justify-content: space-between;",
                    span { style: "color: #64748b;", "LRA" }
                    span { "{range:.1} LU" }
                }
                div { style: "display: flex; justify-content: space-between;",
                    span { style: "color: #64748b;", "TRUE PEAK" }
                    span {
                        style: if true_peak > -1.0 { "color: #f87171;" } else { "color: #e2e8f0;" },
                        "{fmt_lufs(true_peak)} dBTP"
                    }
                }
            }
        }
    }
}
