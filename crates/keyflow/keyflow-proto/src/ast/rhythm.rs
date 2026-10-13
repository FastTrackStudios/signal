//! Rhythm AST types.
//!
//! Intermediate representation for rhythm/duration notation that preserves
//! the original notation style (slashes, lily syntax, push/pull).

use super::span::AstNode;
use crate::parsing::TextSpan;
use facet::Facet;

/// Abstract syntax tree for rhythm notation.
///
/// Captures the syntactic structure of rhythm notation without computing
/// actual durations. This allows different interpretation strategies
/// depending on context (time signature, tempo, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct RhythmAst {
    /// The kind of rhythm notation
    pub kind: RhythmKind,
    /// Full span covering the rhythm notation
    pub span: TextSpan,
}

impl RhythmAst {
    /// Create a new rhythm AST.
    #[must_use]
    pub const fn new(kind: RhythmKind, span: TextSpan) -> Self {
        Self { kind, span }
    }

    /// Create a default rhythm (one measure).
    #[must_use]
    pub fn default_rhythm() -> Self {
        Self::new(RhythmKind::Default, TextSpan::empty(0))
    }

    /// Create a slash rhythm.
    #[must_use]
    pub fn slashes(count: u8, span: TextSpan) -> Self {
        Self::new(RhythmKind::Slashes(SlashCountAst(count)), span)
    }

    /// Create a lily-style duration.
    #[must_use]
    pub fn duration(duration: DurationAst, span: TextSpan) -> Self {
        Self::new(RhythmKind::Duration(duration), span)
    }

    /// Create a rest.
    #[must_use]
    pub fn rest(duration: DurationAst, span: TextSpan) -> Self {
        Self::new(RhythmKind::Rest(duration), span)
    }

    /// Create a space (invisible rest).
    #[must_use]
    pub fn space(duration: DurationAst, span: TextSpan) -> Self {
        Self::new(RhythmKind::Space(duration), span)
    }

    /// Check if this is a chord rhythm (not rest/space).
    #[must_use]
    pub fn is_sounding(&self) -> bool {
        matches!(
            self.kind,
            RhythmKind::Default | RhythmKind::Slashes(_) | RhythmKind::Duration(_)
        )
    }

    /// Check if this is a rest.
    #[must_use]
    pub fn is_rest(&self) -> bool {
        matches!(self.kind, RhythmKind::Rest(_))
    }

    /// Check if this is a space.
    #[must_use]
    pub fn is_space(&self) -> bool {
        matches!(self.kind, RhythmKind::Space(_))
    }
}

impl AstNode for RhythmAst {
    fn span(&self) -> TextSpan {
        self.span
    }
}

/// Kind of rhythm notation.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum RhythmKind {
    /// Default duration (one measure, no explicit notation)
    Default,

    /// Slash notation (/, //, ///, ////)
    Slashes(SlashCountAst),

    /// Lily-style duration (_4, _8., _2, etc.)
    Duration(DurationAst),

    /// Rest notation (r4, r8, etc.)
    Rest(DurationAst),

    /// Space/tacet notation (s4, s8, etc.)
    Space(DurationAst),

    /// Push notation - anticipate (play earlier)
    Push(PushPullAst),

    /// Pull notation - delay (play later)
    Pull(PushPullAst),
}

/// Number of slashes in slash notation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
pub struct SlashCountAst(pub u8);

impl SlashCountAst {
    /// Get the number of beats this represents.
    ///
    /// Each slash typically represents one beat.
    #[must_use]
    pub const fn beats(&self) -> u8 {
        self.0
    }
}

/// Duration in lily-style notation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
pub struct DurationAst {
    /// The base note value (1=whole, 2=half, 4=quarter, 8=eighth, 16=sixteenth, 32=thirty-second)
    pub base: u8,
    /// Number of dots (0, 1, or 2)
    pub dots: u8,
    /// Whether this is a triplet duration
    pub triplet: bool,
    /// Optional multiplier (for s1*8 syntax)
    pub multiplier: Option<u16>,
    /// Whether this is tied to the next duration
    pub tied: bool,
}

impl DurationAst {
    /// Create a new duration.
    #[must_use]
    pub const fn new(base: u8) -> Self {
        Self {
            base,
            dots: 0,
            triplet: false,
            multiplier: None,
            tied: false,
        }
    }

    /// Whole note duration.
    #[must_use]
    pub const fn whole() -> Self {
        Self::new(1)
    }

    /// Half note duration.
    #[must_use]
    pub const fn half() -> Self {
        Self::new(2)
    }

    /// Quarter note duration.
    #[must_use]
    pub const fn quarter() -> Self {
        Self::new(4)
    }

    /// Eighth note duration.
    #[must_use]
    pub const fn eighth() -> Self {
        Self::new(8)
    }

    /// Sixteenth note duration.
    #[must_use]
    pub const fn sixteenth() -> Self {
        Self::new(16)
    }

    /// Thirty-second note duration.
    #[must_use]
    pub const fn thirty_second() -> Self {
        Self::new(32)
    }

    /// Add a dot to this duration.
    #[must_use]
    pub const fn dotted(mut self) -> Self {
        self.dots = 1;
        self
    }

    /// Make this a triplet duration.
    #[must_use]
    pub const fn as_triplet(mut self) -> Self {
        self.triplet = true;
        self
    }

    /// Add a multiplier.
    #[must_use]
    pub const fn with_multiplier(mut self, mult: u16) -> Self {
        self.multiplier = Some(mult);
        self
    }

    /// Make this tied to the next.
    #[must_use]
    pub const fn tied_to_next(mut self) -> Self {
        self.tied = true;
        self
    }

    /// Check if this is a valid duration base value.
    #[must_use]
    pub const fn is_valid_base(base: u8) -> bool {
        matches!(base, 1 | 2 | 4 | 8 | 16 | 32 | 64)
    }
}

impl Default for DurationAst {
    fn default() -> Self {
        Self::quarter()
    }
}

/// Push/pull timing modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
pub struct PushPullAst {
    /// Level of subdivision (1=eighth, 2=sixteenth, 3=thirty-second)
    pub level: u8,
    /// Base timing type
    pub base: PushPullBaseAst,
}

impl PushPullAst {
    /// Create from apostrophe count with standard timing.
    #[must_use]
    pub const fn from_count(count: u8) -> Self {
        Self {
            level: count,
            base: PushPullBaseAst::Standard,
        }
    }

    /// Create from apostrophe count with triplet timing.
    #[must_use]
    pub const fn from_count_triplet(count: u8) -> Self {
        Self {
            level: count,
            base: PushPullBaseAst::Triplet,
        }
    }

    /// Create with explicit duration.
    #[must_use]
    pub const fn from_duration(duration: DurationAst) -> Self {
        Self {
            level: 1,
            base: PushPullBaseAst::Duration(duration),
        }
    }

    /// Standard eighth note push/pull.
    #[must_use]
    pub const fn eighth() -> Self {
        Self::from_count(1)
    }

    /// Standard sixteenth note push/pull.
    #[must_use]
    pub const fn sixteenth() -> Self {
        Self::from_count(2)
    }

    /// Triplet eighth push/pull.
    #[must_use]
    pub const fn eighth_triplet() -> Self {
        Self::from_count_triplet(1)
    }
}

/// Base timing for push/pull.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum PushPullBaseAst {
    /// Standard (binary) subdivision
    Standard,
    /// Triplet subdivision
    Triplet,
    /// Arbitrary tuplet
    Tuplet(u8),
    /// Explicit duration
    Duration(DurationAst),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rhythm_ast_slashes() {
        let rhythm = RhythmAst::slashes(4, TextSpan::new(0, 4));

        assert!(rhythm.is_sounding());
        assert!(!rhythm.is_rest());
        assert!(!rhythm.is_space());

        match rhythm.kind {
            RhythmKind::Slashes(count) => assert_eq!(count.beats(), 4),
            _ => panic!("Expected Slashes"),
        }
    }

    #[test]
    fn test_duration_ast_builder() {
        let duration = DurationAst::quarter().dotted().as_triplet();

        assert_eq!(duration.base, 4);
        assert_eq!(duration.dots, 1);
        assert!(duration.triplet);
        assert!(!duration.tied);
    }

    #[test]
    fn test_duration_ast_with_multiplier() {
        let duration = DurationAst::whole().with_multiplier(8);

        assert_eq!(duration.base, 1);
        assert_eq!(duration.multiplier, Some(8));
    }

    #[test]
    fn test_rhythm_rest_and_space() {
        let rest = RhythmAst::rest(DurationAst::quarter(), TextSpan::new(0, 2));
        assert!(rest.is_rest());
        assert!(!rest.is_sounding());

        let space = RhythmAst::space(DurationAst::half(), TextSpan::new(0, 2));
        assert!(space.is_space());
        assert!(!space.is_sounding());
    }

    #[test]
    fn test_push_pull_ast() {
        let push = PushPullAst::eighth();
        assert_eq!(push.level, 1);
        assert_eq!(push.base, PushPullBaseAst::Standard);

        let triplet_push = PushPullAst::eighth_triplet();
        assert_eq!(triplet_push.level, 1);
        assert_eq!(triplet_push.base, PushPullBaseAst::Triplet);
    }

    #[test]
    fn test_valid_base_values() {
        assert!(DurationAst::is_valid_base(1));
        assert!(DurationAst::is_valid_base(2));
        assert!(DurationAst::is_valid_base(4));
        assert!(DurationAst::is_valid_base(8));
        assert!(DurationAst::is_valid_base(16));
        assert!(DurationAst::is_valid_base(32));
        assert!(DurationAst::is_valid_base(64));

        assert!(!DurationAst::is_valid_base(3));
        assert!(!DurationAst::is_valid_base(5));
        assert!(!DurationAst::is_valid_base(128));
    }
}
