//! The compressor's preset surfaces.
//!
//! The same pair every FTS editor gets: a strip across the top that always
//! says what is loaded and steps through the library, and a browser that opens
//! when you want to look. Both come from `preset-browser-ui` — what belongs to
//! the compressor is where its library lives and which parameters a preset
//! writes.
//!
//! # A preset writes one stage
//!
//! The plugin is a stack of eight complete compressors, so "apply a preset"
//! has to mean applying it *somewhere*. It writes the **focused** stage — the
//! one the editor is showing and the one the meters are following — because
//! that is the compressor the user is looking at. A preset is a compressor,
//! not a stack.
//!
//! Native-only, because a library is a directory.

use dioxus::prelude::*;
use std::collections::HashMap;

use fts_audio_ui::ParamHandle;
use preset_browser::PresetBrowser;

use crate::param_map::{core_param_ptr, CORE_PARAM_NAMES};
use crate::params::CompStageParams;
use fts_plug_ui::param_adapter::param_handle;

/// Where the compressor's presets live.
pub fn preset_library_root() -> std::path::PathBuf {
    preset_browser_ui::library_root(
        "FTS_COMP_PRESETS",
        "/run/media/AudioHaven/Signal/Libraries/Presets/FTS-Comp",
    )
}

/// The compressor's library: every bank under the root, as one list.
pub fn load_library() -> (PresetBrowser, String) {
    preset_browser_ui::load_library_tree(&preset_library_root())
}

/// Every parameter a preset can write on one stage, by core name.
///
/// Built from [`CORE_PARAM_NAMES`] rather than a second hand-written list, so
/// the browser and the profile system agree on what this plugin exposes.
pub fn preset_handles(
    stage: &CompStageParams,
    ctx: &nice_plug_dioxus::prelude::ParamContext,
) -> HashMap<String, ParamHandle> {
    CORE_PARAM_NAMES
        .iter()
        .filter_map(|name| {
            core_param_ptr(stage, name)
                .map(|ptr| (name.to_string(), param_handle(ptr, ctx.clone())))
        })
        .collect()
}

/// Write a preset to the focused stage, and report anything that did not land.
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

/// The preset browser panel.
#[component]
pub fn CompPresetSidecar(
    browser: Signal<PresetBrowser>,
    handles: HashMap<String, ParamHandle>,
    note: Signal<String>,
    ink: String,
    accent: String,
) -> Element {
    let message = note.read().clone();

    rsx! {
        div {
            "data-testid": "comp-presets",
            style: "position:absolute; inset:0; display:flex; flex-direction:column; gap:3px; \
                    padding:6px;",
            if !message.is_empty() {
                div {
                    "data-testid": "comp-presets-note",
                    style: format!("font-size:8px; opacity:0.6; color:{ink};"),
                    "{message}"
                }
            }
            preset_browser_ui::PresetBrowserPanel {
                browser,
                ink: ink.clone(),
                accent: accent.clone(),
                title: "Comp Presets".to_string(),
                on_apply: move |p: Vec<(String, f64)>| apply(&p, &handles, note),
            }
        }
    }
}
