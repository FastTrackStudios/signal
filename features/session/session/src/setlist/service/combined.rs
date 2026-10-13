//! Combined setlist generation — merges all open song projects into a single
//! REAPER project with songs laid out sequentially on the timeline.
//!
//! Uses the `combine_rpp_files` pipeline from the daw crate to handle:
//! - PREROLL/POSTROLL bounds resolution (trimming)
//! - Guide track merging (Click, Loop, Count, Guide → shared header)
//! - Per-song folder creation under TRACKS/
//! - Tempo envelope concatenation
//! - Marker/region offset + lane classification
//!
//! After the daw crate produces the combined RPP text, post-processing steps
//! run via the Daw facade on the opened project:
//! - Mark as combined setlist (`ExtState`)
//! - Wire routing receives (future: when routing folder is added at RPP level)

use crate::setlist::service::SetlistServiceImpl;
use daw::rpc::Daw;
use daw::service::{ExtState, ProjectContext, Projects};
use session_proto::SessionServiceError;
use std::path::PathBuf;
use tracing::{debug, info};

/// `ExtState` section/key used to identify combined setlist projects.
const COMBINED_EXT_SECTION: &str = "FTS";
const COMBINED_EXT_KEY: &str = "is_combined_setlist";

/// `ExtState` keys for sync group identity.
/// Written to every project tab involved in a setlist so they can find each other.
const SYNC_SECTION: &str = "FTS_SYNC";
const SYNC_KEY_SETLIST_ID: &str = "setlist_id";
const SYNC_KEY_SONG_INDEX: &str = "song_index";
const SYNC_KEY_SETLIST_PATH: &str = "setlist_path";
const SYNC_KEY_SONG_COUNT: &str = "song_count";

impl<D> SetlistServiceImpl<D>
where
    D: ExtState + Projects + Clone + architect::MaybeSendSync + 'static,
{
    /// Generate a combined setlist project from open song projects.
    ///
    /// Pipeline:
    /// 1. Save all open projects to disk
    /// 2. Enumerate projects, skip combined setlists and routing projects
    /// 3. Collect RPP file paths
    /// 4. Combine via `combine_rpp_files` (handles bounds, guide merging, folders, tempo, markers)
    /// 5. Write combined RPP to disk
    /// 6. Open in REAPER as a new tab
    /// 7. Post-process: mark as combined, wire routing receives
    ///
    /// Returns the GUID of the newly opened combined project.
    pub(crate) async fn generate_combined_setlist_impl(
        &self,
        gap_measures: u32,
    ) -> Result<String, SessionServiceError> {
        // ── 1. Save all open projects to ensure RPP files are current ─
        //
        // `Projects::save_all` / `list` / `ExtState::get_project` on
        // `daw_reaper::Reaper` hit main-thread-only REAPER FFI. From this
        // async RPC handler they hang on REAPER's internal lock. Bounce
        // each phase through `daw_proto::main_thread::query`.
        info!("Saving all open projects before generating combined setlist...");
        {
            let daw = self.daw.clone();
            daw_proto::main_thread::query(move || daw.save_all()).await;
        }

        // Small delay to let REAPER finish writing files
        architect::platform::sleep(std::time::Duration::from_millis(500)).await;

        // ── 2. Collect RPP paths from open projects ─────────
        let rpp_paths = self.collect_rpp_paths().await?;

        // ── 3-4. Combine and write RPP files ─────────
        let (output_path, song_infos) = Self::combine_and_write_rpp(&rpp_paths, gap_measures)?;

        // ── 5. Open in REAPER as a new tab ────────────────────────────
        let new_project = {
            let daw = self.daw.clone();
            let output_path_str = output_path.to_string_lossy().to_string();
            daw_proto::main_thread::query(move || daw.open(&output_path_str))
                .await
                .flatten()
                .ok_or_else(|| {
                    SessionServiceError::DawError("Failed to open combined setlist".to_string())
                })?
        };

        // ── 6. Post-process: mark all projects with sync identity ──────
        let setlist_id = uuid::Uuid::new_v4().to_string();
        let setlist_path_str = output_path.to_string_lossy().to_string();
        let guid = new_project.guid.clone();
        let song_idx = self
            .mark_projects_with_sync_identity(
                &guid,
                rpp_paths.len(),
                &setlist_id,
                &setlist_path_str,
            )
            .await;

        // ── 7. Start bidirectional position sync ────────────────────────
        self.setup_position_sync_bridge(&guid, &song_infos).await;

        info!(
            "Combined setlist opened: {} (setlist_id={}, {} songs tagged)",
            guid, setlist_id, song_idx
        );

        Ok(guid)
    }

    fn combine_and_write_rpp(
        rpp_paths: &[PathBuf],
        gap_measures: u32,
    ) -> Result<(PathBuf, Vec<daw::file::setlist_rpp::SongInfo>), SessionServiceError> {
        // Combine RPP files using the daw crate pipeline.
        // This handles everything at the RPP level:
        // - Resolves song bounds (PREROLL → POSTROLL priority chain)
        // - Merges guide tracks (Click, Loop, Count, Guide) into shared header
        // - Creates per-song folders under TRACKS/
        // - Concatenates tempo envelopes with square-shape boundaries
        // - Offsets markers/regions with lane classification
        // - Resolves relative media paths to absolute
        let options = daw::file::setlist_rpp::CombineOptions {
            gap_measures,
            trim_to_bounds: true,
        };
        let (combined_rpp, song_infos) =
            daw::file::setlist_rpp::combine_rpp_files(rpp_paths, &options).map_err(|e| {
                SessionServiceError::Internal(format!("Failed to combine RPP files: {e}"))
            })?;

        info!(
            "Combined {} songs: {}",
            song_infos.len(),
            song_infos
                .iter()
                .map(|s| format!("{} ({:.1}s)", s.name, s.duration_seconds))
                .collect::<Vec<_>>()
                .join(", ")
        );

        // Write combined RPP to disk
        let output_dir = rpp_paths
            .first()
            .and_then(|p| p.parent())
            .map_or_else(std::env::temp_dir, std::path::Path::to_path_buf);
        let output_path = output_dir.join("Combined Setlist.RPP");

        std::fs::write(&output_path, &combined_rpp).map_err(|e| {
            SessionServiceError::Internal(format!(
                "Failed to write combined RPP to {}: {e}",
                output_path.display()
            ))
        })?;

        info!(
            "Combined setlist RPP written: {} ({:.0} bytes)",
            output_path.display(),
            combined_rpp.len(),
        );

        Ok((output_path, song_infos))
    }

    async fn collect_rpp_paths(&self) -> Result<Vec<PathBuf>, SessionServiceError> {
        // ── Get all open projects, skip combined setlists ─────────
        let projects_with_flags = {
            let daw = self.daw.clone();
            daw_proto::main_thread::query(move || {
                daw.list()
                    .into_iter()
                    .map(|project| {
                        let ctx = ProjectContext::Project(project.guid.clone());
                        let is_combined = daw
                            .get_project(ctx.clone(), COMBINED_EXT_SECTION, COMBINED_EXT_KEY)
                            .is_some_and(|v| v == "1");
                        let is_routing = daw
                            .get_project(
                                ctx,
                                session_proto::routing_project::EXT_STATE_SECTION,
                                session_proto::routing_project::EXT_STATE_KEY_IS_ROUTING,
                            )
                            .is_some_and(|v| v == "1");
                        (project, is_combined, is_routing)
                    })
                    .collect::<Vec<_>>()
            })
            .await
            .ok_or_else(|| {
                SessionServiceError::Internal(
                    "main thread unavailable (TaskSupport not initialised)".to_string(),
                )
            })?
        };

        let mut rpp_paths = Vec::new();
        for (project, is_combined, is_routing) in &projects_with_flags {
            if *is_combined {
                debug!("Skipping combined setlist project: {}", project.guid);
                continue;
            }
            if *is_routing {
                debug!("Skipping routing project: {}", project.guid);
                continue;
            }
            if project.path.is_empty() {
                debug!("Skipping unsaved project: {}", project.name);
                continue;
            }
            rpp_paths.push(PathBuf::from(&project.path));
        }

        if rpp_paths.is_empty() {
            return Err(SessionServiceError::Internal(
                "No saved song projects found".to_string(),
            ));
        }

        info!(
            "Generating combined setlist from {} projects",
            rpp_paths.len()
        );

        Ok(rpp_paths)
    }

    async fn mark_projects_with_sync_identity(
        &self,
        new_project_guid: &str,
        song_count: usize,
        setlist_id: &str,
        setlist_path_str: &str,
    ) -> u32 {
        // Every project in the setlist gets a shared setlist_id so they
        // can find each other for sync (within the same REAPER instance
        // or across the network via mDNS). Batch all the marker ExtState
        // writes + the song-tag loop into one main-thread bounce.
        let daw = self.daw.clone();
        let new_project_guid = new_project_guid.to_string();
        let setlist_id = setlist_id.to_string();
        let setlist_path_str = setlist_path_str.to_string();
        let song_count_str = song_count.to_string();

        daw_proto::main_thread::query(move || {
            let new_project_ctx = ProjectContext::Project(new_project_guid.clone());

            // Mark the combined setlist project
            let _ = daw.set_project(
                new_project_ctx.clone(),
                COMBINED_EXT_SECTION,
                COMBINED_EXT_KEY,
                "1",
            );
            let _ = daw.set_project(
                new_project_ctx.clone(),
                SYNC_SECTION,
                SYNC_KEY_SETLIST_ID,
                &setlist_id,
            );
            let _ = daw.set_project(
                new_project_ctx.clone(),
                SYNC_SECTION,
                SYNC_KEY_SONG_COUNT,
                &song_count_str,
            );
            let _ = daw.set_project(
                new_project_ctx,
                SYNC_SECTION,
                SYNC_KEY_SETLIST_PATH,
                &setlist_path_str,
            );

            // Mark each individual song project with the same setlist_id + its index
            let mut song_idx = 0u32;
            for project in daw.list() {
                if project.guid == new_project_guid {
                    continue;
                }
                let ctx = ProjectContext::Project(project.guid.clone());
                let is_combined = daw
                    .get_project(ctx.clone(), COMBINED_EXT_SECTION, COMBINED_EXT_KEY)
                    .is_some_and(|v| v == "1");
                if is_combined {
                    continue;
                }
                let is_routing = daw
                    .get_project(
                        ctx.clone(),
                        session_proto::routing_project::EXT_STATE_SECTION,
                        session_proto::routing_project::EXT_STATE_KEY_IS_ROUTING,
                    )
                    .is_some_and(|v| v == "1");
                if is_routing {
                    continue;
                }
                if project.path.is_empty() {
                    continue;
                }

                let _ =
                    daw.set_project(ctx.clone(), SYNC_SECTION, SYNC_KEY_SETLIST_ID, &setlist_id);
                let _ = daw.set_project(
                    ctx.clone(),
                    SYNC_SECTION,
                    SYNC_KEY_SONG_INDEX,
                    &song_idx.to_string(),
                );
                let _ =
                    daw.set_project(ctx, SYNC_SECTION, SYNC_KEY_SETLIST_PATH, &setlist_path_str);

                debug!(
                    "Song {} ({}) tagged with setlist_id={}",
                    song_idx, project.name, setlist_id
                );
                song_idx = song_idx.saturating_add(1);
            }
            song_idx
        })
        .await
        .unwrap_or(0)
    }

    async fn setup_position_sync_bridge(
        &self,
        guid: &str,
        song_infos: &[daw::file::setlist_rpp::SongInfo],
    ) {
        use session_proto::SongId;
        use session_proto::offset_map::{SetlistOffsetMap, SongOffset};

        let offset_map = Self::build_offset_map(song_infos);
        let song_guids = self.fetch_song_project_guids(guid).await;

        // Update offset map with actual GUIDs
        let mut offset_map = offset_map;
        for (i, song) in offset_map.songs.iter_mut().enumerate() {
            if let Some(g) = song_guids.get(i) {
                song.project_guid.clone_from(g);
            }
        }

        let bridge = super::position_sync::PositionSyncBridge::new(
            offset_map.clone(),
            Some(guid.to_string()),
        );
        *self.position_sync.write().await = Some(bridge);

        self.spawn_position_sync_tick_loop();

        info!(
            "Position sync bridge started for {} songs",
            song_guids.len()
        );

        Self::spawn_live_daw_sync_if_reaper(guid, &offset_map);
    }

    fn build_offset_map(
        song_infos: &[daw::file::setlist_rpp::SongInfo],
    ) -> session_proto::offset_map::SetlistOffsetMap {
        use session_proto::SongId;
        use session_proto::offset_map::{SetlistOffsetMap, SongOffset};

        SetlistOffsetMap {
            songs: song_infos
                .iter()
                .enumerate()
                .map(|(i, si)| SongOffset {
                    index: i,
                    song_id: SongId::new(),
                    project_guid: String::new(), // Filled below from open projects
                    global_start_seconds: si.global_start_seconds,
                    global_start_qn: si.global_start_seconds * 2.0, // approximate
                    duration_seconds: si.duration_seconds,
                    duration_qn: si.duration_seconds * 2.0,
                    count_in_seconds: 0.0,
                    start_tempo: 120.0,
                    start_time_sig: daw::service::TimeSignature::new(4, 4),
                })
                .collect(),
            total_seconds: song_infos
                .last()
                .map_or(0.0, |s| s.global_start_seconds + s.duration_seconds),
            total_qn: 0.0,
        }
    }

    async fn fetch_song_project_guids(&self, guid: &str) -> Vec<String> {
        let daw = self.daw.clone();
        let guid_filter = guid.to_string();
        daw_proto::main_thread::query(move || {
            daw.list()
                .into_iter()
                .filter(|project| project.guid != guid_filter)
                .filter(|project| !project.path.is_empty())
                .filter(|project| {
                    daw.get_project(
                        ProjectContext::Project(project.guid.clone()),
                        COMBINED_EXT_SECTION,
                        COMBINED_EXT_KEY,
                    )
                    .is_none_or(|v| v != "1")
                })
                .map(|project| project.guid)
                .collect()
        })
        .await
        .unwrap_or_default()
    }

    fn spawn_position_sync_tick_loop(&self) {
        let position_sync = self.position_sync.clone();
        architect::platform::spawn(async move {
            let Some(daw) = daw::get().cloned() else {
                tracing::warn!("position sync loop skipped; daw facade is not initialized");
                return;
            };
            loop {
                architect::platform::sleep(std::time::Duration::from_millis(33)).await; // ~30Hz
                let mut guard = position_sync.write().await;
                if let Some(ref mut bridge) = *guard {
                    bridge.tick(&daw).await;
                } else {
                    break;
                }
            }
        });
    }

    /// Start the live DAW sync bridge (marker/region/item replication) —
    /// REAPER-only (cross-window sync has no standalone equivalent; a
    /// single daw-standalone process has no separate windows to sync).
    #[cfg(feature = "reaper")]
    fn spawn_live_daw_sync_if_reaper(
        guid: &str,
        offset_map: &session_proto::offset_map::SetlistOffsetMap,
    ) {
        let guid = guid.to_string();
        let offset_map = offset_map.clone();
        architect::platform::spawn(async move {
            // Collect song project handles for binding.
            let song_projects: Vec<(usize, daw::rpc::Project)> = {
                let mut sp = Vec::new();
                let daw = Daw::get();
                let all = daw.projects().await.unwrap_or_default();
                for project in all {
                    if project.guid() == guid {
                        continue;
                    }
                    // Check song_index from ExtState
                    let idx_str = project
                        .ext_state()
                        .get(SYNC_SECTION, SYNC_KEY_SONG_INDEX)
                        .await
                        .unwrap_or(None);
                    if let Some(idx_str) = idx_str
                        && let Ok(idx) = idx_str.parse::<usize>()
                    {
                        sp.push((idx, project));
                    }
                }
                sp
            };

            if !song_projects.is_empty() {
                let daw = Daw::get();
                let setlist_project = daw.project(&guid).await;
                if let Ok(setlist_project) = setlist_project {
                    let daw_sync = super::live_daw_sync::DawSyncBridge::new(
                        setlist_project,
                        song_projects,
                        &offset_map,
                    )
                    .await;
                    daw_sync.start().await;
                    info!("Live DAW sync bridge started");
                }
            }
        });
    }

    /// No-op: cross-window live DAW sync has no standalone equivalent.
    #[cfg(not(feature = "reaper"))]
    const fn spawn_live_daw_sync_if_reaper(
        _guid: &str,
        _offset_map: &session_proto::offset_map::SetlistOffsetMap,
    ) {
    }
}
