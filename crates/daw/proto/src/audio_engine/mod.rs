//! Audio engine — types + service trait.

mod service;
mod types;

pub use service::*;
pub use types::{AudioEngineState, AudioInputChannel, AudioInputInfo, AudioLatency};
