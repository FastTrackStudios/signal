//! Chord symbol normalization.
//!
//! This module contains the normalization implementation for [`Chord`],
//! which generates the canonical string representation of a chord symbol.

use super::definition::Chord;
use super::degree::ChordDegree;
use super::extensions::ExtensionQuality;
use super::family::ChordFamily;
use super::quality::ChordQuality;

// ============================================================================
// Normalization
// ============================================================================

impl Chord {
    /// Normalize the chord to its canonical string representation.
    ///
    /// This method generates the `descriptor` (everything after the root) and
    /// `normalized` (full chord symbol) fields based on the chord's components.
    ///
    /// The normalization follows standard chord symbol conventions:
    /// - Quality comes first (unless subsumed by family)
    /// - Family/extensions next (7, maj7, 9, 13, etc.)
    /// - Alterations (b5, #9, etc.)
    /// - Additions (add9, 6, 6/9)
    /// - Omissions (no3, no5)
    /// - Bass note for slash chords (/G)
    pub fn normalize(&mut self) {
        let mut desc = String::new();

        let is_sixth_chord = self.additions.contains(&ChordDegree::Sixth) && self.family.is_none();
        let is_suspended = matches!(self.quality, ChordQuality::Suspended(_));
        let is_suspended_with_seventh = is_suspended && self.family.is_some();

        // Check if the family already includes the quality in its symbol
        // FullyDiminished.symbol() = "dim7", so we shouldn't output "dim" first
        // HalfDiminished.symbol() = "m7b5", so we shouldn't output "dim" first either
        let family_includes_quality = matches!(
            self.family,
            Some(ChordFamily::FullyDiminished) | Some(ChordFamily::HalfDiminished)
        );

        // Check if we have extensions or a family that makes Power quality redundant
        // "C59" is not standard notation - it should just be "C9" (Power implied when no 3rd)
        // B5/C should display as B/C — power implied by voicing when there's a bass note
        let power_is_redundant = self.quality == ChordQuality::Power
            && (self.family.is_some() || self.extensions.has_any() || self.bass.is_some());

        // For suspended chords with seventh (7sus4, 9sus4), defer quality until after family
        if is_suspended_with_seventh {
            // Quality will be written after family/extensions
        } else if family_includes_quality {
            // Skip quality output - the family symbol already includes it
        } else if power_is_redundant {
            // Skip "5" when we have a 7th or extensions - it's implied
        } else if is_sixth_chord {
            // For sixth chords:
            // - Major/Power: don't output quality (just the 6)
            // - Minor: output "m" (for Fm6, etc.)
            // - Others: output the quality
            match self.quality {
                ChordQuality::Major | ChordQuality::Power => {}
                ChordQuality::Minor => desc.push('m'),
                _ => desc.push_str(&self.quality.to_string()),
            }
        } else {
            desc.push_str(&self.quality.to_string());
        }

        if let Some(ref family) = self.family {
            if matches!(family, ChordFamily::Major7 | ChordFamily::MinorMajor7)
                && self.extensions.has_any()
            {
                desc.push_str("maj");
                if self.extensions.thirteenth.is_some() {
                    desc.push_str("13");
                } else if self.extensions.eleventh.is_some() {
                    desc.push_str("11");
                } else if self.extensions.ninth.is_some() {
                    desc.push('9');
                }
                if let Some(qual) = self.extensions.ninth
                    && qual != ExtensionQuality::Natural
                {
                    match qual {
                        ExtensionQuality::Flat => desc.push_str("b9"),
                        ExtensionQuality::Sharp => desc.push_str("#9"),
                        _ => {}
                    }
                }
                if let Some(qual) = self.extensions.eleventh
                    && qual != ExtensionQuality::Natural
                {
                    match qual {
                        ExtensionQuality::Flat => desc.push_str("b11"),
                        ExtensionQuality::Sharp => desc.push_str("#11"),
                        _ => {}
                    }
                }
                if let Some(qual) = self.extensions.thirteenth
                    && qual != ExtensionQuality::Natural
                {
                    match qual {
                        ExtensionQuality::Flat => desc.push_str("b13"),
                        ExtensionQuality::Sharp => desc.push_str("#13"),
                        _ => {}
                    }
                }
            } else {
                // Non-major family or no extensions
                // The highest natural extension masks the seventh
                // If all extensions are altered, the seventh must be shown explicitly
                let should_show_seventh =
                    !self.extensions.has_any() || !self.extensions.has_natural();
                if should_show_seventh {
                    // MinorMajor7 uses slash notation: Cm/maj7 instead of CmMaj7
                    if matches!(family, ChordFamily::MinorMajor7) {
                        desc.push_str("/maj7");
                    } else {
                        desc.push_str(&family.to_string());
                    }
                }
                // Extensions
                if self.extensions.has_any() {
                    desc.push_str(&self.extensions.to_string());
                }
            }
        } else {
            // No family, just show extensions
            if self.extensions.has_any() {
                desc.push_str(&self.extensions.to_string());
            }
        }

        // For suspended chords with seventh, write the quality AFTER the family/extensions
        // This produces "7sus4" instead of "sus47"
        if is_suspended_with_seventh {
            desc.push_str(&self.quality.to_string());
        }

        // Alterations
        for alteration in &self.alterations {
            desc.push_str(&alteration.to_string());
        }

        // Additions (with special handling for 6 and 6/9), spelled as
        // written: an added 2nd is add2 (`42`, `G2`) and an added 4th add4
        // (`5(add4)`), the way worship charts write them — not the
        // respelled add9 / add11.
        let is_sixth_chord = self.additions.contains(&ChordDegree::Sixth) && self.family.is_none();
        let is_six_nine_chord = is_sixth_chord && self.additions.contains(&ChordDegree::Ninth);

        let addition_value = |degree: &ChordDegree| -> u8 { degree.value() };

        if is_six_nine_chord {
            desc.push_str("6/9");
            // Add any other additions
            for addition in &self.additions {
                // 13 is enharmonically the same scale degree as 6 in 6/9 chords,
                // so avoid impossible forms like "6/9add13".
                if *addition != ChordDegree::Sixth
                    && *addition != ChordDegree::Ninth
                    && *addition != ChordDegree::Thirteenth
                {
                    desc.push_str(&format!("add{}", addition_value(addition)));
                }
            }
        } else {
            for addition in &self.additions {
                if *addition == ChordDegree::Sixth && is_sixth_chord {
                    desc.push('6');
                } else {
                    desc.push_str(&format!("add{}", addition_value(addition)));
                }
            }
        }

        // Omissions
        for omission in &self.omissions {
            desc.push_str(&format!("no{}", omission));
        }

        // Bass note
        if let Some(ref bass) = self.bass {
            desc.push_str(&format!("/{}", bass));
        }

        self.descriptor = desc.clone();
        self.normalized = format!("{}{}", self.root, desc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::quality::ChordQuality;
    use crate::primitives::{MusicalNote, RootNotation};

    #[test]
    fn test_normalize_major_triad() {
        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::new(root, ChordQuality::Major);
        chord.normalize();

        assert_eq!(chord.normalized, "C");
        assert_eq!(chord.descriptor, "");
    }

    #[test]
    fn test_normalize_minor_triad() {
        let root = RootNotation::from_note_name(MusicalNote::a());
        let mut chord = Chord::new(root, ChordQuality::Minor);
        chord.normalize();

        assert_eq!(chord.normalized, "Am");
        assert_eq!(chord.descriptor, "m");
    }

    #[test]
    fn test_normalize_dominant_seventh() {
        let root = RootNotation::from_note_name(MusicalNote::g());
        let mut chord = Chord::new(root, ChordQuality::Major);
        chord.family = Some(ChordFamily::Dominant7);
        chord.normalize();

        assert_eq!(chord.normalized, "G7");
        assert_eq!(chord.descriptor, "7");
    }

    #[test]
    fn test_normalize_minor_seventh() {
        let root = RootNotation::from_note_name(MusicalNote::d());
        let mut chord = Chord::new(root, ChordQuality::Minor);
        chord.family = Some(ChordFamily::Minor7);
        chord.normalize();

        assert_eq!(chord.normalized, "Dm7");
        assert_eq!(chord.descriptor, "m7");
    }

    #[test]
    fn test_normalize_sixth_chord() {
        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::new(root, ChordQuality::Major);
        chord.additions.push(ChordDegree::Sixth);
        chord.normalize();

        assert_eq!(chord.normalized, "C6");
        assert_eq!(chord.descriptor, "6");
    }

    #[test]
    fn test_normalize_six_nine_chord() {
        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::new(root, ChordQuality::Major);
        chord.additions.push(ChordDegree::Sixth);
        chord.additions.push(ChordDegree::Ninth);
        chord.normalize();

        assert_eq!(chord.normalized, "C6/9");
        assert_eq!(chord.descriptor, "6/9");
    }

    #[test]
    fn test_normalize_minor_sixth() {
        let root = RootNotation::from_note_name(MusicalNote::a());
        let mut chord = Chord::new(root, ChordQuality::Minor);
        chord.additions.push(ChordDegree::Sixth);
        chord.normalize();

        assert_eq!(chord.normalized, "Am6");
        assert_eq!(chord.descriptor, "m6");
    }
}
