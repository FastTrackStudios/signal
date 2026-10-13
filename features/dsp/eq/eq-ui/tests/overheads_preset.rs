//! The Overheads preset — FTS EQ issue #2 in `processor`.
//!
//! This is the first FTS-authored preset and the pattern the rest follow:
//! named bands, each documenting why it exists, parked on its region with
//! its character already dialled and its one control at rest. The shipped
//! file lives outside the repo, at
//! `/run/media/AudioHaven/Signal/Libraries/Presets/FTS-EQ/fts/Overheads.json`
//! (a preset library is a directory someone drops files into, same as the
//! translated `fabfilter-proq4` bank beside it) — this fixture is kept in
//! sync with it by hand, the same way `proq_presets.rs` keeps its translated
//! fixture in sync with the FabFilter converter rather than reading a file
//! off disk.
//!
//! What "at rest" means for audio is proven separately, at the engine level,
//! in `eq-dsp`'s `tests/overheads_preset.rs` — this file covers the half
//! that lives above the engine: that the plugin can recall every name this
//! preset uses, and that the labels and notes land verbatim.

#![cfg(feature = "native")]

use eq_ui::params::FtsEqParams;
use eq_ui::preset_view::{apply_band_text, capture_band_text, preset_handle_names};

/// The Overheads preset's numeric parameters — one-based band names, exactly
/// as `preset_view::BAND_FIELDS` addresses them.
fn overheads_parameters() -> Vec<(String, f64)> {
    let mut p = vec![
        // 1 — High Pass: off by default.
        ("b1_on".into(), 0.0),
        ("b1_freq".into(), 150.0),
        ("b1_gain".into(), 0.0),
        ("b1_q".into(), 1.0),
        ("b1_shape".into(), 3.0), // Low Cut
        ("b1_slope".into(), 2.0), // 12 dB/oct
        ("b1_placement".into(), 0.0),
        // 2 — Clank: dynamic bell, at rest.
        ("b2_on".into(), 1.0),
        ("b2_freq".into(), 300.0),
        ("b2_gain".into(), 0.0),
        ("b2_q".into(), 1.0),
        ("b2_shape".into(), 0.0), // Bell
        ("b2_slope".into(), 2.0),
        ("b2_placement".into(), 0.0),
        ("b2_dyn_range".into(), 0.0),
        ("b2_dyn_thr".into(), -18.0),
        ("b2_dyn_atk".into(), 65.0),
        ("b2_dyn_rel".into(), 50.0),
        ("b2_dyn_auto".into(), 0.0),
        ("b2_spectral".into(), 0.0),
        // 3 — Snare Ring: dynamic bell, at rest.
        ("b3_on".into(), 1.0),
        ("b3_freq".into(), 450.0),
        ("b3_gain".into(), 0.0),
        ("b3_q".into(), 6.0),
        ("b3_shape".into(), 0.0),
        ("b3_slope".into(), 2.0),
        ("b3_placement".into(), 0.0),
        ("b3_dyn_range".into(), 0.0),
        ("b3_dyn_thr".into(), -18.0),
        ("b3_dyn_atk".into(), 30.0),
        ("b3_dyn_rel".into(), 50.0),
        ("b3_dyn_auto".into(), 0.0),
        ("b3_spectral".into(), 0.0),
        // 4 — Lowest Cymbal: spectral, at rest.
        ("b4_on".into(), 1.0),
        ("b4_freq".into(), 3500.0),
        ("b4_gain".into(), 0.0),
        ("b4_q".into(), 1.2),
        ("b4_shape".into(), 0.0),
        ("b4_slope".into(), 2.0),
        ("b4_placement".into(), 0.0),
        ("b4_dyn_range".into(), 0.0),
        ("b4_dyn_thr".into(), -18.0),
        ("b4_dyn_atk".into(), 95.0),
        ("b4_dyn_rel".into(), 50.0),
        ("b4_dyn_auto".into(), 0.0),
        ("b4_spectral".into(), 1.0),
        ("b4_spectral_density".into(), 75.0),
        // 5 — Highest Cymbal: spectral, at rest.
        ("b5_on".into(), 1.0),
        ("b5_freq".into(), 6500.0),
        ("b5_gain".into(), 0.0),
        ("b5_q".into(), 1.2),
        ("b5_shape".into(), 0.0),
        ("b5_slope".into(), 2.0),
        ("b5_placement".into(), 0.0),
        ("b5_dyn_range".into(), 0.0),
        ("b5_dyn_thr".into(), -18.0),
        ("b5_dyn_atk".into(), 95.0),
        ("b5_dyn_rel".into(), 50.0),
        ("b5_dyn_auto".into(), 0.0),
        ("b5_spectral".into(), 1.0),
        ("b5_spectral_density".into(), 75.0),
        // 6 — Air: high shelf, gain at rest.
        ("b6_on".into(), 1.0),
        ("b6_freq".into(), 12000.0),
        ("b6_gain".into(), 0.0),
        ("b6_q".into(), 0.7),
        ("b6_shape".into(), 2.0), // High Shelf
        ("b6_slope".into(), 2.0),
        ("b6_placement".into(), 0.0),
    ];
    // Bands 7-24: explicitly off, so loading this preset never leaves a
    // previous preset's bands sounding.
    for n in 7..=24 {
        p.push((format!("b{n}_on"), 0.0));
    }
    p
}

const NAMES: [(&str, &str); 6] = [
    ("b1_name", "High Pass"),
    ("b2_name", "Clank"),
    ("b3_name", "Snare Ring"),
    ("b4_name", "Lowest Cymbal"),
    ("b5_name", "Highest Cymbal"),
    ("b6_name", "Air"),
];

const NOTES: [(&str, &str); 6] = [
    (
        "b1_notes",
        "High Pass. Everything below this in an overhead is spill and rumble; \
         the kick and the kit's low end belong to the close mics. Off by \
         default so the pair can be heard whole first, then switched in.",
    ),
    (
        "b2_notes",
        "Clank. The gong-and-cardboard region a pair of overheads adds to the \
         kit. Dynamic rather than static because it is only a problem when \
         the kit is loud. Medium attack: there is no transient here worth \
         protecting.",
    ),
    (
        "b3_notes",
        "Snare Ring. The one shell resonance no close mic can remove once it \
         is in the overheads. Narrow, and found by sweeping with the range \
         down before setting it. Fast attack, because a ring starts with the \
         hit.",
    ),
    (
        "b4_notes",
        "Lowest Cymbal — slow the attack until the snare walks through. Park \
         it on the smaller crash / hat region, around 3.5 kHz. Attack \
         deliberately slow: the snare transient is faster than a cymbal's \
         harsh sustain, so a slow attack lets the stick hit pass and only \
         clamps what rings after. Dial density finer so it works on the \
         offending partials instead of a whole octave. Bypass-toggle on an \
         isolated snare hit — if the snare changes, you're too fast or too \
         low.",
    ),
    (
        "b5_notes",
        "Highest Cymbal — the same move, one region up. China or the bigger \
         crash, 6–7 kHz. It's a separate band only because those cymbals \
         don't share a range with the small ones; one wide band would clamp \
         them together and make the whole top end move as a unit. Set it on \
         the crash-heaviest bar in the song, not the verse.",
    ),
    (
        "b6_notes",
        "Air — restorative, not additive. Add it last and only to put back \
         the openness bands 4 and 5 removed. Gentle shelf, a dB or two. If \
         you find yourself needing more than that, the fix is upstream — go \
         back and lighten the cymbal thresholds rather than papering over \
         them here.",
    ),
];

fn overheads_text_parameters() -> Vec<(String, String)> {
    let mut text = Vec::new();
    for (name, value) in NAMES.iter().zip(NOTES.iter()) {
        text.push((name.0.to_string(), name.1.to_string()));
        text.push((value.0.to_string(), value.1.to_string()));
    }
    text
}

/// Every numeric name the preset uses reaches a real parameter on the plugin
/// — checked against the actual handle map, so a future rename would fail
/// here rather than silently dropping band 4 or 5 on load.
#[test]
fn the_plugin_can_recall_every_parameter_the_preset_names() {
    let names = preset_handle_names();
    let parameters = overheads_parameters();
    let missing: Vec<&String> = parameters
        .iter()
        .map(|(name, _)| name)
        .filter(|name| !names.contains(name))
        .collect();
    assert!(
        missing.is_empty(),
        "the plugin cannot recall {missing:?} — the Overheads preset would \
         load as a different EQ than the one it was authored on",
    );
}

/// Loading the preset names all six bands and fills all six notes, verbatim.
#[test]
fn loading_the_preset_names_and_documents_all_six_bands() {
    let params = FtsEqParams::default();
    apply_band_text(&params, &overheads_text_parameters());

    for (key, expected) in NAMES {
        let n: usize = key
            .trim_start_matches('b')
            .trim_end_matches("_name")
            .parse()
            .unwrap();
        assert_eq!(
            *params.bands[n - 1].name.read(),
            expected,
            "band {n}'s name",
        );
    }
    for (key, expected) in NOTES {
        let n: usize = key
            .trim_start_matches('b')
            .trim_end_matches("_notes")
            .parse()
            .unwrap();
        assert_eq!(
            *params.bands[n - 1].notes.read(),
            expected,
            "band {n}'s notes",
        );
    }
    // Bands 7-24 come back to their shipped (blank) label — the preset is
    // authoritative for every band it does not name, not just the six it
    // does.
    assert!(params.bands[6].name.read().is_empty());
}

/// The text block round-trips through the library format exactly the way
/// `proq_presets.rs` checks for a hand-typed label — this just does it for
/// all twelve strings the Overheads preset carries at once.
#[test]
fn the_preset_survives_a_save_and_reload() {
    let dir = std::env::temp_dir().join(format!(
        "eq-overheads-preset-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let description = "Dialing order: cymbal bands on the loudest crash section, snare bands \
                        on the sparsest intro, air at the end. Final test is the fader itself \
                        — push the overheads well past where you'd normally sit them. If it \
                        stays natural instead of turning harsh and boxy, it's set."
        .to_string();
    let preset = preset_browser::Preset {
        name: "Overheads".to_string(),
        category: Some("Overheads".to_string()),
        origin: Some("FTS EQ".to_string()),
        description: Some(description.clone()),
        parameters: overheads_parameters(),
        text_parameters: overheads_text_parameters(),
        ..preset_browser::Preset::default()
    };
    preset_browser::save_preset(dir.join("Overheads.json"), &preset).unwrap();

    let mut report = preset_browser::load_directory(&dir).unwrap();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let read_back = report.presets.remove(0);

    assert_eq!(read_back.parameters, overheads_parameters());
    assert_eq!(read_back.text_parameters, overheads_text_parameters());
    assert_eq!(read_back.description, Some(description));

    // And the round trip is exactly what a mounted editor would apply: name
    // capture is the inverse of recall.
    let params = FtsEqParams::default();
    apply_band_text(&params, &read_back.text_parameters);
    assert_eq!(capture_band_text(&params), overheads_text_parameters());
}

/// Bands 4 and 5 load with spectral mode on and density at 75 %; bands 2 and
/// 3 load as ordinary dynamic bells.
#[test]
fn bands_four_and_five_are_spectral_two_and_three_are_not() {
    let parameters = overheads_parameters();
    let get = |name: &str| parameters.iter().find(|(n, _)| n == name).map(|(_, v)| *v);
    assert_eq!(get("b2_spectral"), Some(0.0));
    assert_eq!(get("b3_spectral"), Some(0.0));
    assert_eq!(get("b4_spectral"), Some(1.0));
    assert_eq!(get("b4_spectral_density"), Some(75.0));
    assert_eq!(get("b5_spectral"), Some(1.0));
    assert_eq!(get("b5_spectral_density"), Some(75.0));
}

/// Every band is at rest on load: bands 2-5 at `dyn_range` 0, band 6 at
/// `gain` 0, band 1 `on` off. (What "at rest" means for audio is proven at
/// the engine level, in `eq-dsp`'s `tests/overheads_preset.rs`.)
#[test]
fn every_band_is_at_rest_on_load() {
    let parameters = overheads_parameters();
    let get = |name: &str| parameters.iter().find(|(n, _)| n == name).map(|(_, v)| *v);
    assert_eq!(get("b1_on"), Some(0.0), "band 1's high pass is off");
    for n in 2..=5 {
        assert_eq!(
            get(&format!("b{n}_dyn_range")),
            Some(0.0),
            "band {n}'s dyn_range is 0"
        );
    }
    assert_eq!(get("b6_gain"), Some(0.0), "band 6's shelf gain is 0 dB");
}
