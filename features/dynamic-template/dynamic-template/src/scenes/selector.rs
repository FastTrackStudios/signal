//! The selector vocabulary: which tracks a rule is about.
//!
//! A rule starts with a [`Selector`] — which tracks it is about, said in
//! the template's own taxonomy rather than by name. The patch list (#56)
//! lowers to the same vocabulary: an entry is a selector paired with an
//! input role, so the scene engine, the patch applier and the performer
//! rows all agree on what "Cody's amp A 57" means without a second way
//! of saying it.

use facet::Facet;

use super::language::Language;

/// Which tracks a rule is about, in the template's taxonomy.
///
/// Every field is a constraint, and an absent one does not constrain,
/// so `Selector::default()` matches everything. Taxonomy first: kinds
/// and dimensions match what the template knows about a track; `name`
/// is only for a user's own one-off folder the taxonomy has no word
/// for. There is no negation.
#[derive(Facet, Debug, Clone, Default, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
pub struct Selector {
    /// The group path, matched as a prefix: `("Drum Kit" "Kick")` is
    /// the kick piece and everything under it.
    #[facet(default)]
    pub group: Vec<String>,
    /// The taxonomy kind the track carries in its ext-state — a
    /// template folder's (`Process`, `Drum Kit`, `Headphones`) or a
    /// source's (`drums`, `guitar`, `bass`, `vocal`, `keys`).
    #[facet(default)]
    pub kind: Option<String>,
    /// Bus or leaf; `Any` by default.
    #[facet(default)]
    pub role: Role,
    /// Every match, or the topmost per instrument.
    #[facet(default)]
    pub rank: Rank,
    /// The Performer dimension.
    #[facet(default)]
    pub performer: Option<String>,
    /// The Layer dimension (`Main`, `DBL`, `Octave`).
    #[facet(default)]
    pub layer: Option<String>,
    /// The Channel dimension (`L`, `R`).
    #[facet(default)]
    pub channel: Option<String>,
    /// The multi-mic dimension (`In`, `Top`, `DI`, `Amp A 57`).
    #[facet(default)]
    pub multi_mic: Option<String>,
    /// The Arrangement dimension (`Rhythm`, `Lead`, `Solo`).
    #[facet(default)]
    pub arrangement: Option<String>,
    /// The Language dimension (`flow.vocals.language`): `En`, `Es`,
    /// `Pt`, or `All` for a language-free source.
    #[facet(default)]
    pub language: Option<Language>,
    /// A track name, for a user's own folder only.
    #[facet(default)]
    pub name: Option<String>,
    /// How deep the track sits, 0 at the top: the overviews fold by
    /// level, whatever the folders are called.
    #[facet(default)]
    pub depth: Option<u32>,
}

/// The structural role a selector can ask for.
#[derive(Facet, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum Role {
    /// Either.
    #[default]
    Any,
    /// A folder parent — an instrument or a bus.
    Bus,
    /// A track with no children — a source or a return.
    Leaf,
}

/// How many of the matched tracks a rule keeps.
#[derive(Facet, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum Rank {
    /// Every track the selector matches.
    #[default]
    All,
    /// The topmost track per instrument group — one mic of a kit piece.
    TopmostPerInstrument,
}
