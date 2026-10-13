//! Reading and writing this domain's styx files.
//!
//! Both documents — the album's list and the room's profile — are read
//! and written the same way, and the ONE thing that is not the default
//! is worth having in one place: absent options are omitted rather than
//! written as the unit `@`, because the parser does not read `@` back
//! into an `Option`. Writing them would produce a file this crate
//! cannot open, which is the round-trip test's whole subject.

use facet::Facet;

/// What writing a document can fail with.
pub type WriteError = facet_styx::SerializeError<facet_styx::StyxSerializeError>;

/// Parse a document.
///
/// # Errors
///
/// When the text is not a document of this shape.
pub fn read<T: Facet<'static>>(text: &str) -> Result<T, facet_styx::DeserializeError> {
    facet_styx::from_str(text)
}

/// Write a document, absent options omitted.
///
/// # Errors
///
/// When the value cannot be serialized.
pub fn write<'a, T: Facet<'a>>(value: &T) -> Result<String, WriteError> {
    facet_styx::to_string_with_options(value, &facet_styx::SerializeOptions::default().omit_none())
}
