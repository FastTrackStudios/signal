//! Theme support for syntax highlighting.
//!
//! Provides color mappings for highlight kinds, with preset themes
//! for common use cases (dark mode, light mode, terminal).

use super::HighlightKind;
use facet::Facet;
use serde::{Deserialize, Serialize};

/// An RGBA color value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Facet)]
pub struct Color {
    /// Red component (0-255)
    pub r: u8,
    /// Green component (0-255)
    pub g: u8,
    /// Blue component (0-255)
    pub b: u8,
    /// Alpha component (0-255, 255 = fully opaque)
    pub a: u8,
}

impl Color {
    /// Create a new color with full opacity.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Create a new color with specified alpha.
    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Create a color from a hex string (e.g., "#FF5500" or "FF5500").
    #[must_use]
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 && hex.len() != 8 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        let a = if hex.len() == 8 {
            u8::from_str_radix(&hex[6..8], 16).ok()?
        } else {
            255
        };
        Some(Self { r, g, b, a })
    }

    /// Convert to a hex string (without # prefix).
    #[must_use]
    pub fn to_hex(&self) -> String {
        if self.a == 255 {
            format!("{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
    }

    /// Convert to CSS color string.
    #[must_use]
    pub fn to_css(&self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!(
                "rgba({}, {}, {}, {:.2})",
                self.r,
                self.g,
                self.b,
                f64::from(self.a) / 255.0
            )
        }
    }

    /// Convert to ANSI 256-color code (approximate).
    #[must_use]
    pub fn to_ansi_256(&self) -> u8 {
        // Convert RGB to 6x6x6 color cube index
        let r = (u16::from(self.r) * 5 / 255) as u8;
        let g = (u16::from(self.g) * 5 / 255) as u8;
        let b = (u16::from(self.b) * 5 / 255) as u8;
        16 + 36 * r + 6 * g + b
    }

    // Common colors
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}

/// Style properties for a highlight kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct Style {
    /// Foreground (text) color
    pub color: Color,
    /// Whether text should be bold
    pub bold: bool,
    /// Whether text should be italic
    pub italic: bool,
    /// Whether text should be underlined
    pub underline: bool,
}

impl Style {
    /// Create a basic style with just a color.
    #[must_use]
    pub const fn color(color: Color) -> Self {
        Self {
            color,
            bold: false,
            italic: false,
            underline: false,
        }
    }

    /// Create a bold style with a color.
    #[must_use]
    pub const fn bold(color: Color) -> Self {
        Self {
            color,
            bold: true,
            italic: false,
            underline: false,
        }
    }

    /// Create an italic style with a color.
    #[must_use]
    pub const fn italic(color: Color) -> Self {
        Self {
            color,
            bold: false,
            italic: true,
            underline: false,
        }
    }

    /// Make this style bold.
    #[must_use]
    pub const fn with_bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Make this style italic.
    #[must_use]
    pub const fn with_italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Make this style underlined.
    #[must_use]
    pub const fn with_underline(mut self) -> Self {
        self.underline = true;
        self
    }
}

impl Default for Style {
    fn default() -> Self {
        Self::color(Color::WHITE)
    }
}

/// A theme defines colors and styles for all highlight kinds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Theme {
    /// Name of this theme
    pub name: String,

    /// Background color (for HTML output)
    pub background: Color,

    /// Default text color
    pub foreground: Color,

    // ==================== Chord Components ====================
    /// Style for chord root notes
    pub root: Style,
    /// Style for scale degrees
    pub scale_degree: Style,
    /// Style for roman numerals
    pub roman_numeral: Style,
    /// Style for accidentals
    pub accidental: Style,
    /// Style for chord quality
    pub quality: Style,
    /// Style for extensions
    pub extension: Style,
    /// Style for modifiers
    pub modifier: Style,
    /// Style for bass notes
    pub bass: Style,

    // ==================== Rhythm ====================
    /// Style for duration notation
    pub duration: Style,
    /// Style for slash rhythm
    pub slash_rhythm: Style,
    /// Style for rests
    pub rest: Style,
    /// Style for push/pull
    pub push_pull: Style,

    // ==================== Structure ====================
    /// Style for section keywords
    pub section: Style,
    /// Style for measure counts
    pub measure_count: Style,
    /// Style for section comments
    pub section_comment: Style,
    /// Style for measure separators
    pub measure_separator: Style,

    // ==================== Metadata ====================
    /// Style for titles
    pub title: Style,
    /// Style for tempo
    pub tempo: Style,
    /// Style for time signature
    pub time_signature: Style,
    /// Style for key signature
    pub key: Style,

    // ==================== Other ====================
    /// Style for comments
    pub comment: Style,
    /// Style for commands
    pub command: Style,
    /// Style for dynamic markings
    pub dynamic: Style,
    /// Style for memory recall
    pub memory_recall: Style,
    /// Style for unknown/error tokens
    pub unknown: Style,
}

impl Theme {
    /// Get the style for a highlight kind.
    #[must_use]
    pub fn style_for(&self, kind: HighlightKind) -> &Style {
        match kind {
            HighlightKind::Root => &self.root,
            HighlightKind::ScaleDegree => &self.scale_degree,
            HighlightKind::RomanNumeral => &self.roman_numeral,
            HighlightKind::Accidental => &self.accidental,
            HighlightKind::Quality => &self.quality,
            HighlightKind::Extension => &self.extension,
            HighlightKind::Modifier => &self.modifier,
            HighlightKind::Bass | HighlightKind::BassSlash => &self.bass,
            HighlightKind::Duration | HighlightKind::Triplet | HighlightKind::Dot => &self.duration,
            HighlightKind::SlashRhythm => &self.slash_rhythm,
            HighlightKind::Rest | HighlightKind::Space => &self.rest,
            HighlightKind::Push | HighlightKind::Pull => &self.push_pull,
            HighlightKind::Section | HighlightKind::SectionBracket => &self.section,
            HighlightKind::MeasureCount => &self.measure_count,
            HighlightKind::SectionComment => &self.section_comment,
            HighlightKind::MeasureSeparator => &self.measure_separator,
            HighlightKind::Title | HighlightKind::Artist => &self.title,
            HighlightKind::Tempo | HighlightKind::TempoArrow => &self.tempo,
            HighlightKind::TimeSignature => &self.time_signature,
            HighlightKind::Key => &self.key,
            HighlightKind::Comment | HighlightKind::CommentMarker => &self.comment,
            HighlightKind::Command | HighlightKind::TextCue => &self.command,
            HighlightKind::Dynamic => &self.dynamic,
            HighlightKind::MemoryRecall | HighlightKind::Repeat => &self.memory_recall,
            HighlightKind::TrackMarker | HighlightKind::MelodyBlock => &self.section,
            HighlightKind::Whitespace => &self.unknown, // Use default for whitespace
            HighlightKind::Unknown => &self.unknown,
        }
    }

    /// Create a dark theme optimized for dark backgrounds.
    ///
    /// Uses a refined color palette inspired by professional music notation
    /// and modern code editors, with careful attention to harmony and readability.
    #[must_use]
    pub fn default_dark() -> Self {
        Self {
            name: "Keyflow Dark".to_string(),
            background: Color::rgb(24, 24, 28),
            foreground: Color::rgb(200, 200, 205),

            // Chord components - warm amber/gold palette for harmony elements
            root: Style::bold(Color::rgb(240, 180, 100)), // Warm amber - primary chord element
            scale_degree: Style::bold(Color::rgb(240, 180, 100)), // Same as root
            roman_numeral: Style::bold(Color::rgb(240, 180, 100)), // Same as root
            accidental: Style::color(Color::rgb(220, 140, 95)), // Burnt orange - stands out but harmonizes
            quality: Style::color(Color::rgb(130, 190, 140)),   // Sage green - soft contrast
            extension: Style::color(Color::rgb(140, 180, 220)), // Soft blue - complements amber
            modifier: Style::color(Color::rgb(180, 140, 200)),  // Soft lavender
            bass: Style::color(Color::rgb(200, 150, 120)),      // Warm tan

            // Rhythm - cooler, more subdued tones
            duration: Style::color(Color::rgb(120, 180, 190)), // Muted teal
            slash_rhythm: Style::color(Color::rgb(120, 120, 125)), // Subtle gray
            rest: Style::italic(Color::rgb(110, 110, 115)),    // Quieter gray italic
            push_pull: Style::color(Color::rgb(180, 150, 110)), // Muted gold

            // Structure - accent colors for navigation
            section: Style::bold(Color::rgb(130, 200, 170)), // Seafoam green - eye-catching but calm
            measure_count: Style::color(Color::rgb(170, 160, 140)), // Warm gray
            section_comment: Style::italic(Color::rgb(140, 160, 130)), // Olive italic
            measure_separator: Style::color(Color::rgb(80, 80, 85)), // Subtle separator

            // Metadata - distinctive but not distracting
            title: Style::bold(Color::rgb(220, 200, 160)), // Cream/gold
            tempo: Style::color(Color::rgb(150, 180, 200)), // Steel blue
            time_signature: Style::color(Color::rgb(150, 180, 200)), // Steel blue
            key: Style::color(Color::rgb(160, 190, 150)),  // Sage

            // Other
            comment: Style::italic(Color::rgb(100, 130, 100)), // Muted forest green
            command: Style::color(Color::rgb(170, 140, 190)),  // Soft purple
            dynamic: Style::bold(Color::rgb(200, 130, 120)),   // Muted coral
            memory_recall: Style::color(Color::rgb(140, 180, 180)), // Muted cyan
            unknown: Style::color(Color::rgb(200, 90, 90)),    // Muted error red
        }
    }

    /// Create a light theme optimized for light backgrounds.
    ///
    /// Uses colors inspired by popular light editor themes.
    #[must_use]
    pub fn default_light() -> Self {
        Self {
            name: "Keyflow Light".to_string(),
            background: Color::rgb(255, 255, 255),
            foreground: Color::rgb(36, 36, 36),

            // Chord components - darker, saturated versions
            root: Style::bold(Color::rgb(179, 119, 0)), // Dark orange
            scale_degree: Style::bold(Color::rgb(179, 119, 0)),
            roman_numeral: Style::bold(Color::rgb(179, 119, 0)),
            accidental: Style::color(Color::rgb(194, 69, 69)), // Dark red
            quality: Style::color(Color::rgb(80, 120, 60)),    // Forest green
            extension: Style::color(Color::rgb(0, 102, 153)),  // Dark blue
            modifier: Style::color(Color::rgb(130, 70, 150)),  // Dark purple
            bass: Style::color(Color::rgb(170, 70, 70)),       // Dark salmon

            // Rhythm
            duration: Style::color(Color::rgb(0, 128, 128)), // Teal
            slash_rhythm: Style::color(Color::rgb(100, 100, 100)),
            rest: Style::italic(Color::rgb(100, 100, 100)),
            push_pull: Style::color(Color::rgb(150, 100, 50)), // Brown

            // Structure
            section: Style::bold(Color::rgb(130, 70, 150)), // Purple
            measure_count: Style::color(Color::rgb(150, 100, 50)),
            section_comment: Style::italic(Color::rgb(60, 120, 60)),
            measure_separator: Style::color(Color::rgb(150, 150, 150)),

            // Metadata
            title: Style::bold(Color::rgb(140, 100, 30)), // Dark gold
            tempo: Style::color(Color::rgb(0, 102, 153)),
            time_signature: Style::color(Color::rgb(0, 102, 153)),
            key: Style::color(Color::rgb(80, 120, 60)),

            // Other
            comment: Style::italic(Color::rgb(60, 120, 60)), // Forest green
            command: Style::color(Color::rgb(130, 70, 150)),
            dynamic: Style::bold(Color::rgb(170, 70, 70)),
            memory_recall: Style::color(Color::rgb(0, 128, 128)),
            unknown: Style::color(Color::rgb(200, 0, 0)), // Error red
        }
    }

    /// Create a high-contrast theme for accessibility.
    #[must_use]
    pub fn high_contrast() -> Self {
        Self {
            name: "Keyflow High Contrast".to_string(),
            background: Color::BLACK,
            foreground: Color::WHITE,

            root: Style::bold(Color::rgb(255, 255, 0)), // Yellow
            scale_degree: Style::bold(Color::rgb(255, 255, 0)),
            roman_numeral: Style::bold(Color::rgb(255, 255, 0)),
            accidental: Style::color(Color::rgb(255, 128, 128)), // Light red
            quality: Style::color(Color::rgb(128, 255, 128)),    // Light green
            extension: Style::color(Color::rgb(128, 200, 255)),  // Light blue
            modifier: Style::color(Color::rgb(255, 128, 255)),   // Light magenta
            bass: Style::color(Color::rgb(255, 200, 128)),       // Light orange

            duration: Style::color(Color::rgb(0, 255, 255)), // Cyan
            slash_rhythm: Style::color(Color::rgb(192, 192, 192)),
            rest: Style::italic(Color::rgb(192, 192, 192)),
            push_pull: Style::color(Color::rgb(255, 200, 100)),

            section: Style::bold(Color::rgb(255, 128, 255)), // Magenta
            measure_count: Style::color(Color::rgb(255, 200, 100)),
            section_comment: Style::italic(Color::rgb(128, 255, 128)),
            measure_separator: Style::color(Color::rgb(128, 128, 128)),

            title: Style::bold(Color::WHITE),
            tempo: Style::color(Color::rgb(128, 200, 255)),
            time_signature: Style::color(Color::rgb(128, 200, 255)),
            key: Style::color(Color::rgb(128, 255, 128)),

            comment: Style::italic(Color::rgb(128, 255, 128)),
            command: Style::color(Color::rgb(255, 128, 255)),
            dynamic: Style::bold(Color::rgb(255, 128, 128)),
            memory_recall: Style::color(Color::rgb(0, 255, 255)),
            unknown: Style::color(Color::rgb(255, 0, 0)).with_underline(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::default_dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_from_hex() {
        assert_eq!(Color::from_hex("#FF5500"), Some(Color::rgb(255, 85, 0)));
        assert_eq!(Color::from_hex("FF5500"), Some(Color::rgb(255, 85, 0)));
        assert_eq!(
            Color::from_hex("#FF550080"),
            Some(Color::rgba(255, 85, 0, 128))
        );
        assert_eq!(Color::from_hex("invalid"), None);
        assert_eq!(Color::from_hex("#FFF"), None); // Too short
    }

    #[test]
    fn test_color_to_hex() {
        assert_eq!(Color::rgb(255, 85, 0).to_hex(), "FF5500");
        assert_eq!(Color::rgba(255, 85, 0, 128).to_hex(), "FF550080");
    }

    #[test]
    fn test_color_to_css() {
        assert_eq!(Color::rgb(255, 85, 0).to_css(), "#ff5500");
        // Alpha colors use rgba()
        let css = Color::rgba(255, 85, 0, 128).to_css();
        assert!(css.starts_with("rgba(255, 85, 0,"));
    }

    #[test]
    fn test_style_builders() {
        let style = Style::color(Color::WHITE).with_bold().with_italic();
        assert!(style.bold);
        assert!(style.italic);
        assert!(!style.underline);
    }

    #[test]
    fn test_theme_style_for() {
        let theme = Theme::default_dark();
        let root_style = theme.style_for(HighlightKind::Root);
        assert!(root_style.bold);

        let comment_style = theme.style_for(HighlightKind::Comment);
        assert!(comment_style.italic);
    }

    #[test]
    fn test_themes_have_names() {
        assert_eq!(Theme::default_dark().name, "Keyflow Dark");
        assert_eq!(Theme::default_light().name, "Keyflow Light");
        assert_eq!(Theme::high_contrast().name, "Keyflow High Contrast");
    }
}
