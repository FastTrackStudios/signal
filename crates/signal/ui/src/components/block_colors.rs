//! Block color palette for rig grid UIs.
//!
//! Colors are inspired by the Quad Cortex and Helix modelers, providing
//! visual differentiation between different types of DSP blocks.
//!
//! This module is domain-agnostic: colors are looked up by string key
//! (e.g. `"amp"`, `"drive"`) rather than a specific enum type. Wrapper
//! code can map domain enums to keys with a simple `.to_string()` or match.

/// Color configuration for a block type.
#[derive(Debug, Clone, Copy)]
pub struct BlockColor {
    /// Background color (hex, e.g. `"#3B82F6"`).
    pub bg: &'static str,
    /// Text/foreground color (hex).
    pub fg: &'static str,
    /// Border/accent color (slightly darker than bg).
    pub border: &'static str,
}

impl BlockColor {
    pub const fn new(bg: &'static str, fg: &'static str, border: &'static str) -> Self {
        Self { bg, fg, border }
    }
}

/// Default fallback color for unknown block types.
const FALLBACK: BlockColor = BlockColor::new("#A8A29E", "#FAFAF9", "#78716C");

/// Get the color configuration for a block type by string key.
///
/// Keys are matched case-insensitively. Returns a Quad Cortex-inspired
/// color palette where:
/// - Dynamics (compressor, gate, limiter) = Blue
/// - Drive/Saturation = Orange/Red
/// - Amp = Gold
/// - Cabinet = Brown
/// - EQ = Green
/// - Modulation = Purple
/// - Time (delay, reverb) = Cyan/Sky
/// - Special/Utility = Gray/Pink
///
/// Unknown keys return a neutral gray.
pub fn block_color(key: &str) -> BlockColor {
    match key.to_ascii_lowercase().as_str() {
        // Input/Output — Neutral gray
        "input" | "send" | "volume" => BlockColor::new("#6B7280", "#F9FAFB", "#4B5563"),

        // Dynamics — Blue family
        "compressor" => BlockColor::new("#3B82F6", "#EFF6FF", "#2563EB"),
        "gate" => BlockColor::new("#3B82F6", "#EFF6FF", "#2563EB"),
        "limiter" => BlockColor::new("#3B82F6", "#EFF6FF", "#2563EB"),
        "deesser" | "de_esser" | "de-esser" => BlockColor::new("#60A5FA", "#EFF6FF", "#3B82F6"),

        // Drive/Saturation — Orange/Red family
        "drive" => BlockColor::new("#F97316", "#FFF7ED", "#EA580C"),
        "saturator" => BlockColor::new("#EF4444", "#FEF2F2", "#DC2626"),
        "boost" => BlockColor::new("#FB923C", "#FFF7ED", "#F97316"),

        // Amp — Gold
        "amp" => BlockColor::new("#EAB308", "#FEFCE8", "#CA8A04"),

        // Cabinet — Warm Brown
        "cabinet" | "cab" => BlockColor::new("#B45309", "#FEF3C7", "#92400E"),

        // EQ — Green
        "eq" | "crossover" => BlockColor::new("#22C55E", "#F0FDF4", "#16A34A"),

        // Modulation — Purple family
        "modulation" | "chorus" | "flanger" | "phaser" => {
            BlockColor::new("#A855F7", "#FAF5FF", "#9333EA")
        }
        "ringmodulator" | "ring_modulator" | "ring-modulator" => {
            BlockColor::new("#9333EA", "#FAF5FF", "#7E22CE")
        }

        // Motion — Lighter purple
        "tremolo" | "panner" | "vibrato" | "rotary" => {
            BlockColor::new("#C084FC", "#FAF5FF", "#A855F7")
        }

        // Pitch — Violet
        "pitch" => BlockColor::new("#8B5CF6", "#FAF5FF", "#7C3AED"),

        // Time-based — Cyan/Sky family
        "delay" => BlockColor::new("#06B6D4", "#ECFEFF", "#0891B2"),
        "reverb" => BlockColor::new("#0EA5E9", "#F0F9FF", "#0284C7"),
        "freeze" => BlockColor::new("#22D3EE", "#ECFEFF", "#06B6D4"),

        // Special/Utility — Pink/Gray
        "special" | "wah" | "filter" | "doubler" => {
            BlockColor::new("#EC4899", "#FDF2F8", "#DB2777")
        }
        "tuner" => BlockColor::new("#78716C", "#FAFAF9", "#57534E"),
        "custom" => BlockColor::new("#A8A29E", "#FAFAF9", "#78716C"),

        _ => FALLBACK,
    }
}

/// Get a CSS inline style string for a block color key.
///
/// Returns a `style` attribute value with background, color, and border-color.
pub fn block_style(key: &str) -> String {
    let color = block_color(key);
    format!(
        "background-color: {}; color: {}; border-color: {};",
        color.bg, color.fg, color.border
    )
}

/// Get a faded/bypassed CSS inline style string for a block color key.
pub fn block_bypassed_style(key: &str) -> String {
    let color = block_color(key);
    format!(
        "background-color: {}40; color: {}80; border-color: {}40; opacity: 0.6;",
        color.bg, color.fg, color.border
    )
}

/// Get a slightly varied color for a specific block instance.
///
/// Takes the base color for a key and applies a subtle variation based on
/// the instance identifier (e.g., "Drive 1", "Drive 2"). This helps
/// distinguish between multiple blocks of the same type.
pub fn block_instance_color(key: &str, instance_id: &str) -> BlockColor {
    let base = block_color(key);

    // Generate a hash from the instance_id to get a consistent variation
    let hash: u32 = instance_id.bytes().map(|b| b as u32).sum();
    let variation = (hash % 15) as i32 - 7; // -7 to +7 variation

    // Parse the hex color and apply variation
    let varied_bg = vary_hex_color(base.bg, variation);
    let varied_border = vary_hex_color(base.border, variation);

    BlockColor {
        bg: Box::leak(varied_bg.into_boxed_str()),
        fg: base.fg,
        border: Box::leak(varied_border.into_boxed_str()),
    }
}

/// Vary a hex color by adjusting its lightness.
fn vary_hex_color(hex: &str, variation: i32) -> String {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return format!("#{hex}");
    }

    let r = i32::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = i32::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = i32::from_str_radix(&hex[4..6], 16).unwrap_or(0);

    let adjust = |val: i32| -> u8 { (val + variation * 3).clamp(0, 255) as u8 };

    format!("#{:02X}{:02X}{:02X}", adjust(r), adjust(g), adjust(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_keys_return_colors() {
        let keys = [
            "input",
            "compressor",
            "drive",
            "amp",
            "cabinet",
            "eq",
            "modulation",
            "delay",
            "reverb",
            "gate",
            "volume",
            "pitch",
            "tremolo",
            "limiter",
            "send",
            "special",
            "freeze",
            "custom",
            "deesser",
            "saturator",
            "tuner",
            "chorus",
            "flanger",
            "phaser",
            "ringmodulator",
            "wah",
            "filter",
            "doubler",
            "panner",
            "vibrato",
            "rotary",
            "crossover",
            "boost",
        ];

        for key in keys {
            let color = block_color(key);
            assert!(!color.bg.is_empty(), "missing bg for {key}");
            assert!(!color.fg.is_empty(), "missing fg for {key}");
            assert!(!color.border.is_empty(), "missing border for {key}");
        }
    }

    #[test]
    fn unknown_key_returns_fallback() {
        let color = block_color("unknown_block_type");
        assert_eq!(color.bg, FALLBACK.bg);
    }

    #[test]
    fn case_insensitive_lookup() {
        let lower = block_color("amp");
        let upper = block_color("AMP");
        let mixed = block_color("Amp");
        assert_eq!(lower.bg, upper.bg);
        assert_eq!(lower.bg, mixed.bg);
    }
}
