//! The chrome's icon set — hand-rolled inline SVG.
//!
//! Deliberately not `lucide-dioxus`: the rig UIs render through Blitz, which
//! wants inline markup and no asset loading, and the chrome needs ~20 glyphs,
//! not an icon library. Every glyph is drawn on a 24×24 grid with a 2px round
//! stroke so they sit on one optical weight.

use dioxus::prelude::*;

/// A glyph. Views name one when they register a rail destination or a panel;
/// the chrome draws it. An enum (not an `Element`) so specs stay `PartialEq`
/// and can live in a signal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    // Workspaces.
    Home,
    Signal,
    Session,
    Arrangement,
    Mixer,
    Lyrics,
    Charts,
    // Rigs.
    Guitar,
    Bass,
    Drums,
    Keys,
    Synth,
    // Panels.
    Browser,
    Midi,
    Routing,
    Logs,
    Settings,
    Engine,
    Perform,
    // Play modes: what the footswitches choose between.
    /// One sound — a tone preset (sliders).
    Preset,
    /// A profile's stacks of patches (layers).
    Profile,
    /// The night's songs, in order (list with a note).
    Setlist,
    // Work views.
    /// Play and shape the rig (a knob).
    Control,
    /// The capture catalog (download from a cloud).
    Tones,
    // Chrome affordances.
    /// Bypass / engage (the power symbol).
    Power,
    /// Zoom a panel to full size.
    Expand,
    /// A marked default.
    Star,
    /// Confirm.
    Check,
    /// Edit / rename.
    Pencil,
    /// A note (tuner idle).
    Note,
    /// Opens a menu below.
    ChevronDown,
    /// The command palette (⌘).
    Command,
    /// Reload / refresh.
    Refresh,
    RailLeft,
    RailRight,
    Close,
    Minimize,
    Maximize,
}

impl Icon {
    /// The glyph's paths, drawn stroked on a 24×24 view box.
    fn paths(self) -> &'static [&'static str] {
        match self {
            Icon::Home => &["M3 10.5 12 3l9 7.5", "M5 9.5V21h14V9.5"],
            // A signal chain: source → node → out.
            Icon::Signal => &["M3 12h4", "M17 12h4", "M9 12h6", "M12 8v8"],
            Icon::Session => &["M4 5h16v14H4z", "M4 10h16", "M9 10v9"],
            Icon::Arrangement => &["M3 6h18", "M3 12h12", "M3 18h7"],
            Icon::Mixer => &[
                "M6 3v18", "M12 3v18", "M18 3v18", "M3 8h6", "M9 15h6", "M15 6h6",
            ],
            Icon::Lyrics => &["M5 4h14v16H5z", "M8 9h8", "M8 13h8", "M8 17h4"],
            Icon::Charts => &["M4 4v16h16", "M8 15l3-4 3 3 4-6"],
            Icon::Guitar => &["M14 4l6 6", "M11 7l6 6", "M9 9a5 5 0 1 0 6 6z"],
            Icon::Bass => &["M15 3l6 6", "M12 6l6 6", "M10 8a6 6 0 1 0 6 6z"],
            Icon::Drums => &[
                "M3 9h18",
                "M3 9v7c0 2 4 3 9 3s9-1 9-3V9",
                "M12 6a9 3 0 1 0 0 6a9 3 0 1 0 0-6",
            ],
            Icon::Keys => &["M3 5h18v14H3z", "M8 5v9", "M13 5v9", "M18 5v9"],
            Icon::Synth => &["M3 15c3 0 3-8 6-8s3 8 6 8 3-8 6-8", "M3 20h18"],
            Icon::Browser => &["M4 5h16v14H4z", "M4 9h16", "M8 13h8", "M8 16h5"],
            Icon::Midi => &[
                "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z",
                "M8 10v.01",
                "M16 10v.01",
                "M12 8v.01",
                "M9 15h6",
            ],
            Icon::Routing => &["M6 4v5a3 3 0 0 0 3 3h9", "M6 20v-5", "M18 9l3 3-3 3"],
            Icon::Logs => &["M5 4h14v16H5z", "M9 8h6", "M9 12h6", "M9 16h3"],
            // A gear — Lucide's `settings` outline (ISC licence). The old
            // spoked circle read as a sun.
            Icon::Settings => &[
                "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z",
                "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
            ],
            Icon::Engine => &["M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z", "M12 7v5l3 3"],
            Icon::Perform => &["M4 6h4v12H4z", "M11 6h2v12h-2z", "M16 6h4v12h-4z"],
            Icon::Preset => &[
                "M21 4h-7", "M10 4H3", "M21 12h-9", "M8 12H3", "M21 20h-5", "M12 20H3",
                "M14 2v4", "M8 10v4", "M16 18v4",
            ],
            Icon::Profile => &["M12 3 3 7.5 12 12l9-4.5z", "M3 12l9 4.5 9-4.5", "M3 16.5 12 21l9-4.5"],
            Icon::Setlist => &["M3 6h13", "M3 12h9", "M3 18h9", "M21 6v10", "M18.5 18.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5z"],
            Icon::Control => &["M12 4a8 8 0 1 0 0 16 8 8 0 0 0 0-16z", "M12 12l4-4"],
            Icon::Tones => &["M20 16.6A5 5 0 0 0 18 7h-1.3A8 8 0 1 0 4 15.3", "M12 12v9", "M8 17l4 4 4-4"],
            Icon::Power => &["M12 2v10", "M18.4 6.6a9 9 0 1 1-12.77.04"],
            Icon::Expand => &["M15 3h6v6", "M9 21H3v-6", "M21 3l-7 7", "M3 21l7-7"],
            Icon::Star => &["M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01z"],
            Icon::Check => &["M20 6 9 17l-5-5"],
            Icon::Pencil => &["M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5z"],
            Icon::Note => &["M9 18V5l12-2v13", "M6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6z", "M18 19a3 3 0 1 0 0-6 3 3 0 0 0 0 6z"],
            Icon::ChevronDown => &["M6 9l6 6 6-6"],
            Icon::Command => &["M15 6v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3V6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3"],
            Icon::Refresh => &["M21 12a9 9 0 1 1-3-6.7", "M21 4v5h-5"],
            Icon::RailLeft => &["M4 5h16v14H4z", "M10 5v14"],
            Icon::RailRight => &["M4 5h16v14H4z", "M14 5v14"],
            Icon::Close => &["M6 6l12 12", "M18 6L6 18"],
            Icon::Minimize => &["M5 12h14"],
            Icon::Maximize => &["M5 5h14v14H5z"],
        }
    }
}

/// Render a glyph at `size` px in `currentColor`.
#[component]
pub fn Glyph(icon: Icon, #[props(default = 16)] size: u32) -> Element {
    rsx! {
        svg {
            width: "{size}", height: "{size}", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.8",
            stroke_linecap: "round", stroke_linejoin: "round",
            // Size in the style too: a stylesheet rule like Tailwind's
            // preflight (`svg { height: auto }`) beats the attributes and
            // collapses the glyph to nothing.
            style: "display: block; flex-shrink: 0; width: {size}px; height: {size}px;",
            for (i, d) in icon.paths().iter().enumerate() {
                path { key: "{i}", d: "{d}" }
            }
        }
    }
}
