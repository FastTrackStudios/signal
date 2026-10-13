//! Applying a session template to a project — one driver, two backends.
//!
//! [`buses`](crate::buses) decides *what* the bus tree is; this module puts it
//! into a project. The same driver runs over an `.RPP` on disk and over a live
//! REAPER session, because both are reached through one trait,
//! [`TemplateTarget`]:
//!
//! | backend | target | when |
//! |---|---|---|
//! | [`dawfile`] | `dawfile_reaper::types::Project` | offline, batch — organize a folder of projects without opening REAPER |
//! | [`chunk`] | the raw `.RPP` chunk tree | offline — the `--apply-buses` CLI; edits leave untouched lines byte-identical |
//! | [`live`] | any `daw::service` backend: REAPER, or `daw-standalone` | live — an extension action, or the Session window |
//!
//! The trait is the *primitive* surface — create a track, nest it, color it,
//! send it somewhere. Everything template-shaped lives in [`apply_buses`],
//! written once against the trait, so the two backends cannot drift.
//!
//! # Idempotence
//!
//! [`apply_buses`] looks a bus up by name before creating it
//! ([`TemplateTarget::find_track`]) and adds a send only when one is not
//! already there. Running it twice over the same project is a no-op, which is
//! what makes it safe to re-run over an album's worth of sessions as the
//! taxonomy changes.

use std::collections::HashMap;

use daw_proto::FolderDepthChange;
use dynamic_template_proto::{TemplateBus, TemplateNode};

use crate::buses::bus_nodes;

pub mod chunk;
pub mod dawfile;
pub mod live;
mod organize;

pub use live::DawTarget;
#[cfg(feature = "reaper")]
pub use live::ReaperTarget;
pub use organize::{organize, Organized};

/// A project a session template can be materialized into.
///
/// Implementors provide track primitives only; the template logic is
/// [`apply_buses`]. Ids are opaque and backend-chosen — a track index for the
/// file backend, a GUID for the live one.
pub trait TemplateTarget {
    /// How this backend names a track it has created.
    type TrackId: Clone;
    /// Failure from the underlying project.
    type Error;

    /// The id of an existing track with exactly this name, if there is one.
    /// Drives idempotence, so it must see tracks created earlier in this same
    /// run, not just ones that were already in the project.
    /// Matching is by name and should be case-insensitive: a project with a
    /// `Keys Bus` must not gain a second `KEYS BUS` beside it.
    fn find_track(&self, name: &str) -> Option<Self::TrackId>;

    /// Append a track at the end of the project.
    ///
    /// # Errors
    ///
    /// Returns [`Self::Error`] if the backend operation fails.
    fn append_track(&mut self, name: &str) -> Result<Self::TrackId, Self::Error>;

    /// Set the track's folder nesting — whether it opens a folder, closes
    /// some, or neither.
    ///
    /// # Errors
    ///
    /// Returns [`Self::Error`] if the backend operation fails.
    fn set_folder_depth(
        &mut self,
        track: &Self::TrackId,
        depth: FolderDepthChange,
    ) -> Result<(), Self::Error>;

    /// Set the track color from an `#RRGGBB` string. A backend that cannot
    /// parse the string should leave the color alone rather than fail.
    ///
    /// # Errors
    ///
    /// Returns [`Self::Error`] if the backend operation fails.
    fn set_color(&mut self, track: &Self::TrackId, hex: &str) -> Result<(), Self::Error>;

    /// Set the track's channel count.
    ///
    /// # Errors
    ///
    /// Returns [`Self::Error`] if the backend operation fails.
    fn set_channel_count(
        &mut self,
        track: &Self::TrackId,
        channels: u32,
    ) -> Result<(), Self::Error>;

    /// Whether `source` already sends to `dest`.
    fn has_send(&self, source: &Self::TrackId, dest: &Self::TrackId) -> bool;

    /// Add a post-fader send from `source` to `dest`.
    ///
    /// # Errors
    ///
    /// Returns [`Self::Error`] if the backend operation fails.
    fn add_send(&mut self, source: &Self::TrackId, dest: &Self::TrackId)
        -> Result<(), Self::Error>;

    /// Enable or disable the send to the parent folder (REAPER's `MAINSEND`).
    ///
    /// # Errors
    ///
    /// Returns [`Self::Error`] if the backend operation fails.
    fn set_parent_send(&mut self, track: &Self::TrackId, enabled: bool) -> Result<(), Self::Error>;

    /// Every track's id, name, and folder-depth change, in track order.
    ///
    /// The read side of [`set_folder_depth`](TemplateTarget::set_folder_depth),
    /// used by [`normalize_folder_depths`] to see the structure before fixing
    /// it. The name is carried for reporting.
    fn folder_depths(&self) -> Vec<(Self::TrackId, String, i32)>;

    /// Move `tracks` to the end of the project inside a new folder named
    /// `folder`, returning the folder's id — or `None` if nothing was moved.
    ///
    /// Implementations must skip any track that carries folder structure (a
    /// folder parent, or the track that closes one). Pulling such a track out
    /// leaves the tracks around it inside a folder that never closes, which
    /// silently swallows the rest of the project. See
    /// [`gather_unsorted`] for the caller's side of that contract.
    ///
    /// **Invalidates previously issued ids** on any backend that keys tracks by
    /// position, so call it last.
    ///
    /// # Errors
    ///
    /// Returns [`Self::Error`] if the backend operation fails.
    fn gather_into_folder(
        &mut self,
        folder: &str,
        tracks: &[Self::TrackId],
    ) -> Result<Option<Gathered<Self::TrackId>>, Self::Error>;

    /// Nest a "DI" capture under its sibling primary track, for any group
    /// that opts in via [`monarchy::Group::nest_secondary_mics`]. Changes
    /// folder nesting and mute only — no track moves. Idempotent. Backends
    /// that cannot do it leave the tracks as they are.
    fn nest_secondary_mics(&mut self) {}
}

/// Find a group by name anywhere in the config's tree (recursing into nested
/// groups).
pub(crate) fn find_group<'a>(
    config: &'a crate::DynamicTemplateConfig,
    name: &str,
) -> Option<&'a monarchy::Group<crate::ItemMetadata>> {
    fn search<'a>(
        group: &'a monarchy::Group<crate::ItemMetadata>,
        name: &str,
    ) -> Option<&'a monarchy::Group<crate::ItemMetadata>> {
        if group.name == name {
            return Some(group);
        }
        group.groups.iter().find_map(|g| search(g, name))
    }
    config.groups.iter().find_map(|g| search(g, name))
}

/// What [`apply_buses`] created: every bus by name, so callers can wire
/// instrument folders into them afterwards.
#[derive(Debug, Clone)]
pub struct AppliedBuses<Id> {
    /// Bus name → the track carrying it.
    pub by_name: HashMap<String, Id>,
    /// Names of buses that already existed and were left alone.
    pub reused: Vec<String>,
    /// Names of buses newly created by this run.
    pub created: Vec<String>,
    /// Whether the buses were created as a nested folder tree (a fresh
    /// project) or appended flat and wired with sends (an existing one).
    pub nested: bool,
}

impl<Id> Default for AppliedBuses<Id> {
    fn default() -> Self {
        Self {
            by_name: HashMap::new(),
            reused: Vec::new(),
            created: Vec::new(),
            nested: true,
        }
    }
}

impl<Id: Clone> AppliedBuses<Id> {
    /// The track carrying `bus`, if it was created or found.
    #[must_use]
    pub fn get(&self, bus: &str) -> Option<&Id> {
        self.by_name.get(bus)
    }
}

/// One bus flattened into the linear track list a DAW actually stores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatBusTrack {
    /// Bus name.
    pub name: String,
    /// Nesting depth, 0 at the top of the bus tree.
    pub depth: usize,
    /// Folder nesting for this row.
    pub folder_depth: FolderDepthChange,
    /// Color as `#RRGGBB`, if the bus has one.
    pub color_hex: Option<String>,
    /// Channel count.
    pub channels: u32,
}

/// Flatten the nested bus tracks into the linear, folder-depth-encoded list a
/// DAW stores.
///
/// REAPER has no nested track list: nesting is carried by a per-track depth
/// change, where a folder parent is `+1` and the **last** track inside one or
/// more folders closes all of them at once with a single negative value. So a
/// leaf's folder depth depends on what follows it, which is why this is one
/// pass over the flattened rows rather than something the recursion can decide
/// on its own.
#[must_use]
pub fn flatten_buses(nodes: &[TemplateNode]) -> Vec<FlatBusTrack> {
    fn walk(node: &TemplateNode, depth: usize, out: &mut Vec<FlatBusTrack>) {
        out.push(FlatBusTrack {
            name: node.name.clone(),
            depth,
            // Provisional — fixed up below once the next row is known.
            folder_depth: if node.children.is_empty() {
                FolderDepthChange::Normal
            } else {
                FolderDepthChange::FolderStart
            },
            color_hex: node.defaults.color_hex.clone(),
            channels: 2,
        });
        for child in &node.children {
            walk(child, depth.saturating_add(1), out);
        }
    }

    let mut rows = Vec::new();
    for node in nodes {
        walk(node, 0, &mut rows);
    }

    // A leaf closes every folder that ends on it: the drop from its own depth
    // to the next row's (or to 0 at the end of the list).
    for i in 0..rows.len() {
        if let Some(row) = rows.get(i) {
            if row.folder_depth == FolderDepthChange::FolderStart {
                continue;
            }
            let next_depth = rows.get(i.saturating_add(1)).map_or(0, |r| r.depth);
            let row_depth = i32::try_from(row.depth).unwrap_or(i32::MAX);
            let next_depth = i32::try_from(next_depth).unwrap_or(i32::MAX);
            let closes = row_depth.saturating_sub(next_depth);
            if closes > 0 {
                let closes_i8 = i8::try_from(closes).unwrap_or(i8::MAX);
                if let Some(slot) = rows.get_mut(i) {
                    slot.folder_depth = FolderDepthChange::ClosesLevels(closes_i8.saturating_neg());
                }
            }
        }
    }
    rows
}

/// Create `buses` in `target` and return them by name.
///
/// Buses the project already has — under their canonical name or any
/// [`alias`](crate::buses::BusSpec::aliases) — are reused untouched, so this is
/// safe to re-run. Color and channel count are set on newly created buses only;
/// re-running never overwrites something changed by hand.
///
/// # Two placements, because a project's own tracks come first
///
/// - **Nothing reused** (a fresh project): the buses are created as the nested
///   folder tracks of [`flatten_buses`] — `DRUM BUS` inside `INST BUS` inside
///   `MIX BUS`, reaching its parent by the folder send.
/// - **Something reused** (an existing session): the missing buses are appended
///   flat and wired to their parent bus with an explicit send instead.
///
/// The second case exists because a bus can only be nested by *position*, and
/// the buses a session already has are scattered through a track list this
/// function must not reorder. Emitting the nested depths anyway is what
/// produced an unbalanced project — folder depths that did not sum to zero,
/// which makes REAPER swallow every following track into a folder that never
/// closes. Signal flow is identical either way; only the track-list shape
/// differs, and [`AppliedBuses::nested`] reports which happened.
///
/// # Errors
///
/// Returns [`T::Error`] if any backend operation fails.
pub fn apply_buses<T: TemplateTarget>(
    target: &mut T,
    buses: &[TemplateBus],
) -> Result<AppliedBuses<T::TrackId>, T::Error> {
    let rows = flatten_buses(&bus_nodes(buses));

    // Resolve what the project already has before creating anything, so the
    // placement decision is made once for the whole tree rather than drifting
    // partway through it.
    let found: Vec<Option<T::TrackId>> = rows
        .iter()
        .map(|row| {
            crate::buses::spec(&row.name)
                .map_or_else(
                    || vec![row.name.as_str()],
                    |s| s.known_names().collect::<Vec<_>>(),
                )
                .into_iter()
                .find_map(|candidate| target.find_track(candidate))
        })
        .collect();

    let nested = found.iter().all(Option::is_none);
    let mut applied = AppliedBuses {
        nested,
        ..AppliedBuses::default()
    };

    for (row, existing) in rows.iter().zip(found) {
        if let Some(existing) = existing {
            applied.by_name.insert(row.name.clone(), existing);
            applied.reused.push(row.name.clone());
            continue;
        }

        let id = target.append_track(&row.name)?;
        // Flat when adopting an existing project — see the note above.
        let depth = if nested {
            row.folder_depth
        } else {
            FolderDepthChange::Normal
        };
        target.set_folder_depth(&id, depth)?;
        target.set_channel_count(&id, row.channels)?;
        if let Some(hex) = &row.color_hex {
            target.set_color(&id, hex)?;
        }
        applied.by_name.insert(row.name.clone(), id);
        applied.created.push(row.name.clone());
    }

    if !nested {
        // Without the folder nesting to carry it, each newly created bus needs
        // an explicit send to the bus it feeds. A bus with no parent feeds the
        // master, which is the default, so it is left alone.
        for name in applied.created.clone() {
            let Some(parent) = crate::buses::spec(&name).and_then(|s| s.parent) else {
                continue;
            };
            let (Some(source), Some(dest)) = (
                applied.by_name.get(&name).cloned(),
                applied.by_name.get(parent).cloned(),
            ) else {
                continue;
            };
            route_to_bus(target, &source, &dest)?;
        }
    }

    Ok(applied)
}

/// Wire a track into a bus: a post-fader send, with the parent send dropped.
///
/// This is the instrument-folder side of the routing — the bus tree itself
/// nests, so a bus reaches its parent by the folder send and never comes
/// through here. Mirrors
/// [`NodeRouting::to_bus`](dynamic_template_proto::NodeRouting::to_bus): the
/// attachment point feeds its bus instead of the folder above it.
///
/// Does nothing if the send is already there.
///
/// # Errors
///
/// Returns [`T::Error`] if any backend operation fails.
pub fn route_to_bus<T: TemplateTarget>(
    target: &mut T,
    track: &T::TrackId,
    bus: &T::TrackId,
) -> Result<(), T::Error> {
    if !target.has_send(track, bus) {
        target.add_send(track, bus)?;
    }
    target.set_parent_send(track, false)
}

/// One track's folder depth, corrected.
#[derive(Debug, Clone)]
pub struct FolderFix<Id> {
    /// The track that was wrong.
    pub track: Id,
    /// Its name, for reporting.
    pub name: String,
    /// The depth change it carried.
    pub from: i32,
    /// The depth change it now carries.
    pub to: i32,
}

/// Rewrite folder depths so they describe an actual tree.
///
/// REAPER stores nesting as a per-track depth *change*, and nothing enforces
/// that the changes are consistent: a track can close a folder that was never
/// opened, or close three when two are open. REAPER clamps the result at render
/// time, so the project opens and looks fine while the stored structure is not
/// a tree — and anything reasoning about folder membership is working from
/// nonsense.
///
/// The repair is deliberately minimal, because these are sessions someone has
/// mixed:
///
/// - **Every folder opening is kept.** Openings are unambiguous intent.
/// - **An over-close is clamped to what is actually open.** A track closing
///   three levels with two open closes two; a track closing one with none open
///   closes none and becomes an ordinary track.
/// - **Folders still open at the end are closed on the last track**, which is
///   the only place they can be closed without moving anything.
///
/// Nothing is reordered and no track is added or removed, so the only thing
/// that changes is folder membership that was already undefined. Returns one
/// [`FolderFix`] per track actually changed.
///
/// # Panics
///
/// Panics if `rows` is non-empty but modifying the last element fails, which
/// should not happen. This is a defensive panic on a precondition checked at entry.
///
/// # Errors
///
/// Returns [`T::Error`] if any backend operation fails.
pub fn normalize_folder_depths<T: TemplateTarget>(
    target: &mut T,
) -> Result<Vec<FolderFix<T::TrackId>>, T::Error> {
    let rows = target.folder_depths();
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let mut corrected: Vec<i32> = Vec::with_capacity(rows.len());
    let mut depth = 0i32;
    let mut depth_before_last = 0i32;
    for (i, (_, _, change)) in rows.iter().enumerate() {
        let is_last = i.saturating_add(1) == rows.len();
        if is_last {
            depth_before_last = depth;
        }
        // Openings always stand; a close can only close what is open.
        let fixed = if *change < 0 && depth.saturating_add(*change) < 0 {
            depth.saturating_neg()
        } else {
            *change
        };
        depth = depth.saturating_add(fixed);
        corrected.push(fixed);
    }

    // Close whatever is still open on the last track. `-depth_before_last`
    // closes exactly what was open going into it, so the running depth lands on
    // zero and never dips below it.
    if depth != 0 {
        if let Some(last) = corrected.last_mut() {
            *last = depth_before_last.saturating_neg();
        }
    }

    let mut fixes = Vec::new();
    for ((id, name, before), after) in rows.into_iter().zip(corrected) {
        if before == after {
            continue;
        }
        target.set_folder_depth(&id, FolderDepthChange::from_raw_value(after))?;
        fixes.push(FolderFix {
            track: id,
            name,
            from: before,
            to: after,
        });
    }
    Ok(fixes)
}

/// What a [`TemplateTarget::gather_into_folder`] call actually did.
#[derive(Debug, Clone)]
pub struct Gathered<Id> {
    /// The folder that was created.
    pub folder: Id,
    /// Tracks moved into it.
    pub moved: Vec<Id>,
    /// Tracks left where they were because moving them would have broken the
    /// project's folder structure — see [`TemplateTarget::gather_into_folder`].
    pub skipped: Vec<Id>,
}

/// Each track with the folder path it sits under and the group path it
/// classifies into, read from the project's folder nesting.
///
/// A track deep in a session often carries a name that means nothing alone —
/// `In`, `Top`, `DI`, `Amp 1`. That is not a naming failure to be corrected but
/// the house style: `NAMING_GUIDELINES.md` has a track prefixed with its
/// parent's name only when it would otherwise be bare, so a mic position inside
/// a `Kick` folder is just `In`. Classifying such a name on its own yields
/// nothing; classifying `"Kick In"` yields the kick.
///
/// So each name is classified against its ancestor folders, exactly as
/// [`track_schema::classify_track_dimension`](crate::track_schema::classify_track_dimension)
/// does with its `context` argument. Tracks that still classify to nothing are
/// genuinely unplaced.
pub fn contextual_paths<T: TemplateTarget>(target: &T) -> Vec<TrackContext<T::TrackId>> {
    let rows = target.folder_depths();
    let mut ancestors: Vec<String> = Vec::new();
    let mut out = Vec::with_capacity(rows.len());

    for (id, name, change) in rows {
        // A folder's own name is context for what is inside it, not for itself.
        let context = ancestors.clone();
        let contextual = if context.is_empty() {
            name.clone()
        } else {
            format!("{} {name}", context.join(" "))
        };
        let path = crate::track_schema::classify_track(&contextual).matched_groups;

        if change > 0 {
            ancestors.push(name.clone());
        } else if change < 0 {
            // This track closes `-change` folders, so it is the last member of
            // each; pop them now that its own context is recorded.
            // change is i32 from template data (negative in this branch),
            // negation is safe and the result is always non-negative.
            #[allow(
                clippy::arithmetic_side_effects,
                clippy::cast_sign_loss,
                clippy::as_conversions
            )]
            let closes = -change as usize;
            for _ in 0..closes {
                ancestors.pop();
            }
        }

        out.push(TrackContext {
            track: id,
            name,
            context,
            path,
        });
    }
    out
}

/// Reclassify a cohesive set of stem-split outputs as `Reference/Stem Split`.
///
/// A single name cannot be recognised as a stem: `..._Piano` on its own is a
/// piano, and a live-tracked `Drums` is drums. Only the *set* gives it away —
/// three or more of the standard separator categories (drums / bass / vocals /
/// other / piano / …) sharing a source, which is what
/// [`is_stem_split_set`](crate::is_stem_split_set) tests for.
///
/// Left alone, a demucs separation of the finished record classifies as
/// content and sums into the mix beside the real tracks, doubling everything.
///
/// Entries are grouped by parent folder **and apparent source** before testing,
/// and only the members that actually carry a stem category are reclassified.
/// Both matter: grouping by folder alone puts every top-level track in one
/// bucket, and reclassifying a whole qualifying bucket sweeps in whatever
/// merely sat beside the stems. Returns the reclassified list.
#[must_use]
pub fn reclassify_stem_splits<Id>(entries: Vec<TrackContext<Id>>) -> Vec<TrackContext<Id>> {
    use std::collections::HashMap;

    // Group by (folder, apparent source): stems from one separation share both.
    // Grouping by folder alone is not enough — at the top level every track
    // shares the empty folder, so a handful of real stems would drag the whole
    // project in with them.
    let mut sets: HashMap<(Vec<String>, String), Vec<usize>> = HashMap::new();
    for (i, entry) in entries.iter().enumerate() {
        // A stem is a track of audio, never a folder: a session's own
        // `Drums`, `Bass` and `Vocals` folders side by side are its groups,
        // not a separation of the record.
        let folder = entries
            .get(i.saturating_add(1))
            .is_some_and(|next| next.context.len() > entry.context.len());
        if folder {
            continue;
        }
        let Some(source) = crate::stem_source(&entry.name) else {
            continue;
        };
        sets.entry((entry.context.clone(), source))
            .or_default()
            .push(i);
    }

    let stem_path = vec!["Reference".to_string(), "Stem Split".to_string()];
    let mut is_stem: Vec<bool> = vec![false; entries.len()];
    for indices in sets.values() {
        let names: Vec<String> = indices
            .iter()
            .filter_map(|i| entries.get(*i).map(|e| e.name.clone()))
            .collect();
        if crate::is_stem_split_set(&names) {
            for i in indices {
                if let Some(stem_flag) = is_stem.get_mut(*i) {
                    *stem_flag = true;
                }
            }
        }
    }

    entries
        .into_iter()
        .zip(is_stem)
        .map(|(mut entry, stem)| {
            if stem {
                entry.path.clone_from(&stem_path);
            }
            entry
        })
        .collect()
}

/// Every track's group path, the way the organizer files it.
///
/// Classified against its folders ([`contextual_paths`]), then the sets
/// only a set gives away — a stem split ([`reclassify_stem_splits`]), a
/// rehearsal and the song's own bounces ([`reclassify_rehearsal`]) — and
/// last the folders their names say nothing about ([`folders_by_contents`]).
pub fn classified_paths<T: TemplateTarget>(target: &T) -> Vec<TrackContext<T::TrackId>> {
    folders_by_contents(reclassify_rehearsal(reclassify_stem_splits(
        contextual_paths(target),
    )))
}

/// A folder its name says nothing about, filed by what it holds.
///
/// Unnamed, or named nothing the groups know: it takes the group path every
/// classified track inside it shares, if they share one. A nameless folder
/// of acoustic guitars is the acoustics' folder, and routes as they do.
#[must_use]
pub fn folders_by_contents<Id>(mut entries: Vec<TrackContext<Id>>) -> Vec<TrackContext<Id>> {
    let count = entries.len();
    // Innermost first, so a folder of such folders follows.
    for i in (0..count).rev() {
        let Some(depth) = entries.get(i).map(|e| e.context.len()) else {
            continue;
        };
        let after = i.saturating_add(1);
        let end = (after..count)
            .find(|j| entries.get(*j).is_some_and(|e| e.context.len() <= depth))
            .unwrap_or(count);
        if after >= end || entries.get(i).is_some_and(|e| !e.path.is_empty()) {
            continue;
        }
        let mut shared: Option<Vec<String>> = None;
        for inner in entries.get(after..end).unwrap_or_default() {
            if inner.path.is_empty() {
                continue;
            }
            shared = Some(shared.map_or_else(
                || inner.path.clone(),
                |so_far| {
                    so_far
                        .iter()
                        .zip(&inner.path)
                        .take_while(|(a, b)| a == b)
                        .map(|(a, _)| a.clone())
                        .collect()
                },
            ));
        }
        if let (Some(path), Some(entry)) = (shared.filter(|p| !p.is_empty()), entries.get_mut(i)) {
            entry.path = path;
        }
    }
    entries
}

/// Where a rehearsal's tracks are filed: reference material, kept out of
/// the mix, together.
pub const REHEARSAL_PATH: [&str; 2] = ["Reference", "Rehearsal"];

/// Reclassify a rehearsal recording, and the song's own bounces, as
/// reference material.
///
/// A band's rehearsal arrives as a stem set named for the song — `Bass -
/// Unbreakable`, `Lead - Unbreakable`, a `Drums` folder of `Kick -
/// Unbreakable`… — usually under a generic container (`Tracks`, `Track`, or
/// the song's own name). Named that way they classify as the parts they
/// are, or as nothing, and sum into the mix beside the tracks actually being
/// produced. They are references to the arrangement, not parts of it: they go
/// to [`REHEARSAL_PATH`]. So does a container's whole contents (the click
/// excepted — it is the session's click), and a folder holding nothing else.
///
/// The song is the suffix the set shares: `Part - Song` on at least two
/// tracks with different parts. A track named the song itself, or the
/// song's stems (`<song>_stems`, `… - stem`), is a bounce of it: `Reference`.
#[must_use]
pub fn reclassify_rehearsal<Id>(mut entries: Vec<TrackContext<Id>>) -> Vec<TrackContext<Id>> {
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    let song = rehearsal_song(&names);
    let is_song = |name: &str| song.as_deref().is_some_and(|s| loose(name) == s);
    let of_song = |name: &str| {
        song.as_deref()
            .is_some_and(|s| part_and_song(name).is_some_and(|(_, of)| of == s))
    };
    let count = entries.len();
    let depth = |i: usize| entries.get(i).map_or(0, |e| e.context.len());
    // A folder: the next track sits deeper. Its contents: everything after
    // it that does.
    let is_folder = |i: usize| {
        i.checked_add(1)
            .is_some_and(|next| next < count && depth(next) > depth(i))
    };
    let inside = |i: usize| {
        let d = depth(i);
        let after = i.saturating_add(1);
        after..(after..count).find(|j| depth(*j) <= d).unwrap_or(count)
    };
    let guide = |e: &TrackContext<Id>| {
        e.path
            .first()
            .is_some_and(|g| g.eq_ignore_ascii_case("Guide"))
    };

    let mut rehearsal = vec![false; count];
    // The containers, and all they hold.
    for i in 0..count {
        let Some(entry) = entries.get(i) else {
            continue;
        };
        let generic = matches!(loose(&entry.name).as_str(), "tracks" | "track");
        if is_folder(i) && entry.context.is_empty() && (generic || is_song(&entry.name)) {
            for j in std::iter::once(i).chain(inside(i)) {
                if let Some(flag) = rehearsal.get_mut(j) {
                    *flag = true;
                }
            }
        }
    }
    // The set's tracks wherever they are — and, beside a set, a track with
    // no name at all, which is how a rehearsal's room recording arrives.
    for (i, (entry, flag)) in entries.iter().zip(rehearsal.iter_mut()).enumerate() {
        let nameless = song.is_some() && loose(&entry.name).is_empty() && !is_folder(i);
        if of_song(&entry.name) || nameless {
            *flag = true;
        }
    }
    // A folder of nothing but the set (innermost first, so a folder of
    // such folders follows).
    for i in (0..count).rev() {
        let range = inside(i);
        if is_folder(i)
            && !range.is_empty()
            && range.clone().all(|j| rehearsal.get(j) == Some(&true))
        {
            if let Some(flag) = rehearsal.get_mut(i) {
                *flag = true;
            }
        }
    }

    for (entry, rehearsed) in entries.iter_mut().zip(rehearsal) {
        // Reference material already (a stem split) keeps what it is.
        if entry.path.first().is_some_and(|g| g == "Reference") {
            continue;
        }
        let bounce = is_song(&entry.name)
            || stems_of(&entry.name, song.as_deref())
            || dated(&entry.name)
            || says_reference(&entry.name);
        if rehearsed && !guide(entry) {
            entry.path = REHEARSAL_PATH.iter().map(|s| (*s).to_string()).collect();
        } else if bounce {
            entry.path = vec!["Reference".to_string()];
        }
    }
    entries
}

/// Whether `name` is a render named the way REAPER names one by default:
/// `Song - <date and time>` (`Intro - 6_16_26, 9.28 PM`).
fn dated(name: &str) -> bool {
    name.rsplit_once(" - ").is_some_and(|(song, when)| {
        let when = when.trim().to_lowercase();
        let when = when.trim_end_matches("am").trim_end_matches("pm");
        !song.trim().is_empty()
            && when.chars().any(|c| c.is_ascii_digit())
            && when
                .chars()
                .all(|c| c.is_ascii_digit() || " _.:,/-".contains(c))
    })
}

/// Whether a name says it is a reference: a `REF` / `Reference` word
/// (`DRUM REF`, `Vocal Reference`), whatever instrument it also names.
fn says_reference(name: &str) -> bool {
    name.split(|c: char| !c.is_alphanumeric())
        .any(|w| w.eq_ignore_ascii_case("ref") || w.eq_ignore_ascii_case("reference"))
}

/// A name with case, edge spaces and punctuation at its ends ignored.
fn loose(name: &str) -> String {
    name.trim()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

/// `Part - Song` split at its last ` - `, both sides [`loose`]; `None`
/// without one or with either side empty.
fn part_and_song(name: &str) -> Option<(String, String)> {
    let (part, song) = name.rsplit_once(" - ")?;
    let (part, song) = (loose(part), loose(song));
    (!part.is_empty() && !song.is_empty()).then_some((part, song))
}

/// The song a rehearsal set is named for: the `Part - Song` suffix shared by
/// two or more tracks with different parts — the most such, if several.
fn rehearsal_song(names: &[&str]) -> Option<String> {
    let mut parts: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    // A date is when a render was made, not what it is a render of.
    for (part, song) in names
        .iter()
        .filter(|n| !dated(n))
        .filter_map(|n| part_and_song(n))
    {
        parts.entry(song).or_default().insert(part);
    }
    parts
        .into_iter()
        .filter(|(_, parts)| parts.len() >= 2)
        .max_by_key(|(_, parts)| parts.len())
        .map(|(song, _)| song)
}

/// Whether `name` is a bounce's stems: `… - stem`, a bare `stem(s)`, or the
/// song's own name with `stems` after it (`empty focus_stems`).
fn stems_of(name: &str, song: Option<&str>) -> bool {
    let lower = name.trim().to_lowercase();
    let stemmed = |s: &str| s.ends_with("stem") || s.ends_with("stems");
    lower.ends_with("- stem")
        || matches!(loose(&lower).as_str(), "stem" | "stems")
        || song.is_some_and(|song| lower.starts_with(song) && stemmed(&lower))
}

/// One track, its ancestor folders, and what it classifies into.
#[derive(Debug, Clone)]
pub struct TrackContext<Id> {
    /// The track.
    pub track: Id,
    /// Its own name.
    pub name: String,
    /// Ancestor folder names, outermost first.
    pub context: Vec<String>,
    /// The canonical group path it classifies into, empty if none.
    pub path: Vec<String>,
}

/// Colour every track by what it classifies as, offline.
///
/// The same taxonomy that decides a track's folder and its bus decides its
/// colour: a track that classifies into `Guitars/Electric` gets the electric-
/// guitar colour because that is what it *is*. Nothing here consults the DAW,
/// so a project can be coloured on disk and simply open correct.
///
/// Bus tracks are skipped — [`apply_buses`] already colours those from the bus
/// spec, and re-colouring `DRUM BUS` as a drum would tint it like content.
/// Tracks that classify to nothing are left alone rather than guessed at; they
/// are [`gather_unsorted`]'s problem.
///
/// Returns the number of tracks coloured.
///
/// # Note on the other implementation
///
/// `session::color::classify` does this same name → group → palette lookup for
/// the live REAPER runtime, and cannot be called from here (`session` depends
/// on this crate, so the arrow only points one way). It is worth collapsing the
/// two — a third divergent colour rule is exactly what that module was created
/// to end — but the move has to go downward, into this crate, with `session`
/// re-exporting it.
///
/// # Errors
///
/// Returns [`T::Error`] if any backend operation fails.
pub fn apply_colors<T: TemplateTarget>(target: &mut T) -> Result<usize, T::Error> {
    let mut painted: usize = 0;
    for entry in classified_paths(target) {
        if crate::buses::is_bus_name(&entry.name) {
            continue;
        }
        let refs: Vec<&str> = entry.path.iter().map(String::as_str).collect();
        let Some(color) = drum_color(&refs).or_else(|| crate::colors::color_for_path(&refs)) else {
            continue;
        };
        target.set_color(&entry.track, &color.to_hex_string())?;
        painted = painted.saturating_add(1);
    }
    Ok(painted)
}

/// A drum's colour: a shade of the drums' red by which part it is.
///
/// Kick, toms, snare, cymbals, rooms each a step lighter or darker, so a
/// kit reads as one family down the panel, not as a red kick, a yellow hat
/// and a blue snare. `None` for anything not under `Drums`.
#[must_use]
pub fn drum_color(path: &[&str]) -> Option<crate::colors::Color> {
    if path
        .first()
        .is_none_or(|g| !g.eq_ignore_ascii_case("Drums"))
    {
        return None;
    }
    let part = |name: &str| path.iter().any(|p| p.eq_ignore_ascii_case(name));
    let lighter = if part("Kick") {
        -0.04
    } else if part("Toms") {
        0.04
    } else if part("Snare") {
        0.08
    } else if part("Cymbals") {
        0.14
    } else if part("Rooms") {
        0.2
    } else {
        0.0
    };
    Some(crate::colors::adjust_lightness(
        crate::colors::groups::DRUMS,
        lighter,
    ))
}

/// The name of the holding folder for tracks that classify to nothing.
pub const UNSORTED_FOLDER: &str = "UNSORTED";

/// The folder reference material is laid out in: the song's bounces, and
/// [`REHEARSAL_FOLDER`] inside it.
pub const REFERENCES_FOLDER: &str = "REFERENCES";

/// Whether a track is a bus: one of the template's, a session's own named
/// `… Bus`, or a headphone mix.
#[must_use]
pub fn is_bus_track(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    crate::buses::is_bus_name(name)
        || lower.ends_with(" bus")
        || lower == "bus"
        || lower.contains("headphone")
}

/// Whether a track is a DI capture — a `DI` token in its name (`DI`,
/// `DI Pedalboard`, `Bass DI`), the clean signal kept to reamp from.
#[must_use]
pub fn is_capture(name: &str) -> bool {
    name.split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .any(|w| w.eq_ignore_ascii_case("di") || w.eq_ignore_ascii_case("d.i."))
}

/// Whether a track or folder is a drum trigger — a `Trig` or `Trigger`
/// word in its name (`Trig`, `T1 Trig`, `KICK TRIG`): a source for sample
/// replacement, silent until a sampler is put on it.
#[must_use]
pub fn is_trigger(name: &str) -> bool {
    name.split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .any(|w| w.eq_ignore_ascii_case("trig") || w.eq_ignore_ascii_case("trigger"))
}

/// The vocal returns ([`DawTarget::apply_vox_fx`]) that start on, and
/// their level (dB): `(folder, return, level)`.
pub const VOX_FX_START: &[(&str, &str, f64)] = &[
    ("Verb", "Short", -16.5),
    ("Verb", "Long", -28.6),
    ("Delay", "Slap", -6.7),
    ("Pitch", "Oct-", -29.4),
];

/// Where the vocal returns that start muted sit.
pub const VOX_FX_QUIET_DB: f64 = -20.0;

/// Whether a track is a drum's samples — `Sample`, `T1 Sample`, a
/// `Sample` folder of layers: what plays from its `Trig`, which is what
/// says the drum was hit. Not a mic: no Fund is fed by it.
#[must_use]
pub fn is_sample(name: &str) -> bool {
    name.split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .any(|w| w.eq_ignore_ascii_case("sample") || w.eq_ignore_ascii_case("samples"))
}

/// Whether a track is one of the golden template's one-note tracks:
/// a piece's `Fund`, the kick's `Sub` — a band-pass at the drum's
/// fundamental, fed by its mics, gated by its hits.
#[must_use]
pub fn is_one_note(name: &str) -> bool {
    let name = name.trim();
    name.eq_ignore_ascii_case("fund") || name.eq_ignore_ascii_case("sub")
}

/// The drum kit's parallel chains' folder (the golden template's).
pub const PROCESS_FOLDER: &str = "Process";

/// The kit's parallel compressors, dry to crushed.
pub const PROCESS_COMPRESS: &[&str] = &["Dry", "Tight", "Punch", "Smash", "Crunch"];

/// The kit's reverb bank: the room the band is in.
pub const PROCESS_VERBS: &[&str] = &[
    "Wood Room",
    "Music Club",
    "Stadium",
    "RMX 16",
    "Nonlin",
    "Brick Wall",
];

/// The folder the guide's rendered prints (`ClickPrint`, `ShakePrint`)
/// go in, inside the Guide — the `Prints` group's name.
pub const PRINTS_FOLDER: &str = "PRINTS";

/// The folder a rehearsal recording is laid out in, inside
/// [`REFERENCES_FOLDER`].
pub const REHEARSAL_FOLDER: &str = "REHEARSAL";

/// Gather `tracks` into an [`UNSORTED_FOLDER`] at the end of the project.
///
/// Tracks that match no group are not guesses to be routed somewhere — they
/// are work for a human. Parking them together at the end keeps them audible
/// and obvious on open, rather than scattered through the track list or
/// silently dropped from the mix.
///
/// The folder is left on its default parent send, so nothing that was audible
/// before becomes inaudible. Returns the folder id, or `None` when there is
/// nothing to gather.
///
/// # Errors
///
/// Returns [`T::Error`] if any backend operation fails.
pub fn gather_unsorted<T: TemplateTarget>(
    target: &mut T,
    tracks: &[T::TrackId],
) -> Result<Option<Gathered<T::TrackId>>, T::Error> {
    if tracks.is_empty() {
        return Ok(None);
    }
    target.gather_into_folder(UNSORTED_FOLDER, tracks)
}

/// What [`apply_routing`] did, per track.
#[derive(Debug, Default, Clone)]
pub struct RoutingReport {
    /// Tracks given a send to their bus.
    pub routed: Vec<String>,
    /// Tracks left alone because an ancestor folder already routes to a bus —
    /// they reach it through the folder.
    pub covered: Vec<String>,
    /// Tracks that classify to no bus.
    pub unrouted: Vec<String>,
    /// Tracks that already fed a bus, left as the engineer had them.
    pub already_routed: Vec<String>,
    /// Tracks whose group deliberately reaches no bus — a VCA carries fader
    /// control, not audio. Finished, not a gap; see
    /// [`is_deliberately_unrouted`](crate::buses::is_deliberately_unrouted).
    pub control_only: Vec<String>,
}

/// Send every classified track to its bus.
///
/// # Only the outermost classified track carries the send
///
/// A track inside a folder already feeds that folder. If the folder routes to
/// `GUITAR BUS` and each guitar inside it *also* routes there, every guitar
/// arrives twice — once through the folder, once directly — and the bus is 6 dB
/// hot with the folder fader no longer controlling anything.
///
/// So this walks the project's nesting and routes the **outermost** track that
/// classifies, exactly mirroring the golden template's attachment points: the
/// `Guitars` folder takes the send, and `Guitars/Electric/Amp 1` inside it
/// keeps its ordinary parent send and rides along.
///
/// Bus tracks are skipped — [`apply_buses`] already wired those, and a bus
/// sending to itself is a feedback loop.
///
/// # A track that already feeds *some* bus is left alone
///
/// Checking only the bus this track *would* get is not enough. Real sessions
/// are already routed, sometimes to a different bus than the classifier picks:
/// `GTR A` feeding `Guitar A BUS` classifies as electric, so adding its send to
/// `ELECTRIC BUS` leaves it arriving on both and 6 dB hot. The engineer's own
/// routing is the better evidence, so any existing send into the bus tree wins
/// and the track is reported as [`already_routed`](RoutingReport::already_routed).
///
/// Idempotent, both against itself and against a project that was routed by
/// hand.
///
/// # Errors
///
/// Returns [`T::Error`] if any backend operation fails.
pub fn apply_routing<T: TemplateTarget>(
    target: &mut T,
    buses: &AppliedBuses<T::TrackId>,
) -> Result<RoutingReport, T::Error> {
    let depths: Vec<i32> = target
        .folder_depths()
        .into_iter()
        .map(|(_, _, d)| d)
        .collect();
    let entries = classified_paths(target);

    let bus_ids: Vec<T::TrackId> = buses.by_name.values().cloned().collect();
    let mut report = RoutingReport::default();
    // One flag per open folder: whether it (or something above it) already
    // carries a send, in which case everything inside is covered.
    let mut open: Vec<bool> = Vec::new();

    for (entry, change) in entries.into_iter().zip(depths) {
        let covered = open.iter().any(|routed| *routed);
        let mut routed_here = false;

        if crate::buses::is_bus_name(&entry.name) {
            // A bus is not content; leave its own routing to apply_buses.
        } else if covered {
            report.covered.push(entry.name.clone());
        } else if let Some(existing) = bus_ids
            .iter()
            .find(|bus| target.has_send(&entry.track, bus))
        {
            let _ = existing;
            report.already_routed.push(entry.name.clone());
            routed_here = true;
        } else if crate::buses::is_deliberately_unrouted(&entry.path) {
            report.control_only.push(entry.name.clone());
        } else {
            match crate::buses::bus_for_path(&entry.path).and_then(|b| buses.get(b)) {
                Some(bus) => {
                    let bus = bus.clone();
                    route_to_bus(target, &entry.track, &bus)?;
                    report.routed.push(entry.name.clone());
                    routed_here = true;
                }
                None => report.unrouted.push(entry.name.clone()),
            }
        }

        if change > 0 {
            open.push(routed_here || covered);
        } else if change < 0 {
            // change is i32 from template data (negative in this branch),
            // negation is safe and the result is always non-negative.
            #[allow(
                clippy::arithmetic_side_effects,
                clippy::cast_sign_loss,
                clippy::as_conversions
            )]
            let closes = -change as usize;
            for _ in 0..closes {
                open.pop();
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buses::{all_buses, names};

    fn flat() -> Vec<FlatBusTrack> {
        flatten_buses(&bus_nodes(&all_buses()))
    }

    #[test]
    fn flattening_encodes_the_nesting_as_folder_depth() {
        let rows = flat();
        let shape: Vec<(&str, usize, i32)> = rows
            .iter()
            .map(|r| (r.name.as_str(), r.depth, r.folder_depth.to_raw_value()))
            .collect();

        assert_eq!(
            shape,
            vec![
                (names::MIX, 0, 1),
                (names::INST, 1, 1),
                (names::DRUM, 2, 0),
                (names::PERC, 2, 0),
                (names::BASS, 2, 0),
                (names::GUITAR, 2, 1),
                (names::ACOUSTIC, 3, 0),
                // Last inside GUITAR BUS — closes it, but KEYS BUS follows in
                // INST BUS, so only one level.
                (names::ELECTRIC, 3, -1),
                (names::KEYS, 2, 0),
                (names::ORCH, 2, 0),
                // Last track in INST BUS — closes it, but not MIX BUS, since
                // VOX BUS follows at the same depth.
                (names::FX, 2, -1),
                (names::VOX, 1, 1),
                (names::LEAD_VOX, 2, 0),
                // Last track in both VOX BUS and MIX BUS — closes two levels.
                (names::BGV, 2, -2),
                (names::GUIDE, 0, 0),
                (names::HEADPHONES, 0, 0),
                (names::TALKBACK, 0, 0),
                (names::UTILITY, 0, 0),
            ]
        );
    }

    #[test]
    fn depth_changes_sum_to_zero() {
        // A project whose folder depths do not balance is corrupt — REAPER
        // renders every following track inside a folder that never closes.
        let total: i32 = flat().iter().map(|r| r.folder_depth.to_raw_value()).sum();
        assert_eq!(total, 0);
    }

    #[test]
    fn a_lone_bus_opens_no_folder() {
        let single: Vec<TemplateBus> = all_buses()
            .into_iter()
            .filter(|b| b.name == names::GUIDE)
            .collect();
        let rows = flatten_buses(&bus_nodes(&single));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].folder_depth, FolderDepthChange::Normal);
    }

    #[test]
    fn buses_carry_their_color_and_channel_count() {
        let rows = flat();
        let drum = rows.iter().find(|r| r.name == names::DRUM).unwrap();
        assert_eq!(
            rows.iter().find(|r| r.name == names::MIX).unwrap().channels,
            2
        );
        assert!(
            drum.color_hex.is_some(),
            "DRUM BUS should take the drums color"
        );
    }
}

#[cfg(test)]
mod rehearsal_tests {
    use super::{folders_by_contents, reclassify_rehearsal, TrackContext, REHEARSAL_PATH};

    /// `(depth, name, path)` rows as the classifier leaves them: each track
    /// with its ancestor folders, as `contextual_paths` builds them.
    fn entries(rows: &[(usize, &str, &[&str])]) -> Vec<TrackContext<usize>> {
        let mut ancestors: Vec<String> = Vec::new();
        rows.iter()
            .enumerate()
            .map(|(i, (depth, name, path))| {
                ancestors.truncate(*depth);
                let entry = TrackContext {
                    track: i,
                    name: (*name).to_string(),
                    context: ancestors.clone(),
                    path: path.iter().map(|s| (*s).to_string()).collect(),
                };
                ancestors.push((*name).to_string());
                entry
            })
            .collect()
    }

    fn path_of<'a>(entries: &'a [TrackContext<usize>], name: &str) -> Vec<&'a str> {
        entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.path.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// Crescendum's `unbreakable`: a `Tracks` container holding the band's
    /// rehearsal stems, the click among them, beside the real recording.
    #[test]
    fn a_rehearsal_container_is_reference_material_but_its_click_is_not() {
        let out = reclassify_rehearsal(entries(&[
            (0, "Tracks", &["Tracks"]),
            (1, "Click", &["Guide", "Click"]),
            (1, " - stem", &[]),
            (1, "", &[]),
            (1, "Bass - Unbreakable", &["Bass"]),
            (1, "Drums", &["Drums"]),
            (2, "Kick - Unbreakable", &["Drums", "Kick"]),
            (2, "Snare - Unbreakable", &["Drums", "Snare"]),
            (1, "Lead - Unbreakable", &[]),
            (0, "Electric GTR", &["Guitars", "Electric"]),
            (1, "Main", &["Guitars", "Electric"]),
        ]));
        for name in [
            "Tracks",
            " - stem",
            "",
            "Bass - Unbreakable",
            "Drums",
            "Kick - Unbreakable",
            "Lead - Unbreakable",
        ] {
            assert_eq!(path_of(&out, name), REHEARSAL_PATH, "{name:?}");
        }
        assert_eq!(
            path_of(&out, "Click"),
            ["Guide", "Click"],
            "the session's click stays"
        );
        assert_eq!(
            path_of(&out, "Main"),
            ["Guitars", "Electric"],
            "the recording is untouched"
        );
    }

    /// `empty focus`: no container — the set loose at the top, a folder of
    /// nothing but the set, a nameless room track, and the song's stems.
    #[test]
    fn a_loose_rehearsal_and_the_songs_bounces_are_found_by_the_song() {
        let out = reclassify_rehearsal(entries(&[
            (0, "Vocals - Empty Focus", &["Vocals"]),
            (0, "Drums", &["Drums"]),
            (1, "Snare - Empty Focus", &["Drums", "Snare"]),
            (1, "Kick - Empty Focus", &["Drums", "Kick"]),
            (0, "Electric GTR", &["Guitars", "Electric"]),
            (1, "Main", &["Guitars", "Electric"]),
            (0, "", &[]),
            (0, "empty focus_stems", &[]),
            (0, "Intro - 6_16_26, 9.28 PM", &[]),
        ]));
        for name in ["Vocals - Empty Focus", "Drums", "Kick - Empty Focus", ""] {
            assert_eq!(path_of(&out, name), REHEARSAL_PATH, "{name:?}");
        }
        assert_eq!(path_of(&out, "empty focus_stems"), ["Reference"]);
        assert_eq!(
            path_of(&out, "Intro - 6_16_26, 9.28 PM"),
            ["Reference"],
            "a dated render"
        );
        assert_eq!(path_of(&out, "Electric GTR"), ["Guitars", "Electric"]);
    }

    /// One `Part - Song` is not a set: a track named `Vocals - Lead` in a
    /// session with no rehearsal is a vocal, and an unnamed track is left
    /// alone.
    #[test]
    fn without_a_set_nothing_is_rehearsal() {
        let out = reclassify_rehearsal(entries(&[
            (0, "Vocals - Lead", &["Vocals", "Lead"]),
            (0, "", &[]),
            (0, "Tracks2", &[]),
        ]));
        assert_eq!(path_of(&out, "Vocals - Lead"), ["Vocals", "Lead"]);
        assert!(path_of(&out, "").is_empty());
    }

    /// `Intro`: a stem split of a dated render (`…, Intro - 6_16_26`) is
    /// not a rehearsal set — the date is not a song — and keeps its path.
    #[test]
    fn a_dated_stem_split_is_not_a_rehearsal() {
        let out = reclassify_rehearsal(entries(&[
            (0, "Intro Strems", &["Reference", "Stem Split"]),
            (
                1,
                "(Example: Intro - 6_16_26, (Bass)_1_0001),Intro - 6_16_26",
                &["Reference", "Stem Split"],
            ),
            (
                1,
                "(Example: Intro - 6_16_26, (Drums)_1_0002),Intro - 6_16_26",
                &["Reference", "Stem Split"],
            ),
            (0, "Steel String", &["Guitars", "Acoustic"]),
            (0, "", &[]),
        ]));
        assert_eq!(path_of(&out, "Intro Strems"), ["Reference", "Stem Split"]);
        assert!(
            path_of(&out, "").is_empty(),
            "no set, so a nameless track is not one"
        );
    }

    /// A session's own group folders side by side are not a stem split.
    #[test]
    fn group_folders_are_not_a_stem_split() {
        let out = super::reclassify_stem_splits(entries(&[
            (0, "Drums", &["Drums"]),
            (1, "Kick", &["Drums", "Kick"]),
            (0, "Bass", &["Bass"]),
            (1, "Bass DI", &["Bass"]),
            (0, "Vocals", &["Vocals"]),
            (1, "Lead", &["Vocals", "Lead"]),
        ]));
        assert_eq!(path_of(&out, "Bass"), ["Bass"]);
        assert_eq!(path_of(&out, "Drums"), ["Drums"]);
        assert_eq!(path_of(&out, "Vocals"), ["Vocals"]);
    }

    /// A kit is one family of reds; nothing else is touched.
    #[test]
    fn every_drum_is_a_shade_of_the_drums_red() {
        let hue = |c: crate::colors::Color| c.to_hsl().0;
        let base = hue(crate::colors::groups::DRUMS);
        for path in [
            &["Drums", "Drum Kit", "Kick"][..],
            &["Drums", "Drum Kit", "Snare"],
            &["Drums", "Drum Kit", "Cymbals", "OH"],
            &["Drums", "Drum Kit", "Rooms"],
            &["Drums"],
        ] {
            let color = super::drum_color(path).expect("a drum");
            // Around the circle: red sits where 360° meets 0°.
            let apart = (hue(color) - base).abs();
            assert!(apart.min(360.0 - apart) < 1.0, "{path:?} is the drums' hue");
        }
        assert!(super::drum_color(&["Bass"]).is_none());
    }

    /// `The ballad`: a nameless folder of acoustics is the acoustics'.
    #[test]
    fn a_nameless_folder_takes_what_its_contents_share() {
        let out = folders_by_contents(entries(&[
            (0, "", &[]),
            (1, "AG GTR", &["Guitars", "Acoustic"]),
            (2, "L", &["Guitars", "Acoustic"]),
            (1, "AG GTR", &["Guitars", "Acoustic"]),
            (0, "Mixed", &[]),
            (1, "Kick", &["Drums", "Kick"]),
            (1, "Bass", &["Bass"]),
        ]));
        assert_eq!(path_of(&out, ""), ["Guitars", "Acoustic"]);
        assert!(
            path_of(&out, "Mixed").is_empty(),
            "contents that disagree say nothing"
        );
    }

    /// A shaker is percussion; a shaker guide or a printed shaker is the
    /// guide's; a print of the mix is still a reference.
    #[test]
    fn a_shaker_guide_is_the_guides_and_a_shaker_is_percussion() {
        let top = |name: &str| {
            crate::track_schema::classify_track(name)
                .matched_groups
                .first()
                .cloned()
                .unwrap_or_default()
        };
        for name in ["ShakePrint", "Shake Print", "Shaker Guide", "ClickPrint"] {
            assert_eq!(top(name), "Guide", "{name}");
        }
        for name in ["Shaker", "130-Shaker"] {
            assert_eq!(top(name), "Percussion", "{name}");
        }
        assert_eq!(top("Mix Print"), "Reference");
        let path = crate::track_schema::classify_track("ClickPrint").matched_groups;
        assert_eq!(
            path,
            ["Guide", super::PRINTS_FOLDER],
            "a print is in PRINTS"
        );
        // Whatever folder it sits in.
        let inside = crate::track_schema::classify_track("CLICK + GUIDE ShakePrint").matched_groups;
        assert_eq!(
            inside,
            ["Guide", super::PRINTS_FOLDER],
            "inside a guide folder too"
        );
    }

    /// A track that says REF is a reference, whatever it also names; a
    /// screamed part is a vocal.
    #[test]
    fn a_ref_is_a_reference_and_a_scream_is_a_vocal() {
        let out = reclassify_rehearsal(entries(&[
            (0, "Drums", &["Drums"]),
            (1, "Kick", &["Drums", "Kick"]),
            (1, "DRUM REF", &["Drums"]),
            (0, "Referee", &[]),
        ]));
        assert_eq!(path_of(&out, "DRUM REF"), ["Reference"]);
        assert_eq!(path_of(&out, "Kick"), ["Drums", "Kick"]);
        assert!(path_of(&out, "Referee").is_empty(), "a word, not a prefix");
        let scream = crate::track_schema::classify_track("Scream 2").matched_groups;
        assert_eq!(scream.first().map(String::as_str), Some("Vocals"));
    }

    #[test]
    fn a_steel_string_is_an_acoustic() {
        let path = crate::track_schema::classify_track("Steel String").matched_groups;
        assert!(path.iter().any(|g| g == "Acoustic"), "{path:?}");
        let pedal = crate::track_schema::classify_track("Pedal Steel").matched_groups;
        assert!(pedal.iter().any(|g| g == "Steel"), "{pedal:?}");
    }
}

#[cfg(test)]
mod trigger_tests {
    #[test]
    fn a_trigger_is_named_trig() {
        for name in ["Trig", "T1 Trig", "KICK TRIG", "Snare Trigger", "Tom_trig"] {
            assert!(super::is_trigger(name), "{name}");
        }
        for name in ["Kick In", "Trigga", "Triangle", "Strings"] {
            assert!(!super::is_trigger(name), "{name}");
        }
    }
}

#[cfg(test)]
mod player_dash_tests {
    /// `<player> - <instrument>` is the instrument; `<part> - <song>` is
    /// the part.
    #[test]
    fn a_dash_names_the_instrument_after_it_or_the_part_before() {
        let first = |name: &str| {
            crate::track_schema::classify_track(name)
                .matched_groups
                .first()
                .cloned()
        };
        assert_eq!(first("Tom - Keys").as_deref(), Some("Keys"));
        assert_eq!(first("Kick - Unbreakable").as_deref(), Some("Drums"));
        assert_eq!(first("Bass - The Plague").as_deref(), Some("Bass"));
        assert_eq!(
            first("Toms").as_deref(),
            Some("Drums"),
            "a tom is still a drum"
        );
    }
}
