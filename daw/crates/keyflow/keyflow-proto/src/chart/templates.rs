//! Section Template System
//!
//! Manages reusable section progressions (chord templates)

use super::types::Measure;
use crate::key::Key;
use crate::primitives::{MusicalNote, Note};
use crate::sections::SectionType;
use facet::Facet;
use std::collections::HashMap;

/// Manages section templates for chord progression reuse
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct TemplateManager {
    /// Stored templates (section type -> measures)
    templates: HashMap<String, Vec<Measure>>,

    /// Original keys when templates were created (for transposition)
    original_keys: HashMap<String, Key>,
}

impl TemplateManager {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
            original_keys: HashMap::new(),
        }
    }

    /// Store a template for a section type
    pub fn store_template(
        &mut self,
        section_type: &SectionType,
        measures: Vec<Measure>,
        key: Option<Key>,
    ) {
        let key_str = section_type.full_name();
        self.templates.insert(key_str.clone(), measures);

        if let Some(key) = key {
            self.original_keys.insert(key_str, key);
        }
    }

    /// Alias for store_template - store a template for a section type
    pub fn store(&mut self, section_type: &SectionType, measures: &[Measure], key: Option<&Key>) {
        self.store_template(section_type, measures.to_vec(), key.cloned());
    }

    /// Recall a template for a section type
    pub fn recall_template(&self, section_type: &SectionType) -> Option<&Vec<Measure>> {
        let key_str = section_type.full_name();
        self.templates.get(&key_str)
    }

    /// Check if a template exists for a section type
    pub fn has_template(&self, section_type: &SectionType) -> bool {
        let key_str = section_type.full_name();
        self.templates.contains_key(&key_str)
    }

    /// Get the original key for a template
    pub fn original_key(&self, section_type: &SectionType) -> Option<&Key> {
        let key_str = section_type.full_name();
        self.original_keys.get(&key_str)
    }

    /// Clear all templates
    pub fn clear(&mut self) {
        self.templates.clear();
        self.original_keys.clear();
    }

    /// Clear a specific template
    pub fn clear_template(&mut self, section_type: &SectionType) {
        let key_str = section_type.full_name();
        self.templates.remove(&key_str);
        self.original_keys.remove(&key_str);
    }

    /// Recall a template and transpose it to a new key if needed
    ///
    /// Returns a cloned and potentially transposed version of the template measures
    pub fn recall_transposed(
        &self,
        section_type: &SectionType,
        target_key: Option<&Key>,
    ) -> Option<Vec<Measure>> {
        let template = self.recall_template(section_type)?;
        let original_key = self.original_key(section_type);

        // If no target key or no original key, return as-is
        let (from_key, to_key) = match (original_key, target_key) {
            (Some(from), Some(to)) if from != to => (from, to),
            _ => return Some(template.clone()),
        };

        // Calculate transposition interval
        let interval = Self::calculate_transposition_interval(from_key, to_key);

        // Transpose each measure
        Some(
            template
                .iter()
                .map(|measure| Self::transpose_measure(measure, interval))
                .collect(),
        )
    }

    /// Calculate the interval between two keys
    fn calculate_transposition_interval(from_key: &Key, to_key: &Key) -> i8 {
        let from_semitone = from_key.root.semitone();
        let to_semitone = to_key.root.semitone();

        // Calculate the shortest distance (considering octave wrapping)
        let diff = to_semitone as i8 - from_semitone as i8;

        // Normalize to -6 to +6 range
        if diff > 6 {
            diff - 12
        } else if diff < -6 {
            diff + 12
        } else {
            diff
        }
    }

    /// Transpose a measure by a given interval
    fn transpose_measure(measure: &Measure, interval: i8) -> Measure {
        let mut transposed = measure.clone();

        // Transpose all chords in the measure
        for chord in &mut transposed.chords {
            // Only transpose absolute note names, not scale degrees or roman numerals
            if let Some(resolved_note) = chord.root.resolved_note() {
                // Transpose the root note
                let new_note = Self::transpose_note(resolved_note, interval);

                // Update the root notation to use the new note
                // (This assumes we want to preserve it as a note name after transposition)
                chord.root = crate::primitives::RootNotation::from_note_name(new_note.clone());

                // Update full_symbol to reflect transposed root
                // (This is a simplified approach - a full implementation would reparse)
                chord.full_symbol = format!(
                    "{}{}",
                    new_note.name(),
                    chord
                        .full_symbol
                        .chars()
                        .skip_while(|c| c.is_alphabetic() || *c == '#' || *c == 'b')
                        .collect::<String>()
                );
            }
        }

        transposed
    }

    /// Transpose a musical note by a given number of semitones
    fn transpose_note(note: &MusicalNote, semitones: i8) -> MusicalNote {
        let current_semitone = note.semitone() as i8;
        let new_semitone = (current_semitone + semitones + 12) % 12;
        // Default to preferring sharps for transposition
        MusicalNote::from_semitone(new_semitone as u8, true)
    }
}

impl Default for TemplateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_and_recall_template() {
        let mut manager = TemplateManager::new();
        let verse = SectionType::Verse;

        let measures = vec![Measure::new(), Measure::new()];
        manager.store_template(&verse, measures, None);

        assert!(manager.has_template(&verse));
        assert_eq!(manager.recall_template(&verse).map(|m| m.len()), Some(2));
    }

    #[test]
    fn test_template_per_section_type() {
        let mut manager = TemplateManager::new();
        let verse = SectionType::Verse;
        let chorus = SectionType::Chorus;

        manager.store_template(&verse, vec![Measure::new(); 4], None);
        manager.store_template(&chorus, vec![Measure::new(); 8], None);

        assert_eq!(manager.recall_template(&verse).map(|m| m.len()), Some(4));
        assert_eq!(manager.recall_template(&chorus).map(|m| m.len()), Some(8));
    }

    #[test]
    fn test_transposition_interval_calculation() {
        use crate::primitives::MusicalNote;

        let c_major = Key::major(MusicalNote::c());
        let g_major = Key::major(MusicalNote::g());
        let f_major = Key::major(MusicalNote::f());

        // C to G: shortest path is down 5 semitones (-5) rather than up 7
        assert_eq!(
            TemplateManager::calculate_transposition_interval(&c_major, &g_major),
            -5
        );

        // C to F: up a perfect 4th (5 semitones)
        assert_eq!(
            TemplateManager::calculate_transposition_interval(&c_major, &f_major),
            5
        );

        // G to C: shortest path is down 5 semitones
        assert_eq!(
            TemplateManager::calculate_transposition_interval(&g_major, &c_major),
            5
        );
    }

    #[test]
    fn test_transpose_note() {
        use crate::primitives::{MusicalNote, Note};

        let c = MusicalNote::c();

        // Transpose up 2 semitones (C to D)
        let d = TemplateManager::transpose_note(&c, 2);
        assert_eq!(d.semitone(), 2);

        // Transpose up 7 semitones (C to G)
        let g = TemplateManager::transpose_note(&c, 7);
        assert_eq!(g.semitone(), 7);

        // Transpose down 2 semitones (wraps around: C to Bb)
        let bb = TemplateManager::transpose_note(&c, -2);
        assert_eq!(bb.semitone(), 10);
    }

    #[test]
    fn test_recall_transposed_same_key() {
        use crate::primitives::MusicalNote;

        let mut manager = TemplateManager::new();
        let verse = SectionType::Verse;
        let c_major = Key::major(MusicalNote::c());

        let measures = vec![Measure::new(), Measure::new()];
        manager.store_template(&verse, measures, Some(c_major.clone()));

        // Recall in same key - should not transpose
        let recalled = manager.recall_transposed(&verse, Some(&c_major));
        assert!(recalled.is_some());
        assert_eq!(recalled.unwrap().len(), 2);
    }

    #[test]
    fn test_recall_transposed_different_key() {
        use crate::primitives::MusicalNote;

        let mut manager = TemplateManager::new();
        let verse = SectionType::Verse;
        let c_major = Key::major(MusicalNote::c());
        let g_major = Key::major(MusicalNote::g());

        let measures = vec![Measure::new(), Measure::new()];
        manager.store_template(&verse, measures, Some(c_major));

        // Recall in different key - should transpose
        let recalled = manager.recall_transposed(&verse, Some(&g_major));
        assert!(recalled.is_some());
        assert_eq!(recalled.unwrap().len(), 2);
        // Note: actual chord transposition would be tested with real chord data
    }
}
