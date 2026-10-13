//! Chord definition and parsing
//!
//! Combines root notation with quality, family, and extensions to represent musical chords
//!
//! # Module Structure
//!
//! - **Chord Struct**: Core chord data structure with root, quality, family, extensions
//! - **Construction**: `new`, `with_duration`, `with_family` methods
//! - **Interval Computation**: `compute_intervals`, `intervals`, `semantic_degrees`
//! - **Transposition**: `transpose_to`, `respell_root`
//! - **Normalization**: `normalize` for chord symbol display
//! - **Parsing**: `parse` and helper methods for token parsing
//! - **Display**: `Display` impl for chord symbol output
//! - **LilyPond**: `to_lilypond` for notation export

use super::alteration::Alteration;
use super::degree::ChordDegree;
use super::duration::ChordRhythm;
use super::extensions::{ExtensionQuality, Extensions};
use super::family::ChordFamily;
use super::quality::{ChordQuality, SuspendedType};
use super::root;
use crate::parsing::{ParseError, Token, TokenType};
use crate::primitives::{Interval, RootNotation};
use facet::Facet;
use std::collections::HashMap;
use tracing::{debug, instrument, trace};

// region:    --- Chord Struct

/// A complete chord with root, quality, family, extensions, and alterations
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Chord {
    /// The original input string that was parsed
    pub origin: String,

    /// The descriptor (everything after the root)
    pub descriptor: String,

    /// The normalized version of the chord
    pub normalized: String,

    /// The root note (can be note name, scale degree, or roman numeral)
    pub root: RootNotation,

    /// The chord quality (major, minor, diminished, augmented, suspended, power)
    pub quality: ChordQuality,

    /// The chord family (seventh type: maj7, dom7, m7, mM7, ø7, dim7). None = triad
    pub family: Option<ChordFamily>,

    /// Extensions (9th, 11th, 13th)
    pub extensions: Extensions,

    /// Alterations (b5, #5, b9, #9, #11, b13, etc.)
    pub alterations: Vec<Alteration>,

    /// Additions (add9, add11, etc.) - degrees added without implying lower extensions
    pub additions: Vec<ChordDegree>,

    /// Omissions (no3, no5, etc.) - degrees explicitly removed
    pub omissions: Vec<ChordDegree>,

    /// Bass note for slash chords (if different from root)
    pub bass: Option<RootNotation>,

    /// When `bass` is set, render the bass note vertically beneath the main
    /// symbol (root over horizontal rule over bass) instead of inline (`B/C#`).
    /// Maps to MusicXML `<bass arrangement="vertical">`.
    pub bass_vertical: bool,

    /// Optional rhythm/duration (None = no duration specified)
    pub duration: Option<ChordRhythm>,

    /// Computed intervals (real intervals from root)
    /// Key: ChordDegree, Value: Interval
    pub(crate) intervals: HashMap<ChordDegree, Interval>,

    /// Semantic degrees present in the chord
    pub(crate) semantic_degrees: Vec<ChordDegree>,

    /// Number of tokens consumed during parsing
    pub(crate) tokens_consumed: usize,
}

// endregion: --- Chord Struct

// region:    --- Construction

impl Chord {
    /// Create a new chord with just root and quality (triad, no extensions)
    pub fn new(root: RootNotation, quality: ChordQuality) -> Self {
        let mut chord = Self {
            origin: String::new(),
            descriptor: String::new(),
            normalized: String::new(),
            root,
            quality,
            family: None,
            extensions: Extensions::none(),
            alterations: Vec::new(),
            additions: Vec::new(),
            omissions: Vec::new(),
            bass: None,
            bass_vertical: false,
            duration: None,
            intervals: HashMap::new(),
            semantic_degrees: Vec::new(),
            tokens_consumed: 0,
        };
        chord.compute_intervals();
        chord.normalize();
        chord
    }

    /// Create a new chord with duration
    pub fn with_duration(root: RootNotation, quality: ChordQuality, duration: ChordRhythm) -> Self {
        let mut chord = Self::new(root, quality);
        chord.duration = Some(duration);
        chord
    }

    /// Replace the triad quality and recompute the interval set. Used to apply
    /// a key-implied (diatonic) quality to a bare number-system chord after
    /// parsing — the seventh family / extensions / alterations already on the
    /// chord then stack on the new triad (so `2` → ii minor, `2:7` → ii m7).
    pub fn set_triad_quality(&mut self, quality: ChordQuality) {
        self.quality = quality;
        self.compute_intervals();
    }

    /// Create a chord with explicit family (seventh type)
    pub fn with_family(root: RootNotation, quality: ChordQuality, family: ChordFamily) -> Self {
        let mut chord = Self {
            origin: String::new(),
            descriptor: String::new(),
            normalized: String::new(),
            root,
            quality,
            family: Some(family),
            extensions: Extensions::none(),
            alterations: Vec::new(),
            additions: Vec::new(),
            omissions: Vec::new(),
            bass: None,
            bass_vertical: false,
            duration: None,
            intervals: HashMap::new(),
            semantic_degrees: Vec::new(),
            tokens_consumed: 0,
        };
        chord.compute_intervals();
        chord.normalize();
        chord
    }
}

// endregion: --- Construction

// NOTE: Interval computation methods have been extracted to intervals.rs
// This includes: compute_intervals, intervals, semantic_degrees, interval_for_degree,
// has_degree, exceeds_level, display_at_level, add_alteration, add_addition,
// add_omission, set_extensions, set_bass, respell_root, root_note, semitone_sequence,
// pitch_classes, notes

// NOTE: Transposition methods have been extracted to transposition.rs
// This includes: transpose_to, transpose_root_notation, calculate_quality_from_notes, scale_degrees

// region:    --- Parsing

impl Chord {
    /// Parse a chord from tokens
    ///
    /// Expected format:
    /// - Root: C, F#, Bb (note name) OR 1, #4, b7 (scale degree) OR I, IV, vi (roman)
    /// - Quality: m, maj, dim, aug, sus4, sus2, 5, etc.
    /// - Duration (optional): _4, ////, r4, etc.
    ///
    /// Examples:
    /// - "Cmaj" -> C major (no duration)
    /// - "F#m_4" -> F# minor quarter note
    /// - "4////" -> scale degree 4 with 4 slashes
    /// - "IVm" -> roman IV minor
    /// - "Gsus4_2." -> G suspended 4th, dotted half note
    #[instrument(level = "debug", skip(tokens), fields(token_count = tokens.len()))]
    pub fn parse(tokens: &[Token]) -> Result<Self, ParseError> {
        Self::parse_with_system(tokens, crate::chord::root::NotationSystem::Auto)
    }

    /// Parse a chord, using a notation-system hint to resolve the ambiguous
    /// `b<digit>` root (note B vs flat scale degree). See
    /// [`crate::chord::root::NotationSystem`].
    pub fn parse_with_system(
        tokens: &[Token],
        system: crate::chord::root::NotationSystem,
    ) -> Result<Self, ParseError> {
        if tokens.is_empty() {
            return Err(ParseError::EmptyInput);
        }

        // Step 1: Parse the root (note name, scale degree, or roman numeral)
        trace!("Parsing root from {} tokens", tokens.len());
        let root_result = root::parse_root_with_system(tokens, system)?;
        let mut consumed = root_result.tokens_consumed;
        debug!(
            "Parsed root: {:?}, consumed {} tokens",
            root_result.root, consumed
        );

        // Optional readability separator between the root and the quality:
        // `1:7`, `4:maj9`, `2:m7`. It carries no meaning of its own — skip it.
        if consumed < tokens.len() && tokens[consumed].token_type == TokenType::Colon {
            consumed += 1;
        }

        // Step 2: Parse the quality (if present)
        let quality = if consumed < tokens.len() {
            trace!("Parsing quality from remaining tokens");
            Self::parse_quality(&tokens[consumed..], &root_result.root)?
        } else {
            // No quality specified, default to major
            debug!("No quality tokens, defaulting to Major");
            (ChordQuality::Major, 0)
        };

        consumed += quality.1;
        debug!(
            "Parsed quality: {:?}, total consumed: {}",
            quality.0, consumed
        );

        // Step 3: Parse family (seventh type) if present
        let family = if consumed < tokens.len() {
            trace!("Parsing family from remaining tokens");
            match ChordFamily::parse(&tokens[consumed..], quality.0) {
                Ok((fam, tokens_used)) => {
                    consumed += tokens_used;
                    debug!(
                        "Parsed family: {:?}, consumed {} additional tokens",
                        fam, tokens_used
                    );
                    fam
                }
                Err(_) => {
                    trace!("No family found");
                    None
                }
            }
        } else {
            None
        };

        // Step 4: Parse extensions (9, 11, 13) if present
        let extensions = if consumed < tokens.len() {
            trace!("Parsing extensions from remaining tokens");
            match Extensions::parse(&tokens[consumed..]) {
                Ok((ext, tokens_used)) => {
                    if ext.has_any() {
                        consumed += tokens_used;
                        debug!(
                            "Parsed extensions: {:?}, consumed {} additional tokens",
                            ext, tokens_used
                        );
                        ext
                    } else {
                        Extensions::none()
                    }
                }
                Err(_) => {
                    trace!("No extensions found");
                    Extensions::none()
                }
            }
        } else {
            Extensions::none()
        };

        // If we have extensions but no explicit family, infer the family from quality
        // Extensions (9, 11, 13) imply a seventh chord
        let family = if family.is_none() && extensions.has_any() {
            let inferred = match quality.0 {
                ChordQuality::Minor => Some(ChordFamily::Minor7),
                ChordQuality::Diminished => Some(ChordFamily::FullyDiminished),
                _ => Some(ChordFamily::Dominant7), // Major, Augmented, Suspended
            };
            debug!("Inferred family from extensions: {:?}", inferred);
            inferred
        } else {
            family
        };

        // Step 4b: Check for "sus" after family/extensions for "7sus4" format
        // This handles chords like "G7sus4" or "C9sus4" where sus comes after the 7th
        let mut quality = quality.0;
        if consumed < tokens.len()
            && family.is_some()
            && let Ok((sus_quality, sus_consumed)) =
                Self::parse_suspended_suffix(&tokens[consumed..])
            && matches!(sus_quality, ChordQuality::Suspended(_))
        {
            quality = sus_quality;
            consumed += sus_consumed;
            debug!(
                "Parsed suspended suffix (7sus4 format): {:?}, consumed {}",
                quality, sus_consumed
            );
        }

        // Step 5: Parse alterations (b5, #5, b9, #9, #11, b13) if present
        let alterations = if consumed < tokens.len() {
            trace!("Parsing alterations from remaining tokens");
            match Alteration::parse(&tokens[consumed..]) {
                Ok((alts, tokens_used)) => {
                    if !alts.is_empty() {
                        consumed += tokens_used;
                        debug!(
                            "Parsed alterations: {:?}, consumed {} additional tokens",
                            alts, tokens_used
                        );
                        alts
                    } else {
                        Vec::new()
                    }
                }
                Err(_) => {
                    trace!("No alterations found");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        // Step 6: Check for sixth chord (special case - "6" without "add")
        // Sixth chords like C6, Cm6 are additions but displayed without "add"
        // Also handle 6/9 chords (C6/9, Cm6/9)
        let mut is_sixth_chord = false;
        let mut is_six_nine = false;
        if consumed < tokens.len()
            && family.is_none()
            && let TokenType::Number(n) = &tokens[consumed].token_type
            && n == "6"
        {
            is_sixth_chord = true;
            consumed += 1;
            debug!("Found sixth chord notation at position {}", consumed);

            // Check for "/9" after the "6"
            if consumed + 1 < tokens.len()
                && let TokenType::Slash = tokens[consumed].token_type
                && let TokenType::Number(num) = &tokens[consumed + 1].token_type
                && num == "9"
            {
                is_six_nine = true;
                consumed += 2; // Skip "/" and "9"
                debug!("Found 6/9 chord notation");
            }
        }

        // Step 6b: A bare `2` after a triad is an added 2nd — `42` (written
        // `4:2` once the chart splits the digits), `G2` — the way worship
        // charts write it. Only on a plain triad: after a seventh or an
        // extension a `2` means nothing here.
        let mut is_add_two = false;
        if consumed < tokens.len()
            && family.is_none()
            && !extensions.has_any()
            && !is_sixth_chord
            && matches!(quality, ChordQuality::Major | ChordQuality::Minor)
            && let TokenType::Number(n) = &tokens[consumed].token_type
            && n == "2"
        {
            is_add_two = true;
            consumed += 1;
        }

        // Step 7: Parse additions (add9, add11) if present
        let mut additions = if consumed < tokens.len() {
            trace!("Parsing additions from remaining tokens");
            Self::parse_additions(&tokens[consumed..])?
        } else {
            (Vec::new(), 0)
        };

        // If it's a sixth chord, add the sixth to additions
        if is_sixth_chord {
            additions.0.push(ChordDegree::Sixth);
        }

        // If it's a 6/9 chord, also add the ninth to additions
        if is_six_nine {
            additions.0.push(ChordDegree::Ninth);
        }
        if is_add_two {
            additions.0.insert(0, ChordDegree::Second);
        }

        consumed += additions.1;
        debug!(
            "Parsed additions: {:?}, total consumed: {}",
            additions.0, consumed
        );

        // Step 8: Parse omissions (no3, no5) if present
        let omissions = if consumed < tokens.len() {
            trace!("Parsing omissions from remaining tokens");
            Self::parse_omissions(&tokens[consumed..])?
        } else {
            (Vec::new(), 0)
        };

        consumed += omissions.1;
        debug!(
            "Parsed omissions: {:?}, total consumed: {}",
            omissions.0, consumed
        );

        // Step 9a: Check for slash family notation (/maj7, /m7) before slash chord
        // This handles cases like "Abaug/maj7" or "Cm/maj7"
        let mut final_family = family;
        if consumed < tokens.len()
            && let Ok((Some(slash_family), slash_consumed)) =
                Self::parse_slash_family(&tokens[consumed..])
        {
            // Override or set the family from slash notation
            if final_family.is_none() {
                final_family = Some(slash_family);
            }
            consumed += slash_consumed;
            debug!(
                "Parsed slash family: {:?}, total consumed: {}",
                slash_family, consumed
            );
        }

        // Step 9b: Parse slash chord (bass note) if present
        let bass = if consumed < tokens.len() {
            trace!("Parsing slash chord from remaining tokens");
            Self::parse_slash_chord(&tokens[consumed..])?
        } else {
            (None, 0)
        };

        consumed += bass.1;
        debug!("Parsed bass: {:?}, total consumed: {}", bass.0, consumed);

        // Step 9c: A Roman numeral over another Roman numeral is a secondary /
        // applied chord (`V/V` = "five of five"), not a slash bass. Fold the
        // target into the root so it resolves relative to the tonicised degree;
        // every other `/x` (note, scale degree) stays an ordinary slash bass.
        let mut root = root_result.root;
        let bass = match bass.0 {
            Some(b) if root.is_roman() && b.is_roman() => {
                root = root.with_applied_target(b);
                None
            }
            other => other,
        };

        // Step 10: Parse the duration (if present)
        let duration = if consumed < tokens.len() {
            trace!("Attempting to parse duration from remaining tokens");
            match ChordRhythm::parse(&tokens[consumed..]) {
                Ok((rhythm, tokens_used)) => {
                    consumed += tokens_used;
                    debug!(
                        "Parsed duration: {:?}, consumed {} additional tokens",
                        rhythm, tokens_used
                    );
                    Some(rhythm)
                }
                Err(_) => {
                    trace!("No duration found");
                    None
                }
            }
        } else {
            trace!("No remaining tokens for duration");
            None
        };

        debug!(
            "Chord parsing complete: root={:?}, quality={:?}, family={:?}, extensions={:?}, duration={:?}",
            root, quality, final_family, extensions, duration
        );

        let mut chord = Self {
            origin: String::new(),     // Will be set from tokens if needed
            descriptor: String::new(), // Will be computed
            normalized: String::new(), // Will be computed in normalize()
            root,
            quality,
            family: final_family,
            extensions,
            alterations: alterations.clone(),
            additions: additions.0,
            omissions: omissions.0,
            bass,
            bass_vertical: false,
            duration,
            intervals: HashMap::new(),
            semantic_degrees: Vec::new(),
            tokens_consumed: consumed,
        };

        chord.compute_intervals();
        chord.normalize();
        Ok(chord)
    }

    /// Parse chord quality from tokens
    /// Returns (quality, tokens_consumed)
    fn parse_quality(
        tokens: &[Token],
        root: &RootNotation,
    ) -> Result<(ChordQuality, usize), ParseError> {
        if tokens.is_empty() {
            return Ok((ChordQuality::Major, 0));
        }

        // Skip whitespace
        let tokens = Self::skip_whitespace(tokens);
        if tokens.is_empty() {
            return Ok((ChordQuality::Major, 0));
        }

        let mut consumed = 0;

        // Check for quality indicators
        match &tokens[0].token_type {
            // Check for "min", "maj", or just "m"/"M"
            TokenType::Letter('m') | TokenType::Letter('M') => {
                let is_upper = matches!(tokens[0].token_type, TokenType::Letter('M'));

                // Check for "maj7", "maj9", etc. - DON'T consume as quality, let family parser handle it
                if consumed + 2 < tokens.len() {
                    if let TokenType::Letter('a') = tokens[1].token_type {
                        if let TokenType::Letter('j') = tokens[2].token_type {
                            // Look ahead for a number (7, 9, 11, 13)
                            if consumed + 3 < tokens.len()
                                && let TokenType::Number(_) = tokens[3].token_type
                            {
                                // It's "maj7", "maj9", etc. - don't consume, return Major quality with 0 tokens
                                return Ok((ChordQuality::Major, 0));
                            }
                            // Just "maj" without a number - consume it as quality
                            return Ok((ChordQuality::Major, 3));
                        }
                    } else if !is_upper {
                        // Check for "min7", "min9", etc.
                        if let TokenType::Letter('i') = tokens[1].token_type
                            && let TokenType::Letter('n') = tokens[2].token_type
                        {
                            // Look ahead for a number
                            if consumed + 3 < tokens.len()
                                && let TokenType::Number(_) = tokens[3].token_type
                            {
                                // It's "min7", "min9", etc. - don't consume, return Minor quality with 0 tokens
                                return Ok((ChordQuality::Minor, 0));
                            }
                            // Just "min" without a number
                            return Ok((ChordQuality::Minor, 3));
                        }
                    }
                }

                // Just "m" or "M"
                // Note: We DO consume "m" even if followed by a number like "m9"
                // The number will be handled by family/extension parsers
                if is_upper {
                    Ok((ChordQuality::Major, 1))
                } else {
                    Ok((ChordQuality::Minor, 1))
                }
            }

            TokenType::Minus => Ok((ChordQuality::Minor, 1)),

            TokenType::Triangle => Ok((ChordQuality::Major, 1)),

            // Diminished: "dim", "o", "°"
            // Note: 'd' could also be part of "add" in other contexts.
            // We only consume as Diminished if we see "dim", otherwise return Major with 0 consumed.
            TokenType::Letter('d') => {
                if consumed + 2 < tokens.len()
                    && let TokenType::Letter('i') = tokens[1].token_type
                    && let TokenType::Letter('m') = tokens[2].token_type
                {
                    return Ok((ChordQuality::Diminished, 3)); // "dim"
                }
                // Not "dim" - don't consume, let other parsers handle it
                Ok((ChordQuality::Major, 0))
            }

            // 'o' / '°' is the diminished symbol — except when it starts the
            // `omit5`/`omit3` keyword, which is an omission, not a quality.
            // Leave those for the omissions parser to consume.
            TokenType::Letter('o') => {
                if consumed + 3 < tokens.len()
                    && let TokenType::Letter('m') = tokens[1].token_type
                    && let TokenType::Letter('i') = tokens[2].token_type
                    && let TokenType::Letter('t') = tokens[3].token_type
                {
                    return Ok((ChordQuality::Major, 0));
                }
                Ok((ChordQuality::Diminished, 1))
            }
            TokenType::Circle => Ok((ChordQuality::Diminished, 1)),

            // Augmented: "aug", "+"
            // Note: 'a' could also be the start of "add" (additions) which isn't a quality.
            // We only consume as Augmented if we see "aug", otherwise return Major with 0 consumed
            // to let additions parser handle "add9", "add11", etc.
            TokenType::Letter('a') => {
                if consumed + 2 < tokens.len()
                    && let TokenType::Letter('u') = tokens[1].token_type
                    && let TokenType::Letter('g') = tokens[2].token_type
                {
                    return Ok((ChordQuality::Augmented, 3)); // "aug"
                }
                // Not "aug" - don't consume, let additions parser handle it
                Ok((ChordQuality::Major, 0))
            }

            TokenType::Plus => Ok((ChordQuality::Augmented, 1)),

            // Suspended: "sus", "sus2", "sus4"
            // Note: 's' could also be a space token or other syntax.
            // We only consume as Suspended if we see "sus", otherwise return Major with 0 consumed.
            TokenType::Letter('s') => {
                if consumed + 2 < tokens.len()
                    && let TokenType::Letter('u') = tokens[1].token_type
                    && let TokenType::Letter('s') = tokens[2].token_type
                {
                    consumed = 3; // "sus"

                    // Check for "sus2" or "sus4"
                    if consumed < tokens.len() {
                        match &tokens[consumed].token_type {
                            TokenType::Number(n) if n == "2" => {
                                return Ok((
                                    ChordQuality::Suspended(SuspendedType::Second),
                                    consumed + 1,
                                ));
                            }
                            TokenType::Number(n) if n == "4" => {
                                return Ok((
                                    ChordQuality::Suspended(SuspendedType::Fourth),
                                    consumed + 1,
                                ));
                            }
                            _ => {}
                        }
                    }

                    // Just "sus" defaults to sus4
                    return Ok((ChordQuality::Suspended(SuspendedType::Fourth), consumed));
                }
                // Not "sus" - don't consume, let other parsers handle it
                Ok((ChordQuality::Major, 0))
            }

            // Power chord: "5"
            TokenType::Number(n) if n == "5" => Ok((ChordQuality::Power, 1)),

            // No quality indicator found, infer from root if possible
            _ => {
                // For roman numerals, uppercase = major, lowercase = minor
                if let Some(case) = root.roman_case() {
                    match case {
                        crate::primitives::RomanCase::Upper => Ok((ChordQuality::Major, 0)),
                        crate::primitives::RomanCase::Lower => Ok((ChordQuality::Minor, 0)),
                    }
                } else {
                    // Default to major for other root types
                    Ok((ChordQuality::Major, 0))
                }
            }
        }
    }

    /// Skip leading whitespace tokens
    fn skip_whitespace(tokens: &[Token]) -> &[Token] {
        let mut i = 0;
        while i < tokens.len() && tokens[i].token_type == TokenType::Space {
            i += 1;
        }
        &tokens[i..]
    }

    /// Parse additions (add9, add11, add13, etc.)
    /// Returns (Vec<ChordDegree>, tokens_consumed)
    fn parse_additions(tokens: &[Token]) -> Result<(Vec<ChordDegree>, usize), ParseError> {
        if tokens.is_empty() {
            return Ok((Vec::new(), 0));
        }

        let mut additions = Vec::new();
        let mut consumed = 0;

        loop {
            if consumed >= tokens.len() {
                break;
            }

            // Skip whitespace before "add"
            while consumed < tokens.len() && matches!(tokens[consumed].token_type, TokenType::Space)
            {
                consumed += 1;
            }

            if consumed >= tokens.len() {
                break;
            }

            // An addition may be parenthesised, as lead sheets write it:
            // `5(add4)` is `5add4`.
            let paren = usize::from(tokens[consumed].token_type == TokenType::LParen);

            // Check for "add"
            if consumed + paren + 2 < tokens.len()
                && let TokenType::Letter('a') = tokens[consumed + paren].token_type
                && let TokenType::Letter('d') = tokens[consumed + paren + 1].token_type
                && let TokenType::Letter('d') = tokens[consumed + paren + 2].token_type
            {
                let start = consumed;
                consumed += paren + 3; // "(add" or "add"

                // Parse the degree number
                if consumed < tokens.len()
                    && let TokenType::Number(n) = &tokens[consumed].token_type
                    && let Some(degree) = ChordDegree::from_number(n.parse().ok().unwrap_or(0))
                {
                    consumed += 1;
                    let closed = paren == 0
                        || (consumed < tokens.len()
                            && tokens[consumed].token_type == TokenType::RParen);
                    if closed {
                        consumed += paren;
                        additions.push(degree);
                        continue;
                    }
                }
                // "add" found but no valid number (or an unclosed paren):
                // back up and stop
                consumed = start;
                break;
            }

            // No more additions
            break;
        }

        Ok((additions, consumed))
    }

    /// Parse omissions (no3, no5, omit3, omit5, etc.)
    /// Returns (Vec<ChordDegree>, tokens_consumed)
    fn parse_omissions(tokens: &[Token]) -> Result<(Vec<ChordDegree>, usize), ParseError> {
        if tokens.is_empty() {
            return Ok((Vec::new(), 0));
        }

        let mut omissions = Vec::new();
        let mut consumed = 0;

        loop {
            if consumed >= tokens.len() {
                break;
            }

            // Check for "no" or "omit"
            let keyword_len = if consumed + 1 < tokens.len() {
                if let TokenType::Letter('n') = tokens[consumed].token_type {
                    if let TokenType::Letter('o') = tokens[consumed + 1].token_type {
                        Some(2) // "no"
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let keyword_len = if keyword_len.is_none() && consumed + 3 < tokens.len() {
                if let TokenType::Letter('o') = tokens[consumed].token_type {
                    if let TokenType::Letter('m') = tokens[consumed + 1].token_type {
                        if let TokenType::Letter('i') = tokens[consumed + 2].token_type {
                            if let TokenType::Letter('t') = tokens[consumed + 3].token_type {
                                Some(4) // "omit"
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                keyword_len
            };

            if let Some(len) = keyword_len {
                consumed += len;

                // Parse the degree number
                if consumed < tokens.len()
                    && let TokenType::Number(n) = &tokens[consumed].token_type
                    && let Some(degree) = ChordDegree::from_number(n.parse().ok().unwrap_or(0))
                {
                    omissions.push(degree);
                    consumed += 1;
                    continue;
                }
                // Keyword found but no valid number, back up and stop
                consumed -= len;
                break;
            }

            // No more omissions
            break;
        }

        Ok((omissions, consumed))
    }

    /// Parse slash chord (bass note): /E, /G, etc.
    /// Returns (Option<RootNotation>, tokens_consumed)
    ///
    /// Note: This does NOT handle "/maj7" or "/m7" - those are handled by
    /// `parse_slash_family` which should be called first.
    fn parse_slash_chord(tokens: &[Token]) -> Result<(Option<RootNotation>, usize), ParseError> {
        if tokens.is_empty() {
            return Ok((None, 0));
        }

        let mut consumed = 0;

        // Check for slash
        if consumed < tokens.len()
            && let TokenType::Slash = tokens[consumed].token_type
        {
            consumed += 1;

            // Parse the bass note (same as parsing root)
            if consumed < tokens.len() {
                match root::parse_root(&tokens[consumed..]) {
                    Ok(result) => {
                        consumed += result.tokens_consumed;
                        return Ok((Some(result.root), consumed));
                    }
                    Err(_) => {
                        // Slash found but no valid root, back up
                        return Ok((None, 0));
                    }
                }
            }
        }

        Ok((None, 0))
    }

    /// Parse slash family notation: /maj7, /m7, etc.
    /// This handles cases like "Abaug/maj7" where the slash indicates family, not bass note.
    /// Returns (Option<ChordFamily>, tokens_consumed)
    fn parse_slash_family(tokens: &[Token]) -> Result<(Option<ChordFamily>, usize), ParseError> {
        if tokens.len() < 2 {
            return Ok((None, 0));
        }

        // Check for slash
        if !matches!(tokens[0].token_type, TokenType::Slash) {
            return Ok((None, 0));
        }

        // Check for "maj7" after slash
        if tokens.len() >= 5
            && let (
                TokenType::Letter('m'),
                TokenType::Letter('a'),
                TokenType::Letter('j'),
                TokenType::Number(n),
            ) = (
                &tokens[1].token_type,
                &tokens[2].token_type,
                &tokens[3].token_type,
                &tokens[4].token_type,
            )
            && n == "7"
        {
            return Ok((Some(ChordFamily::Major7), 5));
        }

        // Check for "m7" after slash (minor-major 7th or just minor 7th context)
        if tokens.len() >= 3
            && let (TokenType::Letter('m'), TokenType::Number(n)) =
                (&tokens[1].token_type, &tokens[2].token_type)
            && n == "7"
        {
            // /m7 in context like "Caug/m7" would be unusual
            // but "Cm/maj7" means minor with major 7th
            return Ok((Some(ChordFamily::MinorMajor7), 3));
        }

        Ok((None, 0))
    }

    /// Parse suspended suffix: sus, sus2, sus4
    /// This handles the "7sus4" format where sus comes after the family
    /// Returns (ChordQuality, tokens_consumed)
    fn parse_suspended_suffix(tokens: &[Token]) -> Result<(ChordQuality, usize), ParseError> {
        if tokens.len() < 3 {
            return Ok((ChordQuality::Major, 0));
        }

        // Check for "sus"
        if let (TokenType::Letter('s'), TokenType::Letter('u'), TokenType::Letter('s')) = (
            &tokens[0].token_type,
            &tokens[1].token_type,
            &tokens[2].token_type,
        ) {
            let consumed = 3;

            // Check for "2" or "4" after "sus"
            if consumed < tokens.len()
                && let TokenType::Number(n) = &tokens[consumed].token_type
            {
                match n.as_str() {
                    "2" => {
                        return Ok((ChordQuality::Suspended(SuspendedType::Second), consumed + 1));
                    }
                    "4" => {
                        return Ok((ChordQuality::Suspended(SuspendedType::Fourth), consumed + 1));
                    }
                    _ => {}
                }
            }

            // Just "sus" defaults to sus4
            return Ok((ChordQuality::Suspended(SuspendedType::Fourth), consumed));
        }

        Ok((ChordQuality::Major, 0))
    }

    /// Get the number of tokens consumed during parsing
    pub fn tokens_consumed(&self) -> usize {
        self.tokens_consumed
    }
}

// endregion: --- Parsing

// region:    --- LilyPond

impl Chord {
    /// Convert this chord to LilyPond chordmode notation
    ///
    /// # Arguments
    /// * `key` - Optional key context for resolving scale degrees and roman numerals
    ///
    /// # Returns
    /// LilyPond chord notation string (e.g., "cis:maj7", "ces:m7")
    pub fn to_lilypond(&self, key: Option<&crate::key::Key>) -> String {
        // Convert root to LilyPond format
        let root_str = self.root.to_lilypond(key);

        // Build chord quality suffix
        let mut suffix = String::new();

        // Quality
        match self.quality {
            ChordQuality::Major => {
                // Major is default, no suffix needed for triads
            }
            ChordQuality::Minor => {
                suffix.push_str(":m");
            }
            ChordQuality::Diminished => {
                suffix.push_str(":dim");
            }
            ChordQuality::Augmented => {
                suffix.push_str(":aug");
            }
            ChordQuality::Suspended(_) => {
                suffix.push_str(":sus");
            }
            ChordQuality::Power => {
                suffix.push_str(":5");
            }
        }

        // Family (seventh type)
        if let Some(family) = &self.family {
            match family {
                ChordFamily::Major7 => {
                    if suffix.is_empty() {
                        suffix.push_str(":maj7");
                    } else {
                        suffix.push_str("maj7");
                    }
                }
                ChordFamily::Dominant7 => {
                    if suffix.is_empty() {
                        suffix.push_str(":7");
                    } else {
                        suffix.push('7');
                    }
                }
                ChordFamily::Minor7 => {
                    // Already has :m, just add 7
                    suffix.push('7');
                }
                ChordFamily::MinorMajor7 => {
                    // Already has :m, add maj7
                    suffix.push_str("maj7");
                }
                ChordFamily::HalfDiminished => {
                    if suffix.is_empty() {
                        suffix.push_str(":m7.5-");
                    } else {
                        suffix.push_str("7.5-");
                    }
                }
                ChordFamily::FullyDiminished => {
                    if suffix.is_empty() {
                        suffix.push_str(":dim7");
                    } else {
                        suffix.push_str("dim7");
                    }
                }
            }
        }

        // Extensions
        if self.extensions.has_any() {
            if self.extensions.ninth.is_some() {
                suffix.push('9');
            }
            if self.extensions.eleventh.is_some() {
                suffix.push_str("11");
            }
            if self.extensions.thirteenth.is_some() {
                suffix.push_str("13");
            }
        }

        // Alterations (simplified)
        for alt in &self.alterations {
            match alt.degree {
                ChordDegree::Fifth => {
                    if alt.interval.semitones() == 8 {
                        suffix.push_str("5+");
                    } else if alt.interval.semitones() == 6 {
                        suffix.push_str("5-");
                    }
                }
                _ => {
                    // Other alterations could be added here
                }
            }
        }

        // Bass note (slash chord)
        if let Some(bass) = &self.bass {
            if let Some(bass_note) = bass.resolved_note() {
                suffix.push_str(&format!("/{}", bass_note.to_lilypond()));
            } else if let Some(bass_note) = bass.resolve(key) {
                suffix.push_str(&format!("/{}", bass_note.to_lilypond()));
            }
        }

        format!("{}{}", root_str, suffix)
    }
}

// endregion: --- LilyPond

// region:    --- Display

impl std::fmt::Display for Chord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Root
        write!(f, "{}", self.root)?;

        // Check if this is a sixth chord (has 6th addition but no seventh family)
        let is_sixth_chord = self.additions.contains(&ChordDegree::Sixth) && self.family.is_none();

        // Check if this is a suspended chord with a seventh/extensions
        // These display as "C7sus4" or "C9sus4", not "Csus47" or "Csus49"
        let is_suspended = matches!(self.quality, ChordQuality::Suspended(_));
        let is_suspended_with_seventh = is_suspended && self.family.is_some();

        // Quality (uses its own Display which outputs the symbol)
        // Special case: Power chord with only Second addition displays as "2" not "52"
        // So we skip displaying the quality "5" in this case
        let is_power_with_second = self.quality == ChordQuality::Power
            && self.additions.len() == 1
            && self.additions.contains(&ChordDegree::Second)
            && self.family.is_none();

        // Check if the family already includes the quality in its symbol
        // FullyDiminished.symbol() = "dim7", HalfDiminished.symbol() = "m7b5"
        // So we shouldn't output "dim" before these
        let family_includes_quality = matches!(
            self.family,
            Some(ChordFamily::FullyDiminished) | Some(ChordFamily::HalfDiminished)
        );

        // Check if Power quality is redundant (has 7th, extensions, or is a slash chord)
        // B5/C should display as B/C, not B5/C — power implied by voicing
        let power_is_redundant = self.quality == ChordQuality::Power
            && (self.family.is_some() || self.extensions.has_any() || self.bass.is_some());

        // For suspended chords with sevenths, we defer writing the "sus" part
        // until after the family/extensions (C7sus4, not Csus47)
        if is_suspended_with_seventh {
            // Don't write quality yet - will be written after family/extensions
        } else if family_includes_quality {
            // Skip quality - the family symbol already includes it
            // (e.g., FullyDiminished outputs "dim7", HalfDiminished outputs "m7b5")
        } else if power_is_redundant {
            // Skip "5" when we have a 7th or extensions
        } else if is_sixth_chord && self.quality == ChordQuality::Major {
            // For major sixth chords, just use "C6" (standard notation)
            // Minor sixth chords will show "Cm6" via the quality display
            // Skip quality display for major sixth chords
        } else if !is_power_with_second {
            // Skip quality display for Power+Second (will show as "2" in additions)
            write!(f, "{}", self.quality)?;
        }

        // Family (seventh type) and Extensions
        // Special handling for Major family with extensions: show "maj" + extension number
        if let Some(family) = &self.family {
            if matches!(family, ChordFamily::Major7 | ChordFamily::MinorMajor7)
                && self.extensions.has_any()
            {
                // For maj9, maj11, maj13: show "maj" + highest extension
                write!(f, "maj")?;
                // Show only the highest extension number (e.g., "13" not "9 11 13")
                if self.extensions.thirteenth.is_some() {
                    write!(f, "13")?;
                } else if self.extensions.eleventh.is_some() {
                    write!(f, "11")?;
                } else if self.extensions.ninth.is_some() {
                    write!(f, "9")?;
                }
                // Show altered extensions separately
                if let Some(qual) = self.extensions.ninth
                    && qual != ExtensionQuality::Natural
                {
                    match qual {
                        ExtensionQuality::Flat => write!(f, "b9")?,
                        ExtensionQuality::Sharp => write!(f, "#9")?,
                        _ => {}
                    }
                }
                if let Some(qual) = self.extensions.eleventh
                    && qual != ExtensionQuality::Natural
                {
                    match qual {
                        ExtensionQuality::Flat => write!(f, "b11")?,
                        ExtensionQuality::Sharp => write!(f, "#11")?,
                        _ => {}
                    }
                }
                if let Some(qual) = self.extensions.thirteenth
                    && qual != ExtensionQuality::Natural
                {
                    match qual {
                        ExtensionQuality::Flat => write!(f, "b13")?,
                        ExtensionQuality::Sharp => write!(f, "#13")?,
                        _ => {}
                    }
                }
            } else {
                // Non-major family or no extensions
                // The highest natural extension masks the seventh
                // If all extensions are altered, the seventh must be shown explicitly
                let should_show_seventh =
                    !self.extensions.has_any() || !self.extensions.has_natural();
                if should_show_seventh {
                    write!(f, "{}", family)?;
                }
                // Extensions - show only the highest natural extension number
                if self.extensions.has_any() {
                    // Show highest natural extension number
                    if self.extensions.thirteenth.is_some() {
                        write!(f, "13")?;
                    } else if self.extensions.eleventh.is_some() {
                        write!(f, "11")?;
                    } else if self.extensions.ninth.is_some() {
                        write!(f, "9")?;
                    }
                    // Show altered extensions separately
                    if let Some(qual) = self.extensions.ninth
                        && qual != ExtensionQuality::Natural
                    {
                        match qual {
                            ExtensionQuality::Flat => write!(f, "b9")?,
                            ExtensionQuality::Sharp => write!(f, "#9")?,
                            _ => {}
                        }
                    }
                    if let Some(qual) = self.extensions.eleventh
                        && qual != ExtensionQuality::Natural
                    {
                        match qual {
                            ExtensionQuality::Flat => write!(f, "b11")?,
                            ExtensionQuality::Sharp => write!(f, "#11")?,
                            _ => {}
                        }
                    }
                    if let Some(qual) = self.extensions.thirteenth
                        && qual != ExtensionQuality::Natural
                    {
                        match qual {
                            ExtensionQuality::Flat => write!(f, "b13")?,
                            ExtensionQuality::Sharp => write!(f, "#13")?,
                            _ => {}
                        }
                    }
                }
            }
        } else {
            // No family, just show extensions (with same masking logic)
            if self.extensions.has_any() {
                // Show highest natural extension number
                if self.extensions.thirteenth.is_some() {
                    write!(f, "13")?;
                } else if self.extensions.eleventh.is_some() {
                    write!(f, "11")?;
                } else if self.extensions.ninth.is_some() {
                    write!(f, "9")?;
                }
                // Show altered extensions separately
                if let Some(qual) = self.extensions.ninth
                    && qual != ExtensionQuality::Natural
                {
                    match qual {
                        ExtensionQuality::Flat => write!(f, "b9")?,
                        ExtensionQuality::Sharp => write!(f, "#9")?,
                        _ => {}
                    }
                }
                if let Some(qual) = self.extensions.eleventh
                    && qual != ExtensionQuality::Natural
                {
                    match qual {
                        ExtensionQuality::Flat => write!(f, "b11")?,
                        ExtensionQuality::Sharp => write!(f, "#11")?,
                        _ => {}
                    }
                }
                if let Some(qual) = self.extensions.thirteenth
                    && qual != ExtensionQuality::Natural
                {
                    match qual {
                        ExtensionQuality::Flat => write!(f, "b13")?,
                        ExtensionQuality::Sharp => write!(f, "#13")?,
                        _ => {}
                    }
                }
            }
        }

        // For suspended chords with sevenths, write the "sus" part now
        // (after family/extensions, so we get "C7sus4" not "Csus47")
        if is_suspended_with_seventh {
            write!(f, "{}", self.quality)?;
        }

        // Alterations (each uses its own Display)
        for alteration in &self.alterations {
            write!(f, "{}", alteration)?;
        }

        // Additions - sixth chords are special (displayed as "6" not "add6")
        // 6/9 chords are even more special (displayed as "6/9" not "6add9")
        let is_sixth_chord = self.additions.contains(&ChordDegree::Sixth) && self.family.is_none();
        let is_six_nine_chord = is_sixth_chord && self.additions.contains(&ChordDegree::Ninth);

        // For 6/9 chords, display in specific order: 6 first, then /9
        if is_six_nine_chord {
            write!(f, "6/9")?;
            // Display any other additions
            for addition in &self.additions {
                if *addition != ChordDegree::Sixth && *addition != ChordDegree::Ninth {
                    write!(f, "add{}", addition)?;
                }
            }
        } else {
            // Normal addition display
            // Special case: Power chord with only Second addition displays as "2" not "5add2"
            let is_power_with_second = self.quality == ChordQuality::Power
                && self.additions.len() == 1
                && self.additions.contains(&ChordDegree::Second)
                && self.family.is_none();

            if is_power_with_second {
                // Display as "2" (e.g., "D2") not "5add2"
                write!(f, "2")?;
            } else {
                for addition in &self.additions {
                    if *addition == ChordDegree::Sixth && is_sixth_chord {
                        // Display as "6" for sixth chords (C6, Cm6)
                        write!(f, "6")?;
                    } else {
                        // Display with "add" prefix for other additions
                        write!(f, "add{}", addition)?;
                    }
                }
            }
        }

        // Omissions (each degree uses its own Display)
        for omission in &self.omissions {
            write!(f, "no{}", omission)?;
        }

        // Bass note (slash chord - uses RootNotation's Display)
        if let Some(bass) = &self.bass {
            write!(f, "/{}", bass)?;
        }

        Ok(())
    }
}

// endregion: --- Display

// region:    --- ChordSymbol Trait Implementation

use crate::core::ChordSymbol;

impl ChordSymbol for Chord {
    fn root_str(&self) -> String {
        self.root.to_string()
    }

    fn quality_str(&self) -> &str {
        self.quality.symbol()
    }

    fn seventh_str(&self) -> Option<&str> {
        self.family.as_ref().map(|f| f.symbol())
    }

    fn extensions_str(&self) -> String {
        let mut result = String::new();

        // Build extension string from highest to lowest
        if self.extensions.thirteenth.is_some() {
            result.push_str("13");
        } else if self.extensions.eleventh.is_some() {
            result.push_str("11");
        } else if self.extensions.ninth.is_some() {
            result.push('9');
        }

        result
    }

    fn alterations_str(&self) -> String {
        let mut result = String::new();

        for alt in &self.alterations {
            result.push_str(&alt.to_string());
        }

        result
    }

    fn bass_str(&self) -> Option<String> {
        self.bass.as_ref().map(|b| b.to_string())
    }

    // Override to use the already-computed normalized field
    fn to_symbol_string(&self) -> String {
        if self.normalized.is_empty() {
            // Fallback to Display impl if normalized not set
            self.to_string()
        } else {
            format!("{}{}", self.root, self.normalized)
        }
    }
}

// endregion: --- ChordSymbol Trait Implementation

// region:    --- Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::duration::ChordRhythm;
    use crate::key::Key;
    use crate::parsing::Lexer;
    use crate::primitives::MusicalNote;

    fn parse_chord(s: &str) -> Chord {
        let mut lexer = Lexer::new(s.to_string());
        Chord::parse(&lexer.tokenize()).unwrap()
    }

    /// A bare `2` on a triad is an added 2nd, and says so: `42` (split by
    /// the chart to `4:2`) is the 4 chord add2, `G2` is Gadd2 — not a 4 or
    /// a G with the 2 dropped, and not respelled add9.
    #[test]
    fn a_bare_two_on_a_triad_is_add2() {
        for (written, shown) in [
            ("4:2", "4add2"),
            ("G2", "Gadd2"),
            ("Gm2", "Gmadd2"),
            ("4:2/6", "4add2/6"),
            ("4add2", "4add2"),
        ] {
            let chord = parse_chord(written);
            assert_eq!(chord.additions, vec![ChordDegree::Second], "{written}");
            assert_eq!(chord.to_string(), shown, "{written}");
        }
        // An added 4th keeps its spelling too, with or without parentheses.
        assert_eq!(parse_chord("5add4").to_string(), "5add4");
        assert_eq!(parse_chord("5(add4)").to_string(), "5add4");
        // Not on a seventh or a sus2, which already says what it is.
        assert!(parse_chord("4sus2").additions.is_empty());
        assert!(parse_chord("G7").additions.is_empty());
    }

    #[test]
    fn secondary_dominant_resolves_against_tonicised_degree() {
        let c_major = Key::parse("C").unwrap();

        // V/V in C: target V = G, then V of G = D. Default major triad.
        let vv = parse_chord("V/V");
        assert_eq!(vv.root_note(Some(&c_major)).unwrap().name, "D");
        assert_eq!(format!("{}", vv.root), "V/V");
        assert!(vv.bass.is_none(), "a secondary chord is not a slash bass");
        assert!(vv.root.applied_target().is_some());

        // V/vi: vi = A, then V of A = E.
        assert_eq!(
            parse_chord("V/vi").root_note(Some(&c_major)).unwrap().name,
            "E"
        );
        // V/ii: ii = D, then V of D = A.
        assert_eq!(
            parse_chord("V/ii").root_note(Some(&c_major)).unwrap().name,
            "A"
        );

        // V7/V keeps the dominant-7th family, rooted on the resolved D.
        let v7v = parse_chord("V7/V");
        assert_eq!(v7v.root_note(Some(&c_major)).unwrap().name, "D");
        assert!(v7v.family.is_some());
        assert_eq!(format!("{}", v7v.root), "V/V");

        // A plain slash bass is unchanged: `1/3` and `G/B` stay slash chords.
        let slash = parse_chord("1/3");
        assert!(slash.bass.is_some());
        assert!(slash.root.applied_target().is_none());
        let letter = parse_chord("G/B");
        assert!(letter.bass.is_some());
        assert!(letter.root.applied_target().is_none());
    }

    #[test]
    fn test_parse_c_major() {
        let mut lexer = Lexer::new("C".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Major);
    }

    #[test]
    fn test_parse_f_sharp_minor() {
        let mut lexer = Lexer::new("F#m".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Minor);
    }

    #[test]
    fn test_parse_scale_degree() {
        let mut lexer = Lexer::new("4".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Major);
    }

    #[test]
    fn test_parse_roman_upper_case() {
        let mut lexer = Lexer::new("IV".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Major);
    }

    #[test]
    fn test_parse_roman_lower_case() {
        let mut lexer = Lexer::new("vi".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Minor);
    }

    #[test]
    fn test_parse_suspended() {
        let mut lexer = Lexer::new("Gsus4".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(
            chord.quality,
            ChordQuality::Suspended(SuspendedType::Fourth)
        );
    }

    #[test]
    fn test_parse_sus2() {
        let mut lexer = Lexer::new("Dsus2".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(
            chord.quality,
            ChordQuality::Suspended(SuspendedType::Second)
        );
    }

    #[test]
    fn test_parse_diminished() {
        let mut lexer = Lexer::new("Bdim".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Diminished);
    }

    #[test]
    fn test_parse_augmented() {
        let mut lexer = Lexer::new("Caug".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Augmented);
    }

    #[test]
    fn test_parse_power_chord() {
        let mut lexer = Lexer::new("E5".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Power);
    }

    #[test]
    fn test_display() {
        let mut lexer = Lexer::new("F#m".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let display = format!("{}", chord);
        assert!(display.contains("F#"));
        assert!(display.contains("m"));
    }

    #[test]
    fn test_parse_chord_with_lily_duration() {
        use crate::chord::duration::LilySyntax;

        let mut lexer = Lexer::new("C_4".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Major);
        assert!(chord.duration.is_some());
        if let Some(rhythm) = &chord.duration {
            if let Some((duration, _dotted, _triplet)) = rhythm.lily_parts() {
                assert_eq!(duration, LilySyntax::Quarter);
            } else {
                panic!("Expected lily duration");
            }
        } else {
            panic!("Expected Lily duration");
        }
    }

    #[test]
    fn test_parse_chord_with_slashes() {
        let mut lexer = Lexer::new("Dm////".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Minor);
        assert!(chord.duration.is_some());
        if let Some(ChordRhythm::Slashes { count, .. }) = chord.duration {
            assert_eq!(count, 4);
        } else {
            panic!("Expected Slashes duration");
        }
    }

    #[test]
    fn test_parse_chord_without_duration() {
        let mut lexer = Lexer::new("Gmaj".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Major);
        assert!(chord.duration.is_none());
    }

    #[test]
    fn test_parse_scale_degree_with_duration() {
        use crate::chord::duration::LilySyntax;

        let mut lexer = Lexer::new("4_8.".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Major);
        assert!(chord.duration.is_some());
        if let Some(rhythm) = &chord.duration {
            if let Some((duration, dotted, _triplet)) = rhythm.lily_parts() {
                assert_eq!(duration, LilySyntax::Eighth);
                assert!(dotted);
            } else {
                panic!("Expected lily duration");
            }
        } else {
            panic!("Expected dotted Lily duration");
        }
    }

    // === Chord System Tests ===

    #[test]
    fn test_simple_major_triad() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::new(root, ChordQuality::Major);

        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, None);
        assert!(!chord.extensions.has_any());

        // Check intervals
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::Unison));
        assert!(intervals.contains(&Interval::MajorThird));
        assert!(intervals.contains(&Interval::PerfectFifth));
        assert_eq!(intervals.len(), 3);

        // Check semantic degrees
        let degrees = chord.semantic_degrees();
        assert!(degrees.contains(&ChordDegree::Root));
        assert!(degrees.contains(&ChordDegree::Third));
        assert!(degrees.contains(&ChordDegree::Fifth));
        assert_eq!(degrees.len(), 3);
    }

    #[test]
    fn test_minor_triad() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::new(root, ChordQuality::Minor);

        assert_eq!(chord.quality, ChordQuality::Minor);

        // Check intervals
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::MinorThird));
        assert!(intervals.contains(&Interval::PerfectFifth));
    }

    #[test]
    fn test_major_seventh_chord() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::with_family(root, ChordQuality::Major, ChordFamily::Major7);

        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, Some(ChordFamily::Major7));

        // Check intervals
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::Unison));
        assert!(intervals.contains(&Interval::MajorThird));
        assert!(intervals.contains(&Interval::PerfectFifth));
        assert!(intervals.contains(&Interval::MajorSeventh));
        assert_eq!(intervals.len(), 4);

        // Check semantic degrees
        let degrees = chord.semantic_degrees();
        assert!(degrees.contains(&ChordDegree::Seventh));
        assert_eq!(degrees.len(), 4);
    }

    #[test]
    fn test_dominant_seventh_chord() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::with_family(root, ChordQuality::Major, ChordFamily::Dominant7);

        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, Some(ChordFamily::Dominant7));

        // Check intervals - should have minor 7th
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::MinorSeventh));
        assert!(!intervals.contains(&Interval::MajorSeventh));
    }

    #[test]
    fn test_minor_seventh_chord() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::with_family(root, ChordQuality::Minor, ChordFamily::Minor7);

        assert_eq!(chord.quality, ChordQuality::Minor);
        assert_eq!(chord.family, Some(ChordFamily::Minor7));

        // Check intervals
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::MinorThird));
        assert!(intervals.contains(&Interval::MinorSeventh));
    }

    #[test]
    fn test_minor_major_seventh() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::with_family(root, ChordQuality::Minor, ChordFamily::MinorMajor7);

        assert_eq!(chord.quality, ChordQuality::Minor);
        assert_eq!(chord.family, Some(ChordFamily::MinorMajor7));

        // Check intervals - minor 3rd with major 7th
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::MinorThird));
        assert!(intervals.contains(&Interval::MajorSeventh));
    }

    #[test]
    fn test_half_diminished_seventh() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::with_family(root, ChordQuality::Diminished, ChordFamily::HalfDiminished);

        assert_eq!(chord.quality, ChordQuality::Diminished);
        assert_eq!(chord.family, Some(ChordFamily::HalfDiminished));

        // Check intervals
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::MinorThird));
        assert!(intervals.contains(&Interval::DiminishedFifth));
        assert!(intervals.contains(&Interval::MinorSeventh));
    }

    #[test]
    fn test_fully_diminished_seventh() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord =
            Chord::with_family(root, ChordQuality::Diminished, ChordFamily::FullyDiminished);

        assert_eq!(chord.quality, ChordQuality::Diminished);
        assert_eq!(chord.family, Some(ChordFamily::FullyDiminished));

        // Check intervals
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::DiminishedSeventh));
    }

    #[test]
    fn test_chord_with_extensions() {
        use crate::chord::extensions::{ExtensionQuality, Extensions};
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::with_family(root, ChordQuality::Major, ChordFamily::Dominant7);

        // Add ninth extension
        let extensions = Extensions::with_ninth(ExtensionQuality::Natural);
        chord.set_extensions(extensions);

        // Check intervals
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::Ninth));

        // Check semantic degrees
        let degrees = chord.semantic_degrees();
        assert!(degrees.contains(&ChordDegree::Ninth));
    }

    #[test]
    fn test_chord_with_flat_ninth() {
        use crate::chord::extensions::{ExtensionQuality, Extensions};
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::with_family(root, ChordQuality::Major, ChordFamily::Dominant7);

        // Add flat ninth extension
        let extensions = Extensions::with_ninth(ExtensionQuality::Flat);
        chord.set_extensions(extensions);

        // Check intervals
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::FlatNinth));
    }

    #[test]
    fn test_chord_with_sharp_ninth() {
        use crate::chord::extensions::{ExtensionQuality, Extensions};
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::with_family(root, ChordQuality::Major, ChordFamily::Dominant7);

        // Add sharp ninth extension
        let extensions = Extensions::with_ninth(ExtensionQuality::Sharp);
        chord.set_extensions(extensions);

        // Check intervals
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::SharpNinth));
    }

    #[test]
    fn test_chord_with_thirteenth() {
        use crate::chord::extensions::{ExtensionQuality, Extensions};
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::with_family(root, ChordQuality::Major, ChordFamily::Dominant7);

        // Add 13th extension (implies 9th and 11th)
        let extensions = Extensions::with_thirteenth(
            ExtensionQuality::Natural,
            ExtensionQuality::Natural,
            ExtensionQuality::Natural,
        );
        chord.set_extensions(extensions);

        // Check intervals
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::Ninth));
        assert!(intervals.contains(&Interval::Eleventh));
        assert!(intervals.contains(&Interval::Thirteenth));

        // Check semantic degrees
        let degrees = chord.semantic_degrees();
        assert!(degrees.contains(&ChordDegree::Ninth));
        assert!(degrees.contains(&ChordDegree::Eleventh));
        assert!(degrees.contains(&ChordDegree::Thirteenth));
    }

    #[test]
    fn test_chord_with_alteration_flat_five() {
        use crate::chord::alteration::Alteration;
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::with_family(root, ChordQuality::Major, ChordFamily::Dominant7);

        // Add b5 alteration
        let result = chord.add_alteration(Alteration::flat_five());
        assert!(result.is_ok());

        // Check intervals - should have diminished fifth instead of perfect fifth
        assert_eq!(
            chord.interval_for_degree(ChordDegree::Fifth),
            Some(Interval::DiminishedFifth)
        );
    }

    #[test]
    fn test_chord_with_alteration_sharp_five() {
        use crate::chord::alteration::Alteration;
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::with_family(root, ChordQuality::Major, ChordFamily::Dominant7);

        // Add #5 alteration
        let result = chord.add_alteration(Alteration::sharp_five());
        assert!(result.is_ok());

        // Check intervals
        assert_eq!(
            chord.interval_for_degree(ChordDegree::Fifth),
            Some(Interval::AugmentedFifth)
        );
    }

    #[test]
    fn test_chord_with_additions() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::new(root, ChordQuality::Major);

        // Add ninth without implying seventh
        chord.add_addition(ChordDegree::Ninth);

        // Check that 9th is present but 7th is not
        assert!(chord.has_degree(ChordDegree::Ninth));
        assert!(!chord.has_degree(ChordDegree::Seventh));
    }

    #[test]
    fn test_chord_with_omissions() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::new(root, ChordQuality::Major);

        // Omit the third (power chord style)
        chord.add_omission(ChordDegree::Third);

        // Check that third is not present
        assert!(!chord.has_degree(ChordDegree::Third));
        assert!(chord.has_degree(ChordDegree::Root));
        assert!(chord.has_degree(ChordDegree::Fifth));
    }

    #[test]
    fn test_chord_with_bass_note() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::new(root, ChordQuality::Major);

        // Add bass note (slash chord)
        let bass = RootNotation::from_note_name(MusicalNote::g());
        chord.set_bass(bass);

        assert!(chord.bass.is_some());
    }

    #[test]
    fn test_complex_chord() {
        use crate::chord::extensions::{ExtensionQuality, Extensions};
        use crate::primitives::MusicalNote;

        // Build a C13(#11) chord
        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::with_family(root, ChordQuality::Major, ChordFamily::Dominant7);

        // Add extensions
        let extensions = Extensions::with_thirteenth(
            ExtensionQuality::Natural, // 9
            ExtensionQuality::Sharp,   // #11
            ExtensionQuality::Natural, // 13
        );
        chord.set_extensions(extensions);

        // Verify all degrees are present
        assert!(chord.has_degree(ChordDegree::Root));
        assert!(chord.has_degree(ChordDegree::Third));
        assert!(chord.has_degree(ChordDegree::Fifth));
        assert!(chord.has_degree(ChordDegree::Seventh));
        assert!(chord.has_degree(ChordDegree::Ninth));
        assert!(chord.has_degree(ChordDegree::Eleventh));
        assert!(chord.has_degree(ChordDegree::Thirteenth));

        // Verify #11
        assert_eq!(
            chord.interval_for_degree(ChordDegree::Eleventh),
            Some(Interval::SharpEleventh)
        );
    }

    #[test]
    fn test_altered_dominant() {
        use crate::chord::alteration::Alteration;
        use crate::chord::extensions::{ExtensionQuality, Extensions};
        use crate::primitives::MusicalNote;

        // Build a C7(b9,#5,b13) - altered dominant
        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::with_family(root, ChordQuality::Major, ChordFamily::Dominant7);

        let extensions = Extensions::with_thirteenth(
            ExtensionQuality::Flat,  // b9
            ExtensionQuality::Sharp, // #11
            ExtensionQuality::Flat,  // b13
        );
        chord.set_extensions(extensions);

        // Add #5 alteration
        chord.add_alteration(Alteration::sharp_five()).unwrap();

        // Verify alterations
        assert_eq!(
            chord.interval_for_degree(ChordDegree::Ninth),
            Some(Interval::FlatNinth)
        );
        assert_eq!(
            chord.interval_for_degree(ChordDegree::Fifth),
            Some(Interval::AugmentedFifth)
        );
        assert_eq!(
            chord.interval_for_degree(ChordDegree::Thirteenth),
            Some(Interval::FlatThirteenth)
        );
    }

    #[test]
    fn test_display_dominant_seventh() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::with_family(root, ChordQuality::Major, ChordFamily::Dominant7);

        let display = format!("{}", chord);
        assert!(display.contains("C"));
        assert!(display.contains("7"));
    }

    #[test]
    fn test_display_major_seventh() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::with_family(root, ChordQuality::Major, ChordFamily::Major7);

        let display = format!("{}", chord);
        assert!(display.contains("C"));
        assert!(display.contains("maj7"));
    }

    #[test]
    fn test_suspended_chords_intervals() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::new(root, ChordQuality::sus4());

        // Sus4 should have 4th instead of 3rd
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::PerfectFourth));
        assert!(!intervals.contains(&Interval::MajorThird));
        assert!(!intervals.contains(&Interval::MinorThird));
    }

    #[test]
    fn test_power_chord_intervals() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::new(root, ChordQuality::Power);

        // Power chord should only have root and fifth
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::Unison));
        assert!(intervals.contains(&Interval::PerfectFifth));
        assert!(!intervals.contains(&Interval::MajorThird));
        assert!(!intervals.contains(&Interval::MinorThird));
    }

    #[test]
    fn test_augmented_triad_intervals() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::new(root, ChordQuality::Augmented);

        // Augmented should have augmented fifth
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::AugmentedFifth));
        assert!(!intervals.contains(&Interval::PerfectFifth));
    }

    #[test]
    fn test_diminished_triad_intervals() {
        use crate::primitives::MusicalNote;

        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::new(root, ChordQuality::Diminished);

        // Diminished should have diminished fifth
        let intervals = chord.intervals();
        assert!(intervals.contains(&Interval::DiminishedFifth));
        assert!(!intervals.contains(&Interval::PerfectFifth));
    }

    #[test]
    fn test_display_uses_component_displays() {
        use crate::chord::alteration::Alteration;
        use crate::chord::extensions::{ExtensionQuality, Extensions};
        use crate::primitives::MusicalNote;

        // Build a complex chord: C7#5b9
        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::with_family(root, ChordQuality::Major, ChordFamily::Dominant7);

        // Add extensions
        let extensions = Extensions::with_ninth(ExtensionQuality::Flat);
        chord.set_extensions(extensions);

        // Add alteration
        chord.add_alteration(Alteration::sharp_five()).unwrap();

        // Display should use all component Display implementations
        let display = format!("{}", chord);

        // Should contain: C (root), nothing (major quality has empty symbol),
        // 7 (dominant family), b9 (extension with quality), #5 (alteration)
        assert!(display.starts_with("C"));
        assert!(display.contains("7"));
        assert!(display.contains("b9"));
        assert!(display.contains("#5"));
    }

    #[test]
    fn test_display_with_additions_and_omissions() {
        use crate::primitives::MusicalNote;

        // Build: Cadd9no5
        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::new(root, ChordQuality::Major);

        chord.add_addition(ChordDegree::Ninth);
        chord.add_omission(ChordDegree::Fifth);

        let display = format!("{}", chord);
        assert!(display.contains("C"));
        assert!(display.contains("add9"));
        assert!(display.contains("no5"));
    }

    #[test]
    fn test_display_with_slash_chord() {
        use crate::primitives::MusicalNote;

        // Build: C/E
        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::new(root, ChordQuality::Major);

        let bass = RootNotation::from_note_name(MusicalNote::e());
        chord.set_bass(bass);

        let display = format!("{}", chord);
        assert!(display.starts_with("C"));
        assert!(display.contains("/E"));
    }

    #[test]
    fn test_parse_sixth_chord() {
        let mut lexer = Lexer::new("C6".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, None);
        assert!(chord.has_degree(ChordDegree::Sixth));

        // Display should show "C6" (major sixth chord - standard notation)
        let display = format!("{}", chord);
        assert_eq!(display, "C6");

        // Normalized should also be "C6"
        assert_eq!(chord.normalized, "C6");
        assert_eq!(chord.descriptor, "6");
    }

    #[test]
    fn test_parse_minor_sixth_chord() {
        let mut lexer = Lexer::new("Cm6".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Minor);
        assert_eq!(chord.family, None);
        assert!(chord.has_degree(ChordDegree::Sixth));

        // Display should show "Cm6" not "Cmadd6"
        let display = format!("{}", chord);
        assert_eq!(display, "Cm6");
    }

    #[test]
    fn test_parse_sixth_ninth_chord() {
        // Parse using "6/9" notation
        let mut lexer = Lexer::new("C6/9".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, None);
        assert!(chord.has_degree(ChordDegree::Sixth));
        assert!(chord.has_degree(ChordDegree::Ninth));

        // Display should show "C6/9"
        let display = format!("{}", chord);
        assert_eq!(display, "C6/9");
    }

    #[test]
    fn test_parse_sixth_with_add_ninth() {
        // Parse using "6add9" notation
        let mut lexer = Lexer::new("C6add9".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, None);
        assert!(chord.has_degree(ChordDegree::Sixth));
        assert!(chord.has_degree(ChordDegree::Ninth));

        // Display should normalize to "C6/9"
        let display = format!("{}", chord);
        assert_eq!(display, "C6/9");
    }

    #[test]
    fn test_parse_minor_sixth_ninth_chord() {
        let mut lexer = Lexer::new("Cm6/9".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.quality, ChordQuality::Minor);
        assert_eq!(chord.family, None);
        assert!(chord.has_degree(ChordDegree::Sixth));
        assert!(chord.has_degree(ChordDegree::Ninth));

        // Display should show "Cm6/9"
        let display = format!("{}", chord);
        assert_eq!(display, "Cm6/9");
    }

    #[test]
    fn test_sixth_with_seventh_uses_add() {
        use crate::primitives::MusicalNote;

        // C7 with added 6th should display as "C7add6" not "C76"
        let root = RootNotation::from_note_name(MusicalNote::c());
        let mut chord = Chord::with_family(root, ChordQuality::Major, ChordFamily::Dominant7);
        chord.add_addition(ChordDegree::Sixth);

        let display = format!("{}", chord);
        assert_eq!(display, "C7add6");
    }

    #[test]
    fn test_normalization_basic() {
        let mut lexer = Lexer::new("Cmaj7".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        println!("Quality: {:?}", chord.quality);
        println!("Family: {:?}", chord.family);
        println!("Normalized: {}", chord.normalized);
        println!("Descriptor: {}", chord.descriptor);

        assert_eq!(chord.normalized, "Cmaj7");
        assert_eq!(chord.descriptor, "maj7");
    }

    #[test]
    fn test_normalization_with_extensions() {
        let mut lexer = Lexer::new("Dm9".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.normalized, "Dm9");
        assert_eq!(chord.descriptor, "m9");
    }

    #[test]
    fn test_normalization_with_alterations() {
        let mut lexer = Lexer::new("G7#9b13".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        // Note: #9 and b13 are extensions (not alterations)
        // All extensions are altered (no natural extensions)
        // When all extensions are altered, the seventh must be shown explicitly
        // G7#9b13 stays as G7#9b13
        assert_eq!(chord.normalized, "G7#9b13");
        assert!(chord.normalized.contains("7"));
        assert!(chord.normalized.contains("#9"));
        assert!(chord.normalized.contains("b13"));
    }

    #[test]
    fn test_normalization_slash_chord() {
        let mut lexer = Lexer::new("C/E".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        assert_eq!(chord.normalized, "C/E");
        assert_eq!(chord.descriptor, "/E");
    }

    #[test]
    fn test_normalization_complex() {
        let mut lexer = Lexer::new("Dm7b5add11/F".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        // Check that all components are present in normalized form
        assert!(chord.normalized.contains("Dm"));
        assert!(chord.normalized.contains("7"));
        assert!(chord.normalized.contains("b5"));
        assert!(chord.normalized.contains("add11"));
        assert!(chord.normalized.contains("/F"));
    }

    // Tests for new methods: semitone_sequence, root_note, notes, scale_degrees

    #[test]
    fn test_semitone_sequence_major_triad() {
        let mut lexer = Lexer::new("C".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let semitones = chord.semitone_sequence();
        // C major triad: C (0), E (4), G (7)
        assert_eq!(semitones, vec![0, 4, 7]);
    }

    #[test]
    fn test_semitone_sequence_minor_seventh() {
        let mut lexer = Lexer::new("Dm7".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let semitones = chord.semitone_sequence();
        // D minor 7: D (0), F (3), A (7), C (10)
        assert_eq!(semitones, vec![0, 3, 7, 10]);
    }

    #[test]
    fn test_semitone_sequence_major_seventh() {
        let mut lexer = Lexer::new("Cmaj7".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let semitones = chord.semitone_sequence();
        // C major 7: C (0), E (4), G (7), B (11)
        assert_eq!(semitones, vec![0, 4, 7, 11]);
    }

    #[test]
    fn test_semitone_sequence_dominant_ninth() {
        let mut lexer = Lexer::new("G9".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let semitones = chord.semitone_sequence();
        // G9: G (0), B (4), D (7), F (10), A (14 - in second octave)
        assert_eq!(semitones, vec![0, 4, 7, 10, 14]);

        // Pitch classes should wrap to first octave
        let pitch_classes = chord.pitch_classes();
        assert_eq!(pitch_classes, vec![0, 2, 4, 7, 10]);
    }

    #[test]
    fn test_semitone_sequence_altered_chord() {
        let mut lexer = Lexer::new("C7#5".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let semitones = chord.semitone_sequence();
        // C7#5: C (0), E (4), G# (8), Bb (10)
        assert_eq!(semitones, vec![0, 4, 8, 10]);
    }

    #[test]
    fn test_semitone_sequence_sixth_chord() {
        let mut lexer = Lexer::new("C6".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let semitones = chord.semitone_sequence();
        // C6: C (0), E (4), G (7), A (9)
        assert_eq!(semitones, vec![0, 4, 7, 9]);
    }

    #[test]
    fn test_semitone_sequence_thirteenth_chord() {
        let mut lexer = Lexer::new("C13".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let semitones = chord.semitone_sequence();
        // C13: C (0), E (4), G (7), Bb (10), D (14), F (17), A (21)
        // All extensions are in second octave
        assert_eq!(semitones, vec![0, 4, 7, 10, 14, 17, 21]);
    }

    #[test]
    fn test_pitch_classes_vs_semitone_sequence() {
        let mut lexer = Lexer::new("Dm11".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        // Semitone sequence preserves octave information
        let semitones = chord.semitone_sequence();
        // Dm11: D (0), F (3), A (7), C (10), E (14), G (17)
        assert_eq!(semitones, vec![0, 3, 7, 10, 14, 17]);

        // Pitch classes reduces to one octave
        let pitch_classes = chord.pitch_classes();
        // D (0), E (2), F (3), G (5), A (7), C (10)
        assert_eq!(pitch_classes, vec![0, 2, 3, 5, 7, 10]);
    }

    #[test]
    fn test_root_note_from_note_name() {
        let mut lexer = Lexer::new("C".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let root = chord.root_note(None).expect("Should resolve without key");
        assert_eq!(root.name, "C");
        assert_eq!(root.semitone, 0);
    }

    #[test]
    fn test_root_note_from_note_name_sharp() {
        let mut lexer = Lexer::new("F#".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let root = chord.root_note(None).expect("Should resolve without key");
        assert_eq!(root.name, "F#");
        assert_eq!(root.semitone, 6);
    }

    #[test]
    fn test_root_note_from_note_name_flat() {
        let mut lexer = Lexer::new("Bb".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let root = chord.root_note(None).expect("Should resolve without key");
        assert_eq!(root.name, "Bb");
        assert_eq!(root.semitone, 10);
    }

    #[test]
    fn test_notes_major_triad() {
        let mut lexer = Lexer::new("C".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let notes = chord.notes(None).expect("Should resolve");
        assert_eq!(notes.len(), 3);
        assert_eq!(notes[0].name, "C");
        assert_eq!(notes[1].name, "E");
        assert_eq!(notes[2].name, "G");
    }

    #[test]
    fn test_notes_minor_seventh() {
        let mut lexer = Lexer::new("Dm7".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let notes = chord.notes(None).expect("Should resolve");
        assert_eq!(notes.len(), 4);
        assert_eq!(notes[0].name, "D");
        assert_eq!(notes[1].name, "F");
        assert_eq!(notes[2].name, "A");
        assert_eq!(notes[3].name, "C");
    }

    #[test]
    fn test_notes_major_seventh() {
        let mut lexer = Lexer::new("Cmaj7".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let notes = chord.notes(None).expect("Should resolve");
        assert_eq!(notes.len(), 4);
        assert_eq!(notes[0].name, "C");
        assert_eq!(notes[1].name, "E");
        assert_eq!(notes[2].name, "G");
        assert_eq!(notes[3].name, "B");
    }

    #[test]
    fn test_notes_with_sharp_root() {
        let mut lexer = Lexer::new("F#m".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let notes = chord.notes(None).expect("Should resolve");
        assert_eq!(notes.len(), 3);
        assert_eq!(notes[0].name, "F#");
        // F#m: F# (6), A (9), C# (1)
        assert_eq!(notes[1].name, "A");
        assert_eq!(notes[2].name, "C#");
    }

    #[test]
    fn test_notes_with_flat_root() {
        let mut lexer = Lexer::new("Bbmaj7".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let notes = chord.notes(None).expect("Should resolve");
        assert_eq!(notes.len(), 4);
        assert_eq!(notes[0].name, "Bb");
        assert_eq!(notes[1].name, "D");
        assert_eq!(notes[2].name, "F");
        assert_eq!(notes[3].name, "A");
    }

    #[test]
    fn test_notes_dominant_ninth() {
        let mut lexer = Lexer::new("G9".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let notes = chord.notes(None).expect("Should resolve");
        assert_eq!(notes.len(), 5);
        // G9 intervals: G (root=0), B (maj3=4), D (5th=7), F (min7=10), A (9th=14)
        // Sorted by semitone distance (preserving octaves): G, B, D, F, A
        assert_eq!(notes[0].name, "G");
        assert_eq!(notes[1].name, "B");
        assert_eq!(notes[2].name, "D");
        assert_eq!(notes[3].name, "F");
        assert_eq!(notes[4].name, "A");
    }

    #[test]
    fn test_scale_degrees_c_major_in_c_major() {
        let mut lexer = Lexer::new("C".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let key = Key::major(MusicalNote::c());
        let degrees = chord.scale_degrees(&key).expect("Should resolve");

        // C major triad in C major: C=1, E=3, G=5
        assert_eq!(degrees.len(), 3);
        assert_eq!(degrees[0].1, 1); // Root is scale degree 1
        assert_eq!(degrees[1].1, 3); // Third is scale degree 3
        assert_eq!(degrees[2].1, 5); // Fifth is scale degree 5
    }

    #[test]
    fn test_scale_degrees_dm7_in_c_major() {
        let mut lexer = Lexer::new("Dm7".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let key = Key::major(MusicalNote::c());
        let degrees = chord.scale_degrees(&key).expect("Should resolve");

        // Dm7 in C major: D=2, F=4, A=6, C=1
        assert_eq!(degrees.len(), 4);
        assert_eq!(degrees[0].1, 2); // D is scale degree 2
        assert_eq!(degrees[1].1, 4); // F is scale degree 4
        assert_eq!(degrees[2].1, 6); // A is scale degree 6
        assert_eq!(degrees[3].1, 1); // C is scale degree 1
    }

    #[test]
    fn test_scale_degrees_g7_in_c_major() {
        let mut lexer = Lexer::new("G7".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let key = Key::major(MusicalNote::c());
        let degrees = chord.scale_degrees(&key).expect("Should resolve");

        // G7 in C major: G=5, B=7, D=2, F=4
        assert_eq!(degrees.len(), 4);
        assert_eq!(degrees[0].1, 5); // G is scale degree 5
        assert_eq!(degrees[1].1, 7); // B is scale degree 7
        assert_eq!(degrees[2].1, 2); // D is scale degree 2
        assert_eq!(degrees[3].1, 4); // F is scale degree 4
    }

    #[test]
    fn test_scale_degrees_shows_chord_degree() {
        let mut lexer = Lexer::new("Cmaj7".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let key = Key::major(MusicalNote::c());
        let degrees = chord.scale_degrees(&key).expect("Should resolve");

        // Verify we get ChordDegree values
        assert_eq!(degrees.len(), 4);
        assert_eq!(degrees[0].0, ChordDegree::Root);
        assert_eq!(degrees[1].0, ChordDegree::Third);
        assert_eq!(degrees[2].0, ChordDegree::Fifth);
        assert_eq!(degrees[3].0, ChordDegree::Seventh);
    }

    // Transpose tests

    #[test]
    fn test_transpose_major_triad() {
        let mut lexer = Lexer::new("C".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        // Transpose C to G (same scale type)
        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::major(MusicalNote::g());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        assert_eq!(transposed.quality, ChordQuality::Major);
        assert_eq!(transposed.root_note(None).unwrap().name, "G");
        assert_eq!(transposed.to_string(), "G");
    }

    #[test]
    fn test_transpose_minor_seventh() {
        let mut lexer = Lexer::new("Dm7".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        // Transpose Dm7 to Am7 (same scale type - major)
        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::major(MusicalNote::a());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        assert_eq!(transposed.quality, ChordQuality::Minor);
        assert_eq!(transposed.family, Some(ChordFamily::Minor7));
        // Note: root will be A in the target key
        assert_eq!(transposed.to_string(), "Am7");
    }

    #[test]
    fn test_transpose_major_seventh() {
        let mut lexer = Lexer::new("Cmaj7".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        // Transpose Cmaj7 to F#maj7 (same scale type)
        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::major(MusicalNote::from_string("F#").unwrap());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        assert_eq!(transposed.quality, ChordQuality::Major);
        assert_eq!(transposed.family, Some(ChordFamily::Major7));
        assert_eq!(transposed.root_note(None).unwrap().name, "F#");
        assert_eq!(transposed.to_string(), "F#maj7");
    }

    #[test]
    fn test_transpose_dominant_ninth() {
        let mut lexer = Lexer::new("G9".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        // Transpose G9 to D9 (same scale type)
        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::major(MusicalNote::d());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        assert_eq!(transposed.quality, ChordQuality::Major);
        assert_eq!(transposed.family, Some(ChordFamily::Dominant7));
        assert!(transposed.extensions.has_any());
        assert_eq!(transposed.root_note(None).unwrap().name, "D");
        assert_eq!(transposed.to_string(), "D9");
    }

    #[test]
    fn test_transpose_altered_chord() {
        let mut lexer = Lexer::new("C7#5".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        // Transpose C7#5 to Bb7#5 (same scale type)
        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::major(MusicalNote::from_string("Bb").unwrap());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        assert_eq!(transposed.quality, ChordQuality::Major);
        assert_eq!(transposed.family, Some(ChordFamily::Dominant7));
        assert_eq!(transposed.alterations.len(), 1);
        assert_eq!(transposed.root_note(None).unwrap().name, "Bb");
        assert_eq!(transposed.to_string(), "Bb7#5");
    }

    #[test]
    fn test_transpose_with_additions() {
        // Use a chord we know parses correctly
        let mut lexer = Lexer::new("Cno5".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        // Transpose Cno5 to Eno5 (same scale type)
        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::major(MusicalNote::e());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        assert_eq!(transposed.quality, ChordQuality::Major);
        assert_eq!(transposed.omissions.len(), 1);
        assert!(transposed.omissions.contains(&ChordDegree::Fifth));
        assert_eq!(transposed.root_note(None).unwrap().name, "E");
        assert_eq!(transposed.to_string(), "Eno5");
    }

    #[test]
    fn test_transpose_sixth_chord() {
        let mut lexer = Lexer::new("C6".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        // Transpose C6 to A6 (same scale type)
        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::major(MusicalNote::a());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        assert_eq!(transposed.quality, ChordQuality::Major);
        assert!(transposed.additions.contains(&ChordDegree::Sixth));
        assert_eq!(transposed.root_note(None).unwrap().name, "A");
        assert_eq!(transposed.to_string(), "A6"); // Major sixth chords display as "6"
    }

    #[test]
    fn test_transpose_complex_chord() {
        let mut lexer = Lexer::new("Cmaj13".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        // Transpose Cmaj13 to Ebmaj13 (same scale type)
        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::major(MusicalNote::from_string("Eb").unwrap());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        assert_eq!(transposed.quality, ChordQuality::Major);
        assert_eq!(transposed.family, Some(ChordFamily::Major7));
        assert!(transposed.extensions.has_any());
        assert_eq!(transposed.root_note(None).unwrap().name, "Eb");
        // Verify the structure is preserved
        assert_eq!(format!("{}", transposed), "Ebmaj13");
    }

    #[test]
    fn test_transpose_enharmonic_spelling() {
        let mut lexer = Lexer::new("C".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        // Transpose C to Db (same scale type)
        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::major(MusicalNote::from_string("Db").unwrap());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        // Check that notes use correct enharmonic spelling for Db major
        let notes = transposed.notes(None).expect("Should resolve");
        assert_eq!(notes[0].name, "Db");
        assert_eq!(notes[1].name, "F");
        assert_eq!(notes[2].name, "Ab");
    }

    // New tests for scale type changes

    #[test]
    fn test_transpose_scale_type_c_major_to_c_minor() {
        // C major triad in C Major → Cm triad in C Minor
        let mut lexer = Lexer::new("C".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::minor(MusicalNote::c());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        assert_eq!(transposed.quality, ChordQuality::Minor);
        assert_eq!(transposed.root_note(None).unwrap().name, "C");
        assert_eq!(transposed.to_string(), "Cm");
    }

    #[test]
    fn test_transpose_scale_type_em_to_ebmaj() {
        // Em in C Major (iii chord) → Eb in C Minor (iii chord)
        let mut lexer = Lexer::new("Em".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::minor(MusicalNote::c());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        // In C minor, the iii chord is Eb major (Eb-G-Bb)
        assert_eq!(transposed.root_note(None).unwrap().name, "Eb");
        assert_eq!(transposed.quality, ChordQuality::Major);
    }

    #[test]
    fn test_transpose_to_harmonic_minor() {
        use crate::key::ScaleMode;

        // C major in C Major → C harmonic minor
        let mut lexer = Lexer::new("C".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::new(MusicalNote::c(), ScaleMode::harmonic_minor());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        // C harmonic minor has a minor third, so C → Cm
        assert_eq!(transposed.root_note(None).unwrap().name, "C");
        assert_eq!(transposed.quality, ChordQuality::Minor);
        assert_eq!(transposed.to_string(), "Cm");
    }

    #[test]
    fn test_transpose_both_root_and_scale_type() {
        // C major in C Major → Gm in G Minor
        let mut lexer = Lexer::new("C".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::minor(MusicalNote::g());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        assert_eq!(transposed.quality, ChordQuality::Minor);
        assert_eq!(transposed.root_note(None).unwrap().name, "G");
        assert_eq!(transposed.to_string(), "Gm");
    }

    #[test]
    fn test_transpose_complex_chord_with_scale_type_change() {
        use crate::key::ScaleMode;

        // Cmaj13 in C Major → Cm(maj13) in C Harmonic Minor
        // C Major scale: C D E F G A B
        // Cmaj13: 1=C, 3=E, 5=G, 7=B, 9=D, 11=F, 13=A
        // Notes: C E G B D F A
        //
        // C Harmonic Minor scale: C D Eb F G Ab B
        // Counting from C (degree 1):
        // 1=C, 3=Eb, 5=G, 7=B, 9=D, 11=F, 13=Ab
        // Notes: C Eb G B D F Ab
        // This is Cm(maj13) - minor triad with major 7th and extensions

        let mut lexer = Lexer::new("Cmaj13".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::new(MusicalNote::c(), ScaleMode::harmonic_minor());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        // Verify root stayed C
        assert_eq!(transposed.root_note(None).unwrap().name, "C");

        // Verify quality changed from Major to Minor (due to Eb instead of E)
        assert_eq!(transposed.quality, ChordQuality::Minor);

        // Verify it has major 7th family (B natural, not Bb)
        assert_eq!(transposed.family, Some(ChordFamily::MinorMajor7));

        // Verify extensions are present
        assert!(transposed.extensions.has_any());

        // Verify the actual notes (sorted by semitone distance from root, preserving octaves)
        let notes = transposed
            .notes(Some(&target_key))
            .expect("Should resolve notes");
        let note_names: Vec<&str> = notes.iter().map(|n| n.name.as_str()).collect();

        // Extensions are transformed by flattening to scale degree, transforming, then preserving octave
        // C harmonic minor: C D Eb F G Ab B
        // Cmaj13 (C E G B D F A) → Cm(maj13) (C Eb G B D F Ab)
        // Sorted with octaves preserved: C(0) Eb(3) G(7) B(11) D(14) F(17) Ab(20)
        assert_eq!(note_names.len(), 7);
        assert_eq!(note_names[0], "C"); // Root (0 semitones)
        assert_eq!(note_names[1], "Eb"); // Minor third (3 semitones) - E→Eb via scale
        assert_eq!(note_names[2], "G"); // Perfect fifth (7 semitones) - unchanged
        assert_eq!(note_names[3], "B"); // Major seventh (11 semitones) - unchanged
        assert_eq!(note_names[4], "D"); // Ninth (14 semitones = 2 + 12) - unchanged
        assert_eq!(note_names[5], "F"); // Eleventh (17 semitones = 5 + 12) - unchanged
        assert_eq!(note_names[6], "Ab"); // Thirteenth (20 semitones = 8 + 12) - A→Ab via scale
    }

    #[test]
    fn test_transpose_non_root_13th_with_root_and_scale_change() {
        use crate::key::ScaleMode;

        // G13 (V13) in C Major → A13 (V13) in D Harmonic Minor
        // This tests:
        // 1. Root transposition: G → A (2 semitones up, C to D)
        // 2. Scale type change: Major → Harmonic Minor
        // 3. Non-root chord in scale (V chord, scale degree 5)
        // 4. Extension transformation through the new scale
        //
        // G13 in C Major: G B D F A C E
        // - G is scale degree 5 in C Major (C D E F G A B)
        // - Chord: Root=G(0), 3rd=B(4), 5th=D(7), 7th=F(10), 9th=A(14), 11th=C(17), 13th=E(20)
        //
        // A13 in D Harmonic Minor: A C# E G Bb D F
        // - D Harmonic Minor scale: D E F G A Bb C#
        // - A is scale degree 5 in D Harmonic Minor
        // - After scale transformation (mapping through scale degrees from A):
        //   - Chord root (scale deg 1 from A) → A
        //   - 3rd (scale deg 3 from A) → C# (not C, because of harmonic minor raised 7th)
        //   - 5th (scale deg 5 from A) → E
        //   - 7th (scale deg 7 from A) → G
        //   - 9th (scale deg 2 from A, next octave) → Bb (not B)
        //   - 11th (scale deg 4 from A, next octave) → D
        //   - 13th (scale deg 6 from A, next octave) → F
        // - Chord: Root=A(0), 3rd=C#(4), 5th=E(7), 7th=G(10), 9th=Bb(13), 11th=D(17), 13th=F(20)

        let mut lexer = Lexer::new("G13".to_string());
        let tokens = lexer.tokenize();
        let chord = Chord::parse(&tokens).unwrap();

        let source_key = Key::major(MusicalNote::c());
        let target_key = Key::new(MusicalNote::d(), ScaleMode::harmonic_minor());
        let transposed = chord
            .transpose_to(&target_key, Some(&source_key))
            .expect("Should transpose");

        // Verify root transposed from G to A
        assert_eq!(transposed.root_note(None).unwrap().name, "A");

        // Verify quality is Dominant (major triad with minor 7th)
        assert_eq!(transposed.quality, ChordQuality::Major);
        assert_eq!(transposed.family, Some(ChordFamily::Dominant7));

        // Verify extensions are present
        assert!(transposed.extensions.has_any());

        // Verify the actual notes (sorted by semitone distance, preserving octaves)
        let notes = transposed
            .notes(Some(&target_key))
            .expect("Should resolve notes");
        let note_names: Vec<&str> = notes.iter().map(|n| n.name.as_str()).collect();

        // D Harmonic Minor: D E F G A Bb C#
        // A13 chord in D Harmonic Minor:
        // Root (A), Major 3rd (C#), Perfect 5th (E), Minor 7th (G), 9th (Bb), 11th (D), 13th (F)
        // Sorted by semitone: A(0) C#(4) E(7) G(10) Bb(13) D(17) F(20)
        assert_eq!(note_names.len(), 7);
        assert_eq!(note_names[0], "A"); // Root (0 semitones)
        assert_eq!(note_names[1], "C#"); // Major third (4 semitones)
        assert_eq!(note_names[2], "E"); // Perfect fifth (7 semitones)
        assert_eq!(note_names[3], "G"); // Minor seventh (10 semitones)
        assert_eq!(note_names[4], "Bb"); // Ninth (13 semitones = 1 + 12)
        assert_eq!(note_names[5], "D"); // Eleventh (17 semitones = 5 + 12)
        assert_eq!(note_names[6], "F"); // Thirteenth (20 semitones = 8 + 12)

        // Verify the chord displays correctly
        // After scale transformation, all extensions are Natural in the new scale context
        // So the highest extension (13) masks the 7th, displaying as "A13"
        assert_eq!(transposed.to_string(), "A13");
    }
}

// endregion: --- Tests
