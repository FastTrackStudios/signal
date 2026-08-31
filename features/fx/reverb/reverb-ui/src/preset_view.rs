//! The reverb's preset surfaces.
//!
//! Two of them, sharing one browser: a strip across the top of the editor that
//! always says what is loaded and steps through the library, and a sidecar
//! that opens when you want to look. Both come from `preset-browser-ui`, which
//! the EQ and compressor editors mount the same way — what belongs to the
//! reverb is only where its library lives and which parameters a preset writes.
//!
//! Native-only, because a library is a directory.

use dioxus::prelude::*;
use std::collections::HashMap;

use fts_audio_ui::ParamHandle;
use preset_browser::PresetBrowser;

/// Where the reverb's presets live.
pub fn preset_library_root() -> std::path::PathBuf {
    preset_browser_ui::library_root(
        "FTS_REVERB_PRESETS",
        "/run/media/AudioHaven/Signal/Libraries/Presets/FTS-Reverb",
    )
}

/// The reverb's library: every bank under the root, as one list.
pub fn load_library() -> (PresetBrowser, String) {
    preset_browser_ui::load_library_tree(&preset_library_root())
}

/// Write a preset to the plugin's parameters, and report anything that did not
/// land.
///
/// A library outlives a build; naming what was dropped beats a preset that
/// silently recalls half of itself.
pub fn apply(
    params: &[(String, f64)],
    handles: &HashMap<String, ParamHandle>,
    mut note: Signal<String>,
) {
    let (applied, unmatched) = preset_browser_ui::apply_to_handles(params, handles);
    if unmatched.is_empty() {
        note.set(String::new());
    } else {
        note.set(format!(
            "{applied} applied; this build has no {}",
            unmatched.join(", ")
        ));
    }
}

/// The preset sidecar: the browser itself.
#[component]
pub fn ReverbPresetSidecar(
    browser: Signal<PresetBrowser>,
    handles: HashMap<String, ParamHandle>,
    note: Signal<String>,
    ink: String,
    accent: String,
) -> Element {
    let message = note.read().clone();

    rsx! {
        div {
            "data-testid": "reverb-presets",
            style: "position:absolute; inset:0; display:flex; flex-direction:column; gap:3px; \
                    padding:6px;",
            if !message.is_empty() {
                div {
                    "data-testid": "reverb-presets-note",
                    style: format!("font-size:8px; opacity:0.6; color:{ink};"),
                    "{message}"
                }
            }
            preset_browser_ui::PresetBrowserPanel {
                browser,
                ink: ink.clone(),
                accent: accent.clone(),
                title: "Reverb Presets".to_string(),
                on_apply: move |p: Vec<(String, f64)>| apply(&p, &handles, note),
            }
        }
    }
}
