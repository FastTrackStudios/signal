//! Kontakt-style sampler view.
//!
//! Layout:
//! ```text
//! ┌──────────────┬──────────────────────────────────────────┐
//! │ Library      │  Instrument header (name + toolbar)      │
//! │ Browser      ├──────────────────────────────────────────┤
//! │              │  Tab bar: Groups│Instr│Mapper│Mod│FX     │
//! │  - Library 1 ├──────────────────────────────────────────┤
//! │    - Patch   │                                          │
//! │  - Library 2 │  Tab content area                        │
//! │              │                                          │
//! ├──────────────┴──────────────────────────────────────────┤
//! │  Piano keyboard + falling-note waterfall                │
//! └─────────────────────────────────────────────────────────┘
//! ```

use dioxus::prelude::*;

use midicore::midir::MidiStream;

use crate::components::piano::{Piano, WaterfallNote, WaterfallState};

// ── Colour tokens (dark Kontakt-style palette) ─────────────────────────────

const BG_BODY: &str = "#111111";
const BG_PANEL: &str = "#1c1c1e";
const BG_SURFACE: &str = "#252527";
const BG_ELEVATED: &str = "#2c2c2e";
const BG_ACTIVE: &str = "#383838";
const ACCENT: &str = "#e8813a"; // Kontakt orange
const ACCENT_DIM: &str = "#8a4d22";
const TEXT: &str = "#d4d4d4";
const TEXT_BRIGHT: &str = "#ffffff";
const TEXT_DIM: &str = "#777779";
const BORDER: &str = "#3a3a3c";
const BORDER_LIGHT: &str = "#4a4a4c";

// ── Domain model stubs (will be replaced with real SamplerRig state) ───

#[derive(Clone, PartialEq)]
struct LibraryEntry {
    id: usize,
    name: &'static str,
    vendor: &'static str,
    patch_count: usize,
    expanded: bool,
    patches: Vec<PatchEntry>,
}

#[derive(Clone, PartialEq)]
struct PatchEntry {
    id: usize,
    name: &'static str,
}

fn demo_libraries() -> Vec<LibraryEntry> {
    vec![
        LibraryEntry {
            id: 0,
            name: "Cinematic Strings",
            vendor: "FastTrack Studio",
            patch_count: 5,
            expanded: true,
            patches: vec![
                PatchEntry {
                    id: 0,
                    name: "1st Violins — Sustain",
                },
                PatchEntry {
                    id: 1,
                    name: "2nd Violins — Sustain",
                },
                PatchEntry {
                    id: 2,
                    name: "Violas — Sustain",
                },
                PatchEntry {
                    id: 3,
                    name: "Cellos — Sustain",
                },
                PatchEntry {
                    id: 4,
                    name: "Basses — Sustain",
                },
            ],
        },
        LibraryEntry {
            id: 1,
            name: "Brass Section",
            vendor: "FastTrack Studio",
            patch_count: 3,
            expanded: false,
            patches: vec![
                PatchEntry {
                    id: 5,
                    name: "French Horns",
                },
                PatchEntry {
                    id: 6,
                    name: "Trumpets",
                },
                PatchEntry {
                    id: 7,
                    name: "Trombones",
                },
            ],
        },
        LibraryEntry {
            id: 2,
            name: "Grand Piano",
            vendor: "FastTrack Studio",
            patch_count: 2,
            expanded: false,
            patches: vec![
                PatchEntry {
                    id: 8,
                    name: "Steinway — Full",
                },
                PatchEntry {
                    id: 9,
                    name: "Steinway — Close",
                },
            ],
        },
    ]
}

// ── Instrument tab ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum InstrTab {
    Groups,
    Instrument,
    Mapper,
    Modulation,
    Fx,
}

impl InstrTab {
    fn label(self) -> &'static str {
        match self {
            Self::Groups => "GROUPS",
            Self::Instrument => "INSTRUMENT",
            Self::Mapper => "MAPPER",
            Self::Modulation => "MODULATION",
            Self::Fx => "FX",
        }
    }
}

// ── Top-level sampler view ─────────────────────────────────────────────────

/// Full sampler view with library browser, instrument editor, and piano.
#[component]
pub fn SamplerView() -> Element {
    let mut libraries = use_signal(demo_libraries);
    let mut active_patch: Signal<Option<(usize, usize)>> = use_signal(|| Some((0, 0)));
    let mut active_tab = use_signal(|| InstrTab::Instrument);
    let mut active_notes: Signal<Vec<u8>> = use_signal(Vec::new);
    let mut waterfall: Signal<WaterfallState> = use_signal(WaterfallState::default);

    // Full 88-key range (A0 = 21, C8 = 108) — fills entire width
    let start_note: u8 = 21;
    let end_note: u8 = 108;

    // ── MIDI input coroutine ───────────────────────────────────────────
    // Polls the MidiInput context (if provided by the desktop app) at ~60fps
    // and mirrors hardware note-on/off events onto the piano.
    // use_context::<Option<MidiInput>>() — provided by the desktop app.
    // Returns None if no MIDI hardware is available or not running in desktop mode.
    let midi_opt: Option<MidiStream> = use_context::<Option<MidiStream>>();
    // Clone for the coroutine so `midi_opt` stays available for the indicator dot.
    let midi_for_future = midi_opt.clone();
    use_future(move || {
        let midi = midi_for_future.clone();
        async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(16)).await;
                // Tick waterfall animation every frame (~60fps).
                waterfall.write().tick(0.016);
                // Process MIDI events if a device is connected.
                if let Some(ref midi) = midi {
                    for raw in midi.drain() {
                        match raw.to_event() {
                            Some(midicore::MidiEvent::NoteOn { key, .. }) => {
                                let note = key.get();
                                let mut an = active_notes.write();
                                if !an.contains(&note) {
                                    an.push(note);
                                }
                                drop(an);
                                waterfall.write().spawn(note, Some("#e8813a".to_string()));
                            }
                            Some(midicore::MidiEvent::NoteOff { key, .. }) => {
                                let note = key.get();
                                active_notes.write().retain(|&n| n != note);
                                waterfall.write().release(note);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    });

    let wf_notes: Vec<WaterfallNote> = waterfall.read().notes.clone();

    // Derive active library/patch names for the header
    let (active_lib_name, active_patch_name) = active_patch
        .read()
        .and_then(|(lib_id, patch_id)| {
            let libs = libraries.read();
            let lib = libs.iter().find(|l| l.id == lib_id)?;
            let patch = lib.patches.iter().find(|p| p.id == patch_id)?;
            Some((lib.name, patch.name))
        })
        .unwrap_or(("No Library", "No Patch"));

    rsx! {
        div {
            style: format!(
                "display:flex; flex-direction:column; width:100%; height:100%; \
                 background:{bg}; overflow:hidden; font-family:-apple-system,BlinkMacSystemFont,'SF Pro Text',sans-serif;",
                bg = BG_BODY,
            ),

            // ── Main content (browser + instrument editor) ─────────────
            div {
                style: "display:flex; flex:1; overflow:hidden; min-height:0;",

                // ── Left: Library browser ──────────────────────────────
                LibraryBrowser {
                    libraries: libraries.read().clone(),
                    active_patch: *active_patch.read(),
                    on_toggle_library: move |lib_id: usize| {
                        let mut libs = libraries.write();
                        if let Some(lib) = libs.iter_mut().find(|l| l.id == lib_id) {
                            lib.expanded = !lib.expanded;
                        }
                    },
                    on_select_patch: move |(lib_id, patch_id): (usize, usize)| {
                        *active_patch.write() = Some((lib_id, patch_id));
                    },
                }

                // Divider
                div { style: format!("width:1px; background:{BORDER}; flex-shrink:0;") }

                // ── Right: Instrument editor ───────────────────────────
                div {
                    style: "display:flex; flex-direction:column; flex:1; overflow:hidden; min-width:0;",

                    // Instrument header
                    InstrumentHeader {
                        lib_name: active_lib_name,
                        patch_name: active_patch_name,
                    }

                    // Horizontal rule
                    div { style: format!("height:1px; background:{BORDER}; flex-shrink:0;") }

                    // Tab bar
                    InstrTabBar {
                        active: *active_tab.read(),
                        on_select: move |t| { *active_tab.write() = t; },
                    }

                    // Horizontal rule
                    div { style: format!("height:1px; background:{BORDER}; flex-shrink:0;") }

                    // Tab content
                    div {
                        style: "flex:1; overflow:auto; min-height:0;",
                        match *active_tab.read() {
                            InstrTab::Groups     => rsx! { GroupsTab {} },
                            InstrTab::Instrument => rsx! { InstrumentTab {} },
                            InstrTab::Mapper     => rsx! { MapperTab {} },
                            InstrTab::Modulation => rsx! { ModulationTab {} },
                            InstrTab::Fx         => rsx! { FxTab {} },
                        }
                    }
                }
            }

            // Divider above piano
            div { style: format!("height:1px; background:{BORDER}; flex-shrink:0;") }

            // ── Bottom: Piano + controls ───────────────────────────────
            div {
                style: format!(
                    "display:flex; flex-direction:column; flex-shrink:0; background:{bg};",
                    bg = BG_PANEL,
                ),

                // Piano status bar (active notes indicator)
                div {
                    style: format!(
                        "display:flex; align-items:center; gap:8px; padding:2px 14px; \
                         border-bottom:1px solid {BORDER}; height:24px; flex-shrink:0;"
                    ),
                    span {
                        style: format!("font-size:10px; font-weight:700; color:{TEXT_DIM}; letter-spacing:0.08em; text-transform:uppercase;"),
                        "KEYBOARD"
                    }
                    div { style: "flex:1;" }
                    // Active MIDI note display
                    span {
                        style: format!("font-size:11px; color:{ACCENT}; font-family:monospace; letter-spacing:0.05em;"),
                        {
                            let notes = active_notes.read();
                            if notes.is_empty() {
                                "–".to_string()
                            } else {
                                notes.iter().map(|n| midi_note_name(*n)).collect::<Vec<_>>().join("  ")
                            }
                        }
                    }
                    // MIDI indicator dot
                    {
                        let has_midi = midi_opt.is_some(); // Option<MidiStream>
                        rsx! {
                            div {
                                title: if has_midi { "MIDI connected" } else { "No MIDI input" },
                                style: format!(
                                    "width:7px; height:7px; border-radius:50%; background:{col};",
                                    col = if has_midi { ACCENT } else { BG_ELEVATED }
                                ),
                            }
                        }
                    }
                }

                // Full-width piano — 88 keys, no scrolling, scales to container
                Piano {
                    start_note,
                    end_note,
                    active_notes: active_notes.read().clone(),
                    waterfall_notes: wf_notes,
                    show_labels: true,
                    accent_color: ACCENT.to_string(),
                    height: "270px".to_string(),
                    on_note_on: move |note: u8| {
                        let mut an = active_notes.write();
                        if !an.contains(&note) { an.push(note); }
                        drop(an);
                        waterfall.write().spawn(note, Some(ACCENT.to_string()));
                    },
                    on_note_off: move |note: u8| {
                        active_notes.write().retain(|&n| n != note);
                        waterfall.write().release(note);
                    },
                }
            }
        }
    }
}

fn midi_note_name(n: u8) -> String {
    let names = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let oct = (n as i32 / 12) - 1;
    format!("{}{}", names[(n % 12) as usize], oct)
}

// ── Library browser ────────────────────────────────────────────────────────

#[component]
fn LibraryBrowser(
    libraries: Vec<LibraryEntry>,
    active_patch: Option<(usize, usize)>,
    on_toggle_library: EventHandler<usize>,
    on_select_patch: EventHandler<(usize, usize)>,
) -> Element {
    rsx! {
        div {
            style: format!(
                "width:220px; min-width:180px; max-width:280px; \
                 display:flex; flex-direction:column; overflow:hidden; \
                 background:{BG_PANEL}; flex-shrink:0;"
            ),

            // Browser header
            div {
                style: format!(
                    "padding:8px 12px; border-bottom:1px solid {BORDER}; flex-shrink:0; \
                     display:flex; align-items:center; gap:8px;"
                ),
                span {
                    style: format!("font-size:11px; font-weight:700; color:{TEXT_DIM}; letter-spacing:0.08em; text-transform:uppercase;"),
                    "Libraries"
                }
                div { style: "flex:1;" }
                // Add library button (placeholder)
                button {
                    style: format!(
                        "width:18px; height:18px; border-radius:3px; border:1px solid {BORDER_LIGHT}; \
                         background:transparent; color:{TEXT_DIM}; font-size:12px; cursor:pointer; \
                         display:flex; align-items:center; justify-content:center; padding:0;"
                    ),
                    title: "Add Library",
                    "+"
                }
            }

            // Library list
            div {
                style: "flex:1; overflow-y:auto; overflow-x:hidden;",
                for lib in libraries {
                    {
                        let lib_id = lib.id;
                        let tog = on_toggle_library;
                        let sel = on_select_patch;
                        rsx! {
                            LibraryRow {
                                key: "{lib_id}",
                                lib,
                                active_patch,
                                on_toggle: move |_| tog.call(lib_id),
                                on_select: move |patch_id| sel.call((lib_id, patch_id)),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn LibraryRow(
    lib: LibraryEntry,
    active_patch: Option<(usize, usize)>,
    on_toggle: EventHandler<()>,
    on_select: EventHandler<usize>,
) -> Element {
    let arrow = if lib.expanded { "▾" } else { "▸" };
    rsx! {
        div {
            // Library header row
            div {
                onclick: move |_| on_toggle.call(()),
                style: format!(
                    "display:flex; align-items:center; gap:6px; padding:5px 10px; \
                     cursor:pointer; user-select:none; \
                     border-bottom:1px solid {BORDER};"
                ),

                span { style: format!("font-size:10px; color:{TEXT_DIM}; width:10px;"), "{arrow}" }
                div {
                    style: "display:flex; flex-direction:column; min-width:0; flex:1;",
                    span {
                        style: format!("font-size:12px; font-weight:600; color:{TEXT}; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;"),
                        "{lib.name}"
                    }
                    span {
                        style: format!("font-size:10px; color:{TEXT_DIM};"),
                        "{lib.vendor}"
                    }
                }
                span {
                    style: format!("font-size:10px; color:{ACCENT_DIM}; flex-shrink:0;"),
                    "{lib.patch_count}"
                }
            }

            // Patches (when expanded)
            if lib.expanded {
                div {
                    style: format!("background:{BG_BODY};"),
                    for patch in lib.patches.iter() {
                        {
                            let is_active = active_patch == Some((lib.id, patch.id));
                            let patch_id = patch.id;
                            rsx! {
                                div {
                                    key: "{patch.id}",
                                    onclick: move |_| on_select.call(patch_id),
                                    style: format!(
                                        "padding:4px 10px 4px 26px; cursor:pointer; \
                                         font-size:11px; color:{col}; \
                                         background:{bg}; \
                                         border-bottom:1px solid {BORDER}; \
                                         white-space:nowrap; overflow:hidden; text-overflow:ellipsis;",
                                        col = if is_active { TEXT_BRIGHT } else { TEXT_DIM },
                                        bg = if is_active { BG_ACTIVE } else { "transparent" },
                                    ),
                                    if is_active {
                                        span { style: format!("color:{ACCENT}; margin-right:4px;"), "▶" }
                                    }
                                    "{patch.name}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Instrument header ──────────────────────────────────────────────────────

#[component]
fn InstrumentHeader(lib_name: &'static str, patch_name: &'static str) -> Element {
    rsx! {
        div {
            style: format!(
                "display:flex; align-items:center; gap:12px; padding:8px 16px; \
                 background:{BG_PANEL}; flex-shrink:0; min-height:52px;"
            ),

            // Instrument icon (placeholder square)
            div {
                style: format!(
                    "width:36px; height:36px; border-radius:6px; flex-shrink:0; \
                     background:linear-gradient(135deg,{ACCENT_DIM},{ACCENT}); \
                     display:flex; align-items:center; justify-content:center;"
                ),
                span { style: "font-size:18px;", "🎹" }
            }

            // Names
            div {
                style: "display:flex; flex-direction:column; min-width:0; flex:1;",
                span {
                    style: format!("font-size:14px; font-weight:700; color:{TEXT_BRIGHT}; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;"),
                    "{patch_name}"
                }
                span {
                    style: format!("font-size:11px; color:{TEXT_DIM};"),
                    "{lib_name}"
                }
            }

            // Toolbar (placeholder buttons)
            div {
                style: "display:flex; align-items:center; gap:6px; flex-shrink:0;",
                for (icon, tip) in [("⚙", "Options"), ("💾", "Save"), ("↩", "Reset")] {
                    button {
                        key: "{icon}",
                        title: "{tip}",
                        style: format!(
                            "width:28px; height:28px; border-radius:5px; \
                             border:1px solid {BORDER_LIGHT}; background:{BG_ELEVATED}; \
                             color:{TEXT_DIM}; font-size:12px; cursor:pointer;"
                        ),
                        "{icon}"
                    }
                }
            }
        }
    }
}

// ── Instrument tab bar ─────────────────────────────────────────────────────

#[component]
fn InstrTabBar(active: InstrTab, on_select: EventHandler<InstrTab>) -> Element {
    const TABS: &[InstrTab] = &[
        InstrTab::Groups,
        InstrTab::Instrument,
        InstrTab::Mapper,
        InstrTab::Modulation,
        InstrTab::Fx,
    ];
    rsx! {
        div {
            style: format!(
                "display:flex; flex-direction:row; background:{BG_PANEL}; \
                 flex-shrink:0; padding:0 4px; gap:2px; padding-top:4px;"
            ),
            for &tab in TABS {
                {
                    let is_active = active == tab;
                    rsx! {
                        button {
                            key: "{tab.label()}",
                            onclick: move |_| on_select.call(tab),
                            style: format!(
                                "padding:5px 12px; border:none; border-radius:5px 5px 0 0; \
                                 font-size:10px; font-weight:700; letter-spacing:0.06em; \
                                 cursor:pointer; white-space:nowrap; \
                                 background:{bg}; color:{col}; \
                                 border-bottom:2px solid {accent};",
                                bg = if is_active { BG_SURFACE } else { "transparent" },
                                col = if is_active { TEXT_BRIGHT } else { TEXT_DIM },
                                accent = if is_active { ACCENT } else { "transparent" },
                            ),
                            "{tab.label()}"
                        }
                    }
                }
            }
        }
    }
}

// ── Tab content panels ─────────────────────────────────────────────────────

#[component]
fn GroupsTab() -> Element {
    rsx! {
        div {
            style: format!("padding:16px; color:{TEXT_DIM}; font-size:12px;"),

            // Group list header
            div {
                style: format!(
                    "display:flex; align-items:center; gap:8px; margin-bottom:12px; \
                     padding-bottom:8px; border-bottom:1px solid {BORDER};"
                ),
                span { style: format!("font-size:11px; font-weight:700; color:{TEXT}; letter-spacing:0.08em;"), "GROUPS" }
                div { style: "flex:1;" }
                button {
                    style: format!("padding:2px 10px; border-radius:4px; border:1px solid {BORDER_LIGHT}; background:{BG_ELEVATED}; color:{TEXT_DIM}; font-size:11px; cursor:pointer;"),
                    "+ Add Group"
                }
            }

            // Group rows
            for (i, name) in ["All Samples", "Sustain pp", "Sustain mf", "Sustain ff", "Release"].iter().enumerate() {
                div {
                    key: "{i}",
                    style: format!(
                        "display:flex; align-items:center; gap:10px; padding:6px 8px; \
                         border-radius:5px; margin-bottom:2px; \
                         background:{bg};",
                        bg = if i == 0 { BG_ACTIVE } else { BG_SURFACE },
                    ),
                    // Group number
                    span {
                        style: format!("font-size:10px; color:{ACCENT}; width:16px; text-align:right; flex-shrink:0;"),
                        "{i}"
                    }
                    // Name
                    span { style: format!("font-size:12px; color:{TEXT}; flex:1;"), "{name}" }
                    // Sample count
                    span { style: format!("font-size:10px; color:{TEXT_DIM};"), "24 smpl" }
                    // Mute / Solo
                    button {
                        style: format!("width:20px; height:20px; border-radius:3px; border:1px solid {BORDER}; background:{BG_ELEVATED}; color:{TEXT_DIM}; font-size:9px; cursor:pointer;"),
                        "M"
                    }
                    button {
                        style: format!("width:20px; height:20px; border-radius:3px; border:1px solid {BORDER}; background:{BG_ELEVATED}; color:{TEXT_DIM}; font-size:9px; cursor:pointer;"),
                        "S"
                    }
                }
            }
        }
    }
}

#[component]
fn InstrumentTab() -> Element {
    rsx! {
        div {
            style: format!("padding:16px; display:flex; flex-direction:column; gap:16px;"),

            // Playback section
            SectionBlock { title: "PLAYBACK",
                div {
                    style: "display:grid; grid-template-columns:1fr 1fr 1fr; gap:10px;",
                    KnobRow { label: "Volume",  value: "100" }
                    KnobRow { label: "Pan",     value: "0" }
                    KnobRow { label: "Tune",    value: "0" }
                }
            }

            // Amp Envelope
            SectionBlock { title: "AMP ENVELOPE",
                div {
                    style: "display:grid; grid-template-columns:1fr 1fr 1fr 1fr; gap:10px;",
                    KnobRow { label: "Attack",  value: "2ms" }
                    KnobRow { label: "Decay",   value: "300ms" }
                    KnobRow { label: "Sustain", value: "80%" }
                    KnobRow { label: "Release", value: "500ms" }
                }
            }

            // LFO
            SectionBlock { title: "LFO",
                div {
                    style: "display:grid; grid-template-columns:1fr 1fr 1fr; gap:10px;",
                    KnobRow { label: "Rate",   value: "2Hz" }
                    KnobRow { label: "Depth",  value: "0%" }
                    KnobRow { label: "Fade-in", value: "0ms" }
                }
            }
        }
    }
}

#[component]
fn MapperTab() -> Element {
    rsx! {
        div {
            style: format!("padding:16px; color:{TEXT_DIM}; font-size:12px;"),

            // Zone map header
            div {
                style: format!("margin-bottom:12px; padding-bottom:8px; border-bottom:1px solid {BORDER};"),
                span { style: format!("font-size:11px; font-weight:700; color:{TEXT}; letter-spacing:0.08em;"), "ZONE MAP" }
            }

            // Visual zone map (simplified)
            div {
                style: format!(
                    "border:1px solid {BORDER}; border-radius:6px; background:{BG_SURFACE}; \
                     padding:12px; margin-bottom:12px;"
                ),
                // Velocity axis label
                div {
                    style: "display:flex; justify-content:space-between; margin-bottom:6px;",
                    span { style: format!("font-size:10px; color:{TEXT_DIM};"), "vel 1" }
                    span { style: format!("font-size:10px; color:{TEXT_DIM};"), "vel 127" }
                }
                // Zones
                for (note_range, vel_pct, label) in [
                    ("C1–B5", "0–40%",  "pp"),
                    ("C1–B5", "40–70%", "mf"),
                    ("C1–B5", "70–100%","ff"),
                ] {
                    div {
                        key: "{label}",
                        style: format!(
                            "display:flex; align-items:center; gap:8px; margin-bottom:4px;"
                        ),
                        span { style: format!("font-size:10px; color:{TEXT_DIM}; width:60px; flex-shrink:0;"), "{note_range}" }
                        div {
                            style: format!(
                                "flex:1; height:16px; border-radius:3px; background:{ACCENT_DIM}; \
                                 display:flex; align-items:center; padding:0 6px;"
                            ),
                            span { style: format!("font-size:9px; color:{TEXT};"), "{label}" }
                        }
                        span { style: format!("font-size:10px; color:{TEXT_DIM}; width:48px; flex-shrink:0;"), "{vel_pct}" }
                    }
                }
            }

            // Zone table
            div {
                style: format!(
                    "border:1px solid {BORDER}; border-radius:6px; overflow:hidden;"
                ),
                // Header
                div {
                    style: format!(
                        "display:grid; grid-template-columns:2fr 1fr 1fr 1fr; \
                         background:{BG_ELEVATED}; padding:5px 10px; \
                         border-bottom:1px solid {BORDER};"
                    ),
                    for col in ["Sample", "Root", "Lo–Hi Key", "Lo–Hi Vel"] {
                        span { key: "{col}", style: format!("font-size:10px; font-weight:700; color:{TEXT_DIM};"), "{col}" }
                    }
                }
                // Rows
                for (i, (sample, root, key, vel)) in [
                    ("Strings_pp_C3.wav", "C3", "C1–B5", "1–52"),
                    ("Strings_mf_C3.wav", "C3", "C1–B5", "53–89"),
                    ("Strings_ff_C3.wav", "C3", "C1–B5", "90–127"),
                ].iter().enumerate() {
                    div {
                        key: "{i}",
                        style: format!(
                            "display:grid; grid-template-columns:2fr 1fr 1fr 1fr; \
                             padding:5px 10px; background:{bg}; border-bottom:1px solid {BORDER};",
                            bg = if i % 2 == 0 { BG_SURFACE } else { BG_PANEL },
                        ),
                        span { style: format!("font-size:11px; color:{TEXT}; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;"), "{sample}" }
                        span { style: format!("font-size:11px; color:{ACCENT};"), "{root}" }
                        span { style: format!("font-size:11px; color:{TEXT_DIM};"), "{key}" }
                        span { style: format!("font-size:11px; color:{TEXT_DIM};"), "{vel}" }
                    }
                }
            }
        }
    }
}

#[component]
fn ModulationTab() -> Element {
    rsx! {
        div {
            style: format!("padding:16px; color:{TEXT_DIM}; font-size:12px;"),

            div {
                style: format!("margin-bottom:12px; padding-bottom:8px; border-bottom:1px solid {BORDER};"),
                span { style: format!("font-size:11px; font-weight:700; color:{TEXT}; letter-spacing:0.08em;"), "MODULATION MATRIX" }
            }

            // Mod rows
            div {
                style: format!(
                    "border:1px solid {BORDER}; border-radius:6px; overflow:hidden;"
                ),
                // Header
                div {
                    style: format!(
                        "display:grid; grid-template-columns:1fr 1fr 80px; \
                         background:{BG_ELEVATED}; padding:5px 10px; border-bottom:1px solid {BORDER};"
                    ),
                    for col in ["Source", "Destination", "Amount"] {
                        span { key: "{col}", style: format!("font-size:10px; font-weight:700; color:{TEXT_DIM};"), "{col}" }
                    }
                }
                for (i, (src, dst, amt)) in [
                    ("CC1",       "Volume",      "100%"),
                    ("CC2",       "Vibrato Depth", "80%"),
                    ("Velocity",  "Sample Start",  "30%"),
                    ("Pitch Bend","Pitch",          "±2st"),
                ].iter().enumerate() {
                    div {
                        key: "{i}",
                        style: format!(
                            "display:grid; grid-template-columns:1fr 1fr 80px; \
                             padding:6px 10px; background:{bg}; border-bottom:1px solid {BORDER};",
                            bg = if i % 2 == 0 { BG_SURFACE } else { BG_PANEL },
                        ),
                        span { style: format!("font-size:11px; color:{ACCENT};"), "{src}" }
                        span { style: format!("font-size:11px; color:{TEXT};"), "{dst}" }
                        span { style: format!("font-size:11px; color:{TEXT_DIM}; font-variant-numeric:tabular-nums;"), "{amt}" }
                    }
                }
                // Add row
                div {
                    style: format!(
                        "padding:6px 10px; background:{BG_PANEL}; \
                         display:flex; align-items:center; gap:8px; cursor:pointer;"
                    ),
                    span { style: format!("font-size:11px; color:{TEXT_DIM};"), "+ Add Modulation" }
                }
            }
        }
    }
}

#[component]
fn FxTab() -> Element {
    rsx! {
        div {
            style: format!("padding:16px; display:flex; flex-direction:column; gap:12px;"),

            div {
                style: format!("padding-bottom:8px; border-bottom:1px solid {BORDER};"),
                span { style: format!("font-size:11px; font-weight:700; color:{TEXT}; letter-spacing:0.08em;"), "INSERT FX" }
            }

            // FX chain
            for (i, (name, active)) in [
                ("EQ", true), ("Compressor", true), ("Reverb", false), ("Delay", false)
            ].iter().enumerate() {
                div {
                    key: "{i}",
                    style: format!(
                        "display:flex; align-items:center; gap:10px; padding:8px 12px; \
                         border-radius:6px; background:{bg}; border:1px solid {border};",
                        bg = if *active { BG_SURFACE } else { BG_PANEL },
                        border = if *active { BORDER_LIGHT } else { BORDER },
                    ),
                    // Power / bypass
                    div {
                        style: format!(
                            "width:10px; height:10px; border-radius:50%; flex-shrink:0; \
                             background:{col};",
                            col = if *active { ACCENT } else { BG_ELEVATED },
                        ),
                    }
                    span {
                        style: format!(
                            "font-size:12px; font-weight:600; flex:1; color:{col};",
                            col = if *active { TEXT } else { TEXT_DIM },
                        ),
                        "{name}"
                    }
                    span { style: format!("font-size:10px; color:{TEXT_DIM};"), "Edit →" }
                }
            }

            // Add FX
            button {
                style: format!(
                    "padding:6px 14px; border-radius:5px; border:1px dashed {BORDER_LIGHT}; \
                     background:transparent; color:{TEXT_DIM}; font-size:11px; cursor:pointer; \
                     width:100%;"
                ),
                "+ Add Effect"
            }
        }
    }
}

// ── Shared sub-components ──────────────────────────────────────────────────

#[component]
fn SectionBlock(title: &'static str, children: Element) -> Element {
    rsx! {
        div {
            style: format!("background:{BG_SURFACE}; border-radius:6px; border:1px solid {BORDER}; overflow:hidden;"),
            div {
                style: format!(
                    "padding:5px 12px; background:{BG_ELEVATED}; border-bottom:1px solid {BORDER};"
                ),
                span {
                    style: format!("font-size:10px; font-weight:700; color:{TEXT_DIM}; letter-spacing:0.08em;"),
                    "{title}"
                }
            }
            div { style: "padding:12px;", {children} }
        }
    }
}

#[component]
fn KnobRow(label: &'static str, value: &'static str) -> Element {
    rsx! {
        div {
            style: "display:flex; flex-direction:column; align-items:center; gap:4px;",
            // Knob circle (visual placeholder)
            div {
                style: format!(
                    "width:36px; height:36px; border-radius:50%; \
                     border:2px solid {ACCENT}; background:{BG_ELEVATED}; \
                     display:flex; align-items:center; justify-content:center; \
                     position:relative; cursor:pointer;"
                ),
                // Indicator dot
                div {
                    style: format!(
                        "width:4px; height:4px; border-radius:50%; background:{ACCENT}; \
                         position:absolute; top:4px; left:50%; transform:translateX(-50%);"
                    ),
                }
                span { style: format!("font-size:9px; color:{TEXT_DIM};"), "{value}" }
            }
            span {
                style: format!("font-size:10px; color:{TEXT_DIM}; text-align:center;"),
                "{label}"
            }
        }
    }
}
