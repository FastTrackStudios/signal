//! Glyphs for modules and block types — the grid renders these instead of
//! anonymous colored dots, so a cell reads as *what it is* before its label.

use signal_proto::block::BlockType;

/// A compact glyph for a module (by template module name).
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
pub fn block_icon(bt: BlockType) -> &'static str {
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
