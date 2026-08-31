//! The preset browser panel, shared by the EQ, Reverb and Compressor editors.
//!
//! One panel, three plugins. What differs between them is the palette and what
//! happens when a preset is chosen, so those are the props; everything else —
//! searching, filtering, the list, stepping — is the same problem in all three
//! and lives here once.
//!
//! The browsing itself is [`preset_browser::PresetBrowser`], which is headless
//! and separately tested. This crate is rendering.
//!
//! # Styling
//!
//! Inline styles only, and no stylesheet or asset of any kind. Signal UI has
//! to render identically standalone, as a VST3/CLAP plugin, and embedded in
//! REAPER, and that pipeline (Blitz) does not load external CSS reliably. The
//! palette arrives as props so a panel matches the plugin it is mounted in
//! rather than imposing its own look.

use dioxus::prelude::*;
use preset_browser::PresetBrowser;

/// How a preset is applied, and what the panel should look like doing it.
#[derive(Props, Clone, PartialEq)]
pub struct PresetBrowserProps {
    /// The library and its browsing state. A signal rather than a plain value
    /// so the host can drive it too — a "next preset" footswitch and a click
    /// in the list are the same operation, and both should be visible here.
    pub browser: Signal<PresetBrowser>,
    /// Applied when a preset is chosen: the parameters to send to the DSP,
    /// exactly as `set_named` takes them.
    pub on_apply: EventHandler<Vec<(String, f64)>>,
    /// Foreground colour.
    #[props(default = String::from("#e8e6ef"))]
    pub ink: String,
    /// Highlight for the selection and the match badge.
    #[props(default = String::from("#7aa2f7"))]
    pub accent: String,
    /// Panel title — "Reverb Presets", "EQ Presets".
    #[props(default = String::from("Presets"))]
    pub title: String,
}

/// A preset's match quality, as a short badge.
///
/// Shown because a translated library is not uniformly faithful, and a browser
/// that presents an exact match and a rough one identically is lying by
/// omission. `None` means it was never measured, which is different from
/// measured-and-poor and reads differently.
fn match_badge(error: Option<f64>) -> Option<(String, &'static str)> {
    let e = error?;
    Some(match e {
        e if e <= 0.05 => (format!("{e:.2}"), "#5fd08a"),
        e if e <= 0.15 => (format!("{e:.2}"), "#d9c05a"),
        e => (format!("{e:.2}"), "#e2603f"),
    })
}

#[component]
pub fn PresetBrowserPanel(props: PresetBrowserProps) -> Element {
    let PresetBrowserProps {
        mut browser,
        on_apply,
        ink,
        accent,
        title,
    } = props;

    // Read once per render. `visible` is indices into the library, so a row
    // can be applied without re-deriving anything.
    let (visible, query, sort, categories, selected, total) = {
        let b = browser.read();
        (
            b.visible(),
            b.query().to_string(),
            b.sort_mode(),
            b.categories(),
            b.selected_index(),
            b.all().len(),
        )
    };
    let active_category = browser.read().category_filter().map(str::to_string);

    // Applying is the same whether it came from a click or a step, so it is
    // written once and reused.
    let apply_selected = move |b: Signal<PresetBrowser>| {
        let params = b.read().selected_parameters().to_vec();
        if !params.is_empty() {
            on_apply.call(params);
        }
    };

    rsx! {
        div {
            "data-testid": "preset-browser",
            style: format!(
                "display:flex; flex-direction:column; gap:4px; padding:6px 8px; \
                 height:100%; overflow:hidden; color:{ink}; \
                 background:rgba(0,0,0,0.28); border:1px solid rgba(255,255,255,0.10); \
                 border-radius:3px;"
            ),

            // Title, how many are showing, and the stepper.
            div {
                style: "display:flex; justify-content:space-between; align-items:center; \
                        font-size:8px; letter-spacing:0.10em; text-transform:uppercase; \
                        opacity:0.75;",
                span { "{title}" }
                div {
                    style: "display:flex; gap:6px; align-items:center;",
                    span {
                        "data-testid": "preset-count",
                        "{visible.len()} / {total}"
                    }
                    span {
                        "data-testid": "preset-prev",
                        style: "cursor:pointer; padding:0 3px;",
                        onclick: move |_| {
                            browser.write().select_previous();
                            apply_selected(browser);
                        },
                        "‹"
                    }
                    span {
                        "data-testid": "preset-next",
                        style: "cursor:pointer; padding:0 3px;",
                        onclick: move |_| {
                            browser.write().select_next();
                            apply_selected(browser);
                        },
                        "›"
                    }
                }
            }

            // Search.
            input {
                "data-testid": "preset-search",
                r#type: "text",
                value: "{query}",
                placeholder: "Search",
                style: format!(
                    "width:100%; box-sizing:border-box; font-size:10px; padding:3px 5px; \
                     color:{ink}; background:rgba(255,255,255,0.06); border-radius:2px; \
                     border:1px solid rgba(255,255,255,0.10); outline:none;"
                ),
                oninput: move |e| browser.write().set_query(e.value()),
            }

            // Category filter. "All" first, then whatever the library has —
            // a bank with no categories simply shows nothing here.
            div {
                style: "display:flex; flex-wrap:wrap; gap:3px;",
                CategoryChip {
                    label: "All".to_string(),
                    active: active_category.is_none(),
                    accent: accent.clone(),
                    onpick: move |_| browser.write().set_category_filter(None),
                }
                for category in categories {
                    CategoryChip {
                        key: "{category}",
                        label: category.clone(),
                        active: active_category.as_deref() == Some(category.as_str()),
                        accent: accent.clone(),
                        onpick: {
                            let category = category.clone();
                            move |_| browser.write().set_category_filter(Some(category.clone()))
                        },
                    }
                }
                span {
                    "data-testid": "preset-sort",
                    style: "margin-left:auto; cursor:pointer; font-size:8px; opacity:0.7; \
                            letter-spacing:0.08em; text-transform:uppercase; padding:2px 4px;",
                    onclick: move |_| {
                        let next = browser.read().sort_mode().cycle();
                        browser.write().set_sort_mode(next);
                    },
                    "{sort.label()}"
                }
            }

            // The list.
            div {
                style: "flex:1; overflow-y:auto; display:flex; flex-direction:column; gap:1px;",
                if visible.is_empty() {
                    div {
                        "data-testid": "preset-empty",
                        style: "font-size:10px; opacity:0.55; padding:6px 2px;",
                        // Distinguished, because an empty library and a
                        // filter that matches nothing want different actions.
                        if total == 0 { "No presets in this library." }
                        else { "Nothing matches that search." }
                    }
                }
                for index in visible {
                    PresetRow {
                        key: "{index}",
                        preset_index: index,
                        browser,
                        accent: accent.clone(),
                        ink: ink.clone(),
                        selected: selected == Some(index),
                        onpick: move |i: usize| {
                            browser.write().select(i);
                            apply_selected(browser);
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn CategoryChip(
    label: String,
    active: bool,
    accent: String,
    onpick: EventHandler<()>,
) -> Element {
    rsx! {
        span {
            "data-testid": "preset-category",
            style: format!(
                "cursor:pointer; font-size:8px; padding:2px 5px; border-radius:2px; \
                 letter-spacing:0.06em; text-transform:uppercase; \
                 background:{}; border:1px solid {};",
                if active { format!("{accent}33") } else { "rgba(255,255,255,0.05)".into() },
                if active { accent.clone() } else { "rgba(255,255,255,0.10)".into() },
            ),
            onclick: move |_| onpick.call(()),
            "{label}"
        }
    }
}

#[component]
fn PresetRow(
    preset_index: usize,
    browser: Signal<PresetBrowser>,
    accent: String,
    ink: String,
    selected: bool,
    onpick: EventHandler<usize>,
) -> Element {
    let library = browser.read();
    let Some(preset) = library.all().get(preset_index) else {
        return rsx! {};
    };
    let name = preset.name.clone();
    let category = preset.category.clone().unwrap_or_default();
    let badge = match_badge(preset.match_error);

    rsx! {
        div {
            "data-testid": "preset-entry",
            style: format!(
                "display:flex; justify-content:space-between; align-items:center; gap:6px; \
                 cursor:pointer; font-size:10px; padding:2px 4px; border-radius:2px; \
                 background:{}; color:{};",
                if selected { format!("{accent}2e") } else { "transparent".into() },
                if selected { accent.clone() } else { ink.clone() },
            ),
            onclick: move |_| onpick.call(preset_index),
            span {
                style: "white-space:nowrap; overflow:hidden; text-overflow:ellipsis;",
                "{name}"
            }
            div {
                style: "display:flex; gap:5px; align-items:center; flex-shrink:0;",
                if !category.is_empty() {
                    span {
                        style: "font-size:8px; opacity:0.55; letter-spacing:0.05em;",
                        "{category}"
                    }
                }
                if let Some((text, colour)) = badge {
                    span {
                        "data-testid": "preset-match",
                        style: format!("font-size:8px; color:{colour}; opacity:0.9;"),
                        "{text}"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_match_badge_grades_rather_than_just_reporting() {
        // Colour carries the judgement so a glance is enough.
        assert_eq!(match_badge(Some(0.01)).unwrap().1, "#5fd08a");
        assert_eq!(match_badge(Some(0.10)).unwrap().1, "#d9c05a");
        assert_eq!(match_badge(Some(0.90)).unwrap().1, "#e2603f");
        // Never measured is not the same as measured badly, and shows nothing.
        assert!(match_badge(None).is_none());
    }
}

/// Apply a preset's parameters through the host, by name.
///
/// Returns `(applied, unmatched)` — a library is shared across plugin
/// versions, and a preset naming a parameter this build does not have should
/// set the rest rather than fail. The caller can surface the unmatched names;
/// silently dropping them is how a preset quietly stops recalling correctly.
///
/// Each write is a complete gesture (begin / set / end), so the DAW records it
/// as an edit and automation lanes see it. Values are the parameter's own
/// plain units — the preset stores 1.94 seconds, not 0.41 normalized — and are
/// converted through the parameter's own parser, which is the only thing that
/// knows its curve.
///
/// Lives here rather than in each `*-ui` crate deliberately: the adapter this
/// builds on carries a note that it was copied verbatim into seven crates
/// before being lifted, and the copies drifted.
pub fn apply_to_handles(
    parameters: &[(String, f64)],
    handles: &std::collections::HashMap<String, fts_audio_ui::ParamHandle>,
) -> (usize, Vec<String>) {
    let mut applied = 0;
    let mut unmatched = Vec::new();
    for (name, value) in parameters {
        let Some(handle) = handles.get(name) else {
            unmatched.push(name.clone());
            continue;
        };
        match handle.string_to_normalized(&format!("{value}")) {
            Some(normalized) => {
                handle.set_as_gesture(normalized);
                applied += 1;
            }
            // The parameter exists but could not read that value — a range
            // that moved between versions, say. Report it like a missing one.
            None => unmatched.push(name.clone()),
        }
    }
    (applied, unmatched)
}

#[cfg(test)]
mod apply_tests {
    use super::*;
    use fts_audio_ui::ParamHandle;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// A handle that records the gesture it was given.
    fn recording(name: &str, log: Arc<Mutex<Vec<(String, f32)>>>) -> ParamHandle {
        let n = name.to_string();
        let begin_log = log.clone();
        let set_log = log.clone();
        let end_log = log.clone();
        let (b, s, e) = (n.clone(), n.clone(), n.clone());
        ParamHandle::new(
            || 0.0,
            move || begin_log.lock().unwrap().push((format!("begin:{b}"), 0.0)),
            move |v| set_log.lock().unwrap().push((format!("set:{s}"), v)),
            move || end_log.lock().unwrap().push((format!("end:{e}"), 0.0)),
            || String::new(),
            move || n.clone(),
            // Stands in for a 0..4 parameter: the preset stores plain units
            // and only the parameter knows how to normalize them. Asserting
            // the converted value is the point — handing `set_as_gesture` an
            // unconverted 1.94 would simply clamp to 1.0.
            |text| text.parse::<f32>().ok().map(|v| v / 4.0),
        )
    }

    #[test]
    fn every_named_parameter_is_written_as_a_complete_gesture() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut handles = HashMap::new();
        handles.insert("decay_time".to_string(), recording("decay_time", log.clone()));
        handles.insert("mix".to_string(), recording("mix", log.clone()));

        let (applied, unmatched) = apply_to_handles(
            &[("decay_time".into(), 1.94), ("mix".into(), 0.5)],
            &handles,
        );
        assert_eq!(applied, 2);
        assert!(unmatched.is_empty());

        let calls = log.lock().unwrap().clone();
        // A DAW needs the begin/end around the set to record the edit.
        assert_eq!(calls.len(), 6);
        assert_eq!(calls[0].0, "begin:decay_time");
        // 1.94 seconds through the parameter's own curve, not the raw number.
        assert_eq!(calls[1], ("set:decay_time".to_string(), 1.94 / 4.0));
        assert_eq!(calls[2].0, "end:decay_time");
    }

    #[test]
    fn an_unknown_parameter_is_reported_and_the_rest_still_apply() {
        // A library outlives a build; a preset naming something this version
        // dropped must not cost the parameters it does have.
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut handles = HashMap::new();
        handles.insert("mix".to_string(), recording("mix", log.clone()));

        let (applied, unmatched) = apply_to_handles(
            &[("gravity_well".into(), 1.0), ("mix".into(), 0.25)],
            &handles,
        );
        assert_eq!(applied, 1);
        assert_eq!(unmatched, vec!["gravity_well".to_string()]);
        assert_eq!(log.lock().unwrap()[1], ("set:mix".to_string(), 0.25 / 4.0));
    }

    #[test]
    fn a_value_the_parameter_cannot_read_counts_as_unmatched() {
        let log: Arc<Mutex<Vec<(String, f32)>>> = Arc::new(Mutex::new(Vec::new()));
        let mut handles = HashMap::new();
        // A parser that rejects everything stands in for a range that moved.
        handles.insert(
            "mix".to_string(),
            ParamHandle::new(
                || 0.0,
                || {},
                |_| {},
                || {},
                || String::new(),
                || "mix".to_string(),
                |_| None,
            ),
        );
        let (applied, unmatched) = apply_to_handles(&[("mix".into(), 0.5)], &handles);
        assert_eq!(applied, 0);
        assert_eq!(unmatched, vec!["mix".to_string()]);
        assert!(log.lock().unwrap().is_empty());
    }
}

/// The always-visible preset strip.
///
/// A plugin should say what it is currently set to without the user opening
/// anything — the browser is for finding a preset, this is for knowing which
/// one you are on and stepping to the next. Sits across the top of the editor,
/// above the panel and beside nothing: the space selector is already the side
/// rail, and these are different questions.
#[component]
pub fn PresetBar(
    browser: Signal<PresetBrowser>,
    /// Applied when the strip steps to another preset.
    on_apply: EventHandler<Vec<(String, f64)>>,
    /// Open or close the browser.
    on_browse: EventHandler<()>,
    /// Whether the browser is currently open, so the control can show it.
    #[props(default = false)]
    browsing: bool,
    #[props(default = String::from("#e8e6ef"))] ink: String,
    #[props(default = String::from("#7aa2f7"))] accent: String,
) -> Element {
    let mut browser_signal = browser;
    let (name, category, badge, has_selection) = {
        let b = browser.read();
        match b.selected() {
            Some(p) => (
                p.name.clone(),
                p.category.clone().unwrap_or_default(),
                match_badge(p.match_error),
                true,
            ),
            // Never blank: an editor with no preset chosen should say so
            // rather than leave the reader wondering what it is set to.
            None => ("Init".to_string(), String::new(), None, false),
        }
    };

    let mut step = move |delta: isize| {
        let params = {
            let mut b = browser_signal.write();
            b.step(delta);
            b.selected_parameters().to_vec()
        };
        if !params.is_empty() {
            on_apply.call(params);
        }
    };

    rsx! {
        div {
            "data-testid": "preset-bar",
            style: format!(
                "display:flex; align-items:center; gap:8px; height:26px; flex:none; \
                 padding:0 8px; color:{ink}; \
                 border-bottom:1px solid rgba(255,255,255,0.10); \
                 background:rgba(0,0,0,0.22);"
            ),
            span {
                "data-testid": "preset-bar-prev",
                style: "cursor:pointer; font-size:13px; opacity:0.8; padding:0 3px;",
                onclick: move |_| step(-1),
                "‹"
            }
            span {
                "data-testid": "preset-bar-next",
                style: "cursor:pointer; font-size:13px; opacity:0.8; padding:0 3px;",
                onclick: move |_| step(1),
                "›"
            }
            div {
                style: "display:flex; align-items:baseline; gap:6px; min-width:0; flex:1;",
                span {
                    "data-testid": "preset-bar-name",
                    style: format!(
                        "font-size:11px; font-weight:700; white-space:nowrap; \
                         overflow:hidden; text-overflow:ellipsis; \
                         opacity:{};",
                        if has_selection { "1.0" } else { "0.6" },
                    ),
                    "{name}"
                }
                if !category.is_empty() {
                    span {
                        style: "font-size:8px; opacity:0.5; letter-spacing:0.06em; \
                                text-transform:uppercase; white-space:nowrap;",
                        "{category}"
                    }
                }
                if let Some((text, colour)) = badge {
                    span {
                        "data-testid": "preset-bar-match",
                        style: format!("font-size:8px; color:{colour};"),
                        "{text}"
                    }
                }
            }
            span {
                "data-testid": "preset-bar-browse",
                style: format!(
                    "cursor:pointer; font-size:8px; letter-spacing:0.08em; \
                     text-transform:uppercase; padding:2px 6px; border-radius:2px; \
                     background:{}; border:1px solid {};",
                    if browsing { format!("{accent}33") } else { "rgba(255,255,255,0.05)".into() },
                    if browsing { accent.clone() } else { "rgba(255,255,255,0.10)".into() },
                ),
                onclick: move |_| on_browse.call(()),
                "Browse"
            }
        }
    }
}

/// Load every bank under a library root into one browser.
///
/// Banks are subdirectories, and they arrive as a single library rather than
/// behind a picker: the question a user has is "what sound do I want", not
/// "which batch did it come from". Each preset carries its own origin and
/// category, so a filter still separates them. The root itself is scanned too,
/// for a library that is just a folder of files.
///
/// Returns the browser and a note for anything the user should know about the
/// load — an empty library, or files that could not be read. Shared rather
/// than written per plugin, because "where are my presets" has the same answer
/// everywhere and only the root differs.
pub fn load_library_tree(root: &std::path::Path) -> (PresetBrowser, String) {
    let mut presets = Vec::new();
    let mut skipped = 0usize;

    let mut banks: Vec<std::path::PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    banks.sort();
    banks.insert(0, root.to_path_buf());

    for bank in banks {
        if let Ok(report) = preset_browser::load_directory(&bank) {
            skipped += report.skipped.len();
            presets.extend(report.presets);
        }
    }

    let note = if presets.is_empty() {
        format!("No presets under {}", root.display())
    } else if skipped > 0 {
        format!("{skipped} preset file(s) could not be read")
    } else {
        String::new()
    };

    (PresetBrowser::new(presets), note)
}

/// Resolve a library root from an environment override, falling back to a
/// default path.
///
/// Every plugin wants the same escape hatch — the tests point it at a fixture,
/// and a user can point it at a library that is not on the default path.
pub fn library_root(env_var: &str, default: &str) -> std::path::PathBuf {
    match std::env::var_os(env_var) {
        Some(dir) => std::path::PathBuf::from(dir),
        None => std::path::PathBuf::from(default),
    }
}
