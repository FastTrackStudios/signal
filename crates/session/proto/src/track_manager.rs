//! Track Manager — REAPER-facing action contract.
//!
//! The trait only; `session::track_manager::TrackManager<D>` is
//! the implementation. Same split as the `#[architect::rpc]` services in
//! this crate: the contract (and the macro-emitted `ActionMeta` consts +
//! `register_track_manager_actions`) is protocol, so it lives in proto
//! where any host — the REAPER extension, a CLI, a remote client — can
//! see it without pulling in session's implementation.
//!
//! The trait declares only its own identity ("Track Manager"). It knows
//! nothing about being nested under Session or FTS: callers compose that
//! by handing `register_track_manager_actions` an
//! `architect::action::ScopedActionBackend`, one wrap per level.

use daw_proto::DawResult;

/// Builds dynamic-template track structure — channels, multi-mics,
/// layers, performers, arrangements — relative to the current selection.
///
/// `#[action(undo)]` marks the mutating actions: the backend brackets
/// those in a host undo block labelled after the action, so each is one
/// atomic undo point and no implementation does begin/end bookkeeping.
/// Every method returns [`DawResult<()>`]; a failure reaches the user
/// through the backend (a REAPER message box) rather than being silently
/// logged.
#[architect::actions(namespace = "TRACK_MANAGER")]
pub trait TrackManagerActions {
    #[action(
        undo,
        description = "Add the next dynamic-template channel to the selected track scope"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the track
    /// structure cannot be built.
    fn add_channel(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Add the next dynamic-template layer to the selected track scope"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the track
    /// structure cannot be built.
    fn add_layer(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Add the next dynamic-template multi-mic track to the selected track scope"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the track
    /// structure cannot be built.
    fn add_multi_mic(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Add a performer folder to the selected track scope"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the track
    /// structure cannot be built.
    fn add_performer(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Add the next dynamic-template arrangement to the selected instrument scope"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the track
    /// structure cannot be built.
    fn add_arrangement(&self) -> DawResult<()>;

    #[action(
        description = "Reorganize selected tracks with performer as the top metadata dimension"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the track
    /// structure cannot be built.
    fn reorganize_selected_by_performer(&self) -> DawResult<()>;

    #[action(
        description = "Reorganize selected tracks with arrangement as the top metadata dimension"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the track
    /// structure cannot be built.
    fn reorganize_selected_by_arrangement(&self) -> DawResult<()>;
}
