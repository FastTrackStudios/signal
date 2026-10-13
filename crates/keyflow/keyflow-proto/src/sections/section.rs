//! Section Definition
//!
//! Represents a section with its type and optional numbering
//! Supports both chart parsing (measure_count, is_subsection) and DAW integration (positions, id, etc.)

use super::section_type::SectionType;
use daw_proto::{Position, Tempo, TimePosition, TimeSignature};
use facet::Facet;
use std::collections::HashMap;

/// Represents a section with its type and optional number
/// Can be used for both chart parsing and DAW integration
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Section {
    /// Section type
    pub section_type: SectionType,
    /// Optional section number (e.g., 1, 2, 3 for Verse 1, Verse 2, etc.)
    pub number: Option<u32>,
    /// Split letter for consecutive sections (e.g., 'a', 'b', 'c')
    pub split_letter: Option<char>,
    /// Specified measure count (for chart parsing)
    pub measure_count: Option<usize>,
    /// True if prefixed with ^ (e.g., ^Band-In) - for chart parsing
    pub is_subsection: bool,
    /// Optional comment/annotation for the section (e.g., "Down", "Build", "Horns", "Half-time")
    /// Parsed from: CH 4 "Down", Interlude "Horns", or preset modifiers like "Down CH 4"
    pub comment: Option<String>,

    // Optional DAW integration fields
    /// Unique identifier for this section (for DAW integration)
    pub id: Option<String>,
    /// Start position (contains both musical and time position) - for DAW integration
    pub start_position: Option<Position>,
    /// End position (contains both musical and time position) - for DAW integration
    pub end_position: Option<Position>,
    /// Section name (from region name) - for DAW integration
    pub name: Option<String>,
    /// Color from the region (optional, for display purposes) - for DAW integration
    pub color: Option<u32>,
    /// Optional metadata - for DAW integration
    pub metadata: HashMap<String, String>,
}

impl Section {
    /// Create a new section (for chart parsing)
    pub fn new(section_type: SectionType) -> Self {
        Self {
            section_type,
            number: None,
            split_letter: None,
            measure_count: None,
            is_subsection: false,
            comment: None,
            id: None,
            start_position: None,
            end_position: None,
            name: None,
            color: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a new section with DAW positions (for DAW integration)
    pub fn with_positions(
        section_type: SectionType,
        start_position: Position,
        end_position: Position,
        name: String,
        number: Option<u32>,
    ) -> Self {
        Self {
            section_type,
            number,
            split_letter: None,
            measure_count: None,
            is_subsection: false,
            comment: None,
            id: None,
            start_position: Some(start_position),
            end_position: Some(end_position),
            name: Some(name),
            color: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a new section with DAW positions (for DAW integration, with validation)
    /// Returns an error if validation fails
    /// Note: This is different from `new()` which creates a section for chart parsing
    pub fn new_with_positions(
        section_type: SectionType,
        start_position: Position,
        end_position: Position,
        name: String,
        number: Option<u32>,
    ) -> Result<Self, String> {
        let start_time = start_position
            .time
            .as_ref()
            .ok_or_else(|| "Start position has no time component".to_string())?;
        let end_time = end_position
            .time
            .as_ref()
            .ok_or_else(|| "End position has no time component".to_string())?;

        if start_time.as_seconds() >= end_time.as_seconds() {
            return Err(format!(
                "Invalid time range: start ({}) >= end ({})",
                start_time.as_seconds(),
                end_time.as_seconds()
            ));
        }

        if name.trim().is_empty() {
            return Err("Section name cannot be empty".to_string());
        }

        Ok(Self::with_positions(
            section_type,
            start_position,
            end_position,
            name,
            number,
        ))
    }

    /// Create a new section with ID
    pub fn with_id(
        id: String,
        section_type: SectionType,
        start_position: Position,
        end_position: Position,
        name: String,
        number: Option<u32>,
    ) -> Result<Self, String> {
        let mut section =
            Self::new_with_positions(section_type, start_position, end_position, name, number)?;
        section.id = Some(id);
        Ok(section)
    }

    /// Create a section from seconds (for DAW integration compatibility)
    /// Uses default BPM (120) and time signature (4/4) to calculate musical positions
    pub fn from_seconds(
        section_type: SectionType,
        start_seconds: f64,
        end_seconds: f64,
        name: String,
        number: Option<u32>,
    ) -> Result<Self, String> {
        Self::from_seconds_with_tempo(
            section_type,
            start_seconds,
            end_seconds,
            name,
            number,
            120.0,                    // Default BPM
            TimeSignature::new(4, 4), // Default time signature
        )
    }

    /// Create a section from seconds with specified BPM and time signature
    pub fn from_seconds_with_tempo(
        section_type: SectionType,
        start_seconds: f64,
        end_seconds: f64,
        name: String,
        number: Option<u32>,
        bpm: f64,
        time_signature: daw_proto::TimeSignature,
    ) -> Result<Self, String> {
        use daw_proto::TimePosition;

        if start_seconds >= end_seconds {
            return Err(format!(
                "Invalid time range: start ({}) >= end ({})",
                start_seconds, end_seconds
            ));
        }

        if name.trim().is_empty() {
            return Err("Section name cannot be empty".to_string());
        }

        let tempo = Tempo::from_bpm(bpm);
        let start_time = TimePosition::from_seconds(start_seconds);
        let end_time = TimePosition::from_seconds(end_seconds);

        // Calculate musical positions from time positions
        let start_musical = start_time.to_musical(tempo, time_signature);
        let end_musical = end_time.to_musical(tempo, time_signature);

        let start_position = Position::new(Some(start_musical), Some(start_time), None);
        let end_position = Position::new(Some(end_musical), Some(end_time), None);

        Ok(Self::with_positions(
            section_type,
            start_position,
            end_position,
            name,
            number,
        ))
    }

    pub fn with_subsection(mut self, is_subsection: bool) -> Self {
        self.is_subsection = is_subsection;
        self
    }

    pub fn with_measure_count(mut self, count: usize) -> Self {
        self.measure_count = Some(count);
        self
    }

    /// Set the comment/annotation for this section (e.g., "Down", "Build", "Horns", "Half-time")
    pub fn with_comment<S: Into<String>>(mut self, comment: S) -> Self {
        self.comment = Some(comment.into());
        self
    }

    /// Get a display name for this section
    pub fn display_name(&self) -> String {
        // If we have a name from DAW, use it (but still format with number/split letter if needed)
        if let Some(ref name) = self.name {
            // For custom sections, use the name as-is
            if matches!(self.section_type, SectionType::Custom(_)) {
                return name.clone();
            }

            // For other sections, check if the name already includes the number
            // If not, add number and split letter if present
            let with_number = if let Some(num) = self.number {
                if !name.contains(&num.to_string()) {
                    format!("{} {}", name, num)
                } else {
                    name.clone()
                }
            } else {
                name.clone()
            };

            if let Some(letter) = self.split_letter {
                if !with_number.ends_with(letter) {
                    format!("{}{}", with_number, letter)
                } else {
                    with_number
                }
            } else {
                with_number
            }
        } else {
            // Chart parsing mode - use section type
            let base = self.section_type.full_name();
            let prefix = if self.is_subsection { "^" } else { "" };

            match (self.number, self.split_letter) {
                (Some(n), Some(l)) => format!("{}{} {}{}", prefix, base, n, l),
                (Some(n), None) => format!("{}{} {}", prefix, base, n),
                (None, _) => format!("{}{}", prefix, base),
            }
        }
    }

    /// Get display name with comment if present (e.g., "Interlude C (Woodwinds)")
    pub fn display_name_with_comment(&self) -> String {
        match &self.comment {
            Some(comment) => format!("{} ({})", self.display_name(), comment),
            None => self.display_name(),
        }
    }

    /// Get a short display name for space-constrained UI contexts
    ///
    /// Converts section names to abbreviated forms:
    /// - "Interlude A" -> "INT A"
    /// - "Verse 1" -> "VS 1"
    /// - "Chorus 2" -> "CH 2"
    /// - "Pre-Chorus" -> "PRE-CH"
    /// - "Bridge 1" -> "BR 1"
    /// - "Outro A" -> "OUT A"
    pub fn short_display(&self) -> String {
        let abbrev = self.section_type.abbreviation();

        match (self.number, self.split_letter) {
            (Some(n), Some(l)) => format!("{} {}{}", abbrev, n, l),
            (Some(n), None) => format!("{} {}", abbrev, n),
            (None, Some(l)) => format!("{} {}", abbrev, l),
            (None, None) => abbrev,
        }
    }

    /// Calculate the duration of the section in seconds (for DAW integration)
    pub fn duration_seconds(&self) -> Option<f64> {
        if let (Some(start), Some(end)) = (&self.start_position, &self.end_position) {
            let start_time = start.time.as_ref()?;
            let end_time = end.time.as_ref()?;
            Some(end_time.as_seconds() - start_time.as_seconds())
        } else {
            None
        }
    }

    /// Get start time in seconds (for DAW integration)
    pub fn start_seconds(&self) -> Option<f64> {
        self.start_position
            .as_ref()
            .and_then(|p| p.time.as_ref().map(|t| t.as_seconds()))
    }

    /// Get end time in seconds (for DAW integration)
    pub fn end_seconds(&self) -> Option<f64> {
        self.end_position
            .as_ref()
            .and_then(|p| p.time.as_ref().map(|t| t.as_seconds()))
    }

    /// Check if a time position is within this section (for DAW integration)
    pub fn contains_position(&self, seconds: f64) -> bool {
        if let (Some(start), Some(end)) = (self.start_seconds(), self.end_seconds()) {
            seconds >= start && seconds < end
        } else {
            false
        }
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }

    /// Set metadata value
    pub fn set_metadata<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) {
        self.metadata.insert(key.into(), value.into());
    }

    /// Remove metadata value
    pub fn remove_metadata(&mut self, key: &str) -> Option<String> {
        self.metadata.remove(key)
    }

    // DAW integration methods (for FTS compatibility)

    /// Calculate the duration of the section in seconds (alias for duration_seconds for compatibility)
    pub fn duration(&self) -> f64 {
        self.duration_seconds().unwrap_or(0.0)
    }

    /// Check if a Position is within this section (for DAW integration)
    pub fn contains_position_exact(&self, position: Position) -> bool {
        if let Some(time) = position.time.as_ref() {
            self.contains_position(time.as_seconds())
        } else {
            false
        }
    }

    /// Check if this section overlaps with another time range
    pub fn overlaps_with_range(&self, start: f64, end: f64) -> bool {
        if let (Some(section_start), Some(section_end)) = (self.start_seconds(), self.end_seconds())
        {
            !(section_end <= start || section_start >= end)
        } else {
            false
        }
    }

    /// Check if this section overlaps with another section
    pub fn overlaps_with_section(&self, other: &Section) -> bool {
        if let (Some(start), Some(end)) = (other.start_seconds(), other.end_seconds()) {
            self.overlaps_with_range(start, end)
        } else {
            false
        }
    }

    /// Get the section's color as RGB string (bright variant)
    ///
    /// Uses semantic colors based on section type (Tailwind palette).
    /// Falls back to REAPER color if set, or the semantic color otherwise.
    pub fn color_bright(&self) -> String {
        // Use semantic color based on section type
        super::colors::colors_for_section_type(&self.section_type).bright_css()
    }

    /// Get the section's color as RGB string (muted variant)
    ///
    /// Uses semantic colors based on section type (Tailwind palette).
    pub fn color_muted(&self) -> String {
        super::colors::colors_for_section_type(&self.section_type).muted_css()
    }

    /// Get the section's semantic colors based on its type
    ///
    /// Returns `SectionColors` with bright, muted, and text variants.
    #[must_use]
    pub fn colors(&self) -> super::colors::SectionColors {
        super::colors::colors_for_section_type(&self.section_type)
    }

    /// Get the section's color (defaults to bright)
    pub fn color_rgb(&self) -> String {
        self.color_bright()
    }

    /// Calculate progress percentage (0-100) based on transport position
    pub fn progress(&self, transport_position: f64) -> f64 {
        if let (Some(start), Some(end)) = (self.start_seconds(), self.end_seconds()) {
            if end <= start {
                return 0.0;
            }

            if transport_position >= end {
                return 100.0;
            }
            if transport_position < start {
                return 0.0;
            }

            ((transport_position - start) / (end - start) * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        }
    }

    /// Calculate the length of the section in measures, given the project's time signature.
    ///
    /// Uses the musical positions from `start_position` / `end_position`. The time signature
    /// is required to convert any sub-bar beat / subdivision remainder into a fractional measure.
    /// Returns `None` if musical positions are missing.
    pub fn length_measures(&self, time_signature: &TimeSignature) -> Option<f64> {
        let (start_pos, end_pos) = (self.start_position.as_ref()?, self.end_position.as_ref()?);
        let start_musical = start_pos.musical.as_ref()?;
        let end_musical = end_pos.musical.as_ref()?;

        let beats_per_measure = time_signature.beats_per_measure();
        if beats_per_measure <= 0.0 {
            return None;
        }

        let to_measures = |m: &daw_proto::MusicalPosition| {
            m.measure as f64 + (m.beat as f64 + m.subdivision as f64 / 1000.0) / beats_per_measure
        };

        let length = to_measures(end_musical) - to_measures(start_musical);
        (length > 0.0).then_some(length)
    }

    /// Validate section data
    pub fn validate(&self) -> Result<(), String> {
        if let (Some(start), Some(end)) = (self.start_seconds(), self.end_seconds())
            && start >= end
        {
            return Err(format!(
                "Invalid time range: start ({}) >= end ({})",
                start, end
            ));
        }

        if let Some(ref name) = self.name
            && name.trim().is_empty()
        {
            return Err("Section name cannot be empty".to_string());
        }

        if let Some(start) = self.start_seconds()
            && start < 0.0
        {
            return Err("Section start time cannot be negative".to_string());
        }

        if let Some(num) = self.number
            && num == 0
        {
            return Err("Section number must be greater than 0".to_string());
        }

        Ok(())
    }

    /// Clone this section with a new time range
    pub fn with_time_range(&self, start_seconds: f64, end_seconds: f64) -> Result<Self, String> {
        if start_seconds >= end_seconds {
            return Err(format!(
                "Invalid time range: start ({}) >= end ({})",
                start_seconds, end_seconds
            ));
        }

        let start_time = TimePosition::from_seconds(start_seconds);
        let end_time = TimePosition::from_seconds(end_seconds);
        let start_pos = Position::new(None, Some(start_time), None);
        let mut new_section = self.clone();
        new_section.id = None; // New section gets new ID
        new_section.start_position = Some(start_pos);
        new_section.end_position = Some(Position::new(None, Some(end_time), None));
        Ok(new_section)
    }

    /// Clone this section with a new section type
    pub fn with_section_type(&self, section_type: SectionType) -> Self {
        let mut new_section = self.clone();
        new_section.id = None; // New section gets new ID
        new_section.section_type = section_type;
        new_section
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_section() {
        let section = Section::new(SectionType::Verse);
        assert_eq!(section.section_type, SectionType::Verse);
        assert_eq!(section.number, None);
        assert_eq!(section.split_letter, None);
        assert_eq!(section.measure_count, None);
    }

    #[test]
    fn test_with_measure_count() {
        let section = Section::new(SectionType::Chorus).with_measure_count(8);
        assert_eq!(section.measure_count, Some(8));
    }

    #[test]
    fn test_display_name_no_number() {
        let section = Section::new(SectionType::Intro);
        assert_eq!(section.display_name(), "Intro");
    }

    #[test]
    fn test_display_name_with_number() {
        let mut section = Section::new(SectionType::Verse);
        section.number = Some(1);
        assert_eq!(section.display_name(), "Verse 1");
    }

    #[test]
    fn test_display_name_with_split_letter() {
        let mut section = Section::new(SectionType::Verse);
        section.number = Some(1);
        section.split_letter = Some('a');
        assert_eq!(section.display_name(), "Verse 1a");
    }

    #[test]
    fn test_section_clone() {
        let section1 = Section::new(SectionType::Bridge).with_measure_count(4);
        let section2 = section1.clone();
        assert_eq!(section1, section2);
    }
}
