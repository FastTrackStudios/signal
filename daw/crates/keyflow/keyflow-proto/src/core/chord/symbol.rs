//! Chord symbol trait.
//!
//! Provides a unified interface for chord symbol representations,
//! enabling different chord types to be rendered consistently.

/// Unified interface for chord symbol representations.
///
/// This trait allows different chord types to be used interchangeably
/// for rendering and conversion. It provides access to the components
/// needed to display a chord symbol.
///
/// # Example
///
/// ```ignore
/// use keyflow_proto::core::ChordSymbol;
///
/// fn render_chord<C: ChordSymbol>(chord: &C) {
///     println!("Root: {}", chord.root_str());
///     println!("Symbol: {}", chord.to_symbol_string());
/// }
/// ```
pub trait ChordSymbol {
    /// Get the root note name as a string (e.g., "C", "F#", "Bb").
    fn root_str(&self) -> String;

    /// Get the quality symbol (e.g., "", "m", "dim", "aug", "sus4").
    fn quality_str(&self) -> &str;

    /// Get the seventh/family symbol if present (e.g., "7", "maj7", "dim7").
    fn seventh_str(&self) -> Option<&str>;

    /// Get the extensions as a string (e.g., "9", "11", "13").
    fn extensions_str(&self) -> String;

    /// Get the alterations as a string (e.g., "b5", "#9").
    fn alterations_str(&self) -> String;

    /// Get the bass note for slash chords, if present (e.g., "G" in "C/G").
    fn bass_str(&self) -> Option<String>;

    // ================== Provided methods ==================

    /// Build the complete chord symbol string (e.g., "Cm7", "GMaj7/B", "F#m7b5").
    fn to_symbol_string(&self) -> String {
        let mut result = self.root_str();

        // Add quality
        result.push_str(self.quality_str());

        // Add seventh/family
        if let Some(seventh) = self.seventh_str() {
            result.push_str(seventh);
        }

        // Add extensions
        let extensions = self.extensions_str();
        if !extensions.is_empty() {
            result.push_str(&extensions);
        }

        // Add alterations
        let alterations = self.alterations_str();
        if !alterations.is_empty() {
            result.push_str(&alterations);
        }

        // Add bass note for slash chords
        if let Some(bass) = self.bass_str() {
            result.push('/');
            result.push_str(&bass);
        }

        result
    }

    /// Check if this is a major chord (major quality, no seventh or major 7th).
    fn is_major(&self) -> bool {
        self.quality_str().is_empty()
            && matches!(
                self.seventh_str(),
                None | Some("maj7") | Some("Maj7") | Some("M7")
            )
    }

    /// Check if this is a minor chord.
    fn is_minor(&self) -> bool {
        self.quality_str() == "m"
    }

    /// Check if this is a dominant 7th chord (major triad with minor 7th).
    fn is_dominant(&self) -> bool {
        self.quality_str().is_empty() && self.seventh_str() == Some("7")
    }

    /// Check if this is a slash chord (has a bass note different from root).
    fn is_slash_chord(&self) -> bool {
        self.bass_str().is_some()
    }

    /// Check if this chord has a seventh.
    fn has_seventh(&self) -> bool {
        self.seventh_str().is_some()
    }

    /// Check if this is a diminished chord.
    fn is_diminished(&self) -> bool {
        self.quality_str() == "dim" || self.quality_str() == "°"
    }

    /// Check if this is an augmented chord.
    fn is_augmented(&self) -> bool {
        self.quality_str() == "aug" || self.quality_str() == "+"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Simple test implementation
    struct TestChord {
        root: String,
        quality: &'static str,
        seventh: Option<&'static str>,
        extensions: String,
        alterations: String,
        bass: Option<String>,
    }

    impl TestChord {
        fn new(root: &str) -> Self {
            Self {
                root: root.to_string(),
                quality: "",
                seventh: None,
                extensions: String::new(),
                alterations: String::new(),
                bass: None,
            }
        }

        fn minor(mut self) -> Self {
            self.quality = "m";
            self
        }

        fn seventh(mut self, s: &'static str) -> Self {
            self.seventh = Some(s);
            self
        }

        fn with_bass(mut self, bass: &str) -> Self {
            self.bass = Some(bass.to_string());
            self
        }
    }

    impl ChordSymbol for TestChord {
        fn root_str(&self) -> String {
            self.root.clone()
        }
        fn quality_str(&self) -> &str {
            self.quality
        }
        fn seventh_str(&self) -> Option<&str> {
            self.seventh
        }
        fn extensions_str(&self) -> String {
            self.extensions.clone()
        }
        fn alterations_str(&self) -> String {
            self.alterations.clone()
        }
        fn bass_str(&self) -> Option<String> {
            self.bass.clone()
        }
    }

    #[test]
    fn test_to_symbol_string_major() {
        let chord = TestChord::new("C");
        assert_eq!(chord.to_symbol_string(), "C");
    }

    #[test]
    fn test_to_symbol_string_minor() {
        let chord = TestChord::new("A").minor();
        assert_eq!(chord.to_symbol_string(), "Am");
    }

    #[test]
    fn test_to_symbol_string_with_seventh() {
        let chord = TestChord::new("G").seventh("7");
        assert_eq!(chord.to_symbol_string(), "G7");

        let chord = TestChord::new("C").seventh("maj7");
        assert_eq!(chord.to_symbol_string(), "Cmaj7");
    }

    #[test]
    fn test_to_symbol_string_slash_chord() {
        let chord = TestChord::new("C").with_bass("G");
        assert_eq!(chord.to_symbol_string(), "C/G");
    }

    #[test]
    fn test_is_major() {
        let major = TestChord::new("C");
        assert!(major.is_major());

        let minor = TestChord::new("A").minor();
        assert!(!minor.is_major());

        let dom7 = TestChord::new("G").seventh("7");
        assert!(!dom7.is_major());

        let maj7 = TestChord::new("C").seventh("maj7");
        assert!(maj7.is_major());
    }

    #[test]
    fn test_is_dominant() {
        let dom7 = TestChord::new("G").seventh("7");
        assert!(dom7.is_dominant());

        let maj7 = TestChord::new("C").seventh("maj7");
        assert!(!maj7.is_dominant());
    }
}
