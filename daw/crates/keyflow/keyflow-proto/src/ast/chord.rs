//! Chord AST types.
//!
//! Intermediate representation for chord notation that preserves source spans
//! and defers semantic analysis like interval computation.

use super::RhythmAst;
use super::span::{AstNode, Spanned};
use crate::parsing::TextSpan;
use facet::Facet;

/// Abstract syntax tree for a chord.
///
/// Captures the syntactic structure of a chord symbol without computing
/// intervals or performing semantic validation. This allows:
///
/// - Better error messages with precise source locations
/// - Deferred semantic analysis
/// - Testing of parsing independent from semantic rules
///
/// # Example
///
/// For input "Cmaj7/G":
/// - root: RootAst { letter: 'C', accidental: None }
/// - quality: Some(QualityAst::Major)
/// - extension: Some(ExtensionAst::Seventh { quality: Major })
/// - bass: Some(BassToneAst { letter: 'G', accidental: None })
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ChordAst {
    /// The root of the chord (note letter, scale degree, or roman numeral)
    pub root: Spanned<RootAst>,

    /// Optional quality modifier (m, maj, dim, aug, sus, etc.)
    pub quality: Option<Spanned<QualityAst>>,

    /// Extension (7, 9, 11, 13 with optional quality)
    pub extension: Option<Spanned<ExtensionAst>>,

    /// Additional modifiers (alterations, additions, omissions)
    pub modifiers: Vec<Spanned<ChordModifierAst>>,

    /// Slash bass note
    pub bass: Option<Spanned<BassToneAst>>,

    /// Rhythm/duration notation
    pub rhythm: Option<Spanned<RhythmAst>>,

    /// Full span covering the entire chord
    pub span: TextSpan,
}

impl ChordAst {
    /// Create a new chord AST with just a root.
    #[must_use]
    pub fn new(root: Spanned<RootAst>, span: TextSpan) -> Self {
        Self {
            root,
            quality: None,
            extension: None,
            modifiers: Vec::new(),
            bass: None,
            rhythm: None,
            span,
        }
    }

    /// Check if this chord has any explicit quality marker.
    #[must_use]
    pub fn has_explicit_quality(&self) -> bool {
        self.quality.is_some()
    }

    /// Check if this chord has an extension (7th or higher).
    #[must_use]
    pub fn has_extension(&self) -> bool {
        self.extension.is_some()
    }

    /// Check if this is a slash chord (has bass note different from root).
    #[must_use]
    pub fn is_slash_chord(&self) -> bool {
        self.bass.is_some()
    }

    /// Get the number of modifiers (alterations, additions, omissions).
    #[must_use]
    pub fn modifier_count(&self) -> usize {
        self.modifiers.len()
    }

    /// Add a modifier to this chord.
    pub fn add_modifier(&mut self, modifier: Spanned<ChordModifierAst>) {
        // Extend the chord span to include the modifier
        self.span = self.span.merge(&modifier.span);
        self.modifiers.push(modifier);
    }
}

impl AstNode for ChordAst {
    fn span(&self) -> TextSpan {
        self.span
    }
}

/// Root of a chord (note letter, scale degree, or roman numeral).
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum RootAst {
    /// Note name root (C, D, E, F, G, A, B)
    NoteName {
        letter: char,
        accidental: Option<AccidentalAst>,
    },
    /// Scale degree root (1, 2, 3, 4, 5, 6, 7)
    ScaleDegree {
        degree: u8,
        accidental: Option<AccidentalAst>,
    },
    /// Roman numeral root (I, II, III, IV, V, VI, VII)
    RomanNumeral {
        /// The numeral value (1-7)
        numeral: u8,
        /// Whether it's uppercase (major) or lowercase (minor)
        uppercase: bool,
        accidental: Option<AccidentalAst>,
    },
}

impl RootAst {
    /// Create a note name root.
    #[must_use]
    pub const fn note(letter: char, accidental: Option<AccidentalAst>) -> Self {
        Self::NoteName { letter, accidental }
    }

    /// Create a scale degree root.
    #[must_use]
    pub const fn degree(degree: u8, accidental: Option<AccidentalAst>) -> Self {
        Self::ScaleDegree { degree, accidental }
    }

    /// Create a roman numeral root.
    #[must_use]
    pub const fn roman(numeral: u8, uppercase: bool, accidental: Option<AccidentalAst>) -> Self {
        Self::RomanNumeral {
            numeral,
            uppercase,
            accidental,
        }
    }
}

/// Accidental modifier for roots and bass notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum AccidentalAst {
    /// Sharp (#)
    Sharp,
    /// Flat (b)
    Flat,
    /// Double sharp (##)
    DoubleSharp,
    /// Double flat (bb)
    DoubleFlat,
    /// Natural (explicit)
    Natural,
}

impl AccidentalAst {
    /// Get the semitone offset for this accidental.
    #[must_use]
    pub const fn semitones(&self) -> i8 {
        match self {
            Self::Sharp => 1,
            Self::Flat => -1,
            Self::DoubleSharp => 2,
            Self::DoubleFlat => -2,
            Self::Natural => 0,
        }
    }
}

/// Chord quality marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum QualityAst {
    /// Major (M, maj, or implied)
    Major,
    /// Minor (m, min, -)
    Minor,
    /// Diminished (dim, o, circle)
    Diminished,
    /// Augmented (aug, +)
    Augmented,
    /// Suspended 2nd (sus2)
    Sus2,
    /// Suspended 4th (sus4, sus)
    Sus4,
    /// Power chord (5)
    Power,
    /// Half-diminished (0, m7b5)
    HalfDiminished,
}

/// Extension AST (7th, 9th, 11th, 13th).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
pub struct ExtensionAst {
    /// The extension degree (7, 9, 11, 13, 6)
    pub degree: u8,
    /// Optional quality for the extension
    pub quality: Option<ExtensionQualityAst>,
}

impl ExtensionAst {
    /// Create a new extension.
    #[must_use]
    pub const fn new(degree: u8, quality: Option<ExtensionQualityAst>) -> Self {
        Self { degree, quality }
    }

    /// Seventh extension.
    #[must_use]
    pub const fn seventh(quality: Option<ExtensionQualityAst>) -> Self {
        Self::new(7, quality)
    }

    /// Ninth extension.
    #[must_use]
    pub const fn ninth(quality: Option<ExtensionQualityAst>) -> Self {
        Self::new(9, quality)
    }

    /// Eleventh extension.
    #[must_use]
    pub const fn eleventh(quality: Option<ExtensionQualityAst>) -> Self {
        Self::new(11, quality)
    }

    /// Thirteenth extension.
    #[must_use]
    pub const fn thirteenth(quality: Option<ExtensionQualityAst>) -> Self {
        Self::new(13, quality)
    }

    /// Sixth extension.
    #[must_use]
    pub const fn sixth() -> Self {
        Self::new(6, None)
    }
}

/// Quality modifier for extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum ExtensionQualityAst {
    /// Major (maj7, delta)
    Major,
    /// Dominant (7, implied major with minor 7th)
    Dominant,
    /// Minor (m7, -7)
    Minor,
    /// Diminished (dim7, o7)
    Diminished,
    /// Augmented (aug7, +7)
    Augmented,
}

/// Chord modifier (alteration, addition, or omission).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum ChordModifierAst {
    /// Alteration (b5, #5, b9, #9, #11, b13, etc.)
    Alteration(AlterationAst),
    /// Addition (add9, add11, add13, add2, etc.)
    Addition { degree: u8 },
    /// Omission (no3, no5, omit3, omit5)
    Omission { degree: u8 },
}

/// Alteration to a chord degree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
pub struct AlterationAst {
    /// The altered degree (5, 9, 11, 13, etc.)
    pub degree: u8,
    /// The alteration direction
    pub accidental: AccidentalAst,
}

impl AlterationAst {
    /// Create a new alteration.
    #[must_use]
    pub const fn new(degree: u8, accidental: AccidentalAst) -> Self {
        Self { degree, accidental }
    }

    /// Flat five alteration.
    #[must_use]
    pub const fn flat5() -> Self {
        Self::new(5, AccidentalAst::Flat)
    }

    /// Sharp five alteration.
    #[must_use]
    pub const fn sharp5() -> Self {
        Self::new(5, AccidentalAst::Sharp)
    }

    /// Flat nine alteration.
    #[must_use]
    pub const fn flat9() -> Self {
        Self::new(9, AccidentalAst::Flat)
    }

    /// Sharp nine alteration.
    #[must_use]
    pub const fn sharp9() -> Self {
        Self::new(9, AccidentalAst::Sharp)
    }

    /// Sharp eleven alteration.
    #[must_use]
    pub const fn sharp11() -> Self {
        Self::new(11, AccidentalAst::Sharp)
    }

    /// Flat thirteen alteration.
    #[must_use]
    pub const fn flat13() -> Self {
        Self::new(13, AccidentalAst::Flat)
    }
}

/// Bass tone for slash chords.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct BassToneAst {
    /// The bass note letter (C, D, E, F, G, A, B) or scale degree
    pub root: RootAst,
}

impl BassToneAst {
    /// Create a bass tone from a note letter.
    #[must_use]
    pub fn note(letter: char, accidental: Option<AccidentalAst>) -> Self {
        Self {
            root: RootAst::NoteName { letter, accidental },
        }
    }

    /// Create a bass tone from a scale degree.
    #[must_use]
    pub fn degree(degree: u8, accidental: Option<AccidentalAst>) -> Self {
        Self {
            root: RootAst::ScaleDegree { degree, accidental },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chord_ast_new() {
        let root = Spanned::new(RootAst::note('C', None), TextSpan::new(0, 1));
        let chord = ChordAst::new(root, TextSpan::new(0, 5));

        assert!(!chord.has_explicit_quality());
        assert!(!chord.has_extension());
        assert!(!chord.is_slash_chord());
        assert_eq!(chord.modifier_count(), 0);
    }

    #[test]
    fn test_root_ast_variants() {
        let note = RootAst::note('C', Some(AccidentalAst::Sharp));
        match note {
            RootAst::NoteName { letter, accidental } => {
                assert_eq!(letter, 'C');
                assert_eq!(accidental, Some(AccidentalAst::Sharp));
            }
            _ => panic!("Expected NoteName"),
        }

        let degree = RootAst::degree(5, None);
        match degree {
            RootAst::ScaleDegree { degree, .. } => assert_eq!(degree, 5),
            _ => panic!("Expected ScaleDegree"),
        }

        let roman = RootAst::roman(4, true, None);
        match roman {
            RootAst::RomanNumeral {
                numeral, uppercase, ..
            } => {
                assert_eq!(numeral, 4);
                assert!(uppercase);
            }
            _ => panic!("Expected RomanNumeral"),
        }
    }

    #[test]
    fn test_accidental_semitones() {
        assert_eq!(AccidentalAst::Sharp.semitones(), 1);
        assert_eq!(AccidentalAst::Flat.semitones(), -1);
        assert_eq!(AccidentalAst::DoubleSharp.semitones(), 2);
        assert_eq!(AccidentalAst::DoubleFlat.semitones(), -2);
        assert_eq!(AccidentalAst::Natural.semitones(), 0);
    }

    #[test]
    fn test_extension_ast() {
        let ext = ExtensionAst::seventh(Some(ExtensionQualityAst::Major));
        assert_eq!(ext.degree, 7);
        assert_eq!(ext.quality, Some(ExtensionQualityAst::Major));

        let ninth = ExtensionAst::ninth(None);
        assert_eq!(ninth.degree, 9);
        assert!(ninth.quality.is_none());
    }

    #[test]
    fn test_alteration_ast() {
        let alt = AlterationAst::flat5();
        assert_eq!(alt.degree, 5);
        assert_eq!(alt.accidental, AccidentalAst::Flat);

        let sharp11 = AlterationAst::sharp11();
        assert_eq!(sharp11.degree, 11);
        assert_eq!(sharp11.accidental, AccidentalAst::Sharp);
    }

    #[test]
    fn test_chord_modifier_ast() {
        let alt_mod = ChordModifierAst::Alteration(AlterationAst::flat9());
        let add_mod = ChordModifierAst::Addition { degree: 9 };
        let omit_mod = ChordModifierAst::Omission { degree: 3 };

        assert!(matches!(alt_mod, ChordModifierAst::Alteration(_)));
        assert!(matches!(add_mod, ChordModifierAst::Addition { degree: 9 }));
        assert!(matches!(omit_mod, ChordModifierAst::Omission { degree: 3 }));
    }

    #[test]
    fn test_add_modifier_extends_span() {
        let root = Spanned::new(RootAst::note('C', None), TextSpan::new(0, 1));
        let mut chord = ChordAst::new(root, TextSpan::new(0, 5));

        let modifier = Spanned::new(
            ChordModifierAst::Alteration(AlterationAst::flat5()),
            TextSpan::new(5, 2),
        );
        chord.add_modifier(modifier);

        // Span should now cover 0..7
        assert_eq!(chord.span.start, 0);
        assert_eq!(chord.span.end(), 7);
        assert_eq!(chord.modifier_count(), 1);
    }
}
