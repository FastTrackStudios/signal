//! Section Numbering
//!
//! Automatic numbering for repeated sections

use super::{Section, SectionType};
use std::collections::HashMap;

/// Manages auto-numbering of sections
pub struct SectionNumberer {
    counters: HashMap<String, u32>,
    last_section: Option<(String, u32, Option<char>)>, // (type, number, split_letter)
    pending_split: Option<(String, u32)>, // Track if we need to retroactively add 'a' to previous section
}

impl SectionNumberer {
    pub fn new() -> Self {
        Self {
            counters: HashMap::new(),
            last_section: None,
            pending_split: None,
        }
    }

    /// Process a section and assign appropriate numbering
    ///
    /// Returns a tuple: (Section, Option`<Section>`) where the second element is
    /// a retroactively updated previous section (if needed for split letter assignment)
    pub fn process_section(
        &mut self,
        section_type: SectionType,
        _measure_count: Option<usize>,
    ) -> Section {
        let mut section = Section::new(section_type.clone());

        // Only number sections that should be numbered
        if !section_type.should_number() {
            self.last_section = None; // Reset consecutive tracking
            self.pending_split = None;
            return section;
        }

        let key = section_type.full_name();

        // Check if this is a consecutive repeat of the same section type with same number
        let (is_consecutive, last_split) =
            if let Some((last_key, last_num, last_split)) = &self.last_section {
                if last_key == &key {
                    let current_count = self.counters.get(&key).copied().unwrap_or(0);
                    (current_count == *last_num, *last_split)
                } else {
                    (false, None)
                }
            } else {
                (false, None)
            };

        if is_consecutive {
            // Consecutive section with same number - assign split letters
            let current_number = self.counters.get(&key).copied().unwrap_or(1);

            let split_letter = match last_split {
                None => {
                    // This is the second occurrence - mark that we need to update previous to 'a'
                    self.pending_split = Some((key.clone(), current_number));
                    Some('b') // This one gets 'b'
                }
                Some(c) => Some((c as u8 + 1) as char), // Next split (c, d, ...)
            };

            section.number = Some(current_number);
            section.split_letter = split_letter;

            self.last_section = Some((key, current_number, split_letter));
        } else {
            // Not consecutive - increment counter and assign new number
            let count = self.counters.entry(key.clone()).or_insert(0);
            *count += 1;
            section.number = Some(*count);
            section.split_letter = None;

            self.last_section = Some((key, *count, None));
            self.pending_split = None;
        }

        section
    }

    /// Check if there's a pending retroactive split letter update
    pub fn has_pending_split(&self) -> bool {
        self.pending_split.is_some()
    }

    /// Auto-number a list of sections
    pub fn number_sections(&mut self, sections: &mut [Section]) {
        for i in 0..sections.len() {
            let numbered =
                self.process_section(sections[i].section_type.clone(), sections[i].measure_count);
            sections[i].number = numbered.number;
            sections[i].split_letter = numbered.split_letter;

            // If we just detected a consecutive section, retroactively add 'a' to the previous one
            if self.has_pending_split() && i > 0 {
                sections[i - 1].split_letter = Some('a');
                self.pending_split = None; // Clear the pending flag
            }
        }

        // Post-pass: suppress numbers for section types that only ever appear
        // as a single "generation" (i.e. every occurrence of this type in the
        // chart is consecutive, so they all share number 1). The number only
        // exists to distinguish separate generations, so with just one
        // generation it's noise — "BR", "BR A", "BR B" reads better than
        // "BR 1", "BR 1 A", "BR 1 B". Once a second, non-consecutive
        // generation appears (number reaches 2+), every occurrence of that
        // type keeps its number, split letters included, so the reader can
        // tell which generation a given repeat belongs to.
        let mut max_numbers: HashMap<String, u32> = HashMap::new();
        for section in sections.iter() {
            if let Some(n) = section.number {
                let key = section.section_type.full_name();
                let entry = max_numbers.entry(key).or_insert(0);
                *entry = (*entry).max(n);
            }
        }
        for section in sections.iter_mut() {
            if let Some(n) = section.number {
                let key = section.section_type.full_name();
                if max_numbers.get(&key) == Some(&1) && n == 1 {
                    section.number = None;
                }
            }
        }
    }
}

impl Default for SectionNumberer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numbering_verses() {
        let mut numberer = SectionNumberer::new();

        // First verse
        let v1 = numberer.process_section(SectionType::Verse, None);
        assert_eq!(v1.number, Some(1));
        assert_eq!(v1.split_letter, None);

        // Some other section type breaks the consecutive chain
        let _ch1 = numberer.process_section(SectionType::Chorus, None);

        // Second verse (not consecutive) gets number 2
        let v2 = numberer.process_section(SectionType::Verse, None);
        assert_eq!(v2.number, Some(2));
        assert_eq!(v2.split_letter, None);
    }

    #[test]
    fn test_split_letters() {
        let mut numberer = SectionNumberer::new();

        // First verse - no split yet
        let v1 = numberer.process_section(SectionType::Verse, None);
        assert_eq!(v1.number, Some(1));
        assert_eq!(v1.split_letter, None);

        // Second verse (consecutive) - gets 'b', and we should retroactively add 'a' to first
        let v1b = numberer.process_section(SectionType::Verse, None);
        assert_eq!(v1b.number, Some(1));
        assert_eq!(v1b.split_letter, Some('b'));
        assert!(numberer.has_pending_split()); // Flag that previous needs 'a'

        // Third consecutive verse - gets 'c'
        let v1c = numberer.process_section(SectionType::Verse, None);
        assert_eq!(v1c.number, Some(1));
        assert_eq!(v1c.split_letter, Some('c'));
    }

    #[test]
    fn test_non_numbered_sections() {
        let mut numberer = SectionNumberer::new();

        let intro = numberer.process_section(SectionType::Intro, None);
        assert_eq!(intro.number, None);
        assert_eq!(intro.split_letter, None);

        let outro = numberer.process_section(SectionType::Outro, None);
        assert_eq!(outro.number, None);
        assert_eq!(outro.split_letter, None);
    }

    #[test]
    fn test_mixed_sections() {
        let mut numberer = SectionNumberer::new();

        let v1 = numberer.process_section(SectionType::Verse, None);
        assert_eq!(v1.number, Some(1));

        let ch1 = numberer.process_section(SectionType::Chorus, None);
        assert_eq!(ch1.number, Some(1));

        let v2 = numberer.process_section(SectionType::Verse, None);
        assert_eq!(v2.number, Some(2));

        let ch2 = numberer.process_section(SectionType::Chorus, None);
        assert_eq!(ch2.number, Some(2));
    }

    #[test]
    fn test_number_sections_batch() {
        let mut numberer = SectionNumberer::new();

        let mut sections = vec![
            Section::new(SectionType::Verse),
            Section::new(SectionType::Chorus),
            Section::new(SectionType::Verse),
            Section::new(SectionType::Chorus),
        ];

        numberer.number_sections(&mut sections);

        assert_eq!(sections[0].number, Some(1));
        assert_eq!(sections[1].number, Some(1));
        assert_eq!(sections[2].number, Some(2));
        assert_eq!(sections[3].number, Some(2));
    }

    #[test]
    fn test_batch_retroactive_split_letters() {
        let mut numberer = SectionNumberer::new();

        let mut sections = vec![
            Section::new(SectionType::Verse),  // Should become Verse 1a
            Section::new(SectionType::Verse),  // Should be Verse 1b
            Section::new(SectionType::Chorus), // Chorus 1
            Section::new(SectionType::Verse),  // Verse 2
        ];

        numberer.number_sections(&mut sections);

        // First verse should retroactively get 'a'
        assert_eq!(sections[0].number, Some(1));
        assert_eq!(sections[0].split_letter, Some('a'));

        // Second verse should get 'b'
        assert_eq!(sections[1].number, Some(1));
        assert_eq!(sections[1].split_letter, Some('b'));

        // Chorus breaks the chain - only one chorus, so number is suppressed
        assert_eq!(sections[2].number, None);
        assert_eq!(sections[2].split_letter, None);

        // Third verse is not consecutive, so it's Verse 2
        assert_eq!(sections[3].number, Some(2));
        assert_eq!(sections[3].split_letter, None);
    }

    #[test]
    fn test_single_instance_number_suppressed() {
        let mut numberer = SectionNumberer::new();

        let mut sections = vec![
            Section::new(SectionType::Intro),
            Section::new(SectionType::Verse),
            Section::new(SectionType::Chorus),
            Section::new(SectionType::Outro),
        ];

        numberer.number_sections(&mut sections);

        // Only one verse and one chorus — numbers should be suppressed
        assert_eq!(sections[0].number, None); // Intro never numbered
        assert_eq!(sections[1].number, None); // Single verse — suppressed
        assert_eq!(sections[2].number, None); // Single chorus — suppressed
        assert_eq!(sections[3].number, None); // Outro never numbered
    }

    #[test]
    fn test_multiple_instances_still_numbered() {
        let mut numberer = SectionNumberer::new();

        let mut sections = vec![
            Section::new(SectionType::Verse),
            Section::new(SectionType::Chorus),
            Section::new(SectionType::Verse),
            Section::new(SectionType::Chorus),
        ];

        numberer.number_sections(&mut sections);

        // Two verses and two choruses — numbers should remain
        assert_eq!(sections[0].number, Some(1));
        assert_eq!(sections[1].number, Some(1));
        assert_eq!(sections[2].number, Some(2));
        assert_eq!(sections[3].number, Some(2));
    }

    #[test]
    fn test_single_generation_with_splits_suppresses_number() {
        // BR / BR / BR / BR — one generation of four consecutive bridges.
        // The number exists only to distinguish separate generations, so
        // with a single generation it should be suppressed even though each
        // repeat gets a split letter — "BR", "BR" (a), "BR" (b), "BR" (c)
        // rather than "BR 1", "BR 1a", "BR 1b", "BR 1c". (The letters
        // themselves are assigned uppercase by
        // `engraver_proto::compute_section_letters` at render time, not by
        // this module — this test only covers the number.)
        let mut numberer = SectionNumberer::new();
        let mut sections = vec![
            Section::new(SectionType::Bridge),
            Section::new(SectionType::Bridge),
            Section::new(SectionType::Bridge),
            Section::new(SectionType::Bridge),
        ];

        numberer.number_sections(&mut sections);

        assert_eq!(sections[0].number, None);
        assert_eq!(sections[0].split_letter, Some('a'));
        assert_eq!(sections[1].number, None);
        assert_eq!(sections[1].split_letter, Some('b'));
        assert_eq!(sections[2].number, None);
        assert_eq!(sections[2].split_letter, Some('c'));
        assert_eq!(sections[3].number, None);
        assert_eq!(sections[3].split_letter, Some('d'));
    }

    #[test]
    fn test_second_generation_keeps_number_on_both() {
        // BR / BR / BR / BR / CH / BR — a second, later generation of the
        // same type means there's now something for the number to
        // disambiguate, so every bridge (including the first generation's
        // splits) keeps its number: "BR 1" (a), "BR 1" (b), "BR 1" (c),
        // "BR 1" (d), then "BR 2" for the lone bridge after the chorus.
        let mut numberer = SectionNumberer::new();
        let mut sections = vec![
            Section::new(SectionType::Bridge),
            Section::new(SectionType::Bridge),
            Section::new(SectionType::Bridge),
            Section::new(SectionType::Bridge),
            Section::new(SectionType::Chorus),
            Section::new(SectionType::Bridge),
        ];

        numberer.number_sections(&mut sections);

        assert_eq!(sections[0].number, Some(1));
        assert_eq!(sections[0].split_letter, Some('a'));
        assert_eq!(sections[1].number, Some(1));
        assert_eq!(sections[1].split_letter, Some('b'));
        assert_eq!(sections[2].number, Some(1));
        assert_eq!(sections[2].split_letter, Some('c'));
        assert_eq!(sections[3].number, Some(1));
        assert_eq!(sections[3].split_letter, Some('d'));

        // Chorus: single instance, suppressed.
        assert_eq!(sections[4].number, None);

        // Second generation, alone, no split.
        assert_eq!(sections[5].number, Some(2));
        assert_eq!(sections[5].split_letter, None);
    }
}
