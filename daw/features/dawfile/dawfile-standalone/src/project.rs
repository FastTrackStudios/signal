//! A project on disk: the directory, and what tracks whether it changed.
//!
//! A project directory has **exactly two entries** (#155 decision 4):
//!
//! ```text
//! Belief.session/
//!   Belief.session      the readable source of truth
//!   objects/            immutable, content-addressed blobs
//! ```
//!
//! Everything human-readable — tracks, items, takes, envelopes, tempo map,
//! markers, routing, editor state — is in the one file. Everything large is in `objects/`,
//! hash-named and never mutated, which is what makes sync conflicts on large
//! data structurally impossible.
//!
//! What is *not* here, on purpose: `compact`'s GC and save-history retention
//! ([#172][]), the persisted loro oplog ([#173][]), DAWproject import/export
//! ([#174][]), templates ([#175][]). The layout above is what each of those
//! attaches to.
//!
//! [#172]: https://github.com/FastTrackStudios/FastTrackStudio/issues/172
//! [#173]: https://github.com/FastTrackStudios/FastTrackStudio/issues/173
//! [#174]: https://github.com/FastTrackStudios/FastTrackStudio/issues/174
//! [#175]: https://github.com/FastTrackStudios/FastTrackStudio/issues/175

use crate::document::{DawDocument, SourceFormat};
use crate::error::{DawError, DawResult};
use crate::objects::ObjectStore;
use crate::rpp;
use crate::styx;
use std::path::{Path, PathBuf};

/// The name of the object directory inside a project.
pub const OBJECTS_DIR: &str = "objects";

/// The extension of the manifest, and of the project directory itself.
///
/// A session is what the thing *is* — the tracks, the takes, the tempo
/// map, the arrangement — and `.session` is what the app, the session
/// domain and the user all call it. `.daw` is still opened (see
/// [`DAW_EXTENSION`]); only the name changed, never the bytes.
pub const SESSION_EXTENSION: &str = "session";

/// The extension projects were written with before `.session`.
///
/// Still accepted on load, so a project saved by an earlier build opens
/// without conversion. Never written.
pub const DAW_EXTENSION: &str = "daw";

/// Every extension [`DawProject::load`] recognises, newest first.
pub const PROJECT_EXTENSIONS: [&str; 2] = [SESSION_EXTENSION, DAW_EXTENSION];

/// An open project: the document, its objects, and whether it has been
/// touched since it was loaded.
#[derive(Clone, Debug)]
pub struct DawProject {
    /// The document. Mutate it through [`DawProject::edit`] so the modified
    /// flag stays honest.
    document: DawDocument,
    /// The blobs the document references.
    objects: ObjectStore,
    /// Whether anything has changed since load or save.
    ///
    /// This is what decides between the two export paths — see
    /// [`DawProject::to_rpp`].
    modified: bool,
    /// The CRDT history, when this project has any (#173).
    ///
    /// `None` until the first save, and after a hand edit invalidates
    /// the stored log.
    history: Option<loro::LoroDoc>,
}

impl DawProject {
    /// Wrap a document and its objects.
    pub fn new(document: DawDocument, objects: ObjectStore) -> Self {
        Self {
            document,
            objects,
            modified: false,
            history: None,
        }
    }

    /// Read-only access to the document.
    pub fn document(&self) -> &DawDocument {
        &self.document
    }

    /// The project's objects.
    pub fn objects(&self) -> &ObjectStore {
        &self.objects
    }

    /// Whether the project has unsaved, un-exported changes.
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Mutate the document.
    ///
    /// Every path that changes the document goes through here, which is why
    /// [`is_modified`](Self::is_modified) can be trusted — and why an
    /// untouched project can be exported back to its original bytes without
    /// anyone having to remember to say so.
    pub fn edit<T>(&mut self, mutate: impl FnOnce(&mut DawDocument) -> T) -> T {
        let outcome = mutate(&mut self.document);
        self.document.reindex();
        self.modified = true;
        self.mark_diverged();
        outcome
    }

    /// Record that the document no longer matches the bytes it was
    /// imported from.
    ///
    /// Kept in the document rather than beside it because `modified` is
    /// cleared by saving: without this, a project edited, saved and
    /// reopened would export its *original* `.rpp` and throw the edits
    /// away. See [`crate::Provenance::edited`].
    fn mark_diverged(&mut self) {
        if let Some(provenance) = self.document.provenance.as_mut() {
            provenance.edited = true;
        }
    }

    /// Store bytes and return their id, for a caller that needs to attach a
    /// blob (an FX chain, a rendered take) to the document.
    pub fn put_object(&mut self, bytes: impl Into<Vec<u8>>) -> crate::id::ObjectId {
        self.modified = true;
        self.mark_diverged();
        self.objects.put(bytes)
    }

    /// Import a REAPER project.
    pub fn import_rpp(text: &str, name: impl Into<String>) -> DawResult<(Self, rpp::ImportReport)> {
        let mut objects = ObjectStore::new();
        let (document, report) = rpp::from_rpp(text, name, &mut objects)?;
        Ok((Self::new(document, objects), report))
    }

    /// Import a REAPER project from disk. The project name comes from the
    /// file stem.
    pub fn import_rpp_file(path: impl AsRef<Path>) -> DawResult<(Self, rpp::ImportReport)> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string());
        Self::import_rpp(&text, name)
    }

    /// Export back to REAPER project text.
    ///
    /// Untouched projects come back **byte-identical**: the original bytes
    /// are in `objects/`, and there is no honest reason to regenerate them.
    /// A touched project is exported by patching that original, so the parts
    /// the schema never modelled survive too.
    pub fn to_rpp(&self) -> DawResult<String> {
        let provenance = self.document.provenance.as_ref().ok_or_else(|| {
            DawError::Rpp("document has no .rpp provenance to export against".into())
        })?;
        if provenance.format != SourceFormat::Rpp {
            return Err(DawError::Rpp(format!(
                "document was imported from {:?}, not REAPER",
                provenance.format
            )));
        }
        // The verbatim shortcut is only honest while the document still
        // *is* the source. `modified` alone would not do: saving clears
        // it, and a reopened project would then export the bytes it came
        // from rather than the session it now holds.
        if !self.modified && !provenance.edited {
            let bytes = self.objects.get(&provenance.source)?;
            return Ok(String::from_utf8_lossy(bytes).into_owned());
        }
        rpp::to_rpp(&self.document, &self.objects)
    }

    /// Export, and report every token the patch rewrote.
    pub fn to_rpp_patched(&self) -> DawResult<(String, rpp::ExportReport)> {
        rpp::to_rpp_patched(&self.document, &self.objects)
    }

    /// The `.session` manifest text.
    pub fn to_text(&self) -> DawResult<String> {
        styx::to_text(&self.document)
    }

    /// Write the project to `dir`, creating it if needed.
    ///
    /// Writes the two entries and nothing else. Objects already on disk are
    /// left alone — they are immutable, so rewriting them can only cost I/O.
    pub fn save(&mut self, dir: impl AsRef<Path>) -> DawResult<()> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;

        // Refuse to write a manifest that points at bytes we do not hold,
        // rather than producing a project that fails to open later.
        for id in self.document.referenced_objects() {
            if !self.objects.contains(&id) {
                return Err(DawError::MissingObject {
                    id: id.to_string(),
                    referenced_as: format!("{}", dir.display()),
                });
            }
        }

        // Record the save before writing the manifest, so the manifest
        // on disk contains its own entry. An entry is a list of hashes,
        // and the bytes behind an unchanged hash are already in
        // `objects/` — which is where the autosave win comes from.
        let seq = self.document.saves.last().map(|s| s.seq).unwrap_or(0) + 1;
        let referenced = self.document.referenced_objects();
        self.document.saves.push(crate::document::SaveEntry {
            seq,
            objects: referenced,
        });

        // The staleness hash covers the manifest *without* its own
        // oplog ref. A manifest cannot contain a hash of itself — that
        // is a fixpoint with no solution — and the ref is the only part
        // of the text a save writes that a hand edit never touches.
        let history = self.history.clone().unwrap_or_default();
        history.commit();
        let bytes = crate::oplog::export(&history)?;
        let object = self.objects.put(bytes);

        self.install_oplog(object)?;
        let text = self.to_text()?;
        self.history = Some(history);

        self.objects.write_dir(&dir.join(OBJECTS_DIR))?;
        std::fs::write(manifest_path(dir, &self.document.name), text)?;
        // A project directory has exactly two entries. Saving one that was
        // opened from an older `.daw` manifest must therefore retire it
        // rather than leave two spellings of the same document side by
        // side, which is the one state `find_manifest` cannot call an
        // honest project.
        let legacy = dir.join(format!("{}.{DAW_EXTENSION}", self.document.name));
        if legacy.exists() {
            std::fs::remove_file(&legacy)?;
        }
        self.modified = false;
        Ok(())
    }

    /// Every object any retained save still points at.
    pub fn reachable_objects(&self) -> Vec<crate::id::ObjectId> {
        let mut out = self.document.referenced_objects();
        for save in &self.document.saves {
            out.extend(save.objects.iter().cloned());
        }
        out.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
        out.dedup();
        out
    }

    /// Garbage-collect the object store.
    ///
    /// **`compact` is GC, not dedup.** Content-addressing already
    /// deduplicates at write time, so there is nothing left to collapse
    /// afterwards; what is left to do is delete what nothing points at.
    ///
    /// Walks every retained save, marks the hashes they reach, and drops
    /// the rest. `keep_saves` optionally trims the history to the last N
    /// first — dropping a save is what makes its objects unreachable, so
    /// the two have to happen in that order.
    ///
    /// **Explicit and manual, never automatic.** A background process
    /// deleting bytes near an 800 MB orchestral template is how someone
    /// loses a session.
    ///
    /// Returns how many objects were removed.
    pub fn compact(&mut self, keep_saves: Option<usize>) -> usize {
        if let Some(keep) = keep_saves {
            let len = self.document.saves.len();
            if keep < len {
                self.document.saves.drain(..len - keep);
            }
        }
        let reachable = self.reachable_objects();
        let before = self.objects.len();
        self.objects.retain(|id| reachable.contains(id));
        self.modified = true;
        before - self.objects.len()
    }

    /// Write the compacted store back over `dir`, removing deleted blobs
    /// from disk as well as from memory.
    pub fn compact_on_disk(
        &mut self,
        dir: impl AsRef<Path>,
        keep_saves: Option<usize>,
    ) -> DawResult<usize> {
        let dir = dir.as_ref();
        let removed = self.compact(keep_saves);
        let objects_dir = dir.join(OBJECTS_DIR);
        if objects_dir.exists() {
            // Only unlink what we know is unreachable; anything we do not
            // recognise is left alone rather than assumed to be litter.
            for entry in std::fs::read_dir(&objects_dir)? {
                let path = entry?.path();
                let Some(stem) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if !self.objects.ids().any(|id| id.to_string() == stem) {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        // Compaction rewrites the manifest, so the stored hash no
        // longer describes it. Left alone, the next load sees a
        // mismatch and silently drops history — indistinguishable from
        // the hand-edit case, and nothing like what the caller asked
        // for.
        if let Some(oplog) = self.document.oplog.clone() {
            self.install_oplog(oplog.object)?;
        }
        std::fs::write(manifest_path(dir, &self.document.name), self.to_text()?)?;
        self.modified = false;
        Ok(removed)
    }

    /// Point the manifest at `object` and re-hash it.
    ///
    /// The hash covers the manifest with its own oplog ref removed: a
    /// manifest holding a hash of itself is a fixpoint with no
    /// solution, and the ref is the one part of the text a hand edit
    /// never touches.
    ///
    /// The object is also added to the newest save entry, so a retained
    /// save still reaches the history it was written with rather than
    /// having it collected the next time somebody compacts.
    fn install_oplog(&mut self, object: crate::id::ObjectId) -> DawResult<()> {
        self.document.oplog = None;
        if let Some(entry) = self.document.saves.last_mut()
            && !entry.objects.contains(&object)
        {
            entry.objects.push(object.clone());
        }
        let text_hash = crate::oplog::hash_text(&self.to_text()?);
        self.document.oplog = Some(crate::oplog::OplogRef { object, text_hash });
        Ok(())
    }

    /// Open a project directory.
    pub fn load(dir: impl AsRef<Path>) -> DawResult<Self> {
        let dir = dir.as_ref();
        let manifest = find_manifest(dir)?;
        let text = std::fs::read_to_string(&manifest)?;
        let objects = ObjectStore::read_dir(&dir.join(OBJECTS_DIR))?;
        Self::from_parts(&text, &manifest.to_string_lossy(), objects)
    }

    /// Open a project from its parts: the manifest's text (`origin` names
    /// it in errors) and its objects — a project that arrived from
    /// somewhere with no disk (a browser, from a share link), opened
    /// exactly as [`Self::load`] opens one from a directory.
    pub fn from_parts(text: &str, origin: &str, objects: ObjectStore) -> DawResult<Self> {
        let manifest = origin;
        let text = text.to_owned();
        let document = styx::from_text(&text, manifest)?;

        // #155 decision 7: a manifest referencing an unsynced hash must fail
        // loudly rather than open a broken project.
        for id in document.referenced_objects() {
            if !objects.contains(&id) {
                return Err(DawError::MissingObject {
                    id: id.to_string(),
                    referenced_as: manifest.to_owned(),
                });
            }
        }

        // History is restored only when the manifest is byte-for-byte
        // the text the log was built from. A hand edit discards it, and
        // that is a normal outcome rather than an error (#173).
        let bytes = document
            .oplog
            .as_ref()
            .and_then(|r| objects.get(&r.object).ok())
            .map(|b| b.to_vec());
        // Hash the same thing `save` hashed: the manifest with its
        // oplog ref removed.
        let history = {
            let mut without = document.clone();
            without.oplog = None;
            let bare = styx::to_text(&without).unwrap_or_else(|_| text.clone());
            crate::oplog::load_if_current(document.oplog.as_ref(), bytes.as_deref(), &bare)
        };

        let mut project = Self::new(document, objects);
        project.history = history;
        Ok(project)
    }

    /// The project's CRDT history, when it has any.
    ///
    /// `None` means history starts fresh on the next save — either this
    /// project has never been saved, or its manifest was hand-edited
    /// since the log was built.
    pub fn history(&self) -> Option<&loro::LoroDoc> {
        self.history.as_ref()
    }

    /// Adopt `history` as the project's CRDT history, saved with it on the
    /// next [`Self::save`].
    ///
    /// For a host that keeps the live document itself (the Session app's
    /// collaboration doc): the log it saves is the one everybody edited,
    /// not a fresh one started from the text.
    pub fn set_history(&mut self, history: loro::LoroDoc) {
        self.history = Some(history);
    }
}

/// Where the manifest for a project named `name` lives inside `dir`.
///
/// Always `.session`: a project saved by an older build is read through
/// its `.daw` manifest and written back as `.session`, because the
/// content is identical and carrying two spellings forward forever buys
/// nothing.
pub fn manifest_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.{SESSION_EXTENSION}"))
}

/// Find the single manifest in a project directory.
///
/// `.session` wins when a directory somehow has both — that is the
/// spelling a save writes, so it is the newer of the two by construction.
fn find_manifest(dir: &Path) -> DawResult<PathBuf> {
    if !dir.is_dir() {
        return Err(DawError::NotAProject {
            path: dir.display().to_string(),
            reason: "not a directory".to_string(),
        });
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    choose_manifest(&names)
        .map(|name| dir.join(name))
        .map_err(|reason| DawError::NotAProject {
            path: dir.display().to_string(),
            reason,
        })
}

/// Which of a project directory's file names is its manifest: the one of
/// the most preferred extension ([`PROJECT_EXTENSIONS`]), of which there
/// must be exactly one. For a directory that is not on a disk (its files
/// arrived from a share link) as for one that is.
///
/// # Errors
///
/// None, or more than one, of the most preferred kind — the reason.
pub fn choose_manifest(names: &[String]) -> Result<&str, String> {
    let rank = |name: &str| {
        let extension = name.rsplit_once('.').map(|(_, e)| e)?;
        PROJECT_EXTENSIONS
            .iter()
            .position(|known| *known == extension)
    };
    let best = names.iter().filter_map(|n| rank(n)).min();
    let found: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|n| best.is_some() && rank(n) == best)
        .collect();
    match found.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("no *.{SESSION_EXTENSION} manifest")),
        many => Err(format!(
            "{} manifests; a project directory holds exactly one",
            many.len()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: &str = r#"<REAPER_PROJECT 0.1 "7.0/test" 1700000000
  SAMPLERATE 48000 0 0
  TEMPO 120 4 4
  <TRACK {AAAAAAAA-0001-0000-0000-000000000000}
    NAME Kick
    VOLPAN 1 0 -1 -1 1
    TRACKID {AAAAAAAA-0001-0000-0000-000000000000}
  >
>
"#;

    fn scratch_dir() -> PathBuf {
        std::env::temp_dir().join(format!("daw-project-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn a_project_directory_holds_exactly_two_entries() {
        let dir = scratch_dir();
        let (mut project, _) = DawProject::import_rpp(TINY, "Tiny").expect("import");
        project.save(&dir).expect("save");

        let mut entries: Vec<String> = std::fs::read_dir(&dir)
            .expect("read")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        entries.sort();
        assert_eq!(
            entries,
            vec!["Tiny.session".to_string(), "objects".to_string()]
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_project_saved_under_the_old_extension_still_opens_and_is_renamed() {
        // `.daw` was the extension before `.session`. The bytes never
        // changed, so an older project must open as it is — and saving it
        // must leave one manifest behind, not two.
        let dir = scratch_dir();
        let (mut project, _) = DawProject::import_rpp(TINY, "Tiny").expect("import");
        project.save(&dir).expect("save");

        std::fs::rename(dir.join("Tiny.session"), dir.join("Tiny.daw")).expect("rename");
        let mut reopened = DawProject::load(&dir).expect("an older project still opens");
        assert_eq!(reopened.document().tracks.len(), 1);

        reopened.save(&dir).expect("save");
        let mut entries: Vec<String> = std::fs::read_dir(&dir)
            .expect("read")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        entries.sort();
        assert_eq!(
            entries,
            vec!["Tiny.session".to_string(), "objects".to_string()],
            "saving must retire the old manifest rather than keep both"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_saved_project_reloads_with_the_same_entities() {
        let dir = scratch_dir();
        let (mut project, _) = DawProject::import_rpp(TINY, "Tiny").expect("import");
        project.save(&dir).expect("save");

        let reloaded = DawProject::load(&dir).expect("load");
        assert_eq!(reloaded.document().tracks.len(), 1);
        assert_eq!(reloaded.document().tracks[0].track.name, "Kick");
        assert_eq!(reloaded.document().sample_rate, Some(48_000));
        assert!(reloaded.document().check_invariants().is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn saving_clears_the_modified_flag_and_editing_sets_it() {
        let dir = scratch_dir();
        let (mut project, _) = DawProject::import_rpp(TINY, "Tiny").expect("import");
        assert!(!project.is_modified());
        project.edit(|document| document.tracks[0].track.volume = 0.5);
        assert!(project.is_modified());
        project.save(&dir).expect("save");
        assert!(!project.is_modified());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_manifest_referencing_a_missing_object_fails_loudly() {
        let dir = scratch_dir();
        let (mut project, _) = DawProject::import_rpp(TINY, "Tiny").expect("import");
        project.save(&dir).expect("save");

        // Simulate the object half not having finished syncing.
        std::fs::remove_dir_all(dir.join(OBJECTS_DIR)).expect("remove objects");

        assert!(matches!(
            DawProject::load(&dir),
            Err(DawError::MissingObject { .. })
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_directory_without_a_manifest_is_not_a_project() {
        let dir = scratch_dir();
        std::fs::create_dir_all(&dir).expect("mkdir");
        assert!(matches!(
            DawProject::load(&dir),
            Err(DawError::NotAProject { .. })
        ));
        std::fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn a_project_opens_from_its_parts_as_from_its_directory() {
        let dir = std::env::temp_dir().join(format!("dawfile-parts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (mut project, _) = DawProject::import_rpp(TINY, "Tiny").expect("import");
        project.save(&dir).expect("save");
        let from_disk = DawProject::load(&dir).expect("load");

        // The same folder, as files that arrived from elsewhere.
        let mut names = Vec::new();
        let mut objects = Vec::new();
        for entry in std::fs::read_dir(&dir).unwrap() {
            names.push(entry.unwrap().file_name().to_string_lossy().into_owned());
        }
        for entry in std::fs::read_dir(dir.join(OBJECTS_DIR)).unwrap() {
            let entry = entry.unwrap();
            objects.push((
                entry.file_name().to_string_lossy().into_owned(),
                std::fs::read(entry.path()).unwrap(),
            ));
        }
        let manifest = choose_manifest(&names).expect("one manifest");
        let text = std::fs::read_to_string(dir.join(manifest)).unwrap();
        let from_parts =
            DawProject::from_parts(&text, manifest, ObjectStore::from_named(objects).unwrap())
                .expect("from parts");
        assert_eq!(from_parts.to_rpp().unwrap(), from_disk.to_rpp().unwrap());

        // A blob that is not what its name says is refused, as on disk.
        assert!(ObjectStore::from_named([("sha256-00".to_owned(), b"x".to_vec())]).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
