//! `SongBuilder` - Extract song structure from DAW projects
//!
//! Analyzes markers, regions, and tempo maps to build Song domain objects.
//!
//! ## Song Name Convention
//! Project names follow the format: "Title - Artist.rpp"
//! - The ".rpp" extension is stripped
//! - "Title - Artist" is parsed to extract song name and artist
//!
//! ## Section Detection
//! Sections can be built from either:
//! 1. **Regions**: Regions contained within a song region (preferred)
//! 2. **Markers**: When no regions exist, consecutive markers are used to define sections
//!    Each marker defines the start of a section, ending at the next marker.

// REAPER FFI backend + the sync service traits it implements — used only by
// the `*_native` builders below (the async builders drive the backend-agnostic
// `daw::rpc::Project` handle). Native-only.
#[cfg(all(not(target_arch = "wasm32"), feature = "reaper"))]
use daw::reaper::Reaper;
use daw::rpc::Project;
use daw::service::{Marker, ProjectContext, Region};
use daw::service::{Markers, Projects, Regions, TempoMap};

/// What [`SongBuilder::build_on`] reads from a backend: project info and
/// ruler lanes, markers, regions, and the tempo map. REAPER and
/// `daw-standalone` both provide it.
pub trait SongDaw: Projects + Markers + Regions + TempoMap {}
impl<T: Projects + Markers + Regions + TempoMap> SongDaw for T {}
use session_proto::{Comment, Section, SectionId, SectionType, Song, SongId};
use tracing::{Level, debug, warn};

/// Builder for extracting Song structure from DAW projects
pub struct SongBuilder;

/// Helper to get seconds from Position
fn position_to_seconds(pos: &daw::service::Position) -> f64 {
    pos.time
        .as_ref()
        .map_or(0.0, daw_proto::PositionInSeconds::as_seconds)
}

/// Resolved lane indices for a specific project (looked up by name).
///
/// Lane indices vary between projects since users can reorder lanes.
/// We resolve by matching the lane display name (e.g., "SONG", "SECTIONS").
#[derive(Debug, Default)]
struct ResolvedLanes {
    /// Lane index for SONG regions (parent song containers)
    song: Option<u32>,
    /// Lane index for SECTIONS regions (section child regions)
    sections: Option<u32>,
}

impl ResolvedLanes {
    /// Query the project's ruler lanes and resolve indices by name.
    async fn resolve(project: &Project) -> Self {
        let count = project.ruler_lane_count().await.unwrap_or(0);
        let mut resolved = Self::default();

        for idx in 1..=count {
            if let Ok(name) = project.get_ruler_lane_name(idx).await {
                match name.to_uppercase().as_str() {
                    "SONG" => resolved.song = Some(idx),
                    "SECTIONS" => resolved.sections = Some(idx),
                    _ => {}
                }
            }
        }

        debug!(
            "ResolvedLanes: song={:?}, sections={:?}",
            resolved.song, resolved.sections
        );
        resolved
    }

    fn resolve_on<D: SongDaw>(daw: &D, project: &ProjectContext) -> Self {
        let count = daw.ruler_lane_count(project.clone());
        let mut resolved = Self::default();

        // Lanes are numbered from 0 — `RULER_LANE_NAME:0` is the leftmost,
        // and SONG is lane 0 (`CoreLane`). Starting at 1 never saw it, so
        // the SONG region was never recognised as the song.
        for idx in 0..count {
            let name = daw.get_ruler_lane_name(project.clone(), idx);
            match name.to_uppercase().as_str() {
                "SONG" => resolved.song = Some(idx),
                "SECTIONS" => resolved.sections = Some(idx),
                _ => {}
            }
        }

        debug!(
            "ResolvedLanes(native): song={:?}, sections={:?}",
            resolved.song, resolved.sections
        );
        resolved
    }

    /// Check if a region is in the SONG lane.
    fn is_song_lane(&self, region: &Region) -> bool {
        self.song.is_some() && region.lane == self.song
    }
}

impl SongBuilder {
    /// Build one or more Songs from an in-process REAPER project using sync native traits.
    /// REAPER-only (see the `reaper` Cargo feature) — has external callers
    /// (`song::service`, `setlist::actions`, `guide`) that are themselves
    /// generic/unconditional, so this stays present in every build with an
    /// error fallback rather than being gated away at each call site.
    ///
    /// # Errors
    ///
    /// Returns an error if the `reaper` feature is not enabled (REAPER is not available).
    #[cfg(not(feature = "reaper"))]
    pub fn build_native(_project: ProjectContext) -> eyre::Result<Vec<Song>> {
        Err(eyre::eyre!(
            "SongBuilder::build_native requires the `reaper` feature (no REAPER host in this build)"
        ))
    }

    /// Build one or more Songs from an in-process REAPER project using sync native traits.
    ///
    /// # Errors
    ///
    /// Returns an error if querying the project's information, markers, regions, or tempo map fails.
    #[cfg(feature = "reaper")]
    pub fn build_native(project: ProjectContext) -> eyre::Result<Vec<Song>> {
        Self::build_on(&Reaper, project)
    }

    /// Build one or more Songs from a project on any in-process backend —
    /// REAPER or `daw-standalone` — through the sync service traits.
    ///
    /// # Errors
    ///
    /// Returns an error if querying the project's information, markers, regions, or tempo map fails.
    pub fn build_on<D: SongDaw>(daw: &D, project: ProjectContext) -> eyre::Result<Vec<Song>> {
        let project_info = daw.info(project.clone())?;
        debug!(
            "SongBuilder::build_native for project {}",
            project_info.guid
        );

        let markers = Markers::all(daw, project.clone());
        let regions = Regions::all(daw, project.clone());
        let lanes = ResolvedLanes::resolve_on(daw, &project);

        let song_regions = Self::find_song_regions(&regions, &lanes);
        if song_regions.len() >= 2 {
            debug!(
                "Multi-song mode: found {} song regions in project {}",
                song_regions.len(),
                project_info.guid
            );
            let mut songs = Vec::with_capacity(song_regions.len());
            for song_region in &song_regions {
                let song = Self::build_song_from_region_native(
                    daw,
                    project.clone(),
                    &project_info.guid,
                    song_region,
                    &regions,
                    &markers,
                    &lanes,
                );
                songs.push(song);
            }
            Ok(songs)
        } else {
            let song = Self::build_single_song_native(
                daw,
                project,
                &project_info.guid,
                &project_info.name,
                &markers,
                &regions,
                &lanes,
            );
            Ok(vec![song])
        }
    }

    /// Build one or more Songs from a DAW project.
    ///
    /// When the project contains multiple parent regions (each with ≥2 child regions),
    /// each parent region is treated as a separate song (multi-song mode).
    /// Otherwise the entire project is treated as a single song (backward-compatible).
    ///
    /// # Errors
    ///
    /// Returns an error if project information, markers, regions, or tempo map queries fail.
    pub async fn build(project: &Project) -> eyre::Result<Vec<Song>> {
        debug!("SongBuilder::build for project {}", project.guid());

        let markers_api = project.markers();
        let regions_api = project.regions();
        let (project_info, markers, regions) =
            tokio::try_join!(project.info(), markers_api.all(), regions_api.all())?;
        let tempo_map = project.tempo_map();

        // Resolve lane indices by name (lane order can vary between projects)
        let lanes = ResolvedLanes::resolve(project).await;

        // Check for multi-song layout: multiple parent regions each containing ≥2 children
        let song_regions = Self::find_song_regions(&regions, &lanes);
        if song_regions.len() >= 2 {
            debug!(
                "Multi-song mode: found {} song regions in project {}",
                song_regions.len(),
                project.guid()
            );
            let project_guid = project.guid().to_string();
            let mut songs = Vec::with_capacity(song_regions.len());
            for song_region in &song_regions {
                match Self::build_song_from_region(
                    &project_guid,
                    song_region,
                    &regions,
                    &markers,
                    &tempo_map,
                    &lanes,
                )
                .await
                {
                    Ok(song) => songs.push(song),
                    Err(e) => {
                        warn!(
                            "Failed to build song from region '{}': {}",
                            song_region.name, e
                        );
                    }
                }
            }
            Ok(songs)
        } else {
            // Single-song mode: preserve exact backward compatibility
            let song = Self::build_single_song(
                project,
                &project_info.name,
                &markers,
                &regions,
                &tempo_map,
                &lanes,
            )
            .await?;
            Ok(vec![song])
        }
    }

    /// Determine song boundaries by analyzing markers and regions.
    async fn determine_song_bounds(
        markers: &[Marker],
        regions: &[Region],
        tempo_map: &daw::rpc::TempoMap,
        song_region: Option<&Region>,
    ) -> eyre::Result<(f64, f64, f64)> {
        let count_in_marker = markers.iter().find(|m| Self::is_count_in_marker(&m.name));
        let absolute_start_marker = markers.iter().find(|m| m.name == "=START");
        let songstart_marker = markers.iter().find(|m| Self::is_songstart_marker(&m.name));
        let songend_marker = markers.iter().find(|m| Self::is_songend_marker(&m.name));
        let absolute_end_marker = markers.iter().find(|m| m.name == "=END");
        let postroll_marker = markers
            .iter()
            .find(|m| m.name == "POSTROLL" || m.name == "=POSTROLL");

        let start_marker =
            songstart_marker.or_else(|| markers.iter().find(|m| m.name.starts_with("=SONGSTART")));
        let end_marker =
            songend_marker.or_else(|| markers.iter().find(|m| m.name.starts_with("=SONGEND")));

        let (start_seconds, songend_seconds, end_seconds) =
            if let (Some(start), Some(end)) = (start_marker, end_marker) {
                let song_start = position_to_seconds(&start.position);
                let song_end = position_to_seconds(&end.position);
                let absolute_end =
                    absolute_end_marker.map_or(song_end, |m| position_to_seconds(&m.position));
                let outer_end =
                    postroll_marker.map_or(absolute_end, |m| position_to_seconds(&m.position));
                let snapped_end = Self::snap_to_next_barline(tempo_map, outer_end)
                    .await
                    .unwrap_or(outer_end);
                (song_start, song_end, snapped_end)
            } else if let Some(song_region) = song_region {
                let end = song_region.time_range.end_seconds();
                let start = song_region.time_range.start_seconds();
                (start, end, end)
            } else {
                let start = markers
                    .iter()
                    .map(|m| position_to_seconds(&m.position))
                    .chain(regions.iter().map(|r| r.time_range.start_seconds()))
                    .min_by(|a: &f64, b: &f64| a.total_cmp(b))
                    .unwrap_or(0.0);
                let end = markers
                    .iter()
                    .map(|m| position_to_seconds(&m.position))
                    .chain(regions.iter().map(|r| r.time_range.end_seconds()))
                    .max_by(|a: &f64, b: &f64| a.total_cmp(b))
                    .unwrap_or(60.0);
                (start, end, end)
            };

        Ok((start_seconds, songend_seconds, end_seconds))
    }

    /// Build a single Song from the entire project (original build logic).
    ///
    /// Preserves exact backward compatibility for projects with zero or one song region.
    async fn build_single_song(
        project: &Project,
        project_name: &str,
        markers: &[Marker],
        regions: &[Region],
        tempo_map: &daw::rpc::TempoMap,
        lanes: &ResolvedLanes,
    ) -> eyre::Result<Song> {
        // Parse song name and artist from project name
        let (song_name, _artist) = Self::parse_project_name(project_name);

        // Find the song region (if regions exist)
        let song_region = Self::find_song_region(regions, lanes);

        // Determine song bounds
        let (start_seconds, songend_seconds, end_seconds) =
            Self::determine_song_bounds(markers, regions, tempo_map, song_region).await?;

        // Calculate count-in duration
        let count_in_marker = markers.iter().find(|m| Self::is_count_in_marker(&m.name));
        let count_in_seconds = count_in_marker.and_then(|m| {
            let marker_time = position_to_seconds(&m.position);
            if marker_time < start_seconds {
                Some(start_seconds - marker_time)
            } else {
                None
            }
        });

        // Extract sections - prefer regions, fall back to markers
        let mut sections = song_region.map_or_else(
            || {
                if regions.is_empty() {
                    // No regions - build sections from markers
                    debug!("No regions found, building sections from markers");
                    Self::build_sections_from_markers(markers, start_seconds, songend_seconds)
                } else {
                    Self::extract_sections_from_regions(regions, start_seconds, songend_seconds)
                }
            },
            |song_region| Self::extract_sections_from_song_region(regions, song_region, lanes),
        );

        debug!("Extracted {} sections", sections.len());
        if tracing::enabled!(Level::DEBUG) {
            for (i, section) in sections.iter().enumerate() {
                debug!(
                    "  Section[{}]: '{}' type={:?} start={:.3} end={:.3} duration={:.3}",
                    i,
                    section.name,
                    section.section_type,
                    section.start_seconds,
                    section.end_seconds,
                    section.end_seconds - section.start_seconds
                );
            }
        }

        // Add Count-In section at the beginning if there's a count-in
        let song_start_seconds =
            Self::add_count_in_section(&mut sections, count_in_seconds, start_seconds);

        // Add END section if there's a gap between SONGEND and =END
        Self::add_end_section(&mut sections, end_seconds, songend_seconds);

        // Log final sections list
        debug!("Final sections after adding Count-In/End:");
        if tracing::enabled!(Level::DEBUG) {
            for (i, section) in sections.iter().enumerate() {
                debug!(
                    "  Final[{}]: '{}' start={:.3} end={:.3}",
                    i, section.name, section.start_seconds, section.end_seconds
                );
            }
        }

        let (tempo, time_sig, measure_positions, comments) = Self::build_song_tail_fields(
            tempo_map,
            markers,
            &sections,
            start_seconds,
            song_start_seconds,
            end_seconds,
            count_in_marker,
        )
        .await;

        Ok(Song {
            id: SongId::new(),
            name: song_name,
            project_guid: project.guid().to_string(),
            start_seconds: song_start_seconds,
            end_seconds,
            count_in_seconds,
            sections,
            comments,
            tempo,
            time_signature: time_sig,
            measure_positions,
            chart_text: None,
            parsed_chart: None,
            detected_chords: Vec::new(),
            chart_fingerprint: None,
            advance_mode: None,
            color: song_region.as_ref().and_then(|r| r.color),
        })
    }

    /// The tail of [`Self::build_single_song`]: tempo/time-signature lookup,
    /// measure positions, and comment-marker extraction (including the
    /// mid-song count-in special case).
    async fn build_song_tail_fields(
        tempo_map: &daw::rpc::TempoMap,
        markers: &[Marker],
        sections: &[Section],
        start_seconds: f64,
        song_start_seconds: f64,
        end_seconds: f64,
        count_in_marker: Option<&Marker>,
    ) -> (
        Option<f64>,
        Option<daw::service::TimeSignature>,
        Vec<daw::service::Position>,
        Vec<Comment>,
    ) {
        // Get tempo and time signature at song start
        let tempo = tempo_map.tempo_at(start_seconds).await.ok();
        let time_sig = tempo_map
            .time_signature_at(start_seconds)
            .await
            .ok()
            .map(|(num, denom)| {
                daw::service::TimeSignature::new(num.cast_unsigned(), denom.cast_unsigned())
            });

        // Build measure positions if we have tempo and time signature
        let measure_positions = if let (Some(bpm), Some(ts)) = (tempo, time_sig) {
            Self::calculate_measure_positions(song_start_seconds, end_seconds, bpm, ts)
        } else {
            Vec::new()
        };

        // Extract comment markers (non-structural markers within song bounds)
        // Also handle COUNT-IN marker as a comment if it's after the first section starts
        // (for songs where count-in happens mid-song, e.g., keys-only intro)
        let first_section_start = sections
            .first()
            .map_or(song_start_seconds, |s| s.start_seconds);
        let count_in_marker_pos = count_in_marker.map(|m| position_to_seconds(&m.position));
        let count_in_is_mid_song =
            count_in_marker_pos.is_some_and(|pos| pos > first_section_start + 0.01);

        let comments = Self::extract_comments_with_mid_song_count_in(
            markers,
            song_start_seconds,
            end_seconds,
            count_in_is_mid_song,
        );

        if tracing::enabled!(Level::DEBUG) {
            debug!("Extracted {} comments", comments.len());
            for comment in &comments {
                debug!(
                    "  Comment: '{}' at {:.3}s{}",
                    comment.text,
                    comment.position_seconds,
                    if comment.is_count_in {
                        " (count-in)"
                    } else {
                        ""
                    }
                );
            }
        }

        (tempo, time_sig, measure_positions, comments)
    }

    /// Determine song boundaries by analyzing markers and regions (native version).
    fn determine_song_bounds_native<D: SongDaw>(
        daw: &D,
        project: ProjectContext,
        markers: &[Marker],
        regions: &[Region],
        song_region: Option<&Region>,
    ) -> (f64, f64, f64) {
        let songstart_marker = markers.iter().find(|m| Self::is_songstart_marker(&m.name));
        let songend_marker = markers.iter().find(|m| Self::is_songend_marker(&m.name));
        let absolute_end_marker = markers.iter().find(|m| m.name == "=END");
        let postroll_marker = markers
            .iter()
            .find(|m| m.name == "POSTROLL" || m.name == "=POSTROLL");

        let start_marker =
            songstart_marker.or_else(|| markers.iter().find(|m| m.name.starts_with("=SONGSTART")));
        let end_marker =
            songend_marker.or_else(|| markers.iter().find(|m| m.name.starts_with("=SONGEND")));

        if let (Some(start), Some(end)) = (start_marker, end_marker) {
            let song_start = position_to_seconds(&start.position);
            let song_end = position_to_seconds(&end.position);
            let absolute_end =
                absolute_end_marker.map_or(song_end, |m| position_to_seconds(&m.position));
            let outer_end =
                postroll_marker.map_or(absolute_end, |m| position_to_seconds(&m.position));
            let snapped_end = Self::snap_to_next_barline_native(daw, project, outer_end);
            (song_start, song_end, snapped_end)
        } else if let Some(song_region) = song_region {
            let end = song_region.time_range.end_seconds();
            let start = song_region.time_range.start_seconds();
            (start, end, end)
        } else {
            let start = markers
                .iter()
                .map(|m| position_to_seconds(&m.position))
                .chain(regions.iter().map(|r| r.time_range.start_seconds()))
                .min_by(|a: &f64, b: &f64| a.total_cmp(b))
                .unwrap_or(0.0);
            let end = markers
                .iter()
                .map(|m| position_to_seconds(&m.position))
                .chain(regions.iter().map(|r| r.time_range.end_seconds()))
                .max_by(|a: &f64, b: &f64| a.total_cmp(b))
                .unwrap_or(60.0);
            (start, end, end)
        }
    }

    fn build_single_song_native<D: SongDaw>(
        daw: &D,
        project: ProjectContext,
        project_guid: &str,
        project_name: &str,
        markers: &[Marker],
        regions: &[Region],
        lanes: &ResolvedLanes,
    ) -> Song {
        let song_region = Self::find_song_region(regions, lanes);
        // The SONG-lane region names the song when there is one; the
        // project file's name is only the fallback. A project file is
        // often named for where it lives, not for what it holds.
        let (song_name, _artist) =
            Self::parse_project_name(song_region.map_or(project_name, |r| r.name.as_str()));

        let (start_seconds, songend_seconds, end_seconds) =
            Self::determine_song_bounds_native(daw, project.clone(), markers, regions, song_region);

        let count_in_marker = markers.iter().find(|m| Self::is_count_in_marker(&m.name));
        let count_in_seconds = count_in_marker.and_then(|m| {
            let marker_time = position_to_seconds(&m.position);
            (marker_time < start_seconds).then_some(start_seconds - marker_time)
        });

        let mut sections = song_region.map_or_else(
            || {
                if regions.is_empty() {
                    Self::build_sections_from_markers(markers, start_seconds, songend_seconds)
                } else {
                    Self::extract_sections_from_regions(regions, start_seconds, songend_seconds)
                }
            },
            |song_region| Self::extract_sections_from_song_region(regions, song_region, lanes),
        );

        let song_start_seconds =
            Self::add_count_in_section(&mut sections, count_in_seconds, start_seconds);

        Self::add_end_section(&mut sections, end_seconds, songend_seconds);

        let tempo = Some(daw.get_tempo_at(project.clone(), start_seconds));
        let time_sig = {
            let (num, denom) = daw.get_time_signature_at(project, start_seconds);
            Some(daw::service::TimeSignature::new(
                num.max(1).cast_unsigned(),
                denom.max(1).cast_unsigned(),
            ))
        };
        let measure_positions = if let (Some(bpm), Some(ts)) = (tempo, time_sig) {
            Self::calculate_measure_positions(song_start_seconds, end_seconds, bpm, ts)
        } else {
            Vec::new()
        };

        let first_section_start = sections
            .first()
            .map_or(song_start_seconds, |s| s.start_seconds);
        let count_in_marker_pos = count_in_marker.map(|m| position_to_seconds(&m.position));
        let count_in_is_mid_song =
            count_in_marker_pos.is_some_and(|pos| pos > first_section_start + 0.01);

        let comments = Self::extract_comments_with_mid_song_count_in(
            markers,
            song_start_seconds,
            end_seconds,
            count_in_is_mid_song,
        );

        Song {
            id: SongId::new(),
            name: song_name,
            project_guid: project_guid.to_string(),
            start_seconds: song_start_seconds,
            end_seconds,
            count_in_seconds,
            sections,
            comments,
            tempo,
            time_signature: time_sig,
            measure_positions,
            chart_text: None,
            parsed_chart: None,
            detected_chords: Vec::new(),
            chart_fingerprint: None,
            advance_mode: None,
            color: song_region.as_ref().and_then(|r| r.color),
        }
    }

    /// Build a Song from a specific parent region within a multi-song project.
    ///
    /// In multi-song mode, each parent region defines a song. Song name is parsed
    /// from the region name using the "Title - Artist" convention.
    ///
    /// Structural markers (SONGSTART, SONGEND, COUNT-IN) within the region's time
    /// range define the musical boundaries. The region itself is the outer container:
    /// - COUNT-IN → SONGSTART = count-in duration
    /// - SONGSTART = where sections begin
    /// - SONGEND = where sections end
    /// - Region end (or =END marker) = absolute end including render tail
    async fn build_song_from_region(
        project_guid: &str,
        song_region: &Region,
        all_regions: &[Region],
        all_markers: &[Marker],
        tempo_map: &daw::rpc::TempoMap,
        lanes: &ResolvedLanes,
    ) -> eyre::Result<Song> {
        let (song_name, _artist) = Self::parse_project_name(&song_region.name);
        let region_start = song_region.time_range.start_seconds();
        let region_end = song_region.time_range.end_seconds();

        debug!(
            "build_song_from_region: '{}' region={:.3}–{:.3}",
            song_name, region_start, region_end
        );

        // Find structural markers within (or at the edges of) this song region.
        // Use a small tolerance so markers exactly at region boundaries are included.
        let tolerance = 0.01;
        let in_region =
            |pos: f64| -> bool { pos >= region_start - tolerance && pos <= region_end + tolerance };

        let songstart_marker = all_markers.iter().find(|m| {
            Self::is_songstart_marker(&m.name) && in_region(position_to_seconds(&m.position))
        });
        let songend_marker = all_markers.iter().find(|m| {
            Self::is_songend_marker(&m.name) && in_region(position_to_seconds(&m.position))
        });
        let count_in_marker = all_markers.iter().find(|m| {
            Self::is_count_in_marker(&m.name) && in_region(position_to_seconds(&m.position))
        });
        let abs_end_marker = all_markers
            .iter()
            .find(|m| m.name == "=END" && in_region(position_to_seconds(&m.position)));

        // Derive song boundaries from markers, falling back to region edges.
        let marker_songstart_seconds =
            songstart_marker.map_or(region_start, |m| position_to_seconds(&m.position));
        let songend_seconds =
            songend_marker.map_or(region_end, |m| position_to_seconds(&m.position));
        let end_seconds = abs_end_marker.map_or(region_end, |m| position_to_seconds(&m.position));

        debug!(
            "  markers: SONGSTART={:.3} SONGEND={:.3} =END={:.3}",
            marker_songstart_seconds, songend_seconds, end_seconds
        );

        // Extract sections from child regions within this song region
        let mut sections = Self::extract_sections_from_song_region(all_regions, song_region, lanes);

        // Calculate count-in: duration from COUNT-IN marker to SONGSTART
        let count_in_seconds = count_in_marker.and_then(|m| {
            let marker_time = position_to_seconds(&m.position);
            (marker_time < marker_songstart_seconds)
                .then_some(marker_songstart_seconds - marker_time)
        });

        // Add Count-In section if present
        let song_start_seconds =
            Self::add_count_in_section(&mut sections, count_in_seconds, marker_songstart_seconds);

        // Add End section if there's space after SONGEND
        Self::add_end_section(&mut sections, end_seconds, songend_seconds);

        // Get tempo and time signature at song start
        let tempo = tempo_map.tempo_at(marker_songstart_seconds).await.ok();
        let time_sig = tempo_map
            .time_signature_at(marker_songstart_seconds)
            .await
            .ok()
            .map(|(num, denom)| {
                daw::service::TimeSignature::new(num.cast_unsigned(), denom.cast_unsigned())
            });

        let measure_positions = if let (Some(bpm), Some(ts)) = (tempo, time_sig) {
            Self::calculate_measure_positions(song_start_seconds, end_seconds, bpm, ts)
        } else {
            Vec::new()
        };

        // Extract comment markers within this song region's bounds
        let comments =
            Self::extract_comments_in_range(all_markers, song_start_seconds, end_seconds);

        Ok(Song {
            id: SongId::new(),
            name: song_name,
            project_guid: project_guid.to_string(),
            start_seconds: song_start_seconds,
            end_seconds,
            count_in_seconds,
            sections,
            comments,
            tempo,
            time_signature: time_sig,
            measure_positions,
            chart_text: None,
            parsed_chart: None,
            detected_chords: Vec::new(),
            chart_fingerprint: None,
            advance_mode: None,
            color: song_region.color,
        })
    }

    fn build_song_from_region_native<D: SongDaw>(
        daw: &D,
        project: ProjectContext,
        project_guid: &str,
        song_region: &Region,
        all_regions: &[Region],
        all_markers: &[Marker],
        lanes: &ResolvedLanes,
    ) -> Song {
        let (song_name, _artist) = Self::parse_project_name(&song_region.name);
        let region_start = song_region.time_range.start_seconds();
        let region_end = song_region.time_range.end_seconds();

        let tolerance = 0.01;
        let in_region =
            |pos: f64| -> bool { pos >= region_start - tolerance && pos <= region_end + tolerance };

        let songstart_marker = all_markers.iter().find(|m| {
            Self::is_songstart_marker(&m.name) && in_region(position_to_seconds(&m.position))
        });
        let songend_marker = all_markers.iter().find(|m| {
            Self::is_songend_marker(&m.name) && in_region(position_to_seconds(&m.position))
        });
        let count_in_marker = all_markers.iter().find(|m| {
            Self::is_count_in_marker(&m.name) && in_region(position_to_seconds(&m.position))
        });
        let abs_end_marker = all_markers
            .iter()
            .find(|m| m.name == "=END" && in_region(position_to_seconds(&m.position)));

        let marker_songstart_seconds =
            songstart_marker.map_or(region_start, |m| position_to_seconds(&m.position));
        let songend_seconds =
            songend_marker.map_or(region_end, |m| position_to_seconds(&m.position));
        let end_seconds = abs_end_marker.map_or(region_end, |m| position_to_seconds(&m.position));

        let mut sections = Self::extract_sections_from_song_region(all_regions, song_region, lanes);

        let count_in_seconds = count_in_marker.and_then(|m| {
            let marker_time = position_to_seconds(&m.position);
            (marker_time < marker_songstart_seconds)
                .then_some(marker_songstart_seconds - marker_time)
        });

        let song_start_seconds =
            Self::add_count_in_section(&mut sections, count_in_seconds, marker_songstart_seconds);

        Self::add_end_section(&mut sections, end_seconds, songend_seconds);

        let tempo = Some(daw.get_tempo_at(project.clone(), marker_songstart_seconds));
        let time_sig = {
            let (num, denom) = daw.get_time_signature_at(project, marker_songstart_seconds);
            Some(daw::service::TimeSignature::new(
                num.max(1).cast_unsigned(),
                denom.max(1).cast_unsigned(),
            ))
        };
        let measure_positions = if let (Some(bpm), Some(ts)) = (tempo, time_sig) {
            Self::calculate_measure_positions(song_start_seconds, end_seconds, bpm, ts)
        } else {
            Vec::new()
        };

        let comments =
            Self::extract_comments_in_range(all_markers, song_start_seconds, end_seconds);

        Song {
            id: SongId::new(),
            name: song_name,
            project_guid: project_guid.to_string(),
            start_seconds: song_start_seconds,
            end_seconds,
            count_in_seconds,
            sections,
            comments,
            tempo,
            time_signature: time_sig,
            measure_positions,
            chart_text: None,
            parsed_chart: None,
            detected_chords: Vec::new(),
            chart_fingerprint: None,
            advance_mode: None,
            color: song_region.color,
        }
    }

    fn marker_to_comment(marker: &Marker) -> Comment {
        let is_count_in = Self::is_count_in_marker(&marker.name);
        let (text, section_only) = if marker.name.trim().starts_with('>') {
            (
                marker
                    .name
                    .trim()
                    .strip_prefix('>')
                    .unwrap_or(&marker.name)
                    .trim()
                    .to_string(),
                true,
            )
        } else {
            (marker.name.clone(), false)
        };
        Comment {
            id: marker.id,
            text,
            position_seconds: position_to_seconds(&marker.position),
            color: marker.color,
            is_count_in,
            section_only,
        }
    }

    /// Add a Count-In section at the beginning if there's a count-in duration.
    ///
    /// Returns the adjusted song start seconds.
    fn add_count_in_section(
        sections: &mut Vec<Section>,
        count_in_seconds: Option<f64>,
        base_start: f64,
    ) -> f64 {
        count_in_seconds.map_or(base_start, |count_in_duration| {
            if count_in_duration > 0.0 {
                let count_in_start = base_start - count_in_duration;
                let count_in_end = sections
                    .first()
                    .map_or(base_start, |s| s.start_seconds);
                debug!(
                    "Adding Count-In section: start={:.3} end={:.3} (first_section_start={:.3}, marker_start={:.3})",
                    count_in_start, count_in_end, count_in_end, base_start
                );
                sections.insert(
                    0,
                    Section {
                        section_id: SectionId::new(),
                        id: None,
                        name: "Count-In".to_string(),
                        comment: None,
                        section_type: SectionType::CountIn,
                        start_seconds: count_in_start,
                        end_seconds: count_in_end,
                        number: None,
                        color: None,
                    },
                );
                count_in_start
            } else {
                base_start
            }
        })
    }

    /// Add an END section if there's a gap between SONGEND and the absolute end.
    fn add_end_section(sections: &mut Vec<Section>, end_seconds: f64, songend_seconds: f64) {
        if end_seconds > songend_seconds + 0.01 {
            let end_section_start = sections.last().map_or(songend_seconds, |s| s.end_seconds);
            debug!(
                "Adding END section: start={:.3} end={:.3} (last_section_end={:.3}, marker_songend={:.3})",
                end_section_start, end_seconds, end_section_start, songend_seconds
            );
            sections.push(Section {
                section_id: SectionId::new(),
                id: None,
                name: "End".to_string(),
                comment: None,
                section_type: SectionType::End,
                start_seconds: end_section_start,
                end_seconds,
                number: None,
                color: None,
            });
        }
    }

    /// Extract non-structural comment markers within a time range.
    fn extract_comments_in_range(markers: &[Marker], start: f64, end: f64) -> Vec<Comment> {
        let mut comments: Vec<Comment> = markers
            .iter()
            .filter(|m| {
                let pos = position_to_seconds(&m.position);
                let in_bounds = pos >= start && pos <= end;
                let is_structural = Self::is_structural_marker(&m.name);
                in_bounds && !is_structural
            })
            .map(Self::marker_to_comment)
            .collect();

        comments.sort_by(|a, b| {
            a.position_seconds
                .partial_cmp(&b.position_seconds)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        comments
    }

    /// Extract comments with support for mid-song count-in markers.
    fn extract_comments_with_mid_song_count_in(
        markers: &[Marker],
        start: f64,
        end: f64,
        count_in_is_mid_song: bool,
    ) -> Vec<Comment> {
        let mut comments: Vec<Comment> = markers
            .iter()
            .filter(|m| {
                let pos = position_to_seconds(&m.position);
                let in_bounds = pos >= start && pos <= end;
                let is_structural = Self::is_structural_marker(&m.name);
                let is_mid_song_count_in =
                    Self::is_count_in_marker(&m.name) && count_in_is_mid_song;
                in_bounds && (!is_structural || is_mid_song_count_in)
            })
            .map(Self::marker_to_comment)
            .collect();

        comments.sort_by(|a, b| {
            a.position_seconds
                .partial_cmp(&b.position_seconds)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        comments
    }

    /// Parse project name to extract song title and artist
    ///
    /// Format: "Title - Artist.rpp" or "Title - Artist"
    /// Returns: (`song_name`, Option<artist>)
    fn parse_project_name(name: &str) -> (String, Option<String>) {
        // Remove .rpp extension (case insensitive)
        let name = name
            .trim()
            .trim_end_matches(".rpp")
            .trim_end_matches(".RPP")
            .trim_end_matches(".Rpp");

        // Strip a leading zero-padded setlist-order prefix ("00 ", "01 ", …).
        // Per-song-project setups name projects `NN <Song>` so `Projects::list()`
        // (name-sorted) keeps the authored order; the index isn't part of the
        // song title. Only a 2–3 digit run followed by a space is stripped, so
        // real titles ("10,000 Reasons") are untouched.
        let name = Self::strip_order_prefix(name);

        // Look for " - " separator (with spaces around dash)
        name.find(" - ").map_or_else(
            || (name.to_string(), None),
            |sep_pos| {
                let title = name.get(..sep_pos).unwrap_or(name);
                let artist = name.get(sep_pos.saturating_add(3)..).unwrap_or("");
                let title_trimmed = title.trim();
                let artist_trimmed = artist.trim();

                if artist_trimmed.is_empty() {
                    (title_trimmed.to_string(), None)
                } else {
                    (title_trimmed.to_string(), Some(artist_trimmed.to_string()))
                }
            },
        )
    }

    /// Strip a leading `NN ` (2–3 digit, space) setlist-order prefix from a
    /// project name. Returns the input unchanged when there is no such prefix.
    fn strip_order_prefix(name: &str) -> &str {
        let digits = name.chars().take_while(char::is_ascii_digit).count();
        if (2..=3).contains(&digits)
            && let Some(after_digits) = name.get(digits..)
            && after_digits.starts_with(' ')
            && let Some(trimmed) = name.get(digits.saturating_add(1)..)
        {
            return trimmed.trim_start();
        }
        name
    }

    /// Check if a marker name indicates a count-in marker
    /// Supports: COUNTIN, COUNT-IN, COUNT IN, count in, count-in, `COUNT_IN`, etc.
    fn is_count_in_marker(name: &str) -> bool {
        let normalized = name.to_uppercase().replace(['-', ' ', '_'], "");
        normalized == "COUNTIN"
    }

    /// Check if a marker name indicates a SONGSTART marker
    fn is_songstart_marker(name: &str) -> bool {
        let upper = name.to_uppercase();
        upper == "SONGSTART"
            || upper.starts_with("SONGSTART ")
            || upper == "SONG START"
            || upper.starts_with("SONG START ")
    }

    /// Check if a marker name indicates a SONGEND marker
    fn is_songend_marker(name: &str) -> bool {
        let upper = name.to_uppercase();
        upper == "SONGEND"
            || upper.starts_with("SONGEND ")
            || upper == "SONG END"
            || upper.starts_with("SONG END ")
    }

    /// Check if a marker is a special marker (not a section marker)
    fn is_special_marker(name: &str) -> bool {
        Self::is_count_in_marker(name)
            || Self::is_songstart_marker(name)
            || Self::is_songend_marker(name)
            || name == "=START"
            || name == "=END"
            || name == "PREROLL"
            || name == "=PREROLL"
            || name == "POSTROLL"
            || name == "=POSTROLL"
            || name.starts_with("=SONGSTART")
            || name.starts_with("=SONGEND")
    }

    /// Check if a marker is a structural marker (used for song bounds, not for comments)
    /// This is similar to `is_special_marker` but includes COUNT-IN since it can
    /// appear as a comment when it's mid-song
    fn is_structural_marker(name: &str) -> bool {
        Self::is_count_in_marker(name)
            || Self::is_songstart_marker(name)
            || Self::is_songend_marker(name)
            || name == "=START"
            || name == "=END"
            || name == "PREROLL"
            || name == "=PREROLL"
            || name == "POSTROLL"
            || name == "=POSTROLL"
            || name.starts_with("=SONGSTART")
            || name.starts_with("=SONGEND")
    }

    /// Snap a time position to the next barline (measure boundary).
    ///
    /// Converts seconds → musical position (measure, beat, fraction), rounds up
    /// to the next measure, and converts back to seconds. If already exactly on
    /// a barline, returns the same position.
    async fn snap_to_next_barline(
        tempo_map: &daw::rpc::TempoMap,
        seconds: f64,
    ) -> eyre::Result<f64> {
        let (measure, beat, fraction) = tempo_map.time_to_musical(seconds).await?;
        // If already exactly on a barline (beat 1, no fraction), keep it
        if beat <= 1 && fraction < 0.001 {
            return Ok(seconds);
        }
        // Next barline = start of next measure
        let next_measure = measure.saturating_add(1);
        let snapped = tempo_map.musical_to_time(next_measure, 1, 0.0).await?;
        debug!(
            "snap_to_next_barline: {:.3}s (m{}.{}.{:.3}) → {:.3}s (m{})",
            seconds, measure, beat, fraction, snapped, next_measure
        );
        Ok(snapped)
    }

    fn snap_to_next_barline_native<D: SongDaw>(
        daw: &D,
        project: ProjectContext,
        seconds: f64,
    ) -> f64 {
        let (measure, beat, fraction) = daw.time_to_musical(project.clone(), seconds);
        if beat <= 1 && fraction < 0.001 {
            return seconds;
        }
        let next_measure = measure.saturating_add(1);
        let snapped = daw.musical_to_time(project, next_measure, 1, 0.0);
        debug!(
            "snap_to_next_barline_native: {:.3}s (m{}.{}.{:.3}) -> {:.3}s (m{})",
            seconds, measure, beat, fraction, snapped, next_measure
        );
        snapped
    }

    /// Find all regions that qualify as song regions.
    ///
    /// Prefers lane-based detection: regions in the SONG lane are song regions
    /// by definition. Falls back to containment heuristic (regions with ≥2
    /// children) for projects without lane info.
    ///
    /// Returns leaf-level parent regions only, sorted by start time.
    fn find_song_regions<'a>(regions: &'a [Region], lanes: &ResolvedLanes) -> Vec<&'a Region> {
        // Prefer lane-based detection if SONG lane is known
        let mut candidates: Vec<&Region> = lanes.song.map_or_else(Vec::new, |song_lane| {
            let lane_matches: Vec<&Region> = regions
                .iter()
                .filter(|r| r.lane == Some(song_lane))
                .collect();
            if lane_matches.len() >= 2 {
                debug!(
                    "find_song_regions: {} regions in SONG lane (index {})",
                    lane_matches.len(),
                    song_lane
                );
                lane_matches
            } else {
                Vec::new() // not enough — fall through to containment
            }
        });

        if candidates.is_empty() {
            // Fallback: containment heuristic for projects without lane info
            candidates = regions
                .iter()
                .filter(|region| {
                    let contained_count = regions
                        .iter()
                        .filter(|r| {
                            r.id != region.id
                                && r.time_range.start_seconds() >= region.time_range.start_seconds()
                                && r.time_range.end_seconds() <= region.time_range.end_seconds()
                        })
                        .count();
                    contained_count >= 2
                })
                .collect();

            // Remove grandparents — any candidate that contains another candidate.
            let grandparent_ids: Vec<_> = candidates
                .iter()
                .filter(|candidate| {
                    candidates.iter().any(|other| {
                        other.id != candidate.id
                            && other.time_range.start_seconds()
                                >= candidate.time_range.start_seconds()
                            && other.time_range.end_seconds() <= candidate.time_range.end_seconds()
                            && (other.time_range.start_seconds()
                                > candidate.time_range.start_seconds()
                                || other.time_range.end_seconds()
                                    < candidate.time_range.end_seconds())
                    })
                })
                .map(|r| r.id)
                .collect();
            candidates.retain(|r| !grandparent_ids.contains(&r.id));
        }

        // Sort by start time
        candidates.sort_by(|a, b| {
            a.time_range
                .start_seconds()
                .partial_cmp(&b.time_range.start_seconds())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        candidates
    }

    /// Find the song region — the single region that wraps all sections.
    ///
    /// Prefers lane-based detection (SONG lane), falls back to containment heuristic.
    fn find_song_region<'a>(regions: &'a [Region], lanes: &ResolvedLanes) -> Option<&'a Region> {
        // Prefer SONG-lane region if exactly one exists
        if let Some(song_lane) = lanes.song {
            let song_lane_regions: Vec<&Region> = regions
                .iter()
                .filter(|r| r.lane == Some(song_lane))
                .collect();
            if song_lane_regions.len() == 1 {
                return song_lane_regions.first().copied();
            }
        }

        // Fallback: region containing the most other regions
        let mut best_region: Option<&Region> = None;
        let mut best_count = 0;

        for region in regions {
            let contained_count = regions
                .iter()
                .filter(|r| {
                    r.id != region.id
                        && r.time_range.start_seconds() >= region.time_range.start_seconds()
                        && r.time_range.end_seconds() <= region.time_range.end_seconds()
                })
                .count();

            if contained_count > best_count {
                best_count = contained_count;
                best_region = Some(region);
            }
        }

        if best_count > 0 { best_region } else { None }
    }

    /// Build sections from markers (when no regions exist)
    ///
    /// Each marker defines the start of a section, ending at the next marker.
    fn build_sections_from_markers(
        markers: &[Marker],
        song_start: f64,
        song_end: f64,
    ) -> Vec<Section> {
        // Filter to section markers within song bounds (excluding special markers)
        let mut section_markers: Vec<&Marker> = markers
            .iter()
            .filter(|m| {
                let pos = position_to_seconds(&m.position);
                pos >= song_start && pos < song_end && !Self::is_special_marker(&m.name)
            })
            .collect();

        // Sort by position
        section_markers.sort_by(|a, b| {
            position_to_seconds(&a.position)
                .partial_cmp(&position_to_seconds(&b.position))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut sections = Vec::new();

        for (idx, marker) in section_markers.iter().enumerate() {
            let start = position_to_seconds(&marker.position);

            // End is at the next marker or song end
            let end = section_markers
                .get(idx.saturating_add(1))
                .map_or(song_end, |next_marker| {
                    position_to_seconds(&next_marker.position)
                });

            let (section_type, number, clean_name, comment) =
                Self::parse_section_name(&marker.name);

            sections.push(Section {
                section_id: SectionId::new(),
                id: marker.id,
                name: clean_name,
                comment,
                section_type,
                start_seconds: start,
                end_seconds: end,
                number,
                color: marker.color,
            });
        }

        sections
    }

    /// Extract sections from regions contained within the song region
    fn extract_sections_from_song_region(
        regions: &[Region],
        song_region: &Region,
        lanes: &ResolvedLanes,
    ) -> Vec<Section> {
        let song_start = song_region.time_range.start_seconds();
        let song_end = song_region.time_range.end_seconds();

        let mut sections: Vec<Section> = regions
            .iter()
            .filter(|r| {
                r.id != song_region.id
                    && r.time_range.start_seconds() >= song_start
                    && r.time_range.end_seconds() <= song_end
                    // Exclude other SONG-lane regions (they're sibling songs, not sections)
                    && !lanes.is_song_lane(r)
            })
            .map(|r| {
                let (section_type, number, clean_name, comment) = Self::parse_section_name(&r.name);
                Section {
                    section_id: SectionId::new(),
                    id: r.id,
                    name: clean_name,
                    comment,
                    section_type,
                    start_seconds: r.time_range.start_seconds(),
                    end_seconds: r.time_range.end_seconds(),
                    number,
                    color: r.color,
                }
            })
            .collect();

        sections.sort_by(|a, b| {
            a.start_seconds
                .partial_cmp(&b.start_seconds)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        sections
    }

    /// Extract sections from regions within song bounds (fallback when no song region)
    fn extract_sections_from_regions(regions: &[Region], start: f64, end: f64) -> Vec<Section> {
        let mut sections: Vec<Section> = regions
            .iter()
            .filter(|r| r.time_range.start_seconds() >= start && r.time_range.end_seconds() <= end)
            .filter(|r| !r.name.starts_with("SONG:") && r.name.to_uppercase() != "SONG")
            .map(|r| {
                let (section_type, number, clean_name, comment) = Self::parse_section_name(&r.name);
                Section {
                    section_id: SectionId::new(),
                    id: r.id,
                    name: clean_name,
                    comment,
                    section_type,
                    start_seconds: r.time_range.start_seconds(),
                    end_seconds: r.time_range.end_seconds(),
                    number,
                    color: r.color,
                }
            })
            .collect();

        sections.sort_by(|a, b| {
            a.start_seconds
                .partial_cmp(&b.start_seconds)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        sections
    }

    /// Calculate measure positions for the song
    fn calculate_measure_positions(
        start_seconds: f64,
        end_seconds: f64,
        bpm: f64,
        ts: daw::service::TimeSignature,
    ) -> Vec<daw::service::Position> {
        let beats_per_measure = f64::from(ts.numerator());
        let seconds_per_beat = 60.0 / bpm;
        let measure_duration = beats_per_measure * seconds_per_beat;

        let song_duration = end_seconds - start_seconds;
        let count_f64 = (song_duration / measure_duration).ceil().max(1.0);
        let measure_count = if count_f64.is_finite() && count_f64 <= f64::from(i32::MAX) {
            // Range-checked above; std has no non-`as` float-to-int conversion.
            #[allow(
                clippy::as_conversions,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss
            )]
            {
                count_f64 as i32
            }
        } else {
            i32::MAX
        };

        (0..measure_count)
            .map(|idx| {
                let time_seconds = f64::from(idx).mul_add(measure_duration, start_seconds);
                daw::service::Position::from_time(daw::service::TimePosition::from_seconds(
                    time_seconds,
                ))
            })
            .collect()
    }

    /// Parse section type, number, name (without comment), and optional comment from region/marker name
    ///
    /// Supports formats like:
    /// - "Verse 1" -> (Verse, Some(1), "Verse 1", None)
    /// - "Interlude C" -> (Instrumental, Some(3), "Interlude C", None) // C=3rd variant
    /// - `Interlude C "Woodwinds"` -> (Instrumental, Some(3), "Interlude C", Some("Woodwinds"))
    /// - `Chorus 2 "Big Build"` -> (Chorus, Some(2), "Chorus 2", Some("Big Build"))
    fn parse_section_name(name: &str) -> (SectionType, Option<u32>, String, Option<String>) {
        // First, extract any quoted/bracketed comment
        let (name_without_comment, comment) = Self::extract_comment(name);

        let name_upper = name_without_comment.to_uppercase();
        let name_trimmed = name_upper.trim();

        let (type_part, number) = Self::extract_type_and_number(name_trimmed);
        // Pass the original case name part for Custom types
        let original_type_part = Self::extract_original_type_part(name_without_comment.trim());
        let section_type = Self::parse_section_type_with_original(type_part, &original_type_part);

        // Use the original name (without comment) preserving case
        let clean_name = name_without_comment.trim().to_string();

        (section_type, number, clean_name, comment)
    }

    /// Extract the type part from the original (non-uppercased) name
    /// This preserves case for Custom section types
    fn extract_original_type_part(name: &str) -> String {
        // Try "Type Number" format (e.g., "Verse 1", "CH 2")
        if let Some(last_space) = name.rfind(' ')
            && let Some(potential_suffix) = name.get(last_space.saturating_add(1)..)
            && (potential_suffix.parse::<u32>().is_ok()
                || (potential_suffix.len() == 1
                    && potential_suffix
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_uppercase())))
            && let Some(type_part) = name.get(..last_space)
        {
            return type_part.trim().to_string();
        }

        // Try concatenated format (e.g., "V1", "CH2")
        let mut num_start = name.len();
        for (i, c) in name.chars().rev().enumerate() {
            let pos = name.len().saturating_sub(1).saturating_sub(i);
            if c.is_ascii_digit() {
                num_start = pos;
            } else if num_start != name.len() {
                break;
            }
        }

        if num_start < name.len() {
            name.get(..num_start).map_or_else(
                || name.to_string(),
                |type_part| type_part.trim().to_string(),
            )
        } else {
            name.to_string()
        }
    }

    /// Extract a comment/descriptor from a section name
    ///
    /// Supports multiple delimiter styles:
    /// - Double quotes: `Interlude C "Woodwinds"` -> ("Interlude C", "Woodwinds")
    /// - Curly braces: `Riff {Back In}` -> ("Riff", "Back In")
    /// - Parentheses: `Verse 1 (Acoustic)` -> ("Verse 1", "Acoustic")
    ///
    /// Returns (`name_without_comment`, `optional_comment`)
    fn extract_comment(name: &str) -> (&str, Option<String>) {
        let name = name.trim();

        // Try double quotes first: `Something "Comment"`
        if let Some(last_quote) = name.rfind('"')
            && let Some(open_quote) = name.get(..last_quote).and_then(|s| s.rfind('"'))
            && let Some(comment_str) = name.get(open_quote.saturating_add(1)..last_quote)
        {
            let comment = comment_str.trim();
            if let Some(name_part) = name.get(..open_quote)
                && !comment.is_empty()
            {
                return (name_part.trim(), Some(comment.to_string()));
            }
        }

        // Try curly braces: `Something {Comment}`
        if let Some(close_brace) = name.rfind('}')
            && let Some(open_brace) = name.get(..close_brace).and_then(|s| s.rfind('{'))
            && let Some(comment_str) = name.get(open_brace.saturating_add(1)..close_brace)
        {
            let comment = comment_str.trim();
            if let Some(name_part) = name.get(..open_brace)
                && !comment.is_empty()
            {
                return (name_part.trim(), Some(comment.to_string()));
            }
        }

        // Try parentheses: `Something (Comment)`
        // Only if it looks like a descriptor (not a number like "Verse (1)")
        if let Some(close_paren) = name.rfind(')')
            && let Some(open_paren) = name.get(..close_paren).and_then(|s| s.rfind('('))
            && let Some(comment_str) = name.get(open_paren.saturating_add(1)..close_paren)
        {
            let comment = comment_str.trim();
            // Only treat as comment if it's not just a number
            if let Some(name_part) = name.get(..open_paren)
                && !comment.is_empty()
                && !comment.chars().all(|c| c.is_ascii_digit())
            {
                return (name_part.trim(), Some(comment.to_string()));
            }
        }

        (name, None)
    }

    /// Extract the type part and optional number from a section name
    fn extract_type_and_number(name: &str) -> (&str, Option<u32>) {
        // Try "Type Number" format (e.g., "Verse 1", "CH 2")
        if let Some(last_space) = name.rfind(' ')
            && let Some(potential_num) = name.get(last_space.saturating_add(1)..)
        {
            if let Ok(num) = potential_num.parse::<u32>()
                && let Some(type_part) = name.get(..last_space)
            {
                return (type_part, Some(num));
            }
            // Try single letter variant (A=1, B=2, C=3, etc.) for cases like "Interlude C"
            if potential_num.len() == 1
                && let Some(c) = potential_num.chars().next()
                && c.is_ascii_uppercase()
            {
                // A=1, B=2, C=3, etc.
                let num = u32::from(c)
                    .saturating_sub(u32::from('A'))
                    .saturating_add(1);
                if let Some(type_part) = name.get(..last_space) {
                    return (type_part, Some(num));
                }
            }
        }

        // Try concatenated format (e.g., "V1", "CH2", "VS1A")
        // Handle letter suffix after number (e.g., "1A" -> 1)
        let mut num_start = name.len();
        let mut num_end = name.len();

        for (i, c) in name.chars().rev().enumerate() {
            let pos = name.len().saturating_sub(1).saturating_sub(i);
            if c.is_ascii_digit() {
                num_start = pos;
                if num_end == name.len() {
                    num_end = pos.saturating_add(1);
                }
            } else if num_end != name.len() {
                // Found non-digit after finding digits, stop
                break;
            }
        }

        if num_start < num_end
            && let Some(num_str) = name.get(num_start..num_end)
            && let Ok(num) = num_str.parse::<u32>()
            && let Some(type_part) = name.get(..num_start)
        {
            return (type_part, Some(num));
        }

        (name, None)
    }

    /// Parse section type from the type part of the name, preserving original case for Custom
    ///
    /// Uses keyflow-proto's `SectionType` parsing with fallback for session-specific patterns.
    fn parse_section_type_with_original(type_part: &str, original_type_part: &str) -> SectionType {
        let s = type_part.trim().to_lowercase();

        // Handle pre/post modifiers first
        if s.starts_with("pre-") || s.starts_with("pre ") {
            let rest = s
                .trim_start_matches("pre-")
                .trim_start_matches("pre ")
                .trim();
            // Try to parse the inner type
            if let Ok(inner) = SectionType::parse(rest) {
                return SectionType::Pre(Box::new(inner));
            }
            // Default pre-chorus for ambiguous cases
            if rest == "chorus" || rest == "ch" || rest == "c" || rest.is_empty() {
                return SectionType::Pre(Box::new(SectionType::Chorus));
            }
        }

        if s.starts_with("post-") || s.starts_with("post ") {
            let rest = s
                .trim_start_matches("post-")
                .trim_start_matches("post ")
                .trim();
            if let Ok(inner) = SectionType::parse(rest) {
                return SectionType::Post(Box::new(inner));
            }
            if rest == "chorus" || rest == "ch" || rest == "c" || rest.is_empty() {
                return SectionType::Post(Box::new(SectionType::Chorus));
            }
        }

        // Try keyflow's parser first (handles most cases including fuzzy matching)
        if let Ok(section_type) = SectionType::parse(&s) {
            return section_type;
        }

        // Handle session-specific variations not in keyflow
        match s.as_str() {
            // Pre-chorus shorthand
            "prechorus" | "pre-chorus" | "pre chorus" | "pc" => {
                SectionType::Pre(Box::new(SectionType::Chorus))
            }

            // Single-letter DAW-region abbreviations ("V2", "C1"). keyflow's
            // format parser deliberately rejects these (too ambiguous in chart
            // source), but they're idiomatic REAPER region names.
            "v" => SectionType::Verse,
            "c" => SectionType::Chorus,
            "b" => SectionType::Bridge,

            // Build is sometimes used as breakdown
            "build" => SectionType::Breakdown,

            // Unknown - use Custom with original case preserved
            _ => SectionType::Custom(original_type_part.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_project_name() {
        let (name, artist) = SongBuilder::parse_project_name("Cryin' - Mateus Asato.rpp");
        assert_eq!(name, "Cryin'");
        assert_eq!(artist, Some("Mateus Asato".to_string()));

        let (name, artist) = SongBuilder::parse_project_name("My Song.rpp");
        assert_eq!(name, "My Song");
        assert_eq!(artist, None);

        let (name, artist) = SongBuilder::parse_project_name("Another Song - The Artist");
        assert_eq!(name, "Another Song");
        assert_eq!(artist, Some("The Artist".to_string()));
    }

    #[test]
    fn test_parse_section_name() {
        // Basic section types with numbers
        let (section_type, number, name, comment) = SongBuilder::parse_section_name("Verse 1");
        assert_eq!(section_type, SectionType::Verse);
        assert_eq!(number, Some(1));
        assert_eq!(name, "Verse 1");
        assert_eq!(comment, None);

        let (section_type, number, name, comment) = SongBuilder::parse_section_name("V2");
        assert_eq!(section_type, SectionType::Verse);
        assert_eq!(number, Some(2));
        assert_eq!(name, "V2");
        assert_eq!(comment, None);

        let (section_type, number, name, comment) = SongBuilder::parse_section_name("VS 1A");
        assert_eq!(section_type, SectionType::Verse);
        assert_eq!(number, Some(1));
        assert_eq!(name, "VS 1A");
        assert_eq!(comment, None);

        let (section_type, number, name, comment) = SongBuilder::parse_section_name("VS 1B");
        assert_eq!(section_type, SectionType::Verse);
        assert_eq!(number, Some(1));
        assert_eq!(name, "VS 1B");
        assert_eq!(comment, None);

        let (section_type, number, name, comment) = SongBuilder::parse_section_name("CH 1");
        assert_eq!(section_type, SectionType::Chorus);
        assert_eq!(number, Some(1));
        assert_eq!(name, "CH 1");
        assert_eq!(comment, None);

        let (section_type, number, name, comment) = SongBuilder::parse_section_name("INST");
        assert_eq!(section_type, SectionType::Instrumental);
        assert_eq!(number, None);
        assert_eq!(name, "INST");
        assert_eq!(comment, None);

        let (section_type, number, name, comment) = SongBuilder::parse_section_name("GTR SOLO");
        assert_eq!(section_type, SectionType::Custom("GTR SOLO".to_string()));
        assert_eq!(number, None);
        assert_eq!(name, "GTR SOLO");
        assert_eq!(comment, None);

        let (section_type, number, name, comment) = SongBuilder::parse_section_name("SYNTH SOLO");
        assert_eq!(section_type, SectionType::Custom("SYNTH SOLO".to_string()));
        assert_eq!(number, None);
        assert_eq!(name, "SYNTH SOLO");
        assert_eq!(comment, None);
    }

    #[test]
    fn test_parse_section_name_with_comment() {
        // Section with quoted comment - "C" is interpreted as variant 3 (A=1, B=2, C=3)
        let (section_type, number, name, comment) =
            SongBuilder::parse_section_name(r#"Interlude C "Woodwinds""#);
        assert_eq!(section_type, SectionType::Interlude);
        assert_eq!(number, Some(3)); // C = 3rd variant
        assert_eq!(name, "Interlude C");
        assert_eq!(comment, Some("Woodwinds".to_string()));

        // Chorus with comment
        let (section_type, number, name, comment) =
            SongBuilder::parse_section_name(r#"Chorus 2 "Big Build""#);
        assert_eq!(section_type, SectionType::Chorus);
        assert_eq!(number, Some(2));
        assert_eq!(name, "Chorus 2");
        assert_eq!(comment, Some("Big Build".to_string()));

        // Bridge with descriptive comment
        let (section_type, number, name, comment) =
            SongBuilder::parse_section_name(r#"Bridge "Key Change to Eb""#);
        assert_eq!(section_type, SectionType::Bridge);
        assert_eq!(number, None);
        assert_eq!(name, "Bridge");
        assert_eq!(comment, Some("Key Change to Eb".to_string()));

        // Verse with instrument indication
        let (section_type, number, name, comment) =
            SongBuilder::parse_section_name(r#"Verse 1 "Guitar Solo""#);
        assert_eq!(section_type, SectionType::Verse);
        assert_eq!(number, Some(1));
        assert_eq!(name, "Verse 1");
        assert_eq!(comment, Some("Guitar Solo".to_string()));
    }

    #[test]
    fn test_parse_section_name_with_curly_braces() {
        // Curly braces descriptor: Riff {Back In}
        // "Riff" is not a standard section type, so it becomes Custom
        let (section_type, number, name, comment) =
            SongBuilder::parse_section_name("Riff {Back In}");
        assert_eq!(section_type, SectionType::Custom("Riff".to_string()));
        assert_eq!(number, None);
        assert_eq!(name, "Riff");
        assert_eq!(comment, Some("Back In".to_string()));

        // Verse with curly braces
        let (section_type, number, name, comment) =
            SongBuilder::parse_section_name("Verse 1 {Acoustic}");
        assert_eq!(section_type, SectionType::Verse);
        assert_eq!(number, Some(1));
        assert_eq!(name, "Verse 1");
        assert_eq!(comment, Some("Acoustic".to_string()));

        // Outro with descriptor
        let (section_type, number, name, comment) =
            SongBuilder::parse_section_name("Outro {Fade Out}");
        assert_eq!(section_type, SectionType::Outro);
        assert_eq!(number, None);
        assert_eq!(name, "Outro");
        assert_eq!(comment, Some("Fade Out".to_string()));
    }

    #[test]
    fn test_parse_section_name_with_parentheses() {
        // Parentheses descriptor
        let (section_type, number, name, comment) =
            SongBuilder::parse_section_name("Intro (Keys Only)");
        assert_eq!(section_type, SectionType::Intro);
        assert_eq!(number, None);
        assert_eq!(name, "Intro");
        assert_eq!(comment, Some("Keys Only".to_string()));

        // Verse with number in parentheses - number is still extracted from name
        // The (1) is not treated as a comment since it's numeric-only
        // But the number extraction logic still finds the 1
        let (section_type, number, name, comment) = SongBuilder::parse_section_name("Verse (1)");
        assert_eq!(section_type, SectionType::Verse);
        assert_eq!(number, Some(1)); // Number extracted from (1)
        assert_eq!(name, "Verse (1)"); // Name preserved (no comment extracted)
        assert_eq!(comment, None); // Numeric-only parentheses are not comments

        // Bridge with descriptive parentheses
        let (section_type, number, name, comment) =
            SongBuilder::parse_section_name("Bridge 2 (Half Time)");
        assert_eq!(section_type, SectionType::Bridge);
        assert_eq!(number, Some(2));
        assert_eq!(name, "Bridge 2");
        assert_eq!(comment, Some("Half Time".to_string()));
    }

    #[test]
    fn test_is_special_marker() {
        assert!(SongBuilder::is_special_marker("COUNT-IN"));
        assert!(SongBuilder::is_special_marker("SONGSTART"));
        assert!(SongBuilder::is_special_marker("SONGEND"));
        assert!(SongBuilder::is_special_marker("=START"));
        assert!(SongBuilder::is_special_marker("=END"));
        assert!(SongBuilder::is_special_marker("PREROLL"));
        assert!(SongBuilder::is_special_marker("=PREROLL"));
        assert!(SongBuilder::is_special_marker("POSTROLL"));
        assert!(SongBuilder::is_special_marker("=POSTROLL"));
        assert!(!SongBuilder::is_special_marker("Intro"));
        assert!(!SongBuilder::is_special_marker("VS 1A"));
        assert!(!SongBuilder::is_special_marker("CH 1"));
    }

    #[test]
    fn test_is_structural_marker() {
        assert!(SongBuilder::is_structural_marker("PREROLL"));
        assert!(SongBuilder::is_structural_marker("=PREROLL"));
        assert!(SongBuilder::is_structural_marker("POSTROLL"));
        assert!(SongBuilder::is_structural_marker("=POSTROLL"));
    }
}
