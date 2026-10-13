//! The media browser — a right-hand sidebar for auditioning media.
//!
//! Matching REAPER *functionality*, not REAPER's chrome: this panel is a
//! deliberate departure from the pixel-matching the other panels do, per
//! the two references it grew from (TK Media Browser, MIDI Browser — see
//! the daw-ui docs). The UI decisions that are ours:
//!
//! - **Every row carries its own preview.** A waveform or note roll drawn
//!   inline per entry, so a folder reads like a contact sheet instead of
//!   click-row-look-up-at-one-big-pane. The renderers are the arrange
//!   view's own [`Waveform`]-style shapes, reused through
//!   [`ItemPreview`] — one drawing per kind of content, everywhere.
//! - **Metadata is chips, not columns.** BPM, key and duration read off
//!   the *filename* the way sample packs actually encode them
//!   (`"Am 92bpm groove.wav"`), parsed by [`parse_media_name`] — the MIDI
//!   Browser's key-detection trick, kept.
//! - **Selection expands in place.** The selected row grows a taller
//!   preview band rather than opening a second pane.
//!
//! Pure and fixture-driven like every `*Preview`: entries in, markup out.
//! The live half — filesystem scan, audition transport, drag-to-arrange —
//! comes behind it; nothing here assumes a backend.

use crate::components::arrangement_view::{ItemPreview, NotePreview};
use crate::prelude::*;

/// What kind of media an entry is — decides the badge and the renderer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MediaKind {
    Audio,
    Midi,
}

/// One file in the browser.
#[derive(Clone, PartialEq, Debug)]
pub struct MediaEntry {
    /// The filename, as it is on disk.
    pub name: String,
    /// The folder the entry groups under — a display name, not a full
    /// path. Empty means ungrouped.
    pub folder: String,
    pub kind: MediaKind,
    /// Length in seconds, for the duration chip.
    pub duration: f32,
    /// Content preview, same currency as the arrange items'.
    pub preview: Option<ItemPreview>,
}

/// Which shelf the browser is showing.
///
/// **Project** is the files the open project actually uses — every take's
/// source, deduplicated — so the browser doubles as the project's file
/// manager. **Library** is the sample and MIDI assets being auditioned
/// for it. The distinction is TK Media Browser's locations-vs-collections
/// idea folded to the two scopes that matter.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MediaScope {
    Project,
    #[default]
    Library,
}

/// Does an entry answer a search?
///
/// Every whitespace-separated term must land somewhere: the filename, the
/// folder, the kind, or the *parsed* metadata — `f#m` finds
/// `"F#MIN [SHARK].mid"` even though the substring never occurs, because
/// the query is matched against the normalised key too. Case-blind.
pub fn entry_matches(entry: &MediaEntry, query: &str) -> bool {
    let meta = parse_media_name(&entry.name);
    let name = entry.name.to_ascii_lowercase();
    let folder = entry.folder.to_ascii_lowercase();
    let key = meta.key.as_deref().unwrap_or("").to_ascii_lowercase();
    let bpm = meta.bpm.map(|b| b.to_string()).unwrap_or_default();
    let kind = match entry.kind {
        MediaKind::Audio => "audio wav",
        MediaKind::Midi => "midi mid",
    };
    query.split_whitespace().all(|term| {
        let term = term.to_ascii_lowercase();
        name.contains(&term)
            || folder.contains(&term)
            || key == term
            || bpm == term
            || kind.contains(&term)
    })
}

/// The rows the panel shows: filtered, then grouped by folder in first-
/// appearance order. Pure so it is testable — the panel body is a map of
/// this.
pub fn shelve(
    entries: &[MediaEntry],
    query: &str,
    kind: Option<MediaKind>,
) -> Vec<(String, Vec<MediaEntry>)> {
    let mut groups: Vec<(String, Vec<MediaEntry>)> = Vec::new();
    for entry in entries {
        if let Some(k) = kind {
            if entry.kind != k {
                continue;
            }
        }
        if !query.is_empty() && !entry_matches(entry, query) {
            continue;
        }
        match groups.iter_mut().find(|(f, _)| *f == entry.folder) {
            Some((_, list)) => list.push(entry.clone()),
            None => groups.push((entry.folder.clone(), vec![entry.clone()])),
        }
    }
    groups
}

/// What a filename says about its media, in the conventions sample packs
/// actually use.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct MediaMeta {
    /// The name with its extension and any parsed tokens intact — the
    /// display name is the honest filename, the chips are the parse.
    pub bpm: Option<u32>,
    /// `"F#m"`, `"Am"`, `"C"` — normalised to note + optional `m`.
    pub key: Option<String>,
}

/// Read BPM and key out of a filename.
///
/// The conventions, from actual pack names: BPM is a number glued to
/// `bpm` (`92bpm`, `bpm140`) or a bare 2–3 digit token in the plausible
/// range; a key is a note letter, optional accidental, and an optional
/// minor mark (`F#MIN`, `Am`, `d min`). Case-blind, order-blind. A token
/// that parses as both (`120`) is a BPM — nobody writes a key as digits.
pub fn parse_media_name(name: &str) -> MediaMeta {
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    let mut meta = MediaMeta::default();

    for raw in stem.split(|c: char| !c.is_ascii_alphanumeric() && c != '#') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        let lower = token.to_ascii_lowercase();

        // BPM: `92bpm` / `bpm92` / a bare plausible number.
        if meta.bpm.is_none() {
            let digits: String = lower.chars().filter(|c| c.is_ascii_digit()).collect();
            let rest: String = lower.chars().filter(|c| c.is_ascii_alphabetic()).collect();
            if !digits.is_empty() && (rest == "bpm" || rest.is_empty()) {
                if let Ok(n) = digits.parse::<u32>() {
                    let plausible = (40..=250).contains(&n);
                    if rest == "bpm" || plausible {
                        meta.bpm = Some(n);
                        continue;
                    }
                }
            }
        }

        // Key: note letter + optional #/b + optional minor mark.
        if meta.key.is_none() {
            if let Some(key) = parse_key(&lower) {
                meta.key = Some(key);
            }
        }
    }
    meta
}

/// `"f#min"` → `"F#m"`, `"am"` → `"Am"`, `"c"` → `"C"`. `None` when the
/// token is not a key — which is most tokens, so this refuses eagerly.
fn parse_key(token: &str) -> Option<String> {
    let mut chars = token.chars();
    let note = chars.next()?;
    if !('a'..='g').contains(&note) {
        return None;
    }
    let rest: String = chars.collect();
    let (accidental, rest) = match rest.strip_prefix('#') {
        Some(r) => ("#", r),
        None => match rest.strip_prefix('b') {
            // `bb…` could be the word "bass"; only accept a lone flat.
            Some(r) if r.len() <= 3 => ("b", r),
            _ => ("", rest.as_str()),
        },
    };
    let minor = matches!(rest, "m" | "min" | "minor");
    if !rest.is_empty() && !minor && rest != "maj" && rest != "major" {
        return None;
    }
    Some(format!(
        "{}{}{}",
        note.to_ascii_uppercase(),
        accidental,
        if minor { "m" } else { "" }
    ))
}

/// The browser panel.
///
/// Owns its interaction state — the props seed it, clicks and typing move
/// it — so the same component is a controlled screenshot in a test and a
/// working browser in the app.
#[component]
pub fn MediaBrowserPanel(
    /// The open project's files — every take's source, deduplicated.
    #[props(default)]
    project: Vec<MediaEntry>,
    /// The sample/MIDI assets being auditioned.
    #[props(default)]
    library: Vec<MediaEntry>,
    /// The shelf shown first.
    #[props(default)]
    scope: MediaScope,
    /// The search the panel starts with.
    #[props(default)]
    query: String,
    /// The selected entry, by name, expanded in place.
    #[props(default)]
    selected: Option<String>,
    width: f32,
    height: f32,
) -> Element {
    let t = daw_theme::Theme::default();
    // A raised surface, deliberately one step above the arrange it sits
    // beside — this panel is furniture, not project.
    let bg = t.chrome.surface_raised.css();
    let rule = t.chrome.surface_sunken.shade(-0.2).css();
    let field = t.chrome.surface_sunken.css();
    let ink = t.chrome.text.css();
    let dim = t.chrome.text_dim.css();
    let accent = t.chrome.accent.css();

    let mut scope = use_signal(move || scope);
    let mut query = use_signal(move || query);
    let mut kind_filter = use_signal(|| Option::<MediaKind>::None);
    let mut selected = use_signal(move || selected);

    let entries = match scope() {
        MediaScope::Project => &project,
        MediaScope::Library => &library,
    };
    let total = entries.len();
    let q = query();
    let shelves = shelve(entries, &q, kind_filter());
    let shown: usize = shelves.iter().map(|(_, l)| l.len()).sum();
    let footer = if shown == total {
        format!("{total} files")
    } else {
        format!("{shown} of {total} files")
    };

    let tab = |this: MediaScope, label: &str| {
        let on = scope() == this;
        let (colour, weight, line) = if on {
            (ink.clone(), 700, accent.clone())
        } else {
            (dim.clone(), 400, "transparent".to_string())
        };
        rsx! {
            div {
                style: "flex:1 1 0; text-align:center; padding:5px 0 4px 0; \
                        font-size:10px; font-weight:{weight}; color:{colour}; \
                        border-bottom:2px solid {line}; cursor:pointer; \
                        letter-spacing:0.04em; \
                        font-family:Fira Sans, DejaVu Sans, sans-serif;",
                onclick: move |_| scope.set(this),
                "{label}"
            }
        }
    };
    let chip = |this: Option<MediaKind>, label: &str| {
        let on = kind_filter() == this;
        let (colour, border) = if on {
            (ink.clone(), accent.clone())
        } else {
            (dim.clone(), rule.clone())
        };
        rsx! {
            div {
                style: "flex:0 0 auto; font-size:8px; color:{colour}; \
                        border:1px solid {border}; border-radius:8px; \
                        padding:1px 7px; line-height:11px; cursor:pointer; \
                        font-family:Fira Sans, DejaVu Sans, sans-serif;",
                onclick: move |_| kind_filter.set(this),
                "{label}"
            }
        }
    };

    rsx! {
        div {
            style: "width:{width}px; height:{height}px; display:flex; \
                    flex-direction:column; background:{bg}; \
                    border-left:1px solid {rule}; overflow:hidden;",

            // ── The two shelves ──
            div {
                style: "flex:0 0 auto; display:flex; border-bottom:1px solid {rule};",
                {tab(MediaScope::Project, "PROJECT")}
                {tab(MediaScope::Library, "LIBRARY")}
            }

            // ── Search + kind chips ──
            div {
                style: "flex:0 0 auto; padding:7px 10px 6px 10px;",
                div {
                    style: "height:22px; background:{field}; border-radius:11px; \
                            display:flex; align-items:center; padding:0 9px; \
                            margin-bottom:6px;",
                    svg {
                        width: "10", height: "10", view_box: "0 0 10 10",
                        xmlns: "http://www.w3.org/2000/svg",
                        circle { cx: "4.2", cy: "4.2", r: "3.2", fill: "none",
                                 stroke: "{dim}", stroke_width: "1.4" }
                        path { d: "M 6.6 6.6 L 9.2 9.2", stroke: "{dim}",
                               stroke_width: "1.4", stroke_linecap: "round" }
                    }
                    input {
                        style: "margin-left:6px; flex:1 1 auto; min-width:0; \
                                font-size:10px; color:{ink}; background:transparent; \
                                border:none; outline:none; \
                                font-family:Fira Sans, DejaVu Sans, sans-serif;",
                        r#type: "text",
                        placeholder: "Search name, key, bpm…",
                        value: "{q}",
                        oninput: move |evt| query.set(evt.value()),
                    }
                }
                div {
                    style: "display:flex; gap:4px;",
                    {chip(None, "All")}
                    {chip(Some(MediaKind::Audio), "Audio")}
                    {chip(Some(MediaKind::Midi), "MIDI")}
                }
            }

            // ── The shelves' rows, grouped by folder ──
            div {
                style: "flex:1 1 0; min-height:0; overflow:hidden; padding:2px 6px;",
                if shown == 0 {
                    div {
                        style: "padding:18px 10px; font-size:10px; color:{dim}; \
                                text-align:center; \
                                font-family:Fira Sans, DejaVu Sans, sans-serif;",
                        if total == 0 && scope() == MediaScope::Project {
                            "No media in this project yet"
                        } else if total == 0 {
                            "No library locations added"
                        } else {
                            "Nothing matches"
                        }
                    }
                }
                for (folder, list) in shelves.iter() {
                    if !folder.is_empty() {
                        div {
                            key: "h{folder}",
                            style: "padding:5px 4px 2px 4px; font-size:9px; \
                                    color:{dim}; letter-spacing:0.03em; \
                                    font-family:Fira Sans, DejaVu Sans, sans-serif;",
                            "▾ {folder}"
                        }
                    }
                    for entry in list.iter() {
                        {
                            let name = entry.name.clone();
                            rsx! {
                                div {
                                    key: "{folder}/{entry.name}",
                                    onclick: move |_| selected.set(Some(name.clone())),
                                    MediaRow {
                                        entry: entry.clone(),
                                        selected: selected.read().as_deref()
                                            == Some(entry.name.as_str()),
                                        width: width - 12.0,
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Footer ──
            div {
                style: "flex:0 0 auto; padding:5px 10px; font-size:9px; color:{dim}; \
                        border-top:1px solid {rule}; \
                        font-family:Fira Sans, DejaVu Sans, sans-serif;",
                "{footer}"
            }
        }
    }
}

/// One entry: badge, name, chips, and the inline preview.
#[component]
fn MediaRow(entry: MediaEntry, selected: bool, width: f32) -> Element {
    let t = daw_theme::Theme::default();
    let meta = parse_media_name(&entry.name);

    let ink = t.chrome.text.shade(0.25).css();
    let dim = t.chrome.text_dim.css();
    let row_bg = if selected {
        t.chrome.surface_sunken.shade(0.06).css()
    } else {
        "transparent".to_string()
    };
    let edge = if selected {
        t.chrome.accent.css()
    } else {
        "transparent".to_string()
    };
    // The preview inks: audio in the accent family, MIDI in the solo
    // amber — the two content kinds stay tellable apart at a glance.
    let mark = match entry.kind {
        MediaKind::Audio => t.chrome.accent.shade(-0.15),
        MediaKind::Midi => t.signal.solo.shade(-0.05),
    };
    let (badge, badge_ink) = match entry.kind {
        MediaKind::Audio => ("WAV", t.chrome.accent.css()),
        MediaKind::Midi => ("MID", t.signal.solo.css()),
    };
    // Selection expands in place: the preview band is the part that grows.
    let band_h = if selected { 44.0 } else { 20.0 };
    let dur = format_duration(entry.duration);

    rsx! {
        div {
            style: "position:relative; margin:2px 0; padding:4px 6px 4px 8px; \
                    background:{row_bg}; border-radius:4px; \
                    border-left:2px solid {edge}; overflow:hidden;",

            // Name line: badge, name, chips.
            div {
                style: "display:flex; align-items:center; gap:5px; \
                        margin-bottom:3px;",
                div {
                    style: "flex:0 0 auto; font-size:7px; font-weight:700; \
                            color:{badge_ink}; letter-spacing:0.08em; \
                            border:1px solid {badge_ink}; border-radius:2px; \
                            padding:0 3px; line-height:9px; opacity:0.85; \
                            font-family:Fira Sans, DejaVu Sans, sans-serif;",
                    "{badge}"
                }
                div {
                    style: "flex:1 1 auto; min-width:0; font-size:10px; color:{ink}; \
                            white-space:nowrap; overflow:hidden; \
                            text-overflow:ellipsis; \
                            font-family:Fira Sans, DejaVu Sans, sans-serif;",
                    "{entry.name}"
                }
                if let Some(bpm) = meta.bpm {
                    MetaChip { text: format!("{bpm}") }
                }
                if let Some(key) = &meta.key {
                    MetaChip { text: key.clone() }
                }
                div {
                    style: "flex:0 0 auto; font-size:8px; color:{dim}; \
                            font-variant-numeric:tabular-nums; \
                            font-family:DejaVu Sans Mono, monospace;",
                    "{dur}"
                }
            }

            // The inline preview — the same drawings the arrange items use.
            div {
                style: "position:relative; height:{band_h}px;",
                match &entry.preview {
                    Some(ItemPreview::Waveform(amps)) if !amps.is_empty() => rsx! {
                        crate::components::arrangement_view::Waveform {
                            amps: amps.clone(),
                            width: width - 16.0,
                            top: 0.0,
                            height: band_h,
                            colour: mark.css(),
                        }
                    },
                    Some(ItemPreview::Notes(notes)) if !notes.is_empty() => rsx! {
                        crate::components::arrangement_view::NoteRoll {
                            notes: notes.clone(),
                            length: entry.duration,
                            width: width - 16.0,
                            top: 0.0,
                            height: band_h,
                            colour: mark.css(),
                        }
                    },
                    _ => rsx! {
                        div {
                            style: "height:{band_h}px; border-radius:2px; \
                                    background:rgba(0,0,0,0.12);",
                        }
                    },
                }
            }
        }
    }
}

/// A small rounded metadata chip.
#[component]
fn MetaChip(text: String) -> Element {
    let t = daw_theme::Theme::default();
    let ink = t.chrome.text_dim.shade(0.15).css();
    let bg = t.chrome.surface_sunken.shade(0.04).css();
    rsx! {
        div {
            style: "flex:0 0 auto; font-size:8px; color:{ink}; background:{bg}; \
                    border-radius:7px; padding:1px 5px; line-height:10px; \
                    font-family:Fira Sans, DejaVu Sans, sans-serif;",
            "{text}"
        }
    }
}

fn format_duration(secs: f32) -> String {
    if secs >= 60.0 {
        format!("{}:{:04.1}", (secs / 60.0) as u32, secs % 60.0)
    } else {
        format!("{secs:.1}s")
    }
}

/// The live browser: the panel fed from the connected DAW.
///
/// The **Project** shelf is real: every take's source file, walked off
/// the item list, deduplicated by path, grouped by its parent folder, and
/// carrying the same content preview the arrange items draw (audio
/// through the reapeaks-backed `take_peaks`, MIDI through the take's
/// notes). The **Library** shelf stays empty until a filesystem/location
/// service exists — the panel says so rather than pretending.
#[component]
pub fn MediaBrowser() -> Element {
    let mut project_files = use_signal(Vec::<MediaEntry>::new);
    let mut size = use_signal(|| Option::<(f32, f32)>::None);

    use_future(move || async move {
        let Some(project) = crate::controls::reach::connected_project().await else {
            return;
        };
        loop {
            if let Ok(items) = project.items().all().await {
                let mut entries: Vec<MediaEntry> = Vec::new();
                for item in &items {
                    let Ok(Some(handle)) = project.items().by_guid(&item.guid).await else {
                        continue;
                    };
                    let Ok(info) = handle.active_take().info().await else {
                        continue;
                    };
                    // MIDI takes have no file; they list under the take's
                    // own name so the project shelf is complete.
                    let (name, folder) = match &info.source_file_path {
                        Some(path) => split_path(path),
                        None if info.source_type == daw_proto::SourceType::Midi => {
                            (format!("{}.mid", info.name), "In-project MIDI".to_string())
                        }
                        None => continue,
                    };
                    if entries.iter().any(|e| e.name == name && e.folder == folder) {
                        continue;
                    }
                    let kind = match info.source_type {
                        daw_proto::SourceType::Audio => MediaKind::Audio,
                        daw_proto::SourceType::Midi => MediaKind::Midi,
                        _ => continue,
                    };
                    let duration = info
                        .source_length
                        .map(|d| d.as_seconds() as f32)
                        .unwrap_or(item.length.as_seconds() as f32);
                    let preview =
                        crate::components::arrangement_view::fetch_preview(&project, item).await;
                    entries.push(MediaEntry {
                        name,
                        folder,
                        kind,
                        duration,
                        preview,
                    });
                }
                project_files.set(entries);
            }
            futures_timer::Delay::new(std::time::Duration::from_secs(5)).await;
        }
    });

    let (w, h) = size.read().unwrap_or((260.0, 600.0));
    rsx! {
        div {
            style: "height:100%; width:100%; overflow:hidden;",
            onmounted: move |evt| {
                spawn(async move {
                    if let Ok(rect) = evt.get_client_rect().await {
                        if rect.size.width > 0.0 && rect.size.height > 0.0 {
                            size.set(Some((rect.size.width as f32, rect.size.height as f32)));
                        }
                    }
                });
            },
            MediaBrowserPanel {
                project: project_files.read().clone(),
                scope: MediaScope::Project,
                width: w,
                height: h,
            }
        }
    }
}

/// A path's file name and its parent folder's display name.
fn split_path(path: &str) -> (String, String) {
    let path = path.replace('\\', "/");
    let mut parts = path.rsplit('/');
    let name = parts.next().unwrap_or(&path).to_string();
    let folder = parts.next().unwrap_or("").to_string();
    (name, folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The conventions the references' packs actually use, and the traps:
    /// a bare number is a BPM only in range, `bass` is not B-flat, and
    /// `120` never reads as a key.
    #[test]
    fn filenames_give_up_their_bpm_and_key() {
        let m = parse_media_name("F#MIN 92bpm [SHARK].mid");
        assert_eq!(m.bpm, Some(92));
        assert_eq!(m.key.as_deref(), Some("F#m"));

        let m = parse_media_name("Am_140_groove.wav");
        assert_eq!(m.bpm, Some(140));
        assert_eq!(m.key.as_deref(), Some("Am"));

        let m = parse_media_name("kick_punchy.wav");
        assert_eq!(m.bpm, None);
        assert_eq!(m.key, None);

        // "bass" must not read as B-flat; 808 is not a plausible BPM.
        let m = parse_media_name("bass_808_sub.wav");
        assert_eq!(m.key, None);
        assert_eq!(m.bpm, None);

        let m = parse_media_name("bpm175 D loop.wav");
        assert_eq!(m.bpm, Some(175));
        assert_eq!(m.key.as_deref(), Some("D"));
    }

    #[test]
    fn durations_format_both_sides_of_a_minute() {
        assert_eq!(format_duration(3.4), "3.4s");
        assert_eq!(format_duration(83.0), "1:23.0");
    }

    fn entry(name: &str, folder: &str, kind: MediaKind) -> MediaEntry {
        MediaEntry {
            name: name.into(),
            folder: folder.into(),
            kind,
            duration: 1.0,
            preview: None,
        }
    }

    /// Search matches the *parse*, not just the substring: `f#m` finds a
    /// file spelling its key `F#MIN`, `92` finds the bpm token, and every
    /// term of a multi-word query must land.
    #[test]
    fn search_reaches_the_parsed_metadata() {
        let e = entry("F#MIN 92bpm [SHARK].mid", "Loops", MediaKind::Midi);
        assert!(entry_matches(&e, "f#m"));
        assert!(entry_matches(&e, "92"));
        assert!(entry_matches(&e, "shark 92"));
        assert!(entry_matches(&e, "loops"));
        assert!(entry_matches(&e, "midi"));
        assert!(!entry_matches(&e, "am"), "F#m is not A minor");
        assert!(
            !entry_matches(&e, "shark 93"),
            "one dead term kills the match"
        );
    }

    /// Shelving filters then groups, keeping folder first-appearance
    /// order — the shape the panel body maps over.
    #[test]
    fn shelving_filters_and_groups() {
        let entries = vec![
            entry("kick.wav", "Drums", MediaKind::Audio),
            entry("riff.mid", "Loops", MediaKind::Midi),
            entry("snare.wav", "Drums", MediaKind::Audio),
        ];

        let all = shelve(&entries, "", None);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].0, "Drums");
        assert_eq!(all[0].1.len(), 2);
        assert_eq!(all[1].0, "Loops");

        let audio = shelve(&entries, "", Some(MediaKind::Audio));
        assert_eq!(audio.len(), 1, "the empty Loops group is not shown");

        let hit = shelve(&entries, "snare", None);
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].1[0].name, "snare.wav");
    }

    /// Windows and POSIX paths both split into name + parent folder.
    #[test]
    fn paths_split_into_name_and_folder() {
        assert_eq!(
            split_path("/media/Loops/kick.wav"),
            ("kick.wav".to_string(), "Loops".to_string())
        );
        assert_eq!(
            split_path(r"C:\Samples\Drums\snare.wav"),
            ("snare.wav".to_string(), "Drums".to_string())
        );
    }
}
