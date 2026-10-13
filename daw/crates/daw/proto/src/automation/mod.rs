//! Automation — envelope + point types + service trait.

mod envelope;
mod error;
mod event;
mod play;
mod service;

pub use envelope::*;
pub use error::*;
pub use event::*;
pub use play::{item_value_at, played, value_at};
pub use service::*;
