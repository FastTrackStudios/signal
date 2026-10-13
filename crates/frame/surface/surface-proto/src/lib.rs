//! The `surface` contract: how a control surface talks to an engine.
//!
//! - [`curve`]: the one response-curve type (from `feed-proto`).
//! - [`param`]: what an engine exposes ([`Catalog`]: params, actions,
//!   feeds, collections) and how values map to engineering units.
//! - feeds ([`FeedKind`], [`FeedInfo`], [`FeedRequest`], [`FeedFrame`]):
//!   `feed-proto`'s typed telemetry, re-exported. `StreamKind`,
//!   `StreamInfo` and `StreamFrame` are the same types under their old names.
//! - [`control`]: what makes a design node a control ([`Control`]:
//!   kind, [`Binding`], MIDI mapping, slots) and validation against a catalog.
//! - [`engine::SurfaceEngine`]: the service any engine implements (signal rigs, a
//!   plugin, the mock engine). Surfaces only ever talk to an engine through
//!   it — in-process, over a WebSocket, or from an iPad.

pub mod control;
pub mod interact;
pub mod param;

pub use feed_proto::curve;
pub use feed_proto::{FeedFrame, FeedOptions, FeedRequest, Tap, Trigger, VoiceState};

pub use control::*;
pub use interact::*;
pub use curve::{Curve, CurvePoint};
pub use param::*;

/// One parameter's current value (normalized 0..1).
#[derive(Debug, Clone, PartialEq, facet::Facet, serde::Serialize, serde::Deserialize)]
pub struct ParamValue {
    pub addr: ParamAddr,
    pub value: f64,
}

/// One text parameter's current value (a band's name).
#[derive(Debug, Clone, PartialEq, facet::Facet, serde::Serialize, serde::Deserialize)]
pub struct TextValue {
    pub addr: ParamAddr,
    pub text: String,
}

/// One sample of a telemetry stream — a [`FeedFrame`] (`values` laid out
/// by the feed's [`FeedKind`]). The old name, kept for existing code.
pub type StreamFrame = FeedFrame;

/// Where a parameter edit is in its gesture, so engines can group undo and
/// automation writes (begin → changes → end).
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum EditPhase {
    Begin,
    Change,
    End,
}

/// Why an engine refused a call.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum SurfaceError {
    UnknownParam(String),
    UnknownAction(String),
    UnknownCollection(String),
    OutOfRange(u32),
    UnknownFeed(String),
    /// A text was set on a param that isn't a text param (or the engine
    /// keeps no text).
    NotText(String),
}

impl std::fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownParam(a) => write!(f, "no parameter `{a}`"),
            Self::UnknownAction(a) => write!(f, "no action `{a}`"),
            Self::UnknownCollection(c) => write!(f, "no collection `{c}`"),
            Self::OutOfRange(i) => write!(f, "index {i} out of range"),
            Self::UnknownFeed(id) => write!(f, "no feed `{id}`"),
            Self::NotText(a) => write!(f, "`{a}` is not a text parameter"),
        }
    }
}

impl std::error::Error for SurfaceError {}

#[cfg(feature = "rpc")]
pub mod engine {
    //! The engine service. `SurfaceEngine` → `SurfaceEngineClient` /
    //! `SurfaceEngineService` / `surface_engine_serve`, plus the stream
    //! sibling (`SurfaceEngineStreamClient`, `SurfaceEngineStreamService`,
    //! the `SurfaceEngineStreamSource` backend contract).
    use super::{Catalog, EditPhase, EntryId, FeedInfo, FeedRequest, ParamAddr, ParamValue, StreamFrame, SurfaceError, TextValue};

    /// An engine, as a surface sees it. Backends implement it synchronously
    /// (like signal's rig services); remote callers get the async client.
    #[architect::rpc]
    pub trait SurfaceEngine {
        /// What this engine exposes.
        fn catalog(&self) -> Catalog;
        /// Every parameter's current value.
        fn values(&self) -> Vec<ParamValue>;
        /// Set a parameter (normalized). `phase` brackets a gesture.
        fn set_param(&self, addr: ParamAddr, value: f64, phase: EditPhase) -> Result<(), SurfaceError>;
        /// Fire an action.
        fn invoke(&self, action: EntryId) -> Result<(), SurfaceError>;
        /// Select an item of a collection.
        fn select(&self, collection: EntryId, index: u32) -> Result<(), SurfaceError>;
        /// Ask for feeds shaped by options (spectrum bins, history length,
        /// rate, tap, smoothing). The engine answers with each feed as it
        /// will now publish it on `telemetry` ([`FeedInfo::resolve`]).
        /// Options are engine-wide: the latest request for a feed wins.
        ///
        /// The default ignores the options and answers with the catalog's
        /// entries, so engines written before feeds keep compiling.
        fn request_feeds(&self, requests: Vec<FeedRequest>) -> Result<Vec<FeedInfo>, SurfaceError> {
            let catalog = self.catalog();
            requests
                .iter()
                .map(|r| catalog.stream(&r.id).cloned().ok_or_else(|| SurfaceError::UnknownFeed(r.id.0.clone())))
                .collect()
        }
        /// Every text parameter's current value. The default: none (an
        /// engine written before text params).
        fn texts(&self) -> Vec<TextValue> {
            Vec::new()
        }
        /// Set a text parameter (a band's name). The default refuses, so
        /// engines written before text params keep compiling.
        fn set_text(&self, addr: ParamAddr, text: String) -> Result<(), SurfaceError> {
            let _ = text;
            Err(SurfaceError::NotText(addr.0))
        }
        /// Parameter changes from anywhere (other surfaces, MIDI, automation).
        #[subscribe]
        fn param_changes(&self) -> ParamValue;
        /// Feed frames (meters, spectrum, traces, scopes, envelopes, tuner)
        /// at each feed's rate, shaped as last requested.
        #[subscribe]
        fn telemetry(&self) -> StreamFrame;
    }
}

#[cfg(feature = "rpc")]
pub use engine::*;
