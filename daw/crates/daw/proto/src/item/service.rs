//! Items service — architect::rpc port.
//!
//! Stateless singleton backends. `ProjectContext` flows through every
//! call; `TrackRef` / `ItemRef` identify the target. The legacy
//! async `ItemService` retired with this port — see commit log for
//! the dropped surfaces (subscribe_items is sibling-trait territory).

use super::{FadeShape, Item, ItemRef};
use crate::batch::{ProjectArg, TrackArg};
use crate::primitives::{BeatAttachMode, Duration, PositionInSeconds};
use crate::{DawResult, ProjectContext, TrackRef};
use facet::Facet;

/// Where an item sits on its track: the `position`/`length` pair
/// [`Items::add_item`] takes, bundled for [`Items::add_item_with_guid`]
/// (the RPC surface allows four parameters after `self`).
#[derive(Clone, Copy, Debug, PartialEq, Facet)]
pub struct ItemSpan {
    pub position: PositionInSeconds,
    pub length: Duration,
}

impl ItemSpan {
    pub fn new(position: PositionInSeconds, length: Duration) -> Self {
        Self { position, length }
    }
}

#[architect::rpc(ops(ProjectContext as ProjectArg, TrackRef as TrackArg))]
pub trait Items {
    // ── Queries ──────────────────────────────────────────────────────

    fn get_items(&self, project: ProjectContext, track: TrackRef) -> Vec<Item>;
    fn get_item(&self, project: ProjectContext, item: ItemRef) -> Option<Item>;
    fn get_all_items(&self, project: ProjectContext) -> Vec<Item>;
    fn get_selected_items(&self, project: ProjectContext) -> Vec<Item>;
    fn item_count(&self, project: ProjectContext, track: TrackRef) -> u32;

    // ── CRUD ─────────────────────────────────────────────────────────

    /// Create a new (MIDI) item at the given position/length. Returns
    /// the new item's GUID.
    fn add_item(
        &self,
        project: ProjectContext,
        track: TrackRef,
        position: PositionInSeconds,
        length: Duration,
    ) -> Option<String>;

    /// [`Items::add_item`], but the item takes `guid` instead of a fresh
    /// one — how a peer re-creates an item another engine made, so both
    /// key it the same way. Makes the same kind of item `add_item` does
    /// (no source: give it one with `Takes::set_source_file`). Returns
    /// the guid as the backend stores it (the standalone engine keeps
    /// `guid` verbatim; REAPER normalises it to its own `{UPPERCASE}`
    /// spelling, `guid` must then parse as a UUID).
    ///
    /// A `guid` an item of the project already has is
    /// [`DawError::AlreadyExists`](crate::DawError::AlreadyExists) —
    /// never a second item with the same guid.
    fn add_item_with_guid(
        &self,
        project: ProjectContext,
        track: TrackRef,
        guid: &str,
        span: ItemSpan,
    ) -> DawResult<String>;

    fn delete_item(&self, project: ProjectContext, item: ItemRef) -> DawResult<()>;

    /// Duplicate an item. Returns the new item's GUID (or a pointer
    /// string fallback when REAPER doesn't expose a stable handle).
    fn duplicate_item(&self, project: ProjectContext, item: ItemRef) -> Option<String>;

    // ── Position & Length ────────────────────────────────────────────

    fn set_position(
        &self,
        project: ProjectContext,
        item: ItemRef,
        position: PositionInSeconds,
    ) -> DawResult<()>;
    fn set_length(&self, project: ProjectContext, item: ItemRef, length: Duration)
    -> DawResult<()>;
    fn move_to_track(
        &self,
        project: ProjectContext,
        item: ItemRef,
        track: TrackRef,
    ) -> DawResult<()>;
    fn set_snap_offset(
        &self,
        project: ProjectContext,
        item: ItemRef,
        offset: Duration,
    ) -> DawResult<()>;

    // ── State ────────────────────────────────────────────────────────

    fn set_muted(&self, project: ProjectContext, item: ItemRef, muted: bool) -> DawResult<()>;
    fn set_selected(&self, project: ProjectContext, item: ItemRef, selected: bool)
    -> DawResult<()>;
    fn set_locked(&self, project: ProjectContext, item: ItemRef, locked: bool) -> DawResult<()>;
    fn select_all_items(&self, project: ProjectContext, selected: bool) -> DawResult<()>;

    // ── Audio Properties ─────────────────────────────────────────────

    fn set_volume(&self, project: ProjectContext, item: ItemRef, volume: f64) -> DawResult<()>;
    fn set_fade_in(
        &self,
        project: ProjectContext,
        item: ItemRef,
        length: Duration,
        shape: FadeShape,
    ) -> DawResult<()>;
    fn set_fade_out(
        &self,
        project: ProjectContext,
        item: ItemRef,
        length: Duration,
        shape: FadeShape,
    ) -> DawResult<()>;

    // ── Timing Behavior ──────────────────────────────────────────────

    fn set_loop_source(
        &self,
        project: ProjectContext,
        item: ItemRef,
        loop_source: bool,
    ) -> DawResult<()>;
    fn set_beat_attach_mode(
        &self,
        project: ProjectContext,
        item: ItemRef,
        mode: BeatAttachMode,
    ) -> DawResult<()>;
    fn set_auto_stretch(
        &self,
        project: ProjectContext,
        item: ItemRef,
        auto_stretch: bool,
    ) -> DawResult<()>;

    // ── Visual Properties ────────────────────────────────────────────

    fn set_color(
        &self,
        project: ProjectContext,
        item: ItemRef,
        color: Option<u32>,
    ) -> DawResult<()>;
    /// The item's on-screen label — REAPER's `P_NOTES`.
    ///
    /// The natural home for what an item *means* (a chord symbol, a key
    /// change): visible in the arrange view, editable by hand, saved with
    /// the project, and it moves when the item moves. Reads back, so it
    /// needs no side-car store.
    ///
    /// Called `label` rather than `notes` deliberately: REAPER's field is
    /// `P_NOTES`, but "notes" on a DAW trait already means pitches, and
    /// `Midi::notes` is a method away. The two are unrelated and must not
    /// read alike.
    fn label(&self, project: ProjectContext, item: ItemRef) -> Option<String>;

    /// Set the item's label. See [`Items::label`].
    fn set_label(&self, project: ProjectContext, item: ItemRef, label: &str) -> DawResult<()>;

    fn set_group_id(
        &self,
        project: ProjectContext,
        item: ItemRef,
        group_id: Option<u32>,
    ) -> DawResult<()>;

    /// Move the item onto fixed lane `lane` of its track (REAPER
    /// `I_FIXEDLANE`). The track must have that many lanes.
    fn set_fixed_lane(&self, project: ProjectContext, item: ItemRef, lane: u32) -> DawResult<()>;
}
