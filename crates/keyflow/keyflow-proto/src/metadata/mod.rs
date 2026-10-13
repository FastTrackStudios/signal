//! Metadata Module
//!
//! Song metadata parsing and representation

pub mod parser;

use facet::Facet;

/// Song Metadata
///
/// Represents song information like title, artist, composer, etc.
/// Complete song metadata
#[derive(Debug, Clone, PartialEq, Default, Facet)]
pub struct SongMetadata {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub artist: Option<String>,
    pub composer: Option<String>,
    pub writer: Option<String>,
    pub arranger: Option<String>,
    pub lyricist: Option<String>,
    pub copyright: Option<String>,
    pub year: Option<u16>,
    pub tempo: Option<u32>,
    /// Part name (e.g., "Master Rhythm", "Lead Sheet", "Piano")
    pub part_name: Option<String>,
    /// Chart version (1, 2, 3, etc.) - defaults to 1 in rendering
    pub version: Option<u8>,
}

impl SongMetadata {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_integration() {
        // Test that parser and metadata work together
        let (title, artist) = SongMetadata::parse_title_artist("Integration Test - Test Artist");

        let mut metadata = SongMetadata::new();
        metadata.title = title;
        metadata.artist = artist;

        assert_eq!(metadata.title, Some("Integration Test".to_string()));
        assert_eq!(metadata.artist, Some("Test Artist".to_string()));
    }
}
