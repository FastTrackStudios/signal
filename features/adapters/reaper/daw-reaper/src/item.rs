//! REAPER Item and Take Implementation
//!
//! Implements ItemService and TakeService by dispatching REAPER API calls to the main thread
//! via [`crate::main_thread`].

use crate::main_thread;
use crate::project_context::{
    MAX_PROJECT_TABS, project_guid as project_guid_from, resolve_project_context,
};
use crate::safe_wrappers::item as item_sw;
use crate::track::{resolve_project, resolve_track};
use daw_control::lock::LockExt;
use daw_proto::{
    BeatAttachMode, Duration, FadeShape, Item, ItemEvent, ItemRef, PositionInSeconds,
    ProjectContext, SourceType, Take, TakeRef, TrackRef,
};
use reaper_high::{Project, Reaper};
use reaper_medium::{
    DurationInSeconds, ItemAttributeKey, MediaItem, MediaItemTake,
    ProjectContext as ReaperProjectContext, ProjectRef, TakeAttributeKey, UiRefreshBehavior,
};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tokio::sync::broadcast;

// =============================================================================
// Helpers
// =============================================================================

/// Extract a REAPER item's GUID as a braced string (e.g., "{XXXXXXXX-XXXX-...}")
pub(crate) fn item_guid_string(medium: &reaper_medium::Reaper, item: MediaItem) -> String {
    let guid_ptr = unsafe {
        medium
            .low()
            .GetSetMediaItemInfo(item.as_ptr(), c"GUID".as_ptr() as _, std::ptr::null_mut())
            as *const reaper_low::raw::GUID
    };
    if !guid_ptr.is_null() {
        let g = unsafe { &*guid_ptr };
        format!(
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
            g.Data1,
            g.Data2,
            g.Data3,
            g.Data4[0],
            g.Data4[1],
            g.Data4[2],
            g.Data4[3],
            g.Data4[4],
            g.Data4[5],
            g.Data4[6],
            g.Data4[7]
        )
    } else {
        format!("{:p}", item.as_ptr())
    }
}

// =============================================================================
// Broadcaster / Cache State
// =============================================================================

/// Global item event broadcaster
static ITEM_BROADCASTER: OnceLock<broadcast::Sender<ItemEvent>> = OnceLock::new();

// `TAKE_BROADCASTER` retired alongside the architect::rpc port — see
// `crate::take`. Sibling-trait territory if take streaming returns.

/// Per-project cached item states for change detection.
/// Key is project GUID, value is a vec of lightweight snapshots.
static ITEM_CACHE: OnceLock<Mutex<HashMap<String, Vec<CachedItemState>>>> = OnceLock::new();

/// Lightweight snapshot of item state used for fast diff comparisons.
#[derive(Clone, Debug)]
struct CachedItemState {
    guid: String,
    track_guid: String,
    /// The item's fixed lane, `None` off a fixed-lanes track. Carried so
    /// a `Created` event announces the lane the item is actually on.
    fixed_lane: Option<u32>,
    position: f64,
    length: f64,
    muted: bool,
    selected: bool,
    volume: f64,
    active_take_index: u32,
}

/// Thresholds for floating-point change detection
const ITEM_POSITION_THRESHOLD: f64 = 0.001; // 1ms
const ITEM_LENGTH_THRESHOLD: f64 = 0.001; // 1ms
const ITEM_VOLUME_THRESHOLD: f64 = 0.0001;

/// Send one item event to both places it has to go: the dedicated
/// broadcast channel and the cross-domain bus.
///
/// One function rather than two calls at each site, because the second
/// of two calls is the one that gets forgotten when a variant is added.
fn emit_item(tx: Option<&broadcast::Sender<ItemEvent>>, event: ItemEvent) {
    crate::event_hub::hub().publish_item(event.clone());
    if let Some(tx) = tx {
        let _ = tx.send(event);
    }
}

/// Initialize item and take broadcasters.
/// Called by the extension during initialization.
pub fn init_item_broadcaster() {
    let (tx, _rx) = broadcast::channel::<ItemEvent>(1024);
    let _ = ITEM_BROADCASTER.set(tx);
    let _ = ITEM_CACHE.set(Mutex::new(HashMap::new()));
}

/// Subscribe to item events. Returns `None` until [`init_item_broadcaster`] runs.
pub fn subscribe_items() -> Option<broadcast::Receiver<ItemEvent>> {
    ITEM_BROADCASTER.get().map(|tx| tx.subscribe())
}

/// Poll REAPER item state for ALL open projects and broadcast changes.
/// **MUST be called from the main thread** (e.g., from timer callback).
///
/// Uses a two-phase approach:
/// 1. Quick count check per project — if item count matches cached count, skip full diff
/// 2. Full per-item diff when count changes or any item state is stale
pub fn poll_and_broadcast_items() {
    let item_tx = ITEM_BROADCASTER.get();

    // Anyone on the cross-domain bus counts as a subscriber too. They
    // did not before, so a client watching the bus for item moves kept
    // this poller asleep and then waited forever for the events it was
    // keeping asleep — a silence that reads exactly like a broken
    // stream.
    let has_item_subs = item_tx.map(|t| t.receiver_count() > 0).unwrap_or(false)
        || crate::event_hub::hub().items_subscriber_count() > 0;
    if !has_item_subs {
        return;
    }

    let Some(cache) = ITEM_CACHE.get() else {
        return;
    };
    let mut cache_guard = cache.lock_recoverable("item");

    let reaper = Reaper::get();
    let medium = reaper.medium_reaper();
    let low = medium.low();

    let mut seen_guids = Vec::new();

    // Iterate through all open projects
    for tab_index in 0..MAX_PROJECT_TABS {
        let Some(result) = medium.enum_projects(ProjectRef::Tab(tab_index), 0) else {
            break;
        };

        let project = Project::new(result.project);
        let project_guid = project_guid_from(&project);
        seen_guids.push(project_guid.clone());

        let reaper_ctx = ReaperProjectContext::Proj(result.project);

        // Phase 1: quick count check
        let current_count = medium.count_media_items(reaper_ctx);
        let cached_items = cache_guard.get(&project_guid);
        let cached_count = cached_items.map(|v| v.len() as u32).unwrap_or(u32::MAX);

        // Phase 2: build current snapshot and diff
        let mut current_states = Vec::with_capacity(current_count as usize);
        for i in 0..current_count {
            let Some(item) = medium.get_media_item(reaper_ctx, i) else {
                continue;
            };

            let guid = item_sw::get_item_state_chunk(low, item, 1024)
                .and_then(|chunk| extract_guid_from_chunk(&chunk))
                .unwrap_or_default();

            let track = item_sw::get_media_item_track(medium, item);
            let track_guid = track
                .map(|t| item_sw::get_track_guid(low, t))
                .unwrap_or_default();
            let fixed_lane = track.and_then(|t| item_fixed_lane(medium, item, t));

            let position = item_sw::get_item_info_value(medium, item, ItemAttributeKey::Position);
            let length = item_sw::get_item_info_value(medium, item, ItemAttributeKey::Length);
            let muted = item_sw::get_item_info_value(medium, item, ItemAttributeKey::Mute) != 0.0;
            let selected = item_sw::is_item_selected(low, item);
            let volume = item_sw::get_item_info_value(medium, item, ItemAttributeKey::Vol);

            let active_take = item_sw::get_active_take(medium, item);
            let take_count = item_sw::count_takes(low, item) as u32;
            let active_take_index = if let Some(active) = active_take {
                let mut idx = 0u32;
                for ti in 0..take_count {
                    if let Some(t) = item_sw::get_take(low, item, ti as i32)
                        && t == active
                    {
                        idx = ti;
                        break;
                    }
                }
                idx
            } else {
                0
            };

            current_states.push(CachedItemState {
                guid,
                track_guid,
                fixed_lane,
                position,
                length,
                muted,
                selected,
                volume,
                active_take_index,
            });
        }

        // If counts match and we have a cache, do per-item diff
        if current_count == cached_count
            && let Some(prev_states) = cached_items
        {
            // Compare each item
            for (prev, curr) in prev_states.iter().zip(current_states.iter()) {
                emit_item_diffs(item_tx, &project_guid, prev, curr);
            }
            // Update cache
            cache_guard.insert(project_guid.clone(), current_states);
            continue;
        }

        // Count mismatch or no cache — detect creates/deletes and update everything
        if let Some(prev_states) = cached_items.cloned() {
            // Build lookup of previous GUIDs
            let prev_guids: HashMap<&str, &CachedItemState> =
                prev_states.iter().map(|s| (s.guid.as_str(), s)).collect();
            let curr_guids: HashMap<&str, &CachedItemState> = current_states
                .iter()
                .map(|s| (s.guid.as_str(), s))
                .collect();

            // Deleted items: in prev but not in curr
            for prev in &prev_states {
                if !prev.guid.is_empty() && !curr_guids.contains_key(prev.guid.as_str()) {
                    emit_item(
                        item_tx,
                        ItemEvent::Deleted {
                            project_guid: project_guid.clone(),
                            track_guid: prev.track_guid.clone(),
                            item_guid: prev.guid.clone(),
                        },
                    );
                }
            }

            // Created items: in curr but not in prev; changed items: in both
            for curr in &current_states {
                if curr.guid.is_empty() {
                    continue;
                }
                if let Some(prev) = prev_guids.get(curr.guid.as_str()) {
                    emit_item_diffs(item_tx, &project_guid, prev, curr);
                } else {
                    // New item — emit Created with a full Item snapshot
                    // We build a lightweight Item here; the subscriber can query for full details
                    {
                        emit_item(
                            item_tx,
                            ItemEvent::Created {
                                project_guid: project_guid.clone(),
                                track_guid: curr.track_guid.clone(),
                                item: Item {
                                    label: None,
                                    guid: curr.guid.clone(),
                                    track_guid: curr.track_guid.clone(),
                                    index: 0,
                                    position: daw_proto::PositionInSeconds::from_seconds(
                                        curr.position,
                                    ),
                                    length: Duration::from_seconds(curr.length),
                                    snap_offset: Duration::from_seconds(0.0),
                                    muted: curr.muted,
                                    selected: curr.selected,
                                    locked: false,
                                    volume: curr.volume,
                                    fade_in_length: Duration::from_seconds(0.0),
                                    fade_out_length: Duration::from_seconds(0.0),
                                    fade_in_shape: FadeShape::Linear,
                                    fade_out_shape: FadeShape::Linear,
                                    beat_attach_mode: BeatAttachMode::Time,
                                    loop_source: false,
                                    auto_stretch: false,
                                    color: None,
                                    group_id: None,
                                    fixed_lane: curr.fixed_lane,
                                    start_offset: Duration::ZERO,
                                    take_count: 0,
                                    active_take_index: curr.active_take_index,
                                },
                            },
                        );
                    }
                }
            }
        } else {
            // No previous cache at all — first poll, don't emit events
        }

        cache_guard.insert(project_guid.clone(), current_states);
    }

    // Clean up cache entries for projects that are no longer open
    cache_guard.retain(|guid, _| seen_guids.contains(guid));
}

/// Emit diff events for a single item by comparing previous and current cached states.
fn emit_item_diffs(
    item_tx: Option<&broadcast::Sender<ItemEvent>>,
    project_guid: &str,
    prev: &CachedItemState,
    curr: &CachedItemState,
) {
    if (prev.position - curr.position).abs() > ITEM_POSITION_THRESHOLD {
        emit_item(
            item_tx,
            ItemEvent::PositionChanged {
                project_guid: project_guid.to_string(),
                item_guid: curr.guid.clone(),
                old_position: prev.position,
                new_position: curr.position,
            },
        );
    }

    if (prev.length - curr.length).abs() > ITEM_LENGTH_THRESHOLD {
        emit_item(
            item_tx,
            ItemEvent::LengthChanged {
                project_guid: project_guid.to_string(),
                item_guid: curr.guid.clone(),
                old_length: prev.length,
                new_length: curr.length,
            },
        );
    }

    if prev.track_guid != curr.track_guid {
        emit_item(
            item_tx,
            ItemEvent::MovedToTrack {
                project_guid: project_guid.to_string(),
                item_guid: curr.guid.clone(),
                old_track_guid: prev.track_guid.clone(),
                new_track_guid: curr.track_guid.clone(),
            },
        );
    }

    if prev.muted != curr.muted {
        emit_item(
            item_tx,
            ItemEvent::MuteChanged {
                project_guid: project_guid.to_string(),
                item_guid: curr.guid.clone(),
                muted: curr.muted,
            },
        );
    }

    if prev.selected != curr.selected {
        emit_item(
            item_tx,
            ItemEvent::SelectionChanged {
                project_guid: project_guid.to_string(),
                item_guid: curr.guid.clone(),
                selected: curr.selected,
            },
        );
    }

    if (prev.volume - curr.volume).abs() > ITEM_VOLUME_THRESHOLD {
        emit_item(
            item_tx,
            ItemEvent::VolumeChanged {
                project_guid: project_guid.to_string(),
                item_guid: curr.guid.clone(),
                volume: curr.volume,
            },
        );
    }

    if prev.active_take_index != curr.active_take_index {
        emit_item(
            item_tx,
            ItemEvent::ActiveTakeChanged {
                project_guid: project_guid.to_string(),
                item_guid: curr.guid.clone(),
                old_take_index: prev.active_take_index,
                new_take_index: curr.active_take_index,
            },
        );
    }
}

// =============================================================================
// ReaperItem
// =============================================================================

/// REAPER item implementation.
///
/// All methods dispatch to the main thread via [`crate::main_thread`].
#[derive(Clone)]
pub struct ReaperItem;

impl ReaperItem {
    pub fn new() -> Self {
        Self
    }

    /// Resolve an ItemRef to a MediaItem pointer.
    ///
    /// Validates the pointer after resolution to guard against stale items.
    pub(crate) fn resolve_item(
        item_ref: &ItemRef,
        project: ReaperProjectContext,
    ) -> Option<MediaItem> {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        let item = match item_ref {
            ItemRef::Guid(guid) => {
                // Try the specified project first, then fall back to CurrentProject
                Self::find_item_by_guid(medium, project, guid).or_else(|| {
                    if !matches!(project, ReaperProjectContext::CurrentProject) {
                        Self::find_item_by_guid(medium, ReaperProjectContext::CurrentProject, guid)
                    } else {
                        None
                    }
                })?
            }
            ItemRef::Index(idx) => {
                let track = medium.get_track(project, 0)?;
                item_sw::get_track_media_item(medium, track, *idx)?
            }
            ItemRef::ProjectIndex(idx) => medium.get_media_item(project, *idx)?,
        };
        if !main_thread::is_item_valid(project, item) {
            return None;
        }
        Some(item)
    }

    /// Find a media item by GUID within a specific project context.
    fn find_item_by_guid(
        medium: &reaper_medium::Reaper,
        project: ReaperProjectContext,
        guid: &str,
    ) -> Option<MediaItem> {
        let item_count = medium.count_media_items(project);
        for i in 0..item_count {
            if let Some(candidate) = medium.get_media_item(project, i) {
                let guid_ptr = unsafe {
                    medium.low().GetSetMediaItemInfo(
                        candidate.as_ptr(),
                        c"GUID".as_ptr() as _,
                        std::ptr::null_mut(),
                    ) as *const reaper_low::raw::GUID
                };
                if !guid_ptr.is_null() {
                    let g = unsafe { &*guid_ptr };
                    let item_guid = format!(
                        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
                        g.Data1,
                        g.Data2,
                        g.Data3,
                        g.Data4[0],
                        g.Data4[1],
                        g.Data4[2],
                        g.Data4[3],
                        g.Data4[4],
                        g.Data4[5],
                        g.Data4[6],
                        g.Data4[7]
                    );
                    if item_guid == *guid {
                        return Some(candidate);
                    }
                }
            }
        }
        None
    }

    /// Convert MediaItem to Item struct
    pub(crate) fn media_item_to_item(item: MediaItem) -> Option<Item> {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();
        let low = medium.low();

        let guid = item_sw::get_item_state_chunk(low, item, 1024)
            .and_then(|chunk| extract_guid_from_chunk(&chunk))
            .unwrap_or_default();

        let track = item_sw::get_media_item_track(medium, item)?;
        let track_guid = item_sw::get_track_guid(low, track);
        let fixed_lane = item_fixed_lane(medium, item, track);

        let position = item_sw::get_item_info_value(medium, item, ItemAttributeKey::Position);
        let length = item_sw::get_item_info_value(medium, item, ItemAttributeKey::Length);
        let snap_offset = item_sw::get_item_info_value(medium, item, ItemAttributeKey::SnapOffset);

        let muted = item_sw::get_item_info_value(medium, item, ItemAttributeKey::Mute) != 0.0;
        let selected = item_sw::is_item_selected(low, item);
        // Lock is not available in reaper_medium ItemAttributeKey
        let locked = false;

        let volume = item_sw::get_item_info_value(medium, item, ItemAttributeKey::Vol);
        let fade_in_length =
            item_sw::get_item_info_value(medium, item, ItemAttributeKey::FadeInLen);
        let fade_out_length =
            item_sw::get_item_info_value(medium, item, ItemAttributeKey::FadeOutLen);

        // Fade shapes - REAPER uses different numbering
        let fade_in_shape_raw =
            item_sw::get_item_info_value(medium, item, ItemAttributeKey::FadeInShape) as u8;
        let fade_out_shape_raw =
            item_sw::get_item_info_value(medium, item, ItemAttributeKey::FadeOutShape) as u8;

        let loop_source =
            item_sw::get_item_info_value(medium, item, ItemAttributeKey::LoopSrc) != 0.0;

        let color_raw =
            item_sw::get_item_info_value(medium, item, ItemAttributeKey::CustomColor) as i32;
        let color = if color_raw > 0 {
            Some(color_raw as u32)
        } else {
            None
        };

        // C_BEATATTACHMODE: -1 → project default, 0 → time, 1 → full beats, 2 → position only.
        let beat_attach_mode =
            match item_sw::get_item_info_value(medium, item, ItemAttributeKey::BeatAttachMode)
                as i32
            {
                1 => BeatAttachMode::Beats,
                2 => BeatAttachMode::BeatsPositionOnly,
                // -1 (project default) and 0 (time) both map to Time; project
                // default is approximated as Time at the proto level.
                _ => BeatAttachMode::Time,
            };

        // C_AUTOSTRETCH: 1 = enabled (requires BeatAttachMode == 1).
        let auto_stretch =
            item_sw::get_item_info_value(medium, item, ItemAttributeKey::AutoStretch) != 0.0;

        // I_GROUPID: 0 = no group, otherwise an item group identifier.
        let group_id_raw =
            item_sw::get_item_info_value(medium, item, ItemAttributeKey::GroupId) as i32;
        let group_id = if group_id_raw > 0 {
            Some(group_id_raw as u32)
        } else {
            None
        };

        let take_count = item_sw::count_takes(low, item) as u32;

        let active_take = item_sw::get_active_take(medium, item);
        // Find active take index by comparing pointers
        let active_take_index = if let Some(active) = active_take {
            let mut found_index = 0;
            for i in 0..take_count {
                if let Some(take) = item_sw::get_take(low, item, i as i32)
                    && take == active
                {
                    found_index = i;
                    break;
                }
            }
            found_index
        } else {
            0
        };

        // The active take's offset into its source, for the window's
        // waveform. Zero when the item has no take — an empty item
        // draws nothing, so there is nothing to offset.
        let start_offset = active_take
            .map(|take| item_sw::get_take_info_value(medium, take, TakeAttributeKey::StartOffs))
            .unwrap_or(0.0);

        Some(Item {
            label: {
                let text = item_sw::get_item_notes(low, item);
                (!text.is_empty()).then_some(text)
            },
            guid,
            track_guid,
            index: 0, // Will be set by caller if needed
            position: PositionInSeconds::from_seconds(position),
            length: Duration::from_seconds(length),
            snap_offset: Duration::from_seconds(snap_offset),
            muted,
            selected,
            locked,
            volume,
            fade_in_length: Duration::from_seconds(fade_in_length),
            fade_out_length: Duration::from_seconds(fade_out_length),
            fade_in_shape: reaper_fade_to_proto(fade_in_shape_raw),
            fade_out_shape: reaper_fade_to_proto(fade_out_shape_raw),
            beat_attach_mode,
            loop_source,
            auto_stretch,
            color,
            group_id,
            fixed_lane,
            start_offset: Duration::from_seconds(start_offset),
            take_count,
            active_take_index,
        })
    }
}

/// The item's fixed lane (`I_FIXEDLANE`), only on a fixed-lanes track —
/// the value is meaningless elsewhere.
fn item_fixed_lane(
    medium: &reaper_medium::Reaper,
    item: MediaItem,
    track: reaper_medium::MediaTrack,
) -> Option<u32> {
    crate::lanes::has_fixed_lanes(track).then(|| {
        item_sw::get_item_info_value(medium, item, ItemAttributeKey::FixedLane).max(0.0) as u32
    })
}

impl Default for ReaperItem {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// architect::rpc port — `impl Items for crate::Reaper`
// =============================================================================
//
// Each call assumes main-thread execution (the architect bridge dispatches via
// `HasDispatcher`). Mount through `daw_proto::item::serve(Reaper)`. Mutations
// return `DawResult<()>` so callers can see why a handle resolved to nothing.

use daw_proto::Items;

fn item_not_found(item: &ItemRef) -> daw_proto::DawError {
    daw_proto::DawError::not_found("Item", &format!("{item:?}"))
}

fn track_not_found(track: &TrackRef) -> daw_proto::DawError {
    daw_proto::DawError::not_found("Track", &format!("{track:?}"))
}

impl Items for crate::Reaper {
    fn get_items(&self, project: ProjectContext, track: TrackRef) -> Vec<Item> {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();
        let proj_ctx = resolve_project_context(&project);

        let track_ptr = match &track {
            TrackRef::Master => Some(medium.get_master_track(proj_ctx)),
            TrackRef::Index(idx) => medium.get_track(proj_ctx, *idx),
            TrackRef::Guid(_) => resolve_project(&project)
                .or_else(|| Some(reaper.current_project()))
                .and_then(|proj| resolve_track(&proj, &track))
                .and_then(|t| t.raw().ok()),
        };

        let Some(track) = track_ptr else {
            return Vec::new();
        };
        let count = item_sw::count_track_media_items(medium, track);
        let mut items = Vec::with_capacity(count as usize);
        for i in 0..count {
            if let Some(item) = item_sw::get_track_media_item(medium, track, i)
                && let Some(mut data) = ReaperItem::media_item_to_item(item)
            {
                data.index = i;
                items.push(data);
            }
        }
        items
    }

    fn get_item(&self, project: ProjectContext, item: ItemRef) -> Option<Item> {
        let proj_ctx = resolve_project_context(&project);
        let item_ptr = ReaperItem::resolve_item(&item, proj_ctx)?;
        ReaperItem::media_item_to_item(item_ptr)
    }

    fn get_all_items(&self, _project: ProjectContext) -> Vec<Item> {
        let medium = Reaper::get().medium_reaper();
        let count = medium.count_media_items(ReaperProjectContext::CurrentProject);
        let mut items = Vec::with_capacity(count as usize);
        for i in 0..count {
            if let Some(item) = medium.get_media_item(ReaperProjectContext::CurrentProject, i)
                && let Some(mut data) = ReaperItem::media_item_to_item(item)
            {
                data.index = i;
                items.push(data);
            }
        }
        items
    }

    fn get_selected_items(&self, _project: ProjectContext) -> Vec<Item> {
        let medium = Reaper::get().medium_reaper();
        let count = medium.count_selected_media_items(ReaperProjectContext::CurrentProject);
        let mut items = Vec::with_capacity(count as usize);
        for i in 0..count {
            if let Some(item) =
                medium.get_selected_media_item(ReaperProjectContext::CurrentProject, i)
                && let Some(data) = ReaperItem::media_item_to_item(item)
            {
                items.push(data);
            }
        }
        items
    }

    fn item_count(&self, project: ProjectContext, track: TrackRef) -> u32 {
        let proj = resolve_project(&project).unwrap_or_else(|| Reaper::get().current_project());
        resolve_track(&proj, &track)
            .map(|t| t.item_count())
            .unwrap_or(0)
    }

    fn add_item(
        &self,
        project: ProjectContext,
        track: TrackRef,
        position: PositionInSeconds,
        length: Duration,
    ) -> Option<String> {
        let proj = resolve_project(&project).unwrap_or_else(|| Reaper::get().current_project());
        let reaper_track = resolve_track(&proj, &track)?;
        let low = Reaper::get().medium_reaper().low();
        let track_ptr = reaper_track.raw().ok()?;
        let start = position.as_seconds();
        let end = start + length.as_seconds();
        let item = crate::safe_wrappers::midi::create_new_midi_item(low, track_ptr, start, end)?;
        Reaper::get().medium_reaper().update_timeline();
        Some(item_guid_string(Reaper::get().medium_reaper(), item))
    }

    fn add_item_with_guid(
        &self,
        project: ProjectContext,
        track: TrackRef,
        guid: &str,
        span: daw_proto::ItemSpan,
    ) -> daw_proto::DawResult<String> {
        use crate::track::{guid_value, reaper_guid};
        use daw_proto::DawError;
        let (wanted, spelled) = reaper_guid("Item", guid)?;
        let proj = resolve_project(&project).unwrap_or_else(|| Reaper::get().current_project());
        let reaper_track = resolve_track(&proj, &track)
            .ok_or_else(|| DawError::not_found("Track", &format!("{track:?}")))?;
        let medium = Reaper::get().medium_reaper();
        let ctx = proj.context();
        let taken = (0..medium.count_media_items(ctx))
            .filter_map(|i| medium.get_media_item(ctx, i))
            .any(|item| guid_value(&item_guid_string(medium, item)) == Some(wanted));
        if taken {
            return Err(DawError::already_exists("Item", guid));
        }
        let low = medium.low();
        let track_ptr = reaper_track
            .raw()
            .map_err(|e| DawError::operation_failed(format!("track invalid: {e:?}")))?;
        let start = span.position.as_seconds();
        let end = start + span.length.as_seconds();
        let item = crate::safe_wrappers::midi::create_new_midi_item(low, track_ptr, start, end)
            .ok_or_else(|| DawError::operation_failed("CreateNewMIDIItemInProj failed"))?;
        item_sw::set_item_guid(low, item, &spelled);
        // Read back rather than trust the setter: an item REAPER did not
        // re-key must not be left behind under a guid nobody asked for.
        let stored = item_guid_string(medium, item);
        if guid_value(&stored) != Some(wanted) {
            if let Some(track) = item_sw::get_media_item_track(medium, item) {
                item_sw::delete_track_media_item(medium, track, item);
            }
            return Err(DawError::operation_failed(format!(
                "REAPER did not take item GUID {guid}"
            )));
        }
        medium.update_timeline();
        Ok(stored)
    }

    fn delete_item(&self, _project: ProjectContext, item: ItemRef) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let medium = Reaper::get().medium_reaper();
        let track = item_sw::get_media_item_track(medium, item_ptr).ok_or_else(|| {
            daw_proto::DawError::operation_failed("get_media_item_track returned None")
        })?;
        item_sw::delete_track_media_item(medium, track, item_ptr);
        Ok(())
    }

    fn duplicate_item(&self, _project: ProjectContext, item: ItemRef) -> Option<String> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)?;
        let medium = Reaper::get().medium_reaper();
        medium.select_all_media_items(ReaperProjectContext::CurrentProject, false);
        item_sw::set_media_item_selected(medium, item_ptr, true);
        item_sw::main_on_command_ex(
            medium,
            reaper_medium::CommandId::new(41295),
            0,
            ReaperProjectContext::CurrentProject,
        );
        let count = medium.count_selected_media_items(ReaperProjectContext::CurrentProject);
        if count > 0 {
            let new_item =
                medium.get_selected_media_item(ReaperProjectContext::CurrentProject, count - 1)?;
            Some(format!("{:p}", new_item.as_ptr()))
        } else {
            None
        }
    }

    fn set_position(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        position: PositionInSeconds,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let pos = reaper_medium::PositionInSeconds::new(position.as_seconds()).map_err(|e| {
            daw_proto::DawError::operation_failed(format!("invalid position: {e:?}"))
        })?;
        let medium = Reaper::get().medium_reaper();
        item_sw::set_media_item_position(medium, item_ptr, pos, UiRefreshBehavior::Refresh);
        Ok(())
    }

    fn set_length(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        length: Duration,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let len = DurationInSeconds::new(length.as_seconds())
            .map_err(|e| daw_proto::DawError::operation_failed(format!("invalid length: {e:?}")))?;
        let medium = Reaper::get().medium_reaper();
        item_sw::set_media_item_length(medium, item_ptr, len, UiRefreshBehavior::Refresh);
        Ok(())
    }

    fn move_to_track(
        &self,
        project: ProjectContext,
        item: ItemRef,
        track: TrackRef,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let proj = resolve_project(&project)
            .ok_or_else(|| daw_proto::DawError::not_found("Project", "context"))?;
        let target = resolve_track(&proj, &track).ok_or_else(|| track_not_found(&track))?;
        let raw = target
            .raw()
            .map_err(|e| daw_proto::DawError::invalid_object("Track", &format!("{e:?}")))?;
        item_sw::move_item_to_track(Reaper::get().medium_reaper().low(), item_ptr, raw);
        Ok(())
    }

    fn set_snap_offset(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        offset: Duration,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        item_sw::set_item_info_value(
            Reaper::get().medium_reaper(),
            item_ptr,
            ItemAttributeKey::SnapOffset,
            offset.as_seconds(),
        );
        Ok(())
    }

    fn set_muted(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        muted: bool,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        item_sw::set_item_info_value(
            Reaper::get().medium_reaper(),
            item_ptr,
            ItemAttributeKey::Mute,
            if muted { 1.0 } else { 0.0 },
        );
        Ok(())
    }

    fn set_selected(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        selected: bool,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        item_sw::set_media_item_selected(Reaper::get().medium_reaper(), item_ptr, selected);
        Ok(())
    }

    fn set_locked(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        _locked: bool,
    ) -> daw_proto::DawResult<()> {
        // Lock attribute isn't exposed via reaper_medium ItemAttributeKey;
        // chunk-manipulation fallback lives in the async surface. No-op
        // for now once the handle resolves.
        ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        Ok(())
    }

    fn select_all_items(
        &self,
        _project: ProjectContext,
        selected: bool,
    ) -> daw_proto::DawResult<()> {
        Reaper::get()
            .medium_reaper()
            .select_all_media_items(ReaperProjectContext::CurrentProject, selected);
        Ok(())
    }

    fn set_volume(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        volume: f64,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        item_sw::set_item_info_value(
            Reaper::get().medium_reaper(),
            item_ptr,
            ItemAttributeKey::Vol,
            volume,
        );
        Ok(())
    }

    fn set_fade_in(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        length: Duration,
        shape: FadeShape,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let medium = Reaper::get().medium_reaper();
        item_sw::set_item_info_value(
            medium,
            item_ptr,
            ItemAttributeKey::FadeInLen,
            length.as_seconds(),
        );
        item_sw::set_item_info_value(
            medium,
            item_ptr,
            ItemAttributeKey::FadeInShape,
            proto_fade_to_reaper(shape) as f64,
        );
        Ok(())
    }

    fn set_fade_out(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        length: Duration,
        shape: FadeShape,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let medium = Reaper::get().medium_reaper();
        item_sw::set_item_info_value(
            medium,
            item_ptr,
            ItemAttributeKey::FadeOutLen,
            length.as_seconds(),
        );
        item_sw::set_item_info_value(
            medium,
            item_ptr,
            ItemAttributeKey::FadeOutShape,
            proto_fade_to_reaper(shape) as f64,
        );
        Ok(())
    }

    fn set_loop_source(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        loop_source: bool,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        item_sw::set_item_info_value(
            Reaper::get().medium_reaper(),
            item_ptr,
            ItemAttributeKey::LoopSrc,
            if loop_source { 1.0 } else { 0.0 },
        );
        Ok(())
    }

    fn set_beat_attach_mode(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        mode: BeatAttachMode,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let timebase = match mode {
            BeatAttachMode::Time => 0.0,
            BeatAttachMode::Beats => 1.0,
            BeatAttachMode::BeatsPositionOnly => 2.0,
        };
        item_sw::set_item_info_value(
            Reaper::get().medium_reaper(),
            item_ptr,
            ItemAttributeKey::BeatAttachMode,
            timebase,
        );
        Ok(())
    }

    fn set_auto_stretch(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        auto_stretch: bool,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        item_sw::set_item_info_value(
            Reaper::get().medium_reaper(),
            item_ptr,
            ItemAttributeKey::AutoStretch,
            if auto_stretch { 1.0 } else { 0.0 },
        );
        Ok(())
    }

    fn set_color(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        color: Option<u32>,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let value = color.map(|c| (c as i32) | 0x01000000).unwrap_or(0);
        item_sw::set_item_info_value(
            Reaper::get().medium_reaper(),
            item_ptr,
            ItemAttributeKey::CustomColor,
            value as f64,
        );
        Ok(())
    }

    fn label(&self, _project: ProjectContext, item: ItemRef) -> Option<String> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)?;
        let notes = item_sw::get_item_notes(Reaper::get().medium_reaper().low(), item_ptr);
        (!notes.is_empty()).then_some(notes)
    }

    fn set_label(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        label: &str,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let text = std::ffi::CString::new(label)
            .map_err(|_| daw_proto::DawError::OperationFailed("item notes contain a NUL".into()))?;
        item_sw::set_item_notes(Reaper::get().medium_reaper().low(), item_ptr, &text);
        Ok(())
    }

    fn set_fixed_lane(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        lane: u32,
    ) -> daw_proto::DawResult<()> {
        let medium = Reaper::get().medium_reaper();
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let track =
            item_sw::get_media_item_track(medium, item_ptr).ok_or_else(|| item_not_found(&item))?;
        let lane_count = crate::lanes::read_lanes(track).lane_count;
        if lane >= lane_count {
            return Err(daw_proto::DawError::out_of_range(
                lane,
                lane_count,
                "fixed lane",
            ));
        }
        item_sw::set_item_info_value(
            medium,
            item_ptr,
            ItemAttributeKey::FixedLane,
            f64::from(lane),
        );
        medium.update_timeline();
        Ok(())
    }

    fn set_group_id(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        group_id: Option<u32>,
    ) -> daw_proto::DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let value = group_id.map(|g| g as i32).unwrap_or(0);
        item_sw::set_item_info_value(
            Reaper::get().medium_reaper(),
            item_ptr,
            ItemAttributeKey::GroupId,
            value as f64,
        );
        Ok(())
    }
}

// =============================================================================
// TakeService Implementation
// =============================================================================

/// REAPER take implementation.
#[derive(Clone)]
pub struct ReaperTake;

impl ReaperTake {
    pub fn new() -> Self {
        Self
    }

    /// Resolve a TakeRef within an item
    pub(crate) fn resolve_take(item: MediaItem, take_ref: &TakeRef) -> Option<MediaItemTake> {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();
        let low = medium.low();

        match take_ref {
            TakeRef::Guid(guid) => {
                let count = item_sw::count_takes(low, item);
                for i in 0..count {
                    if let Some(take) = item_sw::get_take(low, item, i) {
                        let take_guid = item_sw::get_take_guid_string(low, take);
                        if &take_guid == guid {
                            return Some(take);
                        }
                    }
                }
                None
            }
            TakeRef::Index(idx) => item_sw::get_take(low, item, *idx as i32),
            TakeRef::Active => item_sw::get_active_take(medium, item),
        }
    }

    /// Convert MediaItemTake to Take struct
    pub(crate) fn media_take_to_take(
        item: MediaItem,
        take: MediaItemTake,
        index: u32,
    ) -> Option<Take> {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();
        let low = medium.low();

        let guid = item_sw::get_take_guid_string(low, take);

        let item_guid = item_sw::get_item_state_chunk(low, item, 1024)
            .and_then(|chunk| extract_guid_from_chunk(&chunk))
            .unwrap_or_default();

        let active_take = item_sw::get_active_take(medium, item);
        let is_active = active_take == Some(take);

        let name = item_sw::get_take_name(low, take);

        let volume = item_sw::get_take_info_value(medium, take, TakeAttributeKey::Vol);

        let play_rate = item_sw::get_take_info_value(medium, take, TakeAttributeKey::PlayRate);

        let pitch = item_sw::get_take_pitch(medium, take);

        let preserve_pitch_raw =
            item_sw::get_take_info_value(medium, take, TakeAttributeKey::PPitch);
        let preserve_pitch = preserve_pitch_raw != 0.0;

        let start_offset = item_sw::get_take_info_value(medium, take, TakeAttributeKey::StartOffs);

        // I_CUSTOMCOLOR — same encoding as items: positive value carries
        // the color; 0 / negative means "no color set". REAPER OR-masks
        // 0x1000000 in to mark the color as live, but we surface the
        // raw u32 to the proto layer.
        let take_color_raw =
            item_sw::get_take_info_value(medium, take, TakeAttributeKey::CustomColor) as i32;
        let take_color = if take_color_raw > 0 {
            Some(take_color_raw as u32)
        } else {
            None
        };

        // Get source info. Source type comes from REAPER's
        // `GetMediaSourceType` tag string ("midi" / "wave" / "flac" /
        // "ogg" / "mp3" / "video" / "empty" / etc.). Length, sample
        // rate, and channel count would need PCM_source_GetSampleRate /
        // PCM_source_GetNumChannels wrappers — deferred to #25.
        let source_file_path = item_sw::get_take_source_file_path(medium, take);
        let source = item_sw::get_take_source(medium, take);
        let (source_type, source_length, source_sample_rate, source_channels, is_midi) =
            match source {
                Some(src) => {
                    let tag = item_sw::get_pcm_source_type(low, src).unwrap_or_default();
                    let kind = match tag.as_str() {
                        "midi" | "midipool" => SourceType::Midi,
                        "video" => SourceType::Video,
                        "" | "empty" => SourceType::Empty,
                        // Most audio formats: wave, flac, ogg, mp3, opus, wavpack, etc.
                        _ => SourceType::Audio,
                    };
                    let is_midi = matches!(kind, SourceType::Midi);
                    (kind, None, None, None, is_midi)
                }
                None => (SourceType::Empty, None, None, None, false),
            };

        // For MIDI takes, count notes via the existing safe wrapper around
        // `MIDI_CountEvts`. CC + text/sysex counts are intentionally not
        // surfaced here yet — the proto only carries `midi_note_count`.
        let midi_note_count = if is_midi {
            let counts = crate::safe_wrappers::midi::count_events(low, take);
            Some(counts.notes.max(0) as u32)
        } else {
            None
        };

        Some(Take {
            guid,
            item_guid,
            index,
            is_active,
            name,
            color: take_color,
            volume,
            play_rate,
            pitch,
            preserve_pitch,
            // I_TAKE_CHANMODE via the live API — not yet wired.
            channel_mode: 0,
            start_offset: Duration::from_seconds(start_offset),
            source_type,
            source_file_path,
            source_length,
            source_sample_rate,
            source_channels,
            is_midi,
            midi_note_count,
        })
    }
}

impl Default for ReaperTake {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Extract item GUID from a REAPER state chunk.
///
/// Uses the `IGUID` field (item GUID), not `GUID` (which is the take GUID).
fn extract_guid_from_chunk(chunk: &str) -> Option<String> {
    for line in chunk.lines() {
        if let Some(guid_part) = line.strip_prefix("IGUID ") {
            return Some(guid_part.trim().to_string());
        }
    }
    None
}

/// Convert proto FadeShape to REAPER fade shape index
fn proto_fade_to_reaper(shape: FadeShape) -> i32 {
    match shape {
        FadeShape::Linear => 0,
        FadeShape::FastStart => 1,
        FadeShape::FastEnd => 2,
        FadeShape::FastStartSteep => 3,
        FadeShape::FastEndSteep => 4,
        FadeShape::SlowStartEnd => 5,
        FadeShape::SlowStartEndSteep => 6,
    }
}

/// Convert REAPER fade shape index to proto FadeShape
fn reaper_fade_to_proto(shape: u8) -> FadeShape {
    match shape {
        0 => FadeShape::Linear,
        1 => FadeShape::FastStart,
        2 => FadeShape::FastEnd,
        3 => FadeShape::FastStartSteep,
        4 => FadeShape::FastEndSteep,
        5 => FadeShape::SlowStartEnd,
        6 => FadeShape::SlowStartEndSteep,
        _ => FadeShape::Linear,
    }
}
