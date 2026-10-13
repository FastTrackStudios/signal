//! The `feed` contract: typed data a surface can request from an engine.
//!
//! A **feed** is a telemetry stream with a declared shape ([`FeedKind`]).
//! An engine lists its feeds in its catalog ([`FeedInfo`]); a surface asks
//! for them with options ([`FeedRequest`]: bins, history length, rate,
//! tap, smoothing) and receives [`FeedFrame`]s whose `values` follow the
//! kind's payload layout ([`FeedKind::frame_len`]).
//!
//! `surface-proto`'s `StreamKind` / `StreamInfo` / `StreamFrame` are these
//! types under their old names, so catalogs and surfaces written before
//! feeds existed keep working (same JSON).
//!
//! - [`range`]: engineering ranges and units ([`Range`], [`Unit`]) — shared
//!   with parameters, and what a [`FeedKind::Scalar`] reads in.
//! - [`curve`]: the one response-curve type ([`Curve`]).

pub mod curve;
pub mod feed;
pub mod range;

pub use curve::{Curve, CurvePoint};
pub use feed::*;
pub use range::*;

use facet::Facet;
use serde::{Deserialize, Serialize};

/// Identity of an action, stream (feed) or collection in a catalog.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Facet)]
#[facet(transparent)]
#[serde(transparent)]
pub struct EntryId(pub String);

impl EntryId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for EntryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
