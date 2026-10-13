//! `objects/` — immutable, content-addressed blobs.
//!
//! This is the *format's* half of the object store: put bytes, get bytes,
//! named by hash, one loose file each. The **store proper** — `compact`'s
//! mark-and-sweep GC, save-history retention, tolerating OneDrive-style
//! unmaterialised placeholders — is [#172][]. Nothing here forecloses it;
//! the on-disk layout is exactly what #172 will garbage-collect.
//!
//! [#172]: https://github.com/FastTrackStudios/FastTrackStudio/issues/172
//!
//! ## Why loose files, and why immutable
//!
//! Loose, not packed: the cloud-sync pathology is thousands of *tiny* files,
//! and ours are multi-megabyte (#155 decision 5).
//!
//! Immutable and hash-named is what makes sync conflicts on large data
//! structurally impossible — two machines cannot produce conflicting objects,
//! because the same name necessarily means the same bytes (#155 decision 7).
//! A reader that "repairs" a mismatched object would break that invariant, so
//! [`ObjectStore::get`] verifies and refuses instead.

use crate::error::{DawError, DawResult};
use crate::id::ObjectId;
use std::collections::BTreeMap;
use std::path::Path;

/// The blobs belonging to one project.
///
/// Held in memory alongside the document; written out as `objects/<id>` on
/// save and read back on load.
#[derive(Clone, Debug, Default)]
pub struct ObjectStore {
    blobs: BTreeMap<ObjectId, Vec<u8>>,
}

impl ObjectStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Store bytes, returning their id.
    ///
    /// Idempotent by construction: putting identical bytes twice costs one
    /// object. This is what makes autosave cheap — forty plugin instances
    /// whose state has not changed are not rewritten.
    pub fn put(&mut self, bytes: impl Into<Vec<u8>>) -> ObjectId {
        let bytes = bytes.into();
        let id = ObjectId::of(&bytes);
        self.blobs.entry(id.clone()).or_insert(bytes);
        id
    }

    /// Fetch bytes by id.
    ///
    /// A missing object is an error, never a silently empty project: #155
    /// decision 7 requires a manifest that references an unsynced hash to
    /// fail loudly.
    pub fn get(&self, id: &ObjectId) -> DawResult<&[u8]> {
        self.blobs
            .get(id)
            .map(Vec::as_slice)
            .ok_or_else(|| DawError::MissingObject {
                id: id.to_string(),
                referenced_as: "document".to_string(),
            })
    }

    /// Whether the store holds this object.
    pub fn contains(&self, id: &ObjectId) -> bool {
        self.blobs.contains_key(id)
    }

    /// How many objects are held.
    pub fn len(&self) -> usize {
        self.blobs.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.blobs.is_empty()
    }

    /// Every id held, in a stable order.
    /// Drop every object the predicate rejects.
    ///
    /// Only `compact` should call this. Objects are immutable and
    /// content-addressed, so removing one is never an update — it is a
    /// deletion, and the only safe basis for it is reachability.
    pub fn retain(&mut self, keep: impl Fn(&ObjectId) -> bool) {
        self.blobs.retain(|id, _| keep(id));
    }

    pub fn ids(&self) -> impl Iterator<Item = &ObjectId> {
        self.blobs.keys()
    }

    /// Write every object as a loose file under `dir`.
    ///
    /// Objects already present are left alone — they are immutable, so
    /// rewriting them can only waste I/O.
    pub fn write_dir(&self, dir: &Path) -> DawResult<()> {
        std::fs::create_dir_all(dir)?;
        for (id, bytes) in &self.blobs {
            let path = dir.join(id.as_str());
            if path.exists() {
                continue;
            }
            std::fs::write(path, bytes)?;
        }
        Ok(())
    }

    /// Read every loose object under `dir`.
    ///
    /// Verifies each file against its own name. A blob whose content does
    /// not hash to its filename is corruption, and is reported rather than
    /// loaded — see the module docs on why silent repair is not an option.
    pub fn read_dir(dir: &Path) -> DawResult<Self> {
        let mut store = Self::new();
        if !dir.exists() {
            return Ok(store);
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            store.insert_named(&name, std::fs::read(entry.path())?)?;
        }
        Ok(store)
    }

    /// Objects that arrived by their file names from somewhere other than
    /// a disk (a share link, a peer) — verified as [`Self::read_dir`] does.
    pub fn from_named(objects: impl IntoIterator<Item = (String, Vec<u8>)>) -> DawResult<Self> {
        let mut store = Self::new();
        for (name, bytes) in objects {
            store.insert_named(&name, bytes)?;
        }
        Ok(store)
    }

    /// Hold `bytes` found under the object file name `name`, refusing a
    /// blob whose content does not hash to it.
    fn insert_named(&mut self, name: &str, bytes: Vec<u8>) -> DawResult<()> {
        let actual = ObjectId::of(&bytes);
        if actual.as_str() != name {
            return Err(DawError::MissingObject {
                id: name.to_owned(),
                referenced_as: format!("objects/ — content hashes to {actual} instead"),
            });
        }
        self.blobs.insert(actual, bytes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn putting_identical_bytes_costs_one_object() {
        let mut store = ObjectStore::new();
        let first = store.put(b"same".to_vec());
        let second = store.put(b"same".to_vec());
        assert_eq!(first, second);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn a_missing_object_is_a_loud_error() {
        let store = ObjectStore::new();
        let absent = ObjectId::of(b"never stored");
        assert!(matches!(
            store.get(&absent),
            Err(DawError::MissingObject { .. })
        ));
    }

    #[test]
    fn objects_survive_a_directory_roundtrip() {
        let dir = std::env::temp_dir().join(format!("daw-objects-{}", uuid::Uuid::new_v4()));
        let mut store = ObjectStore::new();
        let id = store.put(b"payload".to_vec());
        store.write_dir(&dir).expect("write");

        let read_back = ObjectStore::read_dir(&dir).expect("read");
        assert_eq!(read_back.get(&id).expect("present"), b"payload");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_corrupt_object_is_refused_not_repaired() {
        let dir = std::env::temp_dir().join(format!("daw-objects-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let lying_name = ObjectId::of(b"one thing");
        std::fs::write(dir.join(lying_name.as_str()), b"another thing").expect("write");

        assert!(matches!(
            ObjectStore::read_dir(&dir),
            Err(DawError::MissingObject { .. })
        ));

        std::fs::remove_dir_all(&dir).ok();
    }
}
