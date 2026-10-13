//! Additional staff-attached notations.
//!
//! These elements are attached to a measure (and optionally a beat within it)
//! but are not themselves chords, rests, or rhythm slashes. They include:
//!
//! - [`Dynamic`] — classical dynamic markings (mp, mf, f, ff, …)
//! - [`Hairpin`] — crescendo / decrescendo spanners between two beats
//! - [`Volta`] — 1st/2nd ending brackets spanning one or more measures
//! - [`FiguredBass`] — stacked-numeral annotations (e.g. "4-3" over "2-1")
//! - [`StaffText`] — free-form text directions (`*Ac. Gtr. groove`, `==STOP==`)
//! - [`BarlineStyle`] / [`MeasureRepeat`] — barline + repeat metadata
//!
//! All elements live alongside chords on a [`Measure`](super::Measure) and
//! follow the same "above / below staff" placement convention.

use facet::Facet;

// ─────────────────────────────────────────────────────────────────────────────
// Placement
// ─────────────────────────────────────────────────────────────────────────────

/// Where a staff-attached element renders relative to the staff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
#[derive(Default)]
pub enum Placement {
    #[default]
    Above,
    Below,
}

// ─────────────────────────────────────────────────────────────────────────────
// Dynamics
// ─────────────────────────────────────────────────────────────────────────────

/// Classical dynamic level. Each variant has a canonical SMuFL glyph that the
/// engraver renders in the chosen music font (Leland by default).
///
/// For free-form intensity prose (e.g. `"Build"`, `"Go Crazy"`) use
/// [`DynamicMarking`](super::dynamics::DynamicMarking) instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DynamicLevel {
    Ppp,
    Pp,
    P,
    Mp,
    Mf,
    F,
    Ff,
    Fff,
    Sf,
    Sfz,
    Fp,
}

impl DynamicLevel {
    /// SMuFL glyph string for this dynamic (uses the music font's PUA range).
    #[must_use]
    pub fn smufl_glyph(self) -> &'static str {
        match self {
            Self::Ppp => "\u{E52A}",
            Self::Pp => "\u{E52B}",
            Self::P => "\u{E520}",
            Self::Mp => "\u{E52C}",
            Self::Mf => "\u{E52D}",
            Self::F => "\u{E522}",
            Self::Ff => "\u{E52F}",
            Self::Fff => "\u{E530}",
            Self::Sf => "\u{E536}",
            Self::Sfz => "\u{E539}",
            Self::Fp => "\u{E534}",
        }
    }

    /// Plain-text shorthand used in `.kf` source.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ppp => "ppp",
            Self::Pp => "pp",
            Self::P => "p",
            Self::Mp => "mp",
            Self::Mf => "mf",
            Self::F => "f",
            Self::Ff => "ff",
            Self::Fff => "fff",
            Self::Sf => "sf",
            Self::Sfz => "sfz",
            Self::Fp => "fp",
        }
    }
}

/// A dynamic marking attached to a measure at a specific beat.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Dynamic {
    pub level: DynamicLevel,
    /// 1-based beat position within the measure (1 = first beat).
    pub beat: u8,
    pub placement: Placement,
}

// ─────────────────────────────────────────────────────────────────────────────
// Hairpins (crescendo / decrescendo)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum HairpinKind {
    Crescendo,
    Decrescendo,
}

/// A hairpin spans from one beat to another, possibly crossing measures.
/// The start position is tied to the owning measure; `end_measure_offset`
/// counts measures forward (0 = same measure, 1 = next, …) and `end_beat`
/// is a 1-based beat within that measure.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Hairpin {
    pub kind: HairpinKind,
    pub start_beat: u8,
    pub end_measure_offset: u16,
    pub end_beat: u8,
    pub placement: Placement,
}

// ─────────────────────────────────────────────────────────────────────────────
// Voltas (1st / 2nd ending brackets)
// ─────────────────────────────────────────────────────────────────────────────

/// A volta bracket spans one or more measures. Anchored at its starting
/// measure; `length_measures` is inclusive (1 = single-measure ending).
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Volta {
    /// Repeat numbers this ending applies to (1, 2, ...). Typically `[1]`
    /// or `[2]`, occasionally `[1, 3]`.
    pub numbers: Vec<u8>,
    /// Display label (e.g. `"1."`, `"1, 2."`, `"1st Ending"`). When empty,
    /// the engraver formats `numbers` itself.
    pub label: String,
    pub length_measures: u16,
}

// ─────────────────────────────────────────────────────────────────────────────
// Figured bass (stacked numerals)
// ─────────────────────────────────────────────────────────────────────────────

/// One numeral row in a figured-bass stack — e.g. `4-3`, `#4-3`, `2-1`.
/// `accidental` is an optional leading symbol like `"#"`, `"b"`, or `"♮"`.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct FiguredBassRow {
    pub accidental: String,
    pub text: String,
}

/// Stacked-numeral annotation attached to a beat. Rows render top-to-bottom
/// with tight line-spacing, in the chord-symbol text font (MuseJazz).
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct FiguredBass {
    pub rows: Vec<FiguredBassRow>,
    pub beat: u8,
    pub placement: Placement,
    /// MusicXML `words@default-x`, when present. This is a measure-local
    /// horizontal position in tenths from the source engraving.
    pub source_default_x: Option<f64>,
    /// MusicXML `words@relative-x`, when present. This nudges the annotation
    /// horizontally from its beat or source anchor.
    pub source_relative_x: Option<f64>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Suspension figures (e.g. 4-3, 2-3, 3)
// ─────────────────────────────────────────────────────────────────────────────

/// A suspension/resolution figure attached to a beat — e.g. `4-3`, `2-3`,
/// `3`, `2`. Renders as a small superscript next to the chord symbol, or in
/// its own slot when it floats as a continuation of the prior chord (the held
/// chord keeps sounding while the figure marks the suspension/resolution).
///
/// Distinct from [`FiguredBass`]: a single inline figure, not a stacked
/// numeral column, and it tracks the chord rather than the bass line.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct SuspensionFigure {
    /// The figure text exactly as written, e.g. `"4-3"`, `"2-3"`, `"3"`.
    pub figure: String,
    /// 1-based beat position within the measure.
    pub beat: u8,
    /// Above (default) or below the staff.
    pub placement: Placement,
    /// `false` for an attached figure (`Eb2`, `F4-3`) — renders as a small
    /// superscript hugging the upper-right of its chord symbol. `true` for a
    /// floating figure (`Bb // 4-3`, `F 4-3 ///`) — renders as its own symbol
    /// in the chord row at `beat`, the held chord sounding underneath.
    pub standalone: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Free-form staff text
// ─────────────────────────────────────────────────────────────────────────────

/// Free-form text attached to a measure/beat. Distinct from
/// [`TextCue`](super::cues::TextCue) (which is scoped to an instrument group)
/// and from [`Dynamic`] (which renders SMuFL glyphs).
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StaffText {
    pub text: String,
    pub beat: u8,
    pub placement: Placement,
    /// MusicXML `words@default-x`, when present. Measure-local source x in tenths.
    pub source_default_x: Option<f64>,
    /// Optional rectangular enclosure (matches MusicXML `enclosure="rectangle"`).
    pub boxed: bool,
    pub bold: bool,
    pub italic: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Barlines / repeats
// ─────────────────────────────────────────────────────────────────────────────

/// Style of a barline. Maps to MusicXML `<bar-style>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet, Default)]
#[repr(u8)]
pub enum BarlineStyle {
    /// Regular single thin line. Default.
    #[default]
    Normal,
    /// Light then heavy (final / end-of-section barline; also `:|` half).
    LightHeavy,
    /// Heavy then light (start-of-section).
    HeavyLight,
    /// Two thin lines (section break without repeat).
    LightLight,
    /// Double heavy (final final).
    HeavyHeavy,
    /// Dashed barline.
    Dashed,
    /// No barline drawn.
    None,
}

/// Repeat-dot decoration on a barline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet, Default)]
#[repr(u8)]
pub enum RepeatMark {
    #[default]
    None,
    /// `|:` — repeat starts on this barline (forward).
    Forward,
    /// `:|` — repeat ends on this barline (backward).
    Backward,
}
