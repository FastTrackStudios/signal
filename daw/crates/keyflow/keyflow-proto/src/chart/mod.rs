//! Chart module - Chart data types and structures

#[allow(clippy::module_inception)]
pub mod chart;
pub mod commands;
pub mod cues;
pub mod display;
pub mod dynamics;
pub mod index;
pub mod measure;
pub mod melody;
pub mod memory;
pub mod notations;
pub mod position;
pub mod rhythm;
pub mod semantic_role;
pub mod settings;
pub mod source_link;
pub mod templates;
pub mod track;
pub mod types;

// Lyric and chord-syllable alignment modules
pub mod chord_syllable_alignment;
pub mod lyric_chord_parser;
pub mod lyrics;
pub mod syllable_parser;

pub use chart::{Chart, ChartClef};
pub use commands::Command;
pub use cues::{InstrumentGroup, TextCue};
pub use dynamics::DynamicMarking;
pub use index::{ChartIndex, ElementId};
pub use measure::{KeyChange, Measure, RhythmSlash, TempoChange, TimeSignatureChange};
pub use melody::{Melody, MelodyNote, MelodyVariables, OctaveModifier};
pub use memory::ChordMemory;
pub use notations::{
    BarlineStyle, Dynamic, DynamicLevel, FiguredBass, FiguredBassRow, Hairpin, HairpinKind,
    Placement, RepeatMark, StaffText, SuspensionFigure, Volta,
};
pub use position::ChartPosition;
pub use rhythm::{BeatStructure, ResolvedRhythm, SectionRhythms, Spillback};
pub use semantic_role::{NavigationType, SemanticRole};
pub use settings::{ChartSetting, ChartSettings, ChartSettingsCheckpoint, SettingValue};
pub use source_link::SourceLink;
pub use templates::TemplateManager;
pub use track::{Track, TrackType};
pub use types::{ChartSection, ChordInstance, Measure as TypesMeasure};

// Lyric and chord-syllable alignment re-exports
pub use chord_syllable_alignment::{
    ChordAttachmentType, ChordSyllableAligner, ChordSyllableMapping, SectionAlignment,
};
pub use lyric_chord_parser::LyricChordParser;
pub use lyrics::{
    ChordAttachment, LyricLine, LyricSegment, LyricSourceFormat, LyricSyllable, LyricSyncLevel,
};
pub use syllable_parser::SyllableParser;
