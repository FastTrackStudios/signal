//! Chord Memory System
//!
//! Manages global and section-scoped chord memory for chord quality recall.
//!
//! ## Section-Scoped Memory Architecture
//!
//! Memory is organized in two tiers:
//! 1. **Global memory** - populated from:
//!    - Explicit metadata assignments (e.g., `Cm = Cm7b5`)
//!    - First section chord definitions
//! 2. **Section memory** - cleared at the start of each new section
//!
//! Lookup order: Section-local → Global → Key inference
//!
//! ## Split Memory for Major/Minor Families
//!
//! Memory is split between "major family" and "minor family" chords:
//! - Major family: C, Cmaj7, C7, C9, Caug, etc. (no "m" modifier)
//! - Minor family: Cm, Cm7, Cm9, etc. (has "m" modifier)
//!
//! **Basic chords** (just triads like `C` or `Cm`) can RECALL from memory
//! but don't STORE to memory. This means:
//! - `C` after `Cmaj7` → recalls `Cmaj7` (from major family)
//! - `Cm` after `Cm7` → recalls `Cm7` (from minor family)
//! - `C` alone → stays `C` (no memory to recall from)

use crate::sections::SectionType;
use facet::Facet;
use std::collections::HashMap;

/// Chord family for split memory
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChordFamily {
    /// Major family: C, Cmaj7, C7, C9, Caug, Csus, etc.
    Major,
    /// Minor family: Cm, Cm7, Cm9, Cdim, etc.
    Minor,
}

/// Manages chord memory for quality recall
///
/// Memory hierarchy:
/// 1. One-time overrides (prefix `!`) - highest priority (bypasses ALL memory)
/// 2. Section-local memory - middle priority
/// 3. Global memory - fallback (from metadata + first section)
/// 4. Default qualities from key - lowest priority
///
/// Section lifecycle:
/// - `enter_section()` - clears section memory for non-first sections
/// - `complete_first_section()` - copies first section memory to global
/// - First section definitions automatically populate global memory
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct ChordMemory {
    /// Global chord memory using family-aware keys (e.g., "c:major" -> "Cmaj7")
    /// Populated from metadata assignments and first section definitions
    global_family: HashMap<String, String>,

    /// Section-local chord memory using family-aware keys
    /// Cleared on each new section (except first section)
    section_family: HashMap<String, String>,

    /// Legacy global memory (root -> full symbol) - kept for compatibility
    global: HashMap<String, String>,

    /// Legacy section-specific memory - kept for compatibility
    section_specific: HashMap<String, HashMap<String, String>>,

    /// Roots that have been used at least once in any section.
    seen_roots: std::collections::HashSet<String>,

    /// Whether we're currently in the first section
    is_first_section: bool,

    /// Whether the first section has been completed
    first_section_complete: bool,

    /// Number of sections that have been entered
    section_count: usize,

    /// Whether bare roots recall what their degree last was. On for the
    /// engine by itself; a chart turns it off unless it asks for it
    /// (`/CHORD_MEMORY=true`, see [`crate::chart::settings::ChartSetting::ChordMemory`]).
    enabled: bool,

    /// Explicit metadata assignments (`Cm = Cm7b5`), by family key. They
    /// are written on purpose, so they hold with memory off too.
    assignments: HashMap<String, String>,
}

impl ChordMemory {
    pub fn new() -> Self {
        Self {
            global_family: HashMap::new(),
            section_family: HashMap::new(),
            global: HashMap::new(),
            section_specific: HashMap::new(),
            seen_roots: std::collections::HashSet::new(),
            is_first_section: true,
            first_section_complete: false,
            section_count: 0,
            enabled: true,
            assignments: HashMap::new(),
        }
    }

    /// Turn recall on or off. Off, a chord is what is written: nothing is
    /// remembered or recalled but the chart's explicit assignments.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether recall is on.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Section Lifecycle Methods
    // ─────────────────────────────────────────────────────────────────────────

    /// Called when entering a new section.
    ///
    /// For the first section: memory starts empty, definitions will populate global.
    /// For subsequent sections: clears section-local memory, global remains for fallback.
    pub fn enter_section(&mut self) {
        self.section_count += 1;

        if self.first_section_complete {
            // Not the first section - clear section-local memory
            self.section_family.clear();
            self.is_first_section = false;
        } else {
            // First section - keep is_first_section = true
            self.is_first_section = true;
        }
    }

    /// Called when the first section is complete.
    ///
    /// Copies all first section memory to global memory for fallback in later sections.
    pub fn complete_first_section(&mut self) {
        if !self.first_section_complete {
            // First section memory is already in global_family (stored during processing)
            // Just mark as complete
            self.first_section_complete = true;
            self.is_first_section = false;
        }
    }

    /// Add an explicit global assignment from metadata (e.g., `Cm = Cm7b5`).
    ///
    /// These assignments take precedence and are available in all sections.
    pub fn add_global_assignment(&mut self, basic_chord: &str, full_chord: &str) {
        // Extract the root from the basic chord (first letter + optional accidental)
        let root = Self::extract_root(basic_chord);

        // Determine the family from the basic chord
        let family = Self::get_chord_family(basic_chord, &root);
        let family_key = Self::memory_key(&root, family);
        self.assignments
            .insert(family_key.clone(), full_chord.to_string());
        self.global_family
            .insert(family_key, full_chord.to_string());
    }

    /// Extract the root note from a chord symbol (e.g., "Cm" -> "C", "F#m7" -> "F#")
    fn extract_root(symbol: &str) -> String {
        let mut chars = symbol.chars();
        let mut root = String::new();

        // First character is the note name
        if let Some(c) = chars.next() {
            root.push(c);
        }

        // Second character might be an accidental (# or b)
        if let Some(c) = chars.next()
            && (c == '#' || c == 'b')
        {
            root.push(c);
        }

        root
    }

    /// Check if we're currently in the first section
    pub fn in_first_section(&self) -> bool {
        self.is_first_section
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Legacy API (kept for compatibility)
    // ─────────────────────────────────────────────────────────────────────────

    /// Remember a chord in global memory
    pub fn remember_global(&mut self, root: &str, full_symbol: &str) {
        self.global
            .insert(root.to_lowercase(), full_symbol.to_string());
    }

    /// Remember a chord in section-specific memory
    pub fn remember_section(&mut self, section_type: &SectionType, root: &str, full_symbol: &str) {
        let section_key = section_type.full_name();
        self.section_specific
            .entry(section_key)
            .or_default()
            .insert(root.to_lowercase(), full_symbol.to_string());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Family-Aware Memory Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Store a chord in section-local family memory.
    /// If in first section, also stores to global family memory.
    fn store_to_family_memory(&mut self, family_key: &str, full_symbol: &str) {
        // Only store the first occurrence per section (first-wins)
        self.section_family
            .entry(family_key.to_string())
            .or_insert_with(|| full_symbol.to_string());

        // If first section, also store to global (first-wins)
        if self.is_first_section {
            self.global_family
                .entry(family_key.to_string())
                .or_insert_with(|| full_symbol.to_string());
        }
    }

    /// Recall a chord from family memory.
    /// Checks section-local first, then falls back to global.
    fn recall_from_family_memory(&self, family_key: &str) -> Option<String> {
        // Check section-local first
        if let Some(symbol) = self.section_family.get(family_key) {
            return Some(symbol.clone());
        }

        // Fall back to global
        self.global_family.get(family_key).cloned()
    }

    /// Process a chord token and return the full symbol to use
    ///
    /// This is the main API for the parser to use.
    ///
    /// Hierarchy (v1-compatible):
    /// 1. Override (`!`): Use quality but DON'T update memory
    /// 2. Has explicit quality: Remember in BOTH global AND section memory
    /// 3. Just root: Check section → Check global (and copy to section) → Infer from key (and remember)
    ///
    /// # Arguments
    /// * `root` - The root note as a string (e.g., "C", "D", "G")
    /// * `token` - The original token (e.g., "Cmaj7", "c", "d")
    /// * `parsed_symbol` - The normalized symbol from the chord parser
    /// * `section_type` - The current section type
    /// * `is_override` - Whether this is a one-time override (prefix `!`)
    /// * `current_key` - The current key for inferring default qualities (optional)
    ///
    /// # Returns
    /// The full chord symbol to use for this chord
    pub fn process_chord(
        &mut self,
        root: &str,
        token: &str,
        parsed_symbol: &str,
        _section_type: &SectionType,
        is_override: bool,
        current_key: Option<&crate::key::Key>,
    ) -> String {
        // Determine the chord family (major or minor)
        let family = Self::get_chord_family(parsed_symbol, root);
        let family_key = Self::memory_key(root, family);
        let is_basic = Self::is_basic_chord(parsed_symbol, root);

        // Determine if this chord has explicit quality information
        // Strip rhythm notation (/, _, ') from token before checking length
        let token_chord_part = if let Some(pos) = token.find(['/', '_', '\'']) {
            &token[..pos]
        } else {
            token
        };
        let has_quality = token_chord_part.len() > root.len();

        if !self.enabled && !is_override {
            return self.process_without_memory(
                root,
                token_chord_part,
                parsed_symbol,
                &family_key,
                (is_basic, has_quality),
                current_key,
            );
        }

        if is_override {
            // Override with `!` prefix: use parsed quality but DON'T update any memory
            // This bypasses ALL memory including global
            parsed_symbol.to_string()
        } else if is_basic {
            // Basic chord (C or Cm) - CAN recall from family memory, but DON'T store
            // Lookup order: section-local → global
            if let Some(remembered) = self.recall_from_family_memory(&family_key) {
                // Found in memory (section-local or global fallback)
                self.seen_roots.insert(root.to_lowercase());
                remembered
            } else {
                // No memory for this family - return basic chord as-is
                self.seen_roots.insert(root.to_lowercase());
                parsed_symbol.to_string()
            }
        } else if has_quality {
            // Extended chord with explicit quality - store to family memory
            // For scale degrees with explicit quality (e.g., "2maj"), preserve the quality
            let output_symbol = if parsed_symbol == root && token_chord_part.len() > root.len() {
                token_chord_part.to_string()
            } else {
                parsed_symbol.to_string()
            };

            // Store to section-local memory (and global if in first section)
            self.store_to_family_memory(&family_key, &output_symbol);
            self.seen_roots.insert(root.to_lowercase());
            output_symbol
        } else {
            // Just root - lookup hierarchy: section-local → global → key inference
            let root_lower = root.to_lowercase();

            // Try to recall from family memory first
            if let Some(remembered) = self.recall_from_family_memory(&family_key) {
                self.seen_roots.insert(root_lower);
                return remembered;
            }

            // No memory - try key inference
            let result = if let Some(key_default) = Self::infer_from_key(root, current_key) {
                key_default
            } else {
                // No key or couldn't infer - use parsed as-is
                parsed_symbol.to_string()
            };

            // Store result for future recall within this section
            self.store_to_family_memory(&family_key, &result);
            self.seen_roots.insert(root_lower);
            result
        }
    }

    /// [`Self::process_chord`] with memory off: the same three cases, with
    /// nothing remembered or recalled. A basic triad is as written; an
    /// explicit quality is as written; a bare root takes the key's quality
    /// for its degree. An explicit assignment (`Cm = Cm7b5`) still applies
    /// to a basic or bare chord — it is written on purpose, not memory.
    fn process_without_memory(
        &mut self,
        root: &str,
        token_chord_part: &str,
        parsed_symbol: &str,
        family_key: &str,
        (is_basic, has_quality): (bool, bool),
        current_key: Option<&crate::key::Key>,
    ) -> String {
        self.seen_roots.insert(root.to_lowercase());
        if (is_basic || !has_quality)
            && let Some(assigned) = self.assignments.get(family_key)
        {
            return assigned.clone();
        }
        if is_basic {
            parsed_symbol.to_string()
        } else if has_quality {
            // A bare scale degree with an explicit quality (`2maj`) keeps it.
            if parsed_symbol == root {
                token_chord_part.to_string()
            } else {
                parsed_symbol.to_string()
            }
        } else {
            Self::infer_from_key(root, current_key).unwrap_or_else(|| parsed_symbol.to_string())
        }
    }

    /// Check if a chord is a "basic" major or minor triad without extensions.
    ///
    /// Basic chords CAN RECALL from memory but DON'T STORE:
    /// - Basic major: just the root (e.g., "C", "G", "Bb")
    /// - Basic minor: root + "m" only (e.g., "Cm", "Gm", "Bbm")
    ///
    /// NOT basic (has extensions, alterations, or non-standard quality):
    /// - `C7`, `Cm7`, `Cmaj7` (7ths)
    /// - `C9`, `Cm9`, `Cmaj9` (9ths)
    /// - `Cdim`, `Cdim7` (diminished)
    /// - `Caug`, `C+` (augmented)
    /// - `Csus`, `Csus2`, `Csus4` (suspended)
    /// - `C5` (power chord)
    /// - `C6`, `Cm6` (6th chords)
    /// - `Cadd9`, `Cadd11` (add chords)
    fn is_basic_chord(parsed_symbol: &str, root: &str) -> bool {
        // Basic major: symbol is exactly the root
        if parsed_symbol == root {
            return true;
        }

        // Basic minor: symbol is root + "m" (case insensitive for the "m")
        // But NOT if there's more after the "m" (like "maj", "m7", etc.)
        if let Some(rest) = parsed_symbol.strip_prefix(root) {
            // Check if rest is exactly "m" (minor)
            if rest == "m" {
                return true;
            }
        }

        false
    }

    /// Determine the chord family (major or minor) from the parsed symbol.
    ///
    /// Minor family: contains "m" after the root (Cm, Cm7, Cdim, etc.)
    /// Major family: everything else (C, Cmaj7, C7, Caug, Csus, etc.)
    fn get_chord_family(parsed_symbol: &str, root: &str) -> ChordFamily {
        if let Some(rest) = parsed_symbol.strip_prefix(root) {
            // Check if the quality starts with "m" (minor, but not "maj")
            if rest.starts_with('m') && !rest.starts_with("maj") {
                return ChordFamily::Minor;
            }
            // Also check for "dim" as minor family (debatable, but commonly used in minor contexts)
            if rest.starts_with("dim") {
                return ChordFamily::Minor;
            }
        }
        ChordFamily::Major
    }

    /// Create a memory key that includes root and family.
    /// Format: "root:major" or "root:minor"
    fn memory_key(root: &str, family: ChordFamily) -> String {
        let family_str = match family {
            ChordFamily::Major => "major",
            ChordFamily::Minor => "minor",
        };
        format!("{}:{}", root.to_lowercase(), family_str)
    }

    /// Infer a chord quality from the current key
    ///
    /// Returns a full chord symbol with the appropriate quality appended to the original root
    fn infer_from_key(root: &str, key: Option<&crate::key::Key>) -> Option<String> {
        use crate::key::scale::harmonization::{HarmonizationDepth, harmonize_scale};

        let key = key?;

        // Harmonize the scale (returns Vec<Chord>)
        let chords = harmonize_scale(&key.mode, &key.root, HarmonizationDepth::Triads);

        // Check if this is a scale degree (1-7)
        if let Ok(degree) = root.parse::<usize>()
            && (1..=7).contains(&degree)
            && let Some(_chord) = chords.get(degree - 1)
        {
            // For scale degrees, the quality is implied by the key
            // Don't append any quality suffix - just return the root
            return Some(root.to_string());
        }

        // Check if this is a Roman numeral
        let root_upper = root.to_uppercase();
        match root_upper.as_str() {
            "I" | "II" | "III" | "IV" | "V" | "VI" | "VII" => {
                let degree = match root_upper.as_str() {
                    "I" => 1,
                    "II" => 2,
                    "III" => 3,
                    "IV" => 4,
                    "V" => 5,
                    "VI" => 6,
                    "VII" => 7,
                    _ => return None,
                };
                if let Some(_chord) = chords.get(degree - 1) {
                    // For Roman numerals, the quality is implied by the key
                    // Don't append any quality suffix - just return the root
                    // Preserve original casing of Roman numeral
                    return Some(root.to_string());
                }
            }
            _ => {}
        }

        // Try to match by note name (case-insensitive, enharmonically aware)
        for chord in &chords {
            let chord_root_name = format!("{}", chord.root).to_lowercase();
            let target_root = root.to_lowercase();

            // Direct match
            if chord_root_name == target_root {
                let quality = Self::extract_quality(&chord.normalized);
                return Some(format!("{}{}", root, quality));
            }

            // Enharmonic match (e.g., A# = Bb, Gb = F#)
            if Self::are_enharmonic(&chord_root_name, &target_root) {
                let quality = Self::extract_quality(&chord.normalized);
                return Some(format!("{}{}", root, quality));
            }
        }

        None
    }

    /// Check if two note names are enharmonic equivalents
    fn are_enharmonic(note1: &str, note2: &str) -> bool {
        // Map each note to its semitone value (0-11)
        let semitone = |n: &str| -> Option<u8> {
            let n_upper = n.to_uppercase();
            let base = match n_upper.chars().next()? {
                'C' => 0,
                'D' => 2,
                'E' => 4,
                'F' => 5,
                'G' => 7,
                'A' => 9,
                'B' => 11,
                _ => return None,
            };

            let modifier: i8 = if n_upper.contains('#') {
                1
            } else if n_upper.contains('B') && n_upper.len() > 1 {
                -1
            } else {
                0
            };

            Some(((base as i8 + modifier).rem_euclid(12)) as u8)
        };

        match (semitone(note1), semitone(note2)) {
            (Some(s1), Some(s2)) => s1 == s2,
            _ => false,
        }
    }

    /// Extract the quality portion from a normalized chord symbol
    /// E.g., "Gmaj7" -> "maj7", "Em" -> "m", "A" -> ""
    fn extract_quality(normalized: &str) -> &str {
        // Find where the note name ends (could be 1-2 chars: C, C#, Bb, etc.)
        let mut chars = normalized.chars();
        let first = chars.next();

        if first.is_none() {
            return "";
        }

        let second = chars.next();

        // If second char is 'b' or '#', quality starts at index 2
        if let Some(c) = second
            && (c == 'b' || c == '#')
        {
            return &normalized[2..];
        }

        // Otherwise, quality starts at index 1
        &normalized[1..]
    }

    /// Recall a chord from memory (section-specific first, then global)
    pub fn recall(&self, root: &str, section_type: Option<&SectionType>) -> Option<String> {
        let root_lower = root.to_lowercase();

        // Try section-specific memory first
        if let Some(section_type) = section_type {
            let section_key = section_type.full_name();
            if let Some(section_mem) = self.section_specific.get(&section_key)
                && let Some(symbol) = section_mem.get(&root_lower)
            {
                return Some(symbol.clone());
            }
        }

        // Fall back to global memory
        self.global.get(&root_lower).cloned()
    }

    /// Recall from global memory only
    pub fn recall_global(&self, root: &str) -> Option<String> {
        self.global.get(&root.to_lowercase()).cloned()
    }

    /// Recall from section-specific memory only
    pub fn recall_section(&self, section_type: &SectionType, root: &str) -> Option<String> {
        let section_key = section_type.full_name();
        self.section_specific
            .get(&section_key)
            .and_then(|section_mem| section_mem.get(&root.to_lowercase()).cloned())
    }

    /// Check if a root is in global memory
    pub fn has_global(&self, root: &str) -> bool {
        self.global.contains_key(&root.to_lowercase())
    }

    /// Check if a root is in section-specific memory
    pub fn has_section(&self, section_type: &SectionType, root: &str) -> bool {
        let section_key = section_type.full_name();
        self.section_specific
            .get(&section_key)
            .map(|section_mem| section_mem.contains_key(&root.to_lowercase()))
            .unwrap_or(false)
    }

    /// Clear all memory (full reset)
    pub fn clear(&mut self) {
        self.global_family.clear();
        self.section_family.clear();
        self.global.clear();
        self.section_specific.clear();
        self.seen_roots.clear();
        self.is_first_section = true;
        self.first_section_complete = false;
        self.section_count = 0;
    }

    /// Clear section-local memory.
    ///
    /// This is called when entering a section with explicit content,
    /// allowing it to build its own memory from scratch.
    /// Global memory is preserved for fallback.
    pub fn clear_section(&mut self, _section_type: &SectionType) {
        // Clear section-local family memory
        self.section_family.clear();
    }

    // Note: Scale-derived defaults have been removed from ChordMemory.
    // Future scale-to-chord inference will live in a dedicated module.
}

impl Default for ChordMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::Key;

    #[test]
    fn test_global_memory() {
        let mut memory = ChordMemory::new();

        memory.remember_global("G", "Gmaj7");
        memory.remember_global("C", "Cmaj7");

        assert_eq!(memory.recall_global("G"), Some("Gmaj7".to_string()));
        assert_eq!(memory.recall_global("C"), Some("Cmaj7".to_string()));
        assert_eq!(memory.recall_global("D"), None);
    }

    #[test]
    fn test_section_memory() {
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;
        let chorus = SectionType::Chorus;

        memory.remember_section(&verse, "G", "Gmaj13");
        memory.remember_section(&chorus, "G", "Gmaj6");

        assert_eq!(
            memory.recall_section(&verse, "G"),
            Some("Gmaj13".to_string())
        );
        assert_eq!(
            memory.recall_section(&chorus, "G"),
            Some("Gmaj6".to_string())
        );
    }

    #[test]
    fn test_memory_hierarchy() {
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;

        memory.remember_global("G", "Gmaj7");
        memory.remember_section(&verse, "G", "Gmaj13");

        // Section memory takes precedence
        assert_eq!(memory.recall("G", Some(&verse)), Some("Gmaj13".to_string()));

        // Falls back to global for other sections
        assert_eq!(memory.recall("G", None), Some("Gmaj7".to_string()));
    }

    #[test]
    fn test_process_chord_with_quality() {
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;

        // Process a chord with explicit quality - stores in family memory
        let result = memory.process_chord("C", "Cmaj7", "Cmaj7", &verse, false, None);
        assert_eq!(result, "Cmaj7");

        // Verify by recalling with a basic chord
        let result2 = memory.process_chord("C", "C", "C", &verse, false, None);
        assert_eq!(result2, "Cmaj7"); // Basic chord recalls from family memory
    }

    #[test]
    fn test_process_chord_recall() {
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;

        // Define a quality with an extended chord - stores in major family memory
        memory.process_chord("C", "Cmaj7", "Cmaj7", &verse, false, None);

        // Basic major chord RECALLS from major family memory
        let result = memory.process_chord("C", "c", "C", &verse, false, None);
        assert_eq!(result, "Cmaj7"); // Recalls from major family

        // Basic minor chord has no memory yet (minor family is separate)
        let result = memory.process_chord("C", "Cm", "Cm", &verse, false, None);
        assert_eq!(result, "Cm"); // No minor family memory yet
    }

    #[test]
    fn test_basic_chords_recall_from_family() {
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;

        // Store Cmaj7 in major family memory
        memory.process_chord("C", "Cmaj7", "Cmaj7", &verse, false, None);

        // Basic major chord RECALLS Cmaj7 from major family
        let result = memory.process_chord("C", "C", "C", &verse, false, None);
        assert_eq!(result, "Cmaj7");

        // Basic minor chord has no memory (minor family is separate)
        let result = memory.process_chord("C", "Cm", "Cm", &verse, false, None);
        assert_eq!(result, "Cm");

        // Extended chords with explicit quality return their own quality
        memory.process_chord("D", "Dmaj9", "Dmaj9", &verse, false, None);
        let result = memory.process_chord("D", "D7", "D7", &verse, false, None);
        // D7 has explicit quality, returns D7 (but memory keeps Dmaj9 — first-wins)
        assert_eq!(result, "D7");

        // Basic D recalls the FIRST stored quality (Dmaj9), not the later D7
        let result = memory.process_chord("D", "D", "D", &verse, false, None);
        assert_eq!(result, "Dmaj9");
    }

    #[test]
    fn test_process_chord_global_to_section_copy() {
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;
        let chorus = SectionType::Chorus;

        // Define in verse - stores Cmaj7 in major family memory
        memory.process_chord("C", "Cmaj7", "Cmaj7", &verse, false, None);

        // Basic chord in chorus recalls from global major family memory
        let result = memory.process_chord("C", "c", "C", &chorus, false, None);

        // Basic chords recall from their family's global memory
        assert_eq!(result, "Cmaj7");
    }

    #[test]
    fn test_split_family_memory() {
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;

        // Store Cmaj7 in major family memory
        memory.process_chord("C", "Cmaj7", "Cmaj7", &verse, false, None);

        // Store Cm7 in minor family memory
        memory.process_chord("C", "Cm7", "Cm7", &verse, false, None);

        // Basic major C recalls from major family
        let result = memory.process_chord("C", "C", "C", &verse, false, None);
        assert_eq!(result, "Cmaj7");

        // Basic minor Cm recalls from minor family
        let result = memory.process_chord("C", "Cm", "Cm", &verse, false, None);
        assert_eq!(result, "Cm7");

        // Families are separate: major doesn't affect minor
        memory.process_chord("D", "Dmaj9", "Dmaj9", &verse, false, None);
        // Basic Dm has no minor family memory for D
        let result = memory.process_chord("D", "Dm", "Dm", &verse, false, None);
        assert_eq!(result, "Dm"); // No minor family memory
    }

    #[test]
    fn test_process_chord_override() {
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;

        // Define default quality in family memory
        memory.process_chord("C", "Cmaj7", "Cmaj7", &verse, false, None);

        // Override without remembering
        let result = memory.process_chord("C", "Cmaj9", "Cmaj9", &verse, true, None);
        assert_eq!(result, "Cmaj9");

        // Verify family memory still has maj7 by recalling with basic chord
        let result2 = memory.process_chord("C", "C", "C", &verse, false, None);
        assert_eq!(result2, "Cmaj7"); // Override didn't change memory
    }

    #[test]
    fn test_clear_section_memory() {
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;

        // Enter section and store a chord
        memory.enter_section();
        let result = memory.process_chord("C", "Cmaj7", "Cmaj7", &verse, false, None);
        assert_eq!(result, "Cmaj7");

        // Clear section memory
        memory.clear_section(&verse);

        // After clearing, there's no section-local memory (but global may exist)
        // Since this was the first section, it also stored to global
        // A new basic C should still recall from global
        let result2 = memory.process_chord("C", "c", "C", &verse, false, None);
        assert_eq!(result2, "Cmaj7"); // Still recalls from global
    }

    #[test]
    fn test_section_redefinition_workflow() {
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;

        // First verse: define Cmaj7 - stores in major family memory
        memory.process_chord("C", "Cmaj7", "Cmaj7", &verse, false, None);

        // Clear section memory (simulating explicit redefinition)
        memory.clear_section(&verse);

        // Basic chord recalls from global major family memory
        let result = memory.process_chord("C", "c", "C", &verse, false, None);
        assert_eq!(result, "Cmaj7"); // Recalls from global major family
    }

    #[test]
    fn test_multiple_sections_independence() {
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;
        let chorus = SectionType::Chorus;

        // Define Cmaj7 in verse (first section — stores to both section + global)
        memory.process_chord("C", "Cmaj7", "Cmaj7", &verse, false, None);

        // Define Cmaj9 in chorus — first-wins: global keeps Cmaj7, section keeps Cmaj7
        memory.process_chord("C", "Cmaj9", "Cmaj9", &chorus, false, None);

        // Clear verse and recall — basic chord recalls from global major family
        // Global has Cmaj7 (first-wins)
        memory.clear_section(&verse);
        let result = memory.process_chord("C", "c", "C", &verse, false, None);

        // Recalls from global major family (first = Cmaj7)
        assert_eq!(result, "Cmaj7");
    }

    #[test]
    fn test_explicit_quality_always_works() {
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;
        let chorus = SectionType::Chorus;

        // Define Cmaj7 in verse
        memory.process_chord("C", "Cmaj7", "Cmaj7", &verse, false, None);

        // Explicit quality in chorus still returns its own quality
        let result = memory.process_chord("C", "Cmaj9", "Cmaj9", &chorus, false, None);
        assert_eq!(result, "Cmaj9");

        // Basic chord recalls from family memory (first-wins: Cmaj7)
        let result2 = memory.process_chord("C", "c", "C", &chorus, false, None);
        assert_eq!(result2, "Cmaj7"); // Recalls first occurrence
    }

    #[test]
    fn test_first_wins_chord_memory() {
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;

        // Dm7 is the first minor-family chord for D — stores Dm7
        memory.enter_section();
        let result = memory.process_chord("D", "Dm7", "Dm7", &verse, false, None);
        assert_eq!(result, "Dm7");

        // Dm9 appears later — returns Dm9 (explicit quality) but doesn't overwrite memory
        let result = memory.process_chord("D", "Dm9", "Dm9", &verse, false, None);
        assert_eq!(result, "Dm9");

        // Basic Dm recalls the FIRST stored quality (Dm7), not the later Dm9
        let result = memory.process_chord("D", "Dm", "Dm", &verse, false, None);
        assert_eq!(result, "Dm7");
    }

    #[test]
    fn test_first_wins_across_sections() {
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;

        // First section: Gm7 sets the memory
        memory.enter_section();
        memory.process_chord("G", "Gm7", "Gm7", &verse, false, None);
        memory.complete_first_section();

        // Second section: Gm9 appears first — sets NEW section memory
        memory.enter_section();
        memory.process_chord("G", "Gm9", "Gm9", &verse, false, None);

        // Later Gm11 doesn't overwrite section memory (first-wins)
        memory.process_chord("G", "Gm11", "Gm11", &verse, false, None);

        // Basic Gm recalls section-first = Gm9 (not Gm11, not global Gm7)
        let result = memory.process_chord("G", "Gm", "Gm", &verse, false, None);
        assert_eq!(result, "Gm9");
    }

    #[test]
    fn test_debug_csharp_major_harmonization() {
        use crate::key::scale::harmonization::{HarmonizationDepth, harmonize_scale};
        use crate::primitives::MusicalNote;

        // Create C# major key from semitone
        let c_sharp = MusicalNote::from_semitone(1, true); // C# is 1 semitone above C
        let key = Key::major(c_sharp);

        // Get harmonization
        let chords = harmonize_scale(&key.mode, &key.root, HarmonizationDepth::Triads);

        println!("\n=== C# Major Harmonization ===");
        for (i, chord) in chords.iter().enumerate() {
            println!(
                "Degree {}: root={}, normalized={}",
                i + 1,
                chord.root,
                chord.normalized
            );
        }

        // Test that basic chords (like A#) don't use key inference
        // Key inference only applies to scale degrees and Roman numerals
        let mut memory = ChordMemory::new();
        let verse = SectionType::Verse;

        // Writing "a#" (basic major) gives A# major, NOT A#m from key inference
        // This is intentional: explicit note names with no quality suffix = major
        let result = memory.process_chord("A#", "a#", "A#", &verse, false, Some(&key));
        println!("\nA# written in C# major context: {}", result);
        assert_eq!(
            result, "A#",
            "A# (basic major) stays A# - basic chords don't use key inference"
        );

        // Scale degrees WOULD use key inference, but note names don't
        // For scale degree 6 in C# major: "6" -> would infer minor
    }
}
