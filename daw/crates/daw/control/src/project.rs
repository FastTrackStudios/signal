//! Project handle

use std::sync::Arc;

use crate::{
    DawClients, Error, ExtState, FxChain, Markers, ProjectItems, Regions, TempoMap, TrackHandle,
    Tracks, Transport,
};
use daw_proto::FxChainContext;

/// Project handle - lightweight wrapper around project GUID
///
/// This handle represents a specific DAW project. It stores only the project GUID
/// and provides methods to access project subsystems (transport, tracks, etc.).
///
/// Like reaper-rs, this handle is lightweight and cheap to clone.
///
/// # Example
///
/// ```no_run
/// use daw_control::Daw;
///
/// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
/// let daw = Daw::new(handle);
/// let project = daw.current_project().await?;
/// println!("Project GUID: {}", project.guid());
///
/// // Access transport
/// project.transport().play().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Project {
    guid: String,
    clients: Arc<DawClients>,
}

impl Project {
    /// Create a new project handle
    pub(crate) fn new(guid: String, clients: Arc<DawClients>) -> Self {
        Self { guid, clients }
    }

    /// Get the project GUID
    pub fn guid(&self) -> &str {
        &self.guid
    }

    /// Get the project info (name, path, etc.)
    ///
    /// Fetches the full project information from the DAW.
    pub async fn info(&self) -> crate::Result<daw_proto::ProjectInfo> {
        self.clients
            .project
            .get(self.guid.clone())
            .await?
            .ok_or_else(|| Error::Other(format!("Project not found: {}", self.guid)))
    }

    /// Get transport accessor for this project
    ///
    /// Returns a handle to control and monitor the transport (playback, recording, etc.)
    /// for this specific project.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// let project = daw.current_project().await?;
    ///
    /// // Control transport
    /// project.transport().play().await?;
    /// project.transport().stop().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn transport(&self) -> Transport {
        Transport::new(self.guid.clone(), self.clients.clone())
    }

    /// This project's song folder, as a peer streaming the song in reads it.
    pub fn song_files(&self) -> crate::SongFiles {
        crate::SongFiles::new(self.guid.clone(), self.clients.clone())
    }

    /// Transport sync for this project: the server's sync clock, its
    /// stamped positions, and a [`TransportLeader`](crate::TransportLeader)
    /// to keep a local backend in step with it.
    pub fn transport_sync(&self) -> crate::ProjectTransportSync {
        crate::ProjectTransportSync::new(self.guid.clone(), self.clients.clone())
    }

    /// Get markers accessor for this project
    ///
    /// Returns a handle to query and manipulate markers in this project.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// let project = daw.current_project().await?;
    ///
    /// // Query and manipulate markers
    /// let markers = project.markers().all().await?;
    /// project.markers().add(10.0, "Verse 1").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn markers(&self) -> Markers {
        Markers::new(self.guid.clone(), self.clients.clone())
    }

    /// Get regions accessor for this project
    ///
    /// Returns a handle to query and manipulate regions in this project.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// let project = daw.current_project().await?;
    ///
    /// // Query and manipulate regions
    /// let regions = project.regions().all().await?;
    /// project.regions().add(0.0, 30.0, "Intro").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn regions(&self) -> Regions {
        Regions::new(self.guid.clone(), self.clients.clone())
    }

    /// Get tempo map accessor for this project
    ///
    /// Returns a handle to query and manipulate tempo/time signature changes
    /// in this project.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// let project = daw.current_project().await?;
    ///
    /// // Query tempo
    /// let bpm = project.tempo_map().tempo_at(10.0).await?;
    /// let (measure, beat, frac) = project.tempo_map().time_to_musical(30.0).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn tempo_map(&self) -> TempoMap {
        TempoMap::new(self.guid.clone(), self.clients.clone())
    }

    /// Get tracks accessor for this project
    ///
    /// Returns a handle to enumerate tracks and perform batch track operations.
    /// Individual track operations are performed through [`TrackHandle`](crate::TrackHandle).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// let project = daw.current_project().await?;
    ///
    /// // Enumerate tracks
    /// for track in project.tracks().all().await? {
    ///     println!("Track: {}", track.name);
    /// }
    ///
    /// // Get specific track and control it
    /// if let Some(vocals) = project.tracks().by_name("Vocals").await? {
    ///     vocals.solo_exclusive().await?;
    ///     vocals.fx_chain().add("ReaComp").await?;
    /// }
    ///
    /// // Batch operations
    /// project.tracks().clear_solo().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn tracks(&self) -> Tracks {
        Tracks::new(self.guid.clone(), self.clients.clone())
    }

    // =========================================================================
    // rea-rs-style Track Convenience Methods
    //
    // These flatten the project.tracks().method() indirection into direct
    // project.method() calls, matching rea-rs's ergonomic patterns:
    //   project.n_tracks()      → track count
    //   project.get_track(i)    → track by index
    //   project.iter_tracks()   → all tracks as TrackHandles
    //   project.master_track()  → master track handle
    // =========================================================================

    /// Get the number of tracks in this project.
    ///
    /// Equivalent to `project.tracks().count()`.
    pub async fn n_tracks(&self) -> crate::Result<u32> {
        self.tracks().count().await
    }

    /// Live per-track meter stream for this project (architect
    /// `#[subscribe]`): one [`MeterFrame`](daw_proto::MeterFrame) at
    /// ~30 Hz while an audio engine is attached, carrying linear
    /// `0..1` peak + peak-hold levels for EVERY track (`tracks[i]` =
    /// project track index `i`). The server streams every project's
    /// frames; filtering to this project happens client-side. One
    /// subscription drives a whole mixer — far cheaper than polling
    /// `track_peak` per track. Drop the stream to unsubscribe.
    pub fn meter_events(&self) -> crate::EventStream<daw_proto::MeterFrame> {
        let (raw_tx, raw_rx) = vox::channel();
        let stream = self.clients.peaks_stream.clone();
        let want = self.guid.clone();
        crate::EventStream::spawn(
            async move {
                let _ = stream.meters(raw_tx).await;
            },
            raw_rx,
            Box::new(move |ev| ev.project_guid == want),
        )
    }

    /// Live post-fader peak level for one track channel (0 = left,
    /// 1 = right). dB values, −150 = silence. Drives hardware meter
    /// bridges and UI meters.
    pub async fn track_peak(
        &self,
        track: daw_proto::TrackRef,
        channel: u32,
    ) -> crate::Result<daw_proto::TrackPeak> {
        Ok(self
            .clients
            .peaks
            .track_peak(
                daw_proto::ProjectContext::project(&self.guid),
                track,
                channel,
            )
            .await?)
    }

    /// Touch / release / write a parameter through the automation
    /// engine (surfaces: fader-touch sensors + automated writes that
    /// record envelope points in touch/latch/write modes).
    pub async fn automation_touch(
        &self,
        location: daw_proto::automation::EnvelopeLocation,
    ) -> crate::Result<()> {
        self.clients
            .automation
            .touch_param(daw_proto::ProjectContext::project(&self.guid), location)
            .await??;
        Ok(())
    }

    pub async fn automation_release(
        &self,
        location: daw_proto::automation::EnvelopeLocation,
    ) -> crate::Result<()> {
        self.clients
            .automation
            .release_param(daw_proto::ProjectContext::project(&self.guid), location)
            .await??;
        Ok(())
    }

    pub async fn automation_write(
        &self,
        location: daw_proto::automation::EnvelopeLocation,
        value: f64,
    ) -> crate::Result<()> {
        self.clients
            .automation
            .write_param(
                daw_proto::ProjectContext::project(&self.guid),
                location,
                value,
            )
            .await??;
        Ok(())
    }

    /// Get a track by index.
    ///
    /// Equivalent to `project.tracks().by_index(index)`.
    pub async fn get_track(&self, index: u32) -> crate::Result<Option<TrackHandle>> {
        self.tracks().by_index(index).await
    }

    /// Get a track by GUID.
    ///
    /// Equivalent to `project.tracks().by_guid(guid)`.
    pub async fn get_track_by_guid(&self, guid: &str) -> crate::Result<Option<TrackHandle>> {
        self.tracks().by_guid(guid).await
    }

    /// Get a track by name.
    ///
    /// Equivalent to `project.tracks().by_name(name)`.
    pub async fn get_track_by_name(&self, name: &str) -> crate::Result<Option<TrackHandle>> {
        self.tracks().by_name(name).await
    }

    /// Get all tracks as TrackHandles.
    ///
    /// Returns a Vec of TrackHandles that can be iterated over. Each handle
    /// is a lightweight reference that can query/mutate the track via RPC.
    ///
    /// Equivalent to rea-rs's `project.iter_tracks()`, but returns a Vec
    /// since our API is async (can't return a lazy iterator over RPC).
    ///
    /// # Example
    ///
    /// ```no_run
    /// # async fn example(project: daw_control::Project) -> daw_control::Result<()> {
    /// for track in project.iter_tracks().await? {
    ///     let info = track.info().await?;
    ///     println!("{}: {}", info.index, info.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn iter_tracks(&self) -> crate::Result<Vec<TrackHandle>> {
        let tracks = self.tracks().all().await?;
        Ok(tracks
            .into_iter()
            .map(|t| TrackHandle::new(t.guid, self.guid.clone(), self.clients.clone()))
            .collect())
    }

    /// Get the master track handle.
    ///
    /// Equivalent to `project.tracks().master()`.
    pub async fn master_track(&self) -> crate::Result<TrackHandle> {
        self.tracks().master().await
    }

    /// Get all currently selected tracks.
    ///
    /// Equivalent to `project.tracks().selected()`.
    pub async fn selected_tracks(&self) -> crate::Result<Vec<TrackHandle>> {
        self.tracks().selected().await
    }

    /// Add a new track to this project.
    ///
    /// Equivalent to `project.tracks().add(name, at_index)`.
    pub async fn add_track(&self, name: &str, at_index: Option<u32>) -> crate::Result<TrackHandle> {
        self.tracks().add(name, at_index).await
    }

    // =========================================================================
    // Undo
    // =========================================================================

    /// Begin an undo block.
    ///
    /// All state changes until `end_undo_block` are grouped into a single
    /// entry in the DAW's undo history.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # async fn example(project: daw_control::Project) -> daw_control::Result<()> {
    /// project.begin_undo_block("Batch rename tracks").await?;
    /// for track in project.iter_tracks().await? {
    ///     track.rename("New Name").await?;
    /// }
    /// project.end_undo_block("Batch rename tracks").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn begin_undo_block(&self, label: &str) -> crate::Result<()> {
        self.clients
            .project
            .begin_undo_block(
                daw_proto::ProjectContext::project(&self.guid),
                label.to_string(),
            )
            .await?;
        Ok(())
    }

    /// End the current undo block.
    pub async fn end_undo_block(&self, label: &str) -> crate::Result<()> {
        self.clients
            .project
            .end_undo_block(
                daw_proto::ProjectContext::project(&self.guid),
                label.to_string(),
                None, // Use default UndoScope::All
            )
            .await?;
        Ok(())
    }

    /// Trigger undo. Returns true if an action was undone.
    pub async fn undo(&self) -> crate::Result<bool> {
        Ok(self
            .clients
            .project
            .undo(daw_proto::ProjectContext::project(&self.guid))
            .await?)
    }

    /// Trigger redo. Returns true if an action was redone.
    pub async fn redo(&self) -> crate::Result<bool> {
        Ok(self
            .clients
            .project
            .redo(daw_proto::ProjectContext::project(&self.guid))
            .await?)
    }

    /// Get the label of the last undoable action.
    pub async fn last_undo_label(&self) -> crate::Result<Option<String>> {
        Ok(self
            .clients
            .project
            .last_undo_label(daw_proto::ProjectContext::project(&self.guid))
            .await?)
    }

    /// Get the label of the last redoable action.
    pub async fn last_redo_label(&self) -> crate::Result<Option<String>> {
        Ok(self
            .clients
            .project
            .last_redo_label(daw_proto::ProjectContext::project(&self.guid))
            .await?)
    }

    /// Save this project.
    ///
    /// If the project hasn't been saved before (no path), this may prompt
    /// the user with a "Save As" dialog.
    pub async fn save(&self) -> crate::Result<()> {
        self.clients
            .project
            .save(daw_proto::ProjectContext::project(&self.guid))
            .await?;
        Ok(())
    }

    /// Run a REAPER action/command by its string identifier.
    ///
    /// Accepts either a numeric command ID (e.g. `"40001"`) or a named
    /// command identifier (e.g. `"_S&M_WNTSHW1"` for SWS actions).
    ///
    /// Returns `true` if the command was found and executed.
    pub async fn run_command(&self, command: &str) -> crate::Result<bool> {
        Ok(self
            .clients
            .project
            .run_command(
                daw_proto::ProjectContext::project(&self.guid),
                command.to_string(),
            )
            .await?)
    }

    // =========================================================================
    // Ruler Lane Management (v7.62+)
    // =========================================================================

    /// Set the name of a ruler lane (1-based index). Creates the lane if needed.
    pub async fn set_ruler_lane_name(&self, lane_index: u32, name: &str) -> crate::Result<()> {
        self.clients
            .project
            .set_ruler_lane_name(
                daw_proto::ProjectContext::project(&self.guid),
                lane_index,
                name.to_string(),
            )
            .await?;
        Ok(())
    }

    /// Get the name of a ruler lane, or empty string if it doesn't exist.
    pub async fn get_ruler_lane_name(&self, lane_index: u32) -> crate::Result<String> {
        Ok(self
            .clients
            .project
            .get_ruler_lane_name(daw_proto::ProjectContext::project(&self.guid), lane_index)
            .await?)
    }

    // =========================================================================
    // Project Info
    // =========================================================================

    /// Get a project info string value (e.g. `"RENDER_FILE"`, `"RENDER_PATTERN"`).
    pub async fn get_info_string(&self, key: &str) -> crate::Result<String> {
        Ok(self
            .clients
            .project
            .get_project_info_string(
                daw_proto::ProjectContext::project(&self.guid),
                key.to_string(),
            )
            .await?)
    }

    /// Set a project info string value (e.g. `"RENDER_FILE"`, `"RENDER_PATTERN"`).
    pub async fn set_info_string(&self, key: &str, value: &str) -> crate::Result<()> {
        self.clients
            .project
            .set_project_info_string(
                daw_proto::ProjectContext::project(&self.guid),
                key.to_string(),
                value.to_string(),
            )
            .await?;
        Ok(())
    }

    /// Get a project info numeric value (e.g. `"RENDER_SETTINGS"`, `"PROJECT_SRATE"`).
    pub async fn get_info(&self, key: &str) -> crate::Result<f64> {
        Ok(self
            .clients
            .project
            .get_project_info(
                daw_proto::ProjectContext::project(&self.guid),
                key.to_string(),
            )
            .await?)
    }

    /// Set a project info numeric value.
    pub async fn set_info(&self, key: &str, value: f64) -> crate::Result<()> {
        self.clients
            .project
            .set_project_info(
                daw_proto::ProjectContext::project(&self.guid),
                key.to_string(),
                value,
            )
            .await?;
        Ok(())
    }

    /// Get a project/global config value by REAPER-style config key.
    pub async fn get_config(&self, key: &str) -> crate::Result<Option<f64>> {
        Ok(self
            .clients
            .project
            .get_project_config(
                daw_proto::ProjectContext::project(&self.guid),
                key.to_string(),
            )
            .await?)
    }

    /// Set a project/global config value by REAPER-style config key.
    pub async fn set_config(&self, key: &str, value: f64) -> crate::Result<bool> {
        Ok(self
            .clients
            .project
            .set_project_config(
                daw_proto::ProjectContext::project(&self.guid),
                key.to_string(),
                value,
            )
            .await?)
    }

    /// Get the number of named ruler lanes.
    pub async fn ruler_lane_count(&self) -> crate::Result<u32> {
        Ok(self
            .clients
            .project
            .ruler_lane_count(daw_proto::ProjectContext::project(&self.guid))
            .await?)
    }

    /// Get a handle to the persistent key-value storage (ExtState).
    ///
    /// Same API as `Daw::ext_state()` — lightweight, cheap to clone.
    pub fn ext_state(&self) -> ExtState {
        ExtState::new(self.clients.clone())
    }

    /// Get monitoring FX chain (global, not per-track)
    ///
    /// Returns a handle to the monitoring FX chain, which is applied globally
    /// to the monitoring output.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// let project = daw.current_project().await?;
    ///
    /// // Add monitoring FX
    /// let mon_chain = project.monitoring_fx();
    /// for fx in mon_chain.all().await? {
    ///     println!("Monitoring FX: {}", fx.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn monitoring_fx(&self) -> FxChain {
        FxChain::new(
            FxChainContext::Monitoring,
            self.guid.clone(),
            self.clients.clone(),
        )
    }

    // =========================================================================
    // Position Conversions
    // =========================================================================
    //
    // Following reaper-rs design philosophy:
    // - Position conversions require project context (tempo map)
    // - Conversions return rich result types with measure/beat metadata
    // - No implicit conversions between position types

    /// Convert time position to beats
    ///
    /// Returns beat position with measure context and time signature information.
    /// Requires the project's tempo map for accurate conversion.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    /// use daw_proto::{PositionInSeconds, MeasureMode};
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// let project = daw.current_project().await?;
    ///
    /// let pos = PositionInSeconds::from_seconds(30.0);
    /// let result = project.time_to_beats(pos, MeasureMode::IgnoreMeasure).await?;
    /// println!("Beat: {}, Measure: {}", result.full_beats.as_beats(), result.measure_index);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn time_to_beats(
        &self,
        position: daw_proto::PositionInSeconds,
        measure_mode: daw_proto::MeasureMode,
    ) -> crate::Result<daw_proto::TimeToBeatsResult> {
        let result = self
            .clients
            .position_conversion
            .time_to_beats(
                daw_proto::ProjectContext::project(&self.guid),
                position,
                measure_mode,
            )
            .await?;
        Ok(result)
    }

    /// Convert beat position to time
    ///
    /// Returns time position in seconds based on the project's tempo map.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    /// use daw_proto::{PositionInBeats, MeasureMode};
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// let project = daw.current_project().await?;
    ///
    /// let pos = PositionInBeats::from_beats(16.0);
    /// let seconds = project.beats_to_time(pos, MeasureMode::IgnoreMeasure).await?;
    /// println!("Time: {}s", seconds.as_seconds());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn beats_to_time(
        &self,
        position: daw_proto::PositionInBeats,
        measure_mode: daw_proto::MeasureMode,
    ) -> crate::Result<daw_proto::PositionInSeconds> {
        let result = self
            .clients
            .position_conversion
            .beats_to_time(
                daw_proto::ProjectContext::project(&self.guid),
                position,
                measure_mode,
            )
            .await?;
        Ok(result)
    }

    /// Convert time position to quarter notes
    ///
    /// Returns quarter note position with measure context and time signature.
    /// Quarter notes are REAPER's native time mapping unit.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    /// use daw_proto::PositionInSeconds;
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// let project = daw.current_project().await?;
    ///
    /// let pos = PositionInSeconds::from_seconds(30.0);
    /// let result = project.time_to_quarter_notes(pos).await?;
    /// println!("QN: {}", result.quarter_notes.as_quarter_notes());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn time_to_quarter_notes(
        &self,
        position: daw_proto::PositionInSeconds,
    ) -> crate::Result<daw_proto::TimeToQuarterNotesResult> {
        let result = self
            .clients
            .position_conversion
            .time_to_quarter_notes(daw_proto::ProjectContext::project(&self.guid), position)
            .await?;
        Ok(result)
    }

    /// Convert quarter notes to time position
    ///
    /// Returns time position in seconds based on the project's tempo map.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    /// use daw_proto::PositionInQuarterNotes;
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// let project = daw.current_project().await?;
    ///
    /// let pos = PositionInQuarterNotes::from_quarter_notes(16.0);
    /// let seconds = project.quarter_notes_to_time(pos).await?;
    /// println!("Time: {}s", seconds.as_seconds());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn quarter_notes_to_time(
        &self,
        position: daw_proto::PositionInQuarterNotes,
    ) -> crate::Result<daw_proto::PositionInSeconds> {
        let result = self
            .clients
            .position_conversion
            .quarter_notes_to_time(daw_proto::ProjectContext::project(&self.guid), position)
            .await?;
        Ok(result)
    }

    /// Convert quarter notes to measure information
    ///
    /// Returns the measure index and start/end positions for that measure.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    /// use daw_proto::PositionInQuarterNotes;
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// let project = daw.current_project().await?;
    ///
    /// let pos = PositionInQuarterNotes::from_quarter_notes(16.0);
    /// let measure = project.quarter_notes_to_measure(pos).await?;
    /// println!("Measure {}: {} to {} QN",
    ///     measure.measure_index,
    ///     measure.start.as_quarter_notes(),
    ///     measure.end.as_quarter_notes()
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub async fn quarter_notes_to_measure(
        &self,
        position: daw_proto::PositionInQuarterNotes,
    ) -> crate::Result<daw_proto::QuarterNotesToMeasureResult> {
        let result = self
            .clients
            .position_conversion
            .quarter_notes_to_measure(daw_proto::ProjectContext::project(&self.guid), position)
            .await?;
        Ok(result)
    }

    /// Convert beats to quarter notes
    ///
    /// In most cases beats == quarter notes, but this can vary with time signature.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    /// use daw_proto::PositionInBeats;
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// let project = daw.current_project().await?;
    ///
    /// let beats = PositionInBeats::from_beats(8.0);
    /// let qn = project.beats_to_quarter_notes(beats).await?;
    /// println!("QN: {}", qn.as_quarter_notes());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn beats_to_quarter_notes(
        &self,
        position: daw_proto::PositionInBeats,
    ) -> crate::Result<daw_proto::PositionInQuarterNotes> {
        let result = self
            .clients
            .position_conversion
            .beats_to_quarter_notes(daw_proto::ProjectContext::project(&self.guid), position)
            .await?;
        Ok(result)
    }

    /// Convert quarter notes to beats
    ///
    /// In most cases quarter notes == beats, but this can vary with time signature.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    /// use daw_proto::PositionInQuarterNotes;
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// let project = daw.current_project().await?;
    ///
    /// let qn = PositionInQuarterNotes::from_quarter_notes(8.0);
    /// let beats = project.quarter_notes_to_beats(qn).await?;
    /// println!("Beats: {}", beats.as_beats());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn quarter_notes_to_beats(
        &self,
        position: daw_proto::PositionInQuarterNotes,
    ) -> crate::Result<daw_proto::PositionInBeats> {
        let result = self
            .clients
            .position_conversion
            .quarter_notes_to_beats(daw_proto::ProjectContext::project(&self.guid), position)
            .await?;
        Ok(result)
    }

    // =========================================================================
    // Items Access
    // =========================================================================

    /// Get access to all items in the project
    ///
    /// Returns a handle for project-wide item operations like getting all items
    /// or selected items.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use daw_control::Daw;
    ///
    /// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
    /// let daw = Daw::new(handle);
    /// let project = daw.current_project().await?;
    ///
    /// // Get all selected items
    /// let selected = project.items().selected().await?;
    /// for item in selected {
    ///     println!("Selected item: {}", item.guid());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn items(&self) -> ProjectItems {
        ProjectItems::new(self.guid.clone(), self.clients.clone())
    }
}

impl std::fmt::Debug for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Project").field("guid", &self.guid).finish()
    }
}
