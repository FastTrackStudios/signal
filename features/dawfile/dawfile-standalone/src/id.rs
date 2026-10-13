//! Stable identity.
//!
//! The hard requirement from
//! [#155](https://github.com/FastTrackStudios/FastTrackStudio/issues/155):
//! every entity carries a stable unique id, and **nothing is ever referenced
//! by array index or position**. This module is where that lives.
//!
//! Two id kinds, and they are not interchangeable:
//!
//! - [`EntityId`] names a *thing in the document* — a track, an item, a take,
//!   an envelope, a marker, a template. It is stable for the life of the
//!   entity and survives reordering, save/load and (later) CRDT merge.
//! - [`ObjectId`] names *bytes* — a content hash into `objects/`. Two
//!   machines that produce the same bytes necessarily produce the same
//!   `ObjectId`, which is why large data cannot conflict on sync.
//!
//! ## What is deliberately *not* an entity
//!
//! Envelope points, tempo points and stretch markers carry no id. They are
//! values inside one owner's ordered list, nothing outside that list ever
//! refers to one, and giving each of an envelope's several thousand points a
//! UUID would wreck the readability that is the whole point of the format.
//! Their merge identity comes from loro's list CRDT at the oplog layer
//! (#173), not from the text.
//!
//! ## Index fields are a cache, never a reference
//!
//! `daw_proto`'s `Track::index`, `Item::index`, `Take::index` and
//! `EnvelopePoint::index` exist because the facade's callers want them. In a
//! `.session` document they are **derived**: recomputed from list position by
//! [`crate::DawDocument::reindex`] on every load and after every structural
//! edit. Nothing in the format reads them to find anything.

use facet::Facet;
use sha2::{Digest, Sha256};
use std::fmt;

/// A stable, unique identity for one entity in a document.
///
/// Backed by a UUID string so it is meaningful across machines and readable
/// in the file. Ids imported from another DAW keep that DAW's identifier
/// verbatim (a REAPER `{GUID}`) so a round trip is recognisable by eye.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Facet)]
#[facet(transparent)]
pub struct EntityId(String);

impl EntityId {
    /// Mint a fresh id. Used for entities the editor creates, and for
    /// entities imported from a source that had no stable identifier of
    /// its own.
    pub fn new() -> Self {
        Self(format!("{}", uuid::Uuid::new_v4()))
    }

    /// Adopt an identifier that is already stable — a REAPER `TRACKID`, an
    /// `IGUID`, a take `GUID`. Preferred over minting, because it makes a
    /// re-import of the same project reuse the same ids.
    pub fn adopt(existing: impl Into<String>) -> Self {
        Self(existing.into())
    }

    /// Derive an id deterministically from a parent id and a discriminator.
    ///
    /// For entities whose source format gives them no identifier of their own
    /// but which *are* uniquely determined by their owner plus a name — a
    /// track's `<VOLENV>`, say. Deterministic derivation means two imports of
    /// the same `.rpp` agree on the id, which is what keeps re-import from
    /// looking like a delete-and-recreate.
    pub fn derived(parent: &EntityId, discriminator: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(parent.as_str().as_bytes());
        hasher.update(b"/");
        hasher.update(discriminator.as_bytes());
        let digest = hasher.finalize();
        let bytes: [u8; 16] = digest[..16].try_into().expect("sha256 yields 32 bytes");
        Self(format!("{}", uuid::Uuid::from_bytes(bytes)))
    }

    /// The id as it appears in the file.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for EntityId {
    fn from(s: &str) -> Self {
        Self::adopt(s)
    }
}

impl From<String> for EntityId {
    fn from(s: String) -> Self {
        Self::adopt(s)
    }
}

/// A content hash naming immutable bytes in `objects/`.
///
/// Rendered as `sha256-<hex>` so the algorithm is visible in the file and a
/// future migration to a different hash is not a guessing game.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Facet)]
#[facet(transparent)]
pub struct ObjectId(String);

impl ObjectId {
    /// Hash bytes into their object id. Pure — same bytes, same id, on every
    /// machine. That is the property that makes sync conflicts on large data
    /// structurally impossible (#155 decision 7).
    pub fn of(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        let mut hex = String::with_capacity(7 + digest.len() * 2);
        hex.push_str("sha256-");
        for byte in digest {
            hex.push_str(&format!("{byte:02x}"));
        }
        Self(hex)
    }

    /// Adopt an id read back from a file.
    pub fn adopt(existing: impl Into<String>) -> Self {
        Self(existing.into())
    }

    /// The id as it appears in the file, and as the filename under
    /// `objects/`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minted_ids_are_unique() {
        assert_ne!(EntityId::new(), EntityId::new());
    }

    #[test]
    fn adopted_ids_are_preserved_verbatim() {
        let guid = "{6D5E0D4E-F122-9D41-3EDF-A96725F69EE6}";
        assert_eq!(EntityId::adopt(guid).as_str(), guid);
    }

    #[test]
    fn derived_ids_are_deterministic_and_distinct() {
        let parent = EntityId::adopt("{parent}");
        assert_eq!(
            EntityId::derived(&parent, "VOLENV"),
            EntityId::derived(&parent, "VOLENV")
        );
        assert_ne!(
            EntityId::derived(&parent, "VOLENV"),
            EntityId::derived(&parent, "PANENV")
        );
    }

    #[test]
    fn object_ids_are_content_addressed() {
        assert_eq!(ObjectId::of(b"hello"), ObjectId::of(b"hello"));
        assert_ne!(ObjectId::of(b"hello"), ObjectId::of(b"world"));
        assert!(ObjectId::of(b"hello").as_str().starts_with("sha256-"));
    }
}
