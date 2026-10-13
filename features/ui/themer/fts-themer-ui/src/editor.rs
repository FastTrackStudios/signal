//! The editor shell: group nav, swatch grid, and the live preview.

use dioxus::prelude::*;
use fts_themer::{Group, ThemeIni, color::Rgb, groups};

use crate::preview::Preview;
use crate::server::{ThemeSources, add_accent, load_theme, save_theme};

/// Where the theme lives, relative to the repo root the server runs from.
const DEFAULT_THEME: &str = "features/reaper/fts-theme";

/// Editing state, shared down the tree.
#[derive(Clone, Copy)]
pub struct Editor {
    /// The `.ReaperTheme` text, parsed. Every edit rewrites one line of it.
    pub ini: Signal<ThemeIni>,
    /// `rtconfig.txt`, read-only here — the preview's WALTER program.
    pub rtconfig: Signal<String>,
    /// Keys changed since the last save.
    pub dirty: Signal<Vec<String>>,
}

impl Editor {
    /// Set a color and mark its key dirty.
    fn set(&mut self, key: &str, color: Rgb) {
        self.ini.write().set_color(key, color);
        let mut dirty = self.dirty.write();
        if !dirty.iter().any(|k| k == key) {
            dirty.push(key.to_string());
        }
    }
}

#[component]
pub fn ThemeEditor() -> Element {
    let sources = use_resource(|| load_theme(DEFAULT_THEME.to_string()));

    match &*sources.read_unchecked() {
        Some(Ok(loaded)) => rsx! { Loaded { sources: loaded.clone() } },
        Some(Err(e)) => rsx! {
            div { class: "fatal",
                h1 { "Couldn't load the theme" }
                pre { "{e}" }
                p { "Expected an unpacked theme at " code { "{DEFAULT_THEME}" } "." }
            }
        },
        // Resource still resolving.
        None => rsx! { div { class: "loading", "Loading theme…" } },
    }
}

#[component]
fn Loaded(sources: ThemeSources) -> Element {
    let editor = Editor {
        ini: use_signal(|| ThemeIni::parse(&sources.ini)),
        rtconfig: use_signal(|| sources.rtconfig.clone()),
        dirty: use_signal(Vec::new),
    };
    use_context_provider(|| editor);

    let mut group = use_signal(|| Group::Main);
    let mut filter = use_signal(String::new);

    // Grouping is derived from the key list, which never changes — only
    // values do — so this is computed once rather than per keystroke.
    let grouped = use_memo(move || {
        let ini = editor.ini.read();
        let keys: Vec<String> = ini.keys().into_iter().map(str::to_string).collect();
        groups::group_all(keys.iter().map(String::as_str))
            .into_iter()
            .map(|(g, members)| {
                (
                    g,
                    members.into_iter().map(str::to_string).collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    });

    rsx! {
        div { class: "app",
            Toolbar { name: sources.name.clone(), accents: sources.accents.clone() }
            div { class: "body",
                nav { class: "groups",
                    input {
                        class: "search",
                        r#type: "search",
                        placeholder: "Filter keys…",
                        value: "{filter}",
                        oninput: move |e| filter.set(e.value()),
                    }
                    for (g, members) in grouped.read().iter() {
                        button {
                            key: "{g:?}",
                            class: if *g == group() { "group active" } else { "group" },
                            onclick: {
                                let g = *g;
                                move |_| group.set(g)
                            },
                            span { "{g.label()}" }
                            span { class: "count", "{members.len()}" }
                        }
                    }
                }
                main { class: "swatches",
                    Swatches { group: group(), filter: filter() }
                }
                aside { class: "preview",
                    Preview {}
                }
            }
        }
    }
}

#[component]
fn Swatches(group: Group, filter: String) -> Element {
    let editor = use_context::<Editor>();
    let ini = editor.ini.read();

    let keys: Vec<String> = ini
        .keys()
        .into_iter()
        .filter(|k| groups::classify(k) == group)
        .filter(|k| filter.is_empty() || k.contains(&filter))
        .map(str::to_string)
        .collect();

    if keys.is_empty() {
        return rsx! { p { class: "empty", "No keys match." } };
    }

    rsx! {
        h2 { "{group.label()}" }
        div { class: "grid",
            for key in keys {
                Swatch { key: "{key}", name: key.clone() }
            }
        }
    }
}

/// One palette key. Blend/flag words get a number field, not a color picker —
/// a color input on a drawmode would silently destroy the blend word.
#[component]
fn Swatch(name: String) -> Element {
    let mut editor = use_context::<Editor>();
    let is_color = groups::is_color(&name);
    let raw = editor.ini.read().int(&name).unwrap_or(0);
    let dirty = editor.dirty.read().contains(&name);

    if !is_color {
        return rsx! {
            label { class: if dirty { "swatch mode dirty" } else { "swatch mode" },
                span { class: "key", "{name}" }
                input {
                    r#type: "number",
                    value: "{raw}",
                    oninput: {
                        let name = name.clone();
                        move |e: FormEvent| {
                            if let Ok(v) = e.value().parse::<i32>() {
                                editor.ini.write().set_int(&name, v);
                                let mut d = editor.dirty.write();
                                if !d.contains(&name) { d.push(name.clone()); }
                            }
                        }
                    },
                }
            }
        };
    }

    let hex = Rgb::from_colorref(raw).to_hex();
    rsx! {
        label { class: if dirty { "swatch dirty" } else { "swatch" },
            input {
                r#type: "color",
                value: "{hex}",
                oninput: {
                    let name = name.clone();
                    move |e: FormEvent| {
                        if let Ok(c) = Rgb::parse_hex(&e.value()) {
                            editor.set(&name, c);
                        }
                    }
                },
            }
            span { class: "key", "{name}" }
            span { class: "hex", "{hex}" }
        }
    }
}

#[component]
fn Toolbar(name: String, accents: Vec<String>) -> Element {
    let mut editor = use_context::<Editor>();
    let mut status = use_signal(String::new);
    let mut accents = use_signal(|| accents);

    let mut accent_name = use_signal(String::new);
    let mut accent_color = use_signal(|| "#d1283c".to_string());
    let mut keep_tone = use_signal(|| true);

    let dirty_count = editor.dirty.read().len();

    let save = move |_| async move {
        let text = editor.ini.read().to_text();
        match save_theme(DEFAULT_THEME.to_string(), text).await {
            Ok(path) => {
                editor.dirty.write().clear();
                status.set(format!("Saved {path}"));
            }
            Err(e) => status.set(format!("Save failed: {e}")),
        }
    };

    let generate = move |_| async move {
        let (n, c) = (accent_name(), accent_color());
        if n.trim().is_empty() {
            status.set("Name the accent first.".into());
            return;
        }
        // "blue" is the canonical source: it's the theme's own base accent,
        // so its artwork is the least-processed of the shipped set.
        match add_accent(
            DEFAULT_THEME.to_string(),
            n.clone(),
            c,
            "blue".into(),
            keep_tone(),
        )
        .await
        {
            Ok(files) => {
                accents.write().push(n.clone());
                accent_name.set(String::new());
                status.set(format!("Generated accent {n} ({} files)", files.len()));
            }
            Err(e) => status.set(format!("Accent failed: {e}")),
        }
    };

    rsx! {
        header { class: "toolbar",
            div { class: "title",
                strong { "{name}" }
                span { class: "sub", "REAPER theme" }
            }

            div { class: "spacer" }

            div { class: "accent-tool",
                input {
                    r#type: "color",
                    value: "{accent_color}",
                    oninput: move |e| accent_color.set(e.value()),
                }
                input {
                    class: "accent-name",
                    placeholder: "new accent name",
                    value: "{accent_name}",
                    oninput: move |e| accent_name.set(e.value()),
                }
                label { class: "toggle",
                    input {
                        r#type: "checkbox",
                        checked: keep_tone(),
                        oninput: move |e| keep_tone.set(e.checked()),
                    }
                    "match set tone"
                }
                button { onclick: generate, "Generate accent" }
            }

            span { class: "accents",
                for a in accents.read().iter() {
                    span { key: "{a}", class: "chip", "{a}" }
                }
            }

            button {
                class: if dirty_count > 0 { "save dirty" } else { "save" },
                disabled: dirty_count == 0,
                onclick: save,
                if dirty_count > 0 { "Save {dirty_count} change(s)" } else { "Saved" }
            }

            span { class: "status", "{status}" }
        }
    }
}
