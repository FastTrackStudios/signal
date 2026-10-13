//! Offline (no live REAPER needed) driver for the Keyflow marker/region
//! pipeline, against an `.RPP` loaded from disk via `dawfile-reaper`.
//!
//! `convert_markers_to_session_format`, `normalize_section_regions`, and the
//! rest of `keyflow::actions` are written against `daw::service::{Projects,
//! Markers, Regions}` — stateless-singleton service traits with a live
//! REAPER instance as the only backend today. [`OfflineDaw`] is a second
//! backend for the same traits, holding one in-memory `ReaperProject`
//! instead of talking to a running REAPER, so a whole album of `.RPP` files
//! can be batch-organized without opening any of them.
//!
//! [`auto_organize_regions`] is the entry point: it runs the same
//! convert → normalize → song-region → lane-normalize → hide-stray sequence
//! `run_action` runs for `KeyflowAction::ConvertMarkersToSessionFormat` live,
//! then hands the mutated project back.

use std::cell::RefCell;

use daw::service::{DawError, DawResult, Marker, Position, ProjectContext, ProjectInfo, Region};
use daw::service::{Markers, Projects, Regions};
use dawfile_reaper::types::ReaperProject;
use dawfile_reaper::types::marker_region::MarkerRegion;
use dawfile_reaper::types::project::RulerLane;
use session_proto::ruler_lanes::CoreLane;

use super::actions::{
    convert_markers_to_session_format, ensure_core_lanes, hide_stray_lanes, normalize_marker_lanes,
};

/// A REAPER-shaped GUID for a freshly-created marker/region.
///
/// `dawfile_reaper`'s line tokenizer treats runs of whitespace as a single
/// separator, so an *empty* guid field (as opposed to a real `{...}` token)
/// disappears entirely on round-trip instead of parsing as a blank token —
/// every field after it (`additional`, `lane`) then silently shifts left by
/// one. A region added with an empty guid loses its lane on the very next
/// parse, which is exactly what let `ensure_song_region`'s "does one already
/// exist" check miss its own region and add another one on every rerun.
fn new_guid() -> String {
    format!("{{{}}}", uuid::Uuid::new_v4().to_string().to_uppercase())
}

/// One in-memory `.RPP` project, playing the `daw::service` backend role
/// that a live REAPER instance plays elsewhere.
///
/// Interior mutability because the service traits take `&self` (mirroring
/// the stateless-singleton `Reaper` backend, which resolves everything
/// through FFI calls instead of owned state).
pub struct OfflineDaw {
    project: RefCell<ReaperProject>,
    /// Best-effort project name for the synthesized `ProjectInfo` and the
    /// whole-song region's default name — usually the `.RPP` file stem.
    name: String,
}

impl OfflineDaw {
    #[must_use]
    pub fn new(project: ReaperProject, name: impl Into<String>) -> Self {
        Self {
            project: RefCell::new(project),
            name: name.into(),
        }
    }

    #[must_use]
    pub fn into_inner(self) -> ReaperProject {
        self.project.into_inner()
    }

    /// The id one more than every existing marker/region id, so a fresh one
    /// never collides — REAPER shares one id space between markers and
    /// regions (a region is two `MARKER` lines with the same id).
    fn next_id(&self) -> i32 {
        self.project
            .borrow()
            .markers_regions
            .all
            .iter()
            .map(|m| m.id)
            .max()
            .map_or(0, |max| max.saturating_add(1))
    }

    /// Whether a marker or region already carries `guid` — they share one
    /// `MARKER` list, and a save matches entries by it.
    fn guid_taken(&self, guid: &str) -> bool {
        self.project
            .borrow()
            .markers_regions
            .all
            .iter()
            .any(|m| m.guid == guid)
    }

    fn with_entry_mut<T>(&self, id: u32, f: impl FnOnce(&mut MarkerRegion) -> T) -> DawResult<T> {
        let id = id.cast_signed();
        let mut project = self.project.borrow_mut();
        let entry = project
            .markers_regions
            .all
            .iter_mut()
            .find(|m| m.id == id)
            .ok_or_else(|| DawError::not_found("marker/region", &id.to_string()))?;
        let result = f(entry);
        // `all` is the source of truth the serializer reads; `markers`/
        // `regions` are just filtered views of it. Rebuild them so a
        // rename/recolor/relane is visible through either one too.
        let all = project.markers_regions.all.clone();
        project.markers_regions.markers = all.iter().filter(|m| m.is_marker()).cloned().collect();
        project.markers_regions.regions = all.into_iter().filter(MarkerRegion::is_region).collect();
        Ok(result)
    }

    /// Set the flags on the ruler lane at `index` (1-based, REAPER's own
    /// `RULERLANE` chunk numbering), creating it if it doesn't exist yet.
    fn set_lane_flags(project: &mut ReaperProject, index: i32, flags: i32) {
        match project.ruler_lanes.iter_mut().find(|l| l.index == index) {
            Some(lane) => lane.flags = flags,
            None => project.ruler_lanes.push(RulerLane {
                index,
                flags,
                name: String::new(),
                color: 0,
                extra: -1,
            }),
        }
    }

    /// Set the name on the ruler lane at `index` (1-based), creating it if
    /// it doesn't exist yet.
    fn set_lane_name(project: &mut ReaperProject, index: i32, name: &str) {
        match project.ruler_lanes.iter_mut().find(|l| l.index == index) {
            Some(lane) => lane.name = name.to_string(),
            None => project.ruler_lanes.push(RulerLane {
                index,
                flags: 0,
                name: name.to_string(),
                color: 0,
                extra: -1,
            }),
        }
    }
}

fn marker_region_to_marker(entry: &MarkerRegion) -> Marker {
    Marker {
        id: Some(entry.id.cast_unsigned()),
        position: Position::from_time(daw::service::PositionInSeconds::from_seconds(
            entry.position,
        )),
        name: entry.name.clone(),
        color: Some(entry.color.cast_unsigned()),
        guid: Some(entry.guid.clone()),
        lane: entry.lane.map(i32::cast_unsigned),
    }
}

fn marker_region_to_region(entry: &MarkerRegion) -> Option<Region> {
    let end = entry.end_position?;
    Some(Region {
        id: Some(entry.id.cast_unsigned()),
        time_range: daw::service::TimeRange::from_seconds(entry.position, end),
        name: entry.name.clone(),
        color: Some(entry.color.cast_unsigned()),
        guid: Some(entry.guid.clone()),
        lane: entry.lane.map(i32::cast_unsigned),
    })
}

impl Markers for OfflineDaw {
    fn all(&self, _project: ProjectContext) -> Vec<Marker> {
        self.project
            .borrow()
            .markers_regions
            .markers
            .iter()
            .map(marker_region_to_marker)
            .collect()
    }

    fn get(&self, _project: ProjectContext, id: u32) -> Option<Marker> {
        let id = id.cast_signed();
        self.project
            .borrow()
            .markers_regions
            .markers
            .iter()
            .find(|m| m.id == id)
            .map(marker_region_to_marker)
    }

    fn count(&self, _project: ProjectContext) -> u32 {
        u32::try_from(self.project.borrow().markers_regions.markers.len()).unwrap_or(u32::MAX)
    }

    fn add(&self, project: ProjectContext, position: f64, name: &str) -> DawResult<u32> {
        Markers::add_with_guid(self, project, &new_guid(), position, name)
    }

    fn add_with_guid(
        &self,
        _project: ProjectContext,
        guid: &str,
        position: f64,
        name: &str,
    ) -> DawResult<u32> {
        if self.guid_taken(guid) {
            return Err(DawError::already_exists("Marker", guid));
        }
        let id = self.next_id();
        self.project.borrow_mut().markers_regions.add(MarkerRegion {
            id,
            position,
            name: name.to_string(),
            color: 0,
            flags: 0,
            locked: 0,
            guid: guid.to_string(),
            additional: 0,
            end_position: None,
            lane: None,
            beat_position: None,
        });
        Ok(id.cast_unsigned())
    }

    fn remove(&self, _project: ProjectContext, id: u32) -> DawResult<()> {
        let id = id.cast_signed();
        let mut project = self.project.borrow_mut();
        project.markers_regions.all.retain(|m| m.id != id);
        project.markers_regions.markers.retain(|m| m.id != id);
        project.markers_regions.regions.retain(|m| m.id != id);
        Ok(())
    }

    fn set_position(&self, _project: ProjectContext, id: u32, position: f64) -> DawResult<()> {
        self.with_entry_mut(id, |m| m.position = position)
    }

    fn rename(&self, _project: ProjectContext, id: u32, name: &str) -> DawResult<()> {
        self.with_entry_mut(id, |m| m.name = name.to_string())
    }

    fn set_color(&self, _project: ProjectContext, id: u32, color: u32) -> DawResult<()> {
        self.with_entry_mut(id, |m| m.color = color.cast_signed())
    }

    fn set_lane(&self, _project: ProjectContext, id: u32, lane: Option<u32>) -> DawResult<()> {
        self.with_entry_mut(id, |m| m.lane = lane.map(u32::cast_signed))
    }
}

impl Regions for OfflineDaw {
    fn all(&self, _project: ProjectContext) -> Vec<Region> {
        self.project
            .borrow()
            .markers_regions
            .regions
            .iter()
            .filter_map(marker_region_to_region)
            .collect()
    }

    fn get(&self, _project: ProjectContext, id: u32) -> Option<Region> {
        let id = id.cast_signed();
        self.project
            .borrow()
            .markers_regions
            .regions
            .iter()
            .find(|m| m.id == id)
            .and_then(marker_region_to_region)
    }

    fn count(&self, _project: ProjectContext) -> u32 {
        u32::try_from(self.project.borrow().markers_regions.regions.len()).unwrap_or(u32::MAX)
    }

    fn add(&self, project: ProjectContext, start: f64, end: f64, name: &str) -> DawResult<u32> {
        Regions::add_with_guid(
            self,
            project,
            &new_guid(),
            daw::service::TimeRange::from_seconds(start, end),
            name,
        )
    }

    fn add_with_guid(
        &self,
        _project: ProjectContext,
        guid: &str,
        range: daw::service::TimeRange,
        name: &str,
    ) -> DawResult<u32> {
        if self.guid_taken(guid) {
            return Err(DawError::already_exists("Region", guid));
        }
        let (start, end) = (range.start_seconds(), range.end_seconds());
        let id = self.next_id();
        self.project.borrow_mut().markers_regions.add(MarkerRegion {
            id,
            position: start,
            name: name.to_string(),
            color: 0,
            // Bit 0 is the region marker `dawfile_reaper`'s own
            // `MarkerRegionCollection` pairing logic gates on
            // (`start.flags & 1 == 0` skips pairing) — without it, a
            // region synthesized here degrades into two orphan point
            // markers the moment the file is re-parsed, so the next
            // pipeline run's "does one already exist" checks never find
            // it and add another one every time.
            flags: 1,
            locked: 0,
            guid: guid.to_string(),
            additional: 0,
            end_position: Some(end),
            lane: None,
            beat_position: None,
        });
        Ok(id.cast_unsigned())
    }

    fn remove(&self, project: ProjectContext, id: u32) -> DawResult<()> {
        Markers::remove(self, project, id)
    }

    fn set_bounds(&self, _project: ProjectContext, id: u32, start: f64, end: f64) -> DawResult<()> {
        self.with_entry_mut(id, |m| {
            m.position = start;
            m.end_position = Some(end);
        })
    }

    fn rename(&self, project: ProjectContext, id: u32, name: &str) -> DawResult<()> {
        Markers::rename(self, project, id, name)
    }

    fn set_color(&self, project: ProjectContext, id: u32, color: u32) -> DawResult<()> {
        Markers::set_color(self, project, id, color)
    }

    fn set_lane(&self, project: ProjectContext, id: u32, lane: Option<u32>) -> DawResult<()> {
        Markers::set_lane(self, project, id, lane)
    }
}

impl Projects for OfflineDaw {
    fn info(&self, _project: ProjectContext) -> DawResult<ProjectInfo> {
        Ok(ProjectInfo {
            guid: String::new(),
            name: self.name.clone(),
            path: String::new(),
        })
    }

    fn current(&self) -> Option<ProjectInfo> {
        self.info(ProjectContext::Current).ok()
    }

    fn get(&self, _project_id: &str) -> Option<ProjectInfo> {
        self.current()
    }

    fn list(&self) -> Vec<ProjectInfo> {
        self.current().into_iter().collect()
    }

    fn get_by_slot(&self, _slot: u32) -> Option<ProjectInfo> {
        self.current()
    }

    fn select(&self, _project_id: &str) -> bool {
        true
    }

    fn open(&self, _path: &str) -> Option<ProjectInfo> {
        None
    }

    fn create(&self) -> Option<ProjectInfo> {
        None
    }

    fn close(&self, _project_id: &str) -> bool {
        false
    }

    fn begin_undo_block(&self, _project: ProjectContext, _label: &str) {}

    fn end_undo_block(
        &self,
        _project: ProjectContext,
        _label: &str,
        _scope: Option<daw::service::UndoScope>,
    ) {
    }

    fn undo(&self, _project: ProjectContext) -> bool {
        false
    }

    fn redo(&self, _project: ProjectContext) -> bool {
        false
    }

    fn last_undo_label(&self, _project: ProjectContext) -> Option<String> {
        None
    }

    fn last_redo_label(&self, _project: ProjectContext) -> Option<String> {
        None
    }

    fn run_command(&self, _project: ProjectContext, _command: &str) -> bool {
        false
    }

    fn save(&self, _project: ProjectContext) {}

    fn save_all(&self) {}

    fn get_project_info_string(&self, _project: ProjectContext, _key: &str) -> String {
        String::new()
    }

    fn set_project_info_string(&self, _project: ProjectContext, _key: &str, _value: &str) {}

    fn get_project_info(&self, _project: ProjectContext, _key: &str) -> f64 {
        0.0
    }

    /// The only variant this offline backend actually needs: `ensure_core_lanes`
    /// sets each core lane's flags via `RULER_LANE_FLAGS:{name_key_index}`.
    /// Everything else is a no-op — nothing in the offline pipeline reads
    /// back an arbitrary project-info key.
    fn set_project_info(&self, _project: ProjectContext, key: &str, value: f64) {
        let Some(index) = key
            .strip_prefix("RULER_LANE_FLAGS:")
            .and_then(|n| n.parse::<u32>().ok())
        else {
            return;
        };
        // REAPER flag bitfields (0/4/8 here) arrive as f64 through the
        // generic project-info surface; `daw-reaper`'s own
        // `set_project_info` round-trips the same way on the live side.
        #[allow(clippy::as_conversions, clippy::cast_possible_truncation)]
        let flags = value.round() as i32;
        let rpp_index = i32::try_from(index).unwrap_or(0).saturating_add(1);
        Self::set_lane_flags(&mut self.project.borrow_mut(), rpp_index, flags);
    }

    fn get_project_config(&self, _project: ProjectContext, _key: &str) -> Option<f64> {
        None
    }

    fn set_project_config(&self, _project: ProjectContext, _key: &str, _value: f64) -> bool {
        false
    }

    /// `name_key_index` is 0-based (matches `I_LANENUMBER`); the `RULERLANE`
    /// chunk's own `index` field is 1-based (matches the legacy
    /// `setlist_rpp::organize_ruler_lanes` convention) — offset by one.
    fn set_ruler_lane_name(&self, _project: ProjectContext, lane_index: u32, name: &str) {
        let rpp_index = i32::try_from(lane_index).unwrap_or(0).saturating_add(1);
        Self::set_lane_name(&mut self.project.borrow_mut(), rpp_index, name);
    }

    fn get_ruler_lane_name(&self, _project: ProjectContext, lane_index: u32) -> String {
        let rpp_index = i32::try_from(lane_index).unwrap_or(0).saturating_add(1);
        self.project
            .borrow()
            .ruler_lanes
            .iter()
            .find(|l| l.index == rpp_index)
            .map(|l| l.name.clone())
            .unwrap_or_default()
    }

    fn ruler_lane_count(&self, _project: ProjectContext) -> u32 {
        u32::try_from(self.project.borrow().ruler_lanes.len()).unwrap_or(u32::MAX)
    }
}

/// Ensure a whole-song region exists on the `Song` lane, spanning every
/// section region's bounds. A no-op if one is already there — mirrors
/// [`convert_markers_to_session_format`]'s own SONG-region synthesis, which
/// only fires from *marker*-derived bounds and so never runs for a project
/// whose sections already arrived as regions (the common case for an
/// existing `.RPP`, as opposed to one built up marker-by-marker live).
fn ensure_song_region<D>(daw: &D, project_name: &str) -> eyre::Result<()>
where
    D: Regions,
{
    let project = ProjectContext::Current;
    let song_lane = CoreLane::Song.lane_index();
    let regions = Regions::all(daw, project.clone());
    if regions.iter().any(|r| r.lane == Some(song_lane)) {
        return Ok(());
    }

    let start = regions
        .iter()
        .map(Region::start_seconds)
        .fold(f64::INFINITY, f64::min);
    let end = regions
        .iter()
        .map(Region::end_seconds)
        .fold(f64::NEG_INFINITY, f64::max);
    if !start.is_finite() || !end.is_finite() || end <= start {
        return Ok(());
    }

    let id = Regions::add(daw, project.clone(), start, end, project_name)?;
    Regions::set_lane(daw, project, id, Some(song_lane))?;
    Ok(())
}

/// Convert an offline `.RPP`'s markers/regions into the FTS lane system.
///
/// Stray point markers become regions (rare — most albums already track
/// sections as regions, not markers), every section region is recolored,
/// renamed (disambiguating touching/repeated sections), and pinned to the
/// `Sections` lane, a whole-song region is added to the `Song` lane if
/// missing, and stray non-FTS lanes are cleaned up.
///
/// Mirrors `run_action`'s handling of
/// `KeyflowAction::ConvertMarkersToSessionFormat` live in REAPER, with one
/// addition (`ensure_song_region`) to cover regions that were already
/// regions on disk rather than markers this call converts itself.
///
/// # Errors
///
/// Returns an error if the underlying offline backend fails to add, rename,
/// recolor, or relane a marker/region — in practice this backend's own
/// operations never fail, so an error here would indicate a bug in the
/// shared `keyflow::actions` pipeline rather than a bad input file.
pub fn auto_organize_regions(project: &mut ReaperProject, project_name: &str) -> eyre::Result<()> {
    let daw = OfflineDaw::new(std::mem::take(project), project_name);

    ensure_core_lanes(&daw);
    convert_markers_to_session_format(&daw)?;
    let regions = Regions::all(&daw, ProjectContext::Current);
    super::actions::normalize_section_regions(&daw, regions)?;
    ensure_song_region(&daw, project_name)?;
    normalize_marker_lanes(&daw)?;
    hide_stray_lanes(&daw);

    *project = daw.into_inner();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dawfile_reaper::types::RppSerialize;

    /// A multitrack's markers made the session's: sections regions running
    /// section to section — never to the note between them — the last to
    /// the media's end; the song's region and SONGSTART/SONGEND; the note
    /// and the count-in left markers, the note on the MARKS lane.
    #[test]
    fn section_markers_become_regions_and_the_rest_stay_marks() {
        let daw = OfflineDaw::new(ReaperProject::default(), "unbreakable.current");
        let at = ProjectContext::Current;
        for (position, name) in [
            (0.0, "COUNT"),
            (2.0, "IN"),
            (5.0, "Scream"),
            (10.0, "VS 1"),
            (20.0, "B - Pre Chorus"),
            (25.0, "A"),
            (30.0, "Last Hit"),
        ] {
            Markers::add(&daw, at.clone(), position, name).unwrap();
        }
        let done =
            super::super::actions::convert_markers_in(&daw, &at, Some("Unbreakable"), Some(40.0))
                .unwrap();
        assert_eq!(
            done.sections,
            ["IN", "VS 1", "B - Pre Chorus", "A", "Last Hit"]
        );
        assert_eq!(done.song, Some((2.0, 40.0)));

        let regions = Regions::all(&daw, at.clone());
        let lane = |r: &Region| r.lane;
        let span = |name: &str| {
            regions
                .iter()
                .find(|r| r.name == name && lane(r) == Some(CoreLane::Sections.lane_index()))
                .map(|r| (r.start_seconds(), r.end_seconds()))
        };
        assert_eq!(
            span("IN"),
            Some((2.0, 10.0)),
            "to the next section, past the note"
        );
        assert_eq!(
            span("Last Hit"),
            Some((30.0, 40.0)),
            "the last to the media's end"
        );
        let song: Vec<_> = regions
            .iter()
            .filter(|r| r.lane == Some(CoreLane::Song.lane_index()))
            .map(|r| (r.name.clone(), r.start_seconds(), r.end_seconds()))
            .collect();
        assert_eq!(song, [("Unbreakable".to_string(), 2.0, 40.0)]);

        let markers: Vec<(String, f64, Option<u32>)> = Markers::all(&daw, at)
            .into_iter()
            .map(|m| (m.name.clone(), m.position.seconds().unwrap_or(-1.0), m.lane))
            .collect();
        let named = |name: &str| markers.iter().find(|(n, ..)| n == name).cloned();
        assert_eq!(named("SONGSTART").map(|m| m.1), Some(2.0));
        assert_eq!(named("SONGEND").map(|m| m.1), Some(40.0));
        assert_eq!(
            named("Scream").and_then(|m| m.2),
            Some(CoreLane::Marks.lane_index()),
            "the note a mark"
        );
        assert!(named("COUNT").is_some(), "the count-in a marker still");
        assert!(named("IN").is_none(), "a section is no longer a marker");
    }

    /// A project with section regions already keeps them: no marker is
    /// made a second one.
    #[test]
    fn a_project_with_sections_is_left_its_regions() {
        let daw = OfflineDaw::new(ReaperProject::default(), "song");
        let at = ProjectContext::Current;
        let id = Regions::add(&daw, at.clone(), 0.0, 8.0, "VS 1").unwrap();
        Regions::set_lane(&daw, at.clone(), id, Some(CoreLane::Sections.lane_index())).unwrap();
        Markers::add(&daw, at.clone(), 8.0, "CH 1").unwrap();
        let done = super::super::actions::convert_markers_in(&daw, &at, None, Some(16.0)).unwrap();
        assert!(done.sections.is_empty());
        assert_eq!(Regions::all(&daw, at).len(), 1);
    }

    /// A region added offline (empty guid, no `flags` bit set) used to
    /// degrade into two orphan point markers the moment it was serialized
    /// and re-parsed: `dawfile_reaper`'s tokenizer collapses an empty guid
    /// field instead of parsing it as blank, shifting every field after it
    /// (including `lane`) left by one, and its region-pairing logic
    /// separately requires `flags & 1 != 0` on the start marker. Together
    /// these meant `ensure_song_region`'s "does one already exist" check
    /// never recognized its own region on the next run, silently adding a
    /// duplicate whole-song region every single time the pipeline ran —
    /// found by rerunning it on the Rockstars album a second time.
    #[test]
    fn added_region_survives_a_serialize_reparse_round_trip_as_a_region_on_its_lane() {
        // `ensure_song_region` derives the whole-song span from existing
        // section regions, so seed one real section for it to work with.
        let mut project = ReaperProject::default();
        {
            let daw = OfflineDaw::new(std::mem::take(&mut project), "Round Trip Test");
            let id = Regions::add(&daw, ProjectContext::Current, 0.0, 8.0, "VS").unwrap();
            Regions::set_lane(
                &daw,
                ProjectContext::Current,
                id,
                Some(CoreLane::Sections.lane_index()),
            )
            .unwrap();
            project = daw.into_inner();
        }
        auto_organize_regions(&mut project, "Round Trip Test").unwrap();
        let text = project.to_rpp_string();
        let reparsed = dawfile_reaper::io::parse_project_text(&text).unwrap();

        let daw = OfflineDaw::new(reparsed, "Round Trip Test");
        let song_lane = CoreLane::Song.lane_index();
        let song_regions: Vec<_> = Regions::all(&daw, ProjectContext::Current)
            .into_iter()
            .filter(|r| r.lane == Some(song_lane))
            .collect();

        assert_eq!(
            song_regions.len(),
            1,
            "expected exactly one whole-song region to survive the round trip, got {song_regions:?}"
        );

        // And running the pipeline again on the reparsed project must not
        // add a second one.
        let mut project = daw.into_inner();
        auto_organize_regions(&mut project, "Round Trip Test").unwrap();
        let text = project.to_rpp_string();
        let reparsed = dawfile_reaper::io::parse_project_text(&text).unwrap();
        let daw = OfflineDaw::new(reparsed, "Round Trip Test");
        let song_regions: Vec<_> = Regions::all(&daw, ProjectContext::Current)
            .into_iter()
            .filter(|r| r.lane == Some(song_lane))
            .collect();
        assert_eq!(
            song_regions.len(),
            1,
            "a second pipeline run must not duplicate the whole-song region, got {song_regions:?}"
        );
    }
}
