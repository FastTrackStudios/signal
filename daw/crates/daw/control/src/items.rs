//! Items handle and ItemHandle for individual items

use std::sync::Arc;

use crate::Result;
use crate::{DawClients, Error, MidiEditor};
use daw_proto::{
    ProjectContext,
    item::{FadeShape, Item, ItemRef, ItemSpan, Take, TakeRef},
    primitives::{Duration, PositionInSeconds},
    track::TrackRef,
};

/// Items handle for a specific track
///
/// This handle provides access to item enumeration and operations on a track.
/// Individual item operations are performed through [`ItemHandle`].
#[derive(Clone)]
pub struct Items {
    track_guid: String,
    project_id: String,
    clients: Arc<DawClients>,
}

impl Items {
    /// Create a new items handle for a track
    pub(crate) fn new(track_guid: String, project_id: String, clients: Arc<DawClients>) -> Self {
        Self {
            track_guid,
            project_id,
            clients,
        }
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
    // Query Methods
    // =========================================================================

    /// Get all items on this track
    pub async fn all(&self) -> Result<Vec<Item>> {
        let items = self
            .clients
            .item
            .get_items(self.context(), self.track_ref())
            .await?;
        Ok(items)
    }

    /// Get item by index
    pub async fn by_index(&self, index: u32) -> Result<Option<ItemHandle>> {
        let items = self
            .clients
            .item
            .get_items(self.context(), self.track_ref())
            .await?;

        let item = items.into_iter().nth(index as usize);

        Ok(item.map(|i| {
            ItemHandle::new(
                i.guid,
                self.track_guid.clone(),
                self.project_id.clone(),
                self.clients.clone(),
            )
        }))
    }

    /// Get item by GUID
    pub async fn by_guid(&self, guid: &str) -> Result<Option<ItemHandle>> {
        let item = self
            .clients
            .item
            .get_item(self.context(), ItemRef::Guid(guid.to_string()))
            .await?;

        Ok(item.map(|i| {
            ItemHandle::new(
                i.guid,
                self.track_guid.clone(),
                self.project_id.clone(),
                self.clients.clone(),
            )
        }))
    }

    /// Get item count on this track
    pub async fn count(&self) -> Result<u32> {
        let count = self
            .clients
            .item
            .item_count(self.context(), self.track_ref())
            .await?;
        Ok(count)
    }

    // =========================================================================
    // Create Items
    // =========================================================================

    /// Add a new empty item at the given position
    pub async fn add(&self, position: PositionInSeconds, length: Duration) -> Result<ItemHandle> {
        let guid = self
            .clients
            .item
            .add_item(self.context(), self.track_ref(), position, length)
            .await?
            .ok_or_else(|| Error::Other("Failed to create item".to_string()))?;

        Ok(ItemHandle::new(
            guid,
            self.track_guid.clone(),
            self.project_id.clone(),
            self.clients.clone(),
        ))
    }

    /// [`Items::add`], but the item takes `guid` instead of a fresh one —
    /// how a peer re-creates an item another engine made. A `guid` the
    /// project already has is an error (`DawError::AlreadyExists`). Give
    /// it a source afterwards through its takes (`set_source_file`).
    pub async fn add_with_guid(
        &self,
        guid: &str,
        position: PositionInSeconds,
        length: Duration,
    ) -> Result<ItemHandle> {
        let guid = self
            .clients
            .item
            .add_item_with_guid(
                self.context(),
                self.track_ref(),
                guid.to_string(),
                ItemSpan::new(position, length),
            )
            .await??;
        Ok(ItemHandle::new(
            guid,
            self.track_guid.clone(),
            self.project_id.clone(),
            self.clients.clone(),
        ))
    }

    /// Create a MIDI item on this track and add notes to it.
    ///
    /// Combines `create_midi_item` + `add_notes` into a single operation.
    /// Returns the item handle if successful.
    pub async fn create_midi_item_with_notes(
        &self,
        start_seconds: f64,
        end_seconds: f64,
        notes: Vec<daw_proto::MidiNoteCreate>,
    ) -> Result<Option<ItemHandle>> {
        // Create empty MIDI item
        let location = self
            .clients
            .midi
            .create_midi_item(self.context(), self.track_ref(), start_seconds, end_seconds)
            .await?;

        let Some(location) = location else {
            return Ok(None);
        };

        // Add notes
        if !notes.is_empty() {
            self.clients.midi.add_notes(location.clone(), notes).await?;
        }

        // Return handle to the created item
        let item_guid = match &location.item {
            daw_proto::item::ItemRef::Guid(g) => g.clone(),
            _ => return Ok(None),
        };

        Ok(Some(ItemHandle::new(
            item_guid,
            self.track_guid.clone(),
            self.project_id.clone(),
            self.clients.clone(),
        )))
    }
}

impl std::fmt::Debug for Items {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Items")
            .field("track_guid", &self.track_guid)
            .field("project_id", &self.project_id)
            .finish()
    }
}

/// Project-wide items accessor
///
/// Provides access to all items in a project, selected items, etc.
#[derive(Clone)]
pub struct ProjectItems {
    project_id: String,
    clients: Arc<DawClients>,
}

impl ProjectItems {
    /// Create a new project items handle
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

    /// Get all items in the project
    pub async fn all(&self) -> Result<Vec<Item>> {
        let items = self.clients.item.get_all_items(self.context()).await?;
        Ok(items)
    }

    /// Get all selected items
    pub async fn selected(&self) -> Result<Vec<ItemHandle>> {
        let items = self.clients.item.get_selected_items(self.context()).await?;
        Ok(items
            .into_iter()
            .map(|i| {
                ItemHandle::new(
                    i.guid.clone(),
                    i.track_guid,
                    self.project_id.clone(),
                    self.clients.clone(),
                )
            })
            .collect())
    }

    /// Get item by GUID
    pub async fn by_guid(&self, guid: &str) -> Result<Option<ItemHandle>> {
        let item = self
            .clients
            .item
            .get_item(self.context(), ItemRef::Guid(guid.to_string()))
            .await?;

        Ok(item.map(|i| {
            ItemHandle::new(
                i.guid.clone(),
                i.track_guid,
                self.project_id.clone(),
                self.clients.clone(),
            )
        }))
    }

    /// Create an item with `guid` on the track whose guid is
    /// `track_guid` — [`Items::add_with_guid`] without first taking a
    /// track handle.
    pub async fn add_with_guid(
        &self,
        track_guid: &str,
        guid: &str,
        position: PositionInSeconds,
        length: Duration,
    ) -> Result<ItemHandle> {
        Items::new(
            track_guid.to_string(),
            self.project_id.clone(),
            self.clients.clone(),
        )
        .add_with_guid(guid, position, length)
        .await
    }

    /// Select all items
    pub async fn select_all(&self) -> Result<()> {
        self.clients
            .item
            .select_all_items(self.context(), true)
            .await??;
        Ok(())
    }

    /// Deselect all items
    pub async fn deselect_all(&self) -> Result<()> {
        self.clients
            .item
            .select_all_items(self.context(), false)
            .await??;
        Ok(())
    }

    // Item + take subscriptions retired with the architect::rpc port —
    // sibling-trait territory if streaming surfaces a real consumer.
}

impl std::fmt::Debug for ProjectItems {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectItems")
            .field("project_id", &self.project_id)
            .finish()
    }
}

// =============================================================================
// ItemHandle
// =============================================================================

/// Handle to a single item
#[derive(Clone)]
pub struct ItemHandle {
    item_guid: String,
    track_guid: String,
    project_id: String,
    clients: Arc<DawClients>,
}

impl ItemHandle {
    /// Create a new item handle
    pub(crate) fn new(
        item_guid: String,
        track_guid: String,
        project_id: String,
        clients: Arc<DawClients>,
    ) -> Self {
        Self {
            item_guid,
            track_guid,
            project_id,
            clients,
        }
    }

    /// Get the item GUID
    pub fn guid(&self) -> &str {
        &self.item_guid
    }

    /// Get the track GUID
    pub fn track_guid(&self) -> &str {
        &self.track_guid
    }

    /// Helper to create project context
    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    /// Helper to create item reference
    fn item_ref(&self) -> ItemRef {
        ItemRef::Guid(self.item_guid.clone())
    }

    // =========================================================================
    // Info
    // =========================================================================

    /// Get full item state
    pub async fn info(&self) -> Result<Item> {
        self.clients
            .item
            .get_item(self.context(), self.item_ref())
            .await?
            .ok_or_else(|| Error::Other(format!("Item not found: {}", self.item_guid)))
    }

    // =========================================================================
    // Position/Length
    // =========================================================================

    /// Get item position
    pub async fn position(&self) -> Result<PositionInSeconds> {
        Ok(self.info().await?.position)
    }

    /// Set item position
    pub async fn set_position(&self, position: PositionInSeconds) -> Result<()> {
        self.clients
            .item
            .set_position(self.context(), self.item_ref(), position)
            .await??;
        Ok(())
    }

    /// Get item length
    pub async fn length(&self) -> Result<Duration> {
        Ok(self.info().await?.length)
    }

    /// Set item length
    pub async fn set_length(&self, length: Duration) -> Result<()> {
        self.clients
            .item
            .set_length(self.context(), self.item_ref(), length)
            .await??;
        Ok(())
    }

    // =========================================================================
    // State
    // =========================================================================

    /// Mute the item
    pub async fn mute(&self) -> Result<()> {
        self.clients
            .item
            .set_muted(self.context(), self.item_ref(), true)
            .await??;
        Ok(())
    }

    /// Unmute the item
    pub async fn unmute(&self) -> Result<()> {
        self.clients
            .item
            .set_muted(self.context(), self.item_ref(), false)
            .await??;
        Ok(())
    }

    /// Set item color (None to use default)
    pub async fn set_color(&self, color: Option<u32>) -> Result<()> {
        self.clients
            .item
            .set_color(self.context(), self.item_ref(), color)
            .await??;
        Ok(())
    }

    /// The item's on-screen label (REAPER's `P_NOTES`).
    ///
    /// Where an item records what it *means* — a chord symbol, a key
    /// change — visibly and hand-editably. Not MIDI notes.
    pub async fn label(&self) -> Result<Option<String>> {
        Ok(self
            .clients
            .item
            .label(self.context(), self.item_ref())
            .await?)
    }

    /// Set the item's label. See [`ItemHandle::label`].
    pub async fn set_label(&self, label: &str) -> Result<()> {
        self.clients
            .item
            .set_label(self.context(), self.item_ref(), label.to_string())
            .await??;
        Ok(())
    }

    /// Move the item onto fixed lane `lane` of its track.
    pub async fn set_fixed_lane(&self, lane: u32) -> Result<()> {
        self.clients
            .item
            .set_fixed_lane(self.context(), self.item_ref(), lane)
            .await??;
        Ok(())
    }

    /// Select the item
    pub async fn select(&self) -> Result<()> {
        self.clients
            .item
            .set_selected(self.context(), self.item_ref(), true)
            .await??;
        Ok(())
    }

    /// Deselect the item
    pub async fn deselect(&self) -> Result<()> {
        self.clients
            .item
            .set_selected(self.context(), self.item_ref(), false)
            .await??;
        Ok(())
    }

    /// Lock the item
    pub async fn lock(&self) -> Result<()> {
        self.clients
            .item
            .set_locked(self.context(), self.item_ref(), true)
            .await??;
        Ok(())
    }

    /// Loop its source across it, or play it once.
    pub async fn set_loop_source(&self, loop_source: bool) -> Result<()> {
        self.clients
            .item
            .set_loop_source(self.context(), self.item_ref(), loop_source)
            .await??;
        Ok(())
    }

    /// Unlock the item
    pub async fn unlock(&self) -> Result<()> {
        self.clients
            .item
            .set_locked(self.context(), self.item_ref(), false)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Audio
    // =========================================================================

    /// Get item volume
    pub async fn volume(&self) -> Result<f64> {
        Ok(self.info().await?.volume)
    }

    /// Set item volume
    pub async fn set_volume(&self, volume: f64) -> Result<()> {
        self.clients
            .item
            .set_volume(self.context(), self.item_ref(), volume)
            .await??;
        Ok(())
    }

    /// Set fade in
    pub async fn set_fade_in(&self, length: Duration, shape: FadeShape) -> Result<()> {
        self.clients
            .item
            .set_fade_in(self.context(), self.item_ref(), length, shape)
            .await??;
        Ok(())
    }

    /// Set fade out
    pub async fn set_fade_out(&self, length: Duration, shape: FadeShape) -> Result<()> {
        self.clients
            .item
            .set_fade_out(self.context(), self.item_ref(), length, shape)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Operations
    // =========================================================================

    /// Move this item to a different track
    pub async fn move_to_track(&self, track: TrackRef) -> Result<()> {
        self.clients
            .item
            .move_to_track(self.context(), self.item_ref(), track)
            .await??;
        Ok(())
    }

    /// Delete this item
    pub async fn delete(&self) -> Result<()> {
        self.clients
            .item
            .delete_item(self.context(), self.item_ref())
            .await??;
        Ok(())
    }

    /// Duplicate this item
    pub async fn duplicate(&self) -> Result<ItemHandle> {
        let guid = self
            .clients
            .item
            .duplicate_item(self.context(), self.item_ref())
            .await?
            .ok_or_else(|| Error::Other("Failed to duplicate item".to_string()))?;

        Ok(ItemHandle::new(
            guid,
            self.track_guid.clone(),
            self.project_id.clone(),
            self.clients.clone(),
        ))
    }

    // =========================================================================
    // Takes
    // =========================================================================

    /// Get access to this item's takes
    pub fn takes(&self) -> Takes {
        Takes::new(
            self.item_guid.clone(),
            self.project_id.clone(),
            self.clients.clone(),
        )
    }

    /// Get the active take
    pub fn active_take(&self) -> TakeHandle {
        TakeHandle::new(
            self.item_guid.clone(),
            TakeRef::Active,
            self.project_id.clone(),
            self.clients.clone(),
        )
    }
}

impl std::fmt::Debug for ItemHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemHandle")
            .field("item_guid", &self.item_guid)
            .field("track_guid", &self.track_guid)
            .field("project_id", &self.project_id)
            .finish()
    }
}

// =============================================================================
// Takes
// =============================================================================

/// Takes accessor for an item
#[derive(Clone)]
pub struct Takes {
    item_guid: String,
    project_id: String,
    clients: Arc<DawClients>,
}

impl Takes {
    /// Create a new takes handle
    pub(crate) fn new(item_guid: String, project_id: String, clients: Arc<DawClients>) -> Self {
        Self {
            item_guid,
            project_id,
            clients,
        }
    }

    /// Helper to create project context
    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    /// Helper to create item reference
    fn item_ref(&self) -> ItemRef {
        ItemRef::Guid(self.item_guid.clone())
    }

    /// Get all takes
    pub async fn all(&self) -> Result<Vec<Take>> {
        let takes = self
            .clients
            .take
            .get_takes(self.context(), self.item_ref())
            .await?;
        Ok(takes)
    }

    /// Get take by index
    pub async fn by_index(&self, index: u32) -> Result<Option<TakeHandle>> {
        let take = self
            .clients
            .take
            .get_take(self.context(), self.item_ref(), TakeRef::Index(index))
            .await?;

        Ok(take.map(|_| {
            TakeHandle::new(
                self.item_guid.clone(),
                TakeRef::Index(index),
                self.project_id.clone(),
                self.clients.clone(),
            )
        }))
    }

    /// Get the active take
    pub async fn active(&self) -> Result<TakeHandle> {
        let _ = self
            .clients
            .take
            .get_active_take(self.context(), self.item_ref())
            .await?
            .ok_or_else(|| Error::Other("No active take".to_string()))?;

        Ok(TakeHandle::new(
            self.item_guid.clone(),
            TakeRef::Active,
            self.project_id.clone(),
            self.clients.clone(),
        ))
    }

    /// Add a new take
    pub async fn add(&self) -> Result<TakeHandle> {
        let guid = self
            .clients
            .take
            .add_take(self.context(), self.item_ref())
            .await?
            .ok_or_else(|| Error::Other("Failed to create take".to_string()))?;

        Ok(TakeHandle::new(
            self.item_guid.clone(),
            TakeRef::Guid(guid),
            self.project_id.clone(),
            self.clients.clone(),
        ))
    }
}

impl std::fmt::Debug for Takes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Takes")
            .field("item_guid", &self.item_guid)
            .field("project_id", &self.project_id)
            .finish()
    }
}

// =============================================================================
// TakeHandle
// =============================================================================

/// Handle to a single take
#[derive(Clone)]
pub struct TakeHandle {
    item_guid: String,
    take_ref: TakeRef,
    project_id: String,
    clients: Arc<DawClients>,
}

impl TakeHandle {
    /// Create a new take handle
    pub(crate) fn new(
        item_guid: String,
        take_ref: TakeRef,
        project_id: String,
        clients: Arc<DawClients>,
    ) -> Self {
        Self {
            item_guid,
            take_ref,
            project_id,
            clients,
        }
    }

    /// Helper to create project context
    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    /// Helper to create item reference
    fn item_ref(&self) -> ItemRef {
        ItemRef::Guid(self.item_guid.clone())
    }

    // =========================================================================
    // Info
    // =========================================================================

    /// Get full take state
    pub async fn info(&self) -> Result<Take> {
        self.clients
            .take
            .get_take(self.context(), self.item_ref(), self.take_ref.clone())
            .await?
            .ok_or_else(|| Error::Other("Take not found".to_string()))
    }

    /// Waveform peak data for this take.
    ///
    /// `block_size` is samples per peak — 1024 to 4096 is typical; larger
    /// is fewer peaks. Both backends answer in the same shape: one
    /// `(min ≤ 0, max ≥ 0)` pair per channel per block, blocks of
    /// `block_size` source samples of take playback time (start offset
    /// and play rate applied), covering the item's length. Under REAPER
    /// this goes through `PCM_source::GetPeaks`, served from the
    /// `.reapeaks` cache — the same data REAPER's own arrange view draws,
    /// with no decoding of the media file. Standalone reads the take's
    /// materialized source, folding from its own REAPER-compatible
    /// `.reapeaks` sidecar at coarse zooms, and caches the result per
    /// `(take, block_size)` at the project revision.
    pub async fn peaks(&self, block_size: u32) -> Result<daw_proto::TakePeakData> {
        let peaks = self
            .clients
            .peaks
            .take_peaks(
                self.context(),
                self.item_ref(),
                self.take_ref.clone(),
                block_size,
            )
            .await?;
        Ok(peaks)
    }

    // =========================================================================
    // Metadata
    // =========================================================================

    /// Get take name
    pub async fn name(&self) -> Result<String> {
        Ok(self.info().await?.name)
    }

    /// Set take name
    pub async fn set_name(&self, name: &str) -> Result<()> {
        self.clients
            .take
            .set_name(
                self.context(),
                self.item_ref(),
                self.take_ref.clone(),
                name.to_string(),
            )
            .await??;
        Ok(())
    }

    // =========================================================================
    // Playback
    // =========================================================================

    /// Get take pitch adjustment (semitones)
    pub async fn pitch(&self) -> Result<f64> {
        Ok(self.info().await?.pitch)
    }

    /// Set take pitch adjustment (semitones)
    pub async fn set_pitch(&self, semitones: f64) -> Result<()> {
        self.clients
            .take
            .set_pitch(
                self.context(),
                self.item_ref(),
                self.take_ref.clone(),
                semitones,
            )
            .await??;
        Ok(())
    }

    /// Get take play rate
    pub async fn play_rate(&self) -> Result<f64> {
        Ok(self.info().await?.play_rate)
    }

    /// Set the take's start offset into its source (`D_STARTOFFS`).
    pub async fn set_start_offset(&self, offset: Duration) -> Result<()> {
        self.clients
            .take
            .set_start_offset(
                self.context(),
                self.item_ref(),
                self.take_ref.clone(),
                offset,
            )
            .await??;
        Ok(())
    }

    /// The take's stretch markers, in order.
    pub async fn stretch_markers(&self) -> Result<Vec<daw_proto::StretchMarker>> {
        Ok(self
            .clients
            .stretch_markers
            .get_stretch_markers(self.context(), self.item_ref(), self.take_ref.clone())
            .await?)
    }

    /// Replace the take's stretch markers with `markers`.
    pub async fn set_stretch_markers(&self, markers: Vec<daw_proto::StretchMarker>) -> Result<()> {
        self.clients
            .stretch_markers
            .set_stretch_markers(
                daw_proto::StretchTakeRef::new(
                    self.context(),
                    self.item_ref(),
                    self.take_ref.clone(),
                ),
                markers,
            )
            .await??;
        Ok(())
    }

    /// Set take play rate
    pub async fn set_play_rate(&self, rate: f64) -> Result<()> {
        self.clients
            .take
            .set_play_rate(self.context(), self.item_ref(), self.take_ref.clone(), rate)
            .await??;
        Ok(())
    }

    /// Get take volume
    pub async fn volume(&self) -> Result<f64> {
        Ok(self.info().await?.volume)
    }

    /// Set take volume
    pub async fn set_volume(&self, volume: f64) -> Result<()> {
        self.clients
            .take
            .set_volume(
                self.context(),
                self.item_ref(),
                self.take_ref.clone(),
                volume,
            )
            .await??;
        Ok(())
    }

    // =========================================================================
    // Operations
    // =========================================================================

    /// Make this take the active take
    pub async fn make_active(&self) -> Result<()> {
        self.clients
            .take
            .set_active_take(self.context(), self.item_ref(), self.take_ref.clone())
            .await??;
        Ok(())
    }

    /// Delete this take
    pub async fn delete(&self) -> Result<()> {
        self.clients
            .take
            .delete_take(self.context(), self.item_ref(), self.take_ref.clone())
            .await??;
        Ok(())
    }

    /// Set source file
    pub async fn set_source_file(&self, path: &str) -> Result<()> {
        self.clients
            .take
            .set_source_file(
                self.context(),
                self.item_ref(),
                self.take_ref.clone(),
                path.to_string(),
            )
            .await??;
        Ok(())
    }

    /// Set the take's custom color. Pass `None` to clear it.
    pub async fn set_color(&self, color: Option<u32>) -> Result<()> {
        self.clients
            .take
            .set_color(
                self.context(),
                self.item_ref(),
                self.take_ref.clone(),
                color,
            )
            .await??;
        Ok(())
    }

    /// Toggle preserve-pitch-when-changing-rate (`B_PPITCH`).
    pub async fn set_preserve_pitch(&self, preserve: bool) -> Result<()> {
        self.clients
            .take
            .set_preserve_pitch(
                self.context(),
                self.item_ref(),
                self.take_ref.clone(),
                preserve,
            )
            .await??;
        Ok(())
    }

    /// Detect the take's source type (`Audio`, `Midi`, `Video`, `Empty`).
    pub async fn source_type(&self) -> Result<daw_proto::SourceType> {
        Ok(self
            .clients
            .take
            .get_source_type(self.context(), self.item_ref(), self.take_ref.clone())
            .await?)
    }

    // =========================================================================
    // Take Markers
    // =========================================================================

    /// List all take markers in source-PPQ order.
    pub async fn markers(&self) -> Result<Vec<daw_proto::TakeMarker>> {
        Ok(self
            .clients
            .take
            .get_take_markers(self.context(), self.item_ref(), self.take_ref.clone())
            .await?)
    }

    /// Append a new take marker at an exact source-time position.
    /// Returns the new enumeration index.
    ///
    /// `source_position_seconds` is in the take's own source time
    /// (REAPER's `srcpos`) — `0.0` is the start of the source media,
    /// independent of where the item sits on the timeline. For
    /// project-time positioning (drop a marker at "second verse" or
    /// "playhead position"), use [`Self::add_marker_at`].
    pub async fn add_marker(
        &self,
        name: &str,
        source_position_seconds: f64,
        color: Option<u32>,
    ) -> Result<Option<u32>> {
        Ok(self
            .clients
            .take
            .add_take_marker(
                self.context(),
                self.item_ref(),
                self.take_ref.clone(),
                daw_proto::TakeMarkerCreate {
                    name: name.to_string(),
                    source_position_seconds,
                    color,
                    ..Default::default()
                },
            )
            .await?)
    }

    /// Replace every marker on the take's source with `markers`, as one
    /// change — what a detection run writes. Each keeps its `id`.
    pub async fn set_markers(&self, markers: Vec<daw_proto::TakeMarker>) -> Result<()> {
        self.clients
            .take
            .set_take_markers(
                self.context(),
                self.item_ref(),
                self.take_ref.clone(),
                markers,
            )
            .await??;
        Ok(())
    }

    /// Update an existing take marker. Pass `None` for fields that should
    /// keep their current value; for `color`, `Some(None)` clears the color.
    pub async fn update_marker(
        &self,
        index: u32,
        name: Option<&str>,
        source_position_seconds: Option<f64>,
        color: Option<Option<u32>>,
    ) -> Result<()> {
        self.clients
            .take
            .set_take_marker(
                self.context(),
                self.item_ref(),
                self.take_ref.clone(),
                daw_proto::TakeMarkerUpdate {
                    index,
                    name: name.map(|s| s.to_string()),
                    source_position_seconds,
                    color,
                    ..Default::default()
                },
            )
            .await??;
        Ok(())
    }

    /// Place a take marker at an absolute project-timeline position. The
    /// service does the project-time → take-source-time conversion (the
    /// item's start position, the take's start offset, and its play rate
    /// all factor in).
    ///
    /// Returns:
    /// * `Ok(Some(index))` — marker added; the value is REAPER's
    ///   enumeration index for the new marker.
    /// * `Ok(None)` — the position fell outside the item's playable
    ///   region (or the resulting source-time was negative). The service
    ///   logs a warning and does nothing else.
    ///
    /// Use this for "drop a marker at the playhead", "annotate at second
    /// verse", etc. — anywhere the caller is thinking in terms of project
    /// time / musical time, not the take's source frame.
    pub async fn add_marker_at(
        &self,
        position: daw_proto::Position,
        name: &str,
        color: Option<u32>,
    ) -> Result<Option<u32>> {
        Ok(self
            .clients
            .take
            .add_take_marker_at_position(
                self.context(),
                self.item_ref(),
                self.take_ref.clone(),
                daw_proto::AddTakeMarkerAtPositionRequest {
                    name: name.to_string(),
                    position,
                    color,
                },
            )
            .await?)
    }

    /// Delete the take marker at the given enumeration index.
    pub async fn delete_marker(&self, index: u32) -> Result<()> {
        self.clients
            .take
            .delete_take_marker(
                self.context(),
                self.item_ref(),
                self.take_ref.clone(),
                index,
            )
            .await??;
        Ok(())
    }

    // =========================================================================
    // Take Ratings (REAPER 7.17+)
    // =========================================================================

    /// Detect this take's current rating by reading its take markers.
    /// Returns the *first* up-rank or down-rank marker found, or `None` if
    /// the take is unranked. Multiple rank markers on one take aren't
    /// expected — REAPER's actions clear before re-ranking — but we don't
    /// validate.
    pub async fn rating(&self) -> Result<Option<daw_proto::TakeRating>> {
        let markers = self.markers().await?;
        Ok(markers
            .iter()
            .find_map(|m| daw_proto::TakeRating::from_marker_name(&m.name)))
    }

    /// Up-rank the active take one level. Wraps REAPER action 42680
    /// (`Item: Up-rank active take or last recording pass`). The
    /// underlying service snapshots and restores the prior item selection
    /// so this is safe to call mid-session.
    pub async fn up_rank(&self) -> Result<()> {
        self.run_rating_action(daw_proto::take_rating_actions::UP_RANK_ACTIVE_TAKE)
            .await
    }

    /// Cycle through up-rank/down-rank levels (action 43197). The cycle
    /// is up→max→down→neutral→up→… per REAPER's pref settings.
    pub async fn cycle_rating(&self) -> Result<()> {
        self.run_rating_action(daw_proto::take_rating_actions::CYCLE_RANK_ACTIVE_TAKE)
            .await
    }

    /// Clear all up-rank/down-rank markers on this take's item (action
    /// 43161). Affects every take on the same item, not just this one.
    pub async fn clear_rating(&self) -> Result<()> {
        self.run_rating_action(daw_proto::take_rating_actions::CLEAR_RANK_SELECTED_ITEMS)
            .await
    }

    /// Run an arbitrary REAPER take-ranking action against this take. See
    /// [`daw_proto::take_rating_actions`] for the documented command IDs.
    pub async fn run_rating_action(&self, command_id: u32) -> Result<()> {
        self.clients
            .take
            .run_take_rating_action(
                self.context(),
                self.item_ref(),
                self.take_ref.clone(),
                command_id,
            )
            .await??;
        Ok(())
    }

    // =========================================================================
    // MIDI Editing
    // =========================================================================

    /// Get MIDI editor for this take (only for MIDI takes)
    ///
    /// Returns a handle for editing MIDI notes, CC events, and other MIDI data
    /// in this take. The take must be a MIDI take.
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
    /// let item = project.items().selected().await?.into_iter().next().unwrap();
    /// let midi = item.active_take().midi();
    ///
    /// // Add a note (middle C, velocity 100, at beat 0, duration 1 beat)
    /// midi.add_note(60, 100, 0.0, 1.0).await?;
    ///
    /// // Quantize selected notes to 1/4 note grid
    /// midi.quantize(1.0, 1.0).await?;
    /// # Ok(())
    /// # }
    /// ```
    /// This take's own envelopes — volume, pan, mute, pitch.
    ///
    /// Async because the envelope reference is by take *guid*, and a
    /// handle built from a `TakeRef::Active` does not know it yet. One
    /// lookup here beats every caller doing it.
    pub async fn envelopes(&self) -> Result<crate::automation::TakeEnvelopes> {
        let take = self.info().await?;
        Ok(crate::automation::TakeEnvelopes::new(
            self.item_guid.clone(),
            take.guid,
            self.project_id.clone(),
            self.clients.clone(),
        ))
    }

    pub fn midi(&self) -> MidiEditor {
        MidiEditor::new(
            self.item_guid.clone(),
            self.take_ref.clone(),
            self.project_id.clone(),
            self.clients.clone(),
        )
    }
}

impl std::fmt::Debug for TakeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TakeHandle")
            .field("item_guid", &self.item_guid)
            .field("take_ref", &self.take_ref)
            .field("project_id", &self.project_id)
            .finish()
    }
}
