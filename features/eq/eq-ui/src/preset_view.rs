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
use preset_browser::{Preset, PresetBrowser};

use crate::param_adapter::param_handle;
use crate::params::{EqUiState, FtsEqParams, NUM_BANDS};

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
    "on",
    "freq",
    "gain",
    "q",
    "shape",
    "slope",
    "dyn_range",
    "dyn_thr",
    "dyn_atk",
    "dyn_rel",
    "dyn_auto",
    "spectral",
    "placement",
    "spectral_density",
    "spectral_tilt",
    "dyn_side",
    "dyn_side_lo",
    "dyn_side_hi",
];

/// The per-band string fields a preset can name.
///
/// Deliberately the same `b{n}_` namespace as [`BAND_FIELDS`]: a band's label
/// is addressed the way its frequency is, so a library file reads as one set
/// of per-band settings rather than two.
///
/// They are not in [`BAND_FIELDS`] because they are not host parameters:
/// `name` and `notes` are `#[persist]` strings with no `ParamHandle`, so the
/// host never sees them, nothing automates them, and the repo's "every
/// parameter must parse the string it prints" rule has nothing to check. That
/// is also exactly why a preset could never carry them.
const BAND_TEXT_FIELDS: [&str; 2] = ["name", "notes"];

/// The name a preset addresses one band's field by. One-based, like every
/// other `b{n}_` key, and built here rather than at each use so capture and
/// recall cannot drift into naming the same thing differently.
fn band_key(index: usize, field: &str) -> String {
    format!("b{}_{field}", index + 1)
}

/// The lock behind one band's string field, or `None` for a band or field this
/// build does not have.
///
/// One lookup for both directions, so reading and writing cannot disagree
/// about which field is which — and adding a third label is one arm here plus
/// one entry in [`BAND_TEXT_FIELDS`], not an edit in three places.
///
/// A miss is silent for the same reason
/// [`preset_browser_ui::apply_to_handles`] reports rather than panics: a
/// library outlives a build, and a preset from a version with more bands must
/// cost only the settings it names.
fn band_text_slot<'a>(
    params: &'a FtsEqParams,
    index: usize,
    field: &str,
) -> Option<&'a parking_lot::RwLock<String>> {
    let band = params.bands.get(index)?;
    match field {
        "name" => Some(&band.name),
        "notes" => Some(&band.notes),
        _ => None,
    }
}

/// The `b{n}_name` / `b{n}_notes` a preset should carry for this editor state.
///
/// What is captured is what *differs from the shipped defaults*, not merely
/// what is non-empty. Bands 1 and 2 are born labelled "Low Shelf" and
/// "High Shelf", so "non-empty" would put a text block in every preset ever
/// saved — and because a text block is authoritative (see
/// [`apply_band_text`]), every one of them would clear the user's labels on
/// bands 3-24 on load. The constraint is that loading a preset nobody
/// labelled costs nobody their labels, and only measuring against the
/// defaults gets there: an EQ nobody has labelled really does save with no
/// text block at all.
#[must_use]
pub fn capture_band_text(params: &FtsEqParams) -> Vec<(String, String)> {
    // Asking the type for its own defaults rather than restating them here
    // keeps this right when they change. Save-time only, never the hot path.
    let defaults = FtsEqParams::default();
    let mut text = Vec::new();
    for i in 0..NUM_BANDS {
        for field in BAND_TEXT_FIELDS {
            let Some(slot) = band_text_slot(params, i, field) else {
                continue;
            };
            let value = slot.read().clone();
            if value != default_band_text(&defaults, i, field) {
                text.push((band_key(i, field), value));
            }
        }
    }
    text
}

/// What one band's label reads as on a freshly constructed EQ.
fn default_band_text(defaults: &FtsEqParams, index: usize, field: &str) -> String {
    band_text_slot(defaults, index, field).map_or_else(String::new, |slot| slot.read().clone())
}

/// Recall the per-band labels a preset carries.
///
/// **An empty text block leaves every label alone.** The 171 translated
/// Pro-Q 4 presets carry none, and they are the bulk of the library: if
/// loading one cleared the labels, browsing the factory bank would destroy
/// work the user had done by hand, and there would be no way to audition a
/// curve against your own annotations.
///
/// **A non-empty text block is authoritative for every band.** Once a preset
/// says what the bands are, a band it does not name is a band it says is
/// unlabelled — leaving the previous preset's label there would attach
/// "Overheads honk" to a curve that is no longer the overheads honk, which is
/// worse than no label at all.
///
/// Unlabelled means *the shipped default*, not blank, for the same reason
/// [`capture_band_text`] measures against the defaults: the two are inverses,
/// so capturing an EQ and recalling it elsewhere reproduces it exactly —
/// bands 1 and 2 reading "Low Shelf" / "High Shelf" again included.
pub fn apply_band_text(params: &FtsEqParams, text: &[(String, String)]) {
    if text.is_empty() {
        return;
    }
    let named: std::collections::HashMap<&str, &str> = text
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let defaults = FtsEqParams::default();
    for i in 0..NUM_BANDS {
        for field in BAND_TEXT_FIELDS {
            let Some(slot) = band_text_slot(params, i, field) else {
                continue;
            };
            let key = band_key(i, field);
            *slot.write() = named.get(key.as_str()).map_or_else(
                || default_band_text(&defaults, i, field),
                |value| (*value).to_string(),
            );
        }
    }
}

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

/// Write a preset to the plugin, and report anything that did not land.
///
/// Two halves, because a preset has two: the numeric parameters go through
/// the `ParamHandle` map the host can see, and the per-band labels go straight
/// to the persisted strings, which have no handle.
pub fn apply(
    preset: &Preset,
    params: &FtsEqParams,
    handles: &HashMap<String, ParamHandle>,
    mut note: Signal<String>,
) {
    apply_band_text(params, &preset.text_parameters);
    let (applied, unmatched) = preset_browser_ui::apply_to_handles(&preset.parameters, handles);
    if unmatched.is_empty() {
        note.set(String::new());
    } else {
        note.set(format!(
            "{applied} applied; {} not in this build",
            unmatched.len()
        ));
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
    // The labels are not parameters, so landing them needs the param struct
    // itself rather than the handle map. It is already in context — the
    // editor put it there — so it does not have to become a prop, which it
    // could not anyway: `Props` wants `PartialEq` and `FtsEqParams` is a tree
    // of atomics and locks.
    let params = use_context::<nice_plug_dioxus::SharedState>()
        .get::<EqUiState>()
        .expect("EqUiState missing")
        .params
        .clone();

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
                on_apply: move |p: Preset| apply(&p, &params, &handles, note),
            }
        }
    }
}
