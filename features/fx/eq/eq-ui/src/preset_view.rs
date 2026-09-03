//! The EQ's preset surfaces.
//!
//! The same pair every FTS editor gets: a strip across the top that always
//! says what is loaded and steps through the library, and a sidecar that opens
//! when you want to look. Both come from `preset-browser-ui` — what belongs to
//! the EQ is only where its library lives and which parameters a preset writes.
//!
//! Native-only, because a library is a directory.

use std::collections::HashMap;

use dioxus::prelude::*;
use fts_audio_ui::ParamHandle;
use nice_plug::prelude::Param;
use nice_plug_dioxus::prelude::ParamContext;
use preset_browser::PresetBrowser;

use crate::param_adapter::param_handle;
use crate::params::{FtsEqParams, NUM_BANDS};

/// Where the EQ's presets live.
#[must_use] 
pub fn preset_library_root() -> std::path::PathBuf {
    preset_browser_ui::library_root(
        "FTS_EQ_PRESETS",
        "/run/media/AudioHaven/Signal/Libraries/Presets/FTS-EQ",
    )
}

/// The EQ's library: every bank under the root, as one list.
#[must_use] 
pub fn load_library() -> (PresetBrowser, String) {
    preset_browser_ui::load_library_tree(&preset_library_root())
}

/// Every parameter a preset can name, by the name it uses.
///
/// The names are the engine's — `b1_freq`, `b1_gain`, `b1_dyn_range` and so
/// on, one-based — which is what the translated `FabFilter` presets are written
/// against.
///
/// The dynamics and placement entries are the point of the consolidation: the
/// plugin and the rig now play through one engine, so a preset that reaches
/// for a dynamic or spectral band gets one here too. It used to recall the
/// static curve and drop the rest, which mattered because **131 of the 171
/// Pro-Q 4 factory presets use dynamic bands and 42 use spectral bands**.
///
/// `b{n}_used` is still not here and does not need to be: the plugin carries
/// all 24 bands at all times and `on` is what decides whether one sounds.
pub fn preset_handles(params: &FtsEqParams, ctx: &ParamContext) -> HashMap<String, ParamHandle> {
    let mut handles = HashMap::new();
    for (i, band) in params.bands.iter().enumerate().take(NUM_BANDS) {
        let n = i + 1;
        // Same order as `BAND_FIELDS`; the assertion below keeps them paired.
        let ptrs = [
            band.enabled.as_ptr(),
            band.freq_hz.as_ptr(),
            band.gain_db.as_ptr(),
            band.q.as_ptr(),
            band.filter_type.as_ptr(),
            band.slope.as_ptr(),
            band.dyn_range_db.as_ptr(),
            band.dyn_threshold_db.as_ptr(),
            band.dyn_attack.as_ptr(),
            band.dyn_release.as_ptr(),
            band.dyn_auto.as_ptr(),
            band.spectral.as_ptr(),
            band.placement.as_ptr(),
            band.spectral_density.as_ptr(),
            band.spectral_tilt.as_ptr(),
            band.side_filtered.as_ptr(),
            band.side_lo_hz.as_ptr(),
            band.side_hi_hz.as_ptr(),
        ];
        debug_assert_eq!(ptrs.len(), BAND_FIELDS.len());
        for (suffix, ptr) in BAND_FIELDS.iter().zip(ptrs) {
            handles.insert(format!("b{n}_{suffix}"), param_handle(ptr, ctx.clone()));
        }
    }
    for (name, ptr) in [
        ("output_gain", params.output_gain_db.as_ptr()),
        ("gain_scale", params.gain_scale.as_ptr()),
    ] {
        handles.insert(name.to_string(), param_handle(ptr, ctx.clone()));
    }
    handles
}

/// The per-band suffixes a preset can name.
///
/// One list, read by both [`preset_handles`] (which needs a live editor to
/// build) and [`preset_handle_names`] (which does not), so the two cannot
/// drift into disagreeing about what this plugin can recall.
const BAND_FIELDS: [&str; 18] = [
    "on", "freq", "gain", "q", "shape", "slope", "dyn_range", "dyn_thr", "dyn_atk", "dyn_rel",
    "dyn_auto", "spectral", "placement", "spectral_density", "spectral_tilt", "dyn_side",
    "dyn_side_lo", "dyn_side_hi",
];

/// The names [`preset_handles`] builds, without needing a live editor.
///
/// The handle map itself can only be built inside a mounted editor (it needs a
/// `ParamContext`), which would put the one thing worth asserting — that the
/// plugin can recall what the library writes — out of reach of a plain test.
/// This is the same list, derived the same way.
#[must_use] 
pub fn preset_handle_names() -> Vec<String> {
    let mut names = Vec::new();
    for i in 0..NUM_BANDS {
        let n = i + 1;
        for suffix in BAND_FIELDS {
            names.push(format!("b{n}_{suffix}"));
        }
    }
    names.push("output_gain".to_string());
    names.push("gain_scale".to_string());
    names
}

/// Write a preset to the plugin's parameters, and report anything that did not
/// land.
pub fn apply(
    values: &[(String, f64)],
    handles: &HashMap<String, ParamHandle>,
    mut note: Signal<String>,
) {
    let (applied, unmatched) = preset_browser_ui::apply_to_handles(values, handles);
    if unmatched.is_empty() {
        note.set(String::new());
    } else {
        note.set(format!("{applied} applied; {} not in this build", unmatched.len()));
    }
}

/// The preset sidecar: the browser itself.
#[component]
pub fn EqPresetSidecar(
    browser: Signal<PresetBrowser>,
    handles: HashMap<String, ParamHandle>,
    note: Signal<String>,
    ink: String,
    accent: String,
) -> Element {
    let message = note.read().clone();

    rsx! {
        div {
            "data-testid": "eq-presets",
            style: "position:absolute; inset:0; display:flex; flex-direction:column; gap:3px; \
                    padding:6px;",
            if !message.is_empty() {
                div {
                    "data-testid": "eq-presets-note",
                    style: format!("font-size:8px; opacity:0.6; color:{ink};"),
                    "{message}"
                }
            }
            preset_browser_ui::PresetBrowserPanel {
                browser,
                ink: ink,
                accent: accent,
                title: "EQ Presets".to_string(),
                on_apply: move |p: Vec<(String, f64)>| apply(&p, &handles, note),
            }
        }
    }
}
