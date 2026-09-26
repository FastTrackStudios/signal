//! Glyphs for modules and block types — the grid renders these instead of
//! anonymous colored dots, so a cell reads as *what it is* before its label.

use dioxus::prelude::*;
use signal_proto::block::BlockType;

/// Stroke paths (24×24 grid, round 1.8px stroke) for a module's glyph.
///
/// Drawn rather than typed: the symbol and emoji characters `module_icon`
/// returns are missing from the font Blitz renders with, so on the rig
/// surfaces they drew as empty boxes.
fn module_paths(module: &str) -> &'static [&'static str] {
    match module.to_ascii_lowercase().as_str() {
        // An input jack.
        "source" => &["M12 3v6", "M8 9h8v4a4 4 0 0 1-8 0z", "M12 17v4"],
        // A compressor's knee.
        "dynamics" => &["M4 5v14h16", "M4 19l6-6 10-4"],
        // A sparkle.
        "special" => &[
            "M12 3v4", "M12 17v4", "M3 12h4", "M17 12h4",
            "M6 6l2.5 2.5", "M15.5 15.5 18 18", "M18 6l-2.5 2.5", "M8.5 15.5 6 18",
        ],
        // A bolt.
        "drive" => &["M13 2 4 14h7l-1 8 9-12h-7z"],
        // A speaker.
        "volume" => &["M11 5 6 9H3v6h3l5 4z", "M15.5 8.5a5 5 0 0 1 0 7"],
        // EQ bars.
        "pre-fx" | "prefx" => &["M6 20V10", "M12 20V4", "M18 20v-6"],
        // An amp head.
        "amp" => &["M3 7h18v12H3z", "M3 11h18", "M7 15h.01", "M11 15h.01"],
        // A wrench.
        "utility" => &["M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.5 2.5-2.5-.5-.5-2.5z"],
        // A wave.
        "modulation" => &["M2 12c3-7 6-7 9 0s6 7 9 0"],
        // A clock.
        "time" => &["M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z", "M12 7v5l3 2"],
        // A turn.
        "motion" => &["M21 12a9 9 0 1 1-3-6.7", "M21 4v5h-5"],
        // A diamond with a ceiling.
        "mastering" => &["M12 3l9 9-9 9-9-9z", "M8 12h8"],
        _ => &["M12 10a2 2 0 1 0 0 4 2 2 0 0 0 0-4z"],
    }
}

/// A module's glyph as inline SVG, in the text colour of wherever it sits.
/// Use this on any rendered surface; [`module_icon`] is the text form, kept
/// for logs and plain-text contexts.
#[component]
pub fn ModuleGlyph(module: String, #[props(default = 12)] size: u32) -> Element {
    rsx! {
        svg {
            width: "{size}", height: "{size}", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.8",
            stroke_linecap: "round", stroke_linejoin: "round",
            style: "display: block; flex-shrink: 0; width: {size}px; height: {size}px;",
            for (i, d) in module_paths(&module).iter().enumerate() {
                path { key: "{i}", d: "{d}" }
            }
        }
    }
}

/// A compact glyph for a module (by template module name).
#[must_use]
pub fn module_icon(module: &str) -> &'static str {
    match module.to_ascii_lowercase().as_str() {
        "source" => "⌁",
        "dynamics" => "◆",
        "special" => "✦",
        "drive" => "↯",
        "volume" => "◔",
        "pre-fx" | "prefx" => "≋",
        "amp" => "⏦",
        "utility" => "⚙",
        "modulation" => "〜",
        "time" => "◷",
        "motion" => "↻",
        "mastering" => "◈",
        _ => "•",
    }
}

/// A compact glyph for a block type.
#[must_use]
pub const fn block_icon(bt: BlockType) -> &'static str {
    match bt {
        BlockType::Gate => "⎍",
        BlockType::Volume => "◔",
        BlockType::Compressor => "◆",
        BlockType::Eq => "≋",
        BlockType::Filter => "⌒",
        BlockType::Wah => "∪",
        BlockType::Pitch => "♯",
        BlockType::Doubler => "⧉",
        BlockType::Boost => "↥",
        BlockType::Drive | BlockType::Saturator => "↯",
        BlockType::Amp => "⏦",
        BlockType::Cabinet => "▭",
        BlockType::Chorus => "〜",
        BlockType::Flanger => "≈",
        BlockType::Phaser => "◠",
        BlockType::Trem => "∿",
        BlockType::Vibrato => "≀",
        BlockType::Rotary => "↻",
        BlockType::Delay => "◷",
        BlockType::Reverb => "◈",
        BlockType::Limiter => "⌸",
        _ => "●",
    }
}
