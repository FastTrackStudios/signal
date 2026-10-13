//! Semantic roles for chart elements
//!
//! Describes the contextual significance of elements in the chart,
//! used for special rendering, navigation, and playback behavior.

use facet::Facet;

/// Semantic role describing an element's contextual significance.
///
/// Elements can have multiple roles (e.g., both `FirstInMeasure` and `SectionStart`).
/// These roles affect rendering decisions, playback behavior, and user interaction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Facet)]
#[repr(u8)]
pub enum SemanticRole {
    /// First element in a system (affects spacing, may show clef/time sig).
    FirstInSystem,

    /// Last element in a system (affects barline type, line break decision).
    LastInSystem,

    /// First element in a measure (may have special spacing).
    FirstInMeasure,

    /// Last element in a measure (precedes barline).
    LastInMeasure,

    /// First chord/element after a section change.
    /// The section_type contains the name of the new section (e.g., "Verse", "Chorus").
    SectionStart { section_type: String },

    /// Element belongs to a repeat structure (first/second ending, D.S., etc.).
    RepeatPass {
        /// Which pass (1 = first ending, 2 = second ending, etc.)
        pass: u8,
        /// Total number of passes in this repeat structure
        total_passes: u8,
    },

    /// Element is a navigation target (D.S., D.C., Coda jump destination).
    NavigationTarget {
        /// Type of navigation marker
        nav_type: NavigationType,
    },

    /// Element is part of a pickup/anacrusis measure.
    /// Pickup measures don't count in measure numbering.
    Pickup,

    /// Element is explicitly tacet (rest with specific duration, not just empty space).
    Tacet,

    /// Element was generated from template expansion (section recall).
    /// The original definition is preserved in `template_span` of SourceLink.
    FromTemplate {
        /// Original section type that defined this content (e.g., "Verse")
        template_section: String,
    },

    /// Element marks a key change.
    KeyChange {
        /// The new key after the change
        new_key: String,
    },

    /// Element marks a time signature change.
    TimeSignatureChange {
        /// New numerator
        numerator: u8,
        /// New denominator
        denominator: u8,
    },

    /// Element marks a tempo change.
    TempoChange {
        /// New tempo in BPM
        bpm: u16,
    },

    /// Custom role for extensibility.
    Custom {
        /// Role name
        name: String,
        /// Optional value
        value: Option<String>,
    },
}

impl SemanticRole {
    /// Check if this is a boundary role (first/last in system/measure).
    #[must_use]
    pub const fn is_boundary(&self) -> bool {
        matches!(
            self,
            SemanticRole::FirstInSystem
                | SemanticRole::LastInSystem
                | SemanticRole::FirstInMeasure
                | SemanticRole::LastInMeasure
        )
    }

    /// Check if this is a navigation-related role.
    #[must_use]
    pub const fn is_navigation(&self) -> bool {
        matches!(
            self,
            SemanticRole::NavigationTarget { .. } | SemanticRole::RepeatPass { .. }
        )
    }

    /// Check if this is a structural change (key, time, tempo).
    #[must_use]
    pub const fn is_structural_change(&self) -> bool {
        matches!(
            self,
            SemanticRole::KeyChange { .. }
                | SemanticRole::TimeSignatureChange { .. }
                | SemanticRole::TempoChange { .. }
        )
    }
}

/// Types of navigation markers in music notation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Facet)]
#[repr(u8)]
pub enum NavigationType {
    /// Dal Segno - go back to the segno sign
    DalSegno,
    /// Da Capo - go back to the beginning
    DaCapo,
    /// To Coda - jump to the coda section
    ToCoda,
    /// Coda section marker
    Coda,
    /// Fine - end point for D.C./D.S. al Fine
    Fine,
    /// Segno sign (target for D.S.)
    Segno,
}

impl NavigationType {
    /// Get the display symbol for this navigation type.
    #[must_use]
    pub const fn symbol(&self) -> &'static str {
        match self {
            NavigationType::DalSegno => "D.S.",
            NavigationType::DaCapo => "D.C.",
            NavigationType::ToCoda => "To Coda",
            NavigationType::Coda => "Coda",
            NavigationType::Fine => "Fine",
            NavigationType::Segno => "Segno",
        }
    }

    /// Check if this is a jump instruction (D.S., D.C., To Coda).
    #[must_use]
    pub const fn is_jump(&self) -> bool {
        matches!(
            self,
            NavigationType::DalSegno | NavigationType::DaCapo | NavigationType::ToCoda
        )
    }

    /// Check if this is a target marker (Segno, Coda, Fine).
    #[must_use]
    pub const fn is_target(&self) -> bool {
        matches!(
            self,
            NavigationType::Segno | NavigationType::Coda | NavigationType::Fine
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boundary_roles() {
        assert!(SemanticRole::FirstInSystem.is_boundary());
        assert!(SemanticRole::LastInMeasure.is_boundary());
        assert!(
            !SemanticRole::SectionStart {
                section_type: "Verse".to_string()
            }
            .is_boundary()
        );
    }

    #[test]
    fn test_navigation_roles() {
        let repeat = SemanticRole::RepeatPass {
            pass: 1,
            total_passes: 2,
        };
        let nav = SemanticRole::NavigationTarget {
            nav_type: NavigationType::DalSegno,
        };

        assert!(repeat.is_navigation());
        assert!(nav.is_navigation());
        assert!(!SemanticRole::FirstInSystem.is_navigation());
    }

    #[test]
    fn test_navigation_type_symbols() {
        assert_eq!(NavigationType::DalSegno.symbol(), "D.S.");
        assert_eq!(NavigationType::DaCapo.symbol(), "D.C.");
        assert_eq!(NavigationType::Coda.symbol(), "Coda");
    }

    #[test]
    fn test_navigation_type_classification() {
        assert!(NavigationType::DalSegno.is_jump());
        assert!(NavigationType::ToCoda.is_jump());
        assert!(!NavigationType::Segno.is_jump());

        assert!(NavigationType::Segno.is_target());
        assert!(NavigationType::Fine.is_target());
        assert!(!NavigationType::DaCapo.is_target());
    }

    #[test]
    fn test_structural_change() {
        let key_change = SemanticRole::KeyChange {
            new_key: "G".to_string(),
        };
        let time_change = SemanticRole::TimeSignatureChange {
            numerator: 3,
            denominator: 4,
        };
        let tempo_change = SemanticRole::TempoChange { bpm: 120 };

        assert!(key_change.is_structural_change());
        assert!(time_change.is_structural_change());
        assert!(tempo_change.is_structural_change());
        assert!(!SemanticRole::FirstInSystem.is_structural_change());
    }
}
