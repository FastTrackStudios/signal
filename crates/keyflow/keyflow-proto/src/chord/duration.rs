//! Chord duration and rhythm parsing
//!
//! Implements various rhythm notation systems for chords:
//! - Slash syntax (/, //, ///)
//! - Lily-inspired syntax (_4, _8., _2)
//! - Push/Pull notation ('C, C')
//! - Ties and rests (r, s, ~)

use crate::core::duration::{NotationDuration, NoteValue, RhythmType, TupletRatio};
use crate::parsing::{ParseError, Token, TokenType};
use crate::time::{MusicalDuration, MusicalPositionExt, TimeSignature};
use facet::Facet;
use tracing::instrument;

// Re-export core time signature for use in notation conversions
use crate::core::time::TimeSignature as CoreTimeSignature;

/// Rhythm notation for a chord
///
/// This enum represents the input notation for rhythm, which is then converted
/// to [`NotationDuration`] for unified duration calculations.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ChordRhythm {
    /// Default - one bar (implied if no rhythm specified)
    Default,

    /// Slash notation - each slash is one beat (/, //, ///, ////)
    /// Can be dotted (/., //., etc.) and/or tied
    Slashes { count: u8, dotted: bool, tied: bool },

    /// Explicit duration (chord, rest, or space) using NotationDuration
    /// This unified representation supports dots, ties, triplets, and multipliers
    Explicit(NotationDuration),
}

/// Lily-inspired duration values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum LilySyntax {
    Whole,        // 1
    Half,         // 2
    Quarter,      // 4
    Eighth,       // 8
    Sixteenth,    // 16
    ThirtySecond, // 32
}

impl LilySyntax {
    /// Get the numeric value (1, 2, 4, 8, 16, 32)
    pub fn value(&self) -> u8 {
        match self {
            LilySyntax::Whole => 1,
            LilySyntax::Half => 2,
            LilySyntax::Quarter => 4,
            LilySyntax::Eighth => 8,
            LilySyntax::Sixteenth => 16,
            LilySyntax::ThirtySecond => 32,
        }
    }

    /// Parse from number string ("1", "2", "4", "8", "16", "32")
    pub fn from_number(s: &str) -> Option<Self> {
        match s {
            "1" => Some(LilySyntax::Whole),
            "2" => Some(LilySyntax::Half),
            "4" => Some(LilySyntax::Quarter),
            "8" => Some(LilySyntax::Eighth),
            "16" => Some(LilySyntax::Sixteenth),
            "32" => Some(LilySyntax::ThirtySecond),
            _ => None,
        }
    }
}

// region:    --- NoteValue Conversions

// Note: NoteValue is already imported at the top from crate::core::duration

impl From<LilySyntax> for NoteValue {
    fn from(lily: LilySyntax) -> Self {
        match lily {
            LilySyntax::Whole => NoteValue::Whole,
            LilySyntax::Half => NoteValue::Half,
            LilySyntax::Quarter => NoteValue::Quarter,
            LilySyntax::Eighth => NoteValue::Eighth,
            LilySyntax::Sixteenth => NoteValue::Sixteenth,
            LilySyntax::ThirtySecond => NoteValue::ThirtySecond,
        }
    }
}

impl From<NoteValue> for LilySyntax {
    fn from(note: NoteValue) -> Self {
        match note {
            NoteValue::Whole => LilySyntax::Whole,
            NoteValue::Half => LilySyntax::Half,
            NoteValue::Quarter => LilySyntax::Quarter,
            NoteValue::Eighth => LilySyntax::Eighth,
            NoteValue::Sixteenth => LilySyntax::Sixteenth,
            NoteValue::ThirtySecond => LilySyntax::ThirtySecond,
            // SixtyFourth is not supported by LilySyntax, use ThirtySecond as fallback
            NoteValue::SixtyFourth => LilySyntax::ThirtySecond,
        }
    }
}

// endregion: --- NoteValue Conversions

/// Base subdivision for push/pull timing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet, Default)]
#[repr(u8)]
pub enum PushPullBase {
    /// Standard (binary) subdivision
    #[default]
    Standard,
    /// Triplet subdivision (3 in the space of 2)
    Triplet,
    /// Arbitrary tuplet (n in the space of the next lower power of 2)
    Tuplet(u8),
    /// Explicit duration (e.g., '_4 for quarter note, '_8 for eighth)
    /// Fields: (duration, dotted, triplet)
    Duration {
        duration: LilySyntax,
        dotted: bool,
        triplet: bool,
    },
}

impl PushPullBase {
    /// Get the multiplier for this base
    /// For triplets: 2/3 (triplet eighth = 2/3 of regular eighth)
    /// For tuplets: calculates based on tuplet number
    /// For Duration: returns 1.0 (beats are calculated directly from duration)
    pub fn multiplier(&self) -> f64 {
        match self {
            PushPullBase::Standard => 1.0,
            PushPullBase::Triplet => 2.0 / 3.0,
            PushPullBase::Tuplet(n) => {
                // For a quintuplet (5): 4/5 (5 notes in space of 4)
                // For a septuplet (7): 4/7 (7 notes in space of 4)
                // General: next lower power of 2 / n
                let power_of_2 = (*n as f64).log2().floor() as u8;
                let base = 2u8.pow(power_of_2 as u32) as f64;
                base / (*n as f64)
            }
            PushPullBase::Duration { .. } => 1.0, // Not used for Duration
        }
    }

    /// Get the beat value directly for Duration base (in 4/4 time)
    /// Returns None for non-Duration bases
    pub fn duration_beats(&self) -> Option<f64> {
        match self {
            PushPullBase::Duration {
                duration,
                dotted,
                triplet,
            } => {
                // Calculate beats based on duration (assuming 4/4 time, quarter = 1 beat)
                let base_beats = match duration {
                    LilySyntax::Whole => 4.0,
                    LilySyntax::Half => 2.0,
                    LilySyntax::Quarter => 1.0,
                    LilySyntax::Eighth => 0.5,
                    LilySyntax::Sixteenth => 0.25,
                    LilySyntax::ThirtySecond => 0.125,
                };
                let dotted_beats = if *dotted {
                    base_beats * 1.5
                } else {
                    base_beats
                };
                let triplet_beats = if *triplet {
                    dotted_beats * 2.0 / 3.0
                } else {
                    dotted_beats
                };
                Some(triplet_beats)
            }
            _ => None,
        }
    }
}

/// Push/Pull amount (number of apostrophes + optional base)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
pub struct PushPullAmount {
    /// The subdivision level (1 = eighth, 2 = sixteenth, 3 = thirty-second)
    pub level: u8,
    /// The base timing (standard, triplet, or custom tuplet)
    pub base: PushPullBase,
}

impl PushPullAmount {
    /// Create from apostrophe count with standard (binary) timing
    pub fn from_count(count: u8) -> Option<Self> {
        if count == 0 || count > 3 {
            return None;
        }
        Some(Self {
            level: count,
            base: PushPullBase::Standard,
        })
    }

    /// Create from apostrophe count with triplet timing
    pub fn from_count_triplet(count: u8) -> Option<Self> {
        if count == 0 || count > 3 {
            return None;
        }
        Some(Self {
            level: count,
            base: PushPullBase::Triplet,
        })
    }

    /// Create from apostrophe count with custom tuplet
    pub fn from_count_tuplet(count: u8, tuplet: u8) -> Option<Self> {
        if count == 0 || count > 3 || tuplet < 3 {
            return None;
        }
        Some(Self {
            level: count,
            base: PushPullBase::Tuplet(tuplet),
        })
    }

    /// Create a single apostrophe amount with a specific base
    pub fn single(base: PushPullBase) -> Self {
        Self { level: 1, base }
    }

    /// Get the beat value for this push/pull amount
    pub fn to_beats(&self) -> f64 {
        // For Duration base, get beats directly from the duration
        if let Some(beats) = self.base.duration_beats() {
            return beats;
        }

        // For level-based amounts (Standard, Triplet, Tuplet)
        let base_beats = match self.level {
            1 => 0.5,   // eighth note
            2 => 0.25,  // sixteenth note
            3 => 0.125, // thirty-second note
            _ => 0.5,   // fallback
        };
        base_beats * self.base.multiplier()
    }

    /// Convenience constructors
    pub fn eighth() -> Self {
        Self::from_count(1).unwrap()
    }

    pub fn sixteenth() -> Self {
        Self::from_count(2).unwrap()
    }

    pub fn thirty_second() -> Self {
        Self::from_count(3).unwrap()
    }

    pub fn eighth_triplet() -> Self {
        Self::from_count_triplet(1).unwrap()
    }

    pub fn sixteenth_triplet() -> Self {
        Self::from_count_triplet(2).unwrap()
    }

    /// Create from explicit duration (e.g., quarter note push)
    pub fn from_duration(duration: LilySyntax, dotted: bool, triplet: bool) -> Self {
        Self {
            level: 1, // Not used for Duration base
            base: PushPullBase::Duration {
                duration,
                dotted,
                triplet,
            },
        }
    }

    /// Quarter note push/pull (1 beat)
    pub fn quarter() -> Self {
        Self::from_duration(LilySyntax::Quarter, false, false)
    }

    /// Half note push/pull (2 beats)
    pub fn half() -> Self {
        Self::from_duration(LilySyntax::Half, false, false)
    }
}

impl std::fmt::Display for PushPullAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.base {
            PushPullBase::Duration {
                duration,
                dotted,
                triplet,
            } => {
                let d = match duration {
                    LilySyntax::Whole => "1",
                    LilySyntax::Half => "2",
                    LilySyntax::Quarter => "4",
                    LilySyntax::Eighth => "8",
                    LilySyntax::Sixteenth => "16",
                    LilySyntax::ThirtySecond => "32",
                };
                write!(f, "{d}")?;
                if *dotted {
                    write!(f, ".")?;
                }
                if *triplet {
                    write!(f, "t")?;
                }
                Ok(())
            }
            _ => {
                let note = match self.level {
                    1 => "8",
                    2 => "16",
                    3 => "32",
                    _ => "?",
                };
                match &self.base {
                    PushPullBase::Standard => write!(f, "{note}"),
                    PushPullBase::Triplet => write!(f, "{note}t"),
                    PushPullBase::Tuplet(n) => write!(f, "{note}:{n}"),
                    PushPullBase::Duration { .. } => unreachable!(),
                }
            }
        }
    }
}

impl ChordRhythm {
    // region:    --- Constructors

    /// Create a new slash rhythm with the given count
    pub fn slashes(count: u8) -> Self {
        ChordRhythm::Slashes {
            count,
            dotted: false,
            tied: false,
        }
    }

    /// Create a chord rhythm from a NotationDuration
    pub fn from_notation(duration: NotationDuration) -> Self {
        ChordRhythm::Explicit(duration)
    }

    /// Create a Lily-style chord duration
    pub fn lily(
        duration: LilySyntax,
        dotted: bool,
        triplet: bool,
        tied: bool,
        multiplier: Option<u16>,
    ) -> Self {
        let mut nd = NotationDuration::new(duration.into());
        nd.dots = if dotted { 1 } else { 0 };
        nd.tied = tied;
        nd.multiplier = multiplier;
        nd.rhythm_type = RhythmType::Chord;
        if triplet {
            nd.tuplet = Some(TupletRatio::TRIPLET);
        }
        ChordRhythm::Explicit(nd)
    }

    /// Create a rest rhythm
    pub fn rest(
        duration: LilySyntax,
        dotted: bool,
        triplet: bool,
        multiplier: Option<u16>,
    ) -> Self {
        let mut nd = NotationDuration::new(duration.into());
        nd.dots = if dotted { 1 } else { 0 };
        nd.multiplier = multiplier;
        nd.rhythm_type = RhythmType::Rest;
        if triplet {
            nd.tuplet = Some(TupletRatio::TRIPLET);
        }
        ChordRhythm::Explicit(nd)
    }

    /// Create a space rhythm
    pub fn space(
        duration: LilySyntax,
        dotted: bool,
        triplet: bool,
        multiplier: Option<u16>,
    ) -> Self {
        let mut nd = NotationDuration::new(duration.into());
        nd.dots = if dotted { 1 } else { 0 };
        nd.multiplier = multiplier;
        nd.rhythm_type = RhythmType::Space;
        if triplet {
            nd.tuplet = Some(TupletRatio::TRIPLET);
        }
        ChordRhythm::Explicit(nd)
    }

    // endregion: --- Constructors

    // region:    --- Modifiers

    /// Add a tie to this rhythm
    pub fn with_tie(self) -> Self {
        match self {
            ChordRhythm::Default => {
                // Convert to explicit slashes with tie
                ChordRhythm::Slashes {
                    count: 4, // Default is 4 beats in 4/4
                    dotted: false,
                    tied: true,
                }
            }
            ChordRhythm::Slashes { count, dotted, .. } => ChordRhythm::Slashes {
                count,
                dotted,
                tied: true,
            },
            ChordRhythm::Explicit(mut nd) => {
                nd.tied = true;
                ChordRhythm::Explicit(nd)
            }
        }
    }

    /// Check if this rhythm is tied
    pub fn is_tied(&self) -> bool {
        match self {
            ChordRhythm::Default => false,
            ChordRhythm::Slashes { tied, .. } => *tied,
            ChordRhythm::Explicit(nd) => nd.tied,
        }
    }

    /// Clear the tie flag
    pub fn clear_tie(&mut self) {
        match self {
            ChordRhythm::Default => {}
            ChordRhythm::Slashes { tied, .. } => *tied = false,
            ChordRhythm::Explicit(nd) => nd.tied = false,
        }
    }

    // endregion: --- Modifiers

    // region:    --- Query Methods

    /// Extract lily duration parts (duration, dotted, triplet) if available.
    ///
    /// Returns `Some((LilySyntax, dotted, triplet))` for Explicit variants with note value.
    /// Returns `None` for Default and Slashes variants.
    ///
    /// This method is useful for layout engines that need to convert rhythm
    /// to visual duration without nested pattern matching.
    pub fn lily_parts(&self) -> Option<(LilySyntax, bool, bool)> {
        match self {
            ChordRhythm::Explicit(nd) => {
                let lily: LilySyntax = nd.note_value.into();
                let dotted = nd.dots > 0;
                let triplet = nd.is_triplet();
                Some((lily, dotted, triplet))
            }
            ChordRhythm::Default | ChordRhythm::Slashes { .. } => None,
        }
    }

    /// Check if this rhythm is a rest variant.
    pub fn is_rest(&self) -> bool {
        matches!(
            self,
            ChordRhythm::Explicit(nd) if nd.rhythm_type == RhythmType::Rest
        )
    }

    /// Check if this rhythm is a space variant.
    pub fn is_space(&self) -> bool {
        matches!(
            self,
            ChordRhythm::Explicit(nd) if nd.rhythm_type == RhythmType::Space
        )
    }

    /// Check if this rhythm has lily-style duration info (explicit notation).
    pub fn has_lily_duration(&self) -> bool {
        matches!(self, ChordRhythm::Explicit(_))
    }

    // endregion: --- Query Methods

    // region:    --- Parsing

    /// Parse chord rhythm from tokens following the chord
    /// Returns (rhythm, tokens_consumed)
    /// Returns error if no valid rhythm notation is found
    #[instrument(level = "debug", skip(tokens), fields(token_count = tokens.len()))]
    pub fn parse(tokens: &[Token]) -> Result<(Self, usize), ParseError> {
        if tokens.is_empty() {
            return Err(ParseError::NoValidParser {
                context: "No rhythm tokens found".to_string(),
            });
        }

        // Check for push notation (leading apostrophes before we get here are handled by chord parser)
        // This handles trailing apostrophes for pull notation - but we don't store them in rhythm anymore
        // Push/pull is stored separately in ChordInstance.push_pull
        if let TokenType::Apostrophe = tokens[0].token_type {
            let count = Self::count_apostrophes(tokens);
            if count > 0 && count <= 3 {
                // Return default rhythm - push/pull is handled elsewhere
                return Err(ParseError::NoValidParser {
                    context: "Pull notation handled by chord parser".to_string(),
                });
            }
        }

        // Check for underscore (Lily syntax) or slash syntax
        match &tokens[0].token_type {
            TokenType::Underscore => {
                // Lily duration: _4, _8., _2, etc.
                let consumed = 1;
                let (rhythm, tokens_used) = Self::parse_lily_duration(&tokens[consumed..])?;
                Ok((rhythm, consumed + tokens_used))
            }

            TokenType::Slash => {
                // Slash notation: /, //, ///, ////, or with dots: /., //., etc.
                let slash_count = Self::count_slashes(tokens);

                // Check if there's a dot after the slashes for dotted rhythm
                let has_dot = if slash_count < tokens.len() {
                    matches!(tokens[slash_count].token_type, TokenType::Dot)
                } else {
                    false
                };

                if has_dot {
                    Ok((
                        ChordRhythm::Slashes {
                            count: slash_count as u8,
                            dotted: true,
                            tied: false,
                        },
                        slash_count + 1, // +1 for the dot
                    ))
                } else {
                    Ok((ChordRhythm::slashes(slash_count as u8), slash_count))
                }
            }

            TokenType::Letter('r') | TokenType::Letter('s') => {
                // Rest or Space: r4, s8, r_4., s1*8
                Self::parse_rest_or_space(tokens)
            }

            _ => {
                // No rhythm tokens recognized
                Err(ParseError::NoValidParser {
                    context: "No valid rhythm notation found".to_string(),
                })
            }
        }
    }

    /// Parse Lily duration after underscore: _4, _8., _8t, _2~, etc.
    /// `tokens` should be the slice starting AFTER the underscore
    fn parse_lily_duration(tokens: &[Token]) -> Result<(Self, usize), ParseError> {
        let mut consumed = 0;

        if tokens.is_empty() {
            return Err(ParseError::NoValidParser {
                context: "Expected duration after underscore".to_string(),
            });
        }

        // Parse the duration number
        let duration = match &tokens[consumed].token_type {
            TokenType::Number(n) => {
                LilySyntax::from_number(n).ok_or(ParseError::NoValidParser {
                    context: format!("Invalid Lily duration: {}", n),
                })?
            }
            _ => {
                return Err(ParseError::NoValidParser {
                    context: "Expected duration number after underscore".to_string(),
                });
            }
        };
        consumed += 1;

        // Check for triplet suffix 't' (e.g., _8t for triplet eighth)
        let triplet = if consumed < tokens.len() {
            if matches!(tokens[consumed].token_type, TokenType::Letter('t')) {
                consumed += 1;
                true
            } else {
                false
            }
        } else {
            false
        };

        // Check for dot
        let dotted = if consumed < tokens.len() {
            if matches!(tokens[consumed].token_type, TokenType::Dot) {
                consumed += 1;
                true
            } else {
                false
            }
        } else {
            false
        };

        // Check for tie (~)
        let tied = if consumed < tokens.len() {
            if matches!(tokens[consumed].token_type, TokenType::Tilde) {
                consumed += 1;
                true
            } else {
                false
            }
        } else {
            false
        };

        // Check for multiplier (*8)
        let multiplier = if consumed < tokens.len() {
            if matches!(tokens[consumed].token_type, TokenType::Asterisk) {
                consumed += 1;
                if consumed < tokens.len() {
                    if let TokenType::Number(n) = &tokens[consumed].token_type {
                        consumed += 1;
                        Some(n.parse::<u16>().unwrap_or(1))
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
        };

        Ok((
            ChordRhythm::lily(duration, dotted, triplet, tied, multiplier),
            consumed,
        ))
    }

    /// Parse rest (r) or space (s): r4, s8, r_4., r8t, s1*8
    fn parse_rest_or_space(tokens: &[Token]) -> Result<(Self, usize), ParseError> {
        let mut consumed = 0;

        let is_rest = matches!(tokens[0].token_type, TokenType::Letter('r'));
        consumed += 1;

        // Check for underscore (optional)
        let has_underscore = if consumed < tokens.len() {
            matches!(tokens[consumed].token_type, TokenType::Underscore)
        } else {
            false
        };

        if has_underscore {
            consumed += 1;
        }

        // Parse duration number
        if consumed >= tokens.len() {
            return Err(ParseError::NoValidParser {
                context: "Expected duration after rest/space".to_string(),
            });
        }

        let duration = match &tokens[consumed].token_type {
            TokenType::Number(n) => {
                LilySyntax::from_number(n).ok_or(ParseError::NoValidParser {
                    context: format!("Invalid duration: {}", n),
                })?
            }
            _ => {
                return Err(ParseError::NoValidParser {
                    context: "Expected duration number".to_string(),
                });
            }
        };
        consumed += 1;

        // Check for triplet suffix 't' (e.g., r8t for triplet eighth rest)
        let triplet = if consumed < tokens.len() {
            if matches!(tokens[consumed].token_type, TokenType::Letter('t')) {
                consumed += 1;
                true
            } else {
                false
            }
        } else {
            false
        };

        // Check for dot
        let dotted = if consumed < tokens.len() {
            if matches!(tokens[consumed].token_type, TokenType::Dot) {
                consumed += 1;
                true
            } else {
                false
            }
        } else {
            false
        };

        // Check for multiplier (*8)
        let multiplier = if consumed < tokens.len() {
            if matches!(tokens[consumed].token_type, TokenType::Asterisk) {
                consumed += 1;
                if consumed < tokens.len() {
                    if let TokenType::Number(n) = &tokens[consumed].token_type {
                        consumed += 1;
                        Some(n.parse::<u16>().unwrap_or(1))
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
        };

        if is_rest {
            Ok((
                ChordRhythm::rest(duration, dotted, triplet, multiplier),
                consumed,
            ))
        } else {
            Ok((
                ChordRhythm::space(duration, dotted, triplet, multiplier),
                consumed,
            ))
        }
    }

    /// Count consecutive slashes
    fn count_slashes(tokens: &[Token]) -> usize {
        let mut count = 0;
        for token in tokens {
            if matches!(token.token_type, TokenType::Slash) {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    /// Count consecutive apostrophes
    fn count_apostrophes(tokens: &[Token]) -> usize {
        let mut count = 0;
        for token in tokens {
            if matches!(token.token_type, TokenType::Apostrophe) {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    // endregion: --- Parsing

    // region:    --- Duration Conversion

    /// Helper to convert daw TimeSignature (i32) to core TimeSignature (u8)
    fn to_core_time_sig(time_sig: TimeSignature) -> CoreTimeSignature {
        CoreTimeSignature::new(
            time_sig.numerator.clamp(1, 255) as u8,
            time_sig.denominator.clamp(1, 255) as u8,
        )
    }

    /// Convert to NotationDuration (the unified representation)
    pub fn to_notation(&self, time_sig: TimeSignature) -> NotationDuration {
        let core_ts = Self::to_core_time_sig(time_sig);

        match self {
            ChordRhythm::Default => NotationDuration::one_measure(core_ts),
            ChordRhythm::Slashes {
                count,
                dotted,
                tied,
            } => {
                let mut nd = NotationDuration::new(NoteValue::Quarter);
                nd.rhythm_type = RhythmType::Slashes(*count);
                nd.dots = if *dotted { 1 } else { 0 };
                nd.tied = *tied;
                nd
            }
            ChordRhythm::Explicit(nd) => *nd,
        }
    }

    /// Convert to MusicalDuration based on time signature
    pub fn to_duration(&self, time_sig: TimeSignature) -> MusicalDuration {
        match self {
            ChordRhythm::Default => {
                // Default is one full measure
                MusicalDuration::new(1, 0, 0)
            }
            ChordRhythm::Slashes { count, dotted, .. } => {
                // Each slash is one beat (in the time signature denominator)
                let base_beats = f64::from(*count);
                let total_beats = if *dotted {
                    base_beats * 1.5
                } else {
                    base_beats
                };
                MusicalDuration::from_beats(total_beats, time_sig)
            }
            ChordRhythm::Explicit(nd) => {
                // Convert using core time signature
                let beats = nd.to_beats(crate::core::time::TimeSignature::new(
                    time_sig.numerator() as u8,
                    time_sig.denominator() as u8,
                ));
                MusicalDuration::from_beats(beats, time_sig)
            }
        }
    }

    /// Convert Lily syntax duration to beats (helper for backward compatibility)
    pub fn lily_to_beats(duration: LilySyntax, dotted: bool, time_sig: TimeSignature) -> f64 {
        // The denominator tells us what note value gets one beat
        // For 4/4: quarter note = 1 beat
        // For 6/8: eighth note = 1 beat

        // First, calculate how many 32nd notes this duration has
        let thirty_seconds = match duration {
            LilySyntax::Whole => 32.0,
            LilySyntax::Half => 16.0,
            LilySyntax::Quarter => 8.0,
            LilySyntax::Eighth => 4.0,
            LilySyntax::Sixteenth => 2.0,
            LilySyntax::ThirtySecond => 1.0,
        };

        let actual_thirty_seconds = if dotted {
            thirty_seconds * 1.5
        } else {
            thirty_seconds
        };

        // Now convert to beats based on the time signature denominator
        // denominator = 4 means quarter note = 1 beat (8 thirty-seconds)
        // denominator = 8 means eighth note = 1 beat (4 thirty-seconds)
        let thirty_seconds_per_beat = 32.0 / time_sig.denominator as f64;

        actual_thirty_seconds / thirty_seconds_per_beat
    }

    // endregion: --- Duration Conversion
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::Lexer;

    #[test]
    fn test_parse_slash_rhythm() {
        let mut lexer = Lexer::new("////".to_string());
        let tokens = lexer.tokenize();
        let (rhythm, consumed) = ChordRhythm::parse(&tokens).unwrap();

        assert_eq!(rhythm, ChordRhythm::slashes(4));
        assert_eq!(consumed, 4);
    }

    #[test]
    fn test_parse_lilypond_quarter() {
        let mut lexer = Lexer::new("_4".to_string());
        let tokens = lexer.tokenize();
        let (rhythm, consumed) = ChordRhythm::parse(&tokens).unwrap();

        if let Some((lily, dotted, _triplet)) = rhythm.lily_parts() {
            assert_eq!(lily, LilySyntax::Quarter);
            assert!(!dotted);
        } else {
            panic!("Expected explicit rhythm with lily parts");
        }
        assert_eq!(consumed, 2);
    }

    #[test]
    fn test_parse_lilypond_dotted() {
        let mut lexer = Lexer::new("_4.".to_string());
        let tokens = lexer.tokenize();
        let (rhythm, _consumed) = ChordRhythm::parse(&tokens).unwrap();

        if let Some((lily, dotted, _triplet)) = rhythm.lily_parts() {
            assert_eq!(lily, LilySyntax::Quarter);
            assert!(dotted);
        } else {
            panic!("Expected explicit rhythm with lily parts");
        }
    }

    #[test]
    fn test_parse_rest() {
        let mut lexer = Lexer::new("r4".to_string());
        let tokens = lexer.tokenize();
        let (rhythm, _) = ChordRhythm::parse(&tokens).unwrap();

        assert!(rhythm.is_rest());
        if let Some((lily, _dotted, _triplet)) = rhythm.lily_parts() {
            assert_eq!(lily, LilySyntax::Quarter);
        } else {
            panic!("Expected explicit rhythm with lily parts");
        }
    }

    #[test]
    fn test_parse_space_with_multiplier() {
        let mut lexer = Lexer::new("s1*8".to_string());
        let tokens = lexer.tokenize();
        let (rhythm, _) = ChordRhythm::parse(&tokens).unwrap();

        assert!(rhythm.is_space());
        if let ChordRhythm::Explicit(nd) = rhythm {
            assert_eq!(nd.note_value, NoteValue::Whole);
            assert_eq!(nd.multiplier, Some(8));
        } else {
            panic!("Expected explicit rhythm");
        }
    }

    #[test]
    fn test_no_rhythm_returns_error() {
        let tokens = vec![];
        let result = ChordRhythm::parse(&tokens);

        assert!(result.is_err());
    }

    #[test]
    fn test_lily_parts_extraction() {
        // Explicit Lily variant
        let rhythm = ChordRhythm::lily(LilySyntax::Quarter, true, false, false, None);
        assert_eq!(
            rhythm.lily_parts(),
            Some((LilySyntax::Quarter, true, false))
        );

        // Rest variant with triplet
        let rhythm = ChordRhythm::rest(LilySyntax::Eighth, false, true, None);
        assert_eq!(rhythm.lily_parts(), Some((LilySyntax::Eighth, false, true)));

        // Default has no lily parts
        assert_eq!(ChordRhythm::Default.lily_parts(), None);

        // Slashes has no lily parts
        assert_eq!(ChordRhythm::slashes(2).lily_parts(), None);
    }

    #[test]
    fn test_rhythm_type_checks() {
        let rest = ChordRhythm::rest(LilySyntax::Quarter, false, false, None);
        assert!(rest.is_rest());
        assert!(!rest.is_space());
        assert!(rest.has_lily_duration());

        let space = ChordRhythm::space(LilySyntax::Half, false, false, None);
        assert!(!space.is_rest());
        assert!(space.is_space());
        assert!(space.has_lily_duration());

        assert!(!ChordRhythm::Default.has_lily_duration());
        assert!(!ChordRhythm::slashes(4).has_lily_duration());
    }

    #[test]
    fn test_tie_handling() {
        // Test with_tie on slashes
        let rhythm = ChordRhythm::slashes(2);
        assert!(!rhythm.is_tied());
        let tied = rhythm.with_tie();
        assert!(tied.is_tied());

        // Test with_tie on explicit
        let rhythm = ChordRhythm::lily(LilySyntax::Half, false, false, false, None);
        assert!(!rhythm.is_tied());
        let tied = rhythm.with_tie();
        assert!(tied.is_tied());

        // Test clear_tie
        let mut rhythm = ChordRhythm::slashes(2).with_tie();
        assert!(rhythm.is_tied());
        rhythm.clear_tie();
        assert!(!rhythm.is_tied());
    }
}
