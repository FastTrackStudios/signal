//! Language: a taxonomy dimension, not a name.
//!
//! `flow.vocals.language`: a vocal source track is sung in one language,
//! or in [`Language::All`] when it is language-free — a wordless "Hey!",
//! a hummed pad. It sits at source level, **under** the performer and
//! the layer, never above them: `Vocals / Ron / Main / {EN, ES, PT}`.
//!
//! Read the same way every other dimension in this crate is read
//! (`track_schema::classify_track_dimension`): from the track's own
//! name first, and when that says nothing, inherited from the nearest
//! enclosing folder that IS a language — `Vocals / Ron / Main / EN /
//! COMP` inherits `EN` from the `EN` folder above it, which is what
//! lets the comp stack (COMP/EDIT/TUNE) live under a language folder
//! without repeating the language on every track inside it.

use facet::Facet;

/// The vocabulary a track name or a folder is checked against.
///
/// The decision behind `flow.vocals.language` says "EN, ES, PT, All
/// from the config" — read here as *the* config: a small, closed,
/// project-wide vocabulary, the same status the kit's five pieces
/// (`table::PIECES`) and the growth orders in `track_schema` have,
/// hardcoded rather than threaded through `DynamicTemplateConfig`
/// (monarchy's `Config<ItemMetadata>`, built for a per-instrument
/// pattern vocabulary a user's own config extends — not a project-wide
/// three-value taxonomy dimension). Widening this to a value a user's
/// config can actually override is a mechanical follow-up once there is
/// a real second vocabulary asking for it, not a redesign — see
/// `track_schema::configured_values_for_dimension` for the shape it
/// would take.
#[derive(Facet, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum Language {
    /// English.
    En,
    /// Spanish.
    Es,
    /// Portuguese.
    Pt,
    /// Language-free: a wordless "Hey!", a hummed pad. Never hidden by
    /// the active-language prelude, whatever language is active.
    #[default]
    All,
}

impl Language {
    /// Every sung language — [`Self::All`] excluded, because it is never
    /// one of "the other languages" a switch hides.
    pub const SUNG: [Self; 3] = [Self::En, Self::Es, Self::Pt];

    /// The stable slug: also what the active-language ext-state value
    /// round-trips through.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
            Self::Pt => "pt",
            Self::All => "all",
        }
    }

    /// Parse a stable slug back — the inverse of [`Self::as_str`], and
    /// case-insensitive so `"EN"` (the ext-state a scene selector reads)
    /// and `"en"` (what got written) both come back the same value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "en" => Some(Self::En),
            "es" => Some(Self::Es),
            "pt" => Some(Self::Pt),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

/// The words a track's own name is checked against, alongside the
/// slug itself — a track is as likely to be named "English" as "EN".
const fn aliases(language: Language) -> &'static [&'static str] {
    match language {
        Language::En => &["en", "english"],
        Language::Es => &["es", "esp", "spanish", "espanol", "español"],
        Language::Pt => &["pt", "por", "portuguese", "portugues", "português"],
        Language::All => &["all"],
    }
}

/// The language a track's own name carries, if it names one word for
/// word — never a substring, so a `Percussion` folder does not read as
/// Portuguese because it contains "per".
///
/// This is the *own* half of the classifier; folder inheritance (the
/// other half) is `adapt::Walk::step`'s job, since it needs the tree
/// this function does not see.
// r[impl flow.vocals.language]
#[must_use]
pub fn classify_language(name: &str) -> Option<Language> {
    let words: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    // A name is a language name when the WHOLE name is one word from
    // the vocabulary ("EN", "Portuguese") — a longer name that merely
    // contains "En" ("Ensemble") does not count, the way `classify_track_dimension`
    // never lets a substring stand for the whole.
    let [word] = words.as_slice() else {
        return None;
    };
    let word = word.as_str();
    [Language::En, Language::Es, Language::Pt, Language::All]
        .into_iter()
        .find(|&language| aliases(language).contains(&word))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every configured name classifies, case and word-form insensitive.
    // r[verify flow.vocals.language]
    #[test]
    fn names_classify() {
        assert_eq!(classify_language("EN"), Some(Language::En));
        assert_eq!(classify_language("en"), Some(Language::En));
        assert_eq!(classify_language("English"), Some(Language::En));
        assert_eq!(classify_language("ES"), Some(Language::Es));
        assert_eq!(classify_language("Spanish"), Some(Language::Es));
        assert_eq!(classify_language("PT"), Some(Language::Pt));
        assert_eq!(classify_language("Portuguese"), Some(Language::Pt));
        assert_eq!(classify_language("All"), Some(Language::All));
    }

    /// `All` is language-free: it classifies to its own value, and that
    /// value is excluded from the sung set the active-language prelude
    /// hides from.
    // r[verify flow.vocals.language]
    #[test]
    fn all_is_language_free() {
        assert_eq!(classify_language("All"), Some(Language::All));
        assert!(!Language::SUNG.contains(&Language::All));
    }

    /// A track that merely contains a language's letters is not a
    /// match — the classifier reads the whole name, not a substring.
    #[test]
    fn a_substring_does_not_classify() {
        assert_eq!(classify_language("Ensemble"), None);
        assert_eq!(classify_language("Percussion"), None);
        assert_eq!(classify_language("Ron"), None);
        assert_eq!(classify_language("Main"), None);
        assert_eq!(classify_language("DBL"), None);
    }

    /// The ext-state round trip: what gets written comes back the same
    /// value, case-insensitively.
    #[test]
    fn slug_round_trips() {
        for language in [Language::En, Language::Es, Language::Pt, Language::All] {
            assert_eq!(Language::parse(language.as_str()), Some(language));
            assert_eq!(
                Language::parse(&language.as_str().to_uppercase()),
                Some(language)
            );
        }
    }
}
