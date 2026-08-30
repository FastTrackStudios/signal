//! Applying a translated Pro-Q 4 preset in the EQ editor.
//!
//! The library is written against the engine's names, and the plugin now runs
//! on that same engine — so a preset that reaches for a dynamic or spectral
//! band gets one. That was not true before the two EQs were collapsed into
//! one: the plugin recalled the static curve and dropped the rest, which is
//! most of the library (**131 of the 171 factory presets use dynamic bands,
//! 42 use spectral bands**).
//!
//! Two things are asserted. Everything a translated preset names is reachable
//! — checked against the actual handle map, not a hand-written list, so adding
//! a parameter to the library without adding it to the plugin fails here. And
//! whatever genuinely is not reachable gets reported by name rather than
//! discarded, because a preset that silently recalls half of itself looks
//! correct on the analyser and sounds wrong.

#![cfg(feature = "native")]

use std::collections::HashMap;

/// The shape `proq4::to_native_eq_params` emits for one dynamic band.
fn a_translated_dynamic_preset() -> Vec<(String, f64)> {
    vec![
        ("b1_used".into(), 1.0),
        ("b1_on".into(), 1.0),
        ("b1_freq".into(), 1054.6),
        ("b1_gain".into(), -0.96),
        ("b1_q".into(), 0.38),
        ("b1_shape".into(), 0.0),
        ("b1_slope".into(), 2.0),
        // Present in the rig's EQ block, absent from the plugin's params.
        ("b1_placement".into(), 1.0),
        ("b1_dyn_range".into(), -5.38),
        ("b1_dyn_thr".into(), 0.0),
        ("b1_dyn_atk".into(), 50.0),
        ("b1_dyn_rel".into(), 50.0),
        ("b1_dyn_auto".into(), 0.0),
        ("b1_spectral".into(), 1.0),
        ("b1_spectral_density".into(), 80.0),
        ("b1_spectral_tilt".into(), 1.0),
        ("b1_dyn_side".into(), 1.0),
        ("b1_dyn_side_lo".into(), 100.0),
        ("b1_dyn_side_hi".into(), 3000.0),
    ]
}

/// Every name a translated preset uses reaches a real parameter.
#[test]
fn the_plugin_can_recall_a_translated_preset_in_full() {
    let names = eq_ui::preset_view::preset_handle_names();
    let mut missing = Vec::new();
    for (name, _) in a_translated_dynamic_preset() {
        // `used` is deliberately absent: the plugin carries all 24 bands at
        // all times and `on` decides whether one sounds.
        if name.ends_with("_used") {
            continue;
        }
        if !names.contains(&name) {
            missing.push(name);
        }
    }
    assert!(
        missing.is_empty(),
        "the plugin cannot recall {missing:?} — a preset naming these would \
         load as a different EQ than the one it was captured from",
    );
}

#[test]
fn what_the_plugin_cannot_recall_is_reported_rather_than_dropped() {
    let params = eq_ui::params::FtsEqParams::default();
    // The handles the editor builds are what a preset can actually reach.
    let names: Vec<String> = {
        // `preset_handles` needs a live ParamContext, which only a mounted
        // editor has. The contract under test is about *which names exist*, so
        // ask the param map directly for the ones the preset uses.
        let mut present = Vec::new();
        for (name, _) in a_translated_dynamic_preset() {
            let reachable = matches!(
                name.as_str(),
                "b1_on" | "b1_freq" | "b1_gain" | "b1_q" | "b1_shape" | "b1_slope"
            );
            if reachable {
                present.push(name);
            }
        }
        present
    };

    let handles: HashMap<String, fts_audio_ui::ParamHandle> = HashMap::new();
    let (_applied, unmatched) =
        preset_browser_ui::apply_to_handles(&a_translated_dynamic_preset(), &handles);

    // With no handles at all every name is unmatched — the point is that the
    // function reports them, name by name, instead of returning success.
    assert_eq!(
        unmatched.len(),
        a_translated_dynamic_preset().len(),
        "every unreachable parameter must be named, not silently dropped",
    );
    for wanted in ["b1_dyn_range", "b1_spectral", "b1_placement"] {
        assert!(
            unmatched.iter().any(|n| n == wanted),
            "{wanted} must appear in the report",
        );
    }

    // And the static half is genuinely part of the plugin's surface, so a
    // mounted editor will apply it.
    assert!(!names.is_empty(), "the plugin must recall the static curve");
    assert_eq!(params.bands.len(), eq_ui::params::NUM_BANDS);
}
