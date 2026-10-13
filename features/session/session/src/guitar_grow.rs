//! Growing a guitar part: double, add an arrangement, add a layer, add a
//! source.
//!
//! r[impl flow.guitars.dimensions]
//! r[impl flow.guitars.grow]
//! r[impl flow.guitars.mixing.source-defaults]
//! r[impl flow.guitars.acoustics]
//!
//! The four gestures are one operation at four dimensions. A part is
//! organised Arrangement → Layer → Channel → `MultiMic`, and **a level is a
//! folder only when it has more than one member**, so growing at a
//! dimension is always the same two-step move:
//!
//! 1. If that level does not exist yet, insert it — the container's own
//!    content becomes the level's *first* member, which is the only
//!    moment a folder is allowed to appear.
//! 2. Append the next member, named from the template's vocabulary for
//!    that dimension, carrying a copy of the first member's subtree.
//!
//! Nothing is restructured: items stay on the tracks that hold them, the
//! existing tracks keep their guids, and no send is touched.
//!
//! Acoustics need no code of their own. Every name here comes out of
//! `dynamic_template::track_schema`, which answers with the acoustic's
//! vocabulary under an acoustic track and the electric's under an
//! electric one (`flow.guitars.acoustics`).

use daw::service::{
    DawError, DawResult, Items, Projects, Track, TrackRef, TrackShape, TrackTree, Tracks, TracksExt,
};
use dynamic_template::source_defaults::{self, Balance, Source};
use dynamic_template::track_schema::{self, TrackDimension};

// The action contract lives in session-proto (traits are protocol); this
// file is the implementation. `register_guitar_grow_actions` is
// macro-emitted alongside the trait there.
pub use session_proto::guitar_grow::{GuitarGrowActions, register_guitar_grow_actions};

/// Session's guitar-part grower for one DAW backend.
///
/// Wraps whichever `D` for the duration it is used, always acting on
/// `ProjectContext::Current`. Production wraps `daw::reaper::Reaper`;
/// tests wrap `daw_standalone::sync::Standalone` — the same trait impl
/// either way, which is what lets the scenario tests drive the real
/// gesture without a REAPER process or a window.
pub struct GuitarGrow<D> {
    daw: D,
}

impl<D> GuitarGrow<D> {
    pub const fn new(daw: D) -> Self {
        Self { daw }
    }
}

/// Derefs to the wrapped backend so `Tracks`/`Items`/`Projects`/
/// `TracksExt` methods read as `self.method(...)`; `GuitarGrow`'s own
/// methods still resolve to themselves first.
impl<D> std::ops::Deref for GuitarGrow<D> {
    type Target = D;
    fn deref(&self) -> &D {
        &self.daw
    }
}

impl<D: Tracks + Items + Projects> GuitarGrowActions for GuitarGrow<D> {
    fn double(&self) -> DawResult<()> {
        let container = self.container_for(TrackDimension::Channel)?;
        self.grow_level(&container.guid, TrackDimension::Channel)?;
        // A doubled part's new side is a whole channel of sources the
        // configuration has never balanced.
        self.apply_source_defaults(&container.guid)
    }

    fn add_arrangement(&self) -> DawResult<()> {
        let container = self.container_for(TrackDimension::Arrangement)?;
        self.grow_level(&container.guid, TrackDimension::Arrangement)?;
        self.apply_source_defaults(&container.guid)
    }

    fn add_layer(&self) -> DawResult<()> {
        let container = self.container_for(TrackDimension::Layer)?;
        self.grow_level(&container.guid, TrackDimension::Layer)?;
        self.apply_source_defaults(&container.guid)
    }

    /// Every channel of the part, not one of them: a source is a property
    /// of the configuration, so a double gets the new mic on both sides.
    fn add_source(&self) -> DawResult<()> {
        let part = self.selected_scope()?;
        let channels = self.channels_of(&part.guid);

        // One name for the whole part. Asking each channel separately
        // would let two sides drift apart the moment one of them is
        // missing a source the other has.
        let reference = channels
            .first()
            .ok_or_else(|| DawError::not_found("a channel to add a source to", &part.name))?;
        let name = self.next_member_name(reference, TrackDimension::MultiMic)?;

        for channel in &channels {
            self.grow_level_to(&channel.guid, TrackDimension::MultiMic, &name)?;
        }
        self.apply_source_defaults(&part.guid)
    }
}

// ── resolving what a gesture acts on ────────────────────────────────

impl<D: Tracks + Items + Projects> GuitarGrow<D> {
    /// The track whose children are (or are about to be) the members of
    /// `dimension`.
    ///
    /// Selecting the R channel and asking to double means "another
    /// channel beside this one", so the container is the parent; selecting
    /// the part itself means the same thing, and the container is the
    /// selection. Both land on the same track.
    fn container_for(&self, dimension: TrackDimension) -> DawResult<Track> {
        let selected = self.selected_scope()?;
        let tree = self.track_tree();
        let scope = Self::group_scope(&tree, &selected.guid);
        if let Some(parent) = tree.parent_of(&selected)
            && Self::dimension_of(&selected.name, &scope) == dimension
        {
            return Ok(parent.clone());
        }
        Ok(selected)
    }

    /// The name the template classifies a track's names *against*: the
    /// outermost track of the chain it hangs from.
    ///
    /// It has to be that one, not the immediate parent. The classifier
    /// reads a name in context by parsing the two together, so a mic
    /// under an "L" folder would be read as "L DI" and come back as a
    /// Channel — the parent's own dimension value shadowing the child's.
    /// The outermost name is the one that says which instrument's
    /// vocabulary applies ("GTR E ...", "GTR A ...") without carrying a
    /// dimension value of its own, which is exactly the context the
    /// classifier wants.
    ///
    /// Returned as the one-element slice the classifier takes, built once
    /// per walk rather than per name.
    fn group_scope(tree: &TrackTree, guid: &str) -> [String; 1] {
        let Some(mut node) = tree.get(guid).cloned() else {
            return [String::new()];
        };
        while let Some(parent) = tree.parent_of(&node) {
            node = parent.clone();
        }
        [node.name]
    }

    /// Every channel under `guid`, or `guid` itself when the part has no
    /// Channel level yet — a part that has never been doubled *is* one
    /// channel, which is the whole point of not spending a folder on a
    /// level with one member.
    fn channels_of(&self, guid: &str) -> Vec<Track> {
        let tree = self.track_tree();
        let scope = Self::group_scope(&tree, guid);
        let mut found = Vec::new();
        Self::collect_dimension(&tree, guid, &scope, TrackDimension::Channel, &mut found);
        if found.is_empty() {
            return tree.get(guid).cloned().into_iter().collect();
        }
        found
    }

    /// Depth-first walk collecting every descendant of `guid` that reads
    /// as `dimension`, without descending into one that already matched.
    fn collect_dimension(
        tree: &TrackTree,
        guid: &str,
        scope: &[String; 1],
        dimension: TrackDimension,
        found: &mut Vec<Track>,
    ) {
        let children: Vec<Track> = tree.children_of(guid).cloned().collect();
        for child in children {
            if Self::dimension_of(&child.name, scope) == dimension {
                found.push(child);
            } else {
                Self::collect_dimension(tree, &child.guid, scope, dimension, found);
            }
        }
    }
}

// ── growing one level ───────────────────────────────────────────────

impl<D: Tracks + Items + Projects> GuitarGrow<D> {
    /// Grow `container`'s `dimension` level by the next name the template
    /// offers.
    fn grow_level(&self, container_guid: &str, dimension: TrackDimension) -> DawResult<()> {
        let container = self.get_track(container_guid)?;
        let name = self.next_member_name(&container, dimension)?;
        self.grow_level_to(container_guid, dimension, &name)
    }

    /// Grow `container`'s `dimension` level by the member called `name`,
    /// inserting the level first when it does not exist yet.
    ///
    /// A no-op when the member is already there, so "add a source to
    /// every channel" stays idempotent on a part whose sides are uneven.
    fn grow_level_to(
        &self,
        container_guid: &str,
        dimension: TrackDimension,
        name: &str,
    ) -> DawResult<()> {
        let members = self.members_of(container_guid, dimension);
        if members.iter().any(|m| eq_name(&m.name, name)) {
            return Ok(());
        }
        if members.is_empty() {
            // The level gains its second member here and only here, so
            // this is the one moment the folder is allowed to appear.
            self.insert_level(container_guid, dimension)?;
        }
        let first = self
            .members_of(container_guid, dimension)
            .into_iter()
            .next()
            .ok_or_else(|| {
                DawError::not_found(&format!("a {dimension} to grow"), container_guid)
            })?;

        let children = Self::new_member_children(&self.track_tree(), &first, dimension, name);
        self.append_shape(
            container_guid,
            std::slice::from_ref(&TrackShape::with_children(name, children)),
        )
    }

    /// The subtree a newly added member arrives with.
    ///
    /// A new channel, layer or arrangement mirrors the one beside it —
    /// the second take of a part is the same rig as the first. A new
    /// *source* is not a mirror of its neighbour: an amp arrives as the
    /// folder over its own 57 and 121, and a DI or a pedalboard arrives
    /// bare.
    fn new_member_children(
        tree: &TrackTree,
        first: &Track,
        dimension: TrackDimension,
        name: &str,
    ) -> Vec<TrackShape> {
        if dimension == TrackDimension::MultiMic {
            return source_defaults::default_mics(name)
                .into_iter()
                .map(TrackShape::leaf)
                .collect();
        }
        tree.shape_of_children(&first.guid)
    }

    /// Turn `container`'s single implicit member of `dimension` into a
    /// real one, so the level can hold a second.
    ///
    /// Two shapes of container. One with children — a part holding its
    /// channels, about to hold them under a Main layer — where the new
    /// folder wraps them in place. One without — a track carrying items,
    /// which is what a level with one member looks like — where the new
    /// member takes the items and the container becomes its folder.
    fn insert_level(&self, container_guid: &str, dimension: TrackDimension) -> DawResult<()> {
        let tree = self.track_tree();
        let container = tree
            .get(container_guid)
            .ok_or_else(|| DawError::invalid_object("track", container_guid))?
            .clone();
        let first = self.first_member_name(&container, dimension)?;

        // The container's own name may be carrying the value — a part
        // track called "GTR E Rhythm" *is* the Rhythm arrangement. Hand
        // that word down to the folder that now holds it, so the
        // classifier does not read the same arrangement twice.
        if let Some(carried) = track_schema::dimension_value(&container.name, &[], dimension)
            && eq_name(&carried, &first)
        {
            let stripped = strip_token(&container.name, &carried);
            if !stripped.is_empty() && !eq_name(&stripped, &container.name) {
                self.rename(
                    daw::service::ProjectContext::Current,
                    TrackRef::Guid(container.guid.clone()),
                    &stripped,
                )?;
            }
        }

        let children: Vec<Track> = tree.children_of(container_guid).cloned().collect();
        match children.first() {
            Some(head) => {
                // Wrap in place. The track that currently terminates the
                // container's subtree has to close one level more, and
                // the index is read from the pre-insert snapshot.
                let terminator = tree
                    .subtree_end_index(container_guid)
                    .and_then(|end| tree.at_index(end.saturating_sub(1)))
                    .cloned();
                let folder = self.insert_track_at(&first, head.index)?;
                self.set_depth(&folder, 1)?;
                if let Some(terminator) = terminator {
                    self.set_depth(&terminator.guid, terminator.folder_depth.saturating_sub(1))?;
                }
            }
            None => {
                // A leaf carrying items. It becomes the folder; the new
                // member takes the items and closes whatever the leaf
                // used to close, plus itself.
                let closing = container.folder_depth.min(0).saturating_sub(1);
                self.set_depth(container_guid, 1)?;
                let member = self.insert_track_at(&first, container.index.saturating_add(1))?;
                self.set_depth(&member, closing)?;
                self.move_items(container_guid, &member)?;
            }
        }
        Ok(())
    }
}

// ── the template's vocabulary ───────────────────────────────────────

impl<D: Tracks + Items + Projects> GuitarGrow<D> {
    /// The children of `container_guid` that read as `dimension`.
    fn members_of(&self, container_guid: &str, dimension: TrackDimension) -> Vec<Track> {
        let tree = self.track_tree();
        let scope = Self::group_scope(&tree, container_guid);
        tree.children_of(container_guid)
            .filter(|child| Self::dimension_of(&child.name, &scope) == dimension)
            .cloned()
            .collect()
    }

    /// What the level's first member is called once it exists: the value
    /// the container's own name already carries, or the first the
    /// template offers (L for a channel, Main for a layer, DI for a
    /// source).
    fn first_member_name(&self, container: &Track, dimension: TrackDimension) -> DawResult<String> {
        if let Some(carried) = track_schema::dimension_value(&container.name, &[], dimension) {
            return Ok(carried);
        }
        let scope = Self::group_scope(&self.track_tree(), &container.guid);
        track_schema::next_growth_value(dimension, &scope, Vec::<String>::new()).ok_or_else(|| {
            DawError::not_found(&format!("a configured {dimension} name"), &container.name)
        })
    }

    /// The next value of `dimension` this container does not already
    /// carry.
    fn next_member_name(&self, container: &Track, dimension: TrackDimension) -> DawResult<String> {
        let mut taken: Vec<String> = self
            .members_of(&container.guid, dimension)
            .into_iter()
            .map(|m| m.name)
            .collect();
        if taken.is_empty()
            && let Ok(first) = self.first_member_name(container, dimension)
        {
            // The level is still implicit: its one member is whatever the
            // container itself stands for, and that name is taken.
            taken.push(first);
        }
        let scope = Self::group_scope(&self.track_tree(), &container.guid);
        track_schema::next_growth_value(dimension, &scope, taken.iter().map(String::as_str))
            .ok_or_else(|| {
                DawError::not_found(&format!("a configured {dimension} name"), &container.name)
            })
    }

    /// Which dimension `name` reads as, in the context of the scope it
    /// sits under.
    fn dimension_of(name: &str, scope: &[String; 1]) -> TrackDimension {
        track_schema::classify_track_dimension(name, scope)
    }
}

// ── source defaults ─────────────────────────────────────────────────

impl<D: Tracks + Items + Projects> GuitarGrow<D> {
    /// Put every channel under `guid` at the balance its configuration
    /// starts at (`flow.guitars.mixing.source-defaults`).
    ///
    /// Applied at creation and never re-applied to anything an engineer
    /// has already balanced: a track still centred and unmuted has never
    /// been set, and only those are written. That is the whole mechanism
    /// — there is no "was this defaulted" flag anywhere, and a source a
    /// user has moved is left exactly where they put it.
    fn apply_source_defaults(&self, guid: &str) -> DawResult<()> {
        for channel in self.channels_of(guid) {
            self.apply_channel_defaults(&channel)?;
        }
        Ok(())
    }

    fn apply_channel_defaults(&self, channel: &Track) -> DawResult<()> {
        let sources = self.members_of(&channel.guid, TrackDimension::MultiMic);
        if sources.is_empty() {
            return Ok(());
        }

        let tree = self.track_tree();
        let described: Vec<Source> = sources
            .iter()
            .map(|source| {
                Source::with_mics(
                    source.name.clone(),
                    self.members_of(&source.guid, TrackDimension::MultiMic)
                        .into_iter()
                        .map(|mic| mic.name)
                        .collect::<Vec<_>>(),
                )
            })
            .collect();

        for default in source_defaults::channel_defaults(&described) {
            let Some(track) = Self::resolve_path(&tree, &channel.guid, &default.path) else {
                continue;
            };
            if !Balance::of(track.pan, track.muted).is_untouched() {
                continue;
            }
            let reference = TrackRef::Guid(track.guid.clone());
            self.set_pan(
                daw::service::ProjectContext::Current,
                reference.clone(),
                default.balance.pan,
            )?;
            Tracks::set_muted(
                &self.daw,
                daw::service::ProjectContext::Current,
                reference,
                default.balance.muted,
            )?;
        }
        Ok(())
    }

    /// Walk `path` (source, then mic) down from a channel.
    fn resolve_path(tree: &TrackTree, channel_guid: &str, path: &[String]) -> Option<Track> {
        let mut current = channel_guid.to_string();
        let mut found = None;
        for step in path {
            let next = tree
                .children_of(&current)
                .find(|child| eq_name(&child.name, step))?
                .clone();
            current.clone_from(&next.guid);
            found = Some(next);
        }
        found
    }
}

/// Names from the template compare case- and space-insensitively — an
/// engineer's "amp 1" is the template's "Amp 1".
fn eq_name(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

/// Remove `token` from `name`, tidying the separator it leaves behind.
///
/// "GTR E Rhythm" minus "Rhythm" is "GTR E", which is what the part's
/// folder is called once Rhythm is one arrangement inside it.
fn strip_token(name: &str, token: &str) -> String {
    let kept: Vec<&str> = name
        .split_whitespace()
        .filter(|word| {
            !word
                .trim_matches(['-', '_'])
                .eq_ignore_ascii_case(token.trim())
        })
        .collect();
    kept.join(" ").trim_matches(['-', '_', ' ']).to_string()
}

#[cfg(test)]
mod tests {
    use super::{eq_name, strip_token};

    #[test]
    fn stripping_an_arrangement_leaves_the_part_name() {
        assert_eq!(strip_token("GTR E Rhythm", "Rhythm"), "GTR E");
        assert_eq!(strip_token("GTR A Strum", "Strum"), "GTR A");
        // Nothing to strip leaves the name alone.
        assert_eq!(strip_token("GTR E", "Rhythm"), "GTR E");
    }

    #[test]
    fn template_names_compare_loosely() {
        assert!(eq_name("Amp 1", "amp 1"));
        assert!(!eq_name("Amp 1", "Amp 2"));
    }
}
