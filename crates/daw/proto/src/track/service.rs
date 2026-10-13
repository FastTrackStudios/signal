//! Tracks service trait.
//!
//! Stateless singleton backends — `ProjectContext` flows through every
//! call. `#[architect::rpc]` derives the async vox client + `serve`
//! function; backends impl `Tracks` directly. See
//! `daw-proto/src/marker/service.rs` for the same pattern.
//!
//! Scope was trimmed from the previous async `TrackService` to the
//! verbs the daw-control facade actually drives. Broader surfaces
//! (track chunks, hierarchy apply, ext state, input monitoring,
//! subscribe) land on follow-on sibling traits if/when a real consumer
//! needs them.

use super::event::TrackStreamEvent;
use super::{Comp, CompArea, LaneComping, RecordInput, ReorderTracksBehavior, Track, TrackRef};
use crate::batch::{ProjectArg, TrackArg};
use crate::{DawResult, ProjectContext};
use facet::Facet;

/// Track-scoped ext state payload — groups section + key + value into
/// a single Facet struct. Kept here so batch op definitions can name
/// it without dragging in the full ext-state surface.
#[derive(Clone, Debug, Facet)]
pub struct TrackExtStateRequest {
    pub section: String,
    pub key: String,
    pub value: String,
}

/// One ext-state key a track carries, and its value: what a template or
/// an extension wrote on the track (`FTS:kind` → `piece`), REAPER's
/// `P_EXT`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Facet)]
pub struct TrackExtEntry {
    pub key: String,
    pub value: String,
}

#[architect::rpc(ops(ProjectContext as ProjectArg, TrackRef as TrackArg), scopes(project: ProjectContext, track: TrackRef))]
pub trait Tracks {
    // ── Queries ─────────────────────────────────────────────────────

    /// Every track in the project, in mixer order.
    fn all(&self, project: ProjectContext) -> Vec<Track>;

    /// One track by reference (guid or index), if it still exists.
    fn get(&self, project: ProjectContext, track: TrackRef) -> Option<Track>;

    /// Total number of tracks (master excluded).
    fn count(&self, project: ProjectContext) -> u32;

    /// All currently selected tracks.
    fn selected(&self, project: ProjectContext) -> Vec<Track>;

    /// The master track.
    fn master(&self, project: ProjectContext) -> Option<Track>;

    // ── Mute / solo / arm ───────────────────────────────────────────

    fn set_muted(&self, project: ProjectContext, track: TrackRef, muted: bool) -> DawResult<()>;

    fn set_soloed(&self, project: ProjectContext, track: TrackRef, soloed: bool) -> DawResult<()>;

    fn set_solo_exclusive(&self, project: ProjectContext, track: TrackRef) -> DawResult<()>;

    fn clear_all_solo(&self, project: ProjectContext) -> DawResult<()>;

    fn set_armed(&self, project: ProjectContext, track: TrackRef, armed: bool) -> DawResult<()>;

    // ── Volume / pan ────────────────────────────────────────────────

    fn set_volume(&self, project: ProjectContext, track: TrackRef, volume: f64) -> DawResult<()>;

    fn set_pan(&self, project: ProjectContext, track: TrackRef, pan: f64) -> DawResult<()>;

    /// Set polarity/phase inversion (flip the signal's sign).
    /// Set the track automation mode (trim/read/touch/write/latch).
    fn set_automation_mode(
        &self,
        project: ProjectContext,
        track: TrackRef,
        mode: crate::primitives::AutomationMode,
    ) -> DawResult<()>;

    /// Set record-input monitoring (off / on / tape-auto).
    fn set_input_monitor(
        &self,
        project: ProjectContext,
        track: TrackRef,
        monitor: super::InputMonitoringMode,
    ) -> DawResult<()>;

    fn set_phase_inverted(
        &self,
        project: ProjectContext,
        track: TrackRef,
        inverted: bool,
    ) -> DawResult<()>;

    // ── Track groups ────────────────────────────────────────────────
    //
    // The DAW's fixed set of track-group slots (REAPER 7: 128, addressed
    // 1-based, `GROUP_SLOTS`). Each slot has ten lead/follow flag
    // families (`GroupFamily`) and five modifiers (`GroupModifier`);
    // a track's part in every slot is its `TrackGrouping`.

    /// Set the display name of track-group `slot` (1-based). Reads back
    /// through `Projects::get_project_info_string("TRACK_GROUP_NAME:<slot>")`.
    fn set_group_name(&self, project: ProjectContext, slot: u32, name: &str) -> DawResult<()>;

    /// First slot in `[band_start, band_end]` (inclusive, 1-based) that no
    /// track is a member of through any family or modifier, or `None` if
    /// the band is full.
    fn first_free_group_slot(
        &self,
        project: ProjectContext,
        band_start: u32,
        band_end: u32,
    ) -> Option<u32>;

    /// Add or remove `track` from track-group `slot` as a *mutual* member —
    /// every flag family, both lead and follow — so any member's
    /// mute/solo/volume/etc. moves the whole group equally.
    fn set_group_membership(
        &self,
        project: ProjectContext,
        track: TrackRef,
        slot: u32,
        member: bool,
    ) -> DawResult<()>;

    /// Set `track`'s role in one family of one slot. Lead and follow are
    /// exclusive per family and slot; `GroupRole::None` leaves the
    /// family. Errors on a slot outside `1..=GROUP_SLOTS`.
    fn set_group_flags(
        &self,
        project: ProjectContext,
        track: TrackRef,
        change: super::GroupFlagChange,
    ) -> DawResult<()>;

    /// Switch one modifier of one slot on or off for `track`.
    fn set_group_modifier(
        &self,
        project: ProjectContext,
        track: TrackRef,
        change: super::GroupModifierChange,
    ) -> DawResult<()>;

    /// `track`'s live group membership, every family and modifier over
    /// all slots. On REAPER this is the only read of the live matrix —
    /// `Track::grouping` from `all`/`get` there is not populated.
    fn group_flags(
        &self,
        project: ProjectContext,
        track: TrackRef,
    ) -> DawResult<super::TrackGrouping>;

    // ── Selection ───────────────────────────────────────────────────

    fn set_selected(
        &self,
        project: ProjectContext,
        track: TrackRef,
        selected: bool,
    ) -> DawResult<()>;

    fn select_exclusive(&self, project: ProjectContext, track: TrackRef) -> DawResult<()>;

    fn clear_selection(&self, project: ProjectContext) -> DawResult<()>;

    // ── Bulk mute ───────────────────────────────────────────────────

    fn mute_all(&self, project: ProjectContext) -> DawResult<()>;

    fn unmute_all(&self, project: ProjectContext) -> DawResult<()>;

    // ── Mutation ────────────────────────────────────────────────────

    /// Insert a track. `at_index = None` appends at the end. Returns
    /// the new track's guid.
    fn add(&self, project: ProjectContext, name: &str, at_index: Option<u32>) -> DawResult<String>;

    /// [`Tracks::add`], but the track takes `guid` instead of a fresh
    /// one — how a peer re-creates a track another engine made, so both
    /// key it the same way. Returns the guid as the backend stores it
    /// (the standalone engine keeps `guid` verbatim; REAPER normalises
    /// it to its own spelling, `guid` must then parse as a UUID).
    ///
    /// A `guid` a track of the project already has is
    /// [`DawError::AlreadyExists`](crate::DawError::AlreadyExists) —
    /// never a second track with the same guid.
    fn add_with_guid(
        &self,
        project: ProjectContext,
        guid: &str,
        name: &str,
        at_index: Option<u32>,
    ) -> DawResult<String>;

    fn remove(&self, project: ProjectContext, track: TrackRef) -> DawResult<()>;

    fn remove_all(&self, project: ProjectContext) -> DawResult<()>;

    fn rename(&self, project: ProjectContext, track: TrackRef, name: &str) -> DawResult<()>;

    fn set_color(&self, project: ProjectContext, track: TrackRef, color: u32) -> DawResult<()>;

    /// Set REAPER folder-depth change for a track.
    fn set_folder_depth(
        &self,
        project: ProjectContext,
        track: TrackRef,
        folder_depth: i32,
    ) -> DawResult<()>;

    /// Set the track channel count.
    fn set_num_channels(
        &self,
        project: ProjectContext,
        track: TrackRef,
        num_channels: u32,
    ) -> DawResult<()>;

    /// Set the track record input source.
    fn set_record_input(
        &self,
        project: ProjectContext,
        track: TrackRef,
        input: RecordInput,
    ) -> DawResult<()>;

    /// Move one track so it ends up at `index` (0-based, in project
    /// order, master excluded), leaving the selection alone.
    ///
    /// Only the order changes: the track keeps its own folder depth,
    /// and nothing re-parents it on purpose — set depths afterwards
    /// with [`Tracks::set_folder_depth`] if the move should change
    /// folder membership. (A depth left as it was can still put the
    /// track, or the ones after it, in a different folder at the new
    /// position; that is what the depths say, not something `move_to`
    /// decides.) An `index` past the last track is
    /// [`DawError::OutOfRange`](crate::DawError::OutOfRange).
    fn move_to(&self, project: ProjectContext, track: TrackRef, index: u32) -> DawResult<()>;

    /// Move all currently selected tracks to `index`.
    fn reorder_selected(
        &self,
        project: ProjectContext,
        index: u32,
        behavior: ReorderTracksBehavior,
    ) -> DawResult<()>;

    /// Set TCP/MCP visibility for a track.
    fn set_visibility(
        &self,
        project: ProjectContext,
        track: TrackRef,
        visible_in_tcp: bool,
        visible_in_mixer: bool,
    ) -> DawResult<()>;

    /// Set the TCP height override for a track. `height_pixels = 0` clears the
    /// override and lets REAPER choose the default height.
    fn set_tcp_height(
        &self,
        project: ProjectContext,
        track: TrackRef,
        height_pixels: u32,
    ) -> DawResult<()>;

    // ── Fixed lanes ─────────────────────────────────────────────────
    //
    // The lanes themselves ride the bulk `Track` read (`lane_count`,
    // `lane_play_mask`, `lane_names`, `lane_display`); these are their
    // writers. REAPER: `I_NUMFIXEDLANES`, `C_LANEPLAYS:N`, `P_LANENAME:n`.

    /// Set the number of fixed item lanes. `0` switches lanes off, which
    /// drops the lane names, the play mask and the comping state with
    /// them. Growing from `0` makes lane 0 the playing lane, as REAPER
    /// does when lanes are first enabled.
    fn set_lane_count(&self, project: ProjectContext, track: TrackRef, count: u32)
    -> DawResult<()>;

    /// Set which lanes play: bit n = lane n audible. Bits past
    /// `lane_count` are ignored.
    fn set_lane_play_mask(
        &self,
        project: ProjectContext,
        track: TrackRef,
        mask: u64,
    ) -> DawResult<()>;

    /// Name lane `lane`. This is also how a comp is renamed — a comp's
    /// name is its lane's name (see [`Comp`]).
    fn set_lane_name(
        &self,
        project: ProjectContext,
        track: TrackRef,
        lane: u32,
        name: &str,
    ) -> DawResult<()>;

    // ── Comping ─────────────────────────────────────────────────────
    //
    // A getter of its own rather than `Track` fields: on REAPER this is
    // a state-chunk read (`LANEREC`, `ITEMLANES`, `LINKEDLANE` have no
    // SDK accessor), which a bulk track read must never pay.

    /// The track's comping state: record / comping lanes and comp areas.
    fn comping(&self, project: ProjectContext, track: TrackRef) -> DawResult<LaneComping>;

    /// Replace the track's comp areas. Every lane an area names must
    /// exist.
    fn set_comp_areas(
        &self,
        project: ProjectContext,
        track: TrackRef,
        areas: Vec<CompArea>,
    ) -> DawResult<()>;

    /// The track's named comps, ascending by lane — see
    /// [`LaneComping::comps`].
    fn comps(&self, project: ProjectContext, track: TrackRef) -> DawResult<Vec<Comp>>;

    /// Add a lane named `name`, make it the comping lane, and return its
    /// index. The new comp starts with no areas.
    fn create_comp(&self, project: ProjectContext, track: TrackRef, name: &str) -> DawResult<u32>;

    /// Make `lane` the comping lane (`None` = no comp active). The lane
    /// that was active becomes the previous one.
    fn set_active_comp(
        &self,
        project: ProjectContext,
        track: TrackRef,
        lane: Option<u32>,
    ) -> DawResult<()>;

    // ── Ext state ───────────────────────────────────────────────────

    /// Every ext-state key `track` carries, with its value, by key.
    fn ext_state(&self, project: ProjectContext, track: TrackRef) -> Vec<TrackExtEntry>;

    /// Set `key` on `track` to `value`; an empty value removes it.
    fn set_ext_state(
        &self,
        project: ProjectContext,
        track: TrackRef,
        key: &str,
        value: &str,
    ) -> DawResult<()>;

    // ── Streaming ───────────────────────────────────────────────────

    /// Track add/remove/modify events across all open projects, as
    /// they happen. Served from the backend's `TracksStreamSource`
    /// hub; subscribers filter by `project_guid` on the envelope.
    #[subscribe]
    fn events(&self) -> TrackStreamEvent;
}
