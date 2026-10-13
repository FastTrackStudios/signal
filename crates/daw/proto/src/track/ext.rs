//! Generic convenience helpers layered over the raw `Tracks` / `Items` /
//! `Projects` service traits — selection scoping, name lookup, moving items
//! between tracks, and `ProjectContext::Current`-defaulted mutators. None
//! of this is REAPER-specific or tied to any particular domain (session's
//! Track Manager, dynamic-template, anything else driving tracks); it's
//! plumbing every track-editing feature needs, so it lives once here
//! rather than being reinvented per feature.
//!
//! Undo-block wrapping is deliberately not here: `#[action(undo)]` gets it
//! from the action backend (see `daw-reaper`'s `ActionBackend` impl), and
//! anything outside an action can call `Projects::begin_undo_block` /
//! `end_undo_block` directly.
//!
//! Blanket-impl'd for any backend that already speaks the three traits —
//! `daw::reaper::Reaper` and `daw_standalone::sync::Standalone` get it for
//! free, same as `Tracks`/`Items`/`Projects` themselves.

use crate::DawError;
use crate::item::{ItemRef, Items};
use crate::project::{ProjectContext, Projects};
use crate::track::{Track, TrackRef, Tracks};

/// A subtree to create: one track plus, recursively, its children.
///
/// The nested counterpart to the flat [`TrackNode`](super::TrackNode) /
/// [`FolderDepthChange`](super::FolderDepthChange) representation a DAW
/// actually stores — [`TracksExt::append_shape`] flattens one into the
/// other on the way to the backend, and
/// [`TrackTree::shape_of_children`] reads an existing subtree back out as
/// one (so "give the new channel the same mics the old one has" is a
/// read-then-append, not a hand-rolled recursion).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackShape {
    pub name: String,
    pub children: Vec<TrackShape>,
}

impl TrackShape {
    pub fn leaf(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            children: Vec::new(),
        }
    }

    pub fn with_children(name: impl Into<String>, children: Vec<TrackShape>) -> Self {
        Self {
            name: name.into(),
            children,
        }
    }

    /// Flatten into `(name, folder_depth)` pairs in mixer order, with
    /// `folder_depth` in the DAW's relative-depth encoding (`1` opens a
    /// folder, `0` plain, negative closes that many levels). The final
    /// entry closes one extra level so the whole run sits inside its
    /// parent.
    pub fn flatten(shape: &[TrackShape]) -> Vec<(String, i32)> {
        fn walk(shape: &[TrackShape], out: &mut Vec<(String, i32)>) {
            for node in shape {
                out.push((
                    node.name.clone(),
                    if node.children.is_empty() { 0 } else { 1 },
                ));
                if !node.children.is_empty() {
                    walk(&node.children, out);
                    if let Some(last) = out.last_mut() {
                        last.1 -= 1;
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(shape, &mut out);
        if let Some(last) = out.last_mut() {
            last.1 -= 1;
        }
        out
    }
}

/// Where a new last child goes, and what its arrival costs the tracks
/// already there.
///
/// Worked out from a [`TrackTree`] snapshot alone, which is what makes
/// the arithmetic testable: the bug this type exists to fix was in the
/// sums, not in the backend calls, and a test that needed a backend to
/// reach them is a test nobody writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppendPlan {
    /// The mixer index the newcomer is inserted at.
    pub index: u32,
    /// The track currently terminating the subtree and the depth it must
    /// take instead, when there is one to fix.
    pub retarget: Option<(String, i32)>,
    /// How many levels the newcomer closes — the parent, plus whatever
    /// the old terminator handed over.
    pub closes: i32,
}

/// An immutable snapshot of a project's whole track list, for navigating
/// the folder tree without re-querying the backend per lookup.
///
/// The per-call helpers on [`TracksExt`] (`children_of`, `get_track`,
/// `subtree_end_index`) each re-fetch the entire track list, so walking a
/// tree with them is quadratic and — over a real RPC transport — a
/// round-trip per node. Take one `TrackTree` and navigate it in memory
/// instead.
///
/// Being a snapshot is the point *and* the caveat: it reflects the project
/// as of when it was taken, so re-take it after any mutation rather than
/// reusing a stale one to compute indices.
#[derive(Debug, Clone)]
pub struct TrackTree {
    /// Sorted by `index` (mixer order), so positional walks are just
    /// slice order.
    tracks: Vec<Track>,
}

impl TrackTree {
    pub fn new(mut tracks: Vec<Track>) -> Self {
        tracks.sort_by_key(|track| track.index);
        Self { tracks }
    }

    /// Every track, in mixer order.
    pub fn all(&self) -> &[Track] {
        &self.tracks
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn get(&self, guid: &str) -> Option<&Track> {
        self.tracks.iter().find(|track| track.guid == guid)
    }

    pub fn at_index(&self, index: u32) -> Option<&Track> {
        self.tracks.iter().find(|track| track.index == index)
    }

    /// Direct children of `guid`, in mixer order.
    pub fn children_of<'a>(&'a self, guid: &'a str) -> impl Iterator<Item = &'a Track> + 'a {
        self.tracks
            .iter()
            .filter(move |track| track.parent_guid.as_deref() == Some(guid))
    }

    pub fn parent_of(&self, track: &Track) -> Option<&Track> {
        self.get(track.parent_guid.as_deref()?)
    }

    /// How many folders `track` sits inside. A top-level track is 0.
    ///
    /// Walked through `parent_guid` rather than summed from
    /// `folder_depth`, because the depths are what an append is in the
    /// middle of fixing and the parent links are not.
    pub fn nesting_of(&self, track: &Track) -> usize {
        let mut nesting = 0;
        let mut at = track;
        while let Some(parent) = self.parent_of(at) {
            nesting += 1;
            at = parent;
            // A cycle in `parent_guid` would hang this. It should not
            // happen; a project that has one should not also hang.
            if nesting > self.tracks.len() {
                break;
            }
        }
        nesting
    }

    /// The subtree under `guid` as nested [`TrackShape`]s — read entirely
    /// from this snapshot, no per-node backend query. Pair with
    /// [`TracksExt::append_shape`] to clone an existing subtree's shape
    /// onto a new sibling.
    pub fn shape_of_children(&self, guid: &str) -> Vec<TrackShape> {
        self.children_of(guid)
            .map(|track| {
                TrackShape::with_children(track.name.clone(), self.shape_of_children(&track.guid))
            })
            .collect()
    }

    /// Plan the append of a new last child of `guid`.
    ///
    /// A terminator closes every folder from its own parent outward.
    /// Once a newcomer follows it, it must stop at the folders strictly
    /// INSIDE `guid`, and the newcomer takes over the rest. The old rule
    /// here was "one level fewer", which is right only when the
    /// terminator's outermost closed folder is `guid` itself —
    /// appending onto a container whose last mic closes amp *and*
    /// channel *and* part closed the folder early, and the newcomer
    /// landed outside the container it was meant to join.
    ///
    /// A parent with no children yet is its own terminator, and whatever
    /// IT was closing passes to the newcomer too: a leaf channel closing
    /// its part becomes a folder, and the part still has to be closed by
    /// somebody.
    pub fn plan_append(&self, guid: &str) -> Option<AppendPlan> {
        let parent = self.get(guid)?;
        let index = self.subtree_end_index(guid).unwrap_or(parent.index + 1);
        let Some(previous) = self.at_index(index.saturating_sub(1)) else {
            return Some(AppendPlan {
                index,
                retarget: None,
                closes: 1,
            });
        };
        if previous.guid == guid {
            return Some(AppendPlan {
                index,
                retarget: None,
                closes: 1 + (-parent.folder_depth).max(0),
            });
        }
        if previous.folder_depth >= 0 {
            return Some(AppendPlan {
                index,
                retarget: None,
                closes: 1,
            });
        }
        let closed = -previous.folder_depth;
        let inside = i32::try_from(
            self.nesting_of(previous)
                .saturating_sub(self.nesting_of(parent) + 1),
        )
        .unwrap_or(0);
        let keep = inside.min(closed);
        Some(AppendPlan {
            index,
            retarget: Some((previous.guid.clone(), -keep)),
            closes: closed - keep,
        })
    }

    /// The index just past the end of `guid`'s subtree — where a new last
    /// child should be inserted, or `guid`'s own next-sibling position if
    /// it has no children yet.
    pub fn subtree_end_index(&self, guid: &str) -> Option<u32> {
        let parent = self.get(guid)?;
        // A track that does not open a folder has no subtree, so its
        // subtree ends immediately after it. Without this the walk below
        // starts at a cumulative depth of zero and the very next track
        // satisfies its stop condition — returning a position one track
        // too far, which put a first child on the far side of whatever
        // happened to follow its parent. The doc above always said this
        // is what it does; the code did not.
        if parent.folder_depth <= 0 {
            return Some(parent.index + 1);
        }
        let mut depth = 0;
        for track in self.tracks.iter().filter(|t| t.index >= parent.index) {
            depth += track.folder_depth;
            if track.index > parent.index && depth <= 0 {
                return Some(track.index + 1);
            }
        }
        Some(parent.index + 1)
    }
}

pub trait TracksExt: Tracks + Items + Projects {
    // ── Snapshot ────────────────────────────────────────────────────

    /// One fetch of the whole track list, for tree navigation without a
    /// query per node. See [`TrackTree`].
    fn track_tree(&self) -> TrackTree {
        TrackTree::new(self.all(ProjectContext::Current))
    }

    // ── Selection ───────────────────────────────────────────────────

    /// The single currently-selected track, or an error if nothing is
    /// selected. Most track-editing commands are scoped to "whatever's
    /// selected" — this is the first call almost all of them make.
    fn selected_scope(&self) -> Result<Track, DawError> {
        self.selected(ProjectContext::Current)
            .into_iter()
            .next()
            .ok_or_else(|| DawError::NotFound("no track is selected".into()))
    }

    /// Clear the current selection and select exactly `guid`. Use
    /// [`add_to_selection`](Self::add_to_selection) instead to extend an
    /// existing selection rather than replace it.
    fn select(&self, guid: &str) -> Result<(), DawError> {
        let project = ProjectContext::Current;
        self.clear_selection(project.clone())?;
        Tracks::set_selected(self, project, TrackRef::Guid(guid.to_string()), true)
    }

    /// Select `guid` in addition to whatever's already selected.
    fn add_to_selection(&self, guid: &str) -> Result<(), DawError> {
        Tracks::set_selected(
            self,
            ProjectContext::Current,
            TrackRef::Guid(guid.to_string()),
            true,
        )
    }

    // ── Lookup ──────────────────────────────────────────────────────

    /// One track by guid, or an "invalid object" error if it no longer
    /// exists (deleted between when a caller last fetched it and now).
    fn get_track(&self, guid: &str) -> Result<Track, DawError> {
        Tracks::get(
            self,
            ProjectContext::Current,
            TrackRef::Guid(guid.to_string()),
        )
        .ok_or_else(|| DawError::invalid_object("track", guid))
    }

    /// One track by mixer position.
    fn track_at_index(&self, index: u32) -> Option<Track> {
        Tracks::get(self, ProjectContext::Current, TrackRef::Index(index))
    }

    /// Find a track by exact name. Errors (rather than silently picking
    /// one) if the name is ambiguous — callers scope by selection first
    /// for anything that isn't a top-level, presumed-unique name.
    fn find_track(&self, name: &str) -> Result<Track, DawError> {
        let mut matches = self
            .all(ProjectContext::Current)
            .into_iter()
            .filter(|track| track.name == name);
        let found = matches
            .next()
            .ok_or_else(|| DawError::not_found("track", name))?;
        if matches.next().is_some() {
            return Err(DawError::operation_failed(format!(
                "multiple tracks named {name:?}; disambiguate by guid"
            )));
        }
        Ok(found)
    }

    /// Every track whose direct parent is `guid`, in mixer order. Fetches
    /// the whole track list — prefer [`track_tree`](Self::track_tree) when
    /// walking more than one level.
    fn children_of(&self, guid: &str) -> Vec<Track> {
        self.track_tree().children_of(guid).cloned().collect()
    }

    /// See [`TrackTree::subtree_end_index`]. Fetches the whole track list;
    /// prefer a [`TrackTree`] when you need more than one lookup.
    fn subtree_end_index(&self, guid: &str) -> Option<u32> {
        self.track_tree().subtree_end_index(guid)
    }

    // ── Mutation (all on `ProjectContext::Current`) ─────────────────

    /// Insert a new track and return its guid. With no other tracks or
    /// selection context to place it relative to, it lands at the top
    /// level, at the end of the track list.
    fn insert_track(&self, name: &str) -> Result<String, DawError> {
        self.add(ProjectContext::Current, name, None)
    }

    /// Insert a new track at a specific mixer position; returns its guid.
    fn insert_track_at(&self, name: &str, index: u32) -> Result<String, DawError> {
        self.add(ProjectContext::Current, name, Some(index))
    }

    /// Set a track's folder-depth change (`1` opens a folder, `0` is a
    /// plain track, negative closes that many levels).
    fn set_depth(&self, guid: &str, depth: i32) -> Result<(), DawError> {
        self.set_folder_depth(
            ProjectContext::Current,
            TrackRef::Guid(guid.to_string()),
            depth,
        )
    }

    /// Move every item on `from_guid` onto `to_guid`.
    fn move_items(&self, from_guid: &str, to_guid: &str) -> Result<(), DawError> {
        let project = ProjectContext::Current;
        let target = TrackRef::Guid(to_guid.to_string());
        for item in self.get_items(project.clone(), TrackRef::Guid(from_guid.to_string())) {
            self.move_to_track(project.clone(), ItemRef::Guid(item.guid), target.clone())?;
        }
        Ok(())
    }

    /// Create a single new track as the last child of `parent_guid`.
    fn append_child(&self, parent_guid: &str, name: &str) -> Result<(), DawError> {
        self.append_shape(parent_guid, &[TrackShape::leaf(name)])
    }

    /// Create `shape` (a nested subtree) as the last children of
    /// `parent_guid`, opening `parent_guid` as a folder if it isn't one
    /// already.
    fn append_shape(&self, parent_guid: &str, shape: &[TrackShape]) -> Result<(), DawError> {
        let (insertion_index, closes) = self.prepare_append(parent_guid)?;
        self.set_depth(parent_guid, 1)?;
        let mut flat = TrackShape::flatten(shape);
        if let Some(last) = flat.last_mut() {
            // `flatten` already closes one level for the parent. Anything
            // beyond that is what the old terminator handed over.
            last.1 -= closes - 1;
        }
        self.insert_flat_at(&flat, insertion_index)
    }

    /// Create `shape` starting at an explicit mixer position, without any
    /// parent bookkeeping. Prefer [`append_shape`](Self::append_shape) —
    /// this is the escape hatch for callers that already know exactly
    /// where the subtree goes (e.g. mid-restructure, when the tree is
    /// briefly not well-formed enough for a subtree-end walk).
    fn insert_shape_at(&self, shape: &[TrackShape], index: u32) -> Result<(), DawError> {
        self.insert_flat_at(&TrackShape::flatten(shape), index)
    }

    /// The same, from already-flattened `(name, depth)` pairs — for the
    /// callers that have adjusted a depth before inserting.
    fn insert_flat_at(&self, flat: &[(String, i32)], index: u32) -> Result<(), DawError> {
        for (offset, (name, depth)) in flat.iter().enumerate() {
            let at = index + u32::try_from(offset).unwrap_or(u32::MAX);
            let track = self.insert_track_at(name, at)?;
            self.set_depth(&track, *depth)?;
        }
        Ok(())
    }

    /// Carry out a [`TrackTree::plan_append`]: fix the old terminator and
    /// say where the newcomer goes and how much it closes.
    ///
    /// The plan is computed from a single snapshot taken *before* the
    /// fixup — the fixup changes what a fresh subtree-end walk would
    /// see, so recomputing afterwards silently yields the wrong
    /// position.
    fn prepare_append(&self, parent_guid: &str) -> Result<(u32, i32), DawError> {
        let plan = self
            .track_tree()
            .plan_append(parent_guid)
            .ok_or_else(|| DawError::invalid_object("track", parent_guid))?;
        if let Some((guid, depth)) = plan.retarget {
            self.set_depth(&guid, depth)?;
        }
        Ok((plan.index, plan.closes))
    }
}

impl<D: Tracks + Items + Projects + ?Sized> TracksExt for D {}

#[cfg(test)]
mod append_plan_tests {
    use super::{AppendPlan, TrackTree};
    use crate::Track;

    /// One track. `parent` is the guid it hangs under, `depth` REAPER's
    /// relative folder depth.
    fn track(index: u32, guid: &str, parent: Option<&str>, depth: i32) -> Track {
        Track {
            guid: guid.to_owned(),
            name: guid.to_uppercase(),
            index,
            parent_guid: parent.map(str::to_owned),
            folder_depth: depth,
            ..Track::default()
        }
    }

    /// A guitar's worth of nesting: part > channel > amp > mic, where the
    /// single mic closes all three at once. This is the shape #90 was
    /// found on.
    fn three_deep() -> TrackTree {
        TrackTree::new(vec![
            track(0, "part", None, 1),
            track(1, "channel", Some("part"), 1),
            track(2, "amp", Some("channel"), 1),
            track(3, "mic", Some("amp"), -3),
        ])
    }

    /// Appending a second mic under the amp. The old mic stops closing
    /// anything; the newcomer closes all three.
    #[test]
    fn a_terminator_closing_three_levels_hands_over_all_three() {
        let plan = three_deep().plan_append("amp").expect("the amp is there");
        assert_eq!(
            plan,
            AppendPlan {
                index: 4,
                retarget: Some(("mic".to_owned(), 0)),
                closes: 3,
            },
            "the old rule gave the mic -2 and the newcomer -1, which closed \
             the amp early and put the new mic outside it"
        );
    }

    /// Appending a second amp under the channel. The mic keeps closing
    /// the amp and hands over the channel and the part.
    #[test]
    fn a_terminator_keeps_the_levels_inside_the_parent() {
        let plan = three_deep()
            .plan_append("channel")
            .expect("the channel is there");
        assert_eq!(
            plan,
            AppendPlan {
                index: 4,
                retarget: Some(("mic".to_owned(), -1)),
                closes: 2,
            }
        );
    }

    /// And appending at the top: the mic keeps amp and channel, the
    /// newcomer takes the part.
    #[test]
    fn appending_at_the_top_leaves_the_inner_folders_closed() {
        let plan = three_deep().plan_append("part").expect("the part is there");
        assert_eq!(
            plan,
            AppendPlan {
                index: 4,
                retarget: Some(("mic".to_owned(), -2)),
                closes: 1,
            }
        );
    }

    /// The case the old rule got right, kept so a future simplification
    /// has to stay right about it: a grandchild closing two levels,
    /// appending a third channel.
    #[test]
    fn a_grandchild_closing_two_levels_still_hands_over_one() {
        let tree = TrackTree::new(vec![
            track(0, "part", None, 1),
            track(1, "l", Some("part"), 1),
            track(2, "l-mic", Some("l"), -1),
            track(3, "r", Some("part"), 1),
            track(4, "r-mic", Some("r"), -2),
        ]);
        let plan = tree.plan_append("part").expect("the part is there");
        assert_eq!(
            plan,
            AppendPlan {
                index: 5,
                retarget: Some(("r-mic".to_owned(), -1)),
                closes: 1,
            }
        );
    }

    /// A parent with nothing under it yet is its own terminator, and
    /// whatever it was closing passes to the newcomer — or the folder it
    /// used to close never closes at all.
    #[test]
    fn a_childless_parent_hands_over_what_it_was_closing() {
        let tree = TrackTree::new(vec![
            track(0, "part", None, 1),
            track(1, "channel", Some("part"), -1),
        ]);
        let plan = tree.plan_append("channel").expect("the channel is there");
        assert_eq!(
            plan,
            AppendPlan {
                index: 2,
                retarget: None,
                // The channel it is about to open, and the part the
                // channel was closing as a leaf.
                closes: 2,
            }
        );
    }

    /// A plain childless parent mid-list closes only itself.
    #[test]
    fn a_childless_parent_closing_nothing_costs_one_level() {
        let tree = TrackTree::new(vec![track(0, "a", None, 0), track(1, "b", None, 0)]);
        let plan = tree.plan_append("a").expect("a is there");
        assert_eq!(
            plan,
            AppendPlan {
                index: 1,
                retarget: None,
                closes: 1,
            }
        );
    }

    /// A leaf parent's first child goes straight after it, not after
    /// whatever follows it.
    ///
    /// `subtree_end_index` walked from a cumulative depth of zero, so
    /// the very next track met its stop condition and the index came
    /// back one too far — a first child landed on the far side of its
    /// parent's next sibling.
    #[test]
    fn a_first_child_goes_straight_after_its_parent() {
        let tree = TrackTree::new(vec![
            track(0, "a", None, 0),
            track(1, "b", None, 0),
            track(2, "c", None, 0),
        ]);
        assert_eq!(tree.subtree_end_index("a"), Some(1));
        assert_eq!(
            tree.plan_append("a").map(|p| p.index),
            Some(1),
            "the child was put after b"
        );
    }

    /// Nesting is walked through the parent links, not summed from the
    /// depths — the depths are what an append is in the middle of
    /// fixing.
    #[test]
    fn nesting_is_counted_through_the_parents() {
        let tree = three_deep();
        let of = |guid: &str| tree.nesting_of(tree.get(guid).expect(guid));
        assert_eq!(
            (of("part"), of("channel"), of("amp"), of("mic")),
            (0, 1, 2, 3)
        );
    }
}
