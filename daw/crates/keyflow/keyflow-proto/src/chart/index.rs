//! Chart Index for Bidirectional Lookups
//!
//! Provides efficient queries between:
//! - Source text positions (byte offsets) → rendered elements
//! - Musical positions (measure/beat) → rendered elements
//! - Element IDs → source links
//!
//! This enables features like:
//! - Click on rendered chord → highlight source text
//! - Select source text → highlight rendered elements
//! - Navigate by musical position

use std::collections::HashMap;
use std::ops::Range;

use super::position::ChartPosition;
use super::source_link::SourceLink;
use crate::parsing::TextSpan;
use facet::Facet;

/// Unique identifier for indexed elements.
/// This corresponds to element IDs in the scene graph.
pub type ElementId = u64;

/// Index for bidirectional lookups between source text and rendered elements.
///
/// The index maintains three lookup tables:
/// - `by_source_offset`: Find elements by byte offset in source text
/// - `by_position`: Find elements by musical position (measure, beat)
/// - `source_links`: Get full source link info by element ID
///
/// # Example
///
/// ```ignore
/// // Build index from scene graph
/// let index = ChartIndex::from_scene(&scene_root);
///
/// // Click handling: find element at byte offset
/// let elements = index.find_by_source_offset(42);
///
/// // Find all elements in measure 4, beat 2
/// let elements = index.find_by_position(4, 2);
///
/// // Get source info for an element
/// if let Some(link) = index.get_source_link(element_id) {
///     println!("Source: line {}, column {}", link.span.line, link.span.column);
/// }
/// ```
#[derive(Debug, Clone, Default, Facet)]
pub struct ChartIndex {
    /// Map from source byte offset to element IDs.
    /// Multiple elements can share the same source offset (e.g., chord + accidental).
    by_source_offset: HashMap<usize, Vec<ElementId>>,

    /// Map from musical position (measure, beat) to element IDs.
    /// Key is (measure_index, beat_index).
    by_position: HashMap<(u32, u32), Vec<ElementId>>,

    /// Map from element ID to full source link information.
    source_links: HashMap<ElementId, SourceLink>,

    /// Map from element ID to chart position (for elements without full source links).
    positions: HashMap<ElementId, ChartPosition>,
}

impl ChartIndex {
    /// Create a new empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an element with its source link to the index.
    pub fn add_element(&mut self, id: ElementId, source_link: SourceLink) {
        // Index by source offset
        let start = source_link.span.start;
        self.by_source_offset.entry(start).or_default().push(id);

        // Index by musical position
        let pos_key = (source_link.position.measure, source_link.position.beat);
        self.by_position.entry(pos_key).or_default().push(id);

        // Store the position
        self.positions.insert(id, source_link.position.clone());

        // Store the full source link
        self.source_links.insert(id, source_link);
    }

    /// Add an element with just a chart position (no source span).
    pub fn add_element_position(&mut self, id: ElementId, position: ChartPosition) {
        let pos_key = (position.measure, position.beat);
        self.by_position.entry(pos_key).or_default().push(id);
        self.positions.insert(id, position);
    }

    /// Add an element with just a source span (no chart position).
    pub fn add_element_span(&mut self, id: ElementId, span: TextSpan) {
        self.by_source_offset
            .entry(span.start)
            .or_default()
            .push(id);
    }

    // =========================================================================
    // Source-based Queries (for click-to-highlight)
    // =========================================================================

    /// Find all elements that contain a specific byte offset.
    ///
    /// This is the primary method for click-to-highlight: when the user clicks
    /// on source text at a given byte offset, find the corresponding rendered elements.
    #[must_use]
    pub fn find_by_source_offset(&self, offset: usize) -> Vec<ElementId> {
        // First check exact match
        if let Some(ids) = self.by_source_offset.get(&offset) {
            return ids.clone();
        }

        // Otherwise, find elements whose spans contain this offset
        let mut results = Vec::new();
        for (id, link) in &self.source_links {
            if link.span.contains(offset) {
                results.push(*id);
            }
        }
        results
    }

    /// Find all elements whose source spans overlap with a byte range.
    ///
    /// This is for selection handling: when the user selects a range of source text,
    /// find all rendered elements that correspond to that selection.
    #[must_use]
    pub fn find_by_source_range(&self, range: Range<usize>) -> Vec<ElementId> {
        let mut results = Vec::new();
        for (id, link) in &self.source_links {
            if link.span.overlaps_range(range.start, range.end) {
                results.push(*id);
            }
        }
        results
    }

    /// Find all elements on a specific line of source text.
    #[must_use]
    pub fn find_by_source_line(&self, line: u32) -> Vec<ElementId> {
        let mut results = Vec::new();
        for (id, link) in &self.source_links {
            if link.span.line == line {
                results.push(*id);
            }
        }
        results
    }

    // =========================================================================
    // Position-based Queries (for musical navigation)
    // =========================================================================

    /// Find all elements at a specific musical position (measure and beat).
    #[must_use]
    pub fn find_by_position(&self, measure: u32, beat: u32) -> Vec<ElementId> {
        self.by_position
            .get(&(measure, beat))
            .cloned()
            .unwrap_or_default()
    }

    /// Find all elements in a specific measure (any beat).
    #[must_use]
    pub fn find_by_measure(&self, measure: u32) -> Vec<ElementId> {
        let mut results = Vec::new();
        for ((m, _), ids) in &self.by_position {
            if *m == measure {
                results.extend(ids.iter().copied());
            }
        }
        results
    }

    /// Find all elements in a range of measures.
    #[must_use]
    pub fn find_by_measure_range(&self, start_measure: u32, end_measure: u32) -> Vec<ElementId> {
        let mut results = Vec::new();
        for ((m, _), ids) in &self.by_position {
            if *m >= start_measure && *m < end_measure {
                results.extend(ids.iter().copied());
            }
        }
        results
    }

    /// Find all elements in a specific system.
    #[must_use]
    pub fn find_by_system(&self, system: u32) -> Vec<ElementId> {
        let mut results = Vec::new();
        for (id, pos) in &self.positions {
            if pos.system == system {
                results.push(*id);
            }
        }
        results
    }

    // =========================================================================
    // Element Lookup
    // =========================================================================

    /// Get the source link for an element by ID.
    #[must_use]
    pub fn get_source_link(&self, id: ElementId) -> Option<&SourceLink> {
        self.source_links.get(&id)
    }

    /// Get the chart position for an element by ID.
    #[must_use]
    pub fn get_position(&self, id: ElementId) -> Option<&ChartPosition> {
        self.positions.get(&id)
    }

    /// Get the source span for an element by ID.
    #[must_use]
    pub fn get_source_span(&self, id: ElementId) -> Option<&TextSpan> {
        self.source_links.get(&id).map(|link| &link.span)
    }

    /// Check if an element has source link information.
    #[must_use]
    pub fn has_source_link(&self, id: ElementId) -> bool {
        self.source_links.contains_key(&id)
    }

    // =========================================================================
    // Statistics
    // =========================================================================

    /// Get the total number of indexed elements.
    #[must_use]
    pub fn element_count(&self) -> usize {
        self.positions.len()
    }

    /// Get the number of elements with source links.
    #[must_use]
    pub fn source_link_count(&self) -> usize {
        self.source_links.len()
    }

    /// Get the number of unique source offsets indexed.
    #[must_use]
    pub fn source_offset_count(&self) -> usize {
        self.by_source_offset.len()
    }

    /// Get the number of unique positions indexed.
    #[must_use]
    pub fn position_count(&self) -> usize {
        self.by_position.len()
    }

    /// Get all indexed element IDs.
    pub fn all_element_ids(&self) -> impl Iterator<Item = ElementId> + '_ {
        self.positions.keys().copied()
    }

    /// Get all indexed source links.
    pub fn all_source_links(&self) -> impl Iterator<Item = (&ElementId, &SourceLink)> {
        self.source_links.iter()
    }

    // =========================================================================
    // Merge and Clear
    // =========================================================================

    /// Merge another index into this one.
    pub fn merge(&mut self, other: ChartIndex) {
        for (offset, ids) in other.by_source_offset {
            self.by_source_offset.entry(offset).or_default().extend(ids);
        }
        for (pos, ids) in other.by_position {
            self.by_position.entry(pos).or_default().extend(ids);
        }
        self.source_links.extend(other.source_links);
        self.positions.extend(other.positions);
    }

    /// Clear all entries from the index.
    pub fn clear(&mut self) {
        self.by_source_offset.clear();
        self.by_position.clear();
        self.source_links.clear();
        self.positions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::TextSpan;

    fn create_test_source_link(
        offset: usize,
        len: usize,
        line: u32,
        measure: u32,
        beat: u32,
    ) -> SourceLink {
        let span = TextSpan::with_location(offset, len, line, 1);
        let position = ChartPosition::at_beat(measure, beat);
        SourceLink::new(span, position)
    }

    #[test]
    fn test_new_index() {
        let index = ChartIndex::new();
        assert_eq!(index.element_count(), 0);
        assert_eq!(index.source_link_count(), 0);
    }

    #[test]
    fn test_add_element() {
        let mut index = ChartIndex::new();
        let link = create_test_source_link(10, 5, 3, 4, 2);

        index.add_element(100, link.clone());

        assert_eq!(index.element_count(), 1);
        assert_eq!(index.source_link_count(), 1);
        assert!(index.has_source_link(100));
    }

    #[test]
    fn test_find_by_source_offset_exact() {
        let mut index = ChartIndex::new();
        index.add_element(100, create_test_source_link(10, 5, 1, 0, 0));
        index.add_element(101, create_test_source_link(20, 5, 1, 0, 1));

        let found = index.find_by_source_offset(10);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], 100);
    }

    #[test]
    fn test_find_by_source_offset_within_span() {
        let mut index = ChartIndex::new();
        // Span from offset 10 to 15 (len 5)
        index.add_element(100, create_test_source_link(10, 5, 1, 0, 0));

        // Offset 12 is within the span
        let found = index.find_by_source_offset(12);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], 100);

        // Offset 16 is outside the span
        let found = index.find_by_source_offset(16);
        assert!(found.is_empty());
    }

    #[test]
    fn test_find_by_source_range() {
        let mut index = ChartIndex::new();
        index.add_element(100, create_test_source_link(10, 5, 1, 0, 0)); // 10-15
        index.add_element(101, create_test_source_link(20, 5, 1, 0, 1)); // 20-25
        index.add_element(102, create_test_source_link(30, 5, 1, 1, 0)); // 30-35

        // Range 8-22 should match elements at 10-15 and 20-25
        let found = index.find_by_source_range(8..22);
        assert_eq!(found.len(), 2);
        assert!(found.contains(&100));
        assert!(found.contains(&101));
    }

    #[test]
    fn test_find_by_position() {
        let mut index = ChartIndex::new();
        index.add_element(100, create_test_source_link(10, 5, 1, 4, 2));
        index.add_element(101, create_test_source_link(20, 5, 1, 4, 3));
        index.add_element(102, create_test_source_link(30, 5, 1, 5, 0));

        let found = index.find_by_position(4, 2);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], 100);
    }

    #[test]
    fn test_find_by_measure() {
        let mut index = ChartIndex::new();
        index.add_element(100, create_test_source_link(10, 5, 1, 4, 0));
        index.add_element(101, create_test_source_link(20, 5, 1, 4, 1));
        index.add_element(102, create_test_source_link(30, 5, 1, 5, 0));

        let found = index.find_by_measure(4);
        assert_eq!(found.len(), 2);
        assert!(found.contains(&100));
        assert!(found.contains(&101));
    }

    #[test]
    fn test_get_source_link() {
        let mut index = ChartIndex::new();
        let link = create_test_source_link(10, 5, 3, 4, 2);
        index.add_element(100, link);

        let retrieved = index.get_source_link(100);
        assert!(retrieved.is_some());
        let link = retrieved.unwrap();
        assert_eq!(link.span.start, 10);
        assert_eq!(link.span.len, 5);
        assert_eq!(link.position.measure, 4);
        assert_eq!(link.position.beat, 2);
    }

    #[test]
    fn test_find_by_source_line() {
        let mut index = ChartIndex::new();
        index.add_element(100, create_test_source_link(10, 5, 3, 0, 0));
        index.add_element(101, create_test_source_link(20, 5, 3, 0, 1));
        index.add_element(102, create_test_source_link(30, 5, 4, 1, 0));

        let found = index.find_by_source_line(3);
        assert_eq!(found.len(), 2);
        assert!(found.contains(&100));
        assert!(found.contains(&101));
    }

    #[test]
    fn test_merge() {
        let mut index1 = ChartIndex::new();
        index1.add_element(100, create_test_source_link(10, 5, 1, 0, 0));

        let mut index2 = ChartIndex::new();
        index2.add_element(101, create_test_source_link(20, 5, 1, 0, 1));

        index1.merge(index2);

        assert_eq!(index1.element_count(), 2);
        assert!(index1.has_source_link(100));
        assert!(index1.has_source_link(101));
    }

    #[test]
    fn test_clear() {
        let mut index = ChartIndex::new();
        index.add_element(100, create_test_source_link(10, 5, 1, 0, 0));
        index.add_element(101, create_test_source_link(20, 5, 1, 0, 1));

        assert_eq!(index.element_count(), 2);

        index.clear();

        assert_eq!(index.element_count(), 0);
        assert_eq!(index.source_link_count(), 0);
    }

    #[test]
    fn test_add_element_position_only() {
        let mut index = ChartIndex::new();
        let pos = ChartPosition::at_beat(4, 2);
        index.add_element_position(100, pos);

        assert_eq!(index.element_count(), 1);
        assert!(!index.has_source_link(100)); // No source link
        assert!(index.get_position(100).is_some()); // But has position

        let found = index.find_by_position(4, 2);
        assert_eq!(found.len(), 1);
    }

    // =========================================================================
    // Example Usage Tests - Demonstrating Real-World Workflows
    // =========================================================================

    /// Example: Indexing a realistic chart with multiple sections.
    ///
    /// This test shows how a chart parser would build an index as it processes
    /// chord symbols, with source locations pointing back to the original text.
    #[test]
    fn example_realistic_chart_index() {
        // Simulates parsing this chart:
        // ```
        // vs                          <- line 1, offset 0
        // | G . . . | C . . . |       <- line 2, offset 3 (G at 5, C at 15)
        // | Am . . . | D . . . |      <- line 3, offset 25 (Am at 27, D at 38)
        //
        // ch                          <- line 5, offset 50
        // | Em . . . | C . . . |      <- line 6, offset 53 (Em at 55, C at 66)
        // ```

        let mut index = ChartIndex::new();

        // Verse section - measure 0
        let g_chord = SourceLink::new(
            TextSpan::with_location(5, 1, 2, 3), // "G" at line 2, col 3
            ChartPosition::new(0, 0, 0),         // System 0, measure 0, beat 0
        );
        index.add_element(1001, g_chord);

        let c_chord = SourceLink::new(
            TextSpan::with_location(15, 1, 2, 13), // "C" at line 2, col 13
            ChartPosition::new(0, 1, 0),           // System 0, measure 1, beat 0
        );
        index.add_element(1002, c_chord);

        // Verse section - measure 1
        let am_chord = SourceLink::new(
            TextSpan::with_location(27, 2, 3, 3), // "Am" at line 3, col 3
            ChartPosition::new(0, 2, 0),          // System 0, measure 2, beat 0
        );
        index.add_element(1003, am_chord);

        let d_chord = SourceLink::new(
            TextSpan::with_location(38, 1, 3, 14), // "D" at line 3, col 14
            ChartPosition::new(0, 3, 0),           // System 0, measure 3, beat 0
        );
        index.add_element(1004, d_chord);

        // Chorus section - system 1
        let em_chord = SourceLink::new(
            TextSpan::with_location(55, 2, 6, 3), // "Em" at line 6, col 3
            ChartPosition::new(1, 4, 0),          // System 1, measure 4, beat 0
        );
        index.add_element(1005, em_chord);

        let c2_chord = SourceLink::new(
            TextSpan::with_location(66, 1, 6, 14), // "C" at line 6, col 14
            ChartPosition::new(1, 5, 0),           // System 1, measure 5, beat 0
        );
        index.add_element(1006, c2_chord);

        // Verify index statistics
        assert_eq!(index.element_count(), 6);
        assert_eq!(index.source_link_count(), 6);

        // Query by source line (find all chords on line 2)
        let line2_chords = index.find_by_source_line(2);
        assert_eq!(line2_chords.len(), 2);
        assert!(line2_chords.contains(&1001)); // G
        assert!(line2_chords.contains(&1002)); // C

        // Query by measure (find all chords in measure 2)
        let measure2_chords = index.find_by_measure(2);
        assert_eq!(measure2_chords.len(), 1);
        assert!(measure2_chords.contains(&1003)); // Am

        // Query by system (find all chords in chorus system)
        let chorus_chords = index.find_by_system(1);
        assert_eq!(chorus_chords.len(), 2);
        assert!(chorus_chords.contains(&1005)); // Em
        assert!(chorus_chords.contains(&1006)); // C
    }

    /// Example: Click-to-highlight workflow.
    ///
    /// When a user clicks on rendered notation, we need to:
    /// 1. Hit-test the click position against rendered bounds
    /// 2. Get the element ID from the hit node
    /// 3. Look up the source span to highlight in the editor
    #[test]
    fn example_click_to_highlight_workflow() {
        let mut index = ChartIndex::new();

        // Add chord elements with their source locations
        // Source text: "| Gmaj7 . . . | Dm7 . . . |"
        //               ^     ^         ^   ^
        //               0     5         14  17

        let gmaj7 = SourceLink::new(
            TextSpan::with_location(2, 5, 1, 3), // "Gmaj7" at offset 2, length 5
            ChartPosition::at_beat(0, 0),
        );
        index.add_element(100, gmaj7);

        let dm7 = SourceLink::new(
            TextSpan::with_location(16, 3, 1, 17), // "Dm7" at offset 16, length 3
            ChartPosition::at_beat(1, 0),
        );
        index.add_element(101, dm7);

        // Simulate: User clicks on Gmaj7 chord symbol
        // After hit-testing, we know they clicked on element 100
        let clicked_element_id: ElementId = 100;

        // Look up the source location to highlight
        let source_link = index.get_source_link(clicked_element_id).unwrap();

        // Extract highlight range for the editor
        let highlight_start = source_link.span.start;
        let highlight_end = source_link.span.end();
        let highlight_line = source_link.span.line;

        assert_eq!(highlight_start, 2);
        assert_eq!(highlight_end, 7); // 2 + 5
        assert_eq!(highlight_line, 1);

        // The editor would now:
        // 1. Scroll to line 1 if needed
        // 2. Select characters 2-7 to highlight "Gmaj7"
    }

    /// Example: Source text selection to rendered elements.
    ///
    /// When a user selects text in the editor, highlight the corresponding
    /// rendered elements in the notation view.
    #[test]
    fn example_selection_to_rendered_elements() {
        let mut index = ChartIndex::new();

        // Source text with chords at known positions:
        // "vs\n| G . . . | C . . . | Am . . . | D . . . |"
        //      ^3        ^13        ^23         ^34
        //      len=1     len=1      len=2       len=1

        index.add_element(
            1,
            SourceLink::new(
                TextSpan::with_location(5, 1, 2, 3),
                ChartPosition::at_beat(0, 0),
            ),
        );
        index.add_element(
            2,
            SourceLink::new(
                TextSpan::with_location(15, 1, 2, 13),
                ChartPosition::at_beat(1, 0),
            ),
        );
        index.add_element(
            3,
            SourceLink::new(
                TextSpan::with_location(25, 2, 2, 23),
                ChartPosition::at_beat(2, 0),
            ),
        );
        index.add_element(
            4,
            SourceLink::new(
                TextSpan::with_location(36, 1, 2, 34),
                ChartPosition::at_beat(3, 0),
            ),
        );

        // User selects bytes 10-30 (covers C and Am chords)
        let selection_start = 10;
        let selection_end = 30;

        let selected_elements = index.find_by_source_range(selection_start..selection_end);

        // Should find C (at 15) and Am (at 25)
        assert_eq!(selected_elements.len(), 2);
        assert!(selected_elements.contains(&2)); // C chord
        assert!(selected_elements.contains(&3)); // Am chord

        // The notation view would highlight these rendered chord symbols
    }

    /// Example: Playback cursor synchronization.
    ///
    /// During audio playback, sync the cursor position with rendered notation
    /// by querying elements at the current musical position.
    #[test]
    fn example_playback_cursor_sync() {
        let mut index = ChartIndex::new();

        // Build index with chord and rhythm slash elements
        // Measure 0: G on beat 0, slashes on beats 1-3
        index.add_element(
            100,
            SourceLink::new(TextSpan::new(0, 1), ChartPosition::at_beat(0, 0)),
        );
        index.add_element_position(101, ChartPosition::at_beat(0, 1)); // slash
        index.add_element_position(102, ChartPosition::at_beat(0, 2)); // slash
        index.add_element_position(103, ChartPosition::at_beat(0, 3)); // slash

        // Measure 1: C on beat 0, slashes on beats 1-3
        index.add_element(
            200,
            SourceLink::new(TextSpan::new(10, 1), ChartPosition::at_beat(1, 0)),
        );
        index.add_element_position(201, ChartPosition::at_beat(1, 1));
        index.add_element_position(202, ChartPosition::at_beat(1, 2));
        index.add_element_position(203, ChartPosition::at_beat(1, 3));

        // Simulate playback at measure 0, beat 2
        let current_measure = 0;
        let current_beat = 2;

        let elements_at_cursor = index.find_by_position(current_measure, current_beat);
        assert_eq!(elements_at_cursor.len(), 1);
        assert_eq!(elements_at_cursor[0], 102); // The slash at beat 2

        // Get position info for scrolling/highlighting
        let position = index.get_position(102).unwrap();
        assert_eq!(position.measure, 0);
        assert_eq!(position.beat, 2);

        // Advance to measure 1, beat 0
        let elements_at_new_pos = index.find_by_position(1, 0);
        assert_eq!(elements_at_new_pos.len(), 1);
        assert_eq!(elements_at_new_pos[0], 200); // The C chord

        // This element has a source link, so we could also scroll the editor
        let source = index.get_source_link(200);
        assert!(source.is_some());
    }

    /// Example: Building an index for a multi-page score.
    ///
    /// Shows how to use measure ranges to find elements on specific pages.
    #[test]
    fn example_multi_page_navigation() {
        let mut index = ChartIndex::new();

        // Page 1: measures 0-7 (2 systems of 4 measures each)
        for measure in 0..8 {
            let system = measure / 4;
            index.add_element(
                measure as u64,
                SourceLink::new(
                    TextSpan::with_location(measure * 10, 1, (measure + 1) as u32, 1),
                    ChartPosition::new(system as u32, measure as u32, 0),
                ),
            );
        }

        // Page 2: measures 8-15
        for measure in 8..16 {
            let system = 2 + (measure - 8) / 4; // Systems 2-3
            index.add_element(
                measure as u64,
                SourceLink::new(
                    TextSpan::with_location(measure * 10, 1, (measure + 1) as u32, 1),
                    ChartPosition::new(system as u32, measure as u32, 0),
                ),
            );
        }

        // Find all elements on page 1 (measures 0-7)
        let page1_elements = index.find_by_measure_range(0, 8);
        assert_eq!(page1_elements.len(), 8);

        // Find all elements on page 2 (measures 8-15)
        let page2_elements = index.find_by_measure_range(8, 16);
        assert_eq!(page2_elements.len(), 8);

        // Find elements in system 2 (first system of page 2)
        let system2_elements = index.find_by_system(2);
        assert_eq!(system2_elements.len(), 4); // measures 8-11
    }

    /// Example: Bidirectional lookup for editing.
    ///
    /// When editing a chord in the source, find and update the rendered element.
    /// When clicking a rendered element, navigate to its source.
    #[test]
    fn example_bidirectional_editing() {
        let mut index = ChartIndex::new();

        // Source: "| Cmaj7 | Am7 | Dm7 | G7 |"
        //           ^2      ^10   ^16   ^22
        let chords = [
            (1, 2, 5, "Cmaj7", 0), // element_id, offset, len, name, measure
            (2, 10, 3, "Am7", 1),
            (3, 18, 3, "Dm7", 2),
            (4, 26, 2, "G7", 3),
        ];

        for (id, offset, len, _name, measure) in chords {
            index.add_element(
                id,
                SourceLink::new(
                    TextSpan::with_location(offset, len, 1, offset as u32),
                    ChartPosition::at_beat(measure, 0),
                ),
            );
        }

        // SCENARIO 1: User edits source at offset 18 (Dm7 chord)
        // Find what rendered element needs updating
        let affected = index.find_by_source_offset(18);
        assert_eq!(affected.len(), 1);
        assert_eq!(affected[0], 3); // Dm7 element

        // Get full info to re-render
        let link = index.get_source_link(3).unwrap();
        assert_eq!(link.position.measure, 2);

        // SCENARIO 2: User clicks on rendered element 2 (Am7)
        // Navigate editor to source location
        let source = index.get_source_link(2).unwrap();
        assert_eq!(source.span.start, 10);
        assert_eq!(source.span.len, 3);
        // Editor would: goto line 1, select columns 10-13

        // SCENARIO 3: User wants to see context around clicked element
        // Find neighboring elements for context highlighting
        let prev_measure_elements = index.find_by_measure(0); // Cmaj7
        let next_measure_elements = index.find_by_measure(2); // Dm7

        assert_eq!(prev_measure_elements.len(), 1);
        assert_eq!(next_measure_elements.len(), 1);
    }
}
