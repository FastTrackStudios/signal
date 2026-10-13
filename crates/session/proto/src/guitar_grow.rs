//! Growing a guitar part — REAPER-facing action contract.
//!
//! The trait only; `session::guitar_grow::GuitarGrow<D>` is the
//! implementation. Same split as [`crate::track_manager`]: the contract
//! (and the macro-emitted `ActionMeta` consts +
//! `register_guitar_grow_actions`) is protocol, so it lives in proto
//! where any host — the REAPER extension, a CLI, a remote client — can
//! see it without pulling in session's implementation.
//!
//! The trait declares only its own identity ("Guitar Grow"). Callers
//! compose the nesting by handing `register_guitar_grow_actions` an
//! `architect::action::ScopedActionBackend`, one wrap per level.

use daw_proto::DawResult;

/// The four gestures a guitar part grows by (`flow.guitars.grow`).
///
/// A part is never restructured: each action inserts the one missing
/// folder level around what already exists — and only when that level
/// gains its second member (`flow.guitars.dimensions`) — names the new
/// tracks from the template's own dimension vocabulary so
/// `dynamic_template::track_schema::classify_track_dimension` reads them
/// straight back into the same dimensions, and leaves items and routing
/// exactly where they were.
///
/// Every method is `#[action(undo)]`: the backend brackets the handler in
/// one host undo block, so a gesture that touches seven mics on two
/// channels is still one undo step.
///
/// Acoustics are not a separate set of actions. The dimensions are the
/// same four read out of the template with the acoustic's own values —
/// Arrangement (Strum, Fingerpick), Layer, Channel (L, R), `MultiMic`
/// (DI, Neck, Body) — so growing an acoustic part runs this same code
/// (`flow.guitars.acoustics`).
#[architect::actions(namespace = "GUITAR")]
pub trait GuitarGrowActions {
    #[action(
        undo,
        description = "Double the selected guitar part: add the R channel, folding what is there into L"
    )]
    /// Add the Channel dimension's second member to the selection.
    ///
    /// L/R is the **Channel** dimension for guitars, not a Layer: a
    /// Channel shares a chain, a Layer has its own. A part that had one
    /// channel's worth of content becomes a folder over L and R, the
    /// existing tracks and their items folded into L and the R side
    /// mirroring L's sources.
    ///
    /// # Errors
    ///
    /// Returns an error if nothing is selected, the DAW is unavailable,
    /// or the template configures no further channel name.
    fn double(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Add the next guitar arrangement beside the selected one (a Chug beside the Rhythm)"
    )]
    /// Add the Arrangement dimension's next member — a Chug beside the
    /// Rhythm. *Which* one is the template's to say: the action takes the
    /// first arrangement the group configures that the part does not
    /// already carry.
    ///
    /// # Errors
    ///
    /// Returns an error if nothing is selected, the DAW is unavailable,
    /// or the template configures no further arrangement name.
    fn add_arrangement(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Add the next guitar layer beside the selected one (an octave beside the Main)"
    )]
    /// Add the Layer dimension's next member — a voice beside the Main,
    /// which for a guitar is the octave the template lists first.
    ///
    /// # Errors
    ///
    /// Returns an error if nothing is selected, the DAW is unavailable,
    /// or the template configures no further layer name.
    fn add_layer(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Add the next source to every channel of the selected guitar part"
    )]
    /// Add the `MultiMic` dimension's next member to **every** channel of
    /// the selected part — a second amp mic on both sides of a double,
    /// not on one of them.
    ///
    /// The new tracks start at the configuration's default balance
    /// (`flow.guitars.mixing.source-defaults`), applied here and never
    /// re-applied to anything an engineer has already balanced.
    ///
    /// # Errors
    ///
    /// Returns an error if nothing is selected, the DAW is unavailable,
    /// or the template configures no further source name.
    fn add_source(&self) -> DawResult<()>;
}

#[cfg(test)]
mod tests {
    /// The command ids REAPER lists, a keybinding binds and a toolbar
    /// button fires. They are derived from the method names, so a rename
    /// silently breaks every binding on the old one — this is the test
    /// that makes that a failure instead.
    ///
    /// Bare here, without the `FTS_SESSION` prefix production registers
    /// them under: the trait names only itself, and the scope is composed
    /// by `session::register_all_actions`.
    #[test]
    fn command_ids_are_stable() {
        let ids: Vec<&str> = super::GuitarGrowActionsActions::all()
            .iter()
            .map(|action| action.id)
            .collect();
        assert_eq!(
            ids,
            vec![
                "GUITAR_DOUBLE",
                "GUITAR_ADD_ARRANGEMENT",
                "GUITAR_ADD_LAYER",
                "GUITAR_ADD_SOURCE",
            ]
        );
    }
}
