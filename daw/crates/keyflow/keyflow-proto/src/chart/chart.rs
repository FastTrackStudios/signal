//! Main Chart Structure
//!
//! The complete parsed chart with all sections and metadata

use super::melody::MelodyVariables;
use super::memory::ChordMemory;
use super::settings::ChartSettings;
use super::templates::TemplateManager;
use super::track::TrackType;
use super::types::{ChartSection, ChordInstance, KeyChange, TempoChange, TimeSignatureChange};
use crate::key::Key;
use crate::metadata::SongMetadata;
use crate::primitives::Note;
use crate::sections::{Section, SectionType};
use crate::time::{AbsolutePosition, MusicalPosition, Tempo, TimeSignature};
use facet::Facet;
use std::collections::HashMap;

/// Top-level clef choice for a chart. The engraver maps each variant to its
/// own ClefType — proto-side this is kept small (no glyph data) so the
/// keyflow workspace can stay engraver-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet, Default)]
#[repr(u8)]
pub enum ChartClef {
    #[default]
    Treble,
    Bass,
    Alto,
    Tenor,
    Percussion,
}

/// The complete parsed chart structure
#[derive(Clone, PartialEq, Facet)]
pub struct Chart {
    /// Song metadata (title, artist, etc.)
    pub metadata: SongMetadata,

    /// All sections in the chart
    pub sections: Vec<ChartSection>,

    /// Current key (last known key during parsing)
    pub current_key: Option<Key>,

    /// Initial key at the start of the song
    pub initial_key: Option<Key>,

    /// Ending key at the end of the song
    pub ending_key: Option<Key>,

    /// All key changes throughout the song
    pub key_changes: Vec<KeyChange>,

    /// Tempo in BPM
    pub tempo: Option<Tempo>,

    /// Current time signature (last known during parsing)
    pub time_signature: Option<TimeSignature>,

    /// Initial time signature at the start
    pub initial_time_signature: Option<TimeSignature>,

    /// Initial clef. `None` means the renderer picks its default (typically
    /// treble for lead-sheet charts).
    pub initial_clef: Option<ChartClef>,

    /// All time signature changes throughout the song
    pub time_signature_changes: Vec<TimeSignatureChange>,

    /// All tempo changes throughout the song
    pub tempo_changes: Vec<TempoChange>,

    /// Chord memory manager (used by text parsing and DAW integration)
    pub chord_memory: ChordMemory,

    /// Template manager (used by text parsing)
    pub templates: TemplateManager,

    /// Chart configuration settings
    pub settings: ChartSettings,

    /// Melody variables (named melody patterns)
    pub melody_variables: MelodyVariables,

    /// Section measure memory (for expression evaluation)
    /// Tracks the last used measure count for each section type
    pub section_measure_memory: HashMap<SectionType, usize>,

    /// Default push/pull amount for the entire chart.
    /// If set, pushed chords matching this amount are displayed as just `'`
    /// instead of `'_8t`, etc. This is detected from the most common push type.
    pub default_push_amount: Option<crate::chord::PushPullAmount>,
}

impl Chart {
    /// Create a new empty chart
    pub fn new() -> Self {
        Self {
            metadata: SongMetadata::new(),
            sections: Vec::new(),
            current_key: None,
            initial_key: None,
            ending_key: None,
            key_changes: Vec::new(),
            tempo: None,
            time_signature: None,
            initial_time_signature: None,
            initial_clef: None,
            time_signature_changes: Vec::new(),
            tempo_changes: Vec::new(),
            chord_memory: ChordMemory::new(),
            templates: TemplateManager::new(),
            settings: ChartSettings::new(),
            melody_variables: MelodyVariables::new(),
            section_measure_memory: HashMap::new(),
            default_push_amount: None,
        }
    }

    /// Register a key change at the current position
    pub fn add_key_change(
        &mut self,
        new_key: Key,
        position: AbsolutePosition,
        section_index: usize,
    ) {
        let from_key = self.current_key.clone();
        let key_change = KeyChange::new(position, from_key, new_key.clone(), section_index);

        self.key_changes.push(key_change);
        self.current_key = Some(new_key);
    }

    /// Register a time signature change at the current position
    pub fn add_time_signature_change(
        &mut self,
        new_time_sig: TimeSignature,
        position: AbsolutePosition,
        section_index: usize,
    ) {
        let change = TimeSignatureChange::new(position, new_time_sig, section_index);

        self.time_signature_changes.push(change);
        self.time_signature = Some(new_time_sig);
    }

    fn format_measure_inline_content(&self, measure: &super::types::Measure) -> String {
        let chord_parts: Vec<String> = measure
            .chords
            .iter()
            .filter_map(|chord| {
                let chord_str = self.format_chord_for_syntax(chord, &measure.time_signature);
                (!chord_str.is_empty()).then_some(chord_str)
            })
            .collect();

        let melody_parts: Vec<String> = measure
            .melodies
            .iter()
            .map(|melody| {
                if let Some(name) = &melody.name
                    && self.melody_variables.get(name).is_some()
                {
                    format!("${}", name)
                } else {
                    format!("{}", melody)
                }
            })
            .collect();

        match (chord_parts.is_empty(), melody_parts.is_empty()) {
            (false, false) => format!(
                "<< {} ; {} >>",
                chord_parts.join(" "),
                melody_parts.join(" ; ")
            ),
            (false, true) => chord_parts.join(" "),
            (true, false) => melody_parts.join(" "),
            (true, true) => String::new(),
        }
    }

    /// Get the active key at a specific position
    pub fn key_at_position(&self, position: &AbsolutePosition) -> Option<&Key> {
        // Start with initial key
        let mut current_key = self.initial_key.as_ref();

        // Apply key changes up to this position
        for change in &self.key_changes {
            if change.position.total_duration.measure <= position.total_duration.measure {
                current_key = Some(&change.to_key);
            } else {
                break;
            }
        }

        current_key
    }

    /// Get the active time signature at a specific position
    ///
    /// Time signature changes take effect at the START of the measure they're marked at.
    /// So if checking position M.X.Y where M equals the change position measure,
    /// the change has already taken effect.
    pub fn time_signature_at_position(
        &self,
        position: &AbsolutePosition,
    ) -> Option<&TimeSignature> {
        // Start with initial time signature
        let mut current_ts = self.initial_time_signature.as_ref();

        // Apply time signature changes up to (and including) this position
        for change in &self.time_signature_changes {
            if change.position.total_duration.measure <= position.total_duration.measure {
                current_ts = Some(&change.time_signature);
            } else {
                break;
            }
        }

        current_ts
    }

    /// Calculate duration from start of song to a given section and measure
    pub fn calculate_position(
        &self,
        section_index: usize,
        measure_index: usize,
    ) -> AbsolutePosition {
        let mut total_duration = MusicalPosition::ZERO;

        // Sum durations of all sections before this one
        for (idx, section) in self.sections.iter().enumerate() {
            if idx >= section_index {
                break;
            }

            // Sum all measures in this section (each measure is always 1.0.0)
            for _measure in section.measures() {
                // Each measure adds exactly 1.0.0 to the total duration
                total_duration = MusicalPosition::new(
                    total_duration.measure + 1,
                    total_duration.beat,
                    total_duration.subdivision,
                );
            }
        }

        // Add measures in current section up to measure_index (each is 1.0.0)
        if let Some(section) = self.sections.get(section_index) {
            for _idx in 0..measure_index.min(section.measures().len()) {
                // Each measure adds exactly 1.0.0 to the total duration
                total_duration = MusicalPosition::new(
                    total_duration.measure + 1,
                    total_duration.beat,
                    total_duration.subdivision,
                );
            }
        }

        AbsolutePosition::new(total_duration, section_index)
    }

    /// Generate rhythm slashes for all measures in the chart.
    ///
    /// This fills in beats that don't have explicit chords with slash noteheads,
    /// which is standard lead sheet notation. Call this after parsing or before rendering.
    pub fn generate_rhythm_slashes(&mut self) {
        let mut current_measure: i32 = 0;

        for (section_idx, section) in self.sections.iter_mut().enumerate() {
            for measure in section.measures_mut() {
                measure.generate_rhythm_slashes(current_measure, section_idx);
                current_measure += 1;
            }
        }
    }

    /// Convert the chart back to text syntax
    ///
    /// This produces a valid syntax string that, when parsed, will result in the same chart structure.
    /// The output may not match the original formatting exactly, but it will be functionally equivalent.
    pub fn to_syntax(&self) -> String {
        let mut output = String::new();

        // 1. Title and Artist
        if let Some(title) = &self.metadata.title {
            output.push_str(title);
            if let Some(artist) = &self.metadata.artist {
                output.push_str(" - ");
                output.push_str(artist);
            }
            output.push('\n');
        }

        // 2. Tempo, Time Signature, Key
        let mut metadata_parts = Vec::new();

        if let Some(tempo) = &self.tempo {
            metadata_parts.push(format!("{}bpm", tempo.bpm as u32));
        }

        // Use initial_time_signature if available, otherwise fall back to current time_signature
        let ts = self
            .initial_time_signature
            .as_ref()
            .or(self.time_signature.as_ref());
        if let Some(ts) = ts {
            metadata_parts.push(format!("{}/{}", ts.numerator, ts.denominator));
        }

        if let Some(key) = &self.initial_key {
            // Format key as #G, bBb, or #C (always use # prefix for major keys)
            let key_str = self.format_key_for_syntax(key);
            metadata_parts.push(key_str);
        }

        if !metadata_parts.is_empty() {
            output.push_str(&metadata_parts.join(" "));
            output.push('\n');
        }

        // 3. Settings
        if self.settings.smart_repeats() {
            output.push_str("/SMART_REPEATS=true\n");
        }

        // 4. Sections (melody variables are output at the start of the first section)
        let mut melody_vars_output = false;
        for (section_idx, section) in self.sections.iter().enumerate() {
            // Empty line before section (except first)
            if section_idx > 0 {
                output.push('\n');
            }

            // Section header
            let section_name = self.format_section_name(&section.section);
            output.push_str(&section_name);
            output.push('\n');

            // Output melody variable definitions at the start of the first section
            if !melody_vars_output {
                for (name, melody) in self.melody_variables.iter() {
                    // Output just the notes part, since the name is stored separately
                    let notes_str: Vec<String> =
                        melody.notes.iter().map(|n| n.to_string()).collect();
                    output.push_str(&format!("{} = m{{ {} }}\n", name, notes_str.join(" ")));
                }
                melody_vars_output = true;
            }

            // Output tracks
            let has_multiple_tracks = section.tracks.len() > 1;

            for track in &section.tracks {
                // Output track marker if this section has multiple tracks or track has a name
                let needs_marker =
                    has_multiple_tracks || track.name.is_some() || !track.is_default_chord_track();

                if needs_marker {
                    let marker = track.track_type.marker();
                    if let Some(name) = &track.name {
                        output.push_str(&format!("[{} {}] ", marker, name));
                    } else {
                        output.push_str(&format!("[{}] ", marker));
                    }
                }

                match track.track_type {
                    TrackType::Melody => {
                        // Output melody content
                        if let Some(melody) = &track.melody {
                            let notes_str: Vec<String> =
                                melody.notes.iter().map(|n| n.to_string()).collect();
                            output.push_str(&format!("m{{ {} }}\n", notes_str.join(" ")));
                        }
                    }
                    TrackType::Chords | TrackType::Rhythm => {
                        // Output chord/rhythm measures
                        if needs_marker {
                            output.push('\n');
                        }

                        let mut measure_idx = 0;
                        while measure_idx < track.measures.len() {
                            // Look ahead to find if there's a repeat pattern starting here
                            let pattern_end =
                                self.find_repeat_pattern_end_in_track(&track.measures, measure_idx);

                            if let Some((pattern_end_idx, repeat_count)) = pattern_end {
                                // Output the entire pattern on one line
                                self.output_measure_group_from_track(
                                    &mut output,
                                    &track.measures,
                                    section_idx,
                                    measure_idx,
                                    pattern_end_idx,
                                    repeat_count,
                                );

                                // Skip past the pattern and its duplicates
                                let pattern_len = pattern_end_idx - measure_idx + 1;
                                let total_measures = pattern_len * repeat_count;
                                measure_idx += total_measures;
                            } else {
                                // Single measure, no repeat
                                self.output_single_measure_from_track(
                                    &mut output,
                                    &track.measures,
                                    section_idx,
                                    measure_idx,
                                );
                                measure_idx += 1;
                            }
                        }
                    }
                    TrackType::Lyrics => {
                        if let Some(lyric_line) = &track.lyrics {
                            if lyric_line.has_chords() {
                                output.push_str(&lyric_line.to_chord_text());
                            } else {
                                output.push_str(&lyric_line.full_text());
                            }
                            output.push('\n');
                        }
                    }
                }
            }
        }

        output
    }

    /// Find if there's a repeat pattern starting at the given index in a track's measures
    fn find_repeat_pattern_end_in_track(
        &self,
        measures: &[super::types::Measure],
        start_idx: usize,
    ) -> Option<(usize, usize)> {
        for idx in start_idx..measures.len() {
            let measure = &measures[idx];
            if measure.repeat_count > 1 {
                return Some((idx, measure.repeat_count));
            }
            if idx > start_idx {
                let start_chords: Vec<_> = measures[start_idx]
                    .chords
                    .iter()
                    .map(|c| c.root.clone())
                    .collect();
                let current_chords: Vec<_> =
                    measure.chords.iter().map(|c| c.root.clone()).collect();
                if start_chords == current_chords && idx > start_idx {
                    break;
                }
            }
        }
        None
    }

    /// Output a group of measures from a track on a single line with repeat syntax
    fn output_measure_group_from_track(
        &self,
        output: &mut String,
        measures: &[super::types::Measure],
        section_idx: usize,
        start_idx: usize,
        end_idx: usize,
        repeat_count: usize,
    ) {
        // Collect text cues and dynamics from all measures in the pattern
        for measure in measures.iter().take(end_idx + 1).skip(start_idx) {
            for cue in &measure.text_cues {
                output.push_str(&format!("{}\n", cue));
            }
            for dynamic in &measure.dynamics {
                output.push_str(&format!("{}\n", dynamic));
            }
        }

        // Output chords from each measure in the pattern
        let mut all_chords = Vec::new();
        for (idx, measure) in measures
            .iter()
            .enumerate()
            .take(end_idx + 1)
            .skip(start_idx)
        {
            // Check for key/time sig changes
            let position = self.calculate_position(section_idx, idx);
            for key_change in &self.key_changes {
                if key_change.position.total_duration.measure == position.total_duration.measure
                    && key_change.section_index == section_idx
                {
                    let key_str = self.format_key_for_syntax(&key_change.to_key);
                    all_chords.push(key_str);
                }
            }
            for ts_change in &self.time_signature_changes {
                if ts_change.position.total_duration.measure == position.total_duration.measure
                    && ts_change.section_index == section_idx
                {
                    all_chords.push(format!(
                        "{}/{}",
                        ts_change.time_signature.numerator, ts_change.time_signature.denominator
                    ));
                }
            }

            let measure_content = self.format_measure_inline_content(measure);
            if !measure_content.is_empty() {
                all_chords.push(measure_content);
            }
        }

        if !all_chords.is_empty() {
            output.push_str(&all_chords.join(" "));
            output.push_str(&format!(" x{}", repeat_count));
        }
        output.push('\n');
    }

    /// Output a single measure from a track (no repeat pattern)
    fn output_single_measure_from_track(
        &self,
        output: &mut String,
        measures: &[super::types::Measure],
        section_idx: usize,
        measure_idx: usize,
    ) {
        let measure = &measures[measure_idx];

        // Text cues before this measure
        for cue in &measure.text_cues {
            output.push_str(&format!("{}\n", cue));
        }

        // Dynamic markings before this measure
        for dynamic in &measure.dynamics {
            output.push_str(&format!("{}\n", dynamic));
        }

        // Check for key change at this position
        let position = self.calculate_position(section_idx, measure_idx);
        for key_change in &self.key_changes {
            if key_change.position.total_duration.measure == position.total_duration.measure
                && key_change.section_index == section_idx
            {
                let key_str = self.format_key_for_syntax(&key_change.to_key);
                output.push_str(&key_str);
                output.push(' ');
            }
        }

        // Check for time signature change at this position
        for ts_change in &self.time_signature_changes {
            if ts_change.position.total_duration.measure == position.total_duration.measure
                && ts_change.section_index == section_idx
            {
                output.push_str(&format!(
                    "{}/{} ",
                    ts_change.time_signature.numerator, ts_change.time_signature.denominator
                ));
            }
        }

        // Check for tempo change at this position
        for tempo_change in &self.tempo_changes {
            if tempo_change.position.total_duration.measure == position.total_duration.measure
                && tempo_change.section_index == section_idx
            {
                output.push_str(&format!("->{}bpm ", tempo_change.to_tempo.bpm as u32));
            }
        }

        let measure_content = self.format_measure_inline_content(measure);
        if !measure_content.is_empty() {
            output.push_str(&measure_content);

            // Add measure separator at end of measure (except last in track)
            if measure_idx < measures.len() - 1 {
                output.push_str(" |");
            }
        } else {
            // Empty measure - just add separator if not last
            if measure_idx < measures.len() - 1 {
                output.push('|');
            }
        }

        output.push('\n');
    }

    /// Format a key for syntax output (#G, bBb, or #C)
    /// Always use # prefix for major keys to match parser expectations
    fn format_key_for_syntax(&self, key: &Key) -> String {
        let root = key.root();
        let note_name = root.name();

        // Remove any existing # or b from note name
        let clean_note = note_name.trim_start_matches('#').trim_start_matches('b');

        // For major keys (Ionian), use # prefix
        // For minor keys, we'd use b prefix, but for now all keys are major
        if note_name.contains('b') {
            format!("b{}", clean_note)
        } else {
            // Use # prefix for all major keys (matches parser expectation)
            format!("#{}", clean_note)
        }
    }

    /// Format a section name for syntax output
    fn format_section_name(&self, section: &Section) -> String {
        let mut name = String::new();

        // Subsection prefix
        if section.is_subsection {
            name.push('^');
        }

        // Section type
        match &section.section_type {
            crate::sections::SectionType::Custom(custom_name) => {
                name.push('[');
                name.push_str(custom_name);
                name.push(']');
            }
            _ => {
                name.push_str(&section.section_type.abbreviation());
            }
        }

        // Note: Section numbers are NOT serialized - they are auto-generated during parsing
        // based on section order. Outputting them would cause the parser to fail since it
        // doesn't expect section numbers in the syntax (e.g., "VS 1 8" would fail, only "VS 8" works)

        // Split letter
        if let Some(letter) = section.split_letter {
            name.push(letter);
        }

        // Measure count
        if let Some(count) = section.measure_count {
            name.push(' ');
            name.push_str(&count.to_string());
        }

        name
    }

    /// Format a push/pull amount for syntax output
    pub(crate) fn format_push_pull_amount(amount: &crate::chord::PushPullAmount) -> String {
        use crate::chord::PushPullBase;

        // For duration-based push/pull, output the '_N' syntax
        if let PushPullBase::Duration {
            duration,
            dotted,
            triplet,
        } = &amount.base
        {
            let dot_suffix = if *dotted { "." } else { "" };
            let triplet_suffix = if *triplet { "t" } else { "" };
            return format!("'_{}{}{}", duration.value(), dot_suffix, triplet_suffix);
        }

        // Generate apostrophes based on level
        let apostrophes: String = "'".repeat(amount.level as usize);

        // Add base modifier if not standard
        let modifier = match &amount.base {
            PushPullBase::Standard => String::new(),
            PushPullBase::Triplet => "t".to_string(),
            PushPullBase::Tuplet(n) => format!(":{}", n),
            PushPullBase::Duration { .. } => unreachable!(), // Already handled above
        };

        format!("{}{}", apostrophes, modifier)
    }

    /// Format a chord for syntax output
    fn format_chord_for_syntax(&self, chord: &ChordInstance, _time_sig: &(u8, u8)) -> String {
        let mut output = String::new();

        // Push notation (leading apostrophes with optional triplet/tuplet)
        if let Some((is_push, amount)) = &chord.push_pull
            && *is_push
        {
            output.push_str(&Self::format_push_pull_amount(amount));
        }

        // Chord symbol (use full_symbol which preserves the original format)
        output.push_str(&chord.full_symbol);

        // Rhythm notation
        output.push_str(&self.format_rhythm_for_syntax(&chord.rhythm));

        // Pull notation (trailing apostrophes with optional triplet/tuplet)
        if let Some((is_push, amount)) = &chord.push_pull
            && !is_push
        {
            output.push_str(&Self::format_push_pull_amount(amount));
        }

        // Commands
        for command in &chord.commands {
            match command {
                super::commands::Command::Fermata => {
                    output.push_str(" /fermata");
                }
                super::commands::Command::Accent | super::commands::Command::AccentOnPush => {
                    // Both accent types output the same inline syntax
                    // The distinction is in parsing order (>'C vs '>C)
                    output.push_str("->");
                }
                super::commands::Command::Staccato => {
                    // Staccato is encoded as '.' prefix in the token, not inline
                }
                super::commands::Command::Stop
                | super::commands::Command::StopAfter
                | super::commands::Command::StopGroove
                | super::commands::Command::StopGrooveAfter => {
                    // Stop commands are standalone tokens (!STOP), not inline
                }
            }
        }

        output
    }

    /// Format rhythm notation for syntax output
    fn format_rhythm_for_syntax(&self, rhythm: &crate::chord::ChordRhythm) -> String {
        use crate::chord::ChordRhythm;
        match rhythm {
            ChordRhythm::Default => String::new(),
            ChordRhythm::Slashes {
                count,
                dotted,
                tied,
            } => {
                let mut output = "/".repeat(*count as usize);
                if *dotted {
                    output.push('.');
                }
                if *tied {
                    output.push('~');
                }
                output
            }
            ChordRhythm::Explicit(nd) => {
                use crate::core::duration::RhythmType;

                // Determine prefix based on rhythm type
                let prefix = match nd.rhythm_type {
                    RhythmType::Chord => "_",
                    RhythmType::Rest => "r",
                    RhythmType::Space => "s",
                    RhythmType::Slashes(count) => return "/".repeat(count as usize),
                };

                // Format the note value
                let value_str = match nd.note_value {
                    crate::core::duration::NoteValue::Whole => "1",
                    crate::core::duration::NoteValue::Half => "2",
                    crate::core::duration::NoteValue::Quarter => "4",
                    crate::core::duration::NoteValue::Eighth => "8",
                    crate::core::duration::NoteValue::Sixteenth => "16",
                    crate::core::duration::NoteValue::ThirtySecond => "32",
                    crate::core::duration::NoteValue::SixtyFourth => "64",
                };

                let mut output = format!("{}{}", prefix, value_str);

                // Add triplet suffix if applicable
                if nd.is_triplet() {
                    output.push('t');
                }

                // Add dots
                for _ in 0..nd.dots {
                    output.push('.');
                }

                // Add multiplier
                if let Some(mult) = nd.multiplier {
                    output.push_str(&format!("*{}", mult));
                }

                // Add tie
                if nd.tied {
                    output.push('~');
                }

                output
            }
        }
    }
}

impl Default for Chart {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{ChartSection, Measure};
    use super::*;
    use crate::primitives::MusicalNote;
    use crate::sections::{Section, SectionType};

    #[test]
    fn test_position_calculation_with_key_and_time_signature_changes() {
        let mut chart = Chart::new();

        // Setup: C major, 4/4 time
        let c_major = Key::major(MusicalNote::c());
        let time_4_4 = TimeSignature::new(4, 4);

        chart.initial_key = Some(c_major.clone());
        chart.current_key = Some(c_major.clone());
        chart.initial_time_signature = Some(time_4_4);
        chart.time_signature = Some(time_4_4);

        // Section 0: Intro - 2 measures in C major, 4/4
        let intro = Section::new(SectionType::Intro).with_measure_count(2);
        let intro_measures = vec![
            Measure::new().with_time_signature((4, 4)),
            Measure::new().with_time_signature((4, 4)),
        ];
        chart
            .sections
            .push(ChartSection::new(intro).with_measures(intro_measures));

        // Section 1: Verse - 4 measures, key changes to G major after 2 measures
        let verse = Section::new(SectionType::Verse).with_measure_count(4);
        let verse_measures = vec![
            Measure::new().with_time_signature((4, 4)), // Measure 0 in verse
            Measure::new().with_time_signature((4, 4)), // Measure 1 in verse
            Measure::new().with_time_signature((4, 4)), // Measure 2 (key change to G here)
            Measure::new().with_time_signature((4, 4)), // Measure 3
        ];
        chart
            .sections
            .push(ChartSection::new(verse).with_measures(verse_measures));

        // Add key change at measure 4 (start of measure 2 in verse = section 1, measure 2)
        let g_major = Key::major(MusicalNote::g());
        let key_change_position = AbsolutePosition::new(MusicalPosition::new(4, 0, 0), 1);
        chart.add_key_change(g_major.clone(), key_change_position.clone(), 1);

        // Section 2: Chorus - 4 measures in 6/8 time
        let chorus = Section::new(SectionType::Chorus).with_measure_count(4);
        let chorus_measures = vec![
            Measure::new().with_time_signature((6, 8)),
            Measure::new().with_time_signature((6, 8)),
            Measure::new().with_time_signature((6, 8)),
            Measure::new().with_time_signature((6, 8)),
        ];
        chart
            .sections
            .push(ChartSection::new(chorus).with_measures(chorus_measures));

        // Add time signature change at measure 6 (start of chorus = section 2, measure 0)
        let time_6_8 = TimeSignature::new(6, 8);
        let ts_change_position = AbsolutePosition::new(MusicalPosition::new(6, 0, 0), 2);
        chart.add_time_signature_change(time_6_8, ts_change_position.clone(), 2);

        // Test 1: Position at start of song (section 0, measure 0)
        let pos_start = chart.calculate_position(0, 0);
        assert_eq!(pos_start.total_duration.measure, 0);
        assert_eq!(pos_start.section_index, 0);

        // Test 2: Position at end of intro (section 0, after 2 measures)
        let pos_end_intro = chart.calculate_position(0, 2);
        assert_eq!(pos_end_intro.total_duration.measure, 2);
        assert_eq!(pos_end_intro.section_index, 0);

        // Test 3: Position at start of verse (section 1, measure 0)
        let pos_start_verse = chart.calculate_position(1, 0);
        assert_eq!(pos_start_verse.total_duration.measure, 2);
        assert_eq!(pos_start_verse.section_index, 1);

        // Test 4: Position where key changes (section 1, measure 2)
        let pos_key_change = chart.calculate_position(1, 2);
        assert_eq!(pos_key_change.total_duration.measure, 4);
        assert_eq!(pos_key_change.section_index, 1);

        // Test 5: Position at end of verse (section 1, after 4 measures)
        let pos_end_verse = chart.calculate_position(1, 4);
        assert_eq!(pos_end_verse.total_duration.measure, 6);
        assert_eq!(pos_end_verse.section_index, 1);

        // Test 6: Position at start of chorus / time sig change (section 2, measure 0)
        let pos_start_chorus = chart.calculate_position(2, 0);
        assert_eq!(pos_start_chorus.total_duration.measure, 6);
        assert_eq!(pos_start_chorus.section_index, 2);

        // Test 7: Position in middle of chorus (section 2, measure 2)
        let pos_mid_chorus = chart.calculate_position(2, 2);
        assert_eq!(pos_mid_chorus.total_duration.measure, 8);
        assert_eq!(pos_mid_chorus.section_index, 2);

        // Test 8: Verify key_at_position works correctly
        let key_at_start = chart.key_at_position(&pos_start);
        assert_eq!(key_at_start, Some(&c_major));

        let key_before_change =
            chart.key_at_position(&AbsolutePosition::new(MusicalPosition::new(3, 0, 0), 1));
        assert_eq!(key_before_change, Some(&c_major));

        let key_after_change = chart.key_at_position(&pos_key_change);
        assert_eq!(key_after_change, Some(&g_major));

        let key_in_chorus = chart.key_at_position(&pos_mid_chorus);
        assert_eq!(key_in_chorus, Some(&g_major));

        // Test 9: Verify time_signature_at_position works correctly
        let ts_at_start = chart.time_signature_at_position(&pos_start);
        assert_eq!(ts_at_start, Some(&time_4_4));

        // Position 6 is exactly where chorus starts - the change happens AT this position
        // So at position 6, we should already be in 6/8
        let ts_at_change_point = chart.time_signature_at_position(&pos_start_chorus);
        assert_eq!(ts_at_change_point, Some(&time_6_8));

        // Just before the change (end of verse at position 6) - still in 4/4
        // Note: pos_end_verse is at 6.0.0, which is the same as pos_start_chorus!
        // So we need a position just before that
        let pos_before_change = AbsolutePosition::new(MusicalPosition::new(5, 0, 0), 1);
        let ts_before_change = chart.time_signature_at_position(&pos_before_change);
        assert_eq!(ts_before_change, Some(&time_4_4));

        let ts_in_chorus = chart.time_signature_at_position(&pos_mid_chorus);
        assert_eq!(ts_in_chorus, Some(&time_6_8));
    }

    #[test]
    fn test_multiple_key_changes() {
        let mut chart = Chart::new();

        let c_major = Key::major(MusicalNote::c());
        let g_major = Key::major(MusicalNote::g());
        let d_major = Key::major(MusicalNote::d());

        chart.initial_key = Some(c_major.clone());
        chart.current_key = Some(c_major.clone());

        // Add sections with measures
        let section1 = Section::new(SectionType::Verse);
        let measures1 = vec![Measure::new(), Measure::new()];
        chart
            .sections
            .push(ChartSection::new(section1).with_measures(measures1));

        let section2 = Section::new(SectionType::Chorus);
        let measures2 = vec![Measure::new(), Measure::new()];
        chart
            .sections
            .push(ChartSection::new(section2).with_measures(measures2));

        let section3 = Section::new(SectionType::Bridge);
        let measures3 = vec![Measure::new(), Measure::new()];
        chart
            .sections
            .push(ChartSection::new(section3).with_measures(measures3));

        // Key change at measure 2 (start of section 1)
        chart.add_key_change(
            g_major.clone(),
            AbsolutePosition::new(MusicalPosition::new(2, 0, 0), 1),
            1,
        );

        // Key change at measure 4 (start of section 2)
        chart.add_key_change(
            d_major.clone(),
            AbsolutePosition::new(MusicalPosition::new(4, 0, 0), 2),
            2,
        );

        // Test that we have 2 key changes recorded
        assert_eq!(chart.key_changes.len(), 2);

        // Test key at different positions
        let pos_0 = AbsolutePosition::new(MusicalPosition::new(0, 0, 0), 0);
        assert_eq!(chart.key_at_position(&pos_0), Some(&c_major));

        let pos_1 = AbsolutePosition::new(MusicalPosition::new(1, 0, 0), 0);
        assert_eq!(chart.key_at_position(&pos_1), Some(&c_major));

        let pos_2 = AbsolutePosition::new(MusicalPosition::new(2, 0, 0), 1);
        assert_eq!(chart.key_at_position(&pos_2), Some(&g_major));

        let pos_3 = AbsolutePosition::new(MusicalPosition::new(3, 0, 0), 1);
        assert_eq!(chart.key_at_position(&pos_3), Some(&g_major));

        let pos_4 = AbsolutePosition::new(MusicalPosition::new(4, 0, 0), 2);
        assert_eq!(chart.key_at_position(&pos_4), Some(&d_major));

        let pos_5 = AbsolutePosition::new(MusicalPosition::new(5, 0, 0), 2);
        assert_eq!(chart.key_at_position(&pos_5), Some(&d_major));
    }

    #[test]
    fn test_ending_key_tracking() {
        let mut chart = Chart::new();

        let c_major = Key::major(MusicalNote::c());
        let f_major = Key::major(MusicalNote::f());

        chart.initial_key = Some(c_major.clone());
        chart.current_key = Some(c_major.clone());

        // Add a section
        let section = Section::new(SectionType::Verse);
        chart
            .sections
            .push(ChartSection::new(section).with_measures(vec![Measure::new()]));

        // Add key change
        chart.add_key_change(
            f_major.clone(),
            AbsolutePosition::new(MusicalPosition::new(1, 0, 0), 0),
            0,
        );

        // Verify current_key was updated
        assert_eq!(chart.current_key, Some(f_major));
    }
}
