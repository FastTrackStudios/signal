//! Live MIDI — types + events + service trait.
//!
//! Send-batch + subscribe-input streaming retire with the
//! architect::rpc port; sibling-trait territory.

mod device;
mod error;
mod event;
mod message;
mod service;

pub use device::*;
pub use error::*;
pub use event::*;
pub use message::*;
pub use service::*;
