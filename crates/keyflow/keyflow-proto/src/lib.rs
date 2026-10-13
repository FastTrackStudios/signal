//! Keyflow Protocol - Shared types and service definitions for Keyflow cells
//!
//! This crate defines the data types and service interfaces for music chart operations.
//! All types are Facet-derived for RPC compatibility.

#![deny(unsafe_code)]

// Domain modules
pub mod api;
pub mod chart;
pub mod chord;
pub mod core;
pub mod guide;
pub mod key;
pub mod lrc;
pub mod metadata;
pub mod primitives;
pub mod sections;
pub mod services;
pub mod time;
pub mod transpose;

pub(crate) mod ast {}

pub(crate) mod parsing {
    pub(crate) use keyflow_syntax::parsing::*;

    pub(crate) mod token {
        pub(crate) use keyflow_syntax::parsing::token::*;
    }
}

// Multi-block document support
#[cfg(feature = "serde")]
pub mod document;

// Re-export common types for convenience
pub use api::prelude as api_prelude;
pub use chart::{
    Chart, ChartIndex, ChartPosition, ChartSection, ChordAttachment, ChordAttachmentType,
    ChordInstance, ChordSyllableAligner, ChordSyllableMapping, DynamicMarking, ElementId,
    KeyChange, LyricChordParser, LyricLine, LyricSegment, LyricSourceFormat, LyricSyllable,
    LyricSyncLevel, Measure, NavigationType, SectionAlignment, SemanticRole, SourceLink,
    SyllableParser, TempoChange, TextCue, TimeSignatureChange,
};

#[cfg(feature = "serde")]
pub use document::{KfBlock, KfBlockKind, KfDocument};

pub use chord::{
    Alteration, Chord, ChordDegree, ChordFamily, ChordParseError, ChordParseErrors, ChordQuality,
    ChordRhythm, DetailLevel, ExtensionQuality, Extensions, LilySyntax, PushPullAmount,
    RootParseResult, SuspendedType, UpperStructure, parse_root,
};

pub use key::{Key, ScaleMode, ScaleType};

// Non-destructive chart display transform (transpose / re-notate / capo).
pub use transpose::{ChartView, NotationSystem, apply_view, renotate_symbol, transpose_source};

pub use metadata::SongMetadata;

pub use keyflow_syntax::parsing::{Lexer, ParseError, TextSpan, Token, TokenType};

pub use primitives::{
    Interval, MusicalNote, MusicalNoteToken, Note, RomanCase, RomanNumeralToken, RootFormat,
    RootNotation, ScaleDegreeToken,
};

pub use guide::{
    ClickConfig, ClickEvent, ClickType, CountEvent, CountInConfig, CountInState, GuideConfig,
    GuideEvent, SectionCueEvent,
};

pub use sections::{Section, SectionNumberer, SectionType};

pub use time::{
    AbsolutePosition, Duration, MusicalDuration, MusicalPosition, PPQDuration, PPQPosition,
    Position, Tempo, TimeDuration, TimePosition, TimeSignature,
};

// RPC contracts + request/response payloads at the crate root for
// `use keyflow_proto::{Charts, GuideRequest, ...}`-style imports.
// Per-contract module aliases (`services::<svc>::Dispatcher`,
// `<svc>::descriptor`, `<svc>::serve`, `<svc>::layer`) live under
// `services::*` — touch those at the mounting site only.
pub use services::{
    ChartParsers, Charts, GuideRequest, Guides, ParseRequest, ParseResponse, Parsers,
};
#[cfg(feature = "vox")]
pub use services::{ChartParsersClient, ChartsClient, GuidesClient, ParsersClient};
