//! Tracks handle and TrackHandle for individual tracks

use std::sync::Arc;

use crate::Result;
use crate::{DawClients, Envelopes, Error, FxChain, HardwareOutputs, Items, Receives, Sends};
use daw_proto::track::{Comp, CompArea, LaneComping, ReorderTracksBehavior};
use daw_proto::{FxChainContext, ProjectContext, Track, TrackRef};

/// Tracks handle for a specific project
///
/// This handle provides access to track enumeration and batch operations.
/// Individual track operations are performed through [`TrackHandle`].
///
/// # Example
///
/// ```no_run
/// use daw_control::Daw;
///
/// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
/// let daw = Daw::new(handle);
/// let project = daw.current_project().await?;
/// let tracks = project.tracks();
///
/// // Enumerate tracks
/// for track in tracks.all().await? {
///     println!("Track: {} ({})", track.name, track.guid);
/// }
///
/// // Get specific track
/// let vocals = tracks.by_name("Vocals").await?;
/// if let Some(track) = vocals {
///     track.solo_exclusive().await?;
/// }
///
/// // Batch operations
/// tracks.clear_solo().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Tracks {
    project_id: String,
    clients: Arc<DawClients>,
}

impl Tracks {
    /// Create a new tracks handle for a project
    pub(crate) fn new(project_id: String, clients: Arc<DawClients>) -> Self {
        Self {
            project_id,
            clients,
        }
    }

    /// Helper to create project context
    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    // =========================================================================
    // Query Methods
    // =========================================================================

    /// Get all tracks in the project
    pub async fn all(&self) -> Result<Vec<Track>> {
        let tracks = self.clients.track.all(self.context()).await?;
        Ok(tracks)
    }

    /// Get track by index
    pub async fn by_index(&self, index: u32) -> Result<Option<TrackHandle>> {
        let track = self
            .clients
            .track
            .get(self.context(), TrackRef::Index(index))
            .await?;

        Ok(track.map(|t| TrackHandle::new(t.guid, self.project_id.clone(), self.clients.clone())))
    }

    /// Get track by GUID
    pub async fn by_guid(&self, guid: &str) -> Result<Option<TrackHandle>> {
        // List-and-find dodges the vox JIT/schema issue on `Option<Track>`
        // responses from `track.get`. `track.all` (returning `Vec<Track>`)
        // serializes cleanly through both transports.
        let all = self.clients.track.all(self.context()).await?;
        Ok(all
            .into_iter()
            .find(|t| t.guid == guid)
            .map(|t| TrackHandle::new(t.guid, self.project_id.clone(), self.clients.clone())))
    }

    /// Get track by name (first match)
    pub async fn by_name(&self, name: &str) -> Result<Option<TrackHandle>> {
        // Get all tracks and find first match
        let tracks = self.clients.track.all(self.context()).await?;
        let track = tracks.into_iter().find(|t| t.name == name);

        Ok(track.map(|t| TrackHandle::new(t.guid, self.project_id.clone(), self.clients.clone())))
    }

    /// Get the master track
    pub async fn master(&self) -> Result<TrackHandle> {
        let track = self
            .clients
            .track
            .master(self.context())
            .await?
            .ok_or_else(|| Error::Other("No master track found".to_string()))?;

        Ok(TrackHandle::new(
            track.guid,
            self.project_id.clone(),
            self.clients.clone(),
        ))
    }

    /// Get all currently selected tracks
    pub async fn selected(&self) -> Result<Vec<TrackHandle>> {
        let tracks = self.clients.track.selected(self.context()).await?;

        Ok(tracks
            .into_iter()
            .map(|t| TrackHandle::new(t.guid, self.project_id.clone(), self.clients.clone()))
            .collect())
    }

    /// Get total track count (excluding master)
    pub async fn count(&self) -> Result<u32> {
        let count = self.clients.track.count(self.context()).await?;
        Ok(count)
    }

    // =========================================================================
    // Batch Operations
    // =========================================================================

    /// Clear solo from all tracks
    pub async fn clear_solo(&self) -> Result<()> {
        self.clients.track.clear_all_solo(self.context()).await??;
        Ok(())
    }

    /// Mute all tracks
    pub async fn mute_all(&self) -> Result<()> {
        self.clients.track.mute_all(self.context()).await??;
        Ok(())
    }

    /// Unmute all tracks
    pub async fn unmute_all(&self) -> Result<()> {
        self.clients.track.unmute_all(self.context()).await??;
        Ok(())
    }

    /// Clear selection from all tracks
    pub async fn clear_selection(&self) -> Result<()> {
        self.clients.track.clear_selection(self.context()).await??;
        Ok(())
    }

    /// Move one track so it ends up at `index` (0-based, project order),
    /// leaving the selection alone. Folder depths are not touched — see
    /// `daw_proto::Tracks::move_to`.
    pub async fn move_to(&self, track: TrackRef, index: u32) -> Result<()> {
        self.clients
            .track
            .move_to(self.context(), track, index)
            .await??;
        Ok(())
    }

    /// Move all currently selected tracks to `index`.
    pub async fn reorder_selected(
        &self,
        index: u32,
        behavior: ReorderTracksBehavior,
    ) -> Result<()> {
        self.clients
            .track
            .reorder_selected(self.context(), index, behavior)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Bulk Operations
    // =========================================================================

    // =========================================================================
    // Track Creation / Deletion
    // =========================================================================

    /// Add a new track to the project.
    ///
    /// If `at_index` is `Some(i)`, inserts at that position (0-based), shifting
    /// existing tracks down. If `None`, appends at the end.
    /// Returns a [`TrackHandle`] for the newly created track.
    pub async fn add(&self, name: &str, at_index: Option<u32>) -> Result<TrackHandle> {
        // Architect-emitted client returns `Result<DawResult<T>, vox>`;
        // `.await??` flattens both transport + app errors. An empty
        // guid was a sentinel-failure under the old async surface
        // (REAPER refusing the op without returning an error); the new
        // sync `Tracks::add` returns `DawResult<String>` directly, so
        // any failure surfaces here as `Err` and we don't need the
        // empty-guid check.
        let guid = self
            .clients
            .track
            .add(self.context(), name.to_string(), at_index)
            .await??;
        Ok(TrackHandle::new(
            guid,
            self.project_id.clone(),
            self.clients.clone(),
        ))
    }

    /// [`Tracks::add`], but the track takes `guid` instead of a fresh one
    /// — how a peer re-creates a track another engine made. A `guid` the
    /// project already has is an error (`DawError::AlreadyExists`). The
    /// handle carries the guid as the backend stored it (verbatim on the
    /// standalone engine; REAPER's own spelling there).
    pub async fn add_with_guid(
        &self,
        guid: &str,
        name: &str,
        at_index: Option<u32>,
    ) -> Result<TrackHandle> {
        let guid = self
            .clients
            .track
            .add_with_guid(self.context(), guid.to_string(), name.to_string(), at_index)
            .await??;
        Ok(TrackHandle::new(
            guid,
            self.project_id.clone(),
            self.clients.clone(),
        ))
    }

    /// Remove a track from the project by GUID, index, or master reference.
    pub async fn remove(&self, track: daw_proto::TrackRef) -> Result<()> {
        self.clients.track.remove(self.context(), track).await??;
        Ok(())
    }

    /// Remove all tracks from the project (excluding master).
    pub async fn remove_all(&self) -> Result<()> {
        self.clients.track.remove_all(self.context()).await??;
        Ok(())
    }

    // =========================================================================
    // Track groups (project-level)
    // =========================================================================

    /// Name track-group `slot` (1-based, 1..=128). Reads back through
    /// `Project::get_info_string("TRACK_GROUP_NAME:<slot>")`.
    pub async fn set_group_name(&self, slot: u32, name: &str) -> Result<()> {
        self.clients
            .track
            .set_group_name(self.context(), slot, name.to_string())
            .await??;
        Ok(())
    }

    /// First slot in `[band_start, band_end]` no track is a member of
    /// through any family or modifier; `None` when the band is full.
    pub async fn first_free_group_slot(
        &self,
        band_start: u32,
        band_end: u32,
    ) -> Result<Option<u32>> {
        Ok(self
            .clients
            .track
            .first_free_group_slot(self.context(), band_start, band_end)
            .await?)
    }

    // =========================================================================
    // Streaming
    // =========================================================================

    /// Subscribe to track add/remove/modify events. Returns an
    /// [`crate::EventStream`] that yields this project's
    /// `TrackStreamEvent`s until the subscription ends (drop it to
    /// unsubscribe). The server streams every open project's events
    /// (argless `#[subscribe]` stream); this handle filters to its
    /// own `project_guid` client-side.
    pub async fn subscribe(
        &self,
    ) -> Result<crate::EventStream<daw_proto::track::TrackStreamEvent>> {
        let (raw_tx, raw_rx) = vox::channel();
        let stream = self.clients.track_stream.clone();
        let want = self.project_id.clone();
        Ok(crate::EventStream::spawn(
            async move {
                let _ = stream.events(raw_tx).await;
            },
            raw_rx,
            Box::new(move |ev| ev.project_guid == want),
        ))
    }
}

impl std::fmt::Debug for Tracks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tracks")
            .field("project_id", &self.project_id)
            .finish()
    }
}

// =============================================================================
// TrackHandle
// =============================================================================

/// Handle to a single track - all track operations
///
/// This handle represents a specific track in a DAW project. It's lightweight
/// (stores only GUIDs) and cheap to clone.
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
/// // Get a track and work with it
/// let track = project.tracks().by_name("Vocals").await?.unwrap();
///
/// // Solo/mute
/// track.solo_exclusive().await?;
/// track.mute().await?;
///
/// // Volume/pan
/// track.set_volume(0.8).await?;
/// track.set_pan(-0.3).await?;
///
/// // Access FX chain
/// let fx = track.fx_chain().by_name("ReaComp").await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct TrackHandle {
    track_guid: String,
    project_id: String,
    clients: Arc<DawClients>,
}

impl TrackHandle {
    /// Create a new track handle
    pub(crate) fn new(track_guid: String, project_id: String, clients: Arc<DawClients>) -> Self {
        Self {
            track_guid,
            project_id,
            clients,
        }
    }

    /// Get the track GUID
    pub fn guid(&self) -> &str {
        &self.track_guid
    }

    /// Helper to create project context
    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    /// Helper to create track reference
    fn track_ref(&self) -> TrackRef {
        TrackRef::Guid(self.track_guid.clone())
    }

    // =========================================================================
    // Info
    // =========================================================================

    /// Get full track state
    pub async fn info(&self) -> Result<Track> {
        self.clients
            .track
            .get(self.context(), self.track_ref())
            .await?
            .ok_or_else(|| Error::Other(format!("Track not found: {}", self.track_guid)))
    }

    // =========================================================================
    // Mute
    // =========================================================================

    /// Mute the track
    pub async fn mute(&self) -> Result<()> {
        self.clients
            .track
            .set_muted(self.context(), self.track_ref(), true)
            .await??;
        Ok(())
    }

    /// Unmute the track
    pub async fn unmute(&self) -> Result<()> {
        self.clients
            .track
            .set_muted(self.context(), self.track_ref(), false)
            .await??;
        Ok(())
    }

    /// Toggle mute state
    pub async fn toggle_mute(&self) -> Result<()> {
        let info = self.info().await?;
        self.clients
            .track
            .set_muted(self.context(), self.track_ref(), !info.muted)
            .await??;
        Ok(())
    }

    /// Check if track is muted
    pub async fn is_muted(&self) -> Result<bool> {
        Ok(self.info().await?.muted)
    }

    // =========================================================================
    // Solo
    // =========================================================================

    /// Solo the track
    pub async fn solo(&self) -> Result<()> {
        self.clients
            .track
            .set_soloed(self.context(), self.track_ref(), true)
            .await??;
        Ok(())
    }

    /// Unsolo the track
    pub async fn unsolo(&self) -> Result<()> {
        self.clients
            .track
            .set_soloed(self.context(), self.track_ref(), false)
            .await??;
        Ok(())
    }

    /// Toggle solo state
    pub async fn toggle_solo(&self) -> Result<()> {
        let info = self.info().await?;
        self.clients
            .track
            .set_soloed(self.context(), self.track_ref(), !info.soloed)
            .await??;
        Ok(())
    }

    /// Solo this track exclusively (unsolo all others)
    pub async fn solo_exclusive(&self) -> Result<()> {
        self.clients
            .track
            .set_solo_exclusive(self.context(), self.track_ref())
            .await??;
        Ok(())
    }

    /// Check if track is soloed
    pub async fn is_soloed(&self) -> Result<bool> {
        Ok(self.info().await?.soloed)
    }

    // =========================================================================
    // Arm
    // =========================================================================

    /// Arm the track for recording
    pub async fn arm(&self) -> Result<()> {
        self.clients
            .track
            .set_armed(self.context(), self.track_ref(), true)
            .await??;
        Ok(())
    }

    /// Disarm the track
    pub async fn disarm(&self) -> Result<()> {
        self.clients
            .track
            .set_armed(self.context(), self.track_ref(), false)
            .await??;
        Ok(())
    }

    /// Toggle arm state
    pub async fn toggle_arm(&self) -> Result<()> {
        let info = self.info().await?;
        self.clients
            .track
            .set_armed(self.context(), self.track_ref(), !info.armed)
            .await??;
        Ok(())
    }

    /// Check if track is armed
    pub async fn is_armed(&self) -> Result<bool> {
        Ok(self.info().await?.armed)
    }

    // =========================================================================
    // Volume/Pan
    // =========================================================================

    /// Get track volume (0.0 = -inf dB, 1.0 = 0 dB)
    pub async fn volume(&self) -> Result<f64> {
        Ok(self.info().await?.volume)
    }

    /// Set track volume (0.0 = -inf dB, 1.0 = 0 dB)
    pub async fn set_volume(&self, volume: f64) -> Result<()> {
        self.clients
            .track
            .set_volume(self.context(), self.track_ref(), volume)
            .await??;
        Ok(())
    }

    /// Set the track automation mode (trim/read/touch/write/latch).
    pub async fn set_automation_mode(&self, mode: daw_proto::AutomationMode) -> Result<()> {
        self.clients
            .track
            .set_automation_mode(self.context(), self.track_ref(), mode)
            .await??;
        Ok(())
    }

    /// Set record-input monitoring (off / on / tape-auto).
    pub async fn set_input_monitor(
        &self,
        monitor: daw_proto::track::InputMonitoringMode,
    ) -> Result<()> {
        self.clients
            .track
            .set_input_monitor(self.context(), self.track_ref(), monitor)
            .await??;
        Ok(())
    }

    /// Set the track's record-input source (hardware audio channel, MIDI, or
    /// raw REAPER `I_RECINPUT`). Mirrors [`set_input_monitor`](Self::set_input_monitor).
    pub async fn set_record_input(&self, input: daw_proto::track::RecordInput) -> Result<()> {
        self.clients
            .track
            .set_record_input(self.context(), self.track_ref(), input)
            .await??;
        Ok(())
    }

    /// Set polarity / phase invert.
    pub async fn set_phase_inverted(&self, inverted: bool) -> Result<()> {
        self.clients
            .track
            .set_phase_inverted(self.context(), self.track_ref(), inverted)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Track groups
    // =========================================================================

    /// Add or remove the track from `slot` as a mutual member (every
    /// family, lead and follow).
    pub async fn set_group_membership(&self, slot: u32, member: bool) -> Result<()> {
        self.clients
            .track
            .set_group_membership(self.context(), self.track_ref(), slot, member)
            .await??;
        Ok(())
    }

    /// Set the track's role in one family of one slot (1-based, 1..=128).
    pub async fn set_group_flags(
        &self,
        slot: u32,
        family: daw_proto::track::GroupFamily,
        role: daw_proto::track::GroupRole,
    ) -> Result<()> {
        self.clients
            .track
            .set_group_flags(
                self.context(),
                self.track_ref(),
                daw_proto::track::GroupFlagChange { slot, family, role },
            )
            .await??;
        Ok(())
    }

    /// Switch one modifier of one slot on or off.
    pub async fn set_group_modifier(
        &self,
        slot: u32,
        modifier: daw_proto::track::GroupModifier,
        enabled: bool,
    ) -> Result<()> {
        self.clients
            .track
            .set_group_modifier(
                self.context(),
                self.track_ref(),
                daw_proto::track::GroupModifierChange {
                    slot,
                    modifier,
                    enabled,
                },
            )
            .await??;
        Ok(())
    }

    /// The track's live group membership over all slots.
    pub async fn group_flags(&self) -> Result<daw_proto::track::TrackGrouping> {
        Ok(self
            .clients
            .track
            .group_flags(self.context(), self.track_ref())
            .await??)
    }

    /// Get track pan (-1.0 = left, 0.0 = center, 1.0 = right)
    pub async fn pan(&self) -> Result<f64> {
        Ok(self.info().await?.pan)
    }

    /// Set track pan (-1.0 = left, 0.0 = center, 1.0 = right)
    pub async fn set_pan(&self, pan: f64) -> Result<()> {
        self.clients
            .track
            .set_pan(self.context(), self.track_ref(), pan)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Selection
    // =========================================================================

    /// Select the track
    pub async fn select(&self) -> Result<()> {
        self.clients
            .track
            .set_selected(self.context(), self.track_ref(), true)
            .await??;
        Ok(())
    }

    /// Deselect the track
    pub async fn deselect(&self) -> Result<()> {
        self.clients
            .track
            .set_selected(self.context(), self.track_ref(), false)
            .await??;
        Ok(())
    }

    /// Select this track exclusively (deselect all others)
    pub async fn select_exclusive(&self) -> Result<()> {
        self.clients
            .track
            .select_exclusive(self.context(), self.track_ref())
            .await??;
        Ok(())
    }

    // =========================================================================
    // Track Management
    // =========================================================================

    /// Rename the track
    pub async fn rename(&self, name: &str) -> Result<()> {
        self.clients
            .track
            .rename(self.context(), self.track_ref(), name.to_string())
            .await??;
        Ok(())
    }

    /// Set track color (0xRRGGBB format, or 0 for default)
    pub async fn set_color(&self, color: u32) -> Result<()> {
        self.clients
            .track
            .set_color(self.context(), self.track_ref(), color)
            .await??;
        Ok(())
    }

    /// Set REAPER folder-depth change for this track.
    pub async fn set_folder_depth(&self, folder_depth: i32) -> Result<()> {
        self.clients
            .track
            .set_folder_depth(self.context(), self.track_ref(), folder_depth)
            .await??;
        Ok(())
    }

    /// Move this track so it ends up at `index` (0-based, project order),
    /// leaving the selection alone. Its folder depth is not touched — set
    /// it with [`TrackHandle::set_folder_depth`] if the move should change
    /// folder membership.
    pub async fn move_to(&self, index: u32) -> Result<()> {
        self.clients
            .track
            .move_to(self.context(), self.track_ref(), index)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Visibility
    // =========================================================================

    /// Set track visibility in the arrange view and mixer.
    pub async fn set_visibility(&self, visible_in_tcp: bool, visible_in_mixer: bool) -> Result<()> {
        self.clients
            .track
            .set_visibility(
                self.context(),
                self.track_ref(),
                visible_in_tcp,
                visible_in_mixer,
            )
            .await??;
        Ok(())
    }

    /// Set the arrange-view track height override in pixels. Use `0` to clear it.
    pub async fn set_tcp_height(&self, height_pixels: u32) -> Result<()> {
        self.clients
            .track
            .set_tcp_height(self.context(), self.track_ref(), height_pixels)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Fixed lanes and comping
    // =========================================================================

    /// Set the number of fixed item lanes (`0` switches lanes off).
    pub async fn set_lane_count(&self, count: u32) -> Result<()> {
        self.clients
            .track
            .set_lane_count(self.context(), self.track_ref(), count)
            .await??;
        Ok(())
    }

    /// Set which lanes play: bit n = lane n audible.
    pub async fn set_lane_play_mask(&self, mask: u64) -> Result<()> {
        self.clients
            .track
            .set_lane_play_mask(self.context(), self.track_ref(), mask)
            .await??;
        Ok(())
    }

    /// Every ext-state key the track carries, with its value, by key.
    pub async fn ext_state(&self) -> Result<Vec<daw_proto::TrackExtEntry>> {
        Ok(self
            .clients
            .track
            .ext_state(self.context(), self.track_ref())
            .await?)
    }

    /// Set the ext-state `key` to `value`; an empty value removes it.
    pub async fn set_ext_state(&self, key: &str, value: &str) -> Result<()> {
        self.clients
            .track
            .set_ext_state(
                self.context(),
                self.track_ref(),
                key.to_string(),
                value.to_string(),
            )
            .await??;
        Ok(())
    }

    /// Name a lane. Also how a comp is renamed — a comp's name is its
    /// lane's name.
    pub async fn set_lane_name(&self, lane: u32, name: &str) -> Result<()> {
        self.clients
            .track
            .set_lane_name(self.context(), self.track_ref(), lane, name.to_string())
            .await??;
        Ok(())
    }

    /// The track's comping state: record / comping lanes and comp areas.
    pub async fn comping(&self) -> Result<LaneComping> {
        Ok(self
            .clients
            .track
            .comping(self.context(), self.track_ref())
            .await??)
    }

    /// Replace the track's comp areas.
    pub async fn set_comp_areas(&self, areas: Vec<CompArea>) -> Result<()> {
        self.clients
            .track
            .set_comp_areas(self.context(), self.track_ref(), areas)
            .await??;
        Ok(())
    }

    /// The track's named comps, ascending by lane.
    pub async fn comps(&self) -> Result<Vec<Comp>> {
        Ok(self
            .clients
            .track
            .comps(self.context(), self.track_ref())
            .await??)
    }

    /// Add a lane named `name`, make it the comping lane, and return its
    /// index.
    pub async fn create_comp(&self, name: &str) -> Result<u32> {
        Ok(self
            .clients
            .track
            .create_comp(self.context(), self.track_ref(), name.to_string())
            .await??)
    }

    /// Make `lane` the comping lane (`None` = no comp active).
    pub async fn set_active_comp(&self, lane: Option<u32>) -> Result<()> {
        self.clients
            .track
            .set_active_comp(self.context(), self.track_ref(), lane)
            .await??;
        Ok(())
    }

    // =========================================================================
    // FX Chain Access
    // =========================================================================

    /// Get the track's FX chain (output/playback)
    pub fn fx_chain(&self) -> FxChain {
        FxChain::new(
            FxChainContext::Track(self.track_guid.clone()),
            self.project_id.clone(),
            self.clients.clone(),
        )
    }

    /// Get the track's input FX chain (recording)
    pub fn input_fx_chain(&self) -> FxChain {
        FxChain::new(
            FxChainContext::Input(self.track_guid.clone()),
            self.project_id.clone(),
            self.clients.clone(),
        )
    }

    // =========================================================================
    // Items Access
    // =========================================================================

    /// Get access to items on this track
    pub fn items(&self) -> Items {
        Items::new(
            self.track_guid.clone(),
            self.project_id.clone(),
            self.clients.clone(),
        )
    }

    // =========================================================================
    // Parent Send (folder routing)
    // =========================================================================

    /// Enable or disable the parent send (folder bus routing).
    ///
    /// When disabled, audio from this track does not flow to the parent
    /// folder track — it only flows through explicit sends.
    pub async fn set_parent_send(&self, enabled: bool) -> Result<()> {
        self.clients
            .routing
            .set_parent_send_enabled(self.context(), self.track_ref(), enabled)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Routing Access
    // =========================================================================

    /// Get access to sends from this track
    pub fn sends(&self) -> Sends {
        Sends::new(
            self.track_guid.clone(),
            self.project_id.clone(),
            self.clients.clone(),
        )
    }

    /// Get access to receives to this track
    pub fn receives(&self) -> Receives {
        Receives::new(
            self.track_guid.clone(),
            self.project_id.clone(),
            self.clients.clone(),
        )
    }

    /// Get access to hardware outputs from this track
    pub fn hardware_outputs(&self) -> HardwareOutputs {
        HardwareOutputs::new(
            self.track_guid.clone(),
            self.project_id.clone(),
            self.clients.clone(),
        )
    }

    // =========================================================================
    // Automation Access
    // =========================================================================

    /// Get access to automation envelopes on this track
    pub fn envelopes(&self) -> Envelopes {
        Envelopes::new(
            self.track_guid.clone(),
            self.project_id.clone(),
            self.clients.clone(),
        )
    }

    /// Get the volume envelope
    pub fn volume_envelope(&self) -> crate::EnvelopeHandle {
        self.envelopes().volume()
    }

    /// Get the pan envelope
    pub fn pan_envelope(&self) -> crate::EnvelopeHandle {
        self.envelopes().pan()
    }

    // =========================================================================
    // Track ExtState (P_EXT)
    // =========================================================================
}

impl std::fmt::Debug for TrackHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackHandle")
            .field("track_guid", &self.track_guid)
            .field("project_id", &self.project_id)
            .finish()
    }
}
