//! From the quantize panel's sliders to the engine, in one pass.
//!
//! The panel (`expression_editor_ui::quantize_panel`) is deliberately
//! engine-blind: it holds plain config values and emits them, and the UI
//! crate does not depend on this one. This module is the other half of
//! that seam — panel-shaped config structs in, [`DetectConfig`] /
//! [`QuantizeConfig`] / [`SplitConfig`] and preview data out — so a host
//! (standalone or REAPER) wires the two with a couple of calls and no
//! translation code of its own.
//!
//! Detection follows the group rule: each trigger lane detects on its
//! members' **summed** signal and the lists are merged, union with
//! nearest-duplicate-collapses-to-the-louder — never per mic.
//! Spec: `drum-mode.md` r[drums.group.detection-source].

use expression_editor_tools::quantize as tools;
pub use expression_editor_tools::quantize::{HitPreview, hit_previews};

use crate::detect::{DetectConfig, Filters, Transient};
use crate::group_detect::{detect_summed, merge_hits};
use crate::quantize::{Plan, QuantizeConfig, SplitConfig, plan};

/// The Detect section's values, as the panel holds them.
///
/// Field-for-field the shape of `quantize_panel::DetectSettings`; a
/// mirrored struct rather than a dependency because the arrow between
/// the crates points the other way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelDetect {
    pub threshold_db: f64,
    pub sensitivity: f64,
    pub crest_db: f64,
    pub high_pass_hz: Option<f64>,
    pub low_pass_hz: Option<f64>,
    pub gain: f64,
    pub retrigger_secs: f64,
    pub time_offset_secs: f64,
}

impl Default for PanelDetect {
    fn default() -> Self {
        Self {
            threshold_db: -60.0,
            sensitivity: 0.5,
            crest_db: 3.0,
            high_pass_hz: None,
            low_pass_hz: None,
            gain: 1.0,
            retrigger_secs: 0.050,
            time_offset_secs: 0.0,
        }
    }
}

/// The Target section's values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelTarget {
    /// Seconds between grid divisions — the host computes this from its
    /// tempo map and the panel's division/feel choice.
    pub grid_secs: f64,
    /// Where the grid starts, seconds from the take's start.
    pub grid_offset_secs: f64,
    /// Swing on the off-beat divisions, `0.0..=1.0`. Target placement
    /// only — the planner never sees it.
    pub swing: f64,
    /// Whether each division scans a window for its loudest hit.
    pub grid_scan: bool,
    /// The window half-width while `grid_scan` is on, seconds.
    pub tolerance_secs: f64,
    /// `0.0` leaves the take alone, `1.0` puts every hit on its
    /// division.
    pub strength: f64,
}

impl Default for PanelTarget {
    fn default() -> Self {
        Self {
            grid_secs: 0.125,
            grid_offset_secs: 0.0,
            swing: 0.0,
            grid_scan: true,
            tolerance_secs: 0.05,
            strength: 1.0,
        }
    }
}

/// The Write section's values. SPLIT is the kit default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelWrite {
    /// `true` cuts and moves pieces; `false` warps.
    pub split: bool,
    pub pad_secs: f64,
    pub crossfade_secs: f64,
}

impl Default for PanelWrite {
    fn default() -> Self {
        Self {
            split: true,
            pad_secs: 0.007,
            crossfade_secs: 0.007,
        }
    }
}

/// Panel detect values → the detector's own config.
// r[impl drums.quantize.panel]
pub fn detect_config(d: &PanelDetect) -> DetectConfig {
    let mut cfg = DetectConfig::default();
    cfg.gate.threshold_db = d.threshold_db;
    cfg.gate.crest_db = d.crest_db;
    cfg.gate.retrigger_secs = d.retrigger_secs;
    cfg.filters = Filters {
        high_pass_hz: d.high_pass_hz,
        low_pass_hz: d.low_pass_hz,
        gain: d.gain,
    };
    cfg.sensitivity = d.sensitivity;
    cfg.time_offset_secs = d.time_offset_secs;
    cfg
}

/// Panel target values → the planner's config.
// r[impl drums.quantize.panel]
pub fn quantize_config(t: &PanelTarget) -> QuantizeConfig {
    QuantizeConfig {
        grid_secs: t.grid_secs,
        grid_offset_secs: t.grid_offset_secs,
        tolerance_secs: t.grid_scan.then_some(t.tolerance_secs),
        strength: t.strength,
    }
}

/// Panel write values → SPLIT's config.
// r[impl drums.quantize.panel]
pub fn split_config(w: &PanelWrite) -> SplitConfig {
    SplitConfig {
        leading_pad_secs: w.pad_secs,
        crossfade_secs: w.crossfade_secs,
    }
}

/// Detect over the trigger lanes and merge, per the group rule.
///
/// `trigger_lanes` is one entry per armed lane (kick, snare, an armed
/// tom), each a list of that lane's member sample slices. Every lane
/// detects on its members' summed signal, and the lists merge with the
/// retrigger window as the duplicate window.
// r[impl drums.group.detection-source]
pub fn detect_group(
    trigger_lanes: &[Vec<&[f64]>],
    sample_rate: f64,
    detect: &PanelDetect,
) -> Vec<Transient> {
    let cfg = detect_config(detect);
    let per_lane: Vec<Vec<Transient>> = trigger_lanes
        .iter()
        .map(|members| detect_summed(members, sample_rate, cfg))
        .collect();
    let lists: Vec<&[Transient]> = per_lane.iter().map(|l| l.as_slice()).collect();
    merge_hits(&lists, detect.retrigger_secs)
}

/// The whole panel loop in one call: detect, merge, plan, preview.
///
/// Returns the merged hits (for the histogram), the plan (for Apply to
/// render as splits or a warp map), and the per-hit from→to list the
/// lanes and the panel's strip draw.
// r[impl drums.quantize.preview]
pub fn preview(
    trigger_lanes: &[Vec<&[f64]>],
    sample_rate: f64,
    detect: &PanelDetect,
    target: &PanelTarget,
) -> (Vec<Transient>, Plan, Vec<HitPreview>) {
    let hits = detect_group(trigger_lanes, sample_rate, detect);
    let (p, previews) = preview_hits(&hits, target);
    (hits, p, previews)
}

/// Plan + preview over a hit list the caller already holds — the same
/// pass as [`preview`], split out so a host that overlays hand-added or
/// hand-removed hits (r[drums.manual.add-remove]) plans over the list
/// the user actually sees.
pub fn preview_hits(hits: &[Transient], target: &PanelTarget) -> (Plan, Vec<HitPreview>) {
    let mut p = plan(hits, quantize_config(target));
    // The same swing pass the panel's own preview runs — one body, in
    // the tools crate, so the two can never disagree.
    tools::swing(&mut p.0, quantize_config(target).into(), target.swing);
    let previews = hit_previews(&p);
    (p, previews)
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify drums.quantize.panel]
    #[test]
    fn panel_values_land_on_the_engine_types_field_for_field() {
        let d = PanelDetect {
            threshold_db: -48.0,
            sensitivity: 0.7,
            crest_db: 6.0,
            high_pass_hz: Some(30.0),
            low_pass_hz: Some(800.0),
            gain: 1.5,
            retrigger_secs: 0.080,
            time_offset_secs: -0.002,
        };
        let cfg = detect_config(&d);
        assert_eq!(cfg.gate.threshold_db, -48.0);
        assert_eq!(cfg.gate.crest_db, 6.0);
        assert_eq!(cfg.gate.retrigger_secs, 0.080);
        assert_eq!(cfg.filters.high_pass_hz, Some(30.0));
        assert_eq!(cfg.filters.low_pass_hz, Some(800.0));
        assert_eq!(cfg.filters.gain, 1.5);
        assert_eq!(cfg.sensitivity, 0.7);
        assert_eq!(cfg.time_offset_secs, -0.002);

        let t = PanelTarget {
            grid_secs: 0.125,
            grid_offset_secs: 0.01,
            swing: 0.0,
            grid_scan: true,
            tolerance_secs: 0.04,
            strength: 0.8,
        };
        let q = quantize_config(&t);
        assert_eq!(q.grid_secs, 0.125);
        assert_eq!(q.grid_offset_secs, 0.01);
        assert_eq!(q.tolerance_secs, Some(0.04));
        assert_eq!(q.strength, 0.8);

        // Grid scan off is tolerance off — nearest-division mode.
        let q = quantize_config(&PanelTarget {
            grid_scan: false,
            ..t
        });
        assert_eq!(q.tolerance_secs, None);

        let w = PanelWrite {
            split: true,
            pad_secs: 0.005,
            crossfade_secs: 0.010,
        };
        let s = split_config(&w);
        assert_eq!(s.leading_pad_secs, 0.005);
        assert_eq!(s.crossfade_secs, 0.010);
    }

    /// A decaying burst at `at` seconds — a synthetic drum hit.
    fn burst(out: &mut [f64], sample_rate: f64, at: f64, level: f64) {
        let start = (at * sample_rate) as usize;
        for i in 0..((0.005 * sample_rate) as usize) {
            if let Some(s) = out.get_mut(start + i) {
                *s = level * (-(i as f64) / (0.001 * sample_rate)).exp();
            }
        }
    }

    // r[verify drums.group.detection-source]
    #[test]
    fn detection_runs_on_the_summed_members_and_merges_lanes() {
        let sr = 48_000.0;
        let mut kick_a = vec![0.0; sr as usize];
        burst(&mut kick_a, sr, 0.500, 0.9);
        let kick_b = kick_a.clone();
        let mut snare = vec![0.0; sr as usize];
        burst(&mut snare, sr, 0.502, 0.8); // inside the retrigger window
        burst(&mut snare, sr, 0.750, 0.8);

        let lanes: Vec<Vec<&[f64]>> = vec![
            vec![kick_a.as_slice(), kick_b.as_slice()],
            vec![snare.as_slice()],
        ];
        let hits = detect_group(&lanes, sr, &PanelDetect::default());
        assert_eq!(
            hits.len(),
            2,
            "the near-duplicate collapsed into the kick's hit: {hits:?}"
        );
        assert!((hits[0].at - 0.5).abs() < 0.005);
        assert!((hits[1].at - 0.75).abs() < 0.005);
    }

    // r[verify drums.quantize.preview]
    #[test]
    fn the_preview_says_which_hits_move_and_which_are_left_alone() {
        let sr = 48_000.0;
        let mut kick = vec![0.0; (2.0 * sr) as usize];
        burst(&mut kick, sr, 0.520, 0.9); // late — moves to 0.5
        burst(&mut kick, sr, 1.250, 0.9); // between divisions — left alone
        burst(&mut kick, sr, 1.505, 0.9); // barely late — moves to 1.5

        let lanes: Vec<Vec<&[f64]>> = vec![vec![kick.as_slice()]];
        let target = PanelTarget {
            grid_secs: 0.5,
            grid_offset_secs: 0.0,
            swing: 0.0,
            grid_scan: true,
            tolerance_secs: 0.05,
            strength: 1.0,
        };
        let (hits, plan, previews) = preview(&lanes, sr, &PanelDetect::default(), &target);
        assert_eq!(hits.len(), 3, "all three bursts were detected: {hits:?}");
        assert_eq!(plan.moves.len(), 2);
        assert_eq!(previews.len(), 3);
        assert_eq!(previews.iter().filter(|h| h.moved).count(), 2);
        assert_eq!(previews.iter().filter(|h| h.excluded).count(), 1);
        let excluded = previews.iter().find(|h| h.excluded).unwrap();
        assert!(
            (excluded.at - 1.25).abs() < 0.01,
            "the off-grid hit is the one left alone"
        );
    }

    // r[verify drums.quantize.grid-options]
    #[test]
    fn swing_in_the_bridge_is_the_same_pass_the_panel_previews() {
        let sr = 48_000.0;
        let mut kick = vec![0.0; (2.0 * sr) as usize];
        burst(&mut kick, sr, 0.760, 0.9); // near the off-beat at 0.75

        let lanes: Vec<Vec<&[f64]>> = vec![vec![kick.as_slice()]];
        let target = PanelTarget {
            grid_secs: 0.25,
            grid_offset_secs: 0.0,
            swing: 1.0,
            grid_scan: true,
            tolerance_secs: 0.05,
            strength: 1.0,
        };
        let (_, plan, _) = preview(&lanes, sr, &PanelDetect::default(), &target);
        assert_eq!(plan.moves.len(), 1);
        // 0.75 is an odd division of a 0.25 grid; full swing delays the
        // target by grid/3.
        let want = 0.75 + 0.25 / 3.0;
        assert!(
            (plan.moves[0].to - want).abs() < 1e-6,
            "swung target: {} vs {want}",
            plan.moves[0].to
        );
    }
}
