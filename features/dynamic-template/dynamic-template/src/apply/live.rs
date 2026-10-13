//! [`TemplateTarget`] over a live DAW — any backend behind the
//! `daw::service` traits: a running REAPER (`daw_reaper::Reaper`) or the
//! in-process `daw-standalone` engine.
//!
//! The offline counterparts are [`super::dawfile`] and [`super::chunk`]; all
//! of them are driven by [`organize`](super::organize), so the bus tree this
//! builds in a live session is the same one the file backends write to an
//! `.RPP`.
//!
//! Everything goes through the service traits — never raw
//! `reaper_low`/`reaper_medium` FFI. On REAPER these calls are synchronous
//! and must run on its main thread, which is where extension action
//! handlers already are.

use daw::service::{Routing, Tracks};
use daw_proto::{FolderDepthChange, ProjectContext, ReorderTracksBehavior, TrackRef};
#[cfg(feature = "reaper")]
use daw_reaper::Reaper;

use super::TemplateTarget;

/// A live project the template can be applied to, on whatever backend `D`
/// is.
pub struct DawTarget<D> {
    daw: D,
    project: ProjectContext,
}

/// A live REAPER project — the extension actions' target.
#[cfg(feature = "reaper")]
pub type ReaperTarget = DawTarget<Reaper>;

impl<D> DawTarget<D> {
    /// Target `project` on `daw`.
    pub const fn on(daw: D, project: ProjectContext) -> Self {
        Self { daw, project }
    }
}

#[cfg(feature = "reaper")]
impl ReaperTarget {
    /// Target the project REAPER currently has in front.
    #[must_use]
    pub const fn current() -> Self {
        Self::new(ProjectContext::Current)
    }

    /// Target a specific project.
    #[must_use]
    pub const fn new(project: ProjectContext) -> Self {
        Self {
            daw: Reaper,
            project,
        }
    }
}

impl<D: Tracks + Routing> TemplateTarget for DawTarget<D> {
    /// Track GUIDs — stable across the inserts and reorders that would
    /// invalidate an index.
    type TrackId = String;
    type Error = daw_proto::DawError;

    fn find_track(&self, name: &str) -> Option<String> {
        self.daw
            .all(self.project.clone())
            .into_iter()
            .find(|t| t.name.trim().eq_ignore_ascii_case(name.trim()))
            .map(|t| t.guid)
    }

    fn append_track(&mut self, name: &str) -> Result<String, Self::Error> {
        self.daw.add(self.project.clone(), name, None)
    }

    fn set_folder_depth(
        &mut self,
        id: &String,
        depth: FolderDepthChange,
    ) -> Result<(), Self::Error> {
        self.daw.set_folder_depth(
            self.project.clone(),
            TrackRef::Guid(id.clone()),
            depth.to_raw_value(),
        )
    }

    fn set_color(&mut self, id: &String, hex: &str) -> Result<(), Self::Error> {
        // An unparseable color leaves the track's own color alone rather than
        // failing the whole apply — the routing matters, the tint does not.
        let Ok(color) = color_palette::Color::from_hex_str(hex) else {
            return Ok(());
        };
        // REAPER color values are 24-bit RGB codes; the i32 is a signed
        // wrapper around unsigned bit patterns, so reinterpret rather than
        // convert.
        let color_u32 = color.to_reaper_native().cast_unsigned();
        self.daw
            .set_color(self.project.clone(), TrackRef::Guid(id.clone()), color_u32)
    }

    fn set_channel_count(&mut self, id: &String, channels: u32) -> Result<(), Self::Error> {
        self.daw
            .set_num_channels(self.project.clone(), TrackRef::Guid(id.clone()), channels)
    }

    fn has_send(&self, source: &String, dest: &String) -> bool {
        self.daw
            .sends(self.project.clone(), TrackRef::Guid(source.clone()))
            .iter()
            .any(|route| route.dest_track_guid.as_ref() == Some(dest))
    }

    fn add_send(&mut self, source: &String, dest: &String) -> Result<(), Self::Error> {
        self.daw.add_send(
            self.project.clone(),
            TrackRef::Guid(source.clone()),
            TrackRef::Guid(dest.clone()),
        );
        Ok(())
    }

    fn set_parent_send(&mut self, id: &String, enabled: bool) -> Result<(), Self::Error> {
        self.daw
            .set_parent_send_enabled(self.project.clone(), TrackRef::Guid(id.clone()), enabled)
    }

    fn folder_depths(&self) -> Vec<(String, String, i32)> {
        self.daw
            .all(self.project.clone())
            .into_iter()
            .map(|t| (t.guid, t.name, t.folder_depth))
            .collect()
    }

    /// The live form of the file backends' DI nesting: a "DI" track right
    /// after a plain sibling, in a group that opts in, becomes a muted child
    /// of it. (The file backends also collapse the new folder; the service
    /// surface has no collapse call, so a live nest stays expanded.)
    fn nest_secondary_mics(&mut self) {
        let config = crate::default_config();
        let entries = super::contextual_paths(self);
        let depths: std::collections::HashMap<String, i32> = self
            .folder_depths()
            .into_iter()
            .map(|(guid, _, depth)| (guid, depth))
            .collect();
        for pair in entries.windows(2) {
            let [main, di] = pair else { continue };
            if main.context != di.context {
                continue;
            }
            let is_di = |name: &str| name.trim().eq_ignore_ascii_case("di");
            if !is_di(&di.name) || is_di(&main.name) {
                continue;
            }
            let Some(leaf) = di.path.last() else { continue };
            if !super::find_group(&config, leaf).is_some_and(|g| g.nest_secondary_mics) {
                continue;
            }
            let main_depth = depths.get(&main.track).copied().unwrap_or(0);
            let di_depth = depths.get(&di.track).copied().unwrap_or(0);
            if main_depth != 0 || di_depth > 0 {
                continue;
            }
            let project = self.project.clone();
            let _ = self.daw.set_folder_depth(
                project.clone(),
                TrackRef::Guid(main.track.clone()),
                FolderDepthChange::FolderStart.to_raw_value(),
            );
            let _ = self.daw.set_folder_depth(
                project.clone(),
                TrackRef::Guid(di.track.clone()),
                di_depth.saturating_sub(1),
            );
            let _ = Tracks::set_muted(&self.daw, project, TrackRef::Guid(di.track.clone()), true);
        }
    }

    fn gather_into_folder(
        &mut self,
        folder: &str,
        tracks: &[String],
    ) -> Result<Option<crate::apply::Gathered<String>>, Self::Error> {
        // Same rule as the file backends: only a plain track at the top
        // level moves. One carrying folder structure would strand what it
        // held open, and one nested inside a folder belongs to that folder —
        // pulling it out reshapes a folder the user built.
        let all = self.daw.all(self.project.clone());
        let mut running = 0i32;
        let top_level_plain: std::collections::HashSet<&str> = all
            .iter()
            .filter_map(|t| {
                let at_top = running == 0;
                running = running.saturating_add(t.folder_depth);
                (at_top && t.folder_depth == 0).then_some(t.guid.as_str())
            })
            .collect();
        let movable: Vec<String> = tracks
            .iter()
            .filter(|guid| top_level_plain.contains(guid.as_str()))
            .cloned()
            .collect();
        if movable.is_empty() {
            return Ok(None);
        }

        let folder_guid = self.daw.add(self.project.clone(), folder, None)?;
        let folder_index = u32::try_from(
            self.daw
                .all(self.project.clone())
                .iter()
                .position(|t| t.guid == folder_guid)
                .ok_or_else(|| {
                    daw_proto::DawError::NotFound(format!("track {folder_guid} vanished after add"))
                })?,
        )
        .map_err(|_| daw_proto::DawError::NotFound("track index out of range".to_string()))?;

        self.daw.clear_selection(self.project.clone())?;
        for guid in &movable {
            self.daw
                .set_selected(self.project.clone(), TrackRef::Guid(guid.clone()), true)?;
        }
        // MakeChildOfPreviousTrack drops them inside the folder we just made,
        // and REAPER keeps every send intact across the move because it tracks
        // routing by pointer, not position.
        self.daw.reorder_selected(
            self.project.clone(),
            folder_index.saturating_add(1),
            ReorderTracksBehavior::MakeChildOfPreviousTrack,
        )?;
        self.daw.clear_selection(self.project.clone())?;

        self.daw.set_folder_depth(
            self.project.clone(),
            TrackRef::Guid(folder_guid.clone()),
            FolderDepthChange::FolderStart.to_raw_value(),
        )?;
        let skipped = tracks
            .iter()
            .filter(|g| !movable.contains(g))
            .cloned()
            .collect();
        Ok(Some(crate::apply::Gathered {
            folder: folder_guid,
            moved: movable,
            skipped,
        }))
    }
}

// ── Folder layout, live ─────────────────────────────────────────────────
//
// The bus pass (`organize`) wires a session into the mix; these give it
// the folder shape the template describes — Guide, Keyflow, Drums, Bass,
// Guitars … — which the file backends leave to REAPER's own user.

use daw::service::Items;

/// The kept block (see `DawTarget::kept_folders`) whose folder is named
/// `name`, taken out of `units` and left open — its last track no longer
/// closing its folder — so the group's other tracks can follow inside it.
fn open_kept(
    units: &mut Vec<Vec<(String, i32)>>,
    all: &[daw::service::Track],
    name: &str,
) -> Option<Vec<(String, i32)>> {
    let at = units.iter().position(|unit| {
        unit.first()
            .and_then(|(g, _)| all.iter().find(|t| &t.guid == g))
            .is_some_and(|t| t.name.trim().eq_ignore_ascii_case(name.trim()))
    })?;
    let mut unit = units.remove(at);
    if let Some(last) = unit.last_mut() {
        last.1 = last.1.saturating_add(1);
    }
    Some(unit)
}

/// The instrument a rehearsal track (`Kick - Unbreakable`, `Lead -
/// Unbreakable`) belongs to: its part — the name before ` - Song` —
/// classified. A band's `Lead` and `Rhythm` are its guitars, which the
/// name alone does not say.
fn rehearsal_group(name: &str) -> Option<String> {
    let part = name
        .rsplit_once(" - ")
        .map_or(name, |(part, _)| part)
        .trim();
    if part.is_empty() {
        return None;
    }
    if let Some(group) = crate::track_schema::classify_track(part)
        .matched_groups
        .into_iter()
        .next()
    {
        return Some(group);
    }
    let guitar = ["lead", "rhythm", "rhy", "solo"];
    part.split_whitespace()
        .any(|w| guitar.contains(&w.to_lowercase().as_str()))
        .then(|| "Guitars".to_string())
}

/// The instrument groups in the order a session lays them out: the rhythm
/// section, then the harmony, then the voices.
const INSTRUMENT_ORDER: &[&str] = &[
    "Drums",
    // The kit's parallel chains, beside it (`DawTarget::add_drum_process`).
    super::PROCESS_FOLDER,
    "Percussion",
    "Bass",
    "Guitars",
    "Keys",
    "Synths",
    "Strings",
    "Orchestra",
    "Horns",
    "Harmonica",
    "Vocals",
    "Choir",
    "SFX",
];

/// What [`DawTarget::arrange_into_groups`] did.
#[derive(Debug, Default)]
pub struct Arranged {
    /// Wrapper folders removed (their tracks kept).
    pub unwrapped: Vec<String>,
    /// Group folders created.
    pub created: Vec<String>,
    /// Tracks placed inside a group.
    pub placed: usize,
}

impl<D: Tracks + Routing + Items> DawTarget<D> {
    /// Write what the template says `guid` is on the track
    /// ([`crate::golden_session::KIND_KEY`]), as REAPER's `P_EXT`: what
    /// the scenes read a track's kind from, kept with the song.
    fn mark(&self, guid: &str, kind: crate::golden_session::Kind) -> eyre::Result<()> {
        Tracks::set_ext_state(
            &self.daw,
            self.project.clone(),
            TrackRef::Guid(guid.to_owned()),
            crate::golden_session::KIND_KEY,
            kind.as_str(),
        )?;
        Ok(())
    }

    /// [`Self::mark`] with what `node` is, and the template group it
    /// stands for when it stands for one
    /// ([`crate::golden_session::GROUP_KEY`]: `Drums/Drum Kit/Kick`).
    fn mark_node(&self, guid: &str, node: &crate::golden_session::shape::Node) -> eyre::Result<()> {
        self.mark(guid, node.kind)?;
        if let Some(group) = node.template {
            Tracks::set_ext_state(
                &self.daw,
                self.project.clone(),
                TrackRef::Guid(guid.to_owned()),
                crate::golden_session::GROUP_KEY,
                &group.join("/"),
            )?;
        }
        Ok(())
    }

    /// A piece's recorded tracks marked by what they are: its trigger, or
    /// a mic.
    fn mark_recorded(&self, tracks: &[Branch]) -> eyre::Result<()> {
        use crate::golden_session::Kind;
        for track in tracks.iter().filter(|t| !t.is_folder()) {
            let kind = if super::is_trigger(&track.name) {
                Kind::Trigger
            } else {
                Kind::Source
            };
            self.mark(&track.guid, kind)?;
        }
        Ok(())
    }

    fn has_items(&self, guid: &str) -> bool {
        !Items::get_items(
            &self.daw,
            self.project.clone(),
            TrackRef::Guid(guid.to_owned()),
        )
        .is_empty()
    }

    /// Put every content track into the group folder the template's
    /// grouping engine (`organize_into_tracks`) assigns it — Drums, Bass,
    /// Guitars / Electric, Keys, Guide … — creating the folders it needs,
    /// and take apart any folder that only wrapped a multitrack (no media
    /// of its own, not a bus or a group). Tracks keep their own names.
    ///
    /// Buses, the Keyflow folder and anything the engine does not place
    /// keep their order, after the groups. Idempotent: a second run finds
    /// every track already in its group.
    ///
    /// # Errors
    ///
    /// The grouping engine, or a backend call, failed.
    pub fn arrange_into_groups(&mut self) -> eyre::Result<Arranged> {
        use crate::OrganizeIntoTracks;

        let mut arranged = Arranged::default();
        let project = self.project.clone();
        // Reference material, read with each track's folders around it —
        // a rehearsal is recognised by its set and its container, which a
        // name alone does not show (`reclassify_rehearsal`). It is laid out
        // apart, below, not grouped by what its tracks are called.
        let mut rehearsal: Vec<String> = Vec::new();
        let mut bounces: Vec<String> = Vec::new();
        let mut classified: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut group_of: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        // The guide's rendered prints, by name: kept in their own folder.
        let mut prints: std::collections::HashSet<String> = std::collections::HashSet::new();
        for entry in super::classified_paths(self) {
            // A bus is a bus: its name ("MIX BUS") classifies as what it
            // carries — the mix, a reference — which it is not.
            if crate::buses::is_bus_name(&entry.name) {
                continue;
            }
            if let Some(group) = entry.path.first() {
                group_of.insert(entry.track.clone(), group.clone());
            }
            if entry.path.iter().any(|g| g == super::PRINTS_FOLDER) {
                prints.insert(entry.name.clone());
            }
            let reference = entry.path.first().is_some_and(|g| g == "Reference");
            if entry
                .path
                .iter()
                .map(String::as_str)
                .eq(super::REHEARSAL_PATH)
            {
                rehearsal.push(entry.track);
            } else if reference {
                bounces.push(entry.track);
            } else if !entry.path.is_empty() {
                classified.insert(entry.track);
            }
        }
        let all = Tracks::all(&self.daw, project.clone());
        // A session already built: a top-level folder whose tracks are what
        // its name and theirs say (an `Electric GTR` of amps, sides and
        // mics) is the engineer's structure, kept whole — moved as one
        // block, never taken apart for its tracks to be sorted by their
        // names alone, which reads `Main` and `DI` as nothing. Each is its
        // tracks, `(guid, depth change)`, in order.
        let mut units = self.kept_folders(&all, &classified, &rehearsal, &bounces);
        let in_unit: std::collections::HashSet<String> =
            units.iter().flatten().map(|(g, _)| g.clone()).collect();

        // Groups the template knows, so a folder named for one is kept.
        let config = crate::default_config();
        let is_known_folder = |name: &str| {
            let n = name.trim();
            crate::buses::is_bus_name(n)
                || super::find_group(&config, n).is_some()
                || n.eq_ignore_ascii_case("Keyflow")
                || n.eq_ignore_ascii_case(super::UNSORTED_FOLDER)
        };

        // 1. Unwrap: a folder with no media that is not a bus or a group
        //    only held a multitrack together — nor does a rehearsal's
        //    container, whatever it is called. Its children stay.
        for track in &all {
            let container = rehearsal.contains(&track.guid) || bounces.contains(&track.guid);
            if in_unit.contains(&track.guid) {
                continue;
            }
            if track.folder_depth > 0
                && (container || !is_known_folder(&track.name))
                && !self.has_items(&track.guid)
            {
                rehearsal.retain(|g| g != &track.guid);
                bounces.retain(|g| g != &track.guid);
                Tracks::remove(
                    &self.daw,
                    project.clone(),
                    TrackRef::Guid(track.guid.clone()),
                )?;
                arranged.unwrapped.push(track.name.clone());
            }
        }
        let all = Tracks::all(&self.daw, project.clone());

        // 2. Content: tracks with media, outside the bus / Keyflow tree.
        let keyflow_children: std::collections::HashSet<String> = {
            let mut inside = std::collections::HashSet::new();
            let mut depth: Option<i32> = None;
            let mut running = 0i32;
            for t in &all {
                if let Some(d) = depth {
                    if running > d {
                        inside.insert(t.guid.clone());
                    } else {
                        depth = None;
                    }
                }
                if t.name.trim().eq_ignore_ascii_case("Keyflow") && t.folder_depth > 0 {
                    depth = Some(running);
                }
                running += t.folder_depth;
            }
            inside
        };
        let content: Vec<&daw::service::Track> = all
            .iter()
            .filter(|t| {
                !crate::buses::is_bus_name(&t.name)
                    && !keyflow_children.contains(&t.guid)
                    && !rehearsal.contains(&t.guid)
                    && !bounces.contains(&t.guid)
                    && !in_unit.contains(&t.guid)
                    && self.has_items(&t.guid)
            })
            .collect();
        let names: Vec<String> = content.iter().map(|t| t.name.clone()).collect();
        let hierarchy = if names.is_empty() {
            crate::TrackHierarchy::default()
        } else {
            names
                .organize_into_tracks(&config, None)
                .map_err(|e| eyre::eyre!("grouping: {e}"))?
        };

        // 3. The desired layout: (guid, depth change) in order.
        let mut unused: Vec<(String, String)> = content
            .iter()
            .map(|t| (t.name.clone(), t.guid.clone()))
            .collect();
        let mut claim = |name: &str| -> Option<String> {
            let at = unused.iter().position(|(n, _)| n == name)?;
            Some(unused.remove(at).1)
        };
        let mut layout: Vec<(String, i32)> = Vec::new();
        // A kept folder is not a group folder to reuse.
        let mut folders_in_use: std::collections::HashSet<String> = in_unit.clone();
        // The folders open at this point of the layout, by name, and the
        // top-level ones laid out so far.
        let mut open: Vec<String> = Vec::new();
        let mut top_folders: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut prints_open = false;
        let is_print = |node: &daw_proto::TrackNode| {
            !node.is_folder && node.items.first().is_some_and(|n| prints.contains(n))
        };
        for (at, node) in hierarchy.tracks.iter().enumerate() {
            let depth = node.folder_depth_change.to_raw_value();
            if node.is_folder || node.items.is_empty() {
                // A group whose folder is already a kept block (the kit is
                // `Drums`, and a stray tom goes in Drums): the block, left
                // open, and the group's tracks after its own — one folder.
                let kept = (open.is_empty() && depth > 0)
                    .then(|| open_kept(&mut units, &all, &node.name))
                    .flatten();
                if let Some(unit) = kept {
                    layout.extend(unit);
                    top_folders.insert(node.name.to_lowercase());
                    open.push(node.name.clone());
                    continue;
                }
                let guid = self.group_folder(&node.name, &mut folders_in_use, &mut arranged)?;
                layout.push((guid, depth));
                if depth > 0 {
                    if open.is_empty() {
                        top_folders.insert(node.name.to_lowercase());
                    }
                    open.push(node.name.clone());
                }
            } else {
                // A leaf: the original track its item names. The engine
                // may give a leaf more than one item; the first names it.
                let Some(guid) = node.items.first().and_then(|n| claim(n)) else {
                    continue;
                };
                arranged.placed += 1;
                let mut depth = depth;
                // Alone at the top, it still goes in its instrument's folder:
                // a lone `Bass DI` is the Bass folder's, not a stray track.
                // (Not when the group already has its folder here: the
                // layout runs in order, and a second one is a duplicate.)
                let group = group_of
                    .get(&guid)
                    .filter(|g| g.as_str() != "Reference")
                    .filter(|g| !top_folders.contains(&g.to_lowercase()));
                if let (true, Some(group)) = (open.is_empty(), group) {
                    top_folders.insert(group.to_lowercase());
                    // Into the kept block of that name, if there is one.
                    if let Some(unit) = open_kept(&mut units, &all, group) {
                        layout.extend(unit);
                    } else {
                        let folder =
                            self.group_folder(group, &mut folders_in_use, &mut arranged)?;
                        layout.push((folder, 1));
                    }
                    depth = depth.saturating_sub(1);
                }
                // A print goes in PRINTS — even the only kind of guide track
                // there is, which the engine would leave bare in the Guide.
                if is_print(node) {
                    if !prints_open {
                        let folder = self.group_folder(
                            super::PRINTS_FOLDER,
                            &mut folders_in_use,
                            &mut arranged,
                        )?;
                        layout.push((folder, 1));
                        open.push(super::PRINTS_FOLDER.to_string());
                        prints_open = true;
                    }
                    // The last of a run closes it too.
                    let next = hierarchy.tracks.get(at.saturating_add(1));
                    if !next.is_some_and(&is_print) {
                        depth = depth.saturating_sub(1);
                        prints_open = false;
                    }
                }
                layout.push((guid, depth));
            }
            // Closes: the folders it ends.
            if depth < 0 {
                for _ in 0..depth.unsigned_abs() {
                    open.pop();
                }
            }
        }

        // The kept folders, whole, beside the groups.
        layout.extend(units.into_iter().flatten());

        // 4. The reference material, after the groups: REFERENCES, the
        //    rehearsal in REHEARSAL inside it, then the song's bounces.
        let references =
            self.references_layout(&rehearsal, &bounces, &mut folders_in_use, &mut arranged)?;
        layout.extend(references);

        // 5. Everything else keeps its order and depth, after them.
        let placed: std::collections::HashSet<&str> =
            layout.iter().map(|(g, _)| g.as_str()).collect();
        let rest: Vec<(String, i32)> = Tracks::all(&self.daw, project.clone())
            .into_iter()
            .filter(|t| !placed.contains(t.guid.as_str()))
            .map(|t| (t.guid, t.folder_depth))
            .collect();
        layout.extend(rest);

        self.apply_layout(&layout)?;
        Ok(arranged)
    }

    /// Hide the DI captures that sit beside the tracks meant to be used —
    /// an amp, a mic — in the track panel and the mixer. A DI is kept to
    /// reamp from, not to mix: `DI` and `DI Pedalboard` beside `Amp 1` and
    /// `Amp 2` are two lanes of noise between the guitar you play back.
    /// Only beside a sibling (same folder) that is not a capture and has
    /// audio, or nested under such a track; a DI alone — a bass recorded
    /// only DI — is the track, and stays. Returns the names hidden.
    /// Idempotent.
    ///
    /// # Errors
    ///
    /// A backend call failed.
    pub fn hide_captures(&mut self) -> eyre::Result<Vec<String>> {
        let project = self.project.clone();
        let all = Tracks::all(&self.daw, project.clone());
        // Each track's parent folder (by index), `None` at the top.
        let mut parents: Vec<Option<usize>> = Vec::with_capacity(all.len());
        let mut open: Vec<usize> = Vec::new();
        for (i, track) in all.iter().enumerate() {
            parents.push(open.last().copied());
            if track.folder_depth > 0 {
                open.push(i);
            } else if track.folder_depth < 0 {
                for _ in 0..track.folder_depth.unsigned_abs() {
                    open.pop();
                }
            }
        }
        let mut hidden = Vec::new();
        for (i, track) in all.iter().enumerate() {
            let Some(parent) = parents.get(i).copied().flatten() else {
                continue;
            };
            if track.folder_depth > 0 || !super::is_capture(&track.name) || !track.visible_in_tcp {
                continue;
            }
            let real_beside = all.iter().enumerate().any(|(j, sibling)| {
                j != i
                    && parents.get(j).copied().flatten() == Some(parent)
                    && sibling.folder_depth <= 0
                    && !super::is_capture(&sibling.name)
                    && self.has_items(&sibling.guid)
            });
            // Or nested under one: the bus pass nests a DI under its
            // primary (`nest_secondary_mics`), which makes the track it
            // stands beside its parent.
            let real_above = all
                .get(parent)
                .is_some_and(|p| !super::is_capture(&p.name) && self.has_items(&p.guid));
            if real_beside || real_above {
                Tracks::set_visibility(
                    &self.daw,
                    project.clone(),
                    TrackRef::Guid(track.guid.clone()),
                    false,
                    false,
                )?;
                hidden.push(track.name.clone());
            }
        }
        Ok(hidden)
    }

    /// Hide the buses from the track panel — the template's (`MIX BUS`
    /// and its tree), a session's own (`Guitar A BUS`), and a headphone
    /// mix. They are wiring: summed in the mixer, where they stay, and
    /// rows of nothing in an arrangement. Returns the names hidden.
    /// Idempotent.
    ///
    /// # Errors
    ///
    /// A backend call failed.
    pub fn hide_buses(&mut self) -> eyre::Result<Vec<String>> {
        let project = self.project.clone();
        let mut hidden = Vec::new();
        for track in Tracks::all(&self.daw, project.clone()) {
            if track.visible_in_tcp && super::is_bus_track(&track.name) {
                Tracks::set_visibility(
                    &self.daw,
                    project.clone(),
                    TrackRef::Guid(track.guid.clone()),
                    false,
                    track.visible_in_mixer,
                )?;
                hidden.push(track.name);
            }
        }
        Ok(hidden)
    }

    /// Mute every drum trigger track and folder ([`super::is_trigger`]):
    /// its clicks are never heard, only fed — so each of its sends taps it
    /// before the fader (a post-fader one is made pre-fader), and what it
    /// keys or feeds (a gate's sidechain, a sampler) still hears it while
    /// the track stays muted. Returns the names muted. Idempotent.
    ///
    /// # Errors
    ///
    /// A backend call failed.
    pub fn mute_triggers(&mut self) -> eyre::Result<Vec<String>> {
        use daw_proto::{routing::SendMode, RouteRef};
        let muted = self.mute_where(super::is_trigger)?;
        for track in Tracks::all(&self.daw, self.project.clone()) {
            if !super::is_trigger(&track.name) {
                continue;
            }
            let source = TrackRef::Guid(track.guid.clone());
            for send in Routing::sends(&self.daw, self.project.clone(), source.clone()) {
                if send.send_mode == SendMode::PostFader {
                    Routing::set_send_mode(
                        &self.daw,
                        self.project.clone(),
                        source.clone(),
                        RouteRef::Index(send.index),
                        SendMode::PostFx,
                    )?;
                }
            }
        }
        Ok(muted)
    }

    /// The golden template's drum Process folder, inside the session's
    /// top-level Drums folder, for a session with one and no Process yet:
    ///
    /// ```text
    /// Drums                 -> DRUM BUS, as organized
    ///   Kick, Snare, Toms, Cymbals, Rooms …   each sends to every chain
    ///   Process             hidden in the track panel, in the mixer
    ///     Compress          Dry, Tight, Punch, Smash, Crunch
    ///     FX                Room Sim; Verb: Wood Room, Music Club,
    ///                       Stadium, RMX 16, Nonlin, Brick Wall
    /// ```
    ///
    /// One kit is not a folder of its own: the pieces are Drums' (a
    /// level is a folder only when it has more than one member). Each
    /// piece feeds the chains — not Drums, which sums Process, so a send
    /// from it would come back round. Every chain starts muted: a palette
    /// to pick from, not all of it under the kit at once. Process is
    /// mixing, not arranging: in the mixer, out of the track panel.
    /// Returns the tracks made; none when there is no Drums folder or a
    /// Process already.
    ///
    /// # Errors
    ///
    /// A backend call failed.
    pub fn add_drum_process(&mut self) -> eyre::Result<Vec<String>> {
        use super::TemplateTarget as _;
        let project = self.project.clone();
        let tracks = Tracks::all(&self.daw, project.clone());
        let Some(drums_at) = tracks.iter().position(|t| {
            t.parent_guid.is_none() && t.is_folder && t.name.trim().eq_ignore_ascii_case("drums")
        }) else {
            return Ok(Vec::new());
        };
        if tracks
            .iter()
            .any(|t| t.name.trim().eq_ignore_ascii_case(super::PROCESS_FOLDER))
        {
            return Ok(Vec::new());
        }
        let Some(drums) = tracks.get(drums_at) else {
            return Ok(Vec::new());
        };
        let pieces: Vec<String> = tracks
            .iter()
            .filter(|t| t.parent_guid.as_deref() == Some(drums.guid.as_str()))
            .map(|t| t.guid.clone())
            .collect();
        // The Drums block: the folder and everything to the track closing it.
        let mut running = 0_i32;
        let mut end = drums_at;
        for (i, t) in tracks.iter().enumerate().skip(drums_at) {
            running = running.saturating_add(t.folder_depth);
            end = i;
            if running <= 0 {
                break;
            }
        }
        // (name, folder depth change, the pieces send to it)
        let mut chains: Vec<(&str, i32, bool)> =
            vec![(super::PROCESS_FOLDER, 1, false), ("Compress", 1, false)];
        chains.extend(super::PROCESS_COMPRESS.iter().map(|n| (*n, 0, true)));
        if let Some(last) = chains.last_mut() {
            last.1 = -1;
        }
        chains.push(("FX", 1, false));
        chains.push(("Room Sim", 0, true));
        chains.push(("Verb", 1, false));
        chains.extend(super::PROCESS_VERBS.iter().map(|n| (*n, 0, true)));
        if let Some(last) = chains.last_mut() {
            // Closes Verb, FX, Process — and Drums.
            last.1 = -4;
        }
        let mut made = Vec::new();
        let mut process: Vec<(String, i32)> = Vec::new();
        for (name, depth, fed) in chains {
            let track = self.append_track(name)?;
            if fed {
                for piece in &pieces {
                    self.add_send(piece, &track)?;
                }
                Tracks::set_muted(
                    &self.daw,
                    project.clone(),
                    TrackRef::Guid(track.clone()),
                    true,
                )?;
            }
            Tracks::set_visibility(
                &self.daw,
                project.clone(),
                TrackRef::Guid(track.clone()),
                false,
                true,
            )?;
            process.push((track, depth));
            made.push(name.to_string());
        }
        // The whole project laid out again: Process after the last piece,
        // which no longer closes Drums — Process does.
        let mut layout: Vec<(String, i32)> = Vec::new();
        for (i, t) in tracks.iter().enumerate() {
            if i == end {
                layout.push((t.guid.clone(), t.folder_depth.saturating_add(1)));
                layout.extend(process.iter().cloned());
            } else {
                layout.push((t.guid.clone(), t.folder_depth));
            }
        }
        self.apply_layout(&layout)?;
        Ok(made)
    }

    /// Each drum piece's gate keyed from its trigger: a piece (`Kick`,
    /// `Snare`, `Tom 1` — a folder in the Drums folder) with a trigger in
    /// it or in its `Sum` (`Trig`, `T1 Trig`, or a `Trig` folder of them
    /// on a kit the toms could not be split on) gets a pre-FX send from it
    /// onto the piece's channels 3/4, which keys the gate on the piece
    /// (`session_daw::mix_chains`).
    ///
    /// The trigger is then unmuted — a muted track's sends are muted, in
    /// REAPER as here — and taken off its parent instead: alive to key,
    /// never heard under the kit. Returns the pieces keyed. Idempotent: a
    /// trigger already keying a piece is left alone.
    ///
    /// # Errors
    ///
    /// A backend call failed.
    pub fn key_drum_gates(&mut self) -> eyre::Result<Vec<String>> {
        let project = self.project.clone();
        let tracks = Tracks::all(&self.daw, project.clone());
        let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
            tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
        let in_drums = |t: &daw_proto::Track| {
            let mut at = t.parent_guid.as_deref();
            while let Some(guid) = at {
                let Some(parent) = by_guid.get(guid) else {
                    break;
                };
                if parent.parent_guid.is_none() && parent.name.trim().eq_ignore_ascii_case("drums")
                {
                    return true;
                }
                at = parent.parent_guid.as_deref();
            }
            false
        };
        let mut keyed = Vec::new();
        // A piece, not the Sum of one's mics (`apply_golden_kit`), where
        // its trigger goes.
        let piece_like = |t: &&daw_proto::Track| {
            t.is_folder
                && !super::is_trigger(&t.name)
                && !t.name.trim().eq_ignore_ascii_case("sum")
                && in_drums(t)
        };
        for piece in tracks.iter().filter(piece_like) {
            // Its triggers: in it, or in its Sum (`apply_golden_kit`).
            let sums: Vec<&str> = tracks
                .iter()
                .filter(|t| {
                    t.parent_guid.as_deref() == Some(piece.guid.as_str())
                        && t.name.trim().eq_ignore_ascii_case("sum")
                })
                .map(|t| t.guid.as_str())
                .collect();
            let triggers = tracks.iter().filter(|t| {
                t.parent_guid
                    .as_deref()
                    .is_some_and(|p| p == piece.guid || sums.contains(&p))
                    && super::is_trigger(&t.name)
            });
            for trigger in triggers {
                let source = TrackRef::Guid(trigger.guid.clone());
                let sends = Routing::sends(&self.daw, project.clone(), source.clone());
                // Keying something already: this piece, or the one its SUM
                // is in.
                if sends.iter().any(|s| s.dest_channels.start_channel >= 2) {
                    continue;
                }
                // The trigger and, for a folder of them, every one inside.
                for track in tracks.iter().filter(|t| {
                    t.guid == trigger.guid
                        || t.parent_guid.as_deref() == Some(trigger.guid.as_str())
                }) {
                    if track.muted {
                        Tracks::set_muted(
                            &self.daw,
                            project.clone(),
                            TrackRef::Guid(track.guid.clone()),
                            false,
                        )?;
                    }
                }
                Routing::set_parent_send_enabled(
                    &self.daw,
                    project.clone(),
                    source.clone(),
                    false,
                )?;
                // The piece, and its one-note tracks (`Fund`, the kick's
                // `Sub`): their gates open on the same hits.
                let one_notes = tracks.iter().filter(|t| {
                    !t.is_folder
                        && t.parent_guid
                            .as_deref()
                            .is_some_and(|p| p == piece.guid || sums.contains(&p))
                        && super::is_one_note(&t.name)
                });
                for keyed_track in std::iter::once(piece).chain(one_notes) {
                    self.key_from(&trigger.guid, &keyed_track.guid)?;
                }
                keyed.push(piece.name.clone());
            }
        }
        Ok(keyed)
    }

    /// The golden template's kit, made of the session's: each piece the
    /// template gives a `Sum` (`golden_session::shape::drum_kit` — Kick,
    /// Snare) gets one around its tracks, with what the template puts in
    /// it and beside it that the recording cannot have — a `Fund`, the
    /// kick's `Sub`, the piece's `Verb` bank. Toms become the template's
    /// `Tom N` pieces, each its mic, its trigger and a `Fund`, beside the
    /// toms' shared `Verb` bank.
    ///
    /// A bank's returns are fed by its piece — the `Sum`, or each tom —
    /// post-fader, and are muted but for the first: the one the song
    /// wants is a mute away. A mic the recording has no track for is not
    /// made: a template's `Out` with no kick-out mic would be a lane of
    /// nothing. Returns the tracks made. Idempotent: a piece that has its
    /// `Sum`, a tom already a `Tom N`, a bank already there is left.
    ///
    /// # Errors
    ///
    /// A backend call failed.
    pub fn apply_golden_kit(&mut self) -> eyre::Result<Vec<String>> {
        let golden = crate::golden_session::shape::drums();
        let tracks = Tracks::all(&self.daw, self.project.clone());
        let Some(drums) = tracks.iter().find(|t| {
            t.parent_guid.is_none()
                && t.is_folder
                && (t.name.trim().eq_ignore_ascii_case("drums")
                    || t.name.trim().eq_ignore_ascii_case("drum kit"))
        }) else {
            return Ok(Vec::new());
        };
        let drums = drums.guid.clone();
        let mut forest = Branch::forest(&tracks);
        let mut made = Vec::new();
        let mut feeds: Vec<(String, String)> = Vec::new();
        let mut quiet: Vec<String> = Vec::new();
        let mut gone: Vec<String> = Vec::new();
        for piece in &golden.children {
            let Some(at) = Branch::find_mut(
                &mut forest,
                &|b| b.is_folder() && b.name.trim().eq_ignore_ascii_case(&piece.name),
                &drums,
            ) else {
                continue;
            };
            if let Some(sum) = piece
                .children
                .iter()
                .find(|n| n.kind == crate::golden_session::Kind::Sum)
            {
                self.golden_piece(at, piece, sum, &mut made, &mut feeds, &mut quiet)?;
            } else if piece
                .children
                .iter()
                .any(|n| n.kind == crate::golden_session::Kind::Piece)
            {
                self.golden_toms(at, piece, &mut made, &mut feeds, &mut quiet, &mut gone)?;
            }
        }
        if made.is_empty() {
            return Ok(made);
        }
        let project = self.project.clone();
        for guid in &gone {
            Branch::remove(&mut forest, guid);
            Tracks::remove(&self.daw, project.clone(), TrackRef::Guid(guid.clone()))?;
        }
        self.apply_layout(&Branch::flatten(&forest))?;
        for (from, to) in &feeds {
            self.add_send(from, to)?;
        }
        for guid in &quiet {
            Tracks::set_muted(
                &self.daw,
                project.clone(),
                TrackRef::Guid(guid.clone()),
                true,
            )?;
        }
        Ok(made)
    }

    /// Mark what every track of the kit is, from the golden template's
    /// shape, matched by name: each piece, its Sum, the mics and trigger
    /// in it, and what the template made beside them. For a kit laid out
    /// before kinds were written; [`Self::apply_golden_kit`] marks what it
    /// makes as it goes. Returns how many tracks were marked.
    ///
    /// # Errors
    ///
    /// A backend call failed.
    pub fn mark_kit_kinds(&mut self) -> eyre::Result<usize> {
        let golden = crate::golden_session::shape::drums();
        let tracks = Tracks::all(&self.daw, self.project.clone());
        let Some(drums) = tracks.iter().find(|t| {
            t.parent_guid.is_none()
                && t.is_folder
                && (t.name.trim().eq_ignore_ascii_case("drums")
                    || t.name.trim().eq_ignore_ascii_case("drum kit"))
        }) else {
            return Ok(0);
        };
        let forest = Branch::forest(&tracks);
        let Some(kit) = forest.iter().find(|b| b.guid == drums.guid) else {
            return Ok(0);
        };
        self.mark_node(&kit.guid, &golden)?;
        let mut marked = 1_usize;
        self.mark_like(&golden.children, &kit.children, &mut marked)?;
        Ok(marked)
    }

    /// `tracks` marked by the `shape` nodes of the same names, and what is
    /// in them by the nodes' children; a Sum's tracks as what they record.
    fn mark_like(
        &self,
        shape: &[crate::golden_session::shape::Node],
        tracks: &[Branch],
        marked: &mut usize,
    ) -> eyre::Result<()> {
        use crate::golden_session::Kind;
        for track in tracks {
            let Some(node) = shape
                .iter()
                .find(|n| n.name.trim().eq_ignore_ascii_case(track.name.trim()))
            else {
                continue;
            };
            self.mark_node(&track.guid, node)?;
            *marked = marked.saturating_add(1);
            if node.kind == Kind::Sum {
                self.mark_recorded(&track.children)?;
                *marked =
                    marked.saturating_add(track.children.iter().filter(|t| !t.is_folder()).count());
            }
            self.mark_like(&node.children, &track.children, marked)?;
        }
        Ok(())
    }

    /// One summed piece ([`Self::apply_golden_kit`]): its tracks under a
    /// `Sum`, the template's `Fund` in it, the tracks beside it made.
    fn golden_piece(
        &mut self,
        at: &mut Branch,
        piece: &crate::golden_session::shape::Node,
        sum: &crate::golden_session::shape::Node,
        made: &mut Vec<String>,
        feeds: &mut Vec<(String, String)>,
        quiet: &mut Vec<String>,
    ) -> eyre::Result<()> {
        use super::TemplateTarget as _;
        if at
            .children
            .iter()
            .any(|c| c.name.trim().eq_ignore_ascii_case(&sum.name))
        {
            return Ok(());
        }
        let guid = self.append_track(&sum.name)?;
        made.push(format!("{}/{}", at.name, sum.name));
        self.mark_node(&guid, sum)?;
        self.mark_node(&at.guid, piece)?;
        let mut summed = Branch::new(guid, &sum.name);
        summed.children = std::mem::take(&mut at.children);
        self.mark_recorded(&summed.children)?;
        for one in &sum.children {
            self.add_made(
                one,
                &mut summed,
                &format!("{}/{}", at.name, sum.name),
                made,
                feeds,
            )?;
        }
        let from = summed.guid.clone();
        at.children.push(summed);
        for beside in piece
            .children
            .iter()
            .filter(|n| n.kind != crate::golden_session::Kind::Sum)
        {
            let branch = self.make(beside, &at.name, made)?;
            branch.wire_bank(std::slice::from_ref(&from), feeds, quiet);
            // The kick's Sub: fed by the Sum it sits beside.
            if !branch.is_folder() && super::is_one_note(&branch.name) {
                feeds.push((from.clone(), branch.guid.clone()));
            }
            at.children.push(branch);
        }
        Ok(())
    }

    /// The toms ([`Self::apply_golden_kit`]): each `TN` with its `TN Trig`
    /// made the template's `Tom N` with a `Fund`, the triggers' emptied
    /// folder gone, the shared `Verb` bank beside them, fed by every tom.
    fn golden_toms(
        &mut self,
        at: &mut Branch,
        toms: &crate::golden_session::shape::Node,
        made: &mut Vec<String>,
        feeds: &mut Vec<(String, String)>,
        quiet: &mut Vec<String>,
        gone: &mut Vec<String>,
    ) -> eyre::Result<()> {
        use super::TemplateTarget as _;
        let mut pieces: Vec<String> = Vec::new();
        let mut built: Vec<Branch> = Vec::new();
        for tom in toms
            .children
            .iter()
            .filter(|n| n.kind == crate::golden_session::Kind::Piece)
        {
            if at
                .children
                .iter()
                .any(|c| c.name.trim().eq_ignore_ascii_case(&tom.name))
            {
                continue;
            }
            // The template's mic and trigger names for this tom, `T1` and
            // `T1 Trig`: what the session's are found by.
            let names: Vec<(&str, crate::golden_session::Kind)> = tom
                .children
                .iter()
                .map(|n| (n.name.as_str(), n.kind))
                .collect();
            let Some((mic, _)) = names
                .iter()
                .find(|(_, k)| *k == crate::golden_session::Kind::Source)
            else {
                continue;
            };
            let Some(mic) = Branch::take(&mut at.children, mic) else {
                continue;
            };
            let guid = self.append_track(&tom.name)?;
            made.push(format!("{}/{}", at.name, tom.name));
            self.mark_node(&guid, tom)?;
            let mut piece = Branch::new(guid, &tom.name);
            piece.children.push(mic);
            let trig = names
                .iter()
                .find(|(_, k)| *k == crate::golden_session::Kind::Trigger)
                .and_then(|(trig, _)| Branch::take(&mut at.children, trig));
            piece.children.extend(trig);
            self.mark_recorded(&piece.children)?;
            for one in &tom.children {
                self.add_made(
                    one,
                    &mut piece,
                    &format!("{}/{}", at.name, tom.name),
                    made,
                    feeds,
                )?;
            }
            pieces.push(piece.guid.clone());
            built.push(piece);
        }
        if built.is_empty() {
            return Ok(());
        }
        // The folders the triggers came out of, now empty.
        at.children.retain(|c| {
            let empty = c.was_folder && c.children.is_empty();
            if empty {
                gone.push(c.guid.clone());
            }
            !empty
        });
        let rest = std::mem::take(&mut at.children);
        at.children = built;
        at.children.extend(rest);
        for beside in toms
            .children
            .iter()
            .filter(|n| n.kind != crate::golden_session::Kind::Piece)
        {
            if at
                .children
                .iter()
                .any(|c| c.name.trim().eq_ignore_ascii_case(&beside.name))
            {
                continue;
            }
            let branch = self.make(beside, &at.name, made)?;
            branch.wire_bank(&pieces, feeds, quiet);
            at.children.push(branch);
        }
        Ok(())
    }

    /// The template's vocal returns (`golden_session::shape::vox_fx` —
    /// Delay: Slap, Short, Long, Throw; Verb: Room, Short, Long, Moment,
    /// Throw; Wide; Pitch: Oct+, Oct-) at the end of the top-level Vocals
    /// folder, every lead in it — each track or folder directly in
    /// Vocals — sending to every return, post-fader: whichever lead is
    /// up goes into the same delays and verbs.
    ///
    /// The returns in [`super::VOX_FX_START`] start on at their level;
    /// the rest start muted at [`super::VOX_FX_QUIET_DB`] — a mute away,
    /// and not a full-scale wet return when one is unmuted. Returns the
    /// tracks made; none when Vocals has its `Vox FX` already.
    ///
    /// # Errors
    ///
    /// A backend call failed.
    pub fn apply_vox_fx(&mut self) -> eyre::Result<Vec<String>> {
        use super::TemplateTarget as _;
        let golden = crate::golden_session::shape::vox_fx();
        let tracks = Tracks::all(&self.daw, self.project.clone());
        let mut forest = Branch::forest(&tracks);
        let Some(vocals) = forest
            .iter_mut()
            .find(|b| b.is_folder() && b.name.trim().eq_ignore_ascii_case("vocals"))
        else {
            return Ok(Vec::new());
        };
        if vocals
            .children
            .iter()
            .any(|c| c.name.trim().eq_ignore_ascii_case(&golden.name))
        {
            return Ok(Vec::new());
        }
        let leads: Vec<String> = vocals
            .children
            .iter()
            .filter(|c| !c.name.trim().to_ascii_uppercase().starts_with("VOX "))
            .map(|c| c.guid.clone())
            .collect();
        let mut made = Vec::new();
        let vocals_name = vocals.name.clone();
        let branch = self.make(&golden, &vocals_name, &mut made)?;
        let mut returns: Vec<(String, String, String)> = Vec::new();
        branch.leaves_with_parent(&mut returns);
        vocals.children.push(branch);
        self.apply_layout(&Branch::flatten(&forest))?;
        let project = self.project.clone();
        for (guid, parent, name) in &returns {
            for lead in &leads {
                self.add_send(lead, guid)?;
            }
            let start = super::VOX_FX_START
                .iter()
                .find(|(p, n, _)| p.eq_ignore_ascii_case(parent) && n.eq_ignore_ascii_case(name));
            let level = start.map_or(super::VOX_FX_QUIET_DB, |(_, _, db)| *db);
            Tracks::set_volume(
                &self.daw,
                project.clone(),
                TrackRef::Guid(guid.clone()),
                10_f64.powf(level / 20.0),
            )?;
            if start.is_none() {
                Tracks::set_muted(
                    &self.daw,
                    project.clone(),
                    TrackRef::Guid(guid.clone()),
                    true,
                )?;
            }
        }
        Ok(made)
    }

    /// What a piece has that a recording cannot, made into `into` (a
    /// Sum, a tom): its `Sample` beside its `Trig` — unless it has one —
    /// and its `Fund`, fed by the piece's mics, not the Sum it sits in,
    /// whose sum would come round through it. A mic or trigger the
    /// template names is the recording's, and is not made.
    fn add_made(
        &mut self,
        one: &crate::golden_session::shape::Node,
        into: &mut Branch,
        at: &str,
        made: &mut Vec<String>,
        feeds: &mut Vec<(String, String)>,
    ) -> eyre::Result<()> {
        use crate::golden_session::Kind;
        match one.kind {
            Kind::Sample => {
                if into.children.iter().any(|c| super::is_sample(&c.name)) {
                    return Ok(());
                }
                let sample = self.make(one, at, made)?;
                // Beside the trigger it plays from.
                let after = into
                    .children
                    .iter()
                    .rposition(|c| super::is_trigger(&c.name))
                    .map_or(into.children.len(), |i| i.saturating_add(1));
                into.children.insert(after, sample);
            }
            Kind::Fundamental => {
                let fund = self.make(one, at, made)?;
                for mic in into.children.iter().filter(|c| c.is_source()) {
                    feeds.push((mic.guid.clone(), fund.guid.clone()));
                }
                into.children.push(fund);
            }
            _ => {}
        }
        Ok(())
    }

    /// `node` and everything in it made as tracks, under `at` (a path,
    /// for what is returned in `made`).
    fn make(
        &mut self,
        node: &crate::golden_session::shape::Node,
        at: &str,
        made: &mut Vec<String>,
    ) -> eyre::Result<Branch> {
        use super::TemplateTarget as _;
        let guid = self.append_track(&node.name)?;
        self.mark_node(&guid, node)?;
        let path = format!("{at}/{}", node.name);
        made.push(path.clone());
        let mut branch = Branch::new(guid, &node.name);
        for child in &node.children {
            branch.children.push(self.make(child, &path, made)?);
        }
        Ok(branch)
    }

    /// A pre-FX send from `trigger` onto `keyed`'s channels 3/4: its
    /// sidechain, which keys the gate on it.
    fn key_from(&self, trigger: &str, keyed: &str) -> eyre::Result<()> {
        use daw_proto::{routing::SendMode, RouteRef};
        let source = TrackRef::Guid(trigger.to_owned());
        let Some(index) = Routing::add_send(
            &self.daw,
            self.project.clone(),
            source.clone(),
            TrackRef::Guid(keyed.to_owned()),
        ) else {
            return Ok(());
        };
        Routing::set_send_mode(
            &self.daw,
            self.project.clone(),
            source.clone(),
            RouteRef::Index(index),
            SendMode::PreFx,
        )?;
        let send = daw_proto::RouteLocation {
            track: source,
            route_type: daw_proto::RouteType::Send,
            route: RouteRef::Index(index),
        };
        Routing::set_dest_channels(&self.daw, self.project.clone(), send, 2, 2)?;
        Ok(())
    }

    /// Mute every track whose name `matches` and is not muted yet.
    /// Returns the names muted.
    fn mute_where(&self, matches: impl Fn(&str) -> bool) -> eyre::Result<Vec<String>> {
        let project = self.project.clone();
        let mut muted = Vec::new();
        for track in Tracks::all(&self.daw, project.clone()) {
            if !track.muted && matches(&track.name) {
                Tracks::set_muted(
                    &self.daw,
                    project.clone(),
                    TrackRef::Guid(track.guid.clone()),
                    true,
                )?;
                muted.push(track.name);
            }
        }
        Ok(muted)
    }

    /// The folder `name` a group's tracks go in: one of the project's with
    /// no media and not already in use, or a new one.
    fn group_folder(
        &self,
        name: &str,
        in_use: &mut std::collections::HashSet<String>,
        arranged: &mut Arranged,
    ) -> eyre::Result<String> {
        let project = self.project.clone();
        let existing = Tracks::all(&self.daw, project.clone())
            .into_iter()
            .find(|t| {
                t.name.trim().eq_ignore_ascii_case(name.trim())
                    && !self.has_items(&t.guid)
                    && !in_use.contains(&t.guid)
            })
            .map(|t| t.guid);
        let guid = if let Some(g) = existing {
            g
        } else {
            arranged.created.push(name.to_string());
            Tracks::add(&self.daw, project, name, None)?
        };
        in_use.insert(guid.clone());
        Ok(guid)
    }

    /// The top-level folders kept whole by [`Self::arrange_into_groups`]:
    /// each a folder with no media of its own that classified (with its
    /// folders as context) and is not a bus, Keyflow, UNSORTED or reference
    /// material — with everything inside it, as `(guid, depth change)`.
    fn kept_folders(
        &self,
        all: &[daw::service::Track],
        classified: &std::collections::HashSet<String>,
        rehearsal: &[String],
        bounces: &[String],
    ) -> Vec<Vec<(String, i32)>> {
        let mut units = Vec::new();
        let mut running = 0i32;
        let mut i = 0;
        while let Some(track) = all.get(i) {
            let opens = running == 0 && track.folder_depth > 0;
            let name = track.name.trim();
            let keep = opens
                && classified.contains(&track.guid)
                && !rehearsal.contains(&track.guid)
                && !bounces.contains(&track.guid)
                && !crate::buses::is_bus_name(name)
                && !name.eq_ignore_ascii_case("Keyflow")
                && !name.eq_ignore_ascii_case(super::UNSORTED_FOLDER)
                && !name.eq_ignore_ascii_case(super::REFERENCES_FOLDER)
                && !self.has_items(&track.guid);
            if !keep {
                running = running.saturating_add(track.folder_depth);
                i = i.saturating_add(1);
                continue;
            }
            // The folder and everything until the depth comes back to the top.
            let mut unit = Vec::new();
            let mut depth = 0i32;
            while let Some(t) = all.get(i) {
                unit.push((t.guid.clone(), t.folder_depth));
                depth = depth.saturating_add(t.folder_depth);
                i = i.saturating_add(1);
                if depth <= 0 {
                    break;
                }
            }
            // Reference material inside it (a `DRUM REF` in the kit) is laid
            // out in REFERENCES, so it comes out — the track before it
            // taking over the folders it closed. A reference that is itself
            // a folder takes too much with it: the block is not kept.
            let reference = |g: &String| rehearsal.contains(g) || bounces.contains(g);
            if unit.iter().any(|(g, depth)| reference(g) && *depth > 0) {
                continue;
            }
            let mut kept: Vec<(String, i32)> = Vec::with_capacity(unit.len());
            for (guid, depth) in unit {
                if reference(&guid) {
                    if let Some(last) = kept.last_mut() {
                        last.1 = last.1.saturating_add(depth);
                    }
                } else {
                    kept.push((guid, depth));
                }
            }
            units.push(kept);
        }
        units
    }

    /// The REFERENCES folder's part of a layout: the folder (reused if the
    /// project has one), REHEARSAL inside it holding `rehearsal`, then
    /// `bounces` — each a `(guid, depth change)`. Empty when both are.
    fn references_layout(
        &self,
        rehearsal: &[String],
        bounces: &[String],
        in_use: &mut std::collections::HashSet<String>,
        arranged: &mut Arranged,
    ) -> eyre::Result<Vec<(String, i32)>> {
        if rehearsal.is_empty() && bounces.is_empty() {
            return Ok(Vec::new());
        }
        let made = arranged.created.len();
        let references = self.folder_named(super::REFERENCES_FOLDER, arranged)?;
        // Made here, it starts muted: a reference (DRUM REF, a mix print)
        // is listened against on purpose, never under the song.
        if arranged.created.len() > made {
            let _ = Tracks::set_muted(
                &self.daw,
                self.project.clone(),
                TrackRef::Guid(references.clone()),
                true,
            );
        }
        in_use.insert(references.clone());
        let mut layout = vec![(references, 1)];
        if !rehearsal.is_empty() {
            layout.extend(self.rehearsal_layout(rehearsal, in_use, arranged)?);
        }
        layout.extend(bounces.iter().map(|g| (g.clone(), 0)));
        // The last of it closes REFERENCES.
        if let Some(last) = layout.last_mut() {
            last.1 = last.1.saturating_sub(1);
        }
        Ok(layout)
    }

    /// The REHEARSAL folder's part of a layout, closed: the rehearsal filed
    /// by instrument the way the session is — Drums (`Kick - Song`, `OH L -
    /// Song`), Bass, Guitars (`Lead`, `Rhythm`), Vocals — in the template's
    /// order, then what names no instrument (the room track, a stem).
    fn rehearsal_layout(
        &self,
        rehearsal: &[String],
        in_use: &mut std::collections::HashSet<String>,
        arranged: &mut Arranged,
    ) -> eyre::Result<Vec<(String, i32)>> {
        let all = Tracks::all(&self.daw, self.project.clone());
        let name_of = |guid: &str| {
            all.iter()
                .find(|t| t.guid == guid)
                .map(|t| t.name.clone())
                .unwrap_or_default()
        };
        let mut by_group: std::collections::BTreeMap<usize, (String, Vec<String>)> =
            std::collections::BTreeMap::new();
        let mut loose: Vec<String> = Vec::new();
        for guid in rehearsal {
            match rehearsal_group(&name_of(guid)) {
                Some(group) => {
                    let at = INSTRUMENT_ORDER
                        .iter()
                        .position(|o| o.eq_ignore_ascii_case(&group))
                        .unwrap_or(INSTRUMENT_ORDER.len());
                    by_group
                        .entry(at)
                        .or_insert_with(|| (group, Vec::new()))
                        .1
                        .push(guid.clone());
                }
                None => loose.push(guid.clone()),
            }
        }
        let made = arranged.created.len();
        let folder = self.folder_named(super::REHEARSAL_FOLDER, arranged)?;
        // Made here, it starts muted: the rehearsal is something to listen
        // back to on purpose, never under the song as it plays.
        if arranged.created.len() > made {
            let _ = Tracks::set_muted(
                &self.daw,
                self.project.clone(),
                TrackRef::Guid(folder.clone()),
                true,
            );
        }
        in_use.insert(folder.clone());
        let mut layout: Vec<(String, i32)> = vec![(folder, 1)];
        for (_, (group, tracks)) in by_group {
            layout.push((self.group_folder(&group, in_use, arranged)?, 1));
            layout.extend(tracks.into_iter().map(|g| (g, 0)));
            if let Some(last) = layout.last_mut() {
                last.1 = -1;
            }
        }
        layout.extend(loose.into_iter().map(|g| (g, 0)));
        // The last of it closes REHEARSAL.
        if let Some(last) = layout.last_mut() {
            last.1 = last.1.saturating_sub(1);
        }
        Ok(layout)
    }

    /// A folder track named `name` with no media of its own — the
    /// project's, or a new one.
    fn folder_named(&self, name: &str, arranged: &mut Arranged) -> eyre::Result<String> {
        let project = self.project.clone();
        let existing = Tracks::all(&self.daw, project.clone())
            .into_iter()
            .find(|t| t.name.trim().eq_ignore_ascii_case(name) && !self.has_items(&t.guid))
            .map(|t| t.guid);
        if let Some(guid) = existing {
            return Ok(guid);
        }
        arranged.created.push(name.to_string());
        Ok(Tracks::add(&self.daw, project, name, None)?)
    }

    /// Reorder the whole project to `layout` and set every depth from it.
    fn apply_layout(&mut self, layout: &[(String, i32)]) -> eyre::Result<()> {
        let project = self.project.clone();
        for (i, (guid, _)) in layout.iter().enumerate() {
            let current = Tracks::all(&self.daw, project.clone())
                .iter()
                .position(|t| &t.guid == guid);
            if current == Some(i) {
                continue;
            }
            Tracks::clear_selection(&self.daw, project.clone())?;
            Tracks::set_selected(
                &self.daw,
                project.clone(),
                TrackRef::Guid(guid.clone()),
                true,
            )?;
            Tracks::reorder_selected(
                &self.daw,
                project.clone(),
                u32::try_from(i).unwrap_or(u32::MAX),
                ReorderTracksBehavior::Normal,
            )?;
        }
        Tracks::clear_selection(&self.daw, project.clone())?;
        // The order is right; now say exactly where every folder opens
        // and closes, whatever the moves did to it on the way.
        for (guid, depth) in layout {
            Tracks::set_folder_depth(
                &self.daw,
                project.clone(),
                TrackRef::Guid(guid.clone()),
                *depth,
            )?;
        }
        Ok(())
    }

    /// Put the top level in the template's order: `first` (by folder
    /// name, in that order), then everything else as it stands, then
    /// `last`. Each top-level track moves with everything inside it.
    ///
    /// # Errors
    ///
    /// A backend call failed.
    pub fn order_top_level(&mut self, first: &[&str], last: &[&str]) -> eyre::Result<()> {
        let project = self.project.clone();
        let all = Tracks::all(&self.daw, project.clone());
        // Top-level blocks: a track at depth 0 and everything it opens.
        let mut blocks: Vec<Vec<(String, i32)>> = Vec::new();
        let mut running = 0i32;
        for t in &all {
            if running == 0 {
                blocks.push(Vec::new());
            }
            if let Some(block) = blocks.last_mut() {
                block.push((t.guid.clone(), t.folder_depth));
            }
            running += t.folder_depth;
        }
        let name_of = |block: &Vec<(String, i32)>| {
            all.iter()
                .find(|t| t.guid == block[0].0)
                .map(|t| t.name.trim().to_owned())
                .unwrap_or_default()
        };
        // The instruments between, by group in the template's order — an
        // `Electric GTR` and a `GTR E` are both the guitars'.
        let group_of: std::collections::HashMap<String, String> = super::classified_paths(self)
            .into_iter()
            .filter_map(|e| Some((e.track, e.path.first()?.clone())))
            .collect();
        let rank = |block: &Vec<(String, i32)>| {
            let name = name_of(block);
            if let Some(i) = first.iter().position(|n| n.eq_ignore_ascii_case(&name)) {
                (0, i)
            } else if let Some(i) = last.iter().position(|n| n.eq_ignore_ascii_case(&name)) {
                (2, i)
            } else {
                // By its own name when the order names it (Process, beside
                // the kit), else by the group it classifies into.
                let group = block.first().and_then(|(g, _)| group_of.get(g));
                let place = |of: &str| {
                    INSTRUMENT_ORDER
                        .iter()
                        .position(|o| o.eq_ignore_ascii_case(of))
                };
                let at = place(&name)
                    .or_else(|| group.and_then(|g| place(g)))
                    .unwrap_or(INSTRUMENT_ORDER.len());
                (1, at)
            }
        };
        let mut ordered = blocks.clone();
        ordered.sort_by_key(|b| rank(b)); // stable: the middle keeps its order
        if ordered == blocks {
            return Ok(());
        }
        let layout: Vec<(String, i32)> = ordered.into_iter().flatten().collect();
        self.apply_layout(&layout)
    }

    /// Hide the named folder track from the track panel (the mixer keeps
    /// it). Only the folder itself: a panel hides what a hidden folder
    /// holds, so unhiding this one track brings the whole tree back. For
    /// the MIX BUS — routing, not something to look at while playing.
    ///
    /// # Errors
    ///
    /// A backend call failed.
    pub fn hide_in_tcp(&mut self, folder: &str) -> eyre::Result<()> {
        let project = self.project.clone();
        if let Some(track) = Tracks::all(&self.daw, project.clone())
            .into_iter()
            .find(|t| t.name.trim().eq_ignore_ascii_case(folder))
        {
            Tracks::set_visibility(&self.daw, project, TrackRef::Guid(track.guid), false, true)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod rehearsal_group_tests {
    use super::rehearsal_group;

    /// A rehearsal track goes in its instrument's folder by its part — the
    /// name before ` - Song`; a band's Lead and Rhythm are its guitars; a
    /// nameless track or a bare stem names none.
    #[test]
    fn a_rehearsal_track_is_filed_by_its_part() {
        let group = |name: &str| rehearsal_group(name);
        assert_eq!(group("Kick - The Plague").as_deref(), Some("Drums"));
        assert_eq!(group("OH L - The Plague").as_deref(), Some("Drums"));
        assert_eq!(group("Bass - The Plague").as_deref(), Some("Bass"));
        assert_eq!(group("Lead - The Plague").as_deref(), Some("Guitars"));
        assert_eq!(group("Rhythm - The Plague").as_deref(), Some("Guitars"));
        assert_eq!(group("Vocals - The Plague").as_deref(), Some("Vocals"));
        assert_eq!(group(""), None);
        assert_eq!(group(" - stem"), None);
    }
}

/// A track and the tracks in it: the project as a folder tree, for a
/// restructure built on the tree and written back as one layout
/// ([`Branch::flatten`]).
#[derive(Clone, Debug)]
struct Branch {
    guid: String,
    name: String,
    /// Whether it was a folder before it was changed: a folder emptied
    /// by a restructure is gone, an empty track is a track.
    was_folder: bool,
    children: Vec<Self>,
}

impl Branch {
    fn new(guid: String, name: &str) -> Self {
        Self {
            guid,
            name: name.to_owned(),
            was_folder: false,
            children: Vec::new(),
        }
    }

    const fn is_folder(&self) -> bool {
        !self.children.is_empty()
    }

    /// A mic or a direct source: a track, not a trigger, not one-note.
    fn is_source(&self) -> bool {
        !self.is_folder()
            && !super::is_trigger(&self.name)
            && !super::is_sample(&self.name)
            && !super::is_one_note(&self.name)
    }

    /// The project's tracks as a tree, in order.
    fn forest(tracks: &[daw_proto::Track]) -> Vec<Self> {
        fn grow(parent: Option<&str>, tracks: &[daw_proto::Track]) -> Vec<Branch> {
            tracks
                .iter()
                .filter(|t| t.parent_guid.as_deref() == parent)
                .map(|t| Branch {
                    guid: t.guid.clone(),
                    name: t.name.clone(),
                    was_folder: t.is_folder,
                    children: grow(Some(t.guid.as_str()), tracks),
                })
                .collect()
        }
        grow(None, tracks)
    }

    /// The first branch under the one with guid `within` that `matches`.
    fn find_mut<'a>(
        forest: &'a mut [Self],
        matches: &dyn Fn(&Self) -> bool,
        within: &str,
    ) -> Option<&'a mut Self> {
        fn inside<'a>(
            branches: &'a mut [Branch],
            matches: &dyn Fn(&Branch) -> bool,
        ) -> Option<&'a mut Branch> {
            for branch in branches {
                if matches(branch) {
                    return Some(branch);
                }
                if let Some(found) = inside(&mut branch.children, matches) {
                    return Some(found);
                }
            }
            None
        }
        fn root<'a>(branches: &'a mut [Branch], guid: &str) -> Option<&'a mut Branch> {
            for branch in branches {
                if branch.guid == guid {
                    return Some(branch);
                }
                if let Some(found) = root(&mut branch.children, guid) {
                    return Some(found);
                }
            }
            None
        }
        inside(&mut root(forest, within)?.children, matches)
    }

    /// Take the branch named `name` out of `branches`, wherever in them.
    fn take(branches: &mut Vec<Self>, name: &str) -> Option<Self> {
        if let Some(at) = branches
            .iter()
            .position(|b| b.name.trim().eq_ignore_ascii_case(name))
        {
            return Some(branches.remove(at));
        }
        branches
            .iter_mut()
            .find_map(|b| Self::take(&mut b.children, name))
    }

    /// Drop the branch `guid` from the tree.
    fn remove(forest: &mut Vec<Self>, guid: &str) {
        forest.retain(|b| b.guid != guid);
        for branch in forest {
            Self::remove(&mut branch.children, guid);
        }
    }

    /// The tree as the project's order: each track and its folder depth
    /// change.
    fn flatten(forest: &[Self]) -> Vec<(String, i32)> {
        fn walk(branches: &[Branch], level: i32, out: &mut Vec<(String, i32)>) {
            for branch in branches {
                out.push((branch.guid.clone(), level));
                walk(&branch.children, level.saturating_add(1), out);
            }
        }
        let mut levels = Vec::new();
        walk(forest, 0, &mut levels);
        let next: Vec<i32> = levels.iter().skip(1).map(|(_, l)| *l).chain([0]).collect();
        levels
            .iter()
            .zip(next)
            .map(|((guid, level), next)| (guid.clone(), next.saturating_sub(*level)))
            .collect()
    }

    /// Feed this bank's returns from `from`, post-fader, all but the
    /// first muted; a branch that is no bank (the kick's `Sub`) is fed
    /// nothing.
    fn wire_bank(
        &self,
        from: &[String],
        feeds: &mut Vec<(String, String)>,
        quiet: &mut Vec<String>,
    ) {
        if !self.is_folder() {
            return;
        }
        for (i, ret) in self.children.iter().enumerate() {
            for source in from {
                feeds.push((source.clone(), ret.guid.clone()));
            }
            if i > 0 {
                quiet.push(ret.guid.clone());
            }
        }
    }

    /// Every track with nothing in it, below this one: its guid, its
    /// folder's name and its own.
    fn leaves_with_parent(&self, out: &mut Vec<(String, String, String)>) {
        for child in &self.children {
            if child.is_folder() {
                child.leaves_with_parent(out);
            } else {
                out.push((child.guid.clone(), self.name.clone(), child.name.clone()));
            }
        }
    }
}
