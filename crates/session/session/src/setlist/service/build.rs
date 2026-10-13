//! Setlist assembly from open DAW projects

use super::{HYDRATION_CONCURRENCY, SetlistServiceImpl};
use crate::setlist::service::hydration::ProjectLoad;
use daw::service::Projects;
// `Semaphore` + `JoinSet` back the native-only background hydration pass below.
use rustc_hash::{FxHashMap, FxHashSet};
use session_proto::{AdvanceMode, SessionServiceError, Setlist, Song};
use std::sync::Arc;
use std::sync::atomic::Ordering;
#[cfg(not(target_arch = "wasm32"))]
use tokio::sync::Semaphore;
#[cfg(not(target_arch = "wasm32"))]
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

impl<D> SetlistServiceImpl<D>
where
    // `MaybeSendSync` = `Send + Sync` on native (byte-for-byte the old
    // bound), empty on wasm — lets the browser's `!Send` Standalone backend
    // serve this in-process (see architect's LocalServer wasm support).
    D: Clone + Projects + architect::MaybeSendSync + 'static,
{
    /// Initialize caches and build maps for setlist construction.
    async fn initialize_build_context(
        &self,
        project_loads: &[ProjectLoad],
    ) -> (FxHashMap<String, Song>, FxHashMap<String, Song>) {
        let open_guids: FxHashSet<String> =
            project_loads.iter().map(|load| load.guid.clone()).collect();
        self.song_cache
            .retain(|guid, _| open_guids.contains(guid))
            .await;
        self.chart_cache
            .retain(|guid, _| open_guids.contains(guid))
            .await;
        self.last_chart_refresh_attempt
            .retain(|guid, _| open_guids.contains(guid))
            .await;

        let existing_setlist = self.setlist.read().await.clone();
        let existing_songs_by_guid: FxHashMap<String, Song> = existing_setlist
            .as_ref()
            .map(|sl| {
                sl.songs
                    .iter()
                    .cloned()
                    .map(|song| (song.project_guid.clone(), song))
                    .collect()
            })
            .unwrap_or_default();
        let cached_songs: FxHashMap<String, Song> = self
            .song_cache
            .with_read(|map| {
                map.iter()
                    .filter_map(|(guid, entry)| {
                        entry.songs.first().map(|s| (guid.clone(), s.clone()))
                    })
                    .collect()
            })
            .await;

        (existing_songs_by_guid, cached_songs)
    }

    /// Phase 2 of setlist building: hydrate remaining projects in background with bounded concurrency.
    ///
    /// # Errors
    ///
    /// May emit errors to tracing; hydration failures do not fail the build.
    fn spawn_phase2_hydration(
        &self,
        project_loads: Vec<ProjectLoad>,
        existing_songs_by_guid: FxHashMap<String, Song>,
        build_generation: u64,
        focused_index: usize,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        let this = Arc::new((*self).clone());
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move {
            let semaphore = Arc::new(Semaphore::new(HYDRATION_CONCURRENCY));
            let mut join_set = JoinSet::new();

            for load in project_loads {
                if load.index == focused_index {
                    continue;
                }

                let permit = semaphore.clone();
                let this_clone = this.clone();
                let existing_song = existing_songs_by_guid.get(&load.guid).cloned();
                join_set.spawn(async move {
                    let Ok(_permit) = permit.acquire_owned().await else {
                        return (load.guid.clone(), Vec::new());
                    };
                    let songs = this_clone
                        .build_songs_with_cache(&load, existing_song.as_ref())
                        .await;
                    (load.guid, songs)
                });
            }

            while let Some(result) = join_set.join_next().await {
                if this.build_generation.load(Ordering::SeqCst) != build_generation {
                    info!("Skipping stale hydration results from older build generation");
                    return;
                }

                match result {
                    Ok((project_guid, hydrated_songs)) if !hydrated_songs.is_empty() => {
                        for song in &hydrated_songs {
                            this.cache_chart_payload_for_song(song).await;
                        }

                        // Find the range of songs belonging to this project using a read lock.
                        let (first_idx, count) = {
                            let guard = this.setlist.read().await;
                            let Some(current_setlist) = guard.as_ref() else {
                                continue;
                            };
                            let first = current_setlist
                                .songs
                                .iter()
                                .position(|s| s.project_guid == project_guid);
                            let Some(first_idx) = first else { continue };
                            let count = current_setlist
                                .songs
                                .get(first_idx..)
                                .unwrap_or(&[])
                                .iter()
                                .take_while(|s| s.project_guid == project_guid)
                                .count();
                            drop(guard);
                            (first_idx, count)
                        };
                        // Read lock is released here.

                        // Compute indexed songs without holding any lock.
                        let end_idx = first_idx.saturating_add(count);
                        let indexed_songs: Vec<(usize, Song)> = hydrated_songs
                            .into_iter()
                            .map(|mut s| {
                                Self::strip_song_chart_payload(&mut s);
                                s
                            })
                            .enumerate()
                            .map(|(i, s)| (first_idx.saturating_add(i), s))
                            .collect();

                        // Now acquire write lock just for the splice.
                        {
                            let mut guard = this.setlist.write().await;
                            let Some(ref mut current_setlist) = *guard else {
                                return;
                            };
                            current_setlist.songs.splice(
                                first_idx..end_idx,
                                indexed_songs.iter().map(|(_, s): &(usize, Song)| s.clone()),
                            );
                            drop(guard);
                        }
                        // Write lock is released here.

                        this.notify_setlist_changed();
                        for (song_index, song) in indexed_songs.iter().map(|(i, s)| (i, s)) {
                            this.hydration_bus.emit((*song_index, song.clone()));
                            this.emit_cached_chart_payload_for_song(*song_index, &project_guid)
                                .await;
                        }
                    }
                    Ok((_project_guid, _)) => {
                        // Keep placeholder if build returned empty.
                    }
                    Err(e) => {
                        warn!("Song hydration worker failed: {}", e);
                    }
                }
            }
        });
    }

    /// Phase 1 of setlist building: synchronously build the focused project and placeholder others.
    ///
    /// # Panics
    ///
    /// Never panics in normal operation.
    async fn perform_phase1_build(
        &self,
        project_loads: &[ProjectLoad],
        existing_songs_by_guid: &FxHashMap<String, Song>,
        cached_songs: &FxHashMap<String, Song>,
        focused_index: usize,
    ) -> (Setlist, usize) {
        let mut songs = Vec::with_capacity(project_loads.len());
        let mut focused_song_count = 0usize;
        for load in project_loads {
            let existing_song = existing_songs_by_guid.get(&load.guid);
            let cached_song = cached_songs.get(&load.guid);

            let full_build = load.index == focused_index || cfg!(target_arch = "wasm32");
            if full_build {
                let project_songs = self.build_songs_with_cache(load, existing_song).await;
                if !project_songs.is_empty() {
                    for song in &project_songs {
                        self.cache_chart_payload_for_song(song).await;
                    }
                    let light_songs: Vec<Song> = project_songs
                        .into_iter()
                        .map(|mut s| {
                            Self::strip_song_chart_payload(&mut s);
                            s
                        })
                        .collect();
                    if load.index == focused_index {
                        focused_song_count = light_songs.len();
                    }
                    songs.extend(light_songs);
                    continue;
                }
            }

            songs.push(Self::make_name_only_song(
                &load.guid,
                &load.project_name,
                existing_song,
                cached_song,
            ));
        }

        let setlist = Setlist {
            id: None,
            name: format!("Setlist - {}", chrono::Local::now().format("%Y-%m-%d")),
            advance_mode: AdvanceMode::default(),
            songs,
        };

        (setlist, focused_song_count)
    }

    pub(crate) async fn build_from_open_projects_impl(&self) -> Result<(), SessionServiceError> {
        debug!("Building setlist from open projects...");

        let build_generation = self
            .build_generation
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);

        // `Projects::list` / `Projects::current` on `daw_reaper::Reaper`
        // hit REAPER's main-thread-only FFI (EnumProjects, etc.). Run
        // them from the async RPC handler and they hang waiting on
        // REAPER's internal lock. Bounce via `main_thread::query` —
        // same pattern mode/take/record services already use.
        let daw = self.daw.clone();
        let (projects, current_project_guid) = daw_proto::main_thread::query(move || {
            let projects = daw.list();
            let current = daw.current().map(|p| p.guid);
            (projects, current)
        })
        .await
        .ok_or_else(|| {
            SessionServiceError::Internal(
                "main thread unavailable (TaskSupport not initialised)".to_string(),
            )
        })?;

        let project_loads = Self::fetch_project_loads(projects).await;
        if project_loads.is_empty() {
            let empty = Setlist {
                id: None,
                name: "Empty Setlist".to_string(),
                advance_mode: AdvanceMode::default(),
                songs: Vec::new(),
            };
            *self.setlist.write().await = Some(empty.clone());
            self.notify_setlist_changed();
            return Ok(());
        }

        let (existing_songs_by_guid, cached_songs) =
            self.initialize_build_context(&project_loads).await;

        let focused_index = current_project_guid
            .as_ref()
            .and_then(|guid| project_loads.iter().position(|load| &load.guid == guid))
            .unwrap_or(0);

        // Phase 1: focused project full details, all others names only.
        // A single project may produce multiple songs (multi-song mode).
        //
        // Wasm exception: the browser engine has no Phase-2 background
        // hydration pass (below — it's tokio JoinSet, native-only), so a
        // name-only placeholder there would NEVER gain its sections/chart. The
        // in-browser setlist is small (one ext-state chart read per song, all
        // in-process), so fully build EVERY project up front instead — this is
        // what lights up the navigator + chart pane for songs beyond the
        // focused one. Native is byte-for-byte unchanged (`full_build` reduces
        // to `load.index == focused_index`).
        let (setlist, focused_song_count) = self
            .perform_phase1_build(
                &project_loads,
                &existing_songs_by_guid,
                &cached_songs,
                focused_index,
            )
            .await;

        // Compute the actual song index of the first focused song
        let focused_song_start = project_loads
            .iter()
            .take_while(|l| l.index != focused_index)
            .count(); // one placeholder per project before focused

        if let Some(active_song) = setlist.songs.get(focused_song_start) {
            *self.active_song_id.write().await = Some(active_song.id.to_string());
            debug!(
                "Set initial active song to: {} ({})",
                active_song.name, active_song.id
            );
        }

        debug!(
            "Phase 1 complete: focused project ready ({} songs), {} total songs loaded",
            focused_song_count,
            setlist.songs.len()
        );
        *self.setlist.write().await = Some(setlist);
        self.notify_setlist_changed();
        if let Some(load) = project_loads.get(focused_index) {
            self.emit_cached_chart_payload_for_song(focused_song_start, &load.guid)
                .await;
        }

        // Phase 2: hydrate remaining projects in background with bounded concurrency.
        // Each project may produce multiple songs, so we splice by project_guid.
        // Native-only: the background pass uses `tokio::spawn` + `tokio::task::
        // JoinSet` (a tokio runtime + `Send` futures), which don't build for the
        // browser's single-threaded runtime. On wasm the initial (Phase 1)
        // synchronous build above already populated the setlist; lazy hydration
        // for the browser engine is a Phase-2 runtime path.
        #[cfg(target_arch = "wasm32")]
        let _ = build_generation;
        #[cfg(not(target_arch = "wasm32"))]
        self.spawn_phase2_hydration(
            project_loads,
            existing_songs_by_guid,
            build_generation,
            focused_index,
        );
        Ok(())
    }

    pub(crate) async fn refresh_impl(&self) -> Result<(), SessionServiceError> {
        info!("Refreshing setlist...");
        self.build_from_open_projects_impl().await?;
        Ok(())
    }
}
