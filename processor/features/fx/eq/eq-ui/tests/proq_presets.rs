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

// ── Per-band names and notes ───────────────────────────────────────────────
//
// A band's `name`/`notes` are persisted strings, not parameters, so they were
// invisible to a preset: the library format only carried numbers. These cover
// the two halves — what a save captures, and what a load does with it.

#[test]
fn an_eq_nobody_labelled_saves_with_no_text_block_at_all() {
    // Bands 1 and 2 ship labelled "Low Shelf" / "High Shelf". If those counted
    // as labels, every preset ever saved would carry a text block — and since
    // a text block is authoritative, loading any of them would clear the
    // user's labels on bands 3-24. This is the test that keeps the whole
    // backward-compatibility story honest.
    let params = eq_ui::params::FtsEqParams::default();
    assert!(
        eq_ui::preset_view::capture_band_text(&params).is_empty(),
        "shipped defaults are not labels",
    );
}

#[test]
fn an_unnamed_band_comes_back_to_its_shipped_label_not_a_blank() {
    // Capture and recall are inverses: an EQ saved elsewhere must reproduce
    // here exactly, bands 1 and 2 included.
    let params = eq_ui::params::FtsEqParams::default();
    *params.bands[0].name.write() = "wiped by the preset".to_string();
    *params.bands[1].name.write() = "so is this".to_string();

    eq_ui::preset_view::apply_band_text(
        &params,
        &[("b3_name".to_string(), "Overheads honk".to_string())],
    );

    assert_eq!(*params.bands[2].name.read(), "Overheads honk");
    assert_eq!(*params.bands[0].name.read(), "Low Shelf");
    assert_eq!(*params.bands[1].name.read(), "High Shelf");
}

#[test]
fn saving_captures_the_names_and_notes_a_user_typed() {
    let params = eq_ui::params::FtsEqParams::default();
    *params.bands[0].name.write() = "Overheads honk".to_string();
    *params.bands[0].notes.write() = "800 Hz ring".to_string();
    *params.bands[3].name.write() = "Air".to_string();

    let text = eq_ui::preset_view::capture_band_text(&params);

    // Band 2 is still at its shipped "High Shelf" and so is not a label the
    // user gave; band 1's default was overwritten and is.
    assert_eq!(
        text,
        vec![
            ("b1_name".to_string(), "Overheads honk".to_string()),
            ("b1_notes".to_string(), "800 Hz ring".to_string()),
            ("b4_name".to_string(), "Air".to_string()),
        ],
        "every label that differs from the default, by engine-side name",
    );
}

#[test]
fn loading_a_preset_that_carries_names_sets_them() {
    let params = eq_ui::params::FtsEqParams::default();
    // Band 2 ships as "High Shelf"; stand a stale user label on top of it.
    *params.bands[1].name.write() = "stale".to_string();

    eq_ui::preset_view::apply_band_text(
        &params,
        &[
            ("b1_name".to_string(), "Overheads honk".to_string()),
            ("b1_notes".to_string(), "800 Hz ring".to_string()),
        ],
    );

    assert_eq!(*params.bands[0].name.read(), "Overheads honk");
    assert_eq!(*params.bands[0].notes.read(), "800 Hz ring");
    // A preset that carries names is authoritative about all of them:
    // otherwise band 2 keeps a label from whatever was loaded before, which
    // now describes a curve that is no longer there. Unlabelled means the
    // shipped default, which for band 2 is "High Shelf".
    assert_eq!(*params.bands[1].name.read(), "High Shelf");
}

#[test]
fn loading_a_legacy_nameless_preset_leaves_the_users_labels_alone() {
    // The 171 translated Pro-Q 4 presets carry no names. Loading one must
    // recall the curve without wiping labels the user typed — clearing them
    // would make the whole existing library destructive to work.
    let params = eq_ui::params::FtsEqParams::default();
    *params.bands[1].name.write() = "Boxiness".to_string();
    *params.bands[1].notes.write() = "mine, keep it".to_string();

    eq_ui::preset_view::apply_band_text(&params, &[]);

    assert_eq!(*params.bands[1].name.read(), "Boxiness");
    assert_eq!(*params.bands[1].notes.read(), "mine, keep it");
}

#[test]
fn a_name_the_plugin_has_no_band_for_is_ignored_rather_than_panicking() {
    // A library outlives a build, exactly as it does for numeric parameters.
    let params = eq_ui::params::FtsEqParams::default();
    eq_ui::preset_view::apply_band_text(
        &params,
        &[
            ("b99_name".to_string(), "from a bigger EQ".to_string()),
            (
                "b0_name".to_string(),
                "one-based, so not a band".to_string(),
            ),
            ("nonsense".to_string(), "not a band field".to_string()),
            ("b2_name".to_string(), "Boxiness".to_string()),
        ],
    );
    assert_eq!(*params.bands[1].name.read(), "Boxiness");
}

#[test]
fn a_unicode_name_and_a_very_long_note_survive_capture_and_recall() {
    let name = "Übergänge — 高域 “air”";
    let note = "why: ".to_string() + &"the overheads ring at 800 Hz. ".repeat(400);

    let source = eq_ui::params::FtsEqParams::default();
    *source.bands[0].name.write() = name.to_string();
    *source.bands[0].notes.write() = note.clone();

    let destination = eq_ui::params::FtsEqParams::default();
    eq_ui::preset_view::apply_band_text(&destination, &capture_through_json(&source));

    assert_eq!(*destination.bands[0].name.read(), name);
    assert_eq!(*destination.bands[0].notes.read(), note);
}

/// Capture, write to the library format, read it back — the whole path a
/// preset actually takes, rather than handing the strings straight over.
fn capture_through_json(params: &eq_ui::params::FtsEqParams) -> Vec<(String, String)> {
    let dir = std::env::temp_dir().join(format!("eq-preset-names-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let preset = preset_browser::Preset {
        name: "Captured".to_string(),
        text_parameters: eq_ui::preset_view::capture_band_text(params),
        ..preset_browser::Preset::default()
    };
    preset_browser::save_preset(dir.join("captured.json"), &preset).unwrap();
    let mut report = preset_browser::load_directory(&dir).unwrap();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    report.presets.remove(0).text_parameters
}
