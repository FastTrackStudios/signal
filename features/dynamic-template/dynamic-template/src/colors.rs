//! Track and region colors based on instrument/section classification.
//!
//! All color definitions and lookup tables live in [`music_catalog`].
//! This module re-exports them for backward compatibility and adds
//! REAPER-specific conversion utilities.
//!
//! # Usage
//!
//! ```rust
//! use dynamic_template::colors::{color_for_group, groups, drums};
//!
//! // Get color for a top-level group
//! let drum_color = groups::DRUMS;
//!
//! // Get color for a sub-group
//! let kick_color = drums::KICK;
//!
//! // Look up by name
//! if let Some(color) = color_for_group("Electric Guitar") {
//!     println!("Color: {}", color);
//! }
//! ```

// Re-export the Color type and palette
pub use color_palette::palette;
pub use color_palette::Color;

// Re-export all instrument color modules from music-catalog
pub use music_catalog::instruments::bass;
pub use music_catalog::instruments::drums;
pub use music_catalog::instruments::groups;
pub use music_catalog::instruments::guitars;
pub use music_catalog::instruments::keys;
pub use music_catalog::instruments::orchestra;
pub use music_catalog::instruments::percussion;
pub use music_catalog::instruments::synths;
pub use music_catalog::instruments::vocals;

// Re-export section colors as `guide` for backward compatibility
pub use music_catalog::sections::colors as section_colors;
pub use music_catalog::sections::cues;
pub use music_catalog::sections::utility;

/// Backward-compatible `guide` module that re-exports from music-catalog.
/// New code should prefer `section_colors` and `cues` directly.
pub mod guide {
    pub use music_catalog::sections::colors::*;
    pub use music_catalog::sections::cues::*;
    pub use music_catalog::sections::utility::*;
}

// Re-export lookup functions
pub use music_catalog::lookup::color_for_name as color_for_group;
pub use music_catalog::lookup::color_for_path;
pub use music_catalog::lookup::color_for_region;

// ============================================================================
// REAPER-specific color conversion utilities
// ============================================================================

/// Convert a color to REAPER's native format
#[must_use]
pub const fn to_reaper_color(color: Color) -> i32 {
    color.to_reaper_native()
}

/// Convert REAPER's native color format back to Color
#[must_use]
pub const fn from_reaper_color(native: i32) -> Color {
    Color::from_reaper_native(native)
}

// ============================================================================
// HSL Color Utilities (thin wrappers over color-palette methods)
// ============================================================================

/// Convert RGB color to HSL
#[must_use]
pub fn rgb_to_hsl(color: Color) -> (f32, f32, f32) {
    color.to_hsl()
}

/// Convert HSL to RGB color
#[must_use]
pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> Color {
    Color::from_hsl(h, s, l)
}

/// Adjust the lightness of a color
#[must_use]
pub fn adjust_lightness(color: Color, delta: f32) -> Color {
    let (h, s, l) = color.to_hsl();
    Color::from_hsl(h, s, (l + delta).clamp(0.0, 1.0))
}

/// Get a shade of a color (0-9 scale, 4 = base)
#[must_use]
pub fn get_shade(color: Color, shade: u8) -> Color {
    color.shade(shade)
}

/// Calculate a color on a gradient between two colors
#[must_use]
pub fn calc_gradient(start: Color, end: Color, position: f32) -> Color {
    start.mix(end, position)
}

/// Generate a full gradient between two colors
#[must_use]
pub fn generate_gradient(start: Color, end: Color, steps: usize) -> Vec<Color> {
    Color::gradient(start, end, steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_for_group_top_level() {
        assert_eq!(color_for_group("Drums"), Some(groups::DRUMS));
        assert_eq!(color_for_group("drums"), Some(groups::DRUMS));
        assert_eq!(color_for_group("DRUMS"), Some(groups::DRUMS));
        assert_eq!(color_for_group("Bass"), Some(groups::BASS));
        assert_eq!(color_for_group("Vocals"), Some(groups::VOCALS));
    }

    #[test]
    fn test_color_for_group_subgroups() {
        assert_eq!(color_for_group("Electric Guitar"), Some(guitars::ELECTRIC));
        assert_eq!(color_for_group("Kick"), Some(drums::KICK));
        assert_eq!(color_for_group("BGVs"), Some(vocals::BACKGROUND));
    }

    #[test]
    fn test_color_for_path() {
        assert_eq!(color_for_path(&["Drums", "Kick"]), Some(drums::KICK));
        assert_eq!(color_for_path(&["Drums"]), Some(groups::DRUMS));
    }

    #[test]
    fn test_color_for_region() {
        assert_eq!(color_for_region("Verse"), Some(guide::VERSE));
        assert_eq!(color_for_region("1. Verse"), Some(guide::VERSE));
        assert_eq!(color_for_region("V1"), Some(guide::VERSE));
        assert_eq!(color_for_region("CHORUS - 8 bars"), Some(guide::CHORUS));
    }

    #[test]
    fn test_hsl_roundtrip() {
        let color = palette::blue::S500;
        let (h, s, l) = rgb_to_hsl(color);
        let restored = hsl_to_rgb(h, s, l);
        let (r1, g1, b1) = color.to_rgb();
        let (r2, g2, b2) = restored.to_rgb();
        assert!((i16::from(r1) - i16::from(r2)).abs() <= 1);
        assert!((i16::from(g1) - i16::from(g2)).abs() <= 1);
        assert!((i16::from(b1) - i16::from(b2)).abs() <= 1);
    }

    #[test]
    fn test_generate_gradient() {
        let gradient = generate_gradient(Color::BLACK, Color::WHITE, 5);
        assert_eq!(gradient.len(), 5);
        assert_eq!(gradient[0], Color::BLACK);
        assert_eq!(gradient[4], Color::WHITE);
    }
}
