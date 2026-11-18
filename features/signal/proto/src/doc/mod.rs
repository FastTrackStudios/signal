//! Signal's model in Loro docs — the codec under the live docs.
//!
//! A [`SignalDoc`] holds Signal entities — [`Rig`], [`Patch`], profiles
//! ([`library::ProfileEntry`]), [`Song`], [`Setlist`] and the library's
//! [`LibrarySettings`] — each under the name every device and agent knows it
//! by ([`DocEntity::doc_key`]). Schema (version [`SCHEMA`]):
//!
//! ```text
//! meta    : Map { schema: i64 }
//! rig     : Map  "rig:<id>"          → the rig     (codec layout, see below)
//! patch   : Map  "patch:<slug>"      → the patch
//! profile : Map  "profile:<id>"      → { id, name, default_patch: "patch:<slug>",
//!                                        patches: keyed list of { id: "patch:<slug>" },
//!                                        metadata }
//! song    : Map  "song:<id>"         → the song
//! setlist : Map  "setlist:<id>"      → the setlist
//! library : Map  "library:settings"  → { default_profile_id } (All-Around)
//! ```
//!
//! A profile holds its patches **by key**, not by value: a patch is one
//! entity wherever it is listed, so the Captain tweaking a patch and an
//! agent adding another to the same profile both land, and a song scene
//! that inherits the default profile (`SceneSource::DefaultProfile`)
//! follows `library:settings` → the profile → its default patch.
//!
//! Each entity is its serde form laid out by [`codec`]: a field per map
//! entry, lists of things with ids as keyed lists (an entry edited in place,
//! a reorder a move), any other list one value. Writing is a reconcile —
//! only what differs becomes operations — and reading is lossless:
//! `get(put(x)) == x` for every entity (the round-trip property test in
//! `tests/doc_roundtrip.rs`).
//!
//! One doc can hold one entity or many; which entities go in which doc is
//! [`library::Layout`]. The decision between the layouts goes in
//! `docs/adr/0001-signal-live-doc-granularity.md`, from the numbers
//! `tests/doc_granularity.rs` prints.

pub mod codec;
pub mod library;
pub mod seed;

use loro::{LoroDoc, LoroEncodeError, LoroError, LoroMap, LoroValue, VersionVector};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::default_profile::LibrarySettings;
use crate::profile::Patch;
use crate::rig::Rig;
use crate::setlist::Setlist;
use crate::song::Song;

/// The schema version written into `meta.schema`.
pub const SCHEMA: i64 = 1;

const META: &str = "meta";

/// What can go wrong between a model and a doc.
#[derive(Debug, thiserror::Error)]
pub enum DocError {
    #[error("loro: {0}")]
    Loro(#[from] LoroError),
    #[error("loro encode: {0}")]
    Encode(#[from] LoroEncodeError),
    #[error("{key}: {source}")]
    Json {
        key: String,
        #[source]
        source: serde_json::Error,
    },
}

/// The kinds of entity a doc holds, each a root map of the doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityKind {
    Rig,
    Patch,
    /// A profile: its name, default patch and patches, the patches by key.
    Profile,
    Song,
    Setlist,
    /// Library-wide settings — the default profile pointer.
    Library,
}

impl EntityKind {
    pub const ALL: [Self; 6] = [
        Self::Rig,
        Self::Patch,
        Self::Profile,
        Self::Song,
        Self::Setlist,
        Self::Library,
    ];

    /// The root map's name, and the prefix of every key in it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rig => "rig",
            Self::Patch => "patch",
            Self::Profile => "profile",
            Self::Song => "song",
            Self::Setlist => "setlist",
            Self::Library => "library",
        }
    }

    /// The kind a doc key names (`patch:worship-lead` → `Patch`).
    #[must_use]
    pub fn of_key(key: &str) -> Option<Self> {
        let (prefix, _) = key.split_once(':')?;
        Self::ALL.into_iter().find(|k| k.as_str() == prefix)
    }
}

/// An entity a [`SignalDoc`] can hold.
pub trait DocEntity: Serialize + DeserializeOwned {
    const KIND: EntityKind;

    /// The name the live docs know this entity by — stable across renames.
    fn doc_key(&self) -> String;
}

impl DocEntity for Rig {
    const KIND: EntityKind = EntityKind::Rig;
    fn doc_key(&self) -> String {
        format!("rig:{}", self.id)
    }
}

impl DocEntity for Patch {
    const KIND: EntityKind = EntityKind::Patch;
    fn doc_key(&self) -> String {
        Self::doc_key(self)
    }
}

impl DocEntity for library::ProfileEntry {
    const KIND: EntityKind = EntityKind::Profile;
    fn doc_key(&self) -> String {
        format!("profile:{}", self.id)
    }
}

impl DocEntity for LibrarySettings {
    const KIND: EntityKind = EntityKind::Library;
    fn doc_key(&self) -> String {
        LIBRARY_SETTINGS_KEY.to_owned()
    }
}

/// The key of the library's settings (the default profile pointer).
pub const LIBRARY_SETTINGS_KEY: &str = "library:settings";

impl DocEntity for Song {
    const KIND: EntityKind = EntityKind::Song;
    fn doc_key(&self) -> String {
        format!("song:{}", self.id)
    }
}

impl DocEntity for Setlist {
    const KIND: EntityKind = EntityKind::Setlist;
    fn doc_key(&self) -> String {
        format!("setlist:{}", self.id)
    }
}

/// A Loro doc holding Signal entities.
#[derive(Debug)]
pub struct SignalDoc {
    doc: LoroDoc,
}

impl Default for SignalDoc {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalDoc {
    /// An empty doc.
    #[must_use]
    pub fn new() -> Self {
        Self {
            doc: LoroDoc::new(),
        }
    }

    /// Wrap an existing doc — one restored from disk or handed over by a
    /// sync driver.
    #[must_use]
    pub const fn from_loro(doc: LoroDoc) -> Self {
        Self { doc }
    }

    /// A doc from a snapshot or updates.
    ///
    /// # Errors
    /// When the bytes are not a Loro export.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DocError> {
        let doc = Self::new();
        doc.import(bytes)?;
        Ok(doc)
    }

    /// A replica with its own peer id, for a second participant.
    #[must_use]
    pub fn fork(&self) -> Self {
        Self {
            doc: self.doc.fork(),
        }
    }

    /// The underlying Loro doc (for sync drivers, persistence, history).
    #[must_use]
    pub const fn loro(&self) -> &LoroDoc {
        &self.doc
    }

    fn root(&self, kind: EntityKind) -> LoroMap {
        self.doc.get_map(kind.as_str())
    }

    /// Reconcile the doc's copy of `entity` with it. Nothing is recorded
    /// until [`Self::commit`].
    ///
    /// # Errors
    /// When the entity does not serialize, or a Loro operation fails.
    pub fn put<E: DocEntity>(&self, entity: &E) -> Result<(), DocError> {
        let key = entity.doc_key();
        let value = serde_json::to_value(entity).map_err(|source| DocError::Json {
            key: key.clone(),
            source,
        })?;
        codec::put_value(&self.doc.get_map(META), "schema", LoroValue::I64(SCHEMA))?;
        codec::put(&self.root(E::KIND), &key, &value)?;
        Ok(())
    }

    /// Drop the entity at `key` (a no-op when there is none).
    ///
    /// # Errors
    /// When a Loro operation fails.
    pub fn remove(&self, key: &str) -> Result<(), DocError> {
        if let Some(kind) = EntityKind::of_key(key) {
            let root = self.root(kind);
            if root.get(key).is_some() {
                root.delete(key)?;
            }
        }
        Ok(())
    }

    /// The entity at `key`, if the doc holds one.
    ///
    /// # Errors
    /// When what the doc holds does not read as an `E`.
    pub fn get<E: DocEntity>(&self, key: &str) -> Result<Option<E>, DocError> {
        let Some(value) = codec::get(&self.root(E::KIND), key) else {
            return Ok(None);
        };
        serde_json::from_value(value)
            .map(Some)
            .map_err(|source| DocError::Json {
                key: key.to_owned(),
                source,
            })
    }

    /// Every `E` the doc holds, by key.
    ///
    /// # Errors
    /// When one of them does not read as an `E`.
    pub fn all<E: DocEntity>(&self) -> Result<Vec<E>, DocError> {
        self.keys(E::KIND)
            .iter()
            .filter_map(|key| self.get(key).transpose())
            .collect()
    }

    /// The keys of every entity of `kind` the doc holds, sorted.
    #[must_use]
    pub fn keys(&self, kind: EntityKind) -> Vec<String> {
        let mut keys: Vec<String> = self.root(kind).keys().map(|k| k.to_string()).collect();
        keys.sort();
        keys
    }

    /// Record what was put since the last commit, under `origin` (who made
    /// the change: `captain`, `agent:<id>`, `signal:<device>`).
    pub fn commit(&self, origin: &str) {
        self.doc.set_next_commit_origin(origin);
        self.doc.commit();
    }

    /// The whole history, for saving or a cold join.
    ///
    /// # Errors
    /// When Loro cannot encode the document.
    pub fn snapshot(&self) -> Result<Vec<u8>, DocError> {
        Ok(self.doc.export(loro::ExportMode::Snapshot)?)
    }

    /// What this replica has that a peer at `since` does not.
    ///
    /// # Errors
    /// When Loro cannot encode the updates.
    pub fn updates_since(&self, since: &VersionVector) -> Result<Vec<u8>, DocError> {
        Ok(self.doc.export(loro::ExportMode::updates(since))?)
    }

    /// This replica's version, to ask a peer for what it is missing.
    #[must_use]
    pub fn version(&self) -> VersionVector {
        self.doc.oplog_vv()
    }

    /// Merge a snapshot or updates from elsewhere.
    ///
    /// # Errors
    /// When the bytes are not a Loro export this doc can import.
    pub fn import(&self, bytes: &[u8]) -> Result<(), DocError> {
        self.doc.import(bytes)?;
        Ok(())
    }

    /// Everything the doc holds as JSON, `{ meta, rig: { key: … }, … }`
    /// — what the dump tool prints. Entities read as plain JSON, so a doc
    /// a newer Signal wrote still dumps.
    #[must_use]
    pub fn dump(&self) -> serde_json::Value {
        let mut out = serde_json::Map::new();
        out.insert(
            META.to_owned(),
            codec::from_deep(&self.doc.get_map(META).get_deep_value()),
        );
        for kind in EntityKind::ALL {
            out.insert(
                kind.as_str().to_owned(),
                codec::from_deep(&self.root(kind).get_deep_value()),
            );
        }
        serde_json::Value::Object(out)
    }
}
