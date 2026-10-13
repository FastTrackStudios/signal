//! Section Color Palette
//!
//! Provides semantic colors for sections based on their type.
//! Colors are sourced from [`music_catalog`] — the single source of truth
//! for section colors across FastTrackStudio.
//!
//! ## Color Philosophy
//!
//! - **Bright variants** (400/500 shades): Used for progress bars, boundaries, active elements
//! - **Muted variants** (200 shades): Used for backgrounds, inactive states, pre/post sections
//! - **Text colors** (800 shades): Used for text on muted backgrounds

use super::SectionType;

// Re-export the SectionColorSet from music-catalog as SectionColors for backward compatibility
pub use music_catalog::sections::SectionColorSet as SectionColors;

// Re-export the UI palettes from music-catalog
pub use music_catalog::sections::ui_palettes as palettes;

/// Tag's colour set, built from the catalog's `TAG` (purple-300) the way
/// the UI palettes build theirs.
const TAG: SectionColors = SectionColors::new(
    color_palette::palette::purple::S300,
    color_palette::palette::purple::S100,
    color_palette::palette::purple::S800,
);

/// Get semantic colors for a section type.
///
/// Uses the canonical palettes from [`music_catalog`] to ensure consistency
/// with auto-coloring in REAPER and other FTS tools.
#[must_use]
pub fn colors_for_section_type(section_type: &SectionType) -> SectionColors {
    use palettes::*;

    match section_type {
        // Main sections - distinct colors for each
        SectionType::Intro => INTRO,
        SectionType::Verse => VERSE,
        SectionType::Chorus => CHORUS,
        SectionType::Bridge => BRIDGE,
        SectionType::Outro => OUTRO,
        SectionType::Instrumental => INSTRUMENTAL,
        SectionType::Solo => SOLO,
        SectionType::Interlude => INTERLUDE,

        // Pre/Post sections - use parent section's muted as bright
        SectionType::Pre(inner) | SectionType::Post(inner) => {
            let parent = colors_for_section_type(inner);
            SectionColors {
                bright: parent.muted,
                muted: parent.muted,
                text: parent.text,
            }
        }

        // Vamp sections - lime (repeated/improvisation)
        SectionType::Vamp => VAMP,

        // Refrain - the recurring hook reads as chorus-family (blue), matching
        // the catalog's own REFRAIN = blue::S500 = CHORUS.
        SectionType::Refrain => CHORUS,

        // Turnaround - a short transitional instrumental link (reads like a vamp).
        SectionType::Turnaround => VAMP,

        // Tag - the catalog's TAG colour (purple-300, a lighter bridge);
        // the UI palettes have no tag set of their own yet.
        SectionType::Tag => TAG,

        // Utility sections - neutral colors
        SectionType::CountIn | SectionType::Opening | SectionType::End => SLATE,
        SectionType::Hits | SectionType::Breakdown => SLATE,

        // Custom sections - slate
        SectionType::Custom(_) => SLATE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verse_colors() {
        let colors = colors_for_section_type(&SectionType::Verse);
        assert_eq!(colors.bright_css(), "rgb(52, 211, 153)");
        assert_eq!(colors.bright_hex(), "#34d399");
    }

    #[test]
    fn test_chorus_colors() {
        let colors = colors_for_section_type(&SectionType::Chorus);
        assert_eq!(colors.bright_css(), "rgb(59, 130, 246)");
    }

    #[test]
    fn test_pre_verse_uses_muted_verse() {
        let pre_verse = colors_for_section_type(&SectionType::Pre(Box::new(SectionType::Verse)));
        let verse = colors_for_section_type(&SectionType::Verse);
        // Pre-verse bright should be verse muted
        assert_eq!(pre_verse.bright, verse.muted);
    }

    #[test]
    fn test_color_conversions() {
        let colors = colors_for_section_type(&SectionType::Intro);
        // Sky 400 = Intro color from music-catalog
        let intro_color = music_catalog::sections::colors::INTRO;
        assert_eq!(colors.bright, intro_color);
    }
}
