//! Impulse-response loading, transformation, library and async engine.
//!
//! Mirrors the design of REEV-R (https://github.com/tiagolr/reevr):
//! load an IR from disk, apply non-destructive transforms (trim,
//! stretch, reverse, attack/decay envelope, predelay, gain), then push
//! the processed stereo buffer into the Convolution algorithm.
//!
//! Pipeline:
//!   File → [`IrAsset::load`] (decode + resample)
//!        → [`IrTransforms::apply`] (DSP shaping)
//!        → [`crate::algorithms::convolution::Convolution::load_ir_stereo`]
//!
//! [`IrEngine`] wraps a worker thread so the plugin GUI never blocks on
//! decoding. Pop processed IRs from its `Receiver` on whichever thread
//! you control IR swaps from.

pub mod asset;
pub mod engine;
pub mod library;
pub mod prepared;
pub mod transforms;

pub use asset::{IrAsset, IrLoadError};
pub use engine::{IrEngine, IrJob, IrResult};
pub use library::{IrEntry, IrLibrary};
pub use prepared::{PreparedIr, PreparedIrPair};
pub use transforms::{ChannelLayout, IrTransforms};
