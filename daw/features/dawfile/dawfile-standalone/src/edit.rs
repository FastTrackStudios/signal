//! The typed manipulation surface.
//!
//! #156 asks for the format to be "easily manipulatable", which means a
//! typed API over the document rather than reaching into `Vec`s by hand.
//! `dawfile_reaper`'s `builder.rs` and `chunk_ops.rs` are the precedent; the
//! difference is that everything here addresses entities **by
//! [`EntityId`]**, never by position, so a caller physically cannot write
//! the bug the format is designed to prevent.
//!
//! ```
//! # use dawfile_standalone::{DawDocument, DocumentEdit, DocumentQuery};
//! let mut document = DawDocument::new("Demo");
//! let drums = document.add_track("Drums");
//! let kick = document.add_track("Kick");
//! document.set_parent(&kick, Some(drums.clone())).unwrap();
//!
//! // Reordering does not disturb identity: the child still names its
//! // parent, and the parent is still findable.
//! document.tracks.reverse();
//! document.reindex();
//! assert_eq!(document.track(&kick).unwrap().parent.as_ref(), Some(&drums));
//! ```

use crate::document::{
    DawDocument, EnvelopeNode, ItemNode, MarkerNode, ReceiveNode, SourceRef, TakeNode, TrackNode,
};
use crate::error::{DawError, DawResult};
use crate::id::EntityId;
use daw_proto::automation::{Envelope, EnvelopePoint, EnvelopeType};
use daw_proto::item::{Item, SourceType, Take};
use daw_proto::marker::Marker;
use daw_proto::primitives::{AutomationMode, Duration, Position, PositionInSeconds};
use daw_proto::track::Track;

/// Finding things by id.
///
/// Every lookup is by [`EntityId`]. There is deliberately no
/// `track_at(index)`.
pub trait DocumentQuery {
    /// The track with this id.
    fn track(&self, id: &EntityId) -> Option<&TrackNode>;
    /// The track with this id, mutably.
    fn track_mut(&mut self, id: &EntityId) -> Option<&mut TrackNode>;
    /// The item with this id, wherever it sits.
    fn item(&self, id: &EntityId) -> Option<&ItemNode>;
    /// The item with this id, mutably.
    fn item_mut(&mut self, id: &EntityId) -> Option<&mut ItemNode>;
    /// The take with this id, wherever it sits.
    fn take(&self, id: &EntityId) -> Option<&TakeNode>;
    /// The take with this id, mutably.
    fn take_mut(&mut self, id: &EntityId) -> Option<&mut TakeNode>;
    /// The envelope with this id — track, item or take.
    fn envelope(&self, id: &EntityId) -> Option<&EnvelopeNode>;
    /// The envelope with this id, mutably.
    fn envelope_mut(&mut self, id: &EntityId) -> Option<&mut EnvelopeNode>;
    /// The direct children of a folder track, in arrange order.
    fn children_of(&self, parent: &EntityId) -> Vec<&TrackNode>;
    /// Every item on a track, in order.
    fn items_of(&self, track: &EntityId) -> Vec<&ItemNode>;
    /// A track's active take on its item, if the item has one.
    fn active_take(&self, item: &EntityId) -> Option<&TakeNode>;
}

/// Changing things.
///
/// Structural operations return the new entity's id, so a caller composes by
/// id from the first line rather than fishing an index back out.
pub trait DocumentEdit {
    /// Append a track and return its id.
    fn add_track(&mut self, name: impl Into<String>) -> EntityId;
    /// Remove a track and everything under it. Children are re-parented to
    /// the removed track's parent rather than orphaned.
    fn remove_track(&mut self, id: &EntityId) -> DawResult<TrackNode>;
    /// Re-parent a track. Rejects a cycle rather than producing one.
    fn set_parent(&mut self, id: &EntityId, parent: Option<EntityId>) -> DawResult<()>;
    /// Move a track to a new position in arrange order, by naming the track
    /// it should sit before (`None` appends).
    fn move_track_before(&mut self, id: &EntityId, before: Option<&EntityId>) -> DawResult<()>;

    /// Add an item to a track and return its id.
    fn add_item(&mut self, track: &EntityId, position: f64, length: f64) -> DawResult<EntityId>;
    /// Remove an item.
    fn remove_item(&mut self, id: &EntityId) -> DawResult<ItemNode>;
    /// Move an item to another track, keeping its identity and its takes.
    fn move_item(&mut self, id: &EntityId, to_track: &EntityId) -> DawResult<()>;

    /// Add a take to an item and return its id.
    fn add_take(&mut self, item: &EntityId, source: SourceRef) -> DawResult<EntityId>;
    /// Make a take the active one, deactivating its siblings.
    fn set_active_take(&mut self, take: &EntityId) -> DawResult<()>;

    /// Add an envelope to a track and return its id.
    fn add_track_envelope(
        &mut self,
        track: &EntityId,
        envelope_type: EnvelopeType,
        name: impl Into<String>,
    ) -> DawResult<EntityId>;
    /// Add an envelope to an item and return its id.
    ///
    /// Scenario 1 of #149 needs four of these on one item, composited into
    /// its volume envelope — hence item envelopes being first-class rather
    /// than a track-envelope special case.
    fn add_item_envelope(
        &mut self,
        item: &EntityId,
        envelope_type: EnvelopeType,
        name: impl Into<String>,
    ) -> DawResult<EntityId>;
    /// Replace an envelope's points, keeping them sorted by time.
    fn set_envelope_points(&mut self, id: &EntityId, points: Vec<EnvelopePoint>) -> DawResult<()>;

    /// Add a marker and return its id.
    fn add_marker(&mut self, position: f64, name: impl Into<String>) -> EntityId;
    /// Add a region and return its id.
    fn add_region(&mut self, start: f64, end: f64, name: impl Into<String>) -> EntityId;

    /// Route `source` into `destination` as a post-fader stereo send at
    /// unity, replacing an existing send between the same pair.
    ///
    /// Named from the destination's side because that is where REAPER
    /// keeps the fact, and idempotent because a routing pass that runs
    /// twice should not double the gain into the bus.
    fn add_send(&mut self, source: &EntityId, destination: &EntityId) -> DawResult<()>;
    /// Remove the send from `source` into `destination`, if there is one.
    fn remove_send(&mut self, source: &EntityId, destination: &EntityId) -> DawResult<()>;
}

impl DocumentQuery for DawDocument {
    fn track(&self, id: &EntityId) -> Option<&TrackNode> {
        self.tracks.iter().find(|node| &node.id == id)
    }

    fn track_mut(&mut self, id: &EntityId) -> Option<&mut TrackNode> {
        self.tracks.iter_mut().find(|node| &node.id == id)
    }

    fn item(&self, id: &EntityId) -> Option<&ItemNode> {
        self.tracks
            .iter()
            .flat_map(|track| track.items.iter())
            .find(|node| &node.id == id)
    }

    fn item_mut(&mut self, id: &EntityId) -> Option<&mut ItemNode> {
        self.tracks
            .iter_mut()
            .flat_map(|track| track.items.iter_mut())
            .find(|node| &node.id == id)
    }

    fn take(&self, id: &EntityId) -> Option<&TakeNode> {
        self.tracks
            .iter()
            .flat_map(|track| track.items.iter())
            .flat_map(|item| item.takes.iter())
            .find(|node| &node.id == id)
    }

    fn take_mut(&mut self, id: &EntityId) -> Option<&mut TakeNode> {
        self.tracks
            .iter_mut()
            .flat_map(|track| track.items.iter_mut())
            .flat_map(|item| item.takes.iter_mut())
            .find(|node| &node.id == id)
    }

    fn envelope(&self, id: &EntityId) -> Option<&EnvelopeNode> {
        self.tracks.iter().find_map(|track| {
            track
                .envelopes
                .iter()
                .chain(track.items.iter().flat_map(|item| {
                    item.envelopes
                        .iter()
                        .chain(item.takes.iter().flat_map(|take| take.envelopes.iter()))
                }))
                .find(|node| &node.id == id)
        })
    }

    fn envelope_mut(&mut self, id: &EntityId) -> Option<&mut EnvelopeNode> {
        self.tracks.iter_mut().find_map(|track| {
            track
                .envelopes
                .iter_mut()
                .chain(track.items.iter_mut().flat_map(|item| {
                    item.envelopes.iter_mut().chain(
                        item.takes
                            .iter_mut()
                            .flat_map(|take| take.envelopes.iter_mut()),
                    )
                }))
                .find(|node| &node.id == id)
        })
    }

    fn children_of(&self, parent: &EntityId) -> Vec<&TrackNode> {
        self.tracks
            .iter()
            .filter(|node| node.parent.as_ref() == Some(parent))
            .collect()
    }

    fn items_of(&self, track: &EntityId) -> Vec<&ItemNode> {
        self.track(track)
            .map(|node| node.items.iter().collect())
            .unwrap_or_default()
    }

    fn active_take(&self, item: &EntityId) -> Option<&TakeNode> {
        self.item(item)
            .and_then(|node| node.takes.iter().find(|take| take.take.is_active))
    }
}

impl DocumentEdit for DawDocument {
    fn add_track(&mut self, name: impl Into<String>) -> EntityId {
        let id = EntityId::new();
        let track = Track::new(id.to_string(), self.tracks.len() as u32, name.into());
        self.tracks.push(TrackNode {
            id: id.clone(),
            track,
            parent: None,
            envelopes: Vec::new(),
            items: Vec::new(),
            fx_chain: None,
            input_fx_chain: None,
            comping: daw_proto::track::LaneComping::default(),
            receives: Vec::new(),
        });
        self.reindex();
        // Appending a row changes what the *previous* last row closes, so
        // the folder encoding is refreshed even though nothing was nested.
        self.rebuild_folder_encoding();
        id
    }

    fn remove_track(&mut self, id: &EntityId) -> DawResult<TrackNode> {
        let position = self
            .tracks
            .iter()
            .position(|node| &node.id == id)
            .ok_or_else(|| DawError::NoSuchEntity {
                kind: "track",
                id: id.to_string(),
            })?;
        let removed = self.tracks.remove(position);
        // Re-parent rather than orphan: a dangling parent id would fail
        // `check_invariants`, and silently deleting a folder's children is
        // never what anyone meant.
        for node in &mut self.tracks {
            if node.parent.as_ref() == Some(id) {
                node.parent = removed.parent.clone();
            }
            // A send out of a track that no longer exists is not a send.
            node.receives.retain(|receive| &receive.source != id);
        }
        self.reindex();
        self.rebuild_folder_encoding();
        Ok(removed)
    }

    fn set_parent(&mut self, id: &EntityId, parent: Option<EntityId>) -> DawResult<()> {
        if let Some(parent_id) = &parent {
            if parent_id == id {
                return Err(DawError::NoSuchEntity {
                    kind: "track",
                    id: format!("{id} cannot be its own parent"),
                });
            }
            if self.track(parent_id).is_none() {
                return Err(DawError::NoSuchEntity {
                    kind: "track",
                    id: parent_id.to_string(),
                });
            }
            // Walk up from the proposed parent; meeting `id` would close a
            // cycle, and a cyclic folder tree hangs every consumer.
            let mut cursor = Some(parent_id.clone());
            while let Some(current) = cursor {
                if &current == id {
                    return Err(DawError::NoSuchEntity {
                        kind: "track",
                        id: format!("re-parenting {id} under {parent_id} would make a cycle"),
                    });
                }
                cursor = self.track(&current).and_then(|node| node.parent.clone());
            }
        }
        let node = self.track_mut(id).ok_or_else(|| DawError::NoSuchEntity {
            kind: "track",
            id: id.to_string(),
        })?;
        node.parent = parent;
        self.reindex();
        self.rebuild_folder_encoding();
        Ok(())
    }

    fn move_track_before(&mut self, id: &EntityId, before: Option<&EntityId>) -> DawResult<()> {
        let from = self
            .tracks
            .iter()
            .position(|node| &node.id == id)
            .ok_or_else(|| DawError::NoSuchEntity {
                kind: "track",
                id: id.to_string(),
            })?;
        let node = self.tracks.remove(from);
        let to = match before {
            Some(anchor) => self
                .tracks
                .iter()
                .position(|candidate| &candidate.id == anchor)
                .ok_or_else(|| DawError::NoSuchEntity {
                    kind: "track",
                    id: anchor.to_string(),
                })?,
            None => self.tracks.len(),
        };
        self.tracks.insert(to, node);
        self.reindex();
        self.rebuild_folder_encoding();
        Ok(())
    }

    fn add_item(&mut self, track: &EntityId, position: f64, length: f64) -> DawResult<EntityId> {
        let id = EntityId::new();
        let node = self
            .track_mut(track)
            .ok_or_else(|| DawError::NoSuchEntity {
                kind: "track",
                id: track.to_string(),
            })?;
        let track_guid = node.id.to_string();
        node.items.push(ItemNode {
            id: id.clone(),
            item: Item {
                guid: id.to_string(),
                track_guid,
                position: PositionInSeconds::from_seconds(position),
                length: Duration::from_seconds(length),
                ..Default::default()
            },
            takes: Vec::new(),
            envelopes: Vec::new(),
        });
        self.reindex();
        Ok(id)
    }

    fn remove_item(&mut self, id: &EntityId) -> DawResult<ItemNode> {
        for track in &mut self.tracks {
            if let Some(position) = track.items.iter().position(|node| &node.id == id) {
                let removed = track.items.remove(position);
                self.reindex();
                return Ok(removed);
            }
        }
        Err(DawError::NoSuchEntity {
            kind: "item",
            id: id.to_string(),
        })
    }

    fn move_item(&mut self, id: &EntityId, to_track: &EntityId) -> DawResult<()> {
        if self.track(to_track).is_none() {
            return Err(DawError::NoSuchEntity {
                kind: "track",
                id: to_track.to_string(),
            });
        }
        let node = self.remove_item(id)?;
        let destination = self.track_mut(to_track).expect("checked above");
        destination.items.push(node);
        self.reindex();
        Ok(())
    }

    fn add_take(&mut self, item: &EntityId, source: SourceRef) -> DawResult<EntityId> {
        let id = EntityId::new();
        let node = self.item_mut(item).ok_or_else(|| DawError::NoSuchEntity {
            kind: "item",
            id: item.to_string(),
        })?;
        let first = node.takes.is_empty();
        let (source_type, path) = match &source {
            SourceRef::Empty => (SourceType::Empty, None),
            SourceRef::File { path, .. } => (SourceType::Audio, Some(path.clone())),
            SourceRef::Object { kind, .. } if kind == "MIDI" => (SourceType::Midi, None),
            SourceRef::Object { .. } => (SourceType::Unknown, None),
        };
        node.takes.push(TakeNode {
            id: id.clone(),
            take: Take {
                guid: id.to_string(),
                item_guid: node.id.to_string(),
                // The first take on a fresh item is the active one; later
                // takes are alternates until something says otherwise.
                is_active: first,
                source_type,
                source_file_path: path,
                is_midi: source_type == SourceType::Midi,
                ..Default::default()
            },
            source,
            stretch_markers: Vec::new(),
            envelopes: Vec::new(),
        });
        self.reindex();
        Ok(id)
    }

    fn set_active_take(&mut self, take: &EntityId) -> DawResult<()> {
        for track in &mut self.tracks {
            for item in &mut track.items {
                if item.takes.iter().any(|node| &node.id == take) {
                    for node in &mut item.takes {
                        node.take.is_active = &node.id == take;
                    }
                    self.reindex();
                    return Ok(());
                }
            }
        }
        Err(DawError::NoSuchEntity {
            kind: "take",
            id: take.to_string(),
        })
    }

    fn add_track_envelope(
        &mut self,
        track: &EntityId,
        envelope_type: EnvelopeType,
        name: impl Into<String>,
    ) -> DawResult<EntityId> {
        let name = name.into();
        let id = EntityId::derived(track, &name);
        let node = self
            .track_mut(track)
            .ok_or_else(|| DawError::NoSuchEntity {
                kind: "track",
                id: track.to_string(),
            })?;
        let owner = node.id.to_string();
        node.envelopes
            .push(new_envelope(id.clone(), owner, envelope_type, name));
        self.reindex();
        Ok(id)
    }

    fn add_item_envelope(
        &mut self,
        item: &EntityId,
        envelope_type: EnvelopeType,
        name: impl Into<String>,
    ) -> DawResult<EntityId> {
        let name = name.into();
        let id = EntityId::derived(item, &name);
        let node = self.item_mut(item).ok_or_else(|| DawError::NoSuchEntity {
            kind: "item",
            id: item.to_string(),
        })?;
        let owner = node.item.track_guid.clone();
        node.envelopes
            .push(new_envelope(id.clone(), owner, envelope_type, name));
        self.reindex();
        Ok(id)
    }

    fn set_envelope_points(&mut self, id: &EntityId, points: Vec<EnvelopePoint>) -> DawResult<()> {
        let node = self
            .envelope_mut(id)
            .ok_or_else(|| DawError::NoSuchEntity {
                kind: "envelope",
                id: id.to_string(),
            })?;
        node.points = points;
        // An envelope is a function of time, so out-of-order points are not
        // a different envelope, they are a broken one. Sorting here means no
        // caller has to remember.
        node.points.sort_by(|left, right| {
            left.time
                .as_seconds()
                .partial_cmp(&right.time.as_seconds())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        node.sync();
        Ok(())
    }

    fn add_marker(&mut self, position: f64, name: impl Into<String>) -> EntityId {
        // Minted in REAPER's brace form rather than as a bare UUID: the
        // `.rpp` importer only recognises a marker GUID that looks like
        // one, so a bare id would come back derived and the marker would
        // change identity on every export/import round trip.
        let id = EntityId::adopt(format!(
            "{{{}}}",
            uuid::Uuid::new_v4().to_string().to_uppercase()
        ));
        self.markers.push(MarkerNode {
            id: id.clone(),
            marker: Marker {
                id: None,
                position: Position::from_time(PositionInSeconds::from_seconds(position)),
                name: name.into(),
                color: None,
                guid: Some(id.to_string()),
                lane: None,
            },
            region_end_seconds: None,
        });
        id
    }

    fn add_region(&mut self, start: f64, end: f64, name: impl Into<String>) -> EntityId {
        let id = self.add_marker(start, name);
        if let Some(node) = self.markers.iter_mut().find(|node| node.id == id) {
            node.region_end_seconds = Some(end);
        }
        id
    }

    fn add_send(&mut self, source: &EntityId, destination: &EntityId) -> DawResult<()> {
        if self.track(source).is_none() {
            return Err(DawError::NoSuchEntity {
                kind: "track",
                id: source.to_string(),
            });
        }
        if source == destination {
            return Err(DawError::NoSuchEntity {
                kind: "track",
                id: format!("{source} cannot send to itself"),
            });
        }
        let node = self
            .track_mut(destination)
            .ok_or_else(|| DawError::NoSuchEntity {
                kind: "track",
                id: destination.to_string(),
            })?;
        let receive = ReceiveNode::new(source.clone());
        match node
            .receives
            .iter_mut()
            .find(|existing| &existing.source == source)
        {
            Some(existing) => *existing = receive,
            None => node.receives.push(receive),
        }
        self.reindex();
        Ok(())
    }

    fn remove_send(&mut self, source: &EntityId, destination: &EntityId) -> DawResult<()> {
        let node = self
            .track_mut(destination)
            .ok_or_else(|| DawError::NoSuchEntity {
                kind: "track",
                id: destination.to_string(),
            })?;
        node.receives.retain(|existing| &existing.source != source);
        self.reindex();
        Ok(())
    }
}

fn new_envelope(
    id: EntityId,
    owner: String,
    envelope_type: EnvelopeType,
    name: String,
) -> EnvelopeNode {
    EnvelopeNode {
        id,
        envelope: Envelope {
            track_guid: owner,
            envelope_type,
            name,
            fx_guid: None,
            param_index: None,
            visible: true,
            armed: false,
            automation_mode: AutomationMode::TrimRead,
            // A new envelope is overlaid until something asks for a lane.
            in_own_lane: false,
            lane_height: 0,
            automation_item_count: 0,
            point_count: 0,
        },
        points: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_proto::automation::EnvelopeShape;

    fn point(time: f64, value: f64) -> EnvelopePoint {
        EnvelopePoint {
            index: 0,
            time: PositionInSeconds::from_seconds(time),
            value,
            shape: EnvelopeShape::Linear,
            tension: 0.0,
            selected: false,
        }
    }

    #[test]
    fn entities_are_found_by_id_after_reordering() {
        let mut document = DawDocument::new("Demo");
        let first = document.add_track("Kick");
        let second = document.add_track("Snare");
        let item = document.add_item(&first, 0.0, 4.0).expect("add item");

        document.tracks.swap(0, 1);
        document.reindex();

        assert_eq!(document.track(&first).expect("kick").track.name, "Kick");
        assert_eq!(document.track(&second).expect("snare").track.name, "Snare");
        // The item moved with its track without anyone updating an index.
        assert_eq!(
            document.item(&item).expect("item").item.track_guid,
            first.to_string()
        );
        assert!(document.check_invariants().is_empty());
    }

    #[test]
    fn removing_a_folder_reparents_its_children_instead_of_orphaning_them() {
        let mut document = DawDocument::new("Demo");
        let bus = document.add_track("Drums");
        let child = document.add_track("Kick");
        document
            .set_parent(&child, Some(bus.clone()))
            .expect("parent");

        document.remove_track(&bus).expect("remove");

        assert_eq!(document.track(&child).expect("kick").parent, None);
        assert!(document.check_invariants().is_empty());
    }

    #[test]
    fn a_parenting_cycle_is_refused() {
        let mut document = DawDocument::new("Demo");
        let outer = document.add_track("Outer");
        let inner = document.add_track("Inner");
        document
            .set_parent(&inner, Some(outer.clone()))
            .expect("parent");

        assert!(document.set_parent(&outer, Some(inner)).is_err());
        assert!(document.set_parent(&outer, Some(outer.clone())).is_err());
        assert!(document.check_invariants().is_empty());
    }

    #[test]
    fn moving_an_item_between_tracks_keeps_its_identity_and_takes() {
        let mut document = DawDocument::new("Demo");
        let from = document.add_track("Scratch");
        let to = document.add_track("Comp");
        let item = document.add_item(&from, 1.0, 2.0).expect("add item");
        let take = document
            .add_take(
                &item,
                SourceRef::File {
                    path: "Media/vox.wav".into(),
                    kind: "WAVE".into(),
                },
            )
            .expect("add take");

        document.move_item(&item, &to).expect("move");

        assert!(document.items_of(&from).is_empty());
        assert_eq!(document.items_of(&to).len(), 1);
        assert_eq!(
            document.take(&take).expect("take").take.item_guid,
            item.to_string()
        );
        assert_eq!(
            document.item(&item).expect("item").item.track_guid,
            to.to_string()
        );
        assert!(document.check_invariants().is_empty());
    }

    #[test]
    fn exactly_one_take_is_active() {
        let mut document = DawDocument::new("Demo");
        let track = document.add_track("Vox");
        let item = document.add_item(&track, 0.0, 4.0).expect("add item");
        let first = document.add_take(&item, SourceRef::Empty).expect("take 1");
        let second = document.add_take(&item, SourceRef::Empty).expect("take 2");

        assert!(document.take(&first).expect("first").take.is_active);
        assert_eq!(document.active_take(&item).expect("active").id, first);

        document.set_active_take(&second).expect("switch");
        assert_eq!(document.active_take(&item).expect("active").id, second);
        assert_eq!(
            document
                .item(&item)
                .expect("item")
                .takes
                .iter()
                .filter(|take| take.take.is_active)
                .count(),
            1
        );
    }

    #[test]
    fn envelope_points_are_kept_in_time_order() {
        let mut document = DawDocument::new("Demo");
        let track = document.add_track("Vox");
        let envelope = document
            .add_track_envelope(&track, EnvelopeType::Volume, "VOLENV2")
            .expect("envelope");

        document
            .set_envelope_points(&envelope, vec![point(4.0, 0.2), point(1.0, 0.9)])
            .expect("points");

        let node = document.envelope(&envelope).expect("envelope");
        assert_eq!(node.points[0].time.as_seconds(), 1.0);
        assert_eq!(node.points[1].time.as_seconds(), 4.0);
        assert_eq!(node.envelope.point_count, 2);
        assert_eq!(node.points[1].index, 1);
    }

    #[test]
    fn an_item_carries_four_source_envelopes_plus_its_composite() {
        // Scenario 1 of #149: gating, compression, sibilance and de-breathing
        // are generated as four separate envelopes on the item, composited
        // into the item's one volume envelope.
        let mut document = DawDocument::new("Demo");
        let track = document.add_track("Lead Vox");
        let item = document.add_item(&track, 0.0, 30.0).expect("item");

        for name in ["Gating", "Compression", "Sibilance", "De-Breathing"] {
            document
                .add_item_envelope(&item, EnvelopeType::Volume, name)
                .expect("source envelope");
        }
        let composite = document
            .add_item_envelope(&item, EnvelopeType::Volume, "VOLENV")
            .expect("composite");

        assert_eq!(document.item(&item).expect("item").envelopes.len(), 5);
        // Each has its own id, so recomputing the composite from any one of
        // them is addressable rather than positional.
        assert!(document.envelope(&composite).is_some());
        assert!(document.check_invariants().is_empty());
    }

    #[test]
    fn missing_entities_are_named_in_the_error() {
        let mut document = DawDocument::new("Demo");
        let absent = EntityId::adopt("{nope}");
        let error = document.add_item(&absent, 0.0, 1.0).unwrap_err();
        assert!(error.to_string().contains("{nope}"), "{error}");
    }
}
