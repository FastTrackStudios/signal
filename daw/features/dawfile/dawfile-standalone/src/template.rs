//! Templates: a project fragment, in the project format.
//!
//! A template is **a project fragment at any granularity** — a set of
//! items, one track, a group of tracks — stored in the identical format
//! as a project, just rooted at something other than a project. One
//! serializer, one parser, one test suite, and a template can be
//! inspected exactly like a project.
//!
//! That identity is the whole design decision: the alternative was a
//! second format for "the same thing but smaller", which is two of
//! everything and a second way to be wrong.
//!
//! ## Objects come with it
//!
//! Loading a template copies its objects into the receiving project, so
//! the project stays self-contained — #155 made self-containment an
//! invariant rather than a default, and a template that left a project
//! pointing at bytes it does not hold would break it on the next open.

use crate::document::{DawDocument, TrackNode};
use crate::error::DawResult;
use crate::id::{EntityId, ObjectId};
use crate::objects::ObjectStore;
use crate::project::DawProject;
use std::collections::HashMap;

/// The extension a template is written under.
pub const TEMPLATE_EXTENSION: &str = "dawtemplate";

/// A saved fragment: a manifest plus its objects, exactly like a
/// project.
#[derive(Clone, Debug)]
pub struct DawTemplate {
    /// The fragment, held in the same document type a project uses.
    /// Its `tracks` are the fragment's roots.
    document: DawDocument,
    objects: ObjectStore,
}

impl DawTemplate {
    pub fn new(document: DawDocument, objects: ObjectStore) -> Self {
        Self { document, objects }
    }

    pub fn document(&self) -> &DawDocument {
        &self.document
    }

    pub fn objects(&self) -> &ObjectStore {
        &self.objects
    }

    /// Capture some of a project's tracks as a template.
    ///
    /// Only the objects the selection actually references come along —
    /// a one-track template should not carry the other thirty-nine
    /// tracks' FX chunks.
    pub fn from_tracks(
        project: &DawProject,
        name: impl Into<String>,
        selected: &[EntityId],
    ) -> Self {
        let mut document = DawDocument::new(name);
        document.tempo_map = project.document().tempo_map.clone();

        for node in project
            .document()
            .tracks
            .iter()
            .filter(|t| selected.contains(&t.id))
        {
            document.tracks.push(node.clone());
        }

        let mut objects = ObjectStore::new();
        for id in document.referenced_objects() {
            if let Ok(bytes) = project.objects().get(&id) {
                objects.put(bytes.to_vec());
            }
        }

        Self { document, objects }
    }

    /// Insert this template's tracks into a project.
    ///
    /// Every entity is given a **fresh id**, so loading the same
    /// template twice produces two distinct sets of tracks rather than
    /// one set that silently collides. The returned map is old id ->
    /// new id, for callers that need to fix up their own references.
    ///
    /// The template's objects are copied in, keeping the project
    /// self-contained.
    pub fn instantiate(&self, project: &mut DawProject) -> DawResult<HashMap<EntityId, EntityId>> {
        let mut remap: HashMap<EntityId, EntityId> = HashMap::new();

        // Objects first: a track referencing a chunk the project does
        // not hold yet would fail its own save.
        for id in self.document.referenced_objects() {
            if let Ok(bytes) = self.objects.get(&id) {
                project.put_object(bytes.to_vec());
            }
        }

        let mut fresh: Vec<TrackNode> = Vec::new();
        for node in &self.document.tracks {
            let new_id = EntityId::new();
            remap.insert(node.id.clone(), new_id.clone());

            let mut copy = node.clone();
            copy.id = new_id.clone();
            copy.track.guid = new_id.to_string();
            fresh.push(copy);
        }

        // Parents are remapped after the fact, so a group template keeps
        // its shape rather than pointing at the source project's tracks.
        for node in &mut fresh {
            if let Some(parent) = node.parent.clone() {
                node.parent = remap.get(&parent).cloned();
            }
        }

        project.edit(|d| d.tracks.extend(fresh));
        Ok(remap)
    }

    /// Every object this template carries.
    pub fn referenced_objects(&self) -> Vec<ObjectId> {
        self.document.referenced_objects()
    }
}
