//! Chord system
//!
//! Defines chords with quality, family, extensions, alterations, and parsing
//!
//! # Module Organization
//!
//! - [`definition`] - Core Chord struct and construction
//! - [`intervals`] - Interval computation and queries (impl blocks for Chord)
//! - [`normalization`] - Chord symbol normalization (impl blocks for Chord)
//! - [`transposition`] - Transposition and key analysis (impl blocks for Chord)
//! - [`quality`], [`family`], [`extensions`] - Chord component types
//! - [`alteration`], [`degree`] - Modification types
//! - [`duration`] - Rhythm notation
//! - [`root`] - Root parsing
//! - [`semitone_sequence`] - Building chords from semitones
//! - [`midi`] - MIDI note detection

pub mod alteration;
pub mod chordpro;
pub mod definition;
pub mod degree;
pub mod detail_level;
pub mod duration;
pub mod error;
pub mod extensions;
pub mod family;
mod intervals; // Internal module - extends Chord with interval methods
pub mod midi;
mod normalization; // Internal module - extends Chord with normalization
pub mod palette;
pub mod quality;
pub mod realize;
pub mod root;
pub mod semitone_sequence;
pub mod timing;
mod transposition; // Internal module - extends Chord with transposition

pub use alteration::Alteration;
pub use chordpro::{
    ChordProChunk, ChordProDirective, ChordProDocument, ChordProLine, ChordProSection,
};
pub use definition::Chord;
pub use degree::ChordDegree;
pub use detail_level::{DetailLevel, UpperStructure};
pub use duration::{ChordRhythm, LilySyntax, PushPullAmount, PushPullBase};
pub use error::{ChordParseError, ChordParseErrors};
pub use extensions::{ExtensionQuality, Extensions};
pub use family::ChordFamily;
pub use midi::{
    DetectedChord, MidiNote, MidiNoteName, detect_chords_from_midi_notes,
    detect_chords_from_midi_notes_with_spelling, midi_pitch_to_note_name,
};
pub use palette::{
    ApproachKind, CHORD_TYPES, ChordCandidate, ChordRole, ChordType, approach, diatonic, grid,
    palette, parallel_key, variations,
};
pub use quality::{ChordQuality, SuspendedType};
pub use realize::{ChordSize, chord_notes, scale_chords, scale_note};
pub use root::NotationSystem;
pub use root::{RootParseResult, parse_root};
pub use semitone_sequence::{
    SemitoneSequenceError, from_semitones, from_semitones_no_inversion, quality_from_semitones,
};
pub use timing::{
    ChordTimingAnalysis, TimingAnalysisConfig, analyze_chord_timing, has_rhythmic_complexity,
    reaper_ppq_to_layout_ticks,
};
