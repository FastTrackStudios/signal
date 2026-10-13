//! Runs a control surface.
//!
//! - [`gesture`]: the interaction contract as pure state machines.
//! - [`visual`]: control state → ops on the design's slot nodes.
//! - [`meter`]: level ballistics and dB scales.
//! - [`plot`]: plot geometry (points → decimated paths).
//! - [`curves`]: named curve providers (`eq.response`, `comp.transfer`, …).
//! - [`coalesce`]: outgoing edit rate limiting that never loses the end.
//! - [`runtime`]: design + controls + live values, driven by the host.
//! - [`sync`]: the async bridge to any [`SurfaceEngine`](surface_proto::SurfaceEngine).

pub mod coalesce;
pub mod curves;
pub mod gesture;
pub mod meter;
pub mod plot;
pub mod runtime;
#[cfg(feature = "rpc")]
pub mod sync;
pub mod visual;
pub mod visual_ui;
pub mod viz_curves;

pub use curves::{CurveInputs, CurveRegistry, Series};
pub use gesture::{Bounds, Input, Key, Modifiers};
pub use runtime::text::{EditView, TextMeasure};
pub use runtime::{EngineCall, Runtime, Surface};
pub use visual::ControlState;
pub use visual_ui::Interaction;
