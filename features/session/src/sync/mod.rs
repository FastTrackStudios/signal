//! The live session as a Loro CRDT document.
//!
//! Everyone in a session edits the same [`SessionDoc`]. Each replica is
//! the source of truth for its own engine: local edits are read back
//! from the engine as a [`SessionModel`] and reconciled into the doc
//! ([`SessionDoc::write`]); remote edits arrive as Loro updates, and
//! [`diff`] between the model the engine last showed and the doc's new
//! one says what the engine must do. Concurrent edits merge field by
//! field, and the doc keeps the whole history.
//!
//! The schema, model and diff are engine- and transport-free. The
//! `engine` feature adds the bridge to a daw project (daw-standalone
//! in-process, or REAPER), through `daw_control`.

#[cfg(feature = "engine")]
pub mod bridge;
#[cfg(feature = "net")]
pub mod clock;
pub mod diff;
#[cfg(feature = "engine")]
pub mod direct;
pub mod doc;
#[cfg(feature = "engine")]
pub mod engine;
pub mod model;
#[cfg(feature = "net")]
pub mod net;
pub mod presence;
pub mod slug;
pub mod transport;
#[cfg(feature = "engine")]
pub mod whole;

#[cfg(feature = "engine")]
pub use bridge::Bridge;
pub use diff::{Change, ItemField, TrackField, diff};
pub use doc::{ORIGIN_LOCAL, SCHEMA, SessionDoc};
pub use loro;
pub use model::{
    AutomationItemState, CcState, ChannelPressureState, CompAreaState, CompingState,
    EnvelopePointState, EnvelopeState, FxState, GainPoint, HitState, ItemState, ItemTiming,
    LaneState, MarkerState, MasterState, MidiEvents, NoteExpressionState, NoteState,
    PitchBendState, PolyPressureState, ProgramChangeState, ProjectSettings, RegionState,
    RulerLaneState, SendState, SessionModel, Sources, StretchPoint, SysexState, TakeMarkerState,
    TakeState, TempoPoint, TrackState,
};
