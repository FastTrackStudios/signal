//! Metadata Parser
//!
//! Parses song metadata from chart input

use super::SongMetadata;

impl SongMetadata {
    /// Parse a title/artist line, optionally extracting subtitle from parentheses
    ///
    /// Format: "Song Title - Artist Name"
    ///         "Song Title (Subtitle) - Artist Name"
    ///         "Song Title"
    ///
    /// Returns: (title, artist, subtitle)
    pub fn parse_title_artist_subtitle(
        input: &str,
    ) -> (Option<String>, Option<String>, Option<String>) {
        let input = input.trim();

        if input.is_empty() {
            return (None, None, None);
        }

        // First split by " - " to get title+subtitle and artist
        let (title_part, artist) = if let Some(dash_pos) = input.find(" - ") {
            let title = input[..dash_pos].trim().to_string();
            let artist = input[dash_pos + 3..].trim().to_string();
            (
                title,
                if artist.is_empty() {
                    None
                } else {
                    Some(artist)
                },
            )
        } else {
            (input.to_string(), None)
        };

        // Now extract subtitle from parentheses in the title part
        // e.g., "Amazing Grace (Hymn)" -> title="Amazing Grace", subtitle="Hymn"
        let (title, subtitle) = Self::extract_subtitle_from_parentheses(&title_part);

        (
            if title.is_empty() { None } else { Some(title) },
            artist,
            subtitle,
        )
    }

    /// Parse a title/artist line (legacy method for backwards compatibility)
    ///
    /// Format: "Song Title - Artist Name"
    /// or just: "Song Title"
    pub fn parse_title_artist(input: &str) -> (Option<String>, Option<String>) {
        let (title, artist, _subtitle) = Self::parse_title_artist_subtitle(input);
        (title, artist)
    }

    /// Extract subtitle from parentheses in a title string
    ///
    /// "Amazing Grace (Hymn)" -> ("Amazing Grace", Some("Hymn"))
    /// "Amazing Grace ()" -> ("Amazing Grace", None) - empty parens stripped
    /// "Amazing Grace" -> ("Amazing Grace", None)
    fn extract_subtitle_from_parentheses(title: &str) -> (String, Option<String>) {
        // Find the last opening parenthesis to handle nested parens
        if let Some(open_paren) = title.rfind('(')
            && let Some(close_paren) = title[open_paren..].find(')')
        {
            let subtitle = title[open_paren + 1..open_paren + close_paren].trim();
            let title_without_subtitle = title[..open_paren].trim();

            // Return subtitle if non-empty, but always strip parentheses from title
            return (
                title_without_subtitle.to_string(),
                if subtitle.is_empty() {
                    None
                } else {
                    Some(subtitle.to_string())
                },
            );
        }

        (title.to_string(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_title_only() {
        let (title, artist) = SongMetadata::parse_title_artist("Amazing Grace");
        assert_eq!(title, Some("Amazing Grace".to_string()));
        assert_eq!(artist, None);
    }

    #[test]
    fn test_parse_title_and_artist() {
        let (title, artist) = SongMetadata::parse_title_artist("Reckless Love - Cory Asbury");
        assert_eq!(title, Some("Reckless Love".to_string()));
        assert_eq!(artist, Some("Cory Asbury".to_string()));
    }

    #[test]
    fn test_parse_empty_string() {
        let (title, artist) = SongMetadata::parse_title_artist("");
        assert_eq!(title, None);
        assert_eq!(artist, None);
    }

    #[test]
    fn test_parse_with_extra_spaces() {
        let (title, artist) = SongMetadata::parse_title_artist("  Song Title  -  Artist Name  ");
        assert_eq!(title, Some("Song Title".to_string()));
        assert_eq!(artist, Some("Artist Name".to_string()));
    }

    #[test]
    fn test_parse_multiple_dashes() {
        let (title, artist) = SongMetadata::parse_title_artist("Title - With - Dashes - Artist");
        // Should only split on first " - "
        assert_eq!(title, Some("Title".to_string()));
        assert_eq!(artist, Some("With - Dashes - Artist".to_string()));
    }

    #[test]
    fn test_parse_title_with_dash_no_spaces() {
        let (title, artist) = SongMetadata::parse_title_artist("Title-WithDash");
        // No " - " separator, so entire string is title
        assert_eq!(title, Some("Title-WithDash".to_string()));
        assert_eq!(artist, None);
    }

    // Subtitle parsing tests
    #[test]
    fn test_parse_with_subtitle() {
        let (title, artist, subtitle) =
            SongMetadata::parse_title_artist_subtitle("Amazing Grace (Hymn) - John Newton");
        assert_eq!(title, Some("Amazing Grace".to_string()));
        assert_eq!(artist, Some("John Newton".to_string()));
        assert_eq!(subtitle, Some("Hymn".to_string()));
    }

    #[test]
    fn test_parse_with_subtitle_no_artist() {
        let (title, artist, subtitle) =
            SongMetadata::parse_title_artist_subtitle("Amazing Grace (Traditional Hymn)");
        assert_eq!(title, Some("Amazing Grace".to_string()));
        assert_eq!(artist, None);
        assert_eq!(subtitle, Some("Traditional Hymn".to_string()));
    }

    #[test]
    fn test_parse_without_subtitle() {
        let (title, artist, subtitle) =
            SongMetadata::parse_title_artist_subtitle("Amazing Grace - John Newton");
        assert_eq!(title, Some("Amazing Grace".to_string()));
        assert_eq!(artist, Some("John Newton".to_string()));
        assert_eq!(subtitle, None);
    }

    #[test]
    fn test_parse_subtitle_with_spaces() {
        let (title, artist, subtitle) =
            SongMetadata::parse_title_artist_subtitle("Song Title ( Spaced Subtitle ) - Artist");
        assert_eq!(title, Some("Song Title".to_string()));
        assert_eq!(artist, Some("Artist".to_string()));
        assert_eq!(subtitle, Some("Spaced Subtitle".to_string()));
    }

    #[test]
    fn test_parse_empty_parentheses() {
        let (title, artist, subtitle) =
            SongMetadata::parse_title_artist_subtitle("Song Title () - Artist");
        assert_eq!(title, Some("Song Title".to_string()));
        assert_eq!(artist, Some("Artist".to_string()));
        assert_eq!(subtitle, None); // Empty parens should not create subtitle
    }
}
