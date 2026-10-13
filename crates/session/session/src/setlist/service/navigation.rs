//! Navigation methods: `go_to_song`, next/previous song/section, seeking

use super::SetlistServiceImpl;
use daw::service::transport::service::Transport;
use daw::service::{AudioEngine, ProjectContext, Projects, TempoMap};
use session_proto::{QueuedTarget, SessionServiceError, Song};
use std::time::Duration;
use tracing::{debug, info, warn};

fn project_ctx(project_guid: &str) -> ProjectContext {
    ProjectContext::Project(project_guid.to_string())
}

impl<D> SetlistServiceImpl<D>
where
    D: AudioEngine + Projects + TempoMap + Transport + Clone + architect::MaybeSendSync + 'static,
{
    // All `self.daw.*` calls below hit REAPER main-thread-only FFI on the
    // `daw_reaper::Reaper` backend. Each helper bounces through
    // `daw_proto::main_thread::query` so async RPC handlers running on
    // tokio workers don't spin on REAPER's internal lock. Batched where
    // multiple calls need to happen together.
    async fn current_project_guid(&self) -> Option<String> {
        let daw = self.daw.clone();
        daw_proto::main_thread::query(move || daw.current().map(|p| p.guid))
            .await
            .flatten()
    }

    /// Recompute the active song/section from the current playhead and push
    /// them to subscribers.
    ///
    /// Seeks (`seek_to_song`, `seek_to_section`, `seek_to_musical_position`,
    /// `goto_measure`) move the DAW position but emit no active-index event
    /// of their own — the transport pump only republishes `ActiveIndices`
    /// when playback state changes. So a *stopped* seek would leave the UI's
    /// active song stale (the sidebar never expands the seeked-to song, its
    /// sections never show). Calling this at the end of each seek keeps the
    /// active indices in lockstep with the playhead whether or not the
    /// transport is rolling. Idempotent: publishing the current indices when
    /// nothing moved is harmless.
    pub(crate) async fn refresh_active_indices(&self) {
        let indices = self.calculate_active_indices().await;
        self.set_cached_indices(indices.clone()).await;
        // Cursor changes ride the dedicated active-indices stream
        // (`#[subscribe] fn active_indices`), not the setlist-events stream.
        self.indices_hub.publish(indices);
    }

    async fn select_project(&self, project_guid: &str) -> bool {
        let daw = self.daw.clone();
        let project_guid = project_guid.to_string();
        daw_proto::main_thread::query(move || daw.select(&project_guid))
            .await
            .unwrap_or(false)
    }

    /// Stop the previously active project if it's not playing.
    /// Called before switching to a new project so paused projects don't linger.
    pub(crate) async fn stop_previous_project_if_idle(&self, new_project_guid: &str) {
        let prev_song = self.get_cached_active_song().await;
        if let Some(prev) = prev_song
            && prev.project_guid != new_project_guid
        {
            let daw = self.daw.clone();
            let prev_guid = prev.project_guid.clone();
            daw_proto::main_thread::query(move || {
                let ctx = project_ctx(&prev_guid);
                let state = daw.get_play_state(ctx.clone());
                if state != daw::service::PlayState::Playing {
                    debug!(
                        "Stopping idle project {} (state={:?}) before switching",
                        prev_guid, state
                    );
                    let _ = daw.stop(ctx);
                }
            })
            .await;
        }
    }

    /// Compute the seek position for a song: count-in position if available,
    /// otherwise song start (SONGSTART marker).
    ///
    /// `Song.start_seconds` already includes the count-in offset (it's the count-in
    /// start, not SONGSTART). So the SONGSTART position is:
    ///   `start_seconds` + `count_in_seconds`
    ///
    /// We want to seek to `start_seconds` (the count-in position), but if that's
    /// negative (count-in marker is before timeline origin), fall back to SONGSTART.
    pub(crate) fn song_seek_position(song: &Song) -> f64 {
        let count_in_pos = song.start_seconds();
        if count_in_pos >= 0.0 {
            count_in_pos
        } else {
            // Count-in is before timeline origin — fall back to SONGSTART
            let songstart = song.start_seconds() + song.count_in_seconds.unwrap_or(0.0);
            debug!(
                "song_seek_position: count-in pos {:.2} is negative, falling back to SONGSTART {:.2}",
                count_in_pos, songstart
            );
            songstart.max(0.0)
        }
    }

    /// Navigate to a song by index without requiring an RPC Context.
    /// Used by the transport loop for auto-advance.
    pub(crate) async fn seek_to_song_internal(&self, song_index: usize) {
        info!("seek_to_song_internal: song_index={}", song_index);
        let Some(skeleton) = self.get_song_internal(song_index).await else {
            warn!("seek_to_song_internal: song {} not found", song_index);
            return;
        };

        // Check if we're already on the correct project (skip tab switch for same-project songs)
        let already_on_project = self
            .current_project_guid()
            .await
            .is_some_and(|guid| guid == skeleton.project_guid);

        if !already_on_project {
            self.stop_previous_project_if_idle(&skeleton.project_guid)
                .await;
        }
        self.set_active_song_id(skeleton.id.as_str()).await;

        if !already_on_project && !self.select_project(&skeleton.project_guid).await {
            warn!(
                "seek_to_song_internal: failed to switch to project {}",
                skeleton.project_guid
            );
            return;
        }

        let Ok(Some(song)) = Box::pin(architect::platform::timeout(
            Duration::from_secs(5),
            self.ensure_song_hydrated(song_index),
        ))
        .await
        else {
            warn!(
                "seek_to_song_internal: hydration failed for song {} ({})",
                song_index, skeleton.name
            );
            return;
        };

        let seek_pos = Self::song_seek_position(&song);
        let daw = self.daw.clone();
        let project_guid = skeleton.project_guid.clone();
        daw_proto::main_thread::query(move || {
            let ctx = project_ctx(&project_guid);
            if let Err(e) = daw.set_position(ctx.clone(), seek_pos) {
                warn!("seek_to_song_internal: failed to seek: {}", e);
            }
            if let Err(e) = daw.play(ctx) {
                warn!("seek_to_song_internal: failed to start playback: {}", e);
            }
        })
        .await;
    }

    // =========================================================================
    // SetlistService trait: Navigation Commands
    // =========================================================================

    pub(crate) async fn go_to_song_impl(&self, index: usize) -> Result<(), SessionServiceError> {
        debug!("go_to_song: {}", index);
        // Get the skeleton song first (no RPC, just reads from the setlist).
        // This gives us the project GUID so we can switch tabs even if hydration fails.
        let Some(skeleton) = self.get_song_internal(index).await else {
            warn!("go_to_song: song {} not found in setlist", index);
            return Ok(());
        };

        // Stop previous project if it's not actively playing
        self.stop_previous_project_if_idle(&skeleton.project_guid)
            .await;

        // Update the cached active song ID for fast playback commands
        self.set_active_song_id(skeleton.id.as_str()).await;

        // Switch to the correct project tab first — this must always happen
        if !self.select_project(&skeleton.project_guid).await {
            warn!(
                "Failed to switch to project {} for song {}",
                skeleton.project_guid, index
            );
            return Ok(());
        }

        // Now try to hydrate (best-effort, with timeout to prevent freezes)
        let song = match Box::pin(architect::platform::timeout(
            Duration::from_secs(5),
            self.ensure_song_hydrated(index),
        ))
        .await
        {
            Ok(Some(song)) => song,
            Ok(None) => {
                warn!(
                    "go_to_song: hydration returned None for song {} ({}), staying on project tab",
                    index, skeleton.name
                );
                return Ok(());
            }
            Err(_) => {
                warn!(
                    "go_to_song: hydration timed out for song {} ({}), staying on project tab",
                    index, skeleton.name
                );
                return Ok(());
            }
        };

        // Only seek if the project is NOT already playing — preserves playback
        // position when switching between songs. Batch is_playing/set_position/
        // get_position into one main-thread bounce.
        let seek_pos = Self::song_seek_position(&song);
        let daw = self.daw.clone();
        let project_guid = skeleton.project_guid.clone();
        let (is_playing, actual_pos) = daw_proto::main_thread::query(move || {
            let ctx = project_ctx(&project_guid);
            let is_playing = daw.is_playing(ctx.clone());
            if !is_playing && let Err(e) = daw.set_position(ctx.clone(), seek_pos) {
                warn!("Failed to set position for song {}: {}", index, e);
            }
            let actual_pos = daw.get_position(ctx);
            (is_playing, actual_pos)
        })
        .await
        .unwrap_or((false, 0.0));

        info!(
            "go_to_song: song {} ({}) — seek_target={:.2}s, actual_pos={:.2}s, is_playing={}",
            index, song.name, seek_pos, actual_pos, is_playing
        );
        Ok(())
    }

    pub(crate) async fn next_song_impl(&self) -> Result<(), SessionServiceError> {
        let active = self.get_cached_indices().await;
        info!("next_song: cached song_index={:?}", active.song_index);
        if let Some(current_idx) = active.song_index {
            let next_idx = current_idx.saturating_add(1);
            self.go_to_song_impl(next_idx).await?;
        } else {
            warn!("next_song: no active song index, cannot navigate");
        }
        Ok(())
    }

    pub(crate) async fn previous_song_impl(&self) -> Result<(), SessionServiceError> {
        let active = self.get_cached_indices().await;
        info!("previous_song: cached song_index={:?}", active.song_index);
        if let Some(current_idx) = active.song_index {
            if current_idx > 0 {
                let prev_idx = current_idx.saturating_sub(1);
                self.go_to_song_impl(prev_idx).await?;
            } else {
                info!("previous_song: already at first song (index 0)");
            }
        } else {
            warn!("previous_song: no active song index, cannot navigate");
        }
        Ok(())
    }

    pub(crate) async fn go_to_section_impl(&self, index: usize) -> Result<(), SessionServiceError> {
        debug!("go_to_section: {}", index);
        // Use cached indices for instant response (updated at 60Hz by polling loop)
        let active = self.get_cached_indices().await;

        if let Some(song_idx) = active.song_index
            && let Some(song) = self.ensure_song_hydrated(song_idx).await
        {
            // Queue the target immediately for visual feedback
            self.queue_target(QueuedTarget::Section {
                song_id: song.id.clone(),
                song_index: song_idx,
                section_index: index,
            })
            .await;

            if let Some(section) = song.sections.get(index) {
                let project_guid = song.project_guid.clone();
                let section_start = section.start_seconds;
                let daw = self.daw.clone();
                let outcome = daw_proto::main_thread::query(move || {
                    if !daw.select(&project_guid) {
                        return Err("select_failed");
                    }
                    daw.set_position(project_ctx(&project_guid), section_start)
                        .map_err(|_| "set_position_failed")
                })
                .await
                .unwrap_or(Err("main_thread_unavailable"));

                match outcome {
                    Err("select_failed") => warn!(
                        "Failed to switch to project {} for section navigation",
                        song.project_guid
                    ),
                    Err(reason) => {
                        warn!(
                            "Failed to navigate to section {} ({}): {}",
                            index, song.project_guid, reason
                        );
                        self.clear_queued_target().await;
                    }
                    Ok(()) => info!(
                        "Navigated to section {} ({}) in song {} (project {})",
                        index, section.name, song.name, song.project_guid
                    ),
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn next_section_impl(&self) -> Result<(), SessionServiceError> {
        debug!("next_section");

        // Use cached indices for instant response (updated at 60Hz by polling loop)
        let active = self.get_cached_indices().await;
        if let Some(section_idx) = active.section_index {
            let next_idx = section_idx.saturating_add(1);
            self.go_to_section_impl(next_idx).await?;
        }
        Ok(())
    }

    pub(crate) async fn previous_section_impl(&self) -> Result<(), SessionServiceError> {
        debug!("previous_section");

        // Use cached indices for instant response (updated at 60Hz by polling loop)
        let active = self.get_cached_indices().await;
        if let Some(section_idx) = active.section_index {
            // Smart previous: if we're past the beginning of the section (>5% progress),
            // go to the start of the current section. Only go to previous section
            // if we're already at/near the beginning.
            let at_section_start = active.section_progress.is_none_or(|p| p < 0.05);

            if at_section_start && section_idx > 0 {
                // Already at the start, go to previous section
                let prev_idx = section_idx.saturating_sub(1);
                self.go_to_section_impl(prev_idx).await?;
            } else {
                // Not at start, go to beginning of current section
                self.go_to_section_impl(section_idx).await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn seek_to_impl(&self, seconds: f64) -> Result<(), SessionServiceError> {
        debug!("seek_to: {}", seconds);
        // Use cached indices for instant response (updated at 60Hz by polling loop)
        let active = self.get_cached_indices().await;

        if let Some(song_idx) = active.song_index
            && let Some(song) = self.ensure_song_hydrated(song_idx).await
        {
            // HARD INVARIANT: a `seek_to` is song-relative and may never
            // leave the current song — clamp instead of letting an
            // out-of-range target run the playhead past the song end (where
            // the advance logic would flip the whole set to another song).
            // The 10 ms end margin keeps the playhead strictly INSIDE the
            // song: on a shared timeline song N's end == song N+1's start,
            // and landing exactly on the boundary re-indexes to the next song.
            let seconds = seconds.clamp(0.0, (song.duration() - 0.01).max(0.0));
            // Mirror the (working) seek_to_section path exactly: select the
            // project first, seek, then republish. Without the republish a
            // PAUSED seek moves the cursor with no playhead tick, so
            // ACTIVE_INDICES never follows and Play "resumes" at the stale
            // position the UI snapped back to.
            let absolute_pos = song.start_seconds() + seconds;
            let daw = self.daw.clone();
            let project_guid = song.project_guid.clone();
            let outcome = daw_proto::main_thread::query(move || {
                if !daw.select(&project_guid) {
                    return Err("select_failed");
                }
                daw.set_position(project_ctx(&project_guid), absolute_pos)
                    .map_err(|_| "set_position_failed")
            })
            .await
            .unwrap_or(Err("main_thread_unavailable"));

            match outcome {
                Ok(()) => info!(
                    "Seeked to {:.2}s (abs {:.2}s) in song {} ({})",
                    seconds, absolute_pos, song_idx, song.name
                ),
                Err(reason) => warn!(
                    "Failed to seek to {:.2}s in song {}: {}",
                    seconds, song_idx, reason
                ),
            }
            self.refresh_active_indices().await;
            self.publish_transport_snapshot().await;
        } else {
            warn!("seek_to: no active song (indices not cached yet)");
        }
        Ok(())
    }

    pub(crate) async fn seek_to_time_impl(
        &self,
        song_index: usize,
        seconds: f64,
    ) -> Result<(), SessionServiceError> {
        info!(
            "seek_to_time: song_index={}, seconds={}",
            song_index, seconds
        );
        if let Some(song) = self.ensure_song_hydrated(song_index).await {
            // Queue the target immediately for visual feedback (comment marker)
            self.queue_target(QueuedTarget::Comment {
                song_id: song.id.clone(),
                song_index,
                position_seconds: seconds,
            })
            .await;
            let daw = self.daw.clone();
            let project_guid = song.project_guid.clone();
            let outcome = daw_proto::main_thread::query(move || {
                if !daw.select(&project_guid) {
                    return Err("select_failed");
                }
                daw.set_position(project_ctx(&project_guid), seconds)
                    .map_err(|_| "set_position_failed")
            })
            .await
            .unwrap_or(Err("main_thread_unavailable"));

            match outcome {
                Err("select_failed") => warn!(
                    "Failed to switch to project {} for song {}",
                    song.project_guid, song_index
                ),
                Err(reason) => {
                    warn!(
                        "Failed to seek to {} seconds in song {}: {}",
                        seconds, song_index, reason
                    );
                    self.clear_queued_target().await;
                }
                Ok(()) => info!(
                    "Seeked to {} seconds in song {} ({})",
                    seconds, song_index, song.name
                ),
            }
            self.publish_transport_snapshot().await;
        } else {
            warn!("Song {} not found", song_index);
        }
        Ok(())
    }

    pub(crate) async fn seek_to_song_impl(
        &self,
        song_index: usize,
    ) -> Result<(), SessionServiceError> {
        info!("seek_to_song called: song_index={}", song_index);
        // Get the skeleton song first so we can switch tabs even if hydration fails.
        let Some(skeleton) = self.get_song_internal(song_index).await else {
            warn!("seek_to_song: song {} not found in setlist", song_index);
            return Ok(());
        };

        // Check if we're already on the correct project (skip tab switch for same-project songs)
        let already_on_project = self
            .current_project_guid()
            .await
            .is_some_and(|guid| guid == skeleton.project_guid);

        // Only stop previous project if we're actually changing projects
        if !already_on_project {
            self.stop_previous_project_if_idle(&skeleton.project_guid)
                .await;
        }

        // Update the cached active song ID for fast playback commands
        self.set_active_song_id(skeleton.id.as_str()).await;

        // Switch to the correct project tab (or reuse if already there)
        if !already_on_project && !self.select_project(&skeleton.project_guid).await {
            warn!(
                "Failed to switch to project {} for song {}",
                skeleton.project_guid, song_index
            );
            return Ok(());
        }

        // Now try to hydrate (best-effort, with timeout to prevent freezes)
        let song = match Box::pin(architect::platform::timeout(
            Duration::from_secs(5),
            self.ensure_song_hydrated(song_index),
        ))
        .await
        {
            Ok(Some(song)) => song,
            Ok(None) => {
                warn!(
                    "seek_to_song: hydration returned None for song {} ({}), staying on project tab",
                    song_index, skeleton.name
                );
                return Ok(());
            }
            Err(_) => {
                warn!(
                    "seek_to_song: hydration timed out for song {} ({}), staying on project tab",
                    song_index, skeleton.name
                );
                return Ok(());
            }
        };

        // Only seek if the project is NOT already playing. Batch
        // is_playing/set_position/get_position in one main-thread bounce.
        let seek_pos = Self::song_seek_position(&song);
        let daw = self.daw.clone();
        let project_guid = skeleton.project_guid.clone();
        let (is_playing, actual_pos) = daw_proto::main_thread::query(move || {
            let ctx = project_ctx(&project_guid);
            let is_playing = daw.is_playing(ctx.clone());
            if !is_playing && let Err(e) = daw.set_position(ctx.clone(), seek_pos) {
                warn!("Failed to seek to song {}: {}", song_index, e);
            }
            let actual_pos = daw.get_position(ctx);
            (is_playing, actual_pos)
        })
        .await
        .unwrap_or((false, 0.0));

        info!(
            "seek_to_song: song {} ({}) — seek_target={:.2}s, actual_pos={:.2}s, is_playing={}",
            song_index, song.name, seek_pos, actual_pos, is_playing
        );
        self.refresh_active_indices().await;
        // A stopped seek moves the edit cursor without a playhead tick, so the
        // reactive loop won't emit transport. Publish a fresh snapshot so the
        // playhead/BPM/musical-position badges update to the new song.
        self.publish_transport_snapshot().await;
        Ok(())
    }

    pub(crate) async fn seek_to_section_impl(
        &self,
        song_index: usize,
        section_index: usize,
    ) -> Result<(), SessionServiceError> {
        debug!(
            "seek_to_section: song={}, section={}",
            song_index, section_index
        );
        if let Some(song) = self.ensure_song_hydrated(song_index).await {
            if let Some(section) = song.sections.get(section_index) {
                let daw = self.daw.clone();
                let project_guid = song.project_guid.clone();
                let section_start = section.start_seconds;
                let section_name = section.name.clone();
                let song_name = song.name.clone();
                let song_project_guid = song.project_guid.clone();
                let outcome = daw_proto::main_thread::query(move || {
                    if !daw.select(&project_guid) {
                        return Err("select_failed");
                    }
                    daw.set_position(project_ctx(&project_guid), section_start)
                        .map_err(|_| "set_position_failed")
                })
                .await
                .unwrap_or(Err("main_thread_unavailable"));

                match outcome {
                    Err("select_failed") => warn!(
                        "Failed to switch to project {} for song {}",
                        song_project_guid, song_index
                    ),
                    Err(reason) => warn!(
                        "Failed to seek to section {} in song {}: {}",
                        section_index, song_index, reason
                    ),
                    Ok(()) => info!(
                        "Seeked to section {} ({}) in song {} (project {})",
                        section_index, section_name, song_name, song_project_guid
                    ),
                }
            } else {
                warn!("Section {} not found in song {}", section_index, song_index);
            }
        } else {
            warn!("Song {} not found", song_index);
        }
        self.refresh_active_indices().await;
        self.publish_transport_snapshot().await;
        Ok(())
    }

    pub(crate) async fn seek_to_musical_position_impl(
        &self,
        song_index: usize,
        position: daw::service::MusicalPosition,
    ) -> Result<(), SessionServiceError> {
        debug!(
            "seek_to_musical_position: song={}, position={}.{}.{}",
            song_index, position.measure, position.beat, position.subdivision
        );
        if let Some(song) = self.ensure_song_hydrated(song_index).await {
            let daw = self.daw.clone();
            let project_guid = song.project_guid.clone();
            let song_start = song.start_seconds();
            let song_name = song.name.clone();
            let song_project_guid = song.project_guid.clone();
            let measure = position.measure;
            let beat = position.beat;
            let subdivision = position.subdivision;
            let outcome = daw_proto::main_thread::query(move || {
                if !daw.select(&project_guid) {
                    return Err("select_failed");
                }
                let fraction = f64::from(subdivision) / 1000.0;
                let relative_seconds =
                    daw.musical_to_time(project_ctx(&project_guid), measure, beat, fraction);
                let absolute_pos = song_start + relative_seconds;
                daw.set_position(project_ctx(&project_guid), absolute_pos)
                    .map_err(|_| "set_position_failed")
            })
            .await
            .unwrap_or(Err("main_thread_unavailable"));

            match outcome {
                Err("select_failed") => warn!(
                    "Failed to switch to project {} for song {}",
                    song_project_guid, song_index
                ),
                Err(reason) => warn!(
                    "Failed to seek to musical position in song {}: {}",
                    song_index, reason
                ),
                Ok(()) => info!(
                    "Seeked to {}.{}.{} in song {} (project {})",
                    measure, beat, subdivision, song_name, song_project_guid
                ),
            }
        } else {
            warn!("Song {} not found", song_index);
        }
        self.refresh_active_indices().await;
        Ok(())
    }

    pub(crate) async fn goto_measure_impl(
        &self,
        song_index: usize,
        measure: i32,
    ) -> Result<(), SessionServiceError> {
        info!(
            "goto_measure: song_index={}, measure={}",
            song_index, measure
        );
        if let Some(song) = self.ensure_song_hydrated(song_index).await {
            let daw = self.daw.clone();
            let project_guid = song.project_guid.clone();
            let song_name = song.name.clone();
            let song_project_guid = song.project_guid.clone();
            let outcome = daw_proto::main_thread::query(move || {
                if !daw.select(&project_guid) {
                    return Err("select_failed");
                }
                let ctx = project_ctx(&project_guid);
                let seconds = daw.musical_to_time(ctx.clone(), measure, 1, 0.0);
                daw.set_position(ctx, seconds)
                    .map_err(|_| "set_position_failed")
            })
            .await
            .unwrap_or(Err("main_thread_unavailable"));

            match outcome {
                Err("select_failed") => warn!(
                    "Failed to switch to project {} for song {}",
                    song_project_guid, song_index
                ),
                Err(reason) => warn!(
                    "Failed to goto measure {} in song {}: {}",
                    measure, song_index, reason
                ),
                Ok(()) => info!(
                    "Went to measure {} in song {} ({})",
                    measure, song_name, song_project_guid
                ),
            }
        } else {
            warn!("Song {} not found", song_index);
        }
        self.refresh_active_indices().await;
        self.publish_transport_snapshot().await;
        Ok(())
    }
}
