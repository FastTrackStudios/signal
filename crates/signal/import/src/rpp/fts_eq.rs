//! Building an FTS EQ plugin state out of a translated Pro-Q instance.
//!
//! The translation itself already exists — [`crate::fabfilter::proq4`] turns a
//! Pro-Q 4 state into the parameter list the FTS engine takes, and that list
//! is what the whole preset-library measurement was run against. This module
//! is deliberately the thin layer on top: it renames those parameters to the
//! ids the *plugin* publishes and fixes the two places where the plugin and
//! the engine disagree about units. Nothing here decides what a preset means.
//!
//! Keeping it that way matters, because the alternative is two translators
//! that drift. A fix to how a shelf's Q is read belongs in `proq4.rs`, and it
//! reaches the plugin through here for free.
//!
//! ## Where the plugin and the engine differ
//!
//! Two, and both are silent if you get them wrong — the preset loads, it just
//! sounds different:
//!
//! - **Q.** The plugin publishes Pro-Q's *display* Q and multiplies by
//!   `1/√2` on its way to the engine, so a Butterworth reads 1.0 on the
//!   dial. The engine takes the real thing. Going the other way means
//!   multiplying by `√2`.
//! - **Gain Scale.** A percentage on the plugin, a fraction in the engine.
//!
//! ## Shape indices
//!
//! The plugin's `type` parameter is a canonical shape index and crosses
//! unchanged. It is worth saying out loud because the plugin used to carry a
//! table claiming otherwise — that Low Cut and High Shelf were swapped in its
//! persisted order — which was dead code, never on the audio path. Believing
//! it here turned every high shelf into a high-pass, and the plugin-level
//! verification is what caught it.

use std::collections::BTreeMap;

use crate::fabfilter::proq4::{to_native_eq_params, ProQ4};

/// The plugin's CLAP id. REAPER resolves the plugin by this string, which is
/// why writing a CLAP block needs no id hash or class UID.
pub const CLAP_ID: &str = "com.fasttrackstudio.eq";
/// `Plugin::NAME`.
pub const NAME: &str = "FTS EQ";
/// `Plugin::VENDOR`.
pub const VENDOR: &str = "FastTrackStudio";
/// `Plugin::VERSION` — the string nih-plug stamps into the state.
pub const VERSION: &str = "0.1.0";

/// A value in nih-plug's state map, in the shape its `ParamValue` serialises
/// to: `{"f32": …}` / `{"i32": …}`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamValue {
    F32(f32),
    I32(i32),
    Bool(bool),
}

impl ParamValue {
    fn to_json(self) -> serde_json::Value {
        match self {
            ParamValue::F32(v) => serde_json::json!({ "f32": v }),
            ParamValue::I32(v) => serde_json::json!({ "i32": v }),
            ParamValue::Bool(v) => serde_json::json!({ "bool": v }),
        }
    }
}

/// Every band field, at the value a fresh plugin gives it.
///
/// These have to be written out, not left implicit. A plugin state does not
/// reset what it omits — it sets what it names — so a preset that says
/// nothing about band 3's dynamics inherits whatever band 3's dynamics were
/// before it loaded. That is invisible in a fresh instance and wrong the
/// moment a second preset is loaded over a first: measured through the real
/// plugin, presets several slots into a project came back up to 6 dB off,
/// carrying the dynamics of the preset that happened to precede them.
///
/// So a converted instance states its whole mind. It costs about 20 kB per
/// FX block and buys a preset that means the same thing wherever it lands.
const BAND_DEFAULTS: &[(&str, ParamValue)] = &[
    ("on", ParamValue::F32(0.0)),
    ("solo", ParamValue::F32(0.0)),
    ("freq", ParamValue::F32(1000.0)),
    ("gain", ParamValue::F32(0.0)),
    // Published Q is √2 × the engine's, so this is a Butterworth.
    ("q", ParamValue::F32(std::f32::consts::SQRT_2)),
    ("type", ParamValue::I32(0)),
    ("slope", ParamValue::F32(2.0)),
    ("place", ParamValue::I32(0)),
    ("dynrange", ParamValue::F32(0.0)),
    ("dynthr", ParamValue::F32(-18.0)),
    ("dynatk", ParamValue::F32(50.0)),
    ("dynrel", ParamValue::F32(50.0)),
    ("dynauto", ParamValue::F32(1.0)),
    ("spectral", ParamValue::F32(0.0)),
    ("specdens", ParamValue::F32(50.0)),
    ("spectilt", ParamValue::F32(0.0)),
    ("scfilt", ParamValue::F32(0.0)),
    ("sclo", ParamValue::F32(20.0)),
    ("schi", ParamValue::F32(20_000.0)),
];

/// The instance-wide parameters, likewise.
const GLOBAL_DEFAULTS: &[(&str, ParamValue)] = &[
    ("gain_scale", ParamValue::F32(100.0)),
    ("output_gain", ParamValue::F32(0.0)),
    ("auto_gain", ParamValue::F32(0.0)),
    ("character", ParamValue::I32(0)),
    ("output_pan", ParamValue::F32(0.0)),
    ("output_pan_mode", ParamValue::F32(0.0)),
];

/// Translate the engine parameter list into the plugin's parameter map.
///
/// Every band is written, used or not — see [`BAND_DEFAULTS`] for why.
pub fn plugin_params(native: &[(String, f64)]) -> BTreeMap<String, ParamValue> {
    use ParamValue::{F32, I32};
    let mut out: BTreeMap<String, ParamValue> = BTreeMap::new();
    for n in 1..=24usize {
        for (field, value) in BAND_DEFAULTS {
            out.insert(format!("{field}_{n}"), *value);
        }
    }
    for (name, value) in GLOBAL_DEFAULTS {
        out.insert((*name).to_string(), *value);
    }
    // `used` and `on` are separate in the engine and one switch on the
    // plugin, so they are collected and combined rather than mapped.
    let mut used: BTreeMap<usize, (bool, bool)> = BTreeMap::new();

    for (name, value) in native {
        let v = *value;
        if let Some((n, field)) = split_band(name) {
            let id = |suffix: &str| format!("{suffix}_{n}");
            match field {
                "used" => used.entry(n).or_insert((true, true)).0 = v >= 0.5,
                "on" => used.entry(n).or_insert((true, true)).1 = v >= 0.5,
                "freq" => drop(out.insert(id("freq"), F32(v as f32))),
                "gain" => drop(out.insert(id("gain"), F32(v as f32))),
                "q" => drop(out.insert(
                    id("q"),
                    F32((v * std::f64::consts::SQRT_2) as f32),
                )),
                "shape" => drop(out.insert(id("type"), I32(v.round() as i32))),
                "slope" => drop(out.insert(id("slope"), F32(v as f32))),
                "placement" => drop(out.insert(id("place"), I32(v.round() as i32))),
                "dyn_range" => drop(out.insert(id("dynrange"), F32(v as f32))),
                "dyn_thr" => drop(out.insert(id("dynthr"), F32(v as f32))),
                "dyn_atk" => drop(out.insert(id("dynatk"), F32(v as f32))),
                "dyn_rel" => drop(out.insert(id("dynrel"), F32(v as f32))),
                "dyn_auto" => drop(out.insert(id("dynauto"), F32(v as f32))),
                "spectral" => drop(out.insert(id("spectral"), F32(v as f32))),
                "spectral_density" => drop(out.insert(id("specdens"), F32(v as f32))),
                "spectral_tilt" => drop(out.insert(id("spectilt"), F32(v as f32))),
                "dyn_side" => drop(out.insert(id("scfilt"), F32(v as f32))),
                "dyn_side_lo" => drop(out.insert(id("sclo"), F32(v as f32))),
                "dyn_side_hi" => drop(out.insert(id("schi"), F32(v as f32))),
                _ => {}
            }
            continue;
        }
        match name.as_str() {
            // A fraction in the engine, a percentage on the dial.
            "gain_scale" => drop(out.insert("gain_scale".into(), F32((v * 100.0) as f32))),
            "output_gain" => drop(out.insert("output_gain".into(), F32(v as f32))),
            "auto_gain" => drop(out.insert("auto_gain".into(), F32(v as f32))),
            "character" => drop(out.insert("character".into(), I32(v.round() as i32))),
            "output_pan" => drop(out.insert("output_pan".into(), F32(v as f32))),
            "output_pan_mode" => {
                drop(out.insert("output_pan_mode".into(), F32(v as f32)))
            }
            _ => {}
        }
    }

    for (n, (is_used, is_on)) in used {
        let on = is_used && is_on;
        out.insert(format!("on_{n}"), ParamValue::F32(if on { 1.0 } else { 0.0 }));
        if !on {
            // A dead slot goes all the way back to neutral, so it cannot
            // sound if something later switches it on.
            for (field, value) in BAND_DEFAULTS {
                out.insert(format!("{field}_{n}"), *value);
            }
        }
    }

    out
}

/// `b3_dyn_thr` → `(3, "dyn_thr")`.
fn split_band(name: &str) -> Option<(usize, &str)> {
    let rest = name.strip_prefix('b')?;
    let (digits, field) = rest.split_at(rest.find('_')?);
    Some((digits.parse().ok()?, &field[1..]))
}

/// The bytes an FTS EQ instance would have saved for this Pro-Q preset.
///
/// nih-plug's own framing: a little-endian `u64` length, then the JSON. That
/// prefix is the plugin's, not REAPER's — a REAPER `<STATE>` block holds
/// exactly what the plugin wrote and nothing more.
pub fn clap_state(eq: &ProQ4) -> Vec<u8> {
    state_bytes(&plugin_params(&to_native_eq_params(eq)))
}

/// The same, from an already-translated parameter list.
pub fn state_bytes(params: &BTreeMap<String, ParamValue>) -> Vec<u8> {
    encode_state(VERSION, params)
}

/// nih-plug's state framing, for any FTS plugin: a little-endian `u64`
/// length, then the JSON. The prefix is the plugin's own — a REAPER `<STATE>`
/// block holds exactly what the plugin wrote and nothing more.
pub fn encode_state(version: &str, params: &BTreeMap<String, ParamValue>) -> Vec<u8> {
    let doc = serde_json::json!({
        "version": version,
        "params": params
            .iter()
            .map(|(k, v)| (k.clone(), v.to_json()))
            .collect::<serde_json::Map<_, _>>(),
        "fields": serde_json::Map::new(),
    });
    let json = serde_json::to_vec(&doc).expect("state is plain data");
    let mut out = Vec::with_capacity(8 + json.len());
    out.extend_from_slice(&(json.len() as u64).to_le_bytes());
    out.extend_from_slice(&json);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native(pairs: &[(&str, f64)]) -> Vec<(String, f64)> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    }

    #[test]
    fn display_q_is_the_engine_q_scaled_back_up() {
        // Butterworth: the engine's 1/√2 is 1.0 on the plugin's dial, which
        // is the convention Pro-Q shows and the plugin publishes.
        let p = plugin_params(&native(&[
            ("b1_used", 1.0),
            ("b1_on", 1.0),
            ("b1_q", std::f64::consts::FRAC_1_SQRT_2),
        ]));
        match p["q_1"] {
            ParamValue::F32(v) => assert!((v - 1.0).abs() < 1e-6, "{v}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn gain_scale_crosses_as_a_percentage() {
        let p = plugin_params(&native(&[("gain_scale", 1.5)]));
        assert_eq!(p["gain_scale"], ParamValue::F32(150.0));
    }

    #[test]
    fn shapes_cross_as_canonical_indices() {
        // Canonical 2 is High Shelf and 3 is Low Cut, and the plugin means
        // the same by both. Swapping them here — which a dead lookup table in
        // the plugin used to invite — turns every high shelf into a
        // high-pass without changing a single number in the report.
        let p = plugin_params(&native(&[
            ("b1_used", 1.0),
            ("b1_on", 1.0),
            ("b1_shape", 3.0),
            ("b2_used", 1.0),
            ("b2_on", 1.0),
            ("b2_shape", 2.0),
        ]));
        assert_eq!(p["type_1"], ParamValue::I32(3));
        assert_eq!(p["type_2"], ParamValue::I32(2));
    }

    #[test]
    fn a_bell_keeps_its_index() {
        let p = plugin_params(&native(&[
            ("b1_used", 1.0),
            ("b1_on", 1.0),
            ("b1_shape", 0.0),
        ]));
        assert_eq!(p["type_1"], ParamValue::I32(0));
    }

    #[test]
    fn an_unused_slot_is_written_back_to_neutral() {
        let p = plugin_params(&native(&[
            ("b7_used", 0.0),
            ("b7_freq", 1000.0),
            ("b7_gain", 6.0),
        ]));
        assert_eq!(p["on_7"], ParamValue::F32(0.0));
        assert_eq!(p["gain_7"], ParamValue::F32(0.0), "a dead slot is silent, not stale");
    }

    #[test]
    fn a_static_band_says_it_is_static() {
        // The bug this guards: a preset that says nothing about band 1's
        // dynamics inherits the previous preset's. Measured through the real
        // plugin, that put instances late in a project up to 6 dB out.
        let p = plugin_params(&native(&[
            ("b1_used", 1.0),
            ("b1_on", 1.0),
            ("b1_freq", 1000.0),
        ]));
        assert_eq!(p["dynrange_1"], ParamValue::F32(0.0));
        assert_eq!(p["spectral_1"], ParamValue::F32(0.0));
        assert_eq!(p["scfilt_1"], ParamValue::F32(0.0));
        assert_eq!(p["solo_1"], ParamValue::F32(0.0));
    }

    #[test]
    fn every_band_is_accounted_for() {
        // 24 bands x 19 fields + 6 globals: a converted instance states its
        // whole mind, so it means the same thing wherever it is loaded.
        let p = plugin_params(&native(&[("b1_used", 1.0), ("b1_on", 1.0)]));
        assert_eq!(p.len(), 24 * 19 + 6);
        for n in 1..=24 {
            assert!(p.contains_key(&format!("freq_{n}")), "band {n} is unstated");
        }
    }

    #[test]
    fn a_band_switched_off_but_present_is_still_off() {
        // Pro-Q distinguishes "this slot exists" from "this band sounds"; the
        // plugin has one switch, and it has to mean the conjunction.
        let p = plugin_params(&native(&[("b1_used", 1.0), ("b1_on", 0.0)]));
        assert_eq!(p["on_1"], ParamValue::F32(0.0));
    }

    #[test]
    fn the_state_blob_is_a_length_prefixed_json_document() {
        let params = plugin_params(&native(&[("output_gain", -3.0)]));
        let blob = state_bytes(&params);
        let len = u64::from_le_bytes(blob[..8].try_into().unwrap()) as usize;
        assert_eq!(len, blob.len() - 8);
        let doc: serde_json::Value = serde_json::from_slice(&blob[8..]).expect("json");
        assert_eq!(doc["version"], VERSION);
        assert_eq!(doc["params"]["output_gain"]["f32"], -3.0);
        assert!(doc["fields"].is_object());
    }

    #[test]
    fn splitting_a_band_name_finds_multi_word_fields() {
        assert_eq!(split_band("b12_dyn_side_lo"), Some((12, "dyn_side_lo")));
        assert_eq!(split_band("gain_scale"), None);
    }
}
